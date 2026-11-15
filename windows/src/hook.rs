//! CPIME-Next Windows 低階鍵盤攔截器與輸入法狀態機 (`hook.rs`)
//!
//! 透過 `SetWindowsHookExW(WH_KEYBOARD_LL, ...)` 實作全域鍵盤狀態機：
//! 1. 檢查 `dwExtraInfo == CPIME_EXTRA_INFO (0x4350494D)` 忽略自發送事件，杜絕自我攔截迴圈。
//! 2. 支援 `Ctrl + Space` 或單按 `Left Shift`（按下至放開期間無其他按鍵）切換「粵語模式 `[粵]`」與「英文模式 `[英]`」，
//!    切換時在游標旁短暫顯示狀態提示。
//! 3. 在「粵語模式」下攔截 `a-z` 送入 `CpimeEngine` 查詢，並支援 `Space`（第 1 候選詞上屏）、
//!    `1`~`9`（第 N 候選詞上屏）、`Enter`（原始英文字母直出上屏）、`Backspace`、`Escape`、
//!    以及 `-` / `=` / `PageUp` / `PageDown` 候選詞翻頁。

use std::cell::RefCell;
use std::ptr::null_mut;

use cpime_core::{Candidate, CpimeEngine, OutputMode};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_BACK, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_F4, VK_LEFT, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_NEXT, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7,
    VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_PRIOR, VK_RETURN, VK_RIGHT,
    VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostQuitMessage, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::injector::{send_unicode_string, CPIME_EXTRA_INFO};
use crate::overlay::{update_overlay, ImeMode, OverlayDisplay};
use crate::tray::{remove_tray_icon, update_tray_icon};

/// 每頁顯示候選詞數量（5~9 個候選詞規範，設定為 9 讓數字鍵 1~9 完整對應當頁每一項）
pub const PAGE_SIZE: usize = 9;

/// 單次查詢向 `CpimeEngine` 取回的最大候選詞池大小（支援多頁翻頁）
const MAX_CANDIDATE_POOL: usize = 54;

/// 低階鍵盤攔截器內部狀態機
pub struct ImeHookState {
    pub engine: CpimeEngine,
    pub mode: ImeMode,
    pub input_buffer: String,
    pub candidates: Vec<Candidate>,
    pub current_page: usize,
    /// 追蹤 `Left Shift` 是否正被按下
    lshift_down: bool,
    /// 追蹤 `Left Shift` 按下期間是否未曾觸發任何其他按鍵
    lshift_pure: bool,
    /// 記錄哪些虛擬鍵碼 (`0..256`) 的 `WM_KEYDOWN` 已被本輸入法攔截，以便同步吞掉對應的 `WM_KEYUP`
    swallowed_keys: [bool; 256],
}

impl ImeHookState {
    pub fn new(engine: CpimeEngine) -> Self {
        Self {
            engine,
            mode: ImeMode::Cantonese,
            input_buffer: String::with_capacity(32),
            candidates: Vec::new(),
            current_page: 0,
            lshift_down: false,
            lshift_pure: false,
            swallowed_keys: [false; 256],
        }
    }

    /// 重新向 `CpimeEngine` 查詢候選詞清單
    fn refresh_candidates(&mut self) {
        if self.input_buffer.is_empty() {
            self.candidates.clear();
            self.current_page = 0;
        } else {
            self.candidates = self
                .engine
                .lookup_with_limit(&self.input_buffer, MAX_CANDIDATE_POOL);
            let total = self.total_pages();
            if self.current_page >= total {
                self.current_page = 0;
            }
        }
    }

    /// 計算目前候選詞總頁數
    fn total_pages(&self) -> usize {
        if self.candidates.is_empty() {
            1
        } else {
            (self.candidates.len() + PAGE_SIZE - 1) / PAGE_SIZE
        }
    }

