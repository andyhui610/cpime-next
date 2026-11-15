//! CPIME-Next Windows 桌面端廣東話雙軌輸入法前端 (`cpime-win`)
//!
//! 啟動後常駐於背景，透過 `WH_KEYBOARD_LL` 全域鍵盤攔截與無焦點 GDI 雙緩衝浮動視窗，
//! 讓使用者可直接在記事本 (Notepad)、瀏覽器 (Chrome / Edge)、VS Code 等任何 Windows 應用程式中
//! 流暢輸入廣東話（支援標準粵拼、教院/耶魯/劉錫祥/港式拼法、懶音容錯、首字母簡拼、打英文直出中文與智慧自造詞）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod hook;
pub mod injector;
pub mod overlay;
pub mod tray;

use std::env;
use std::mem::zeroed;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use cpime_core::{CpimeEngine, OutputMode};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, SetProcessDPIAware, TranslateMessage, MSG,
};

use crate::hook::{install_keyboard_hook, uninstall_keyboard_hook};
use crate::overlay::{init_overlay_window, update_overlay, ImeMode, OverlayDisplay};
use crate::tray::{init_tray_icon, remove_tray_icon};

/// 取得目前執行檔 (`.exe`) 所在的資料夾路徑
fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|parent| parent.to_path_buf()))
}

/// 依可攜式優先順序解析系統詞庫 (`cpime_dict.db`) 路徑
fn resolve_sys_db_path_from(
    exe_dir: Option<&Path>,
    cwd: Option<&Path>,
    fallback_default: PathBuf,
) -> PathBuf {
    // 1. 第一優先：與 .exe 同級的可攜式相對路徑 (assets/cpime_dict.db 或 ./cpime_dict.db)
    if let Some(dir) = exe_dir {
        let portable_candidates = [
            dir.join("assets").join("cpime_dict.db"),
            dir.join("cpime_dict.db"),
        ];
        for path in portable_candidates {
            if path.exists() {
                return path;
            }
        }
    }

    // 2. 第二優先：工作目錄相對路徑與開發環境 (cargo run) 回退路徑
    if let Some(wd) = cwd {
        let cwd_candidates = [
            wd.join("assets").join("cpime_dict.db"),
            wd.join("cpime_dict.db"),
            wd.join("core").join("assets").join("cpime_dict.db"),
            wd.join("..").join("core").join("assets").join("cpime_dict.db"),
        ];
        for path in cwd_candidates {
            if path.exists() {
                return path;
            }
        }
    }

    if fallback_default.exists() {
        return fallback_default;
    }

    fallback_default
}

/// 自動解析系統詞庫 (`cpime_dict.db`) 路徑
fn resolve_sys_db_path() -> PathBuf {
    resolve_sys_db_path_from(
        exe_dir().as_deref(),
        env::current_dir().ok().as_deref(),
        CpimeEngine::default_db_path(),
    )
}

/// 依可攜式優先順序解析個人學習詞庫 (`user_dict.db`) 路徑
fn resolve_user_db_path_from(exe_dir: Option<&Path>, default_user_path: PathBuf) -> PathBuf {
    // 1. 若處於可攜式發佈目錄（exe 同級已有 user_dict.db 或系統詞庫），直接在 exe 同目錄生成/讀取 user_dict.db
    if let Some(dir) = exe_dir {
        let portable_user_db = dir.join("user_dict.db");
        let has_portable_sys_db =
            dir.join("assets").join("cpime_dict.db").exists() || dir.join("cpime_dict.db").exists();
        if portable_user_db.exists() || has_portable_sys_db {
            return portable_user_db;
        }
    }

    // 2. 開發環境回退至專案目錄 /core/assets/user_dict.db
    if let Some(parent) = default_user_path.parent() {
        if parent.exists() {
            return default_user_path;
        }
    }

    // 3. 若無專案目錄則優先於 exe 同級目錄生成，最後才退至使用者設定檔目錄
    if let Some(dir) = exe_dir {
        return dir.join("user_dict.db");
    }

    if let Ok(profile) = env::var("USERPROFILE") {
        let user_dir = PathBuf::from(profile).join(".cpime");
        let _ = std::fs::create_dir_all(&user_dir);
        return user_dir.join("user_dict.db");
    }

    PathBuf::from("user_dict.db")
}

/// 自動解析個人學習詞庫 (`user_dict.db`) 路徑
fn resolve_user_db_path() -> PathBuf {
    resolve_user_db_path_from(exe_dir().as_deref(), CpimeEngine::default_user_db_path())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_portable_path_resolution_priority() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = env::temp_dir().join(format!("cpime_portable_test_{}_{}", std::process::id(), nanos));
        let portable_dir = base.join("portable");
        let portable_assets = portable_dir.join("assets");
        let empty_cwd = base.join("empty_cwd");
        let dev_assets = base.join("dev_core").join("assets");

        fs::create_dir_all(&portable_assets).unwrap();
        fs::create_dir_all(&empty_cwd).unwrap();
        fs::create_dir_all(&dev_assets).unwrap();

        let portable_assets_db = portable_assets.join("cpime_dict.db");
        let portable_root_db = portable_dir.join("cpime_dict.db");
        let dev_sys_db = dev_assets.join("cpime_dict.db");
        let dev_user_db = dev_assets.join("user_dict.db");

        // 同時建立開發環境 DB 與可攜式目錄 assets/cpime_dict.db，驗證可攜式路徑絕對優先
        fs::write(&dev_sys_db, b"dev").unwrap();
        fs::write(&portable_assets_db, b"portable_assets").unwrap();
        fs::write(&portable_root_db, b"portable_root").unwrap();

        assert_eq!(
            resolve_sys_db_path_from(Some(&portable_dir), Some(&empty_cwd), dev_sys_db.clone()),
            portable_assets_db
        );
        assert_eq!(
            resolve_user_db_path_from(Some(&portable_dir), dev_user_db.clone()),
            portable_dir.join("user_dict.db")
        );

        // 移除 assets/cpime_dict.db 後，應命中同級 ./cpime_dict.db
        fs::remove_file(&portable_assets_db).unwrap();
        assert_eq!(
            resolve_sys_db_path_from(Some(&portable_dir), Some(&empty_cwd), dev_sys_db.clone()),
            portable_root_db
        );
        assert_eq!(
            resolve_user_db_path_from(Some(&portable_dir), dev_user_db.clone()),
            portable_dir.join("user_dict.db")
        );

        // 若執行檔目錄內皆無詞庫（如 cargo run 在 target/debug 執行），應回退至開發環境路徑
        fs::remove_file(&portable_root_db).unwrap();
        assert_eq!(
            resolve_sys_db_path_from(Some(&portable_dir), Some(&empty_cwd), dev_sys_db.clone()),
            dev_sys_db
        );
        assert_eq!(
            resolve_user_db_path_from(Some(&portable_dir), dev_user_db.clone()),
            dev_user_db
        );

        let _ = fs::remove_dir_all(&base);
    }
}

