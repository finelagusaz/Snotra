# 計画 — #1289 キーボード入力を IME subclass へ寄せる

## 目的

キーボード由来の egui イベント（`Event::Key` / 文字の Commit / Copy・Cut・Paste）を、`windows_ime.rs` の subclass が Win32 メッセージから直接作る。tao から受け取るのはウィンドウとイベントループ（とポインタ・focus・修飾の保持）だけにする。二重入力が起きないことを、tao の振る舞いへの前提ではなく「キーボードの持ち主が 1 つ」という構造で保つ。

## 受け入れ条件

1. 通常文字・Ctrl 併用ショートカット（Ctrl+A/C/X/V/H/Z）・↑↓←→・Enter・Escape・IME の確定と未確定表示が、現行と同じに動く。日本語配列・日本語 IME で実測する。**実 config は `ime_off_on_show = true` なので、変換の経路は IME を手で入れて測る**
2. egui へ入るキーボード由来のイベントが、tao の `KeyEvent` / `ReceivedImeText` を経由しない。tao の `KeyboardInput` は egui へ渡さず、trace にだけ残す
3. 次の smoke が CI で走って緑になる
   - 英字 1 文字: 既存の #1274 の節の手順 2（Z で `after_chars == 1`）
   - focus を跨いだ押しっぱなし（#1274）
   - Ctrl+H（#1268）
   - 新設 2 本: `KEYEVENTF_UNICODE` による非 ASCII とサロゲートペアの 1 文字、`PostMessage(WM_CHAR)` の 1 文字（#1269 の残余）
   - IME 確定（1 回だけ入る）は CI に載せず、受け入れ条件 1 の手打ちで確かめる（未確定欄の決定。issue の完了条件 3 を弱める）
4. #1269 の残余（直前のキー押下に連結されない `WM_CHAR` が入らない）を測り直す。入るなら `snotra-egui-runtime/CLAUDE.md` の受容する残余から外す

## 設計の決定（research.md の疑問 1〜6 への答え）

