import { describe, expect, it, beforeEach } from "vitest";
import { t, setLanguage } from "./i18n";

beforeEach(() => {
  // Ensure each test starts with a known language (ja).
  // The module-level default depends on navigator.language which varies by CI.
  setLanguage("ja");
});

describe("t (Japanese - default)", () => {
  describe("パラメータなし", () => {
    it("プレースホルダーのないキーをそのまま返す", () => {
      expect(t("search.placeholder.default")).toBe("検索...");
    });

    it("params を省略した場合もプレースホルダー付き文字列をそのまま返す", () => {
      expect(t("search.placeholder.folder")).toBe("{dir} 内を検索...");
    });
  });

  describe("パラメータ置換", () => {
    it("{dir} プレースホルダーを置換する", () => {
      expect(t("search.placeholder.folder", { dir: "C:\\Users" })).toBe(
        "C:\\Users 内を検索...",
      );
    });

    it("{detail} プレースホルダーを置換する（timeout）", () => {
      expect(t("notice.launch.timeout", { detail: "（5秒経過）" })).toBe(
        "起動に時間がかかっています（5秒経過）",
      );
    });

    it("{detail} プレースホルダーを置換する（failed）", () => {
      expect(t("notice.launch.failed", { detail: "：code 1" })).toBe(
        "起動に失敗しました：code 1",
      );
    });

    it("空文字に置換できる", () => {
      expect(t("notice.launch.timeout", { detail: "" })).toBe(
        "起動に時間がかかっています",
      );
    });

    it("マッチしないパラメータキーは無視される", () => {
      expect(t("search.placeholder.folder", { unknown: "X" })).toBe(
        "{dir} 内を検索...",
      );
    });

    it("str.replace は最初の出現のみ置換する（単一置換の仕様固定）", () => {
      expect(t("notice.launch.timeout", { detail: "{detail}" })).toBe(
        "起動に時間がかかっています{detail}",
      );
    });

    it("{hotkey} を置換する（initial_failed）", () => {
      expect(t("notice.hotkey.initial_failed", { hotkey: "Alt+Q" })).toBe(
        "ホットキー (Alt+Q) の登録に失敗しました。他のアプリが使用中の可能性があります",
      );
    });

    it("{hotkey} を置換する（change_failed）", () => {
      expect(t("notice.hotkey.change_failed", { hotkey: "Ctrl+Space" })).toBe(
        "ホットキー (Ctrl+Space) の登録に失敗しました。元のホットキーを維持します",
      );
    });
  });
});

describe("t (English)", () => {
  it("returns English strings after setLanguage('en')", () => {
    setLanguage("en");
    expect(t("search.placeholder.default")).toBe("Search...");
    expect(t("search.status.indexing")).toBe("Building index...");
    expect(t("search.status.no_results")).toBe("No results found");
    expect(t("cmd.history.description")).toBe("Show recent history");
    expect(t("cmd.quit.description")).toBe("Quit application");
  });

  it("placeholder replacement works in English", () => {
    setLanguage("en");
    expect(t("search.placeholder.folder", { dir: "C:\\Users" })).toBe(
      "Search in C:\\Users...",
    );
    expect(t("notice.hotkey.initial_failed", { hotkey: "Alt+Q" })).toBe(
      "Failed to register hotkey (Alt+Q). Another application may be using it",
    );
  });

  it("EN_US table has all keys that JA_JP has", () => {
    // Switching to English and back should work for every key
    setLanguage("en");
    const keys: Array<import("./i18n").TranslationKey> = [
      "search.placeholder.default",
      "search.placeholder.folder",
      "search.placeholder.tool_select",
      "search.status.indexing",
      "search.status.launching",
      "search.status.no_results",
      "cmd.history.description",
      "cmd.settings.description",
      "cmd.rebuild_index.description",
      "cmd.quit.description",
      "notice.launch.timeout",
      "notice.launch.failed",
      "notice.hotkey.initial_failed",
      "notice.hotkey.change_failed",
    ];
    for (const key of keys) {
      const val = t(key);
      expect(val, `Missing or empty EN translation for "${key}"`).toBeTruthy();
    }
  });
});

describe("setLanguage switching", () => {
  it("switches between languages", () => {
    expect(t("search.placeholder.default")).toBe("検索...");
    setLanguage("en");
    expect(t("search.placeholder.default")).toBe("Search...");
    setLanguage("ja");
    expect(t("search.placeholder.default")).toBe("検索...");
  });
});
