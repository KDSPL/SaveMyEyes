// GDI owner-draw rendering — replicates the shadcn dark UI

use super::controls::*;
use super::theme::*;
use crate::updater;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn create_font(size: i32, weight: i32, family: &str) -> HFONT {
    let face: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut lf = LOGFONTW {
            lfHeight: size,
            lfWeight: weight,
            lfQuality: CLEARTYPE_QUALITY,
            lfCharSet: DEFAULT_CHARSET,
            ..Default::default()
        };
        let len = face.len().min(32);
        lf.lfFaceName[..len].copy_from_slice(&face[..len]);
        CreateFontIndirectW(&lf)
    }
}

fn fill_rect_color(hdc: HDC, r: &RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(hdc, r, brush);
        let _ = DeleteObject(HGDIOBJ::from(brush));
    }
}

fn draw_rounded_rect(hdc: HDC, r: &RECT, radius: i32, fill: COLORREF, border: COLORREF) {
    unsafe {
        let fill_brush = CreateSolidBrush(fill);
        let border_pen = CreatePen(PS_SOLID, 1, border);
        let old_brush = SelectObject(hdc, HGDIOBJ::from(fill_brush));
        let old_pen = SelectObject(hdc, HGDIOBJ::from(border_pen));
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(HGDIOBJ::from(fill_brush));
        let _ = DeleteObject(HGDIOBJ::from(border_pen));
    }
}

fn draw_text_simple(hdc: HDC, text: &str, x: i32, y: i32, color: COLORREF, font: HFONT) {
    unsafe {
        let old_font = SelectObject(hdc, HGDIOBJ::from(font));
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let _ = TextOutW(hdc, x, y, &wide);
        SelectObject(hdc, old_font);
    }
}

fn measure_text(hdc: HDC, text: &str, font: HFONT) -> (i32, i32) {
    unsafe {
        let old_font = SelectObject(hdc, HGDIOBJ::from(font));
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut size = windows::Win32::Foundation::SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
        SelectObject(hdc, old_font);
        (size.cx, size.cy)
    }
}

fn draw_text_right(hdc: HDC, text: &str, right_x: i32, y: i32, color: COLORREF, font: HFONT) {
    let (w, _) = measure_text(hdc, text, font);
    draw_text_simple(hdc, text, right_x - w, y, color, font);
}

fn draw_circle(hdc: HDC, cx: i32, cy: i32, r: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 0, color);
        let old_brush = SelectObject(hdc, HGDIOBJ::from(brush));
        let old_pen = SelectObject(hdc, HGDIOBJ::from(pen));
        let _ = Ellipse(hdc, cx - r, cy - r, cx + r, cy + r);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(HGDIOBJ::from(brush));
        let _ = DeleteObject(HGDIOBJ::from(pen));
    }
}

// ── Main paint function ─────────────────────────────────────────────────────

pub fn paint(hdc: HDC, client: &RECT, state: &mut UiState) {
    fill_rect_color(hdc, client, CLR_BACKGROUND);

    let fonts = Fonts::create();
    let mut y = PADDING;

    y = draw_header(hdc, y, state, &fonts);
    y += GAP;

    y = draw_tab_bar(hdc, y, state, &fonts);
    y += GAP;

    match state.active_tab {
        Tab::Dimmer => draw_dimmer_tab(hdc, y, state, &fonts),
        Tab::Settings => draw_settings_tab(hdc, y, state, &fonts),
        Tab::Shortcuts => draw_shortcuts_tab(hdc, y, state, &fonts),
    };

    if state.toast_visible {
        draw_toast(hdc, client, state, &fonts);
    }

    fonts.destroy();
}

// ── Font cache ──────────────────────────────────────────────────────────────

struct Fonts {
    title: HFONT,
    small: HFONT,
    small_bold: HFONT,
    xs: HFONT,
    xxs: HFONT,
    mono: HFONT,
}

impl Fonts {
    fn create() -> Self {
        Self {
            title: create_font(FONT_SIZE_TITLE, 600, FONT_NAME),
            small: create_font(FONT_SIZE_SMALL, 400, FONT_NAME),
            small_bold: create_font(FONT_SIZE_SMALL, 500, FONT_NAME),
            xs: create_font(FONT_SIZE_XS, 400, FONT_NAME),
            xxs: create_font(FONT_SIZE_XXS, 400, FONT_NAME),
            mono: create_font(FONT_SIZE_XXS, 500, FONT_MONO_NAME),
        }
    }

