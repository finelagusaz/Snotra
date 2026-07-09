# research — issue #482: PreToolUse の PR 前 push チェックを `pre-bash.mjs` へ移す（Phase 2）

## 1. issue の要約

`.claude/settings.json` の PreToolUse に残る唯一の hook「PR 前 push チェック」に、3 つの欠陥がある。

| # | 欠陥 | 失敗様式 |
|---|---|---|
| D1 | `matcher: "Bash"` が PowerShell tool に一致しない | **fail-open**（`gh pr create` が無検査で通る） |
| D2 | `grep` が `tool_input` **payload 全体**に当たる（`description`・引数文字列にも反応） | fail-closed（誤爆・摩擦） |
| D3 | `@{u}` を**コマンド実行の前**に評価するため `git push -u origin HEAD && gh pr create` が必ずブロックされる | fail-closed（CLAUDE.md 最重要ルール 2 の実在根拠） |

期待する解（issue 本文 = 設計文書 §10 Phase 2）:

1. `JSON.parse(stdin).tool_input.command` **だけ**を判定に使う（`description` を見ない）
2. `matcher` を `Bash|PowerShell` に広げる
3. 判定不能なら **fail-closed** に倒す

完了後、CLAUDE.md 最重要ルール 2 を削除できる。

この領域が hook 固有である理由（設計文書 §2 の (A2)）: `gh pr create` はリポジトリを触らないため git hook は鳴らず、push もしないので `pre-push` も鳴らない。GitHub ruleset も空 PR を防げない。**外部 API への不可逆呼び出しは Claude Code hook にしか見えない。**

## 2. 本セッションで取得した一次証拠（故障注入・実測）

`AGENTS.md`「安全網が『効いている』ことは、故障注入で一度は実測する」に従い、D1 / D2 を実測した。

### E1. D2 の実演 — 起動していない `gh` で誤爆した（Bash tool）

調査中、以下の **`gh` を一切起動しないコマンド**が hook にブロックされた。

```
grep -n "gh pr create\|チェーン" CLAUDE.md
```

```
PreToolUse:Bash hook error: BLOCKED: 未 push のコミット、または upstream 未設定です。
```

`gh pr create` は grep の**検索パターン文字列**であり、コマンド位置に現れていない。加えて本ブランチは作成直後で upstream 未設定のため、条件が両方成立してブロックに至った。**D2（payload 全体 grep）と D3（実行前の `@{u}` 評価）が同時に観測された。**

### E2. D1 の実演 — PowerShell tool は素通りする

同じ文字列 `gh pr create` をコマンド本文に含む PowerShell tool 呼び出しは、**ブロックされずに実行された**（出力が返った）。

```powershell
Write-Output "PROBE: ... contains the literal string 'gh pr create' ..."
```

→ `matcher: "Bash"` は PowerShell tool に一致しない。**この環境の primary shell は PowerShell** であり、`gh pr create` を PowerShell tool から叩けば安全網は存在しない。

### E3. 現在のブランチ状態

`fix/pre-bash-hook` は upstream 未設定（`fatal: no upstream configured`）。**hook の実測プローブは upstream 未設定のうちに行う必要がある**（push 後は「安全」と判定され block 経路が観測できない）。

## 3. Claude Code hook の契約（公式ドキュメント確認済み）

出典: `code.claude.com/docs/en/hooks` — サブエージェントに URL と該当箇所を引用させて確認した。

