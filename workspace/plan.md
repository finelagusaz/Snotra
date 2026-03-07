# Plan: ResultsWindow アイコンキャッシュ LRU 化 (#164)

## 変更ファイル一覧

1. **`ui/src/components/ResultsWindow.tsx`** — iconCache を LRU 化、非表示時解放、fetchIcons 遅延実行
2. **`ui/src/lib/lruIconCache.ts`** — 新規。LRU キャッシュ + Blob URL 管理のヘルパー
3. **`ui/src/lib/lruIconCache.test.ts`** — 新規。LRU キャッシュのユニットテスト
4. **`ui/CLAUDE.md`** — モジュール構成に `lruIconCache.ts` を追加

## 実装順序

### Phase 1: LRU キャッシュヘルパー作成

`ui/src/lib/lruIconCache.ts`:

```ts
const MAX_ICON_CACHE_SIZE = 200;

/** Blob URL 追跡付き LRU キャッシュ。
 *  Map の挿入順序を利用: get 時に delete→re-set で末尾に移動。
 *  先頭が最古（LRU）。 */
export class LruIconCache {
  private map = new Map<string, string>();  // path → Blob URL

  get(path: string): string | undefined {
    const url = this.map.get(path);
    if (url !== undefined) {
      // LRU 更新: 末尾に移動
      this.map.delete(path);
      this.map.set(path, url);
    }
    return url;
  }

  set(path: string, url: string): void {
    if (this.map.has(path)) {
      // 既存エントリを更新（古い URL を revoke）
      const old = this.map.get(path)!;
      this.map.delete(path);
      if (old !== url) URL.revokeObjectURL(old);
    }
    this.map.set(path, url);
    this.evict();
  }

  has(path: string): boolean {
    return this.map.has(path);
  }

  /** 全エントリの Blob URL を revoke してクリア */
  revokeAll(): void {
    for (const url of this.map.values()) {
      URL.revokeObjectURL(url);
    }
    this.map.clear();
  }

  get size(): number {
    return this.map.size;
  }

  private evict(): void {
    while (this.map.size > MAX_ICON_CACHE_SIZE) {
      const first = this.map.keys().next().value;
      if (first === undefined) break;
      const url = this.map.get(first)!;
      URL.revokeObjectURL(url);
      this.map.delete(first);
    }
  }
}
```

**設計判断**: `iconUrls` Set は廃止。`LruIconCache` 内の Map が URL のライフサイクルを一元管理する。`revokeAllIconUrls()` は `cache.revokeAll()` に置換。

### Phase 2: ResultsWindow.tsx の更新

変更点:
1. `iconCache` シグナル (`Map<string, string>`) → `LruIconCache` インスタンス（シグナルではなくモジュールスコープ変数）
2. `iconCacheVersion` シグナル (`number`) を追加: cache 更新時にインクリメントし、SolidJS の再描画をトリガー
3. `iconUrls` Set と `revokeAllIconUrls()` を削除（LruIconCache に統合）
4. `parseBinaryBatch()`: `iconUrls.add(url)` を削除（cache.set が管理）
5. `fetchIcons()`: `cache.has()` / `cache.set()` を使用。末尾で `setIconCacheVersion(v => v + 1)` で再描画トリガー
6. `results-visibility-changed` リスナー追加: `cache.revokeAll()` + version リセット
7. `fetchIcons` 呼び出しを `requestAnimationFrame` でラップ（遅延実行）
8. `show-icons-changed(false)` ハンドラ: `cache.revokeAll()` に変更
9. `onCleanup`: `cache.revokeAll()` に変更
10. テンプレート内 `iconCache().get(result.path)` → `iconCacheVersion(), cache.get(result.path)` に変更

**SolidJS リアクティブ戦略の変更理由**: 現在は `iconCache` を `createSignal<Map>` で管理しているが、毎回 `new Map(cache)` でコピーするのはキャッシュサイズ増加とともにコストが上がる。`LruIconCache` + `iconCacheVersion` カウンタにすれば、コピーコストゼロで再描画トリガーが可能。

### Phase 3: テスト作成

`ui/src/lib/lruIconCache.test.ts`:

テストケース:
- `set` で追加し `get` で取得できる
- `get` した要素は LRU 末尾に移動する（eviction で最後に追い出される）
- 上限超過時に最古エントリが evict される
- evict 時に `URL.revokeObjectURL` が呼ばれる
- `revokeAll` で全 URL が revoke される
- `has` が正しく動作する
- 同一パスの上書き時に古い URL が revoke される

### Phase 4: CLAUDE.md 更新

`ui/CLAUDE.md` の `lib/` セクションに追加:
```
- `lruIconCache.ts`: Blob URL 管理付き LRU アイコンキャッシュ（`LruIconCache` クラス）。ResultsWindow で使用
```

## 不変条件

1. **Blob URL は必ず解放される**: `LruIconCache` の evict/revokeAll/set（上書き）で `URL.revokeObjectURL` が呼ばれる
2. **revoke 済み URL が `<img src>` に渡されない**: `revokeAll()` 後は `iconCacheVersion` 更新で再描画され、`cache.get()` は `undefined` を返す
3. **キャッシュサイズは上限を超えない**: `set` 後に `evict()` が走り、`MAX_ICON_CACHE_SIZE` 以下を保つ
4. **テキスト描画後にアイコン取得が走る**: `requestAnimationFrame` で1フレーム遅延
5. **非表示時にキャッシュがクリアされる**: `results-visibility-changed` で `cache.revokeAll()` + version 更新

## テスト方針

- `ui/src/lib/lruIconCache.test.ts` — LRU キャッシュの純粋ロジックテスト
  - `URL.revokeObjectURL` / `URL.createObjectURL` のモック
- `npm test` — 既存テストが壊れないことを確認
- `npm run build` — ビルド成功確認

## SPEC.md 更新要否

**不要**。挙動変更なし（内部のメモリ管理改善のみ）。SPEC.md §2.4 のアイコン仕様に変更はない。

## セルフレビュー

### 1. 対称コードパス
- `results-data-changed`（アイコン取得）と `results-visibility-changed`（アイコン解放）が対称ペア — 計画に含まれている
- `show-icons-changed(true)` は現在何もしない（次の検索で fetchIcons が走る）。`show-icons-changed(false)` で cache.revokeAll() — 対称性OK

### 2. 影響範囲の網羅性
- `iconCache` の参照箇所: ResultsWindow.tsx 内のみ。他ファイルからの参照なし
- `iconUrls` の参照箇所: ResultsWindow.tsx 内のみ
- `parseBinaryBatch` の `iconUrls.add(url)` 行の削除: cache.set 内で管理に移行

### 3. 境界条件
- キャッシュサイズ 0 件: `evict` は while ループが回らない。OK
- 同一パスの連続 set: 古い URL が revoke される。OK
- 空の検索結果で fetchIcons: `missing.length === 0` で早期リターン。OK

### 4. リソース管理
- `LruIconCache` の生成: コンポーネントスコープで1つ。破棄: `onCleanup(() => cache.revokeAll())` — ペア完備
- `results-visibility-changed` リスナーの生成/破棄: `listen().then(fn => unlisten = fn)` + `onCleanup(() => unlisten?.())` — 既存パターン踏襲

### 5. 既存パターンとの整合
- LRU は `Map` の挿入順序を利用する標準パターン。外部ライブラリ不要
- リスナー登録は既存の `onCleanup` + `listen().then()` パターンを踏襲

### 6. YAGNI 違反
- `MAX_ICON_CACHE_SIZE` を設定値にしない（定数で十分）
- LRU クラスに不要な機能（TTL、統計等）を追加しない

### 7. シンプル化の挑戦
- **`iconCacheVersion` カウンタ vs `createSignal<Map>`**: カウンタの方がシンプル。Map コピーのコストが消え、LRU ロジックも自然に書ける
- **`iconUrls` Set の廃止**: LruIconCache が URL ライフサイクルを一元管理することで、二重管理（Map + Set）が解消される
- **失敗時**: `fetchIcons` 失敗は既存の early return で処理。cache 状態は変わらない

### 8. 破壊不変条件の明示
- **Blob URL の二重 revoke**: `revokeObjectURL` を同じ URL に2回呼ぶのはスペック上 no-op。安全
- **revoke 済み URL の `<img src>` 参照**: `revokeAll()` 後に `iconCacheVersion` を更新しないと古い URL が参照される。計画では `revokeAll()` の直後に version 更新を必ずペアにしている。検知: 目視確認（画像が壊れる）