    fn destroy(&self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ::from(self.title));
            let _ = DeleteObject(HGDIOBJ::from(self.small));
            let _ = DeleteObject(HGDIOBJ::from(self.small_bold));
            let _ = DeleteObject(HGDIOBJ::from(self.xs));
            let _ = DeleteObject(HGDIOBJ::from(self.xxs));
            let _ = DeleteObject(HGDIOBJ::from(self.mono));
        }
    }
}

// ── Section renderers ───────────────────────────────────────────────────────

fn draw_header(hdc: HDC, y: i32, state: &mut UiState, fonts: &Fonts) -> i32 {
    let x = PADDING;
    let right = PADDING + CONTENT_WIDTH;

    let icon_size = 40;
    let icon_x = x;
    let icon_cy = y + icon_size / 2;
    unsafe {
        let pen = CreatePen(PS_SOLID, 2, CLR_BRAND);
        let null_brush = GetStockObject(NULL_BRUSH);
        let old_pen = SelectObject(hdc, HGDIOBJ::from(pen));
        let old_brush = SelectObject(hdc, null_brush);

        // Outer eye shape
        let _ = Ellipse(
            hdc,
            icon_x + 2,
            y + 6,
            icon_x + icon_size - 2,
            y + icon_size - 6,
        );
        // Inner pupil ring
        let pupil_r = 7;
        let _ = Ellipse(
            hdc,
            icon_x + icon_size / 2 - pupil_r,
            icon_cy - pupil_r,
            icon_x + icon_size / 2 + pupil_r,
            icon_cy + pupil_r,
        );
        // Fill pupil center
        let brand_brush = CreateSolidBrush(CLR_BRAND);
        SelectObject(hdc, HGDIOBJ::from(brand_brush));
        let pupil_r2 = 4;
        let _ = Ellipse(
            hdc,
            icon_x + icon_size / 2 - pupil_r2,
            icon_cy - pupil_r2,
            icon_x + icon_size / 2 + pupil_r2,
            icon_cy + pupil_r2,
        );

        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ::from(pen));
        let _ = DeleteObject(HGDIOBJ::from(brand_brush));
    }

    let text_x = icon_x + icon_size + 12;
    draw_text_simple(
        hdc,
        "SaveMyEyes",
        text_x,
        y + 2,
        CLR_FOREGROUND,
        fonts.title,
    );
    draw_text_simple(hdc, "Screen Dimmer", text_x, y + 22, CLR_MUTED_FG, fonts.xs);

    let credit_by = "An open-source project by";
    let credit_name = "KraftPixel";
    let (_, h1) = measure_text(hdc, credit_by, fonts.xxs);
    let (w2, _) = measure_text(hdc, credit_name, fonts.xs);
    draw_text_right(hdc, credit_by, right, y + 4, CLR_MUTED_FG, fonts.xxs);
    draw_text_right(hdc, credit_name, right, y + 4 + h1 + 1, CLR_BRAND, fonts.xs);

    state.credit_rect = RECT {
        left: right - w2.max(120),
        top: y,
        right,
        bottom: y + icon_size,
    };

    let header_bottom = y + icon_size + 8;

    unsafe {
        let pen = CreatePen(PS_SOLID, 1, CLR_BORDER);
        let old_pen = SelectObject(hdc, HGDIOBJ::from(pen));
        let _ = MoveToEx(hdc, PADDING, header_bottom, None);
        let _ = LineTo(hdc, right, header_bottom);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ::from(pen));
    }

    header_bottom + 8
}

fn draw_tab_bar(hdc: HDC, y: i32, state: &mut UiState, fonts: &Fonts) -> i32 {
    let x = PADDING;
    let tab_names = ["Dimmer", "Settings", "Shortcuts"];
    let bar_rect = RECT {
        left: x,
        top: y,
        right: x + CONTENT_WIDTH,
        bottom: y + TAB_HEIGHT + 8,
    };

    draw_rounded_rect(hdc, &bar_rect, CARD_RADIUS, CLR_SECONDARY, CLR_SECONDARY);
    state.tab_bar_rect = bar_rect;

    let tab_width = CONTENT_WIDTH / 3;
    let tab_pad = 4;

    for (i, name) in tab_names.iter().enumerate() {
        let tx = x + tab_pad + (i as i32) * tab_width;
        let tab_rect = RECT {
            left: tx,
            top: y + tab_pad,
            right: tx + tab_width - tab_pad,
            bottom: y + TAB_HEIGHT + tab_pad,
        };

        let is_active = state.active_tab as usize == i;
        if is_active {
            draw_rounded_rect(
                hdc,
                &tab_rect,
                CARD_RADIUS - 2,
                CLR_BACKGROUND,
                CLR_BACKGROUND,
            );
        }

        let text_color = if is_active {
            CLR_FOREGROUND
        } else {
            CLR_MUTED_FG
        };
        let (tw, th) = measure_text(hdc, name, fonts.small_bold);
        let text_x = tab_rect.left + (tab_rect.right - tab_rect.left - tw) / 2;
        let text_y = tab_rect.top + (tab_rect.bottom - tab_rect.top - th) / 2;
        draw_text_simple(hdc, name, text_x, text_y, text_color, fonts.small_bold);

        state.tab_rects[i] = tab_rect;
    }

    bar_rect.bottom
}

