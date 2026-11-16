package com.andyhui.cpime

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed as gridItemsIndexed
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.changedToUpIgnoreConsumed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs

private object ImeColors {
    val KeyboardBg = Color(0xFF14141B)
    val BarBg = Color(0xFF191923)
    val StripBg = Color(0xFF161620)
    val KeyBg = Color(0xFF2B2B3B)
    val KeyPressedBg = Color(0xFF3E3E56)
    val FuncKeyBg = Color(0xFF1F1F2E)
    val KeyBorder = Color(0xFF343448)
    val PopupBg = Color(0xFF3A3A52)
    val PopupSubBg = Color(0xFF2563EB)
    val PopupBorder = Color(0xFF60A5FA)
    val TextPrimary = Color(0xFFF3F4F6)
    val TextSecondary = Color(0xFF9CA3AF)
    val SubLabelColor = Color(0xFF94A3B8)
    val PinyinAccent = Color(0xFF93C5FD)
    val ColloquialBlue = Color(0xFF2563EB)
    val WrittenGreen = Color(0xFF059669)
    val TransPurple = Color(0xFF7C3AED)
    val EnglishSlate = Color(0xFF4B5563)
    val CandidateHighlight = Color(0xFF2563EB)
    val CandidateHighlightBorder = Color(0xFF60A5FA)
    val CandidateChipBg = Color(0xFF232332)
    val TrackpadBg = Color(0xFF1E3A8A)
    val BadgeEnglish = Color(0xFFFBBF24)
    val BadgeSentence = Color(0xFF34D399)
    val BadgeCustom = Color(0xFFA78BFA)
}

private val QwertyRow1 = listOf("q", "w", "e", "r", "t", "y", "u", "i", "o", "p")
private val QwertyRow2 = listOf("a", "s", "d", "f", "g", "h", "j", "k", "l")
private val QwertyRow3 = listOf("z", "x", "c", "v", "b", "n", "m")

private val KeySubLabels = mapOf(
    "q" to "1",
    "w" to "2",
    "e" to "3",
    "r" to "4",
    "t" to "5",
    "y" to "6",
    "u" to "7",
    "i" to "8",
    "o" to "9",
    "p" to "0",
    "a" to "@",
    "s" to "—",
    "d" to "、",
    "f" to "；",
    "g" to "：",
    "h" to "（",
    "j" to "）",
    "k" to "「",
    "l" to "」",
    "z" to "「",
    "x" to "」",
    "c" to "，",
    "v" to "。",
    "b" to "…",
    "n" to "！",
    "m" to "？",
)

private val SymbolsRow1 = listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "0")
private val SymbolsRow2 = listOf("@", "#", "$", "%", "&", "-", "+", "(", ")")
private val SymbolsRow3 = listOf("*", "\"", "'", ":", ";", "/", "!")

private val CantonesePunctuations = listOf("，", "。", "？", "！", "「", "」", "、", "…")

private val KeyHeight = 52.dp
private val KeyCornerRadius = 8.dp
private val RowVerticalSpacing = 6.dp
private val KeyHorizontalSpacing = 5.dp

