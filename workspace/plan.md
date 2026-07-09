# plan: issue #471 — hooks の根治

## 設計の核

現行 5 hook は「payload 全体を grep して発火 → 固定の場所で検査 → プレーン stdout で出力」だった。3 段すべてが壊れている。

```
payload → JSON.parse → tool_input.file_path                       （§1: 判定の入力を正す）
        → root = file_path から最近接の .git を遡って導出          （§2: 検査するツリーを正す）
        → rel  = root からの相対パス（/ 区切りに正規化）
        → checks = selectChecks(rel)                              （§5: 検査の範囲を tsconfig に揃える）
        → 各 check を cwd=root で実行し、進捗行を除去し、予算を適用  （§4: 予算を意味あるものにする）
        → JSON エンベロープに載せて exit 0                          （§6: 出力をエージェントへ届ける）
```

### §6 は issue に書かれていない 6 つ目の課題

**本セッションの実測 15（research.md）で確定した事実**: PostToolUse hook の stdout は**エージェントに届かない**。hook は Edit のたびに走り（buildinfo の mtime で確認）、診断も生成される（tsc 直接実行で確認）が、会話には一行も現れない。公式ドキュメントの記述と一致する。

したがって:

- `CLAUDE.md:51`「Edit/Write 後に会話へ流れる clippy / typecheck 出力はこのフック由来であり、手動での再実行は不要」は **事実と異なる**
- issue §4「出力予算」は、**誰も受け取っていない出力**の予算配分を論じていた
- 「hook が発火の判定に使う情報と、実際に検査する対象がずれている」という issue の根本原因の言明は、**出力の宛先にも及ぶ**。判定・対象・宛先の三つが揃って初めて安全網になる

**決定（ユーザー合意済み）**: exit 0 を保ったまま stdout に JSON エンベロープを出す。

```json
{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"<診断>"},"systemMessage":"<人間向け警告>"}
```

### §2（`cd` の除去）と §3（SSOT 化）は同じ一手で解ける

`/cache-check` で判明した事実: **`tsBuildInfoFile` は tsconfig のあるディレクトリ基準で解決される**（CLI 引数で渡すと cwd 基準）。

現行 hook がフラグを CLI で渡していたため buildinfo のパスが cwd 依存になり、それを固定するために `cd /c/workspace/Snotra` が必要だった。**フラグを tsconfig へ移すと、buildinfo は「その tsconfig のツリー」へ自動的に追従する。** §3 の SSOT 化が §2 の絶対パス `cd` を不要にする。二つの課題は独立ではなく、同一の設計欠陥（設定の所在が実行文脈に依存していた）の両面だった。

### root を `file_path` から導く理由

`CLAUDE_PROJECT_DIR` が worktree で何を指すかは実測していない。**この値に依存しない設計にすれば、答えを知る必要が無くなる。** `.git` はメインツリーではディレクトリ、worktree ではファイルとして必ず root に存在する（research 実測 8）。

### コンパイラの root と検査対象の root を分ける

worktree に `node_modules` は無い（実測 9）。よって:

- **検査対象のツリー** = `findUp(dirname(file_path), '.git')`
- **`tsc` バイナリ** = `findUp(root, 'node_modules/typescript/bin/tsc')` — worktree からは上位の main ツリーで見つかる

依存の型解決も Node の上方探索で main ツリーの `node_modules` に到達する（実測 11）。main ツリーの tsc で worktree の tsconfig を検査して exit 0 を確認済み（実測 10）。

**前提**: この設計は「worktree が main ツリーの内側（`.claude/worktrees/`）にある」ことに依存する。外へ移すと tsc が解決できなくなる。

---

## 変更ファイル一覧

| # | ファイル | 変更内容 | 新規 |
|---|---|---|---|
| 1 | `.claude/hooks/post-edit.mjs` | ディスパッチャ本体。純関数 + `main()` | ✅ |
| 2 | `.claude/hooks/post-edit.test.mjs` | 純関数のユニットテスト（誤爆・スコープ・ドリフトの回帰） | ✅ |
| 3 | `.claude/settings.json` | PostToolUse の 5 hook → 1 本。**`"timeout": 900` を明示**（下記 B4） | |
| 4 | `tsconfig.json` | `compilerOptions` に `incremental` / `tsBuildInfoFile` を追加 | |
| 5 | `package.json` | `"typecheck": "tsc --noEmit"` → `"tsc"` | |
| 6 | `vitest.config.ts` | `include` に `.claude/hooks/**/*.test.mjs` を追加 | |
| 7 | `CLAUDE.md` | **L43 見出し**・**L45**（「発火条件の SSOT は settings.json」→ `post-edit.mjs` へ移る）・**L51**（「会話へ流れる」は実測 15 で偽。訂正必須） | |
| 8 | `docs/build-commands.md` | L20–21 の発火条件記述を更新。**現行から既に漏れている `snotra-settings` テストの自動発火も as-built に合わせて追記** | |

### 変更しないものと根拠（AGENTS.md「変更なしの根拠明示」）

