// 入力解釈の純関数（SolidJS/api 非依存・テスト可能なため stores/ から分離。folderNav.ts / windowHeight.ts と同じパターン）。
// 「入力 → 意図（plain/command/instant + instant の filterName/instantQuery）」を副作用なしで返す。
// stores/search.ts の interpKind memo と dispatchQueryInput の分岐が単一情報源としてこれを消費する。

/** 軸1: 結果リストを占める先頭ビュー（tool > folder > results の射影＝SPEC §18.5 優先度）。 */
export type ViewKind = "results" | "folder" | "tool";

/** 軸2: 入力の意味。viewKind=results のときだけ非 plain。 */
export type InterpKind = "plain" | "command" | "instant";

/** interpret の戻り値。instant のみ parse 済みの filterName（コマンド名）/ instantQuery（スペース以降）を持つ。 */
export type QueryIntent =
  | { kind: "plain" }
  | { kind: "command" }
  | { kind: "instant"; filterName: string; instantQuery: string };

/** instant モード検出述語（instant 判定の SSOT）。空 prefix では false（全入力が instant 化するのを防ぐ）。
 *  trim 規則（trimStart）もここに集約する。 */
export function isInstantPrefix(rawQuery: string, prefix: string): boolean {
  return prefix !== "" && rawQuery.trimStart().startsWith(prefix);
}

/** 入力 → 意図。副作用なし。viewKind!=="results" は常に plain（folder/tool 中は非 plain 化しない）。
 *  instant の parse（prefix 除去・スペース分割）を一箇所に集約し、filterName/instantQuery の
 *  抽出を消費側（表示・実行）で重複させない（DRY）。 */
export function interpret(rawQuery: string, prefix: string, viewKind: ViewKind): QueryIntent {
  if (viewKind !== "results") return { kind: "plain" };
  if (isInstantPrefix(rawQuery, prefix)) {
    // trimStart() を使用: trailing whitespace はクエリの一部として保持する（SPEC §18.5）。
    const input = rawQuery.trimStart().slice(prefix.length);
    const spaceIdx = input.indexOf(" ");
    const filterName = spaceIdx >= 0 ? input.slice(0, spaceIdx) : input;
    const instantQuery = spaceIdx >= 0 ? input.slice(spaceIdx + 1) : "";
    return { kind: "instant", filterName, instantQuery };
  }
  if (rawQuery.trimStart().startsWith("/")) return { kind: "command" };
  return { kind: "plain" };
}
