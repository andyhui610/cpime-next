# Project: CPIME-Next (Cross-Platform Cantonese & English-Lookup IME)

## 1. Monorepo Architecture
- `/core` (Rust): 跨平台輸入法核心引擎（`cpime-core`），負責多制式粵拼正規化、模糊音展開、SQLite 雙軌詞庫檢索與候選詞混排。
- `/tools` (Python): 詞庫編譯腳本，負責將粵語詞庫與英漢詞庫編譯為 `/core/assets/cpime_dict.db`。
- `/windows`: Windows 桌面端輸入法前端。
- `/android`: Android 行動端輸入法前端。

## 2. Core CPIME Features
1. **多拼音方案相容**：內部統一正規化為標準粵拼（Jyutping），前端解析器需自動相容「教院式」、「耶魯拼音」、「劉錫祥拼音」與港式常見拼法（如 ch->c, sh->s, oo->u, ts->c, dz->z）。
2. **模糊音與懶音容錯**：支援 n/l 聲母互換、ng/零聲母互換、gw/g 與 kw/k 互換、-n/-ng 與 -t/-k 韻尾互換，預設免打聲調並支援首字母簡拼（如 nh -> 你好）。
3. **打英文直出中文（雙軌混合檢索）**：
   - 同時查詢粵拼詞庫與英漢反查詞庫（`english_dict`）。
   - 短碼撞碼時（如 `sun`, `man`, `wing`），前 2 位放最高頻粵拼字，第 3 至 4 位強制穿插英文直譯結果（如 `sun` 顯示 `1.新 2.身 3.日 4.太陽`）。
   - 純英文或長單字（如 `afternoon`, `lift`, `strawberry` 或前綴 `aftern`），英文反查結果直接排第 1 順位。
4. **字集安全**：完整支援 UTF-8 四位元組香港口語擴展漢字（如 嘅、喺、咗、唔、啲、𨋢、𥄫、㗎）。

## 3. Agent Workflow Rules
- 每次修改 `/core` 後，必須主動在終端機執行 `cargo test`，全數通過才可回報。
- 單次按鍵查詢延遲需低於 10ms。

## 4. User Dictionary & Contextual Learning (`user_dict.db`)
1. **雙庫分離架構**：
   - `cpime_dict.db` 為唯讀系統詞庫；`user_dict.db` 為獨立讀寫的個人學習詞庫（開啟 WAL 模式確保高併發寫入效能）。
2. **三大智慧學習機制**：
   - **Unigram 個人詞頻與近期置頂**：每次使用者選字上屏，更新 `user_unigrams` 的 `freq` 與 `last_used` 時間戳（無論輸入的是粵拼、簡拼或英文反查皆會記憶）。
   - **Bigram 前後文連帶排序**：引擎需維護 `last_committed_word` 狀態。當使用者連續輸入時，若 `(prev_word, current_input)` 命中 `user_bigrams`，該候選詞獲得最高優先權重直接置頂；同時支援在未輸入拼音時提供 `get_associative_words(prev_word)` 聯想詞建議。
   - **自造詞自動合成（Auto-Phrase Learning）**：當使用者在同一次連續輸入中，逐字選出 2 至 4 個單字組成新詞（且中間未按空白鍵或切換視窗），自動將合成後的詞組與對應無聲調全拼/簡拼寫入 `user_unigrams`。

## 5. Auto-Phrase Composition Algorithm (智慧自造詞自動合成規範)
1. **滑動視窗緩衝區 (`CompositionBuffer`)**：
   - 記錄使用者連續透過「粵拼單字」選出的單字序列（上限 4 個字），每個元素包含：`ch: char`、`canonical_toneless: String`（標準無聲調粵拼）、`abbr: char`、`timestamp_ms: u64`。