| 対象 | 根拠 |
|---|---|
| `SPEC.md` | `SPEC.md:7` が「実装運用ルールは `AGENTS.md` を参照」と明記し、`SPEC.md:5` は実装事実の参照先を `snotra-core/src/*.rs` / `src-tauri/src/*.rs` / `ui/src/**` に限定。**hook は SPEC の管轄外だと SPEC 自身が宣言している**。grep でも hook / フックは 0 件。§7「設定画面」・§13.1「設定データ」はアプリの `config.toml` であって Claude Code の `settings.json` ではない |
| `AGENTS.md` / `docs/architecture.md` / `docs/development-principles.md` | hook / PostToolUse の記述 0 件（grep 実測） |
| `.claude/agents/*.md` / `.claude/rules/*.md` | 同 0 件 |
| `.claude/skills/*/SKILL.md` | hook 言及は `implement/SKILL.md:80`（PreToolUse の `block-main-commit`。本変更と無関係）と `start-issue/SKILL.md:111`（Win32 の hook。同名別概念）のみ |
| 各サブディレクトリ `CLAUDE.md` | hook 言及なし。また新規ファイルは `.mjs` であり、`AGENTS.md` のモジュール構成同期ルールは対象を `.rs` / `.ts` / `.tsx` と明記 |
| `docs/superpowers/plans/2026-06-27-instant-command-exec-action.md:18` | 実施済み計画の史料。日付入りの過去記録であり as-built ドキュメントではない |
| `e2e/` | 中身は `tauri.slash.e2e.ts` 1 本のみ。`typecheck\|tsconfig\|hook` の grep 0 件。`e2e/*.ts` は `tsconfig.json:16` の `include: ["ui/src"]` により**今日も型検査されていない**（実測 5 の TS7016 が証拠）。今回 `include` は触らないので状態は不変 |
| `.claude/settings.local.json` | `permissions` キーのみを持ち `hooks` キーを持たない（実測）。PostToolUse の再構成と衝突しない（`AGENTS.md:49` の後方互換確認） |

---

## 検査対応表（`selectChecks` の仕様）

`rel` は root からの相対パス（`/` 区切り）。**上から順に独立に評価し、マッチしたものすべてを実行する。**

| id | 条件（`rel` に対する判定） | 実行（**I10: 一字一句保存**） | 予算 | 出力先 |
|---|---|---|---|---|
| `clippy` | `*.rs` | `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets --message-format short -- -D warnings` | head 20 | `additionalContext` |
| `core-test` | `snotra-core/**/*.rs` | `cargo test -p snotra-core --lib` | tail 5 | `additionalContext` |
| `settings-test` | `snotra-settings/**/*.rs` | `cargo test -p snotra-settings` | tail 8 | `additionalContext` |
| `typecheck` | `ui/src/**/*.{ts,tsx}` **かつ** `*.test.{ts,tsx}` でない | `node <tscBin> -p <root>/tsconfig.json` | head 30 | `additionalContext` |
| `config-warn` | `**/tauri.conf.json` または `**/config.toml` | コマンド実行なし | — | `systemMessage` |

- **ストリーム**: 各検査は `stdout` + `stderr` を連結してから予算を適用する（現行 `2>&1` 相当。clippy は stderr、tsc は stdout に診断を出す = I12）
- **`config.toml` は追跡下に 0 件**（`git ls-files` 実測。実在するのは `src-tauri/tauri.conf.json` のみ）。ランタイムのユーザー領域ファイルなので Edit 対象にならず、**この分岐の真陽性は構造的にゼロ**。I10 に従いパターンは保存するが、実質的に `tauri.conf.json` 専用である事実を記録する
- **1 ファイルは `.rs` か `.ts` のどちらかなので、cargo 系と tsc 系は同時発火しない**。並行実行の設計は不要

### 現行との差分（意図的な挙動変更）

| ケース | 現行 | 変更後 | 根拠 |
|---|---|---|---|
| **すべての検査結果** | **エージェントに届かない**（stdout は debug log 行き） | **`additionalContext` で届く** | §6 / 実測 15 |
| `docs/notes.md` を Write、content 末尾が `ui/src/api.ts` | typecheck が発火 | 発火しない | §1 |
| `src-tauri/src/main.rs` を Edit、`old_string` に `snotra-core` の語 | clippy + **core test** が発火（`.*` は同一行のどこかにマッチ。payload は 1 行 JSON） | **clippy のみ** | §1 |
| `snotra-core/src/config.rs` を Edit | clippy + core test + **config-warn**（`:40` だけ引用符アンカーが無く、content 中の `config.toml` の語に反応） | **clippy + core test** | §1 |
| `e2e/*.ts` / `vite.config.ts` / `ui/src/**/*.test.ts` を Edit | typecheck が発火（が tsconfig 対象外なので無意味） | 発火せず、**「型検査対象外」の一行を出す**（I16） | §5 |
| **worktree 内の `ui/src/api.ts`** を Edit | main ツリーを検査（偽 green） | worktree を検査 | §2 |
| **worktree 内の `snotra-core/src/lib.rs`** を Edit | **main ツリーで clippy + core test**（hook の cwd はセッションの cwd） | **worktree で clippy + core test**。`target/` 不在のため**初回フルビルド**（進捗行の除去が必須 = I14） | §2 |
| リポジトリ外の `*.rs` を Edit | clippy が発火 | 発火しない | I6 |

