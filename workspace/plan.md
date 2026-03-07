# Plan — 結果ウィンドウの高さを最大表示件数の高さに固定する (#165)

## 変更ファイル一覧

1. **`src-tauri/src/commands/config.rs`** — `BootstrapAppearanceConfig` に `max_results` を追加
2. **`ui/src/lib/types.ts`** — `BootstrapAppearanceConfig` に `max_results` を追加
3. **`src-tauri/src/config_watcher.rs`** — `max_results` 変更時に `max-results-changed` イベントを emit
4. **`ui/src/lib/resultsWindowController.ts`** — `cachedMaxResults` を追加し高さ計算を固定化。Bootstrap + イベントで更新
5. **`ui/src/MainApp.tsx`** — `max-results-changed` リスナーを追加し controller に伝達。bootstrap から初期値設定

## 実装順序

### Phase 1: Rust 側 — BootstrapPayload に max_results を追加

**`src-tauri/src/commands/config.rs`**:
- `BootstrapAppearanceConfig` に `max_results: usize` フィールドを追加
- `get_bootstrap_payload()` で `engine.config().appearance.max_results` を設定

### Phase 2: Rust 側 — config_watcher で max_results 変更を通知

**`src-tauri/src/config_watcher.rs`**:
- `max_results` 変更検知を追加: `new_config.appearance.max_results != old_config.appearance.max_results`
- 変更時に `app.emit("max-results-changed", new_max_results)` を emit

### Phase 3: フロント型定義 — BootstrapAppearanceConfig を更新

**`ui/src/lib/types.ts`**:
- `BootstrapAppearanceConfig` に `max_results: number` を追加

### Phase 4: resultsWindowController — 高さ計算を固定化

**`ui/src/lib/resultsWindowController.ts`**:
- `cachedMaxResults` 変数を追加（初期値 8 = デフォルト）
- `updateMaxResults(maxResults: number)` メソッドを追加
- `handleDataChanged` の高さ計算を変更:
  - Before: `Math.min(count * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2, 400)`
  - After: `cachedMaxResults * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2`
  - **件数に関わらず固定高**。件数 > maxResults 時はスクロールバーが CSS で自然に表示
- **shouldShow 判定は変更なし**: `shouldShow: items.length > 0` で件数 0 時は hide（issue 要件）

### Phase 5: MainApp — 初期値設定 + 変更リスナー

**`ui/src/MainApp.tsx`**:
- bootstrap 取得後に `controller.updateMaxResults(bootstrap.appearance.max_results)` を呼ぶ
- `max-results-changed` リスナーを追加: `controller.updateMaxResults(event.payload)`

## 不変条件

1. **件数 0 の場合は results ウィンドウを hide**: `shouldShow: items.length > 0` は変更しない
2. **件数 > 0 の場合は常に max_results ベースの固定高**: 件数が 1 でも 8 でも 20 でもウィンドウ高さは同じ
3. **config_watcher で max_results が変わったら即時反映**: 次回の handleDataChanged で新しい高さが適用される
4. **width 変更のパターン（config_watcher → Rust 直接 set_size）は変更しない**: height の管理は resultsWindowController が行う

## テスト方針

### 自動テスト
- `npm run build` — typecheck + vite build
- `npm test` — 既存テスト維持
- `cargo check -p snotra-core -p snotra -p snotra-settings` — Rust 型チェック

### E2E テスト（既存）
- `e2e/tauri.slash.e2e.ts:618` に `max_results` 変更の E2E テストが既にある（config.toml 書き換え → 表示件数確認）。高さ固定の変更でこのテストが壊れないことを確認

### 手動確認
- max_results = 8 で 3 件ヒット → ウィンドウ高さが 8 行分を維持し、下部に余白
- max_results = 8 で 10 件ヒット → スクロールバーが表示される
- max_results = 8 で 0 件 → ウィンドウが hide される
- 設定で max_results を変更 → 次回表示で新しい高さが適用される

## SPEC.md 更新要否

SPEC.md §3.5「最大列挙数」に以下を追記:
> 結果ウィンドウの高さは最大表示件数に基づく固定高とする。ヒット数が最大表示件数未満でも高さは維持され、超過時はスクロールバーを表示する。

## セルフレビュー

### 1. 対称コードパス
- `handleDataChanged` / `handleVisibilityChanged`: handleVisibilityChanged は高さ計算に関与しない（hide のみ）→ 変更不要 ✓
- BootstrapPayload / config_watcher: 初期値と変更通知の両方でカバー ✓

### 2. 影響範囲の網羅性
- 高さ計算は `resultsWindowController.ts:116` の1箇所のみ ✓
- max_results は Rust 側で検索結果の切り詰めに使われるが、それはフロントの高さ表示とは独立 ✓
- `config_watcher` の width 変更パス（results ウィンドウの `set_size`）: width のみ変更するので干渉しない。ただし、width 変更時に height が巻き込まれないよう、`logical.height` を保持する現行実装を確認済み ✓

### 3. 境界条件
- max_results = 1: 高さ = 30 + 16 = 46px
- max_results = 0: config バリデーションで 0 は拒否される（`config.rs:617`）→ 発生しない ✓
- フォルダ展開・ツール選択: ツール件数や folder entries が max_results を超えることがある → スクロールバーが出る ✓

### 4. リソース管理
- 新規リスナー `max-results-changed` を追加 → `unlistenFns` に push して cleanup ✓

### 5. 既存パターンとの整合
- BootstrapPayload 拡張: `show_icons` と同じパターン ✓
- config_watcher の emit: `show-icons-changed` と同じパターン ✓
- controller の cached 値: `cachedMainLogicalWidth` / `cachedMainLogicalHeight` と同じパターン ✓

### 6. YAGNI 違反
- なし。要求範囲に限定 ✓

### 7. シンプル化の挑戦
- 新しい状態は `cachedMaxResults` のみ。既存パターン踏襲で最小限 ✓
- `updateMaxResults` が呼ばれるだけで、次回 `handleDataChanged` で自然に反映される。即時リサイズは不要（表示中にリサイズするとちらつきの原因になるため、次回表示時に適用で十分）

### 8. 破壊不変条件の明示
- **results ウィンドウの高さが不正な場合**: 件数 0 で固定高のウィンドウが表示される可能性 → `shouldShow` ガードで 0 件時は hide されるので安全 ✓
- **config_watcher の emit 失敗**: `let _ =` で無視（他の emit と同じ）。次回 bootstrap で復帰 ✓
