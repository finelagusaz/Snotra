# plan: issue #737 — repaint worker に配送の下限間隔（フレーム上限）を入れる

前提は `workspace/research.md` と設計 spec `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` §4（機構確定済み）。type:feature / size:M。挙動変更は「`RequestRedraw` 配送の下限間隔」のみ（描画内容・イベント処理は不変）。

## 変更ファイル一覧

| ファイル | 変更 | Phase |
|---|---|---|
| `snotra-egui-runtime/src/repaint.rs` | ① 純粋核: `interval_from_hz(u32) -> Option<Duration>`（0/1 → None）と `pace(deadline, gate) -> Instant`（= max・dispatch 予定時刻）+ テスト（TDD Red→Green） ② `Arc<AtomicU64>`（interval ナノ秒・初期 1/60 秒）を **worker スレッド生成前に作り clone を closure へ渡す**（`wake_channel` と同じ順序制約・偵察指摘。もう一方は `SchedulerInner` が保持） ③ `RepaintScheduler::set_min_interval(hz: Option<u32>)`（None/0/1 → 60Hz へ） ④ worker の待ち時間計算を `pace(deadline, next_allowed)` 基準へ（**`pending` は書き換えない**——後着の早い要求のたび早発 wake する再武装案は独立導出が却下。`Some(deadline)` 時の `recv_timeout` の目標時刻に max を適用し、`Timeout` 到達 = gate 通過で dispatch・`next_allowed = now + min_interval`） ⑤ モジュール doc へ契約②の要約 + 満期 arm の非飢餓の根拠（/race-check）+ SEND 計器は gate 通過後・send 直前のまま（1:1 維持・偵察指摘） | 1, 2 |
| `snotra-egui-runtime/src/monitor.rs`（新規） | `#[cfg(windows)] fn monitor_refresh_hz(hwnd) -> Option<u32>` と `fn window_monitor(hwnd) -> isize`（変化検知用の安価な `MonitorFromWindow` 単独呼び出し）。カスケード: `EnumDisplaySettingsW(CURRENT)` → 0/1 なら `(REGISTRY)` → None。API は windows 0.61.3 実ソースで確認済み（`MonitorFromWindow` は値返し・`GetMonitorInfoW`/`EnumDisplaySettingsW` は BOOL・`MONITORINFOEXW` は MONITORINFO 先頭埋め込みで cbSize 定石・`dmDisplayFrequency` は直接フィールド） | 3 |
| `snotra-egui-runtime/src/runtime.rs` | ① `ActiveWindow` に `last_monitor: isize`（HWND の isize 保持は `windows_ime.rs` の既存パターン） ② 活性化時: 取得して `scheduler.set_min_interval` ③ `Moved` / `ScaleFactorChanged` 受信時: `window_monitor`（安価）で比較し**変わったときだけ**再取得 + set ④ **`Focused(true)` 受信時は無条件で再取得**（静止中の OS 設定変更・モニター抜き差しの backstop——show のたびに 1 回で頻度は低い・独立導出の推奨） | 3 |
| `snotra-egui-runtime/CLAUDE.md` | モジュール構成へ `monitor.rs` 行を追加・`repaint.rs` 行に「配送の下限間隔（フレーム上限・#737）」を追記 | 3 |
| `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` | §1 の系統 B へ errata 追記: IME preedit 更新の `InvalidateRect`（`windows_ime.rs`）も gate 外の OS 由来経路（IME 打鍵レートに有界・受容——独立導出の発見） | 3 |
| （測定・コミットなし） | A/B 実測（基線 = main release / 上限 = 本ブランチ release・#710 プロトコル A〜D・人間操作）→ 結果を issue #737 へコメント | 4 |

## 実装

### 純粋核（Phase 1・TDD）

```rust
/// リフレッシュレート(Hz)から配送の下限間隔へ。0/1 は「取得失敗」（DEVMODE の慣習値）として None。
fn interval_from_hz(hz: u32) -> Option<Duration>;
/// 次の dispatch 予定時刻（契約②）: deadline と gate（前回 dispatch + min_interval）の遅い方。
/// deadline を落とさず遅らせるだけ——契約③「予約はフレーム 1 枚以上を約束する」を保つ。
fn pace(deadline: Instant, gate: Instant) -> Instant;
```

テスト: `interval_from_hz(0/1) = None`・`interval_from_hz(60) ≈ 16.67ms`・`interval_from_hz(144) ≈ 6.94ms`・`pace` は遅い方（deadline 未来/gate 未来の両向き + 同時刻）。

