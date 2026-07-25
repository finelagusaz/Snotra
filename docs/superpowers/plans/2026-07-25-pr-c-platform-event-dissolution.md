# PR C: `platform-event` 袋の解体 + イベント名の定数化 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 内側種別が 1 つしかない汎用イベント袋 `platform-event` を解体し、`initial-hotkey-failed` を独立イベント名へ昇格する。あわせてアプリ内 Tauri イベント名 9 種を定数化し、emit 側と listen 側が同一 path を参照するようにする。

**Architecture:** `src-tauri/src/events.rs` にイベント名の定数を置く。`platform-event` の payload（`{event, hotkey}` の JSON オブジェクト）は、既に repo 内で実績のある `hotkey-registration-failed` と同じ流儀（**素の `String`**）へ揃え、`serde_json::Value` の手動フィルタを消す。

**Tech Stack:** Rust / Tauri v2.11

**根拠となる spec:** `docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` の**決定 3**。前提サイクル: PR A / A′ / B マージ済み。

## この PR が安全である前提（実測済み）

**`platform-event` の consumer はこのリポジトリの `src-tauri/src` だけである。** TS フロントの受け口（`ui/src/MainApp.tsx` の `listen("platform-event", ...)`）は #532 SU7 PR3（`15933af`）で撤去済みで、リポジトリ全体の grep でも `src-tauri/src` と `SPEC.md` 以外にヒットは無い（残るのは `.superpowers/sdd/` 配下の git 管理外 scratch diff のみ）。**外部プロセス・外部プラグインが購読していないことが、名前を変えてよい根拠である。**

## Global Constraints

- **`register_platform_event_listener` の登録位置を動かさない。** `main.rs` の egui ブロック内、`setup_hotkey_listener`（`RegisterInitialHotkey` を platform スレッドへ送る）より**前**にある必要がある——後ろに移すと emit を取りこぼす。改名はしても位置は動かさない。
- **payload は `hotkey-registration-failed` と同じ流儀（素の `String`）にする。** 新しい流儀を発明しない。2 つの失敗経路の listener が見た目でも対称になることが、決定 3 の言う grep 可能性の回復の実体である。
- **`register_hotkey_failure_listener`（変更失敗）と `register_initial_hotkey_failure_listener`（起動時失敗）を統合しない。** 前者は wake せず、後者は show + wake する——**同じ機構が経路によって逆を向く**のは SU6.5 決定 2 の意図的な設計である（config 変更が随伴するか否かで `config-applied` の到着が変わる）。`/simplify` が統合しないよう理由をコメントに残す。
- **新しい doc コメントに `platform-event` の文字列を書かない。** Task 1 Step 6 の grep 期待値が 0 にならない（PR A / A′ / B で 3 度踏んだ自己参照の罠）。歴史に触れる必要があれば「旧・汎用チャネル袋」等、検索語を含まない言い回しにする。
- **定数化の効能は限定的であることを、実装にも文書にも書く**（下記）。

### 定数化が防ぐもの / 防がないもの（spec 決定 3 の記述をそのまま持ち込む）

> **効能の限定**: 定数化が防ぐのは綴り不一致のみであり、現状 9/9 が一致しているため今この誤りは存在しない。#652 の実形は綴り不一致でも受け口消失でもなく、**新しい UI 経路（egui）を並走させたときに旧経路（TS）にしかない受け口を複製し忘れた coverage gap** である。定数化ではこれを防げない。袋の解体のほうが効果が大きい。

**したがって PR 本文でも「定数化で #652 が防げるようになった」と書いてはならない。** 定数化の実利は「アプリ内イベントの全体像が 1 箇所で見える」ことであり、`events.rs` の `//!` にもそう書く。

## テストの位置づけ（AGENTS.md ステップ 9 への回答）

1. **compile-fail**（定数の未定義参照・関数改名の呼び出し漏れ）
2. `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra`
3. 定数の相互 distinct テスト 1 本（**保証は狭い**。下記 Task 2 Step 3 の注記）
4. `npm run governance:check`（新規ファイル追加のため必須）
5. `npm run smoke:startup`
6. **`SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE=1` の手動起動（load-bearing・省略不可）**

