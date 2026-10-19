//! Wayland fake input fallback (KWin Plasma 6): org_kde_kwin_fake_input.
//! Provides keyboard_key with evdev codes when zwp_virtual_keyboard_manager_v1
//! is not exported (Plasma 6).

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use wayland_client::protocol::{wl_registry, wl_seat, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle};

pub mod fake_input {
    use wayland_client;

    pub mod __interfaces {
        use wayland_backend;
        wayland_scanner::generate_interfaces!("./src/wayland_fake_input.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("./src/wayland_fake_input.xml");
}

use fake_input::org_kde_kwin_fake_input::OrgKdeKwinFakeInput;

#[derive(Default)]
struct Globals {
    seat: Option<WlSeat>,
    fake: Option<OrgKdeKwinFakeInput>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" => {
                    state.seat = Some(registry.bind::<WlSeat, _, _>(name, version, qh, ()));
                }
                "org_kde_kwin_fake_input" => {
                    state.fake = Some(registry.bind::<OrgKdeKwinFakeInput, _, _>(
                        name,
                        version.min(5),
                        qh,
                        (),
                    ));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for Globals {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinFakeInput, ()> for Globals {
    fn event(
        _: &mut Self,
        _: &OrgKdeKwinFakeInput,
        _: <OrgKdeKwinFakeInput as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

struct FakeKb {
    conn: Connection,
    fake: OrgKdeKwinFakeInput,
    _start: Instant,
}

static FAKE: OnceLock<Result<Mutex<FakeKb>, String>> = OnceLock::new();

fn init() -> Result<Mutex<FakeKb>, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("connect: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());
    let mut globals = Globals::default();
    queue
        .roundtrip(&mut globals)
        .map_err(|e| format!("roundtrip: {e}"))?;
    let _seat = globals.seat.ok_or("no wl_seat")?;
    let fake = globals.fake.ok_or("no org_kde_kwin_fake_input global")?;
    fake.authenticate("QuietAutoClicker".into(), "keyboard input".into());
    conn.flush().map_err(|e| format!("flush: {e}"))?;
    std::mem::forget(queue);
    Ok(Mutex::new(FakeKb {
        conn,
        fake,
        _start: Instant::now(),
    }))
}

pub fn send_combo(_mods: &[u32], _main: u32) -> Result<(), String> {
    let res = FAKE.get_or_init(init);
    let m = res.as_ref().map_err(|e| e.clone())?;
    let k = m.lock().map_err(|_| "lock poisoned".to_string())?;
    for mc in _mods {
        k.fake.keyboard_key(*mc, 1);
    }
    k.fake.keyboard_key(_main, 1);
    if !_mods.is_empty() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    k.fake.keyboard_key(_main, 0);
    for mc in _mods.iter().rev() {
        k.fake.keyboard_key(*mc, 0);
    }
    k.conn.flush().map_err(|e| format!("flush: {e}"))
}

pub fn is_available() -> bool {
    FAKE.get().map(|r| r.is_ok()).unwrap_or(false)
}
