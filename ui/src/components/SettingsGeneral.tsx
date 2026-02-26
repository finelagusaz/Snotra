import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";
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
  const hotkeyDisplay = () => (hotkeyInvalid() ? "なし" : hotkeyLabel());

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
        <div class="settings-group-title">ホットキー</div>
        <div class="settings-group-content">
          <SettingRow label="ホットキー" block>
            <input
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
            label="トグル動作"
            description="ホットキーで表示中のウィンドウを非表示にします"
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
        <div class="settings-group-title">動作</div>
        <div class="settings-group-content">
          <SettingRow
            label="起動時に表示"
            description="アプリ起動時に検索ウィンドウを自動表示します"
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
            label="フォーカス喪失時に非表示"
            description="他のウィンドウをクリックした時に自動で隠します"
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
            label="トレイアイコン"
            description="システムトレイにアイコンを表示します"
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
            label="表示時にIMEオフ"
            description="ウィンドウ表示時にIMEを自動でオフにします"
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
