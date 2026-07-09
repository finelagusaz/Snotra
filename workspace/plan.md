# plan — issue #481（Phase 1b: block-main-commit の削除）

## 変更ファイル一覧（5 ファイル）

| ファイル | 変更内容 |
|---|---|
| `.claude/settings.json` | `PreToolUse[0].hooks` の**第 0 要素**を削除。PR 前 push チェック（第 1 要素）・PostToolUse・`enabledPlugins` は不変 |
| `.claude/hooks/post-edit.mjs` | `validateSettings` 直前のコメント 1 行。`block-main-commit` → `PR 前 push チェック`。**振る舞い不変** |
| `CLAUDE.md` | 最重要ルール 2 を narrow / 「Git/GitHub 運用」の 2 項目（narrow + **理由の差し替え**）/ フック表直前の一文削除 / フック表の 1 行削除 |
| `.claude/skills/start-issue/SKILL.md` | `:38` と `:117` の「チェーンせず」記述を「以下を順に実行する:」へ置換（**`:117` の「セッション断絶に備え…」は残す**） |
| `.claude/skills/implement/SKILL.md` | `:81` の括弧内「（`git add` と `git commit` はチェーンせず…）」を削除。`:80` の「フックにも弾かれる」を `.githooks/pre-commit` に明示 |

### CLAUDE.md の確定文面

`:16`（最重要ルール 2 — **番号は保つ**）
```
2. **`gh pr create` を他のコマンドとチェーンしない** — PR 前 push チェック hook はコマンド実行の**前**に upstream を評価するため、`git push -u origin HEAD && gh pr create` は必ずブロックされる（→「Git/GitHub 運用」）
```

`:41`（narrow。`Phase 2` は CLAUDE.md にとって死語なので **#482** と書く）
```
- **`gh pr create` を他のコマンドとチェーンしない** — PR 前 push チェック hook は `tool_input` 全体を grep したうえで、コマンド実行の**前**に `@{u}` を評価する。`git push -u origin HEAD && gh pr create` は upstream 未設定と判定されて必ずブロックされる（この誤爆の根治は #482）
```

`:42`（**削除ではなく理由の差し替え**。`.githooks/pre-merge-commit` は制約を定めるが手順を定めない。CLAUDE.md からレシピが消えると、`/start-issue` を経ない ad-hoc 作業で main を同期する手順が引けなくなる）
```
- **main の同期は `git pull --ff-only` を使う** — 非 FF の `git pull` は main にマージコミットを作るため `.githooks/pre-merge-commit` が拒否する。FF ならマージコミットが生じず hook は呼ばれない
```

`:49`（末尾の一文のみ削除。「main 保護の実体はここではない」は残す）
`:53`（`block-main-commit` の行を削除）

### スキルの確定文面

`start-issue/SKILL.md:38` — 全文を置換（削除するとコードブロック 3 つが理由なく並ぶ）
```
以下を順に実行する:
```

`start-issue/SKILL.md:117` — **2 文が同居している。後半だけを置換する**
```
セッション断絶・別マシン継続に備え、`workspace/` を必ずコミットしてプッシュする。以下を順に実行する:
```

`implement/SKILL.md:81` — 括弧書きのみ削除
```
- このタスクで変更したファイルのみをステージする
```

`implement/SKILL.md:80` — 「フックにも弾かれる」→ 担い手を名指しする（**スコープ過剰候補だが正当化する**: `block-main-commit` 削除後、「フック」は Claude Code hook と git hook の間で曖昧になる。担い手を名指しすることは本設計の主題そのものであり、既に編集するファイル内の 1 節で済む）
```
（`main` 直コミットは禁止・`.githooks/pre-commit` に弾かれる。`/start-issue` を経ていない単体起動時に該当しやすい）
```

**触ってはならない**: `implement/SKILL.md:51` は cargo/clippy の「チェーン実行を推奨」であり、git のチェーンとは別概念（同名・別概念）。

## 実装順序

**文書は実装の後を追う。** 削除を先に行い、実測が緑になってから文書を直す。逆順にすると「まだ存在する hook を、存在しないと書いた文書」が一時的に生まれる。

