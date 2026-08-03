# plan: issue #884 実装順序 5 — SPEC 語彙降格（層 4 シンボル名の系統的降格）

## 目的と受け入れ条件

SPEC.md から「仕様の理解に寄与せず、コード改名で黙って腐る」層 4 シンボル名を降格し（観測文化・パス参照化・削除）、SPEC の契約を層 1〜3 の語彙で完結させる。

受け入れ条件:

1. E 表の全 50 編集が適用され、各文の契約（観測可能な挙動の主張）が変更前と同一である
2. K 表の keep 群は 1 字も動いていない（負の契約・正本参照アンカー・外部 API 契約・層 1/2 語彙）
3. 再計測（enumerate-spec-spans.mjs）で L4 バケットの残余が K 表の意図した keep のみになる
4. `npm run governance:check` 全 18 検査 green
5. コード（`.rs` 等）の変更ゼロ

## 変更ファイル

`SPEC.md` のみ（1 ファイル・50 編集）。

## 判定規則（3 問テストの運用形・K/E の根拠）

- R1 SPEC が定義側の語（状態語彙・窓名 `main`/`results`・スコア代数・overlay boolean・`kana_query`・`display`）→ 動かさない
- R2 ユーザー観測名（config キー・イベント名・`SNOTRA_*`・テンプレート構文・ホットキー語彙・クレート名・ファイル形式・`last_launched` 等永続形式の語彙）→ 動かさない
- R3 正本参照アンカー（「**正本は** X の Y」形: `icon_cache_cap`・`launch_timeout`・`normalize_entry_key` 見出し参照）→ 動かさない。**「実装は X の Y」形（ナビ）はパスだけ残しシンボルを落とす**（§15.3 L711 のシンボル無し表記が家風の先例）
- R4 負の契約（旧 `compute_window_height`・旧 `get_bootstrap_payload`・旧 `data-tauri-drag-region`・旧 WebView2 `spawn_blocking` 比較）→ 動かさない（N2 の射程）
- R5 外部 API の挙動そのものが契約の主語（`ShellExecuteW` の起動意味論・`SHGetFileInfoW`・`REG_EXPAND_SZ`・`DWMWA_*`・§20.4 updater・§8.6 の `!Send`/`tauri::Window`/`Manager`）→ 動かさない（受容残余）。**その場の主張が API と独立に述べられる場合（順序制約・観測可能な結果）は降格してよい**
- R6 説明文中の実装名 → 観測文へ書き換え or 削除（文の契約を変えない）

## E 表（編集 50 件・行番号は現 HEAD 8d13646 時点）

