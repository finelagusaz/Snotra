/**
 * フォルダナビゲーションの純ロジック。
 * SolidJS/api 非依存でテスト可能なため stores/ から分離している。
 */

/**
 * Compute the parent directory of the given path, or null if at root.
 *
 * Root boundaries:
 * - Drive root: `C:\` → null
 * - UNC share root: `\\server\share\` → null
 * - UNC partial: `\\server` → null (invalid, no share)
 */
export function computeParentDir(currentDir: string): string | null {
  // Strip trailing backslash for uniform handling (but keep drive root "X:\").
  let normalized = currentDir;
  if (normalized.length > 3 && normalized.endsWith("\\")) {
    normalized = normalized.slice(0, -1);
  }

  // UNC root: \\server\share (2 parts or fewer) → at root
  if (normalized.startsWith("\\\\")) {
    const parts = normalized
      .replace(/\\+$/, "")
      .slice(2)
      .split("\\")
      .filter((p) => p.length > 0);
    if (parts.length <= 2) return null;
  }

  let parent = normalized.replace(/\\[^\\]+$/, "");
  if (/^[A-Za-z]:$/.test(parent)) {
    parent += "\\";
  }
  if (parent === normalized || parent === "") {
    return null;
  }
  // Guard: parent is \\server (below share level) → stop
  if (parent.startsWith("\\\\")) {
    const parentParts = parent
      .replace(/\\+$/, "")
      .slice(2)
      .split("\\")
      .filter((p) => p.length > 0);
    if (parentParts.length < 2) return null;
  }
  return parent;
}

/**
 * Clamp a selection index within [0, len-1]. Returns 0 if len <= 0.
 */
export function clampSelectedIndex(index: number, len: number): number {
  if (len <= 0) return 0;
  return Math.min(Math.max(index, 0), len - 1);
}
