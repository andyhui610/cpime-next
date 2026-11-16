//! 英文整句直出廣東話模式引擎 (`en_sentence.rs`)
//!
//! 實作結構化「港式英文片語與語法槽位字典（Slot Grammar Lexicon）」及廣東話語序重組演算法：
//! 1. 將輸入英文句子正規化（支援 `;` 或 `/` 前綴觸發、縮寫展開與小寫 Token 化）。
//! 2. 透過最長片語優先匹配（Longest Phrase Matching）將片段歸類為：
//!    `[Subject]`、`[Time]`、`[Negation/Aux]`、`[With_Modifier]`、`[Transport_Modifier]`、
//!    `[Place_Modifier]`、`[Action_Verb_Object]`、`[Particle]`。
//! 3. 自動將伴隨 (`with ...`)、交通工具 (`by ...`/`on foot`) 與地點/時間狀語搬移至核心動詞前，
//!    按道地廣東話語序 `[Subject] + [Time] + [Negation/Aux] + [With] + [Transport] + [Place] + [Action] + [Particle]` 組裝。
//! 4. 支援口語 (`OutputMode::Colloquial`) 與書面語 (`OutputMode::Written`) 雙語體、進行式與過去式，
//!    並對未收錄的一般動詞與受詞退回查詢 `english_dict` 進行動賓合成。

use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::engine::{Candidate, CandidateSource, OutputMode};

/// 英文整句直出廣東話翻譯引擎
pub struct EnSentenceTranslator;

/// 語法槽位分類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Subject,
    Time,
    NegAux,
    WithModifier,
    TransportModifier,
    PlaceModifier,
    Action,
    Particle,
    StopFiller,
}

/// 單一詞條的多語體對照定義
#[derive(Debug, Clone, Copy)]
struct LexiconEntry {
    phrase: &'static [&'static str],
    slot: SlotKind,
    colloquial: &'static [&'static str],
    written: &'static [&'static str],
    is_past: bool,
}

/// 常見英文寒暄與完整疑問句直出字典
#[derive(Debug, Clone, Copy)]
struct WholeSentenceEntry {
    phrase: &'static [&'static str],
    colloquial: &'static [&'static str],
    written: &'static [&'static str],
}

/// 寒暄與高頻疑問整句表
const WHOLE_SENTENCES: &[WholeSentenceEntry] = &[
    WholeSentenceEntry {
        phrase: &["how", "much", "is", "it"],
        colloquial: &["幾多錢呀", "幾多錢"],
        written: &["多少錢", "幾多錢呀"],
    },
    WholeSentenceEntry {
        phrase: &["how", "much"],
        colloquial: &["幾多錢呀", "幾多錢"],
        written: &["多少錢", "幾多錢呀"],
    },
    WholeSentenceEntry {
        phrase: &["where", "are", "you"],
        colloquial: &["你喺邊度", "你喺邊呀"],
        written: &["你在哪裡", "你喺邊度"],
    },
    WholeSentenceEntry {
        phrase: &["what", "time", "is", "it"],
        colloquial: &["而家幾點", "而家幾點呀"],
        written: &["現在幾點", "而家幾點"],
    },
    WholeSentenceEntry {
        phrase: &["thank", "you", "very", "much"],
        colloquial: &["唔該晒", "多謝晒"],
        written: &["非常感謝", "多謝晒", "唔該晒"],
    },
    WholeSentenceEntry {
        phrase: &["thanks", "a", "lot"],
        colloquial: &["唔該晒", "多謝晒"],
        written: &["非常感謝", "多謝晒"],
    },
    WholeSentenceEntry {
        phrase: &["thank", "you"],
        colloquial: &["唔該", "多謝"],
        written: &["謝謝", "多謝"],
    },
    WholeSentenceEntry {
        phrase: &["wait", "a", "minute"],
        colloquial: &["等陣先", "等一陣"],
        written: &["等一下", "稍等一下"],
    },
    WholeSentenceEntry {
        phrase: &["wait", "a", "moment"],
        colloquial: &["等陣先", "等一陣"],
        written: &["稍等一下", "等一下"],
    },
    WholeSentenceEntry {
        phrase: &["wait", "a", "second"],
        colloquial: &["等陣先", "等一秒"],
        written: &["等一下", "稍等"],
    },
    WholeSentenceEntry {
        phrase: &["see", "you", "tomorrow"],
        colloquial: &["聽日見"],
        written: &["明天見"],
    },
    WholeSentenceEntry {
        phrase: &["see", "you", "later"],
        colloquial: &["遲啲見", "陣間見"],
        written: &["待會見", "回頭見"],
    },
    WholeSentenceEntry {
        phrase: &["good", "morning"],
        colloquial: &["早晨"],
        written: &["早安", "早晨"],
    },
    WholeSentenceEntry {
        phrase: &["good", "night"],
        colloquial: &["早唞"],
        written: &["晚安"],
    },
];

