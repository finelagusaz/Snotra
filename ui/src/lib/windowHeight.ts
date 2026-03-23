/** ウィンドウの論理高さを計算する。
 *  shouldShowResults が true なら検索バー + 結果行 + パディング、false なら検索バーのみ。
 *  updateToast が表示中ならその分も加算する。 */
export function computeWindowHeight(params: {
  shouldShowResults: boolean;
  maxResults: number;
  hasUpdateToast: boolean;
  searchBarHeight: number;
  resultRowHeight: number;
  resultsPadding: number;
  updateToastHeight: number;
}): number {
  const { shouldShowResults, maxResults, hasUpdateToast, searchBarHeight, resultRowHeight, resultsPadding, updateToastHeight } = params;
  const contentHeight = shouldShowResults
    ? searchBarHeight + maxResults * resultRowHeight + resultsPadding
    : searchBarHeight;
  return contentHeight + (hasUpdateToast ? updateToastHeight : 0);
}
