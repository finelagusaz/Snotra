use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{
    ExtractIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_WARNING, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, GetMessageTime, HICON,
    HMENU, IDI_APPLICATION, LoadIconW, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL,
    WM_RBUTTONUP,
};
use windows::core::PCWSTR;

use snotra_core::config::Language;

use crate::commands;
use crate::state::AppState;

use super::WM_TRAY_ICON;

pub(super) const ID_MENU_SETTINGS: usize = 1000;
pub(super) const ID_MENU_EXIT: usize = 1001;
const ID_MENU_RECENT_BASE: usize = 2000;
// ツールサブメニュー項目の ID 範囲（>= ID_MENU_TOOL_BASE は常に ID_MENU_RECENT_BASE より大きい）
const ID_MENU_TOOL_BASE: usize = 3000;

pub(super) fn recent_history_items(
    app_handle: &AppHandle,
) -> Vec<snotra_core::ui_types::SearchResult> {
    let state = app_handle.state::<AppState>();
    state.engine.lock().unwrap().recent_history()
}

fn format_recent_history_label(item: &snotra_core::ui_types::SearchResult) -> String {
    const MAX_LABEL_LEN: usize = 90;
    let base = if item.name.is_empty() || item.name == item.path {
        item.path.clone()
    } else {
        format!("{} - {}", item.name, item.path)
    };
    if base.chars().count() <= MAX_LABEL_LEN {
        return base;
    }
    let mut truncated: String = base.chars().take(MAX_LABEL_LEN - 3).collect();
    truncated.push_str("...");
    truncated
}

pub(super) fn handle_menu_command(wparam: WPARAM, app_handle: &AppHandle, tray: &Option<TrayIcon>) {
    let id = wparam.0 & 0xFFFF;
    match id {
        ID_MENU_SETTINGS => {
            let _ = app_handle.emit(crate::events::OPEN_SETTINGS, ());
        }
        ID_MENU_EXIT => {
            let _ = app_handle.emit(crate::events::EXIT_REQUESTED, ());
        }
        // ID_MENU_TOOL_BASE > ID_MENU_RECENT_BASE のため先に評価する
        id if id >= ID_MENU_TOOL_BASE => {
            if let Some((path, exe, args)) = tray.as_ref().and_then(|t| t.recent_tool_for_id(id)) {
                let state = app_handle.state::<AppState>();
                if exe.is_empty() {
                    // exe が空 = 「標準」: オープナールールを無視して ShellExecuteW で起動
                    commands::launch_default_with_state(&path, &state);
                } else {
                    commands::launch_with_tool_with_state(&path, &exe, &args, &state);
                }
            }
        }
        id if id >= ID_MENU_RECENT_BASE => {
            if let Some(path) = tray.as_ref().and_then(|t| t.recent_path_for_id(id)) {
                let state = app_handle.state::<AppState>();
                commands::launch_item_with_state(&path, "", &state);
            }
        }
        _ => {}
    }
}

pub(super) fn handle_tray_message(
    tray: &mut Option<TrayIcon>,
    hwnd: HWND,
    lparam: LPARAM,
    app_handle: &AppHandle,
    indexing: bool,
    last_rbuttonup_msg_time: &mut i32,
) {
    let event = (lparam.0 & 0xFFFF) as u32;
    match event {
        x if x == WM_LBUTTONUP => {
            if let Some(tray) = tray.as_mut() {
                tray.show_recent_history_menu(hwnd, app_handle);
            }
        }
        x if x == WM_RBUTTONUP => {
            // Record the message timestamp so WM_CONTEXTMENU can detect whether it
            // was triggered by the same click (duplicate) or by a keyboard shortcut.
            *last_rbuttonup_msg_time = unsafe { GetMessageTime() };
            if let Some(tray) = tray.as_ref() {
                tray.show_context_menu(hwnd, indexing);
            }
        }
        x if x == WM_CONTEXTMENU => {
            // Some Shell environments deliver both WM_RBUTTONUP and WM_CONTEXTMENU for
            // a single right-click.  When the messages belong to the same click they
            // arrive within a few milliseconds of each other; we use a 500 ms threshold
            // to distinguish that case from a keyboard-triggered context menu request
            // (Shift+F10 / Application key), which can arrive any time after the last
            // right-click.
            //
            // A bool flag is NOT sufficient here: Windows 11 does not send
            // WM_CONTEXTMENU for mouse right-clicks, so a flag set by WM_RBUTTONUP
            // would never be cleared and would permanently suppress keyboard requests.
            let now = unsafe { GetMessageTime() };
            let elapsed = now.wrapping_sub(*last_rbuttonup_msg_time);
            if elapsed > 500
                && let Some(tray) = tray.as_ref()
            {
                tray.show_context_menu(hwnd, indexing);
            }
        }
        _ => {}
    }
}