2. **斷開與過濾條件（Boundary & Stopword Rules）**：
   - 若兩次選字間隔超過 `4000ms`，或使用者輸入標點符號、空白鍵、Backspace、英文反查詞、或一次選出 >= 2 字的現有詞組，立即清空緩衝區。
   - 內建 `CANTONESE_STOPWORDS`（包含：我你佢哋嘅喺係唔咗啲咁咩嘢㗎啦囉喎啫吖呀有冇去嚟睇食做好同和與但就都仲又會要），凡選中上述單字，視為語法邊界，不進入合成緩衝區並同時觸發前序緩衝區結算。
3. **兩階段轉正機制（Staging Promotion）**：
   - 緩衝區累積達 2~4 個有效單字並觸發結算時，若該組合尚不存在於系統詞庫 `cantonese_dict`：
   - 第 1 次出現：在 `user_unigrams` 記錄 `freq = 1`（暫存觀察期，不參與一般檢索或僅給極低權重）。
   - 第 2 次出現：`freq` 升至 `2` 並正式轉正，此後輸入全拼（如 `hingjyu`）或簡拼（如 `hj`）皆可高權重直接檢索出該自造詞。

## 6. Windows Desktop Frontend (`/windows` Standalone Overlay IME)
1. **架構定位**：
   - 在 `/windows` 建立獨立 Rust binary crate (`cpime-win`)，直接依賴 `cpime-core`（共用同一個 Cargo Workspace），使用 `windows-sys`（或 `windows` crate）呼叫 Win32 API。
2. **鍵盤攔截與狀態機（`WH_KEYBOARD_LL`）**：
   - **切換熱鍵**：按下單獨的 `Left Shift`（或 `Ctrl + Space`）在「廣東話模式（粵）」與「英文直通模式（英）」之間切換。
   - **自發送防迴圈（Self-Injection Guard）**：呼叫 `SendInput` 送出字元時，必須在 `KEYBDINPUT.dwExtraInfo` 填入專屬魔術數字（如 `0x4350494D` "CPIM"），Hook 回呼檢查到此標記時直接 `CallNextHookEx` 放行。
   - **按鍵攔截邏輯（當處於「粵」模式）**：
     * `a-z`：若搭配 `Ctrl`/`Alt`/`Win` 組合鍵則直接放行（不阻斷 `Ctrl+C`/`Ctrl+V`）；純字母鍵則攔截（回傳 `1`）並送入 `Engine::process_key`，立即更新浮動候選視窗。
     * **當 `input_buffer` 非空（正在拼字選字中）**：
       - `Space`：選中第 1 個候選詞並上屏。
       - `1-9`：選中對應序號的候選詞並上屏。
       - `Enter`：直接將目前輸入的原始英文字串（`input_buffer`）原封不動送出上屏，並清空緩衝區。
       - `Backspace`：刪除最後一個拼音字母；若刪空則隱藏候選視窗。
       - `Escape`：取消當前輸入並清空緩衝區。
       - `-` / `=`（或 `PageUp` / `PageDown` / 上下方向鍵）：候選詞翻頁。
     * **當 `input_buffer` 為空**：所有按鍵直接放行給目標應用程式；若按下 `Enter`、方向鍵或滑鼠點擊，呼叫 `Engine::clear_context()` / `flush()` 結算自造詞滑動視窗。
3. **UTF-16 Surrogate Pairs 安全上屏（`SendInput`）**：
   - 將選中的中文字串透過 `text.encode_utf16()` 轉為 `u16` 陣列，逐一建立 `KEYEVENTF_UNICODE` 的按下與放開事件批次送出，確保「𨋢」、「𥄫」等 CJK 擴展區漢字完美上屏。
4. **無焦點浮動候選視窗（Win32 GDI / Direct2D Overlay）**：
   - 視窗樣式必須包含 `WS_POPUP | WS_DISABLED` 與擴展樣式 `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`，確保絕不搶奪目標視窗焦點。
   - 採用深色現代化主題（Dark Mode），第一行顯示「當前輸入拼音 + [粵/英狀態]」，第二行橫向顯示 `1.候選詞 2.候選詞 ...`（高亮第 1 順位，並標示英文反查或自造詞標籤），視窗位置自動跟隨文字輸入游標（`GetGUIThreadInfo` / `GetCaretPos`）或滑鼠游標右下方。

