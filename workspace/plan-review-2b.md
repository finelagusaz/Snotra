# plan-review Step 2b: 独立導出（#737 フレームレート上限）

- 日付: 2026-07-26
- 対象 issue: #737（snotra-egui-runtime にフレームレート上限が無く、ポインタ移動中に 448fps / 1 コア 84.7% を消費する）
- 導出条件: issue 本文・コメント・リポジトリ規約・設計 spec `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`（§3 契約・§4 機構）・コードのみ。`workspace/plan.md` / `workspace/research.md` は未読。
- 決定済み前提: 案 1 = ウィンドウが載るモニターのリフレッシュレートを上限、取得失敗時 60Hz、config キーなし。取得カスケードは 動的（現在モード）→ OS 既定 → 60。

## 1. 要件の理解（WHAT）と受け入れ条件の解釈

**WHAT**: repaint worker（`repaint.rs`）の dispatch に下限間隔 `min_interval` を導入する。dispatch 時刻 = `max(最も早い deadline, 前回 dispatch + min_interval)`。`min_interval` = ウィンドウが載るモニターのリフレッシュレートの逆数（取得カスケード: `ENUM_CURRENT_SETTINGS` → `ENUM_REGISTRY_SETTINGS` → 60Hz）。egui の「即時再描画してよい」という要求（`Some(Duration::ZERO)` の連発）を、runtime が「何 fps まで消化するか」という自分の責務として絞る。egui 側・view 側・呼び出し点 8 箇所には一切触れない。

**受け入れ条件の解釈**:

1. **AC1（fps 上限と CPU 減）**: 同一プロトコル（#710 の `workspace/measurement.md` 手順 A〜D）の再実測で、results ウィンドウ上のポインタ移動中 fps ≤ モニター実測値（実測環境 144Hz）、1 コア占有が有意減。理論見積は 84.7% × 144/448 ≈ 27%（spec §4 の期待値と一致）。paint 1 枚のコスト（p50 1.64ms）は不変なので「移動中に軽い」の根治ではない——枚数が減るだけ。
2. **AC2（入力応答性）**: gate が入力起因フレームにも一律に効くため、追加遅延の理論上限は `min_interval`（144Hz で ≤ 6.94ms、60Hz で ≤ 16.7ms）。`SNOTRA_TRACE` の `egui_search:dispatch`（`view.rs:951`）で打鍵→検索 dispatch の遅延を前後比較し、目視で確認。
3. **AC3（両ウィンドウ）**: worker はウィンドウごとに 1 本で機構は共通ゆえ自動的に両方へ効く。main は 12.1fps / 0.9% で上限未満のため**挙動不変が期待値**であり、「それが望ましいか」は (a) main のポインタ移動時も上限が効いて害が無いこと (b) 可視アイドル基準値（2.0fps・`PERFORMANCE.md`）が回帰しないこと、の 2 点の非回帰確認として解釈する。

**スコープ外**（同 spec の別ステップ・#737 に含めない）: #714（スクロールアニメーション）、#711（blur 猶予の再要求）、契約 5 か条の `snotra-egui-runtime/CLAUDE.md` 転記（spec §8 の 5 番）。実施順は spec §8 のとおり #714 の後・#711 の前（測定の帰属を混ぜないため）。

## 2. 必要な変更集合（ファイル + シンボル + 1 行説明）

