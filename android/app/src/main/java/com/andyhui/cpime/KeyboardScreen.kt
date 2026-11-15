package com.andyhui.cpime

import android.view.HapticFeedbackConstants
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private object ImeColors {
    val KeyboardBg = Color(0xFF12151C)
    val BarBg = Color(0xFF181C26)
    val KeyBg = Color(0xFF252B38)
    val KeyPressedBg = Color(0xFF384154)
    val FuncKeyBg = Color(0xFF1C212C)
    val KeyBorder = Color(0xFF2E3545)
    val TextPrimary = Color(0xFFF3F4F6)
    val TextSecondary = Color(0xFF9CA3AF)
    val PinyinAccent = Color(0xFF93C5FD)
    val ColloquialBlue = Color(0xFF2563EB)
    val WrittenGreen = Color(0xFF059669)
    val EnglishSlate = Color(0xFF4B5563)
    val CandidateHighlight = Color(0xFF1E3A8A)
    val CandidateHighlightBorder = Color(0xFF3B82F6)
    val CandidateChipBg = Color(0xFF202634)
    val BadgeEnglish = Color(0xFFF59E0B)
    val BadgeSentence = Color(0xFF34D399)
    val BadgeCustom = Color(0xFFA78BFA)
}

private val QwertyRow1 = listOf("q", "w", "e", "r", "t", "y", "u", "i", "o", "p")
private val QwertyRow2 = listOf("a", "s", "d", "f", "g", "h", "j", "k", "l")
private val QwertyRow3 = listOf("z", "x", "c", "v", "b", "n", "m")

private val SymbolsRow1 = listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "0")
private val SymbolsRow2 = listOf("@", "#", "$", "%", "&", "-", "+", "(", ")")
private val SymbolsRow3 = listOf("*", "\"", "'", ":", ";", "/", "!")

private val CantonesePunctuations = listOf("，", "。", "？", "！", "「", "」", "、", "…")

@Composable
fun KeyboardScreen(
    inputBuffer: String,
    segmentedPinyin: String,
    candidates: List<CandidateItem>,
    predictions: List<CandidateItem>,
    isCantoneseMode: Boolean,
    isWrittenMode: Boolean,
    isEngineReady: Boolean,
    onLetterClick: (String) -> Unit,
    onBackspace: () -> Unit,
    onSpace: () -> Unit,
    onEnter: () -> Unit,
    onCandidateClick: (Int, CandidateItem) -> Unit,
    onPunctuationClick: (String) -> Unit,
    onToggleLanguage: () -> Unit,
    onToggleOutputMode: () -> Unit,
    onClearBuffer: () -> Unit,
) {
    val view = LocalView.current
    var isShifted by remember { mutableStateOf(false) }
    var isSymbolsMode by remember { mutableStateOf(false) }

    // 明確宣告為 () -> Unit，避免 performHapticFeedback 回傳 Boolean 導致型別推斷為 () -> Boolean
    val triggerHaptic: () -> Unit = remember(view) {
        {
            view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
            Unit
        }
    }

    val displayedItems = if (inputBuffer.isNotEmpty()) candidates else predictions
    val isShowingPredictions = inputBuffer.isEmpty() && predictions.isNotEmpty()

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(ImeColors.KeyboardBg)
            .padding(bottom = 8.dp),
    ) {
        // 1. 頂部拼音切分與狀態預覽列 (Composing Bar)
        ComposingBar(
            inputBuffer = inputBuffer,
            segmentedPinyin = segmentedPinyin,
            isCantoneseMode = isCantoneseMode,
            isWrittenMode = isWrittenMode,
            isEngineReady = isEngineReady,
            isShowingPredictions = isShowingPredictions,
            onClearBuffer = {
                triggerHaptic()
                onClearBuffer()
            },
        )

        // 2. 工具鈕與橫向捲動候選詞列 (Toolbar + Candidate LazyRow)
        CandidateToolbarRow(
            candidates = displayedItems,
            isCantoneseMode = isCantoneseMode,
            isWrittenMode = isWrittenMode,
            isShowingPredictions = isShowingPredictions,
            hasActiveInput = inputBuffer.isNotEmpty(),
            onCandidateClick = { idx, item ->
                triggerHaptic()
                onCandidateClick(idx, item)
            },
            onToggleOutputMode = {
                triggerHaptic()
                onToggleOutputMode()
            },
            onToggleLanguage = {
                triggerHaptic()
                onToggleLanguage()
            },
        )

        // 3. 常用廣東話標點快選列
        PunctuationQuickStrip(
            punctuations = CantonesePunctuations,
            onPunctuationClick = { p ->
                triggerHaptic()
                onPunctuationClick(p)
            },
        )

        Spacer(modifier = Modifier.height(4.dp))

        // 4. 鍵盤主體按鍵區
        if (isSymbolsMode) {
            SymbolKeyboardBody(
                onSymbolClick = { sym ->
                    triggerHaptic()
                    onPunctuationClick(sym)
                },
                onBackspace = onBackspace,
                onSwitchToAlpha = {
                    triggerHaptic()
                    isSymbolsMode = false
                },
                onSpace = {
                    triggerHaptic()
                    onSpace()
                },
                onEnter = {
                    triggerHaptic()
                    onEnter()
                },
                onHaptic = triggerHaptic,
            )
        } else {
            QwertyKeyboardBody(
                isShifted = isShifted,
                isCantoneseMode = isCantoneseMode,
                isWrittenMode = isWrittenMode,
                hasInputBuffer = inputBuffer.isNotEmpty(),
                onLetterClick = { ch ->
                    triggerHaptic()
                    val output = if (!isCantoneseMode && isShifted) ch.uppercase() else ch
                    onLetterClick(output)
                    if (!isCantoneseMode && isShifted) {
                        isShifted = false
                    }
                },
                onToggleShift = {
                    triggerHaptic()
                    isShifted = !isShifted
                },
                onSwitchToSymbols = {
                    triggerHaptic()
                    isSymbolsMode = true
                },
                onToggleLanguage = {
                    triggerHaptic()
                    onToggleLanguage()
                },
                onPunctuationClick = { p ->
                    triggerHaptic()
                    onPunctuationClick(p)
                },
                onBackspace = onBackspace,
                onSpace = {
                    triggerHaptic()
                    onSpace()
                },
                onEnter = {
                    triggerHaptic()
                    onEnter()
                },
                onHaptic = triggerHaptic,
            )
        }
    }
}

