# src-tauri

Tauri v2 バイナリ crate。Win32 API 統合とフロントエンドとの IPC を担当。

各ルールは「**太字 = 守る指示**、後続 = 理由・経緯」の形式。迷ったら太字部分に従えば安全。

## モジュール構成

責務を持つ個別モジュールの責務宣言は各ファイルの `//!`（module doc）を正本とする（薄いラッパーを集約記述する `commands/`・`platform/` は責務を本節に直接記す例外）。本節はファイル一覧と、`//!` に収まらない**横断不変条件・チェックリスト**を記す（#562）。

- `main.rs` — エントリポイント・Tauri セットアップ・イベントリスナー登録（責務は `//!`）
- `state.rs` — Tauri managed state `AppState`（責務・構成は `//!`）。以下はビルドフラグの規律:
  - **インデックスビルドの開始/終了は `try_begin_index_build()` / `finish_index_build()` メソッド経由で行う** — `indexing`・`index_build_started` を coherent に更新する
  - **config 変更→index 再構築のコヒーレンシ判断は engine の `index_stale` ledger（軸1）に閉じており、この 2 AtomicBool は二重ビルド防止（CAS）と UI 表示専用に純化されている**（#347/#348-A）
- `icon.rs` — アイコンのオンデマンド抽出とキャッシュ永続化（責務は `//!`）。**`invalidate_icon_cache` はメモリ内 `IconCacheState` と `icons.bin` を単一 lock 内で両方無効化する** — lock 外でファイル削除すると、並行ロード（None 検知 → `icons.bin` 再ロード）が削除直前の旧ファイルをメモリへ戻す TOCTOU が起きる（#522、実測 17/2000 回）。片方だけだと終了時 `save_if_dirty` で古いアイコンが復活する
- `indexing.rs` — バックグラウンドインデックス構築（責務は `//!`）。以下は drain / panic 戦略の不変条件:
  - **`start_index_build` は `mark_index_stale`（CAS の前）→ CAS → spawn の順**で、**drain ループ**（`begin_index_drain` で現在 config の `IndexInputs` snapshot → ロック外で `rebuild_and_save` / `PrebuiltIndex::new` → `complete_index_drain` で swap + re-diff）を stale が消えるまで回す
  - **ビルド本体は `catch_unwind` で包む（panic 戦略依存）**: unwind ビルド=debug/test では panic を捕捉し `finish_index_build` で flag 固着 wedge を防ぐ。release は Cargo.toml で `panic="abort"` のため build panic はプロセス abort＝ここに来ないが silent wedge にもならず、再起動で fresh build される。どちらでも UI 永久構築中は起きない
  - **finish 後に `is_index_stale` を再チェック**し、finish 窓で刺さった変更を再 kick で拾う。**unwind の panic 経路では再 kick しない**（決定論 panic の無限リトライ回避）
  - config 変更→index 再構築のコヒーレンシは engine の `index_stale` ledger に一元化（#347/#348-A）
- `config_watcher.rs` — `config.toml` 監視（100ms debounce）と `apply_config_change()` による反映（責務は `//!`）。以下は適用の不変条件と発火イベント:
  - **不変条件: 言語変更とホットキー変更が同時に発生した場合、`language-changed` イベントをホットキー失敗通知より先に発火する**（フロントエンドが正しい言語でエラー文字列を組み立てるため）
  - **不変条件: `LoadOutcome::ReadFailed`（一時的・環境的な read 失敗）では `apply_config_change` は何も適用せず早期 return する**（`should_apply_config_change()` で判定）。fallback-default を実行中エンジンへ適用すると、live-read 化した履歴剪定が `history.bin` をデータ損失させ、index 再構築判定（`IndexInputs` 差分）が default scan で誤再構築を起こすため。`Config::load` の「一時的失敗は退避も上書きもしない」保全を適用側にも揃える（#348）
    - ただし早期 return の前に短いバウンドリトライ（`load_with_read_failed_retry`、既定 3 回 × 150ms）で一時的ロック解除を待ち、解ければ正規の変更を取りこぼさず適用する（予算超過時のみ skip。リトライ中は適用しないのでデータ損失安全は不変）
  - **不変条件: index 再構築の要否は `IndexInputs::from_config(old) != IndexInputs::from_config(new)` で判定し、ビルド進行中（`indexing`）でも `!indexing` ゲートなしで常に `start_index_build` を kick する**（`start_index_build` が `mark_index_stale` で stale を立て、in-flight ビルドの drain / finish 後再チェックが取りこぼしを拾う。CAS が二重起動を防ぐ。#347/#348-A）
  - 発火するイベント: `language-changed` / `hotkey-registration-failed` / `visual-config-changed` / `show-icons-changed` / `auto-hide-focus-lost-changed` / `max-results-changed` / `instant-prefix-changed` / `top-n-history-changed` / `indexing-started`（indexing.rs から）/ `indexing-complete`（indexing.rs から）/ config-applied（egui wake・値なし・SU6）
