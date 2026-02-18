import type { VisualConfig } from "./types";

export function applyTheme(visual: VisualConfig) {
  const root = document.documentElement;
  root.style.setProperty("--bg-color", visual.background_color);
  root.style.setProperty("--input-bg-color", visual.input_background_color);
  root.style.setProperty("--text-color", visual.text_color);
  root.style.setProperty("--selected-row-color", visual.selected_row_color);
  root.style.setProperty("--hint-text-color", visual.hint_text_color);
  root.style.setProperty("--font-family", visual.font_family);
  root.style.setProperty("--font-size", `${visual.font_size}px`);
}
