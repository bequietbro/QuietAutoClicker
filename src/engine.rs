use std::{
    sync::{Arc, OnceLock},
    thread,
    time::{Duration, Instant},
};

use enigo::{Button, Coordinate, Direction, Enigo, InputError, Key, Keyboard, Mouse, Settings};

use crate::app::{self, AppSettings, SharedState};

/// Position on the straight line from `from` to `to` after `step` of `steps`
/// equal segments (1..=steps yields target at the final step).
fn interpolate(from: (i32, i32), to: (i32, i32), step: u64, steps: u64) -> (i32, i32) {
    let t = step as f64 / steps as f64;
    let x = from.0 as f64 + (to.0 - from.0) as f64 * t;
    let y = from.1 as f64 + (to.1 - from.1) as f64 * t;
    (x as i32, y as i32)
}

/// True once the current activation has run for at least `limit_secs`.
fn limit_reached(elapsed_secs: u64, limit_secs: u64) -> bool {
    elapsed_secs >= limit_secs
}

/// Map a stored click-key name to an enigo `Key`. Names come from the listener's
/// `key_to_str` (Windows capture) or the in-app capture (Linux). Printable keys
/// go through `Unicode`; named specials map to enigo's variants so space,
/// backspace, enter, arrows, and F-keys all work.
fn key_name_to_enigo(name: &str) -> Option<Key> {
    use enigo::Key as E;
    let e = match name {
        "Space" => return Some(E::Unicode(' ')),
        "Enter" => E::Return,
        "Tab" => E::Tab,
        "Esc" => E::Escape,
        "Backspace" => E::Backspace,
        "Delete" => E::Delete,
        "Insert" => E::Insert,
        "Home" => E::Home,
        "End" => E::End,
        "PageUp" => E::PageUp,
        "PageDown" => E::PageDown,
        "Up" => E::UpArrow,
        "Down" => E::DownArrow,
        "Left" => E::LeftArrow,
        "Right" => E::RightArrow,
        "CapsLock" => E::CapsLock,
        "F1" => E::F1,
        "F2" => E::F2,
        "F3" => E::F3,
        "F4" => E::F4,
        "F5" => E::F5,
        "F6" => E::F6,
        "F7" => E::F7,
        "F8" => E::F8,
        "F9" => E::F9,
        "F10" => E::F10,
        "F11" => E::F11,
        "F12" => E::F12,
        _ => {
            // Any single character (letters, digits, punctuation) is sent
            // literally, so numbers and symbols are all valid click keys.
            let c = name.chars().next()?;
            return Some(E::Unicode(c));
        }
    };
    Some(e)
}

/// Split a stored auto-click combo ("A", "Ctrl+Shift+J", "F5", ...) into the
/// modifier keys to hold (in press order) plus the main key. Modifiers are
/// canonicalized by capture to the fixed Ctrl -> Shift -> Alt -> Meta order, so
/// parsing trusts that order. Returns None for a malformed combo (modifier
/// without a main key, unknown modifier, empty segment).
fn parse_combo(name: &str) -> Option<(Vec<Key>, Key)> {
    let parts: Vec<&str> = name.split('+').collect();
    let (main_part, mods_part) = parts.split_last()?;
    let main = match *main_part {
        "Ctrl" | "Shift" | "Alt" | "Meta" => return None,
        _ => key_name_to_enigo(main_part)?,
    };
    let mut mods = Vec::new();
    for m in mods_part {
        let k = match *m {
            "Ctrl" => Key::Control,
            "Shift" => Key::Shift,
            "Alt" => Key::Alt,
            "Meta" => Key::Meta,
            _ => return None,
        };
        if !mods.contains(&k) {
            mods.push(k);
        }
    }
    Some((mods, main))
}

