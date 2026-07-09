# plan — issue #482: PreToolUse を `pre-bash.mjs` へ移す（Phase 2 / (A2)）

前提: `workspace/research.md`。分類は **バグ修正**（`SPEC.md` の意図に触れない。hook はプロダクト挙動ではない）→ **SPEC.md 更新は不要**。

## 0. 受け入れ条件（テスト可能な形）

| # | 条件 | 検証 |
|---|---|---|
| A1 | PowerShell tool からの `gh pr create` がブロックされる | 故障注入 V2（`tool_name` を実測） |
| A2 | `gh pr create` を**文字列として含むだけ**のコマンドは通る | unit + 故障注入 V3（E1 の回帰） |
| A3 | `git push -u origin HEAD && gh pr create` が通る | unit + 故障注入 V4 |
| A4 | 未 push コミット / upstream 未設定での `gh pr create` はブロックされる | unit + 故障注入 V1 |
| A5 | 判定不能（stdin 破損 / `command` 非文字列 / git 状態取得失敗 / 内部例外）はすべて **exit 2** | unit + e2e |
| A6 | 管轄外ツール（`Edit` / `BashOutput` 等）は無出力 exit 0 | e2e |
| A7 | CLAUDE.md 最重要ルール 2 が消え、残るルールが 3 つになる | 目視 + 文書カナリアテスト |

## 1. 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `.claude/hooks/pre-bash.mjs` | **新規**。PreToolUse ディスパッチャ |
| `.claude/hooks/pre-bash.test.mjs` | **新規**。純関数 unit + `spawnSync` e2e + settings.json 整合カナリア |
| `.claude/settings.json` | PreToolUse を `matcher: "Bash\|PowerShell"` + `node .../pre-bash.mjs` へ差し替え |
| `CLAUDE.md` | 最重要ルール 2 削除・繰り上げ（4つ→3つ）、Git/GitHub 運用の同ルール削除、フック節の前文と表を as-built へ |

**触らない**: `.claude/hooks/post-edit.mjs`（Phase 3）、`.githooks/**`（別レイヤ）、`vitest.config.ts`（`.claude/hooks/**/*.test.mjs` を既に include）、`SPEC.md`、`docs/superpowers/**`・`.superpowers/**`（日付入りの過去記録）。

### 1.1 `docs/build-commands.md` にカテゴリ F を足さない（検討して却下）

`.claude/hooks/**` 専用の検証カテゴリを新設する案を検討したが採らない。

- `post-edit.mjs` の `selectChecks` が `.claude/hooks/**` と `.claude/settings.json` の編集で **`hook-selftest`（`vitest run .claude/hooks`）を自動発火**する。検証は既に配線済みで、カテゴリ追加は二重メンテになる
- `npm test` は PR CI で走り、`vitest.config.ts` の include が `.claude/hooks/**/*.test.mjs` を拾う。CI 側の担保も既にある
- カテゴリ追加は `AGENTS.md` と `.claude/skills/implement/SKILL.md` の「A〜E」も書き換える必要があり、**スキル改変は CLAUDE.md 最重要ルール 4（エージェント設定の変更は合意してから）に触れる**。issue #482 の承認範囲外

代わりに CLAUDE.md フック節へ「`.claude/hooks/**` の編集は hook-selftest が自動発火する」旨が既にある（現状維持）。

## 2. 判定ロジック（`pre-bash.mjs`）

### 2.1 hook が問う問い

> `gh pr create` が走る瞬間、コミットは remote に存在するか？

真になる経路は 2 つ（research §5.1）。**静的**: 鎖の中で `git push` が `&&` で先行する。**動的**: upstream 設定済みかつ未 push コミットなし。現行 hook は動的しか見ておらず、それが最重要ルール 2 の根拠だった。

### 2.2 コマンド位置検出（`/plan-review` 後の修正版・全 18 ケースを実測）

コマンド位置 = 文字列先頭、または区切り文字（`;` `&` `|` 改行 `(` `)` `{` `}`）の直後。`&&` の 2 文字目も区切り文字なので `&& gh pr create` は一致する。

