Issue #1289 計画レビュー（観点: 競合・順序・再入 / 対称ペアと状態）。対象: workspace/plan.md。読み取り専用の調査で、コードは変更していない。

## 要対処（実装前に直す）

1. **修飾キーの AltGr フィルタが D3 から落ちている（Ctrl+C 誤発火）**
   - 現行の修飾は tao の `get_agnostic_mods` 由来で、AltGr レイアウトでは Ctrl/Alt を落とす（`tao-0.37.2/src/platform_impl/windows/keyboard_layout.rs:252-264`: `filter_out_altgr = layout.has_alt_graph && key_pressed(VK_RMENU)`）。
   - D3 の「キーイベントの修飾と Ctrl+C/X/V の判定を subclass の `GetKeyState` で取る」を素直に実装すると、AltGr が立てる偽の Ctrl で、AltGr+C/X/V が Copy/Cut/Paste になる。`Event::Key.modifiers` も ctrl+alt になる。
   - 現行は `self.modifiers.command` を見ている（`input.rs:374`）ので、これは挙動の後退である。
   - 開発機は日本語配列で AltGr 無しのため、手打ち確認では出ない。
   - 対処: 修飾の導出を tao と同じ規則（AltGr 判定つき）にするか、`KeyboardTranslator` の入力 `modifiers` の導出関数を純粋関数に切り出してテストに「AltGr 時 Ctrl=false」を入れる。D3 にこの規則を明記する。

2. **クリップボード読み（`arboard`）を `RefCell` 借用の内側で呼ぶ設計になっている**
   - 計画は `on_message(..., read_clipboard)` に読みを注入する（plan.md:35）。
   - 再入は現実の経路である。tao は keydown 処理中に `PeekMessageW` を呼び、コメント自身が「Re-entrance to the event handler」と警告している（`keyboard.rs:896-905` 付近の `next_kbd_msg` の doc）。`PeekMessage` や `SendMessage` は、他スレッドからの sent message を同じ thread で処理する。
   - 借用中にクリップボード（OpenClipboard の再試行・遅延レンダリング）や `catch_unwind` 内の外部呼び出しが他の sent message を処理すると、同じ subclass proc が再入する。`borrow_mut` が二重借用で panic し、`catch_unwind` が握りつぶして、そのキー入力が黙って消える。
   - 現行の `on_keyboard_event`（`input.rs:385-392`）は `&mut self` の中で読むが、subclass には再入がある。
   - 対処: 借用の中ではクリップボードを読まない。`on_message` は `Paste` の意図（例: `PendingPaste` マーカー）を返し、借用を手放してから読んで `Event::Paste` を作る。または `try_borrow_mut` が失敗したときの扱い（捨てる／trace に残す）を決める。計画の「借用は DefSubclassProc の前に手放す」（plan.md:73）だけでは、Def より前の区間の再入が覆われない。

3. **`state` 参照を `DefSubclassProc` の後に使わない、を不変条件に書く（use-after-free の防止）**
   - `ime_subclass_proc` は `DefSubclassProc` の中で tao の `Destroyed` を発火しうる。
   - `runtime.rs:253-256` は `Destroyed` で `self.active.remove(...)` する。そこで `PlatformIme::drop`（`windows_ime.rs:141-150`）が走り、`RemoveWindowSubclass` の直後に `Box<CallbackState>` が解放される。これは subclass proc が `DefSubclassProc` の内側にいる最中に起こりうる。
   - 現行は Def の後に `state` に触れないので安全である。計画の変更で `RefMut` ガードや `state` を Def の後まで生かすと、解放済みメモリを読む。
   - 対処: 計画の不変条件に「`DefSubclassProc` の後で `CallbackState` へ触れない（ガードも `state` 参照もクロージャ内で完結させる）」を足す。現行の `catch_unwind` 内クロージャの形（`windows_ime.rs:211-241`）を保つのがよい。
   - あわせて、`PlatformIme` が `Send` を要求される（`windows_ime.rs:33-35`）ことから、`Drop` が別スレッドで走る可能性がある。`RemoveWindowSubclass` は作成スレッドからしか成功しない。失敗時は解放済みの `ref_data` が残る。⚠️ 既存の問題だが、subclass が `state` を実際に触るメッセージが増える（WM_CHAR 等）ので影響範囲が広がる。確認手段は未実施（`Drop` の実行スレッドを確かめる）。

