# 計画: issue #1542 — egui-runtime の計装の 1 行を stderr への 1 回の書き込みにする

## 目的と受け入れ条件

`snotra-egui-runtime` が stderr へ書く行を、先に `String` へ組んでから `write_all` 1 回で書く形にする。std の `Stderr` はバッファを持たず、`eprintln!` は書式の断片ごとに書くので、リダイレクト先の write-through の単価が断片の数だけ掛かっている。

- AC1: crate の stderr の出口は 1 つで、1 行を `write` 1 回で書く。`CountingWriter` で数えるテストが確かめる。対照として、同じ書式を `writeln!` で書くと 2 回以上になることも同じテストで確かめる（数える仕組みが分割を見分けられることの確認）
- AC2: crate の中に `eprint!` / `eprintln!` が残らない。`lib.rs` の `#![deny(clippy::print_stderr)]` が確かめる（CI の clippy）
- AC3: 行のバイト列は変えない（接頭辞・`key=value` の並び・数値の書式・末尾の `\n` 1 つ）。呼び出し点の書式文字列と引数は一字も変えず、マクロ名だけを置き換える。smoke（`smoke:startup` / `smoke:egui`）と Pester が通る
- AC4: `SNOTRA_EGUI_INPUT_TRACE` の 1 行の所要を、smoke と同じリダイレクト（`Start-SnotraProcess -StandardErrorPath`）で前後に測り、`PERFORMANCE.md` に記録する。「17〜56ms」の記述は、測り方（CI runner・#1539 より前の版・run 30963445957・隣接する 2 行の間隔で、どの行の間かと p50 か外れ値かは記録が無い）を明記した過去の観察として残す。新しい値とは 1 つの数にまとめない（「値は観測点で変わる」の項と同じ扱い）。「この計測コードは系を乱す」「率を測る回と機序を測る回は別の回にする」は、新しい値で 1 行の所要が縮んでも規範として残す（新しい値はローカルの機体で測るもので、runner では測り直さない。runner で乱さなくなったことは示せない）

## 決めたこと

### 出口の置き場: `snotra-egui-runtime` の中に置く（`src-tauri/src/trace.rs` と共有しない）

- 共有できるのは `let _ = w.write_all(line.as_bytes())` の 1 行だけである。組み立ては別物で、`trace.rs` は `[trace] ` + JSON + `seq` + `heap`、こちらは書式文字列そのままの素の行である
- 依存の向き: `src-tauri` は `snotra-egui-runtime` に依存し、逆は無い。共有するなら `snotra-egui-runtime` の公開 API に stderr の書き込みを足すか、1 行のために crate を新設することになる。描画ランタイムの公開面に診断の出口を載せる理由が無い
- 片方だけが変わる将来: `trace.rs` は seq の全順序と heap 標本を持つ構造化 trace で、出力先や組み立てを替える（たとえば背景スレッドへ書き込みを逃がす）なら `trace.rs` だけを変える。egui の計装ハッチは、呼んだスレッドで即時に書くこと（`WAKE_SEND` と `WAKE_RECV` の時刻の照合）に意味がある
- 双子であることは両方の doc に書く（`/dry-check` が重複として拾ったとき、理由がそこにある）

### 対象: crate の `eprintln!` 11 件をすべて移す（`SNOTRA_EGUI_RENDER_ERROR` 2 件・`SNOTRA_EGUI_DPI_CHANGED` も含める）

issue は診断用のエラー行を対象外とする見込みと書いた。理由は「頻度が低く、計測の区間に入らない」で、性能の上では移す必要が無いという意味である。それでも移すのは、AC2 の lint を例外なしで置くためである。`#[allow]` を残すと、その場所に計装を足したときに lint が黙る。3 件は書式も引数も変えずにマクロ名だけを替えるので、行は変わらない。書き込みに失敗したときに panic しなくなる点だけが変わり、これは `trace.rs` の `write_line` と同じ扱いである。`src-tauri` の `[config]`・`[history]` などの行は crate が違うので触らない。

### 形: `format_args!` を受ける関数と、それを包む crate 内マクロ

