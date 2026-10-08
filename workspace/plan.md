# 計画: #1266 tauri 2.12 へ移行する

調査は `workspace/research.md`、敵対的調査は `workspace/adversarial-1266.txt`。

## 目的と受け入れ条件

tauri / tauri-runtime / tauri-runtime-wry `=2.12.1`（tao 0.37.1）の上で、2.11.5（tao 0.35.3）と同じ入力・表示の挙動を保つ。

- [受入 1] `scripts/smoke-egui.ps1 -ExePath target/release/snotra.exe` がローカルで緑（`egui_results:show` 観測・`bar_rect_mismatch` 0 件）。CI の smoke-egui も緑
- [受入 2] 通常文字は 1 打鍵 1 文字で入る。Ctrl+A などの Ctrl 併用は文字を入れない（旧経路と同じく制御文字として弾く）。`SendInput` の `KEYEVENTF_UNICODE`（`VK_PACKET`・OSK / 絵文字パネル / 音声入力が使う形）で送った文字も 1 文字で入る
- [受入 3] IME（日本語）で、未確定は egui 描画・ネイティブ変換ウィンドウは出ない（#532 の不変条件を保つ）。確定は 1 回だけ入る。部分確定（変換中に続けて打鍵）でも確定分が落ちない。Escape で取り消したとき何も入らない——**人間の実打鍵で確かめる**
- [受入 4] 起動後の初回 show で、main / results とも `outer − inner` が 2 回目以降と同じ（stale な 39 を読まない）
- [受入 5] 規範文書とコードコメントのうち、tao 0.35.3 / tauri 2.11.x を根拠にして**挙動に依存している**記述が 0.37.1 / 2.12.1 の根拠へ置き換わっている（`docs/superpowers/` と `docs/adr/` は凍結された歴史ゆえ触らない）

## 設計判断

### D1. 通常文字は `KeyEvent::text_with_all_modifiers()` から取る

0.37.1 で通常文字が乗るのは `KeyEvent` だけである。`KeyEvent.text` ではなく `text_with_all_modifiers()` を使う——後者は `WM_CHAR` / `WM_SYSCHAR` の中身そのもので、旧 `minimal_ime` が `ReceivedImeText` で送っていた文字列と一致する（Ctrl+A は `"\x01"` → 既存の `committed_text_event` が制御文字として弾く。`text` は Ctrl を外した `"a"` を返すため Ctrl+A で `a` が入る）。旧経路との差は 2 つで、どちらも受け入れる（research.md「採否」争点 2）: サロゲートペアが 1 文字として入る（旧経路は捨てていた）、focus 獲得時に押下中だったキーの文字が release まで入らない（`admit_key` の意図と同じ向き）。

**配送は `admit_key` の後、`on_keyboard_event` の内側に置く**——合成 press も `text_with_all_modifiers = Some(..)` を持つため、外に置くと focus 復帰で誤入力になる。

egui へは今までどおり `Event::Ime(ImeEvent::Commit)` として渡す（下流の受け取り方を変えない）。

### D2. IME 確定は `windows_ime.rs` が自分で読み、tao へ通さない

tao 0.37.1 の確定経路（`WM_IME_ENDCOMPOSITION` で `GCS_RESULTSTR` を読む）には懸念が 3 つある（research.md「0.37 で IME 確定を tao の経路に任せたときの懸念」）: 部分確定を落とす・取り消し時に古い確定を読む・`DefWindowProc` が生む `WM_IME_CHAR` → `WM_CHAR` が `KeyEvent.text` へ混ざる。いずれも IME 確定を tao と `DefWindowProc` へ通すことから生じる。

ゆえに subclass が IME メッセージを全部持つ:

- `WM_IME_COMPOSITION` に `GCS_RESULTSTR` が立っていれば、`ImmGetCompositionStringW(GCS_RESULTSTR)` を読んで `ImeEvent::Commit` を既存の `mpsc` へ送る（空は送らない）。続けて `GCS_COMPSTR` があれば従来どおり preedit を送る。**`DefSubclassProc` を呼ばず `LRESULT(0)`**——`WM_IME_CHAR` を作らせない
- `WM_IME_ENDCOMPOSITION` は従来どおり空の preedit を送り、**tao へ通さない**（tao がそこで `ReceivedImeText` を送ると二重確定になる）。`WM_IME_STARTCOMPOSITION` を既に抑止しているので、既定 IME UI の後始末も要らない
- 順序: `runtime.rs` の `on_window_event` は各ウィンドウイベントの処理前に `drain_native_ime()` を呼ぶので、確定は後続のキーイベントより先に egui へ積まれる。後続イベントが無いときは subclass の `InvalidateRect` → `RedrawRequested` で回収される（preedit と同じ経路）