4. **抑止の鍵が VK のため、変換中（`VK_PROCESSKEY`）で抑止が外れる ⚠️**
   - 現行の抑止は物理キー（`KeyCode`）を鍵にする（`input.rs:101-116`）。計画は `held: HashSet<u16>`（VK）にする（plan.md:35）。
   - `WM_SETFOCUS` 時の `GetAsyncKeyState` は実 VK（例 `'A'`）を入れる。一方 IME が有効で変換中の打鍵は `wParam = VK_PROCESSKEY(0xE5)` で届く（research.md:58）。D1 はこのとき物理キー（スキャンコード）で A–Z の `Event::Key` を作る。
   - 結果として、focus 獲得時に押していた英字の変換中リピートは抑止の鍵に当たらず、`Event::Key` が漏れる（現行は物理キー鍵なので抑止される）。
   - Escape は実 VK で届くため #927 の本線は守られる。影響は英字のみ（`Event::Key` のみで文字は IME が持つ）で小さいが、移植の「同じ判定」にはなっていない。
   - 対処: 抑止の鍵を「D1/D6 で `Event::Key` を作るときに使う識別子」に揃える。スキャンコード由来の物理キーを併用する、または `VK_PROCESSKEY` のときは `MapVirtualKey`/スキャンコードから VK を引いて照合する。

5. **`WM_KILLFOCUS` / `WM_SETFOCUS` で何をリセットするかが 1 か所に定まっていない（対称ペアの欠け）**
   - 計画が明示するリセットは `held` の全消去だけ（plan.md:24, 71）。状態は 3 つある（`held`・`drop_chars_until_next_down`・`pending_high_surrogate`、plan.md:35）。
   - `drop_chars_until_next_down` を focus セッション間で残すと、次のセッションで外部から `PostMessage(WM_CHAR)` された文字（#1269 の smoke、plan.md:15）が、直前に抑止した down の残りフラグで落ちる。smoke の新設 2 本が、前のブロックの状態次第で赤くなる。
   - `pending_high_surrogate` は focus を跨ぐと、後から届いた low が無関係な high と連結される。
   - 対処: `on_focus_lost` と `on_focus_gained` の両方で 3 つとも初期化し、`held` は「追記」でなく「置換」（`clear()` してから snapshot）にする。単体テストに「フラグ・保留 high が focus 変化で消える」を足す。

## 軽微

1. **VK_PACKET の surrogate は KEYUP/KEYDOWN を挟んで届く**
   - `SendInput(KEYEVENTF_UNICODE)` の surrogate は、`KEYDOWN(VK_PACKET) → CHAR(high) → KEYUP → KEYDOWN(VK_PACKET) → CHAR(low) → KEYUP` の列になる（high と low がそれぞれ別の打鍵）。
   - `pending_high_surrogate` を keydown/keyup で消してはならず、`drop_chars_until_next_down` の「次の down で解除」とも干渉しない。
   - 計画の新設 smoke（plan.md:15）と単体テストに、この列の fixture を入れる。tao の `utf16parts` は keydown 単位（`keyboard.rs:188-200` 付近）で連結するので、tao とは挙動が違う。「現行と同じ」の主張に含めない。

2. **`drop_chars_until_next_down` は Escape のように文字を生まないキーでも立つ**
   - 抑止した Escape の down の後、離して次の down が来るまで、外部の `PostMessage(WM_CHAR)`（#1269）は落ちる。
   - up でもフラグを下ろす（up は常に通す設計なので自然）か、直後に WM_CHAR が無ければ解除する形が望ましい。

3. **初回 show 前に subclass を取り付けた場合、`WM_SETFOCUS` を取り逃がす ⚠️**
   - `PlatformIme::new` は `attach_pending_windows`（`runtime.rs:203, 384`）で付く。付く前にウィンドウが既に focus を持っていると snapshot が空のまま始まる。
   - 現行の tao も同条件で、初回起動直後の 1 打鍵の窓は同じ。後退ではないが、`SetWindowSubclass` の直後に `GetFocus() == hwnd` なら `on_focus_gained` を呼ぶと対称になる。

4. **tao の `WM_KILLFOCUS` 合成 release が失われる**
   - `keyboard.rs:109-112` は `WM_KILLFOCUS` で押下中キーの `Released` を合成する。`KeyboardInput` を egui へ渡さなくなると、focus を失う前に押されていたキーの release が egui へ届かなくなる。
   - 「release を読む消費者はこの crate にも `src-tauri/src/egui_shell/` にも無い」という `input.rs:98-100` の grep 結果が前提であり、変換核の `on_focus_lost` が `Key{pressed:false}` を出す必要は無さそうだが、egui 自身が `WindowFocused(false)` で `keys_down` を掃除するかは未確認（⚠️）。

