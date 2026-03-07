const MODIFIER_SEPARATOR = "+";
const FORBIDDEN_MAIN_KEYS = new Set([
  "capslock",
  "caps",
  "eisu",
  "kana",
  "kanamode",
  "nonconvert",
  "convert",
  "lang1",
  "lang2",
  "hangul",
  "hangulmode",
  "hanja",
  "hanjamode",
]);

// Forbidden Windows system shortcuts: "normalized_modifier|normalized_key"
// modifier_normalized: parts split by '+', trimmed, alias-resolved, lowercased, sorted, rejoined
// key_normalized: alias-resolved, trimmed, lowercased
// Entries must be pre-sorted alphabetically (e.g. "alt+ctrl" not "ctrl+alt").
const SYSTEM_SHORTCUT_CONFLICTS = new Set([
  "alt|f4",
  "alt|space", // Alt+Space: Windows system menu (SC_KEYMENU)
  "alt|tab",
  "alt+ctrl|delete", // Ctrl+Alt+Delete: sorted alt < ctrl
  "ctrl+shift|escape", // Ctrl+Shift+Escape: sorted ctrl < shift
]);

function parseModifierParts(modifier: string): string[] {
  return modifier
    .split(MODIFIER_SEPARATOR)
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

function normalizeModifierPart(part: string): string {
  const lower = part.toLowerCase();
  // Resolve aliases to match hotkey.rs parse_modifier()
  if (lower === "control") return "ctrl";
  if (lower === "super" || lower === "meta") return "win";
  return lower;
}

function normalizeKey(key: string): string {
  // Resolve aliases to match hotkey.rs parse_vk()
  const lower = key.toLowerCase();
  if (lower === "esc") return "escape";
  if (lower === "return") return "enter";
  if (lower === "del") return "delete";
  return lower;
}

function normalizeModifier(modifier: string): string {
  return parseModifierParts(modifier)
    .map(normalizeModifierPart)
    .sort()
    .join("+");
}

export function formatHotkeyLabel(modifier: string, key: string): string {
  return [modifier, key].filter(Boolean).join(MODIFIER_SEPARATOR);
}

export function isHotkeyInvalid(modifier: string, key: string): boolean {
  const normalizedKey = key.trim();
  if (!normalizedKey) {
    return true;
  }

  const normalizedKeyLower = normalizedKey.toLowerCase();
  if (FORBIDDEN_MAIN_KEYS.has(normalizedKeyLower)) {
    return true;
  }

  const modifierParts = parseModifierParts(modifier);
  if (modifierParts.length === 0) {
    return true;
  }

  const hasWinModifier = modifierParts.some((part) => {
    const normalized = part.toLowerCase();
    return normalized === "win" || normalized === "super" || normalized === "meta";
  });
  if (hasWinModifier) {
    return true;
  }

  const conflictKey = `${normalizeModifier(modifier)}|${normalizeKey(normalizedKeyLower)}`;
  if (SYSTEM_SHORTCUT_CONFLICTS.has(conflictKey)) {
    return true;
  }

  return false;
}