@Composable
fun KeyboardScreen(
    inputBuffer: String,
    segmentedPinyin: String,
    candidates: List<CandidateItem>,
    predictions: List<CandidateItem>,
    isCantoneseMode: Boolean,
    isWrittenMode: Boolean,
    isEnSentenceMode: Boolean,
    isEngineReady: Boolean,
    onLetterClick: (String) -> Unit,
    onBackspace: () -> Unit,
    onBackspaceSwipeLeft: () -> Unit,
    onSpace: () -> Unit,
    onMoveCursor: (Int) -> Unit,
    onEnter: () -> Unit,
    onCandidateClick: (Int, CandidateItem) -> Unit,
    onPunctuationClick: (String) -> Unit,
    onToggleLanguage: () -> Unit,
    onToggleOutputMode: () -> Unit,
    onToggleEnSentenceMode: () -> Unit,
    onClearBuffer: () -> Unit,
) {
    val view = LocalView.current
    var isShifted by remember { mutableStateOf(false) }
    var isSymbolsMode by remember { mutableStateOf(false) }
    var isCandidateGridExpanded by remember { mutableStateOf(false) }

    val triggerHaptic: () -> Unit = remember(view) {
        {
            view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
            Unit
        }
    }

    val displayedItems = if (inputBuffer.isNotEmpty()) candidates else predictions
    val isShowingPredictions = inputBuffer.isEmpty() && predictions.isNotEmpty()
    val isEnSentenceActive = isCantoneseMode && (
        isEnSentenceMode ||
            inputBuffer.trimStart().startsWith(";") ||
            inputBuffer.trimStart().startsWith("/")
        )

    LaunchedEffect(displayedItems.isEmpty()) {
        if (displayedItems.isEmpty()) {
            isCandidateGridExpanded = false
        }
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(ImeColors.KeyboardBg)
            .navigationBarsPadding()
            .padding(bottom = 6.dp),
    ) {
        // 1. 頂部拼音切分與狀態預覽列 (Composing Bar)
        ComposingBar(
            inputBuffer = inputBuffer,
            segmentedPinyin = segmentedPinyin,
            isCantoneseMode = isCantoneseMode,
            isWrittenMode = isWrittenMode,
            isEnSentenceMode = isEnSentenceMode,
            isEngineReady = isEngineReady,
            isShowingPredictions = isShowingPredictions,
            onClearBuffer = {
                triggerHaptic()
                onClearBuffer()
            },
        )

        // 2. 工具鈕、橫向捲動候選詞列與展開切換按鈕
        CandidateToolbarRow(
            candidates = displayedItems,
            isCantoneseMode = isCantoneseMode,
            isWrittenMode = isWrittenMode,
            isEnSentenceMode = isEnSentenceMode,
            isShowingPredictions = isShowingPredictions,
            hasActiveInput = inputBuffer.isNotEmpty(),
            isGridExpanded = isCandidateGridExpanded,
            onToggleGrid = {
                triggerHaptic()
                isCandidateGridExpanded = !isCandidateGridExpanded
            },
            onCandidateClick = { idx, item ->
                triggerHaptic()
                isCandidateGridExpanded = false
                onCandidateClick(idx, item)
            },
            onToggleOutputMode = {
                triggerHaptic()
                onToggleOutputMode()
            },
            onToggleEnSentenceMode = {
                triggerHaptic()
                onToggleEnSentenceMode()
            },
            onToggleLanguage = {
                triggerHaptic()
                onToggleLanguage()
            },
        )

        if (isCandidateGridExpanded && displayedItems.isNotEmpty()) {
            // 3A. 4 欄式大候選字網格面板
            CandidateGridPanel(
                candidates = displayedItems,
                isShowingPredictions = isShowingPredictions,
                hasInputBuffer = inputBuffer.isNotEmpty(),
                onCandidateClick = { idx, item ->
                    triggerHaptic()
                    isCandidateGridExpanded = false
                    onCandidateClick(idx, item)
                },
                onCollapse = {
                    triggerHaptic()
                    isCandidateGridExpanded = false
                },
                onBackspace = onBackspace,
                onBackspaceSwipeLeft = onBackspaceSwipeLeft,
                onClearBuffer = {
                    triggerHaptic()
                    isCandidateGridExpanded = false
                    onClearBuffer()
                },
            )
        } else {
            // 3B. 常用廣東話標點快選列
            PunctuationQuickStrip(
                punctuations = CantonesePunctuations,
                onPunctuationClick = { p ->
                    triggerHaptic()
                    onPunctuationClick(p)
                },
            )

            Spacer(modifier = Modifier.height(6.dp))

            // 4. 鍵盤主體按鍵區
            if (isSymbolsMode) {
                SymbolKeyboardBody(
                    hasInputBuffer = inputBuffer.isNotEmpty(),
                    onSymbolClick = { sym ->
                        onPunctuationClick(sym)
                    },
                    onBackspace = onBackspace,
                    onBackspaceSwipeLeft = onBackspaceSwipeLeft,
                    onSwitchToAlpha = {
                        isSymbolsMode = false
                    },
                    onSpace = onSpace,
                    onMoveCursor = onMoveCursor,
                    onEnter = onEnter,
                )
            } else {
                QwertyKeyboardBody(
                    isShifted = isShifted,
                    isCantoneseMode = isCantoneseMode,
                    isWrittenMode = isWrittenMode,
                    isEnSentenceActive = isEnSentenceActive,
                    hasInputBuffer = inputBuffer.isNotEmpty(),
                    onLetterClick = { ch ->
                        val output = if ((!isCantoneseMode || isEnSentenceActive) && isShifted) ch.uppercase() else ch
                        onLetterClick(output)
                        if ((!isCantoneseMode || isEnSentenceActive) && isShifted) {
                            isShifted = false
                        }
                    },
                    onSubSymbolClick = { sub ->
                        onPunctuationClick(sub)
                    },
                    onToggleShift = {
                        isShifted = !isShifted
                    },
                    onSwitchToSymbols = {
                        isSymbolsMode = true
                    },
                    onToggleLanguage = onToggleLanguage,
                    onPunctuationClick = onPunctuationClick,
                    onBackspace = onBackspace,
                    onBackspaceSwipeLeft = onBackspaceSwipeLeft,
                    onSpace = onSpace,
                    onMoveCursor = onMoveCursor,
                    onEnter = onEnter,
                )
            }
        }
    }
}

