/**
 * i18n モジュール。日本語をデフォルト埋め込みとし、t(key) でアクセスする。
 * setLanguage() で言語を切り替えると、以降の t() 呼び出しが切り替え後の言語を返す。
 */
import { createSignal } from "solid-js";

export type Lang = "ja" | "en";

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
  | "notice.launch.failed"
  // hotkey registration failure
  | "notice.hotkey.initial_failed"
  | "notice.hotkey.change_failed"
  // settings availability
  | "notice.settings.unavailable_while_indexing";

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
  // hotkey registration failure
  "notice.hotkey.initial_failed":
    "ホットキー ({hotkey}) の登録に失敗しました。他のアプリが使用中の可能性があります",
  "notice.hotkey.change_failed":
    "ホットキー ({hotkey}) の登録に失敗しました。元のホットキーを維持します",
  // settings availability
  "notice.settings.unavailable_while_indexing":
    "インデックス構築中のため、設定を開けません",
};

const EN_US: Record<TranslationKey, string> = {
  // SearchWindow
  "search.placeholder.default": "Search...",
  "search.placeholder.folder": "Search in {dir}...",
  "search.placeholder.tool_select": "Select a tool...",
  "search.status.indexing": "Building index...",
  "search.status.launching": "Launching...",
  "search.status.no_results": "No results found",
  // Slash commands
  "cmd.history.description": "Show recent history",
  "cmd.settings.description": "Open settings",
  "cmd.rebuild_index.description": "Rebuild index",
  "cmd.quit.description": "Quit application",
  // stores/search
  "notice.launch.timeout": "Launch is taking a while{detail}",
  "notice.launch.failed": "Launch failed{detail}",
  // hotkey registration failure
  "notice.hotkey.initial_failed":
    "Failed to register hotkey ({hotkey}). Another application may be using it",
  "notice.hotkey.change_failed":
    "Failed to register hotkey ({hotkey}). Keeping the previous hotkey",
  // settings availability
  "notice.settings.unavailable_while_indexing":
    "Cannot open settings while indexing.",
};

const TABLES: Record<Lang, Record<TranslationKey, string>> = {
  ja: JA_JP,
  en: EN_US,
};

/** OS の言語設定から初期言語を同期的に決定する（Rust 側の default_language() と同じロジック）。
 *  bootstrap 完了前の初回描画でも正しい言語で表示するため、ここで確定する。 */
function detectInitialLang(): Lang {
  const locale = navigator.language ?? "";
  return locale.startsWith("ja") ? "ja" : "en";
}

const [currentLang, setCurrentLang] = createSignal<Lang>(detectInitialLang());

export { setCurrentLang as setLanguage };

export function getLanguage(): Lang {
  return currentLang();
}

/**
 * 翻訳文字列を返す。
 * params を渡すと "{key}" 形式のプレースホルダーを置換する。
 *   例: t("search.placeholder.folder", { dir: "C:\\Users" })
 *       → "C:\\Users 内を検索..."
 */
export function t(key: TranslationKey, params?: Record<string, string>): string {
  let str = TABLES[currentLang()][key];
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      str = str.replace(`{${k}}`, v);
    }
  }
  return str;
}
