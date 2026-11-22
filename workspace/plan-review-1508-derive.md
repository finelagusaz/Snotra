# #1508 独立導出（rust-check を文書だけの PR で早く緑にする）

導出者: 独立導出サブエージェント（2026-11-22）。`workspace/` の既存ファイル（plan.md・research.md 等）は読んでいない。計画らしき文書は目にしていない（`docs/superpowers/plans/2026-08-14-governance-check-pruning.md` が `skip-ci` の grep に 1 行ヒットしたが、本 issue とは無関係な過去の計画である）。

状態: 完成版（第 1 版に実測を追記した）。

推奨の要約: `ci.yml` に ubuntu の判定 job（新設・仮名 `scope`）を足し、`scripts/ci-scope.mjs`（新設）で merge commit の `HEAD^1..HEAD` を `--no-renames -z` で取る。変更パスが**すべて `.md`** のときだけ `docs_only=true` を出し、rust-check を `needs: scope` + `!cancelled()` 付きの `if` で job ごと skip する。push では判定しない。

---

## 5. 必須チェックかどうか（先に結論）

**rust-check は必須チェックではない。** ruleset は `default`（id 12941497）の 1 本だけで、規則は `deletion` / `non_fast_forward` / `pull_request`（承認 0）。`required_status_checks` 規則は無い。旧式の branch protection も無い。

```
$ gh api repos/finelagusaz/Snotra/rulesets/12941497
"rules":[{"type":"deletion"},{"type":"non_fast_forward"},{"type":"pull_request","parameters":{"required_approving_review_count":0,...}}]
$ gh api repos/finelagusaz/Snotra/branches/main/protection
{"message":"Branch not protected", "status":"404"}
```

既存の文書も同じことを書いている（`docs/build-commands.md:374`「CI は required status check ではない（ruleset `default` に required status checks の規則が無い・実測）ので、スキップしてもマージは通る」）。

帰結:
- issue の注意「paths フィルタで起動を止めると『未報告』で残る」は、**現状では当たらない**（必須でないチェックは、起動しなければ rollup に現れないだけ）。ただし将来 required にした日に、未起動 = `Expected — Waiting` で永久に止まる。job を起動したうえで中身を飛ばす形（後述 A/B）は、required 化しても壊れない。
- それでもマージが止まるのは、`scripts/merge-pr.mjs:184-186` が `mergeStateStatus !== "CLEAN"` で拒むからである。必須でないチェックが pending / failing のとき、GitHub は `UNSTABLE`（Mergeable with non-passing commit status）を返す。⚠️ pending の非必須チェックで `UNSTABLE` になることは GitHub の定義からの推論で、この PR で実測していない（open な PR が 0 件で、過去の PR の `mergeStateStatus` は後から取れない）。PR を出したら、文書だけの PR で rust-check が走っている間に `gh pr view <N> --json mergeStateStatus` を取り、`UNSTABLE` → 終了後 `CLEAN` の遷移を記録すること。
- ⚠️ `scripts/merge-pr.mjs:185` の拒否文「必須チェックの未通過・衝突・レビュー待ち」は、必須チェックが 1 つも無い現状では原因の言い方がずれている（実際に止めているのは非必須チェックの pending）。本 issue の範囲で直すかは判断事項。

---

## 1. 変更すべきファイルとシンボル

### 1a. 変更する（必須）

| ファイル | 何を |
|---|---|
| `.github/workflows/ci.yml` | 推奨（§2 案 B）: 新 job `scope`（ubuntu・checkout `fetch-depth: 2`・setup-node・`node scripts/ci-scope.mjs`、出力 `docs_only`）を足し、`rust-check`（`:68-72`）に `needs: scope` を付けて `if`（`:69-71`）を拡張する。案 A なら rust-check の各 step に `if` |
| 新設 `scripts/ci-scope.mjs`（名前は仮） | 判定の純関数（例 `isDocsOnly(paths: string[]): boolean`）と、`git diff` を取って `$GITHUB_OUTPUT` へ `docs_only=true|false` を書く CLI。workflow 内の inline bash/pwsh にしない理由は §2「判定をどこに置くか」 |
| 新設 `scripts/ci-scope.test.mjs` | 純関数の境界テスト（§4）。`vitest.config.ts:8-10` の `include` に `scripts/**/*.test.mjs` があるので、置くだけで node-check（ubuntu）と rust-check（windows）の `npm test` に入る |
| `docs/build-commands.md` | 対応表とその下の箇条（下の「偽になる散文」） |

