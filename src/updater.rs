use crate::engine::{hide_console, Engine};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const RELEASE_API: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
const MAX_BINARY: u64 = 150 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub version: String,
    pub hash: String,
    pub executable: PathBuf,
    pub previous: Option<String>,
    pub latest: Option<Release>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Slot {
    version: String,
    hash: String,
    // None은 앱에 동봉된 엔진. 관리 엔진 파일명은 해시로만 생성한다.
    managed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct State {
    active: Slot,
    previous: Option<Slot>,
}

#[derive(Clone)]
pub struct Updater {
    bundled: PathBuf,
    directory: PathBuf,
}

impl Updater {
    pub fn new(engine: &Engine) -> Result<Self, String> {
        let directory = directories::ProjectDirs::from("com", "cafealpa", "YouTubeDownloader")
            .ok_or("사용자 앱 데이터 폴더를 찾지 못했어요.")?
            .data_local_dir()
            .join("engines");
        Ok(Self {
            bundled: engine.root.join("yt-dlp.exe"),
            directory,
        })
    }

    fn path(&self, slot: &Slot) -> Result<PathBuf, String> {
        if !valid_version(&slot.version)
            || slot.hash.len() != 64
            || !slot.hash.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("저장된 엔진 정보가 올바르지 않아요.".into());
        }
        Ok(if slot.managed {
            self.directory.join(format!("yt-dlp-{}.exe", slot.hash))
        } else {
            self.bundled.clone()
        })
    }

    fn read_state(&self) -> Result<Option<State>, String> {
        match fs::read(self.directory.join("state.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "엔진 설정 파일을 읽지 못했어요. 동봉된 엔진은 유지돼요.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("엔진 설정 읽기 실패: {e}")),
        }
    }

    fn verified_path(&self, slot: &Slot) -> Result<PathBuf, String> {
        let path = self.path(slot)?;
        verify_hash(&path, &slot.hash)?;
        Ok(path)
    }

    pub fn resolve_active(&self) -> Result<Option<PathBuf>, String> {
        self.read_state()?
            .map(|state| self.verified_path(&state.active))
            .transpose()
    }

    fn current(&self) -> Result<State, String> {
        if let Some(state) = self.read_state()? {
            self.verified_path(&state.active)?;
            Ok(state)
        } else {
            Ok(State {
                active: Slot {
                    version: executable_version(&self.bundled)?,
                    hash: hash_file(&self.bundled)?,
                    managed: false,
                },
                previous: None,
            })
        }
    }

    fn snapshot_of(&self, state: &State) -> Result<Snapshot, String> {
        Ok(Snapshot {
            version: state.active.version.clone(),
            hash: state.active.hash.clone(),
            executable: self.path(&state.active)?,
            previous: state.previous.as_ref().map(|v| v.version.clone()),
            latest: None,
        })
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.snapshot_of(&self.current()?)
    }

    pub fn check(&self) -> Result<Snapshot, String> {
        let mut snapshot = self.snapshot()?;
        let bytes = download_small(RELEASE_API, 2 * 1024 * 1024)?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "업데이트 응답을 읽지 못했어요.")?;
        let version = value["tag_name"]
            .as_str()
            .filter(|s| valid_version(s))
            .ok_or("공식 릴리스 버전이 올바르지 않아요.")?;
        if value["prerelease"] == true || value["draft"] == true {
            return Err("정식 릴리스 정보를 확인하지 못했어요.".into());
        }
        snapshot.latest = Some(Release {
            version: version.into(),
        });
        Ok(snapshot)
    }

    fn lock(&self) -> Result<File, String> {
        fs::create_dir_all(&self.directory).map_err(|e| format!("업데이트 폴더 준비 실패: {e}"))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("update.lock"))
            .map_err(|e| e.to_string())?;
        file.try_lock_exclusive().map_err(|_| {
            "다른 앱 창에서 엔진을 업데이트하고 있어요. 잠시 뒤 다시 확인해 주세요."
        })?;
        Ok(file)
    }

    fn expected_state(&self, expected: &str) -> Result<State, String> {
        let state = self.current()?;
        if state.active.hash != expected {
            return Err(
                "다른 앱 창에서 엔진이 변경됐어요. 업데이트 확인을 다시 눌러 주세요.".into(),
            );
        }
        Ok(state)
    }

    pub fn install(
        &self,
        release: &Release,
        expected: &str,
        progress: impl Fn(&str),
    ) -> Result<Snapshot, String> {
        let _lock = self.lock()?;
        let current = self.expected_state(expected)?;
        if !valid_version(&release.version) || release.version <= current.active.version {
            return Err("현재보다 새로운 정식 버전만 설치할 수 있어요.".into());
        }
        let base = format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}",
            release.version
        );
        progress("공식 체크섬을 확인하고 있어요…");
        let sums = download_small(&format!("{base}/SHA2-256SUMS"), 1024 * 1024)?;
        let hash = checksum_entry(&String::from_utf8_lossy(&sums))?;
        let staged = TempFile::new(&self.directory, "engine")?;
        progress("새 yt-dlp를 다운로드하고 있어요…");
        let mut response = client()?
            .get(format!("{base}/yt-dlp.exe"))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(network_error)?
            .take(MAX_BINARY + 1);
        let mut file = OpenOptions::new()
            .write(true)
            .open(&staged.0)
            .map_err(|e| e.to_string())?;
        let count =
            std::io::copy(&mut response, &mut file).map_err(|e| format!("다운로드 실패: {e}"))?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if count == 0 || count > MAX_BINARY {
            return Err("엔진 다운로드 크기가 올바르지 않아요.".into());
        }
        progress("파일 무결성과 실행 버전을 검증하고 있어요…");
        verify_hash(&staged.0, &hash)?;
        if executable_version(&staged.0)? != release.version {
            return Err("새 엔진의 실행 버전이 릴리스 정보와 달라요.".into());
        }
        let candidate = Slot {
            version: release.version.clone(),
            hash,
            managed: true,
        };
        // 새 바이너리를 별도 경로에 보관하므로 사용 중인 엔진 파일은 교체하지 않는다.
        let destination = self.path(&candidate)?;
        if destination.exists() {
            verify_hash(&destination, &candidate.hash)?;
        } else {
            fs::rename(&staged.0, &destination).map_err(|e| format!("엔진 보관 실패: {e}"))?;
        }
        progress("검증된 엔진으로 전환하고 있어요…");
        self.activate(candidate, current)
    }

    fn activate(&self, candidate: Slot, current: State) -> Result<Snapshot, String> {
        self.verified_path(&candidate)?;
        // 되돌릴 기존 파일도 확인한다. 상태 전환 이전 실패는 기존 state.json에 영향이 없다.
        self.verified_path(&current.active)?;
        let next = State {
            active: candidate,
            previous: Some(current.active),
        };
        let snapshot = self.snapshot_of(&next)?;
        self.commit(&next)?;
        Ok(snapshot)
    }

    pub fn rollback(&self, expected: &str) -> Result<Snapshot, String> {
        let _lock = self.lock()?;
        let current = self.expected_state(expected)?;
        let previous = current
            .previous
            .clone()
            .ok_or("되돌릴 이전 버전이 없어요.")?;
        let path = self.verified_path(&previous)?;
        if executable_version(&path)? != previous.version {
            return Err("이전 엔진의 실행 버전 확인에 실패했어요.".into());
        }
        self.activate(previous, current)
    }

    fn commit(&self, state: &State) -> Result<(), String> {
        let pending = TempFile::new(&self.directory, "state")?;
        let data = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .write(true)
            .open(&pending.0)
            .map_err(|e| e.to_string())?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("엔진 설정 저장 실패: {e}"))?;
        drop(file);
        replace_file(&pending.0, &self.directory.join("state.json"))
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("YouTubeDownloader/2.0")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .https_only(true)
        .build()
        .map_err(|e| e.to_string())
}

