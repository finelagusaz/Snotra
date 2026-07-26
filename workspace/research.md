# research: issue #737 — snotra-egui-runtime にフレームレート上限を入れる

前提資料: `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`（契約②「配送には下限間隔がある」の実装・§4 が機構を確定・実施順序 §8 の 3 番）。§8 の 1 番（#697・PR #741）・2 番（#714・PR #742）はマージ済み。

## issue の要約と決定事項

結果窓上のポインタ移動中に 448fps / 1 コア 84.7% を消費する（表示は 144Hz・描いたフレームの 68% は表示されない）。egui の ZERO 遅延要求は設計として正しく、**上限を持たない runtime 側の責務**。

- **決定（2026-07-26 セッション合意・本日再確認）**: 案 1 = モニターのリフレッシュレートを上限・取得失敗時 60Hz・**config キーなし**（issue コメントの「フォールバックを config へ」は採らないことを本日ユーザーへ確認済み）
- **取得カスケード（issue コメント 2026-07-26T05:43Z を採用）**: 動的（現在モード）→ OS 既定 → 60Hz。Win32 では `EnumDisplaySettingsW(ENUM_CURRENT_SETTINGS)` → `EnumDisplaySettingsW(ENUM_REGISTRY_SETTINGS)` → 60 に写像できる
- 設計 spec §4 の機構: worker ループに `dispatch_at = max(deadline, 前回 dispatch + min_interval)`、`min_interval` は `Arc<AtomicU64>`（ナノ秒）で供給、plugin が活性化時 + モニター跨ぎで更新

## 受け入れ条件（issue）

1. ポインタ移動中 fps に上限・1 コア占有の有意減を**同一プロトコルの実測**で示す（#710 の手順 A〜D・`git show b64be6c:workspace/measurement.md` で回収済み・スクラッチパッドに `measurement-628.md`）
2. 入力応答性の非悪化（`egui_search:dispatch` trace + 目視）
3. 上限は main / results 両窓に効く（runtime 変更ゆえ自動。main はアイドル 2fps で上限に触れず無害——確認事項として測定に含める）

## 関連コード（2026-07-26 の main = `e66343f` で確認済み）

| 場所 | 現状 |
|---|---|
| `snotra-egui-runtime/src/repaint.rs` worker ループの `None`（deadline 満期）arm | `pending.take()` →（#697 の SEND 計器）→ `proxy.send_event(RequestRedraw)`。**pacing の挿入点**。`Stop` arm は `None` arm より先に match され停止契約は先行（#671 PR D） |
| `repaint.rs` `RepaintScheduler::new` / `SchedulerInner` | worker スレッド所有。`Arc<AtomicU64>`（interval ナノ秒）をここへ足し、worker が dispatch のたび読む。setter `set_min_interval` を `RepaintScheduler` に公開（plugin = `ActiveWindow.scheduler` が呼べる） |
| `snotra-egui-runtime/src/runtime.rs` `attach_pending_windows`（scheduler 生成部） | 活性化時の初回リフレッシュレート取得 + set の挿入点。`window.window`（`tauri::Window`）→ `.hwnd()` で HWND が取れる |
| `runtime.rs` `Event::WindowEvent` arm | `Moved` / `ScaleFactorChanged` の受信点（モニター跨ぎ追従）。**Moved はドラッグ中に連発する**ため、`ActiveWindow` に HMONITOR を cache し、`MonitorFromWindow`（安価）の結果が変わったときだけ `EnumDisplaySettingsW` を再実行する |
| `snotra-egui-runtime/Cargo.toml` | `windows =0.61.3` + `Win32_Graphics_Gdi` / `Win32_Foundation` 既存——`MonitorFromWindow` / `GetMonitorInfoW`(`MONITORINFOEXW`) / `EnumDisplaySettingsW`(`DEVMODEW`) はこの feature 圏。**使用前に 0.61.3 での型・シグネチャを確認する**（`src-tauri/CLAUDE.md` の規範。src-tauri 側は 0.62 でバージョンが違う点に注意） |
| `runtime.rs` repaint callback | `scheduler.request(info.delay)`——egui の ZERO 遅延要求の流入点（変更不要・上限は worker 側で吸収） |
| `PERFORMANCE.md` 計器一覧 | `SNOTRA_EGUI_REPAINT_TRACE` / `SNOTRA_EGUI_WAKE_TRACE`（#697）——測定にそのまま使う。SEND 行は dispatch と 1:1 のため、**上限が効いていること自体の trace 検証にも使える** |

## 既存パターン

- Win32 monitor 問い合わせ: `src-tauri/src/monitor.rs`（`MonitorFromWindow` + `GetMonitorInfoW`）——ただし crate が違う（cap は runtime の責務ゆえ runtime 側へ新設。src-tauri の関数は tauri::Window 非依存の生 HWND 版で流用不可はないが、crate 境界を跨ぐ依存を作らない）
- runtime crate 内の unsafe Win32 前例: `windows_ime.rs`（IMM32 subclass）
- 純粋核 + テストの流儀: `repaint.rs` の既存 tests（`wake_before_activation_is_queued` 等）・`retry_delay`（`runtime.rs`）

## 技術的制約

- **契約③との整合**: pacing は deadline を**落とさず遅らせる**——gate 未達なら `pending = max(gate, deadline)` で再武装し、満期で必ず dispatch する（「予約はフレーム 1 枚以上を約束する」を保つ）
- **入力応答への影響は最大 `min_interval`**（144Hz なら ≤7ms・60Hz でも ≤16.7ms）——契約②が明記する受容範囲
- 活性化直後の初回 dispatch は gate 初期値（worker 起動時刻）を過ぎており即時（ウォームアップフレームを遅らせない・spec §2.4 の初回フレーム維持）
- paint 失敗リトライ（16ms〜）は既に上限より粗く実質不変
- **測定は人間操作**（ホバー往復・ホイール往復は注入で再現しない方針が #710 プロトコルの前提）。A/B は同日・同条件で 2 回（基線 = 現 main の release / 上限 = 本ブランチの release）——`PERFORMANCE.md`「warm frame は日をまたいで比較しない」
- DRR（動的リフレッシュレート）パネルで `ENUM_CURRENT_SETTINGS` が返す値の意味論は環境依存——上限の趣旨（表示されないフレームを描かない）では現在モード値で十分であり、**受容する残余**とする

## 未解決の疑問

- `dmDisplayFrequency` が 0/1 を返す環境（仮想ディスプレイ等）の実在頻度——カスケード + 60 フォールバックで吸収するため設計上は問題にならない
