use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub duration: String,
    pub url: String,
    pub thumbnail: Option<String>,
    pub heights: Vec<u32>,
    pub upload_date: Option<String>,
}

impl Video {
    fn parse(value: &Value) -> Option<Self> {
        let id = value["id"].as_str()?.to_owned();
        if id.len() != 11
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return None;
        }
        let seconds = value["duration"].as_f64().unwrap_or(0.0) as u64;
        let duration = if seconds == 0 {
            "시간 정보 없음".into()
        } else if seconds >= 3600 {
            format!(
                "{}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            )
        } else {
            format!("{}:{:02}", seconds / 60, seconds % 60)
        };
        let thumbnail = value["thumbnail"].as_str().map(str::to_owned).or_else(|| {
            value["thumbnails"]
                .as_array()?
                .iter()
                .rev()
                .find_map(|v| v["url"].as_str().map(str::to_owned))
        });
        let mut heights: Vec<u32> = value["formats"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["vcodec"].as_str().is_some_and(|v| v != "none"))
            .filter_map(|f| f["height"].as_u64().map(|h| h as u32))
            .collect();
        heights.sort_unstable_by(|a, b| b.cmp(a));
        heights.dedup();
        Some(Self {
            url: format!("https://www.youtube.com/watch?v={id}"),
            id,
            title: value["title"].as_str().unwrap_or("제목 없음").into(),
            channel: value["channel"]
                .as_str()
                .or(value["uploader"].as_str())
                .unwrap_or("채널 정보 없음")
                .into(),
            duration,
            thumbnail,
            heights,
            upload_date: value["upload_date"].as_str().and_then(format_date),
        })
    }
}

fn format_date(raw: &str) -> Option<String> {
    if raw.len() != 8 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let month: u32 = raw[4..6].parse().ok()?;
    let day: u32 = raw[6..8].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{}.{}.{}", &raw[..4], &raw[4..6], &raw[6..]))
}

#[derive(Clone, Debug)]
pub struct Engine {
    pub root: PathBuf,
    pub executable: PathBuf,
}

#[derive(Debug)]
pub enum Event {
    Search(u64, Result<Vec<Video>, String>),
    Date(u64, String, Option<String>),
    Detail(u64, u64, String, Result<Video, String>),
    Thumbnail(u64, String, Result<Vec<u8>, String>),
    Progress(f32, String),
    Download(Result<PathBuf, String>),
    UpdateProgress(String),
    Updated(Result<crate::updater::Snapshot, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp4,
    Mp3,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Mp4 => "동영상 · MP4",
            Self::Mp3 => "오디오 · MP3",
        })
    }
}

