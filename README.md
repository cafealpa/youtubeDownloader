# 📺 YouTube Downloader Pro

**바이브코딩으로 만들었습니다.**

PyQt6와 yt-dlp를 기반으로 제작된 모던하고 강력한 유튜브 다운로더입니다.
직관적인 다크 테마 UI와 강력한 다운로드 기능을 제공합니다.

현재 배포 버전은 **1.1.20260930**입니다. Windows 실행 파일은
[최신 GitHub 릴리스](https://github.com/cafealpa/youtubeDownloader/releases/latest)에서 받을 수 있습니다.
ZIP을 모두 압축 해제한 뒤 `YouTubeDownloader.exe`를 실행하세요.

## ✨ 주요 기능
- **🔍 간편한 조회**: 유튜브 링크만 넣으면 썸네일과 영상 정보를 즉시 확인
- **🎨 모던 다크 테마**: 눈이 편안하고 세련된 디자인 (유튜브 프리미엄 스타일)
- **🎥 고화질 다운로드**: 4K, 1080p 등 다양한 해상도 지원 (FFmpeg 자동 연동)
- **🎵 오디오 추출**: 영상 없이 소리만 추출하여 MP3로 저장하는 기능
- **⚡ 실시간 진행률**: 다운로드 진행 상황을 프로그레스 바로 확인
- **⛔ 취소 기능**: 다운로드 중 언제든지 취소 가능

## 🛠️ 설치 및 실행 방법

### 1. 필수 프로그램
이 프로그램은 Python 3.x 환경에서 실행됩니다.

### 2. 가상환경 설정 및 라이브러리 설치
프로젝트의 독립적인 실행 환경을 위해 가상환경 사용을 권장합니다.

**가상환경 생성 및 활성화**
```bash
# 가상환경 생성 (.venv)
py -m venv .venv

# 가상환경 활성화 (Windows)
.venv\Scripts\activate
```

**라이브러리 설치**
(가상환경이 활성화된 상태에서 실행하세요)
```bash
pip install -r requirements.txt
```

### 2-1. JavaScript 런타임 (Deno 내장)
2026년 현재 YouTube 다운로드에는 JavaScript 런타임이 필수입니다.
이 프로젝트는 프로젝트 루트의 `deno.exe`를 자동으로 사용하므로 별도 설치가 필요 없습니다.

`deno.exe`가 없는 경우 아래에서 받아 프로젝트 루트에 두면 됩니다.
```text
https://github.com/denoland/deno/releases (deno-x86_64-pc-windows-msvc.zip)
```
시스템에 `deno`, `node`, `bun`, `quickjs`가 설치되어 있으면 그것도 자동 탐지합니다.

### 3. FFmpeg 설치
고화질 영상 합치기 및 MP3 변환을 위해 FFmpeg가 필요합니다.
포함된 스크립트를 실행하면 자동으로 설치됩니다.
```bash
py install_ffmpeg.py
```

### 4. 프로그램 실행
```bash
py main.py
```

## 문제 해결

### 403 Forbidden 오류가 나는 경우
다음 순서로 확인하세요.

```bash
pip install -U "yt-dlp[default]"
```

그리고 프로젝트 루트에 `deno.exe`가 있는지 확인하세요 (내장 JS 런타임).
없으면 시스템에 설치된 `deno`, `node`, `bun`, `quickjs`를 자동 탐지하며, 둘 다 없으면 YouTube 다운로드가 실패할 수 있습니다.

## 📝 사용법
1. 상단 입력창에 유튜브 동영상 URL을 붙여넣고 **[조회]** 버튼을 누릅니다.
2. 영상 정보가 뜨면 원하는 **화질** 또는 **오디오(MP3)** 옵션을 선택합니다.
3. **[다운로드 시작]** 버튼을 누르면 `Downloads` 폴더에 자동으로 저장됩니다.

## 🔧 유지보수: 라이브러리 업데이트 및 재배포

YouTube는 수시로 사이트 구조를 바꾸기 때문에, 몇 달 지나면 다운로드가 다시 실패할 수 있습니다.
그 경우 아래 절차대로 yt-dlp를 업데이트하고 exe를 다시 만들어 배포하면 됩니다.
(모든 명령은 프로젝트 루트에서 실행)

### 1. 라이브러리 업데이트
```bash
.venv\Scripts\python -m pip install -U "yt-dlp[default]" yt-dlp-ejs
```

업데이트 후 `python main.py`로 실행해서 다운로드가 되는지 먼저 확인합니다.

### 2. exe 재빌드
```bash
.venv\Scripts\pyinstaller YouTubeDownloader.spec --noconfirm
```

결과물은 `dist\YouTubeDownloader\` 폴더에 생성됩니다. `YouTubeDownloader.exe`를 실행해 동작을 확인합니다.

### 3. 배포 (GitHub 릴리즈)
버전은 `1.1.YYYYMMDD` (빌드 날짜) 형식을 사용합니다.

```bash
# dist 폴더를 zip으로 압축 (버전에 맞게 파일명 변경)
powershell Compress-Archive -Path dist\YouTubeDownloader -DestinationPath YouTubeDownloader-1.1.YYYYMMDD-win64.zip

# 변경 사항 커밋/푸시 후 릴리즈 생성
git add -u
git commit -m "다운로드 라이브러리 업데이트"
git push origin main
gh release create v1.1.YYYYMMDD --title "YouTube Downloader 1.1.YYYYMMDD" --notes "yt-dlp 업데이트" YouTubeDownloader-1.1.YYYYMMDD-win64.zip
```

### 참고: 내장 바이너리 업데이트
- **deno.exe**: 오래되어 문제가 생기면 https://github.com/denoland/deno/releases 에서
  `deno-x86_64-pc-windows-msvc.zip`을 받아 프로젝트 루트의 `deno.exe`를 교체 후 재빌드합니다.
- **ffmpeg.exe / ffprobe.exe**: `py install_ffmpeg.py`로 다시 받을 수 있습니다.
- 새 바이너리는 재빌드 시 exe에 자동으로 포함됩니다 (`YouTubeDownloader.spec`에 정의됨).

---
Developed with **Vibe Coding** technology.