@Composable
private fun ComposingBar(
    inputBuffer: String,
    segmentedPinyin: String,
    isCantoneseMode: Boolean,
    isWrittenMode: Boolean,
    isEnSentenceMode: Boolean,
    isEngineReady: Boolean,
    isShowingPredictions: Boolean,
    onClearBuffer: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(ImeColors.BarBg)
            .padding(horizontal = 10.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (inputBuffer.isNotEmpty()) {
            Box(
                modifier = Modifier
                    .weight(1f)
                    .horizontalScroll(rememberScrollState()),
            ) {
                Text(
                    text = segmentedPinyin.ifEmpty { inputBuffer },
                    color = ImeColors.PinyinAccent,
                    fontSize = 15.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                )
            }
            Spacer(modifier = Modifier.width(8.dp))
            TactileChipButton(
                label = "✕ 清空",
                backgroundColor = ImeColors.KeyBg,
                textColor = ImeColors.TextSecondary,
                fontSize = 11.sp,
                horizontalPadding = 8.dp,
                verticalPadding = 3.dp,
                cornerRadius = 6.dp,
                onClick = onClearBuffer,
            )
        } else {
            val statusText = when {
                !isEngineReady -> "正在初始化 CPIME-Next 詞庫引擎..."
                !isCantoneseMode -> "English 直通模式 · 上滑首排輸數字 · 滑動空白鍵移游標"
                isEnSentenceMode -> "英譯粵整句模式 · 輸入英文片語即時組出廣東話整句"
                isShowingPredictions -> "語境聯想詞（Bigram）· 直接點選或輸入粵拼/英文"
                isWrittenMode -> "CPIME-Next 廣東話 · 書面語（上滑首排數字 · 空白鍵移游標）"
                else -> "CPIME-Next 廣東話 · 口語（上滑首排數字 · 空白鍵移游標）"
            }
            Text(
                text = statusText,
                color = ImeColors.TextSecondary,
                fontSize = 11.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun CandidateToolbarRow(
    candidates: List<CandidateItem>,
    isCantoneseMode: Boolean,
    isWrittenMode: Boolean,
    isEnSentenceMode: Boolean,
    isShowingPredictions: Boolean,
    hasActiveInput: Boolean,
    isGridExpanded: Boolean,
    onToggleGrid: () -> Unit,
    onCandidateClick: (Int, CandidateItem) -> Unit,
    onToggleOutputMode: () -> Unit,
    onToggleEnSentenceMode: () -> Unit,
    onToggleLanguage: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(48.dp)
            .background(ImeColors.BarBg)
            .padding(horizontal = 6.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // 固定置頂：[粵/書] 口語/書面語切換膠囊
        val modeBg = if (isWrittenMode) ImeColors.WrittenGreen else ImeColors.ColloquialBlue
        val modeLabel = if (isWrittenMode) "書" else "粵"
        TactileChipButton(
            label = modeLabel,
            backgroundColor = modeBg,
            textColor = Color.White,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold,
            horizontalPadding = 10.dp,
            verticalPadding = 7.dp,
            onClick = onToggleOutputMode,
        )

        Spacer(modifier = Modifier.width(5.dp))

        // 固定置頂：[譯] 英文整句直出廣東話模式切換鈕
        val transActive = isCantoneseMode && isEnSentenceMode
        val transBg = if (transActive) ImeColors.TransPurple else ImeColors.KeyBg
        val transBorder = if (transActive) ImeColors.TransPurple else ImeColors.KeyBorder
        TactileChipButton(
            label = "譯",
            backgroundColor = transBg,
            borderColor = transBorder,
            textColor = if (transActive) Color.White else ImeColors.TextPrimary,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold,
            horizontalPadding = 9.dp,
            verticalPadding = 7.dp,
            onClick = onToggleEnSentenceMode,
        )

        Spacer(modifier = Modifier.width(5.dp))

        // 固定置頂：[粵/英] 輸入語言切換鈕
        val langBg = if (isCantoneseMode) ImeColors.KeyBg else ImeColors.EnglishSlate
        val langLabel = if (isCantoneseMode) "粵/英" else "英"
        TactileChipButton(
            label = langLabel,
            backgroundColor = langBg,
            borderColor = ImeColors.KeyBorder,
            textColor = ImeColors.TextPrimary,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            horizontalPadding = 9.dp,
            verticalPadding = 7.dp,
            onClick = onToggleLanguage,
        )

        Spacer(modifier = Modifier.width(6.dp))

        // 橫向捲動候選詞區
        if (candidates.isEmpty()) {
            Box(
                modifier = Modifier.weight(1f),
                contentAlignment = Alignment.CenterStart,
            ) {
                Text(
                    text = if (hasActiveInput) "無符合候選詞（按 Enter 直出原文）" else "",
                    color = ImeColors.TextSecondary,
                    fontSize = 12.sp,
                )
            }
        } else {
            LazyRow(
                modifier = Modifier.weight(1f),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                contentPadding = PaddingValues(end = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                itemsIndexed(
                    items = candidates,
                    key = { index, item -> "${index}_${item.word}_${item.matchedCode}" },
                ) { index, item ->
                    val isPrimary = index == 0 && !isShowingPredictions
                    CandidateChip(
                        item = item,
                        isPrimary = isPrimary,
                        isPrediction = isShowingPredictions,
                        onClick = { onCandidateClick(index, item) },
                    )
                }
            }
        }

        // 候選詞超過 4 個（或已展開）時在最右端顯示固定置頂的展開切換按鈕 [⌄] / [⌃]
        if (candidates.size > 4 || isGridExpanded) {
            Spacer(modifier = Modifier.width(4.dp))
            TactileChipButton(
                label = if (isGridExpanded) "⌃" else "⌄",
                backgroundColor = if (isGridExpanded) ImeColors.CandidateHighlight else ImeColors.FuncKeyBg,
                borderColor = if (isGridExpanded) ImeColors.CandidateHighlightBorder else ImeColors.KeyBorder,
                textColor = Color.White,
                fontSize = 16.sp,
                fontWeight = FontWeight.Bold,
                horizontalPadding = 11.dp,
                verticalPadding = 5.dp,
                onClick = onToggleGrid,
            )
        }
    }
}

@Composable
private fun CandidateChip(
    item: CandidateItem,
    isPrimary: Boolean,
    isPrediction: Boolean,
    onClick: () -> Unit,
) {
    val bg = if (isPrimary) ImeColors.CandidateHighlight else ImeColors.CandidateChipBg
    val borderColor = if (isPrimary) ImeColors.CandidateHighlightBorder else ImeColors.KeyBorder
    val latestOnClick by rememberUpdatedState(onClick)
    var pressed by remember { mutableStateOf(false) }

    val hintText = when {
        isPrediction -> "聯想"
        item.source == "英文反查" -> item.matchedCode.ifEmpty { "英譯" }
        item.source == "整句連打" -> "整句"
        item.source == "個人詞庫" -> "自造"
        item.source == "懶音" -> "懶音"
        else -> ""
    }
    val hintColor = when {
        isPrimary -> Color(0xFFDBEAFE)
        item.source == "英文反查" -> ImeColors.BadgeEnglish
        item.source == "整句連打" -> ImeColors.BadgeSentence
        item.source == "個人詞庫" -> ImeColors.BadgeCustom
        else -> ImeColors.TextSecondary
    }

    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(KeyCornerRadius))
            .background(if (pressed) ImeColors.KeyPressedBg else bg)
            .border(1.dp, borderColor, RoundedCornerShape(KeyCornerRadius))
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    var normalUp = false
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull() ?: break
                        if (!change.pressed) {
                            normalUp = change.changedToUpIgnoreConsumed() && !change.isConsumed
                            break
                        }
                    }
                    pressed = false
                    if (normalUp) {
                        latestOnClick()
                    }
                }
            }
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = item.word,
            color = if (isPrimary) Color.White else ImeColors.TextPrimary,
            fontSize = 17.sp,
            fontWeight = if (isPrimary) FontWeight.Bold else FontWeight.Normal,
            maxLines = 1,
        )
        if (hintText.isNotEmpty()) {
            Spacer(modifier = Modifier.width(4.dp))
            Text(
                text = hintText,
                color = hintColor,
                fontSize = 10.sp,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun CandidateGridPanel(
    candidates: List<CandidateItem>,
    isShowingPredictions: Boolean,
    hasInputBuffer: Boolean,
    onCandidateClick: (Int, CandidateItem) -> Unit,
    onCollapse: () -> Unit,
    onBackspace: () -> Unit,
    onBackspaceSwipeLeft: () -> Unit,
    onClearBuffer: () -> Unit,
) {
    // 總高度對齊原本標點列 (34dp) + 間距 (6dp) + 4 排鍵盤 (52*4 + 6*3 = 226dp) = 266dp
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(266.dp)
            .background(ImeColors.KeyboardBg)
            .padding(horizontal = 6.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        LazyVerticalGrid(
            columns = GridCells.Fixed(4),
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight(),
            contentPadding = PaddingValues(vertical = 2.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            gridItemsIndexed(
                items = candidates,
                key = { index, item -> "grid_${index}_${item.word}_${item.matchedCode}" },
            ) { index, item ->
                val isPrimary = index == 0 && !isShowingPredictions
                CandidateGridCell(
                    item = item,
                    isPrimary = isPrimary,
                    isPrediction = isShowingPredictions,
                    onClick = { onCandidateClick(index, item) },
                )
            }
        }

        // 右側功能側欄：收合返回、智慧退格、一鍵清空
        Column(
            modifier = Modifier
                .width(58.dp)
                .fillMaxHeight(),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            TactilePanelActionButton(
                label = "⌃\n收合",
                backgroundColor = ImeColors.ColloquialBlue,
                textColor = Color.White,
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                onClick = onCollapse,
            )

            SmartBackspaceKey(
                height = KeyHeight,
                hasInputBuffer = hasInputBuffer,
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                onBackspace = onBackspace,
                onSwipeLeftClear = onBackspaceSwipeLeft,
            )

            TactilePanelActionButton(
                label = "清空",
                backgroundColor = ImeColors.FuncKeyBg,
                textColor = if (hasInputBuffer) ImeColors.TextPrimary else ImeColors.TextSecondary,
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                onClick = onClearBuffer,
            )
        }
    }
}

@Composable
private fun CandidateGridCell(
    item: CandidateItem,
    isPrimary: Boolean,
    isPrediction: Boolean,
    onClick: () -> Unit,
) {
    val bg = if (isPrimary) ImeColors.CandidateHighlight else ImeColors.CandidateChipBg
    val borderColor = if (isPrimary) ImeColors.CandidateHighlightBorder else ImeColors.KeyBorder
    val latestOnClick by rememberUpdatedState(onClick)
    var pressed by remember { mutableStateOf(false) }

    val wordFontSize = when {
        item.word.length >= 6 -> 12.sp
        item.word.length >= 4 -> 14.sp
        item.word.length >= 2 -> 16.sp
        else -> 19.sp
    }

    val badge = when {
        isPrediction -> "聯想"
        item.source == "英文反查" -> "英譯"
        item.source == "整句連打" -> "整句"
        item.source == "個人詞庫" -> "自造"
        else -> ""
    }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(52.dp)
            .clip(RoundedCornerShape(KeyCornerRadius))
            .background(if (pressed) ImeColors.KeyPressedBg else bg)
            .border(1.dp, borderColor, RoundedCornerShape(KeyCornerRadius))
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    var normalUp = false
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull() ?: break
                        if (!change.pressed) {
                            normalUp = change.changedToUpIgnoreConsumed() && !change.isConsumed
                            break
                        }
                    }
                    pressed = false
                    if (normalUp) {
                        latestOnClick()
                    }
                }
            }
            .padding(horizontal = 4.dp, vertical = 4.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Text(
                text = item.word,
                color = if (isPrimary) Color.White else ImeColors.TextPrimary,
                fontSize = wordFontSize,
                fontWeight = if (isPrimary) FontWeight.Bold else FontWeight.Medium,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (badge.isNotEmpty()) {
                Text(
                    text = badge,
                    color = if (isPrimary) Color(0xFFDBEAFE) else ImeColors.SubLabelColor,
                    fontSize = 9.sp,
                    maxLines = 1,
                )
            }
        }
    }
}

@Composable
private fun PunctuationQuickStrip(
    punctuations: List<String>,
    onPunctuationClick: (String) -> Unit,
) {
    val view = LocalView.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(ImeColors.StripBg)
            .padding(horizontal = 6.dp, vertical = 3.dp),
        horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
    ) {
        punctuations.forEach { p ->
            var pressed by remember { mutableStateOf(false) }
            val latestOnClick by rememberUpdatedState { onPunctuationClick(p) }
            Box(
                modifier = Modifier
                    .weight(1f)
                    .height(28.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .background(if (pressed) ImeColors.KeyPressedBg else ImeColors.FuncKeyBg)
                    .pointerInput(Unit) {
                        awaitEachGesture {
                            awaitFirstDown(requireUnconsumed = false)
                            pressed = true
                            view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                            var normalUp = false
                            while (true) {
                                val event = awaitPointerEvent()
                                val change = event.changes.firstOrNull() ?: break
                                if (!change.pressed) {
                                    normalUp = change.changedToUpIgnoreConsumed() && !change.isConsumed
                                    break
                                }
                            }
                            pressed = false
                            if (normalUp) {
                                latestOnClick()
                            }
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = p,
                    color = ImeColors.TextPrimary,
                    fontSize = 14.sp,
                )
            }
        }
    }
}

@Composable
private fun QwertyKeyboardBody(
    isShifted: Boolean,
    isCantoneseMode: Boolean,
    isWrittenMode: Boolean,
    isEnSentenceActive: Boolean,
    hasInputBuffer: Boolean,
    onLetterClick: (String) -> Unit,
    onSubSymbolClick: (String) -> Unit,
    onToggleShift: () -> Unit,
    onSwitchToSymbols: () -> Unit,
    onToggleLanguage: () -> Unit,
    onPunctuationClick: (String) -> Unit,
    onBackspace: () -> Unit,
    onBackspaceSwipeLeft: () -> Unit,
    onSpace: () -> Unit,
    onMoveCursor: (Int) -> Unit,
    onEnter: () -> Unit,
) {
    val useUpperLabels = (!isCantoneseMode || isEnSentenceActive) && isShifted
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 5.dp),
        verticalArrangement = Arrangement.spacedBy(RowVerticalSpacing),
    ) {
        // 第 1 排：Q W E R T Y U I O P（副標記 1-0）
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            QwertyRow1.forEach { letter ->
                val label = if (useUpperLabels) letter.uppercase() else letter
                val sub = KeySubLabels[letter]
                ErgonomicKey(
                    label = label,
                    subLabel = sub,
                    height = KeyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onLetterClick(letter) },
                    onSubClick = { subChar -> onSubSymbolClick(subChar) },
                )
            }
        }

        // 第 2 排：A S D F G H J K L（左右留白並附標點副標記）
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            QwertyRow2.forEach { letter ->
                val label = if (useUpperLabels) letter.uppercase() else letter
                val sub = KeySubLabels[letter]
                ErgonomicKey(
                    label = label,
                    subLabel = sub,
                    height = KeyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onLetterClick(letter) },
                    onSubClick = { subChar -> onSubSymbolClick(subChar) },
                )
            }
        }

        // 第 3 排：Shift / Z X C V B N M / SmartBackspace
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            val shiftLabel = if (useUpperLabels) "⬆" else "⇧"
            ErgonomicKey(
                label = shiftLabel,
                height = KeyHeight,
                backgroundColor = if (useUpperLabels) ImeColors.ColloquialBlue else ImeColors.FuncKeyBg,
                showPopup = false,
                modifier = Modifier.weight(1.35f),
                onClick = onToggleShift,
            )

            QwertyRow3.forEach { letter ->
                val label = if (useUpperLabels) letter.uppercase() else letter
                val sub = KeySubLabels[letter]
                ErgonomicKey(
                    label = label,
                    subLabel = sub,
                    height = KeyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onLetterClick(letter) },
                    onSubClick = { subChar -> onSubSymbolClick(subChar) },
                )
            }

            SmartBackspaceKey(
                height = KeyHeight,
                hasInputBuffer = hasInputBuffer,
                modifier = Modifier.weight(1.35f),
                onBackspace = onBackspace,
                onSwipeLeftClear = onBackspaceSwipeLeft,
            )
        }

        // 第 4 排底部功能列：123 | 粵/英 | ， | 空白鍵(含游標觸控板) | 。 | Enter
        val spaceLabel = when {
            !isCantoneseMode -> "CPIME English"
            isEnSentenceActive -> "空格 (連按兩下上屏)"
            hasInputBuffer -> "選取首選詞"
            isWrittenMode -> "CPIME 廣東話 (書面語)"
            else -> "CPIME 廣東話 (口語)"
        }
        val enterLabel = when {
            isEnSentenceActive && hasInputBuffer -> "上屏"
            isCantoneseMode && hasInputBuffer -> "原文"
            else -> "↵"
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            ErgonomicKey(
                label = "123",
                fontSize = 14.sp,
                height = KeyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                showPopup = false,
                modifier = Modifier.weight(1.3f),
                onClick = onSwitchToSymbols,
            )
            ErgonomicKey(
                label = if (isCantoneseMode) "粵" else "英",
                fontSize = 15.sp,
                height = KeyHeight,
                backgroundColor = if (isCantoneseMode) ImeColors.ColloquialBlue else ImeColors.FuncKeyBg,
                showPopup = false,
                modifier = Modifier.weight(1.1f),
                onClick = onToggleLanguage,
            )
            ErgonomicKey(
                label = if (isCantoneseMode) "，" else ",",
                fontSize = 18.sp,
                height = KeyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(0.95f),
                onClick = { onPunctuationClick(if (isCantoneseMode) "，" else ",") },
            )
            SpacebarTrackpadKey(
                label = spaceLabel,
                height = KeyHeight,
                modifier = Modifier.weight(3.8f),
                onSpace = onSpace,
                onMoveCursor = onMoveCursor,
            )
            ErgonomicKey(
                label = if (isCantoneseMode) "。" else ".",
                fontSize = 18.sp,
                height = KeyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(0.95f),
                onClick = { onPunctuationClick(if (isCantoneseMode) "。" else ".") },
            )
            ErgonomicKey(
                label = enterLabel,
                fontSize = if (enterLabel == "原文" || enterLabel == "上屏") 13.sp else 18.sp,
                height = KeyHeight,
                backgroundColor = if (isEnSentenceActive) ImeColors.TransPurple else ImeColors.ColloquialBlue,
                showPopup = false,
                modifier = Modifier.weight(1.5f),
                onClick = onEnter,
            )
        }
    }
}