### 1b. 偽になる（または意味が変わる）既存の散文

| 場所 | 現在の主張 | 変更後 |
|---|---|---|
| `docs/build-commands.md:352-357`（対応表のトリガー列） | rust-check 由来の行（`npm test` の windows 側・Pester・fmt・check/test/clippy・heap-trace・doc）が「PR 自動」 | 文書だけの PR では走らない。列の注記を 6 行へ写すのではなく、表の下に 1 か所だけ書く（`:375` と同じ「6 行が同じ事実の写しになる」理由） |
| `docs/build-commands.md:36` | 「CI の rust-check が、PR ごとに workspace の全 crate のテストを実行する」 | 「文書だけの PR を除き」が要る（全称が偽になる） |
| `docs/build-commands.md:45`・`:50`・`:53` | カテゴリ A／cargo doc を「CI（rust-check）が PR で自動実行する」 | 文書だけの PR では走らない。ただしその PR はカテゴリ A の対象（`.rs`）を含まないので、実害は無い。`:36` と同じ一句で足りるか判断 |
| `docs/build-commands.md:367` | 「`npm test` は、ubuntu（node-check）と windows（rust-check）の両方で走る（#509）」 | 文書だけの PR では windows 側が走らない |
| `docs/build-commands.md:371-381`（`skip-ci` 節） | `skip-safe` の定義（`:378`）と、ラベルを手で貼る運用 | 文書だけの判定を機構が持つなら、`:378` の skip-safe 集合と新しい判定の集合が**写し**になる。正本を判定コード側に置き、文書からはそこを指す（`.claude/rules/governance-docs.md:15`「機構の実装の詳細を散文へ写さない」） |
| `docs/build-commands.md:376` | 「`governance-check` job の中の `governance manifest delta` step だけは…」 | **既に偽**（`ci.yml:51-66` の governance-check は step 1 本だけ。`governance manifest delta` は `3a3e70f8`（#1254）で撤去済み）。本 issue が触る節の隣なので、`.claude/rules/governance-docs.md`「古い情報を残さない」に従い同時に直すのが筋 |
| `docs/build-commands.md:378` | skip-safe は「node-check / rust-check がテスト対象に持たない `docs/**`・`**/*.md`」 | **既に一部偽**: `scripts/verify-tree.test.mjs:108` が実ファイル `docs/build-commands.md` を読んでテストする。`docs/build-commands.md` の変更は `npm test` を落としうる |
| `.github/workflows/ci.yml:22-24`（node-check の前のコメント） | ubuntu 側は「rust-check（windows）と相補的に担保する（#509）」 | 文書だけの PR では windows 側が無い |
| `.github/workflows/ci.yml:159-162`（Setup Node.js の前のコメント） | Windows 側 selftest が「ubuntu の node-check と相補的に」検知する | 同上 |
| `.claude/hooks/pre-bash.test.mjs:37` | 「CI は ubuntu(frontend-check) と windows(rust-check) の両方で走る（#509）」 | 同上（`frontend-check` は既に旧名で古い） |
| `.github/labels.yml:21` | `skip-ci` の説明「通常 PR CI（frontend-check / rust-check）をスキップする」 | `frontend-check` は既に旧名（#532 で node-check に縮退）。本 issue でラベルを残すなら直す |
| `scripts/merge-pr.mjs:185` | 拒否文「必須チェックの未通過」 | §5 の ⚠️ |

偽にならないもの（確かめた）:
- `.github/workflows/ci.yml:13-19` と `:108-120`（キャッシュの保存経路は main の push だけ）: push を判定の対象外にする限り真のまま（§2）。
- `scripts/governance/checks/G-module-index.mjs:32`「（ただし `skip-ci` ラベルの付いた PR では走らない）」: そのカナリアは `npm test` で、node-check（ubuntu）は本変更で触らない。真のまま。
- `.github/workflows/smoke.yml:77-87`: rust-check のキャッシュの大きさ・プロファイルの話で、判定と無関係。

---

## 2. 判定方法（第 1 版）

### 何を「文書だけ」とみなすか — 許可リスト（fail-closed）

