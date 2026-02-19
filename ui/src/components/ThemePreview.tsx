import type { Component } from "solid-js";
import type { VisualConfig } from "../lib/types";

interface ThemePreviewProps {
  visual: VisualConfig;
}

const ThemePreview: Component<ThemePreviewProps> = (props) => {
  const v = () => props.visual;

  return (
    <div
      class="theme-preview"
      style={{
        background: v().background_color,
        color: v().text_color,
      }}
    >
      <div class="theme-preview-search">
        <div
          class="theme-preview-search-input"
          style={{ background: v().input_background_color }}
        />
      </div>
      <div
        class="theme-preview-row"
        style={{ background: v().selected_row_color }}
      >
        <div
          class="theme-preview-icon"
          style={{ background: v().hint_text_color }}
        />
        <div class="theme-preview-text">
          <div
            class="theme-preview-name"
            style={{ background: v().text_color }}
          />
          <div
            class="theme-preview-path"
            style={{ background: v().hint_text_color }}
          />
        </div>
      </div>
      <div class="theme-preview-row">
        <div
          class="theme-preview-icon"
          style={{ background: v().hint_text_color }}
        />
        <div class="theme-preview-text">
          <div
            class="theme-preview-name"
            style={{ background: v().text_color, width: "45%" }}
          />
          <div
            class="theme-preview-path"
            style={{ background: v().hint_text_color, width: "70%" }}
          />
        </div>
      </div>
      <div class="theme-preview-row">
        <div
          class="theme-preview-icon"
          style={{ background: v().hint_text_color }}
        />
        <div class="theme-preview-text">
          <div
            class="theme-preview-name"
            style={{ background: v().text_color, width: "55%" }}
          />
          <div
            class="theme-preview-path"
            style={{ background: v().hint_text_color, width: "85%" }}
          />
        </div>
      </div>
    </div>
  );
};

export default ThemePreview;