**5 は本 PR の変更を検査しない。** `smoke:startup` は起動 trace に `*:error` が無いことを見るだけで、**listener が壊れても起動はきれいに通り、通知だけが永遠に出ない**。「エラーが無い」は「効いた」ではない（#671 PR A′ の教訓・`src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」）。end-to-end の証明は 6 だけである。

---

### Task 1: `platform-event` 袋の解体

**Files:**
- Modify: `src-tauri/src/platform/mod.rs`（emit）
- Modify: `src-tauri/src/egui_shell/mod.rs`（listener + `HotkeyFailureKind::Initial` の doc）
- Modify: `src-tauri/src/main.rs`（呼び出し側の関数名）
- Test: なし（compile-fail と Task 4 の手動検証が守る）

**Interfaces:**
- Produces: `egui_shell::register_initial_hotkey_failure_listener(app)`（旧 `register_platform_event_listener`）
- イベント: `initial-hotkey-failed`、payload は**素の `String`**（ホットキー表記。例 `"Alt+Q"`）

- [ ] **Step 1: emit 側を独立イベントへ**

`src-tauri/src/platform/mod.rs` の `PlatformCommand::RegisterInitialHotkey` arm を置き換える（`hotkey_str` の作り方と `SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE` ハッチは現行のまま）:

```rust
                if !registered {
                    let hotkey_str =
                        format!("{}+{}", current_hotkey.modifier, current_hotkey.key);
                    // 独立イベント名で emit する（#673 spec 決定 3）。旧実装は汎用チャネル 1 本に
                    // `{event, hotkey}` を詰めており、内側種別は最後までこの 1 種だけだった——
                    // 袋の中身は grep に映らず、受け口の有無を機構で問えない（#652 の gap 特定が
                    // 構造的に不可能だった原因）。payload は
                    // `hotkey-registration-failed` と同じ素の String に揃える。
                    let _ = app_handle.emit("initial-hotkey-failed", hotkey_str);
                }
```

- [ ] **Step 2: listener を改名し、`serde_json::Value` フィルタを消す**

`src-tauri/src/egui_shell/mod.rs` の `register_platform_event_listener` を置き換える。**doc の 3 つの理由（格納が先 / show する / wake する）は現行のまま残す**——SU6.5 決定 2 の設計理由であり、本 PR で失われてはならない。

```rust
/// 起動時 hotkey 登録失敗の受け口（#652・SU6.5 決定 2）。**格納 → show → wake** の順で処理する。
///
/// - **格納が先**: show が起こすフレームは reset-on-show の `notice.clear()` を通ってから
///   pending を消費する（view の順序不変条件）。逆順にすると clear と store の間にフレームが
///   挟まりうるため、通知が消えたまま二度と出ない。
/// - **show する**: ホットキーが登録できていない＝ユーザーが窓を開く手段がトレイしか無い。
///   SPEC §10「初回ホットキー登録失敗時は操作不能回避のため検索 UI を表示し」の実装。
/// - **wake する**: `show_on_startup=true` で既に可視なら `show()` は再描画を生まない。
///   `register_hotkey_failure_listener`（変更失敗）が wake しないのと**意図的に逆**——
///   あちらは必ず `config-applied` が随伴するが、起動時失敗には config 変更が無く
///   `config-applied` は来ない。ここで起こさないと「hidden 中は update() が走らない」
///   不変条件（SU5）により通知が永遠に描かれない。**この非対称ゆえ 2 つの listener を
///   統合してはならない**（`/simplify` 対象外）。
pub(crate) fn register_initial_hotkey_failure_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("initial-hotkey-failed", move |event| {
        // emit 側は String を渡すため payload は JSON 文字列（引用符付き）。
        // `register_hotkey_failure_listener` と同じ流儀（#673 spec 決定 3 で袋を解体）。
        let hotkey: String = serde_json::from_str(event.payload()).unwrap_or_default();
        if let Some(sh) = handle.try_state::<EguiShellState>() {
            *sh.pending_hotkey_failure.lock().unwrap() =
                Some((HotkeyFailureKind::Initial, hotkey));
        }
        show_egui_main(&handle, Instant::now());
        wake_view(&handle);
    });
}
```

