# ADR-main-visibility-live-read: main の可視性はキャッシュせず、イベントループ上で窓の実状態を読む

日付: 2026-11-11 ／ 状態: 承認

## 文脈

main ウィンドウが可視かのキャッシュ（`AtomicBool`）は #156（2026-03-07）で入った。WebView2 期、platform スレッド上の hotkey listener が `is_visible()` を呼ぶと、cold で 61ms / 39ms かかったためである（`PERFORMANCE.md`「ホットキー表示レイテンシ」）。#1338（PR #1364）でキャッシュは `EguiShellState` の `MainVisible` 型へ移り、書き口はモジュールの中に閉じた。それでも、キャッシュを保つための順序の規範（show の**後**に true を立てる・results の hide の**前**に false へ落とす）は残っていた。キャッシュは、`show_egui_main` / `hide_egui_main` 以外による可視性の変化を反映しない。

2026-08-02 の設計書（`docs/superpowers/specs/2026-08-02-results-visibility-single-thread-design.md` §9-3）は、キャッシュを廃止して実状態を真実源にする案を 2 つの理由で却下していた。

1. tauri の `is_visible()` を別スレッドから呼ぶのは、blocking 往復と同じ危険クラスである
2. 生の `IsWindowVisible` は、tao の hide が着弾するまで stale な TRUE を返す。ゲートが要るのは「意図」であって、ラグ付きの実状態ではない

#880 / #881 で、キャッシュの読み手（hotkey の判定・`drive_results_window` の連言①）も書き手（`show_egui_main` / `hide_egui_main`）も、イベントループスレッドの上に移った。

## 決定

**キャッシュを撤去し、`window_coordinator::read_main_visible(app, el)` がイベントループ上で `Window::is_visible()` を読む。** この関数は `&EventLoopProof` を要求する。

§9-3 の 2 つの理由は、どちらも #881 で失効していた（tao 0.37.2 / tauri-runtime-wry 2.12.3 のソースで確かめた）。

1. 別スレッドからの読みはもう無い。tauri-runtime-wry の `Context::send_user_message` は、呼び出しがイベントループスレッド（`main_thread_id`）の上ならその場で `handle_user_message` を実行し、post しない。`is_visible()` は tao の `IsWindowVisible` に落ちる
2. hide の着弾ラグは無い。tao の `set_visible` は `execute_in_thread` を通る。これは、イベントループスレッドの上ならその場で実行する（そうでなければ `PostMessageW`）。`hide_egui_main` はイベントループの上にあるので、`window.hide()` が返った時点で `WS_VISIBLE` は落ちている

所要はイベントループ上で中央値 9.5 µs・初回 45.6 µs だった（`PERFORMANCE.md`「main の可視性の読み」）。

## 却下した代替案

- **キャッシュを残す**: 却下。残っていたのは、写しを保つための 2 つの順序制約だけである。往復を避けるという存在理由は消えていた。写しは、Manager 経由の `.hide()` のように 2 関数を通らない変化を反映しない
- **生の `IsWindowVisible` を直接呼ぶ**: 却下。`.claude/rules/src-tauri.md` の「Win32 を呼ぶ経路は `PlatformBridge` を通す」に当たる。従うと往復が生じる——`PlatformBridge` は別スレッド（platform スレッド）の Win32 メッセージループへ channel でコマンドを送る（`src-tauri/src/platform/mod.rs`）。tauri の getter がイベントループ上でインラインになるので、直接呼ぶ理由が無い
- **証人を取らない読みの関数**: 却下。別スレッドから呼べてしまえば、§9-3 の理由 1（往復）が戻る。証人を要求すれば、その呼び出しはコンパイルが通らない

## 旧 ADR との関係（凍結ゆえ直さず、ここへ書く）

- **`ADR-results-gate-single-point`** の「事前ゲート — `present_results` が `main_visible` を読んでから results を撃つ」: ゲートは残る。読む値がキャッシュから窓の実状態に替わった。同 ADR の決定（事後検査と hide の権威性を再導入しない）は今も真である
- **`ADR-appstate-config-visibility`** の案 3 の却下理由（`main_visible` は `pub` ゆえ crate 内のどこにでも `store()` を書ける）: `main_visible` はもう存在しない。却下の結論（`AppState` の全フィールドを private にしない）は、この ADR と関係しない

凍結された ADR は直さない（`ADR-adr-frozen-history`）。

## 帰結

- 写しの順序の規範（show の後に立てる・results の hide の前に落とす）は消えた。`hide_egui_main` の中で main を results より先に隠す並びは #671 からのもので、ゲートの正しさはそれに依存しない（同関数のコメント）
- Manager 経由の `.hide()` で results が最前面に取り残される残余は、閉じていない。main が hidden の間は `RedrawRequested` が届かず、`drive_results_window` が走らないためである（`hide_egui_main` の doc）
- hidden 中に稀に走るフレームで results を出さないこと（連言①の読みそのもの）には、自動検査が無い