- `GCS_RESULTSTR | GCS_COMPSTR` が同時に立つときは **Commit → Preedit の順**で送る（egui は Commit で preedit 範囲を消してから挿入し、後続の Preedit が新しい範囲を張る——`egui-0.36.1` の `text_edit/builder.rs` の `ImeEvent` 処理）。`ime_subclass_proc` は 1 メッセージ 1 イベントの形なので、**メッセージから送るイベント列を導く純関数**を切り出し、順序を単体テストで固定する
- 確定の送信にも `SNOTRA_EGUI_IME_TRACE` の行（`SNOTRA_EGUI_IME_COMMIT chars=N`・文字列そのものは出さない）を足す——スモークで「確定が 1 回入った」を件数で照合する観測点が他に無い
- subclass の Commit は `committed_text_event`（制御文字フィルタ）を通らない。IME の確定に制御文字は来ない前提で受け入れる（egui も `"\n"` / `"\r"` の Commit を無視する）

`input.rs` の `WindowEvent::ReceivedImeText` の arm と、`runtime.rs` の `rx_text` trace の arm は残す——D2 が効いていれば Windows では発火しない **tripwire** として扱い、両方の doc に「発火したら D2 の抑止が破れている」と書く。人間スモークで `rx_text` / `push_text` が 0 件であることを確かめる。

**受容する残余**: 確定（`mpsc`）と tao のイベント列は別の列なので全順序ではない。tao がイベントを再入中にバッファした場合（モーダルループ中など）は、先行キーより先に Commit が積まれうる。通常打鍵では起きない。`snotra-egui-runtime/CLAUDE.md` に 1 行書く。

### D3. ウィンドウ生成直後に非クライアント領域を再計算させる

tao 0.37.1 は装飾なしの `WM_NCCALCSIZE` 処理が繋がる前に `SWP_FRAMECHANGED` を撃つため、両ウィンドウがキャプション付きのクライアント領域で生まれる（main: client 22 / 生成指定 52、results: 70 / 100）。`egui_shell::create` で `build()` の直後に、両ウィンドウへ `SetWindowPos(SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)` を撃つ。**試作で実測済み**（2026-10-08: main 22 → 52・results 70 → 100・toast シナリオの `bar_rect_mismatch` 消滅）。可視性・Z 順・位置・サイズは変えない（`SWP_SHOWWINDOW` を含めない）。

## 変更ファイルと対象シンボル

| ファイル | 対象 |
|---|---|
| `snotra-egui-runtime/src/input.rs` | `on_keyboard_event`（文字の配送を足す）・新しい純関数（`text_with_all_modifiers` の値と押下状態から egui イベントを導く）・テスト・`:83` / `:220` / `:393` のコメント |
| `snotra-egui-runtime/src/windows_ime.rs` | `ImeAction`・`classify_ime_message`（確定と ENDCOMPOSITION を `Suppress` へ）・`ime_subclass_proc`（確定を読んで送る・`SNOTRA_EGUI_IME_COMMIT` の trace）・新しい純関数（メッセージから送るイベント列を導く・Commit → Preedit の順）・`ImeContext`（`composition_string` を種別を引数に取る形へ変え `GCS_RESULTSTR` も読む）・既存テスト `suppresses_native_composition_but_passes_commit_and_end` の期待値・`:154-179` / `:233-234` / `:343-353` のコメント |
| `snotra-egui-runtime/src/runtime.rs` | `rx_text` の trace arm（tripwire の doc） |
| `PERFORMANCE.md` | `:2723` の `SNOTRA_EGUI_IME_TRACE` の説明に確定の行を足す |
| `src-tauri/src/egui_shell/mod.rs` | `create`（生成直後の再計算）・`:383` のコメント |
| `snotra-egui-runtime/CLAUDE.md` | モジュール構成の `windows_ime.rs` 行（確定の読み出しを持つ）・「一般」の IME 2 項目と順序の受容残余・「focus を獲得した瞬間に押されていたキー」項の tao 根拠 |
| `src-tauri/CLAUDE.md` | 「Win32 メッセージ配送の注意」・「setup フック自身も…」・「宣言的なウィンドウ属性…」・「新しい操作を raw へ寄せるかは…」の版根拠 |
| コメントの版根拠 | `snotra-egui-runtime/src/runtime.rs:517`・`snotra-egui-runtime/src/proof.rs:10` / `:40`・`src-tauri/src/main.rs:301`・`src-tauri/src/egui_shell/window_coordinator.rs:398` / `:1136`・`src-tauri/src/egui_shell/results_window.rs:107-111` / `:150` / `:240`・`src-tauri/src/egui_shell/view.rs:1632` / `:1729` |

