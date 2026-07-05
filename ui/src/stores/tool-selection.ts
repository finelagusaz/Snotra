import { createSignal } from "solid-js";
import type { OpenerTool, SavedViewState } from "../lib/types";

export interface ToolSelectionFrame extends SavedViewState {
  targetPath: string;
  targetIsFolder: boolean;
  tools: OpenerTool[];
  /** ツール起動時に渡す元クエリ（folder の savedQuery と異なり離脱時の復元には使わない） */
  savedQuery: string;
  savedFolderFilter: string;
}

export const [toolSelectionState, setToolSelectionState] =
  createSignal<ToolSelectionFrame | null>(null);