| 項目 | 確定内容 | 確度 |
|---|---|---|
| **exit code** | `0` = stdout を JSON として解析 / `2` = **ブロッキングエラー（PreToolUse はツール呼び出しをブロック）** / **それ以外の非ゼロ（1 含む）= 非ブロッキングエラー。stderr を表示して<br>ツールは実行される** | 確定 |
| **JSON 出力** | exit 0 + stdout に `hookSpecificOutput: { hookEventName: "PreToolUse", permissionDecision: "allow"\|"deny"\|"ask", permissionDecisionReason }`。`deny` の reason は Claude に届く | 確定 |
| **matcher** | `"Bash"` は**完全一致文字列**。`"Edit\|Write"` は `\|` 区切りの**リスト**。`"^Notebook"` のような正規表現も可 | 確定（アンカー `$` の可否のみ出典なし） |
| **stdin payload** | `session_id` / `transcript_path` / **`cwd`** / `permission_mode` / `hook_event_name` / **`tool_name`** / **`tool_input`** | 確定。`cwd` はセッションの作業ディレクトリ |
| **スクリプト起動失敗**（exit 127 等） | ドキュメントに記載なし。非ブロッキング＝ツール実行と推測される | **不明** |
| **PowerShell tool の `tool_name`** | ドキュメントに記載なし | **不明 → 実装時に実測が必要** |

### 3.1 この契約から導かれる、設計上もっとも重要な帰結

> **Node の未捕捉例外は exit 1 を返す。exit 1 は「非ブロッキングエラー」であり、コマンドはそのまま実行される。**

つまり `pre-bash.mjs` が落ちれば **fail-open** になる。さらに Node は未捕捉例外時に `process.exitCode` の設定値を無視して 1 で終了する。ゆえに:

- **すべての経路を `try/catch` で囲み、内部エラーでも明示的に exit 2 を返す**ことが「fail-closed に倒す」の実体である
- `process.exit(2)` は使わない（stdout/stderr がパイプのとき未 flush 出力を切り捨てる — `post-edit.mjs` I1 の教訓）。`process.exitCode = 2` を使う
- 出力方式は **exit 2 + stderr** を採る。JSON deny 方式は exit 0 を要求するため、「既定を block に倒し、安全と確認できたときだけ 0 にする」構造が取れない

### 3.2 matcher と `tool_name` の二重防御

`matcher` が完全一致（または `|` 区切りリスト）であれば `"Bash"` は `BashOutput` / `KillShell` に一致しない。しかし確度 95% の記述であり、かつ `BashOutput` の `tool_input` には `command` が無い。**「`command` が無ければ fail-closed でブロック」を素直に書くと、`BashOutput` を巻き込んでブロックしうる。** ゆえにスクリプト側でも `tool_name` を `Bash` / `PowerShell` に限定し、対象外ツールは「管轄外」として exit 0 で通す（これは「判定不能」ではない）。

## 4. 関連コード

| ファイル | 役割 | 本 issue での扱い |
|---|---|---|
| `.claude/settings.json` | PreToolUse（`matcher: "Bash"` のインライン sh）/ PostToolUse | **触る** — PreToolUse を `pre-bash.mjs` 呼び出しへ差し替え、matcher を拡張 |
| `.claude/hooks/post-edit.mjs` | PostToolUse ディスパッチャ。`selectChecks` が `.claude/hooks/**` と `.claude/settings.json` で `hook-selftest` を発火 | **触らない**（Phase 3）。ただし L243 のコメントが「PR 前 push チェック」に言及しており、記述は引き続き真 |
| `.claude/hooks/post-edit.test.mjs` | vitest。純関数テスト + `spawnSync` による e2e（stdin → exit code / stdout） | **手本にする**（新規テストの構造） |
| `.claude/hooks/pre-bash.mjs` | — | **新規** |
| `.claude/hooks/pre-bash.test.mjs` | — | **新規** |
| `vitest.config.ts` | `include: ["ui/src/**/*.test.{ts,tsx}", ".claude/hooks/**/*.test.mjs", ".githooks/**/*.test.mjs"]` | **触らない** — 新規テストは自動で `npm test` に入る |
| `.githooks/**` | Layer 1（main 保護） | **触らない**（別レイヤ） |

### 4.1 既存パターン（再利用できるもの）

`post-edit.mjs` が確立した規約をそのまま踏襲できる:

