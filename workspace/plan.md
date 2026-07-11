# plan — issue #509: Windows ランナーでも `npm test` を走らせる

## ゴール

CI（`ci.yml`）の windows-latest ジョブ（`rust-check`）で `npm test`（vitest: ui + `.claude/hooks` + `.githooks`）を実行し、Windows 固有の故障モードを PR CI で捕捉できるようにする。対応表（`docs/build-commands.md`）を同期し、故障注入で新ステップが実際に赤くなることを一度実測する。

## 変更ファイル一覧

### 1. `.github/workflows/ci.yml`（`rust-check` ジョブにステップ追加）

`rust-check` ジョブの**末尾**（`cargo clippy` ステップの後）に、以下 3 ステップを追加する。

```yaml
      # #509: main 保護 Layer 1（.githooks）と PreToolUse/PostToolUse フック
      # （.claude/hooks）の selftest は、実運用が Windows でのみ起きる安全網。
      # ubuntu の frontend-check と相補的に、Windows 固有の故障モード
      # （CRLF・shebang 実行・PowerShell tool_name 経路）を検知する。
      - name: Setup Node.js
        uses: actions/setup-node@v6
        with:
          node-version: 22
          cache: npm

      - name: Install frontend dependencies
        run: npm ci

      - name: Run tests on Windows (vitest: ui + .claude/hooks + .githooks)
        run: npm test
```

**設計判断**:
- **ジョブ名 `rust-check` は変えない** — required status check 名を静かに壊さないため。npm test 追加はステップ追加のみで吸収し、ステップ名/コメントで意図を明示する。
- **末尾に置く**（cargo の後） — 「前後どちらでも可」（issue）。rust-check の一次目的は cargo なので、cargo を先に走らせ、追加した安全網検査を末尾に append する。機能上どちらでも同値（GitHub Actions は全ステップを逐次実行し、失敗位置に関わらずジョブは赤になる）。
- **`npm test` 全体を走らせる**（`vitest run .claude/hooks .githooks` に縮退しない） — SSOT コマンド（`package.json` の `test`）との乖離を作らないため（issue の推奨・`docs/build-commands.md` の整合規約）。ui テストは jsdom で OS 非依存性が高く二重実行は冗長だが、縮退のドリフトコストの方が高い。実行時間が問題化した場合のみ縮退を再検討（YAGNI: 今は測っていない問題を先取りしない）。
- **既存 `frontend-check` の写し** — node 22 / `cache: npm` / `npm ci` は ubuntu 側と同一。新規パターンを導入しない。

### 2. `docs/build-commands.md`（対応表の同期）

L112 の `npm test` 行を更新:

```
| `npm test`（Vitest） | `ci.yml`（frontend-check=ubuntu / rust-check=windows） | PR 自動（`skip-ci` ラベルで無効化可） |
```

さらに対応表直下の箇条書き（実ファイルでは L121 の箇条書き本体の後）に 1 項を追加:

> - `npm test` は ubuntu（frontend-check）と windows（rust-check）の両方で走る。`.githooks` / `.claude/hooks` の selftest は実運用が Windows でのみ起きる安全網であり、hook 実行機構（Git-for-Windows の shebang 経由 sh 起動・パス/クォート境界）が本番と一致する OS で回帰検査する。ubuntu 側は実行ビット・POSIX sh 厳密性を相補的に担保する（CRLF 由来の fail-open は `.gitattributes` の `eol=lf` で両 OS 回避済み・かつ dash 側の故障モードなので windows 固有ではない）。#509

**根拠**: `/health-check` Check 10 が「workflow で実行されているが表に無い / 表の workflow 名がずれている」を検知する。npm test が rust-check でも走ることを表へ反映しないと Warning になる。

**【plan-review 訂正】CRLF を windows の検知領域として書かない**。`.gitattributes:1-7`（実測）— CRLF fail-open は Linux/dash の故障で git-for-windows では再現せず、かつ `.githooks/** text eol=lf` が commit 正規化で CRLF を消す。当初案の「windows = CRLF」は逆だった。

### 3. `.claude/hooks/pre-bash.test.mjs`（L19 コメント訂正） / `.claude/hooks/post-edit.test.mjs`（L24 コメント訂正）

両ファイル冒頭に `// CI は ubuntu-latest。Windows リテラルパスを書くと …落ちる。` というコメントがある（`pre-bash.test.mjs:19` / `post-edit.test.mjs:24`）。**本変更でこれらの test は windows-latest（rust-check）でも走る**ため、「CI は ubuntu-latest」は事実として虚偽化し、将来の読者を「この test は Windows CI で走らない」と誤読させる（概念ラベルの静かな腐り。#482 の教訓）。前提を「CI は ubuntu(frontend-check) と windows(rust-check) の両方で走る（#509）」に訂正し、**指針（リテラルパスを書かず `path.join`/`tmpdir` から組む）は保全**する（両 OS で走る今こそ一層重要）。

