import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./invoke";
import { t } from "./i18n";

export interface SlashCommand {
  command: string;
  label: string;
  readonly description: string;
  action: () => void | Promise<void>;
}

/** main webview コンテキストから呼び出すこと（getCurrentWindow() が main を返す前提） */
export async function hideMainWindow() {
  api.notifyMainHidden().catch(() => {});
  await getCurrentWindow().hide();
}

export const SLASH_COMMANDS: SlashCommand[] = [
  {
    command: "/r",
    label: "/r",
    get description() { return t("cmd.history.description"); },
    action: () => {
      // /r is handled by search store as a result-producing command.
    },
  },
  {
    command: "/o",
    label: "/o",
    get description() { return t("cmd.settings.description"); },
    action: async () => {
      await api.openSettings();
    },
  },
  {
    command: "/s",
    label: "/s",
    get description() { return t("cmd.rebuild_index.description"); },
    action: async () => {
      await hideMainWindow();
      await api.rebuildIndex();
    },
  },
  {
    command: "/q",
    label: "/q",
    get description() { return t("cmd.quit.description"); },
    action: () => {
      api.quitApp();
    },
  },
];

export function findCommand(input: string): SlashCommand | undefined {
  const trimmed = input.trim();
  return SLASH_COMMANDS.find((c) => c.command === trimmed);
}
