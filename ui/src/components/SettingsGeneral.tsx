import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";
import { t } from "../lib/i18n";
import { formatHotkeyLabel, isHotkeyInvalid } from "../lib/hotkeyValidation";
import SettingRow from "./SettingRow";
import ToggleSwitch from "./ToggleSwitch";

const STANDALONE_MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);
const KEY_CODE_PREFIX = "Key";
const DIGIT_CODE_PREFIX = "Digit";

function buildModifier(e: KeyboardEvent): string {
  const modifiers: string[] = [];
  if (e.ctrlKey) modifiers.push("Ctrl");
  if (e.altKey) modifiers.push("Alt");
  if (e.shiftKey) modifiers.push("Shift");
  if (e.metaKey) modifiers.push("Win");
  return modifiers.join("+");
}

function normalizeImeConflictKey(e: KeyboardEvent): string | null {
  const key = e.key.toLowerCase();
  const code = e.code.toLowerCase();

  if (key === "capslock" || code === "capslock") return "CapsLock";
  if (key === "eisu") return "Eisu";
  if (key === "kana" || key === "kanamode") return "Kana";
  if (key === "nonconvert") return "NonConvert";
  if (key === "convert") return "Convert";

  if (
    key === "lang1" ||
    code === "lang1" ||
    key === "hangul" ||
    key === "hangulmode"
  ) {
    return "Lang1";
  }

  if (
    key === "lang2" ||
    code === "lang2" ||
    key === "hanja" ||
    key === "hanjamode"
  ) {
    return "Lang2";
  }

  return null;
}

function normalizeMainKey(e: KeyboardEvent): string | null {
  if (e.key === " ") return "Space";
  if (e.key === "Enter") return "Enter";

  const imeConflictKey = normalizeImeConflictKey(e);
  if (imeConflictKey) {
    return imeConflictKey;
  }

  if (e.code.startsWith(KEY_CODE_PREFIX)) {
    const letter = e.code.slice(KEY_CODE_PREFIX.length);
    if (/^[A-Z]$/.test(letter)) {
      return letter;
    }
  }

  if (e.code.startsWith(DIGIT_CODE_PREFIX)) {
    const digit = e.code.slice(DIGIT_CODE_PREFIX.length);
    if (/^[0-9]$/.test(digit)) {
      return digit;
    }
  }

  return null;
}

const SettingsGeneral: Component = () => {
  const d = () => draft()!;
  const hotkeyLabel = () => formatHotkeyLabel(d().hotkey.modifier, d().hotkey.key);
  const hotkeyInvalid = () => isHotkeyInvalid(d().hotkey.modifier, d().hotkey.key);
  const hotkeyDisplay = () => (hotkeyInvalid() ? t("settings.general.hotkey.none") : hotkeyLabel());

  const clearHotkey = () =>
    updateDraft((c) => {
      c.hotkey.modifier = "";
      c.hotkey.key = "";
    });

  const handleHotkeyKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Tab") {
      return;
    }

    e.preventDefault();

    if (e.key === "Backspace" || e.key === "Escape") {
      // SettingsWindow の window-level keydown リスナーへの伝播を止める（二重防御: リスナー側にも hotkey-input ガードあり）
      e.stopPropagation();
      clearHotkey();
      return;
    }

    const modifier = buildModifier(e);
    if (STANDALONE_MODIFIER_KEYS.has(e.key)) {
      updateDraft((c) => {
        c.hotkey.modifier = modifier;
        c.hotkey.key = "";
      });
      return;
    }

    const mainKey = normalizeMainKey(e);
    if (!mainKey) {
      return;
    }

    updateDraft((c) => {
      c.hotkey.modifier = modifier;
      c.hotkey.key = mainKey;
    });
  };

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">{t("settings.general.group.hotkey")}</div>
        <div class="settings-group-content">
          <SettingRow label={t("settings.general.hotkey.label")} block controlId="hotkey-input">
            <input
              id="hotkey-input"
              class="hotkey-input"
              classList={{ "hotkey-input--invalid": hotkeyInvalid() }}
              type="text"
              value={hotkeyDisplay()}
              readOnly
              style={{ "caret-color": "transparent", cursor: "pointer" }}
              onKeyDown={handleHotkeyKeyDown}
            />
          </SettingRow>
          <SettingRow
            label={t("settings.general.toggle.label")}
            description={t("settings.general.toggle.description")}
          >
            <ToggleSwitch
              checked={d().general.hotkey_toggle}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.hotkey_toggle = v;
                })
              }
            />
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">{t("settings.general.group.appearance")}</div>
        <div class="settings-group-content">
          <SettingRow
            label={t("settings.general.max_results.label")}
            description={t("settings.general.max_results.description")}
          >
            <input
              type="number"
              min="1"
              max="50"
              value={d().appearance.max_results}
              onInput={(e) =>
                updateDraft((c) => {
                  c.appearance.max_results =
                    Math.max(1, Math.min(50, parseInt(e.currentTarget.value) || 8));
                })
              }
              style={{ width: "80px" }}
            />
          </SettingRow>
          <SettingRow
            label={t("settings.general.window_width.label")}
            description={t("settings.general.window_width.description")}
          >
            <input
              type="number"
              min="300"
              max="1200"
              value={d().appearance.window_width}
              onInput={(e) =>
                updateDraft((c) => {
                  c.appearance.window_width =
                    Math.max(300, Math.min(1200, parseInt(e.currentTarget.value) || 600));
                })
              }
              style={{ width: "80px" }}
            />
          </SettingRow>
          <SettingRow
            label={t("settings.general.show_icons.label")}
            description={t("settings.general.show_icons.description")}
          >
            <ToggleSwitch
              checked={d().appearance.show_icons}
              onChange={(v) =>
                updateDraft((c) => {
                  c.appearance.show_icons = v;
                })
              }
            />
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">{t("settings.general.group.behavior")}</div>
        <div class="settings-group-content">
          <SettingRow
            label={t("settings.general.show_on_startup.label")}
            description={t("settings.general.show_on_startup.description")}
          >
            <ToggleSwitch
              checked={d().general.show_on_startup}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.show_on_startup = v;
                })
              }
            />
          </SettingRow>
          <SettingRow
            label={t("settings.general.auto_hide.label")}
            description={t("settings.general.auto_hide.description")}
          >
            <ToggleSwitch
              checked={d().general.auto_hide_on_focus_lost}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.auto_hide_on_focus_lost = v;
                })
              }
            />
          </SettingRow>
          <SettingRow
            label={t("settings.general.tray_icon.label")}
            description={t("settings.general.tray_icon.description")}
          >
            <ToggleSwitch
              checked={d().general.show_tray_icon}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.show_tray_icon = v;
                })
              }
            />
          </SettingRow>
          <SettingRow
            label={t("settings.general.ime_off.label")}
            description={t("settings.general.ime_off.description")}
          >
            <ToggleSwitch
              checked={d().general.ime_off_on_show}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.ime_off_on_show = v;
                })
              }
            />
          </SettingRow>
        </div>
      </div>

    </div>
  );
};

export default SettingsGeneral;
