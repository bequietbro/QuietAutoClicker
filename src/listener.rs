use std::cell::Cell;
use std::sync::{atomic::Ordering, Arc};

use enigo::{Enigo, Mouse, Settings};
use rdev::{grab, Event, EventType, Key};

use crate::app::{ActivationMode, SharedState};

// Key <-> string mapping helpers. Only the Windows rdev listener uses these
// (in-app hotkey capture); they landed here when the old hotkey.rs was deleted.

fn is_modifier(key: &Key) -> bool {
    matches!(
        key,
        Key::ControlLeft
            | Key::ControlRight
            | Key::ShiftLeft
            | Key::ShiftRight
            | Key::Alt
            | Key::AltGr
            | Key::MetaLeft
            | Key::MetaRight
    )
}

fn key_to_str(key: &Key) -> Option<String> {
    Some(
        match key {
            Key::KeyA => "A",
            Key::KeyB => "B",
            Key::KeyC => "C",
            Key::KeyD => "D",
            Key::KeyE => "E",
            Key::KeyF => "F",
            Key::KeyG => "G",
            Key::KeyH => "H",
            Key::KeyI => "I",
            Key::KeyJ => "J",
            Key::KeyK => "K",
            Key::KeyL => "L",
            Key::KeyM => "M",
            Key::KeyN => "N",
            Key::KeyO => "O",
            Key::KeyP => "P",
            Key::KeyQ => "Q",
            Key::KeyR => "R",
            Key::KeyS => "S",
            Key::KeyT => "T",
            Key::KeyU => "U",
            Key::KeyV => "V",
            Key::KeyW => "W",
            Key::KeyX => "X",
            Key::KeyY => "Y",
            Key::KeyZ => "Z",
            Key::Num0 => "0",
            Key::Num1 => "1",
            Key::Num2 => "2",
            Key::Num3 => "3",
            Key::Num4 => "4",
            Key::Num5 => "5",
            Key::Num6 => "6",
            Key::Num7 => "7",
            Key::Num8 => "8",
            Key::Num9 => "9",
            Key::F1 => "F1",
            Key::F10 => "F10",
            Key::F11 => "F11",
            Key::F12 => "F12",
            Key::F2 => "F2",
            Key::F3 => "F3",
            Key::F4 => "F4",
            Key::F5 => "F5",
            Key::F6 => "F6",
            Key::F7 => "F7",
            Key::F8 => "F8",
            Key::F9 => "F9",
            Key::Space => "Space",
            Key::Return => "Enter",
            Key::Tab => "Tab",
            Key::Escape => "Esc",
            Key::Backspace => "Backspace",
            Key::Delete => "Delete",
            Key::Insert => "Insert",
            Key::Home => "Home",
            Key::End => "End",
            Key::PageUp => "PageUp",
            Key::PageDown => "PageDown",
            Key::ShiftLeft | Key::ShiftRight => "Shift",
            Key::ControlLeft | Key::ControlRight => "Ctrl",
            Key::Alt => "Alt",
            Key::AltGr => "AltGr",
            Key::MetaLeft | Key::MetaRight => "Meta",
            Key::CapsLock => "CapsLock",
            Key::PrintScreen => "PrintScreen",
            Key::ScrollLock => "ScrollLock",
            Key::Pause => "Pause",
            Key::NumLock => "NumLock",
            Key::UpArrow => "Up",
            Key::DownArrow => "Down",
            Key::LeftArrow => "Left",
            Key::RightArrow => "Right",
            Key::Kp0 => "Num0",
            Key::Kp1 => "Num1",
            Key::Kp2 => "Num2",
            Key::Kp3 => "Num3",
            Key::Kp4 => "Num4",
            Key::Kp5 => "Num5",
            Key::Kp6 => "Num6",
            Key::Kp7 => "Num7",
            Key::Kp8 => "Num8",
            Key::Kp9 => "Num9",
            Key::KpPlus => "Num+",
            Key::KpMinus => "Num-",
            Key::KpMultiply => "Num*",
            Key::KpDivide => "Num/",
            Key::KpDelete => "Num.",
            Key::KpReturn => "NumEnter",
            Key::Minus => "-",
            Key::Equal => "=",
            Key::LeftBracket => "[",
            Key::RightBracket => "]",
            Key::SemiColon => ";",
            Key::Quote => "'",
            Key::BackSlash => "\\",
            Key::IntlBackslash => "\\",
            Key::Comma => ",",
            Key::Dot => ".",
            Key::Slash => "/",
            Key::BackQuote => "`",
            Key::Function => "Function",
            _ => return None,
        }
        .to_string(),
    )
}

