// Windows low-level keyboard hook to intercept PrintScreen
// This allows us to hide the overlay before the screenshot is taken

#![cfg(target_os = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

const VK_SNAPSHOT: u32 = 0x2C; // PrintScreen key

// Wrapper for HHOOK that can be sent across threads
// SAFETY: The hook handle is only used from the hook callback which runs on the same thread
struct SendableHook(isize);
unsafe impl Send for SendableHook {}
unsafe impl Sync for SendableHook {}

// Store the hook handle as an isize (the raw pointer value)
static HOOK_HANDLE: Mutex<Option<SendableHook>> = Mutex::new(None);
static PROCESSING_SCREENSHOT: AtomicBool = AtomicBool::new(false);

// Import overlay functions
use crate::overlay::windows as overlay_impl;

// Constants for KBDLLHOOKSTRUCT flags
const LLKHF_INJECTED: u32 = 0x00000010;

/// Low-level keyboard hook callback
/// IMPORTANT: This must return quickly - no blocking operations!
unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let kb_struct = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        
        // Check if it's PrintScreen key being pressed (not released)
        if kb_struct.vkCode == VK_SNAPSHOT 
            && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN)
        {
            // Skip injected keys
            let is_injected = (kb_struct.flags.0 & LLKHF_INJECTED) != 0;
            if !is_injected {
                // Avoid re-entry if we're already processing
                if !PROCESSING_SCREENSHOT.swap(true, Ordering::SeqCst) {
                    let is_visible = overlay_impl::is_visible();
                    let current_opacity = overlay_impl::get_opacity();
                    
                    if is_visible && current_opacity > 0.0 {
                        println!("[PrintScreen] Detected! Scheduling hide/restore...");
                        
                        // Spawn thread to handle hide and restore
                        // Do NOT block the hook - let the key pass through immediately
                        std::thread::spawn(move || {
                            // Hide overlay immediately (in this thread, not blocking hook)
                            overlay_impl::hide_overlay_temp();
                            println!("[PrintScreen] Overlay hidden");
                            
                            // Wait for screenshot to be captured
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            
                            // Restore overlay
                            overlay_impl::restore_overlay_temp();
                            println!("[PrintScreen] Overlay restored");
                            
                            PROCESSING_SCREENSHOT.store(false, Ordering::SeqCst);
                        });
                    } else {
                        PROCESSING_SCREENSHOT.store(false, Ordering::SeqCst);
                    }
                }
            }
        }
    }
    
    // ALWAYS pass to next hook immediately - never block!
    let hook_val = HOOK_HANDLE.lock().unwrap().as_ref().map(|h| h.0).unwrap_or(0);
    CallNextHookEx(Some(HHOOK(hook_val as *mut std::ffi::c_void)), code, wparam, lparam)
}

/// Install the low-level keyboard hook
pub fn install_hook() -> bool {
    unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0);
        
        match hook {
            Ok(h) => {
                println!("[Keyboard Hook] Installed successfully");
                *HOOK_HANDLE.lock().unwrap() = Some(SendableHook(h.0 as isize));
                true
            }
            Err(e) => {
                println!("[Keyboard Hook] Failed to install: {:?}", e);
                false
            }
        }
    }
}

/// Uninstall the keyboard hook
pub fn uninstall_hook() {
    let hook = HOOK_HANDLE.lock().unwrap().take();
    if let Some(h) = hook {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(h.0 as *mut std::ffi::c_void));
            println!("[Keyboard Hook] Uninstalled");
        }
    }
}
