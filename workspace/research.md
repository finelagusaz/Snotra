# 調査: issue #1542 — egui-runtime の計装の eprintln! を 1 回の書き込みにする

## issue の要約

#1539（PR #1540）は `src-tauri/src/trace.rs` の `trace()` を、1 行を `String` に組んで `write_all` 1 回で書く形へ変えた。
`snotra-egui-runtime` の計装は今も `eprintln!` に書式引数を渡しており、std の `Stderr` はバッファを持たないので、1 行が書式の断片ごとの書き込みに分かれる。
`Start-Process -RedirectStandardError` のリダイレクトは write-through で開かれる（`PERFORMANCE.md`「不採用: `Start-SnotraProcess` の起動の形を替えて stderr の write-through を外す（#1543）」）ので、1 回ごとに単価がかかる。

受け入れ条件（issue 本文）:

1. 対象の計装の 1 行が stderr への書き込み 1 回になる（書き込み回数を数えるテストで確かめる）
2. 行の形（接頭辞・`key=value` の並び・改行）を変えない。smoke と `scripts/` の消費者が通る
3. `SNOTRA_EGUI_INPUT_TRACE` の 1 行の所要を smoke と同じリダイレクト条件で前後に測り、`PERFORMANCE.md` の「17〜56ms」の記述を直す

issue が求める判断: 出口を共通にするか（crate 内に小さな関数を置くか、両 crate が使える場所へ移すか）と、その理由を計画に書く。

## 関連するファイル・シンボル

`git grep -n "eprintln!" -- snotra-egui-runtime` の全件（11 件。`eprint!` / `print!` / `println!` / `dbg!` / `stderr()` は 0 件）:

| ファイル | 行 | 接頭辞 | 出る条件 |
|---|---|---|---|
| `snotra-egui-runtime/src/input.rs` | `input_trace` | `SNOTRA_EGUI_INPUT {kind} ts_ms= {detail}` | `SNOTRA_EGUI_INPUT_TRACE`（`input_trace_enabled`） |
| `snotra-egui-runtime/src/renderer.rs` | `paint` の末尾 | `SNOTRA_EGUI_PAINT win= tess_ms= …`（13 引数） | `SNOTRA_EGUI_PAINT_TRACE` |
| `snotra-egui-runtime/src/repaint.rs` | worker の `None =>` arm | `SNOTRA_EGUI_WAKE_SEND window_id=` | `SNOTRA_EGUI_WAKE_TRACE` |
| `snotra-egui-runtime/src/runtime.rs` | `RedrawRequested` arm | `SNOTRA_EGUI_WAKE_RECV window_id= runtime_id=` | `SNOTRA_EGUI_WAKE_TRACE` |
| `snotra-egui-runtime/src/runtime.rs` | `RedrawRequested` arm / `attach_pending_windows` | `SNOTRA_EGUI_RENDER_ERROR=`（2 件） | 描画失敗・surface 生成失敗（無条件） |
| `snotra-egui-runtime/src/runtime.rs` | `on_window_event` | `SNOTRA_EGUI_DPI_CHANGED scale_factor=` | `ScaleFactorChanged`（無条件） |
| `snotra-egui-runtime/src/runtime.rs` | `render` | `SNOTRA_EGUI_REPAINT window= focused= since_prev_ms= causes=` | `SNOTRA_EGUI_REPAINT_TRACE` |
| `snotra-egui-runtime/src/windows_ime.rs` | 候補ウィンドウ位置 | `SNOTRA_EGUI_IME_RECT scale_factor= spot= area=` | `SNOTRA_EGUI_IME_TRACE` |
| `snotra-egui-runtime/src/windows_ime.rs` | `trace_ime_event` | `SNOTRA_EGUI_IME_COMMIT chars=` / `SNOTRA_EGUI_IME_PREEDIT chars= active=` | `SNOTRA_EGUI_IME_TRACE`（呼び出し側） |

