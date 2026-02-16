// macOS Settings window — polished card-based dark UI matching Windows design.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AllocAnyThread, MainThreadMarker};
use objc2_app_kit::*;
use objc2_foundation::*;

use std::sync::Mutex;

use crate::app;
use crate::autostart;
use crate::config;
use crate::overlay;
use crate::ui::theme::*;

// ---------------------------------------------------------------------------
// Thread-safety wrapper (main-thread-only UI objects behind Mutex)
// ---------------------------------------------------------------------------
struct Mt<T>(T);
unsafe impl<T> Send for Mt<T> {}
unsafe impl<T> Sync for Mt<T> {}
impl<T> std::ops::Deref for Mt<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T> std::ops::DerefMut for Mt<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

// ---------------------------------------------------------------------------
// Static references
// ---------------------------------------------------------------------------
static SETTINGS_WINDOW: Mutex<Option<Mt<Retained<NSWindow>>>> = Mutex::new(None);
static SLIDER_REF: Mutex<Option<Mt<Retained<NSSlider>>>> = Mutex::new(None);
static SLIDER_LABEL_REF: Mutex<Option<Mt<Retained<NSTextField>>>> = Mutex::new(None);
static ENABLED_TOGGLE_REF: Mutex<Option<Mt<Retained<NSButton>>>> = Mutex::new(None);
static SETTINGS_TARGET: Mutex<Option<Retained<SettingsTarget>>> = Mutex::new(None);

// Per-monitor slider/label refs (up to 8 monitors)
static MONITOR_SLIDER_REFS: Mutex<Vec<Mt<Retained<NSSlider>>>> = Mutex::new(Vec::new());
static MONITOR_LABEL_REFS: Mutex<Vec<Mt<Retained<NSTextField>>>> = Mutex::new(Vec::new());
// Display names for current monitors (used to key per_display_opacity)
static MONITOR_NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

// Tab content views — stored so we can show/hide on tab switch
static TAB_VIEWS: Mutex<Option<Mt<[Retained<NSView>; 3]>>> = Mutex::new(None);

