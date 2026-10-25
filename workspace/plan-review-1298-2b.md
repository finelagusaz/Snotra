# plan-review Step 2b（独立導出）— issue #1298

- 対象: #1298 `chore(skills): /implement の委譲から A〜F の実行を外し、worktree の切り出しをスクリプトへ移す`
- 測定時点: ブランチ `chore/implement-verify-worktree-script` @ `ae35a515`（作業ツリーは `workspace/` のみ未追跡）
- 読んでいないもの（指示どおり）: `workspace/plan.md`・`workspace/research.md`・`workspace/adversarial-1298.txt`
- grep の母集団: `git grep`（追跡ファイル）。`/implement` を含むパターンは Git Bash のパス変換で壊れるため `MSYS_NO_PATHCONV=1` で取り直した（初回は 0 件を返した——下の ⚠️0）

---

## 1. 導出の骨格（なぜこのファイル群になるか）

実測事実から、成り立つ形は次に絞られる。

1. **委譲先は `isolation` 無しで起動するしかない。** `isolation: "worktree"` の委譲先は別の木へ EnterWorktree できず、Write も自分の木の外を拒む。道具が切った木を「渡す」には、isolation 無しで起動し、**絶対パスで木を名指す**しかない（cwd は呼び出しごとに本体へ戻るので、相対パス・素の `git diff` は本体を読む）
2. **木の置き場は `.claude/worktrees/` 配下に限られる。** そこでは Node が本体の `node_modules` を解決する（実測）。外へ置くと `npm test` が起動前に落ち、変異検知と同じ exit 1 になる（`.claude/rules/safety-nets.md:27`）。`.claude/worktrees/` は既に gitignore 済み（`.gitignore` の `# Agent worktrees` 節）で、`governance:check` の走査（`scripts/governance/lib.mjs:39` `WALK_EXCLUDE_PATHS`）と lsp-config の走査（`.claude/hooks/lsp-config.mjs:43` `WALK_EXCLUDE`）からも外れている
3. **木の寿命を harness ではなく道具が持つ。** harness の木は「ignore 済みファイルしか無ければ停止時に片づく」ため、巡をまたいだ warm な `target/` は偶然（未追跡レポートが残ったとき）にしか得られなかった。道具が `--detach` で切った木は harness の掃除対象外であり、2 巡目は `checkout --detach <新 sha>` で warm に再利用できる——ADR が「未実測」とした残余（`docs/adr/ADR-implement-verification-delegated-to-worktree.md:32`）がここで構造的に決着する
4. **出力先は木の外にする。** ただし本体の `workspace/` に置くと、本体の `git status --porcelain` が空でなくなり 3a の前提（`.claude/skills/implement/SKILL.md:90`）と衝突する。→ 下の判断点 D3
5. **A〜F の「該当」は意味判定であり、道具は決められない。** `SKILL.md:102`（「該当の判定は拡張子ではなく、変更が触れるコードパスの意味で行う」）・`docs/build-commands.md:60`（D の過大判定も害）。道具は**カテゴリを引数で受け取る**形になる（判断は主に残り、実行と記録だけが構造へ上がる）

---

## 2. 変更ファイルとシンボルの一覧

### 2.1 新設

