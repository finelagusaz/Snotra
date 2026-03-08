# Research — Issue #196: マウスカーソルでの検索リストのアイテム選択を明示的にしたい

## issue の要約

現状、検索結果リスト上にマウスカーソルが乗ると `onMouseEnter`（50ms debounce）で `selected` シグナルが更新され、意図せずホバーだけで選択が変わってしまう。キーボードで検索→選択→起動する操作フローにおいて、マウスカーソルが結果リスト上にあると選択が「吸い込まれる」問題。

**要望**: ホバー選択 → シングルクリック選択、ダブルクリック実行に変更。

**付随問題**: 起動後の最初の検索と2度目以降で挙動が違う（ホバーによる `selected` 更新 + 前回 `selected` のクランプの相互作用が原因と推定。ホバー選択を廃止すれば根本解消）。

## 現状の挙動

| 操作 | 変更前の動作 | 方式 C 適用後 |
|------|-----------|---------------------|
| ホバー | `setSelected(index)` — 選択が変わる | 視覚フィードバック（CSS `:hover`）のみ、`selected` は変えない |
| シングルクリック | `activateSelectedByIndex(index)` — 即起動 | 変更なし（即起動を維持） |
| ダブルクリック | `setSelected(index)` — 選択のみ | 変更なし |
| Enter | 選択中アイテムを起動 | 変更なし |
| Arrow ↑↓ | 選択移動 | 変更なし |

## 関連コード

### マウスイベントハンドラチェーン

| ファイル | 行 | 内容 |
|---------|-----|------|
| `ui/src/components/ResultRow.tsx` | 40-42 | `onClick`, `onDblClick`, `onMouseEnter` バインディング |
| `ui/src/components/ResultsSection.tsx` | 199-206 | `handleHover`: 50ms debounce → `props.onHoverResult(idx)` |
| `ui/src/components/ResultsSection.tsx` | 220-222 | ResultRow への props 転送 |
| `ui/src/MainApp.tsx` | 200-212 | `handleClickResult` → `activateSelectedByIndex(index)` — **起動** |
| `ui/src/MainApp.tsx` | 214-216 | `handleDoubleClickResult` → `setSelected(index)` — **選択のみ** |
| `ui/src/MainApp.tsx` | 218-219 | `handleHoverResult` → `setSelected(index)` — **選択のみ** |

### CSS

| セレクタ | 効果 | 変更要否 |
|---------|------|---------|
| `.result-row:hover` | `color-mix(... 50%, transparent)` — ホバー背景 | 維持（視覚フィードバック） |
| `.result-row.selected` | `var(--selected-row-color)` — 選択行背景 | 維持 |

### SPEC.md での記述

- §17.3: 「クリック: 表示リストの行インデックスで選択ツールを一意に照合し起動」— ツール選択モードでのクリック起動
- ホバー選択、通常モードでのクリック/ダブルクリックの仕様は明示されていない

### ui/CLAUDE.md での記述

- 「クリック起動 (`handleClickResult`) とダブルクリック選択 (`handleDoubleClickResult`) はどちらもリスト行インデックス（`number`）を引数として受け取る」

## 既存パターン

- ダブルクリックのハンドラ（`handleDoubleClickResult`）は既に存在する。現在は `setSelected(index)` だけ
- シングルクリックのハンドラ（`handleClickResult`）も既に存在する。現在は `activateSelectedByIndex(index)` を呼ぶ
- ホバーのハンドラ（`handleHoverResult`）も既に存在する。現在は `setSelected(index)` を呼ぶ

→ 既存のハンドラ構造はそのまま活用可能。各ハンドラの中身を入れ替えるだけ。

## 技術的制約

- CSS の `:hover` 疑似クラスはブラウザネイティブなので維持する（視覚フィードバックとして有用）
- `selected` シグナルはキーボードナビゲーション（ArrowUp/Down）と共有されるため、クリックによる選択更新もこの同一シグナルを使う
- ホバーで `selected` を変えなくなることで、キーボード操作中にマウスカーソルが結果上にあっても干渉しなくなる
- 50ms hover debounce タイマーは不要になる（ホバーで選択を変えないため）
- ツール選択モード（§17.3）では「クリックで起動」が仕様。通常モードとツール選択モードでクリック挙動を変えるか、統一するかの判断が必要
  → issue の要望は「シングルクリックで選択、ダブルクリックで実行」。ツール選択モードも同一挙動に統一するのが自然

## 未解決の疑問

なし。変更範囲が明確。