5. **数字キー・AZERTY の差**
   - D6 は VK `'0'..'9'` を Shift 無しで `Num*` に写す。現行は `logical_key` が文字のときだけ写る（`input.rs:568-578`）ので、AZERTY の非 Shift の数字行（logical は `"&"` 等）は現行で `None`、計画では `Num1` になる。
   - 消費者がいない（research.md:99）ので実害は無い。D6 の「現行と同じ集合」は厳密には「日本語配列で同じ」である。

6. **Shift 押下で VK が変わるテンキー**
   - テンキー `NUMPAD7` 保持中に Shift を押すと VK が `VK_HOME` に変わり、抑止の鍵に合わない（現行は物理キー鍵で同じ KeyCode）。日常の経路ではない。

7. **wake の二重**
   - D5 は subclass の `InvalidateRect` と `drained > 0` による `scheduler.request(ZERO)` の両方を撃つ。1 打鍵で `RedrawRequested` 経由と ZERO の 2 回描画要求になりうる。`repaint.rs` の単一スロットが畳むが、CPU ラスタのコストなので、計測で重複描画が増えないことを見る。IME 側が従来 `request` を受けなかった分、プレエディット更新の描画要求が増える。

8. **#927 の説明の書き換え**
   - 移行後、tao の合成 press は egui へ入らない（`KeyboardInput` を渡さない）ので、#927 の症状そのもの（合成 press による 2 ウィンドウ閉鎖）は構造的に起きない。残る抑止は「実リピートの down」（`input.rs:94-96` の (A)）だけである。
   - 計画の D4・CLAUDE.md の書き換え（plan.md:41）では「合成 press → `WM_SETFOCUS` の取り込み」と書くより、この役割縮小を正直に書く。smoke #1274 の判別力は保たれる（下の「確認できたこと」）。

## 未検証

1. ⚠️ **WM_SETFOCUS の snapshot と実リピートのタイミング**
   - 設定ウィンドウ（別 HWND）が Escape の down で閉じ、本体が focus を取るまでに Escape を離した場合の挙動は、実機で押下タイミングを測っていない。
   - GetAsyncKeyState が押下なしを返すなら抑止は立たず、up が先に別ウィンドウへ行くので漏れは無いはずだが、実測していない。
   - 同様に、SetFocus を他スレッドが撃ち、sent の `WM_SETFOCUS` が遅れて処理される間に新規の物理打鍵が混ざると、その打鍵が抑止される窓が ms 単位で在る（tao の現行実装も同条件）。

2. ⚠️ **IME が `WM_CHAR` を併せて送る経路**
   - `WM_IME_COMPOSITION` は `LRESULT(0)` で消費されるため `DefWindowProc` が `WM_IME_CHAR → WM_CHAR` を作らない（`windows_ime.rs:153-167`）。他方、IME や TSF によっては `WM_IME_CHAR` や `WM_CHAR` を別経路で直接送る。
   - これまで tao が `event_info` 無しで捨てていた `WM_CHAR` を、計画は全部 Commit にする。IME 確定の二重化は CI 対象外という人間の決定どおり、手打ちで確かめる前提。このリスク所在のみ記録する。

3. ⚠️ **egui の `keys_down` が focus 喪失で掃除されるか**（軽微 4 の裏付け）。egui のソースを未読。

4. **`DefSubclassProc` 内からの `Drop` スレッド**（要対処 3 の補足）。`PlatformIme` が実際にどのスレッドで落ちるかは未追跡。

## 確認できたこと（根拠つき・指摘ではない）