/// Send a stored combo: press each modifier in order, press the main key,
/// release the main key, then release the modifiers in reverse order. Explicit
/// Press/Release (instead of `Direction::Click` on the main key) is the pattern
/// the enigo docs recommend for key sequences and is more reliable over
/// XTEST/Xwayland. A tiny delay between main press and release only kicks in for
/// modifier combos so plain single keys keep the full requested click rate.
fn send_key_combo(enigo: &mut Enigo, combo: &str) -> Result<(), InputError> {
    let (mods, main) = match parse_combo(combo) {
        Some(split) => split,
        // Malformed/empty stored combo: fall back to the default key.
        None => parse_combo("A").expect("A is a valid combo"),
    };
    for m in &mods {
        enigo.key(*m, Direction::Press)?;
    }
    enigo.key(main, Direction::Press)?;
    if !mods.is_empty() {
        thread::sleep(Duration::from_millis(5));
    }
    enigo.key(main, Direction::Release)?;
    for m in mods.iter().rev() {
        enigo.key(*m, Direction::Release)?;
    }
    Ok(())
}

/// True once any key click has been sent successfully, so the first successful
/// send can be confirmed in qac_debug.log (fails above are already logged).
static KEY_SENT_ONCE: OnceLock<()> = OnceLock::new();

fn perform_click(enigo: &mut Enigo, settings: &AppSettings) -> Result<(), InputError> {
    match settings.input_type {
        app::InputType::Mouse => {
            let btn = match settings.mouse_button {
                app::MouseButtonChoice::Left => Button::Left,
                app::MouseButtonChoice::Right => Button::Right,
                app::MouseButtonChoice::Middle => Button::Middle,
            };
            enigo.button(btn, Direction::Click)?;
            if settings.double_click {
                thread::sleep(Duration::from_millis(50));
                enigo.button(btn, Direction::Click)?;
            }
        }
        app::InputType::Key => {
            // On Linux/Wayland, prefer the compositor-native virtual keyboard:
            // enigo's XTEST only reaches Xwayland windows, so native Wayland
            // apps never see those keys. Any failure (portal-less session, no
            // virtual-keyboard protocol) falls back to enigo below, which
            // still works for Xwayland windows.
            #[cfg(unix)]
            {
                let (mods, main, _) = crate::wayland_kb::parse_combo(&settings.click_key);
                let vk_ok = if crate::wayland_kb::is_available() {
                    crate::wayland_kb::send_combo(&settings.click_key).is_ok()
                } else if crate::wayland_fake_kb::is_available() {
                    crate::wayland_fake_kb::send_combo(&mods, main).is_ok()
                } else {
                    false
                };
                if vk_ok {
                    let _ = KEY_SENT_ONCE.get_or_init(|| {
                        crate::sequence::write_diag(&format!(
                            "engine: first key click sent (wayland): click_key='{}'",
                            settings.click_key
                        ));
                    });
                    return Ok(());
                } else {
                    // Try to init fake if not available
                    let _ = crate::wayland_fake_kb::send_combo(&mods, main);
                    if crate::wayland_fake_kb::is_available()
                        && crate::wayland_fake_kb::send_combo(&mods, main).is_ok()
                    {
                        let _ = KEY_SENT_ONCE.get_or_init(|| {
                            crate::sequence::write_diag(&format!(
                                "engine: first key click sent (wayland-fake): click_key='{}'",
                                settings.click_key
                            ));
                        });
                        return Ok(());
                    }
                }
            }
            if let Err(e) = send_key_combo(enigo, &settings.click_key) {
                // Under --debug this lands in qac_debug.log on every platform.
                crate::sequence::write_diag(&format!(
                    "engine: key click '{}' failed: {e}",
                    settings.click_key
                ));
                return Err(e);
            }
            let _ = KEY_SENT_ONCE.get_or_init(|| {
                crate::sequence::write_diag(&format!(
                    "engine: first key click sent: click_key='{}'",
                    settings.click_key
                ));
            });
        }
    }
    Ok(())
}

