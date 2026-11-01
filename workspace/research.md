# research — #1336 src-tauri/CLAUDE.md を、テストにも型にも載らない規則だけに絞る

## 要約

`src-tauri/CLAUDE.md`（main・2026-11-01: 199 行 / **29,849 字**（`LC_ALL=C.UTF-8 wc -m`。issue の 54,930 は既定ロケールの `wc -m` が返したバイト数——3b B8）/ 太字 116 組＝`**` 232 個）の条項を、issue の 5 分類（1 テスト/型が守る・2 安くテスト化・3 箇所へ移す・4 文書に残す・5 消す）へ振り分ける。段 1 は振り分け表を出して裁定を待つ。

## 最大の発見: 条項の大半は、コード側に同じ内容が既にある

hooks.md と同じ形。ただし**指す向きが逆**——コード側の doc はほぼ全文を持ちながら「正本は CLAUDE.md」と名指している（例: `state.rs` `AppState::read_config` の doc は CLAUDE.md「config の読み」節の 4 bullet をほぼ逐語で持つ。`config_watcher.rs` `apply_config_change` の本体コメントは「モジュール構成」の config_watcher 2 条項を逐語で持つ）。3 の多くは「移す」ではなく「消して、指しを裏返す」になる。

## 振り分け表（段 1 の成果物）

凡例: 分類の `1?` は**読みだけの 1（変異未確認・仮置き）**。`3=` は移設先に**同じ内容が既にある**（消して指しを外すだけ）、`3+` は移設先へ書き足す。

### 冒頭

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| A1 | `[lib]` 無し → `cargo test -p snotra --lib` は失敗する | 5 | 失敗は即時で自己説明的（`no library targets found`）。正しい形は `docs/build-commands.md` カテゴリ A に既にある。2026-08-03 の 2 回は同日の一件 |
| A2 | 「リード=指示・後続=理由」の読み方 | 5 | 文書の読み方の説明。絞った後は不要 |

### モジュール構成（索引は残す。G-module-index が照合）

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| M0 | 節冒頭の「`//!` が正本・本節は横断不変条件」 | 4（縮約） | 索引の性格宣言として 1 文残す |
| M1 | state: config→index コヒーレンシを 2 AtomicBool から導かない | 3+ | `state.rs` の `indexing` / `index_build_started` フィールド doc へ（現在フィールド doc 無し） |
| M2 | indexing: mark_stale→CAS→spawn の順・drain ループ | 3= | `indexing.rs` `//!` と `start_index_build` doc・本体コメント |
| M3 | indexing: 索引を建てる手順は `build_index_from_material` 1 か所・起動経路は別に持つ | 3= / 3+ | `build_index_from_material` doc が持つ。**命令「PATH マージの規則を変えるときは起動経路も」が欠ける**（3b B4）→ 同 doc と `main.rs` の PATH マージ箇所へ相互の指しを足す |
| M4 | indexing: 木とマスクは組で運ぶ | **1（変異で確認: E0616）** | `IndexMaterial`（snotra-core）の型が片方だけ伸ばす形を書かせない。同関数本体コメントにもある |
| M5 | config_watcher: icon 破棄は `update_config` より後 | 3= | `apply_config_change` 本体コメントが逐語で持つ。`//!` の「CLAUDE.md を正とする」を外す |
| M6 | config_watcher: `IndexInputs` 差分判定・`!indexing` ゲート無し | 3= | 同本体コメント（#347/#348-A） |
| M7 | config_watcher: 発火イベント列挙・SU7 で消えた 7 本 | 5 | 列挙は書いた瞬間から腐る数え上げ。何を emit するかは `apply_config_change` 本体（`emit` を grep）が正本——`events.rs` は位置を持たないと自ら書く（3b B12 で根拠を差し替え）。7 本の経緯は本体コメントにある |
| M8 | commands/・platform/ の責務記述（9 行目が「本節に直接記す例外」と宣言） | 3+（責務） / 5（公開関数列挙） | `commands/mod.rs`・`platform/mod.rs` とも `//!` が 0 行（確認済み）→ 責務を `//!` へ書いて例外を消す。公開関数の列挙はコードが正本 |
| M9 | platform/hotkey.rs: `ParsedHotkey` だけを変換し永続文字列を再解釈しない | 3+ | `platform/hotkey.rs` の `//!` へ（`//!` 0 行を確認済み） |
| M10 | launcher_controller 子モジュールの可視性（`pub(in crate::egui_shell)`） | 1?（型） | 可視性はコンパイラが強制。索引には子ファイル名だけ残す |
| M11 | ソーステキスト検査の置き場（`activation/tests.rs`・非再帰） | 3= | `activation/tests.rs` の `//!` が非再帰・母集団を逐語で持つ（確認済み） |
| M12 | window_coordinator: z-order は含まない | 3= | `window_coordinator.rs` `//!` が逐語で持つ |
| M13 | 幾何 1: main のサイズは show と毎フレームの 2 か所・同じ高さ | 3+（移す） | `window_coordinator.rs` `//!` が「正本は CLAUDE.md」と名指している→ 内容を `//!` へ引き取り、`view.rs:972,1239`・`layout.rs:394` の指しを付け替え |
| M14 | 幾何 2: 基準モニター判断 3 箇所はバー高で共通 | 3= / 3+ | `read_bar_anchor` doc・`layout::bar_rect_height_phys` doc（確認済み）。3 箇所の一覧は `window_coordinator.rs` `//!` へ |
| M15 | 幾何 3: 基準モニターは 2 対 1・`read_bar_anchor` で共有 | 3= | `read_bar_anchor` doc が理由ごと持つ |
| M16 | 幾何 4: `MonitorFromWindow` を位置決めに使わない | 3= | `monitor::point_monitor_work_area` doc（`monitor.rs:78`） |
| M17 | lifecycle / search_state / layout 等の 1 行索引 | 4（索引） | G-module-index |

