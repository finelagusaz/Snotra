# research — #768 シェル環境・git 操作の規範 5 件を pre-bash.mjs へ機構化する

## issue の要約

常時ロード規範（`CLAUDE.md`）にある「エージェントが打ちうるコマンドの形」で判定できる規範 5 件を
`.claude/hooks/pre-bash.mjs` の判定へ吸収し、常時ロード面から降ろす（#593 の階梯「規範を機構へ吸収する」）。
判定点は既存のコマンド位置検出と同じで、新しい発火点は作らない。Windows 専用 3 件は `process.platform`
ゲートの内側に入るため、Windows 機でのライブ実測が 1 度必要（本セッションは win32）。

| # | 規範 | 現在の所在 | platform |
|---|---|---|---|
| 1 | bash の HEREDOC（`<<EOF`）を使わない | `CLAUDE.md:16` | win32 |
| 2 | 文字列中のパスに `\` 区切りを使わない | `CLAUDE.md:17` | win32 |
| 3 | Python で非 ASCII を出すなら `PYTHONIOENCODING=utf-8` | `CLAUDE.md:18` | win32 |
| 4 | `--no-verify` は人間専用 | `CLAUDE.md:24` | 非依存 |
| 5 | main の同期は `git pull --ff-only` を使う | `CLAUDE.md:25` | 非依存 |

## 関連コード（すべて実在を確認済み）

| ファイル | 役割・触る箇所 |
|---|---|
| `.claude/hooks/pre-bash.mjs` | 判定の SSOT。`decide(payload, readGitState, readPlanState)`:111、`AT_CMD_POS`:44、`ENV_PREFIX`:50、`tokenStart`:72、`hasSafeChain`:83、`REMEDY`:69、`main`:211、fail-closed 骨格:234-245 |
| `.claude/hooks/pre-bash.test.mjs` | 既存 483 行。`decide` の呼び出しは **23 箇所**（テスト 22 + `main()` 1・すべて 3 引数。`grep -c "decide("` で実測）。`runHook`:348 が process 級。カナリア 2 群（fail-closed 骨格:437・settings.json ドリフト:461） |
| `.claude/hooks/post-edit.mjs` | **plan-review の独立導出が発見した衝突先**。`buildCommand`:251-259 の `vitestSpec` が `resolveBin(root, path.join("node_modules","vitest","vitest.mjs"))` で `\` 区切りの絶対パスを作り、`nodeSpec`:252 の `repro` と `:344` の `(cwd: ${root})` が会話へ出す。**この文字列は新しい `\` 判定に当たる**（実測 3/3 block）——片方のフックが指示するコマンドを、もう片方が拒む |
| `scripts/governance-check.mjs` | `AREA_BUDGET`:590（`{ alwaysLoaded: 14058, rules: 8056 }`）、由来コメント:544-589、`checkNormativeAreaBudget`:640 |
| `CLAUDE.md` | 12-18 行がシェル環境節（見出し + 表 3 行）、24・25 行が git 2 件。「フック」表 46-49 |
| `docs/hooks.md` | 「PreToolUse（pre-bash.mjs）の実装契約」9-14。fail-closed 契約・コマンド位置・受容する未対応リスクの正本 |
| `.claude/settings.json` | PreToolUse の matcher（`Bash|PowerShell`）。**変更不要**——新しい発火点を作らないため |

## 既存パターン（再利用できるもの）

- **コマンド位置検出**: `AT_CMD_POS`（`(?:^|[;&|\n\r(){}])\s*`）+ `ENV_PREFIX`（`(?:[A-Za-z_]\w*=\S*\s+)*`）。
  5 件の判定はすべてこの 2 定数の上に載る。`tokenStart` が区切り文字を消費しない位置を返す。
- **セグメント終端の計算**: `hasSafeChain`:91-92 が `command.slice(at).search(/[;&|\n\r]/)` でインラインに計算している。
  git セグメント列挙（#4・#5 で必要）が同じ計算を行うため、**共有ヘルパへ寄せる対象**（`/dry-check` トリガー該当）。
- **注入によるテスト**: `readGitState` / `readPlanState` は「失敗しうる I/O」ゆえ `{ ok: false }` を返す形。
  `process.platform` は失敗しない純粋な値なので**同じ形にしてはならない**（`ok` が常に true な reader は嘘の構造）。
- **カナリア**: ソース正規表現で骨格を縛る型（`fail-closed の骨格カナリア`:437）が既に repo の作法。

## 技術的制約

- **fail-closed 骨格**: `exit 2` だけがブロックする。既定 `process.exitCode = 2`、許可が確定した経路だけが 0 を書く。
  `exit 1`（未捕捉例外）は非ブロッキング = fail-open。
- **爆発半径が変わる**: 従来、重い判定は `ghAt >= 0` の後ろにしか無かった。新 5 述語は**全 Bash/PowerShell コマンド**で走る。
  述語が throw すれば `main()` の catch → `emitBlock` → exit 2 で**セッションの全コマンドがブロックされる**。
  ゆえに 5 述語は全域関数（never throw）でなければならない。復旧は Edit ツール（PreToolUse の matcher は `Bash|PowerShell` のみ）。
- **CI の実行 OS**（実測・`.github/workflows/ci.yml`）: `npm test` は **ubuntu（:39）と windows（:116 rust-check job）の両方**で走る。
  ゆえに platform 条件付きの process 級テストは両分岐のライブカナリアになり、ソースカナリアは不要。
- **PostToolUse**: `.claude/hooks/**` の編集は `hook-selftest`（`vitest run .claude/hooks`）を自動発火。沈黙 = 合格。
  `CLAUDE.md` / `scripts/*.mjs` / `docs/*.md` / ADR には**検査が割り当てられていない**——沈黙は「何も走らなかった」。
- **G11 見出し参照**: `CLAUDE.md`「シェル環境（Windows / PowerShell）」を正準形で指す文書は
  `docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:20` のみで、**`headingRefDocs` は `docs/superpowers/` を除外する**
  （`governance-check.mjs`:801-805）。ゆえに節の削除で G11 は落ちない。
- **面積 ratchet**: 現在 常時ロード **14036/14058**（余裕 22 字）・rules 7956/8056（実測）。
  `CLAUDE.md` へ 1 字足せば即赤になる位置にある。

## 判定ロジックの実測（代表入力 49 件・自分で測った一次証拠）

`AGENTS.md`「検証の作法」に従い、plan へ書く前に scratchpad の probe で全件実行した。**49/49 期待一致・
病的入力 130 呼び出しで例外なし**（空文字列・`<<` 単独・不一致引用・20 万字・`$env:TEMP\` × 5000 等）。

```
ALL OK (49 cases)
segments: ["git switch main ","git pull --ff-only"]
NO THROW (130 calls)
```

確定した述語（`\` は JS リテラルの字面）:

| # | 述語 | 形 |
|---|---|---|
| 1 | `usesHeredoc` | `(?<![<>])<<(?!<)-?[ \t]*(['"]?)([A-Za-z_]\w*)\1` の**全候補**を `matchAll` で回し、(a) 終端行が候補より後ろに単独行として在る、または (b) 直後が `[<>|&;)]` / 改行 / 文末 |
| 2 | `usesBackslashPath` | `[A-Za-z]:\\` / `\$env:\w+\\` / `%\w+%\\` の 3 形 |
| 3 | `needsPyEncoding` | コマンド位置の `python3?`/`py` **かつ** 非 ASCII を含む **かつ** `PYTHONIOENCODING=`/`PYTHONUTF8=`/`-X utf8` がいずれも無い |
| 4 | `usesNoVerify` | コマンド位置の `git` セグメントに `--no-verify` トークン、または commit セグメントに短縮 `-n`（`-nm` 等） |
| 5 | `pullWithoutFfOnly` | コマンド位置の `git` セグメントの subcommand が `pull` で `--ff-only` を持たない |

### plan-review で覆った 3 つの誤り（いずれも自分で再実測して確定）

1. **`usesHeredoc` を「最初の候補だけ」で判定すると fail-open する。** `grep -rn "x << y" src && cat <<EOF\nbody\nEOF`
   は囮の `<< y` で `exec` が止まり、本物の heredoc を取り落とす（実測 false）。全候補を回す必要がある。
   ただし候補ごとに終端行を全文走査すると O(候補数 × 長さ) で、候補 2 万件・21 万字に **1812 ms** かかった。
   **単独行の語を 1 パスで `Map` に集めて候補ごとに O(1) 参照**すれば同じ入力が **4.5 ms** で、14 ケース全一致。
   計算量は「never throw」ではなく **never hang** の問題であり、不変条件として別に立てる必要がある。
2. **subcommand 判定に `(?:-\S+\s+)*` を使うと `git -C /x commit -n` / `git -C /x pull` を取りこぼす**（実測 false）。
   スペース区切りのフラグ値を読み飛ばせないため。**既存の `FLAG` 定数**（`pre-bash.mjs`:47）はこのために作られており、
   差し替えると両方 true になり、`git -C /x log -n 5` は false のまま（誤検出は増えない）。
3. **`decide` の呼び出しは 23 箇所**（テスト 22 + `main()` 1）。当初「30 箇所」と書いたのは誤りだった。

### 計算量の実測（述語は全コマンドで走るため税になる）

`ENV_PREFIX`（`(?:[A-Za-z_]\w*=\S*\s+)*`）はネストした量指定子を持ち `g` + `matchAll` で全位置から試行されるが、
敵対的入力でも線形だった: 最悪 **3.01 ms**（区切り 5000 個 = 5000 マッチ）、20 万字で 0.06〜0.82 ms、
`A=1 ` 前置 2000 回で git に到達しない最悪形で 0.40 ms。実コマンド 1 回あたり **0.0005 ms**。ReDoS は再現しない。

### ADR の番号

`docs/adr/` は **0001〜0008 が使用済み**（`0006-plan-ownership-boundary.md` は #749 サイクルの成果物）。
新規 ADR は **0009** を使う。番号の一意性を検査する機構は `governance-check.mjs` に無く、手動確認が唯一の防御線。

**引用の閉じ（`"` / `'`）が誤爆を防いでいる**のが 1 の鍵である——`grep -rn "x << y" src | head` は
`y` の直後が `"` で追従集合に無いため一致しない。`<<<`（herestring）は lookahead で除く。

**過小検出（fail-open）を意図して残す形**（実測で確認）:
- `git commit -m "a;b" --no-verify` — セグメントが引用内の `;` で切れる。引用認識パーサは
  「shell パーサ相当まで広げるな」（`docs/hooks.md`:13）に触れるため採らない。`sh -c` と同格の意図的迂回。
- `python foo.py`（非 ASCII をスクリプト側が出す） — コマンド文字列に非 ASCII が現れないため見えない。

**過剰検出（fail-closed 方向・意図として固定する）**:
- `git commit -m "fix: C:\path handling"` — 散文中の `C:\` も #2 に当たる。回復は自明（`/` にする）。
- `git pull --rebase` / feature ブランチでの `git pull` も #5 で止まる（ブランチを見ない）。

## 未解決の疑問

なし。設計判断（注入面の形・`platform === undefined` の倒し方）は plan.md で決める。
