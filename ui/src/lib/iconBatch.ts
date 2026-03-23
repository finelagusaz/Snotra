/** Parse length-prefixed binary batch into per-path Blob URLs.
 *  Format: [count:u32 LE] then per icon: [status:u8] [if 1: png_len:u32 LE, png_bytes] */
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
