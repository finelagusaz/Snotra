# snotra-egui-runtime

Tauri管理のネイティブWindowへeguiをsoftbuffer（CPUラスタ）で描画する、Snotra専用の接着層。

## モジュール構成

- `lib.rs`: 公開API
- `env.rs`: trace ハッチ（`SNOTRA_EGUI_*_TRACE`）の env 述語。**空文字を「未設定」として扱う唯一の場所**（#872）
- `input.rs`: Taoイベントからegui入力への純粋変換
- `ime.rs`: IME未確定範囲とDPI座標の純粋変換
- `windows_ime.rs`: IMM32 の preedit と確定の取得、候補ウィンドウ位置、subclassの所有/破棄
- `raster.rs`: egui Meshを CPU 側でラスタライズする純関数群（`renderer.rs`が消費）
- `renderer.rs`: softbuffer Surface初期化・`raster.rs`によるCPUラスタ・present
- `monitor.rs`: ウィンドウが載っているモニターのリフレッシュレート取得（現在モード→OS既定→Noneのカスケード・#737。`runtime.rs`が消費）
- `proof.rs`: イベントループスレッド上にいることの証人型`EventLoopProof`と、フレームの外から**その証人を得る**唯一の口`on_event_loop`（イベントループスレッドへ入る手段自体は`AppHandle::run_on_main_thread`にもある——`on_event_loop`はその薄い包みで、唯一なのは証人が付くことのほう。責務詳細は`//!`）
- `repaint.rs`: 即時／遅延repaintをTauriイベントループへ配送（配送規律は「不変条件」を参照）。ウィンドウを外部（別スレッド・別ウィンドウ・Tauriイベントリスナー）から起こす公開ハンドル`WindowWaker`（`EguiRuntime::attach`の戻り値）もここが所有する
- `runtime.rs`: Tauri wry pluginとWindowごとの状態管理（visibleガード・描画失敗リトライを含む）
- `surface.rs`: `is_renderable_extent`（0×0 Surfaceの描画/configureを防ぐガード。renderer.rsが消費）

## 不変条件

### 配送の規律（フレームスケジューリング契約）

