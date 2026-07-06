use snotra_core::config::Language;

pub struct Tr(pub Language);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrKey {
    // Window title
    WindowTitle,
    // Tab names
    TabGeneral,
    TabSearch,
    TabIndex,
    TabVisual,
    TabOpener,
    TabInstantCommand,
    // Footer buttons
    BtnSave,
    BtnDiscard,
    BtnResetDefault,
    // Status messages
    StatusSaved,
    StatusUnsaved,
    StatusValidationError,
    StatusSaveFailed,
    // 読み込み失敗（finding C: 一時的 read 失敗で既定値表示中）の保存ガード
    ReadFailedBanner,
    ReadFailedConfirm,
    StatusReadFailedBlocked,
    // Config error messages
    ErrHotkeyModifierEmpty,
    ErrHotkeyKeyEmpty,
    ErrHotkeySystemConflict,
    ErrVisibleRowsZero,
    ErrWindowWidthTooSmall,
    ErrFuzzyCapRatioOutOfRange,
    ErrScanPathEmpty,
    ErrInstantPrefixEmpty,
    ErrInstantPrefixSlash,
    ErrInstantDuplicateName,
    ErrInstantUnknownModifier,
    // General tab
    HeadingHotkey,
    LabelHotkey,
    CbHotkeyToggle,
    HeadingAppearance,
    LabelVisibleRows,
    LabelWindowWidth,
    CbShowIcons,
    HeadingBehavior,
    CbShowOnStartup,
    CbAutoHide,
    CbTrayIcon,
    CbImeOff,
    CbFollowCursorMonitor,
    HeadingLanguage,
    LabelLanguage,
    LanguageJa,
    LanguageEn,
    // Search tab
    HeadingSearchMode,
    LabelNormalMode,
    LabelFolderMode,
    SearchModePrefix,
    SearchModeSubstring,
    SearchModeFuzzy,
    HeadingVisibility,
    CbShowHiddenSystem,
    HeadingHistory,
    LabelResultLimit,
    LabelRecentLimit,
    HeadingHistoryScore,
    LabelNormalization,
    NormalizationDisabled,
    NormalizationFuzzyRelativeCap,
    LabelFuzzyCapRatio,
    // Index tab
    HeadingScanTargets,
    LabelNoScanPaths,
    IndexScanExtensionsWithFolders,
    BtnEdit,
    BtnAdd,
    ModalEditScanPath,
    ModalAddScanPath,
    LabelPath,
    BtnBrowse,
    LabelExtensions,
    CbIncludeFolders,
    BtnDelete,
    BtnCancel,
    DialogSelectFolder,
    // Visual tab
    HeadingTheme,
    BtnSaveCustomTheme,
    HeadingColor,
    LabelBgColor,
    LabelInputBg,
    LabelTextColor,
    LabelSelectedRow,
    LabelHintText,
    HeadingFont,
    LabelFontFamily,
    LabelFontSize,
    LabelCustomTheme,
    // Opener tab - presets
    HeadingPresets,
    PresetDescription,
    BtnAddPreset,
    LabelAlreadyAdded,
    // Opener tab
    HeadingOpenerRules,
    OpenerDescription,
    LabelNoRules,
    LabelNoName,
    ModalEditRule,
    ModalAddRule,
    LabelTarget,
    TargetKindFolder,
    TargetKindExtension,
    LabelExtension,
    HintExtensionFormat,
    LabelPathCondition,
    HintPathCondition,
    LabelAllFolders,
    LabelToolName,
    LabelExecutable,
    LabelArguments,
    HintPathPlaceholder,
    DialogSelectExe,
    FilterExecutables,
    FilterAllFiles,
    // Hotkey input
    HotkeyPressKey,
    HotkeyNotSet,
    // Instant command tab
    HeadingInstantPrefix,
    LabelInstantPrefix,
    HintInstantPrefix,
    HeadingInstantCommands,
    InstantDescription,
    LabelNoInstantCommands,
    LabelInstantName,
    HintInstantName,
    LabelInstantCommand,
    HintInstantCommand,
    LabelInstantDescription,
    HintInstantDescription,
    LabelInstantPreview,
    BtnDuplicate,
    ModalAddInstant,
    ModalEditInstant,
    LabelInstantKind,
    RadioInstantUrl,
    RadioInstantProgram,
    LabelInstantExe,
    LabelInstantArgs,
    HintInstantProgram,
    HintInstantMigrate,
    // Backup tab
    TabBackup,
    HeadingExport,
    LabelExportDescription,
    BtnExport,
    HeadingImport,
    LabelImportDescription,
    BtnImport,
    HeadingDataFolder,
    LabelDataFolderDescription,
    BtnOpenFolder,
    StatusExportSuccess,
    StatusExportFailed,
    StatusImportSuccess,
    StatusImportFailed,
    StatusImportValidationError,
    DialogExportConfig,
    DialogImportConfig,
    FilterToml,
    ErrTomlMissingField,
    ErrTomlInvalidType,
    ErrTomlParseError,
    HeadingPathEnv,
    CbIncludePathEnv,
    ErrMigemoMinCharsZero,
    HeadingMigemo,
    CbMigemoEnabled,
    HintMigemo,
    LabelMigemoMinChars,
    HeadingAutoUpdate,
    AutoUpdateFull,
    AutoUpdateCheckOnly,
    AutoUpdateDisabled,
}