/// Update the settings UI to reflect current state (called after hotkey toggle).
pub fn update_ui() {
    let st = app::state();
    let cfg = st.lock().unwrap().config.clone();

    if let Some(toggle) = ENABLED_TOGGLE_REF.lock().unwrap().as_ref() {
        let is_on = cfg.is_enabled;
        toggle.setState(if is_on {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        style_toggle(toggle, is_on);
    }
    if let Some(slider) = SLIDER_REF.lock().unwrap().as_ref() {
        slider.setFloatValue(cfg.opacity * 100.0);
    }
    if let Some(label) = SLIDER_LABEL_REF.lock().unwrap().as_ref() {
        let pct = (cfg.opacity * 100.0).round() as i32;
        label.setStringValue(&NSString::from_str(&format!("{}%", pct)));
    }

    // Update per-monitor sliders
    let sliders = MONITOR_SLIDER_REFS.lock().unwrap();
    let labels = MONITOR_LABEL_REFS.lock().unwrap();
    let names = MONITOR_NAMES.lock().unwrap();
    for (i, slider) in sliders.iter().enumerate() {
        let opacity = names.get(i)
            .and_then(|n| cfg.per_display_opacity.get(n))
            .copied()
            .unwrap_or(cfg.opacity);
        slider.setFloatValue(opacity * 100.0);
        if let Some(label) = labels.get(i) {
            let pct = (opacity * 100.0).round() as i32;
            label.setStringValue(&NSString::from_str(&format!("{}%", pct)));
        }
    }
}

// ---------------------------------------------------------------------------
// SettingsTarget — ObjC class for actions
// ---------------------------------------------------------------------------
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SettingsTarget"]
    #[thread_kind = AllocAnyThread]
    struct SettingsTarget;

    unsafe impl NSObjectProtocol for SettingsTarget {}

    impl SettingsTarget {
        #[unsafe(method(sliderChanged:))]
        fn slider_changed(&self, sender: &NSSlider) {
            let val = sender.floatValue();
            let clamped = (val / 100.0).clamp(0.0, 0.9);

            if let Some(label) = SLIDER_LABEL_REF.lock().unwrap().as_ref() {
                let pct = (clamped * 100.0).round() as i32;
                label.setStringValue(&NSString::from_str(&format!("{}%", pct)));
            }

            let st = app::state();
            let mut s = st.lock().unwrap();
            s.config.opacity = clamped;
            s.config.is_enabled = true;
            config::save_config(&s.config);

            // Auto-enable the toggle
            if let Some(toggle) = ENABLED_TOGGLE_REF.lock().unwrap().as_ref() {
                toggle.setState(NSControlStateValueOn);
                style_toggle(toggle, true);
            }

            // Update existing overlays in-place (no flicker) or create if needed
            let mtm = MainThreadMarker::new().unwrap();
            if !overlay::update_opacity(mtm, s.config.opacity, s.config.multi_monitor, &s.config.per_display_opacity) {
                overlay::show(
                    mtm,
                    s.config.opacity,
                    s.config.multi_monitor,
                    &s.config.per_display_opacity,
                );
            }
        }

        #[unsafe(method(monitorSliderChanged:))]
        fn monitor_slider_changed(&self, sender: &NSSlider) {
            let tag: isize = unsafe { msg_send![sender, tag] };
            let monitor_idx = tag as usize;
            let val = sender.floatValue();
            let clamped = (val / 100.0).clamp(0.0, 0.9);

            // Update the label for this monitor
            let labels = MONITOR_LABEL_REFS.lock().unwrap();
            if let Some(label) = labels.get(monitor_idx) {
                let pct = (clamped * 100.0).round() as i32;
                label.setStringValue(&NSString::from_str(&format!("{}%", pct)));
            }
            drop(labels);

            // Get display name for this index
            let display_name = MONITOR_NAMES.lock().unwrap().get(monitor_idx).cloned();

            let st = app::state();
            let mut s = st.lock().unwrap();
            // Store by display name for persistence
            if let Some(name) = &display_name {
                s.config.per_display_opacity.insert(name.clone(), clamped);
            }
            s.config.is_enabled = true;
            // Auto-enable multi-monitor if user interacts with secondary monitor slider
            if monitor_idx > 0 {
                s.config.multi_monitor = true;
            }
            // Also update global opacity to match primary monitor
            if monitor_idx == 0 {
                s.config.opacity = clamped;
            }
            config::save_config(&s.config);

            // Auto-enable the toggle
            if let Some(toggle) = ENABLED_TOGGLE_REF.lock().unwrap().as_ref() {
                toggle.setState(NSControlStateValueOn);
                style_toggle(toggle, true);
            }

            // Update existing overlays in-place (no flicker) or create if needed
            let mtm = MainThreadMarker::new().unwrap();
            if !overlay::update_opacity(mtm, s.config.opacity, s.config.multi_monitor, &s.config.per_display_opacity) {
                overlay::show(
                    mtm,
                    s.config.opacity,
                    s.config.multi_monitor,
                    &s.config.per_display_opacity,
                );
            }
        }

        #[unsafe(method(enabledToggled:))]
        fn enabled_toggled(&self, sender: &NSButton) {
            let checked = sender.state() == NSControlStateValueOn;
            style_toggle(sender, checked);
            let st = app::state();
            let mut s = st.lock().unwrap();

            if checked {
                s.config.is_enabled = true;
                s.config.opacity = s.config.last_opacity;
                config::save_config(&s.config);
                let mtm = MainThreadMarker::new().unwrap();
                overlay::show(
                    mtm,
                    s.config.opacity,
                    s.config.multi_monitor,
                    &s.config.per_display_opacity,
                );
            } else {
                s.config.last_opacity = s.config.opacity;
                s.config.is_enabled = false;
                config::save_config(&s.config);
                overlay::hide();
            }

            if let Some(slider) = SLIDER_REF.lock().unwrap().as_ref() {
                slider.setFloatValue(s.config.opacity * 100.0);
            }
            if let Some(label) = SLIDER_LABEL_REF.lock().unwrap().as_ref() {
                let pct = (s.config.opacity * 100.0).round() as i32;
                label.setStringValue(&NSString::from_str(&format!("{}%", pct)));
            }
        }

        #[unsafe(method(autostartToggled:))]
        fn autostart_toggled(&self, sender: &NSButton) {
            let checked = sender.state() == NSControlStateValueOn;
            style_toggle(sender, checked);
            let st = app::state();
            let mut s = st.lock().unwrap();
            s.config.launch_on_login = checked;
            config::save_config(&s.config);
            if checked {
                autostart::enable();
            } else {
                autostart::disable();
            }
        }

        #[unsafe(method(multiMonitorToggled:))]
        fn multi_monitor_toggled(&self, sender: &NSButton) {
            let checked = sender.state() == NSControlStateValueOn;
            style_toggle(sender, checked);
            let st = app::state();
            let mut s = st.lock().unwrap();
            s.config.multi_monitor = checked;
            config::save_config(&s.config);

            if overlay::is_visible() {
                let mtm = MainThreadMarker::new().unwrap();
                overlay::show(
                    mtm,
                    s.config.opacity,
                    s.config.multi_monitor,
                    &s.config.per_display_opacity,
                );
            }
        }

        #[unsafe(method(autoUpdateToggled:))]
        fn auto_update_toggled(&self, sender: &NSButton) {
            let checked = sender.state() == NSControlStateValueOn;
            style_toggle(sender, checked);
            let st = app::state();
            let mut s = st.lock().unwrap();
            s.config.auto_update = checked;
            config::save_config(&s.config);
        }

        #[unsafe(method(checkForUpdatesClicked:))]
        fn check_for_updates_clicked(&self, _sender: &NSButton) {
            std::thread::spawn(|| {
                let result = crate::updater::check_for_update(crate::updater::APP_VERSION);
                app::run_on_main(move || {
                    match result {
                        crate::updater::UpdateResult::UpdateAvailable {
                            version,
                            download_url,
                            ..
                        } => {
                            crate::ui::prompt_update(&version, &download_url);
                        }
                        crate::updater::UpdateResult::NoUpdate => {
                            crate::ui::show_alert(
                                "No Updates",
                                &format!(
                                    "You are running the latest version (v{}).",
                                    crate::updater::APP_VERSION
                                ),
                            );
                        }
                        crate::updater::UpdateResult::Error(e) => {
                            crate::ui::show_alert("Update Check Failed", &e);
                        }
                    }
                });
            });
        }

        #[unsafe(method(tabChanged:))]
        fn tab_changed(&self, sender: &NSSegmentedControl) {
            let idx = sender.selectedSegment();
            if let Some(views) = TAB_VIEWS.lock().unwrap().as_ref() {
                for (i, view) in views.iter().enumerate() {
                    view.setHidden(i as isize != idx);
                }
            }
        }

        #[unsafe(method(openKraftPixel:))]
        fn open_kraft_pixel(&self, _sender: &NSButton) {
            let url_str = NSString::from_str("https://kraftpixel.com");
            if let Some(url) = NSURL::URLWithString(&url_str) {
                let ws = NSWorkspace::sharedWorkspace();
                ws.openURL(&url);
            }
        }

        #[unsafe(method(quitApp:))]
        fn quit_app(&self, _sender: &NSButton) {
            let mtm = MainThreadMarker::new().unwrap();
            let app = NSApplication::sharedApplication(mtm);
            app.terminate(None);
        }
    }
);

impl SettingsTarget {
    fn new() -> Retained<Self> {
        let alloc = Self::alloc();
        unsafe { msg_send![alloc, init] }
    }
}

// ===========================================================================
// Public entry point
// ===========================================================================

/// Rebuild the settings window if it is currently open (e.g. after monitor change).
pub fn rebuild_settings(mtm: MainThreadMarker) {
    let mut guard = SETTINGS_WINDOW.lock().unwrap();
    if guard.is_some() {
        // Close the existing window
        if let Some(ref win) = *guard {
            win.orderOut(None);
        }
        *guard = None;
        drop(guard);
        // Re-open with fresh data
        show_settings(mtm);
    }
}

pub fn show_settings(mtm: MainThreadMarker) {
    let mut guard = SETTINGS_WINDOW.lock().unwrap();

    if let Some(ref window) = *guard {
        // Bring window to front even if behind other windows
        window.orderFrontRegardless();
        window.makeKeyAndOrderFront(None);
        let app = NSApplication::sharedApplication(mtm);
        app.activate();
        return;
    }

    let target = SettingsTarget::new();
    *SETTINGS_TARGET.lock().unwrap() = Some(target.clone());

    let st = app::state();
    let cfg = st.lock().unwrap().config.clone();

    // ── Window ──────────────────────────────────────────────────────────
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WINDOW_W, WINDOW_H));

    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable;

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc::<NSWindow>(),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    window.setTitle(&NSString::from_str("SaveMyEyes by KraftPixel"));
    window.center();

    // Float above the overlay so the settings window is never dimmed
    window.setLevel(NSScreenSaverWindowLevel + 2000);

    // Force dark appearance
    if let Some(dark) =
        NSAppearance::appearanceNamed(&NSString::from_str("NSAppearanceNameDarkAqua"))
    {
        window.setAppearance(Some(&dark));
    }

    // ── Root content view with dark background ──────────────────────────
    let content = NSView::initWithFrame(mtm.alloc::<NSView>(), frame);
    content.setWantsLayer(true);
    if let Some(layer) = content.layer() {
        let bg = color(CLR_BG);
        unsafe {
            let cg: *const std::ffi::c_void = msg_send![&*bg, CGColor];
            let _: () = msg_send![&*layer, setBackgroundColor: cg];
        }
    }

    // macOS content view height (the frame IS the content rect)
    let content_h = WINDOW_H;

    // Layout cursor: y starts from top and decreases.
    // In AppKit y=0 is at the bottom.
    let mut y = content_h - PADDING;

    // ── Header ──────────────────────────────────────────────────────────
    let icon_size = 40.0;
    y -= icon_size;

    // Eye icon using SF Symbol
    let eye_view = NSImageView::initWithFrame(
        mtm.alloc::<NSImageView>(),
        NSRect::new(NSPoint::new(PADDING, y), NSSize::new(icon_size, icon_size)),
    );
    if let Some(img) = &NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str("eye.fill"),
        None,
    ) {
        eye_view.setImage(Some(img));
    }
    eye_view.setContentTintColor(Some(&color(CLR_BRAND)));
    content.addSubview(&eye_view);

    // Title text
    let title_x = PADDING + icon_size + 12.0;
    let title = make_label(mtm, "SaveMyEyes", FONT_SIZE_TITLE, true);
    title.setFrame(NSRect::new(
        NSPoint::new(title_x, y + 18.0),
        NSSize::new(200.0, 22.0),
    ));
    content.addSubview(&title);

    let subtitle = make_label(mtm, "Screen Dimmer", FONT_SIZE_XS, false);
    subtitle.setTextColor(Some(&color(CLR_MUTED)));
    subtitle.setFrame(NSRect::new(
        NSPoint::new(title_x, y),
        NSSize::new(200.0, 16.0),
    ));
    content.addSubview(&subtitle);

    // Credit (right-aligned)
    let credit1 = make_label(mtm, "An open-source project by", FONT_SIZE_XS, false);
    credit1.setTextColor(Some(&color(CLR_MUTED)));
    credit1.setAlignment(NSTextAlignment::Right);
    credit1.setFrame(NSRect::new(
        NSPoint::new(PADDING, y + 20.0),
        NSSize::new(CONTENT_W, 14.0),
    ));
    content.addSubview(&credit1);

    // Clickable "KraftPixel" link
    let credit_btn = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("KraftPixel"),
            Some(&target as &AnyObject),
            Some(sel!(openKraftPixel:)),
            mtm,
        )
    };
    credit_btn.setBordered(false);
    let brand_font = NSFont::boldSystemFontOfSize(FONT_SIZE_SMALL);
    credit_btn.setFont(Some(&brand_font));
    credit_btn.setContentTintColor(Some(&color(CLR_BRAND)));
    credit_btn.setAlignment(NSTextAlignment::Right);
    credit_btn.setFrame(NSRect::new(
        NSPoint::new(PADDING, y + 2.0),
        NSSize::new(CONTENT_W, 18.0),
    ));
    content.addSubview(&credit_btn);

    // Separator
    y -= 12.0;
    let sep = make_separator(mtm, PADDING, y, CONTENT_W);
    content.addSubview(&sep);
    y -= 16.0;

    // ── Tab Bar (NSSegmentedControl) ────────────────────────────────────
    let tab_bar_h = TAB_H + 8.0;
    y -= tab_bar_h;

    let seg = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &NSArray::from_retained_slice(&[
                NSString::from_str("Dimmer"),
                NSString::from_str("Settings"),
                NSString::from_str("Shortcuts"),
            ]),
            NSSegmentSwitchTracking::SelectOne,
            Some(&target as &AnyObject),
            Some(sel!(tabChanged:)),
            mtm,
        )
    };
    seg.setFrame(NSRect::new(
        NSPoint::new(PADDING, y),
        NSSize::new(CONTENT_W, tab_bar_h),
    ));
    seg.setSegmentStyle(NSSegmentStyle::Capsule);
    seg.setSelectedSegment(0);
    // Equal width for all segments
    let seg_w = CONTENT_W / 3.0;
    seg.setWidth_forSegment(seg_w, 0);
    seg.setWidth_forSegment(seg_w, 1);
    seg.setWidth_forSegment(seg_w, 2);
    content.addSubview(&seg);
    y -= GAP;

    // ── Tab Content Area ────────────────────────────────────────────────
    let tab_area_h = y; // remaining space from y down to 0
    let tab_frame = NSRect::new(
        NSPoint::new(PADDING, 0.0),
        NSSize::new(CONTENT_W, tab_area_h),
    );

    let dimmer_view = build_dimmer_tab(mtm, &cfg, &target, tab_frame);
    let settings_view = build_settings_tab(mtm, &cfg, &target, tab_frame);
    let shortcuts_view = build_shortcuts_tab(mtm, tab_frame);

    // Only dimmer tab visible initially
    settings_view.setHidden(true);
    shortcuts_view.setHidden(true);

    content.addSubview(&dimmer_view);
    content.addSubview(&settings_view);
    content.addSubview(&shortcuts_view);

    *TAB_VIEWS.lock().unwrap() = Some(Mt([
        dimmer_view.clone(),
        settings_view.clone(),
        shortcuts_view.clone(),
    ]));

    window.setContentView(Some(&content));

    let app = NSApplication::sharedApplication(mtm);
    app.activate();
    window.orderFrontRegardless();
    window.makeKeyAndOrderFront(None);

    *guard = Some(Mt(window));
}