- `ime.rs`: IME オフ操作（`ImmSetOpenStatus(false)`）。Win32 IMM API の薄いラッパー
- `trace.rs`: `SNOTRA_TRACE` 環境変数ゲートの構造化トレースログ（`trace_enabled` + `trace`）。`main.rs` の `trace_main` と `commands::trace_command` が薄いラッパーとしてここへ委譲する（#433 で重複を集約）。seq カウンタは単一の `AtomicU64` を共有するため、両者のトレース行は1本の単調増加列で interleave する（旧実装は 2 カウンタに分裂していた。トレースはデバッグ出力専用のため許容される挙動変更）
- `monitor.rs`: マルチモニター対応の Win32 ヘルパー（`GetCursorPos` / `MonitorFromPoint` / `MonitorFromWindow` / `GetMonitorInfoW`）。物理座標ベースで作業領域を取得し、ウィンドウ位置のクランプ・中央配置を提供
- `working_set.rs`: 非表示アイドル時に Win32 `EmptyWorkingSet` でプロセスツリー全体（自プロセス + WebView2 子孫）の物理 working set を回収（Windows のみ、非 Windows は no-op）。`collect_descendant_pids()` は Toolhelp の (pid,ppid) 上を BFS する純関数（Win32 非依存・ユニットテスト対象）。`trim_idle_working_set()` は全 hide 経路から呼ばれる best-effort 操作で、HANDLE は RAII ガードで解放し、全 Win32 失敗を握りつぶす。**WebView2 経路**（hotkey + `notify_main_hidden`）は `suspend_and_trim_after_hide` 経由で suspend とセット、**egui 経路**（`egui_shell::hide_egui_main`）は suspend 対象が無いため trim のみを直呼びする（#532 SU6.5・egui-hidden の PrivWS を可視時同値 43MiB → ~1MiB へ落とす）
- `commands/`: ディレクトリモジュール（`mod.rs` + `search.rs` / `launch.rs` / `config.rs` / `icon.rs` / `window.rs` / `system.rs` / `instant.rs`）。`#[tauri::command]` を責務別に分割。`launch.rs` は `launch_item_core` / `launch_with_tool_core`（いずれも `pub(crate)`、`instant.rs`・`egui_shell/view.rs` から再利用）に加え、トレイメニューからの起動用に `launch_item_with_state` / `launch_with_tool_with_state` / `launch_default_with_state` / `resolve_all_openers` を `pub` で公開
- `platform/`: ディレクトリモジュール（`mod.rs` + `hotkey.rs` / `tray.rs` / `wndproc.rs`）。Win32 メッセージループスレッド + トレイアイコン + ホットキー + ウィンドウプロシージャ
- `egui_shell/`: ディレクトリモジュール（`mod.rs` + `lifecycle.rs` / `search_state.rs` / `layout.rs` / `icon_textures.rs` / `notify.rs` / `strings.rs` / `view.rs`）。WebView2 と並行する egui/softbuffer メインウィンドウの外殻 + 検索体験（#532 SU2/SU3・env フラグ `SNOTRA_EGUI_MAIN`）。`lifecycle.rs` は純粋核（`plan_hotkey` / `blur_should_hide`）、`search_state.rs` は検索状態の純粋核（`SearchState` / `interpret` / `QueryIntent`）、`layout.rs` は高さ算出 + debounce の純粋核（`compute_window_height` / `Debouncer`）、`icon_textures.rs` はアイコン・テクスチャ層の純粋核（PNG→ColorImage decode・抽出要否述語・可視集合 retain。worker spawn / load_texture の driver は `view.rs` が持つ・#532 SU4）、`notify.rs` は通知 primitive の純粋核（一時通知 NoticeSlot + updater toast 状態機械 UpdaterUi・#532 SU5）、`strings.rs` は egui 経路の UI 文言テーブル（i18n.ts と同文言・言語は view.rs lang() の毎フレーム live-read）、`view.rs` は検索 view（TextEdit・結果リスト・キーボードナビ・起動・動的高さ・indexing 案内）、`mod.rs` は窓生成・show/hide・位置永続・reset-on-show・hide/config-wake listener（責務は各 `//!`）
  - **イベント駆動 wake の不変条件（#532 SU5）**: runtime はイベント駆動（`RedrawRequested` 待ち）で通常フレームは勝手に回らない。**フレームの paint より後（遅延 dispatch・クリックハンドラ）や worker スレッドで UI 状態を変えたら、必ず `ctx.request_repaint()` で次フレームを起こす**——欠くと次の無関係な入力まで stale 表示が残る（toast dismiss で実測・PR #647 の e746826 で修正。folder/icon worker の送信毎 repaint と同根）。また **hidden 中は `update()` が走らない**（実測・SU5 要石）——時限処理（timeout・通知期限）の `request_repaint_after` は可視中しか効かず、hide を跨ぐ in-flight 状態は reset-on-show の backstop（クリア）とセットで設計する
  - `wndproc.rs`: `SendMessage` 経由で届く `WM_TRAY_ICON` および `WM_CONTEXTMENU` を `PostThreadMessageW` でスレッドキューに再投入し、メッセージループでの統一処理を保証

