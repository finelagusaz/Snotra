import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { parseBinaryBatch } from "./iconBatch";

// ── Blob / URL モック ─────────────────────────────────────────────────────────

let blobCounter = 0;
const revokedUrls = new Set<string>();

beforeEach(() => {
  blobCounter = 0;
  revokedUrls.clear();
  vi.stubGlobal("Blob", class FakeBlob {
    constructor(public parts: unknown[], public options: { type: string }) {}
  });
  vi.stubGlobal("URL", {
    createObjectURL: () => `blob:fake-${++blobCounter}`,
    revokeObjectURL: (url: string) => revokedUrls.add(url),
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

// ── ヘルパー ──────────────────────────────────────────────────────────────────

/** バイナリバッチを組み立てる。entries: null = status 0 (アイコンなし), Uint8Array = status 1 (PNG) */
function buildBatch(entries: (Uint8Array | null)[]): ArrayBuffer {
  // ヘッダ: count (4 bytes LE)
  let size = 4;
  for (const e of entries) {
    size += 1; // status
    if (e !== null) {
      size += 4 + e.length; // png_len + png_bytes
    }
  }
  const buf = new ArrayBuffer(size);
  const view = new DataView(buf);
  let offset = 0;
  view.setUint32(offset, entries.length, true);
  offset += 4;
  for (const e of entries) {
    if (e === null) {
      view.setUint8(offset, 0);
      offset += 1;
    } else {
      view.setUint8(offset, 1);
      offset += 1;
      view.setUint32(offset, e.length, true);
      offset += 4;
      new Uint8Array(buf, offset, e.length).set(e);
      offset += e.length;
    }
  }
  return buf;
}

// ── テスト ────────────────────────────────────────────────────────────────────

describe("parseBinaryBatch", () => {
  it("全エントリにアイコンがある場合、全パスの Blob URL を返す", () => {
    const png1 = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
    const png2 = new Uint8Array([0x89, 0x50]);
    const buf = buildBatch([png1, png2]);
    const paths = ["C:\\a.exe", "C:\\b.exe"];

    const result = parseBinaryBatch(buf, paths);

    expect(result.size).toBe(2);
    expect(result.get("C:\\a.exe")).toBe("blob:fake-1");
    expect(result.get("C:\\b.exe")).toBe("blob:fake-2");
  });

  it("status=0 のエントリはスキップされる（アイコンなし）", () => {
    const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
    const buf = buildBatch([null, png, null]);
    const paths = ["C:\\no-icon.exe", "C:\\has-icon.exe", "C:\\also-no.exe"];

    const result = parseBinaryBatch(buf, paths);

    expect(result.size).toBe(1);
    expect(result.has("C:\\no-icon.exe")).toBe(false);
    expect(result.get("C:\\has-icon.exe")).toBe("blob:fake-1");
    expect(result.has("C:\\also-no.exe")).toBe(false);
  });

  it("空バッチ（count=0）は空 Map を返す", () => {
    const buf = buildBatch([]);

    const result = parseBinaryBatch(buf, []);

    expect(result.size).toBe(0);
  });

  it("各アイコンごとに URL.createObjectURL が呼ばれる", () => {
    const png = new Uint8Array([1, 2, 3]);
    const buf = buildBatch([png, png, png]);
    const paths = ["a", "b", "c"];

    parseBinaryBatch(buf, paths);

    // blobCounter は 3 回インクリメントされた
    expect(blobCounter).toBe(3);
  });

  it("大きい PNG データも正しくパースされる", () => {
    const largePng = new Uint8Array(10000);
    largePng.fill(0xAB);
    const buf = buildBatch([largePng]);
    const paths = ["C:\\large.exe"];

    const result = parseBinaryBatch(buf, paths);

    expect(result.size).toBe(1);
    expect(result.get("C:\\large.exe")).toBe("blob:fake-1");
  });
});
