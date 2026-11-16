//! CPIME-Next DAG 詞圖 + Viterbi 廣東話長句連打與步進式部分選字 (`Partial Commit`) 測試

use std::time::Instant;

use cpime_core::{
    format_segmented_pinyin, segment_hybrid_syllables, CandidateSource, Engine, OutputMode,
    SyllableKind,
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

    // 驗證尾端未完成聲母簡拼識別與解碼
    let prefix_input = "gamjath";
    let spans = segment_hybrid_syllables(prefix_input);
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].normalized_jyutping, "gam");
    assert_eq!(spans[0].kind, SyllableKind::Full("gam".to_string()));
    assert_eq!(spans[0].abbr_char, 'g');
    assert_eq!(spans[1].normalized_jyutping, "jat");
    assert_eq!(spans[1].kind, SyllableKind::Full("jat".to_string()));
    assert_eq!(spans[1].abbr_char, 'j');
    assert_eq!(spans[2].normalized_jyutping, "h");
    assert_eq!(spans[2].kind, SyllableKind::Abbr("h".to_string()));
    assert_eq!(spans[2].abbr_char, 'h');
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

/// 測試 5：全拼 + 簡拼混合長句（前全後簡）
/// 輸入 `"gamjathg"`，驗證第 1 順位候選詞為 `"今日香港"`，且 `format_segmented_pinyin("gamjathg")` 輸出 `"gam'jat'h'g"`。
#[test]
fn test_hybrid_full_then_abbr_sentence_gamjathg() {
    let engine = setup_test_engine();
    let input = "gamjathg";

    assert_eq!(format_segmented_pinyin(input), "gam'jat'h'g");
    let spans = segment_hybrid_syllables(input);
    assert_eq!(spans.len(), 4);
    assert_eq!(spans[0].kind, SyllableKind::Full("gam".to_string()));
    assert_eq!(spans[1].kind, SyllableKind::Full("jat".to_string()));
    assert_eq!(spans[2].kind, SyllableKind::Abbr("h".to_string()));
    assert_eq!(spans[3].kind, SyllableKind::Abbr("g".to_string()));

    let start = Instant::now();
    let cands = engine.get_candidates(input);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 10,
        "混合長句連打單次查詢延遲應低於 10ms，實測：{:?}",
        elapsed
    );
    assert!(!cands.is_empty());
    assert_eq!(
        cands[0].word, "今日香港",
        "輸入 'gamjathg' 第 1 順位應為「今日香港」，實際候選：{:?}",
        cands
    );
    assert_eq!(cands[0].source, CandidateSource::SentenceDecoded);
    assert_eq!(cands[0].consumed_chars, input.len());
}

/// 測試 6：全拼 + 簡拼混合長句（穿插簡拼）
/// 輸入 `"ngogjhhg"`（我 + 今日`gj` + 去`h` + 香港`hg`），驗證第 1 順位正確解碼為 `"我今日去香港"`。
#[test]
fn test_hybrid_interleaved_abbr_sentence_ngogjhhg() {
    let engine = setup_test_engine();
    let input = "ngogjhhg";

    assert_eq!(format_segmented_pinyin(input), "ngo'g'j'h'h'g");
    let spans = segment_hybrid_syllables(input);
    assert_eq!(spans.len(), 6);
    assert_eq!(spans[0].kind, SyllableKind::Full("ngo".to_string()));
    assert_eq!(spans[1].kind, SyllableKind::Abbr("g".to_string()));
    assert_eq!(spans[2].kind, SyllableKind::Abbr("j".to_string()));
    assert_eq!(spans[3].kind, SyllableKind::Abbr("h".to_string()));
    assert_eq!(spans[4].kind, SyllableKind::Abbr("h".to_string()));
    assert_eq!(spans[5].kind, SyllableKind::Abbr("g".to_string()));

    let start = Instant::now();
    let cands = engine.get_candidates(input);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 10,
        "穿插簡拼長句查詢延遲應低於 10ms，實測：{:?}",
        elapsed
    );
    assert!(!cands.is_empty());
    assert_eq!(
        cands[0].word, "我今日去香港",
        "輸入 'ngogjhhg' 第 1 順位應為「我今日去香港」，實際候選：{:?}",
        cands
    );
    assert_eq!(cands[0].source, CandidateSource::SentenceDecoded);
    assert_eq!(cands[0].consumed_chars, input.len());
}