## 実装パターン

- ホットキーは `RegisterHotKey` を `platform/` の Win32 メッセージループスレッドで処理し、`AppHandle.emit()` で Tauri イベントとして通知
- 設定画面は `snotra-settings.exe`（egui 別バイナリ）を子プロセスとして起動。`SettingsProcessState` で重複起動を防止し、子プロセス存命中はメインウィンドウの `alwaysOnTop` を一時解除する
- `commands/` は薄いラッパーに保ち、実処理は `snotra-core` に寄せる（KISS）
- `AppState` は `Mutex<Engine>` で検索エンジン・履歴・設定を一括管理。Phase 2.3 以前の 3重ロック（`Mutex<SearchEngine>` / `Mutex<HistoryStore>` / `Mutex<Config>`）は Engine facade に統合済み
- **インデックスビルドのフラグは `AppState` のメソッド経由で更新する**: `try_begin_index_build()`（`index_build_started` を CAS 取得 → `indexing` を立てる）と `finish_index_build()`（両方を戻す）が唯一の正しい経路。`indexing` / `index_build_started` を直接 `store()` しない——外部からの force-reset は走行中ビルドのガードを踏み倒す競合の原因になる。2フラグは別物（`index_build_started` は CAS 専用ガード、`indexing` は first-run 時にビルドスレッド不在でも true になる UI 表示用）
- Managed state として `IconCacheState`（`Mutex<Option<IconCache>>`、初回アイコン要求で遅延初期化）と `SettingsProcessState`（`Mutex<Option<Child>>`、設定プロセスのハンドル管理）を保持
- **`show_main_and_emit` の操作順序制約**: 高さリセット（52px）→ `position_on_target_monitor` → `show()` の順。位置計算はウィンドウサイズ（`outer_size()`）でクランプするため、高さリセット前に位置を決めると展開時の高さでクランプされ、折りたたみ時に位置がずれる

