# research: issue #711 — blur 猶予 100ms の再要求経路が無い（1 回きりの予約に依存・潜在）

前提資料: `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` §5（対処案 A 確定済み）・契約③「予約は『フレーム 1 枚以上』を約束し、『条件成立』を約束しない」。実施順序 §8 の 4 番。

## issue の要約

`view.rs` の blur 猶予（focus 喪失 → 100ms 後に hide 判定）は、**予約したフレームが 1 枚来ることに依存し、その 1 枚で条件を満たさなかったときの再要求経路を持たない**。イベント駆動 runtime では「次のフレーム」は誰かが要求しない限り来ないため、`grace_elapsed` が false のまま次の要求が無ければ **hide は次の無関係な入力まで宙吊り**になる。

**今日は顕在化していない**（#628 で `predicted_dt` を 0 にし、`request_repaint_after(100ms)` が実際に 100ms 以降に発火するようになったため）。ゆえに本 issue は**潜在的な脆さの頑健化**であり、挙動は不変。同型の 4 箇所（検索 debounce・通知期限・起動タイムアウト）のうち blur 猶予だけが「毎フレーム再要求」の対称性を欠く。

## 関連コード（2026-07-26 の main `e66343f` で実在確認済み）

| 場所 | 現状 |
|---|---|
| `src-tauri/src/egui_shell/view.rs:1321-1325` | blur エッジ: `was_focused && !focused` で `unfocus_at = Some(now)` + `request_repaint_after(100ms)`（**1 回きりの予約**） |
| `src-tauri/src/egui_shell/view.rs:1326-1338` | 猶予明け判定: `grace_elapsed = at.elapsed() >= 100ms` → `blur_should_hide(..)` が真なら `unfocus_at = None` + `emit_hide()`。**false のときの再要求が無い**（本 issue の欠陥） |
| `src-tauri/src/egui_shell/view.rs:1299-1301` | focus 復帰で `unfocus_at = None`（armed 解除の唯一の他経路） |
| `src-tauri/src/egui_shell/lifecycle.rs:26-33` | 純粋核 `blur_should_hide(focused, grace_elapsed, auto_hide, settings_running)` = `!focused && grace_elapsed && auto_hide && !settings_running`。**変更しない** |
| `src-tauri/src/egui_shell/view.rs:1691-1697`（debounce の armed 節） | **多数派の流儀の手本**: armed の間、毎フレーム残余を `request_repaint_after`。コメントが「scheduler の coalescing で +interval の wake が消されても deadline で確実に起きるよう」と理由を明記 |
| `src-tauri/src/egui_shell/view.rs:1255-1261`（通知期限） | 同型（poll + 残余の再要求） |
| `src-tauri/src/egui_shell/view.rs:502`（起動タイムアウト） | 同型（`LAUNCH_TIMEOUT - elapsed` を launching 中に再要求） |

## 発見: 猶予値 100ms の手書き重複

`Duration::from_millis(100)` が **view.rs の 2 箇所**（エッジの予約 `:1324` と判定の閾値 `:1328`）に手書きされている。片方だけ変えると「予約は 100ms 後・判定は別の閾値」という静かな不整合になる。名前付き定数（`lifecycle.rs`）へ一本化するのが本修正の自然な副産物（`/dry-check` 相当の予防）。

**同名・別概念に注意**: `config_watcher.rs:63` の `from_millis(100)` は config 監視の debounce で**無関係**（統合してはならない）。

## 既存パターン（再利用）

- **3 値 enum + 純粋核 + ユニットテスト**: 直近の #714（`RowScroll` / `scroll_directive`）と同型。`lifecycle.rs` は既に純粋核の置き場で `#[cfg(test)] mod tests` を持つ（`plan_hotkey` のテストあり）
- **時刻の注入でテスト可能にする**: `layout.rs` の `Debouncer::poll(elapsed)` が「時刻は driver が注入する（純粋・テスト可能）」の前例。blur も `elapsed: Duration` を引数に取れば同じ形になる

## 技術的制約

- `view.rs` の `update()` は実窓 Context 依存でユニットテスト対象外。**判定と残余の算出を純粋核へ出す**ことでのみテストできる（設計書 §5 が指摘した論点）
- 挙動不変ゆえ A/B 実測は不要。検証は「純粋核のテスト」+「実機で blur→hide が従来どおり動く」の 2 点
- PR #744（#737・`snotra-egui-runtime`）は未マージだがファイル境界が分かれる（本 issue は `src-tauri/` のみ）

## 未解決の疑問

- なし（対処案は設計書 §5 が案 A で確定・案 B/C の却下理由も記録済み）