- **D1 変換中の打鍵（`VK_PROCESSKEY`）は現行と同じにする**: `Event::Key` の論理キーは作らず、物理キー（スキャンコード）が A–Z のときだけ `Event::Key` を作る。現行の `key_from_tao(Process) = None` → `key_from_key_code(physical)` を写す形で、Ctrl+C/X/V の枝に入ることも同じである。変換中の Enter は届かない
- **D2 `WM_SYSCHAR` は `WM_CHAR` と同じく文字にする**: 現行の tao は `text_with_all_modifiers` に含めている（ソースで確認）。挙動を変える判断はこの issue の範囲外
- **D3 修飾キーは subclass で読まない**: 保持（`InputState.modifiers` と `Event::ModifiersChanged`）は tao の `ModifiersChanged` のまま残す。subclass は修飾を持たない中立のキーイベント（下の D8 の `NativeInput::Key`）を送り、`InputState` が drain の時点で `self.modifiers` を載せて `egui::Event::Key` にする。Ctrl+C/X/V → Copy / Cut / Paste の判定とクリップボードの読みも `InputState` 側に置く（現行の `on_keyboard_event` と同じ場所・同じ値）。理由は 2 つ: (a) tao の `get_agnostic_mods` は AltGr（`has_alt_graph && VK_RMENU`）のとき Ctrl と Alt を外す（`tao-0.37.2/…/keyboard_layout.rs:251-264`）。`GetKeyState` で取ると AltGr+C/X/V が Copy/Cut/Paste になる（plan-review 要対処 1）。(b) クリップボードの読みを subclass の `RefCell` 借用の中で行わずに済む（要対処 2）。**順序**: Ctrl の `WM_KEYDOWN` で tao が `ModifiersChanged` を出し、次の C の `WM_KEYDOWN` で subclass が送ったキーは、tao の `KeyboardInput(C)` の `on_window_event` 冒頭の drain で積まれる。そのとき `self.modifiers` は Ctrl を持っている。tao は `WM_KILLFOCUS` でも `ModifiersChanged(empty)` を出す（`event_loop.rs:836-841`）
- **D4 focus 獲得時の抑止（#927）**: subclass が `WM_SETFOCUS` を見て、`GetAsyncKeyState` で押下中のキーを取り込む（tao の `get_async_kbd_state` → 合成 press と同じ集合）。**取り込みは集合を置き換える**（足し込まない）。**抑止の鍵は物理キー（スキャンコード + 拡張ビット）にする**——現行も物理 `KeyCode` を鍵にしており、VK を鍵にすると変換中に `VK_PROCESSKEY` で届く押しっぱなしの英字が照合されない（要対処 4）。取り込みの VK は `MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX)` でスキャンコードへ直す。メッセージ側のスキャンコードが 0 のとき（`keybd_event` の注入）も同じ関数で VK から導くので、両側の鍵が揃う。抑止中のキーの down は落とし、**その down に続く `WM_CHAR` も落とす**（現行の「文字の配送は `admit_key` の後」と同じ帰結・#1274）。up は常に渡して抑止を解く。**`WM_SETFOCUS` と `WM_KILLFOCUS` では、抑止の集合・`WM_CHAR` を落とす旗・保留中の high サロゲートの 3 つをすべて初期化する**（要対処 5。旗が残ると次の `PostMessage(WM_CHAR)` を落とし、high が残ると無関係な low と対になる）。tao の合成 press は egui へ渡さなくなるので、#927 の抑止が実際に受け持つのは「focus を跨いだ物理キーの autorepeat」になる（ADR と `CLAUDE.md` の書き方もそれに合わせる）
- **D10 focus を失ったときの解放**: tao は `WM_KILLFOCUS` で押下中キーの合成 release を作り、現行はそれを egui へ渡している。subclass は egui へ down を送ったキーを覚えておき、`WM_KILLFOCUS` でその release を送る（egui の `keys_down` に押しっぱなしを残さない・plan-review 軽微）
- **D5 wake**: subclass はキーイベントを送ったら `InvalidateRect` を撃つ（IME と同じ。前段の tao イベントが無い `WM_CHAR`〔#1269 の形〕のため）。加えて `ActiveWindow::on_window_event` は、drain で 1 件以上積んだら true を返す（tao の `KeyboardInput` を渡さなくなっても、打鍵の repaint を今と同じ遅延に保つ）
- **D6 キーの写像は現行と同じ集合にする**: ↑↓←→・Escape・Tab・Backspace・Enter・Insert・Delete・Home・End・PageUp・PageDown・Space・F1–F20・A–Z・数字。数字の VK（`'0'`–`'9'`）は Shift が無いときだけ `Num*` へ写す（tao の logical は Shift+1 で `"!"` となり写らないため）。`VK_NUMPAD0`–`9` も `Num*` へ写す（AZERTY のように数字が Shift 側にある配列では、無修飾の数字 VK が `Num*` になり現行と違う。src-tauri も TextEdit も数字キーの `Event::Key` を読まないので受け入れる）。`VK_PACKET` は `Event::Key` を作らず、続く `WM_CHAR` だけを文字にする。`physical_key` は A–Z だけ。スキャンコードが 0 なら `MapVirtualKeyW(vk, MAPVK_VK_TO_VSC)` へ落ちる（tao と同じ・`keybd_event` の注入は 0 を渡す）
- **D7 文字の弾き方**: 空と制御文字を弾く `committed_text_event` をそのまま使う。サロゲートペアは high を保留し、low が来たら連結する。**保留はキーのメッセージを跨いで保つ**——`KEYEVENTF_UNICODE` のサロゲートは「down・CHAR(high)・up・down・CHAR(low)・up」で届く（plan-review 軽微）。初期化は focus の出入りだけ。対にならない片割れ（high の後に high、high を持たない low）は捨てる。これは現行の tao（keydown ごとに連結）より広く受け取る変更で、smoke の新設 1 本がこの経路を測る。デッドキーの後の `WM_CHAR` 2 通は 2 回の Commit になるが、入る文字列は同じ
- **D8 イベントの列**: subclass の `mpsc` の要素型を `egui::ImeEvent` から crate 内の enum `NativeInput { Ime(egui::ImeEvent), Key { key, physical_key, pressed, repeat }, Text(String) }` へ広げる。IME もキーも同じ列に載り、打鍵と確定が全順序になる。修飾と Copy/Cut/Paste は `InputState` が drain の時点で決める（D3）
- **D9 計器の種別名を保つ**: `SNOTRA_EGUI_INPUT` の `push_key` / `drop_key` / `push_text` は subclass 側から同じ名前で出す（`ADR-injected-arrow-key-physical-identity` の測り方と `scripts/lib/SnotraSmoke.psm1` のコメントが、これらと `rx_key` を突き合わせる前提で書かれている）。`rx_key` / `rx_text` は tao の到着を表す計器として従来どおり残す。影運転の間だけ subclass 側を `sub_key` / `sub_text` と名乗り、切り替えで元の名前へ移す

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `snotra-egui-runtime/src/keyboard.rs`（新規） | `NativeInput` enum（D8）と純粋な変換核 `KeyboardTranslator`（`held: HashSet<PhysicalScan>`・`down: HashSet<…>`〔D10〕・`drop_chars_until_next_down: bool`・`pending_high_surrogate: Option<u16>`）。`on_focus_gained(held_scans)` / `on_focus_lost() -> Vec<NativeInput>`（D10 の release）/ `on_message(msg, wparam, lparam, scan) -> Vec<NativeInput>`。`key_from_vk(vk, shift)`・`physical_key_from_scan`・`committed_text_event`（`input.rs` から移す）。Win32 の呼び出しを含まない（メッセージ番号と VK は自前の定数。Shift だけは写像の判定に要るので引数で受ける） |
| `snotra-egui-runtime/src/windows_ime.rs` | `CallbackState` に `RefCell<KeyboardTranslator>` を持たせる。`Sender<egui::ImeEvent>` を `Sender<NativeInput>` へ。`ime_subclass_proc` で `WM_SETFOCUS`（`GetAsyncKeyState` の取り込み）・`WM_KILLFOCUS`・`WM_KEYDOWN` / `WM_KEYUP` / `WM_SYSKEYDOWN` / `WM_SYSKEYUP` / `WM_CHAR` / `WM_SYSCHAR` を見る。スキャンコードが 0 なら `MapVirtualKeyW` で導く。キーのメッセージは消費せず `DefSubclassProc` へ流す。`//!` と `is_owned_ime_message` の doc を更新する |
| `snotra-egui-runtime/src/ime.rs` | `ImeBridge::drain` と非 Windows のスタブの戻り値を `Vec<NativeInput>` へ |
| `snotra-egui-runtime/src/input.rs` | `push_ime_event` → `push_native_input(NativeInput)`。`Key` は `self.modifiers` を載せて `egui::Event::Key` にし、Ctrl+C/X/V の枝（arboard の読み）をここへ残す。`Text` は Commit にする。`KeyboardInput` の arm は trace（`rx` 相当）だけにして false を返す。`admit_key`・`held_since_focus_gain`・`on_keyboard_event`・`typed_text_event`・`key_from_tao`・`key_from_character`・`key_from_key_code` を消す（テストは `keyboard.rs` へ移す）。`Focused` の arm から抑止の消去を外す。`ReceivedImeText` の tripwire は残す |
| `snotra-egui-runtime/src/runtime.rs` | `drain_native_ime` が積んだ件数を返し、`ActiveWindow::on_window_event` が `drained > 0 \|\| input.on_window_event(event)` を返す（D5） |
| `snotra-egui-runtime/src/lib.rs` | `mod keyboard;`。`pub use input::{key_from_tao, modifiers_from_tao}` から `key_from_tao` を外す（crate の外の利用者は 0 件・`git grep` で実測。歴史記録の `docs/superpowers/` だけが名指す） |
| `snotra-egui-runtime/CLAUDE.md` | モジュール構成へ `keyboard.rs`。不変条件の #1266 の項（2 経路の分担 → 持ち主が 1 つ）・#927 の項（合成 press → `WM_SETFOCUS` の取り込み）・#1269 の残余（測り直した結果で外すか残す）を書き換える |
| `scripts/smoke-egui.ps1` | #1274 の節のコメント（合成 press → `WM_SETFOCUS` の取り込み）。新設 2 本（`KEYEVENTF_UNICODE` の非 ASCII + サロゲート・`PostMessage(WM_CHAR)`）。末尾の成功メッセージ |
| `scripts/lib/SnotraSmoke.psm1` | `SendInput` + `KEYEVENTF_UNICODE` と `PostMessageW` の注入関数 |
| `docs/adr/ADR-keyboard-input-owned-by-subclass.md`（新規） | 「tao から受け取るのはウィンドウとループのみ。入力の意味づけは subclass が持つ」。却下した 2 案（issue の「採らない案」）と、D1・D2 を現行に合わせた理由 |

