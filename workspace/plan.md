# 計画: issue #1539 — trace 1 行を先に組んで stderr へ 1 回で書く

## 目的と受け入れ条件

`trace()` が 1 行を字句ごとの多数の書き込みで出しているのを、`String` に組んで 1 回で書く形へ直す。
`SNOTRA_TRACE` 有効時の計測値（`update_us`・`egui_show:done` の `ms` など）に乗る書き込みの所要と揺れを減らす。

受け入れ条件（issue の 3 条件をテストできる形にしたもの）:

1. `trace()` の 1 行は、`std::io::Write` への `write` 呼び出し 1 回で書かれる（`[trace] ` 接頭辞・compact JSON・`\n` を同じバッファに含む）。
   単体テストで `write` の回数を数えて assert する。
2. 出力のバイト列は変わらない（`[trace] ` + `Value` の compact 表現 + `\n`）。単体テストで旧形（`format!("[trace] {line}\n")` 相当）と一致を assert し、
   `smoke:startup`・`smoke:egui`（`[trace]` を parse する）が通る。
3. `Start-SnotraProcess` と同じリダイレクト（`Start-Process -RedirectStandardError`）で、実アプリの `trace()` 1 行の所要を前後で測り、
   `PERFORMANCE.md` へ日付・機体・版・標本数つきで記録する。
4. trace の所要を前提にした現在形の記述（`PERFORMANCE.md`「計測と受け入れ基準」の当該箇条と子箇条、コードコメントの「ミリ秒単位」）を、
   測定値に合う形へ直す。日付つきの過去の記録（#1004・#1032・#1454・#1526 の節）は書き換えない。

## 変更するファイル

- `src-tauri/src/trace.rs`
  - `trace()`: 行の組み立てと書き込みを分ける。本体は「`format_line` で組み、`write_line(&mut std::io::stderr(), &s)` を呼ぶ」までに薄くする。
  - 新設 `fn format_line(seq, ts_ms, event, data, heap) -> String` 相当の純関数（`[trace] {line}\n` を返す。`heap` 欄の付け方は現行と同じ）。
    名前・引数の切り方は実装者が決めてよいが、`Value` を組むところと文字列化をこの関数に閉じる。
  - 新設 `fn write_line<W: std::io::Write>(w: &mut W, line: &str)`: `let _ = w.write_all(line.as_bytes());`（失敗は無視。下の「不変条件と異常系」）。
  - `#[cfg(test)] mod tests`: 下の「テスト方針」の 2 本。
  - `//!` に「1 行を 1 回で書く」理由と額の所在（`PERFORMANCE.md` の節名）を 1〜2 文足す。`trace()` の doc（「Emit one `[trace]` JSON line to stderr」）も、1 回で書くことが読める形に直す。
- `src-tauri/src/main.rs`: `ForegroundOutcome` の doc と `setup_hotkey_listener` の `t0` のコメントにある「trace 1 行の書き込み（ミリ秒単位）」から「（ミリ秒単位）」を外す
  （額は既に `PERFORMANCE.md` の節を指している）。trace を `egui_show:done` の後で書く配置（#1526）は変えない。
- `src-tauri/src/egui_shell/window_coordinator.rs`: 同じく「（ミリ秒単位）」を外す。配置は変えない。
- `PERFORMANCE.md`
  - 「計測と受け入れ基準」の「フレームループ中の trace は 1 行あたり約 10 ms かかる」の箇条と子箇条（「60fps の予算 16,700 µs は trace 2 本で超える」
    「区間ごとに trace を吐かない … 30 ms 級」「未調停の反例」「値は観測点で変わる」）: 現在形の主張を、#1539 の測定値に合う形へ直す。
    判定規則: 直した後の p50・p90・最大を見て、(a) 主張の数値（約 10 ms・2 本で 16.7 ms・30 ms 級）を新しい値で置き換えるか、
    (b) 上の峰が残るなら「1 回にしても残る裾」として残すかを決める。どちらでも、#1004/#1032 の値は「#1539 より前の値」と明記して残す。
    「A/B の差分で読む」「区間ごとに吐かない」の作法は、値が小さくなっても汚染がゼロではない限り残す。
    子箇条「未調停の反例」は、新しい値で説明がつくなら（1 行の費用が起動段の雑音に埋もれる規模なら）その旨へ直し、つかなければ残す。
    参照先の記録（「起動の端から端まで」の「計測コードは系を乱していない」）は日付つきの記録なので書き換えない。
  - 新しい小節「採用: trace 1 行を 1 回の書き込みにする（#1539）」を「採用: 表示の計測から trace の書き込みを外す（#1526）」の後に置き、
    測定条件・前後の表・読み方を書く。#1526 節の「回ごとに揺れる原因 … 確かめていない」には、新しい小節を指す 1 文を足す（記録本文は変えない）。