| ファイル / シンボル | 内容 | 根拠 |
|---|---|---|
| `scripts/<verify-tree>.mjs`（名前は未定。`verify`は既存 npm script と衝突するので避ける） | sha 受領 → `git rev-parse --verify <sha>^{commit}` で完全 sha へ解決 → 木の作成/再利用 → HEAD 照合 → 選んだカテゴリのコマンドを木の cwd で実行 → カテゴリごとの exit code を要約ファイルへ書く | issue「やること」1 項目目。`.mjs` を推す理由は下の D1 |
| `scripts/<verify-tree>.test.mjs` | 使い捨て repo で実測（`scripts/clean-worktrees.test.mjs:26` `initRepo` と `.githooks/githooks.test.mjs` の作法）。`vitest.config.ts` の `include` `scripts/**/*.test.mjs` に自動で載る | `vitest.config.ts:8-12`・`docs/build-commands.md:158`（E の母集団の正本は include） |
| `package.json` `scripts.<name>` | **必須**。`/implement` の `allowed-tools` は `Bash(cargo *)`・`Bash(npm *)`・`Bash(git *)` だけで（`.claude/skills/implement/SKILL.md:5-8`）、`node scripts/x.mjs` / `pwsh -File` を直接は許していない。npm ラッパーが無いと主が道具を呼ぶたびに許可を求められる | `SKILL.md:5-8`・`package.json:6-24`（`clean:worktrees` が同型の先例） |
| ADR（`docs/adr/ADR-<slug>.md`） | 却下した代替案（委譲先が自分で木を切る／A〜F を委譲に残す／単一の木を道具と委譲先で共有、など判断点で却下した側）。**旧 ADR の決定本文は変えず、日付付きの追記か新 ADR で覆す**（旧 ADR 自身が 2026-08-19 の追記節を持ち、`ADR-canonical-heading-references.md:29` も日付付き追記の先例） | `SKILL.md:131`（否定の知識は ADR へ）。旧 ADR の決定（`ADR-implement-verification-delegated-to-worktree.md:16`）を部分的に覆す |

### 2.2 変更（散文・規範）