// ===========================================================================
// Tab builders
// ===========================================================================

fn build_dimmer_tab(
    mtm: MainThreadMarker,
    cfg: &config::AppConfig,
    target: &SettingsTarget,
    frame: NSRect,
) -> Retained<NSView> {
    let container = NSView::initWithFrame(mtm.alloc::<NSView>(), frame);
    let w = frame.size.width;
    let top = frame.size.height;

    let inner_pad = 20.0;
    let inner_w = w - inner_pad * 2.0;

    // Detect connected monitors and get their display names
    let display_names = overlay::screen_names(mtm);
    let monitor_count = display_names.len().max(1);

    // Store display names for handler lookup
    *MONITOR_NAMES.lock().unwrap() = display_names.clone();

    // Clear per-monitor refs
    MONITOR_SLIDER_REFS.lock().unwrap().clear();
    MONITOR_LABEL_REFS.lock().unwrap().clear();

    let card_h = 110.0;
    let mut current_y = top;

    // ── Per-monitor dimming cards ───────────────────────────────────────
    for idx in 0..monitor_count {
        current_y -= card_h;
        let card = make_card(mtm, 0.0, current_y, w, card_h);

        // Title: use actual display name, truncated if too long
        let raw_name = display_names.get(idx).cloned().unwrap_or_else(|| format!("Monitor {}", idx + 1));
        let title_text = if monitor_count == 1 {
            "Dimming Level".to_string()
        } else {
            let truncated = if raw_name.len() > 22 {
                format!("{}…", &raw_name[..21])
            } else {
                raw_name.clone()
            };
            truncated
        };
        let title = make_label(mtm, &title_text, FONT_SIZE_SMALL, true);
        title.setFrame(NSRect::new(
            NSPoint::new(inner_pad, card_h - 14.0 - 16.0),
            NSSize::new(200.0, 16.0),
        ));
        add_to_card(&card, &title);

        // Per-display opacity (fallback to global) — lookup by display name
        let opacity = cfg
            .per_display_opacity
            .get(&raw_name)
            .copied()
            .unwrap_or(cfg.opacity);
        let pct = (opacity * 100.0).round() as i32;

        // Percentage badge
        let badge_w = 48.0;
        let badge_h = 22.0;
        let (badge_view, badge_label) = make_badge(mtm, &format!("{}%", pct), badge_w, badge_h);
        badge_view.setFrame(NSRect::new(
            NSPoint::new(inner_w + inner_pad - badge_w, card_h - 14.0 - 17.0),
            NSSize::new(badge_w, badge_h),
        ));
        add_to_card(&card, &badge_view);

        // Slider
        let slider_y = card_h - 58.0;
        let slider = NSSlider::initWithFrame(
            mtm.alloc::<NSSlider>(),
            NSRect::new(
                NSPoint::new(inner_pad, slider_y),
                NSSize::new(inner_w, 24.0),
            ),
        );
        slider.setMinValue(0.0);
        slider.setMaxValue(90.0);
        slider.setFloatValue(opacity * 100.0);
        slider.setContinuous(true);
        // Tag identifies which monitor this slider controls
        let _: () = unsafe { msg_send![&slider, setTag: idx as isize] };
        unsafe {
            slider.setTarget(Some(target as &AnyObject));
            slider.setAction(Some(sel!(monitorSliderChanged:)));
        }
        add_to_card(&card, &slider);

        // Store per-monitor refs
        MONITOR_SLIDER_REFS.lock().unwrap().push(Mt(slider.clone()));
        MONITOR_LABEL_REFS.lock().unwrap().push(Mt(badge_label.clone()));

        // Monitor 0 is also the "global" slider
        if idx == 0 {
            *SLIDER_REF.lock().unwrap() = Some(Mt(slider.clone()));
            *SLIDER_LABEL_REF.lock().unwrap() = Some(Mt(badge_label.clone()));
        }

        // Range labels
        let range_y = slider_y - 16.0;
        let min_lbl = make_label(mtm, "0%", FONT_SIZE_XS, false);
        min_lbl.setTextColor(Some(&color(CLR_MUTED)));
        min_lbl.setFrame(NSRect::new(
            NSPoint::new(inner_pad, range_y),
            NSSize::new(40.0, 14.0),
        ));
        add_to_card(&card, &min_lbl);

        let max_lbl = make_label(mtm, "90%", FONT_SIZE_XS, false);
        max_lbl.setTextColor(Some(&color(CLR_MUTED)));
        max_lbl.setAlignment(NSTextAlignment::Right);
        max_lbl.setFrame(NSRect::new(
            NSPoint::new(inner_w + inner_pad - 40.0, range_y),
            NSSize::new(40.0, 14.0),
        ));
        add_to_card(&card, &max_lbl);

        container.addSubview(&card);
        current_y -= GAP;
    }

    // ── Dimmer Enabled card ─────────────────────────────────────────────
    let card2_h = 56.0;
    current_y -= card2_h;
    let card2 = make_card(mtm, 0.0, current_y, w, card2_h);

    // Vertically center the two-line text block (title + desc) within the card
    let text_block_h = 30.0; // title(14) + gap(2) + desc(14)
    let text_top = (card2_h + text_block_h) / 2.0;

    let en_title = make_label(mtm, "Dimmer Enabled", FONT_SIZE_SMALL, true);
    en_title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, text_top - 14.0),
        NSSize::new(200.0, 14.0),
    ));
    add_to_card(&card2, &en_title);

    let en_desc = make_label(mtm, "Apply dimming overlay to screen", FONT_SIZE_XS, false);
    en_desc.setTextColor(Some(&color(CLR_MUTED)));
    en_desc.setFrame(NSRect::new(
        NSPoint::new(inner_pad, text_top - text_block_h),
        NSSize::new(250.0, 14.0),
    ));
    add_to_card(&card2, &en_desc);

    let toggle = make_switch(mtm, target, sel!(enabledToggled:), cfg.is_enabled);
    toggle.setFrame(NSRect::new(
        NSPoint::new(w - inner_pad - TOGGLE_W, (card2_h - TOGGLE_H) / 2.0),
        NSSize::new(TOGGLE_W, TOGGLE_H),
    ));
    add_to_card(&card2, &toggle);
    *ENABLED_TOGGLE_REF.lock().unwrap() = Some(Mt(toggle.clone()));

    container.addSubview(&card2);

    container
}