## 7. Windows System Tray & Output Mode (`Shell_NotifyIconW`)
1. **動態圖示生成（In-Memory GDI `HICON`）**：
   - 不依賴外部 `.ico` 檔案，透過 `CreateDIBSection` + GDI 文字繪製與 `CreateIconIndirect` 在記憶體中預先生成 3 個狀態圖示：
     * **粵語口語模式**：藍底白字 `「粵」`
     * **標準書面語模式**：綠底白字 `「書」`
     * **英文直通模式**：灰底白字 `「英」`
2. **系統列互動行為（`WM_TRAYICON`）**：
   - **左鍵單擊（`WM_LBUTTONUP`）**：快速切換「粵語輸入」與「英文直通」狀態，並即時呼叫 `Shell_NotifyIconW(NIM_MODIFY, ...)` 更新圖示。
   - **右鍵單擊（`WM_RBUTTONUP`）**：呼叫 `SetForegroundWindow` 後彈出原生系統選單（`CreatePopupMenu` + `TrackPopupMenu`），選單項目包含：
     1. `[✓] 廣東話輸入 (Left Shift / Ctrl+Space)`
     2. `---` (分隔線)
     3. `[●] 口語模式（道地粵字：嘅、喺、唔、𨋢）`
     4. `[ ] 書面語模式（粵音轉書面語：的、在、不、升降機）`
     5. `---` (分隔線)
     6. `重新載入個人詞庫 (user_dict.db)`
     7. `結束 CPIME-Next (Exit)`
3. **口語 / 書面語雙語體轉換（Output Style Mode）**：
   - 在 `/core` 的 `Engine` 中維護 `OutputMode::Colloquial`（預設口語）與 `OutputMode::Written`（書面語）。
   - 當處於 `OutputMode::Written` 時：
     * 英漢反查優先將非口語（`is_colloquial == 0`）的標準書面語排在最前面（如打 `afternoon` 優先排「下午、中午」而非「晏晝」；打 `lift` 優先排「升降機、電梯」而非「𨋢」）。
     * 內建高頻粵語詞彙轉書面語映射表（如輸入 `hai` 在書面語模式下額外置頂「是 / 在」、`m` 置頂「不」、`keoi` 置頂「他」、`keoidei` 置頂「他們」、`gamjat` 置頂「今天」、`tingjat` 置頂「明天」、`bin1go3` 置頂「誰」、`dim2gaai2` 置頂「為什麼」、`ge` 置頂「的」）。

## 8. Sentence Decoder: Word Lattice & Viterbi Algorithm (長句連打與步進選字規範)
1. **觸發條件與英文字保護**：
   - 當輸入字串包含 `>= 2` 個音節（或無法作為單一完整詞條精確匹配），且**未精確/前綴命中長英文單字（`english_dict` 長度 >= 5）**時，啟動長句解碼器（`SentenceDecoder`）。
2. **兩層式 DAG 詞圖建構（Two-Layer Lattice Construction）**：
   - **音節切分層（Syllable DAG）**：將連續無空格字串（如 `"ngogamjatheoihoenggong"`）切分為所有合法的標準粵拼音節路徑（相容教院/耶魯/劉錫祥正規化與模糊音），處理歧義邊界（如 `ngoi` vs `ng`+`oi`、`aa` vs `a`、`gw`/`kw`）。
   - **詞彙跨度層（Word Lattice）**：在音節序列 `[0..N]` 上，對所有長度為 `1..=6` 個音節的子區間 `[i..j]` 查詢 `cantonese_dict`、`user_unigrams`（含自造詞）與 `OutputMode::Written` 書面語映射表，建立有向邊 `Edge(i -> j, word, score)`。
