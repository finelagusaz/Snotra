# 計画: issue #1529

## 目的と受け入れ条件

- (A) `/merge-pr` で `--dry-run` を既定の 1 手目にしない、という決定を `.claude/skills/merge-pr/SKILL.md` に反映し、理由を issue #1529 に残す。
- (B) `scripts/verify-tree.mjs` の `resolveSha` が `sha が解決できない` を投げるとき、git の stderr を添える。テストで固定する。

受け入れ条件:
1. SKILL.md「2. ツールを走らせる」の `--dry-run` の段落に、既定にしない理由が 1 文ある。
2. issue #1529 に、既定にしない理由のコメントがある。
3. `verify:tree run --sha <解決できない値>` が exit 2 で、出力に固定文言 `sha が解決できない: <値>` と、同じ git コマンドの stderr（テストの中で取得）が両方含まれる。tree の sha（stderr が固定文言に無い情報を持つ場合）でも同じ。
4. `align` は同じ `resolveSha` を通るので、同じ挙動になる（関数が 1 つであることで担保。テストは `run` で行う）。

## 変更するファイル

- `scripts/verify-tree.mjs` — `resolveSha`: `catch {}` を `catch (e)` にし、`EnvError` の message を `sha が解決できない: ${sha}\n${e.stderr?.trim() || e.message}` にする。`??` ではなく `||` なのは、timeout や stderr 無しの非 0 終了で `e.stderr` が `""` になるため（敵対的調査で実測）。
- `scripts/verify-tree.test.mjs` — `describe` の run 系に「解決できない sha」のテストを 2 件足す（実在しない sha・tree の sha）。
- `.claude/skills/merge-pr/SKILL.md` — 「2. ツールを走らせる」の `--dry-run` 段落の末尾に、既定にしない理由を 1 文足す。

## 既定にしない理由

- SKILL.md には結論だけを 1 文で書く（「止める理由があれば、本番と同じく exit 1 になる」は同じ段落に既にあるので写さない）。文案: 「閉じる issue や本文の先頭を事前に見たいときに選ぶもので、既定の手順にはしない（#1529）。」全称・数え上げを入れない。
- issue #1529 のコメントに議論を書く: 止める判断はツールのマージ前の検査が持ち、本番でも同じ検査に通らなければ exit 1 で止まる。`/merge-pr` の起動そのものがマージの意思表示なので、人の確認の段を既定には足さない。費用（gh の照会 1 巡・人の確認で 1 回止まる）に見合う追加の保護が無い。

## 実装の順序

1. テストを先に足し、Red（出力に stderr が含まれない）を確かめる。
2. `resolveSha` を直して Green。
3. SKILL.md に 1 文足す。
4. issue #1529 にコメントする（`gh issue comment 1529 --body-file <tmp>`）。

## 不変条件と異常系

- exit 2（環境の失敗）のまま。exit 1 と混ぜない。
- `e.stderr` が無い失敗（git が起動できない等）では `e.message` へ落ちる。
- stderr は複数行（曖昧な短縮形の hint）でもそのまま出す。1 行目は固定文言のまま（既存の読み手が先頭で判別できる）。
- `--sha` 未指定の `--sha が要る` は変えない。

## テスト方針と検証コマンド

- テストは git の文言を逐語で書かない。テストの中で、実装と同じ引数 `["rev-parse", "--verify", `${x}^{commit}`]` を同じ一時リポジトリで `spawnSync("git", ...)`（または try/catch の `e.stderr`）で打ち、その stderr を trim した値が出力に含まれることを確かめる（テスト側の `git()` ヘルパは失敗で throw するので使わない）。あわせて固定文言 `sha が解決できない: <x>` と exit 2 を assert する。
- 入力は 2 件: 実在しない sha（`"0".repeat(40)`）と、tree の sha（一時リポジトリで `git rev-parse HEAD^{tree}`）。後者は stderr が固定文言に無い情報（`expected commit type`）を持つので、stderr を添えたことを実質で固定する。曖昧な短縮形は一時リポジトリで決定的に作れないので、テストにしない。
- tree の sha のテストでは、取った stderr 全体の包含に加えて、その `error:` で始まる行（固定文言に無い理由）が出力にあることも assert する（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。
- `||`（空の stderr で `e.message` へ落ちる経路）はテストで固定しない。timeout や stderr 無しの非 0 終了を一時リポジトリで決定的に作る費用に見合わないため。`||` を `??` に戻す変異は新テストでは検出されない（独立レビューの所見）。根拠は敵対的調査の実測だけである。
- Red の理由が assert（出力に stderr が含まれない）であることを出力で確かめる。
- 変異注入（`/implement` の委譲先が行う）: `resolveSha` を元の `catch {}` と固定文言だけに戻すと、新テスト 2 件が assert で落ちること。
- 検証: `npm run verify:tree -- run --sha <sha>`。回るのは常時の E（`npm test`・`scripts/**` に当たる）と F（`npm run governance:check`・SKILL.md に当たる）。A・C・D は該当なし（Rust・製品のコードパスを触らない）。

