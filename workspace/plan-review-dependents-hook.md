# 独立レビュー — #1140 plan.md（観点1: 沈黙=合格契約 / 観点2: hooks→scripts 依存新設）

対象: `workspace/plan.md`（読み取りのみ・未変更）、`workspace/research.md`（読み取りのみ・未変更）、issue #1140。

## 事前確認（結論に影響する検証済み事実）

- **G-hook-fires の母集団照合は `checks.push("<id>")` の正規表現抽出のみを見る**（`scripts/governance/checks/G-hook-fires.mjs:128`）。`isSourceFileWrite` の WARN も、計画の新 reminder も `warnings.push(...)` で直接発行され `checks.push` を経由しない（`.claude/hooks/post-edit.mjs:134-167` に `checks.push` の全出現を確認済み・新設予定の WARN はこの外）。ゆえに plan.md L33「この reminder は…検査 id を持たない…表の母集団照合は id を持つものだけを見る」は **実装と一致する**。表に行を足さない判断は正しい。

---

## 要対処

### 1. 静的 `import` は `main()` の try/catch に守られず、失敗すると全編集で hook が落ちる

`.claude/hooks/post-edit.mjs` は末尾で `try { main(); } catch (e) { emitError(...); }`（`.claude/hooks/post-edit.mjs:476-484`）という形で例外を捕捉しているが、これは **`main()` 呼び出しの例外だけ**を守る。ESM の `import` 文はモジュール本体の実行より前（リンク時）に解決されるため、`scripts/governance/dependents.mjs` を静的 `import` する設計（plan.md L19「hook が import する」）では、**import 解決の失敗はこの try/catch の外側で起き、キャッチされない**。

実測で確認した（`C:/Users/Eoh/AppData/Local/Temp/claude/.../scratchpad/broken-importer.mjs`）: 存在しないモジュールを静的 import し、末尾に `try { main() } catch` を置いたスクリプトを実行すると、`main()` は一度も呼ばれず、`catch` にも入らず、`ERR_MODULE_NOT_FOUND` が未捕捉のまま stderr へ出て **exit code 1**・stdout は空（JSON envelope 無し）で落ちた。

この hook は `matcher: "Edit|Write"` で**全ファイル種別**に対して起動する（`.claude/settings.json:17`）。したがって `scripts/governance/dependents.mjs` が読めない状態（後述「要対処 2」のツリー不一致、あるいは単に一時的な構文エラー）が 1 つでも起きると、**`.rs` 編集の `fmt`/`clippy`/`cargo test` を含む全ての PostToolUse 検査が黙って（envelope 無しで）止まる**。これは「`.md` に沈黙を作ってしまう」という本来のリスクより遥かに重い——「沈黙 = 合格」の契約そのものが全ファイル種別で壊れる。

plan.md の「不変条件と異常系」節（L72-77）は `git diff` 失敗のフォールバックだけを書いており、import 失敗への対処（例: `main()` 内での動的 `import()` + try/catch、あるいは `fs.existsSync` によるガード後の静的 import 併用）が計画に明記されていない。Phase 2 のテスト項目にも「import 失敗時に他の検査が生き残ること」の変異注入が含まれていない（Phase 4 の変異注入 3 種 (a)(b)(c) にも無い）。**設計として dynamic import + try/catch を選ぶか、少なくとも変異注入で実測して安全性を確認する項目を追加する必要がある。**

### 2. worktree では `import` の解決先ツリーが `resolveRoot` の解決先ツリーと一致しない

hook 本体は常に `node "${CLAUDE_PROJECT_DIR:-.}/.claude/hooks/post-edit.mjs"` として起動される（`.claude/settings.json:21`）。一方、検査を実行するツリー（`root`）は `resolveRoot(abs)` が**編集対象ファイルの絶対パス**から最近接の `.git` を遡って求める（`.claude/hooks/post-edit.mjs:93-95`）。`docs/hooks.md:103` はこの非対称性を既に明記している——「root は `file_path`（絶対パス）から最近接の `.git` を遡って導出するため、`${CLAUDE_PROJECT_DIR}` の意味論に依存しない。**ただしスクリプト自身の所在は `settings.json` の `${CLAUDE_PROJECT_DIR:-.}` で解決**」。

ES モジュールの相対 `import` 指定子は、**呼び出し側の `process.cwd()` ではなく import 文を含むファイル自身の場所**を基準に解決される（実測で確認: `/tmp/esmtest/a/sub/importer.mjs` を `cwd=/tmp/esmtest/b` から起動しても `../lib.mjs` は `a` 側が解決された）。したがって `post-edit.mjs` 内の `import ... from "../../scripts/governance/dependents.mjs"` は **`${CLAUDE_PROJECT_DIR}` を基準に解決され、`root`（編集対象ファイルの属するツリー）を基準にしない**。

