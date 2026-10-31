# plan-review 独立導出 — issue #1330（ルート CLAUDE.md / AGENTS.md の縮小）

対象: issue #1330。導出者は `workspace/` の計画・草案・classify・adversarial を**読んでいない**。
測った木: ブランチ `chore/root-norm-slim`（HEAD a400ffc3・作業ツリーは `workspace/` 以外 clean）。
ベースライン: `npm run governance:check` → 全検査 passed（17 件）。

注意: セッション開始時に注入された CLAUDE.md の写しは古い（`ca30b1fd` の heredoc 行＝現行 CLAUDE.md:23 を持たない）。以下の行番号は作業ツリーの実ファイルで取った。

---

## 導出した変更ファイル（4 の要約を先に置く）

| ファイル | 理由 | 必須度 |
|---|---|---|
| `CLAUDE.md` | 本体 | 必須 |
| `AGENTS.md` | 本体 | 必須 |
| `.claude/skills/start-issue/SKILL.md:70` | 削る「画像はサブエージェントに見えない」の出典としてルート `CLAUDE.md`「サブエージェント委譲と worktree」を名指ししている。削ると出典が中身を失う（この行自身が規則の本文を持つので、括弧の出典を外すだけでよい） | 必須 |
| `docs/development-principles.md:93` | **現行で既に偽**: 「常時ロード面自身と `docs/hooks.md` の役割は同節に行が無い」——`AGENTS.md`:24・:25 に両方の行が在る。ドキュメント参照の節を縮めるなら同時に整合させる | 必須（既存の偽） |
| `docs/hooks.md:5` | 「フックにどう応答するか・**沈黙をどう読むか**は `CLAUDE.md`「フック」節が SSOT」——現行のフック節に「沈黙」の条項は無い（PostToolUse 撤去で消えたと推定・⚠）。フック節を縮めるなら SSOT 宣言の中身を合わせる | 必須（既存の偽 ⚠） |
| `rust-toolchain.toml:10` | **現行で既に未着地（中身）**: `AGENTS.md`「条件別チェック（トリガー → 参照先）」の「ガバナンス機構の行」の「気づく契機が残るなら、機構ではなく引き金で足りる」を引くが、その行は #1256（190a2798）で消えている。見出しは着地するので governance は緑 | 推奨（既存の偽） |
| `.claude/skills/retrospective/SKILL.md:59` | 「`AGENTS.md` 冒頭の組織原理」＝ AGENTS.md:5（「注意事項は『トリガー』に括り付けてある」）。:5 を消すなら行き先を失う | 条件付き |
| `.claude/skills/retrospective/SKILL.md:66` | Claude Code 固有（**コミュニケーション原則・スキル表・委譲/worktree 運用**）→ ルート `CLAUDE.md`。節名・節の統廃合をするなら列挙を合わせる | 条件付き |
| `.claude/skills/retrospective/SKILL.md:69` | 「常時ロード面に置いてよいのは『トリガー＋参照＋**根拠 issue 番号**』まで」——issue 番号の括弧を外す今回の方針と規範がぶつかる（許可であって義務ではないので違反ではないが、次の `/retrospective` が番号を戻す読み方を許す）。正本側の書き換え候補 ⚠ | 推奨 |
| `.claude/skills/state-check/SKILL.md:30` | 散文形 `AGENTS.md「初回フローとガードの相互作用」`（バッククォート無し＝governance 射程外）。指す先は見出しでなく表のセル（AGENTS.md:59 の括弧）。括弧を縮めると沈黙で行き先を失う。規則の本文は state-check 自身が持つので、出典を外すか正準形の見出し（`AGENTS.md`「条件別チェック」）へ | 条件付き |
| `src-tauri/src/egui_shell/window_coordinator.rs:1203`・`src-tauri/src/startup.rs:3`・`scripts/occupy-hotkey.ps1:26`・`PERFORMANCE.md:39`・`snotra-core/tests/match_cost.rs:14`・`src-tauri/src/egui_shell/window_coordinator.rs:960`・`docs/comment-guidelines.md:23`・`.claude/skills/plan-review/SKILL.md:25` | 見出しは正準形で着地するが、**特定の行の中身**（「重複した読み」行・足場の行・写しの行・「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）に依存する。行を消す・縮めるなら直す（下の 2-B 表） | 条件付き |
| `.claude/skills/implement/SKILL.md:84`・`:112`、`docs/development-principles.md:259`、`docs/design/2026-08-20-governance-meta-demotion-derivations.md:10` | ルート `CLAUDE.md`「サブエージェント委譲と worktree」の**特定の箇条**（検査対象を変更しながら走らせない／意図的に分けた構造を渡す／成果物を指定パスへ書かせる／所見であって機序ではない）に依存 | 条件付き |
| 各 `.rs` / `.ps1` / `PERFORMANCE.md` / `RETROSPECTIVE.md` / `docs/build-commands.md:119` / health-check | `AGENTS.md`「検証の作法」の特定の箇条を引用（下の 2-B 表） | 条件付き |
| `.claude/skills/deps-update/SKILL.md:26` | `CLAUDE.md` の「`main` 直コミット禁止」——見出しの字面と一致しない言い換え（現行で既に正準形でない）。最重要ルールの字面を変えるなら合わせる | 任意 |
| `src-tauri/src/egui_shell/window_coordinator.rs:776` | 「`/persistence-check` の 4 点セットが要る」——persistence-check に「4 点セット」は無い（Step 1〜6・典型パターン 5 つ）。AGENTS.md:61 の偽と同じ系の残骸 ⚠。範囲外とするなら別 issue | 任意（⚠） |

ADR（`docs/adr/`）と `docs/superpowers/` の参照は凍結された歴史・走査元の外なので、直す対象に含めない（未着地の正準形が既に在る。種類は 2-A の末尾）。

---

## 1. 2 枚の規則の全列挙と判定

凡例: **残す**＝判断の瞬間に要り、正本も機構も無い（消すと失われる）／**D**＝正本が他に在る（生きた文書だけを数えた。ADR は凍結された歴史ゆえ正本に数えない）／**M**＝機構が守る（実在を確かめた機構名）／**S**＝システムプロンプト（Agent ツール説明・Bash ツール説明）と重なる／**X**＝経緯・説明のみ／**裁定**＝issue のユーザー裁定で削る。

