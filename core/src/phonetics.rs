//! 多拼音方案正規化與粵語懶音/模糊音展開模組
//!
//! 支援：
//! 1. 標準粵拼 (Jyutping)
//! 2. 教育學院拼音方案 (ILE: dz->z, ts->c, oen->eon, oet->eot, y->jyu)
//! 3. 耶魯拼音 (Yale: ch->c, sh->s, j->z, y->j, eu->oe/eo, yu->jyu)
//! 4. 劉錫祥拼音 (Sidney Lau: ch->c, sh->s, j->z, y->j, oo->u, ue->yu)
//! 5. 港式常見俗拼 (如 sun->san, lee->lei, ho->hou, lay->lei)
//! 6. 懶音與模糊音容錯 (n/l, ng/零聲母, gw/g, kw/k, -n/-ng, -t/-k)

use std::collections::HashSet;

/// 拼音查詢展開後的變體結構
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneticQuery {
    /// 展開後的標準粵拼字串（無聲調）
    pub jyutping: String,
    /// 是否經過懶音/模糊音容錯轉換
    pub is_fuzzy: bool,
    /// 優先級調整權重（標準直擊 = 0，多制式正規化 = -10，懶音容錯 = -50）
    pub penalty: i32,
}

/// 清理輸入字串：保留英文字母並轉小寫，移除聲調數字 (1-9) 與空白或符號
pub fn sanitize_input(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// 將輸入的單音節或多音節拼音正規化為可能的標準粵拼候選清單（不含懶音）
pub fn normalize_to_jyutping(raw_input: &str) -> Vec<PhoneticQuery> {
    let cleaned = sanitize_input(raw_input);
    if cleaned.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();
    let mut seen = HashSet::new();

    // 1. 原始清理後字串永遠作為第一優先（標準粵拼直擊）
    seen.insert(cleaned.clone());
    results.push(PhoneticQuery {
        jyutping: cleaned.clone(),
        is_fuzzy: false,
        penalty: 0,
    });

    // 2. 針對單音節與常見複合規則進行多拼音方案正規化
    let normalized_variants = apply_scheme_normalization(&cleaned);
    for variant in normalized_variants {
        if !variant.is_empty() && seen.insert(variant.clone()) {
            results.push(PhoneticQuery {
                jyutping: variant,
                is_fuzzy: false,
                penalty: -10,
            });
        }
    }

    results
}

/// 展開包含「多拼音正規化」與「懶音/模糊音容錯」的所有查詢變體
pub fn expand_phonetic_queries(raw_input: &str) -> Vec<PhoneticQuery> {
    let base_queries = normalize_to_jyutping(raw_input);
    if base_queries.is_empty() {
        return Vec::new();
    }

    let mut all_queries = base_queries.clone();
    let mut seen: HashSet<String> = base_queries.iter().map(|q| q.jyutping.clone()).collect();

    // 對每個正規化後的粵拼進行懶音容錯展開
    for base in &base_queries {
        let fuzzy_variants = expand_fuzzy_syllable(&base.jyutping);
        for fv in fuzzy_variants {
            if !fv.is_empty() && seen.insert(fv.clone()) {
                all_queries.push(PhoneticQuery {
                    jyutping: fv,
                    is_fuzzy: true,
                    penalty: base.penalty - 50,
                });
            }
        }
    }

    all_queries
}

/// 多制式拼音（教院、耶魯、劉錫祥、港式俗拼）轉標準粵拼
fn apply_scheme_normalization(s: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    // 支援將全字串做聲母與韻母正規化轉換
    let mut working_set = vec![s.to_string()];

    // A. 聲母正規化（教院 dz/ts、耶魯/劉錫祥/港式 ch/sh/y/j）
    let mut after_initials = Vec::new();
    for item in &working_set {
        after_initials.push(item.clone());
        if let Some(rest) = item.strip_prefix("dz") {
            after_initials.push(format!("z{rest}"));
        }
        if let Some(rest) = item.strip_prefix("ts") {
            after_initials.push(format!("c{rest}"));
        }
        if let Some(rest) = item.strip_prefix("ch") {
            after_initials.push(format!("c{rest}"));
        }
        if let Some(rest) = item.strip_prefix("sh") {
            after_initials.push(format!("s{rest}"));
        }
        // 耶魯/劉錫祥的 y- 聲母對應粵拼 j- 或 jyu-
        if let Some(rest) = item.strip_prefix("yu") {
            // 如 yu -> jyu, yuet -> jyut, yuen -> jyun, yung -> jung
            if rest.is_empty() {
                after_initials.push("jyu".to_string());
            } else if rest == "et" || rest == "t" {
                after_initials.push("jyut".to_string());
            } else if rest == "en" || rest == "n" {
                after_initials.push("jyun".to_string());
            } else if rest == "ng" || rest == "k" {
                after_initials.push(format!("ju{rest}"));
            } else {
                after_initials.push(format!("jyu{rest}"));
            }
        } else if let Some(rest) = item.strip_prefix('y') {
            if !rest.is_empty() {
                after_initials.push(format!("j{rest}"));
            }
        }
        // 耶魯/劉錫祥的 j- 聲母可對應粵拼 z-（如 jat -> zat, jing -> zing）
        if let Some(rest) = item.strip_prefix('j') {
            if !rest.is_empty() {
                after_initials.push(format!("z{rest}"));
            }
        }
    }
    working_set = after_initials;

    // B. 韻母與港式常見拼法正規化
    for item in &working_set {
        // 1. 通用字串層級替換（處理多音節或單音節中常見的教院/耶魯/劉錫祥韻母）
        let mut general = item.clone();
        if general.contains("oo") {
            general = general.replace("oo", "u");
            candidates.push(general.clone());
        }
        if item.contains("ee") {
            candidates.push(item.replace("ee", "i"));
            candidates.push(item.replace("ee", "ei"));
        }
        if item.contains("oen") {
            candidates.push(item.replace("oen", "eon"));
        }
        if item.contains("oet") {
            candidates.push(item.replace("oet", "eot"));
        }
        if item.contains("eung") {
            candidates.push(item.replace("eung", "oeng"));
        }
        if item.contains("euk") {
            candidates.push(item.replace("euk", "oek"));
        }
        if item.contains("eun") {
            candidates.push(item.replace("eun", "eon"));
        }
        if item.contains("eut") {
            candidates.push(item.replace("eut", "eot"));
        }
        if item.contains("uen") {
            candidates.push(item.replace("uen", "yun"));
        }
        if item.contains("uet") {
            candidates.push(item.replace("uet", "yut"));
        }
        if item.ends_with("ue") {
            candidates.push(format!("{}yu", &item[..item.len() - 2]));
        }

        // 2. 港式俗拼韻尾：-un -> -an (如 sun -> san, lun -> lan, chun -> can, fun -> fan)
        if let Some(prefix) = item.strip_suffix("un") {
            if !prefix.is_empty() {
                candidates.push(format!("{prefix}an"));
                candidates.push(format!("{prefix}eon"));
            }
        }

        // 3. 港式俗拼韻尾：-ay -> -ei (如 lay -> lei, may -> mei, kay -> kei)
        if let Some(prefix) = item.strip_suffix("ay") {
            if !prefix.is_empty() {
                candidates.push(format!("{prefix}ei"));
                candidates.push(format!("{prefix}aai"));
            }
        }

        // 4. 劉錫祥/港式：單音節 -o -> -ou (如 ho -> hou, lo -> lou, do -> dou, to -> tou, go -> gou)
        if let Some(prefix) = item.strip_suffix('o') {
            if !prefix.is_empty() && !prefix.ends_with('o') && prefix.len() <= 2 {
                candidates.push(format!("{prefix}ou"));
            }
        }

        // 5. 耶魯/劉錫祥/教院：單音節長 a 省寫 (如 ga -> gaa, ma -> maa, fa -> faa)
        if let Some(prefix) = item.strip_suffix('a') {
            if !prefix.is_empty() && !prefix.ends_with('a') && prefix.len() <= 2 {
                candidates.push(format!("{prefix}aa"));
            }
        }

        // 6. 港式俗拼：-ow -> -au / -ou (如 how -> hau/hou, chow -> cau)
        if let Some(prefix) = item.strip_suffix("ow") {
            if !prefix.is_empty() {
                candidates.push(format!("{prefix}au"));
                candidates.push(format!("{prefix}ou"));
            }
        }

        candidates.push(item.clone());
    }

    candidates
}

/// 針對粵拼字串展開懶音/模糊音容錯變體：
/// - n- <-> l- 聲母互換（如 lei <-> nei）
/// - ng- <-> 零聲母 互換（如 ngaan <-> aan, ngo <-> o）
/// - gw- <-> g- 與 kw- <-> k- 互換（如 gwong <-> gong, kwong <-> kong）
/// - -n <-> -ng 與 -t <-> -k 韻尾互換（如 san <-> sang, jat <-> jak）
fn expand_fuzzy_syllable(jp: &str) -> Vec<String> {
    let mut initial_variants = vec![jp.to_string()];

    // 1. 聲母模糊音展開
    // n <-> l
    if let Some(rest) = jp.strip_prefix('l') {
        if !rest.is_empty() {
            initial_variants.push(format!("n{rest}"));
        }
    } else if let Some(rest) = jp.strip_prefix("ng") {
        // ng -> 零聲母（如 ngaan -> aan, ngo -> o）
        if !rest.is_empty() {
            initial_variants.push(rest.to_string());
        }
    } else if let Some(rest) = jp.strip_prefix('n') {
        if !rest.is_empty() {
            initial_variants.push(format!("l{rest}"));
        }
    }

    // 零聲母（母音 a, e, o, u 開頭） -> ng-
    if jp.starts_with(['a', 'e', 'o']) {
        initial_variants.push(format!("ng{jp}"));
    }

    // 多音節詞內部的 ng- <-> 零聲母 懶音容錯（如 sikaan <-> sikngaan）
    for boundary in ["p", "t", "k", "m", "n", "ng", "i", "u"] {
        for vowel in ["aa", "a", "o", "e"] {
            let zero_pat = format!("{boundary}{vowel}");
            let ng_pat = format!("{boundary}ng{vowel}");
            if jp.contains(&zero_pat) {
                initial_variants.push(jp.replacen(&zero_pat, &ng_pat, 1));
            }
            if jp.contains(&ng_pat) {
                initial_variants.push(jp.replacen(&ng_pat, &zero_pat, 1));
            }
        }
    }

    // gw <-> g, kw <-> k
    if let Some(rest) = jp.strip_prefix("gw") {
        if !rest.is_empty() {
            initial_variants.push(format!("g{rest}"));
        }
    } else if let Some(rest) = jp.strip_prefix('g') {
        if rest.starts_with(['a', 'o', 'u', 'w']) {
            initial_variants.push(format!("gw{rest}"));
        }
    }

    if let Some(rest) = jp.strip_prefix("kw") {
        if !rest.is_empty() {
            initial_variants.push(format!("k{rest}"));
        }
    } else if let Some(rest) = jp.strip_prefix('k') {
        if rest.starts_with(['a', 'o', 'u', 'w']) {
            initial_variants.push(format!("kw{rest}"));
        }
    }

    // 2. 韻尾模糊音展開 (-n <-> -ng, -t <-> -k)
    let mut all_variants = Vec::new();
    for v in &initial_variants {
        all_variants.push(v.clone());

        if let Some(stem) = v.strip_suffix("ng") {
            if !stem.is_empty() && stem.ends_with(['a', 'e', 'i', 'o', 'u']) {
                all_variants.push(format!("{stem}n"));
            }
        } else if let Some(stem) = v.strip_suffix('n') {
            if !stem.is_empty() && stem.ends_with(['a', 'e', 'i', 'o', 'u']) {
                all_variants.push(format!("{stem}ng"));
            }
        } else if let Some(stem) = v.strip_suffix('t') {
            if !stem.is_empty() && stem.ends_with(['a', 'e', 'i', 'o', 'u']) {
                all_variants.push(format!("{stem}k"));
            }
        } else if let Some(stem) = v.strip_suffix('k') {
            if !stem.is_empty() && stem.ends_with(['a', 'e', 'i', 'o', 'u']) {
                all_variants.push(format!("{stem}t"));
            }
        }
    }

    // 移除與原始輸入完全相同的項目
    all_variants.into_iter().filter(|x| x != jp).collect()
}

/// 音節切分節點類型（全拼音節或聲母簡拼節點）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyllableKind {
    /// 完整粵拼音節（含標準粵拼與多制式正規化）
    Full(String),
    /// 聲母簡拼節點（含單字母聲母與正規化後之複合聲母）
    Abbr(String),
}

