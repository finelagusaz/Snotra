# ADR-ime-commit-owned-by-subclass: IME の確定は tao に任せず、IME subclass が `GCS_RESULTSTR` から読む

## 文脈

tauri 2.12.1 へ上げると tao が 0.35.3 → 0.37.1 になり、`WM_CHAR` を `ReceivedImeText` へ変えていた層（`minimal_ime`）が消えた（#1266）。0.37.1 で `ReceivedImeText` が出るのは `WM_IME_ENDCOMPOSITION` を受けて `GCS_RESULTSTR` を読み直したときだけである。通常文字は `KeyEvent` から取ることにしたので、IME の確定をどこから取るかを決める必要があった。

## 決定

`snotra-egui-runtime/src/windows_ime.rs` の subclass が、IME の 3 メッセージ（`STARTCOMPOSITION` / `COMPOSITION` / `ENDCOMPOSITION`）を確定を含めてすべて持ち、tao にも `DefWindowProc` にも通さない。確定は `WM_IME_COMPOSITION` に `GCS_RESULTSTR` が立っていれば自分で読んで送る。

## 検討した代替案と却下理由

- **tao 0.37 の `ENDCOMPOSITION` → `ReceivedImeText` に任せる（subclass は確定を通すだけ）**: 却下。懸念が 3 つあり、いずれも「確定を tao と `DefWindowProc` へ通す」ことから生じる。(1) 変換中に続けて打鍵して前の文節が確定するとき、確定は `ENDCOMPOSITION` を伴わない `WM_IME_COMPOSITION` で来るので落ちる。(2) 取り消しでも `ENDCOMPOSITION` は来るので、`GCS_RESULTSTR` が前回の確定を保持していれば二重確定になる。(3) `DefWindowProc` が確定から作る `WM_IME_CHAR` → `WM_CHAR` が、再入中の通常キーの `KeyEvent` に連結されうる。自前で読めば 3 つとも経路ごと消える。実機で (1)〜(3) のどれが実在するかは測っていない——測る費用より経路を消す費用のほうが小さかった。
- **確定は自前で読み、`ENDCOMPOSITION` だけ tao へ通す**: 却下。tao 0.37 がそこで `ReceivedImeText` を送るので二重確定になる。tao が `ENDCOMPOSITION` で行うのはこの送出だけで（`event_loop.rs`）、通さなくても失うものが無い。
- **通常文字も IME 確定も `KeyEvent.text` から取る**: 却下。`event_info` の無い IME 由来の `WM_CHAR` は tao 0.37 が `KeyEvent` にしないので、確定はそもそも `KeyEvent` に乗らない。

## 帰結

- 確定と通常文字はそれぞれ 1 経路だけになった（不変条件の正本は `snotra-egui-runtime/CLAUDE.md`）。`ReceivedImeText` の arm は tripwire として残したが、出るのは `SNOTRA_EGUI_INPUT_TRACE` を立てたときだけで、自動の検査は見ていない
- 確定（`mpsc`）と tao のイベント列は別の列になり、全順序ではない（受容する残余・同 `CLAUDE.md`）
- 2026-10-09 に日本語 IME 1 種で実打鍵を確かめた（確定・部分確定・取り消し・Ctrl 併用）。TSF 系の別 IME が `GCS_RESULTSTR` を立てずに確定する可能性は未測
