# 計画: issue #1508 — 文書だけの PR で rust-check を早く終える

## 目的と受け入れ条件

文書（`.md`）だけの PR で、`ci.yml` の `rust-check` を数十秒で SUCCESS に終え、`merge:pr` の CLEAN 待ちを解く。Rust・テスト・workflow などにかかわる変更では、従来どおり全 step を走らせる。

受け入れ条件:

1. 変更が全部 `.md` の PR で、`rust-check` が cargo・npm・Pester の step を走らせずに SUCCESS で終わる。この PR の差分は文書だけにならないので、CI での実測はマージ後に最初に来る文書だけの PR で行い、そのことを PR 本文に書く。`src-tauri/**`・`snotra-egui-runtime/**` の `.md` だけの PR では、`smoke.yml` の smoke-egui が従来どおり走る（`.md` を negation で外さない決定が `smoke.yml` にあり、今回は触らない）
2. 1 つでも `.md` 以外を含む PR、差分が 0 件・取れない PR、および main への push では、全 step が走る
3. 判定の境界（`.md` 以外の 1 件で走る・改名元が `.rs` なら走る・0 件で走る・取得失敗で走る・push で走る）を vitest で固定する
4. `docs/build-commands.md`「CI/CD メモ」が新しい挙動を正しく書いている（偽になる記述を残さない）

## 設計

- **判定は許可リストで fail-closed にする。** 「変更されたパスが 1 件以上あり、すべて `.md` で終わる」ときだけ省く。`Cargo.toml`・`build.rs`・`rust-toolchain.toml`・workflow・`scripts/**` などは列挙せずに「走らせる」側へ入る。
- **母集団は PR 全体の差分にする。** checkout を `fetch-depth: 2` にし、merge commit（`HEAD`）と第 1 親（`HEAD^1` = base の tip）の `git diff --name-only --no-renames -z` を取る。`--no-renames` で改名元も母集団に入れる。
- **skip は step の `if` で行う。** job は毎回起動し、判定の step の後の全 step に `if: steps.scope.outputs.rust != 'false'` を付ける。`!= 'false'` にするのは、出力が空（判定の step が書かなかった）でも走らせるためである。job の `if`（skip-ci の条件）は今のまま残す。job の conclusion は常に SUCCESS で、SKIPPED を `merge:pr` がどう扱うか（未実測）に依存しない。
- **push（main）では判定しない。** スクリプトは `GITHUB_EVENT_NAME` が `pull_request` 以外なら `rust=true` を出す。main の rust-check は rust-cache を保存する唯一の経路であり、マージ後に必ず走るという既存の保証も保つ。
- **Node は `actions/setup-node` の後で使う。** 既存の「Setup Node.js」step を checkout の直後へ移し、判定の step をその後に置く（runner のプリインストールへ暗黙に依存しない）。

## 変更するファイル

| ファイル | 対象 | 内容 |
|---|---|---|
| `scripts/ci-rust-scope.mjs`（新規） | `needsRustCheck(paths)`・`parseNameOnlyZ(stdout)`・`decide({ eventName, diff })`・CLI | 判定の純関数と、`GITHUB_OUTPUT` へ `rust=true\|false` を書く入口。理由と対象パスをログへ出す |
| `scripts/ci-rust-scope.test.mjs`（新規） | 上の 3 関数 | 境界のテスト |
| `.github/workflows/ci.yml` | job `rust-check`・job `node-check` の冒頭コメント | checkout に `fetch-depth: 2`、Setup Node.js を前へ移す、判定の step（`id: scope`）を足す、後続の全 step に `if` を付ける。job 冒頭のコメントで理由と残余を書く。node-check のコメント「rust-check（windows）と相補的に担保する」と、rust-check の Windows `npm test` の前のコメント（#509「ubuntu の node-check と相補的に」。Setup Node.js を前へ移すので、コメントは `npm test` の step の前に残す）に、文書だけの PR では windows 側が走らないことを添える |
| `docs/build-commands.md` | 「変更後の検証チェックリスト」の「CI の rust-check が、PR ごとに workspace の全 crate のテストを実行する」、「CI/CD メモ」の対応表の後の箇条（「`npm test` は、ubuntu（node-check）と windows（rust-check）の両方で走る」・`skip-ci` の節の `governance manifest delta` の記述・skip-safe の定義） | 自動で省く条件を 1 か所に書き、正本（`scripts/ci-rust-scope.mjs`）を指す。表の各行には注記を足さない。skip-safe の記述の偽（`docs/**`・`**/*.md` は実物を読む vitest がある）を直す。撤去済みの `governance manifest delta` step の記述を消す |

## 実装の順序

1. `scripts/ci-rust-scope.test.mjs` を先に書く（Red）
2. `scripts/ci-rust-scope.mjs` を実装する（Green）
3. `ci.yml` を変える
4. `docs/build-commands.md` を直す
5. 検証

## 不変条件と異常系

