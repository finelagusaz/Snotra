# #1542 独立導出: 触るファイルとシンボルの一覧

基点: worktree `issue-1542-d536d1`・ブランチ `perf/egui-trace-single-write`（HEAD bfbc0385）。
`workspace/` の計画・調査文書は読んでいない（この出力ファイルだけを書いた。目にした計画・調査の内容は無い）。
根拠の grep は対象ディレクトリを明示して実行した（`snotra-egui-runtime/` `src-tauri/` `scripts/` `.github/` `docs/` `PERFORMANCE.md` ほか）。

## 1. 対象の stderr 出力（母集団）

母集団の取り方: `grep -rn -E 'eprint!?\(|eprintln!|stderr\(\)|io::stderr' snotra-egui-runtime` と、
`grep -rn -E '\bprint!|println!|dbg!|writeln!|write!\(' snotra-egui-runtime/src`（他の出口の有無）。
後者は `log::*` と `panic!`（テスト内）以外を返さなかった。`eprintln!` は 11 箇所（マクロ呼び出し単位）。
`stderr()` を直接書く箇所は 0。

| # | file:line | 関数（所属） | 接頭辞 | ゲート | 書式の断片（概算の write 回数） |
|---|---|---|---|---|---|
| 1 | `snotra-egui-runtime/src/input.rs:68` | `input_trace(kind, detail)` | `SNOTRA_EGUI_INPUT {kind} ts_ms=… {detail}` | `SNOTRA_EGUI_INPUT_TRACE`（`input_trace_enabled` の OnceLock） | リテラル 4 + 引数 3 ≈ 7 |
| 2 | `snotra-egui-runtime/src/renderer.rs:182` | `EguiRenderer::paint`（`renderer.rs:71`・`trace` が真のとき） | `SNOTRA_EGUI_PAINT win=… tess_ms=…` | `SNOTRA_EGUI_PAINT_TRACE`（`renderer.rs:83` で生成時に読む） | 引数 13・リテラル 13 ≈ 26（最多） |
| 3 | `snotra-egui-runtime/src/repaint.rs:198` | `RepaintScheduler::new` が spawn する worker のループ（dispatch） | `SNOTRA_EGUI_WAKE_SEND window_id=…` | `SNOTRA_EGUI_WAKE_TRACE` | ≈ 3（**worker スレッドから書く**） |
| 4 | `snotra-egui-runtime/src/runtime.rs:283` | plugin の `on_event` の `Event::RedrawRequested` arm | `SNOTRA_EGUI_WAKE_RECV window_id=… runtime_id=…` | `SNOTRA_EGUI_WAKE_TRACE` | ≈ 5 |
| 5 | `snotra-egui-runtime/src/runtime.rs:295` | 同 arm（`render()` 失敗） | `SNOTRA_EGUI_RENDER_ERROR=…` | **無条件** | ≈ 3 |
| 6 | `snotra-egui-runtime/src/runtime.rs:334` | `attach_pending_windows`（softbuffer surface 初期化失敗） | `SNOTRA_EGUI_RENDER_ERROR=…` | **無条件** | ≈ 3 |
| 7 | `snotra-egui-runtime/src/runtime.rs:410` | `EguiWindow::on_window_event`（`ScaleFactorChanged`） | `SNOTRA_EGUI_DPI_CHANGED scale_factor=…` | **無条件** | ≈ 3 |
| 8 | `snotra-egui-runtime/src/runtime.rs:471` | `EguiWindow::render`（`repaint_trace_prev`） | `SNOTRA_EGUI_REPAINT window=… focused=… since_prev_ms=… causes=…` | `SNOTRA_EGUI_REPAINT_TRACE` | ≈ 9 |
| 9 | `snotra-egui-runtime/src/windows_ime.rs:119` | `PlatformIme::update`（候補ウィンドウ位置・変化時のみ） | `SNOTRA_EGUI_IME_RECT scale_factor=… spot=… area=…` | `SNOTRA_EGUI_IME_TRACE` | ≈ 14 |
| 10 | `snotra-egui-runtime/src/windows_ime.rs:313` | `trace_ime_event`（Commit） | `SNOTRA_EGUI_IME_COMMIT chars=…` | `SNOTRA_EGUI_IME_TRACE`（`windows_ime.rs:279`） | ≈ 3 |
| 11 | `snotra-egui-runtime/src/windows_ime.rs:318` | `trace_ime_event`（Preedit） | `SNOTRA_EGUI_IME_PREEDIT chars=… active=…` | 同上 | ≈ 5 |