pub(super) struct TrayIcon {
    nid: NOTIFYICONDATAW,
    owned_icon: Option<HICON>,
    language: Language,
    /// 0/1 ツール項目のパス（ID = ID_MENU_RECENT_BASE + vec の添字）
    recent_menu_paths: Vec<String>,
    /// 2+ ツール項目のサブメニュー選択情報 (path, exe, args)（ID = ID_MENU_TOOL_BASE + vec の添字）
    recent_menu_tools: Vec<(String, String, String)>,
}

/// UTF-16 + NUL 終端で固定長 wide 文字列フィールド（`szTip`/`szInfo`/`szInfoTitle`）に書き込む。
/// 長すぎる場合は末尾の NUL を必ず残して切り詰める。
fn write_wide_field(field: &mut [u16], s: &str) {
    field.fill(0);
    let encoded: Vec<u16> = s.encode_utf16().collect();
    let len = encoded.len().min(field.len().saturating_sub(1));
    field[..len].copy_from_slice(&encoded[..len]);
}

/// 「最近使った履歴」メニュー1項目分の純データ表現。ID 割当・表示ラベルの組み立てを
/// Win32 描画（`draw_recent_menu`）から独立させ、ユニットテスト可能にする
/// （`format_recent_history_label` のテストスタイルに合わせる、#433）。
enum RecentMenuEntry {
    /// 履歴が空のときのプレースホルダ（クリック不可）。
    Empty,
    /// ツールが0/1件: 直接起動（クリックで即起動、既存挙動）。
    Direct { id: usize, label: String },
    /// ツールが2件以上: サブメニュー。先頭に「標準」（exe 空文字の番兵）を含む。
    Submenu {
        label: String,
        /// (メニュー ID, 表示名)。`items[0]` が「標準」。
        items: Vec<(usize, String)>,
    },
}

/// `build_recent_menu_entries` の戻り値: (メニュー項目リスト, 0/1ツール項目の
/// パス一覧 `recent_menu_paths`, 2+ツール項目の選択情報一覧 `recent_menu_tools`)。
type RecentMenuBuild = (
    Vec<RecentMenuEntry>,
    Vec<String>,
    Vec<(String, String, String)>,
);

/// `recent`（履歴一覧）と `resolve_tools`（パス→ツール一覧の解決）から、純データと
/// してのメニュー項目リストと ID→(path[, exe, args]) の対応表を組み立てる。
/// `CreatePopupMenu`/`AppendMenuW` 等の Win32 描画を一切呼ばない。
fn build_recent_menu_entries(
    recent: &[snotra_core::ui_types::SearchResult],
    resolve_tools: impl Fn(&str) -> Vec<(String, String, String)>,
    language: Language,
) -> RecentMenuBuild {
    let mut entries = Vec::new();
    let mut recent_menu_paths = Vec::new();
    let mut recent_menu_tools: Vec<(String, String, String)> = Vec::new();

    if recent.is_empty() {
        entries.push(RecentMenuEntry::Empty);
        return (entries, recent_menu_paths, recent_menu_tools);
    }

    let default_tool_label = match language {
        Language::Ja => "標準",
        Language::En => "Default",
    };

    for item in recent {
        let label = format_recent_history_label(item);
        let tools = resolve_tools(&item.path);
        if tools.len() >= 2 {
            // 2 つ以上のツールがある場合はツール選択サブメニューを構築
            // 先頭に「標準」（ShellExecuteW 直接、exe が空文字 = 番兵）を追加
            let standard_id = ID_MENU_TOOL_BASE + recent_menu_tools.len();
            let mut items = vec![(standard_id, default_tool_label.to_string())];
            recent_menu_tools.push((item.path.clone(), String::new(), String::new()));
            for (name, exe, args) in &tools {
                let tool_id = ID_MENU_TOOL_BASE + recent_menu_tools.len();
                items.push((tool_id, name.clone()));
                recent_menu_tools.push((item.path.clone(), exe.clone(), args.clone()));
            }
            entries.push(RecentMenuEntry::Submenu { label, items });
        } else {
            // 0/1 ツール: 直接起動（既存挙動）
            let direct_id = ID_MENU_RECENT_BASE + recent_menu_paths.len();
            entries.push(RecentMenuEntry::Direct {
                id: direct_id,
                label,
            });
            recent_menu_paths.push(item.path.clone());
        }
    }

    (entries, recent_menu_paths, recent_menu_tools)
}