`SPEC.md` の更新は不要（入力の内部経路だけで、仕様の挙動は変えない）。

## 実装順序

### Phase 1 — 変換核と影運転

- [x] `keyboard.rs` に `KeyboardTranslator` と写像を作り、単体テストを Red → Green で書く（下の「テスト方針」の 1）
- [x] subclass に `WM_SETFOCUS` / `WM_KILLFOCUS` とキーのメッセージを足す。**この段では egui へ送らず、作ったイベントを `SNOTRA_EGUI_INPUT sub_key` / `sub_text`（D9） の trace にだけ出す**（影運転）。tao の経路は従来どおり egui へ渡す
- [x] 影運転で、同じ打鍵について tao の経路（`push_key` / Commit）と subclass の経路（`sub_key` / `sub_text`）が同じ egui イベント列になることを確かめる。対象は英字・数字（Shift あり・なし）・記号・Ctrl+A/C/X/V/H/Z・↑↓←→・Enter・Escape・Backspace・Alt+英字（D2 の確認）・テンキー。`keybd_event` の注入と、手打ちの日本語 IME（変換中の英字・Enter・確定）で行う。結果は計画の末尾「影運転の記録」へ書く

### Phase 2 — 切り替え

- [x] subclass のキーイベントを `mpsc` へ送る形に切り替え、影運転の trace を消す（**足場の撤去はこの切り替えそのもの**）
- [x] `input.rs` から tao 由来のキー経路を消し、`KeyboardInput` を trace だけにする。`runtime.rs` の wake（D5）
- [x] smoke の新設 2 本と `SnotraSmoke.psm1` の注入関数。#1274 の節のコメントの書き換え
- [x] #1269 の残余を `PostMessage(WM_CHAR)` の smoke で測り直す（2026-11-02・ローカルの smoke:egui で `p` が 2→3 で入った＝解消。Phase 3 で `CLAUDE.md` の受容する残余から外す）

