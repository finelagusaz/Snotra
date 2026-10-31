# #1312 の変更対象の独立導出

導出者: サブエージェント（`workspace/plan.md`・`workspace/research.md`・`workspace/adversarial-1312.txt` は読んでいない）
基準: ブランチ `chore/merge-pr-tool`（HEAD = `fb0914a3`、`git diff main...HEAD` は空。未追跡は `workspace/` のみ）
要件の出所: `gh issue view 1312`（本文 + ユーザー裁定コメント 1 件。裁定は 2 項目）と `gh issue view 1304`

## 1. 対象 issue

- **#1312**（本体）。裁定により **#1304 を吸収**する（マージ直前に「道具の要約の sha → PR の head」に `workspace/` の外の変更が無いことを照合する。判定は hook と共有し、写しを作らない）
- 裁定 2: `closingIssuesReferences` に閉じたくない issue が在れば **道具は止めて人へ返す**（本文の編集を道具は行わない）
- 関連のみ: #488（auto-close 事故の原点）・#1299 / #1303（PR 前ゲート）・#1302（PR 後に足したコミットの実例）

## 2. 変更が必要なファイル

### 2a. 新設

| ファイル | 対象 | 要旨 | 根拠 |
|---|---|---|---|
| `scripts/merge-pr.mjs`（名前は issue の例。§4 D1 参照） | 新しい道具一式: 引数解析（`--subject` / `-t` / `--body` / `-b` / `--auto` を受け付けない）、`gh` の注入点（`spawn = spawnSync` を引数にとる形）、マージ前の確認、`gh pr merge <PR> --squash --body-file <f> --match-head-commit <headRefOid>`、マージ後の 3 点検証、終了コード | issue「やること」1〜3。骨組みの手本は `scripts/prose-sweep.mjs`（冒頭の契約コメント・`UsageError`・`USAGE`・注入できる `runGrep(term, { cwd, spawn = spawnSync })`（:95）・`process.argv[1].endsWith(...)` で直接起動を判定（:180）・shebang を置かない理由（:3-4）） |
| `scripts/merge-pr.test.mjs` | 偽の `gh` を注入した変異テスト | issue「やること」3 と `.claude/rules/safety-nets.md`「フォールトインジェクション」。変異は最低限: 非 ISO の `mergedAt`・`--subject` を渡す・body-file に closing keyword・検算なしの 0 件。#1304 分: 要約の sha の後に `workspace/` の外の変更・要約が無い・要約が別ブランチ・PR の head が手元に無い・照合とマージのあいだに head が動いた（`--match-head-commit` が渡っていることの検査）。裁定 2 分: 一覧に `--closes` の外の issue が在る → マージを呼ばずに止まる（`gh pr merge` の呼び出しが 0 回であることを assert）。`vitest.config.ts:10` の `scripts/**/*.test.mjs` が自動で拾い、`.github/workflows/ci.yml:45`（ubuntu）と `:173`（windows）の `npm test` で走るので CI の変更は要らない |

### 2b. 既存ファイルの変更