/// `entries`（純データ）を Win32 ポップアップメニューへ描画する。
/// `CreatePopupMenu`/`AppendMenuW` の呼び出しに徹し、ID 割当ロジックは持たない。
unsafe fn draw_recent_menu(hmenu: HMENU, entries: &[RecentMenuEntry], no_history_label: &str) {
    for entry in entries {
        match entry {
            RecentMenuEntry::Empty => {
                let text: Vec<u16> = no_history_label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let _ = unsafe { AppendMenuW(hmenu, MF_GRAYED, 0, PCWSTR(text.as_ptr())) };
            }
            RecentMenuEntry::Direct { id, label } => {
                let text: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = unsafe { AppendMenuW(hmenu, MF_STRING, *id, PCWSTR(text.as_ptr())) };
            }
            RecentMenuEntry::Submenu { label, items } => {
                let Ok(hsub) = (unsafe { CreatePopupMenu() }) else {
                    continue;
                };
                for (id, name) in items {
                    let text: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                    let _ = unsafe { AppendMenuW(hsub, MF_STRING, *id, PCWSTR(text.as_ptr())) };
                }
                let label_wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                // MF_POPUP: uIDNewItem に hsub のハンドル値を渡す
                let _ = unsafe {
                    AppendMenuW(
                        hmenu,
                        MF_POPUP,
                        hsub.0 as usize,
                        PCWSTR(label_wide.as_ptr()),
                    )
                };
            }
        }
    }
}

impl TrayIcon {
    pub(super) fn set_language(&mut self, lang: Language) {
        self.language = lang;
    }

