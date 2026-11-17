# 計画: issue #1470 — 前面化の失敗時に tao が素の Alt を注入し、前面だったアプリのメニューが起動する

## 目的と受け入れ条件

表示時の前面化から、マスクなしの Alt 注入（tao 0.37.2 `force_window_active`）を消す。前面化は自前の関数で行い、`SetForegroundWindow` が失敗したときは、MenuMaskKey（vkE8）で包んだ Alt を注入して 1 回だけ再試行する。

受け入れ条件（実機の注入ハーネスで測る。ハーネスはリポジトリ外の一時の足場で、共有ノートにある）:

- AC1: Alt を離してから d = 1, 3, 6, 10 ms 後に 1 文字を注入したとき、起動後 1 回目の表示で、`vk=0xA4 sc=0x38 injected extended`（tao の注入）が 0 件、試験窓の `SC_KEYMENU`・`WM_ENTERMENULOOP` が 0 件（各 d で 2 起動 × 2 表示）
- AC2: 文字なしと 2 回目以降の表示では、マスクした Alt を注入しない（trace の `egui_show:foreground` の `path` が `direct` か `already`）
- AC3: マスクした Alt を注入した回は、再試行の `SetForegroundWindow` が成功する（`path = "masked_alt"`・`ok = true`）
- AC4: `npm run smoke:startup` と `npm run smoke:egui` が通る
- AC5: `show_egui_main` から tao の `set_focus()` を呼ぶ形へ戻すと、ソーステキスト検査が落ちる

## 変更するファイルとシンボル

1. `src-tauri/src/main.rs`
   - 新設 `bring_to_foreground(hwnd)`（`#[cfg(windows)]`。`send_alt_key_up` の隣）: `GetForegroundWindow() == hwnd` なら何もしない（`already`）。`SetForegroundWindow(hwnd)` が成功すれば終わり（`direct`）。失敗したら `[Alt down, vkE8 down, vkE8 up, Alt up]` を 1 回の `SendInput` で注入し、`SetForegroundWindow` を 1 回だけ再試行する（`masked_alt`・`ok`）。結果を返す（trace 用）。キーの組み立ては `make_key_input` を使う（試作と同じ形。wScan・拡張フラグは付けない）
   - `send_alt_key_up` の関数内ローカル定数 `VK_MASK` をモジュール定数へ上げ、両関数で共有する（マスクキーの選択は 1 か所で決める）
   - `send_alt_key_up` のコメントの「直前の `set_focus()`」を新関数名へ直す
2. `src-tauri/src/egui_shell/window_coordinator.rs` の `show_egui_main`
   - windows では `window.set_focus()` を `crate::bring_to_foreground(hwnd)` へ置き換える。`hwnd()` が取れなければ前面化しない（trace に `path = "no_hwnd"` を残す）。非 windows は `window.set_focus()` のまま
   - trace `egui_show:foreground`（`path`・`ok`）を出す
   - 置き換えた箇所に、tao の `set_focus` を使わない理由を短く書く（マスクなしの Alt を注入し、前面だったアプリのメニューを起動するため・#1470）
   - 同関数内の「`set_focus()` を呼んだ後」「`set_focus()` より後に置く」「`set_focus()` の呼び出し順」の 3 か所を新関数名へ直す
   - `mod tests` にソーステキスト検査を 1 本足す（AC5）: `include_str!("window_coordinator.rs")` の中で `.set_focus()` が現れるのは 1 回だけで、その直前の行が `#[cfg(not(windows))]` である。**needle は `concat!` で割る**（`view.rs` の先例。割らないと検査自身の needle を数える）。**新しく書く説明コメントには `.set_focus()` の形（ドット付き）を書かない**（コメントも数えるため。`set_focus` と書く）
3. `src-tauri/src/platform/mod.rs` の `TurnOffIme` の腕のコメント: 「`set_focus()` → 本コマンド送信」「`set_focus()` 直後のコメント」を新関数名へ直す
4. `snotra-egui-runtime/src/runtime.rs` の `Focused(true)` の腕のコメント: 「show() 直後は必ず set_focus() が続く」を、前面化（`bring_to_foreground`）が続く、へ直す

触らないもの:

- `SPEC.md`（表示時の前面化の機構を規定していない。バグ修正）
- `.claude/skills/dry-check/SKILL.md` の例（`show_egui_main` の主要操作に `.set_focus()` を挙げている）。書式の例であり、規範の主張ではない。セーフティネットの変更になるので、この PR では触らない
- `docs/superpowers/`・`docs/design/`・`docs/adr/` の `set_focus` の言及（書いた後は更新しない記録）
- `src-tauri/gen/schemas/*.json`（tauri の権限スキーマ。生成物）
- 初回の表示が遅いこと・前面化より前の打鍵の取りこぼし（issue 本文「修正しても残るもの」）

## 実装の順序

