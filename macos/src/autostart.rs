// macOS autostart using Login Items (launchd plist or SMAppService).
//
// Strategy: Write a LaunchAgent plist to ~/Library/LaunchAgents/
// that starts the app on login.

use std::fs;
use std::path::PathBuf;

const PLIST_NAME: &str = "com.kraftpixel.SaveMyEyes.plist";

fn plist_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    home.join("Library")
        .join("LaunchAgents")
        .join(PLIST_NAME)
}

/// Enable autostart by writing a LaunchAgent plist.
pub fn enable() -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return false,
    };

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.kraftpixel.SaveMyEyes</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
        exe
    );

    let path = plist_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, plist_content).is_ok()
}

/// Disable autostart by removing the plist.
pub fn disable() -> bool {
    let path = plist_path();
    if path.exists() {
        fs::remove_file(&path).is_ok()
    } else {
        true
    }
}

/// Check if autostart is currently enabled.
#[allow(dead_code)]
pub fn is_enabled() -> bool {
    plist_path().exists()
}