---

## 実装順序（フェーズ分け）

各フェーズは**検証 green を確認してからコミット**する（AGENTS.md 環境制約: 中断を前提に設計）。

> **順序の要件化**: `hooks-guide.md:776` によれば settings ファイルの編集は file watcher が自動で拾う（**ドキュメント根拠のみ・実機未検証**）。つまり `settings.json` を差し替えた瞬間から新 hook が効く。**合成 payload スモークを差し替え「前」に完了させること**は望ましさではなく要件。

### Phase 1 — 純関数 + テスト（Red → Green）

公開関数:

| 関数 | 責務 |
|---|---|
| `findUp(startDir, relTarget)` | 祖先を遡り `relTarget` を含むディレクトリを返す（無ければ `null`） |
| `resolveRoot(filePath)` | `findUp(dirname(filePath), '.git')` |
| `toRelative(root, filePath)` | `path.relative` → `/` 区切りに正規化 |
| `extractFilePath(payload)` | `JSON.parse` 済みオブジェクトから `tool_input.file_path` を取り出す（無ければ `null`） |
| `selectChecks(rel)` | `rel` → check id の配列（純関数） |
| **`checksForPayload(payload, rootResolver)`** | payload → check id の配列。**§1 の回帰を単一アサーションで固定**するための合成（下記 C1） |
| `stripProgressLines(text)` | `^\s*(Compiling\|Checking\|Finished\|Blocking\|Updating\|Downloading\|Downloaded)\b` の行を除去（I14） |
| `formatOutput(text, budget)` | head/tail の予算適用 + 切り捨て通知（I9） |
| `resolveTscBin(root)` | `findUp(root, 'node_modules/typescript/bin/tsc')` |

手順:

1. `.claude/hooks/post-edit.test.mjs` を先に書く（import 先が無く **Red**）
2. `vitest.config.ts` の `include` を拡張
3. `.claude/hooks/post-edit.mjs` に**純関数のみ**実装 → **Green**

検証: `npm test`

### Phase 2 — ランナー + settings.json 差し替え + 実機確認

4. `main()` を実装。**必ず直接起動ガードで囲む**（下記 I13）
5. **合成 payload スモーク**（10 ケース）をスクリプト直接起動で実行
6. `.claude/settings.json` を 1 本化（`"timeout": 900` 込み）。`node -e "JSON.parse(...)"` で JSON 妥当性を検証
7. **実機確認（必須）**: `ui/src` の適当なファイルに一時的な型エラーを入れて Edit → **`additionalContext` の診断が会話に現れることを目視** → 取り消す
   - これは本セッションで実施した実測 15 と同じ手順であり、**期待値が反転する**（届かない → 届く）ことを確認する
   - 届かなければ案 B（exit 2 + stderr）へ切り替える。plan を更新してから進む

検証: 合成 payload スモーク + settings.json パース + 実 Edit 1 回

### Phase 3 — typecheck 定義の SSOT 化

**目的は SSOT であって速度ではない**。実測: cold 1.96s / warm 1.33s → incremental の利得は約 0.6s。hook は既に npm を経由していないため、issue の「`npm run typecheck` 2802ms」との差は npm 起動分であり、Phase 3 で新たに得られるものではない。

8. `tsconfig.json` に `"incremental": true` / `"tsBuildInfoFile": "node_modules/.cache/typecheck.tsbuildinfo"` を追加
9. `package.json` の `typecheck` を `"tsc"` に変更
10. 旧 `node_modules/.cache/hook-typecheck.tsbuildinfo` を削除（孤児の掃除。無害だが as-built に合わせる）

検証:

```bash
npm run typecheck        # cold / warm 2 回
npm run build
npx --yes -p typescript@6.0.3 tsc -p tsconfig.json   # CI と同じ TS 6 で成立を確認
```

> `npx typescript@6.0.3 tsc` は `could not determine executable to run` で落ちる（bin 名が `tsc`/`tsserver` のため）。**`-p` でパッケージを指定する**こと。

**影響する全実行経路（実測で裏取り）**:

| 経路 | 定義 | Phase 3 後の挙動 |
|---|---|---|
| `npm run typecheck` | `tsc`（フラグは tsconfig 側） | incremental 化。**warm で高速化** |
| `npm run prebuild` | `npm run typecheck` | 同上 |
| `npm run build` | `prebuild` → `vite build` | vite は esbuild を使い tsc の buildinfo を読まない。影響なし |
| `npm run verify` | `cargo check ...` → `npm run build` | 上記経由で影響なし |
| CI `frontend-check` | `npm ci` → `npm test` → `npm run build` | **`cache: npm` は npm のグローバルキャッシュ（`~/.npm`）のみを復元し、`node_modules` は復元しない**（actions/setup-node の README に "The action does not cache `node_modules`" と明記）。`node_modules` は `.gitignore:3` によりチェックアウトにも含まれず、`npm ci` が毎回作り直す。よって **CI の buildinfo は常に cold** |
| `npx tauri build` / `npm run tauri build` | `src-tauri/tauri.conf.json:10` の `beforeBuildCommand: "npm run build"` | typecheck に到達する。**script 名不変のため自動追随** |
| `npm run e2e:tauri:setup` | 内部で `npx tauri build --no-bundle` | 同上 |
| CI `e2e.yml`（windows-latest） | 同上 | 同上 |
| CI `release.yml` | 同上 | 同上 |
| `npx tsc` 直叩き（`settings.local.json` で許可済み） | — | Phase 3 後は**従来書かなかった buildinfo を書く**。出力先は gitignore 済みで無害 |
| VS Code の TS server | — | tsc を起動せず buildinfo を読み書きしない。無影響 |
| `vite build` / `vitest` | esbuild 変換 | `incremental` / `tsBuildInfoFile` は esbuild のオプション集合に無い。型検査を一切しない。無影響 |