| ファイル:行 | シンボル / 節 | 偽になる主張 | 根拠 |
|---|---|---|---|
| `.claude/skills/implement/SKILL.md:80` | 見出し `## Step 3 — 検証とレビュー（委譲・最大5サイクル）` | 「委譲」が A〜F を含意。改題するかは任意（改題すると 2.3 の参照が動く） | 本文 |
| `SKILL.md:82` | 「カテゴリ A〜F の検証もレビューも、主エージェントの作業ツリーでは行わない。…worktree のサブエージェントへ委ねる」 | A〜F は主が道具で回す形になる。「主の作業ツリーでは行わない」は道具の木ゆえ**真のまま残る**——残す部分と変える部分を分ける | issue「やること」2 |
| `SKILL.md:84` | 「検証とレビューを 1 つの委譲へ束ねるのも同じ理由である——別々に走らせると、2 つの報告が別の版へ向きうる」 | 束ねる理由の置き換え。「両報告が同じ完全 sha を名乗り、主が照合する」へ（issue 未確定 2） | 本文・ADR:22 |
| `SKILL.md:86` | 「ここで委ねるのは権威ある一式である（`cargo doc` と `governance:check` を含む）」 | 委譲から道具へ | 本文 |
| `SKILL.md:88-94` | `### 3a. 委譲の前に` | 道具の呼び出し（と、その前提の `git status --porcelain` 空）がここか 3b に入る。`porcelain` 空の確認は**道具が機械的に行える**（本体の未コミット変更を検知して拒む）——指示文から構造へ上げる候補（D5） | `SKILL.md:90` |
| `SKILL.md:96-109` | `### 3b. 委譲へ渡すもの` | 98 行 `isolation: "worktree"` で起動→isolation 無し・木の絶対パスを渡す。101 行の HEAD 照合指示→道具が保証するので削除（ただし委譲先が木を読む前の `rev-parse HEAD` 照合は残すかを決める・D4）。102 行「カテゴリ A〜F の実行」→削除し、「緑が何も見ていない」の観点をレビュー項目へ移す（issue 3 項目目）。104 行の変異注入→どの木で行うかを明記（D2）。107 行「出力先は呼び出し側が指定する」→絶対パスで木の外 | 本文・実測事実 1〜4 |
| `SKILL.md:111` | カテゴリ D の目視判定 | 道具は D の対話部分を実行できない。道具の要約で D を「実行不能（人間へ）」と**緑と区別できる値**で出す必要がある | 本文・`docs/build-commands.md:71` |
| `SKILL.md:113-120` | `### 3c. 報告を受けてから` | 115 行「委譲先はビルドとテストと変異注入を実ツリーで走らせるため、同時に触れば衝突する」——委譲先も道具も別の木で走るので**理由が偽になる**。規範（触らない）を残すなら理由を「報告の sha == HEAD を保つため」（ADR:25 の直列化の理由）へ差し替える。117 行「同じエージェントへ新しい sha を SendMessage」→道具の再実行 + 委譲先の再開の 2 本になる。118 行「3b の委譲のうち該当するもの」→道具も母集団へ | 本文 |
| `SKILL.md:138` | 「出力」の「検証の母集団は…カテゴリ A〜F 全件、レビューの母集団は…3b の委譲」 | 検証表の根拠が「道具の要約ファイル」になる | 本文 |
| `SKILL.md:146-149` | 出力テンプレート `### 検証（カテゴリ A〜F 全件）` | 根拠欄に要約ファイルのパスと sha を書かせる形へ | 本文 |
| `SKILL.md:169` | 「委譲の行には、アンカーにした sha と変異注入の結果を書く」 | 検証の行にも道具が名乗った sha を書く（両報告の sha 一致が「束ね」の代替なので、一致を出力で見せる） | issue 未確定 2 |
| `CLAUDE.md:65`（ルート） | 「`/implement`「3b. 委譲へ渡すもの」が worktree で起動し、…（検証も同じ委譲が担う）」 | **括弧内が偽になる**。「worktree で起動し」も isolation 無しへ変わるので偽 | 本文 |
| `CLAUDE.md:35-44`（ルート）「サブエージェント委譲と worktree」 | issue 5 項目目の「不要になる条項の特定」 | 各条項の判定は下の 2.4 | — |
| `.claude/agents/code-reviewer.md:11` | 「worktree はコミットから切られ作業ツリーが空なので、`git diff HEAD` は空を返す」 | 委譲先の cwd は本体（isolation 無し）になるため、**理由が偽**。本体では `git diff HEAD` は空とは限らない。さらに 2「変更ファイルの全文を読んで」は**本体の作業コピーを読む**（sha とずれうる）——木の絶対パスか `git show <sha>:<path>` を読ませる指示が要る | 実測事実 1 |
| `.claude/agents/code-reviewer.md:12` | 「`workspace/plan.md` が存在すれば読み」 | 本体の `workspace/plan.md`（未コミットの消し込みを含みうる）と sha 時点の版がずれうる。どちらを正本にするか明記 | 同上 |
| `.claude/skills/start-issue/SKILL.md:82` | `` `/implement`「3b. 委譲へ渡すもの」の委譲先が行う `` | 変異注入を委譲に残すなら真のまま。3b を改題したら参照だけ直す（G-heading-refs が落とす） | 本文 |
| `scripts/race-boundaries.mjs:15` | `` `/implement`「3a. 委譲の前に」から…呼ばれ `` | 3a を改題したら直す（スクリプトのコメントに書いた正準形も G-heading-refs の走査元に入る——正本は `.claude/rules/governance-docs.md` の「走査元は `.md` だけではない」・#925・#1138） | 本文 |
| `docs/check-skill-skeleton-design.md:119` | `` `/implement`「3a. 委譲の前に」の列挙 `` | 同上（改題したときだけ） | 本文 |
| `docs/build-commands.md` | (a) 道具の npm script を「Windows のみ実行可能」索引（`:187-215`）へ足す（C の smoke を含むため Windows 専用。`npm run X` を書けば G-build-commands が `package.json` との実在を照合する——`scripts/governance/checks/G-build-commands.mjs:37-40`）。(b) 道具がカテゴリ節を**機械で読む**なら、散文にしか無い前提をフェンスへ上げる（D1） | `docs/build-commands.md:51`（C の前の `cargo build -p snotra` は箇条書きにしか無い）・`:161`（`scripts/lib/**` を触ったら `test:powershell` も、は箇条書きにしか無い） |
| `scripts/clean-worktrees.mjs:33-36` と `.test.mjs` | 掃除対象は basename が `agent-` で始まる木だけ。道具の木の名前が `agent-*` でなければ `npm run clean:worktrees` は**拾わない**（`--detach` なのでブランチ掃討 `:81` も関係しない）。道具自身に破棄モードを持たせるか、こちらのフィルタを広げるか（D6） | 本文 |
| `.gitignore`「Agent worktrees」節のコメント | `Agent isolation: "worktree"` と書いてある。道具の木も `.claude/worktrees/` に置くなら説明が狭くなる（無害だが古くなる） | `.gitignore` |

