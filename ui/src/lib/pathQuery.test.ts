import { describe, expect, it } from "vitest";
import { isPathQuery, parsePathQuery } from "./pathQuery";

describe("parsePathQuery", () => {
  it("treats plain C as non-path", () => {
    expect(parsePathQuery("C")).toBeNull();
    expect(isPathQuery("C")).toBe(false);
  });

  it("treats C: as drive root path", () => {
    expect(parsePathQuery("C:")).toEqual({ dir: "C:\\", filter: "" });
    expect(isPathQuery("C:")).toBe(true);
  });

  it("treats C:\\ as drive root path", () => {
    expect(parsePathQuery("C:\\")).toEqual({ dir: "C:\\", filter: "" });
    expect(isPathQuery("C:\\")).toBe(true);
  });

  it("normalizes C:/ as drive root path", () => {
    expect(parsePathQuery("C:/")).toEqual({ dir: "C:\\", filter: "" });
  });

  it("splits C:\\Windows into dir/filter", () => {
    expect(parsePathQuery("C:\\Windows")).toEqual({ dir: "C:\\", filter: "Windows" });
  });

  it("supports UNC share root with and without trailing slash", () => {
    expect(parsePathQuery("\\\\server\\share")).toEqual({
      dir: "\\\\server\\share\\",
      filter: "",
    });
    expect(parsePathQuery("\\\\server\\share\\")).toEqual({
      dir: "\\\\server\\share\\",
      filter: "",
    });
  });

  it("splits UNC child path into dir/filter", () => {
    expect(parsePathQuery("\\\\server\\share\\folder")).toEqual({
      dir: "\\\\server\\share\\",
      filter: "folder",
    });
  });

  it("normalizes yen-sign separators", () => {
    expect(parsePathQuery("C:¥Windows")).toEqual({ dir: "C:\\", filter: "Windows" });
  });

  it("uppercases lowercase drive letter", () => {
    expect(parsePathQuery("c:")).toEqual({ dir: "C:\\", filter: "" });
    expect(parsePathQuery("c:\\")).toEqual({ dir: "C:\\", filter: "" });
    expect(parsePathQuery("c:\\Windows")).toEqual({ dir: "C:\\", filter: "Windows" });
    expect(parsePathQuery("c:\\Windows\\System32")).toEqual({
      dir: "C:\\Windows\\",
      filter: "System32",
    });
  });
});
