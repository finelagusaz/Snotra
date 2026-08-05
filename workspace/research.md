# research — #927 設定ウィンドウの Escape で本体も hide される

## issue の要約

設定ウィンドウ（`snotra-settings`）で Escape を押すと、設定が閉じるのと同時に**本体のメインウィンドウまで hide される**。`auto_hide_on_focus_lost = false` でも起きるため blur 経由ではなく、本体側の Escape ラダー（`SPEC.md` §8.1・行 432）が走っている。issue は機序を (A) キーボードのオートリピート / (B) 物理的に 2 回押していた の 2 択で未確定としていた。

## 実測で確定した機序（2026-08-05・`ca76e2c` の release ビルド）

**(A) でも (B) でもない (C) だった。** 詳細と生の trace は issue #927 のコメント（`issuecomment-5191318068`）。

> tao は `WM_SETFOCUS` を受けたとき、その瞬間に物理的に押されている全キーについて
> **`Pressed` の合成イベント**を生成する
> （`tao-0.35.3/src/platform_impl/windows/keyboard.rs:87-93`
> — `get_async_kbd_state()` → `synthesize_kbd_state(ElementState::Pressed, &kbd_state)`）。

設定窓が Escape の **down** で閉じ、フォーカスが本体へ戻った時点でまだ Escape が押されていれば、本体は合成された Escape の press を受け取り、ガードの無い消費点から Escape ラダーが走って hide する。

| 試行（Escape の down→up 間隔） | 本体の hide |
|---|---|
| 60ms | 起きた（`egui_hide:done` 1 件） |
| 900ms | 起きた |
| 1ms | 起きない（本体は Escape を 1 つも受け取らない） |

- `repeat=false`・注入 1 回きり（`keybd_event` はキーリピートを生まない）→ (A)/(B) ではない
- 本体が press を受けた時刻が **focus 復帰と同一フレーム**（`take frame=22 events=2` は `WindowFocused` + Key）
- 押していない CapsLock / Backquote の `Released` が同時に届く＝tao の合成経路の指紋
- **Escape 固有ではない**: `Z` を押しっぱなしのまま設定を閉じると、本体は `physical=KeyZ` の合成 press を受け取る（実測）

**測れていないこと**: 物理キーボードのオートリピート (A) が focus 移行を跨いで新しい前面窓へ届くか。`keybd_event` はリピートを生まないため注入では原理的に測れない（4 秒保持しても `repeat=true` は 0 件だったが、これは (A) の否定にならない）。→ 対処後に人の実打鍵で一括検証する（ユーザー合意済み）。

## 関連ファイル・シンボル

| ファイル | シンボル | 役割 |
|---|---|---|
| `snotra-egui-runtime/src/input.rs` | `InputState::on_window_event` | tao の `WindowEvent` を egui の `RawInput` へ積む。`WindowEvent::KeyboardInput { event, .. }` で **`is_synthetic` を捨てている**（今回の seam） |
| 同上 | `InputState::on_keyboard_event` | `KeyEvent` → `egui::Event::Key` 変換と `push_key` trace（`repeat` / `mapped` を残す） |
| 同上 | `WindowEvent::Focused(focused)` の arm | `raw.focused` 更新 + `Event::WindowFocused` |
| `snotra-egui-runtime/src/runtime.rs:217` | `rx_key` trace | 引き当て前に受信を残す。ここも `KeyboardInput { event: key, .. }` |
| `src-tauri/src/egui_shell/view.rs:302-342` | `read_pre_widget_input` | `ctx.input(|i| i.key_pressed(Escape))` を段 13 で読む |
| 同 `view.rs:530-534` | Escape 消費点 | `if pre.escape { on_escape_pressed }`。**`pre.focused` によるガードは無い** |
| `src-tauri/src/egui_shell/launcher_controller.rs:1037-1062` | `on_escape_pressed` | top-level は `EscapeOutcome::Hide` → `emit_hide()` |
| `snotra-settings/src/app.rs:377-385` | Escape で `ViewportCommand::Close` | 設定側が Escape で閉じるのは現行仕様（**今回は変更しない**・ユーザー判断） |

## 技術的制約（一次資料で確認）

- `tao 0.35.3` の `WindowEvent::KeyboardInput` は **`is_synthetic: bool` を公開している**（`tao-0.35.3/src/event.rs:349-364`）。合成 press は focus 獲得時、合成 release は focus 喪失時に生成される
- **`KeyboardInput` バリアントは `#[non_exhaustive]`**（同 `event.rs:349`）→ **crate 外から構築できない**。ゆえに `on_window_event` へキーイベントを流すユニットテストは書けない。**判定は純粋核へ切り出してテストする**（`input.rs` の既存テストと同じ形）
- `WindowEvent::Focused(bool)` は `#[non_exhaustive]` ではない → **テストから構築でき、`on_window_event` を直接駆動できる**
- `keyboard::KeyCode` は `Copy + Eq + Hash`（`tao-0.35.3/src/keyboard.rs:208-211`）→ `HashSet<KeyCode>` に置ける
- egui の modifiers は `WindowEvent::ModifiersChanged` から作る（`input.rs` の `modifiers_from_tao`）ため、キーイベントを落としても modifiers 追従には影響しない
- 文字入力は `WindowEvent::ReceivedImeText` 経由（`input.rs` の `committed_text_event`）で、キーイベントとは別経路

## 再利用できる既存パターン

- **落とした側も残す**（#872/#936）: `push_text` は `committed` を、`push_key` は `mapped` を残している。今回の抑止も**落としたことを trace に残す**
- **純粋核 + 駆動の分離**: `input.rs` の既存テスト（`modifiers_preserve_windows_shortcut_semantics` 等）は変換関数を直接叩く。同じ形で抑止判定を書く
- 実機再現: `scratchpad/repro-927.ps1`（`scripts/lib/SnotraSmoke.psm1` の `Send-SnotraKey` / `New-SnotraVerificationProfile` / `Wait-SnotraTraceEvent` を利用）。`SNOTRA_EGUI_INPUT_TRACE=1` で `rx_key` / `push_key` / `take` が出る

## 影響範囲

- **触る**: `snotra-egui-runtime/src/input.rs`、`snotra-egui-runtime/CLAUDE.md`（不変条件 1 項目）
- **触らない**: `src-tauri/src/egui_shell/view.rs`（Escape ラダーそのものは正しい）、`snotra-settings/`（Escape 閉じは維持・別 issue 化）、`SPEC.md`（文書化された挙動を変えない——「Escape で非表示」は §8.1 のまま）
- **波及先**: この runtime は main 窓と results 窓の両方が使う。設定窓（eframe）は対象外

## 未解決の疑問（plan の未確定欄へ引き継ぐ）

1. 合成 press を落とすと、hotkey（Alt+Q）で show した直後に届く Alt / Q の合成 press も落ちる。これが従来の挙動を変えないか（`egui_show:done` 後の初回打鍵の喪失は #938 が直したばかりの領域）
2. 抑止したキーの release が届かない経路（release 時に窓が focus を失っている）で、抑止が持ち越されないか