`input_trace` を経由する呼び出し元（形は #1 に集約済みなので、出口を変えれば自動で直る。個別の変更は不要）:
`input.rs:112`（`take` の心拍）・`runtime.rs:221`（`rx_key`）・`runtime.rs:229`（`rx_text`）・
`windows_ime.rs:366`（`trace_suppressed` の `drop_key`）・`windows_ime.rs:389` / `:397`（`trace_keyboard_event` の `push_key` / `push_text`）。

- ⚠️ **#5〜#7 は計装ではなく無条件の診断行である**（ゲートなし）。issue の「対象の計装」に入るかは読み方次第。
  入れる理由: 出口を 1 つにすると検出器（§5）を例外なしで張れる。`log::error!`（`runtime.rs:294`・`:333`）は
  `src-tauri` に logger が無い（`grep -E 'env_logger|tauri_plugin_log|set_logger|fern' src-tauri/src src-tauri/Cargo.toml` が 0 件）ので no-op で、
  stderr の行だけが実際の出力である。入れない理由: 稀にしか出ず性能の意味は無い。**推奨: 入れる**（検出器を除外リストなしで置けるため）。
- ⚠️ 断片の回数は書式文字列から数えた概算で、測っていない。`{:?}`（`WindowId` など）や `format_repaint_causes` の `Display` は内部で複数回書きうる。
  受け入れ条件 (1) はテストで測るので、この概算は根拠に使わない。
- **範囲外として記録**: `src-tauri/src` に `eprintln!` が 11 箇所残る（`commands/window.rs:59,116,134`・`config_watcher.rs:83,124`・`indexing.rs:102`・`main.rs:256`・`platform/hotkey.rs:187,202`・テスト内の `egui_shell/font_stack.rs:564,607`）。
  issue は egui-runtime に限っているので触らない。ただし §5 の検出器を workspace 単位で張ると、これらに当たる（⚠️ 範囲の決定が要る）。

## 2. 行を消費する側（受け入れ条件 (2) の母集団）

- **プログラムで egui の行を読む消費者は、scripts・CI・Rust テストに見つからなかった。**
  `grep -rn -E 'EGUI_(INPUT|PAINT|WAKE|REPAINT|IME|DPI|RENDER)|tess_ms|since_prev_ms|rx_key|IME_COMMIT' scripts .github src-tauri/src` は
  `scripts/lib/SnotraSmoke.psm1:921,925,927,933`（コメントと `Send-SnotraKey` の env 読み）だけを返した。`.github/` は 0 件。
  `grep -rn 'SNOTRA_EGUI|SNOTRA_SMOKE' scripts .github`（切り詰めなし）も、egui の出力行を読む箇所は `SnotraSmoke.psm1:925-936` だけで、残りは `SNOTRA_EGUI_FAKE_UPDATE*`（入力の env）だった。`src-tauri/tests` は存在しない（`ls` で確認）。⚠️ 全称否定であり、
  **人が読む手順（PERFORMANCE.md の読み方の記述・過去の issue の解析スクリプト）は母集団の外**である。