- **純関数を export し、`main()` は薄く保つ** → ユニットテストが配線ミスまで捉える（`resolveTarget` の例）
- **`invokedDirectly` ガード**（`import.meta.url === pathToFileURL(process.argv[1]).href`）— import しただけで stdin 読み取りが走らない（I13）
- **`process.exit()` を使わず `process.exitCode`**（I1）
- **`spawnSync` に `timeout` を渡し、自前で打ち切って報告する**（hook 全体の timeout に丸投げしない）
- **`readFileSync(0, "utf8")` で stdin を同期読み取り**
- テストは `spawnSync(process.execPath, [SCRIPT], { input: JSON.stringify(payload) })` で e2e 検証

### 4.2 テスト・検証の配線（既存で足りる）

- `vitest.config.ts` の `include` に `.claude/hooks/**/*.test.mjs` が既にある → `npm test` が新規テストを拾う
- `post-edit.mjs` の `selectChecks` は `rel.startsWith(".claude/hooks/")` と `rel === ".claude/settings.json"` で `hook-selftest`（= `vitest run .claude/hooks`）を発火 → **`pre-bash.mjs` を編集した瞬間に自テストが走る**。新規に配線する必要はない
- `docs/build-commands.md` の検証カテゴリ E は `.githooks/**` 専用。`.claude/hooks/**` を対象とするカテゴリは**存在しない**（hook-selftest が自動発火するため実害はないが、`npm test` を明示するカテゴリの追加は検討余地あり）

## 5. 判定ロジックの設計論点

### 5.1 hook が本当に問うべき問い

> **`gh pr create` が走る瞬間、コミットは remote に存在するか？**

これが真になる経路は 2 つある。

1. **静的**: コマンド鎖の中で `gh pr create` **より前**に `git push` が走り、両者が `&&` で結ばれている（`&&` は前段成功を保証する）
2. **動的**: hook 実行時点で upstream が設定済みかつ `git log @{u}..HEAD` が空

現行 hook は 2 しか見ていない。**1 を見ないことが D3 の正体**であり、CLAUDE.md 最重要ルール 2 が存在する理由そのものである。1 を実装すれば `git push -u origin HEAD && gh pr create` が通り、ルール 2 を削除できる。

### 5.2 「コマンド位置」で検出する — D2 の根治

payload 全体でも、`command` 全体の素朴な `grep` でもなく、**コマンド位置に現れる `gh pr create`** を検出する。コマンド位置 = 文字列先頭、または区切り文字（`;` `&` `|` 改行 `(` `)`）の直後。

- `grep -n "gh pr create" CLAUDE.md` → `gh` の直前は `"` → **検出しない**（E1 の誤爆が消える）
- `git push -u origin HEAD && gh pr create` → `&` の直後 → **検出する**
- `echo "&& gh pr create"` → `&` の直後と読める → **検出する（誤爆）**。ただし fail-closed 方向であり、実害は摩擦のみ

引用の内側まで正確に判定するには shell パーサが要る。**過剰検出は摩擦・過小検出は fail-open** であり、後者のみが issue の実害であるため、境界は過剰側へ倒す。

### 5.3 fail-closed の定義（何を「判定不能」と呼ぶか）

| 状況 | 判断 | 理由 |
|---|---|---|
| stdin が JSON として壊れている | **block** | 何が走るのか見えない |
| `tool_name` が `Bash` / `PowerShell` 以外 | **allow** | 管轄外（判定不能ではない） |
| 対象ツールなのに `tool_input.command` が文字列でない | **block** | 見えない |
| `gh pr create` を検出せず | **allow** | 管轄外 |
| 検出あり + `git push` が `&&` で先行 | **allow** | 5.1 の経路 1 |
| 検出あり + `git` の状態取得に失敗（非 repo / git 不在 / timeout） | **block** | 見えない |
| 検出あり + upstream 未設定 or 未 push コミットあり | **block** | 本来の目的 |
| スクリプト内部の例外 | **block**（exit 2） | §3.1 |

### 5.4 残る既知の穴（受容する性質として記録する）