### Phase 3 — 文書

- [x] `snotra-egui-runtime/CLAUDE.md`（モジュール構成・不変条件 3 項）と ADR
- [x] `windows_ime.rs` / `input.rs` / `runtime.rs` の doc とコメントの残骸を、撤去した語彙（`admit_key`・`held_since_focus_gain`・`text_with_all_modifiers`・`synthetic`・`合成 press`）で `git grep` して直す

## 影運転の記録

- 2026-11-02・`97a2c4f1`（debug）・`workspace/shadow-1289.ps1`（`keybd_event` の注入・検証用プロファイル）: 英字・Shift+英字・数字・Shift+数字・テンキー・OEM・Space・Ctrl+A/C/V/Z/H・↑↓←→・Home/End・Backspace/Delete・Tab・Alt+A・F1・Escape。**`tao_key` 44 件と `sub_key` 44 件、`tao_text` 8 件と `sub_text` 8 件が、順序まで一致**（Alt+A の `WM_SYSCHAR` は両経路とも文字になる＝D2 を実測で確認）
- 2026-11-02・同じ版・ユーザーの手打ち（日本語配列・日本語 IME を手で入れた。確定・続け打ち・Escape での取り消し・変換中の Ctrl+V・IME オフで Ctrl+A → Delete → `xyz`）: **`tao_key` / `sub_key` 181 件、`tao_text` / `sub_text` 3 件（`xyz`）が順序まで一致**。変換中の `VK_PROCESSKEY` の英字も一致（D1）。変換と確定の間に subclass へ `WM_CHAR` は来ておらず（`sub_text` は `xyz` の 3 件だけ）、IME 由来の文字が二重に入る経路は観測されなかった。IME の確定は subclass の既存経路から 9 件、tao の tripwire（`push_text`）は 0 件

## 検証の巡

- 巡 1・`a54da096`: 道具 green（A・C・E・F）/ 委譲の指摘 Low 10（Critical・High・Medium 0）/ 変異 9 本中 8 本捕捉・生き残り 1（`|| drained > 0` を外す——遅延だけに効き、測る検出器が無い。受け入れて報告に残す）。Low のうち L0（Shift 変化で release が落ちる）・L1（`VK_PACKET` が物理キーへ落ちる）・L4（`WM_CHAR` で物理キーを導く）・L6（AltGr のテストが修飾を見ない）・L8（旧 ADR の `mapped=` の読み方）・L9（コメントの言い過ぎ）を直した。L2・L3・L5・L7 は受け入れ

## 不変条件と異常系