### ルート CLAUDE.md

| 行 | 規則 | 判定 | 根拠 |
|---|---|---|---|
| 3 | 前文（AGENTS.md は次行で自動読込・モジュール不変条件は各 CLAUDE.md） | 索引 | `AGENTS.md`「ドキュメント参照」:20 と重なる（D）。`@AGENTS.md` の説明部分は X |
| 9 | main へ直接コミット・プッシュしない／feature ブランチ名 `feat/`・`fix/`・`chore/` | **M**＋残す（短く） | `.githooks/pre-commit`・`pre-merge-commit`・`pre-rebase`・`pre-push`（`_lib.sh`:18 の拒否文言がブランチ接頭辞まで案内）＋ GitHub ruleset。ただし `cherry-pick` 等は hook を通らない（:15）ので最重要ルールとして 1 行は残す価値がある。`deps-update/SKILL.md:26` がこの規則を名指す |
| 10 | セーフティネット（規範文書を含む）の変更は合意してから。母集団は AGENTS の表のセーフティネット行 | **残す** | 合意要件の正本は他に無い（`.claude/rules/safety-nets.md` は検証手順だけで合意を言わない。grep 実測）。**母集団を AGENTS.md:67 に預けているので、:67 の「規範」を残すこと** |
| 14 | main 保護の実体は `.githooks/` と ruleset | M（説明） | `.githooks/_lib.sh`:3-10・`githooks.test.mjs` |
| 15 | Layer 1 は best-effort。checkout fail-open。`cherry-pick`/`revert`/`am`/`branch -f`/`update-ref` は pre-commit を呼ばず沈黙で main が進む。取りこぼしは ruleset。不在検知は意図的に置かない | 半分 D | 「best-effort」「不在検知を意図的に作らない」の正本は `.githooks/_lib.sh`:8-10。**`cherry-pick` 等 5 操作の列挙はこの行にしか無い**（grep 0 件）。消すと失われる事実だが、最終防衛線（ruleset）が在るので失われるのは「main で cherry-pick してはいけない」という先回りだけ ⚠ |
| 16 | squash マージは `/merge-pr` の道具で行う／`gh pr merge --auto` を使わない | D＋残す（1 行） | 正本 `.claude/skills/merge-pr/SKILL.md`:18・:49（`--auto`）・:62。道具 `scripts/merge-pr.mjs`:15・:30 は `--auto` を受け付けない（M）。ただし merge-pr は `disable-model-invocation: true` で roster に載らず、頼まれた時しか読まれない。「手でマージしない」を常時ロードに 1 行残すかは判断 ⚠ |
| 20 | PreToolUse フックがコマンドの形で拒む／**検証は AGENTS のワークフロー手順 6 で手で行う**／main 保護の実体はフックではない | X＋**偽**（3 を見よ） | |
| 22 | 拒否メッセージが復帰手順を持つ。それに従う | **残す** | 短い行動形。機構（拒否文言）を読む姿勢の規則で、他に正本無し |
| 23 | Windows の Bash では複数行テキストは Write ツールで書く。harness が勧めてもこの行が優先（**試行・#1324**） | **残す（試行の印も残す）** | `usesHeredoc`（M）が拒むが、ADR-command-shape-norms-in-hook の 2026-10-31 追記が「選ぶ前に目に入るのは常時ロード面だけ」として意図的に戻した**期限つきの試行**（2026-11-14 ごろ再計数）。**括弧の「試行・#1324」は経緯ではなく撤去条件の在り処**——外すとこの行を外す合図が常時ロード面から消える。本導出中にも heredoc を打って実際に拒まれた |
| 24 | `gh pr create` のゲート説明／`git push -u origin HEAD` を先に打つか `&&`／鎖に `cd` を含めない | **裁定で削る**・M | `pre-bash.mjs` の `REMEDY`（:82）が push の 2 形を、`CWD_CHANGE`（:80・:331-333）が cd を拒み文言を出す。判定の正本は `judgeSummary`（:433）・`readVerifyState`（:453）・plan.md ゲート。失われるのは先回りだけ |
| 25 | hook が守る不可逆呼び出しは `gh pr create` だけ。道具を経ないマージは人間専用の迂回 | D | `merge-pr/SKILL.md`:62 が同文。`docs/hooks.md` も参照。ただし前述のとおり merge-pr は頼まれた時しか読まれない ⚠ |
| 26 | 文書の決定的整合は PR CI の `governance-check` job（`skip-ci` 非対象） | M | `.github/workflows/ci.yml`:47-66 |
| 27 | フックを改修するときは `docs/hooks.md` を読む | **残す** | hook 編集時に自動配送される `.claude/rules/safety-nets.md` も、`pre-bash.mjs` 自身も `docs/hooks.md` を名指ししない（両方 grep 0 件。`lsp-config.mjs` だけが別節を指す）。消すと改修時に hooks.md へ辿り着く経路が無くなる |
| 31 | 意図は明確に、指示は短くていい | **裁定で削る** | 人間側への約束で、エージェントの行動を変えない |
| 32 | 複雑さ・意図不明に気づいたら声を上げる | 残す（畳む） | 他に正本無し |
| 33 | 「やりすぎでは」を歓迎（削る方向の指摘） | 残す（畳む） | 他に正本無し |
| 34 | 気づいたその場で記録へ書く | 残す（畳む） | 他に正本無し（`/retrospective` は事後の抽出） |
| 38 | 並列委譲はファイル境界で衝突を予測／リベース解決は実装エージェント本人に | **残す** | 正本無し（grep 0 件） |
| 39 | 委譲はコンテキストを継承しない／メインにしか無い事実（メモリ領域の絶対パス等）は渡す／issue 添付の画像はサブエージェントに見えない | **S**＋**裁定で削る（画像）** | Agent ツール説明（「知るのはプロンプトに書いたことだけ」）と重なる。**「メモリ領域の絶対パス」という具体例は S に無い**——ただし `/health-check`:38 が自前で持つ（D）。画像の件は `start-issue/SKILL.md`:70 が本文を持つ（D・スキル経由でのみ） |
| 40 | 検査対象を変更しながら検査を走らせない（全枠の報告が出揃うまで触らない） | **残す** | `implement/SKILL.md`:84 がこの箇条を根拠として引く（2-B）。正本は他に無い（`lib.mjs`:261・:321 は検査器の母集団の話で別概念） |
| 41 | 委譲した検査の成果物は指定パスへ書かせる／`report*.md` にしない | **残す**（`report*.md` は D） | `start-issue/SKILL.md`:68 が `report*.md` を持つ（D）。`development-principles.md`:259 がこの箇条を対処として引く。⚠ 実際のハーネスの制約（サブエージェントのシステムプロンプト）は「report/summary/findings/analysis の .md を Write するな」であり、`report*.md` より広い。この行は制約を狭く書いている ⚠ |
| 42 | 長時間の委譲は中断前提。Phase 分割・各 Phase の green 後にコミット／検査なら「測る前に今ある分を書け」 | **残す** | 正本無し（grep 0 件）。`development-principles.md`:259 の対処の候補 |
| 43 | レビューの委譲では意図的に分けた構造を根拠つきで先に渡す | **残す**（D 一部） | `implement/SKILL.md`:112 が同じことを言い、出典として**この箇条を引く**。`/implement` を経ない委譲には正本が無い |
| 44 | 指摘 0 件の報告は対象（ブランチ・sha・件数）を突き合わせてから受け取る | **残す** | 正本無し |
| 45 | ⚠️ も返り値に含めさせる／前段の裁定は争点の形で渡す／差分が消した行の不変条件を探す逆向きの監査を 1 枠 | **残す**（⚠️ だけ D） | ⚠️ は `implement`:113・`start-issue`:67 にも在る。**「争点の形で渡す」「逆向きの監査を 1 枠」は他に無い**（grep 0 件） |
| 46 | 採るのは所見であって機序の説明ではない／直した版を実データへ当ててから対処を決める | **残す**（前半 D 一部） | `start-issue/SKILL.md`:72 が前半を持つ。`governance-meta-demotion-derivations.md`:10 がこの箇条を引く。後半（実データへ当てる）は他に無い |
| 50 | 実行にバイアス／コミット・PR 作成は即実行（main 直は除く）／不明点は焦点を絞った 1 問 | **S** | システムプロンプトと重なる（issue 本文の分類どおり）。括弧の「main へ直接コミットしない」は :9 の写し |
| 51 | 計画書・設計書を提示された実装は忠実に（省略・統合・削除は明示時のみ） | **残す** | 正本無し（grep「忠実」は無関係の 4 件のみ） |
| 52 | 意図的なリファクタリングを戻さない／意図は導入コミット・`docs/adr/`・`//!` で確かめる／見つからない≠意図的でない→ユーザーに確認 | **残す** | 正本無し |
| 53 | 分析・調査・助言は報告のみ／変更の意図と問いが同居する依頼ではこの条項が優先 | **残す**（優先の相手を直す） | 優先の相手が :50（削られる）を指す。issue のとおりシステムプロンプト側の「動けるなら動く」へ替える |
| 57 | トリガーの SSOT は AGENTS の表／引数は `argument-hint`／表が索引するのは `disable-model-invocation: true` だけ（残りは roster が注入） | 残す（索引の前提） | **真**: `disable-model-invocation: true` は deps-update・health-check・merge-pr・retrospective の 4 本で表と一致（grep 実測）。この導出エージェントに注入された roster にも 4 本は無い |
| 59-64 | user 起動専用スキル表 | **残す** | roster に載らないので、ここが唯一の常時の入口 |
| （新） | user 起動専用スキルは Skill ツールから呼べない。頼まれたら `SKILL.md` を読んで手順を辿る | 裁定で足す | 文言に「4 本」と数を書かない方がよい（`AGENTS.md`「検証の作法」の数え上げの箇条。表と frontmatter が正本） |
| 66 | `code-reviewer` は `/implement` 3b が worktree で起動・コミット済み sha が対象・変異注入も同じ委譲・A〜F は `verify:tree` | **裁定で削る**／一部偽 | 3 を見よ |

