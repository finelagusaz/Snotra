# governance-mechanism

## 問題なし
- `AREA_BUDGET` の参照先は全て洗った — `scripts/governance-check.mjs:604,662,667,678,683,842`。テストは相対参照（`AREA_BUDGET.rules + 1`）で固定値を持たない — `scripts/governance-check.test.mjs:433,439`。CI（`.github/workflows/ci.yml:58`）は `node scripts/governance-check.mjs` を呼ぶだけで定数変更に追随不要。**`AREA_BUDGET.rules` 引き上げでテスト・CI が壊れる経路は無い**
- `AREA_BUDGET.rules` の母集団（`/^\.claude\/rules\/[^/]+\.md$/`・`scripts/governance-check.mjs:672,697`）に `safety-nets.md` が含まれることを実測で確認 — `npm run governance:check` の実行結果は `rules 7956/8056 字`。Task 2 Step 1 の期待値（`rules 7956/8056 字`）と一致。計画の母集団理解は正しい
- Task 1 の ADR は `docs/adr/0011-*.md` として `governanceDocs()`（`scripts/governance-check.mjs:798-808`）の対象になり、G3（参照実在）・G11（見出し参照）の照合対象になる。ADR 案文が引用する 3 件の見出し参照はすべて実在を確認済み: `AGENTS.md:48`「条件別チェック（トリガー → 参照先）」、`docs/development-principles.md:66`「構造的設計原則と強制の階梯」、`.claude/skills/implement/SKILL.md:102`「4a. check スキルの実行」（G11 の正準形 `` `<対象>`「<見出し>」 `` に合致し `resolveRefTarget`/`collectAnchors` で解決可能）。Task 1 Step 3 の「governance:check が緑になる」という期待は根拠がある
- `docs/build-commands.md:89`「カテゴリ F」の発火条件（`*.md`・`.claude/rules/`・`.claude/skills/`・workflow）と、計画（`workspace/plan.md:16`）の記述（`.claude/rules/` と `*.md` の変更が発火条件）は矛盾しない——本 PR が触る範囲（ADR=`*.md`、rules ポインタ）の部分集合を正しく引用している
- `governance:check` にはテストがある（`scripts/governance-check.test.mjs:419-` の G10 セクション）ことを確認。母集団欠落・両面独立・改行非依存・CR 非算入は検証済みだが、**budget とmeasured 値の間の最小マージンを強制するテストは無い**——このためゼロ余裕運用（下記「要対処」）はテストでは検出されない
- 計画は「骨格の遵守を測る検出器は置かない」（`workspace/plan.md:81`）と決めており、Task 3 Step 7 も「塞ぎがあった場合のみ」`governance:check` を走らせるとしていて、骨格遵守を自動判定する仕組みを新設する手順は混じっていない

## 軽微な懸念
- Task 2 の見積もり「ポインタ行は約 114 字」（`workspace/plan.md:149`）は不正確 — 実際にプランに書かれた行そのものを実測すると 131 字（末尾改行込みで 132 字）。計画は「見積もりではなく出力の実測値を使う」と明記しており実害は無いが、乖離が 15 字超と大きく、実測値が見積もりから大きくずれたときに実装者が「何かおかしいのでは」と余計な確認をする可能性がある

## 要対処
- **Task 2 Step 4（`workspace/plan.md:151-157`）は `AREA_BUDGET.rules` に Step 3 の実測値をそのまま代入しており、ゼロ余裕（基準 = 実測ちょうど）になる。** これは `docs/adr/0005-area-metric-characters.md:28` が「ゼロ余裕で据える（基準 = 実測ちょうど）」として明示的に**却下済み**の設計そのものである（却下理由: 文字数指標では誤字修正 1 文字でも定数書き換えを要求し、摩擦の日常化で赤が反射的に引き上げられ意味を失う）。既存の引き上げ履歴（`scripts/governance-check.mjs:551-562,584`）はすべて「新実測値 + 100 字」の形を取っており、Step 4 の literal な指示はこの慣行からも外れる
- 上記の帰結として、**ADR 案文自身の主張（`workspace/plan.md:79`「引き上げ幅がポインタ 1 行の実測値ちょうどである」）が Step 4 の formula と矛盾する。** 現行 budget（8056）は実測（7956）に対しすでに +100 字の余裕を内包している。Step 4 のとおり `rules: <Step 3 の実測値>`（= 旧実測 7956 + ポインタ行 131〜132 字 ≈ 8088）を budget にすると、引き上げ幅は `8088 − 8056 = 32` 字前後にしかならず、ポインタ行の実測値（131/132 字）とは一致しない。「ちょうど」が成立するのは `rules: <Step 3 の実測値> + 100` とし余裕を引き継いだ場合のみ（このとき増加幅は旧余裕 100 が両辺で相殺し、純粋にポインタ行の字数と一致する）。**Step 4 の formula と Step 4 直後のコメント文言（`<差分>` の計算式）を `<Step 3 の実測値> + 100` へ修正する必要がある**
- Task 3 Step 7（`workspace/plan.md:252-255`）は「塞ぎで rules の面積が増えていたら、Task 2 Step 3〜5 と同じ手順で予算を再調整する」とし、上と同じ欠陥手順を継承する。Task 2 側を直せば自動的に解消するが、Task 3 側にも同じ formula への言及がないか（独立した誤りを生まないか）は Task 2 修正後に確認が要る

## 未検証（理由）
- `.claude/rules/safety-nets.md` を実際に編集して `npm run governance:check` を再実行し、新しい実測値（新 rules 合計）を直接得ることはできなかった——理由: 分類器が `.claude/rules/safety-nets.md` の Edit と、`.claude/rules/` 配下ファイルを読む node スクリプトの実行の両方を許可拒否した（このレビューの書き込み対象は `workspace/plan-review/governance-mechanism.md` の 1 ファイルのみと指示されているため、それ以上の回避は行っていない）。上記「要対処」の数値（132 字・32 字前後）は、ポインタ行の文字列そのものを直接測った実測値（`node -e` で 131/132 字を確認済み）と、`sumChars` が単純加算であるというコード読解（`scripts/governance-check.mjs:611-621`）から算出した理論値であり、実際に編集後の `governance:check` 出力そのものでは確認していない
- Task 3・Task 4（`/norm-review` の 2 クラス読者運用・ADR「帰結」への記録・PR closing keyword 対策）は本レビューの担当レイヤー（governance-mechanism）の対象外——Task 3 Step 7 の budget 再調整以外は検証していない