## IPC コマンドの返り値契約

新規 `#[tauri::command]` は以下の3系統のいずれかに従う（#434: as-built で4系統に分裂していたものを整理した規約）。

1. **読み取り・検索系**（失敗を結果 DTO で表現するもの）: 素の `T` を返す。エラーは DTO 内の `is_error` フラグ + UI 層で表示文字列を決定する（`snotra-core` の設計と整合）。例: `search` / `get_history_results` / `list_folder`
2. **起動系**: `LaunchResult { status, code, message }` 契約（`launch_item` / `launch_with_tool` / `execute_instant_command`）
3. **失敗しうる操作系**: `Result<T, String>`。「実行できない状態」（インデックス構築中など）も `Err(定数)` で表現する。例: `open_settings` / `rebuild_index`

`bool` 返しは新規コマンドで使用しない（「成功/失敗」と「実行できない状態」を混同しやすく、フロントエンドが呼び出しごとに異なる判定を実装する原因になる）。

補足:

- `notify_main_shown` / `notify_main_hidden` の命名は実態（`AppState` のフラグ更新が主目的）とややズレるが、IPC 改名はフロントエンド・トレースログ双方に波及する churn の割に得られる整合性が小さいため改名しない
- 「インデックス構築中で実行できない」という同一条件を表すエラー定数は、`open_settings`（`commands/window.rs` の `ERR_INDEXING_IN_PROGRESS`）と `rebuild_index`（同定数を再利用）で共有する。新たに「実行できない状態」を追加する場合もこの定数を再利用するか、命名パターン（`ERR_<状態>`）を揃える
- 既存コマンドの一斉移行はしない。上記規約は「新規コマンド」と、契約と実装が乖離していた `list_folder`（型が `Result` だが `Err` を返さない死蔵の型）・`rebuild_index`（`bool` と `open_settings` の `Result` で同一条件が不一致）の是正に適用する

## Win32 メッセージ配送の注意

Shell のトレイコールバック (`uCallbackMessage`) は `SendMessage` で配送される場合があり、`GetMessageW` ループに到達しない。カスタムメッセージ (`WM_APP + N`) をウィンドウプロシージャ (`DefWindowProcW`) だけで処理すると消滅するため、`platform_default_wnd_proc` で検出して `PostThreadMessageW` でスレッドキューに再投入する設計にしている。

NOTIFYICON_VERSION_4 では、キーボード操作（Shift+F10 / Application キー）によるコンテキストメニュー要求は `uCallbackMessage` を経由せずウィンドウプロシージャに直接 `WM_CONTEXTMENU` として届く。`platform_default_wnd_proc` で同様に再投入することで `handle_tray_message` に統一している。

**Win32 メッセージハンドラを削除・変更する前に「そのメッセージが届く全経路」を列挙すること。** 同一メッセージでも発火源が複数ある場合がある（例: `WM_CONTEXTMENU` はマウス右クリック環境とキーボード操作の両経路で届く）。「問題の原因になっている経路」だけを削除しようとすると、問題でない別の経路も同時に消える。

## WebView2 ウィンドウ生成の制約

WebViewを持たないTauriバイナリは、このcrateの別binとして追加しない。`build.rs`が既定の`tauri.conf.json`をコンパイル時に取り込むため、実行時に別configを渡しても既定WebViewが生成される。独立crateに置き、実行時の子プロセスツリーで`msedgewebview2.exe`が0件であることを確認する。

`WebviewWindowBuilder::build()` は WebView2 初期化のために Win32 メッセージポンプの進行を必要とする。**「メインスレッドにいる」と「メッセージポンプが自由に回る」は別物**であり、以下の制約がある:

- **setup フェーズ（イベントループ開始前）**: `build()` が自前でメッセージを処理できるため正常動作する
- **イベントループ中のコールバック（`run_on_main_thread` / `listen` / `RunEvent` 等）**: メッセージポンプが1イテレーション内で停止しているため、`build()` がポンプ進行を待ってデッドロックする
- **IPC ハンドラスレッド**: メインスレッドではないため同様にデッドロックする

