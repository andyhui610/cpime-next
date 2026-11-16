//! Android JNI 橋接模組 (`com.andyhui.cpime.NativeEngine`)
//!
//! 暴露 C ABI / JNI 函式給 Kotlin 前端呼叫，支援引擎生命週期管理、音節切分顯示、
//! 候選詞查詢（JSON）、候選詞選取上屏（含個人學習與自造詞合成）、Bigram 聯想詞及口語/書面語切換。

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jlong, jstring, JNI_FALSE};
use jni::JNIEnv;
use serde::Serialize;

use crate::engine::{Candidate, CpimeEngine, OutputMode};
use crate::phonetics::format_segmented_pinyin;

/// JNI 回傳給 Android 前端之候選詞 JSON 結構（同時相容 snake_case 與 camelCase 欄位）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct JniCandidateDto {
    word: String,
    matched_code: String,
    #[serde(rename = "matchedCode")]
    matched_code_camel: String,
    consumed_chars: usize,
    #[serde(rename = "consumedChars")]
    consumed_chars_camel: usize,
    source: String,
    score: i32,
}

impl From<Candidate> for JniCandidateDto {
    fn from(c: Candidate) -> Self {
        let matched = c.matched_code;
        Self {
            word: c.word,
            matched_code: matched.clone(),
            matched_code_camel: matched,
            consumed_chars: c.consumed_chars,
            consumed_chars_camel: c.consumed_chars,
            source: c.source.label().to_string(),
            score: c.score,
        }
    }
}

/// 輔助函式：安全將 `JString` 轉換為 Rust `String`
fn get_string(env: &mut JNIEnv, jstr: &JString) -> String {
    if jstr.is_null() {
        return String::new();
    }
    env.get_string(jstr)
        .map(|s| s.into())
        .unwrap_or_default()
}

/// 輔助函式：安全建立回傳的 `jstring`，若建立失敗則回傳 null 指標
fn to_jstring(env: &mut JNIEnv, text: &str) -> jstring {
    match env.new_string(text) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 輔助函式：將候選詞清單序列化為 JSON 陣列字串
pub fn serialize_candidates_json(candidates: Vec<Candidate>) -> String {
    let dtos: Vec<JniCandidateDto> = candidates.into_iter().map(JniCandidateDto::from).collect();
    serde_json::to_string(&dtos).unwrap_or_else(|_| "[]".to_string())
}

/// 輔助函式：從 `jlong` 取得可變引擎參考（需確保呼叫端不並發存取同一指標且指標仍有效）
unsafe fn engine_mut<'a>(ptr: jlong) -> Option<&'a mut CpimeEngine> {
    if ptr == 0 {
        None
    } else {
        Some(&mut *(ptr as *mut CpimeEngine))
    }
}

/// 輔助函式：從 `jlong` 取得唯讀引擎參考
unsafe fn engine_ref<'a>(ptr: jlong) -> Option<&'a CpimeEngine> {
    if ptr == 0 {
        None
    } else {
        Some(&*(ptr as *const CpimeEngine))
    }
}

fn init_engine_impl(sys_path: &str, user_path: &str) -> jlong {
    if sys_path.trim().is_empty() {
        return 0;
    }

    let engine = if user_path.trim().is_empty() {
        CpimeEngine::open(sys_path)
    } else {
        CpimeEngine::open_with_user_dict(sys_path, user_path)
            .or_else(|_| CpimeEngine::open(sys_path))
    };

    match engine {
        Ok(eng) => Box::into_raw(Box::new(eng)) as jlong,
        Err(_) => 0,
    }
}

/// 初始化輸入法引擎，回傳 `CpimeEngine` 原生指標（失敗回傳 `0`）
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    sys_db_path: JString,
    user_db_path: JString,
) -> jlong {
    let sys_path = get_string(&mut env, &sys_db_path);
    let user_path = get_string(&mut env, &user_db_path);
    init_engine_impl(&sys_path, &user_path)
}

