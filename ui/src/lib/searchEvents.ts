import type { SearchResult } from "./types";

export type ResultsPresentationReason =
  | "query"
  | "reset"
  | "launch"
  | "command"
  | "selection";

export interface ResultsSyncPayload {
  generation: number;
  results: SearchResult[];
  selected: number;
  shouldShow: boolean;
  reason: ResultsPresentationReason;
}

export interface ResultsRenderDonePayload {
  requestId: number;
}
