# src-tauri

Tauri v2 バイナリ crate。検索 UI（`egui_shell/`・egui + softbuffer）と Win32 API 統合を担当する。本書に置くのは、テストにも型にもコードの doc にも載らない規則だけである——個々の不変条件は各ファイルの `//!` と、その関数・型の doc が正本。

## モジュール構成

各ファイルの責務は `//!`（module doc）が正本。本節はファイルの索引である（#562）。

- `main.rs` — エントリポイント・Tauri セットアップ・イベントリスナー登録。業務ロジックを増やさない
- `state.rs` — Tauri managed state `AppState`
- `icon.rs` — アイコンのオンデマンド抽出とキャッシュ永続化
- `indexing.rs` — バックグラウンドインデックス構築
- `config_watcher.rs` — `config.toml` 監視と `apply_config_change()` による反映
- `events.rs` — アプリ内 Tauri イベント名の定数
- `ime.rs` — IME をオフにする Win32 IMM API の薄いラッパー
- `trace.rs` — `SNOTRA_TRACE` 環境変数ゲートの構造化トレースログ
- `heap_trace.rs` — Rust ヒープ常駐の計装（`heap-trace` feature でのみ有効）
- `startup.rs` — 起動の端から端まで（プロセス作成 → ホットキー登録完了）を刻む計測コード
- `monitor.rs` — マルチモニター対応の Win32 ヘルパー
- `working_set.rs` — 非表示アイドル時のプロセスツリー working set 回収
- `commands/` — egui view・トレイが共有する core 関数群（`mod.rs` + `launch.rs` / `icon.rs` / `window.rs` / `system.rs` / `instant.rs`）
- `platform/` — Win32 メッセージループスレッドとトレイ・ホットキー・ウィンドウプロシージャ（`mod.rs` + `hotkey.rs` / `tray.rs` / `wndproc.rs`）
- `egui_shell/` — 製品メインウィンドウ（egui/softbuffer）の外枠と検索体験
  - `mod.rs` — ウィンドウ生成（main/results）・共有状態・config の 1 フレーム読み・listener 登録
  - `view.rs` — main ウィンドウの 1 フレーム
  - `results_view.rs` — 結果リストウィンドウの従属 view
  - `results_window.rs` — results ウィンドウの所有型
  - `window_coordinator.rs` — ウィンドウの可視性・位置・サイズ・wake を駆動する
  - `visual.rs` — テーマの 1 フレーム分の読み取り値と純粋な導出
  - `font_stack.rs` — フォント解決と `set_fonts` 登録
  - `layout.rs` — 高さ算出・results 可視性の導出・幾何・debounce の純粋核
  - `lifecycle.rs` — hotkey と blur の純粋核
  - `search_state.rs` — 検索状態の純粋核
  - `search_dispatch.rs` — 検索 dispatch の同一性の純粋核
  - `search_worker.rs` — 検索を実行する単一 worker
  - `icon_textures.rs` — アイコン・テクスチャ層の純粋核
  - `notify.rs` — 通知 primitive の純粋核
  - `strings.rs` — UI 文言テーブル
  - `launcher_controller.rs` — 検索セッション層の所有者。遷移は責務ごとの子モジュールが持つ
    - `launcher_controller/activation.rs` — 起動の入口と dispatch
    - `launcher_controller/search_flow.rs` — 検索の発行と採り込み、打鍵の処置
    - `launcher_controller/folder_nav.rs` — folder の突入/深掘り/折り返しと列挙のロード
    - `launcher_controller/hide_request.rs` — hide 要求の経路
    - `launcher_controller/frame_stages.rs` — フレーム毎の消費と回収
    - `launcher_controller/updater_toast.rs` — 更新 toast のボタン処置と install の spawn
    - `launcher_controller/activation/tests.rs` — `launcher_controller/` 直下の子を読むソーステキスト検査

## プラットフォーム固有の設定

- `tauri.conf.json` や platform 固有ファイルへ設定を足すときは、Windows で対応しているかを先に確かめる。非対応の設定はビルドエラーになることがある

## 状態の二軸導出（`search_state.rs`）

モードは `src-tauri/src/egui_shell/search_state.rs` の `ViewKind`（軸1: ビュースタック頂点）と `QueryIntent`（軸2: 入力の意味）の二軸で導く。原則は `docs/development-principles.md`「状態を散在フラグで表さない（単一の導出源に集約する）」にある。

- **オーバーレイ状態を軸に混ぜない。** 相互排他なモードは discriminated kind にし、どのモードにも重なりうる状態は独立フラグのまま分けておく
- **仕様との一対一対応を保つ。** 仕様側に優先順位・状態遷移が記されているなら、実装の導出源がそれと対応するようにする（`SPEC.md` §8.6 状態図・§18.5 優先度 ↔ `view_kind()` の射影順）。整合検証は `/state-check`

## 処置を返す純粋核の強制（#934）

**`&mut self` で状態を進めてから処置を返す関数は `#[must_use = "…"]` を持つ。** 返り値を落とすと副作用（hide 要求・repaint 予約・検索の発火・ビュー復元）が黙って消え、状態は既に進んでいるので呼び直しても戻らない。メッセージには、落としたときに失うものを名指す。

