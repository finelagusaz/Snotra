# snotra-egui-runtime

Tauri管理のネイティブWindowへeguiをsoftbuffer（CPUラスタ）で描画する、Snotra専用の接着層。

## モジュール構成

- `lib.rs`: 公開API
- `env.rs`: trace ハッチ（`SNOTRA_EGUI_*_TRACE`）の env 述語。**空文字を「未設定」として扱う唯一の場所**（#872）
- `input.rs`: Tao イベント（ポインタ・focus・修飾）と subclass の入力から egui 入力を組む
- `keyboard.rs`: キーボードの Win32 メッセージを egui 入力へ写す純粋な変換核（subclass が使う）
- `ime.rs`: IME未確定範囲とDPI座標の純粋変換
- `windows_ime.rs`: subclass の所有/破棄（キーボードと IME のメッセージを読み `keyboard.rs` で変換して送る）、IMM32 の preedit と確定の取得、候補ウィンドウ位置
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
- **キーボード由来の入力（打鍵・文字・IME の確定と未確定）は `windows_ime.rs` の subclass だけが作り、tao の `KeyboardInput` / `ReceivedImeText` は egui へ渡さない**（#1289・`ADR-keyboard-input-owned-by-subclass`）。メッセージから egui の入力への変換は `keyboard.rs` の `KeyboardTranslator`（純粋）が持つ。subclass はキーのメッセージを消費せず `DefSubclassProc` へ流す（Alt+F4・システムメニュー・tao の修飾の追跡を保つ）。打鍵・文字・確定は同じ `mpsc` に載って全順序を保ち、`drain_native_ime` が各ウィンドウイベントの前と `render` で回収し、積んだら repaint を要求する
  - **修飾は subclass で読まない**——drain の時点で `InputState` が tao の `ModifiersChanged` の保持値を載せ、Ctrl+C/X/V の判定もそこで行う。tao は AltGr のとき Ctrl・Alt を外すので、`GetKeyState` で取ると AltGr+C が Copy になる
  - **subclass の `RefCell` は変換の 1 呼び出しの間だけ借り、`DefSubclassProc` の後は `CallbackState` に触れない**——`DefSubclassProc` の中で同じ proc が再入しうる（二重借用は `catch_unwind` が飲み込み、打鍵が黙って消える）うえ、tao がその中でウィンドウを壊すと `CallbackState` が解放される
  - tao の `ReceivedImeText` は Windows では 0 件が正常である。`rx_text` に現れたら、下の IME の 3 メッセージの抑止が破れている tripwire である
  - **tao に残る依存は、そのループが `TranslateMessage` を呼ぶことだけである**（tao 0.37.2 の `event_loop.rs`）。崩れると `WM_CHAR` が来なくなり、smoke の英字 1 文字（`scripts/smoke-egui.ps1` の #1274 の節の手順 2）が赤にする
