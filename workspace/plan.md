# 計画 — issue #1430: main ウィンドウの取得を共通ヘルパーとラベル定数へ寄せる

調査: `workspace/research.md`（3b の採否を含む）。

## 目的と受け入れ条件

目的: main ウィンドウのラベル `"main"` を定数 1 か所に置き、生成側と取得側がその定数を通るようにする。取得はヘルパー 1 本に寄せる。挙動は変えない。

受け入れ条件:

1. `git grep -n 'get_window("main")' -- 'src-tauri/src/**.rs'` が 0 件（範囲を付けない形は過去の計画書 `docs/superpowers/plans/2026-07-22-su2-window-shell.md` に当たるが、歴史記録なので対象外）
2. `git grep -n '"main"' -- 'src-tauri/src/**.rs'` が定数の定義 1 行だけ
3. `hide_egui_main` の中で main を引くのは 1 回
4. `read_main_visible` / `show_egui_main` / `hide_egui_main` のシグネチャ（`&EventLoopProof` を含む）が変わらない
5. カテゴリ A と C が緑

## 変更するファイルと対象シンボル（4 ファイル）

| ファイル | 変更 |
|---|---|
| `src-tauri/src/egui_shell/mod.rs` | 定数 `pub(crate) const MAIN_WINDOW_LABEL: &str = "main";` とヘルパー `pub(crate) fn main_window(app: &tauri::AppHandle) -> Option<tauri::Window>` を新設。`create` の builder を `tauri::Window::builder(app, MAIN_WINDOW_LABEL)` に。`create` の doc の `"main"` を定数への参照（[`MAIN_WINDOW_LABEL`]）へ書き換え |
| `src-tauri/src/egui_shell/window_coordinator.rs` | 7 箇所（`read_main_visible`・`show_egui_main`・`hide_egui_main` 2 箇所・`clamp_main_into_work_area`・`check_show_bar_rect`・`position_results_below_main`）を `super::main_window(app)` へ。`hide_egui_main` は 1 回の引きに寄せる |
| `src-tauri/src/egui_shell/view.rs` | `SearchWindowView::update` 内の `set_size` 前の 1 箇所を `crate::egui_shell::main_window(&app)` へ |
| `src-tauri/src/commands/window.rs` | `launch_settings_process` の 2 箇所（`app` と `handle_for_monitor`）を `crate::egui_shell::main_window(&…)` へ。97 行付近の「get_window で取る」コメントのうち、取得手段の説明はヘルパーの doc へ移し、ここは理由（results と対称に解除する）だけを残す |

ヘルパーの doc に書くこと: 「egui の main は webview を持たないので `get_webview_window` では取れず `get_window` で取る（SPEC §8.5）」「ラベルの正は SPEC.md §8『ウィンドウの名称と役割』」「証人（`EventLoopProof`）は要求しない——可視性を読む・変える関数の側が要求する」。

`"results"` には定数を置かない（直書きは builder の 1 か所で、照合相手が無い。research.md「未解決の疑問」）。直書きを禁じる検査も置かない（issue 判断）。

## 実装の順序

1. 変更前の基準を取る: `cargo build -p snotra` の後に `npm run smoke:egui` を走らせ、結果（合否と主要 trace の件数）を記録する
2. 定数・ヘルパーの新設と 10 箇所の移行を 1 タスクで行う（新 API の導入と呼び出し点の移行は束ねる・AGENTS.md）
3. `hide_egui_main` の 2→1
4. 検証

## 不変条件と異常系

- 定数値は `"main"` のまま（SPEC §8「ウィンドウの名称と役割」）
- ヘルパーは `Manager::get_window` の結果をそのまま返す。None の扱い（early return・`unwrap_or(false)` 等）は各呼び出し元のまま変えない
- `hide_egui_main` の順序は「placement の読み → `.hide()` → results の hide → 書き込み」のまま。1 回化の書き換え形:
  ```rust
  let main = super::main_window(app);
  let placement = main.as_ref().and_then(|w| read_placement_relative(w, bar_height));
  if let Some(window) = main {
      let _ = window.hide();
  }
  ```
  2 回目の引きに依存していた行の書き出し: 2 回目の引きが返すものは 1 回目と同じ窓（同一スレッド・間に破棄経路なし・3b P3）。2 回目の結果に依存していた行は `.hide()` だけで、1 回目の結果に依存していたのは `placement` だけ。1 回目が None なら 2 回目も None だった（窓が途中で生えない）ので、None 時の挙動（読みも hide もしない）も同じ