- **配置**: 型を本 crate が所有するなら型段（`enum` 宣言。その型を返す将来の関数も覆う）、`bool` / `Option<T>` のような primitive・外来型を返すならメソッド段（`Option` は型に `#[must_use]` を持たない）
- **参照（`&T`）・`impl Trait`・ジェネリック型に包んで返す関数を足すとき、および既存の関数の返り値型をそう変えるときは、型段に加えてメソッド段も付ける。** 型段の属性は、そう包まれた返り値では発火しない（例外は `Box<T>` / tuple / 配列だけ）。見落としやすいのは返り値型を変えたときである——シグネチャを編集した瞬間に、型段のガードだけが黙って消える
- **対象外は 2 種**: (1) トークンを返すもの——落としても、トークンを消費する呼び出しが無いことが同じ数行の中で目に見える。付けてもテストに `let _ =` を要求するだけになる。(2) 状態を進めずに導くもの（自由関数と `&self` メソッド）——同じ入力で呼び直せるので失うものが無い
- **型段へ移すときは、古いメソッド段を消したことを目で確かめる。** `clippy::double_must_use` が検出するのはメッセージ無しの残存だけで、メッセージ付きの残存は沈黙する
- **`#[must_use]` が検出するのは「一度も見なかった」だけである**——`let _ =` も、`if ….is_some() { }` のような「見たが捨てた」形も通る

## イベント駆動 wake の不変条件（#532 SU5） — フレームの paint より後や worker で UI 状態を変えたら `ctx.request_repaint()` を撃つ

runtime はイベント駆動（`RedrawRequested` 待ち）であり、誰かが起こさない限り次のフレームは来ない。**フレームの paint より後（遅延 dispatch・クリックハンドラ）や worker スレッドで UI 状態を変えたら、必ず `ctx.request_repaint()` で次フレームを起こす**——欠くと次の無関係な入力まで stale 表示が残る（toast dismiss で実測・PR #647）。別ウィンドウや外部スレッドからは `wake_main` / `wake_results` を通す。

- **hidden 中は `update()` が走らない**（機序は `wake_main` の doc）。時限処理の `request_repaint_after` は可視中しか効かないので、hide を跨ぐ in-flight 状態は reset-on-show の backstop（クリア）とセットで設計する
- **期限を待つ状態（armed）は、条件が成立するか解除されるまで毎フレーム残余を再要求する**（#711）——`request_repaint_after(d)` はフレームの到来を約束しない（`snotra-egui-runtime/CLAUDE.md`「不変条件」）。1 回きりの予約に頼ると、hide や再検索が次の無関係な入力まで止まったままになる
- **再要求してよいのは「時間経過で解消する不成立」だけである**——フォーカス・設定値・他プロセスの生死のように時計と無関係な条件で再要求すると `request_repaint_after(ZERO)` の永久スピンになる。それらの変化は変えた側が wake する

## main ウィンドウへ egui コンテナを足すときは visuals を自分で渡す

**main ウィンドウへ新しく egui コンテナ（`Area` / `Window` / `CentralPanel` / popup / tooltip）を足すなら、その Ui へ自分で visuals を渡す。** テーマの 3 値（`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）は global style に置いておらず、`search_input_ui` の入口で `ui.visuals_mut()` へ適用している（#949・理由は同関数の doc）。`ctx.global_style()` から Ui を作るコンテナは既定色で描かれる。

## `app.listen` のコールバックは emit 元のスレッドで同期実行される

tauri の `emit_filter` は listener を別スレッドへ dispatch せず直接呼ぶ（2.11.4 で実測、2.12.1 のソースで再確認）。ゆえに listener の中身は「emit 元のスレッドで走るコード」である——Win32 メッセージループスレッド（hotkey）・config 監視スレッド・index build スレッドが、そのまま managed state やウィンドウ API を触る。**listener を足すことは worker を足すことと同じ**であり、並行境界として扱う（→ `/race-check`）。

## trace の presence 検査は状態の検査ではない（#671 PR A′）

「操作を要求した」ログは、その操作が効いたことを意味しない。`egui_results:hide` は出るのにウィンドウは残る、という回帰を `smoke:egui` が緑のまま通した。trace で不変条件を守るなら「何が起きたか」ではなく**「起きてはならないことが起きていないか」**（区間内に事象が現れないこと等）を書く。

## ウィンドウ生成の制約

**ウィンドウの生成は setup フェーズで行い、ランタイムでは show/hide だけで制御する**（main・results とも `egui_shell::create`）。イベントループ中のコールバック（`run_on_main_thread` / `listen` / `RunEvent` 等）ではメッセージポンプが 1 イテレーション内で停止しており、ポンプ進行を要する操作（ウィンドウ生成・COM STA 初期化・モーダルダイアログ等）はデッドロックする。「メインスレッドにいる」ことと「メッセージポンプが自由に回る」ことは別物である（旧 WebView2 期に実測）。

**setup フック自身もイベントループの中で走る**（tauri `src/app.rs` の `make_run_event_loop_callback` が `RuntimeRunEvent::Ready` の arm で setup を呼ぶ・#671 PR D で確認、2.12.1 でも同じ）。「setup はイベントループより前」ではない。ゆえに setup 内で「ポンプが回ること」を期待する操作（`run_on_main_thread` の完了待ち等）を足してはならない。なお setup の実行中は egui フレームが 1 枚も走らない。その帰結（managed state を載せる順序）は `main.rs` の setup のコメントが持つ。