    /// 設定の読み込みに失敗し既定値で起動した（壊れた config を `config.toml.bak` へ退避済み）
    /// ことをトレイバルーンで通知する。`NIM_MODIFY` + `NIF_INFO`。再発火を防ぐため送信直後に
    /// `NIF_INFO` を落とす（以後の `NIM_MODIFY` でバルーンが再表示されない）。
    pub(super) fn show_config_recovery_balloon(&mut self) {
        let (title, body) = match self.language {
            Language::Ja => (
                "Snotra: 設定を読み込めませんでした",
                "config.toml を読み込めず既定値で起動しました。元の内容は config.toml.bak に退避しています。",
            ),
            Language::En => (
                "Snotra: failed to load settings",
                "Couldn't read config.toml; started with defaults. The original was backed up to config.toml.bak.",
            ),
        };
        write_wide_field(&mut self.nid.szInfoTitle, title);
        write_wide_field(&mut self.nid.szInfo, body);
        self.nid.dwInfoFlags = NIIF_WARNING;
        self.nid.uFlags |= NIF_INFO;
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &self.nid);
        }
        // 再発火防止: 以後の NIM_MODIFY でバルーンが再表示されないよう NIF_INFO を落とす
        self.nid.uFlags &= !NIF_INFO;
    }

    pub(super) fn create(hwnd: HWND, language: Language) -> Self {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            // For NOTIFYICON_VERSION_4, show the standard tooltip explicitly.
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_TRAY_ICON,
            ..Default::default()
        };
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;

        write_wide_field(&mut nid.szTip, "Snotra");

        let owned_icon = load_tray_icon_from_exe();
        nid.hIcon = owned_icon
            .unwrap_or_else(|| unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default());

        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }

        Self {
            nid,
            owned_icon,
            language,
            recent_menu_paths: Vec::new(),
            recent_menu_tools: Vec::new(),
        }
    }

    pub(super) fn recent_path_for_id(&self, id: usize) -> Option<String> {
        let offset = id.checked_sub(ID_MENU_RECENT_BASE)?;
        self.recent_menu_paths.get(offset).cloned()
    }

    pub(super) fn recent_tool_for_id(&self, id: usize) -> Option<(String, String, String)> {
        let offset = id.checked_sub(ID_MENU_TOOL_BASE)?;
        self.recent_menu_tools.get(offset).cloned()
    }

    fn show_context_menu(&self, hwnd: HWND, indexing: bool) {
        unsafe {
            let Ok(hmenu) = CreatePopupMenu() else {
                return;
            };

            let indexing_label = match self.language {
                Language::Ja => "インデックス再構築中",
                Language::En => "Rebuilding index",
            };
            let settings_label = match self.language {
                Language::Ja => "設定(&S)",
                Language::En => "&Settings",
            };
            let exit_label = match self.language {
                Language::Ja => "終了(&X)",
                Language::En => "E&xit",
            };

            if indexing {
                let indexing_text: Vec<u16> = indexing_label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let settings_text: Vec<u16> = settings_label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let exit_text: Vec<u16> = exit_label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();

                let _ = AppendMenuW(hmenu, MF_GRAYED, 0, PCWSTR(indexing_text.as_ptr()));
                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(
                    hmenu,
                    MF_GRAYED,
                    ID_MENU_SETTINGS,
                    PCWSTR(settings_text.as_ptr()),
                );
                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(hmenu, MF_GRAYED, ID_MENU_EXIT, PCWSTR(exit_text.as_ptr()));
            } else {
                let settings_text: Vec<u16> = settings_label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let exit_text: Vec<u16> = exit_label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();

                let _ = AppendMenuW(
                    hmenu,
                    MF_STRING,
                    ID_MENU_SETTINGS,
                    PCWSTR(settings_text.as_ptr()),
                );
                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(hmenu, MF_STRING, ID_MENU_EXIT, PCWSTR(exit_text.as_ptr()));
            }

            let mut pt = Default::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(hwnd);

            let command = TrackPopupMenuEx(
                hmenu,
                (TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_NONOTIFY | TPM_RETURNCMD)
                    .0,
                pt.x,
                pt.y,
                hwnd,
                None,
            );

            if command.0 != 0 {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_COMMAND,
                    WPARAM(command.0 as usize),
                    LPARAM(0),
                );
            }

            // MSDN: send WM_NULL after TrackPopupMenuEx so the menu dismisses correctly.
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(hmenu);
        }
    }

    fn show_recent_history_menu(&mut self, hwnd: HWND, app_handle: &AppHandle) {
        unsafe {
            let Ok(hmenu) = CreatePopupMenu() else {
                return;
            };

            let recent = recent_history_items(app_handle);
            let state = app_handle.state::<AppState>();
            let no_history_label = match self.language {
                Language::Ja => "履歴なし",
                Language::En => "No history",
            };

            let (entries, recent_menu_paths, recent_menu_tools) = build_recent_menu_entries(
                &recent,
                |path| commands::resolve_all_openers(path, &state),
                self.language,
            );
            self.recent_menu_paths = recent_menu_paths;
            self.recent_menu_tools = recent_menu_tools;

            draw_recent_menu(hmenu, &entries, no_history_label);

            let mut pt = Default::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(hwnd);

            let command = TrackPopupMenuEx(
                hmenu,
                (TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD).0,
                pt.x,
                pt.y,
                hwnd,
                None,
            );
            if command.0 != 0 {
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_COMMAND,
                    WPARAM(command.0 as usize),
                    LPARAM(0),
                );
            }

            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(hmenu);
        }
    }

    fn remove(&self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
        if let Some(icon) = self.owned_icon.take() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}