fn network_error(e: reqwest::Error) -> String {
    if e.status()
        .is_some_and(|s| s.as_u16() == 403 || s.as_u16() == 429)
    {
        "GitHub 요청 한도에 도달했어요. 잠시 후 다시 확인해 주세요.".into()
    } else {
        format!("공식 업데이트 서버 연결 실패: {e}")
    }
}

fn download_small(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut response = client()?
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(network_error)?
        .take(limit + 1);
    let mut data = Vec::new();
    response.read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > limit {
        return Err("업데이트 응답이 너무 커요.".into());
    }
    Ok(data)
}

fn valid_version(version: &str) -> bool {
    version.len() == 10
        && version.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'.'
            } else {
                b.is_ascii_digit()
            }
        })
}

fn checksum_entry(sums: &str) -> Result<String, String> {
    sums.lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            let name = fields.next()?.trim_start_matches('*');
            (name == "yt-dlp.exe"
                && hash.len() == 64
                && hash.bytes().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
        })
        .ok_or("공식 체크섬에서 Windows 엔진을 찾지 못했어요.".into())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("엔진 파일 읽기 실패: {e}"))?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buf).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buf[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verify_hash(path: &Path, expected: &str) -> Result<(), String> {
    if hash_file(path)? != expected {
        return Err("엔진 파일의 SHA-256이 일치하지 않아요. 기존 엔진을 유지해요.".into());
    }
    Ok(())
}

fn executable_version(path: &Path) -> Result<String, String> {
    let mut command = Command::new(path);
    hide_console(&mut command);
    let mut child = command
        .args(["--ignore-config", "--version"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("엔진 실행 확인 실패: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("엔진 버전 출력을 읽지 못했어요.")?;
    let output = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.take(4096).read_to_end(&mut bytes);
        bytes
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < Duration::from_secs(15) => {
                thread::sleep(Duration::from_millis(50))
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(match result {
                    Err(e) => format!("엔진 상태 확인 실패: {e}"),
                    _ => "엔진 버전 확인 시간이 초과됐어요.".into(),
                });
            }
        }
    };
    let bytes = output.join().unwrap_or_default();
    if !status?.success() {
        return Err("새 엔진을 실행할 수 없어요.".into());
    }
    let value = String::from_utf8_lossy(&bytes).trim().to_owned();
    if !valid_version(&value) {
        return Err("엔진 버전 응답이 올바르지 않아요.".into());
    }
    Ok(value)
}

struct TempFile(PathBuf);
impl TempFile {
    fn new(directory: &Path, name: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        // Windows에서 실행 검증이 가능하도록 임시 엔진에도 exe 확장자를 사용한다.
        let path = directory.join(format!(".{name}-{}-{nonce}.exe", std::process::id()));
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(format!(
                "엔진 설정 전환 실패: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fs::rename(source, destination).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        updater: Updater,
    }
    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!("update-test-{nonce}"));
            fs::create_dir_all(&directory).unwrap();
            let bundled = directory.join("bundled.exe");
            fs::write(&bundled, b"original").unwrap();
            Self {
                updater: Updater { bundled, directory },
            }
        }
        fn original(&self) -> Slot {
            Slot {
                version: "2026.08.19".into(),
                hash: hash_file(&self.updater.bundled).unwrap(),
                managed: false,
            }
        }
        fn candidate(&self) -> Slot {
            let hash = format!("{:x}", Sha256::digest(b"new engine"));
            let slot = Slot {
                version: "2026.10.20".into(),
                hash,
                managed: true,
            };
            fs::write(self.updater.path(&slot).unwrap(), b"new engine").unwrap();
            slot
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .canonicalize();
            let directory = self.updater.directory.canonicalize();
            if let (Ok(root), Ok(directory)) = (root, directory) {
                if directory.starts_with(root)
                    && directory
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("update-test-"))
                {
                    let _ = fs::remove_dir_all(directory);
                }
            }
        }
    }
    #[test]
    fn activation_and_reverse_preserve_both_files() {
        let f = Fixture::new();
        let old = f.original();
        let new = f.candidate();
        f.updater
            .activate(
                new.clone(),
                State {
                    active: old.clone(),
                    previous: None,
                },
            )
            .unwrap();
        let state = f.updater.read_state().unwrap().unwrap();
        assert_eq!(state.previous.as_ref(), Some(&old));
        f.updater.activate(old.clone(), state).unwrap();
        let state = f.updater.read_state().unwrap().unwrap();
        assert_eq!(state.active, old);
        assert_eq!(state.previous, Some(new.clone()));
        assert!(f.updater.path(&new).unwrap().is_file());
    }
    #[test]
    fn checksum_failure_leaves_manifest_and_active_engine_untouched() {
        let f = Fixture::new();
        let old = f.original();
        let new = f.candidate();
        let state = State {
            active: old.clone(),
            previous: None,
        };
        f.updater.commit(&state).unwrap();
        let before = fs::read(f.updater.directory.join("state.json")).unwrap();
        fs::write(f.updater.path(&new).unwrap(), b"corrupted").unwrap();
        assert!(f.updater.activate(new, state).is_err());
        assert_eq!(
            before,
            fs::read(f.updater.directory.join("state.json")).unwrap()
        );
        f.updater.verified_path(&old).unwrap();
    }
    #[test]
    fn failed_atomic_replace_preserves_original() {
        let f = Fixture::new();
        f.updater
            .commit(&State {
                active: f.original(),
                previous: None,
            })
            .unwrap();
        let target = f.updater.directory.join("state.json");
        let before = fs::read(&target).unwrap();
        assert!(replace_file(&f.updater.directory.join("missing"), &target).is_err());
        assert_eq!(before, fs::read(&target).unwrap());
    }
    #[test]
    fn concurrent_updates_and_stale_state_are_rejected() {
        let f = Fixture::new();
        let _lock = f.updater.lock().unwrap();
        assert!(f.updater.lock().is_err());
        f.updater
            .commit(&State {
                active: f.original(),
                previous: None,
            })
            .unwrap();
        assert!(f.updater.expected_state("stale").is_err());
    }
    #[test]
    fn only_exact_official_asset_and_safe_versions_are_accepted() {
        assert!(!valid_version("../engine"));
        assert!(!valid_version("nightly"));
        let hash = "a".repeat(64);
        assert_eq!(
            checksum_entry(&format!("{hash}  yt-dlp.exe\n")).unwrap(),
            hash
        );
        assert!(checksum_entry(&format!("{hash}  yt-dlp_x86.exe\n")).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn locked_manifest_keeps_current_version_on_commit_failure() {
        use std::os::windows::fs::OpenOptionsExt;
        let f = Fixture::new();
        let old = f.original();
        let current = State {
            active: old.clone(),
            previous: None,
        };
        f.updater.commit(&current).unwrap();
        let path = f.updater.directory.join("state.json");
        let before = fs::read(&path).unwrap();
        // 읽기는 허용하지만 교체(삭제 공유)는 허용하지 않는 열린 파일.
        let _locked = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(f.updater.activate(f.candidate(), current).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            f.updater.resolve_active().unwrap(),
            Some(f.updater.path(&old).unwrap())
        );
    }

    #[test]
    #[ignore = "공식 GitHub 다운로드와 Windows 엔진 실행이 필요한 통합 검증"]
    fn live_update_and_rollback() {
        let mut f = Fixture::new();
        // 사용자 엔진은 건드리지 않고 공식 구버전을 격리된 테스트 폴더에 준비한다.
        let baseline = "https://github.com/yt-dlp/yt-dlp/releases/download/2025.06.30";
        let sums = download_small(&format!("{baseline}/SHA2-256SUMS"), 1024 * 1024).unwrap();
        let hash = checksum_entry(&String::from_utf8_lossy(&sums)).unwrap();
        f.updater.bundled = f.updater.directory.join("baseline.exe");
        let binary = download_small(&format!("{baseline}/yt-dlp.exe"), MAX_BINARY).unwrap();
        fs::write(&f.updater.bundled, binary).unwrap();
        verify_hash(&f.updater.bundled, &hash).unwrap();
        let initial = f.updater.check().unwrap();
        let latest = initial.latest.unwrap();
        assert!(
            latest.version > initial.version,
            "업데이트 검증에는 최신보다 오래된 동봉 엔진이 필요합니다."
        );
        let installed = f
            .updater
            .install(&latest, &initial.hash, |s| println!("{s}"))
            .unwrap();
        assert_eq!(installed.version, latest.version);
        assert_eq!(
            installed.previous.as_deref(),
            Some(initial.version.as_str())
        );
        assert_eq!(
            f.updater.resolve_active().unwrap(),
            Some(installed.executable.clone())
        );
        let restored = f.updater.rollback(&installed.hash).unwrap();
        assert_eq!(restored.hash, initial.hash);
        assert_eq!(restored.executable, f.updater.bundled);
        assert!(installed.executable.is_file());
        // 네트워크 실패도 이미 저장된 활성 엔진과 복구 정보를 바꾸지 않는다.
        let before = fs::read(f.updater.directory.join("state.json")).unwrap();
        assert!(f
            .updater
            .install(
                &Release {
                    version: "9999.01.01".into()
                },
                &restored.hash,
                |_| {}
            )
            .is_err());
        assert_eq!(
            fs::read(f.updater.directory.join("state.json")).unwrap(),
            before
        );
        let forward = f.updater.rollback(&restored.hash).unwrap();
        assert_eq!(forward.hash, installed.hash);
        println!(
            "실제 업데이트/복구 성공: {} → {} → {} (재전환 포함)",
            initial.version, installed.version, restored.version
        );
    }
}
