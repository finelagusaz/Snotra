# Plan — Issue #293: E2E Shift+Enter ツール選択テスト失敗

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|----------|
| `e2e/tauri.slash.e2e.ts:39-97` | `buildE2EConfigToml()` に `.txt` 拡張子用の openers ルール（2 ツール）を追加 |
| `e2e/tauri.slash.e2e.ts:406-444` | テストのクエリを `C:\` → `E2E_SEARCH_QUERY` に変更し、復元確認も合わせて更新 |

## 実装順序

### Phase 1: E2E config に `.txt` 用 openers 追加

`buildE2EConfigToml()` に以下を追加:

```toml
[[openers]]
target = "ext:txt"

[[openers.tools]]
name = "Notepad"
exe = "notepad.exe"

[[openers.tools]]
name = "VS Code"
exe = "code.exe"
```

これにより `.txt` ファイルに対して 2 ツールがマッチし、ツール選択モードに入れる。

### Phase 2: テスト本体の更新

1. `C:\` → `E2E_SEARCH_QUERY` に変更（他の更新済みテストと一貫）
2. 結果待ちのエラーメッセージを更新
3. `"C:\\"` → `E2E_SEARCH_QUERY` の復元確認
4. パスクエリモード前提のコメントを更新

## 不変条件

- `find_matching_tools("...snotra-e2e-alpha.txt", false, rules)` が 2 件返す（`ext:txt` ルールにマッチ）
- 既存の `target = "folder"` openers は変更しない（フォルダ展開テスト等に影響しない）
- `E2E_SEARCH_QUERY` で結果が表示される前提は他テストで検証済み

## テスト方針

- 修正対象が E2E テスト自体のため、`npm run e2e:tauri -- --grep "Shift+Enter"` で検証
- 他の E2E テストに影響しないことを全テスト実行で確認

## SPEC.md 更新要否

不要。E2E テストの修正のみで、挙動変更なし。

## セルフレビュー

1. **対称コードパス**: Shift+Enter（ツール選択進入）と Escape（ツール選択離脱）のペアはテスト内で両方カバー済み — 変更なし
2. **影響範囲の網羅性**: `buildE2EConfigToml()` を使う全テストを確認。`.txt` 用 openers 追加は既存テストに影響しない（下記「要注意」参照）
3. **境界条件**: フィクスチャファイルが 0 件の場合は既存の結果待ちタイムアウトで検出される
4. **リソース管理**: 新規リソース生成なし
5. **既存パターンとの整合**: `E2E_SEARCH_QUERY` 使用は #294 で確立済みパターン
6. **YAGNI 違反**: なし
7. **シンプル化**: 2 ファイル・2 箇所の最小変更
8. **破壊不変条件**: E2E テスト限定の変更でシステム不変条件への影響なし

### 要注意: Enter 単独テストへの影響

`e2e/tauri.slash.e2e.ts` の「Enter で先頭の結果を起動」テスト（716行付近）が `E2E_SEARCH_QUERY` で `.txt` ファイルを Enter 起動する。openers に `ext:txt` を追加すると `tools.length == 2` だが、Enter（Shift なし）は `SearchWindow.tsx` で `activateSelected()` を呼び、`enterToolSelection()` は呼ばれない。ツール選択は Shift+Enter 専用。→ **影響なし**。
