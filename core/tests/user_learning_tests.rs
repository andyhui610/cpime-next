//! CPIME-Next 個人詞頻記憶 (`user_unigrams`) 與 Bigram 前後字連帶 (`user_bigrams`) 自動化測試

use cpime_core::{CandidateSource, Engine};

fn setup_test_engine() -> Engine {
    let db_path = Engine::default_db_path();
    Engine::open_with_user_dict(db_path, ":memory:").expect("無法初始化測試引擎與記憶體個人詞庫")
}

/// 案例 1（Unigram 選字置頂）：
/// 初次輸入 "san" 第 1 名為「新」，當模擬使用者選擇排在後面的「晨」1 次後，
/// 再次輸入 "san"，「晨」必須躍升為第 1 名。
#[test]
fn test_case_1_unigram_selection_boost_to_top() {
    let mut engine = setup_test_engine();

    let initial = engine.get_candidates("san");
    assert!(!initial.is_empty(), "輸入 'san' 應回傳候選詞");
    assert_eq!(initial[0].word, "新", "初次輸入 'san' 第 1 名應為「新」");

    let san_idx = initial
        .iter()
        .position(|c| c.word == "晨")
        .expect("候選詞中應包含「晨」");
    assert!(san_idx > 0, "初次輸入 'san' 時「晨」應排在「新」後面");

    // 模擬使用者選擇「晨」1 次
    let committed = engine.commit_candidate("san", san_idx);
    assert_eq!(committed.as_deref(), Some("晨"));

    // 清除前後字上下文，純測 Unigram 置頂效果
    engine.clear_context();

    let after = engine.get_candidates("san");
    assert_eq!(
        after[0].word, "晨",
        "選過 1 次「晨」後，再次輸入 'san'，「晨」必須躍升為第 1 名"
    );
}

/// 案例 2（英文反查偏好記憶）：
/// 初次輸入 "sun" 時「日」排在「太陽」前面；
/// 當使用者選過一次「太陽」後，下次輸入 "sun" 時「太陽」必須排在「日」前面。
#[test]
fn test_case_2_english_lookup_preference_memory() {
    let mut engine = setup_test_engine();

    let initial = engine.get_candidates("sun");
    let pos_jat_before = initial
        .iter()
        .position(|c| c.word == "日")
        .expect("初次輸入 'sun' 應包含「日」");
    let pos_sun_before = initial
        .iter()
        .position(|c| c.word == "太陽")
        .expect("初次輸入 'sun' 應包含「太陽」");
    assert!(
        pos_jat_before < pos_sun_before,
        "初次輸入 'sun' 時「日」(index={}) 應排在「太陽」(index={}) 前面",
        pos_jat_before,
        pos_sun_before
    );

    // 模擬使用者選過一次「太陽」
    let committed = engine.commit_candidate("sun", pos_sun_before);
    assert_eq!(committed.as_deref(), Some("太陽"));

    engine.clear_context();

    let after = engine.get_candidates("sun");
    let pos_jat_after = after
        .iter()
        .position(|c| c.word == "日")
        .expect("再次輸入 'sun' 應包含「日」");
    let pos_sun_after = after
        .iter()
        .position(|c| c.word == "太陽")
        .expect("再次輸入 'sun' 應包含「太陽」");
    assert!(
        pos_sun_after < pos_jat_after,
        "選過一次「太陽」後，下次輸入 'sun' 時「太陽」(index={}) 必須排在「日」(index={}) 前面",
        pos_sun_after,
        pos_jat_after
    );
    assert_eq!(
        after[0].word, "太陽",
        "剛選過的「太陽」在下次輸入 'sun' 時亦應直接置頂第 1 名"
    );
}