- **間接の消費者（守るべき不変条件）**:
  - `scripts/lib/SnotraSmoke.psm1` の `Read-SnotraTraceSnapshot`（`:731` の DESCRIPTION）と `scripts/lib/SnotraTraceInvariants.psm1:125-127`:
    `[trace]` で始まり parse できない行だけを「捨てた行」に数える。**egui の行は `[trace]` で始めてはならない**（`input.rs:53-58` の doc が理由を書いている）。
    **改行を落とす・2 行を 1 回にまとめ損ねると、次の `[trace]` 行が egui 行の後ろに連結され、行頭が `[trace]` でなくなって trace イベントが黙って消える**（捨てた行にも数えられない）。
    よって「末尾の `\n` を 1 つだけ」をテストで固定する必要がある（`src-tauri/src/trace.rs` の `render_line_is_prefix_compact_json_and_one_newline` と同型）。
  - `scripts/lib/SnotraSmoke.psm1:933-937` の `Send-SnotraKey`: 子の `SNOTRA_EGUI_INPUT rx_key` と自分の `SNOTRA_SMOKE_INJECT ts_ms=…` を `ts_ms`（epoch ms）で突き合わせる前提。
    `ts_ms` の時計（`SystemTime::now()` の epoch ms）と `ts_ms=` のキー名を変えない。
  - smoke（`scripts/smoke-egui.ps1`・`.github/workflows/smoke.yml:20` が `snotra-egui-runtime/**` で発火）: egui の trace env を立てない（根拠は切り詰めなしの `grep -rn 'SNOTRA_EGUI|SNOTRA_SMOKE' scripts .github`。smoke-egui.ps1 が立てるのは `SNOTRA_EGUI_FAKE_UPDATE` だけ・`:863`）。
    ゆえに smoke が通ることは「既定（計装オフ）で何も壊していない」ことの確認にしかならない。⚠️ **行の形を守ったことの証拠にはならない**ので、形はユニットテストで固定する。
  - 行の並行性: #3 は repaint worker スレッド、それ以外はイベントループ／subclass（UI スレッド）、`src-tauri/src/trace.rs` は複数スレッドから書く。
    `Stderr::write_all` も `eprintln!` も std の stderr の再入可能ロックを 1 呼び出しの間とるので、行の途中で他スレッドの行が割り込まないことは変わらない（⚠️ std の実装から読んだ理解で、測っていない）。
- **Pester**: `scripts/lib/SnotraSmoke.Tests.ps1` は stderr を `[config] ` などで照合するだけで egui の行を見ない（`:856-1002` を grep で確認）。変更不要。

## 3. 更新が要る文書・コメント

### 「17〜56ms」の全出現（`grep -rn -E '17[〜~-]56|30963445957'` を egui-runtime・src-tauri・scripts・.github・docs・PERFORMANCE.md・SPEC.md・AGENTS.md・CLAUDE.md へ）

| file:line | 中身 | 処置の案 |
|---|---|---|
| `snotra-egui-runtime/src/input.rs:23-25` | `last_take_trace` の doc「stderr 1 行が 17〜56ms かかると実測されており（run 30963445957）」 | 測り直した値と条件へ直す。数値の正本は PERFORMANCE.md に置き、ここは参照にする（`docs/comment-guidelines.md`「短く保つ」の測定値の置き場所） |
| `snotra-egui-runtime/src/input.rs:98-103` | `take` 内コメント「runner では stderr 1 行が 17〜56ms かかると実測した（run 30963445957・隣接する 2 行の間隔）」 | 同上 |
| `PERFORMANCE.md:3788` | `SNOTRA_EGUI_INPUT_TRACE` の項「runner では stderr 1 行が 17〜56ms かかり」 | 測り直した値で直す。CI runner の値と開発機の値を混ぜない（`PERFORMANCE.md:3778`「1 つの数へまとめない」） |
| `PERFORMANCE.md:3778` | 「CI runner での実測は同節の `SNOTRA_EGUI_INPUT_TRACE` の項にある」 | 3788 の書き方次第で文言を合わせる（所在の主張なので、3788 が runner の値を消すなら偽になる） |

`docs/superpowers/` の 17〜56 は 0 件。`56 ?ms|stderr 1 行|1 行が` の言い換えでも上の 3 箇所以外は出なかった。

