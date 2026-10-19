#![windows_subsystem = "windows"]

use std::sync::Arc;

use slint::{winit_030::WinitWindowAccessor, ComponentHandle};

mod app;
mod engine;
mod sequence;
mod ui;

#[cfg(unix)]
mod hotkeys;
#[cfg(unix)]
mod kwincursor;
#[cfg(unix)]
mod linux;
#[cfg(windows)]
mod listener;
#[cfg(unix)]
mod wayland_fake_kb;
#[cfg(unix)]
mod wayland_kb;

fn main() -> anyhow::Result<()> {
    // Windows: render with the CPU software renderer so the frameless window's
    // per-pixel alpha is reliable everywhere — in VMs/RDP where OpenGL alpha of
    // the default femtovg renderer fails, the transparent base turns opaque and
    // the rounded card's corners show as a square. This must be set before any
    // Slint/winit window is created.
    #[cfg(windows)]
    unsafe {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }

    // Single instance: a second launch asks the running instance to focus its
    // window and exits before anything is created. The bound loopback port IS
    // the lock, so nothing extra is written to disk.
    let single = match acquire_single_instance() {
        Some(listener) => listener,
        None => return Ok(()),
    };

    // Diagnostics are opt-in: only when launched with `--debug` does the app
    // write a fresh qac_debug.log per run. Release runs write no log at all.
    let debug = std::env::args().any(|a| a == "--debug");
    sequence::set_debug(debug);

    let shared = app::SharedState::new();

    #[cfg(unix)]
    linux::install_linux_integration();

    // Wipe the per-launch diagnostic log BEFORE anything writes to it (the
    // listener/hotkeys thread starts it too, but doing it here on the main
    // thread makes log ordering deterministic — otherwise a concurrent truncate
    // can race with an early write_diag line and erase it).
    sequence::reset_diag_log();

    #[cfg(windows)]
    listener::start(Arc::clone(&shared));
    #[cfg(unix)]
    hotkeys::start(Arc::clone(&shared));
    #[cfg(unix)]
    kwincursor::start();
    let engine_handle = engine::start(Arc::clone(&shared));

    let ui = ui::setup(Arc::clone(&shared))?;

    // Wayland app_id: KWin resolves the window to a .desktop via this id, so
    // without it the taskbar shows a generic icon. set_xdg_app_id is Wayland/
    // X11-only (cfg(unix) in slint) and must run before the window is shown.
    #[cfg(unix)]
    let _ = slint::set_xdg_app_id("QuietAutoClicker");

    // Native window layer is fully transparent (the rounded Rectangle in
    // main.slint is the only drawn surface; the default opaque background would
    // show through at the bottom edge). On Windows also drop the invisible
    // DWM resize border of a resizable frameless window — its hit-test strip
    // can render as a dark band below the content.
    configure_window(&ui);

    // Serve "focus me" requests from later launches (see acquire_single_instance).
    spawn_activate_listener(single, ui.as_weak());

    // Taskbar/launcher icon on Wayland: KWin resolves the window to a .desktop
    // file by app_id, which we force up front with slint::set_xdg_app_id above.
    // The installed entry is still named exactly `QuietAutoClicker.desktop` to
    // match that id (and the binary); the legacy comment claimed no runtime
    // setter existed in Slint 1.17.1 — it does, so the app_id is now explicit.

    let weak = ui.as_weak();
    ui.on_drag_window(move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
        }
    });

    ui.on_close_window(move || {
        let _ = slint::quit_event_loop();
    });

    let weak = ui.as_weak();
    ui.on_minimize_window(move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().set_minimized(true);
        }
    });

    {
        let shared_aot = Arc::clone(&shared);
        let weak_aot = ui.as_weak();
        ui.on_toggle_always_on_top(move || {
            if let Some(ui) = weak_aot.upgrade() {
                let mut settings = shared_aot.settings.write().unwrap();
                settings.always_on_top = !settings.always_on_top;
                set_window_always_top(&ui, settings.always_on_top);
                ui.set_is_always_on_top(settings.always_on_top);
            }
        });
    }

    {
        let aot = shared.settings.read().unwrap().always_on_top;
        set_window_always_top(&ui, aot);
        ui.set_is_always_on_top(aot);
    }

    slint::invoke_from_event_loop({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                #[cfg(windows)]
                log_window_diagnostics(&ui);
                center_window(&ui);
            }
        }
    })?;

    ui.run()?;

    shared.signal_stop();
    let _ = engine_handle.join();

    #[cfg(unix)]
    hotkeys::shutdown();

    Ok(())
}

/// Window-level setup that needs the created window. The Slint window already
/// declares `background: transparent` (native layer fully transparent, only the
/// rounded Rectangle in main.slint is drawn). On Windows a frameless *resizable*
/// window gets an invisible DWM resize border whose hit-test strip can render
/// as an opaque dark band at the bottom/right edge — dropping resizability
/// removes it. We deliberately do NOT apply DWM corner rounding here: the
/// rounded Rectangle already rounds the corners, and applying
/// DWMWA_WINDOW_CORNER_PREFERENCE to the transparent (layered) window paints
/// its background opaque.
fn configure_window(ui: &impl ComponentHandle) {
    #[cfg(windows)]
    {
        ui.window().with_winit_window(|winit_window| {
            winit_window.set_resizable(false);
        });
        // The window isn't realized yet at this point, so the layered/corner
        // diagnostic usually can't read its handle here; it's re-run once after
        // realization via invoke_from_event_loop (see main()).
        log_window_diagnostics(ui);
    }
    #[cfg(not(windows))]
    let _ = ui;
}

