use std::sync::{
    atomic::Ordering,
    mpsc::{channel, Sender},
    Arc, Mutex, OnceLock,
};
use std::time::Duration;

use ashpd::desktop::global_shortcuts::{
    BindShortcutsOptions, ConfigureShortcutsOptions, GlobalShortcuts, ListShortcutsOptions,
    NewShortcut,
};
use ashpd::desktop::CreateSessionOptions;
use futures::executor::block_on;
use futures::stream::{select_all, Stream, StreamExt};

use crate::app::{ActivationMode, SharedState};
use crate::sequence::SequencePoint;

pub const ACTION_TOGGLE: &str = "toggle";
pub const ACTION_CAPTURE: &str = "capture";

/// App id and portal component name. Must match the installed
/// `QuietAutoClicker.desktop` basename: the KCM display name comes from that
/// file's `Name=` entry and the Edit button opens `systemsettings://kcm_keys/<component>`.
const APP_ID: &str = "QuietAutoClicker";

/// UI -> listener commands, drained on the 800ms poll tick. The listener
/// owns the `GlobalShortcuts`/`Session` handles.
enum Cmd {
    OpenShortcutSettings,
}

static CMD_TX: OnceLock<Mutex<Option<Sender<Cmd>>>> = OnceLock::new();

// Factory defaults, seeded only for fresh actions: portal-kde applies
// `preferred_trigger` solely when the action has no existing kglobalshortcutsrc
// entry, so a user-configured or user-cleared binding is never overwritten or
// resurrected. The UI shows these as a placeholder until the portal reports a
// real reading.
pub const TRIGGER_TOGGLE_DEFAULT: &str = "F6";
pub const TRIGGER_CAPTURE_DEFAULT: &str = "F7";

const MAX_POINTS: usize = 10;
const POLL_INTERVAL: Duration = Duration::from_millis(800);

