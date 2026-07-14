import { createSignal } from "solid-js";
import type { SavedViewState } from "../lib/types";

export interface FolderFrame extends SavedViewState {
  kind: "folder";
  currentDir: string;
  /** フォルダモード離脱時に setQuery() で復元する検索欄の値。tool の launchQuery（launch 引数）とは
   *  別概念で、ModalFrame union + 用途別の名前により型で分離される（#538）。 */
  restoreQuery: string;
}

export const [folderState, setFolderState] = createSignal<FolderFrame | null>(null);
export const [folderFilter, setFolderFilter] = createSignal("");
