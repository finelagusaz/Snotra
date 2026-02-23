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

function parseModifierParts(modifier: string): string[] {
  return modifier
    .split(MODIFIER_SEPARATOR)
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
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

  const isAltOnly =
    modifierParts.length === 1 && modifierParts[0].toLowerCase() === "alt";
  if (isAltOnly && normalizedKeyLower === "space") {
    return true;
  }

  return false;
}