impl Engine {
    pub fn discover() -> Result<Self, String> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let mut candidates = Vec::new();
        if let Some(dir) = exe_dir {
            candidates.push(dir.join("tools"));
            candidates.push(dir);
        }
        if cfg!(debug_assertions) {
            candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools"));
        }
        let mut engine = candidates
            .into_iter()
            .find_map(|root| {
                let executable = root.join("yt-dlp.exe");
                executable.is_file().then_some(Self { root, executable })
            })
            .ok_or(
                "다운로드 엔진을 찾지 못했어요. 앱의 tools 폴더에 yt-dlp.exe를 준비해 주세요.",
            )?;
        if let Ok(updater) = crate::updater::Updater::new(&engine) {
            if let Ok(Some(path)) = updater.resolve_active() {
                engine.executable = path;
            }
        }
        Ok(engine)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        hide_console(&mut command);
        command.args([
            "--ignore-config",
            "--no-playlist",
            "--no-colors",
            "--encoding",
            "utf-8",
            "--socket-timeout",
            "15",
            "--retries",
            "1",
            "--extractor-retries",
            "1",
        ]);
        command.arg("--ffmpeg-location").arg(&self.root);
        if self.root.join("deno.exe").is_file() {
            command
                .arg("--js-runtimes")
                .arg(format!("deno:{}", self.root.join("deno.exe").display()));
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    pub fn search(
        &self,
        input: &str,
        offset: usize,
        cancel: Arc<AtomicBool>,
    ) -> Result<Vec<Video>, String> {
        let target = search_target(input)?;
        let target = if let Some(query) = target.strip_prefix("ytsearch20:") {
            format!("ytsearch{}:{query}", offset + 20)
        } else {
            target
        };

        let mut command = self.command();
        command.args(["--flat-playlist", "--dump-single-json", "--skip-download"]);
        command.args([
            "--playlist-start",
            &(offset + 1).to_string(),
            "--playlist-end",
            &(offset + 20).to_string(),
        ]);
        command.args(["--", &target]);
        let result = run_json(command, cancel)?;
        if let Some(entries) = result["entries"].as_array() {
            Ok(entries.iter().filter_map(Video::parse).collect())
        } else {
            Video::parse(&result)
                .map(|v| vec![v])
                .ok_or("영상 정보를 읽지 못했어요.".into())
        }
    }

    pub fn detail(&self, url: &str, cancel: Arc<AtomicBool>) -> Result<Video, String> {
        let mut command = self.command();
        command.args(["--dump-single-json", "--skip-download", "--", url]);
        Video::parse(&run_json(command, cancel)?).ok_or("영상 정보를 읽지 못했어요.".into())
    }

    pub fn download(
        &self,
        video: &Video,
        format: Format,
        quality: Option<u32>,
        destination: &Path,
        cancel: Arc<AtomicBool>,
        events: &mpsc::Sender<Event>,
    ) -> Result<PathBuf, String> {
        for file in ["ffmpeg.exe", "ffprobe.exe", "deno.exe"] {
            if !self.root.join(file).is_file() {
                return Err(format!("다운로드 엔진 구성 요소가 없어요: {file}"));
            }
        }
        std::fs::create_dir_all(destination)
            .map_err(|e| format!("저장 폴더를 만들 수 없어요: {e}"))?;
        let mut command = self.command();
        command
            .args([
                "--newline",
                "--progress",
                "--no-simulate",
                "--windows-filenames",
                "--progress-template",
                "download:PROGRESS:%(progress)j",
                "--progress-template",
                "postprocess:PROCESS:%(progress.status)s",
                "--print",
                "after_move:FILE:%(filepath)j",
                "--no-overwrites",
                "-P",
            ])
            .arg(destination);
        // 형식/화질마다 파일명을 분리해 이미 받은 다른 화질과 충돌하지 않게 한다.
        let label = if format == Format::Mp3 {
            "audio".into()
        } else {
            quality.map(|q| format!("{q}p")).unwrap_or("best".into())
        };
        command.args(["-o", &format!("%(title).160B [%(id)s]_{label}.%(ext)s")]);
        match format {
            Format::Mp3 => {
                command.args([
                    "-f",
                    "bestaudio/best",
                    "-x",
                    "--audio-format",
                    "mp3",
                    "--audio-quality",
                    "192K",
                ]);
            }
            Format::Mp4 => {
                let selector = quality
                    .map(|q| format!("bv[height<={q}]+ba/b[height<={q}]"))
                    .unwrap_or("bv+ba/b".into());
                command.args([
                    "-f",
                    &selector,
                    "--merge-output-format",
                    "mp4",
                    "--recode-video",
                    "mp4",
                ]);
            }
        }
        command.arg("--").arg(&video.url);
        let mut process = ManagedChild::spawn(&mut command)?;
        let stdout = process.child.stdout.take().ok_or("출력 스트림이 없어요.")?;
        let stderr = process.child.stderr.take().ok_or("오류 스트림이 없어요.")?;
        let errors = thread::spawn(move || read_bounded(stderr));
        let tx = events.clone();
        let output = thread::spawn(move || {
            let mut final_path = None;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(json) = line.strip_prefix("PROGRESS:") {
                    if let Ok(progress) = serde_json::from_str::<Value>(json) {
                        let total = progress["total_bytes"]
                            .as_f64()
                            .or(progress["total_bytes_estimate"].as_f64());
                        let received = progress["downloaded_bytes"].as_f64().unwrap_or(0.0);
                        let percent = total
                            .filter(|v| *v > 0.0)
                            .map(|v| (received / v * 100.0) as f32)
                            .unwrap_or(0.0);
                        let speed = progress["speed"]
                            .as_f64()
                            .map(|v| format!(" · {:.1} MB/s", v / 1_000_000.0))
                            .unwrap_or_default();
                        let label = if progress["status"] == "finished" {
                            "병합·변환 준비 중".into()
                        } else {
                            format!("다운로드 중 · {:.0}%{speed}", percent.min(100.0))
                        };
                        let _ = tx.send(Event::Progress(percent.clamp(0.0, 100.0), label));
                    }
                } else if line.starts_with("PROCESS:") {
                    let _ = tx.send(Event::Progress(100.0, "파일 병합·변환 중".into()));
                } else if let Some(json) = line.strip_prefix("FILE:") {
                    final_path = serde_json::from_str::<String>(json).ok().map(PathBuf::from);
                }
            }
            final_path
        });
        let status = process.wait(cancel, None);
        let final_path = output
            .join()
            .map_err(|_| "다운로드 출력 처리에 실패했어요.")?;
        let error = errors.join().unwrap_or_default();
        if !status? {
            return Err(friendly_error(&error));
        }
        final_path
            .filter(|path| path.is_file())
            .ok_or("작업은 끝났지만 최종 저장 파일을 확인하지 못했어요.".into())
    }
}