### コード（すべて `snotra-egui-runtime`。src-tauri は変更なし）

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-egui-runtime/src/repaint.rs` | worker ループ（`RepaintScheduler::new` 内クロージャ） | 待ち・発火の deadline を `max(pending, next_allowed)` で計算し、dispatch 後に `next_allowed = now + min_interval` を前進。`min_interval` は dispatch のたびに `Arc<AtomicU64>`（ナノ秒）から読む |
| 同上 | `RepaintScheduler::new` | シグネチャに `min_interval_nanos: Arc<AtomicU64>` を追加（呼び出し元は `runtime.rs` の 1 箇所のみ） |
| 同上 | 新規純粋核（例 `FrameGate` または `dispatch_at(pending, next_allowed) -> Instant` + `advance(now, min_interval)`） | gate 判定をテスト可能な純関数へ切り出す（worker は `EventLoopProxy` 依存でユニットテスト不能なため。既存の純粋核スタイルに合わせる） |
| 同上 | `#[cfg(test)] mod tests` | gate の純粋核テストを追加（§5 参照）。既存 4 テスト（`wake_before_activation_is_queued` 等）はチャネル層のみで契約不変・無改変 |
| `snotra-egui-runtime/src/monitor.rs`（新規。名前は `display.rs` / `refresh_rate.rs` でも可） | `pub(crate) fn window_refresh_interval(hwnd: isize) -> Duration` | `MonitorFromWindow(MONITOR_DEFAULTTONEAREST)` → `GetMonitorInfoW`（`MONITORINFOEXW` で `szDevice`）→ `EnumDisplaySettingsW(ENUM_CURRENT_SETTINGS)` の `dmDisplayFrequency`、0/1/失敗なら `ENUM_REGISTRY_SETTINGS`、それも 0/1/失敗なら 60Hz |
| 同上 | 純粋核（例 `interval_from_frequencies(current: Option<u32>, registry: Option<u32>) -> Duration`）+ `const FALLBACK_HZ: u32 = 60` | カスケードの判定を Win32 非依存の純関数にしてテスト（0・1 は「ハードウェア既定」の意味で無効値扱い） |
| `snotra-egui-runtime/src/lib.rs` | `mod monitor;` | 新規モジュール宣言（公開 API 追加なし。`#[cfg(windows)]` 実体 + 非 Windows は 60Hz スタブ、`windows_ime.rs` と同様の cfg 分離） |
| `snotra-egui-runtime/src/runtime.rs` | `ActiveWindow` | `min_interval: Arc<AtomicU64>` フィールド追加（plugin が書き worker が読む） |
| 同上 | `attach_pending_windows` | Arc を生成 → `window.window.hwnd()` で初回取得・store（イベントループスレッド上・活性化時に 1 回）→ `RepaintScheduler::new` へ渡す |
| 同上 | `RuntimePlugin::on_event` の `WindowEvent` arm | `Moved` / `ScaleFactorChanged`（+ 推奨: `Focused(true)`。§3-8 参照）受信時に再取得して store（モニター跨ぎ追従） |

`Cargo.toml` の変更は不要: `MonitorFromWindow` / `GetMonitorInfoW` / `MONITORINFOEXW` / `EnumDisplaySettingsW` / `DEVMODEW` / `ENUM_*_SETTINGS` はすべて `Win32_Graphics_Gdi` にあり、既に feature 宣言済み（`windows =0.61.3`）。

### 文書（§4 に詳細）

- `snotra-egui-runtime/CLAUDE.md`: モジュール構成へ新規ファイル行を追加 + `repaint.rs` 行に下限間隔の言及
- `docs/architecture.md`: runtime の責務記述（71 行付近「repaint・描画失敗リトライ…を管理」）へフレーム上限を追記
- 設計 spec の進捗行（冒頭「残りは 2〜5 番」）を更新
- `PERFORMANCE.md`: 実測完了後にポインタ移動中の基準値を追記

## 3. エッジケースの列挙