@Composable
private fun ComposingBar(
    inputBuffer: String,
    segmentedPinyin: String,
    isCantoneseMode: Boolean,
    isWrittenMode: Boolean,
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
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(6.dp))
                    .background(ImeColors.KeyBg)
                    .clickable(onClick = onClearBuffer)
                    .padding(horizontal = 8.dp, vertical = 2.dp),
            ) {
                Text(
                    text = "✕ 清空",
                    color = ImeColors.TextSecondary,
                    fontSize = 11.sp,
                )
            }
        } else {
            val statusText = when {
                !isEngineReady -> "正在初始化 CPIME-Next 詞庫引擎..."
                !isCantoneseMode -> "English 直通模式 · 點擊「英」切回廣東話"
                isShowingPredictions -> "語境聯想詞（Bigram）· 直接點選或輸入粵拼/英文"
                isWrittenMode -> "CPIME-Next 廣東話 · 書面語模式 (粵音轉書面語)"
                else -> "CPIME-Next 廣東話 · 口語模式 (粵拼 / 簡拼 / 打英文出中文)"
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
    isShowingPredictions: Boolean,
    hasActiveInput: Boolean,
    onCandidateClick: (Int, CandidateItem) -> Unit,
    onToggleOutputMode: () -> Unit,
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
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(8.dp))
                .background(modeBg)
                .clickable(onClick = onToggleOutputMode)
                .padding(horizontal = 10.dp, vertical = 7.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = modeLabel,
                color = Color.White,
                fontSize = 13.sp,
                fontWeight = FontWeight.Bold,
            )
        }

        Spacer(modifier = Modifier.width(5.dp))

        // 固定置頂：[粵/英] 輸入語言切換鈕
        val langBg = if (isCantoneseMode) ImeColors.KeyBg else ImeColors.EnglishSlate
        val langLabel = if (isCantoneseMode) "粵/英" else "英"
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(8.dp))
                .background(langBg)
                .border(1.dp, ImeColors.KeyBorder, RoundedCornerShape(8.dp))
                .clickable(onClick = onToggleLanguage)
                .padding(horizontal = 9.dp, vertical = 7.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = langLabel,
                color = ImeColors.TextPrimary,
                fontSize = 12.sp,
                fontWeight = FontWeight.Medium,
            )
        }

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

    val hintText = when {
        isPrediction -> "聯想"
        item.source == "英文反查" -> item.matchedCode.ifEmpty { "英譯" }
        item.source == "整句連打" -> "整句"
        item.source == "個人詞庫" -> "自造"
        item.source == "懶音" -> "懶音"
        else -> ""
    }
    val hintColor = when {
        item.source == "英文反查" -> ImeColors.BadgeEnglish
        item.source == "整句連打" -> ImeColors.BadgeSentence
        item.source == "個人詞庫" -> ImeColors.BadgeCustom
        else -> ImeColors.TextSecondary
    }

    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .background(bg)
            .border(1.dp, borderColor, RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = item.word,
            color = ImeColors.TextPrimary,
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
private fun PunctuationQuickStrip(
    punctuations: List<String>,
    onPunctuationClick: (String) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(Color(0xFF151922))
            .padding(horizontal = 6.dp, vertical = 3.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        punctuations.forEach { p ->
            Box(
                modifier = Modifier
                    .weight(1f)
                    .height(28.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .background(ImeColors.FuncKeyBg)
                    .clickable { onPunctuationClick(p) },
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
    hasInputBuffer: Boolean,
    onLetterClick: (String) -> Unit,
    onToggleShift: () -> Unit,
    onSwitchToSymbols: () -> Unit,
    onToggleLanguage: () -> Unit,
    onPunctuationClick: (String) -> Unit,
    onBackspace: () -> Unit,
    onSpace: () -> Unit,
    onEnter: () -> Unit,
    onHaptic: () -> Unit,
) {
    val keyHeight = 46.dp
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 4.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // 第 1 排：Q W E R T Y U I O P
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            QwertyRow1.forEach { letter ->
                val label = if (!isCantoneseMode && isShifted) letter.uppercase() else letter
                KeyButton(
                    label = label,
                    height = keyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onLetterClick(letter) },
                )
            }
        }

        // 第 2 排：A S D F G H J K L（左右微留白對齊標準鍵盤階梯感）
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            QwertyRow2.forEach { letter ->
                val label = if (!isCantoneseMode && isShifted) letter.uppercase() else letter
                KeyButton(
                    label = label,
                    height = keyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onLetterClick(letter) },
                )
            }
        }

        // 第 3 排：Shift / Z X C V B N M / Backspace（支援長按連續刪除）
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            val shiftLabel = if (!isCantoneseMode && isShifted) "⬆" else "⇧"
            KeyButton(
                label = shiftLabel,
                height = keyHeight,
                backgroundColor = if (!isCantoneseMode && isShifted) ImeColors.ColloquialBlue else ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1.35f),
                onClick = onToggleShift,
            )

            QwertyRow3.forEach { letter ->
                val label = if (!isCantoneseMode && isShifted) letter.uppercase() else letter
                KeyButton(
                    label = label,
                    height = keyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onLetterClick(letter) },
                )
            }

            RepeatableBackspaceKey(
                height = keyHeight,
                modifier = Modifier.weight(1.35f),
                onBackspace = onBackspace,
                onHaptic = onHaptic,
            )
        }

        // 第 4 排底部功能列：123 | 粵/英 | ， | 空白鍵 (顯示目前模式) | 。 | Enter
        val spaceLabel = when {
            !isCantoneseMode -> "CPIME English"
            hasInputBuffer -> "選取首選詞"
            isWrittenMode -> "CPIME 廣東話 (書面語)"
            else -> "CPIME 廣東話 (口語)"
        }
        val enterLabel = if (isCantoneseMode && hasInputBuffer) "原文" else "↵"

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            KeyButton(
                label = "123",
                fontSize = 14.sp,
                height = keyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1.3f),
                onClick = onSwitchToSymbols,
            )
            KeyButton(
                label = if (isCantoneseMode) "粵" else "英",
                fontSize = 15.sp,
                height = keyHeight,
                backgroundColor = if (isCantoneseMode) ImeColors.ColloquialBlue else ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1.1f),
                onClick = onToggleLanguage,
            )
            KeyButton(
                label = if (isCantoneseMode) "，" else ",",
                fontSize = 18.sp,
                height = keyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(0.95f),
                onClick = { onPunctuationClick(if (isCantoneseMode) "，" else ",") },
            )
            KeyButton(
                label = spaceLabel,
                fontSize = 13.sp,
                height = keyHeight,
                backgroundColor = ImeColors.KeyBg,
                textColor = ImeColors.TextSecondary,
                modifier = Modifier.weight(3.8f),
                onClick = onSpace,
            )
            KeyButton(
                label = if (isCantoneseMode) "。" else ".",
                fontSize = 18.sp,
                height = keyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(0.95f),
                onClick = { onPunctuationClick(if (isCantoneseMode) "。" else ".") },
            )
            KeyButton(
                label = enterLabel,
                fontSize = if (enterLabel == "原文") 13.sp else 18.sp,
                height = keyHeight,
                backgroundColor = ImeColors.ColloquialBlue,
                modifier = Modifier.weight(1.5f),
                onClick = onEnter,
            )
        }
    }
}