@Composable
private fun SymbolKeyboardBody(
    hasInputBuffer: Boolean,
    onSymbolClick: (String) -> Unit,
    onBackspace: () -> Unit,
    onBackspaceSwipeLeft: () -> Unit,
    onSwitchToAlpha: () -> Unit,
    onSpace: () -> Unit,
    onMoveCursor: (Int) -> Unit,
    onEnter: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 5.dp),
        verticalArrangement = Arrangement.spacedBy(RowVerticalSpacing),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            SymbolsRow1.forEach { sym ->
                ErgonomicKey(
                    label = sym,
                    height = KeyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onSymbolClick(sym) },
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            SymbolsRow2.forEach { sym ->
                ErgonomicKey(
                    label = sym,
                    height = KeyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onSymbolClick(sym) },
                )
            }
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            ErgonomicKey(
                label = "：",
                height = KeyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1.3f),
                onClick = { onSymbolClick("：") },
            )
            SymbolsRow3.forEach { sym ->
                ErgonomicKey(
                    label = sym,
                    height = KeyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onSymbolClick(sym) },
                )
            }
            SmartBackspaceKey(
                height = KeyHeight,
                hasInputBuffer = hasInputBuffer,
                modifier = Modifier.weight(1.3f),
                onBackspace = onBackspace,
                onSwipeLeftClear = onBackspaceSwipeLeft,
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(KeyHorizontalSpacing),
        ) {
            ErgonomicKey(
                label = "ABC",
                fontSize = 14.sp,
                height = KeyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                showPopup = false,
                modifier = Modifier.weight(1.6f),
                onClick = onSwitchToAlpha,
            )
            ErgonomicKey(
                label = "（",
                height = KeyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1f),
                onClick = { onSymbolClick("（") },
            )
            SpacebarTrackpadKey(
                label = "空白（滑動移動游標）",
                height = KeyHeight,
                modifier = Modifier.weight(4f),
                onSpace = onSpace,
                onMoveCursor = onMoveCursor,
            )
            ErgonomicKey(
                label = "）",
                height = KeyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1f),
                onClick = { onSymbolClick("）") },
            )
            ErgonomicKey(
                label = "↵",
                fontSize = 18.sp,
                height = KeyHeight,
                backgroundColor = ImeColors.ColloquialBlue,
                showPopup = false,
                modifier = Modifier.weight(1.6f),
                onClick = onEnter,
            )
        }
    }
}