### Phase 1 — hook の削除と検証（`.claude/**`）

1. `.claude/settings.json` の第 0 要素を削除
2. **明示的に JSON を検証する**（沈黙に頼らない）:
   `node -e "JSON.parse(require('fs').readFileSync('.claude/settings.json','utf8')); console.log('valid JSON')"`
3. `.claude/hooks/post-edit.mjs` のコメントを事実訂正
4. `npx vitest run .claude/hooks` が緑

### Phase 2 — 実環境の故障注入（main セッションが実行・コミットなし）

`.githooks/` は既に `main` のツリーにある（#480 のマージ）。よって削除しても Layer 1 が守る。

| # | 操作 | 期待 | 何を証明するか |
|---|---|---|---|
| **F1** | **Bash tool** から main 上で `git commit --allow-empty` | `BLOCKED: main への直接コミットは禁止です。`（`.githooks/` の文面） | ガードの担い手が git 側へ移った。**メッセージの出自が変わることが証拠** |
| **F2** | **Bash tool** から main 上で `git merge --ff-only origin/main` | `Already up to date.` | **誤爆の消滅**。旧 hook なら `BLOCKED` だった |
| **F3** | **PowerShell tool** から main 上で `git commit --allow-empty` | `BLOCKED`（`.githooks/`） | 不変であること（削除で退行していない） |

F1 と F3 でメッセージが**同一**になることが要点。旧 hook のメッセージ（`BLOCKED: main ブランチへの直接コミットは禁止です。feature ブランチを作成してください。`）とは文面が異なるため、どちらが止めたかを出力で判別できる。

**F1〜F3 は main ブランチ上で実行するが、コミットは 1 つも作らない**（すべて拒否される／FF で up to date）。

### Phase 3 — 文書（`CLAUDE.md` + スキル 2 本）

Phase 2 が全て緑になってから着手する。落ちていたら Phase 1 を revert する。

## 不変条件

| # | 不変条件 | 破れたときに起きること | 検知手段 |
|---|---|---|---|
| **I1** | `.claude/settings.json` は妥当な JSON である | **全 hook が停止する**（PostToolUse を含む）。しかも JSON 検証器（`validateSettings`）は停止する側の hook の中にあり、**settings.json が壊れたとき PostToolUse が発火するかは未実測** | **明示的な `JSON.parse`**（Phase 1 手順 2）。「沈黙 = 合格」に頼らない |
| **I2** | PR 前 push チェックは残る | 空 PR / `Closes` 誤 close の防止機構が消える | `settings.json` に `gh\s+pr\s+create` が 1 件残ることを grep で確認 |
| **I3** | `PostToolUse` と `enabledPlugins` は不変 | 編集後の自動検証が壊れる | `git diff` で当該行に変更が無いこと |
| **I4** | 最重要ルールの番号は 1〜4 のまま | #473/#475/#476/#477/#479 が番号で参照しており、静かに陳腐化する（**実測**: `gh issue view` で 5 件すべてが本文に「最重要ルール 4」を含むことを確認済み。#473 は「1」も含む） | `git diff CLAUDE.md` で番号行を目視 |
| **I5** | `post-edit.mjs` の**振る舞い**は不変（コメントのみ） | hook の検査が変わる | `git diff` がコメント行のみであること + `npx vitest run .claude/hooks` |
| **I6** | main 保護が一瞬も空にならない | ローカルの main が無防備になる | Phase 2 の F1/F3。`.githooks/` は既に main にある |
| **I7** | 文書は実装の後を追う | 存在しない hook を「ある」と書く／その逆 | Phase 3 を Phase 2 の後に置く |

### 失敗・異常終了・予期しない順序

- **Phase 1 で JSON を壊した場合**: file watcher が即座に拾い、全 hook が停止する。**この状態は沈黙するため気づけない。** ゆえに手順 2 の明示的検証を**必ず** Phase 1 の内部で行う。壊れていたら `git checkout -- .claude/settings.json` で即復旧
- **Phase 2 の F1/F3 が拒否されなかった場合**: Layer 1 が効いていない。**Phase 1 を revert し、原因を突き止めるまで進まない。** main にコミットが乗ってしまったら `git reset --soft HEAD~1`（`--hard` は使わない — 未コミットの変更を破壊する）
- **Phase 2 の F2 がブロックされた場合**: `block-main-commit` の削除が反映されていない（file watcher の遅延、または編集ミス）。settings.json を再確認する
- **harness の classifier が settings.json の編集を拒否した場合**: 迂回しない。ユーザーに何をしようとしているか説明して判断を仰ぐ

