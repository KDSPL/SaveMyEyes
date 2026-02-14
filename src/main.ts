import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

interface AppConfig {
  opacity: number;
  is_enabled: boolean;
  launch_on_login: boolean;
  allow_capture: boolean;
  last_opacity: number;
  hotkey_toggle: string;
  hotkey_increase: string;
  hotkey_decrease: string;
  auto_update: boolean;
}

let opacitySlider: HTMLInputElement | null;
let opacityBadge: HTMLElement | null;
let sliderFill: HTMLElement | null;
let enabledToggle: HTMLInputElement | null;
let autostartToggle: HTMLInputElement | null;
let captureToggle: HTMLInputElement | null;
let autoUpdateToggle: HTMLInputElement | null;
let updateStatus: HTMLElement | null;
let checkUpdateBtn: HTMLElement | null;
let toast: HTMLElement | null;
let statusMsg: HTMLElement | null;

function updateSliderFill(value: number) {
  if (sliderFill) {
    const percentage = (value / 90) * 100;
    sliderFill.style.width = `${percentage}%`;
  }
}

function setupTabs() {
  const tabs = document.querySelectorAll<HTMLButtonElement>('.tab');
  const contents = document.querySelectorAll<HTMLElement>('.tab-content');

  tabs.forEach(tab => {
    tab.addEventListener('click', () => {
      const target = tab.dataset.tab;
      
      // Update active tab
      tabs.forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      
      // Update active content
      contents.forEach(content => {
        content.classList.remove('active');
        if (content.id === `tab-${target}`) {
          content.classList.add('active');
        }
      });
    });
  });
}

async function loadConfig() {
  try {
    const config: AppConfig = await invoke("get_config");
    
    if (opacitySlider) {
      const value = Math.round(config.opacity * 100);
      opacitySlider.value = String(value);
      updateSliderFill(value);
    }
    if (opacityBadge) {
      opacityBadge.textContent = `${Math.round(config.opacity * 100)}%`;
    }
    if (enabledToggle) {
      enabledToggle.checked = config.is_enabled;
    }
    if (autostartToggle) {
      autostartToggle.checked = config.launch_on_login;
    }
    if (captureToggle) {
      captureToggle.checked = config.allow_capture;
    }
    if (autoUpdateToggle) {
      autoUpdateToggle.checked = config.auto_update;
    }
    
    // Update hotkey displays
    const toggleKey = document.getElementById("hotkey-toggle");
    const increaseKey = document.getElementById("hotkey-increase");
    const decreaseKey = document.getElementById("hotkey-decrease");
    
    if (toggleKey) toggleKey.textContent = config.hotkey_toggle;
    if (increaseKey) increaseKey.textContent = config.hotkey_increase;
    if (decreaseKey) decreaseKey.textContent = config.hotkey_decrease;
  } catch (err) {
    console.error("Failed to load config:", err);
  }
}

async function setOpacity(value: number) {
  try {
    await invoke("set_opacity", { opacity: value / 100 });
    showStatus("Opacity updated");
  } catch (err) {
    console.error("Failed to set opacity:", err);
  }
}

async function toggleDimmer() {
  try {
    const isEnabled: boolean = await invoke("toggle_dimmer");
    showStatus(isEnabled ? "Dimmer enabled" : "Dimmer disabled");
  } catch (err) {
    console.error("Failed to toggle dimmer:", err);
  }
}

async function setAllowCapture(allow: boolean) {
  try {
    await invoke("set_allow_capture", { allow });
    showStatus(allow ? "Screen capture allowed" : "Hidden from capture");
  } catch (err) {
    console.error("Failed to set capture mode:", err);
  }
}

async function setAutoUpdate(enabled: boolean) {
  try {
    await invoke("set_auto_update", { enabled });
    showStatus(enabled ? "Auto-update enabled" : "Auto-update disabled");
  } catch (err) {
    console.error("Failed to set auto-update:", err);
  }
}

