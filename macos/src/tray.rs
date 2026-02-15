// macOS system tray (menu bar status item) using NSStatusBar.
//
// Creates an NSStatusItem with a menu containing:
//   • Opacity slider (per-monitor sliders in multi-monitor mode)
//   • Toggle Dimmer
//   • Settings (opens preferences window — future)
//   • Quit

use objc2::rc::Retained;
use objc2::{msg_send, sel, MainThreadMarker};
use objc2_app_kit::{
    NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::NSString;

use std::sync::Mutex;

static STATUS_ITEM: Mutex<Option<Retained<NSStatusItem>>> = Mutex::new(None);

/// Set up the system tray icon and menu.
pub fn setup(mtm: MainThreadMarker) {
    let status_bar = NSStatusBar::systemStatusBar(mtm);
    let item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

    // Set the icon text (emoji eye as placeholder; swap to a real icon later)
    if let Some(button) = item.button(mtm) {
        let title = NSString::from_str("👁");
        button.setTitle(&title);
    }

    // Build the menu
    let menu = build_menu();
    item.setMenu(Some(&menu));

    *STATUS_ITEM.lock().unwrap() = Some(item);
}

/// Remove the tray icon.
pub fn remove() {
    let mut guard = STATUS_ITEM.lock().unwrap();
    if let Some(item) = guard.take() {
        let status_bar = unsafe { NSStatusBar::systemStatusBar(MainThreadMarker::new().unwrap()) };
        status_bar.removeStatusItem(&item);
    }
}

fn build_menu() -> Retained<NSMenu> {
    unsafe {
        let menu = NSMenu::new();

        // Toggle Dimmer
        let toggle_title = NSString::from_str("Toggle Dimmer");
        let toggle_key = NSString::from_str("e"); // Cmd+E as menu shortcut
        let toggle_item =
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(),
                &toggle_title,
                Some(sel!(toggleDimmer:)),
                &toggle_key,
            );
        menu.addItem(&toggle_item);

        // Separator
        menu.addItem(&NSMenuItem::separatorItem());

        // Quit
        let quit_title = NSString::from_str("Quit SaveMyEyes");
        let quit_key = NSString::from_str("q");
        let quit_item =
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(),
                &quit_title,
                Some(sel!(terminate:)),
                &quit_key,
            );
        menu.addItem(&quit_item);

        menu
    }
}