fn str_to_key(s: &str) -> Option<Key> {
    Some(match s {
        "A" => Key::KeyA,
        "B" => Key::KeyB,
        "C" => Key::KeyC,
        "D" => Key::KeyD,
        "E" => Key::KeyE,
        "F" => Key::KeyF,
        "G" => Key::KeyG,
        "H" => Key::KeyH,
        "I" => Key::KeyI,
        "J" => Key::KeyJ,
        "K" => Key::KeyK,
        "L" => Key::KeyL,
        "M" => Key::KeyM,
        "N" => Key::KeyN,
        "O" => Key::KeyO,
        "P" => Key::KeyP,
        "Q" => Key::KeyQ,
        "R" => Key::KeyR,
        "S" => Key::KeyS,
        "T" => Key::KeyT,
        "U" => Key::KeyU,
        "V" => Key::KeyV,
        "W" => Key::KeyW,
        "X" => Key::KeyX,
        "Y" => Key::KeyY,
        "Z" => Key::KeyZ,
        "0" => Key::Num0,
        "1" => Key::Num1,
        "2" => Key::Num2,
        "3" => Key::Num3,
        "4" => Key::Num4,
        "5" => Key::Num5,
        "6" => Key::Num6,
        "7" => Key::Num7,
        "8" => Key::Num8,
        "9" => Key::Num9,
        "F1" => Key::F1,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "Space" => Key::Space,
        "Enter" => Key::Return,
        "Tab" => Key::Tab,
        "Esc" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Insert" => Key::Insert,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "CapsLock" => Key::CapsLock,
        "PrintScreen" => Key::PrintScreen,
        "ScrollLock" => Key::ScrollLock,
        "Pause" => Key::Pause,
        "NumLock" => Key::NumLock,
        "Up" => Key::UpArrow,
        "Down" => Key::DownArrow,
        "Left" => Key::LeftArrow,
        "Right" => Key::RightArrow,
        "Num0" => Key::Kp0,
        "Num1" => Key::Kp1,
        "Num2" => Key::Kp2,
        "Num3" => Key::Kp3,
        "Num4" => Key::Kp4,
        "Num5" => Key::Kp5,
        "Num6" => Key::Kp6,
        "Num7" => Key::Kp7,
        "Num8" => Key::Kp8,
        "Num9" => Key::Kp9,
        "Num+" => Key::KpPlus,
        "Num-" => Key::KpMinus,
        "Num*" => Key::KpMultiply,
        "Num/" => Key::KpDivide,
        "Num." => Key::KpDelete,
        "NumEnter" => Key::KpReturn,
        "-" => Key::Minus,
        "=" => Key::Equal,
        "[" => Key::LeftBracket,
        "]" => Key::RightBracket,
        ";" => Key::SemiColon,
        "'" => Key::Quote,
        "\\" => Key::BackSlash,
        "," => Key::Comma,
        "." => Key::Dot,
        "/" => Key::Slash,
        "`" => Key::BackQuote,
        "AltGr" => Key::AltGr,
        "Function" => Key::Function,
        _ => return None,
    })
}

fn parse_hotkey(hk: &str) -> Option<(Key, bool, bool, bool, bool)> {
    let parts: Vec<&str> = hk.split('+').collect();
    if parts.is_empty() {
        return None;
    }
    let key_str = parts.last()?;
    let key = str_to_key(key_str)?;
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut meta = false;
    for mod_str in &parts[..parts.len() - 1] {
        match *mod_str {
            "Ctrl" => ctrl = true,
            "Shift" => shift = true,
            "Alt" => alt = true,
            "Meta" => meta = true,
            _ => return None,
        }
    }
    Some((key, ctrl, shift, alt, meta))
}

fn format_hotkey(key: &Key, ctrl: bool, shift: bool, alt: bool, meta: bool) -> String {
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
    if let Some(k) = key_to_str(key) {
        parts.push(&k);
        parts.join("+")
    } else {
        "F6".to_string()
    }
}

fn hotkey_matches(hk: &str, key: &Key, ctrl: bool, shift: bool, alt: bool, meta: bool) -> bool {
    if let Some((target_key, tc, ts, ta, tm)) = parse_hotkey(hk) {
        *key == target_key && ctrl == tc && shift == ts && alt == ta && meta == tm
    } else {
        false
    }
}

