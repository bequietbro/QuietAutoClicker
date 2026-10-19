// QuietAutoClicker cursor bridge.
//
// KWin has no public protocol to *query* the absolute global cursor position
// (reading it through Xwayland is stale on Wayland), but its scripting API
// exposes `workspace.cursorPos`. This script pushes that value to the app's
// D-Bus object (src/kwincursor.rs) whenever the cursor moves, so the app can
// capture a point at the real cursor on the F7 hotkey.
//
// Loaded on demand via org.kde.kwin.Scripting.loadScript at app startup, so it
// is only ever running while the app is running -- nothing persists.
var connected = false;
function init() {
    // Guard against double loading (loadScript can race KWin's own init call).
    if (!connected) {
        connected = true;
        workspace.cursorPosChanged.connect(sendPos);
    }
}
function sendPos() {
    callDBus(
        "org.QuietAutoClicker",
        "/org/QuietAutoClicker/Cursor",
        "org.QuietAutoClicker.Cursor",
        "PositionChanged",
        workspace.cursorPos.x,
        workspace.cursorPos.y
    );
}
init();