/// 單一音節在原始輸入字串中的跨度與正規化結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyllableSpan {
    /// 在原始 input 中的起始位元組索引 (inclusive)
    pub start_byte: usize,
    /// 在原始 input 中的結束位元組索引 (exclusive)
    pub end_byte: usize,
    /// 正規化後的無聲調粵拼或聲母簡拼字串
    pub normalized_jyutping: String,
    /// 音節類型：`Full(String)` 或 `Abbr(String)`
    pub kind: SyllableKind,
    /// 該音節對應的首字母（供簡拼索引 `idx_yue_abbr` 查詢）
    pub abbr_char: char,
    /// 是否為簡拼/前綴節點（相容欄位，`matches!(kind, SyllableKind::Abbr(_))`）
    pub is_prefix: bool,
}

impl SyllableSpan {
    /// 以 `(start_byte, end_byte, normalized_jyutping)` 元組形式取得跨度資訊
    pub fn as_tuple(&self) -> (usize, usize, &str) {
        (self.start_byte, self.end_byte, &self.normalized_jyutping)
    }
}

/// 合法粵拼單字母子音聲母表（含耶魯/劉錫祥之 y 聲母）
const VALID_SINGLE_CONSONANT_INITIALS: &[char] = &[
    'b', 'c', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'm', 'n', 'p', 's', 't', 'w', 'y', 'z',
];

