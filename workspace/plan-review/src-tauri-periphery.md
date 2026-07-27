# B層レビュー: src-tauri 外周と検証カテゴリ（#666 段3 plan.md）

## 1. 問題なし

- `docs/architecture.md` の `view.rs` 言及 4 箇所は plan.md L23 が挙げる L80・L125・L147・L156 と grep 実測で完全一致（他に L160 は `results_view.rs` 言及で対象外・正しく除外されている）。
- `src-tauri/src/` 配下（`egui_shell/` 除く）で `view.rs` の項目を参照するのは `commands/launch.rs:50` と `commands/instant.rs:19` の2箇所のみ（grep 実測）。plan の変更ファイル一覧（L20-21）と過不足なく一致する。
- `view.rs` が emit する `crate::events::EGUI_HIDE_REQUESTED`（L331）・`EXIT_REQUESTED`（L555）は `events.rs` の定数で、`emit_hide`/`execute_slash(Quit)` ごと `launcher_controller.rs` へ移っても定数パス自体は不変。listen 側は `mod.rs` にあり本移設で経路は変わらない。
- `.github/workflows/e2e.yml` L14 の paths に `src-tauri/**` が含まれるため、本変更（`.rs` 追加2件を含む）で `Smoke` workflow（smoke-egui job = カテゴリC）は PR で自動発火する。plan Phase5 のカテゴリC実行判断は正しい。
- カテゴリA・Cのコマンド文字列は `docs/build-commands.md` と完全一致（`cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`、`npm test` / `npm run smoke:startup` / `npm run smoke:egui`。build-commands.md L14-21・L38-40 と plan.md L94-95 を突合）。B・E不要の判断も正しい（`.ts`/`.githooks` を触らない）。
- `notify.rs` の `NoticeSlot::remaining`/`poll` は純粋関数で、実際に repaint を予約するのは `view.rs:1179-1181`（`ctx.request_repaint_after(remaining)`）のみ。これが無いと `poll`（実際にメッセージを消す side effect、L1176）を呼ぶフレームが二度と来ない。破壊不変条件#5の実機目視（放置して数秒で自然に消えること）はこの機構を正しく検知できる。
- `scripts/smoke-egui.ps1` は `Wait-TraceEvent`（presence待ち）と末尾のorphan検出（L423-444、hide後の余分な`egui_results:show`不在確認）のみを見ており、`update()` 内部の文の実行順序は一切検査していない。plan の「フレーム順序の不変条件に自動検出器は無い」（plan.md L120）は正しい。

## 2. 軽微な懸念

- `view.rs` 自身が発する trace イベント名（grep 実測: `egui_launch` L358-360・`egui_instant` L403-405・`egui_launch_done` L433-435・`egui_slash`/`egui_slash_error` L529,541,549・`egui_instant_error` L584-587・`egui_tool_enter` L641-644・`egui_tool_launch` L666・`egui_search:dispatch` L856-858・`egui_update_install_*` L934,946,951,957・`egui_results:click_stale` L1715-1718）は、`smoke-egui.ps1` が実際に観測する名前（`hotkey:registered`・`egui_show:done`・`egui_hide:done`・`egui_results:show`・`egui_results:hide` — 全て `window_coordinator.rs` / `platform/hotkey.rs` にあり `view.rs` 側には無い）と完全に不連続。plan.md L95 の「スラッシュコマンド経路 execute_slash を移設したため発火」は `.claude/rules/src-tauri.md`「トリガー→検査」の引用としては正しいが、`smoke:egui` が実際に slash command 経路を検証すると誤読されうる（実際は 1 文字クエリ + hotkey + Escape のみで、slash/tool-launch/toast は素通り）。
- plan.md L97 の `SNOTRA_EGUI_FAKE_UPDATE_FAILED=1 cargo run -p snotra` は bash 形式の inline env var 代入で、Windows PowerShell では動作しない（`$env:VAR = "1"; cmd` が正しい形。`docs/build-commands.md` L72-73 は正しく PowerShell 形式で書いている）。Phase5 実行者（人間）がこのまま実行すると失敗する。
- 指定された対象ファイル `scripts/smoke-manual.ps1` は存在しない。実ファイルは `scripts/manual-smoke.ps1`（`package.json:13` の `smoke:manual` script）。内容は確認済みで計画上の問題は見当たらないが、命名の食い違いがある。
- `LauncherController` へ移る23メソッドのうち `execute_slash`（`/o` `/s` `/q` の dispatch）は、`smoke-egui.ps1` にも `manual-smoke.ps1` の既定10項目（L48-105、いずれも window/results/hide/font_size/click 系）にも検査対象が無く、plan の「破壊不変条件と検知手段」5件（plan.md L122-128）にも含まれない。移設は本文不変のため clippy/test で構造的破損は検知できるが、slash 経路固有の挙動退行を捉える手段が計画に明示されていない。

## 3. 要対処

- Phase4 の検証「`cargo doc --workspace --no-deps --document-private-items`（doc コメント内の intra-doc link 切れ）」（plan.md L89）は、`commands/launch.rs:50` と `commands/instant.rs:19` の対象参照を検知できない。両箇所は `` `egui_shell::view::SearchWindowView::activate` `` のような**素の backtick コードスパン**であり、rustdoc の intra-doc link 解決は `[...]` 記法のみを対象にする——bracket の無い code span は単なる等幅テキストとして扱われ、リンク切れ検査の対象にならない（rustdoc 標準仕様）。Phase4 でこの2箇所の文言更新を忘れる・誤った新パスへ書き換えても `cargo doc` は緑のまま通る。人間のコードレビュー（差分の目視）が唯一の検知手段になる旨を計画に明記すべき。

## 4. 未検証（理由）

- Phase1〜3 で新設される `font.rs` / `launcher_controller.rs` の実コードはまだ存在しない（plan 段階のレビューのため、実装後の diff に対する 34段照合表の目視結果は検証対象外）。
- `.claude/skills/state-check/SKILL.md` の変更内容・`/norm-review` の実施結果は担当外レイヤー（他エージェントの割当）のため深掘りしていない。
- `create-release.yml` / `release.yml` / `label-sync.yml` の paths は、本変更に無関係と判断し中身を読んでいない（`e2e.yml` のみ確認）。
- `cargo doc` / `cargo clippy` / `cargo test -p snotra` を実際に実行して現状のベースラインが緑であることは確認していない（計画レビューのみのため、コード変更・コマンド実行は行わなかった）。
