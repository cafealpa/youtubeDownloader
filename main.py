import sys
from PyQt6.QtWidgets import QApplication
from ui import YouTubeDownloader

if __name__ == '__main__':
    app = QApplication(sys.argv)
    
    # Set fallback font just in case
    font = app.font()
    font.setFamily("Segoe UI")
    app.setFont(font)
    
    ex = YouTubeDownloader()
    ex.show()
    sys.exit(app.exec())
