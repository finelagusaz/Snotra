# ルート CLAUDE.md / AGENTS.md の条項分類（提案・main a400ffc3）

- 条項の切り出しと id は `norm-usage.md` と同じ（`clauses.mjs`）。字数は `[...raw].length`（コードポイント・CR 除く）。全体: ルート CLAUDE.md 4,889 字・AGENTS.md 9,396 字
- 引用 = `norm-usage.md` の「セッション 除外後/本日込み」。**0 件は判定に使っていない**（参考）
- 分類: K 残す／S システムプロンプトと重なる／D 写し（正本 file:line）／M 機構が守る／H 経緯／X 消してよい。複合は主たるものを先に書く。⚠ = 確信が持てない
- 「外参照」= `git grep` ではなく `scripts/governance/lib.mjs` の `allHeadingRefDocs` + `HEADING_REF` + `resolveRefTarget` で G-heading-refs と同じ母集団・同じ解決をして数えた件数（script: `rootrefs.mjs`・結果: `rootrefs.txt`）。**前方一致**なので、太字リード・見出しは「短くする」と全件が落ちる（伸ばすのは可）

## 測定で見つかった偽・腐り（縮める以前に直すもの）

1. **AGENTS.md:61（id 76）「config キーは `/plan-review` の 4点セット」は偽。** 4 点（新規記録 / 既存データ移行 / 参照 API）は `.claude/skills/persistence-check/SKILL.md:95` にあり、`plan-review/SKILL.md` に「4点」は 0 件（`git grep -n '4点\|4 点セット\|新規記録'`）。`src-tauri/src/egui_shell/window_coordinator.rs:776` は正しく `/persistence-check` の 4 点セットと書いている
2. **CLAUDE.md:20（id 6）「検証は `AGENTS.md` のワークフロー手順 6 で手で行う」は序数参照**（`.claude/rules/governance-docs.md` が禁じる形・G-heading-refs に載らない）。かつ `/implement` ではもう手で行わない（`npm run verify:tree` が A〜F を回す・`implement/SKILL.md:82,100`）
3. AREA_BUDGET は存在しない（下の「面積の上限」）——`ADR-check-skill-skeleton` 等の凍結 ADR に名前が残るだけ

## ルート CLAUDE.md（36 条項）

