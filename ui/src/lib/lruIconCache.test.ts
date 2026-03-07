import { describe, it, expect, vi, beforeEach } from "vitest";
import { LruIconCache } from "./lruIconCache";

const revokeObjectURL = vi.fn();
vi.stubGlobal("URL", { revokeObjectURL, createObjectURL: vi.fn() });

describe("LruIconCache", () => {
  let cache: LruIconCache;

  beforeEach(() => {
    cache = new LruIconCache();
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
    for (let i = 0; i < 201; i++) {
      cache.set(`p${i}`, `blob:${i}`);
    }
    expect(cache.size).toBe(200);
    // p0 が evict されている
    expect(cache.has("p0")).toBe(false);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:0");
    // p1〜p200 は残っている
    expect(cache.has("p1")).toBe(true);
    expect(cache.has("p200")).toBe(true);
  });

  it("get した要素は LRU 末尾に移動し eviction で最後に追い出される", () => {
    for (let i = 0; i < 200; i++) {
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
});
