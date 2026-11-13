# 調査 — issue #1430

## issue の要約

`app.get_window("main")` が `src-tauri/src` の 10 行に、ラベルの文字列リテラルつきで直書きされている。生成側（`egui_shell/mod.rs` の `tauri::Window::builder(app, "main")`）との一致を担保しているのは文字列の一致だけである。ラベルの定数と、main ウィンドウを引く共通ヘルパーを置き、直書きをヘルパーへ移す。`hide_egui_main` の 2 回引きも 1 回へ寄せられるかを見る。挙動は変えない。

受け入れ条件: `git grep 'get_window("main")'` が 0 件。ラベルのリテラルは定数の定義だけに残る。カテゴリ A と C が緑。

## 関連するファイル・シンボル（2026-11-13・ブランチ `chore/main-window-helper` で `git grep -n 'get_window("main")' -- 'src-tauri/src/**.rs'` = 10 行）

| ファイル:行 | 関数 | 受け手の型 |
|---|---|---|
| `src-tauri/src/commands/window.rs:100` | `launch_settings_process`（設定起動で always_on_top を外す） | `AppHandle` |
| `src-tauri/src/commands/window.rs:158` | 同関数内の設定プロセス終了の監視スレッド（`handle_for_monitor`） | `AppHandle`（clone） |
| `src-tauri/src/egui_shell/view.rs:1241` | `SearchWindowView::update` 内の `set_size` | `AppHandle`（`self.controller.app().clone()`） |
| `src-tauri/src/egui_shell/window_coordinator.rs:73` | `read_main_visible` | `&tauri::AppHandle` |
| `window_coordinator.rs:312` | `show_egui_main` | `&tauri::AppHandle` |
| `window_coordinator.rs:526` / `:528` | `hide_egui_main`（placement の読みと `.hide()` で 2 回引く） | `&tauri::AppHandle` |
| `window_coordinator.rs:1043` | `clamp_main_into_work_area`（`#[cfg(windows)]`） | `&tauri::AppHandle` |
| `window_coordinator.rs:1186` | `check_show_bar_rect`（show 後の矩形検出） | `&tauri::AppHandle` |
| `window_coordinator.rs:1271` | `position_results_below_main` | `&tauri::AppHandle` |

生成側: `src-tauri/src/egui_shell/mod.rs:350` `tauri::Window::builder(app, "main")`（`create`、受け手は `&mut tauri::App`）。同 `:325` の doc にも `"main"` が散文で出る（リテラルではなく説明）。

`"results"` は `mod.rs:368` の builder 1 か所だけで、`get_window("results")` は無い（results は `ResultsWindow` を state として引く）。

## 再利用できる既存パターン

- `egui_shell/mod.rs` は生成と登録の置き場（`//!`）。ヘルパーと定数を生成と同じファイルへ置けば、一致の担保が 1 ファイル内に閉じる。
- `mod.rs` には `read_config` / `read_visual` のような `&tauri::AppHandle` を取る `pub(crate) fn` が既にある。同じ形で `pub(crate) fn main_window(app: &tauri::AppHandle) -> Option<tauri::Window>` を置ける。
- `commands/window.rs` は既に `crate::egui_shell::ResultsWindow` を参照しており、`crate::egui_shell::main_window` を呼ぶ形と揃う。

## 技術的な制約

- `read_main_visible` / `show_egui_main` / `hide_egui_main` の `&EventLoopProof` 引数は意図（`docs/adr/ADR-main-visibility-live-read.md`・各 doc）。ヘルパーは窓を引くだけで、証人を要求しない・外さない。呼び出し元のシグネチャは変えない。
- `get_window` は `tauri::Manager` トレイトのメソッド。ヘルパー化後も 4 ファイルとも `try_state` 等で `Manager` を使い続ける（各ファイルで `.try_state`/`.state` を確認）ので、`use tauri::Manager` が未使用になって clippy `-D warnings` で落ちることは無い見込み。最終判定は clippy で取る。
- `SPEC.md` §8「ウィンドウの名称と役割」が「コード上のウィンドウラベルは英字 `main` / `results` を正とする（Win32・tao・トレースが参照するため）」と定める。定数の値は `"main"` のまま変えない。
- `hide_egui_main` の 2 回引きを 1 回へ: 同じイベントループスレッド上で連続して引いており、間にウィンドウが破棄される経路は無い。`let main = main_window(app);` → `main.as_ref().and_then(|w| read_placement_relative(w, bar_height))` → `if let Some(w) = main { w.hide() }` で順序（読み → hide）を保てる。`read_placement_relative` の引数型は要確認（`&tauri::Window` のはず）。
- `tauri::Window` は `Clone` で、`get_window` は毎回 `Option<Window>` を返す（所有）。ヘルパーの戻り値も同じ型にすれば、呼び出し側の形は変わらない。
- テスト: tauri の窓を引く経路にユニットテストは無い（`tauri::test` / `mock_app` の使用は 0 件）。挙動不変の確認はカテゴリ A（コンパイル・clippy・`cargo test -p snotra`）と C（`smoke:egui` 等）。Red は受け入れ条件の grep（10 件 → 0 件）。

## 文書の写し

