//! CPIME-Next DAG 詞圖與 Viterbi 整句連打解碼器 (`decoder.rs`)
//!
//! 實作 `GEMINI.md` 第 8 節及第 10 節規範之「兩層式混合 DAG 詞圖建構 + 簡拼錨點檢索 + Viterbi 動態規劃 + 步進式部分選字前綴提取」：
//! 1. 根據區間 `[i..j]` 音節型態分流查詢：
//!    - 情況 A（全為 `Full`）：批次 SQL (`WHERE toneless IN (...)`) 查詢 `idx_yue_toneless`、`user_unigrams` 與書面語映射表。
//!    - 情況 B（全為 `Abbr` 且跨度 >= 2）：以 `abbr_key` 查詢 `cantonese_dict` 與 `user_unigrams`（支援純簡拼 `"ng"` 歧義展開）。
//!    - 情況 C（`Full` + `Abbr` 混合）：以 `idx_yue_abbr` 撈取前 50 筆候選詞後，於記憶體核對 `Full` 音節位置之無聲調拼音（含模糊音容錯）。
//!    - 情況 D（單一 `Abbr`）：查詢 `WHERE abbr = ? AND length(zh_word) = 1` 最高頻單字作為單字節點保底。
//! 2. 套用邊權重公式：
//!    `edge_score = word_weight + user_boost + (full_count * 1200) + (abbr_count * 300) + ((j - i)^2 * 3500) - 2000`
//!    並在命中 `user_bigrams` 時額外加上 `+6000` 轉移加分。
//! 3. 同步產出從第 0 個音節開始的開頭前綴候選詞（含 `consumed_chars`），供前端「剝洋蔥式」部分選字上屏。

use std::collections::{HashMap, HashSet};

use rusqlite::{params, params_from_iter, Connection};

use crate::engine::{
    written_cantonese_mapping, Candidate, CandidateSource, OutputMode,
};
use crate::phonetics::{expand_phonetic_queries, SyllableKind, SyllableSpan};
use crate::user_dict::{UserDict, UserUnigramEntry};

/// 單一詞圖邊的最大音節跨度
const MAX_LATTICE_SPAN: usize = 6;

/// 每個音節跨度區間最多保留的候選詞數量
const TOP_K_PER_SPAN: usize = 5;

/// 3 音節以上純簡拼與混合簡拼區間只保留高頻詞條的最低權重門檻
/// （避免如「今日好焗 `5200`」或「機緣巧合 `5200`」這類長詞靠跨度平方加分壓過「今日 `10000` + 香港 `10000`」）
const ABBR_LONG_SPAN_MIN_WEIGHT: i32 = 8_000;

/// 區間型態分類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanMode {
    AllFull,
    AllAbbrMulti,
    Hybrid,
    SingleAbbr,
}

/// 詞圖區間查詢摘要
#[derive(Debug, Clone)]
struct SpanDescriptor {
    mode: SpanMode,
    /// 供 `user_bigrams` 與 `written_cantonese_mapping` 使用的查詢碼（全拼為 `toneless_key`，其餘為 `abbr_key`）
    lookup_key: String,
    /// 若全為 `Full` 且包含短 `-a` 音節（如 `"ma"`），額外準備正規化為 `-aa` 的鍵（如 `"maa"`）
    alt_full_key: Option<String>,
    /// 對應此區間內各音節首字母組成的簡拼字串
    abbr_key: String,
    full_count: i64,
    abbr_count: i64,
}

/// 詞圖有向邊中的單一候選詞條
#[derive(Debug, Clone)]
pub struct LatticeWord {
    pub word: String,
    pub matched_code: String,
    pub db_weight: i32,
    pub user_boost: i32,
    pub source: CandidateSource,
}

/// Viterbi 整句解碼結果與部分選字前綴候選清單
#[derive(Debug, Clone)]
pub struct DecodedSentence {
    /// Viterbi 最佳路徑組出的完整漢字長句
    pub full_sentence: String,
    /// 對應的整句拼音字串（以空白分隔各詞/音節）
    pub matched_code: String,
    /// 整句累計總分
    pub total_score: i32,
    /// 最佳路徑是否只由單一整個詞條 `[0..N]` 構成
    pub is_single_whole_word: bool,
    /// 從第 0 個音節開始的前綴候選詞清單（已依跨度與分數排序，含 `consumed_chars`）
    pub prefix_candidates: Vec<Candidate>,
}

/// 廣東話長句連打解碼器（DAG 詞圖 + Viterbi 演算法）
pub struct SentenceDecoder;