/// 釋放 `CpimeEngine` 原生記憶體
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    engine_ptr: jlong,
) {
    if engine_ptr != 0 {
        unsafe {
            drop(Box::from_raw(engine_ptr as *mut CpimeEngine));
        }
    }
}

/// 格式化連續無空格粵拼字串為帶單引號分隔的音節（如 `ngo'gam'jat`）
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeFormatPinyin(
    mut env: JNIEnv,
    _class: JClass,
    _engine_ptr: jlong,
    input: JString,
) -> jstring {
    let raw = get_string(&mut env, &input);
    let formatted = format_segmented_pinyin(&raw);
    to_jstring(&mut env, &formatted)
}

/// 查詢候選詞並回傳 JSON 陣列（含 `word`, `consumed_chars`, `consumedChars`, `source`, `matched_code`, `matchedCode`, `score`）
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeGetCandidatesJson(
    mut env: JNIEnv,
    _class: JClass,
    engine_ptr: jlong,
    input: JString,
    limit: jint,
) -> jstring {
    let raw = get_string(&mut env, &input);
    if limit <= 0 {
        return to_jstring(&mut env, "[]");
    }

    let json = unsafe {
        match engine_ref(engine_ptr) {
            Some(engine) => {
                let candidates = engine.lookup_with_limit(&raw, limit as usize);
                serialize_candidates_json(candidates)
            }
            None => "[]".to_string(),
        }
    };
    to_jstring(&mut env, &json)
}

/// 選中第 `index` 個候選詞（0-based），更新個人學習詞庫與自造詞緩衝區，並回傳實際上屏的文字
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeCommitCandidate(
    mut env: JNIEnv,
    _class: JClass,
    engine_ptr: jlong,
    input: JString,
    index: jint,
) -> jstring {
    let raw = get_string(&mut env, &input);
    if index < 0 {
        return to_jstring(&mut env, "");
    }

    let committed = unsafe {
        match engine_mut(engine_ptr) {
            Some(engine) => engine
                .commit_candidate(&raw, index as usize)
                .unwrap_or_default(),
            None => String::new(),
        }
    };
    to_jstring(&mut env, &committed)
}

/// 查詢 Bigram 語境聯想詞（當未輸入拼音且 `last_committed_word` 存在時）
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeGetPredictionsJson(
    mut env: JNIEnv,
    _class: JClass,
    engine_ptr: jlong,
    limit: jint,
) -> jstring {
    if limit <= 0 {
        return to_jstring(&mut env, "[]");
    }

    let json = unsafe {
        match engine_ref(engine_ptr) {
            Some(engine) => {
                let preds = engine.get_predictions(limit as usize);
                serialize_candidates_json(preds)
            }
            None => "[]".to_string(),
        }
    };
    to_jstring(&mut env, &json)
}

/// 切換輸出語體模式（`true` = 書面語模式，`false` = 道地口語模式）
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeSetOutputMode(
    _env: JNIEnv,
    _class: JClass,
    engine_ptr: jlong,
    is_written: jboolean,
) {
    unsafe {
        if let Some(engine) = engine_mut(engine_ptr) {
            let mode = if is_written != JNI_FALSE {
                OutputMode::Written
            } else {
                OutputMode::Colloquial
            };
            engine.set_output_mode(mode);
        }
    }
}

/// 切換英文整句直出廣東話模式（`true` = 啟用整句模式，`false` = 一般粵拼雙軌模式）
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeSetEnSentenceMode(
    _env: JNIEnv,
    _class: JClass,
    engine_ptr: jlong,
    enabled: jboolean,
) {
    unsafe {
        if let Some(engine) = engine_mut(engine_ptr) {
            engine.set_en_sentence_mode(enabled != JNI_FALSE);
        }
    }
}

/// 清空當前上下文並結算自造詞緩衝區
#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_nativeClearContext(
    _env: JNIEnv,
    _class: JClass,
    engine_ptr: jlong,
) {
    unsafe {
        if let Some(engine) = engine_mut(engine_ptr) {
            engine.clear_context();
        }
    }
}