**先頭の区切り文字を「食う」ままにしてはならない。** `match.index` が `gh` ではなく `&` を指し、§2.3 の位置計算が崩れる（実測で確認。§2.3 参照）。ゆえにコマンド本体を**キャプチャ**し、そこから `gh` の開始位置を復元する。

```js
const AT_CMD_POS = String.raw`(?:^|[;&|\n\r(){}])\s*`;
// フラグはサブコマンドの前後どちらにも来うる。値がスペース区切り（`--repo o/r`）でも許す。
const FLAG = String.raw`(?:-{1,2}\S+(?:\s+[^-\s]\S*)?\s+)*`;
const GH_PR_CREATE = new RegExp(`${AT_CMD_POS}(gh\\s+${FLAG}pr\\s+${FLAG}create\\b)`);
const GIT_PUSH     = new RegExp(`${AT_CMD_POS}(git\\s+(?:-\\S+\\s+)*push\\b)`);
const CWD_CHANGE   = new RegExp(`${AT_CMD_POS}(cd|pushd|popd|chdir|Set-Location)\\b`, "i");

/** マッチしたコマンド本体（キャプチャ 1）の開始位置。区切り文字を含まない。 */
function tokenStart(re, command) {
  const m = re.exec(command);
  return m ? m.index + m[0].length - m[1].length : -1;
}
```

実測（`node` で 18 ケース）:

| 入力 | 検出 | 備考 |
|---|---|---|
| `grep -n "gh pr create" CLAUDE.md` | **false** | E1 の誤爆が消える |
| `gh pr create --title x` | true | — |
| `gh --repo o/r pr create` | **true** | 原案（`(?:--?\S+\s+)*`）は **false** ＝過小検出＝ fail-open だった |
| `gh pr --repo x create` | **true** | 同上 |
| `gh pr list --search create` | **false** | 過剰検出しない |
| `gh pr list` / `gh issue create` | false | — |
| `echo "&& gh pr create"` | true | 過剰検出。fail-closed 方向ゆえ許容し、テストで意図を固定 |
| `git -C /x push` | `GIT_PUSH` false | 別ツリーへの push は安全な鎖と見なさない（fail-closed） |

**`/plan-review` が「原案でも `gh --repo o/r pr create` は一致する」と報告したが、自分で測ったところ不一致だった。** サブエージェントの実測もそのまま信じず、判定の中核は自分で測る。

### 2.3 `hasSafeChain(command)`（原案にバグ 2 件・実測で確認）

原案の擬似コード `between = command.slice(push.index + push[0].length, pr.index)` は、`pr.index` が区切り文字を指すため **`&&` の片方の `&` しか `between` に入らない**。実測した帰結:

| 入力 | 原案 | 正しい判断 |
|---|---|---|
| `git push -u origin HEAD && gh pr create` | **false（block）** | **allow** — 本 issue の目的 D3 / 受け入れ条件 A3 が未達だった |
| `git push -u origin HEAD && npm test; gh pr create` | **true（allow）** | **block** — `;` は push 失敗時も `gh` を走らせる ＝ **fail-open** |

両者は単一原因（区切り文字の消費）であり、`gh` トークン位置を使えば同時に解ける。さらに「`between` に `&&` が含まれる」ではなく **「`between` の区切り run がすべて `&&` である」**を要求する。

```js
export function hasSafeChain(command) {
  const ghAt = tokenStart(GH_PR_CREATE, command);
  if (ghAt < 0) return false;
  const push = GIT_PUSH.exec(command);
  if (!push) return false;
  const pushEnd = push.index + push[0].length;
  if (pushEnd > ghAt) return false;                    // push が後ろ
  const seps = command.slice(pushEnd, ghAt).match(/[;&|\n\r]+/g) ?? [];
  return seps.length >= 1 && seps.every((s) => s === "&&");
}
```

`&&` は前段の成功を保証するため、push が失敗すれば `gh pr create` は走らない。区切りが 1 つでも `;` / `||` / `|` / 改行なら **判定不能として block** する（`git push -u origin HEAD &&\ngh pr create` のような `&&` 直後の改行も block する。摩擦は許容し、fail-closed を優先する）。

### 2.4 `decide(payload, readGitState)` — 分岐表

