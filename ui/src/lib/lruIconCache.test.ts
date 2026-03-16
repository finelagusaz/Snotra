import { describe, it, expect, vi, beforeEach } from "vitest";
import { LruIconCache } from "./lruIconCache";

const revokeObjectURL = vi.fn();
vi.stubGlobal("URL", { revokeObjectURL, createObjectURL: vi.fn() });

describe("LruIconCache", () => {
  let cache: LruIconCache;

  beforeEach(() => {
    cache = new LruIconCache(100);
    revokeObjectURL.mockClear();
  });

  it("set で追加し get で取得できる", () => {
    cache.set("a", "blob:a");
    expect(cache.get("a")).toBe("blob:a");
    expect(cache.size).toBe(1);
  });

  it("has が正しく動作する", () => {
    expect(cache.has("a")).toBe(false);
    cache.set("a", "blob:a");
    expect(cache.has("a")).toBe(true);
  });

  it("存在しないキーの get は undefined を返す", () => {
    expect(cache.get("missing")).toBeUndefined();
  });

  it("同一パスの上書き時に古い URL が revoke される", () => {
    cache.set("a", "blob:old");
    cache.set("a", "blob:new");
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:old");
    expect(cache.get("a")).toBe("blob:new");
  });

  it("同一パスに同じ URL を set しても revoke されない", () => {
    cache.set("a", "blob:same");
    cache.set("a", "blob:same");
    expect(revokeObjectURL).not.toHaveBeenCalled();
  });

  it("revokeAll で全 URL が revoke される", () => {
    cache.set("a", "blob:a");
    cache.set("b", "blob:b");
    cache.revokeAll();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:a");
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:b");
    expect(cache.size).toBe(0);
    expect(cache.get("a")).toBeUndefined();
  });

  it("上限超過時に最古エントリが evict され revoke される", () => {
    for (let i = 0; i < 101; i++) {
      cache.set(`p${i}`, `blob:${i}`);
    }
    expect(cache.size).toBe(100);
    // p0 が evict されている
    expect(cache.has("p0")).toBe(false);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:0");
    // p1〜p100 は残っている
    expect(cache.has("p1")).toBe(true);
    expect(cache.has("p100")).toBe(true);
  });

  it("get した要素は LRU 末尾に移動し eviction で最後に追い出される", () => {
    for (let i = 0; i < 100; i++) {
      cache.set(`p${i}`, `blob:${i}`);
    }
    // p0 を get して末尾に移動
    cache.get("p0");
    // 1件追加して eviction をトリガー
    cache.set("new", "blob:new");
    // p1 が最古として evict される（p0 は get で末尾に移動済み）
    expect(cache.has("p1")).toBe(false);
    expect(cache.has("p0")).toBe(true);
  });

  it("コンストラクタに小さい値を渡すとその値で eviction が動作する", () => {
    const small = new LruIconCache(3);
    small.set("a", "blob:a");
    small.set("b", "blob:b");
    small.set("c", "blob:c");
    small.set("d", "blob:d");
    expect(small.size).toBe(3);
    expect(small.has("a")).toBe(false);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:a");
    expect(small.has("b")).toBe(true);
    expect(small.has("d")).toBe(true);
  });

  it("setMaxSize で縮小すると超過分が evict され revoke される", () => {
    cache.set("a", "blob:a");
    cache.set("b", "blob:b");
    cache.set("c", "blob:c");
    cache.set("d", "blob:d");
    cache.set("e", "blob:e");
    expect(cache.size).toBe(5);

    cache.setMaxSize(3);
    expect(cache.size).toBe(3);
    // 最古の a, b が evict される
    expect(cache.has("a")).toBe(false);
    expect(cache.has("b")).toBe(false);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:a");
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:b");
    // c, d, e は残る
    expect(cache.has("c")).toBe(true);
    expect(cache.has("d")).toBe(true);
    expect(cache.has("e")).toBe(true);
  });

  it("setMaxSize で拡大しても既存エントリは保持される", () => {
    const small = new LruIconCache(3);
    small.set("a", "blob:a");
    small.set("b", "blob:b");
    small.set("c", "blob:c");
    expect(small.size).toBe(3);

    small.setMaxSize(10);
    expect(small.size).toBe(3);
    expect(small.has("a")).toBe(true);
    expect(small.has("b")).toBe(true);
    expect(small.has("c")).toBe(true);
    expect(revokeObjectURL).not.toHaveBeenCalled();
  });
});