impl SentenceDecoder {
    /// 根據已切分之音節序列 `syllables`（長度 $N \ge 2$）建構詞圖並執行 Viterbi 解碼
    pub fn decode(
        conn: &Connection,
        user_dict: &UserDict,
        output_mode: OutputMode,
        last_committed_word: Option<&str>,
        raw_input: &str,
        syllables: &[SyllableSpan],
    ) -> Option<DecodedSentence> {
        let n = syllables.len();
        if n < 2 {
            return None;
        }

        // 1. 分類所有合法跨度 [i..j] (1 <= j - i <= min(6, N))，並收集批次 SQL 所需的查詢鍵
        let mut span_descs: Vec<Vec<Option<SpanDescriptor>>> = vec![vec![None; n + 1]; n];
        let mut full_keys: Vec<String> = Vec::new();
        let mut seen_full_keys: HashSet<String> = HashSet::new();
        let mut abbr_keys: Vec<String> = Vec::new();
        let mut seen_abbr_keys: HashSet<String> = HashSet::new();
        let mut single_abbr_chars: Vec<String> = Vec::new();
        let mut seen_single_abbr: HashSet<String> = HashSet::new();
        let mut all_user_keys: Vec<String> = Vec::new();
        let mut seen_user_keys: HashSet<String> = HashSet::new();

        let mut push_user_key = |key: &str| {
            if !key.is_empty() && seen_user_keys.insert(key.to_string()) {
                all_user_keys.push(key.to_string());
            }
        };

        for i in 0..n {
            let max_j = (i + MAX_LATTICE_SPAN).min(n);
            for j in (i + 1)..=max_j {
                let slice = &syllables[i..j];
                let mut full_count = 0i64;
                let mut abbr_count = 0i64;
                let mut abbr_key = String::with_capacity(slice.len());

                for s in slice {
                    abbr_key.push(s.abbr_char);
                    match s.kind {
                        SyllableKind::Full(_) => full_count += 1,
                        SyllableKind::Abbr(_) => abbr_count += 1,
                    }
                }

                let mode = if abbr_count == 0 {
                    SpanMode::AllFull
                } else if full_count == 0 {
                    if slice.len() == 1 {
                        SpanMode::SingleAbbr
                    } else {
                        SpanMode::AllAbbrMulti
                    }
                } else {
                    SpanMode::Hybrid
                };

                let (lookup_key, alt_full_key) = match mode {
                    SpanMode::AllFull => {
                        let mut code = String::new();
                        let mut alt_code = String::new();
                        let mut has_alt = false;
                        for s in slice {
                            code.push_str(&s.normalized_jyutping);
                            if s.normalized_jyutping.len() == 2
                                && s.normalized_jyutping.ends_with('a')
                                && !s.normalized_jyutping.ends_with("aa")
                            {
                                alt_code.push_str(&s.normalized_jyutping);
                                alt_code.push('a');
                                has_alt = true;
                            } else {
                                alt_code.push_str(&s.normalized_jyutping);
                            }
                        }
                        if seen_full_keys.insert(code.clone()) {
                            full_keys.push(code.clone());
                        }
                        push_user_key(&code);
                        let alt = if has_alt {
                            if seen_full_keys.insert(alt_code.clone()) {
                                full_keys.push(alt_code.clone());
                            }
                            push_user_key(&alt_code);
                            Some(alt_code)
                        } else {
                            None
                        };
                        (code, alt)
                    }
                    SpanMode::AllAbbrMulti => {
                        if seen_abbr_keys.insert(abbr_key.clone()) {
                            abbr_keys.push(abbr_key.clone());
                        }
                        push_user_key(&abbr_key);
                        // 純簡拼 "ng" 歧義展開：在詞圖中同時支援單一音節 "ng" / "ngo"
                        if slice.len() == 2 && abbr_key == "ng" {
                            for extra in ["ng", "ngo"] {
                                if seen_full_keys.insert(extra.to_string()) {
                                    full_keys.push(extra.to_string());
                                }
                                push_user_key(extra);
                            }
                        }
                        (abbr_key.clone(), None)
                    }
                    SpanMode::Hybrid => {
                        if seen_abbr_keys.insert(abbr_key.clone()) {
                            abbr_keys.push(abbr_key.clone());
                        }
                        push_user_key(&abbr_key);
                        (abbr_key.clone(), None)
                    }
                    SpanMode::SingleAbbr => {
                        let single = slice[0].normalized_jyutping.clone();
                        if seen_single_abbr.insert(single.clone()) {
                            single_abbr_chars.push(single.clone());
                        }
                        push_user_key(&single);
                        (single, None)
                    }
                };

                span_descs[i][j] = Some(SpanDescriptor {
                    mode,
                    lookup_key,
                    alt_full_key,
                    abbr_key,
                    full_count,
                    abbr_count,
                });
            }
        }

        // 2. 批次從系統詞庫 cantonese_dict 撈取四類候選詞條：
        //    - 情況 A：全拼區間 (toneless IN (...))
        //    - 情況 B / C：多音節簡拼與混合區間錨點池 (abbr = ? ORDER BY weight DESC LIMIT 50)
        //    - 情況 D：單一簡拼單字保底 (abbr = ? AND length(zh_word) = 1)
        let sys_full_map = Self::batch_query_cantonese_dict(conn, &full_keys);
        let sys_abbr_pool = Self::query_abbr_pools(conn, &abbr_keys);
        let single_abbr_map = Self::query_single_abbr_chars(conn, &single_abbr_chars);

        // 3. 批次從 user_dict 撈取相關個人詞頻 (user_unigrams) 與上下文連帶 (user_bigrams)
        let uni_map = user_dict.query_unigrams_batch(&all_user_keys);
        let bi_map = user_dict.query_bigrams_batch(&all_user_keys);

        // 快取每個 Full 音節位置展開後的多制式/懶音變體集合，加速情況 C 混合過濾
        let full_variants_cache: Vec<Option<HashSet<String>>> = syllables
            .iter()
            .map(|s| match &s.kind {
                SyllableKind::Full(jp) => {
                    let mut set: HashSet<String> = expand_phonetic_queries(jp)
                        .into_iter()
                        .map(|q| q.jyutping)
                        .collect();
                    set.insert(jp.clone());
                    Some(set)
                }
                SyllableKind::Abbr(_) => None,
            })
            .collect();

        // 4. 組裝每個區間 [i..j] 的詞圖候選清單
        let mut lattice: Vec<Vec<Vec<LatticeWord>>> = vec![vec![Vec::new(); n + 1]; n];
        for i in 0..n {
            let max_j = (i + MAX_LATTICE_SPAN).min(n);
            for j in (i + 1)..=max_j {
                let Some(ref desc) = span_descs[i][j] else {
                    continue;
                };
                let slice = &syllables[i..j];
                let mut words: Vec<LatticeWord> = Vec::with_capacity(TOP_K_PER_SPAN + 2);
                let mut seen_words: HashSet<String> = HashSet::new();

                match desc.mode {
                    SpanMode::AllFull => {
                        // 情況 A：全為 Full 音節
                        if output_mode == OutputMode::Written {
                            let written_hits = written_cantonese_mapping(&desc.lookup_key)
                                .or_else(|| {
                                    desc.alt_full_key
                                        .as_deref()
                                        .and_then(written_cantonese_mapping)
                                });
                            if let Some(written_words) = written_hits {
                                for (idx, &w) in written_words.iter().enumerate() {
                                    if seen_words.insert(w.to_string()) {
                                        words.push(LatticeWord {
                                            word: w.to_string(),
                                            matched_code: desc.lookup_key.clone(),
                                            db_weight: 12_000 - (idx as i32 * 100),
                                            user_boost: 0,
                                            source: CandidateSource::Jyutping,
                                        });
                                    }
                                }
                            }
                        }

                        if let Some(entries) = sys_full_map.get(&desc.lookup_key) {
                            for lw in entries {
                                if seen_words.insert(lw.word.clone()) {
                                    words.push(lw.clone());
                                }
                            }
                        }
                        if let Some(ref alt) = desc.alt_full_key {
                            if let Some(entries) = sys_full_map.get(alt) {
                                for lw in entries {
                                    if seen_words.insert(lw.word.clone()) {
                                        words.push(lw.clone());
                                    }
                                }
                            }
                        }

                        Self::merge_user_unigrams(
                            &mut words,
                            &mut seen_words,
                            uni_map.get(&desc.lookup_key),
                            &desc.lookup_key,
                        );
                        if let Some(ref alt) = desc.alt_full_key {
                            Self::merge_user_unigrams(
                                &mut words,
                                &mut seen_words,
                                uni_map.get(alt),
                                alt,
                            );
                        }
                    }

                    SpanMode::AllAbbrMulti => {
                        // 情況 B：全為 Abbr 且跨度 >= 2
                        if let Some(pool) = sys_abbr_pool.get(&desc.abbr_key) {
                            let span_len = j - i;
                            for entry in pool {
                                if entry.syllables.len() != span_len {
                                    continue;
                                }
                                if span_len >= 3 && entry.weight < ABBR_LONG_SPAN_MIN_WEIGHT {
                                    continue;
                                }
                                if seen_words.insert(entry.word.clone()) {
                                    words.push(LatticeWord {
                                        word: entry.word.clone(),
                                        matched_code: entry.toned_jp.clone(),
                                        db_weight: entry.weight,
                                        user_boost: 0,
                                        source: CandidateSource::Jyutping,
                                    });
                                    if words.len() >= TOP_K_PER_SPAN + 2 {
                                        break;
                                    }
                                }
                            }
                        }

                        // 純簡拼 "ng" 歧義展開：同時支援單一音節 "ng" / "ngo" 的單字（如「我」「五」）
                        if slice.len() == 2 && desc.abbr_key == "ng" {
                            for extra in ["ngo", "ng"] {
                                if let Some(entries) = sys_full_map.get(extra) {
                                    for lw in entries {
                                        if lw.word.chars().count() == 1
                                            && seen_words.insert(lw.word.clone())
                                        {
                                            words.push(lw.clone());
                                        }
                                    }
                                }
                                Self::merge_user_unigrams(
                                    &mut words,
                                    &mut seen_words,
                                    uni_map.get(extra),
                                    extra,
                                );
                            }
                        }

                        Self::merge_user_unigrams(
                            &mut words,
                            &mut seen_words,
                            uni_map.get(&desc.abbr_key),
                            &desc.abbr_key,
                        );
                    }

                    SpanMode::Hybrid => {
                        // 情況 C：Full + Abbr 混合區間，先以 abbr_key 撈出前 50 筆再逐音節核對 Full 錨點
                        if let Some(pool) = sys_abbr_pool.get(&desc.abbr_key) {
                            let span_len = j - i;
                            for entry in pool {
                                if entry.syllables.len() != span_len {
                                    continue;
                                }
                                // 若混合區間跨度 >= 3 且有簡拼，僅納入高頻詞條（避免低頻固定長詞如「今日好焗」壓過「今日 + 香港」）
                                if span_len >= 3 && entry.weight < ABBR_LONG_SPAN_MIN_WEIGHT {
                                    continue;
                                }
                                if Self::matches_hybrid_anchors(
                                    &entry.syllables,
                                    &full_variants_cache[i..j],
                                ) && seen_words.insert(entry.word.clone())
                                {
                                    words.push(LatticeWord {
                                        word: entry.word.clone(),
                                        matched_code: entry.toned_jp.clone(),
                                        db_weight: entry.weight,
                                        user_boost: 0,
                                        source: CandidateSource::Jyutping,
                                    });
                                    if words.len() >= TOP_K_PER_SPAN + 2 {
                                        break;
                                    }
                                }
                            }
                        }

                        // 同步比對個人學習詞庫中的自造詞/記憶詞條
                        if let Some(uni_entries) = uni_map.get(&desc.abbr_key) {
                            let filtered: Vec<UserUnigramEntry> = uni_entries
                                .iter()
                                .filter(|e| {
                                    Self::user_entry_matches_hybrid(e, slice, &full_variants_cache[i..j])
                                })
                                .cloned()
                                .collect();
                            if !filtered.is_empty() {
                                Self::merge_user_unigrams(
                                    &mut words,
                                    &mut seen_words,
                                    Some(&filtered),
                                    &desc.abbr_key,
                                );
                            }
                        }
                    }

                    SpanMode::SingleAbbr => {
                        // 情況 D：單一 Abbr 音節 [i..i+1]，查詢最高頻單字作為單字節點保底
                        if let Some(entries) = single_abbr_map.get(&desc.lookup_key) {
                            for lw in entries {
                                if seen_words.insert(lw.word.clone()) {
                                    words.push(lw.clone());
                                }
                            }
                        }
                        Self::merge_user_unigrams(
                            &mut words,
                            &mut seen_words,
                            uni_map.get(&desc.lookup_key),
                            &desc.lookup_key,
                        );
                    }
                }

                // 若單音節區間仍完全無候選詞，提供原始拼音保底邊確保詞圖連通
                if words.is_empty() && j == i + 1 {
                    let fallback = raw_input
                        .get(syllables[i].start_byte..syllables[i].end_byte)
                        .unwrap_or(desc.lookup_key.as_str())
                        .to_string();
                    words.push(LatticeWord {
                        word: fallback,
                        matched_code: desc.lookup_key.clone(),
                        db_weight: 100,
                        user_boost: 0,
                        source: CandidateSource::Jyutping,
                    });
                }

                // 依綜合權重降序排列，並保留前 TOP_K_PER_SPAN 條參與 Viterbi 路徑計算
                words.sort_by(|a, b| (b.db_weight + b.user_boost).cmp(&(a.db_weight + a.user_boost)));
                if words.len() > TOP_K_PER_SPAN {
                    words.truncate(TOP_K_PER_SPAN);
                }
                lattice[i][j] = words;
            }
        }

        // 5. 執行 Viterbi 動態規劃
        #[derive(Clone)]
        struct DpState {
            score: i64,
            prev_idx: usize,
            chosen_word: String,
            chosen_code: String,
        }

        let mut dp: Vec<Option<DpState>> = vec![None; n + 1];
        dp[0] = Some(DpState {
            score: 0,
            prev_idx: 0,
            chosen_word: last_committed_word.unwrap_or("").to_string(),
            chosen_code: String::new(),
        });

        for j in 1..=n {
            let min_i = j.saturating_sub(MAX_LATTICE_SPAN);
            for i in min_i..j {
                let Some(ref desc) = span_descs[i][j] else {
                    continue;
                };
                let candidates = &lattice[i][j];
                if candidates.is_empty() {
                    continue;
                }
                let (prev_score, prev_idx, prev_word, prev_code) = match dp[i].as_ref() {
                    Some(s) => (
                        s.score,
                        s.prev_idx,
                        s.chosen_word.clone(),
                        s.chosen_code.clone(),
                    ),
                    None => continue,
                };

                // 若為純簡拼 "ng" 展開出的單一漢字（如 [Abbr("n"), Abbr("g")] -> 「我」），其語意跨度為 1 個漢字而非 2 個漢字
                let span = (j - i) as i64;
                let is_ng_single_char_span = desc.mode == SpanMode::AllAbbrMulti
                    && desc.abbr_key == "ng"
                    && j - i == 2;
                let effective_span = |lw: &LatticeWord| -> i64 {
                    if is_ng_single_char_span && lw.word.chars().count() == 1 {
                        1
                    } else {
                        span
                    }
                };

                // 對單音節起點 i==1 保留所有同級候選字（如「你」「我」）以便搭配下一詞的語義轉移分選出最佳首字
                let mut prev_alternatives: Vec<(i64, usize, String, String)> = Vec::new();
                if prev_idx == 0 && i == 1 {
                    for alt_lw in &lattice[0][1] {
                        let alt_span = 1i64;
                        let alt_kind = span_descs[0][1]
                            .as_ref()
                            .map(|d| (d.full_count * 1_200) + (d.abbr_count * 300))
                            .unwrap_or(0);
                        let alt_score = (alt_lw.db_weight as i64) * alt_span
                            + (alt_lw.user_boost as i64)
                            + ((alt_span * alt_span * 3_500) - 2_000)
                            + alt_kind;
                        prev_alternatives.push((
                            alt_score,
                            0,
                            alt_lw.word.clone(),
                            alt_lw.matched_code.clone(),
                        ));
                    }
                }
                if prev_alternatives.is_empty() {
                    prev_alternatives.push((prev_score, prev_idx, prev_word.clone(), prev_code));
                }

                for (alt_prev_score, _alt_prev_idx, alt_prev_word, alt_prev_code) in
                    &prev_alternatives
                {
                    let bigram_hits = if !alt_prev_word.is_empty() {
                        bi_map
                            .get(&(alt_prev_word.clone(), desc.lookup_key.clone()))
                            .or_else(|| {
                                desc.alt_full_key
                                    .as_ref()
                                    .and_then(|alt| bi_map.get(&(alt_prev_word.clone(), alt.clone())))
                            })
                    } else {
                        None
                    };

                    for lw in candidates {
                        let eff_span = effective_span(lw);
                        let span_bonus = (eff_span * eff_span * 3_500) - 2_000;
                        let kind_bonus = if is_ng_single_char_span && eff_span == 1 {
                            // 當 "ng" 展開為單一音節（如 "ng" + "gjhhg" -> 我今日去香港）時，
                            // 必須確保高頻單字「我」分數高於同區間 2 字低頻詞（如 "𠸎家" weight=6200 -> 25000）
                            15_000
                        } else {
                            (desc.full_count * 1_200) + (desc.abbr_count * 300)
                        };
                        let word_weight = (lw.db_weight as i64) * eff_span;
                        let user_boost = lw.user_boost as i64;
                        let bigram_bonus = if bigram_hits.is_some_and(|set| set.contains(&lw.word)) {
                            6_000
                        } else {
                            0
                        };
                        let collocation_bonus =
                            Self::builtin_collocation_bonus(alt_prev_word, &lw.word);
                        let edge_score = word_weight
                            + user_boost
                            + span_bonus
                            + kind_bonus
                            + bigram_bonus
                            + collocation_bonus;
                        let total = alt_prev_score + edge_score;
                        let should_update = match &dp[j] {
                            None => true,
                            Some(existing) => total > existing.score,
                        };
                        if should_update {
                            if i == 1 && prev_idx == 0 && *alt_prev_word != prev_word {
                                dp[1] = Some(DpState {
                                    score: *alt_prev_score,
                                    prev_idx: 0,
                                    chosen_word: alt_prev_word.clone(),
                                    chosen_code: alt_prev_code.clone(),
                                });
                            }
                            dp[j] = Some(DpState {
                                score: total,
                                prev_idx: i,
                                chosen_word: lw.word.clone(),
                                chosen_code: lw.matched_code.clone(),
                            });
                        }
                    }
                }
            }
        }

        let final_state = dp[n].as_ref()?;
        let total_score = final_state.score.clamp(i32::MIN as i64, i32::MAX as i64) as i32;

        // 6. 從 dp[N] 反向回溯組出整句漢字與路徑邊界
        let mut word_segments: Vec<String> = Vec::new();
        let mut code_segments: Vec<String> = Vec::new();
        let mut span_bounds: Vec<(usize, usize)> = Vec::new();
        let mut cur = n;
        while cur > 0 {
            let state = dp[cur].as_ref()?;
            word_segments.push(state.chosen_word.clone());
            code_segments.push(state.chosen_code.clone());
            span_bounds.push((state.prev_idx, cur));
            cur = state.prev_idx;
        }
        word_segments.reverse();
        code_segments.reverse();
        span_bounds.reverse();

        let full_sentence: String = word_segments.concat();
        let matched_code: String = code_segments.join(" ");
        let is_single_whole_word = span_bounds.len() == 1;

        // 7. 生成從第 0 個音節開始的前綴候選詞（供步進式部分選字：優先放入最佳路徑的首個開頭詞）
        let mut prefix_candidates: Vec<Candidate> = Vec::new();
        let mut seen_prefix_words: HashSet<String> = HashSet::new();
        seen_prefix_words.insert(full_sentence.clone());

        let mut push_span_candidates = |k: usize| {
            if k == 0 || k >= n {
                return;
            }
            let consumed_chars = syllables[k - 1].end_byte;
            let span_i64 = k as i32;
            let kind_bonus = span_descs[0][k]
                .as_ref()
                .map(|d| (d.full_count * 1_200 + d.abbr_count * 300) as i32)
                .unwrap_or(0);
            let span_bonus = (span_i64 * span_i64 * 3_500) - 2_000 + kind_bonus;

            for lw in &lattice[0][k] {
                if seen_prefix_words.insert(lw.word.clone()) {
                    let score = (lw.db_weight * span_i64) + lw.user_boost + span_bonus;
                    prefix_candidates.push(Candidate {
                        word: lw.word.clone(),
                        matched_code: lw.matched_code.clone(),
                        source: lw.source,
                        score,
                        consumed_chars,
                    });
                }
            }
        };

        // 若最佳路徑有多段詞，先納入第一段詞（確保如 "gamjathg" 的第 2 順位必為 "今日"）
        if let Some(&(first_start, first_end)) = span_bounds.first() {
            if first_start == 0 && first_end < n {
                push_span_candidates(first_end);
            }
        }

        let max_prefix_span = (n - 1).min(MAX_LATTICE_SPAN);
        for k in (1..=max_prefix_span).rev() {
            push_span_candidates(k);
        }

        Some(DecodedSentence {
            full_sentence,
            matched_code,
            total_score,
            is_single_whole_word,
            prefix_candidates,
        })
    }