@Composable
private fun SymbolKeyboardBody(
    onSymbolClick: (String) -> Unit,
    onBackspace: () -> Unit,
    onSwitchToAlpha: () -> Unit,
    onSpace: () -> Unit,
    onEnter: () -> Unit,
    onHaptic: () -> Unit,
) {
    val keyHeight = 46.dp
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 4.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            SymbolsRow1.forEach { sym ->
                KeyButton(
                    label = sym,
                    height = keyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onSymbolClick(sym) },
                )
            }
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            SymbolsRow2.forEach { sym ->
                KeyButton(
                    label = sym,
                    height = keyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onSymbolClick(sym) },
                )
            }
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            KeyButton(
                label = "：",
                height = keyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1.3f),
                onClick = { onSymbolClick("：") },
            )
            SymbolsRow3.forEach { sym ->
                KeyButton(
                    label = sym,
                    height = keyHeight,
                    modifier = Modifier.weight(1f),
                    onClick = { onSymbolClick(sym) },
                )
            }
            RepeatableBackspaceKey(
                height = keyHeight,
                modifier = Modifier.weight(1.3f),
                onBackspace = onBackspace,
                onHaptic = onHaptic,
            )
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            KeyButton(
                label = "ABC",
                fontSize = 14.sp,
                height = keyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1.6f),
                onClick = onSwitchToAlpha,
            )
            KeyButton(
                label = "（",
                height = keyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1f),
                onClick = { onSymbolClick("（") },
            )
            KeyButton(
                label = "空白",
                fontSize = 13.sp,
                height = keyHeight,
                textColor = ImeColors.TextSecondary,
                modifier = Modifier.weight(4f),
                onClick = onSpace,
            )
            KeyButton(
                label = "）",
                height = keyHeight,
                backgroundColor = ImeColors.FuncKeyBg,
                modifier = Modifier.weight(1f),
                onClick = { onSymbolClick("）") },
            )
            KeyButton(
                label = "↵",
                fontSize = 18.sp,
                height = keyHeight,
                backgroundColor = ImeColors.ColloquialBlue,
                modifier = Modifier.weight(1.6f),
                onClick = onEnter,
            )
        }
    }
}

