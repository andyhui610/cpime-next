//! CPIME-Next 無焦點浮動候選字視窗 (`overlay.rs`)
//!
//! 實作具備 `WS_POPUP | WS_DISABLED` 與 `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`
//! 屬性的自繪浮動視窗，確保在記事本、瀏覽器等任何目標應用程式打字時絕對不搶奪焦點。
//! 內建：
//! 1. `GetGUIThreadInfo` 游標追蹤 + `ClientToScreen` 座標換算（若遇 Chrome 等自繪游標為 0,0 則自動 fallback 至 `GetCursorPos` 右下方 20px 或螢幕底部中央）。
//! 2. GDI 雙緩衝繪製（Double-Buffered Dark UI，`#1E1E2E` 深色主題 + `"Microsoft JhengHei UI"` 微軟正黑體）。

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use cpime_core::{format_segmented_pinyin, Candidate, OutputMode};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
    CreatePen, CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect, GetDC,
    GetTextExtentPoint32W, InvalidateRect, LineTo, MoveToEx, ReleaseDC, RoundRect, SelectObject,
    SetBkMode, SetTextColor, TextOutW, UpdateWindow, BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, FF_DONTCARE, FW_BOLD, FW_MEDIUM, FW_SEMIBOLD, HFONT,
    OUT_TT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetCursorPos, GetGUIThreadInfo, GetSystemMetrics, KillTimer,
    PostQuitMessage, RegisterClassW, SetTimer, SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW,
    GUITHREADINFO, HWND_TOPMOST, MA_NOACTIVATE, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SW_HIDE, WM_COMMAND, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONUP, WM_MOUSEACTIVATE,
    WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

use crate::tray::{
    ID_EXIT, ID_MODE_COLLOQUIAL, ID_MODE_WRITTEN, ID_RELOAD_USER_DICT, ID_TOGGLE_IME, WM_TRAYICON,
};

/// 狀態提示計時器 ID（切換 `[粵]` / `[英]` 時短暫顯示 1000ms 後自動隱藏）
const TIMER_ID_TOAST: usize = 1001;
const TOAST_DURATION_MS: u32 = 1000;

/// 將 R, G, B 轉換為 Win32 `COLORREF` (`0x00BBGGRR`)
#[inline]
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

// 深色現代化主題色票 (Catppuccin Mocha Dark Theme)
const COLOR_BG: COLORREF = rgb(0x1E, 0x1E, 0x2E); // #1E1E2E 深灰背景
const COLOR_BORDER: COLORREF = rgb(0x58, 0x5B, 0x70); // #585B70 外框邊線
const COLOR_DIVIDER: COLORREF = rgb(0x31, 0x32, 0x44); // #313244 分隔線
const COLOR_PINYIN: COLORREF = rgb(0xCD, 0xD6, 0xF4); // #CDD6F4 第一行拼音亮白字
const COLOR_SUBTEXT: COLORREF = rgb(0xA6, 0xAD, 0xC8); // #A6ADC8 頁碼與狀態輔助字
const COLOR_FIRST_CAND: COLORREF = rgb(0x00, 0xE5, 0xFF); // #00E5FF 第 1 順位醒目亮青藍色
const COLOR_FIRST_BG: COLORREF = rgb(0x28, 0x34, 0x4A); // #28344A 第 1 順位底色膠囊
const COLOR_OTHER_CAND: COLORREF = rgb(0xEC, 0xEF, 0xF4); // #ECEFF4 其他候選詞亮白色
const COLOR_ENG_MODE: COLORREF = rgb(0xA6, 0xE3, 0xA1); // #A6E3A1 [英] 模式提示亮綠色
const COLOR_WRITTEN_ACCENT: COLORREF = rgb(0x34, 0xD3, 0x99); // #34D399 [粵·書面] 模式翠綠色

/// 輸入法當前模式（粵語模式 / 英文模式）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeMode {
    /// 粵語雙軌輸入模式（粵拼 + 英文反查）
    Cantonese,
    /// 英文直通模式
    English,
}