    /// 驗證資料庫候選詞的各音節是否吻合混合區間內的所有 `Full` 音節錨點
    fn matches_hybrid_anchors(
        cand_syllables: &[String],
        full_variants_slice: &[Option<HashSet<String>>],
    ) -> bool {
        if cand_syllables.len() != full_variants_slice.len() {
            return false;
        }
        for (cand_syl, expected_opt) in cand_syllables.iter().zip(full_variants_slice.iter()) {
            if let Some(expected_set) = expected_opt {
                if !expected_set.contains(cand_syl.as_str()) {
                    return false;
                }
            }
        }
        true
    }

    /// 驗證個人詞庫詞條是否吻合混合區間內的 `Full` 音節錨點
    fn user_entry_matches_hybrid(
        entry: &UserUnigramEntry,
        slice: &[SyllableSpan],
        full_variants_slice: &[Option<HashSet<String>>],
    ) -> bool {
        if entry.zh_word.chars().count() != slice.len() {
            return false;
        }
        // 若所有 Full 音節片段均依序出現在無聲調全拼 `input_code` 中且剩餘長度合理，視為吻合
        let mut cursor = 0usize;
        for (idx, span) in slice.iter().enumerate() {
            match &full_variants_slice[idx] {
                Some(variants) => {
                    let rem = &entry.input_code[cursor..];
                    if let Some(matched) = variants.iter().find(|v| rem.starts_with(v.as_str())) {
                        cursor += matched.len();
                    } else {
                        return false;
                    }
                }
                None => {
                    let rem = &entry.input_code[cursor..];
                    if !rem.starts_with(span.abbr_char) {
                        return false;
                    }
                    // 跳至下一個音節的合理起點（純簡拼至少佔 1 個字元）
                    cursor += 1;
                    while cursor < entry.input_code.len()
                        && idx + 1 < slice.len()
                        && full_variants_slice[idx + 1]
                            .as_ref()
                            .is_some_and(|vs| !vs.iter().any(|v| entry.input_code[cursor..].starts_with(v.as_str())))
                    {
                        cursor += 1;
                    }
                }
            }
        }
        true
    }

