#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod updater;

use engine::{Engine, Event, Format, Video};
use iced::{
    alignment,
    widget::{
        button, column, container, image, pick_list, progress_bar, row,
        rule::horizontal as horizontal_rule, scrollable, text, text_input, Space,
    },
    window, Border, Color, Element, Font, Length, Subscription, Task, Theme,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

const KOREAN_FONT: Font = Font::with_name("Malgun Gothic");
const ICON_FONT: Font = Font::with_name("Segoe MDL2 Assets");

fn main() -> iced::Result {
    let mut app = iced::application(App::new, App::update, App::view)
        .title("YouTube Downloader")
        .subscription(App::subscription)
        .theme(App::theme)
        .default_font(KOREAN_FONT)
        .window(window::Settings {
            size: iced::Size::new(1080.0, 800.0),
            min_size: Some(iced::Size::new(880.0, 680.0)),
            ..Default::default()
        });
    // Windows가 제공하는 글꼴을 읽으므로 글꼴 파일을 재배포하지 않는다.
    let windows = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or(PathBuf::from("C:\\Windows"));
    for name in ["malgun.ttf", "segmdl2.ttf"] {
        if let Ok(bytes) = std::fs::read(windows.join("Fonts").join(name)) {
            app = app.font(bytes);
        }
    }
    app.run()
}

#[derive(Debug, Clone)]
enum Message {
    Input(String),
    Search,
    More,
    Select(usize),
    Tick,
    Format(Format),
    Quality(String),
    Download,
    Cancel,
    OpenVideo,
    OpenFolder,
    ToggleTheme,
    Settings,
    CheckUpdate,
    InstallUpdate,
    Rollback,
    Screenshot(window::Screenshot),
}

struct App {
    engine: Result<Engine, String>,
    query: String,
    videos: Vec<Video>,
    selected: Option<usize>,
    detail_ready: Option<String>,
    thumbnails: HashMap<String, image::Handle>,
    failed_thumbnails: Vec<String>,
    format: Format,
    quality: String,
    qualities: Vec<String>,
    directory: PathBuf,
    searching: bool,
    search_query: String,
    search_offset: usize,
    has_more: bool,
    date_queue: VecDeque<(String, String)>,
    date_active: usize,
    date_done: HashSet<String>,
    date_cache: HashMap<String, String>,
    date_cancel: Arc<AtomicBool>,
    loading_detail: bool,
    downloading: bool,
    cancelling: bool,
    error: Option<String>,
    status: String,
    progress: f32,
    completed: Option<PathBuf>,
    generation: u64,
    detail_request: u64,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    search_cancel: Arc<AtomicBool>,
    detail_cancel: Arc<AtomicBool>,
    download_cancel: Arc<AtomicBool>,
    dark: bool,
    settings_open: bool,
    update_busy: bool,
    update_operation: UpdateOperation,
    update_status: String,
    update_error: Option<String>,
    engine_snapshot: Option<updater::Snapshot>,
    latest_release: Option<updater::Release>,
    // 로컬 QA: 실제 Iced 화면을 PNG로 저장하고 종료한다.
    screenshot: Option<PathBuf>,
    screenshot_started: bool,
    started: Instant,
    pending_query: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum UpdateOperation {
    Inspect,
    Check,
    Install,
    Rollback,
}

impl App {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let directory = directories::UserDirs::new()
            .and_then(|dirs| dirs.download_dir().map(PathBuf::from))
            .unwrap_or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join("Downloads")
            });
        let args: Vec<String> = std::env::args().collect();
        let arg = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        Self {
            engine: Engine::discover(),
            query: String::new(),
            videos: vec![],
            selected: None,
            detail_ready: None,
            thumbnails: HashMap::new(),
            failed_thumbnails: vec![],
            format: Format::Mp4,
            quality: "최고 화질".into(),
            qualities: vec!["최고 화질".into()],
            directory,
            searching: false,
            search_query: String::new(),
            search_offset: 0,
            has_more: false,
            date_queue: VecDeque::new(),
            date_active: 0,
            date_done: HashSet::new(),
            date_cache: HashMap::new(),
            date_cancel: Arc::new(AtomicBool::new(false)),
            loading_detail: false,
            downloading: false,
            cancelling: false,
            error: None,
            status: "영상을 검색하거나 YouTube 링크를 붙여 넣어 주세요.".into(),
            progress: 0.0,
            completed: None,
            generation: 0,
            detail_request: 0,
            tx,
            rx,
            search_cancel: Arc::new(AtomicBool::new(false)),
            detail_cancel: Arc::new(AtomicBool::new(false)),
            download_cancel: Arc::new(AtomicBool::new(false)),
            dark: false,
            settings_open: args.iter().any(|a| a == "--settings"),
            update_busy: false,
            update_operation: UpdateOperation::Inspect,
            update_status: "업데이트 확인을 눌러 현재 버전과 최신 버전을 확인하세요.".into(),
            update_error: None,
            engine_snapshot: None,
            latest_release: None,
            screenshot: arg("--screenshot").map(PathBuf::from),
            screenshot_started: false,
            started: Instant::now(),
            pending_query: arg("--query"),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let loading_images = self.videos.iter().any(|video| {
            !self.thumbnails.contains_key(&video.id) && !self.failed_thumbnails.contains(&video.id)
        });
        if self.date_active > 0
            || !self.date_queue.is_empty()
            || self.searching
            || self.loading_detail
            || self.downloading
            || self.update_busy
            || (self.settings_open && self.engine_snapshot.is_none() && self.update_error.is_none())
            || loading_images
            || self.pending_query.is_some()
            || (self.screenshot.is_some() && !self.screenshot_started)
        {
            iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Input(value) => self.query = value,
            Message::Search if !self.searching && !self.downloading && !self.update_busy => {
                self.search()
            }
            Message::More
                if self.has_more && !self.searching && !self.downloading && !self.update_busy =>
            {
                self.load_page()
            }
            Message::Select(index) if !self.downloading && !self.update_busy => self.select(index),
            Message::Tick => {
                if self.settings_open
                    && self.engine_snapshot.is_none()
                    && self.update_error.is_none()
                    && !self.engine_in_use()
                    && !self.update_busy
                {
                    self.start_update(UpdateOperation::Inspect);
                }
                if let Some(query) = self.pending_query.take() {
                    self.query = query;
                    self.search();
                }
                while let Ok(event) = self.rx.try_recv() {
                    self.handle_event(event);
                }
                if self.screenshot.is_some()
                    && !self.screenshot_started
                    && ((self.started.elapsed() > Duration::from_secs(4)
                        && !self.searching
                        && !self.loading_detail
                        && !self.update_busy
                        && self.videos.iter().all(|v| {
                            self.thumbnails.contains_key(&v.id)
                                || self.failed_thumbnails.contains(&v.id)
                        }))
                        || self.started.elapsed() > Duration::from_secs(110))
                {
                    self.screenshot_started = true;
                    return window::latest()
                        .and_then(window::screenshot)
                        .map(Message::Screenshot);
                }
            }
            Message::Format(format) if !self.downloading => self.format = format,
            Message::Quality(quality) if !self.downloading => self.quality = quality,
            Message::Download if !self.downloading && !self.update_busy => self.download(),
            Message::Cancel if self.downloading => {
                self.cancelling = true;
                self.download_cancel.store(true, Ordering::Relaxed);
                self.status = "취소 중 · 다운로드와 변환 작업을 정리하고 있어요.".into();
            }
            Message::OpenVideo => {
                if let Some(video) = self.selected.and_then(|i| self.videos.get(i)) {
                    if let Err(error) = open_external(&video.url) {
                        self.error = Some(error);
                    }
                }
            }
            Message::OpenFolder => {
                if let Some(folder) = self.completed.as_ref().and_then(|p| p.parent()) {
                    if !folder.is_dir() {
                        self.error = Some("저장 폴더를 찾을 수 없어요.".into());
                    } else {
                        let mut command = Command::new("explorer.exe");
                        engine::hide_console(&mut command);
                        if let Err(e) = command.arg(folder).spawn() {
                            self.error = Some(format!("폴더를 열지 못했어요: {e}"));
                        }
                    }
                }
            }
            Message::ToggleTheme => self.dark = !self.dark,
            Message::Settings => {
                self.settings_open = !self.settings_open;
                if self.settings_open && !self.engine_in_use() && !self.update_busy {
                    self.start_update(UpdateOperation::Inspect);
                }
            }
            Message::CheckUpdate if !self.engine_in_use() && !self.update_busy => {
                self.start_update(UpdateOperation::Check)
            }
            Message::InstallUpdate if !self.engine_in_use() && !self.update_busy => {
                self.start_update(UpdateOperation::Install)
            }
            Message::Rollback if !self.engine_in_use() && !self.update_busy => {
                self.start_update(UpdateOperation::Rollback)
            }
            Message::Screenshot(capture) => {
                if let Some(path) = &self.screenshot {
                    if let Err(e) = ::image::save_buffer(
                        path,
                        capture.rgba.as_ref(),
                        capture.size.width,
                        capture.size.height,
                        ::image::ColorType::Rgba8,
                    ) {
                        eprintln!("화면 캡처 실패: {e}");
                    }
                }
                return iced::exit();
            }
            _ => {}
        }
        Task::none()
    }

    fn search(&mut self) {
        if let Err(error) = engine::search_target(&self.query) {
            self.error = Some(error);
            return;
        }
        let _engine = match &self.engine {
            Ok(engine) => engine.clone(),
            Err(e) => {
                self.error = Some(e.clone());
                return;
            }
        };
        self.search_cancel.store(true, Ordering::Relaxed);
        self.detail_cancel.store(true, Ordering::Relaxed);
        self.search_cancel = Arc::new(AtomicBool::new(false));
        self.date_cancel.store(true, Ordering::Relaxed);
        self.date_cancel = Arc::new(AtomicBool::new(false));
        self.date_queue.clear();
        self.date_active = 0;
        self.date_done.clear();
        self.search_query = self.query.clone();
        self.search_offset = 0;
        self.has_more =
            engine::search_target(&self.query).is_ok_and(|v| v.starts_with("ytsearch20:"));
        self.generation += 1;
        self.searching = true;
        self.loading_detail = false;
        self.videos.clear();
        self.thumbnails.clear();
        self.failed_thumbnails.clear();
        self.selected = None;
        self.detail_ready = None;
        self.error = None;
        self.load_page();
    }

    fn load_page(&mut self) {
        let Ok(engine) = self.engine.clone() else {
            return;
        };
        self.searching = true;
        self.error = None;
        let (tx, generation, query, offset, cancel) = (
            self.tx.clone(),
            self.generation,
            self.search_query.clone(),
            self.search_offset,
            self.search_cancel.clone(),
        );
        std::thread::spawn(move || {
            let _ = tx.send(Event::Search(
                generation,
                engine.search(&query, offset, cancel),
            ));
        });
    }

    fn fill_dates(&mut self) {
        while self.date_active < 2 {
            let Some((id, url)) = self.date_queue.pop_front() else {
                break;
            };
            if self.date_done.contains(&id) {
                continue;
            }
            let Ok(engine) = self.engine.clone() else {
                self.date_done.insert(id);
                continue;
            };
            self.date_active += 1;
            let (tx, generation, cancel) =
                (self.tx.clone(), self.generation, self.date_cancel.clone());
            std::thread::spawn(move || {
                let date = engine.detail(&url, cancel).ok().and_then(|v| v.upload_date);
                let _ = tx.send(Event::Date(generation, id, date));
            });
        }
    }

    fn select(&mut self, index: usize) {
        let Some(video) = self.videos.get(index).cloned() else {
            return;
        };
        self.detail_cancel.store(true, Ordering::Relaxed);
        self.detail_cancel = Arc::new(AtomicBool::new(false));
        self.detail_request += 1;
        self.selected = Some(index);
        self.detail_ready = None;
        self.loading_detail = true;
        self.quality = "최고 화질".into();
        self.qualities = vec!["최고 화질".into()];
        self.error = None;
        if let Ok(engine) = &self.engine {
            let (engine, tx, generation, request, cancel) = (
                engine.clone(),
                self.tx.clone(),
                self.generation,
                self.detail_request,
                self.detail_cancel.clone(),
            );
            std::thread::spawn(move || {
                let _ = tx.send(Event::Detail(
                    generation,
                    request,
                    video.id,
                    engine.detail(&video.url, cancel),
                ));
            });
        } else {
            self.loading_detail = false;
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::UpdateProgress(message) => self.update_status = message,
            Event::Updated(result) => {
                self.update_busy = false;
                match result {
                    Ok(snapshot) => {
                        if let Ok(engine) = &mut self.engine {
                            engine.executable = snapshot.executable.clone();
                        }
                        if let Some(latest) = &snapshot.latest {
                            self.latest_release = Some(latest.clone());
                        }
                        self.update_status = match self.update_operation {
                            UpdateOperation::Inspect => "설치된 엔진 정보를 확인했어요.".into(),
                            UpdateOperation::Check
                                if self
                                    .latest_release
                                    .as_ref()
                                    .is_some_and(|v| v.version > snapshot.version) =>
                            {
                                "새 버전을 설치할 수 있어요.".into()
                            }
                            UpdateOperation::Check => "설치할 새 정식 버전이 없어요.".into(),
                            UpdateOperation::Install => {
                                "업데이트 완료 · 다음 작업부터 새 엔진을 사용해요.".into()
                            }
                            UpdateOperation::Rollback => {
                                "이전 버전으로 전환했어요. 다음 작업부터 적용돼요.".into()
                            }
                        };
                        self.engine_snapshot = Some(snapshot);
                    }
                    Err(error) => {
                        self.update_error = Some(error);
                        self.update_status =
                            "작업에 실패했어요. 현재 엔진은 변경하지 않았어요.".into();
                    }
                }
            }
            Event::Search(generation, result) if generation == self.generation => {
                self.searching = false;
                match result {
                    Ok(videos) => {
                        let first_page = self.search_offset == 0;
                        self.has_more = self.has_more && videos.len() == 20;
                        self.search_offset += 20;
                        for mut video in videos {
                            if self.videos.iter().any(|v| v.id == video.id) {
                                continue;
                            }
                            video.upload_date = video
                                .upload_date
                                .or_else(|| self.date_cache.get(&video.id).cloned());
                            if let Some(date) = &video.upload_date {
                                self.date_cache.insert(video.id.clone(), date.clone());
                                self.date_done.insert(video.id.clone());
                            } else {
                                self.date_queue
                                    .push_back((video.id.clone(), video.url.clone()));
                            }
                            if let Some(url) = &video.thumbnail {
                                let (tx, id, url) =
                                    (self.tx.clone(), video.id.clone(), url.clone());
                                std::thread::spawn(move || {
                                    let _ = tx.send(Event::Thumbnail(
                                        generation,
                                        id,
                                        engine::thumbnail(&url),
                                    ));
                                });
                            } else {
                                self.failed_thumbnails.push(video.id.clone());
                            }
                            self.videos.push(video);
                        }
                        self.fill_dates();
                        if first_page && !self.videos.is_empty() {
                            self.select(0);
                        } else if self.videos.is_empty() {
                            self.error = Some(
                                "검색 결과가 없어요. 다른 검색어나 영상 링크를 입력해 주세요."
                                    .into(),
                            );
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Event::Date(generation, id, date) if generation == self.generation => {
                self.date_active = self.date_active.saturating_sub(1);
                self.date_done.insert(id.clone());
                if let Some(date) = date {
                    self.date_cache.insert(id.clone(), date.clone());
                    if let Some(video) = self.videos.iter_mut().find(|v| v.id == id) {
                        video.upload_date = Some(date);
                    }
                }
                self.fill_dates();
            }
            Event::Detail(generation, request, id, result)
                if generation == self.generation
                    && request == self.detail_request
                    && self
                        .selected
                        .and_then(|i| self.videos.get(i))
                        .is_some_and(|v| v.id == id) =>
            {
                self.loading_detail = false;
                match result {
                    Ok(mut video) => {
                        video.upload_date = video
                            .upload_date
                            .or_else(|| self.date_cache.get(&id).cloned());
                        if let Some(date) = &video.upload_date {
                            self.date_cache.insert(id.clone(), date.clone());
                            self.date_done.insert(id.clone());
                        }
                        self.qualities = std::iter::once("최고 화질".into())
                            .chain(video.heights.iter().map(|h| format!("{h}p")))
                            .collect();
                        self.detail_ready = Some(id);
                        if let Some(index) = self.selected {
                            self.videos[index] = video;
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Event::Thumbnail(generation, id, result) if generation == self.generation => {
                match result {
                    Ok(bytes) => {
                        self.thumbnails.insert(id, image::Handle::from_bytes(bytes));
                    }
                    Err(_) => self.failed_thumbnails.push(id),
                }
            }
            Event::Progress(progress, label) if self.downloading && !self.cancelling => {
                self.progress = progress;
                self.status = label;
            }
            Event::Download(result) => {
                self.downloading = false;
                self.cancelling = false;
                match result {
                    Ok(path) => {
                        self.completed = Some(path);
                        self.status = "다운로드 완료".into();
                        self.progress = 100.0;
                    }
                    Err(error) if error == "취소됨" => {
                        self.status = "다운로드가 취소됐어요.".into();
                        self.progress = 0.0;
                    }
                    Err(error) => {
                        self.status = "다운로드 실패 · 다시 시도할 수 있어요.".into();
                        self.error = Some(error);
                    }
                }
            }
            _ => {}
        }
    }

    fn download(&mut self) {
        let Some(video) = self.selected.and_then(|i| self.videos.get(i)).cloned() else {
            return;
        };
        if self.detail_ready.as_deref() != Some(video.id.as_str()) {
            return;
        }
        let Ok(engine) = &self.engine else {
            return;
        };
        let (engine, format, directory, tx) = (
            engine.clone(),
            self.format,
            self.directory.clone(),
            self.tx.clone(),
        );
        let quality = self.quality.strip_suffix('p').and_then(|s| s.parse().ok());
        self.download_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.download_cancel.clone();
        self.downloading = true;
        self.progress = 0.0;
        self.completed = None;
        self.error = None;
        self.status = "다운로드 준비 중".into();
        std::thread::spawn(move || {
            let result = engine.download(&video, format, quality, &directory, cancel, &tx);
            let _ = tx.send(Event::Download(result));
        });
    }

    fn engine_in_use(&self) -> bool {
        self.searching
            || self.loading_detail
            || self.downloading
            || self.date_active > 0
            || !self.date_queue.is_empty()
    }

    fn start_update(&mut self, operation: UpdateOperation) {
        let manager = match self
            .engine
            .as_ref()
            .map_err(Clone::clone)
            .and_then(updater::Updater::new)
        {
            Ok(manager) => manager,
            Err(error) => {
                self.update_error = Some(error);
                return;
            }
        };
        let snapshot = self.engine_snapshot.clone();
        let release = self.latest_release.clone();
        if matches!(
            operation,
            UpdateOperation::Install | UpdateOperation::Rollback
        ) && snapshot.is_none()
        {
            return;
        }
        if operation == UpdateOperation::Install && release.is_none() {
            return;
        }
        self.update_busy = true;
        self.update_error = None;
        self.update_operation = operation;
        self.update_status = match operation {
            UpdateOperation::Inspect => "현재 엔진을 확인하고 있어요…",
            UpdateOperation::Check => "공식 최신 버전을 확인하고 있어요…",
            UpdateOperation::Install => "업데이트를 준비하고 있어요…",
            UpdateOperation::Rollback => "이전 버전을 검증하고 있어요…",
        }
        .into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = match operation {
                UpdateOperation::Inspect => manager.snapshot(),
                UpdateOperation::Check => manager.check(),
                UpdateOperation::Install => {
                    manager.install(&release.unwrap(), &snapshot.unwrap().hash, |message| {
                        let _ = tx.send(Event::UpdateProgress(message.into()));
                    })
                }
                UpdateOperation::Rollback => manager.rollback(&snapshot.unwrap().hash),
            };
            let _ = tx.send(Event::Updated(result));
        });
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let idle = !self.engine_in_use() && !self.update_busy;
        let current = self
            .engine_snapshot
            .as_ref()
            .map(|s| s.version.as_str())
            .unwrap_or("확인 전");
        let previous = self
            .engine_snapshot
            .as_ref()
            .and_then(|s| s.previous.as_deref());
        let latest = self
            .latest_release
            .as_ref()
            .map(|v| v.version.as_str())
            .unwrap_or("확인 전");
        let can_install = idle
            && self.engine_snapshot.as_ref().is_some_and(|s| {
                self.latest_release
                    .as_ref()
                    .is_some_and(|r| r.version > s.version)
            });
        let mut content = column![
            text("설정").size(23),
            text("다운로드 엔진").size(18),
            text("yt-dlp의 공식 정식 릴리스를 확인하고 설치할 수 있어요.")
                .size(14)
                .color(self.muted()),
            horizontal_rule(1),
            row![text("현재 버전").width(140), text(current)].spacing(16),
            row![text("최신 정식 버전").width(140), text(latest)].spacing(16),
            row![
                text("되돌릴 버전").width(140),
                text(previous.unwrap_or("없음"))
            ]
            .spacing(16),
            row![
                button(text("업데이트 확인").size(14))
                    .padding(11)
                    .style(button::secondary)
                    .on_press_maybe(idle.then_some(Message::CheckUpdate)),
                button(text("새 버전 설치").size(14))
                    .padding(11)
                    .on_press_maybe(can_install.then_some(Message::InstallUpdate)),
                button(text("이전 버전으로 되돌리기").size(14))
                    .padding(11)
                    .style(button::secondary)
                    .on_press_maybe((idle && previous.is_some()).then_some(Message::Rollback))
            ]
            .spacing(10),
            text(&self.update_status).size(14),
        ]
        .spacing(17);
        if self.engine_in_use() {
            content = content.push(
                text("검색·영상 정보 조회·다운로드가 끝나면 엔진을 변경할 수 있어요.")
                    .size(13)
                    .color(self.muted()),
            );
        }
        if let Some(error) = &self.update_error {
            content = content.push(text(error).size(13).color(Color::from_rgb8(211, 85, 85)));
        }
        content = content.push(horizontal_rule(1)).push(
            text("새 파일의 SHA-256과 실행 버전을 검증한 뒤 적용해요.\n이전 엔진은 보관되며, 앱을 다시 실행해도 선택한 버전을 사용해요.\nFFmpeg와 Deno는 이번 업데이트에 포함되지 않아요.").size(13).color(self.muted())
        );
        scrollable(
            container(content)
                .padding(20)
                .width(Length::Fill)
                .style(|theme: &Theme| {
                    let mut style = container::rounded_box(theme);
                    style.background = Some(
                        if self.dark {
                            Color::from_rgb8(29, 32, 38)
                        } else {
                            Color::WHITE
                        }
                        .into(),
                    );
                    style.border = Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: if self.dark {
                            Color::from_rgb8(52, 57, 68)
                        } else {
                            Color::from_rgb8(226, 230, 236)
                        },
                    };
                    style
                }),
        )
        .height(Length::Fill)
        .into()
    }

    fn thumbnail_view<'a>(
        &'a self,
        video: &Video,
        width: f32,
        height: f32,
    ) -> Element<'a, Message> {
        if let Some(handle) = self.thumbnails.get(&video.id) {
            image(handle.clone())
                .width(width)
                .height(height)
                .content_fit(iced::ContentFit::Cover)
                .into()
        } else {
            container(
                text(if self.failed_thumbnails.contains(&video.id) {
                    "이미지 없음"
                } else {
                    "불러오는 중"
                })
                .size(12)
                .color(self.muted()),
            )
            .center_x(width)
            .center_y(height)
            .style(container::rounded_box)
            .into()
        }
    }

    fn muted(&self) -> Color {
        if self.dark {
            Color::from_rgb8(163, 173, 190)
        } else {
            Color::from_rgb8(100, 111, 130)
        }
    }

    fn theme(&self) -> Theme {
        let base = if self.dark { Theme::Dark } else { Theme::Light };
        Theme::custom(
            "Downloader",
            iced::theme::Palette {
                background: if self.dark {
                    Color::from_rgb8(21, 23, 27)
                } else {
                    Color::from_rgb8(247, 248, 250)
                },
                text: if self.dark {
                    Color::from_rgb8(237, 240, 245)
                } else {
                    Color::from_rgb8(29, 38, 53)
                },
                primary: if self.dark {
                    Color::from_rgb8(155, 179, 255)
                } else {
                    Color::from_rgb8(49, 89, 217)
                },
                ..base.palette()
            },
        )
    }

    fn view(&self) -> Element<'_, Message> {
        let accent = if self.dark {
            Color::from_rgb8(155, 179, 255)
        } else {
            Color::from_rgb8(49, 89, 217)
        };
        let header = row![
            text("↓").size(25).color(accent),
            text("YouTube Downloader").size(18),
            Space::new().width(Length::Fill),
            button(
                text(if self.settings_open {
                    "검색으로 돌아가기"
                } else {
                    "설정"
                })
                .size(13)
            )
            .style(button::secondary)
            .on_press(Message::Settings),
            button(
                text(if self.dark {
                    "라이트 모드"
                } else {
                    "다크 모드"
                })
                .size(12)
            )
            .style(button::text)
            .on_press(Message::ToggleTheme)
        ]
        .spacing(12)
        .align_y(alignment::Vertical::Center);
        let input = text_input("검색어 또는 YouTube 링크 입력", &self.query)
            .on_input(Message::Input)
            .padding(13)
            .size(16);
        let input = if !self.searching && !self.downloading && !self.update_busy {
            input.on_submit(Message::Search)
        } else {
            input
        };
        let search = button(
            text(if self.searching {
                "검색 중…"
            } else {
                "검색"
            })
            .size(15),
        )
        .padding([13, 24])
        .on_press_maybe(
            (!self.searching && !self.downloading && !self.update_busy).then_some(Message::Search),
        );
        let searchbar = row![input, search].spacing(10);

        let mut list = column![].spacing(9);
        for (index, video) in self.videos.iter().enumerate() {
            let selected = self.selected == Some(index);
            let title: String = video.title.chars().take(64).collect();
            let item = row![
                self.thumbnail_view(video, 144.0, 81.0),
                column![
                    text(title).size(15).width(Length::Fill),
                    text(&video.channel).size(12).color(self.muted()),
                    text(format!(
                        "{} · {}",
                        video.duration,
                        video.upload_date.as_deref().unwrap_or(
                            if self.date_done.contains(&video.id) {
                                "날짜 확인 불가"
                            } else {
                                "날짜 확인 중…"
                            }
                        )
                    ))
                    .size(12)
                    .color(self.muted())
                ]
                .spacing(5)
                .width(Length::Fill)
            ]
            .spacing(13)
            .align_y(alignment::Vertical::Center);
            let dark = self.dark;
            list = list.push(
                button(item)
                    .width(Length::Fill)
                    .padding(10)
                    .on_press_maybe(
                        (!self.downloading && !self.update_busy).then_some(Message::Select(index)),
                    )
                    .style(move |theme: &Theme, state| {
                        let mut style = button::secondary(theme, state);
                        let p = theme.extended_palette();
                        style.background = Some(
                            if selected {
                                if dark {
                                    Color::from_rgb8(39, 47, 70)
                                } else {
                                    Color::from_rgb8(238, 242, 255)
                                }
                            } else if dark {
                                Color::from_rgb8(29, 32, 38)
                            } else {
                                Color::WHITE
                            }
                            .into(),
                        );
                        style.text_color = p.background.base.text;
                        style.border = Border {
                            radius: 9.0.into(),
                            width: if selected { 1.5 } else { 1.0 },
                            color: if selected {
                                p.primary.strong.color
                            } else {
                                p.background.strong.color
                            },
                        };
                        if state == button::Status::Hovered && !selected {
                            style.background = Some(p.background.weak.color.into());
                        }
                        style
                    }),
            );
        }
        let results: Element<'_, Message> = if self.videos.is_empty() {
            container(
                column![
                    text(if self.searching {
                        "YouTube에서 영상을 찾고 있어요"
                    } else {
                        "어떤 영상을 저장할까요?"
                    })
                    .size(20),
                    text(if self.searching {
                        "썸네일과 영상 정보를 가져오는 중이에요."
                    } else {
                        "검색 결과를 선택하면 오른쪽에서\n화질과 저장 형식을 고를 수 있어요."
                    })
                    .size(14)
                    .color(self.muted())
                ]
                .spacing(12)
                .align_x(alignment::Horizontal::Center),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else {
            scrollable(container(list).padding(iced::Padding::default().right(14)))
                .height(Length::Fill)
                .into()
        };
        let left = column![
            row![
                text("검색 결과").size(16),
                text(format!("{}개", self.videos.len()))
                    .size(13)
                    .color(self.muted())
            ]
            .spacing(8),
            results,
            button(
                text(if self.searching && !self.videos.is_empty() {
                    "추가 검색 중…"
                } else if self.has_more {
                    "더 보기 · 20개"
                } else {
                    "추가 결과 없음"
                })
                .size(13)
            )
            .width(Length::Fill)
            .on_press_maybe(
                (self.has_more && !self.searching && !self.downloading && !self.update_busy)
                    .then_some(Message::More)
            )
        ]
        .spacing(14)
        .width(Length::Fill)
        .height(Length::Fill);

        let detail: Element<'_, Message> =
            if let Some(video) = self.selected.and_then(|i| self.videos.get(i)) {
                let title = text(&video.title).size(16).width(Length::Fill);
                let formats = [Format::Mp4, Format::Mp3];
                let format_picker = pick_list(formats, Some(self.format), Message::Format)
                    .padding(9)
                    .text_size(13)
                    .width(Length::Fill);
                let quality: Element<'_, Message> = if self.format == Format::Mp3 {
                    container(text("192 kbps").size(14))
                        .padding(10)
                        .style(container::rounded_box)
                        .width(Length::Fill)
                        .into()
                } else {
                    pick_list(
                        self.qualities.clone(),
                        Some(self.quality.clone()),
                        Message::Quality,
                    )
                    .padding(9)
                    .text_size(13)
                    .width(Length::Fill)
                    .into()
                };
                let options: Element<'_, Message> = if self.downloading {
                    column![text(format!(
                        "{} · {}",
                        self.format,
                        if self.format == Format::Mp3 {
                            "192 kbps"
                        } else {
                            &self.quality
                        }
                    ))
                    .size(14)]
                    .into()
                } else {
                    row![
                        column![
                            text("저장 형식").size(12).color(self.muted()),
                            format_picker
                        ]
                        .spacing(6)
                        .width(Length::FillPortion(3)),
                        column![
                            text(if self.format == Format::Mp3 {
                                "음질"
                            } else {
                                "화질"
                            })
                            .size(12)
                            .color(self.muted()),
                            quality
                        ]
                        .spacing(6)
                        .width(Length::FillPortion(2))
                    ]
                    .spacing(8)
                    .into()
                };
                let available = self.detail_ready.as_deref() == Some(&video.id)
                    && !self.downloading
                    && !self.update_busy;
                let path = self.directory.to_string_lossy().into_owned();
                let metadata = column![
                    self.thumbnail_view(video, 286.0, 161.0),
                    title,
                    text(format!("{} · {}", video.channel, video.duration))
                        .size(12)
                        .color(self.muted()),
                ]
                .spacing(9);
                let content = column![
                    row![
                        text("선택한 영상").size(12).color(self.muted()),
                        Space::new().width(Length::Fill),
                        button(text("YouTube ↗").size(12))
                            .style(button::text)
                            .padding(0)
                            .on_press(Message::OpenVideo)
                    ],
                    scrollable(container(metadata).padding(iced::Padding::default().right(14)))
                        .height(Length::Fill),
                    horizontal_rule(1),
                    text(if self.loading_detail {
                        "지원 화질을 확인하고 있어요…"
                    } else if self.detail_ready.is_none() {
                        "영상 정보를 불러오지 못했어요. 다시 선택해 주세요."
                    } else {
                        "다운로드 설정"
                    })
                    .size(13)
                    .color(self.muted()),
                    options,
                    text(format!("저장 위치 · {path}"))
                        .size(11)
                        .color(self.muted()),
                    button(row![text("↓").size(19), text("다운로드").size(16)].spacing(8))
                        .width(Length::Fill)
                        .padding(12)
                        .on_press_maybe(available.then_some(Message::Download))
                ]
                .spacing(10)
                .height(Length::Fill);
                content.into()
            } else {
                container(
                    column![
                        text("선택한 영상").size(14),
                        text("왼쪽 목록에서 영상을 선택해 주세요.")
                            .size(13)
                            .color(self.muted())
                    ]
                    .spacing(14),
                )
                .width(Length::Fill)
                .into()
            };
        let right = container(detail)
            .padding(18)
            .width(340)
            .height(Length::Fill)
            .style(|theme: &Theme| {
                let mut style = container::rounded_box(theme);
                style.background = Some(
                    if self.dark {
                        Color::from_rgb8(29, 32, 38)
                    } else {
                        Color::WHITE
                    }
                    .into(),
                );
                style.border = Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: if self.dark {
                        Color::from_rgb8(52, 57, 68)
                    } else {
                        Color::from_rgb8(226, 230, 236)
                    },
                };
                style
            });
        let workspace = row![left, right].spacing(22).height(Length::Fill);
        let mut main = if self.settings_open {
            column![header, horizontal_rule(1)].spacing(18)
        } else {
            column![
                header,
                horizontal_rule(1),
                column![
                    text("찾고, 선택하고, 저장하세요.").size(26),
                    text("검색어와 YouTube 링크를 한 곳에서.")
                        .size(14)
                        .color(self.muted())
                ]
                .spacing(6),
                searchbar
            ]
            .spacing(18)
        };
        if let Some(error) = self.error.as_ref().or_else(|| self.engine.as_ref().err()) {
            main = main.push(
                container(text(error).size(13).color(Color::from_rgb8(211, 85, 85)))
                    .padding(10)
                    .width(Length::Fill)
                    .style(container::rounded_box),
            );
        }
        main = main.push(if self.settings_open {
            self.settings_view()
        } else {
            workspace.into()
        });
        let mut status = column![text(if self.update_busy {
            &self.update_status
        } else {
            &self.status
        })
        .size(14)]
        .spacing(5);
        if let Some(path) = &self.completed {
            status = status.push(
                text(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                )
                .size(12)
                .color(self.muted()),
            );
        }
        if self.downloading {
            status = status.push(progress_bar(0.0..=100.0, self.progress).girth(5));
        }
        let action: Element<'_, Message> = if self.downloading {
            button(
                text(if self.cancelling {
                    "취소 중…"
                } else {
                    "다운로드 취소"
                })
                .size(14),
            )
            .on_press_maybe((!self.cancelling).then_some(Message::Cancel))
            .padding(10)
            .style(button::secondary)
            .into()
        } else {
            button(
                row![
                    text("\u{e8b7}").font(ICON_FONT).size(17),
                    text("폴더 열기").size(14)
                ]
                .spacing(8),
            )
            .on_press_maybe(self.completed.as_ref().map(|_| Message::OpenFolder))
            .padding(10)
            .style(button::secondary)
            .into()
        };
        container(column![
            container(main).padding(24).height(Length::Fill),
            horizontal_rule(1),
            container(
                row![status.width(Length::Fill), action]
                    .spacing(18)
                    .align_y(alignment::Vertical::Center)
            )
            .padding([14, 24])
        ])
        .height(Length::Fill)
        .into()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.date_cancel.store(true, Ordering::Relaxed);
        self.search_cancel.store(true, Ordering::Relaxed);
        self.detail_cancel.store(true, Ordering::Relaxed);
        self.download_cancel.store(true, Ordering::Relaxed);
    }
}