## 実装の順序

1. 前（A）の測定: 現行コードに一時の計装を入れたビルドで測る（下の「計測手順」）。A 側は実装前にしか取れない。
2. テストを先に書く（Red）: `write_line` に旧形相当を入れた状態では回数のテストが落ちることを確かめる（例: `write!(w, "[trace] {line}\n")` は複数回になる）。
3. 実装（Green）: `format_line` / `write_line` / 薄い `trace()`。
4. 後（B）の測定: 同じ計装を新コードへ入れたビルドで、A と交互に測る。
5. 計装を撤去する前に、値を `PERFORMANCE.md` へ記録する。計装を撤去する。
6. `PERFORMANCE.md` とコメントを直す。
7. 検証コマンドを回す。

## 計測手順（一時の足場・コミットしない）

- 計装: `trace()` の中で、行の組み立て開始から書き込み完了までを `Instant` で計り、その値を**次の行**の `data` とは別の欄 `prev_emit_us` として出す
  （計った値を同じ行へ書くと、その行の書き込みを計れない。蓄積して後の行へ載せることで、計時そのものは書き込みの外に置く）。
  A（`eprintln!`）では `eprintln!` 全体を、B では `format_line` + `write_line` 全体を計る（どちらも「組み立て + 書き込み」を含む同じ区間）。
- ビルド: A と B を別々の worktree・別々の `CARGO_TARGET_DIR` で release ビルドする（`PERFORMANCE.md`「計測と受け入れ基準」の、変種ごとの独立ビルド）。
- 起動: scratchpad に置く一時の pwsh スクリプトで `scripts/lib/SnotraSmoke.psm1` を import し、`Start-SnotraProcess -Trace -StandardErrorPath <temp>` で起動する
  （smoke と同じリダイレクト）。検証用プロファイルは `New-SnotraVerificationProfile`。
- 操作: `Send-SnotraKeyChord` でホットキーの表示・非表示を各 25 回、表示中に数文字打ってフレームループ上の行（`egui_frame` など）を出す。
- 順序: A → B → A → B の 2 巡（同日・同機体）。
- 集計: 全行の `prev_emit_us` の p50・p90・最大と、標本数。経路別（フレームループ中の行、`hotkey:listener_enter`）にも出す。生ログはリポジトリの外（scratchpad）に置く。
- 記録: 日付・機体（`hostname` で HAL9000 を確認済み）・release・`SNOTRA_TRACE=1`・リダイレクトの形・標本数・A/B の版（コミット sha）。
- 撤去: 計装はコミットしない。撤去条件は「B 側の測定値を `PERFORMANCE.md` へ記録したとき」。worktree と一時スクリプトは記録の後に消す。

## 不変条件と異常系

- 出力形式（`[trace] ` + compact JSON + `\n`）を変えない。消費者は `scripts/lib/SnotraSmoke.psm1`・`SnotraTraceInvariants.psm1`・`smoke-startup.ps1`。
- `seq` の単調性と `heap` 欄の有無の規則（`heap_trace.rs` の `//!`）を変えない。
- 行の原子性: `Stderr::write_all` はロックを取り、1 回の書き込みになるので、他の stderr 出力と行の途中で混ざらない（現行の `eprintln!` と同等以上）。
- 書き込み失敗: `eprintln!` は panic し、release（`panic = "abort"`）ではプロセスごと abort していた。新しい形は無視し、診断が黙って欠ける。
  診断用の出口で、製品を trace の失敗で止めない。失敗を待つ検査は無い。release（`windows_subsystem = "windows"`）で stderr が無いとき
  std は黙って成功を返すので、通常の経路では差が出ない。この挙動の変化は PR 本文に書く。