| ファイル | 対象シンボル / 箇所 | 要旨 | 根拠 |
|---|---|---|---|
| `.claude/hooks/pre-bash.mjs` | `readVerifyState`（:453-497） | **#1304 の判定共有のための分割。** 今は `git branch --show-current`（:465）でブランチを、`HEAD`（:484）で比較先を決め打ちしている。マージの照合では branch = PR の `headRefName`、比較先 = `headRefOid` が要る。ブランチ導出の後ろ（要約の読み込み → `judgeSummary` → 2 端点の `git diff --name-only --no-renames -z <sha> <target>` → `workspace/` 外の抽出）を、`{ root, branch, target }` を受ける export 関数へ切り出し、`readVerifyState` はそれを `target: "HEAD"` で呼ぶ薄い殻にする（または `readVerifyState` に options を足す）。道具はそれを import する | 裁定 1「判定は hook と共有し、写しを作らない」。`judgeSummary`（:433）は branch を引数に取るのでそのまま共有できる。共有できないのは :465 と :484 の 2 点だけ |
| `.claude/hooks/pre-bash.mjs` | `VERIFY_REMEDY`（:425-427） | 文言が `--sha HEAD` と「変更をコミットしてから」を前提にしている。マージ時の拒否では「PR の head（手元の HEAD と同じとは限らない）」に対して回し直す案内が要る。共有関数が理由文を返す以上、文言を消費者ごとに変えるか、中立な文言にするかを決める（§4 D7） | :426 |
| `.claude/hooks/pre-bash.mjs` | 冒頭コメント（:1-6）・`readVerifyState` の JSDoc（:449-452） | 「`gh pr create` 検出後のみ」の説明は hook の発火の話なので真のまま。共有関数の JSDoc に「消費者は hook と `scripts/merge-pr.mjs` の 2 つ」を書く（写しを作らない宣言の置き場） | :449-452 |
| `.claude/hooks/pre-bash.test.mjs` | `describe("readVerifyState — 本物の git と要約ファイル")`（:409-501） | 切り出した関数へ `target` に HEAD 以外の sha を渡すケースを足す。既存の `readVerifyState` のケースはそのまま緑であることを測る（#1299 の options 化と同じ作法・`ADR-pr-gate-verified-sha.md:17`） | :410 `readVerifyState(dir, summaryPathFor)`。fail-closed カナリア（:684-720）は `invokedDirectly` ブロックと `decide(…platform…)` の形だけを縛っているので、この分割では動かない |
| `package.json` | `scripts` | `"merge:pr": "node scripts/merge-pr.mjs"`（名前は §4 D1） | 既存の道具は `verify:tree` / `race:boundaries` / `prose:sweep`（:11-14） |
| `.claude/skills/merge-pr/SKILL.md` | 全体 | 手順を「道具を走らせて出力を読む」へ縮める（issue「やること」4）。**偽になる散文**: 見出し「この手順が唯一の防御である理由」（:16）と :26「機構がここに無いので、下の手順が唯一の防御である」。手順 1〜4（:34-95）は道具の中身に移るので、残すのは「人へ返されたときに何をするか」（本文の編集 → 再実行。:40-50 の ⚠ 9 形の注意と「判定は一覧であって自分のキーワード走査ではない」は人の作業として残る）・`--subject` を使わず件名は PR タイトルで直す（:58）・PR タイトルの型名 + 番号の注意（:62-64。道具が見ないなら残す）・誤って閉じたときの reopen（:97-101）・`--auto` 不使用（:107）・`--delete-branch` の節（:109-119。道具が持たないなら残す）。frontmatter: `description`（:3）・`allowed-tools`（:6-9 の `Bash(gh *)` に道具の起動形を足す。`/plan-review` の `Bash(npm run prose:sweep *)` が前例） | issue 本文「事実」・「やること」4 |
| `docs/adr/ADR-squash-merge-issue-autoclose.md` | 決定 2（:15）・却下「hook で merge / close を守る」（:22）・帰結（:27） | issue「経緯は ADR-squash-merge-issue-autoclose.md へ寄せる」。決定 2「残余は手順で守る」は「user 起動の手順が呼ぶ道具で守る」に変わる。ADR は凍結された歴史（`ADR-adr-frozen-history`）なので決定 2 を書き換えずに**追記節**で足すのが慣例に合う（この ADR は 2026-07-28 改訂の前例を本文中に持つ）。:22 の「hook が守る外部 API の不可逆呼び出しは `gh pr create` だけ」は hook を変えないなら真のまま。追記に載せる却下案: 道具が本文を編集する（裁定 2）・hook で `gh pr merge` を検出する（#1304 の未確定の第 1 案・D5）・照合の相手を手元の HEAD にする（#1304 の未確定 2） | issue「やること」4・§4 D8 |
| ルート `CLAUDE.md` | :24「フックが守る外部 API の不可逆呼び出しは `gh pr create` だけである。`merge` / `close` の残余は `/merge-pr` の手順が持つ」 | 「手順が持つ」→ 道具（`/merge-pr` が走らせる `scripts/merge-pr.mjs`）が持つ、へ。道具を経ない `gh pr merge` の扱い（issue 未確定 3）をここに 1 句で書く | issue 未確定 3 が名指し。**規範文書ゆえ最重要ルール 2（セーフティネットの変更は合意してから）の対象**——裁定 1・2 はこの行の変更までは明示していない（⚠ §5） |
| ルート `CLAUDE.md` | :60 スキル表の `/merge-pr` 行「closingIssuesReferences 確認 → 本文編集 → マージ後 3 点検証」 | 「本文編集」は道具の仕事ではなくなる（裁定 2）。行の記述を「道具で確認 → マージ → 事後検証、止まったら本文を人が直す」へ | :60 |
| ルート `CLAUDE.md` | :16「squash マージは `/merge-pr` の手順で行う」 | 「手順」の語は残しても偽ではない（手順が道具を呼ぶ）。⚠ 変えるかは任意。:23 の「止める条件の正本は … `judgeSummary` / `readVerifyState`」は、切り出し後に正本の関数名が変わるなら直す（G-stale-identifiers は関数名が残っていれば鳴らない＝**名前が残る限り赤にならずに偽になりうる**） | :16・:23 |
| `docs/hooks.md` | 「要約ゲート」の項（:16） | 「止める条件の正本は `judgeSummary` / `readVerifyState` である」に、同じ判定を `scripts/merge-pr.mjs` がマージ直前に PR の head へ当てることを 1 句足す（写しの所在を書いておかないと、片方だけ変える将来が見えない） | :16 |
| `docs/build-commands.md` | 「Windows/macOS/Linux で実行可能」のフェンス（:182-187）と直後の箇条（:189-190） | 新しい `npm run merge:pr …` を 1 行足す（`prose:sweep` の前例）。**PowerShell では `npm run` を経由しない**注意（:190 で #1300 が実測した `npm.ps1` が最初の `--` を食う件）が道具にも当たるかは測ってから書く | G-build-commands（`scripts/governance/checks/G-build-commands.mjs:35-38`）は `docs/build-commands.md` の `npm run X` が `package.json` に在ることを要求する。**一覧に載せること自体は強制されない**（慣例） |