`★ CI は cold・ローカルのみ warm` という非対称は**安全側**である。CI で incremental の恩恵は無いが、stale buildinfo による偽 green も構造的に起こりえない。

### Phase 4 — ドキュメント同期

11. `CLAUDE.md` L43 / L45 / **L51（誤記の訂正）** を更新
12. `docs/build-commands.md` L20–21 を更新（`snotra-settings` テストの発火も追記）

検証: 記述と `.claude/settings.json` / `post-edit.mjs` の対応を目視照合

---

## 不変条件

| id | 不変条件 | 破れたときの症状 |
|---|---|---|
| I1 | hook は**常に exit 0** で終わる。`process.exit(0)` は使わず **`process.exitCode = 0`** で自然終了する | `process.exit()` は stdout がパイプのとき未 flush 出力を切り捨てる。診断が消える |
| I2 | 検査出力は **stdout に JSON エンベロープ**として出す（`hookSpecificOutput.additionalContext`）。人間向け警告は `systemMessage` | プレーン stdout は debug log 行きでエージェントに届かない（実測 15） |
| I3 | `tool_input.file_path` が無ければ**何もせず** exit 0 | matcher `Edit\|Write` は**完全一致リスト**なので `NotebookEdit` は発火しない。このガードは「payload 破損・将来の matcher 変更」への保険 |
| I4 | 検査は `file_path` が属するツリー（最近接 `.git`）を `cwd` として実行する | worktree で偽 green（§2） |
| I5 | `tsc` バイナリは `root` から**上方へ**探索して解決する | worktree に `node_modules` が無く ENOENT |
| I6 | `file_path` がどの `.git` にも属さないなら**何もせず** exit 0 | リポジトリ外のファイル編集でリポジトリの検査が走る |
| I7 | typecheck の発火条件 ⊆ `tsconfig.json` の `include` − `exclude` | 検査されないファイルで「検査が通った」と誤認（§5） |
| I8 | スクリプト内部エラーは捕捉し、**`additionalContext` と `systemMessage` の両方**に `HOOK ERROR: ...` を出して exit 0 | 安全網が沈黙して消える（本 issue が最も嫌う失敗様式）。エージェントにも人間にも見せる |
| I9 | 出力を切り捨てたら、**切り捨てた事実と総行数**を明示する。head なら通知は**末尾**、tail なら**先頭** | 通知を反対側に置くと通知自体が切り捨て領域に落ちて消える |
| I10 | **検査コマンドは現行を一字一句保存する**（`--all-targets` / `--message-format short` / `-- -D warnings` / `--lib` / 予算の行数と head/tail の別） | 本 PR はディスパッチ機構の置換であり検査集合の変更ではない。混ぜると回帰の切り分けが不能 |
| I11 | 子プロセスは `process.execPath` で起動し、`maxBuffer` を明示的に引き上げる（32MB） | Windows の PATH 依存と、出力 1MB 超での `ENOBUFS` 沈黙を防ぐ |
| I12 | 出力は `stdout` + `stderr` を連結して予算を適用する（現行 `2>&1` 相当） | clippy は stderr、tsc は stdout に診断を出す。片方だけ見ると診断が消える |
| I13 | `main()` は**直接起動時のみ**実行する: `if (import.meta.url === pathToFileURL(process.argv[1]).href) main()` | vitest が import しただけで stdin 読み取りが走り、`npm test` が停止する。Phase 1 の green が Phase 2 で壊れる |
| I14 | 予算適用の**前に** cargo の進捗行を除去する | I4 で worktree（cold build）に移った途端、`Compiling ...` が数十〜数百行流れ `head 20` が進捗行だけで埋まる。**I4 が §4 を悪化させる**相互作用 |
| I15 | JSON エンベロープは**必ず妥当な JSON** である。診断文字列は `JSON.stringify` に委ねる | 診断に含まれる `"`・改行・パスの `\` が生の連結でエンベロープを壊す |
| I16 | `.ts` / `.tsx` を編集したのに検査が 0 件なら、**「型検査対象外」の一行を出す** | §5 の偽 green が「無言」という別形態で残る。沈黙も誤情報 |
| I17 | `resolveTscBin` は必ず**フルパス `node_modules/typescript/bin/tsc`** で probe する。`node_modules` ディレクトリの存在で判定してはならない | Phase 3 後、hook 自身が `<worktree>/node_modules/` を作る（実測 19）。`findUp(root,'node_modules')` だと **2 回目以降**その空ディレクトリで探索が止まり tsc が見つからない。しかも `ENOENT` ではなく「候補なし」なので **I8 の HOOK ERROR にも捕捉されず沈黙する**。順序依存かつ無音 = 本 issue が最も憎む失敗様式を、修正自身が作り込む |
| I18 | typecheck の診断は **warm run でも replay される**ことに依存する | 実測 18: 無変更の 2 回目もエラー 1 件を再報告（exit は 2 → 1 に変わる）。replay されなければ「2 回目以降だけ安全網が沈黙する」失敗様式を新規に作り込むことになる |
| I19 | `selectChecks` の `ui/src` 判定は**深さ 0 のファイルを含む**こと | `git` の `**/` は 1 段以上を要求するが、TypeScript の `exclude` は 0 段にマッチする（実測 17）。`^ui/src/.+/.+\.test\.tsx?$` のような「1 段以上」正規表現は実在の `ui/src/MainApp.test.tsx` で §5 を再現する |

### 異常系・順序の想定

新たな常駐状態（`AtomicBool`・ウィンドウ・常駐プロセス）は**導入しない**。

| 異常 | 挙動 |
|---|---|
| stdin が空 / 不正 JSON | catch → `HOOK ERROR`（I8）、exit 0 |
| `file_path` が相対パス | `path.resolve()` で正規化してから root 探索 |
| `file_path` のファイルが既に消えている | root 探索は**ディレクトリ**を遡るため成立。検査コマンド側が自然に失敗し出力に出る |
| 検査コマンドが存在しない（`cargo` 不在等） | `spawnSync` の `error` を捕捉し `HOOK ERROR` |
| `tsc` が見つからない | `typecheck` だけスキップし他の検査は続行 |
| **検査の出力が 1MB を超える** | `spawnSync` の `maxBuffer` 既定は **1MB**。超えると `ENOBUFS` で出力が失われる。**現行はパイプなので上限が無く、これは置換で新たに生まれるリスク**。`maxBuffer: 32*1024*1024` を明示。`error` と `status === null` の扱いを分け、**診断が出ているのに ENOBUFS で HOOK ERROR に化けて捨てられる**ことを防ぐ |
| 検査が長時間ハング | `settings.json` の `"timeout": 900` に委ねる（下記） |
| 同一 buildinfo への並行書き込み | tsc がバージョン/整合を検査し不正なら再ビルド。**本セッションで 3 並列を実測: exit `0 0 0`・valid JSON・後続実行も clean**。復旧は `node_modules/.cache` の削除 |

**タイムアウト予算の縮小**: 「all matching hooks run in parallel」（`hooks-guide.md:439`）のため、現行は 5 hook が**各自の予算**を持つ。1 本化すると `clippy` → `core-test` が**単一予算を直列に共有**する。worktree の cold build は分オーダー（research 技術的制約 6）。よって `"timeout": 900` を明示する。これは「タイムアウト機構の新規導入」ではなく、**予算縮小の補償**である（YAGNI に抵触しない）。

### 生成されるリソース（生成/破棄ペア）

| 生成物 | 生成者 | 破棄 | 安全性 |
|---|---|---|---|
| 検査の子プロセス 1 個 | `spawnSync` | 同期的に回収（`kill` 不要） | リークなし |
| `<root>/node_modules/.cache/typecheck.tsbuildinfo` | tsc（親ディレクトリごと自動生成） | 永続。誰も消さない | 無害。復旧は削除のみ |
| `<worktree>/node_modules/`（worktree 検査時の副産物） | tsc が buildinfo を書くとき自動生成 | worktree 削除に道連れ | 下記参照 |
| 旧 `node_modules/.cache/hook-typecheck.tsbuildinfo` | 現行 hook | Phase 3 手順 10 で明示削除 | 無害 |

**worktree 副産物の帰結（`/cache-check` Step 5 で実測）**:

- tsc は `tsBuildInfoFile` を **tsconfig のあるディレクトリ基準**で解決する。worktree の tsconfig を指すと `<worktree>/node_modules/.cache/` が**新規に生まれる**（`node_modules` と `.cache` の二段を再帰生成）
- 生まれた空の `<worktree>/node_modules` が依存解決を遮ることは**ない**（buildinfo 削除後のフル型検査で exit 0 を実測）。Node/TS の解決は見つからなければ上位へ遡り続ける
- worktree 内の `.gitignore` も `/node_modules` を含むため**誤コミットは起きず、`git status` も clean のまま**
- Claude Code の worktree 自動 cleanup が `git status --porcelain` で判定するのかファイルシステム上の存在で判定するのかは**未確認**。どちらであれ、`.ts` を編集したエージェントはファイルを変更済みで手動 cleanup 対象なので実害は増えない

### 破壊不変条件

「壊れたら即アウト」なのは **hook そのものが安全網**である点。

| 破壊 | 検知手段 |
|---|---|
| `post-edit.mjs` が壊れて**全 PostToolUse 検査が沈黙**する | (a) `npm test` のユニットテスト、(b) 合成 payload スモーク 10 ケース、(c) I8 により内部エラーは `additionalContext` + `systemMessage` の両方に出る |
| **`settings.json` 差し替え直後から新 hook が効く**（file watcher）ため、壊れたスクリプトが即座に全検査を無効化する | Phase 2 の順序（スモーク → 差し替え）を**要件**とする |
| `.claude/settings.json` の JSON が壊れて**`block-main-commit` を含む全 hook が停止**する | Phase 2 手順 6 の `JSON.parse` 検証をコミット前に必ず実行 |
| JSON エンベロープが壊れて出力が届かない（届いているつもりで届かない = 実測 15 の再来） | I15 のユニットテスト（`JSON.parse(emit(...))` が成功する）+ Phase 2 手順 7 の**実 Edit による目視確認** |

---

## テスト方針

### 前提の実測（本セッションで検証済み）

使い捨て probe で `vitest -c <include: .claude/hooks/**/*.test.mjs>` を実行 → **5 passed / exit 0**。`vite-plugin-solid` は `.mjs` に干渉しない（transform の正規表現 `/\.[mc]?[tj]sx$/i` が末尾 `x` を要求するため）。vitest は glob に `dot: true` を渡すので `.claude/` 配下も拾う。

**CI 移植性の制約**: CI は `ubuntu-latest`（`ci.yml:20`）で `npm test` を走らせる。テスト内に `C:\workspace\Snotra\...` のような **Windows リテラルパスを書いてはならない**（`path.relative` の挙動が Linux で変わり CI が落ちる）。パスは `path.join()` で組み立てるか、`rel` を直接与える純関数テストに寄せる。

### C1 — §1 誤爆の回帰は `checksForPayload` で固定する

`selectChecks("docs/notes.md") === []` は §1 を**証明しない**。§1 は*抽出*のバグであって*選択*のバグではない。`extractFilePath` と `selectChecks` を別々にテストしても「抽出は正しいが選択へ渡す配線を間違えた」失敗を捕まえられない。

→ `checksForPayload(payload, rootResolver)` を切り出し、**payload 丸ごとから check id 配列まで**を単一アサーションで固定する。

| payload | 期待 |
|---|---|
| `{"tool_name":"Write","tool_input":{"file_path":"<root>/docs/notes.md","content":"参照は ui/src/api.ts"}}` | `[]` |
| `{"tool_name":"Write","tool_input":{"file_path":"<root>/AGENTS.md","content":"実装は snotra-core/src/lib.rs"}}` | `[]` |
| `{"tool_name":"Edit","tool_input":{"file_path":"<root>/src-tauri/src/main.rs","old_string":"snotra-core","new_string":"x"}}` | `["clippy"]` |
| `{"tool_name":"Edit","tool_input":{}}` | `[]`（I3） |

### C2 — §5 の回帰は「tsconfig への追随」を証明する必要がある

I7 は現時点で厳密成立（`tsconfig.json:16-17`、`ui/src` に `.mts`/`.cts` は 0 件と実測）。しかし hook 側の期待値をハードコードするだけでは、**tsconfig が変わったときに落ちるテストがない**。§3/§5 の根（SSOT 分裂）が形を変えて残る。

→ **ドリフト検出カナリア**を一本置く: `tsconfig.json` を読み、`include` / `exclude` が期待リテラルと一致することを検証する。将来 `include` を触った人がこのテストで気づく。

**真実源は `tsc --listFilesOnly` である**（glob の見た目ではない。実測 17）。現在 program の `ui/src` ファイルは 24 件で、git 上の非テスト `.ts`/`.tsx` 24 件と**両方向で差分ゼロ**（`vite-env.d.ts` を含む）。I7 は今日「⊆」ではなく「＝」で成立している。

将来の穴（現状は無害・`⊆` は保たれる）:

| ケース | tsconfig の `include` 展開 | plan の `{ts,tsx}` glob | 帰結 |
|---|---|---|---|
| `.mts` / `.cts` | **拾う** | マッチしない | typecheck が発火せず、しかしファイルは検査される（安全側の取りこぼし。`ui/src` に現在 0 件） |
| `.json`（`resolveJsonModule: true`） | 拾わない | マッチしない | ただし **import されると program に入る**（実測）。編集しても typecheck は走らない |

### C3 — `selectChecks(rel)` のケース表

| 入力 `rel` | 期待する check id |
|---|---|
| `ui/src/api.ts` | `["typecheck"]` |
| `ui/src/components/SearchWindow.tsx` | `["typecheck"]` |
| `ui/src/lib/i18n.test.ts` | `[]` ← §5（深さ 2） |
| **`ui/src/MainApp.test.tsx`** | `[]` ← **§5 かつ I19。実在ファイル・深さ 0。「1 段以上」正規表現はここで壊れる** |
| `e2e/tauri.slash.e2e.ts` | `[]` ← §5 |
| `vite.config.ts` | `[]` ← §5 |
| `snotra-core/src/lib.rs` | `["clippy","core-test"]` |
| `snotra-core/src/config.rs` | `["clippy","core-test"]` ← `config-warn` を誤発火しないこと |
| `snotra-settings/src/main.rs` | `["clippy","settings-test"]` |
| `src-tauri/src/lib.rs` | `["clippy"]` |
| `src-tauri/build.rs` | `["clippy"]` |
| `src-tauri/tauri.conf.json` | `["config-warn"]` |
| `.claude/settings.json` | `[]`（自己参照ループが無いこと） |
| `docs/notes.md` | `[]` |

`resolveRoot(file)` — 一時ディレクトリに `.git`（**ファイル**）を置いて worktree を模し、最近接が選ばれること / ネストで内側が勝つこと / どの `.git` にも属さないパスで `null`（I6）。

`resolveTscBin(root)` — **I17 の回帰テスト**: `root` 直下に**空の `node_modules/` を置いた状態**で、上位ツリーの `node_modules/typescript/bin/tsc` を正しく見つけること。これは 2 回目以降の hook 実行を模す（実測 19）。

`formatOutput(text, budget)` — 対称性が要点:

| ケース | 期待 |
|---|---|
| `{lines: 20, from: 'head'}` で 10 行 | そのまま 10 行、**通知なし** |
| `{lines: 20, from: 'head'}` で 50 行 | 先頭 20 行 + **末尾**に切り捨て通知 |
| `{lines: 5, from: 'tail'}` で 50 行 | **先頭**に切り捨て通知 + 末尾 5 行 |
| 空文字列 | 空を返す（通知なし） |

`stripProgressLines(text)` — `Compiling snotra-core v0.1.0` を落とし、`error[E0308]: ...` を残す。

`emit(...)` — 出力が `JSON.parse` 可能であること（I15）。診断に `"` と改行と `C:\path\to` を含めて検証。