### AGENTS.md

| 行 | 規則 | 判定 | 根拠 |
|---|---|---|---|
| 3 | 前文（*.md で意図・コードで事実／Claude 固有は CLAUDE.md／実装前チェックは rules） | X（索引） | :9 と :13-25 と重なる |
| 5 | 注意事項はトリガーに括り付けてある（rules と表が示す） | X（組織原理の宣言） | **`retrospective/SKILL.md`:59 が「`AGENTS.md` 冒頭の組織原理」として指す**。消すなら :59 を直す |
| 9 | 意図は SPEC と本ファイル、事実はコード。挙動を変える変更では同じ変更で整合 | **残す** | `implement/SKILL.md`:78 が `AGENTS.md`「3層分担」を引く（見出しを残す） |
| 13 | 末尾の一句は書式の索引。判定の正本は dev-principles「規範の書式は文書の役割から選ぶ」 | D | dev-principles :74-91 の表が同じ役割→書式を持つ（**写し**）。各行末尾の「— 語彙層で書く」等も同表の写し（SPEC の「語彙層」だけは同表に無い ⚠） |
| 15-25 | ドキュメント参照（各文書の所在） | 残す（索引） | `CLAUDE.md`:3・`development-principles.md`:93 が見出しを引く。パスの実在は `G-references`（M）。:20 のモジュール CLAUDE.md 4 枚は実在を確認 |
| 22 | rules は `A のときは B する。C が起きたため（#NNN 実測）` | D | dev-principles :85 |
| 23 | ADR は否定の知識が生じた決定のみ（#593）・連番を振らない（#812） | **括弧の中に規則** | 連番は `.claude/rules/governance-docs.md`:15・:17（D）＋ `G-adr-file-names`（M）。**「否定の知識が生じた決定のみ」は括弧の中にしか無い**——`implement/SKILL.md`:141 は「workspace 削除前に否定の知識を ADR へ回収」で近いが「のみ」（作らない側）は言わない |
| 29 | サイクル開始前に RETROSPECTIVE.md を読む | 残す（D 一部） | `start-issue/SKILL.md`:45 が RETROSPECTIVE.md を読ませる。`/start-issue` を経ない作業には正本が無い |
| 31-35 | バグか仕様変更かの判定（SPEC に記述があるか・合わせるか変えるか／fix でも文書化された挙動を変えたら仕様変更） | **残す** | `code-reviewer.md`:85・`state-check`:90・`comment-guidelines.md`:178 が `AGENTS.md`「バグか仕様変更かを判定する」を引く。**番号付き項目「1. 「バグ」か「仕様変更」かを判定する」がアンカー**（normAnchor が「」を落とすので一致）。番号付き項目をやめるなら太字リードに替えて着地を保つ |
| 36 | 要件をテスト可能な形で確定 | 残す（短い） | 他に正本無し（`start-issue`:115 は境界条件のみ） |
| 38 | マージ状態の確認は `git log` でなく `git diff` | **残す** | 正本無し（grep 0 件） |
| 39 | バグ発見時は同一パターン全コードパス検索 | D＋**見出しを残す** | 完了ゲート版の正本は dev-principles「デバッグ・バグ修正」。ただし **`development-principles.md`:246 が `AGENTS.md`「バグ発見時は同一パターン全コードパス検索を行う」を正準形で引く**（太字リードがアンカー）。消すと governance が赤 |
| 40 | 「変更なし」と判断するときは影響ケースを列挙 | D | dev-principles「列挙の完全性」 |
| 41・46 | 条件別チェック表／rules へ振り分ける | X（索引） | |
| 43 | 挙動変更なし前提は代表入出力をベースライン化して差分検証 | **残す** | **`.claude/rules/snotra-core.md`:25 が `AGENTS.md`「事前調査」を引いてこの中身に依存**。他に正本無し（grep 0 件） |
| 44 | 境界条件を列挙し 1 件ずつ検証ケース | D 一部 | `start-issue/SKILL.md`:115 |
| 45 | 生成/破棄・真偽のペアで計画（場所・構造・理由まで） | D | `.claude/rules/src-tauri.md`:22（Rust の具体則）・`/symmetric-check`。**ただし `src-tauri.md`:22 は「一般則は `AGENTS.md`「事前調査」」と逆にこちらを正本として引く**——双方が相手を正本と呼ぶ形。消すなら src-tauri.md:22 を自立させる |
| 47 | テスト先行（Red→Green） | 残す（短い） | 他に正本無し |
| 48-50 | 変更後の検証（カテゴリ A〜F）・コマンド文字列はこのファイルに書かない | D＋見出しを残す | 正本 `docs/build-commands.md`「変更後の検証チェックリスト」。`health-check/SKILL.md`:30 が `AGENTS.md`「変更後の検証を実行する」を引き、**コマンド本体の直書きを grep する検査の対象としてこの節を名指す**。**括弧の「コマンド文字列はこのファイルに書かない」は括弧の中の規則** |
| 58 | 対称ペア → `/symmetric-check` | D（スキル description） | roster が description を注入。ただし表が「トリガー→検査の SSOT」と各所（`implement`:94・`start-issue`:120・`plan-review`:25・`rules/src-tauri.md`:27・`rules/snotra-settings.md`:20・`code-reviewer.md`:15・:52）から宣言されている——**表の行を消すと、その宣言が「SSOT は表」と言いながら表に行が無い状態になる** |
| 59 | UI モード・ガード → `/state-check`（初回フローとガードの相互作用を含む） | D | `state-check/SKILL.md`:30 が本文を持つ。同行は散文形でこの括弧を指し返す |
| 60 | 重複した読み・冗長な状態を束ねる前に「後で読まれる/立つ」依存を 1 行ずつ書き出す | **残す** | 正本無し（grep 0 件）。`window_coordinator.rs`:1203 が「`AGENTS.md`「条件別チェック」の『重複した読み』」として引く |
| 61 | 永続形式 → `/persistence-check`。history キーは `snotra-core/CLAUDE.md`、config キーは「/plan-review の 4点セット」 | **偽**（3） | |
| 62 | 関数・型の新規定義/改名：findReferences で列挙（LSP 無しのときだけ grep）＋ `/dry-check`／旧 API 削除は下流 compile-fail を移行漏れ検出器に／新 API 導入と呼び出し点の移行を 1 タスクに束ねる | **残す** | 正本無し。**`plan-review/SKILL.md`:25 が「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」をこの表の既存トリガーとして引く**。括弧の中に規則（「LSP の無い環境でのみ grep へ落とす」）。dev-principles:54 は compile-fail の盲点を言うが本則ではない |
| 63 | 並行境界 → `/race-check` | D（description） | `rules/src-tauri.md`:27・`rules/snotra-settings.md`:20 が「**述語の全文は常時ロードの `AGENTS.md`「条件別チェック」表が SSOT**——ここに写しは置かない」と宣言。**この行の述語を縮めると、2 つの rule が SSOT と名指す全文が消える** |
| 64 | 網羅性が要件 → 母集団を誰が知っているか＋ Step 2b | D | dev-principles :22・:230 |
| 65 | 検査手段の新設 → 層と層の隙間 | D | dev-principles :21・:181 |
| 66 | 足場：新設時は撤去条件を成果物自身の doc へ／撤去時は引用されうる値が PERFORMANCE.md へ出所つきで着地しているか／実 config・実索引で測るなら生ログをリポジトリの外へ／コミットしてよいのは派生表だけ | **残す**（4 規則） | 撤去時の規則は `PERFORMANCE.md`:39 が「同じ引き金を持つ」と言い D 一部。**「生ログをリポジトリの外へ」「派生表だけ」は他に無い**（grep 0 件）。`startup.rs`:3・`occupy-hotkey.ps1`:26・`window_coordinator.rs`:960・`match_cost.rs`:14・`PERFORMANCE.md`:39 がこの行の中身に依存 |
| 67 | セーフティネット → `safety-nets.md`（**規範文書は自動配送されないので手動で読む**）／足す前に「壊れたとき緑のまま推移するか」 | **残す** | **CLAUDE.md:10 がセーフティネットの母集団の正本としてこの行を指す**——「規範」を外せない。括弧の規則は `safety-nets.md`:15 にも在るが、safety-nets.md は規範文書の編集では配送されない（`paths` に AGENTS.md/CLAUDE.md が無い。配送されるのは `governance-docs.md`）——**常時ロードのここが唯一の到達経路** |
| 68 | 撤去するときは語彙を `git grep` で数え上げ、散文の残存を直す。識別子の残存 0 件を根拠にしない | **残す** | dev-principles「撤去（消す変更）の作法」（:46-）は「最終 grep」「所在の散文はシンボル grep で届かない」（:67）まで言うが、「削除したファイル名と持ち込んだ語彙を数え上げ、描写／前提へ振り分ける」手順は無い（実読）。D 一部 |
| 69 | ガバナンス文書 → `governance:check` | M | CI governance-check job |
| 70 | 写しを増やす変更 → 正本 1 か所／PR 本文も母集団 | **残す** | `governance-docs.md`:13 の「書く約束 (1)」と D 一部。「PR 本文も数え上げの母集団」は他に無い。`comment-guidelines.md`:23 が「写しの行が覆う」と依存 |
| 71 | `.rs` 追加/削除 → `//!` と CLAUDE.md のモジュール構成／`mod` 宣言の書き忘れは cargo も RA も報せない | 前半 M・後半**裁定で削る** | `G-module-index`・`G-module-linkage`（実在）。`implement/SKILL.md`:77 が前半を持つ（D） |
| 72 | 機能削除・trace 名・hotkey → `smoke-egui.ps1` と build-commands の smoke 前提を確認（WebView2 e2e は撤去済み） | 残す／括弧は X | smoke 前提の正本は `docs/build-commands.md`:253-260 |
| 73 | 分岐を決める値の出所の変更 → diff に現れない下流を 1 段辿る・呼び忘れ変異で落ちることまで | **残す** | 正本無し（grep 0 件） |
| 74 | 件数 N・上限・導出の入力 → 下流全段・永続化する消費者 | **残す** | 正本無し（grep 0 件） |
| 75 | fix-forward → 枠組みを修正差分にも再実行／解消判定は自分で測る／単位を決めたら外に残るものを 1 つ挙げる | **残す**（3 規則） | 正本無し（grep 0 件）。括弧・#984・#999 は例示で、規則は括弧の外に在る |
| 76 | アーキ・原則に影響 → 対応 docs を触る範囲へ | 残す | |
| 77 | 言語ファイル → rules | X（索引） | |
| 78 | 委譲・worktree → ルート CLAUDE.md の委譲節 | 残す（索引） | 正準形 1 件 |
| 84 | 全称表現は「何が増えたら偽になるか」を 1 つ挙げる | **残す** | `health-check`:47・`RETROSPECTIVE.md`:23 が引く |
| 85 | 全称否定は探し方を変えて所在を確かめる（**which が空・grep 0 件は確定に足りない** は括弧の中） | **残す**（括弧の中に規則） | |
| 86 | 数え上げは腐る→正本を指す | 残す | |
| 87 | 代理条件も同型→述語の逐語か関数を指す | 残す | 括弧（#1201）は X |
| 88 | 自分が引用した規範ほど破りやすい | 残す ⚠ | 中身は全部括弧の例（#977・#992）。括弧を外すと 1 句だけ残る |
| 89 | 保証を宣言する文の隣に保証されない数・一覧を置かない／直し方は数を直すことでなく正本を指す | 残す | |
| 90 | 照合は SSOT に対して／派生コピー同士の一致を証拠にしない／列挙も SSOT のツールに問う（**glob の意味論はツールごとに違う**＝括弧）／派生コピーには今作った表も含む | **残す** | `layout.rs`:9・`health-check`:94 が引く |
| 91 | 消す/共通化する前に「片方だけが変わる将来」を挙げる | **残す** | `win_registry.rs`:7 が引く |
| 92 | 判定ロジック・fixture・母集団の導出コマンドは代表入力で実行して測る／サブエージェントの実測も一次証拠にしない／検証コマンドは観測形が対象を含むかまで（**`..` と 2 点形も同じ**＝括弧） | **残す** | `visual.rs`:232・`docs/build-commands.md`:119（「観測形が対象を含むか」）が引く |
| 93 | 主張は代理でなく対象そのもので測ってから書く（**一行のコマンドの列挙**＝括弧） | **残す** | `development-principles.md`:26 が「`AGENTS.md`「検証の作法（全タスク共通）」が持つ」と正本指名。`occupy-hotkey.ps1`:13・`PERFORMANCE.md`:3085 も引く |
| 97 | RETROSPECTIVE.md に持続タスクを置かない（上書き前に抽出・振り分け） | D | 正本 `/retrospective`（`retrospective/SKILL.md`:103 が `AGENTS.md`「RETROSPECTIVE.md の運用」を引く——見出しを残す） |
| 98 | 手順の SSOT は `/retrospective` | X（索引） | |

