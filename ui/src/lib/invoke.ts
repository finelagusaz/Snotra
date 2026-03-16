import { invoke } from "@tauri-apps/api/core";
import type { BootstrapPayload, InstantCommand, OpenerTool, SearchResult } from "./types";
import { trace } from "./trace";

const IPC = {
  SEARCH: "search",
  GET_HISTORY_RESULTS: "get_history_results",
  LAUNCH_ITEM: "launch_item",
  LIST_FOLDER: "list_folder",
  GET_BOOTSTRAP_PAYLOAD: "get_bootstrap_payload",
  GET_ICONS_BATCH: "get_icons_batch",
  OPEN_SETTINGS: "open_settings",
  SAVE_SEARCH_PLACEMENT: "save_search_placement",
  NOTIFY_MAIN_SHOWN: "notify_main_shown",
  NOTIFY_MAIN_HIDDEN: "notify_main_hidden",
  GET_INDEXING_STATE: "get_indexing_state",
  REBUILD_INDEX: "rebuild_index",
  QUIT_APP: "quit_app",
  RECORD_FOLDER_EXPANSION: "record_folder_expansion",
  GET_MATCHING_TOOLS: "get_matching_tools",
  LAUNCH_WITH_TOOL: "launch_with_tool",
  GET_INSTANT_COMMANDS: "get_instant_commands",
  EXECUTE_INSTANT_COMMAND: "execute_instant_command",
  RESTART_APP: "restart_app",
} as const;

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
  return tracedInvoke<SearchResult[]>(IPC.SEARCH, { query });
}

export async function getHistoryResults(): Promise<SearchResult[]> {
  return tracedInvoke<SearchResult[]>(IPC.GET_HISTORY_RESULTS);
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
  return tracedInvoke<LaunchResult>(IPC.LAUNCH_ITEM, { path, query });
}

export async function listFolder(
  dir: string,
  filter: string,
): Promise<SearchResult[]> {
  return tracedInvoke<SearchResult[]>(IPC.LIST_FOLDER, { dir, filter });
}

export async function getBootstrapPayload(): Promise<BootstrapPayload> {
  return tracedInvoke<BootstrapPayload>(IPC.GET_BOOTSTRAP_PAYLOAD);
}

export async function getIconsBatch(paths: string[]): Promise<ArrayBuffer> {
  return tracedInvoke<ArrayBuffer>(IPC.GET_ICONS_BATCH, { paths });
}

export async function openSettings(): Promise<void> {
  return tracedInvoke(IPC.OPEN_SETTINGS);
}

export async function saveSearchPlacement(): Promise<void> {
  return tracedInvoke(IPC.SAVE_SEARCH_PLACEMENT);
}

export async function notifyMainShown(): Promise<void> {
  return tracedInvoke(IPC.NOTIFY_MAIN_SHOWN);
}

export async function notifyMainHidden(): Promise<void> {
  return tracedInvoke(IPC.NOTIFY_MAIN_HIDDEN);
}

export async function getIndexingState(): Promise<boolean> {
  return tracedInvoke<boolean>(IPC.GET_INDEXING_STATE);
}

export async function rebuildIndex(): Promise<boolean> {
  return tracedInvoke<boolean>(IPC.REBUILD_INDEX);
}

export async function quitApp(): Promise<void> {
  return tracedInvoke(IPC.QUIT_APP);
}

export async function recordFolderExpansion(path: string): Promise<void> {
  return tracedInvoke(IPC.RECORD_FOLDER_EXPANSION, { path });
}

export async function getMatchingTools(
  path: string,
  isFolder: boolean,
): Promise<OpenerTool[]> {
  return tracedInvoke<OpenerTool[]>(IPC.GET_MATCHING_TOOLS, { path, isFolder });
}

export async function launchWithTool(
  path: string,
  query: string,
  toolExe: string,
  toolArgs: string,
): Promise<LaunchResult> {
  return tracedInvoke<LaunchResult>(IPC.LAUNCH_WITH_TOOL, { path, query, toolExe, toolArgs });
}

export async function getInstantCommands(
  prefixInput: string,
): Promise<InstantCommand[]> {
  return tracedInvoke<InstantCommand[]>(IPC.GET_INSTANT_COMMANDS, { prefixInput });
}

export async function executeInstantCommand(
  name: string,
  query: string,
): Promise<LaunchResult> {
  return tracedInvoke<LaunchResult>(IPC.EXECUTE_INSTANT_COMMAND, { name, query });
}

export async function restartApp(): Promise<void> {
  return tracedInvoke(IPC.RESTART_APP);
}
