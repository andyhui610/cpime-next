//! CPIME-Next 核心雙軌檢索、個人詞頻學習與 Bigram 上下文混排引擎
//!
//! 負責管理 SQLite 詞庫連線、執行粵語軌（標準粵拼 / 多制式正規化 / 懶音容錯 / 首字母簡拼 / 前綴）
//! 與英文反查軌（精確匹配 / 前綴匹配）查詢，並整合 `UserDict` 實作個人詞頻加權與前後字連帶排序。

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags, Result as SqlResult};

use crate::decoder::SentenceDecoder;
use crate::phonetics::{
    expand_phonetic_queries, normalize_to_jyutping, sanitize_input, segment_hybrid_syllables,
    SyllableKind,
};
use crate::phrase_composer::{PhraseComposer, PhraseCompositionEvent};
use crate::user_dict::UserDict;

/// 候選詞來源標籤
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateSource {
    /// 標準粵拼、多制式正規化（教院/耶魯/劉錫祥/港式）或首字母簡拼
    Jyutping,
    /// 模糊音與懶音容錯（如 n/l、ng/零聲母、gw/g、-n/-ng 等）
    Fuzzy,
    /// 英漢反查詞庫（打英文直出中文）
    English,
    /// Bigram 前後字語境連帶推薦
    BigramContext,
    /// 使用者自造詞或個人記憶詞庫
    UserCustom,
    /// DAG 詞圖 + Viterbi 演算法產出之長句連打整句候選
    SentenceDecoded,
}

impl CandidateSource {
    /// 與規範文件對應之整句來源別名
    #[allow(non_upper_case_globals)]
    pub const Sentence: CandidateSource = CandidateSource::SentenceDecoded;

    /// 回傳繁體中文來源標籤名稱
    pub fn label(&self) -> &'static str {
        match self {
            CandidateSource::Jyutping => "粵拼",
            CandidateSource::Fuzzy => "懶音",
            CandidateSource::English => "英文反查",
            CandidateSource::BigramContext => "前後字連帶",
            CandidateSource::UserCustom => "個人詞庫",
            CandidateSource::SentenceDecoded => "整句連打",
        }
    }
}

impl fmt::Display for CandidateSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// 單一候選詞結構
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// 候選中文字詞（完整支援 UTF-8 四位元組香港擴展漢字，如 𨋢、嘅、喺）
    pub word: String,
    /// 匹配到的讀音或英文單字
    pub matched_code: String,
    /// 候選詞來源標籤（粵拼 / 懶音 / 英文反查 / 前後字連帶 / 個人詞庫 / 整句連打）
    pub source: CandidateSource,
    /// 內部排序權重分數
    pub score: i32,
    /// 選中此候選詞後應從原始 `input_buffer` 開頭消耗掉的英文字元數
    pub consumed_chars: usize,
}

/// 粵語軌內部查詢結果摘要
struct CantoneseTrackResult {
    candidates: Vec<Candidate>,
    /// 是否有非懶音的完整粵拼匹配（含標準粵拼與多制式正規化，不含簡拼與前綴）
    has_exact_or_normalized_full: bool,
    /// 是否有任何完整音節匹配（含懶音容錯）
    has_any_full_syllable: bool,
    /// 是否有高頻簡拼直接命中（如 `hg` -> 香港、`nh` -> 你好，避免 2 字母簡拼被低頻英語縮寫覆蓋）
    has_strong_abbr_match: bool,
}

/// 英文反查軌內部查詢結果摘要
struct EnglishTrackResult {
    candidates: Vec<Candidate>,
    /// 是否有精確英文單字匹配（用於精準判定短碼撞碼）
    has_exact_match: bool,
}

/// 輸出語體模式（道地口語 / 標準書面語）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// 口語模式（預設）：優先輸出道地香港粵語用字（如：嘅、喺、唔、𨋢、佢哋、聽日）
    #[default]
    Colloquial,
    /// 書面語模式：自動將高頻粵音映射為標準書面語並提升非口語詞權重（如：的、在、不、升降機、他們、明天）
    Written,
}