| 状況 | 判断 |
|---|---|
| `tool_name` が `Bash` / `PowerShell` 以外 | **allow**（管轄外。`BashOutput` に `command` が無いことへの二重防御） |
| 対象ツールで `tool_input.command` が文字列でない | **block** |
| `GH_PR_CREATE` 不一致 | **allow**（管轄外） |
| `CWD_CHANGE` が `gh pr create` より前にある | **block**（どの repo か判定不能） |
| ↑ 判定は真偽値ではなく **位置比較**（`tokenStart(CWD_CHANGE, c) < tokenStart(GH_PR_CREATE, c)`）。`.test()` で書くと `gh pr create && cd x` まで block する | |
| `hasSafeChain` が true | **allow**（静的経路） |
| `readGitState()` が失敗（非 repo / git 不在 / timeout） | **block** |
| upstream 未設定 | **block** |
| 未 push コミットあり | **block** |
| それ以外 | **allow** |

`readGitState` は `decide` へ**注入**する（`post-edit.mjs` の `resolveTarget(payload, rootResolver)` と同型）。これで分岐表全体が git 無しで unit テストでき、「判定は正しいが配線を誤った」失敗も捕まる。

### 2.5 `readGitState(cwd)`

`spawnSync("git", ...)` を `shell: false` / `timeout: 10_000` で 2 回。`cwd` は `payload.cwd`（無ければ `process.cwd()`）。

1. `git rev-parse --abbrev-ref --symbolic-full-name @{u}` — `error` → `{ok:false}` / `status!==0` → upstream 未設定
2. `git log @{u}..HEAD --oneline` — `error` または `status!==0` → `{ok:false}` / stdout が非空 → 未 push あり

`error.code === "ETIMEDOUT"` も `{ok:false}` に落とす（**hook 全体の timeout に丸投げしない**。丸投げすると kill された沈黙が exit≠2 になり fail-open）。

### 2.6 出力と exit code

**exit 2 + stderr** を採る。JSON `permissionDecision: "deny"` 方式は exit 0 を要求するため、「既定を block に倒す」構造が取れない（research §3.1）。

```js
if (invokedDirectly) {
  process.exitCode = 2;                       // 既定は block。allow が確定したときだけ 0 へ落とす
  try { main(); } catch (e) { blockWith(`HOOK ERROR: ${e?.stack ?? String(e)}`); }
}
```

- `process.exit()` は使わない（未 flush 出力の切り捨て — `post-edit.mjs` I1）
- Node は**未捕捉例外時に `process.exitCode` を無視して 1 で終了**する。ゆえに `try/catch` は省略不可。exit 1 は「非ブロッキング＝コマンドは実行される」＝ fail-open
- block メッセージに **観測した `tool_name` を含める**（V2 で PowerShell tool の実名を実測するため）
- allow は**無出力**

## 3. 実装順序（フェーズ）

各フェーズの検証が緑になってからコミットする。

### Phase 1 — スクリプトとテスト（TDD）
1. `pre-bash.test.mjs` を先に書く（§5 の全ケース）→ `npx vitest run .claude/hooks` が **Red**
2. `pre-bash.mjs` を実装 → **Green**

この時点で `.claude/settings.json` は未変更＝**旧 hook が生きており、安全網は一瞬も空にならない**（設計文書 §7 の「安全網を一瞬も空にしない」に倣う）。

### Phase 2 — 配線と故障注入
3. `.claude/settings.json` の PreToolUse を差し替え（file watcher が即反映・要セッション再起動なし）
4. 故障注入 V1〜V4 を実測（§6）
5. `tool_name` が `PowerShell` でなければ matcher と `TARGET_TOOLS` を実測値へ修正し、V2 を再実行

### Phase 3 — ドキュメント同期
6. `CLAUDE.md` を as-built へ（§7）
7. OPEN issue 5 件の番号参照を名前参照へ書き換える（§7.1・ユーザー承認済み）

## 4. 不変条件

