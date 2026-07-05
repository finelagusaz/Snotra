import type { InstantCommand, SearchResult } from "../lib/types";
import * as api from "../lib/invoke";

/** インスタントコマンド候補の IPC 取得デバウンス（高速タイピング時の不要な IPC を削減）。 */
const INSTANT_CMD_DEBOUNCE_MS = 30;

/** インスタントコマンドモード中のコマンド一覧（activateSelected 系が起動対象を解決するときに
 *  参照する）。書き込みは本モジュールの関数（setInstantCommandItems/clearInstantCommandItems/
 *  scheduleInstantCommandFetch 内部）に閉じ、search.ts からは直接代入しない。 */
let instantCommandItems: InstantCommand[] = [];
let instantCmdDebounceTimer: ReturnType<typeof setTimeout> | undefined;

export function getInstantCommandItems(): InstantCommand[] {
  return instantCommandItems;
}

/** 失敗時のロールバック（executeInstantCommandSelected）用に候補一覧を差し替える。 */
export function setInstantCommandItems(items: InstantCommand[]) {
  instantCommandItems = items;
}

export function clearInstantCommandItems() {
  instantCommandItems = [];
}

export function hasPendingInstantCommandFetch(): boolean {
  return instantCmdDebounceTimer !== undefined;
}

/** 保留中の IPC デバウンスタイマーを破棄する（モード離脱・resetForShow 等で呼ぶ）。 */
export function cancelInstantCommandDebounce() {
  if (instantCmdDebounceTimer !== undefined) {
    clearTimeout(instantCmdDebounceTimer);
    instantCmdDebounceTimer = undefined;
  }
}

/** インスタントコマンド候補の取得を 30ms デバウンスでスケジュールする唯一の経路（choke point）。
 *  world 世代（searchGeneration）は search.ts 側の関心事のため、staleness 判定・世代更新は
 *  呼び出し側が hooks 経由で担う（本モジュールは api/types にのみ依存し、search.ts への
 *  逆依存を持たない——循環 import を避ける設計）。
 *  IPC 応答前の Enter/クリックで古いコマンドを誤起動しないよう、呼び出し時点で即座に
 *  候補一覧をクリアする。 */
export function scheduleInstantCommandFetch(
  filterName: string,
  hooks: {
    /** 世代を進めて今回のリクエスト ID を発行する（search.ts の nextGeneration）。 */
    nextRequestId: () => number;
    /** 応答到着時に world 世代が変わっていれば true（stale として破棄）。 */
    isStale: (requestId: number) => boolean;
    onFetched: (results: SearchResult[]) => void;
    onError: (error: unknown) => void;
  },
): void {
  instantCommandItems = [];
  cancelInstantCommandDebounce();
  instantCmdDebounceTimer = setTimeout(() => {
    instantCmdDebounceTimer = undefined;
    void (async () => {
      const requestId = hooks.nextRequestId();
      try {
        const commands = await api.getInstantCommands(filterName);
        if (hooks.isStale(requestId)) return;
        instantCommandItems = commands;
        const results: SearchResult[] = commands.map((cmd) => ({
          name: cmd.name,
          path: cmd.name,
          isFolder: false,
          isError: false,
          description: cmd.description || cmd.display,
        }));
        hooks.onFetched(results);
      } catch (e) {
        hooks.onError(e);
      }
    })();
  }, INSTANT_CMD_DEBOUNCE_MS);
}