| id | 条項（書き出し） | 字 | 引用 | 分類 | 根拠・正本 | 案 |
|---|---|---|---|---|---|---|
| 0 | 前書き（運用ガイド。共通プロセスは AGENTS.md…） | 142 | 0/0 | K（短縮） | `@AGENTS.md` の直前に要る。`AGENTS.md`「ドキュメント参照」への参照は `CLAUDE.md:3` が外参照 1 件の元 | 1 文へ |
| 1 | main へ直接コミット・プッシュしない | 101 | 2/2 | K + M + S | 機構: `.githooks/pre-commit`・`_lib.sh:18` の拒否文がブランチ接頭辞まで案内・GitHub ruleset。SP「既定ブランチ上ならブランチを切ってから」と重なる | 残す（最重要ルール・注意書きの要求）。命名接頭辞は 1 語ずつに。機構を名指す |
| 2 | セーフティネットの変更は合意してから | 173 | 2/3 | K | 他に正本なし。`AGENTS.md`「条件別チェック」のセーフティネット行を母集団として指す | 残す・短縮 |
| 3 | main 保護の実体は .githooks/ と ruleset | 189 | 0/0 | M + H | `githooks.test.mjs`・`.githooks/_lib.sh:9`「最終防衛線は GitHub ruleset」 | 4 と 1 行に統合 |
| 4 | Layer 1 は best-effort | 246 | 1/1 | K（短縮）+ M | `.githooks/_lib.sh:9`。「cherry-pick 等で沈黙で main が進む」は他に無い | 「`cherry-pick` 等は素通り・ruleset が push で捕まえる」だけ残す |
| 5 | squash マージは /merge-pr から道具で | 311 | 0/1 | K（短縮）+ D + H | 理由は `docs/adr/ADR-squash-merge-issue-autoclose.md`、手順は `.claude/skills/merge-pr/SKILL.md:28`。#488 は H | 「`/merge-pr` から道具で。`gh pr merge` を手で打たない（`--auto` も）」＋ADR を指す |
| 6 | PreToolUse フックがコマンドの形を見て拒む（節の地の文） | 195 | 0/0 | M + X | 序数参照「手順 6」（上の偽 2）。「編集後の自動検証は無い」は推論で足りる | 削除 |
| 7 | 拒否メッセージが復帰手順を持つ。それに従う | 80 | 1/1 | K | `docs/hooks.md`「PreToolUse（pre-bash.mjs）の実装契約」（`docs/hooks.md:9`） | 残す。12 を吸収 |
| 8 | Windows の Bash では複数行は Write ツールで | 148 | 0/1 | K（喧嘩の裁定） | 拒否 79 件（全拒否の 76%）。拒否文 `pre-bash.mjs:257-260` が復帰手順を持つが、harness が heredoc を勧めるので先回りが要る。#1325 で入ったばかり＝効果未測 | 残す。「（試行・#1324）」は H→消す |
| 9 | gh pr create は未 push 等で拒まれる・鎖に cd を含めない | 231 | 0/0 | M | 拒否文 `pre-bash.mjs:82`（REMEDY）・`:345`・`:360`。判定の正本 `judgeSummary` / `readVerifyState` | 削除 ⚠（4 週間で拒否 3 件。先回りの価値が heredoc ほど無い） |
| 10 | フックが守る不可逆呼び出しは gh pr create だけ／道具を経ないマージは人間専用 | 240 | 1/2 | D + K（一部） | `docs/adr/ADR-squash-merge-issue-autoclose.md`。「人間専用の迂回」だけは行動規範 | 5 へ統合（「Web UI・手打ちの `gh pr merge` は人間専用」） |
| 11 | 文書の決定的な整合は governance-check job | 92 | 0/0 | M + D | `.github/workflows/ci.yml:51`・`AGENTS.md` の governance 行と写し | 削除（AGENTS 側の 1 行が持つ） |
| 12 | フックを改修するときは docs/hooks.md を読む | 31 | 0/0 | K（7 へ統合） | — | 7 に「改修時も同じ文書」と足す |
| 13 | 意図は明確に、指示は短くていい | 92 | 0/0 | X ⚠ | 人間側への約束。エージェントへの指示としては SP「自分の判断で」と重なる | 削除 ⚠（チーム憲章は合意事項ゆえ確認要） |
| 14 | 複雑さや意図不明さに気づいたら声を上げる | 86 | 0/0 | K（統合） | SP に無い | 14・15 を 1 行へ |
| 15 | 「やりすぎでは」を歓迎する | 63 | 0/1 | K（統合） | ユーザー訂正 #6〜#8（norm-usage §4）はすべて「削る」方向——この条項の実在の需要 | 14 と統合 |
| 16 | 記録への信頼で動く——その場で記録へ書く | 94 | 0/0 | K ⚠ | auto-memory（SP）は「メモリへ書け」と言う。この条項は「リポジトリの記録へ」——割れている（喧嘩 一覧 #7） | 「教訓はメモリより先にリポジトリの記録へ」と優先を明記 |
| 17 | 並列委譲はファイル境界で衝突を予測 | 94 | 1/1 | K（短縮）+ H | SP の Agent 説明に無い。#439 は H | 短縮 |
| 18 | 委譲はコンテキストを継承しない | 112 | 1/1 | S + D | SP の Agent 説明「It knows only what you put in the prompt」。画像の件は `start-issue/SKILL.md:70` | 削除。画像の 1 句は start-issue が持つ ⚠（start-issue の外の委譲では射程が縮む） |
| 19 | 検査対象を変更しながら検査を走らせない | 83 | 1/1 | K + D（部分） | `/implement` 内は `implement/SKILL.md:122`（3c「両方届くまで作業ツリーに触らず」）。スキル外の並列検査には他に正本なし | 残す・短縮 |
| 20 | 成果物は指定パスへ・report*.md にしない | 133 | 1/1 | K + D（部分） | `start-issue/SKILL.md:68`・`implement/SKILL.md:114`・`plan-review/SKILL.md:54`。**スキル外の委譲（チーム作業）には届かない**——本日の依頼文もこの条項を使っている | 残す・短縮（#725 は H） |
| 21 | 長時間の委譲は中断を前提にする | 98 | 8/9 | K | 他に正本なし（implement の「フェーズのコミット」は plan.md の話） | 残す・短縮 |
| 22 | レビューの委譲では意図的に分けた構造を先に渡す | 61 | 10/11 | K + D | `implement/SKILL.md:112` が逐語、かつ**この節を指し返す**（外参照） | 残す（#872 だけ消す） |
| 23 | 指摘 0 件の報告は対象を突き合わせてから受け取る | 61 | 2/2 | K | 他に正本なし | 残す |
| 24 | 確信の持てない所見（⚠）も返り値に・争点の形・逆向きの監査 | 99 | 11/12 | K + D（前半） | 前半は `start-issue/SKILL.md:67`・`implement/SKILL.md:113`。「逆向きの監査」は他に無い | 残す・短縮 |
| 25 | 採るのは所見であって機序ではない | 73 | 4/5 | K + D | `start-issue/SKILL.md:72`。本日 #768 巻き戻しかけの件で効く場面が実在。`docs/design/2026-08-20-governance-meta-demotion-derivations.md:10` が節を指す | 残す |
| 26 | 実行にバイアスをかける | 162 | 0/1 | **S** | SP「When you have enough information to act, act … re-litigate a decision the user has already made」。プロジェクト差分は「コミット・PR 作成の指示は確認なしに即実行」——SP「Commit or push only when the user asks」と整合しており足す情報が無い | 削除。29 の優先規則は 29 の側へ移す |
| 27 | 計画書・設計書は忠実に実装する | 57 | 0/0 | K ⚠ | SP に無い。`/implement` は plan.md を前提に進むが「省略・統合しない」は書いていない（未確認 ⚠） | 28 と 1 行へ |
| 28 | 意図的なリファクタリングの結果を元に戻さない | 187 | 0/1 | K | 本日の実例（#768 の巻き戻しかけ）。他に正本なし | 残す・短縮（確かめ方 1 句は残す） |
| 29 | 分析・調査・助言は報告のみ | 123 | 0/0 | K（喧嘩の裁定） | SP は逆向き（「動けるなら動け」）。26 を消すと内蔵の優先規則の相手が変わる | 「システムプロンプトの『動けるなら動け』より優先する」と相手を SP に替えて残す |
| 30 | スキル表の前文（user 起動専用だけを索引する理由） | 232 | 0/0 | K（短縮）+ 喧嘩の裁定 | roster に出ない 4 本（`disable-model-invocation: true`: deps-update・health-check・merge-pr・retrospective を grep で確認）はここでしか知れない。理由（二重課税）は H | 「Skill ツールからは呼べない——頼まれたら SKILL.md を読んで手順を辿る」を足す（メモリ `feedback_user_only_skill_invocation` の回避策を正本化・喧嘩 #9） ⚠ 追記 |
| 31〜34 | /merge-pr・/health-check・/retrospective・/deps-update の行 | 297 | 14・0・2・5 | K（短縮） | `description` が roster に載らないので写しにならない | 各行を半分へ |
| 35 | サブエージェント code-reviewer | 208 | 11/12 | D | `.claude/agents/code-reviewer.md`（agent roster に description ごと載る）・起動条件は `implement/SKILL.md`「3b. 委譲へ渡すもの」 | 削除 ⚠ |

