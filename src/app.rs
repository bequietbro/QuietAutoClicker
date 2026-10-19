use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, RwLock,
};

pub use crate::sequence::SequencePoint;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RateInputMode {
    Rate,
    Duration,
}

impl RateInputMode {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Duration,
            _ => Self::Rate,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Rate => 0,
            Self::Duration => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClickInterval {
    S,
    M,
    H,
    D,
}

impl ClickInterval {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::M,
            2 => Self::H,
            3 => Self::D,
            _ => Self::S,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::S => 0,
            Self::M => 1,
            Self::H => 2,
            Self::D => 3,
        }
    }

    pub fn multiplier_ms(self) -> u64 {
        match self {
            Self::S => 1000,
            Self::M => 60_000,
            Self::H => 3_600_000,
            Self::D => 86_400_000,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ActivationMode {
    Toggle,
    Hold,
}

impl ActivationMode {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Hold,
            _ => Self::Toggle,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Toggle => 0,
            Self::Hold => 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MouseButtonChoice {
    Left,
    Right,
    Middle,
}

impl MouseButtonChoice {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Right,
            2 => Self::Middle,
            _ => Self::Left,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum InputType {
    Mouse,
    Key,
}

impl InputType {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Key,
            _ => Self::Mouse,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Mouse => 0,
            Self::Key => 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppSettings {
    pub rate_input_mode: RateInputMode,
    pub click_speed: u64,
    pub click_interval: ClickInterval,
    pub duration_hours: u64,
    pub duration_minutes: u64,
    pub duration_seconds: u64,
    pub duration_milliseconds: u64,
    pub random_enabled: bool,
    pub random_variation: u64,
    pub double_click: bool,
    pub mouse_button: MouseButtonChoice,
    pub input_type: InputType,
    pub activation_mode: ActivationMode,
    pub limit_time_enabled: bool,
    pub limit_time_secs: u64,
    // The auto-click key when input_type is Key: a single key name ("A", "5",
    // "Space", "Backspace", "Enter", ...) or a modifier combo ("Ctrl+Shift+J").
    // Stored as a name so non-printable keys (space, backspace, arrows) are
    // representable, unlike a plain char. Modifiers are canonicalized by capture
    // to the fixed Ctrl -> Shift -> Alt -> Meta order.
    pub click_key: String,
    #[cfg_attr(unix, allow(dead_code))]
    pub hotkey: String,
    #[cfg_attr(unix, allow(dead_code))]
    pub sequence_hotkey: String,
    pub theme_dark: bool,
    pub always_on_top: bool,
}

impl AppSettings {
    pub fn effective_interval_ms(&self) -> u64 {
        match self.rate_input_mode {
            RateInputMode::Rate => {
                let mult = self.click_interval.multiplier_ms();
                let speed = self.click_speed.max(1);
                (mult / speed).max(1)
            }
            RateInputMode::Duration => {
                let total = self.duration_hours * 3_600_000
                    + self.duration_minutes * 60_000
                    + self.duration_seconds * 1000
                    + self.duration_milliseconds;
                total.max(1)
            }
        }
    }

    pub fn random_interval_ms(&self) -> u64 {
        if !self.random_enabled || self.random_variation == 0 {
            return self.effective_interval_ms();
        }
        let base = self.effective_interval_ms();
        let var = (base * self.random_variation) / 100;
        let min = base.saturating_sub(var).max(1);
        let max = base + var;
        fastrand::u64(min..=max)
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            rate_input_mode: RateInputMode::Rate,
            click_speed: 25,
            click_interval: ClickInterval::S,
            duration_hours: 0,
            duration_minutes: 0,
            duration_seconds: 0,
            duration_milliseconds: 40,
            random_enabled: false,
            random_variation: 35,
            double_click: false,
            mouse_button: MouseButtonChoice::Left,
            input_type: InputType::Mouse,
            activation_mode: ActivationMode::Toggle,
            limit_time_enabled: false,
            limit_time_secs: 60,
            click_key: "A".into(),
            hotkey: "F6".into(),
            sequence_hotkey: "F7".into(),
            theme_dark: true,
            always_on_top: true,
        }
    }
}

pub struct SharedState {
    pub active: AtomicBool,
    pub stop_flag: AtomicBool,
    #[cfg_attr(unix, allow(dead_code))]
    pub hotkey_capture_active: AtomicBool,
    pub sequence_active: AtomicBool,
    pub advanced_mode: AtomicBool,
    pub sequence_enabled: AtomicBool,
    pub sequence_loop: AtomicBool,
    pub sequence_smooth: AtomicBool,
    pub sequence_smooth_ms: AtomicU64,
    pub settings: RwLock<AppSettings>,
    #[cfg_attr(windows, allow(dead_code))]
    pub use_kglobalaccel: AtomicBool,
    #[cfg_attr(windows, allow(dead_code))]
    pub kg_binding_toggle: RwLock<String>,
    #[cfg_attr(windows, allow(dead_code))]
    pub kg_binding_capture: RwLock<String>,
    // True once the portal has reported the binding for this action at least
    // once (even as empty -> the user cleared it). Before that the UI shows the
    // factory default instead of a misleading "None".
    #[cfg_attr(windows, allow(dead_code))]
    pub kg_binding_toggle_valid: AtomicBool,
    #[cfg_attr(windows, allow(dead_code))]
    pub kg_binding_capture_valid: AtomicBool,
    #[cfg_attr(unix, allow(dead_code))]
    pub sequence_hotkey_capture_active: AtomicBool,
    // A click-key capture is in progress: the next physical key (or the next
    // in-app key press on Linux / Window-fallback) becomes the auto-click key.
    pub key_capture_active: AtomicBool,
    // Windows listener diagnostics: how many global key events reached the rdev
    // grab callback (heartbeat), whether the hook is up, and the failure text if
    // Enigo::new/rdev::grab died. Lets the UI show "unavailable" instead of a
    // hotkey that can never be captured, and feeds qac_debug.log.
    #[cfg_attr(unix, allow(dead_code))]
    pub listener_events: AtomicU64,
    #[cfg_attr(unix, allow(dead_code))]
    pub listener_ready: AtomicBool,
    #[cfg_attr(unix, allow(dead_code))]
    pub listener_error: RwLock<String>,
}

impl SharedState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            active: AtomicBool::new(false),
            stop_flag: AtomicBool::new(false),
            hotkey_capture_active: AtomicBool::new(false),
            sequence_active: AtomicBool::new(false),
            advanced_mode: AtomicBool::new(false),
            sequence_enabled: AtomicBool::new(false),
            sequence_loop: AtomicBool::new(true),
            sequence_smooth: AtomicBool::new(false),
            sequence_smooth_ms: AtomicU64::new(80),
            settings: RwLock::new(AppSettings::default()),
            use_kglobalaccel: AtomicBool::new(false),
            kg_binding_toggle: RwLock::new(String::new()),
            kg_binding_capture: RwLock::new(String::new()),
            kg_binding_toggle_valid: AtomicBool::new(false),
            kg_binding_capture_valid: AtomicBool::new(false),
            sequence_hotkey_capture_active: AtomicBool::new(false),
            key_capture_active: AtomicBool::new(false),
            listener_events: AtomicU64::new(0),
            listener_ready: AtomicBool::new(false),
            listener_error: RwLock::new(String::new()),
        })
    }

    pub fn get_sequence_data(&self) -> (Vec<SequencePoint>, bool, bool, u64) {
        let cfg = crate::sequence::load_sequence();
        (
            cfg.points,
            self.sequence_loop.load(Ordering::Relaxed),
            self.sequence_smooth.load(Ordering::Relaxed),
            self.sequence_smooth_ms.load(Ordering::Relaxed),
        )
    }

    pub fn set_sequence_data(
        &self,
        points: Vec<SequencePoint>,
        r#loop: bool,
        smooth: bool,
        smooth_ms: u64,
    ) {
        let cfg = crate::sequence::SequenceConfig { points };
        if let Err(e) = crate::sequence::save_sequence(&cfg) {
            eprintln!("[app] Failed to save sequence: {e}");
        }
        self.sequence_loop.store(r#loop, Ordering::Relaxed);
        self.sequence_smooth.store(smooth, Ordering::Relaxed);
        self.sequence_smooth_ms.store(smooth_ms, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn set_active(&self, val: bool) {
        self.active.store(val, Ordering::Relaxed);
    }

    pub fn should_stop(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }

    pub fn signal_stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_from_index_to_index_round_trips() {
        for i in -2..5 {
            assert_eq!(
                RateInputMode::from_index(i).to_index(),
                RateInputMode::from_index(i).to_index()
            );
        }
        assert_eq!(ClickInterval::from_index(0).to_index(), 0);
        assert_eq!(ClickInterval::from_index(1).to_index(), 1);
        assert_eq!(ClickInterval::from_index(2).to_index(), 2);
        assert_eq!(ClickInterval::from_index(3).to_index(), 3);
        assert_eq!(ClickInterval::from_index(99).to_index(), 0);
        assert_eq!(ActivationMode::from_index(0).to_index(), 0);
        assert_eq!(ActivationMode::from_index(1).to_index(), 1);
        assert_eq!(MouseButtonChoice::from_index(0).to_index(), 0);
        assert_eq!(MouseButtonChoice::from_index(1).to_index(), 1);
        assert_eq!(MouseButtonChoice::from_index(2).to_index(), 2);
        assert_eq!(InputType::from_index(0).to_index(), 0);
        assert_eq!(InputType::from_index(1).to_index(), 1);
    }

    #[test]
    fn interval_multipliers_are_correct() {
        assert_eq!(ClickInterval::S.multiplier_ms(), 1000);
        assert_eq!(ClickInterval::M.multiplier_ms(), 60_000);
        assert_eq!(ClickInterval::H.multiplier_ms(), 3_600_000);
        assert_eq!(ClickInterval::D.multiplier_ms(), 86_400_000);
    }

    #[test]
    fn effective_interval_rate_mode_clamps() {
        let mut s = AppSettings {
            click_interval: ClickInterval::S,
            click_speed: 25,
            ..Default::default()
        };
        assert_eq!(s.effective_interval_ms(), 40);

        s.click_speed = 0;
        assert_eq!(s.effective_interval_ms(), 1000, "speed clamped to 1");

        s.click_speed = u64::MAX;
        assert_eq!(s.effective_interval_ms(), 1, "interval floored at 1");
    }

    #[test]
    fn effective_interval_duration_mode_sums() {
        let mut s = AppSettings {
            rate_input_mode: RateInputMode::Duration,
            duration_hours: 1,
            duration_minutes: 2,
            duration_seconds: 3,
            duration_milliseconds: 400,
            ..Default::default()
        };
        assert_eq!(s.effective_interval_ms(), 3_723_400);

        s.duration_hours = 0;
        s.duration_minutes = 0;
        s.duration_seconds = 0;
        s.duration_milliseconds = 0;
        assert_eq!(s.effective_interval_ms(), 1, "zero duration floored at 1");
    }

    #[test]
    fn random_interval_disabled_returns_effective() {
        let mut s = AppSettings::default();
        assert_eq!(s.random_interval_ms(), s.effective_interval_ms());

        s.random_enabled = true;
        s.random_variation = 0;
        assert_eq!(s.random_interval_ms(), s.effective_interval_ms());
    }

    #[test]
    fn random_interval_stays_within_bounds() {
        let s = AppSettings {
            random_enabled: true,
            random_variation: 50,
            ..Default::default()
        };
        for _ in 0..1000 {
            let base = s.effective_interval_ms();
            let var = base * s.random_variation / 100;
            let min = base.saturating_sub(var).max(1);
            let max = base + var;
            let v = s.random_interval_ms();
            assert!(v >= min && v <= max, "{v} not in [{min}..={max}]");
        }
    }

    #[test]
    fn random_interval_never_zero() {
        let s = AppSettings {
            random_enabled: true,
            random_variation: 100,
            click_speed: u64::MAX,
            ..Default::default()
        };
        for _ in 0..1000 {
            assert!(s.random_interval_ms() >= 1);
        }
    }
}