### worker ループ（Phase 2・待ち時間計算と `None` arm）

```rust
// Some(deadline) の待ち時間計算（現行: deadline.saturating_duration_since(now)）を
// pace() 基準へ差し替える——pending の意味論（最も早い要求 deadline）は不変:
Some(deadline) => {
    let target = pace(deadline, next_allowed);        // ← ここで max を取る
    let timeout = target.saturating_duration_since(Instant::now());
    match receiver.recv_timeout(timeout) { .. }        // 構造は現行のまま
}
// None（Timeout = target 到達）arm: gate は通過済みなので再判定不要:
None => {
    let Some(_) = pending.take() else { continue; };
    // （既存の SNOTRA_EGUI_WAKE_SEND 計器——gate 通過後・send 直前で 1:1 維持）
    if proxy.send_event(..).is_err() { break; }
    next_allowed = Instant::now()
        + Duration::from_nanos(min_interval_nanos.load(Ordering::Relaxed));
}
```

再武装（`pending = Some(target)` の書き戻し）は行わない——後着の早い Request が pending を引き戻すたび早発 wake が生じるため（独立導出の指摘）。max は待ち時間計算に閉じ、`Timeout` 到達がそのまま gate 通過を意味する。

- `Stop` arm が `None` arm より先に match される現行構造は不変（停止契約 #671 PR D）
- **満期 arm の非飢餓の根拠をコメントに残す**（/race-check 境界 2）: Request 洪水（マウス移動 ≤~1kHz）でもメッセージ処理は ns 級で queue が枯れ、空 queue の `recv_timeout` が `Timeout` を返して満期 arm へ到達する
- `Request` 受信時の coalescing（早い deadline を採る）は不変。gate 待ち中の新規 Request で `pending` が手前に動いても、次の満期で再び `pace` が gate へ繰り延べる——**上限は要求側から迂回できない**
- `next_allowed` の初期値は worker 起動時刻——活性化直後の初回 dispatch は即時（ウォームアップ維持・spec §2.4）

### リフレッシュレート取得（Phase 3・`runtime.rs`）

```rust
#[cfg(windows)]
fn monitor_refresh_hz(window: &tauri::Window) -> Option<u32> {
    // HWND → MonitorFromWindow(MONITOR_DEFAULTTONEAREST) → MONITORINFOEXW.szDevice
    // → EnumDisplaySettingsW(dev, ENUM_CURRENT_SETTINGS)   … 動的（現在モード）
    // → 0/1 なら EnumDisplaySettingsW(dev, ENUM_REGISTRY_SETTINGS) … OS 既定
    // → 0/1 なら None（呼び出し側 set_min_interval が 60Hz へ）
}
```

- **windows crate 0.61.3 の API 型を書く前に確認**（`src-tauri/CLAUDE.md` 規範。runtime は 0.61.3・src-tauri は 0.62 でシグネチャが違いうる）
- `Moved` 連発対策: `ActiveWindow.last_monitor`（`HMONITOR.0 as isize`）と比較し、変化時のみ `EnumDisplaySettingsW` を呼ぶ

## 不変条件

1. **deadline は落とさない**（契約③）: gate 未達の deadline は gate 時刻へ繰り延べて必ず配送する。`pending = None` へ黙って落とす経路を作らない
2. **停止契約不変**: `Stop` → break が pacing より優先（match 順は現行のまま）
3. **上限は配送点にのみ効く**: 要求側 8 経路・render()・入力処理・OS 由来（系統 B）の RedrawRequested には触れない
4. **取得失敗は 60Hz へ静かに倒す**（カスケード終端）。パニック・ログ洪水を作らない（`Moved` 中の再取得はモニター変化時のみ）
5. **失敗・異常系**: `EnumDisplaySettingsW` 失敗 / `GetMonitorInfoW` 失敗 → None → 60Hz。`set_min_interval` は AtomicU64 store のみで失敗しない。worker が gate 再武装中に `Stop` を受けても即 break（recv_timeout が `Stop` を返す）

## テスト方針

