// UI control state tracking and hit-testing

use windows::Win32::Foundation::RECT;

/// Which tab is active
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dimmer = 0,
    Settings = 1,
    Shortcuts = 2,
}

/// State for a toggle switch control
#[derive(Debug, Clone)]
pub struct ToggleState {
    pub checked: bool,
    pub rect: RECT,
}

impl ToggleState {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            rect: RECT::default(),
        }
    }
}

/// State for the opacity slider
#[derive(Debug, Clone)]
pub struct SliderState {
    pub value: i32, // 0-90
    pub dragging: bool,
    pub rect: RECT,       // full track rect
    pub thumb_rect: RECT, // thumb hit area
}

impl SliderState {
    pub fn new(value: i32) -> Self {
        Self {
            value: value.clamp(0, 90),
            dragging: false,
            rect: RECT::default(),
            thumb_rect: RECT::default(),
        }
    }

    /// Get x position of slider thumb based on current value
    pub fn thumb_x(&self) -> i32 {
        let track_width = self.rect.right - self.rect.left;
        self.rect.left + (self.value as f32 / 90.0 * track_width as f32) as i32
    }

    /// Calculate value from an x position within the slider track
    pub fn value_from_x(&self, x: i32) -> i32 {
        let track_width = self.rect.right - self.rect.left;
        if track_width <= 0 {
            return self.value;
        }
        let rel_x = (x - self.rect.left).clamp(0, track_width);
        ((rel_x as f32 / track_width as f32) * 90.0).round() as i32
    }
}

/// State for the "Check Now" button
#[derive(Debug, Clone)]
pub struct ButtonState {
    pub rect: RECT,
    pub hover: bool,
    pub disabled: bool,
    pub text: String,
}

impl ButtonState {
    pub fn new(text: &str) -> Self {
        Self {
            rect: RECT::default(),
            hover: false,
            disabled: false,
            text: text.to_string(),
        }
    }
}

/// Complete UI state
pub struct UiState {
    pub active_tab: Tab,
    pub tab_rects: [RECT; 3],
    pub tab_bar_rect: RECT,

    // Dimmer tab
    pub slider: SliderState,
    pub enabled_toggle: ToggleState,

    // Settings tab
    pub autostart_toggle: ToggleState,
    pub auto_update_toggle: ToggleState,
    pub check_update_btn: ButtonState,
    pub update_status_text: String,

    // Shortcuts tab
    pub shortcut_texts: [String; 3],

    // Toast
    pub toast_message: String,
    pub toast_visible: bool,

    // Header credit link
    pub credit_rect: RECT,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            active_tab: Tab::Dimmer,
            tab_rects: [RECT::default(); 3],
            tab_bar_rect: RECT::default(),

            slider: SliderState::new(30),
            enabled_toggle: ToggleState::new(true),

            autostart_toggle: ToggleState::new(false),
            auto_update_toggle: ToggleState::new(true),
            check_update_btn: ButtonState::new("Check Now"),
            update_status_text: String::new(),

            shortcut_texts: [
                "Ctrl+Alt+End".into(),
                "Ctrl+Alt+Up".into(),
                "Ctrl+Alt+Down".into(),
            ],

            toast_message: String::new(),
            toast_visible: false,

            credit_rect: RECT::default(),
        }
    }
}

/// Check if a point is inside a rect
pub fn point_in_rect(x: i32, y: i32, r: &RECT) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}
