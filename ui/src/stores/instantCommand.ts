import type { InstantCommand, SearchResult } from "../lib/types";
import type { LatestRun } from "../lib/latestRun";
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
 *  world 世代（staleness/supersede）は search.ts 側の関心事のため、検索/データ lane と同じ
 *  `latestRun` runner の `run` を呼び出し側から注入して調停する（本モジュールは api/types にのみ
 *  依存し、search.ts への逆依存を持たない——循環 import を避ける設計）。`run` は内部で世代を進め、
 *  task に `isStale()` を渡す。await 後 stale なら適用をスキップする。
 *  IPC 応答前の Enter/クリックで古いコマンドを誤起動しないよう、呼び出し時点で即座に
 *  候補一覧をクリアする。 */
export function scheduleInstantCommandFetch(
  filterName: string,
  deps: {
    /** 検索/データ lane と共有する `latestRun` の run（型契約は latestRun.ts が SSOT）。
     *  最新実行だけが結果を適用する（task は ctx.isStale のみ参照する）。 */
    run: LatestRun["run"];
    onFetched: (results: SearchResult[]) => void;
    onError: (error: unknown) => void;
  },
): void {
  instantCommandItems = [];
  cancelInstantCommandDebounce();
  instantCmdDebounceTimer = setTimeout(() => {
    instantCmdDebounceTimer = undefined;
    void deps.run(async ({ isStale }) => {
      try {
        const commands = await api.getInstantCommands(filterName);
        if (isStale()) return;
        instantCommandItems = commands;
        const results: SearchResult[] = commands.map((cmd) => ({
          name: cmd.name,
          path: cmd.name,
          isFolder: false,
          isError: false,
          description: cmd.description || cmd.display,
        }));
        deps.onFetched(results);
      } catch (e) {
        deps.onError(e);
      }
    });
  }, INSTANT_CMD_DEBOUNCE_MS);
}
