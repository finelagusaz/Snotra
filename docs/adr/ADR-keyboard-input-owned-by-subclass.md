# ADR-keyboard-input-owned-by-subclass: キーボード入力の意味づけは tao に任せず、IME subclass が Win32 メッセージから作る

## 文脈

#1265 で tauri 2.12.1（tao 0.35.3 → 0.37.1）へ上げたとき、入力経路が 3 か所壊れた。どれもコンパイルは通り、実行時に黙って壊れる種類だった。当時の入力は持ち主が 2 つに分かれており、IME の 3 メッセージは `windows_ime.rs` の subclass、通常の打鍵と文字は tao の `KeyEvent` が運んでいた。二重入力が起きないことは、tao の**振る舞い**（IME 由来の `WM_CHAR` を `KeyEvent` にしない、`ReceivedImeText` を `WM_IME_ENDCOMPOSITION` でだけ送る）を前提にしていた（#1289）。

## 決定

キーボードのメッセージ（`WM_KEYDOWN` / `WM_KEYUP` / `WM_SYSKEYDOWN` / `WM_SYSKEYUP` / `WM_CHAR` / `WM_SYSCHAR`）と `WM_SETFOCUS` / `WM_KILLFOCUS` も subclass が見て、egui の入力を作る。メッセージは消費せず `DefSubclassProc` へ流す。tao から受け取るのは、ウィンドウ・イベントループ・ポインタ・focus・修飾の保持だけにする。

## 検討した代替案と却下理由

- **tao ごと外して自前の Win32 ループにする**: 却下。依存は消えるが、updater・トレイ・単一インスタンスなど Tauri が持つ機能を抱えることになり、規模が見合わない。
- **現行の分担のまま、tao の振る舞いを検査で固定する**: 却下。壊れたことには気づけるが、壊れること自体は防げない。tao を上げるたびに #1265 と同じ規模の修理が来る構造が残る。
- **キーイベントに載せる修飾を subclass の `GetKeyState` で取る**: 却下。tao の `get_agnostic_mods` は AltGr（`has_alt_graph && VK_RMENU`）のとき Ctrl と Alt を外す。`GetKeyState` で取ると AltGr+C/X/V が Copy/Cut/Paste になる。日本語配列には AltGr が無く、手打ちでは見つからない（計画レビューで見つかった）。修飾は tao の `ModifiersChanged` の保持値を drain の時点で載せる。クリップボードの読みも同じ場所に残るので、subclass の `RefCell` の借用の中で Win32 を呼ばずに済む。
- **focus 獲得時の抑止（#927）の鍵を VK にする**: 却下。変換中の打鍵は `WM_KEYDOWN` の VK が `VK_PROCESSKEY` になるので、focus 獲得時に取り込んだ VK と照合できない。旧実装も物理キー（tao の `KeyCode`）を鍵にしていた。鍵はスキャンコードにした。
- **抑止を「lParam bit 30（直前も押下）の down を、この focus で down を見ていないキーなら落とす」で行う**: 却下。`WM_SETFOCUS` の瞬間に押下中のキーを取り込む形のほうが、旧実装が受けていた tao の合成 press と同じ集合になる。`scripts/smoke-egui.ps1` の #1274 の節は「取り込みが起きなければ 2 回目の down が通って赤に倒れる」という判別を持っており、bit 30 の判定へ替えると、その節が何を判別しているかが変わる。
- **CI の runner に `Install-Language ja-JP` を入れて、IME の確定を smoke で打つ**: 却下（ユーザーの判断）。runner で動くかは未検証で、CI 時間が増え、IME の状態に依存する不安定さを抱える。CI では `SendInput` の `KEYEVENTF_UNICODE`（非 ASCII とサロゲートペア）までを打ち、IME の確定が 1 回だけ入ることは手打ちで確かめた。

## 帰結

- 変換中の英字キー（`VK_PROCESSKEY` のときの物理 A–Z）と Alt+英字の文字（`WM_SYSCHAR`）は、挙動を変えずに旧実装と同じに保った。挙動を変える判断はこの決定の範囲外である。切り替えの前に、旧経路と新経路を並走させて同じ egui イベント列になることを、注入の打鍵（キー 44 件・文字 8 件）と日本語 IME の手打ち（キー 181 件・文字 3 件）で確かめた
- 直前のキー押下を持たない `WM_CHAR`（`PostMessage` で他のアプリが送る文字・#1269 の残余）が入るようになった。`SendInput` の Unicode のサロゲートペアも、キーのメッセージを跨いで連結するので入る。どちらも smoke が固定する
- tao に残る依存は、そのループが `TranslateMessage` を呼ぶことだけである。不変条件の正本は `snotra-egui-runtime/CLAUDE.md`
- AZERTY のように数字が Shift 側にある配列では、無修飾の数字キーが `Num*` の `Event::Key` になり、旧実装（tao の logical）と違う。src-tauri も TextEdit も数字キーの `Event::Key` を読まないので受け入れた