### 処置を返す純粋核の強制（#934）— 外から 2 箇所が見出しを正準形で指す（lifecycle.rs:35, search_state.rs:129）

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| P1 | `&mut self` で状態を進めてから処置を返す関数は `#[must_use]` | 4 | crate 横断の約束で特定箇所に紐づかない。新規関数に付け忘れても何も落ちない（`clippy::must_use_candidate` は別物・pedantic） |
| P2 | 配置: 自前型は型段・primitive/外来型はメソッド段 | 4（P1 に統合） | 同上 |
| P3 | 参照・`impl Trait`・ジェネリックで包むなら型段に加えメソッド段（probe 実測） | 4（縮約） | 気づきにくく（シグネチャ変更で黙ってガードが消える）、テスト不能。probe の経緯は削る |
| P4 | `= "…"` で失うものを名指す | 4（P1 に統合） | 1 句 |
| P5 | 対象外 2 種（token 返し・状態を進めない導出） | 4（縮約） | P1 の述語の境界。列挙した関数名は削る |
| P6 | 「落とせなくなった」と書かない・受容残余 4 件 | 5 / 4（一部） | 文書の書き方のメタ規範。`try_begin_install` の残余は同関数 doc（`notify.rs:182-187`）が持つ。**ただし「`let _ =` で今も逃げられる（`let_underscore_must_use` は pedantic で未有効）」はコード側に無い**（3b B5）→ P1 の 4 に 1 文で統合 |
| P7 | 型段へ移すとき古いメソッド段を目で消す（`double_must_use` を当てにしない） | 5（3b B5 は 4 寄りと指摘・裁定事項） | 一度きり（#932→#934）で再発記録なし。`-D warnings` の所在は root `Cargo.toml` コメントが正本 |

### 外部からウィンドウを起こすときは wake_main / wake_results（#671 PR D）

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| W1 | 外部 wake は `wake_main` / `wake_results` を通す | **1（変異で確認: E0616）**。射程: `EguiShellHandles` の waker は `pub(crate)` で、handles を握る `main.rs` の setup では直呼びが書ける（3b B10）→ 射程の 1 文を `EguiShellState` の waker フィールド doc へ（3+） | **本文の「`main_waker` は `pub(crate)` ゆえ直呼びが通る」は今は偽**——フィールドは private（`egui_shell/mod.rs:173,175`）。egui_shell 部分木の外からは綴れない。変異（crate の別モジュールから `sh.main_waker.wake()`）で E0616 を確かめる |
| W2 | managed state に `egui::Context` の clone を置かない | 3= | `snotra-egui-runtime/CLAUDE.md`「不変条件」が正本として持つ・`EguiShellState::main_waker` フィールド doc にも旧実装の記述 |
| W3 | 自ウィンドウの Context を持つ場所では `ctx.request_repaint()` | 5 | 一般則（自明） |