| # | 行 | 現文言（該当部） | 変更後 |
|---|---|---|---|
| E1 | 46 | デフォルトスキャン対象（`Config::default_scan_paths()`）: | デフォルトスキャン対象（正本は `snotra-core/src/config.rs` の既定値）: |
| E2 | 92 | worker スレッドの `commands::load_icon_pngs` → ColorImage decode → `load_texture`（`egui_shell/icon_textures.rs`・#532 SU4） | worker スレッドで PNG を取得 → decode → テクスチャ登録（`egui_shell/icon_textures.rs`・#532 SU4） |
| E3 | 97 | 取得（`get`）はアクセス順を更新しない | 取得はアクセス順を更新しない |
| E4 | 110-111 | エントリ名に加えて `target_path` | エントリ名に加えてターゲットのフルパス |
| E5 | 117 | 前後空白除去（`trim`） | 前後空白除去 |
| E6 | 123 | `to_hiragana()` で `kana_query` を生成し | ローマ字→かな変換で `kana_query` を生成し |
| E7 | 140 | 実際の kana 文字列の `starts_with` 比較が必要 | 実際の kana 文字列の prefix 比較が必要 |
| E8 | 184 | `main` の高さは `bar_height`（+ status 行・toast 行が出ていればそれぞれ `toast_height` を加算） | `main` の高さはバー高（+ status 行・toast 行が出ていればそれぞれトースト行高を加算） |
| E9 | 184 | `main` が算出し `set_size()` する | `main` が算出して適用する |
| E10 | 194 | ホバーで `selected` 状態は変化しない | ホバーで選択状態は変化しない |
| E11 | 197 | `focusable(false)` でフォーカスを取らない | フォーカスを取らない |
| E12 | 282 | 本体（`snotra`）から `std::process::Command` で子プロセスとして起動 | 本体（`snotra`）が子プロセスとして起動 |
| E13 | 345 | `Config::default()` 相当の値をドラフトに適用する | 既定設定相当の値をドラフトに適用する |
| E14 | 350 | 保存時に `Config::validate()` で以下を検証する | 保存時に以下を検証する |
| E15 | 375 | （保存時の `Config::validate()` がバックストップ） | （保存時検証がバックストップ） |
| E16 | 380 | 検知時に `PlatformCommand::SetHotkey` で再登録 | 検知時に platform スレッドへ委譲して再登録 |
| E17 | 381 | `HotkeyConfig::default()`（`Alt+Q`）へ自動修復 | 既定ホットキー（`Alt+Q`）へ自動修復 |
| E18 | 382 | 検知時に `PlatformCommand::SetTrayVisible` で切替 | 検知時に platform スレッドへ委譲して切替 |
| E19 | 386 | `PlatformCommand::SetLanguage` でトレイメニューを切替 | platform スレッドへ委譲してトレイメニューを切替 |
| E20 | 388 | `AppState` の実行中 config から直接読む | 実行中 config から直接読む |
| E21 | 394 | （権限/ロック/共有違反, `LoadOutcome::ReadFailed`） | （権限/ロック/共有違反） |
| E22a | 395 | index build 完了世代（`index_generation`）の差分で | index build 完了世代の差分で |
| E22b | 395 | （`run_ui` → paint の順に進むため | （view 実行 → paint の順に進むため |
| E22c | 395 | hidden 中は `update()` が走らず | hidden 中はフレームが走らず |
| E23 | 399 | 起動時に `Config::load` が読み込んだ設定を `Engine` が保持し | 起動時に読み込んだ設定を本体が保持し |
| E24 | 421 | （`blur_should_hide` 純粋核・設定で切替・100ms 猶予付き） | （設定で切替・100ms 猶予付き） |
| E25 | 434 | （実装は `src-tauri/src/egui_shell/window_coordinator.rs` の `position_on_target_monitor`） | （実装は `src-tauri/src/egui_shell/window_coordinator.rs`） |
| E26 | 444 | （`egui_shell::create` が `Window::builder(...).decorations(false)` を `main`/`results` 両窓に指定） | （生成時に `main`/`results` 両窓とも装飾なしで作る） |
| E27 | 445 | 入力欄以外の全域への `ui.interact` 検出 + `Window::start_dragging()`（tao `drag_window`）で移動 | 入力欄以外の全域をドラッグ検出し OS のウィンドウ移動へ委譲して移動 |
| E28 | 449 | `main` ウィンドウは `visible: false` で作成し、条件付きで `window.show()` を呼ぶ | `main` ウィンドウは不可視で作成し、条件付きで表示する |
| E29a | 454 | 起動時のセットアップで生成する（いずれも `visible: false`）。`results` は `focusable(false)` でフォーカスを取らない従属窓とし | 起動時のセットアップで不可視のまま生成する。`results` はフォーカスを取らない従属窓とし |
| E29b | 454 | `main` の毎フレーム更新（`drive_results_window`）が駆動する | `main` の毎フレーム更新が駆動する |
| E30 | 455 | 本体は `SettingsProcessState` で子プロセスを管理し | 本体は子プロセスハンドルを保持し |
| E31 | 459 | ホットキー登録（`RegisterHotKey`）は | ホットキー登録は |
| E32 | 517 | `open_settings` は no-op | 設定オープンは no-op |
| E33 | 518 | 初回起動（`is_first_run`）では | 初回起動では |
| E34 | 546 | `results` への `ShowWindow` の間には | `results` への表示適用の間には |
| E35 | 558 | 起動時の setup で 1 回だけ・`visible: false`（ランタイム中は生成しない） | 起動時の setup で 1 回だけ・不可視で生成（ランタイム中は生成しない） |
| E36 | 583 | Win32 `Shell_NotifyIconW` で実装（`platform/tray.rs`） | Win32 API で実装（`platform/tray.rs`） |
| E37 | 669 | 欠損キーはデフォルト補完（`#[serde(default)]` 付きセクション）、未知キーは無視 | 欠損キーはデフォルト補完、未知キーは無視 |
| E38 | 678 | `.lnk` はショートカット本体を `ShellExecute` で起動 | `.lnk` はショートカット本体を `ShellExecuteW` で起動（表記の正確化・R5 の名前へ統一） |
| E39 | 698 | エントリの `target_path` への部分一致 | エントリのターゲットパスへの部分一致 |
| E40 | 841 | `Command::new(exe).args(args_vec)` で起動。`CREATE_NO_WINDOW` フラグ + stdout/stderr を `/dev/null` にリダイレクト | 引数ベクタをシェルを介さずそのままプロセス生成に渡して起動。コンソールウィンドウは出さず、stdout/stderr は破棄する |
| E41 | 854 | 読み込み時に `apply_migrations()` が自動で `url` へ変換 | 読み込み時に自動で `url` へ変換 |
| E42 | 861 | デフォルト登録済みコマンド（`Config::default()`）: | デフォルト登録済みコマンド（既定設定に含まれる）: |
| E43 | 903 | その時点の値が空（`is_empty()`）なら | その時点の値が空なら |
| E44 | 958 | 1. `split_args`: args テンプレートを | 1. トークン分割: args テンプレートを |
| E45 | 1005 | 実行時に `action` フィールドの種別に応じてディスパッチ | 実行時にコマンドの種別（URL / プログラム実行）に応じてディスパッチ |
| E46a | 1007 | **URL 種別** (`InstantAction::Url`): | **URL 種別**: |
| E46b | 1012 | **プログラム実行種別** (`InstantAction::Exec`): | **プログラム実行種別**: |
| E47 | 1010 | 既存の `launch_item_core` を再利用 | 既存の起動経路を再利用 |
| E48 | 1013 | `expand_exec_args(args, query, clipboard, env_expand)` で引数ベクタを構築 | 引数ベクタを構築 |
| E49 | 1016 | `Command::new(exe).args(args_vec)` で生成 | 引数ベクタをそのままプロセス生成に渡す（§19.3 と同じ・シェルを介さない） |
| E50 | 1029 | イベントループスレッドで `ShellExecuteW` / `spawn` を同期実行しない | イベントループスレッドで起動を同期実行しない |
| E51 | 1048 | `src-tauri/src/egui_shell/search_state.rs` の `interpret`（クエリからモードを決める） | `src-tauri/src/egui_shell/search_state.rs` のクエリ解釈 |
| E52 | 781, 1050 | （実装は `src-tauri/src/egui_shell/search_state.rs` の `reset`） | （実装は `src-tauri/src/egui_shell/search_state.rs`）※2 箇所同文・replace_all で適用 |
| E53 | 1120 | 検索バー直下に `toast_height`（= `bar_height` と同値・既定 43px）の**単一行**として | 検索バー直下にトースト行高（= バー高と同値・既定 43px）の**単一行**として |
| E54 | 1122 | show 時は `bar_height` collapse 後に toast 分へ拡張する | show 時はバー高への collapse 後に toast 分へ拡張する |
| E55 | 1133 | `SNOTRA_TRACE` の `egui_update_install_failed` には全文が残る | `SNOTRA_TRACE` のトレースには全文が残る |
| E56 | 1145 | `Err` 復帰（download 失敗等）時のみ | 失敗復帰（download 失敗等）時のみ |