## テスト方針

### 追加・更新するテスト

**無し。** `.claude/hooks/post-edit.test.mjs` は `block-main-commit` を一切参照していない（実測 0 件）。今回の変更は hook 定義の削除と文書であり、`post-edit.mjs` の振る舞いは変わらない。

`validateSettings` に単体テストが無い（Phase 1a で判明）ことは既知の穴だが、**本 issue のスコープ外**（`selectChecks` への `.githooks/**` 追加を扱う #484、および「沈黙 = 合格」の撤廃を扱う Phase 3 と同時が自然）。

### 検証コマンド

`docs/build-commands.md` のカテゴリ判定:

- Rust なし → カテゴリ A 不要
- TypeScript なし → カテゴリ B 不要
- `.githooks/**` 変更なし → カテゴリ E 不要
- `.claude/hooks/**` は PostToolUse が `hook-selftest` を自動発火する

```
node -e "JSON.parse(require('fs').readFileSync('.claude/settings.json','utf8')); console.log('valid JSON')"
npx vitest run .claude/hooks
```

加えて Phase 2 の故障注入 F1 / F2 / F3。

## SPEC.md 更新要否

**不要。** `SPEC.md` は Snotra アプリの仕様書であり、エージェントの運用ルール（hook・CLAUDE.md・スキル）は対象外。

## 参照

- 手順の原典: `docs/superpowers/plans/2026-07-09-hook-responsibility-layers.md` の Task 8 / Task 9b
- 受け入れ条件: `docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md` §8「Phase 1b」
- Refs #471, #473, #480 / follow-up #482

**注意**: 上記 `docs/superpowers/**` は当時の認識の記録である。Phase 1b の完了を反映して**遡って書き換えない**（マージ済み PR の説明文を後から書き換えないのと同じ）。

---

## plan-review 結果

`Explore` × 2（`.claude/` レイヤ / ドキュメントレイヤ）と `Plan` × 1（独立再導出・`workspace/**` を読ませない）を並列実行。

### 要対処

**なし。** 3 体とも要対処ゼロ。

### 問題なし（実測で裏取りされたもの）

- `block-main-commit` の実参照は `CLAUDE.md`（4 箇所）と `post-edit.mjs:242` のみ。**`.claude/settings.json` に文字列は存在しない**（hook に `name` フィールドが無い）
- `post-edit.test.mjs:87` の `selectChecks(".claude/settings.json")` は**パス文字列だけを見る純関数**へのテストであり、JSON の内容にも `PreToolUse` の配列長にも依存しない
- `post-edit.mjs:242` のコメントは通常のブロックコメント。テンプレートリテラル内でも JSDoc 消費対象でもない（load-bearing でない）
- **生き残る hook は同一の失敗様式を持つ**（`settings.json:13` を直接読み、`@{u}` をコマンド実行前に評価することを確認）。ゆえに narrow 後の文面は新規の未測定主張ではない
- **#473/#475/#476/#477/#479 の 5 件すべてが本文に「最重要ルール 4」を含む**（`gh issue view` で実測）。番号保存の必要性が確定
- `SPEC.md` は hook / git ワークフローに 0 件。更新不要
- `AGENTS.md`・`docs/**`・`.claude/rules/**`・`.claude/agents/**` に参照なし

### 軽微な懸念（→ すべて計画へ反映済み）

1. **main 同期レシピの消失** — `:42` を削除すると CLAUDE.md から手順が消える。`.githooks/pre-merge-commit` は制約を述べるが手順を述べない → **削除ではなく理由の差し替え**へ変更
2. **スキルの文の切り方** — `start-issue:117` は「セッション断絶に備え…」と「チェーンせず…」の 2 文が同居。機械的削除は正当な理由を巻き添えにする → **後半のみ置換**。`:38` も削除ではなく置換（コードブロックが理由なく並ぶのを避ける）
3. **「Phase 2」は CLAUDE.md にとって死語** → **`#482`** と書く

