## 問題なし

- **変更ファイル一覧の双方向カウントを検算した。** リポジトリ全体を `plan:ledger|plan-review-ledger|台帳|ledger`（大小無視）で grep すると 16 `*.md` + `package.json`/`scripts/*.mjs` がヒットするが、内訳は (a) plan.md が挙げた 5 ファイル（`scripts/plan-review-ledger.mjs` / `scripts/plan-review-ledger.test.mjs` / `.claude/skills/plan-review/SKILL.md` / `.claude/skills/start-issue/SKILL.md` / 新規 ADR）、(b) 変更不要と明記済み（`package.json:12`・`docs/superpowers/**`）、(c) 無関係な同名語（`docs/design/2026-05-31-coherence-staleset.md`・`docs/architecture.md:99` の「stale ledger」は snotra-core の index 鮮度管理で無関係。`.claude/skills/norm-review/SKILL.md:80` の「配送台帳」は比喩で無関係）、(d) `RETROSPECTIVE.md:15` は `init` の呼び出し形（変わらない）を引く歴史記述で影響なし——のいずれかに収まる。**計画が見落としたファイルは無い。**
- **「変更不要」の各根拠を個別に確認した。** `package.json:12` はサブコマンド非依存（`"node scripts/plan-review-ledger.mjs"` のみ）。`AGENTS.md`・ルート `CLAUDE.md` に `plan:ledger`/`plan-review-ledger` の言及は 0 件（grep で確認）。`docs/build-commands.md`・`docs/hooks.md` も同様に 0 件。`docs/superpowers/**` は `scripts/governance-check.mjs:1041`（`governanceDocs`）と `:1055`（`headingRefDocs`）の両方で `docs/superpowers/` 前方一致が明示的に除外されており、plan の「歴史資料・`governanceDocs` 対象外」という主張は実装コードで裏取りできた。
- **既存見出しを改名・挿入しない制約と両立するかを確認した。** 計画が touch する行（`.claude/skills/plan-review/SKILL.md:27-31, 33-39, 137, 140` と `.claude/skills/start-issue/SKILL.md:58`）はすべて「## Step 2」「## Step 3」の既存見出し配下の箇条書き・コード例の本文であり、見出し行そのものの改名・追加は伴わない。後置追記のみで両立する。
- **「この変更で偽になる既存の文」を both SKILL.md 全体で洗った**（`台帳`/`--slug`/`verify`/`スラグ`/`引数` で grep し、ヒット全行を個別に評価）。`:137`（verify 呼び出し、要修正と計画済み）以外に、変更後に文字どおり偽になる文は見つからなかった。`:27-31`（起動前に会話へ台帳を確定する、という記述）は init 実装の変更後も真——ここでいう「台帳」は Step 2 起動前に会話へ出す「レイヤー名→出力パス→分割根拠」の表であり、`ledger.json`（スラグのみを保持）とは別概念。両者を同じ語で指すのは計画の変更起因ではなく既存の用法であり、本 PR のスコープ外。
- **`/start-issue`「5a」ループとの整合、ラウンドをまたぐ残存経路の有無を確認した。** `init` を打たずにセッション再開すると前ラウンドの台帳と成果物が対で残り緑化しうる、という経路は実在するが、plan.md「受容する残余 2」がこれを名指しし、「台帳へラウンド番号等を持たせると `/start-issue`『5a』の"収束判定に要る状態は plan.md に置く"原則と衝突する」という理由で明示的に受容している。裁定・理由とも記載済みで、AGENTS.md の全称表現の検算要求も満たす。

## 軽微な懸念

- **`/norm-review` の要否について plan.md が一言も触れていない。** `.claude/rules/safety-nets.md` は `.claude/skills/**` への「判定を足す変更」で `/norm-review` を要求し、「索引の追随・改名には仕事が無い」を免除としている。本変更は false green 経路をスクリプト側の構造的強制（`verify` が `--slug` を渡されると error）で塞ぐものであり、SKILL.md 側は追随的な記述更新（読者の裁量に依存する新しい「判定」文は増えない）に留まるため、免除に当たる可能性が高い。ただしこの判断自体が plan.md のどこにも明記されておらず、他レイヤーのスカウトが独立に同じ結論へ達したかを確認できない。未確定欄か「受容する残余」へ一行で理由を残すことを推奨する。

## 要対処

- **Phase 2 で新設予定の「SKILL.md フェンス行 → `parseArgs` 直結テスト」が、現行の `.claude/skills/plan-review/SKILL.md:36` の `init` 呼び出し例に対して失敗する。** 該当行は `npm run plan:ledger -- init --slug <slug1> --slug <slug2> ...` で終端に `...`（プレースホルダの省略記号）を含む。この行を素直に抽出し `scripts/plan-review-ledger.mjs:65-79` の `parseArgs` へ `["init","--slug","<slug1>","--slug","<slug2>","..."]` として渡すと、末尾の `"..."` トークンが `init`/`verify`/`--slug` のいずれの分岐にも一致せず `{ error: "未知の引数: ..." }` を返す（実際のソースをトレースして確認）。plan.md はこの抽出ロジック（プレースホルダ・省略記号の扱い）を未確定のまま残しており、`.claude/skills/plan-review/SKILL.md` の「変更内容」一覧にも `:36` 側の調整（例文からの `...` 除去、または抽出時のプレースホルダ処理）が入っていない。フェーズ2着手前に「抽出対象を `verify` 行だけに絞る」「抽出時に末尾の `...` を捨てる」「SKILL.md の例文自体から `...` を落とす」のいずれかを未確定欄で決めておく必要がある——決めずに実装すると、新設テスト自身が代表入力（SKILL.md の実文言）で赤くなり、フェーズ2の完了条件を満たせない。

## 未検証

- `allowed-tools` の `Bash(npm run plan:ledger *)` が、変更後の実際の呼び出し文字列（引数の形が変わる `init`/`verify` 双方）に対して実測でも前方一致で通るか — plan.md 自身が「フェーズ3で実測する」と明記しており、実装（`parseArgs` のコマンド別分岐）がまだ存在しないため本ラウンドでは実行確認できない
- 新設 ADR 本体が実際に「識別子 vs 内容」の境界判断 1 点へ絞られ、他の小さな決定（台帳ファイル名・JSON 形式・exit code）を重複して書き込まないか — ADR ファイル自体が未作成のため確認不能（ADR 担当レイヤーの成果物 `workspace/plan-review/adr.md` にも同じ観点が「未検証」として記載されている）