/// 結構化槽位語法字典（按最長片語優先匹配）
const SLOT_LEXICON: &[LexiconEntry] = &[
    // ── 時間狀語 [Time] ─────────────────────────────────────────────────────
    LexiconEntry {
        phrase: &["in", "the", "morning"],
        slot: SlotKind::Time,
        colloquial: &["朝早"],
        written: &["早上"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["in", "the", "afternoon"],
        slot: SlotKind::Time,
        colloquial: &["晏晝"],
        written: &["下午"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["in", "the", "evening"],
        slot: SlotKind::Time,
        colloquial: &["夜晚"],
        written: &["晚上"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["at", "night"],
        slot: SlotKind::Time,
        colloquial: &["夜晚"],
        written: &["晚上"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["right", "now"],
        slot: SlotKind::Time,
        colloquial: &["而家"],
        written: &["現在"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["every", "day"],
        slot: SlotKind::Time,
        colloquial: &["日日"],
        written: &["每天"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["today"],
        slot: SlotKind::Time,
        colloquial: &["今日"],
        written: &["今天"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["tomorrow"],
        slot: SlotKind::Time,
        colloquial: &["聽日"],
        written: &["明天"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["yesterday"],
        slot: SlotKind::Time,
        colloquial: &["尋日"],
        written: &["昨天"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["now"],
        slot: SlotKind::Time,
        colloquial: &["而家"],
        written: &["現在"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["tonight"],
        slot: SlotKind::Time,
        colloquial: &["今晚"],
        written: &["今晚"],
        is_past: false,
    },
    // ── 主詞 [Subject] ──────────────────────────────────────────────────────
    LexiconEntry {
        phrase: &["my", "friend"],
        slot: SlotKind::Subject,
        colloquial: &["我朋友"],
        written: &["我的朋友"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["my", "friends"],
        slot: SlotKind::Subject,
        colloquial: &["我班朋友", "我朋友"],
        written: &["我的朋友們"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["i"],
        slot: SlotKind::Subject,
        colloquial: &["我"],
        written: &["我"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["you"],
        slot: SlotKind::Subject,
        colloquial: &["你"],
        written: &["你"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["he"],
        slot: SlotKind::Subject,
        colloquial: &["佢"],
        written: &["他"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["she"],
        slot: SlotKind::Subject,
        colloquial: &["佢"],
        written: &["她"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["we"],
        slot: SlotKind::Subject,
        colloquial: &["我哋"],
        written: &["我們"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["they"],
        slot: SlotKind::Subject,
        colloquial: &["佢哋"],
        written: &["他們"],
        is_past: false,
    },
    // ── 否定與助動詞 [NegAux] ───────────────────────────────────────────────
    LexiconEntry {
        phrase: &["do", "not"],
        slot: SlotKind::NegAux,
        colloquial: &["唔"],
        written: &["不"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["does", "not"],
        slot: SlotKind::NegAux,
        colloquial: &["唔"],
        written: &["不"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["did", "not"],
        slot: SlotKind::NegAux,
        colloquial: &["冇"],
        written: &["沒有"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["can", "not"],
        slot: SlotKind::NegAux,
        colloquial: &["唔可以"],
        written: &["不能"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["cannot"],
        slot: SlotKind::NegAux,
        colloquial: &["唔可以"],
        written: &["不能"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["want", "to"],
        slot: SlotKind::NegAux,
        colloquial: &["想"],
        written: &["想"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["need", "to"],
        slot: SlotKind::NegAux,
        colloquial: &["要"],
        written: &["需要"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["have", "to"],
        slot: SlotKind::NegAux,
        colloquial: &["要"],
        written: &["必須"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["going", "to"],
        slot: SlotKind::NegAux,
        colloquial: &["將會", "打算"],
        written: &["將會", "打算"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["not"],
        slot: SlotKind::NegAux,
        colloquial: &["唔"],
        written: &["不"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["can"],
        slot: SlotKind::NegAux,
        colloquial: &["可以"],
        written: &["可以"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["will"],
        slot: SlotKind::NegAux,
        colloquial: &["會"],
        written: &["會"],
        is_past: false,
    },
    // ── 伴隨修飾語 [WithModifier]（搬移至動詞前）────────────────────────────
    LexiconEntry {
        phrase: &["with", "my", "friend"],
        slot: SlotKind::WithModifier,
        colloquial: &["同我朋友", "同朋友"],
        written: &["和我的朋友", "跟朋友"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "my", "friends"],
        slot: SlotKind::WithModifier,
        colloquial: &["同我班朋友", "同朋友"],
        written: &["和我的朋友們", "和朋友"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "my", "family"],
        slot: SlotKind::WithModifier,
        colloquial: &["同屋企人"],
        written: &["和家人"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "friends"],
        slot: SlotKind::WithModifier,
        colloquial: &["同朋友"],
        written: &["和朋友"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "family"],
        slot: SlotKind::WithModifier,
        colloquial: &["同屋企人"],
        written: &["和家人"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "you"],
        slot: SlotKind::WithModifier,
        colloquial: &["同你"],
        written: &["和你"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "him"],
        slot: SlotKind::WithModifier,
        colloquial: &["同佢"],
        written: &["和他"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "her"],
        slot: SlotKind::WithModifier,
        colloquial: &["同佢"],
        written: &["和她"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "them"],
        slot: SlotKind::WithModifier,
        colloquial: &["同佢哋"],
        written: &["和他們"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["with", "me"],
        slot: SlotKind::WithModifier,
        colloquial: &["同我"],
        written: &["和我"],
        is_past: false,
    },
    // ── 交通工具修飾語 [TransportModifier]（搬移至動詞前）───────────────────
    LexiconEntry {
        phrase: &["by", "bus"],
        slot: SlotKind::TransportModifier,
        colloquial: &["搭巴士"],
        written: &["乘巴士", "坐巴士"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["by", "mtr"],
        slot: SlotKind::TransportModifier,
        colloquial: &["搭地鐵"],
        written: &["乘地鐵", "坐地鐵"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["by", "subway"],
        slot: SlotKind::TransportModifier,
        colloquial: &["搭地鐵"],
        written: &["乘地鐵", "坐地鐵"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["by", "taxi"],
        slot: SlotKind::TransportModifier,
        colloquial: &["搭的士"],
        written: &["乘計程車", "坐的士"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["by", "train"],
        slot: SlotKind::TransportModifier,
        colloquial: &["搭火車"],
        written: &["乘火車", "坐火車"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["by", "plane"],
        slot: SlotKind::TransportModifier,
        colloquial: &["搭飛機"],
        written: &["乘飛機", "坐飛機"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["by", "car"],
        slot: SlotKind::TransportModifier,
        colloquial: &["坐車", "揸車"],
        written: &["乘車", "開車"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["on", "foot"],
        slot: SlotKind::TransportModifier,
        colloquial: &["行路"],
        written: &["走路", "步行"],
        is_past: false,
    },
    // ── 地點修飾語 [PlaceModifier]（搬移至動詞前）───────────────────────────
    LexiconEntry {
        phrase: &["in", "the", "office"],
        slot: SlotKind::PlaceModifier,
        colloquial: &["喺寫字樓"],
        written: &["在辦公室"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["at", "the", "office"],
        slot: SlotKind::PlaceModifier,
        colloquial: &["喺寫字樓"],
        written: &["在辦公室"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["in", "hong", "kong"],
        slot: SlotKind::PlaceModifier,
        colloquial: &["喺香港"],
        written: &["在香港"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["at", "home"],
        slot: SlotKind::PlaceModifier,
        colloquial: &["喺屋企"],
        written: &["在家裡", "在家"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["at", "school"],
        slot: SlotKind::PlaceModifier,
        colloquial: &["喺學校"],
        written: &["在學校"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["at", "work"],
        slot: SlotKind::PlaceModifier,
        colloquial: &["喺公司"],
        written: &["在公司"],
        is_past: false,
    },
    // ── 核心動詞與香港慣用片語 [Action] ─────────────────────────────────────
    LexiconEntry {
        phrase: &["go", "to", "school"],
        slot: SlotKind::Action,
        colloquial: &["返學校", "返學"],
        written: &["去學校", "上學"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["went", "to", "school"],
        slot: SlotKind::Action,
        colloquial: &["返咗學校", "返咗學"],
        written: &["去了學校", "上了學"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["go", "to", "work"],
        slot: SlotKind::Action,
        colloquial: &["返工"],
        written: &["去上班", "上班"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["went", "to", "work"],
        slot: SlotKind::Action,
        colloquial: &["返咗工"],
        written: &["去上班了", "上班了"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["get", "off", "work"],
        slot: SlotKind::Action,
        colloquial: &["收工", "放工"],
        written: &["下班"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["got", "off", "work"],
        slot: SlotKind::Action,
        colloquial: &["收咗工", "放咗工"],
        written: &["下班了"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["go", "to", "sleep"],
        slot: SlotKind::Action,
        colloquial: &["瞓覺"],
        written: &["睡覺"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["went", "to", "sleep"],
        slot: SlotKind::Action,
        colloquial: &["瞓咗覺"],
        written: &["睡覺了"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["watch", "a", "movie"],
        slot: SlotKind::Action,
        colloquial: &["睇戲"],
        written: &["看電影"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["watched", "a", "movie"],
        slot: SlotKind::Action,
        colloquial: &["睇咗戲"],
        written: &["看了電影"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["watch", "movie"],
        slot: SlotKind::Action,
        colloquial: &["睇戲"],
        written: &["看電影"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["watched", "movie"],
        slot: SlotKind::Action,
        colloquial: &["睇咗戲"],
        written: &["看了電影"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["go", "home"],
        slot: SlotKind::Action,
        colloquial: &["返屋企"],
        written: &["回家"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["went", "home"],
        slot: SlotKind::Action,
        colloquial: &["返咗屋企"],
        written: &["回家了", "回了家"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["have", "breakfast"],
        slot: SlotKind::Action,
        colloquial: &["食早餐"],
        written: &["吃早餐"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["had", "breakfast"],
        slot: SlotKind::Action,
        colloquial: &["食咗早餐"],
        written: &["吃了早餐"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["have", "lunch"],
        slot: SlotKind::Action,
        colloquial: &["食晏", "食午餐"],
        written: &["吃午餐", "吃午飯"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["had", "lunch"],
        slot: SlotKind::Action,
        colloquial: &["食咗晏", "食咗午餐"],
        written: &["吃了午餐", "吃了午飯"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["have", "dinner"],
        slot: SlotKind::Action,
        colloquial: &["食晚飯"],
        written: &["吃晚餐", "吃晚飯"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["had", "dinner"],
        slot: SlotKind::Action,
        colloquial: &["食咗晚飯"],
        written: &["吃了晚餐", "吃了晚飯"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["go", "shopping"],
        slot: SlotKind::Action,
        colloquial: &["行街", "買嘢"],
        written: &["逛街", "購物"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["went", "shopping"],
        slot: SlotKind::Action,
        colloquial: &["行咗街", "買咗嘢"],
        written: &["逛街了", "購物了"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["sleep"],
        slot: SlotKind::Action,
        colloquial: &["瞓覺"],
        written: &["睡覺"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["slept"],
        slot: SlotKind::Action,
        colloquial: &["瞓咗覺"],
        written: &["睡了覺"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["chat"],
        slot: SlotKind::Action,
        colloquial: &["傾偈"],
        written: &["聊天"],
        is_past: false,
    },
    LexiconEntry {
        phrase: &["chatted"],
        slot: SlotKind::Action,
        colloquial: &["傾咗偈"],
        written: &["聊了天"],
        is_past: true,
    },
    // ── 時態助詞 / 填充詞 ───────────────────────────────────────────────────
    LexiconEntry {
        phrase: &["already"],
        slot: SlotKind::Particle,
        colloquial: &["已經"],
        written: &["已經"],
        is_past: true,
    },
    LexiconEntry {
        phrase: &["please"],
        slot: SlotKind::StopFiller,
        colloquial: &[],
        written: &[],
        is_past: false,
    },
];

/// 一般單字動詞表（用於動賓動態組合，例如 `eat apple` -> `食蘋果`）
struct VerbSpec {
    colloquial: &'static str,
    written: &'static str,
    is_past: bool,
    is_prog: bool,
    default_obj_colloquial: Option<&'static str>,
    default_obj_written: Option<&'static str>,
}

fn lookup_verb(token: &str) -> Option<VerbSpec> {
    match token {
        "eat" | "eats" => Some(VerbSpec {
            colloquial: "食",
            written: "吃",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "ate" => Some(VerbSpec {
            colloquial: "食",
            written: "吃",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "eating" => Some(VerbSpec {
            colloquial: "食",
            written: "吃",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "drink" | "drinks" => Some(VerbSpec {
            colloquial: "飲",
            written: "喝",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "drank" => Some(VerbSpec {
            colloquial: "飲",
            written: "喝",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "drinking" => Some(VerbSpec {
            colloquial: "飲",
            written: "喝",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "buy" | "buys" => Some(VerbSpec {
            colloquial: "買",
            written: "買",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "bought" => Some(VerbSpec {
            colloquial: "買",
            written: "買",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "buying" => Some(VerbSpec {
            colloquial: "買",
            written: "買",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: Some("嘢"),
            default_obj_written: Some("東西"),
        }),
        "watch" | "watches" | "see" | "sees" => Some(VerbSpec {
            colloquial: "睇",
            written: "看",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "watched" | "saw" => Some(VerbSpec {
            colloquial: "睇",
            written: "看",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "watching" => Some(VerbSpec {
            colloquial: "睇",
            written: "看",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "read" | "reads" => Some(VerbSpec {
            colloquial: "睇",
            written: "讀",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: Some("書"),
            default_obj_written: Some("書"),
        }),
        "reading" => Some(VerbSpec {
            colloquial: "睇",
            written: "讀",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: Some("書"),
            default_obj_written: Some("書"),
        }),
        "write" | "writes" => Some(VerbSpec {
            colloquial: "寫",
            written: "寫",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "wrote" => Some(VerbSpec {
            colloquial: "寫",
            written: "寫",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "writing" => Some(VerbSpec {
            colloquial: "寫",
            written: "寫",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "sleeping" => Some(VerbSpec {
            colloquial: "瞓",
            written: "睡",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: Some("覺"),
            default_obj_written: Some("覺"),
        }),
        "working" => Some(VerbSpec {
            colloquial: "返",
            written: "上",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: Some("工"),
            default_obj_written: Some("班"),
        }),
        "work" | "works" => Some(VerbSpec {
            colloquial: "返",
            written: "工作",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: Some("工"),
            default_obj_written: None,
        }),
        "worked" => Some(VerbSpec {
            colloquial: "返",
            written: "工作",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: Some("工"),
            default_obj_written: None,
        }),
        "play" | "plays" => Some(VerbSpec {
            colloquial: "玩",
            written: "玩",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "played" => Some(VerbSpec {
            colloquial: "玩",
            written: "玩",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "playing" => Some(VerbSpec {
            colloquial: "玩",
            written: "玩",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "like" | "likes" => Some(VerbSpec {
            colloquial: "鍾意",
            written: "喜歡",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "love" | "loves" => Some(VerbSpec {
            colloquial: "愛",
            written: "愛",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "go" | "goes" => Some(VerbSpec {
            colloquial: "去",
            written: "去",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "went" => Some(VerbSpec {
            colloquial: "去",
            written: "去",
            is_past: true,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "going" => Some(VerbSpec {
            colloquial: "去",
            written: "去",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "wait" | "waits" => Some(VerbSpec {
            colloquial: "等",
            written: "等",
            is_past: false,
            is_prog: false,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        "waiting" => Some(VerbSpec {
            colloquial: "等",
            written: "等",
            is_past: false,
            is_prog: true,
            default_obj_colloquial: None,
            default_obj_written: None,
        }),
        _ => None,
    }
}

/// 內建高頻受詞名詞（在無 DB 連線時亦可立即運作，並優先於模糊詞義）
fn lookup_builtin_noun(token: &str) -> Option<(&'static str, &'static str)> {
    match token {
        "apple" | "apples" => Some(("蘋果", "蘋果")),
        "coffee" => Some(("咖啡", "咖啡")),
        "tea" => Some(("茶", "茶")),
        "water" => Some(("水", "水")),
        "orange" | "oranges" => Some(("橙", "橙")),
        "strawberry" | "strawberries" => Some(("士多啤梨", "草莓")),
        "chocolate" => Some(("朱古力", "巧克力")),
        "cheese" => Some(("芝士", "起司")),
        "breakfast" => Some(("早餐", "早餐")),
        "lunch" => Some(("晏", "午餐")),
        "dinner" => Some(("晚飯", "晚餐")),
        "bus" => Some(("巴士", "巴士")),
        "mtr" | "subway" => Some(("地鐵", "地鐵")),
        "taxi" => Some(("的士", "計程車")),
        "school" => Some(("學校", "學校")),
        "work" => Some(("工", "上班")),
        "home" => Some(("屋企", "家")),
        "movie" | "movies" => Some(("戲", "電影")),
        "phone" => Some(("電話", "電話")),
        "computer" => Some(("電腦", "電腦")),
        "book" | "books" => Some(("書", "書")),
        "friend" | "friends" => Some(("朋友", "朋友")),
        "food" => Some(("嘢食", "食物")),
        _ => None,
    }
}

/// 透過 SQLite `english_dict` 查詢未知英文單字的中文字詞（區分口語與書面語排序）
fn query_english_dict_word(
    conn: Option<&Connection>,
    token: &str,
    prefer_written: bool,
) -> Option<String> {
    let conn = conn?;
    let order_clause = if prefer_written {
        "ORDER BY is_colloquial ASC, weight DESC LIMIT 1"
    } else {
        "ORDER BY is_colloquial DESC, weight DESC LIMIT 1"
    };
    let sql = format!(
        "SELECT zh_word FROM english_dict WHERE en_word = ?1 {}",
        order_clause
    );
    if let Ok(mut stmt) = conn.prepare_cached(&sql) {
        if let Ok(word) = stmt.query_row(params![token], |row| row.get::<_, String>(0)) {
            return Some(word);
        }
    }
    None
}

/// 判斷是否應該啟動「英文整句直出廣東話模式」
/// - `en_sentence_mode` 為 `true` 時一律觸發
/// - 輸入以 `;` 或 `/` 開頭（免切換前綴）
/// - 或去除前後空白後仍包含空白字元 `' '`（代表多單字英文句子）
pub fn is_en_sentence_trigger(raw_input: &str, en_sentence_mode: bool) -> bool {
    let trimmed = raw_input.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    en_sentence_mode
        || trimmed.starts_with(';')
        || trimmed.starts_with('/')
        || raw_input.trim().contains(' ')
}

/// 剝除觸發前綴（`;` 或 `/`）並回傳乾淨的原始英文展示文字與小寫 Token 清單
fn preprocess_input(raw_input: &str) -> (String, Vec<String>) {
    let trimmed = raw_input.trim();
    let without_prefix = trimmed
        .strip_prefix(';')
        .or_else(|| trimmed.strip_prefix('/'))
        .unwrap_or(trimmed)
        .trim();

    let display_original = without_prefix.to_string();
    let mut tokens = Vec::new();

    for raw_tok in without_prefix.split_whitespace() {
        // 移除前後常見標點，保留內部縮寫撇號 (')
        let cleaned = raw_tok
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
            .to_ascii_lowercase();
        if cleaned.is_empty() {
            continue;
        }
        match cleaned.as_str() {
            "i'm" => {
                tokens.push("i".to_string());
                tokens.push("am".to_string());
            }
            "you're" => {
                tokens.push("you".to_string());
                tokens.push("are".to_string());
            }
            "he's" => {
                tokens.push("he".to_string());
                tokens.push("is".to_string());
            }
            "she's" => {
                tokens.push("she".to_string());
                tokens.push("is".to_string());
            }
            "we're" => {
                tokens.push("we".to_string());
                tokens.push("are".to_string());
            }
            "they're" => {
                tokens.push("they".to_string());
                tokens.push("are".to_string());
            }
            "don't" => {
                tokens.push("do".to_string());
                tokens.push("not".to_string());
            }
            "doesn't" => {
                tokens.push("does".to_string());
                tokens.push("not".to_string());
            }
            "didn't" => {
                tokens.push("did".to_string());
                tokens.push("not".to_string());
            }
            "can't" => {
                tokens.push("can".to_string());
                tokens.push("not".to_string());
            }
            "won't" => {
                tokens.push("will".to_string());
                tokens.push("not".to_string());
            }
            "wanna" => {
                tokens.push("want".to_string());
                tokens.push("to".to_string());
            }
            "gonna" => {
                tokens.push("going".to_string());
                tokens.push("to".to_string());
            }
            _ => {
                let alpha_only: String = cleaned
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect();
                if !alpha_only.is_empty() {
                    tokens.push(alpha_only);
                }
            }
        }
    }

    (display_original, tokens)
}

/// 針對片語清單執行笛卡兒組合（限制最大產出數以確保 < 1ms 延遲）
fn combine_slots(parts: &[Vec<String>], max_out: usize) -> Vec<String> {
    let mut results = vec![String::new()];
    for options in parts {
        if options.is_empty() {
            continue;
        }
        let mut next = Vec::new();
        for prefix in &results {
            for opt in options {
                let mut s = prefix.clone();
                s.push_str(opt);
                next.push(s);
                if next.len() >= max_out {
                    return next;
                }
            }
        }
        if !next.is_empty() {
            results = next;
        }
    }
    results.into_iter().filter(|s| !s.is_empty()).collect()
}

/// 將過去式助詞 `咗` / `了` 補入動詞片段
fn inject_past_marker(action: &str, is_written: bool) -> String {
    let marker = if is_written { '了' } else { '咗' };
    if action.contains('咗') || action.contains('了') || action.contains('緊') {
        return action.to_string();
    }
    let mut chars = action.chars();
    let Some(first_ch) = chars.next() else {
        return action.to_string();
    };
    let rest: String = chars.collect();
    if is_written && rest.is_empty() {
        format!("{first_ch}了")
    } else {
        format!("{first_ch}{marker}{rest}")
    }
}

/// 將進行式標記 `緊` / `喺度...緊`（書面語 `正在...` / `在...`）套用至動作片段
fn format_progressive_action(verb: &str, obj: Option<&str>, is_written: bool) -> Vec<String> {
    if is_written {
        match obj {
            Some(o) => vec![format!("在{verb}{o}"), format!("正在{verb}{o}")],
            None => vec![format!("正在{verb}"), format!("在{verb}")],
        }
    } else {
        match obj {
            Some(o) => vec![format!("{verb}緊{o}"), format!("喺度{verb}緊{o}")],
            None => vec![format!("喺度{verb}緊"), format!("{verb}緊")],
        }
    }
}

/// 依語法槽位建構特定語體（口語或書面語）的所有可能句子
fn build_style_sentences(
    tokens: &[String],
    conn: Option<&Connection>,
    is_written: bool,
) -> Vec<String> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut subjects: Vec<Vec<String>> = Vec::new();
    let mut times: Vec<Vec<String>> = Vec::new();
    let mut neg_auxs: Vec<Vec<String>> = Vec::new();
    let mut with_mods: Vec<Vec<String>> = Vec::new();
    let mut transport_mods: Vec<Vec<String>> = Vec::new();
    let mut place_mods: Vec<Vec<String>> = Vec::new();
    let mut actions: Vec<Vec<String>> = Vec::new();
    let mut has_past = false;
    let mut has_progressive_be = false;

    let mut idx = 0;
    while idx < tokens.len() {
        // 1. 最長片語優先匹配 SLOT_LEXICON
        let mut best_entry: Option<&LexiconEntry> = None;
        let mut best_len = 0;
        for entry in SLOT_LEXICON {
            let len = entry.phrase.len();
            if len > best_len
                && idx + len <= tokens.len()
                && tokens[idx..idx + len]
                    .iter()
                    .zip(entry.phrase.iter())
                    .all(|(a, b)| a == b)
            {
                best_entry = Some(entry);
                best_len = len;
            }
        }

        if let Some(entry) = best_entry {
            if entry.is_past {
                has_past = true;
            }
            let choices: Vec<String> = if is_written {
                entry.written.iter().map(|s| (*s).to_string()).collect()
            } else {
                entry.colloquial.iter().map(|s| (*s).to_string()).collect()
            };

            if !choices.is_empty() {
                match entry.slot {
                    SlotKind::Subject => subjects.push(choices),
                    SlotKind::Time => times.push(choices),
                    SlotKind::NegAux => neg_auxs.push(choices),
                    SlotKind::WithModifier => with_mods.push(choices),
                    SlotKind::TransportModifier => transport_mods.push(choices),
                    SlotKind::PlaceModifier => place_mods.push(choices),
                    SlotKind::Action => actions.push(choices),
                    SlotKind::Particle => {
                        // `already` 不一定固定置尾；透過 `has_past` 補 `咗` 並可視需要省略重複
                        if entry.phrase != ["already"] {
                            actions.push(choices);
                        }
                    }
                    SlotKind::StopFiller => {}
                }
            }
            idx += best_len;
            continue;
        }

        // 2. 處理 be 動詞（進行式偵測）與冠詞 / 填充字
        let tok = tokens[idx].as_str();
        if matches!(tok, "am" | "is" | "are" | "was" | "were") {
            if matches!(tok, "was" | "were") {
                has_past = true;
            }
            has_progressive_be = true;
            idx += 1;
            continue;
        }
        if matches!(tok, "a" | "an" | "the" | "to") {
            idx += 1;
            continue;
        }

        // 3. 一般單字動詞 + 可選受詞（如 eat apple -> 食蘋果, I am eating -> 我食緊嘢）
        if let Some(vspec) = lookup_verb(tok) {
            if vspec.is_past {
                has_past = true;
            }
            let verb_str = if is_written {
                vspec.written
            } else {
                vspec.colloquial
            };

            // 檢查下一個 Token 是否為受詞（跳過冠詞 a/an/the，且不吞掉介詞片語開頭）
            let mut next_idx = idx + 1;
            while next_idx < tokens.len()
                && matches!(tokens[next_idx].as_str(), "a" | "an" | "the")
            {
                next_idx += 1;
            }

            let mut obj_word: Option<String> = None;
            if next_idx < tokens.len() {
                let next_tok = tokens[next_idx].as_str();
                let is_preposition_or_time = matches!(
                    next_tok,
                    "by" | "on"
                        | "with"
                        | "at"
                        | "in"
                        | "today"
                        | "tomorrow"
                        | "yesterday"
                        | "now"
                        | "already"
                        | "every"
                );
                if !is_preposition_or_time {
                    if let Some((c_noun, w_noun)) = lookup_builtin_noun(next_tok) {
                        obj_word = Some(if is_written { w_noun } else { c_noun }.to_string());
                        idx = next_idx + 1;
                    } else if let Some(db_word) =
                        query_english_dict_word(conn, next_tok, is_written)
                    {
                        obj_word = Some(db_word);
                        idx = next_idx + 1;
                    } else {
                        idx += 1;
                    }
                } else {
                    idx += 1;
                }
            } else {
                idx += 1;
            }

            let default_obj = if is_written {
                vspec.default_obj_written
            } else {
                vspec.default_obj_colloquial
            };
            let effective_obj = obj_word.as_deref().or(default_obj);

            if vspec.is_prog || has_progressive_be {
                actions.push(format_progressive_action(
                    verb_str,
                    effective_obj,
                    is_written,
                ));
            } else {
                let combined = match effective_obj {
                    Some(o) => format!("{verb_str}{o}"),
                    None => verb_str.to_string(),
                };
                actions.push(vec![combined]);
            }
            continue;
        }

        // 4. 退回查詢內建名詞或 SQLite `english_dict` 單字
        if let Some((c_noun, w_noun)) = lookup_builtin_noun(tok) {
            actions.push(vec![if is_written { w_noun } else { c_noun }.to_string()]);
        } else if let Some(db_word) = query_english_dict_word(conn, tok, is_written) {
            actions.push(vec![db_word]);
        }
        idx += 1;
    }

    // 若整句帶有過去式標記（如 yesterday / already / went / ate），為第一個動詞片語補上「咗」/「了」
    if has_past {
        if let Some(first_action) = actions.first_mut() {
            for act in first_action.iter_mut() {
                *act = inject_past_marker(act, is_written);
            }
        }
    }

    // 依道地廣東話語序組裝：
    // [Subject] + [Time] + [Negation/Aux] + [With] + [Transport] + [Place] + [Action]
    let mut ordered_slots: Vec<Vec<String>> = Vec::new();
    ordered_slots.extend(subjects);
    ordered_slots.extend(times);
    ordered_slots.extend(neg_auxs);
    ordered_slots.extend(with_mods);
    ordered_slots.extend(transport_mods);
    ordered_slots.extend(place_mods);
    ordered_slots.extend(actions);

    if ordered_slots.is_empty() {
        return Vec::new();
    }

    combine_slots(&ordered_slots, 6)
}

impl EnSentenceTranslator {
    /// 翻譯英文整句為廣東話候選詞清單（不依賴外部 SQLite 連線的獨立 API）
    pub fn translate_en_sentence(input: &str, mode: OutputMode) -> Vec<Candidate> {
        Self::translate_en_sentence_with_conn(input, mode, None)
    }

    /// 翻譯英文整句為廣東話候選詞清單（可選擇性提供 `cpime_dict.db` 連線供一般動詞/受詞查詢 `english_dict`）
    pub fn translate_en_sentence_with_conn(
        input: &str,
        mode: OutputMode,
        conn: Option<&Connection>,
    ) -> Vec<Candidate> {
        let (display_original, tokens) = preprocess_input(input);
        if tokens.is_empty() {
            return Vec::new();
        }

        let consumed_chars = input.len();
        let mut colloquial_list = Vec::new();
        let mut written_list = Vec::new();

        // 1. 優先檢查常用整句寒暄與疑問句
        for ws in WHOLE_SENTENCES {
            if tokens.len() == ws.phrase.len()
                && tokens
                    .iter()
                    .zip(ws.phrase.iter())
                    .all(|(a, b)| a == b)
            {
                colloquial_list.extend(ws.colloquial.iter().map(|s| (*s).to_string()));
                written_list.extend(ws.written.iter().map(|s| (*s).to_string()));
                break;
            }
        }

        // 2. 若未命中整句寒暄，執行槽位解析與廣東話語序重組
        if colloquial_list.is_empty() && written_list.is_empty() {
            colloquial_list = build_style_sentences(&tokens, conn, false);
            written_list = build_style_sentences(&tokens, conn, true);
        }

        let (primary_list, secondary_list) = match mode {
            OutputMode::Colloquial => (colloquial_list, written_list),
            OutputMode::Written => (written_list, colloquial_list),
        };

        let mut results = Vec::new();
        let mut seen = HashSet::new();
        let mut rank_score = 30_000;

        for sentence in primary_list {
            if seen.insert(sentence.clone()) {
                results.push(Candidate {
                    word: sentence,
                    matched_code: display_original.clone(),
                    source: CandidateSource::English,
                    score: rank_score,
                    consumed_chars,
                });
                rank_score -= 500;
            }
        }

        for sentence in secondary_list {
            if seen.insert(sentence.clone()) {
                results.push(Candidate {
                    word: sentence,
                    matched_code: display_original.clone(),
                    source: CandidateSource::English,
                    score: rank_score - 3_000,
                    consumed_chars,
                });
                rank_score -= 500;
            }
        }

        // 將原始英文保留在候選清單後方（方便使用者隨時保留英文原句）
        if !display_original.is_empty() && seen.insert(display_original.clone()) {
            results.push(Candidate {
                word: display_original.clone(),
                matched_code: display_original,
                source: CandidateSource::English,
                score: 1_000,
                consumed_chars,
            });
        }

        results
    }
}

/// 便捷函式：轉發至 `EnSentenceTranslator::translate_en_sentence`
pub fn translate_en_sentence(input: &str, mode: OutputMode) -> Vec<Candidate> {
    EnSentenceTranslator::translate_en_sentence(input, mode)
}