`Cargo.toml` / `Cargo.lock` / `package-lock.json` / `src-tauri/gen/schemas/` は #1265 の既存コミットで済んでいる。`SPEC.md` は変えない（入力・表示の仕様は不変。`SPEC.md:449` の NSIS 記述は CLI 2.12.1 で生成した `installer.nsi:818` が同じ `DeleteRegValue` を持つことを 2026-10-08 に確認——日付付きの実測として真のまま）。

## 実装順序

### Phase 1: ウィンドウ生成直後の再計算（D3）

- [x] Red: 現状で smoke の toast シナリオが `bar_rect_mismatch` を出すことを確認（文字入力の赤と並んで出る） — CI run 37781825112 とローカル 3 回で `show 616x82 / frame 616x52`
- [x] `egui_shell::create` に両ウィンドウの再計算を足す（`#[cfg(windows)]`・`apply_rounded_corners` と同じ並び）。理由と測定値を doc に書く
- [x] Green: smoke の toast シナリオで `bar_rect_mismatch` が消える。results の初回 inset は検出器が無いので、一時計装（コミットしない）で `outer − inner` が 2 回目以降と同じことを測ってから計装を消す — 2026-10-08: smoke 緑（`bar_rect_mismatch` 0 件）。`refresh_borderless_frame` 直後に main outer 616x61 / inner 600x52、results 616x109 / 600x100（inset 9 = 定常値）。計装は除去

### Phase 2: 通常文字（D1）

- [x] Red: 純関数のテストを先に書く——`"a"` 押下 → `Commit("a")`、`"\x01"`（Ctrl+A）→ なし、`"\r"` / `"\u{8}"` / `"\t"` / `"\u{1b}"` → なし、release → なし、`None` → なし、サロゲートペア `"😀"` → `Commit("😀")`、`"@"`（AltGr 相当・印字可能）→ `Commit("@")`
- [x] 純関数を実装し `on_keyboard_event` から呼ぶ（`admit_key` の後）。`:393` のコメントを新しい経路の説明へ置き換える。`ReceivedImeText` の arm（`input.rs`）と `rx_text` の arm（`runtime.rs`）の doc に tripwire であることを書く
- [x] `VK_PACKET` の測定: release ビルドを起動し、`SendInput` の `KEYEVENTF_UNICODE` で `z` を送る一時スクリプト（scratchpad に置きコミットしない）で `egui_input:changed` の `after_chars` が 1 増えることを見る。入らなければ同じ Phase で直す（受入 2） — 2026-10-08: `z` は `after_chars` 0 → 1 で入った。**絵文字（`U+1F600`）は入らない**: `VK_PACKET` はサロゲートを 1 単位ずつ別の keydown + `WM_CHAR` で送るので、tao が片割れの UTF-16 を文字列にできず `text` が `None` になる。旧 0.35 の `minimal_ime` も `String::from_utf16(&[wparam])` で 1 単位ずつ復号して捨てていた（`tao-0.35.3/.../minimal_ime.rs`）ので**回帰ではない**——受入 2 の「1 文字で入る」は BMP の文字について満たす
- [x] Green: `cargo test -p snotra-egui-runtime` と smoke の `egui_results:show` 観測 — 38 passed・smoke 緑（`egui smoke passed (show/hide + results show/hide observed, webview delta 0)`）

### Phase 3: IME 確定の所有（D2）