impl ImeMode {
    /// 簡短模式標記（顯示於浮動候選窗標題列：`[粵·口語]` / `[粵·書面]` / `[英]`）
    pub fn short_badge(self, output_mode: OutputMode) -> &'static str {
        match (self, output_mode) {
            (ImeMode::Cantonese, OutputMode::Colloquial) => "[粵·口語]",
            (ImeMode::Cantonese, OutputMode::Written) => "[粵·書面]",
            (ImeMode::English, _) => "[英]",
        }
    }

    /// 切換模式時的浮動提示文字
    pub fn toast_label(self, output_mode: OutputMode) -> &'static str {
        match (self, output_mode) {
            (ImeMode::Cantonese, OutputMode::Colloquial) => "[粵·口語] 廣東話口語模式",
            (ImeMode::Cantonese, OutputMode::Written) => "[粵·書面] 標準書面語模式",
            (ImeMode::English, _) => "[英] 英文直通模式",
        }
    }
}

/// 浮動候選字視窗的顯示內容狀態
#[derive(Debug, Clone)]
pub enum OverlayDisplay {
    /// 隱藏視窗
    Hidden,
    /// 短暫顯示 `[粵·口語]` / `[粵·書面]` / `[英]` 模式切換提示
    ModeToast {
        mode: ImeMode,
        output_mode: OutputMode,
    },
    /// 顯示當前拼音與分頁候選詞清單
    Composing {
        mode: ImeMode,
        output_mode: OutputMode,
        input_buffer: String,
        page_candidates: Vec<Candidate>,
        current_page: usize,
        total_pages: usize,
    },
}

struct OverlayContext {
    hwnd: HWND,
    display: OverlayDisplay,
}

thread_local! {
    static OVERLAY_CTX: RefCell<OverlayContext> = const {
        RefCell::new(OverlayContext {
            hwnd: null_mut(),
            display: OverlayDisplay::Hidden,
        })
    };
}

/// 將 Rust `&str` 轉為以 `\0` 結尾的 UTF-16 `Vec<u16>`
fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 建立 `"Microsoft JhengHei UI"` 微軟正黑體字型物件
unsafe fn create_jhenghei_font(height: i32, weight: i32) -> HFONT {
    let face = to_wide_null("Microsoft JhengHei UI");
    let _ = BI_RGB;
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_TT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        (DEFAULT_PITCH | FF_DONTCARE) as u32,
        face.as_ptr(),
    )
}

/// 計算 UTF-16 字串在指定 `HDC` 下的像素寬高
unsafe fn measure_text(hdc: *mut c_void, text: &str) -> (i32, i32) {
    if text.is_empty() {
        return (0, 0);
    }
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let mut size: windows_sys::Win32::Foundation::SIZE = zeroed();
    GetTextExtentPoint32W(hdc, utf16.as_ptr(), utf16.len() as i32, &mut size);
    (size.cx, size.cy)
}

/// 輸出 UTF-8 字串（自動轉為 UTF-16 支援香港四位元組擴展漢字如「𨋢」）
unsafe fn draw_text_utf16(hdc: *mut c_void, x: i32, y: i32, text: &str) {
    if text.is_empty() {
        return;
    }
    let utf16: Vec<u16> = text.encode_utf16().collect();
    TextOutW(hdc, x, y, utf16.as_ptr(), utf16.len() as i32);
}

/// 註冊並建立無焦點浮動候選字視窗 (`WS_POPUP | WS_DISABLED` + `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`)
pub fn init_overlay_window() -> Result<HWND, String> {
    unsafe {
        let hinstance = GetModuleHandleW(null());
        let class_name = to_wide_null("CpimeOverlayWindowClass");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wc);

        let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        // 不設定 WS_DISABLED，確保右鍵選單 (TrackPopupMenu) 能正確將 WM_COMMAND 派發至本視窗，
        // 同時透過 WS_EX_NOACTIVATE 與 WM_MOUSEACTIVATE (MA_NOACTIVATE) 保證絕不搶奪前景焦點
        let style = WS_POPUP;
        let title = to_wide_null("CPIME Candidate Overlay");

        let hwnd = CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            100,
            100,
            320,
            78,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );

        if hwnd.is_null() {
            return Err("建立無焦點浮動候選字視窗 (CreateWindowExW) 失敗".to_string());
        }

        OVERLAY_CTX.with(|ctx| {
            ctx.borrow_mut().hwnd = hwnd;
        });

        Ok(hwnd)
    }
}