/// 案例 3（Bigram 前後字語境記憶）：
/// 模擬使用者先選出 "食"，接著輸入 "maan" 並選擇排名較後的 "晚"（代表「食晚」）；
/// 清除狀態後，當使用者再次先輸入 "食"、接著輸入 "maan" 時，"晚" 必須因 Bigram 連帶直接空降第 1 名；
/// 若沒有先打 "食" 而直接單獨打 "maan"，則維持原排序。
#[test]
fn test_case_3_bigram_context_memory_and_fallback_order() {
    let mut engine = setup_test_engine();

    // 記錄學習前單獨打 "maan" 的原始候選詞順序
    let original_maan: Vec<String> = engine
        .get_candidates("maan")
        .into_iter()
        .map(|c| c.word)
        .collect();
    assert!(!original_maan.is_empty());
    let maan_night_idx = original_maan
        .iter()
        .position(|w| w == "晚")
        .expect("輸入 'maan' 候選清單應包含「晚」");
    assert!(
        maan_night_idx > 0,
        "初次單獨輸入 'maan' 時，「晚」應排在較後位置 (index={})",
        maan_night_idx
    );

    // 1. 模擬使用者先輸入 "sik" 選出 "食"
    let sik_candidates = engine.get_candidates("sik");
    let sik_idx = sik_candidates
        .iter()
        .position(|c| c.word == "食")
        .expect("輸入 'sik' 應包含「食」");
    let committed_first = engine.commit_candidate("sik", sik_idx);
    assert_eq!(committed_first.as_deref(), Some("食"));

    // 2. 接著輸入 "maan" 並選擇排名較後的 "晚"（形成「食晚」Bigram 記憶）
    let committed_second = engine.commit_candidate("maan", maan_night_idx);
    assert_eq!(committed_second.as_deref(), Some("晚"));

    // 3. 清除上下文狀態后，先測試「沒有先打『食』而直接單獨打 'maan'」：應維持原排序
    engine.clear_context();
    let solo_maan: Vec<String> = engine
        .get_candidates("maan")
        .into_iter()
        .map(|c| c.word)
        .collect();
    assert_eq!(
        solo_maan, original_maan,
        "若沒有先打「食」而直接單獨打 'maan'，應維持原排序"
    );

    // 4. 當使用者再次先輸入 "sik" 選出 "食"，接著輸入 "maan" 時，「晚」必須因 Bigram 連帶直接空降第 1 名
    let _ = engine.commit_candidate("sik", sik_idx);
    assert_eq!(engine.last_committed_word.as_deref(), Some("食"));

    // 同時驗證無碼聯想 get_predictions 也能撈出「晚」
    let predictions = engine.get_predictions(5);
    assert!(
        predictions.iter().any(|c| c.word == "晚"),
        "選完「食」後的 get_predictions 應包含聯想字「晚」"
    );

    let contextual_maan = engine.get_candidates("maan");
    assert_eq!(
        contextual_maan[0].word, "晚",
        "先打「食」再打 'maan' 時，「晚」必須因 Bigram 連帶直接空降第 1 名"
    );
    assert_eq!(
        contextual_maan[0].source,
        CandidateSource::BigramContext,
        "Bigram 命中的候選詞來源應標記為 BigramContext"
    );
}

/// 案例 4（自造詞記憶）：
/// 呼叫 `record_custom_phrase("hingjyu", "hj", "興如")` 後，
/// 輸入全拼 "hingjyu" 或簡拼 "hj" 皆能在第 1 名看到 "興如"。
#[test]
fn test_case_4_custom_phrase_full_and_abbr_lookup() {
    let engine = setup_test_engine();

    engine
        .user_dict
        .record_custom_phrase("hingjyu", "hj", "興如");

    let full_candidates = engine.get_candidates("hingjyu");
    assert!(
        !full_candidates.is_empty(),
        "輸入自造詞全拼 'hingjyu' 應回傳候選詞"
    );
    assert_eq!(
        full_candidates[0].word, "興如",
        "輸入全拼 'hingjyu' 必須在第 1 名看到自造詞「興如」"
    );

    let abbr_candidates = engine.get_candidates("hj");
    assert!(
        !abbr_candidates.is_empty(),
        "輸入自造詞簡拼 'hj' 應回傳候選詞"
    );
    assert_eq!(
        abbr_candidates[0].word, "興如",
        "輸入簡拼 'hj' 必須在第 1 名看到自造詞「興如」"
    );

    // 額外驗證 remove_user_word 後悔刪除功能
    engine.remove_user_word("hingjyu", "興如");
    let after_remove = engine.get_candidates("hingjyu");
    assert!(
        after_remove.iter().all(|c| c.word != "興如"),
        "呼叫 remove_user_word 刪除後，輸入 'hingjyu' 不應再出現「興如」"
    );
}

