import { beforeAll, describe, expect, it, vi } from "vitest";
import { truncatePath } from "./truncatePath";

// Canvas API は Node 環境に存在しないため、measureText が文字数×CHAR_W を返す
// モックを document.createElement に差し込む。canvas 変数はモジュール内で遅延生成
// されるため、初回 truncatePath 呼び出し前にスタブを設置すれば確実に機能する。
const CHAR_W = 10;
const FONT = "12px sans-serif";

function px(s: string): number {
  return s.length * CHAR_W;
}

beforeAll(() => {
  vi.stubGlobal("document", {
    createElement: () => ({
      getContext: () => ({
        font: "",
        measureText: (text: string) => ({ width: text.length * CHAR_W }),
      }),
    }),
  });
});

describe("truncatePath", () => {
  describe("早期リターン", () => {
    it("maxWidth <= 0 のときパスをそのまま返す", () => {
      expect(truncatePath("C:\\Users\\file.exe", 0, FONT)).toBe(
        "C:\\Users\\file.exe",
      );
    });

    it("パスが幅内に収まる場合はそのまま返す", () => {
      const path = "C:\\Users\\file.exe"; // 17 chars = 170px
      expect(truncatePath(path, px(path), FONT)).toBe(path);
    });

    it("ローカルパスでセグメントが 2 以下は省略不可でそのまま返す", () => {
      // parts = ["C:", "file.exe"], length=2 → 省略対象なし
      const path = "C:\\file.exe";
      expect(truncatePath(path, 1, FONT)).toBe(path);
    });
  });

  describe("ローカルパスの省略", () => {
    // "C:\\Users\\Desktop\\app.exe" = 24 chars = 240px
    // minimal "C:\\...\\app.exe"        = 14 chars = 140px
    // +Desktop "C:\\...\\Desktop\\app.exe" = 22 chars = 220px
    // +Users   "C:\\...\\Users\\Desktop\\app.exe" = 28 chars = 280px
    const path = "C:\\Users\\Desktop\\app.exe";

    it("パスが収まらない場合に最小形式（prefix\\...\\last）を返す", () => {
      // 141 ≤ maxWidth < 220: minimal は入るが Desktop は入らない
      expect(truncatePath(path, 150, FONT)).toBe("C:\\...\\app.exe");
    });

    it("最小形式すら収まらない場合も最小形式を返す", () => {
      // minimal (140px) > maxWidth (100px)
      expect(truncatePath(path, 100, FONT)).toBe("C:\\...\\app.exe");
    });

    it("右から中間セグメントを追加して最大限表示する", () => {
      // maxWidth=220: Desktop まで入る、Users は入らない
      expect(truncatePath(path, 220, FONT)).toBe("C:\\...\\Desktop\\app.exe");
    });
  });

  describe("末尾セパレータの保持", () => {
    // "C:\\Users\\Desktop\\"  = 17 chars = 170px
    // minimal "C:\\...\\Desktop\\" = 15 chars = 150px
    it("省略後も末尾の \\ を保持する", () => {
      expect(truncatePath("C:\\Users\\Desktop\\", 150, FONT)).toBe(
        "C:\\...\\Desktop\\",
      );
    });
  });

  describe("UNC パスの省略", () => {
    // "\\\\server\\share\\longdir\\file.txt" = 31 chars = 310px
    // prefix "\\\\server\\share"               = 14 chars
    // minimal "\\\\server\\share\\...\\file.txt" = 27 chars = 270px
    const path = "\\\\server\\share\\longdir\\file.txt";

    it("UNC パスを最小形式（prefix\\...\\last）で返す", () => {
      expect(truncatePath(path, 270, FONT)).toBe(
        "\\\\server\\share\\...\\file.txt",
      );
    });

    it("UNC パスでセグメントが 2 以下（prefix のみ）はそのまま返す", () => {
      const short = "\\\\server\\share"; // parts after \\\\ = ["server","share"], length=2
      expect(truncatePath(short, 1, FONT)).toBe(short);
    });
  });

  describe("キャッシュ", () => {
    it("同じ引数で 2 回呼んでも同じ結果を返す", () => {
      const path = "C:\\cache\\test\\cached.exe"; // 24 chars = 240px
      const r1 = truncatePath(path, px(path), FONT);
      const r2 = truncatePath(path, px(path), FONT);
      expect(r1).toBe(r2);
    });
  });
});