- **変更されたパスがすべて `.md` で終わるときだけ「文書だけ」。** 1 つでも他の拡張子・拡張子なしがあれば全部走らせる。
- 拒否リスト（`.rs` / `Cargo.toml` / `build.rs` … に当たれば走る）にしない。issue の注意どおり、拒否リストは `rust-toolchain.toml`・`.gitattributes`（`include_str!` が checkout 後の改行コードを読む: `src-tauri/src/indexing.rs:344`）・`src-tauri/clippy.toml`・`src-tauri/tauri.conf.json`・`src-tauri/icons/*`（`snotra-settings/src/app.rs:656` が `include_bytes!("../../src-tauri/icons/32x32.png")`）・`.github/workflows/ci.yml` 自身・`package-lock.json`・`scripts/lib/*.psm1`（Pester）を数え漏らすと、**漏れた分だけ黙って走らなくなる**（fail-open）。許可リストなら、漏れは「走らせすぎ」の向き（遅いだけ）に倒れる。
- ディレクトリ単位の許可（`docs/**`・`.claude/skills/**` など、`docs/build-commands.md:378` の skip-safe 集合）を使わない。現在そこに `.md` 以外のファイルは無い（`git ls-files docs .claude/skills .claude/rules .claude/agents | grep -v '\.md$'` が 0 行）ので `.md` 判定と同じ結果になる一方、将来そこへ `.mjs` などを置いた日に黙って広がる。

### 差分の母集団

- `pull_request` では、checkout されるのは merge ref（`refs/pull/N/merge`）の merge commit である。`actions/checkout` に `fetch-depth: 2` を渡し、`git diff --name-only --no-renames HEAD^1 HEAD` を取る。`HEAD^1` は merge ref を作った時点の base tip なので、これは「この PR を今の base に載せたときに変わるファイル」そのもの（AGENTS.md の 3 点形 `origin/main...<rev>` と同じ意味を、merge commit が既に持っている）。
- `--no-renames` が要る。既定の rename 検出では `foo.rs → foo.md` の改名が `foo.md` だけとして出て、**`foo.rs` の削除が見えずに文書だけと判定される**。
- 実行時の `origin/main` tip を基準にしない。#1092（`72005caf`）が同じ判断をしている: 実行時の tip を取ると「event 発火と job 実行の間に main が動いた分」が PR の差分に混ざる。merge commit の親は event と同時に凍結されている。
- GitHub API（`pulls/{n}/files`）や `paths-ignore` を母集団にしない。⚠️ どちらもファイル数の上限を持つ（API は 3000 件、workflow の paths フィルタは 300 件まで評価）。後者は GitHub のドキュメント上「最初の 300 件で一致しなければ走らない」ことがあり、許可リストの意味で fail-open になりうる。

### fail-closed の扱い

- 判定は「文書だけと**証明できた**ときに `docs_only=true` を出す」形にし、後段の条件は `!= 'true'` で書く（出力が空・未定義・typo なら走る）。
- `HEAD^2` が無い（merge commit でない）・`git diff` が失敗・差分が空、はすべて `docs_only=false`（走らせる）。差分が空を「すべてが文書」の空虚な真にしない。

### main への push の扱い

- **push では判定しない（常に全部走らせる）。** 理由は 2 つ:
  1. `ci.yml:13-19`・`:108-120` が main の push を rust-cache の唯一の保存経路にしている。push まで飛ばすと、文書だけのマージが続く間キャッシュが更新されず（rustc 更新を跨ぐと完全 cold へ戻る: `smoke.yml:37-42` と同じ論理）、次のコード PR が遅くなる。
  2. 判定の残余（将来 `include_str!` で `.md` を読むコードが入る等）を、マージ後の main で拾う最後の網になる。既存の `skip-ci` と同じ形（`docs/build-commands.md:375`「スキップした node-check / rust-check は、マージ後の main への push では必ず走る」）。

### 判定をどこに置くか

- `scripts/` の node スクリプト + vitest にする。境界（改名・削除・空・拡張子の大小文字）を純関数のテストで理由ごとに固定でき（`.claude/rules/safety-nets.md:45`）、両 OS の `npm test` に乗る。inline の bash/pwsh だと境界のテストが CI の実走でしか取れない。
- ⚠️ rust-check（windows）の中で走らせるなら、Node の用意を判定より前へ動かす必要がある（現在 `Setup Node.js` は `:163` で cargo 系の後）。runner にプリインストールされた node に頼ると、`ci.yml:89-92` が戒める「プリインストールへの暗黙依存」になる。

### 差分取得の実測（一時リポジトリ・scratchpad）

base → `foo.rs` を追加 → PR ブランチで `git mv foo.rs foo.md` と `日本語.md` を追加 → main を 1 つ進めて `--no-ff` で merge し、`HEAD^1..HEAD` を取った:

