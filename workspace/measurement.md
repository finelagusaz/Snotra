# 実測: #628 Phase 2 — 可視アイドルの再描画（2026-07-26）

ログ: `%TEMP%\628-idle.log`（5075 行）/ release ビルド `target/release/snotra.exe` / 計器 `SNOTRA_EGUI_REPAINT_TRACE` + `SNOTRA_EGUI_PAINT_TRACE` + `SNOTRA_TRACE`。

## 結果（数字）

| 項目 | 値 |
|---|---|
| REPAINT 行 / PAINT 行 | 2533 / 2533（全フレームが実描画に至っている） |
| 窓別フレーム数 | main 1194 / results 1339 |
| `focused=false` の行 | main 23 行のみ（判定を汚す長さではない） |
| **可視中の main の定常フレームレート** | **約 11.5 fps**（5 秒あたり 55〜60 フレームが 70 秒間ほぼ一定） |
| バースト構造（main） | 154 バースト・**1 バースト平均 7.8 フレーム / 26.3ms**・バースト内間隔の中央値 **3.7ms** |
| バースト間の間隔 | **480ms が 129/141 件**（他は入力・世代検知由来の少数） |
| paint コスト | `total_ms` 平均 1.41ms（`raster_ms` p50 1.63 / p95 1.71 / 最大 2.30） |
| **可視アイドルの CPU 占有** | 合計 3,578ms / 70s = **約 5.1%（1 コア比）** |
| hidden 30 秒 | `egui_hide:done` 後のフレーム **0**（SU6.5 G3(a) 再確認・合格） |

原因（`repaint_causes()`）の内訳:

| 窓 | 原因 | 件数 |
|---|---|---|
| main | `egui/text_selection/visuals.rs:313`（キャレット点滅） | 1162 |
| results | `-`（egui 由来の要求なし＝`wake_results` / `SetWindowPos` による外部起床） | 950 |
| results | `egui/containers/scroll_area.rs:1473`（スクロール領域） | 280 |
| main | `egui/context.rs:525` ほか少数 | 11 |

**main の原因はほぼ常に点滅である**——`paint_text_cursor` は毎パス無条件に `request_repaint_after` を撃つため、cause 欄は「引き金」ではなく「毎フレーム積まれる常在項」として読む必要がある。引き金の識別はバースト構造（下記）が担う。

## 機構（数字が指す唯一の説明）

キャレット点滅が源であることは判定表の (b) に当たる。ただし **2fps ではなく約 11.5fps** で、差の約 6 倍は次の増幅による:

1. `paint_text_cursor`（egui `text_selection/visuals.rs:313`）が `request_repaint_after(wake_in)` を撃つ。`wake_in` は点滅サイクルの残余で、**境界に近づくほど 0 に近づく**
2. egui の `request_repaint_after` は `delay - predicted_dt` へ短縮する（`context.rs:148-151`）。**本 runtime は `RawInput::predicted_dt` を一度も書かない**（`input.rs:29-56` の `take` は screen_rect / max_texture_side / time / viewport のみ）ため、既定の **1/60 秒 = 16.7ms** のままである
3. ゆえに `wake_in < 16.7ms` の領域では delay が **ZERO へ飽和**する。ZERO は「即時再描画」であり、さらに `outstanding = 1` を立てて**次パスでもう 1 枚**生む（`context.rs:137-145`）
4. 即時再描画された次のパスは、さらに小さい `wake_in` を計算して再び ZERO——**点滅の遷移ごとに約 26ms スピンする**

観測との一致:

- バースト間 **480ms** = 点滅 0.5s − predicted_dt 16.7ms ✓
- バースト持続 **26.3ms** ≈ 飽和領域 16.7ms を 3.7ms/フレームで抜ける時間 ✓
- バースト内 **7.8 フレーム** ≈ 飽和領域のフレーム数 × `outstanding` の 2 倍化 ✓

**results 窓が main に追随する**のは `view.rs:850` の `wake_results`（`drive_results_window` 末尾で無条件）による。results の原因が `-` に偏るのはそのため（egui 由来の要求ではなく外部起床）。加えて `position_results_below_main` が main のフレームごとに `SetWindowPos` を撃つ。**main のフレームが増えれば results のフレームと Win32 呼び出しも比例して増える。**

## 判定

