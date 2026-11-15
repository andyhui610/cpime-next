//! CPIME-Next 智慧自造詞滑動視窗與廣東話虛詞防火牆 (`phrase_composer.rs`)
//!
//! 實作 `GEMINI.md` 第 5 節規範之「智慧自造詞自動合成（Auto-Phrase Composition）」演算法：
//! 1. 內建 `CANTONESE_STOPWORDS` 廣東話高頻虛詞、語氣詞、代名詞與結構字過濾表。
//! 2. 維護最多 4 個實詞單字的滑動視窗 `buffer: Vec<CharToken>`。
//! 3. 支援超過 `4000ms` 自動斷詞、遇到廣東話虛詞時立即結算前序緩衝區並阻擋虛詞入列、
//!    以及選中英文反查詞/多字詞/Backspace 時透過 `break_sequence()` 切斷序列。
//! 4. 結合 `UserDict::observe_custom_phrase` 實作兩階段自造詞觀察（1 次暫存觀察、2 次正式轉正）。

use std::time::{SystemTime, UNIX_EPOCH};

use crate::phonetics::sanitize_input;
use crate::user_dict::UserDict;

/// 滑動視窗最大單字容量（2~4 字自造詞）
pub const MAX_BUFFER_LEN: usize = 4;

/// 連續選字最大允許時間間隔（毫秒），超過則視為不同輸入片段並先結算清空舊緩衝區
pub const MAX_CHAR_INTERVAL_MS: u64 = 4_000;

/// 廣東話虛詞、語氣詞、代名詞與高頻結構字防火牆集合
pub const CANTONESE_STOPWORDS: [char; 47] = [
    '我', '你', '佢', '哋', '呢', '嗰', '邊', '嘅', '喺', '係', '唔', '咗', '啲', '咁', '咩', '嘢',
    '㗎', '啦', '囉', '喎', '啫', '吖', '呀', '吧', '嗎', '有', '冇', '去', '嚟', '睇', '食', '做',
    '好', '同', '和', '與', '但', '就', '都', '仲', '又', '會', '要', '個', '隻', '件', '次',
];

/// 判斷指定字元是否屬於廣東話虛詞/語氣詞/代名詞/高頻結構字
#[inline]
pub fn is_cantonese_stopword(ch: char) -> bool {
    CANTONESE_STOPWORDS.contains(&ch)
}

/// 判斷是否為有效中日韓統一表意文字（含擴展區如香港口語字 𨋢、𥄫、㗎）
#[inline]
pub fn is_cjk_ideograph(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0x20000..=0x323AF
    )
}

/// 取得當前系統毫秒時間戳
pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 滑動視窗中的單字節點
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharToken {
    /// 選中的單字
    pub ch: char,
    /// 標準無聲調全拼（如 `"hing"`）
    pub toneless: String,
    /// 聲母簡拼（如 `"h"`）
    pub abbr: String,
    /// 選字當下的毫秒時間戳
    pub timestamp_ms: u64,
}

/// 自造詞觀察或轉正通知事件（供 REPL 或前端顯示提示）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseCompositionEvent {
    /// 合成的中文詞組（如 `"興如"`）
    pub zh_phrase: String,
    /// 合成的標準無聲調全拼（如 `"hingjyu"`）
    pub toneless: String,
    /// 合成的首字母簡拼（如 `"hj"`）
    pub abbr: String,
    /// 是否已在此次觀察後正式轉正（`freq >= 2`）
    pub promoted: bool,
    /// 當前累計觀察次數
    pub freq: i32,
}

/// 智慧自造詞滑動視窗合成器
#[derive(Debug, Clone, Default)]
pub struct PhraseComposer {
    /// 當前連續實詞單字滑動視窗（上限 4 個字）
    pub buffer: Vec<CharToken>,
    /// 記錄 `buffer` 中已完成後綴子序列觀察的進度索引，避免 `flush()` 時重複計算同一組字
    settled_len: usize,
    /// 最近一次操作產生的自造詞觀察與轉正事件
    pub last_events: Vec<PhraseCompositionEvent>,
}

impl PhraseComposer {
    /// 建立新的 `PhraseComposer`
    pub fn new() -> Self {
        Self::default()
    }

    /// 當使用者透過粵拼（含全拼、簡拼、多制式或懶音）選中 1 個單字時呼叫（自動取當前時間戳）
    pub fn on_char_selected(
        &mut self,
        ch: char,
        hint_input: &str,
        fallback_code: &str,
        user_dict: &mut UserDict,
    ) -> Vec<PhraseCompositionEvent> {
        let now_ms = current_timestamp_ms();
        self.on_char_selected_at(ch, hint_input, fallback_code, now_ms, user_dict)
    }

