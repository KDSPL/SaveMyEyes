// SaveMyEyes — native macOS screen dimmer
// Uses Cocoa/AppKit via objc2 for a fully native experience.

mod app;
mod autostart;
mod config;
mod hotkeys;
mod overlay;
mod tray;
mod updater;

fn main() {
    app::run();
}