/// 追蹤當前前景視窗輸入游標位置：
/// 1. 優先透過 `GetGUIThreadInfo(0, &mut gui_info)` 取得 `rcCaret` 並以 `ClientToScreen` 轉為螢幕座標。
/// 2. 若目標為 Chrome / Edge / VS Code 等自繪游標（座標為 0,0），則自動 fallback 至 `GetCursorPos`（滑鼠游標右下方 20px）。
/// 3. 若滑鼠座標亦不可用，則 fallback 至螢幕底部中央，並做螢幕邊界防裁切修正。
unsafe fn get_target_screen_position(win_w: i32, win_h: i32) -> (i32, i32) {
    let screen_w = GetSystemMetrics(SM_CXSCREEN).max(800);
    let screen_h = GetSystemMetrics(SM_CYSCREEN).max(600);

    let mut pos: Option<(i32, i32)> = None;

    // 1. 嘗試透過 GetGUIThreadInfo 取得標準 Win32 Caret 位置
    let mut gui_info: GUITHREADINFO = zeroed();
    gui_info.cbSize = size_of::<GUITHREADINFO>() as u32;
    if GetGUIThreadInfo(0, &mut gui_info) != 0 {
        let rc = gui_info.rcCaret;
        let has_caret_rect = rc.left != 0 || rc.top != 0 || rc.right != 0 || rc.bottom != 0;
        let target_hwnd = if !gui_info.hwndCaret.is_null() {
            gui_info.hwndCaret
        } else {
            gui_info.hwndFocus
        };

        if !target_hwnd.is_null() && has_caret_rect {
            let mut pt = POINT {
                x: rc.left,
                y: rc.bottom,
            };
            if ClientToScreen(target_hwnd, &mut pt) != 0 && (pt.x != 0 || pt.y != 0) {
                pos = Some((pt.x, pt.y + 6));
            }
        }
    }

    // 2. Fallback：若 Caret 為 (0, 0)（例如瀏覽器或 Electron 視窗），改用滑鼠游標右下方 20px
    if pos.is_none() {
        let mut cursor_pt: POINT = zeroed();
        if GetCursorPos(&mut cursor_pt) != 0 && (cursor_pt.x != 0 || cursor_pt.y != 0) {
            pos = Some((cursor_pt.x + 20, cursor_pt.y + 20));
        }
    }

    // 3. Fallback：螢幕底部中央
    let (mut x, mut y) = pos.unwrap_or(((screen_w - win_w) / 2, screen_h - win_h - 80));

    // 螢幕邊界防超出修正
    if x + win_w + 12 > screen_w {
        x = (screen_w - win_w - 12).max(8);
    }
    if x < 8 {
        x = 8;
    }
    if y + win_h + 48 > screen_h {
        y = (y - win_h - 32).max(8);
    }
    if y < 8 {
        y = 8;
    }

    (x, y)
}

