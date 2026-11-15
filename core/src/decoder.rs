//! CPIME-Next DAG 詞圖與 Viterbi 整句連打解碼器 (`decoder.rs`)
//!
//! 實作 `GEMINI.md` 第 8 節規範之「兩層式 DAG 詞圖建構 + Viterbi 動態規劃 + 步進式部分選字前綴提取」：
//! 1. 收集音節跨度 $1 \le len \le \min(6, N)$ 的子音節組合，使用單次批次 SQL (`WHERE toneless IN (...)`)
//!    一次撈出 `cantonese_dict`、`user_unigrams` 與書面語映射表候選詞條。
//! 2. 支援尾端未完成聲母 (`PrefixSyllable`) 以 `abbr` / 前綴查詢補齊最後一個音節區間。
//! 3. 套用邊權重公式：
//!    `edge_score = word_weight + user_boost + ((j - i)^2 * 3500) - 2000`
//!    並在命中 `user_bigrams` 時額外加上 `+6000` 轉移加分。
//! 4. 同步產出從第 0 個音節開始的開頭前綴候選詞（含 `consumed_chars`），供前端「剝洋蔥式」部分選字上屏。

use std::collections::{HashMap, HashSet};

use rusqlite::{params, params_from_iter, Connection};

use crate::engine::{
    next_prefix_bound, written_cantonese_mapping, Candidate, CandidateSource, OutputMode,
};
use crate::phonetics::{SyllableKind, SyllableSpan};
use crate::user_dict::UserDict;

/// 單一詞圖邊的最大音節跨度
const MAX_LATTICE_SPAN: usize = 6;

