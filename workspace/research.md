# research — issue #488: (A2) の残り（`gh pr merge --squash` / `gh issue close` の誤 close）

## 1. issue の要約

設計文書 §2 は「(A2) 外部 API への不可逆呼び出しは Claude Code の hook だけが見える領域」と定義し、`gh pr create` と並べて `gh pr merge --squash` の誤 close・`gh issue close` を挙げた。#482（PR #487）は `gh pr create` だけを実装し、残りを YAGNI として見送った。本 issue はその残りを追跡する。

受け入れ条件（issue 本文より）:

1. `gh pr merge --squash` の誤 close を hook で検知できるか、**原理的な可否を実測で判断する**
2. 守るなら `pre-bash.mjs` の `decide` へ統合する（新しい hook を増やさない）
3. 守らないなら、その判断根拠を `CLAUDE.md` に記録して本 issue を閉じる

**本ファイルは調査結果のみを述べる。** 方針の決定は `plan.md` で行う。

---

## 2. 前提の裏取り — auto-close の経路は 1 本ではなく 2 本ある

issue と `CLAUDE.md` はいずれも「ブランチ各コミット本文の `Fixes/Closes #N` を squash 時に GitHub が拾う」という**単一の経路**を前提にしている。実測すると経路は 2 本あり、**文書が制御しているのは危険でない方**だった。

### Channel 1 — PR 本文の closing keyword（GitHub が「リンク」として計算）

| 測定 | コマンド | 結果 |
|---|---|---|
| PR #491 の本文 | `gh pr view 491 --json body` | 59 行目に `Closes #489` |
| PR #491 のマージ時刻 | `gh pr view 491 --json mergedAt` | `2026-07-10T01:45:29Z` |
| issue #489 の close 時刻 | `gh issue view 489 --json closedAt` | `2026-07-10T01:45:30Z`（**1 秒後**） |
| main に載った squash commit の本文 | `git log -1 --format=%B c79c3a1` | **`Closes` は無い**（`(#489)` は裸の参照） |
| close イベントの `commit_id` | `gh api .../issues/489/timeline` | `null`（＝コミット由来ではない） |

**この merge は `--body-file` で本文を明示していた。** それでも #489 は閉じた。

> **結論 A: `--subject` / `--body-file` は Channel 1 を抑止しない。** `CLAUDE.md`「Git/GitHub 運用」の手順 1（`--body-file` で `Closes`/`Refs` を制御する）は、**実際に issue を閉じている経路を制御していない**。手順 2（マージ後の `gh issue view --json state` 検証）だけが Channel 1 を捕捉している——事後に。

### Channel 2 — squash commit 本文（既定 = ブランチ全コミット本文の連結）

| 測定 | 結果 |
|---|---|
| `gh api repos/:owner/:repo --jq .squash_merge_commit_message` | **`COMMIT_MESSAGES`** |
| 同 `.squash_merge_commit_title` | `COMMIT_OR_PR_TITLE` |
| 同 `.allow_squash_merge` / `.allow_merge_commit` / `.allow_rebase_merge` | `true` / `false` / `false`（**squash が唯一の手段**） |
| 既定本文の実例（`--body-file` 無しでマージした `be7a8ed`） | `* chore: workspace 調査・計画 (issue #481)` `* refactor(hooks): …` ＝ 連結 |

つまり issue の主張どおり、`--body-file` を渡さなければブランチ全コミットの本文が squash commit に流入する。**この経路はリポジトリ設定 `squash_merge_commit_message` が生んでいる。**

---

## 3. 危険の実体を数える — Channel 2 ∖ Channel 1

「PR がリンクしていないのに、コミット本文が閉じてしまう issue」が **Channel 2 ∖ Channel 1** である。これが (A2) の事故の実体。

直近 30 件のマージ済み PR について、`closingIssuesReferences`（Channel 1）と**ブランチ各コミットの `messageBody`**（Channel 2 の入力）を突き合わせた（`scratchpad/channel-diff2.sh`）:

```
PR#493  linked=[]     branchCommits=[]
PR#491  linked=[489]  branchCommits=[489]
PR#487  linked=[482]  branchCommits=[482]
PR#486  linked=[481]  branchCommits=[481]
PR#480  linked=[]     branchCommits=[]
...（30 件すべて）
差分 (Channel 2 ∖ Channel 1): 0 件
```

> **結論 B: 30 件中 0 件。** ブランチのコミット本文が、PR 本文のリンクを超えて issue を閉じたことは一度も無い。運用上、`Closes #N` は PR 本文とコミット本文の**両方に同じ N** で書かれている。

### 「実害 1 件」とされた #480 を直接見る

