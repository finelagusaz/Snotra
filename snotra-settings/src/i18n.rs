use snotra_core::config::Language;

pub struct Tr(pub Language);

impl Tr {
    // Window title
    pub fn window_title(&self) -> &'static str {
        match self.0 {
            Language::Ja => "Snotra 設定",
            Language::En => "Snotra Settings",
        }
    }

    // Tab names
    pub fn tab_general(&self) -> &'static str {
        match self.0 {
            Language::Ja => "全般",
            Language::En => "General",
        }
    }

    pub fn tab_search(&self) -> &'static str {
        match self.0 {
            Language::Ja => "検索",
            Language::En => "Search",
        }
    }

    pub fn tab_index(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インデックス",
            Language::En => "Index",
        }
    }

    pub fn tab_visual(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ビジュアル",
            Language::En => "Visual",
        }
    }

    pub fn tab_opener(&self) -> &'static str {
        match self.0 {
            Language::Ja => "オープナー",
            Language::En => "Opener",
        }
    }

    pub fn tab_instant_command(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インスタント",
            Language::En => "Instant",
        }
    }

    // Footer buttons
    pub fn btn_save(&self) -> &'static str {
        match self.0 {
            Language::Ja => "保存",
            Language::En => "Save",
        }
    }

    pub fn btn_discard(&self) -> &'static str {
        match self.0 {
            Language::Ja => "破棄",
            Language::En => "Discard",
        }
    }

    pub fn btn_reset_default(&self) -> &'static str {
        match self.0 {
            Language::Ja => "初期設定に戻す",
            Language::En => "Reset to default",
        }
    }

    // Status messages
    pub fn status_saved(&self) -> &'static str {
        match self.0 {
            Language::Ja => "保存しました",
            Language::En => "Saved",
        }
    }

    pub fn status_unsaved(&self) -> &'static str {
        match self.0 {
            Language::Ja => "未保存の変更があります",
            Language::En => "Unsaved changes",
        }
    }

    pub fn status_validation_error(&self) -> &'static str {
        match self.0 {
            Language::Ja => "検証エラー: ",
            Language::En => "Validation error: ",
        }
    }

    pub fn status_save_failed(&self) -> &'static str {
        match self.0 {
            Language::Ja => "保存失敗: ",
            Language::En => "Save failed: ",
        }
    }

    // 読み込み失敗（finding C: 一時的 read 失敗で既定値表示中）の保存ガード
    pub fn read_failed_banner(&self) -> &'static str {
        match self.0 {
            Language::Ja => "⚠ 設定ファイルを読み込めませんでした。既定値で表示しています。このまま保存すると既存の設定が失われる可能性があります。",
            Language::En => "⚠ Could not read the settings file. Showing defaults. Saving now may overwrite your existing settings.",
        }
    }

    pub fn read_failed_confirm(&self) -> &'static str {
        match self.0 {
            Language::Ja => "既存設定が失われる可能性を承知の上で保存する",
            Language::En => "Save anyway (I understand existing settings may be lost)",
        }
    }

    pub fn status_read_failed_blocked(&self) -> &'static str {
        match self.0 {
            Language::Ja => "読み込み失敗中: 上のチェックを入れると保存できます",
            Language::En => "Load failed: check the box above to enable saving",
        }
    }

    // Config error messages
    pub fn err_hotkey_modifier_empty(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ホットキーの修飾キーが未設定です",
            Language::En => "Hotkey modifier is not set",
        }
    }

    pub fn err_hotkey_key_empty(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ホットキーのキーが未設定です",
            Language::En => "Hotkey key is not set",
        }
    }

    pub fn err_hotkey_system_conflict(&self) -> &'static str {
        match self.0 {
            Language::Ja => " はシステムショートカットと競合します",
            Language::En => " conflicts with a system shortcut",
        }
    }

    pub fn err_max_results_zero(&self) -> &'static str {
        match self.0 {
            Language::Ja => "最大表示件数は1以上にしてください",
            Language::En => "Max results must be at least 1",
        }
    }

    pub fn err_window_width_too_small(&self) -> &'static str {
        match self.0 {
            Language::Ja => " は小さすぎます（200以上）",
            Language::En => " is too small (min 200)",
        }
    }

    pub fn err_fuzzy_cap_ratio_out_of_range(&self) -> &'static str {
        match self.0 {
            Language::Ja => " は 0.0〜1.0 の範囲にしてください",
            Language::En => " must be between 0.0 and 1.0",
        }
    }

    pub fn err_scan_path_empty(&self) -> &'static str {
        match self.0 {
            Language::Ja => "のパスが空です",
            Language::En => " path is empty",
        }
    }

    pub fn err_instant_prefix_empty(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インスタントコマンドのプレフィックスが空です",
            Language::En => "Instant command prefix is empty",
        }
    }

    pub fn err_instant_prefix_slash(&self) -> &'static str {
        match self.0 {
            Language::Ja => "プレフィックスに / は使用できません（スラッシュコマンドと競合）",
            Language::En => "Prefix cannot be / (conflicts with slash commands)",
        }
    }

    pub fn err_instant_duplicate_name(&self) -> &'static str {
        match self.0 {
            Language::Ja => " はコマンド名が重複しています",
            Language::En => " has a duplicate command name",
        }
    }

    // General tab
    pub fn heading_hotkey(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ホットキー",
            Language::En => "Hotkey",
        }
    }

    pub fn label_hotkey(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ホットキー:",
            Language::En => "Hotkey:",
        }
    }

    pub fn cb_hotkey_toggle(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ホットキーで表示中のウィンドウを非表示にする",
            Language::En => "Toggle window visibility with hotkey",
        }
    }

    pub fn heading_appearance(&self) -> &'static str {
        match self.0 {
            Language::Ja => "外観",
            Language::En => "Appearance",
        }
    }

    pub fn label_max_results(&self) -> &'static str {
        match self.0 {
            Language::Ja => "最大表示件数:",
            Language::En => "Max results:",
        }
    }

    pub fn label_window_width(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ウィンドウ幅:",
            Language::En => "Window width:",
        }
    }

    pub fn cb_show_icons(&self) -> &'static str {
        match self.0 {
            Language::Ja => "アイコンを表示",
            Language::En => "Show icons",
        }
    }

    pub fn heading_behavior(&self) -> &'static str {
        match self.0 {
            Language::Ja => "動作",
            Language::En => "Behavior",
        }
    }

    pub fn cb_show_on_startup(&self) -> &'static str {
        match self.0 {
            Language::Ja => "起動時にウィンドウを表示",
            Language::En => "Show window on startup",
        }
    }

    pub fn cb_auto_hide(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォーカス喪失時に非表示",
            Language::En => "Hide on focus lost",
        }
    }

    pub fn cb_tray_icon(&self) -> &'static str {
        match self.0 {
            Language::Ja => "トレイアイコンを表示",
            Language::En => "Show tray icon",
        }
    }

    pub fn cb_ime_off(&self) -> &'static str {
        match self.0 {
            Language::Ja => "表示時に IME をオフにする",
            Language::En => "Turn off IME on show",
        }
    }

    pub fn cb_follow_cursor_monitor(&self) -> &'static str {
        match self.0 {
            Language::Ja => "カーソルのあるモニターに表示",
            Language::En => "Show on monitor with cursor",
        }
    }

    pub fn heading_language(&self) -> &'static str {
        match self.0 {
            Language::Ja => "言語",
            Language::En => "Language",
        }
    }

    pub fn label_language(&self) -> &'static str {
        match self.0 {
            Language::Ja => "言語:",
            Language::En => "Language:",
        }
    }

    pub fn language_ja(&self) -> &'static str {
        "日本語"
    }

    pub fn language_en(&self) -> &'static str {
        "English"
    }

    // Search tab
    pub fn heading_search_mode(&self) -> &'static str {
        match self.0 {
            Language::Ja => "検索モード",
            Language::En => "Search mode",
        }
    }

    pub fn label_normal_mode(&self) -> &'static str {
        match self.0 {
            Language::Ja => "通常モード:",
            Language::En => "Normal mode:",
        }
    }

    pub fn label_folder_mode(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォルダモード:",
            Language::En => "Folder mode:",
        }
    }

    pub fn search_mode_prefix(&self) -> &'static str {
        match self.0 {
            Language::Ja => "前方一致",
            Language::En => "Prefix",
        }
    }

    pub fn search_mode_substring(&self) -> &'static str {
        match self.0 {
            Language::Ja => "部分一致",
            Language::En => "Substring",
        }
    }

    pub fn search_mode_fuzzy(&self) -> &'static str {
        match self.0 {
            Language::Ja => "あいまい",
            Language::En => "Fuzzy",
        }
    }

    pub fn heading_visibility(&self) -> &'static str {
        match self.0 {
            Language::Ja => "表示",
            Language::En => "Visibility",
        }
    }

    pub fn cb_show_hidden_system(&self) -> &'static str {
        match self.0 {
            Language::Ja => "隠しファイル・システムファイルを表示",
            Language::En => "Show hidden/system files",
        }
    }

    pub fn heading_history(&self) -> &'static str {
        match self.0 {
            Language::Ja => "履歴",
            Language::En => "History",
        }
    }

    pub fn label_max_history(&self) -> &'static str {
        match self.0 {
            Language::Ja => "最大列挙数:",
            Language::En => "Max entries:",
        }
    }

    pub fn label_max_history_display(&self) -> &'static str {
        match self.0 {
            Language::Ja => "最大履歴表示件数:",
            Language::En => "Max history display:",
        }
    }

    pub fn heading_history_score(&self) -> &'static str {
        match self.0 {
            Language::Ja => "履歴スコア",
            Language::En => "History score",
        }
    }

    pub fn label_normalization(&self) -> &'static str {
        match self.0 {
            Language::Ja => "正規化:",
            Language::En => "Normalization:",
        }
    }

    pub fn normalization_disabled(&self) -> &'static str {
        match self.0 {
            Language::Ja => "無効",
            Language::En => "Disabled",
        }
    }

    pub fn normalization_fuzzy_relative_cap(&self) -> &'static str {
        match self.0 {
            Language::Ja => "Fuzzy 相対キャップ",
            Language::En => "Fuzzy relative cap",
        }
    }

    pub fn label_fuzzy_cap_ratio(&self) -> &'static str {
        match self.0 {
            Language::Ja => "Fuzzy 履歴キャップ比率:",
            Language::En => "Fuzzy history cap ratio:",
        }
    }

    // Index tab
    pub fn heading_scan_targets(&self) -> &'static str {
        match self.0 {
            Language::Ja => "スキャン対象",
            Language::En => "Scan targets",
        }
    }

    pub fn label_no_scan_paths(&self) -> &'static str {
        match self.0 {
            Language::Ja => "スキャンパスが設定されていません。",
            Language::En => "No scan paths configured.",
        }
    }

    pub fn label_incl_folders(&self) -> &'static str {
        match self.0 {
            Language::Ja => "(フォルダ含む)",
            Language::En => "(incl. folders)",
        }
    }

    pub fn btn_edit(&self) -> &'static str {
        match self.0 {
            Language::Ja => "編集",
            Language::En => "Edit",
        }
    }

    pub fn btn_add(&self) -> &'static str {
        match self.0 {
            Language::Ja => "追加…",
            Language::En => "Add…",
        }
    }

    pub fn modal_edit_scan_path(&self) -> &'static str {
        match self.0 {
            Language::Ja => "スキャンパスを編集",
            Language::En => "Edit scan path",
        }
    }

    pub fn modal_add_scan_path(&self) -> &'static str {
        match self.0 {
            Language::Ja => "スキャンパスを追加",
            Language::En => "Add scan path",
        }
    }

    pub fn label_path(&self) -> &'static str {
        match self.0 {
            Language::Ja => "パス:",
            Language::En => "Path:",
        }
    }

    pub fn btn_browse(&self) -> &'static str {
        match self.0 {
            Language::Ja => "参照…",
            Language::En => "Browse…",
        }
    }

    pub fn label_extensions(&self) -> &'static str {
        match self.0 {
            Language::Ja => "拡張子 (カンマ区切り):",
            Language::En => "Extensions (comma-separated):",
        }
    }

    pub fn cb_include_folders(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォルダを含む",
            Language::En => "Include folders",
        }
    }

    pub fn btn_delete(&self) -> &'static str {
        match self.0 {
            Language::Ja => "削除",
            Language::En => "Delete",
        }
    }

    pub fn btn_cancel(&self) -> &'static str {
        match self.0 {
            Language::Ja => "キャンセル",
            Language::En => "Cancel",
        }
    }

    pub fn dialog_select_folder(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォルダを選択",
            Language::En => "Select folder",
        }
    }

    // Visual tab
    pub fn heading_theme(&self) -> &'static str {
        match self.0 {
            Language::Ja => "テーマ",
            Language::En => "Theme",
        }
    }

    pub fn btn_save_custom_theme(&self) -> &'static str {
        match self.0 {
            Language::Ja => "カスタムテーマとして保存",
            Language::En => "Save as custom theme",
        }
    }

    pub fn heading_color(&self) -> &'static str {
        match self.0 {
            Language::Ja => "カラー",
            Language::En => "Colors",
        }
    }

    pub fn label_bg_color(&self) -> &'static str {
        match self.0 {
            Language::Ja => "背景色:",
            Language::En => "Background:",
        }
    }

    pub fn label_input_bg(&self) -> &'static str {
        match self.0 {
            Language::Ja => "入力欄背景:",
            Language::En => "Input background:",
        }
    }

    pub fn label_text_color(&self) -> &'static str {
        match self.0 {
            Language::Ja => "テキスト:",
            Language::En => "Text:",
        }
    }

    pub fn label_selected_row(&self) -> &'static str {
        match self.0 {
            Language::Ja => "選択行:",
            Language::En => "Selected row:",
        }
    }

    pub fn label_hint_text(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ヒントテキスト:",
            Language::En => "Hint text:",
        }
    }

    pub fn heading_font(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォント",
            Language::En => "Font",
        }
    }

    pub fn label_font_family(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォントファミリー:",
            Language::En => "Font family:",
        }
    }

    pub fn label_font_size(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォントサイズ:",
            Language::En => "Font size:",
        }
    }

    pub fn label_custom_theme(&self) -> &'static str {
        match self.0 {
            Language::Ja => "カスタム",
            Language::En => "Custom",
        }
    }

    // Opener tab - presets
    pub fn heading_presets(&self) -> &'static str {
        match self.0 {
            Language::Ja => "よく使うツール",
            Language::En => "Common tools",
        }
    }

    pub fn preset_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "検出されたツールをワンクリックで追加できます。",
            Language::En => "Add detected tools with one click.",
        }
    }

    pub fn btn_add_preset(&self) -> &'static str {
        match self.0 {
            Language::Ja => "追加",
            Language::En => "Add",
        }
    }

    pub fn label_already_added(&self) -> &'static str {
        match self.0 {
            Language::Ja => "追加済み",
            Language::En => "Added",
        }
    }

    // Opener tab
    pub fn heading_opener_rules(&self) -> &'static str {
        match self.0 {
            Language::Ja => "オープナールール",
            Language::En => "Opener rules",
        }
    }

    pub fn opener_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ファイル種別ごとに起動するアプリケーションを設定します。",
            Language::En => "Configure applications to launch by file type.",
        }
    }

    pub fn label_no_rules(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ルールが設定されていません。",
            Language::En => "No rules configured.",
        }
    }

    pub fn label_no_name(&self) -> &'static str {
        match self.0 {
            Language::Ja => "(名前なし)",
            Language::En => "(unnamed)",
        }
    }

    pub fn modal_edit_rule(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ルールを編集",
            Language::En => "Edit rule",
        }
    }

    pub fn modal_add_rule(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ルールを追加",
            Language::En => "Add rule",
        }
    }

    pub fn label_target(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ターゲット:",
            Language::En => "Target:",
        }
    }

    pub fn target_kind_folder(&self) -> &'static str {
        match self.0 {
            Language::Ja => "フォルダ",
            Language::En => "Folder",
        }
    }

    pub fn target_kind_extension(&self) -> &'static str {
        match self.0 {
            Language::Ja => "拡張子",
            Language::En => "Extension",
        }
    }

    pub fn label_extension(&self) -> &'static str {
        match self.0 {
            Language::Ja => "拡張子:",
            Language::En => "Extension:",
        }
    }

    pub fn hint_extension_format(&self) -> &'static str {
        match self.0 {
            Language::Ja => "カンマ区切り (例: .png, .jpg)",
            Language::En => "Comma-separated (e.g. .png, .jpg)",
        }
    }

    pub fn label_path_condition(&self) -> &'static str {
        match self.0 {
            Language::Ja => "パス条件:",
            Language::En => "Path condition:",
        }
    }

    pub fn hint_path_condition(&self) -> &'static str {
        match self.0 {
            Language::Ja => "空欄 = すべてにマッチ (例: C:\\workspace)",
            Language::En => "Empty = match all (e.g. C:\\workspace)",
        }
    }

    pub fn label_all_folders(&self) -> &'static str {
        match self.0 {
            Language::Ja => "すべてのフォルダ",
            Language::En => "All folders",
        }
    }

    pub fn label_tool_name(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ツール名:",
            Language::En => "Tool name:",
        }
    }

    pub fn label_executable(&self) -> &'static str {
        match self.0 {
            Language::Ja => "実行ファイル:",
            Language::En => "Executable:",
        }
    }

    pub fn label_arguments(&self) -> &'static str {
        match self.0 {
            Language::Ja => "引数:",
            Language::En => "Arguments:",
        }
    }

    pub fn hint_path_placeholder(&self) -> &'static str {
        match self.0 {
            Language::Ja => "{path} でファイルパスを埋め込み",
            Language::En => "Use {path} for file path",
        }
    }

    pub fn dialog_select_exe(&self) -> &'static str {
        match self.0 {
            Language::Ja => "実行ファイルを選択",
            Language::En => "Select executable",
        }
    }

    pub fn filter_executables(&self) -> &'static str {
        match self.0 {
            Language::Ja => "実行ファイル",
            Language::En => "Executables",
        }
    }

    // Hotkey input
    pub fn hotkey_press_key(&self) -> &'static str {
        match self.0 {
            Language::Ja => "キーを押してください...",
            Language::En => "Press a key...",
        }
    }

    pub fn hotkey_not_set(&self) -> &'static str {
        match self.0 {
            Language::Ja => "(未設定)",
            Language::En => "(not set)",
        }
    }

    // Instant command tab
    pub fn heading_instant_prefix(&self) -> &'static str {
        match self.0 {
            Language::Ja => "プレフィックス",
            Language::En => "Prefix",
        }
    }

    pub fn label_instant_prefix(&self) -> &'static str {
        match self.0 {
            Language::Ja => "プレフィックス:",
            Language::En => "Prefix:",
        }
    }

    pub fn hint_instant_prefix(&self) -> &'static str {
        match self.0 {
            Language::Ja => "検索バーでこの文字を入力するとインスタントコマンドモードになります",
            Language::En => "Type this character in the search bar to enter instant command mode",
        }
    }

    pub fn heading_instant_commands(&self) -> &'static str {
        match self.0 {
            Language::Ja => "コマンド一覧",
            Language::En => "Commands",
        }
    }

    pub fn instant_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "URL や実行ファイルを登録し、検索バーから即座に実行できます。{query} と {clip} が変数として展開されます",
            Language::En => "Register URLs or executables to run instantly from the search bar. {query} and {clip} are expanded as variables",
        }
    }

    pub fn label_no_instant_commands(&self) -> &'static str {
        match self.0 {
            Language::Ja => "コマンドが登録されていません",
            Language::En => "No commands registered",
        }
    }

    pub fn label_instant_name(&self) -> &'static str {
        match self.0 {
            Language::Ja => "名前:",
            Language::En => "Name:",
        }
    }

    pub fn hint_instant_name(&self) -> &'static str {
        match self.0 {
            Language::Ja => "プレフィックスの後に入力する文字列です（例: \"g\" と設定すると \"@g\" で呼び出し）",
            Language::En => "Text typed after the prefix to invoke this command (e.g. name \"g\" → type \"@g\")",
        }
    }

    pub fn label_instant_command(&self) -> &'static str {
        match self.0 {
            Language::Ja => "コマンド:",
            Language::En => "Command:",
        }
    }

    pub fn hint_instant_command(&self) -> &'static str {
        match self.0 {
            Language::Ja => "{query} でクエリ、{clip} でクリップボードの内容を埋め込み。URL は自動エンコード",
            Language::En => "Use {query} for query, {clip} for clipboard content. URLs are auto-encoded",
        }
    }

    pub fn label_instant_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "説明（任意）:",
            Language::En => "Description (optional):",
        }
    }

    pub fn hint_instant_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "コマンドの用途を記述します。検索結果リストに表示されます",
            Language::En => "Describe what this command does. Shown in the search results list",
        }
    }

    pub fn label_instant_preview(&self) -> &'static str {
        match self.0 {
            Language::Ja => "プレビュー:",
            Language::En => "Preview:",
        }
    }

    pub fn btn_duplicate(&self) -> &'static str {
        match self.0 {
            Language::Ja => "複製",
            Language::En => "Duplicate",
        }
    }

    pub fn modal_add_instant(&self) -> &'static str {
        match self.0 {
            Language::Ja => "コマンドを追加",
            Language::En => "Add Command",
        }
    }

    pub fn modal_edit_instant(&self) -> &'static str {
        match self.0 {
            Language::Ja => "コマンドを編集",
            Language::En => "Edit Command",
        }
    }

    // Backup tab
    pub fn tab_backup(&self) -> &'static str {
        match self.0 {
            Language::Ja => "バックアップ",
            Language::En => "Backup",
        }
    }

    pub fn heading_export(&self) -> &'static str {
        match self.0 {
            Language::Ja => "エクスポート",
            Language::En => "Export",
        }
    }

    pub fn label_export_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "現在の設定ファイルを別の場所にコピーします。",
            Language::En => "Copy the current settings file to another location.",
        }
    }

    pub fn btn_export(&self) -> &'static str {
        match self.0 {
            Language::Ja => "設定をエクスポート…",
            Language::En => "Export settings…",
        }
    }

    pub fn heading_import(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インポート",
            Language::En => "Import",
        }
    }

    pub fn label_import_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "エクスポートした設定ファイルを読み込みます。現在の設定は上書きされます。",
            Language::En => "Load an exported settings file. Current settings will be overwritten.",
        }
    }

    pub fn btn_import(&self) -> &'static str {
        match self.0 {
            Language::Ja => "設定をインポート…",
            Language::En => "Import settings…",
        }
    }

    pub fn heading_data_folder(&self) -> &'static str {
        match self.0 {
            Language::Ja => "データフォルダ",
            Language::En => "Data folder",
        }
    }

    pub fn label_data_folder_description(&self) -> &'static str {
        match self.0 {
            Language::Ja => "設定ファイルやキャッシュが保存されているフォルダを開きます。",
            Language::En => "Open the folder where settings and cache files are stored.",
        }
    }

    pub fn btn_open_folder(&self) -> &'static str {
        match self.0 {
            Language::Ja => "設定フォルダを開く",
            Language::En => "Open settings folder",
        }
    }

    pub fn status_export_success(&self) -> &'static str {
        match self.0 {
            Language::Ja => "エクスポートしました",
            Language::En => "Exported",
        }
    }

    pub fn status_export_failed(&self) -> &'static str {
        match self.0 {
            Language::Ja => "エクスポート失敗: ",
            Language::En => "Export failed: ",
        }
    }

    pub fn status_import_success(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インポートしました",
            Language::En => "Imported",
        }
    }

    pub fn status_import_failed(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インポート失敗: ",
            Language::En => "Import failed: ",
        }
    }

    pub fn status_import_validation_error(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インポートファイルにエラーがあります: ",
            Language::En => "Import file has errors: ",
        }
    }

    pub fn dialog_export_config(&self) -> &'static str {
        match self.0 {
            Language::Ja => "設定をエクスポート",
            Language::En => "Export settings",
        }
    }

    pub fn dialog_import_config(&self) -> &'static str {
        match self.0 {
            Language::Ja => "設定をインポート",
            Language::En => "Import settings",
        }
    }

    pub fn filter_toml(&self) -> &'static str {
        match self.0 {
            Language::Ja => "TOML ファイル",
            Language::En => "TOML files",
        }
    }

    pub fn err_toml_missing_field(&self, field: &str) -> String {
        match self.0 {
            Language::Ja => format!("\"{}\" が必要です", field),
            Language::En => format!("missing field \"{}\"", field),
        }
    }

    pub fn err_toml_invalid_type(&self) -> &'static str {
        match self.0 {
            Language::Ja => "値の型が違います",
            Language::En => "invalid type",
        }
    }

    pub fn err_toml_parse_error(&self) -> &'static str {
        match self.0 {
            Language::Ja => "構文エラー",
            Language::En => "syntax error",
        }
    }

    pub fn heading_path_env(&self) -> &'static str {
        match self.0 {
            Language::Ja => "PATH 実行ファイル",
            Language::En => "PATH Executables",
        }
    }

    pub fn cb_include_path_env(&self) -> &'static str {
        match self.0 {
            Language::Ja => "PATH の実行ファイルを検索対象に含める",
            Language::En => "Include executables from PATH",
        }
    }

    pub fn err_migemo_min_chars_zero(&self) -> &'static str {
        match self.0 {
            Language::Ja => "migemo 最小文字数は 1 以上である必要があります",
            Language::En => "Migemo min chars must be at least 1",
        }
    }

    pub fn err_icon_cache_cap_too_small(&self) -> &'static str {
        match self.0 {
            Language::Ja => "アイコンキャッシュ上限は表示件数（max_results）以上である必要があります",
            Language::En => "Icon cache cap must be at least max_results",
        }
    }

    pub fn heading_migemo(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ローマ字検索（Migemo）",
            Language::En => "Romaji Search (Migemo)",
        }
    }

    pub fn cb_migemo_enabled(&self) -> &'static str {
        match self.0 {
            Language::Ja => "ローマ字入力で日本語名ファイルを検索する",
            Language::En => "Search Japanese filenames with romaji input",
        }
    }

    pub fn hint_migemo(&self) -> &'static str {
        match self.0 {
            Language::Ja => "例: \"dokyu\" と入力すると \"ドキュメント\" がヒットします。漢字名は対象外",
            Language::En => "e.g. type \"dokyu\" to find \"ドキュメント\". Kanji names are not supported",
        }
    }

    pub fn label_migemo_min_chars(&self) -> &'static str {
        match self.0 {
            Language::Ja => "最小文字数:",
            Language::En => "Min chars:",
        }
    }

    pub fn heading_auto_update(&self) -> &'static str {
        match self.0 {
            Language::Ja => "自動更新",
            Language::En => "Auto Update",
        }
    }

    pub fn auto_update_full(&self) -> &'static str {
        match self.0 {
            Language::Ja => "インストール含む",
            Language::En => "Check and install",
        }
    }

    pub fn auto_update_check_only(&self) -> &'static str {
        match self.0 {
            Language::Ja => "更新チェックのみ",
            Language::En => "Check only (notify)",
        }
    }

    pub fn auto_update_disabled(&self) -> &'static str {
        match self.0 {
            Language::Ja => "更新チェックしない",
            Language::En => "Disabled",
        }
    }
}
