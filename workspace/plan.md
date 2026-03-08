# Plan — Issue #196: マウスカーソルでの検索リストのアイテム選択を明示的にしたい

## 設計方針

マウス操作を「ホバー→選択」から「シングルクリック→選択、ダブルクリック→実行」に変更する。
既存のハンドラ構造（`handleClickResult` / `handleDoubleClickResult` / `handleHoverResult`）の中身を入れ替えるだけで実現可能。ホバー debounce タイマーは不要になるため削除。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `ui/src/MainApp.tsx` | `handleClickResult` を選択のみに、`handleDoubleClickResult` を起動に、`handleHoverResult` を削除（or 空関数化） |
| `ui/src/components/ResultsSection.tsx` | hover debounce タイマー（50ms）を削除。`onHoverResult` prop を削除 |
| `ui/src/components/ResultRow.tsx` | `onMouseEnter` prop を削除 |
| `SPEC.md` | マウス操作仕様を明記（シングルクリック＝選択、ダブルクリック＝実行） |

計 4 ファイル、1 フェーズ。

## 実装詳細

### MainApp.tsx

**handleClickResult** (L200-212): 起動 → 選択のみに変更
```ts
function handleClickResult(index: number) {
  trace("app:event:result_clicked", { index });
  setSelected(index);
}
```

**handleDoubleClickResult** (L214-216): 選択のみ → 起動に変更
```ts
function handleDoubleClickResult(index: number) {
  trace("app:event:result_double_clicked", { index });
  void activateSelectedByIndex(index).then((launched) => {
    if (launched) {
      setMainVisible(false);
      api.notifyMainHidden().catch(() => {});
      void win.hide();
    }
  });
}
```

**handleHoverResult** (L218-219): 削除（props から除去）

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
- 通常モード/フォルダ展開モード: シングルクリック＝選択、ダブルクリック＝実行
- ツール選択モード: シングルクリック＝選択、ダブルクリック＝そのツールで起動
- ホバー: CSS `:hover` による視覚フィードバックのみ（`selected` 状態は変化しない）

## 不変条件

1. **`selected` シグナルはキーボードナビゲーションとクリックで共有**: Arrow ↑↓ とシングルクリックが同じ `setSelected()` を使う
2. **ダブルクリック起動は `activateSelectedByIndex(index)` を経由**: 現在の `handleClickResult` と同じ起動パスを使う（コードパスの一貫性）
3. **CSS `:hover` は維持**: 視覚フィードバックとして引き続き機能する（`selected` クラスとは独立）
4. **ツール選択モードも同一挙動**: §17.3 のクリック起動もダブルクリックに統一（`activateSelectedByIndex` がモードを意識して正しく起動するため）

## テスト方針

### 自動テスト
- `npm test`: フロントユニットテスト（既存テストが破壊されていないか確認）
- `npm run build`: フロントビルド成功確認

### 手動確認
- キーボードで検索→Arrow で選択→Enter で起動（変更なし）
- マウスホバーで `selected` が変わらないこと
- シングルクリックで `selected` が変わること
- ダブルクリックでアイテムが起動すること
- ツール選択モードでもシングルクリック＝選択、ダブルクリック＝起動
- フォルダ展開モードでも同様

## SPEC.md 更新要否

**必要**。マウス操作（クリック/ダブルクリック/ホバー）の仕様を明記する。

## セルフレビュー

### 1. 対称コードパス
- `handleClickResult`（選択）と `handleDoubleClickResult`（起動）は中身を入れ替える対称変更 ✓
- 起動パスは `activateSelectedByIndex` 一本に統一されており、Enter / ダブルクリックで同じ ✓

### 2. 影響範囲の網羅性
- `handleHoverResult` は MainApp.tsx でのみ定義・使用 ✓
- `onHoverResult` prop は ResultsSection.tsx → ResultRow.tsx の2箇所のみ ✓
- `onMouseEnter` は ResultRow.tsx でのみバインド ✓
- hover debounce タイマーは ResultsSection.tsx でのみ管理 ✓
- ツール選択モード（§17.3）のクリック起動も `activateSelectedByIndex` 経由なので同一変更でカバー ✓

### 3. 境界条件
- エラー行（`isError: true`）: `activateSelectedByIndex` 内で `is_error` チェックがあり起動されない。ダブルクリックしても問題なし ✓
- 結果0件: ResultsSection が非表示なのでクリック不可 ✓

### 4. リソース管理
- hover debounce タイマー（`setTimeout`）と `onCleanup`（`clearTimeout`）をセットで削除 ✓
- 新しいリソースの追加なし ✓

### 5. 既存パターンとの整合
- 新規パターンの導入なし。既存ハンドラの中身を入れ替えるだけ ✓

### 6. YAGNI 違反
- なし。要望された変更のみ ✓

### 7. シンプル化の挑戦
- hover debounce タイマーが不要になり、コードが減る方向 ✓
- 新しい状態やフラグの追加なし ✓

### 8. 破壊不変条件
- ダブルクリック起動は既存の `activateSelectedByIndex` を経由するため、起動→非表示→リセットの一連のフローは変更なし ✓
- 唯一のリスク: SPEC.md §17.3「クリック: 表示リストの行インデックスで選択ツールを一意に照合し起動」との矛盾。SPEC.md を同時に更新して解消 ✓

### ui/CLAUDE.md 更新要否
- 「クリック起動 (`handleClickResult`) とダブルクリック選択 (`handleDoubleClickResult`)」の記述を入れ替える必要あり → 計画に追加