issue は「実害は『#480 のマージで意図しない issue が閉じかけた』1 件に留まる」と述べる。実測:

- PR #480 の**ブランチ全コミット本文**の `#N` 参照は `Refs #471, #473` / `Refs #473` / 裸の `#473/#475/#476/#477/#479` のみ。**`Closes`/`Fixes` は 1 件も無い**
- ゆえに `--body-file` を渡さず既定の連結本文でマージしても、**閉じる issue は 0 件だった**
- #471 / #473 の close イベントはいずれも `commit_id: null`（＝別経路。#473 は `gh issue close` を人が実行した）

> **結論 C: 引用された実害は再現しない。** Channel 2 による誤 close は、この repo の歴史上ゼロ件であり、根拠とされた事例でも発火しえなかった。

---

## 4. hook から何が見えるか（原理的可否）

### 現状の `pre-bash.mjs` の挙動（生 probe）

```
{"tool_name":"PowerShell","tool_input":{"command":"gh pr merge 488 --squash --delete-branch"}}  → exit 0（素通り）
{"tool_name":"Bash","tool_input":{"command":"gh issue close 488"}}                              → exit 0（素通り）
```

`GH_PR_CREATE` に一致しないため「管轄外 = allow」。設計どおり。

### 検知は可能。ただし「何が閉じるか」までで、「それが誤りか」は分からない

hook が `tool_input.command` から取れる情報と、追加の照会:

| 必要な情報 | 取得手段 | 実測コスト |
|---|---|---|
| PR 番号 | コマンド引数、無ければ現在ブランチから `gh pr view --json number` | — |
| Channel 1 の集合 | `gh pr view <N> --json closingIssuesReferences` | **0.73s**（`commits` 同時取得込み） |
| Channel 2 の集合 | `--body`/`--body-file` があればその本文、無ければ `gh pr view <N> --json commits` の連結 | 同上 |

閉じる集合は**正確に計算できる**。しかし hook は「どれを閉じるつもりだったか」という**意図の真実源を持たない**。ゆえに `deny`（exit 2）は原理的に書けず、できるのは「これを閉じます」と提示して人に委ねること＝ `ask` まで。

### `ask` は `pre-bash.mjs` の fail-closed 骨格と衝突する

公式ドキュメント（サブエージェント調査。**引用であって実測ではない**）:

- `permissionDecision: "allow" | "deny" | "ask" | "defer"` を **stdout の JSON** で返せる
- **JSON 出力は exit 0 を要求する**。exit 2 との併用は「混ぜるな」と明記
- **JSON パース失敗時に allow / block どちらへ倒れるかは記載なし**
- `ask` 時に `permissionDecisionReason` がユーザーへ表示されるかも**記載なし**

`pre-bash.mjs` の骨格は「既定 `process.exitCode = 2`、許可が確定した経路だけが 0 を書く」（原則 7「失敗方向は既定値に埋め込む」）。`ask` を返すには exit 0 を書かねばならず、**JSON が壊れた瞬間に fail-open** へ倒れる可能性がある（挙動は未文書＝未測定）。#482 が根治した失敗様式そのものを、`ask` の導入は再び持ち込む。

### hook の視界は「エージェントがツール経由で打つコマンド」だけ

設計文書 §2 の三層表がそう定義している（Layer 0 = すべての経路 / Layer 2 = ツール呼び出しのみ）。マージの実行経路:

| 経路 | hook から見えるか |
|---|---|
| エージェントが `Bash`/`PowerShell` で `gh pr merge` | ✅ 見える |
| **ユーザーが自分の端末で `gh pr merge`** | ❌ 見えない |
| **GitHub の Web UI で "Squash and merge"** | ❌ 見えない |

このセッションだけでも、ユーザーは PR #487 を**手動でマージした**（「手動でマージしました」）。hook は最も使われる経路を守れない。

> **結論 D: 検知は原理的に可能。だが (a) 意図が無いので `deny` できず、(b) `ask` は fail-closed 骨格を壊し、(c) 視界が実運用の経路を覆わない。**

---

## 5. `gh issue close` について

- hook が見るのは `gh issue close <N>` という文字列。**どの issue を閉じるべきかの真実源が存在しない**（PR と違い、GitHub 側に「閉じる予定の集合」が無い）
- `.claude/settings.local.json` の `permissions.allow` に `gh` は 1 つも無い。ゆえに `gh issue close` は**すでに権限プロンプトが出る**（コマンド文字列＝issue 番号がそのまま表示される）
- hook を足しても「番号を再掲する」以上のことができない

> **結論 E: `gh issue close` に hook を足す余地は無い。** 既存の権限プロンプトが提示している情報を超えられない。