- `stderr_line.rs` に `render_line(args: fmt::Arguments<'_>) -> String`（`format!("{args}\n")`）と `write_line<W: Write>(w, line: &str)`（`write_all` 1 回・失敗は捨てる）を置く。`trace.rs` と同じ分け方である
- 呼び出し点は `trace_line!(...)` と書く。マクロは `$crate::stderr_line::emit(format_args!($($arg)*))` に展開する。`format_args!` の一時値は同じ文の中で使い切る
- マクロはパスで引く: `stderr_line.rs` で `macro_rules! trace_line` の直後に `pub(crate) use trace_line;` を置き、使う側のファイルは `use crate::stderr_line::trace_line;` で取り込む。`#[macro_use]` と `mod` の宣言順には頼らない（rustfmt は `mod` 宣言を名前順に並べ替えるので、順に頼る形は保てない）。scratchpad の最小 workspace（toolchain 1.99.1）で、使う側の `mod` を `stderr_line` より前に宣言した形が `cargo fmt` 後の clippy（`-D warnings`）を exit 0 で通ることを測った
- 有効判定（`trace_hatch_enabled` / `input_trace_enabled`）は呼び出し側に残す。未設定なら計装の費用が 0 であること（`PERFORMANCE.md`「計測と受け入れ基準」の egui の計装の項）を変えない
- 行の原子性: `Stderr::write_all` は 1 呼び出しの間ロックを持ち、部分書き込みのループもロックの内側にある（`eprintln!` も 1 行の間持っていた。敵対的調査が std の `stdio.rs` を読んで確かめた）。repaint worker スレッド（`WAKE_SEND`）とイベントループの行が互いに割り込まないことは変わらない
- テストは本物の stderr へ書く `emit` ではなく、`W: Write` を受ける `emit_to` を測る（`stderr().write_all` は libtest の出力の捕捉に乗らないので、`emit` をテストから呼ぶとテストの出力に生の行が混ざる）

## 変更するファイル

- `snotra-egui-runtime/src/stderr_line.rs`（新設）: `//!`・`render_line`・`write_line`・`emit`・`trace_line!`・テスト（`CountingWriter`）
- `snotra-egui-runtime/src/lib.rs`: `mod stderr_line;`（宣言の位置は問わない。マクロはパスで引く）と `#![deny(clippy::print_stderr)]`（理由の 1 行コメント）
- `snotra-egui-runtime/src/input.rs`: `input_trace` の `eprintln!` → `trace_line!`。`last_take_trace` の doc と `take` のコメントにある「17〜56ms」を、数を写さず `PERFORMANCE.md`「計測と受け入れ基準」の `SNOTRA_EGUI_INPUT_TRACE` の項を指す形にする（間引く理由そのものは残す）
- `snotra-egui-runtime/src/renderer.rs`: `SNOTRA_EGUI_PAINT`
- `snotra-egui-runtime/src/repaint.rs`: `SNOTRA_EGUI_WAKE_SEND`
- `snotra-egui-runtime/src/runtime.rs`: `SNOTRA_EGUI_WAKE_RECV`・`SNOTRA_EGUI_RENDER_ERROR`（2 件）・`SNOTRA_EGUI_DPI_CHANGED`・`SNOTRA_EGUI_REPAINT`
- `snotra-egui-runtime/src/windows_ime.rs`: `SNOTRA_EGUI_IME_RECT`・`SNOTRA_EGUI_IME_COMMIT`・`SNOTRA_EGUI_IME_PREEDIT`
- `snotra-egui-runtime/CLAUDE.md`: 「モジュール構成」へ `stderr_line.rs` を足す
- `src-tauri/src/trace.rs`: `write_line` の doc に、同じ形が `snotra-egui-runtime/src/stderr_line.rs` にあり、共有しない理由はそちらの `//!` にある、と 1 文足す（コードは変えない）
- `PERFORMANCE.md`: 「計測と受け入れ基準」の `SNOTRA_EGUI_INPUT_TRACE` の項を直す。#1539 の節の後ろに「採用: egui の計装の 1 行を 1 回の書き込みにする（#1542）」の節を足し、測定条件と表を置く

## 実装の順序