### 2.3 見出し参照の母集団（改題時に直す先）

`MSYS_NO_PATHCONV=1 git grep -n '/implement'` の結果から、`/implement` の Step 3 系の見出しを正準形で指す**生きた層**は次の 4 件だけ（ADR は凍結ゆえ対象外）:

- `CLAUDE.md:65` — 「3b. 委譲へ渡すもの」
- `.claude/skills/start-issue/SKILL.md:82` — 「3b. 委譲へ渡すもの」
- `scripts/race-boundaries.mjs:15` — 「3a. 委譲の前に」
- `docs/check-skill-skeleton-design.md:119` — 「3a. 委譲の前に」

（`start-issue/SKILL.md:175` の「3b の 1 体」は **start-issue 自身の 3b（敵対的調査）**であり無関係。誤って直さない）

### 2.4 ルート `CLAUDE.md`「サブエージェント委譲と worktree」の条項ごとの判定

| 条項（`CLAUDE.md:37-44`） | この変更後 | 理由 |
|---|---|---|
| 並列委譲はファイル境界で衝突を予測 | 残す | 実装委譲の話で無関係 |
| 委譲はコンテキストを継承しない | 残す。**むしろ効く**——木の絶対パスは主にしか無い事実になる | 実測事実 1 |
| 検査対象を変更しながら検査を走らせない（#992） | 残す | 道具と委譲先が同じ木を共有する設計なら、まさにこの条項が問題になる（D2） |
| 委譲した検査の成果物は呼び出し側が指定したパスへ | 残す（3b の出力先を木の外にする変更の根拠そのもの） | — |
| 長時間の委譲は中断を前提に | 残す | — |
| レビューの委譲では意図的に分けた構造を先に渡す | 残す | — |
| 指摘 0 件の報告は名乗る対象（sha）と突き合わせ | 残す。**道具の要約にも同じ照合を当てる**（要約が名乗る sha と主の HEAD） | — |
| ⚠️ も返り値に・逆向き監査 | 残す | — |
| 採るのは所見であって機序ではない | 残す | — |

→ **消せる条項は見つからなかった。** 消える候補は `/implement` 本文側（`SKILL.md:101` の HEAD 合わせ指示、`:102` の A〜F 実行、`:115` の衝突理由）であって、ルート `CLAUDE.md` 側では `:65` の括弧の訂正だけ。⚠️ 見出し名「…と worktree」は残ってよい（`AGENTS.md:78` が同名で指す）

### 2.5 触らない / 変更不要と判断したもの

