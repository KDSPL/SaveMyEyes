// Overlay module - platform-specific screen dimming overlay
// This module will contain the overlay window implementation for Windows and macOS

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

/// Overlay manager for creating and controlling dimming overlays
pub struct OverlayManager {
    opacity: f32,
    visible: bool,
    allow_capture: bool,
}

impl OverlayManager {
    pub fn new(opacity: f32) -> Self {
        Self {
            opacity,
            visible: false,
            allow_capture: false,
        }
    }

    /// Set overlay opacity (0.0 - 0.9)
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 0.9);
        // TODO: update platform overlay alpha
    }

    /// Show overlay on all monitors
    pub fn show(&mut self) {
        self.visible = true;
        #[cfg(target_os = "windows")]
        windows::show_overlay(self.opacity, self.allow_capture);
        #[cfg(target_os = "macos")]
        macos::show_overlay(self.opacity);
    }

    /// Hide overlay
    pub fn hide(&mut self) {
        self.visible = false;
        #[cfg(target_os = "windows")]
        windows::hide_overlay();
        #[cfg(target_os = "macos")]
        macos::hide_overlay();
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }
}