3. **Viterbi 動態規劃計分公式（Path Scoring Formula）**：
   - 每個跨度邊的基礎分數：$EdgeScore(i, j) = BaseWeight + UserUnigramBoost + (SyllableSpan^2 \times 3500) - 2000$（跨音節數 `SyllableSpan = j - i` 平方加分並扣除切分懲罰 `-2000`，確保「今日(2音節)」的分數絕對高於「金(1音節) + 日(1音節)」）。
   - 轉移加分（Bigram Transition）：若前序節點最佳路徑的結尾詞 `prev_word` 與當前邊在 `user_bigrams` 匹配，額外加上 $+6000$ 轉移分數。
   - 透過 Viterbi 演算法從音節 `0` 推算至終點 `N`，回溯找出總分最高的全句組合，作為候選清單的第 1 順位（`CandidateSource::Sentence`）。
4. **步進式部分選字（Progressive Partial Commit）**：
   - 每個 `Candidate` 必須記錄 `consumed_len: usize`（該候選詞實際消耗的原始輸入字元長度）。
   - 整句候選詞的 `consumed_len = input.len()`（選中後整句上屏並清空 `input_buffer`）。
   - 排在第 2 名之後的開頭前綴詞/單字（如長句開頭的「今日」或「我」），其 `consumed_len` 為該詞對應的原始拼音長度（如 `"ngo"` 為 3）。當使用者選中它時，僅將該詞上屏，並從 `input_buffer` 前端切除 `consumed_len` 個字元，剩餘拼音留在緩衝區立即重新解碼！

## 9. Android IME Frontend (`/android` JNI + InputMethodService + Jetpack Compose)
1. **Rust JNI 橋接層 (`/core/src/jni_bridge.rs`)**：
   - 啟用 `cdylib` 編譯目標（輸出 `libcpime_core.so`），使用 `jni` crate 暴露給 Kotlin `com.andyhui.cpime.NativeEngine` 呼叫：
     * `initEngine(sysDbPath: String, userDbPath: String): Long`（回傳 `Box::into_raw` 的引擎指標）
     * `destroyEngine(ptr: Long)`
     * `getSegmentedPinyin(ptr: Long, input: String): String`
     * `getCandidatesJson(ptr: Long, input: String, limit: Int): String`（回傳 JSON 陣列，含 `word`, `consumedChars`, `source`）
     * `commitCandidate(ptr: Long, input: String, index: Int): String`（記錄 Unigram/Bigram/自造詞並回傳上屏詞與消耗長度）
     * `getPredictionsJson(ptr: Long, limit: Int): String`
     * `setOutputMode(ptr: Long, isWritten: Boolean)`
     * `clearContext(ptr: Long)`
2. **資料庫初始化與資產複製 (`AssetCopyHelper`)**：
   - 首次啟動鍵盤時，檢查 `context.filesDir/cpime_dict.db` 是否存在，若不存在則從 APK `assets/cpime_dict.db` 複製過去；個人詞庫則固定存放於 `context.filesDir/user_dict.db`。
3. **`InputMethodService` + Jetpack Compose 生命週期綁定**：
   - 建立 `CantoneseIMEService : InputMethodService, LifecycleOwner, ViewModelStoreOwner, SavedStateRegistryOwner`，並將 `window.window.decorView` 透過 `setViewTreeLifecycleOwner` 等擴充方法綁定至 Service，避免 `ComposeView` 因找不到 Lifecycle 而崩潰。
4. **軟鍵盤 UI 與操作體驗 (`KeyboardScreen.kt`)**：
   - **頂部狀態與拼音列（Composing Bar）**：顯示格式化後的音節字串（如 `ngo'gam'jat`），右側提供 `[粵/書]`（口語/書面語）一鍵切換膠囊與 `[粵/英]` 切換鈕。
   - **候選詞橫向捲動列（Candidate Row）**：使用 `LazyRow` 顯示候選詞（第 1 順位高亮顯示，英文反查詞附小字提示），支援點擊候選詞觸發「整句上屏」或「剝洋蔥式部分選字（Partial Commit）」。當未輸入拼音時，顯示 Bigram 聯想詞。
   - **鍵盤主體（QWERTY 26 鍵 + 廣東話標點列）**：支援震動回饋（Haptic Feedback）、長按退格鍵連續刪除、空白鍵選首詞上屏、`Enter` 鍵送出原始英文字串。