---

## 6. 選択肢空間（前提の裏取りで広がったもの）

issue は「hook で守る / 守らずに根拠を記録する」の二択を提示する。実測により第三の選択肢が現れた。

| 案 | 層 | Channel 1 | Channel 2 | Web UI / 手動端末 | コード |
|---|---|---|---|---|---|
| **(i) hook で `ask`** | Layer 2 | 提示できる | 提示できる | ❌ 盲 | `pre-bash.mjs` に ~60 行 + `ask` 骨格の穴 |
| **(ii) 文書を訂正するだけ** | 書き手の記憶 | 手順で見る | **手順が 2 つ要る** | ✅（人が従えば） | 0 行 |
| **(iii) `squash_merge_commit_message` を変える** | **Layer 0** | 変わらず（元から可視） | **経路ごと消滅** | ✅ すべての経路 | 0 行 |

(iii) の意味: `COMMIT_MESSAGES` → `PR_BODY` あるいは `BLANK` にすると、squash commit の本文はブランチのコミット本文を含まなくなる。したがって

- 閉じる集合 == `closingIssuesReferences` **のみ**（GitHub が計算し、PR ページに表示し、`gh pr view --json closingIssuesReferences` で読める）
- マージ前チェックが **1 コマンドで十分**になる。閉じたくない issue があれば **PR 本文を編集する**（`--body-file` ではない）
- Web UI からのマージにも効く

(ii) 単独だと、Channel 2 が残るためマージ前チェックは 2 コマンド必要（`closingIssuesReferences` と `commits[].messageBody`）。**後者は人間が忘れる方**である。

### (iii) の副作用

| 設定値 | `--body-file` を渡さずにマージしたときの squash 本文 |
|---|---|
| `COMMIT_MESSAGES`（現状） | ブランチ全コミット本文の連結（`* …` 箇条書き）。`Closes` が紛れうる |
| `PR_BODY` | PR の説明文そのもの（表・チェックリストを含み冗長。ただし要約としては読める） |
| `BLANK` | 空（タイトルのみ。`(#N)` で PR へ辿れる） |

いずれも `--subject` / `--body-file` による明示は従来どおり効く。既定値が変わるだけ。

---

## 7. 影響範囲

### 触る候補

- `CLAUDE.md`「Git/GitHub 運用」— squash auto-close 制御の手順（**結論 A により事実誤り**）。および「(A2) を hook で守らない」判断根拠の記録（受け入れ条件 3）
- `docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md` §2 — 「`gh pr merge --squash` の誤 close も `gh issue close` も同じ（hook だけが見える）」という一文（**結論 D により不正確**。この文書は「改訂: 実装中の実測により…」を持つ as-built 追従型）

### 触らない

- `.claude/hooks/pre-bash.mjs` / `pre-bash.test.mjs` — (i) を採らない限り変更不要
- `.claude/settings.json` — matcher は既に `Bash|PowerShell`。変更不要
- `SPEC.md` — 製品の挙動ではない（エージェント運用）。更新不要
- `snotra-core` / `src-tauri` / `ui` — 無関係

### 対称ペア

`gh pr create`（既に守られている）↔ `gh pr merge` / `gh issue close`（本 issue）。**対称なのは「外部 API の不可逆呼び出し」という分類だけで、性質は対称ではない**:

| | `gh pr create` | `gh pr merge` / `gh issue close` |
|---|---|---|
| 誤りの判定に必要なもの | **リポジトリの状態**（未 push コミットの有無）— hook が git に問える | **人の意図**（どれを閉じるつもりか）— どこにも無い |
| 判定結果 | 二値（空 PR になる / ならない） | 集合の提示のみ |
| 主な実行者 | エージェント | **人**（Web UI・手動端末を含む） |

`pre-bash.mjs` が `gh pr create` を守れるのは、**誤りの定義がリポジトリ状態として観測可能**だからである。この性質は merge / close に無い。

---

## 8. 未解決の疑問

1. **`permissionDecision: "ask"` の実挙動は未測定。** ドキュメントの引用のみ。JSON パース失敗時に allow へ倒れるかは未文書。測るには一時的な hook 配線（＝エージェント設定の変更）が要り、合意が必要
2. **`squash_merge_commit_message` を変えたとき、Channel 2 が本当に消えるか**は GitHub の仕様であり、設定を変えずに実測できない（変更は可逆。`gh api -X PATCH` で戻せる。repo 権限は `admin: true` を確認済み）
3. #480 で「閉じかけた」と記録された事象の一次記録が見つからない。commit 本文は `Refs` のみで、危険は当時から存在しなかった可能性が高い
