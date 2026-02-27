import { createSignal } from "solid-js";
import type { SearchResult } from "../lib/types";

export interface FolderFrame {
  currentDir: string;
  savedResults: SearchResult[];
  savedSelected: number;
  savedQuery: string;
}

export const [folderState, setFolderState] = createSignal<FolderFrame | null>(null);
export const [folderFilter, setFolderFilter] = createSignal("");

export function isInFolderMode(): boolean {
  return folderState() !== null;
}