このため、ウィンドウの生成は必ず setup フェーズで行い、ランタイムでは show/hide のみで制御する。メインウィンドウは `decorations: false` で閉じるボタンを持たないため `CloseRequested` ハンドラは不要。非表示化はフロントエンド側の `win.hide()`（フォーカス喪失時・Escape 時等）で行う。

**事前チェック**: ある操作が「内部でメッセージポンプの進行を必要とするか」を確認する。ウィンドウ生成・COM STA 初期化・モーダルダイアログ等は該当し、イベントループコールバック内から呼べない。

## WebView2 TrySuspend / Resume パターン

非表示中に WebView2 レンダラーを中断してメモリ・CPU を削減する。`ICoreWebView2_3::TrySuspend` / `Resume`（Edge 88+）を使用。

- **`TrySuspend` の前提は `ICoreWebView2Controller.IsVisible=false` であり、HWND の非表示とは独立**（2026-07-17 実測）。wry は `win.hide()` で controller 側を下げないため、`suspend_webview` が `SetIsVisible(false)` を自前で実行してから `TrySuspend` を呼ぶ。これを欠くと `TrySuspend` は**同期 Err（0x8007139F ERROR_INVALID_STATE）で失敗し、完了ハンドラは呼ばれず沈黙する**——導入以来この失敗が全 hide で起きており、suspend は一度も成立していなかった。`SNOTRA_TRACE=1` で `suspend:call_returned`（同期戻り値）/ `suspend:completed`（成否）を観測できる
- **hide 後の後処理は `suspend_and_trim_after_hide(app, source)`（`main.rs`）に一本化**: hotkey トグル（`w.hide()` → `emit("window-hidden")` → ヘルパー）とフロントエンド起因（Escape / クリック起動 / フォーカス喪失 / `/s`: `notify_main_hidden` が `emit` 後にヘルパー）の全 hide 経路が共有する。ヘルパーは `run_on_main_thread` で suspend → trim をメインスレッドのイベントループへ積む——`with_webview` / `run_on_main_thread` のクロージャは**同一キューで FIFO 直列化**されるため、後続 show の resume に追い越されない。emit を suspend より先に送ることで、JS 側のクリーンアップ（Blob URL 解放等）がレンダラー中断前にキューイングされる。ウィンドウ hide が先なのは UX 上の要請（中断は不可視状態でのみ行う）。フロント経路は `await win.hide()` 完了後に IPC を呼ぶ（#361）
- **再表示と競合した suspend は 2 段で無害化する**: 旧実装で「競合時は黙って失敗」を担っていたのは TrySuspend 自身の前提条件チェック（IsVisible=true → 同期 Err）だが、IsVisible を自前で下げる現実装ではその検査は機構として働かない。代わりに (1) suspend クロージャ内（メインスレッド実行時点）の `main_visible` ガードが「show 完了後に実行される」ケースを放棄し、(2) `show_main_and_emit` が show 完了後にもう一度 `resume_webview` を積んで「冒頭 resume の後〜可視フラグ反映前に滑り込む」残余窓を是正する。ガードは**必ずクロージャ内で読む**（外で読むとディスパッチ待ち中に show が完了する TOCTOU）
- **show 時**: `resume_webview(&main)`（`SetIsVisible(true)` + `Resume`。suspend 側が下げた controller 可視フラグと対称）→ `set_size` → `show`（+ 末尾に resume 再適用）→ `emit`。Resume は同期 API で即座に復帰（実測: show p50 33→37ms 帯で劣化なし）
- **`with_webview()` は呼び出しスレッドによらずクロージャがメインスレッドで実行される**: setup フェーズでは同期的に完了するが、それ以外（IPC ハンドラ / `std::thread::spawn` / イベント listener）では非同期ディスパッチとして扱うこと。順序が要る箇所はクロージャの FIFO 直列化（同一キュー）に依拠する
- **TrySuspend と MemoryUsageTargetLevel は混用禁止**: TrySuspend が自動で MemoryUsageTargetLevel を Low に設定し、Resume が Normal に戻す
- **`SNOTRA_DISABLE_SUSPEND=1` で suspend を無効化できる（E2E 専用エスケープハッチ）**: WebDriver は非表示中のレンダラーへの `executeScript` で可視性判定・入力を行うため、suspend されたレンダラーとは原理的に非互換（script が応答せずタイムアウト）。E2E ハーネス（`e2e/tauri.slash.e2e.ts` の `spawnTauriDriver`）がこの変数を立てて起動する。`EmptyWorkingSet` trim は無効化されない
- **WebView2 150+ の High IL E2E はアプリ API から remote debugging を有効化する**: WebView2 150 は elevated host でユーザー書き換え可能な `WEBVIEW2_*` 環境変数と HKCU policy のブラウザ引数を無視するため、msedgedriver が通常使う経路では `DevToolsActivePort` が生成されない。`e2e-webview-automation` Cargo feature を有効にしたテスト専用バイナリだけが、`SNOTRA_E2E_WEBVIEW_DATA_DIR`（単一の相対ディレクトリ名）がある起動で `CoreWebView2EnvironmentOptions.AdditionalBrowserArguments` に `--remote-debugging-port=0` を直接設定する。通常ビルドには feature が無く、feature 付きでも環境変数のない startup smoke は現行設定のまま。E2E ハーネスは同じディレクトリを `webviewOptions.userDataFolder` に渡し、app/driver 終了後に削除する

