#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
CPIME-Next 種子詞庫生成腳本 (init_seed_db.py)
無須外部網路下載，直接建立內建高頻粵語詞庫與英漢反查詞庫的 SQLite 資料庫：
路徑：/core/assets/cpime_dict.db
"""

import os
import sqlite3
from pathlib import Path


def get_Cantonese_seed_data() -> list[tuple[str, str, str, str, int]]:
    """
    回傳高頻粵語字詞清單：
    格式：(word, jyutping, jyutping_spaced, initials, freq)
    """
    raw_entries = [
        # 核心必備與高頻單字 / 詞彙
        ("你", "nei", "nei", "n", 10000),
        ("嘅", "ge", "ge", "g", 9950),
        ("你好", "neihou", "nei hou", "nh", 9920),
        ("喺", "hai", "hai", "h", 9900),
        ("係", "hai", "hai", "h", 9850),
        ("好", "hou", "hou", "h", 9800),
        ("新", "san", "san", "s", 9750),
        ("身", "san", "san", "s", 9650),
        ("文", "man", "man", "m", 9700),
        ("問", "man", "man", "m", 9550),
        ("香港", "hoenggong", "hoeng gong", "hg", 9680),
        ("今日", "gamjat", "gam jat", "gj", 9600),
        ("𨋢", "lip", "lip", "l", 9500),
        ("傾偈", "kinggai", "king gai", "kg", 9400),
        ("食晏", "sikngaan", "sik ngaan", "sn", 9350),
        # 擴展口語字 (含四位元組 UTF-8 漢字)
        ("咗", "zo", "zo", "z", 9600),
        ("唔", "m", "m", "m", 9650),
        ("啲", "di", "di", "d", 9580),
        ("𥄫", "gap", "gap", "g", 8800),
        ("㗎", "gaa", "gaa", "g", 9100),
        ("咩", "me", "me", "m", 9200),
        ("嘢", "je", "je", "j", 9300),
        ("佢", "keoi", "keoi", "k", 9450),
        ("我", "ngo", "ngo", "n", 9900),
        ("哋", "dei", "dei", "d", 9150),
        # 常用代名詞與疑問詞
        ("我哋", "ngodei", "ngo dei", "nd", 9400),
        ("你哋", "neidei", "nei dei", "nd", 9300),
        ("佢哋", "keoidei", "keoi dei", "kd", 9250),
        ("乜嘢", "matje", "mat je", "mj", 9100),
        ("點解", "dimgaai", "dim gaai", "dg", 9200),
        ("邊度", "bindou", "bin dou", "bd", 9150),
        ("幾時", "geisi", "gei si", "gs", 9100),
        # 時間與日常用語
        ("而家", "jigaa", "ji gaa", "jg", 9250),
        ("聽日", "tingjat", "ting jat", "tj", 9200),
        ("尋日", "camjat", "cam jat", "cj", 9100),
        ("朝早", "ziuzou", "ziu zou", "zz", 9050),
        ("晏晝", "ngaanzaau", "ngaan zaau", "nz", 9050),
        ("夜晚", "jemaan", "je maan", "jm", 9000),
        ("多謝", "doze", "do ze", "dz", 9350),
        ("唔該", "mgoi", "m goi", "mg", 9450),
        ("對唔住", "deoimzyu", "deoi m zyu", "dmz", 9000),
        ("冇問題", "moumantai", "mou man tai", "mmt", 9100),
        # 日常生活與動作
        ("返工", "faangung", "faan gung", "fg", 9200),
        ("放工", "fonggung", "fong gung", "fg", 9100),
        ("食飯", "sikfaan", "sik faan", "sf", 9300),
        ("飲茶", "jamcaa", "jam caa", "jc", 9150),
        ("買嘢", "maaije", "maai je", "mj", 9050),
        ("行街", "haanggaai", "haang gaai", "hg", 9000),
        ("睇戲", "taihei", "tai hei", "th", 8950),
        ("鍾意", "zungji", "zung ji", "zj", 9350),
        ("知道", "zidou", "zi dou", "zd", 9250),
        ("覺得", "gokdak", "gok dak", "gd", 9200),
        ("可以", "hoji", "ho ji", "hj", 9300),
        ("應該", "jinggoi", "jing goi", "jg", 9150),
        ("因為", "janwai", "jan wai", "jw", 9100),
        ("所以", "soji", "so ji", "sj", 9100),
        ("不過", "batgwo", "bat gwo", "bg", 9050),
        ("如果", "jyugwo", "jyu gwo", "jg", 9050),
        # 常見名詞與短碼同音字
        ("時間", "sigaan", "si gaan", "sg", 9150),
        ("地方", "deifong", "dei fong", "df", 9100),
        ("工作", "gungzok", "gung zok", "gz", 9100),
        ("生活", "saangwut", "saang wut", "sw", 9050),
        ("朋友", "pangjau", "pang jau", "pj", 9200),
        ("屋企", "ukkei", "uk kei", "uk", 9250),
        ("電話", "dinwaa", "din waa", "dw", 9150),
        ("電腦", "dinnou", "din nou", "dn", 9100),
        ("上網", "soengmong", "soeng mong", "sm", 9000),
        ("搭車", "daapce", "daap ce", "dc", 9000),
        ("地鐵", "deitit", "dei tit", "dt", 9100),
        ("巴士", "baasi", "baa si", "bs", 9100),
        ("的士", "diksi", "dik si", "ds", 9050),
        ("銀包", "nganbaau", "ngan baau", "nb", 8950),
        ("埋單", "maaidan", "maai dan", "md", 9000),
        ("幫手", "bongsau", "bong sau", "bs", 9050),
        ("緊要", "ganjiu", "gan jiu", "gj", 9000),
        ("民", "man", "man", "m", 9100),
        ("蚊", "man", "man", "m", 8900),
        ("神", "san", "san4", "s", 9000),
        ("晨", "san", "san4", "s", 8500),
        ("辛", "san", "san", "s", 8200),
        ("伸", "san", "san", "s", 8000),
        ("食", "sik", "sik6", "s", 9950),
        ("識", "sik", "sik1", "s", 9900),
        ("色", "sik", "sik1", "s", 9800),
        ("萬", "maan", "maan6", "m", 9950),
        ("慢", "maan", "maan6", "m", 9900),
        ("漫", "maan", "maan6", "m", 9850),
        ("晚", "maan", "maan5", "m", 5800),
        ("蠻", "maan", "maan4", "m", 5500),
        ("生", "saang", "saang", "s", 9200),
        ("永", "wing", "wing", "w", 9400),
        ("穎", "wing", "wing", "w", 9300),
        ("榮", "wing", "wing", "w", 9100),
        ("廣東話", "gwongdungwaa", "gwong dung waa", "gdw", 9300),
        ("國語", "gwokjyu", "gwok jyu", "gj", 9100),
        ("開心", "hoisam", "hoi sam", "hs", 9250),
        ("太陽", "taaijoeng", "taai joeng", "tj", 9100),
        ("日", "jat", "jat", "j", 9300),
        ("興", "hing", "hing1", "h", 9600),
        ("如", "jyu", "jyu4", "j", 9600),
        ("做", "zou", "zou6", "z", 9850),
    ]
    return raw_entries


def get_english_seed_data() -> list[tuple[str, str, int]]:
    """
    回傳高頻英漢反查詞庫清單：
    格式：(eng_word, canto_word, priority)
    """
    raw_entries = [
        # 核心指定測試詞彙
        ("sun", "日", 1000),
        ("sun", "太陽", 950),
        ("afternoon", "中午", 1000),
        ("afternoon", "下午", 950),
        ("afternoon", "晏晝", 900),
        ("man", "人", 1000),
        ("man", "男人", 950),
        ("lift", "𨋢", 1000),
        ("lift", "升降機", 950),
        ("strawberry", "士多啤梨", 1000),
        ("strawberry", "草莓", 950),
        ("morning", "朝早", 1000),
        ("morning", "早上", 950),
        # 其他常用英漢/英粵詞彙
        ("wing", "翼", 1000),
        ("wing", "翅膀", 950),
        ("bus", "巴士", 1000),
        ("taxi", "的士", 1000),
        ("cheese", "芝士", 1000),
        ("chocolate", "朱古力", 1000),
        ("coffee", "咖啡", 1000),
        ("tea", "茶", 1000),
        ("apple", "蘋果", 1000),
        ("orange", "橙", 1000),
        ("water", "水", 1000),
        ("lunch", "食晏", 1000),
        ("lunch", "午餐", 950),
        ("dinner", "食晚飯", 1000),
        ("dinner", "晚餐", 950),
        ("night", "夜晚", 1000),
        ("night", "晚上", 950),
        ("today", "今日", 1000),
        ("tomorrow", "聽日", 1000),
        ("yesterday", "尋日", 1000),
        ("hello", "你好", 1000),
        ("thanks", "唔該", 1000),
        ("thanks", "多謝", 950),
        ("sorry", "對唔住", 1000),
        ("friend", "朋友", 1000),
        ("home", "屋企", 1000),
        ("work", "返工", 1000),
        ("work", "工作", 950),
        ("phone", "電話", 1000),
        ("computer", "電腦", 1000),
    ]
    return raw_entries


def build_seed_database(db_path: Path) -> None:
    """建立並寫入 cpime_dict.db SQLite 資料庫"""
    db_path.parent.mkdir(parents=True, exist_ok=True)
    if db_path.exists():
        db_path.unlink()

    conn = sqlite3.connect(str(db_path))
    try:
        cursor = conn.cursor()
        # 設定 UTF-8 與效能最佳化
        cursor.execute("PRAGMA encoding = 'UTF-8';")
        cursor.execute("PRAGMA journal_mode = WAL;")
        cursor.execute("PRAGMA synchronous = NORMAL;")

        # 建立 cantonese_dict 資料表
        cursor.execute(
            """
            CREATE TABLE cantonese_dict (
                zh_word TEXT NOT NULL,
                jyutping_toned TEXT NOT NULL,
                toneless TEXT NOT NULL,
                abbr TEXT NOT NULL,
                weight INTEGER NOT NULL,
                PRIMARY KEY (zh_word, jyutping_toned)
            );
            """
        )

        # 建立 english_dict 資料表
        cursor.execute(
            """
            CREATE TABLE english_dict (
                en_word TEXT NOT NULL,
                zh_word TEXT NOT NULL,
                is_colloquial INTEGER DEFAULT 0,
                weight INTEGER NOT NULL,
                PRIMARY KEY (en_word, zh_word)
            );
            """
        )

        canto_data = [
            (w, toned, toneless, abbr, freq)
            for (w, toneless, toned, abbr, freq) in get_Cantonese_seed_data()
        ]
        cursor.executemany(
            """
            INSERT INTO cantonese_dict (zh_word, jyutping_toned, toneless, abbr, weight)
            VALUES (?, ?, ?, ?, ?);
            """,
            canto_data,
        )

        eng_data = [
            (eng, canto, 1, prio)
            for (eng, canto, prio) in get_english_seed_data()
        ]
        cursor.executemany(
            """
            INSERT INTO english_dict (en_word, zh_word, is_colloquial, weight)
            VALUES (?, ?, ?, ?);
            """,
            eng_data,
        )

        # 建立索引以確保單次查詢 < 1ms
        cursor.execute(
            "CREATE INDEX idx_yue_toneless ON cantonese_dict(toneless, weight DESC);"
        )
        cursor.execute(
            "CREATE INDEX idx_yue_abbr ON cantonese_dict(abbr, weight DESC);"
        )
        cursor.execute(
            "CREATE INDEX idx_en_lookup ON english_dict(en_word, weight DESC);"
        )

        conn.commit()
        print(
            f"[OK] 成功建立種子資料庫：{db_path}\n"
            f"     - cantonese_dict 收錄筆數：{len(canto_data)} 筆\n"
            f"     - english_dict 收錄筆數：{len(eng_data)} 筆"
        )
    finally:
        conn.close()


if __name__ == "__main__":
    project_root = Path(__file__).resolve().parent.parent
    target_db = project_root / "core" / "assets" / "cpime_dict.db"
    build_seed_database(target_db)

