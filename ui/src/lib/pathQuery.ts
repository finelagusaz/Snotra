export type PathQuery = {
  dir: string;
  filter: string;
};

const DRIVE_ROOT_RE = /^[A-Za-z]:\\?$/;

function normalizePathInput(input: string): string {
  return input.trim().replace(/[\/¥]/g, "\\");
}

export function parsePathQuery(input: string): PathQuery | null {
  const normalized = normalizePathInput(input);
  if (normalized === "") {
    return null;
  }

  if (DRIVE_ROOT_RE.test(normalized)) {
    const drive = normalized[0];
    return { dir: `${drive}:\\`, filter: "" };
  }

  if (normalized.startsWith("\\\\")) {
    const parts = normalized.slice(2).split("\\").filter((p) => p.length > 0);
    if (parts.length < 2) {
      return null;
    }
    if (parts.length === 2) {
      return { dir: `\\\\${parts[0]}\\${parts[1]}\\`, filter: "" };
    }
  }

  if (!normalized.includes("\\")) {
    return null;
  }

  if (normalized.endsWith("\\")) {
    return { dir: normalized, filter: "" };
  }

  const lastSlash = normalized.lastIndexOf("\\");
  if (lastSlash < 0) {
    return null;
  }

  let dir = normalized.slice(0, lastSlash + 1);
  if (dir === "") {
    dir = "\\";
  }

  return {
    dir,
    filter: normalized.slice(lastSlash + 1),
  };
}

export function isPathQuery(input: string): boolean {
  return parsePathQuery(input) !== null;
}