/// 高頻「粵音直出書面語映射表」（於 `OutputMode::Written` 啟用時注入候選清單第 1~2 名）
pub(crate) fn written_cantonese_mapping(code: &str) -> Option<&'static [&'static str]> {
    match code {
        "ge" => Some(&["的"]),
        "hai" => Some(&["是", "在"]),
        "haidou" => Some(&["在度", "在這裡"]),
        "m" => Some(&["不", "沒"]),
        "mou" => Some(&["沒有"]),
        "zo" => Some(&["了"]),
        "keoi" => Some(&["他", "她"]),
        "keoidei" => Some(&["他們"]),
        "ngo" => Some(&["我"]),
        "ngodei" => Some(&["我們"]),
        "nei" => Some(&["你"]),
        "neidei" => Some(&["你們"]),
        "gamjat" => Some(&["今天"]),
        "tingjat" => Some(&["明天"]),
        "camjat" | "kamjat" => Some(&["昨天"]),
        "jigaa" => Some(&["現在"]),
        "bingo" => Some(&["誰"]),
        "dimgaai" => Some(&["為什麼"]),
        "bindou" => Some(&["哪裡"]),
        "sikngaan" => Some(&["吃午飯"]),
        "kinggai" => Some(&["聊天"]),
        "lip" => Some(&["升降機", "電梯"]),
        _ => None,
    }
}

/// CPIME-Next 核心輸入法引擎
pub struct CpimeEngine {
    conn: Connection,
    /// 個人詞頻與 Bigram 語境記憶資料庫 (`user_dict.db`)
    pub user_dict: UserDict,
    /// 上一次上屏的字詞（用於 Bigram 前後字語境連帶）
    pub last_committed_word: Option<String>,
    /// 智慧自造詞滑動視窗與虛詞過濾合成器
    pub phrase_composer: PhraseComposer,
    /// 目前輸出語體模式（口語 / 書面語）
    pub output_mode: OutputMode,
}

/// `Engine` 型別別名，方便外部模組以 `Engine` 或 `CpimeEngine` 存取
pub type Engine = CpimeEngine;

impl CpimeEngine {
    /// 從指定的系統詞庫路徑初始化引擎（預設搭配 `:memory:` 個人詞庫，適合單元測試或獨立執行個體）
    pub fn open<P: AsRef<Path>>(db_path: P) -> SqlResult<Self> {
        let user_dict = UserDict::open_in_memory()?;
        Self::with_user_dict(db_path, user_dict)
    }

    /// 從指定的系統詞庫路徑與 `user_dict.db` 路徑初始化引擎
    pub fn open_with_user_dict<P1: AsRef<Path>, P2: AsRef<Path>>(
        db_path: P1,
        user_db_path: P2,
    ) -> SqlResult<Self> {
        let user_dict = UserDict::open(user_db_path)?;
        Self::with_user_dict(db_path, user_dict)
    }

    /// 從指定的系統詞庫路徑與已初始化的 `UserDict` 實例建立引擎
    pub fn with_user_dict<P: AsRef<Path>>(db_path: P, mut user_dict: UserDict) -> SqlResult<Self> {
        let path_ref = db_path.as_ref();
        user_dict.set_sys_dict_path(path_ref);

        // 將路徑轉換為 SQLite URI 並附加 immutable=1，徹底免除唯讀詞庫在每次查詢時的 OS 檔案鎖 (LockFileEx) 開銷
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

        let conn = Connection::open_with_flags(&uri, flags).or_else(|_| {
            Connection::open_with_flags(
                path_ref,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
        })?;

        // 啟用唯讀查詢最佳化、記憶體快取與 64MB Memory-Mapped I/O，確保單次查詢延遲低於 1ms
        conn.execute_batch(
            "PRAGMA query_only = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -16000;
             PRAGMA mmap_size = 67108864;",
        )?;

        // 預熱 SQLite Schema、Statement Cache 與 B-Tree 索引根節點，避免首次按鍵時承擔冷啟動延遲
        let engine = Self {
            conn,
            user_dict,
            last_committed_word: None,
            phrase_composer: PhraseComposer::new(),
            output_mode: OutputMode::Colloquial,
        };
        let _ = engine.lookup_with_limit("sun", 1);
        let _ = engine.lookup_with_limit("aftern", 1);
        let _ = engine.lookup_with_limit("gamjat", 1);
        Ok(engine)
    }

    /// 設定輸出語體模式（口語 / 書面語）
    pub fn set_output_mode(&mut self, mode: OutputMode) {
        self.output_mode = mode;
    }

    /// 取得目前輸出語體模式（口語 / 書面語）
    pub fn get_output_mode(&self) -> OutputMode {
        self.output_mode
    }

    /// 重新載入個人記憶詞庫 (`user_dict.db`)
    pub fn reload_user_dict(&mut self) -> SqlResult<()> {
        self.user_dict.reload()
    }

    /// 自動尋找預設的 `/core/assets/cpime_dict.db` 並開啟引擎（搭配 `:memory:` 個人詞庫）
    pub fn open_default() -> SqlResult<Self> {
        let db_path = Self::default_db_path();
        Self::open(db_path)
    }

    /// 自動尋找預設系統詞庫並搭配指定的 `user_dict.db` 路徑開啟引擎
    pub fn open_default_with_user_dict<P: AsRef<Path>>(user_db_path: P) -> SqlResult<Self> {
        let db_path = Self::default_db_path();
        Self::open_with_user_dict(db_path, user_db_path)
    }

    /// 取得預設系統詞庫路徑 (`/core/assets/cpime_dict.db`)
    pub fn default_db_path() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("assets").join("cpime_dict.db")
    }

