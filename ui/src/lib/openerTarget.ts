import { t } from "./i18n";

function normalizeExtensionToken(ext: string): string {
  const trimmed = ext.trim().replace(/^\.+/, "");
  if (!trimmed) {
    return "";
  }
  return `.${trimmed.toLowerCase()}`;
}

export function normalizeOpenerTarget(target: string): string {
  const trimmed = target.trim();
  if (trimmed.toLowerCase() === "folder") {
    return "folder";
  }

  const separatorIndex = trimmed.indexOf(":");
  if (
    separatorIndex >= 0 &&
    trimmed.slice(0, separatorIndex).toLowerCase() === "ext"
  ) {
    const exts = trimmed
      .slice(separatorIndex + 1)
      .split(",")
      .map(normalizeExtensionToken)
      .filter((ext) => ext.length > 0);
    const normalized = [...new Set(exts)].sort();
    return `ext:${normalized.join(",")}`;
  }

  return trimmed;
}

export function getOpenerTargetExtensions(target: string): string {
  const normalized = normalizeOpenerTarget(target);
  return normalized.startsWith("ext:") ? normalized.slice(4) : normalized;
}

export function getOpenerTargetLabel(target: string): string {
  const normalized = normalizeOpenerTarget(target);
  if (normalized === "folder") {
    return t("settings.opener.target.folder");
  }
  return normalized.startsWith("ext:") ? normalized.slice(4) : normalized;
}