- IME未確定文字列はeguiが自前描画し、ネイティブ変換ウィンドウは抑制する（#532 の二重表示）。**IME の 3 メッセージ（`WM_IME_STARTCOMPOSITION` / `WM_IME_COMPOSITION` / `WM_IME_ENDCOMPOSITION`）は確定を含めてすべて `ime_subclass_proc` が持ち（判定は `is_owned_ime_message`）、tao にも `DefWindowProc` にも通さない**（#1266）。確定を通すと二重に入る——tao 0.37 が `ENDCOMPOSITION` で `ReceivedImeText` を送り直し、`DefWindowProc` が確定から `WM_IME_CHAR` → `WM_CHAR` を作る。同じメッセージで確定と未確定が立つときは Commit を Preedit より先に送る（理由と順序のテストは `windows_ime.rs` の `ime_events_for`）
- `RedrawRequested`は`on_event`で`WindowEvent`と別armとして扱い、egui入力（`on_window_event`）へ渡さない。渡すとrepaint応答が再描画要求を生み描画が自己永続ループになる（#579で実測: 15秒で約2,000フレーム）
- **`RawInput` の埋めないフィールドは既定値が黙って効く**——`RawInput` を組み立てる箇所を触るときは `InputState::take` が書く値の集合を確かめる。それがそのまま egui への契約である。**`predicted_dt` は 0 のまま保ち、既定の 1/60 へ戻さない**（イベント駆動ゆえ「次フレームは vsync 後に来る」前提を持たない）。戻すと egui が `request_repaint_after(d)` を `d - 16.7ms` へ切り詰め、短い予約を「即時再描画」へ飽和させて**遷移ごとにスピンする**（#628 実測: キャレット点滅で 2fps のはずが 11.5fps・CPU 5.1%）。値は `input.rs` のテストが固定する。アイドルの基準値は `PERFORMANCE.md`
- **`RuntimeFrame` の埋めないフィールドは既定値が黙って効く**（`RawInput` と同型）——**`set_clear_color` は毎フレーム撃つ**。呼ばなかったフレームは `renderer.rs` の `CLEAR_COLOR`（`0x0028_2828`）へ落ちる。**呼び忘れはビルドでも自動テストでも落ちない**——検知するのは `npm run check:colors` と目視で、どちらも CI には無い（`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」）。**`CLEAR_COLOR` と `snotra-core` の既定背景色の一致は規約ではなく機構が固定する**——`src-tauri/src/egui_shell/window_coordinator.rs` の `runtime_fallback_matches_config_default_background`（由来と理由は `snotra-egui-runtime/src/renderer.rs` の doc）
- **focus を獲得した瞬間に押されていたキーは、release まで down を egui へ渡さず、その down に続く `WM_CHAR` も入れない**（#927・#1274）。subclass が `WM_SETFOCUS` で `GetAsyncKeyState` から押下中のキーを取り込み（`windows_ime.rs` の `held_keys`）、`KeyboardTranslator` が照合する。設定ウィンドウを Escape の down で閉じて本体が focus を取り戻したとき、押しっぱなしの Escape の autorepeat が本体へ届いて **1 回の押下で 2 つのウィンドウが閉じる**のを止める。**照合のキーは VK ではなく物理キー（スキャンコード）である**——変換中の打鍵は `VK_PROCESSKEY` で届く。**`WM_SETFOCUS` と `WM_KILLFOCUS` で、抑止の集合・`WM_CHAR` を落とす旗・保留中のサロゲートをすべて初期化する**——消去点を外すと抑止が focus セッションを越えて Escape が永久に効かなくなる（fail-closed）。focus を失ったときは、egui へ down を送ったキーの release を送る（tao の合成 release の代わり。egui の `keys_down` に押しっぱなしを残さない）
- **視覚欠陥を追うときは、まず自作 `fill_mesh` がカバレッジ AA を持たないことを疑う**（ピクセル中心の二値判定・`raster.rs`）。glow/wgpu が sub-pixel で吸収していた分数差——フォント間のベースライン差・薄いストローク——が**整数 px へ丸められて顕在化する**ため、「レンダラーを替えてから見えるようになった」類の症状はまずこのクラスに当たる。#399（複数フォントを 1 ファミリに混ぜるとベースラインがずれる・`snotra-settings/CLAUDE.md`）が #579 で再発したときも、糸口はここだった。視覚欠陥は型検査・clippy・ユニットテストを素通りするため、**症状語を先に正確化してから**追う（第一印象に anchor すると再現条件の探索を丸ごと無駄にする）
- **OS へ書く platform output は「変化したときだけ」撃つ**——`handle_platform_output` が毎フレーム無条件に呼ぶと、**ウィンドウに紐づかない Win32 API では 2 つのウィンドウが撃ち合う**。`set_cursor_icon` は tao が `SetCursor` を直接呼ぶ（スレッド共通・最後に呼んだ者が勝つ）ため、ポインタを持つウィンドウ（`Text`）と持たないウィンドウ（`Default`）が交互に上書きしてカーソルが点滅した（#628 の計測中に実機発見。マウス静止中は `WM_SETCURSOR` が来ないので OS の復元も入らない）。同じ形の経路（IME 位置の更新等）を足すときも変化検出を伴わせる