1. `stderr_line.rs` とテストを書く（Red: マクロ・関数が無くてコンパイルが通らない → Green）
2. 11 件を移し、lint を置く
3. 文書（crate の `CLAUDE.md`・`trace.rs` の doc・`input.rs` のコメント）
4. 測定（前後のビルド・一時の計装・回す・撤去）
5. `PERFORMANCE.md`
6. 検証

## 不変条件と異常系

- 行のバイト列が変わらない: 書式文字列リテラルと引数を変えない。`render_line` のテストは `format!` + `"\n"` と一致することを見る。各呼び出し点の変更がマクロ名だけであることを、Phase 2 の時点で `git diff -w --word-diff=plain -- snotra-egui-runtime/src/renderer.rs snotra-egui-runtime/src/repaint.rs snotra-egui-runtime/src/runtime.rs snotra-egui-runtime/src/windows_ime.rs snotra-egui-runtime/src/input.rs` で確かめる。`-w` は rustfmt の折り返し（マクロ名が 2 文字長くなる）を無視するためである。変わった語が `eprintln!` → `trace_line!` と、`use crate::stderr_line::trace_line;` の行と、`input.rs` のコメントの直しだけであることを読む（新設の `stderr_line.rs` と `lib.rs` はこの確認の対象外）
- 未設定のハッチの費用 0: 判定を呼び出し側に残す。`trace_line!` の中で env を読まない
- 書き込みの失敗: 捨てる（panic しない）。`trace.rs` と同じ
- 行が他のスレッドの行と混ざらない: `write_all` 1 回で、`Stderr` のロックの下で書く

## テスト方針と検証コマンド

- `stderr_line.rs` の単体テスト:
  - `emit_to`（`W: Write` を受ける版）へ 13 引数の書式（`SNOTRA_EGUI_PAINT` と同じ形）を渡すと `writes == 1`、バイト列は `format!(..) + "\n"` と一致
  - 対照: 同じ書式を `writeln!(&mut CountingWriter, ..)` で書くと `writes >= 2`（数える仕組みが分割を見分けること）
  - `render_line` の末尾の改行は 1 つだけ
- 変異注入（`/implement` の委譲先が行う）:
  - `input.rs` の `trace_line!` を `eprintln!` へ戻す → `cargo clippy -p snotra-egui-runtime --all-targets -- -D warnings` が `print_stderr` で赤になる（コンパイルエラーではなく lint の診断であることを出力で確かめる）
  - `write_line` を `for b in line.as_bytes().chunks(8) { w.write_all(b) }` に替える → AC1 のテストが `writes == 1` の assert で赤になる
- 検証: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（Rust）・C（`test:powershell`・`smoke:startup`・`smoke:egui`。描画ループを所有する crate を触るので `smoke.yml` も走る）・F（`governance:check`）。doc コメントを触るので、カテゴリ A の `cargo doc` 行を手で走らせる

## 測定（AC4）

- 版: 前 = `origin/main`（`bfbc0385`）、後 = 実装のコミット。別々の worktree と `CARGO_TARGET_DIR` で release をビルドする（`PERFORMANCE.md`「計測と受け入れ基準」のビルドごとの揺れの項）
- 一時の計装（コミットしない）: 両方の版の `input_trace` に、「行の組み立て + 書き込み」を `Instant` で計時し、次の行の末尾へ ` emit_us=<前の行の所要>` を足す計装を入れる（#1539 と同じ測り方）。両方の版で同じ文字列を足すので、行の長さの差は両側に等しく乗る
- 起動: `Start-SnotraProcess -StandardErrorPath`（smoke と同じリダイレクト）・検証用プロファイル（`New-SnotraVerificationProfile`）・`-ExtraVariables @{ SNOTRA_EGUI_INPUT_TRACE = '1' }`。ビルドは release（smoke の既定は debug なので、本体のパスを明示する）。`SNOTRA_EGUI_INPUT_TRACE` を立てて回すスクリプトは `scripts/` に無いので、駆動スクリプトを scratchpad に書く。打鍵の区間では呼び出し側の env も立てる（`Send-SnotraKey` が注入時刻の行を出すのは呼び出し側の env を読むときだけ・`SnotraSmoke.psm1` の #999 のコメント）
- 標本数: 最初に前の版を 1 回だけ回し、1 操作で `SNOTRA_EGUI_INPUT` の行が `kind` ごとに何本出るかを数えてから本番を回す（数は節に書く）
- 操作: Alt+F12 で表示 → 3 文字打鍵（`Send-SnotraKey`）→ Escape で非表示。これを 1 回とし、25 回で 1 巡。前 → 後 → 前 → 後の順に 2 巡ずつ
- 集計: `kind` ごと（`take`・`rx_key`・push 系）と全行の p50 / p90 / 最大（µs）、標本数。あわせて、隣接する 2 行の `ts_ms` の間隔（17〜56ms と同じ測り方）の p50 / p90 も前後で出す
- 駆動スクリプトは scratchpad に置き、コミットしない。計装は測定用の worktree の中にだけ入れ、測定が終わったらその worktree ごと撤去する（`git worktree remove`）。撤去したことを `git worktree list` で確かめる