### 消すと失われる規則（正本も機構も無いもの）

ルート CLAUDE.md: :10（セーフティネットは合意）・:15 の cherry-pick 等 5 操作の列挙（⚠ ruleset が最終防衛）・:22・:23（試行）・:27（⚠）・:32-34・:38・:40・:41（指定パスへ書かせる）・:42・:44・:45 の「争点の形」「逆向きの監査」・:46 後半（実データへ当てる）・:51・:52・:53。

AGENTS.md: :9・:23 括弧の「ADR は否定の知識が生じた決定のみ」・:29（/start-issue 外）・:31-35・:36・:38・:43・:47・:50 括弧の「コマンド文字列はここに書かない」・:60・:62（括弧の「LSP 無しのときだけ grep」を含む）・:66 の「生ログはリポジトリの外」「派生表だけ」・:67（規範文書は配送されない→常時ロードが唯一の経路）・:68・:70 の「PR 本文も母集団」・:73・:74・:75・:84-93（検証の作法の全箇条。:85 と :92 は括弧の中にも規則）。

**経緯の括弧の中に埋まった規則**（括弧ごと消すと失われる）: AGENTS:23（否定の知識のみ）・:50（コマンド文字列を書かない）・:59（初回フロー——ただし state-check が本文を持つ）・:62（LSP 無しのときだけ grep）・:67（規範文書は配送されない）・:85（which 空・grep 0 件は確定に足りない）・:90（glob の意味論はツールごとに違う）・:92（`..` と 2 点形も同じ）・:93（一行のコマンドの列挙）。CLAUDE:23（試行の印＝撤去条件の所在）・:15（不在検知を意図的に置かない——ただし `_lib.sh`:10 が正本）。