fn build_settings_tab(
    mtm: MainThreadMarker,
    cfg: &config::AppConfig,
    target: &SettingsTarget,
    frame: NSRect,
) -> Retained<NSView> {
    let container = NSView::initWithFrame(mtm.alloc::<NSView>(), frame);
    let w = frame.size.width;
    let top = frame.size.height;
    let inner_pad = 20.0;
    let inner_w = w - inner_pad * 2.0;

    // ── Card 1: General ─────────────────────────────────────────────────
    let card1_h = 130.0;
    let card1_y = top - card1_h;
    let card1 = make_card(mtm, 0.0, card1_y, w, card1_h);

    // "General" title
    let gen_title = make_label(mtm, "General", FONT_SIZE_SMALL, true);
    gen_title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, card1_h - 14.0 - 14.0),
        NSSize::new(200.0, 16.0),
    ));
    add_to_card(&card1, &gen_title);

    // Layout: header 28px from top, two rows with divider in remaining space
    let header_bottom = card1_h - 28.0;
    let row_h = 32.0; // title(16) + desc(14) + gap(2)
    let div_gap = 12.0;
    let content_h = row_h * 2.0 + div_gap;
    let content_bot = (header_bottom - content_h) / 2.0;

    // Row 2 (bottom): Multi-Monitor Brightness
    let r2_center = content_bot + row_h / 2.0;
    let mm_title = make_label(mtm, "Multi-Monitor Brightness", FONT_SIZE_SMALL, true);
    mm_title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, r2_center),
        NSSize::new(250.0, 16.0),
    ));
    add_to_card(&card1, &mm_title);

    let mm_desc = make_label(mtm, "Independent dimming per monitor", FONT_SIZE_XS, false);
    mm_desc.setTextColor(Some(&color(CLR_MUTED)));
    mm_desc.setFrame(NSRect::new(
        NSPoint::new(inner_pad, r2_center - 16.0),
        NSSize::new(250.0, 14.0),
    ));
    add_to_card(&card1, &mm_desc);

    let mm_toggle = make_switch(mtm, target, sel!(multiMonitorToggled:), cfg.multi_monitor);
    mm_toggle.setFrame(NSRect::new(
        NSPoint::new(w - inner_pad - TOGGLE_W, r2_center - TOGGLE_H / 2.0 + 1.0),
        NSSize::new(TOGGLE_W, TOGGLE_H),
    ));
    add_to_card(&card1, &mm_toggle);

    // Divider
    let div1_y = content_bot + row_h + div_gap / 2.0;
    let divider1 = make_separator(mtm, inner_pad, div1_y, inner_w);
    add_to_card(&card1, &divider1);

    // Row 1 (top): Start on Login
    let r1_center = content_bot + row_h + div_gap + row_h / 2.0;
    let login_title = make_label(mtm, "Start on Login", FONT_SIZE_SMALL, true);
    login_title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, r1_center),
        NSSize::new(200.0, 16.0),
    ));
    add_to_card(&card1, &login_title);

    let login_desc = make_label(mtm, "Launch automatically at startup", FONT_SIZE_XS, false);
    login_desc.setTextColor(Some(&color(CLR_MUTED)));
    login_desc.setFrame(NSRect::new(
        NSPoint::new(inner_pad, r1_center - 16.0),
        NSSize::new(250.0, 14.0),
    ));
    add_to_card(&card1, &login_desc);

    let login_toggle = make_switch(mtm, target, sel!(autostartToggled:), cfg.launch_on_login);
    login_toggle.setFrame(NSRect::new(
        NSPoint::new(w - inner_pad - TOGGLE_W, r1_center - TOGGLE_H / 2.0 + 1.0),
        NSSize::new(TOGGLE_W, TOGGLE_H),
    ));
    add_to_card(&card1, &login_toggle);

    container.addSubview(&card1);

    // ── Card 2: Updates ─────────────────────────────────────────────────
    let card2_h = 140.0;
    let card2_y = card1_y - GAP - card2_h;
    let card2 = make_card(mtm, 0.0, card2_y, w, card2_h);

    // "Updates" title + version
    let upd_title = make_label(mtm, "Updates", FONT_SIZE_SMALL, true);
    upd_title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, card2_h - 14.0 - 14.0),
        NSSize::new(100.0, 16.0),
    ));
    add_to_card(&card2, &upd_title);

    let ver_label = make_label(
        mtm,
        &format!("v{}", crate::updater::APP_VERSION),
        FONT_SIZE_XS,
        false,
    );
    ver_label.setTextColor(Some(&color(CLR_MUTED)));
    ver_label.setFrame(NSRect::new(
        NSPoint::new(inner_pad + 60.0, card2_h - 14.0 - 15.0),
        NSSize::new(80.0, 14.0),
    ));
    add_to_card(&card2, &ver_label);

    // Row 1: Auto-Update — vertically centered with toggle
    let header2_bottom = card2_h - 28.0;
    let row_h2 = 32.0;
    let div_gap2 = 12.0;
    let btn_row_h = 28.0;
    let content2_h = row_h2 + div_gap2 + btn_row_h;
    let content2_bot = (header2_bottom - content2_h) / 2.0;

    // Bottom row: Check for Updates
    let chk_center = content2_bot + btn_row_h / 2.0;
    let chk_title = make_label(mtm, "Check for Updates", FONT_SIZE_SMALL, true);
    chk_title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, chk_center - 8.0),
        NSSize::new(200.0, 16.0),
    ));
    add_to_card(&card2, &chk_title);

    // "Check Now" button
    let check_btn = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Check Now"),
            Some(target as &AnyObject),
            Some(sel!(checkForUpdatesClicked:)),
            mtm,
        )
    };
    check_btn.setBezelStyle(NSBezelStyle::Push);
    check_btn.setFrame(NSRect::new(
        NSPoint::new(w - inner_pad - 100.0, chk_center - 14.0),
        NSSize::new(100.0, 28.0),
    ));
    add_to_card(&card2, &check_btn);

    // Divider
    let div2_y = content2_bot + btn_row_h + div_gap2 / 2.0;
    let divider = make_separator(mtm, inner_pad, div2_y, inner_w);
    add_to_card(&card2, &divider);

    // Top row: Auto-Update
    let au_center = content2_bot + btn_row_h + div_gap2 + row_h2 / 2.0;
    let au_title = make_label(mtm, "Auto-Update", FONT_SIZE_SMALL, true);
    au_title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, au_center),
        NSSize::new(200.0, 16.0),
    ));
    add_to_card(&card2, &au_title);

    let au_desc = make_label(
        mtm,
        "Automatically download and install updates",
        FONT_SIZE_XS,
        false,
    );
    au_desc.setTextColor(Some(&color(CLR_MUTED)));
    au_desc.setFrame(NSRect::new(
        NSPoint::new(inner_pad, au_center - 16.0),
        NSSize::new(280.0, 14.0),
    ));
    add_to_card(&card2, &au_desc);

    let au_toggle = make_switch(mtm, target, sel!(autoUpdateToggled:), cfg.auto_update);
    au_toggle.setFrame(NSRect::new(
        NSPoint::new(w - inner_pad - TOGGLE_W, au_center - TOGGLE_H / 2.0 + 1.0),
        NSSize::new(TOGGLE_W, TOGGLE_H),
    ));
    add_to_card(&card2, &au_toggle);

    container.addSubview(&card2);

    // ── Quit Button ─────────────────────────────────────────────────────
    let quit_btn_h = 36.0;
    let quit_btn_y = card2_y - GAP - quit_btn_h;
    let quit_btn = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Quit SaveMyEyes"),
            Some(target as &AnyObject),
            Some(sel!(quitApp:)),
            mtm,
        )
    };
    quit_btn.setBezelStyle(NSBezelStyle::Push);
    quit_btn.setFrame(NSRect::new(
        NSPoint::new(0.0, quit_btn_y),
        NSSize::new(w, quit_btn_h),
    ));
    // Red-tinted text for quit button
    let quit_font = NSFont::systemFontOfSize(FONT_SIZE_SMALL);
    quit_btn.setFont(Some(&quit_font));
    quit_btn.setContentTintColor(Some(&NSColor::colorWithRed_green_blue_alpha(
        0.90, 0.30, 0.30, 1.0,
    )));
    container.addSubview(&quit_btn);

    container
}

