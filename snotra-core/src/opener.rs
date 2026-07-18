//! opener（外部ツール起動ルール）ターゲットの解析・正規化・マッチングエンジンと、
//! Win 環境のプリセット検出（`detect_opener_presets`）。`config.rs` から分離（issue #435）。
//! `OpenerRule` / `OpenerTool` は `Config.openers` として config.toml に紐づく serde 型のため、
//! 依存方向は `config.rs` → `opener.rs`（config が opener の型・関数を re-export で使う）。

use crate::config::{normalize_extensions, normalize_scan_path_key};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenerTool {
    pub name: String,
    pub exe: String,
    #[serde(default)]
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenerRule {
    pub target: String,
    pub tools: Vec<OpenerTool>,
}

/// オープナーターゲットからパス条件を抽出する。
/// - `"folder"` → None
/// - `"folder:C:\\workspace"` → Some("C:\\workspace")
/// - `"ext:.md"` → None
/// - `"ext:.md:C:\\projects"` → Some("C:\\projects")
pub fn extract_path_condition(target: &str) -> Option<&str> {
    if let Some(rest) = target.strip_prefix("folder:") {
        if !rest.is_empty() {
            return Some(rest);
        }
    } else if let Some(after_ext) = target.strip_prefix("ext:") {
        return split_ext_and_path(after_ext).1;
    }
    None
}

/// ターゲットから拡張子リスト部分のみ取得する（パス条件を除外）。
pub fn extract_ext_part(target: &str) -> &str {
    debug_assert!(target.starts_with("ext:"), "extract_ext_part called on non-ext target: {target}");
    let after_ext = &target["ext:".len()..];
    if let Some(path_cond) = extract_path_condition(target) {
        // パス条件の直前のコロンまでが拡張子部分
        let path_start = after_ext.len() - path_cond.len() - 1; // -1 for ':'
        &after_ext[..path_start]
    } else {
        after_ext
    }
}

/// パスとフォルダフラグに対してマッチするツール一覧を返す。
/// 最も具体的にマッチした1ルールを返す（パス条件が長い方が具体的）。
/// マッチするルールがなければ空スライスを返す（呼び出し側でフォールバック処理）。
pub fn find_matching_tools<'a>(
    path: &str,
    is_folder: bool,
    rules: &'a [OpenerRule],
) -> &'a [OpenerTool] {
    let path_lower = path.to_lowercase().replace('/', "\\");
    let path_ext = path_lower
        .rfind('.')
        .map(|i| &path_lower[i..])
        .unwrap_or("");

    let mut best: Option<(usize, usize)> = None; // (rule_index, specificity)
    // specificity: 0 = no path condition, N = path condition length

    for (idx, rule) in rules.iter().enumerate() {
        let target = &rule.target;
        let path_cond = extract_path_condition(target);

        // パス条件チェック（パス境界で一致を検証）
        if let Some(cond) = path_cond {
            let cond_lower = cond.to_lowercase().replace('/', "\\");
            if !path_lower.starts_with(&cond_lower) {
                continue;
            }
            // パス条件がパス境界で終わっていることを確認
            // 例: 条件 "C:\workspace" はパス "C:\workspace123" にマッチしない
            // 条件自体がパス区切りで終わっている場合はすでに境界OK
            let cond_ends_with_sep =
                cond_lower.ends_with('\\') || cond_lower.ends_with('/');
            if !cond_ends_with_sep {
                let next_byte = path_lower.as_bytes().get(cond_lower.len());
                if next_byte.is_some()
                    && next_byte != Some(&b'\\')
                    && next_byte != Some(&b'/')
                {
                    continue;
                }
            }
        }

        let kind_match = if is_folder {
            target == "folder" || target.starts_with("folder:")
        } else if target.starts_with("ext:") {
            let ext_part = extract_ext_part(target);
            ext_part.split(',').any(|raw_ext| {
                let ext = raw_ext.trim().to_lowercase();
                let ext_with_dot = if ext.starts_with('.') {
                    ext
                } else {
                    format!(".{ext}")
                };
                path_ext == ext_with_dot
            })
        } else {
            false
        };

        if !kind_match {
            continue;
        }

        let specificity = path_cond.map_or(0, |c| c.len());

        if let Some((_, best_spec)) = best {
            if specificity > best_spec {
                best = Some((idx, specificity));
            }
            // 同具体度は先のルール（定義順）が勝つので更新しない
        } else {
            best = Some((idx, specificity));
        }
    }

    match best {
        Some((idx, _)) => &rules[idx].tools,
        None => &[],
    }
}

