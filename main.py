import sys
import os
import requests
from PyQt6.QtWidgets import (QApplication, QWidget, QVBoxLayout, QHBoxLayout, 
                             QLineEdit, QPushButton, QLabel, QMessageBox, QFrame, QProgressBar,
                             QRadioButton, QButtonGroup, QGroupBox)
from PyQt6.QtGui import QPixmap, QImage
from PyQt6.QtCore import Qt, QThread, pyqtSignal
import yt_dlp

class Worker(QThread):
    finished = pyqtSignal(dict)
    error = pyqtSignal(str)
    progress = pyqtSignal(int)

    def __init__(self, url, mode='info', format_str=None):
        """
        워커 스레드를 초기화합니다.
        :param url: 유튜브 동영상 URL
        :param mode: 'info' (정보 조회) 또는 'download' (다운로드)
        :param format_str: 다운로드 포맷 문자열 (비디오 해상도 또는 오디오)
        """
        super().__init__()
        self.url = url
        self.mode = mode
        self.format_str = format_str

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
                
                # Base Options
                opts = {
                    'outtmpl': os.path.join(download_path, '%(title)s.%(ext)s'),
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

class YouTubeDownloader(QWidget):
    def __init__(self):
        """애플리케이션 메인 윈도우를 초기화합니다."""
        super().__init__()
        self.initUI()
        self.current_video_info = None

    def initUI(self):
        """
        UI 컴포넌트들을 생성하고 레이아웃을 구성합니다.
        모던 다크 테마 스타일시트도 정의합니다.
        """
        self.setWindowTitle('YouTube Downloader Pro')
        self.setGeometry(300, 300, 800, 600)

        # Main Layout
        main_layout = QVBoxLayout()
        main_layout.setSpacing(20)
        main_layout.setContentsMargins(30, 30, 30, 30)

        # --- Header ---
        header_label = QLabel("YOUTUBE DOWNLOADER")
        header_label.setObjectName("header")
        header_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(header_label)

        # --- Input Area ---
        input_frame = QFrame()
        input_frame.setObjectName("card")
        input_layout = QHBoxLayout(input_frame)
        input_layout.setContentsMargins(15, 15, 15, 15)
        
        self.url_input = QLineEdit()
        self.url_input.setPlaceholderText('🔗 여기에 유튜브 링크를 붙여넣으세요')
        
        self.search_btn = QPushButton('조회')
        self.search_btn.setObjectName("action_btn")
        self.search_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.search_btn.setFixedWidth(100)
        self.search_btn.clicked.connect(self.search_video)
        
        input_layout.addWidget(self.url_input)
        input_layout.addWidget(self.search_btn)
        main_layout.addWidget(input_frame)

        # --- Info Area (Card Style) ---
        self.info_frame = QFrame()
        self.info_frame.setObjectName("card")
        self.info_frame.setVisible(False) # Initially hidden
        
        # Horizontal Layout for Image + Text
        info_layout = QHBoxLayout(self.info_frame)
        info_layout.setContentsMargins(15, 15, 15, 15)
        info_layout.setSpacing(20)
        
        # Thumbnail (Left)
        self.thumbnail_label = QLabel()
        self.thumbnail_label.setFixedSize(320, 180)
        self.thumbnail_label.setStyleSheet("background-color: #000; border-radius: 8px;")
        self.thumbnail_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        # Text Info (Right)
        text_info_layout = QVBoxLayout()
        text_info_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        
        self.title_label = QLabel("-")
        self.title_label.setObjectName("title")
        self.title_label.setWordWrap(True)
        
        self.view_count_label = QLabel("👁️ 조회수: -")
        self.like_count_label = QLabel("👍 좋아요: -")
        
        text_info_layout.addWidget(self.title_label)
        text_info_layout.addSpacing(10)
        text_info_layout.addWidget(self.view_count_label)
        text_info_layout.addWidget(self.like_count_label)
        text_info_layout.addStretch()

        info_layout.addWidget(self.thumbnail_label)
        info_layout.addLayout(text_info_layout)
        main_layout.addWidget(self.info_frame)

        # --- Options Area ---
        self.quality_group = QGroupBox("다운로드 옵션")
        self.quality_layout = QHBoxLayout() # Horizontal options
        self.quality_group.setLayout(self.quality_layout)
        self.quality_group.setVisible(False)
        main_layout.addWidget(self.quality_group)
        
        self.quality_btn_group = QButtonGroup()

        # --- Status & Progress ---
        self.status_label = QLabel("준비됨")
        self.status_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.status_label.setObjectName("status")
        main_layout.addWidget(self.status_label)

        self.progress_bar = QProgressBar()
        self.progress_bar.setVisible(False)
        self.progress_bar.setTextVisible(True)
        self.progress_bar.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(self.progress_bar)

        # --- Action Buttons ---
        buttons_layout = QHBoxLayout()
        
        self.download_btn = QPushButton('다운로드 시작')
        self.download_btn.setObjectName("action_btn")
        self.download_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.download_btn.setMinimumHeight(50)
        self.download_btn.clicked.connect(self.download_video)
        self.download_btn.setEnabled(False)
        
        self.cancel_btn = QPushButton('취소')
        self.cancel_btn.setObjectName("cancel_btn")
        self.cancel_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.cancel_btn.setMinimumHeight(50)
        self.cancel_btn.clicked.connect(self.cancel_download)
        self.cancel_btn.setVisible(False)

        buttons_layout.addWidget(self.download_btn)
        buttons_layout.addWidget(self.cancel_btn)
        main_layout.addLayout(buttons_layout)

        main_layout.addStretch()
        self.setLayout(main_layout)

        # --- Stylesheet (Modern Dark Theme) ---
        self.setStyleSheet("""
            QWidget {
                background-color: #181818;
                color: #ffffff;
                font-family: 'Segoe UI', sans-serif;
                font-size: 14px;
            }
            
            /* Header */
            QLabel#header {
                font-size: 24px;
                font-weight: bold;
                color: #ff0000;
                letter-spacing: 2px;
                margin-bottom: 10px;
            }

            /* Cards */
            QFrame#card {
                background-color: #212121;
                border: 1px solid #303030;
                border-radius: 12px;
            }

            /* Input */
            QLineEdit {
                background-color: #121212;
                border: 2px solid #303030;
                border-radius: 8px;
                padding: 12px;
                color: #ffffff;
                font-size: 15px;
            }
            QLineEdit:focus {
                border: 2px solid #ff0000;
            }

            /* Buttons */
            QPushButton {
                background-color: #303030;
                border: none;
                border-radius: 8px;
                padding: 12px;
                font-weight: bold;
                color: #aaaaaa;
            }
            QPushButton:hover {
                background-color: #404040;
                color: #ffffff;
            }
            
            QPushButton#action_btn {
                background-color: #cc0000;
                color: white;
                font-size: 16px;
            }
            QPushButton#action_btn:hover {
                background-color: #ff0000;
            }
            QPushButton#action_btn:disabled {
                background-color: #333333;
                color: #666666;
            }

            QPushButton#cancel_btn {
                background-color: #444444;
                color: white;
            }
            QPushButton#cancel_btn:hover {
                background-color: #666666;
            }

            /* Labels */
            QLabel#title {
                font-size: 18px;
                font-weight: bold;
                color: #ffffff;
            }
            QLabel#status {
                color: #aaaaaa;
                font-size: 13px;
                margin-top: 10px;
            }

            /* GroupBox */
            QGroupBox {
                border: 1px solid #303030;
                border-radius: 8px;
                margin-top: 20px;
                padding-top: 15px;
                font-weight: bold;
                color: #aaaaaa;
            }
            QGroupBox::title {
                subcontrol-origin: margin;
                left: 10px;
                padding: 0 5px;
            }

            /* Radio Button */
            QRadioButton {
                padding: 5px;
            }
            QRadioButton::indicator {
                width: 18px;
                height: 18px;
                border-radius: 9px;
                border: 2px solid #555555;
            }
            QRadioButton::indicator:checked {
                background-color: #ff0000;
                border: 2px solid #ff0000;
            }

            /* Progress Bar */
            QProgressBar {
                background-color: #121212;
                border: none;
                border-radius: 4px;
                height: 8px;
                text-align: center;
            }
            QProgressBar::chunk {
                background-color: #ff0000;
                border-radius: 4px;
            }
        """)

    def search_video(self):
        """
        입력된 URL로 비디오 정보를 검색합니다.
        기존 UI 상태를 초기화하고 정보 조회 스레드를 시작합니다.
        """
        url = self.url_input.text().strip()
        if not url:
            QMessageBox.warning(self, 'Error', 'URL을 입력해주세요.')
            return

        # Reset UI
        self.info_frame.setVisible(False) # Hide info card initially
        self.thumbnail_label.clear()
        self.title_label.setText("-")
        self.view_count_label.setText("👁️ 조회수: -")
        self.like_count_label.setText("👍 좋아요: -")
        self.download_btn.setEnabled(False)
        self.current_video_info = None
        
        # Clear Quality Options
        self.quality_group.setVisible(False)
        for i in reversed(range(self.quality_layout.count())): 
            self.quality_layout.itemAt(i).widget().setParent(None)
        
        # Reset Buttons
        self.download_btn.setVisible(True)
        self.cancel_btn.setVisible(False)
        self.progress_bar.setVisible(False)

        self.search_btn.setEnabled(False)
        self.status_label.setText("🔍 정보를 가져오는 중입니다...")
        
        self.worker = Worker(url, mode='info')
        self.worker.finished.connect(self.handle_info_result)
        self.worker.error.connect(self.handle_error)
        self.worker.start()

    def handle_info_result(self, info):
        """
        비디오 정보 조회 성공 시 호출됩니다.
        썸네일, 제목, 조회수 등을 표시하고 다운로드 옵션을 생성합니다.
        """
        self.search_btn.setEnabled(True)
        self.status_label.setText("✅ 조회 완료")
        self.current_video_info = info
        self.info_frame.setVisible(True) # Show info card
        
        # Update Labels
        self.title_label.setText(info.get('title', '-'))
        self.view_count_label.setText(f"👁️ 조회수: {info.get('view_count', 0):,}")
        self.like_count_label.setText(f"👍 좋아요: {info.get('like_count', 0):,}")

        # Update Thumbnail
        thumbnail_url = info.get('thumbnail')
        if thumbnail_url:
            try:
                data = requests.get(thumbnail_url).content
                pixmap = QPixmap()
                pixmap.loadFromData(data)
                
                # Scale to fit fixed size (320x180) - 16:9 ratio
                scaled_pixmap = pixmap.scaled(self.thumbnail_label.size(), 
                                            Qt.AspectRatioMode.KeepAspectRatioByExpanding, 
                                            Qt.TransformationMode.SmoothTransformation)
                self.thumbnail_label.setPixmap(scaled_pixmap)
            except Exception:
                self.thumbnail_label.setText("이미지 없음")

        # Create Radio Buttons for Resolutions
        resolutions = info.get('resolutions', [])
        if resolutions:
            self.quality_group.setVisible(True)
            for res in resolutions:
                rb = QRadioButton(f"{res}p")
                self.quality_layout.addWidget(rb)
                self.quality_btn_group.addButton(rb, res)
            
            # Add Audio Only Option
            audio_rb = QRadioButton("MP3 오디오")
            self.quality_layout.addWidget(audio_rb)
            self.quality_btn_group.addButton(audio_rb, 1)

            # Select the highest quality by default
            if self.quality_layout.count() > 0:
                self.quality_layout.itemAt(0).widget().setChecked(True)
        
        self.download_btn.setEnabled(True)
        self.download_btn.setText("다운로드 시작")

    def download_video(self):
        """
        사용자가 선택한 옵션으로 비디오/오디오 다운로드를 시작합니다.
        UI를 다운로드 중 상태로 변경하고 다운로드 스레드를 시작합니다.
        """
        if not self.current_video_info:
            return

        url = self.url_input.text().strip()
        
        # Get Selected Resolution
        selected_id = self.quality_btn_group.checkedId()
        format_str = 'best'
        
        if selected_id == 1:
            format_str = 'audio'
            status_msg = "🎵 오디오 다운로드 중... (MP3)"
        elif selected_id > 1:
            format_str = f'bestvideo[height={selected_id}]+bestaudio/best[height={selected_id}]'
            status_msg = f"🎬 다운로드 중... ({selected_id}p)"
        else:
             status_msg = "⬇️ 다운로드 중..."

        # Toggle Buttons
        self.download_btn.setVisible(False)
        self.cancel_btn.setVisible(True)
        
        # Show Progress Bar
        self.progress_bar.setVisible(True)
        self.progress_bar.setValue(0)
        
        self.status_label.setText(status_msg)

        self.worker = Worker(url, mode='download', format_str=format_str)
        self.worker.finished.connect(self.handle_download_result)
        self.worker.error.connect(self.handle_error)
        self.worker.progress.connect(self.update_progress)
        self.worker.start()

    def update_progress(self, percent):
        """다운로드 진행률 바를 업데이트합니다."""
        self.progress_bar.setValue(percent)

    def cancel_download(self):
        """진행 중인 다운로드를 취소하고 UI를 초기화합니다."""
        if self.worker and self.worker.isRunning():
            self.worker.terminate()
            self.worker.wait()
        
        self.status_label.setText("⛔ 다운로드 취소됨")
        self.download_btn.setVisible(True)
        self.cancel_btn.setVisible(False)
        self.progress_bar.setVisible(False)
        self.download_btn.setEnabled(True)

    def handle_download_result(self, result):
        """다운로드 완료 시 호출되어 성공 메시지를 표시합니다."""
        self.download_btn.setVisible(True)
        self.cancel_btn.setVisible(False)
        self.progress_bar.setVisible(False)
        self.download_btn.setEnabled(True)
        self.status_label.setText("✨ 다운로드 완료!")
        QMessageBox.information(self, '완료', '다운로드가 완료되었습니다!\n다운로드 폴더를 확인하세요.')

    def handle_error(self, error_msg):
        """에러 발생 시 호출되어 에러 메시지를 표시하고 UI를 복구합니다."""
        self.search_btn.setEnabled(True)
        self.download_btn.setVisible(True)
        self.cancel_btn.setVisible(False)
        self.progress_bar.setVisible(False)
        self.download_btn.setEnabled(True if self.current_video_info else False)
        self.status_label.setText("⚠️ 오류 발생")
        QMessageBox.critical(self, 'Error', f"오류가 발생했습니다:\n{error_msg}")

if __name__ == '__main__':
    app = QApplication(sys.argv)
    
    # Set fallback font just in case
    font = app.font()
    font.setFamily("Segoe UI")
    app.setFont(font)
    
    ex = YouTubeDownloader()
    ex.show()
    sys.exit(app.exec())