- ⚠️ **17〜56ms は「CI runner・隣接する 2 行の間隔」であって、1 回の書き込みの所要ではない**（`input.rs:99` が条件を書いている）。
  #1543 の実測（`PERFORMANCE.md`「不採用: `Start-SnotraProcess` の起動の形を替えて stderr の write-through を外す（#1543）」）で write-through 1 回は 0.2 ms 規模、
  egui の 1 行は断片 ≈ 7 回なので、開発機では ≈ 1.5 ms 規模と見込まれる。**17〜56ms は断片の回数だけでは説明できない見込みが高い**（runner のディスク・別の待ち）。
  直す文は「前後の値」と「17〜56ms は条件の違う別の量」を書き分ける必要がある。runner の値を測り直すかは決める事項（後述 §6）。
- ⚠️ `input.rs:98-103` の心拍の間引き（`TAKE_TRACE_HEARTBEAT`）の理由はこの数値に依っている。値が小さくなっても**間引き自体は変えない**（受け入れ条件 (2) 行の形を変えない・行数も変えない）。理由の文だけを直す。

### その他の文書

- `PERFORMANCE.md`: 新しい節「採用: egui-runtime の計装の 1 行を 1 回の書き込みにする（#1542）」を「採用: trace 1 行を 1 回の書き込みにする（#1539）」（`:774`）の近くに置き、前後の表と測定条件を書く。
  `:3784-3789`（計測コードの env の一覧。「このリストが計測コードの正本」`:3783`）は行の形を書いているだけなので、形を変えなければ数値以外は変更不要。
- `PERFORMANCE.md:3748-3752`（「フレームループ中の trace は 1 行あたり p50 約 0.11 ms」）: `[trace]` の話であって egui の行ではない。変更不要だが、新節から egui の値を参照させるなら相互参照を確かめる。
- `snotra-egui-runtime/CLAUDE.md`「モジュール構成」: **新しい `.rs` を足すときだけ**行を足す（`AGENTS.md`「条件別チェック」の「ファイル（`.rs`）を追加/削除」・`governance:check` が照合）。既存ファイル（`env.rs` など）へ置くなら、そのファイルの行の責務文を直す（例: `env.rs` は今「trace ハッチの env 述語」だけを名乗る）。
- 置いたファイルの `//!`: 出口の責務（1 行を組んで 1 回で書く・失敗を捨てる・理由は #1539 と同じ）を書く。`src-tauri/src/trace.rs:17-21` の `//!` と同じ内容を写さず、そちらか PERFORMANCE.md の節を指す。
- `src-tauri/src/trace.rs` の `//!`（`:1-6`「Single home of the trace logic」）: 出口を両 crate で共通にする場合だけ直す。§4 の推奨（共通にしない）なら変更不要。
- `input.rs:53-58`（`input_trace` の doc）: 「既存の `SNOTRA_EGUI_WAKE_TRACE` と同じ流儀で独自の接頭辞を使い」は真のまま。変更不要。
- `docs/superpowers/` の `eprintln!` を含む設計書・計画（`2026-07-21-su1-softbuffer-runtime.md:567`・`2026-07-24-su6.5-flip-hardening.md:859-887`）: 日付付きの歴史記録なので触らない。
- `PERFORMANCE.md:3765` は #1032 当時の `trace()` の `eprintln!` の記録で、歴史として真。変更不要。

## 4. 出口の置き場の判断

選択肢:

- (a) **egui-runtime の中に小さな関数を置く（推奨）**
- (b) 両 crate が使える場所へ移す: `snotra-core`（src-tauri は依存しているが egui-runtime は依存していない。`snotra-egui-runtime/Cargo.toml` の `[dependencies]` に無い）、または egui-runtime に置いて src-tauri の `trace.rs` から使う
- (c) 新しい共有 crate

(a) を推す理由:

1. **依存の向き**: `src-tauri/Cargo.toml:19` が egui-runtime に依存する。egui-runtime から src-tauri は使えない（循環）。共有するなら core か egui-runtime だが、
   core に egui の診断出口を入れるのは責務違い、egui-runtime（「Tauri 管理の Window へ egui を描く接着層」）に src-tauri の `[trace]` の出口を置くのも責務違い。
2. **共通部分が 1 行しかない**: `trace.rs` の `write_line` は `let _ = w.write_all(line.as_bytes());` だけである（`src-tauri/src/trace.rs:84-86`）。
   行の組み立て（`[trace]` + JSON + `seq` + `heap`・`render_line`）と egui の行（独自接頭辞の key=value・`seq` を持たない。`input.rs:53-58`）は別物。
3. **片方だけが変わる将来が挙がる**（`AGENTS.md`「検証の作法」）: `[trace]` 側は `seq`・ヒープ標本・`SNOTRA_TRACE` の許可リストの意味論を持ち、egui 側は空文字＝未設定の意味論（`env.rs:10-14` が「寄せない」と明記）。
   出口を共有すると、env の意味論の違いを抱えた 2 つの機構を 1 つの関数が繋ぐことになる。
4. 共有のための crate 依存の追加（core → egui-runtime）は、3 行の関数のために依存グラフを変える。

(a) の中での置き場（⚠️ 未決）:
- `env.rs` に同居: 「trace ハッチ」の 1 か所に寄せられるが、`env.rs` の `//!` は「env 述語」と「空文字の唯一の場所」を名乗っており、責務が 2 つになる。
- 新ファイル（例 `trace_line.rs` / `diag.rs`）: 責務が明確。`CLAUDE.md` のモジュール構成へ 1 行と `//!` が要る。
- `input.rs` の `input_trace` の隣: 他の 10 箇所が `input` に依存する形になり、名前と責務がずれる。推奨しない。

関数の形（`src-tauri/src/trace.rs` と同型）: 行を `String` に組む純関数（あるいは各呼び出し点で `format!` して `\n` を足す）と、
`fn write_line<W: Write>(w: &mut W, line: &str)`（`write_all` 1 回・失敗は捨てる）と、`std::io::stderr()` へ書く薄い入口。
- ⚠️ **失敗時の挙動が変わる**: `eprintln!` は書き込み失敗で panic し、release は `panic = "abort"`（ルート `Cargo.toml` の `[profile.release]`）。
  `write_all` の結果を捨てると「止まらない」に変わる。`trace.rs:80-83` と同じ判断（診断なので止めない）で揃うが、挙動の変更なので PR 本文に書く。
- ⚠️ `format!` の割り当てがゲートの内側に入っていること（計装オフで費用 0・`PERFORMANCE.md:3783`「どれも未設定なら、計測コードのコストは 0」）を各呼び出し点で保つ。
  `input_trace_enabled` の doc（`input.rs:33-35`）が同じ懸念を書いている。

## 5. 検出器（`eprintln!` が戻ったら落ちる仕組み）の案

- **案 A（推奨）: `snotra-egui-runtime/src/lib.rs` の先頭に `#![deny(clippy::print_stderr)]`**（必要なら `clippy::print_stdout` も）。
  `print_stderr` は restriction 群の lint で、`eprint!` / `eprintln!` を検出する。CI は `cargo clippy --workspace --all-targets -- -D warnings`（`.github/workflows/ci.yml:178`）で走る。
  - crate の `Cargo.toml` は `[lints] workspace = true` なので、`[lints.clippy]` を crate 側に足す形は使えない（Cargo は workspace 継承と crate 固有の lint 表を併用できない・⚠️ Cargo の仕様の記憶で、このリポジトリでは試していない）。属性で張るのが最小。
    ルートの `[workspace.lints.clippy]` に足すと src-tauri の 11 箇所（§1 の範囲外）に当たる。
  - `--all-targets` なので `#[cfg(test)]` 内の `eprintln!` も当たる。現状 egui-runtime のテストには 0 件（§1 の grep）。
  - ⚠️ 検出しない経路: `writeln!(std::io::stderr(), …)` / `std::io::stderr().write_fmt(…)`（これも断片ごとに書く）・`dbg!`。
    前者は `disallowed_methods`（`std::io::Write::write_fmt`）で塞げるが、`clippy.toml` は src-tauri にしか無く（`snotra-egui-runtime/clippy.toml` は不在）、
    新設すると `G-clippy-disallowed`（`scripts/governance/checks/G-clippy-disallowed.mjs`）の母集団の外に出る。`dbg!` は `clippy::dbg_macro`。費用に見合うかは要判断。
  - 故障注入（`.claude/rules/safety-nets.md`）: 複製で `eprintln!` を 1 行戻し、clippy の診断が `print_stderr` であること（コンパイルエラーではなく lint であること）を出力で確かめる。
    逆向き: 属性を消すと緑に戻ることも見る。