fn open_external(url: &str) -> Result<(), String> {
    // 문자열을 셸 명령으로 조합하지 않고 URL을 단일 인수로 넘긴다.
    let mut command = Command::new("rundll32.exe");
    engine::hide_console(&mut command);
    command
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("브라우저를 열지 못했어요: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_responses_do_not_replace_the_current_selection() {
        let mut app = App::new();
        app.generation = 3;
        app.detail_request = 5;
        app.searching = true;
        app.handle_event(Event::Search(2, Err("이전 검색 오류".into())));
        assert!(app.searching);
        assert!(app.error.is_none());
        app.videos.push(Video {
            id: "jNQXAC9IVRw".into(),
            title: "현재 영상".into(),
            channel: "채널".into(),
            duration: "0:19".into(),
            url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".into(),
            thumbnail: None,
            heights: vec![240],
            upload_date: None,
        });
        app.selected = Some(0);
        app.loading_detail = true;
        app.handle_event(Event::Detail(
            3,
            4,
            "jNQXAC9IVRw".into(),
            Err("지난 요청 오류".into()),
        ));
        assert!(app.loading_detail);
        assert!(app.error.is_none());
        app.handle_event(Event::Detail(
            3,
            5,
            "jNQXAC9IVRw".into(),
            Ok(app.videos[0].clone()),
        ));
        assert!(!app.loading_detail);
        assert_eq!(app.qualities, vec!["최고 화질", "240p"]);
    }

    fn fixture(id: &str) -> Video {
        Video {
            id: id.into(),
            title: "영상".into(),
            channel: "채널".into(),
            duration: "1:00".into(),
            url: format!("https://www.youtube.com/watch?v={id}"),
            thumbnail: None,
            heights: vec![],
            upload_date: Some("2026.10.21".into()),
        }
    }

    #[test]
    fn more_appends_deduplicates_and_preserves_selection_and_retry_offset() {
        let mut app = App::new();
        app.engine = Err("테스트".into());
        app.generation = 4;
        app.search_offset = 20;
        app.has_more = true;
        app.videos = vec![fixture("jNQXAC9IVRw")];
        app.selected = Some(0);
        app.detail_ready = Some("jNQXAC9IVRw".into());
        app.handle_event(Event::Search(4, Err("네트워크 실패".into())));
        assert_eq!(app.search_offset, 20);
        assert_eq!(app.videos.len(), 1);
        assert!(app.has_more);
        app.handle_event(Event::Search(
            4,
            Ok(vec![fixture("jNQXAC9IVRw"), fixture("abcdefghijk")]),
        ));
        assert_eq!(app.videos.len(), 2);
        assert_eq!(app.selected, Some(0));
        assert_eq!(app.detail_ready.as_deref(), Some("jNQXAC9IVRw"));
        assert_eq!(app.search_offset, 40);
        assert!(!app.has_more);
    }

    #[test]
    fn date_results_ignore_old_searches_and_keep_success_on_failure() {
        let mut app = App::new();
        app.generation = 4;
        let mut video = fixture("jNQXAC9IVRw");
        video.upload_date = None;
        app.videos.push(video);
        app.date_active = 2;
        app.handle_event(Event::Date(
            3,
            "jNQXAC9IVRw".into(),
            Some("2005.04.23".into()),
        ));
        assert_eq!(app.date_active, 2);
        assert!(app.videos[0].upload_date.is_none());
        app.handle_event(Event::Date(
            4,
            "jNQXAC9IVRw".into(),
            Some("2005.04.23".into()),
        ));
        app.handle_event(Event::Date(4, "jNQXAC9IVRw".into(), None));
        assert_eq!(app.videos[0].upload_date.as_deref(), Some("2005.04.23"));
        assert_eq!(app.date_cache["jNQXAC9IVRw"], "2005.04.23");
        assert_eq!(app.date_active, 0);
        app.date_active = 1;
        let _ = app.update(Message::InstallUpdate);
        assert!(!app.update_busy);
    }

    #[test]
    fn cancellation_never_enables_completion_actions() {
        let mut app = App::new();
        app.downloading = true;
        app.cancelling = true;
        app.status = "취소 중".into();
        app.handle_event(Event::Progress(100.0, "완료처럼 보이는 이전 진행률".into()));
        assert_eq!(app.status, "취소 중");
        app.handle_event(Event::Download(Err("취소됨".into())));
        assert!(!app.downloading);
        assert!(app.completed.is_none());
    }

    #[test]
    fn updating_is_blocked_during_download_and_failure_keeps_engine() {
        let mut app = App::new();
        app.engine = Ok(Engine {
            root: PathBuf::from("bundled"),
            executable: PathBuf::from("bundled/yt-dlp.exe"),
        });
        let original = app.engine.as_ref().unwrap().executable.clone();
        app.downloading = true;
        let _ = app.update(Message::CheckUpdate);
        let _ = app.update(Message::InstallUpdate);
        let _ = app.update(Message::Rollback);
        assert!(!app.update_busy);
        app.downloading = false;
        app.update_busy = true;
        app.handle_event(Event::Updated(Err("체크섬 불일치".into())));
        assert_eq!(app.engine.as_ref().unwrap().executable, original);
        assert!(!app.update_busy);
        assert!(app.update_error.is_some());
    }
}