fn draw_dimmer_tab(hdc: HDC, y: i32, state: &mut UiState, fonts: &Fonts) {
    let x = PADDING;
    let inner_x = x + 16;
    let inner_right = x + CONTENT_WIDTH - 16;

    // Card 1: Dimming Level
    let card1_top = y;
    let card1 = RECT {
        left: x,
        top: card1_top,
        right: x + CONTENT_WIDTH,
        bottom: card1_top + 100,
    };
    draw_rounded_rect(hdc, &card1, CARD_RADIUS, CLR_BACKGROUND, CLR_BORDER);

    draw_text_simple(
        hdc,
        "Dimming Level",
        inner_x,
        card1_top + 14,
        CLR_FOREGROUND,
        fonts.small_bold,
    );

    // Badge
    let badge_text = format!("{}%", state.slider.value);
    let (bw, bh) = measure_text(hdc, &badge_text, fonts.xs);
    let badge_w = bw + 20;
    let badge_h = bh + 4;
    let badge_x = inner_right - badge_w;
    let badge_y = card1_top + 12;
    let badge_rect = RECT {
        left: badge_x,
        top: badge_y,
        right: badge_x + badge_w,
        bottom: badge_y + badge_h,
    };
    draw_rounded_rect(hdc, &badge_rect, badge_h / 2, CLR_BRAND, CLR_BRAND);
    draw_text_simple(
        hdc,
        &badge_text,
        badge_x + (badge_w - bw) / 2,
        badge_y + (badge_h - bh) / 2,
        CLR_FOREGROUND,
        fonts.xs,
    );

    // Slider
    let slider_y = card1_top + 48;
    let track_h = 8;
    let thumb_r = 9;

    state.slider.rect = RECT {
        left: inner_x,
        top: slider_y,
        right: inner_right,
        bottom: slider_y + track_h,
    };

    let track_rect = state.slider.rect;
    draw_rounded_rect(hdc, &track_rect, 4, CLR_SECONDARY, CLR_SECONDARY);

    let fill_w = ((state.slider.value as f32 / 90.0) * (inner_right - inner_x) as f32) as i32;
    if fill_w > 0 {
        let fill_rect = RECT {
            left: inner_x,
            top: slider_y,
            right: inner_x + fill_w,
            bottom: slider_y + track_h,
        };
        draw_rounded_rect(hdc, &fill_rect, 4, CLR_BRAND, CLR_BRAND);
    }

    let thumb_x = state.slider.thumb_x();
    let thumb_cy = slider_y + track_h / 2;
    draw_circle(hdc, thumb_x, thumb_cy, thumb_r, CLR_FOREGROUND);

    state.slider.thumb_rect = RECT {
        left: inner_x - thumb_r,
        top: slider_y - thumb_r - 4,
        right: inner_right + thumb_r,
        bottom: slider_y + track_h + thumb_r + 4,
    };

    draw_text_simple(
        hdc,
        "0%",
        inner_x,
        slider_y + track_h + 6,
        CLR_MUTED_FG,
        fonts.xxs,
    );
    draw_text_right(
        hdc,
        "90%",
        inner_right,
        slider_y + track_h + 6,
        CLR_MUTED_FG,
        fonts.xxs,
    );

    // Card 2: Dimmer Enabled
    let card2_top = card1.bottom + GAP;
    let card2 = RECT {
        left: x,
        top: card2_top,
        right: x + CONTENT_WIDTH,
        bottom: card2_top + 56,
    };
    draw_rounded_rect(hdc, &card2, CARD_RADIUS, CLR_BACKGROUND, CLR_BORDER);

    draw_text_simple(
        hdc,
        "Dimmer Enabled",
        inner_x,
        card2_top + 10,
        CLR_FOREGROUND,
        fonts.small_bold,
    );
    draw_text_simple(
        hdc,
        "Apply dimming overlay to screen",
        inner_x,
        card2_top + 28,
        CLR_MUTED_FG,
        fonts.xs,
    );

    let toggle_x = inner_right - 44;
    state.enabled_toggle.rect =
        draw_toggle(hdc, toggle_x, card2_top + 16, state.enabled_toggle.checked);
}

