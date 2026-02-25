import * as api from "./invoke";

export interface SlashCommand {
  command: string;
  label: string;
  description: string;
  action: () => void | Promise<void>;
}

let hideAllWindowsFn: (() => void | Promise<void>) | undefined;

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
      await hideAllWindowsFn?.();
      await api.openAbout();
    },
  },
  {
    command: "/o",
    label: "/o",
    description: "設定を開く",
    action: async () => {
      await hideAllWindowsFn?.();
      await api.openSettings();
    },
  },
  {
    command: "/s",
    label: "/s",
    description: "インデックス再構築",
    action: async () => {
      await hideAllWindowsFn?.();
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

export function initCommands(hideAllWindows: () => void | Promise<void>) {
  hideAllWindowsFn = hideAllWindows;
}

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
