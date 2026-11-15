# CPIME-Next

**跨平台現代化廣東話及英漢反查雙軌輸入法**  
*A Modern Cross-Platform Cantonese & English-Lookup Dual-Track Input Method Engine*

致敬經典 CPIME（Cantonese Pinyin Input Method Editor），以高效能 **Rust** 核心引擎（`cpime-core`）驅動，結合 Python 詞庫編譯器、Windows 獨立浮動候選視窗桌面前端（`cpime-win`），並為未來 Android 行動端保留跨平台架構。

Built in tribute to the classic CPIME, **CPIME-Next** pairs a high-performance **Rust** core engine (`cpime-core`) with a Python dual-track lexicon builder and a native Windows standalone overlay IME (`cpime-win`), with an extensible monorepo layout designed for future Android support.

---

## 核心特色 / Key Features

1. **多拼音方案無縫相容 & 懶音容錯 / Multi-Romanization & Fuzzy Pinyin Tolerance**
   - 內部統一正規化為標準粵拼（LSHK Jyutping），自動相容**教院式（Cantonese Pinyin）**、**耶魯拼音（Yale）**、**劉錫祥拼音（Sidney Lau）**與常見港式拼法（例如 `ch -> c`, `sh -> s`, `oo -> u`, `ts -> c`, `dz -> z`）。
   - 支援常見懶音與模糊音容錯：`n / l` 聲母互換、`ng / 零聲母`互換、`gw / g` 與 `kw / k` 互換、`-n / -ng` 與 `-t / -k` 韻尾互換。
   - 預設免打聲調，並支援首字母簡拼（如輸入 `nh` 即可匹配「你好」）。
   - *Normalizes LSHK Jyutping, Cantonese Pinyin, Yale, Sidney Lau, and HK informal spellings seamlessly; supports fuzzy initial/final rules, toneless input, and initialism abbreviations.*

2. **打英文直出中文（雙軌混合檢索）/ English-to-Chinese Reverse Lookup**
   - 同時檢索粵拼詞庫（`cantonese_dict`）與英漢反查詞庫（`english_dict`）。
   - **短碼撞碼智慧穿插**：如輸入 `sun` 顯示 `1.新 2.身 3.日 4.太陽`，兼顧粵拼與英文直譯。
   - **長單字或純英文直排第一**：如輸入 `afternoon` 直接出「中午 / 下午 / 晏晝」、輸入 `lift` 出「𨋢 / 升降機」、支援前綴匹配（如 `aftern`）。
   - *Simultaneous dual-track querying across Cantonese and English lexicons, blending English translations into short ambiguous codes while prioritizing long English words and prefixes.*

3. **智慧個人詞庫與自造詞（`user_dict.db`）/ Smart User Dictionary & Auto-Phrase Learning**
   - 採用唯讀系統詞庫（`cpime_dict.db`）與獨立讀寫個人詞庫（`user_dict.db`，開啟 SQLite WAL 模式）雙庫分離架構。
   - **Unigram 選字即置頂**：無論透過粵拼、簡拼或英文反查選字上屏，即時累積個人詞頻與最近使用時間權重。
   - **Bigram 前後文語境連帶**：追蹤上一上屏詞彙，依 `(prev_word, current_input)` 動態置頂最符合語境的下一詞。
   - **粵語虛詞防火牆自動造詞**：透過 `CompositionBuffer` 滑動視窗與 `CANTONESE_STOPWORDS` 語法邊界過濾，經兩階段轉正機制自動將常用 2–4 字連續單字合成為個人新詞。
   - *Dual-DB separation (`cpime_dict.db` read-only + `user_dict.db` in WAL mode) with Unigram recency/frequency boost, Bigram contextual prediction, and stopword-guarded two-stage auto-phrase learning.*