---

## 2. この 2 枚を指す参照

### 2-A. 正準形（governance が照合するもの）

測り方: `lib.mjs` の `HEADING_REF`・`isRefTargetSpelling`・`resolveRefTarget`・`collectAnchors`/`normAnchor` をそのまま import し、全ファイルの全行に当てて**解決先がルート `CLAUDE.md` / `AGENTS.md` のもの**だけ残した（スクリプト: scratchpad の `refs.mjs`）。

- 全 190 件。うち **照合母集団の行**（`headingRefDocs`/`headingRefSourceDocs`/`headingRefCommentDocs` かつ `refScanLines` が返す行）は **53 件で全件着地**。
- ラベル別（生きた層）: `AGENTS.md`「条件別チェック（トリガー → 参照先）」13・「条件別チェック」6・「検証の作法」8・「検証の作法（全タスク共通）」4・「バグか仕様変更かを判定する」3・「事前調査」2・「ドキュメント参照」2・「変更後の検証を実行する」1・「照合は SSOT に対して行う」1・「3層分担」1・「RETROSPECTIVE.md の運用」1・「バグ発見時は同一パターン全コードパス検索を行う」1／ルート `CLAUDE.md`「サブエージェント委譲と worktree」6・「フック」3・「Git/GitHub 運用」1（合計 53。2 枚どうしの相互参照——AGENTS→CLAUDE「フック」「委譲」、CLAUDE→AGENTS「ドキュメント参照」「条件別チェック…」「条件別チェック」——は上の件数の内数）。
- **この列挙の死角**: `refs.mjs` は物理行単位で `HEADING_REF` を当てるので、改行で折れた正準形は一致を生成しない。列挙が完全なのは、`G-folded-heading-refs` が緑（走査母集団にその形が無い）であることに依る——本導出時点の `governance:check` は緑なので成立する。
- **残すべき見出し・アンカー**（消す/改題すると governance が赤）: `## ドキュメント参照`・`## 3層分担`・`## 条件別チェック（トリガー → 参照先）`（前方一致なので「条件別チェック」も同じ見出しで着地）・`## 検証の作法（全タスク共通）`・`## RETROSPECTIVE.md の運用`・番号付き項目「「バグ」か「仕様変更」かを判定する」「事前調査…」「変更後の検証を実行する…」・太字リード「バグ発見時は同一パターン全コードパス検索を行う」「照合は SSOT に対して行う…」／ルート `## Git/GitHub 運用`・`## フック`・`## サブエージェント委譲と worktree`。**番号付き項目をやめて表や散文にすると、`ANCHOR_SPECS` の番号付きリストの腕から外れて着地しなくなる**（「事前調査」「変更後の検証を実行する」「バグか仕様変更か」）。
- 母集団外（`docs/adr/`・`docs/superpowers/`）の未着地（fixture を除き 19 件・本導出時点）は凍結された歴史で直さない: 「全称表現は前提条件とセットで書く」5・「文書に事実の写しを増やす変更」4（表の行は見出しでないのでアンカーにならない）・「ガバナンス機構**自身**の配置を変える」・「レビュー指摘へ修正を当てた」・「列挙も SSOT のツール自身に問う」・「調査・測定のための一時的な足場」・「正本を 1 か所に定め他は参照へ」・CLAUDE「シェル環境」「最重要ルール 2」ほか。テストの文字列リテラル（`lib.test.mjs` の「Git 運用」・`G-heading-refs.test.mjs` の「無い見出し」）は負の fixture。

