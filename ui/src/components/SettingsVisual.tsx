import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";
import SettingRow from "./SettingRow";
import ThemePreview from "./ThemePreview";

const PRESETS = [
  {
    value: "obsidian",
    label: "Obsidian",
    colors: {
      background_color: "#282828",
      input_background_color: "#383838",
      text_color: "#E0E0E0",
      selected_row_color: "#505050",
      hint_text_color: "#808080",
    },
  },
  {
    value: "paper",
    label: "Paper",
    colors: {
      background_color: "#ffffff",
      input_background_color: "#f2f2f2",
      text_color: "#111111",
      selected_row_color: "#d0d0d0",
      hint_text_color: "#666666",
    },
  },
  {
    value: "solarized",
    label: "Solarized",
    colors: {
      background_color: "#002b36",
      input_background_color: "#073642",
      text_color: "#839496",
      selected_row_color: "#073642",
      hint_text_color: "#586e75",
    },
  },
] as const;

interface ColorFieldDef {
  key: "background_color" | "input_background_color" | "text_color" | "selected_row_color" | "hint_text_color";
  label: string;
}

const COLOR_FIELDS: ColorFieldDef[] = [
  { key: "background_color", label: "背景色" },
  { key: "input_background_color", label: "入力欄背景色" },
  { key: "text_color", label: "テキスト色" },
  { key: "selected_row_color", label: "選択行色" },
  { key: "hint_text_color", label: "ヒントテキスト色" },
];

const SettingsVisual: Component = () => {
  const d = () => draft()!;

  function applyPreset(presetValue: string) {
    const preset = PRESETS.find((p) => p.value === presetValue);
    if (!preset) return;
    updateDraft((c) => {
      c.visual.preset = preset.value;
      Object.assign(c.visual, preset.colors);
    });
  }

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-content" style={{ "align-items": "flex-start" }}>
          <ThemePreview visual={d().visual} />
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">テーマ</div>
        <div class="settings-group-content">
          <div class="preset-cards">
            {PRESETS.map((preset) => (
              <button
                class="preset-card"
                classList={{ active: d().visual.preset === preset.value }}
                onClick={() => applyPreset(preset.value)}
              >
                <div class="preset-swatches">
                  <div
                    class="swatch"
                    style={{ background: preset.colors.background_color }}
                  />
                  <div
                    class="swatch"
                    style={{ background: preset.colors.text_color }}
                  />
                  <div
                    class="swatch"
                    style={{ background: preset.colors.selected_row_color }}
                  />
                </div>
                {preset.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">カラー</div>
        <div class="settings-group-content">
          {COLOR_FIELDS.map((field) => (
            <SettingRow label={field.label}>
              <div class="color-picker-row">
                <div class="color-swatch">
                  <input
                    type="color"
                    value={d().visual[field.key]}
                    onInput={(e) =>
                      updateDraft((c) => {
                        c.visual[field.key] = e.currentTarget.value;
                      })
                    }
                  />
                </div>
                <input
                  class="color-hex-input"
                  type="text"
                  value={d().visual[field.key]}
                  onInput={(e) => {
                    const val = e.currentTarget.value;
                    if (/^#[0-9a-fA-F]{6}$/.test(val)) {
                      updateDraft((c) => {
                        c.visual[field.key] = val;
                      });
                    }
                  }}
                />
              </div>
            </SettingRow>
          ))}
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">フォント</div>
        <div class="settings-group-content">
          <SettingRow label="フォントファミリー" block>
            <input
              type="text"
              value={d().visual.font_family}
              onInput={(e) =>
                updateDraft((c) => {
                  c.visual.font_family = e.currentTarget.value;
                })
              }
            />
          </SettingRow>
          <SettingRow label="フォントサイズ">
            <input
              type="number"
              min="8"
              max="48"
              value={d().visual.font_size}
              onInput={(e) =>
                updateDraft((c) => {
                  c.visual.font_size = parseInt(e.currentTarget.value) || 15;
                })
              }
              style={{ width: "80px" }}
            />
          </SettingRow>
        </div>
      </div>
    </div>
  );
};

export default SettingsVisual;