### 独立導出との差分（Step 2b）

- **漏れ（導出 ∖ plan）: なし。** 独立導出は同じ 6 サイト + スキル 3 箇所へ到達した
- **スコープ過剰（plan ∖ 導出）: 1 件。** `implement/SKILL.md:80` の「フックにも弾かれる」明示化は導出が挙げなかった → **正当化した**（削除後「フック」が Claude Code hook と git hook の間で曖昧になる。担い手を名指しすることは本設計の主題であり、既に編集するファイル内で完結する）
- **一致（完全性の能動的証拠）**:
  - `settings.json` は第 0 要素のみ削除、PR 前 push チェックは不変
  - `docs/superpowers/**` は歴史的記録として**書き換えない**（独立導出が「マージ済み PR の説明文を遡って偽造することになる」と表現。判断が一致）
  - `start-issue:45` の `git pull --ff-only` は実コマンドとして残す
  - 「チェーン」の同名・別概念（Rust の修飾子チェーン `snotra-core/src/instant.rs:75,148,218`、イテレータチェーン `code-reviewer.md:121`、**cargo/clippy のチェーン実行推奨 `implement/SKILL.md:51`**）を正しく除外
  - `post-edit.test.mjs` は変更不要

### 総評

- 計画の completeness: **高**（独立導出との漏れ差分ゼロ）
- 実装着手可否: **可**（軽微な懸念 3 件を反映済み）

---

## セルフレビュー（Step 5b）

1. **対称コードパス** — 該当なし。`show`/`hide` のような対称ペアを持つコードパスに触れない。ただし `PreToolUse` の 2 hook は「片方を消し、片方を残す」非対称な操作であり、**残す側（PR 前 push チェック）が不変であること**を I2 として不変条件に立てた
2. **影響範囲の網羅性** — `block-main-commit` / `チェーン` / `--ff-only` / `最重要ルール` の 4 語で全 repo を grep。加えて**独立再導出**（計画を読ませない）で盲点クラスを検査し、漏れゼロを確認。hook に `name` が無いため、**名前ではなく振る舞い**（正規表現・exit 2・メッセージ文字列・matcher）でも検索させた
3. **境界条件** — settings.json が壊れたとき（I1）／Layer 1 が効いていないとき（Phase 2 の失敗）／file watcher が遅延するとき（F2 の失敗）を列挙し、それぞれ復旧手順を書いた
4. **リソース管理** — 該当なし。新規の状態フラグ・プロセス・リスナ・ウィンドウを一切導入しない。**削除のみ**
5. **既存パターンとの整合** — 新規パターンなし。Phase 1a と同じ規律（明示的検証・確度の書き分け・文書は実装の後を追う）に従う
6. **YAGNI 違反** — テストを追加しない。`validateSettings` の単体テスト欠如は既知だがスコープ外（#484 と Phase 3 の主題）。スキル 2 本への波及はユーザーが明示承認済み。`implement/SKILL.md:80` の 1 節のみ導出との差分があり、上記で正当化した
7. **シンプル化の挑戦** — 「この変更、本当にこの複雑さが必要か」→ **削除は最も単純な変更である。** 追加するのは narrow された 1 ルールと同期レシピ 1 行のみ。より単純な代替（例: hook を残して matcher だけ広げる）は Phase 1a の実測が否定した——漏れの根が `matcher` ではなく**語彙そのもの**だったため
8. **破壊不変条件の明示** — 「壊れたら即アウト」は 2 つ:
   - **I1（settings.json の JSON 妥当性）**: 壊れると全 hook が停止し、**その事実は沈黙する**。検知手段は hook に依存しない明示的な `JSON.parse`（Phase 1 手順 2）。復旧は `git checkout -- .claude/settings.json`
   - **I6（main 保護が空にならない）**: `.githooks/` は既に main のツリーにある（#480 のマージで実測済み）。検知手段は Phase 2 の F1/F3。落ちたら Phase 1 を revert する
