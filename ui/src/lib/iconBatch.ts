/** 長さプレフィックス付きバイナリバッチを、パスごとの Blob URL へパースする。
 *  ワイヤ形式の正本は `src-tauri/src/icon.rs` の `encode_batch_binary` rustdoc。
 *  本デコーダはそれに一致する。デコードは `iconBatch.test.ts` が形式どおりに組み立てた
 *  実バイト列で検証する（Rust エンコーダ出力との言語横断往復ではない）。
 *
 *  **解放契約**: 返り値 Map の Blob URL の所有権は呼び出し側へ移る。`LruIconCache.set()` へ
 *  渡して管理を委ねるか、渡さず捨てる経路（stale guard 等の早期リターン）では全 URL を
 *  `revokeObjectURL` すること。怠ると Blob URL がリークする。 */
export function parseBinaryBatch(
  buf: ArrayBuffer,
  paths: string[],
): Map<string, string> {
  const view = new DataView(buf);
  let offset = 0;
  const count = view.getUint32(offset, true);
  offset += 4;
  const result = new Map<string, string>();
  for (let i = 0; i < count; i++) {
    const status = view.getUint8(offset);
    offset += 1;
    if (status === 1) {
      const pngLen = view.getUint32(offset, true);
      offset += 4;
      const pngBytes = new Uint8Array(buf, offset, pngLen);
      offset += pngLen;
      const blob = new Blob([pngBytes], { type: "image/png" });
      const url = URL.createObjectURL(blob);
      result.set(paths[i], url);
    }
  }
  return result;
}
