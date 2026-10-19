#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Whether diagnostics are enabled. Off by default (release ships with no log
/// writes); `main` enables it when launched with `--debug`.
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_debug(enabled: bool) {
    DEBUG_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SequencePoint {
    pub x: i32,
    pub y: i32,
    #[serde(default = "default_sequence_clicks")]
    pub clicks: u32,
}

const fn default_sequence_clicks() -> u32 {
    1
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SequenceConfig {
    #[serde(default)]
    pub points: Vec<SequencePoint>,
}

/// Directory that holds the runtime files (`sequence.json`, `qac_debug.log`).
///
/// Windows is always portable: the exe sits in a user-controlled folder, so
/// these live next to it. On Linux the rule is the same *only* for a plain
/// executable in a writable folder. AppImage mounts the executable read-only
/// and the AUR package installs to root-owned `/usr/bin` — neither can write
/// next to the binary, so runtime files fall back to the XDG data home
/// (`~/.local/share/quietautoclicker`). Resolved once; the environment cannot
/// change mid-run.
pub fn get_root_dir() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(resolve_root_dir).clone()
}

fn execute_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

fn resolve_root_dir() -> PathBuf {
    if let Some(dir) = execute_dir() {
        #[cfg(windows)]
        {
            return dir;
        }
        #[cfg(unix)]
        {
            if dir_writable(&dir) {
                return dir;
            }
        }
    }
    fallback_dir()
}

#[cfg(windows)]
fn fallback_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_default()
}

#[cfg(unix)]
fn fallback_dir() -> PathBuf {
    let dir = xdg_data_base().join("quietautoclicker");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[cfg(unix)]
fn xdg_data_base() -> PathBuf {
    let xdg = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    xdg_data_base_from(xdg.as_deref(), home.as_deref())
}

#[cfg(unix)]
fn xdg_data_base_from(xdg: Option<&Path>, home: Option<&Path>) -> PathBuf {
    match xdg.filter(|p| !p.as_os_str().is_empty()) {
        Some(base) => base.to_path_buf(),
        None => home.unwrap_or(Path::new("")).join(".local/share"),
    }
}

#[cfg(unix)]
fn dir_writable(dir: &Path) -> bool {
    // Probe: create + remove a zero-byte file. Reliable where permission bits
    // lie (ACLs) or where creation always fails — a read-only AppImage squashfs,
    // root-owned /usr, kernel filesystems like /proc.
    let probe = dir.join(format!(".qac-write-probe-{}", std::process::id()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

pub fn get_sequence_path() -> PathBuf {
    get_root_dir().join("sequence.json")
}

/// Wipe `qac_debug.log` in the runtime data directory so each `--debug` launch
/// reports a single clean run. No-op unless debug is enabled. Silent on failure.
pub fn reset_diag_log() {
    if !debug_enabled() {
        return;
    }
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(get_root_dir().join("qac_debug.log"));
}

/// Append one line to `qac_debug.log` in the runtime data directory. On
/// Windows the app has no console (`#![windows_subsystem = "windows"]`), so
/// stderr is invisible there and this file is how a user reports failures
/// back; on Linux it mirrors the same stderr diagnostics into the file for
/// parity. Truncates the file once it grows past the cap. Silent on failure
/// (never poison the caller).
pub fn write_diag(msg: &str) {
    use std::io::Write;
    const MAX_DIAG_LOG_BYTES: u64 = 64 * 1024;

    if !debug_enabled() {
        return;
    }

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let ts = format!(
        "{:02}:{:02}:{:02}",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60
    );

    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(get_root_dir().join("qac_debug.log"))
    else {
        return;
    };
    if f.metadata()
        .map(|m| m.len() > MAX_DIAG_LOG_BYTES)
        .unwrap_or(false)
    {
        let _ = f.set_len(0);
    }
    let _ = writeln!(f, "[{ts}] {msg}");
}

/// Load only the persisted sequence points; everything else resets each launch.
pub fn load_sequence() -> SequenceConfig {
    let path = get_sequence_path();
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str(&data) {
                return cfg;
            }
        }
    }
    SequenceConfig::default()
}

pub fn save_sequence(cfg: &SequenceConfig) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(get_sequence_path(), json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_point_round_trip() {
        let pt = SequencePoint {
            x: 123,
            y: -45,
            clicks: 3,
        };
        let json = serde_json::to_string(&pt).unwrap();
        let back: SequencePoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back.x, 123);
        assert_eq!(back.y, -45);
        assert_eq!(back.clicks, 3);
    }

    #[test]
    fn sequence_point_clicks_defaults_to_one() {
        let json = r#"{"x":10,"y":20}"#;
        let pt: SequencePoint = serde_json::from_str(json).unwrap();
        assert_eq!(pt.clicks, 1);
    }

    #[test]
    fn config_round_trip_preserves_points() {
        let cfg = SequenceConfig {
            points: vec![
                SequencePoint {
                    x: 1,
                    y: 2,
                    clicks: 1,
                },
                SequencePoint {
                    x: 3,
                    y: 4,
                    clicks: 2,
                },
            ],
        };
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let back: SequenceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.points.len(), 2);
        assert_eq!(back.points[1].clicks, 2);
    }

    #[test]
    fn empty_config_deserializes() {
        let json = r#"{}"#;
        let cfg: SequenceConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.points.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn xdg_data_base_uses_xdg_when_set() {
        let base = xdg_data_base_from(Some(Path::new("/custom/data")), Some(Path::new("/home/u")));
        assert_eq!(base, PathBuf::from("/custom/data"));
    }

    #[cfg(unix)]
    #[test]
    fn xdg_data_base_defaults_to_home_local_share() {
        let base = xdg_data_base_from(None, Some(Path::new("/home/u")));
        assert_eq!(base, PathBuf::from("/home/u/.local/share"));
    }

    #[cfg(unix)]
    #[test]
    fn xdg_data_base_empty_xdg_falls_back() {
        let base = xdg_data_base_from(Some(Path::new("")), Some(Path::new("/home/u")));
        assert_eq!(base, PathBuf::from("/home/u/.local/share"));
    }

    #[cfg(unix)]
    #[test]
    fn writable_dir_detected_as_portable() {
        let cwd = std::env::current_dir().unwrap();
        assert!(dir_writable(&cwd));
    }

    #[cfg(unix)]
    #[test]
    fn read_only_dir_not_portable() {
        // Kernel filesystems reject file creation even for root.
        assert!(!dir_writable(Path::new("/proc")));
    }
}