```
== default             （rename 検出あり。foo.rs の削除が見えない → 「文書だけ」に誤判定する）
foo.md
日本語.md
== --no-renames        （foo.rs が出る → 走る）
foo.md
foo.rs
日本語.md
```

- 手元は `core.quotePath` を変えてあるので引用符が出なかった。runner の既定（`core.quotePath=true`）では非 ASCII のパスが `"\346\227\245...md"` と引用されて末尾が `.md"` になり、**安全側（走る）へ倒れるが、日本語名の文書だけの PR は永久に速くならない**。`-z`（NUL 区切り・引用しない）で取る。現在のツリーに非 ASCII 名の `.md` は無い（`git -c core.quotePath=true ls-files '*.md' | grep '"'` が 0 行）。

### 案の比較（job を飛ばすか、job の中の step を飛ばすか）

| | 案 A: rust-check の中で step を飛ばす | 案 B: ubuntu の判定 job + rust-check を job ごと skip | 案 C: rust-check を別 workflow へ出して `paths-ignore: ['**/*.md']` |
|---|---|---|---|
| 形 | 先頭 step で `docs_only` を出し、以降の全 step に `if: steps.scope.outputs.docs_only != 'true'` | 新 job `scope`（ubuntu・setup-node・判定）→ rust-check に `needs: scope` と `if` | workflow レベルのフィルタ。判定コードを持たない |
| 文書だけの PR の rust-check | SUCCESS（windows runner は起動する。checkout + 判定で数十秒） | SKIPPED（windows runner を起動しない） | 未起動（rollup に現れない） |
| required 化した日 | 壊れない | 壊れない（skipped は必須チェックを満たす扱い） | `Expected — Waiting` で止まる（issue の注意そのもの） |
| 失敗時の向き | 判定 step が赤 → job 赤（見える）。出力が空 → `!= 'true'` で全部走る | **罠**: `needs` 先が失敗すると後続は既定で skipped（= 緑扱い）。`if: ${{ !cancelled() && (github.event_name == 'push' \|\| (!contains(...skip-ci...) && needs.scope.outputs.docs_only != 'true')) }}` のように `!cancelled()`（または `always()`）を付け、判定 job が赤でも rust-check が走るようにする。判定 job 自体の赤で PR は CLEAN にならないので見える | 判定の失敗という経路が無い代わりに、評価上限（⚠️ GitHub のドキュメントで 300 件と記憶。未確認）を超えた差分で走らない向きがありうる |
| 保守 | `if` を 17 step に写す。新しい step へ付け忘れても「走りすぎ」の向き（安全側）だが、DRY の指摘を受けやすい | `if` は 1 か所 | 最小。ただし concurrency・キャッシュのコメント・`docs/build-commands.md` の対応表を別ファイルへ割る |
| Node の用意 | rust-check（windows）で判定より前に `actions/setup-node` を動かす要がある（現状 `:163`）。プリインストールの node に頼ると `ci.yml:89-92` が戒める暗黙依存 | ubuntu の判定 job で setup-node するので問題が消える | 不要 |
| コード PR の待ち | 増えない | 判定 job（ubuntu・約 5〜10 秒。node-check の Checkout〜Setup Node が 5 秒: run 44923964497）だけ直列に伸びる | 増えない |

**推奨: 案 B。** 理由: windows runner を起動しない（最も速い）、`if` が 1 か所、Node のプリインストール依存が生まれない、required 化しても壊れない。`needs` 失敗時の skipped を `!cancelled()` で塞ぐことを必須条件にする。⚠️ 案 B の「job の skipped で `mergeStateStatus` が CLEAN になる」ことは未実測。既存の手がかりは `skip-ci` で job が SKIPPED のままマージされた PR（#585・#459・#443。`gh pr list --state merged --label skip-ci --json number,statusCheckRollup` で `rust-check:COMPLETED:SKIPPED`）だが、どれも `merge-pr.mjs`（#1312）より前で、CLEAN だった証拠にはならない。PR 本文のチェックリストで測る（§4）。

既存の `if` の 2 条件（`github.event_name == 'push' ||` と `skip-ci` ラベル）は新しい `if` にも残す。node-check と governance-check は変えない（governance-check は文書だけの PR こそが対象: `ci.yml:47-50`）。

---

## 3. rust-check の中で Rust 以外に走っている検査と、省いたときに失うもの