/// 計算給定 `OverlayDisplay` 所需的最佳視窗寬高
unsafe fn compute_window_size(hwnd: HWND, display: &OverlayDisplay) -> (i32, i32) {
    let hdc = GetDC(hwnd);
    if hdc.is_null() {
        return (320, 78);
    }

    let size = match display {
        OverlayDisplay::Hidden => (0, 0),
        OverlayDisplay::ModeToast { mode, output_mode } => {
            let font = create_jhenghei_font(-19, FW_BOLD as i32);
            let old_font = SelectObject(hdc, font);
            let (tw, _) = measure_text(hdc, mode.toast_label(*output_mode));
            SelectObject(hdc, old_font);
            DeleteObject(font);
            ((tw + 36).max(168), 44)
        }
        OverlayDisplay::Composing {
            mode,
            output_mode,
            input_buffer,
            page_candidates,
            current_page,
            total_pages,
        } => {
            let font_top = create_jhenghei_font(-17, FW_SEMIBOLD as i32);
            let font_cand = create_jhenghei_font(-20, FW_MEDIUM as i32);

            let old_font = SelectObject(hdc, font_top);
            let segmented_pinyin = format_segmented_pinyin(input_buffer);
            let top_left = format!("> {}", segmented_pinyin);
            let top_right = format!(
                "{}  {}/{}",
                mode.short_badge(*output_mode),
                current_page + 1,
                (*total_pages).max(1)
            );
            let (w_left, _) = measure_text(hdc, &top_left);
            let (w_right, _) = measure_text(hdc, &top_right);
            let line1_w = w_left + w_right + 48;

            SelectObject(hdc, font_cand);
            let mut line2_w = 28;
            if page_candidates.is_empty() {
                let (empty_w, _) = measure_text(hdc, "(無匹配候選詞 — 按 Enter 直接出英文)");
                line2_w += empty_w;
            } else {
                for (idx, cand) in page_candidates.iter().enumerate() {
                    let label = format!("{}.{}", idx + 1, cand.word);
                    let (cw, _) = measure_text(hdc, &label);
                    line2_w += cw + 22;
                }
            }

            SelectObject(hdc, old_font);
            DeleteObject(font_top);
            DeleteObject(font_cand);

            let width = line1_w.max(line2_w).max(240);
            (width, 80)
        }
    };

    ReleaseDC(hwnd, hdc);
    size
}

/// 更新浮動視窗顯示內容並自動移動至游標旁重繪
pub fn update_overlay(display: OverlayDisplay) {
    let hwnd = OVERLAY_CTX.with(|ctx| {
        let mut guard = ctx.borrow_mut();
        guard.display = display.clone();
        guard.hwnd
    });

    if hwnd.is_null() {
        return;
    }

    unsafe {
        match display {
            OverlayDisplay::Hidden => {
                KillTimer(hwnd, TIMER_ID_TOAST);
                ShowWindow(hwnd, SW_HIDE);
            }
            OverlayDisplay::ModeToast { .. } => {
                let (win_w, win_h) = compute_window_size(hwnd, &display);
                let (x, y) = get_target_screen_position(win_w, win_h);
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    x,
                    y,
                    win_w,
                    win_h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                InvalidateRect(hwnd, null(), 1);
                UpdateWindow(hwnd);
                SetTimer(hwnd, TIMER_ID_TOAST, TOAST_DURATION_MS, None);
            }
            OverlayDisplay::Composing { .. } => {
                KillTimer(hwnd, TIMER_ID_TOAST);
                let (win_w, win_h) = compute_window_size(hwnd, &display);
                let (x, y) = get_target_screen_position(win_w, win_h);
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    x,
                    y,
                    win_w,
                    win_h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                InvalidateRect(hwnd, null(), 1);
                UpdateWindow(hwnd);
            }
        }
    }
}

