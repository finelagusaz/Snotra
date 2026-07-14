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

/** モーダル的なビュー（フォルダ展開・ツール選択）が退避する「戻り先」の共通部分。
 *  `search.ts` の `saveView()`/`restoreView()` が唯一の生成/復元経路（choke point）。
 *  各 frame（`FolderFrame`/`ToolSelectionFrame`）はこれを `kind` 判別子付きで拡張する。
 *  かつて同名 `savedQuery` が担っていた別概念は、用途別の名前（folder=`restoreQuery`／
 *  tool=`launchQuery`）と `ModalFrame` union により型で分離済み（#538）。 */
export interface SavedViewState {
  savedResults: SearchResult[];
  savedSelected: number;
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
  display: string;
  description: string;
}

export interface UpdateAvailablePayload {
  version: string;
  can_install: boolean;
}
