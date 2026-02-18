import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";

const SettingsVisual: Component = () => {
  const d = () => draft()!;

  return (
    <div class="settings-section">
      <label>
        テーマプリセット
        <select
          value={d().visual.preset}
          onChange={(e) =>
            updateDraft((c) => {
              c.visual.preset = e.currentTarget.value;
              // Apply preset defaults
              switch (e.currentTarget.value) {
                case "obsidian":
                  c.visual.background_color = "#282828";
                  c.visual.input_background_color = "#383838";
                  c.visual.text_color = "#E0E0E0";
                  c.visual.selected_row_color = "#505050";
                  c.visual.hint_text_color = "#808080";
                  break;
                case "paper":
                  c.visual.background_color = "#ffffff";
                  c.visual.input_background_color = "#f2f2f2";
                  c.visual.text_color = "#111111";
                  c.visual.selected_row_color = "#d0d0d0";
                  c.visual.hint_text_color = "#666666";
                  break;
                case "solarized":
                  c.visual.background_color = "#002b36";
                  c.visual.input_background_color = "#073642";
                  c.visual.text_color = "#839496";
                  c.visual.selected_row_color = "#073642";
                  c.visual.hint_text_color = "#586e75";
                  break;
              }
            })
          }
        >
          <option value="obsidian">Obsidian</option>
          <option value="paper">Paper</option>
          <option value="solarized">Solarized</option>
        </select>
      </label>

      <label>
        背景色
        <input
          type="color"
          value={d().visual.background_color}
          onInput={(e) =>
            updateDraft((c) => {
              c.visual.background_color = e.currentTarget.value;
            })
          }
        />
      </label>
      <label>
        入力欄背景色
        <input
          type="color"
          value={d().visual.input_background_color}
          onInput={(e) =>
            updateDraft((c) => {
              c.visual.input_background_color = e.currentTarget.value;
            })
          }
        />
      </label>
      <label>
        テキスト色
        <input
          type="color"
          value={d().visual.text_color}
          onInput={(e) =>
            updateDraft((c) => {
              c.visual.text_color = e.currentTarget.value;
            })
          }
        />
      </label>
      <label>
        選択行色
        <input
          type="color"
          value={d().visual.selected_row_color}
          onInput={(e) =>
            updateDraft((c) => {
              c.visual.selected_row_color = e.currentTarget.value;
            })
          }
        />
      </label>
      <label>
        ヒントテキスト色
        <input
          type="color"
          value={d().visual.hint_text_color}
          onInput={(e) =>
            updateDraft((c) => {
              c.visual.hint_text_color = e.currentTarget.value;
            })
          }
        />
      </label>

      <label>
        フォントファミリー
        <input
          type="text"
          value={d().visual.font_family}
          onInput={(e) =>
            updateDraft((c) => {
              c.visual.font_family = e.currentTarget.value;
            })
          }
        />
      </label>
      <label>
        フォントサイズ
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
        />
      </label>
    </div>
  );
};

export default SettingsVisual;