| # | 不変条件 | 破れたときの症状 |
|---|---|---|
| I1 | **既定の exit code は 2。allow が確定した経路だけが 0 を書く** | 内部例外が exit 1 → 非ブロッキング → `gh pr create` が通る（fail-open） |
| I2 | 判定に使うのは `tool_input.command` **のみ**。`description` を読まない | D2 の誤爆が再発 |
| I3 | 管轄外ツールは allow、判定不能は block。**両者を混同しない** | `BashOutput` を巻き込んでブロック（`command` が無いため） |
| I4 | 検出は「コマンド位置」。過剰検出は許容、過小検出は許容しない | 過小検出 = fail-open = 本 issue の再発 |
| I5 | `git` 実行は `timeout` 付き。打ち切りは block へ倒す | hook 全体の timeout で kill → exit≠2 → fail-open |
| I6 | allow は無出力。stderr を書くのは block のときだけ | 「沈黙 = 許可」の契約が濁る |
| I7 | `import` しただけで `main()` が走らない（`invokedDirectly` ガード） | `npm test` が stdin 待ちで停止（I13） |
| I8 | `.claude/settings.json` は常に妥当な JSON | 壊すと**全 hook が停止**する。編集は hook-selftest が即検証する |

### 4.1 「失敗・異常終了・予期しない順序」での挙動

- **`git` が無い / repo でない** → `readGitState` が `{ok:false}` → block。`gh pr create` はどのみち失敗する
- **hook スクリプト自体が起動できない**（`node` 不在・パス誤り）→ ドキュメント記載なし。非ブロッキング＝ fail-open と推測される。**スクリプト内部からは塞げない既知の穴**として research §5.4 に記録済み。`post-edit.mjs` と同じ配線（`${CLAUDE_PROJECT_DIR:-.}`）を使い、経路を増やさない
- **`.claude/settings.json` が壊れる** → 全 hook 停止。編集時に `hook-selftest` が JSON 妥当性を即検証する（既存機構）

## 5. テスト方針

`.claude/hooks/pre-bash.test.mjs`。`npm test` と `hook-selftest`（`vitest run .claude/hooks`）の両方が拾う。CI は ubuntu-latest なので **Windows リテラルパスを書かない**。

### 5.1 unit（`decide` に fake `readGitState` を注入）

| # | 入力 | 期待 | 守る不変条件 |
|---|---|---|---|
| U1 | `tool_name: "Edit"` | allow | I3 |
| U2 | `tool_name: "BashOutput"`（`command` 無し） | allow | I3 |
| U3 | `tool_name: "Bash"`, `command` 無し | block | I3 |
| U4 | `grep -n "gh pr create" CLAUDE.md` | allow | **E1 回帰**・I2/I4 |
| U5 | `gh pr create --title x`, upstream 未設定 | block | A4 |
| U6 | 同, upstream あり・未 push なし | allow | — |
| U7 | 同, 未 push コミットあり | block | A4 |
| U8 | `git push -u origin HEAD && gh pr create` , upstream 未設定 | **allow** | **D3 回帰**・A3 |
| U9 | `git push -u origin HEAD; gh pr create` | block | 2.3（`&&` でなければ判定不能） |
| U10 | `gh pr create && git push` （順序逆） | block | 2.3 |
| U11 | `tool_name: "PowerShell"` + `gh pr create` | block | **D1 回帰**・A1 |
| U12 | `readGitState` が `{ok:false}` | block | I5 |
| U13 | `cd ../other && gh pr create` | block | 2.4 |
| U14 | `echo "&& gh pr create"` | block（過剰検出の意図を固定） | I4 |
| U15 | `gh pr list` / `gh pr view 1` / `gh issue create` / `gh pr list --search create` | allow | I4 |
| U16 | `gh --repo o/r pr create` / `gh pr --repo x create` / `gh --repo=o/r pr create` | block（検出する） | **原案が過小検出だった箇所**・I4 |
| U17 | `readGitState` は `gh pr create` 非検出時に**呼ばれない** | spy で検証 | 無駄な git 起動を防ぐ |
| **U18** | `git push origin x && echo hi; gh pr create` | **block** | **原案が fail-open だった箇所**（§2.3） |
| **U19** | `git push -u origin HEAD && npm test; gh pr create` | **block** | 同上。`;` は push 失敗時も `gh` を走らせる |
| **U20** | `git push -u origin HEAD && echo x && gh pr create` | **allow** | 区切り run がすべて `&&` |
| **U21** | `git -C /x push && gh pr create` | **block** | 別ツリーへの push は安全な鎖でない |
| **U22** | `git push -u origin HEAD\ngh pr create`（改行区切り） | **block** | 改行は `;` と等価 |
| **U23** | `gh pr create --body "a && b"`（`&&` が後方のみ） | **block** | `push` 不在。誤 allow しない |
| **U24** | `tool_input.description` に `gh pr create`、`command` は無害 | **allow** | **I2**（`description` を見ない）の直接証明 |

