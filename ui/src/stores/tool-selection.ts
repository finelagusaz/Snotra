import { createSignal } from "solid-js";
import type { OpenerTool, SearchResult } from "../lib/types";

export interface ToolSelectionFrame {
  targetPath: string;
  targetIsFolder: boolean;
  tools: OpenerTool[];
  savedResults: SearchResult[];
  savedSelected: number;
  savedQuery: string;
  savedFolderFilter: string;
}

export const [toolSelectionState, setToolSelectionState] =
  createSignal<ToolSelectionFrame | null>(null);