pub fn search_target(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("검색어 또는 YouTube 링크를 입력해 주세요.".into());
    }
    if input.len() > 2048 {
        return Err("검색어 또는 링크가 너무 길어요.".into());
    }
    let normalized = if input.starts_with("youtu.be/")
        || input.starts_with("youtube.com/")
        || input.starts_with("www.youtube.com/")
    {
        format!("https://{input}")
    } else {
        input.to_owned()
    };
    if normalized.contains("://") {
        let parsed =
            url::Url::parse(&normalized).map_err(|_| "올바른 영상 링크를 입력해 주세요.")?;
        if !["http", "https"].contains(&parsed.scheme())
            || !matches!(
                parsed.host_str(),
                Some(
                    "youtube.com"
                        | "www.youtube.com"
                        | "m.youtube.com"
                        | "music.youtube.com"
                        | "youtu.be"
                )
            )
        {
            return Err("YouTube 영상 링크만 지원해요.".into());
        }
        let id = if parsed.host_str() == Some("youtu.be") {
            parsed
                .path_segments()
                .and_then(|mut p| p.next())
                .map(str::to_owned)
        } else if parsed.path() == "/watch" {
            parsed
                .query_pairs()
                .find(|(key, _)| key == "v")
                .map(|(_, v)| v.into_owned())
        } else {
            let mut parts = parsed.path_segments().ok_or("영상 링크를 확인해 주세요.")?;
            match parts.next() {
                Some("shorts" | "live" | "embed") => parts.next().map(str::to_owned),
                _ => None,
            }
        };
        let id = id
            .filter(|id| {
                id.len() == 11
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
            .ok_or("재생목록이나 채널 대신 개별 영상 링크를 입력해 주세요.")?;
        return Ok(format!("https://www.youtube.com/watch?v={id}"));
    }
    Ok(format!("ytsearch20:{input}"))
}

fn run_json(mut command: Command, cancel: Arc<AtomicBool>) -> Result<Value, String> {
    let mut process = ManagedChild::spawn(&mut command)?;
    let stdout = process.child.stdout.take().ok_or("출력 스트림이 없어요.")?;
    let stderr = process.child.stderr.take().ok_or("오류 스트림이 없어요.")?;
    let out = thread::spawn(move || read_bounded(stdout));
    let err = thread::spawn(move || read_bounded(stderr));
    let status = process.wait(cancel, Some(Duration::from_secs(90)));
    let output = out.join().unwrap_or_default();
    let error = err.join().unwrap_or_default();
    if !status? {
        return Err(friendly_error(&error));
    }
    serde_json::from_slice(&output).map_err(|_| "엔진의 영상 정보 응답을 읽지 못했어요.".into())
}

fn read_bounded(mut reader: impl Read) -> Vec<u8> {
    let mut data = Vec::new();
    // 큰 메타데이터나 로그도 파이프는 계속 비워 프로세스 교착을 막는다.
    let mut buffer = [0; 8192];
    while let Ok(n) = reader.read(&mut buffer) {
        if n == 0 {
            break;
        }
        let count = n.min((32 * 1024 * 1024usize).saturating_sub(data.len()));
        data.extend_from_slice(&buffer[..count]);
    }
    data
}

fn friendly_error(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let line = text
        .lines()
        .rev()
        .find(|s| s.contains("ERROR:"))
        .unwrap_or("네트워크 연결과 영상 공개 상태를 확인해 주세요.");
    line.chars().take(500).collect()
}

pub fn thumbnail(url: &str) -> Result<Vec<u8>, String> {
    let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" {
        return Err("안전하지 않은 썸네일 주소".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .take(5_000_001);
    let mut data = Vec::new();
    response.read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() > 5_000_000 {
        return Err("썸네일 크기 제한 초과".into());
    }
    normalize_thumbnail(data)
}

fn normalize_thumbnail(data: Vec<u8>) -> Result<Vec<u8>, String> {
    // 4:3 이미지가 Cover 렌더링 중 카드 밖으로 그려지지 않도록 미리 16:9로 자른다.
    let mut reader = ::image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = ::image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    let normalized = decoded.resize_to_fill(640, 360, ::image::imageops::FilterType::Triangle);
    let mut output = std::io::Cursor::new(Vec::new());
    normalized
        .write_to(&mut output, ::image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}

pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}

struct ManagedChild {
    child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}

impl ManagedChild {
    fn spawn(command: &mut Command) -> Result<Self, String> {
        let mut child = command
            .spawn()
            .map_err(|e| format!("엔진 실행 실패: {e}"))?;
        #[cfg(windows)]
        unsafe {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if job.is_null()
                || SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                || AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0
            {
                let error = std::io::Error::last_os_error();
                let _ = child.kill();
                let _ = child.wait();
                if !job.is_null() {
                    CloseHandle(job);
                }
                return Err(format!("다운로드 프로세스 관리 초기화 실패: {error}"));
            }
            Ok(Self { child, job })
        }
        #[cfg(not(windows))]
        Ok(Self { child })
    }

    fn wait(&mut self, cancel: Arc<AtomicBool>, timeout: Option<Duration>) -> Result<bool, String> {
        let start = Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed)
                || timeout.is_some_and(|limit| start.elapsed() > limit)
            {
                self.terminate();
                let _ = self.child.wait();
                return Err(if cancel.load(Ordering::Relaxed) {
                    "취소됨".into()
                } else {
                    "조회 시간이 초과됐어요. 다시 시도하거나 영상 링크를 입력해 주세요.".into()
                });
            }
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status.success()),
                Ok(None) => thread::sleep(Duration::from_millis(80)),
                Err(e) => {
                    self.terminate();
                    return Err(e.to_string());
                }
            }
        }
    }

    fn terminate(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
        let _ = self.child.kill();
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.terminate();
        let _ = self.child.wait();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inputs_are_canonicalized_and_restricted() {
        assert_eq!(
            search_target(" 파이썬 강좌 ").unwrap(),
            "ytsearch20:파이썬 강좌"
        );
        assert_eq!(
            search_target("https://youtu.be/jNQXAC9IVRw?t=1").unwrap(),
            "https://www.youtube.com/watch?v=jNQXAC9IVRw"
        );
        assert!(search_target("https://youtube.com.evil.test/watch?v=jNQXAC9IVRw").is_err());
        assert!(search_target("https://youtube.com/playlist?list=x").is_err());
        assert!(search_target("file:///C:/secret").is_err());
        assert!(search_target(" ").is_err());
    }
    #[test]
    fn upload_dates_are_optional_and_formatted() {
        assert_eq!(format_date("20261021").as_deref(), Some("2026.10.21"));
        assert!(format_date("20261340").is_none());
        assert!(format_date("한글날짜").is_none());
        let v = Video::parse(&serde_json::json!({"id":"jNQXAC9IVRw", "upload_date":"20050423"}))
            .unwrap();
        assert_eq!(v.upload_date.as_deref(), Some("2005.04.23"));
    }

    #[test]
    #[ignore = "실제 YouTube 검색 페이지와 날짜 조회 검증"]
    fn live_search_pages_and_dates() {
        let engine = Engine::discover().unwrap();
        let first = engine
            .search("파이썬 강좌", 0, Arc::new(AtomicBool::new(false)))
            .unwrap();
        let second = engine
            .search("파이썬 강좌", 20, Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert_eq!(first.len(), 20);
        assert_eq!(second.len(), 20);
        assert!(second
            .iter()
            .any(|v| !first.iter().any(|old| old.id == v.id)));
        let detail = engine
            .detail(
                "https://www.youtube.com/watch?v=jNQXAC9IVRw",
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        // yt-dlp 날짜는 UTC 기준이며 YouTube의 현지 날짜와 하루 차이가 날 수 있다.
        assert_eq!(detail.upload_date.as_deref(), Some("2005.04.24"));
        println!(
            "검색 20개 + 추가 20개, 업로드 날짜 {:?}",
            detail.upload_date
        );
    }

    #[test]
    fn formats_and_sparse_search_results() {
        let v = Video::parse(&serde_json::json!({"id":"jNQXAC9IVRw","title":"한글","duration":3661,
            "formats":[{"vcodec":"avc1","height":720},{"vcodec":"avc1","height":1080},{"vcodec":"avc1","height":720},{"vcodec":"none"}]})).unwrap();
        assert_eq!(v.heights, vec![1080, 720]);
        assert_eq!(v.duration, "1:01:01");
        assert!(v.thumbnail.is_none());
    }

    #[test]
    fn four_by_three_thumbnails_fit_the_widescreen_slot() {
        let source = ::image::DynamicImage::new_rgb8(800, 600);
        let mut encoded = std::io::Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, ::image::ImageFormat::Png)
            .unwrap();
        let output = normalize_thumbnail(encoded.into_inner()).unwrap();
        let decoded = ::image::load_from_memory(&output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (640, 360));
        assert!(normalize_thumbnail(b"broken image".to_vec()).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn cancel_stops_a_running_process() {
        let mut command = Command::new("powershell.exe");
        hide_console(&mut command);
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 30",
        ]);
        let mut child = ManagedChild::spawn(&mut command).unwrap();
        let start = Instant::now();
        assert_eq!(
            child
                .wait(Arc::new(AtomicBool::new(true)), None)
                .unwrap_err(),
            "취소됨"
        );
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(child.child.try_wait().unwrap().is_some());
    }

    #[test]
    #[ignore = "실제 YouTube 네트워크와 sidecar가 필요한 통합 검증"]
    fn live_search_and_download() {
        let engine = Engine::discover().unwrap();
        let cancel = || Arc::new(AtomicBool::new(false));
        let results = engine.search("파이썬 강좌", 0, cancel()).unwrap();
        assert!(!results.is_empty());
        let thumbnail_bytes = thumbnail(results[0].thumbnail.as_ref().unwrap()).unwrap();
        assert!(::image::load_from_memory(&thumbnail_bytes).is_ok());
        let video = engine
            .detail("https://www.youtube.com/watch?v=jNQXAC9IVRw", cancel())
            .unwrap();
        assert!(!video.heights.is_empty());
        let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("검증 다운로드");
        let (tx, rx) = mpsc::channel();
        for format in [Format::Mp4, Format::Mp3] {
            let output = engine
                .download(&video, format, Some(240), &output_dir, cancel(), &tx)
                .unwrap();
            assert_eq!(
                output.extension().unwrap(),
                if format == Format::Mp4 { "mp4" } else { "mp3" }
            );
            assert!(std::fs::metadata(&output).unwrap().len() > 1000);
            let decoded = Command::new(engine.root.join("ffprobe.exe"))
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-of",
                    "json",
                ])
                .arg(&output)
                .output()
                .unwrap();
            assert!(decoded.status.success());
            let probe: Value = serde_json::from_slice(&decoded.stdout).unwrap();
            assert!(
                probe["format"]["duration"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    > 1.0
            );
            println!("검증 성공: {}", output.display());
        }
        assert!(rx.try_iter().any(|e| matches!(e, Event::Progress(_, _))));
    }
}