### 集計（ルート CLAUDE.md）

| 主分類 | 条項 | 字 |
|---|---|---|
| K（短縮して残す） | 0,1,2,4,5,7,8,12,14,15,16,17,19〜25,27,28,29,30,31〜34 = 27 | 3,073 |
| S | 18,26 = 2 | 274 |
| D | 10,35 = 2 | 448 |
| M | 3,6,9,11 = 4 | 707 |
| H | （各行の issue 番号の括弧として分散・独立条項なし）0 | — |
| X | 13 = 1 | 92 |
| 見出し・空行等（条項外） | — | 約 300 |

（字数は現行。草案全体は 2,313 字——下の「草案」参照。H は独立条項が無く、K 行の括弧の issue 番号・経緯として分散）

## AGENTS.md（71 条項）

| id | 条項（書き出し） | 字 | 引用 | 分類 | 根拠・正本 | 案 |
|---|---|---|---|---|---|---|
| 36 | 前書き | 153 | 0/0 | K（短縮） | — | 1 文へ |
| 37 | 注意事項は「トリガー」に括り付けてある | 140 | 0/1 | X | 構造の自己説明。表の前文 72 と重複 | 削除 |
| 38 | 3層分担（挙動を変える変更は SPEC と同じ変更で整合） | 73 | 1/1 | K | 見出し「3層分担（責務分離）」は `implement/SKILL.md:78` が指す | 残す |
| 39 | ドキュメント参照の前文（末尾の一句は書式の索引） | 118 | 0/1 | K ⚠ | 書式の句は 2026-08-22 `docs/superpowers/plans/2026-08-22-norm-format-by-role.md` で入れた意図的構造。正本 `docs/development-principles.md:74`「規範の書式は文書の役割から選ぶ」 | 残す（前文を 1 文へ）。消すならユーザー確認 |
| 40〜50 | 索引 11 行 | 785 | 0〜8 | K（短縮）+ H + M | 48 の「#593・連番を振らない・#812」は H + M（G-adr-file-names）。50 は `CLAUDE.md`「フック」を指す外参照の元 | 括弧の経緯を外す |
| 51 | サイクル開始前に RETROSPECTIVE.md を読む | 80 | 0/0 | K ⚠ | どのスキルも持たない（`git grep RETROSPECTIVE.md -- .claude/skills` は retrospective だけ）。/start-issue へ移す案もあるがそれは射程の変更 | 残す・短縮 |
| 52 | 「バグ」か「仕様変更」かを判定する | 17 | 2/2 | K | **外参照 3**（`code-reviewer.md:85`・`state-check/SKILL.md:90`・`docs/comment-guidelines.md:178`）。番号付きリストのアンカー | 逐語保持 |
| 53〜56 | ラベルではなく 2 参照で決まる／バグ／仕様変更／fix でも仕様変更 | 277 | 0〜1 | K（統合） | 56 は `code-reviewer.md:85`（2e SPEC 同期）と写し | 1 行へ |
| 57 | 要件（受け入れ条件）をテスト可能な形で確定 | 23 | 11/12 | K + D | `start-issue/SKILL.md:78` | 残す |
| 58 | 影響範囲（読む/触る/触らない）を列挙 | 21 | 2/2 | K | — | 残す |
| 59 | git マージ状態の確認は git diff で | 109 | 0/0 | K（短縮） | 他に無し | 短縮 |
| 60 | バグ発見時は同一パターン全コードパス検索 | 125 | 0/1 | K + D | 太字リードは **外参照 1**（`docs/development-principles.md:246`）。完了ゲート版の正本は同 `:246` | リード逐語保持・本文削る |
| 61 | 「変更なし」と判断するときは影響ケースを列挙 | 101 | 0/0 | K + D | `docs/development-principles.md:226`「列挙の完全性」 | 短縮 |
| 62 | 下の条件別チェック表で振り分ける | 33 | 0/0 | X | 表の前文と重複 | 削除 |
| 63 | 事前調査（レビュー未然防止）を実施する | 19 | 0/0 | K | **外参照 2**（`.claude/rules/snotra-core.md:25`・`src-tauri.md:22`） | 逐語保持 |
| 64 | 挙動変更なし前提はベースライン化 | 32 | 0/0 | K | 他に無し | 残す |
| 65 | 境界条件を列挙し最低 1 件ずつ | 24 | 0/0 | D | `start-issue/SKILL.md:115`（5b セルフレビュー） | 残す ⚠（24 字。消しても得が無い） |
| 66 | リソース・状態フラグは生成/破棄・真偽のペアで計画 | 218 | 0/0 | D | 表の対称ペア行（id 73）→ `/symmetric-check`・Rust 則 `.claude/rules/src-tauri.md:20` | 1 句へ ⚠ |
| 67 | 事前調査は表と rules が示す | 65 | 0/0 | X | 前文と重複 | 削除 |
| 68 | 失敗するテストから実装する | 63 | 3/4 | K（短縮） | — | 短縮 |
| 69 | 変更後の検証を実行する（スキップ不可） | 19 | 0/0 | K | **外参照 1**（`health-check/SKILL.md:30`） | 逐語保持 |
| 70〜71 | カテゴリ A〜F を実行／コマンドは書かない | 166 | 11/12・0 | K + M | 正本 `docs/build-commands.md:7`「変更後の検証チェックリスト」。/implement 内は `verify:tree` が回す。71 の「コマンドを直書きしない」は `health-check/SKILL.md:30` が検査 | 1 行へ |
| 72 | 表の前文 | 87 | 0/0 | K（短縮） | — | 1 文 |
| 73 | 対称ペア → /symmetric-check | 89 | 9/10 | K | skill description と部分重複（索引として残す） | 残す |
| 74 | UI モード・ガード → /state-check | 84 | 8/9 | K | 同上 | 短縮 |
| 75 | 重複した読み・冗長な状態を束ねる/消す | 156 | 0/0 | K + H | #671/#673 は H | 経緯を外す |
| 76 | 永続形式・キー形式 → /persistence-check | 141 | 9/10 | K（**偽を含む**） | 上の偽 1 | `/persistence-check` だけに。history キーの所在は snotra-core 側の rules が配る ⚠ |
| 77 | 関数・型を新規定義／改名／導入 | 336 | 2/2 | K（短縮） | SP に LSP 優先の指示は無い | 3 句へ |
| 78 | 並行性 → /race-check | 139 | 10/11 | K | skill description と同じ語の列（写し）⚠ | トリガーを 1 句へ |
| 79 | 網羅性が要件 | 141 | 5/6 | K + D | `docs/development-principles.md:226`「列挙の完全性」・`/plan-review`「Step 2b」 | 短縮 |
| 80 | 検査・検証手段を新設 | 143 | 3/3 | D | `docs/development-principles.md:181`「検証の層と、層と層の隙間」（「穴は境界に空く」も同節） | 参照だけに |
| 81 | 一時的な足場（新設・撤去・生ログ） | **711** | 3/4 | K + H | `PERFORMANCE.md:31`「この文書へ記録するときの規約」・`docs/adr/ADR-measurement-canon-in-code-doc.md`。#872/#1032/#999 の物語は H。**公開リポジトリへ利用者のパスを入れない**は他に正本なし | 3 命令へ（約 190 字） |
| 82 | セーフティネットを新設/変更 | 179 | 7/8 | K | `.claude/rules/safety-nets.md`。最重要ルール 2 がこの行を母集団として指す | 残す・短縮 |
| 83 | 機構・層・ファイル群を撤去する | 154 | 1/1 | K + D（部分） | `docs/development-principles.md:46`「撤去（消す変更）の作法」。「識別子の残存 0 件を根拠にしない」は他に無し | 短縮 |
| 84 | ガバナンス文書 → governance:check | 146 | 14/15 | M | `.github/workflows/ci.yml:51`・`verify:tree` が F を常に回す（`implement/SKILL.md:100`） | 1 行 |
| 85 | 文書に事実の写しを増やす変更 | 130 | 4/5 | D + K（PR 本文） | `.claude/rules/governance-docs.md`「書く約束 (1)」・`docs/development-principles.md:34` DRY。PR 本文を母集団に入れるは他に無し | 短縮 |
| 86 | .rs を追加/削除 | 180 | 0/1 | M + K | `G-module-linkage`（mod 宣言）・`G-module-index`（モジュール構成表 ↔ 実ファイル） | 「`//!` に責務を書き、CLAUDE.md のモジュール構成へ足す（governance:check が照合）」 |
| 87 | 機能削除・trace 名・hotkey → smoke 前提 | 164 | 14/14 | K + H | `docs/build-commands.md:49-50`。「WebView2 e2e は撤去済み」は H | 短縮 |
| 88 | Option/フラグ/enum の出所を変更 | 294 | 0/0 | K + H | #977 は H | 短縮 |
| 89 | 件数 N・上限・導出の入力 | 164 | 0/0 | K + H | #755/#801 は H | 短縮 |
| 90 | fix-forward を当てた | **457** | 4/5 | K + H | `implement/SKILL.md:127,129`（「解消した」の判定は自分で測る）が一部写し。#984・#999 の物語は H | 3 命令へ |
| 91 | アーキ・横断パターンに影響 → docs/ を触る範囲へ | 53 | 0/0 | K ⚠ | 推論で辿り着けるかもしれない（X 寄り） | 残す |
| 92 | 各言語ファイル → .claude/rules/ | 52 | 0/0 | M | harness がパスで自動配送 | 削除 |
| 93 | 委譲 → ルート CLAUDE.md「サブエージェント委譲と worktree」 | 70 | 0/0 | X | ルート CLAUDE.md は常時ロード（同じ面）。外参照 6 件のうち 1 件の元だが、消しても見出しは残る | 削除 |
| 94 | 検証の作法の前文 | 57 | 0/0 | X | — | 削除 |
| 95 | 全称表現は反証プローブ | 274 | 3/4 | **D** + K | `docs/development-principles.md:174`「規範を書くときの作法」の第 2 項がほぼ同文 | 命令 1 文＋参照 |
| 96 | 全称否定はプローブの形が違う | 100 | 2/2 | K | 他に無し | 短縮 |
| 97 | 数え上げは偽になる時点が確定 | 50 | 2/2 | K | — | 98 と統合 |
| 98 | 代理条件も同じ型 | 175 | 4/4 | K + H | #1201 は H | 短縮 |
| 99 | 自分が引用した規範ほど破りやすい | 121 | 0/0 | H | 中身は #977・#992 の物語。命令が無い | 削除 |
| 100 | 保証を宣言する文の隣に保証されない数を置かない | 173 | 3/4 | K + H | — | 短縮 |
| 101 | 照合は SSOT に対して行う | 285 | 0/0 | K + D | 太字リード「照合は SSOT に対して行う。派生コピー同士の一致を完全性の証拠にしない」は **外参照 1**（`health-check/SKILL.md:94`）。glob の件は `docs/development-principles.md:226` | リード逐語保持・本文は「自分が今作った表も派生コピー」だけ |
| 102 | 消す/共通化する前に片方だけが変わる将来を問う | 94 | 3/4 | K | — | 残す |
| 103 | 判定ロジック・fixture は代表入力で実行して測る | **508** | 1/1 | K + H | 他に無し。#999・#482・#922 の物語は H | 2 命令へ |
| 104 | 主張は代理ではなく対象そのもので測る | 154 | 0/0 | K + H | `docs/development-principles.md:26` がこの節を指して「全タスクに効くので AGENTS が持つ」と言う。撤回走査の数値は H | 短縮 |
| 105 | RETROSPECTIVE: 持続タスクを置かない | 158 | 0/0 | K + D | `retrospective/SKILL.md:62,75`。見出しは **外参照 1**（`retrospective/SKILL.md:103` が「この節の不変条件」を指す） | 見出しと不変条件 1 文を残す |
| 106 | 更新手順は /retrospective | 106 | 0/0 | D | `.claude/skills/retrospective/SKILL.md` | 105 へ統合 |

