# research — #757 results 窓の目視ハーネス（trace 不変条件 H1/H4/H5）

## issue の要約

**#757 は「検討:」issue であり、defer の trigger は「段 1（#749）・段 3（#666）で実際にもう一度使うことが見えてから」だった。** 両方とも CLOSED（#749 = 2026-07-27 / #666 = 2026-07-27）ゆえ trigger は発火済み。

issue 本文の「判断すること」2 点のうち、

1. **「そもそも `scripts/` へ入れるか」は #749（PR #759・`5ef346f`）で決着済み**——`scripts/manual-smoke.ps1` として入り、`package.json` の `smoke:manual`・`docs/build-commands.md` カテゴリ D・`.claude/skills/implement/SKILL.md` まで配線されている
2. ゆえに 2 点目（入れないなら docs のどこへ）は成立しない

**残るのは issue 作者コメントが絞り込んだ本体 3 つだけである**（2026-08-01 時点で 1 つも実装されていない）。

1. 区間マーカーを打つ（各項目の**開始前**に trace 上の位置を記録する）
2. H1 / H4 / H5 を判定する。**hide 側は要求レベルゆえ連続してよい**という非対称を織り込む
3. 判定できた項目は目視の結果と**並べて**出す（trace が緑でも目視が赤なら赤）

加えてコメントが「二重の正本」として挙げた `docs/build-commands.md` の 1 行がある（後述）。

2026-08-01、実装する方向でユーザーの承認を得た。

## 判定する 3 不変条件（issue 本文が本体と呼ぶもの）

| # | 判定 | 何を捕まえるか |
|---|---|---|
| H1 | `egui_hide:done` の後、次の `egui_show:done` より前に `egui_results:show` が現れたら異常 | #671 PR A′ の事故そのもの（main が hidden なのに results が最前面に残る） |
| H4 | `egui_results:show` の `rows` が 0 なら異常 | 「高さ 0 ⇔ hide」の契約違反 |
| H5 | hide を挟まない連続 `egui_results:show` は異常 | 二重発火抑止（`ResultsWindow.visible` の `swap`）の破れ |

### コードで裏取りした前提（すべて実在を確認済み）

- `src-tauri/src/trace.rs:39` `trace()` は `eprintln!` 直書き。**Rust の `std::io::Stderr` は無バッファゆえ 1 行ごとに書き出される**——`Start-Process -RedirectStandardError` で受けたファイルを実行中に読んでも取りこぼしが起きない（区間マーカー設計の成立条件）
- `seq` は単一の `AtomicU64`（`trace.rs:43`）で全行に載る**単調増加列**。`main.rs` と `commands/` が interleave しても 1 本の全順序（`src-tauri/CLAUDE.md`「モジュール構成」`trace.rs` 項）
- **H1 の擬陽性が無いこと**: `hide_egui_main`（`window_coordinator.rs:296`）は `egui_results:hide`（327 行）を出した**後**に `egui_hide:done`（339 行）を出す。ゆえに正常な teardown が H1 の区間へ食い込まない
- **H4 が真の契約違反であること**: `egui_results:show` の `rows` は `drive_results_window` の `count`（`window_coordinator.rs:568`）＝ `present_results` の `result_count`。`layout::present_results`（`layout.rs:207`）は `desired_height > 0.0` を連言に持ち、`results_window_height(0, _, _)` は 0 を返す。**`Visible` は `count > 0` を含意する**ので `rows = 0` の show は導出の破れである
- **H5 が真の不変条件であること**: `ResultsWindow::show`（`results_window.rs:91`）は `visible.swap(true)` が既に true なら `false` を返し、呼び出し側（`window_coordinator.rs:567`）は `true` のときだけ trace する。ゆえに hide を挟まない連続 show は swap 契約の破れである
- **hide 側の非対称**: `hide_egui_main` の `egui_results:hide` は**戻り値を無視して無条件に出す**（要求レベル・`window_coordinator.rs:321-323` のコメントが明記）。`drive_results_window` 側（533 行）は遷移時のみ。ゆえに hide の連続は正常であり、H5 の判定に持ち込んではならない