### 2-B. 見出しは着地するが、特定の箇条の**中身**に依存する参照（governance は見ない）

| 参照元 | 指す先（実際に依存する中身） | 縮めたときの危険 |
|---|---|---|
| `.claude/skills/implement/SKILL.md:84` | CLAUDE「委譲」:40 検査対象を変更しながら走らせない | 箇条を消すと根拠が空 |
| `.claude/skills/implement/SKILL.md:112` | CLAUDE「委譲」:43 意図的に分けた構造 | 同上 |
| `.claude/skills/start-issue/SKILL.md:70` | CLAUDE「委譲」:39 画像は見えない | **裁定で消える→必ず直す** |
| `docs/development-principles.md:259` | CLAUDE「委譲」:41/:42（報告が届かないときの対処） | 箇条を消すと空 |
| `docs/design/2026-08-20-governance-meta-demotion-derivations.md:10` | CLAUDE「委譲」:46 所見であって機序ではない | 設計メモ（日付つき）。直すかは任意 |
| `docs/hooks.md:5` | CLAUDE「フック」＝応答と**沈黙の読み方**の SSOT | **現行で既に「沈黙」の中身が無い** ⚠ |
| `docs/development-principles.md:93` | AGENTS「ドキュメント参照」に常時ロード面と hooks.md の行が**無い** | **現行で既に偽**（:24・:25 に在る） |
| `CLAUDE.md:10` | AGENTS 表のセーフティネット行（:67）＝母集団の正本 | 行から「規範」を外すと最重要ルール 2 の母集団が変わる |
| `.claude/rules/src-tauri.md:27`・`.claude/rules/snotra-settings.md:20` | AGENTS 表 :63 の race-check の述語の**全文**（「ここに写しは置かない」と宣言） | 述語を短くすると rule が SSOT と呼ぶ全文が消える |
| `.claude/rules/snotra-core.md:25` | AGENTS「事前調査」:43 ベースライン差分検証 | :43 を消すと空 |
| `.claude/rules/src-tauri.md:22` | AGENTS「事前調査」:45 ペアで計画（一般則） | :45 を D として消すと、相互に正本を呼び合う形が片側だけになる |
| `.claude/skills/plan-review/SKILL.md:25` | AGENTS 表 :62「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」 | :62 を縮めると空 |
| `window_coordinator.rs:1203` | AGENTS 表 :60「重複した読み」 | 同上 |
| `startup.rs:3`・`occupy-hotkey.ps1:26`・`PERFORMANCE.md:39`・`window_coordinator.rs:960`（散文形）・`match_cost.rs:14`（散文形） | AGENTS 表 :66 足場の行（撤去条件・出所の着地・**生ログの置き場**） | match_cost.rs は「利用者の実ファイルパスを持つ」中身に依存 |
| `comment-guidelines.md:23` | AGENTS 表 :70 写しの行 | 同上 |
| `check-skill-skeleton-design.md:119`・`:127`（散文形） | AGENTS 表が check スキルを別行で起動すること | 行を束ねると偽 |
| `rust-toolchain.toml:10` | AGENTS 表「ガバナンス機構の行」 | **現行で既に無い行**（#1256 で消えた） |
| `health-check/SKILL.md:30` | AGENTS「変更後の検証を実行する」にコマンド本体が無いこと（grep 検査の対象） | 見出しを消すと検査対象が消える |
| `health-check/SKILL.md:47`・`RETROSPECTIVE.md:23` | 検証の作法 :84-85（全称・反証プローブ） | |
| `health-check/SKILL.md:94`・`layout.rs:9` | 検証の作法 :90（照合は SSOT に対して） | |
| `win_registry.rs:7` | 検証の作法 :91（重複排除の判定） | |
| `visual.rs:232`・`docs/build-commands.md:119`・`manual-smoke.ps1:24` | 検証の作法 :92（代表入力で測る・観測形が対象を含むか・届かない検証） | :24 は「届かない検証は実施の有無すら区別できない」——これは CLAUDE:41 の言い回しに近く、検証の作法には逐語で無い ⚠（現行で既にずれ） |
| `development-principles.md:26`・`occupy-hotkey.ps1:13`・`PERFORMANCE.md:3085` | 検証の作法 :93（代理でなく対象で測る） | dev-principles が正本として名指し |
| `retrospective/SKILL.md:52`・`:64`・`:67` | 検証の作法＝横断原則の置き場／表＝トリガー→参照 1 行 | 構造の約束 |
| `retrospective/SKILL.md:59`（散文形） | AGENTS:5「冒頭の組織原理」 | :5 を消すと行き先消失 |
| `retrospective/SKILL.md:66`（散文形） | ルート CLAUDE の「コミュニケーション原則・スキル表・委譲/worktree 運用」 | 節名を変えるなら合わせる |
| `state-check/SKILL.md:30`（散文形・バッククォート無し） | AGENTS:59 の括弧（表のセル） | 括弧を縮めると沈黙で切れる |
| `deps-update/SKILL.md:26`（散文形） | CLAUDE:9（言い換え「`main` 直コミット禁止」） | 現行で字面不一致 |
| `docs/design/2026-05-31-coherence-staleset.md:9`（散文形） | AGENTS「横断パターンの変更は相談してから」 | **現行で既に無い**（日付つき設計メモ・直すかは任意） |
| `.claude/skills/health-check/references/mechanized-checks.md:8` | 「Check 3 — `AGENTS.md` ドキュメント参照の実在性」 | G-references が全 governanceDocs へ一般化済みと同行が明記。影響なし |
| `SPEC.md:7`・`docs/build-commands.md:5`・`.claude/rules/safety-nets.md:15`・`.claude/rules/governance-docs.md:13`・`docs/development-principles.md:78` | ファイル単位の言及（「AGENTS.md は表だけでなく…」等） | :78 は AGENTS.md の書式に関する主張——縮めた後も真か要確認 ⚠ |