- [x] Red: `classify_ime_message` のテストを新しい期待値へ書き換える（`GCS_RESULTSTR` を含む `WM_IME_COMPOSITION` と `WM_IME_ENDCOMPOSITION` が `Suppress`）。イベント列の純関数のテスト——`RESULTSTR` だけ → `[Commit]`、`RESULTSTR|COMPSTR` → `[Commit, Preedit]`（**この順**）、`COMPSTR` だけ → `[Preedit]`、`ENDCOMPOSITION` → `[Preedit("")]`、確定が空 → Commit を含まない — stub（空 `Vec`）に当てて 5/5 落ちた。実装の都合で `RESULTSTR` だけのときも `read_preedit` の結果（実機では空の Preedit）が Commit の後に続く形でテストを書いた。`classify_ime_message` は lparam を使わなくなったので引数から外した
- [x] `ime_subclass_proc` で確定を読んで `Commit` を送る。`ImeContext` に `GCS_RESULTSTR` の読み出しを足す（`composition_string` と同じ UTF-16 復号を共有する）
- [x] Green: `cargo test -p snotra-egui-runtime` — 42 passed・`cargo clippy --workspace --all-targets -- -D warnings` 緑
- [ ] 人間の実打鍵スモークを依頼し、結果を記録する——`SNOTRA_EGUI_INPUT_TRACE=1` と `SNOTRA_EGUI_IME_TRACE=1` で起動し、(a) 「にほんご」→変換→Enter 確定、(b) 変換中に続けて打鍵して前の文節を確定、(c) 変換中に Escape で取り消し、(d) IME オフで英字、(e) Ctrl+A / Ctrl+C / Ctrl+V。各ケースで入力欄の文字列を**厳密に**照合し（(b) は `GCS_RESULTSTR` が累積するなら二重入力として現れる）、trace の `SNOTRA_EGUI_IME_COMMIT` が確定 1 回につき 1 行・`rx_text` / `push_text` が 0 件・(a)(c) の後に候補ウィンドウが残らないことを見る。手元に IME が 2 種あれば両方で（TSF 系が `GCS_RESULTSTR` を立てずに確定する可能性・plan-review 未検証 2）

### Phase 4: 文書と版根拠（受入 5）

- [x] `snotra-egui-runtime/CLAUDE.md` の IME 2 項目を D1 / D2 の新しい不変条件へ書き換える（通常文字は `KeyEvent::text_with_all_modifiers`・IME 確定は subclass が `GCS_RESULTSTR` から読む・`ENDCOMPOSITION` と確定 `WM_IME_COMPOSITION` を tao へ通さない理由）
- [x] 「focus を獲得した瞬間に押されていたキー」項の tao 根拠を 0.37.1 の行へ差し替え、`PendingEventQueue` で合成 press の順序保証が弱まった受容残余を書く（破綻するのは遅れた press が次の `Focused(false)` の後に届く並びだけ・adversarial 争点 5）
- [x] `src-tauri/CLAUDE.md` の 4 項の版根拠を 2.12.1 / 0.37.1 で読み直して差し替える。「宣言的なウィンドウ属性」項の `MARKER_DONT_FOCUS` の機序は、0.37.1 では生成直後に remove されると直す（結論は不変）
- [x] 変更ファイル表の「コメントの版根拠」の各行を、新版のソースを読んで差し替える（行番号が変わったものは新しい行へ・挙動が変わったものは文を直す）
- [x] D3 の再計算について、`window_coordinator.rs:1136`（非クライアント分）の doc から生成時の前提へ辿れるようにする
- [x] 撤去の語彙検査: `git grep -n "ReceivedImeText\|minimal_ime\|tao-0.35\|tao 0.35\|0.35.3\|2\.11\.[0-9]"` を走らせ、残った出現を「歴史の記述」と「在る前提の記述」へ振り分けて後者を直す（`docs/superpowers/` と `docs/adr/` は前者） — 2026-10-09: 残った出現は移行の描写（`input.rs` / `windows_ime.rs` / `CLAUDE.md` の「0.35 は〜していた」）と日付付きの実測（`SPEC.md:449`・`src-tauri/CLAUDE.md` の「2.11.4 で実測、2.12.1 で再確認」）だけ。版根拠はすべて 2.12.1 / 0.37.1 のソースで読み直した: `emit_filter` 同期実行・setup の `Ready` arm（`app.rs:1442`）・`impl Clone for Window`・`send_user_message` の分岐（`lib.rs:263-280`）・`SetCursor` 直呼び（`window.rs:424-428`）・runner の `event_buffer`・`WM_NULL` のハンドラ皆無・`apply_diff` の早期 return（`window_state.rs:318`）と末尾 `SW_HIDE`（`:408`）・`set_background_color` が `apply_diff` を通らない。`MARKER_DONT_FOCUS` の機序だけ記述を直した（生成直後に外れる・`window.rs:1419`）