plan.md L76「**worktree で動くこと**——`post-edit.mjs` は `resolveRoot` でツリー根を求める。`scripts/governance/` はそのツリー内に在る」は、**「検査を実行するツリー」と「hook 自身のコードが読み込まれるツリー」を同一視している**点で不正確である。`${CLAUDE_PROJECT_DIR}` と編集対象ファイルの属するツリーが一致しない構成（例: worktree がこの機能のコミットをまだ持たない・別ツリーの `scripts/governance/dependents.mjs` を誤って読む）では、要対処 1 のクラッシュか、あるいは**誤ったツリーの索引で依存判定する**という静かな誤りのどちらかが起きる。Phase 2/4 のテストに、`${CLAUDE_PROJECT_DIR}` 相当と `root` が異なるツリーを指す状況を模した変異注入を追加すべき（少なくとも「対象ツリーに `scripts/governance/dependents.mjs` が無い」ケースの実測）。

### 3. `docs/hooks.md:59` の発火一覧の行自体が、これから偽になる同種の主張を含んでいるが、計画の修正対象 4 か所に入っていない

`docs/hooks.md` の「PostToolUse（post-edit.mjs）の発火一覧」表は、`G-hook-fires` の `sawEmptyRow` 契約を満たすための**空集合の行**を持つ（`scripts/governance/checks/G-hook-fires.mjs:118-127` のコメントが「割り当ての無いファイルの沈黙は合格ではない、という契約を運ぶ行」と明記）。その行の全文（`docs/hooks.md:59`）:

> `` `docs/hooks.md` `` | （なし） | 上記以外（`` `*.md` ``・`` `.claude/rules/**` ``・`` `.claude/skills/**` ``・`` `scripts/**` `` 等）は**何も走らない**——沈黙は「合格」ではない

この計画の実装後は、`.md` 編集は「純追記でなく依存を持つ節に当たる」場合に WARN を出す。つまり「何も走らない」という**字義通りの全称主張**は偽になる（検査 id が付かないという意味では真だが、この行の補足列は「検査」に限定した主張になっていない）。plan.md L34-36・L62 の「偽になる散文 4 か所」（ルート `CLAUDE.md` L29 / `docs/build-commands.md` L32・L167 / `.claude/rules/governance-docs.md` L23）に **この行は含まれていない**。plan.md L40 の「散文の母集団の取り方」で述べる grep（「`.md` の沈黙は何も走らなかった」という概念）も、この特定の行の存在は数え上げに現れていない。

なお修正しても機構は壊れない——`sawEmptyRow` の判定は検査 id 列 `cols[2] === "（なし）"` だけを見て、補足列の自由文は読まない（`G-hook-fires.mjs:103`）ので、この行の**補足列プローズだけ**を書き換えても表の機械照合は通る。Phase 3 のチェックリストへ、この行の文言修正（「検査は走らない」等、検査に限定する言い回しへ）を明示的に加えるべき。

### 4. `.claude/hooks/post-edit.mjs:11-12` 自身の契約コメントが、修正対象の grep 母集団から漏れている

`post-edit.mjs` 冒頭の doc comment（この機能が改修する当のファイル）は次の主張を持つ:

> `.claude/hooks/post-edit.mjs:11-12`: 「この契約は『検査が割り当てられたファイル』についてのみ成り立つ。割り当てが無いファイル（*.md 等）の沈黙は『何も走らなかった』であり、合格ではない。」

これは root `CLAUDE.md` L29 と同型の主張であり、この計画によって同様に不正確になる。しかし plan.md の「変更ファイルと対象シンボル」表（L27-38）は `.claude/hooks/post-edit.mjs` の変更内容を「`changedHunks` と…WARN 発行」としか書いておらず、この doc comment の訂正は挙がっていない。plan.md L40 の grep 母集団（4 件）にも含まれていない。この 1 件が最も見落とされやすい——**修正対象のロジックのすぐ真上にある契約コメント**であり、AGENTS.md が名指す「写しを直す当のコミットが一枚落とす」パターン（#977/#1056）に該当する典型例である。Phase 3 の対象ファイル一覧に明示的に追加すべき。

---

## 軽微

### 1. `scripts/governance-check.mjs:6` は近縁の主張を持つが、字義上は偽にならない