/**
 * 人體工學即時觸控按鍵：
 * - Down 瞬間觸發零延遲震動回饋 (`KEYBOARD_TAP`) 與浮動放大預覽氣泡
 * - 支援上滑 (`dragAmount.y < -25f`) 與長按 (`280ms`) 直接輸出右上角數字或廣東話標點副標記
 */
@Composable
private fun ErgonomicKey(
    label: String,
    modifier: Modifier = Modifier,
    subLabel: String? = null,
    height: Dp = KeyHeight,
    fontSize: TextUnit = 19.sp,
    backgroundColor: Color = ImeColors.KeyBg,
    textColor: Color = ImeColors.TextPrimary,
    showPopup: Boolean = true,
    onClick: () -> Unit,
    onSubClick: ((String) -> Unit)? = null,
) {
    val view = LocalView.current
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()

    var pressed by remember { mutableStateOf(false) }
    var isFlickMode by remember { mutableStateOf(false) }
    var isLongPressed by remember { mutableStateOf(false) }

    val latestSubLabel by rememberUpdatedState(subLabel)
    val latestOnClick by rememberUpdatedState(onClick)
    val latestOnSubClick by rememberUpdatedState(onSubClick)

    val popupOffsetY = remember(density) { with(density) { 64.dp.roundToPx() } }

    Box(
        modifier = modifier
            .height(height)
            .widthIn(min = 24.dp)
            .clip(RoundedCornerShape(KeyCornerRadius))
            .background(if (pressed) ImeColors.KeyPressedBg else backgroundColor)
            .border(
                width = 0.5.dp,
                color = if (pressed) ImeColors.PopupBorder.copy(alpha = 0.45f) else ImeColors.KeyBorder.copy(alpha = 0.6f),
                shape = RoundedCornerShape(KeyCornerRadius),
            )
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    isFlickMode = false
                    isLongPressed = false
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)

                    val activeSub = latestSubLabel
                    val longPressJob = if (activeSub != null && latestOnSubClick != null) {
                        scope.launch {
                            delay(280L)
                            if (pressed && !isFlickMode) {
                                isLongPressed = true
                                view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                            }
                        }
                    } else {
                        null
                    }

                    var normalUp = false
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id }
                            ?: event.changes.firstOrNull()
                            ?: break

                        if (!change.pressed) {
                            normalUp = change.changedToUpIgnoreConsumed() && !change.isConsumed
                            break
                        }

                        if (activeSub != null && latestOnSubClick != null) {
                            val totalDy = change.position.y - down.position.y
                            val nowFlick = totalDy < -25f
                            if (nowFlick != isFlickMode) {
                                isFlickMode = nowFlick
                                if (nowFlick) {
                                    longPressJob?.cancel()
                                    view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                }
                            }
                        }
                    }

                    longPressJob?.cancel()
                    val shouldEmitSub = (isFlickMode || isLongPressed) &&
                        activeSub != null &&
                        latestOnSubClick != null

                    pressed = false
                    isFlickMode = false
                    isLongPressed = false

                    if (normalUp) {
                        if (shouldEmitSub && activeSub != null) {
                            latestOnSubClick?.invoke(activeSub)
                        } else {
                            latestOnClick()
                        }
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        if (!subLabel.isNullOrEmpty()) {
            Text(
                text = subLabel,
                color = ImeColors.SubLabelColor,
                fontSize = 10.sp,
                fontWeight = FontWeight.Medium,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(top = 2.dp, end = 4.dp),
                maxLines = 1,
            )
        }

        Text(
            text = label,
            color = textColor,
            fontSize = fontSize,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
        )

        if (pressed && showPopup) {
            val showingSub = (isFlickMode || isLongPressed) && !subLabel.isNullOrEmpty()
            val previewText = if (showingSub && subLabel != null) subLabel else label
            Popup(
                alignment = Alignment.TopCenter,
                offset = IntOffset(0, -popupOffsetY),
                properties = PopupProperties(
                    focusable = false,
                    clippingEnabled = false,
                    dismissOnBackPress = false,
                    dismissOnClickOutside = false,
                ),
            ) {
                Box(
                    modifier = Modifier
                        .widthIn(min = 52.dp)
                        .height(58.dp)
                        .shadow(10.dp, RoundedCornerShape(10.dp))
                        .clip(RoundedCornerShape(10.dp))
                        .background(if (showingSub) ImeColors.PopupSubBg else ImeColors.PopupBg)
                        .border(
                            width = 1.5.dp,
                            color = if (showingSub) Color(0xFF93C5FD) else ImeColors.PopupBorder.copy(alpha = 0.65f),
                            shape = RoundedCornerShape(10.dp),
                        )
                        .padding(horizontal = 12.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = previewText,
                        color = Color.White,
                        fontSize = 26.sp,
                        fontWeight = FontWeight.Bold,
                        maxLines = 1,
                    )
                }
            }
        }
    }
}

