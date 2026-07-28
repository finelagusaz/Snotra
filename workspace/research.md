# research — #831 `plan:ledger verify` の母集団がスラグの打ち直しに依存する

## issue の要約

`npm run plan:ledger -- verify --slug ...` は照合の母集団を**エージェントが打ち直したスラグ**から再導出する。台帳は会話にしか存在しないため、`init --slug a --slug b` の後に `verify --slug a` を打つと「台帳 1 件中 1 件が実在」で exit 0 になる——`/plan-review` の設計が狙っている false green そのものの形。

**採る方針（ユーザー裁定・2026-07-28）: 案 A**——`init` が slug 集合を `workspace/plan-review/.ledger.json` へ保存し、`verify` は引数なしでそれを読む。打ち直す機会そのものを消す。

## 関連コード（grep で実在確認済み）

| 対象 | 現在の役割 |
|---|---|
| `scripts/plan-review-ledger.mjs` | `parseArgs` / `validateSlugs` / `classifyEntries` / `formatReport` / `readLedgerDir` / `main`。定数 `LEDGER_DIR` `SLUG_RE` `MIN_CHARS` `PRESENT` `MISSING` `REASON` |
| `scripts/plan-review-ledger.test.mjs` | 19 件。`parseArgs` の「内容を受け取る口を持たない」契約テストを含む |
| `package.json:12` | `"plan:ledger": "node scripts/plan-review-ledger.mjs"` |
| `.claude/skills/plan-review/SKILL.md:9` | `allowed-tools` の `Bash(npm run plan:ledger *)` |
| `.claude/skills/plan-review/SKILL.md:36` | `init` の呼び出し |
| `.claude/skills/plan-review/SKILL.md:39` | 判定の中身は script 冒頭の契約とテストが正本、というポインタ |
| `.claude/skills/plan-review/SKILL.md:137` | `verify` の呼び出し（**`--slug` を渡す形。ここが変わる**） |

`plan:ledger` / `plan-review-ledger` を参照する文書は上記 + `RETROSPECTIVE.md:15`（`init` の呼び出し形を引く歴史記述。`init` の形は変わらないため影響なし）。**他に無い**（`--include=*.md --include=*.json` でリポジトリ全体を grep）。

## オーケストレーターの権限（案 A の安全性の根拠）

`.claude/skills/plan-review/SKILL.md` の `allowed-tools` は `Read` / `Glob` / `Agent` / `Bash(gh issue view *)` / `Bash(npm run plan:ledger *)` の 5 つのみ。**`Write` も汎用 `Bash` も持たない**ため、オーケストレーターは `.ledger.json` を改竄できない。台帳をディスクへ置いても自作自演の経路は生まれない。

（スカウトは `general-purpose` ゆえ全ツールを持つ。これは #827 が既に「受容する残余」として記録した範囲であり、本 issue で変わらない。）

## 実測（本 issue のために測ったもの）

1. **`git add workspace/` はドットファイルを拾う。** `workspace/plan-review/.ledger.json` を置いて `git add workspace/` した結果 `A  workspace/plan-review/.ledger.json` が staged になった。`/start-issue`「Step 6 — workspace をコミット & プッシュ」で台帳が PR に載る＝人がレビューできる。
2. **現行の `readLedgerDir` は `.ledger.json` を無視する。** フィルタが `n.endsWith(".md")` のため、`.ledger.json` を置いたまま `verify --slug a` を走らせても命名逸脱として現れず「台帳 1 件中 1 件が実在」になった。**台帳を同じディレクトリへ置いても成果物スキャンに混ざらない。**

## 技術的制約

- Win32 依存・IPC・イベントループのいずれにも触れない（Node スクリプトと Markdown のみ）。
- スクリプトは依存ゼロ・決定的の契約を持つ（`Date.now()` 等を使わない）。JSON の読み書きは Node 標準で足りる。

## 既存パターン

- `scripts/race-boundaries.mjs` が同型の「skill の母集団取得をスクリプトへ出す」先例。ただし**あちらは状態をディスクへ持たない**（毎回 git から導出する）ため、台帳の永続化に相当する先例はリポジトリ内に無い。
- `workspace/plan-review/` の新鮮性は `/plan-review`「Step 2 — 並列サブエージェントで検証」の削除が保証する設計（`.claude/skills/start-issue/SKILL.md:58`）。`init` が削除→作成→台帳書き込みを行う順序であれば、台帳の新鮮性も同じ削除に乗る。

## 契約への影響（実装で必ず扱うもの）

スクリプト冒頭の契約は現在こう書かれている:

> **呼び出し側が渡した内容を、このスクリプトは決して書かない。** 受け取るのはスラグだけで、`init` はディレクトリを作り、`verify` は読んで報告する。

案 A は `init` にファイル書き込みを追加するため、**この一文はそのままでは偽になる**。`docs/development-principles.md`「撤去（消す変更）の作法」の「自分の変更が偽にした記述・作った矛盾は『範囲外』に置けない」に当たる。契約は「何を書かないか」を**成果物とその他で線引きする**形へ書き直す必要がある。

## 未解決の疑問

なし（設計上の選択は下記 `plan.md` の未確定欄で潰す）。