- **これは Step 2b の独立再導出だけが拾った漏れ**。当初計画は「ci.yml + docs の 2 ファイル」に anchor して見落としていた。
- **注意**: `.claude/hooks/**` の編集は PostToolUse フックが `hook-selftest`（`vitest run .claude/hooks`）を自動発火する。コメントのみの変更なので green のまま（沈黙 = 合格）。

### 触らないファイル（変更なしの根拠）

- `vitest.config.ts` — `include` は既に 3 群を含む。windows で `npm test` を呼べばそのまま 3 群が走る。変更不要。
- `.githooks/githooks.test.mjs` — 使い捨て repo が git 設定を自足するため、CI ランナーで無改変で green になる。変更不要。
- `package.json` — `test` = `vitest run`、`prepare` の副作用は CI checkout で無害（research 論点 2）。変更不要。
- `SPEC.md` — CI/ツーリングの変更であり、プロダクトの挙動（IPC 契約・状態遷移・フロー）を変えない。SPEC.md はスコープ外。**挙動変更なし**。

## 実装順序（フェーズ）

- **Phase 1**: `ci.yml` に 3 ステップ追加 → `docs/build-commands.md` の表 + 箇条書き更新。
- **Phase 2**: ローカル検証（YAML 妥当性・`npm test` の green 再現）。
- **Phase 3**: コミット → push → PR 作成。
- **Phase 4**: 故障注入（受け入れ条件 3）— 本 feature ブランチを土台にした throwaway ブランチで実測（下記「故障注入」）。

## 不変条件

1. **`rust-check` ジョブ名は不変**（required status check の安定性）。
2. **ライブの `.githooks` を故障注入で壊さない**（#504）。破壊は throwaway worktree/ブランチに限定し、main worktree の `core.hooksPath` 解決先（＝ライブのガード実体）を無傷に保つ。
3. **最終 PR の diff は 4 ファイル**（`ci.yml` + `docs/build-commands.md` + `.claude/hooks/pre-bash.test.mjs` + `.claude/hooks/post-edit.test.mjs`）。故障注入コミットは throwaway 側に閉じ、最終ブランチへ混入させない。
4. **`--no-verify` を使わない**（人間専用）。破壊は feature ブランチ上で行い、feature ブランチの commit は pre-commit を通る設計なので迂回不要。

## テスト方針

このタスクは CI 設定 + ドキュメントの変更で、追加すべきユニットテストは無い（新規コードパスが無い）。検証は以下:

1. **YAML 妥当性**: `python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`（PyYAML があれば）。無ければインデントを既存ステップと厳密一致させ、Phase 3 の CI 実行でパースエラーが出ないことを確認。
2. **ローカル `npm test` green**: `npm test` を手元 Windows で実行し 3 群すべて green を確認（既存の PostToolUse githooks-selftest / hook-selftest が日常的に green である裏取りでもある）。
3. **故障注入（受け入れ条件 3・スキップ不可）**: 下記。

### 故障注入（安全網が効いていることの実測）

**目的**: 追加した windows 側 `npm test` ステップが、hook 破壊を実際に赤で捕捉することを一度実測する。

**手順**（Phase 3 の後、本 feature ブランチが push 済みであることが前提 — windows 側 npm test ステップは本ブランチにしか無いため、破壊ブランチは本ブランチを土台にする）:

1. 本 feature ブランチから throwaway ブランチを切る（例 `chore/ci-npm-test-windows-faultinject`）。**ライブのガードを壊さないため worktree で行う**（`core.hooksPath` は相対 `.githooks` で操作対象ツリーのトップ基準に解決されるため、別 worktree での破壊は main worktree のガードに波及しない）。
2. **破壊の選び方**（research 論点 B・plan-review 訂正）:
   - **汎用破壊を採用**（primary）: `.githooks/pre-commit`（または `_lib.sh`）の main ブロック判定を無効化し、`githooks.test.mjs` の `expectBlocked`（「main 上の commit を拒否する」等）を失敗させる。これで windows 側 `npm test` が red になる = 受け入れ条件 3 の字義を満たす。故障注入が検証するのは「新設 windows ステップが hook テストを実際に走らせ、失敗時に赤で報告する（no-op でない・誤配線でない）」ことであり、汎用破壊で十分。
   - **CRLF・shebang 案は取り下げ**: CRLF は windows 固有ではなく（ubuntu/dash の領域）、`.gitattributes` の `eol=lf` で commit 正規化されて CI に届かない（research 論点 3）。「windows 固有破壊で相補性を実証」は criterion に不要かつクリーンに構成しにくいので追わない。相補性は対応表の注記で担保する。
   - **自コミットの詰まり回避**: 破壊は「main をブロックしなくする」方向（exit 0 相当）を選ぶ。feature ブランチの commit は元々通るため、破壊コミット自体が pre-commit に詰まらない（「全ブロック」方向を選ぶと自コミットが詰まるので避ける）。`--no-verify` は使わない。
