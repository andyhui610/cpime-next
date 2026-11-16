# CPIME-Next

**跨平台現代化廣東話及英漢反查雙軌輸入法**  
*A Modern Cross-Platform Cantonese & English-Lookup Dual-Track Input Method Engine*

[![CI](https://github.com/andyhui610/cpime-next/actions/workflows/ci.yml/badge.svg)](https://github.com/andyhui610/cpime-next/actions/workflows/ci.yml)
[![Release](https://github.com/andyhui610/cpime-next/actions/workflows/release.yml/badge.svg)](https://github.com/andyhui610/cpime-next/actions/workflows/release.yml)
![Platform: Windows x64](https://img.shields.io/badge/Platform-Windows%20x64-0078D6)
![Platform: Android 8.0+](https://img.shields.io/badge/Platform-Android%208.0%2B%20(API%2026%2B)-3DDC84)

致敬經典 CPIME（Cantonese Pinyin Input Method Editor），**CPIME-Next** 由高效能 **Rust** 核心引擎（`cpime-core`）驅動，同時提供 **Windows 桌面端獨立浮動視窗輸入法**（`cpime-win`）與 **Android Jetpack Compose 行動端輸入法**（`android`），並內建 Python 詞庫編譯器，實現多制式粵拼混打、全拼／簡拼混合長句連打、打英文單字直出中文，以及英文整句直出廣東話功能。

Built in tribute to the classic CPIME, **CPIME-Next** is powered by a high-performance **Rust** core engine (`cpime-core`) and ships with both a standalone native **Windows** overlay IME (`cpime-win`) and an **Android** Jetpack Compose IME (`android`).

---

## 📥 一般使用者下載與安裝指南 / Download & Install (For End Users)

一般使用者無需安裝編譯環境，直接下載預編譯成品即可使用：

### 方法 A：Windows 桌面免安裝版 (`CPIME-Next-Windows-x64.zip`)

1. 前往 [GitHub Releases](https://github.com/andyhui610/cpime-next/releases) 下載最新版本的 **`CPIME-Next-Windows-x64.zip`**（已內建完整雙軌詞庫 `assets/cpime_dict.db`）。
2. 解壓縮至任意資料夾，雙擊執行 **`CPIME-Next.exe`** 即可使用（綠色免安裝、不修改系統登錄檔；個人學習詞庫 `user_dict.db` 會自動建立於同一目錄下）。
3. 啟動後，Windows 右下角系統工作列（System Tray）會出現動態狀態圖示：
   - 🟦 **「粵」**（藍底白字）：**廣東話口語模式**（道地粵字：嘅、喺、唔、𨋢）
   - 🟩 **「書」**（綠底白字）：**廣東話書面語模式**（粵音轉標準書面語：的、在、不、升降機）
   - 🟪 **「譯」**（紫底白字）：**英文整句直出廣東話模式**（輸入英文整句自動重組語序轉為廣東話）
   - ⬜ **「英」**（灰底白字）：**英文直通模式**
4. **系統列圖示操作**：
   - 滑鼠**左鍵單擊**：快速切換「廣東話輸入」與「英文直通模式」。
   - 滑鼠**右鍵單擊**：開啟功能選單，可切換「啟用廣東話輸入」、「英譯粵整句模式」、「口語模式 / 書面語模式」、執行「重新載入個人詞庫 (`user_dict.db`)」或「結束 CPIME-Next」。

> **提示**：可在記事本（Notepad）、Chrome / Edge 瀏覽器、VS Code 等任意 Windows 視窗中直接使用；候選視窗採用無焦點（`WS_EX_NOACTIVATE`）懸浮設計，不搶奪原程式游標焦點。

### 方法 B：Android 手機版 (`CPIME-Next-Android-debug.apk`)

1. 前往 [GitHub Releases](https://github.com/andyhui610/cpime-next/releases) 下載 **`CPIME-Next-Android-debug.apk`** 至 Android 手機並安裝（支援 **Android 8.0 / API 26** 以上，已內建完整詞庫）。
2. 開啟桌面上的 **「CPIME-Next 廣東話輸入法」** App，依畫面引導完成兩步設定：
   - 點擊 **「1. 前往系統設定啟用 CPIME-Next 鍵盤」**，在系統「管理螢幕鍵盤」中開啟 **CPIME-Next 廣東話輸入法**。
   - 返回 App 點擊 **「2. 切換目前輸入法為 CPIME-Next」**，於輸入法選擇器中選取 **CPIME-Next**。
3. 可直接在 App 內的「即時試打測試區」測試打字，或切換至 WhatsApp、Signal、Telegram、瀏覽器等任何 App 開始輸入。

---

## ⌨️ 操作方式與快捷鍵一覽 / Controls & Shortcuts

### Windows 桌面端快捷鍵

| 按鍵 / Shortcut | 功能說明 / Action |
| :--- | :--- |
| `Left Shift`（單按放開）或 `Ctrl + Space` | 切換 **廣東話模式（粵）** / **英文直通模式（英）** |
| `Ctrl + Shift + S` 或 `F4` | 秒速切換 **口語模式（粵）** / **書面語模式（書）** |
| `Ctrl + Shift + E` | 切換 **英譯粵整句模式（譯）** |
| `;` 或 `/`（於緩衝區為空時輸入） | **免切換前綴**：在一般粵語模式下直接輸入英文整句轉廣東話（如 `;I go to school by bus`） |
| `a - z` | 輸入粵拼、簡拼或英文反查字串 |
| `Space`（空白鍵） | 一般粵拼模式下選取當頁第 1 個候選詞上屏；在「英譯粵整句模式」下單按插入空格，**連按兩下空白鍵（Double Space）** 將首選廣東話整句上屏 |
| `1 - 9` | 選取當頁對應序號（1–9）候選詞上屏（長句時選第 2 順位起的前綴詞可觸發「剝洋蔥式」分步選字） |
| `Enter` | 一般粵拼模式下將目前輸入的原始英文字母直接上屏；「英譯粵整句模式」下將第 1 順位廣東話整句上屏 |
| `-` / `=` 或 `PageUp` / `PageDown` 或 `↑` / `↓` | 候選詞清單上一頁 / 下一頁翻頁 |
| `Backspace` | 刪除最後一個輸入字元（刪空則隱藏浮動候選視窗） |
| `Esc` | 取消目前輸入並清空緩衝區 |
| `Ctrl + Alt + Q` | 安全結束 CPIME-Next 背景常駐程序 |

### Android 軟鍵盤操作說明

- **頂部狀態與拼音列（Composing Bar）**：左側即時顯示切分後的音節（如 `ngo'gam'jat` 或 `gam'jat'h'g`）與清空按鈕（`✕ 清空`），未輸入時顯示目前模式或語境聯想提示。
- **工具列模式切換膠囊**：
  - **`[粵]` / `[書]`**：一鍵切換「口語模式」與「書面語模式」。
  - **`[譯]`**：點亮紫色後進入「英文整句直出廣東話模式」（空白鍵可於句中輸入空格，**連按兩下空白鍵**或點擊 **`上屏`** 鍵即可將首選廣東話整句上屏）。
  - **`[粵/英]` / `[英]`**：切換廣東話與純英文輸入模式。
- **候選詞橫向捲動列（Candidate Row）**：橫向滑動瀏覽候選詞；點擊第 1 順位高亮詞可整句上屏，點擊後方前綴詞可執行「剝洋蔥式分步選字（Partial Commit）」；未輸入拼音時自動顯示上下文 Bigram 聯想詞。
- **標點與退格體驗**：內建常用廣東話標點快選列（`，。？！「」、…`）、`123` 數字符號鍵盤切換、按鍵震動回饋（Haptic Feedback），以及長按退格鍵（`⌫`）連續刪除。

---

## 💡 實用打字秘笈與範例 / Typing Guide & Examples

| 輸入場景 / Scenario | 輸入範例 / Input | 候選結果 / Output & Behavior |
| :--- | :--- | :--- |
| **多制式粵拼無縫混打** | `ngo` / `tsung` / `dzau` / `chut` / `shui` / `kwoo` | 自動正規化為標準粵拼：`ngo`、`cung`、`zau`、`ceot`、`seoi`、`gwu` |
| **懶音與模糊音容錯** | `lei`（代替 `nei`）、`oi`（代替 `ngoi`）、`go`（代替 `gwo`）、`han`（代替 `hang`） | 支援 `n/l`、`ng/零聲母`、`gw/g`、`kw/k`、`-n/-ng`、`-t/-k` 容錯檢索 |
| **首字母簡拼** | `nh`、`gj`、`hg` | `nh` → **你好**；`gj` → **今日**；`hg` → **香港** |
| **全拼 + 簡拼混合長句連打** | `gamjathg`、`ngogjhhg`、`nhma` | `gamjathg` (`gam'jat'h'g`) → **今日香港**<br>`ngogjhhg` (`ngo'g'j'h'h'g`) → **我今日去香港**<br>`nhma` → **你好嗎** |
| **Viterbi 長句連打與分步選字** | `ngogamjatheoihoenggong` / `gamjattingjat` | 第 1 順位整句 **我今日去香港** / **今日聽日**；若在 `gamjattingjat` 按 `2` 先選前綴詞 **今日**，緩衝區會自動留下 `tingjat` 並即時解碼為 **聽日** |
| **單字打英文直出中文（雙軌）** | `sun`、`lift`、`afternoon`、`aftern` | 短碼撞碼 `sun` → `1.新 2.身 3.日 4.太陽`<br>長單字/前綴直排第一：`lift` → **𨋢 / 升降機**，`afternoon` → **中午 / 下午 / 晏晝** |
| **英文整句直出廣東話** | `;I go to school by bus`<br>`;we go to work by mtr today`<br>`;have lunch with my friend`<br>`;how much is it` / `;eat apple` / `/go home on foot` | 自動將時間、伴隨、交通工具前置為道地廣東話語序：<br>口語：`我搭巴士返學校` / `我搭巴士返學`、`我哋今日搭地鐵返工`、`同我朋友食晏`、`幾多錢呀`、`食蘋果`、`行路返屋企`<br>書面語（切換 `[書]`）：`我乘巴士去學校`、`多少錢` |

### 口語 `[粵]` vs 書面語 `[書]` 對照範例

| 輸入 / Input | 口語模式 `[粵]` | 書面語模式 `[書]` |
| :--- | :--- | :--- |
| `keoideihaidoukinggai` | 佢哋喺度傾偈 | 他們在度聊天 |
| `;I go to school by bus` | 我搭巴士返學校 / 我搭巴士返學 | 我乘巴士去學校 / 我坐巴士上學 |
| `lift` | 𨋢（優先） | 升降機 / 電梯（優先） |
| `afternoon` | 晏晝 / 下午 | 下午 / 中午（優先） |
| `hai` / `m` / `ge` | 喺 / 唔 / 嘅 | 在·是 / 不 / 的 |
| `gamjat` / `tingjat` / `dim2gaai2` | 今日 / 聽日 / 點解 | 今天 / 明天 / 為什麼 |

---

## ✨ 核心技術特色 / Core Features

1. **多拼音方案無縫相容 & 懶音容錯 / Multi-Romanization & Fuzzy Pinyin Tolerance**
   - 內部統一正規化為標準粵拼（LSHK Jyutping），自動相容**教院式**、**耶魯拼音**、**劉錫祥拼音**與常見港式拼法（例如 `ch -> c`、`sh -> s`、`oo -> u`、`ts -> c`、`dz -> z`），並支援懶音互換、免聲調與首字母簡拼。
   - *Normalizes LSHK Jyutping, Cantonese Pinyin, Yale, Sidney Lau, and informal HK spellings with fuzzy initial/final rules and toneless/initialism support.*

2. **打英文單字直出中文（雙軌混合檢索）/ English-to-Chinese Reverse Lookup**
   - 同時檢索粵拼詞庫（`cantonese_dict`）與英漢反查詞庫（`english_dict`）；短碼撞碼智慧穿插英文翻譯，長單字與前綴直接置頂。
   - *Simultaneous dual-track lookup across Cantonese and English lexicons, blending short ambiguous codes while prioritizing longer English words and prefixes.*

3. **英文整句直出廣東話 / English Sentence to Cantonese Mode**
   - 內建結構化「港式英文片語與語法槽位字典」（`EnSentenceTranslator`），支援最長片語匹配、廣東話語序重組（將時間、`with` 伴隨、`by` 交通工具與地點自動搬移至動詞前）、進行式／過去式標記，以及未知動賓查詢 `english_dict` 合成；支援獨立 `[譯]` 模式與 `;` / `/` 免切換前綴。
   - *Rule- and slot-grammar-based English-to-Cantonese sentence translator with Cantonese word-order reordering, tense/aspect handling, and dual trigger (`[譯]` mode or `;` / `/` prefix).*

4. **全拼與簡拼混合 Viterbi 長句連打及剝洋蔥式分步選字 / Hybrid Full/Abbr Viterbi Sentence Decoder**
   - 建構兩層式混合 DAG 詞圖（`segment_hybrid_syllables` + Word Lattice）與 Viterbi 動態規劃解碼器，支援全拼、純簡拼、全簡拼混打以及 `"ng"` 歧義展開；支援整句上屏或依 `consumed_chars` 進行步進式部分選字（Partial Commit），單次查詢延遲低於 10ms。
   - *Two-layer hybrid DAG lattice and Viterbi decoder supporting mixed full/abbreviated Cantonese sentences and progressive prefix partial commits under 10ms.*

5. **口語（粵）/ 書面語（書）雙語體即時切換 / Colloquial & Written Style Toggle**
   - 一鍵切換廣東話道地口語與標準書面語，同步作用於粵拼選字、英漢反查優先級、Viterbi 長句解碼與英文整句翻譯；Windows 系統列透過 In-Memory GDI 動態呈現「粵 / 書 / 譯 / 英」狀態圖示。
   - *Instant toggling between Colloquial Cantonese and Standard Written Chinese across all input modes, reflected by dynamic system tray / toolbar indicators.*

6. **智慧個人詞庫與自造詞（`user_dict.db`）/ Smart User Dictionary & Auto-Phrase Learning**
   - 採用唯讀系統詞庫（`cpime_dict.db`）與獨立讀寫個人學習詞庫（`user_dict.db`，開啟 SQLite WAL 模式）雙庫分離架構，提供 Unigram 個人詞頻置頂、Bigram 前後文連帶排序與聯想詞，以及透過 `CANTONESE_STOPWORDS` 虛詞防火牆實現兩階段觀察轉正的自動造詞機制。
   - *Dual-DB separation (`cpime_dict.db` + WAL-mode `user_dict.db`) with Unigram boosting, Bigram contextual prediction, and stopword-guarded two-stage auto-phrase learning.*

7. **完整 UTF-8 / UTF-16 四位元組擴展漢字安全 / Full CJK Extension Character Safety**
   - 完整支援「嘅、喺、咗、唔、啲、𨋢、𥄫、㗎」等香港口語與 CJK 擴展區漢字；Windows 前端透過 `SendInput`（`KEYEVENTF_UNICODE` + UTF-16 Surrogate Pairs）並搭配 `CPIME_EXTRA_INFO` 防自我攔截標記安全上屏。
   - *Full support for 4-byte UTF-8 / UTF-16 surrogate pair HK Cantonese characters across core matching and Win32 Unicode injection.*

---

## 🗂️ 專案架構圖 / Monorepo Layout

```text
cpime/
├── Cargo.toml                                  # Cargo Workspace 設定（core, windows）
├── .github/workflows/
│   ├── ci.yml                                  # 自動執行 Windows 單元/整合測試與 Android APK 建置檢查
│   └── release.yml                             # Tag (v*) 觸發編譯完整詞庫並發布 Windows ZIP 與 Android APK
├── core/                                       # [Rust] cpime-core 核心引擎 (rlib + cdylib)
│   ├── assets/                                 # 詞庫目錄（cpime_dict.db 與 user_dict.db 不進版控）
│   ├── src/
│   │   ├── lib.rs                              # 公開 API 匯出
│   │   ├── phonetics.rs                        # 多拼音正規化、模糊音展開與全/簡拼混合音節切分
│   │   ├── decoder.rs                          # 混合 DAG 詞圖 + Viterbi 長句解碼與部分選字前綴提取
│   │   ├── engine.rs                           # 雙軌混合檢索、混排與口語/書面語轉換核心
│   │   ├── en_sentence.rs                      # 英文整句直出廣東話槽位語法與語序重組引擎
│   │   ├── user_dict.rs                        # user_dict.db (WAL) Unigram / Bigram 管理
│   │   ├── phrase_composer.rs                  # 自造詞滑動視窗與粵語虛詞邊界過濾
│   │   ├── jni_bridge.rs                       # Android JNI 橋接介面 (libcpime_core.so)
│   │   └── bin/repl.rs                         # CLI 互動式測試工具 (repl)
│   └── tests/
│       ├── sentence_decoder_tests.rs           # Viterbi 長句、混合簡拼與步進選字測試
│       ├── en_sentence_tests.rs                # 英文整句直出廣東話測試
│       └── user_learning_tests.rs              # 個人詞庫 Unigram/Bigram 與自造詞測試
├── tools/                                      # [Python] 詞庫編譯與種子資料庫腳本
│   ├── build_full_lexicon.py                   # 下載 rime-cantonese + ECDICT 並編譯完整 cpime_dict.db
│   ├── init_seed_db.py                         # 產生輕量種子詞庫供開發與 CI 測試使用
│   └── requirements.txt                        # Python 依賴清單 (opencc-python-reimplemented)
├── windows/                                    # [Rust] cpime-win Windows 桌面端獨立輸入法前端
│   └── src/
│       ├── main.rs                             # 程式進入點、可攜式詞庫路徑解析與訊息迴圈
│       ├── hook.rs                             # WH_KEYBOARD_LL 全域鍵盤攔截與狀態機
│       ├── overlay.rs                          # WS_EX_NOACTIVATE 無焦點深色浮動候選視窗
│       ├── injector.rs                         # SendInput UTF-16 安全上屏與防自我攔截標記
│       └── tray.rs                             # Shell_NotifyIconW 動態 GDI 圖示（粵/書/譯/英）與右鍵選單
└── android/                                    # [Kotlin / Jetpack Compose] Android 行動端輸入法
    └── app/src/main/java/com/andyhui/cpime/
        ├── MainActivity.kt                     # 啟用引導頁與即時試打測試區
        ├── CantoneseIMEService.kt              # InputMethodService 與 Compose 生命週期綁定
        ├── KeyboardScreen.kt                   # 軟鍵盤 UI（工具列、候選列、QWERTY、標點與符號盤）
        ├── NativeEngine.kt                     # Kotlin JNI 包裝器
        └── AssetHelper.kt                      # APK 內建詞庫解壓與複製助手
```

---

## 🛠️ 開發者從原始碼建置指南 / Build from Source (For Developers)

### 前置需求 / Prerequisites

- **Rust 工具鏈**：穩定版 `rustc` 與 `cargo`（安裝方式：[rustup.rs](https://rustup.rs/)）
- **Python 3.9+**（編譯詞庫用）
- **Git**（`build_full_lexicon.py` 會透過 `git` 下載 `rime-cantonese` 資料源）
- **Android 編譯額外需求（僅編譯 APK 時需要）**：JDK 17、Android SDK / NDK（API 34）、Rust Android targets（`aarch64-linux-android`、`x86_64-linux-android`）與 `cargo-ndk`

### 1. 編譯詞庫 / Build the Lexicon Database

> **提示 / Note**：大型詞庫 `core/assets/cpime_dict.db` 不包含在 Git 儲存庫中，請先在本地執行腳本產生。

安裝 Python 相依套件並編譯完整雙軌詞庫（由 `rime-cantonese` 與 `ECDICT` 建置）：

```bash
pip install -r tools/requirements.txt
python tools/build_full_lexicon.py
```

若僅需進行快速開發或執行測試，可產生輕量種子詞庫：

```bash
python tools/init_seed_db.py
```

### 2. 執行單元與整合測試及 CLI REPL / Run Tests & CLI REPL

```bash
cargo test --workspace
```

使用內建互動式命令列工具（CLI REPL）測試拼字、混合長句、英文反查與英譯粵整句：

```bash
cargo run --bin repl
```

### 3. 編譯與啟動 Windows 桌面端輸入法 / Build & Launch Windows IME

在 Windows 環境下以 Release 模式編譯並啟動 `cpime-win`（或直接執行根目錄的 `run_win_ime.bat`）：

```bash
cargo run --release -p cpime-win
```

### 4. 交叉編譯 Android JNI 與打包 APK / Build Android APK

1. 將編譯好的詞庫複製到 Android assets 目錄：
   ```bash
   mkdir -p android/app/src/main/assets
   cp core/assets/cpime_dict.db android/app/src/main/assets/cpime_dict.db
   ```
2. 安裝 Android targets 與 `cargo-ndk` 後，於 `core/` 目錄下交叉編譯 `libcpime_core.so`：
   ```bash
   rustup target add aarch64-linux-android x86_64-linux-android
   cargo install cargo-ndk
   cd core
   cargo ndk -t arm64-v8a -t x86_64 -o ../android/app/src/main/jniLibs build --release -p cpime-core
   ```
3. 於 `android/` 目錄下使用 Gradle 打包 Debug APK：
   ```bash
   cd ../android
   ./gradlew assembleDebug
   ```
   編譯完成的 APK 位於 `android/app/build/outputs/apk/debug/app-debug.apk`。

### 5. GitHub Actions 自動化發布 / Automated Releases

專案已配置 `.github/workflows/release.yml`：
- 推送版本標籤（例如 `git tag v0.1.0 && git push origin v0.1.0`），或在 GitHub Actions 頁面手動觸發 **`Release Windows & Android Packages`** workflow。
- CI 會自動編譯完整詞庫、建置 Windows Release 可攜式壓縮包與 Android APK，並自動建立 GitHub Release 上傳 **`CPIME-Next-Windows-x64.zip`** 與 **`CPIME-Next-Android-debug.apk`**。

---

## 授權與致謝 / Acknowledgments & License

- **粵語詞彙資料來源**：[rime/rime-cantonese](https://github.com/rime/rime-cantonese)
- **英漢詞彙資料來源**：[skywind3000/ECDICT](https://github.com/skywind3000/ECDICT)
- 致敬經典廣東話拼音輸入法 **CPIME**，讓粵語輸入與英漢混打在現代跨平台環境下繼續傳承。