4. **Viterbi 長句連打與剝洋蔥式分步選字 / Viterbi Sentence Decoder & Progressive Partial Commit**
   - 建構兩層式有向無環詞圖（Syllable DAG + Word Lattice），結合 Viterbi 動態規劃演算法進行全句連打解碼（同時保護長英文單字不誤切分）。
   - 支援第 1 順位整句直接上屏，或選用第 2 順位起的開頭前綴詞進行「剝洋蔥式」分步上屏（Partial Commit），自動切除已消耗的原始拼音並即時重新解碼剩餘字串。
   - *Two-layer DAG lattice construction with Viterbi decoding for continuous sentence input, supporting both full-sentence commit and progressive prefix partial commits.*

5. **口語（粵）/ 書面語（書）雙模式切換與 Windows 系統列圖示 / Colloquial & Written Style Toggle + System Tray**
   - 支援在「粵語口語模式（如：嘅、喺、唔、𨋢）」與「標準書面語模式（如：的、在、不、升降機）」之間即時切換，英漢反查與高頻粵語詞彙會自動調整排序與轉換。
   - Windows 系統列（System Tray）採用 In-Memory GDI 動態產生狀態圖示（藍底「粵」、綠底「書」、灰底「英」），支援左鍵點擊切換中英與右鍵功能選單。
   - *Instant toggling between Colloquial Cantonese and Standard Written Chinese output styles, reflected by dynamic in-memory GDI tray icons (`粵`, `書`, `英`).*

6. **完整 UTF-8 / UTF-16 四位元組擴展漢字安全 / Full CJK Extension Character Safety**
   - 完整支援「嘅、喺、咗、唔、啲、𨋢、𥄫、㗎」等香港口語與 CJK 擴展區漢字；Windows 前端採用 UTF-16 Surrogate Pairs 透過 `SendInput` (`KEYEVENTF_UNICODE`) 安全上屏，不亂碼、不掉字。
   - *Full support for 4-byte UTF-8 / UTF-16 surrogate pair HK Cantonese characters across core matching and Win32 Unicode injection.*

---

## 專案架構圖 / Monorepo Layout

```text
cpime/
├── Cargo.toml                  # Cargo Workspace 設定（core, windows）
├── core/                       # [Rust] cpime-core 核心引擎
│   ├── assets/                 # 詞庫目錄（cpime_dict.db 與 user_dict.db 不進版控）
│   ├── src/
│   │   ├── lib.rs              # 公開 API 匯出
│   │   ├── phonetics.rs        # 多拼音正規化、模糊音展開與音節切分
│   │   ├── decoder.rs          # Syllable DAG + Word Lattice + Viterbi 長句解碼器
│   │   ├── engine.rs           # 雙軌混合檢索、混排與書面語轉換核心
│   │   ├── user_dict.rs        # user_dict.db (WAL) Unigram / Bigram 管理
│   │   ├── phrase_composer.rs  # 自造詞滑動視窗與粵語虛詞邊界過濾
│   │   └── bin/repl.rs         # CLI 互動式測試工具 (repl)
│   └── tests/                  # 整合測試（長句解碼、個人詞庫學習）
├── tools/                      # [Python] 詞庫編譯與種子資料庫腳本
│   ├── build_full_lexicon.py   # 下載 rime-cantonese + ECDICT 並編譯完整 cpime_dict.db
│   ├── init_seed_db.py         # 產生輕量種子詞庫供開發與 CI 測試使用
│   └── requirements.txt        # Python 依賴清單 (opencc-python-reimplemented)
├── windows/                    # [Rust] cpime-win Windows 桌面端獨立輸入法前端
│   └── src/
│       ├── main.rs             # 程式進入點與訊息迴圈
│       ├── hook.rs             # WH_KEYBOARD_LL 全域鍵盤攔截與狀態機
│       ├── overlay.rs          # WS_EX_NOACTIVATE 無焦點深色浮動候選視窗
│       ├── injector.rs         # SendInput UTF-16 安全上屏與防自我攔截標記
│       └── tray.rs             # Shell_NotifyIconW 動態 GDI 圖示與右鍵選單
└── android/                    # [Planned] Android 行動端輸入法前端（架構預留）
```

