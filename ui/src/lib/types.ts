export interface SearchResult {
  name: string;
  path: string;
  isFolder: boolean;
  isError: boolean;
}

export interface OpenerTool {
  name: string;
  exe: string;
  args: string;
}

export interface VisualConfig {
  preset: string;
  background_color: string;
  input_background_color: string;
  text_color: string;
  selected_row_color: string;
  hint_text_color: string;
  font_family: string;
  font_size: number;
}

export interface BootstrapGeneralConfig {
  auto_hide_on_focus_lost: boolean;
}

export interface BootstrapAppearanceConfig {
  show_icons: boolean;
  max_results: number;
}

export interface BootstrapPayload {
  visual: VisualConfig;
  general: BootstrapGeneralConfig;
  appearance: BootstrapAppearanceConfig;
  indexing: boolean;
}