- 判定表の **(b)（源はキャレット点滅）が当たる**。ただし増幅機構が併存するため、(d) の性質も併せ持つ
- **私が Phase 3 の分岐 B を勧めたときの前提「2fps・CPU 1% 未満」は誤りだった**。実測は約 11.5fps・**5.1%**（2 窓合計・1 コア比）で、実額は約 5 倍である
- **hidden の停止は健全**（0 フレーム）。#579 型の自己永続ループも無い（バースト間は確実に眠っている）

## 選択肢（実測後に更新）

| 案 | 内容 | 期待アイドル | 副作用 |
|---|---|---|---|
| A | `text_cursor.blink = false` | **0 fps** | キャレット非点滅（IME 変換中も）・WebView2 parity gap・SPEC 同期要 |
| B | 現状受容 | 11.5 fps / 5.1% | なし（ただし前提だった「1% 未満」は成り立たない） |
| **C** | `input.rs::take` で `predicted_dt` を設定し、スピンを消す | **2 fps 前後**（点滅そのものは残る） | 点滅・parity は維持。egui 内の `predicted_dt` 利用箇所（アニメーション補間）への影響を要確認 |

C は「点滅を維持したまま増幅だけを消す」ため、A と B の双方を支配しうる。ただし `predicted_dt` は egui のアニメーション補間にも使われるため、**値を決める前に利用箇所を洗う必要がある**（0 が正しいのか、実測フレーム時間が正しいのか）。

## 決定と実装（2026-07-26・C 採用）

`predicted_dt` の利用箇所を egui 0.35 で洗った結果、**0 を渡して安全**と判断した:

| 利用箇所 | 用途 | 0 のときの挙動 |
|---|---|---|
| `context.rs:148-151` | `request_repaint_after` の切り詰め | **切り詰めなし＝スピンが消える**（狙い） |
| `animation_manager.rs:91` / `area.rs:634` | アニメーションの半フレーム先読み（`+ predicted_dt / 2.0`） | 先読みなし。乗算のみ・除算なし |
| `input_state/mod.rs:379-385` | sleep 明けフレームの `stable_dt` フォールバック | その 1 フレームだけ経過 0。以降はアニメ中＝即時 repaint 要求ゆえ実測 dt が使われる。消費側は `at_most(0.1)` 付きの乗算のみ |
| `input_state/mod.rs:376` | `time` 未指定時の補完 | 本 runtime は `time` を毎フレーム渡すため不使用 |

実装（いずれも `snotra-egui-runtime`・コミットは分離）:

1. `input.rs::take` で `self.raw.predicted_dt = 0.0`（+ 回帰テスト `take_reports_zero_predicted_dt_so_repaint_delays_are_not_truncated`）
2. `runtime.rs::handle_platform_output` のカーソル適用を変化検出化（**別欠陥**・下記）

**副作用として全ての遅延予約が最大 16.7ms 遅くなる**（これまでが早すぎた）。影響が読める箇所:

- `view.rs:1305-1321` の blur 猶予 100ms は、これまで実質 83.3ms で起きて `grace_elapsed` が false になりえた。0 にすると 100ms 以降に起きるため**判定が素直に通る**（plan-review が「再要求経路が無い」と指摘した脆さの緩和方向）
- 検索 debounce 50ms・通知期限・launch timeout 4s はいずれも 16.7ms の遅れが体感・論理に影響しない

## 併発して発見した別欠陥: カーソル形状の点滅（同ブランチで修正）

計測中、**入力欄にマウスを乗せるとカーソルが矢印とビームで高速に切り替わる**とユーザーが報告。原因は #628 とは別:

- tao の `set_cursor_icon` は窓に紐づかない `SetCursor` を直接撃つ（tao 0.35.3 `platform_impl/windows/window.rs:460-466`）。最後に呼んだ者が勝ち、マウス静止中は `WM_SETCURSOR` が来ないので OS の復元も入らない
- `handle_platform_output` はこれを**毎フレーム無条件**に呼んでいた。ポインタを持つ main（`Text`）と持たない results（`Default`）が交互に上書きし合う

**症状の激しさは #628 のフレームレートに比例するが、根は独立**（C だけでは 2Hz の点滅として残る）。窓ごとに最後に適用した `egui::CursorIcon` を保持し、変化時だけ呼ぶ修正を入れた。

## 再測（Phase 2′）で確認すること

1. main の可視アイドルが **11.5fps → 2fps 前後**（バーストが消え、間隔が 500ms 前後に揃う）
2. results のフレームが main に比例して減る（`wake_results` 経由）
3. CPU 5.1% → 1% 前後
4. **カーソル点滅が止まる**（目視）
5. 回帰が無いこと: blur→hide が効く・検索の応答・IME 変換・hidden で 0 フレーム
