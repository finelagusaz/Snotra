let canvas: HTMLCanvasElement | null = null;
const MAX_MEASURE_CACHE = 4096;
const MAX_TRUNCATE_CACHE = 2048;
const measureCache = new Map<string, number>();
const truncateCache = new Map<string, string>();

function getContext(): CanvasRenderingContext2D {
  if (!canvas) {
    canvas = document.createElement("canvas");
  }
  return canvas.getContext("2d")!;
}

function measureText(text: string, font: string): number {
  const key = `${font}\n${text}`;
  const cached = measureCache.get(key);
  if (cached !== undefined) {
    return cached;
  }

  const ctx = getContext();
  ctx.font = font;
  const width = ctx.measureText(text).width;
  if (measureCache.size >= MAX_MEASURE_CACHE) {
    measureCache.clear();
  }
  measureCache.set(key, width);
  return width;
}

/**
 * Truncate a path by collapsing middle segments with "..." to fit within maxWidth.
 *
 * Strategy:
 *   1. If the full path fits, return it as-is.
 *   2. Strip trailing "\" (restored after truncation for folders).
 *   3. Detect UNC prefix ("\\server\share") vs local drive ("C:").
 *   4. Keep prefix and last segment, progressively add middle segments from the right.
 *   5. Minimal form: "C:\...\filename.exe" or "\\server\share\...\file.txt"
 */
export function truncatePath(
  path: string,
  maxWidth: number,
  font: string,
): string {
  const roundedWidth = Math.round(maxWidth);
  const cacheKey = `${font}\n${roundedWidth}\n${path}`;
  const cached = truncateCache.get(cacheKey);
  if (cached !== undefined) {
    return cached;
  }

  if (maxWidth <= 0) return path;

  if (measureText(path, font) <= maxWidth) {
    if (truncateCache.size >= MAX_TRUNCATE_CACHE) {
      truncateCache.clear();
    }
    truncateCache.set(cacheKey, path);
    return path;
  }

  const sep = "\\";

  // Strip trailing separator (folders) — will be restored at the end
  const trailingSep = path.endsWith(sep) ? sep : "";
  const workPath = trailingSep ? path.slice(0, -1) : path;

  let prefix: string;
  let rest: string[];

  if (workPath.startsWith(sep + sep)) {
    // UNC path: \\server\share\...
    const parts = workPath.slice(2).split(sep); // remove leading "\\" then split
    if (parts.length <= 2) {
      // Only \\server\share (or less) — cannot truncate
      return path;
    }
    prefix = sep + sep + parts[0] + sep + parts[1]; // "\\server\share"
    rest = parts.slice(2); // segments after share
  } else {
    // Local path: C:\...
    const parts = workPath.split(sep);
    if (parts.length <= 2) {
      return path;
    }
    prefix = parts[0]; // "C:"
    rest = parts.slice(1);
  }

  const last = rest[rest.length - 1];
  const middle = rest.slice(0, -1);

  // Minimal form: prefix\...\last + trailingSep
  const minimal = prefix + sep + "..." + sep + last + trailingSep;
  if (measureText(minimal, font) > maxWidth) {
    if (truncateCache.size >= MAX_TRUNCATE_CACHE) {
      truncateCache.clear();
    }
    truncateCache.set(cacheKey, minimal);
    return minimal;
  }

  // Try adding middle segments from the right
  let bestResult = minimal;

  for (let keepRight = 1; keepRight <= middle.length; keepRight++) {
    const rightSegments = middle.slice(middle.length - keepRight);
    const candidate =
      prefix +
      sep +
      "..." +
      sep +
      rightSegments.join(sep) +
      sep +
      last +
      trailingSep;

    if (measureText(candidate, font) <= maxWidth) {
      bestResult = candidate;
    } else {
      break;
    }
  }

  if (truncateCache.size >= MAX_TRUNCATE_CACHE) {
    truncateCache.clear();
  }
  truncateCache.set(cacheKey, bestResult);
  return bestResult;
}
