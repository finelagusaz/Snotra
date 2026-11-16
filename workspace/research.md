# 調査 — issue #1443

## 要約

打鍵ごとに、同じクエリで worker へ検索が 2 回送られる。1 回目は `Debouncer::on_input` が返す leading、2 回目は `Debouncer::poll` が返す trailing である。trailing は leading の後に入力が無くても発火する。人の打鍵間隔はたいてい 50 ms（debounce の interval）を超えるので、ほぼ毎打鍵が「leading + 入力の無い trailing」になる。issue の実測では、2 回目もほぼ全額かかる（Fuzzy `c:\` で 12,455 → 12,608 µs）。

直し方（issue の案）: `Debouncer` で「leading の後に `on_input` があったときだけ trailing を発火する」（lodash の `debounce({leading: true, trailing: true})` と同じ契約）。`armed` の意味（`on_input` で立ち、`poll` / `cancel` で落ちる）は変えない。`run_search_with` に「同じクエリなら飛ばす」を入れてはならない（索引の再構築後・起動失敗・フォルダの drain が、同じクエリで検索し直す経路でもある）。

分類: 挙動を変えるが、`SPEC.md` に debounce の発火回数の記述は無い（`SPEC.md` の debounce 言及は §15.3 のコマンド即時実行の 1 か所だけ）。仕様変更ではなく性能の改善（refactor/perf）である。

## 関連するファイル・シンボル

- `src-tauri/src/egui_shell/layout.rs`
  - `Debouncer`（フィールド `interval` / `leading` / `armed`）、`Debouncer::new` / `interval` / `is_armed` / `on_input` / `poll` / `cancel`
  - テスト `leading_fires_on_burst_start_then_trailing` / `no_leading_mode_only_trailing` / `cancel_disarms_pending_trailing`（同ファイルの `mod tests`）
- `src-tauri/src/egui_shell/launcher_controller/search_flow.rs`
  - `on_input_changed` の Plain 分岐: `last_input_at` を更新 → `search_debounce.on_input()` が真なら `run_search_with(&prefix)`（leading）→ `request_repaint_after(interval)`
  - `poll_search_debounce`: `poll(last_input_at.elapsed())` が真なら `run_search()`（trailing）。armed の間は残り時間で `request_repaint_after` を再要求
  - `run_search_with` の Results/Plain 分岐: 空クエリ・`indexing` 中は同期でクリア、それ以外は `issue_search` → `search_tx.send`
  - `clear_search`（`cancel` を呼ぶ）
- `src-tauri/src/egui_shell/launcher_controller.rs` — `new_search_debounce`（`Debouncer::new(50 ms, true)`）・`search_debounce` フィールド・`is_armed` の読み（168 行付近）
- `src-tauri/src/egui_shell/launcher_controller/frame_stages.rs`
  - `consume_reset_pending`: show のたびに `Debouncer` を作り直す
  - `consume_external_pending`: 索引の構築完了（`index_generation` の変化）で `run_search()` を撃つ——trailing に頼らない
- `src-tauri/src/egui_shell/launcher_controller/activation.rs` — `on_enter` の flush（`should_flush_on_enter` ∘ `SearchState::is_unsettled(armed)`）と `cancel`、ツール突入の `cancel`
- `src-tauri/src/egui_shell/launcher_controller/folder_nav.rs` — `enter_folder(…, is_armed())`（`unsettled_at_entry` へ控える）
- `src-tauri/src/egui_shell/search_state.rs` — `is_unsettled(armed)` = `armed || pending_seq != 0 || restored_rows_stale`
- `src-tauri/src/egui_shell/results_view.rs` — `input_idle`（`!is_armed()`・アイコン要求のゲート）
- `src-tauri/src/egui_shell/view.rs` — 段 27 で `poll_search_debounce` を呼ぶ
- trace: `egui_search:settled` / `egui_search:dropped`（`search_flow.rs` の `drain_search`）。`SNOTRA_TRACE` で有効

## 変更で偽になる散文（現在形の主張）

- `src-tauri/src/egui_shell/layout.rs` の `Debouncer` の doc（「poll: armed かつ elapsed>=interval で trailing 発火し disarm」）と `poll` の doc / `#[must_use]` 文言
- `src-tauri/src/egui_shell/search_dispatch.rs` の `Settled` の doc「打鍵起点には 50 ms の trailing debounce 待ちが必ず入る」——孤立した打鍵は leading の結果で settled になり、50 ms 待ちは入らない
- `PERFORMANCE.md`（`put_rows` の節）「単発の打鍵では leading と trailing の結果着で 2 回走る」
- `snotra-core/tests/search_frame_cost.rs` の `//!`「egui 経路は trailing debounce 1 回ごとに `engine.search` を呼ぶ」——leading も呼ぶ（変更前から不正確）
- 真のまま残るもの: `docs/architecture.md` の sequence 図（「leading 発火 + trailing 50ms を arm」「poll_search_debounce（trailing）」は arm と poll の記述で、発火の有無を言っていない）、`search_state.rs` の `is_unsettled` の doc（「trailing は poll 経由で別に出す」「解けるのは trailing が発火するか、cancel か…」——poll が disarm する点は変わらない）、`PERFORMANCE.md` の冒頭（「leading edge + trailing 50ms」）と 513 行（過去の計測の表）
- `docs/superpowers/specs/2026-08-10-search-worker-design.md` は書いた後は更新しない設計メモなので直さない