## WebView2 working set の能動回収（EmptyWorkingSet）

`working_set::trim_idle_working_set()` が hide 経路で Win32 `EmptyWorkingSet` をプロセスツリー全体（自プロセス + WebView2 子孫）へ能動適用し、hide 直後の物理 RSS を即時に落とす（再表示は OS の透過 re-fault で ~44ms 維持、UI 正常）。

- **TrySuspend とは別レイヤーで補完的**: EmptyWorkingSet=物理 working set の**即時トリミング**、TrySuspend=レンダラー停止によるアイドル中の**再増殖防止**（+ CPU 静止）。suspend が成立しないと trim 後もレンダラーがページを touch し続け、アイドル 30 秒でツリー WS が ~70-86MB へ戻る（2026-07-17 実測。suspend 成立後は 12-31MB で低空安定）。なお旧記述「TrySuspend は物理 WS を回収しない（~110MB 不変）」は、TrySuspend が実は同期失敗していた期間の測定に基づく（→「TrySuspend / Resume パターン」）
- **全 hide 経路に適用**: hotkey トグル（`main.rs`、`suspend_webview` の後）と `notify_main_hidden`（`commands/system.rs`、全フロントエンド hide の IPC チョークポイント）の両方から呼ぶ。**`EmptyWorkingSet` はスレッド非依存**（`with_webview` のような非同期制約がない）ため、tokio IPC スレッドの `notify_main_hidden` からも安全
- **show 側に逆操作は不要**: trim されたページは show 時に OS が透過的に re-fault する。明示 untrim API は存在しない。trim が hide 前後どちらで走っても無害（再 fault するだけ）
- **best-effort・物理 RAM のみ**: Toolhelp / `OpenProcess` / `EmptyWorkingSet` の全失敗は黙ってスキップ（機能影響ゼロ）。HANDLE は RAII ガードで解放。削減対象は working set であって commit ではない

## Win32 / Tauri 注意事項