#[deny(clippy::wildcard_enum_match_arm)]
fn ja(key: TrKey) -> &'static str {
    match key {
        // Window title
        TrKey::WindowTitle => "Snotra 設定",
        // Tab names
        TrKey::TabGeneral => "全般",
        TrKey::TabSearch => "検索",
        TrKey::TabIndex => "インデックス",
        TrKey::TabVisual => "ビジュアル",
        TrKey::TabOpener => "オープナー",
        TrKey::TabInstantCommand => "インスタント",
        // Footer buttons
        TrKey::BtnSave => "保存",
        TrKey::BtnDiscard => "破棄",
        TrKey::BtnResetDefault => "初期設定に戻す",
        // Status messages
        TrKey::StatusSaved => "保存しました",
        TrKey::StatusUnsaved => "未保存の変更があります",
        TrKey::StatusValidationError => "検証エラー: ",
        TrKey::StatusSaveFailed => "保存失敗: ",
        // 読み込み失敗（finding C: 一時的 read 失敗で既定値表示中）の保存ガード
        TrKey::ReadFailedBanner => "⚠ 設定ファイルを読み込めませんでした。既定値で表示しています。このまま保存すると既存の設定が失われる可能性があります。",
        TrKey::ReadFailedConfirm => "既存設定が失われる可能性を承知の上で保存する",
        TrKey::StatusReadFailedBlocked => "読み込み失敗中: 上のチェックを入れると保存できます",
        // Config error messages
        TrKey::ErrHotkeyModifierEmpty => "ホットキーの修飾キーが未設定です",
        TrKey::ErrHotkeyKeyEmpty => "ホットキーのキーが未設定です",
        TrKey::ErrHotkeySystemConflict => "{value} はシステムショートカットと競合します",
        TrKey::ErrVisibleRowsZero => "最大表示件数は1以上にしてください",
        TrKey::ErrWindowWidthTooSmall => "{value} は小さすぎます（200以上）",
        TrKey::ErrFuzzyCapRatioOutOfRange => "{value} は 0.0〜1.0 の範囲にしてください",
        TrKey::ErrScanPathEmpty => "{n}のパスが空です",
        TrKey::ErrInstantPrefixEmpty => "インスタントコマンドのプレフィックスが空です",
        TrKey::ErrInstantPrefixSlash => "プレフィックスに / は使用できません（スラッシュコマンドと競合）",
        TrKey::ErrInstantDuplicateName => "{name} はコマンド名が重複しています",
        TrKey::ErrInstantUnknownModifier => "{name}: 不明な修飾子「{modifier}」が含まれています",
        // General tab
        TrKey::HeadingHotkey => "ホットキー",
        TrKey::LabelHotkey => "ホットキー:",
        TrKey::CbHotkeyToggle => "ホットキーで表示中のウィンドウを非表示にする",
        TrKey::HeadingAppearance => "外観",
        TrKey::LabelVisibleRows => "最大表示件数:",
        TrKey::LabelWindowWidth => "ウィンドウ幅:",
        TrKey::CbShowIcons => "アイコンを表示",
        TrKey::HeadingBehavior => "動作",
        TrKey::CbShowOnStartup => "起動時にウィンドウを表示",
        TrKey::CbAutoHide => "フォーカス喪失時に非表示",
        TrKey::CbTrayIcon => "トレイアイコンを表示",
        TrKey::CbImeOff => "表示時に IME をオフにする",
        TrKey::CbFollowCursorMonitor => "カーソルのあるモニターに表示",
        TrKey::HeadingLanguage => "言語",
        TrKey::LabelLanguage => "言語:",
        TrKey::LanguageJa => "日本語",
        TrKey::LanguageEn => "English",
        // Search tab
        TrKey::HeadingSearchMode => "検索モード",
        TrKey::LabelNormalMode => "通常モード:",
        TrKey::LabelFolderMode => "フォルダモード:",
        TrKey::SearchModePrefix => "前方一致",
        TrKey::SearchModeSubstring => "部分一致",
        TrKey::SearchModeFuzzy => "あいまい",
        TrKey::HeadingVisibility => "表示",
        TrKey::CbShowHiddenSystem => "隠しファイル・システムファイルを表示",
        TrKey::HeadingHistory => "履歴",
        TrKey::LabelResultLimit => "最大列挙数:",
        TrKey::LabelRecentLimit => "最大履歴表示件数:",
        TrKey::HeadingHistoryScore => "履歴スコア",
        TrKey::LabelNormalization => "正規化:",
        TrKey::NormalizationDisabled => "無効",
        TrKey::NormalizationFuzzyRelativeCap => "Fuzzy 相対キャップ",
        TrKey::LabelFuzzyCapRatio => "Fuzzy 履歴キャップ比率:",
        // Index tab
        TrKey::HeadingScanTargets => "スキャン対象",
        TrKey::LabelNoScanPaths => "スキャンパスが設定されていません。",
        TrKey::IndexScanExtensionsWithFolders => "{extensions} (フォルダ含む)",
        TrKey::BtnEdit => "編集",
        TrKey::BtnAdd => "追加…",
        TrKey::ModalEditScanPath => "スキャンパスを編集",
        TrKey::ModalAddScanPath => "スキャンパスを追加",
        TrKey::LabelPath => "パス:",
        TrKey::BtnBrowse => "参照…",
        TrKey::LabelExtensions => "拡張子 (カンマ区切り):",
        TrKey::CbIncludeFolders => "フォルダを含む",
        TrKey::BtnDelete => "削除",
        TrKey::BtnCancel => "キャンセル",
        TrKey::DialogSelectFolder => "フォルダを選択",
        // Visual tab
        TrKey::HeadingTheme => "テーマ",
        TrKey::BtnSaveCustomTheme => "カスタムテーマとして保存",
        TrKey::HeadingColor => "カラー",
        TrKey::LabelBgColor => "背景色:",
        TrKey::LabelInputBg => "入力欄背景:",
        TrKey::LabelTextColor => "テキスト:",
        TrKey::LabelSelectedRow => "選択行:",
        TrKey::LabelHintText => "ヒントテキスト:",
        TrKey::HeadingFont => "フォント",
        TrKey::LabelFontFamily => "フォントファミリー:",
        TrKey::LabelFontSize => "フォントサイズ:",
        TrKey::LabelCustomTheme => "カスタム",
        // Opener tab - presets
        TrKey::HeadingPresets => "よく使うツール",
        TrKey::PresetDescription => "検出されたツールをワンクリックで追加できます。",
        TrKey::BtnAddPreset => "追加",
        TrKey::LabelAlreadyAdded => "追加済み",
        // Opener tab
        TrKey::HeadingOpenerRules => "オープナールール",
        TrKey::OpenerDescription => "ファイル種別ごとに起動するアプリケーションを設定します。",
        TrKey::LabelNoRules => "ルールが設定されていません。",
        TrKey::LabelNoName => "(名前なし)",
        TrKey::ModalEditRule => "ルールを編集",
        TrKey::ModalAddRule => "ルールを追加",
        TrKey::LabelTarget => "ターゲット:",
        TrKey::TargetKindFolder => "フォルダ",
        TrKey::TargetKindExtension => "拡張子",
        TrKey::LabelExtension => "拡張子:",
        TrKey::HintExtensionFormat => "カンマ区切り (例: .png, .jpg)",
        TrKey::LabelPathCondition => "パス条件:",
        TrKey::HintPathCondition => "空欄 = すべてにマッチ (例: C:\\workspace)",
        TrKey::LabelAllFolders => "すべてのフォルダ",
        TrKey::LabelToolName => "ツール名:",
        TrKey::LabelExecutable => "実行ファイル:",
        TrKey::LabelArguments => "引数:",
        TrKey::HintPathPlaceholder => "{path} でファイルパスを埋め込み",
        TrKey::DialogSelectExe => "実行ファイルを選択",
        TrKey::FilterExecutables => "実行ファイル",
        TrKey::FilterAllFiles => "すべてのファイル",
        // Hotkey input
        TrKey::HotkeyPressKey => "キーを押してください...",
        TrKey::HotkeyNotSet => "(未設定)",
        // Instant command tab
        TrKey::HeadingInstantPrefix => "プレフィックス",
        TrKey::LabelInstantPrefix => "プレフィックス:",
        TrKey::HintInstantPrefix => "検索バーでこの文字を入力するとインスタントコマンドモードになります",
        TrKey::HeadingInstantCommands => "コマンド一覧",
        TrKey::InstantDescription => "URL や実行ファイルを登録し、検索バーから即座に実行できます。{query} {clip} {date} {uuid} が変数として展開されます",
        TrKey::LabelNoInstantCommands => "コマンドが登録されていません",
        TrKey::LabelInstantName => "名前:",
        TrKey::HintInstantName => "プレフィックスの後に入力する文字列です（例: \"g\" と設定すると \"@g\" で呼び出し）",
        TrKey::LabelInstantCommand => "コマンド:",
        TrKey::HintInstantCommand => "{query} クエリ・{clip} クリップボード・{date:%Y-%m-%d} 日時・{uuid} を埋め込み。literal は {{ }}（{{date}}）。URL は自動エンコード",
        TrKey::LabelInstantDescription => "説明（任意）:",
        TrKey::HintInstantDescription => "コマンドの用途を記述します。検索結果リストに表示されます",
        TrKey::LabelInstantPreview => "プレビュー:",
        TrKey::BtnDuplicate => "複製",
        TrKey::ModalAddInstant => "コマンドを追加",
        TrKey::ModalEditInstant => "コマンドを編集",
        TrKey::LabelInstantKind => "種別",
        TrKey::RadioInstantUrl => "URL / 既定アプリで開く",
        TrKey::RadioInstantProgram => "プログラム（exe + 引数）",
        TrKey::LabelInstantExe => "実行ファイル (.exe)",
        TrKey::LabelInstantArgs => "引数",
        TrKey::HintInstantProgram => ".exe のみ。スクリプトはインタプリタを実行ファイルに指定。{query} / {clip} / {date} / {uuid} と %VAR% が使えます。literal は {{ }}",
        TrKey::HintInstantMigrate => "引数つきの可能性があります。プログラム種別へ作り直すと正しく起動します",
        // Backup tab
        TrKey::TabBackup => "バックアップ",
        TrKey::HeadingExport => "エクスポート",
        TrKey::LabelExportDescription => "現在の設定ファイルを別の場所にコピーします。",
        TrKey::BtnExport => "設定をエクスポート…",
        TrKey::HeadingImport => "インポート",
        TrKey::LabelImportDescription => "エクスポートした設定ファイルを読み込みます。現在の設定は上書きされます。",
        TrKey::BtnImport => "設定をインポート…",
        TrKey::HeadingDataFolder => "データフォルダ",
        TrKey::LabelDataFolderDescription => "設定ファイルやキャッシュが保存されているフォルダを開きます。",
        TrKey::BtnOpenFolder => "設定フォルダを開く",
        TrKey::StatusExportSuccess => "エクスポートしました",
        TrKey::StatusExportFailed => "エクスポート失敗: ",
        TrKey::StatusImportSuccess => "インポートしました",
        TrKey::StatusImportFailed => "インポート失敗: ",
        TrKey::StatusImportValidationError => "インポートファイルにエラーがあります: ",
        TrKey::DialogExportConfig => "設定をエクスポート",
        TrKey::DialogImportConfig => "設定をインポート",
        TrKey::FilterToml => "TOML ファイル",
        TrKey::ErrTomlMissingField => "\"{field}\" が必要です",
        TrKey::ErrTomlInvalidType => "値の型が違います",
        TrKey::ErrTomlParseError => "構文エラー",
        TrKey::HeadingPathEnv => "PATH 実行ファイル",
        TrKey::CbIncludePathEnv => "PATH の実行ファイルを検索対象に含める",
        TrKey::ErrMigemoMinCharsZero => "migemo 最小文字数は 1 以上である必要があります",
        TrKey::HeadingMigemo => "ローマ字検索（Migemo）",
        TrKey::CbMigemoEnabled => "ローマ字入力で日本語名ファイルを検索する",
        TrKey::HintMigemo => "例: \"dokyu\" と入力すると \"ドキュメント\" がヒットします。漢字名は対象外",
        TrKey::LabelMigemoMinChars => "最小文字数:",
        TrKey::HeadingAutoUpdate => "自動更新",
        TrKey::AutoUpdateFull => "インストール含む",
        TrKey::AutoUpdateCheckOnly => "更新チェックのみ",
        TrKey::AutoUpdateDisabled => "更新チェックしない",
    }
}