### 集計（AGENTS.md）

| 主分類 | 条項 | 字 |
|---|---|---|
| K | 36,38,39,40〜50,51,52〜56,57,58,59,60,61,63,64,68,69,70〜71,72,73〜79,81,82,83,85,87〜91,96〜98,100〜105 = 57 | 7,380 |
| D | 65,66,80,95,106 = 5 | 765 |
| M | 84,86,92 = 3 | 378 |
| H | 99 = 1（独立条項）。ほかに 75・81・88・89・90・98・103・104 の括弧が H（計 約 1,500 字） | 121 |
| X | 37,62,67,93,94 = 5 | 365 |

S はなし（AGENTS.md はプロジェクト固有の工程が主で、SP と重なる条項は見つからなかった）。

## 外から参照されている見出し（G-heading-refs が照合するもの）

| 見出し／アンカー | 件数 | 参照元 | 草案での扱い |
|---|---|---|---|
| `AGENTS.md`「条件別チェック（トリガー → 参照先）」 | 19 | `.claude/agents/code-reviewer.md:15,52`・`.claude/rules/snotra-settings.md:20`・`src-tauri.md:27`・`.claude/skills/implement/SKILL.md:94,128,184`・`plan-review/SKILL.md:25`・`retrospective/SKILL.md:64`・`start-issue/SKILL.md:120`・`CLAUDE.md:10,57`・`docs/check-skill-skeleton-design.md:119`・`docs/comment-guidelines.md:23`・`PERFORMANCE.md:39`・`src-tauri/src/egui_shell/window_coordinator.rs:1203`・`src-tauri/src/startup.rs:3`・`rust-toolchain.toml:10`・`scripts/occupy-hotkey.ps1:26` | 逐語保持 |
| `AGENTS.md`「検証の作法（全タスク共通）」 | 12 | `health-check/SKILL.md:47`・`retrospective/SKILL.md:52,67`・`docs/build-commands.md:119`・`docs/development-principles.md:26`・`PERFORMANCE.md:3085`・`RETROSPECTIVE.md:23`・`snotra-core/src/win_registry.rs:7`・`src-tauri/src/egui_shell/layout.rs:9`・`visual.rs:232`・`scripts/manual-smoke.ps1:24`・`scripts/occupy-hotkey.ps1:13` | 逐語保持 |
| `AGENTS.md`「「バグ」か「仕様変更」かを判定する」 | 3 | `code-reviewer.md:85`・`state-check/SKILL.md:90`・`docs/comment-guidelines.md:178` | 番号付き項目を逐語保持 |
| `AGENTS.md`「ドキュメント参照」 | 2 | `CLAUDE.md:3`・`docs/development-principles.md:93` | 保持 |
| `AGENTS.md`「事前調査（レビュー未然防止）を実施する」 | 2 | `.claude/rules/snotra-core.md:25`・`src-tauri.md:22` | 逐語保持 |
| `AGENTS.md`「3層分担（責務分離）」 | 1 | `implement/SKILL.md:78` | 保持 |
| `AGENTS.md`「RETROSPECTIVE.md の運用」 | 1 | `retrospective/SKILL.md:103` | 保持 |
| `AGENTS.md`「バグ発見時は同一パターン全コードパス検索を行う」 | 1 | `docs/development-principles.md:246` | 太字リード逐語保持 |
| `AGENTS.md`「変更後の検証を実行する（スキップ不可）」 | 1 | `health-check/SKILL.md:30` | 逐語保持 |
| `AGENTS.md`「照合は SSOT に対して行う。派生コピー…」 | 1 | `health-check/SKILL.md:94` | 太字リード逐語保持 |
| ルート `CLAUDE.md`「サブエージェント委譲と worktree」 | 6 | `implement/SKILL.md:84,112`・`start-issue/SKILL.md:70`・`AGENTS.md:78`（草案で削除）・`docs/design/2026-08-20-governance-meta-demotion-derivations.md:10`・`docs/development-principles.md:259` | 保持 |
| ルート `CLAUDE.md`「フック（.claude/settings.json）」 | 3 | `AGENTS.md:25`・`docs/development-principles.md:93`・`docs/hooks.md:5` | 保持 |
| ルート `CLAUDE.md`「Git/GitHub 運用」 | 1 | `CONTRIBUTING.md:16` | 保持 |

