import { describe, expect, it } from "vitest";
import { computeParentDir, clampSelectedIndex } from "./folderNav";

describe("computeParentDir", () => {
  // Drive root boundaries
  it("returns null at drive root C:\\", () => {
    expect(computeParentDir("C:\\")).toBeNull();
  });

  it("returns drive root from one level deep", () => {
    expect(computeParentDir("C:\\Windows")).toBe("C:\\");
  });

  it("returns parent from two levels deep", () => {
    expect(computeParentDir("C:\\Windows\\System32")).toBe("C:\\Windows");
  });

  it("returns parent from three levels deep", () => {
    expect(computeParentDir("C:\\Windows\\System32\\drivers")).toBe("C:\\Windows\\System32");
  });

  // Trailing backslash handling
  it("handles trailing backslash on non-root dir", () => {
    expect(computeParentDir("C:\\Windows\\")).toBe("C:\\");
  });

  // UNC root boundaries
  it("returns null at UNC share root \\\\server\\share", () => {
    expect(computeParentDir("\\\\server\\share")).toBeNull();
  });

  it("returns null at UNC share root with trailing backslash", () => {
    expect(computeParentDir("\\\\server\\share\\")).toBeNull();
  });

  it("returns null for UNC server-only path \\\\server", () => {
    expect(computeParentDir("\\\\server")).toBeNull();
  });

  it("returns UNC share root from one level deep", () => {
    expect(computeParentDir("\\\\server\\share\\folder")).toBe("\\\\server\\share");
  });

  it("returns parent from two levels into UNC", () => {
    expect(computeParentDir("\\\\server\\share\\folder\\sub")).toBe("\\\\server\\share\\folder");
  });

  // Edge: lowercase drive letter
  it("handles lowercase drive letter", () => {
    expect(computeParentDir("c:\\Windows")).toBe("c:\\");
  });

  it("returns null at lowercase drive root", () => {
    expect(computeParentDir("c:\\")).toBeNull();
  });
});

describe("clampSelectedIndex", () => {
  it("returns 0 for empty list", () => {
    expect(clampSelectedIndex(0, 0)).toBe(0);
    expect(clampSelectedIndex(5, 0)).toBe(0);
  });

  it("returns 0 for negative len", () => {
    expect(clampSelectedIndex(0, -1)).toBe(0);
  });

  it("clamps negative index to 0", () => {
    expect(clampSelectedIndex(-1, 5)).toBe(0);
    expect(clampSelectedIndex(-100, 5)).toBe(0);
  });

  it("clamps beyond-end index to last", () => {
    expect(clampSelectedIndex(10, 5)).toBe(4);
    expect(clampSelectedIndex(5, 5)).toBe(4);
  });

  it("preserves valid index", () => {
    expect(clampSelectedIndex(0, 5)).toBe(0);
    expect(clampSelectedIndex(2, 5)).toBe(2);
    expect(clampSelectedIndex(4, 5)).toBe(4);
  });

  it("handles single-element list", () => {
    expect(clampSelectedIndex(0, 1)).toBe(0);
    expect(clampSelectedIndex(1, 1)).toBe(0);
    expect(clampSelectedIndex(-1, 1)).toBe(0);
  });
});
