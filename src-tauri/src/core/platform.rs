//! Startup hooks that adapt the process to the host OS.

/// Applies the platform-specific tweaks a resident menu bar app needs.
pub fn apply() {
    #[cfg(target_os = "macos")]
    disable_automatic_termination();
}

/// macOS may terminate an app that has no visible windows to reclaim memory
/// (Automatic Termination). A tray-only app is exactly that shape, so we opt
/// out: the process must stay alive until the user quits it.
#[cfg(target_os = "macos")]
fn disable_automatic_termination() {
    use objc2_foundation::{NSProcessInfo, NSString};

    let reason = NSString::from_str("OpenQuoteBar runs in the menu bar");
    NSProcessInfo::processInfo().disableAutomaticTermination(&reason);
}
