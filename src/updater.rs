// HTTP-based update checker against GitHub releases

use std::sync::atomic::{AtomicBool, Ordering};

static CHECKING: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub enum UpdateResult {
    NoUpdate,
    UpdateAvailable {
        _version: String,
        url: String,
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

    // Simple version comparison (works for semver x.y.z)
    if version_newer(latest_version, current_version) {
        UpdateResult::UpdateAvailable {
            _version: latest_version.to_string(),
            url: html_url,
        }
    } else {
        UpdateResult::NoUpdate
    }
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