注: 同一行の複数編集（184・395・454・1007/1012）は E 番号を分けてあるが 1 回の Edit でよい。行番号は目安であり、置換は「現文言」の一意一致で行う（E52 のみ 2 箇所同文の意図的 replace_all）。

## K 表（意図した keep・動かさない群）

| 群 | 対象（代表） | 理由 |
|---|---|---|
| K1 層 1 | `main` / `results`・§8.6/§19 状態語彙とガード式・`indexing`/`launching` overlay・`kana_query`（§4.2 が生成規則ごと定義し §4.2.1 不変条件が名前を要る）・`display`（SPEC 自身が「バックエンドが算出する派生値」と定義）・スコア代数（`final_score` 式と 4 変数） | SPEC が定義側。ドリフト率ゼロ |
| K2 層 2 | config キー（serde フィールド全数照合済み・`appearance.window_width` 含む）・イベント名 7 種・`SNOTRA_*`・テンプレート構文（`{query}`・修飾子 `trim`/`lower` 等——L117 の std メソッド `trim` とは別語義）・ホットキー語彙・クレート名・ファイル形式・`last_launched`（永続形式の語彙）・`panic = "abort"`（ビルド設定キー） | ユーザー観測名。変更＝破壊的変更で自己同期 |
| K3 層 3 | パス参照 119 件・`config_watcher`（`src-tauri/src/config_watcher.rs` のファイル名＝コンポーネント呼称・独立導出の裁定を採用） | 機械照合可能な辺 |
| K4 アンカー | `icon_cache_cap`（97・config.rs 側に逆リンクあり双方向）・`launch_timeout`（1035）・`normalize_entry_key` ほか CLAUDE.md 見出し参照（735-736・G-heading-refs 射程）・`FOLDER_EXPANSION_WEIGHT = 5`（151・SCREAMING_SNAKE は G-stale-identifiers 射程で実在照合される） | 「正本は」形の SSOT 委譲。機械の守りがある辺 |
| K5 負の契約 | 旧 `compute_window_height`（184）・旧 `get_bootstrap_payload`（399）・旧 `data-tauri-drag-region`（445）・旧 WebView2 `spawn_blocking` 比較（1030,1036）・V1 廃止（730） | N2（極性反転）の正準形。装置裁定前に形を変えない |
| K6 外部 API が主語 | `ShellExecuteW`（821,1009・起動意味論とverb "open"）・`SHGetFileInfoW`（87）・`REG_EXPAND_SZ`（54）・`DWMWA_*`（623）・§20.4 updater（`Update`/`UpdaterExt`/`on_before_exit`/`download_and_install`/`std::process::exit(0)`/`app.restart()`）・§8.6 の `!Send`/`tauri::Window`/`Manager`（残る生の面の同定が主語・正本は `src-tauri/CLAUDE.md` へ委譲済み） | 主張が外部 API の挙動そのもの。**受容残余**（版数連動の腐りは Cargo.lock が固定） |