    /// 將 `user_unigrams` 個人詞頻與轉正自造詞合併至詞圖區間候選清單
    fn merge_user_unigrams(
        words: &mut Vec<LatticeWord>,
        seen_words: &mut HashSet<String>,
        uni_opt: Option<&Vec<UserUnigramEntry>>,
        code: &str,
    ) {
        let Some(uni_entries) = uni_opt else {
            return;
        };
        let max_last_used = uni_entries.iter().map(|e| e.last_used).max();
        let mut boost_by_word: HashMap<&str, (i32, bool)> = HashMap::new();
        for e in uni_entries {
            let freq_bonus = (e.freq * 400).min(5_000);
            let latest_bonus = if Some(e.last_used) == max_last_used && e.last_used > 0 {
                3_500
            } else {
                0
            };
            let custom_bonus = if e.is_custom && e.freq >= 2 { 5_500 } else { 0 };
            boost_by_word.insert(
                e.zh_word.as_str(),
                (freq_bonus + latest_bonus + custom_bonus, e.is_custom),
            );
        }

        for lw in words.iter_mut() {
            if let Some(&(boost, is_custom)) = boost_by_word.get(lw.word.as_str()) {
                lw.user_boost += boost;
                if is_custom {
                    lw.source = CandidateSource::UserCustom;
                }
            }
        }

        for e in uni_entries {
            if seen_words.insert(e.zh_word.clone()) {
                let (boost, is_custom) = boost_by_word[e.zh_word.as_str()];
                words.push(LatticeWord {
                    word: e.zh_word.clone(),
                    matched_code: code.to_string(),
                    db_weight: 10_000,
                    user_boost: boost,
                    source: if is_custom {
                        CandidateSource::UserCustom
                    } else {
                        CandidateSource::Jyutping
                    },
                });
            }
        }
    }

