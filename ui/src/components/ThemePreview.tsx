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
        "font-family": v().font_family || undefined,
        "font-size": `${v().font_size || 15}px`,
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
          <div class="theme-preview-path" style={{ color: v().text_color }}>
            C:\Windows\notepad.exe
          </div>
        </div>
      </div>
      <div class="theme-preview-row">
        <div
          class="theme-preview-icon"
          style={{ background: v().hint_text_color }}
        />
        <div class="theme-preview-text">
          <div class="theme-preview-path" style={{ color: v().text_color }}>
            C:\Windows\System32\calc.exe
          </div>
        </div>
      </div>
      <div class="theme-preview-row">
        <div
          class="theme-preview-icon"
          style={{ background: v().hint_text_color }}
        />
        <div class="theme-preview-text">
          <div class="theme-preview-path" style={{ color: v().text_color }}>
            C:\Windows\System32\mspaint.exe
          </div>
        </div>
      </div>
    </div>
  );
};

export default ThemePreview;