- **TDD（純粋核）**: `interval_from_hz` / `pace` に Red→Green（スタブで落とすことを確認してから実装）
- 既存テスト（`wake_before_activation_is_queued` 等）が緑のまま（worker 起動前の queue 契約に pacing は関与しない）
- post-edit hook: clippy + `cargo test -p snotra-egui-runtime`（沈黙=合格）+ `cargo check --workspace` + `cargo doc`（doc 追記のため）
- `npm run smoke:egui`（カテゴリ C 相当: 描画ループ所有 crate の変更。show/hide の trace 観測が上限で壊れないこと）
- **実機測定（Phase 4・受け入れ条件 1〜3）**: #710 プロトコル A〜D を**同日 2 回**（基線 = main の release / 上限 = 本ブランチの release）、人間操作 + 私が解析。判定: 段 C（ホバー往復）の大バースト fps が 448 → ≤ モニター値、1 コア占有の有意減、段 A のアイドル 2fps 不変（M5）、hide 後 0 フレーム（M6）、打鍵応答の目視（受け入れ条件 2）
- governance:check（CLAUDE.md / SPEC.md 編集・カテゴリ F）

## SPEC.md 更新要否

**不要へ変更**（初版は 1 行追記としていたが、独立導出の判定を採用）。SPEC はフレームレート・fps を一切文書化しておらず、上限は性能特性であって機能仕様ではない。恒久記録の行き先は (1) 契約②＝設計 spec（既存）(2) `PERFORMANCE.md` の基準値節（実測後に「操作中 fps ≤ モニターリフレッシュレート」を追記）(3) `snotra-egui-runtime/CLAUDE.md`。

## コミット構成

1. `chore: workspace 調査・計画 (issue #737)`
2. `feat(egui-runtime): repaint worker に配送の下限間隔を入れる（純粋核 + pacing）(#737)` — Phase 1+2
3. `feat(egui-runtime): モニターのリフレッシュレートを取得し下限間隔へ供給する (#737)` — Phase 3（SPEC / CLAUDE.md 追記込み）
4. （測定はコミットなし・issue コメント）

## セルフレビュー

### 5a. plan-review の反映（偵察 1 + 独立導出 1 + /race-check）

- 偵察: 要対処なし。軽微 4 件を反映——(1) `Arc<AtomicU64>` はスレッド生成前に構築し clone を closure へ（`wake_channel` と同順序）(2) set の呼び出し点は活性化 + `WindowEvent` arm の 2 箇所で足りることを構造で確認（`ActiveWindow` だけが window と scheduler の両方に触れる）(3) SEND 計器の順序（gate 通過後・send 直前）を明文化 (4) 独立導出の `Focused(true)` backstop が初版 plan に未反映だった点を反映。windows 0.61.3 API・`hwnd()` 利用可否・`predicted_dt=0` との独立性・定常ループの min_interval 周期収束は実ソースで裏付け済み
- 独立導出との差分の解決: **採用** = timeout 計算時 max（pending 再武装案の却下・否定の知識として上に記載）/ `monitor.rs` 新規ファイル分割 / `Focused(true)` backstop / IME `InvalidateRect` 第 3 経路の spec errata / SPEC.md 更新不要への反転。**一致** = 変更は runtime crate に閉じる・Cargo.toml 変更不要・エッジケース集合（Stop 即応・活性化直後非遅延・2 窓独立 gate・hidden 中の負債非蓄積・paint リトライ）
- /race-check: 全 4 境界 [安全]。満期 arm の非飢餓の根拠をコメントへ残す義務を Phase 2 に追加済み

### 5b. plan-review が扱わない 3 観点

1. **境界条件**: interval 供給前の初回 dispatch（初期値 1/60s・活性化直後は gate 過去で即時）/ hz=0/1（カスケード→60）/ min_interval 変更の反映（次 dispatch から・遅延 1 回は benign）/ 長時間 hidden 後（gate 遥か過去→即時）/ DRR・VRR（現在モード値を上限とする受容残余）
2. **シンプル化**: config キーなし（合意済み・4 点セット回避）。新規状態は Arc<AtomicU64> 1 個 + last_monitor 1 個が最小形。「取得失敗したらどうなるか」= 60Hz へ静かに倒れる（カスケード終端・パニックなし）
3. **破壊不変条件 + 検知手段**: ①「deadline を落とさない」（契約③）→ pace は max のみで pending 非破壊・純粋核テストで固定 + 測定 M6（hide 後 0 フレーム）と smoke:egui の show/hide 観測 ②「停止契約」→ Stop arm の match 順不変・既存テストと `SchedulerInner::drop` の join で顕在化 ③「上限が効く」→ 測定 A/B の段 C（448fps → ≤144）+ SEND 計器の dispatch レート ④「入力応答」→ 測定の打鍵目視 + `egui_search:dispatch` trace
