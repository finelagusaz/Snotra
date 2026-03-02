import { type Component, For, Show, createMemo, createResource, createSignal } from "solid-js";
import * as api from "../lib/invoke";
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
  {
    value: "monokai",
    label: "Monokai",
    colors: {
      background_color: "#272822",
      input_background_color: "#3e3d32",
      text_color: "#f8f8f2",
      selected_row_color: "#49483e",
      hint_text_color: "#75715e",
    },
  },
] as const;

type PresetColors = (typeof PRESETS)[number]["colors"];
type ColorKey = keyof PresetColors;

const COLOR_KEYS: ColorKey[] = [
  "background_color",
  "input_background_color",
  "text_color",
  "selected_row_color",
  "hint_text_color",
];

interface ColorFieldDef {
  key: ColorKey;
  label: string;
}

const COLOR_FIELDS: ColorFieldDef[] = [
  { key: "background_color", label: "背景色" },
  { key: "input_background_color", label: "入力欄背景色" },
  { key: "text_color", label: "テキスト色" },
  { key: "selected_row_color", label: "選択行色" },
  { key: "hint_text_color", label: "ヒントテキスト色" },
];

function colorsMatch(visual: { [K in ColorKey]: string }, target: { [K in ColorKey]: string }): boolean {
  return COLOR_KEYS.every((k) => visual[k].toLowerCase() === target[k].toLowerCase());
}

const SettingsVisual: Component = () => {
  const d = () => draft()!;
  const [fonts] = createResource(api.listSystemFonts);
  const [hexErrors, setHexErrors] = createSignal<Set<string>>(new Set());

  function applyPreset(presetValue: string) {
    const preset = PRESETS.find((p) => p.value === presetValue);
    if (!preset) return;
    updateDraft((c) => {
      c.visual.preset = preset.value;
      Object.assign(c.visual, preset.colors);
    });
    setHexErrors(new Set<string>());
  }

  const activePreset = createMemo((): string | null => {
    const v = d().visual;
    for (const p of PRESETS) {
      if (colorsMatch(v, p.colors)) return p.value;
    }
    const ct = v.custom_theme;
    if (ct && colorsMatch(v, ct)) return "custom";
    return null;
  });

  function canSaveCustom(): boolean {
    return activePreset() === null;
  }

  function saveCustomTheme() {
    updateDraft((c) => {
      c.visual.custom_theme = {
        background_color: c.visual.background_color,
        input_background_color: c.visual.input_background_color,
        text_color: c.visual.text_color,
        selected_row_color: c.visual.selected_row_color,
        hint_text_color: c.visual.hint_text_color,
      };
      c.visual.preset = "custom";
    });
  }

  function deleteCustomTheme(e: MouseEvent | KeyboardEvent) {
    e.stopPropagation();
    updateDraft((c) => {
      c.visual.custom_theme = undefined;
    });
  }

  function applyCustomTheme() {
    const ct = d().visual.custom_theme;
    if (!ct) return;
    updateDraft((c) => {
      c.visual.preset = "custom";
      Object.assign(c.visual, ct);
    });
    setHexErrors(new Set<string>());
  }

  function updateColor(key: ColorKey, value: string) {
    updateDraft((c) => {
      c.visual[key] = value;
      c.visual.preset = "custom";
    });
  }

  return (
    <div class="settings-section">
      <div class="settings-group settings-group--sticky">
        <div class="settings-group-title">プレビュー</div>
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
                classList={{ active: activePreset() === preset.value }}
                onClick={() => applyPreset(preset.value)}
              >
                <div class="preset-swatches">
                  <div
                    class="swatch"
                    style={{ background: preset.colors.background_color }}
                  />
                  <div
                    class="swatch"
                    style={{ background: preset.colors.input_background_color }}
                  />
                  <div
                    class="swatch"
                    style={{ background: preset.colors.text_color }}
                  />
                  <div
                    class="swatch"
                    style={{ background: preset.colors.selected_row_color }}
                  />
                  <div
                    class="swatch"
                    style={{ background: preset.colors.hint_text_color }}
                  />
                </div>
                {preset.label}
              </button>
            ))}
            <Show when={d().visual.custom_theme}>
              {(ct) => (
                <button
                  class="preset-card"
                  classList={{ active: activePreset() === "custom" }}
                  onClick={() => applyCustomTheme()}
                >
                  <span
                    class="custom-theme-delete"
                    role="button"
                    tabIndex={0}
                    onClick={(e) => deleteCustomTheme(e)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        deleteCustomTheme(e);
                      }
                    }}
                    title="マイテーマを削除"
                    aria-label="マイテーマを削除"
                  >
                    ×
                  </span>
                  <div class="preset-swatches">
                    <div class="swatch" style={{ background: ct().background_color }} />
                    <div class="swatch" style={{ background: ct().input_background_color }} />
                    <div class="swatch" style={{ background: ct().text_color }} />
                    <div class="swatch" style={{ background: ct().selected_row_color }} />
                    <div class="swatch" style={{ background: ct().hint_text_color }} />
                  </div>
                  マイテーマ
                </button>
              )}
            </Show>
          </div>
          <Show when={canSaveCustom()}>
            <button class="custom-theme-save" onClick={() => saveCustomTheme()}>
              現在の配色を保存
            </button>
          </Show>
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
                    aria-label={field.label}
                    value={d().visual[field.key]}
                    onInput={(e) => {
                      setHexErrors((prev) => {
                        const next = new Set(prev);
                        next.delete(field.key);
                        return next;
                      });
                      updateColor(field.key, e.currentTarget.value);
                    }}
                  />
                </div>
                <input
                  class="color-hex-input"
                  classList={{ "color-hex-input--invalid": hexErrors().has(field.key) }}
                  type="text"
                  value={d().visual[field.key]}
                  onInput={(e) => {
                    const val = e.currentTarget.value;
                    const valid = /^#[0-9a-fA-F]{6}$/.test(val);
                    setHexErrors((prev) => {
                      const next = new Set(prev);
                      if (val.length > 0 && !valid) {
                        next.add(field.key);
                      } else {
                        next.delete(field.key);
                      }
                      return next;
                    });
                    if (valid) updateColor(field.key, val);
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
          <SettingRow label="フォントファミリー">
            <select
              value={d().visual.font_family}
              disabled={fonts.loading}
              onChange={(e) =>
                updateDraft((c) => {
                  c.visual.font_family = e.currentTarget.value;
                })
              }
            >
              <Show when={fonts.loading}>
                <option value={d().visual.font_family}>{d().visual.font_family}（読み込み中...）</option>
              </Show>
              <For each={fonts() ?? []}>{(f) =>
                <option value={f}>{f}</option>
              }</For>
            </select>
          </SettingRow>
          <SettingRow label="フォントサイズ">
            <input
              type="number"
              min="8"
              max="48"
              value={d().visual.font_size}
              onInput={(e) =>
                updateDraft((c) => {
                  c.visual.font_size = Math.max(8, Math.min(48, parseInt(e.currentTarget.value) || 15));
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
