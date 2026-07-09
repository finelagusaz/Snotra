# research: issue #471 — hooks の根治

## issue の要約

`.claude/settings.json` の PostToolUse hook 群が抱える 5 つの課題を、共通の根 **「hook が発火の判定に使う情報と、実際に検査する対象がずれている」** に還元して根治する。

対症療法（個別パッチ）ではなく、hook が「どのファイルが変わったか」を構造化された形で受け取り、**そのファイルに対応する検査だけを、そのファイルが属するツリーで走らせる**形に置き換える。

`--pretty` による診断切り捨て（§4 の一部）は #472 で解消済み。

### 方針決定（ユーザー合意済み・2026-07-09）

`CLAUDE.md` 最重要ルール 4「エージェント設定の変更は合意してから」に基づき、着手前に 3 点を確認して合意を得た。

| 論点 | 決定 |
|---|---|
| hook のロジックの置き場所 | **外部スクリプト `.claude/hooks/post-edit.mjs` に集約**。settings.json の PostToolUse は 1 本に統合 |
| §5 スコープ不一致の解消方向 | **hook を tsconfig の `include` に合わせて絞る**。tsconfig 拡張は別 issue |
| §3 SSOT 分裂の解き方 | **tsc のフラグを `tsconfig.json` へ移す**。hook も npm script も素の `tsc` を呼ぶ |

---

## 実測ログ（すべて本セッションで trace 取得）

推測を排し、計画の前提を実測で裏取りした。

| # | 検証した命題 | 実測結果 |
|---|---|---|
| 1 | `jq` は使えるか | **不在**（`command -v jq` → not found）。JSON 抽出は `node` で行う |
| 2 | `node` は使えるか | **v26.4.0**（ローカル）。CI は Node 22 |
| 3 | `CLAUDE_PROJECT_DIR` は通常シェルから見えるか | **UNSET**。hook 実行時のみ設定される |
| 4 | `tsc` は `tsBuildInfoFile` の親ディレクトリを自動生成するか | **する**。`node_modules/.cache` を `rm -rf` してから実行 → exit 0 で再生成。既存 hook の `mkdir -p` は**不要** |
| 5 | tsconfig の `include` を全 TS に広げると何が起きるか | **既存の型エラー 9 件**が露出（下表） |
| 6 | `package.json` / `package-lock.json` / 実体の typescript バージョン | 宣言 `^6.0.2` / lock **6.0.3** / 実体 **5.9.3** |
| 7 | CI と同じ TS 6.0.3 で `noEmit` + `incremental`（tsconfig 記述）は成立するか | **成立**。exit 0、buildinfo 生成 OK |
| 8 | worktree の `.git` はファイルかディレクトリか | **ファイル**（内容 `gitdir: C:/workspace/Snotra/.git/worktrees/<name>`） |
| 9 | worktree に `node_modules` はあるか | **無い**（gitignore 対象のため checkout されない） |
| 10 | main ツリーの `tsc` で worktree の `tsconfig.json` を検査できるか | **できる**。`node node_modules/typescript/bin/tsc -p <worktree>/tsconfig.json --noEmit` → exit 0 |
| 11 | worktree 内ソースから依存はどう解決されるか | 上位へ遡り **`C:\workspace\Snotra\node_modules`** を発見（worktree が main ツリー内側にあるため） |
| 12 | root 検出のアンカー適性 | `package.json` はルートに **1 つのみ**。`Cargo.toml` は **4 つ**（ルート + 3 crate）で最近接探索が crate で止まる |
| 13 | vitest の `include` | `["ui/src/**/*.test.{ts,tsx}"]` のみ。hook のテストを走らせるには追加が必要 |
| 14 | payload 全体 grep を行う hook の総数 | **7 箇所**（PostToolUse 5 + PreToolUse 2） |
| **15** | **PostToolUse hook の stdout は Claude（エージェント）に届くか** | **届かない。** 下記「実測 15 の詳細」参照 |
| 16 | hook は Edit のたびに実行されているか | **されている**。`hook-typecheck.tsbuildinfo` の mtime が Edit ごとに更新される（16:40:55 → 16:41:55） |

### 実測 15 の詳細 — 安全網は最初からエージェントに不可視だった

**手順**: `ui/src/lib/truncatePath.ts` に型エラー（`const __hookProbe: number = "..."`）を Edit で注入 → 会話を観察 → 直後に Edit で取り消し。

| 観測点 | 結果 |
|---|---|
| typecheck hook は実行されたか | **YES** — `node_modules/.cache/hook-typecheck.tsbuildinfo` の mtime が Edit 直後（16:40:55）に更新 |
| 診断は生成されたか | **YES** — `tsc --noEmit` を直接叩くと `truncatePath.ts(4,7): error TS2322: Type 'string' is not assignable to type 'number'.` / exit 2 |
| 診断は会話に現れたか | **NO** — 一行も届かず |
| 取り消し後の再現 | 2 回目の Edit でも hook は走り（mtime 16:41:55）、やはり何も届かず。型検査は exit 0 に復帰 |