## 再利用できる既存パターン

- `Debouncer` は純粋核（時刻を注入）。テストは同ファイルの `mod tests` にあり、`poll(Duration)` で経過を与える
- `armed` の意味を変えないので、`is_unsettled` / `input_idle` / `unsettled_at_entry` / repaint の再要求の式はそのまま使える

## 技術的な制約

- `armed` は「leading の後 interval のあいだ」も立ち続ける。これは Enter の flush（#1038）とアイコンのゲートの材料なので変えない。変えるのは `poll` の返り値だけ
- leading=false のモード（製品では使っていない・テストだけ）では、`on_input` はいつも「入力があった」になり、従来どおり trailing だけが発火する
- `cancel` と `new` は「入力があった」の印も落とす必要がある（落とさないと、cancel 後の次バーストで leading だけのときに trailing が出る——現状の 2 回より悪くはないが、契約が崩れる）
- trailing が兼ねていた役目（issue の注意）: 50 ms 後に `instant_command_prefix` を読み直す（`run_search` が `instant_prefix()` を読む）。leading と trailing の間に prefix が変わると、従来は trailing が新しい prefix で再解釈していた。prefix を変えるのは設定ウィンドウなので、打鍵から 50 ms 以内に変わることは手操作では起きない
- 索引の構築完了は `consume_external_pending` が別に再検索するので、trailing に頼っていない
- smoke（`scripts/smoke-egui.ps1`）は Snotra が既に起動していると throw する。開発機では常駐の Snotra が動いている（2026-11-16 時点で `snotra.exe` が稼働中）

## 敵対的調査（3b）の所見と採否

出力: `workspace/adversarial-1443.txt`。

