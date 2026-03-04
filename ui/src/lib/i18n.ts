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
  // SettingsWindow
  | "settings.tab.general"
  | "settings.tab.search"
  | "settings.tab.index"
  | "settings.tab.visual"
  | "settings.tab.opener"
  | "settings.loading"
  | "settings.unsaved_changes"
  | "settings.save"
  | "settings.no_changes"
  | "settings.discard"
  // SettingsGeneral
  | "settings.general.group.hotkey"
  | "settings.general.hotkey.label"
  | "settings.general.hotkey.none"
  | "settings.general.toggle.label"
  | "settings.general.toggle.description"
  | "settings.general.group.appearance"
  | "settings.general.max_results.label"
  | "settings.general.max_results.description"
  | "settings.general.window_width.label"
  | "settings.general.window_width.description"
  | "settings.general.show_icons.label"
  | "settings.general.show_icons.description"
  | "settings.general.group.behavior"
  | "settings.general.show_on_startup.label"
  | "settings.general.show_on_startup.description"
  | "settings.general.auto_hide.label"
  | "settings.general.auto_hide.description"
  | "settings.general.tray_icon.label"
  | "settings.general.tray_icon.description"
  | "settings.general.ime_off.label"
  | "settings.general.ime_off.description"
  // SettingsSearch
  | "settings.search.group.mode"
  | "settings.search.normal_mode.label"
  | "settings.search.normal_mode.description"
  | "settings.search.folder_mode.label"
  | "settings.search.folder_mode.description"
  | "settings.search.mode.prefix"
  | "settings.search.mode.substring"
  | "settings.search.mode.fuzzy"
  | "settings.search.group.visibility"
  | "settings.search.show_hidden.label"
  | "settings.search.show_hidden.description"
  | "settings.search.group.history"
  | "settings.search.history_size.label"
  | "settings.search.history_size.description"
  | "settings.search.group.history_score"
  | "settings.search.history_normalization.label"
  | "settings.search.history_normalization.description"
  | "settings.search.normalization.disabled"
  | "settings.search.normalization.fuzzy_relative_cap"
  | "settings.search.history_cap.label"
  | "settings.search.history_cap.description"
  // SettingsIndex
  | "settings.index.group.scan"
  | "settings.index.empty"
  | "settings.index.path.unset"
  | "settings.index.extensions.unset"
  | "settings.index.include_folders.badge"
  | "settings.index.edit"
  | "settings.index.modal.edit_title"
  | "settings.index.modal.add_title"
  | "settings.index.path.label"
  | "settings.index.path.hint"
  | "settings.index.extensions.label"
  | "settings.index.include_folders.label"
  | "settings.index.browse"
  | "settings.index.delete"
  | "settings.index.cancel"
  | "settings.index.save"
  | "settings.index.merged_duplicate"
  | "settings.index.merged_existing"
  // SettingsOpener
  | "settings.opener.group.rules"
  | "settings.opener.description"
  | "settings.opener.empty"
  | "settings.opener.edit"
  | "settings.opener.move_up"
  | "settings.opener.move_down"
  | "settings.opener.modal.edit_title"
  | "settings.opener.modal.add_title"
  | "settings.opener.target.label"
  | "settings.opener.target.folder"
  | "settings.opener.target.ext"
  | "settings.opener.tool_name.label"
  | "settings.opener.tool_name.unset"
  | "settings.opener.exe.label"
  | "settings.opener.browse"
  | "settings.opener.args.label"
  | "settings.opener.args.hint"
  | "settings.opener.delete"
  | "settings.opener.cancel"
  | "settings.opener.save"
  // SettingsVisual
  | "settings.visual.group.preview"
  | "settings.visual.group.theme"
  | "settings.visual.custom_theme.label"
  | "settings.visual.custom_theme.delete"
  | "settings.visual.custom_theme.save"
  | "settings.visual.group.colors"
  | "settings.visual.color.background"
  | "settings.visual.color.input_bg"
  | "settings.visual.color.text"
  | "settings.visual.color.selected_row"
  | "settings.visual.color.hint_text"
  | "settings.visual.group.font"
  | "settings.visual.font_family.label"
  | "settings.visual.font_loading"
  | "settings.visual.font_size.label"
  // Common
  | "common.add"
  // Slash commands
  | "cmd.history.description"
  | "cmd.about.description"
  | "cmd.settings.description"
  | "cmd.rebuild_index.description"
  | "cmd.quit.description"
  // stores/search
  | "notice.launch.timeout"
  | "notice.launch.failed"
  // stores/settings
  | "status.load_failed"
  | "status.saved"
  | "status.save_failed.hotkey"
  | "status.save_failed.generic"
  // AboutWindow
  | "about.email_subject";