/// 測試 7：純簡拼整句連打與延遲驗證
/// 輸入 `"gjhhg"`（今日去香港）與 `"nhhg"`（你好香港），驗證第 1 順位組出對應合理粵語句子，
/// 並驗證 `"nhma"` 切分及混合簡拼 `"gamj"` / `"gjjat"`，單次查詢延遲低於 10ms。
#[test]
fn test_pure_abbr_sentence_decoding_and_latency() {
    let engine = setup_test_engine();

    // 1. 純簡拼 "gjhhg" -> "今日去香港"
    let start_gjhhg = Instant::now();
    let cands_gjhhg = engine.get_candidates("gjhhg");
    let elapsed_gjhhg = start_gjhhg.elapsed();
    assert!(
        elapsed_gjhhg.as_millis() < 10,
        "'gjhhg' 查詢延遲應低於 10ms，實測：{:?}",
        elapsed_gjhhg
    );
    assert!(!cands_gjhhg.is_empty());
    assert_eq!(
        cands_gjhhg[0].word, "今日去香港",
        "輸入 'gjhhg' 第 1 順位應為「今日去香港」，實際候選：{:?}",
        cands_gjhhg
    );

    // 2. 純簡拼 "nhhg" -> "你好香港"
    let start_nhhg = Instant::now();
    let cands_nhhg = engine.get_candidates("nhhg");
    let elapsed_nhhg = start_nhhg.elapsed();
    assert!(
        elapsed_nhhg.as_millis() < 10,
        "'nhhg' 查詢延遲應低於 10ms，實測：{:?}",
        elapsed_nhhg
    );
    assert!(!cands_nhhg.is_empty());
    assert_eq!(
        cands_nhhg[0].word, "你好香港",
        "輸入 'nhhg' 第 1 順位應為「你好香港」，實際候選：{:?}",
        cands_nhhg
    );

    // 3. 混合後置全拼 "nhma" -> [Abbr("n"), Abbr("h"), Full("ma")] -> "你好嗎"
    let nhma_spans = segment_hybrid_syllables("nhma");
    assert_eq!(nhma_spans.len(), 3);
    assert_eq!(nhma_spans[0].kind, SyllableKind::Abbr("n".to_string()));
    assert_eq!(nhma_spans[1].kind, SyllableKind::Abbr("h".to_string()));
    assert_eq!(nhma_spans[2].kind, SyllableKind::Full("ma".to_string()));
    let cands_nhma = engine.get_candidates("nhma");
    assert!(!cands_nhma.is_empty());
    assert_eq!(
        cands_nhma[0].word, "你好嗎",
        "輸入 'nhma' 第 1 順位應為「你好嗎」，實際候選：{:?}",
        cands_nhma
    );

    // 4. 純簡拼 "ngjhhg"（n=我 + gj=今日 + h=去 + hg=香港）、"nggjhhg"（ng 歧義展開=我 + gj=今日 + h=去 + hg=香港）與 "nghhg"（ng=我 + h=去 + hg=香港）
    let start_ng = Instant::now();
    let cands_ng = engine.get_candidates("ngjhhg");
    let elapsed_ng = start_ng.elapsed();
    assert!(elapsed_ng.as_millis() < 10);
    assert!(!cands_ng.is_empty());
    assert_eq!(
        cands_ng[0].word, "我今日去香港",
        "輸入 'ngjhhg' 第 1 順位應解碼為「我今日去香港」，實際候選：{:?}",
        cands_ng
    );

    let cands_ngg = engine.get_candidates("nggjhhg");
    assert!(!cands_ngg.is_empty());
    assert_eq!(
        cands_ngg[0].word, "我今日去香港",
        "輸入 'nggjhhg' 第 1 順位應展開 'ng' 歧義為「我今日去香港」，實際候選：{:?}",
        cands_ngg
    );

    let cands_ng_ngo = engine.get_candidates("nghhg");
    assert!(!cands_ng_ngo.is_empty());
    assert_eq!(
        cands_ng_ngo[0].word, "我去香港",
        "輸入 'nghhg' 應將開頭 'ng' 展開為單一音節「我」並組出「我去香港」，實際候選：{:?}",
        cands_ng_ngo
    );
}

/// 測試 8：混合連打下的步進選字（Partial Commit）
/// 輸入 `"gamjathg"` 時，驗證排在第 2 順位的開頭詞 `"今日"` 其 `consumed_chars == 6`（僅消耗 `"gamjat"`，留下 `"hg"` 顯示 `"香港"`）。
#[test]
fn test_hybrid_partial_commit_gamjathg() {
    let engine = setup_test_engine();
    let input = "gamjathg";

    let cands = engine.get_candidates(input);
    assert!(
        cands.len() >= 2,
        "輸入 'gamjathg' 應至少回傳 2 個候選詞，實際：{:?}",
        cands
    );
    assert_eq!(cands[0].word, "今日香港");
    assert_eq!(cands[0].consumed_chars, 8);

    assert_eq!(
        cands[1].word, "今日",
        "第 2 順位開頭詞應為「今日」，實際候選：{:?}",
        cands
    );
    assert_eq!(
        cands[1].consumed_chars, 6,
        "選中「今日」應僅消耗 'gamjat'（6 bytes），實際 consumed_chars={}",
        cands[1].consumed_chars
    );

    let remaining = &input[cands[1].consumed_chars..];
    assert_eq!(remaining, "hg");
    let next_cands = engine.get_candidates(remaining);
    assert!(!next_cands.is_empty());
    assert_eq!(next_cands[0].word, "香港");
    assert_eq!(next_cands[0].consumed_chars, 2);
}
