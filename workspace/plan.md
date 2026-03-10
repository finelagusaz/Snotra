# Plan: マルチモニター・高DPI環境でのウィンドウ挙動 (#225)

## 設計方針（ユーザーとの合意事項）

1. **座標はモニター原点からの相対座標で1つ保持**（ディスプレイ単位の保持はしない）
2. **設定で表示先モニターを選択可能**: プライマリ固定 / カーソル追尾（デフォルト: カーソル追尾）
3. **アスペクト比・DPI 違いへの対応**: ターゲットモニターの作業領域にクランプ（はみ出し分を押し戻し）
4. **高DPI**: Tauri/WebView2 のデフォルト挙動に委ねる（独自DPI処理なし）

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `snotra-core/src/config.rs` | `GeneralConfig` に `follow_cursor_monitor: bool` 設定を追加（デフォルト: true） |
| `snotra-core/src/window_data.rs` | `WindowPlacement` を絶対座標からモニター相対座標に変更（バージョンバンプ + 旧フォーマットフォールバック） |
| `src-tauri/src/main.rs` | `show_main_and_emit` にモニター判定 + 相対座標→絶対座標変換を追加。setup の位置復元にモニター存在チェック + クランプを追加 |
| `src-tauri/src/config_watcher.rs` | `follow_cursor_monitor` 変更検出（イベント発火不要 — Rust 側で直接読む） |
| `src-tauri/src/commands/window.rs` | `save_search_placement` を相対座標保存に変更 |
| `ui/src/MainApp.tsx` | `onMoved` の保存ロジックを相対座標計算に変更（または Rust 側で変換） |
| `SPEC.md` | §7.2 にマルチモニター位置復元ルール・設定・高DPI方針を追記 |

## 実装順序

### Phase 0: 設定追加

`snotra-core/src/config.rs`:
- `fn default_follow_cursor_monitor() -> bool { true }` を追加
- `GeneralConfig` に `#[serde(default = "default_follow_cursor_monitor")] pub follow_cursor_monitor: bool` を追加
- `impl Default for GeneralConfig` に `follow_cursor_monitor: true` を追加

`src-tauri/src/config_watcher.rs`:
- `follow_cursor_monitor` 変更を検出し、Engine config を更新（既存の `update_config` で自動反映）
- フロントエンドへのイベント発火は不要（Rust 側の `show_main_and_emit` で Engine から直接読む）

### Phase 1: WindowPlacement を相対座標に変更

`snotra-core/src/window_data.rs`:
- `WindowPlacement { x: i32, y: i32 }` の意味を「モニター作業領域原点からの相対座標（論理）」に変更
- バージョンを V4 → V5 にバンプ
- V4 フォールバック: 旧形式（絶対座標）を読み込んだ場合は `None` を返す（次回保存時に新形式で上書き）
  - 旧絶対座標をモニター相対に変換するのは複雑なため、単純に破棄してデフォルト位置にフォールバック

### Phase 2: 保存ロジック変更

ウィンドウ位置の保存時に絶対座標→相対座標に変換する。

**方針 A（Rust 側で変換）**: `save_search_placement` コマンドで受け取った絶対論理座標を、ウィンドウが現在いるモニターの作業領域原点で引く → UI 側の変更不要
**方針 B（UI 側で変換）**: UI 側でモニター情報を取得して相対化 → Tauri API では困難

→ **方針 A を採用**。`save_search_placement` の IPC ハンドラ内で Win32 API を使い相対座標に変換してから保存。

`src-tauri/src/commands/window.rs`:
```rust
#[tauri::command]
pub fn save_search_placement(x: i32, y: i32) {
    // x, y は論理絶対座標。物理座標に変換してモニター特定 → 作業領域原点を引く → 論理相対座標で保存
    let relative = absolute_to_relative(x, y);
    window_data::save_search_placement(relative);
}
```

