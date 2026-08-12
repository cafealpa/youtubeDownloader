import os
import shutil
import sys
from importlib.util import find_spec
import yt_dlp
from PyQt6.QtCore import QThread, pyqtSignal
from yt_dlp.version import __version__ as YT_DLP_VERSION


SUPPORTED_JS_RUNTIMES = {
    'deno': ('deno',),
    'node': ('node',),
    'bun': ('bun',),
    'quickjs': ('qjs', 'quickjs'),
}


def get_base_dir():
    """번들 리소스(ffmpeg, deno 등)가 위치한 디렉터리를 반환합니다."""
    if getattr(sys, 'frozen', False):
        return sys._MEIPASS
    return os.path.dirname(os.path.abspath(__file__))


def detect_js_runtimes():
    runtimes = {}

    # 앱에 내장된 deno.exe를 최우선으로 사용 (별도 설치 불필요)
    bundled_deno = os.path.join(get_base_dir(), 'deno.exe')
    if os.path.isfile(bundled_deno):
        runtimes['deno'] = {'path': bundled_deno}

    for runtime_name, candidates in SUPPORTED_JS_RUNTIMES.items():
        if runtime_name in runtimes:
            continue
        for candidate in candidates:
            runtime_path = shutil.which(candidate)
            if runtime_path:
                runtimes[runtime_name] = {'path': runtime_path}
                break
    return runtimes


def has_yt_dlp_ejs():
    return find_spec('yt_dlp_ejs') is not None

class Worker(QThread):
    finished = pyqtSignal(dict)
    error = pyqtSignal(str)
    progress = pyqtSignal(int)

    def __init__(self, url, mode='info', format_str=None, quality_label='best'):
        """
        워커 스레드를 초기화합니다.
        :param url: 유튜브 동영상 URL
        :param mode: 'info' (정보 조회) 또는 'download' (다운로드)
        :param format_str: 다운로드 포맷 문자열
        :param quality_label: 파일명에 사용될 화질 라벨 (예: '1080p', 'audio')
        """
        super().__init__()
        self.url = url
        self.mode = mode
        self.format_str = format_str
        self.quality_label = quality_label
        self.is_cancelled = False

    def cancel(self):
        """다운로드 작업을 취소합니다."""
        self.is_cancelled = True

    def run(self):
        """
        스레드 실행 메인 함수입니다.
        FFmpeg 경로를 설정하고 yt-dlp를 사용하여 정보 조회 또는 다운로드를 수행합니다.
        """
        try:
            ffmpeg_path = os.path.join(get_base_dir(), 'ffmpeg.exe')
            js_runtimes = detect_js_runtimes()
            ejs_installed = has_yt_dlp_ejs()
            
            print(f"DEBUG: Mode={self.mode}, Format={self.format_str}")
            print(f"DEBUG: FFmpeg Path={ffmpeg_path}")
            print(f"DEBUG: JS Runtimes={js_runtimes}")
            print(f"DEBUG: yt-dlp Version={YT_DLP_VERSION}")
            print(f"DEBUG: yt-dlp-ejs Installed={ejs_installed}")

            ydl_opts = {
                'quiet': True,
                'no_warnings': True,
                'ffmpeg_location': ffmpeg_path,
                'js_runtimes': js_runtimes,
                'remote_components': ['ejs:github'],
            }
            if self.mode == 'download':
                # Get Downloads folder
                download_path = os.path.join(os.path.expanduser("~"), "Downloads")
                
                # Base Options with custom filename
                # Use standard string formatting for the label, let yt-dlp handle title and ext
                filename_tmpl = f'%(title)s_{self.quality_label}.%(ext)s'
                
                opts = {
                    'outtmpl': os.path.join(download_path, filename_tmpl),
                    'progress_hooks': [self.progress_hook],
                }

                if self.format_str == 'audio':
                    # Audio Only (MP3)
                    opts.update({
                        'format': 'bestaudio/best',
                        'postprocessors': [{
                            'key': 'FFmpegExtractAudio',
                            'preferredcodec': 'mp3',
                            'preferredquality': '192',
                        }],
                    })
                else:
                    # Video (MP4)
                    opts.update({
                        'format': self.format_str if self.format_str else 'best',
                        # Force MP4 Output
                        'merge_output_format': 'mp4',
                        'postprocessors': [{
                            'key': 'FFmpegVideoConvertor',
                            'preferedformat': 'mp4',
                        }],
                    })
                
                ydl_opts.update(opts)

            with yt_dlp.YoutubeDL(ydl_opts) as ydl:
                if self.mode == 'info':
                    if self.is_cancelled: return
                    info = ydl.extract_info(self.url, download=False)
                    
                    # Extract unique resolutions
                    formats = info.get('formats', [])
                    resolutions = set()
                    for f in formats:
                        if f.get('vcodec') != 'none' and f.get('height'):
                            resolutions.add(f['height'])
                    
                    info['resolutions'] = sorted(list(resolutions), reverse=True)
                    self.finished.emit(info)
                elif self.mode == 'download':
                    ydl.download([self.url])
                    if not self.is_cancelled:
                         self.finished.emit({'status': 'downloaded'})

        except Exception as e:
            if self.is_cancelled:
                print("Download cancelled by user.")
                self.finished.emit({'status': 'cancelled'})
            else:
                self.error.emit(self._format_error_message(str(e)))

    def _format_error_message(self, message):
        lower_message = message.lower()
        if 'http error 403' in lower_message or 'unable to download video data' in lower_message:
            available = ', '.join(detect_js_runtimes().keys()) or '없음'
            return (
                "YouTube에서 403 오류가 발생했습니다.\n\n"
                f"- 현재 감지된 JS 런타임: {available}\n"
                f"- 현재 yt-dlp 버전: {YT_DLP_VERSION}\n\n"
                "최근 YouTube는 yt-dlp 최신 버전만으로는 부족하고, "
                "yt-dlp-ejs 및 JavaScript 런타임 구성이 필요할 수 있습니다.\n"
                "1. `pip install -U \"yt-dlp[default]\"`로 업데이트\n"
                "2. Node.js 또는 Deno 설치\n"
                "3. 앱을 다시 실행해 보세요.\n\n"
                f"원본 오류: {message}"
            )

        if 'no supported javascript runtime could be found' in lower_message:
            return (
                "지원되는 JavaScript 런타임을 찾지 못했습니다.\n"
                "Node.js 또는 Deno를 설치한 뒤 다시 실행해 주세요."
            )

        return message

    def progress_hook(self, d):
        """
        yt-dlp 다운로드 진행 상황을 처리하는 훅 함수입니다.
        진행률을 계산하여 메인 스레드로 시그널을 보냅니다.
        """
        if self.is_cancelled:
            raise Exception("Download cancelled")

        if d['status'] == 'downloading':
            total = d.get('total_bytes') or d.get('total_bytes_estimate')
            downloaded = d.get('downloaded_bytes', 0)
            if total:
                percent = int(downloaded / total * 100)
                self.progress.emit(percent)