fn build_shortcuts_tab(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSView> {
    let container = NSView::initWithFrame(mtm.alloc::<NSView>(), frame);
    let w = frame.size.width;
    let top = frame.size.height;
    let inner_pad = 20.0;

    // ── Card: Keyboard Shortcuts ────────────────────────────────────────
    let card_h = 190.0;
    let card_y = top - card_h;
    let card = make_card(mtm, 0.0, card_y, w, card_h);

    let title = make_label(mtm, "Keyboard Shortcuts", FONT_SIZE_SMALL, true);
    title.setFrame(NSRect::new(
        NSPoint::new(inner_pad, card_h - 14.0 - 16.0),
        NSSize::new(200.0, 16.0),
    ));
    add_to_card(&card, &title);

    // Each shortcut: (label, list-of-individual-keys)
    let shortcuts: &[(&str, &[&str])] = &[
        ("Toggle Dimmer", &["⌘", "⇧", "D"]),
        ("Increase Dimming", &["⌘", "⇧", ">"]),
        ("Decrease Dimming", &["⌘", "⇧", "<"]),
    ];

    let key_w = 26.0_f64;
    let key_h = 26.0_f64;
    let plus_w = 14.0_f64;
    let key_gap = 4.0_f64;

    let mut row_y = card_h - 54.0;
    for (action, keys) in shortcuts {
        // Action label
        let action_lbl = make_label(mtm, action, FONT_SIZE_SMALL, false);
        action_lbl.setTextColor(Some(&color(CLR_MUTED)));
        action_lbl.setFrame(NSRect::new(
            NSPoint::new(inner_pad, row_y),
            NSSize::new(200.0, 16.0),
        ));
        add_to_card(&card, &action_lbl);

        // Build key pills right-aligned with "+" separators
        let n = keys.len();
        let total_w = (n as f64) * key_w
            + ((n - 1) as f64) * (plus_w + key_gap * 2.0);
        let mut x = w - inner_pad - total_w;
        let badge_y = row_y - (key_h - 16.0) / 2.0; // vertically center with action label

        for (i, key) in keys.iter().enumerate() {
            // Key pill (centered text inside styled container)
            let (key_view, _key_label) = make_key_pill(mtm, key, key_w, key_h);
            key_view.setFrame(NSRect::new(
                NSPoint::new(x, badge_y),
                NSSize::new(key_w, key_h),
            ));
            add_to_card(&card, &key_view);
            x += key_w;

            // "+" separator between keys
            if i < n - 1 {
                x += key_gap;
                let plus = make_label(mtm, "+", FONT_SIZE_XS, false);
                plus.setTextColor(Some(&color(CLR_MUTED)));
                plus.setAlignment(NSTextAlignment::Center);
                plus.setFrame(NSRect::new(
                    NSPoint::new(x, badge_y + (key_h - 14.0) / 2.0),
                    NSSize::new(plus_w, 14.0),
                ));
                add_to_card(&card, &plus);
                x += plus_w + key_gap;
            }
        }

        row_y -= 40.0;
    }

    container.addSubview(&card);

    // Hint text — color depends on accessibility permission status
    let has_access = crate::hotkeys::is_accessibility_granted();
    let hint_text = if has_access {
        "Accessibility permission granted"
    } else {
        "Global hotkeys require Accessibility permission"
    };
    let hint = make_label(mtm, hint_text, FONT_SIZE_XS, false);
    let hint_color = if has_access {
        NSColor::colorWithRed_green_blue_alpha(0.30, 0.80, 0.40, 1.0) // green
    } else {
        NSColor::colorWithRed_green_blue_alpha(0.90, 0.30, 0.30, 1.0) // red
    };
    hint.setTextColor(Some(&hint_color));
    hint.setAlignment(NSTextAlignment::Center);
    hint.setFrame(NSRect::new(
        NSPoint::new(0.0, card_y - 24.0),
        NSSize::new(w, 14.0),
    ));
    container.addSubview(&hint);

    container
}

// ===========================================================================
// Helper: card (NSBox with custom style)
// ===========================================================================

fn make_card(mtm: MainThreadMarker, x: f64, y: f64, w: f64, h: f64) -> Retained<NSBox> {
    let card = NSBox::initWithFrame(
        mtm.alloc::<NSBox>(),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)),
    );
    card.setBoxType(NSBoxType(4)); // NSBoxCustom = 4
    card.setTitlePosition(NSTitlePosition::NoTitle);
    card.setCornerRadius(CARD_RADIUS);
    card.setBorderWidth(1.0);
    card.setBorderColor(&color(CLR_SECONDARY));
    card.setFillColor(&color(CLR_BG));
    card.setContentViewMargins(NSSize::new(0.0, 0.0));
    card
}