1. **gate × coalescing**: pending（最も早い希望 deadline）の意味論は不変に保ち、待ち時刻・発火時刻だけを `max(pending, next_allowed)` で clamp する。spec §4 スケッチの「`pending = dispatch_at` に書き戻す」形は、後着のより早い deadline が `min` で pending を next_allowed 未満へ引き戻し、早発 wake → 再 clamp を要求メッセージごとに繰り返す（busy-loop ではないが無駄 wake）。**pending を汚さず、timeout 計算時に都度 max を取る**方が簡潔で、`SchedulerMessage::Request` 到着が早発 wake を生まない。どちらでも正しさは保たれる（発火は next_allowed より早くならない）が、実装時に意識する。
2. **Stop との順序**: gated 待ちも `recv_timeout` の中なので、`Stop` は待ち中でも即受信して break する。`SchedulerInner::drop` の join が `min_interval` 分遅延しない。Stop 時に pending の 1 発が捨てられるのは現行と同じ契約（破棄済みウィンドウ宛ゆえ無害）。
3. **活性化直後**: `next_allowed` の初期値は worker 起動時刻（= 過去）とし、初回フレーム（活性化時の `scheduler.request(ZERO)` と queue 済み wake）を遅延させない。queue された複数 wake は現行どおり coalescing で 1 発に畳まれる。
4. **モニター跨ぎ**: `Moved` で再取得。main のドラッグ中は `Moved` が連発し、results も `SetWindowPos` 追従（`position_results_below_main`）で `Moved`（WM_MOVE）を受ける——再取得は Win32 呼び出し 3 つで µs オーダーゆえ受容（気になるなら HMONITOR をキャッシュし変化時のみ `EnumDisplaySettingsW`）。境界跨ぎ中に値が 144/60 で振れても、worker は dispatch ごとに読むので次フレームから追従する。
5. **取得失敗**: hwnd 取得失敗・`GetMonitorInfoW` 失敗・`dmDisplayFrequency` が 0/1（「ハードウェア既定」の意味）→ カスケードの次段へ、最終 60Hz。AtomicU64 に 0 が入る経路を作らない（防御として読み側でも 0 なら 60Hz 扱い）。
6. **DRR/VRR**: VRR パネルは現在モードとして最大値が返る → 上限が緩めに効くだけで趣旨（表示されないフレームを描かない）に反しない。Windows 11 の DRR（60↔120 切替）は `ENUM_CURRENT_SETTINGS` がブースト前の値を返す局面がありうる → 実レートより低い cap になっても入力応答遅延の上限が 16.7ms に収まる範囲で受容（issue の「取得経路で値が違いうる」の受容と同型）。
7. **複数ウィンドウ**: worker・gate・AtomicU64 はウィンドウごとに独立。main と results が別モニターに載る（境界跨ぎ）場合も各自のモニター値で cap。プロセス合計 fps は最大 2×cap になるが、どちらも表示されるフレームなので目的に反しない。
8. **ウィンドウ静止のまま表示モード変更**（OS 設定でリフレッシュレート変更・モニター抜き差し）: `Moved` / `ScaleFactorChanged` のどちらも来ず、**再取得トリガーが無い**（spec §4 の供給設計の穴）。害は「cap が古いままになる」だけで安全側にも危険側にも小さいが、ランチャーは hide/show を高頻度に繰り返すので **`Focused(true)`（show 直後に必ず来る・`visible` 復帰と同じ代理シグナル）でも再取得する** backstop を推奨。
9. **hidden 中**: worker の dispatch 自体は gate されつつ送信され、hidden なら tao/OS 層で `RedrawRequested` が落ちる（#697 実測）。gate は hidden を跨いで負債を溜めない（next_allowed は単調前進のみ）。
10. **paint 失敗リトライとの相互作用**: リトライは `request_repaint_after(16ms〜)` → repaint callback → worker 経由なので gate の内側。60Hz 環境では初回 16ms < 16.7ms が僅かに繰り延べられるが無害。バックオフの単調性も不変。

### 「表示されないフレームを描かない」に対する spec §4 機構の漏れ（数え上げ）

gate は worker（系統 A の合流点）にしか無い。`render()` に到達する経路で gate を通らないもの:

1. **OS 由来の `RedrawRequested`**（expose・リサイズ・occlusion 解除。spec §1 系統 B）——描き直しの必要が実在するフレームであり、意図的に対象外。頻度は OS 事象に有界。
2. **`windows_ime.rs:218` の `InvalidateRect`**（preedit 更新時）→ WM_PAINT → tao が `RedrawRequested` を配送——**spec §1 の表に載っていない第 3 の発生源**。IME 変換中の打鍵レートに有界で、gate 対象の入力イベント起因フレームと同一フレームに合流することが多く、実害は小さい。受容するが、数え上げから漏れていたことは記録する。
3. **同一間隔への重畳**: worker dispatch と OS 由来が同じ `min_interval` 窓に重なると瞬間的に cap を超える。定常的には超えない。受容。

逆に、gate の**内側**であることを確認済みの経路（spec §1 系統 A の全 6 行）: egui 内部要求（ポインタ移動・点滅・アニメーション）／view の `request_repaint(_after)`／paint 失敗リトライ／入力イベント（plugin の `scheduler.request(ZERO)`）／外部 wake（`wake_main` / `wake_results`）／活性化直後の初回要求。**すべて `SchedulerMessage::Request` に合流する**（`repaint.rs` のチャネルが唯一の入口であることをコードで確認した）。

