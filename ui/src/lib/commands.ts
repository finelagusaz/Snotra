import * as api from "./invoke";

export interface SlashCommand {
  command: string;
  label: string;
  description: string;
  action: () => void;
}

let hideAllWindowsFn: (() => void) | undefined;

export const SLASH_COMMANDS: SlashCommand[] = [
  {
    command: "/o",
    label: "/o",
    description: "設定を開く",
    action: () => {
      api.openSettings();
      hideAllWindowsFn?.();
    },
  },
  {
    command: "/s",
    label: "/s",
    description: "インデックス再構築",
    action: () => {
      api.rebuildIndex();
      hideAllWindowsFn?.();
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

export function initCommands(hideAllWindows: () => void) {
  hideAllWindowsFn = hideAllWindows;
}

export function findCommand(input: string): SlashCommand | undefined {
  const trimmed = input.trim();
  return SLASH_COMMANDS.find((c) => c.command === trimmed);
}

export function isCommandPrefix(input: string): boolean {
  return input.trim() === "/";
}