Win32 API 呼び出し:
1. 論理座標 (x, y) を物理座標に変換（scale_factor 掛け算 — ただし正確な変換には対象モニターの DPI が必要）
2. `MonitorFromPoint(physical_pt, MONITOR_DEFAULTTONEAREST)` でモニター取得
3. `GetMonitorInfoW` で `rcWork` 取得
4. 相対座標 = (x - rcWork.left/sf, y - rcWork.top/sf)

**簡略化**: Tauri の `onMoved` は論理座標を返す。`MonitorFromPoint` は物理座標を取る。変換に DPI が要る。
→ より安全なアプローチ: IPC で論理座標を受け取るのではなく、**Rust 側でウィンドウの HWND から直接モニターを取得**して計算する。

```rust
fn save_relative_placement(window: &WebviewWindow) {
    let Ok(pos) = window.outer_position() else { return }; // PhysicalPosition
    let Ok(hwnd) = window.hwnd() else { return };
    // MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
    // GetMonitorInfoW → rcWork
    // relative_x = pos.x - rcWork.left
    // relative_y = pos.y - rcWork.top
    // 論理座標に変換して保存
}
```

→ UI 側は `saveSearchPlacement(x, y)` をそのまま呼ぶが、**Rust 側で物理座標ベースで計算**するため、引数の x, y は使わず `window.outer_position()` を使う。
→ さらに簡略化: **IPC 引数を消して、Rust 側でウィンドウ位置を直接読み取る**。

`save_search_placement` のシグネチャ変更:
- 旧: `fn save_search_placement(x: i32, y: i32)`
- 新: `fn save_search_placement(app: AppHandle)` — ウィンドウから位置を取得

UI 側 (`MainApp.tsx`): `saveSearchPlacement()` に引数なしで呼ぶ。

### Phase 3: 復元ロジック変更（`show_main_and_emit`）

`show_main_and_emit` の `show()` 前に以下を追加:

1. `AppState.engine` から `follow_cursor_monitor` を読む
2. **表示先モニターの決定**:
   - `follow_cursor_monitor == true`: `GetCursorPos` → `MonitorFromPoint(MONITOR_DEFAULTTOPRIMARY)` でカーソルのモニター
   - `follow_cursor_monitor == false`: プライマリモニター（`MonitorFromPoint(origin, MONITOR_DEFAULTTOPRIMARY)`）
3. `GetMonitorInfoW` でターゲットモニターの `rcWork` を取得
4. `load_search_placement()` で相対座標を読む
5. 相対座標をターゲットモニターの `rcWork` 基準で絶対座標に変換:
   ```
   abs_x = rcWork.left + relative_x * sf
   abs_y = rcWork.top + relative_y * sf
   ```
   ※ 実際には物理座標で計算し `set_position(Physical(...))` を使う
6. **クランプ**: ウィンドウがターゲットモニターの作業領域からはみ出す場合は押し戻す
   ```
   abs_x = clamp(abs_x, rcWork.left, rcWork.right - window_width)
   abs_y = clamp(abs_y, rcWork.top, rcWork.bottom - window_height)
   ```
7. `set_position` で配置
8. 保存位置がない場合（初回起動・V4→V5 移行後）: モニター中央に配置

### Phase 4: setup フェーズの位置復元

`main.rs` setup（L338-344）の既存ロジックを変更:

- `load_search_placement()` で得た相対座標を、**初回表示のモニターで絶対化**する
- ただし setup 時点ではまだウィンドウが非表示で、カーソル位置モニターを使うのが適切
- `show_main_and_emit` が初回 show 時にも呼ばれるため、**setup での位置復元を削除し、`show_main_and_emit` に一元化**する
  - setup では幅の設定のみ残す（L345-354）
  - 位置は毎回 `show_main_and_emit` で決定

### Phase 5: SPEC.md 更新

§7.2 に追記:
- ウィンドウ位置はモニター作業領域原点からの相対座標で保存
- ホットキー押下時の表示先: `follow_cursor_monitor` 設定に基づきカーソルモニター or プライマリモニター
- ターゲットモニターの作業領域にクランプして表示
- 高DPI対応は Tauri/WebView2 に委ねる