### 5.2 e2e（`spawnSync(process.execPath, [SCRIPT], { input })`）

| # | 入力 | 期待 |
|---|---|---|
| E1 | 壊れた JSON（`"{ not json"`） | **exit 2** + stderr に理由（**exit 1 でないこと**を明示 assert） |
| E2 | `tool_name: "Edit"` の payload | exit 0 + **無出力** |
| E3 | `tool_input: null` の Bash payload | exit 2 |
| E4 | `import` しただけで `main()` が走らない | exit 0 + `"imported"` |

### 5.3 カナリア（`.claude/settings.json` 整合）

`post-edit.test.mjs` の「tsconfig ドリフト検出カナリア」と同型。settings.json を触った人がここで気づく。

- PreToolUse の `matcher` が `TARGET_TOOLS` の全要素を含む
- PreToolUse の `command` が `pre-bash.mjs` を指す
- PreToolUse に**インライン `grep` が残っていない**（旧 hook の残骸検出）

### 5.4 検証コマンド

```bash
npx vitest run .claude/hooks   # Phase 1 の Red / Green
npm test                       # 全体（ui + .claude/hooks + .githooks）
```

`docs/build-commands.md` のカテゴリ A〜E はいずれも該当しない（`*.rs` / `ui/src/**` / ウィンドウ・ホットキー / UI スタイル / `.githooks/**` のいずれでもない）。

## 6. 故障注入（`AGENTS.md`「安全網が効いていることは一度は実測する」）

**upstream 未設定の状態でしか block 経路は観測できない**（research E3）。本ブランチは Step 6 で push されるため、**使い捨てブランチ `tmp/probe-482`（upstream 無し）を切って実測し、終わったら削除する**。

| # | 故障注入 | 期待 | 何を証明するか |
|---|---|---|---|
| V1 | **Bash** tool から `gh pr create --help`（upstream 無し） | block。stderr に `tool_name=Bash` | 新 hook が発火している |
| V2 | **PowerShell** tool から同じ | block。stderr に `tool_name=<実測値>` | **D1 が塞がった**。PowerShell tool の `tool_name` を実測 |
| V3 | Bash から `grep -n "gh pr create" CLAUDE.md` | **通る** | **D2 が塞がった**（E1 の誤爆消滅） |
| V4 | Bash から `git push -u origin HEAD && gh pr create --help`（push 済みブランチ） | **通る** | **D3 が塞がった**。ルール 2 削除の根拠 |
| V5 | `npm test` | 緑 | 回帰なし |

- `gh pr create --help` は help を印字するだけで **PR を作らない**。仮に hook が発火しなくても副作用は無い（プローブとして安全）
- V2 で `tool_name` が `PowerShell` でなければ、matcher と `TARGET_TOOLS` を実測値へ直して再実行する。**この 1 点が未確定のまま実装を終えてはならない**（未確定＝ D1 が直っていない可能性）
- **V3・V4 が「通ること」の確認は、V1・V2 の「止まること」と同じだけ重要**である。V3/V4 が失敗すれば CLAUDE.md 最重要ルール 2 は削除できず、Phase 3 は取り消しになる（設計文書 §6 V9 と同じ理由）

## 7. ドキュメント更新（`CLAUDE.md`）

| 箇所 | 変更 |
|---|---|
| L13 | 「適用される**4つ**」→「**3つ**」 |
| L16 | 最重要ルール 2（チェーン禁止）を**削除**、以降を繰り上げ（HEREDOC → 2、エージェント設定 → 3） |
| L41 | Git/GitHub 運用の同ルールを**削除** |
| L49 | フック節前文: **発火（`matcher`）は `.claude/settings.json`、判定は `.claude/hooks/pre-bash.mjs` の `decide` が SSOT**。matcher は settings.json に残るため「発火条件ごと pre-bash.mjs へ移る」と書いてはならない（PostToolUse の `selectChecks` と同じ非対称） |
| L53 | フック表を as-built へ: 発火は `Bash` / `PowerShell` の両ツール。判定は `tool_input.command` の**コマンド位置**のみ。`git push ... && gh pr create` は通る。判定不能は block |

