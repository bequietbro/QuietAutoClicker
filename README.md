<p align="center"><img src="https://github.com/user-attachments/assets/17948fa3-7533-45ef-8be5-516b0ca3aec8" alt="QuietAutoClicker" height="100"></p>
<h1 align="center">QuietAutoClicker</h1>
Lightweight native auto-clicker with a compact frameless GUI. Built with [Slint](https://slint.dev) and Rust.

## Features
- Fixed or randomized click rate
- Mouse (left/right/middle) or keyboard key/shortcut clicking
- Sequence Clicking: capture up to 10 positions and loop through them (optional smooth interpolation)
- Toggle or Hold activation, optional time limit
- Always-on-top, dark theme

## Installation
### Releases
Download from [GitHub Releases](https://github.com/bequietbro/QuietAutoClicker/releases):
- `QuietAutoClicker-Linux` — Linux binary (UPX, portable)
- `QuietAutoClicker-Linux-x86_64.AppImage` — Portable AppImage for Linux
- `QuietAutoClicker-Windows.exe` — Windows executable (UPX)

### AUR
```bash
yay -S quietautoclicker
```

## Usage
1. **Global hotkeys** — KDE Plasma Wayland: first launch opens a portal dialog to add `F6` (start/stop) and `F7` (add point). They can be changed in KDE System Settings → Keyboard → Shortcuts → QuietAutoClicker.
2. **Auto-click key** — click the key box and press a key or modifier+key combo. Works in-app on all platforms.
3. **Sequence Clicking** — enable Advanced Mode → switch to Sequence. Press `F7` at each target to add points (up to 10). Toggle/Loop options available.

## Notes
- **Linux (Wayland/KDE)**: global hotkeys use XDG GlobalShortcuts portal. On non-KDE or no portal, global hotkeys show as "unavailable"; the auto-click key still works.
- **X11**: mouse and key clicking work; global hotkeys are not implemented via portal on X11.
- **GNOME/Hyprland**: native Wayland key injection varies by compositor. Basic clicking works; some native Wayland apps may not receive key combos depending on compositor.
- **Windows**: maintenance-only (bug fixes, smoke test per release).
- **Persistence**: only sequence points are saved (`sequence.json`). All other settings reset on launch.

## Building from source
Requires Rust toolchain (no extra system libs):

```bash
cargo build --release
cargo run
```

For Windows cross-build:
```bash
cargo build --release --target x86_64-pc-windows-gnu
```

<h2 align="center">Credits</h2>

<p align="center">
Developed by <a href="https://github.com/bequietbro">BEQUIETBRO</a>.
</p>

<h2 align="center">License</h2>

<p align="center">
<img src="https://www.gnu.org/graphics/agplv3-155x51.png" alt="AGPLv3 License" />
</p>

<p align="center">
This project is licensed under the <strong>GNU Affero General Public License v3.0</strong>.
</p>
