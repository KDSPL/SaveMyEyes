// HTTP-based update checker and auto-updater against GitHub releases

use std::sync::atomic::{AtomicBool, Ordering};

static CHECKING: AtomicBool = AtomicBool::new(false);

/// Application version — single source of truth
pub const APP_VERSION: &str = "0.9.0";

#[derive(Debug)]
pub enum UpdateResult {
    NoUpdate,
    UpdateAvailable {
        version: String,
        #[allow(dead_code)]
        url: String,
        download_url: String,
    },
    #[allow(dead_code)]
    Error(String),
}

/// Check for updates by fetching the latest release info from GitHub.
/// This runs synchronously — call from a background thread.
pub fn check_for_update(current_version: &str) -> UpdateResult {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return UpdateResult::Error("Already checking".into());
    }

    let result = do_check(current_version);
    CHECKING.store(false, Ordering::SeqCst);
    result
}

fn do_check(current_version: &str) -> UpdateResult {
    let url = "https://api.github.com/repos/KDSPL/savemyeyes/releases/latest";

    let response = match ureq::get(url)
        .set("User-Agent", "SaveMyEyes-Updater")
        .set("Accept", "application/json")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            let err_str = e.to_string();
            // Treat 404 / network errors as "no update"
            if err_str.contains("404") || err_str.contains("network") || err_str.contains("connect")
            {
                return UpdateResult::NoUpdate;
            }
            return UpdateResult::Error(format!("Request failed: {}", err_str));
        }
    };

    let body = match response.into_string() {
        Ok(b) => b,
        Err(e) => return UpdateResult::Error(format!("Failed to read response: {}", e)),
    };

    // Parse json manually to avoid pulling in full json parsing for this
    let tag = extract_json_string(&body, "tag_name").unwrap_or_default();
    let html_url = extract_json_string(&body, "html_url")
        .unwrap_or_else(|| "https://github.com/KDSPL/savemyeyes/releases".into());

    // Strip leading 'v' from tag
    let latest_version = tag.trim_start_matches('v');

    if latest_version.is_empty() {
        return UpdateResult::NoUpdate;
    }

    // Find the .exe download URL from assets
    let download_url = extract_exe_download_url(&body)
        .unwrap_or_else(|| format!(
            "https://github.com/KDSPL/savemyeyes/releases/download/{}/savemyeyes.exe",
            tag
        ));

    // Simple version comparison (works for semver x.y.z)
    if version_newer(latest_version, current_version) {
        UpdateResult::UpdateAvailable {
            version: latest_version.to_string(),
            url: html_url,
            download_url,
        }
    } else {
        UpdateResult::NoUpdate
    }
}

/// Extract the first .exe asset download URL from a GitHub release JSON
fn extract_exe_download_url(json: &str) -> Option<String> {
    // Look for browser_download_url that ends with .exe
    let marker = "browser_download_url";
    let mut search_from = 0;
    while let Some(pos) = json[search_from..].find(marker) {
        let abs_pos = search_from + pos;
        let after_key = &json[abs_pos + marker.len()..];
        // Skip ": "
        let after_colon = after_key.trim_start().strip_prefix('"')?;
        let colon_stripped = after_colon.trim_start().strip_prefix(':')?;
        let val_start = colon_stripped.trim_start().strip_prefix('"');

        if let Some(val) = val_start {
            if let Some(end) = val.find('"') {
                let url = &val[..end];
                if url.ends_with(".exe") {
                    return Some(url.to_string());
                }
            }
        }
        search_from = abs_pos + marker.len();
    }
    None
}

/// Extract a string value from JSON by key name (simple extraction, no full parser)
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let search = format!("\"{}\"", key);
    let pos = json.find(&search)?;
    let after_key = &json[pos + search.len()..];
    // Skip whitespace and colon
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    // Expect opening quote
    let after_quote = after_colon.strip_prefix('"')?;
    // Find closing quote
    let end = after_quote.find('"')?;
    Some(after_quote[..end].to_string())
}

/// Returns true if `a` is newer than `b` (simple semver comparison)
fn version_newer(a: &str, b: &str) -> bool {
    let parse =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse::<u32>().ok()).collect() };
    let va = parse(a);
    let vb = parse(b);
    for i in 0..va.len().max(vb.len()) {
        let ca = va.get(i).copied().unwrap_or(0);
        let cb = vb.get(i).copied().unwrap_or(0);
        if ca > cb {
            return true;
        }
        if ca < cb {
            return false;
        }
    }
    false
}

/// Open a URL in the default browser
pub fn open_url(url: &str) {
    let url_wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(url_wide.as_ptr()),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

/// Download the update .exe to a temp file.
/// Returns the path to the downloaded file on success.
pub fn download_update(download_url: &str) -> Result<std::path::PathBuf, String> {
    let response = ureq::get(download_url)
        .set("User-Agent", "SaveMyEyes-Updater")
        .call()
        .map_err(|e| format!("Download failed: {}", e))?;

    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join("savemyeyes_update.exe");

    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    std::io::copy(&mut reader, &mut file)
        .map_err(|e| format!("Failed to write update: {}", e))?;

    Ok(temp_path)
}

/// Replace the current exe with the downloaded update and relaunch.
/// Strategy:
///   1. Rename current exe to .old
///   2. Copy new exe to current path
///   3. Launch new exe with --updated flag
///   4. Exit current process
pub fn apply_update_and_relaunch(downloaded_path: &std::path::Path) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Cannot determine current exe path: {}", e))?;

    let old_path = current_exe.with_extension("exe.old");

    // Remove previous .old if it exists
    let _ = std::fs::remove_file(&old_path);

    // Rename current → .old
    std::fs::rename(&current_exe, &old_path)
        .map_err(|e| format!("Failed to rename current exe: {}", e))?;

    // Copy downloaded → current path
    std::fs::copy(downloaded_path, &current_exe)
        .map_err(|e| {
            // Try to restore old exe
            let _ = std::fs::rename(&old_path, &current_exe);
            format!("Failed to copy new exe: {}", e)
        })?;

    // Clean up temp file
    let _ = std::fs::remove_file(downloaded_path);

    // Launch the new exe with --updated flag
    let _ = std::process::Command::new(&current_exe)
        .arg("--updated")
        .spawn();

    // Exit current process
    std::process::exit(0);
}

/// Check if the app was just updated (launched with --updated flag)
pub fn was_just_updated() -> bool {
    std::env::args().any(|a| a == "--updated")
}

/// Clean up the .old file from a previous update (best-effort)
pub fn cleanup_old_exe() {
    if let Ok(current) = std::env::current_exe() {
        let old_path = current.with_extension("exe.old");
        let _ = std::fs::remove_file(old_path);
    }
}

/// Show a Win32 Yes/No message box asking the user to update.
/// Returns true if user clicked Yes.
pub fn prompt_update_dialog(new_version: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONINFORMATION, MB_YESNO};

    let message = format!(
        "A new version (v{}) of SaveMyEyes is available!\n\nWould you like to download and install it now?\n\nThe app will restart automatically after updating.",
        new_version
    );
    let msg_wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let title_wide: Vec<u16> = "SaveMyEyes Update".encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let result = MessageBoxW(
            None,
            PCWSTR(msg_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            MB_YESNO | MB_ICONINFORMATION,
        );
        result == IDYES
    }
}