1. ソーステキスト検査を先に書き、今の `show_egui_main` で落ちることを確かめる（Red）
2. `main.rs` に `bring_to_foreground` と共有のマスクキー定数を入れる
3. `show_egui_main` を置き換え、trace を足す（Green）
4. コメントの 4 ファイルを直す
5. カテゴリ A・C を走らせる
6. release をビルドし直し（いまの `target/release/snotra.exe` は調査の試作版である）、注入ハーネスで AC1〜AC3 を測る

## 不変条件と異常系

- 前面化は `show()` の後・IME オフの送信と `send_alt_key_up` の前に置く（今の `set_focus()` の位置）。IME オフが対象ウィンドウに効く順序を変えない
- マスクした Alt の注入は、前面化に失敗したときだけ、1 回の表示につき最大 1 回。ループで再試行しない
- 注入する Alt は、vkE8 を挟んでから離す。旧アプリがこの Alt を受けても、Alt 単独の解放にならない（`send_alt_key_up` と同じ技法）
- **修飾キーの押下中でも注入を省かない**（未確定欄の実測による裁定）。省くと、Snotra は最前面に見えるのに前面でない窓として残り、打鍵は旧アプリへ流れ続ける
- **再試行も失敗したとき**: そのまま進める。利用者に見えるのは、最前面に表示されているが前面ではない Snotra である（打鍵は旧アプリへ行く。クリックすれば前面になる）。trace に `path = "masked_alt"`・`ok = false` が残る。置き換え前の tao も、再試行の失敗では同じ状態になる（違いは注入した Alt がマスクされていることだけ）
- `Focused(true)` は前面化の結果として届く（`SetForegroundWindow` の成功で `WM_ACTIVATE`/`WM_SETFOCUS`）。`runtime.rs` が `visible` を戻す代理シグナルは変わらない（tao の `set_focus` も中身は `SetForegroundWindow` なので）

## テスト方針と検証コマンド

