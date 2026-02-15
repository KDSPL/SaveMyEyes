// HTTP-based update checker against GitHub releases (platform-agnostic)

use std::sync::atomic::{AtomicBool, Ordering};

static CHECKING: AtomicBool = AtomicBool::new(false);

/// Application version — single source of truth
pub const APP_VERSION: &str = "0.9.2";

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
/// `asset_suffix` should be ".exe" on Windows, ".dmg" on macOS, etc.
/// This runs synchronously — call from a background thread.
pub fn check_for_update(current_version: &str, asset_suffix: &str) -> UpdateResult {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return UpdateResult::Error("Already checking".into());
    }

    let result = do_check(current_version, asset_suffix);
    CHECKING.store(false, Ordering::SeqCst);
    result
}

fn do_check(current_version: &str, asset_suffix: &str) -> UpdateResult {
    let url = "https://api.github.com/repos/KDSPL/savemyeyes/releases/latest";

    let response = match ureq::get(url)
        .set("User-Agent", "SaveMyEyes-Updater")
        .set("Accept", "application/json")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("404")
                || err_str.contains("network")
                || err_str.contains("connect")
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

    let tag = extract_json_string(&body, "tag_name").unwrap_or_default();
    let html_url = extract_json_string(&body, "html_url")
        .unwrap_or_else(|| "https://github.com/KDSPL/savemyeyes/releases".into());

    let latest_version = tag.trim_start_matches('v');

    if latest_version.is_empty() {
        return UpdateResult::NoUpdate;
    }

    let download_url = extract_asset_download_url(&body, asset_suffix).unwrap_or_else(|| {
        format!(
            "https://github.com/KDSPL/savemyeyes/releases/download/{}/savemyeyes{}",
            tag, asset_suffix
        )
    });

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

/// Extract the first asset download URL matching the given suffix from a GitHub release JSON
pub fn extract_asset_download_url(json: &str, suffix: &str) -> Option<String> {
    let marker = "browser_download_url";
    let mut search_from = 0;
    while let Some(pos) = json[search_from..].find(marker) {
        let abs_pos = search_from + pos;
        let after_key = &json[abs_pos + marker.len()..];
        let after_colon = after_key.trim_start().strip_prefix('"')?;
        let colon_stripped = after_colon.trim_start().strip_prefix(':')?;
        let val_start = colon_stripped.trim_start().strip_prefix('"');

        if let Some(val) = val_start {
            if let Some(end) = val.find('"') {
                let url = &val[..end];
                if url.ends_with(suffix) {
                    return Some(url.to_string());
                }
            }
        }
        search_from = abs_pos + marker.len();
    }
    None
}

/// Extract a string value from JSON by key name (simple extraction, no full parser)
pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let search = format!("\"{}\"", key);
    let pos = json.find(&search)?;
    let after_key = &json[pos + search.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let end = after_quote.find('"')?;
    Some(after_quote[..end].to_string())
}

/// Returns true if `a` is newer than `b` (simple semver comparison)
pub fn version_newer(a: &str, b: &str) -> bool {
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

/// Download a file from a URL to a temp path. Returns the path on success.
pub fn download_to_temp(download_url: &str, filename: &str) -> Result<std::path::PathBuf, String> {
    let response = ureq::get(download_url)
        .set("User-Agent", "SaveMyEyes-Updater")
        .call()
        .map_err(|e| format!("Download failed: {}", e))?;

    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(filename);

    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    std::io::copy(&mut reader, &mut file)
        .map_err(|e| format!("Failed to write update: {}", e))?;

    Ok(temp_path)
}

/// Check if the app was just updated (launched with --updated flag)
pub fn was_just_updated() -> bool {
    std::env::args().any(|a| a == "--updated")
}

/// Open a URL in the default browser (cross-platform)
pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        // On Windows, the windows/ crate provides its own open_url via ShellExecuteW.
        // This fallback uses cmd as a last resort.
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}