## 関連ファイル・モジュール・関数

| パス | 役割 | 本件での扱い |
|---|---|---|
| `scripts/manual-smoke.ps1` | カテゴリ D 目視の器（13 項目・#749） | 区間マーカーと trace 判定の並置を足す |
| `scripts/lib/SnotraSmoke.psm1` | smoke 共有配管（#843）。`Read-SnotraTraceEvents`（334 行）が `[trace] {json}` を parse 済みオブジェクト列へ | **再利用する**（parser を書き直さない） |
| `scripts/lib/SnotraSmoke.Tests.ps1` | Pester テスト | 触らない（隣に新規テストを置く） |
| `scripts/run-pester.ps1` | Pester ランナー。`$testPath = scripts/lib` を丸ごと discover し、実行前に snotra 実行ファイルの実在を要求する | 新規テストは `scripts/lib/*.Tests.ps1` に置けば自動で拾われる |
| `src-tauri/src/trace.rs` | trace 出力（`seq` / `ts_ms` / `event` / `data`） | 読むだけ・変更しない |
| `src-tauri/src/egui_shell/window_coordinator.rs` | 3 イベントの発火点 | 読むだけ・変更しない |
| `docs/build-commands.md` カテゴリ D（51-63 行） | `smoke:manual` の手順と注意 | 1 行足し + 「二重の正本」の訂正 |

## 再利用できる既存パターン

- **`Read-SnotraTraceEvents`**（`SnotraSmoke.psm1:334`）: `^\[trace\]\s+(.+)$` にマッチした行だけ `ConvertFrom-Json` し、壊れた行は黙って捨てる。書き込み途中の末尾行を握り潰す設計が既にある
- **Pester のテスト様式**（`SnotraSmoke.Tests.ps1`）: `BeforeAll` で `Import-Module -Force`、`Describe`/`It` は日本語の命題文
- **`docs/build-commands.md`「スモーク運用メモ」の作法**: 「観測できなかった」を合格と読ませない・沈黙経路を列挙して塞ぐ（#804 の `-RequireResults` 格上げが手本）

## 技術的制約

- **`run-pester.ps1` は実行ファイルの実在を要求する**（`Resolve-SnotraCargoExecutable` → `Test-Path` で throw）。純関数のテストでも同じランナーに乗るため、ローカル実行前に `cargo build -p snotra` が要る。CI（`ci.yml` rust-check・windows）はビルド済み
- **`manual-smoke.ps1` は対話入力を要求する**（`[Console]::IsInputRedirected` で先に落ちる）。ゆえに**エージェントはスクリプト全体を実行できない**——判定ロジックの検証は Pester 側で閉じる必要がある（本計画がモジュールを分ける理由）
- `scripts/*.ps1` と `scripts/lib/**` は `.claude/rules/safety-nets.md` の `paths` に含まれる＝**セーフティネットの変更**である。フォールトインジェクション（故意に壊して検知されることの実測）が要求される
- `docs/build-commands.md` の変更は `npm run governance:check` のトリガー（`AGENTS.md`「条件別チェック」）

## 明示的に射程外とするもの

- **#760（main が作業領域の下端を割ると results が丸ごとタスクバー下へ入る）**: issue コメントが明記するとおり **H1〜H5 のいずれも捕まえられない**。位置は trace に載っておらず、判定するには trace のスキーマ変更が要る。本件では触らない
- **`C:/tmp/snotra836-tools/` のカテゴリ D 治具のリポジトリ取り込み**: #836/#870 で使った打鍵注入 + 窓矩形キャプチャの 4 本。#757 の本文はこれに触れておらず、`gh search` でも相互参照が無い。**別軸の話ゆえ本件へ混ぜない**（必要なら別 issue）
- **SPEC.md**: 製品の挙動を 1 つも変えない（検証ハーネスのみ）ため同期不要

## 未解決の疑問

なし（`plan.md`「未確定」へ移した項目も含め、着手前に潰す形で記載する）
