// macOS system tray (menu bar status item) using NSStatusBar.
//
// Creates an NSStatusItem with a menu containing:
//   • Opacity percentage display
//   • Toggle Dimmer (Cmd+Shift+D)
//   • Settings (Cmd+,) — opens preferences window
//   • Check for Updates
//   • Quit (Cmd+Q)
//
// Menu actions are dispatched via a custom TrayTarget that implements
// Objective-C selectors using define_class!.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AllocAnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

use std::sync::Mutex;

// Safety: All tray state is accessed exclusively on the main thread.
struct Mt<T>(T);
unsafe impl<T> Send for Mt<T> {}
unsafe impl<T> Sync for Mt<T> {}
impl<T> std::ops::Deref for Mt<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}

static STATUS_ITEM: Mutex<Option<Mt<Retained<NSStatusItem>>>> = Mutex::new(None);
static TRAY_TARGET: Mutex<Option<Retained<TrayTarget>>> = Mutex::new(None);

// ── TrayTarget — receives menu actions ──────────────────────────────────────

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "TrayTarget"]
    #[thread_kind = AllocAnyThread]
    struct TrayTarget;

    unsafe impl NSObjectProtocol for TrayTarget {}

    impl TrayTarget {
        #[unsafe(method(toggleDimmer:))]
        fn toggle_dimmer(&self, _sender: *mut NSObject) {
            eprintln!("SaveMyEyes: toggleDimmer called");
            crate::app::dispatch_hotkey(crate::hotkeys::HotkeyAction::Toggle);
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: *mut NSObject) {
            eprintln!("SaveMyEyes: openSettings called");
            let mtm = MainThreadMarker::new().unwrap();
            crate::ui::show_settings(mtm);
        }

        #[unsafe(method(checkForUpdates:))]
        fn check_for_updates(&self, _sender: *mut NSObject) {
            eprintln!("SaveMyEyes: checkForUpdates called");
            std::thread::spawn(|| {
                let result = crate::updater::check_for_update(crate::updater::APP_VERSION);
                crate::app::run_on_main(move || {
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
                                "No Updates Available",
                                &format!(
                                    "You're running the latest version (v{}).",
                                    crate::updater::APP_VERSION
                                ),
                            );
                        }
                        crate::updater::UpdateResult::Error(e) => {
                            crate::ui::show_alert(
                                "Update Check Failed",
                                &format!("Could not check for updates: {}", e),
                            );
                        }
                    }
                });
            });
        }

        #[unsafe(method(quitApp:))]
        fn quit_app(&self, _sender: *mut NSObject) {
            eprintln!("SaveMyEyes: quitApp called");
            let mtm = MainThreadMarker::new().unwrap();
            let app = NSApplication::sharedApplication(mtm);
            app.terminate(None);
        }
    }
);

impl TrayTarget {
    fn new() -> Retained<Self> {
        let alloc = Self::alloc();
        unsafe { msg_send![alloc, init] }
    }
}

/// Set up the system tray icon and menu.
pub fn setup(mtm: MainThreadMarker) {
    eprintln!("SaveMyEyes: Setting up tray...");
    let status_bar = NSStatusBar::systemStatusBar();
    let item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

    // Set the icon using SF Symbol for a native macOS look
    if let Some(button) = item.button(mtm) {
        // Try to use SF Symbol "eye.fill" for macOS 11+
        let symbol_name = NSString::from_str("eye.fill");
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &symbol_name,
            None,
        ) {
            image.setTemplate(true); // Makes it adapt to menu bar theme (light/dark)
            button.setImage(Some(&image));
        } else {
            // Fallback to emoji for older macOS versions
            let title = NSString::from_str("👁");
            button.setTitle(&title);
        }
    }

    // Create the target for menu actions
    let target = TrayTarget::new();
    eprintln!("SaveMyEyes: TrayTarget created: {:p}", &*target);

    // Verify class registration at runtime
    {
        use objc2::runtime::AnyClass;
        use std::ffi::CStr;
        let class_name = CStr::from_bytes_with_nul(b"TrayTarget\0").unwrap();
        if let Some(cls) = AnyClass::get(class_name) {
            eprintln!("SaveMyEyes: TrayTarget class found: {:?}", cls.name());
            let responds = target.respondsToSelector(sel!(toggleDimmer:));
            eprintln!("SaveMyEyes: respondsToSelector(toggleDimmer:) = {}", responds);
            let responds2 = target.respondsToSelector(sel!(quitApp:));
            eprintln!("SaveMyEyes: respondsToSelector(quitApp:) = {}", responds2);
        } else {
            eprintln!("SaveMyEyes: ERROR - TrayTarget class NOT found!");
        }
    }

    // Build the menu
    let menu = build_menu(mtm, &target);
    item.setMenu(Some(&menu));
    eprintln!("SaveMyEyes: Menu set on status item");

    *STATUS_ITEM.lock().unwrap() = Some(Mt(item));
    *TRAY_TARGET.lock().unwrap() = Some(target);
    eprintln!("SaveMyEyes: Tray setup complete");
}