| ファイル | 理由 |
|---|---|
| `docs/adr/ADR-implement-verification-delegated-to-worktree.md` の決定本文 | 変えない。覆す内容は新 ADR（または旧 ADR 末尾への日付付き追記）に書く |
| `docs/adr/ADR-harness-executable-path-derivation.md:8`・`ADR-implement-step4-report-slot.md` など ADR 内の `/implement` 参照 | 同上（ADR 内の参照は照合されない） |
| `AGENTS.md:49`（手順 6「カテゴリ A〜F から該当するものをすべて実行する」） | 実行主体を言っていないので真のまま |
| `AGENTS.md:78`（委譲・worktree → ルート CLAUDE.md） | 見出しを変えなければ真 |
| `docs/build-commands.md` のコマンド本体（A〜F のフェンス） | 正本はここに残る。道具は**読むか、引数で受けるか**であって写さない（D1）。フェンスへの追記は D1 の選択次第 |
| `scripts/lib/SnotraSmoke.psm1`・各 smoke の `-ExePath` 導出 | 既定値は `$PSScriptRoot` 起点（`SnotraSmoke.psm1:392-396`）なので、**道具が木を cwd として `npm run smoke:*` を叩けば、木の本体を測る**。変更不要。ただし道具は必ず木の中の `scripts/` を起動すること（本体の `npm run` を叩くと本体のバイナリを測る） |
| `.claude/hooks/pre-bash.mjs` | `cd` の判定は `gh pr create` の前だけ（`pre-bash.mjs:324-327`）。`git worktree add`／`git -C` を拒む判定は無い |
| `.githooks/` | post-checkout 系の hook は無い（`ls .githooks`）。道具は木でコミットしない |
| `RETROSPECTIVE.md:27` | サイクルごとに上書きされる記録。触らない |
| `docs/superpowers/**` | 履歴資料 |

---

## 3. 道具のライフサイクルと異常系（「対象が無いのに緑」を塞ぐ）

| 局面 | 必要な挙動 | 塞ぐ穴 |
|---|---|---|
| sha の受領 | `git rev-parse --verify <arg>^{commit}` で完全 sha に解決し、**要約に完全 sha を名乗らせる**。`HEAD` やブランチ名も解決後の sha で固定する | 動く参照を渡して「どの版か」が後から分からない |
| 本体の未コミット変更 | 本体の `git status --porcelain` が空でなければ止める（または警告して要約に記す）。3a の指示文を構造へ上げる | `SKILL.md:90` の「コミットし忘れが緑で返る」（#922） |
| 作成 | 木が無ければ `git worktree add --detach <path> <sha>` | — |
| 再利用 | 登録済みなら `git -C <path> status --porcelain` が空であることを確かめてから `checkout --detach <sha>`。**dirty なら止める**（委譲先の変異注入の巻き戻し漏れ・未追跡レポートを検知器として使う。黙って `--force` しない） | 前巡の変異が残った木で検証して緑（または赤） |
| 照合 | checkout 後に `git -C <path> rev-parse HEAD` == 解決済み sha を確かめ、違えば exit≠0 | 現状 20/20 で起きている HEAD 不一致 |
| 迷子 | ディレクトリはあるが未登録／登録はあるがディレクトリが無い → `worktree prune` の扱いを決める。黙って作り直さない | — |
| カテゴリ 0 件 | 選択が空なら exit≠0（「何も走らせずに緑」） | 対象ゼロで自明に緑（#922 と同型） |
| コマンド 0 件 | 選んだカテゴリから導いたコマンドが 0 本なら exit≠0（パーサが節を見失った形。`sectionOf` の①〜④と同じ思想） | 見出しの改題でパースが空になる（`scripts/governance/lib.mjs:357` に同型の実測あり） |
| 途中失敗 | 1 本落ちても残りを走らせて全カテゴリの exit を記録し、総合 exit≠0 | 最初の赤で止まり、他カテゴリの状態が不明 |
| 中断 | 要約は**各コマンドの終了ごとに追記**する（中断しても実施済み分が残る） | `CLAUDE.md:42`（長時間の委譲は中断を前提に）と同型 |
| D | 「実行不能（対話）」を exit code とは別の値で記録。緑扱いしない | `SKILL.md:165`（実行不能の定義） |
| C の前提 | `cargo build -p snotra` を C より前に木で打つ。木の `target/` は初回 cold で、古い本体が在る形は再利用時に起きる | `docs/build-commands.md:51`（#835） |
| Snotra 常駐 | C の smoke は既存プロセスを Stop/Reject する（`SnotraSmoke.psm1:530-552`）。道具の実行中に委譲先が smoke を回すと衝突。開発者が常駐させている Snotra も止まる | 実測事実 7 |
| 破棄 | Step 4 で木を消す手段（道具の `--remove` か `clean:worktrees` の拡張）。未消去の木は `target/` 分のディスクを食い続ける | D6 |
| `CARGO_TARGET_DIR` | 設定されていると木と本体が target を共有し fingerprint が往復する（ADR:24 が却下した形）。道具は値を検知して要約に記すか、木では unset する | ⚠️3 |