// =========================================================================
// GEMINI.md 第 9 節別名 JNI 入口（直接委派至核心實作，維持雙向名稱相容）
// =========================================================================

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_initEngine(
    env: JNIEnv,
    class: JClass,
    sys_db_path: JString,
    user_db_path: JString,
) -> jlong {
    Java_com_andyhui_cpime_NativeEngine_nativeInit(env, class, sys_db_path, user_db_path)
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_destroyEngine(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
) {
    Java_com_andyhui_cpime_NativeEngine_nativeDestroy(env, class, engine_ptr);
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_getSegmentedPinyin(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
    input: JString,
) -> jstring {
    Java_com_andyhui_cpime_NativeEngine_nativeFormatPinyin(env, class, engine_ptr, input)
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_getCandidatesJson(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
    input: JString,
    limit: jint,
) -> jstring {
    Java_com_andyhui_cpime_NativeEngine_nativeGetCandidatesJson(env, class, engine_ptr, input, limit)
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_commitCandidate(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
    input: JString,
    index: jint,
) -> jstring {
    Java_com_andyhui_cpime_NativeEngine_nativeCommitCandidate(env, class, engine_ptr, input, index)
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_getPredictionsJson(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
    limit: jint,
) -> jstring {
    Java_com_andyhui_cpime_NativeEngine_nativeGetPredictionsJson(env, class, engine_ptr, limit)
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_setOutputMode(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
    is_written: jboolean,
) {
    Java_com_andyhui_cpime_NativeEngine_nativeSetOutputMode(env, class, engine_ptr, is_written);
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_setEnSentenceMode(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
    enabled: jboolean,
) {
    Java_com_andyhui_cpime_NativeEngine_nativeSetEnSentenceMode(env, class, engine_ptr, enabled);
}

#[no_mangle]
pub extern "system" fn Java_com_andyhui_cpime_NativeEngine_clearContext(
    env: JNIEnv,
    class: JClass,
    engine_ptr: jlong,
) {
    Java_com_andyhui_cpime_NativeEngine_nativeClearContext(env, class, engine_ptr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::CandidateSource;

    #[test]
    fn test_serialize_candidates_json_contains_expected_fields() {
        let candidates = vec![
            Candidate {
                word: "今日".to_string(),
                matched_code: "gam1 jat6".to_string(),
                source: CandidateSource::Jyutping,
                score: 15000,
                consumed_chars: 6,
            },
            Candidate {
                word: "太陽".to_string(),
                matched_code: "sun".to_string(),
                source: CandidateSource::English,
                score: 12000,
                consumed_chars: 3,
            },
        ];

        let json_str = serialize_candidates_json(candidates);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("JSON 解析應成功");
        let arr = parsed.as_array().expect("應為 JSON 陣列");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["word"], "今日");
        assert_eq!(arr[0]["consumed_chars"], 6);
        assert_eq!(arr[0]["consumedChars"], 6);
        assert_eq!(arr[0]["source"], "粵拼");
        assert_eq!(arr[1]["word"], "太陽");
        assert_eq!(arr[1]["source"], "英文反查");
    }

    #[test]
    fn test_init_engine_impl_and_cleanup() {
        let db_path = CpimeEngine::default_db_path();
        let db_str = db_path.to_string_lossy();
        let ptr = init_engine_impl(&db_str, "");
        assert_ne!(ptr, 0, "使用預設資料庫初始化引擎指標不應為 0");

        unsafe {
            let engine = engine_mut(ptr).expect("應能還原引擎指標");
            let cands = engine.lookup_with_limit("sun", 5);
            let json = serialize_candidates_json(cands);
            assert!(json.contains("\"word\":\"新\""));
            assert!(json.contains("\"consumed_chars\":3"));
            drop(Box::from_raw(ptr as *mut CpimeEngine));
        }

        assert_eq!(init_engine_impl("", ""), 0);
        assert_eq!(init_engine_impl("/non/existent/cpime_dict.db", ""), 0);
    }
}
