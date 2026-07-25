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
