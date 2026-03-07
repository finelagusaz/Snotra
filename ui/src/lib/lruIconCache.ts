const MAX_ICON_CACHE_SIZE = 200;

/** Blob URL 追跡付き LRU キャッシュ。
 *  Map の挿入順序を利用: get 時に delete→re-set で末尾に移動。
 *  先頭が最古（LRU）。 */
export class LruIconCache {
  private map = new Map<string, string>();

  get(path: string): string | undefined {
    const url = this.map.get(path);
    if (url !== undefined) {
      this.map.delete(path);
      this.map.set(path, url);
    }
    return url;
  }

  set(path: string, url: string): void {
    if (this.map.has(path)) {
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