### イベント駆動 wake の不変条件（#532 SU5）— race-check SKILL・notify.rs:84・window_coordinator.rs:848 が指す

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| E1 | paint 後・worker で UI 状態を変えたら `request_repaint()` | 4 | 横断・テスト不能・気づきにくい（stale 表示が次の入力まで残る）。外部からの指しが 3 本 |
| E2 | hidden 中は `update()` が走らない・hide を跨ぐ状態は reset-on-show とセット | 4（縮約） | 同上。機序（tao 配送抑止）は `wake_main` doc が持つ |

### 期限を待つ状態（armed）（#711）

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| D1 | armed 期限は毎フレーム残余を再要求 | 4（E 節と統合） | 横断。`snotra-egui-runtime/CLAUDE.md` は保証側だけを持ち、消費側の規範は本書を指す |
| D2 | armed 期限は 4 つ（列挙） | 5 | 数え上げ（腐る） |
| D3 | 再要求してよいのは時間経過で解消する不成立だけ（ZERO スピン） | 4 | 気づきにくく高くつく（CPU 永久スピン） |

### テーマ色・font・行高の読みは 1 フレーム 1 回（#673）— code-reviewer.md:124 が指す

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| V1 | `read_visual` を `update()` で 1 回・snapshot は `[visual]` 全体ではない（`window_gap`） | 3= | `visual.rs` `//!` が逐語で持つ |
| V2 | 後段で読み直さない・`self.` に保持しない | 3= | 同 `//!`「保持しないこと」 |
| V3 | 色パーサは `from_hex` 1 本・ブラシは `visual::native_brush_color` | 3= ＋**偽の訂正** | パーサ 1 本は `visual.rs` `//!`、alpha 255 固定と premultiply は `native_brush_color` doc（`window_coordinator.rs:150-163`）と `background_color_premultiplies_…` テスト。**`visual::native_brush_color` は偽**——実体は `window_coordinator.rs:164`。**`config_watcher.rs:23` も同じ偽を持つ**（3b B2） |
| V4 | 背景色は style を経由しない・3 値は `ui.visuals_mut()` | 3= | `search_input_ui` doc |
| V5 | `ctx.set_visuals` 等 global style を書かない | **1（変異で確認: clippy disallowed）** | `src-tauri/clippy.toml` `disallowed-methods` 群 1。deny の機序は root `Cargo.toml` の `[workspace.lints.clippy] disallowed_methods = "deny"`（3b (b)。本文の「`-D warnings` が deny 化」は機序として不正確）。穴（`options_mut`/`memory_mut`）も同ファイルが正本。変異で確かめる |
| V6 | 3 値の適用は `search_input_ui` の入口 | **1（変異で確認: テスト FAILED）** | `search_input_ui_applies_theme_values_to_child_ui_in_the_first_pass`。残余（`update()` 側の先行読み）は同 doc。変異で確かめる |
| V7 | main に egui コンテナを足すなら自分で visuals を渡す | 4 | 気づきにくい（既定色で描かれる）・テスト不能・特定箇所に紐づかない（新規コンテナはどこにでも足せる） |
| V8 | 却下した代替案の ADR 名 | 5 | 経緯は ADR 自身。`search_input_ui` doc が ADR を名指し済み |

### config の読みは read_config を通す（#1032 / #1123）— architecture.md:247 等が参照

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| R1 | 口は 2 つ・`AppState.config` は private（E0616）・構築は `AppState::new`（E0451） | **1（変異で確認: E0616 / E0451）** | 可視性。`AppState::read_config` / `new` の doc が逐語で持つ。変異で E0616/E0451 を確かめる |
| R2 | `read` へ渡すのは純 CPU だけ（錠・I/O 禁止・engine.lock は永久待ち） | 3= / 3+ | 「射程は読みだけ・書き込みは `update_config` 1 本で engine lock の内側」が doc に欠ける（3b B11）→ 1 句足す。残りは `AppState::read_config` doc が逐語で持つ |
| R3 | 条件は付かない・`Engine::config` は `#[cfg(test)]`（E0599） | **1（変異で確認: E0599）** | 変異で E0599 を確かめる |
| R4 | 規範は機構より広い（`config_handle().read()` 迂回・錠越しの `search` 等） | 3= / 4? | 迂回は `read_config` doc が持つ。「`Engine` の他メソッドを錠越しにイベントループから呼ぶ形も待つ」は read_config の射程外の一般則→ **4 で 1 文残すか要裁定**（`snotra-core/CLAUDE.md` のロック最小化と重なるかも） |
| R5 | 前史と却下案の ADR 3 本 | 5 | ADR 自身が経緯の正本 |