## 4. 文書の追随が要る箇所（概念で分類）

**A. 機構の記述（同 PR で必須）**

- `snotra-egui-runtime/CLAUDE.md` モジュール構成: 新規ファイル行の追加（AGENTS.md「条件別チェック」の「ファイルを追加」トリガー。責務散文は新ファイルの `//!` を正本に）+ `repaint.rs` の行へ「dispatch の下限間隔（モニターのリフレッシュレート・#737）」を追記。
- `docs/architecture.md` 71 行付近: runtime の責務列挙（「repaint・描画失敗リトライ」）にフレーム上限を追加（AGENTS.md「アーキ・横断パターンに影響」トリガー）。

**B. 設計 spec の進捗（同 PR で必須）**

- `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` 冒頭の進捗行: 3 番（#737）完了を反映。

**C. 基準値・計測（実測後に追記）**

- `PERFORMANCE.md`「計測と受け入れ基準」: 可視アイドル基準値（2.0fps・「アイドルで 2fps を超えていたら回帰を疑う」）は本変更で不変——**変えない**ことを確認する側。ポインタ移動中の新基準値（fps ≤ モニターレート・1 コア占有）を実測後に 1 行追加。計器リスト（3 つの env）は変更なし（選択レートの trace を足すなら正本のこのリストへ追記が必要）。

**D. 変更不要と判断した箇所（根拠付き）**

- `SPEC.md`: fps・フレームレートを文書化した箇所が無い（grep で確認。§「非表示中はフレームが走らない」520 行は hidden の話で無関係）。文書化された挙動を変えないため仕様変更に当たらず、SPEC 同期不要。
- `snotra-egui-runtime/CLAUDE.md` の不変条件節への契約 5 か条転記: spec §8 の 5 番（全実装一致後）であり #737 では行わない。
- `.claude/rules/`: snotra-egui-runtime 向け rule ファイルは存在せず、`src-tauri.md` の対象コードにも触れない。
- `docs/build-commands.md`: IPC ルート・smoke 前提（trace イベント名・hotkey）に変更なし。
- `input.rs` の `predicted_dt = 0`: gate は dispatch 側の絞りで、egui への「要求どおりの時刻に起こす」契約は不変。触らない（#628 の回帰テストが固定している）。

## 5. 測定計画の要点

**前提**: `workspace/measurement.md`（#710 の手順 A〜D）は現ブランチに存在しない。git 履歴（#710 のブランチ）から復元するか、issue 記載の要点（release ビルド・`SNOTRA_EGUI_REPAINT_TRACE` + `SNOTRA_EGUI_PAINT_TRACE`・REPAINT/PAINT の 1:1 照合・PAINT 行は直前の REPAINT と対にし `px` で裏取り）で同一プロトコルを再構成する。

1. **AC1**: 実装前後の同日・同条件比較（`PERFORMANCE.md` の「warm frame は日をまたいで比較しない」規範）。results ウィンドウ上でポインタを動かし続け、REPAINT 行の `since_prev_ms` 分布から fps を算出。判定: 移動中 fps ≤ 144（実測環境）かつ p50 間隔 ≥ 6.9ms、1 コア占有 84.7% → 30% 弱（≈27% 見積り）。フレーム数削減率 ≈ 68% を PAINT 行数でも裏取り。
2. **AC2**: `SNOTRA_TRACE=1` で `egui_search:dispatch` の打鍵→dispatch 遅延を前後比較（悪化上限は理論値 6.9ms 以内か）。加えて目視（打鍵エコー・ホバーハイライト・カテゴリ D）。
3. **AC3**: main 側でも同プロトコル（ポインタ移動時 fps が cap 以下・非移動時 12.1fps 相当から悪化しない）。**可視アイドル 2.0fps / 0.59% が不変**であること（gate は遅延を足すだけで頻度を上げられないが、実測で接地する）。
4. **カスケードの接地**: 144Hz 実機で `min_interval` ≈ 6.94ms が選ばれたことを確認する手段が要る。REPAINT 間隔の下限から間接推定できるが、選択値を直接出す trace 1 行（既存 env ゲート下・一時計装でも可。恒久化するなら `PERFORMANCE.md` の計器リストへ追記）を推奨。60Hz フォールバックはユニットテスト（純粋核）で固定し、実機では代表 1 ケースのみ。
5. **回帰**: `cargo clippy` + crate テスト（PostToolUse hook が自動・沈黙 = 合格）、`npm run smoke:egui`、カテゴリ D 目視（runtime 変更は 2 ウィンドウ全部に効くため範囲が広がる・issue 注意欄）。