fn capture_sequence_point(shared: &Arc<SharedState>) {
    match crate::kwincursor::position() {
        Some((x, y)) => {
            crate::sequence::write_diag(&format!("hotkeys: cursor = {x},{y}"));
            let (mut points, r#loop, smooth, smooth_ms) = shared.get_sequence_data();
            if points.len() < MAX_POINTS {
                points.push(SequencePoint { x, y, clicks: 1 });
                shared.set_sequence_data(points, r#loop, smooth, smooth_ms);
            } else {
                crate::sequence::write_diag("hotkeys: capture skipped (10 point cap)");
            }
        }
        None => {
            crate::sequence::write_diag("hotkeys: capture ignored (cursor bridge not ready)");
        }
    }
}

fn on_activated(shared: &Arc<SharedState>, action: &str) {
    // The `capture` action (F7 default) adds a sequence point at the cursor.
    // No explicit capture mode anymore: it is active whenever advanced mode is
    // on and the Sequence Clicking checkbox is ticked (mirrors the Windows
    // rdev listener). Toggle still activates the clicker.
    if action == ACTION_CAPTURE {
        let (advanced, seq) = (
            shared.advanced_mode.load(Ordering::Relaxed),
            shared.sequence_enabled.load(Ordering::Relaxed),
        );
        if advanced && seq {
            capture_sequence_point(shared);
        } else {
            crate::sequence::write_diag(&format!(
                "hotkeys: capture fired but gated (advanced={advanced} sequence={seq})"
            ));
        }
        return;
    }
    if action == ACTION_TOGGLE {
        crate::sequence::write_diag(&format!(
            "hotkeys: toggle Activated (active before = {})",
            shared.is_active()
        ));
        apply_toggle(shared);
    }
}

/// The single start/stop key follows the in-app activation mode, exactly like
/// the Windows rdev listener. Hold mode acts on press + release.
fn apply_toggle(shared: &SharedState) {
    let mode = match shared.settings.read() {
        Ok(s) => s.activation_mode,
        Err(_) => return,
    };
    match mode {
        ActivationMode::Toggle => {
            let new_state = !shared.is_active();
            shared.set_active(new_state);
        }
        ActivationMode::Hold => shared.set_active(true),
    }
}

fn on_deactivated(shared: &Arc<SharedState>, action: &str) {
    if action == ACTION_TOGGLE {
        let hold = matches!(
            shared.settings.read().map(|s| s.activation_mode),
            Ok(ActivationMode::Hold)
        );
        if hold {
            crate::sequence::write_diag("hotkeys: toggle Deactivated (hold) -> active = false");
            shared.set_active(false);
        }
    }
}

/// Save the portal's reported binding for an action so the UI can show it
/// read-only. Empty means "None". The valid flag tells the UI to drop the
/// factory-default placeholder once the portal has reported anything.
fn store_binding(shared: &SharedState, action: &str, trigger: &str) {
    match action {
        ACTION_TOGGLE => {
            if let Ok(mut binding) = shared.kg_binding_toggle.write() {
                *binding = trigger.to_string();
            }
            shared
                .kg_binding_toggle_valid
                .store(true, Ordering::Relaxed);
        }
        ACTION_CAPTURE => {
            if let Ok(mut binding) = shared.kg_binding_capture.write() {
                *binding = trigger.to_string();
            }
            shared
                .kg_binding_capture_valid
                .store(true, Ordering::Relaxed);
        }
        _ => {}
    }
}

/// Register APP_ID with the portal's host-app Registry so the frontend reports
/// OUR app id instead of deriving it from the process cgroup. Launched from
/// another app's terminal (e.g. a VSCodium integrated terminal) we inherit the
/// parent's systemd `app-*.scope`, which would catalogue us as that app -- a
/// wrong component in System Settings and a wrong Edit page. Registration is
/// keyed per D-Bus sender, so it must run on the same session-bus connection
/// ashpd later uses, before the first create_session. Requires portal >=
/// 1.19.4 and an installed `QuietAutoClicker.desktop` (see install_linux_integration).
/// Failures are stderr-only: portal errors never surface in the UI.
async fn register_app_id(conn: &zbus::Connection) -> bool {
    // A cold start can race the portal service still coming up; retry a few
    // times rather than failing on the first UnknownMethod.
    for attempt in 1..=8 {
        if try_register_app_id(conn).await {
            return true;
        }
        if attempt < 8 {
            async_io::Timer::after(Duration::from_millis(300)).await;
        }
    }
    eprintln!("[hotkeys] register_app_id: giving up after 8 attempts");
    false
}

async fn try_register_app_id(conn: &zbus::Connection) -> bool {
    let options: std::collections::HashMap<&str, zbus::zvariant::Value> =
        std::collections::HashMap::new();
    // The Registry moved into the "host portal" namespace in newer releases;
    // the old name was used by 1.19.4-1.20. Try current, then legacy.
    for iface in [
        "org.freedesktop.host.portal.Registry",
        "org.freedesktop.portal.Registry",
    ] {
        let proxy = match zbus::Proxy::new(
            conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            iface,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[hotkeys] register_app_id: proxy failed: {e}");
                continue;
            }
        };
        match proxy
            .call_method("Register", &(APP_ID, options.clone()))
            .await
        {
            Ok(_) => {
                eprintln!("[hotkeys] registered app id: {APP_ID}");
                return true;
            }
            Err(e) => {
                // Interface missing (portal predates Register) or still starting up.
                eprintln!("[hotkeys] register_app_id ({iface}) failed: {e}");
            }
        }
    }
    false
}

/// Make sure the `QuietAutoClicker` component and its actions exist in
/// kglobalacceld BEFORE the portal session. KGlobalAccel keeps components in
/// memory only while something registers them (the portal re-registers ours
/// when its session closes), so on a cold start the component can be entirely
/// absent or present but emptied of our actions. portal-kde would then treat
/// our actions as brand new on every `bind_shortcuts` and show the add-bindings
/// QuickDialog, whose reply can stall the whole listener thread. Mirror the
/// registration portal-kde itself performs on session close (`doRegister`, one
/// call per action) for exactly the actions that are missing. We never touch
/// keys here: kglobalacceld loads the stored bindings from kglobalshortcutsrc,
/// so user-configured keys survive.
async fn ensure_kglobalaccel_component(conn: &zbus::Connection) {
    let proxy = match zbus::Proxy::new(
        conn,
        "org.kde.kglobalaccel",
        "/kglobalaccel",
        "org.kde.KGlobalAccel",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[hotkeys] kglobalaccel proxy failed: {e}");
            return;
        }
    };
    // A missing component makes allActionsForComponent fail; an emptied one
    // returns an empty list. Either way we only re-register what is gone.
    let existing: std::collections::HashSet<String> = match proxy
        .call_method("allActionsForComponent", &(vec![APP_ID.to_string()],))
        .await
    {
        Ok(reply) => match reply.body().deserialize::<Vec<Vec<String>>>() {
            Ok(lists) => lists
                .into_iter()
                .next()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            Err(_) => std::collections::HashSet::new(),
        },
        Err(_) => std::collections::HashSet::new(),
    };
    for (action, desc) in [
        (ACTION_TOGGLE, "Toggle AutoClick"),
        (ACTION_CAPTURE, "Add Sequence Point"),
    ] {
        if existing.contains(action) {
            continue;
        }
        let args: Vec<String> = vec![
            APP_ID.to_string(),
            action.to_string(),
            APP_ID.to_string(),
            desc.to_string(),
        ];
        if let Err(e) = proxy.call_method("doRegister", &(args,)).await {
            eprintln!("[hotkeys] pre-register component action {action}: {e}");
        }
    }
}