### trace の presence 検査は状態の検査ではない（#671 PR A′）— build-commands.md:92・smoke-egui.ps1:701・visual-check-colors.ps1:32 が正準形で指す

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| T1 | trace で不変条件を守るなら「起きてはならないことが起きていないか」を書く | 4 | 気づきにくく（smoke が緑のまま通した実績）、テスト不能、検査を書く人全般に効く。3 本の正準形参照あり。**ただし src-tauri 固有ではない**→ `docs/build-commands.md` に移す案もあるが「ほかの文書へ手を広げない」の制約で残す |

### 実装パターン — `.claude/rules/src-tauri.md` が「実装パターン」を指す

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| I1 | ホットキーは platform スレッド → emit | 5 | 構造の説明（コードと `docs/architecture.md`） |
| I2 | 設定画面は子プロセス・`SettingsProcessState`・topmost 一時解除 | 3= | 型 doc（`commands/window.rs:17`）・`launch_settings_process` doc（重複防止）・本体コメント（topmost 解除と復帰）が持つ（3b B3 で 3+ から訂正） |
| I3 | `commands/` は薄く保ち実処理は core（KISS） | 5 | 一般的な良い作法 |
| I4 | `AppState` は `Mutex<Engine>`・Phase 2.3 以前の 3 重ロック | 5 | `state.rs` `//!` が構成を持つ。経緯は不要 |
| I5 | index build フラグは `try_begin`/`finish` 経由・直接 `store()` しない | 3+ ＋ **2 候補** | フィールドが `pub` で型は守らない。`indexing` / `index_build_started` のフィールド doc へ移す（M1 と同所）。フィールドを private 化する案は型で守れるが「やらないこと」に照らし範囲外か要裁定（first-run で `indexing` を立てる経路が `AppState::new` 経由なら private 化は安い） |
| I6 | `IconCacheState` / `SettingsProcessState` の型説明 | 5 | 型定義が正本 |
| I7 | show の操作順序（position → set_size → show）・位置はバー高/サイズは実高 | 3+ | **3=**: `show_egui_main` 本体コメント（`window_coordinator.rs:293-300,359-363`）が順序・バー高/実高・`SWP_NOMOVE`・2 回撃っていた経緯まで持つ（確認済み）。「かつて 2 回撃っていた」経緯は 5（ADR） |

### 共有 core 関数の返り値契約 — SPEC.md:767・code-reviewer.md:89・commands/system.rs:19 が正準形で指す

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| C1 | 3 系統（素の T / LaunchResult / Result<T,String>） | 3+ | `commands/mod.rs` の `//!` へ（`//!` 0 行を確認済み・M8 と同所）。SPEC.md:767・code-reviewer.md:89・system.rs:19 を付け替え |
| C2 | `bool` 返しを新規関数で使わない | 3+（C1 と同所） | |
| C3 | `ERR_INDEXING_IN_PROGRESS` を共有・`ERR_<状態>` 命名 | 3=（共有） / 3+（命名） | 定数 doc が共有を持つ。命名規約を 1 句足す |

### Win32 メッセージ配送の注意

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| N1 | トレイ callback は SendMessage で来る → 再投入 | 3= | `platform_default_wnd_proc` 本体コメント |
| N2 | `app.listen` のコールバックは emit 元スレッドで同期実行・listener 追加=worker 追加 | 4 | 気づきにくく（tauri の挙動・ソース確認が要る）、高くつく（並行境界の見落とし）、テスト不能、`app.listen` は crate 中に散在。**コード側に記述なし**（grep 0 件）→ 残す。`AGENTS.md` の race-check トリガーは「Tauri listener」を既に含む |
| N3 | `WM_CONTEXTMENU` はキーボード経由で直接届く → 再投入 | 3= | 同本体コメント |
| N4 | メッセージハンドラ削除前に全経路列挙 | 3+ | `platform_default_wnd_proc` の doc へ 1 文（例が同所にある） |

