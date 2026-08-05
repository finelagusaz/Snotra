# ADR-synthetic-key-press-suppression: 合成 press の抑止は入力層に置き、射程を全キー・release までとする

#927（設定ウィンドウの Escape で本体まで hide される）の対処として、`snotra-egui-runtime` の入力層に「focus を獲得した瞬間に押されていたキーは、release されるか focus を失うまで press を egui へ渡さない」を置いた。ここに残すのは、その過程で却下した 4 つの代替案と、理由である。

## 文脈

tao は `WM_SETFOCUS` を受けたとき、その瞬間に押されている全キーの `Pressed` を合成する（`tao-0.35.3/src/platform_impl/windows/keyboard.rs:87-93`）。設定窓が Escape の **down** で閉じ、本体が focus を取り戻した時点でまだ Escape が押されていれば、本体は合成 press を受け取って Escape ラダーを走らせる。

**issue が挙げていた 2 択（(A) オートリピート / (B) 二度押し）はどちらも外れだった。** `keybd_event`（リピートを生まない）で Escape を 1 回だけ注入して再現し、本体が受けた press が `synthetic=true` であることを実測して確定した。down→up が 1ms の対照試行では本体は press を 1 つも受け取らない。

## 決定

- 判定は `input.rs` の純粋関数 `admit_key`（`is_synthetic` / `pressed` / `physical` と抑止集合から bool）
- 抑止の射程は**全キー**。解除は **release** または **`Focused(false)`**
- **非合成の press も抑止中は落とす**——注入では測れない物理オートリピートの経路も同時に塞ぐ

## 検討した代替案と却下理由

### 1. 合成 press だけを落とす（抑止集合を持たない）

`is_synthetic && pressed` を捨てるだけ。状態がゼロで、実測で確定した機序（合成 press）をちょうど塞ぐ最小形である。

**却下したのは、物理キーボードのオートリピートが focus 移行を跨いで新しい前面窓へ届く経路（issue の (A)）を塞げないからである。** そしてその経路の実在は**注入では原理的に測れない**（`keybd_event` はリピートを生成しない。`Z` を 4 秒保持しても `repeat=true` は 0 件だったが、これは (A) の否定にならない）。塞げるのに測れない残余を運用で背負うより、抑止集合 1 つ（`HashSet<KeyCode>` と解除点 2 つ）を持つほうが安い。人間裁定（2026-08-05）。

### 2. view 層で「focus を獲得したフレームの Escape」を落とす

`src-tauri/src/egui_shell/view.rs` の Escape 消費点に `!gained_focus` を足す案。#927 の受け入れ条件の文言（「フォーカス獲得直後の Escape を落とす」）に最も近く、runtime を触らない。

**却下した理由は 2 つ。**（1）**射程が Escape に閉じる**——同じ穴は ↑↓・文字キーにも開いている（`Z` を押しっぱなしで設定を閉じると検索欄へ合成 press が届くことを実測した）。（2）**view 層は合成と本物を弁別できない**——`is_synthetic` は tao の `WindowEvent` にしかなく、egui の `InputState` まで来ると消える。ゆえに「そのフレームの Escape を一律に落とす」ヒューリスティックになり、本物の Escape も巻き添えにする条件を自分で作ることになる。

### 3. 設定ウィンドウの「Escape で閉じる」を廃止する

引き金そのものを消す案。モーダルダイアログと違い、独立したアプリ窓が Escape で閉じるのは確かに珍しい（Chrome・VS Code・Slack の設定はいずれも閉じない）。`SPEC.md` にも明文化されておらず、実装だけが持つ挙動である。

**却下ではなく分離した**（人間裁定・2026-08-05）。理由は 3 つ。（1）**機序が残る**——将来また別の窓が同じ形を踏む。（2）**↑↓・文字キーの混入は消えない**（却下 2 と同じ）。（3）**先に廃止すると、この修正を実機で検証する再現手順が消える**——`repro-927.ps1` は設定窓が Escape で閉じることに乗っている。是非は別 issue で製品判断として決める。

### 4. `Focused(true)` でも抑止集合を消す（focus セッションごとに完全リセット）

対称に見えるうえ、「集合は各 focus セッションから導かれる」と言い切れて読みやすい。

**却下したのはイベント順序で自壊するからである。** tao は合成 key event を `public_window_callback_inner` の `keyboard_callback`（`event_loop.rs:967-993`）で送り、`Focused(true)` はその**後**の `match msg` → `gain_active_focus`（同 `:870-878`・`:1792`）で送る。**合成 press は `Focused(true)` より先に届く**ので、`Focused(true)` で消すと直前に立てた抑止を自分で捨てる——修正が丸ごと no-op になる。消去点は `Focused(false)` 側だけが正しい。
