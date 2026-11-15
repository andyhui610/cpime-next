//! CPIME-Next Windows 桌面端廣東話雙軌輸入法前端 (`cpime-win`)
//!
//! 啟動後常駐於背景，透過 `WH_KEYBOARD_LL` 全域鍵盤攔截與無焦點 GDI 雙緩衝浮動視窗，
//! 讓使用者可直接在記事本 (Notepad)、瀏覽器 (Chrome / Edge)、VS Code 等任何 Windows 應用程式中
//! 流暢輸入廣東話（支援標準粵拼、教院/耶魯/劉錫祥/港式拼法、懶音容錯、首字母簡拼、打英文直出中文與智慧自造詞）。

pub mod hook;
pub mod injector;
pub mod overlay;
pub mod tray;

use std::env;
use std::mem::zeroed;
use std::path::PathBuf;
use std::ptr::null_mut;

use cpime_core::{CpimeEngine, OutputMode};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, SetProcessDPIAware, TranslateMessage, MSG,
};

use crate::hook::{install_keyboard_hook, uninstall_keyboard_hook};
use crate::overlay::{init_overlay_window, update_overlay, ImeMode, OverlayDisplay};
use crate::tray::{init_tray_icon, remove_tray_icon};

/// 自動解析系統詞庫 (`cpime_dict.db`) 路徑
fn resolve_sys_db_path() -> PathBuf {
    let default_path = CpimeEngine::default_db_path();
    if default_path.exists() {
        return default_path;
    }

    let candidates = [
        PathBuf::from("core/assets/cpime_dict.db"),
        PathBuf::from("../core/assets/cpime_dict.db"),
        PathBuf::from("cpime_dict.db"),
    ];
    for path in candidates {
        if path.exists() {
            return path;
        }
    }

    default_path
}

/// 自動解析個人學習詞庫 (`user_dict.db`) 路徑（優先使用專案目錄 `/core/assets/user_dict.db`，若無法存取則使用使用者目錄）
fn resolve_user_db_path() -> PathBuf {
    let default_user_path = CpimeEngine::default_user_db_path();
    if let Some(parent) = default_user_path.parent() {
        if parent.exists() {
            return default_user_path;
        }
    }

    if let Ok(profile) = env::var("USERPROFILE") {
        let user_dir = PathBuf::from(profile).join(".cpime");
        let _ = std::fs::create_dir_all(&user_dir);
        return user_dir.join("user_dict.db");
    }

    PathBuf::from("user_dict.db")
}

fn main() {
    unsafe {
        // 啟用高 DPI 感知，確保微軟正黑體在 4K / 縮放螢幕上清晰銳利且游標座標不偏移
        SetProcessDPIAware();
    }

    let sys_db_path = resolve_sys_db_path();
    let user_db_path = resolve_user_db_path();

    let engine = match CpimeEngine::open_with_user_dict(&sys_db_path, &user_db_path) {
        Ok(eng) => eng,
        Err(err) => {
            eprintln!(
                "[錯誤] 無法載入 CPIME 詞庫資料庫:\n  系統詞庫: {}\n  個人詞庫: {}\n  原因: {}",
                sys_db_path.display(),
                user_db_path.display(),
                err
            );
            std::process::exit(1);
        }
    };

    let overlay_hwnd = match init_overlay_window() {
        Ok(hwnd) => hwnd,
        Err(err) => {
            eprintln!("[錯誤] {}", err);
            std::process::exit(1);
        }
    };

    if let Err(err) = init_tray_icon(overlay_hwnd) {
        eprintln!("[錯誤] {}", err);
        std::process::exit(1);
    }

    let hhook = match install_keyboard_hook(engine) {
        Ok(h) => h,
        Err(err) => {
            remove_tray_icon();
            eprintln!("[錯誤] {}", err);
            std::process::exit(1);
        }
    };

    println!("======================================================================");
    println!("  CPIME-Next Windows 桌面端廣東話輸入法已啟動！");
    println!("  系統詞庫 : {}", sys_db_path.display());
    println!("  個人詞庫 : {}", user_db_path.display());
    println!("  當前狀態 : [粵·口語] 廣東話雙軌模式（粵拼 + 英文反查 + 智慧自造詞）");
    println!("----------------------------------------------------------------------");
    println!("  【按鍵與系統列操作指南】");
    println!("  • 中/英切換 : 單按 [Left Shift]、[Ctrl + Space] 或左鍵點擊右下角圖示");
    println!("  • 口語/書面 : 按 [Ctrl + Shift + S] 或 [F4] 秒速切換 [粵·口語] / [粵·書面]");
    println!("  • 右鍵選單  : 右鍵點擊系統列「粵 / 書 / 英」圖示開啟功能選單");
    println!("  • 空白鍵    : [Space] 將當頁第 1 順位候選詞上屏");
    println!("  • 數字選字  : [1] ~ [9] 選擇對應序號候選詞上屏");
    println!("  • 英文直出  : [Enter] 將目前輸入的英文字母直接上屏（如打英文單字）");
    println!("  • 候選翻頁  : [-] / [=] 或 [PageUp] / [PageDown] 切換上/下一頁");
    println!("  • 修正/取消 : [Backspace] 刪除拼音字元、[Esc] 清空當前輸入");
    println!("  • 結束程式  : 右鍵選單「結束 CPIME-Next」、[Ctrl + Alt + Q] 或終端機 [Ctrl + C]");
    println!("======================================================================\n");

    // 啟動時於游標旁短暫顯示 [粵·口語] 狀態提示
    update_overlay(OverlayDisplay::ModeToast {
        mode: ImeMode::Cantonese,
        output_mode: OutputMode::Colloquial,
    });

    // 進入 Win32 訊息迴圈（維持 WH_KEYBOARD_LL 攔截器、系統列圖示與無焦點浮動視窗運作）
    unsafe {
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    uninstall_keyboard_hook(hhook);
    remove_tray_icon();
}

