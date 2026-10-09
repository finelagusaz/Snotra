# 調査: #1269 tao 0.37 で Alt+テンキーの文字コード入力が入らない疑い

## issue の要約

tauri 2.12.1 / tao 0.37.1（#1265）で、通常文字を `KeyEvent::text_with_all_modifiers()` から取る形に変えた。tao 0.37.1 は「`event_info` の無い `WM_CHAR`」を `KeyEvent` にせず捨てる。Alt+テンキー（Alt を押したまま `6` `5` → Alt を離すと `A`）の合成文字はこの形で届くはずなので入らない、という疑い。tao 0.35.3 では `minimal_ime` がすべての `WM_CHAR` / `WM_SYSCHAR` を `ReceivedImeText` で送っていたため入っていた。注入プローブの結果は忠実度が疑わしく、実キーボードでの確認が要る。

## 関連ファイル・シンボル

- `tao-0.37.1/src/platform_impl/windows/keyboard.rs` の `KeyEventBuilder::process_message`
  - `WM_KEYDOWN | WM_SYSKEYDOWN` 分岐: `*event_info = None` で始め、`next_kbd_msg` が「キー押下・離上以外」（＝文字）なら `event_info` に部分情報を残す
  - `WM_CHAR | WM_SYSCHAR` 分岐: `event_info.is_none()` なら `trace!("... probably IME, returning.")` → `MatchResult::Nothing`（`KeyEvent` を作らない）。続く `WM_CHAR` が無ければ `event_info.take()` で消費する
  - `WM_KEYUP | WM_SYSKEYUP` 分岐: `event_info` を**立ても消しもしない**
  - `WM_DEADCHAR | WM_SYSDEADCHAR` 分岐: `event_info` を `take()` する（立てない）
  - 帰結: **キー離上の後に届く `WM_CHAR` は `KeyEvent` にならない**——下の「実測」で `PostMessage(WM_CHAR)` により確かめた。ただし離上は `event_info` を消さないので、「押下の直後が文字ではなく離上で、`event_info` が残る」並びがあれば、その後の孤立した文字は拾われうる（敵対的調査の ⚠️・未測）
- `snotra-egui-runtime/src/input.rs`
  - `InputState::on_window_event` の `KeyboardInput` arm → `typed_text_event(pressed, event.text_with_all_modifiers())`（押下時だけ文字を積む）
  - `committed_text_event`（空・制御文字を弾く）
  - `ReceivedImeText` の arm（tripwire。Windows では 0 件が正常）
  - `input_trace` の種別: `rx_key` / `rx_text` / `push_key` / `push_text` / `drop_key` / `take`（`SNOTRA_EGUI_INPUT_TRACE`）
- `snotra-egui-runtime/src/runtime.rs`: `on_event` が `rx_key` / `rx_text` を出す。`on_window_event` が各イベントの前に `drain_native_ime`
- `snotra-egui-runtime/src/windows_ime.rs`
  - `ime_subclass_proc`（`SetWindowSubclass` で tao の wndproc より**前**に呼ばれる）
  - `is_owned_ime_message`（IME の 3 メッセージだけ所有・`WM_CHAR` は所有しない＝テストで固定）
  - `ime_events_for`（純粋核。メッセージ → `egui::ImeEvent` 列）
  - `PlatformIme::drain`（mpsc の受信側）
- `src-tauri/src/egui_shell/launcher_controller/search_flow.rs`: `egui_input:changed`（`SNOTRA_TRACE`・`after_chars`）
- 不変条件: `snotra-egui-runtime/CLAUDE.md`「一般」の「通常文字は `KeyEvent::text_with_all_modifiers()` から、IME 確定は…それぞれ 1 経路だけ」と「IME の 3 メッセージは…すべて `ime_subclass_proc` が持ち」
- ADR: `docs/adr/ADR-ime-commit-owned-by-subclass.md`（`DefWindowProc` が作る `WM_IME_CHAR` → `WM_CHAR` は IME メッセージを通さないことで抑止済み）