- [ ] **Step 3: `HotkeyFailureKind::Initial` の doc から袋の名を消す**

`egui_shell/mod.rs` の enum:

```rust
    /// 起動時の初回登録失敗（`initial-hotkey-failed`）。
    /// SPEC §10 のとおり窓を能動表示してから通知する
    /// （listener は `register_initial_hotkey_failure_listener`・#652 Task 4）。
    Initial,
```

- [ ] **Step 4: `main.rs` の呼び出し側を直す**

`main.rs` の setup ブロック。**位置は動かさない**（Global Constraints）:

```rust
            // 起動時 hotkey 登録失敗の受け口（#652）。RegisterInitialHotkey を送る
            // setup_hotkey_listener より前に登録される位置なので emit を取りこぼさない
            //（この egui ブロック自体が setup_platform_thread の直後・hotkey listener の前）。
            egui_shell::register_initial_hotkey_failure_listener(&app_handle);
```

- [ ] **Step 5: ビルドとリント**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 警告 0（関数改名の呼び出し漏れはコンパイルエラーになる）

Run: `cargo test -p snotra`
Expected: 既存件数のまま pass

- [ ] **Step 6: 袋の名が消えたことを数える**

Run: `grep -rn "platform-event" src-tauri/src`
Expected: **0 件**。**新しいコメントに `platform-event` と書かないこと**——書くと 0 にならない（PR A / A′ / B で 3 度踏んだ罠）。0 でなければ、ヒットがコメントか実コードかを見て、コメントなら検索語を含まない言い回しへ変える。

Run: `grep -rn "register_platform_event_listener" src-tauri/src`
Expected: **0 件**

- [ ] **Step 7: コミット**

```
refactor: #673 platform-event 袋を解体し initial-hotkey-failed へ昇格

内側種別は最後までこの 1 種だけだった（TS フロントが汎用チャネルを 1 本持っていた
時代の遺物）。袋の中身は grep に映らず、受け口の有無を機構で問えない——#652 の gap
特定が構造的に不可能だった原因そのもの。payload は hotkey-registration-failed と同じ
素の String へ揃え、serde_json::Value の手動フィルタを消す。

2 つの失敗経路の listener を統合しないのは意図的（変更失敗は wake せず、起動時失敗は
show + wake する。config-applied が随伴するか否かで逆を向く・SU6.5 決定 2）。
```

---

### Task 2: イベント名の定数化

**Files:**
- Create: `src-tauri/src/events.rs`
- Modify: `src-tauri/src/main.rs`（`mod events;` + listen 3 箇所）
- Modify: `src-tauri/src/config_watcher.rs`（emit 2 箇所）
- Modify: `src-tauri/src/indexing.rs`（emit 2 箇所）
- Modify: `src-tauri/src/platform/mod.rs`（emit 2 箇所）
- Modify: `src-tauri/src/platform/tray.rs`（emit 2 箇所）
- Modify: `src-tauri/src/egui_shell/view.rs`（emit 2 箇所）
- Modify: `src-tauri/src/egui_shell/mod.rs`（listen 4 箇所）
- Test: `src-tauri/src/events.rs` の `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `crate::events::{HOTKEY_PRESSED, HOTKEY_REGISTRATION_FAILED, INITIAL_HOTKEY_FAILED, CONFIG_APPLIED, INDEXING_STARTED, INDEXING_COMPLETE, EGUI_HIDE_REQUESTED, OPEN_SETTINGS, EXIT_REQUESTED}`

**現状の全 emit / listen（実測・移行対象）**:

| イベント | emit | listen |
|---|---|---|
| `hotkey-pressed` | `platform/mod.rs` | `main.rs` |
| `hotkey-registration-failed` | `config_watcher.rs` | `egui_shell/mod.rs` |
| `initial-hotkey-failed` | `platform/mod.rs`（Task 1 で新設） | `egui_shell/mod.rs` |
| `config-applied` | `config_watcher.rs` | `egui_shell/mod.rs`（wake 表） |
| `indexing-started` | `indexing.rs` | 同上 |
| `indexing-complete` | `indexing.rs` | 同上 |
| `egui-hide-requested` | `egui_shell/view.rs` | `egui_shell/mod.rs` |
| `open-settings` | `platform/tray.rs` | `main.rs` |
| `exit-requested` | `platform/tray.rs`・`egui_shell/view.rs` | `main.rs` |

- [ ] **Step 1: `events.rs` を新規作成する**

```rust
//! アプリ内 Tauri イベント名の定数（#673 spec 決定 3）。
//!
//! **実利は「アプリ内イベントの全体像が 1 箇所で見える」こと**である。#652 の gap は
//! 「新しい UI 経路を並走させたとき、旧経路にしかない受け口を複製し忘れた」形であり、
//! **定数化ではそれを防げない**（防げるのは綴り不一致だけで、現状その誤りは存在しない）。
//! 効能を過大に読まないこと。
//!
//! **文字列リテラルで emit / listen しない**——両端が同じ path を参照して初めて意味を持つ。

