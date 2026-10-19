use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, Model, Timer, TimerMode};

use crate::app::{self, SharedState};

slint::include_modules!();

type SeqModel = std::rc::Rc<std::cell::RefCell<Option<std::rc::Rc<slint::VecModel<i32>>>>>;

struct NumSync {
    prev_val: i32,
    prev_text: String,
}

impl NumSync {
    fn new(val: i32, text: &str) -> Self {
        Self {
            prev_val: val,
            prev_text: text.into(),
        }
    }
}

macro_rules! sync_ni {
    ($w:expr, $s:expr, $gt:ident, $st:ident, $gv:ident, $sv:ident, $min:expr, $max:expr) => {{
        let text: String = $w.$gt().into();
        let value = $w.$gv();
        if text != $s.prev_text {
            let parsed = text.parse::<i32>().unwrap_or(value).clamp($min, $max);
            if parsed != value {
                $w.$sv(parsed);
            }
            let canonical = parsed.to_string();
            if text != canonical {
                $w.$st(canonical.into());
            }
            $s.prev_text = $w.$gt().into();
            $s.prev_val = parsed;
        } else if value != $s.prev_val {
            let canonical = value.to_string();
            $w.$st(canonical.clone().into());
            $s.prev_text = canonical;
            $s.prev_val = value;
        } else {
            $s.prev_val = value;
            $s.prev_text = text;
        }
    }};
}

/// Build a canonical key or combo name from the in-app Slint capture path.
/// Printable keys arrive as their single character (uppercased to match rdev
/// naming); named keys were already mapped to engine names ("F1", "Space",
/// "Up", ...) by the .slint Key-namespace table. Modifiers are prefixed in the
/// fixed Ctrl -> Shift -> Alt -> Meta order, so e.g. Ctrl+Shift+J reads back
/// exactly as captured. Ctrl+letter also arrives as a control character on some
/// backends — map it back to the letter. Returns "" for a modifier-only press
/// or a raw key that can't be represented, so the callback can reject it.
fn normalize_capture(raw: &str, ctrl: bool, shift: bool, alt: bool, meta: bool) -> String {
    let mut key = raw.to_string();
    if let Some(c) = key.chars().next() {
        let cp = c as u32;
        if (1..=26).contains(&cp) {
            // Ctrl+A..Z surfaced as a control character.
            key = char::from_u32('A' as u32 + cp - 1).unwrap().to_string();
        } else if key.chars().count() == 1 {
            if !c.is_ascii() || c.is_ascii_control() {
                return String::new();
            }
            key = c.to_ascii_uppercase().to_string();
        }
    } else {
        // Modifier-only press (Ctrl/Alt/...), no main key.
        return String::new();
    }
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if shift {
        parts.push("Shift");
    }
    if alt {
        parts.push("Alt");
    }
    if meta {
        parts.push("Meta");
    }
    parts.push(&key);
    parts.join("+")
}

