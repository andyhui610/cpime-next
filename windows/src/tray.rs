//! CPIME-Next Windows 系統列常駐圖示與右鍵選單 (`tray.rs`)
//!
//! 透過 Win32 `Shell_NotifyIconW` 與 GDI `CreateDIBSection` + `CreateIconIndirect`
//! 在記憶體中動態生成高解析度 32x32 狀態圖示（不依賴外部 `.ico` 檔案）：
//! - 藍底 (`#2563EB`) 白字 `"粵"`：粵語口語模式
//! - 綠底 (`#059669`) 白字 `"書"`：粵音標準書面語模式
//! - 灰底 (`#4B5563`) 白字 `"英"`：英文直通模式
//! 並支援右下角圖示左鍵切換中/英、右鍵彈出繁體中文選單。

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use cpime_core::OutputMode;
use windows_sys::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreatePen, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, FillRect, GetDC, ReleaseDC, RoundRect, SelectObject,
    SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE,
    DT_VCENTER, FF_DONTCARE, FW_BOLD, OUT_TT_PRECIS, PS_SOLID, RGBQUAD, TRANSPARENT,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    PostMessageW, SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED, MF_SEPARATOR,
    MF_STRING, MF_UNCHECKED, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_NULL, WM_USER,
};

use crate::overlay::ImeMode;

/// 系統列圖示自訂回呼訊息 (`WM_USER + 100`)
pub const WM_TRAYICON: u32 = WM_USER + 100;

/// 右鍵選單項目 ID
pub const ID_TOGGLE_IME: usize = 1001;
pub const ID_MODE_COLLOQUIAL: usize = 1002;
pub const ID_MODE_WRITTEN: usize = 1003;
pub const ID_RELOAD_USER_DICT: usize = 1004;
pub const ID_MODE_EN_SENTENCE: usize = 1005;
pub const ID_EXIT: usize = 1009;

const TRAY_ICON_UID: u32 = 1;
const ICON_SIZE: i32 = 32;

const BG_RGB_YUE: u32 = 0x2563EB; // 藍底白字「粵」
const BG_RGB_WRITTEN: u32 = 0x059669; // 綠底白字「書」
const BG_RGB_TRANS: u32 = 0x7C3AED; // 紫底白字「譯」
const BG_RGB_ENG: u32 = 0x4B5563; // 灰底白字「英」

struct TrayState {
    hwnd: HWND,
    icon_yue: HICON,
    icon_written: HICON,
    icon_trans: HICON,
    icon_eng: HICON,
    registered: bool,
}

thread_local! {
    static TRAY_STATE: RefCell<TrayState> = const {
        RefCell::new(TrayState {
            hwnd: null_mut(),
            icon_yue: null_mut(),
            icon_written: null_mut(),
            icon_trans: null_mut(),
            icon_eng: null_mut(),
            registered: false,
        })
    };
}

/// 將 `0xRRGGBB` 轉換為 Win32 `COLORREF` (`0x00BBGGRR`)
#[inline]
const fn hex_rgb_to_colorref(rgb_hex: u32) -> COLORREF {
    let r = (rgb_hex >> 16) & 0xFF;
    let g = (rgb_hex >> 8) & 0xFF;
    let b = rgb_hex & 0xFF;
    r | (g << 8) | (b << 16)
}

/// 將 Rust `&str` 轉為以 `\0` 結尾的 UTF-16 `Vec<u16>`
fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 將字串寫入固定長度的 UTF-16 Tip 陣列（最多 127 字元 + 結尾 `\0`）
fn fill_wide_tip(dst: &mut [u16; 128], tip: &str) {
    dst.fill(0);
    for (idx, code_unit) in tip.encode_utf16().take(127).enumerate() {
        dst[idx] = code_unit;
    }
}