    /// 取得當前分頁應顯示的 `OverlayDisplay::Composing` 快照
    fn build_composing_display(&self) -> OverlayDisplay {
        let start = self.current_page * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(self.candidates.len());
        let page_candidates = if start < end {
            self.candidates[start..end].to_vec()
        } else {
            Vec::new()
        };

        OverlayDisplay::Composing {
            mode: self.mode,
            output_mode: self.engine.get_output_mode(),
            input_buffer: self.input_buffer.clone(),
            page_candidates,
            current_page: self.current_page,
            total_pages: self.total_pages(),
        }
    }

    /// 切換「口語模式」與「書面語模式」（若正在拼字中則即時重新查詢候選詞）
    fn toggle_output_mode(&mut self) -> OutputMode {
        let next = match self.engine.get_output_mode() {
            OutputMode::Colloquial => OutputMode::Written,
            OutputMode::Written => OutputMode::Colloquial,
        };
        self.engine.set_output_mode(next);
        if !self.input_buffer.is_empty() {
            self.refresh_candidates();
        }
        next
    }

    /// 選取全域索引 `global_idx` 的候選詞並觸發個人詞庫學習與自造詞滑動視窗更新（支援步進式部分選字）
    fn commit_candidate_at(&mut self, global_idx: usize) -> Option<(String, String)> {
        if global_idx >= self.candidates.len() {
            return None;
        }
        let raw_input = self.input_buffer.clone();
        let consumed = self.candidates[global_idx]
            .consumed_chars
            .min(raw_input.len());
        let chosen = self
            .engine
            .commit_candidate(&raw_input, global_idx)
            .or_else(|| Some(self.candidates[global_idx].word.clone()))?;

        let mut log_str = format!("  [上屏] \"{}\" -> 「{}」", raw_input, chosen);
        for ev in &self.engine.phrase_composer.last_events {
            if ev.promoted {
                log_str.push_str(&format!(
                    " | ✨ 已學會新詞：{} ({} / {})",
                    ev.zh_phrase, ev.toneless, ev.abbr
                ));
            } else {
                log_str.push_str(&format!(
                    " | 👀 自造詞觀察中 ({}/2)：{} ({} / {})",
                    ev.freq, ev.zh_phrase, ev.toneless, ev.abbr
                ));
            }
        }

        if consumed > 0
            && consumed < self.input_buffer.len()
            && self.input_buffer.is_char_boundary(consumed)
        {
            // 步進式部分選字 (Partial Commit)：僅切除開頭已消耗字元，保留剩餘拼音並立即重新解碼
            self.input_buffer.drain(..consumed);
            self.current_page = 0;
            self.refresh_candidates();
            log_str.push_str(&format!(" | [剩餘拼音] \"{}\"", self.input_buffer));
        } else {
            self.input_buffer.clear();
            self.candidates.clear();
            self.current_page = 0;
        }

        Some((chosen, log_str))
    }

    /// 切換「粵語模式 `[粵]`」與「英文模式 `[英]`」
    fn toggle_mode(&mut self) -> ImeMode {
        self.mode = match self.mode {
            ImeMode::Cantonese => ImeMode::English,
            ImeMode::English => ImeMode::Cantonese,
        };
        self.input_buffer.clear();
        self.candidates.clear();
        self.current_page = 0;
        self.engine.clear_context();
        self.mode
    }
}

/// 鍵盤事件處理決策（確保在釋放 `RefCell` 借用後才呼叫 `SendInput` 與 `update_overlay`，徹底杜絕重入衝突）
enum HookAction {
    /// 放行按鍵給目標視窗
    PassThrough,
    /// 放行按鍵給目標視窗，但同時更新浮動視窗與系統列圖示（例如單按 `Left Shift` 放開切換模式時）
    PassThroughWithSideEffect {
        overlay: Option<OverlayDisplay>,
        tray_state: Option<(ImeMode, OutputMode)>,
        log_msg: Option<String>,
    },
    /// 攔截按鍵（回傳 `1` 不傳給目標視窗），並執行上屏或重繪浮動視窗 / 系統列圖示
    Intercept {
        overlay: Option<OverlayDisplay>,
        tray_state: Option<(ImeMode, OutputMode)>,
        commit_text: Option<String>,
        log_msg: Option<String>,
    },
}

