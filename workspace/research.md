# research: #628 softbuffer render perf（fill_mesh 全画面塗り最適化・アイドル再描画）

調査日: 2026-07-26 / ブランチ: `chore/628-idle-repaint` / HEAD: `4e888df`

## issue の要約

SU1（PR #627）受け入れ検証中に観測された softbuffer render の 2 件:

1. **`fill_mesh` の全画面塗り最適化** — per-pixel の 3 除算を `inv_area` の乗算へ、単色・不透明・非テクスチャ mesh の fast path。期待「30ms → 数 ms」
2. **~200ms アイドル再描画の調査** — 無入力時も ~200-300ms 間隔（~4-5fps）で `RedrawRequested` が連続発火する。源の特定と「アイドル時は眠る」の確認

いずれも非ブロッキング。issue 記載の計測は **SU1 スパイク時（900×588 ≈ 529k px・`MvpView`）** のものである。

## 結論（先に置く）

- **項目 1 は実施しない**。SU6.5 の G3(b) が製品経路の release で実測し、`raster_ms` p95 = **4.68ms**（フレーム予算 16.7ms の 1/3 以下）で「fill_mesh 最適化不要」と記録済み。issue が前提にした 30ms は**スパイクの窓サイズと計測表示付き View での値**であり、製品では再現しない
- **項目 2 は残っている**。SU6.5 の G3(a) が測ったのは **hidden**（非表示 60s で paint 0 回）であり、issue の言う**可視アイドル**は測っていない。ただし観測源だった `MvpView`（毎フレーム計測表示）は #702 で撤去済みのため、**現象自体が消えている可能性がある**
- ゆえに本サイクルは「**測ってから決める**」（#532 SU6.5 設計 決定 6 と同じ刻み）。可視アイドルの repaint 要求源を計器で名指しし、残っていた場合のみ最小の措置を採る

## 既存の実測記録（項目 1 を閉じる一次証拠）

`docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md:37`（SU6.5 完了行）:

> **実測でゲート合格**: … G3(a) hidden 再描画停止（60s trim 保持で実測確認・**#628 は flip 非ブロッカー確定**）/ G3(b) 入力レスポンス（**raster_ms p95 4.68ms << 16.7ms・fill_mesh 最適化不要**）

裏付け（設計・計画側の条文）:

- `docs/superpowers/specs/2026-07-24-su6.5-flip-hardening-design.md:81-87`（決定 6「#628 は測ってから決める」・**無条件の `fill_mesh` 最適化は採らない**・「落ちたときだけ実施」）
- 同 `:99-100`（G3(a) は hidden の paint 回数、G3(b) は `raster_ms` p95 < 16.7ms が主判定）
- `docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:914`（見立て「製品窓は ≈185k px = 約 1/3 → 9〜15ms」を先に固定 → 実測 4.68ms でさらに下回った）

**G3(a) は項目 2 を閉じない**——測定条件は「Alt+Q で非表示 → 60 秒放置」であり（同 plan `:845-853`）、可視状態のアイドルは観測範囲外である。

## 関連コード（実在確認済み）

| ファイル | 位置 | 役割 |
|---|---|---|
| `snotra-egui-runtime/src/raster.rs` | `fill_mesh`（91行〜） | 項目 1 の対象。per-pixel `w/area` の 3 除算は 132 行に実在（issue の記述は正確） |
| `snotra-egui-runtime/src/renderer.rs` | `paint`（44行〜） | `SNOTRA_EGUI_PAINT_TRACE` 計器が既にある（SU6.5 Task 5a で追加・`tess_ms`/`raster_ms`/`total_ms`/`meshes`/`px`）。env 未設定なら `Instant` も取らない |
| `snotra-egui-runtime/src/repaint.rs` | `RepaintScheduler::new` の worker | `Request{deadline}` を最早期限へ畳んで `RequestRedraw` を投げる。**周期の源ではない**——要求が来なければ `recv()` で眠る |
| `snotra-egui-runtime/src/runtime.rs` | `attach_pending_windows`（240-242行） | `context.set_request_repaint_callback(move \|info\| callback_scheduler.request(info.delay))`。**egui の repaint 要求が唯一の駆動源**である |
| 同 | `EguiWindow::render`（299行〜） | `RedrawRequested` で `run_ui` → `paint`。`visible == false` なら早期 return（現在到達不能・受け口） |
| `src-tauri/src/egui_shell/view.rs` | `1522: response.request_focus()` | 検索 TextEdit は**可視中つねにフォーカスを持つ** |
| 同 | `1214-1219`（`ctx.set_visuals(visuals)`） | 毎フレームのテーマ適用点。`visuals.text_cursor.*` を触るならここ |
| 同 | `509: ctx.request_repaint_after(LAUNCH_TIMEOUT - elapsed)` | in-flight 起動中のみ（アイドルでは発火しない） |
| `src-tauri/src/egui_shell/notify.rs` | `remaining()`（73行付近） | 一時通知の表示中のみ `repaint_after` 予約（アイドルでは発火しない） |

