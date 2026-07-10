# research — issue #504: githooks-selftest の故障注入がライブの `.githooks/` を壊している

## issue の要約

`AGENTS.md` は「**稼働中のガードを弱めて測ってはならない——複製に変異を当てる**」と定めている。にもかかわらず #484（PR #501）で `githooks-selftest` を新設したとき、**ライブの `.githooks/pre-commit` を無害化**して検査が赤くなることを確かめた。main 保護の Layer 1 が数十秒間、作業ツリーから消えていた。

求められているのは 2 つ:

1. `.githooks/` を壊さずに `githooks-selftest` の故障注入ができることを、実際に一度やって示す
2. `AGENTS.md` の該当ルールに、実 git を回す検査での手口を一文で追記する

## issue の前提を測り直した（結論: 前提は誤り）

issue はこう述べる:

> `.githooks/githooks.test.mjs` は `$env:TEMP` に使い捨ての git repo を作り、`core.hooksPath` をリポジトリの `.githooks/` に向けて実 git 操作を走らせる。つまり検査対象は「追跡ファイルとしての hook」であり、`post-edit.mjs` のようなソース述語ではない。**メモリ上でコピーして正規表現を当てる、という #482 の手口がそのまま使えない。**

**「そのまま使えない」は正しいが、「複製に変異を当てられない」は誤りである。** 差し替え点はテストファイルではなく `core.hooksPath` にある。`core.hooksPath` は**任意のディレクトリ**を受け取るのだから、使い捨て repo をライブではなく**複製へ向ければよい**。テストへ引数を足す（issue 候補 1）必要も、worktree を切る（候補 2）必要もない。

### 実測（`scratchpad/measure-504.mjs`。ライブの `.githooks/` には一切書き込まない）

| # | 条件 | 観測 | 含意 |
|---|---|---|---|
| C0 | ライブの `.githooks` を `core.hooksPath` に向け、main で commit | **拒否**・stderr に `BLOCKED` | 対照群。ライブは健全 |
| M1 | `.githooks` を**複製**し、複製の `pre-commit` を `exit 0` に置換。main で commit | **通った** | `expectBlocked` が throw ⇒ `githooks-selftest` は赤くなる。**故障注入が複製だけで成立する** |
| M2 | hook が一切無いディレクトリを向け、feature ブランチで commit | 通った | 「通る（誤爆しない）」系のテストは、**総沈黙下でも緑**（vacuous green） |
| M3 | hook が一切無いディレクトリを向け、main で commit | 通った | `expectBlocked` が throw ⇒ **総沈黙はスイート全体では捕まる** |

実行後 `git status --porcelain -- .githooks` は clean。ライブは無傷のまま測れた。

### この 4 行が意味すること

- **`expectBlocked` 系のテストは自己証拠的である**。`BLOCKED` を stderr に書くのは hook スクリプトだけなので、これらは hook が実行されなければ緑になりようがない（M3）。ゆえに「ライブを壊して赤を見る」ことは、そもそも新しい情報を与えていなかった。
- **危ういのは逆で、「通る」系の否定テストである**。hook が総沈黙でも緑になる（M2）。`githooks.test.mjs:237-238` のコメントは既にこれを自白している（「hook が一切起動しなくても通るため、これは総沈黙の検知にはならない」）。
- したがって、この PR が足す故障注入テストは**否定テスト**（「変異させたら通る」）になる。**M2 の罠にそのまま嵌まる形である**。同一テスト内に「変異なしの複製 → 拒否」という対照を置いて初めて、緑の原因を「変異したから」に特定できる。

## 関連コード

