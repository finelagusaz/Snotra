let canvas: HTMLCanvasElement | null = null;

function getContext(): CanvasRenderingContext2D {
  if (!canvas) {
    canvas = document.createElement("canvas");
  }
  return canvas.getContext("2d")!;
}

function measureText(text: string, font: string): number {
  const ctx = getContext();
  ctx.font = font;
  return ctx.measureText(text).width;
}

/**
 * Truncate a path by collapsing middle segments with "..." to fit within maxWidth.
 *
 * Strategy:
 *   1. If the full path fits, return it as-is.
 *   2. Split by "\", keep the first segment (drive) and last segment (filename).
 *   3. Progressively add middle segments from the right (closest to filename).
 *   4. Return the most segments that fit, replacing the rest with "...".
 *   5. Minimal form: "C:\...\filename.exe"
 */
export function truncatePath(
  path: string,
  maxWidth: number,
  font: string,
): string {
  if (maxWidth <= 0) return path;

  if (measureText(path, font) <= maxWidth) {
    return path;
  }

  const sep = "\\";
  const segments = path.split(sep);

  // If 2 or fewer segments, cannot truncate further
  if (segments.length <= 2) {
    return path;
  }

  const first = segments[0]; // e.g. "C:"
  const last = segments[segments.length - 1]; // e.g. "filename.exe"
  const middle = segments.slice(1, -1); // everything in between

  // Minimal form: first\...\last
  const minimal = first + sep + "..." + sep + last;
  if (measureText(minimal, font) > maxWidth) {
    return minimal;
  }

  // Try adding middle segments from the right
  // Start with none, add one more from the right each iteration
  let bestResult = minimal;

  for (let keepRight = 1; keepRight <= middle.length; keepRight++) {
    const rightSegments = middle.slice(middle.length - keepRight);
    const candidate =
      first + sep + "..." + sep + rightSegments.join(sep) + sep + last;

    if (measureText(candidate, font) <= maxWidth) {
      bestResult = candidate;
    } else {
      break;
    }
  }

  return bestResult;
}