fn start_poll_timer(
    timer: &Timer,
    shared: Arc<SharedState>,
    weak: slint::Weak<MainWindow>,
    seq_x: SeqModel,
    seq_y: SeqModel,
    seq_z: SeqModel,
) {
    let mut cs_sync = NumSync::new(25, "25");
    let mut dh_sync = NumSync::new(0, "0");
    let mut dm_sync = NumSync::new(0, "0");
    let mut ds_sync = NumSync::new(0, "0");
    let mut dms_sync = NumSync::new(40, "40");
    let mut rv_sync = NumSync::new(35, "35");
    let mut lt_sync = NumSync::new(60, "60");
    let mut sm_sync = NumSync::new(80, "80");

    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let window = match weak.upgrade() {
            Some(w) => w,
            None => return,
        };
        let active = shared.is_active();
        window.set_is_active(active);

        let theme_dark = shared
            .settings
            .read()
            .ok()
            .map(|s| s.theme_dark)
            .unwrap_or(true);
        window.set_is_dark_theme(theme_dark);

        let aot = shared
            .settings
            .read()
            .ok()
            .map(|s| s.always_on_top)
            .unwrap_or(true);
        if aot != window.get_is_always_on_top() {
            window.set_is_always_on_top(aot);
        }

        let mut settings = match shared.settings.write() {
            Ok(s) => s,
            Err(_) => return,
        };

        let new_mode = window.get_rate_input_mode();
        if new_mode != settings.rate_input_mode.to_index() {
            settings.rate_input_mode = app::RateInputMode::from_index(new_mode);
        }

        sync_ni!(
            window,
            cs_sync,
            get_click_speed_text,
            set_click_speed_text,
            get_click_speed,
            set_click_speed,
            1,
            500
        );
        sync_ni!(
            window,
            dh_sync,
            get_duration_hours_text,
            set_duration_hours_text,
            get_duration_hours,
            set_duration_hours,
            0,
            999
        );
        sync_ni!(
            window,
            dm_sync,
            get_duration_minutes_text,
            set_duration_minutes_text,
            get_duration_minutes,
            set_duration_minutes,
            0,
            59
        );
        sync_ni!(
            window,
            ds_sync,
            get_duration_seconds_text,
            set_duration_seconds_text,
            get_duration_seconds,
            set_duration_seconds,
            0,
            59
        );
        sync_ni!(
            window,
            dms_sync,
            get_duration_milliseconds_text,
            set_duration_milliseconds_text,
            get_duration_milliseconds,
            set_duration_milliseconds,
            0,
            999
        );
        sync_ni!(
            window,
            rv_sync,
            get_random_variation_text,
            set_random_variation_text,
            get_random_variation,
            set_random_variation,
            0,
            200
        );
        sync_ni!(
            window,
            lt_sync,
            get_limit_time_sec_text,
            set_limit_time_sec_text,
            get_limit_time_sec,
            set_limit_time_sec,
            1,
            86400
        );
        sync_ni!(
            window,
            sm_sync,
            get_sequence_smooth_ms_text,
            set_sequence_smooth_ms_text,
            get_sequence_smooth_ms,
            set_sequence_smooth_ms,
            10,
            2000
        );

        // Sync NumberInput values → Rust settings
        let cs_v = window.get_click_speed() as u64;
        if cs_v != settings.click_speed {
            settings.click_speed = cs_v;
        }

        let ci = window.get_click_interval();
        if ci != settings.click_interval.to_index() {
            settings.click_interval = app::ClickInterval::from_index(ci);
        }

        let dh_v = window.get_duration_hours() as u64;
        if dh_v != settings.duration_hours {
            settings.duration_hours = dh_v;
        }

        let dm_v = window.get_duration_minutes() as u64;
        if dm_v != settings.duration_minutes {
            settings.duration_minutes = dm_v;
        }

        let ds_v = window.get_duration_seconds() as u64;
        if ds_v != settings.duration_seconds {
            settings.duration_seconds = ds_v;
        }

        let dms_v = window.get_duration_milliseconds() as u64;
        if dms_v != settings.duration_milliseconds {
            settings.duration_milliseconds = dms_v;
        }

        let rv = window.get_random_variation() as u64;
        if rv != settings.random_variation {
            settings.random_variation = rv;
        }

        let lt = window.get_limit_time_sec() as u64;
        if lt != settings.limit_time_secs {
            settings.limit_time_secs = lt;
        }

        let btn = window.get_mouse_button();
        if btn != settings.mouse_button.to_index() {
            settings.mouse_button = app::MouseButtonChoice::from_index(btn);
        }

        let it = window.get_input_type();
        if it != settings.input_type.to_index() {
            settings.input_type = app::InputType::from_index(it);
        }

        // Sequence Clicking is mouse-only: with Key selected, force it off so
        // the Enable box, its sub-checks and the F7 add-point path (engine and
        // hotkeys both gate on shared.sequence_enabled) are all inert. The
        // .slint section is also disabled/dimmed; switching back to Mouse
        // re-enables the checkbox.
        if it == 1 && window.get_sequence_enabled() {
            window.set_sequence_enabled(false);
        }

        let mode = window.get_activation_mode();
        if mode != settings.activation_mode.to_index() {
            settings.activation_mode = app::ActivationMode::from_index(mode);
        }

        let re = window.get_random_enabled();
        if re != settings.random_enabled {
            settings.random_enabled = re;
        }

        let lte = window.get_limit_time_enabled();
        if lte != settings.limit_time_enabled {
            settings.limit_time_enabled = lte;
        }

        let dc = window.get_double_click();
        if dc != settings.double_click {
            settings.double_click = dc;
        }

        // Sync hotkey from Rust → UI (listener sets settings.hotkey during capture)
        #[cfg(windows)]
        let listener_ok = shared
            .listener_ready
            .load(std::sync::atomic::Ordering::Relaxed)
            && shared
                .listener_error
                .read()
                .map(|e| e.is_empty())
                .unwrap_or(false);
        #[cfg(windows)]
        {
            let cur_hotkey = if listener_ok {
                settings.hotkey.clone()
            } else {
                // Listener down (enigo/grab failed): clear the stuck capture
                // flags so the chip can't show "Press..." forever (there is no
                // key event to ever complete the capture) and mirror the Linux
                // non-portal "unavailable" label. Details land in qac_debug.log.
                shared
                    .hotkey_capture_active
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                shared
                    .sequence_hotkey_capture_active
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                "unavailable".into()
            };
            let ui_hotkey: String = window.get_hotkey().into();
            if cur_hotkey != ui_hotkey {
                window.set_hotkey(cur_hotkey.into());
            }
            window.set_hotkey_editable(true);
        }
        #[cfg(unix)]
        {
            // Linux binds live in KDE System Settings via the GlobalShortcuts
            // portal (see hotkeys.rs). Show the factory default until the
            // portal reports a real value (valid flag); after that trust it,
            // including empty ("None").
            let binding = shared
                .kg_binding_toggle
                .read()
                .map(|b| b.clone())
                .unwrap_or_default();
            let label = if shared
                .use_kglobalaccel
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                if shared
                    .kg_binding_toggle_valid
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    if binding.is_empty() {
                        "None".into()
                    } else {
                        binding
                    }
                } else {
                    crate::hotkeys::TRIGGER_TOGGLE_DEFAULT.to_string()
                }
            } else {
                "unavailable".into()
            };
            let ui_hotkey: String = window.get_hotkey().into();
            if label != ui_hotkey {
                window.set_hotkey(label.into());
            }
            window.set_hotkey_editable(false);
        }

        #[cfg(windows)]
        {
            let capturing = shared
                .hotkey_capture_active
                .load(std::sync::atomic::Ordering::Relaxed);
            if capturing != window.get_is_capturing_hotkey() {
                window.set_is_capturing_hotkey(capturing);
            }
        }

        // Sync sequence hotkey (bind for adding sequence points): editable and
        // in-app capturable on Windows, read-only on Linux (KDE System Settings).
        let seq_label: String;
        #[cfg(windows)]
        {
            if listener_ok {
                seq_label = settings.sequence_hotkey.clone();
            } else {
                seq_label = "unavailable".into();
            }
            let ui_seq: String = window.get_sequence_hotkey().into();
            if seq_label != ui_seq {
                window.set_sequence_hotkey(seq_label.clone().into());
            }
            window.set_sequence_hotkey_editable(true);
            let capturing_seq_hk = shared
                .sequence_hotkey_capture_active
                .load(std::sync::atomic::Ordering::Relaxed);
            if capturing_seq_hk != window.get_is_capturing_sequence_hotkey() {
                window.set_is_capturing_sequence_hotkey(capturing_seq_hk);
            }
        }
        #[cfg(unix)]
        {
            let binding = shared
                .kg_binding_capture
                .read()
                .map(|b| b.clone())
                .unwrap_or_default();
            seq_label = if shared
                .use_kglobalaccel
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                if shared
                    .kg_binding_capture_valid
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    if binding.is_empty() {
                        "None".into()
                    } else {
                        binding
                    }
                } else {
                    crate::hotkeys::TRIGGER_CAPTURE_DEFAULT.to_string()
                }
            } else {
                "unavailable".into()
            };
            let ui_seq: String = window.get_sequence_hotkey().into();
            if seq_label != ui_seq {
                window.set_sequence_hotkey(seq_label.clone().into());
            }
            window.set_sequence_hotkey_editable(false);
        }

        // Click key: one-way Rust → UI. Both capture paths (the Windows rdev
        // listener and the in-app capture-input fallback) write
        // settings.click_key; the reset-X calls reset-click-key() which resets
        // the Rust model too, so no UI→settings mirroring is needed here.
        let cur_key: String = window.get_click_key().into();
        if cur_key != settings.click_key {
            window.set_click_key(settings.click_key.clone().into());
        }
        let capturing_key = shared
            .key_capture_active
            .load(std::sync::atomic::Ordering::Relaxed);
        if capturing_key != window.get_capturing_click_key() {
            window.set_capturing_click_key(capturing_key);
        }

        // Sync advanced mode
        let adv = window.get_advanced_mode();
        shared
            .advanced_mode
            .store(adv, std::sync::atomic::Ordering::Relaxed);

        // Sync sequence enabled
        let seq_en = window.get_sequence_enabled();
        shared
            .sequence_enabled
            .store(seq_en, std::sync::atomic::Ordering::Relaxed);

        // Sync sequence data — read atoms
        let (seq_points, _, _, _) = shared.get_sequence_data();
        let ui_loop = window.get_sequence_loop();
        let ui_smooth = window.get_sequence_smooth();
        let ui_smooth_ms = window.get_sequence_smooth_ms() as u64;

        let atom_loop = shared
            .sequence_loop
            .load(std::sync::atomic::Ordering::Relaxed);
        let atom_smooth = shared
            .sequence_smooth
            .load(std::sync::atomic::Ordering::Relaxed);
        let atom_smooth_ms = shared
            .sequence_smooth_ms
            .load(std::sync::atomic::Ordering::Relaxed);

        if ui_loop != atom_loop {
            shared
                .sequence_loop
                .store(ui_loop, std::sync::atomic::Ordering::Relaxed);
        }
        if ui_smooth != atom_smooth {
            shared
                .sequence_smooth
                .store(ui_smooth, std::sync::atomic::Ordering::Relaxed);
        }
        if ui_smooth_ms != atom_smooth_ms {
            shared
                .sequence_smooth_ms
                .store(ui_smooth_ms, std::sync::atomic::Ordering::Relaxed);
        }

        // Sync UI ← atoms (in case engine changed them — loop sets false on end)
        if window.get_sequence_loop()
            != shared
                .sequence_loop
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            window.set_sequence_loop(
                shared
                    .sequence_loop
                    .load(std::sync::atomic::Ordering::Relaxed),
            );
        }

        // Derive sequence_active: true if advanced mode + enabled + has points
        let has_points = !seq_points.is_empty();
        #[cfg(windows)]
        let prev_seq_active = shared
            .sequence_active
            .load(std::sync::atomic::Ordering::Relaxed);
        shared.sequence_active.store(
            adv && seq_en && has_points,
            std::sync::atomic::Ordering::Relaxed,
        );
        #[cfg(windows)]
        if (adv && seq_en && has_points) != prev_seq_active {
            crate::sequence::write_diag(&format!(
                "ui: sequence_active -> {} (advanced={adv} enabled={seq_en} points={})",
                adv && seq_en && has_points,
                seq_points.len()
            ));
        }

        // Passive hint: while Sequence Clicking is on, the capture hotkey adds
        // a point at the cursor (no explicit capture mode on either platform).
        let hint = if seq_en {
            match seq_label.as_str() {
                "" | "None" | "unavailable" => {
                    "Press the capture hotkey at each target position".to_string()
                }
                bind => format!("Press {bind} at each target position"),
            }
        } else {
            String::new()
        };
        let cur_hint: String = window.get_sequence_capture_hint().into();
        if hint != cur_hint {
            window.set_sequence_capture_hint(hint.clone().into());
            #[cfg(windows)]
            crate::sequence::write_diag(&format!("ui: sequence hint = '{hint}'"));
        }

        // Sync sequence points from Rust → UI (cached VecModels, set_row_data for perf)
        let new_count = seq_points.len() as i32;
        let old_count = window.get_sequence_points_count();
        let mut xm = seq_x.borrow_mut();
        let mut ym = seq_y.borrow_mut();
        let mut zm = seq_z.borrow_mut();

        if new_count != old_count || xm.is_none() {
            // Point count changed → the 170px list panel appears/disappears and
            // the window resizes on Wayland; release any stale titlebar grab so
            // the minimize/close buttons stay clickable afterwards (same
            // ui-reset mechanism as the advanced-mode toggle).
            if new_count != old_count {
                window.set_ui_reset(true);
            }
            let xs: Vec<i32> = seq_points.iter().map(|p| p.x).collect();
            let ys: Vec<i32> = seq_points.iter().map(|p| p.y).collect();
            let cs: Vec<i32> = seq_points.iter().map(|p| p.clicks as i32).collect();
            let xv: std::rc::Rc<slint::VecModel<i32>> = std::rc::Rc::new(xs.into());
            let yv: std::rc::Rc<slint::VecModel<i32>> = std::rc::Rc::new(ys.into());
            let cv: std::rc::Rc<slint::VecModel<i32>> = std::rc::Rc::new(cs.into());
            window.set_sequence_points_count(new_count);
            window.set_sequence_points_x(xv.clone().into());
            window.set_sequence_points_y(yv.clone().into());
            window.set_sequence_points_clicks(cv.clone().into());
            *xm = Some(xv);
            *ym = Some(yv);
            *zm = Some(cv);
        } else if let (Some(ref xr), Some(ref yr), Some(ref zr)) = (&*xm, &*ym, &*zm) {
            for (i, p) in seq_points.iter().enumerate() {
                if xr.row_data(i) != Some(p.x) {
                    xr.set_row_data(i, p.x);
                }
                if yr.row_data(i) != Some(p.y) {
                    yr.set_row_data(i, p.y);
                }
                if zr.row_data(i) != Some(p.clicks as i32) {
                    zr.set_row_data(i, p.clicks as i32);
                }
            }
        }
        drop(xm);
        drop(ym);
        drop(zm);
    });
}

