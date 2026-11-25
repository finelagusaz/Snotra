# plan-review 1529（独立レビュー）

## 要対処
- なし（致命的な欠陥は見つからなかった）。

## 軽微
1. 実在しない sha の stderr は `fatal: Needed a single revision` だけで、固定文言に無い情報だが理由は薄い。tree の sha の stderr だけが理由（`expected commit type`）を持つ（実測: `git rev-parse --verify <tree>^{commit}` は `error: ...expected commit type, but the object dereferences to tree type` + `fatal: Needed a single revision`）。safety-nets.md 45 行「理由まで assert」を満たすのは tree のテストなので、tree 側は「テスト内で取った stderr 全体が出力に含まれる」に加え、理由行（`expected commit type` か、取った stderr の `error:` 行）が出力にあることまで assert する。実在しない sha 側は stderr 全体の包含で足りる。
2. テストの git 呼び出しは実装と同じ引数・同じ cwd（`repo`。実装は `caller` = `git rev-parse --show-toplevel` = repo）にする。`runScript(repo, ...)` の cwd は repo なので一致する（verify-tree.test.mjs:95-102、verify-tree.mjs:128-133）。Windows の 8.3 短縮名・junction で cwd 表記が違っても git の stderr には cwd は出ないので影響しない。
3. trim・CRLF: 実装は `e.stderr.trim()`、テストも `.trim()` する。git は stderr を LF で出す。複数行の stderr は文字列としてそのまま連続して含まれるので、包含判定は成立する。ただし main の出力は `verify-tree: ${message}`（verify-tree.mjs:435）で、message 内の改行はそのまま出る。テストの期待値を行ごとに分割して比べる必要は無い。
4. 変異: 元の `catch {}`（固定文言のみ）へ戻すと、出力に stderr（`Needed a single revision`）が無いので assert で赤くなる。`catch (e)` + `e.message` への変異は、Node の execFileSync の message が `Command failed: ...\n<stderr>` を含むため、緑のまま残る。これは害の無い等価変異（stderr は載る）。`||` を `??` に戻す変異は、stderr が空文字になる経路（timeout・stderr 無しの非 0 終了）でしか差が出ず、2 件のテストでは検出できない。計画が `||` の理由に挙げた経路は未固定（下記「未検証」）。
5. SKILL.md への 1 文: 既存の `--dry-run` 段落（SKILL.md:30）は「何をするか」「`--` の位置」を書いている。`docs/build-commands.md:246` は書き込みの有無と # だけ、`scripts/merge-pr.mjs` 冒頭（7, 20-23 行）は使い方と構造。「既定にしない理由」はどこにも無く、矛盾・重複は無い。ただし計画の要旨は 3 文分（検査は本番も同じ・起動がマージの意思表示・事前に見たいときに選ぶ）あるので、SKILL.md には「本番も同じ検査で止まる」と「閉じる issue や本文を事前に見たいときに選ぶ」を 1 文にまとめ、詳しい議論は issue に置く（governance-docs.md の「必要なことだけ」）。「本番も同じ検査で止まる」は SKILL.md:30 の「止める理由があれば、本番と同じく exit 1 になる」と重なる。重ねず、「だから既定にはしない」と結論だけを書くほうが重複が少ない。
6. 「既定にしない」と書く 1 文に、数え上げ・全称（「すべての検査」）を入れない（AGENTS.md 検証の作法）。SKILL.md:30 の「マージ前の検査をすべて行う」は既存の記述なので触らない。

## 未検証
- ⚠️ `||` の空 stderr 経路（timeout・stderr 無しの非 0 終了）は、新テストでは再現できない。計画は敵対的調査で実測済みとしている。テストで固定しないなら、計画にその旨（「`||` は実測根拠のみで、テスト無し」）を書く。固定するなら `git` を PATH から外した起動などが必要で、費用に見合わない。
- ⚠️ ロケール: git のメッセージは `LANGUAGE`/`LC_ALL` で翻訳されうる。テストは同じ環境で git を打つので食い違わない。ただし `expected commit type` を逐語で assert する案にすると、翻訳環境で落ちる。取った stderr の `error:` 行を使うほうが安全。
- 実際に Red/Green・変異を走らせてはいない（読み取りとコマンド実測のみ）。`align` が同じ `resolveSha` を通ること（verify-tree.mjs:301）は確認した。