### ウィンドウ生成の制約 — rules/src-tauri.md・hide_request.rs:5・window_coordinator.rs:412,476,557・main.rs:616 が指す（正準形 6 本）

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| G1 | 生成は setup 限定・イベントループ中のコールバックではポンプ停止→デッドロック | 4 | 気づきにくい（「メインスレッドにいる」と「ポンプが回る」の区別）・高くつく（デッドロック）・テスト不能・6 本の正準形参照。**節名を残す** |
| G2 | `CloseRequested` ハンドラ不要（decorations: false） | 5 | 自明・構造の説明 |
| G3 | setup フック自身もイベントループの中 | 4（縮約） | 同上。帰結 1（manage 順）は `main.rs:295-310` のコメントが逐語で持つ→ 帰結 1 は 3= |
| G4 | setup 内でポンプ前提の操作を足さない | 4（G1 に統合） | |

### working set の能動回収（EmptyWorkingSet）— PERFORMANCE.md:87・working_set.rs:5・window_coordinator.rs:530,545 が指す

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| S1 | hide 経路で EmptyWorkingSet をツリーへ | 3= | `working_set.rs` `//!` が逐語で持つ |
| S2 | show 側に untrim 不要 | 3= | 同 `//!` |
| S3 | best-effort・物理 RAM のみ・任意スレッド | 3= / 3+ | 同 `//!`（「任意スレッドから呼べる」の句だけ無い——呼び出し点が hide 経路 1 つなので 5） |

→ 節ごと消し、4 本の指しを `working_set.rs` へ付け替え。

### フォント登録（混在スクリプトのベースライン）

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| F1 | 規則は `font_definitions_*` テストが固定 | **1（変異で確認: 2 変異とも FAILED）** | `font_stack.rs` の 4 テスト。変異（`insert(0, …)` → `push`）で確かめる |
| F2 | softbuffer 固有の増幅（`fill_mesh` にカバレッジ AA 無し） | 3+ | `font_stack.rs` `//!` へ 1 文 |
| F3 | 再発経路（#399 → #579） | 3= | テストのメッセージと本体コメント（`font_stack.rs:83,261`） |
| F4 | フォント登録変更時は目視スモークを省略しない | 3+ | `font_stack.rs` `//!` へ（その箇所に紐づく） |

### Win32 / Tauri 注意事項 — rules/src-tauri.md（2 行）・results_window.rs:151・SnotraTraceInvariants.psm1:8 が指す

