# 調査: issue #1470

## issue の要約

Alt を含むホットキーで表示するとき、Alt を離してから Snotra が前面になるまでの間に打鍵が入ると、前面だったアプリのメニューが起動する（`SC_KEYMENU`・メニューループ。メニュー中に文字が落ちれば警告音）。起動直後の最初の表示で起きやすい。

機序（2026-11-17 に計測で特定。証拠は issue 本文）:

1. `show_egui_main` が呼ぶ `window.set_focus()` は、tao 0.37.2 の `Window::set_focus` → `force_window_active`。`SetForegroundWindow` が失敗すると、素の左 Alt の down/up（`KEYEVENTF_EXTENDEDKEY`、マスクなし）を `SendInput` で注入して再試行する
2. 注入された Alt は、まだ前面の旧アプリへ届き、素の Alt としてメニューを起動する
3. `SetForegroundWindow` が失敗するのは、Alt 解放の後に打鍵が旧アプリへ落ちたとき（文字なし・15 ms 以降の文字では失敗しない）
4. 初回は `show()` の後も前面になっておらず、`set_focus` まで届くのが遅い（Alt 解放から `egui_show:done` まで 22〜25 ms。2 回目以降 15〜17 ms）。2 回目以降は `show()` の時点で前面になっていて、`set_focus` は何もしない

バグ修正である。`SPEC.md` は表示時の前面化の機構を書いていない（`grep -n "前面\|フォーカス\|Alt" SPEC.md` で確認。§12 の IME オフ・§8.5 の results のフォーカス不取得は無関係）。

## 関連するファイル・シンボル

- `src-tauri/src/egui_shell/window_coordinator.rs` の `show_egui_main` — `window.show()` → `window.set_focus()` → `send_alt_key_up()`（Alt 非押下時）→ `TurnOffIme` 送信 → `egui_show:done`。`set_focus()` の直後に、フォーカス同期待ちを置かない理由を述べる長いコメントがある
- `src-tauri/src/main.rs` の `send_alt_key_up` / `make_key_input` / `is_alt_pressed` — `send_alt_key_up` が MenuMaskKey 技法（vkE8 を Alt up の前に挟む）を既に持つ。`VK_MASK` は関数内のローカル定数
- `src-tauri/src/main.rs` の `setup_hotkey_listener` — `plan_hotkey` が `ShowAfterAltRelease` を返すと、別スレッドで `wait_alt_release_or_timeout` してから `show_egui_main` へ再入する
- tao 0.37.2 `src/platform_impl/windows/window.rs` の `set_focus`（139 行）と `force_window_active`（1514 行）— `is_visible && !is_minimized && !is_foreground` のときだけ `force_window_active` を呼ぶ（`Cargo.lock` で tao 0.37.2 を確認）
- `src-tauri/src/platform/tray.rs` — `SetForegroundWindow` を直接呼んでいる既存の箇所。platform スレッド内の実装なので、イベントループ上で直接呼ぶ根拠にはならない（反証所見で訂正）
- Snotra が呼ぶのは tauri の `Window::set_focus` で、tauri-runtime-wry はメインスレッドならそのまま tao の `set_focus` を実行する（反証所見で追加）

## 再利用できる既存のパターン

- `send_alt_key_up` の MenuMaskKey（vkE8）と `make_key_input`。`send_alt_key_up` 自体は Alt down を送らず末尾で 5 ms 眠るので再利用できない。マスクした Alt 注入は `make_key_input` とマスクキーの定数で新しく組む
- 試作（`EXP_FG=1` で効く版・共有ノートの `experiment-masked-alt.diff`）: `GetForegroundWindow() == hwnd || SetForegroundWindow(hwnd)` なら終わり、失敗したら `[Alt down, vkE8 down, vkE8 up, Alt up]` を `SendInput` して `SetForegroundWindow` を再試行。release でビルドが通り、d = 1, 3, 6, 10 ms の 1 回目で tao の注入・`SC_KEYMENU`・メニューループが 8/8 で 0、マスクした Alt の後の再試行は 8/8 で成功
- ソーステキスト検査: `launcher_controller/activation/tests.rs` が `launcher_controller/` 直下の子のソースを読む先例

## 技術的な制約