- main への push では、rust-check の全 step が走る（`decide` が push で `true` を返す。テストで固定）
- 差分の取得失敗（`HEAD^1` が無い・git の非ゼロ終了）→ `rust=true`。理由をログに出す
- 差分 0 件 → `rust=true`
- `GITHUB_OUTPUT` が無い → スクリプトは非ゼロで終わる（step が赤 = 見える失敗。黙って省かない）
- 判定の step が出力を書かずに成功しても、後続は `!= 'false'` で走る
- パス区切り: git は `/` で返す。`-z` で読むので、空白・非 ASCII のパスも 1 件ずつ取れる
- 拡張子の大小: `.MD` 等は今 0 件。判定は小文字の `.md` だけにする（大文字は走らせる側へ倒れ、安全側）
- 受容する残余: 文書だけの PR では、実物の `.md` を読む vitest（`verify-tree.test.mjs`・`governance-check.test.mjs` の dogfood・`G-module-linkage.test.mjs` のカナリア）が Windows で走らない。ubuntu の node-check は走り、main への push では Windows でも走る。この残余を `ci.yml` のコメントと `docs/build-commands.md` に書く

## テスト方針と検証コマンド

`scripts/ci-rust-scope.test.mjs` の境界:

- `["AGENTS.md", ".claude/skills/x/SKILL.md"]` → false（省く）
- `["AGENTS.md", "src-tauri/build.rs"]` → true
- `["Cargo.toml"]`・`["rust-toolchain.toml"]`・`[".github/workflows/ci.yml"]`・`["scripts/merge-pr.mjs"]` → true
- `[]` → true
- `["README.MD"]` → true（大文字は省かない）
- `["docs/x.md/inner.rs"]` → true（`.md` を含むが末尾でない）
- `parseNameOnlyZ`: NUL 区切り・末尾の NUL・空出力・非 ASCII のパス（`-z` は `core.quotepath` の引用をしない）
- `decide`: push → true（diff を呼ばない）、`pull_request` で diff が失敗 → true、改名（`--no-renames` の出力に旧 `.rs` と新 `.md`）→ true
- CLI の結合テスト（一時リポジトリに merge commit を作り、CLI を `GITHUB_OUTPUT` を一時ファイルへ向けて走らせる。git 操作の手本は `.githooks/githooks.test.mjs`）: `.md` だけ → `rust=false`／`.rs` → `.md` の改名 → `rust=true`／非 ASCII 名の `.md` だけ → `rust=false`／`HEAD` に第 1 親が無い（単一コミットのリポジトリ）→ `rust=true`／`GITHUB_EVENT_NAME=push` → `rust=true`

検証:

- カテゴリ A 相当は Rust を触らないので対象外。`npm test`（vitest）を走らせる
- `npm run governance:check`（ガバナンス文書を変えるため）
- ローカルで実リポジトリに対して CLI を走らせ、`.md` だけのコミット／`.rs` を含むコミットで出力が変わることを確かめる（`GITHUB_OUTPUT` を一時ファイルへ向ける）
- 変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: (a) `needsRustCheck` の `every` を `some` に替える (b) `--no-renames` を外す (c) `-z` を外す (d) 0 件で false を返す (e) push の分岐を消す — それぞれテストの assert で赤になることを確かめる。workflow 側の `if` の式（`!= 'false'`）は vitest では捕まらないので、CI の実走（PR 本文のチェックリスト）で確かめる
- CI での実測は PR 本文のチェックリストに置く: (a) この PR で、判定の step が `rust=true` を出し全 step が走ること (b) この PR の rust-check の実行中に `gh pr view <PR> --json mergeStateStatus` を読み、pending で UNSTABLE になるか（issue の前提の裏取り・敵対的調査 W1） (c) マージ後に最初に来る文書だけの PR で、rust-check の所要と SUCCESS、`merge:pr -- <PR> --dry-run` が CLEAN を通すこと

## SPEC.md・関連文書

- `SPEC.md` は変えない（製品の挙動ではない）
- `docs/build-commands.md` を上記のとおり直す
- `.github/labels.yml` の `skip-ci` の説明（撤去済みの `frontend-check`）は今回の範囲外。`skip-ci` ラベル自体の要否（文書だけの PR では不要になる）も範囲外とし、PR 本文に残す

## 作業項目

### フェーズ 1: 判定スクリプト

- [x] `scripts/ci-rust-scope.test.mjs` に境界のテストを書き、Red を確かめる
- [x] `scripts/ci-rust-scope.mjs` を実装し、Green を確かめる
- [x] 一時リポジトリの merge commit に CLI を当てる結合テストを書き、通す
- [x] 実リポジトリで CLI を走らせ、`.md` だけ／`.rs` を含む差分で出力が分かれることを確かめる

### フェーズ 2: workflow

- [x] `ci.yml` の rust-check に `fetch-depth: 2`・Setup Node.js の移動・判定の step・後続の `if` を入れる
- [x] 後続の全 step に `if` が付いていることを、step の数と `if` の数の照合で確かめる