**純粋核テスト（Red から書く）**:
- gate: (a) 要求 deadline が next_allowed より過去でも発火は next_allowed 以降 (b) dispatch 後 next_allowed が min_interval 前進 (c) 長い無風後の要求は即時発火（next_allowed が過去） (d) coalescing（早い方を採る）と clamp の直交性 (e) min_interval 変更が次 dispatch から効く。
- カスケード: current 有効値 → そのまま／current 0・1・None → registry／registry も無効 → 60Hz。境界: 1Hz は無効値、2Hz は有効値として通る（0/1 のみ特別扱い）。

## 6. 落とし穴・注意点

1. **worker へ `egui::Context` や `RepaintScheduler` の参照を渡さない**（`snotra-egui-runtime/CLAUDE.md` 不変条件）。モニター取得は plugin（イベントループスレッド）で行い、worker へは `Arc<AtomicU64>` の値だけを渡す設計がこの不変条件と両立する唯一の形。取得を worker 側でやろうとすると hwnd の Send 問題（`windows_ime.rs` の `hwnd_value: isize` と同じ罠）を踏む。
2. **gate を `render()` 側に置かない**: 起床コスト（proxy send → `RedrawRequested` 配送）を払ってから捨てることになる。合流点（worker）一択という spec §4 の判断に独立導出でも一致。また `render()` 側で間引くと系統 B（OS が「描け」と言った damage フレーム）まで捨てて表示破損する。
3. **`recv_timeout` の timeout 計算**: `deadline.saturating_duration_since(now)` の deadline を `max(pending, next_allowed)` に差し替えるとき、期限到来（`Timeout`）後の再判定でも同じ max を取り直すこと。片方だけだと早発 dispatch する。
4. **windows crate はこの crate では `=0.61.3`**（src-tauri の 0.62 と別バージョン）。`GetMonitorInfoW` / `EnumDisplaySettingsW` のシグネチャ（`BOOL` 戻り・`PCWSTR`）は 0.61.3 で確認してから書く（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」の一般則）。src-tauri の `monitor.rs` は前例として読むだけで、bin crate ゆえ依存共有はできない——重複実装になるが対象 API が違う（work area vs refresh rate）ので `/dry-check` 上は別概念。
5. **`Moved` を egui 入力へ流さない**: 再取得のフックは `RuntimePlugin::on_event` の `WindowEvent` arm（または `EguiWindow::on_window_event`）に置くが、`input.rs` が `Moved` を消費しない現状を変えない（余計な repaint 要求を生まない）。
6. **AtomicU64 の単位はナノ秒で統一**し、書き込み側で 0 を排除、読み側でも 0 → 60Hz の防御を入れる（ゼロ除算・ゼロ間隔スピンの二重防御)。
7. **既存テストの命題を孤立させない**（AGENTS.md ワークフロー 5）: `wake_before_activation_is_queued` 等が証明する「活性化前 wake の queue」「Stop 契約」は gate 導入後も同じ命題のまま成立することを確認（チャネル層なので無改変で成立する見込み）。
8. **該当スキル**: worker・channel・共有状態（AtomicU64）の変更 → `/race-check`。新規関数 → `/dry-check` + 呼び出し元 grep。`/symmetric-check` は生成/破棄ペアの新設なし（Arc は ActiveWindow の Drop で自然解放）だが、`RepaintScheduler::new` のシグネチャ変更で呼び出し元 1 箇所の compile-fail を移行検出器に使う。
9. **実施順**: spec §8 のとおり #714 の後に実測すること（アニメーションフレームの除去前に測ると上限の寄与に #714 分が混入し、AC1 の帰属が読めない）。
10. **`gh pr create` は `git push` と `&&` で繋ぐ**・PR 本文の closing keyword は `#737` のみ意図的に（#710 は Refs であって close しない——マージ直前に `closingIssuesReferences` を必ず確認）。
