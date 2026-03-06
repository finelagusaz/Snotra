import { describe, expect, it } from "vitest";
import { t } from "./i18n";

describe("t", () => {
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
      // {detail} が2回連続する文字列は現状の翻訳テーブルには存在しないが、
      // replace() の単一置換動作を仕様として確認する
      expect(t("notice.launch.timeout", { detail: "{detail}" })).toBe(
        "起動に時間がかかっています{detail}",
      );
    });
  });
});
