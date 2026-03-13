# Issue #253 実装計画

## 変更ファイル一覧

### `ui/src/components/SearchWindow.tsx`

`handleInput` 内のインデックスガード（line 246）を削除する。

**変更前:**
```ts
const trimmed = value.trimStart();
// インデックス構築中は通常検索を無視。ただしスラッシュコマンド（/）と
// インスタントコマンドプレフィックス（@等）はインデックス不要のためバイパスする。
if (indexing() && !trimmed.startsWith("/") && !trimmed.startsWith(instantCommandPrefix())) return;
```

**変更後:**
```ts
// trimmed は folderState 判定に使わないが、残してもいい（削除でもよい）
// indexing() ガードを削除: setQuery は常に呼ぶ。IPC をスキップするガードは refreshResults() 側にある
```

`trimmed` はこのガード削除後は使用箇所がなくなるため一緒に削除する。

## 実装順序

フェーズ 1 のみ（1ファイル、数行の変更）。

1. `SearchWindow.tsx:238-255` の `handleInput` を修正
   - `trimmed` 変数の宣言を削除（使用箇所がなくなる）
   - `indexing()` ガード行とコメントを削除

## 不変条件

- `setQuery` は常に最新の入力値を持つ（インデックス中も含む）
- `refreshResults()` の `indexing()` ガードが IPC スキップを保証する
- インデックス完了時の `runRefresh()` が蓄積 query で検索を実行する
- `/` や `@` で始まる入力は引き続き通常ルートで `setQuery` に届く（変化なし、ただしガードが不要になる）
- `toolSelectionState()` ガードと `launching()` ガードは維持する

## テスト方針

- 既存テスト: `npm run typecheck` + `npm run build`
- 手動確認: インデックス構築中に文字を入力し、完了後に即座に検索結果が出ることを確認（バックエンドのインデックス完了イベントをトリガーするか、デバッグビルドで `setIndexing` を手動で切り替える）

## SPEC.md 更新要否

挙動変更（インデックス中の入力が即座に query に反映される）だが、UI の「期待する挙動」セクションへの記述は不要。SPEC.md に該当セクションがあれば更新する。

---

## セルフレビュー

1. **対称コードパス**: `indexing-started` → `handleInput` のガード有効化、`indexing-complete` → ガード無効化という対称性はガード削除で解消される。対称パスの確認は不要。
2. **影響範囲の網羅性**: 変更は `handleInput` の1箇所のみ。`refreshResults()` 側ガードは変更しない。
3. **境界条件**: インデックス中の入力が `query()` に蓄積され、完了後に `runRefresh()` が走る。`runRefresh()` は `indexing-complete` リスナーで確定的に呼ばれる。
4. **リソース管理**: 新規リソースなし。
5. **既存パターンとの整合**: `refreshResults()` のガードパターンを再利用。新規パターン不要。
6. **YAGNI 違反**: なし。最小変更。
7. **シンプル化**: ガードを削除するだけ。これ以上シンプルにできない。
8. **破壊不変条件**: IPC はインデックス中に飛ばない（`refreshResults()` ガードが保証）。