/// ホットキー押下（platform スレッド → main）。
pub(crate) const HOTKEY_PRESSED: &str = "hotkey-pressed";
/// 設定変更によるホットキー再登録の失敗（payload: 素の `String`。旧設定は維持される）。
pub(crate) const HOTKEY_REGISTRATION_FAILED: &str = "hotkey-registration-failed";
/// 起動時の初回ホットキー登録の失敗（payload: 素の `String`）。受け口は窓を能動表示する
/// （SPEC §10）。`HOTKEY_REGISTRATION_FAILED` と**受け口の挙動が逆を向く**理由は
/// `egui_shell::register_initial_hotkey_failure_listener` の doc。
pub(crate) const INITIAL_HOTKEY_FAILED: &str = "initial-hotkey-failed";
/// config 適用完了の合図（値は運ばない・#532 SU6 spec 決定 1）。
pub(crate) const CONFIG_APPLIED: &str = "config-applied";
/// index 構築の開始／完了の合図（値は運ばない）。
pub(crate) const INDEXING_STARTED: &str = "indexing-started";
pub(crate) const INDEXING_COMPLETE: &str = "indexing-complete";
/// view からの hide 要求（`hide_egui_main` の 1 経路へ集約するための内部イベント）。
pub(crate) const EGUI_HIDE_REQUESTED: &str = "egui-hide-requested";
/// トレイメニューからの設定画面起動要求。
pub(crate) const OPEN_SETTINGS: &str = "open-settings";
/// 終了要求（トレイメニュー / `/q` スラッシュコマンド）。
pub(crate) const EXIT_REQUESTED: &str = "exit-requested";

#[cfg(test)]
mod tests {
    use super::*;