pub fn start(shared: Arc<SharedState>) {
    std::thread::spawn(move || {
        let ctrl = Cell::new(false);
        let shift = Cell::new(false);
        let alt = Cell::new(false);
        let meta = Cell::new(false);

        // The per-launch log is reset in main() before the listener starts, so
        // entries here (and configure_window's transparency line) are never
        // wiped by a racing truncate. Just record our own start.
        crate::sequence::write_diag("listener: starting");

        let enigo = match Enigo::new(&Settings::default()) {
            Ok(e) => e,
            Err(e) => {
                let msg = format!("listener: enigo init failed: {e}");
                crate::sequence::write_diag(&msg);
                if let Ok(mut err) = shared.listener_error.write() {
                    *err = msg.clone();
                }
                eprintln!("[listener] Failed to create enigo: {e}");
                return;
            }
        };

        // rdev::grab installs the hook and then BLOCKS in the Windows message
        // pump until the app exits, so it never returns while the app is alive.
        // Mark the hook ready BEFORE calling it; reaching this line already means
        // enigo works and the reference to this thread is alive. On Err we flip
        // it back so the UI can show "unavailable".
        shared.listener_ready.store(true, Ordering::Relaxed);
        crate::sequence::write_diag("listener: entering grab loop");

        let shared_diag = Arc::clone(&shared);
        // Tracks which non-modifier keys are currently physically down so a held
        // key's auto-repeat does not re-trigger a hotkey: rdev reports every
        // WM_KEYDOWN repeat as a fresh KeyPress, which would otherwise flip a
        // Toggle back and forth (and spam hold activation) while the key is held.
        let down = std::cell::RefCell::new(std::collections::HashSet::<Key>::new());
        let result = grab(move |event: Event| -> Option<Event> {
            // Heartbeat: any received key event marks the hook alive, so the UI
            // can tell a dead listener from an idle one and qac_debug.log shows
            // whether events were ever delivered.
            let count = shared.listener_events.fetch_add(1, Ordering::Relaxed);
            if count == 0 {
                crate::sequence::write_diag("listener: first key event received");
            } else if count.is_multiple_of(1000) {
                crate::sequence::write_diag(&format!("listener: {count} key events"));
            }

            let (key, is_press) = match event.event_type {
                EventType::KeyPress(k) => (k, true),
                EventType::KeyRelease(k) => (k, false),
                _ => return Some(event),
            };

            let is_repeat_press = if is_press {
                // A press while the key is already marked down is auto-repeat:
                // swallow it so it never reaches the hotkey/capture logic below.
                !down.borrow_mut().insert(key)
            } else {
                down.borrow_mut().remove(&key);
                false
            };
            if is_repeat_press {
                return None;
            }

            match &key {
                Key::ControlLeft | Key::ControlRight => ctrl.set(is_press),
                Key::ShiftLeft | Key::ShiftRight => shift.set(is_press),
                Key::Alt => alt.set(is_press),
                Key::MetaLeft | Key::MetaRight => meta.set(is_press),
                _ => {}
            }

            if !is_press {
                let mode = match shared.settings.read() {
                    Ok(s) => s.activation_mode,
                    Err(_) => return Some(event),
                };
                if matches!(mode, ActivationMode::Hold) {
                    let hotkey = match shared.settings.read() {
                        Ok(s) => s.hotkey.clone(),
                        Err(_) => return Some(event),
                    };
                    if hotkey_matches(
                        &hotkey,
                        &key,
                        ctrl.get(),
                        shift.get(),
                        alt.get(),
                        meta.get(),
                    ) {
                        shared.set_active(false);
                        crate::sequence::write_diag(
                            "listener: toggle hotkey hold release -> active = false",
                        );
                        return None;
                    }
                }
                return Some(event);
            }

            if is_modifier(&key) {
                return Some(event);
            }

            if shared.hotkey_capture_active.load(Ordering::Relaxed) {
                let hk = format_hotkey(&key, ctrl.get(), shift.get(), alt.get(), meta.get());
                if hk.split('+').count() <= 3 {
                    if let Ok(mut s) = shared.settings.write() {
                        s.hotkey = hk.clone();
                    }
                    shared.hotkey_capture_active.store(false, Ordering::Relaxed);
                    crate::sequence::write_diag(&format!("listener: toggle hotkey set to {hk}"));
                    return None;
                }
                // Unusable combo (too many modifiers): swallow nothing, keep capturing.
            }

            if shared
                .sequence_hotkey_capture_active
                .load(Ordering::Relaxed)
            {
                let hk = format_hotkey(&key, ctrl.get(), shift.get(), alt.get(), meta.get());
                if hk.split('+').count() <= 3 {
                    if let Ok(mut s) = shared.settings.write() {
                        s.sequence_hotkey = hk.clone();
                    }
                    shared
                        .sequence_hotkey_capture_active
                        .store(false, Ordering::Relaxed);
                    crate::sequence::write_diag(&format!("listener: sequence hotkey set to {hk}"));
                    return None;
                }
                // Unusable combo: swallow nothing, keep capturing.
            }

            // Auto-click key capture: store the pressed key's name (A, 5,
            // Space, Backspace, ...) with any held modifiers ("Ctrl+Shift+J")
            // as the click key. Non-printable keys are representable because we
            // keep a name, not a single char.
            if shared.key_capture_active.load(Ordering::Relaxed) {
                if key_to_str(&key).is_some() {
                    let combo = format_hotkey(&key, ctrl.get(), shift.get(), alt.get(), meta.get());
                    if let Ok(mut s) = shared.settings.write() {
                        s.click_key = combo.clone();
                    }
                    shared.key_capture_active.store(false, Ordering::Relaxed);
                    crate::sequence::write_diag(&format!("listener: click key set to {combo}"));
                }
                return None;
            }

            let mode = match shared.settings.read() {
                Ok(s) => s.activation_mode,
                Err(_) => return Some(event),
            };
            let hotkey = match shared.settings.read() {
                Ok(s) => s.hotkey.clone(),
                Err(_) => return Some(event),
            };

            if hotkey_matches(
                &hotkey,
                &key,
                ctrl.get(),
                shift.get(),
                alt.get(),
                meta.get(),
            ) {
                match mode {
                    ActivationMode::Toggle => {
                        let new_state = !shared.is_active();
                        shared.set_active(new_state);
                        crate::sequence::write_diag(&format!(
                            "listener: toggle hotkey press -> active = {new_state}"
                        ));
                    }
                    ActivationMode::Hold => {
                        shared.set_active(true);
                        crate::sequence::write_diag(
                            "listener: toggle hotkey hold press -> active = true",
                        );
                    }
                }
                return None;
            }

            // Sequence hotkey (F7 default) -> add a point at the cursor. No
            // explicit capture mode anymore: it is active whenever advanced
            // mode is on and the Sequence Clicking checkbox is ticked (mirrors
            // the Linux portal capture action, see hotkeys.rs).
            if shared.advanced_mode.load(Ordering::Relaxed)
                && shared.sequence_enabled.load(Ordering::Relaxed)
            {
                let seq_hotkey = match shared.settings.read() {
                    Ok(s) => s.sequence_hotkey.clone(),
                    Err(_) => return Some(event),
                };
                if hotkey_matches(
                    &seq_hotkey,
                    &key,
                    ctrl.get(),
                    shift.get(),
                    alt.get(),
                    meta.get(),
                ) {
                    if let Ok(pos) = enigo.location() {
                        let (mut points, r#loop, smooth, smooth_ms) = shared.get_sequence_data();
                        if points.len() < 10 {
                            let pt = crate::app::SequencePoint {
                                x: pos.0,
                                y: pos.1,
                                clicks: 1,
                            };
                            points.push(pt);
                            let count = points.len();
                            shared.set_sequence_data(points, r#loop, smooth, smooth_ms);
                            crate::sequence::write_diag(&format!(
                                "listener: sequence point added ({count})"
                            ));
                        }
                    }
                    return None;
                }
            }

            Some(event)
        });

        match result {
            // grab() blocking means this only ever runs at shutdown (the
            // message pump got a WM_QUIT). Nothing to fix: the hook was up.
            Ok(()) => crate::sequence::write_diag("listener: grab loop ended"),
            Err(e) => {
                let msg = format!("listener: rdev grab failed: {e:?}");
                crate::sequence::write_diag(&msg);
                shared_diag.listener_ready.store(false, Ordering::Relaxed);
                if let Ok(mut err) = shared_diag.listener_error.write() {
                    *err = msg.clone();
                }
                eprintln!("[listener] rdev error: {e:?}");
            }
        }
    });
}