rust-check の step（`ci.yml:74-176`）のうち、Rust 以外:

| step | 中身 | 実測の所要（run 44923964497） |
|---|---|---|
| `Run tests on Windows (vitest: ...)`（`:172-173`） | `npm test` = `vitest run`（`package.json:8`）。対象は `vitest.config.ts:7-11` の `include`（`.claude/hooks` / `.githooks` / `scripts` の `*.test.mjs`） | 52 秒（ubuntu の node-check では 10 秒） |
| `Run PowerShell tests (Pester)`（`:175-176`） | `npm run test:powershell` → `scripts/run-pester.ps1`。`scripts/lib/*.Tests.ps1`（6 本）。一部は `cargo build -p snotra`（`:148-149`）の成果物を起動する実バイナリ統合 | 23 秒 |
| （前提）`Setup Node.js` / `Install dependencies` | 上 2 つのための準備 | 5 秒 + 4 秒 |

ジョブ全体は 4 分 57 秒（16:19:47 → 16:24:44）。キャッシュ復元だけで 42 秒。

**失うものは、テストを列挙するのではなく性質で書く**（列挙は母集団を保証できない: 下の ⚠️）:

1. **Windows で `npm test` を回すこと（#509）。** `.md` だけの差分では、同じ `vitest run` が ubuntu の node-check で走り続ける（node-check は変えない）。したがって失うのは、「文書を入力に読むテストの、Windows 固有の挙動」に限られる。Windows 固有の挙動として残るのは、checkout の改行コード（windows runner の autocrlf）による入力の違いである。hook の実行機構（shebang 経由の sh・パスとクォート）の回帰は `.claude/hooks/**` / `.githooks/**` / `scripts/**` の変更でしか入らず、それらは `.md` でないので判定で必ず走る。
   - 文書を入力に読むテストの例: `scripts/verify-tree.test.mjs:108` が実ファイル `docs/build-commands.md` を読む（`parseCategoryCommands` は `\r?\n` で割る: `scripts/verify-tree.mjs:70`）。⚠️ これで全部とは言えない。grep は同じ行にリテラルのある読み方しか拾わず、ツリー全体を入力に取るテスト（`scripts/governance-check.test.mjs` の母集団カナリアなど）は母集団の外である。
2. **Pester。** 文書だけの差分は `scripts/lib/*.ps1|psm1` も製品バイナリも変えないので、Pester の入力は変わらない。⚠️ Pester が `.md` を読むかは `*.Tests.ps1` の grep（ヒットはコメントだけ）でしか見ていない。
3. **「Rust 以外だけ残す」案は成立しにくい。** Pester の一部は `cargo build -p snotra` の成果物に依存するので、Rust のビルドを飛ばして Pester だけ残すことはできない。vitest（Windows）だけ残すと、windows runner の起動 + checkout + setup-node + `npm ci` + 52 秒で、受け入れ条件 (1)「早く緑」を満たしにくい。
4. **外界の変化の検出。** `rust-toolchain.toml` の冒頭コメントは「リポジトリが 1 行も変わらなくても外界の toolchain 更新で赤くなる」事故（`.rs` を含まない PR が踏んだ）を記録している。toolchain は pin 済み（`channel = "1.99"`）だが、runner イメージの変化は文書だけの PR では観測されなくなる。main への push は常に全部走る（§2）ので、マージ後の main で拾われる。

---

## 4. 境界条件と検証方法

### 判定の境界（純関数 `isDocsOnly` のテストで理由ごとに固定）