- 案 B: ソーステキスト検査（`include_str!` で crate の `.rs` を読み `eprintln!` の不在を assert）や `governance:check` の新設。lint がある以上、機構を足す理由が弱い。推奨しない。
- **書き込み回数のテスト**（受け入れ条件 (1)）: `src-tauri/src/trace.rs` の `CountingWriter` と同型のテストを egui-runtime の出口に置く。
  出口 1 つに対して `writes == 1`・バイト列一致・末尾の `\n` がちょうど 1 つ・行頭が `[trace]` でないことを assert する。
  - ⚠️ **このテストは出口が 1 回で書くことしか見ない**。11 箇所がすべて出口を通ることは案 A の lint が担う。両方そろって (1) になる。
  - ⚠️ `CountingWriter` を src-tauri と egui-runtime の両方に置くと重複に見えるが、テスト用の 15 行で crate をまたぐ共有の費用に見合わない。`/dry-check` で指摘されたら §4 の理由を返す。
  - 行の形の固定: 各呼び出し点の書式を純関数へ切り出すとテストできるが、11 箇所すべてを切り出すのは過剰。
    形を変えない保証は「書式文字列を変えずに `eprintln!` → `format!` + 出口へ機械的に置き換えた」差分の読みと、代表 1〜2 行（`input_trace` と `PAINT`）の文字列テストで足りるとみる（⚠️ 判断）。
    「挙動を変えない前提の変更では代表的な入力と出力を先に記録しておく」（`AGENTS.md`「開発ワークフロー」4）に従い、変更前に各接頭辞の実際の 1 行を取っておき、変更後と突き合わせる。

## 6. 測定の手順（受け入れ条件 (3)）

`PERFORMANCE.md`「採用: trace 1 行を 1 回の書き込みにする（#1539）」の測定条件を型にする。

1. 版: 前（`bfbc0385` か `origin/main`）と後を、別々の worktree と `CARGO_TARGET_DIR` で release ビルドする。
2. 起動: `Start-SnotraProcess -StandardErrorPath <file> -ExtraVariables @{ SNOTRA_EGUI_INPUT_TRACE = '1' }`（smoke と同じ `Start-Process -RedirectStandardError`＝write-through。`scripts/lib/SnotraSmoke.psm1:369`）。
   `-Trace`（`SNOTRA_TRACE`）は立てない（`[trace]` の行が混ざって 1 行の所要がぶれる）か、立てるなら両版で同じにする。検証用プロファイル（`New-SnotraVerificationProfile`）。
3. 操作: Alt+F12 で表示 → 数文字打鍵 → Escape で非表示、を 25 回で 1 巡。前 → 後 → 前 → 後の 2 巡ずつ。
   `SNOTRA_SMOKE_INJECT` は**呼び出し側（親の pwsh）の** `$env:SNOTRA_EGUI_INPUT_TRACE` を見て出る（`SnotraSmoke.psm1:931-933`）。`-ExtraVariables` は子の env を立てて戻すだけなので、
   注入から `rx_key` までの区間も取るなら、打鍵の区間で親の env も立てる。
   - ⚠️ **ビルド種別**: smoke のローカル既定は debug（`SnotraSmoke.psm1:375`・`:414-415` の `-Profile` 既定 `'debug'`・#1292）、#1539 の測定は release。
     「smoke と同じリダイレクト条件」がプロファイルまで含むかは決める事項。debug はコンソールサブシステムで stderr の扱いも違いうるので、少なくとも条件に明記する。
