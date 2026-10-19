use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, OnceLock};

use zbus::interface;

const SERVICE: &str = "org.QuietAutoClicker";
const OBJECT_PATH: &str = "/org/QuietAutoClicker/Cursor";

// KWin scripting host that runs our cursor script on demand.
const KWIN_SERVICE: &str = "org.kde.KWin";
const KWIN_PATH: &str = "/Scripting";
const KWIN_IFACE: &str = "org.kde.kwin.Scripting";

/// The KWin cursor script bundled at build time (kwin/cursor.js). Embedded so
/// it works no matter how the binary is launched (portable folder, `cargo run`,
/// an AUR package).
const CURSOR_JS: &str = include_str!("../kwin/cursor.js");

/// Latest cursor position that the KWin script pushed. Read by the capture
/// path via `position()`; written by the D-Bus object on this thread.
#[derive(Clone)]
struct Position {
    x: Arc<AtomicI32>,
    y: Arc<AtomicI32>,
}

static POSITION: OnceLock<Position> = OnceLock::new();

/// D-Bus target for the KWin script's `callDBus`. Caches the cursor position so
/// the F7 capture handler can read it synchronously — no async D-Bus round-trip
/// in the hotkey path.
struct CursorServer(Position);

#[interface(name = "org.QuietAutoClicker.Cursor")]
impl CursorServer {
    fn position_changed(&self, x: i32, y: i32) -> zbus::fdo::Result<()> {
        self.0.x.store(x, Ordering::Relaxed);
        self.0.y.store(y, Ordering::Relaxed);
        Ok(())
    }
}

/// Start the KWin cursor bridge on its own thread (Linux/KDE Wayland). It loads
/// the bundled script into KWin via `org.kde.kwin.Scripting` and serves the
/// D-Bus object the script pushes `workspace.cursorPos` into. Best-effort: a
/// failure only means `position()` returns `None` (capture does nothing) and is
/// logged to qac_debug.log.
pub fn start() {
    std::thread::spawn(run);
}

fn run() {
    crate::sequence::write_diag("kwincursor: bridge starting");
    // `async_io::block_on` drives the smol reactor that zbus needs to service
    // incoming PositionChanged calls; futures' own `block_on` would not pump the
    // connection's socket.
    async_io::block_on(async {
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                crate::sequence::write_diag(&format!("kwincursor: no session bus: {e}"));
                return;
            }
        };

        if let Err(e) = conn.request_name(SERVICE).await {
            crate::sequence::write_diag(&format!("kwincursor: request_name failed: {e}"));
            return;
        }

        let pos = POSITION.get_or_init(|| Position {
            x: Arc::new(AtomicI32::new(0)),
            y: Arc::new(AtomicI32::new(0)),
        });
        if let Err(e) = conn
            .object_server()
            .at(OBJECT_PATH, CursorServer(pos.clone()))
            .await
        {
            crate::sequence::write_diag(&format!("kwincursor: serve object failed: {e}"));
            return;
        }

        load_script(&conn).await;

        // Keep servicing incoming calls until the process exits (this thread is
        // detached, so teardown kills it).
        std::future::pending::<()>().await;
    });
}

/// Write the bundled script to a per-session path and register it with KWin so
/// its `init()` starts pushing cursor position to our object. Idempotent per
/// session: if KWin already has it loaded we don't load a second copy.
async fn load_script(conn: &zbus::Connection) {
    let Some(path) = write_script() else { return };

    let proxy = match zbus::Proxy::new(conn, KWIN_SERVICE, KWIN_PATH, KWIN_IFACE).await {
        Ok(p) => p,
        Err(e) => {
            crate::sequence::write_diag(&format!("kwincursor: KWin script proxy failed: {e}"));
            return;
        }
    };

    let already = match proxy.call_method("isScriptLoaded", &(path.clone(),)).await {
        Ok(r) => r.body().deserialize::<bool>().unwrap_or(false),
        Err(e) => {
            crate::sequence::write_diag(&format!("kwincursor: isScriptLoaded failed: {e}"));
            false
        }
    };
    if already {
        crate::sequence::write_diag("kwincursor: script already loaded");
        return;
    }

    let id = match proxy
        .call_method("loadScript", &(path.clone(), "QuietAutoClicker"))
        .await
    {
        Ok(r) => match r.body().deserialize::<i32>() {
            Ok(id) => id,
            Err(_) => {
                crate::sequence::write_diag("kwincursor: loadScript returned no id");
                return;
            }
        },
        Err(e) => {
            crate::sequence::write_diag(&format!("kwincursor: loadScript failed: {e}"));
            return;
        }
    };
    // `start` takes no arguments and runs the console-loaded scripts. Some KWin
    // versions run a script as soon as it is loaded, so a missing/unknown `start`
    // is not fatal — the script is registered either way.
    if let Err(e) = proxy.call_method("start", &()).await {
        crate::sequence::write_diag(&format!("kwincursor: start skipped ({id}): {e}"));
    }
    crate::sequence::write_diag("kwincursor: cursor script loaded and started");
}

/// Materialize the cursor script into a per-session location KWin can load.
/// XDG_RUNTIME_DIR is cleared on logout, which is fine: the script is only
/// needed while the app is running.
fn write_script() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("QuietAutoClicker");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("cursor.js");
    if std::fs::write(&path, CURSOR_JS).is_err() {
        crate::sequence::write_diag("kwincursor: could not write cursor script");
        return None;
    }
    Some(path)
}

/// Read the latest cursor position the KWin script reported. `None` if the
/// bridge never came up or nothing has been reported yet (0,0 is treated as
/// "unknown").
pub fn position() -> Option<(i32, i32)> {
    POSITION
        .get()
        .map(|p| (p.x.load(Ordering::Relaxed), p.y.load(Ordering::Relaxed)))
        .filter(|(x, y)| *x != 0 || *y != 0)
}