fn draw_settings_tab(hdc: HDC, y: i32, state: &mut UiState, fonts: &Fonts) {
    let x = PADDING;
    let inner_x = x + 16;
    let inner_right = x + CONTENT_WIDTH - 16;
    let toggle_x = inner_right - 44;

    // Card 1: General
    let card1_top = y;
    let card1 = RECT {
        left: x,
        top: card1_top,
        right: x + CONTENT_WIDTH,
        bottom: card1_top + 80,
    };
    draw_rounded_rect(hdc, &card1, CARD_RADIUS, CLR_BACKGROUND, CLR_BORDER);

    draw_text_simple(
        hdc,
        "General",
        inner_x,
        card1_top + 12,
        CLR_FOREGROUND,
        fonts.small_bold,
    );
    draw_text_simple(
        hdc,
        "Start on Login",
        inner_x,
        card1_top + 36,
        CLR_FOREGROUND,
        fonts.small_bold,
    );
    draw_text_simple(
        hdc,
        "Launch automatically at startup",
        inner_x,
        card1_top + 52,
        CLR_MUTED_FG,
        fonts.xs,
    );
    state.autostart_toggle.rect = draw_toggle(
        hdc,
        toggle_x,
        card1_top + 40,
        state.autostart_toggle.checked,
    );

    // Card 2: Updates
    let card2_top = card1.bottom + GAP;
    let card2 = RECT {
        left: x,
        top: card2_top,
        right: x + CONTENT_WIDTH,
        bottom: card2_top + 130,
    };
    draw_rounded_rect(hdc, &card2, CARD_RADIUS, CLR_BACKGROUND, CLR_BORDER);

    draw_text_simple(
        hdc,
        "Updates",
        inner_x,
        card2_top + 12,
        CLR_FOREGROUND,
        fonts.small_bold,
    );

    // Show current version in tiny text next to "Updates" heading
    let version_text = format!("v{}", updater::APP_VERSION);
    draw_text_simple(
        hdc,
        &version_text,
        inner_x + 60,
        card2_top + 15,
        CLR_MUTED_FG,
        fonts.xxs,
    );
    draw_text_simple(
        hdc,
        "Auto-Update",
        inner_x,
        card2_top + 38,
        CLR_FOREGROUND,
        fonts.small_bold,
    );
    draw_text_simple(
        hdc,
        "Automatically download and install updates",
        inner_x,
        card2_top + 54,
        CLR_MUTED_FG,
        fonts.xs,
    );
    state.auto_update_toggle.rect = draw_toggle(
        hdc,
        toggle_x,
        card2_top + 42,
        state.auto_update_toggle.checked,
    );

    // Divider
    let div_y = card2_top + 74;
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, CLR_BORDER);
        let old = SelectObject(hdc, HGDIOBJ::from(pen));
        let _ = MoveToEx(hdc, inner_x, div_y, None);
        let _ = LineTo(hdc, inner_right, div_y);
        SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ::from(pen));
    }

    draw_text_simple(
        hdc,
        "Check for Updates",
        inner_x,
        div_y + 12,
        CLR_FOREGROUND,
        fonts.small_bold,
    );

    if !state.update_status_text.is_empty() {
        let s = state.update_status_text.clone();
        draw_text_simple(hdc, &s, inner_x, div_y + 28, CLR_BRAND, fonts.xs);
    }

    // Button
    let btn_text = state.check_update_btn.text.clone();
    let (bw, bh) = measure_text(hdc, &btn_text, fonts.xs);
    let btn_w = bw + 28;
    let btn_h = bh + 12;
    let btn_x = inner_right - btn_w;
    let btn_y = div_y + 10;
    let btn_rect = RECT {
        left: btn_x,
        top: btn_y,
        right: btn_x + btn_w,
        bottom: btn_y + btn_h,
    };

    let btn_bg = if state.check_update_btn.hover {
        CLR_MUTED_FG
    } else {
        CLR_SECONDARY
    };
    let btn_border = if state.check_update_btn.hover {
        CLR_MUTED_FG
    } else {
        CLR_BORDER
    };
    let btn_fg = if state.check_update_btn.disabled {
        CLR_MUTED_FG
    } else {
        CLR_FOREGROUND
    };
    draw_rounded_rect(hdc, &btn_rect, CARD_RADIUS, btn_bg, btn_border);
    draw_text_simple(
        hdc,
        &btn_text,
        btn_x + (btn_w - bw) / 2,
        btn_y + (btn_h - bh) / 2,
        btn_fg,
        fonts.xs,
    );
    state.check_update_btn.rect = btn_rect;
}