/// 使用 Win32 GDI 在記憶體中動態生成 32x32 清晰狀態字元圖示 (`HICON`)
pub fn create_text_icon(ch: &str, bg_rgb: u32) -> HICON {
    unsafe {
        let screen_dc = GetDC(null_mut());
        let mem_dc = CreateCompatibleDC(screen_dc);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: ICON_SIZE,
                biHeight: -ICON_SIZE, // Top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };

        let mut bits_ptr: *mut c_void = null_mut();
        let hbm_color = CreateDIBSection(
            mem_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            null_mut(),
            0,
        );

        if hbm_color.is_null() || bits_ptr.is_null() {
            if !hbm_color.is_null() {
                DeleteObject(hbm_color);
            }
            DeleteDC(mem_dc);
            ReleaseDC(null_mut(), screen_dc);
            return null_mut();
        }

        let old_bmp = SelectObject(mem_dc, hbm_color);

        // 1. 先將整張畫布清為全黑 (0x00000000)，稍後圓角外側維持透明
        let full_rect = RECT {
            left: 0,
            top: 0,
            right: ICON_SIZE,
            bottom: ICON_SIZE,
        };
        let clear_brush = CreateSolidBrush(0);
        FillRect(mem_dc, &full_rect, clear_brush);
        DeleteObject(clear_brush);

        // 2. 繪製帶圓角的實心色塊背景
        let bg_colorref = hex_rgb_to_colorref(bg_rgb);
        let bg_brush = CreateSolidBrush(bg_colorref);
        let bg_pen = CreatePen(PS_SOLID, 1, bg_colorref);
        let old_brush = SelectObject(mem_dc, bg_brush);
        let old_pen = SelectObject(mem_dc, bg_pen);
        RoundRect(mem_dc, 1, 1, ICON_SIZE - 1, ICON_SIZE - 1, 8, 8);
        SelectObject(mem_dc, old_brush);
        SelectObject(mem_dc, old_pen);
        DeleteObject(bg_brush);
        DeleteObject(bg_pen);

        // 3. 使用 "Microsoft JhengHei UI" 粗體在中央繪製白色漢字
        let font_face = to_wide_null("Microsoft JhengHei UI");
        let font = CreateFontW(
            -21,
            0,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_TT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            font_face.as_ptr(),
        );
        let old_font = SelectObject(mem_dc, font);
        SetBkMode(mem_dc, TRANSPARENT as i32);
        SetTextColor(mem_dc, hex_rgb_to_colorref(0xFFFFFF));

        let text_utf16: Vec<u16> = ch.encode_utf16().collect();
        let mut text_rect = RECT {
            left: 0,
            top: 0,
            right: ICON_SIZE,
            bottom: ICON_SIZE,
        };
        DrawTextW(
            mem_dc,
            text_utf16.as_ptr(),
            text_utf16.len() as i32,
            &mut text_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );

        SelectObject(mem_dc, old_font);
        DeleteObject(font);

        // 必須先還原點陣圖並刪除 mem_dc，確保所有 GDI 繪製命令皆已刷新寫入 bits_ptr
        SelectObject(mem_dc, old_bmp);
        DeleteDC(mem_dc);
        ReleaseDC(null_mut(), screen_dc);

        // 4. 修正 DIB 像素 Alpha 通道：GDI 會將 Alpha Byte 清為 0，
        //    此處將有效色塊像素設為不透明 (0xFF000000)，圓角外黑色像素保留透明 (0x00000000)
        let pixel_count = (ICON_SIZE * ICON_SIZE) as usize;
        let pixels = std::slice::from_raw_parts_mut(bits_ptr as *mut u32, pixel_count);
        for px in pixels.iter_mut() {
            if (*px & 0x00FF_FFFF) != 0 {
                *px |= 0xFF00_0000;
            } else {
                *px = 0;
            }
        }

        // 5. 建立單色 AND Mask 並透過 CreateIconIndirect 生成 HICON
        let mask_bytes = [0u8; (ICON_SIZE * ICON_SIZE / 8) as usize];
        let hbm_mask = CreateBitmap(
            ICON_SIZE,
            ICON_SIZE,
            1,
            1,
            mask_bytes.as_ptr() as *const c_void,
        );

        let icon_info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm_color,
        };
        let hicon = CreateIconIndirect(&icon_info);

        DeleteObject(hbm_mask);
        DeleteObject(hbm_color);

        hicon
    }
}