/// GDI 雙緩衝繪製（Double-Buffered Dark UI）
unsafe fn paint_overlay(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }

    let width = (ps.rcPaint.right - ps.rcPaint.left).max(1);
    let height = (ps.rcPaint.bottom - ps.rcPaint.top).max(1);

    let mem_dc = CreateCompatibleDC(hdc);
    let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
    let old_bmp = SelectObject(mem_dc, mem_bmp);

    // 1. 填滿背景與繪製圓角深色外框 (#1E1E2E)
    let bg_rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let bg_brush = CreateSolidBrush(COLOR_BG);
    FillRect(mem_dc, &bg_rect, bg_brush);

    let border_pen = CreatePen(PS_SOLID, 1, COLOR_BORDER);
    let old_pen = SelectObject(mem_dc, border_pen);
    let old_brush = SelectObject(mem_dc, bg_brush);
    RoundRect(mem_dc, 0, 0, width, height, 10, 10);

    SetBkMode(mem_dc, TRANSPARENT as i32);

    let display = OVERLAY_CTX.with(|ctx| ctx.borrow().display.clone());

    match display {
        OverlayDisplay::Hidden => {}
        OverlayDisplay::ModeToast { mode, output_mode } => {
            let font = create_jhenghei_font(-19, FW_BOLD as i32);
            let old_font = SelectObject(mem_dc, font);
            let color = match (mode, output_mode) {
                (ImeMode::Cantonese, OutputMode::Colloquial) => COLOR_FIRST_CAND,
                (ImeMode::Cantonese, OutputMode::Written) => COLOR_WRITTEN_ACCENT,
                (ImeMode::English, _) => COLOR_ENG_MODE,
            };
            SetTextColor(mem_dc, color);
            let label = mode.toast_label(output_mode);
            let (tw, th) = measure_text(mem_dc, label);
            let tx = ((width - tw) / 2).max(10);
            let ty = ((height - th) / 2).max(6);
            draw_text_utf16(mem_dc, tx, ty, label);

            SelectObject(mem_dc, old_font);
            DeleteObject(font);
        }
        OverlayDisplay::Composing {
            mode,
            output_mode,
            input_buffer,
            page_candidates,
            current_page,
            total_pages,
        } => {
            // 第一行：繪製當前拼音字串（如 `> afternoon` 或 `> gamjat`）與模式標籤/頁碼
            let font_top = create_jhenghei_font(-17, FW_SEMIBOLD as i32);
            let old_font = SelectObject(mem_dc, font_top);

            let segmented_pinyin = format_segmented_pinyin(&input_buffer);
            let top_left = format!("> {}", segmented_pinyin);
            SetTextColor(mem_dc, COLOR_PINYIN);
            draw_text_utf16(mem_dc, 14, 9, &top_left);

            let badge_text = mode.short_badge(output_mode);
            let page_text = format!("  {}/{}", current_page + 1, total_pages.max(1));
            let (bw, _) = measure_text(mem_dc, badge_text);
            let (pw, _) = measure_text(mem_dc, &page_text);
            let right_start = (width - (bw + pw) - 14).max(100);

            let badge_color = match (mode, output_mode) {
                (ImeMode::Cantonese, OutputMode::Written) => COLOR_WRITTEN_ACCENT,
                (ImeMode::Cantonese, OutputMode::Colloquial) => COLOR_FIRST_CAND,
                (ImeMode::English, _) => COLOR_ENG_MODE,
            };
            SetTextColor(mem_dc, badge_color);
            draw_text_utf16(mem_dc, right_start, 9, badge_text);

            SetTextColor(mem_dc, COLOR_SUBTEXT);
            draw_text_utf16(mem_dc, right_start + bw, 9, &page_text);

            // 分隔線
            let div_pen = CreatePen(PS_SOLID, 1, COLOR_DIVIDER);
            SelectObject(mem_dc, div_pen);
            MoveToEx(mem_dc, 10, 36, null_mut());
            LineTo(mem_dc, width - 10, 36);
            SelectObject(mem_dc, border_pen);
            DeleteObject(div_pen);

            // 第二行：橫向繪製當頁候選詞（第 1 順位以醒目亮藍/青色標示）
            let font_cand_first = create_jhenghei_font(-20, FW_BOLD as i32);
            let font_cand_other = create_jhenghei_font(-20, FW_MEDIUM as i32);

            if page_candidates.is_empty() {
                SelectObject(mem_dc, font_cand_other);
                SetTextColor(mem_dc, COLOR_SUBTEXT);
                draw_text_utf16(mem_dc, 14, 44, "(無匹配候選詞 — 按 Enter 直接出英文)");
            } else {
                let mut cur_x = 14;
                let cand_y = 44;

                for (idx, cand) in page_candidates.iter().enumerate() {
                    let item_text = format!("{}.{}", idx + 1, cand.word);
                    if idx == 0 {
                        SelectObject(mem_dc, font_cand_first);
                        let (cw, ch) = measure_text(mem_dc, &item_text);

                        // 為第 1 順位繪製深藍膠囊襯底與亮青色字體
                        let pill_brush = CreateSolidBrush(COLOR_FIRST_BG);
                        let pill_pen = CreatePen(PS_SOLID, 1, COLOR_FIRST_CAND);
                        SelectObject(mem_dc, pill_brush);
                        SelectObject(mem_dc, pill_pen);
                        RoundRect(
                            mem_dc,
                            cur_x - 5,
                            cand_y - 2,
                            cur_x + cw + 7,
                            cand_y + ch + 2,
                            6,
                            6,
                        );
                        SelectObject(mem_dc, bg_brush);
                        SelectObject(mem_dc, border_pen);
                        DeleteObject(pill_brush);
                        DeleteObject(pill_pen);

                        SetTextColor(mem_dc, COLOR_FIRST_CAND);
                        draw_text_utf16(mem_dc, cur_x + 1, cand_y, &item_text);
                        cur_x += cw + 24;
                    } else {
                        SelectObject(mem_dc, font_cand_other);
                        let (cw, _) = measure_text(mem_dc, &item_text);
                        SetTextColor(mem_dc, COLOR_OTHER_CAND);
                        draw_text_utf16(mem_dc, cur_x, cand_y, &item_text);
                        cur_x += cw + 20;
                    }
                }
            }

            SelectObject(mem_dc, old_font);
            DeleteObject(font_top);
            DeleteObject(font_cand_first);
            DeleteObject(font_cand_other);
        }
    }

    // 將雙緩衝記憶體畫布一次性複製至螢幕 DC
    BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

    // 釋放所有 GDI 資源，杜絕 Handle 洩漏
    SelectObject(mem_dc, old_pen);
    SelectObject(mem_dc, old_brush);
    DeleteObject(border_pen);
    DeleteObject(bg_brush);

    SelectObject(mem_dc, old_bmp);
    DeleteObject(mem_bmp);
    DeleteDC(mem_dc);

    EndPaint(hwnd, &ps);
}

