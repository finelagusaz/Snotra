# plan-review: #1266（tauri 2.12 移行）D2 の正しさ / 規範文書の網羅性

対象 issue: #1266。観点は 2 つだけ（D2、文書網羅性）。編集なし・読むだけ。

## 要対処

1. **Phase 3 の人間スモークは新経路を観測できない**。`ime_subclass_proc` の trace（`windows_ime.rs:209-219`）は `Preedit` の chars しか出さない。計画は「`push_text` 0 件・preedit の chars」で照合すると書くが（plan.md:73）、確定が subclass 経由で 1 回入ったことを示す行が無い。`push_text` は `ReceivedImeText` arm（`input.rs:307-318`）専用で、新経路では 0 件になるのが正常。入力欄の文字列しか証拠が無く、「二重確定していない」「落ちていない」を件数で照合できない。subclass の Commit 送信に `SNOTRA_EGUI_IME_TRACE` の行（chars 数のみ・文字内容は出さない）を足す。足すなら `PERFORMANCE.md:2723`（`SNOTRA_EGUI_IME_TRACE` は「preedit 取得と候補ウィンドウ位置」と説明）も更新対象になる。
2. **変更ファイル表に `runtime.rs:228` が無い**。`TaoWindowEvent::ReceivedImeText(text) => input_trace("rx_text", …)`（`runtime.rs:228-231`）。表の `runtime.rs` は `:517` のみ。D2 が効けば Windows では `ReceivedImeText` が発火しない（tao 0.37.1 で送出元は `event_loop.rs:1194-1201` の `WM_IME_ENDCOMPOSITION` だけ）ので、この arm は `input.rs:307` と同じ tripwire になる。計画が `input.rs` の arm を「残す」と判断した以上、ここも同じ扱いを書くか、両方消すかを決める。`rx_text` は計画のスモーク手順に出てこないので、人間スモークの確認項目に `rx_text` / `push_text` 両方 0 件を入れる。
3. **D2 の実装形が既存コードの構造と合わない点を計画が拾っていない**。`ime_subclass_proc` は 1 メッセージにつき `Option<ImeEvent>` を 1 つだけ送る形（`windows_ime.rs:200-227`）。`GCS_RESULTSTR | GCS_COMPSTR` が同時に立つと Commit と Preedit の 2 つを、この順で送る必要がある。egui の `TextEdit` は Commit が preedit 範囲を消して挿入し（`egui-0.36.1/.../builder.rs:1308-1316`）、後続の Preedit が新しい範囲を挿入する（`:1289-1306`）ので、Commit → Preedit の順が正しい。逆順だと確定分が preedit として扱われうる。送信順を決める純関数（メッセージ・lparam・読み出し結果 → 送る `ImeEvent` の列）を切り出し、「Commit が先」を単体テストで固定する。計画の単体テストは `classify_ime_message` だけで、この順序が未固定。
4. **`src-tauri/CLAUDE.md:142` が表に無い**。「`app.listen` のコールバックは…（tauri 2.11.4 の `event/listener.rs::emit_filter` が…・実測）」。2.12.1 で挙動に依存する版根拠で、計画の 4 項（Win32 メッセージ配送・setup フック・宣言的なウィンドウ属性・raw へ寄せる）に入っていない。`git grep -n "tauri 2\.11"` で出る。Phase 4 の語彙検査（plan.md:82）は `0.35.3` / `tao-0.35` / `tao 0.35` / `ReceivedImeText` / `minimal_ime` しか grep しないので、`tauri 2.11` の散文はこの検査にも映らない。検査語に `2\.11` を足す。

## 軽微

1. `snotra-egui-runtime/CLAUDE.md:11` のモジュール構成「`windows_ime.rs`: IMM32 preedit取得、候補ウィンドウ位置、subclassの所有/破棄」が、確定読み出しを持つ後は不完全になる。表の CLAUDE.md 行は「一般」の IME 2 項目と focus 項だけを挙げている。
2. 確定の読み出しは tao 側と違い、`committed_text_event`（`input.rs:430-433`）の制御文字フィルタを通らない。subclass から `ImeEvent::Commit` を直接送るので、`\t` や `\x7f` を含む確定は素通りする。実 IME の確定でこれらが出る可能性は低いので受容でよいが、「旧経路と同じく制御文字は弾く」（受入 2 は通常文字の話）と読まれないよう不変条件節に一言書く。egui は `"\n"` / `"\r"` の Commit を自分で無視する（`builder.rs:1284-1288`）。
3. 順序保証の説明（plan.md:33, :117）は「確定は後続のキーイベントより先に積まれる」と言うが、この経路は tao のイベント列とは別の `mpsc` なので、全順序は保証されない。通常は subclass が呼ばれる時点とイベント配送が同期しているので問題ないが、tao の `event_loop_runner` がイベントを再入中にバッファした場合（モーダルループ中など）、バッファされた先行キーより先に Commit が積まれうる。旧経路（`ReceivedImeText` は tao のイベント列そのもの）にあった保証を 1 つ手放す。通常打鍵では発生しないので「受容残余」として `snotra-egui-runtime/CLAUDE.md` に 1 行書けば足りる。
4. `ImeContext::composition_string` は `GCS_COMPSTR` 固定（`windows_ime.rs:262-274`）。計画の「同じ UTF-16 復号を共有する」は、種別を引数に取る形への変更を意味する。`read_preedit` は `composition_string()?` が `None` のとき preedit を送らない現状の倒し方を保つ必要がある。
5. 空 `Commit`（`GCS_RESULTSTR` が立ったが長さ 0）は送らない、と計画にあるのは正しい。egui 側も、composition 中でなければ空 Commit を無視する（`builder.rs:1274-1279`）ので害は無い。
6. `input.rs:307` の `ReceivedImeText` arm と `runtime.rs:228` の arm は、D2 が効けば Windows では死んだコードになる。「残す」判断は tripwire として成立するが、`rx_text` / `push_text` は人間スモーク以外で誰も見ない。残すなら doc に「Windows では発火しない見込み・発火したら D2 の抑止が破れている」と書く。消すなら両方。