fn load_tray_icon_from_exe() -> Option<HICON> {
    let exe_path = std::env::current_exe().ok()?;
    let wide_path: Vec<u16> = exe_path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // Extract the first icon from the running executable so tray icon and app icon stay aligned.
    let icon = unsafe { ExtractIconW(None, PCWSTR(wide_path.as_ptr()), 0) };
    if (icon.0 as usize) <= 1 {
        return None;
    }
    Some(icon)
}

#[cfg(test)]
mod tests {
    use super::{
        ID_MENU_RECENT_BASE, ID_MENU_TOOL_BASE, RecentMenuEntry, build_recent_menu_entries,
        format_recent_history_label,
    };
    use snotra_core::config::Language;
    use snotra_core::ui_types::{IconSource, SearchResult};

    fn item(name: &str, path: &str) -> SearchResult {
        SearchResult {
            name: name.to_string(),
            path: path.to_string(),
            is_folder: false,
            is_error: false,
            icon: IconSource::FromPath,
        }
    }

    #[test]
    fn empty_name_falls_back_to_path_only() {
        let label = format_recent_history_label(&item("", "C:/Users/foo/bar.exe"));
        assert_eq!(label, "C:/Users/foo/bar.exe");
    }

    #[test]
    fn name_equal_to_path_dedupes_to_path_only() {
        let label =
            format_recent_history_label(&item("C:/Users/foo/bar.exe", "C:/Users/foo/bar.exe"));
        assert_eq!(label, "C:/Users/foo/bar.exe");
    }

    #[test]
    fn both_empty_yields_empty_label() {
        // name.is_empty() は true なので path (空文字) をそのまま返す。
        let label = format_recent_history_label(&item("", ""));
        assert_eq!(label, "");
    }

    #[test]
    fn distinct_name_and_path_formats_as_name_dash_path() {
        let label = format_recent_history_label(&item("bar.exe", "C:/Users/foo/bar.exe"));
        assert_eq!(label, "bar.exe - C:/Users/foo/bar.exe");
    }

    #[test]
    fn exactly_max_len_is_not_truncated() {
        // MAX_LABEL_LEN(90) ちょうどでは "..." を付けない（境界値）。
        let path = "x".repeat(90);
        let label = format_recent_history_label(&item("", &path));
        assert_eq!(label, path);
        assert_eq!(label.chars().count(), 90);
    }

    #[test]
    fn one_over_max_len_truncates_to_87_chars_plus_ellipsis() {
        // 91 文字は 87 文字 + "..." の合計 90 文字に切り詰められる。
        let path = "x".repeat(91);
        let label = format_recent_history_label(&item("", &path));
        let expected = format!("{}...", "x".repeat(87));
        assert_eq!(label, expected);
        assert_eq!(label.chars().count(), 90);
    }

    #[test]
    fn multibyte_truncation_cuts_on_char_boundary_not_byte_boundary() {
        // 日本語（1文字3バイト）で 103 文字のラベルを構築し、マルチバイト境界での
        // 切り詰めがパニックせず、文字単位（バイト単位ではない）で 87 文字に切り詰められることを検証する。
        let name = "あ".repeat(50);
        let path = "い".repeat(50);
        let label = format_recent_history_label(&item(&name, &path));

        // base = "あ"*50 + " - " + "い"*50 = 103 文字。先頭 87 文字 = "あ"*50 + " - " + "い"*34。
        let expected = format!("{}{}{}...", "あ".repeat(50), " - ", "い".repeat(34));
        assert_eq!(label, expected);
        assert_eq!(label.chars().count(), 90);
    }

    #[test]
    fn empty_recent_yields_single_placeholder_entry() {
        let (entries, paths, tools) = build_recent_menu_entries(&[], |_| vec![], Language::En);
        assert!(matches!(entries.as_slice(), [RecentMenuEntry::Empty]));
        assert!(paths.is_empty());
        assert!(tools.is_empty());
    }