**hook の拒否文言**: `.claude/hooks/pre-bash.mjs` と `.githooks/` の拒否メッセージで 2 枚を指すものは **0 件**（`pre-bash.mjs`:130・:215 はコメント、`pre-bash.test.mjs` は grep の fixture）。`scripts/merge-pr.mjs`・`scripts/verify-tree.mjs` の出力文言にも 2 枚の名指しは無い。

### 2-C. governance の照合が見るもの・見ないもの（`scripts/governance/lib.mjs` 実読）

見る:
- `G-heading-refs`: 正準形 `` `<対象>`「<見出し>」 ``（`HEADING_REF`。ラベルは 1 段の入れ子まで）。対象綴りは `isRefTargetSpelling`（`.md`・`.mjs`・`/skill`）。解決は `resolveRefTarget`——**文書ディレクトリ基準を優先**するので、サブディレクトリの文書に書いた `` `CLAUDE.md` `` は同じディレクトリの CLAUDE.md へ解決されうる。着地は `collectAnchors`（ATX 見出し・番号付き項目の全文・箇条の太字リード・`describe`/`it`）を `normAnchor`（バッククォート・`*`・「」・空白を除去）して**前方一致**。
- 走査元: `headingRefDocs`（`.md` から `docs/superpowers/`・`workspace/`・`docs/adr/` を除く）＋ `.rs` 全行＋コメント記法を持つスクリプトのコメント行。参照先の解決は全ファイル。
- `G-near-heading-refs`（助詞が挟まった近傍形を、見出しへ着地するときだけ促す）・`G-folded-heading-refs`（物理改行で折れた形）。
- `G-references`: `governanceDocs`（ルートの CLAUDE/AGENTS/CONTRIBUTING/SPEC・docs/・モジュール CLAUDE.md・rules・SKILL.md）の中のパスの実在。
- `G-stale-identifiers`: `STALE_EXTRA_DOCS` に CLAUDE.md・AGENTS.md が入り、バッククォートの識別子（`judgeSummary` 等）の実在を見る。
- `G-adr-citations`: `ADR-<slug>` の実在。

見ない:
- 散文形（バッククォート無し・「冒頭の」「の足場の行」「の委譲の節」）——2-B の散文形すべて。
- **見出しは着地するが、引用している箇条の中身が消えた形**——2-B の大半。今回の縮小で最も起きやすい型。
- 表のセル（アンカーにならない）を指す参照。
- 規則の消失そのもの（どの検査も「規則が別の場所に正本を持つか」を見ない）。
- ルート CLAUDE.md・AGENTS.md の中身を節単位で読む機構は無い（`sectionOf` の呼び手は `G-module-index`〔サブ crate の CLAUDE.md〕と `verify-tree.mjs`〔build-commands.md〕だけ）。スキル表と `disable-model-invocation` の一致も機械照合されない。

---

## 3. 偽の記述（コード・スキル・hook と食い違う）

