# research: issue #697 — hidden 中の paint 抑止機構の実測 + トートロジーテスト + 決定 5 の記録欠落

前提資料: `workspace/frame-scheduling-design.md`（フレームスケジューリング契約のたたき台・2026-07-26 合意済み）。本 issue はその契約④「hidden 中のフレームは約束されない」の接地作業にあたる（実施順序 §8 の 1 番）。

## issue の要約

#671/#673 サイクルの残余 3 項目。共通項は「接地していない記述が、接地した根拠として引用されうる形で残っている」。

1. **項目 1**: 「hidden 中は `update()` が走らない」の抑止機構が未同定・未測定。issue コメント（2026-07-25・#628 の副産物実測）で「**受信側（`RedrawRequested`）には来ていない**」まで確定済み。残る切り分けは **(A) worker は送ったが tao/OS が落とした** か **(B) そもそも送られていない** かで、コメントが実験設計（送信側・受信側の 2 計器 + hidden 中 config 変更の刺激）まで指定している
2. **項目 2**: `hidden_window_is_not_painted` はトートロジーテスト。処置は「削る」か「接地コメント付きで残す」の 2 択で、**項目 1 の後に判断**（実 `render()` を検査する形は不可能——`EguiWindow` は実 HWND + `ImeBridge` を要し、crate は dev-dependencies ゼロ）。あわせて `runtime.rs` の「（本ブランチ b9a9caf）により」を PR #677 へ言い換える
3. **項目 3**: spec 決定 5 が要求した「無条件 `wake_results` を削ると壊れる理由」の doc コメントが as-built に無い。`view.rs` の `drive_results_window` 末尾へ 1〜4 行で記録する。項目 1・2 と独立

## 関連コード（すべて 2026-07-26 の main で実在確認済み・行番号は現在値）

| 場所 | 現状 |
|---|---|
| `snotra-egui-runtime/src/repaint.rs:143-149` | worker の `proxy.send_event(Message::Window(id, RequestRedraw))`。**送信側計器の挿入点**（直前） |
| `snotra-egui-runtime/src/runtime.rs:184-194` | `Event::RedrawRequested` arm。**受信側計器の挿入点**（`window_id_map` 引き当てより前・issue コメント指定） |
| `snotra-egui-runtime/src/runtime.rs:307-318` | `render()` の `visible` 早期 return と到達可能性注記。「（本ブランチ b9a9caf）により」が**宛先を失った参照**（項目 2 後半） |
| `snotra-egui-runtime/src/runtime.rs:344-358` | #628 の既存計器 `SNOTRA_EGUI_REPAINT`（`run_ui` 直後・env ゲート・未設定時コスト 0）。**新計器の流儀の手本** |
| `snotra-egui-runtime/src/runtime.rs:481-488` | トートロジーテスト `hidden_window_is_not_painted`（項目 2） |
| `src-tauri/src/egui_shell/mod.rs:536-549` | `wake_main` doc。「OS/tao 層にあると推測されており未測定」の限定並記（測定後に書き換え） |
| `src-tauri/src/egui_shell/mod.rs:551-563` | `wake_results` doc。「可視時・毎フレーム・level-triggered」のラベルはあるが「削ると壊れる理由」は無い |
| `src-tauri/src/egui_shell/mod.rs:619-634` | `register_config_wake_listeners`。3 イベントとも **`wake_main` のみ**・可視性を見ない（issue の記述どおり） |
| `src-tauri/src/egui_shell/view.rs:857` | `drive_results_window` 末尾の無条件 `wake_results`。**コメント無し**（項目 3 の記録先。issue 記載の :850 から移動） |
| `src-tauri/src/egui_shell/view.rs:1756-1767` | snapshot 差分 wake（`matches` 不一致時のみ）。visual-only config 変更では `RowsSnapshot` 不変ゆえ発火しない——「削ると壊れる理由」の根拠、現在も成立 |
| `src-tauri/src/config_watcher.rs:162` | `CONFIG_APPLIED` は load 成功なら**値の差分と無関係に無条件 emit**（`ReadFailed` のみ適用 skip）。→ **同一内容の書き戻し（no-op rewrite）が刺激として使える** |
| `src-tauri/src/events.rs:29-35` | `CONFIG_APPLIED` / `INDEXING_STARTED` / `INDEXING_COMPLETE` 定数 |

## 既存パターン（再利用）

- **計器の流儀**: #628 の `SNOTRA_EGUI_REPAINT`（env ゲート内 eprintln・未設定時は Instant 取得も clone もしない）。新計器 2 本も同じ env `SNOTRA_EGUI_REPAINT_TRACE` のゲート内に置く（issue コメント「#628 の計器と同じ流儀」）
- **実機操作の自動化**: `scripts/smoke-egui.ps1` が keybd_event によるホットキー注入・trace 観測・stderr 捕捉の全部品を持つ（起動時の `hotkey:registered` trace から VK 列を導出）。測定スクリプトはこれを雛形にスクラッチパッドへ書く（リポジトリへは入れない——1 回きりの測定でありスモークの回帰資産ではない）
- **trace の読み方の規範**（`src-tauri/CLAUDE.md`）: presence ではなく「**区間内に事象が現れないこと**」を数える。本測定は「hidden 区間で受信 0」という不在の観測であり、この規範に合致する形

## 技術的制約

- `snotra-egui-runtime` は dev-dependencies ゼロ・mock 無し。実 `render()` の検証はユニットテスト不可能（純関数 + 実機 smoke の二分が既定・issue 明記）
- 送信側計器は worker スレッド上の eprintln（stderr はプロセス共通・問題なし）。`WindowId` は Debug 表示で識別する（main/results の 2 窓を区別できれば足り、ラベルは要らない）
- config.toml は `%APPDATA%\Snotra\config.toml`（`snotra-core/src/config.rs` の `//!`）。watcher は 100ms debounce + ReadFailed 時バウンドリトライ——同一内容の atomic 書き戻しで安全に `CONFIG_APPLIED` を発火できる
- `predicted_dt = 0`（PR #709）後のコードで測る前提（issue コメントの注意）。現 main はその後継であり満たす
- ホットキーに Alt を含む場合、注入時に Alt 解放まで送る（`ShowAfterAltRelease` で最大 350ms 遅延・smoke スクリプト既知）
- **キャレット点滅の副次信号**: hide 直前の可視フレームは `request_repaint_after(≤0.5s)` を必ず積んでいる（#628 実測）。よって (A) なら hidden 直後に送信計器が config 刺激**なしでも** 1 発以上出るはず——刺激前の区間も判定材料になる

## 未解決の疑問

- 測定結果が (B)（送信 0）または (C)（`render()` 到達）だった場合はスコープが変わる（deadline 管理の欠陥調査 / `visible` ガード実効化設計）。**その場では直さず、報告 + 別 issue 起票で止める**（設計書 §6 の分岐どおり）