/**
 * 空白鍵兼游標觸控板：
 * - 點擊：選取首選詞 / 輸入空格
 * - 水平滑動：進入「游標控制模式」（顯示 `◀ 滑動移動游標 ▶`），每滑動 45px 觸發微震動並移動游標
 */
@Composable
private fun SpacebarTrackpadKey(
    label: String,
    height: Dp,
    modifier: Modifier = Modifier,
    onSpace: () -> Unit,
    onMoveCursor: (Int) -> Unit,
) {
    val view = LocalView.current
    var pressed by remember { mutableStateOf(false) }
    var isTrackpadMode by remember { mutableStateOf(false) }

    val latestOnSpace by rememberUpdatedState(onSpace)
    val latestOnMoveCursor by rememberUpdatedState(onMoveCursor)

    Box(
        modifier = modifier
            .height(height)
            .clip(RoundedCornerShape(KeyCornerRadius))
            .background(
                when {
                    isTrackpadMode -> ImeColors.TrackpadBg
                    pressed -> ImeColors.KeyPressedBg
                    else -> ImeColors.KeyBg
                },
            )
            .border(
                width = if (isTrackpadMode) 1.dp else 0.5.dp,
                color = if (isTrackpadMode) ImeColors.PopupBorder else ImeColors.KeyBorder.copy(alpha = 0.6f),
                shape = RoundedCornerShape(KeyCornerRadius),
            )
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    isTrackpadMode = false
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)

                    var stepAccumulator = 0f
                    var normalUp = false
                    val stepThresholdPx = 45f
                    val enterTrackpadThresholdPx = 22f

                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id }
                            ?: event.changes.firstOrNull()
                            ?: break

                        if (!change.pressed) {
                            normalUp = change.changedToUpIgnoreConsumed() && !change.isConsumed
                            break
                        }

                        val totalDx = change.position.x - down.position.x
                        if (!isTrackpadMode && abs(totalDx) > enterTrackpadThresholdPx) {
                            isTrackpadMode = true
                            stepAccumulator = totalDx
                            change.consume()
                        } else if (isTrackpadMode) {
                            stepAccumulator += change.positionChange().x
                            change.consume()
                        }

                        if (isTrackpadMode) {
                            while (stepAccumulator <= -stepThresholdPx) {
                                view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                latestOnMoveCursor(-1)
                                stepAccumulator += stepThresholdPx
                            }
                            while (stepAccumulator >= stepThresholdPx) {
                                view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
                                latestOnMoveCursor(1)
                                stepAccumulator -= stepThresholdPx
                            }
                        }
                    }

                    val wasTrackpad = isTrackpadMode
                    pressed = false
                    isTrackpadMode = false

                    if (!wasTrackpad && normalUp) {
                        latestOnSpace()
                    }
                }
            }
            .padding(horizontal = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = if (isTrackpadMode) "◀ 滑動移動游標 ▶" else label,
            color = if (isTrackpadMode) Color.White else ImeColors.TextSecondary,
            fontSize = 13.sp,
            fontWeight = if (isTrackpadMode) FontWeight.SemiBold else FontWeight.Normal,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * 智慧退格鍵：
 * - 按下立即單次退格並震動
 * - 按住 350ms 後每 45ms 高速連發退格（無緩衝區且按住超過 1500ms 改為按詞刪除）
 * - 在退格鍵上向左滑動超過 30dp 直接觸發一鍵清空緩衝區 / 刪除前一詞
 */
@Composable
private fun SmartBackspaceKey(
    height: Dp,
    hasInputBuffer: Boolean,
    modifier: Modifier = Modifier,
    onBackspace: () -> Unit,
    onSwipeLeftClear: () -> Unit,
) {
    val view = LocalView.current
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()

    var pressed by remember { mutableStateOf(false) }
    var swipedLeft by remember { mutableStateOf(false) }

    val latestBackspace by rememberUpdatedState(onBackspace)
    val latestSwipeLeftClear by rememberUpdatedState(onSwipeLeftClear)
    val latestHasInputBuffer by rememberUpdatedState(hasInputBuffer)
    val swipeThresholdPx = remember(density) { with(density) { 30.dp.toPx() } }

    Box(
        modifier = modifier
            .height(height)
            .clip(RoundedCornerShape(KeyCornerRadius))
            .background(
                when {
                    swipedLeft -> ImeColors.ColloquialBlue
                    pressed -> ImeColors.KeyPressedBg
                    else -> ImeColors.FuncKeyBg
                },
            )
            .border(0.5.dp, ImeColors.KeyBorder.copy(alpha = 0.6f), RoundedCornerShape(KeyCornerRadius))
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    swipedLeft = false
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    latestBackspace()

                    val repeatJob = scope.launch {
                        delay(350L)
                        var elapsedMs = 350L
                        while (true) {
                            if (elapsedMs >= 1500L && !latestHasInputBuffer) {
                                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                                latestSwipeLeftClear()
                                delay(120L)
                                elapsedMs += 120L
                            } else {
                                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                                latestBackspace()
                                delay(45L)
                                elapsedMs += 45L
                            }
                        }
                    }

                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id }
                            ?: event.changes.firstOrNull()
                            ?: break

                        if (!change.pressed) {
                            break
                        }

                        val dx = change.position.x - down.position.x
                        if (!swipedLeft && dx < -swipeThresholdPx) {
                            swipedLeft = true
                            repeatJob.cancel()
                            view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                            latestSwipeLeftClear()
                            change.consume()
                        }
                    }

                    repeatJob.cancel()
                    pressed = false
                    swipedLeft = false
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = if (swipedLeft) "✕" else "⌫",
            color = ImeColors.TextPrimary,
            fontSize = 18.sp,
            fontWeight = FontWeight.Medium,
        )
    }
}

