import { describe, expect, it } from "vitest";
import { interpret, isInstantPrefix } from "./interpretQuery";

// 純関数ゆえ SolidJS 初期化不要（folderNav.ts / windowHeight.ts と同じ分離パターン）。

describe("isInstantPrefix", () => {
  it("prefix にマッチすれば true", () => {
    expect(isInstantPrefix("@goo", "@")).toBe(true);
  });

  it("prefix にマッチしなければ false", () => {
    expect(isInstantPrefix("goo", "@")).toBe(false);
  });

  it("空 prefix では常に false（全入力の instant 化を防ぐ）", () => {
    expect(isInstantPrefix("@goo", "")).toBe(false);
    expect(isInstantPrefix("", "")).toBe(false);
  });

  it("先頭空白を無視して判定する（trimStart）", () => {
    expect(isInstantPrefix("   @goo", "@")).toBe(true);
  });

  it("複数文字 prefix にも対応する", () => {
    expect(isInstantPrefix(">>run", ">>")).toBe(true);
    expect(isInstantPrefix(">run", ">>")).toBe(false);
  });
});

describe("interpret", () => {
  describe("kind の分類（viewKind=results）", () => {
    it("通常クエリは plain", () => {
      expect(interpret("hello", "@", "results")).toEqual({ kind: "plain" });
    });

    it("空文字は plain", () => {
      expect(interpret("", "@", "results")).toEqual({ kind: "plain" });
    });

    it("'/' 始まりは command", () => {
      expect(interpret("/xyz", "@", "results")).toEqual({ kind: "command" });
    });

    it("'/r'（履歴）も command として分類する（/r 特例は dispatch の関心）", () => {
      expect(interpret("/r", "@", "results")).toEqual({ kind: "command" });
    });

    it("先頭空白付き '/' も command（trimStart 後に判定）", () => {
      expect(interpret("  /open", "@", "results")).toEqual({ kind: "command" });
    });

    it("prefix 始まりは instant", () => {
      expect(interpret("@goo", "@", "results")).toEqual({
        kind: "instant",
        filterName: "goo",
        instantQuery: "",
      });
    });
  });

  describe("instant の parse（filterName / instantQuery 抽出）", () => {
    it("スペースなし: filterName は全体、instantQuery は空", () => {
      expect(interpret("@google", "@", "results")).toEqual({
        kind: "instant",
        filterName: "google",
        instantQuery: "",
      });
    });

    it("スペースあり: filterName はスペース前、instantQuery はスペース後", () => {
      expect(interpret("@google SolidJS tutorial", "@", "results")).toEqual({
        kind: "instant",
        filterName: "google",
        instantQuery: "SolidJS tutorial",
      });
    });

    it("スペースのみ後続なし: instantQuery は空", () => {
      expect(interpret("@g ", "@", "results")).toEqual({
        kind: "instant",
        filterName: "g",
        instantQuery: "",
      });
    });

    it("先頭空白は trimStart され、prefix 除去後を parse する", () => {
      expect(interpret("   @google X", "@", "results")).toEqual({
        kind: "instant",
        filterName: "google",
        instantQuery: "X",
      });
    });

    it("複数文字 prefix でも prefix 長ぶん除去する", () => {
      expect(interpret(">>run task", ">>", "results")).toEqual({
        kind: "instant",
        filterName: "run",
        instantQuery: "task",
      });
    });
  });

  describe("空 prefix ガード", () => {
    it("空 prefix では '@' 始まりでも plain（instant 化しない）", () => {
      expect(interpret("@goo", "", "results")).toEqual({ kind: "plain" });
    });
  });

  describe("viewKind による plain 固定", () => {
    it("folder 中は '/' でも plain", () => {
      expect(interpret("/xyz", "@", "folder")).toEqual({ kind: "plain" });
    });

    it("folder 中は '@' でも plain", () => {
      expect(interpret("@goo", "@", "folder")).toEqual({ kind: "plain" });
    });

    it("tool 中は '@' でも plain", () => {
      expect(interpret("@goo", "@", "tool")).toEqual({ kind: "plain" });
    });
  });
});