1. **AGENTS.md:61「config キーは `/plan-review` の 4点セット（新規記録・既存移行・外部参照 API を同時に揃える）」**——二重に偽。(a) plan-review に 4 点セットは無い。**#849（4998405e）で plan-review から削除された**（削除された中身は「(1) `Config::default`/serde デフォルト (2) `snotra-settings` の UI (3) `config_watcher` の反映経路 (4) 既存 `config.toml` への後方互換」）。(b) 括弧の 3 項目は config の 4 点ではなく **history キーの 3 者**（`snotra-core/CLAUDE.md`「history.rs のキー正規化に関するチェックリスト」）の写し。今の生きた層に config キーの 4 点は無い（`config_watcher` を言うのは SPEC だけ）。直す先の候補: `snotra-core/CLAUDE.md`「データ永続化の注意」「Config のデシリアライズ経路」と `docs/development-principles.md`「config の値は到達性の検出器を持たない」。
2. **CLAUDE.md:20「検証は `AGENTS.md` のワークフロー手順 6 で手で行う」**——序数での参照（`.claude/rules/governance-docs.md`:15 が禁じる形）。`/implement` では `npm run verify:tree` が A〜F を道具の木で回す（`implement/SKILL.md`:82）。
3. **CLAUDE.md:66「カテゴリ A〜F の検証は `npm run verify:tree` が担う」**——道具は D を回さず `handoff` と書き、B は受け付けない（`scripts/verify-tree.mjs`:16-17・`RUNNABLE = ["A","C","E","F"]`）。「`code-reviewer` を `/implement` 3b が起動する」も ⚠: 3b が起動するのは `isolation: "worktree"` の Agent 1 体で、`code-reviewer` は「観点」として渡される（`implement/SKILL.md`:110）。subagent_type として起動する記述は無い。裁定で行ごと削るので直しは不要。
4. **`docs/development-principles.md:93`**（2 枚の外だが 2 枚の内容についての主張）——「`AGENTS.md`「ドキュメント参照」に常時ロード面自身と hooks.md の行が無い」は偽（:24・:25）。
5. **`docs/hooks.md:5`**（同上）——フック節が「沈黙をどう読むか」の SSOT と言うが、フック節に沈黙の条項は無い ⚠。
6. **`rust-toolchain.toml:10`**（同上）——存在しない「ガバナンス機構の行」を引用。
7. ⚠ **CLAUDE.md:41「`report*.md` にしない——サブエージェントからの Write が harness に拒否される」**——サブエージェントのシステムプロンプト上の制約は「report/summary/findings/analysis の .md を書くな」で、`report*.md` より広い（本導出エージェント自身のシステムプロンプトで確認）。「拒否される」が harness の機構か指示かはリポジトリから検証できない。
8. ⚠ **`src-tauri/src/egui_shell/window_coordinator.rs:776`「`/persistence-check` の 4 点セット」**——persistence-check に 4 点セットは無い。1 と同じ系の残骸（範囲外なら別 issue）。
9. ⚠ **`scripts/manual-smoke.ps1:24`**——「`AGENTS.md`「検証の作法」に照らせば、届かない検証は実施の有無すら区別できない」——この言い回しは検証の作法ではなく CLAUDE.md:41（委譲した検査の成果物）のもの。現行で既に出典ずれ。

真と確かめたもの: CLAUDE.md:57 の「表が索引するのは `disable-model-invocation: true` だけ」（4 本一致）・:24 の `judgeSummary`/`readVerifyState`（実在）・:26 の `skip-ci` 非対象（ci.yml:47）・:14 の `githooks.test.mjs`（実在）・:16 の `--auto` 不使用（merge-pr.mjs が口を持たない）・AGENTS.md:20 の 4 モジュール CLAUDE.md（実在）・:69 カテゴリ F（build-commands.md:170）。

---

## 4. 変更が必要なファイル

冒頭の表のとおり。最小（必須）は: `CLAUDE.md`・`AGENTS.md`・`.claude/skills/start-issue/SKILL.md`（:70 の出典）・`docs/development-principles.md`（:93 の既存の偽。:259 は CLAUDE:41/:42 を残すなら不要）・`docs/hooks.md`（:5）。
縮め方次第で増えるもの: 2-B の表の各参照元（とくに `.claude/rules/src-tauri.md`:22/:27・`.claude/rules/snotra-settings.md`:20・`.claude/rules/snotra-core.md`:25・`.claude/skills/plan-review/SKILL.md`:25・`.claude/skills/implement/SKILL.md`:84/:112・`.claude/skills/retrospective/SKILL.md`:59/:66/:69・`.claude/skills/state-check/SKILL.md`:30）。
既存の偽として推奨: `rust-toolchain.toml`:10・`scripts/manual-smoke.ps1`:24（⚠）・`src-tauri/src/egui_shell/window_coordinator.rs`:776（⚠・範囲外可）。

### 計画への注意（導出から出た争点）
- **見出しを保つだけでは足りない**: 生きた層の 53 件は全部着地するが、うち 20 件以上は見出しではなく**特定の箇条の中身**を根拠として引いている。草案が「正準形 349 件照合・未着地 0」を根拠にしているなら、それは 2-B の型を見ていない（governance の宣言する死角そのもの）。
- **「表が SSOT」の宣言が 7 か所以上ある**（`implement`:94・:128・:184、`start-issue`:120、`plan-review`:25、`rules/src-tauri.md`:27、`rules/snotra-settings.md`:20、`code-reviewer.md`:15・:52、`retrospective`:64）。表の行を「スキルの description が持つ」として消すと、これらが空の SSOT を指す。
- **CLAUDE.md:23 の「試行・#1324」は経緯の括弧ではない**——期限つき試行の撤去条件の所在。外さない。
- `retrospective/SKILL.md`:69 の「根拠 issue 番号まで」と今回の「経緯の括弧を外す」の整合を決める。
- 新設する 1 行に「4 本」と書かない。
- `.claude/rules/governance-docs.md`:18 が求める旧版/新版の**集合差**（バッククォート語・issue 番号・ADR 名）は識別子しか見ないので、**括弧の中に埋まった規則の喪失を拾わない**。1 の末尾の「経緯の括弧の中に埋まった規則」の一覧はその集合差の補完であり、実装後の検算に使える。
- 「X が持つから消す」の判定そのものに `docs/development-principles.md`「撤去（消す変更）の作法」の箇条「**『これは X が持つから消す』と書くときは、X の母集団が元の観点を覆っているかを読んで確かめる**」が当たる。D 判定のうち正本が**スキル**（`/start-issue`・`/implement`・`/merge-pr`）にあるものは、そのスキルを経ない作業では覆われない——裁定で受け入れたのは 4 件だけで、ほかの D（CLAUDE:16・:25・:43・:45 の ⚠️・:46 前半、AGENTS:29・:44）も同じ型である。