/// 根據當前中/英/英譯粵整句模式與口語/書面語模式取得對應圖示與系統列 Tooltip 文字
fn resolve_icon_and_tip(
    state: &TrayState,
    mode: ImeMode,
    output_mode: OutputMode,
) -> (HICON, &'static str) {
    match (mode, output_mode) {
        (ImeMode::Cantonese, OutputMode::Colloquial) => (
            state.icon_yue,
            "CPIME-Next 廣東話輸入法 [粵·口語]",
        ),
        (ImeMode::Cantonese, OutputMode::Written) => (
            state.icon_written,
            "CPIME-Next 廣東話輸入法 [書·書面語]",
        ),
        (ImeMode::EnSentence, OutputMode::Colloquial) => (
            state.icon_trans,
            "CPIME-Next 英文整句直出廣東話模式 [譯·口語]",
        ),
        (ImeMode::EnSentence, OutputMode::Written) => (
            state.icon_trans,
            "CPIME-Next 英文整句直出廣東話模式 [譯·書面語]",
        ),
        (ImeMode::English, _) => (
            state.icon_eng,
            "CPIME-Next 廣東話輸入法 [英·英文模式]",
        ),
    }
}

/// 註冊右下角系統列常駐圖示 (`Shell_NotifyIconW(NIM_ADD, ...)`)
pub fn init_tray_icon(hwnd: HWND) -> Result<(), String> {
    let icon_yue = create_text_icon("粵", BG_RGB_YUE);
    let icon_written = create_text_icon("書", BG_RGB_WRITTEN);
    let icon_trans = create_text_icon("譯", BG_RGB_TRANS);
    let icon_eng = create_text_icon("英", BG_RGB_ENG);

    if icon_yue.is_null() || icon_written.is_null() || icon_trans.is_null() || icon_eng.is_null() {
        unsafe {
            if !icon_yue.is_null() {
                DestroyIcon(icon_yue);
            }
            if !icon_written.is_null() {
                DestroyIcon(icon_written);
            }
            if !icon_trans.is_null() {
                DestroyIcon(icon_trans);
            }
            if !icon_eng.is_null() {
                DestroyIcon(icon_eng);
            }
        }
        return Err("無法在記憶體中動態生成系統列圖示 (CreateIconIndirect 失敗)".to_string());
    }

    unsafe {
        let mut nid: NOTIFYICONDATAW = zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ICON_UID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = icon_yue;
        fill_wide_tip(&mut nid.szTip, "CPIME-Next 廣東話輸入法");

        if Shell_NotifyIconW(NIM_ADD, &nid) == 0 {
            DestroyIcon(icon_yue);
            DestroyIcon(icon_written);
            DestroyIcon(icon_trans);
            DestroyIcon(icon_eng);
            return Err("註冊 Windows 系統列圖示 (Shell_NotifyIconW NIM_ADD) 失敗".to_string());
        }
    }

    TRAY_STATE.with(|cell| {
        let mut st = cell.borrow_mut();
        st.hwnd = hwnd;
        st.icon_yue = icon_yue;
        st.icon_written = icon_written;
        st.icon_trans = icon_trans;
        st.icon_eng = icon_eng;
        st.registered = true;
    });

    Ok(())
}