### 合成 payload スモーク（Phase 2 手順 5・手動）

```bash
echo '{"tool_name":"Write","tool_input":{"file_path":"C:/workspace/Snotra/docs/notes.md","content":"参照は ui/src/api.ts"}}' | node .claude/hooks/post-edit.mjs
# → 何も出力されず exit 0
```

C3 の 14 ケースを実 payload 形式で。

> **予言（`review-hooks` より）**: Phase 1 で `post-edit.test.mjs` を書くとき、フィクスチャ文字列 `"snotra-core/src/lib.rs"` や `"ui/src/api.ts"` は末尾が `.rs"` / `.ts"` の形になるため、**旧 hook の payload 全体 grep が反応して clippy と core test と typecheck が走る**。issue §1 の誤爆が、それを葬るテストを書く手つきの上で最後に一度だけ立ち現れる。騒がしいが故障ではない。

### 検証コマンド（`docs/build-commands.md` 参照）

- **カテゴリ A（Rust）**: 該当なし（`.rs` を変更しない）
- **カテゴリ B（TS/フロント）**: `npm run typecheck` / `npm run build` — `tsconfig.json`・`package.json`・`vitest.config.ts` を触るため必須
- `npm test` — vitest include 変更と新規テストのため必須
- **カテゴリ C・D**: 該当なし

