# plan.md — issue #576: 設定の反映に再起動がいる

## 変更ファイル一覧

### Phase 1: Rust バックエンド — `hotkey_toggle` / `ime_off_on_show` を都度読みへ

1. **`src-tauri/src/main.rs`**
   - **`ime_off_on_show` は `show_main_and_emit` 内部に一本化する**（plan-review Step 2b の独立導出で指摘された改善——呼び出し元が4箇所ある `ime_control: bool` 引数を都度計算して渡すより、消費点である `show_main_and_emit` 自身が読む方が呼び出し元を単純化できる）:
     - `show_main_and_emit(app_handle: &AppHandle, ime_control: bool)`（475行目）から `ime_control: bool` 引数を削除し `show_main_and_emit(app_handle: &AppHandle)` にする
     - 関数内 508〜510行目 `if ime_control { apply_ime_control(...) }` の直前に都度読みを挿入:
       ```rust
       let ime_control = app_handle
           .try_state::<AppState>()
           .map(|s| s.engine.lock().unwrap().config().general.ime_off_on_show)
           .unwrap_or(false); // config.rs の既定値と一致
       if ime_control {
           apply_ime_control(app_handle, &main, t0);
       }
       ```
     - 呼び出し元4箇所を引数なしに変更する:
       - `603` 行目（single-instance プラグインのクロージャ）: `show_main_and_emit(app, ime_off_for_si)` → `show_main_and_emit(app)`。`let ime_off_for_si = ime_off;`（596行目）の move capture は削除
       - `824` 行目（alt release 待ち後の spawn 内）: `show_main_and_emit(&handle_for_show, ime_control)` → `show_main_and_emit(&handle_for_show)`
       - `828` 行目: `show_main_and_emit(&handle_for_hotkey, ime_control)` → `show_main_and_emit(&handle_for_hotkey)`
       - `942` 行目（`setup_startup_display` 内）: `show_main_and_emit(app_handle, ime_off)` → `show_main_and_emit(app_handle)`
     - `setup_hotkey_listener` のクロージャの `let ime_control = ime_off;` move capture（780行目）を削除し、関数シグネチャから `ime_off: bool` 引数を削除する
     - `setup_startup_display(app_handle: &AppHandle, show_on_startup: bool, ime_off: bool)`（940行目）から `ime_off: bool` 引数を削除する（内部で使うのは `show_main_and_emit` への引き渡しのみだったため丸ごと不要になる）
     - `658` 行目 `setup_hotkey_listener(&app_handle, hotkey_toggle, ime_off)` と `681` 行目 `setup_startup_display(&app_handle, show_on_startup, ime_off)` の呼び出しを引数なし版に合わせて更新する
     - `563` 行目 `let ime_off = config.general.ime_off_on_show;` はどの呼び出し元にも渡さなくなるため削除する

   - **`hotkey_toggle` は唯一の消費点（`hotkey-pressed` リスナー、801行目）でインライン都度読みにする**（消費点が1箇所のみのため、`ime_off_on_show` のような関数内一本化ではなく `follow_cursor_monitor`（`main.rs:335-339`）と同じ「使う場所で直接読む」スタイルを踏襲——新しい named helper 関数は追加しない。YAGNI: 1箇所しか使わない読み取りのために関数を切り出す必要はない）:
     ```rust
     // 801行目 `if visible && toggle {` の直前に挿入
     let toggle = handle_for_hotkey
         .try_state::<AppState>()
         .map(|s| s.engine.lock().unwrap().config().general.hotkey_toggle)
         .unwrap_or(true); // config.rs の既定値と一致
     if visible && toggle {
     ```
     クロージャの `let toggle = hotkey_toggle;` move capture（779行目）を削除する
   - `setup_hotkey_listener` のシグネチャから `hotkey_toggle: bool` 引数を削除する（`ime_off: bool` と合わせて `fn setup_hotkey_listener(app_handle: &AppHandle)` になる）
   - `658` 行目の呼び出しを `setup_hotkey_listener(&app_handle);` に変更する
   - `564` 行目 `let hotkey_toggle = config.general.hotkey_toggle;` は呼び出し元が無くなるため削除する

   **不変条件**: `setup_hotkey_listener` 内のリスナー登録 → `RegisterInitialHotkey` の順序制約（`src-tauri/CLAUDE.md`「有効化 ≥ リスナー登録」）は変更しない。都度読みは `AppState` 未登録時（理論上到達しない、`try_state` は setup 後は常に `Some`）に既定値へ安全にフォールバックする——`follow_cursor_monitor` と同じ防御的スタイル。

### Phase 2: Rust バックエンド — `auto_hide_on_focus_lost` の変更検知 + イベント発火

2. **`src-tauri/src/config_watcher.rs`**
   - `apply_config_change()` の diff 検出ブロック（107〜122行目付近）に追加:
     ```rust
     let auto_hide_focus_lost_changed = new_config.general.auto_hide_on_focus_lost
         != old_config.general.auto_hide_on_focus_lost;
     let new_auto_hide_focus_lost = new_config.general.auto_hide_on_focus_lost;
     ```
   - `show_icons_changed` の emit（197〜199行目）と `visible_rows_changed` の emit（202〜204行目）の**間**に追加する（発火順序をコード上の diff 検出順と一致させ、`src-tauri/CLAUDE.md` のイベント一覧の記載順ともずれないようにする——plan-review Step 2 で「一覧への追加位置を明示すべき」との指摘を受けての明確化）:
     ```rust
     if auto_hide_focus_lost_changed {
         let _ = app.emit("auto-hide-focus-lost-changed", new_auto_hide_focus_lost);
     }
     ```
   - 既存の不変条件（`language-changed` をホットキー失敗通知より先に発火・`ReadFailed` 早期 return・`index_changed` の `!indexing` ゲートなし）とは独立した位置に追加するため、順序制約に抵触しない

### Phase 3: フロントエンド — 常時リスニング + シグナルゲート方式

3. **`ui/src/MainApp.tsx`**

   plan-review で当初案（`register`/`unregister` の動的付け外し）に **TOCTOU レース**が見つかった: `auto-hide-focus-lost-changed(true)` を2回連続で受けたとき、`void registerAutoHideOnFocusLost()` が `await win.onFocusChanged(...)` で中断している間（`unlistenAutoHide` がまだ `null`）に2回目が同じガードを通過し、`onFocusChanged` リスナーが二重登録され、1回目の unlisten がリークする。

   plan-review Step 2b の独立導出が提案した**常時リスニング + シグナルゲート**方式に変更する。これは既存の `show-icons-changed` → `setShowIcons` と同型の「listen → シグナル更新」パターン（`MainApp.tsx:172-175`）に揃えるものであり、register/unregister という非対称な状態機械そのものを無くすためレースが構造的に発生しない（ガード付き `if (unlistenAutoHide) return` のような対策コードは不要になる）。

   - `mainVisible` 等と同じ並びにシグナルを追加: `const [autoHideOnFocusLost, setAutoHideOnFocusLost] = createSignal(false);`（既定 `false`——bootstrap 到着前は現行同様どのフォーカス喪失でも非表示にしない挙動を保つ。config.rs の既定値は `true` だが、bootstrap 到着前の窓は現行コードでも「リスナー未登録＝非表示にならない」なので、シグナル既定値もそれに揃える）
   - `registerAutoHideOnFocusLost`（現行 62〜81行目）を廃止し、`onFocusChanged` リスナーを他の「常時登録」リスナー（`onResized`/`onMoved` 等）と同じ並びで一度だけ登録する。**登録失敗を局所 try/catch で隔離する**（Codex Critical 1 対応）——現行は `auto_hide=false` のユーザーには onFocusChanged を一切登録しない（215〜217行の条件登録）ため、常時登録化すると登録失敗の窓を全ユーザーへ広げる。try/catch で囲えば、`await win.onFocusChanged(...)` が reject しても onMount の後続（bootstrap 適用・他イベント購読）を巻き添えにしない。register/unregister 方式へ戻す（= TOCTOU レース再来）のではなく、常時登録のまま失敗だけ隔離する:
     ```tsx
     try {
       const unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
         if (!focused) {
           if (!autoHideOnFocusLost()) return; // 設定オフなら何もしない
           blurTimer.arm(() => {
             void (async () => {
               try {
                 await hideMain();
               } catch (e) {
                 console.warn("auto-hide focus check failed:", e);
               }
             })();
           });
         } else {
           blurTimer.cancel();
         }
       });
       unlistenFns.push(unlistenFocus); // 他の常時リスナーと同じく unlistenFns 管理でよい（対称性の特別扱いが不要になった）
     } catch (e) {
       console.warn("auto-hide focus listener registration failed:", e);
     }
     ```
   - config 変更イベントを購読してシグナルを更新する（既存の `show-icons-changed` 等と同じ並びに追加）:
     ```tsx
     const unlistenAutoHideConfig = await listen<boolean>("auto-hide-focus-lost-changed", (event) => {
       setAutoHideOnFocusLost(event.payload);
     });
     unlistenFns.push(unlistenAutoHideConfig);
     ```
   - **設定オフへ切り替わった瞬間に保留中の非表示タイマーを止める**ため、`createEffect` を追加する（signal 変化を監視する既存の `createEffect` パターンに揃える）:
     ```tsx
     createEffect(() => {
       if (!autoHideOnFocusLost()) blurTimer.cancel();
     });
     ```
     これにより「フォーカス喪失 → `blurTimer.arm` 直後に設定がオフになる」という 100ms 未満の狭い窓でも、pending タイマーが即座にキャンセルされる（当初案の `unregisterAutoHideOnFocusLost` が担っていた「OFF 直後に保留中の非表示アクションを発火させない」不変条件を、レースのない形で再現する）
   - bootstrap 到着処理（215〜217行目）を書き換える: `if (bootstrap?.general.auto_hide_on_focus_lost) { await registerAutoHideOnFocusLost(); }` を `setAutoHideOnFocusLost(bootstrap?.general.auto_hide_on_focus_lost ?? false);` に置き換える（他の bootstrap 由来シグナル `setMaxResults`/`setShowIcons` 等と同じ代入スタイル）
     - **bootstrap 値がより新しいイベント値を上書きしうるレース（Codex Critical 2）は、本 PR ではガードを足さず既存パターンに整合させる**——同一構造のレースが `show_icons`/`max_results`/`instant_prefix`/`result_limit`/`language`/`visual` の 6 設定に既に存在し（早期購読 → 後から bootstrap 無条件代入）、`auto_hide` だけガードすると兄弟設定と非対称になる。発火は「起動の数百 ms 内に外部から config.toml が書き換わる」極稀条件で、次の変更で自己修復する。**全設定横断の本格対応は #578 に切り出した**（購読前 bootstrap 適用への順序入れ替え等）。#576 はこの残余を意図的に受容する
   - `onCleanup` は変更不要（`unlistenFocus`/`unlistenAutoHideConfig` はどちらも通常の `unlistenFns` ループで解除される。`unregisterAutoHideOnFocusLost` という専用関数は無くなったため呼び出しも不要）

   **不変条件（新設計）**: `onFocusChanged` リスナーは登録から破棄まで**常時1本**（register/unregister の対称性ではなく「シグナルが唯一のゲート」という単純な不変条件に置き換わる）。`blurTimer` の pending 状態は「設定オフ」または「フォーカス復帰」のどちらでも `cancel()` される（前者は新設の `createEffect`、後者は既存の `else` 節）。

## 実装順序

1. Phase 1（Rust: `hotkey_toggle` インライン都度読み・`ime_off_on_show` の `show_main_and_emit` 一本化）— 他 Phase と独立、`main.rs` のみ
2. Phase 2（Rust: config_watcher の diff + event 追加）— Phase 3 の前提（イベント名を先に確定させる）
3. Phase 3（フロントエンド: 常時リスニング + シグナルゲート + イベント購読）— Phase 2 が発火するイベント名 `auto-hide-focus-lost-changed` に依存
4. ドキュメント更新（SPEC.md §7.5/§7.6, `src-tauri/CLAUDE.md` の config_watcher.rs イベント一覧）
5. テスト追加・実行

## 不変条件（横断）

- **`config_watcher.rs` の既存不変条件を壊さない**: 言語変更→ホットキー失敗通知の順序、`ReadFailed` 早期 return、index 再構築の `!indexing` ゲートなし。今回追加する diff/emit はこれらと独立した箇所に挿入するため抵触しない
- **リソース管理（listen/unlisten ペア）**: Phase 3 の新設計では `onFocusChanged` リスナーと config イベントリスナーの両方が、他の常時リスナー（`window-shown`/`onResized` 等）と同じ `unlistenFns` ループで一括解除される。専用の register/unregister ペアは廃止したため、対で管理すべき状態が `unlistenFns` 配列1本に統合され、解除漏れのリスクが下がる
- **異常系**: `win.onFocusChanged` が reject した場合（現状も try/catch なし）——既存コードも同様に無防備であり、今回のスコープでエラーハンドリングを新設するのは YAGNI（bootstrap 到着時の初回呼び出しでも同じリスクが既にあり、issue のスコープ外）
- **Rust 側都度読みの失敗時**: `try_state::<AppState>()` が `None` を返すことは setup フェーズ完了後は理論上起きない（`AppState` は `.manage()` で setup 前に登録済み）。`follow_cursor_monitor` の既存パターンに倣い `unwrap_or(default)` でフォールバックする（config.rs の実際の既定値と一致——`hotkey_toggle: true` / `ime_off_on_show: false`）

## テスト方針

### 追加する自動テスト

1. **`ui/src/MainApp.test.tsx`**（新規 it を追加、モック強化を伴う）
   - `@tauri-apps/api/event` の `listen` モックをイベント名でハンドラを捕捉できる形に強化する（`vi.hoisted` 内に `listenHandlers: Record<string, (e: {payload: unknown}) => void>` を追加。**各 `beforeEach` で `listenHandlers` の中身をクリアする**——plan-review で指摘された「`vi.clearAllMocks()` はプレーンオブジェクトの中身までは消さない」点への対処）
   - `@tauri-apps/api/window` の `onFocusChanged` モックを `mockOnFocusChanged`（コールバックを `focusHandler` として捕捉する `vi.fn`）+ `mockUnlistenFocus` に差し替える
   - **fake timer は mount 後に切り替える（Codex Moderate 1 対応）**: `renderMainApp()` は `tick()`（`setTimeout(resolve, 0)` を2回 await）で onMount の直列 await を流し切るため、`beforeEach` 冒頭で `vi.useFakeTimers()` すると mount が進まず刺さる。各テストで **(1) real timer のまま `await renderMainApp()` → (2) `vi.useFakeTimers()` → (3) `blurTimer` 依存のアサート → (4) `finally { vi.useRealTimers() }`** の順にする。100ms 経過は `await vi.advanceTimersByTimeAsync(100)`（timer 前進 + microtask flush を兼ね、`await hideMain()` → `await hideMainWindow()` の解決に必要。`search.test.ts:970` の実績パターン）で行う
   - 新規テスト「`auto-hide-focus-lost-changed` イベントでフォーカス喪失時の非表示挙動が切り替わる」（bootstrap=false 始点）:
     - bootstrap の `auto_hide_on_focus_lost: false` で起動 → `mockOnFocusChanged` は起動時に1回だけ呼ばれる（常時登録のため）。`focusHandler({ payload: false })` を発火させても、gate オフのため `blurTimer` が arm されず、`advanceTimersByTimeAsync(100)` 後も `hideMainWindow` が呼ばれないことを確認（初期状態＝ゲートで止まる）
     - `listenHandlers["auto-hide-focus-lost-changed"]({ payload: true })` を発火 → `focusHandler({ payload: false })` → `await vi.advanceTimersByTimeAsync(100)` → `hideMainWindow` が呼ばれることを確認
     - `focusHandler({ payload: false })` で `blurTimer` を arm した状態 → 100ms 経過前に `listenHandlers["auto-hide-focus-lost-changed"]({ payload: false })` を発火 → `await vi.advanceTimersByTimeAsync(100)` しても `hideMainWindow` が呼ばれないことを確認（`createEffect` によるタイマーキャンセルの検証）
     - `mockOnFocusChanged` が起動時に**1回だけ**呼ばれ、イベント発火を繰り返しても再登録されないことを確認（常時リスニング設計の検証）
   - **新規テスト「既定値 true の起動経路で初回フォーカス喪失が非表示にする」（Codex Moderate 3 対応）**: 実既定は `auto_hide_on_focus_lost: true`（`config.rs:159`）だが上記テストは bootstrap=false 始点のため、**移行後の通常既定挙動を直接は証明しない**。別ケースとして bootstrap の `auto_hide_on_focus_lost: true` で起動 → `focusHandler({ payload: false })` → `await vi.advanceTimersByTimeAsync(100)` → `hideMainWindow` が呼ばれることを確認する
   - 既存テスト「bootstrap の値が...に反映される」等は無変更（イベントモックの後方互換を保つ——`listen` は依然 `Promise<() => void>` を返す）

### Rust 側は新規ユニットテストを追加しない（理由）

- Phase 1 の都度読みはフィールドアクセス + `unwrap_or` のみで分岐ロジックを持たない。同型の `follow_cursor_monitor`（`main.rs:335-339`）も既存のまま無テストであり、これに合わせる（一貫性・YAGNI）。`&AppHandle` を要求するため単体テストには実質的に Tauri アプリのモックが要り、既存コードもその投資をしていない
- `apply_config_change()` 内の diff 判定（`auto_hide_focus_lost_changed` 等）も、既存の同型 diff（`show_icons_changed` 等）が同様に無テストであり、一貫性のため新規テストを追加しない
- **Codex Moderate 2（backend contract test を足せ）への回答**: この指摘は**最大のリスク（イベント名 `"auto-hide-focus-lost-changed"` の Rust/TS 間 typo）を捕捉しない**。Rust テストは Rust 側の文字列を使うため、両言語で綴りを間違えても両方通る。文字列の乖離を捕らえるのは**言語を跨ぐ経路（実起動）だけ**であり、それは plan の**手動スモーク（`SNOTRA_TRACE` + 実フォーカス喪失、下記）が経験的に担う唯一の接地点**である。加えて `apply_config_change()` は `&AppHandle` + 実 config ファイルを要求し、既存 diff が一つも unit test されていないのは意図的にその infra 投資を避けているため。よって Rust テスト追加は費用対効果が合わず、かつ真のリスクを外している。event 名 contract の検証責任は手動スモークに置く（受容する残余）

### 手動検証（自動化不可・理由あり）

`hotkey_toggle` の実際のホットキー押下挙動、`ime_off_on_show` の実際の IME 状態変化は、E2E ハーネス（WebDriver）が OS レベルのグローバルホットキー送出や IME 状態取得の手段を持たないため自動化できない（`e2e/tauri.slash.e2e.ts` には実ホットキー送出の仕組みが存在しない）。`auto_hide_on_focus_lost` の実フォーカス喪失も、E2E フィクスチャが自動化との干渉を避けるため意図的に `auto_hide_on_focus_lost = false` で固定している（既存 e2e 設定より）。フロントエンドの登録/ゲートロジックは上記の `MainApp.test.tsx` で自動検証できるため、手動検証は「実際に Win32 API 層まで反映されるか」に絞る。

`SNOTRA_TRACE=1` で以下を手動確認する（`feedback_win32_input_trace_smoke` の確立パターンを踏襲）:

1. アプリ起動 → 設定ウィンドウで「フォーカス喪失時に非表示」をオフ → メインウィンドウを表示しフォーカスを外す → **再起動なしで**非表示にならないことを確認。オンに戻す → 非表示になることを確認
2. 「ホットキーでのトグル動作」をオフ → メインウィンドウ表示中にホットキーを押す → **再起動なしで**非表示にならず再フォーカスされることを確認（`hotkey:visible_check` トレースで `visible: true` を確認しつつ、非表示イベントが出ないことを stderr で確認）
3. 「表示時に IME をオフにする」をオン → メインウィンドウを表示 → **再起動なしで** IME がオフになることを目視確認

## SPEC.md 更新要否

**要更新**（挙動変更を伴う仕様変更）。

- `SPEC.md` §7.5「設定反映タイミング」に以下を追記する:
  ```
  - フォーカス喪失時自動非表示（`auto_hide_on_focus_lost`）: 検知時に `auto-hide-focus-lost-changed`
    イベントでフロントエンドのシグナルを更新し、次回のフォーカス喪失から新しい設定値が反映される
    （`onFocusChanged` リスナー自体は常時登録済みで、シグナルがゲートとして働く）
  - ホットキートグル動作（`hotkey_toggle`）・表示時の IME オフ（`ime_off_on_show`）: config_watcher は
    専用のイベントを発火しない。ホットキー押下時・表示時に都度 `AppState` の実行中 config から
    直接読むため、次回のホットキー押下/表示から新しい設定値が反映される（再起動不要）
  ```
- §7.6「起動時ブートストラップ」に一言追記し、`auto_hide_on_focus_lost` が「初期値の取得」であって以後の変更は §7.5 のイベント経由で追従することを明記する（現状の記述は「起動時ブートストラップのみ」と読める曖昧さがあり、issue #576 の一因だった）
- `src-tauri/CLAUDE.md` の `config_watcher.rs` セクションの発火イベント一覧に `auto-hide-focus-lost-changed` を追加する（`show-icons-changed` と `max-results-changed` の間、実際の発火順と一致させる）

## セルフレビュー

### 5a. `/plan-review` 結果

**Step 2（並列偵察・3エージェント）**:
- Rust バックエンド（Phase 1/2）: 影響範囲・シグネチャ変更・ヘルパー設計・不変条件・テスト影響・同型パターンの見落とし、全項目「問題なし」。`show_on_startup` は起動シーケンス限定の意味論のため同種バグではないと確認
- フロントエンド（Phase 3）: **「要対処」検出**——`register`/`unregister` 方式に TOCTOU レース（`auto-hide-focus-lost-changed(true)` 連続発火で二重登録・unlisten リーク）。→ 常時リスニング + シグナルゲート方式へ設計変更して解消（上記 Phase 3 参照）。軽微な懸念（`listenHandlers` のテスト間クリア漏れ）はテスト方針に反映済み
- ドキュメント同期: 軽微な懸念（`src-tauri/CLAUDE.md` のイベント一覧追加位置が未指定）→ 挿入位置を明示して解消。SPEC.md 記述整合・セクション番号・config キー4点セット・e2e 影響・architecture.md 影響は全て「問題なし」

**Step 2b（独立再導出、1エージェント・plan.md 非公開）**:
- 対象フィールドの独立列挙は本計画と完全一致（`auto_hide_on_focus_lost` / `hotkey_toggle` / `ime_off_on_show` の3件、`show_on_startup`/`auto_update` は対象外）——**一致は完全性の証拠**
- 設計面で本計画より優れた提案を採用: (1) `ime_off_on_show` を `show_main_and_emit` 内に一本化（4箇所の呼び出し元を単純化）、(2) `auto_hide_on_focus_lost` を register/unregister ではなく常時リスニング+シグナルゲート方式に（Step 2 が独立に検出したレースをそもそも構造的に発生させない設計）
- 差分（導出 ∖ plan）: なし（対象範囲・ファイルとも一致）
- 差分（plan ∖ 導出）: なし（本計画が挙げたヘルパー関数抽出は、独立導出のインライン化提案を採用したことで解消）

### 5b. 追加3観点

1. **境界条件**:
   - `auto-hide-focus-lost-changed` の同値連続発火（`true, true` / `false, false`）: シグナルの `setAutoHideOnFocusLost` は SolidJS の等価チェックにより同値なら再描画・副作用の再実行を起こさない（`createEffect` は依存値が実際に変化した時のみ発火）。新設計では register/unregister のような手続き的副作用がないため、同値連続発火は自明に安全（テストケースに追加済み）
   - フォーカス喪失 → `blurTimer.arm` 直後（100ms 未満）に設定オフ: `createEffect` によるキャンセルで対処（テストケースに追加済み）
   - `config.toml` 読み込み失敗（`ReadFailed`）中の設定変更: `apply_config_change()` が早期 return するため diff 計算自体が走らない（既存不変条件のまま、新規コードはこの分岐の外側にあるため影響なし）
2. **シンプル化の挑戦**: 当初計画の register/unregister 対称ペア + 専用 unlisten 変数管理は、plan-review で「常時リスニング + シグナルゲート」に置き換わり複雑さが正味で減少した（新規の状態変数は `unlistenAutoHide: (() => void) | null` から `autoHideOnFocusLost: Signal<boolean>` へ——後者は他の bootstrap 由来シグナルと同型で特別扱いが不要）。Rust 側もヘルパー関数2個の追加案を「1箇所しか使わないインライン読み取り」+「消費点への一本化」に整理し、新規関数追加をゼロにした
3. **破壊不変条件 + 検知手段**:
   - 破壊されたら即アウト: `onFocusChanged` リスナーの二重登録（メモリリーク・二重 hide 呼び出し）→ 常時1本のみ登録する設計 + `MainApp.test.tsx` の「起動時1回だけ呼ばれ再登録されない」テストで検知
   - 破壊されたら即アウト: 設定オフなのに非表示化が走る／設定オンなのに非表示化が走らない → `MainApp.test.tsx` のゲート検証テスト（オン/オフそれぞれで `hideMainWindow` 呼び出しの有無を確認）で検知
   - 破壊されたら即アウト: ホットキートグル・IME オフ設定が実行中に反映されない（今回の issue の再発）→ 自動テストでは検知不能（Win32 依存）。`SNOTRA_TRACE=1` + 手動確認手順（上記）が唯一の検知手段——これは受容する残余（E2E ハーネスの技術的制約による）

### 5c. Codex 非対話レビュー結果（`codex:rescue`・plan.md/research.md を渡して実施）

| Codex 指摘 | 判定 | 対応 |
|---|---|---|
| **Critical 1**: 常時 onFocusChanged 登録が起動失敗経路を全ユーザーへ広げる | **採用** | 登録を局所 try/catch で隔離（Phase 3 に反映）。register/unregister へは戻さない（TOCTOU 再来を避ける） |
| **Critical 2**: bootstrap 値が新しいイベント値を上書きするレース | **本 PR は見送り（残余受容）＋別 issue** | 同一構造のレースが既存6設定に存在するパターン全体の問題。`auto_hide` だけガードすると非対称。横断本格対応を **#578** に切り出し。#576 はパターン踏襲（Phase 3 に注記） |
| **Moderate 1**: fake timer が `renderMainApp()` の `tick()` をハングさせる | **採用** | real timer で mount → `vi.useFakeTimers()` → `advanceTimersByTimeAsync` → `finally` で復元（テスト方針に反映） |
| **Moderate 2**: フロント mock だけでは Rust の diff/イベント名を検証できない | **見送り（理由補強）** | Rust unit test は event 名 typo を捕えない（Rust 側文字列を使うため）。真の contract 検証は手動スモークが担う——理由書きに明記 |
| **Moderate 3**: 既定値 true の起動経路を直接証明するテストがない | **採用** | bootstrap=true → 初回フォーカス喪失 → hide のテストを追加（テスト方針に反映） |

Confirmed-correct（Codex が実コードと一致を確認）: 対象3設定の把握、Phase 1 の `show_main_and_emit` 集約とシグネチャ変更、Phase 2 の diff/emit 挿入位置と既存不変条件の理解、`update_config` による Engine 全体置換の前提、常時登録+signal gate が二重登録レースを構造的に除去する点。
