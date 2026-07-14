import { createSignal } from "solid-js";
import type { OpenerTool, SavedViewState } from "../lib/types";

export interface ToolSelectionFrame extends SavedViewState {
  kind: "tool";
  targetPath: string;
  targetIsFolder: boolean;
  tools: OpenerTool[];
  /** ツール起動 API へ渡す元クエリ。folder の restoreQuery（離脱時復元）とは別概念で、
   *  ModalFrame union + 用途別の名前により型で分離される（離脱時の復元には使わない・#538）。 */
  launchQuery: string;
  savedFolderFilter: string;
}

export const [toolSelectionState, setToolSelectionState] =
  createSignal<ToolSelectionFrame | null>(null);
