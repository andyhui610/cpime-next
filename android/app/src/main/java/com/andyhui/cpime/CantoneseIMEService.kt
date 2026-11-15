package com.andyhui.cpime

import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import java.util.concurrent.Executors

/**
 * CPIME-Next Android 輸入法服務
 *
 * 實作 `LifecycleOwner`、`ViewModelStoreOwner` 與 `SavedStateRegistryOwner`，
 * 並綁定至 IME `decorView` 與 `ComposeView`，確保 Jetpack Compose 軟鍵盤穩定運行。
 */
class CantoneseIMEService :
    InputMethodService(),
    LifecycleOwner,
    ViewModelStoreOwner,
    SavedStateRegistryOwner {

    private val lifecycleRegistry = LifecycleRegistry(this)
    private val store = ViewModelStore()
    private val savedStateRegistryController = SavedStateRegistryController.create(this)

    override val lifecycle: Lifecycle
        get() = lifecycleRegistry

    override val viewModelStore: ViewModelStore
        get() = store

    override val savedStateRegistry: SavedStateRegistry
        get() = savedStateRegistryController.savedStateRegistry

    private val mainHandler = Handler(Looper.getMainLooper())
    private val initExecutor = Executors.newSingleThreadExecutor()

    @Volatile
    private var nativeEngine: NativeEngine? = null

    private var inputBuffer by mutableStateOf("")
    private var segmentedPinyin by mutableStateOf("")
    private var candidates by mutableStateOf<List<CandidateItem>>(emptyList())
    private var predictions by mutableStateOf<List<CandidateItem>>(emptyList())
    private var isCantoneseMode by mutableStateOf(true)
    private var isWrittenMode by mutableStateOf(false)
    private var isEngineReady by mutableStateOf(false)

    override fun onCreate() {
        super.onCreate()
        savedStateRegistryController.performRestore(null)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_CREATE)

        // 於背景執行緒複製資產資料庫並初始化 Rust 核心引擎，避免阻塞主執行緒
        initExecutor.execute {
            val createdEngine = try {
                val paths = AssetHelper.ensureDatabasesReady(applicationContext)
                NativeEngine.create(paths.sysDbPath, paths.userDbPath)
            } catch (_: Throwable) {
                null
            }

            mainHandler.post {
                if (lifecycleRegistry.currentState == Lifecycle.State.DESTROYED) {
                    createdEngine?.close()
                    return@post
                }
                nativeEngine = createdEngine
                isEngineReady = createdEngine != null
                createdEngine?.setOutputMode(isWrittenMode)
                if (inputBuffer.isNotEmpty()) {
                    refreshCompositionAndCandidates()
                }
            }
        }
    }

    override fun onCreateInputView(): View {
        val composeView = ComposeView(this).apply {
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)
            setViewTreeLifecycleOwner(this@CantoneseIMEService)
            setViewTreeViewModelStoreOwner(this@CantoneseIMEService)
            setViewTreeSavedStateRegistryOwner(this@CantoneseIMEService)
            setContent {
                KeyboardScreen(
                    inputBuffer = inputBuffer,
                    segmentedPinyin = segmentedPinyin,
                    candidates = candidates,
                    predictions = predictions,
                    isCantoneseMode = isCantoneseMode,
                    isWrittenMode = isWrittenMode,
                    isEngineReady = isEngineReady,
                    onLetterClick = ::handleLetterInput,
                    onBackspace = ::handleBackspace,
                    onSpace = ::handleSpace,
                    onEnter = ::handleEnter,
                    onCandidateClick = ::handleCandidateClick,
                    onPunctuationClick = ::handlePunctuationInput,
                    onToggleLanguage = ::handleToggleLanguage,
                    onToggleOutputMode = ::handleToggleOutputMode,
                    onClearBuffer = ::clearCurrentBuffer,
                )
            }
        }

        window?.window?.decorView?.let { decor ->
            decor.setViewTreeLifecycleOwner(this)
            decor.setViewTreeViewModelStoreOwner(this)
            decor.setViewTreeSavedStateRegistryOwner(this)
        }

        return composeView
    }

    override fun onEvaluateFullscreenMode(): Boolean = false

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        if (!restarting) {
            resetSessionState()
        }
    }

    override fun onFinishInput() {
        super.onFinishInput()
        resetSessionState()
    }

    override fun onWindowShown() {
        super.onWindowShown()
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_START)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)
    }

    override fun onWindowHidden() {
        super.onWindowHidden()
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_PAUSE)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_STOP)
    }

    override fun onDestroy() {
        initExecutor.shutdownNow()
        nativeEngine?.close()
        nativeEngine = null
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_DESTROY)
        store.clear()
        super.onDestroy()
    }

    private fun resetSessionState() {
        inputBuffer = ""
        segmentedPinyin = ""
        candidates = emptyList()
        predictions = emptyList()
        nativeEngine?.clearContext()
    }

    private fun handleLetterInput(letter: String) {
        val ic = currentInputConnection ?: return
        if (!isCantoneseMode) {
            ic.commitText(letter, 1)
            return
        }
        inputBuffer += letter.lowercase()
        refreshCompositionAndCandidates()
    }

    private fun handleBackspace() {
        val ic = currentInputConnection ?: return
        if (isCantoneseMode && inputBuffer.isNotEmpty()) {
            inputBuffer = inputBuffer.dropLast(1)
            if (inputBuffer.isEmpty()) {
                segmentedPinyin = ""
                candidates = emptyList()
                ic.beginBatchEdit()
                ic.setComposingText("", 1)
                ic.finishComposingText()
                ic.endBatchEdit()
                predictions = nativeEngine?.getPredictions(16).orEmpty()
            } else {
                refreshCompositionAndCandidates()
            }
            return
        }

        // 無拼音緩衝時發送標準退格鍵事件，安全刪除目標 App 文字（含 UTF-16 Surrogate Pairs 如「𨋢」）
        sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL)
        nativeEngine?.clearContext()
        predictions = emptyList()
    }

    private fun handleSpace() {
        val ic = currentInputConnection ?: return
        if (isCantoneseMode && inputBuffer.isNotEmpty()) {
            val first = candidates.firstOrNull()
            if (first != null) {
                handleCandidateClick(0, first)
            } else {
                // 若無候選詞，直接送出原始拼音字串並補空白
                ic.beginBatchEdit()
                ic.commitText("$inputBuffer ", 1)
                ic.endBatchEdit()
                inputBuffer = ""
                segmentedPinyin = ""
                candidates = emptyList()
                nativeEngine?.clearContext()
            }
            return
        }
        ic.commitText(" ", 1)
        nativeEngine?.clearContext()
        predictions = emptyList()
    }

    private fun handleEnter() {
        val ic = currentInputConnection ?: return
        if (isCantoneseMode && inputBuffer.isNotEmpty()) {
            // 規範：Enter 鍵將目前原始英文字串原封不動上屏
            val raw = inputBuffer
            ic.commitText(raw, 1)
            inputBuffer = ""
            segmentedPinyin = ""
            candidates = emptyList()
            nativeEngine?.clearContext()
            predictions = emptyList()
            return
        }

        nativeEngine?.clearContext()
        predictions = emptyList()

        val editorInfo = currentInputEditorInfo
        val action = editorInfo?.imeOptions?.and(EditorInfo.IME_MASK_ACTION)
            ?: EditorInfo.IME_ACTION_NONE
        val isMultiLine = (editorInfo?.inputType ?: 0) and
            android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE != 0

        if (!isMultiLine && action != EditorInfo.IME_ACTION_NONE &&
            action != EditorInfo.IME_ACTION_UNSPECIFIED
        ) {
            ic.performEditorAction(action)
        } else {
            sendDownUpKeyEvents(KeyEvent.KEYCODE_ENTER)
        }
    }

    private fun handlePunctuationInput(punctuation: String) {
        val ic = currentInputConnection ?: return
        ic.beginBatchEdit()
        if (isCantoneseMode && inputBuffer.isNotEmpty()) {
            val first = candidates.firstOrNull()
            val textToCommit = if (first != null) {
                nativeEngine?.commitCandidate(inputBuffer, 0)?.ifEmpty { first.word } ?: first.word
            } else {
                inputBuffer
            }
            ic.commitText(textToCommit, 1)
            inputBuffer = ""
            segmentedPinyin = ""
            candidates = emptyList()
        }
        ic.commitText(punctuation, 1)
        ic.endBatchEdit()
        nativeEngine?.clearContext()
        predictions = emptyList()
    }

    private fun handleCandidateClick(index: Int, item: CandidateItem) {
        val ic = currentInputConnection ?: return
        val engine = nativeEngine

        if (inputBuffer.isEmpty()) {
            // Bigram 語境聯想詞直接上屏
            ic.commitText(item.word, 1)
            predictions = engine?.getPredictions(16).orEmpty()
            return
        }

        val currentBuffer = inputBuffer
        val committedWord = engine?.commitCandidate(currentBuffer, index)
            ?.takeIf { it.isNotEmpty() }
            ?: item.word

        val consumed = item.consumedChars.coerceIn(0, currentBuffer.length)

        ic.beginBatchEdit()
        ic.commitText(committedWord, 1)
        if (consumed in 1 until currentBuffer.length) {
            // 剝洋蔥式部分選字：僅切除前綴並重新解碼剩餘拼音
            inputBuffer = currentBuffer.substring(consumed)
            val remainingFormatted =
                engine?.formatPinyin(inputBuffer)?.ifEmpty { inputBuffer } ?: inputBuffer
            segmentedPinyin = remainingFormatted
            candidates = engine?.getCandidates(inputBuffer, 24).orEmpty()
            predictions = emptyList()
            ic.setComposingText(remainingFormatted, 1)
        } else {
            inputBuffer = ""
            segmentedPinyin = ""
            candidates = emptyList()
            predictions = engine?.getPredictions(16).orEmpty()
        }
        ic.endBatchEdit()
    }

    private fun handleToggleLanguage() {
        val ic = currentInputConnection
        if (isCantoneseMode && inputBuffer.isNotEmpty()) {
            // 切換至英文直通模式時，保留已敲下的原始字母並上屏
            ic?.commitText(inputBuffer, 1)
            inputBuffer = ""
            segmentedPinyin = ""
            candidates = emptyList()
        }
        isCantoneseMode = !isCantoneseMode
        nativeEngine?.clearContext()
        predictions = emptyList()
    }

    private fun handleToggleOutputMode() {
        isWrittenMode = !isWrittenMode
        nativeEngine?.setOutputMode(isWrittenMode)
        if (inputBuffer.isNotEmpty()) {
            refreshCompositionAndCandidates()
        }
    }

    private fun clearCurrentBuffer() {
        inputBuffer = ""
        segmentedPinyin = ""
        candidates = emptyList()
        currentInputConnection?.let { ic ->
            ic.beginBatchEdit()
            ic.setComposingText("", 1)
            ic.finishComposingText()
            ic.endBatchEdit()
        }
        nativeEngine?.clearContext()
        predictions = emptyList()
    }

    private fun refreshCompositionAndCandidates() {
        val engine = nativeEngine
        if (inputBuffer.isEmpty()) {
            segmentedPinyin = ""
            candidates = emptyList()
            currentInputConnection?.finishComposingText()
            predictions = engine?.getPredictions(16).orEmpty()
            return
        }

        val formatted = engine?.formatPinyin(inputBuffer)?.ifEmpty { inputBuffer } ?: inputBuffer
        segmentedPinyin = formatted
        candidates = engine?.getCandidates(inputBuffer, 24).orEmpty()
        predictions = emptyList()
        currentInputConnection?.setComposingText(formatted, 1)
    }
}