/// 輔助函式：在候選清單中找出指定字詞並呼叫 `commit_candidate` 模擬使用者選字上屏
fn select_word(engine: &mut Engine, input: &str, target_word: &str) {
    let candidates = engine.lookup_with_limit(input, 30);
    let idx = candidates
        .iter()
        .position(|c| c.word == target_word)
        .unwrap_or_else(|| {
            panic!(
                "輸入 '{}' 的候選詞中找不到目標字 '{}'，實際候選詞：{:?}",
                input, target_word, candidates
            )
        });
    let committed = engine.commit_candidate(input, idx);
    assert_eq!(committed.as_deref(), Some(target_word));
}

/// 案例 A（單字合成與兩階段轉正）：
/// 模擬使用者依序打單字 "hing" 選 "興"、再打 "jyu" 選 "如"（第 1 次觀察）；
/// 重置上下文後，再次打 "hing" 選 "興"、打 "jyu" 選 "如"（第 2 次正式轉正）。
/// 驗證此時直接輸入全拼 "hingjyu" 或簡拼 "hj"，候選詞第 1 名皆為 "興如"！
#[test]
fn test_case_a_auto_phrase_two_stage_promotion() {
    let mut engine = setup_test_engine();

    // 第 1 次觀察：依序打單字 "hing" -> "興"、"jyu" -> "如"
    select_word(&mut engine, "hing", "興");
    select_word(&mut engine, "jyu", "如");

    // 驗證第 1 次僅處於觀察期 (freq = 1)，尚未正式轉正
    let stage1_full = engine.get_candidates("hingjyu");
    assert!(
        stage1_full.iter().all(|c| c.word != "興如"),
        "第 1 次僅為觀察期 (freq=1)，尚未轉正時查詢 'hingjyu' 不應提前出現「興如」"
    );

    // 重置上下文後，第 2 次打 "hing" -> "興"、"jyu" -> "如"（正式轉正 freq = 2）
    engine.clear_context();
    select_word(&mut engine, "hing", "興");
    select_word(&mut engine, "jyu", "如");
    engine.clear_context();

    // 驗證此時直接輸入全拼 "hingjyu" 或簡拼 "hj"，候選詞第 1 名皆為 "興如"
    let full_candidates = engine.get_candidates("hingjyu");
    assert!(!full_candidates.is_empty(), "查詢全拼 'hingjyu' 應有候選詞");
    assert_eq!(
        full_candidates[0].word, "興如",
        "兩階段轉正後輸入全拼 'hingjyu'，第 1 名必須為「興如」"
    );
    assert_eq!(full_candidates[0].source, CandidateSource::UserCustom);

    let abbr_candidates = engine.get_candidates("hj");
    assert!(!abbr_candidates.is_empty(), "查詢簡拼 'hj' 應有候選詞");
    assert_eq!(
        abbr_candidates[0].word, "興如",
        "兩階段轉正後輸入簡拼 'hj'，第 1 名必須為「興如」"
    );
    assert_eq!(abbr_candidates[0].source, CandidateSource::UserCustom);
}