async fn refresh_bindings(
    gs: &GlobalShortcuts,
    session: &ashpd::desktop::Session<GlobalShortcuts>,
    shared: &SharedState,
) {
    let request = match gs
        .list_shortcuts(session, ListShortcutsOptions::default())
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[hotkeys] list_shortcuts failed: {e}");
            return;
        }
    };
    let resp = match request.response() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[hotkeys] list_shortcuts response: {e}");
            return;
        }
    };
    for shortcut in resp.shortcuts() {
        store_binding(shared, shortcut.id(), shortcut.trigger_description());
    }
}

enum Event {
    Activated(String),
    Deactivated(String),
    Changed,
    Tick,
}

fn run(shared: Arc<SharedState>) {
    crate::sequence::write_diag("hotkeys: linux listener starting");
    block_on(async move {
        // The Registry override is keyed per D-Bus sender, so the session must
        // be created on this same connection or portal-kde still derives the
        // component name from the cgroup (e.g. lowercased exe name).
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[hotkeys] no session bus: {e}");
                crate::sequence::write_diag(&format!("hotkeys: no session bus: {e}"));
                shared.use_kglobalaccel.store(false, Ordering::Relaxed);
                return;
            }
        };
        crate::sequence::write_diag("hotkeys: session bus connected");
        if !register_app_id(&conn).await {
            // Never bind under a guessed id: the portal would catalogue us as
            // the launching terminal's app (e.g. VSCodium) and create a stale
            // duplicate component in System Settings.
            crate::sequence::write_diag("hotkeys: app-id registration failed -> disabled");
            shared.use_kglobalaccel.store(false, Ordering::Relaxed);
            return;
        }
        crate::sequence::write_diag("hotkeys: app-id registered (QuietAutoClicker)");
        // Pre-register the actions so portal-kde never takes the "new actions"
        // dialog path (see ensure_kglobalaccel_component).
        ensure_kglobalaccel_component(&conn).await;
        let gs = match GlobalShortcuts::with_connection(conn).await {
            Ok(g) => g,
            Err(e) => {
                shared.use_kglobalaccel.store(false, Ordering::Relaxed);
                eprintln!("[hotkeys] GlobalShortcuts portal unavailable: {e}");
                crate::sequence::write_diag(&format!("hotkeys: portal unavailable: {e}"));
                return;
            }
        };
        let session = match gs.create_session(CreateSessionOptions::default()).await {
            Ok(s) => s,
            Err(e) => {
                shared.use_kglobalaccel.store(false, Ordering::Relaxed);
                eprintln!("[hotkeys] create_session failed: {e}");
                crate::sequence::write_diag(&format!("hotkeys: create_session failed: {e}"));
                return;
            }
        };
        crate::sequence::write_diag("hotkeys: globalshortcuts session created");
        shared.use_kglobalaccel.store(true, Ordering::Relaxed);

        let shortcuts = [
            NewShortcut::new(ACTION_TOGGLE, "Toggle AutoClick")
                .preferred_trigger(TRIGGER_TOGGLE_DEFAULT),
            NewShortcut::new(ACTION_CAPTURE, "Add Sequence Point")
                .preferred_trigger(TRIGGER_CAPTURE_DEFAULT),
        ];
        match gs
            .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
            .await
        {
            Ok(request) => {
                // Consume the bind reply on a detached thread: `response()`
                // blocks until the portal emits the Response signal, and on the
                // "new actions" path portal-kde also stalls it behind a
                // QuickDialog. Blocking here used to freeze the whole listener
                // (no polling, no hotkey events, bindings shown as "None");
                // detached, the 800ms poll keeps driving. The reply carries the
                // current shortcuts, an extra early source for the display.
                let shared2 = Arc::clone(&shared);
                std::thread::spawn(move || match request.response() {
                    Ok(resp) => {
                        for shortcut in resp.shortcuts() {
                            store_binding(&shared2, shortcut.id(), shortcut.trigger_description());
                        }
                    }
                    Err(e) => eprintln!("[hotkeys] bind_shortcuts cancelled/failed: {e}"),
                });
            }
            Err(e) => {
                eprintln!("[hotkeys] bind_shortcuts failed: {e}");
                crate::sequence::write_diag(&format!("hotkeys: bind_shortcuts failed: {e}"));
            }
        }
        // Let the portal settle after bind before the first listing.
        async_io::Timer::after(Duration::from_millis(300)).await;
        refresh_bindings(&gs, &session, &shared).await;

        // Expose a command channel so the UI can ask the listener (the sole
        // owner of the portal handles) to open the shortcut config page.
        let (cmd_tx, cmd_rx) = channel::<Cmd>();
        if let Ok(mut slot) = CMD_TX.get_or_init(|| Mutex::new(None)).lock() {
            *slot = Some(cmd_tx);
        }

        // The portal unicasts these signals to the session-owning app on
        // /org/freedesktop/portal/desktop, so the ashpd streams only ever carry
        // our own session's events. A failed subscription is NOT fatal: fall
        // back to an empty stream and keep the poll tick running so the
        // read-only binding display still refreshes.
        let activated: Box<dyn Stream<Item = Event> + Unpin + Send> =
            match gs.receive_activated().await {
                Ok(s) => Box::new(s.map(|a| Event::Activated(a.shortcut_id().to_string()))),
                Err(e) => {
                    eprintln!("[hotkeys] receive_activated failed: {e}");
                    crate::sequence::write_diag(&format!("hotkeys: receive_activated failed: {e}"));
                    Box::new(futures::stream::empty())
                }
            };
        let deactivated: Box<dyn Stream<Item = Event> + Unpin + Send> = match gs
            .receive_deactivated()
            .await
        {
            Ok(s) => Box::new(s.map(|d| Event::Deactivated(d.shortcut_id().to_string()))),
            Err(e) => {
                eprintln!("[hotkeys] receive_deactivated failed: {e}");
                crate::sequence::write_diag(&format!("hotkeys: receive_deactivated failed: {e}"));
                Box::new(futures::stream::empty())
            }
        };
        let changed: Box<dyn Stream<Item = Event> + Unpin + Send> =
            match gs.receive_shortcuts_changed().await {
                Ok(s) => Box::new(s.map(|_| Event::Changed)),
                Err(e) => {
                    eprintln!("[hotkeys] receive_shortcuts_changed failed: {e}");
                    crate::sequence::write_diag(&format!(
                        "hotkeys: receive_shortcuts_changed failed: {e}"
                    ));
                    Box::new(futures::stream::empty())
                }
            };
        let tick: Box<dyn Stream<Item = Event> + Unpin + Send> =
            Box::new(async_io::Timer::interval(POLL_INTERVAL).map(|_| Event::Tick));

        let mut events = select_all([activated, deactivated, changed, tick]);
        while let Some(event) = events.next().await {
            if shared.should_stop() {
                break;
            }
            match event {
                Event::Activated(action) => on_activated(&shared, &action),
                Event::Deactivated(action) => on_deactivated(&shared, &action),
                Event::Changed => refresh_bindings(&gs, &session, &shared).await,
                Event::Tick => {
                    refresh_bindings(&gs, &session, &shared).await;
                    // Drain any commands posted from the UI thread.
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        match cmd {
                            Cmd::OpenShortcutSettings => {
                                if let Err(e) = gs
                                    .configure_shortcuts(
                                        &session,
                                        None,
                                        ConfigureShortcutsOptions::default(),
                                    )
                                    .await
                                {
                                    eprintln!("[hotkeys] configure_shortcuts failed: {e}");
                                }
                            }
                        }
                    }
                }
            }
        }

        // Best-effort close; the portal also ends the session when our
        // session-bus connection drops on exit.
        let _ = session.close().await;
    });
}

