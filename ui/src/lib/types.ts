export interface SearchResult {
  name: string;
  path: string;
  isFolder: boolean;
  isError: boolean;
}

export interface HotkeyConfig {
  modifier: string;
  key: string;
}

export interface GeneralConfig {
  hotkey_toggle: boolean;
  show_on_startup: boolean;
  auto_hide_on_focus_lost: boolean;
  show_tray_icon: boolean;
  ime_off_on_show: boolean;
  show_title_bar: boolean;
  renderer: string;
  wgpu_backend: string;
}

export interface AppearanceConfig {
  max_results: number;
  window_width: number;
  top_n_history: number;
  max_history_display: number;
  show_icons: boolean;
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

export interface SearchConfig {
  normal_mode: string;
  folder_mode: string;
  show_hidden_system: boolean;
}

export interface ScanPath {
  path: string;
  extensions: string[];
  include_folders: boolean;
}

export interface PathsConfig {
  additional: string[];
  scan: ScanPath[];
}

export interface Config {
  hotkey: HotkeyConfig;
  general: GeneralConfig;
  appearance: AppearanceConfig;
  visual: VisualConfig;
  paths: PathsConfig;
  search: SearchConfig;
}