    #[test]
    fn item_with_no_tools_becomes_direct_entry() {
        let items = vec![item("bar.exe", "C:/Users/foo/bar.exe")];
        let (entries, paths, tools) = build_recent_menu_entries(&items, |_| vec![], Language::En);
        match entries.as_slice() {
            [RecentMenuEntry::Direct { id, label }] => {
                assert_eq!(*id, ID_MENU_RECENT_BASE);
                assert_eq!(label, "bar.exe - C:/Users/foo/bar.exe");
            }
            other => panic!(
                "expected a single Direct entry, got {} entries",
                other.len()
            ),
        }
        assert_eq!(paths, vec!["C:/Users/foo/bar.exe".to_string()]);
        assert!(tools.is_empty());
    }

    #[test]
    fn item_with_one_tool_still_becomes_direct_entry_not_submenu() {
        // 1 ツールは「直接起動」扱い（サブメニューが必要になるのは 2 件以上）。
        let items = vec![item("bar.exe", "C:/Users/foo/bar.exe")];
        let (entries, paths, tools) = build_recent_menu_entries(
            &items,
            |_| {
                vec![(
                    "Notepad".to_string(),
                    "notepad.exe".to_string(),
                    String::new(),
                )]
            },
            Language::En,
        );
        assert!(matches!(
            entries.as_slice(),
            [RecentMenuEntry::Direct { .. }]
        ));
        assert_eq!(paths.len(), 1);
        assert!(tools.is_empty());
    }

    #[test]
    fn item_with_two_tools_becomes_submenu_with_standard_entry_first() {
        let items = vec![item("bar.exe", "C:/Users/foo/bar.exe")];
        let (entries, paths, tools) = build_recent_menu_entries(
            &items,
            |_| {
                vec![
                    (
                        "Notepad".to_string(),
                        "notepad.exe".to_string(),
                        String::new(),
                    ),
                    ("VS Code".to_string(), "code.exe".to_string(), String::new()),
                ]
            },
            Language::En,
        );
        match entries.as_slice() {
            [RecentMenuEntry::Submenu { label, items }] => {
                assert_eq!(label, "bar.exe - C:/Users/foo/bar.exe");
                assert_eq!(items.len(), 3); // 標準 + 2 ツール
                assert_eq!(items[0].0, ID_MENU_TOOL_BASE);
                assert_eq!(items[0].1, "Default");
                assert_eq!(items[1].1, "Notepad");
                assert_eq!(items[2].1, "VS Code");
            }
            other => panic!(
                "expected a single Submenu entry, got {} entries",
                other.len()
            ),
        }
        assert!(paths.is_empty()); // サブメニュー項目は recent_menu_tools 側に積まれる
        assert_eq!(tools.len(), 3);
        assert_eq!(
            tools[0],
            (
                "C:/Users/foo/bar.exe".to_string(),
                String::new(),
                String::new()
            )
        );
        assert_eq!(tools[1].1, "notepad.exe");
        assert_eq!(tools[2].1, "code.exe");
    }

    #[test]
    fn tool_ids_accumulate_across_multiple_submenu_items() {
        // 2 項目ともサブメニューになる場合、ID が項目をまたいで連番であることを確認する
        // （show_recent_history_menu の self.recent_menu_tools と同じ蓄積順序）。
        let items = vec![item("a.exe", "C:/a.exe"), item("b.exe", "C:/b.exe")];
        let two_tools = |_: &str| {
            vec![
                ("T1".to_string(), "t1.exe".to_string(), String::new()),
                ("T2".to_string(), "t2.exe".to_string(), String::new()),
            ]
        };
        let (entries, _paths, tools) = build_recent_menu_entries(&items, two_tools, Language::Ja);
        assert_eq!(entries.len(), 2);
        assert_eq!(tools.len(), 6); // (標準+2) * 2 項目
        match &entries[1] {
            RecentMenuEntry::Submenu { items, .. } => {
                // 2件目のサブメニューの標準 ID は 1件目の3項目分だけ進んでいる
                assert_eq!(items[0].0, ID_MENU_TOOL_BASE + 3);
            }
            _ => panic!("expected Submenu"),
        }
    }
}