### 2c. ガバナンス検査が新しい語・コマンドに要求すること（実測で確かめた範囲）

- **G-build-commands**: `docs/build-commands.md` に書いた `npm run <name>` は `package.json` の `scripts` に要る（:35-38）。他の文書の `npm run` は見ない
- **G-references**: バッククォート内の `/` を含み拡張子 `.mjs` 等のパス（`scripts/merge-pr.mjs`）は実在が要る（`G-references.mjs` の `REF_EXTENSIONS`）。新ファイルと文書を同じ PR に入れれば足りる
- **G-stale-identifiers**: `.claude/skills/**`・`.claude/rules/*`・`docs/**`（ADR・superpowers を除く）・`CLAUDE.md`・`AGENTS.md`・`SPEC.md` のバッククォート内の camelCase / SNAKE 識別子（空白も `/` も含まない span）は、**テストでない production ソースの非コメント本文**に現れなければ赤（`G-stale-identifiers.mjs:143-160`）。SKILL.md や `docs/hooks.md` に `` `headRefOid` `` `` `mergedAt` `` `` `closingIssuesReferences` `` を単独で書くなら、道具の中で**文字列リテラルとして**使っていること（コメントだけでは語彙にならない）。切り出した共有関数の名前を文書へ書くときも同じ。過去に `mergedAt` が単独で書かれて finding になった記録がある（`docs/superpowers/specs/2026-08-10-stale-identifier-qualified-form-design.md:95`）
- **G-heading-refs**: `/merge-pr` の見出しを正準形で指す生きた参照は 0 件（`git grep '`/merge-pr`「'` が `docs/adr`・`docs/superpowers`・`workspace` を除いて 0 件）。ゆえに SKILL.md の見出しは改名してよい。CLAUDE.md の「フック」「Git/GitHub 運用」見出しは SKILL.md:24 と ADR から参照されているので改名しない
- **G-adr-citations**: 新しい ADR を作るなら、生きた層からの `ADR-<slug>` 引用は実在するファイル名と一致させる
- `scripts/*.mjs` の一覧を持つ検査は無い（`grep -ln "scripts/" scripts/governance/checks/G-*.mjs` が test を除き 0 件）ので、新ファイルを索引へ登録する義務は無い
- 走らせるもの: カテゴリ F（`npm run governance:check`）と `npm test`

## 3. 変更しないが確認が要るファイル