- `show_egui_main` はイベントループスレッドで走る（`EventLoopProof`）。`PlatformBridge` は channel で platform スレッドへ渡すので非同期であり、`set_focus` の位置で前面化を同期的に済ませる用途には使えない。`.claude/rules/src-tauri.md` の「Win32 は `PlatformBridge` を既定にする」の例外になる（既存の `send_alt_key_up` も直接呼び出し）
- `SetForegroundWindow` の許可条件は Win32 の規則で決まり、Snotra から制御できない。Alt の押下で許可されることは `AllowSetForegroundWindow` の文書にある。打鍵で取り消されることは対照から読んだ（文書ではない）
- Win32 の経路は `cargo test` で検証できない（`.claude/rules/src-tauri.md`）。検証は実機の注入ハーネス（リポジトリ外の一時の足場）と `smoke:egui` で行う
- 非 windows ビルドでは `set_focus()` を残す必要がある（`show_egui_main` は `#[cfg(windows)]` で分岐している箇所がある）

## 未解決の疑問

- ~~tao の `set_focus()` を残すか置き換えるか~~ → 置き換える（下の裁定）
- マスクした Alt の down で、Word のキーヒントのような「Alt の押下で出る表示」が残らないか（実在のアプリでは見ていない）
- 初回の遅さ（`show()` で前面にならない理由）は、この issue では扱わない（issue 本文「修正しても残るもの」）

## 反証調査（`workspace/adversarial-1470.txt`）の裁定

壊せなかった: 争点 1（tao の分岐と版）・2（イベントループ上・同期待ちなし・bridge は非同期）・4（SPEC 変更なし。grep のみ）・6（シンボルと行の実在）。

採った所見:

- 争点 3: `send_alt_key_up` はそのまま再利用できない → 上の「再利用できる既存のパターン」を訂正した
- 争点 5: 置き換えれば、自前の再試行が失敗しても素の Alt へは落ちない。前段に置く案だと tao の注入経路が残る → **置き換えを採る**（未解決の疑問の 1 つ目を裁定）。tao の「可視・非最小化」の条件は、直前の `show()` が可視を作り、`decorations(false)`・`skip_taskbar(true)` の main は最小化の経路を持たないので写さない。前面かどうかの判定は写す
- ⚠️ `ShowNow` では Ctrl/Shift が押されたまま show に入りうる。そこへ Alt を注入すると、Alt+Shift の入力言語切替などを踏みうる（今の tao の素の Alt はより危ない）→ 所見（危険の指摘）は採り、実測で処置を決めた。Shift を押したまま前面化を失敗させると、マスクした Alt の注入は 3/3 で前面化に成功し、メニュー起動も入力言語切替要求も 0 だった。省くと前面にならない。**注入を省かない**（裁定の詳細は `workspace/plan.md` の未確定欄）
- `tray.rs` は例外の根拠にならない → 関連ファイルの記述を訂正した
- tauri の層の記述漏れ → 追記した

採らなかった所見:

- ⚠️ `make_key_input` は wScan と `KEYEVENTF_EXTENDEDKEY` を付けないので、その形で効くかは未確認 → 試作は `make_key_input` そのままで組み、8/8 で再試行が成功した（一次証拠: 共有ノートの `experiment-masked-alt.diff` と、issue 本文の試作の結果）
- ⚠️ 「打鍵で `SetForegroundWindow` が失敗する」は処理時間と交絡している → 文字なしの対照は、初回の同じ遅い処理時間（22〜25 ms）のまま 0/4 で失敗しなかった。処理時間を固定して打鍵だけを外した対照なので、交絡は残っていない。d = 15 ms で失敗しないのも、文字が `set_focus`（約 +11 ms）より後に届くことと合う
- ⚠️ single-instance・startup・初回ホットキー失敗の経路は測っていない → 受け入れる。どの経路でも、置き換えで変わるのは「前面化に失敗したときの注入が素の Alt からマスクした Alt（または注入なし）になる」ことだけで、成功する経路の挙動は同じ（`SetForegroundWindow` の呼び出し）
- ⚠️ Electron・UWP・Word でマスクした Alt の down がどう見えるか → 未検証のまま PR 本文に残す（未解決の疑問の 2 つ目と同じ）
