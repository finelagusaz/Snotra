# Plan: マルチモニター・高DPI環境でのウィンドウ挙動 (#225)

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/main.rs` | `show_main_and_emit` にカーソルモニター判定 + ウィンドウ移動を追加。setup の位置復元にモニター存在チェックを追加 |
| `SPEC.md` | §7.2 にマルチモニター位置復元ルール・高DPI方針を追記 |

## 実装順序

### Phase 1: カーソルモニターへの表示

`show_main_and_emit()` に以下のロジックを追加（`show()` の前）:

1. `GetCursorPos` でカーソルの物理座標を取得
2. `MonitorFromPoint(cursor_pt, MONITOR_DEFAULTTOPRIMARY)` でカーソルのモニターを取得
3. `GetMonitorInfoW` でそのモニターの作業領域 `rcWork` を取得
4. ウィンドウの現在位置を `outer_position()` で取得（物理座標）
5. ウィンドウの現在位置がカーソルのモニター内にあるか判定
6. モニターが異なる場合、カーソルモニターの作業領域の中央にウィンドウを移動

**判定ロジック**: ウィンドウの左上座標 (x, y) が `rcWork` の矩形内に含まれるかで判定。
含まれない = 別モニター上にある → カーソルモニターの中央に移動。

**座標変換**: `GetMonitorInfoW` は物理座標を返す。Tauri の `set_position(Physical(...))` を使えば変換不要。
ウィンドウ幅は `inner_size()` (物理) で取得し、中央計算に使う。

```
center_x = rcWork.left + (rcWork.right - rcWork.left - window_width) / 2
center_y = rcWork.top + (rcWork.bottom - rcWork.top - window_height) / 2
```

### Phase 2: 位置復元時のモニター存在チェック

`main.rs` の setup フェーズ（L338-344）で `load_search_placement()` 後:

1. 復元座標を物理座標に変換（`scale_factor` 使用）
2. `MonitorFromPoint(pt, MONITOR_DEFAULTTONULL)` で座標のモニター存在を確認
3. NULL の場合 → 位置復元をスキップ（Tauri デフォルト = 中央表示）

### Phase 3: SPEC.md 更新

§7.2 に以下を追記:
- ホットキー押下時はマウスカーソルのあるモニターに表示（ウィンドウが別モニター上なら移動）
- 位置復元時にモニター存在を検証し、画面外ならデフォルト位置にフォールバック
- 高DPI対応は Tauri/WebView2 のデフォルト挙動に委ねる

## 不変条件

1. **show_main_and_emit の冪等性**: モニター移動は show() の前に実行。移動しない場合（同一モニター）は何もしない
2. **座標系の一貫性**: Win32 API（物理座標）と Tauri API（論理/物理選択可）の変換を明示
3. **GetCursorPos / MonitorFromPoint の失敗**: 失敗時はフォールバック（移動しない / デフォルト位置）。クラッシュしない
4. **既存の位置保存には影響しない**: `onMoved` による保存ロジックは変更なし。保存は引き続き論理座標
5. **Win32 API は同期呼び出し**: `GetCursorPos`, `MonitorFromPoint`, `GetMonitorInfoW` はすべて同期 API。platform スレッド経由不要、デッドロックリスクなし

## テスト方針

- Win32 依存のため自動テストは困難（`src-tauri/src/` の Win32 モジュールはユニットテスト前提にしない）
- `cargo check -p snotra -p snotra-core` で型チェック
- `MonitorFromPoint` の `MONITOR_DEFAULTTONULL` / `MONITOR_DEFAULTTOPRIMARY` の使い分けをコードコメントで明示
- 手動検証シナリオ:
  1. シングルモニター: ホットキーで通常表示（位置記憶あり）
  2. マルチモニター: カーソルが別モニター → そのモニターの中央に表示
  3. マルチモニター: カーソルと同じモニター → 記憶位置で表示
  4. モニター切断後: 記憶位置のモニターが無い → デフォルト位置で表示

## SPEC.md 更新要否

あり。§7.2 に追記（上記 Phase 3 参照）。

---

## セルフレビュー

1. **対称コードパス**: `show_main_and_emit` のみ変更。`hide` 側は位置に関与しないため変更不要 ✓
2. **影響範囲の網羅性**: `show_main_and_emit` の全呼び出し元（hotkey toggle, alt-wait, show_on_startup, tray double-click）で同じコードパスを通る ✓
3. **境界条件**: GetCursorPos 失敗、MonitorFromPoint NULL、単一モニター環境 — すべてフォールバック設計 ✓
4. **リソース管理**: 新規リソース（ハンドル等）なし。HMONITOR はシステム管理で解放不要 ✓
5. **既存パターンとの整合**: `GetCursorPos` は tray.rs で使用済み。同じ Win32 feature で動作 ✓
6. **YAGNI 違反**: Per-Monitor DPI 独自ハンドリングは実装しない（Tauri/WebView2 に委ねる）✓
7. **シンプル化**: 新たな状態（AtomicBool, Mutex 等）は導入しない。show の直前に一回判定するだけ ✓
8. **破壊不変条件**: `show_main_and_emit` のタイミング（show 前にモニター判定 → set_position → show）。Win32 API 失敗時は何もしない（フォールバック）ため、既存動作を壊さない ✓