- 「main に対する `.hide()` は `hide_egui_main` の中の 1 か所だけ」（`hide_egui_main` doc）は変更後も真

## テスト方針と検証コマンド

- Red: 受け入れ条件 1 の grep が変更前に 10 件であること（基準として記録）。窓の取得経路にユニットテストは無く、tauri の mock を新たに入れるほどの価値は無い
- 挙動不変の根拠は、同じ型（`Option<tauri::Window>`）を返す関数への機械的置換と、コンパイラ・clippy
- カテゴリ A: `cargo fmt --all -- --check` / `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`
- カテゴリ C: `cargo build -p snotra` の後に `npm test` / `npm run test:powershell` / `npm run smoke:startup` / `npm run smoke:egui`
- smoke の射程（3b で確定）: 守られるのは `view.rs` の `set_size`・`show_egui_main`・`read_main_visible`・`hide_egui_main` の `.hide()`。射程外は `launch_settings_process` の 2 箇所・`clamp_main_into_work_area`・`check_show_bar_rect`・`hide_egui_main` の placement 読み・`position_results_below_main`。射程外は置換の機械性で担保し、完了報告にそう書く
- 変異注入（`/implement` の委譲先が行う）: ヘルパーが常に None を返すよう変えて `smoke:egui` が赤になることを確かめ、戻す（全 10 箇所が同じヘルパーを通るので、ヘルパー自体の退行は smoke の射程内の経路で捕まることの確認）。生成側だけを定数ではなくリテラル `"mainx"` に変えて `smoke:egui` が赤になることも確かめ、戻す（生成と取得のラベルがずれたら smoke が検出することの確認。定数の値そのものを変える変異は両側が追随して緑になるので、検出力の証拠にならない）

## SPEC・関連文書の更新

- `SPEC.md`: 不要（挙動もラベルも変えない）
- `src-tauri/CLAUDE.md`: 不要（ファイルを足さない）
- `docs/architecture.md`: 不要（`get_window` の記述なし）

## 作業項目

### Phase 0 — 基準

- [x] `cargo build -p snotra` → `npm run smoke:egui` の結果を記録する — 472e2a22 で exit 0（show/hide + results show/hide、toast ありの show 2 回とも DWM 752x110・payload height 86、`window.bin` の書き込みあり）
- [x] 受け入れ条件 1・2 の grep を変更前に実行して件数を記録する — 472e2a22 で 10 件 / 12 行（12 = 取得 10 + builder 1 + `create` の doc 1。計画時の「2 行」は取得分を数え落としていた）

### Phase 1 — 定数・ヘルパーと移行

- [x] `mod.rs` に `MAIN_WINDOW_LABEL` と `main_window` を新設し、`create` の builder と doc を定数へ寄せる
- [x] `window_coordinator.rs` の 7 箇所を移行し、`hide_egui_main` を 1 回の引きにする
- [x] `view.rs` の 1 箇所を移行する
- [x] `commands/window.rs` の 2 箇所を移行し、取得手段のコメントをヘルパーの doc へ移す
- [x] `main_window` の呼び出し元が 9 箇所であることを確かめ、`/dry-check` を実行する — LSP findReferences は 7 件しか返さなかった（`commands/window.rs:157` のスレッド内クロージャと `view.rs:1241` を落とした）。ヘルパーを一時改名した `cargo check -p snotra --all-targets` の E0425 で 9 件を列挙し、内容ハッシュで復元を確認。`/dry-check`: `get_window\(|get_webview_window\(|"main"` の全 `.rs` grep は定義と本体の 2 行だけ＝手書き重複なし

### Phase 2 — 検証