- **キーのメッセージは消費しない**（`DefSubclassProc` へ流す）。Alt+F4・システムメニュー・tao 自身のキー処理を保つ
- **focus の出入りで変換核の状態をすべて初期化する**（D4）: 抑止の集合は `WM_SETFOCUS` の取り込みで置き換え、`WM_KILLFOCUS` で空にする。旗と保留中の high も両方で初期化する。抑止したキーの up が別ウィンドウへ行っても持ち越さない（fail-closed の防止・#927）
- **抑止した down に続く `WM_CHAR` は入れない**（#1274）。次の down（抑止されないもの）で解除する
- **subclass proc の panic は `catch_unwind` で止める**（既存）。`RefCell` の二重借用は、subclass proc の再入（`DefSubclassProc` の中で同じ proc が呼ばれる）で起きうる。**借用は変換の 1 呼び出しの間だけ持ち、送信・`InvalidateRect`・`DefSubclassProc` の前に手放す**。借用の中で Win32 の他の API（クリップボード等）を呼ばない（D3 でクリップボードは `InputState` 側へ出した）
- **`DefSubclassProc` を呼んだ後は `ref_data`（`CallbackState`）に触れない**（plan-review 要対処 3）。tao のイベント処理の中で `Destroyed` → `PlatformIme` の drop → `Box<CallbackState>` の解放が起きうるため、作業はすべて `DefSubclassProc` の前に済ませ、後は戻り値を返すだけにする（既存の proc も同じ形）
- **`Drop` で `RemoveWindowSubclass` してから `CallbackState` を解放する**（既存の順序を保つ。drop がどのスレッドで走るかは既存の `PlatformIme` から変わらない）
- **subclass は attach（ウィンドウ生成直後・不可視）で付く**ので、最初の `WM_SETFOCUS` より前に在る。取り込みを逃す経路は無い
- 非 Windows ではキーイベントを作らない（スタブ）。現行の非 Windows の tao キー経路は消えるが、この crate は Windows 専用の製品にしか使っていない（`#[cfg(windows)]` の subclass と同じ扱い）
- IME の 3 メッセージは従来どおり `LRESULT(0)` で消費する（変更なし）

## テスト方針と検証コマンド

1. **単体テスト（`keyboard.rs`）**
   - 写像（D6 の全キー・Shift+数字が写らない・テンキー・`VK_PACKET` で Key を作らない）
   - `VK_PROCESSKEY`（論理キーは作らず、物理 A–Z だけ・Enter は作らない）
   - 制御文字と空の文字を弾く
   - サロゲートペアの連結（キーのメッセージを挟んでも連結する）と、対にならない片割れの破棄
   - repeat（lParam bit 30）
   - #927 の抑止の 7 本を移植する（合成 press の代わりに `on_focus_gained`。鍵は物理スキャン）
   - 変換中（`VK_PROCESSKEY`）に届いた押しっぱなしの英字も抑止される（鍵が物理であること）
   - 抑止した down に続く `WM_CHAR` を落とし、次の down で解除する
   - focus の出入りで旗と保留中の high が初期化され、取り込みが集合を置き換える
   - `on_focus_lost` が down 済みのキーの release を返す（D10）
   - `WM_SYSCHAR` を文字にする
   - **`input.rs`**: `NativeInput::Key` に drain 時点の `self.modifiers` が載ること、Ctrl+C/X/V（`command`）が Copy / Cut / Paste になること、AltGr（tao が Ctrl・Alt を外した状態）では C/X/V が Key のままであること
2. **カテゴリ A**（`docs/build-commands.md`）と **カテゴリ C**（smoke:startup / smoke:egui・Pester）。`npm run verify:tree -- run --sha HEAD --categories A,C`
3. **実機**: 受け入れ条件 1 の手打ち（IME は手で入れる）。影運転の記録と同じ表で確かめる
4. 変異注入は `/implement` の委譲先が行う。最低限: 抑止した down の後の `WM_CHAR` を通す、`WM_KILLFOCUS` の全消去を外す、旗の初期化を外す、サロゲートの連結を外す、wake の true を外す、抑止の鍵を VK へ戻す

## 未確定（実装前に潰す）

- [x] **IME 確定の smoke を CI でどう扱うか** — ユーザー回答「CI は非 ASCII まで」（2026-11-01）。CI では `KEYEVENTF_UNICODE` の非 ASCII + サロゲートの 1 文字までを打ち、IME 確定（1 回だけ入る）は手打ちの実測で受け入れる。issue の完了条件 3 を弱めたことを PR 本文と issue へ明記する。却下: runner への `Install-Language ja-JP`（未検証・CI 時間・IME の状態に依存する不安定さ）

## 人間レビュー