pub fn setup(shared: Arc<SharedState>) -> Result<MainWindow, slint::PlatformError> {
    let window = MainWindow::new()?;

    let seq_x: SeqModel = Default::default();
    let seq_y: SeqModel = Default::default();
    let seq_z: SeqModel = Default::default();

    {
        let shared_cap = Arc::clone(&shared);
        window.on_request_capture_hotkey(move || {
            #[cfg(windows)]
            shared_cap
                .hotkey_capture_active
                .store(true, std::sync::atomic::Ordering::Relaxed);
            #[cfg(unix)]
            {
                let _ = &shared_cap;
                // On Linux the binding is set in KDE System Settings, nothing to capture.
            }
        });
    }

    {
        let shared_clr = Arc::clone(&shared);
        let weak_clr = window.as_weak();
        window.on_clear_hotkey(move || {
            #[cfg(windows)]
            {
                if let Ok(mut s) = shared_clr.settings.write() {
                    if s.hotkey == "F6" {
                        return;
                    }
                    s.hotkey = "F6".into();
                }
                if let Some(ui) = weak_clr.upgrade() {
                    ui.set_hotkey("F6".into());
                }
            }
            #[cfg(unix)]
            let _ = (&shared_clr, &weak_clr);
        });
    }

    {
        let shared_seq = Arc::clone(&shared);
        window.on_request_capture_sequence_hotkey(move || {
            #[cfg(windows)]
            shared_seq
                .sequence_hotkey_capture_active
                .store(true, std::sync::atomic::Ordering::Relaxed);
            #[cfg(unix)]
            let _ = &shared_seq;
        });
    }

    {
        let shared_clr = Arc::clone(&shared);
        let weak_clr = window.as_weak();
        window.on_clear_sequence_hotkey(move || {
            #[cfg(windows)]
            {
                if let Ok(mut s) = shared_clr.settings.write() {
                    if s.sequence_hotkey == "F7" {
                        return;
                    }
                    s.sequence_hotkey = "F7".into();
                }
                if let Some(ui) = weak_clr.upgrade() {
                    ui.set_sequence_hotkey("F7".into());
                }
            }
            #[cfg(unix)]
            let _ = (&shared_clr, &weak_clr);
        });
    }

    {
        // Linux (KDE Wayland): open the KDE System Settings shortcut page for
        // our component instead of in-app hotkey capture.
        window.on_open_shortcut_settings(move || {
            #[cfg(unix)]
            crate::hotkeys::open_shortcut_settings();
        });
    }

    {
        let shared_key = Arc::clone(&shared);
        window.on_request_capture_key(move || {
            // In-app capture on both platforms: the portal 'clickkey' action
            // was removed (KGlobalAccel grabbed the key system-wide), and on
            // Windows this also covers the listener-down fallback. The next
            // pressed key arrives here via the .slint capture-input path (or
            // via the rdev hook when it is up — that one clears the flag).
            shared_key
                .key_capture_active
                .store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }

    {
        let shared_reset = Arc::clone(&shared);
        let weak_reset = window.as_weak();
        window.on_reset_click_key(move || {
            if let Ok(mut s) = shared_reset.settings.write() {
                s.click_key = "A".into();
            }
            shared_reset
                .key_capture_active
                .store(false, std::sync::atomic::Ordering::Relaxed);
            if let Some(ui) = weak_reset.upgrade() {
                ui.set_click_key("A".into());
                ui.set_capturing_click_key(false);
            }
            crate::sequence::write_diag("ui: click key reset to A");
        });
    }

    {
        let shared_cap = Arc::clone(&shared);
        let weak_cap = window.as_weak();
        window.on_capture_input(
            move |text: slint::SharedString,
                  repeat: bool,
                  ctrl: bool,
                  shift: bool,
                  alt: bool,
                  meta: bool|
                  -> bool {
                // Raw per-event trace under --debug: records EXACTLY what the
                // backends report for every key while capture is armed (incl.
                // rejected events), so a lone-modifier press that fabricates a
                // letter (Shift -> "Shift+P") is visibly distinguishable from a
                // real combo before normalization.
                let captured_hotkey =
                    shared_cap.hotkey_capture_active.load(std::sync::atomic::Ordering::Relaxed);
                let captured_seq =
                    shared_cap.sequence_hotkey_capture_active.load(std::sync::atomic::Ordering::Relaxed);
                let captured_key =
                    shared_cap.key_capture_active.load(std::sync::atomic::Ordering::Relaxed);
                let armed = captured_hotkey || captured_seq || captured_key;
                crate::sequence::write_diag(&format!(
                    "capture-raw: text='{}' repeat={repeat} ctrl={ctrl} shift={shift} alt={alt} meta={meta} armed={armed}",
                    text.as_str().escape_default(),
                ));
                if !armed { return false; }
                if repeat {
                    crate::sequence::write_diag("capture-raw: -> reject repeat");
                    return false;
                }
                #[cfg(unix)]
                {
                    // On KDE Wayland lone modifier keypresses arrive as a C0
                    // control char (Ctrl->\x11->'Q', Shift->\x10->'P',
                    // Alt->\x12->'R') derived from a fabricated letter; real
                    // keys always arrive as real text ('J'). So any C0 control
                    // char in the text is a phantom modifier press: reject it
                    // and keep capture armed so the real (possibly combo) key
                    // that follows is captured instead.
                    if text.chars().any(|c| ('\u{1}'..='\u{1a}').contains(&c)) {
                        crate::sequence::write_diag(&format!(
                            "capture-raw: -> reject c0-control '{}'",
                            text.as_str().escape_default()
                        ));
                        return false;
                    }
                }
                let name = normalize_capture(text.as_str(), ctrl, shift, alt, meta);
                if name.is_empty() {
                    crate::sequence::write_diag("capture-raw: -> reject ''");
                    return false;
                }

                if captured_hotkey {
                    if let Ok(mut s) = shared_cap.settings.write() {
                        s.hotkey = name.clone();
                    }
                    shared_cap.hotkey_capture_active.store(false, std::sync::atomic::Ordering::Relaxed);
                }
                if captured_seq {
                    if let Ok(mut s) = shared_cap.settings.write() {
                        s.sequence_hotkey = name.clone();
                    }
                    shared_cap.sequence_hotkey_capture_active.store(false, std::sync::atomic::Ordering::Relaxed);
                }
                if captured_key {
                    if let Ok(mut s) = shared_cap.settings.write() {
                        s.click_key = name.clone();
                    }
                    shared_cap.key_capture_active.store(false, std::sync::atomic::Ordering::Relaxed);
                }

                if let Some(ui) = weak_cap.upgrade() {
                    if captured_hotkey {
                        ui.set_hotkey(name.clone().into());
                        ui.set_is_capturing_hotkey(false);
                        #[cfg(windows)]
                        crate::sequence::write_diag(&format!("ui: in-app hotkey set to {name}"));
                    }
                    if captured_seq {
                        ui.set_sequence_hotkey(name.clone().into());
                        ui.set_is_capturing_sequence_hotkey(false);
                        #[cfg(windows)]
                        crate::sequence::write_diag(&format!("ui: in-app sequence hotkey set to {name}"));
                    }
                    if captured_key {
                        ui.set_click_key(name.clone().into());
                        ui.set_capturing_click_key(false);
                        crate::sequence::write_diag(&format!("ui: in-app click key set to {name}"));
                    }
                }
                crate::sequence::write_diag(&format!("capture-raw: -> accept '{name}'"));
                true
            },
        );
    }

    {
        let shared_seq = Arc::clone(&shared);
        let sx = seq_x.clone();
        let sy = seq_y.clone();
        let sz = seq_z.clone();
        window.on_move_sequence_point(move |idx: i32, dir: i32| {
            let (mut points, r#loop, smooth, smooth_ms) = shared_seq.get_sequence_data();
            let count = points.len();
            if count >= 2 {
                let new_idx = (idx + dir).clamp(0, count as i32 - 1);
                if new_idx != idx {
                    let i = idx as usize;
                    let j = new_idx as usize;
                    points.swap(i, j);
                    shared_seq.set_sequence_data(points, r#loop, smooth, smooth_ms);
                    if let (Some(ref xr), Some(ref yr), Some(ref zr)) =
                        (&*sx.borrow(), &*sy.borrow(), &*sz.borrow())
                    {
                        let xi = xr.row_data(i).unwrap_or_default();
                        let xj = xr.row_data(j).unwrap_or_default();
                        let yi = yr.row_data(i).unwrap_or_default();
                        let yj = yr.row_data(j).unwrap_or_default();
                        let zi = zr.row_data(i).unwrap_or_default();
                        let zj = zr.row_data(j).unwrap_or_default();
                        xr.set_row_data(i, xj);
                        xr.set_row_data(j, xi);
                        yr.set_row_data(i, yj);
                        yr.set_row_data(j, yi);
                        zr.set_row_data(i, zj);
                        zr.set_row_data(j, zi);
                    }
                }
            }
        });
    }

    {
        let shared_seq = Arc::clone(&shared);
        window.on_delete_sequence_point(move |idx: i32| {
            let (mut points, r#loop, smooth, smooth_ms) = shared_seq.get_sequence_data();
            if idx >= 0 && (idx as usize) < points.len() {
                points.remove(idx as usize);
                shared_seq.set_sequence_data(points, r#loop, smooth, smooth_ms);
            }
        });
    }

    {
        let shared_seq = Arc::clone(&shared);
        window.on_set_sequence_point_x(move |idx: i32, val: slint::SharedString| {
            if let Ok(num) = val.parse::<i32>() {
                let (mut points, r#loop, smooth, smooth_ms) = shared_seq.get_sequence_data();
                if idx >= 0 && (idx as usize) < points.len() {
                    points[idx as usize].x = num;
                    shared_seq.set_sequence_data(points, r#loop, smooth, smooth_ms);
                }
            }
        });
    }

    {
        let shared_seq = Arc::clone(&shared);
        window.on_set_sequence_point_y(move |idx: i32, val: slint::SharedString| {
            if let Ok(num) = val.parse::<i32>() {
                let (mut points, r#loop, smooth, smooth_ms) = shared_seq.get_sequence_data();
                if idx >= 0 && (idx as usize) < points.len() {
                    points[idx as usize].y = num;
                    shared_seq.set_sequence_data(points, r#loop, smooth, smooth_ms);
                }
            }
        });
    }

    {
        let shared_seq = Arc::clone(&shared);
        window.on_set_sequence_point_clicks(move |idx: i32, val: slint::SharedString| {
            if let Ok(num) = val.parse::<u32>() {
                let clamped = num.clamp(1, 100);
                let (mut points, r#loop, smooth, smooth_ms) = shared_seq.get_sequence_data();
                if idx >= 0 && (idx as usize) < points.len() {
                    points[idx as usize].clicks = clamped;
                    shared_seq.set_sequence_data(points, r#loop, smooth, smooth_ms);
                }
            }
        });
    }

    {
        let weak_adv = window.as_weak();
        window.on_toggle_advanced_mode(move || {
            if let Some(ui) = weak_adv.upgrade() {
                let current = ui.get_advanced_mode();
                ui.set_advanced_mode(!current);
            }
        });
    }

    let timer = Box::new(Timer::default());
    start_poll_timer(&timer, shared, window.as_weak(), seq_x, seq_y, seq_z);
    Box::leak(timer);

    Ok(window)
}