`AGENTS.md` / `SPEC.md` / サブディレクトリの `CLAUDE.md` / `e2e/` / `docs/build-commands.md` は無影響（`/plan-review` が独立 grep で確認）。

### 7.1 番号繰り上げが OPEN issue 5 件の参照を陳腐化させる（`/plan-review` の発見）

ルール 2 を削除すると「最重要ルール 4（エージェント設定の変更は合意してから）」は **3** に繰り上がる。実測: **#473 / #475 / #476 / #477 / #479（すべて OPEN）が本文で「最重要ルール 4」を番号参照している。**

これは前身計画（`docs/superpowers/plans/2026-07-09-hook-responsibility-layers.md:43`）が Phase 1b で削除でなく narrow を選んだ 2 つの理由のうちの 1 つ（「番号 1〜4 が保たれる」）である。#482 はもう一方の理由（hook の誤爆）を解消するが、**この理由は依然として有効**であり、計画は当初これに沈黙していた。

**リポジトリ内のライブファイルには番号参照による破損はない**（`CLAUDE.md:79` は不変のルール 1 を指す。スキル・`AGENTS.md` に番号参照なし）。破損するのは GitHub 上の 5 issue の本文のみ。

**決定（ユーザー承認済み）: 5 件の issue 本文を「名前参照」へ書き換える。**

```
「CLAUDE.md 最重要ルール 4」 → 「CLAUDE.md 最重要ルール『エージェント設定の変更は合意してから』」
```

- 番号は「順序」という暗黙の状態を持ち、ルールを増減するたびに外部参照が静かに腐る。名前は順序に依存しない安定した識別子であり、**今回直すだけでなく再発を止める**
- 実施は Phase 3（CLAUDE.md の変更と同時）。`gh issue edit <N> --body-file <tmp>` を 5 回。**対外的操作のため、各 issue の現在の本文を取得してから当該行のみを置換する**（本文全体を再生成しない）
- 検証: 書き換え後に `gh issue view <N> --json body` で「最重要ルール 4」が残っていないことを 5 件すべてで確認する
- `CLAUDE.md:79` の「（→「最重要ルール」1）」はルール 1 を指し、ルール 1 は不変ゆえ破損しない。今回は触らない

## 8. セルフレビュー

### 8a. `/plan-review` 結果（Explore ×2 + 独立再導出 `Plan` ×1）

**要対処（すべて計画へ反映済み）**

| # | 指摘 | 検証 | 反映先 |
|---|---|---|---|
| R1 | `hasSafeChain` が正準 allow ケース `git push … && gh pr create` を **block** する | **自分で node 実測して確認**（`planSafe=false`） | §2.3 を書き直し |
| R2 | `hasSafeChain` が `git push … && npm test; gh pr create` を **allow** する（fail-open） | 同上（`planSafe=true`） | §2.3 + U18/U19 |
| R3 | ルール番号繰り上げが OPEN issue 5 件の「最重要ルール 4」参照を陳腐化させる | `gh issue view` ×5 で実測 | §7.1（ユーザー判断待ち） |

**軽微（反映済み）**: `CWD_CHANGE` は真偽値でなく位置比較（§2.4） / L49 の SSOT 文言は「発火＝settings.json・判定＝pre-bash.mjs」と書き分ける（§7）。

**サブエージェントの誤りを 1 件検出**: 「原案の正規表現でも `gh --repo o/r pr create` は一致する」との報告は、自分の実測では **不一致**（過小検出＝ fail-open）。原案 regex を `FLAG`（スペース区切りのフラグ値・`pr` 前後の両方）へ拡張し U16 を追加した。**判定の中核はサブエージェントの実測も鵜呑みにせず自分で測る。**

