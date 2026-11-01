# 調査 — #1289 キーボード入力を IME subclass へ寄せる

## issue の要約

いまの入力経路は持ち主が 2 つに分かれている。IME の 3 メッセージは `windows_ime.rs` の subclass が持ち、通常の打鍵と文字は tao の `KeyEvent` が運ぶ。二重入力を防いでいるのは、tao の**振る舞い**（IME 由来の `WM_CHAR` を `KeyEvent` にしない、`ReceivedImeText` を `WM_IME_ENDCOMPOSITION` でだけ送る）である。#1265 で tao 0.37 に上げたとき、壊れた 3 件はどれもこの振る舞いの変化（型検査を通る種類）だった。

方針は次のとおり。キーボード（`WM_KEYDOWN` / `WM_KEYUP` / `WM_SYSKEYDOWN` / `WM_SYSKEYUP` / `WM_CHAR`）も subclass が見て egui イベントを直接作り、メッセージは消費せずに `DefSubclassProc` へ流す。tao の `KeyboardInput` / `ReceivedImeText` は egui へ渡さず、trace の tripwire にする。完了条件は 4 つ。

- 現行と同じ挙動であること（日本語配列・日本語 IME で実測）
- egui へ入るキーボード由来のイベントが、tao を経由しないこと
- 英字 1 文字と IME 確定の smoke が CI で走ること
- #1272 の残余を測り直すこと

## 関連ファイル・シンボル（grep で実在を確認）

- `snotra-egui-runtime/src/windows_ime.rs`
  - `PlatformIme`（`new` で `SetWindowSubclass(…, ime_subclass_proc, IME_SUBCLASS_ID, …)`・`drain`・`update`・`Drop` で `RemoveWindowSubclass`）
  - `CallbackState { sender: Sender<egui::ImeEvent> }`
  - `is_owned_ime_message`・`ime_events_for`（IMM32 に触らない純粋関数・テスト付き）
  - `ime_subclass_proc`（`catch_unwind` で包む。IME の 3 メッセージは `LRESULT(0)`、それ以外は `DefSubclassProc`。送信したら `InvalidateRect`）
- `snotra-egui-runtime/src/ime.rs`: `ImeBridge`（`platform::PlatformIme` の包み。非 Windows はスタブ）
- `snotra-egui-runtime/src/input.rs`
  - `InputState`（`held_since_focus_gain: HashSet<KeyCode>`・`modifiers: egui::Modifiers`）
  - `admit_key`（#927。focus 獲得時の合成 press を抑止）
  - `on_window_event` の arm: `Focused`（`Focused(false)` で抑止を全消去）・`ModifiersChanged`・`KeyboardInput`・`ReceivedImeText`（tripwire）
  - `on_keyboard_event`（Ctrl+C/X/V → `Copy` / `Cut` / `Paste`〔arboard〕・`Event::Key`・`typed_text_event`）
  - `key_from_tao` / `key_from_character` / `key_from_key_code`（A–Z だけ）/ `modifiers_from_tao`
  - `committed_text_event`（空と制御文字を弾く）・`typed_text_event`
  - 計器 `input_trace` / `input_trace_enabled`（`SNOTRA_EGUI_INPUT_TRACE`）
- `snotra-egui-runtime/src/runtime.rs`
  - `on_event`（`rx_key` / `rx_text` の trace は引き当ての前）・`ActiveWindow::on_window_event`（先頭で `drain_native_ime`。戻り値が true なら `scheduler.request(ZERO)`）
  - `render`（`drain_native_ime` → `input.take`）
  - `drain_native_ime`
- `scripts/smoke-egui.ps1`: Ctrl+H の打鍵（#1268/#1273。`c:\` → `c:`）、focus を跨いで押しっぱなしの Z（#1274）。注入は `scripts/lib/SnotraSmoke.psm1` の `keybd_event`
- 規範: `snotra-egui-runtime/CLAUDE.md`「不変条件」の #1266（2 経路の分担）・#1269（`WM_CHAR` の残余）・IME の 3 メッセージ・#927（focus 獲得時の抑止）
- `docs/adr/ADR-ime-commit-owned-by-subclass.md`（確定を subclass が持つ決定。凍結された歴史）

## tao 0.37.2 の振る舞い（ソースで確認）

- **`WM_CHAR` は subclass にも届く。** tao の `WM_KEYDOWN` 処理は `next_kbd_msg` で後続を覗くが、`PeekMessageW(…, PM_NOREMOVE)` なのでキューから取り除かない（`keyboard.rs:906-921`）。`WM_CHAR` は普通に dispatch され、前段の subclass を通る
- **`TranslateMessage` は tao のループが呼ぶ**（`event_loop.rs:262` / `:401` / `:407` / `:770`）。例外は `msg_hook` が処理済みと答えたときで、Snotra は `msg_hook` を設定していない（`git grep msg_hook` は 0 件。`tauri-runtime-wry` 側は `args.msg_hook` がある場合だけ設定する）
- tao の `WM_KEYDOWN` は `ProcResult::Value(0)` を返し、`WM_SYSKEYDOWN` は `DefWindowProc` へ回す（`keyboard.rs:122-126`）。subclass はどちらも `DefSubclassProc` へ流すだけなので、tao の扱いは変わらない
- `WM_CHAR` / `WM_SYSCHAR` は、`event_info` があるとき（直前の keydown に連結されるとき）だけ tao が `KeyEvent` へ載せる（`keyboard.rs:188-196`）。`event_info` が無ければ捨てる（#1269 の残余の機序）
- focus 獲得時には `WM_SETFOCUS` で `get_async_kbd_state()` から押下中の全キーの合成 press を作り（`keyboard.rs:103-107`）、`WM_KILLFOCUS` で合成 release を作る