    /// **保証は狭い**: ここに並べた 9 種が互いに異なることだけを見る。定数を新設しても
    /// この配列へ足さなければ検査対象にならない——「将来の追加を守る」機構ではなく、
    /// 現時点のコピペ重複（同じ文字列を持つ 2 定数 = listener の相互誤発火）を弾くだけ。
    #[test]
    fn event_names_are_pairwise_distinct() {
        let all = [
            HOTKEY_PRESSED,
            HOTKEY_REGISTRATION_FAILED,
            INITIAL_HOTKEY_FAILED,
            CONFIG_APPLIED,
            INDEXING_STARTED,
            INDEXING_COMPLETE,
            EGUI_HIDE_REQUESTED,
            OPEN_SETTINGS,
            EXIT_REQUESTED,
        ];
        let mut sorted = all.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "イベント名が重複している");
    }
}
```

- [ ] **Step 2: `main.rs` に `mod events;` を足し、listen 3 箇所を移行**

`main.rs` の `mod` 宣言群へ `mod events;` を追加（既存の並びに合わせる）。listen:

```rust
    app_handle.listen(crate::events::HOTKEY_PRESSED, move |_| {
    app_handle.listen(crate::events::OPEN_SETTINGS, move |_| {
    app_handle.listen(crate::events::EXIT_REQUESTED, move |_| {
```

- [ ] **Step 3: 残る emit / listen を定数へ移行**

上の表の全行を `crate::events::*` 参照へ置き換える。`egui_shell/mod.rs` の wake 表は:

```rust
    for event in [
        crate::events::CONFIG_APPLIED,
        crate::events::INDEXING_STARTED,
        crate::events::INDEXING_COMPLETE,
    ] {
```

- [ ] **Step 4: 文字列リテラルが残っていないことを数える**

Run: `grep -rn '\.emit("\|\.listen("' src-tauri/src`
Expected: **0 件**（すべて定数参照になっている）

Run: `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra`
Expected: 警告 0 / 既存 +1 本 pass

- [ ] **Step 5: コミット**

```
refactor: #673 アプリ内 Tauri イベント名 9 種を events.rs へ定数化

emit 側と listen 側が同一 path を参照する形にする。実利は「アプリ内イベントの全体像が
1 箇所で見える」ことで、#652 の coverage gap（新経路を並走させたとき旧経路にしかない
受け口を複製し忘れる）はこれでは防げない——効能を過大に読まないよう events.rs の //! と
テストのコメントに限定を明記した。
```

---

### Task 3: 文書の同期

**Files:**
- Modify: `SPEC.md`（§7.5・**1 箇所のみ**。実測で確認済み）
- Modify: `src-tauri/CLAUDE.md`（モジュール索引 + config_watcher の発火イベント行）

- [ ] **Step 1: SPEC.md §7.5 を実態へ**

`SPEC.md` §7.5「設定反映タイミング」の反映機構の行のうち、末尾を置き換える:

```
起動時（`initial-hotkey-failed`）は §10 のとおり検索 UI を能動表示してから通知する（#652）
```

（旧: 「起動時（`platform-event` の `initial-hotkey-failed`）は…」）

**§10（`:504` 付近）は改訂不要**——「初回ホットキー登録失敗時は操作不能回避のため検索UIを表示し、ウィンドウ内にエラー通知を表示する」は経路非依存でイベント名を含まない。

**これは仕様変更ではなくイベント名の実装事実の同期である**が、SPEC.md が名前を明記している以上、AGENTS.md ステップ 0 の「文書化された挙動を変えたら SPEC を同期する」に従って更新する。

- [ ] **Step 2: `src-tauri/CLAUDE.md` を更新**

1. モジュール構成に `events.rs` の行を足す（ファイル名の索引を保つ・#562）:

```
- `events.rs`: アプリ内 Tauri イベント名の定数（#673）。emit / listen の両端が同一 path を参照する
```

2. `config_watcher.rs` の「発火するイベント」列挙はそのまま（`platform-event` を含まないため変更不要・実測）

- [ ] **Step 3: ガバナンス検査**

Run: `npm run governance:check`
Expected: G1..G10 passed。**新規ファイルを含む PR では PR 作成前に必ず走らせる**（#629 / #630 の同型再発）。

- [ ] **Step 4: コミット**

```
docs: #673 PR C のイベント名を SPEC.md とモジュール索引へ同期
```

---

### Task 4: 検証

**Files:** なし

- [ ] **Step 1: 起動 smoke（非回帰・ただし本 PR の変更は検査しない）**

Run: `npm run smoke:startup`
Expected: PASS。

**この検査は listener の破損を検出できない**——起動 trace に `*:error` が無いことを見るだけで、受け口が壊れても起動はきれいに通る。非回帰の確認としてのみ数える。

- [ ] **Step 2: `SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE=1` の手動起動（load-bearing・省略不可）**

PowerShell:

```powershell
$env:SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE=1; $env:SNOTRA_TRACE=1; ./target/release/snotra.exe
```

Expected（SU6.5 spec §検証 と同一）:

1. **窓が自動で開く**（listener の `show_egui_main`）
2. **`initial_failed` 文言の通知が 5000ms 表示される**（pending 格納 → reset-on-show の後に消費 → wake）

**このハッチは登録自体は成功させたまま失敗イベントだけを流す**ため、ホットキーは動いたままである（`platform/mod.rs` のコメント参照）。確認後はプロセスを終了する。

**これが本 PR の唯一の end-to-end 証明である。** 名前を変えて受け口が繋がっていなければ、窓は開かず通知も出ない——そして `smoke:startup` は緑のままになる。

- [ ] **Step 3: 結果を PR 本文へ記録する**

「追加/更新テスト名 + 検証した不変条件」（AGENTS.md ステップ 9）。**Step 2 を実施していないなら「未実施」と書く**——実施したことにしない。