- **草案は外参照のある見出し・太字リードを全部逐語で残したので、直す参照元は 0 件**（草案に対して同じ前方一致で照合した——下の「草案の検算」）
- 外参照 0 の見出し: 最重要ルール（常に適用）・チーム憲章・コミュニケーション原則・利用できるスキル・開発ワークフロー。改名・統合しても G-heading-refs は落ちない。散文の言及は `docs/build-commands.md:5`（「`AGENTS.md` の開発ワークフロー」）と `.claude/hooks/pre-bash.mjs:130` のコメント（「ルート CLAUDE.md の常時ロード面」——見出しではない）だけ
- 照合の死角（G-heading-refs のヘッダ自身が受容残余に書くもの）: 散文形・`docs/adr/`・`docs/superpowers/`・`workspace/`・hook の文字列リテラル。`docs/superpowers/` には `AGENTS.md`「全称表現は前提条件とセットで書く」など**既に着地しない旧見出し**が複数あるが、#589 で非規範化された層ゆえ照合外（直さない）

## 面積の上限（AREA_BUDGET）

**上限も計器も無い。縮めても governance:check は何も要求しない。**
- `AREA_BUDGET` と上限比較は `docs/adr/ADR-retire-area-budget.md`（2026-08-11）で廃止、後継の `G-area-instrument` も #1253（段 1「格下げ中のメタ層を計器ごと撤去」）で撤去済み。`scripts/governance/checks/` に `G-area-*` は無く、`git grep AREA_BUDGET` の出現は凍結 ADR だけ
- 一次規範は `.claude/rules/governance-docs.md` 冒頭の「書く約束」（かぶりなく・必要なことだけ・古い情報を残さない）
- 縮めたときに効く機構は G-heading-refs（上の表）・G-references（パス参照の実在）・G-adr-citations（ADR 名の実在）・G-stale-identifiers（識別子の実在）だけ