async function checkForUpdate() {
  if (updateStatus) updateStatus.textContent = "Checking...";
  if (checkUpdateBtn) checkUpdateBtn.classList.add("disabled");
  try {
    const result: string = await invoke("check_for_update");
    if (result === "no_update") {
      if (updateStatus) updateStatus.textContent = "You're on the latest version!";
      showStatus("No updates available");
    } else {
      if (updateStatus) updateStatus.textContent = "Update installed! Restart to apply.";
      showStatus("Update downloaded!");
    }
  } catch (err: any) {
    console.error("Update check failed:", err);
    if (updateStatus) updateStatus.textContent = "Update check failed";
    showStatus("Update check failed");
  }
  setTimeout(() => {
    if (checkUpdateBtn) checkUpdateBtn.classList.remove("disabled");
    if (updateStatus) updateStatus.textContent = "";
  }, 5000);
}

function showStatus(message: string) {
  if (toast && statusMsg) {
    statusMsg.textContent = message;
    toast.classList.add("show");
    setTimeout(() => {
      toast?.classList.remove("show");
    }, 2000);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  opacitySlider = document.querySelector("#opacity-slider");
  opacityBadge = document.querySelector("#opacity-badge");
  sliderFill = document.querySelector("#slider-fill");
  enabledToggle = document.querySelector("#enabled-toggle");
  autostartToggle = document.querySelector("#autostart-toggle");
  captureToggle = document.querySelector("#capture-toggle");
  autoUpdateToggle = document.querySelector("#auto-update-toggle");
  updateStatus = document.querySelector("#update-status");
  checkUpdateBtn = document.querySelector("#check-update-btn");
  toast = document.querySelector("#toast");
  statusMsg = document.querySelector("#status-msg");

  // Credit link - open KraftPixel website
  const creditLink = document.querySelector("#credit-link");
  creditLink?.addEventListener("click", (e) => {
    e.preventDefault();
    openUrl("https://kraftpixel.com");
  });

  // Setup tabs
  setupTabs();

  // Load initial config
  loadConfig();

  // Listen for config changes from shortcuts
  listen<AppConfig>("config-changed", (event) => {
    const config = event.payload;
    if (opacitySlider) {
      const value = Math.round(config.opacity * 100);
      opacitySlider.value = String(value);
      updateSliderFill(value);
    }
    if (opacityBadge) {
      opacityBadge.textContent = `${Math.round(config.opacity * 100)}%`;
    }
    if (enabledToggle) {
      enabledToggle.checked = config.is_enabled;
    }
    showStatus(config.is_enabled ? `Dimmer: ${Math.round(config.opacity * 100)}%` : "Dimmer OFF");
  });

  // Opacity slider events
  opacitySlider?.addEventListener("input", () => {
    if (opacitySlider && opacityBadge) {
      const value = parseInt(opacitySlider.value, 10);
      opacityBadge.textContent = `${value}%`;
      updateSliderFill(value);
    }
  });

  opacitySlider?.addEventListener("change", async () => {
    if (opacitySlider) {
      const value = parseInt(opacitySlider.value, 10);
      await setOpacity(value);
      
      // Auto-enable dimmer when user adjusts slider (if not already enabled)
      if (enabledToggle && !enabledToggle.checked && value > 0) {
        enabledToggle.checked = true;
        await toggleDimmer();
      }
    }
  });

  // Enabled toggle
  enabledToggle?.addEventListener("change", () => {
    toggleDimmer();
  });

  // Capture toggle
  captureToggle?.addEventListener("change", () => {
    if (captureToggle) {
      setAllowCapture(captureToggle.checked);
    }
  });

  // Autostart toggle (placeholder - will be implemented)
  autostartToggle?.addEventListener("change", () => {
    showStatus("Autostart setting saved");
  });

  // Auto-update toggle
  autoUpdateToggle?.addEventListener("change", () => {
    if (autoUpdateToggle) {
      setAutoUpdate(autoUpdateToggle.checked);
    }
  });

  // Check for update button
  checkUpdateBtn?.addEventListener("click", () => {
    checkForUpdate();
  });

  // Auto-check for updates on startup (silent)
  setTimeout(async () => {
    try {
      const config: AppConfig = await invoke("get_config");
      if (config.auto_update) {
        const result: string = await invoke("check_for_update");
        if (result !== "no_update") {
          showStatus("Update downloaded! Restart to apply.");
        }
      }
    } catch (_) {
      // Silent fail on startup check
    }
  }, 5000);
});