fn add_to_card(card: &NSBox, view: &NSView) {
    if let Some(content) = card.contentView() {
        content.addSubview(view);
    }
}

// ===========================================================================
// Helper: separator line
// ===========================================================================

fn make_separator(mtm: MainThreadMarker, x: f64, y: f64, w: f64) -> Retained<NSBox> {
    let sep = NSBox::initWithFrame(
        mtm.alloc::<NSBox>(),
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, 1.0)),
    );
    sep.setBoxType(NSBoxType(4)); // Custom
    sep.setTitlePosition(NSTitlePosition::NoTitle);
    sep.setCornerRadius(0.0);
    sep.setBorderWidth(0.0);
    sep.setFillColor(&color(CLR_SECONDARY));
    sep.setContentViewMargins(NSSize::new(0.0, 0.0));
    sep
}

// ===========================================================================
// Helper: NSSwitch toggle
// ===========================================================================

fn make_switch(
    mtm: MainThreadMarker,
    target: &SettingsTarget,
    action: objc2::runtime::Sel,
    is_on: bool,
) -> Retained<NSButton> {
    let button = {
        NSButton::initWithFrame(
            mtm.alloc::<NSButton>(),
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(TOGGLE_W, TOGGLE_H),
            ),
        )
    };
    button.setButtonType(NSButtonType::OnOff);
    button.setBordered(false);
    button.setTitle(&NSString::from_str(""));
    button.setAlternateTitle(&NSString::from_str(""));
    // Suppress default highlight/state drawing — our layers handle everything
    unsafe {
        let cell: Retained<AnyObject> = msg_send![&button, cell];
        let _: () = msg_send![&*cell, setHighlightsBy: 0_isize];
        let _: () = msg_send![&*cell, setShowsStateBy: 0_isize];
    }
    button.setState(if is_on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    });
    unsafe {
        button.setTarget(Some(target as &AnyObject));
        button.setAction(Some(action));
    }

    // Layer-based custom toggle appearance
    style_toggle(&button, is_on);

    button
}