## 草案

- ルート CLAUDE.md: 4,889 → **2,313 字（47%）**／AGENTS.md: 9,396 → **4,888 字（52%）**／計 14,285 → 7,201 字（50%）（`draftcheck.mjs`・コードポイント）
- 外参照の着地: 草案 2 枚を差し替えたスナップショットで 349 件照合・**草案が関わる未着地 0 件**。陰性対照: 草案の「条件別チェック（トリガー → 参照先）」を「条件別チェック」へ縮めると 13 件が未着地として出る（検出器が効くことを確認）
- 草案のバッククォートのパスは全部実在
- **`governance:check` 全 17 検査も通した**: `git archive a400ffc3` を scratchpad へ展開して草案 2 枚を差し替え、`node scripts/governance-check.mjs` が「全検査 passed」。陰性対照（同じ複製で見出しを縮める）で `PERFORMANCE.md:39` 等の未着地が赤になることを確認。複製は削除済み。**意味の検査（条項の内容が正しいか）は機構の外**
- 旧版から消えるバッククォート語（集合差）: ルート = `gh pr create` のゲート一式（`judgeSummary`・`readVerifyState`・`workspace/plan.md`・`- [ ]`・`git push -u origin HEAD`・`cd`）・`code-reviewer`・`/simplify`・`githooks.test.mjs`・`skip-ci` ほか。AGENTS = 事件の識別子（`extend_cached_masks`・`40〜95 ms`・`icon:extract_failed`・`input_trace(`）・`AtomicBool` / `Child` / `listen()`・`-D warnings` / `dead_code`（1 タスクに束ねる理由）・`G-module-linkage` / `mod`（mod 宣言の書き忘れは cargo も RA も報せない、という事実）・`.claude/rules/src-tauri.md`
  - ⚠ `mod` 宣言の事実は、`/implement` 外で .rs を足して cargo だけ回す作業者には届かなくなる（`governance:check` を回せば捕まる）

