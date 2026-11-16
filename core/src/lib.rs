//! CPIME-Next (`cpime-core`) 跨平台粵語與英漢反查雙軌輸入法引擎核心庫

pub mod decoder;
pub mod en_sentence;
pub mod engine;
pub mod jni_bridge;
pub mod phonetics;
pub mod phrase_composer;
pub mod user_dict;

pub use decoder::{DecodedSentence, LatticeWord, SentenceDecoder};
pub use en_sentence::{is_en_sentence_trigger, translate_en_sentence, EnSentenceTranslator};
pub use engine::{Candidate, CandidateSource, CpimeEngine, Engine, InputMode, OutputMode};
pub use phonetics::{
    expand_phonetic_queries, format_segmented_pinyin, normalize_to_jyutping, sanitize_input,
    segment_hybrid_syllables, segment_syllables_with_spans, PhoneticQuery, SyllableKind,
    SyllableSpan,
};
pub use phrase_composer::{
    is_cantonese_stopword, CharToken, PhraseComposer, PhraseCompositionEvent, CANTONESE_STOPWORDS,
};
pub use user_dict::{UserBigramEntry, UserDict, UserUnigramEntry};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn setup_engine() -> CpimeEngine {
        CpimeEngine::open_default().expect("無法開啟預設種子資料庫 /core/assets/cpime_dict.db")
    }

    #[test]
    fn test_short_code_collision_sun() {
        let engine = setup_engine();
        let start = Instant::now();
        let candidates = engine.lookup("sun");
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 10,
            "單次查詢延遲應低於 10ms，實測：{:?}",
            elapsed
        );
        assert!(
            candidates.len() >= 4,
            "輸入 'sun' 應至少回傳 4 個候選詞，實得：{:?}",
            candidates
        );

        // 前 2 位放最高頻粵拼字（新、身），第 3 至 4 位強制穿插英文直譯結果（日、太陽）
        assert_eq!(candidates[0].word, "新");
        assert_eq!(candidates[0].source, CandidateSource::Jyutping);
        assert_eq!(candidates[1].word, "身");
        assert_eq!(candidates[1].source, CandidateSource::Jyutping);
        assert_eq!(candidates[2].word, "日");
        assert_eq!(candidates[2].source, CandidateSource::English);
        assert_eq!(candidates[3].word, "太陽");
        assert_eq!(candidates[3].source, CandidateSource::English);
    }

    #[test]
    fn test_english_exact_afternoon() {
        let engine = setup_engine();
        let start = Instant::now();
        let candidates = engine.lookup("afternoon");
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 10);
        assert!(candidates.len() >= 3);
        assert_eq!(candidates[0].word, "中午");
        assert_eq!(candidates[0].source, CandidateSource::English);
        assert_eq!(candidates[1].word, "下午");
        assert_eq!(candidates[2].word, "晏晝");
    }

    #[test]
    fn test_english_prefix_aftern() {
        let engine = setup_engine();
        let start = Instant::now();
        let candidates = engine.lookup("aftern");
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 10);
        assert!(
            !candidates.is_empty(),
            "輸入前綴 'aftern' 應能匹配到 afternoon 的中文候選詞"
        );
        assert_eq!(candidates[0].word, "中午");
        assert_eq!(candidates[0].source, CandidateSource::English);
        assert_eq!(candidates[1].word, "下午");
        assert_eq!(candidates[2].word, "晏晝");
    }

    #[test]
    fn test_fuzzy_lazy_pronunciation_lei_to_nei() {
        let engine = setup_engine();
        let start = Instant::now();
        let candidates = engine.lookup("lei");
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 10);
        assert!(!candidates.is_empty());
        // 輸入 "lei" 透過 l/n 懶音容錯應出「你」且標記為懶音
        assert_eq!(candidates[0].word, "你");
        assert_eq!(candidates[0].source, CandidateSource::Fuzzy);
    }

    #[test]
    fn test_initials_abbreviation_nh_to_neihou() {
        let engine = setup_engine();
        let start = Instant::now();
        let candidates = engine.lookup("nh");
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 10);
        assert!(!candidates.is_empty());
        // 輸入 "nh" 簡拼應出「你好」
        assert_eq!(candidates[0].word, "你好");
        assert_eq!(candidates[0].source, CandidateSource::Jyutping);
    }

    #[test]
    fn test_english_to_hk_four_byte_char_lift() {
        let engine = setup_engine();
        let start = Instant::now();
        let candidates = engine.lookup("lift");
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 10);
        assert!(candidates.len() >= 2);
        // 輸入 "lift" 應直接第 1 順位出四位元組香港擴展漢字「𨋢」
        assert_eq!(candidates[0].word, "𨋢");
        assert_eq!(candidates[0].word.len(), 4, "「𨋢」應為 UTF-8 四位元組漢字");
        assert_eq!(candidates[0].source, CandidateSource::English);
        assert_eq!(candidates[1].word, "升降機");
    }

    #[test]
    fn test_multi_scheme_normalization_and_fuzzy_features() {
        let engine = setup_engine();

        // 1. 耶魯/港式：heung -> hoeng (前綴匹配出「香港」)
        let c_heung = engine.lookup("heunggong");
        assert!(!c_heung.is_empty());
        assert_eq!(c_heung[0].word, "香港");

        // 2. 劉錫祥/港式：ho -> hou (出「好」)
        let c_ho = engine.lookup("ho");
        assert!(c_ho.iter().any(|c| c.word == "好"));

        // 3. ng/零聲母懶音互換：輸入 "sikaan" 出「食晏」(sikngaan)
        let c_sikaan = engine.lookup("sikaan");
        assert!(c_sikaan
            .iter()
            .any(|c| c.word == "食晏" && c.source == CandidateSource::Fuzzy));

        // 4. 短碼撞碼 "man"：1.文 2.問 3.人 4.男人
        let c_man = engine.lookup("man");
        assert!(c_man.len() >= 4);
        assert_eq!(c_man[0].word, "文");
        assert_eq!(c_man[1].word, "問");
        assert_eq!(c_man[2].word, "人");
        assert_eq!(c_man[3].word, "男人");
    }

    #[test]
    fn test_full_lexicon_queries_and_sub_5ms_latency() {
        let engine = setup_engine();

        for (query, expected_first) in [
            ("gamjat", "今日"),
            ("hoenggong", "香港"),
            ("kinggai", "傾偈"),
            ("sun", "新"),
            ("afternoon", "中午"),
            ("lift", "𨋢"),
        ] {
            let start = Instant::now();
            let candidates = engine.lookup(query);
            let elapsed = start.elapsed();

            assert!(
                elapsed.as_micros() < 5_000,
                "查詢 '{}' 耗時應低於 5ms，實測：{:?}",
                query,
                elapsed
            );
            assert!(
                !candidates.is_empty(),
                "查詢 '{}' 應回傳候選詞",
                query
            );
            assert_eq!(candidates[0].word, expected_first);
        }
    }

    #[test]
    fn test_output_mode_colloquial_vs_written_switching() {
        let mut engine = setup_engine();
        assert_eq!(engine.get_output_mode(), OutputMode::Colloquial);

        // 1. 口語模式：維持道地廣東話口語優先
        assert_eq!(engine.lookup("keoidei")[0].word, "佢哋");
        assert_eq!(engine.lookup("dimgaai")[0].word, "點解");
        assert_eq!(engine.lookup("tomorrow")[0].word, "聽日");
        assert_eq!(engine.lookup("lift")[0].word, "𨋢");

        // 2. 切換為書面語模式 (OutputMode::Written)
        engine.set_output_mode(OutputMode::Written);
        assert_eq!(engine.get_output_mode(), OutputMode::Written);

        assert_eq!(engine.lookup("keoidei")[0].word, "他們");
        assert_eq!(engine.lookup("dimgaai")[0].word, "為什麼");
        assert_eq!(engine.lookup("tomorrow")[0].word, "明天");
        assert_eq!(engine.lookup("lift")[0].word, "升降機");
        assert_eq!(engine.lookup("ge")[0].word, "的");

        let hai_cands = engine.lookup("hai");
        assert_eq!(hai_cands[0].word, "是");
        assert_eq!(hai_cands[1].word, "在");

        let m_cands = engine.lookup("m");
        assert_eq!(m_cands[0].word, "不");
        assert_eq!(m_cands[1].word, "沒");

        // 驗證其他常用映射與切回口語模式後立即恢復
        assert_eq!(engine.lookup("camjat")[0].word, "昨天");
        assert_eq!(engine.lookup("kamjat")[0].word, "昨天");
        assert_eq!(engine.lookup("lip")[0].word, "升降機");

        engine.set_output_mode(OutputMode::Colloquial);
        assert_eq!(engine.lookup("keoidei")[0].word, "佢哋");
        assert_eq!(engine.lookup("dimgaai")[0].word, "點解");
        assert_eq!(engine.lookup("tomorrow")[0].word, "聽日");
    }
}