- Win32 関連の不具合では、まず `config.toml`（テーマ含む）を確認し、次にウィンドウライフサイクル順序、最後に API 呼び出しを調査する（白画面バグの真因がテーマ設定だった事例あり）
- Rust クレートをバージョン昇格する際は、対象バージョンが crates.io に実在・正当であることを確認する。大版ジャンプを前提にしない（例: `bincode 3.0.0` は `compile_error!` のみを含むジョークパッケージでコンパイル不能）
- `windows` クレート（現在 v0.62）はバージョンごとに API シグネチャが変わる（`Result` 型の有無、ハンドル型の変更など）。コードを書く前に、使用中のバージョンで対象 API が利用可能か・型が一致するかを確認する
- `webview2-com` は `windows-core 0.61` に依存するが、プロジェクトの `windows` クレート（v0.62）は `windows-core 0.62` を使う。`Interface::cast()` 等を呼ぶ際は `windows-core_0_61 = { package = "windows-core", version = "0.61" }` のエイリアス依存を使い、`use windows_core_0_61::Interface` とする
- WebView2 COM インターフェース（`ICoreWebView2_3` 等）は `!Send + !Sync`（内部が `NonNull<c_void>`）。Tauri managed state（`Send + Sync` 必須）に保持できない。`with_webview()` コールバック内でインラインにキャストして使う
- 必要な feature フラグ（`Win32_UI_WindowsAndMessaging` 等）が `Cargo.toml` に宣言されているか確認してから実装する
- `UpdateWindow` など一部 API は windows クレートのバージョンによっては未提供。代替 API（`RedrawWindow` 等）の存在を事前に調べる
- Windows パスの正規化では `C:` と `C:\` の違いに注意する（ドライブルートは末尾 `\` が必須）
- ファイルメタデータ取得時、シンボリックリンクを考慮する場合は `symlink_metadata` を使う（`metadata()` はリンク先を辿る）
- **Win32 の「サイズ取得 → バッファ充填」2回呼び出しパターン**（`ExpandEnvironmentStringsW` 等）では、2回目の戻り値（書込長）を必ず**バッファ長で clamp してからスライス**する（`written.min(buf.len())`）。値が2呼び出し間で伸びると戻り値 > バッファ長になり `buf[..written-1]` が境界外 panic、release は `Cargo.toml` で `panic="abort"` のためプロセス abort に化ける（#394）
- Tauri プラグインの新機能を使う際は `capabilities/*.json` の権限宣言を確認する
- `tauri.conf.json` の CSP で特定ディレクティブ（`connect-src` 等）を明示すると、そのディレクティブは `default-src` を継承しなくなる。`'self'` が必要な場合は明示的に含めること。また `tauri dev` では CSP が適用されないため、CSP 起因の問題はリリースビルドでしか再現しない
- `ShellExecuteW` でフォルダ・画像・文書ファイルを開く場合は COM STA が必要。Tauri コマンドハンドラスレッドは COM 状態が保証されないため、`std::thread::spawn` + `CoInitializeEx(None, COINIT_APARTMENTTHREADED)` + `ShellExecuteW` + `if com_ok { CoUninitialize() }` パターンで新規スレッドに COM 環境を用意する。`is_ok()` は S_OK(0) と S_FALSE(1) を両方 true とし、どちらも CoUninitialize が必要。EXE ファイルは COM 不要なため同問題を起こさない
- `with_webview()` → `PlatformWebview::controller()` で `ICoreWebView2Controller` にアクセスし、WebView2 COM API を直接呼べる。**setup フェーズでのみ安全**（イベントループ中はデッドロック）。`webview2_com` クレートが必要。現在 `AcceleratorKeyPressed` ハンドラを登録し、`WM_SYSKEYDOWN`（Alt+char）を `SetHandled(true)` で消費してビープ音を防止している
- `SendInput` はシステム入力キューに注入し、ルーティングはキュー取り出し時に決定される。**フォーカス移行直後の `SendInput` は対象ウィンドウに届かない場合がある**（`SetForegroundWindow` は部分的に非同期）。`SendMessageTimeoutW(hwnd, WM_NULL, ..., SMTO_NORMAL, 100, ...)` でフォーカス完了を同期待ちしてから `SendInput` を呼ぶ（Raymond Chen 推奨パターン）
- JS の `preventDefault()` は Chromium レンダラプロセスの IPC 経由で動作するため、**ネイティブ HWND レベルの `DefWindowProc` 呼び出し（`MessageBeep` 等）を阻止できない**。ネイティブ側で阻止する必要がある場合は `AcceleratorKeyPressed` や HWND サブクラスを使う