- [x] 受け入れ条件 1〜4 を確かめる — 1: 0 件／2: `mod.rs:327` の定義 1 行／3: `hide_egui_main` の引きは 1 回／4: `git diff` で既存関数のシグネチャ行の変化なし（`fn` を含む差分は新設の `main_window` だけ）。主ツリーで clippy（snotra・all-targets）exit 0・`cargo test -p snotra` 318 passed・`cargo doc -p snotra` exit 0
- [x] カテゴリ A を実行する
- [x] カテゴリ C を実行し、Phase 0 の基準と比べる — 52545771 で smoke:egui exit 0（基準と同じく合格）
- [x] 変異注入（委譲先）の結果を受け取る
- ラウンド 1: 52545771 / verify:tree green（A・C・E・F。警告: node_modules が lock と食い違う＝vitest 5.0.5 vs 5.1.0 等。Rust の変更とは無関係）/ 指摘 Low 4 件（Critical・High 0）/ 生き残った変異 0（M1 None・M2 生成側だけ "mainx"・M3 `.hide()` 削除の 3 件とも smoke:egui 赤）
- ラウンド 1 の対処: ヘルパー doc の「SPEC §8.5」（§8.5「ウィンドウ生成とプロセス管理」が持つのは設定起動中の最前面解除の項で、get_window の根拠ではない）を外し、always_on_top の解除側コメントへ戻した／「値の正」→「値の正本」／証人を要求する関数の列挙をやめた／`commands/window.rs` 復元側の「egui は get_window」を削除。「証人」の用語集追加は当初見送り→ラウンド 2 で委譲先の反論（code-reviewer.md は差分が新しく書いた語を対象にする）を採り、追加
- ラウンド 2: 387fc201 / verify:tree green（A・C・E・F、14 コマンドすべて exit 0）/ 指摘 Low 2 件（Critical・High 0）+ 用語集見送りへの反論 / 変異注入なし（コード変更なし）
- ラウンド 2 の対処: `commands/window.rs` 解除側コメントの英文末尾へ日本語の括弧を継いだ混在を、日本語の 1 文に分けた／本行の §8.5 の言い方を節名つきへ直した／`docs/glossary.md` に「証人・証人型」を追加
- ラウンド 3: 3e98e88b / verify:tree green（A・C・E・F、14 コマンドすべて exit 0）/ 指摘 Low 1 件（Critical・High 0）/ 変異注入なし（コード変更なし）
- ラウンド 3 の対処: 用語集の定義が `ADR-startup-terminal-single-derivation` の「網羅 `match` の証人」に収まらなかったので、定義を「条件が成り立つことをコンパイラが確かめる形で示す値・型・構文」へ広げ、両方の例を添えた（ADR は凍結なので語の側は直さない）
- ラウンド 4: fbec83b9 / verify:tree green（E・F。差分は用語集と plan.md のみ。コードは 3e98e88b の A・C green から不変）/ 指摘 0 件 / 変異注入なし
- ADR: 起こさない（却下した案は「results の定数」「直書き禁止の機構」「定数だけ置く案」で、いずれも issue 本文か本計画に理由があり、PR 本文へも書く）

## 未確定（実装前に潰す）

- [x] ヘルパーの置き場と名前 — 生成と同じ `egui_shell/mod.rs`、`main_window` / `MAIN_WINDOW_LABEL`。生成と取得が 1 ファイル内に閉じる。名前の衝突なし（3b P4）
- [x] `hide_egui_main` の 2→1 が可能か — 可能。`read_placement_relative` は `&tauri::Window` を借りるだけ（シグネチャ確認・3b P3）
- [x] `use tauri::Manager` が未使用にならないか — 4 ファイルとも `try_state` で使い続ける（3b P2）。最終は clippy で確定

## 人間レビュー

- [x] 承認済み — 2026-11-13 / 問い: "`workspace/plan.md` をこのまま承認するか、`workspace/plan.md` へ注釈を追加するか" / 回答: "承認します"

## セルフレビュー

- リスク: 通常（永続形式・並行性・状態遷移・ガバナンス文書・網羅性要件のいずれも変えない。可視性を変える関数の証人は維持）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査のみ）
- 要対処: 4 件を反映——受け入れ grep の範囲を `src-tauri/src/**.rs` に限定／`create` の doc の `"main"` を定数参照へ／smoke の射程外 6 箇所を明記／変異注入を生成側のみのリテラル変異に修正
- 未検証: smoke の射程外 6 箇所の実挙動（置換の機械性とコンパイラで担保）
- 散文の全件列挙: 未実施（通常リスク）
- 照合: issue の全要件（定数・ヘルパー・生成側・移行・2→1・証人維持・検査を置かない）に作業項目あり／境界は None 時の挙動で検証あり／新しい状態・リソースなし／より単純な案なし（ヘルパー無しで定数だけ置く案は、`get_window(MAIN_WINDOW_LABEL)` が 9 箇所に残り「webview 無しゆえ get_window」の理由が散る）／不変条件（シグネチャ・順序）は受け入れ条件 3・4 とコンパイラで検知
- 条件別チェック: 「関数を新規定義」→ LSP findReferences と `/dry-check`（Phase 1）、「重複した読みを束ねる」→ `hide_egui_main` の行ごとの依存を「不変条件と異常系」に書き出し済み
