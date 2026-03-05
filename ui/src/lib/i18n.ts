/**
 * i18n モジュール。日本語をデフォルト埋め込みとし、t(key) でアクセスする。
 */
export type TranslationKey =
  // SearchWindow
  | "search.placeholder.default"
  | "search.placeholder.folder"
  | "search.placeholder.tool_select"
  | "search.status.indexing"
  | "search.status.launching"
  | "search.status.no_results"
  // Slash commands
  | "cmd.history.description"
  | "cmd.settings.description"
  | "cmd.rebuild_index.description"
  | "cmd.quit.description"
  // stores/search
  | "notice.launch.timeout"
  | "notice.launch.failed";

const JA_JP: Record<TranslationKey, string> = {
  // SearchWindow
  "search.placeholder.default": "検索...",
  "search.placeholder.folder": "{dir} 内を検索...",
  "search.placeholder.tool_select": "ツールを選択...",
  "search.status.indexing": "インデックス構築中...",
  "search.status.launching": "起動中...",
  "search.status.no_results": "見つかりません",
  // Slash commands
  "cmd.history.description": "直近履歴を表示",
  "cmd.settings.description": "設定を開く",
  "cmd.rebuild_index.description": "インデックス再構築",
  "cmd.quit.description": "アプリを終了",
  // stores/search
  "notice.launch.timeout": "起動に時間がかかっています{detail}",
  "notice.launch.failed": "起動に失敗しました{detail}",
};

/**
 * 翻訳文字列を返す。
 * params を渡すと "{key}" 形式のプレースホルダーを置換する。
 *   例: t("search.placeholder.folder", { dir: "C:\\Users" })
 *       → "C:\\Users 内を検索..."
 */
export function t(key: TranslationKey, params?: Record<string, string>): string {
  let str = JA_JP[key];
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      str = str.replace(`{${k}}`, v);
    }
  }
  return str;
}
