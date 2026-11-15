#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
CPIME-Next 完整雙軌詞庫編譯器 (rime-cantonese + ECDICT -> cpime_dict.db)
"""

import csv
import re
import sqlite3
import subprocess
import urllib.request
from pathlib import Path
from opencc import OpenCC

# 1. 路徑與資料源設定
BASE_DIR = Path(__file__).resolve().parent
CACHE_DIR = BASE_DIR / "cache"
RIME_DIR = CACHE_DIR / "rime-cantonese"
ECDICT_CSV = CACHE_DIR / "ecdict.csv"
DB_PATH = BASE_DIR.parent / "core" / "assets" / "cpime_dict.db"

RIME_REPO_URL = "https://github.com/rime/rime-cantonese.git"
ECDICT_URL = "https://raw.githubusercontent.com/skywind3000/ECDICT/master/ecdict.csv"

cc = OpenCC("s2hk")

# 2. 廣東話核心高頻字詞保底權重 (確保最常用口語與單字永遠排在第一頁最前面)
CORE_CANTONESE_BOOST = {
    ("嘅", "ge3"): 10000,
    ("喺", "hai2"): 10000,
    ("係", "hai6"): 10000,
    ("唔", "m4"): 10000,
    ("咗", "zo2"): 10000,
    ("啲", "di1"): 10000,
    ("咁", "gam3"): 9950,
    ("咁", "gam2"): 9950,
    ("佢", "keoi5"): 10000,
    ("我", "ngo5"): 10000,
    ("你", "nei5"): 10000,
    ("好", "hou2"): 9950,
    ("哋", "dei6"): 9950,
    ("咩", "me1"): 9950,
    ("嘢", "je5"): 9950,
    ("𨋢", "lip1"): 9950,
    ("𥄫", "gap6"): 9900,
    ("㗎", "gaa3"): 9950,
    ("今日", "gam1 jat6"): 10000,
    ("聽日", "ting1 jat6"): 10000,
    ("尋日", "cam4 jat6"): 9950,
    ("而家", "ji4 gaa1"): 10000,
    ("香港", "hoeng1 gong2"): 10000,
    ("你好", "nei5 hou2"): 10000,
    ("多謝", "do1 ze6"): 10000,
    ("唔該", "m4 goi1"): 10000,
    ("傾偈", "king1 gai2"): 9950,
    ("食晏", "sik6 ngaan3"): 9950,
    ("埋單", "maai4 daan1"): 9950,
    ("返工", "faan1 gung1"): 9950,
    ("收工", "sau1 gung1"): 9950,
    ("邊個", "bin1 go3"): 9950,
    ("點解", "dim2 gaai2"): 9950,
    ("邊度", "bin1 dou6"): 9950,
    ("新", "san1"): 9950,
    ("身", "san1"): 9900,
    ("神", "san4"): 9000,
    ("晨", "san4"): 8500,
    ("辛", "san1"): 8200,
    ("伸", "san1"): 8000,
    ("文", "man4"): 9950,
    ("問", "man6"): 9900,
    ("食", "sik6"): 9950,
    ("識", "sik1"): 9900,
    ("色", "sik1"): 9800,
    ("式", "sik1"): 9500,
    ("萬", "maan6"): 9950,
    ("慢", "maan6"): 9900,
    ("漫", "maan6"): 9850,
    ("晚", "maan5"): 5800,
    ("蠻", "maan4"): 5500,
    ("興", "hing1"): 9600,
    ("如", "jyu4"): 9600,
    ("做", "zou6"): 9850,
    ("有", "jau5"): 9800,
    ("冇", "mou5"): 9800,
    ("去", "heoi3"): 9800,
    ("嚟", "lai4"): 9800,
    ("睇", "tai2"): 9800,
    ("要", "jiu3"): 9750,
    ("會", "wui5"): 9750,
    ("就", "zau6"): 9750,
    ("都", "dou1"): 9750,
    ("仲", "zung6"): 9700,
    ("又", "jau6"): 9700,
    ("同", "tung4"): 9700,
    ("和", "wo4"): 9650,
    ("與", "jyu5"): 9500,
    ("但", "daan6"): 9650,
    ("個", "go3"): 9800,
    ("隻", "zek3"): 9650,
    ("件", "gin6"): 9650,
    ("次", "ci3"): 9650,
    ("呢", "ne1"): 9750,
    ("嗰", "go2"): 9750,
    ("邊", "bin1"): 9700,
    ("啦", "laa1"): 9750,
    ("囉", "lo1"): 9700,
    ("喎", "wo3"): 9700,
    ("啫", "ze1"): 9650,
    ("吖", "aa1"): 9650,
    ("呀", "aa4"): 9700,
    ("吧", "baa6"): 9600,
    ("嗎", "maa1"): 9600,
}

# 3. 香港道地口語與 CPIME 英漢反查高頻覆蓋表
HK_COLLOQUIAL_OVERRIDES = {
    "sun": [("日", 0, 10000), ("太陽", 0, 9900), ("星期日", 0, 9500)],
    "moon": [("月", 0, 10000), ("月亮", 0, 9900)],
    "morning": [("朝早", 1, 10000), ("早上", 0, 9900), ("上午", 0, 9800)],
    "afternoon": [("中午", 0, 10000), ("下午", 0, 9950), ("晏晝", 1, 9900)],
    "night": [("夜晚", 1, 10000), ("晚上", 0, 9900), ("晚黑", 1, 9800)],
    "tomorrow": [("聽日", 1, 10000), ("明天", 0, 9900), ("明日", 0, 9800)],
    "tmr": [("聽日", 1, 10000), ("明天", 0, 9900)],
    "yesterday": [("尋日", 1, 10000), ("琴日", 1, 9950), ("昨天", 0, 9900)],
    "now": [("而家", 1, 10000), ("依家", 1, 9950), ("現在", 0, 9900)],
    "man": [("人", 0, 10000), ("男人", 0, 9900)],
    "woman": [("女人", 0, 10000), ("婦女", 0, 9800)],
    "wing": [("翼", 0, 10000), ("翅膀", 0, 9900)],
    "lift": [("𨋢", 1, 10000), ("升降機", 0, 9950), ("電梯", 0, 9900)],
    "elevator": [("𨋢", 1, 10000), ("升降機", 0, 9950), ("電梯", 0, 9900)],
    "bus": [("巴士", 0, 10000), ("公車", 0, 9500)],
    "taxi": [("的士", 0, 10000), ("計程車", 0, 9500)],
    "office": [("寫字樓", 1, 10000), ("辦公室", 0, 9950)],
    "strawberry": [("士多啤梨", 1, 10000), ("草莓", 0, 9900)],
    "cherry": [("車厘子", 1, 10000), ("櫻桃", 0, 9900)],
    "cheese": [("芝士", 0, 10000), ("起司", 0, 9500)],
    "butter": [("牛油", 0, 10000), ("奶油", 0, 9500)],
    "toast": [("多士", 1, 10000), ("吐司", 0, 9800)],
    "chocolate": [("朱古力", 1, 10000), ("巧克力", 0, 9900)],
    "lunch": [("食晏", 1, 10000), ("午餐", 0, 9950), ("午飯", 0, 9900)],
    "bill": [("埋單", 1, 10000), ("帳單", 0, 9900)],
    "schedule": [("時間表", 0, 10000), ("行程", 0, 9950), ("排期", 1, 9900)],
    "chat": [("傾偈", 1, 10000), ("聊天", 0, 9950)],
    "sleep": [("瞓覺", 1, 10000), ("睡覺", 0, 9950)],
    "look": [("睇", 1, 10000), ("𥄫", 1, 9800), ("看", 0, 9950)],
}

# 正規表達式工具
TONE_DIGIT_RE = re.compile(r"[1-6]")
VALID_JYUTPING_RE = re.compile(r"^[a-z1-6\s]+$")
POS_PREFIX_RE = re.compile(
    r"^(?:n|v|vt|vi|adj|adv|prep|conj|pron|art|num|int|aux|pl|abbr)\.\s*",
    re.IGNORECASE,
)
BRACKET_RE = re.compile(r"(\[.*?\]|【.*?】|\(.*?\)|（.*?）|<.*?>)")
SPLIT_DELIM_RE = re.compile(r"[,，;；/、]+")
PURE_ALPHA_RE = re.compile(r"^[a-z]{2,20}$")
CJK_ONLY_RE = re.compile(r"^[\u3400-\u4dbf\u4e00-\u9fff\U00020000-\U000323af]+$")


def ensure_sources_downloaded():
    """下載 rime-cantonese 與 ECDICT 至本地快取"""
    CACHE_DIR.mkdir(parents=True, exist_ok=True)

    if not RIME_DIR.exists():
        print("[*] 正在透過 git clone 下載 rime-cantonese 粵語詞庫...")
        subprocess.run(
            ["git", "clone", "--depth", "1", RIME_REPO_URL, str(RIME_DIR)],
            check=True,
        )
    else:
        print(f"[*] 發現本地 rime-cantonese 快取: {RIME_DIR}")

    if not ECDICT_CSV.exists():
        print("[*] 正在下載 ECDICT 英漢詞庫 (約 60MB)...")
        urllib.request.urlretrieve(ECDICT_URL, ECDICT_CSV)
    else:
        print(f"[*] 發現本地 ECDICT 快取: {ECDICT_CSV}")


def normalize_rime_weight(raw_weight: str, word_len: int, zh_word: str = "") -> int:
    """
    將 Rime 詞庫中的多樣化權重（百分比 50%、整數詞頻、或空白無權重）
    統一正規化為 1000 ~ 9000 的整數分數
    """
    raw_weight = raw_weight.strip()
    if not raw_weight:
        # 雙字與三字常用詞在 rime-cantonese 常省略權重，給予合理基礎分
        if word_len == 1:
            # 區分基本漢字區 (U+4E00..U+9FFF) 與 CJK 擴展罕用字 (如 Ext-A U+3400..U+4DBF)
            if zh_word and 0x4E00 <= ord(zh_word[0]) <= 0x9FFF:
                return 4500
            return 2500
        elif word_len == 2:
            return 6200
        elif word_len == 3:
            return 5800
        else:
            return 5200

    if raw_weight.endswith("%"):
        try:
            pct = float(raw_weight[:-1])
            if pct <= 0:
                return 0  # 0% 代表罕用異讀音，過濾或降權
            return int(4000 + (min(pct, 100.0) / 100.0) * 4500)
        except ValueError:
            return 4000

    try:
        val = float(raw_weight)
        if val <= 0:
            return 0
        # 將原始頻率對數或分段映射至 2000 ~ 8800
        if val >= 50000:
            return 8800
        elif val >= 10000:
            return 8000
        elif val >= 1000:
            return 7000
        elif val >= 100:
            return 5500
        else:
            return 3500
    except ValueError:
        return 4500


def parse_rime_cantonese() -> dict:
    """
    解析 rime-cantonese 目錄下所有 *.dict.yaml 檔案
    回傳 {(zh_word, jyutping_toned): (toneless, abbr, weight)}
    """
    cantonese_records = {}
    yaml_files = sorted(RIME_DIR.glob("*.dict.yaml"))

    print(f"[*] 找到 {len(yaml_files)} 個 Rime 粵語詞庫檔案，開始解析...")
    for yaml_file in yaml_files:
        # 跳過符號表或純異體字映射表
        if "symbols" in yaml_file.name or "maps" in yaml_file.name:
            continue

        in_body = False
        with open(yaml_file, mode="r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not in_body:
                    if line == "...":
                        in_body = True
                    continue

                if not line or line.startswith("#"):
                    continue

                parts = line.split("\t")
                if len(parts) < 2:
                    continue

                zh_word = parts[0].strip()
                jyutping_toned = parts[1].strip().lower()
                raw_weight = parts[2].strip() if len(parts) >= 3 else ""

                if not zh_word or not VALID_JYUTPING_RE.match(jyutping_toned):
                    continue

                # 排除「食晏」零聲母俗標 sik6 aan3，保留正音 sik6 ngaan3 供核心引擎懶音容錯推導
                if zh_word == "食晏" and jyutping_toned == "sik6 aan3":
                    continue

                # 計算正規化權重
                char_count = len(zh_word)
                weight = normalize_rime_weight(raw_weight, char_count, zh_word)
                if weight == 0:
                    continue

                # 提取無聲調全拼 (例如 "gam1 jat6" -> "gamjat")
                syllables = jyutping_toned.split()
                clean_syllables = [TONE_DIGIT_RE.sub("", s) for s in syllables]
                clean_syllables = [s for s in clean_syllables if s]
                if not clean_syllables:
                    continue

                toneless = "".join(clean_syllables)
                # 提取首字母簡拼 (例如 "gam1 jat6" -> "gj")
                abbr = "".join(s[0] for s in clean_syllables)

                key = (zh_word, jyutping_toned)
                if key not in cantonese_records or weight > cantonese_records[key][2]:
                    cantonese_records[key] = (toneless, abbr, weight)

    # 覆蓋核心高頻字詞保底權重
    for (zh_word, jyutping_toned), boost_weight in CORE_CANTONESE_BOOST.items():
        syllables = [TONE_DIGIT_RE.sub("", s) for s in jyutping_toned.split()]
        toneless = "".join(syllables)
        abbr = "".join(s[0] for s in syllables)
        cantonese_records[(zh_word, jyutping_toned)] = (toneless, abbr, boost_weight)

    print(f"[+] Rime 粵語詞庫解析完成，共 {len(cantonese_records):,} 條有效詞條。")
    return cantonese_records


def clean_en_translation(raw_trans: str) -> list[str]:
    """清洗 ECDICT 中文釋義並轉為香港繁體"""
    if not raw_trans:
        return []
    candidates = []
    lines = raw_trans.replace("\\n", "\n").splitlines()
    for line in lines:
        line = line.strip()
        if not line or line.startswith("["):
            continue
        line = POS_PREFIX_RE.sub("", line)
        line = BRACKET_RE.sub("", line)
        for part in SPLIT_DELIM_RE.split(line):
            word = part.strip()
            if word.endswith("的") and len(word) > 2:
                continue
            if any(stop in word for stop in ("某人", "某物", "之複數", "過去式", "分詞")):
                continue
            hk_word = cc.convert(word)
            if 1 <= len(hk_word) <= 6 and CJK_ONLY_RE.match(hk_word):
                if hk_word not in candidates:
                    candidates.append(hk_word)
    return candidates[:4]


def calculate_en_weight(row: dict) -> int:
    """計算 ECDICT 英文單字基礎權重"""
    oxford = 1 if row.get("oxford") == "1" else 0
    collins = int(row.get("collins") or 0)
    bnc = int(row.get("bnc") or 0)
    frq = int(row.get("frq") or 0)
    if not oxford and collins == 0 and (bnc == 0 or bnc > 50000) and (frq == 0 or frq > 50000):
        return 0
    score = 3000 + (2500 if oxford else 0) + (collins * 500)
    best_rank = min([r for r in (bnc, frq) if r > 0], default=50000)
    if best_rank <= 50000:
        score += int((50000 - best_rank) / 50000 * 1500)
    return min(score, 8900)


def parse_ecdict() -> dict:
    """解析 ECDICT 並結合香港道地口語覆蓋表"""
    en_records = {}
    for en_word, items in HK_COLLOQUIAL_OVERRIDES.items():
        for zh_word, is_colloq, weight in items:
            en_records[(en_word.lower(), zh_word)] = (is_colloq, weight)

    excluded_homographs = {"nh", "lei", "nei", "hai", "hou", "gam", "jat", "sik"}

    print("[*] 正在清洗並轉換 ECDICT 英漢詞條 (OpenCC s2hk)...")
    with open(ECDICT_CSV, mode="r", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            word = (row.get("word") or "").strip().lower()
            if not PURE_ALPHA_RE.match(word) or word in excluded_homographs:
                continue
            base_weight = calculate_en_weight(row)
            if base_weight == 0:
                continue
            for idx, zh_word in enumerate(clean_en_translation(row.get("translation", ""))):
                key = (word, zh_word)
                if key not in en_records:
                    en_records[key] = (0, max(1000, base_weight - (idx * 150)))

    print(f"[+] 英漢反查詞庫解析完成，共 {len(en_records):,} 條有效詞條。")
    return en_records


def compile_sqlite(cantonese_records: dict, en_records: dict):
    """將粵語詞庫與英漢詞庫寫入同一個 SQLite 並建立高速索引"""
    DB_PATH.parent.mkdir(parents=True, exist_ok=True)
    if DB_PATH.exists():
        DB_PATH.unlink()
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.cursor()

    cursor.execute("PRAGMA synchronous = OFF;")
    cursor.execute("PRAGMA journal_mode = MEMORY;")

    # 1. 建立廣東話詞庫表
    cursor.execute("DROP TABLE IF EXISTS cantonese_dict;")
    cursor.execute("""
        CREATE TABLE cantonese_dict (
            zh_word TEXT NOT NULL,
            jyutping_toned TEXT NOT NULL,
            toneless TEXT NOT NULL,
            abbr TEXT NOT NULL,
            weight INTEGER NOT NULL,
            PRIMARY KEY (zh_word, jyutping_toned)
        );
    """)

    cantonese_batch = [
        (zh, toned, toneless, abbr, w)
        for (zh, toned), (toneless, abbr, w) in cantonese_records.items()
    ]
    cursor.executemany(
        "INSERT INTO cantonese_dict (zh_word, jyutping_toned, toneless, abbr, weight) VALUES (?, ?, ?, ?, ?);",
        cantonese_batch,
    )

    # 2. 建立英漢反查詞庫表
    cursor.execute("DROP TABLE IF EXISTS english_dict;")
    cursor.execute("""
        CREATE TABLE english_dict (
            en_word TEXT NOT NULL,
            zh_word TEXT NOT NULL,
            is_colloquial INTEGER DEFAULT 0,
            weight INTEGER NOT NULL,
            PRIMARY KEY (en_word, zh_word)
        );
    """)

    en_batch = [
        (en_w, zh_w, is_col, w)
        for (en_w, zh_w), (is_col, w) in en_records.items()
    ]
    cursor.executemany(
        "INSERT INTO english_dict (en_word, zh_word, is_colloquial, weight) VALUES (?, ?, ?, ?);",
        en_batch,
    )

    # 3. 建立 B-Tree 複合索引與統計優化
    print("[*] 正在建立高速 B-Tree 索引並執行 VACUUM 壓縮...")
    cursor.execute("CREATE INDEX idx_yue_toneless ON cantonese_dict (toneless, weight DESC);")
    cursor.execute("CREATE INDEX idx_yue_abbr ON cantonese_dict (abbr, weight DESC);")
    cursor.execute("CREATE INDEX idx_en_lookup ON english_dict (en_word, weight DESC);")

    conn.commit()
    cursor.execute("PRAGMA journal_mode = DELETE;")
    cursor.execute("ANALYZE;")
    cursor.execute("VACUUM;")
    conn.close()

    size_mb = DB_PATH.stat().st_size / (1024 * 1024)
    print(f"[+] 編譯大功告成！資料庫路徑: {DB_PATH} (大小: {size_mb:.2f} MB)")


if __name__ == "__main__":
    ensure_sources_downloaded()
    yue_data = parse_rime_cantonese()
    en_data = parse_ecdict()
    compile_sqlite(yue_data, en_data)
