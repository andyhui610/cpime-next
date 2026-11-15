//! CPIME-Next 個人詞頻記憶 (`user_unigrams`)、前後字連帶 (`user_bigrams`) 與兩階段自造詞模組
//!
//! 負責管理獨立的 `user_dict.db` SQLite 資料庫，支援：
//! 1. Unigram 個人選字詞頻與最後使用時間記憶 (`user_unigrams`)
//! 2. Bigram 前後字上下文連帶記憶 (`user_bigrams`)
//! 3. 兩階段自造詞觀察與轉正 (`observe_custom_phrase`)，同時支援無聲調全拼 (`input_code`) 與首字母簡拼 (`abbr_code`)
//! 4. 誤選記憶詞條刪除（後悔刪除機制）

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OpenFlags, Result as SqlResult};

use crate::phonetics::{expand_phonetic_queries, sanitize_input};

/// 單調遞增的時間戳產生器，確保即使在同一微秒內連續選字，`last_used` 亦嚴格遞增
static MONOTONIC_TS: AtomicI64 = AtomicI64::new(0);

fn next_timestamp() -> i64 {
    let now_micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(1);

    let mut current = MONOTONIC_TS.load(Ordering::Relaxed);
    loop {
        let next = if now_micros > current {
            now_micros
        } else {
            current + 1
        };
        match MONOTONIC_TS.compare_exchange_weak(
            current,
            next,
            Ordering::SeqCst,
            Ordering::Relaxed,
        ) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}

/// 個人 Unigram 詞頻與自造詞記憶紀錄
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserUnigramEntry {
    pub input_code: String,
    pub abbr_code: String,
    pub zh_word: String,
    pub freq: i32,
    pub last_used: i64,
    pub is_custom: bool,
}

/// 個人 Bigram 前後字語境記憶紀錄
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserBigramEntry {
    pub prev_word: String,
    pub next_code: String,
    pub zh_word: String,
    pub freq: i32,
    pub last_used: i64,
}

/// 個人詞庫與語境記憶管理器 (`user_dict.db`)
pub struct UserDict {
    conn: Connection,
    /// 唯讀系統詞庫連線 (`cpime_dict.db`)，用於檢查詞組是否已存在於 `cantonese_dict` 及反查單字標準粵拼
    sys_conn: Option<Connection>,
    /// 個人詞庫檔案路徑（若為記憶體模式則為 `":memory:"`）
    db_path: PathBuf,
}

impl UserDict {
    /// 初始化或開啟指定的 `user_dict.db` 檔案（支援傳入檔案路徑，測試時亦支援 `":memory:"`）
    pub fn open<P: AsRef<Path>>(path: P) -> SqlResult<Self> {
        let path_ref = path.as_ref();
        let conn = if path_ref.to_str() == Some(":memory:") {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = path_ref.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            Connection::open(path_ref)?
        };

        let default_sys_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("cpime_dict.db");
        let sys_conn = Self::open_readonly_sys_conn(&default_sys_path).ok();

        let dict = Self {
            conn,
            sys_conn,
            db_path: path_ref.to_path_buf(),
        };
        dict.init_schema()?;
        Ok(dict)
    }

    /// 重新載入個人詞庫連線 (`user_dict.db`)，同步外部寫入或 WAL 變更
    pub fn reload(&mut self) -> SqlResult<()> {
        if self.db_path.to_str() == Some(":memory:") {
            return Ok(());
        }
        let new_conn = Connection::open(&self.db_path)?;
        self.conn = new_conn;
        self.init_schema()
    }

    /// 開啟記憶體內 (`:memory:`) 的個人詞庫（適合單元測試與無痕模式）
    pub fn open_in_memory() -> SqlResult<Self> {
        Self::open(":memory:")
    }

    /// 綁定指定的唯讀系統詞庫路徑 (`cpime_dict.db`)
    pub fn set_sys_dict_path<P: AsRef<Path>>(&mut self, sys_db_path: P) {
        if let Ok(c) = Self::open_readonly_sys_conn(sys_db_path.as_ref()) {
            self.sys_conn = Some(c);
        }
    }

    fn open_readonly_sys_conn(path_ref: &Path) -> SqlResult<Connection> {
        let canonical = path_ref
            .canonicalize()
            .unwrap_or_else(|_| path_ref.to_path_buf());
        let mut path_str = canonical.to_string_lossy().replace('\\', "/");
        if let Some(stripped) = path_str.strip_prefix("//?/") {
            path_str = stripped.to_string();
        }
        let uri = format!("file:///{}?immutable=1", path_str.trim_start_matches('/'));
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;

        Connection::open_with_flags(&uri, flags).or_else(|_| {
            Connection::open_with_flags(
                path_ref,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
        })
    }

    /// 啟用 WAL 模式並自動建立 `user_unigrams`（含 `abbr_code` 與 `is_custom`）、`user_bigrams` 資料表與查詢索引
    fn init_schema(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;

             CREATE TABLE IF NOT EXISTS user_unigrams (
                 input_code TEXT,
                 abbr_code TEXT DEFAULT '',
                 zh_word TEXT,
                 freq INTEGER,
                 last_used INTEGER,
                 is_custom INTEGER DEFAULT 0,
                 PRIMARY KEY (input_code, zh_word)
             );

             CREATE TABLE IF NOT EXISTS user_bigrams (
                 prev_word TEXT,
                 next_code TEXT,
                 zh_word TEXT,
                 freq INTEGER,
                 last_used INTEGER,
                 PRIMARY KEY (prev_word, next_code, zh_word)
             );",
        )?;

        // 相容舊版 user_dict.db 結構：若舊表缺少 abbr_code 或 is_custom 欄位則自動補齊
        let _ = self.conn.execute(
            "ALTER TABLE user_unigrams ADD COLUMN abbr_code TEXT DEFAULT ''",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE user_unigrams ADD COLUMN is_custom INTEGER DEFAULT 0",
            [],
        );

        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_user_uni_code ON user_unigrams (input_code);
             CREATE INDEX IF NOT EXISTS idx_user_uni_abbr ON user_unigrams (abbr_code);
             CREATE INDEX IF NOT EXISTS idx_user_bi_lookup ON user_bigrams (prev_word, next_code);",
        )?;

        // 從既有資料中同步最大時間戳，確保重啟後時間戳依然嚴格單調遞增
        let max_uni: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(last_used), 0) FROM user_unigrams",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let max_bi: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(last_used), 0) FROM user_bigrams",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let db_max = max_uni.max(max_bi);
        if db_max > 0 {
            let _ = MONOTONIC_TS.fetch_max(db_max, Ordering::SeqCst);
        }

        Ok(())
    }

    /// 檢查指定詞彙是否已經存在於系統詞庫 `cantonese_dict` 中
    pub fn exists_in_cantonese_dict(&self, zh_word: &str) -> bool {
        let word = zh_word.trim();
        if word.is_empty() {
            return false;
        }
        let Some(ref sys) = self.sys_conn else {
            return false;
        };
        if let Ok(mut stmt) = sys.prepare_cached(
            "SELECT 1 FROM cantonese_dict WHERE zh_word = ?1 LIMIT 1",
        ) {
            if let Ok(mut rows) = stmt.query(params![word]) {
                if let Ok(Some(_)) = rows.next() {
                    return true;
                }
            }
        }
        false
    }

    /// 反查單字在系統詞庫 `cantonese_dict` 中的標準無聲調全拼 (`toneless`) 與聲母簡拼 (`abbr`)
    /// 若使用者是用簡拼（如 `h` 選「興」），會自動還原為標準全拼 `("hing", "h")`
    pub fn lookup_canonical_char_phonetics(
        &self,
        ch: char,
        hint_input: &str,
    ) -> Option<(String, String)> {
        let Some(ref sys) = self.sys_conn else {
            return None;
        };
        let word = ch.to_string();
        let cleaned_hint = sanitize_input(hint_input);
        let expanded_hints: Vec<String> = expand_phonetic_queries(&cleaned_hint)
            .into_iter()
            .map(|q| q.jyutping)
            .collect();

        let mut stmt = sys
            .prepare_cached(
                "SELECT toneless, abbr FROM cantonese_dict WHERE zh_word = ?1 ORDER BY weight DESC",
            )
            .ok()?;

        let rows = stmt
            .query_map(params![&word], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .ok()?;

        let mut first_match: Option<(String, String)> = None;
        for row in rows.flatten() {
            let (toneless, abbr) = row;
            if first_match.is_none() {
                first_match = Some((toneless.clone(), abbr.clone()));
            }
            if !cleaned_hint.is_empty()
                && (toneless == cleaned_hint
                    || abbr == cleaned_hint
                    || expanded_hints.iter().any(|h| h == &toneless))
            {
                return Some((toneless, abbr));
            }
        }
        first_match
    }

    /// 記錄使用者選字行為：
    /// - 更新 `user_unigrams` 的詞頻 (`freq = freq + 1`) 與最後使用時間 (`last_used`)
    /// - 若 `prev_word` 存在且非空，則同步更新 `user_bigrams`
    pub fn record_selection(&self, input_code: &str, zh_word: &str, prev_word: Option<&str>) {
        let _ = self.try_record_selection(input_code, zh_word, prev_word);
    }

    /// 記錄使用者選字行為（回傳 `SqlResult` 版本）
    pub fn try_record_selection(
        &self,
        input_code: &str,
        zh_word: &str,
        prev_word: Option<&str>,
    ) -> SqlResult<()> {
        let cleaned = sanitize_input(input_code);
        let code = if cleaned.is_empty() {
            input_code.trim()
        } else {
            &cleaned
        };
        let word = zh_word.trim();
        if code.is_empty() || word.is_empty() {
            return Ok(());
        }

        let ts = next_timestamp();

        self.conn.execute(
            "INSERT INTO user_unigrams (input_code, abbr_code, zh_word, freq, last_used, is_custom)
             VALUES (?1, '', ?2, 1, ?3, 0)
             ON CONFLICT(input_code, zh_word) DO UPDATE SET
                 freq = freq + 1,
                 last_used = ?3",
            params![code, word, ts],
        )?;

        if let Some(prev) = prev_word {
            let prev_trimmed = prev.trim();
            if !prev_trimmed.is_empty() {
                self.conn.execute(
                    "INSERT INTO user_bigrams (prev_word, next_code, zh_word, freq, last_used)
                     VALUES (?1, ?2, ?3, 1, ?4)
                     ON CONFLICT(prev_word, next_code, zh_word) DO UPDATE SET
                         freq = freq + 1,
                         last_used = ?4",
                    params![prev_trimmed, code, word, ts],
                )?;
            }
        }

        Ok(())
    }

    /// 兩階段觀察與合成使用者自造詞：
    /// - 先檢查 `zh_phrase` 是否已經存在於系統詞庫 `cantonese_dict` 中，若已存在則無須重複造詞（直接依一般 Unigram 加分即可，並回傳 `false`）。
    /// - 若為全新詞彙：
    ///   * 第 1 次呼叫時寫入 `user_unigrams (input_code=toneless, abbr_code=abbr, zh_word=zh_phrase, freq=1, is_custom=1)`，回傳 `false`；
    ///   * 當第 2 次呼叫（`freq >= 2`）時正式啟用並回傳 `true`，此後在 `get_candidates` 查詢 `toneless` 或 `abbr` 時給予 `+5500` 的自造詞專屬加分。
    pub fn observe_custom_phrase(&self, toneless: &str, abbr: &str, zh_phrase: &str) -> bool {
        self.observe_custom_phrase_with_freq(toneless, abbr, zh_phrase)
            .map(|(promoted, _)| promoted)
            .unwrap_or(false)
    }

    /// 兩階段觀察與合成使用者自造詞（回傳 `Some((是否已達 freq >= 2 正式啟用, 最新 freq))`，若已存在於系統詞庫或無效則回傳 `None`）
    pub fn observe_custom_phrase_with_freq(
        &self,
        toneless: &str,
        abbr: &str,
        zh_phrase: &str,
    ) -> Option<(bool, i32)> {
        let phrase = zh_phrase.trim();
        let clean_toneless = sanitize_input(toneless);
        let clean_abbr = sanitize_input(abbr);
        if phrase.is_empty() || clean_toneless.is_empty() {
            return None;
        }

        // 1. 先檢查 zh_phrase 是否已經存在於系統詞庫 cantonese_dict 中
        if self.exists_in_cantonese_dict(phrase) {
            // 已存在於系統詞庫中，無須重複造詞，直接依一般 Unigram 加分即可
            self.record_selection(&clean_toneless, phrase, None);
            return None;
        }

        // 2. 全新詞彙：寫入或更新兩階段自造詞紀錄
        let ts = next_timestamp();
        let _ = self.conn.execute(
            "INSERT INTO user_unigrams (input_code, abbr_code, zh_word, freq, last_used, is_custom)
             VALUES (?1, ?2, ?3, 1, ?4, 1)
             ON CONFLICT(input_code, zh_word) DO UPDATE SET
                 abbr_code = CASE WHEN ?2 != '' THEN ?2 ELSE user_unigrams.abbr_code END,
                 freq = user_unigrams.freq + 1,
                 last_used = ?4,
                 is_custom = 1",
            params![&clean_toneless, &clean_abbr, phrase, ts],
        );

        let current_freq: i32 = self
            .conn
            .query_row(
                "SELECT freq FROM user_unigrams WHERE input_code = ?1 AND zh_word = ?2",
                params![&clean_toneless, phrase],
                |row| row.get(0),
            )
            .unwrap_or(1);

        Some((current_freq >= 2, current_freq))
    }

    /// 直接將使用者自造詞正式寫入並啟用（設定 `freq >= 2, is_custom = 1`，同時支援全拼 `toneless_code` 與簡拼 `abbr_code`）
    pub fn record_custom_phrase(&self, toneless_code: &str, abbr_code: &str, zh_phrase: &str) {
        let _ = self.try_record_custom_phrase(toneless_code, abbr_code, zh_phrase);
    }

    /// 直接將使用者自造詞正式寫入並啟用（回傳 `SqlResult` 版本）
    pub fn try_record_custom_phrase(
        &self,
        toneless_code: &str,
        abbr_code: &str,
        zh_phrase: &str,
    ) -> SqlResult<()> {
        let phrase = zh_phrase.trim();
        let toneless = sanitize_input(toneless_code);
        let abbr = sanitize_input(abbr_code);
        if phrase.is_empty() || toneless.is_empty() {
            return Ok(());
        }

        let ts = next_timestamp();
        self.conn.execute(
            "INSERT INTO user_unigrams (input_code, abbr_code, zh_word, freq, last_used, is_custom)
             VALUES (?1, ?2, ?3, 2, ?4, 1)
             ON CONFLICT(input_code, zh_word) DO UPDATE SET
                 abbr_code = CASE WHEN ?2 != '' THEN ?2 ELSE user_unigrams.abbr_code END,
                 freq = MAX(user_unigrams.freq + 1, 2),
                 last_used = ?4,
                 is_custom = 1",
            params![&toneless, &abbr, phrase, ts],
        )?;

        Ok(())
    }

    /// 允許使用者後悔並刪除誤選的記憶詞條（同時清理 Unigram 與對應的 Bigram 紀錄）
    pub fn remove_user_word(&self, input_code: &str, zh_word: &str) {
        let _ = self.try_remove_user_word(input_code, zh_word);
    }

    /// 刪除誤選的記憶詞條（回傳 `SqlResult` 版本）
    pub fn try_remove_user_word(&self, input_code: &str, zh_word: &str) -> SqlResult<()> {
        let cleaned = sanitize_input(input_code);
        let code = if cleaned.is_empty() {
            input_code.trim()
        } else {
            &cleaned
        };
        let word = zh_word.trim();

        self.conn.execute(
            "DELETE FROM user_unigrams WHERE (input_code = ?1 OR abbr_code = ?1) AND zh_word = ?2",
            params![code, word],
        )?;
        self.conn.execute(
            "DELETE FROM user_bigrams WHERE next_code = ?1 AND zh_word = ?2",
            params![code, word],
        )?;
        Ok(())
    }

    /// 查詢指定查詢碼（可為 `input_code` 全拼或 `abbr_code` 簡拼）下的所有已啟用個人 Unigram 記憶紀錄
    /// 注意：兩階段自造詞 (`is_custom = 1`) 必須達到 `freq >= 2` 才會在此回傳並參與候選加權
    pub fn query_unigrams(&self, input_code: &str) -> Vec<UserUnigramEntry> {
        let mut entries = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare_cached(
            "SELECT input_code, COALESCE(abbr_code, ''), zh_word, freq, last_used, is_custom
             FROM user_unigrams
             WHERE (input_code = ?1 OR (abbr_code = ?1 AND abbr_code != ''))
               AND (is_custom = 0 OR freq >= 2)
             ORDER BY last_used DESC, freq DESC",
        ) {
            if let Ok(rows) = stmt.query_map(params![input_code], |row| {
                Ok(UserUnigramEntry {
                    input_code: row.get(0)?,
                    abbr_code: row.get(1)?,
                    zh_word: row.get(2)?,
                    freq: row.get(3)?,
                    last_used: row.get(4)?,
                    is_custom: row.get::<_, i32>(5).unwrap_or(0) != 0,
                })
            }) {
                for row in rows.flatten() {
                    entries.push(row);
                }
            }
        }
        entries
    }

    /// 查詢所有自造詞紀錄（包含 `freq == 1` 觀察期與 `freq >= 2` 已轉正紀錄，供診斷與測試驗證）
    pub fn query_all_custom_phrases(&self) -> Vec<UserUnigramEntry> {
        let mut entries = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare_cached(
            "SELECT input_code, COALESCE(abbr_code, ''), zh_word, freq, last_used, is_custom
             FROM user_unigrams
             WHERE is_custom = 1
             ORDER BY last_used DESC, freq DESC",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok(UserUnigramEntry {
                    input_code: row.get(0)?,
                    abbr_code: row.get(1)?,
                    zh_word: row.get(2)?,
                    freq: row.get(3)?,
                    last_used: row.get(4)?,
                    is_custom: row.get::<_, i32>(5).unwrap_or(0) != 0,
                })
            }) {
                for row in rows.flatten() {
                    entries.push(row);
                }
            }
        }
        entries
    }

    /// 查詢指定 `(prev_word, next_code)` 下的所有個人 Bigram 語境紀錄
    pub fn query_bigrams(&self, prev_word: &str, next_code: &str) -> Vec<UserBigramEntry> {
        let mut entries = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare_cached(
            "SELECT prev_word, next_code, zh_word, freq, last_used
             FROM user_bigrams
             WHERE prev_word = ?1 AND next_code = ?2
             ORDER BY freq DESC, last_used DESC",
        ) {
            if let Ok(rows) = stmt.query_map(params![prev_word, next_code], |row| {
                Ok(UserBigramEntry {
                    prev_word: row.get(0)?,
                    next_code: row.get(1)?,
                    zh_word: row.get(2)?,
                    freq: row.get(3)?,
                    last_used: row.get(4)?,
                })
            }) {
                for row in rows.flatten() {
                    entries.push(row);
                }
            }
        }
        entries
    }

    /// 查詢接在指定 `prev_word` 後面最常用的 Bigram 聯想詞（供未輸入拼音時的無碼聯想使用）
    pub fn query_predictions(&self, prev_word: &str, limit: usize) -> Vec<UserBigramEntry> {
        let mut entries = Vec::new();
        if prev_word.trim().is_empty() || limit == 0 {
            return entries;
        }
        if let Ok(mut stmt) = self.conn.prepare_cached(
            "SELECT prev_word, next_code, zh_word, freq, last_used
             FROM user_bigrams
             WHERE prev_word = ?1
             ORDER BY freq DESC, last_used DESC
             LIMIT ?2",
        ) {
            if let Ok(rows) = stmt.query_map(params![prev_word, limit as i64], |row| {
                Ok(UserBigramEntry {
                    prev_word: row.get(0)?,
                    next_code: row.get(1)?,
                    zh_word: row.get(2)?,
                    freq: row.get(3)?,
                    last_used: row.get(4)?,
                })
            }) {
                for row in rows.flatten() {
                    entries.push(row);
                }
            }
        }
        entries
    }

    /// 批次查詢多個拼音碼 (`codes`) 對應的已啟用個人 Unigram 記憶紀錄（供長句詞圖建構一次撈取）
    pub fn query_unigrams_batch(
        &self,
        codes: &[String],
    ) -> std::collections::HashMap<String, Vec<UserUnigramEntry>> {
        use std::collections::HashMap;
        let mut map: HashMap<String, Vec<UserUnigramEntry>> = HashMap::new();
        if codes.is_empty() {
            return map;
        }

        let placeholders: String = (1..=codes.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT input_code, COALESCE(abbr_code, ''), zh_word, freq, last_used, is_custom
             FROM user_unigrams
             WHERE (input_code IN ({placeholders}) OR (abbr_code IN ({placeholders}) AND abbr_code != ''))
               AND (is_custom = 0 OR freq >= 2)
             ORDER BY last_used DESC, freq DESC"
        );

        if let Ok(mut stmt) = self.conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(codes.iter()), |row| {
                Ok(UserUnigramEntry {
                    input_code: row.get(0)?,
                    abbr_code: row.get(1)?,
                    zh_word: row.get(2)?,
                    freq: row.get(3)?,
                    last_used: row.get(4)?,
                    is_custom: row.get::<_, i32>(5).unwrap_or(0) != 0,
                })
            }) {
                let code_set: std::collections::HashSet<&str> =
                    codes.iter().map(|s| s.as_str()).collect();
                for entry in rows.flatten() {
                    if code_set.contains(entry.input_code.as_str()) {
                        map.entry(entry.input_code.clone())
                            .or_default()
                            .push(entry.clone());
                    }
                    if !entry.abbr_code.is_empty()
                        && entry.abbr_code != entry.input_code
                        && code_set.contains(entry.abbr_code.as_str())
                    {
                        map.entry(entry.abbr_code.clone()).or_default().push(entry);
                    }
                }
            }
        }
        map
    }

    /// 批次查詢多個拼音碼 (`codes`) 對應的個人 Bigram 語境紀錄（回傳 `(prev_word, next_code) -> HashSet<zh_word>`）
    pub fn query_bigrams_batch(
        &self,
        codes: &[String],
    ) -> std::collections::HashMap<(String, String), std::collections::HashSet<String>> {
        use std::collections::{HashMap, HashSet};
        let mut map: HashMap<(String, String), HashSet<String>> = HashMap::new();
        if codes.is_empty() {
            return map;
        }

        let placeholders: String = (1..=codes.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT prev_word, next_code, zh_word
             FROM user_bigrams
             WHERE next_code IN ({placeholders})"
        );

        if let Ok(mut stmt) = self.conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(codes.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            }) {
                for (prev_word, next_code, zh_word) in rows.flatten() {
                    map.entry((prev_word, next_code))
                        .or_default()
                        .insert(zh_word);
                }
            }
        }
        map
    }
}