| 入力（変更パスの集合） | 期待 | 理由 |
|---|---|---|
| `["docs/a.md", "AGENTS.md"]` | true | 文書だけ |
| `["src-tauri/CLAUDE.md"]`・`["snotra-settings/SETTINGS-DESIGN.md"]` | true | crate 内の `.md`。Rust のビルドは読まない（`include_str!` / `include_bytes!` の対象は `.rs` と `src-tauri/icons/32x32.png` だけ: `git grep -nE 'include_str!|include_bytes!' -- '*.rs'`） |
| `["src-tauri/capabilities/README.md"]` | true | tauri-build は capabilities を拡張子 `json` / `json5` / `toml` だけで拾う（`tauri-utils-2.10.2/src/acl/build.rs:42-47` の `CAPABILITY_FILE_EXTENSIONS` と `:226-231` のフィルタ。`Cargo.lock` の tauri-utils は 2.10.2）。現在このディレクトリには README.md しか無い |
| `["docs/a.md", "Cargo.toml"]` | false | 1 つでも非 `.md` |
| `["rust-toolchain.toml"]`・`["src-tauri/build.rs"]`・`[".gitattributes"]`・`[".github/workflows/ci.yml"]`・`["package-lock.json"]`・`["src-tauri/icons/32x32.png"]`・`["scripts/lib/SnotraSmoke.psm1"]` | false | 拒否リストなら漏れうる代表 |
| `[]`（差分が空） | false | 空虚な真にしない |
| `["README"]`（拡張子なし）・`["a.md.rs"]`・`["a.markdown"]` | false | 末尾が `.md` のものだけ |
| `["A.MD"]` | ⚠️ 判断事項 | 大文字を許すか。false（走る）に倒すのが安全側。現在のツリーに大文字の `.MD` は無い |
| 改名 `foo.rs → foo.md` | false | CLI が `--no-renames` で取るので `foo.rs`（削除）が入力に入る。純関数ではなく CLI のテストで見る（下） |
| 非 ASCII 名 `docs/日本語.md` | true | CLI が `-z` で取る。純関数ではなく CLI のテストで見る |

### 差分取得（CLI）の検証

- 純関数のテストでは `--no-renames` / `-z` / merge commit の親の取り方が見えない。`.githooks/githooks.test.mjs` の手本のように、一時リポジトリに base・PR ブランチ・merge commit を作り、CLI を当てて `docs_only` の出力を見るテストを 1 本以上置く（上の実測の再現: 改名・非 ASCII・空差分・merge commit でない HEAD）。

### 変異（設計の確定前に書く: `.claude/rules/safety-nets.md:46`）

| 変異 | 落ちるべきテスト | 赤の理由として確かめる文言 |
|---|---|---|
| `--no-renames` を外す | CLI テストの「改名 `foo.rs → foo.md` は false」 | `foo.rs` が入力に無いことによる true |
| `-z` を外す（引用つきで読む） | CLI テストの「`docs/日本語.md` は true」（runner と同じく `core.quotePath=true` を明示した一時リポジトリで） | 引用符つきパスで false |
| 空差分を true にする（`every` の空虚な真をそのまま返す） | 純関数の `[]` → false | |
| workflow の条件 `!= 'true'` を `== 'false'` にする | ⚠️ vitest では捕まらない（workflow の式を評価する仕組みが無い）。判定 job で出力を書く前に落とす変異（`exit 1`）を検証用 PR に入れ、rust-check が走る（skipped にならない）ことを Actions の実走で 1 回確かめる。`!cancelled()` を外す変異も同じ実走で見る | |

### 受け入れ条件の測り方（PR 本文のチェックリストへ: `.claude/rules/safety-nets.md:47`）

- **(1) は導入 PR 自身では観測できない。** 導入 PR は `ci.yml` と `scripts/` を触るので必ず全部走る。次のどちらかで測る:
  - マージ後の最初の文書だけの PR で、rust-check が SKIPPED（案 B）／短時間の SUCCESS（案 A）になり、その間に `gh pr view <N> --json mergeStateStatus` が `CLEAN` を返し、`npm run merge:pr -- <N> ... --dry-run` が mergeStateStatus で拒まないことを記録する。
  - あるいは導入前に検証用 PR を立て、`.md` だけのコミットで測る。
  - あわせて、判定の導入前に文書だけの PR で rust-check の走行中に `mergeStateStatus` を取り、`UNSTABLE` であること（§5 の ⚠️ の裏付け）を記録すると、issue の前提そのものを実測できる。
- **(2) 境界の記録。** 同じ検証用 PR に 1 コミットずつ `rust-toolchain.toml`・`Cargo.toml`・`src-tauri/build.rs`・`.github/workflows/ci.yml`・`.gitattributes` の無害な変更を足し、毎回 rust-check が走ることを run id つきで記録する。その後 `.md` だけに戻したコミットで skipped に戻ることも記録する。
- main の push で判定を通らないこと: マージ後の main の run で rust-check が全 step 走ったことを run id で記録する。

### 実施していないこと

- `mergeStateStatus` の `UNSTABLE` / `CLEAN` の実測（open PR が 0 件）。
- 案 B の `!cancelled()` の挙動の Actions 上での実走。
- GitHub の paths フィルタの評価上限（300 件）は記憶由来で未確認。案 C を採らない主理由ではない（主理由は required 化した日に止まること）。