fn draw_shortcuts_tab(hdc: HDC, y: i32, state: &mut UiState, fonts: &Fonts) {
    let x = PADDING;
    let inner_x = x + 16;
    let inner_right = x + CONTENT_WIDTH - 16;

    let card = RECT {
        left: x,
        top: y,
        right: x + CONTENT_WIDTH,
        bottom: y + 160,
    };
    draw_rounded_rect(hdc, &card, CARD_RADIUS, CLR_BACKGROUND, CLR_BORDER);

    draw_text_simple(
        hdc,
        "Keyboard Shortcuts",
        inner_x,
        y + 14,
        CLR_FOREGROUND,
        fonts.small_bold,
    );

    let labels = ["Toggle Dimmer", "Increase Dimming", "Decrease Dimming"];
    let keys = state.shortcut_texts.clone();

    for (i, (label, key)) in labels.iter().zip(keys.iter()).enumerate() {
        let row_y = y + 44 + (i as i32) * 38;
        draw_text_simple(hdc, label, inner_x, row_y + 4, CLR_MUTED_FG, fonts.small);

        let (kw, kh) = measure_text(hdc, key, fonts.mono);
        let kbd_w = kw + 16;
        let kbd_h = kh + 8;
        let kbd_x = inner_right - kbd_w;
        let kbd_rect = RECT {
            left: kbd_x,
            top: row_y,
            right: kbd_x + kbd_w,
            bottom: row_y + kbd_h,
        };
        draw_rounded_rect(hdc, &kbd_rect, CARD_RADIUS - 2, CLR_SECONDARY, CLR_BORDER);
        draw_text_simple(
            hdc,
            key,
            kbd_x + (kbd_w - kw) / 2,
            row_y + (kbd_h - kh) / 2,
            CLR_MUTED_FG,
            fonts.mono,
        );
    }

    let hint = "Press a key combo while focused on a shortcut to change it.";
    let (hw, _) = measure_text(hdc, hint, fonts.xxs);
    let hint_x = PADDING + (CONTENT_WIDTH - hw) / 2;
    draw_text_simple(hdc, hint, hint_x, card.bottom + 8, CLR_MUTED_FG, fonts.xxs);
}

fn draw_toggle(hdc: HDC, x: i32, y: i32, checked: bool) -> RECT {
    let w = 44;
    let h = 24;
    let rect = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };

    let track_color = if checked { CLR_BRAND } else { CLR_INPUT };
    draw_rounded_rect(hdc, &rect, h / 2, track_color, track_color);

    let thumb_r = 10;
    let thumb_x = if checked {
        x + w - 2 - thumb_r
    } else {
        x + 2 + thumb_r
    };
    let thumb_cy = y + h / 2;
    draw_circle(hdc, thumb_x, thumb_cy, thumb_r, CLR_FOREGROUND);

    rect
}

fn draw_toast(hdc: HDC, client: &RECT, state: &UiState, fonts: &Fonts) {
    let msg = &state.toast_message;
    if msg.is_empty() {
        return;
    }

    let (tw, th) = measure_text(hdc, msg, fonts.small_bold);
    let toast_w = tw + 48;
    let toast_h = th + 24;
    let toast_x = (client.right - toast_w) / 2;
    let toast_y = client.bottom - toast_h - 24;

    let toast_rect = RECT {
        left: toast_x,
        top: toast_y,
        right: toast_x + toast_w,
        bottom: toast_y + toast_h,
    };
    draw_rounded_rect(
        hdc,
        &toast_rect,
        CARD_RADIUS,
        CLR_FOREGROUND,
        CLR_FOREGROUND,
    );
    draw_text_simple(
        hdc,
        msg,
        toast_x + (toast_w - tw) / 2,
        toast_y + (toast_h - th) / 2,
        CLR_BACKGROUND,
        fonts.small_bold,
    );
}