**独立再導出（`Plan`・plan.md を読ませない）との差分**
- **漏れ（導出 ∖ plan）**: R3 の 5 issue（概念ラベル参照のため `gh pr create` の grep では到達不能）。→ §7.1 へ反映
- **スコープ過剰（plan ∖ 導出）**: なし
- **一致（完全性の証拠）**: 新規 2 ファイル + `settings.json` + `CLAUDE.md` の 4 点、`vitest.config.ts` 変更不要、`post-edit.mjs` の hook-selftest が自動発火、`SPEC.md`・`e2e/` 無影響、カテゴリ F 却下、exit 2 のみが block・内部例外は exit 2 へ倒す、`payload.cwd ?? process.cwd()` — **すべて独立に再一致**

**`/symmetric-check` は実行しない**: 本変更は生成/破棄ペア（`listen`/`unlisten` 等）も `show`/`hide` 型の対称コードパスも導入しない。唯一の対称軸である PreToolUse ⇄ PostToolUse の契約差と allow ⇄ block の非対称は §8b-1 と I6 で明示的に扱い、`/plan-review` の hook 層エージェントが検証済み。`/race-check`・`/cache-check`・`/persistence-check`・`/state-check` はいずれもトリガー該当なし（async・キャッシュ・on-disk 形式・UI モードのいずれにも触れない）。

### 8b. Step 5b チェックリスト

1. **対称コードパス** — PreToolUse ⇄ PostToolUse が対称ペア。両者の契約は**鏡像**である（PostToolUse: 沈黙 = 検査合格 / PreToolUse: 沈黙 = 許可）。危険な沈黙も鏡像で、後者は「exit 1 でコマンドが通る」。§2.6 の「既定 exitCode = 2」がこれに対応。`allow`/`block` の対称ペアについては「allow は無出力・block のみ stderr」（I6）で非対称性を明示的に固定した
2. **影響範囲の網羅性** — `gh pr create` / `PR 前 push` / `pre-bash` を全 `*.md` / `*.json` で grep 済み（research §7）。ヒットのうち `docs/superpowers/**` と `.superpowers/**` は日付入りの過去記録につき凍結。`vitest.config.ts` の include は既に `.claude/hooks/**/*.test.mjs` を含み変更不要（実測）
3. **境界条件** — U9（`;` 区切り）・U10（順序逆）・U13（`cd`）・U14（引用内 `&&`）・U16（`gh --repo` 前置）・E1（stdin 破損）・E3（`tool_input: null`）・I5（git timeout）
4. **リソース管理** — 生成/破棄ペアは無い（`spawnSync` は同期・自己完結、`timeout` で必ず回収）。新規の状態フラグ・プロセス・ウィンドウを導入しない
5. **既存パターンとの整合** — `post-edit.mjs` の規約（純関数 export / 依存注入 / `invokedDirectly` ガード / `process.exitCode` / `spawnSync` + timeout / e2e テストの形）をそのまま踏襲。新規パターンを導入しない
6. **YAGNI** — 対象は `gh pr create` のみ。設計文書 §2 が挙げる `gh pr merge --squash` / `gh issue close` の誤 close は**実装しない**（issue の要求範囲外）。`docs/build-commands.md` のカテゴリ F も却下（§1.1）
7. **シンプル化の挑戦** — 「既定 exitCode = 2」により、異常経路ごとの個別ハンドリングが不要になる（**安全側を既定値に置く**）。shell パーサは書かない（正規表現 2 本 + 位置比較で足りる）。`cd` 検出は 1 行 + 1 テストだが、**摩擦（`cd x && gh pr create` が永久にブロックされる）と引き換えである点を `/plan-review` に問う**。`readGitState` の注入により、分岐表 9 行すべてが git 無しでテストできる
8. **破壊不変条件と検知手段** — 「壊れたら即アウト」は 2 つ。(a) **`.claude/settings.json` が不正 JSON になると全 hook が停止する** → 編集が `hook-selftest` を自動発火し JSON を検証する（既存機構・I8）。(b) **hook が fail-open に倒れると `gh pr create` の安全網が消える** → 検知手段は §6 の故障注入 V1・V2（block されること）と unit E1（**exit 1 でないこと**の明示 assert）。いずれも「書かれた期待」ではなく実測である