/// 標準無聲調粵拼單音節總表（嚴格依字典序排列以供 `binary_search` 使用）
const STANDARD_JYUTPING_SYLLABLES: &[&str] = &[
    "a", "aa", "aai", "aak", "aam", "aan", "aang", "aap", "aat", "aau", "ai", "ak", "am", "an",
    "ang", "ap", "at", "au", "baa", "baai", "baak", "baan", "baang", "baat", "baau", "bai", "bak",
    "bam", "ban", "bang", "bat", "bau", "be", "bei", "bek", "beng", "bi", "bik", "bin", "bing",
    "bit", "biu", "bo", "bok", "bon", "bong", "bou", "bui", "buk", "bun", "bung", "but", "caa",
    "caai", "caak", "caam", "caan", "caang", "caap", "caat", "caau", "cai", "cak", "cam", "can",
    "cang", "cap", "cat", "cau", "ce", "cek", "ceng", "ceoi", "ceon", "ceot", "ci", "cik", "cim",
    "cin", "cing", "cip", "cit", "ciu", "co", "coek", "coeng", "coet", "coi", "cok", "cong", "cou",
    "cuk", "cung", "cyu", "cyun", "cyut", "daa", "daai", "daak", "daam", "daan", "daap", "daat",
    "dai", "dak", "dam", "dan", "dang", "dap", "dat", "dau", "de", "dei", "dek", "deng", "deoi",
    "deon", "deot", "deu", "di", "dik", "dim", "din", "ding", "dip", "dit", "diu", "do", "doe",
    "doek", "doeng", "doi", "dok", "dong", "dou", "duk", "dung", "dyun", "dyut", "e", "ei", "em",
    "en", "et", "eu", "faa", "faai", "faak", "faan", "faat", "fai", "fan", "fang", "fat", "fau",
    "fe", "fei", "fik", "fing", "fit", "fiu", "fo", "fok", "fong", "fu", "fui", "fuk", "fun",
    "fung", "fut", "ga", "gaa", "gaai", "gaak", "gaam", "gaan", "gaang", "gaap", "gaat", "gaau",
    "gai", "gak", "gam", "gan", "gang", "gap", "gat", "gau", "ge", "gei", "geng", "geoi", "gep",
    "gi", "gik", "gim", "gin", "ging", "gip", "git", "giu", "go", "goe", "goek", "goeng", "goi",
    "gok", "gon", "gong", "got", "gou", "gu", "gui", "guk", "gun", "gung", "gut", "gwaa",
    "gwaai", "gwaak", "gwaan", "gwaang", "gwaat", "gwai", "gwak", "gwan", "gwang", "gwat", "gwe",
    "gwei", "gwi", "gwik", "gwing", "gwit", "gwo", "gwok", "gwong", "gyun", "gyut", "haa", "haai",
    "haak", "haam", "haan", "haang", "haap", "haau", "hai", "hak", "ham", "han", "hang", "hap",
    "hat", "hau", "he", "hei", "hek", "heng", "heoi", "hi", "hik", "him", "hin", "hing", "hip",
    "hit", "hiu", "hm", "hng", "ho", "hoe", "hoeng", "hoi", "hok", "hon", "hong", "hot", "hou",
    "huk", "hung", "hyun", "hyut", "it", "jaa", "jaai", "jaak", "jaang", "jaap", "jaau", "jai",
    "jam", "jan", "jap", "jat", "jau", "je", "jeng", "jeoi", "jeon", "ji", "jik", "jim", "jin",
    "jing", "jip", "jit", "jiu", "jo", "joek", "joeng", "juk", "jung", "jyu", "jyun", "jyut", "kaa",
    "kaai", "kaak", "kaam", "kaat", "kaau", "kai", "kak", "kam", "kan", "kang", "kap", "kat", "kau",
    "ke", "kei", "kek", "keoi", "kep", "keu", "kik", "kim", "kin", "king", "kip", "kit", "kiu",
    "ko", "koe", "koek", "koeng", "koi", "kok", "kong", "ku", "kui", "kuk", "kung", "kut", "kwaa",
    "kwaai", "kwaak", "kwaang", "kwai", "kwan", "kwang", "kwik", "kwok", "kwong", "kyun", "kyut",
    "la", "laa", "laai", "laak", "laam", "laan", "laang", "laap", "laat", "laau", "lai", "lak",
    "lam", "lan", "lang", "lap", "lat", "lau", "le", "lei", "lek", "lem", "leng", "leoi", "leon",
    "leot", "leu", "li", "lik", "lim", "lin", "ling", "lip", "lit", "liu", "lo", "loe", "loek",
    "loeng", "loi", "lok", "long", "lou", "lu", "luk", "lung", "lyun", "lyut", "m", "ma", "maa", "maai",
    "maak", "maan", "maang", "maat", "maau", "mai", "mak", "mam", "man", "mang", "mat", "mau", "me",
    "mei", "meng", "meu", "mi", "mik", "min", "ming", "mit", "miu", "mo", "mok", "mong", "mou",
    "mui", "muk", "mun", "mung", "mut", "naa", "naai", "naam", "naan", "naap", "naat", "naau",
    "nai", "nak", "nam", "nan", "nang", "nap", "nat", "nau", "ne", "nei", "neng", "neoi", "neot",
    "ng", "ngaa", "ngaai", "ngaak", "ngaam", "ngaan", "ngaang", "ngaap", "ngaat", "ngaau", "ngai",
    "ngak", "ngam", "ngan", "ngang", "ngap", "ngat", "ngau", "nge", "ngit", "ngo", "ngoi", "ngok",
    "ngon", "ngong", "ngot", "ngou", "ni", "nik", "nim", "nin", "ning", "nip", "nit", "niu", "no",
    "noek", "noeng", "noi", "nok", "nong", "nou", "nuk", "nung", "nyun", "o", "oet", "oi", "ok",
    "on", "ong", "ou", "paa", "paai", "paak", "paan", "paang", "paat", "paau", "pai", "pan",
    "pang", "pat", "pau", "pe", "pei", "pek", "peng", "pet", "pi", "pik", "pin", "ping", "pit",
    "piu", "po", "poi", "pok", "pong", "pou", "pui", "puk", "pun", "pung", "put", "saa", "saai",
    "saak", "saam", "saan", "saang", "saap", "saat", "saau", "sai", "sak", "sam", "san", "sang",
    "sap", "sat", "sau", "se", "sei", "sek", "seng", "seoi", "seon", "seot", "set", "si", "sik",
    "sim", "sin", "sing", "sip", "sit", "siu", "so", "soe", "soek", "soeng", "soi", "sok", "song",
    "sou", "suk", "sung", "syu", "syun", "syut", "taa", "taai", "taam", "taan", "taap", "taat",
    "tai", "tam", "tan", "tang", "tap", "tau", "tek", "teng", "teoi", "teon", "teot", "ti", "tik",
    "tim", "tin", "ting", "tip", "tit", "tiu", "to", "toe", "toi", "tok", "tong", "tou", "tuk",
    "tung", "tyun", "tyut", "uk", "ung", "waa", "waai", "waak", "waan", "waang", "waat", "wai",
    "wak", "wan", "wang", "wat", "we", "wet", "wi", "wik", "wing", "wo", "wok", "wong", "wu",
    "wui", "wun", "wut", "zaa", "zaai", "zaak", "zaam", "zaan", "zaang", "zaap", "zaat", "zaau",
    "zai", "zak", "zam", "zan", "zang", "zap", "zat", "zau", "ze", "zei", "zek", "zem", "zeng",
    "zeoi", "zeon", "zeot", "zep", "zi", "zik", "zim", "zin", "zing", "zip", "zit", "ziu", "zo",
    "zoe", "zoek", "zoeng", "zoet", "zoi", "zok", "zong", "zou", "zuk", "zung", "zyu", "zyun",
    "zyut",
];

