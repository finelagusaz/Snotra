# ADR-results-gate-single-point: `results 可視 ⇒ main 可視` は事前ゲート 1 点で守る（事後検査と hide の権威性を置かない）

## 文脈

results 窓は main 窓の下に張り付いて出るため、`results 可視 ⇒ main 可視` を崩してはならない。崩れると results だけが画面に取り残され、main が hidden の間は `RedrawRequested` が配送されず `drive_results_window` が走らないので、拾い直すフレームが来ない。

`EventLoopProof`（#880 サイクル段 2）の導入前、この不変条件は 3 点で守っていた。

1. **事前ゲート** — `present_results` が `main_visible` を読んでから results を撃つ。
2. **事後検査** — 撃った後に `main_visible` を読み直し、失われていれば results を撤回する。
3. **hide の権威性** — 「main が可視でない」ことを理由とする hide は、可視フラグを無視して raw 操作を撃つ。

②と③が要った理由は、ゲートが「読んだ時刻」しか守れなかったことにある。ゲートの読みと raw `ShowWindow` の間には Win32 呼び出しが挟まり、`hide_egui_main` が**別スレッド**（hotkey listener は Win32 メッセージループスレッド上で走る）からその隔たりへ割り込めた。割り込まれると「フラグ = false・窓 = 可視」の食い違いが生まれ、フラグを見る hide は黙って no-op する。②と③はこの食い違いを事後に拾うための装置であり、cross-thread な Win32 呼び出しを lock で囲むと race がデッドロックへ化けるため、`SeqCst` の全順序（撃った**後**に読み直す）だけで封鎖していた。

## 決定

- **可視性を変える操作はイベントループスレッドに閉じる。** `show_egui_main` / `hide_egui_main` / `drive_results_window` / `ResultsWindow::{show, hide}` は `&EventLoopProof`（`!Send + !Sync`・crate 外で構築不能）を要求し、別スレッドからの呼び出しはコンパイルが通らない。
- **これにより「フラグ = false・窓 = 可視」は構築不能になったので、②事後検査と③hide の権威性は撤去した**（#880 サイクル段 2）。残るのは①事前ゲートだけである。
- **②③を再導入してはならない。** それらが必要になるのは証人型を引数から外したときだけであり、そのときの正しい対処は証人を戻すことである。

## 検討した代替案と却下理由

- **②③を安全側の冗長として残す**: 却下。到達しない分岐はテストで落とせず、可視性の遷移を読む者に「別スレッドから割り込める経路がまだ在る」と誤読させる。守る対象が構造で消えた装置を残すと、次に同種の race を疑う者がここを先に見て時間を失う。
- **証人型ではなく lock で囲む**: 却下。窓を所有しないスレッドからの `ShowWindow` は所有スレッドのメッセージポンプ待ちでブロックしうるため、イベントループ側が取る lock で囲むと race がデッドロックへ化ける。この原則は `set_topmost`（`commands/window.rs` のポーリングスレッドから撃つ真の cross-thread 経路）に今も生きている。

## 帰結

- `hide_egui_main` の hide 側同期は別軸として引き続き必要である（`src-tauri/CLAUDE.md`「可視性は『誰が撃つか』だけでは閉じない」・#646 PR2 決定 6）。この ADR が消したのは results 側の事後装置だけである。
- 生の `tauri::Window` 面（`Manager` 経由の `.hide()` / `.show()`）は閉じていない。受容する残余の内訳は `src-tauri/CLAUDE.md` の当該 bullet が正本。