const JA_JP: Record<TranslationKey, string> = {
  // SearchWindow
  "search.placeholder.default": "検索...",
  "search.placeholder.folder": "{dir} 内を検索...",
  "search.placeholder.tool_select": "ツールを選択...",
  "search.status.indexing": "インデックス構築中...",
  "search.status.launching": "起動中...",
  "search.status.no_results": "見つかりません",
  // SettingsWindow
  "settings.tab.general": "全般",
  "settings.tab.search": "検索",
  "settings.tab.index": "インデックス",
  "settings.tab.visual": "ビジュアル",
  "settings.tab.opener": "オープナー",
  "settings.loading": "設定を読み込み中...",
  "settings.unsaved_changes": "未保存の変更があります。",
  "settings.save": "保存",
  "settings.no_changes": "変更なし",
  "settings.discard": "保存せずに閉じる",
  // SettingsGeneral
  "settings.general.group.hotkey": "ホットキー",
  "settings.general.hotkey.label": "ホットキー",
  "settings.general.hotkey.none": "なし",
  "settings.general.toggle.label": "トグル動作",
  "settings.general.toggle.description": "ホットキーで表示中のウィンドウを非表示にします",
  "settings.general.group.appearance": "表示設定",
  "settings.general.max_results.label": "最大表示件数",
  "settings.general.max_results.description": "検索結果に表示する最大候補数",
  "settings.general.window_width.label": "ウィンドウ幅",
  "settings.general.window_width.description": "検索ウィンドウの横幅（論理ピクセル）",
  "settings.general.show_icons.label": "アイコン表示",
  "settings.general.show_icons.description": "検索結果にファイルアイコンを表示します",
  "settings.general.group.behavior": "動作",
  "settings.general.show_on_startup.label": "起動時に表示",
  "settings.general.show_on_startup.description": "アプリ起動時に検索ウィンドウを自動表示します",
  "settings.general.auto_hide.label": "フォーカス喪失時に非表示",
  "settings.general.auto_hide.description": "他のウィンドウをクリックした時に自動で隠します",
  "settings.general.tray_icon.label": "トレイアイコン",
  "settings.general.tray_icon.description": "システムトレイにアイコンを表示します",
  "settings.general.ime_off.label": "表示時にIMEオフ",
  "settings.general.ime_off.description": "ウィンドウ表示時にIMEを自動でオフにします",
  // SettingsSearch
  "settings.search.group.mode": "検索方式",
  "settings.search.normal_mode.label": "通常検索モード",
  "settings.search.normal_mode.description": "通常の検索時に使用するマッチング方式",
  "settings.search.folder_mode.label": "フォルダ内検索モード",
  "settings.search.folder_mode.description": "フォルダ内ファイルの検索時に使用するマッチング方式",
  "settings.search.mode.prefix": "前方一致",
  "settings.search.mode.substring": "部分一致",
  "settings.search.mode.fuzzy": "あいまい",
  "settings.search.group.visibility": "表示",
  "settings.search.show_hidden.label": "隠しファイルを表示",
  "settings.search.show_hidden.description": "Windowsの隠しファイルやシステムファイルを結果に含めます",
  "settings.search.group.history": "履歴",
  "settings.search.history_size.label": "履歴保持数",
  "settings.search.history_size.description": "保持する起動履歴の件数",
  "settings.search.group.history_score": "履歴スコア",
  "settings.search.history_normalization.label": "履歴正規化",
  "settings.search.history_normalization.description": "履歴の使用回数をスコアに反映する方式",
  "settings.search.normalization.disabled": "無効",
  "settings.search.normalization.fuzzy_relative_cap": "上限付き相対",
  "settings.search.history_cap.label": "履歴スコア上限比率",
  "settings.search.history_cap.description": "スコアに加算できる履歴ボーナスの上限（0〜1、正規化が有効の場合のみ）",
  // SettingsIndex
  "settings.index.group.scan": "スキャンパス",
  "settings.index.empty": "スキャンパスはまだ登録されていません",
  "settings.index.path.unset": "(未設定)",
  "settings.index.extensions.unset": "(拡張子未指定)",
  "settings.index.include_folders.badge": "フォルダを含む",
  "settings.index.edit": "編集",
  "settings.index.modal.edit_title": "スキャンパスを編集",
  "settings.index.modal.add_title": "スキャンパスを追加",
  "settings.index.path.label": "パス",
  "settings.index.path.hint": "同じパスは自動的に統合されます",
  "settings.index.extensions.label": "拡張子 (カンマ区切り)",
  "settings.index.include_folders.label": "フォルダを含める",
  "settings.index.browse": "参照...",
  "settings.index.delete": "削除",
  "settings.index.cancel": "キャンセル",
  "settings.index.save": "保存",
  "settings.index.merged_duplicate": "重複するパスを統合しました",
  "settings.index.merged_existing": "既存のパスに統合しました",
  // SettingsOpener
  "settings.opener.group.rules": "オープナールール",
  "settings.opener.description": "ファイル/フォルダを開く際に使用するツールを設定します。Enter で先頭ツールを起動、Shift+Enter でツール選択メニューを表示します。",
  "settings.opener.empty": "オープナールールはまだ登録されていません",
  "settings.opener.edit": "編集",
  "settings.opener.move_up": "上へ",
  "settings.opener.move_down": "下へ",
  "settings.opener.modal.edit_title": "オープナールールを編集",
  "settings.opener.modal.add_title": "オープナールールを追加",
  "settings.opener.target.label": "対象",
  "settings.opener.target.folder": "フォルダ",
  "settings.opener.target.ext": "拡張子",
  "settings.opener.tool_name.label": "名前",
  "settings.opener.tool_name.unset": "(名前未設定)",
  "settings.opener.exe.label": "実行ファイル",
  "settings.opener.browse": "参照...",
  "settings.opener.args.label": "引数 (オプション)",
  "settings.opener.args.hint": "{path} を使うとその位置にパスを展開します。未指定時は末尾に自動追加されます",
  "settings.opener.delete": "削除",
  "settings.opener.cancel": "キャンセル",
  "settings.opener.save": "保存",
  // SettingsVisual
  "settings.visual.group.preview": "プレビュー",
  "settings.visual.group.theme": "テーマ",
  "settings.visual.custom_theme.label": "マイテーマ",
  "settings.visual.custom_theme.delete": "マイテーマを削除",
  "settings.visual.custom_theme.save": "現在の配色を保存",
  "settings.visual.group.colors": "カラー",
  "settings.visual.color.background": "背景色",
  "settings.visual.color.input_bg": "入力欄背景色",
  "settings.visual.color.text": "テキスト色",
  "settings.visual.color.selected_row": "選択行色",
  "settings.visual.color.hint_text": "ヒントテキスト色",
  "settings.visual.group.font": "フォント",
  "settings.visual.font_family.label": "フォントファミリー",
  "settings.visual.font_loading": "（読み込み中...）",
  "settings.visual.font_size.label": "フォントサイズ",
  // Common
  "common.add": "追加",
  // Slash commands
  "cmd.history.description": "直近履歴を表示",
  "cmd.about.description": "バージョン情報",
  "cmd.settings.description": "設定を開く",
  "cmd.rebuild_index.description": "インデックス再構築",
  "cmd.quit.description": "アプリを終了",
  // stores/search
  "notice.launch.timeout": "起動に時間がかかっています{detail}",
  "notice.launch.failed": "起動に失敗しました{detail}",
  // stores/settings
  "status.load_failed": "設定の読み込みに失敗しました",
  "status.saved": "保存しました",
  "status.save_failed.hotkey": "保存に失敗: ホットキーの登録に失敗しました（他のアプリが同じキーを使用している可能性があります）",
  "status.save_failed.generic": "保存に失敗: {error}",
  // AboutWindow
  "about.email_subject": "Snotraについて",
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
