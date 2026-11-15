package com.andyhui.cpime

import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

class MainActivity : ComponentActivity() {
    private var isImeEnabled by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        refreshImeEnabledStatus()
        setContent {
            MaterialTheme(
                colorScheme = darkColorScheme(
                    primary = Color(0xFF3B82F6),
                    background = Color(0xFF0F172A),
                    surface = Color(0xFF1E293B),
                ),
            ) {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color(0xFF0F172A),
                ) {
                    SetupGuideScreen(
                        isImeEnabled = isImeEnabled,
                        onOpenImeSettings = {
                            startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                        },
                        onShowImePicker = {
                            val imm = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
                            imm.showInputMethodPicker()
                        },
                    )
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        refreshImeEnabledStatus()
    }

    private fun refreshImeEnabledStatus() {
        val imm = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
        isImeEnabled = imm.enabledInputMethodList.any { it.packageName == packageName }
    }
}

@Composable
private fun SetupGuideScreen(
    isImeEnabled: Boolean,
    onOpenImeSettings: () -> Unit,
    onShowImePicker: () -> Unit,
) {
    var testText by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xFF0F172A))
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Spacer(modifier = Modifier.height(8.dp))

        Text(
            text = "CPIME-Next 廣東話輸入法",
            color = Color(0xFFF8FAFC),
            fontSize = 24.sp,
            fontWeight = FontWeight.Bold,
        )
        Text(
            text = "多制式粵拼（標準／教院／耶魯／劉錫祥）‧ 懶音容錯 ‧ 打英文直出中文 ‧ 長句連打與口語／書面語即時切換",
            color = Color(0xFF94A3B8),
            fontSize = 13.sp,
            lineHeight = 19.sp,
        )

        Card(
            colors = CardDefaults.cardColors(containerColor = Color(0xFF1E293B)),
            shape = RoundedCornerShape(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        text = "啟用狀態",
                        color = Color(0xFFCBD5E1),
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                    )
                    Text(
                        text = if (isImeEnabled) "已於系統啟用 ✓" else "尚未啟用",
                        color = if (isImeEnabled) Color(0xFF34D399) else Color(0xFFFBBF24),
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }

                Button(
                    onClick = onOpenImeSettings,
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(10.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF2563EB)),
                ) {
                    Text(
                        text = "1. 前往系統設定啟用 CPIME-Next 鍵盤",
                        color = Color.White,
                        fontSize = 15.sp,
                        modifier = Modifier.padding(vertical = 4.dp),
                    )
                }

                OutlinedButton(
                    onClick = onShowImePicker,
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(10.dp),
                ) {
                    Text(
                        text = "2. 切換目前輸入法為 CPIME-Next",
                        color = Color(0xFF93C5FD),
                        fontSize = 15.sp,
                        modifier = Modifier.padding(vertical = 4.dp),
                    )
                }
            }
        }

        Text(
            text = "即時試打測試區",
            color = Color(0xFFE2E8F0),
            fontSize = 16.sp,
            fontWeight = FontWeight.SemiBold,
        )

        OutlinedTextField(
            value = testText,
            onValueChange = { testText = it },
            modifier = Modifier
                .fillMaxWidth()
                .height(150.dp),
            placeholder = {
                Text(
                    text = "點此測試打字：試下打 nh（你好）、sun（新/太陽）、lift（𨋢）、ngogamjatheoihoenggong（長句連打）...",
                    color = Color(0xFF64748B),
                    fontSize = 14.sp,
                )
            },
            colors = OutlinedTextFieldDefaults.colors(
                focusedTextColor = Color(0xFFF8FAFC),
                unfocusedTextColor = Color(0xFFF8FAFC),
                focusedBorderColor = Color(0xFF3B82F6),
                unfocusedBorderColor = Color(0xFF334155),
                focusedContainerColor = Color(0xFF1E293B),
                unfocusedContainerColor = Color(0xFF1E293B),
            ),
            shape = RoundedCornerShape(12.dp),
        )
    }
}