この crate が消費側（`src-tauri/egui_shell/`）へ与える保証は**配送の下限間隔**と**予約の単一スロット化**で、**消費側の規範（armed の間は毎フレーム再要求する）はこの 2 つから導かれる**——`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」。「要求しても永久に描かれないウィンドウがある」は保証ではなく**その射程の否定**である。導出の経緯・却下案・errata は `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`（**日付付き設計書ゆえ歴史記録であり、規範の正本はここ**）。

- **配送には下限間隔がある**（フレーム上限＝ウィンドウが載っているモニターのリフレッシュレート・取得失敗時 60Hz・#737）。gate は要求 deadline を**早めも取りこぼしもしない**（遅らせるだけ）。**`min_interval` の変更は次の dispatch から完全反映される**——リフレッシュレートが下がった直後の 1 回だけ旧値の下限で配送されうる（自己回復するため是正しない）
- **予約は「フレームが来ること」を約束しない**（#711）。worker は最も早い deadline だけを**単一スロット**で保持し、dispatch 時に `pending.take()` で**予約全体を空にする**——より早い要求（入力・外部 wake・アニメーション）が 1 つ割り込むと、両者は 1 回の dispatch へ畳まれ、**後の deadline は黙って消える**。`request_repaint_after(d)` を「d 後に 1 枚は来る」と要約してはならない
- **要求しても永久に描かれないウィンドウがある**: 活性化時の softbuffer surface 初期化に失敗したウィンドウは `active` へ入らず、`attach()` が既に返した `WindowWaker` は恒久 no-op になる（`Destroyed`・proxy 切断・hidden も同様に「要求は消えないが何も起きない」経路）

### 一般

- UI状態は`Send`を要求し、無条件の`unsafe impl Send/Sync`を追加しない
- 0×0のSurfaceをconfigureまたは描画しない
- repaint workerは所有型のDropで停止し、joinする。**外部へ渡す wake 経路に`RepaintScheduler`（の強参照・弱参照いずれも）や`egui::Context`の clone を持たせない**——Context の clone は repaint callback ごと複製し、callback が握る Arc がウィンドウの`Destroyed`を越えて停止を止める（#646 PR2〜#671 PR D で実在した破れ）。`WindowWaker`は mpsc の送信側だけを持ち、`SchedulerInner::drop`が`Stop`を明示送信してから join するため、外部が waker を永久保持しても停止は成立する
- Tauri内部型をUI実装へ公開しない
- **通常文字は `KeyEvent::text_with_all_modifiers()` から、IME 確定は `windows_ime.rs` の subclass が `GCS_RESULTSTR` から読んで、それぞれ 1 経路だけで egui へ渡す**（#1266）。tao 0.37 は通常文字を `KeyEvent` にだけ載せ、`event_info` の無い IME 由来の `WM_CHAR` は `KeyEvent` にしない——ゆえに両経路は重ならない。`KeyEvent.text` は使わない（Ctrl を外した文字を返し、Ctrl+A で `a` が入る）。文字の配送は `admit_key` の後に置く（合成 press も文字を持つ）。tao の `ReceivedImeText` はどちらの経路でもなく、`input.rs` / `runtime.rs` に残した arm は**発火したら二重確定を示す tripwire** である。**確定（`mpsc`）と tao のイベント列は別の列で、全順序ではない**——`on_window_event` が各イベントの前に `drain_native_ime` するので通常打鍵では順に積まれるが、tao が再入中にイベントをバッファした場合は先行キーより先に確定が積まれうる（受容する残余）
- **直前のキー押下に連結されない `WM_CHAR` は入らない**（受容する残余・#1269）。tao 0.37 は `event_info` の無い `WM_CHAR` を `KeyEvent` にしないので、他のアプリが `PostMessage(WM_CHAR)` で直接送る文字は、キー押下の直後に並んで連結されない限り捨てられる（tao 0.35 では入っていた・2026-10-09 に直前が離上の並びで両版を実測）。Alt+テンキーの文字コード入力もこの形で届くはずだが、測った PC（日本語配列・日本語 IME）では OS 側で文字にならず（メモ帳でも出ない）、測れていない。**直していない**——拾うには subclass が tao の判定を写す必要があり、写しがずれると通常の打鍵が二重に入る。直すなら、判定を純粋関数にして、`PostMessage(WM_CHAR)` の注入で 0→1 文字になることと、通常の打鍵が二重にならないことを実測する
- IME未確定文字列はeguiが自前描画し、ネイティブ変換ウィンドウは抑制する（#532 の二重表示）。**IME の 3 メッセージ（`WM_IME_STARTCOMPOSITION` / `WM_IME_COMPOSITION` / `WM_IME_ENDCOMPOSITION`）は確定を含めてすべて `ime_subclass_proc` が持ち（判定は `is_owned_ime_message`）、tao にも `DefWindowProc` にも通さない**（#1266）。確定を通すと二重に入る——tao 0.37 が `ENDCOMPOSITION` で `ReceivedImeText` を送り直し、`DefWindowProc` が確定から `WM_IME_CHAR` → `WM_CHAR` を作る。同じメッセージで確定と未確定が立つときは Commit を Preedit より先に送る（理由と順序のテストは `windows_ime.rs` の `ime_events_for`）
- `RedrawRequested`は`on_event`で`WindowEvent`と別armとして扱い、egui入力（`on_window_event`）へ渡さない。渡すとrepaint応答が再描画要求を生み描画が自己永続ループになる（#579で実測: 15秒で約2,000フレーム）
- **`RawInput` の埋めないフィールドは既定値が黙って効く**——`RawInput` を組み立てる箇所を触るときは `InputState::take` が書く値の集合を確かめる。それがそのまま egui への契約である。**`predicted_dt` は 0 のまま保ち、既定の 1/60 へ戻さない**（イベント駆動ゆえ「次フレームは vsync 後に来る」前提を持たない）。戻すと egui が `request_repaint_after(d)` を `d - 16.7ms` へ切り詰め、短い予約を「即時再描画」へ飽和させて**遷移ごとにスピンする**（#628 実測: キャレット点滅で 2fps のはずが 11.5fps・CPU 5.1%）。値は `input.rs` のテストが固定する。アイドルの基準値は `PERFORMANCE.md`
- **`RuntimeFrame` の埋めないフィールドは既定値が黙って効く**（`RawInput` と同型）——**`set_clear_color` は毎フレーム撃つ**。呼ばなかったフレームは `renderer.rs` の `CLEAR_COLOR`（`0x0028_2828`）へ落ちる。**呼び忘れはビルドでも自動テストでも落ちない**——検知するのは `npm run check:colors` と目視で、どちらも CI には無い（`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」）。**`CLEAR_COLOR` と `snotra-core` の既定背景色の一致は規約ではなく機構が固定する**——`src-tauri/src/egui_shell/window_coordinator.rs` の `runtime_fallback_matches_config_default_background`（由来と理由は `snotra-egui-runtime/src/renderer.rs` の doc）
- **focus を獲得した瞬間に押されていたキーは、release まで press を egui へ渡さない**（#927・判定は `input.rs` の `admit_key`）。tao は `WM_SETFOCUS` で**押下中の全キーの合成 press** を作るため（`tao-0.37.1/src/platform_impl/windows/keyboard.rs:103-107`）、設定ウィンドウを Escape の down で閉じて本体が focus を取り戻すと、**1 回の押下で 2 つのウィンドウが閉じる**（実測: 本体が受けた press は `synthetic=true`）。**`Focused(true)` で抑止を消してはならない**——tao は合成 press を `keyboard_callback` で送り、`Focused(true)` はその後の `match msg` → `gain_active_focus` で送る（`event_loop.rs:943` が `:1734` / `:1745` より前）ため、合成 press は `Focused(true)` より**先**に届く。**0.37 で保証は弱まった**——合成 press は `PendingEventQueue` で未完了のキー処理の後ろへ回されうる（崩れる並びと倒れる向きは `input.rs` の `Focused` の arm）。消去点は `Focused(false)` 側であり、**これを外すと抑止が focus セッションを越えて Escape が永久に効かなくなる**（fail-closed）
- **視覚欠陥を追うときは、まず自作 `fill_mesh` がカバレッジ AA を持たないことを疑う**（ピクセル中心の二値判定・`raster.rs`）。glow/wgpu が sub-pixel で吸収していた分数差——フォント間のベースライン差・薄いストローク——が**整数 px へ丸められて顕在化する**ため、「レンダラーを替えてから見えるようになった」類の症状はまずこのクラスに当たる。#399（複数フォントを 1 ファミリに混ぜるとベースラインがずれる・`snotra-settings/CLAUDE.md`）が #579 で再発したときも、糸口はここだった。視覚欠陥は型検査・clippy・ユニットテストを素通りするため、**症状語を先に正確化してから**追う（第一印象に anchor すると再現条件の探索を丸ごと無駄にする）
- **OS へ書く platform output は「変化したときだけ」撃つ**——`handle_platform_output` が毎フレーム無条件に呼ぶと、**ウィンドウに紐づかない Win32 API では 2 つのウィンドウが撃ち合う**。`set_cursor_icon` は tao が `SetCursor` を直接呼ぶ（スレッド共通・最後に呼んだ者が勝つ）ため、ポインタを持つウィンドウ（`Text`）と持たないウィンドウ（`Default`）が交互に上書きしてカーソルが点滅した（#628 の計測中に実機発見。マウス静止中は `WM_SETCURSOR` が来ないので OS の復元も入らない）。同じ形の経路（IME 位置の更新等）を足すときも変化検出を伴わせる