---

## 4. テストと、検知器が効くことを確かめる変異

使い捨て repo（`clean-worktrees.test.mjs` の `initRepo` 形）で、**カテゴリのコマンドを差し替え可能**にして（実 cargo を走らせない）測る。各テストに「壊すと落ちる」変異を対にする。

| テスト（不変条件） | 確かめる変異（この変異で赤くなること） |
|---|---|
| 存在しない sha で exit≠0・木を作らない | `rev-parse --verify` を外す |
| 作成後の木の HEAD == 解決済み sha | `--detach <sha>` の sha を落とす（既定 HEAD で切れる） |
| 再利用時、2 つ目の sha へ移る | `checkout` 呼び出しを消す（**現状の 2 巡目以降の不一致**の再現） |
| 木が dirty なら再利用を拒む | porcelain 検査を消す |
| カテゴリ 0 件で exit≠0 | 0 件の分岐を消す |
| 1 カテゴリが exit≠0 → 総合 exit≠0、他カテゴリも記録される | 早期 return を入れる |
| 要約が完全 sha と各カテゴリの exit を持つ | 要約の sha 欄を HEAD の短縮形にする |
| （D1 でパースを採るなら）節の見出しが無い／フェンスが空 → exit≠0 | `docs/build-commands.md` のコピーで見出しを改題する |
| D は「実行不能」で記録され、総合を緑にしない | D を exit 0 として記録する |

- 道具が `.mjs` なら `npm test`（E）に載る。`.ps1` なら `scripts/run-pester.ps1` が `scripts/lib` を見るので `scripts/lib/*.Tests.ps1` に置かないと載らない（`docs/build-commands.md:161`）
- `.claude/rules/safety-nets.md:25` に従い、変異は複製に当てる（ライブの道具を壊さない）

---

## 5. 設計上の判断が分かれる点と推奨

### D1. コマンドの出所: `docs/build-commands.md` を読むか、道具に持つか
- (a) 実行時に `docs/build-commands.md` のカテゴリ節のフェンスを読む。`scripts/governance/lib.mjs` の `sectionOf`（見出し 0 件・2 件・終端の崩れを `body: null` で赤にする fail-closed 実装）が再利用できる。**ただしフェンスだけでは足りない**: C の前の `cargo build -p snotra`（`:51`）と `scripts/lib/**` 変更時の `test:powershell`（`:161`）は散文にしか無く、D のフェンスは対話コマンド（`smoke:manual`・`:63`）、A のフェンスは条件付き行（`必須（snotra-core を変更した場合）`・`:19-22`）を含む
- (b) 道具にコマンド列を持つ → `docs/build-commands.md` の写しになり、ずれは誰も見ない（照合する G 検査が無い）
- **推奨: (a)。** 散文の前提 2 件をフェンスへ上げ（`cargo build -p snotra` を C のフェンスの先頭へ、`npm run test:powershell  # 必須（scripts/lib/** を変更した場合）` を E のフェンスへ）、条件付き行は「条件の文字列」を要約へそのまま出して主が該当を選ぶ（または A の `cargo test -p` は全 crate を走らせる——CI と同じで費用は増えるが判定が要らない）。D は道具の対象外として「実行不能」固定
- ⚠️ フェンスへ上げると `cargo build -p snotra` が C の「必須コマンド」として CI 対応表（G-ci-table）の照合に関わるかは未確認（G-ci-table は「CI/CD メモ」の表だけを読むので無関係と読んだが、測っていない）