- **hook スクリプト自体が起動できない場合**（`node` 不在・パス誤り）は非ブロッキングと推測され、fail-open になる。スクリプト内部からは塞げない
- **`sh -c 'gh pr create'` / `eval` / バッククォート**はコマンド位置検出を回避する。事故モードではなく意図的迂回であり、`--no-verify` と同格に扱う（人間専用）
- **`git push origin other-branch && gh pr create`** は許可される（HEAD を push していない）。refspec の意味論まで解釈しない
- **`cd other-repo && gh pr create`** は hook の `cwd` と実際の repo がずれる（#473 のバグ 2 と同型）。`cd` / `Set-Location` / `pushd` を鎖に検出したら block するか否かは **plan で判断する**
- hook の git 状態評価は `payload.cwd`（セッションの作業ディレクトリ）で行う。Bash tool の永続 cwd がそこからずれている場合は追随しない

## 6. 技術的制約

- **Win32 / IPC は無関係**。本変更はエージェント運用層のみで、`snotra-core` / `src-tauri` / `ui` のいずれにも触れない
- **SPEC.md は製品仕様の意図管理**であり、hook はプロダクト挙動ではない → **SPEC.md 更新は不要**（AGENTS.md ステップ 0 の「仕様変更」に当たらない）
- **`.claude/settings.json` は file watcher が即座に拾う**（セッション再起動不要・CLAUDE.md 実測）。壊れた JSON を書いた瞬間に全 hook が停止する。ゆえに settings.json の編集は `hook-selftest` を発火させ、JSON 妥当性を即検証する経路が既にある
- **`jq` はこの環境に存在しない**（#473 実測）。`node` を使う
- **CI は ubuntu-latest**。テストで Windows リテラルパスを書かない（`post-edit.test.mjs` 冒頭の注意）
- **エージェント設定の変更は合意してから**（CLAUDE.md 最重要ルール 4）。本作業は issue #482 が承認済みの変更要求である

## 7. 影響を受けるドキュメント（as-built へ同期する）

| ファイル | 箇所 | 変更 |
|---|---|---|
| `CLAUDE.md` | L13「適用される**4つ**」 | → 3つ |
| `CLAUDE.md` | L16 最重要ルール 2（チェーン禁止） | **削除**し 3・4 を繰り上げ |
| `CLAUDE.md` | L41 Git/GitHub 運用の同ルール | **削除**（または「`&&` チェーンは通る」へ差し替え） |
| `CLAUDE.md` | L49 フック節の前文（PreToolUse の SSOT は `settings.json`） | PreToolUse の判定は **`pre-bash.mjs` が SSOT** へ差し替え |
| `CLAUDE.md` | L53 フック表の PR 前 push チェック行 | 発火条件を as-built へ（コマンド位置検出 / `Bash`・`PowerShell` 両対応 / `&&` 先行 push は通る） |
| `docs/build-commands.md` | 検証カテゴリ | `.claude/hooks/**` を対象とするカテゴリの要否を plan で判断 |
| `docs/superpowers/specs/2026-07-09-...-design.md` | §10 | 日付入り設計記録。**触らない** |
| `docs/superpowers/plans/2026-07-09-...md`, `.superpowers/sdd/task-*.md` | — | 過去の作業記録。**触らない** |

`SPEC.md` / `AGENTS.md` / サブディレクトリの `CLAUDE.md` / `e2e/` は無影響。

## 8. 未解決の疑問（実装時に実測で潰す）

1. **PowerShell tool の `tool_name` は正確に何か。** ドキュメントに記載なし。`matcher: "Bash|PowerShell"` が発火しなければ D1 は直っていない。→ **block メッセージに観測した `tool_name` を含め、upstream 未設定のうちに PowerShell tool から `gh pr create --help` を叩いて実測する**（block されれば hook が発火した証拠。仮に通っても `--help` は PR を作らない）
2. **`matcher` に `$` アンカーが効くか。** 出典なし。効かなくてもスクリプト側の `tool_name` 判定が二重防御になるため、リスクは吸収済み
3. **`cd` を含む鎖を block すべきか。** fail-closed の原則に沿うが、YAGNI と摩擦のトレードオフ。`/plan-review` で問う