---

## スコープ外（YAGNI 境界）— follow-up issue に起票済み

**`AGENTS.md:87` に従い、PR 作成前に起票した。PR 本文にも同じ番号を記入すること。**

| issue | 項目 | severity |
|---|---|---|
| **#473** | **PreToolUse hook 2 本の同根修正** — 本 issue は PostToolUse を対象と明記（issue §1「4 つの PostToolUse hook」）。ただし根は同一 | **本 issue より重い。`block-main-commit` は `git branch --show-current` を hook の cwd で評価するため、`git -C <別ツリー> commit` や worktree で判定対象と実際のコミット先がずれ、main への直コミットを通しうる（fail-open）。PostToolUse 側の「偽 green」は fail-closed** |
| #474 | `tsconfig.json` の `include` 拡張（e2e / config / test）。既存の型エラー 9 件の修正が先行条件（実測 5） | 中。§5 の副作用として `ui/src/**/*.test.{ts,tsx}` 14 本が**どの安全網にも掛からなくなる**（実測 5 の 8 件は誰も検知しない） |
| #475 | `cargo test -p snotra`（src-tauri の **`#[test]` 68 個**）が hook（`settings.json:28,32`）にも CI（`ci.yml:66,69`）にも無い。加えて `ui/src/lib/cspValidation.test.ts` が `src-tauri/tauri.conf.json` の CSP を検証する**本物の契約テスト**なのに、hook は WARN を echo するだけ（「判定情報と検査対象のずれ」の 5 例目） | 中 |
| #476 | hook の `cargo test -p snotra-core --lib` と `docs/build-commands.md:16`（`--lib` 無し）の乖離。§3 と同型の SSOT 分裂が Rust 側にも存在。かつ `/health-check` Check 5 は `.claude/hooks/` を見ないため**検知器が無い** | 低 |
| #477 | worktree での cargo 検査が毎回 cold full build（`target/` が worktree に無い）。I4 の副作用。**まず計測**、次に `CARGO_TARGET_DIR` 共有を検討 | 低 |

