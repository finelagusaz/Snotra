export interface SearchResult {
  name: string;
  path: string;
  isFolder: boolean;
  isError: boolean;
  /** インスタントコマンドモード時の副表示テキスト（description or command テンプレート） */
  description?: string;
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
  auto_update: "full" | "check_only" | "disabled";
}

export interface BootstrapAppearanceConfig {
  show_icons: boolean;
  visible_rows: number;
}

export interface BootstrapPayload {
  visual: VisualConfig;
  general: BootstrapGeneralConfig;
  appearance: BootstrapAppearanceConfig;
  language: "ja" | "en";
  indexing: boolean;
  instant_command_prefix: string;
  result_limit: number;
}

export interface InstantCommand {
  name: string;
  command: string;
  description: string;
}

export interface UpdateAvailablePayload {
  version: string;
  can_install: boolean;
}