- `input_trace` の呼び出し元: `input.rs`（`take` の心拍）、`runtime.rs`（`rx_key` / `ReceivedImeText`）、`windows_ime.rs`（`drop_key`・`push_key`・`push_text` の 3 件）。行の出口は `input_trace` の 1 か所である
- 双子: `src-tauri/src/trace.rs` の `render_line`（`[trace] ` + JSON + `\n`）と `write_line<W: Write>`（`let _ = w.write_all(..)`）。テストの `CountingWriter` が `write` の回数を数える
- `snotra-egui-runtime/CLAUDE.md` の「モジュール構成」: `.rs` を足したらここへ足す（`governance:check` が照合）
- `PERFORMANCE.md`「計測と受け入れ基準」の `SNOTRA_EGUI_INPUT_TRACE` の項: 「runner では stderr 1 行が 17〜56ms かかり、最初のフレームの到来を押し下げる」。`PERFORMANCE.md` へ入れたのは `dd338215`（#939）、`input.rs` の doc（`last_take_trace` と `take` のコメント）へ入れたのは `8a032526`（#938）。根拠は CI の run 30963445957（Pester 反復再現 job。#940 で撤去済み）で、測り方は `take` のコメントにある「隣接する 2 行の間隔」だけが残っている

## 消費者

- 行を機械で読む消費者は `scripts/` にない。`git grep` で各接頭辞（`_TRACE` を除く）を数えると、`scripts/` の出現は `scripts/lib/SnotraSmoke.psm1` の `Send-SnotraKey` のコメント 1 件だけ（`SNOTRA_EGUI_INPUT rx_key` と突き合わせると区間を測れる、という説明）。同じ関数の `if ($env:SNOTRA_EGUI_INPUT_TRACE)` は呼び出し側の env を読むだけで、本体の行は読まない
- smoke の不変条件検査（`scripts/lib/SnotraTraceInvariants.psm1`）は `[trace]` 行を読む。egui の計装は `[trace]` を名乗らない（`input.rs` の `input_trace` の doc）
- `docs/superpowers/` の過去の計画・設計に `SNOTRA_EGUI_PAINT` / `SNOTRA_EGUI_RENDER_ERROR` の記述があるが、書いた後に更新しない記録である

## 再利用できる既存のパターン

- `src-tauri/src/trace.rs` の分け方: 組む関数（純粋）と、`W: Write` を受けて 1 回で書く関数。テストは `CountingWriter` で `writes == 1` とバイト列の一致を見る
- 書き込みの失敗は捨てる（`eprintln!` は失敗で panic し、release は `panic = "abort"`）。同じ扱いにすれば失敗の経路は改善の向きにしか変わらない

## 技術的な制約

- `snotra-egui-runtime` は `src-tauri` に依存しない。逆に `src-tauri` は `snotra-egui-runtime` に依存する（`src-tauri/Cargo.toml`）。つまり「両方が使える置き場」は `snotra-egui-runtime` 自身か、新しい crate か、両者が依存する crate（`snotra-egui-runtime` は `snotra-core` に依存しない）
- `snotra-egui-runtime/Cargo.toml` は `[lints] workspace = true`。Cargo はこの形と crate 独自の `[lints.clippy]` の併記を許さないので、crate だけの lint は `lib.rs` の `#![..]` 属性で置く。ルートの `[workspace.lints.clippy]` へ足すと全 crate に及ぶ
- clippy の `print_stderr` lint（restriction 群）は `eprint!` / `eprintln!` を検出する。CI は `cargo clippy --workspace --all-targets -- -D warnings`（`.github/workflows/ci.yml`）
- 行の原子性: `eprintln!` は `Stderr` のロックを 1 行の間持つ。`std::io::stderr().write_all(..)` も 1 呼び出しの間ロックを持つ。repaint worker スレッド（`WAKE_SEND`）とイベントループスレッドの行は、どちらの形でも互いに割り込まない

## 敵対的調査（3b）の採否

出力: `workspace/adversarial-1542.txt`（sonnet 1 体）。

壊せた項目:

