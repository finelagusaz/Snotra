# skill-text レイヤー レビュー結果（`.claude/skills/implement/SKILL.md`）

## 問題なし

- **`:123` の免除列挙が内部から参照される唯一の名前参照である**: `grep -rn "最終検証結果\|コミットハッシュ" --include="*.md" .` を実行すると `.claude/` 配下では `SKILL.md:123`（免除列挙）と `SKILL.md:127-128`（出力項目本体）の 2 箇所のみがヒットする。`docs/superpowers/plans/*.md` にも同語がヒットするが、これらは日付名が付いた過去の設計・実行ログであり `research.md`「序数参照の巻き込みは無い」の grep 対象（`.claude/`/`AGENTS.md`/`CLAUDE.md`/`docs/` から `superpowers/` を除く）から意図的に除外されている。plan.md の「免除列挙 1 件だけを挙げている——それで全部か」への答えは**それで全部**（`workspace/plan.md:15,24`）。
- **項目 3 の母集団は SKILL.md 自己完結で閉じる**: `### 4a. check スキルの実行`（`SKILL.md:102-104`）は本文中に `/symmetric-check`・`/dry-check`・`/race-check`・`/cache-check`・`/persistence-check`・`/state-check` の 6 スキルを**名前で直接列挙**しており、`AGENTS.md`「条件別チェック」表を経由せずとも母集団を閉じられる。不変条件 1 が言う「4a が名指しするスキル集合」は実在する具体的な参照先であり、AGENTS.md 表の非スキル行（rules 参照・grep 指示等）を混入させる読み違いのリスク（合格条件 4）を、population の定義そのものが先回りして回避している。
- **Step 3 本文「該当するカテゴリ A〜F をすべて実行する」（`SKILL.md:88`）と項目 2 の三値化は矛盾しない**: Step 3 は実行義務の記述、項目 2 は実行結果の報告形式であり、層が異なる。カテゴリ D は Step 3 自身が「エージェントが実行できない」と明記しており、報告側の三値「実行不能」はこの D を素直に受ける。
- **`.claude/skills/**` に PostToolUse 検査が無いことを実測で確認**: `.claude/hooks/post-edit.mjs` の `selectChecks`（118-143行）は `.rs`／Cargo manifest／`tauri.conf.json`・`config.toml`／`CHECK_DEFINITION`（`.claude/settings.json`・`package.json`・`vitest.config.ts`・`Cargo.toml`）／`.claude/hooks/`／`.githooks/` のみを対象にしており、`.claude/skills/**` を拾う分岐が無い。plan.md「テスト方針」の前提は正しい。
- **AREA_BUDGET（G10）が skills 本文を対象外とすることも実測で確認**: `scripts/governance-check.mjs:536` のコメント「対象外は意図的である: skills 本文…」と `AREA_BUDGET = { alwaysLoaded: 13374, rules: 8056 }`（`:604`）の算入対象が `description` のみであることから、plan.md の該当記述（`workspace/research.md:41`）は妥当。
- **SPEC.md 更新不要の判断は妥当**: `grep -n "implement\|Step 4\|code-reviewer" SPEC.md` は 0 件。SPEC.md はエージェント運用規範を一切扱っておらず、「不要」の判断に矛盾する記載は無い。
- **見出し文字列を触らない前提は正しい**: `CLAUDE.md:74` は `` `/implement`「4b. code-reviewer エージェント」 `` を正準見出し形で参照し、`docs/adr/0004-canonical-heading-references.md:28` も同見出しを G11 照合対象と明記している。plan.md は `### 4a.`/`### 4b.`/`## 出力` の見出し文字列自体を変更対象に含めておらず、この参照は壊れない。
- **issue #765 の指摘との整合**: issue 本文が明示する「`/symmetric-check` を実行、発見なし」が `:19` の実況制限と衝突し「正直に書いた側が罰される」問題は、plan.md 不変条件 3 が正確に踏襲している。issue 後半の「check, clippy, test の 3 種」指摘が既に `#764`（`880be34`）で解消済みという research.md の前提是正も、`git log -S"check, clippy, test"` の 2 件（`39fe33e` 導入・`880be34` 除去）で裏付けられる。

## 軽微な懸念

- **項目 3 の記述粒度（1 件 1 行 vs 6 件全列挙）は Phase 1 のチェックリストだけでは確定しておらず、Phase 2 の `/norm-review` 判定に委ねられている**（`workspace/research.md:48`「未解決の疑問」／`workspace/plan.md:21-22`）。これ自体は plan が意図的に選んだ設計（停止条件を先に決め、2 巡以内で判定）であり手順としては妥当だが、Phase 1 単独を読むと「該当なしは 1 行にまとめて列挙」という指示が曖昧語（「まとめて」の具体形）を残したまま実装に入りうる。`/norm-review` 巡で文言が収束しなければ Phase 3 検証前に人間確認が要る可能性がある。
- **`:19` の従属節をどこに/どの粒度で足すかが plan.md 上でテキスト未確定**（「1 つの従属節で足す」との方針のみ、具体文言はレビュー時点で書かれていない）。方針は明確だが、`/norm-review` の合格条件 2（「発見があったときだけ書けばよい」と読ませない）を満たす具体文言は実装時の裁量に委ねられており、この時点では成立を検証できない。

## 要対処

（無し）

## 未検証（理由）

- **`/norm-review` 実行そのものの成否**: Phase 2 は計画段階でまだ実行されていない（`workspace/plan.md` のチェックリストは全て未チェック）。本レビューは Phase 1 で書き換えられる**予定の文言**が Phase 2 の合格条件 1〜4 を実際に通すかどうかまでは検証できない——それは `/norm-review` 自身が担う工程であり、計画レビューの時点では文言が未確定のため先回りして判定できない。
- **項目 3 の「発見件数・修正の有無」というテンプレートが `code-reviewer` エージェント（4b）の実際の出力形式と噛み合うか**: `.claude/agents/` 配下の `code-reviewer` 定義は本レビューの担当レイヤー外（`skill-text` は `.claude/skills/implement/SKILL.md` の文言のみを担当）のため未確認。4a の check スキル群（`/symmetric-check` 等）の出力形式との整合も同様に未確認。他レイヤーのスカウトが担当していれば重複を避けるためここでは扱わない。