/// Update the visual appearance of a custom toggle button (capsule + knob).
fn style_toggle(button: &NSButton, is_on: bool) {
    button.setWantsLayer(true);
    if let Some(layer) = button.layer() {
        unsafe {
            let radius = TOGGLE_H / 2.0;
            let _: () = msg_send![&*layer, setCornerRadius: radius];
            let _: () = msg_send![&*layer, setMasksToBounds: true];

            // Capsule background: brand purple when ON, dark grey when OFF
            let bg = if is_on {
                color(CLR_BRAND)
            } else {
                color(CLR_TOGGLE_OFF)
            };
            let cg: *const std::ffi::c_void = msg_send![&*bg, CGColor];
            let _: () = msg_send![&*layer, setBackgroundColor: cg];

            // Remove previous knob sublayer (named "knob")
            let sublayers: Option<Retained<NSArray<AnyObject>>> = msg_send![&*layer, sublayers];
            if let Some(subs) = sublayers {
                for i in (0..subs.count()).rev() {
                    let sub = subs.objectAtIndex(i);
                    let name: Option<Retained<NSString>> = msg_send![&*sub, name];
                    if let Some(n) = name {
                        if n.to_string() == "knob" {
                            let _: () = msg_send![&*sub, removeFromSuperlayer];
                        }
                    }
                }
            }

            // White knob circle
            let knob_d = TOGGLE_H - 6.0;
            let knob_x = if is_on {
                TOGGLE_W - knob_d - 3.0
            } else {
                3.0
            };
            let cls = objc2::runtime::AnyClass::get(c"CALayer").unwrap();
            let knob: Retained<AnyObject> = msg_send![cls, new];
            let frame = NSRect::new(
                NSPoint::new(knob_x, 3.0),
                NSSize::new(knob_d, knob_d),
            );
            let _: () = msg_send![&*knob, setFrame: frame];
            let _: () = msg_send![&*knob, setCornerRadius: knob_d / 2.0];
            let _: () = msg_send![&*knob, setName: &*NSString::from_str("knob")];

            let white = NSColor::colorWithRed_green_blue_alpha(1.0, 1.0, 1.0, 1.0);
            let wcg: *const std::ffi::c_void = msg_send![&*white, CGColor];
            let _: () = msg_send![&*knob, setBackgroundColor: wcg];

            let _: () = msg_send![&*layer, addSublayer: &*knob];
        }
    }
}