- A（採る・結論は変わらない）: `SnotraSmoke.psm1` の `Send-SnotraKey` には、コメントのほかに `if ($env:SNOTRA_EGUI_INPUT_TRACE)` の分岐がある。呼び出し側の env を読むだけで、本体の行は読まない。「行を機械で読む消費者は無い」は保つ
- B（採る）: 「17〜56ms」が `SNOTRA_EGUI_INPUT` 行 1 本の書き込みの所要だという根拠は無い。`input.rs` の `take` のコメントが残す測り方は「隣接する 2 行の間隔」（run 30963445957）だけで、どの行の間か・p50 か外れ値かは記録されていない。run のログは失効している（HTTP 410、所見の実測）。その run は #1539 より前の版で、`SNOTRA_TRACE` の `[trace]` 行（1 行 81 回の書き込み）が間に挟まっていた可能性も消せない。→ 計画の AC4 は「17〜56ms を新しい値で置き換える」ではなく、「測り方を明記した過去の観察として残し、新しい値を別に置く」にする（advisor の指摘と同じ向き）
- C（採る）: `PERFORMANCE.md` へ「17〜56ms」を入れたのは `dd338215`（#939）で、`8a032526`（#938）が入れたのは `input.rs` の doc と、撤去済みの再現スクリプトである（`git show dd338215 -- PERFORMANCE.md` に 1 件。`git show 8a032526 --stat` に `PERFORMANCE.md` は無い。自分で確かめた）。本文の「導入は `8a032526`」を訂正する
- D（採る）: `SNOTRA_EGUI_INPUT_TRACE` を立てて回す経路は、`scripts/` のどのスクリプトにも無い（`grep SNOTRA_EGUI_INPUT_TRACE scripts/*.ps1` は 0 件。自分で確かめた）。→ 計画の測定は、`Start-SnotraProcess -StandardErrorPath -ExtraVariables` を使う駆動スクリプトを scratchpad に書く形にする。打鍵を注入する区間では、呼び出し側の env も立てる（`Send-SnotraKey` の注入時刻の行を出すため。`SnotraSmoke.psm1` のコメントの #999）
- E（採る・計画に影響なし）: 「`format_args!` の値はその式の中でしか使えない」は、この toolchain（1.99）では偽である（`let` へ束縛できる）。計画は同じ文の中で使い切る形なので、どちらでも動く。制約の記述は消す
- F（採る）: `windows_ime.rs` の `input_trace` の 3 件は `drop_key`・`push_key`・`push_text` である。「push 系 3 件」を訂正する

壊せなかった項目（所見の要旨）: `eprintln!` 11 件が全件であること（他の出力マクロ・`build.rs`・`tests/` は無い）、`log` の logger が無く `log::` は stderr へ出ないこと、行を機械で読む消費者が無いこと（`Read-SnotraTraceSnapshot` は `^\[trace\]` だけを拾う）、`eprintln!` と `write_all` がどちらも 1 呼び出しの間 `Stderr` のロックを持つこと（std の `stdio.rs` を読んだ）、`[lints] workspace = true` と `[lints.clippy]` の併記を cargo が拒むこと、`#![deny(clippy::print_stderr)]` が workspace lints と併用でき `eprintln!` を拾い `write_all` を拾わないこと、`input_trace` の出口が 1 か所であること、依存の向き。

lint の所見は自分でも測った: scratchpad の最小 workspace（`[lints] workspace = true`・`lib.rs` に `#![deny(clippy::print_stderr)]`）で、`eprintln!` に `error: use of eprintln!` が立ち exit 101、同じファイルの `stderr().write_all` には立たなかった。

⚠️ の扱い:

- stderr に載る行は `eprintln!` 11 件より広い（panic フック・依存先）。対象は「この crate の計装の行」なので範囲は変えない
- ロックの射程はプロセス内の std `Stderr` 経由だけ。`stderr().write_all` は libtest の出力の捕捉に乗らない——製品のコードには関係しないが、テストで `emit`（本物の stderr）を呼ばず `emit_to`（`W: Write`）を測る理由になる
- 測定は release に揃える（smoke の既定は debug）。計画に書く
- `clippy.toml` の `disallowed_macros` による代案は採らない。`disallowed_*` は G-clippy-disallowed の照合の対象で、crate だけに効かせる属性のほうが影響が狭い
- 失敗で panic するのは EBADF 以外のエラーのとき（std は stderr の EBADF を成功として扱う）。計画の「書き込みに失敗したときに panic しなくなる」は、EBADF 以外の失敗についての記述として保つ
- `log::` は stderr へ出ない。範囲の外

## 未解決の疑問（計画で潰す）

- 対象範囲: issue は診断用のエラー行を対象外とする見込みと書く。`SNOTRA_EGUI_RENDER_ERROR`・`SNOTRA_EGUI_DPI_CHANGED` を含めるか
- 出口の置き場（issue の最初の論点）
- 書き込み回数を数えるテストを、各呼び出し点にどう効かせるか（出口のテストだけで足りるか）
- 測定の手順と、17〜56ms（runner・前の版・測り方の記録なし）とどう照らすか