## 再利用できる既存パターン

- subclass → `mpsc` → `drain_native_ime` → `InputState` の経路（IME で稼働中）。キーも同じ列へ載せれば、打鍵と確定が 1 本の全順序になる
- 「Win32 から読む部分」と「メッセージ → egui イベントの判定」を分け、後者を純粋関数にしてテストする形（`ime_events_for`）
- `admit_key` は `KeyCode` を鍵にしているが、論理は鍵の型に依存しない（スキャンコードや VK へ置き換えられる）
- `committed_text_event` で制御文字を弾く
- `catch_unwind` で subclass proc を包む（FFI 境界の panic を止める）

## 技術的制約

- **subclass はメッセージを消費しない**（Alt+F4・`WM_SYSKEYDOWN` の既定動作を保つ）。tao の `KeyboardInput` は従来どおり発火するので、`input.rs` 側で egui へ渡さないようにする
- **wake の経路**: `ActiveWindow::on_window_event` は `input.on_window_event` が true を返したときだけ `scheduler.request(ZERO)` を撃つ。`KeyboardInput` を渡さなくなると、打鍵での即時 repaint が「drain したものがあったか」に依存する。IME と同じく subclass の `InvalidateRect` に頼るか、drain の件数を戻り値へ反映する
- **`VK_PROCESSKEY`（0xE5）**: IME の変換中は `WM_KEYDOWN` の wParam がこれになる。現行は `logical_key` が `Key::Process` → `key_from_tao` が None で、変換中の Enter は egui へ届かない（確定の Enter で検索が起動しない）。ただし `physical_key` が A–Z なら `key_from_key_code` が `Some` を返し、**変換中の英字キーは `Event::Key` として egui へ渡っている**。同等性の基準をどちらに置くかは計画で決める
- **サロゲートペア**: BMP 外の文字は `WM_CHAR` 2 通（high → low）で来る。subclass 側で連結する必要がある（tao は `utf16parts` で連結している）
- **`WM_SYSCHAR`**: tao は `text_with_all_modifiers` に Alt+文字の `WM_SYSCHAR` も含める。現行の `typed_text_event` は印字可能なら Commit する。issue は `WM_CHAR` だけを挙げている
- **`WM_DEADCHAR`**: デッドキーは `WM_DEADCHAR` の後、合成済みの文字が `WM_CHAR` で来る。subclass は `WM_CHAR` だけ見ればよい
- **修飾キー**: いまは tao の `ModifiersChanged` で保持し、`PointerButton` / `MouseWheel` / `Key` に載せている。キーイベントの修飾を subclass の `GetKeyState` で取る場合、ポインタ側の修飾と出所が 2 つになる
- **`Event::Key.physical_key`**: いまは `key_from_key_code(physical_key)` で A–Z だけ。subclass ではスキャンコード（lParam の 16–23 bit + 拡張 bit 24）から導く
- **focus 獲得時の抑止（#927）**: subclass が見るのは実メッセージだけで、tao の合成 press は見えない。focus 移行を跨いだ押しっぱなしは、新しいウィンドウには autorepeat の `WM_KEYDOWN`（lParam bit 30 = 直前も押下）として届く
- **results ウィンドウ**: `PlatformIme` は main と results の両方に付く（`ImeBridge::new` は attach ごと）。results は `focusable(false)` でキーを受けない
- **smoke の IME 確定**: CI の Windows runner に日本語 IME が入っているかは未確認。`WM_IME_COMPOSITION` を `PostMessage` で送っても、`ImmGetCompositionStringW` は実 IME 無しでは空を返す
- 足場（`SNOTRA_EGUI_INPUT_TRACE` での 2 経路の併走）は、新設の時点で撤去条件を決める（`docs/development-principles.md`「一時的な足場は、撤去までを新設の時点で決める」）

## 未解決の疑問

1. 変換中（`VK_PROCESSKEY`）の英字キーの `Event::Key` を保つか（現行の挙動）、落とすか
2. `WM_SYSCHAR` を文字として入れるか（Alt+英字。現行は入るはず。実測で確かめる）
3. 修飾キーの出所を subclass へ寄せるか（`GetKeyState` → `ModifiersChanged` も subclass が送る）、tao の `ModifiersChanged` を残すか
4. focus 獲得時の抑止を「`WM_SETFOCUS` で押下中キーを取り込む」と「bit 30 の立った down を、この focus で down を見ていないキーなら落とす」のどちらで移植するか
5. IME 確定の smoke を CI で走らせる手段があるか（runner の IME の有無・代替の注入）
6. wake を `InvalidateRect`（IME と同じ）に寄せるか、drain の件数を `on_window_event` の戻り値へ反映するか

