//! CPIME-Next 終端機互動測試工具 (`cargo run --bin repl`)
//!
//! 讓使用者在終端機直接輸入粵拼（支援多拼音方案、懶音、簡拼）或英文，
//! 即時印出排序後的 1~9 候選詞清單、來源標籤（粵拼 / 懶音 / 英文反查 / 前後字連帶 / 個人詞庫）與查詢耗時。
//! 支援：
//! - 輸入 `1~9` 選擇對應候選詞上屏，自動記錄個人詞頻與 Bigram 上下文，並顯示目前已打出的句子 (Context)
//! - 輸入 `:clear` 清空上下文與目前句子
//! - 輸入 `:predict` 查看基於上一個上屏字的 Bigram 聯想詞

use std::io::{self, Write};
use std::time::Instant;

use cpime_core::CpimeEngine;

fn main() {
    let db_path = CpimeEngine::default_db_path();
    let user_db_path = CpimeEngine::default_user_db_path();
    let mut engine = match CpimeEngine::open_with_user_dict(&db_path, &user_db_path) {
        Ok(eng) => eng,
        Err(err) => {
            eprintln!(
                "[錯誤] 無法開啟詞庫資料庫 ({}): {}\n請先執行 `python tools/init_seed_db.py` 建立詞庫。",
                db_path.display(),
                err
            );
            std::process::exit(1);
        }
    };

    println!("==============================================================");
    println!("  CPIME-Next 互動式核心驗證工具 (Dual-Track + User Learning)");
    println!("  系統詞庫: {}", db_path.display());
    println!("  個人詞庫: {}", user_db_path.display());
    println!("  測試範例: sun | san | sik -> maan | afternoon | lei | nh | lift");
    println!("  互動操作:");
    println!("    - 輸入拼音/英文查詢候選字，接著輸入 1~9 將候選字上屏並學習");
    println!("    - 輸入 :clear   清空目前已打出的句子與上下文狀態");
    println!("    - 輸入 :predict 查看目前基於前字的 Bigram 聯想字");
    println!("    - 輸入 :q / exit / quit 結束程式");
    println!("==============================================================\n");

    let stdin = io::stdin();
    let mut input_buf = String::new();
    let mut last_query: Option<String> = None;
    let mut committed_sentence = String::new();

    loop {
        if committed_sentence.is_empty() {
            print!("CPIME> ");
        } else {
            print!("CPIME [{}]> ", committed_sentence);
        }
        if io::stdout().flush().is_err() {
            break;
        }

        input_buf.clear();
        match stdin.read_line(&mut input_buf) {
            Ok(0) => {
                println!("\n[結束] 感謝使用 CPIME-Next REPL。");
                break;
            }
            Ok(_) => {
                let query = input_buf.trim();
                if query.is_empty() {
                    continue;
                }
                if matches!(query, ":q" | "exit" | "quit") {
                    println!("[結束] 感謝使用 CPIME-Next REPL。");
                    break;
                }

                if query == ":clear" {
                    engine.clear_context();
                    committed_sentence.clear();
                    last_query = None;
                    println!("[狀態] 已清空上下文 (Context) 與目前已打出的句子。\n");
                    continue;
                }

                if query == ":predict" {
                    let prev = engine.last_committed_word.clone();
                    let preds = engine.get_predictions(9);
                    match prev {
                        None => {
                            println!("── 聯想字 (:predict) ─────────────────────────────────");
                            println!("  (目前尚無前字上下文，請先輸入拼音並按 1~9 選字上屏)\n");
                        }
                        Some(p) => {
                            println!(
                                "── 基於前字「{}」的 Bigram 聯想字 ────────────────────",
                                p
                            );
                            if preds.is_empty() {
                                println!("  (尚無接在「{}」後面的記憶聯想詞)", p);
                            } else {
                                for (idx, cand) in preds.iter().enumerate() {
                                    println!(
                                        "  {}. {}  [{}]  (常見拼音: {}, 分數: {})",
                                        idx + 1,
                                        cand.word,
                                        cand.source.label(),
                                        cand.matched_code,
                                        cand.score
                                    );
                                }
                            }
                            println!();
                        }
                    }
                    continue;
                }

                // 若輸入為單一數字 1~9，代表選擇上一次查詢的對應候選詞上屏
                if query.len() == 1 {
                    if let Some(digit) = query.chars().next().and_then(|c| c.to_digit(10)) {
                        if (1..=9).contains(&digit) {
                            let idx = (digit - 1) as usize;
                            if let Some(ref prev_q) = last_query {
                                if let Some(chosen_word) = engine.commit_candidate(prev_q, idx) {
                                    committed_sentence.push_str(&chosen_word);
                                    println!(
                                        "  [上屏] 已選取第 {} 個候選詞：「{}」 (對應輸入: \"{}\")",
                                        digit, chosen_word, prev_q
                                    );
                                    for ev in &engine.phrase_composer.last_events {
                                        if ev.promoted {
                                            println!(
                                                "  [✨ 已學會新詞：{} ({} / {})]",
                                                ev.zh_phrase, ev.toneless, ev.abbr
                                            );
                                        } else {
                                            println!(
                                                "  [👀 自造詞觀察中 ({}/2)：{} ({} / {})]",
                                                ev.freq, ev.zh_phrase, ev.toneless, ev.abbr
                                            );
                                        }
                                    }
                                    println!(
                                        "  目前已打出的句子 (Context): {}  (當前前字: {:?})\n",
                                        committed_sentence, engine.last_committed_word
                                    );
                                } else {
                                    println!("  [提示] 第 {} 個候選詞不存在。\n", digit);
                                }
                            } else {
                                println!("  [提示] 請先輸入拼音或英文查詢候選字，再輸入 1~9 選字。\n");
                            }
                            continue;
                        }
                    }
                }

                last_query = Some(query.to_string());
                let start = Instant::now();
                let candidates = engine.get_candidates(query);
                let elapsed = start.elapsed();

                println!(
                    "── 查詢: \"{}\" | 目前已打出的句子 (Context): \"{}\" (耗時: {:.3} ms) ──",
                    query,
                    committed_sentence,
                    elapsed.as_secs_f64() * 1000.0
                );

                if candidates.is_empty() {
                    println!("  (無匹配候選詞)");
                } else {
                    for (idx, cand) in candidates.iter().enumerate() {
                        println!(
                            "  {}. {}  [{}]  (匹配碼: {}, 分數: {})",
                            idx + 1,
                            cand.word,
                            cand.source.label(),
                            cand.matched_code,
                            cand.score
                        );
                    }
                    println!("  (提示: 輸入 1~{} 可將對應候選詞上屏並記錄學習)", candidates.len());
                }
                println!();
            }
            Err(err) => {
                eprintln!("[錯誤] 讀取標準輸入失敗: {}", err);
                break;
            }
        }
    }
}