/// 案例 B（簡拼單字還原為全拼自造詞）：
/// 模擬使用者用簡拼打 "h" 選 "興"、再用簡拼打 "j" 選 "如" 兩次，
/// 驗證引擎能自動反查出完整粵拼，讓使用者之後打全拼 "hingjyu" 也能查到 "興如"。
#[test]
fn test_case_b_abbr_single_chars_restored_to_full_jyutping_phrase() {
    let mut engine = setup_test_engine();

    // 第 1 次用簡拼 "h" 選 "興"、"j" 選 "如"
    select_word(&mut engine, "h", "興");
    select_word(&mut engine, "j", "如");
    engine.clear_context();

    // 第 2 次用簡拼 "h" 選 "興"、"j" 選 "如"
    select_word(&mut engine, "h", "興");
    select_word(&mut engine, "j", "如");
    engine.clear_context();

    // 驗證引擎已自動反查並還原為標準全拼 "hingjyu"，打全拼 "hingjyu" 與簡拼 "hj" 皆能查到 "興如" 排第 1 名
    let full_candidates = engine.get_candidates("hingjyu");
    assert!(
        !full_candidates.is_empty(),
        "用簡拼造詞後輸入全拼 'hingjyu' 應能查到候選詞"
    );
    assert_eq!(
        full_candidates[0].word, "興如",
        "用簡拼 'h' + 'j' 選字兩次後，輸入全拼 'hingjyu' 第 1 名應為「興如」"
    );

    let abbr_candidates = engine.get_candidates("hj");
    assert!(!abbr_candidates.is_empty());
    assert_eq!(abbr_candidates[0].word, "興如");
}

/// 案例 C（廣東話虛詞防火牆防污染）：
/// 模擬使用者連續打單字 "我" -> "喺" -> "興" -> "如" -> "做" -> "嘢" 兩次，
/// 驗證引擎只會將 "興如" 合成為自造詞，而絕對不會合成出 "我喺"、"喺興"、"如做" 或 "做嘢" 等包含虛詞的垃圾詞條。
#[test]
fn test_case_c_cantonese_stopword_firewall_prevents_garbage_phrases() {
    let mut engine = setup_test_engine();

    let sequence = [
        ("ngo", "我"),
        ("hai", "喺"),
        ("hing", "興"),
        ("jyu", "如"),
        ("zou", "做"),
        ("je", "嘢"),
    ];

    // 連續打兩次「我 -> 喺 -> 興 -> 如 -> 做 -> 嘢」
    for _ in 0..2 {
        for &(code, word) in &sequence {
            select_word(&mut engine, code, word);
        }
        engine.clear_context();
    }

    // 1. 驗證「興如」已成功轉正並可透過全拼 "hingjyu" 與簡拼 "hj" 在第 1 名查出
    let hingjyu_cands = engine.get_candidates("hingjyu");
    assert!(!hingjyu_cands.is_empty());
    assert_eq!(hingjyu_cands[0].word, "興如");

    let hj_cands = engine.get_candidates("hj");
    assert!(!hj_cands.is_empty());
    assert_eq!(hj_cands[0].word, "興如");

    // 2. 檢查 user_unigrams 中所有自造詞紀錄 (is_custom = 1)，驗證只會有「興如」，
    //    絕對不會合成出「我喺」、「喺興」、「如做」、「做嘢」或任何包含虛詞的詞條
    let custom_entries = engine.user_dict.query_all_custom_phrases();
    let custom_words: Vec<&str> = custom_entries.iter().map(|e| e.zh_word.as_str()).collect();
    assert_eq!(
        custom_words,
        vec!["興如"],
        "自造詞庫中應只有「興如」，實際紀錄：{:?}",
        custom_entries
    );

    for forbidden in ["我喺", "喺興", "如做", "做嘢", "我喺興", "喺興如", "興如做", "如做嘢"] {
        assert!(
            !custom_words.contains(&forbidden),
            "虛詞防火牆應攔截垃圾自造詞「{}」",
            forbidden
        );
    }
}