#[deny(clippy::wildcard_enum_match_arm)]
fn en(key: TrKey) -> &'static str {
    match key {
        // Window title
        TrKey::WindowTitle => "Snotra Settings",
        // Tab names
        TrKey::TabGeneral => "General",
        TrKey::TabSearch => "Search",
        TrKey::TabIndex => "Index",
        TrKey::TabVisual => "Visual",
        TrKey::TabOpener => "Opener",
        TrKey::TabInstantCommand => "Instant",
        // Footer buttons
        TrKey::BtnSave => "Save",
        TrKey::BtnDiscard => "Discard",
        TrKey::BtnResetDefault => "Reset to default",
        // Status messages
        TrKey::StatusSaved => "Saved",
        TrKey::StatusUnsaved => "Unsaved changes",
        TrKey::StatusValidationError => "Validation error: ",
        TrKey::StatusSaveFailed => "Save failed: ",
        // 読み込み失敗（finding C: 一時的 read 失敗で既定値表示中）の保存ガード
        TrKey::ReadFailedBanner => "⚠ Could not read the settings file. Showing defaults. Saving now may overwrite your existing settings.",
        TrKey::ReadFailedConfirm => "Save anyway (I understand existing settings may be lost)",
        TrKey::StatusReadFailedBlocked => "Load failed: check the box above to enable saving",
        // Config error messages
        TrKey::ErrHotkeyModifierEmpty => "Hotkey modifier is not set",
        TrKey::ErrHotkeyKeyEmpty => "Hotkey key is not set",
        TrKey::ErrHotkeySystemConflict => "{value} conflicts with a system shortcut",
        TrKey::ErrVisibleRowsZero => "Max results must be at least 1",
        TrKey::ErrWindowWidthTooSmall => "{value} is too small (min 200)",
        TrKey::ErrFuzzyCapRatioOutOfRange => "{value} must be between 0.0 and 1.0",
        TrKey::ErrScanPathEmpty => "{n} path is empty",
        TrKey::ErrInstantPrefixEmpty => "Instant command prefix is empty",
        TrKey::ErrInstantPrefixSlash => "Prefix cannot be / (conflicts with slash commands)",
        TrKey::ErrInstantDuplicateName => "{name} has a duplicate command name",
        TrKey::ErrInstantUnknownModifier => "{name}: contains unknown modifier \"{modifier}\"",
        // General tab
        TrKey::HeadingHotkey => "Hotkey",
        TrKey::LabelHotkey => "Hotkey:",
        TrKey::CbHotkeyToggle => "Toggle window visibility with hotkey",
        TrKey::HeadingAppearance => "Appearance",
        TrKey::LabelVisibleRows => "Max results:",
        TrKey::LabelWindowWidth => "Window width:",
        TrKey::CbShowIcons => "Show icons",
        TrKey::HeadingBehavior => "Behavior",
        TrKey::CbShowOnStartup => "Show window on startup",
        TrKey::CbAutoHide => "Hide on focus lost",
        TrKey::CbTrayIcon => "Show tray icon",
        TrKey::CbImeOff => "Turn off IME on show",
        TrKey::CbFollowCursorMonitor => "Show on monitor with cursor",
        TrKey::HeadingLanguage => "Language",
        TrKey::LabelLanguage => "Language:",
        TrKey::LanguageJa => "日本語",
        TrKey::LanguageEn => "English",
        // Search tab
        TrKey::HeadingSearchMode => "Search mode",
        TrKey::LabelNormalMode => "Normal mode:",
        TrKey::LabelFolderMode => "Folder mode:",
        TrKey::SearchModePrefix => "Prefix",
        TrKey::SearchModeSubstring => "Substring",
        TrKey::SearchModeFuzzy => "Fuzzy",
        TrKey::HeadingVisibility => "Visibility",
        TrKey::CbShowHiddenSystem => "Show hidden/system files",
        TrKey::HeadingHistory => "History",
        TrKey::LabelResultLimit => "Max entries:",
        TrKey::LabelRecentLimit => "Max history display:",
        TrKey::HeadingHistoryScore => "History score",
        TrKey::LabelNormalization => "Normalization:",
        TrKey::NormalizationDisabled => "Disabled",
        TrKey::NormalizationFuzzyRelativeCap => "Fuzzy relative cap",
        TrKey::LabelFuzzyCapRatio => "Fuzzy history cap ratio:",
        // Index tab
        TrKey::HeadingScanTargets => "Scan targets",
        TrKey::LabelNoScanPaths => "No scan paths configured.",
        TrKey::IndexScanExtensionsWithFolders => "{extensions} (incl. folders)",
        TrKey::BtnEdit => "Edit",
        TrKey::BtnAdd => "Add…",
        TrKey::ModalEditScanPath => "Edit scan path",
        TrKey::ModalAddScanPath => "Add scan path",
        TrKey::LabelPath => "Path:",
        TrKey::BtnBrowse => "Browse…",
        TrKey::LabelExtensions => "Extensions (comma-separated):",
        TrKey::CbIncludeFolders => "Include folders",
        TrKey::BtnDelete => "Delete",
        TrKey::BtnCancel => "Cancel",
        TrKey::DialogSelectFolder => "Select folder",
        // Visual tab
        TrKey::HeadingTheme => "Theme",
        TrKey::BtnSaveCustomTheme => "Save as custom theme",
        TrKey::HeadingColor => "Colors",
        TrKey::LabelBgColor => "Background:",
        TrKey::LabelInputBg => "Input background:",
        TrKey::LabelTextColor => "Text:",
        TrKey::LabelSelectedRow => "Selected row:",
        TrKey::LabelHintText => "Hint text:",
        TrKey::HeadingFont => "Font",
        TrKey::LabelFontFamily => "Font family:",
        TrKey::LabelFontSize => "Font size:",
        TrKey::LabelCustomTheme => "Custom",
        // Opener tab - presets
        TrKey::HeadingPresets => "Common tools",
        TrKey::PresetDescription => "Add detected tools with one click.",
        TrKey::BtnAddPreset => "Add",
        TrKey::LabelAlreadyAdded => "Added",
        // Opener tab
        TrKey::HeadingOpenerRules => "Opener rules",
        TrKey::OpenerDescription => "Configure applications to launch by file type.",
        TrKey::LabelNoRules => "No rules configured.",
        TrKey::LabelNoName => "(unnamed)",
        TrKey::ModalEditRule => "Edit rule",
        TrKey::ModalAddRule => "Add rule",
        TrKey::LabelTarget => "Target:",
        TrKey::TargetKindFolder => "Folder",
        TrKey::TargetKindExtension => "Extension",
        TrKey::LabelExtension => "Extension:",
        TrKey::HintExtensionFormat => "Comma-separated (e.g. .png, .jpg)",
        TrKey::LabelPathCondition => "Path condition:",
        TrKey::HintPathCondition => "Empty = match all (e.g. C:\\workspace)",
        TrKey::LabelAllFolders => "All folders",
        TrKey::LabelToolName => "Tool name:",
        TrKey::LabelExecutable => "Executable:",
        TrKey::LabelArguments => "Arguments:",
        TrKey::HintPathPlaceholder => "Use {path} for file path",
        TrKey::DialogSelectExe => "Select executable",
        TrKey::FilterExecutables => "Executables",
        TrKey::FilterAllFiles => "All files",
        // Hotkey input
        TrKey::HotkeyPressKey => "Press a key...",
        TrKey::HotkeyNotSet => "(not set)",
        // Instant command tab
        TrKey::HeadingInstantPrefix => "Prefix",
        TrKey::LabelInstantPrefix => "Prefix:",
        TrKey::HintInstantPrefix => "Type this character in the search bar to enter instant command mode",
        TrKey::HeadingInstantCommands => "Commands",
        TrKey::InstantDescription => "Register URLs or executables to run instantly from the search bar. {query}, {clip}, {date}, {uuid} are expanded as variables",
        TrKey::LabelNoInstantCommands => "No commands registered",
        TrKey::LabelInstantName => "Name:",
        TrKey::HintInstantName => "Text typed after the prefix to invoke this command (e.g. name \"g\" → type \"@g\")",
        TrKey::LabelInstantCommand => "Command:",
        TrKey::HintInstantCommand => "Embed {query}, {clip}, {date:%Y-%m-%d}, {uuid}; literal via {{ }}. URLs are auto-encoded",
        TrKey::LabelInstantDescription => "Description (optional):",
        TrKey::HintInstantDescription => "Describe what this command does. Shown in the search results list",
        TrKey::LabelInstantPreview => "Preview:",
        TrKey::BtnDuplicate => "Duplicate",
        TrKey::ModalAddInstant => "Add Command",
        TrKey::ModalEditInstant => "Edit Command",
        TrKey::LabelInstantKind => "Kind",
        TrKey::RadioInstantUrl => "URL / open with default app",
        TrKey::RadioInstantProgram => "Program (exe + args)",
        TrKey::LabelInstantExe => "Executable (.exe)",
        TrKey::LabelInstantArgs => "Arguments",
        TrKey::HintInstantProgram => ".exe only. For scripts, set the interpreter as the executable. {query} / {clip} / {date} / {uuid} and %VAR% are supported; literal via {{ }}",
        TrKey::HintInstantMigrate => "This may contain arguments. Recreate it as a Program command to launch correctly",
        // Backup tab
        TrKey::TabBackup => "Backup",
        TrKey::HeadingExport => "Export",
        TrKey::LabelExportDescription => "Copy the current settings file to another location.",
        TrKey::BtnExport => "Export settings…",
        TrKey::HeadingImport => "Import",
        TrKey::LabelImportDescription => "Load an exported settings file. Current settings will be overwritten.",
        TrKey::BtnImport => "Import settings…",
        TrKey::HeadingDataFolder => "Data folder",
        TrKey::LabelDataFolderDescription => "Open the folder where settings and cache files are stored.",
        TrKey::BtnOpenFolder => "Open settings folder",
        TrKey::StatusExportSuccess => "Exported",
        TrKey::StatusExportFailed => "Export failed: ",
        TrKey::StatusImportSuccess => "Imported",
        TrKey::StatusImportFailed => "Import failed: ",
        TrKey::StatusImportValidationError => "Import file has errors: ",
        TrKey::DialogExportConfig => "Export settings",
        TrKey::DialogImportConfig => "Import settings",
        TrKey::FilterToml => "TOML files",
        TrKey::ErrTomlMissingField => "missing field \"{field}\"",
        TrKey::ErrTomlInvalidType => "invalid type",
        TrKey::ErrTomlParseError => "syntax error",
        TrKey::HeadingPathEnv => "PATH Executables",
        TrKey::CbIncludePathEnv => "Include executables from PATH",
        TrKey::ErrMigemoMinCharsZero => "Migemo min chars must be at least 1",
        TrKey::HeadingMigemo => "Romaji Search (Migemo)",
        TrKey::CbMigemoEnabled => "Search Japanese filenames with romaji input",
        TrKey::HintMigemo => "e.g. type \"dokyu\" to find \"ドキュメント\". Kanji names are not supported",
        TrKey::LabelMigemoMinChars => "Min chars:",
        TrKey::HeadingAutoUpdate => "Auto Update",
        TrKey::AutoUpdateFull => "Check and install",
        TrKey::AutoUpdateCheckOnly => "Check only (notify)",
        TrKey::AutoUpdateDisabled => "Disabled",
    }
}

