# YouTube Downloader

**바이브코딩으로 만들었습니다.**

## Windows 네이티브 앱 · v2.0.0

**Rust + Iced 0.14**로 구현한 Windows x64 YouTube 다운로더입니다.
React나 WebView2 없이 네이티브 창을 렌더링하고, yt-dlp를 sidecar로 실행합니다.
사용자 PC에는 Rust, Cargo, Python, Node.js 설치가 필요하지 않습니다.

이 저장소의 기본 소스는 Rust 앱입니다. 이전 Python/PyQt 소스는 Git 이력에서 확인할 수 있습니다.
Windows 실행 패키지는 [v2.0.0 릴리스](https://github.com/cafealpa/youtubeDownloader/releases/tag/v2.0.0)에서 받을 수 있습니다.
ZIP 전체를 압축 해제한 뒤 `YouTubeDownloader.exe`를 실행하세요. `tools/` 폴더를 함께 유지해야 합니다.

- 왼쪽: 검색어 또는 영상 링크 조회, 실제 썸네일과 검색 결과
- 검색 결과는 처음 20개, `더 보기`로 다음 20개씩 추가합니다. 중복 영상은 제외하며 추가 검색 실패 시 기존 목록을 유지합니다.
- 업로드 날짜는 결과 표시 후 최대 2개씩 백그라운드 조회합니다. `확인 중` → `YYYY.MM.DD` 또는 `날짜 확인 불가`로 표시하며, 성공한 날짜는 앱 실행 중 캐시합니다. 날짜는 yt-dlp의 UTC 기준입니다.
- 더 보기는 같은 검색어의 다음 순위 범위를 다시 조회하므로 YouTube 검색 순위가 바뀌면 일부 중복이나 누락이 생길 수 있습니다.
- 오른쪽: 선택한 영상, YouTube 링크 열기, 지원 화질, MP4/MP3 선택
- 하단: 진행률, 병합/변환 상태, 취소, 완료된 파일의 **폴더 열기 아이콘**
- `yt-dlp.exe`를 별도 프로세스로 실행하며 FFmpeg/ffprobe/Deno를 함께 사용합니다.
- Windows의 실제 다운로드 폴더를 사용합니다. 폴더를 다른 드라이브로 옮긴 경우도 반영합니다.
- 완료는 엔진 정상 종료와 최종 파일 존재를 확인한 뒤 표시합니다.
- Windows Job Object로 다운로드와 자식 프로세스를 관리합니다.
- 새 검색/영상 선택으로 오래된 응답이 도착해도 현재 선택을 덮지 않습니다.
- 라이트/다크 전환을 지원합니다. 현재 테마 선택은 앱 실행 동안만 유지됩니다.
- **설정 → 다운로드 엔진**에서 현재/최신/되돌릴 버전을 확인하고 업데이트하거나 이전 버전으로 전환할 수 있습니다.

### 앱 안에서 yt-dlp 업데이트

1. 상단 **설정**을 열고 **업데이트 확인**을 누릅니다.
2. 새 정식 버전이 있으면 **새 버전 설치**를 누릅니다.
3. 공식 SHA-256 및 실제 `--version` 응답을 검증한 뒤 적용합니다.
4. 문제가 생기면 **이전 버전으로 되돌리기**를 누릅니다. 재시작 없이 다음 작업부터 적용됩니다.

검색·영상 상세 및 날짜 조회·다운로드 중에는 업데이트 및 복구 버튼을 잠급니다.
업데이트 중에는 새 검색·다운로드를 시작할 수 없습니다.
확인은 사용자가 버튼을 눌렀을 때만 수행하며, 자동 설치하지 않습니다.
현재는 GitHub 공식 **stable** 채널의 Windows x64 독립 실행 파일을 지원합니다.

업데이트한 엔진은 `ProjectDirs::data_local_dir()/engines` 아래에 저장합니다
(일반적인 Windows 경로: `%LOCALAPPDATA%\cafealpa\YouTubeDownloader\data\engines`).
엔진별 SHA-256 파일명으로 보관하며, 같은 디렉터리의 `state.json`을 원자적으로 교체해
활성 버전과 이전 버전을 기록합니다. 내려받기·해시 검증·버전 실행·설정 교체 중 실패하면
현재 선택은 유지됩니다. 다른 앱 창과의 동시 업데이트는 파일 잠금으로 방지합니다.
복구 시에도 이전 파일의 해시와 실행 버전을 재확인합니다.

동봉 엔진과 기존 관리 엔진 파일은 삭제하거나 덮어쓰지 않습니다. 중간 종료 후 남은
미사용 파일은 실행 대상으로 선택되지 않습니다. FFmpeg/ffprobe/Deno는 앱의 `tools/`를
계속 사용하며 yt-dlp 업데이트 대상에는 포함하지 않습니다.

### 개발 실행

Windows x64, Rust MSVC 툴체인과 Visual Studio C++ 빌드 도구가 필요합니다.
검증에 사용한 Rust 버전은 1.93.1입니다.

```powershell
# 공식 Windows standalone 엔진 다운로드 및 공식 SHA-256 확인.
# 저장소 루트의 FFmpeg/ffprobe/Deno를 tools/로 복사합니다.
.\scripts\prepare-sidecar.ps1
cargo run --locked
```

엔진은 `tools/yt-dlp.exe`에서 실행됩니다. 이 파일은 Python 설치가 필요 없는
공식 독립 실행 파일입니다. `tools/`는 Git에 포함하지 않습니다.
초기 동봉 엔진 버전은 동작 검증 기준인 `2026.08.19`로 고정했습니다.
다른 버전은 `prepare-sidecar.ps1 -Version YYYY.MM.DD`로 명시적으로 준비합니다.

```powershell
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
# 실제 네트워크/짧은 공개 영상 다운로드 검증. target/검증 다운로드에만 저장.
cargo test --locked live_search_and_download -- --ignored --nocapture
# 실제 검색 20개 + 다음 20개 및 업로드 날짜 검증.
cargo test --locked live_search_pages_and_dates -- --ignored --nocapture
# 별도 테스트 폴더에서 실제 공식 구버전 → 최신 → 복구, 실패 시 유지 검증.
# 사용자 앱 데이터의 활성 엔진은 변경하지 않습니다.
cargo test --locked live_update_and_rollback -- --ignored --nocapture

# 로컬 실행 패키지 생성 (공개 릴리스/설치 프로그램 배포는 하지 않습니다).
.\scripts\package-rust.ps1
```

로컬 패키지 실행 파일은 `dist/YouTubeDownloader-Iced/YouTubeDownloader.exe`입니다.
같은 폴더의 `tools/`와 함께 이동해야 합니다.
공개 배포용 패키지는 `scripts/package-release.ps1`로 생성합니다.
라이선스 고지, 파일별 SHA-256 manifest 및 ZIP 체크섬을 포함합니다.
이 스크립트는 기존 버전 폴더를 덮어쓰지 않습니다. 별도 PC 실행 검증은 아직 완료하지 않았습니다.

검색 실패 시 개별 영상 링크로 재시도할 수 있습니다. YouTube 차단, 지역/로그인 제한은
검색과 다운로드 모두에 영향을 줄 수 있습니다. 현재는 단일 영상 작업을 지원하며,
재생목록/채널 주소는 받지 않습니다.

실제 Iced 화면을 확인하는 로컬 QA 옵션:

```powershell
cargo run --locked -- --query "파이썬 강좌" --screenshot target/iced-search.png
cargo run --locked -- --settings --screenshot target/iced-settings.png
```

조회가 끝나면 화면을 PNG로 저장하고 종료합니다. 일반 실행에서는 자동 조회/종료하지 않습니다.


### 저장소 구성

```text
src/main.rs                 Iced 화면과 작업 상태 관리
src/engine.rs               검색, 날짜/상세 조회, 다운로드 및 프로세스 관리
src/updater.rs              yt-dlp 업데이트 검증 및 이전 버전 복구
scripts/prepare-sidecar.ps1 다운로드 엔진 준비
scripts/package-rust.ps1    Windows 로컬 패키지 생성
Cargo.toml / Cargo.lock    Rust 의존성과 고정 버전
ffmpeg.exe / ffprobe.exe / deno.exe  sidecar 보조 도구
```

`tools/`, `target/`, `dist/` 및 개인 IDE 설정은 Git에 포함하지 않습니다.
패키지를 만들기 전에 실행 중인 해당 패키지 앱을 종료해야 실행 파일을 교체할 수 있습니다.
개발 도구가 없는 별도 Windows PC에서의 실행 검증은 아직 완료하지 않았습니다.