**公式ドキュメントの記述と整合する**（`https://code.claude.com/docs/en/hooks.md`, Exit code output）:

> Exit 0 means success. Claude Code parses stdout for JSON output fields. **For most events, stdout is written to the debug log but not shown in the transcript.** The exceptions are `UserPromptSubmit`, `UserPromptExpansion`, and `SessionStart`.

PostToolUse は例外に含まれない。現行 5 hook はすべて `| head` / `| tail` / `echo` 終端で **exit 0 + プレーン stdout**（`.claude/settings.json:24,28,32,36,40`）。

**帰結（issue の前提の書き換え）**:

1. `CLAUDE.md:51`「Edit/Write 後に会話へ流れる clippy / typecheck 出力はこのフック由来であり、手動での再実行は不要」は **事実と異なる**。チームの誤った信念の震源
2. issue §4「出力予算（`head -N`）」は、**誰も受け取っていない出力**の予算配分を論じていた。直前にマージされた #472（`--pretty` 除去）も、見えない診断を見えないまま増やした
3. したがって本 issue の「根治」には、**出力をエージェントへ届ける経路の修正**が含まれざるを得ない。これは issue 本文の §1〜§5 のいずれにも書かれていない **6 つ目の課題**

**届ける手段の候補**（いずれも `claude --version` = 2.1.205、**実機未検証**）:

| 案 | 形 | 備考 |
|---|---|---|
| A | exit 0 + stdout に JSON `{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"<診断>"}}` | exit 0 を保ったまま届く。ユーザー可視にするなら `systemMessage` を併用 |
| B | exit 2 + stderr | ドキュメントの exit code 2 表: `PostToolUse` は **Can block? No / Shows stderr to Claude**。ブロックせず届く。ただし "hook error" として描画される |

**副次的に判明した誤り**: `plan.md` の I1 が「exit 2 なら検査失敗がツール失敗として扱われる」としていたのは誤り。PostToolUse で exit 2 は**ブロックしない**。

### 実測 5 の内訳（tsconfig `include` 拡張時に露出する既存エラー 9 件）

| ファイル | 件数 | 内容 |
|---|---|---|
| `e2e/tauri.slash.e2e.ts` | 1 | TS7016: `selenium-webdriver` の型定義が無い（`@types/selenium-webdriver` 未導入） |
| `ui/src/stores/search.test.ts` | 8 | `listen()` モックの型不整合（TS2345 ×6）、`LaunchStatus` に `"error"` が無い（TS2322）、`FolderFrame` に `dir` が無い（TS2769） |

→ §5 を「tsconfig を広げる」で解こうとすると、この 9 件の修正が先行条件になり本 issue のスコープが膨らむ。**hook 側を絞る**決定の根拠。

---

## 関連コード

### 変更の中心

| ファイル | 現状 | 役割 |
|---|---|---|
| `.claude/settings.json` L18–44 | PostToolUse に 5 つの inline hook（clippy / core test / settings test / typecheck / WARN） | 本 issue の対象 |
| `tsconfig.json` | `noEmit: true` / `include: ["ui/src"]` / `exclude: [ui/src/**/*.test.ts(x)]` | typecheck の実体的 SSOT 候補 |
| `package.json` L13 | `"typecheck": "tsc --noEmit"` / L14 `"prebuild": "npm run typecheck"` | npm 側の typecheck 定義 |
| `vitest.config.ts` | `include: ["ui/src/**/*.test.{ts,tsx}"]` | hook スクリプトのテストを走らせる窓口 |

### 現行 5 hook の発火条件と検査内容

| # | 現行の grep（payload **全体**に対して） | 実行される検査 | 出力予算 |
|---|---|---|---|
| 1 | `'\.rs"'` | `cargo clippy -p snotra-core -p snotra -p snotra-settings --all-targets --message-format short -- -D warnings` | `head -20` |
| 2 | `'snotra-core.*\.rs"'` | `cargo test -p snotra-core --lib` | `tail -5` |
| 3 | `'snotra-settings.*\.rs"'` | `cargo test -p snotra-settings` | `tail -8` |
| 4 | `'\.(ts\|tsx)"'` | `cd /c/workspace/Snotra && mkdir -p node_modules/.cache && node node_modules/typescript/bin/tsc --noEmit --incremental --tsBuildInfoFile ...` | `head -30` |
| 5 | `'(tauri\.conf\.json\|config\.toml)'` | `echo 'WARN: ...' >&2` | — |

**出力経路の不変条件**: 5 本すべてパイプ終端が `head` / `tail` / `echo` のため **exit code は常に 0**。検査は失敗してもツール実行をブロックしない。検査結果は stdout、WARN のみ stderr。この意味論は変更してはならない。

### ドキュメント同期対象

| ファイル | 箇所 | 内容 |
|---|---|---|
| `CLAUDE.md` | L43–51「フック」表 | PostToolUse の発火条件記述 |
| `docs/build-commands.md` | L20–21 | 「PostToolUse フックも `.rs` 編集で clippy、`snotra-core` 編集で core テストを自動発火する」 |
| `SPEC.md` | — | **フックの記述なし**（grep で 0 件）→ **更新不要** |