- `heap-trace` ビルド: 行の `String` 確保が `heap_trace::snapshot()` の後に起きるので、次の行の確保回数に 1 行分が乗る。
  前後でヒープ欄を比べる人のために PR 本文に書く（`String::with_capacity` で再確保を減らすかは実装者が決めてよい）。
- `SNOTRA_TRACE` 無効時は早期 return のまま（組み立ても書き込みもしない）。

## テスト方針と検証コマンド

- 単体テスト（`src-tauri/src/trace.rs` の `tests`）:
  1. `write_line` に `write` の回数を数える `Write` 実装を渡し、回数が 1 であることを assert する（受け入れ条件 1）。
  2. `format_line` の出力が `format!("[trace] {}\n", <同じ Value>)` と一致し、`\n` で終わり、改行が 1 つだけであること、
     `[trace] ` を剥いだ本文を `serde_json::from_str` で読み直すと元の `Value` に戻ることを assert する（受け入れ条件 2）。
- このテストが**見ていないもの**: `trace()` 本体が `write_line` を経ずに `eprintln!` へ戻る変更は、テストを緑のまま通す。
  対策として `trace()` 本体を薄く保ち、レビューで見る。残る穴は PR 本文の「見ていないもの」に書く。
- 変異注入の候補（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）:
  - `write_line` を `w.write_all(prefix)?; w.write_all(body)?` の 2 回に分ける → テスト 1 が落ちる。
  - `writeln!(w, "[trace] {line}")` で W へ直接書く → テスト 1 が落ちる（断片ごとの書き込み）。
  - `format_line` から `\n` を落とす → テスト 2 が落ちる。
- 検証コマンド: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（Rust）、C（`smoke:startup`・`smoke:egui`。`[trace]` を parse して出力形式の同一を端から端まで確かめる）、
  F（`PERFORMANCE.md` を変えるので `governance:check`）。

## SPEC.md・関連文書の更新

- `SPEC.md`: 変更なし（trace は診断用で、仕様の挙動を変えない）。
- `PERFORMANCE.md`: 上記。
- `src-tauri/CLAUDE.md`: モジュール構成の `trace.rs` の 1 行は変わらない（ファイルの追加・削除なし）。

## 作業項目

### Phase 1 — 前の測定

- [x] A 側の一時計装を入れた release ビルドを、専用 worktree と `CARGO_TARGET_DIR` で作る
- [x] 測定スクリプト（scratchpad）を書き、A 側を 1 回走らせて集計が出ることを確かめる

### Phase 2 — 実装

- [x] `trace.rs` に `format_line`・`write_line` とテスト 2 本を書き、旧形の `write_line` でテスト 1 が落ちることを確かめる（Red）
- [x] `trace()` を `format_line` + `write_line(&mut std::io::stderr(), ..)` へ書き換え、テストを通す（Green）
- [x] `trace.rs` の `//!` に 1 回で書く理由と `PERFORMANCE.md` の節名を足す
  - 実装記録: 組み立ての純関数は `render_line(&Value) -> String` とした（`Value` は従来どおり `trace()` が組み、`heap` 欄の snapshot の位置も変えない）。
    Red は `render_line` が空文字・`write_line` が no-op のスタブで 2 本とも落ちることで確かめた（旧形を当てる変異は委譲先が行う）

### Phase 3 — 後の測定と記録