コメント「PostToolUse hook は `.md`・rules・skills に検査を割り当てない（#497 で受容した残余）」は、実装後も**検査 id は引き続き 1 つも付かない**ため、字義通りには真のままである（reminder は `検査` ではない、という plan の区別と同型）。ただし `.claude/rules/governance-docs.md:23` に対して計画が適用した理由（「姉妹文書だけ直してここを残すと不整合に見える」・plan.md L36）と同じ配慮がここにも当てはまりうる。必須の修正ではないが、一貫性の観点で 1 句足すことを検討する価値はある。

### 2. `scripts/governance/dependents.mjs` 自身への編集は PostToolUse 検査の対象外のままである

`hook-selftest` は `vitest run .claude/hooks`（`.claude/hooks/post-edit.mjs:341`）に限定されているため、`scripts/governance/dependents.mjs` 単体を編集しても**新設する `dependents.test.mjs` はこの hook-selftest では直接走らない**（`.claude/hooks/post-edit.test.mjs` が `post-edit.mjs` を import する経路で、import 時の壊れ方＝構文/export 崩れは間接的に拾われる）。ただしこれは `scripts/` 配下の非 TS ファイル全般が元々 PostToolUse 対象外という**既存の残余**（root `CLAUDE.md` L29 が既に明記）であり、この計画固有の新しい穴ではない。plan.md L50 は Phase 1 の検証を「`npm test` が緑」と明示しており、手動実行が前提になっている点は妥当。

---

## 未検証

### 1. この harness で `${CLAUDE_PROJECT_DIR}` が worktree セッション中にどう解決されるか

`Agent` ツールの `isolation: "worktree"` や、この session に見えている deferred tool `EnterWorktree`/`ExitWorktree` が、実際に `${CLAUDE_PROJECT_DIR}` を worktree のパスへ切り替えるのか、それとも元セッションのパスに固定したまま cwd だけを動かすのかを実地に確認していない（deferred tool のスキーマ取得や実際の worktree 起動は本レビューの「実装しない・他ファイルを変更しない」制約の範囲外と判断し、行っていない）。要対処 2 の実害の頻度はこの挙動に強く依存する——`docs/hooks.md:103` の記述はこの非対称が**存在すること**は裏付けるが、**頻度**までは分からない。

### 2. Claude Code の hook ランナーが「exit 1・stdout 空」をどう扱うか

要対処 1 のクラッシュ時、PostToolUse hook が exit 1 + stdout 無しを返したとき、エージェント側に何が届くか（無反応・生の stderr の断片・ツール自体のブロック等）を実地で確認していない。今回のレビューでは実際に稼働中の hook を壊すことは他エージェントへの影響とタスク制約（他ファイル変更禁止）の両面から避け、隔離したスクラッチパッドでの模擬実験に留めた。

### 3. `sectionOf` を使わない独自アンカー方式（plan.md 未確定 1）とこの 2 観点との相互作用

節の切り出し方式そのものは観点 1・2 の範囲外と判断し検証していない。

---

## 返り値用サマリ

**要対処（4件）**
1. `scripts/governance/dependents.mjs` の静的 `import` は `main()` の try/catch に守られず、失敗すると `.rs` 等を含む全編集で hook が丸ごと落ちる（実測で確認）。動的 import + try/catch 等の対策が計画に無い。
2. worktree では `post-edit.mjs` 自身が読み込まれるツリー（`${CLAUDE_PROJECT_DIR}`）と検査対象ツリー（`resolveRoot`）が食い違いうる。相対 import は前者基準で解決されるため、plan.md L76「scripts/governance/ はそのツリー内に在る」は不正確。
3. `docs/hooks.md:59`（発火一覧の空行）自身が「何も走らない」という、この計画で偽になる主張を含むが、修正対象の 4 か所リストに入っていない。
4. `.claude/hooks/post-edit.mjs:11-12` の契約コメント自身が同種の主張を持つが、修正対象・grep 母集団のどちらからも漏れている。

**軽微（2件）**
1. `scripts/governance-check.mjs:6` は近縁だが字義上は偽にならない主張。一貫性のためだけの任意修正候補。
2. `dependents.mjs` 単体の編集は PostToolUse 対象外のまま（既存の残余の範囲内、新しい穴ではない）。

**未検証（3件）**
1. worktree セッションでの `${CLAUDE_PROJECT_DIR}` の実際の解決挙動（要対処2の実害頻度に影響）。
2. hook が exit 1・stdout 空で落ちたときのエージェント側の見え方。
3. `sectionOf` 不使用の節切り出し方式との相互作用（観点外と判断・未着手）。
