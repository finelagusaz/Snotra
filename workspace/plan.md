# 計画: #1334 docs/hooks.md を、テストとコードのコメントに載らない契約だけに絞る

## 目的と受け入れ条件

hooks.md の条項を置き場所の順（テスト → コメント → 文書）で振り分け直す。振り分けの結果、文書に残るべき条項は 3 つだけで、いずれも lsp-config.mjs の先頭 doc に置けるとわかったので、**hooks.md は撤去する**（ユーザー裁定）。

- hooks.md にしか無かった事実が、すべて移した先（コード・テスト・ADR）にあり、PR 本文の削除台帳で 1 条項 1 行ずつ追える
- 安いテスト 3 種（下記）が入り、それぞれ規則を破る変異で落ちる
- hooks.md を指す生きた参照が 0 件（ADR・`docs/superpowers/`・テストの fixture 名を除く）
- `npm test`・`npm run governance:check` が緑

振り分けの根拠と変異の実測は `workspace/research.md`。

## 変更ファイルと対象

| ファイル | 変更 |
|---|---|
| `.claude/hooks/pre-bash.mjs` | 先頭コメントに A3 の「判定単位は 2 種類（コマンド位置／コマンド全体）。後者は §6 からの意図的な逸脱で、理由は言及と実行を分ける構文的位置が無いこと」を足す（非 ASCII 判定はコマンド位置の python を要する点を正確に書く）。`gitSegments` の doc に行継続（B4b）、`SEPARATOR` 付近のコメントに「継続の扱いを変えると `gh pr create` ゲートも動く」（B4c）、`BACKSLASH_PATH` のコメントに `.\` 相対形と接頭辞なしの受容（B4e）を足す。`judgeCommandShape` の doc の「第 4 引数」「経路は 1 本だけ」を直す（B2） |
| `.claude/hooks/pre-bash.mjs`（定数） | `GIT_TIMEOUT_MS`（46 行目）と `VERIFY_GIT_TIMEOUT_MS`（423 行目）を `export` にする（テスト ② が import する） |
| `.claude/hooks/pre-bash.test.mjs` | 安いテスト: ① `usesNoVerify("echo git commit --no-verify") === false`（コマンド位置要件）を `usesNoVerify` の describe に足す。② `readVerifyState` の describe に接頭辞のテスト（`workspaceX/` のパスは外として止める）を足し、予算のテスト（`VERIFY_GIT_TIMEOUT_MS * 2 + GIT_TIMEOUT_MS * 2 < .claude/settings.json の PreToolUse timeout * 1000`）を「settings.json ドリフト検出カナリア」に足す。現状は 28 秒 < 30 秒。③ 新設せず、既存の「拒否文言は代わりの手段を含む」（1001 行目）を強める: heredoc は `Write ツール`、noVerify は `理由を解消` を assert に足す（noVerify は今「人間専用」しか見ていない） |
| `.claude/hooks/lsp-config.mjs` | 先頭 doc へ移す: C1（RA は navigation の道具で検証の権威ではない。確定判定は fmt / clippy / test）、C4（`.lsp.json` に `diagnostics` を置かない。正本は ADR-ra-diagnostics-suppression）、C6（壊れ方 3 分類の表とバイナリ欠落の実例・守り手）、C8（worktree の残余。サーバ名だけ変えた実測の形と、`.claude/lsp/` を持たない古い枝は公式 plugin へ落ちることを含む）、C9（`settings.local.json` の残余。gitignore 済みで実在検査が ignore 対象を免除すること・#1088 を含む）。8 行目は移した C6 の節への内向きの指しに、121 行目は C8・C9 の節への指しに書き換える |
| `docs/hooks.md` | 削除 |
| `CLAUDE.md`:19 | 「判定の一覧と改修時の契約は `docs/hooks.md`「…」」を「判定と改修時の契約の正本は `.claude/hooks/pre-bash.mjs`（先頭コメントと各判定の doc）とそのテスト」へ |
| `AGENTS.md`:23 | 「ドキュメント参照」の hooks.md の行を削除 |
| `docs/development-principles.md`:78 | 反例の文から hooks.md の半分（「実装契約」の `docs/hooks.md` に完全な行動形の条項があり）を削る。AGENTS.md の例が残るので「反例が実在する」は真のまま |
| `docs/development-principles.md`:89 | 表の「改修者向けの実装契約」行を削除する（その役割の面が無くなる。射程宣言の一般論は 79 行目に残る） |
| `docs/development-principles.md`:93 | 括弧内の「と `docs/hooks.md` の行」を削る |
| `scripts/governance/checks/G-stale-identifiers.mjs`:96 | hooks.md を現在形で指す注釈を過去形へ |
| （触らない）`scripts/governance/checks/G-stale-identifiers.test.mjs`:303・305・322 | `docs/hooks.md` は合成 fixture のファイル名で、実ファイルを読まない。ADR 群・`docs/superpowers/` も凍結された歴史として触らない（prose:sweep の★未掲載 22 件はこの 2 種だけ） |

## 実装順序

1. テスト 3 種を先に書く（Red にならないことを確認：いずれも現行コードで緑。そのうえで規則を破る変異で赤になることは委譲先が測る）
2. pre-bash.mjs のコメントへ A3・B4b・B4c・B4e を移し、B2 の偽を直す
3. lsp-config.mjs の先頭 doc へ C1・C4・C6・C8・C9 を移し、指し先を外す
4. hooks.md を削除し、生きた参照（CLAUDE.md・AGENTS.md・development-principles.md・G-stale-identifiers.mjs）を直す
5. 撤去の語彙を数え直す（下記）

## 不変条件と異常系

- 移してから消す: 手順 2・3 が終わるまで hooks.md を消さない
- 削除台帳: research.md の振り分け表の行 ID（I0・A1〜A6・B0〜B4g・C1〜C9）ごとに、PR 本文で「移した先」か「消してよい理由」を 1 行ずつ持つ。件数は書かない
- PR 本文に 1 行: ADR 5 本（ADR-claude-code-ra-lsp-plugin-delivery ほか）は「正本は hooks.md」と書いたまま宛先を失うが、凍結された歴史なので触らない
- 手で grep するときは `.claude/worktrees/` を除く（旧 hooks.md の複製がある。git grep には出ない）
- `G-heading-refs` は生きた層から消えた見出しへの参照で赤になる。ADR は走査元から外れている（lib.mjs `headingRefDocs`）

## テスト方針と検証コマンド

- 安いテスト 3 種の変異（コマンド位置アンカーを緩める・接頭辞判定を `startsWith("workspace")` にする・上限を 40 秒にする・拒否文言から代替手段を消す）は `/implement` の委譲先が複製に当てて赤を確かめる
- `npm test`
- `npm run governance:check`
- `npm run verify:tree -- run --sha <HEAD> --categories E,F`
- 撤去の語彙: `git grep -n "hooks\.md"` と `git grep -n "実装契約"` を ADR・`docs/superpowers/` を除いて数え、在る前提の記述が 0 件

## SPEC.md・関連文書

- SPEC.md: 変更なし（開発プロセスの文書で、製品の挙動に触れない）
- `docs/development-principles.md` の 3 行は、次の issue（3 本目）で全体を絞る前に、ここで行き先だけ直す

## 作業項目

### Phase 1: テスト
- [x] `pre-bash.mjs` の 2 定数を export する
- [x] `pre-bash.test.mjs`: ① コマンド位置・② 接頭辞と予算を足し、③ 既存の拒否文言テストを強める

### Phase 2: コードのコメントへ移す
- [x] `pre-bash.mjs`: A3 の逸脱・B4b・B4c・B4e を移し、B2 の偽を直す
- [x] `lsp-config.mjs`: C1・C4・C6・C8・C9 を先頭 doc へ移し、hooks.md への指し先を外す

### Phase 3: 撤去と参照の付け替え
- [x] `docs/hooks.md` を削除する
- [x] `CLAUDE.md`:19・`AGENTS.md`:23・`docs/development-principles.md`:78/89/93・`G-stale-identifiers.mjs`:96 を直す
- [x] 撤去の語彙を数え、在る前提の記述を直す（`hooks.md`・`実装契約` の残りは G-stale-identifiers.mjs:96 の過去形の 1 件と合成 fixture 名だけ）

### Phase 4: 検証
- [ ] `npm test`・`npm run governance:check`・`verify:tree`（E,F）を通す
- [ ] PR 本文用の削除台帳・字数と太字数の前後・分類ごとの件数を `workspace/` 外の一時ファイルへ用意する

## 未確定（実装前に潰す）

- [x] hooks.md を残すか撤去するか — ユーザー裁定: 撤去する（2026-11-01）
- [x] 安いテスト 3 種をこの PR で書くか — ユーザー裁定: 3 種とも書く（2026-11-01）

## 人間レビュー

- [x] 承認済み — 2026-11-01 / 問い: "`workspace/plan.md` に注釈を書き込んでいただくか、「承認」とお返事くださいませ。" / 回答: "Ok"

## plan-review 結果

- リスク: 高
- レビュー方式: 計画準拠レビュー 1 体（Step 2。撤去とテスト追加は issue に無い会話上の裁定のため）。成果物 `workspace/plan-review-hooks-removal.md`
- エージェント数: 1

### 要対処（6 件すべて計画へ反映。根拠は主が再照合）
- 「全 23 条項」が表と合わない → 台帳を行 ID ごとにし、導入 I0 を表へ足した
- テスト ② の定数が非 export → 2 定数を export する行を足した（`pre-bash.mjs`:46・423 で確認）
- テスト ③ は既存の 1001 行目と重複 → 新設をやめ、既存の it を強める形にした（noVerify が「人間専用」しか見ていないことを確認）
- CLAUDE.md:19 の指し先に 5 判定の一覧が無い → 文を「先頭コメントと各判定の doc とそのテスト」へ
- development-principles.md の書き換えが選択式 → 78・89・93 の扱いを 1 つに決めた
- lsp-config.mjs:8 が自己参照になる → 8・121 を移設後の節への内向きの指しにした

### 軽微
- hooks.md:99〜101 の細部を C8・C9 の移設内容に明記した。ADR 5 本の宛先喪失は PR 本文に 1 行残す。`.claude/worktrees/` の旧複製に注意書きを足した
- C1 が lsp-config.mjs を編集する人にしか届かない可能性（⚠️ 未実測）— 受容する。RA の扱いを決めるのはその設定を触る人である

### 未検証
- 書き換え後の governance:check・テストの変異・移設文面 — 実装時に測る

### 判断
- 実装着手: 可（人間の承認待ち）。再レビューはしない: 変更はレビューの指示どおりで、対象ファイルの追加は無い（インターフェースの変化は定数 2 個の export だけ）

## セルフレビュー

- リスク: 高（ガバナンス文書の撤去・規範ルート CLAUDE.md / AGENTS.md の変更）
- plan-review: 独立レビュー 1 体
- エージェント数: 2（3b の 1 体・plan-review の 1 体）
- 要対処: 3b の所見 10 件・plan-review の要対処 6 件を反映
- 未検証: 安いテストの変異（委譲先が測る）
- 散文の全件列挙: 語 5 / 総件数 84 行 / ★未掲載 22（計画へ足した 0。ADR・`docs/superpowers/`・合成 fixture 名で、触らない理由を変更ファイル表に記載）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 2（テスト ③ を新設から既存強化へ・lsp-config の指しを内向きへ） / 主の見落とし 6
