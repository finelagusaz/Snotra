import { createSignal } from "solid-js";
import type { SavedViewState } from "../lib/types";

export interface FolderFrame extends SavedViewState {
  currentDir: string;
  /** フォルダモード離脱時に setQuery() で復元する検索欄の値（tool の savedQuery とは用途が異なる） */
  savedQuery: string;
}

export const [folderState, setFolderState] = createSignal<FolderFrame | null>(null);
export const [folderFilter, setFolderFilter] = createSignal("");