| # | 条項 | 分類 | 根拠 |
|---|---|---|---|
| X1 | Win32 不具合はまず config（テーマ）を見る | 5 | 一度きりの事件（白画面）・一般的なデバッグ作法 |
| X2 | crate 昇格は crates.io 実在確認（bincode 3.0.0） | 5 | 一般作法・src-tauri 固有でない |
| X3 | `windows` crate の API 型はバージョン依存 | 5 | コンパイラが即座に教える |
| X4 | 宣言的属性（`focusable(false)`）の全分岐確認・tao の `MARKER_DONT_FOCUS` | 3= | `results_window.rs:108-111` が機序を持つ |
| X5 | show/hide/topmost は 1 つ迂回したら全部迂回・main=tao / results=raw | 3= / 4? | `results_window.rs:150` 付近・`ResultsWindow` doc が持つ。results 側は `commands/window.rs:105-107` も持つ。**main 側の「main の show だけ raw にするのは禁止」はコードに無い**（確認済み）。**横断の規則なので 4 で 1 文残す案あり** |
| X6 | raw へ寄せるかは「フラグ差分が生じるか」で判定 | 3= | `results_window.rs:242-246` が逐語で持つ |
| X7 | 可視性は show 述語側のゲート（`present_results`） | 3= | `results_window.rs` `//!`・`layout::present_results` doc |
| X8 | 可視性操作は `&EventLoopProof` を要求 | **1（変異で確認: E0277 `*const ()` は Sync でない）** | 証人型。`show_egui_main` doc が「外してはならない」を持つ。変異（別スレッドから呼ぶ）で確かめる |
| X9 | 5 関数の閉包は表現不能化ではない（`Manager` から main を `.hide()` は効く） | 3+（移す） | `state.rs:37`・`layout.rs:453`・`results_window.rs:28,122` が**この bullet 群を正本と名指す**。移設先は `hide_egui_main` doc（要裁定） |
| X10 | `results 可視 ⇒ main 可視` は事前ゲート 1 点 | 3= | `present_results` doc・`ADR-results-gate-single-point` |
| X11 | race を lock で囲まない（cross-thread ShowWindow） | 3= | `results_window.rs:337` が持ち、見出し語で本書を指す → 指しを外す |
| X12 | presence 検査では捕まらない・H1 | 3= | `SnotraTraceInvariants.psm1` 先頭コメント |
| X13 | feature フラグを Cargo.toml で確認 | 5 | コンパイラが即座に教える |
| X14 | `UpdateWindow` 等が未提供のことがある | 5 | 同上 |
| X15 | `C:` と `C:\` の違い | 5 | 一般知識（Windows パス） |
| X16 | `symlink_metadata` | 5 | 一般知識。src-tauri に使用箇所なし（grep 0 件） |
| X17 | 2 回呼び出しの clamp（#394） | 3+ | `expand_env` は既に `.min(buf.len())` している。理由（release `panic="abort"`）を 1 行足す。`snotra-core/src/indexer/path_env.rs:66-82` にも同形があるが戻り値を使わず panic しない（3b B6）——他 crate は範囲外ゆえ触らない |
| X18 | Tauri プラグインの capabilities 宣言 | 5 | Tauri の一般知識・実行時エラーで即時に分かる |
| X19 | `ShellExecuteW` は COM STA・S_FALSE も CoUninitialize | 3+ | `launch_item_core` に一部（S_FALSE）あり。STA が要る理由を 1 行足す。rules/src-tauri.md の行を付け替え |
| X20 | `SendInput` とフォーカス・`SendMessageTimeoutW(WM_NULL)` は他スレッド所有の宛先にだけ有効 | 3= / 3+ | `window_coordinator.rs:391-405`・`platform/mod.rs:324` の撤去跡コメントが持つ |
| X21 | 例外: `SnotraPlatformWindow` には有効 | 3+ | コード側に無い（3b B7）→ `platform/mod.rs` の `SnotraPlatformWindow` 生成箇所へ 1 文。tray.rs の `WM_NULL` はメニュー消去用の `PostMessageW` で同期待ちではない——本文の例示は誤導なので移さない |

## 今も偽の記述（段 2 で直す）

1. **V3**: `visual::native_brush_color` → 実体は `window_coordinator::native_brush_color`
2. **W1**: `EguiShellState` の `main_waker` / `results_waker` は `pub(crate)` → 実際は private（`egui_shell/mod.rs:173,175`）
3. **`main.rs:342,413`** が「listener を先に登録してから hotkey を有効化する順序」を `src-tauri/CLAUDE.md` に委ねているが、本書にその記述は無い → `docs/architecture.md`「起動シーケンスと初期化順序」（「有効化 ≥ リスナー登録」不変条件）へ付け替える（3b B9）
8. **`config_watcher.rs:23`** も `egui_shell::visual::native_brush_color` と書く（3b B2）
~~4・5（撤回）~~: `###` 7 節（処置〜trace）は `## モジュール構成`（7 行目〜109 行目の `## 実装パターン` 直前）の**内側**にあるので、「モジュール構成」の #1032 条項・trace 規範という指しは正しい（advisor 指摘で撤回）。**帰結**: `###` 群を消す/移すと「モジュール構成」節の範囲が変わり G-module-index の照合範囲も動く（段 2 で `G-module-index.mjs` の `ending` を読む）
6. **`.claude/agents/code-reviewer.md:68`**: イベント名のリテラル禁止の正本を「モジュール構成」とするが、実際は `events.rs` の `//!`
7. **`config_watcher.rs` `//!`**「多サブシステムに跨る不変条件は CLAUDE.md を正とする」→ 本体コメントが全文を持つ

## 参照（行き先を失いうるもの）

