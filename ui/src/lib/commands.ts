import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./invoke";
import { t } from "./i18n";

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
  api.notifyMainHidden().catch(() => {});
  await getCurrentWindow().hide();
  await hideResultsWindow();
}

export const SLASH_COMMANDS: SlashCommand[] = [
  {
    command: "/r",
    label: "/r",
    description: t("cmd.history.description"),
    action: () => {
      // /r is handled by search store as a result-producing command.
    },
  },
  {
    command: "/o",
    label: "/o",
    description: t("cmd.settings.description"),
    action: async () => {
      // results のみ非表示（検索ウィンドウは残す）
      await hideResultsWindow();
      await api.openSettings();
    },
  },
  {
    command: "/s",
    label: "/s",
    description: t("cmd.rebuild_index.description"),
    action: async () => {
      await hideAllWindows();
      await api.rebuildIndex();
    },
  },
  {
    command: "/q",
    label: "/q",
    description: t("cmd.quit.description"),
    action: () => {
      api.quitApp();
    },
  },
];

export function findCommand(input: string): SlashCommand | undefined {
  const trimmed = input.trim();
  return SLASH_COMMANDS.find((c) => c.command === trimmed);
}

