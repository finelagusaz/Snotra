# plan — #768 シェル環境・git 操作の規範 5 件を pre-bash.mjs へ機構化する

前提は `workspace/research.md`（述語は代表入力で実測済み・一次証拠）。`/plan-review` の指摘を反映済み（→ 末尾「セルフレビュー」）。

## 設計判断（先に確定させる）

| # | 判断 | 採る形 | 却下した形と理由 |
|---|---|---|---|
| D1 | `decide` の注入面 | 第 4 位置引数 `platform`（素の文字列） | `readPlatform()` reader 形 — `process.platform` は失敗しないので `{ ok: false }` を持つ形は嘘の構造になる |
| D2 | 引数の増やし方 | 位置引数を 1 本足す | options オブジェクト（`decide(payload, deps)`）への一括改修 — 既存 23 呼び出し点を機械的に書き換える差分に見合う利益が今は無い。3 本目を足すときに再検討する（ADR-0009 に残す） |
| D3 | `platform === undefined` | Windows 専用判定は**発火しない**（非 Windows と同じ） | block へ倒す — 「判定不能」ではなく「規範の射程外」であり、macOS で false block を作る。到達経路は「呼び出し側の渡し忘れ」1 本のみで、**両 OS の CI の process 級 e2e とソースカナリアの二重**で固定する（独立導出はここを Windows へ倒すべきと結論した。判断は分かれたが、**配線の固定という要求は共通**なのでカナリアを両方置いて解消する） |
| D4 | セグメント終端の計算 | `segmentEnd(command, at)` を共有ヘルパへ抽出し `hasSafeChain` も載せ替える | インラインの二重実装を残す — `/dry-check` トリガー該当。挙動は不変（既存テストが固定） |
| D5 | `git commit -n` | 検出する（commit セグメント限定） | 見送る — `-n` は commit では `--no-verify` そのもの。**issue の文言（`--no-verify`）より広い**ため独立の判断として記録する。`git push -n`（= `--dry-run`）は commit セグメント限定ゆえ無傷 |
| D6 | 引用認識のセグメント分割 | 採らない | `docs/hooks.md`:13「shell パーサ相当まで広げると payload 全体 grep の誤爆を作り直す」に触れる。残る fail-open は `sh -c` と同格の意図的迂回として `docs/hooks.md` の「受容する未対応リスク」へ列挙する |
| D7 | `.\` / `..\` の検出、ブランチを見る `pull` ゲート | 採らない | `.\scripts\x.ps1` は PowerShell で正しく動く形であり止める理由が弱い。ブランチ判定は `readGitState` の責務拡大を招く。どちらも `docs/hooks.md` の残余へ記す |
| D8 | `usesHeredoc` の走査 | **全候補**を回し、**単独行の語を 1 パスで `Map` 化**して候補ごとに O(1) 参照 | (a) 最初の候補だけ — 囮の `<<` が先行すると fail-open（実測）。(b) 候補ごとに終端行を全文走査 — 候補 2 万件で 1812 ms（実測）。採る形は同入力 4.5 ms で 14 ケース全一致 |
| D9 | subcommand の判定 | 既存の `FLAG` 定数（`pre-bash.mjs`:47）を使う | `(?:-\S+\s+)*` — スペース区切りのフラグ値を読み飛ばせず `git -C /x commit -n` / `git -C /x pull` を取りこぼす（実測 fail-open） |
| D10 | `post-edit.mjs` の再現コマンドが `\` 判定に当たる | **`repro` 文字列だけ**を POSIX 区切りへ正規化する（実行は `spec.cmd`/`spec.args` のままなので無影響） | `\` 判定に `node_modules` の免除を彫る — 免除句は名指しした対象しか守らず、逃げ道が列挙の隣へ移る（`/norm-review`「Step 3」）。**根本は「エージェントへ渡す文字列が規範を破っている」ことである** |
| D11 | `docs/development-principles.md` §6「判定単位は構文的位置で定義する」との緊張 | §6 は変更せず、`docs/hooks.md` に**意図的な逸脱**として理由付きで記す | §6 を書き換える — `\` パスと非 ASCII は「言及と実行を分ける構文的位置が存在しない」種類の失敗（シェルが `\` を食う）であり、§6 の一般則は正しいまま。narrow な逸脱を広い原則文書へ書き戻すと原則が弱まる |

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `.claude/hooks/pre-bash.mjs` | 述語 5 件 + `segmentEnd` / `gitSegments` を追加、拒否文言 5 件、`decide` に第 4 引数 `platform`、`main()` が `process.platform` を渡す |
| `.claude/hooks/pre-bash.test.mjs` | 代表入力・platform 注入（win32 / darwin / undefined）・process 級 e2e・病的入力の no-throw / no-hang・ソースカナリア追加 |
| `.claude/hooks/post-edit.mjs` | `buildCommand` / `runCheck` が出す `repro` 文字列を POSIX 区切りへ正規化（D10・**独立導出が発見した漏れ**） |
| `.claude/hooks/post-edit.test.mjs` | 再現文字列が `\` を含まないことのカナリア（`pre-bash.mjs` の `decide` が block しないことを直に照合する） |
| `CLAUDE.md` | 「シェル環境（Windows / PowerShell）」節を削除（見出し + 表 3 行）、「Git/GitHub 運用」から 2 bullet を削除、「フック」表の PreToolUse 行をラベルごと一般化 |
| `docs/hooks.md` | 「PreToolUse（pre-bash.mjs）の実装契約」へ 5 判定・platform 注入・爆発半径を追記。**:12 の全称表現（「コマンド位置だけを見る」）を是正**し、受容する未対応リスクへ D6・D7・過小/過剰検出を列挙 |
| `docs/adr/0009-*.md` | 新規。注入面の形（D1・D2・D3）の否定の知識。**0006 は使用済み**（`0006-plan-ownership-boundary.md`） |
| `scripts/governance-check.mjs` | `AREA_BUDGET.alwaysLoaded` を実測 + 100 字へ引き下げ、由来コメントへ #768 入りの項目を追加 |

**SPEC.md の更新は不要** — 変更対象は開発ハーネスであり、アプリの挙動・状態遷移・IPC 契約・設定キー・データ形式を一切変えない。

## 不変条件

- **I1 fail-closed 骨格を壊さない** — 既定 `process.exitCode = 2` は `try` より前、許可が確定した経路だけが 0 を書く、`process.exit()` を使わない。既存カナリア群（`pre-bash.test.mjs`:437）がそのまま緑であること。
- **I2 5 述語は全域関数である（never throw かつ never hang）** — 従来は重い判定が `ghAt >= 0` の後ろにしか無かったが、5 述語は**全 Bash/PowerShell コマンド**で走る。throw すれば `main()` の catch → exit 2 で**セッションの全コマンドがブロックされ**、hang すれば hook の timeout まで全コマンドが待つ。`usesHeredoc` は候補数と長さに対して**線形**でなければならない（D8）。`new RegExp` 補間が安全なのは捕獲群が `[A-Za-z_]\w*` でメタ文字を含みえないためである（この理由をコード注釈に書く）。
- **I3 判定の起点はコマンド位置である** — `description` も payload 全体も読まない（#482）。`--no-verify` / `pull` は「コマンド位置の `git` から次の区切りまで」のセグメントに閉じて判定し、`grep -n "--no-verify" CLAUDE.md` を止めない。**ただし heredoc 演算子・`\` パス・非 ASCII はコマンド全体を見る**——この 3 件には「言及と実行を分ける構文的位置」が無いため（D11）。引用内の言及でも発火する過剰検出を意図として受け入れ、テストで固定する。
- **I4 新しい発火点を作らない** — `.claude/settings.json` の matcher は無変更。settings.json ドリフト検出カナリア（同:461）が緑のままであること。
- **I5 `platform === undefined` は Windows 判定を発火させない**（D3）。到達経路は「`decide` の呼び出し側が第 4 引数を渡さない」1 本だけであり、実行経路は `main()` のみ、テストは明示的に渡す。配線切れは (a) Windows CI の process 級 e2e と (b) `main()` が `process.platform` を渡すことのソースカナリアの**両方**で赤になる。
- **I6 拒否は必ず復帰手順を含む** — 5 件それぞれに「何が起きるか」+「代わりに何をするか」を含む文言。規範を降ろす設計は「拒否メッセージがその場で教える」ことに賭けているので、これが破れると規範が消えるだけになる。
- **I7 既存 23 個の `decide` 呼び出し（3 引数）の意味を変えない** — すべて `gh pr create` ケースで Windows 専用判定に触れる文字列を含まない。書き換えずに緑であること。
- **I8 フック間で矛盾する指示を出さない** — `post-edit.mjs` が会話へ出す再現コマンドは、`pre-bash.mjs` の判定が通す形でなければならない（D10）。片方が指示し片方が拒む状態は、規範を機構へ移す設計の信頼を直に壊す。

### 破壊不変条件と検知手段

| 壊れたら即アウト | 検知手段 |
|---|---|
| **全 Bash がブロック/停止する**（述語の throw・hang・構文エラー） | `.claude/hooks/**` 編集で自動発火する `hook-selftest`（沈黙 = 合格）。加えて process 級「無害なコマンドは exit 0 で無出力」・病的入力 no-throw・**候補 2 万件の入力に時間上限を課すテスト**。復旧は Edit ツール（PreToolUse の matcher は `Bash`/`PowerShell` のみでファイル編集は通る） |
| **ガードが沈黙で外れる**（platform 配線切れ・正規表現の後退） | 各述語の赤方向テスト（両向き）＋ platform 注入テスト（win32 / darwin / undefined）＋ **両 OS の CI で走る** process 級 e2e（`npm test` は ubuntu:39 と windows:116 の双方で実行される・実測）＋ ソースカナリア |
| **`.githooks/` の迂回が通る** | Windows 機でのライブ実測（Phase 3）。**滑っても無害な形で打つ**——`--dry-run` を併記し、`git pull` は存在しない remote 名を与える |
| **フック同士が矛盾する**（I8） | `post-edit.test.mjs` のカナリアが `decide` を直に呼び、再現文字列が allow されることを照合する |

## 実装順序

### Phase 1 — フック間の矛盾を先に解消する（`post-edit.mjs`・独立導出が発見）

**述語を配線する前に置く。** Phase 2 以降は `.claude/hooks/**` を編集して `hook-selftest` を自動発火させるが、
それが失敗したとき出る再現コマンドは `node C:\workspace\Snotra\node_modules\vitest\vitest.mjs run .claude/hooks` で、
**再現コマンドが最も要る局面でそのまま打てない**。この Phase は述語と独立なので、先に置けば窓が消える。

- [x] `buildCommand` / `runCheck` の `repro` 文字列を POSIX 区切りへ正規化する（D10。実行に使う `spec.cmd`/`spec.args` は変えない＝`governance-check.mjs`:485 の G9 に無影響）
- [x] 正規化は **`toRelative`（`post-edit.mjs`:95）と同じ `.split(path.sep).join("/")`** を共有ヘルパへ束ねて使う（`replaceAll("\\","/")` は POSIX でファイル名中の `\` を潰すため使わない・`/dry-check` 候補 2）
- [x] `post-edit.test.mjs` に post-edit 側の義務のカナリアを足す（`repro` に `\` を出さない・`args` は正規化しない）。**相互契約カナリア（`decide` を直に呼ぶ側）は述語が存在してから置くため Phase 3 へ移した** — Phase 1 の目的は「窓を閉じる」ことであり、判定に依存させると Phase 1 が red のまま進むことになる
- [x] 既存の post-edit テストが緑であることを確認する

### Phase 2 — 判定の実装（`.claude/hooks/pre-bash.mjs`）

- [x] `segmentEnd(command, at)` を抽出し、`hasSafeChain` のインライン計算を載せ替える（D4・挙動不変）
- [x] 区切り文字集合 `[;&|\n\r]` を定数へ寄せる（現在 `:91` と `:98` に別リテラル。`:98` は区切り列の列挙で関数は共有できないため定数だけ共有する・`/dry-check` 候補 3）
- [x] `gitSegments(command)` を追加（コマンド位置の `git` ごとに `segmentEnd` までを切り出す）
- [x] `usesHeredoc` を D8 の形で実装する（全候補 + 単独行 `Map` の 1 パス。**囮の `<<` が先行しても取り落とさない**）
- [x] `usesBackslashPath` / `needsPyEncoding` を追加する（`\` は `C:\` / `$env:X\` / `%X%\` の 3 形に限る・D7）
- [x] `usesNoVerify` / `pullWithoutFfOnly` を追加し、subcommand 判定に既存 `FLAG` 定数を使う（D9）
- [x] 5 件の拒否文言を定数で置く（I6。事故の理由 + 代わりの手段を各 1 文で。`CLAUDE.md` から降ろす知識の受け皿はここである）
- [x] `decide(payload, readGitState, readPlanState, platform)` へ第 4 引数を足し、**`command` 取得の直後・`gh pr create` 検出の前**に 5 判定を置く（I/O 不要な文字列判定を先に走らせる）
- [x] Windows 専用 3 件を `platform === "win32"` ゲートの内側に入れる（D3 の倒し方をコード注釈に書く）
- [x] `main()` の `decide` 呼び出しへ `process.platform` を渡す
- [x] `usesHeredoc` の `new RegExp` 補間が安全な理由と、線形であることの理由を注釈する（I2）

### Phase 3 — テスト（`.claude/hooks/pre-bash.test.mjs`）

- [x] 述語ごとの代表入力を移植する（research.md の実測ケース。真/偽の両方向）
- [x] 囮が先行する heredoc（`grep -rn "x << y" src && cat <<EOF\nbody\nEOF`）が block されることを固定する（D8 の回帰）
- [x] `git -C /x commit -n` / `git -C /x pull` が block されることを固定する（D9 の回帰）
- [x] 結合短フラグ（`-nm`）と、`git log -n 5` / `git push -n` を block しないことを固定する（D5 の射程）
- [x] 意図として固定する過剰検出をテストに残す（`git commit -m "fix: C:\path handling"` が block・`git pull --rebase` が block）
- [x] 意図として残す過小検出をテストで明示する（`git commit -m "a;b" --no-verify` が allow・D6）
- [x] `decide` の platform 注入テスト（`"win32"` で block・`"darwin"` で allow・省略時は allow = I5）
- [x] 非依存 2 件が platform を問わず block することを両値で固定する
- [x] 病的入力の no-throw テスト（空文字列・`<<` 単独・不一致引用・20 万字・`$env:TEMP\` 反復。I2）
- [x] **候補 2 万件の heredoc 入力に時間上限を課すテスト**（no-hang。素朴実装なら 1800 ms・採る形なら 5 ms 程度ゆえ十分な余裕で分離できる）
- [x] process 級 e2e: heredoc payload が `process.platform === "win32"` なら exit 2、他なら exit 0（両 OS CI がライブカナリアになる）
- [x] process 級 e2e: `--no-verify` payload はどの OS でも exit 2
- [x] ソースカナリア: `main()` が `decide` へ `process.platform` を渡していること（I5）
- [x] 既存 23 呼び出しが 3 引数のまま緑であることを確認する（I7・書き換えない）
- [x] `npx vitest run .claude/hooks` を実行して緑を確認する

### Phase 4 — ライブ フォールトインジェクション（Windows 機・1 度）

`.claude/rules/safety-nets.md`: これはガードの**行使**であり弱化ではないため、稼働中の hook に対して直接打ってよい。
**滑っても無害な形で打つ**（block されなければ何も起きないコマンドを選ぶ）。結果は PR 本文へ残す。

- [x] #1 heredoc: `cat <<EOF`（本体なし）→ exit 2 を確認
- [x] #2 `\` パス: `echo C:\workspace` → exit 2 を確認
- [x] #3 Python: `python -c "print('日本語')"` → exit 2 を確認
- [x] #4 `--no-verify`: `git commit --no-verify --dry-run -m x` → exit 2 を確認（滑っても `--dry-run` ゆえコミットは生じない）
- [x] #5 `git pull`: `git pull no-such-remote-768` → exit 2 を確認（滑っても remote 不在で失敗するだけ）
- [x] 逆方向: 無害なコマンド（`git status --short` / `npm run governance:check`）が通ることを確認する（誤爆していない証拠）
- [x] 5 件の block メッセージ全文を PR 本文用に記録する（I6 の確認も兼ねる）

#### Phase 4 の実測結果（Windows 11 / win32・稼働中の hook を行使・PR 本文へ転記する）

5 件すべてが `exit 2` で拒否され、**5/5 が復帰手順を含んでいた**（I6 の確認）。滑っても無害な形を選んだため、
拒否されなかった場合も副作用は生じない設計だった（`--dry-run` 併記・存在しない remote 名）。

| # | 打ったコマンド | 結果 | 拒否メッセージ（全文） |
|---|---|---|---|
| 1 | `cat <<EOF` | exit 2 | bash の HEREDOC は Windows で引用境界が壊れ、終端マーカーがコミットメッセージ本文へ漏れる事故が起きています。複数行テキストは Write ツールで一時ファイルへ書いて `git commit -F <path>` / `--body-file <path>` で渡すか、PowerShell の here-string `@'...'@`（閉じ `'@` は必ず行頭）を使ってください。**コマンドに書くパスは、先頭から末尾まで区切りを `/` にしてください**（絶対パスなら `C:/Users/...` の形）。 |
| 2 | `echo C:\workspace` | exit 2 | パス区切りの `\` はエスケープが要るため壊れやすく、Bash では黙って食われて**エラーではなく誤った結果**になります。`/` で統一してください — PowerShell でも Git / Node / Cargo は `/` を受け付けます。 |
| 3 | `python -c "print('日本語')"` | exit 2 | cp932 コンソールで非 ASCII（`—`・日本語など）を print すると `UnicodeEncodeError` で落ちます。`PYTHONIOENCODING=utf-8` を前置してください（`PYTHONUTF8=1` か `python -X utf8` でも同じです）。 |
| 4 | `git commit --no-verify --dry-run -m x` | exit 2 | `--no-verify`（commit では `-n` も同義）は `.githooks/` の main 保護を迂回します。**人間専用であり、エージェントは使用してはなりません**。hook が拒んだなら、迂回せずその理由を解消してください。main へ入れたい変更は feature ブランチと PR を経由します。 |
| 5 | `git pull no-such-remote-768` | exit 2 | 非 FF の `git pull` はマージコミットを作り、main では `.githooks/pre-merge-commit` が拒否します。`git pull --ff-only` を使ってください（**この判定はブランチを問わず発火します**。FF できないほど発散しているなら、feature ブランチでは `git fetch` してから `git rebase` を明示的に打ち、main では `.githooks/` が rebase も非 FF merge も拒むので、想定外のコミットが local main に入っていないかを先に確認してください）。 |

**#1 と #5 は `/norm-review` の 2 巡でそれぞれ書き換わっており、上の表は最終版を測り直した値である**（各巡の後にライブで取り直した）。

## norm-review — 5 件の拒否文言 + `docs/hooks.md` の追記

### 停止条件（着手前に決めたもの）
合格条件 3 件 / 上限 2 巡 / 塞ぎ 1 件あたり 1 文まで（超えるなら残余へ）

### 各巡の結果

- **1 巡目**: 手を抜く読者 6 件・規則を守る読者 4 件 → **成立 3 件を修正（降格 1 件）**
  - **成立①（両クラスが別経路で到達した最重要）**: heredoc の拒否文言が代替として「`$env:TEMP` 配下の一時ファイル」を挙げていたが、区切りを示していなかった。忠実な読者は `$env:TEMP\msg.txt` と書き、**同じ hook の `\` 判定に落ちる**——「代わりにこうせよ」が別の拒否を招く状態だった（`usesBackslashPath` が `\$env:\w+\\` で発火することを実測で確認）
  - **成立②**: `pull` の文言が「main にマージコミットを作り」と main 限定で書いていたが、判定はブランチを見ない。かつ発散した feature ブランチでは `--ff-only` が失敗し、代替経路がどこにも無かった
  - **成立③**: `docs/hooks.md` の残余リストに、接頭辞を持たない相対パス（`cat docs\hooks.md`）の検出漏れが挙がっていなかった（3 形に絞った代償が列挙から欠けていた）
  - **降格 1 件**: 「win32 のみの 3 判定に『規範の射程外』と書くのが『Windows だけの話』の足場になる」。**降格の理由**: platform 限定は issue #768 自身の設計（3 件は Windows 固有の失敗様態に当たる）であり、抜け道ではなく意図された射程である
- **2 巡目**（変更した 4 箇所のみを両クラスへ渡した）: 手を抜く読者 4 件・規則を守る読者 3 件 → **成立 3 件を修正**。**4 件すべてが 1 巡目の修正自身が作ったものだった**（#764 の実測と同型）
  - **成立④（最重要・規則を守る読者）**: `$env:TEMP/<名前>` と末尾だけ `/` にしても、Write ツールは絶対パスを要求するため解決結果は `C:\Users\...\Temp` から始まり、`git commit -F C:\...\Temp/名前` で **`C:\` 形を自ら踏む**。→ `$env:TEMP` の言及をやめ、「**コマンドに書くパスは先頭から末尾まで区切りを `/` にする**」という要件そのものを書いた（削る方向の修正）
  - **成立⑤（両クラス）**: 「`\` はこの hook の**別判定**に当たります」という付記が、読者を「何が正しいか」ではなく「**何が検出されるか**」の推論へ誘導していた（接頭辞なしの相対パスなら両方すり抜ける、という読み方を生む）。→ 機構への言及を削除
  - **成立⑥（両クラス）**: `docs/hooks.md` の `.\` 項には「規範としては `/` を使う」の念押しがあるのに、新設した相対パス項には無く、**非対称が「こちらは許可」と読ませた**。→ 念押しを二重に書くのではなく **2 項目を 1 項目へ束ねて**非対称を構造ごと消した（`/norm-review`「Step 3」の「列挙ではなく原理で書けないか」に従った）
  - **成立②の再修正**: 代替として挙げた `git merge` が main では `.githooks/` に拒否され**行き止まりになる**（規則を守る読者）。かつ `git merge` も merge commit を作るので `rebase` と同格に並べたのは誤り（手を抜く読者）。→ feature と main で別々の指示にした

### 機構化（規範ではなくテストへ落とした 1 件）

成立①の再発防止を文書の戒めにせず、**`pre-bash.test.mjs` のカナリア 2 本**にした（#593 の階梯: 規範 → 検査）。拒否文言に `$env:X` が `/` 以外を伴って現れたら赤、ドライブレター絶対パスが現れたら赤。**カナリア自身が効くことを、文言の複製に変異を当てて 6 ケース実測した**（`safety-nets.md`「稼働中のガードを弱めない」に従い、`SHAPE_REMEDY` は触っていない）。1 巡目の欠陥そのものを赤にすることを確認済み。
なお**このカナリアは最初バグっていた**（`\w+` の貪欲さがバックトラックして `$env:TEM` で一致し、正しい文言でも赤になった）——規範を機構へ移すとき、機構が正しいことは別に測る必要がある。

### 受容する残余

- **2 巡目で塞いだ 3 件は検証されていない**（上限巡数に達したため）。#764 の実測どおり「塞ぎが次の抜け道を作る」のは繰り返し起きるので、3 件目の巡があれば新たな指摘が出る可能性は高い。ただし 2 巡目の修正は**すべて削る方向または束ねる方向**であり、条項を足していないため、新しい免除句・新しい非対称は生じていない
- **`docs/hooks.md` の残余リストは網羅ではない。** リスト前置きが「検出されないなら使ってよいではない」と原理で否定しているが、これは規範であって機構ではない
- **降格した 1 件（platform 射程）は「抜け道ではない」と判断しただけで、非 Windows 環境での運用実績は無い**（この repo は Windows 専用の開発環境である）

**逆方向（誤爆していない証拠）**: `git status --short && git pull --ff-only --dry-run | tail -2` が通った
（`--ff-only` 付きの pull が allow されることを含む）。

**副次的な実測**: Phase 1 の効果が Phase 2 の途中で観測された。`ALLOW` の重複宣言で `hook-selftest` が
落ちたとき、会話へ届いた再現コマンドは `node C:/workspace/Snotra/node_modules/vitest/vitest.mjs run .claude/hooks`
（`/` 区切り）だった。Phase 1 を後回しにしていれば、この行はそのまま打てない形で届いていた。

### Phase 5 — 文書（規範を降ろす / 受け皿を作る）

- [x] `CLAUDE.md`「シェル環境（Windows / PowerShell）」節を削除する（見出し + 表 3 行）
- [x] `CLAUDE.md`「Git/GitHub 運用」から `--no-verify` と `git pull --ff-only` の 2 bullet を削除する
- [x] `CLAUDE.md`「フック」表の PreToolUse 行を**ラベルごと**一般化する（現ラベル「PR 作成前 push チェック」は push 専用の名前。**5 件を書き写さない**——写せば降ろした意味が消える。正準形で `docs/hooks.md`「PreToolUse（pre-bash.mjs）の実装契約」を指し、「拒否メッセージが復帰手順を持つ」だけを残す）
- [x] `docs/hooks.md`:12 の全称表現を是正する（「コマンド位置だけを見る」は 3 述語で偽になる。I3 の形へ・D11）
- [x] `docs/hooks.md` へ 5 判定・platform 注入（D1/D3）・爆発半径（I2）・フック間契約（I8）を追記する
- [x] `docs/hooks.md` の受容する未対応リスクへ D6・D7 と過小検出 2 件・過剰検出 2 件を列挙する（**「検出されないなら使ってよい」と読めない書きぶりにする**——既存の `sh -c` 項が採る「人間専用の意図的迂回」の形を踏襲する）
- [x] 同じ箇所に、`.githooks/_lib.sh`:20 が `--no-verify` での迂回を案内すること（**明示的に人間専用と書いてあるため欠陥ではない**）を 1 文で書き添える（次の読者が矛盾として再発見しないため）
- [x] `docs/adr/0009-*.md` を新規作成する（D1・D2・D3 の否定の知識に限る。D6・D7 は `docs/hooks.md` の既存リストが受け皿）
- [x] 削除した節を正準形で指す文書が無いことを `npm run governance:check` で裏取りする（G11 の母集団は `docs/superpowers/` を除外することを実測済み）

### Phase 6 — 面積 ratchet

- [x] Phase 5 の文書編集をすべて確定させた**後で** `npm run governance:check` を実行し、常時ロードの実測値を得る
- [x] `AREA_BUDGET.alwaysLoaded` を「実測 + 100 字」へ引き下げる（`rules` は本 PR で `.claude/rules/**` を触らないため据え置き）
- [x] 由来コメントへ日付 + **#768** 入りの項目を既存 6 件と同じ様式で追加する（引き下げ幅・実測値・理由 = 5 件の機構吸収）
- [x] `npm run governance:check` が G1..G11 緑であることを確認する

### Phase 7 — 規範レビューと最終検証

- [x] `/norm-review` を起動する（対象: **5 件の拒否メッセージ文言 + `docs/hooks.md` の追記**。停止条件は下記）
- [x] `/norm-review` の成立指摘を塞ぐ（1 件 1 文の予算内）。残余は PR 本文へ書く
- [x] `npm test` 緑（全ファイル）
- [x] `npm run governance:check` 緑
- [x] コミット（`chore:` / メッセージは一時ファイル経由）・push・PR 作成（本文に Windows ライブ実測 5 件の結果と block メッセージ全文を載せる）

**`/norm-review` の停止条件**（起動前に決める・SKILL Step 1）:
1. 合格条件 — (a) 5 件それぞれについて、拒否メッセージだけを読んだ読者が**代わりに打つコマンドを一意に決められる**こと (b) 「Windows だけの話だから macOS では好きにしてよい」と読める箇所が無いこと (c) `docs/hooks.md` の残余リストが「検出されないなら使ってよい」と読めないこと
2. 上限 2 巡
3. 上限時点で残るものは PR 本文の「受容する残余」へ
4. 塞ぎ 1 件あたり 1 文まで（超えるなら残余へ回す）

## テスト方針

| 追加/更新するテスト | 固定する不変条件 |
|---|---|
| 述語 5 件 × 代表入力（真/偽 両方向） | 検出の意味論（過剰検出は fail-closed 方向ゆえ許容・過小検出は意図として明示） |
| 囮先行 heredoc / `git -C` 付き commit・pull | D8・D9 の回帰（どちらも当初案が fail-open だった） |
| `decide` platform 注入 3 値（`win32` / `darwin` / 省略） | I5（`undefined` → 非 Windows）・Windows 専用 3 件の射程 |
| 非依存 2 件 × platform 両値 | `--no-verify` / `pull` が platform を問わないこと |
| 病的入力 no-throw + 候補 2 万件の時間上限 | I2（全域関数・線形）= 全 Bash のブロック/停止の回避 |
| process 級 e2e（heredoc は platform 条件付き / `--no-verify` は無条件）+ ソースカナリア | I5 の配線を両 OS の CI でライブに固定 |
| `post-edit.test.mjs` のフック間カナリア | I8（片方が指示し片方が拒む状態を作らない） |
| 既存カナリア 2 群（無変更で緑） | I1（fail-closed 骨格）・I4（matcher ドリフト） |

検証コマンド: `npx vitest run .claude/hooks`（Phase 1〜3 の反復用）→ `npm test`（全件）→ `npm run governance:check`。
`.claude/hooks/**` の編集では PostToolUse が `hook-selftest` を自動発火するため、**沈黙 = 合格**（手動再実行は不要）。
`CLAUDE.md` / `docs/**` / `scripts/*.mjs` / ADR の編集には検査が割り当てられていない（`selectChecks` を実測）——沈黙は
「何も走らなかった」であり、Phase 6 の `governance:check` を手で実行するまで何も測られていない。

## 境界条件（最低 1 件ずつ検証ケースを用意した）

| 境界 | ケース |
|---|---|
| heredoc 演算子と shift 演算子 | `cat <<EOF`（真）/ `grep -rn "1 << 3" src/`（偽）/ `echo x <<< y`（偽・herestring） |
| heredoc の追従集合 | `cat <<EOF \| tee f`（真）/ `grep -rn "x << y" src \| head`（偽・引用の閉じが守る） |
| heredoc の候補順序 | `grep -rn "x << y" src && cat <<EOF…`（真・囮が先行）/ 候補 2 万件（時間上限内） |
| `\` の用途 | `node C:\ws\x.mjs`（真）/ `grep -E "\d+" f`（偽・正規表現エスケープ）/ `find . -exec rm {} \;`（偽） |
| Python の逃げ道 | `PYTHONIOENCODING=utf-8 python …`（偽）/ `python -X utf8 …`（偽）/ `python -c "print(1)"`（偽・非 ASCII 無し） |
| セグメント境界 | `npm test && git commit -n -m x`（真）/ `grep -n "--no-verify" CLAUDE.md`（偽・言及と実行） |
| git のグローバルフラグ | `git -C /x commit -n`（真）/ `git -C /x log -n 5`（偽）/ `git --no-pager pull --ff-only`（偽） |
| `pull` のフラグ位置 | `git pull --ff-only origin main`（偽）/ `git pull origin main`（真） |
| platform 境界 | 同一コマンドが `win32` で block・`darwin` で allow・省略時 allow |
| 空・巨大・不一致引用 | 病的入力が throw しない |

## セルフレビュー

### `/plan-review`（Step 5a・台帳 4 件中 4 件実在）

**要対処 3 件をすべて再照合し、3 件成立（降格 0 件）。いずれも計画に反映した。**

1. **ADR 番号の衝突** — `docs/adr/` を自分で `Glob` し 0001〜0008 が使用済み・`0006-plan-ownership-boundary.md` 実在を確認。計画の「0006」を **0009** へ全箇所訂正した。
2. **`usesHeredoc` の計算量（+ 私が見ていなかった fail-open）** — 自分で 3 実装を比較実測。「最初の候補だけ」は囮先行で取り落とし、「全候補 × 全文走査」は候補 2 万件で 1812 ms。**単独行を 1 パスで `Map` 化する形が 4.5 ms で 14 ケース全一致**。D8 として採用し、I2 に **never hang** を追加、テストに時間上限を追加した。
3. **`post-edit.mjs` とのフック間衝突**（独立導出） — `post-edit.mjs`:252/257/344 を自分で読み、`path.join` が `\` 区切りの絶対パスを作ることを確認。再現コマンド 3 種すべてが `\` 判定に当たることを実測（3/3 block）。**片方のフックが指示するコマンドをもう片方が拒む**状態になるため、`post-edit.mjs` / `post-edit.test.mjs` を変更ファイルへ追加し、I8 と Phase 3 を新設した。

軽微な懸念からの反映: `-nm` / `git -C` 境界をテストへ明示（D9 で fail-open だったことが判明）・`segmentEnd`/`gitSegments` を export する方針を明記・「フック」表のラベル改名を Phase 5 へ明記・由来コメントに #768 を明示。

**独立導出との差分**:
- **漏れ（導出 ∖ plan）**: `post-edit.mjs` + `post-edit.test.mjs`（上記 3）と `docs/hooks.md`:12 の全称表現の是正。両方を計画へ取り込んだ。
- **判断が割れた点**: `platform === undefined` の倒し方。独立導出は Windows（fail-closed）、私は非 Windows。**要求は「配線を固定せよ」で共通**なので、倒し方は D3 のまま維持し、独立導出が求めたソースカナリアを process 級 e2e に**追加**して両方置いた。
- **スコープ過剰（plan ∖ 導出）**: なし。
- **一致（完全性の証拠）**: 第 4 位置引数による platform 注入・`git commit -n` の検出・`\` 判定を narrow に留めること・`.claude/settings.json` を変えないこと・`.githooks/_lib.sh` と `.claude/rules/safety-nets.md` を変えないことは独立に再一致した。
- **`docs/development-principles.md` の変更**は導出が挙げたが採らない（D11・理由付きで却下）。

### `\` 判定と衝突する「指示面」の自前走査（独立導出の corpus 報告を一次証拠にしない）

独立導出は「repo コマンド corpus 128 件中 6 ヒット（すべて説明済み）」と報告したが、**`post-edit.mjs` の衝突と同じクラス**
（文書がエージェントへ、新判定が拒むコマンドを打たせる）なので自分で走査し直した。`[A-Za-z]:\\` / `\$env:\w+\\` / `%\w+%\\`
を指示面（`docs/*.md`・`docs/adr/`・`.claude/**`・ルートと各モジュールの `CLAUDE.md`・`AGENTS.md`・`CONTRIBUTING.md`・`README.md`・`package.json`）へ当てた結果:

- **`docs/build-commands.md`（コマンドの SSOT）は 0 ヒット** — 文書化された呼び出しはすべて相対パス（`scripts/*.ps1` の
  `-ExePath` 既定値は script 内の default であってエージェントが打つ文字列ではない）
- `docs/architecture.md`:104（`%APPDATA%\Snotra\`）・`src-tauri/CLAUDE.md`:111（`C:` と `C:\` の違い）は**散文の説明**で命令ではない
- `.claude/hooks/post-edit.test.mjs` の 3 件はテスト fixture の文字列

**新たな変更対象は増えなかった**（`post-edit.mjs` が唯一の I8 サイト）。**受容する残余**: `src-tauri/src/icon.rs`:402 の
rustdoc が `$env:SNOTRA_ICON_DIAG_PATHS = "C:\path\a;C:\path\b"` という診断レシピを載せている。値はプレースホルダで
置換して使う形であり、#768 のスコープ外ゆえ変更しない。

### `/dry-check`（`AGENTS.md`「条件別チェック」の「関数・型を新規定義／導入」トリガー）

7 個の新規純関数の主要操作を grep（`search(/`・`[;&|`・`matchAll`・`-\S+\s+`・`x00-\x7F`・`no-verify`・`ff-only`・
`replaceAll(`・`path.sep`・`posix`）で `.claude/hooks/*.mjs` / `scripts/*.mjs` / `.githooks/*` へ当て、候補 7 件を判定した。
**[置換] 3 件・[維持] 4 件。D4 は唯一のヒットではなかった**:

1. **[置換]** `pre-bash.mjs`:91 → `segmentEnd`（D4・既知）
2. **[置換] `post-edit.mjs`:95 `toRelative` が既に `.split(path.sep).join("/")` を持つ（docstring:「区切りは常に / に正規化する」）。**
   D10 の実装で `replaceAll("\\", "/")` を新設してはならない——**POSIX では `path.sep === "/"` ゆえ `split/join` 版は
   no-op だが、`replaceAll` 版はファイル名中の正当な `\` を潰す。`npm test` は ubuntu でも走るため後者は誤りである**。
   → D10 は `toRelative` と同じイディオムを共有ヘルパへ束ねて使う（**probe で書いた形の訂正**）
3. **[置換]** 区切り文字集合 `[;&|\n\r]` が `pre-bash.mjs`:91 と :98 に別々のリテラルで在り、`gitSegments` が 3 つ目を作る
   → 文字クラスを定数へ寄せる（:98 は区切り**列の列挙**で `segmentEnd` では表現できないため、関数ではなく定数だけ共有する）
4. **[維持]** `governance-check.mjs`:38 / `clean-worktrees.mjs`:34 の `replaceAll("\\","/")` — 別ツリーの既存コード。
   `pre-bash.mjs`:176 が明文化する「2 hook 間の import は結合を増やすため許容」と同じ理由で hooks ↔ scripts も束ねない
5. **[維持]** `githooks.test.mjs`:17,63 の同イディオム 2 重 — #768 は `.githooks/` を触らないためスコープ外
6. **[維持]** `.githooks/_lib.sh`:20 の `--no-verify` / `githooks.test.mjs`:146 の `--ff-only` — 判定ロジックではなく
   メッセージ文字列と実行。前者は Phase 5 で 1 文書き添える扱い
7. **[維持]**（DRY 違反ではないが影響範囲の発見）`governance-check.mjs`:485 の G9 は `cargoSpec` の**引数配列**を読む。
   D10 が `repro` 文字列だけを変え `spec.cmd`/`spec.args` を変えない方針は **G9 に無影響**である

### 5b の 3 観点

1. **境界条件** — 上表 10 行。plan-review 後に「heredoc の候補順序」「git のグローバルフラグ」の 2 行が増えた（どちらも当初案が fail-open だった箇所）。
2. **シンプル化の挑戦** — 新たな状態（`AtomicBool` / `Mutex` / 子プロセス / listener）を**一切導入しない**ため生成/破棄のペアは発生しない。追加するのは純関数 7 個と定数のみで、`decide` の分岐表は I/O 前に文字列判定を挿すだけ。最も複雑なのは `usesHeredoc`（2 段の選択肢 + 1 パスの `Map`）だが、単純化した「最初の候補だけ」は実測で fail-open、「全候補 × 全文走査」は実測で 1812 ms であり、**この複雑さは両方を回避する最小の形**である。5 判定のうち `\` は最も摩擦が大きく（散文中の `C:\` も止まる）、唯一もう 1 つのフックの変更を強いた——それでも残す根拠は issue が名指しし、かつ独立導出が「自分も規範を読んだ直後に破り、`\` の破損が**エラーではなく誤った結果**を生んだ」と一次証拠を報告したことである。
3. **破壊不変条件 + 検知手段** — 上表 4 行。すべて検知手段（テスト・自動発火する `hook-selftest`・ライブ実測）とセットで書いた。「戻ってこない」系のリスクは**壊れた hook を配線した瞬間に全 Bash が止まること**であり、復旧経路（Edit ツールは PreToolUse の対象外）を明記した。

## code-reviewer の結果（`/implement` 4b）

成果物は `workspace/code-review.md`。**High 2 件・Medium 2 件を報告され、High 2 件 + Medium 1 件を修正した**（すべて自分で再照合して成立を確認。修正案の挙動同値も自分で測った）。

- **H1（High・I2 違反）— `FLAG` の指数爆発。** `-{1,2}\S+` は `--x` を「`-`+`-x`」「`--`+`x`」の 2 通りに解釈でき、どちらも同じ位置で終わる。この曖昧さが `*` の下にあると全体が失敗するとき 2^n 通りを探索する。**#768 でこの `FLAG` が `git` セグメント判定経由で全コマンドに乗ったため、爆発半径が広がっていた**（従来は `gh pr create` を含むコマンドだけ）。
  - **再照合で一度誤判定した**: 最初 `git log --f0=v0 …` で測って 0.0ms を得て「問題なし」と判断しかけた。`log` が非フラグなので `FLAG` は 0 回で即失敗し、バックトラックが起きない。フラグを `git` の**直後**に並べる形（`git --f0=v0 … x`）で測り直すと n=24（225 字）で **747ms**、+1 ごとに倍増した。**指摘の再現には入力の形が要る——「測って出なかった」は「無い」ではない。**
  - 修正: `(?:(?:--?[^-\s]\S*|--)(?:\s+[^-\s]\S*)?\s+)*`。ダッシュの直後に非ダッシュを要求して分割を一意にする。**17/17 挙動同値**・n=2000（25785 字）で 0.26ms。
  - **これは throw でも hang でもなく「遅い」形の fail-open** である（hook が timeout すると exit≠2 = 非ブロッキング = コマンドがそのまま実行される）。ゆえに回帰テストは**時間**で縛った。
- **H2（High・I6 違反）— `\` 判定が正規表現を巻き込む。** 素の `[A-Za-z]:\\` は「語末 1 文字 + `:` + 正規表現エスケープ」と区別できず、`rg "version:\s+" src` / `grep -E "error:\s" log` / `rg "name:\w+" src` を block した（3/3 実測）。**拒否文言は「`/` で統一せよ」なので復帰手順が適用できない**——誤爆の中で最も害が大きい形である。修正: `(?<!\w)[A-Za-z]:\\[\w.$%~-]{2,}`（真陽性 7 件維持・**15/15 一致**）。代償として `cd C:\` と `C:\a` を見ない（過小検出・テストで意図として固定）。`docs/hooks.md` の「正規表現エスケープは巻き込まない」は修正前は**偽**だったので実装に合わせて書き換えた。
- **M1（Medium）— `HEREDOC_FOLLOW` の `)`。** `node -e "console.log(1<<n)"` / `node -e "x = (a << b)"` を誤爆した（2/2 実測）。`)` を落として **16/16 一致**（真陽性 11 件維持）。サブシェル内の heredoc `(cat <<EOF)` はこの枝で拾えなくなるが、終端行の索引が拾うので取りこぼしにはならない。
- **M2（Medium）— 行継続をまたぐ `--no-verify` は fail-open。** `git commit \`+改行+`--no-verify`（PowerShell のバッククォート継続も同型）はセグメントが改行で切れて見えない。**修正せず残余とする**——D6（引用内の `;` でセグメントが切れる）と**同一クラス**（区切りの走査がシェルの構文を理解しない）であり、`segmentEnd` は `hasSafeChain` と共有されているので継続の扱いを変えると既存判定の意味も変わる。エージェントが 1 行で書く常用形では起きない。
- **H0（Medium・プロセス）**: レビューは working tree に対して行われた（`git diff main...HEAD` では norm-review 2 巡目の修正が入っていない）。**指摘は正しい**——緑を確認してからコミットする運びだったため、レビュー時点で 4 ファイルが未コミットだった。レビュアーは dirty tree で再実測して緑を確認している。
- **観点 4 への回答（テストが固定していない不変条件）を受けて追加したもの**: I2 の no-hang は `usesHeredoc` 1 本しか縛っておらず、**隣の 2 述語で実際に破れた**（H1）。時間で縛るテストを `git` フラグ列にも足した。

### 自分で追加で測ったこと（レビュー依頼の観点 1 の裏取り）

PowerShell 固有の 18 形を実測（`;` 区切り・`&` 呼び出し演算子・バッククォート継続・パイプライン改行・`if`/`foreach` ブロック内・`Select-String` での言及）。**18/18 期待どおり**——`AT_CMD_POS` の区切り集合に `{}` が入っているためブロック内も拾い、文字列内の言及は `git` がコマンド位置に来ないので拾わない。