**起票しなかったもの**: `node_modules`（TS 5.9.3）と `package-lock`（TS 6.0.3）の乖離は、リポジトリの欠陥ではなく**ローカル `node_modules` の陳腐化**。`npm ci` で解消する。ただしローカル green / CI red の乖離を生みうるため、Phase 3 の検証は CI と同じ TS 6.0.3 でも行う（Phase 3 の検証コマンド参照）。

**誤削除防止（#475 に記載）**: `cspValidation.ts`（実装）は存在せずテストのみだが、#409 で削除した孤児 `hotkeyValidation.ts` と違い**死蔵ではない**。`tauri.conf.json` に対する契約テストである。

---

## セルフレビュー

### 5a. check スキルによる計画検証

| スキル | 実施 | 主な発見 |
|---|---|---|
| `/plan-review` | ✅ サブエージェント 3 体（hook 層 / TS ツールチェーン層 / ドキュメント・スコープ層）+ Step 2b 独立再導出 1 体 | **A1（PostToolUse stdout がエージェントに届かない）を発見 → 本セッションで実測確定**。他に `maxBuffer`・`main()` ガード・進捗行・`checksForPayload`・ドリフトカナリア・timeout 予算縮小 |
| `/plan-review` Step 2b（独立再導出） | ✅ `Plan` エージェント 1 体に plan/research を読ませず issue + コードのみから再導出させ差分 | **枠組み独立の 2 体が同一の 2 点に収束**: (1) timeout 予算が 5 個 → 1 個の直列共有に縮む、(2) §5 の再発は tsconfig と述語のドリフト検出でしか止まらない。両方とも `/plan-review` Step 2 の 3 体は挙げなかった |
| `/cache-check` | ✅ | 全述語で単調性が保証され安全。副産物として **§2 と §3 が同一設計欠陥の両面**であることが判明 |
| `/symmetric-check` | ✅ | `maxBuffer`・`process.execPath`・stdout+stderr 連結・`formatOutput` の通知位置の対称性・src-tauri テストの欠落・`--lib` の SSOT 分裂 |
| `/race-check` | ⏭️ 対象外 | async 関数を導入しない（`readFileSync(0)` + `spawnSync` の同期実装） |
| `/state-check` | ⏭️ 対象外 | UI モード・状態遷移に触れない |
| `/persistence-check` | ⏭️ 対象外 | アプリの on-disk 形式（index.bin / config.toml / history / window.bin）に触れない。tsbuildinfo は `/cache-check` で検証済み |

