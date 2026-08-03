# 測定: `config-applied` の wake で main のフレームが走るか（#751 Phase 0）

日付: 2026-08-03 / ビルド: `target/debug/snotra.exe`（main = 53ad731 相当）

## 問い

色だけの config 変更で main のフレームが**何枚**走るか。0 枚なら `ui.visuals_mut()` への
置換だけでは症状が消えず、`ctx.request_repaint()` も要る。

## 方法

スクリプト: `scratchpad/measure-751-repaint.ps1`（使い捨て）。

- `SNOTRA_CONFIG_DIR` で `target/measure-751/profile` を指す（**実 config には触れていない**）
- seed: `show_on_startup = true` / `auto_hide_on_focus_lost = false` /
  `background_color = "#4A2B5C"` / `input_background_color = "#203040"`
- `SNOTRA_EGUI_REPAINT_TRACE=1` + `SNOTRA_TRACE=1` で起動し stderr をファイルへ
- `egui_show:done` を観測後、**前面を元の窓へ戻して main を unfocused にする**
  （focused だと egui のキャレット点滅 repaint が計数を汚す）
- main のフレームが 3 秒連続で増えなくなるまで待つ（静穏化）
- `config.toml` の `input_background_color` **だけ**を `#203040` → `#803020` へ書き換える
- 4 秒待って `SNOTRA_EGUI_REPAINT window=main` の増分を数える

**issue の再現条件（設定サイドカー存命）ではなく `auto_hide_on_focus_lost = false` を使った。**
どちらも作る状態は同じ（main が可視のまま unfocused）で、wake 経路
（`register_config_wake_listeners` → `wake_main`）は両者で同一だからである。

## 結果

```
静穏化: main のフレーム累計 15 行（直近 3 秒は増加なし）
config.toml を書き換えた（input_background_color: #203040 → #803020）

書き換え前の main フレーム累計: 15
書き換え後の main フレーム累計: 17
増分: 2

--- 増分の行 ---
  SNOTRA_EGUI_REPAINT window=main focused=false since_prev_ms=3404.7 causes=-
  SNOTRA_EGUI_REPAINT window=main focused=false since_prev_ms=101.9 causes=-
```

seed は読めている（`[config] ` で始まる行は 0）。results 窓のフレームは 0（未表示のため）。

## 判定

**フレームは走る。** 3.4 秒アイドルだった main が、config 書き換え直後に描き直されている。
`causes=-`（egui 内部の repaint 要求は空）＝**外部 wake によるフレーム**であり、
`wake_main` 経由という読みと整合する。

→ **`ui.visuals_mut()` への置換だけで足りる。`ctx.request_repaint()` は足さない。**

## 注記（この測定が答えていないこと）

- **増分が 2 枚だったのは、この書き換え方に固有の可能性が高い。** 2 枚目は 1 枚目の
  101.9ms 後で、`config_watcher` の debounce（100ms）をわずかに超えている。
  PowerShell の `Set-Content` が truncate + write の 2 つの変更イベントを出し、
  debounce が 2 回成立したと読むのが自然である。**設定 UI からの保存が何イベントを出すかは
  測っていない。**
- ゆえに「現行コードでは 1 枚しか走らないから症状が出る」という**機序の後半は未確認**である。
  ただし本 issue の判断に要るのは「**0 枚ではない**」ことだけであり、それは確定した。
