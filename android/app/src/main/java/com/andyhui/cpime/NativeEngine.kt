package com.andyhui.cpime

import org.json.JSONArray

/**
 * 候選詞資料模型（對應 Rust `JniCandidateDto`）
 */
data class CandidateItem(
    val word: String,
    val matchedCode: String,
    val consumedChars: Int,
    val source: String,
    val score: Int,
)

/**
 * `cpime-core` Rust 核心引擎之 Kotlin JNI 封裝層
 */
class NativeEngine private constructor(
    private var enginePtr: Long,
) {
    val isReady: Boolean
        get() = enginePtr != 0L

    @Synchronized
    fun formatPinyin(input: String): String {
        if (enginePtr == 0L || input.isEmpty()) return input
        return nativeFormatPinyin(enginePtr, input)
    }

    @Synchronized
    fun getCandidates(input: String, limit: Int = 24): List<CandidateItem> {
        if (enginePtr == 0L || input.isEmpty() || limit <= 0) return emptyList()
        val rawJson = nativeGetCandidatesJson(enginePtr, input, limit)
        return parseCandidatesJson(rawJson)
    }

    @Synchronized
    fun commitCandidate(input: String, index: Int): String {
        if (enginePtr == 0L || input.isEmpty() || index < 0) return ""
        return nativeCommitCandidate(enginePtr, input, index)
    }

    @Synchronized
    fun getPredictions(limit: Int = 15): List<CandidateItem> {
        if (enginePtr == 0L || limit <= 0) return emptyList()
        val rawJson = nativeGetPredictionsJson(enginePtr, limit)
        return parseCandidatesJson(rawJson)
    }

    @Synchronized
    fun setOutputMode(isWritten: Boolean) {
        if (enginePtr == 0L) return
        nativeSetOutputMode(enginePtr, isWritten)
    }

    @Synchronized
    fun setEnSentenceMode(enabled: Boolean) {
        if (enginePtr == 0L) return
        nativeSetEnSentenceMode(enginePtr, enabled)
    }

    @Synchronized
    fun clearContext() {
        if (enginePtr == 0L) return
        nativeClearContext(enginePtr)
    }

    @Synchronized
    fun close() {
        if (enginePtr != 0L) {
            nativeDestroy(enginePtr)
            enginePtr = 0L
        }
    }

    private fun parseCandidatesJson(rawJson: String): List<CandidateItem> {
        if (rawJson.isBlank() || rawJson == "[]") return emptyList()
        return try {
            val array = JSONArray(rawJson)
            buildList(array.length()) {
                for (i in 0 until array.length()) {
                    val obj = array.optJSONObject(i) ?: continue
                    val word = obj.optString("word", "")
                    if (word.isEmpty()) continue
                    val matchedCode = if (obj.has("matchedCode")) {
                        obj.optString("matchedCode", "")
                    } else {
                        obj.optString("matched_code", "")
                    }
                    val consumedChars = if (obj.has("consumedChars")) {
                        obj.optInt("consumedChars", 0)
                    } else {
                        obj.optInt("consumed_chars", 0)
                    }
                    val source = obj.optString("source", "")
                    val score = obj.optInt("score", 0)
                    add(
                        CandidateItem(
                            word = word,
                            matchedCode = matchedCode,
                            consumedChars = consumedChars,
                            source = source,
                            score = score,
                        ),
                    )
                }
            }
        } catch (_: Exception) {
            emptyList()
        }
    }

    companion object {
        init {
            System.loadLibrary("cpime_core")
        }

        fun create(sysDbPath: String, userDbPath: String): NativeEngine? {
            val ptr = nativeInit(sysDbPath, userDbPath)
            return if (ptr != 0L) NativeEngine(ptr) else null
        }

        @JvmStatic
        private external fun nativeInit(sysDbPath: String, userDbPath: String): Long

        @JvmStatic
        private external fun nativeDestroy(enginePtr: Long)

        @JvmStatic
        private external fun nativeFormatPinyin(enginePtr: Long, input: String): String

        @JvmStatic
        private external fun nativeGetCandidatesJson(
            enginePtr: Long,
            input: String,
            limit: Int,
        ): String

        @JvmStatic
        private external fun nativeCommitCandidate(
            enginePtr: Long,
            input: String,
            index: Int,
        ): String

        @JvmStatic
        private external fun nativeGetPredictionsJson(enginePtr: Long, limit: Int): String

        @JvmStatic
        private external fun nativeSetOutputMode(enginePtr: Long, isWritten: Boolean)

        @JvmStatic
        private external fun nativeSetEnSentenceMode(enginePtr: Long, enabled: Boolean)

        @JvmStatic
        private external fun nativeClearContext(enginePtr: Long)
    }
}