    /// 當使用者透過粵拼選中 1 個單字時呼叫（可指定時間戳 `timestamp_ms`，方便單元測試驗證 4000ms 超時邊界）
    pub fn on_char_selected_at(
        &mut self,
        ch: char,
        hint_input: &str,
        fallback_code: &str,
        timestamp_ms: u64,
        user_dict: &mut UserDict,
    ) -> Vec<PhraseCompositionEvent> {
        self.last_events.clear();

        // 1. 若距離上一個字超過 4000ms，先呼叫 flush() 結算並清空舊緩衝區
        if let Some(prev_token) = self.buffer.last() {
            if timestamp_ms.saturating_sub(prev_token.timestamp_ms) > MAX_CHAR_INTERVAL_MS {
                let mut flushed_events = self.flush(user_dict);
                self.last_events.append(&mut flushed_events);
            }
        }

        // 2. 若該單字為標點/非漢字或屬於 CANTONESE_STOPWORDS，立即呼叫 flush() 結算前序緩衝區並清空（不將虛詞放入緩衝區）
        if !is_cjk_ideograph(ch) || is_cantonese_stopword(ch) {
            let mut flushed_events = self.flush(user_dict);
            self.last_events.append(&mut flushed_events);
            return self.last_events.clone();
        }

        // 3. 若為一般實詞單字，反查其在系統詞庫中的標準無聲調粵拼（如簡拼 "h" 選「興」還原為 "hing"）
        let (toneless, abbr) = user_dict
            .lookup_canonical_char_phonetics(ch, hint_input)
            .unwrap_or_else(|| {
                let fb = sanitize_input(fallback_code);
                let clean = if !fb.is_empty() {
                    fb
                } else {
                    sanitize_input(hint_input)
                };
                let first_char = clean
                    .chars()
                    .next()
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                (clean, first_char)
            });

        if toneless.is_empty() {
            self.break_sequence();
            return Vec::new();
        }

        let abbr_str = if !abbr.is_empty() {
            abbr
        } else {
            toneless
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default()
        };

        // 4. 推入 buffer（若推入後 buffer.len() > 4 則移除最舊的第 1 個字）
        self.buffer.push(CharToken {
            ch,
            toneless,
            abbr: abbr_str,
            timestamp_ms,
        });
        while self.buffer.len() > MAX_BUFFER_LEN {
            self.buffer.remove(0);
            self.settled_len = self.settled_len.saturating_sub(1);
        }

        // 5. 針對 buffer 內以新單字結尾的 2~4 字後綴子序列進行兩階段自造詞觀察
        let mut suffix_events = self.settle_pending_spans(user_dict);
        self.last_events.append(&mut suffix_events);
        self.last_events.clone()
    }

    /// 結算 `buffer` 中連續 2~4 個單字的組合，呼叫 `user_dict.observe_custom_phrase(...)` 並清空緩衝區
    pub fn flush(&mut self, user_dict: &mut UserDict) -> Vec<PhraseCompositionEvent> {
        let events = self.settle_pending_spans(user_dict);
        self.buffer.clear();
        self.settled_len = 0;
        events
    }

    /// 當使用者按 Backspace、輸入英文反查詞或選中多字詞時呼叫，立即清空緩衝區切斷合成序列
    pub fn break_sequence(&mut self) {
        self.buffer.clear();
        self.settled_len = 0;
        self.last_events.clear();
    }

    /// 內部輔助：將 `buffer` 中尚未結算的連續 2~4 字後綴組合送入 `user_dict.observe_custom_phrase`
    fn settle_pending_spans(&mut self, user_dict: &mut UserDict) -> Vec<PhraseCompositionEvent> {
        let mut events = Vec::new();
        if self.buffer.len() < 2 {
            self.settled_len = self.buffer.len();
            return events;
        }

        let start_end = (self.settled_len + 1).max(2);
        for end in start_end..=self.buffer.len() {
            for span_len in 2..=MAX_BUFFER_LEN {
                if end < span_len {
                    continue;
                }
                let slice = &self.buffer[end - span_len..end];
                let zh_phrase: String = slice.iter().map(|t| t.ch).collect();
                let toneless: String = slice.iter().map(|t| t.toneless.as_str()).collect();
                let abbr: String = slice.iter().map(|t| t.abbr.as_str()).collect();

                if let Some((promoted, freq)) =
                    user_dict.observe_custom_phrase_with_freq(&toneless, &abbr, &zh_phrase)
                {
                    events.push(PhraseCompositionEvent {
                        zh_phrase,
                        toneless,
                        abbr,
                        promoted,
                        freq,
                    });
                }
            }
        }

        self.settled_len = self.buffer.len();
        events
    }
}