| ファイル | 確認する点 |
|---|---|
| `docs/adr/ADR-pr-gate-verified-sha.md` | 受容した残余 :22「PR を作ったあとに足したコミットは射程外である」は、道具経由のマージに限って射程内になる。凍結された歴史なので書き換えない。生きた層（`docs/hooks.md`）の側に、残余がどこで拾われるようになったかを書くのが筋。:23「`--out` で既定の置き場の外へ出した要約は見ない」・:24「`gh pr create --head <別ブランチ>`」・:28「linked worktree のセッションからは復帰できない」は道具にもそのまま遺伝する（`findUp(cwd, ".git")` が linked worktree のルートを返すので要約の置き場がずれる → 要約なしで止まる） |
| `scripts/verify-tree.mjs` | `summaryPathFor`（:91）は `(root, branch)` を取るので PR の `headRefName` を渡せば共有できる（変更不要）。`mainContext`（:100-108）は「本体の作業ツリーから打つこと」を強制する——道具も同じ前提（本体から打つ）を名乗る必要がある。`treePathFor` の置換（`[^A-Za-z0-9._-]` → `-`・:96）は branch 名が違えば同じ置き場に衝突しうる（`fix/a` と `fix-a`）が、`judgeSummary` が `summary.branch !== branch` で止めるので安全側 |
| `.claude/hooks/pre-bash.test.mjs` の fail-closed カナリア（:684-720） | 分割で `invokedDirectly` ブロックと `decide(payload, {…platform: process.platform})` の形が変わらないこと |
| `.claude/skills/deps-update/SKILL.md` | :82 は PR 前に道具を回させるので要約は残る。**Dependabot の PR（`dependabot/...` ブランチ）は手元で道具を回していないので要約が無く、道具は構造上止める**（:102-104 は「マージはユーザーが判断する」）。これを残余として受容するか、人が Web UI でマージする経路を明記するか（§4 D6） |
| `.claude/skills/implement/SKILL.md` | :141-142・:144: Step 4 は ADR の回収 → 道具の回し直し → 木の片づけ → `workspace/` の撤去。`verify-tree remove` は要約を残す（`ADR-pr-gate-verified-sha.md:25`）ので、マージ時点で要約は在る。変更不要の見込み |
| `.claude/skills/retrospective/SKILL.md` | :77「手動マージ」を PR 本文チェックリストへ、の記述は道具化後も真 |
| `AGENTS.md` | 「条件別チェック」のセーフティネット行が新しい道具を母集団に含む（`scripts/**` は `safety-nets.md` の paths・:10）。表の変更は不要 |
| `.claude/rules/safety-nets.md` | 変更不要。新しい道具が守る変異の一覧は計画とテストへ（:43「新設する検出器は、設計を確定する前に注入する変異を先に書く」） |
| `.claude/settings.json` | hook の matcher / timeout（30 秒）は変えない（D5 で hook を変えない場合）。道具の起動は hook の `gh pr create` 検出に当たらない |
| `docs/development-principles.md`「構造的設計原則と強制の階梯」 | issue が「最上段（機構が吸収する）へ上げる候補」と書く。`merge-pr` への言及は 0 件（grep）。例として足すかは任意 |
| `.github/pull_request_template.md` | `Closes #<issue_number>` を埋める（:2）。道具の `--closes` との関係を SKILL.md 側で説明するだけでよい |
| `docs/superpowers/**`・他 ADR の `merge-pr` / `closingIssuesReferences` への言及 | 凍結層。変更しない |

## 4. 設計判断が分かれる点と推奨

