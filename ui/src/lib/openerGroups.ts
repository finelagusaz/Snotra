import { normalizeOpenerTarget } from "./openerTarget";
import type { OpenerRule, OpenerTool } from "./types";

export type GroupedOpenerEntry = {
  targetKind: "folder" | "ext";
  extensions: string[];
  tool: OpenerTool;
};

function cloneTool(tool: OpenerTool): OpenerTool {
  return {
    name: tool.name,
    exe: tool.exe,
    args: tool.args,
  };
}

export function cloneGroupedOpenerEntry(entry: GroupedOpenerEntry): GroupedOpenerEntry {
  return {
    targetKind: entry.targetKind,
    extensions: [...entry.extensions],
    tool: cloneTool(entry.tool),
  };
}

function normalizeToolPathKey(path: string): string {
  return path.trim().replace(/\//g, "\\").toLowerCase();
}

function normalizeToolArgsKey(args: string): string {
  return args.trim();
}

function buildToolIdentity(tool: OpenerTool): string {
  return `${normalizeToolPathKey(tool.exe)}\n${normalizeToolArgsKey(tool.args)}`;
}

function mergeExtensions(current: string[], next: string[]): string[] {
  return [...new Set([...current, ...next])].sort();
}

function extractExtensions(target: string): string[] {
  const normalized = normalizeOpenerTarget(target);
  if (!normalized.startsWith("ext:")) {
    return normalized === "folder" ? [] : [normalized];
  }
  const raw = normalized.slice(4);
  if (!raw) {
    return [];
  }
  return raw.split(",").filter((ext) => ext.length > 0);
}

function buildGroupedIdentity(entry: GroupedOpenerEntry): string {
  return `${entry.targetKind}\n${buildToolIdentity(entry.tool)}`;
}

export function isSameGroupedOpener(
  left: GroupedOpenerEntry,
  right: GroupedOpenerEntry,
): boolean {
  return buildGroupedIdentity(left) === buildGroupedIdentity(right);
}

export function mergeGroupedOpenerEntries(
  current: GroupedOpenerEntry,
  next: GroupedOpenerEntry,
): GroupedOpenerEntry {
  const useCurrentName =
    current.targetKind === "folder"
      ? current.tool.name.trim().length > 0
      : next.tool.name.trim().length === 0;
  const tool = cloneTool(next.tool);
  if (useCurrentName) {
    tool.name = current.tool.name;
  }

  return {
    targetKind: current.targetKind,
    extensions:
      current.targetKind === "ext"
        ? mergeExtensions(current.extensions, next.extensions)
        : [],
    tool,
  };
}

export function buildGroupedOpeners(openers: OpenerRule[]): GroupedOpenerEntry[] {
  const entries: GroupedOpenerEntry[] = [];

  for (const rule of openers) {
    const normalizedTarget = normalizeOpenerTarget(rule.target);
    const targetKind = normalizedTarget === "folder" ? "folder" : "ext";
    const extensions = targetKind === "ext" ? extractExtensions(normalizedTarget) : [];

    for (const tool of rule.tools) {
      const nextEntry: GroupedOpenerEntry = {
        targetKind,
        extensions,
        tool: cloneTool(tool),
      };
      const existingIndex = entries.findIndex((entry) => isSameGroupedOpener(entry, nextEntry));
      if (existingIndex >= 0) {
        entries[existingIndex] = mergeGroupedOpenerEntries(entries[existingIndex], nextEntry);
      } else {
        entries.push(nextEntry);
      }
    }
  }

  return entries;
}

type IndexedGroupedEntry = GroupedOpenerEntry & { index: number };

function buildExtToolListSignature(indices: number[]): string {
  return indices.join(",");
}

export function serializeGroupedOpeners(entries: GroupedOpenerEntry[]): OpenerRule[] {
  const indexedEntries: IndexedGroupedEntry[] = entries.map((entry, index) => ({
    ...cloneGroupedOpenerEntry(entry),
    index,
  }));

  const folderEntries = indexedEntries.filter((entry) => entry.targetKind === "folder");
  const extEntries = indexedEntries.filter((entry) => entry.targetKind === "ext");

  const extBuckets = new Map<
    string,
    { firstIndex: number; extensions: string[]; toolIndices: number[] }
  >();
  const extToToolIndices = new Map<string, number[]>();
  const extToFirstIndex = new Map<string, number>();
  const emptyExtEntries = extEntries.filter((entry) => entry.extensions.length === 0);

  for (const entry of extEntries) {
    for (const ext of entry.extensions) {
      const toolIndices = extToToolIndices.get(ext) ?? [];
      toolIndices.push(entry.index);
      extToToolIndices.set(ext, toolIndices);
      if (!extToFirstIndex.has(ext)) {
        extToFirstIndex.set(ext, entry.index);
      }
    }
  }

  for (const [ext, toolIndices] of extToToolIndices) {
    const signature = buildExtToolListSignature(toolIndices);
    const existing = extBuckets.get(signature);
    if (existing) {
      existing.extensions.push(ext);
    } else {
      extBuckets.set(signature, {
        firstIndex: extToFirstIndex.get(ext) ?? Number.MAX_SAFE_INTEGER,
        extensions: [ext],
        toolIndices: [...toolIndices],
      });
    }
  }

  const extRules: { firstIndex: number; rule: OpenerRule }[] = [];
  for (const bucket of extBuckets.values()) {
    extRules.push({
      firstIndex: bucket.firstIndex,
      rule: {
        target: `ext:${bucket.extensions.sort().join(",")}`,
        tools: bucket.toolIndices.map((index) => cloneTool(indexedEntries[index].tool)),
      },
    });
  }

  if (emptyExtEntries.length > 0) {
    extRules.push({
      firstIndex: emptyExtEntries[0].index,
      rule: {
        target: "ext:",
        tools: emptyExtEntries.map((entry) => cloneTool(entry.tool)),
      },
    });
  }

  extRules.sort((left, right) => left.firstIndex - right.firstIndex);

  const folderRule = folderEntries.length
    ? {
        firstIndex: folderEntries[0].index,
        rule: {
          target: "folder",
          tools: folderEntries.map((entry) => cloneTool(entry.tool)),
        },
      }
    : null;

  if (!folderRule) {
    return extRules.map((entry) => entry.rule);
  }

  if (extRules.length === 0) {
    return [folderRule.rule];
  }

  if (folderRule.firstIndex < extRules[0].firstIndex) {
    return [folderRule.rule, ...extRules.map((entry) => entry.rule)];
  }

  return [...extRules.map((entry) => entry.rule), folderRule.rule];
}
