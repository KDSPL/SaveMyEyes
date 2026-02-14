// macOS overlay implementation using cocoa/objc
// Creates a click-through NSWindow at high window level

#![cfg(target_os = "macos")]

/// Show overlay with given opacity on all monitors
pub fn show_overlay(opacity: f32) {
    // TODO: Implement using cocoa crate
    // 1. Get all screens via NSScreen.screens
    // 2. Create NSWindow for each with styleMask = borderless
    // 3. Set window level to NSFloatingWindowLevel or higher
    // 4. Set ignoresMouseEvents = true
    // 5. Set backgroundColor to black with alpha = opacity
    // 6. Make key and order front
    let _ = opacity;
    println!("[macOS] show_overlay called with opacity: {}", opacity);
}

/// Hide overlay windows
pub fn hide_overlay() {
    // TODO: Order out or close overlay windows
    println!("[macOS] hide_overlay called");
}

/// Update overlay alpha
pub fn set_opacity(opacity: f32) {
    let _ = opacity;
    // TODO: Update window background color alpha
    println!("[macOS] set_opacity called with: {}", opacity);
}