/// "ext:" プレフィックスの後の部分から拡張子リストとパス条件を分離する。
/// 例: "md,txt" → ("md,txt", None)
/// 例: ".md:C:\\projects" → (".md", Some("C:\\projects"))
/// ドライブレターパターン `:X:\` or `:X:/` でパス条件の開始を検出する。
fn split_ext_and_path(rest: &str) -> (&str, Option<&str>) {
    let bytes = rest.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b':'
            && bytes[i + 1].is_ascii_alphabetic()
            && i + 2 < bytes.len()
            && (bytes[i + 2] == b':' || bytes[i + 2] == b'\\' || bytes[i + 2] == b'/')
        {
            let ext_part = &rest[..i];
            let path_part = rest[i + 1..].trim();
            if !path_part.is_empty() {
                return (ext_part, Some(path_part));
            }
        }
    }
    (rest, None)
}

fn normalize_opener_target(target: &str) -> String {
    let trimmed = target.trim();

    // folder, folder:<path>, ext:<exts>, ext:<exts>:<path>
    if let Some((kind, rest)) = trimmed.split_once(':') {
        if kind.eq_ignore_ascii_case("folder") {
            let path_trimmed = rest.trim();
            if path_trimmed.is_empty() {
                return "folder".to_string();
            }
            let normalized_path = normalize_scan_path_key(path_trimmed);
            if normalized_path.is_empty() {
                return "folder".to_string();
            }
            return format!("folder:{normalized_path}");
        }
        if kind.eq_ignore_ascii_case("ext") {
            // rest から拡張子部分とパス条件を分離
            let (raw_exts, path_suffix) = split_ext_and_path(rest);
            let exts = normalize_extensions(
                &raw_exts
                    .split(',')
                    .map(|ext| ext.to_string())
                    .collect::<Vec<_>>(),
            );
            let ext_str = exts.join(",");
            return if let Some(path) = path_suffix {
                let normalized_path = normalize_scan_path_key(path);
                if normalized_path.is_empty() {
                    format!("ext:{ext_str}")
                } else {
                    format!("ext:{ext_str}:{normalized_path}")
                }
            } else {
                format!("ext:{ext_str}")
            };
        }
    }

    if trimmed.eq_ignore_ascii_case("folder") {
        return "folder".to_string();
    }

    trimmed.to_string()
}

/// オープナールール列のターゲットを正規化し、具体度の高い順にソートして返す。
/// 各 `target` を `normalize_opener_target`（拡張子・パス条件の正規化）に通したうえで、
/// `opener_specificity_order` をキーにソートする——`find_matching_tools` が「最も具体的な
/// 1ルール」を先頭から解決できるようにするため。重複ターゲットは畳み込む。
pub fn normalize_openers(openers: &[OpenerRule]) -> Vec<OpenerRule> {
    let mut result: Vec<OpenerRule> = Vec::new();
    let mut targets: Vec<String> = Vec::new();

    for rule in openers {
        let target = normalize_opener_target(&rule.target);

        if let Some(pos) = targets.iter().position(|existing| existing == &target) {
            result[pos].tools.extend(rule.tools.iter().cloned());
        } else {
            targets.push(target.clone());
            result.push(OpenerRule {
                target,
                tools: rule.tools.clone(),
            });
        }
    }

    result.sort_by_key(|a| opener_specificity_order(&a.target));
    result
}

/// オープナールールの具体度順ソートキーを返す。
/// (1) パス付きフォルダ（パスが長い順）
/// (2) パスなしフォルダ
/// (3) パス付き拡張子（パスが長い順）
/// (4) パスなし拡張子
pub fn opener_specificity_order(target: &str) -> (u8, i64) {
    let path_cond = extract_path_condition(target);
    let is_folder = target == "folder" || target.starts_with("folder:");
    let path_len = path_cond.map_or(0i64, |p| p.len() as i64);

    if is_folder {
        if path_cond.is_some() {
            (0, -path_len)
        } else {
            (1, 0)
        }
    } else if path_cond.is_some() {
        (2, -path_len)
    } else {
        (3, 0)
    }
}

