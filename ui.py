import requests
from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, 
    QLineEdit, QPushButton, QLabel, QMessageBox, QFrame, QProgressBar,
    QSpacerItem, QSizePolicy, QRadioButton, QButtonGroup
)
from PyQt6.QtGui import QPixmap, QIcon
from PyQt6.QtCore import Qt, pyqtSlot
from logic import Worker

class YouTubeDownloader(QWidget):
    def __init__(self):
        super().__init__()
        self.worker = None
        self.current_video_info = None
        self.quality_btn_group = QButtonGroup()
        self.initUI()
        
    def initUI(self):
        """UI 구성 및 스타일 설정"""
        self.setWindowTitle('YouTube Downloader')
        self.setMinimumSize(800, 600)
        
        # 메인 레이아웃 (전체 수직 배치)
        main_layout = QVBoxLayout(self)
        main_layout.setSpacing(25)
        main_layout.setContentsMargins(40, 40, 40, 40)
        
        # 1. 상단: 검색 영역 (입력창 + 조회 버튼)
        search_layout = QHBoxLayout()
        search_layout.setSpacing(15)
        
        self.url_input = QLineEdit()
        self.url_input.setPlaceholderText('Paste YouTube Video URL')
        self.url_input.setFixedHeight(50)
        
        self.search_btn = QPushButton('조회')
        self.search_btn.setFixedSize(100, 50)
        self.search_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.search_btn.clicked.connect(self.search_video)
        
        search_layout.addWidget(self.url_input)
        search_layout.addWidget(self.search_btn)
        
        main_layout.addLayout(search_layout)
        
        # 2. 중단: 비디오 정보 카드 (카드 스타일)
        self.info_frame = QFrame()
        self.info_frame.setObjectName("InfoCard")
        self.info_frame.setVisible(False)
        self.info_frame.setFixedHeight(200)
        
        info_layout = QHBoxLayout(self.info_frame)
        info_layout.setSpacing(25)
        info_layout.setContentsMargins(20, 20, 20, 20)
        
        # 썸네일 (Placeholder)
        self.thumbnail_label = QLabel()
        self.thumbnail_label.setFixedSize(280, 158)  # 16:9 비율
        self.thumbnail_label.setStyleSheet("background-color: #333333; border-radius: 8px;")
        self.thumbnail_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        # 정보 텍스트 영역
        text_layout = QVBoxLayout()
        text_layout.setSpacing(8)
        text_layout.setAlignment(Qt.AlignmentFlag.AlignVCenter)
        
        self.title_label = QLabel("-")
        self.title_label.setObjectName("TitleLabel")
        self.title_label.setWordWrap(True)
        
        self.channel_label = QLabel("-")
        self.channel_label.setObjectName("ChannelLabel")
        
        self.stats_label = QLabel("-") 
        self.stats_label.setObjectName("StatsLabel")
        
        text_layout.addWidget(self.title_label)
        text_layout.addWidget(self.channel_label)
        text_layout.addWidget(self.stats_label)
        
        info_layout.addWidget(self.thumbnail_label)
        info_layout.addLayout(text_layout)
        info_layout.addStretch()
        
        main_layout.addWidget(self.info_frame)
        
        # 2-1. 화질 선택 옵션 영역 (카드 아래)
        self.quality_frame = QFrame()
        self.quality_frame.setVisible(False) 
        self.quality_layout = QHBoxLayout(self.quality_frame)
        self.quality_layout.setContentsMargins(0, 10, 0, 10)
        self.quality_layout.setSpacing(15)
        self.quality_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        main_layout.addWidget(self.quality_frame)
        
        # 2-2. 다운로드 프로그래스 카드 (다운로드 중일 때 표시)
        self.progress_frame = QFrame()
        self.progress_frame.setObjectName("ProgressCard")
        self.progress_frame.setVisible(False)
        self.progress_frame.setFixedHeight(120)
        
        progress_layout = QVBoxLayout(self.progress_frame)
        progress_layout.setContentsMargins(25, 25, 25, 25)
        progress_layout.setSpacing(15)
        progress_layout.setAlignment(Qt.AlignmentFlag.AlignVCenter)
        
        # 다운로드 중인 파일명
        self.download_title_label = QLabel("Downloading...")
        self.download_title_label.setObjectName("DownloadTitle")
        progress_layout.addWidget(self.download_title_label)
        
        # 바 + 퍼센트
        bar_layout = QHBoxLayout()
        bar_layout.setSpacing(15)
        
        self.styled_progress_bar = QProgressBar()
        self.styled_progress_bar.setFixedHeight(10)
        self.styled_progress_bar.setTextVisible(False)
        self.styled_progress_bar.setRange(0, 100)
        self.styled_progress_bar.setValue(0)
        
        self.percentage_label = QLabel("0% Downloaded")
        self.percentage_label.setObjectName("PercentageLabel")
        self.percentage_label.setFixedWidth(120)
        self.percentage_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        
        bar_layout.addWidget(self.styled_progress_bar)
        bar_layout.addWidget(self.percentage_label)
        
        progress_layout.addLayout(bar_layout)
        
        main_layout.addWidget(self.progress_frame)
        
        # 여백 (Spacer)
        main_layout.addStretch()
        
        # 상태 메시지 (간단 알림용)
        self.status_label = QLabel("")
        self.status_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.status_label.setObjectName("StatusLabel")
        main_layout.addWidget(self.status_label)

        # 3. 하단: 다운로드 버튼
        self.download_btn = QPushButton("⬇ 다운로드")
        self.download_btn.setObjectName("DownloadBtn")
        self.download_btn.setFixedHeight(60)
        self.download_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.download_btn.clicked.connect(self.download_video)
        self.download_btn.setEnabled(False) 
        
        main_layout.addWidget(self.download_btn)
        
        self.apply_styles()

    def apply_styles(self):
        """QSS 스타일시트 적용"""
        self.setStyleSheet("""
            QWidget {
                background-color: #121212;
                color: #FFFFFF;
                font-family: 'Segoe UI', sans-serif;
            }
            
            /* 검색 입력창 */
            QLineEdit {
                background-color: #252525;
                border: 1px solid #333333;
                border-radius: 8px;
                padding: 0 15px;
                font-size: 16px;
                color: #FFFFFF;
            }
            QLineEdit:focus {
                border: 1px solid #555555;
            }
            
            /* 일반 버튼 (조회 등) */
            QPushButton {
                background-color: #333333;
                border: none;
                border-radius: 8px;
                color: #FFFFFF;
                font-size: 15px;
                font-weight: bold;
            }
            QPushButton:hover {
                background-color: #444444;
            }
            
            /* 정보 카드 */
            QFrame#InfoCard {
                background-color: #212121;
                border-radius: 12px;
                border: none;
            }

            /* 프로그래스 카드 */
            QFrame#ProgressCard {
                background-color: #252525;
                border-radius: 12px;
                border: 1px solid #333333;
            }
            
            /* 텍스트 라벨 */
            QLabel#TitleLabel {
                font-size: 18px;
                font-weight: bold;
                color: #FFFFFF;
            }
            
            QLabel#ChannelLabel {
                font-size: 14px;
                color: #AAAAAA;
            }
            
            QLabel#StatsLabel {
                font-size: 13px;
                color: #888888;
            }

            QLabel#DownloadTitle {
                font-size: 16px;
                color: #EEEEEE;
            }
            
            QLabel#PercentageLabel {
                font-size: 14px;
                color: #CCCCCC;
            }
            
            /* 라디오 버튼 */
            QRadioButton {
                color: #AAAAAA;
                font-size: 14px;
                spacing: 8px;
            }
            QRadioButton::indicator {
                width: 18px;
                height: 18px;
                border-radius: 10px;
                border: 2px solid #555555;
                background-color: #252525;
            }
            QRadioButton::indicator:checked {
                background-color: #D32F2F;
                border: 2px solid #D32F2F;
            }
            QRadioButton:hover {
                color: #FFFFFF;
            }
            
            /* 하단 다운로드 버튼 (빨간색 강조) */
            QPushButton#DownloadBtn {
                background-color: #D32F2F; 
                color: #FFFFFF;
                font-size: 18px;
                font-weight: bold;
                border-radius: 10px;
            }
            QPushButton#DownloadBtn:hover {
                background-color: #F44336;
            }
            QPushButton#DownloadBtn:disabled {
                background-color: #333333;
                color: #777777;
            }
            
            /* 스타일링된 프로그래스 바 (Gradient) */
            QProgressBar {
                background-color: #404040;
                border: none;
                border-radius: 5px;
            }
            QProgressBar::chunk {
                background-color: qlineargradient(spread:pad, x1:0, y1:0, x2:1, y2:0, stop:0 #3b82f6, stop:1 #2dd4bf);
                border-radius: 5px;
            }
            
            QLabel#StatusLabel {
                color: #AAAAAA;
                font-size: 14px;
                margin-bottom: 5px;
            }
        """)

    @pyqtSlot()
    def search_video(self):
        url = self.url_input.text().strip()
        if not url:
            return

        self.status_label.setText("검색 중...")
        self.search_btn.setEnabled(False)
        self.info_frame.setVisible(False)
        self.quality_frame.setVisible(False)
        self.progress_frame.setVisible(False)
        self.download_btn.setEnabled(False)
        
        # Remove existing radio buttons
        for i in reversed(range(self.quality_layout.count())): 
            item = self.quality_layout.itemAt(i)
            if item.widget():
                item.widget().setParent(None)

        self.worker = Worker(url, mode='info')
        self.worker.finished.connect(self.handle_info_result)
        self.worker.error.connect(self.handle_error)
        self.worker.start()

    @pyqtSlot(dict)
    def handle_info_result(self, info):
        self.search_btn.setEnabled(True)
        self.status_label.setText("")
        self.current_video_info = info
        
        # 정보 표시
        self.title_label.setText(info.get('title', 'Unknown Title'))
        self.channel_label.setText(info.get('uploader', 'Unknown Channel'))
        
        likes = info.get('like_count', 0)
        views = info.get('view_count', 0)
        self.stats_label.setText(f"👍 {likes:,}  •  👁 {views:,}")
        
        # 썸네일 로드
        thumb_url = info.get('thumbnail')
        if thumb_url:
            try:
                data = requests.get(thumb_url).content
                pixmap = QPixmap()
                pixmap.loadFromData(data)
                self.thumbnail_label.setPixmap(pixmap.scaled(
                    self.thumbnail_label.size(), 
                    Qt.AspectRatioMode.KeepAspectRatioByExpanding, 
                    Qt.TransformationMode.SmoothTransformation
                ))
            except:
                self.thumbnail_label.setText("No Image")
                
        # 화질 선택 옵션 생성
        resolutions = info.get('resolutions', [])
        if resolutions:
            for res in resolutions:
                rb = QRadioButton(f"{res}p")
                self.quality_layout.addWidget(rb)
                self.quality_btn_group.addButton(rb, res)
            
            # MP3 옵션 추가
            audio_rb = QRadioButton("MP3 Audio")
            self.quality_layout.addWidget(audio_rb)
            self.quality_btn_group.addButton(audio_rb, 1) # MP3 ID = 1
            
            if self.quality_layout.count() > 0:
                self.quality_layout.itemAt(0).widget().setChecked(True)
                
            self.quality_frame.setVisible(True)

        self.info_frame.setVisible(True)
        self.download_btn.setEnabled(True)

    @pyqtSlot(str)
    def handle_error(self, msg):
        self.search_btn.setEnabled(True)
        self.download_btn.setEnabled(True if self.current_video_info else False)
        self.status_label.setText("오류 발생")
        self.progress_frame.setVisible(False)
        self.info_frame.setVisible(True if self.current_video_info else False)
        QMessageBox.warning(self, "Error", msg)

    @pyqtSlot()
    def download_video(self):
        if not self.current_video_info:
            return
            
        url = self.url_input.text().strip()
        
        # 선택된 화질 확인
        selected_id = self.quality_btn_group.checkedId()
        format_str = 'best'
        quality_label = 'best'
        
        if selected_id == 1:
            format_str = 'audio'
            quality_label = 'audio'
        elif selected_id > 1:
            format_str = f'bestvideo[height={selected_id}]+bestaudio/best[height={selected_id}]'
            quality_label = f'{selected_id}p'

        # UI 전환 logic: 정보창/버튼 숨기고 프로그래스바 카드 보이기
        self.info_frame.setVisible(False)
        self.quality_frame.setVisible(False)
        self.download_btn.setVisible(False)
        
        self.progress_frame.setVisible(True)
        self.styled_progress_bar.setValue(0)
        self.percentage_label.setText("0% Downloaded")
        self.download_title_label.setText(f"{self.current_video_info.get('title', 'Video')} ({quality_label})")
        self.status_label.setText("다운로드 시작...")
        
        self.worker = Worker(url, mode='download', format_str=format_str, quality_label=quality_label)
        self.worker.finished.connect(self.handle_download_complete)
        self.worker.error.connect(self.handle_error)
        self.worker.progress.connect(self.update_progress)
        self.worker.start()

    @pyqtSlot(int)
    def update_progress(self, val):
        self.styled_progress_bar.setValue(val)
        self.percentage_label.setText(f"{val}% Downloaded")

    @pyqtSlot(dict)
    def handle_download_complete(self, result):
        self.status_label.setText("다운로드 완료!")
        self.percentage_label.setText("100% Downloaded")
        self.styled_progress_bar.setValue(100)
        
        QMessageBox.information(self, "Success", "다운로드가 완료되었습니다.")
        
        # UI 복귀
        self.progress_frame.setVisible(False)
        self.info_frame.setVisible(True)
        self.quality_frame.setVisible(True)
        self.download_btn.setVisible(True)