### フェーズ 3: 文書

- [x] `docs/build-commands.md` の該当箇所を直す
- [x] `npm test` と `npm run governance:check` を通す

## 未確定（実装前に潰す）

- [x] 敵対的調査の所見を反映する — `workspace/adversarial-1508.txt` の B1・B2・W3・W7 を採り、受け入れ条件 1・残余・テストへ反映した。W1・W2・W6・W8 は PR の CI とマージ後の文書だけの PR で測る。採否は `research.md`「敵対的調査の採否」

## 承認

- [x] 自己承認 — 2026-11-22 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まない（CI の挙動で、製品の挙動ではない） / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない（workflow とスクリプトは revert で戻る） / 人の裁定: なし

## plan-review 結果

- リスク: 高（CI・`scripts/`・ガバナンス文書を変える）
- レビュー方式: 独立導出 1 体（`workspace/plan-review-1508-derive.md`。`workspace/` を読まず、計画を目にしていないと明記）
- エージェント数: 1

### 要対処

- 導出 ∖ plan: rust-check の Windows `npm test` の前のコメント（#509「ubuntu の node-check と相補的に」）が文書だけの PR で偽になる — 計画の修正（`ci.yml` の行へ追加）— `ci.yml` の該当コメントを読んで確かめた
- 導出 ∖ plan: 差分の取り方（改名・非 ASCII・第 1 親なし）は純関数のテストでは見えない — 計画の修正（一時リポジトリの merge commit に CLI を当てる結合テストを作業項目へ追加、変異に `-z` を追加）

### 軽微

- 判断の不一致: 導出は「ubuntu の判定 job ＋ rust-check を job ごと skip（`needs` と `!cancelled()`）」を推した。計画は step の `if` のまま据え置く。理由: (1) job の skip は SKIPPED を返し、`merge:pr` の CLEAN が SKIPPED を通すかは未実測（両者とも ⚠️）。step の `if` なら conclusion は SUCCESS で、この未実測に依存しない (2) issue が示した形である (3) 上流の失敗で黙って skip する経路（`needs` の既定）を作らない。費用は windows runner の起動と checkout・setup-node（数十秒）と、`if` の写し（後で足した step に `if` を付け忘れても、走るだけで黙って省かれはしない）。Node は setup-node を前へ移して用意する（導出が指摘した暗黙依存は生じない）
- 導出が偽と挙げた `docs/build-commands.md` の「カテゴリ A のコマンドは … PR で自動実行」「CI（rust-check）が PR ごとに通す」「`cargo doc` が自動で走るのは、CI（rust-check）だけ」: カテゴリ A は `.rs` を変えたときの検証で、その PR では従来どおり走るので真のまま。変更しない
- 導出が挙げた `.claude/hooks/pre-bash.test.mjs` の「CI は … 両方で走る」: そのファイルを変える PR は `.md` 以外を含むので両 OS で走る。真のまま。変更しない
- 導出が挙げた `.github/labels.yml` の `skip-ci` の説明: 撤去済みの `frontend-check` を含む既存の古さで、今回の変更では偽にならない。範囲外（PR 本文に残す）
- 導出の「検証用 PR に `Cargo.toml` 等の無害な変更を 1 コミットずつ足す」: 境界は vitest（純関数と結合テスト）で固定する。workflow 側の `if` の式と CLEAN は、この PR の CI とマージ後の最初の文書だけの PR で測る（PR 本文のチェックリスト）
- plan ∖ 導出: なし（導出も同じ 4 ファイルと、許可リスト・`--no-renames -z`・merge commit の第 1 親・push は常に全部・fail-closed を独立に導いた）

### 未検証

- 非必須チェックの pending で mergeStateStatus が UNSTABLE になること・SUCCESS で全 step を飛ばした rust-check で CLEAN になること — この PR の CI とマージ後の文書だけの PR で測る（PR 本文のチェックリスト）

### 判断

- 実装着手: 可

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2b 独立導出）
- エージェント数: 1（ほかに敵対的調査 1 体）
- 要対処: 2 件（`ci.yml` の #509 コメントを変更対象へ追加・CLI の結合テストと `-z` の変異を追加）
- 未検証: CLEAN / UNSTABLE の実挙動（PR の CI でしか測れない）
- check スキル: /dry-check 相当の照合（新設関数のトリガー）— 同じ `git diff --name-only --no-renames -z` は `.claude/hooks/pre-bash.mjs` の要約照合が自前で行っているが、共有できる関数は無く、NUL 区切りの分割は数行なので新設する。ほかのトリガー（対称ペア・UI モード・永続形式）には当たらない
- 散文の全件列挙: 語 2 / 総件数 32 行 / ★未掲載 3（計画へ足した 0。`pre-bash.test.mjs`・`smoke.yml`・`snotra-core/src/lib.rs` はいずれも変更後も真） / ☆行未掲載 0（計画へ足した 0）