## 実装順序

- Phase 1: E 表 50 件を SPEC.md へ適用（上から順・各編集は独立）
  - [ ] E1〜E15（§2〜§6）
  - [ ] E16〜E24（§7 設定反映）
  - [ ] E25〜E36（§8〜§9）
  - [ ] E37〜E44（§16〜§19 前半）
  - [ ] E45〜E56（§19 後半〜§20）
- Phase 2: 検証
  - [ ] 再計測スクリプト実行 → L4 残余が K 表のみであることを確認し、before/after の件数を記録
  - [ ] `npm run governance:check` green
  - [ ] `git diff` を通読し、各編集が「契約不変・語彙のみ降格」であることを 1 件ずつ確認
  - [ ] コード無変更の確認（`git status` で SPEC.md のみ）
- Phase 3: issue #884 へ残余人口の実測値をコメント（装置 2〜4・6 の裁定材料）— PR 作成後

## 不変条件と異常系

- 各文の観測可能な契約は 1 件も変わらない（挙動変更ゼロ・記述形式のみ）
- 負の契約形（K5）・正本参照アンカー（K4）は 1 字も動かさない
- §11 は触らない（#888 で処置済み・並行ブランチ `fix/visual-color-same-frame` との衝突回避）
- 置換が一意一致しない場合はその場で前後文を広げ、機械置換で別箇所を巻き込まない（例外は E52 の意図的 replace_all）

