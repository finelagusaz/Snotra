# 調査: issue #1529

## issue の要約

振り返りで見つけた、ツールまわりの小さな残り 2 件。

1. `/merge-pr` で `--dry-run` を既定の 1 手目にするか（#1363 で `--dry-run` を足したが、既定にするかは #1318「未確定」のまま閉じた）。
2. `scripts/verify-tree.mjs` の `sha が解決できない: ${sha}` に git の stderr を添え、テストで固定する（#1362 で原因の特定に手間取った）。

受け入れ条件:
- dry-run を既定にするかが決まり、スキルに反映されている（しないなら、その理由が issue に残る）
- 解決できないときのメッセージに git の stderr が含まれ、テストで固定されている

## 裁定（2026-11-25・AskUserQuestion）

- dry-run は既定にしない。理由を issue に残し、SKILL.md に「既定ではない理由」を一言添える。

## 関連するファイル・シンボル

- `scripts/verify-tree.mjs` の `resolveSha(cwd, sha)`（161–168 行）。`git(cwd, ["rev-parse", "--verify", `${sha}^{commit}`])` を try し、`catch {}` で `EnvError("sha が解決できない: ${sha}")` を投げる。例外オブジェクトを捨てるので stderr が消える。
  - 呼び出し元: `cmdRun`（`resolveSha(caller, opts.sha)`・194 行）と `cmdAlign`（`resolveSha(tree, opts.sha)`・301 行）。どちらも同じ関数を通るので、1 か所の修正で両方に効く。
  - `git()` は `execFileSync(..., { encoding: "utf8", stdio: "pipe" })`。失敗時の例外は `e.stderr`（utf8 文字列）を持つ。
  - main の `catch (e)` が `EnvError` の `message` を `verify-tree: ...` として stderr へ出し、exit 2。
- `scripts/verify-tree.test.mjs`: `runScript(cwd, args)` が `{ status, output }`（stdout+stderr）を返す。`initRepo()`・`commitFile()` の雛形がある。exit 2 の既存テストの形（236 行「sha が作業ツリーの HEAD でなければ exit 2」）。「解決できない」を確かめるテストは現在 0 件（grep で確認）。
- `.claude/skills/merge-pr/SKILL.md`「2. ツールを走らせる」: `--dry-run` を選択肢として説明している段落。
- `scripts/merge-pr.mjs`: `--dry-run` の実装（変更しない）。

## 再利用できる既存のパターン

- 同じファイルの `discardBranchArtifacts`: `${(e.stderr ?? e.message).trim()}`。`removeTree`・`cmdAlign` の checkout も `e.stderr ?? e.message`。stderr が無い失敗（spawn 失敗など）で `e.message` へ落ちる形をそのまま使う。

## 同じパターンの他の箇所

- `scripts/*.mjs` で「`} catch {` の直後に throw」するのは `verify-tree.mjs:165` だけ（grep）。同ファイルの 175・182 行の `catch {}` は「lock を持たない sha」「読めない」を意図して別の結果へ倒す分岐で、git の失敗を報告する箇所ではない。

## 技術的な制約・実測

git 2.56.1.windows.1 で `git rev-parse --verify "<x>^{commit}"` の stderr を実測した:

| 入力 | stderr |
|---|---|
| 実在しない sha / ref・`HEAD~99999` | `fatal: Needed a single revision` のみ |
| tree の sha | `error: <x>^{commit}: expected commit type, but the object dereferences to tree type` + fatal |
| 曖昧な短い sha | `error: short object ID 019b is ambiguous` + `hint:` 候補一覧 + fatal |

- **#1362 の原因（実在しない sha）では、git の stderr は新しい情報をほぼ持たない。** stderr を添えて効くのは、型の不一致と曖昧な短縮形である。受け入れ条件は満たせるが、#1362 の型の手間は減らない。
- stderr は複数行になりうる（曖昧のとき）。メッセージは 1 行目に固定文言、続けて stderr を置く形にする。
- git のメッセージは locale で訳されうる。テストは git の文言を逐語で書かず、テストの中で同じ git コマンドを打って得た stderr が出力に含まれることで確かめる。

## #1362 の一次資料（PR #1362「残余」）

- 拒まれた値は `96c8962d4`（実在しないオブジェクト・末尾 1 字の打ち間違い）。この値で `git rev-parse --verify '96c8962d4^{commit}'` を打つと stderr は `fatal: Needed a single revision` のみ（再現済み）。
- PR #1362 の残余が挙げた狙いは「打ち間違いと曖昧な接頭辞を見分けられる」こと。曖昧なら `short object ID ... is ambiguous` と候補が出て、打ち間違いなら出ないので、stderr を添えれば見分けられる。追加の診断（`git cat-file -e` など）は要らない。

## 敵対的調査（`workspace/adversarial-1529.txt`）の採否

- 採る: 失敗例外の `e.stderr` は常に非空とは限らない（spawn 失敗では undefined、timeout や stderr 無しの非 0 終了では `""`）。`??` は `""` を素通しするので、`e.stderr?.trim() || e.message` にする（空の stderr のとき `e.message` の `Command failed: git ...` が出る）。同ファイルの既存箇所の `??` は今回の範囲外として触らない。
- 採る: 曖昧な短縮形は一時リポジトリで決定的に作れない。テストで固定するのは「実在しない sha」と「tree の sha（型不一致）」の 2 件。曖昧な形は上の実測（本体リポジトリ）で確かめた記録に留める。
- 採る: テストの期待値は、実装と同じ引数（`rev-parse --verify <x>^{commit}`・`-q` なし）を同じ一時リポジトリで打った stderr を trim したものにする。vitest 設定・テストの `git()`・`runScript` は env を上書きしない（壊せなかった）。
- 壊せなかった: 同型の catch は scripts/ で 1 か所だけ（`catch (_)`・`.catch(` も 0 件）・呼び出し元は 194・301 行の 2 つ・locale で文言が変わらない（この Git for Windows に翻訳なし）。
- ⚠️ の残り: `scripts/**` が safety-nets の paths に入ることは主が全文を読んで確認済み。#1362 の原因は上で確認済み。`.claude/hooks/` の類似 catch は、issue が `verify:tree` に限るので範囲外。

## 未解決の疑問

- なし。
- SKILL.md はセーフティネット（`.claude/skills/**`）、`scripts/**` も同じ。PR 本文の冒頭に「マージは合意してから」が要る。