/// 即時切換右下角系統列圖示與 Tooltip (`Shell_NotifyIconW(NIM_MODIFY, ...)`)
pub fn update_tray_icon(mode: ImeMode, output_mode: OutputMode) {
    TRAY_STATE.with(|cell| {
        let st = cell.borrow();
        if !st.registered || st.hwnd.is_null() {
            return;
        }

        let (hicon, tip) = resolve_icon_and_tip(&st, mode, output_mode);
        unsafe {
            let mut nid: NOTIFYICONDATAW = zeroed();
            nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = st.hwnd;
            nid.uID = TRAY_ICON_UID;
            nid.uFlags = NIF_ICON | NIF_TIP;
            nid.hIcon = hicon;
            fill_wide_tip(&mut nid.szTip, tip);
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    });
}

/// 移除右下角系統列圖示並釋放所有記憶體生成的 `HICON`
pub fn remove_tray_icon() {
    TRAY_STATE.with(|cell| {
        let mut st = cell.borrow_mut();
        if st.registered && !st.hwnd.is_null() {
            unsafe {
                let mut nid: NOTIFYICONDATAW = zeroed();
                nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = st.hwnd;
                nid.uID = TRAY_ICON_UID;
                Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            st.registered = false;
        }

        unsafe {
            if !st.icon_yue.is_null() {
                DestroyIcon(st.icon_yue);
                st.icon_yue = null_mut();
            }
            if !st.icon_written.is_null() {
                DestroyIcon(st.icon_written);
                st.icon_written = null_mut();
            }
            if !st.icon_trans.is_null() {
                DestroyIcon(st.icon_trans);
                st.icon_trans = null_mut();
            }
            if !st.icon_eng.is_null() {
                DestroyIcon(st.icon_eng);
                st.icon_eng = null_mut();
            }
        }
    });
}

/// 彈出右下角系統列右鍵功能選單
pub unsafe fn show_tray_context_menu(hwnd: HWND, mode: ImeMode, output_mode: OutputMode) {
    let hmenu = CreatePopupMenu();
    if hmenu.is_null() {
        return;
    }

    let ime_flag = MF_STRING
        | if mode != ImeMode::English {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
    let en_sentence_flag = MF_STRING
        | if mode == ImeMode::EnSentence {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
    let colloquial_flag = MF_STRING
        | if output_mode == OutputMode::Colloquial {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
    let written_flag = MF_STRING
        | if output_mode == OutputMode::Written {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };

    let lbl_toggle = to_wide_null("啟用廣東話輸入 (Left Shift / Ctrl+Space)");
    let lbl_en_sentence = to_wide_null("英譯粵整句模式 (Ctrl+Shift+E / ; 前綴)");
    let lbl_colloquial = to_wide_null("口語模式 (嘅 / 喺 / 唔 / 𨋢)");
    let lbl_written = to_wide_null("書面語模式 (的 / 在 / 不 / 升降機)");
    let lbl_reload = to_wide_null("重新載入個人詞庫 (user_dict.db)");
    let lbl_exit = to_wide_null("結束 CPIME-Next");

    AppendMenuW(hmenu, ime_flag, ID_TOGGLE_IME, lbl_toggle.as_ptr());
    AppendMenuW(hmenu, en_sentence_flag, ID_MODE_EN_SENTENCE, lbl_en_sentence.as_ptr());
    AppendMenuW(hmenu, MF_SEPARATOR, 0, null());
    AppendMenuW(hmenu, colloquial_flag, ID_MODE_COLLOQUIAL, lbl_colloquial.as_ptr());
    AppendMenuW(hmenu, written_flag, ID_MODE_WRITTEN, lbl_written.as_ptr());
    AppendMenuW(hmenu, MF_SEPARATOR, 0, null());
    AppendMenuW(hmenu, MF_STRING, ID_RELOAD_USER_DICT, lbl_reload.as_ptr());
    AppendMenuW(hmenu, MF_STRING, ID_EXIT, lbl_exit.as_ptr());

    // 先將視窗設為前景，確保使用者點擊選單外部時彈出選單能自動消失
    SetForegroundWindow(hwnd);

    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);

    TrackPopupMenu(
        hmenu,
        TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        null(),
    );

    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(hmenu);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_text_icons_in_memory() {
        let h1 = create_text_icon("粵", BG_RGB_YUE);
        let h2 = create_text_icon("書", BG_RGB_WRITTEN);
        let h3 = create_text_icon("譯", BG_RGB_TRANS);
        let h4 = create_text_icon("英", BG_RGB_ENG);

        assert!(!h1.is_null(), "應能於記憶體生成『粵』圖示");
        assert!(!h2.is_null(), "應能於記憶體生成『書』圖示");
        assert!(!h3.is_null(), "應能於記憶體生成『譯』圖示");
        assert!(!h4.is_null(), "應能於記憶體生成『英』圖示");

        unsafe {
            DestroyIcon(h1);
            DestroyIcon(h2);
            DestroyIcon(h3);
            DestroyIcon(h4);
        }
    }
}
