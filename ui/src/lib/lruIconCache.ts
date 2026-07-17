/** Blob URL 追跡付き LRU キャッシュ。
 *  Map の挿入順序を利用: get 時に delete→re-set で末尾に移動。
 *  先頭が最古（LRU）。
 *
 *  呼び出し側の契約:
 *  - **所有権**: `set(path, url)` で URL の所有権は cache へ移る。上書きされた旧 URL・evict・
 *    `revokeAll` の revoke は cache が行う——呼び出し側が同じ URL を二重に revoke しないこと。
 *  - **staleness**: `get`/`peek` が返した URL は以降の `set`/evict/`revokeAll` で失効しうる。
 *    保持せず即時に使用すること。
 *  - `get` は LRU 順序を更新する（書き込み相当）。高頻度の render パスでは `peek` を使う。 */
export class LruIconCache {
  private map = new Map<string, string>();
  private maxSize: number;

  constructor(maxSize: number) {
    this.maxSize = maxSize;
  }

  setMaxSize(newSize: number): void {
    this.maxSize = newSize;
    this.evict();
  }

  get(path: string): string | undefined {
    const url = this.map.get(path);
    if (url !== undefined) {
      this.map.delete(path);
      this.map.set(path, url);
    }
    return url;
  }

  /** LRU 順序を変更せずに値を取得する。render パス（高頻度）で使用。 */
  peek(path: string): string | undefined {
    return this.map.get(path);
  }

  set(path: string, url: string): void {
    const old = this.map.get(path);
    if (old !== undefined) {
      this.map.delete(path);
      if (old !== url) URL.revokeObjectURL(old);
    }
    this.map.set(path, url);
    this.evict();
  }

  has(path: string): boolean {
    return this.map.has(path);
  }

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
    while (this.map.size > this.maxSize) {
      const entry = this.map.entries().next().value;
      if (entry === undefined) break;
      const [key, url] = entry;
      URL.revokeObjectURL(url);
      this.map.delete(key);
    }
  }
}
