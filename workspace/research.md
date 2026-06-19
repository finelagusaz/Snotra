# research.md — issue #357: Rust 変更時の必須検証を SSOT とスキルで揃える

## issue の要約

Rust ファイル変更時の必須検証コマンドが、エージェントの入口（参照するドキュメント）によって食い違っている。

- `docs/build-commands.md` カテゴリ A: 必須は `cargo check` のみ。clippy と `cargo test -p snotra-core` は「**追加検証**」（任意に読める）。
- `.claude/skills/implement/SKILL.md` Step 4: その「追加検証」を「**必須として扱う**」と明記。

→ `docs/build-commands.md` を SSOT として読むエージェントは、Rust 変更でも `cargo check` だけで完了扱いにしうる。純ロジック層 TDD 重視の運用とズレる。

**採用方針（ユーザー承認済み）: Option A** — SSOT 側で clippy・`cargo test -p snotra-core` を「必須」に昇格し、判断理由を明記。implement スキルの重複した「必須として扱う」記述は SSOT 参照に一本化する。

## 関連コード（ドキュメント・設定）

| ファイル | 行 | 役割 | 変更要否 |
|---|---|---|---|
| `docs/build-commands.md` | 11-18 | カテゴリ A 定義（SSOT 本体） | **変更**: clippy・core テストを必須に昇格、判断理由を明記 |
| `.claude/skills/implement/SKILL.md` | 49-52 | Step 4 検証。カテゴリ A 追加検証を「必須として扱う」 | **変更**: 入口固有のオーバーライドを削除し SSOT 参照に一本化 |
| `.claude/skills/deps-update/SKILL.md` | 39 | カテゴリ A を「必須＋追加検証」として全実行 | **軽微変更**: 「追加検証」語を SSOT 新表現に合わせる（挙動不変） |
| `.claude/settings.json` | 24, 28 | PostToolUse フック（実際の強制力） | 変更なし（既に必須を強制。SSOT を実態に合わせる側） |
| `.claude/skills/health-check/SKILL.md` | 89-102 | Check 10: 必須コマンド↔workflow 対応検証 | 変更なし（昇格後も `ci.yml` 対応あり＝ドリフトなし） |
| `AGENTS.md` | 62 | カテゴリ A〜D を参照（コマンド本体は書かない） | 変更なし |
| `docs/superpowers/**/2026-05-14-deps-update-*.md` | 60, 98 | 歴史的設計/計画記録 | 変更なし（決定時点のスナップショット。遡及編集しない） |

## 既存パターン（証拠: 実態は既に「必須」側）

エコシステム全体が clippy・core テストを既に必須ゲートとして運用しており、「追加検証＝任意」と読めるのは `docs/build-commands.md:17` 一箇所のみ（outlier）。

1. **PostToolUse フック（`.claude/settings.json`）= 実際の強制力**
   - 24行目: `.rs` 編集ごとに `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets ... -- -D warnings` を自動実行。
   - 28行目: `snotra-core.*\.rs` 編集時に `cargo test -p snotra-core --lib` を自動実行。
   - → **clippy は全 .rs／core テストは snotra-core 変更時**、という発火粒度が criterion 3 の判断理由そのもの。

2. **CI（`ci.yml` rust-check）= PR ゲート**
   - `build-commands.md:92` の対応表: `cargo check` / `cargo test -p snotra-core` / `cargo clippy` の三つすべてが PR 自動実行される。

3. **deps-update スキル**: カテゴリ A を「必須＋追加検証」として三つとも実行（既に必須扱い）。

→ SSOT の文言だけが取り残されている。Option A は「文書を実態に合わせる」昇格であり、新たな運用負荷を生まない。

## 技術的制約

- **`.claude/` 配下の編集はセキュリティ分類器に flag されうる**（メモリ [[feedback_subagent_config_commit.md]]）。サブエージェント経由でもメイン直接でも発生する。コミット時に注意。
- **エージェント設定（スキル）の変更は合意が必要**（CLAUDE.md チーム憲章）→ 方針 A はユーザー承認済み。
- Win32 / IPC / リアクティブ制約は該当なし（ドキュメント・設定のみの変更）。
- `.md` のみの変更のため、`docs/build-commands.md` カテゴリ A〜D の検証チェックリストはいずれも非該当（`.rs`/`.ts`/`.tsx` 変更なし）。検証は「記述の自己整合」の目視確認が中心。

## criterion 3 への回答（`cargo test -p snotra-core` の必須/任意の判断理由）

- **`snotra-core`（純ロジック層）を変更した場合 → 必須**。理由: 純ロジック層は TDD 重視（AGENTS.md 開発ワークフロー）。PostToolUse フックも snotra-core 編集時に core テストを自動発火する。
- **`snotra` / `snotra-settings` のみの変更 → ローカルでは任意**。理由: 純ロジックが変わらないため再実行の意味が薄い。ただし CI（rust-check）が PR で常に実行し担保する。
- **clippy は全 `.rs` 変更で必須**（crate を問わない）。理由: フックが全 .rs で発火、CI も常時実行。

## 未解決の疑問

なし。方針・粒度・影響範囲すべて確定済み。
