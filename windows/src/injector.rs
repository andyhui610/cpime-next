//! CPIME-Next Windows Unicode 字串上屏器 (`injector.rs`)
//!
//! 負責將選中的中文字詞或原始英文字串透過 Win32 `SendInput` (`KEYEVENTF_UNICODE`)
//! 批次注入目標視窗。完整支援 UTF-16 Surrogate Pairs（代理對），確保「𨋢」、「𥄫」、「㗎」等
//! UTF-8 四位元組香港口語擴展漢字在記事本、Chrome、Edge、VS Code 等所有視窗完美上屏。

use std::mem::size_of;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
};

/// 專屬自發送魔術標記 (`"CPIM"` = `0x4350494D`)
/// 當 `WH_KEYBOARD_LL` 攔截器檢查到 `dwExtraInfo == CPIME_EXTRA_INFO` 時，
/// 會立即放行，避免自我攔截造成無窮迴圈。
pub const CPIME_EXTRA_INFO: usize = 0x4350494D;

/// 將任意 UTF-8 字串轉換為對應的 `INPUT_KEYBOARD` (`KEYEVENTF_UNICODE`) 事件序列
pub fn build_unicode_inputs(text: &str) -> Vec<INPUT> {
    let utf16_units: Vec<u16> = text.encode_utf16().collect();
    let mut inputs = Vec::with_capacity(utf16_units.len() * 2);

    for unit in utf16_units {
        // 1. 按下事件 (KEYEVENTF_UNICODE)
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: CPIME_EXTRA_INFO,
                },
            },
        });

        // 2. 放開事件 (KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: CPIME_EXTRA_INFO,
                },
            },
        });
    }

    inputs
}

/// 透過 `SendInput` 將 UTF-8 字串以 `KEYEVENTF_UNICODE` 批次送入目標視窗
/// 回傳實際成功送出的 `INPUT` 事件數量。
pub fn send_unicode_string(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }

    let inputs = build_unicode_inputs(text);
    if inputs.is_empty() {
        return 0;
    }

    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surrogate_pair_hk_char_lift() {
        // 「𨋢」(U+282E2) 為 CJK 擴展區 B 四位元組漢字，UTF-16 需拆為 Surrogate Pair (2 個 u16)
        let inputs = build_unicode_inputs("𨋢");
        assert_eq!(
            inputs.len(),
            4,
            "單一四位元組漢字『𨋢』應產生 2 個 u16 × 2 (Down/Up) = 4 個 INPUT 事件"
        );
        unsafe {
            assert_eq!(inputs[0].Anonymous.ki.wVk, 0);
            assert_eq!(inputs[0].Anonymous.ki.wScan, 0xD860);
            assert_eq!(inputs[0].Anonymous.ki.dwFlags, KEYEVENTF_UNICODE);
            assert_eq!(inputs[0].Anonymous.ki.dwExtraInfo, CPIME_EXTRA_INFO);

            assert_eq!(inputs[2].Anonymous.ki.wVk, 0);
            assert_eq!(inputs[2].Anonymous.ki.wScan, 0xDEE2);
            assert_eq!(inputs[2].Anonymous.ki.dwFlags, KEYEVENTF_UNICODE);
            assert_eq!(inputs[2].Anonymous.ki.dwExtraInfo, CPIME_EXTRA_INFO);
        }
    }
}