- [x] B 側の一時計装ビルドを作り、A → B → A → B の 2 巡で測る
- [x] `PERFORMANCE.md` に「採用: trace 1 行を 1 回の書き込みにする（#1539）」を書き、#1526 節に新小節を指す 1 文を足す
- [x] 一時計装・worktree・測定スクリプトを撤去する
  - 測定記録（2026-11-25・HAL9000・release・各 25 回・502 行）: 全行 p50 3,186/3,159 µs → 138/140 µs、`egui_frame` 2,508/2,499 → 114/117 µs。
    `hotkey:listener_enter` は後でも約 8〜10 ms の峰が残った（B の p50 445 / 8,321 µs）。判定規則の (b) を採り、「1 回にしても残る裾」として残した。
    「未調停の反例」は新しい値で説明がつかない（反例は #1539 より前の版のデータ）ので、規模の書き方だけ直して残した。
    #1526 節の 736 行付近は、無くなった文言を引いていたので「#1539 より前に記していた」へ直した

### Phase 4 — 記述の直し

- [x] `PERFORMANCE.md`「計測と受け入れ基準」の trace の所要の箇条と子箇条を、判定規則に従って直す
- [x] `main.rs`（2 か所）と `window_coordinator.rs`（1 か所）の「（ミリ秒単位）」を外す
- [x] 「1 行あたり」「約 10 ms」「trace 2 本」「ミリ秒単位」などの語で `git grep` し直し、残った現在形の主張が無いことを確かめる

### Phase 5 — 検証

- [ ] カテゴリ A・C・F のコマンドを回す

## 未確定（実装前に潰す）

- [x] 敵対的調査（`workspace/adversarial-1539.txt`）の所見を反映する — 採否は `research.md`「敵対的調査（3b）の反映」。
  計画へは、`panic = "abort"` から黙殺への変化と `heap-trace` の確保回数を「不変条件と異常系」へ足した。測定は実アプリ・release・`-StandardErrorPath` 付き（計測手順のとおり）

## 承認

- [x] 自己承認 — 2026-11-25 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まない（trace は診断用の出口で、出力のバイト列も変えない） / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない（`[trace]` 行の形式は不変） / 人の裁定: なし

## plan-review 結果

- リスク: 高（ガバナンス文書 `PERFORMANCE.md` を変更する）
- レビュー方式: 独立導出1体（`workspace/plan-review-1539-derive.md`。計画・調査は読んでいないと申告）
- エージェント数: 1

### 要対処
- `trace()` の doc が変更ファイルの対象シンボルに無かった — 計画の修正（`trace.rs` の項へ追加） — 導出の「`trace()` の doc（40–43 行）」を `trace.rs` で確認
- テスト 2 に JSON の読み直しが無かった — 計画の修正（テスト方針へ追加） — 導出の「本文を JSON として読み直すと元の値に戻る」

### 軽微
- 導出は `PERFORMANCE.md` の 735–736・768 行（#1526 節）を直す側に挙げた。計画は日付つきの記録として本文を変えず、#1526 節に新小節を指す 1 文を足す。判断の不一致だが、規約（「この文書へ記録するときの規約」の時系列の採否ログ）に従い計画のままとする
- 導出の 3730–3734 行（「未調停である」）は日付つきの起動の記録の中にある。計画は正本側の子箇条「未調停の反例」を測定結果で直す形にした（変更するファイルの `PERFORMANCE.md` の項に追記）
- 書き込み失敗の扱い（導出 ⚠️1）は、計画の「不変条件と異常系」で黙殺に決めてある（release の `panic = "abort"` からの変化も記載）
- AC2 を代理のプログラムで満たさない（導出 ⚠️4）: 計画は実アプリで測る
- plan ∖ 導出: なし（導出は計画の変更ファイルをすべて挙げた）

### 未検証
- なし

### 判断
- 実装着手: 可

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2b 独立導出）
- エージェント数: 1（3b の敵対的調査と合わせて 2）
- 要対処: 2 件（`trace()` の doc を対象へ追加・テストに JSON の読み直しを追加）
- 未検証: なし（独立レビュー後に入った差分は要対処 2 件の反映だけで、どちらも独立導出が自ら挙げた項目なので、追加のレビューは同じ導出で済んでいる）
- check スキル: なし（関数の新設に伴う `/dry-check` は実装時に `/implement` が回す。`/race-check` は計画段階では起動しない）
- 散文の全件列挙: 語 3 / 総件数 21 行 / ★未掲載 0（計画へ足した 0） / ☆行未掲載 0（計画へ足した 0）
