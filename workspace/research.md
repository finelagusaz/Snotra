# 調査: issue #1539 — trace 1 行を先に組んで stderr へ 1 回で書く

## issue の要約

`src-tauri/src/trace.rs` の `trace()` は `eprintln!("[trace] {line}")` で `serde_json::Value` を書く。
serde_json の `Display` は字句ごとに `Formatter::write_str` を呼び、std の `Stderr` はバッファを持たないので、
1 行が数十回の `WriteFile` に分かれる。`Start-Process -RedirectStandardError`（smoke ハーネスと同じ）では
1 行 p50 2.5 ms、`String` に組んで 1 回で書くと p50 0.12 ms（issue の別プログラムでの測定）。

受け入れ条件:
1. trace 1 行の書き込みが stderr への書き込み 1 回になっている
2. 計測ハーネスと同じリダイレクト条件で、1 行の所要を前後で測って記録する
3. trace の所要を前提にした `PERFORMANCE.md` の記述を新しい値と照らして直す

## 関連するファイル・シンボル（grep で実在を確認済み）

- `src-tauri/src/trace.rs` の `trace()`（66 行目 `eprintln!("[trace] {line}");`）。唯一の `[trace]` 出口。
  `main.rs` の `trace_main` と `commands::trace_command` はここへ委譲する（`//!`）。
- `src-tauri/src/heap_trace.rs` — `heap` 欄を `line` に足す。形式の変更は無い。
- 出力形式の消費者: `scripts/lib/SnotraSmoke.psm1`・`scripts/lib/SnotraTraceInvariants.psm1`・`scripts/smoke-startup.ps1`。
  いずれも `[trace] ` 接頭辞 + JSON 1 行を parse する。**形式（接頭辞・compact JSON・末尾の改行）は変えない。**
- 一次資料での裏取り:
  - serde_json 1.0.154 `src/value/mod.rs` の `impl Display for Value`: `WriterFormatter` の `io::Write::write` が
    `inner.write_str` を呼び、`ser::to_writer` が字句ごとに `write` するので、1 字句 = 1 `write_str`。
  - std（rustc 1.99.1）`io/stdio.rs`: `Stderr { inner: &'static ReentrantLock<RefCell<StderrRaw>> }`。バッファ無し。
    `eprintln!` は `Stderr::write_fmt` で、ロックを 1 回取って `fmt::Arguments` の各断片を `write` する。
- `trace()` の所要を前提にした記述:
  - `PERFORMANCE.md`「計測と受け入れ基準」: 「フレームループ中の trace は 1 行あたり約 10 ms かかる」の箇条と子箇条
    （「60fps の予算 16,700 µs は trace 2 本で超える」「区間ごとに trace を吐かない … 30 ms 級」「未調停の反例」）。現在形の主張。
  - `PERFORMANCE.md`「採用: 表示の計測から trace の書き込みを外す（#1526）」: 「回ごとに揺れる原因 … 確かめていない」。日付つきの記録。
  - `PERFORMANCE.md` 555 行付近「`update_us` には trace の書き込みが含まれる」（正本は「計測と受け入れ基準」を指す）、
    619 行付近「`update_us` の 50〜96% は trace の書き込み」（#1032 の日付つき記録）。
  - コメント「trace 1 行の書き込み（ミリ秒単位）」: `src-tauri/src/main.rs`（`ForegroundOutcome` の doc・`setup_hotkey_listener` の `t0` のコメント）、
    `src-tauri/src/egui_shell/window_coordinator.rs`（`egui_show:done` の後に書く理由）。直した後は「ミリ秒単位」が偽になりうる。
  - `docs/superpowers/plans/2026-08-10-search-worker.md`・`docs/superpowers/specs/2026-08-10-search-worker-design.md` は、
    当時の設計記録（書いた後は更新しない）。spec は正本を `PERFORMANCE.md` へ指している。

## 再利用できる既存のパターン

- `format!` で `String` に組み、`std::io::Write::write_all` で 1 回書く（issue の提案）。
  `Value` の `Display`（compact）は `serde_json::to_string` と同じ出力なので、行の中身は変わらない。
- テストで書き込み回数を数える: `trace()` を「行を組む純関数」と「`W: Write` へ書く関数」に分け、
  `write` の呼び出し回数を数える `Write` 実装で 1 回を assert する（`.claude/rules/rust.md` の「純粋な導出を切り出す」）。
- マイクロベンチは `ignored` テストとして持つ慣行（`PERFORMANCE.md`「計測と受け入れ基準」）。ただし今回の前後計測は一時の足場にする（下記）。

## 技術的な制約

- `eprintln!` は書き込み失敗で panic する（`print_to`）。`write_all` の結果を `let _ =` で捨てると panic しなくなる。
  Windows の `windows_subsystem = "windows"`（release）では stderr ハンドルが null で、std は黙って成功を返すので、
  どちらの形でも通常は差が出ない。診断用の出口なので、失敗を無視する形を採る。