## 未検証

1. ⚠️ `WM_IME_ENDCOMPOSITION` を `DefWindowProc` に通さない影響。tao 0.37.1 でこのメッセージを触るのは `event_loop.rs:1194-1202` の 1 箇所だけで、その arm は `ReceivedImeText` を送った後 `result` を既定の `ProcResult::DefWindowProc`（`event_loop.rs:928`）のままにする。`WM_IME_*` の他の処理は tao 内に無い（`grep WM_IME` は `:1194` のみ。`ime.rs` の `set_ime_allowed` 等はコメントアウト）。tao の他の処理が止まる心配は無い。残る影響は `DefWindowProc` が default IME window へ ENDCOMPOSITION を転送しないことだけで、`WM_IME_STARTCOMPOSITION` を既に抑止している（default IME のコンポジションウィンドウは一度も開かない）ので実害は見込みにくい。ただし TSF 互換層で候補ウィンドウの後始末が `ENDCOMPOSITION` の `DefWindowProc` に依存していないかは実機未確認。人間スモークの (a)(c) で「候補ウィンドウが残らない」を見る項目を足す。
2. ⚠️ `WM_IME_COMPOSITION` を `DefSubclassProc` に通さないと、`DefWindowProc` が `WM_IME_CHAR` を作らない。これは意図通りで、Win32 の文書にある「アプリが `WM_IME_COMPOSITION` を自分で処理するなら `DefWindowProc` へ渡さない」と整合する。ただし TSF 経由の IME（MS-IME 新版・Google 日本語入力）で、`GCS_RESULTSTR` だけを持つ `WM_IME_COMPOSITION` が必ず来るか（`GCS_RESULTSTR` を立てずに `WM_IME_CHAR` / `WM_CHAR` だけで確定を送る IME が無いか）は、このリポジトリでは実機未測。旧経路は `WM_CHAR` 経由でも拾えたので、IME によっては確定が落ちる回帰がありうる。人間スモークで MS-IME と少なくとももう 1 種を試す項目を足す。
3. ⚠️ VK_PACKET（音声入力・OSK・Win+. 絵文字パネル・Win+V 貼り付け）。`event_info` の無い `WM_CHAR` は tao 0.37.1 が捨てる（`keyboard.rs:188-196`）。`adversarial-1266.txt:6` が既に指摘済みで、計画の受入に含まれていない。D1 の範囲外だが、旧経路（`minimal_ime`）で入っていた入力が入らなくなる回帰として、受容か修正かを書いておく。
4. ⚠️ `GCS_RESULTSTR` が複数の `WM_IME_COMPOSITION` にまたがって累積するか（それとも毎回その時の分だけか）は、このリポジトリでは実機未測。累積するなら部分確定（スモーク (b)）で二重入力になる。(b) で入力欄の文字列を厳密に照合する。
5. ⚠️ 後続イベントが無いときの回収。`InvalidateRect`（`windows_ime.rs:225`）→ `WM_PAINT` → `RedrawRequested`（tao 0.37.1 `event_loop.rs:1025-1036`）→ `render()` の `drain_native_ime()`（`runtime.rs:431`）という経路は、コード上は成立する。ただし `render()` は `!self.visible` のとき drain 前に return する（`runtime.rs:419-430`）。本体ウィンドウは IME 入力中は可視なので問題ないが、`visible` が `Focused(true)` でしか復帰しない（`runtime.rs:408-414`）ことは前提として記録しておく。
6. 未測のまま確認できたこと（ここまでで読んだ範囲では正しい）: `on_window_event` の先頭で `drain_native_ime()` が走る（`runtime.rs:401`）。`WM_IME_COMPOSITION` を tao が処理する箇所は無い。tao の `KeyEventBuilder` は `VK_PROCESSKEY` の `WM_KEYDOWN` で `next_kbd_msg` が `WM_KEYFIRST..WM_KEYLAST` しか覗かないので IME メッセージは拾わない。`text_with_all_modifiers` は `utf16parts` そのもの（`keyboard.rs:622-673`）、`text` は Ctrl を外した文字（`:264-274`）で、D1 の主張は一致。