ADR・`docs/superpowers/` を除く `git grep 'src-tauri/CLAUDE.md'` の全行（2026-11-01）**に加え、パス無しの見出し参照**（`main.rs:302`「ウィンドウ生成の制約」・`.claude/rules/src-tauri.md:8,10,12-15` の「該当節」と見出し名・`heap_trace.rs:23` の「モジュール索引行」・`config_watcher.rs:8`）を段 2 の付け替え母集団とする（3b B1）。パス無しの形は G-heading-refs に載らないので手で潰す。正準形で見出しを指すもの（G-heading-refs が照合）は、消える見出しごとに次のとおり:

- 「処置を返す純粋核の強制」: lifecycle.rs:35, search_state.rs:129 → **見出しを残す（P 節は 4）**
- 「イベント駆動 wake の不変条件」: race-check SKILL, notify.rs:84, window_coordinator.rs:848, G-heading-refs.mjs:16（コメント内の例示） → 残す
- 「テーマ色・font・行高の読みは 1 フレーム 1 回」: code-reviewer.md:124 → 節が消えるなら `visual.rs` へ付け替え
- 「config の読み…」: 正準形参照は無い（architecture.md:247 は散文）→ 要再確認
- 「trace の presence 検査は状態の検査ではない」: build-commands.md:92, smoke-egui.ps1:701, visual-check-colors.ps1:32 → 残す
- 「実装パターン」: rules/src-tauri.md:12 → 付け替え
- 「共有 core 関数の返り値契約」: SPEC.md:767, code-reviewer.md:89, system.rs:19 → 付け替え
- 「ウィンドウ生成の制約」: rules/src-tauri.md:13, hide_request.rs:5, window_coordinator.rs:412,476,557, main.rs:616 → 残す
- 「working set の能動回収」: PERFORMANCE.md:87, working_set.rs:5, window_coordinator.rs:530,545 → 付け替え
- 「Win32 / Tauri 注意事項」: rules/src-tauri.md:14,15,21, results_window.rs:151, SnotraTraceInvariants.psm1:8 → 付け替え
- 「モジュール構成」: code-reviewer.md:68, activation.rs:348, search_flow.rs:44, layout.rs:394, mod.rs:21, view.rs:972,1205,1239, window_coordinator.rs:26,192, state.rs:101 → 見出しは残る（索引）が、指す中身が消えるので全部付け替え
- 「可視性を変える操作はイベントループスレッドに閉じてある」（bullet リード）: layout.rs:453, results_window.rs:28,122, state.rs:37 → 付け替え（X9 の移設先へ）
- 「この種の race を lock で囲んではならない」: results_window.rs:337 → 付け替え

## 再利用できる既存パターン

- hooks.md の撤去（#1334 / PR #1335・`ADR-hooks-doc-retired`）: 削除台帳・段 1/段 2・変異での「1」確認
- G-heading-refs: `.rs` から正準形で指しているものは、見出しを消すと赤になる → 付け替え漏れの検出器として使う

## 技術的制約

- `G-module-index`: 「モジュール構成」節にファイル名の索引が要る（`MODULE_INDEX_CRATES`）
- 変異の複製: `git archive` で全追跡ファイルを写す。`cargo test -p snotra` / `cargo clippy -p snotra` を複製で回す。`CARGO_TARGET_DIR` は複製専用の 1 つを共有し、変異は直列に当てる（並列にすると target の錠で直列化されるうえ、成果物が混ざる）
- 書き込みが当たったか: node で書き、原本との差分を確かめる

## 未解決の疑問（段 1 の裁定事項）

1. **I5（index build フラグ）**: フィールド doc へ移す（3）か、`indexing` / `index_build_started` を private にして型で守る（実質 2）か
2. **R4**: 「`Engine` の他メソッドを錠越しにイベントループから呼ぶ形も待つ」を 4 で残すか、消すか
3. **X5 / X9**: 可視性の層規則（main=tao / results=raw・閉包は表現不能化ではない）を `ResultsWindow` / `hide_egui_main` の doc へ移すか、横断規則として 4 で残すか
4. **T1**: src-tauri 固有でない規則を本書に残すか（他文書へ移すのは範囲外）

## 変異測定（段 1・2026-11-01）