## SPEC.md・関連文書

- `SPEC.md` は変えない（製品の挙動ではない）。`docs/build-commands.md` の `merge:pr` の記述（`--dry-run` は選択肢）は決定と矛盾しないので変えない。

## 作業項目

### フェーズ 1: verify:tree の stderr
- [x] テスト 2 件を足し Red を確かめる（2 件とも assert で赤: 出力に stderr が無い）
- [x] `resolveSha` を直し Green を確かめる（verify-tree.test.mjs 55 件 green）

### フェーズ 2: dry-run の決定
- [x] SKILL.md に既定にしない理由を 1 文足す
- [x] issue #1529 に理由をコメントする（issuecomment-7154167704）

### 検証ラウンド
- [x] ラウンド 1 — sha 028cf900 / verify:tree green（E・F exit 0・warnings なし） / 委譲レビュー: Critical・High・Medium 0、Low 2（コメントの「timeout」はこの呼び出しには当たらない・「曖昧な短縮形は決定的に作れない」は fast-import で反証された）——どちらも挙動を変えないので直さない / 変異 M1・M2 とも assert で赤・生存 0（`||`→`??` は計画どおり未検出の残余） / 実地: 一時リポジトリで実在しない sha・tree・曖昧な短縮形（`89c5`）に `run --sha` を打ち、3 形とも stderr が添えられて見分けられた

## 未確定（実装前に潰す）

- [x] （裁定）dry-run を `/merge-pr` の既定の 1 手目にするか — AskUserQuestion で「既定にしますか？」と問い、回答「既定にしない (Recommended)」（理由を issue に残し、SKILL.md に一言添える）
- [x] 敵対的調査の所見の反映 — 採否を research.md「敵対的調査の採否」に書いた。`??` を `||` へ、テストを 2 件（実在しない sha・tree の sha）に確定
- [x] #1362 の失敗の形 — PR #1362 の値 `96c8962d4` で再現（stderr は `Needed a single revision` のみ）。#1362 の残余の狙い「打ち間違いと曖昧な接頭辞を見分ける」は stderr で満たせるので、追加の診断は足さない

## 承認

- [x] 承認済み — 2026-11-25 / 問い: "workspace/plan.md の計画を承認しますか？" / 回答: "承認して実装へ"

## セルフレビュー

- リスク: 高（`.claude/skills/**` と `scripts/**` を変更する）
- plan-review: 独立レビュー1体（Step 2。issue に無い人間の裁定が前提にあるため）。結果は `workspace/plan-review-1529.md`
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 0 件。軽微 6 件のうち 3 件を反映（tree のテストで理由行も assert・SKILL.md は結論 1 文で既存文と重ねない・`||` の未検出を明記）。残り 3 件は計画の確認（cwd・CRLF・変異）で変更なし
- 未検証: `||` の空 stderr 経路はテストで固定しない（上記）
- check スキル: なし（対称ペア・状態遷移・永続形式・新規関数のどれにも当たらない）
- 散文の全件列挙: 語 3 / 総件数 51 行 / ★未掲載 12（計画へ足した 0。いずれも別の「解決できない」・挙動を変えない `--dry-run` の記述・`git push --dry-run`） / ☆行未掲載 0（計画へ足した 0）