- **計画の名指すファイル・シンボルは実在する**: `windows_ime.rs`（`CallbackState` `:28`・`is_owned_ime_message` `:162`・`ime_events_for` `:176`・`ime_subclass_proc` `:202`・`PlatformIme::drop` `:141`）、`ime.rs`（スタブの `drain` `:95`、`ImeBridge::drain` `:114`）、`input.rs`（`admit_key` `:101`・`held_since_focus_gain` `:22`・`on_keyboard_event` `:352`・`typed_text_event` `:457`・`committed_text_event` `:464`・`key_from_key_code` `:469`・`key_from_tao` `:521`・`key_from_character` `:565`）、`runtime.rs`（`drain_native_ime` `:539`・`on_window_event` `:401`・`rx_key`/`rx_text` `:222/:230`）、`lib.rs:14` の `pub use input::{key_from_tao, modifiers_from_tao}`、`ADR-synthetic-key-press-suppression.md`・`ADR-injected-arrow-key-physical-identity.md`。
- **`key_from_tao` の crate 外の利用者は 0 件**: `.claude/worktrees/` の複製を除くと `git grep` で `snotra-egui-runtime/src/input.rs` 内のみ。
- **D1 は現行と一致**: `key_from_tao(Process)` は `_ => None`（`input.rs:561`）、フォールバックの `key_from_key_code(physical)` は A–Z のみ（`input.rs:473-501`）、`on_keyboard_event` は `active_key` が `Some` なら `self.modifiers.command` の Ctrl+C/X/V 分岐に入る（`input.rs:354-397`）。
- **D2 は一致**: `WM_CHAR | WM_SYSCHAR` を同じ arm で `utf16parts` へ（`keyboard.rs:188`）、現行の `typed_text_event` は印字可能なら Commit（`input.rs:457-466`）。
- **D6 は日本語配列で一致**: 写像は `key_from_tao`（矢印・Escape・Tab・Backspace・Enter・Insert・Delete・Home・End・PageUp/Down・Space・F1–F20、`input.rs:525-559`）と `key_from_character`（数字・A–Z、`:568-604`）の和。`physical_key` は A–Z のみ（`:401`）。#927 の単体テストは移植元が 7 本（`input.rs` の tests 先頭の `synthetic_press_at_focus_gain_is_not_admitted` から `losing_focus_clears_suppression`）で、計画の「7 本」と一致。
- **#1274 smoke は D4 でも判別力が残る**: 2 回目の Z down はリピートビット付きの実 `WM_KEYDOWN` として subclass に届く（`smoke-egui.ps1:597`）。`WM_SETFOCUS` の snapshot が無い、または後続 `WM_CHAR` を落とさないなら `[0->1, 1->2]` で赤になる（`:616-621` の判定）。前提の「合成 press」という文言（`:547-554`）は書き換えが要る。前段（`:573-577`、Z で 0→1）が「英字 1 文字」の既存検査である。
- **#927 Escape の防止**: tao の合成 press は egui に渡らなくなるため構造的に起きない。実リピートは `WM_SETFOCUS` の snapshot（Escape が押下中なら入る）が止める。ただし要対処 4・5 の穴は塞ぐこと。
- **GetKeyState のタイミング**: subclass は `DefSubclassProc` より先に実行されるが、`GetKeyState` は取得したメッセージの時点の入力状態を返すので、tao の `update_modifiers`（`event_loop.rs:804-821`、keydown の前に呼ぶ `:930-936`）と同じ値になる。ただし AltGr フィルタの差が要対処 1。
- **tao の `ModifiersChanged` との順序**: tao は keydown 処理の先頭で `ModifiersChanged` を送る（`event_loop.rs:930-936`）が、subclass は tao より先に動くので、同一 keydown の `Event::Key` は `ModifiersChanged` より先に mpsc へ入る。ただし `Event::Key` は自前の `modifiers` を持ち、修飾キー自身の keydown は `Event::Key` を作らないので、egui 側の `InputState::modifiers` との食い違いは見つからなかった。
- **Focused と WM_SETFOCUS/KILLFOCUS の順序**: tao の `Focused` は `WM_NCACTIVATE` と `WM_SETFOCUS`/`WM_KILLFOCUS` のどちらからも出る（`event_loop.rs:1725-1755`）。抑止状態は subclass の `WM_SETFOCUS`/`WM_KILLFOCUS` だけで完結する（`input.rs` の `Focused` arm から消去を外す計画）ので、tao の `Focused` との到着順に依存せず、状態を誤らせる並びは見つからなかった。
- **RefCell と DefSubclassProc の再入**: 借用を Def の前に手放せば `DefSubclassProc → tao → PeekMessageW → sent message → 同じ proc` で二重借用にはならない。要対処 2・3 の補強を条件とする。
- **subclass の両ウィンドウ付与**: results は `focusable(false)`（`src-tauri/src/egui_shell/mod.rs:376`）で `WM_SETFOCUS` を受けない。translator は遊ぶだけで無害。
