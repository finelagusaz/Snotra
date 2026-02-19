import { invoke } from "@tauri-apps/api/core";
import type { Config, SearchResult } from "./types";

export async function search(query: string): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search", { query });
}

export async function getHistoryResults(): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("get_history_results");
}

export async function launchItem(
  path: string,
  query: string,
): Promise<void> {
  return invoke("launch_item", { path, query });
}

export async function listFolder(
  dir: string,
  filter: string,
): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("list_folder", { dir, filter });
}

export async function loadConfig(): Promise<Config> {
  return invoke<Config>("load_config");
}

export async function saveConfig(config: Config): Promise<void> {
  return invoke("save_config", { config });
}

export async function getConfig(): Promise<Config> {
  return invoke<Config>("get_config");
}

export async function getIconBase64(path: string): Promise<string | null> {
  return invoke<string | null>("get_icon_base64", { path });
}

export async function getIconsBatch(
  paths: string[],
): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_icons_batch", { paths });
}

export async function openSettings(): Promise<void> {
  return invoke("open_settings");
}

export interface WindowPlacement {
  x: number;
  y: number;
}

export interface WindowSize {
  width: number;
  height: number;
}

export async function getSearchPlacement(): Promise<WindowPlacement | null> {
  return invoke<WindowPlacement | null>("get_search_placement");
}

export async function saveSearchPlacement(x: number, y: number): Promise<void> {
  return invoke("save_search_placement", { x, y });
}

export async function getSettingsPlacement(): Promise<
  [WindowPlacement | null, WindowSize | null]
> {
  return invoke<[WindowPlacement | null, WindowSize | null]>(
    "get_settings_placement",
  );
}

export async function saveSettingsPlacement(
  x: number,
  y: number,
): Promise<void> {
  return invoke("save_settings_placement", { x, y });
}

export async function saveSettingsSize(
  width: number,
  height: number,
): Promise<void> {
  return invoke("save_settings_size", { width, height });
}

export async function setWindowNoActivate(): Promise<void> {
  return invoke("set_window_no_activate");
}

export async function notifyResultClicked(index: number): Promise<void> {
  return invoke("notify_result_clicked", { index });
}

export async function notifyResultDoubleClicked(index: number): Promise<void> {
  return invoke("notify_result_double_clicked", { index });
}

export async function getIndexingState(): Promise<boolean> {
  return invoke<boolean>("get_indexing_state");
}

export async function listSystemFonts(): Promise<string[]> {
  return invoke<string[]>("list_system_fonts");
}