## SPEC.md・関連文書の更新

- `SPEC.md`: 不要（挙動を変えない）
- `PERFORMANCE.md`: 要（上記）
- `snotra-egui-runtime/CLAUDE.md`: 要（モジュール構成）
- `docs/architecture.md`: 不要（横断パターンを変えない）

## 作業項目

### Phase 1 — 出口

- [x] `snotra-egui-runtime/src/stderr_line.rs` を書く（`//!`・`render_line`・`write_line`・`emit_to`・`emit`・`trace_line!`）
- [x] テスト（AC1 と対照）を書き、Red → Green を確かめる
- [x] `lib.rs` に `mod stderr_line;` と `#![deny(clippy::print_stderr)]` を置く

### Phase 2 — 呼び出し点の移行

- [x] 11 件をマクロ名だけ替えて移し、使う側の 5 ファイルに `use crate::stderr_line::trace_line;` を足す
- [x] `git diff -w --word-diff=plain`（「不変条件と異常系」の形）で、変わった語がマクロ名と `use` 行とコメントだけであることを確かめる
- [x] `git grep -n "eprintln!\|eprint!" -- snotra-egui-runtime` がコメント以外 0 件であることを確かめる

### Phase 3 — 文書

- [x] `snotra-egui-runtime/CLAUDE.md` のモジュール構成へ `stderr_line.rs` を足す
- [x] `src-tauri/src/trace.rs` の `write_line` の doc に双子の 1 文を足す
- [x] `input.rs` の 2 か所の「17〜56ms」を `PERFORMANCE.md` の項を指す形にする

### Phase 4 — 測定

- [x] 前後の版を別々の worktree・`CARGO_TARGET_DIR` で release ビルドし、一時の計装を入れる（前 `bfbc0385`・後 `9c7b3f94`。scratchpad の `m-before` / `m-after`）
- [x] 前の版を 1 回だけ回し、1 操作あたりの `SNOTRA_EGUI_INPUT` の行数を `kind` ごとに数える（35 行: `take` 11・`rx_key` 13・`push_key` 8・`push_text` 3。起動直後の `take` を含む）
- [x] 前 → 後 → 前 → 後で回し、集計する（各 1 巡 25 回・約 924 行。全行 p50: 前 536 / 515 µs → 後 115 / 113 µs。`push_key` の down の 30〜40 / 100 件が前後とも約 8〜10 ms。表は `PERFORMANCE.md` の #1542 の節）
- [x] 測定用の worktree を撤去し、`git worktree list` で残りが無いことを確かめる（scratchpad の行 0 件）

### Phase 5 — `PERFORMANCE.md`

- [x] 「採用: egui の計装の 1 行を 1 回の書き込みにする（#1542）」の節を、#1543 の節の後ろに足す（測定条件・表・読み方。#1543 の write-through を前提に読むので、#1539 の直後ではなくその後ろに置いた）
- [x] 「計測と受け入れ基準」の `SNOTRA_EGUI_INPUT_TRACE` の項を直す
- [x] 同じ節の trace の所要の項にある「CI runner での実測は同節の `SNOTRA_EGUI_INPUT_TRACE` の項にある」という所在の文を、直した項の文言と合わせる

### Phase 6 — 検証