/// Start the GlobalShortcuts portal listener (Linux). Registers the
/// toggle/capture actions with the XDG GlobalShortcuts portal so they show up
/// in KDE System Settings -> Keyboard -> Shortcuts while the app runs (first
/// launch asks to add them, defaults F6/F7). The single start/stop key
/// (`toggle`) follows the in-app Toggle/Hold mode. Runs on a dedicated thread
/// and exits when `shared` signals stop.
pub fn start(shared: Arc<SharedState>) {
    std::thread::spawn(move || run(shared));
}

/// Ask the listener thread to open the KDE System Settings shortcut page for
/// our component (via the portal's `ConfigureShortcuts`, portal-kde opens
/// `systemsettings://kcm_keys/<component>`). UI-thread safe: it only posts a
/// command over the channel drained by the listener.
pub fn open_shortcut_settings() {
    if let Some(guard) = CMD_TX.get().and_then(|m| m.lock().ok()) {
        if let Some(sender) = guard.as_ref() {
            let _ = sender.send(Cmd::OpenShortcutSettings);
        }
    }
}

/// Nothing to release: `run` closes the portal session when it sees the stop
/// flag, and the portal auto-ends it if the connection drops.
pub fn shutdown() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_mode_flips_active_state() {
        let shared = SharedState::new();
        shared.settings.write().unwrap().activation_mode = ActivationMode::Toggle;

        apply_toggle(&shared);
        assert!(shared.is_active());
        apply_toggle(&shared);
        assert!(!shared.is_active());
    }

    #[test]
    fn hold_mode_sets_active_only() {
        let shared = SharedState::new();
        shared.settings.write().unwrap().activation_mode = ActivationMode::Hold;
        shared.set_active(false);

        apply_toggle(&shared);
        assert!(shared.is_active());
        apply_toggle(&shared);
        assert!(shared.is_active(), "hold mode must never flip to off");
    }

    #[test]
    fn deactivated_turns_off_only_in_hold_mode() {
        let shared = SharedState::new();
        shared.set_active(true);
        shared.settings.write().unwrap().activation_mode = ActivationMode::Hold;
        on_deactivated(&shared, ACTION_TOGGLE);
        assert!(!shared.is_active());

        shared.set_active(true);
        shared.settings.write().unwrap().activation_mode = ActivationMode::Toggle;
        on_deactivated(&shared, ACTION_TOGGLE);
        assert!(shared.is_active(), "release must not stop toggle mode");
    }

    #[test]
    fn deactivated_ignores_other_actions() {
        let shared = SharedState::new();
        shared.set_active(true);
        shared.settings.write().unwrap().activation_mode = ActivationMode::Hold;
        on_deactivated(&shared, ACTION_CAPTURE);
        assert!(shared.is_active());
    }

    #[test]
    fn store_binding_records_and_marks_valid() {
        let shared = SharedState::new();
        store_binding(&shared, ACTION_TOGGLE, "Ctrl+Alt+A");
        assert_eq!(*shared.kg_binding_toggle.read().unwrap(), "Ctrl+Alt+A");
        assert!(shared.kg_binding_toggle_valid.load(Ordering::Relaxed));

        store_binding(&shared, ACTION_CAPTURE, "");
        assert_eq!(*shared.kg_binding_capture.read().unwrap(), "");
        assert!(shared.kg_binding_capture_valid.load(Ordering::Relaxed));
    }

    #[test]
    fn store_binding_ignores_unknown_action() {
        let shared = SharedState::new();
        store_binding(&shared, "unknown", "F1");
        assert!(!shared.kg_binding_toggle_valid.load(Ordering::Relaxed));
        assert!(!shared.kg_binding_capture_valid.load(Ordering::Relaxed));
    }
}