// --- Opener presets ---

/// A detected opener preset available for one-click addition.
pub struct OpenerPreset {
    pub name: &'static str,
    pub exe: String,
    pub args: &'static str,
    pub target: &'static str,
}

/// Search for `filename` in PATH directories. Returns the full path if found.
fn find_in_path(filename: &str) -> Option<String> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(';') {
        let candidate = Path::new(dir).join(filename);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// Detect opener presets available on this system.
/// Checks PATH and known install locations. Explorer is always included.
pub fn detect_opener_presets() -> Vec<OpenerPreset> {
    let mut presets = Vec::new();

    // VSCode: PATH 上の code.cmd から Code.exe を解決、または既知のインストールパス
    // code.cmd はバッチファイルなので直接実行するとコマンドプロンプトが残る。
    // code.cmd は通常 ...\Microsoft VS Code\bin\code.cmd にあるので、
    // 親(bin/)の親に Code.exe があるかを確認する。
    let vscode_exe = find_in_path("code.cmd")
        .and_then(|cmd_path| {
            let bin_dir = Path::new(&cmd_path).parent()?;
            let vscode_dir = bin_dir.parent()?;
            let exe = vscode_dir.join("Code.exe");
            if exe.is_file() {
                Some(exe.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .or_else(|| {
            let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
            let known = Path::new(&local_app_data)
                .join("Programs")
                .join("Microsoft VS Code")
                .join("Code.exe");
            if known.is_file() {
                Some(known.to_string_lossy().into_owned())
            } else {
                None
            }
        });
    if let Some(exe) = vscode_exe {
        presets.push(OpenerPreset {
            name: "Visual Studio Code",
            exe,
            args: "",
            target: "folder",
        });
    }

    // Windows Terminal: PATH 上の wt.exe
    if let Some(exe) = find_in_path("wt.exe") {
        presets.push(OpenerPreset {
            name: "Windows Terminal",
            exe,
            args: "-d {path}",
            target: "folder",
        });
    }

    // Explorer: 常に利用可能。find_in_path でフルパスを解決し、アイコン取得を確実にする
    let explorer_exe = find_in_path("explorer.exe")
        .unwrap_or_else(|| r"C:\Windows\explorer.exe".to_string());
    presets.push(OpenerPreset {
        name: "Explorer",
        exe: explorer_exe,
        args: "",
        target: "folder",
    });

    presets
}

/// Check if a preset's exe is already present in the opener rules (case-insensitive).
/// Compares by file name only so that bare names ("explorer.exe") and full paths
/// ("C:\Windows\explorer.exe") are treated as the same executable.
pub fn is_preset_already_added(openers: &[OpenerRule], preset_exe: &str) -> bool {
    let preset_name = Path::new(preset_exe)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    openers.iter().any(|rule| {
        rule.tools.iter().any(|tool| {
            let tool_name = Path::new(&tool.exe)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            tool_name == preset_name
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(target: &str, tools: &[(&str, &str, &str)]) -> OpenerRule {
        OpenerRule {
            target: target.to_string(),
            tools: tools
                .iter()
                .map(|(name, exe, args)| OpenerTool {
                    name: name.to_string(),
                    exe: exe.to_string(),
                    args: args.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn normalize_opener_target_adds_dot_and_sorts_extensions() {
        assert_eq!(
            normalize_opener_target("ext: png, .JPG, gif , png"),
            "ext:.gif,.jpg,.png"
        );
    }

    #[test]
    fn normalize_openers_merges_equivalent_targets() {
        let openers = vec![
            make_rule("ext:png,jpg", &[("Viewer 1", "viewer.exe", "")]),
            make_rule("ext:.jpg,.png", &[("Viewer 2", "viewer2.exe", "")]),
        ];

        let normalized = normalize_openers(&openers);

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].target, "ext:.jpg,.png");
        assert_eq!(normalized[0].tools.len(), 2);
        assert_eq!(normalized[0].tools[0].name, "Viewer 1");
        assert_eq!(normalized[0].tools[1].name, "Viewer 2");
    }

    #[test]
    fn normalize_openers_sorts_by_specificity() {
        let openers = vec![
            make_rule("ext:txt", &[("Notepad", "notepad.exe", "")]),
            make_rule("folder:c:\\workspace\\snotra", &[("Terminal", "wt.exe", "-d {path}")]),
            make_rule("ext:md:c:\\projects", &[("VSCode", "Code.exe", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
            make_rule("folder:c:\\workspace", &[("VSCode", "Code.exe", "")]),
            make_rule("ext:md", &[("Typora", "typora.exe", "")]),
        ];

        let normalized = normalize_openers(&openers);
        let targets: Vec<&str> = normalized.iter().map(|r| r.target.as_str()).collect();

        // (1) パス付きフォルダ（パスが長い順）→ (2) パスなしフォルダ → (3) パス付き拡張子 → (4) パスなし拡張子
        assert_eq!(targets, vec![
            "folder:c:\\workspace\\snotra",
            "folder:c:\\workspace",
            "folder",
            "ext:.md:c:\\projects",
            "ext:.txt",
            "ext:.md",
        ]);
    }

    #[test]
    fn opener_specificity_order_groups_correctly() {
        // パス付きフォルダ < パスなしフォルダ < パス付き拡張子 < パスなし拡張子
        assert!(opener_specificity_order("folder:c:\\a") < opener_specificity_order("folder"));
        assert!(opener_specificity_order("folder") < opener_specificity_order("ext:.txt:c:\\a"));
        assert!(opener_specificity_order("ext:.txt:c:\\a") < opener_specificity_order("ext:.txt"));

        // パスが長い方が先
        assert!(
            opener_specificity_order("folder:c:\\workspace\\snotra")
                < opener_specificity_order("folder:c:\\workspace")
        );
    }

    // ---- find_matching_tools tests ----

    #[test]
    fn find_matching_tools_folder_target() {
        let rules = vec![
            make_rule("folder", &[("TC", "TOTALCMD64.EXE", "/O /T")]),
            make_rule("ext:png,jpg", &[("IrfanView", "i_view64.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\Projects", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "TC");
    }

    #[test]
    fn find_matching_tools_ext_target_with_dot() {
        let rules = vec![make_rule("ext:.png,jpg", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\image.PNG", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "IrfanView");
    }

    #[test]
    fn find_matching_tools_ext_target_without_dot() {
        let rules = vec![make_rule("ext:png,jpg,gif", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\photo.jpg", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "IrfanView");
    }

    #[test]
    fn find_matching_tools_no_match_returns_empty() {
        let rules = vec![
            make_rule("folder", &[("TC", "TOTALCMD64.EXE", "")]),
            make_rule("ext:png,jpg", &[("IrfanView", "i_view64.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\doc.pdf", false, &rules);
        assert!(tools.is_empty());
    }

    #[test]
    fn find_matching_tools_file_does_not_match_folder_rule() {
        let rules = vec![make_rule("folder", &[("TC", "TOTALCMD64.EXE", "")])];
        let tools = find_matching_tools("C:\\file.exe", false, &rules);
        assert!(tools.is_empty());
    }

    #[test]
    fn find_matching_tools_folder_does_not_match_ext_rule() {
        let rules = vec![make_rule("ext:png", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\MyFolder", true, &rules);
        assert!(tools.is_empty());
    }

    #[test]
    fn find_matching_tools_multiple_rules_first_wins() {
        let rules = vec![
            make_rule("ext:png", &[("Tool1", "tool1.exe", "")]),
            make_rule("ext:png,jpg", &[("Tool2", "tool2.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\image.png", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Tool1");
    }

    #[test]
    fn find_matching_tools_multiple_tools_in_rule() {
        let rules = vec![make_rule(
            "folder",
            &[("TC", "TOTALCMD64.EXE", ""), ("Explorer", "explorer.exe", "")],
        )];
        let tools = find_matching_tools("C:\\Projects", true, &rules);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "TC");
        assert_eq!(tools[1].name, "Explorer");
    }

    #[test]
    fn find_matching_tools_case_insensitive_ext() {
        let rules = vec![make_rule("ext:PNG,JPG", &[("IrfanView", "i_view64.exe", "")])];
        let tools = find_matching_tools("C:\\Photo.png", false, &rules);
        assert_eq!(tools.len(), 1);
    }

    // ---- path condition tests ----

    #[test]
    fn find_matching_tools_folder_with_path_condition() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "Code.exe", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\workspace\\Snotra", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_folder_path_no_match_falls_back() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "Code.exe", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        let tools = find_matching_tools("D:\\other\\dir", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Explorer");
    }

    #[test]
    fn find_matching_tools_most_specific_path_wins() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "Code.exe", "")]),
            make_rule("folder:C:\\workspace\\Snotra", &[("Terminal", "wt.exe", "-d {path}")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\workspace\\Snotra\\src", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Terminal");
    }

    #[test]
    fn find_matching_tools_path_condition_case_insensitive() {
        let rules = vec![
            make_rule("folder:C:\\Workspace", &[("VSCode", "Code.exe", "")]),
        ];
        let tools = find_matching_tools("c:\\workspace\\project", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_ext_with_path_condition() {
        let rules = vec![
            make_rule("ext:md:C:\\projects", &[("VSCode", "Code.exe", "")]),
            make_rule("ext:md", &[("Typora", "typora.exe", "")]),
        ];
        let tools = find_matching_tools("C:\\projects\\readme.md", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_ext_path_no_match_falls_back() {
        let rules = vec![
            make_rule("ext:md:C:\\projects", &[("VSCode", "Code.exe", "")]),
            make_rule("ext:md", &[("Typora", "typora.exe", "")]),
        ];
        let tools = find_matching_tools("D:\\docs\\readme.md", false, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Typora");
    }

    #[test]
    fn find_matching_tools_same_specificity_first_wins() {
        let rules = vec![
            make_rule("folder:C:\\a", &[("Tool1", "tool1.exe", "")]),
            make_rule("folder:C:\\b", &[("Tool2", "tool2.exe", "")]),
        ];
        // C:\a のパス → Tool1
        let tools = find_matching_tools("C:\\a\\sub", true, &rules);
        assert_eq!(tools[0].name, "Tool1");
    }

    #[test]
    fn find_matching_tools_path_condition_slash_normalized() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "Code.exe", "")]),
        ];
        // パスにスラッシュが混在
        let tools = find_matching_tools("C:/workspace/project", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn find_matching_tools_path_condition_boundary_check() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "Code.exe", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        // "C:\workspaces" はパス境界で一致しないのでフォールバック
        let tools = find_matching_tools("C:\\workspaces\\project", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Explorer");
    }

    #[test]
    fn find_matching_tools_path_condition_exact_match() {
        let rules = vec![
            make_rule("folder:C:\\workspace", &[("VSCode", "Code.exe", "")]),
        ];
        // 完全一致もマッチする
        let tools = find_matching_tools("C:\\workspace", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");
    }

    #[test]
    fn normalize_opener_target_folder_with_path() {
        assert_eq!(
            normalize_opener_target("folder:C:\\workspace"),
            "folder:c:\\workspace"
        );
    }

    #[test]
    fn normalize_opener_target_folder_with_empty_path() {
        assert_eq!(normalize_opener_target("folder:"), "folder");
        assert_eq!(normalize_opener_target("folder:  "), "folder");
    }

    #[test]
    fn normalize_opener_target_ext_with_path() {
        assert_eq!(
            normalize_opener_target("ext:md:C:\\projects"),
            "ext:.md:c:\\projects"
        );
    }

    #[test]
    fn normalize_opener_target_ext_with_path_normalizes_exts() {
        assert_eq!(
            normalize_opener_target("ext: PNG, .jpg :C:\\projects"),
            "ext:.jpg,.png:c:\\projects"
        );
    }

    #[test]
    fn split_ext_and_path_no_path() {
        assert_eq!(split_ext_and_path("md,txt"), ("md,txt", None));
    }

    #[test]
    fn split_ext_and_path_with_drive_path() {
        assert_eq!(
            split_ext_and_path(".md:C:\\projects"),
            (".md", Some("C:\\projects"))
        );
    }

    #[test]
    fn split_ext_and_path_forward_slash() {
        assert_eq!(
            split_ext_and_path(".md:D:/repos"),
            (".md", Some("D:/repos"))
        );
    }

    #[test]
    fn detect_opener_presets_returns_at_least_explorer() {
        let presets = detect_opener_presets();
        assert!(
            presets.iter().any(|p| p.name == "Explorer"),
            "Explorer should always be present"
        );
    }

    #[test]
    fn detect_opener_presets_explorer_fields() {
        let presets = detect_opener_presets();
        let explorer = presets.iter().find(|p| p.name == "Explorer").unwrap();
        // exe はフルパスまたは bare name になるが、ファイル名部分は常に "explorer.exe"
        let file_name = Path::new(&explorer.exe)
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        assert_eq!(file_name, "explorer.exe");
        assert_eq!(explorer.args, "");
        assert_eq!(explorer.target, "folder");
    }

    #[test]
    fn find_in_path_returns_none_for_nonexistent() {
        assert!(find_in_path("__snotra_nonexistent_binary_xyz__").is_none());
    }

    #[test]
    fn find_matching_tools_path_condition_trailing_separator() {
        let rules = vec![
            make_rule("folder:C:\\workspace\\", &[("VSCode", "Code.exe", "")]),
            make_rule("folder", &[("Explorer", "explorer.exe", "")]),
        ];
        // 末尾 \ 付き条件は子孫パスにマッチする
        let tools = find_matching_tools("C:\\workspace\\Snotra\\src", true, &rules);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "VSCode");

        // 末尾 / 付き条件でも同様
        let rules2 = vec![
            make_rule("folder:C:\\workspace/", &[("VSCode", "Code.exe", "")]),
        ];
        let tools2 = find_matching_tools("C:\\workspace\\project", true, &rules2);
        assert_eq!(tools2.len(), 1);
        assert_eq!(tools2[0].name, "VSCode");
    }

    #[test]
    fn normalize_opener_target_folder_path_case_and_slash_normalized() {
        // 大文字小文字が正規化される
        assert_eq!(
            normalize_opener_target("folder:C:\\Workspace"),
            "folder:c:\\workspace"
        );
        // / → \ に正規化される
        assert_eq!(
            normalize_opener_target("folder:c:/workspace"),
            "folder:c:\\workspace"
        );
        // 末尾 \ が除去される
        assert_eq!(
            normalize_opener_target("folder:C:\\workspace\\"),
            "folder:c:\\workspace"
        );
        // ext のパス部分も正規化される
        assert_eq!(
            normalize_opener_target("ext:md:C:/Projects"),
            "ext:.md:c:\\projects"
        );
        // 区切り文字だけのパス条件は汎用ルールに畳み込まれる
        assert_eq!(normalize_opener_target("folder:\\"), "folder");
        assert_eq!(normalize_opener_target("folder:/"), "folder");
    }

    #[test]
    fn is_preset_already_added_case_insensitive() {
        let rules = vec![OpenerRule {
            target: "folder".to_string(),
            tools: vec![OpenerTool {
                name: "Explorer".to_string(),
                exe: "Explorer.EXE".to_string(),
                args: String::new(),
            }],
        }];
        assert!(is_preset_already_added(&rules, "explorer.exe"));
        assert!(!is_preset_already_added(&rules, "Code.exe"));
    }

    #[test]
    fn is_preset_already_added_fullpath_matches_bare_name() {
        // 設定に bare name で保存済みのユーザーが、フルパスのプリセットと照合できる
        let rules = vec![OpenerRule {
            target: "folder".to_string(),
            tools: vec![OpenerTool {
                name: "Explorer".to_string(),
                exe: "explorer.exe".to_string(),
                args: String::new(),
            }],
        }];
        assert!(is_preset_already_added(
            &rules,
            r"C:\Windows\explorer.exe"
        ));
        // 逆方向: 設定にフルパス、プリセットが bare name
        let rules_full = vec![OpenerRule {
            target: "folder".to_string(),
            tools: vec![OpenerTool {
                name: "Explorer".to_string(),
                exe: r"C:\Windows\explorer.exe".to_string(),
                args: String::new(),
            }],
        }];
        assert!(is_preset_already_added(&rules_full, "explorer.exe"));
    }
}
