import { invoke } from "@tauri-apps/api/core";
import type { BootstrapPayload, OpenerTool, SearchResult } from "./types";
import { trace } from "./trace";

let invokeSeq = 0;

function roundMs(v: number): number {
  return Math.round(v * 1000) / 1000;
}

async function tracedInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const invokeId = ++invokeSeq;
  const startedAt = performance.now();
  trace("invoke:start", { invokeId, command, args });
  try {
    const result = await invoke<T>(command, args);
    trace("invoke:ok", {
      invokeId,
      command,
      elapsedMs: roundMs(performance.now() - startedAt),
    });
    return result;
  } catch (e) {
    trace("invoke:error", {
      invokeId,
      command,
      elapsedMs: roundMs(performance.now() - startedAt),
      error: String(e),
    });
    throw e;
  }
}

export async function search(query: string): Promise<SearchResult[]> {
  return tracedInvoke<SearchResult[]>("search", { query });
}

export async function getHistoryResults(): Promise<SearchResult[]> {
  return tracedInvoke<SearchResult[]>("get_history_results");
}

export type LaunchStatus = "ok" | "failed" | "timeout";

export interface LaunchResult {
  status: LaunchStatus;
  code: number;
  message: string | null;
}

export async function launchItem(
  path: string,
  query: string,
): Promise<LaunchResult> {
  return tracedInvoke<LaunchResult>("launch_item", { path, query });
}

export async function listFolder(
  dir: string,
  filter: string,
): Promise<SearchResult[]> {
  return tracedInvoke<SearchResult[]>("list_folder", { dir, filter });
}

export async function getBootstrapPayload(): Promise<BootstrapPayload> {
  return tracedInvoke<BootstrapPayload>("get_bootstrap_payload");
}

export async function getIconsBatch(paths: string[]): Promise<ArrayBuffer> {
  return tracedInvoke<ArrayBuffer>("get_icons_batch", { paths });
}

export async function openSettings(): Promise<void> {
  return tracedInvoke("open_settings");
}

export interface WindowPlacement {
  x: number;
  y: number;
}

export async function getSearchPlacement(): Promise<WindowPlacement | null> {
  return tracedInvoke<WindowPlacement | null>("get_search_placement");
}

export async function saveSearchPlacement(x: number, y: number): Promise<void> {
  return tracedInvoke("save_search_placement", { x, y });
}

export async function setWindowNoActivate(): Promise<void> {
  return tracedInvoke("set_window_no_activate");
}

export async function notifyMainShown(): Promise<void> {
  return tracedInvoke("notify_main_shown");
}

export async function notifyMainHidden(): Promise<void> {
  return tracedInvoke("notify_main_hidden");
}

export async function notifyResultClicked(index: number): Promise<void> {
  return tracedInvoke("notify_result_clicked", { index });
}

export async function notifyResultDoubleClicked(index: number): Promise<void> {
  return tracedInvoke("notify_result_double_clicked", { index });
}

export async function notifyResultHovered(index: number): Promise<void> {
  return tracedInvoke("notify_result_hovered", { index });
}

export async function getIndexingState(): Promise<boolean> {
  return tracedInvoke<boolean>("get_indexing_state");
}

export async function rebuildIndex(): Promise<boolean> {
  return tracedInvoke<boolean>("rebuild_index");
}

export async function quitApp(): Promise<void> {
  return tracedInvoke("quit_app");
}

export async function ensureWindow(label: "results"): Promise<boolean> {
  return tracedInvoke<boolean>("ensure_window", { label });
}

export async function isMainForeground(): Promise<boolean> {
  return tracedInvoke<boolean>("is_main_foreground");
}

export async function recordFolderExpansion(path: string): Promise<void> {
  return tracedInvoke("record_folder_expansion", { path });
}

export async function getMatchingTools(
  path: string,
  isFolder: boolean,
): Promise<OpenerTool[]> {
  return tracedInvoke<OpenerTool[]>("get_matching_tools", { path, isFolder });
}

export async function launchWithTool(
  path: string,
  query: string,
  toolExe: string,
  toolArgs: string,
): Promise<LaunchResult> {
  return tracedInvoke<LaunchResult>("launch_with_tool", { path, query, toolExe, toolArgs });
}
