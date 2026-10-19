use std::path::{Path, PathBuf};

// PNG embedded at build time so the icon works no matter where the binary runs
// from (e.g. `cargo run` deep inside a debug target tree).
const ICON_PNG: &[u8] = include_bytes!("../ui/icons/QAC_Logo_256.png");

/// Best-effort Linux integration on every launch. The app stays portable, so
/// the ONLY file written to ~/.local is the launcher entry KDE and the portal
/// need to find us (`QuietAutoClicker.desktop`, whose basename MUST match the
/// app id -- the portal's host-app lookup and the Shortcuts KCM resolve the app
/// through it). The icon goes next to the binary (absolute path in the entry),
/// and files written by earlier builds are cleaned up so ~/.local stays free of
/// our litter.
///
/// Under an AppImage the executable sits in a read-only squashfs mount, so the
/// icon cannot go next to it: it is installed into the standard hicolor theme
/// path instead, and the entry points at the AppImage file itself.
///
/// A system install (AUR/package) already ships `.desktop` + icon in /usr/share,
/// so we skip writing anything then: dropping a png next to a /usr/bin binary
/// would need root, and a user-level entry would only shadow the packaged one.
pub fn install_linux_integration() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        eprintln!("[install] No $HOME, skipping integration");
        return;
    };

    let system_install = Path::new("/usr/share/applications/QuietAutoClicker.desktop").exists();
    let apps_dir = home.join(".local/share/applications");
    let hicolor_png = home.join(".local/share/icons/hicolor/256x256/apps/QuietAutoClicker.png");
    // The AppImage runtime points APPIMAGE at the payload file; its directory is
    // a read-only mount, so nothing may be written there.
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);

    if !system_install {
        if let Err(e) = std::fs::create_dir_all(&apps_dir) {
            eprintln!("[install] Could not create {}: {e}", apps_dir.display());
        }

        let (exec_path, icon_path) = if let Some(appimage_path) = &appimage {
            if let Some(parent) = hicolor_png.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("[install] Could not create {}: {e}", parent.display());
                }
            }
            write_icon(&hicolor_png);
            (appimage_path.clone(), hicolor_png.clone())
        } else {
            let exe = std::env::current_exe().unwrap_or_default();
            let exe_dir = exe.parent().unwrap_or(Path::new(""));
            let icon_path = exe_dir.join("QuietAutoClicker.png");
            write_icon(&icon_path);
            (exe, icon_path)
        };

        let entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=QuietAutoClicker\n\
             GenericName=Auto Clicker\n\
             Comment=Modern auto clicker for Windows and Linux\n\
             Exec={}\n\
             Icon={}\n\
             Terminal=false\n\
             Categories=Utility;\n\
             StartupNotify=true\n",
            exec_path.display(),
            icon_path.display()
        );
        if let Err(e) = std::fs::write(apps_dir.join("QuietAutoClicker.desktop"), entry) {
            eprintln!("[install] Could not write .desktop: {e}");
        }
    }

    // Remove litter from older builds; on a system install also drop any stale
    // user-level entry so the packaged one wins.
    let mut legacy: Vec<PathBuf> = vec![
        home.join(".local/share/applications/quietautoclicker.desktop"),
        home.join(".local/share/pixmaps/QuietAutoClicker.png"),
        home.join(".local/share/pixmaps/QAC_Logo_256.png"),
        home.join(".local/share/pixmaps/QAC_Logo_128.png"),
    ];
    if appimage.is_none() {
        // hicolor is the canonical AppImage icon location; only a plain-binary
        // run sees a leftover there as litter.
        legacy.push(hicolor_png);
    }
    if system_install {
        legacy.push(apps_dir.join("QuietAutoClicker.desktop"));
    }
    for path in legacy {
        match std::fs::remove_file(&path) {
            Ok(_) => eprintln!("[install] Removed legacy file {}", path.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("[install] Could not remove {}: {e}", path.display()),
        }
    }

    prune_legacy_shortcuts();
}

fn write_icon(path: &Path) {
    if let Err(e) = std::fs::write(path, ICON_PNG) {
        // Not fatal: the entry still works, just without an icon.
        eprintln!("[install] Could not write icon {}: {e}", path.display());
    }
}

/// Remove OUR stale shortcut bindings from `~/.config/kglobalshortcutsrc`.
/// Older builds registered the toggle/capture actions under other components
/// (the pre-Registry-fix `codium` app id and the pre-portal lowercase
/// `quietautoclicker`), which shows up as duplicate "QuietAutoClicker" entries
/// in System Settings -> Keyboard -> Shortcuts. The app id is always
/// `QuietAutoClicker`, so only that section is kept. Line-based edit so
/// every unrelated key is preserved verbatim (incl. `[services][x]` headers).
/// Note: on Plasma 6 Wayland KWin owns `org.kde.kglobalaccel` (no separate
/// kglobalacceld process) and caches components in memory, so the KCM may keep
/// showing the stale entries until KWin/session restarts or they are removed
/// via `org.kde.KGlobalAccel.unregister(component, action)` on `/kglobalaccel`.
fn prune_legacy_shortcuts() {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    let Some(config) = config else { return };
    let path = config.join("kglobalshortcutsrc");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };

    // Split into sections. A KConfig header is any line starting with `[`
    // (`[services][x]` counts as a single header); keys belong to the last
    // header seen; blank/comment lines travel with their section.
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in content.lines() {
        if line.starts_with('[') {
            sections.push((line.to_string(), Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(line.to_string());
        }
    }

    let mut changed_any = false;
    let mut kept: Vec<(String, Vec<String>)> = Vec::with_capacity(sections.len());
    for (header, body) in sections {
        let is_live = header == "[QuietAutoClicker]";
        let mut kept_body: Vec<String> = Vec::with_capacity(body.len());
        let mut removed_any = false;
        for line in &body {
            if !is_live
                && (line.starts_with("capture=") || line.starts_with("toggle="))
                && (line.contains("Add Sequence Point") || line.contains("Toggle AutoClick"))
            {
                removed_any = true;
                continue;
            }
            kept_body.push(line.clone());
        }
        if removed_any {
            changed_any = true;
            // Once our actions are gone the section is ours alone: drop it
            // unless it still carries unrelated keys.
            if kept_body
                .iter()
                .all(|l| l.is_empty() || l.starts_with("_k_friendly_name="))
            {
                continue;
            }
        }
        kept.push((header, kept_body));
    }

    if !changed_any {
        return;
    }
    let mut out = String::new();
    for (header, body) in &kept {
        out.push_str(header);
        out.push('\n');
        for line in body {
            out.push_str(line);
            out.push('\n');
        }
    }
    match std::fs::write(&path, out) {
        Ok(_) => eprintln!(
            "[install] Pruned legacy shortcut bindings from {}",
            path.display()
        ),
        Err(e) => eprintln!("[install] Could not prune {}: {e}", path.display()),
    }
}
