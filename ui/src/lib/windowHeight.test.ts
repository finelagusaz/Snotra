import { describe, expect, it } from "vitest";
import { computeWindowHeight } from "./windowHeight";

const DEFAULTS = {
  searchBarHeight: 52,
  resultRowHeight: 30,
  resultsPadding: 8,
  updateToastHeight: 52,
};

describe("computeWindowHeight", () => {
  it("結果非表示 + トーストなし → 検索バーのみ", () => {
    expect(computeWindowHeight({
      ...DEFAULTS,
      shouldShowResults: false,
      maxResults: 8,
      hasUpdateToast: false,
    })).toBe(52);
  });

  it("結果表示 + トーストなし → 検索バー + 行 + パディング", () => {
    expect(computeWindowHeight({
      ...DEFAULTS,
      shouldShowResults: true,
      maxResults: 8,
      hasUpdateToast: false,
    })).toBe(52 + 8 * 30 + 8); // 300
  });

  it("結果非表示 + トーストあり → 検索バー + トースト", () => {
    expect(computeWindowHeight({
      ...DEFAULTS,
      shouldShowResults: false,
      maxResults: 8,
      hasUpdateToast: true,
    })).toBe(52 + 52); // 104
  });

  it("結果表示 + トーストあり → 全部合算", () => {
    expect(computeWindowHeight({
      ...DEFAULTS,
      shouldShowResults: true,
      maxResults: 8,
      hasUpdateToast: true,
    })).toBe(52 + 8 * 30 + 8 + 52); // 352
  });

  it("maxResults=0 の場合も正しく計算（パディングのみ）", () => {
    expect(computeWindowHeight({
      ...DEFAULTS,
      shouldShowResults: true,
      maxResults: 0,
      hasUpdateToast: false,
    })).toBe(52 + 0 + 8); // 60
  });

  it("maxResults が大きくてもオーバーフローしない", () => {
    const h = computeWindowHeight({
      ...DEFAULTS,
      shouldShowResults: true,
      maxResults: 100,
      hasUpdateToast: false,
    });
    expect(h).toBe(52 + 100 * 30 + 8);
  });
});