/// 每個音節跨度區間最多保留的系統詞條數量
const TOP_K_PER_SPAN: usize = 5;

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

        // 1. 收集所有合法跨度 [i..j] (1 <= j - i <= min(6, N)) 的無聲調拼音字串
        let mut span_codes: Vec<Vec<Option<String>>> = vec![vec![None; n + 1]; n];
        let mut unique_full_codes: Vec<String> = Vec::new();
        let mut seen_codes: HashSet<String> = HashSet::new();

        for i in 0..n {
            let max_j = (i + MAX_LATTICE_SPAN).min(n);
            for j in (i + 1)..=max_j {
                let slice = &syllables[i..j];
                if slice.iter().all(|s| s.kind == SyllableKind::FullSyllable) {
                    let mut code = String::new();
                    for s in slice {
                        code.push_str(&s.normalized_jyutping);
                    }
                    if seen_codes.insert(code.clone()) {
                        unique_full_codes.push(code.clone());
                    }
                    span_codes[i][j] = Some(code);
                }
            }
        }

        // 若尾端音節為未完成的聲母前綴 (PrefixSyllable)，單獨記錄該尾端區間 [n-1..n] 的前綴碼
        let tail_prefix_code: Option<String> =
            if syllables[n - 1].kind == SyllableKind::PrefixSyllable {
                let p = syllables[n - 1].normalized_jyutping.clone();
                if seen_codes.insert(p.clone()) {
                    unique_full_codes.push(p.clone());
                }
                span_codes[n - 1][n] = Some(p.clone());
                Some(p)
            } else {
                None
            };

        // 2. 單次批次 SQL 從 cantonese_dict 撈取所有全拼區間的候選詞條（每碼保留前 5 高分詞條）
        let sys_map = Self::batch_query_cantonese_dict(conn, &unique_full_codes);

        // 3. 若有尾端未完成聲母前綴，使用 abbr / 前綴查詢補齊最後一個單音節區間
        let tail_prefix_words = if let Some(ref p_code) = tail_prefix_code {
            Self::query_prefix_syllable_words(conn, p_code)
        } else {
            Vec::new()
        };

        // 4. 批次從 user_dict 撈取相關的個人詞頻 (user_unigrams) 與上下文連帶 (user_bigrams)
        let uni_map = user_dict.query_unigrams_batch(&unique_full_codes);
        let bi_map = user_dict.query_bigrams_batch(&unique_full_codes);

        // 5. 組裝每個區間 [i..j] 的詞圖候選清單
        let mut lattice: Vec<Vec<Vec<LatticeWord>>> = vec![vec![Vec::new(); n + 1]; n];
        for i in 0..n {
            let max_j = (i + MAX_LATTICE_SPAN).min(n);
            for j in (i + 1)..=max_j {
                let Some(ref code) = span_codes[i][j] else {
                    continue;
                };
                let is_tail_prefix = i == n - 1 && j == n && tail_prefix_code.is_some();

                let mut words: Vec<LatticeWord> = Vec::with_capacity(TOP_K_PER_SPAN + 2);
                let mut seen_words: HashSet<String> = HashSet::new();

                // 5a. 若處於書面語模式，優先注入書面語映射詞條
                if !is_tail_prefix && output_mode == OutputMode::Written {
                    if let Some(written_words) = written_cantonese_mapping(code) {
                        for (idx, &w) in written_words.iter().enumerate() {
                            if seen_words.insert(w.to_string()) {
                                words.push(LatticeWord {
                                    word: w.to_string(),
                                    matched_code: code.clone(),
                                    db_weight: 12_000 - (idx as i32 * 100),
                                    user_boost: 0,
                                    source: CandidateSource::Jyutping,
                                });
                            }
                        }
                    }
                }

                // 5b. 填入系統詞庫詞條（每區間最多取 TOP_K_PER_SPAN 條）
                let sys_entries: &[LatticeWord] = if is_tail_prefix {
                    &tail_prefix_words
                } else {
                    sys_map.get(code).map(|v| v.as_slice()).unwrap_or(&[])
                };
                for lw in sys_entries {
                    if words.len() >= TOP_K_PER_SPAN + 2 {
                        break;
                    }
                    if seen_words.insert(lw.word.clone()) {
                        words.push(lw.clone());
                    }
                }

                // 5c. 整合個人詞頻 (user_unigrams) 與已轉正自造詞
                if let Some(uni_entries) = uni_map.get(code) {
                    let max_last_used = uni_entries.iter().map(|e| e.last_used).max();
                    let mut boost_by_word: HashMap<&str, (i32, bool)> = HashMap::new();
                    for e in uni_entries {
                        let freq_bonus = (e.freq * 400).min(5_000);
                        let latest_bonus =
                            if Some(e.last_used) == max_last_used && e.last_used > 0 {
                                3_500
                            } else {
                                0
                            };
                        let custom_bonus = if e.is_custom && e.freq >= 2 {
                            5_500
                        } else {
                            0
                        };
                        boost_by_word.insert(
                            e.zh_word.as_str(),
                            (freq_bonus + latest_bonus + custom_bonus, e.is_custom),
                        );
                    }

                    for lw in &mut words {
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
                                matched_code: code.clone(),
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

                // 5d. 若單音節區間仍完全無候選詞，提供原始拼音保底邊確保圖連通
                if words.is_empty() && j == i + 1 {
                    let fallback = raw_input
                        .get(syllables[i].start_byte..syllables[i].end_byte)
                        .unwrap_or(code.as_str())
                        .to_string();
                    words.push(LatticeWord {
                        word: fallback,
                        matched_code: code.clone(),
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

        // 6. 執行 Viterbi 動態規劃
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
                let (prev_score, prev_word) = match dp[i].as_ref() {
                    Some(prev_state) => (prev_state.score, prev_state.chosen_word.clone()),
                    None => continue,
                };
                let Some(ref code) = span_codes[i][j] else {
                    continue;
                };
                let candidates = &lattice[i][j];
                if candidates.is_empty() {
                    continue;
                }

                let span = (j - i) as i64;
                let span_bonus = (span * span * 3_500) - 2_000;
                let bigram_hits = if !prev_word.is_empty() {
                    bi_map.get(&(prev_word, code.clone()))
                } else {
                    None
                };

                for lw in candidates {
                    // 將詞頻按音節跨度縮放，使多字詞與同長度的多個高頻單字在公平基礎上比較
                    let word_weight = (lw.db_weight as i64) * span;
                    let user_boost = lw.user_boost as i64;
                    let bigram_bonus = if bigram_hits.is_some_and(|set| set.contains(&lw.word)) {
                        6_000
                    } else {
                        0
                    };
                    let edge_score = word_weight + user_boost + span_bonus + bigram_bonus;
                    let total = prev_score + edge_score;

                    let should_update = match &dp[j] {
                        None => true,
                        Some(existing) => total > existing.score,
                    };
                    if should_update {
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

        let final_state = dp[n].as_ref()?;
        let total_score = final_state.score.clamp(i32::MIN as i64, i32::MAX as i64) as i32;

        // 7. 從 dp[N] 反向回溯組出整句漢字
        let mut word_segments: Vec<String> = Vec::new();
        let mut code_segments: Vec<String> = Vec::new();
        let mut segment_count = 0usize;
        let mut cur = n;
        while cur > 0 {
            let state = dp[cur].as_ref()?;
            word_segments.push(state.chosen_word.clone());
            code_segments.push(state.chosen_code.clone());
            segment_count += 1;
            cur = state.prev_idx;
        }
        word_segments.reverse();
        code_segments.reverse();

        let full_sentence: String = word_segments.concat();
        let matched_code: String = code_segments.join(" ");
        let is_single_whole_word = segment_count == 1;

        // 8. 生成從第 0 個音節開始的前綴候選詞（供步進式部分選字：先長詞後單字）
        let mut prefix_candidates: Vec<Candidate> = Vec::new();
        let mut seen_prefix_words: HashSet<String> = HashSet::new();
        seen_prefix_words.insert(full_sentence.clone());

        let max_prefix_span = (n - 1).min(MAX_LATTICE_SPAN);
        for k in (1..=max_prefix_span).rev() {
            let consumed_chars = syllables[k - 1].end_byte;
            let span_bonus = ((k as i32) * (k as i32) * 3_500) - 2_000;
            for lw in &lattice[0][k] {
                if seen_prefix_words.insert(lw.word.clone()) {
                    let score = (lw.db_weight * k as i32) + lw.user_boost + span_bonus;
                    prefix_candidates.push(Candidate {
                        word: lw.word.clone(),
                        matched_code: lw.matched_code.clone(),
                        source: lw.source,
                        score,
                        consumed_chars,
                    });
                }
            }
        }

        Some(DecodedSentence {
            full_sentence,
            matched_code,
            total_score,
            is_single_whole_word,
            prefix_candidates,
        })
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

        // 每個音節碼暫存去重後的非純 ASCII 詞條，最多保留 TOP_K_PER_SPAN 條
        let mut seen_per_code: HashMap<String, HashSet<String>> = HashMap::new();
        for (toneless, zh_word, toned_jp, weight) in rows.flatten() {
            // 跳過純英文 ASCII 詞條（如 Rime 詞庫中混入的 "king"），避免干擾中文長句拼字
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

    /// 針對尾端未完成的聲母或前綴音節 (`PrefixSyllable`) 查詢最高頻單字補齊詞圖
    fn query_prefix_syllable_words(conn: &Connection, prefix: &str) -> Vec<LatticeWord> {
        let mut words = Vec::new();
        let mut seen = HashSet::new();

        if prefix.len() == 1 {
            // 單聲母前綴（如 "h"）：優先查單字高頻詞（如「喺」「係」「好」）
            if let Ok(mut stmt) = conn.prepare_cached(
                "SELECT zh_word, jyutping_toned, weight
                 FROM cantonese_dict
                 WHERE abbr = ?1 AND length(zh_word) = 1
                 ORDER BY weight DESC LIMIT 8",
            ) {
                if let Ok(rows) = stmt.query_map(params![prefix], |row| {
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
                                db_weight: weight,
                                user_boost: 0,
                                source: CandidateSource::Jyutping,
                            });
                            if words.len() >= TOP_K_PER_SPAN {
                                break;
                            }
                        }
                    }
                }
            }
        } else if let Some(upper) = next_prefix_bound(prefix) {
            // 多字元未完成音節前綴（如 "heo" -> "heoi"）：利用 B-Tree 範圍查詢單字
            if let Ok(mut stmt) = conn.prepare_cached(
                "SELECT zh_word, jyutping_toned, weight
                 FROM (
                     SELECT zh_word, jyutping_toned, weight
                     FROM cantonese_dict
                     WHERE toneless > ?1 AND toneless < ?2
                     LIMIT 200
                 )
                 WHERE length(zh_word) = 1
                 ORDER BY weight DESC LIMIT 8",
            ) {
                if let Ok(rows) = stmt.query_map(params![prefix, &upper], |row| {
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
                                db_weight: weight,
                                user_boost: 0,
                                source: CandidateSource::Jyutping,
                            });
                            if words.len() >= TOP_K_PER_SPAN {
                                break;
                            }
                        }
                    }
                }
            }
        }

        words
    }
}