§17.2（設定構造）に `follow_cursor_monitor` を追記。

## 不変条件

1. **show_main_and_emit が位置決定の単一責務**: setup での位置復元を廃止し、`show_main_and_emit` に一元化
2. **座標系**: 保存は論理相対座標、Win32 API は物理座標、Tauri `set_position` は Physical を使用
3. **クランプの保証**: どのモニター構成でもウィンドウは必ず画面内に表示される
4. **Win32 API 失敗時のフォールバック**: 失敗時は位置変更しない（既存動作維持）。クラッシュしない
5. **設定の読み取り**: `follow_cursor_monitor` は `show_main_and_emit` の呼び出し時に `AppState.engine` から毎回読む（キャプチャしない）
6. **バージョン移行**: V4 → V5 で旧絶対座標は破棄。初回は中央表示になり、以降は相対座標で保存
7. **HMONITOR は解放不要**: システム管理リソース

## テスト方針

- `snotra-core`:
  - `window_data.rs`: V5 roundtrip テスト + V4 フォールバック（None 返却）テスト
  - `config.rs`: `follow_cursor_monitor` のデフォルト値テスト（既存の config テストパターンに合わせる）
- Win32 依存部分は自動テスト対象外
- `cargo check -p snotra-core -p snotra -p snotra-settings` で型チェック
- 手動検証シナリオ:
  1. シングルモニター: ホットキーで記憶位置に表示
  2. マルチモニター + カーソル追尾: カーソルのモニターに表示、位置は相対座標で再現
  3. マルチモニター + プライマリ固定: 常にプライマリに表示
  4. サイズ違いモニター: 大モニターで保存した位置が小モニターでクランプされる
  5. モニター切断後の起動: デフォルト位置で表示（V4→V5 移行と同じ）
  6. `config.toml` で `follow_cursor_monitor` を切り替え: 次回ホットキーから反映

## SPEC.md 更新要否

あり（Phase 5 参照）。

---

## セルフレビュー

1. **対称コードパス**: `show` に位置決定を追加。`hide` は無関係。`save` は保存時の相対化。show/save が対称ペアとして整合 ✓
2. **影響範囲の網羅性**:
   - `show_main_and_emit` の全呼び出し元（hotkey, alt-wait, show_on_startup, tray）で同一パス ✓
   - `save_search_placement` の呼び出し元は `MainApp.tsx` の `onMoved` のみ ✓
   - `load_search_placement` の呼び出し元: setup（削除予定）+ show_main_and_emit（新設） ✓
3. **境界条件**: Win32 API 失敗、保存位置なし、V4 旧データ、シングルモニター、小モニターでのクランプ ✓
4. **リソース管理**: HMONITOR は解放不要。新たなスレッド・プロセス・リスナーの追加なし ✓
5. **既存パターンとの整合**: `GetCursorPos` は tray.rs で使用済み。`MonitorFromPoint` / `GetMonitorInfoW` は `Win32_Graphics_Gdi` feature で利用可能 ✓
6. **YAGNI 違反**: ディスプレイ単位保持は見送り（合意済み）。DPI 独自処理なし ✓
7. **シンプル化**: 新たな AtomicBool / Mutex は不要。`follow_cursor_monitor` は Engine config から毎回読むだけ。setup の位置復元を `show_main_and_emit` に一元化して責務を集約 ✓
8. **破壊不変条件**: `show_main_and_emit` のタイミング（位置決定 → 高さリセット → show → focus）は既存の順序を壊さない。Win32 API 失敗時は位置変更をスキップ ✓
9. **window.bin バージョンバンプ**: V4→V5。旧データは None 返却でフォールバック。`save_state` は常に V5 で書く ✓
10. **config_watcher との整合**: `follow_cursor_monitor` は `update_config(new_config)` で Engine に反映される。`show_main_and_emit` は Engine から読むため、config.toml 変更が次回 show から反映される ✓