@Composable
private fun KeyButton(
    label: String,
    modifier: Modifier = Modifier,
    height: Dp = 46.dp,
    fontSize: TextUnit = 19.sp,
    backgroundColor: Color = ImeColors.KeyBg,
    textColor: Color = ImeColors.TextPrimary,
    onClick: () -> Unit,
) {
    var pressed by remember { mutableStateOf(false) }
    val latestOnClick by rememberUpdatedState(onClick)

    Box(
        modifier = modifier
            .height(height)
            .widthIn(min = 24.dp)
            .clip(RoundedCornerShape(7.dp))
            .background(if (pressed) ImeColors.KeyPressedBg else backgroundColor)
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    val up = waitForUpOrCancellation()
                    pressed = false
                    if (up != null) {
                        latestOnClick()
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = label,
            color = textColor,
            fontSize = fontSize,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
        )
    }
}

@Composable
private fun RepeatableBackspaceKey(
    height: Dp,
    modifier: Modifier = Modifier,
    onBackspace: () -> Unit,
    onHaptic: () -> Unit,
) {
    var pressed by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val latestBackspace by rememberUpdatedState(onBackspace)
    val latestHaptic by rememberUpdatedState(onHaptic)

    Box(
        modifier = modifier
            .height(height)
            .clip(RoundedCornerShape(7.dp))
            .background(if (pressed) ImeColors.KeyPressedBg else ImeColors.FuncKeyBg)
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    pressed = true
                    latestHaptic()
                    latestBackspace()

                    val repeatJob = scope.launch {
                        delay(380L)
                        while (true) {
                            latestHaptic()
                            latestBackspace()
                            delay(65L)
                        }
                    }

                    waitForUpOrCancellation()
                    repeatJob.cancel()
                    pressed = false
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = "⌫",
            color = ImeColors.TextPrimary,
            fontSize = 18.sp,
            fontWeight = FontWeight.Medium,
        )
    }
}