### D2. 変異注入をどの木で回すか（issue 未確定 1）
- 道具の木を委譲先と共有すると、道具の A〜F 実行中に委譲先が注入すれば**検証が変異を測る**（ルート `CLAUDE.md`「検査対象を変更しながら検査を走らせない」#992 の再演）。smoke の単一インスタンス制約もある
- **推奨: 同じ道具で名前違いの木を 2 本切る（例: 検証用・レビュー用）、両方に同じ完全 sha。** 委譲先は木を作らない（issue の意図）が、自分専用の木を持つ。費用は cold `target/` が 2 本（初回のみ・再利用で warm）。直列（道具が終わってから木を渡す）も成り立つが、cold ビルド 6 分級を毎巡待たせる
- 委譲先には smoke（C）を回させない、と 3b に書く（道具が回すので）

### D3. 出力先
- 本体の `workspace/` に書かせると、本体 `git status --porcelain` が非空になり次の 3a と衝突する。session scratchpad に書かせると、**issue「比べ方」（次の 10 PR で所見件数・High 採用を数える）の測定経路が消える**——今回の実測は PR の head に残った `workspace/verify-*`／`review-*` を復元して行った
- **推奨: 本体の `workspace/` 配下の絶対パスに書かせ、主が次のコミットへ含める**（3a の「plan.md の消し込みを同じコミットへ」と同じ扱い）。比べ方の計測を成り立たせる唯一の形
- ⚠️ 要約に実行ログを含めるなら、利用者の実パスが載らないかを確かめる（`AGENTS.md` の足場の行: 公開リポジトリで `workspace/` は squash 前の履歴へ残る）。cargo の出力は木の絶対パスを含む

### D4. 委譲先が木の HEAD を照合する指示を残すか
- 道具が照合済みなので冗長に見える。しかし木は委譲先が読む時刻までに、主が次の巡で道具を再実行すれば動く（D2 で木を分ければ委譲先の木は委譲先しか動かさない）
- **推奨:** 木を分けるなら、委譲先には「最初に `git -C <木> rev-parse HEAD` が渡された sha と一致することを報告に書け」の 1 行だけ残す（合わせる操作は道具の責務、名乗るのは報告の責務）

### D5. 道具に本体の `porcelain` 空を強制させるか
- 強制すると、本体に `workspace/` の未追跡ファイルがある（今まさにそう）だけで止まる。3a の文言は「空であること」なので整合はするが、D3 の出力を未コミットのまま次の巡へ入ると毎回止まる
- **推奨:** 強制する（exit≠0）。止まったら「コミットせよ」の復帰手順を文言に持たせる（pre-bash の拒否文言の作法・ルート `CLAUDE.md`「フック」）

### D6. 木の破棄
- `clean-worktrees.mjs` は `agent-*` しか見ない。候補は 3 つ: (i) 道具に破棄モードを持たせ Step 4（`SKILL.md:127-134`）へ 1 行足す、(ii) 道具の木を `agent-verify-*` のように命名して `clean:worktrees` をそのまま流用する（`--detach` なのでブランチ掃討 `:81` は無関係。`clean-worktrees.test.mjs:6` の不変条件「dirty は `--force` なしで消さない」は harness 固有ではなく、そのまま守られる）、(iii) フィルタを広げる。**推奨: (ii)**——コードを足さずに済み、dirty 保護もそのまま効く。ただし harness の `agent-*` と名前空間を共有するので、衝突しない接頭辞にする。Step 4 へ `npm run clean:worktrees` を 1 行足す
- ⚠️ Step 4 の前に Step 3 で 5 サイクル中止した場合にも木が残る。中止経路にも破棄を置くか