impl Tr {
    pub fn t(&self, key: TrKey) -> &'static str {
        match self.0 {
            Language::Ja => ja(key),
            Language::En => en(key),
        }
    }

    /// `{param}` 形式のプレースホルダーを `params` の値で置換した完全文を返す。
    ///
    /// テンプレートを1回だけ左から右へ走査する。置換後の値は再走査しないため、
    /// ある param の値が別 param の `{name}` と同じ文字列を含んでいても
    /// 二重置換は起きない（逐次 `String::replace` の落とし穴を避ける）。
    pub fn t_params(&self, key: TrKey, params: &[(&str, &str)]) -> String {
        let template = self.t(key);
        let mut result = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(brace_pos) = rest.find('{') {
            let (before, after_brace) = rest.split_at(brace_pos);
            result.push_str(before);
            let after_brace = &after_brace[1..];
            match after_brace.find('}') {
                Some(close_pos) => {
                    let name = &after_brace[..close_pos];
                    match params.iter().find(|(n, _)| *n == name) {
                        Some((_, value)) => result.push_str(value),
                        None => {
                            result.push('{');
                            result.push_str(name);
                            result.push('}');
                        }
                    }
                    rest = &after_brace[close_pos + 1..];
                }
                None => {
                    result.push('{');
                    rest = after_brace;
                }
            }
        }
        result.push_str(rest);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_returns_ja_and_en_for_same_key() {
        assert_eq!(Tr(Language::Ja).t(TrKey::BtnSave), "保存");
        assert_eq!(Tr(Language::En).t(TrKey::BtnSave), "Save");
    }

    #[test]
    fn t_params_substitutes_single_placeholder() {
        assert_eq!(
            Tr(Language::Ja).t_params(TrKey::ErrInstantDuplicateName, &[("name", "g")]),
            "g はコマンド名が重複しています"
        );
    }

    #[test]
    fn t_params_leaves_unmatched_placeholder_untouched() {
        assert_eq!(
            Tr(Language::En).t_params(TrKey::ErrScanPathEmpty, &[("wrong_name", "1")]),
            "{n} path is empty"
        );
    }

    // 回帰テスト（code-reviewer 指摘）: 逐次 String::replace だと、1番目の param 値が
    // 2番目の param のプレースホルダ文字列（例 "{modifier}"）を偶然含む場合、
    // その値の中身まで誤って置換されてしまう。単一パス実装ではテンプレート文字列だけを
    // 走査し、置換後の値は再走査しないためこの二重置換が起きないことを検証する。
    #[test]
    fn t_params_does_not_rescan_substituted_values() {
        let out = Tr(Language::Ja).t_params(
            TrKey::ErrInstantUnknownModifier,
            &[("name", "{modifier}"), ("modifier", "xyz")],
        );
        assert_eq!(out, "{modifier}: 不明な修飾子「xyz」が含まれています");
    }
}