- 行の原子性: `eprintln!` はロックを持ったまま各断片を書くので、Rust 内の他の stderr 書き込みと行の途中で混ざらない。
  `Stderr::write_all` も内部でロックを取る。加えて 1 回の `WriteFile` になるので、原子性は弱まらない。
- コンソールへ書くとき、std は UTF-16 へ変換して分割して書くことがある（`write_all` が回す）。計測対象（ファイル・パイプへのリダイレクト）では 1 回。
  受け入れ条件 1 は「`Write` への呼び出し 1 回」で検査する（OS 呼び出し回数はコンソールのとき 1 とは限らない）。
- libtest は `eprintln!` を捕捉し、`io::stderr().write_all` は捕捉しない。テストで stderr へ実際に書くと
  `cargo test` の出力に `[trace]` 行が漏れる。テストは `W: Write` の差し替えで書き、実 stderr へは書かない。
  前後計測のベンチは `--nocapture` で走らせる（旧形の `eprintln!` が捕捉されると比較が壊れる）。
- `SNOTRA_TRACE` が無効のときは早期 return で、製品の既定の挙動・性能には影響しない。

## 敵対的調査（3b）の反映 — `workspace/adversarial-1539.txt`

壊せた・確信なしの所見と採否:

- 採る: 「1 字句 = 1 `write_str`」は不正確。文字列は引用符・本体・エスケープ断片が別々に書かれる（toy で 153 byte の 1 行が 81 回の write）。結論（1 行が数十回に分かれる）は変わらない。
- 採る: 現在形の記述の一覧は網羅でなかった。追加で読んだもの: `PERFORMANCE.md` の #1454 注記（3746 行付近）・「runner でも、区間の中身は trace の書き込みだった」節・#1032 節（585 行付近）・#1526 節。
  いずれも日付つきの記録で、書き換えの対象にしない。`startup.rs` の `finish_at` の doc（「stderr の受け手の速さで動く」）は現在形だが、1 回書きでも真のまま。
  直す対象は計画どおり「計測と受け入れ基準」の箇条と、`main.rs`・`window_coordinator.rs` の「ミリ秒単位」。
- 採る（⚠️）: release は `panic = "abort"`（ルート `Cargo.toml` の `[profile.release]`。確認済み）。`eprintln!` の書き込み失敗は release でプロセスを abort させていた。
  `let _ =` にすると「abort」が「診断が黙って欠ける」に変わる。失敗を待つ検査は無い。計画の「不変条件と異常系」と PR 本文に書く。
- 採る（⚠️）: `heap-trace` ビルドでは、行の `String` 確保が snapshot の後に起きるので、次の行の確保回数に 1 行分（伸長の再確保を含む）が乗る。
  現行も `json!` の確保は snapshot の前にあり、Display は確保しない。計画の不変条件に書き、ヒープの前後比較へ混ざることを PR 本文に書く。
- 採る（⚠️）: toy（アイドル・20 ms 間隔）の 2.5 ms は #1526 の `ime_control`（2.2 ms）と合うが、Alt 経路の `listener_enter` の約 10 ms（二峰の上）は toy に現れない。
  別の機序の可能性がある。前後の計測は実アプリ・release・`-StandardErrorPath` 付きで行う（計画の計測手順のとおり）。上の峰が残るかで `PERFORMANCE.md` の直し方を決める。
- 採る（⚠️）: `Start-SnotraProcess` は `StandardErrorPath` を渡したときだけリダイレクトする。計測スクリプトは必ず渡す。ローカル既定の debug ではなく release で揃える。
- 争点 6 の条件: libtest が捕捉しない `write_all` が漏れるのは `SNOTRA_TRACE` を立てて `cargo test` を回したときだけで、テストで立てる箇所は無い。テストは `W: Write` の差し替えで書くので影響しない。

壊せなかった項目: `trace.rs` の唯一の出口と委譲構造、serde_json の compact Display、`Stderr` のバッファ無しとロック、
`format!` と `eprintln!` のバイト列一致、消費者の依存（接頭辞・compact JSON・LF）、原子性、無効時の早期 return、「ミリ秒単位」コメントの所在。

## 未解決の疑問

- Start-Process のリダイレクトで 1 回の書き込みが cmd の `2>file` より約 10 倍遅い理由（issue の「確かめていないこと」）。本 issue の範囲外。
- ほかの stderr 出口の形: `snotra-egui-runtime` の計装（`input.rs` の `input_trace`・`runtime.rs` の `SNOTRA_EGUI_REPAINT`/`WAKE_RECV` など）は
  `eprintln!` に書式引数を渡す形で、引数の数だけ断片の書き込みに分かれる（Value の字句ほど多くはない）。`PERFORMANCE.md` は
  `SNOTRA_EGUI_INPUT_TRACE` について「runner では stderr 1 行が 17〜56ms」と書いている。本 issue の受け入れ条件は `trace()` だけなので範囲外とし、別 issue の候補として報告する。
- 「約 10 ms」の上の峰（#1526 で `listener_enter` が約 2 ms / 約 10 ms の二峰）が、書き込みを 1 回にした後も残るか。前後計測で測る。