### D7. 「緑が何も見ていない」観点の移し先
- issue 3 項目目。**推奨:** 3b のレビュー項目へ「道具の要約（パス）を渡し、各カテゴリの緑がこの diff の経路を実際に通ったかを読む」と書く。委譲先が要約を読むには D2/D3 で要約の置き場が先に決まっている必要がある（**道具が先、委譲が後**の順序が生じる——並走させるなら、要約を読む観点は 2 巡目以降にしか効かない）
- ⚠️ これは「直列にしないと観点が効かない」を意味し、D2 の並列推奨と緊張する。判断を分けるのはここ

---

## 6. ⚠️（確信の持てない所見）

- ⚠️0 **grep の取りこぼし**: `git grep '/implement'` は Git Bash で `C:/Program Files/Git/implement` に化けて 0 件を返した（メモリの既知事象）。`MSYS_NO_PATHCONV=1` で取り直した結果を 2.3 に使った。計画側の grep が同じ形で 0 件を「参照無し」と読んでいないか確かめてほしい
- ⚠️0b 概念ラベルの grep（36KB）は一度 `A〜F|/implement|3b|Step 3|worktree|委譲へ渡す|権威` で絞った。落とした行を `変異注入|3c|アンカー` で再走査し、`/implement` 固有の主張を持つ行は 0 件だった（当たったのは G 検査の「アンカー」と ADR 内の変異注入の歴史記述だけ）
- ⚠️1 `code-reviewer` エージェントの `tools` は `Read, Grep, Glob, Bash` で **Write が無い**（`.claude/agents/code-reviewer.md:4`）。3b の委譲先が `code-reviewer` 型なら、出力先へは Bash のリダイレクトでしか書けない。現行の 3b がどの `subagent_type` で起動しているかは SKILL に書かれておらず、測っていない
- ⚠️2 isolation 無しの委譲先は**本体へも書ける**。変異注入を木ではなく本体に誤って当てる経路が新たに開く（isolation が構造的に防いでいた）。3b に「注入先は渡した木の絶対パスだけ」と書くだけでは指示文（強制の階梯の最下段）に戻る。道具側で本体の porcelain を見る（D5）と次巡で検知はできる
- ⚠️3 `CARGO_TARGET_DIR` が開発機で設定されているかは未確認。設定されていれば木と本体の `target/` 共有で fingerprint が往復し、ADR:24 が避けた形になる。さらに `Resolve-SnotraCargoExecutable` は `target_directory` に追随するので、**共有 target では木の smoke が本体と同じ exe を測る**（どちらの版かは最後にビルドした側で決まる）
- ⚠️4 Node の `node_modules` 解決は「`.claude/worktrees/` 配下では本体のものを解決する」と実測済みだが、**本体の `node_modules` のバージョンが sha 時点の `package-lock.json` と一致する保証は無い**（依存更新 PR で木の lock が新しい場合、旧い `vitest` で走る）。`/deps-update` の PR でだけ効く穴
- ⚠️5 道具の木を `.claude/worktrees/` 配下に置くと、harness の `isolation: "worktree"` の木と同じ親を共有する。harness が停止時の自動片づけで**名前の衝突しない他の木まで触るか**は未測定（触らないと推定）
- ⚠️6 `cargo test -p <crate>` の「変更した場合」を道具が `git diff --name-only main...<sha>` から導く案は、`main` が並行セッションで進む前提（メモリ: セッション運用）で merge-base がずれると対象 crate が増減する。全 crate を走らせる案のほうが判定が要らない
- ⚠️7 issue の「比べ方」（PR あたり所見 12・High 1.2 から落ちないこと）は、委譲先が A〜F を回さなくなると**「緑が何も見ていない」型の所見が委譲先の読みから出るかどうか**に依存する。この観点が要約を読めない並走構成では落ちうる（D7）