---

## 既存パターン

- `.claude/hooks/` ディレクトリは**存在しない**（新設）。ツール系スクリプトの既存の置き場は `scripts/`（すべて `.ps1` + `run-codex.sh`）だが、hook は Claude Code 規約の `.claude/` 配下が自然
- `docs/build-commands.md` が「コマンドの SSOT」として既に機能しており、`AGENTS.md` / skill がそこを参照する構造がある。今回 tsconfig を typecheck の SSOT に据えるのは**同じ思想の踏襲**
- テストは vitest（`environment: "node"`）。hook スクリプトは Rust でも UI でもないため、純関数を切り出して vitest で検証するのが最小コスト

---

## 技術的制約

1. **`jq` 不在** — JSON 抽出は `node` の `JSON.parse` で行う（実測 1）
2. ~~**hooks は session 起動時にスナップショットされる**~~ — **この記述は誤りの疑いが強い**。`hooks-guide.md:776` は「If you edit settings files directly while Claude Code is running, the file watcher normally picks up hook changes automatically.」と述べる（**ドキュメント根拠のみ・実機未検証**）。
   - 帰結（良）: `settings.json` 差し替え後、**本セッション中の実 Edit 一回**で新 hook を検証できる
   - 帰結（注意）: 差し替えの瞬間から Phase 3/4 の編集に新 hook が効く。したがって plan の「合成 payload スモーク → settings.json 差し替え」という順序は、望ましさではなく**要件**になる
3. **worktree には `node_modules` が無い**（実測 9）— 検査対象のツリーへ `cd` するだけでは `tsc` バイナリが見つからない。**コンパイラの解決元（main ツリー）と検査対象のツリー（worktree）を分離**する必要がある（実測 10・11）
4. **`.git` は worktree ではファイル**（実測 8）— root 検出は「ディレクトリであること」を仮定してはならない
5. **CI と local で tsc のメジャーバージョンが違う**（実測 6）— CI は `npm ci` で **TS 6.0.3**、ローカル hook は **5.9.3**。今回の tsconfig 変更は TS 6.0.3 で成立を確認済み（実測 7）
6. **cargo は worktree で target キャッシュを共有しない** — worktree で clippy を走らせると初回フルビルドになる（未実測だが `target/` が worktree に無いことからの帰結）。`CARGO_TARGET_DIR` 共有は最適化の余地だが本 issue のスコープ外
7. **PostToolUse の exit code 2 は stderr を Claude に差し戻す** — スクリプトが例外で落ちると意図しない差し戻し・ノイズになる。内部エラーは捕捉して exit 0 に正規化する必要がある

---

## 同一パターンの全コードパス検索（AGENTS.md ステップ 2）

「payload 全体を grep する」というバグパターンをコードベース全体で検索した結果、**PreToolUse にも同根の 2 件**が存在する。

| hook | 同根の問題 | 具体的な誤爆 |
|---|---|---|
| `block-main-commit` | Bash payload 全体を `grep -qE 'git\s+(commit\|merge\|rebase)'` | `tool_input.description` に「git commit を実行」と書くだけで発火しうる（`command` を見ていない） |
| `block-main-commit` | `git branch --show-current` を **hook の cwd** で評価 | worktree エージェントでは worktree のブランチを見る。§2「絶対パス cd」と同じ**ツリーのずれ** |
| PR 作成前 push チェック | Bash payload 全体を `grep -qE 'gh\s+pr\s+create'` | 同上（`description` に文字列が出れば発火） |

**判断**: 本 issue は PostToolUse を対象と明記しているため、PreToolUse は**別 issue に切る**（スコープ集中）。ただし根が同一である事実は plan.md に記録し、報告でユーザーに提示する。

---

## 未解決の疑問

1. **worktree エージェントの hook 実行時に `CLAUDE_PROJECT_DIR` が何を指すか** — 実測していない。
   → **設計で回避する**: root を `file_path` から最近接 `.git` を遡って導出すれば、この値の意味論に依存しない。issue §2 のチェックボックスは「実測せず、答えを不要にする形で閉じた」と明示して close する
2. ~~PostToolUse hook の stdout が会話に届く条件~~ → **実測 15 で解決（届かない）**。残る疑問は「案 A（`hookSpecificOutput.additionalContext`）と案 B（exit 2 + stderr）のどちらが実機で機能するか」。**Phase 2 の必須手順として実 Edit 一回で確認する**
3. **worktree での cargo clippy の実コスト** — 初回フルビルドの秒数は未計測。correctness を優先し、計測は follow-up
4. **`cargo` の進捗行（`Compiling ...`）が cold worktree で `head -20` を埋め尽くすか** — 論理的帰結として確実だが秒数・行数は未計測。予算適用前に進捗行を除去する対処が要る
5. **hooks の file watcher による再読込**（技術的制約 2）— ドキュメント根拠のみ。実機未検証
