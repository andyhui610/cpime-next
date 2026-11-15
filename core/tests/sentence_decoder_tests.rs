//! CPIME-Next DAG 詞圖 + Viterbi 廣東話長句連打與步進式部分選字 (`Partial Commit`) 測試

use std::time::Instant;

use cpime_core::{
    format_segmented_pinyin, segment_syllables_with_spans, CandidateSource, Engine, OutputMode,
    PrefixSyllable, SyllableKind,
};

fn setup_test_engine() -> Engine {
    let db_path = Engine::default_db_path();
    Engine::open_with_user_dict(db_path, ":memory:").expect("無法初始化測試引擎與記憶體個人詞庫")
}

/// 測試 1：標準長句連打
/// 輸入 `"ngogamjatheoihoenggong"`，驗證：
/// - `format_segmented_pinyin` 回傳 `"ngo'gam'jat'heoi'hoeng'gong"`
/// - 第 1 個候選詞為整句 `"我今日去香港"`，且 `consumed_chars == 22`
/// - 額外驗證尾端未完成聲母 `"gamjath"` 能將 `"h"` 識別為 `PrefixSyllable` 並補齊詞圖
#[test]
fn test_standard_long_sentence_decoding() {
    let engine = setup_test_engine();

    let input = "ngogamjatheoihoenggong";
    assert_eq!(
        format_segmented_pinyin(input),
        "ngo'gam'jat'heoi'hoeng'gong"
    );

    let start = Instant::now();
    let candidates = engine.get_candidates(input);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 10,
        "長句連打單次查詢延遲應低於 10ms，實測：{:?}",
        elapsed
    );
    assert!(!candidates.is_empty(), "長句連打應回傳候選詞");
    assert_eq!(
        candidates[0].word, "我今日去香港",
        "第 1 名應組出整句「我今日去香港」，實際候選：{:?}",
        candidates
    );
    assert_eq!(candidates[0].source, CandidateSource::SentenceDecoded);
    assert_eq!(candidates[0].consumed_chars, 22);

    // 驗證尾端未完成聲母前綴識別與解碼
    let prefix_input = "gamjath";
    let spans = segment_syllables_with_spans(prefix_input);
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].normalized_jyutping, "gam");
    assert_eq!(spans[0].kind, SyllableKind::FullSyllable);
    assert_eq!(spans[1].normalized_jyutping, "jat");
    assert_eq!(spans[1].kind, SyllableKind::FullSyllable);
    assert_eq!(spans[2].normalized_jyutping, "h");
    assert_eq!(spans[2].kind, PrefixSyllable);
    assert!(spans[2].is_prefix);

    let prefix_cands = engine.get_candidates(prefix_input);
    assert!(!prefix_cands.is_empty());
    assert_eq!(prefix_cands[0].source, CandidateSource::SentenceDecoded);
    assert!(
        prefix_cands[0].word.starts_with("今日"),
        "輸入 'gamjath' 應以「今日」開頭並補齊尾端 'h' 單字，實際：{:?}",
        prefix_cands[0]
    );
    assert_eq!(prefix_cands[0].consumed_chars, prefix_input.len());
}

/// 測試 2：口語與書面語長句切換
/// 輸入 `"keoideihaidoukinggai"`：
/// - 在 `OutputMode::Colloquial` 下第 1 名組出 `"佢哋喺度傾偈"`
/// - 切換至 `OutputMode::Written` 後第 1 名自動組出 `"他們在度聊天"`
#[test]
fn test_colloquial_vs_written_long_sentence_switching() {
    let mut engine = setup_test_engine();
    let input = "keoideihaidoukinggai";
    assert_eq!(
        format_segmented_pinyin(input),
        "keoi'dei'hai'dou'king'gai"
    );

    // 1. 預設口語模式
    assert_eq!(engine.get_output_mode(), OutputMode::Colloquial);
    let colloq_cands = engine.get_candidates(input);
    assert!(!colloq_cands.is_empty());
    assert_eq!(
        colloq_cands[0].word, "佢哋喺度傾偈",
        "口語模式下第 1 名應為「佢哋喺度傾偈」，實際：{:?}",
        colloq_cands
    );
    assert_eq!(colloq_cands[0].source, CandidateSource::SentenceDecoded);
    assert_eq!(colloq_cands[0].consumed_chars, input.len());

    // 2. 切換至書面語模式
    engine.set_output_mode(OutputMode::Written);
    let written_cands = engine.get_candidates(input);
    assert!(!written_cands.is_empty());
    assert_eq!(
        written_cands[0].word, "他們在度聊天",
        "書面語模式下第 1 名應為「他們在度聊天」，實際：{:?}",
        written_cands
    );
    assert_eq!(written_cands[0].source, CandidateSource::SentenceDecoded);
    assert_eq!(written_cands[0].consumed_chars, input.len());
}

/// 測試 3：步進式部分選字（Partial Commit）
/// 輸入 `"gamjattingjat"`（今日聽日），驗證：
/// - 第 1 名為整句 `"今日聽日"`（`consumed_chars == 13`）
/// - 第 2 名為開頭前綴詞 `"今日"` 且 `consumed_chars == 6`
/// - 模擬切除開頭 6 個字元後，對剩餘 `"tingjat"` 重新查詢可得 `"聽日"`（`consumed_chars == 7`）
#[test]
fn test_progressive_partial_commit_onion_peeling() {
    let engine = setup_test_engine();
    let input = "gamjattingjat";
    assert_eq!(format_segmented_pinyin(input), "gam'jat'ting'jat");

    let cands = engine.get_candidates(input);
    assert!(
        cands.len() >= 2,
        "輸入 'gamjattingjat' 應至少有 2 個候選詞（整句與開頭前綴詞），實際：{:?}",
        cands
    );
    assert_eq!(cands[0].word, "今日聽日");
    assert_eq!(cands[0].source, CandidateSource::SentenceDecoded);
    assert_eq!(cands[0].consumed_chars, 13);

    assert_eq!(
        cands[1].word, "今日",
        "第 2 名應為開頭前綴詞「今日」，實際候選：{:?}",
        cands
    );
    assert_eq!(cands[1].consumed_chars, 6);

    // 模擬前端依 consumed_chars 切除開頭 6 個字元後重新查詢
    let remaining = &input[cands[1].consumed_chars..];
    assert_eq!(remaining, "tingjat");
    let next_cands = engine.get_candidates(remaining);
    assert!(!next_cands.is_empty());
    assert_eq!(next_cands[0].word, "聽日");
    assert_eq!(next_cands[0].consumed_chars, 7);
}

/// 測試 4：長英文單字防誤拆保護
/// 輸入 `"afternoon"`，驗證第 1~3 名依然是 `"中午"`、`"下午"`、`"晏晝"`，不會被 Viterbi 誤拆。
#[test]
fn test_long_english_word_protection_afternoon() {
    let engine = setup_test_engine();
    let input = "afternoon";

    // 英文單字不應被 format_segmented_pinyin 強行切分
    assert_eq!(format_segmented_pinyin(input), "afternoon");

    let cands = engine.get_candidates(input);
    assert!(cands.len() >= 3);
    assert_eq!(cands[0].word, "中午");
    assert_eq!(cands[0].source, CandidateSource::English);
    assert_eq!(cands[0].consumed_chars, 9);
    assert_eq!(cands[1].word, "下午");
    assert_eq!(cands[1].source, CandidateSource::English);
    assert_eq!(cands[2].word, "晏晝");
    assert_eq!(cands[2].source, CandidateSource::English);
}