| 目錄 / Directory | 技術棧 / Stack | 職責說明 / Responsibility |
| :--- | :--- | :--- |
| `/core` | Rust (`cpime-core`) | 多制式粵拼正規化、模糊音展開、SQLite 雙軌檢索、Viterbi 長句解碼與個人智慧學習引擎 |
| `/tools` | Python | 從 `rime-cantonese` 與 `ECDICT` 編譯完整 `cpime_dict.db`，或產生輕量測試種子資料庫 |
| `/windows` | Rust (`cpime-win`) | Windows 桌面端獨立輸入法前端（低階鍵盤 Hook、無焦點浮動視窗、System Tray 圖示與 Unicode 上屏） |
| `/android` | Android (預留 / Planned) | 跨平台共用 `cpime-core` 的 Android 行動端輸入法前端 |

---

## 快速開始與建置指南 / Quick Start

### 前置需求 / Prerequisites
- **Rust 工具鏈**：建議使用穩定版 `rustc` 與 `cargo`（安裝方式：[rustup.rs](https://rustup.rs/)）
- **Python 3.9+**（編譯詞庫用）
- **Git**（`build_full_lexicon.py` 會自動透過 `git` 下載 `rime-cantonese` 資料源）

### 1. 編譯詞庫 / Build the Lexicon Database

> **提示 / Note**：大型詞庫 `core/assets/cpime_dict.db` 不包含在 Git 儲存庫中，請先在本地執行腳本產生。

安裝 Python 相依套件並編譯完整雙軌詞庫（約包含 `rime-cantonese` 與 `ECDICT` 完整資料）：

```bash
pip install -r tools/requirements.txt
python tools/build_full_lexicon.py
```

若僅需進行快速開發或 CI 測試，也可先產生輕量種子詞庫：

```bash
python tools/init_seed_db.py
```

### 2. 執行核心單元與整合測試 / Run Core Tests

```bash
cargo test
```

若需使用內建的互動式命令列工具（CLI REPL）測試拼字、長句與英漢反查結果：

```bash
cargo run --bin repl
```

### 3. 啟動 Windows 桌面端輸入法 / Launch Windows Overlay IME

在 Windows 環境下以 Release 模式編譯並啟動 `cpime-win`（或直接執行專案根目錄的 `run_win_ime.bat`）：

```bash
cargo run --release -p cpime-win
```

啟動後會在系統工作列右下方顯示狀態圖示（**藍底「粵」** / **綠底「書」** / **灰底「英」**），可在記事本、瀏覽器、VS Code 等任意 Windows 視窗中直接輸入。

### Windows 快捷鍵一覽 / Windows Shortcuts

| 按鍵 / Shortcut | 功能說明 / Action |
| :--- | :--- |
| `Left Shift`（單按放開）或 `Ctrl + Space` | 切換 **廣東話模式（粵）** / **英文直通模式（英）** |
| `Ctrl + Shift + S` 或 `F4` | 切換 **口語模式（粵）** / **書面語模式（書）** |
| `a - z` | 輸入粵拼、簡拼或英文反查字串 |
| `Space` | 選取當頁第 1 個候選詞並上屏 |
| `1 - 9` | 選取當頁對應序號（1–9）的候選詞上屏（長句時支援前綴分步上屏） |
| `Enter` | 將目前輸入的原始英文字母直接上屏並清空緩衝區 |
| `-` / `=` 或 `PageUp` / `PageDown` 或 `↑` / `↓` | 候選詞清單上一頁 / 下一頁翻頁 |
| `Backspace` | 刪除最後一個輸入字母（刪空則隱藏候選視窗） |
| `Esc` | 取消目前輸入並清空緩衝區 |

---

## 授權與致謝 / Acknowledgments & License

- **粵語詞彙資料來源**：[rime/rime-cantonese](https://github.com/rime/rime-cantonese)
- **英漢詞彙資料來源**：[skywind3000/ECDICT](https://github.com/skywind3000/ECDICT)
- 致敬經典廣東話拼音輸入法 **CPIME**，讓粵語輸入與英漢混打在現代跨平台環境下繼續傳承。