## 喧嘩の一覧と提案

| # | 割れている 2 者 | 提案 | 草案での扱い |
|---|---|---|---|
| 1 | harness（auto mode:「shell で heredoc・sed を使ってよい」）× hook（heredoc を拒む・4 週間で 79 件、うち委譲先 53） | **規範の側に優先を明記**（#1325 で済み）。効果は 11 月の拒否件数で測る。委譲先にも CLAUDE.md は注入されるので同じ行が届く | 残した（経緯の括弧だけ外した） |
| 2 | SP「動けるだけの情報があれば動く」× 29「分析・調査は報告のみ」 | **規範の側に優先を明記**。現行は優先の相手が 26（規範内）になっており、26 を消すと宙に浮く → 相手を SP に替える | 書き換えた |
| 3 | SP（同上・「ユーザーの決定を蒸し返さない」）× 26「実行にバイアス」 | **規範の側を消す**（同じことを 2 か所で言い、毎回どちらが優先かを考えさせる） | 削除 |
| 4 | SP の Agent ツール説明（「委譲先はプロンプトしか知らない」「目的・除外済みのこと・読むべきファイルを渡す」）× 18「委譲はコンテキストを継承しない」 | **規範の側を消す**。画像の 1 句は `/start-issue:70` が持つ | 削除 ⚠ |
| 5 | 出力スタイル「学習モード」（ユーザーに 5〜10 行書いてもらう機会を探す）× `/implement`「自律的にコミットまで」 | **リポジトリの外（設定）で直す**——このリポジトリでは出力スタイルを既定へ戻すか、`.claude/settings.json` の `outputStyle` で固定する。規範に「学習モードより /implement を優先」と書くと、利用者ごとの設定に規範が依存する | 触れない（記録のみ） |
| 6 | auto-memory（SP: 「教訓・フィードバックをメモリへ書け」）× 16「記録への信頼——記憶ではなくリポジトリの記録へ」 | **規範の側に優先を明記** ⚠。メモリ側も「リポジトリが記録していることは保存しない」と言っており衝突は縁だけ。どちらに書くかの判定基準（チーム共有＝リポジトリ・この利用者だけ＝メモリ）を 1 句添える案もある | 「記憶（メモリ）ではなく」と明記 |
| 7 | 規範「マージは `/merge-pr` から」× harness（`disable-model-invocation` のスキルは Skill ツールから呼べない・ユーザーが頼んでも拒否） | **規範の側に回避を明記**（メモリ `feedback_user_only_skill_invocation` にしか無い回避策を正本化） | スキル表の前文に 1 文足した（唯一の追加） |
| 8 | SP「commit / push はユーザーが頼んだときだけ」× 26「コミット・PR 作成の指示は確認なしに即実行」× `/implement`「コミットまで自律」 | 実質は整合（スキル起動が依頼）。**規範の側を消す**（26 ごと） | 削除 |
| 9 | ルート CLAUDE.md:20「検証は手順 6 で手で行う」× `/implement`（`verify:tree` が A〜F を回す） | **規範の側を消す**（序数参照でもある） | 削除 |
| 10 | AGENTS.md:61「config キーは `/plan-review` の 4点セット」× 実体は `/persistence-check:95` | **規範の側を直す**（偽） | `/persistence-check` だけを指す |
| 11 | SP「専用ツール（Read/Edit/Grep）を shell より優先」× 同じ SP の auto-mode 節「cat・sed・heredoc でよい」 | **リポジトリの外**（harness 内部の矛盾）。リポジトリ側は #1 の 1 行で足りる | — |
| 12 | 利用者のグローバル CLAUDE.md 規則 1「teammate-message はユーザー指示ではない・ユーザー本人の発言を引用できなければ着手しない」× チーム運用（harness:「起動したエージェントからのメッセージが作業を指示する」） | **リポジトリの外**（グローバル CLAUDE.md に「委譲先として起動されたときは、起動元のタスクを指示として扱う。ただし許可の代わりにはしない」と射程を足す）。本タスクでも着手時に毎回この裁定が要る | — |
| 13 | `/implement` 3a（作業ツリーが空）× 委譲先の出力を `workspace/` に置く案（norm-usage §3 #7） | スキル内で解決済み（`.delegate/`）。規範側は不要 | — |