## 再利用できる既存パターン

- subclass が tao より先にメッセージを見て、所有するものは `LRESULT(0)` で止め、egui へは mpsc（`CallbackState.sender`）経由で `egui::ImeEvent::Commit` を送る——**確定と同じ経路で孤立した `WM_CHAR` を送れる**。受信側（`drain_native_ime` → `push_ime_event`）は変えずに済む
- 判定を IMM32 から切り離した純粋核（`ime_events_for` に読み出しをクロージャで渡す）——同じ形で「この `WM_CHAR` は tao が捨てるか」の判定を純粋関数にしてテストできる
- 協働スモーク: `SNOTRA_TRACE` + `SNOTRA_EGUI_INPUT_TRACE` を立て、`SNOTRA_CONFIG_DIR` で使い捨てプロファイルへ逃がし、人間の実打鍵を trace の件数で照合する（`scripts/lib/SnotraSmoke.psm1` の `Start-SnotraProcess` / `New-SnotraVerificationProfile`）

## 技術的制約

- subclass から tao の `event_info` は読めない。tao が捨てるかどうかを subclass 側で**写して**判定することになり、写しが tao とずれると二重入力（両方が送る）か取りこぼし（どちらも送らない）になる
- tao の判定は `next_kbd_msg`（`PeekMessage(PM_NOREMOVE)`）で「次のメッセージ」を覗く。subclass も同じ時点で覗けば同じ答えを得るが、tao 版を上げると規則が変わりうる（版を固定する `Cargo.lock` の更新が契機）
- 文字の値: `WM_CHAR` の `wparam` は UTF-16 コード単位。サロゲートペアは 2 通に分かれる（tao も `utf16parts` に積んでから復号する）。Alt+0128 以上や Alt+テンキーで BMP 外は出ないが、`PostMessage(WM_CHAR)` 経由では来うる
- 文字の弾き方は既存の `committed_text_event`（制御文字を弾く）と揃える必要がある——Alt+テンキーで `Alt+8`（BS 相当の `\x08`）なども出せる
- IME: 現状 `DefWindowProc` の `WM_IME_CHAR` → `WM_CHAR` は抑止済み。ただし TSF 系の別 IME が `GCS_RESULTSTR` と `WM_CHAR` の両方で確定を届けるなら、孤立した `WM_CHAR` を拾うと二重確定になる（ADR が「TSF 系の別 IME は未測」と記録）

## 実測（2026-10-09・協働スモーク）

使い捨てプロファイル（`SNOTRA_CONFIG_DIR`・ホットキー Ctrl+Shift+K）で `SNOTRA_TRACE` と `SNOTRA_EGUI_INPUT_TRACE` を立てて起動した。比べたのは main（`a47db00`・tao 0.37.1）と `121fb01`（tao 0.35.3）で、どちらのバイナリにも該当する版の tao の文字列が入っていることを確かめた。「入った」の判定は `egui_input:changed` の `after_chars` で行った——敵対的調査の所見どおり、main では `push_text` / `rx_text` は通常文字で出ないので使えない。

| 操作 | tao 0.37（main） | tao 0.35（`121fb01`） |
|---|---|---|
| 実キーボードで `a` | 入る | 入る（`rx_text` あり） |
| 実キーボードで Alt+6,5 / Alt+0169（NumLock ON・テンキー） | 入らない | **入らない**（`rx_text` 0 件） |
| 絵文字パネル（Win+.） | 入る | 入る |
| `PostMessage(WM_CHAR, 'Z')` を main ウィンドウへ（直前のキーメッセージは離上） | **入らない**（`egui_input:changed` 0 件） | 入る（`rx_text` 1 件 → `after_chars` 0→1） |

