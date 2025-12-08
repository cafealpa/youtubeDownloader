import os
import yt_dlp
from PyQt6.QtCore import QThread, pyqtSignal

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

    def run(self):
        """
        스레드 실행 메인 함수입니다.
        FFmpeg 경로를 설정하고 yt-dlp를 사용하여 정보 조회 또는 다운로드를 수행합니다.
        """
        try:
            # Use absolute path for FFmpeg
            basedir = os.path.dirname(os.path.abspath(__file__))
            ffmpeg_path = os.path.join(basedir, 'ffmpeg.exe')
            
            print(f"DEBUG: Mode={self.mode}, Format={self.format_str}")
            print(f"DEBUG: FFmpeg Path={ffmpeg_path}")

            ydl_opts = {
                'quiet': True,
                'no_warnings': True,
                'ffmpeg_location': ffmpeg_path,
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
                    self.finished.emit({'status': 'downloaded'})

        except Exception as e:
            self.error.emit(str(e))

    def progress_hook(self, d):
        """
        yt-dlp 다운로드 진행 상황을 처리하는 훅 함수입니다.
        진행률을 계산하여 메인 스레드로 시그널을 보냅니다.
        """
        if d['status'] == 'downloading':
            total = d.get('total_bytes') or d.get('total_bytes_estimate')
            downloaded = d.get('downloaded_bytes', 0)
            if total:
                percent = int(downloaded / total * 100)
                self.progress.emit(percent)
