import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./invoke";
import { t } from "./i18n";
import { setLaunchNoticeWithAutoClear } from "../stores/search";

/** インデックス構築中エラーコード。Rust 側 src-tauri/src/commands/window.rs の
 *  ERR_INDEXING_IN_PROGRESS と対になる。変更するときは両ファイルを同時に更新する。 */
const ERR_INDEXING_IN_PROGRESS = "indexing_in_progress";

export interface SlashCommand {
  command: string;
  label: string;
  readonly description: string;
  action: () => void | Promise<void>;
}

/** main webview コンテキストから呼び出すこと（getCurrentWindow() が main を返す前提）。
 *  全 frontend hide（Escape / Enter / Shift+Enter / クリック起動 / フォーカス喪失 / /s）の
 *  単一チョークポイント。notifyMainHidden() の trim（EmptyWorkingSet）は win.hide() 完了後に
 *  走らせる——可視中に trim するとレンダラがページを再 touch し working set 回収が削がれるため
 *  （hotkey 経路と同じ hide→trim 順に揃える。#361）。 */
export async function hideMainWindow() {
  await getCurrentWindow().hide();
  api.notifyMainHidden().catch(() => {});
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
      try {
        await api.openSettings();
      } catch (e) {
        if (String(e).includes(ERR_INDEXING_IN_PROGRESS)) {
          setLaunchNoticeWithAutoClear(
            t("notice.settings.unavailable_while_indexing"),
            3000,
          );
        } else {
          throw e;
        }
      }
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
