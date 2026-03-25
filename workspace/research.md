# Research — Issue #293: E2E Shift+Enter ツール選択テスト失敗

## issue の要約

E2E テスト「Shift+Enter でツール選択リストが表示され Escape で元に戻る」(`e2e/tauri.slash.e2e.ts:406`) が main でも失敗する。エラーは `TimeoutError: tool selection did not activate`。

## 根本原因

#294（`e5be41f`）でパス検索が廃止された際、このテストの更新が漏れた。

**Before（#294 以前）**: `C:\` → パスクエリモード → ドライブルート一覧（フォルダ）が表示 → Shift+Enter → openers `target = "folder"` にマッチ → cmd + PowerShell の 2 ツール → ツール選択モードに入る

**After（#294 以後）**: `C:\` → 通常検索のパスマッチング → E2E フィクスチャの `.txt` ファイルがヒット → Shift+Enter → openers `target = "folder"` にマッチしない（ファイルはフォルダではない）→ ツール 0 件 → `tools.length <= 1` → 通常起動フォールバック → ツール選択に入らない

## 関連コード

| ファイル | 箇所 | 役割 |
|---------|------|------|
| `e2e/tauri.slash.e2e.ts:406-444` | テスト本体 | `C:\` 入力 → Shift+Enter → placeholder 確認 → Escape → 復元確認 |
| `e2e/tauri.slash.e2e.ts:39-97` | `buildE2EConfigToml()` | E2E 用 config 生成。openers は `folder` のみ |
| `ui/src/components/SearchWindow.tsx:213-231` | `handleKeyDown` | Shift+Enter でツール選択呼び出し |
| `ui/src/stores/search.ts:470-484` | `enterToolSelection()` | `tools.length <= 1` でフォールバック |
| `snotra-core/src/config.rs:87-164` | `find_matching_tools()` | openers ルールマッチング |

## 既存パターン

#294 で同じ問題を持つ別テスト（「→ キーでフォルダ展開」538行目付近）は `E2E_SEARCH_QUERY` に更新済み。Shift+Enter テストだけ更新が漏れた。

## 技術的制約

- ツール選択モードに入るには `getMatchingTools` が 2 件以上のツールを返す必要がある
- E2E フィクスチャは `snotra-e2e-{alpha,beta,gamma}.txt`（`.txt` ファイル、フォルダなし）
- 現在の openers は `target = "folder"` のみ → `.txt` ファイルにはマッチしない

## 多角的レビューで検証した懸念事項

| 懸念 | 結論 | 根拠 |
|------|------|------|
| クリック起動パスへの影響 | なし | `handleClickResult` → `activateSelectedByIndex` → `launchAndReset`。`enterToolSelection` 非経由 |
| Enter テスト（711行）への影響 | なし | `resolve_opener` が `notepad.exe` を返すようになるが、テストは「main 非表示」のみ検証 |
| `exitToolSelection` のクエリ復元漏れ | false alarm | `enterToolSelection` は `setQuery` を呼ばず、`query()` シグナル不変。`inputValue()` が display 切替のみ |
| タイミング問題（`C:\` → `E2E_SEARCH_QUERY`） | なし | 同パターンが他テストで問題なく動作 |
| `find_matching_tools` の `ext:txt` マッチ | 正確 | 絶対パスの `rfind('.')` で `.txt` 抽出。大文字小文字無視 |
| `workspace/update-toast-mockup.html` 混入 | 要対処 | 前回作業成果物が `git add workspace/` で混入 |

## 未解決の疑問

なし。原因と修正方針は明確。
