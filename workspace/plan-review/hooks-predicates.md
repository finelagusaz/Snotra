# plan-review — hooks レイヤー

## 問題なし
- I1/I4 骨格カナリア（`pre-bash.test.mjs:437`,`:461`）は matcher・fail-closed 既定を検査対象にしており、5 述語の追加はこの2箇所を触らないため無傷（`.claude/settings.json` に `process.platform` の類は現状皆無 = `grep -rn "process.platform" .claude/` 一致無し、matcher/hook-selftest の配線は plan で変更しない）。
- 既存 `decide` 呼び出しは `pre-bash.test.mjs` に 22 箇所（`grep -o "decide(" | wc -l` = 22）+ `pre-bash.mjs:222` の `main()` 内 1 箇所 = 23、すべて 3 位置引数。全 23 箇所のコマンド文字列を grep したが `\`（バックスラッシュ）・`<<`・`pull`・`no-verify`・`python` はいずれも 0 件 — 4 引数化後も `platform===undefined` で Windows 専用 3 件が発火せず、既存呼び出しは全て緑のまま（I5/I7 の意図は数値の誤りに関わらず成立、後述）。
- `hasSafeChain` / `GIT_PUSH` / `GH_PR_CREATE` の外部呼び出し元は `pre-bash.mjs` 自身と `pre-bash.test.mjs` のみ（`grep -rn "hasSafeChain\|segmentEnd\|gitSegments"` で確認）。D4 のヘルパ抽出で影響を受ける呼び出し元の見落としは無い。
- `usesHeredoc` の `new RegExp` 補間安全性（I2 の主張）は妥当 — 捕獲群 `[A-Za-z_]\w*` は regex メタ文字を一切含みえない文字集合であり injection は原理的に起きない。
- 既存 `ENV_PREFIX`（`(?:[A-Za-z_]\w*=\S*\s+)*`）を `matchAll`/`g` で全コマンドへ適用する懸念を実測: 480,000字・env-like トークン多数の敵対的入力で 2ms、heredoc 検出正規表現も 200,000字の `<<` 反復で 1ms — `\S*\s+` は相互排他な文字クラスの組で曖昧なバックトラックが起きない構造であり、指摘された形の catastrophic backtracking は再現しなかった。
- G11 (`headingRefDocs`) が `docs/superpowers/` を除外する記述（research.md:52）を `scripts/governance-check.mjs:801-805` で確認、一致。`CLAUDE.md` の行番号参照（12-18 節・24-25 の2 bullet・46 のフック表行）も現物と一致。
- CI 実行 OS（research.md:46 の ubuntu:39・windows:116）を `.github/workflows/ci.yml` で確認、一致。darwin の CI job は無い（issue #768 も明記の上でmacOS実測は手元実行に委ねる設計であり計画との齟齬ではない）。
- `findUp` の pre-bash.mjs/post-edit.mjs 間重複は既存かつ既にコメントで許容済み（`pre-bash.mjs:176`）。計画はこれに触れず、新たな二重実装を追加しない。

## 軽微な懸念
- research.md:23 と plan.md:38（I7）が「既存 30 個の `decide` 呼び出し」と書くが実測は 23（test 22 + main 1）。数の誤りだが不変条件そのもの（3 引数呼び出しは書き換えずに緑）は影響を受けない——ただし「判定ロジックは実装前に自分で測る」原則からすれば、この数値も再検算漏れの一種。
- `usesNoVerify` の記述（research.md:74）「commit セグメントに短縮 `-n`（`-nm` 等）」に対応する代表ケースが、plan.md の境界条件表・テスト方針表のどちらにも無い。「セグメント境界」行の `git commit -n -m x` はスペース区切りのみで、結合短フラグ（`-nm`）の境界は名指しされていない。
- Phase1 のブレットは 5 述語のみ「export で追加」と明記し、`segmentEnd`/`gitSegments` の export 有無が書かれていない。D4 は「既存テストが固定」を根拠に挙動不変とするが、これは `hasSafeChain` 経由の間接カバーであり、`gitSegments` 自体を直接単体テストする気があるなら export が要る（Phase2 冒頭に export 前提の記述が無い）。
- `docs/hooks.md:37`「`selectChecks` に発火を足すときはカナリアも対で足す」への類推は、`decide` が（`selectChecks` のような）ディスパッチ表ではなく逐次判定であるため直接は当てはまらないと判断できるが、plan.md にはその判断（「新規カナリア不要」）自体が明記されていない。既存2カナリアの無傷確認だけでは「5 述語自体の配線漏れ」を検知する仕掛けが behavior test 群以外に無いことになる（テストが厚いので実害は薄いが、意図の記録が無い）。

## 要対処
- **`usesHeredoc` の想定実装（research.md:71 のロジック (a)：「終端行が同コマンド内にあるか」を候補ごとに走査）は O(候補数 × コマンド長) になりうる。実測で確認済み**: 候補 20,000 件・コマンド長 209,889 字の敵対的入力（`<<EOF0 <<EOF1 <<EOF2 ...`）で、候補ごとに動的 `new RegExp` を全文に対して `.test()` する素朴実装は **2.9 秒**（`node -e` で実測、本レビュー内で再現可能）。I2 は「5 述語は全域関数（never throw）」としか述べておらず、**有界時間（never hang）** を要求していない。research.md の病的入力カタログ（26 件・20万字 1 本 / `$env:TEMP\` × 5000 等）には「多数の heredoc 候補が 1 コマンドに同居する」形が含まれておらず、この O(N×L) 経路は現状の no-throw テストでは捕捉されない。放置すると、hook 自身に GIT_TIMEOUT_MS 相当の自衛が無いため、長大・多候補コマンドで PreToolUse 全体が長時間ブロックしうる（fail-closed の意図とは無関係に UX を壊す）。対処案: (1) 終端行の集合を 1 パスで先に収集し候補ごとの再走査を O(1) 参照にする、または (2) Phase2 の病的入力に「多数の heredoc 候補・大きめのコマンド長」の組を追加し壁時計時間の上限を assert する。どちらか一方を計画に明記すべき。
  - 参考: git セグメント側（`segmentEnd`/`gitSegments` の想定実装、`command.slice(at).search(...)`）は同種の懸念に見えたが実測では 50,000 件の `git` 出現・約300,000字で 8ms と無視できるコスト（V8 の SlicedString 最適化により `.slice` がコピーを避けるため）。**リスクは heredoc 述語の「候補ごとに全文再スキャン」という設計に限定される**。

## 未検証（理由）
- 5 述語・`segmentEnd`/`gitSegments`・REMEDY 文言の最終的な正規表現・実装コードは未着手（research.md は記述レベルの仕様のみ）。したがって「病的入力 no-throw」以外の性能特性・メタ文字安全性は記述された設計方針からの推論であり、実装確定後の再検証が必要（特に上記「要対処」の heredoc 走査方式）。
- Windows 機でのライブフォールトインジェクション（Phase 3・5件）は未実施（計画段階のため当然）。
- Claude Code ハーネス側で `tool_input.command` の最大長に制約があるかは本リポジトリから確認できない（外部システムの挙動）。制約が十分小さければ heredoc 走査の O(N×L) リスクは実害が小さくなるが、その保証は本リポジトリのコードからは導出できない。
- `/norm-review`（Phase 6）による 5 件の拒否メッセージ文言・`docs/hooks.md` 追記の規範レビュー結果は、文言自体が未作成のため評価不能。