4. 計り方: **「行の組み立て + 書き込み」を直接計時して次の行へ載せる一時の計装**（#1539 と同じ・コミットしない。撤去は `docs/development-principles.md`「観測するとき」の一時的な足場の規約）。
   `input_trace` の 1 箇所に入れれば INPUT の全行を取れる。報告は p50 / p90 / 最大を kind（`take` / `rx_key` / `push_key` / `push_text`）ごとに。
   - ⚠️ 隣接 2 行の `ts_ms` 差（17〜56ms を出した方法）は ms 分解能で、間の処理を含む。直接計時と別の量なので、使うなら列を分ける。
5. 記録: 測定日・機体・pwsh の版・ビルド・行数を PERFORMANCE.md の新節へ。`input.rs` の 2 箇所はその節を指す。
6. ⚠️ **CI runner の値（17〜56ms の出所 run 30963445957）を測り直すか**: ローカルの前後だけでは「runner では 17〜56ms」を反証も更新もできない。
   選択肢は (i) smoke workflow を一時的に `SNOTRA_EGUI_INPUT_TRACE` 付きで回して前後を取る（`workflow_dispatch` / 一時の `push:`・`.claude/rules/safety-nets.md`「CI での実測は PR 本文のチェックリストに書く」）、
   (ii) runner の値は「#1542 より前・条件の違う量」として残し、開発機の前後を並べる。issue の (3)「その値と照らして直す」は (ii) でも満たせるが、runner の数値を残すなら時点と条件を添える。
7. 他の 4 ハッチ（PAINT / REPAINT / WAKE / IME）は受け入れ条件に無い。PAINT は断片が最多（≈ 26）なので、余力があれば同じ計装で 1 回測ると効果の上限が分かる（⚠️ 任意）。

## 7. 実行する検証（`docs/build-commands.md`「変更後の検証チェックリスト」から該当を選ぶ想定）

- `cargo test -p snotra-egui-runtime`（新テスト）・`cargo clippy --workspace --all-targets -- -D warnings`（案 A の lint）・`cargo doc`（`//!` を足すので。`.claude/rules/comments.md`）
- `npm run governance:check`（`CLAUDE.md` のモジュール構成・PERFORMANCE.md の参照）
- smoke（`scripts/smoke-egui.ps1`。既定オフでの非退行）と Pester（`scripts/lib/SnotraSmoke.Tests.ps1`。変更しないが母集団として）
- 計装オン（`SNOTRA_EGUI_INPUT_TRACE=1`）で 1 回起動し、`Read-SnotraTraceSnapshot` の Dropped が 0 のままで、egui 行と `[trace]` 行が別々の行に出ていることを目で見る（§2 の連結の不変条件）

## 8. 追補（advisor の指摘を受けて測り直した分）

### issue 本文（`gh issue view 1542 --json body`）

- 本文が名指す対象: `input.rs` の `input_trace`・`runtime.rs` の `SNOTRA_EGUI_REPAINT`・`SNOTRA_EGUI_WAKE_RECV`・`repaint.rs` の `SNOTRA_EGUI_WAKE_SEND`・`renderer.rs` の paint・`windows_ime.rs` の IME（「たとえば」付きの例示）。
- 「確かめていないこと」に「診断用の `eprintln!`（`[config]`・`[history]` などのエラー行）は対象に含めない見込み。頻度が低く、計測の区間に入らないため」とある。
  **§1 の #5〜#7（`SNOTRA_EGUI_RENDER_ERROR` ×2・`SNOTRA_EGUI_DPI_CHANGED`）はこの「診断用」に当たる**（無条件・稀）。issue の方針に従えば対象外。
  ただし §5 案 A の lint を crate 全体に張るなら、この 3 箇所に `#[allow(clippy::print_stderr)]` + 理由が要る（または出口へ寄せる）。**どちらにするかが決める事項**。
  私見: 出口へ寄せても行の形は変わらず、例外の allow を置かずに済むので寄せるほうが単純。issue の「含めない見込み」は性能の理由であり、寄せることを禁じてはいない。