thread_local! {
    static HOOK_STATE: RefCell<Option<ImeHookState>> = const { RefCell::new(None) };
}

/// 取得目前的輸入模式與口語/書面語輸出模式（供系統列右鍵選單使用）
pub fn get_current_modes() -> (ImeMode, OutputMode) {
    HOOK_STATE.with(|cell| {
        let borrow = cell.borrow();
        if let Some(st) = borrow.as_ref() {
            (st.mode, st.engine.get_output_mode())
        } else {
            (ImeMode::Cantonese, OutputMode::Colloquial)
        }
    })
}

/// 供系統列左鍵或右鍵選單呼叫：切換「粵語模式」與「英文模式」
pub fn toggle_ime_mode_from_ui() {
    let result = HOOK_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let state = borrow.as_mut()?;
        let new_mode = state.toggle_mode();
        let output_mode = state.engine.get_output_mode();
        Some((new_mode, output_mode))
    });

    if let Some((new_mode, output_mode)) = result {
        println!(
            "[模式切換] 系統列 -> {}",
            new_mode.toast_label(output_mode)
        );
        update_tray_icon(new_mode, output_mode);
        update_overlay(OverlayDisplay::ModeToast {
            mode: new_mode,
            output_mode,
        });
    }
}

/// 供系統列右鍵選單呼叫：指定切換至「口語模式」或「書面語模式」
pub fn set_output_mode_from_ui(output_mode: OutputMode) {
    let result = HOOK_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let state = borrow.as_mut()?;
        state.mode = ImeMode::Cantonese;
        state.engine.set_output_mode(output_mode);
        let overlay = if state.input_buffer.is_empty() {
            OverlayDisplay::ModeToast {
                mode: state.mode,
                output_mode,
            }
        } else {
            state.refresh_candidates();
            state.build_composing_display()
        };
        Some((state.mode, output_mode, overlay))
    });

    if let Some((mode, out_mode, overlay)) = result {
        println!("[語體切換] 系統列 -> {}", mode.toast_label(out_mode));
        update_tray_icon(mode, out_mode);
        update_overlay(overlay);
    }
}

/// 供系統列右鍵選單呼叫：重新載入個人詞庫 (`user_dict.db`)
pub fn reload_user_dict_from_ui() {
    let result = HOOK_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let state = borrow.as_mut()?;
        let res = state.engine.reload_user_dict();
        let overlay = if res.is_ok() && !state.input_buffer.is_empty() {
            state.refresh_candidates();
            Some(state.build_composing_display())
        } else {
            None
        };
        Some((res, overlay))
    });

    if let Some((res, overlay)) = result {
        match res {
            Ok(()) => {
                println!("[個人詞庫] 已重新載入 user_dict.db");
                if let Some(ov) = overlay {
                    update_overlay(ov);
                }
            }
            Err(err) => {
                eprintln!("[錯誤] 重新載入個人詞庫失敗: {}", err);
            }
        }
    }
}

/// 檢查指定的虛擬鍵碼目前是否處於按下狀態
#[inline]
unsafe fn is_vk_down(vk: u16) -> bool {
    (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0
}

/// 安裝 `WH_KEYBOARD_LL` 全域低階鍵盤攔截器
pub fn install_keyboard_hook(engine: CpimeEngine) -> Result<HHOOK, String> {
    HOOK_STATE.with(|cell| {
        *cell.borrow_mut() = Some(ImeHookState::new(engine));
    });

    unsafe {
        let hinstance = GetModuleHandleW( std::ptr::null());
        let hhook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            hinstance,
            0,
        );
        if hhook.is_null() {
            return Err("安裝 WH_KEYBOARD_LL 低階鍵盤攔截器 (SetWindowsHookExW) 失敗".to_string());
        }
        Ok(hhook)
    }
}

/// 卸載 `WH_KEYBOARD_LL` 攔截器
pub fn uninstall_keyboard_hook(hhook: HHOOK) {
    if !hhook.is_null() {
        unsafe {
            UnhookWindowsHookEx(hhook);
        }
    }
}

