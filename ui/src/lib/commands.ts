import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./invoke";

export interface SlashCommand {
  command: string;
  label: string;
  description: string;
  action: () => void | Promise<void>;
}

async function hideResultsWindow() {
  const rw = await WebviewWindow.getByLabel("results");
  if (rw) await rw.hide();
}

/** main webview コンテキストから呼び出すこと（getCurrentWindow() が main を返す前提） */
export async function hideAllWindows() {
  await getCurrentWindow().hide();
  await hideResultsWindow();
}

export const SLASH_COMMANDS: SlashCommand[] = [
  {
    command: "/r",
    label: "/r",
    description: "直近履歴を表示",
    action: () => {
      // /r is handled by search store as a result-producing command.
    },
  },
  {
    command: "/a",
    label: "/a",
    description: "バージョン情報",
    action: async () => {
      // results のみ非表示（検索ウィンドウは残す）
      await hideResultsWindow();
      await api.openAbout();
    },
  },
  {
    command: "/o",
    label: "/o",
    description: "設定を開く",
    action: async () => {
      // results のみ非表示（検索ウィンドウは残す）
      await hideResultsWindow();
      await api.openSettings();
    },
  },
  {
    command: "/s",
    label: "/s",
    description: "インデックス再構築",
    action: async () => {
      await hideAllWindows();
      await api.rebuildIndex();
    },
  },
  {
    command: "/q",
    label: "/q",
    description: "アプリを終了",
    action: () => {
      api.quitApp();
    },
  },
];

export function findCommand(input: string): SlashCommand | undefined {
  const trimmed = input.trim();
  return SLASH_COMMANDS.find((c) => c.command === trimmed);
}

export function filterCommands(input: string): SlashCommand[] {
  const trimmed = input.trim();
  if (!trimmed.startsWith("/")) return [];
  if (trimmed === "/") return SLASH_COMMANDS;
  return SLASH_COMMANDS.filter((c) => c.command.startsWith(trimmed));
}