pub fn start(shared: Arc<SharedState>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut enigo = match Enigo::new(&Settings::default()) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[engine] Failed to initialize enigo: {e}");
                return;
            }
        };

        let mut was_active = false;
        let mut session_start: Option<Instant> = None;
        let mut seq_idx = 0usize;
        let mut seq_remaining = 0u32;
        let mut last_x = 0i32;
        let mut last_y = 0i32;
        let mut has_prev = false;
        // Deadline of the next plain click, used to deliver exactly the
        // configured rate (clicks/sec). Resetting on activation starts a clean
        // timeline; without this, per-click overhead would make the delivered
        // count fall short of the number in the GUI.
        let mut when_next: Option<Instant> = None;

        while !shared.should_stop() {
            let is_active = shared.is_active();

            if is_active && !was_active {
                session_start = Some(Instant::now());
                seq_idx = 0;
                seq_remaining = 0;
                when_next = None;
                // Initialize smooth from first sequence point
                if shared
                    .sequence_active
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    let (points, _, smooth, _) = shared.get_sequence_data();
                    if smooth && !points.is_empty() {
                        last_x = points[0].x;
                        last_y = points[0].y;
                        has_prev = true;
                    } else {
                        has_prev = false;
                    }
                } else {
                    has_prev = false;
                }
            }

            if is_active {
                let settings = match shared.settings.read() {
                    Ok(s) => s.clone(),
                    Err(_) => {
                        thread::sleep(Duration::from_millis(10));
                        was_active = is_active;
                        continue;
                    }
                };

                let seq_active = shared
                    .sequence_active
                    .load(std::sync::atomic::Ordering::Relaxed);

                if was_active != is_active {
                    let (bp, _, _, _) = shared.get_sequence_data();
                    crate::sequence::write_diag(&format!(
                        "engine: branch check -> seq_active={seq_active} input={} click_key='{}' points={} advanced={} enabled={}",
                        match settings.input_type {
                            app::InputType::Mouse => "Mouse",
                            app::InputType::Key => "Key",
                        },
                        settings.click_key,
                        bp.len(),
                        shared.advanced_mode.load(std::sync::atomic::Ordering::Relaxed),
                        shared.sequence_enabled.load(std::sync::atomic::Ordering::Relaxed),
                    ));
                }

                if seq_active && matches!(settings.input_type, app::InputType::Mouse) {
                    let (points, loop_seq, smooth, smooth_ms) = shared.get_sequence_data();

                    if !points.is_empty() {
                        let safe_idx = seq_idx % points.len();

                        if seq_remaining == 0 {
                            seq_remaining = points[safe_idx].clicks.max(1);
                        }

                        let pt = &points[safe_idx];

                        #[cfg(windows)]
                        crate::sequence::write_diag(&format!(
                            "engine: sequence branch -> move_mouse({}, {}, Abs)",
                            pt.x, pt.y
                        ));

                        if smooth
                            && smooth_ms > 0
                            && seq_remaining == points[safe_idx].clicks.max(1)
                            && has_prev
                        {
                            let steps = (smooth_ms / 15).max(1);
                            let step_delay_ms = smooth_ms / steps;
                            for i in 1..=steps {
                                let (cx, cy) =
                                    interpolate((last_x, last_y), (pt.x, pt.y), i, steps);
                                let _ = enigo.move_mouse(cx, cy, Coordinate::Abs);
                                thread::sleep(Duration::from_millis(step_delay_ms));
                            }
                        } else {
                            #[cfg(windows)]
                            {
                                let r = enigo.move_mouse(pt.x, pt.y, Coordinate::Abs);
                                if let Err(e) = &r {
                                    crate::sequence::write_diag(&format!(
                                        "engine: move_mouse failed: {e}"
                                    ));
                                }
                            }
                            #[cfg(not(windows))]
                            let _ = enigo.move_mouse(pt.x, pt.y, Coordinate::Abs);
                        }
                        last_x = pt.x;
                        last_y = pt.y;
                        has_prev = true;

                        if let Err(e) = perform_click(&mut enigo, &settings) {
                            eprintln!("[engine] Input error: {e}");
                        }

                        seq_remaining -= 1;
                        if seq_remaining == 0 {
                            let last_point = safe_idx + 1 >= points.len();
                            if last_point && !loop_seq {
                                shared.set_active(false);
                            } else {
                                seq_idx = (safe_idx + 1) % points.len();
                            }
                        }
                    }
                } else {
                    if was_active != is_active {
                        crate::sequence::write_diag(
                            "engine: plain-click branch (sequence not active)",
                        );
                    }
                    // Deadline pacing: wait until the next click's scheduled time
                    // (falling back to "now" when we're already behind, which
                    // caps high rates at the physical maximum instead of drifting
                    // below the requested count). Afterwards the next deadline is
                    // anchored to this one, so the average rate matches the GUI
                    // number exactly instead of being dragged down by click cost.
                    let interval = settings.random_interval_ms();
                    let deadline = match when_next {
                        Some(d) if d > Instant::now() => d,
                        _ => Instant::now(),
                    };
                    let now = Instant::now();
                    if deadline > now {
                        thread::sleep(deadline - now);
                    }
                    if let Err(e) = perform_click(&mut enigo, &settings) {
                        eprintln!("[engine] Input error: {e}");
                    }
                    when_next = Some(deadline + Duration::from_millis(interval));
                }

                if let Some(start) = session_start {
                    if settings.limit_time_enabled {
                        let elapsed = start.elapsed().as_secs();
                        if limit_reached(elapsed, settings.limit_time_secs) {
                            shared.set_active(false);
                        }
                    }
                }

                // Sequence mode paces each point with its own interval sleep
                // (it must also honor per-point click counts + smooth movement);
                // plain mode already paced itself above via its deadline.
                if seq_active && matches!(settings.input_type, app::InputType::Mouse) {
                    let interval = settings.random_interval_ms();
                    thread::sleep(Duration::from_millis(interval));
                }
            } else {
                thread::sleep(Duration::from_millis(20));
            }

            was_active = is_active;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolate_midpoint() {
        let (x, y) = interpolate((0, 0), (100, 200), 1, 2);
        assert_eq!((x, y), (50, 100));
    }

    #[test]
    fn interpolate_endpoint_is_target() {
        let (x, y) = interpolate((10, 10), (30, 50), 1, 1);
        assert_eq!((x, y), (30, 50));
    }

    #[test]
    fn interpolate_backwards() {
        let (x, y) = interpolate((100, 100), (0, 0), 1, 4);
        assert_eq!((x, y), (75, 75));
    }

    #[test]
    fn interpolate_truncates_fractions() {
        let (x, _) = interpolate((0, 0), (100, 0), 1, 3);
        assert_eq!(x, 33);
    }

    #[test]
    fn limit_not_reached_below() {
        assert!(!limit_reached(59, 60));
    }

    #[test]
    fn limit_reached_at_and_above() {
        assert!(limit_reached(60, 60));
        assert!(limit_reached(61, 60));
    }

    #[test]
    fn parse_combo_plain_letter() {
        assert_eq!(parse_combo("A"), Some((vec![], Key::Unicode('A'))));
    }

    #[test]
    fn parse_combo_named_key() {
        assert_eq!(parse_combo("F5"), Some((vec![], Key::F5)));
    }

    #[test]
    fn parse_combo_single_modifier() {
        assert_eq!(
            parse_combo("Ctrl+A"),
            Some((vec![Key::Control], Key::Unicode('A')))
        );
    }

    #[test]
    fn parse_combo_multiple_modifiers_keep_order() {
        assert_eq!(
            parse_combo("Ctrl+Shift+J"),
            Some((vec![Key::Control, Key::Shift], Key::Unicode('J')))
        );
    }

    #[test]
    fn parse_combo_dedupes_modifiers() {
        assert_eq!(
            parse_combo("Ctrl+Ctrl+A"),
            Some((vec![Key::Control], Key::Unicode('A')))
        );
    }

    #[test]
    fn parse_combo_rejects_modifier_only() {
        assert_eq!(parse_combo("Ctrl"), None);
        assert_eq!(parse_combo("Ctrl+Shift"), None);
    }

    #[test]
    fn parse_combo_rejects_unknown() {
        assert_eq!(parse_combo(""), None);
        assert_eq!(parse_combo("+A"), None);
        assert_eq!(parse_combo("Ctrl+"), None);
        assert_eq!(parse_combo("Fake+A"), None);
    }
}