/// Transparency diagnostics: report the window-style bits (is the window
/// actually per-pixel-alpha/WS_EX_LAYERED) and the DWM corner preference, so a
/// wrongly opaque background is diagnosable from qac_debug.log on the next run.
#[cfg(windows)]
fn log_window_diagnostics(ui: &impl ComponentHandle) {
    use raw_window_handle::HasWindowHandle;

    ui.window().with_winit_window(|winit_window| {
        let Ok(handle) = winit_window.window_handle() else {
            return;
        };
        let raw = handle.as_raw();
        let raw_window_handle::RawWindowHandle::Win32(win32) = raw else {
            return;
        };
        let hwnd = win32.hwnd.get() as windows_sys::Win32::Foundation::HWND;

        let mut corner_pref: u32 = 0;
        let _ = unsafe {
            windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                hwnd,
                33,
                &mut corner_pref as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        crate::sequence::write_diag(&format!(
            "window: layered={} corner_pref={}",
            is_layered(hwnd),
            corner_pref
        ));
    });
}

/// WS_EX_LAYERED is the style winit sets when the window was created with
/// transparency requested (per-pixel alpha). Absent ⇒ not transparent.
#[cfg(windows)]
fn is_layered(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED,
    };
    unsafe {
        let exstyle = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        exstyle & (WS_EX_LAYERED as isize) != 0
    }
}

#[cfg(windows)]
fn set_window_always_top(ui: &impl ComponentHandle, always_on_top: bool) {
    use raw_window_handle::HasWindowHandle;

    ui.window().with_winit_window(|winit_window| {
        let Ok(handle) = winit_window.window_handle() else {
            return;
        };
        let raw = handle.as_raw();
        let raw_window_handle::RawWindowHandle::Win32(win32) = raw else {
            return;
        };
        let hwnd = win32.hwnd.get() as windows_sys::Win32::Foundation::HWND;

        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd,
                if always_on_top {
                    windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOPMOST
                } else {
                    windows_sys::Win32::UI::WindowsAndMessaging::HWND_NOTOPMOST
                },
                0,
                0,
                0,
                0,
                windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
            );
        }
    });
}

#[cfg(not(windows))]
fn set_window_always_top(_ui: &impl ComponentHandle, _always_on_top: bool) {}

fn center_window(ui: &impl ComponentHandle) {
    if let Some(Some(pos)) = ui.window().with_winit_window(|winit_window| {
        let monitor = winit_window
            .current_monitor()
            .or_else(|| winit_window.primary_monitor())?;
        let mpos = monitor.position();
        let msize = monitor.size();
        let ws = ui.window().size();
        let (w, h) = if ws.width > 0 && ws.height > 0 {
            (ws.width, ws.height)
        } else {
            let scale = ui.window().scale_factor() as u32;
            (370 * scale, scale)
        };
        let x = mpos.x + (msize.width.saturating_sub(w) / 2) as i32;
        let y = mpos.y + (msize.height.saturating_sub(h) / 2) as i32;
        Some(slint::PhysicalPosition::new(x, y))
    }) {
        ui.window().set_position(pos);
    }
}

/// Loopback port that doubles as the single-instance lock: whoever binds it is
/// the primary instance. Fixed to an uncommon value to avoid clashing with
/// other software; nothing is ever written to disk for this.
const SINGLE_INSTANCE_PORT: u16 = 47321;

/// Become the primary instance by binding the loopback port, or hand the
/// launch over to the running instance. Returns the listener when this process
/// should continue; the caller must move it into `spawn_activate_listener` so
/// it stays alive for the whole app.
fn acquire_single_instance() -> Option<std::net::TcpListener> {
    use std::io::Write;
    use std::net::{SocketAddr, TcpListener, TcpStream};

    let addr = SocketAddr::from(([127, 0, 0, 1], SINGLE_INSTANCE_PORT));
    match TcpListener::bind(addr) {
        Ok(listener) => Some(listener),
        Err(_) => {
            // Another process owns the port. If it's our primary, ask it to
            // focus and give up. If the connect fails the owner just exited,
            // so retry the bind once to take over instead of not running.
            if let Ok(mut stream) = TcpStream::connect(addr) {
                let _ = stream.write_all(b"activate");
                return None;
            }
            TcpListener::bind(addr).ok()
        }
    }
}

/// Serve the "focus me" requests from later launches: each connection is
/// forwarded to the UI thread, which un-minimizes and focuses the window.
fn spawn_activate_listener<W: ComponentHandle + 'static>(
    listener: std::net::TcpListener,
    weak: slint::Weak<W>,
) {
    std::thread::spawn(move || {
        use std::io::Read;
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            let mut buf = [0u8; 16];
            let _ = conn.read(&mut buf);
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    ui.window().set_minimized(false);
                    ui.window().with_winit_window(|w| {
                        w.focus_window();
                    });
                }
            });
        }
    });
}