- **D1 名前**: ファイル `scripts/merge-pr.mjs`（issue の例）、npm script は既存の `<名詞>:<動詞/形>` 揃え（`verify:tree`・`prose:sweep`）に合わせて `merge:pr` か `pr:merge`。推奨 `merge:pr`（スキル名 `/merge-pr` と同じ語順で検索で並ぶ）
- **D2 判定の共有のしかた（#1304）**: (a) 共有関数を `pre-bash.mjs` に置き、道具が import する。(b) `judgeSummary` と照合を `scripts/verify-tree.mjs` へ移し、hook はすでに動的 import しているのでそこから受ける。**推奨 (a)**: 正本の所在を名指す散文（`CLAUDE.md:23`・`docs/hooks.md:16`・`ADR-pr-gate-verified-sha.md:7`）と既存テスト（`pre-bash.test.mjs:19-20` が import）が `pre-bash.mjs` を指しており、動かすと写しの追随が増える。`pre-bash.mjs` は `invokedDirectly` ガード（:547-）を持つので、import しても stdin 読み取りは走らない。(b) の利点は「hook が静的 import で全コマンドを止める」危険とは無関係（道具は hook ではない）
- **D3 比べる相手**: PR の head（`gh pr view <PR> --json headRefOid,headRefName`）。手元に無ければ `git fetch origin refs/pull/<N>/head`（または `headRefName`）で取り、それでも `rev-parse --verify <oid>^{commit}` が失敗すれば止める（exit 2）。要約のブランチは手元の `branch --show-current` ではなく `headRefName` で引く（別ブランチから打っても正しい要約を見る）。**`gh pr merge … --match-head-commit <headRefOid>` を必ず渡す**——照合した sha とマージされる sha を構造で一致させる（照合とマージの間に head へ push されると GitHub が拒む）。これが #1304 で得られる最も強い構造的な利得で、本文の凍結（`SKILL.md:105-107` の残余）は依然として残る
- **D4 意図の入力（裁定 2）**: 道具は「何を閉じるべきか」を知らないと「閉じたくない issue」を判定できない。推奨: `--closes <n,...>`（`none` を許す・省略不可）を必須にし、`closingIssuesReferences` と集合で比べる。**余分があれば止める（裁定 2）**、足りない場合も止める（閉じるつもりの issue が閉じない＝沈黙の失敗）。事後検証 (2)「残すと決めた issue が OPEN か」の母集団は `--keep <n,...>` で受けるか、PR 本文に現れる `#N` から `--closes` を引いて導く。後者は自動だが、本文に現れない issue（関連のみ・別 PR で閉じる予定）を見ない。推奨は「`--keep` を任意で受け、本文中の `#N` − `--closes` を既定で加える」
- **D5 hook の射程（issue 未確定 3）**: 推奨は **hook を変えない**。道具を経ない `gh pr merge` は `--no-verify` と同格の人間専用の迂回として `CLAUDE.md:24` に書く。hook で `gh pr merge` を検出して道具経由を強制する案は、`ADR-squash-merge-issue-autoclose.md:22` の却下理由 (3)「hook の視界は Web UI・ユーザー端末からのマージを覆わない」がそのまま当たり、「hook が守る外部 API は `gh pr create` だけ」という規範（`CLAUDE.md:24`・`docs/hooks.md`）を変える費用に見合わない。ADR 追記の却下案に載せる
- **D6 要約が無いとき（#1304 未確定 3）**: 推奨は fail-closed（止めて人へ返す）。`ADR-pr-gate-verified-sha.md:13` が「要約の無いブランチは管轄外として通す」を却下済みで、同じ判定を共有する以上倒し方も揃える。代償として Dependabot の PR・別の機体で作業した PR は道具では通らない——人が Web UI で（道具の外で）マージする経路を残余として明記する。`--no-verify-summary` のような逃げ口は置かない（`ADR-pr-gate-verified-sha.md:12` の「例外の口」の却下と同型）
- **D7 拒否文言**: 共有関数が返す `reason` に `VERIFY_REMEDY`（`--sha HEAD`）が埋まっている。マージ時は「PR のブランチを checkout して `--sha <headRefOid>` で回し直し、push し直す必要は無い（head は同じ）」が正しい案内。推奨: 共有関数は判定（ok / 理由の核）だけを返し、復帰手順は消費者が付ける。または文言の `HEAD` を「PR の head と同じ sha」へ中立化する。判定の共有と文言の共有を混ぜない
- **D8 経緯の置き場**: issue は `ADR-squash-merge-issue-autoclose.md` へ寄せると書く。この ADR は「決定 2 = 手順で守る」を中心に持ち、裁定で新しい却下案が 3 つ生じる。(a) 同 ADR へ追記節、(b) 新 ADR（例 `ADR-merge-pr-tool`）を作り同 ADR から引く。**推奨 (a)**（issue の指定に従う。凍結は「書き換えない」であって追記の禁止ではなく、同 ADR は 2026-07-28 改訂の前例を本文 :15 に持つ）。ただし決定 2 の本文を書き換えず「その後」の節を足す
- **D9 closing keyword の検査範囲**: body-file への keyword は道具が正規表現で拒む（`SKILL.md:60`「書くと手順 1 の一覧に現れないまま閉じる」）。9 形 + 任意のコロン + `#N` / `owner/repo#N` / issue URL。PR タイトルの `fix: #985` 形（`SKILL.md:62-64`）も同じ判定で見られる——推奨は**タイトルも見る**（安価で、件名はタイトルから組まれるので手順の外で起きる唯一の経路）。正規表現は過剰検出へ倒す（散文の "partially fixes #N" も効く・:60）
- **D10 事後検証 (3) の検算**: `mergedAt` は `--jq .mergedAt` で生文字列を取り、`^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$` を満たさなければ止める（非 ISO の変異）。検算は 2 枝: (1) が非空なら、その issue が `closed:>=<mergedAt>` の結果に現れること。(1) が空なら、**マージ前に**直近に閉じた issue とその `closedAt` を控えておき、マージ後に `closed:>=<その closedAt>` がそれを返すこと（SKILL.md:93 の「既知の閉じた issue を含む境界」を道具が決定的に選ぶ形）。`gh issue list` の既定 `--limit` は 30（`gh issue list --help` で確認）なので明示的に大きくする（切り詰め）
- **D11 終了コード**: `verify-tree` の流儀（0 緑 / 1 赤 / 2 環境・使用法、`scripts/verify-tree.mjs:11-12`）に揃え、さらに「**マージした後の検証で異常**」を 1 と分ける（不可逆の後の失敗は、マージ前に止まった 1 と直し方が全く違う。reopen が要る）。例: 0 / 1 = マージ前に止めた / 2 = 環境・使用法 / 3 = マージ済みだが事後検証が赤または検算不能
- **D12 `--delete-branch`**: 道具が受けるなら `gh pr list --state open --base <branch>` の確認（SKILL.md:109-119）も道具に入れる。受けないなら SKILL.md に節を残す。推奨は**受けない**（スコープを issue の 4 落とし穴に絞る。`--delete-branch` は手元のブランチも消して checkout を動かすので、要約の照合と同じ実行に混ぜない）