### Phase 5: 検証

- [ ] `/race-check` を実装差分に当てる（D2 は既存の `mpsc` へ確定を送る経路を足す。`/race-check` は計画段階では起動しない規約ゆえここで走らせる・#784）
- [ ] `/symmetric-check` を実装差分に当てる（IME メッセージの抑止/通過の対・`STARTCOMPOSITION` と `ENDCOMPOSITION` の扱いが揃ったか）
- [ ] `docs/build-commands.md` カテゴリ A（全 crate のテスト・`cargo doc`）・`npm test`・`npm run governance:check`
- [ ] カテゴリ C: `cargo build -p snotra --release` の後に `scripts/smoke-egui.ps1 -ExePath target/release/snotra.exe` と `npm run smoke:startup`
- [ ] カテゴリ D 相当の目視: 初回 show のバー位置・角丸・影が 2.11 と同じ（`cargo run -p snotra`）

## 不変条件と異常系

- 通常文字と IME 確定は、それぞれ**1 経路だけ**から egui へ入る（通常文字 = `KeyEvent::text_with_all_modifiers`、確定 = subclass の `GCS_RESULTSTR`）。tao の `ReceivedImeText` はどちらの経路でもない
- ネイティブ変換ウィンドウは出さない（#532）——`STARTCOMPOSITION` と未確定 `COMPOSITION` の抑止は不変
- 合成 press の文字は入らない（`admit_key` が先に弾く）
- `GCS_RESULTSTR` の読み出しが失敗・空なら何も送らない（落とすのは確定 1 回分で、プロセスは止めない。preedit の読み出しと同じ倒し方）
- 再計算の `SetWindowPos` が失敗したら従来どおり（初回だけ stale inset）。戻り値は捨てる（`apply_rounded_corners` と同じ倒し方）

## テスト方針と検証コマンド

- 単体: `input.rs` の純関数（Phase 2 の表）・`windows_ime.rs` の `classify_ime_message` とイベント列の純関数（Phase 3 の表）
- smoke: 打鍵は `keybd_event(vk)` で実キーボードと同じ `WM_KEYDOWN` → `WM_CHAR` の枝を通る（adversarial「測定環境」）ので、通常文字の修正の証拠になる。IME・AltGr・デッドキー・サロゲート・合成 press は smoke の外——IME は Phase 3 の人間スモーク、残りは単体テスト
- 変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (1) 文字の配送を `admit_key` の前へ動かす (2) `text_with_all_modifiers` を `text` に替える (3) 確定 `WM_IME_COMPOSITION` を `PassThrough` へ戻す (4) D3 の再計算を外す (5) Commit と Preedit の送る順を入れ替える——それぞれどの検査が落ちるかを測る
- コマンドの正本は `docs/build-commands.md`

## SPEC・関連文書

- `SPEC.md`: 変更なし（理由は「変更ファイルと対象シンボル」の末尾）
- `docs/architecture.md`: 変更なし（入力経路・`ReceivedImeText`・`WM_CHAR` の記述を持たない。2026-10-08 に grep で確認）

## 未確定（実装前に潰す）