- **Alt+テンキーはこの環境では回帰ではない。** 旧版でも `WM_CHAR` が 1 通も届いておらず、**メモ帳でも Alt+65 で `A` は出なかった**（対照・人間が確認）。日本語キーボード（`NonConvert` が出る）と日本語 IME の環境で、OS／IME の段階で Alt-code が文字にならないと見られる。ゆえに issue の見出しの症状は**この環境では測れない**。Alt-code が効く環境（英語配列など）での挙動は未測である
- **実キーボードではテンキーを押すたびに 1 文字入る現象は起きなかった**（両版とも）。issue の注入プローブの観測は注入の副作用である
- **「`event_info` の無い `WM_CHAR` を tao 0.37 が捨てる」は実測で成り立った。** `PostMessage(WM_CHAR)` を送ると、0.35 では入り、0.37 では入らない。これは issue の末尾が挙げた「他のアプリが `PostMessage(WM_CHAR)` で直接送る文字」であり、**この経路は回帰として実在する**
- 計器の注記: 各 show の直後に `Pressed Unidentified(Windows(0))` と `Released AltLeft` ×3 が出る（両版とも）。Snotra 自身が前面化のために撃つ合成キーと見られ、今回の判定には関わらない

## 敵対的調査（3b）の採否

出典: `workspace/adversarial-1269.txt`

- **採る**: 「トレースは main で入った／捨てたを区別できず、`egui_input:changed` だけが区別する」——判定をこれに切り替えた（上の表）
- **採る**: 「旧版で `rx_text` が出て main で出ないのは計器の差である」——比較は `after_chars` だけで行った
- **採る**: 「`WM_CHAR` をすべて `LRESULT(0)` で止めると、tao が `KeyEvent` を作れなくなる」——直すなら、止めるのは tao が捨てる孤立した `WM_CHAR` だけに限る
- **採る**: 「tao の `event_info` 判定を丸ごと写す必要はなく、『直前のキーメッセージが離上だったか』を覚えるだけで孤立を判定できる」——直す場合の設計候補にする。ただし離上は `event_info` を消さないので、古い `Some` が残る並びを潰しておく必要がある
- **採る（訂正）**: DEADCHAR は `event_info` を立てない（`take()` する）。「関連ファイル」の記述を直した
- **採る**: 実 config（Alt+Q・`ime_off_on_show = true`）で測ると Alt が押されたまま show される——測定は Ctrl+Shift+K の使い捨てプロファイルで行った
- **機序は採らない（未決）**: 「注入プローブは SYSKEYDOWN → SYSCHAR の枝を通った」——実キーボードで 1 文字ずつ入る現象が出なかったことは実測したが、注入の機序そのものは測っていない
- **壊せなかった項目**: 命題 2（tao 0.35.3 の `minimal_ime` が全 `WM_CHAR` / `WM_SYSCHAR` を `ReceivedImeText` で送っていた）と命題 4 の中核（IME の 3 メッセージを所有することで、`DefWindowProc` 由来の `WM_IME_CHAR` → `WM_CHAR` は生じない）。後者の全称（「そういう経路は無い」）は、TSF 系 IME が未測なので弱めた

## 未解決の疑問

1. 実キーボードで Alt+テンキーの合成文字は本当に入らないか（main / tao 0.37）。tao 0.35（`121fb01`）では入るか
2. 実キーボードで Alt 押下中のテンキーが 1 文字ずつ入るか（注入プローブで観測した挙動が実在するか）——実在するなら、拾ったうえで Alt+65 は `65A` になる
3. Alt-code の `WM_CHAR` はどのメッセージの後に届くか（`WM_KEYUP VK_MENU` の後か、`WM_SYSKEYUP` か）。subclass 側の判定の形を決める
4. ほかに同じ形で落ちる入力経路はあるか（タッチキーボード・絵文字パネル `Win+.`・音声入力・`PostMessage(WM_CHAR)` を送る他アプリ）
5. 直すなら、subclass が拾う文字と tao の `KeyEvent` が二重にならないことをどう保証するか（判定の写しと、それを守る検査）