## 5. ⚠ 確信のない所見

- ⚠ **GitHub の検索インデックスの遅延**: マージ直後の `gh issue list --search "closed:>=…"` は、閉じた issue をまだ返さないことがありうる（未実測）。検算が「検索が効いていない」と誤判定するので、短い再試行が要る。再試行しても検算が通らなければ「0 件」ではなく「検算不能」として非ゼロで終える
- ⚠ **closing keyword の正確な形**: コロン付き・`owner/repo#N`・issue の URL を GitHub が認めることは SKILL.md と記憶に拠る（`SKILL.md:62` は GitHub docs を引く）。正規表現を書く前に GitHub docs「Linking a pull request to an issue」で形を確かめること
- ⚠ **`gh pr merge` が必須チェック未完了のとき**: `gh pr merge --help` の冒頭は「If required checks have not yet passed, auto-merge will be enabled」と書く。文脈は merge queue の説明だが、merge queue の無いブランチで同じことが起きるかは未実測。起きると `--auto` を使わない規範（`CLAUDE.md:16`）を道具が黙って破る。道具はマージ前に `gh pr view --json mergeStateStatus` 等で「今マージ可能」を確かめるか、マージ後に `state == MERGED` と `mergedAt` の存在を確かめて、マージされていなければ止める
- ⚠ **PR の head が main を取り込んでいる場合**（GitHub の "Update branch" で head にマージコミットが載る）: 2 端点の木の比較なので、取り込んだ main の変更が `workspace/` の外にあれば止まる。fail-closed で、復帰は道具の回し直し（`ADR-pr-gate-verified-sha.md:26` と同じ性質）。運用上の頻度は未確認
- ⚠ **`CLAUDE.md:24` の書き換えが裁定の範囲か**: 裁定は「#1304 を吸収」「人へ返す」の 2 点で、hook の射程（未確定 3）は裁定されていない。D5 の推奨（hook を変えず、`gh pr merge` の直打ちを人間専用の迂回と書く）は常時ロード規範の変更なので、計画段階でユーザーの合意を明示的に取ること（最重要ルール 2）
- ⚠ **PowerShell の `npm run` と `--`**: `docs/build-commands.md:190` が `prose:sweep` で実測した `npm.ps1` が最初の `--` を食う件は、`npm run merge:pr -- --closes 1312 …` にも当たる見込み。`--closes` が npm に吸われると「省略不可」の検査で止まる（fail-closed）ので害は小さいが、SKILL.md の起動形を `node scripts/merge-pr.mjs` にするか測ってから決める
- ⚠ **共有関数の git のタイムアウト**: `VERIFY_GIT_TIMEOUT_MS = 4_000`（:423）は hook の 30 秒予算のための値。道具が `fetch` を含めて同じ関数を使うと、大きな fetch は 4 秒に収まらない可能性がある。fetch は共有関数の外（道具側）に置き、共有関数は手元に在る 2 sha の diff だけを持つのがよい
- ⚠ **`--match-head-commit` の失敗時の出力形**: head が動いたときの `gh` の exit code と stderr を実測していない。テストの偽 `gh` に再現させる形は、一度実物を観測してから決める
- ⚠ **`ADR-pr-gate-verified-sha.md:22` の残余**: 凍結層なので直さないが、読者は「PR 後のコミットは射程外」と読み続ける。生きた層（`docs/hooks.md`）への 1 句で十分かは計画レビューで判断