- `src-tauri/CLAUDE.md`・`docs/architecture.md`・`SPEC.md` に `get_window("main")` の記述は無い（grep 0 件）。`docs/superpowers/plans/2026-07-22-su2-window-shell.md` に出るが、過去の計画書（歴史記録）なので直さない。
- `commands/window.rs:97` のコメント「egui ウィンドウは webview 無しゆえ get_webview_window では取れない。get_window で取る」は、ヘルパーを経由しても真（ヘルパーの中身が `get_window`）。ヘルパーの doc へ移すのが自然。
- 新しい `.rs` は足さないので、`src-tauri/CLAUDE.md` のモジュール構成は変わらない。

## 未解決の疑問

- 定数名と置き場: `egui_shell::MAIN_WINDOW_LABEL`（`mod.rs`）を第一案とする。
- `"results"` にも定数を置くか: issue のスコープ外。直書きは builder の 1 か所だけで、一致を担保すべき相手が無い。置かない（対称性のためだけに足すのはやりすぎ）。
- 直書きを機構で禁じる検査: issue が「置かない」と判断済み。

## 追加の確認（主エージェント）

- 受け入れ条件の grep は、範囲を付けないと `docs/superpowers/plans/2026-07-22-su2-window-shell.md`（過去の計画書）に当たる。範囲は issue の背景にある `-- 'src-tauri/src/**.rs'` と読む。
- `mod.rs:325` の doc に `"main"` が散文で出る。「リテラルは定数の定義だけに残る」を満たすには、この doc を定数への参照へ書き換える。
- 非 Rust の消費者: `src-tauri/tauri.conf.json` の `app.windows` は `[]`、`src-tauri/capabilities/` は README のみ。`src-tauri/**`・`scripts/**` の非 `.rs` に `"main"` は 0 件。
- `window_coordinator.rs` の `hide_egui_main` doc と `drive_results_window` 内コメントの「`Manager` から引いた main の `.hide()`」は、ヘルパーを置いても真（ヘルパーは `Manager::get_window` の薄い包み）。「main に対する `.hide()` はこの関数の中の 1 か所だけ」も 2→1 化の後で真のまま。`ADR-main-visibility-live-read`・`ADR-window-coordinator-split-rule` に、窓の引き方や置き場を縛る記述は無い。

## 敵対的調査（3b）の結果と採否

出力: `workspace/adversarial-1430.txt`（sonnet 1 体）。

壊せなかった（採用・主張を維持）:
- P1: 10 行、他の crate・テストに label で main を引く箇所は無い。`label()` の消費者は `snotra-egui-runtime` の attach の pending map と診断出力で、`"main"` のリテラルを持たない。
- P2: 4 ファイルとも cfg に依らないコードで `try_state` を使うので、`use tauri::Manager` は残る。
- P3: `read_placement_relative` は `&tauri::Window` を借りるだけ。2 回の引きの間に破棄・再生成の経路は無い。
- P4: 全 10 箇所の受け手は `AppHandle`。`main_window` という識別子は未使用（`layout::main_window_height` は別物）。
- P5: smoke は DWM ハンドルで窓を特定し、label に依存しない。

壊せた（採用）: 「smoke:egui の緑が挙動不変を示す」は、10 箇所のうち一部にしか当たらない。`scripts/smoke-egui.ps1` で自分でも確かめた（`launch_settings_process` は 429 行で「出たら不合格」の側、`bar_rect_mismatch` は 957 行で不在だけを見る）。
- 守られる: `view.rs` の `set_size`（DWM 矩形の実測）、`show_egui_main`（`egui_show:done`）、`read_main_visible`（results の show ゲート）、`hide_egui_main` の `.hide()`（`egui_results:hide`）。
- 守られない: `launch_settings_process` の 2 箇所（smoke は通らないことを assert）、`clamp_main_into_work_area`・`check_show_bar_rect`（None なら early return で緑のまま）、`hide_egui_main` の placement 読み（`window.bin` を見ない）、`position_results_below_main`（位置を見ない）。
- 帰結: 挙動不変の根拠は「型が同じ `Option<tauri::Window>` を返す関数への機械的な置換」＋コンパイラ・clippy に置き、smoke は守られる経路の回帰検出として使う。守られない経路は完了報告に「smoke の射程外」と書く。ヘルパー本体が誤って None を返す退行は、守られる経路（show/hide）で smoke が落ちるので捕まる（全箇所が同じヘルパーを通るため）。

⚠️ の扱い:
- view.rs の借用: clippy/check で確定させる（計画の検証に含まれる）。
- 1 回化で `Window` の clone が `hide()` まで生きる: 現状も 1 回目の `Window` は `and_then` の中で落ち、2 回目は `if let` の間生きる。`Window` はハンドル（`Arc` 相当）で、生存期間の延長は数行分。採否: 影響なしと判断（窓の寿命を握るのは Manager 側で、ハンドルの clone ではない）。
- SPEC §8 の「Win32・tao・トレースが参照する」が実態より広い可能性: 本 issue の範囲外（定数値を変えないので主張の真偽は変わらない）。記録だけ残す。
- cfg(not(windows)) 版は `get_window` を呼ばないので、書き換え対象外。
