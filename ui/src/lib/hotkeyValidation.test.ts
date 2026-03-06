import { describe, expect, it } from "vitest";
import { formatHotkeyLabel, isHotkeyInvalid } from "./hotkeyValidation";

describe("isHotkeyInvalid", () => {
  describe("有効なホットキー（false を返す）", () => {
    it("デフォルトホットキー Alt+Q は有効", () => {
      expect(isHotkeyInvalid("Alt", "Q")).toBe(false);
    });

    it("複数修飾キー Ctrl+Alt+A は有効", () => {
      expect(isHotkeyInvalid("Ctrl+Alt", "A")).toBe(false);
    });

    it("ファンクションキー Ctrl+F1 は有効", () => {
      expect(isHotkeyInvalid("Ctrl", "F1")).toBe(false);
    });

    it("Alt+Shift+Space は有効（Alt 単独+Space のみ禁止）", () => {
      expect(isHotkeyInvalid("Alt+Shift", "Space")).toBe(false);
    });

    it("Ctrl+Space は有効", () => {
      expect(isHotkeyInvalid("Ctrl", "Space")).toBe(false);
    });
  });

  describe("キーが空", () => {
    it("空文字は無効", () => {
      expect(isHotkeyInvalid("Alt", "")).toBe(true);
    });

    it("空白のみは無効", () => {
      expect(isHotkeyInvalid("Alt", "   ")).toBe(true);
    });
  });

  describe("禁止キー", () => {
    it.each([
      "CapsLock",
      "capslock",
      "CAPS",
      "Eisu",
      "Kana",
      "KanaMode",
      "NonConvert",
      "Convert",
      "Lang1",
      "Lang2",
      "Hangul",
      "HangulMode",
      "Hanja",
      "HanjaMode",
    ])("禁止キー %s は無効", (key) => {
      expect(isHotkeyInvalid("Alt", key)).toBe(true);
    });
  });

  describe("修飾キーなし", () => {
    it("修飾キーが空は無効", () => {
      expect(isHotkeyInvalid("", "Q")).toBe(true);
    });

    it("修飾キーが空白のみは無効", () => {
      expect(isHotkeyInvalid("   ", "Q")).toBe(true);
    });

    it("区切り文字のみは無効", () => {
      expect(isHotkeyInvalid("+", "Q")).toBe(true);
    });
  });

  describe("Win キーを含む修飾キー", () => {
    it.each(["Win", "win", "Super", "super", "Meta", "meta"])(
      "修飾キー %s は無効",
      (mod) => {
        expect(isHotkeyInvalid(mod, "Q")).toBe(true);
      },
    );

    it("Win+Alt のような複合修飾キーも無効", () => {
      expect(isHotkeyInvalid("Win+Alt", "Q")).toBe(true);
    });

    it("Alt+Super も無効", () => {
      expect(isHotkeyInvalid("Alt+Super", "Q")).toBe(true);
    });
  });

  describe("Alt 単独 + Space の禁止", () => {
    it("Alt + Space は無効", () => {
      expect(isHotkeyInvalid("Alt", "Space")).toBe(true);
    });

    it("Alt + space（小文字）も無効", () => {
      expect(isHotkeyInvalid("Alt", "space")).toBe(true);
    });

    it("alt（小文字）+ Space も無効", () => {
      expect(isHotkeyInvalid("alt", "Space")).toBe(true);
    });
  });
});

describe("formatHotkeyLabel", () => {
  it("修飾キーとキーを + で結合する", () => {
    expect(formatHotkeyLabel("Alt", "Q")).toBe("Alt+Q");
  });

  it("複数修飾キーも正しく結合する", () => {
    expect(formatHotkeyLabel("Ctrl+Alt", "A")).toBe("Ctrl+Alt+A");
  });

  it("修飾キーが空の場合はキーのみ返す", () => {
    expect(formatHotkeyLabel("", "Q")).toBe("Q");
  });

  it("キーが空の場合は修飾キーのみ返す", () => {
    expect(formatHotkeyLabel("Alt+Ctrl", "")).toBe("Alt+Ctrl");
  });

  it("両方空の場合は空文字を返す", () => {
    expect(formatHotkeyLabel("", "")).toBe("");
  });
});