## 周期 repaint の候補（コードから導いた仮説・実測前）

egui 0.35.0（`Cargo.toml:11` で `=0.35.0` 固定）の一次資料から:

1. **キャレット点滅（本命）** — `text_selection/visuals.rs:291-317 paint_text_cursor` が `ui.request_repaint_after_secs(wake_in)` を毎フレーム撃つ。`on_duration = off_duration = 0.5`（`style.rs:970-971`）ゆえ **~0.5s 周期＝2fps**。条件は「`is_mutable() && interactive`」かつ「viewport がフォーカスを持つ」（`text_edit/builder.rs:858-869`）。Snotra は hide-on-blur + 常時 `request_focus()` ゆえ**可視中は常にこの条件を満たす**
2. **`wake_results` の毎フレーム連鎖（増幅器）** — `src-tauri/src/egui_shell/view.rs:850` が `drive_results_window` 末尾で**無条件に** results 窓を起こす（結果非表示時は 829 行で早期 return するため空クエリでは発火しない）。main が 1 フレーム描くたび results も 1 フレーム描く＝**アイドルのコストが 2 窓ぶんになる**。`grep request_repaint` では到達しない**同概念・別名の間接参照**であり、plan-review の独立導出が拾った（#646 PR2 の 2 窓分割で入った・issue 執筆より後）
3. `ScrollArea` のスクロールバー fade（`animate_bool` 系）— 操作後の減衰のみ・定常アイドルでは止まる想定
4. `MvpView` の毎フレーム計測表示 — **#702 で撤去済み**（issue 観測時の 4-5fps を 2fps より速くしていた最有力候補）

**数字が合わないことを明記する**: 点滅だけなら ~500ms 周期であり、issue の観測 200-300ms とは**一致しない**。ゆえに「点滅が源だ」と決め打たない——判別子として `egui::Context::repaint_causes()`（`context.rs:1879`・`RepaintCause { file, line, reason }` を `Display` で `file:line reason` として出す）を使い、**源に名乗らせる**。`cfg(debug_assertions)` ゲートは無く release でも使える。

## 技術的制約

- `repaint_causes()` は **`prev_causes`**（直前 pass で確定した原因列）を返す `read()` + `Vec::clone()`。ゆえに (a) 読む位置は「その pass の `run_ui` 後」、(b) **env ゲートの内側に置く**（clone のコストを常時払わない——`renderer.rs` の既存計器が守っている「計測器が測定対象を汚さない」規範に揃える）
- 可視アイドルの観測は**実機の人手スモーク**でしか取れない（ホットキー起動 + フォーカス依存 + `GetAsyncKeyState`）。`smoke:egui` は既存の自動スモークだが観測窓の性質が違う。手順は `feedback_win32_input_trace_smoke` の型（env + stderr 捕捉 + 人間の実操作 + 件数照合）に倣う
- `repaint.rs` の worker に触ると `/race-check` トリガー（AGENTS.md 条件別チェック表: worker spawn・channel）。**計器を `renderer.rs` / `runtime.rs` の読み取り側に閉じれば scheduler の並行モデルには触れない**
- egui のキャレット点滅を止める手段は `Visuals::text_cursor.blink = false`（`style.rs`）。挙動変更（キャレットが点滅しなくなる）ゆえ SPEC 同期の要否を判断する必要がある

## 破壊不変条件（本変更のブラスト半径）

| 不変条件 | 正本 | 検知手段 |
|---|---|---|
| `RedrawRequested` を egui 入力へ渡さない（渡すと描画が自己永続ループ・#579 で 15 秒 2,000 フレーム） | `snotra-egui-runtime/CLAUDE.md` 不変条件 | `runtime.rs` の arm 分離を変えない + paint trace の件数照合 |
| hidden 中は paint 0 回（SU6.5 G3(a)・flip 基準の既取得分） | 同ロードマップ SU6.5 行 | SU6.5 のレシピ（`SNOTRA_TRACE` + Alt+Q 非表示 60s） |
| repaint worker は Drop で stop + join（外部 `WindowWaker` 保持でも停止する） | `snotra-egui-runtime/CLAUDE.md` / `repaint.rs` doc | 既存ユニットテスト（`repaint.rs` tests）・触らない方針 |

## 未解決の疑問（実測で解く）

- **可視アイドルの `RedrawRequested` は現在も周期発火するか**（`MvpView` 撤去後）。するなら周期は何 ms か、原因の `file:line` は何か
- 原因がキャレット点滅だった場合、**点滅を止めて完全に眠らせるか、点滅を維持して 2fps を受容するか**——UX と電力のトレードオフであり、コードから答えは出ない（→ plan で両分岐を用意し、ユーザー判断を仰ぐ）
- 2fps × `raster_ms` 4.68ms ≒ CPU 1% 未満の見積り。「眠らせる」価値は電力よりも**「アイドルで何も起きない」という不変条件を持てること**にある（#579 の自己永続ループのような回帰を件数 0 で検知できる）