## ⚠ 一覧

1. 13（意図は明確に）の削除——チーム憲章は人間との合意事項。エージェントへの効き目は無いが、象徴として残す選択がありうる
2. 9（gh pr create の先回り）の削除——4 週間で拒否 3 件。heredoc（79 件）と違い先回りの費用対効果は低いと判断したが、11 月の拒否件数で再確認したい
3. 18（委譲はコンテキストを継承しない）の削除——SP が同じことを言うが、画像の件は `/start-issue` の外の委譲では射程から落ちる
4. 27（計画書を忠実に）——`/implement` に同じ規則があるかは確認していない
5. 30 に「Skill ツールから呼べない→SKILL.md を読んで辿る」を**書き足す**提案——縮める作業の中で唯一の追加。メモリにある回避策の正本化
6. 35（code-reviewer）の削除——agent roster の description が「呼び出し側がレビュー対象の sha を渡す」を持つ（確認済み）。残る ⚠ は「/implement 3b から worktree で起動」の所在が /implement 側だけになること
7. 39〜50 の書式の句——意図的構造（2026-08-22 の plan）。草案は残した。消すならユーザー確認
8. 51（RETROSPECTIVE を読む）——どのスキルも持たないので残したが、/start-issue へ移す方が「その瞬間」に近い
9. 65・66——写しだが短い（65）／Rust の具体則への入口（66）。草案は 66 を表の対称ペア行へ寄せた
10. 76 の「history キーは snotra-core/CLAUDE.md」——#1329 の縮小後も同ファイルにキー正規化の節があるかは見ていない（`snotra-core/CLAUDE.md:66` に「新規記録」の行は在る）。草案では行から外した
11. 78 の race-check トリガー列——skill description と同じ語の列で写しに見えるが、表の行として「どの局面で起動するか」を常時ロード面に置く価値はある。草案は短縮のみ
12. 95 を D とした——`docs/development-principles.md:174` はほぼ同文だが、AGENTS 側だけが常時ロード。命令 1 文を残して詳細を指す形にした