## テスト方針と検証コマンド

- コード変更ゼロゆえ cargo 系は該当なし（カテゴリ A〜E 該当なし）
- カテゴリ F: `npm run governance:check`
- 独自検証: enumerate-spec-spans.mjs の before/after 比較（受け入れ条件 3）

## SPEC.md・関連文書の更新要否

- SPEC.md 本体が変更対象。他文書への波及なし
- issue #884 へ実測値コメント（Phase 3）

## 未確定（実装前に潰す）

（なし——置換文言は全件、現文言を読んで確定済み。独立導出との差分 14 件も下記のとおり裁定済み）

## plan-review 結果

- リスク: 高（網羅性が要件・--deep 明示）
- レビュー方式: 独立導出1体（Step 2b・`workspace/plan-review-vocab-demotion.md`）
- エージェント数: 1

### 要対処（すべて計画へ反映済み）

- 導出 ∖ plan の漏れ 12 件を E 表へ追加 — `target_path`×2（config キーでないことを serde 定義で再照合・indexer.rs 内部フィールド）・`trim`(117)・`selected`(194)・`focusable(false)`(197)・`visible: false`(558)・`#[serde(default)]`(669)・`is_empty()`(903)・`action`(1005・`#[serde(flatten)]`+`untagged` でユーザーは書かないことを再照合)・`InstantAction::Url/Exec`(1007,1012)・`reset`(1050)・`Err`(1145)
- 判断の不一致→導出側を採用 2 件 — `bar_height`/`toast_height` 5 出現を降格（導出ペアが情報を保持・kana_query と違い不変条件が名前を要らない）・`config_watcher` を keep へ変更（ファイル名＝層 3 相当）
- 判断の不一致→plan 側を維持 2 件 — `FOLDER_EXPANSION_WEIGHT = 5` keep（SCREAMING は G-stale-identifiers の機械照合射程・導出アンカー）・`tauri::Window`/`Manager`(548) keep（導出の「外部 API の同一性が主語」裁定をむしろ採用し、当初の E43 を取り下げ）

### 軽微

- `RegisterHotKey`(459)・`CREATE_NO_WINDOW`(841)・`Shell_NotifyIconW`(583)・`egui_update_install_failed`(1133) は導出が keep（許容）と判定したが plan は降格を維持——当該サイトの主張が API 名と独立に述べられるため（R5 の但し書き）。どちらでも欠陥ではないと導出自身も認定
- `/dev/null`(841) の事実誤り（Windows に無い比喩）は E40 が同時に解消する

### 未検証

- なし（層 4 候補の実在照合は導出が全数 grep 済み・config キー判別は serde 定義と突き合わせ済み）

### 判断

- 実装着手: 人間の裁定待ち（5c）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2b 独立導出）
- エージェント数: 1
- 要対処: 14 件（漏れ 12 追加・裁定変更 2。全て E/K 表へ反映済み）
- 未検証: なし

## 人間レビュー

- [x] 承認済み — 2026-08-03 / 問い: "workspace/plan.md の計画（SPEC.md の層 4 語彙降格 50 編集）を承認して実装へ進んでよろしいですか？" / 回答: "承認する"
