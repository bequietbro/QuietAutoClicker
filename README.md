<p align="center"><img src="https://github.com/user-attachments/assets/17948fa3-7533-45ef-8be5-516b0ca3aec8" alt="QuietAutoClicker" height="100"></p>
<h1 align="center">QuietAutoClicker</h1>

<h2 align="center">Features</h2>

- Fixed or randomized click rate
- Mouse (left/right/middle) or keyboard key/shortcut clicking
- Sequence Clicking: capture up to 10 positions and loop through them (optional smooth interpolation)
- Toggle or Hold activation, optional time limit
- Always-on-top, dark theme only

<h2 align="center">Installation</h2>

### [GitHub Releases](https://github.com/bequietbro/QuietAutoClicker/releases)

- `QuietAutoClicker-Linux` — Linux binary (portable)
- `QuietAutoClicker-Linux-x86_64.AppImage` — Portable AppImage for Linux
- `QuietAutoClicker-Windows.exe` — Windows executable

<h2 align="center">Notes</h2>

> So far, it has only been tested on CachyOS KDE Plasma and Fedora KDE Plasma.
>
> Found a bug, have a tip or a recommendation? Please open an
> [issue](https://github.com/bequietbro/QuietAutoClicker/issues) — it helps
> future fixes and improvements.

- **Linux (Wayland/KDE)**: global hotkeys use XDG GlobalShortcuts portal. On non-KDE or no portal, global hotkeys show as "unavailable"; the auto-click key still works.
- **X11**: mouse and key clicking work; global hotkeys are not implemented via portal on X11.
- **GNOME/Hyprland**: native Wayland key injection varies by compositor. Basic clicking works; some native Wayland apps may not receive key combos depending on compositor.
- **Windows**: maintenance-only (bug fixes, smoke test per release).
- **Persistence**: only sequence points are saved (`sequence.json`). All other settings reset on launch.

<h2 align="center">FAQ</h2>

**Will macros be added?**
No. KDE Plasma already includes macro functionality in its system settings.
On Windows, use [AutoHotkey](https://www.autohotkey.com/) or
[Microsoft PowerToys](https://learn.microsoft.com/windows/powertoys/).

**Why doesn't `Shift++` work as an auto-click key?**
Known limitation: combos are stored as `Modifier+...+Key` joined with `+`,
so the `+` character itself cannot be the main key. A captured `Shift`+`=`
(stored as `Shift++`) is rejected and the engine falls back to the default
key (`A`). Use a letter, digit or another symbol instead.

<h2 align="center">Building from source</h2>
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
<img src="https://www.gnu.org/graphics/gplv3-127x51.png" alt="GPLv3 License" />
</p>

<p align="center">
This project is licensed under the <strong>GNU General Public License v3.0</strong> License.
</p>