    /// 取得預設個人詞庫路徑 (`/core/assets/user_dict.db`)
    pub fn default_user_db_path() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("assets").join("user_dict.db")
    }

    /// 清空當前上下文狀態（供換行、分段或點擊其他視窗時重置 `last_committed_word` 並結算清空自造詞緩衝區）
    pub fn clear_context(&mut self) {
        self.last_committed_word = None;
        let _ = self.phrase_composer.flush(&mut self.user_dict);
    }

    /// 當使用者按 Backspace、輸入英文反查詞或選中多字詞時，切斷自造詞合成序列
    pub fn break_sequence(&mut self) {
        self.phrase_composer.break_sequence();
    }

    /// 結算並清空目前自造詞緩衝區（如遇到標點符號、空白鍵或視窗切換）
    pub fn flush_custom_phrases(&mut self) -> Vec<PhraseCompositionEvent> {
        self.phrase_composer.flush(&mut self.user_dict)
    }

    /// 執行雙軌混合與個人記憶加權查詢，預設回傳前 9 個排序後的候選詞
    pub fn get_candidates(&self, input: &str) -> Vec<Candidate> {
        self.lookup_with_limit(input, 9)
    }

    /// 執行雙軌混合與個人記憶加權查詢（`get_candidates` 之別名，維持既有 API 相容）
    pub fn lookup(&self, raw_input: &str) -> Vec<Candidate> {
        self.lookup_with_limit(raw_input, 9)
    }

    /// 執行雙軌混合與個人記憶加權查詢並指定最大候選詞數量
    pub fn lookup_with_limit(&self, raw_input: &str, limit: usize) -> Vec<Candidate> {
        let cleaned = sanitize_input(raw_input);
        if cleaned.is_empty() || limit == 0 {
            return Vec::new();
        }

        let canto_res = self.query_cantonese_track(&cleaned);
        let eng_res = self.query_english_track(&cleaned, canto_res.has_any_full_syllable);

        let has_canto_full_or_fuzzy = canto_res.has_any_full_syllable;
        let is_short_collision =
            cleaned.len() <= 4 && canto_res.has_any_full_syllable && eng_res.has_exact_match;
        let is_pure_or_long_english = !eng_res.candidates.is_empty()
            && ((!canto_res.has_any_full_syllable && !canto_res.has_strong_abbr_match)
                || (cleaned.len() >= 5 && !canto_res.has_exact_or_normalized_full));

        // 先保留較寬的雙軌候選池，避免排名較後的系統候選詞在加權前被截斷
        let pool_limit = limit.max(30);
        let mut candidates = self.merge_dual_tracks(&cleaned, canto_res, eng_res, pool_limit);

        // 整合 user_unigrams（含自造詞）與 user_bigrams 動態加權排序
        self.apply_user_learning(&cleaned, &mut candidates);

        let full_consumed = raw_input.len();
        for cand in &mut candidates {
            cand.consumed_chars = full_consumed;
        }

        // 英文長單字與短碼撞碼保護：維持原本雙軌排序，不拆解為粵語長句
        if !is_short_collision && !is_pure_or_long_english {
            let syllables = segment_hybrid_syllables(raw_input);
            if syllables.len() >= 2 {
                let has_whole_word_match = has_canto_full_or_fuzzy
                    || candidates.iter().any(|c| {
                        matches!(
                            c.source,
                            CandidateSource::UserCustom | CandidateSource::BigramContext
                        )
                    });
                let has_abbr_node = syllables
                    .iter()
                    .any(|s| matches!(s.kind, SyllableKind::Abbr(_)));
                let has_full_node = syllables
                    .iter()
                    .any(|s| matches!(s.kind, SyllableKind::Full(_)));
                let is_hybrid_two_syllable =
                    syllables.len() == 2 && has_abbr_node && has_full_node;

                // 若為 2 音節以上且非「只有一個 2 音節非混合完整命中且同時不需要前綴」（如在一般單詞查詢中需要前綴，但對只有 2 音節的 `gamjat` 等仍可快取）
                // 這裡當 `syllables.len() == 2 && has_whole_word_match && !is_hybrid_two_syllable` 且已有足夠候選時，可省略額外字詞圖 SQL 查詢以維持 <1ms 極速；
                // 但須注意 `gamjattingjat` (4音節) 會完整進入 `SentenceDecoder`。
                // 同時對於 `SentenceDecoder` 內部僅在 `abbr_keys` 非空時才查詢簡拼池。
                if has_whole_word_match && syllables.len() == 2 && !is_hybrid_two_syllable {
                    candidates.truncate(limit);
                    return candidates;
                }

                if let Some(decoded) = SentenceDecoder::decode(
                    &self.conn,
                    &self.user_dict,
                    self.output_mode,
                    self.last_committed_word.as_deref(),
                    raw_input,
                    &syllables,
                ) {
                    let mut combined =
                        Vec::with_capacity(limit.max(decoded.prefix_candidates.len() + 4));
                    let mut seen_words = HashSet::new();

                    if !has_whole_word_match
                        && (syllables.len() >= 3
                            || (is_hybrid_two_syllable && decoded.is_single_whole_word))
                    {
                        // 情況 B1：3 音節以上多詞連打長句（含全拼、混合與純簡拼），或 2 音節混合整詞命中，
                        // 將 Viterbi 解碼結果置於第 1 順位，第 2~9 位放開頭前綴詞/單字
                        let sentence_cand = Candidate {
                            word: decoded.full_sentence.clone(),
                            matched_code: decoded.matched_code,
                            source: CandidateSource::SentenceDecoded,
                            score: decoded.total_score,
                            consumed_chars: full_consumed,
                        };
                        seen_words.insert(sentence_cand.word.clone());
                        combined.push(sentence_cand);

                        for p_cand in decoded.prefix_candidates {
                            if seen_words.insert(p_cand.word.clone()) {
                                combined.push(p_cand);
                            }
                        }
                        for cand in candidates {
                            if seen_words.insert(cand.word.clone()) {
                                combined.push(cand);
                            }
                        }
                    } else {
                        // 情況 A（完整詞命中，如 "gamjat" -> "今日"）與
                        // 情況 B2（2 音節未登入詞，保留兩階段自造詞觀察期與刪除記憶）：
                        // 先保留既有雙軌與個人記憶候選詞，再補入開頭單音節前綴候選詞供部分選字
                        for cand in candidates {
                            if seen_words.insert(cand.word.clone()) {
                                combined.push(cand);
                            }
                        }
                        for p_cand in decoded.prefix_candidates {
                            if seen_words.insert(p_cand.word.clone()) {
                                combined.push(p_cand);
                            }
                        }
                    }

                    combined.truncate(limit);
                    return combined;
                }
            }
        }

        candidates.truncate(limit);
        candidates
    }

    /// 當使用者選中第 `index` 個候選詞（0-based）時，自動更新 `user_dict`、維護 `last_committed_word`，
    /// 並透過 `PhraseComposer` 執行單字滑動視窗合成與廣東話虛詞過濾
    pub fn commit_candidate(&mut self, input: &str, index: usize) -> Option<String> {
        let fetch_limit = (index + 1).max(30);
        let candidates = self.lookup_with_limit(input, fetch_limit);
        let chosen = candidates.into_iter().nth(index)?;

        let consumed = chosen.consumed_chars.min(input.len());
        let consumed_slice = if consumed > 0 {
            &input[..consumed]
        } else {
            input
        };
        let cleaned = sanitize_input(consumed_slice);
        let code = if cleaned.is_empty() {
            consumed_slice.trim()
        } else {
            &cleaned
        };

        self.user_dict
            .record_selection(code, &chosen.word, self.last_committed_word.as_deref());
        self.last_committed_word = Some(chosen.word.clone());

        // 整合 PhraseComposer 智慧自造詞自動合成：
        // - 若為英文反查詞、整句連打或選中多字詞（char_count != 1），立即呼叫 break_sequence()
        // - 若為粵拼選中 1 個單字，送入 PhraseComposer（自動處理 4000ms 超時、廣東話虛詞過濾、簡拼還原全拼與 2~4 字滑動視窗合成）
        let char_count = chosen.word.chars().count();
        if chosen.source == CandidateSource::English
            || chosen.source == CandidateSource::SentenceDecoded
            || char_count != 1
        {
            self.phrase_composer.break_sequence();
        } else if let Some(ch) = chosen.word.chars().next() {
            self.phrase_composer.on_char_selected(
                ch,
                code,
                &chosen.matched_code,
                &mut self.user_dict,
            );
        }

        Some(chosen.word)
    }

    /// 當目前沒有輸入任何拼音，但 `last_committed_word` 存在時，直接從 `user_bigrams` 撈出最常接在該詞後面的聯想詞
    pub fn get_predictions(&self, limit: usize) -> Vec<Candidate> {
        if limit == 0 {
            return Vec::new();
        }
        let prev_word = match self.last_committed_word.as_deref() {
            Some(w) if !w.trim().is_empty() => w,
            _ => return Vec::new(),
        };

        let entries = self.user_dict.query_predictions(prev_word, limit * 2);
        let mut results = Vec::with_capacity(entries.len().min(limit));
        let mut seen = HashSet::new();

        for entry in entries {
            if seen.insert(entry.zh_word.clone()) {
                let score = 8_000 + (entry.freq * 500).min(4_000);
                results.push(Candidate {
                    word: entry.zh_word,
                    matched_code: entry.next_code,
                    source: CandidateSource::BigramContext,
                    score,
                    consumed_chars: 0,
                });
                if results.len() >= limit {
                    break;
                }
            }
        }
        results
    }

    /// 便捷轉發：記錄使用者選字並同步更新 `last_committed_word`
    pub fn record_selection(&mut self, input_code: &str, zh_word: &str, prev_word: Option<&str>) {
        self.user_dict
            .record_selection(input_code, zh_word, prev_word);
        self.last_committed_word = Some(zh_word.trim().to_string());
    }

    /// 便捷轉發：註冊使用者自造詞（同時支援全拼 `toneless_code` 與簡拼 `abbr_code`）
    pub fn record_custom_phrase(&self, toneless_code: &str, abbr_code: &str, zh_phrase: &str) {
        self.user_dict
            .record_custom_phrase(toneless_code, abbr_code, zh_phrase);
    }

    /// 便捷轉發：刪除誤選的個人記憶詞條
    pub fn remove_user_word(&self, input_code: &str, zh_word: &str) {
        self.user_dict.remove_user_word(input_code, zh_word);
    }

    /// 套用個人詞頻 (`user_unigrams`) 與上下文連帶 (`user_bigrams`) 動態加權公式：
    /// - Unigram 加成：`min(freq * 400, 5000)`，且該 `input` 下 `last_used` 最新的一筆額外加 `3500` 分
    /// - 兩階段自造詞加成：當 `is_custom == true` 且 `freq >= 2` 正式啟用時，查詢 `toneless` 或 `abbr` 給予 `+5500` 的自造詞專屬加分
    /// - Bigram 上下文加成：若命中 `(prev_word, input, zh_word)`，額外加 `8000 + min(freq * 500, 4000)` 分，
    ///   並將其 `CandidateSource` 標記為 `BigramContext`
    fn apply_user_learning(&self, cleaned: &str, candidates: &mut Vec<Candidate>) {
        let uni_entries = self.user_dict.query_unigrams(cleaned);
        let bi_entries = match self.last_committed_word.as_deref() {
            Some(prev) if !prev.is_empty() => self.user_dict.query_bigrams(prev, cleaned),
            _ => Vec::new(),
        };

        if uni_entries.is_empty() && bi_entries.is_empty() {
            return;
        }

        let max_last_used = uni_entries.iter().map(|e| e.last_used).max();
        let mut uni_map: HashMap<String, (i32, bool)> = HashMap::with_capacity(uni_entries.len());
        for entry in &uni_entries {
            let freq_bonus = (entry.freq * 400).min(5_000);
            let latest_bonus = if Some(entry.last_used) == max_last_used && entry.last_used > 0 {
                3_500
            } else {
                0
            };
            let custom_bonus = if entry.is_custom && entry.freq >= 2 {
                5_500
            } else {
                0
            };
            uni_map.insert(
                entry.zh_word.clone(),
                (
                    freq_bonus + latest_bonus + custom_bonus,
                    entry.is_custom,
                ),
            );
        }

        let mut bi_map: HashMap<String, i32> = HashMap::with_capacity(bi_entries.len());
        for entry in &bi_entries {
            let bi_bonus = 8_000 + (entry.freq * 500).min(4_000);
            bi_map.insert(entry.zh_word.clone(), bi_bonus);
        }

        let mut existing_words = HashSet::with_capacity(candidates.len());
        for cand in candidates.iter_mut() {
            existing_words.insert(cand.word.clone());
            if let Some(&(uni_bonus, is_custom)) = uni_map.get(&cand.word) {
                cand.score += uni_bonus;
                if is_custom && cand.source != CandidateSource::BigramContext {
                    cand.source = CandidateSource::UserCustom;
                }
            }
            if let Some(&bi_bonus) = bi_map.get(&cand.word) {
                cand.score += bi_bonus;
                cand.source = CandidateSource::BigramContext;
            }
        }

        // 將尚未出現在系統候選清單中的使用者自造詞或記憶詞條補入候選池
        for entry in &uni_entries {
            if existing_words.insert(entry.zh_word.clone()) {
                let (uni_bonus, is_custom) = uni_map[&entry.zh_word];
                let bi_bonus = bi_map.get(&entry.zh_word).copied().unwrap_or(0);
                let base_score = 20_000;
                let source = if bi_bonus > 0 {
                    CandidateSource::BigramContext
                } else if is_custom {
                    CandidateSource::UserCustom
                } else {
                    CandidateSource::Jyutping
                };
                candidates.push(Candidate {
                    word: entry.zh_word.clone(),
                    matched_code: cleaned.to_string(),
                    source,
                    score: base_score + uni_bonus + bi_bonus,
                    consumed_chars: cleaned.len(),
                });
            }
        }

        for entry in &bi_entries {
            if existing_words.insert(entry.zh_word.clone()) {
                let bi_bonus = bi_map[&entry.zh_word];
                let base_score = 20_000;
                candidates.push(Candidate {
                    word: entry.zh_word.clone(),
                    matched_code: cleaned.to_string(),
                    source: CandidateSource::BigramContext,
                    score: base_score + bi_bonus,
                    consumed_chars: cleaned.len(),
                });
            }
        }

        // 依據動態加權後的總分進行穩定降序排列
        candidates.sort_by(|a, b| b.score.cmp(&a.score));
    }

    /// 查詢粵語軌：包含標準粵拼、多方案正規化、懶音容錯、首字母簡拼、前綴匹配
    fn query_cantonese_track(&self, cleaned: &str) -> CantoneseTrackResult {
        let mut raw_candidates: Vec<Candidate> = Vec::new();
        let mut has_exact_or_normalized_full = false;
        let mut has_any_full_syllable = false;
        let mut has_strong_abbr_match = false;

        // 1. 展開多拼音正規化 + 懶音容錯變體，查詢無聲調全拼 (toneless) 並取出原聲調 (jyutping_toned)
        let expanded_queries = expand_phonetic_queries(cleaned);

        // 1a. 若處於書面語模式 (OutputMode::Written)，優先將高頻「粵音直出書面語」注入候選清單第 1~2 名
        if self.output_mode == OutputMode::Written {
            for pq in &expanded_queries {
                if let Some(written_words) = written_cantonese_mapping(&pq.jyutping) {
                    has_any_full_syllable = true;
                    let source = if pq.is_fuzzy {
                        CandidateSource::Fuzzy
                    } else {
                        has_exact_or_normalized_full = true;
                        CandidateSource::Jyutping
                    };
                    for (idx, &w_word) in written_words.iter().enumerate() {
                        raw_candidates.push(Candidate {
                            word: w_word.to_string(),
                            matched_code: pq.jyutping.clone(),
                            source,
                            score: 25_000 - (idx as i32 * 100) + pq.penalty,
                            consumed_chars: cleaned.len(),
                        });
                    }
                }
            }
        }

        if let Ok(mut stmt) = self.conn.prepare_cached(
            "SELECT zh_word, jyutping_toned, weight FROM cantonese_dict WHERE toneless = ?1 ORDER BY weight DESC LIMIT 20",
        ) {
            for pq in &expanded_queries {
                if let Ok(rows) = stmt.query_map(params![&pq.jyutping], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i32>(2)?,
                    ))
                }) {
                    for row in rows.flatten() {
                        let (word, toned_jp, freq) = row;
                        has_any_full_syllable = true;
                        let (source, base_bonus) = if pq.is_fuzzy {
                            (CandidateSource::Fuzzy, 8_000)
                        } else {
                            has_exact_or_normalized_full = true;
                            (CandidateSource::Jyutping, 10_000)
                        };
                        raw_candidates.push(Candidate {
                            word,
                            matched_code: toned_jp,
                            source,
                            score: freq + pq.penalty + base_bonus,
                            consumed_chars: cleaned.len(),
                        });
                    }
                }
            }
        }

        // 2. 首字母簡拼查詢 (abbr = cleaned，如 "nh" -> "你好", "gj" -> "今日")
        if let Ok(mut stmt) = self.conn.prepare_cached(
            "SELECT zh_word, jyutping_toned, weight FROM cantonese_dict WHERE abbr = ?1 ORDER BY weight DESC LIMIT 15",
        ) {
            // 2a. 直接簡拼匹配
            if let Ok(rows) = stmt.query_map(params![cleaned], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                ))
            }) {
                for (word, toned_jp, freq) in rows.flatten() {
                    if freq >= 8_000 {
                        has_strong_abbr_match = true;
                    }
                    raw_candidates.push(Candidate {
                        word,
                        matched_code: toned_jp,
                        source: CandidateSource::Jyutping,
                        score: freq + 5_000,
                        consumed_chars: cleaned.len(),
                    });
                }
            }

            // 2b. 若首字母為 l/n 懶音（如輸入 "lh" 查 "nh" 你好），支援簡拼懶音容錯
            if let Some(rest) = cleaned.strip_prefix('l') {
                let fuzzy_abbr = format!("n{rest}");
                if let Ok(rows) = stmt.query_map(params![&fuzzy_abbr], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i32>(2)?,
                    ))
                }) {
                    for (word, toned_jp, freq) in rows.flatten() {
                        raw_candidates.push(Candidate {
                            word,
                            matched_code: toned_jp,
                            source: CandidateSource::Fuzzy,
                            score: freq + 4_000,
                            consumed_chars: cleaned.len(),
                        });
                    }
                }
            }
        }

        // 3. 粵拼前綴查詢（當輸入長度 >= 2 且完整匹配候選詞不足 15 個時，利用 B-Tree 範圍索引快速補齊聯想詞）
        if cleaned.len() >= 2 && raw_candidates.len() < 15 {
            let norm_queries = normalize_to_jyutping(cleaned);
            if let Ok(mut stmt) = self.conn.prepare_cached(
                "SELECT zh_word, jyutping_toned, weight FROM (SELECT zh_word, jyutping_toned, weight FROM cantonese_dict WHERE toneless > ?1 AND toneless < ?2 LIMIT 300) ORDER BY weight DESC LIMIT 15",
            ) {
                for nq in &norm_queries {
                    if let Some(upper) = next_prefix_bound(&nq.jyutping) {
                        if let Ok(rows) = stmt.query_map(params![&nq.jyutping, &upper], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, i32>(2)?,
                            ))
                        }) {
                            for (word, toned_jp, freq) in rows.flatten() {
                                raw_candidates.push(Candidate {
                                    word,
                                    matched_code: toned_jp,
                                    source: CandidateSource::Jyutping,
                                    score: freq + nq.penalty + 1_000,
                                    consumed_chars: cleaned.len(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 依分數高低排序並針對相同字詞去重（保留最高分與對應來源）
        raw_candidates.sort_by(|a, b| b.score.cmp(&a.score));
        let mut deduped = Vec::with_capacity(raw_candidates.len());
        let mut seen = HashSet::new();
        for cand in raw_candidates {
            if seen.insert(cand.word.clone()) {
                deduped.push(cand);
            }
        }

        CantoneseTrackResult {
            candidates: deduped,
            has_exact_or_normalized_full,
            has_any_full_syllable,
            has_strong_abbr_match,
        }
    }

    /// 查詢英文反查軌：支援精確英文單字匹配與前綴匹配（如 "aftern" -> "afternoon"）
    fn query_english_track(
        &self,
        cleaned: &str,
        has_canto_full_syllable: bool,
    ) -> EnglishTrackResult {
        let mut raw_candidates = Vec::new();
        let mut has_exact_match = false;

        // 1. 精確英文單字匹配（基礎加成 9_800，使短碼撞碼時預設分數略低於前 2 名高頻粵拼字，選過後即可躍升置頂）
        if let Ok(mut stmt) = self.conn.prepare_cached(
            "SELECT zh_word, en_word, weight, is_colloquial FROM english_dict
             WHERE en_word = ?1
             ORDER BY weight DESC LIMIT 10",
        ) {
            if let Ok(rows) = stmt.query_map(params![cleaned], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, i32>(3)?,
                ))
            }) {
                for (canto_word, eng_word, priority, is_colloquial) in rows.flatten() {
                    has_exact_match = true;
                    let written_bonus =
                        if self.output_mode == OutputMode::Written && is_colloquial == 0 {
                            1_500
                        } else {
                            0
                        };
                    raw_candidates.push(Candidate {
                        word: canto_word,
                        matched_code: eng_word,
                        source: CandidateSource::English,
                        score: priority + 9_800 + written_bonus,
                        consumed_chars: cleaned.len(),
                    });
                }
            }
        }

        // 2. 英文單字前綴匹配（當輸入長度 >= 4，或輸入長度 == 3 且非完整粵拼音節時啟用，如 "aftern" -> "afternoon"）
        if cleaned.len() >= 4
            || (cleaned.len() == 3 && !has_canto_full_syllable && !has_exact_match)
        {
            if let Some(upper) = next_prefix_bound(cleaned) {
                if let Ok(mut stmt) = self.conn.prepare_cached(
                    "SELECT zh_word, en_word, weight, is_colloquial FROM (SELECT zh_word, en_word, weight, is_colloquial FROM english_dict WHERE en_word > ?1 AND en_word < ?2 LIMIT 200) ORDER BY weight DESC LIMIT 10",
                ) {
                    if let Ok(rows) = stmt.query_map(params![cleaned, &upper], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i32>(2)?,
                            row.get::<_, i32>(3)?,
                        ))
                    }) {
                        for (canto_word, eng_word, priority, is_colloquial) in rows.flatten() {
                            let written_bonus =
                                if self.output_mode == OutputMode::Written && is_colloquial == 0 {
                                    1_500
                                } else {
                                    0
                                };
                            raw_candidates.push(Candidate {
                                word: canto_word,
                                matched_code: eng_word,
                                source: CandidateSource::English,
                                score: priority + 5_000 + written_bonus,
                                consumed_chars: cleaned.len(),
                            });
                        }
                    }
                }
            }
        }

        // 依據分數降序排列（確保書面語模式加分後的標準書面語排在口語詞前面）並去重
        raw_candidates.sort_by(|a, b| b.score.cmp(&a.score));
        let mut deduped = Vec::with_capacity(raw_candidates.len());
        let mut seen = HashSet::new();
        for cand in raw_candidates {
            if seen.insert(cand.word.clone()) {
                deduped.push(cand);
            }
        }
        EnglishTrackResult {
            candidates: deduped,
            has_exact_match,
        }
    }

    /// 實作 GEMINI.md 規範之「粵拼 + 英文反查雙軌混排演算法」：
    /// 1. 短碼撞碼時（如 `sun`, `man`, `wing`：長度 <= 4 且同時有完整粵拼字與精確英文反查結果）：
    ///    - 前 2 位放最高頻粵拼字（index 0, 1）
    ///    - 第 3 至 4 位強制穿插英文直譯結果（index 2, 3）
    ///    - 第 5 位起依序放剩餘粵拼字與剩餘英文結果。
    /// 2. 純英文或長單字（如 `afternoon`, `lift`, `strawberry` 或前綴 `aftern`）：
    ///    - 英文反查結果直接排第 1 順位。
    fn merge_dual_tracks(
        &self,
        cleaned: &str,
        canto_res: CantoneseTrackResult,
        eng_res: EnglishTrackResult,
        limit: usize,
    ) -> Vec<Candidate> {
        let mut final_list = Vec::with_capacity(limit);
        let mut seen_words = HashSet::new();

        let mut push_unique = |cand: Candidate, out: &mut Vec<Candidate>| {
            if out.len() < limit && seen_words.insert(cand.word.clone()) {
                out.push(cand);
            }
        };

        let is_short_collision =
            cleaned.len() <= 4 && canto_res.has_any_full_syllable && eng_res.has_exact_match;

        let is_pure_or_long_english = !eng_res.candidates.is_empty()
            && ((!canto_res.has_any_full_syllable && !canto_res.has_strong_abbr_match)
                || (cleaned.len() >= 5 && !canto_res.has_exact_or_normalized_full));

        if is_short_collision {
            let mut canto_iter = canto_res.candidates.into_iter();
            let mut eng_iter = eng_res.candidates.into_iter();

            // 前 2 位：最高頻粵拼字
            for _ in 0..2 {
                if let Some(c) = canto_iter.next() {
                    push_unique(c, &mut final_list);
                }
            }
            // 第 3~4 位：強制穿插英文反查結果
            for _ in 0..2 {
                if let Some(e) = eng_iter.next() {
                    push_unique(e, &mut final_list);
                }
            }
            // 後續：剩餘粵拼候選詞與剩餘英文候選詞
            for c in canto_iter {
                push_unique(c, &mut final_list);
            }
            for e in eng_iter {
                push_unique(e, &mut final_list);
            }
        } else if is_pure_or_long_english {
            // 純英文或長單字（如 afternoon, aftern, lift, strawberry）：英文反查直接排第 1 順位
            for e in eng_res.candidates {
                push_unique(e, &mut final_list);
            }
            for c in canto_res.candidates {
                push_unique(c, &mut final_list);
            }
        } else {
            // 一般粵語查詢（含標準粵拼、長詞粵拼、懶音、簡拼）
            for c in canto_res.candidates {
                push_unique(c, &mut final_list);
            }
            for e in eng_res.candidates {
                push_unique(e, &mut final_list);
            }
        }

        final_list
    }
}

/// 計算字串前綴查詢的上界（例如 "aftern" -> "aftero"），用於 SQLite B-Tree 索引範圍查找
pub(crate) fn next_prefix_bound(prefix: &str) -> Option<String> {
    let mut bytes = prefix.as_bytes().to_vec();
    while let Some(last) = bytes.pop() {
        if last < b'z' {
            bytes.push(last + 1);
            return String::from_utf8(bytes).ok();
        }
    }
    None
}