/// 浮動候選字視窗訊息處理常式 (`WndProc`)
unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => {
            // 絕對不搶奪前景輸入視窗焦點
            MA_NOACTIVATE as LRESULT
        }
        WM_ERASEBKGND => {
            // 由 WM_PAINT 雙緩衝完整繪製背景，避免閃爍
            1
        }
        WM_TIMER => {
            if wparam == TIMER_ID_TOAST {
                KillTimer(hwnd, TIMER_ID_TOAST);
                let is_toast = OVERLAY_CTX.with(|ctx| {
                    matches!(ctx.borrow().display, OverlayDisplay::ModeToast { .. })
                });
                if is_toast {
                    update_overlay(OverlayDisplay::Hidden);
                }
            }
            0
        }
        WM_PAINT => {
            paint_overlay(hwnd);
            0
        }
        WM_TRAYICON => {
            let tray_event = (lparam as u32) & 0xFFFF;
            match tray_event {
                WM_LBUTTONUP => {
                    crate::hook::toggle_ime_mode_from_ui();
                }
                WM_RBUTTONUP => {
                    let (mode, output_mode) = crate::hook::get_current_modes();
                    crate::tray::show_tray_context_menu(hwnd, mode, output_mode);
                }
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            let cmd_id = (wparam & 0xFFFF) as usize;
            match cmd_id {
                ID_TOGGLE_IME => {
                    crate::hook::toggle_ime_mode_from_ui();
                }
                ID_MODE_COLLOQUIAL => {
                    crate::hook::set_output_mode_from_ui(OutputMode::Colloquial);
                }
                ID_MODE_WRITTEN => {
                    crate::hook::set_output_mode_from_ui(OutputMode::Written);
                }
                ID_RELOAD_USER_DICT => {
                    crate::hook::reload_user_dict_from_ui();
                }
                ID_EXIT => {
                    crate::tray::remove_tray_icon();
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