#[inline]
pub fn is_standard_jyutping_syllable(s: &str) -> bool {
    STANDARD_JYUTPING_SYLLABLES.binary_search(&s).is_ok()
}

/// 判斷音節字串是否包含標準粵拼母音（用於避免純子音 `"hm"`, `"hng"`, `"m"`, `"ng"` 在混合切分時誤吞相鄰聲母）
#[inline]
fn contains_vowel(s: &str) -> bool {
    s.bytes().any(|b| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u'))
}

/// 從標準化粵拼或聲母字串擷取首字母（若為多制式聲母則先正規化為標準粵拼首字母）
#[inline]
pub fn extract_initial_abbr_char(syl: &str) -> char {
    if syl.starts_with("ch") || syl.starts_with("ts") {
        'c'
    } else if syl.starts_with("sh") {
        's'
    } else if syl.starts_with("dz") {
        'z'
    } else if syl.starts_with('y') {
        'j'
    } else {
        syl.chars().next().unwrap_or(' ')
    }
}

/// 針對單一粵拼音節計算偏好加權，避免將閉音節拆成罕用開音節 + 零聲母
fn syllable_preference_bonus(syl: &str) -> i32 {
    if matches!(syl, "a" | "e" | "o") {
        return -400;
    }
    if matches!(
        syl,
        "ga" | "la" | "ma" | "bi" | "ti" | "pi" | "de" | "ke" | "be" | "fe" | "pe" | "we" | "hoe"
            | "coe" | "doe" | "goe" | "joe" | "koe" | "loe" | "soe" | "toe" | "zoe" | "hm"
            | "hng" | "it" | "et" | "eu"
    ) {
        return -400;
    }
    if syl.len() >= 3 || syl.ends_with(['i', 'u', 'm', 'n', 'p', 't', 'k']) || syl.ends_with("ng")
    {
        return 150;
    }
    0
}

/// 嘗試將子字串正規化為標準粵拼單音節，回傳 `(標準無聲調粵拼, 方案微調分數)`
fn match_syllable_variant(raw_slice: &str) -> Option<(String, i32)> {
    if is_standard_jyutping_syllable(raw_slice) {
        return Some((raw_slice.to_string(), syllable_preference_bonus(raw_slice)));
    }
    // 支援教院/耶魯/劉錫祥/港式音節正規化（僅於長度 >= 2 時進行，避免單字元過度展開）
    if raw_slice.len() >= 2 {
        for pq in normalize_to_jyutping(raw_slice) {
            if pq.penalty != 0 && is_standard_jyutping_syllable(&pq.jyutping) {
                return Some((
                    pq.jyutping.clone(),
                    syllable_preference_bonus(&pq.jyutping) - 200,
                ));
            }
        }
    }
    None
}

/// 雙態混合音節切分器：在一段連續輸入中混合切出 `Full`（全拼音節）與 `Abbr`（聲母簡拼）節點
/// - 相容標準粵拼及教院/耶魯/劉錫祥聲母正規化（如 `ch->c`, `sh->s`, `ts->c`, `dz->z`, `y->j`）
/// - 例：`"gamjathg"` -> `[Full("gam"), Full("jat"), Abbr("h"), Abbr("g")]`
/// - 例：`"ngogjhhg"` -> `[Full("ngo"), Abbr("g"), Abbr("j"), Abbr("h"), Abbr("h"), Abbr("g")]`
/// - 例：`"nhma"` -> `[Abbr("n"), Abbr("h"), Full("ma")]`
pub fn segment_hybrid_syllables(input: &str) -> Vec<SyllableSpan> {
    let mut letters: Vec<(usize, usize, char)> = Vec::with_capacity(input.len());
    for (byte_idx, ch) in input.char_indices() {
        if ch.is_ascii_alphabetic() {
            let lower = ch.to_ascii_lowercase();
            // q / r / v / x 不屬於粵拼聲母或韻母，直接視為非純粵拼字串
            if matches!(lower, 'q' | 'r' | 'v' | 'x') {
                return Vec::new();
            }
            letters.push((byte_idx, byte_idx + ch.len_utf8(), lower));
        }
    }

    let n = letters.len();
    if n == 0 {
        return Vec::new();
    }

    let cleaned: String = letters.iter().map(|&(_, _, c)| c).collect();

    #[derive(Clone)]
    struct DpNode {
        score: i32,
        prev: usize,
        kind: SyllableKind,
        normalized: String,
    }

    let neg_inf = i32::MIN / 4;
    let mut dp: Vec<Option<DpNode>> = vec![None; n + 1];
    dp[0] = Some(DpNode {
        score: 0,
        prev: 0,
        kind: SyllableKind::Full(String::new()),
        normalized: String::new(),
    });

    for start in 0..n {
        let Some(prev_score) = dp[start].as_ref().map(|d| d.score) else {
            continue;
        };
        if prev_score <= neg_inf {
            continue;
        }

        // 1. 嘗試匹配長度 1..=6 的完整含母音粵拼音節 (Full)
        let max_end = (start + 6).min(n);
        for end in (start + 1)..=max_end {
            let slice = &cleaned[start..end];
            if !contains_vowel(slice) {
                continue;
            }
            if let Some((norm_jp, bonus)) = match_syllable_variant(slice) {
                let len = (end - start) as i32;
                let step_score = if len == 1 {
                    -4200 + bonus
                } else {
                    -2000 + (len * 800) + bonus
                };
                let candidate_score = prev_score + step_score;

                let replace = match &dp[end] {
                    None => true,
                    Some(existing) => {
                        candidate_score > existing.score
                            || (candidate_score == existing.score
                                && matches!(existing.kind, SyllableKind::Abbr(_)))
                    }
                };
                if replace {
                    dp[end] = Some(DpNode {
                        score: candidate_score,
                        prev: start,
                        kind: SyllableKind::Full(norm_jp.clone()),
                        normalized: norm_jp,
                    });
                }
            }
        }

        // 2. 嘗試匹配多制式雙字母複合聲母簡拼 (ch/ts/sh/dz -> c/s/z)
        if start + 2 <= n {
            let di = &cleaned[start..start + 2];
            let norm_di = match di {
                "ch" | "ts" => Some("c"),
                "sh" => Some("s"),
                "dz" => Some("z"),
                _ => None,
            };
            if let Some(norm) = norm_di {
                let end = start + 2;
                let candidate_score = prev_score - 4200;
                let replace = match &dp[end] {
                    None => true,
                    Some(existing) => candidate_score > existing.score,
                };
                if replace {
                    let norm_str = norm.to_string();
                    dp[end] = Some(DpNode {
                        score: candidate_score,
                        prev: start,
                        kind: SyllableKind::Abbr(norm_str.clone()),
                        normalized: norm_str,
                    });
                }
            }
        }

        // 3. 嘗試匹配單字母子音聲母簡拼 (Abbr)
        let ch = cleaned.as_bytes()[start] as char;
        if VALID_SINGLE_CONSONANT_INITIALS.contains(&ch) {
            let end = start + 1;
            let norm_ch = if ch == 'y' { 'j' } else { ch };
            let candidate_score = prev_score - 4500;
            let replace = match &dp[end] {
                None => true,
                Some(existing) => candidate_score > existing.score,
            };
            if replace {
                let norm_str = norm_ch.to_string();
                dp[end] = Some(DpNode {
                    score: candidate_score,
                    prev: start,
                    kind: SyllableKind::Abbr(norm_str.clone()),
                    normalized: norm_str,
                });
            }
        }
    }

    if dp[n].is_none() {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut cur = n;
    while cur > 0 {
        let Some(node) = dp[cur].as_ref() else {
            return Vec::new();
        };
        let start_byte = letters[node.prev].0;
        let end_byte = letters[cur - 1].1;
        let abbr_char = extract_initial_abbr_char(&node.normalized);
        let is_prefix = matches!(node.kind, SyllableKind::Abbr(_));
        spans.push(SyllableSpan {
            start_byte,
            end_byte,
            normalized_jyutping: node.normalized.clone(),
            kind: node.kind.clone(),
            abbr_char,
            is_prefix,
        });
        cur = node.prev;
    }
    spans.reverse();
    spans
}

/// 將連續無空格粵拼字串切分為混合音節序列（委派至 `segment_hybrid_syllables`）
pub fn segment_syllables_with_spans(input: &str) -> Vec<SyllableSpan> {
    segment_hybrid_syllables(input)
}

/// 將連續無空格粵拼字串格式化為帶單引號分隔的音節字串（供 UI 頂部清晰顯示）
/// 例如 `"ngogamjatheoihoenggong"` -> `"ngo'gam'jat'heoi'hoeng'gong"`
/// 例如 `"gamjathg"` -> `"gam'jat'h'g"`
pub fn format_segmented_pinyin(input: &str) -> String {
    let trimmed = input.trim_start();
    if trimmed.starts_with(';') || trimmed.starts_with('/') || input.contains(' ') {
        return input.to_string();
    }
    let spans = segment_hybrid_syllables(input);
    if spans.len() < 2 {
        return input.to_string();
    }
    let parts: Vec<&str> = spans
        .iter()
        .filter_map(|s| input.get(s.start_byte..s.end_byte))
        .collect();
    if parts.len() < 2 {
        input.to_string()
    } else {
        parts.join("'")
    }
}