// ===========================================================================
// Helper: badge (percentage pill)
// ===========================================================================

fn make_badge(
    mtm: MainThreadMarker,
    text: &str,
    w: f64,
    h: f64,
) -> (Retained<NSView>, Retained<NSTextField>) {
    // Container view with purple background + rounded corners
    let container = NSView::initWithFrame(
        mtm.alloc::<NSView>(),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    );
    container.setWantsLayer(true);
    if let Some(layer) = container.layer() {
        unsafe {
            let bg = color(CLR_BRAND);
            let cg: *const AnyObject = msg_send![&*bg, CGColor];
            let _: () = msg_send![&*layer, setBackgroundColor: cg];
            let _: () = msg_send![&*layer, setCornerRadius: 6.0_f64];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
        }
    }

    // Text label — transparent background, vertically centered inside container
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);
    label.setAlignment(NSTextAlignment::Center);

    let font = NSFont::boldSystemFontOfSize(FONT_SIZE_XS);
    label.setFont(Some(&font));
    label.setTextColor(Some(&color(CLR_FG)));

    // Size to fit the text, then center vertically
    label.sizeToFit();
    let text_h = label.frame().size.height;
    let y_offset = (h - text_h) / 2.0;
    label.setFrame(NSRect::new(
        NSPoint::new(0.0, y_offset),
        NSSize::new(w, text_h),
    ));

    container.addSubview(&label);

    (container, label)
}

// ===========================================================================
// Helper: key pill (keyboard shortcut, centered text)
// ===========================================================================

fn make_key_pill(
    mtm: MainThreadMarker,
    text: &str,
    w: f64,
    h: f64,
) -> (Retained<NSView>, Retained<NSTextField>) {
    let container = NSView::initWithFrame(
        mtm.alloc::<NSView>(),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)),
    );
    container.setWantsLayer(true);
    if let Some(layer) = container.layer() {
        unsafe {
            let bg = color(CLR_SECONDARY);
            let cg: *const AnyObject = msg_send![&*bg, CGColor];
            let _: () = msg_send![&*layer, setBackgroundColor: cg];
            let _: () = msg_send![&*layer, setCornerRadius: 6.0_f64];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
            let border_c = color(CLR_SECONDARY);
            let cg2: *const AnyObject = msg_send![&*border_c, CGColor];
            let _: () = msg_send![&*layer, setBorderColor: cg2];
            let _: () = msg_send![&*layer, setBorderWidth: 1.0_f64];
        }
    }

    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);
    label.setAlignment(NSTextAlignment::Center);

    let font = NSFont::monospacedSystemFontOfSize_weight(FONT_SIZE_NORMAL, 0.40);
    label.setFont(Some(&font));
    label.setTextColor(Some(&color(CLR_MUTED)));

    label.sizeToFit();
    let text_h = label.frame().size.height;
    let y_offset = (h - text_h) / 2.0;
    label.setFrame(NSRect::new(
        NSPoint::new(0.0, y_offset),
        NSSize::new(w, text_h),
    ));

    container.addSubview(&label);
    (container, label)
}

// ===========================================================================
// Helper: color from tuple
// ===========================================================================

fn color(c: (f64, f64, f64)) -> Retained<NSColor> {
    NSColor::colorWithRed_green_blue_alpha(c.0, c.1, c.2, 1.0)
}

// ===========================================================================
// Helper: label
// ===========================================================================

fn make_label(mtm: MainThreadMarker, text: &str, size: f64, bold: bool) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setEditable(false);
    label.setSelectable(false);

    let font = if bold {
        NSFont::boldSystemFontOfSize(size)
    } else {
        NSFont::systemFontOfSize(size)
    };
    label.setFont(Some(&font));
    label.setTextColor(Some(&color(CLR_FG)));

    label
}