@Composable
private fun TactileChipButton(
    label: String,
    backgroundColor: Color,
    textColor: Color,
    fontSize: TextUnit,
    fontWeight: FontWeight = FontWeight.Normal,
    borderColor: Color? = null,
    horizontalPadding: Dp = 9.dp,
    verticalPadding: Dp = 6.dp,
    cornerRadius: Dp = KeyCornerRadius,
    onClick: () -> Unit,
) {
    var pressed by remember { mutableStateOf(false) }
    val latestOnClick by rememberUpdatedState(onClick)
    val shape = RoundedCornerShape(cornerRadius)

    Box(
        modifier = Modifier
            .clip(shape)
            .background(if (pressed) ImeColors.KeyPressedBg else backgroundColor)
            .then(if (borderColor != null) Modifier.border(1.dp, borderColor, shape) else Modifier)
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    var normalUp = false
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull() ?: break
                        if (!change.pressed) {
                            normalUp = change.changedToUpIgnoreConsumed() && !change.isConsumed
                            break
                        }
                    }
                    pressed = false
                    if (normalUp) {
                        latestOnClick()
                    }
                }
            }
            .padding(horizontal = horizontalPadding, vertical = verticalPadding),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = label,
            color = textColor,
            fontSize = fontSize,
            fontWeight = fontWeight,
            maxLines = 1,
        )
    }
}

@Composable
private fun TactilePanelActionButton(
    label: String,
    backgroundColor: Color,
    textColor: Color,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    var pressed by remember { mutableStateOf(false) }
    val latestOnClick by rememberUpdatedState(onClick)

    Box(
        modifier = modifier
            .clip(RoundedCornerShape(KeyCornerRadius))
            .background(if (pressed) ImeColors.KeyPressedBg else backgroundColor)
            .border(0.5.dp, ImeColors.KeyBorder.copy(alpha = 0.6f), RoundedCornerShape(KeyCornerRadius))
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    var normalUp = false
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull() ?: break
                        if (!change.pressed) {
                            normalUp = change.changedToUpIgnoreConsumed() && !change.isConsumed
                            break
                        }
                    }
                    pressed = false
                    if (normalUp) {
                        latestOnClick()
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = label,
            color = textColor,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            textAlign = TextAlign.Center,
            lineHeight = 16.sp,
        )
    }
}