### 5b. チェックリスト

| # | 観点 | 結果 |
|---|---|---|
| 1 | 対称コードパス | `/symmetric-check` 実施。Edit/Write（両者とも `file_path` を持つ）、head/tail（通知位置が逆）、生成/破棄（`spawnSync` は同期回収、buildinfo は永続で無害）。PreToolUse は同根だが別 issue |
| 2 | 影響範囲の網羅性 | 全 `.md` を `PostToolUse\|PreToolUse\|フック\|hook\|自動発火\|自動実行\|自動検証` で grep。同期対象は `CLAUDE.md`（L43/45/51）と `docs/build-commands.md`（L20-21）のみ。変更不要の根拠は上表に明記 |
| 3 | 境界条件 | 異常系表 8 件 + C3 の 14 ケース + `formatOutput` の 4 ケース |
| 4 | リソース管理 | 生成/破棄ペア表 4 件。新たな常駐状態はゼロ |
| 5 | 既存パターンとの整合 | `docs/build-commands.md` が「コマンドの SSOT」として機能する構造の踏襲（tsconfig を typecheck の SSOT に据える） |
| 6 | YAGNI 違反 | なし。`"timeout": 900` は**新機能ではなく予算縮小の補償**。ユニットテストは「安全網自身の安全網」で破壊不変条件が正当化 |
| 7 | シンプル化の挑戦 | 新たな状態（`AtomicBool`・Mutex・子プロセス常駐）は導入しない。導入するのは短命な子プロセス 1 個と buildinfo 1 ファイルのみ。「この操作が失敗したらどうなるか」は異常系表に全件記述 |
| 8 | 破壊不変条件の明示 | 4 件を検知手段とセットで記述。**最重要は「settings.json 差し替え直後から新 hook が効く」ため、スモークを差し替え前に完了させることを要件化**した点 |

### 実測で覆した前提（記録）

| 当初の前提 | 実測結果 |
|---|---|
| hook の出力は会話に流れる（`CLAUDE.md:51`） | **届かない**（実測 15）。チーム全体の誤った信念の震源 |
| hooks は起動時スナップショットされる | ドキュメント上は file watcher が拾う（**実機未検証**） |
| exit 2 は PostToolUse でツール失敗として扱われる | **ブロックしない**。stderr を Claude に見せる正規経路の一つ |
| §3 と §4 は独立の小変更 | **§3 は §2 と同一設計欠陥の両面**。独立なのは §4 のみ |
| `head -N` は現行どおりで足りる | **I4（正しいツリー = cold build）が §4 を悪化させる**。進捗行の除去が必須 |
| §2 の `CLAUDE_PROJECT_DIR` チェックボックスは実測が要る | **実測せず、答えを不要にする設計**で閉じる（root を file_path から導出） |