- ユニット: ソーステキスト検査 1 本（AC5）。Win32 の経路そのものは `cargo test` で検証できない（`.claude/rules/src-tauri.md`）。検出器を置くので、`.claude/rules/safety-nets.md`「検出器を置く・動かすとき」を実装時に読む
- `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（`cargo fmt --check`・`check`・`clippy -D warnings`・`cargo test -p snotra`・`cargo test -p snotra-egui-runtime`・`cargo doc`）とカテゴリ C（`cargo build -p snotra`・`npm test`・`npm run test:powershell`・`npm run smoke:startup`・`npm run smoke:egui`）
- 実機: `cargo build --release -p snotra` の後、共有ノートの `run-probe.ps1` を d = 1, 3, 6, 10 と文字なしで回す（AC1〜AC3）
- 変異: AC5 の検査は、`show_egui_main` を `window.set_focus()` に戻した版で落ちることを `/implement` の委譲先が確かめる

## `SPEC.md`・関連文書の更新

- `SPEC.md`: 不要（上記）
- `src-tauri/CLAUDE.md`: 不要（ファイルの追加・削除なし）

## 作業項目

### フェーズ 1: 検出器

- [x] `window_coordinator.rs` の `mod tests` にソーステキスト検査を足し、現状で落ちることを確かめる（Red: 行 433 の `window.set_focus()` で「直前が非 windows の cfg でない」）

### フェーズ 2: 実装

- [x] `main.rs`: マスクキー定数をモジュールへ上げ、`bring_to_foreground` を足す
- [x] `window_coordinator.rs`: `show_egui_main` の前面化を置き換え、`egui_show:foreground` の trace を足す
- [x] コメントを直す（`window_coordinator.rs` の 3 か所・`main.rs`・`platform/mod.rs`・`runtime.rs`）

### フェーズ 3: 検証

- [x] カテゴリ A を走らせる
- [x] カテゴリ C を走らせる
- [x] ラウンド 1: fd0c8d80 / verify:tree green（A・C・E・F）/ 指摘 Critical 0・High 0・Medium 1・Low 4（M1・L1〜L3 を修正、L4 は射程の記述で修正不要）/ 変異 13 件中 生存 4（UFCS・空白入り・別ファイル経由・windows ブロック削除。いずれも字面検査の射程外）
- [x] ラウンド 2: d63108b1 / verify:tree green（A・C・E・F）/ 前回指摘 M1・L1〜L3 解消（再レビューで確認）、新規 Low 3（N1〜N3: コメントの文言。すべて修正）/ 検査 1 passed。実機: d=3 の 1 回目で `masked_alt`・`sent=true`・`ok=true`、2 回目 `already`・`sent=null`
- [x] ラウンド 3: a7db3b93 / verify:tree green（A・E・F。コメントのみの変更で C の経路は変わらない）/ N1〜N3 の解消は指摘の観点で照合。ADR 回収後の 27a85409 も green（A・E・F）
- [x] 注入ハーネスで AC1〜AC3 を測り、結果を PR 本文用に残す（2026-11-17・release・各 d で 2 起動 × 2 表示: d = 1, 3, 6, 10 の 1 回目 8/8 で tao の注入・`SC_KEYMENU`・メニューループ 0、`masked_alt` は 8/8 で `ok = true`。文字なしの 1 回目は `direct`、2 回目以降は全件 `already`）

## 未確定（実装前に潰す）

- [x] 反証調査（`workspace/adversarial-1470.txt`）の所見を裁定し、計画へ反映する — 裁定は `workspace/research.md`「反証調査の裁定」。計画へ入ったのは、`set_focus()` を前段に足すのではなく置き換えること（前段案だと、自前の再試行が失敗したとき tao の素の Alt が残る）。tao の「可視・非最小化」の条件は写さない（直前の `show()` が可視を作り、main は最小化の経路を持たない）。前面かどうかの判定（`already`）は写す。修飾キーの所見は次の項で実測して裁定した
- [x] 修飾キーの押下中に前面化が失敗したとき、マスクした Alt を注入するか省くか — **注入する。省かない。** 呼び出し元は 5 つ（`main.rs` の single-instance・hotkey の `ShowNow`・`ShowAfterAltRelease`・`setup_startup_display`、`egui_shell/mod.rs` の初回ホットキー登録失敗）で、`ShowAfterAltRelease` の 350 ms 打ち切りでは Alt を、`ShowNow` では Ctrl・Shift・Win を押したまま show に入りうる。実測（2026-11-17・release・試作版・各 3 起動）: Alt+Q を離して 5 ms 後に Shift down と A を注入し、Shift を押したまま前面化を失敗させた。**注入する版**は 3/3 で `masked_alt` が成功し、Snotra が前面になり、試験窓の `SC_KEYMENU` も `WM_INPUTLANGCHANGEREQUEST` も 0 だった。**修飾キーの押下中に省く版**は 3/3 で `modifier_held` に入った（試作は省いた後に tao の `set_focus` を残していたので、そこで素の Alt が走って 3/3 でメニューが起動した。置き換え後なら、前面にならないまま残る）。Alt の押下中だけの省略も採らない。理由は、注入した Alt up が押されている Alt を論理的に離す害（その後の打鍵が Alt なしになる）より、前面にならない窓が残る害のほうが大きいから。置き換え前の tao の注入も、同じく Alt up を送っていた

## 人間レビュー

- [x] 承認済み — 2026-11-17 / 問い: "workspace/plan.md の計画で実装に進めてよいですか？" / 回答: "承認する"

## セルフレビュー

- リスク: 通常。`plan-review`「リスク判定」の 6 条件のうち、迷うのは「複数モジュール間のインターフェースを新設」だけである。`main.rs` の関数を `window_coordinator.rs` から呼ぶ形は、既存の `send_alt_key_up`・`is_alt_pressed` と同じで、契約（型・状態・永続化）を持たない。listener・channel・共有状態・状態遷移は触らない。異論があれば高リスクへ上げて Step 2b を回す
- plan-review: 未実施（通常リスク）
- エージェント数: 1（Step 3b の反証調査）
- 要対処: 2 件（置き換えの採用・修飾キー押下中の扱いを実測で裁定。いずれも計画へ反映済み）
- 未検証: マスクした Alt の down が、Word・Electron・UWP でキーヒントなどを残さないか（試験窓のメニューでしか測っていない）。single-instance・startup・初回ホットキー失敗の経路での前面化の失敗は測っていない（成功時の挙動は置き換え前と同じ `SetForegroundWindow`）。Alt 押下中に前面化が失敗する回は作れていない（Shift で代わりに測った）
- 散文の全件列挙: 未実施（通常リスク）
- 照合（5a）: issue の受け入れ条件 3 つは AC1・AC2・AC4 に対応する。境界は d = 1, 3, 6, 10 ms・文字なし・2 回目以降・修飾キー押下中（実測済み）・再試行も失敗（不変条件に扱いを書いた）。新しい状態・リソースは持たない（注入は 1 回きりで、破棄の経路は要らない）。より単純な既存のパターンは無い（tao の `set_focus` そのものが原因）。壊してはならない不変条件「tao の素の Alt を呼ばない」は AC5 の検査が見る
- check スキル（`AGENTS.md`「条件別チェック」）:
  - `/dry-check`（関数の新設）: 当たる。実装時に実行する。既知の近縁は `send_alt_key_up`（マスクキーの定数を共有する）と `tray.rs` の `SetForegroundWindow`（トレイメニューの前置きで、失敗時の注入を持たない別の用途）
  - `/symmetric-check`（show/hide の対称ペア）: 当たらない。変えるのは show の中の前面化の手段だけで、show/hide の対や、立てて戻す状態を増やさない
  - `/state-check`・`/persistence-check`: 当たらない（UI モード・ガード・永続形式を触らない）
  - `/race-check`: 計画段階では起動しない（スキルの規定）。前面化はイベントループスレッド上で同期に行い、新しいスレッド・channel は無い
  - smoke の前提（trace イベント名・表示経路）: `egui_show:foreground` を足すだけで、既存のイベントを消さない。`scripts/smoke-egui.ps1` の前提は壊れない（AC4 で確かめる）
