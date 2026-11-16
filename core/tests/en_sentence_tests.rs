//! 英文整句直出廣東話模式整合單元測試 (`en_sentence_tests.rs`)
//!
//! 驗證：
//! 1. 輸入 `"I go to school by bus"` 或 `";i go to school by bus"`：第 1、2 名候選詞包含 `"我搭巴士返學校"` 與 `"我搭巴士返學"`。
//! 2. 輸入 `"we go to work by mtr today"`：自動將時間與交通工具前置，組出 `"我哋今日搭地鐵返工"`。
//! 3. 輸入 `"have lunch with my friend"`：正確重組為 `"同我朋友食晏"`（或 `"同朋友食晏"`）。
//! 4. 切換至 `OutputMode::Written` 輸入 `"I go to school by bus"`：第 1 名輸出書面語 `"我乘巴士去學校"`（或 `"我坐巴士上學"`）。
//! 5. 進階驗證：獨立開關 `EnSentenceMode`、進行式、過去式、寒暄疑問句與 `english_dict` 動賓查詢。

use cpime_core::{translate_en_sentence, CpimeEngine, InputMode, OutputMode};

fn setup_engine() -> CpimeEngine {
    CpimeEngine::open_default().expect("無法開啟預設種子資料庫 /core/assets/cpime_dict.db")
}

#[test]
fn test_en_sentence_i_go_to_school_by_bus_and_semicolon_prefix() {
    let mut engine = setup_engine();

    // 1. 透過獨立模式輸入 "I go to school by bus"
    engine.set_en_sentence_mode(true);
    let cands = engine.lookup("I go to school by bus");
    assert!(
        cands.len() >= 2,
        "應回傳至少 2 個整句候選詞，實得：{:?}",
        cands
    );
    assert_eq!(cands[0].word, "我搭巴士返學校");
    assert_eq!(cands[1].word, "我搭巴士返學");
    assert_eq!(cands[0].consumed_chars, "I go to school by bus".len());
    assert!(
        cands.iter().any(|c| c.word == "I go to school by bus"),
        "候選清單應保留原始英文句：{:?}",
        cands
    );

    // 2. 切回一般粵拼模式，改用 `;` 免切換前綴輸入 ";i go to school by bus"
    engine.set_en_sentence_mode(false);
    assert_eq!(engine.get_input_mode(), InputMode::Cantonese);
    let prefix_cands = engine.lookup(";i go to school by bus");
    assert!(prefix_cands.len() >= 2);
    assert_eq!(prefix_cands[0].word, "我搭巴士返學校");
    assert_eq!(prefix_cands[1].word, "我搭巴士返學");
    assert_eq!(
        prefix_cands[0].consumed_chars,
        ";i go to school by bus".len()
    );
}

#[test]
fn test_en_sentence_we_go_to_work_by_mtr_today() {
    let engine = setup_engine();
    let cands = engine.lookup(";we go to work by mtr today");
    assert!(!cands.is_empty(), "應能翻譯時間與交通工具前置句");
    assert_eq!(cands[0].word, "我哋今日搭地鐵返工");

    // 亦測試直接透過獨立翻譯函式不帶前綴查詢
    let direct_cands = translate_en_sentence("we go to work by mtr today", OutputMode::Colloquial);
    assert_eq!(direct_cands[0].word, "我哋今日搭地鐵返工");
}

#[test]
fn test_en_sentence_have_lunch_with_my_friend() {
    let engine = setup_engine();
    let cands = engine.lookup("have lunch with my friend");
    assert!(!cands.is_empty());
    assert_eq!(cands[0].word, "同我朋友食晏");
    assert!(
        cands
            .iter()
            .any(|c| c.word == "同朋友食晏" || c.word == "同我朋友食晏"),
        "候選詞應包含「同我朋友食晏」或「同朋友食晏」：{:?}",
        cands
    );
}

#[test]
fn test_en_sentence_written_mode_i_go_to_school_by_bus() {
    let mut engine = setup_engine();
    engine.set_output_mode(OutputMode::Written);
    engine.set_en_sentence_mode(true);

    let cands = engine.lookup("I go to school by bus");
    assert!(!cands.is_empty());
    assert!(
        cands[0].word == "我乘巴士去學校" || cands[0].word == "我坐巴士上學",
        "書面語模式第 1 名應為「我乘巴士去學校」或「我坐巴士上學」，實得：{:?}",
        cands
    );
    assert_eq!(cands[0].word, "我乘巴士去學校");
}

#[test]
fn test_en_sentence_tenses_greetings_and_english_dict_fallback() {
    let mut engine = setup_engine();
    engine.set_en_sentence_mode(true);

    // 1. 進行式：I am eating -> 我食緊嘢 / 我喺度食緊嘢
    let prog = engine.lookup("I am eating");
    assert!(!prog.is_empty());
    assert_eq!(prog[0].word, "我食緊嘢");

    // 2. 過去式 / yesterday 補「咗」
    let past = engine.lookup("I went to school by bus yesterday");
    assert!(!past.is_empty());
    assert_eq!(past[0].word, "我尋日搭巴士返咗學校");

    // 3. 常見寒暄與疑問句
    assert_eq!(engine.lookup("how much is it")[0].word, "幾多錢呀");
    assert_eq!(engine.lookup("where are you")[0].word, "你喺邊度");
    assert_eq!(engine.lookup("what time is it")[0].word, "而家幾點");
    assert_eq!(engine.lookup("thank you very much")[0].word, "唔該晒");
    assert_eq!(engine.lookup("wait a minute")[0].word, "等陣先");

    // 4. 退回查詢動賓詞彙（eat apple / drink coffee）
    assert_eq!(engine.lookup("eat apple")[0].word, "食蘋果");
    assert_eq!(engine.lookup("drink coffee")[0].word, "飲咖啡");

    // 5. '/' 前綴亦可免切換觸發
    engine.set_en_sentence_mode(false);
    assert_eq!(engine.lookup("/go home on foot")[0].word, "行路返屋企");
}