## 敵対的調査（3b）の所見と採否

出力は `workspace/adversarial-1289.txt`（sonnet 1 体・ソースのみ・実行時の測定なし）。

**壊せなかった項目**: 命題 1（`WM_CHAR` は subclass に届く。tao の `PM_REMOVE` はすべて `TranslateMessage` が後に続く）、命題 2（`msg_hook` なし）、命題 3（変換中の Enter は落ち、英字は `Event::Key` で渡る）、命題 4（`WM_SYSCHAR` の文字は Commit される）、すべてのファイル・シンボル・行番号の引用。

| 所見 | 採否 | 理由（一次資料で裁定） |
|---|---|---|
| **#1274 の smoke は tao の合成 press に依存している** | **採る** | smoke 本文（`scripts/smoke-egui.ps1` の #1274 の節）が「合成 press が起きなければ 2 回目の down が通って赤に倒れる」と書いている。移行後は、`WM_SETFOCUS` で押下中のキーを取り込む形（疑問 4 の前者）なら、取り込んだ集合が合成 press の代わりになり、同じ判別力が残る。後者（bit 30 の判定）だと、smoke が何を判別しているかが変わる。→ 疑問 4 は前者に決め、smoke のコメントも書き換える |
| **実 config は `ime_off_on_show = true`**（表示のたびに IME を切る） | **採る** | 手元の実測で既定の打鍵をしても、変換の経路は通らない。完了条件「日本語 IME で実測」は、IME を手で入れて測る手順を計画に書く |
| 英字 1 文字の smoke は、#1274 の節の手順 2（Z で `after_chars == 1` を待つ）で既にある | **採る** | 同節の本文で確認した。英字 1 文字の検査は新設せず、既存の検査へ名指しで寄せる |
| `keybd_event(VK, 0)` はスキャンコード 0 を渡す。tao は 0 のとき `MapVirtualKeyExW` へ落ちる | **採る** | smoke の注入は `bScan = 0`。subclass でも物理キーを導くときに同じ落ち方を持つ |
| `VK_PACKET`（0xE7・絵文字パネル・`SendInput` の Unicode）に触れていない | **採る** | `Event::Key` は作らず、続く `WM_CHAR` だけを文字にする。現行（tao の logical は `Unidentified` 相当）と同じ |
| 変換中の Ctrl+C/X/V も Copy/Cut/Paste へ入る | **採る（現行の挙動として記録）** | 現行の `on_keyboard_event` は `active_key` が `Some` なら `self.modifiers.command` を見る。`VK_PROCESSKEY` の英字も `key_from_key_code` で `Some` になるので、同じ枝に入る |
| tao はマウスのメッセージでも修飾キーを更新する | **採る** | 疑問 3 は、修飾の保持を tao の `ModifiersChanged` のまま残す側に決める。キーイベントに載せる修飾だけを subclass の `GetKeyState` で取る |
| デッドキーの後に結合しないキーが来ると `WM_CHAR` が 2 通になり、subclass では Commit が 2 回になる | **採る（無害と判定）** | egui へ入る文字列は同じ（1 回の Commit に 2 文字か、2 回の Commit に 1 文字ずつか）。`egui_input:changed` の件数は 1 フレームに畳まれる |
| 打鍵の即時 repaint は消えず、遅延が増えるだけ（subclass の `InvalidateRect` が WM_PAINT を起こす） | **採る** | `ime_subclass_proc` は送信後に `InvalidateRect` を撃っている。疑問 6 は両方を持つ側に決める（`InvalidateRect` は前段のイベントが無い `WM_CHAR`〔#1269 の形〕のため、drain の件数の反映は遅延のため） |
| CI で IME 確定は「走らせられない」は言い過ぎ（`Install-Language ja-JP` は未検証、`KEYEVENTF_UNICODE` なら非 ASCII とサロゲートの経路は打てる） | **採る** | 完了条件 3 の IME 確定の検査の扱いは要求の判断なので、計画の未確定欄でユーザーに確かめる |
| #1272 は #1269 を記録した PR で、名前が揃っていない | **採る** | `gh` で確認した: #1269 は issue（CLOSED）、#1272 はそれを残余として記録した PR（MERGED）。計画では「#1269 の残余」と書く |
| Ctrl+H の smoke は #1268 にだけ紐づき、#1273 は本文に出てこない | **採る** | 計画では #1268 と書く |

**数字キーの差（自分で追加）**: tao の `logical_key` では Shift+1 が `"!"` になり、`key_from_character` は None を返す。VK から作ると `Num1` + shift になる。src-tauri が読むキーは ↑↓←→・Escape・Enter だけで（`git grep 'egui::Key::'` で実測）、TextEdit も数字キーの `Event::Key` を使わないので実害は無い。それでも同等性のため、数字の VK は Shift が無いときだけ `Num*` へ写す。テンキーの数字（NumLock 有効時）は現行 `Num*` になるので、`VK_NUMPAD0`–`9` も写す。
