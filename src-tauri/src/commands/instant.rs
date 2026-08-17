use snotra_core::config::InstantCommand;
use snotra_core::instant::{expand_instant_command, filter_instant_commands};

/// 絞り込みと DTO 化を 1 つにした核。
///
/// **DTO 化まで config の読みの中で終える必要がある**——`filter_instant_commands` が返すのは
/// `Vec<&InstantCommand>` で、config を借りたままの参照だからである。読みの外へ出すには所有へ
/// 移す一手が要り、それが `InstantCommandDto` への変換とちょうど同じ仕事になる。
/// 行うのは文字列の確保までで、I/O も錠も無い（[`crate::egui_shell::read_config`] の
/// 「`read` の中で lock を取る操作を書かないこと」を満たす）。
fn matching_dtos(
    commands: &[InstantCommand],
    prefix_input: &str,
) -> Vec<super::launch::InstantCommandDto> {
    filter_instant_commands(commands, prefix_input)
        .into_iter()
        .map(super::launch::InstantCommandDto::from)
        .collect()
}

/// instant 候補の絞り込み（`egui_shell` の Instant 枝と共有する唯一の実装）。
///
/// **この関数は `commands/` に在るが egui フレームの中で毎打鍵走る**——唯一の呼び出し元は
/// `egui_shell::launcher_controller` の `run_search_with` の Instant 枝である。ゆえに config は
/// [`crate::egui_shell::read_config`] から読む（#1076）。#1032 条項の例外が名指すのは
/// **egui フレームの外で行う読み**（icon worker・folder worker・tray スレッド）であって
/// `commands/` というディレクトリではない——**弁別子はフレームを止めるかである**（正本は
/// `src-tauri/CLAUDE.md`「モジュール構成」の当該条項）。
///
/// **AppState 不在時に panic しなくなった**（#1076）: 以前は `app.state::<AppState>()` が
/// panic していた。`.manage` は `.setup` より前ゆえ到達しない経路だが、そこでは既定の
/// instant コマンドで絞り込みが走る（既定は型から導く・`ADR-config-default-fallback-references`）。
pub fn get_instant_commands(
    prefix_input: String,
    app: tauri::AppHandle,
) -> Result<Vec<super::launch::InstantCommandDto>, String> {
    Ok(crate::egui_shell::read_config(
        &app,
        |c| matching_dtos(&c.instant_commands, &prefix_input),
        || {
            matching_dtos(
                &snotra_core::config::Config::default().instant_commands,
                &prefix_input,
            )
        },
    ))
}

/// instant action の種別ディスパッチ核（egui 経路 = `egui_shell::launcher_controller::LauncherController::execute_instant_selected`
/// が呼ぶ・#532 SU3 M3。IPC 経路は SU7 PR3 のフロント撤去で消滅）。
/// clipboard は呼び出し側がエンジンロック外で読んで渡す（Win32 がブロックしうるため）。
///
/// 変数展開: URL テンプレートは自動エンコード、それ以外は生展開。
/// セキュリティモデル: コマンドテンプレートはユーザーが config.toml で
/// 自身で定義したものであり、信頼済みコンテンツとして扱う。
pub(crate) fn execute_instant_action_core(
    action: snotra_core::config::InstantAction,
    query: &str,
    clipboard: &str,
) -> super::launch::LaunchResult {
    use snotra_core::config::InstantAction;
    match action {
        InstantAction::Url { url } => {
            let expanded = expand_instant_command(&url, query, clipboard);
            super::launch::launch_item_core(&expanded)
        }
        InstantAction::Exec { exe, args } => {
            super::launch::launch_exec_core(&exe, &args, query, clipboard)
        }
        // load 後は移行済みで到達しないが、防御的に Url 扱い
        InstantAction::Legacy { command } => {
            let expanded = expand_instant_command(&command, query, clipboard);
            super::launch::launch_item_core(&expanded)
        }
    }
}