| ファイル | 役割 | 本 issue での扱い |
|---|---|---|
| `.githooks/githooks.test.mjs` | Layer 1 の実測テスト（`it()` は 16 本。うち `enableHooks()` でライブを向くのが 13 本、V10 の 3 本は既に**複製**を向く）。`githooks-selftest` の実体 | **変更しない**（→ `plan.md`「初版から反転した判断」） |
| `.githooks/{_lib.sh,pre-commit,pre-merge-commit,pre-rebase,pre-push}` | 判定対象の hook 本体 | **変更しない**（読むだけ） |
| `.claude/hooks/post-edit.mjs` | `rel.startsWith(".githooks/")` → `githooks-selftest`（L147）／`vitestSpec(".githooks")`（L295） | 変更しない（発火条件は不変） |
| `.claude/hooks/post-edit.test.mjs` | `selectChecks(".githooks/pre-commit")` のカナリア（L97-100） | 変更しない |
| `vitest.config.ts` | `include` に `.githooks/**/*.test.mjs` | 変更しない |
| `AGENTS.md` L60 | 故障注入ルール（「複製に変異を当てる」） | **変更**（一文追記） |

## 既存パターン（再利用できるもの）

**手口はこのファイルの中に既にある。** `githooks.test.mjs:226-253`（V10・worktree テスト）は

- `cpSync(HOOKS_DIR, path.join(dir, ".githooks"), { recursive: true })` で**複製を作り**
- `makeExecutable()` で実行ビットを付け（win32 では no-op）
- `writeFileSync(path.join(dir, ".githooks", "pre-commit"), "#!/bin/sh\nexit 0\n")` で**複製側を無害化**して対立仮説を反証する

つまり「複製に変異を当てる」作法は **#484 の時点で同じファイルの下半分に書かれていた**。上半分の `enableHooks()`（`core.hooksPath` をライブへ向ける）だけを見て、手口が無いと判断したのが誤りだった。

**さらに強く言える**: V10 は毎回この手口を**実行している**。ゆえに手口の生存を証明する新テストを足すのは、V10 の手書き重複である（`plan.md`「初版から反転した判断」の根拠 3）。

## 技術的制約

- **`git config` の値に `\` を入れない**（既存コメント L16-17）。`mkdtempSync` は Windows で `\` 区切りを返すため、`core.hooksPath` へ渡す前に posix 化が要る。
- **`cpSync(HOOKS_DIR, ...)` は `githooks.test.mjs` 自身も複製する**。git は `core.hooksPath` 配下の「hook 名に一致するファイル」しか起動しないので無害（V10 テストが既に同じことをしている）。
- **`makeExecutable` は関数宣言なので巻き上げられる**（L218 定義・前方で呼べる）。win32 では early return。
- テスト 1 本あたり git を 5〜7 回起動する。既存 `T = 20_000` のタイムアウトを踏襲する。
- `.githooks/**` の編集は `githooks-selftest` を自動発火する（`post-edit.mjs:147`）。**このファイルを編集した瞬間に検査が走る**ので、Red→Green の確認は hook の報告そのもので取れる。

## AGENTS.md の該当文の欠陥

現行文（L60）:

> ただし**稼働中のガードを弱めて測ってはならない**——**判定がソース述語なら**、ライブのファイルではなく**複製に変異を当てる**

「**判定がソース述語なら**」という条件節が、読者に「実 git を回す検査は適用外だ」と推論させる。#484 のわたくしはまさにそう読んだ。これは #495 で潰したのと同じ**条件性含意**の構造である（`AGENTS.md:37` が「網羅性が要件のタスクでは Step 2b を」と書き、読者に「それ以外は省ける」と推論させていた）。

規則を無条件にし、実 git を回す検査での差し替え点（`core.hooksPath`）を名指しするのが正しい直し方である。**免除条件を書くと、免除を必要とする当人がそれを判定する。**

## 未解決の疑問

なし。設計判断は 2 点あり、いずれも `plan.md` で決めた:

1. 4 つの hook すべてに変異テストを足すか → **足さない**。M3 が示すとおり、各 `describe` の `expectBlocked` テストは hook の沈黙で必ず赤くなる。「**守るのは沈黙する経路だけでよい**」（`AGENTS.md`、#500）
2. 故障注入を「一度だけ手で実施して記録する」か「テストとして恒久化する」か → **恒久化しない**。調査時点では「`機構 > 規範`」を根拠に恒久化する気でいたが、`/plan-review` の独立導出が (a) 規範は「1 度確かめる」としか要求していない、(b) 判断 1 と同じ規則がここにも当たる、(c) V10 が既に手口を実行している、の 3 点で否定した。検算して受け入れた（→ `plan.md`）