    /// 輕量級廣東話常見語法詞性轉移加分（協助純簡拼長句如 `"gjhhg"` 在同權重單字間穩定選出正確動詞）
    #[inline]
    fn builtin_collocation_bonus(prev_word: &str, curr_word: &str) -> i64 {
        match (prev_word, curr_word) {
            ("今日" | "聽日" | "尋日" | "而家" | "我" | "你" | "佢", "去") => 500,
            ("去", "香港" | "屋企" | "返工" | "行街" | "邊度") => 500,
            ("我", "今日" | "聽日" | "尋日") => 400,
            ("你" | "佢", "今日" | "聽日" | "尋日" | "你好") => 200,
            ("你好", "嗎") => 400,
            _ => 0,
        }
    }

    /// 使用單次批次 SQL (`WHERE toneless IN (...)`) 從 `cantonese_dict` 撈取每個跨度音節碼的前 5 高分詞條
    fn batch_query_cantonese_dict(
        conn: &Connection,
        codes: &[String],
    ) -> HashMap<String, Vec<LatticeWord>> {
        let mut map: HashMap<String, Vec<LatticeWord>> = HashMap::new();
        if codes.is_empty() {
            return map;
        }

        let placeholders: String = (1..=codes.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT toneless, zh_word, jyutping_toned, weight
             FROM cantonese_dict
             WHERE toneless IN ({placeholders})
             ORDER BY weight DESC"
        );

        let Ok(mut stmt) = conn.prepare(&sql) else {
            return map;
        };
        let Ok(rows) = stmt.query_map(params_from_iter(codes.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
            ))
        }) else {
            return map;
        };

        let mut seen_per_code: HashMap<String, HashSet<String>> = HashMap::new();
        for (toneless, zh_word, toned_jp, weight) in rows.flatten() {
            if zh_word.is_ascii() {
                continue;
            }
            let list = map.entry(toneless.clone()).or_default();
            if list.len() >= TOP_K_PER_SPAN {
                continue;
            }
            let seen = seen_per_code.entry(toneless).or_default();
            if seen.insert(zh_word.clone()) {
                list.push(LatticeWord {
                    word: zh_word,
                    matched_code: toned_jp,
                    db_weight: weight,
                    user_boost: 0,
                    source: CandidateSource::Jyutping,
                });
            }
        }

        map
    }

    /// 以 `idx_yue_abbr` 高速撈取每個 `abbr_key` 的前 50 筆候選詞條並預先切分無聲調音節陣列
    fn query_abbr_pools(
        conn: &Connection,
        abbr_keys: &[String],
    ) -> HashMap<String, Vec<AbbrPoolEntry>> {
        let mut map: HashMap<String, Vec<AbbrPoolEntry>> = HashMap::new();
        if abbr_keys.is_empty() {
            return map;
        }

        let Ok(mut stmt) = conn.prepare_cached(
            "SELECT zh_word, jyutping_toned, weight
             FROM cantonese_dict
             WHERE abbr = ?1
             ORDER BY weight DESC
             LIMIT 50",
        ) else {
            return map;
        };

        for abbr in abbr_keys {
            let mut entries = Vec::new();
            if let Ok(rows) = stmt.query_map(params![abbr], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2)?,
                ))
            }) {
                for (zh_word, toned_jp, weight) in rows.flatten() {
                    if zh_word.is_ascii() {
                        continue;
                    }
                    let syllables: Vec<String> = toned_jp
                        .split_whitespace()
                        .map(|syl| {
                            syl.chars()
                                .filter(|c| c.is_ascii_alphabetic())
                                .map(|c| c.to_ascii_lowercase())
                                .collect()
                        })
                        .filter(|s: &String| !s.is_empty())
                        .collect();
                    entries.push(AbbrPoolEntry {
                        word: zh_word,
                        toned_jp,
                        syllables,
                        weight,
                    });
                }
            }
            map.insert(abbr.clone(), entries);
        }

        map
    }

    /// 查詢單一簡拼字母 (`WHERE abbr = ? AND length(zh_word) = 1`) 最高頻單字作為單字節點保底
    fn query_single_abbr_chars(
        conn: &Connection,
        abbr_chars: &[String],
    ) -> HashMap<String, Vec<LatticeWord>> {
        let mut map: HashMap<String, Vec<LatticeWord>> = HashMap::new();
        if abbr_chars.is_empty() {
            return map;
        }

        let Ok(mut stmt) = conn.prepare_cached(
            "SELECT zh_word, jyutping_toned, weight
             FROM cantonese_dict
             WHERE abbr = ?1 AND length(zh_word) = 1
             ORDER BY weight DESC
             LIMIT 8",
        ) else {
            return map;
        };

        for ch in abbr_chars {
            let mut words = Vec::new();
            let mut seen = HashSet::new();

            let mut collect_for = |query_abbr: &str, penalty: i32| {
                if let Ok(rows) = stmt.query_map(params![query_abbr], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i32>(2)?,
                    ))
                }) {
                    for (zh_word, toned_jp, weight) in rows.flatten() {
                        if !zh_word.is_ascii() && seen.insert(zh_word.clone()) {
                            words.push(LatticeWord {
                                word: zh_word,
                                matched_code: toned_jp,
                                db_weight: weight + penalty,
                                user_boost: 0,
                                source: if penalty == 0 {
                                    CandidateSource::Jyutping
                                } else {
                                    CandidateSource::Fuzzy
                                },
                            });
                        }
                    }
                }
            };

            collect_for(ch, 0);
            if ch == "l" {
                collect_for("n", -200);
            } else if ch == "n" {
                collect_for("l", -200);
            }

            words.sort_by(|a, b| b.db_weight.cmp(&a.db_weight));
            if words.len() > TOP_K_PER_SPAN {
                words.truncate(TOP_K_PER_SPAN);
            }
            map.insert(ch.clone(), words);
        }

        map
    }
}

/// `idx_yue_abbr` 撈取的簡拼錨點候選池項目
struct AbbrPoolEntry {
    word: String,
    toned_jp: String,
    syllables: Vec<String>,
    weight: i32,
}
