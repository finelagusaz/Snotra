import type { SearchResult } from "./types";

export type ResultsPresentationReason =
  | "query"
  | "reset"
  | "launch"
  | "command";

/** results-data-changed: 結果配列が変わったとき */
export interface ResultsDataPayload {
  generation: number;
  results: SearchResult[];
  selected: number;
  shouldShow: boolean;
  reason: ResultsPresentationReason;
}

/** results-selection-changed: 選択インデックスのみ変わったとき（配列不要） */
export interface ResultsSelectionPayload {
  generation: number;
  selected: number;
}

/** results-visibility-changed: 非表示にするとき（配列不要） */
export interface ResultsVisibilityPayload {
  generation: number;
  shouldShow: false;
  reason: ResultsPresentationReason;
}

export interface ResultsRenderDonePayload {
  requestId: number;
}
