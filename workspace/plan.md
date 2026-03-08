# Plan — Issue #196: マウスカーソルでの検索リストのアイテム選択を明示的にしたい

## 設計方針（方式 C: ホバー選択廃止のみ）

マウスホバーによる `selected` 更新を廃止する。クリック起動は維持。
ホバー debounce タイマーは不要になるため削除。`handleClickResult`（起動）と `handleDoubleClickResult`（選択のみ）は変更しない。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `ui/src/MainApp.tsx` | `handleHoverResult` を削除。ResultsSection への `onHoverResult` prop を削除 |
| `ui/src/components/ResultsSection.tsx` | hover debounce タイマー（50ms）を削除。`onHoverResult` prop を削除 |
| `ui/src/components/ResultRow.tsx` | `onMouseEnter` prop を削除 |
| `SPEC.md` | マウス操作仕様を明記（ホバー＝視覚フィードバックのみ、クリック＝起動） |

計 4 ファイル、1 フェーズ。

## 実装詳細

### MainApp.tsx

- `handleHoverResult` 関数 (L218-219) を削除
- ResultsSection への `onHoverResult={handleHoverResult}` prop を削除
- `handleClickResult`（起動）と `handleDoubleClickResult`（選択のみ）は変更なし

### ResultsSection.tsx

- `onHoverResult` prop を interface から削除
- `handleHover` 関数と `hoverTimer` 変数を削除
- `onCleanup(() => clearTimeout(hoverTimer))` を削除
- ResultRow への `onMouseEnter` prop 転送を削除

### ResultRow.tsx

- `onMouseEnter?: () => void` prop を interface から削除
- `<div ... onMouseEnter={props.onMouseEnter}>` から `onMouseEnter` を削除

### SPEC.md

マウス操作の仕様を追記:
- ホバー: CSS `:hover` による視覚フィードバックのみ（`selected` 状態は変化しない）
- シングルクリック: アイテムを起動（既存動作を明文化）

## 不変条件

1. **`selected` シグナルはキーボードナビゲーション専用になる**: Arrow ↑↓ と Enter のフローは一切変更なし
2. **シングルクリック起動は `activateSelectedByIndex(index)` を経由**: 既存コードパスそのまま
3. **CSS `:hover` は維持**: 視覚フィードバックとして引き続き機能（`selected` クラスとは独立）

## テスト方針

### 自動テスト
- `npm test`: フロントユニットテスト
- `npm run build`: フロントビルド成功確認

### 手動確認
- キーボードで検索→Arrow で選択→Enter で起動（変更なし）
- マウスホバーで `selected` が変わらないこと
- シングルクリックでアイテムが起動すること（変更なし）

## SPEC.md 更新要否

**必要**。マウスホバーの仕様を明記する。

## セルフレビュー

### 1. 対称コードパス
- `handleClickResult`（起動）と `handleDoubleClickResult`（選択のみ）は変更なし ✓
- `handleHoverResult` のみ削除。生成/破棄ペアは hover debounce タイマーのみで、セットで削除 ✓

### 2. 影響範囲の網羅性
- `handleHoverResult` は MainApp.tsx でのみ定義・使用 ✓
- `onHoverResult` prop は ResultsSection.tsx の interface と ResultRow 転送の2箇所のみ ✓
- `onMouseEnter` は ResultRow.tsx でのみバインド ✓
- hover debounce タイマーは ResultsSection.tsx でのみ管理 ✓

### 3. 境界条件
- 結果0件: ResultsSection が非表示なのでホバー不可 ✓

### 4. リソース管理
- hover debounce タイマー（`setTimeout`）と `onCleanup`（`clearTimeout`）をセットで削除 ✓
- 新しいリソースの追加なし ✓

### 5. 既存パターンとの整合
- 新規パターンの導入なし。既存ハンドラを削除するだけ ✓

### 6. YAGNI 違反
- なし ✓

### 7. シンプル化の挑戦
- hover debounce タイマーが不要になりコードが純減 ✓

### 8. 破壊不変条件
- クリック起動は変更なし。起動→非表示→リセットのフロー不変 ✓
