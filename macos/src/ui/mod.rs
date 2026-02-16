mod settings;
mod theme;

pub use settings::show_settings;
pub use settings::update_ui;
pub use settings::rebuild_settings;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication};
use objc2_foundation::NSString;

/// Show an informational alert dialog.
pub fn show_alert(title: &str, message: &str) {
    let mtm = MainThreadMarker::new().unwrap();
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Informational);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(message));
    alert.addButtonWithTitle(&NSString::from_str("OK"));
    alert.runModal();
}

/// Prompt the user about an available update.
pub fn prompt_update(version: &str, download_url: &str) {
    let mtm = MainThreadMarker::new().unwrap();
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Informational);
    alert.setMessageText(&NSString::from_str("Update Available"));
    alert.setInformativeText(&NSString::from_str(&format!(
        "SaveMyEyes v{} is available. Would you like to download and install it?",
        version
    )));
    alert.addButtonWithTitle(&NSString::from_str("Update Now"));
    alert.addButtonWithTitle(&NSString::from_str("Later"));

    let response = alert.runModal();
    if response == NSAlertFirstButtonReturn {
        let url = download_url.to_string();
        // Show progress indicator then download + install in background
        std::thread::spawn(move || {
            match crate::updater::download_update(&url) {
                Ok(dmg_path) => {
                    crate::app::run_on_main(move || {
                        perform_update_install(&dmg_path);
                    });
                }
                Err(e) => {
                    crate::app::run_on_main(move || {
                        show_alert("Update Failed", &format!("Download failed: {}", e));
                    });
                }
            }
        });
    }
}

/// Perform the update: mount .dmg, copy .app, relaunch
fn perform_update_install(dmg_path: &std::path::Path) {
    let dmg_str = dmg_path.to_string_lossy().to_string();

    // Get our own bundle path (e.g. /Applications/SaveMyEyes.app)
    let bundle_path = match std::env::current_exe() {
        Ok(exe) => {
            // exe is like /Applications/SaveMyEyes.app/Contents/MacOS/savemyeyes
            // We need /Applications/SaveMyEyes.app
            let mut p = exe.clone();
            // Go up: MacOS -> Contents -> SaveMyEyes.app
            for _ in 0..3 {
                if let Some(parent) = p.parent() {
                    p = parent.to_path_buf();
                }
            }
            if p.extension().map(|e| e == "app").unwrap_or(false) {
                p
            } else {
                show_alert("Update Failed", "Could not determine app bundle path.");
                return;
            }
        }
        Err(_) => {
            show_alert("Update Failed", "Could not determine executable path.");
            return;
        }
    };

    let app_path = bundle_path.to_string_lossy().to_string();
    let pid = std::process::id();

    // Launch a detached shell script that:
    // 1. Waits for our process to exit
    // 2. Mounts the .dmg
    // 3. Copies the new .app over the old one
    // 4. Unmounts the .dmg
    // 5. Relaunches the new app
    let script = format!(
        r#"#!/bin/bash
# Wait for the current process to exit
while kill -0 {pid} 2>/dev/null; do sleep 0.5; done
sleep 1

# Mount the DMG
MOUNT_POINT=$(hdiutil attach "{dmg}" -nobrowse -noautoopen 2>/dev/null | grep "/Volumes" | awk '{{print $NF}}')
if [ -z "$MOUNT_POINT" ]; then
    osascript -e 'display notification "Failed to mount update DMG" with title "SaveMyEyes"'
    exit 1
fi

# Find the .app inside the mounted volume
SOURCE_APP=$(find "$MOUNT_POINT" -maxdepth 1 -name "*.app" -type d | head -1)
if [ -z "$SOURCE_APP" ]; then
    hdiutil detach "$MOUNT_POINT" -quiet
    osascript -e 'display notification "No app found in update DMG" with title "SaveMyEyes"'
    exit 1
fi

# Replace the current app
rm -rf "{app_path}"
cp -R "$SOURCE_APP" "{app_path}"

# Unmount
hdiutil detach "$MOUNT_POINT" -quiet

# Cleanup DMG
rm -f "{dmg}"

# Clear quarantine so macOS doesn't block the new binary
xattr -cr "{app_path}" 2>/dev/null

# Reset Accessibility TCC entry for our bundle ID so the new binary
# can re-prompt for permission instead of silently failing
tccutil reset Accessibility com.kdspl.savemyeyes 2>/dev/null

# Relaunch
open "{app_path}" --args --updated

exit 0
"#,
        pid = pid,
        dmg = dmg_str,
        app_path = app_path
    );

    // Write script to temp file
    let script_path = std::env::temp_dir().join("savemyeyes_update.sh");
    if std::fs::write(&script_path, &script).is_err() {
        show_alert("Update Failed", "Could not write update script.");
        return;
    }

    // Make executable and launch detached
    let _ = std::process::Command::new("chmod")
        .arg("+x")
        .arg(&script_path)
        .output();

    match std::process::Command::new("bash")
        .arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {
            // Quit our app so the script can replace us
            let mtm = MainThreadMarker::new().unwrap();
            let app = NSApplication::sharedApplication(mtm);
            app.terminate(None);
        }
        Err(e) => {
            show_alert(
                "Update Failed",
                &format!("Could not launch update script: {}", e),
            );
        }
    }
}