- [x] 生成直後の `SWP_FRAMECHANGED` で初回 inset が直るか — 2026-10-08 に試作して実測。main client 22 → 52、results 70 → 100、toast シナリオの `bar_rect_mismatch` 消滅。試作は戻した
- [x] CLI 2.12.1 で NSIS アンインストーラが `Run` 値を消すか（`SPEC.md:449`） — `npx tauri build --bundles nsis` が生成した `target/release/nsis/x64/installer.nsi` の 818 行に `DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"`、36 行に `!define PRODUCTNAME "Snotra"`（`@tauri-apps/cli` 2.12.1）
- [x] IME 確定を tao の `ENDCOMPOSITION` に任せるか自前で読むか — 自前（D2）。tao 経路の懸念 3 つはいずれも「確定を tao と `DefWindowProc` へ通す」ことから生じ、自前にすれば経路ごと消える。正しさは Phase 3 の人間スモークで確かめる
- [x] 確定を `mpsc` で送るとキー入力と順序が入れ替わらないか — `runtime.rs` の `on_window_event` が各イベントの前に `drain_native_ime()` を呼ぶので入れ替わらない（D2）
- [x] 敵対的調査の争点 5（版依存表）で崩れた行があるか — 0 行（⚠️ 2 件は Phase 4 で受容残余と記述の訂正として扱う）

## 人間レビュー

- [x] 承認済み — 2026-10-08 / 問い: "`workspace/plan.md` に注釈を書き込むか、このまま承認するかをお知らせください。" / 回答: "承認、実装して"

## plan-review 結果

- リスク: 高（channel 経由の確定配送・`CLAUDE.md` の不変条件の書き換え）
- レビュー方式: 計画準拠レビュー 1 体（観点: D2 の正しさ・規範書き換えの網羅性）。成果物 `workspace/plan-review-1266-ime.md`
- エージェント数: 1

### 要対処

- Phase 3 のスモークが新経路を観測できない（確定の trace が無い） — 計画の修正（`SNOTRA_EGUI_IME_COMMIT` の行・`PERFORMANCE.md:2723`） — 再照合: `windows_ime.rs` の trace は `if let egui::ImeEvent::Preedit` の内側だけ
- `runtime.rs` の `rx_text` arm が表に無い — 計画の修正（tripwire として残し doc を書く・スモークで 0 件） — 再照合: `runtime.rs:228-231`
- 1 メッセージ 1 イベントの形では Commit → Preedit を送れない — 計画の修正（イベント列の純関数と順序のテスト・変異 (5)） — 再照合: `windows_ime.rs` の `ime_subclass_proc` は `let event = match message { … }` で `Option` を 1 つ作って送る

### 軽微

- **降格 1 件**: 「`src-tauri/CLAUDE.md:142` が表に無い」——同行は表の「Win32 メッセージ配送の注意」に含まれている（再照合で不成立）。語彙検査に `2\.11` を足す提案だけ採った
- `snotra-egui-runtime/CLAUDE.md` のモジュール構成行・subclass の Commit が制御文字フィルタを通らない・順序の受容残余・`composition_string` の引数化——いずれも計画へ反映した

### 未検証

- ENDCOMPOSITION を `DefWindowProc` へ通さないことの TSF 互換層への影響・TSF 系 IME が `GCS_RESULTSTR` を立てずに確定する可能性・`GCS_RESULTSTR` の累積 — 実機でしか測れない。Phase 3 の人間スモークの観察項目へ入れた
- `VK_PACKET` 系の入力 — 受入 2 と Phase 2 の測定項目へ入れた

### 判断

- 実装着手: 人間の承認待ち（要対処はすべて計画へ反映済み）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー 1 体（上）
- エージェント数: 1（敵対的調査 1 体を含めると 2）
- 要対処: 3 件（すべて計画へ反映）
- 未検証: IME の実機挙動 3 点は Phase 3 の人間スモーク、`VK_PACKET` は Phase 2 の測定で潰す
- 5a の照合: 要件 → 作業項目（受入 1〜5 にそれぞれ Phase がある）／境界条件（Ctrl・AltGr・サロゲート・release・空確定・部分確定・取り消し・合成 press・`VK_PACKET`）に検証がある／新しいリソースは無い（`SetWindowPos` は 1 回きりで破棄経路が要らない）／より単純な案（D2 を tao の `ENDCOMPOSITION` に任せる）は懸念 3 つを理由に却下／壊してはならない不変条件（#532 の非表示・二重配送なし・合成 press の抑止）は単体テスト・変異注入・人間スモークで検知する
- 起動する check: `/race-check` と `/symmetric-check` は実装差分に当てる（Phase 5）。`/state-check`・`/persistence-check` は非該当（モード・ガード・永続形式を変えない）