- コメントは読んでいない（`body` だけを取得）。計画・調査の内容は目にしていない。

### 切り詰めた出力に依っていた主張の測り直し

- 「17〜56ms」と、それに依る文: 概念語 `系を乱す|隣接する 2 行|runner では|1 事象あたり 2 行` で `PERFORMANCE.md snotra-egui-runtime src-tauri/src scripts docs` を切り詰めずに引いた。
  egui の計装に関わるのは `PERFORMANCE.md:3788`・`input.rs:22-24`・`input.rs:98-99`・`env.rs:7`（「計測コードは 1 事象あたり 2 行の stderr を挟むため、率も故障の現れ方も変える」）の 4 箇所。
  `env.rs:7` は所要の数値を持たず、行数の主張なので真のまま（変更不要）。残りの当たりは `scripts/` の runner 一般の話で無関係。
  ⚠️ `docs/superpowers/specs/2026-08-05-caret-test-mechanism-design.md:19`（計器が喪失の現れ方も変える旨）も同じ依存を持つが、日付付きの歴史記録なので触らない。
  ⚠️ 表記ゆれの正規表現 `17\s*[〜~–-]\s*56` は grep -E で 0 件を返した（既知の 3 箇所にも当たらず、正規表現が多バイトで効いていない）。結論は最初の `17[〜~-]56` の grep（3 箇所）と、上の概念語 grep の一致に依る。
- 消費者: §2 の根拠を、切り詰めなしの `grep -rn 'SNOTRA_EGUI\|SNOTRA_SMOKE' scripts .github` に差し替えた（§2 本文も修正済み）。

### 範囲外の走査の開示

- 許可範囲の外の `snotra-core/src`・`snotra-settings/src`・`snotra-core/Cargo.toml`・`package.json` を grep の対象に含めた（`eprintln!` の数と logger の有無を見るため）。`workspace/` はこの出力ファイル以外に触れていない。
- そこで得た事実: `snotra-core/src` に `eprintln!` 12 行（`config/io.rs` 5・`config/migrate.rs` 2・`history.rs` 2・`indexer/cache.rs` 1・`indexer/scan/tests.rs` 1・`window_data.rs` 1）、`snotra-settings/src/font.rs` に 1 行。
  **§5 案 A の lint をルートの `[workspace.lints.clippy]` に張ると、src-tauri の 11 箇所に加えてこの 13 行にも当たる。** crate の属性で egui-runtime に限る理由がここにもある。

### 検出器の穴と、構造で塞ぐ案

- lint（`print_stderr`）と「出口が 1 回で書く」テストでは、**呼び出し点が出口を 2 回呼ぶ形**（1 行を 2 回に分けて書く）と、**改行の付け忘れ**（呼び出し側が `\n` を足す設計の場合）を防げない。
- 案: **出口が改行を自分で足す**。`fn emit(args: std::fmt::Arguments)` で中で `String` に組んで `\n` を足し `write_all` 1 回、または crate 内マクロ（`eprintln!` と同じ引数を取る）にする。
  こうすると呼び出し点は `eprintln!(` → `<マクロ名>!(` の機械置換になり、**書式文字列が diff で無変更と読める**ので、受け入れ条件 (2) を差分の読みで確かめられる。
  改行の重複・欠落は出口のテスト 1 本で固定できる。⚠️ 「2 回呼ぶ」形は構造でも完全には防げない（レビューの対象）。
- ⚠️ `format_args!` を受ける形でも、`format!` の割り当てがゲートの内側に留まるかは呼び出し点の書き方に依る（今と同じ）。
