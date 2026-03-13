# Plan: Issue #258 — Blob URL リーク修正

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `ui/src/components/ResultsSection.tsx` | `visible=false` effect に `latestDataGeneration = ++dataGeneration` を 1 行追加 |

テストファイルの変更は不要（既存の staleness guard ロジックを再利用するだけ）。

## 実装順序

**フェーズ 1（唯一）**: `ResultsSection.tsx` を修正

```ts
// 変更前（120–126行目）
createEffect(() => {
  if (!props.visible) {
    iconCache.revokeAll();
    fetchedNone.clear();
    setIconCacheVersion((v) => v + 1);
  }
});

// 変更後
createEffect(() => {
  if (!props.visible) {
    latestDataGeneration = ++dataGeneration; // in-flight fetchIconBatch を stale にする
    iconCache.revokeAll();
    fetchedNone.clear();
    setIconCacheVersion((v) => v + 1);
  }
});
```

## 不変条件

1. **revoke 後に新規 Blob URL が iconCache に蓄積しない**: revokeAll() の直前に generation をインクリメントすることで、in-flight の fetchIconBatch が 88–93 行目の staleness guard で早期リターンし、parsed の URL も即座に revoke する。
2. **次回の results 変化でアイコン取得が正常に再開する**: 次に `createEffect(on(results, ...))` が発火すると `gen = ++dataGeneration` で新しい世代が発行され、`latestDataGeneration` も更新される。生成 → 破棄ペアが維持される。
3. **revokeAll の原子性**: `latestDataGeneration` 代入 → `revokeAll()` は同一同期タスク内で完了する（await をまたがない）。

## テスト方針

- 手動確認: ウィンドウを連続で開閉しながらアイコン付き検索を実行し、DevTools Memory タブで Blob URL が増え続けないことを確認
- 型チェック: `npm run typecheck`
- ビルド: `npm run build`（プロジェクトルートから）

## SPEC.md 更新要否

挙動変更なし（バグ修正）。SPEC.md の更新不要。

---

## セルフレビュー

### 対称コードパス確認

`iconCache.revokeAll()` が呼ばれる箇所は 3 か所:
1. `visible=false` effect（120–126行目）← 今回修正
2. `show-icons-changed` リスナー（162–167行目）← `revokeAll` 後に in-flight fetch を stale にしていない
3. `onCleanup` コールバック（156–157行目）← クリーンアップ時なので問題なし（その後 fetch は起動しない）

**2 について**: `show-icons-changed` で `false` が来たとき（アイコン表示設定が OFF になった瞬間）も、
同様に in-flight fetchIconBatch が revokeAll 後に cache に書き込む可能性がある。
ただし `show-icons-changed` は設定変更イベントであり、変更後は `fetchIcons` 冒頭の
`if (!props.showIcons || props.skipIcons) return;` ガードが発火し、新規の fetch は起動しない。
問題は in-flight fetch（すでに `getIconsBatch` を呼び出し中）の扱いのみ。

修正を `show-icons-changed` リスナーにも適用することで対称性を保つ方が安全。

→ **計画を更新**: `show-icons-changed` ハンドラにも同様の修正を加える。

### 修正後の変更ファイル（更新）

```ts
// 162–167行目 show-icons-changed ハンドラ
listen<boolean>("show-icons-changed", (event) => {
  if (!event.payload) {
    latestDataGeneration = ++dataGeneration; // ← 追加
    iconCache.revokeAll();
    fetchedNone.clear();
    setIconCacheVersion((v) => v + 1);
  }
}),
```

### チェックリスト

1. **対称コードパス**: ✅ 3 か所の revokeAll を全て検討。2 か所に修正適用（onCleanup は不要）
2. **影響範囲の網羅性**: ✅ revokeAll 呼び出し全箇所を grep で確認（3 か所）
3. **境界条件**: ✅ visible=false→true の往復でも次回 results 変化時に正常再開する
4. **リソース管理**: ✅ stale guard が parsed の URL を revoke する既存パスを再利用
5. **既存パターンとの整合**: ✅ generation インクリメントは既存の staleness guard パターンを踏襲
6. **YAGNI 違反**: ✅ なし（1 行 × 2 か所のみ）
7. **シンプル化**: ✅ これ以上シンプルな修正はない
8. **破壊不変条件**: ✅ Blob URL 管理の不変条件（ui/CLAUDE.md）を維持