/// Remove the tray icon.
#[allow(dead_code)]
pub fn remove() {
    let mut guard = STATUS_ITEM.lock().unwrap();
    if let Some(item) = guard.take() {
        let status_bar = NSStatusBar::systemStatusBar();
        status_bar.removeStatusItem(&item);
    }
}

/// Update the menu to reflect current state (called after hotkey actions).
pub fn update_menu(mtm: MainThreadMarker) {
    let target_guard = TRAY_TARGET.lock().unwrap();
    let target = match target_guard.as_ref() {
        Some(t) => t,
        None => return,
    };

    let item_guard = STATUS_ITEM.lock().unwrap();
    if let Some(item) = item_guard.as_ref() {
        let menu = build_menu(mtm, target);
        item.setMenu(Some(&menu));
    }
}

fn build_menu(mtm: MainThreadMarker, target: &TrayTarget) -> Retained<NSMenu> {
    unsafe {
        let menu = NSMenu::new(mtm);

        // Disable auto-validation so items are not greyed out.
        // Without this, NSMenu checks if the target responds to
        // validateMenuItem: and disables items when it doesn't.
        menu.setAutoenablesItems(false);

        // Status line: show current opacity
        let st = crate::app::state();
        let cfg = st.lock().unwrap();
        let status_text = if cfg.config.is_enabled {
            format!(
                "Dimming: {}%",
                (cfg.config.opacity * 100.0).round() as i32
            )
        } else {
            "Dimming: Off".to_string()
        };
        drop(cfg);

        let status_title = NSString::from_str(&status_text);
        let empty_key = NSString::from_str("");
        let status_item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &status_title,
            None,
            &empty_key,
        );
        status_item.setEnabled(false);
        menu.addItem(&status_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        // Toggle Dimmer
        let toggle_title = NSString::from_str("Toggle Dimmer");
        let toggle_key = NSString::from_str("D"); // Cmd+Shift+D (uppercase = Shift)
        let toggle_item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &toggle_title,
            Some(sel!(toggleDimmer:)),
            &toggle_key,
        );
        toggle_item.setTarget(Some(target as &AnyObject));
        toggle_item.setEnabled(true);
        menu.addItem(&toggle_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        // Settings
        let settings_title = NSString::from_str("Settings\u{2026}");
        let settings_key = NSString::from_str(","); // Cmd+,
        let settings_item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &settings_title,
            Some(sel!(openSettings:)),
            &settings_key,
        );
        settings_item.setTarget(Some(target as &AnyObject));
        settings_item.setEnabled(true);
        menu.addItem(&settings_item);

        // Check for Updates
        let update_title = NSString::from_str("Check for Updates\u{2026}");
        let update_item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &update_title,
            Some(sel!(checkForUpdates:)),
            &empty_key,
        );
        update_item.setTarget(Some(target as &AnyObject));
        update_item.setEnabled(true);
        menu.addItem(&update_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        // Quit
        let quit_title = NSString::from_str("Quit SaveMyEyes");
        let quit_key = NSString::from_str("q");
        let quit_item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &quit_title,
            Some(sel!(quitApp:)),
            &quit_key,
        );
        quit_item.setTarget(Some(target as &AnyObject));
        quit_item.setEnabled(true);
        menu.addItem(&quit_item);

        menu
    }
}