- [x] カテゴリ A（`cargo fmt --check`・`cargo clippy --workspace --all-targets -- -D warnings`・`cargo test`・`cargo doc`）
- [x] カテゴリ C（`npm run test:powershell`・`npm run smoke:startup`・`npm run smoke:egui`）
- [x] カテゴリ F（`npm run governance:check`）

### 検証ラウンドの記録

- ラウンド 1: アンカー `076051a8` / ツール green（A・C・E・F） / 委譲 `review-1542`: Critical・High 0、Low 7（⚠️ を含む） / 変異注入: 狙った 4 件（`eprintln!` 戻し・`chunks(8)`・改行落とし・`eprint!`）はすべて赤（lint 診断または assert）。射程外の 2 件は緑で生き残り: `writeln!(stderr())`・`dbg!`（lint の射程外。lib.rs のコメントに明記した）と、`emit` の宛先を `sink()` へ替える変異（宛先を見る検知器が無い。設計上テストしない） / 実地: `[lints] workspace = true` の下での deny を本物の crate で確認・word-diff 手順・run ログの失効（HTTP 410） / 直したもの: Low 1（lib.rs のコメントに lint の射程を書いた）・Low 7（runner の観察の版に、`[trace]` 行も断片ごとに書いていた留保を足した）。直さなかったもの: Low 2（標本数の 1 件差）・Low 3（約 5 分の 1 → 実比 4.6）・Low 4（plan の「2 巡ずつ」）・Low 6（テストの doc の「13」）は精度・言い回しだけ。Low 5 は検知器の所在の記録

- ラウンド 2: アンカー `698f4808` / ツール green（A・E・F） / 委譲 `review-1542`: Low 1・Low 7 の解消を確認、新しい指摘 0 / 変異注入: 散文だけの変更なので新しい変異なし。lib.rs の新しい文の根拠（変異 4a: `writeln!(stderr())`・`dbg!` は緑、対照の `eprintln!` は赤）を 698f4808 で回し直して一致 / 実地: 上の clippy 2 回
- 軸の違うレビュー: 立った条件 (2)（`#![deny(clippy::print_stderr)]` という見張りを足した） / 対象 `perf/egui-trace-single-write` / `/code-review high` 5 件（すべて nit） / 直した 1 件: 「唯一の出口」の全称（crate の `CLAUDE.md` と `stderr_line.rs` の `//!`）を、lint の射程を添えた形へ弱めた / 残した 4 件: 表の標本数の 1 件差・「約 5 分の 1」（実比 4.6）・テスト doc の「13」・plan の「2 巡ずつ」——精度と言い回しだけで、読み手の行動を変えない

## 未確定（実装前に潰す）

- [x] 敵対的調査（`workspace/adversarial-1542.txt`）の所見を反映する — 採否は `research.md`「敵対的調査（3b）の採否」。計画へは B（AC4 の「17〜56ms」の扱い）・D（測定の駆動スクリプトと env）・⚠️（release に揃える・テストは `emit_to` を測る）を入れた
- [x] `[lints] workspace = true` の下で `lib.rs` の `#![deny(clippy::print_stderr)]` が効くこと — scratchpad の最小 workspace で測った。`eprintln!` に `error: use of eprintln!`（exit 101）、`stderr().write_all` には立たない。本物の crate での発火は Phase 2 の変異注入で確かめる
- [x] 測る操作の 1 回で `SNOTRA_EGUI_INPUT` の行が何本出るか — 分岐ではなく測定の最初の手順なので、Phase 4 の作業項目へ移した
- [x] 独立導出（`/plan-review`「Step 2b」）で、変更するファイル・シンボルの漏れと過剰を突き合わせる — `workspace/plan-review-1542-derive.md`。委譲先は計画と調査を目にしていないと書いた。突き合わせの結果は「セルフレビュー」の節

## 承認

- [x] 自己承認 — 2026-11-26 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まず、行のバイト列も変えない / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない（stderr の診断行だけ） / 人の裁定: なし

## セルフレビュー