- [x] 承認済み — 2026-11-01 / 問い: "計画を承認していただくか、`workspace/plan.md` に注釈を書き込んでくださいませ。" / 回答: "IK"（「OK」の打ち間違いと解して承認として扱った旨をユーザーへ伝えた）

## plan-review 結果

- リスク: 高（状態遷移・subclass と `mpsc` の並行境界・複数モジュール間のインターフェース）
- レビュー方式: 計画準拠レビュー 1 体（Step 2。issue に無い人間の裁定〔IME の smoke を CI に載せない〕が前提にあるため）
- エージェント数: 1
- 出力: `workspace/plan-review-keyboard-subclass.md`

### 要対処
- AltGr のとき `GetKeyState` では Ctrl+Alt が立ち、C/X/V が Copy/Cut/Paste になる — 計画の修正（D3: 修飾は tao の `ModifiersChanged` の保持値を drain 時点で載せる）— 再照合: `tao-0.37.2/…/keyboard_layout.rs:251-264` の `get_agnostic_mods` が `has_alt_graph && VK_RMENU` で Ctrl と Alt を外す
- クリップボードの読みが `RefCell` の借用の中にある — 計画の修正（D3: Copy/Cut/Paste と読みを `InputState` 側へ）— 再照合: tao の `next_kbd_msg` の doc が `PeekMessageW` による再入を警告している
- `DefSubclassProc` の後に `CallbackState` へ触れると、解放済みを読みうる — 計画の修正（不変条件に追加）— 再照合: `runtime.rs` の `Destroyed` の arm が `self.active.remove` で `ActiveWindow`（`ImeBridge`）を落とす
- 抑止の鍵が VK だと、変換中に `VK_PROCESSKEY` で届く押しっぱなしの英字を照合できない — 計画の修正（D4: 鍵を物理スキャンへ）— 再照合: 現行 `admit_key` の鍵は物理 `KeyCode`（`input.rs` の `held_since_focus_gain: HashSet<KeyCode>`）
- focus の出入りで旗と保留中の high を初期化していない — 計画の修正（D4・不変条件）— 再照合: 計画の旧版は `held` の全消去だけを書いていた

### 軽微
- `KEYEVENTF_UNICODE` のサロゲートはキーのメッセージを挟んで届く → D7 へ反映（保留をキーを跨いで保つ。現行より広く受け取る変更として明記）
- tao の `WM_KILLFOCUS` の合成 release が egui へ届かなくなる → D10 を新設
- AZERTY の数字 VK の差 → D6 に受け入れる理由とともに明記
- #927 は合成 press ではなく autorepeat の抑止になる → D4 に明記し、ADR と `CLAUDE.md` の書き方へ回す
- 文字を伴わないキー（Escape 等）でも旗が立つ → 次の down で解除されるので無害（反映なし）
- 1 打鍵で `InvalidateRect` と `drained > 0` の 2 回の再描画要求が出うる → scheduler が畳むので受け入れる（反映なし）
- subclass が focus 獲得後に付く経路 → attach は生成直後・不可視なので起きない（不変条件に明記）
- テンキーの VK が Shift で変わる → Windows が VK を変えた後の値を写すので tao の logical と同じ（反映なし）

### 未検証
- Escape の release と `WM_SETFOCUS` の取り込みの実際のタイミング → Phase 1 の影運転と smoke（#1274 の節）で見る
- IME が `WM_CHAR` / `WM_IME_CHAR` を別経路でも送るか → 受け入れ条件 1 の手打ち（確定が 1 回だけ入ること）で見る。subclass が IME の 3 メッセージを `LRESULT(0)` で消費し `DefWindowProc` へ通さないので、IMM が確定から `WM_IME_CHAR` → `WM_CHAR` を作る経路は閉じている
- egui が `WindowFocused(false)` で `keys_down` を消すか → D10 で release を送るので、依存しない形にした
- `PlatformIme` の drop のスレッド → 既存の構造から変えない（この計画の外）

### 判断
- 実装着手: 可（要対処 5 件をすべて計画へ反映。人間の承認待ち）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 2（3b の敵対的調査 1・plan-review 1）
- 要対処: 5 件（すべて計画へ反映）
- 未検証: 4 件（上の「未検証」。Phase 1 の影運転・smoke・手打ちで見る）
- 散文の全件列挙: 語 10 / 総件数 98 行 / ★未掲載 5（計画へ足した 0。5 件とも凍結された ADR と `docs/superpowers/` の日付付き文書で、歴史記録ゆえ直さない）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 3（D3・D4・D10）/ 主の見落とし 5