/// `WH_KEYBOARD_LL` 全域鍵盤事件回呼常式
unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if n_code != HC_ACTION as i32 || lparam == 0 {
        return CallNextHookEx(null_mut(), n_code, wparam, lparam);
    }

    let kb = &*(lparam as *const KBDLLHOOKSTRUCT);

    // 1. 忽略 dwExtraInfo == 0x4350494D ("CPIM") 的自發送 Unicode 上屏事件
    if kb.dwExtraInfo == CPIME_EXTRA_INFO {
        return CallNextHookEx(null_mut(), n_code, wparam, lparam);
    }

    let msg = wparam as u32;
    let vk = kb.vkCode;
    let is_key_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let is_key_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

    if !is_key_down && !is_key_up {
        return CallNextHookEx(null_mut(), n_code, wparam, lparam);
    }

    let ctrl_down = is_vk_down(VK_CONTROL);
    let alt_down = is_vk_down(VK_MENU);
    let win_down = is_vk_down(VK_LWIN) || is_vk_down(VK_RWIN);
    let shift_down = is_vk_down(VK_SHIFT) || is_vk_down(VK_LSHIFT);

    // 安全退出快捷鍵：Ctrl + Alt + Q 結束輸入法常駐程序
    if is_key_down && ctrl_down && alt_down && vk == 0x51 {
        println!("\n[結束] 偵測到 Ctrl + Alt + Q，正在關閉 CPIME-Next Windows 輸入法...");
        remove_tray_icon();
        PostQuitMessage(0);
        return 1;
    }

    let action = HOOK_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let Some(state) = borrow.as_mut() else {
            return HookAction::PassThrough;
        };

        // ── A. 處理按鍵放開事件 (WM_KEYUP / WM_SYSKEYUP) ──────────────────────
        if is_key_up {
            // 單按 Left Shift 放開檢測：按下到放開期間無任何其他按鍵則切換 [粵] / [英] 模式
            if vk == VK_LSHIFT as u32 {
                let should_toggle =
                    state.lshift_down && state.lshift_pure && !ctrl_down && !alt_down && !win_down;
                state.lshift_down = false;
                state.lshift_pure = false;

                if should_toggle {
                    let new_mode = state.toggle_mode();
                    let output_mode = state.engine.get_output_mode();
                    return HookAction::PassThroughWithSideEffect {
                        overlay: Some(OverlayDisplay::ModeToast {
                            mode: new_mode,
                            output_mode,
                        }),
                        tray_state: Some((new_mode, output_mode)),
                        log_msg: Some(format!(
                            "[模式切換] Left Shift -> {}",
                            new_mode.toast_label(output_mode)
                        )),
                    };
                }
                return HookAction::PassThrough;
            }

            // 若該按鍵在 WM_KEYDOWN 時已被本輸入法攔截，則同步吞掉其 WM_KEYUP
            if (vk as usize) < 256 && state.swallowed_keys[vk as usize] {
                state.swallowed_keys[vk as usize] = false;
                return HookAction::Intercept {
                    overlay: None,
                    tray_state: None,
                    commit_text: None,
                    log_msg: None,
                };
            }

            return HookAction::PassThrough;
        }

        // ── B. 處理按鍵按下事件 (WM_KEYDOWN / WM_SYSKEYDOWN) ─────────────────
        // 1. 追蹤單按 Left Shift 狀態
        if vk == VK_LSHIFT as u32 {
            if !state.lshift_down {
                state.lshift_down = true;
                state.lshift_pure = !ctrl_down && !alt_down && !win_down;
            }
            return HookAction::PassThrough;
        } else if vk != VK_SHIFT as u32 {
            // 只要在 Left Shift 按住期間按下了任何其他鍵，就取消單按切換資格
            state.lshift_pure = false;
        }

        // 2. 支援 Ctrl + Space 切換「粵語模式 [粵]」與「英文模式 [英]」
        if vk == VK_SPACE as u32 && ctrl_down && !alt_down && !win_down {
            let new_mode = state.toggle_mode();
            let output_mode = state.engine.get_output_mode();
            if (vk as usize) < 256 {
                state.swallowed_keys[vk as usize] = true;
            }
            return HookAction::Intercept {
                overlay: Some(OverlayDisplay::ModeToast {
                    mode: new_mode,
                    output_mode,
                }),
                tray_state: Some((new_mode, output_mode)),
                commit_text: None,
                log_msg: Some(format!(
                    "[模式切換] Ctrl+Space -> {}",
                    new_mode.toast_label(output_mode)
                )),
            };
        }

        // 2b. 支援 Ctrl + Shift + S 或 F4 秒速切換「口語模式 (粵)」與「書面語模式 (書)」
        let is_output_toggle_hotkey = (ctrl_down && shift_down && !alt_down && !win_down && vk == 0x53)
            || (!ctrl_down && !alt_down && !win_down && !shift_down && vk == VK_F4 as u32);
        if state.mode == ImeMode::Cantonese && is_output_toggle_hotkey {
            let new_output_mode = state.toggle_output_mode();
            if (vk as usize) < 256 {
                state.swallowed_keys[vk as usize] = true;
            }
            let overlay = if state.input_buffer.is_empty() {
                OverlayDisplay::ModeToast {
                    mode: state.mode,
                    output_mode: new_output_mode,
                }
            } else {
                state.build_composing_display()
            };
            return HookAction::Intercept {
                overlay: Some(overlay),
                tray_state: Some((state.mode, new_output_mode)),
                commit_text: None,
                log_msg: Some(format!(
                    "[語體切換] 快捷鍵 -> {}",
                    state.mode.toast_label(new_output_mode)
                )),
            };
        }

        // 3. 若處於「英文模式 [英]」，所有按鍵直接放行
        if state.mode == ImeMode::English {
            return HookAction::PassThrough;
        }

        // 4. 在「粵語模式 [粵]」下：若同時按住 Ctrl、Alt 或 Win 鍵，直接放行以保留系統快捷鍵 (Ctrl+C, Ctrl+V 等)
        if ctrl_down || alt_down || win_down {
            return HookAction::PassThrough;
        }

        // 5. 單純按下 a-z 時攔截按鍵，將字元傳入 Engine 並觸發浮動視窗重繪
        if (0x41..=0x5A).contains(&vk) {
            // 若目前無拼音且使用者按住 Shift + A-Z，允許直接輸入大寫英文字母
            if shift_down && state.input_buffer.is_empty() {
                state.engine.break_sequence();
                return HookAction::PassThrough;
            }

            let ch = (vk as u8 as char).to_ascii_lowercase();
            state.input_buffer.push(ch);
            state.current_page = 0;
            state.refresh_candidates();
            if (vk as usize) < 256 {
                state.swallowed_keys[vk as usize] = true;
            }

            return HookAction::Intercept {
                overlay: Some(state.build_composing_display()),
                tray_state: None,
                commit_text: None,
                log_msg: None,
            };
        }

        // 6. 當 input_buffer 有內容時，攔截並處理選字、翻頁、修改與取消按鍵
        if !state.input_buffer.is_empty() {
            let mark_swallowed = |st: &mut ImeHookState| {
                if (vk as usize) < 256 {
                    st.swallowed_keys[vk as usize] = true;
                }
            };

            let vkey = vk as u16;
            match vkey {
                // Space：選當頁第 1 個候選詞呼叫 send_unicode_string 上屏
                VK_SPACE => {
                    mark_swallowed(state);
                    let first_idx = state.current_page * PAGE_SIZE;
                    if let Some((word, log_str)) = state.commit_candidate_at(first_idx) {
                        let next_overlay = if state.input_buffer.is_empty() {
                            OverlayDisplay::Hidden
                        } else {
                            state.build_composing_display()
                        };
                        return HookAction::Intercept {
                            overlay: Some(next_overlay),
                            tray_state: None,
                            commit_text: Some(word),
                            log_msg: Some(log_str),
                        };
                    } else {
                        // 若無匹配候選詞，按空白鍵直接將原始英文上屏
                        let raw = std::mem::take(&mut state.input_buffer);
                        state.candidates.clear();
                        state.current_page = 0;
                        state.engine.break_sequence();
                        return HookAction::Intercept {
                            overlay: Some(OverlayDisplay::Hidden),
                            tray_state: None,
                            commit_text: Some(raw),
                            log_msg: None,
                        };
                    }
                }

                // 1~9：選當頁第 N 個候選詞上屏
                0x31..=0x39 if !shift_down => {
                    mark_swallowed(state);
                    let offset = (vkey - 0x31) as usize;
                    if offset < PAGE_SIZE {
                        let target_idx = state.current_page * PAGE_SIZE + offset;
                        if let Some((word, log_str)) = state.commit_candidate_at(target_idx) {
                            let next_overlay = if state.input_buffer.is_empty() {
                                OverlayDisplay::Hidden
                            } else {
                                state.build_composing_display()
                            };
                            return HookAction::Intercept {
                                overlay: Some(next_overlay),
                                tray_state: None,
                                commit_text: Some(word),
                                log_msg: Some(log_str),
                            };
                        }
                    }
                    // 若該序號無候選詞，仍攔截數字鍵避免誤打入目標視窗
                    return HookAction::Intercept {
                        overlay: None,
                        tray_state: None,
                        commit_text: None,
                        log_msg: None,
                    };
                }

                // Enter：將 input_buffer 原始英文字母直接透過 send_unicode_string 上屏
                VK_RETURN => {
                    mark_swallowed(state);
                    let raw = std::mem::take(&mut state.input_buffer);
                    state.candidates.clear();
                    state.current_page = 0;
                    state.engine.break_sequence();
                    let log_str = format!("  [英文直出] \"{}\"", raw);
                    return HookAction::Intercept {
                        overlay: Some(OverlayDisplay::Hidden),
                        tray_state: None,
                        commit_text: Some(raw),
                        log_msg: Some(log_str),
                    };
                }

                // Backspace：刪除最後一個拼音字元；若刪空則隱藏浮動視窗
                VK_BACK => {
                    mark_swallowed(state);
                    state.input_buffer.pop();
                    state.current_page = 0;
                    state.engine.break_sequence();

                    if state.input_buffer.is_empty() {
                        state.candidates.clear();
                        return HookAction::Intercept {
                            overlay: Some(OverlayDisplay::Hidden),
                            tray_state: None,
                            commit_text: None,
                            log_msg: None,
                        };
                    } else {
                        state.refresh_candidates();
                        return HookAction::Intercept {
                            overlay: Some(state.build_composing_display()),
                            tray_state: None,
                            commit_text: None,
                            log_msg: None,
                        };
                    }
                }

                // Escape：清空當前拼音並隱藏視窗
                VK_ESCAPE => {
                    mark_swallowed(state);
                    state.input_buffer.clear();
                    state.candidates.clear();
                    state.current_page = 0;
                    state.engine.break_sequence();
                    return HookAction::Intercept {
                        overlay: Some(OverlayDisplay::Hidden),
                        tray_state: None,
                        commit_text: None,
                        log_msg: None,
                    };
                }

                // '-' 或 PageUp 或 上方向鍵：上一頁
                VK_OEM_MINUS | VK_PRIOR | VK_UP => {
                    mark_swallowed(state);
                    if state.current_page > 0 {
                        state.current_page -= 1;
                    }
                    return HookAction::Intercept {
                        overlay: Some(state.build_composing_display()),
                        tray_state: None,
                        commit_text: None,
                        log_msg: None,
                    };
                }

                // '=' 或 PageDown 或 下方向鍵：下一頁
                VK_OEM_PLUS | VK_NEXT | VK_DOWN => {
                    mark_swallowed(state);
                    let total = state.total_pages();
                    if state.current_page + 1 < total {
                        state.current_page += 1;
                    }
                    return HookAction::Intercept {
                        overlay: Some(state.build_composing_display()),
                        tray_state: None,
                        commit_text: None,
                        log_msg: None,
                    };
                }

                _ => {}
            }
        } else {
            // 7. 當 input_buffer 為空時，所有按鍵放行給目標視窗，並依按鍵類型維護自造詞邊界與上下文
            let vkey = vk as u16;
            match vkey {
                VK_BACK => {
                    state.engine.break_sequence();
                }
                VK_RETURN | VK_ESCAPE | VK_TAB | VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN => {
                    state.engine.clear_context();
                }
                VK_SPACE
                | 0x30..=0x39
                | VK_OEM_1
                | VK_OEM_PLUS
                | VK_OEM_COMMA
                | VK_OEM_MINUS
                | VK_OEM_PERIOD
                | VK_OEM_2
                | VK_OEM_3
                | VK_OEM_4
                | VK_OEM_5
                | VK_OEM_6
                | VK_OEM_7 => {
                    let _ = state.engine.flush_custom_phrases();
                }
                _ => {}
            }
        }

        HookAction::PassThrough
    });

    // 在釋放 HOOK_STATE 借用後執行 UI 更新、系統列圖示切換與 SendInput 上屏，確保絕對不會發生重入借用衝突
    match action {
        HookAction::PassThrough => CallNextHookEx(null_mut(), n_code, wparam, lparam),
        HookAction::PassThroughWithSideEffect {
            overlay,
            tray_state,
            log_msg,
        } => {
            if let Some(msg) = log_msg {
                println!("{}", msg);
            }
            if let Some((mode, out_mode)) = tray_state {
                update_tray_icon(mode, out_mode);
            }
            if let Some(ov) = overlay {
                update_overlay(ov);
            }
            CallNextHookEx(null_mut(), n_code, wparam, lparam)
        }
        HookAction::Intercept {
            overlay,
            tray_state,
            commit_text,
            log_msg,
        } => {
            if let Some((mode, out_mode)) = tray_state {
                update_tray_icon(mode, out_mode);
            }
            if let Some(ov) = overlay {
                update_overlay(ov);
            }
            if let Some(ref text) = commit_text {
                send_unicode_string(text);
            }
            if let Some(msg) = log_msg {
                println!("{}", msg);
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_state_lookup_paging_and_mode_toggle() {
        let engine = CpimeEngine::open_default().expect("應能開啟預設種子資料庫");
        let mut state = ImeHookState::new(engine);

        assert_eq!(state.mode, ImeMode::Cantonese);
        assert_eq!(state.engine.get_output_mode(), OutputMode::Colloquial);

        // 1. 模擬輸入 "sun"
        for ch in "sun".chars() {
            state.input_buffer.push(ch);
        }
        state.refresh_candidates();
        assert!(state.candidates.len() >= 4);
        assert_eq!(state.candidates[0].word, "新");
        assert_eq!(state.candidates[1].word, "身");
        assert_eq!(state.candidates[2].word, "日");
        assert_eq!(state.candidates[3].word, "太陽");

        // 2. 模擬選取第 1 個候選詞上屏
        let (committed, _) = state
            .commit_candidate_at(0)
            .expect("應成功選取第 1 個候選詞");
        assert_eq!(committed, "新");
        assert!(state.input_buffer.is_empty());
        assert!(state.candidates.is_empty());

        // 3. 模擬切換中/英模式
        let new_mode = state.toggle_mode();
        assert_eq!(new_mode, ImeMode::English);
        let back_mode = state.toggle_mode();
        assert_eq!(back_mode, ImeMode::Cantonese);

        // 4. 模擬拼字中途切換口語/書面語模式
        state.input_buffer = "keoidei".to_string();
        state.refresh_candidates();
        assert_eq!(state.candidates[0].word, "佢哋");

        let out_written = state.toggle_output_mode();
        assert_eq!(out_written, OutputMode::Written);
        assert_eq!(state.candidates[0].word, "他們");
    }
}