- リスク: 高（`PERFORMANCE.md` の変更と、呼び出し点の全件移行という網羅性）
- plan-review: 独立レビュー1体（Step 2b の独立導出）
- エージェント数: 1（3b の敵対的調査を含めると 2）
- 要対処: 3 件 — (1) 導出 ∖ 計画に、`PERFORMANCE.md` の trace の所要の項にある所在の文があった。Phase 5 に作業項目を足した (2) マクロをパスで引く形に決めた (3) 呼び出し点の差分の確かめ方を `git diff -w --word-diff=plain` に直した（詳細は「plan-review 結果」）
- 突き合わせ:
  - 導出 ∖ 計画（採らなかったもの）: 「行頭が `[trace]` でないこと」のテスト（出口は接頭辞を足さず、呼び出し点の書式文字列を変えないので、マクロ名以外の差分 0 行の確認で足りる）、変更前の各接頭辞の実際の 1 行の記録（同じ理由）
  - 計画 ∖ 導出: `src-tauri/src/trace.rs` の doc に双子の 1 文を足す項目。導出は「共通にしない限り不要」とした。issue が「書かないと `/dry-check` で重複として指摘される」と求めているので残す
  - 判断の一致: 置き場（crate の中）、lint（`#![deny(clippy::print_stderr)]`）、`fmt::Arguments` を受ける出口と crate 内マクロ、「17〜56ms」は隣接 2 行の間隔で書き込み 1 回の所要ではないこと、release に揃えること
  - 導出が ⚠️ で残した「無条件の 3 件を含めるか」: 計画は含める（「決めたこと」の対象の項。lint に例外を作らないため）。issue は「見込み」とだけ書き、判断を調査へ委ねている
  - 導出が ⚠️ で残した「runner を測り直すか」: 測り直さない。AC4 のとおり、条件の違う過去の観察として残す
- 未検証: lint の発火は scratchpad の最小 workspace で測っただけで、本物の crate では Phase 2 の変異注入で測る
- 失敗時の挙動の変更（EBADF 以外の書き込みの失敗で panic しなくなる）は、PR 本文に書く事項である
- check スキル: なし（`/race-check` は計画段階では起動しない。新しい対称ペア・永続形式・UI モードは無い。`/dry-check` の観点は置き場の理由で先に答えた）
- 散文の全件列挙: 語 4 / 総件数 68 行 / ★未掲載 19（計画へ足した 0） / ☆行未掲載 0（計画へ足した 0）。語は `eprintln`・`17〜56`・`SNOTRA_EGUI_INPUT_TRACE`・`write_line`。★19 件の判定: `src-tauri`（`trace.rs` を除く）・`snotra-core`・`snotra-settings` の `eprintln!` とその説明のコメント、`scripts/` の `[config]` の `eprintln` を説明するコメント（`smoke-egui.ps1`・`smoke-startup.ps1`・`visual-check-colors.ps1`）、`SnotraSmoke.psm1` の呼び出し側 env の分岐——どれもこの crate の外の出力か呼び出し側の env を述べており、変更後も真である。生きた文書の `PERFORMANCE.md` 6 行のうち、#1539 の節（776・778）と #1004/#1032 の段落（3765）・WebView2 期の項（3722）は歴史を述べて真のまま、3778 と 3788 は計画に載っている。`src-tauri/src/trace.rs` の 18・82 行（`eprintln!` が分割する・失敗で panic する）は変更後も真

## plan-review 結果

- リスク: 高
- レビュー方式: 独立導出1体
- エージェント数: 1

### 要対処
- `PERFORMANCE.md` の trace の所要の項にある所在の文 — 計画の修正（Phase 5 に作業項目を足した） — `prose:sweep` の 3778 行で再照合した
- マクロの見せ方が rustfmt の `mod` 並べ替えに耐えるか — 計画の修正（パスで引く形に決めた） — scratchpad の最小 workspace で clippy exit 0 を測った
- 呼び出し点の差分の確かめ方 — 計画の修正（`git diff -w --word-diff=plain` で、Phase 2 の時点で読む） — 元の `grep -v` の形は `+++`/`---` 行・`use` 行・コメントを必ず拾い、0 行にならないため

### 軽微
- 導出 ∖ 計画の「行頭が `[trace]` でない」テスト・変更前の実行行の記録は採らない（セルフレビューの突き合わせ）

### 未検証
- lint の本物の crate での発火（Phase 2 の変異注入で測る）

### 判断
- 実装着手: 可