- 壊せなかった: 争点 1（trailing は leading と同じ要求の再送。leading と同じフレーム・同じ prefix で interp し、`set_query` の呼び手は `on_input_changed` と `clear_search` だけ）、2（`view.rs` の順序は drain → poll → publish。disarm したフレームの publish が `input_idle = true` を運ぶので、trailing の結果着の wake に頼っていない。flush-on-Enter は `pending_seq` の項が leading の in-flight を覆う）、3（cancel 後の最初の Plain 入力は必ず leading。indexing 中の leading は世代の変化で再検索される）、4
- 採用 F1: `settled + dropped` は dispatch 数の下限（worker の `coalesce` が間引いた seq はどちらにも出ない）。→ 数え方を **seq の差**に替える。`SearchDispatch::issue` は 1 発行ごとに `next_seq` を 1 進めるので、窓の前後の `dispatch_seq` の差が発行数になる（自分で `search_dispatch.rs` の `issue` を読んで確認）
- 採用 F2: smoke のパスクエリ打鍵は 120 ms 間隔で、「leading のみ」の分岐しか通らない。→ 受け入れ 2（連打の最後は trailing で検索）は `Debouncer` の純粋核テストで縛る
- 採用 F3: main 側の基準線は Backspace バーストとの競合で 5〜6 に揺れる。→ 比較の主張は「本ブランチで 3 キーが 3 発行」に置き、main 側は参考値（揺れを併記）にする
- 採用 F4（⚠️）: smoke に常駐検出のガードは無い。single-instance で smoke 側が即終了する推論。→ smoke の前に常駐を終了してもらう（ユーザーの操作）
- 採用 F5: 実機は `normal_mode = substring`・migemo 有効。Substring の選択的なクエリは 2 回目が増分キャッシュで安いので、実機での効果は issue の Fuzzy の表より小さいクエリがある。smoke の索引は 1 件で、数だけが測れる
- 採用 F6: 偽になる散文の漏れ。`layout.rs` の `on_input` / `is_armed` / `cancel` の doc、`search_flow.rs` の `poll_search_debounce` の doc とコメント（L296・L300）、⚠️ `search_state.rs` の L92 と L609-611（「trailing 発火で armed が落ちる」——発火しない poll でも落ちる）と L1191 付近のテスト doc。→ 計画の変更ファイルへ足す。`smoke-egui.ps1` の「debounce(50ms) の trailing を跨がせる」は「trailing の時刻を跨がせる」の意味で真のまま（`scripts/` はセーフティネットなので触らない）。`PERFORMANCE.md` の該当行は採否ログの過去の記録なので直さない（`PERFORMANCE.md`「この文書へ記録するときの規約」）。`since_key_us` が孤立打鍵で約 50 ms 短くなる点は、新しい記録として残す
- F7（⚠️）: trailing は config（`normal_mode` 等）と history も読み直していた。ただし config 変更時に再検索する機構は元から無く、打鍵から 50 ms 以内の偶然でしかない。→ 実害の根拠にならないので本件では扱わない
- F8 の副作用（採用・記録だけ）: 検索が 50 ms を超えるとき、従来は trailing が leading の結果を捨てていた。変更後は leading の結果がそのまま採り込まれる
- F9（⚠️）: Escape が最後の打鍵から 50 ms 以内の極小の窓だけ trailing が救う余地がある。従来も folder 中の poll でほとんど落ちていた。→ 受容する
- F10（⚠️）: show の全経路が `reset_pending` を立てるかは網羅していない。本件は `armed` の意味を変えないので影響しない
- F11: 新契約の Red テスト（leading のみのバーストで trailing が出ない）と、leading=false モードの最初の `on_input` を「入力あり」とする点 → 計画へ

## 未解決の疑問（計画で決着）

- 受け入れ「孤立した 1 打鍵で、worker へ送る検索が 1 回になる」を何で測るか。`egui_search:settled` は採り込みの数で、dispatch の数ではない（追い越された要求は `dropped`）。送った数は `settled + dropped` で数えるのが正しいか
- 計測の場をどうするか: `scripts/smoke-egui.ps1` のパスクエリ打鍵（`c`・`:`・`\` を 120 ms 間隔）を、main と本ブランチで走らせて数を比べられるか（smoke の改変は不要か）
- leading の要求が送られなかった（空クエリ・`indexing` 中・送信失敗）バーストで、trailing を省いてよいか——どれも trailing が走っても同じ同期クリアになるので省いてよいはず、を確かめる
