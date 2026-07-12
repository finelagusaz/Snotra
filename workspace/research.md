# research: issue #517 — worktree cleanup のスクリプト化

## issue の要約

Agent 委譲で作られる `.claude/worktrees/agent-*` は、ファイル変更があると自動削除されない（成果保全のためのハーネス仕様）。現在は AGENTS.md の規範として手動 cleanup 手順（`git worktree remove --force` + `git branch -D`）を記憶させている。これをスクリプト1コマンドに置き換え、規範を消す。

## 関連コード

- `scripts/` — 既存スクリプト置き場。全て PowerShell（`bench-startup.ps1` / `smoke-startup.ps1` 等）
- `package.json` — `smoke:startup` / `prepare:sidecar` が `pwsh -NoProfile -File scripts/<name>.ps1` で配線されている。同じ形で `clean:worktrees` を追加する
- `AGENTS.md` 「環境制約」節 1項目目 — 置き換え対象の規範（節ごと解体は #518 のスコープ。本 issue では当該項目の手順記述をコマンド名に置き換えるだけ）

## 既存パターン

- npm script + `pwsh -NoProfile -File` の配線パターンあり（再利用）
- worktree の掃除ロジック自体はリポジトリ内に前例なし（`commit-commands:clean_gone` スキルは gone ブランチ用で対象が違う）

## 技術的制約

- Win32 API 依存なし。git CLI のみ
- git のネイティブ安全機構を活かせる:
  - `git worktree remove`（`--force` なし）は未コミット変更があると拒否する
  - `git branch -d`（小文字）は未マージのブランチを拒否する
  - → 「保全すべきものを消さない」判定を自作せず git に委ねられる（recommend-native-over-handrolled）
- `.claude/settings.json` の PostToolUse hook は `scripts/*.ps1` に検査を割り当てていない → 沈黙は「何も走らなかった」。検証は手動手順で行う（受け入れ条件どおり）

## 未解決の疑問

なし。dirty worktree の扱いは issue が「スキップまたは確認」と指定済み → 既定スキップ + `-Force` オプトインを採る。