3. throwaway ブランチをドラフト PR として push し、CI の `rust-check` → `Run tests on Windows` ステップが **red** になり、原因が `githooks.test.mjs` の `expectBlocked` 失敗であることを確認する。
4. 確認できたら **throwaway ブランチ/ドラフト PR を破棄**（マージしない・close）。worktree を `git worktree remove --force` + `git branch -D` で片付ける。ドラフト PR 本文に closing keyword を書かない（`.github/pull_request_template.md` が `Closes` 行を自動挿入しうる。誤って #509 を閉じないよう本文を確認 — ルート `CLAUDE.md` の該当節）。
5. 最終 PR（本 feature ブランチ）の diff が上記 4 ファイルのみであることを再確認。

**破壊不変条件（壊れたら即アウト）**: 故障注入中、main worktree のライブ `.githooks/pre-commit` が無傷であること。検知手段 — 故障注入前後で main worktree の `.githooks/pre-commit` の内容が不変であることを `git status`（main worktree に変更が出ていないこと）で確認。worktree 隔離が効く根拠は `CLAUDE.md:36`（実測「相対 core.hooksPath は操作されるツリーのトップ基準で解決」）+ `githooks.test.mjs:225-274`（V10 の linked worktree / サブディレクトリ実測）。

**外向き操作の注記**: 故障注入はドラフト PR 作成と CI 実行（外向き・課金あり）を伴う。実行は `/implement` フェーズで、Phase 3 の PR が緑になった後に行う。

## SPEC.md 更新要否

**不要**。CI/ツーリングの変更でプロダクト挙動を変えないため（上記「触らないファイル」）。

## セルフレビュー

### 5a. check スキル結果
- **`/plan-review`（常時実行）**: Step 2（Explore・CI/config/docs 層監査）+ Step 2b（Plan・独立再導出）を実行。
  - **要対処 1 件を反映済み**: 故障注入の CRLF ベクトルが `.gitattributes:1-7` により二重に不成立（① CRLF fail-open は Linux/dash の故障で windows では再現しない ② `eol=lf` が commit 正規化で CRLF を消す）。→ 破壊ベクトルを汎用破壊へ変更、docs 注記の CRLF 帰属を訂正、research も訂正。
  - **独立再導出だけが拾った漏れを反映済み**: `.claude/hooks/{pre-bash,post-edit}.test.mjs` の「CI は ubuntu-latest」コメントが本変更で虚偽化 → 変更ファイル 3 番として追加。
  - **一致（完全性の証拠）**: job 名改名リスクなし（他 workflow に `rust-check` への `needs` 参照なし）／`npm ci` の prepare 副作用は無害／`githooks.test.mjs` は git 設定を自足し CI 非依存／SPEC.md 更新不要（CI 節が存在しない）／worktree 隔離で main ガード無傷、を両エージェントが独立に再確認。
- **他の check スキルは非該当**: `/symmetric-check`（ubuntu/windows は対称ペアではなく相補。show/hide 型なし）・`/cache-check`（キャッシュ再利用なし）・`/persistence-check`（on-disk 形式なし）・`/state-check`（UI 状態なし）・`/race-check`（async 追加なし）。

### 5b. チェックリスト
1. **対称コードパス**: 対称ペアなし。frontend-check(ubuntu) が既に持つ `npm test` を rust-check(windows) にも置く「相補」であり show/hide 型の対称ではない。
2. **影響範囲の網羅性**: `.github/workflows/*.yml` 全 grep で `rust-check` への `needs` 依存なしを確認（Explore）。npm test の OS 前提を書いた記述を grep し、腐る 2 コメントを Step 2b が検出 → 反映。
3. **境界条件**: `npm ci` の `prepare` 副作用（CI checkout に `core.hooksPath` 設定）→ rust-check は checkout に commit/push しないため無害。`githooks.test.mjs` の hermetic 性（`initRepo` が user.name/email/gpgsign を自前設定）→ CI global 設定に非依存。
4. **リソース管理**: CI ステップ追加のみで恒久リソースの生成なし。故障注入の worktree は `git worktree remove --force` + `git branch -D` で破棄、ドラフト PR は close（生成/破棄ペア明示）。
5. **既存パターンとの整合**: `frontend-check` / `e2e.yml` の Node setup（`actions/setup-node@v6` / node 22 / `cache: npm` / `npm ci`）の写し。新規パターンなし。
6. **YAGNI**: `npm test` 全体を採用し縮退（`vitest run .claude/hooks .githooks`）は今の（未計測の）実行時間問題を先取りしないため見送り。要求範囲内。
7. **シンプル化**: 独立ジョブを新設せず既存 windows ジョブへ相乗り（checkout/セットアップ重複を回避）。新規状態・子プロセス・汎用インターフェースの導入なし。
8. **破壊不変条件の明示**: (a) `rust-check` ジョブ名不変（required status check の安定性）— 検知: 改名しないことをレビューで担保。(b) 故障注入中の main worktree ライブガード無傷 — 検知: `git status` で main worktree に変更が出ないこと + `.githooks/pre-commit` 内容不変を前後確認。

**総評**: completeness 高（両エージェントの一致で完全性の確度が上がり、相違点＝CRLF 誤りと 2 コメント漏れは反映済み）。実装着手可。