複製: `git archive HEAD` を scratchpad へ展開・`CARGO_TARGET_DIR` は複製専用。書き込み後に原本と違うことを確かめ、測定後に復元を確かめた（ハーネス: scratchpad の `mutate.mjs`）。ベースライン（`cargo check -p snotra --all-targets`・`cargo clippy -p snotra --all-targets -- -D warnings`・`cargo test -p snotra search_input_ui` / `font_definitions`）はすべて緑。

| 条項 | 変異 | 結果 |
|---|---|---|
| R1 | `commands/system.rs` から `state.config.read()` | CAUGHT E0616 |
| R1 | `commands/system.rs` で `AppState { .. }` リテラル | CAUGHT E0451 |
| R3 | `commands/system.rs` から `engine.config()` | CAUGHT E0599 |
| W1 | `commands/system.rs` から `sh.main_waker.wake()` | CAUGHT E0616 |
| X8 | `on_event_loop` 内で `&EventLoopProof` を `thread::scope` の別スレッドへ | CAUGHT E0277（`*const ()` は Sync でない） |
| M4 | `commands/system.rs` から `&mut material.tree` | CAUGHT E0616 |
| V5 | `search_input_ui` 入口に `ui.ctx().set_visuals(..)` | CAUGHT clippy disallowed |
| V6 | 3 値を `ui.visuals()` の写しへ書く（適用しない） | CAUGHT `search_input_ui_applies_theme_values_…` FAILED |
| F1 | 解決失敗枝の `jp_font` を `push` | CAUGHT `font_definitions_fallback_is_jp_single_stack` FAILED |
| F1 | `user_font` を `push` | CAUGHT 2 本 FAILED |

10/10 が期待どおりの理由で落ちた。M10（子モジュールの可視性）は変異を当てていない——索引の補足であり規則として残す理由が無いので 5 へ寄せる。

## 敵対的調査（3b）の採否

出力: `workspace/adversarial-1336.txt`。

| 所見 | 採否 | 理由・反映 |
|---|---|---|
| B1 参照の母集団にパス無し・散文形が漏れる | 採 | 自分で確かめた形（`main.rs:302`・rules の「該当節」）。「参照」節へ追記 |
| B2 `config_watcher.rs:23` も native_brush_color の偽 | 採 | 実物を読んで確認。偽の一覧 8 |
| B3 I2 の「doc が無い」は偽 | 採 | `commands/window.rs:17` を確認。I2 を 3= へ |
| B4 M3 に「起動経路も」の命令が欠ける | 採 | M3 を 3= / 3+ へ |
| B5 P6/P7 の一部はコード側に無い | 一部採 | `let _ =` の逃げ道は P1 へ 1 文で統合。P7（`double_must_use`）は一度きりで 5 のまま、裁定事項に回す |
| B6 X17 の同形が snotra-core にも | 採（記録のみ） | 他 crate は issue の「やらないこと」で範囲外 |
| B7 X20/X21 の例外側がコードに無い | 採 | X21 を 3+ へ |
| B8 字数はバイト数だった | 採 | `LC_ALL=C.UTF-8 wc -m` と node で 29,849 を確認。PR 本文はこの数え方で書く |
| B9 `main.rs:342,413` は architecture.md へ付け替え | 採 | architecture.md:90 を確認 |
| B10 W1 の射程（handles は pub(crate)） | 採 | W1 に射程の 1 文を足す（3+） |
| B11 R2・S3・V3 に欠けた命題 | 採 | R2・S3 を 3= / 3+、V3 の所在を訂正 |
| B12 M7 の根拠 | 採（根拠差し替え） | 分類 5 は維持 |
| ⚠️ X1 / X2 / I3 は 4 寄りかも | 不採 | X1 は一度きりの事件、X2・I3 は一般作法で src-tauri 固有でない（issue「一度きりの事件から生まれた規則は 5」） |
| ⚠️ I5 private 化は安くないかも | 採らず測った | 書き手 0・読み手 5 箇所を grep 済み（plan.md 未確定欄） |
| 壊せなかった: 争点 3（`###` は「モジュール構成」の内側）・CI clippy・テストの cfg・可視性・I5 の書き手 | — | 「節を消しても basename は `###` より前に全部出ている」ので G-module-index は動かない（3b） |
