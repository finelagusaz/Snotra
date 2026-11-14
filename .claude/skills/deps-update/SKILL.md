---
name: deps-update
description: "cargo/npm の依存をまとめて更新したいときに使用。更新 PR の作成と CI 確認まで行い、マージはしない。"
disable-model-invocation: true
argument-hint: "[cargo | npm | 空=両方]"
allowed-tools:
  - Bash(cargo *)
  - Bash(rustup *)
  - Bash(npm *)
  - Bash(git *)
  - Bash(gh *)
  - Read
  - Edit
  - Grep
  - Glob
---

cargo / npm の依存関係を一括で更新する。ローカル検証 → PR 作成 → CI がグリーンになることの確認まで行う。マージはしない（人間が判断する）。

対象: $ARGUMENTS（`cargo` / `npm` / 空=両方）

## Step 1 — 前提確認

- `git status` で作業ツリーが clean かを確認する。未コミットの変更があれば停止し、ユーザーに整理を促す
- `git fetch` して `main` を最新にする
- `main` から `chore/deps-YYYYMMDD` ブランチを作る（`YYYYMMDD` は当日の日付。`CLAUDE.md` の「`main` 直コミット禁止」を守る）

## Step 2 — 更新

- `$ARGUMENTS` の対象に応じて依存を更新する:
  - `cargo` または空: `cargo update`
  - `npm` または空: `npm update`
- 更新後に `git diff Cargo.lock` / `git diff package-lock.json` で差分を確認する。更新されたクレート・パッケージを列挙する
- 各更新を minor/patch と major に分類して記録する（major = メジャーバージョン番号が変わったもの）
- Rust の toolchain も見る（`cargo` または空のとき・#1161）。`rust-toolchain.toml` が版を固定しているので、新しい版が出ても CI は赤にならない。固定したことで、更新を知らせる合図が消えた。そこでこの手順を代わりの確認の場にする:
  - `rustup check` で最新の stable を見る
  - `rust-toolchain.toml` の `channel` と差があれば上げる。ローカルと CI はこの 1 行で揃う（rustup が両方で override し、未インストールなら自動で取得する）
  - rust-analyzer は `rust-toolchain.toml` の `components` が入れる（#1239）。手で `rustup component add` を打つ手順は撤去した。`channel` の行を変えないパッチ更新でも toolchain は入れ替わるので、手順では漏れを防げなかった。toolchain が入れ替わった後、セッションが既に LSP の起動失敗を上限まで数えていると、component が入り直してもそのセッションでは LSP は戻らない。Claude Code を再起動すれば戻る（2026-08-25 に両方向とも実測）。再起動の直後は索引の構築が終わっておらず、最初の 1 回は「見つからない」を返すことがある。これはクラッシュとは別のものなので、構築が終わるまで待ってから測り直す
  - 新しい lint が出ても、この PR では直さず別の PR へ切り出す。直すと、toolchain を上げた差分へ無関係な lint 対応が混ざる。また、切り出さないと他の PR はブロックされたままになる（2026-08-21 に #1160 / #1159 でこの分離を実際に行った）。clippy はエラーの出た crate でコンパイルを止めるので、CI のログだけでは全数が見えない。`git grep` で母集団を数える。新しい版の clippy をローカルへ入れて（`rustup toolchain install <版> --profile minimal --component clippy`・既定は変えない）、`docs/build-commands.md` カテゴリ A の clippy 行を `cargo +<版>` で走らせて検算する

## Step 3 — ローカル検証（最大5サイクル）

修復のループでは、`docs/build-commands.md` を SSOT として、カテゴリ A（Rust ファイル）を手で回す。カテゴリ C（ウィンドウ生成・ホットキー・スラッシュコマンド。smoke を含む）は、Step 4 でツールが A とあわせて回す。

カテゴリの中身の SSOT は `docs/build-commands.md`「変更後の検証チェックリスト」である。ここに写しを置かない（写しを持ったために SSOT と食い違った実例が #736 にある）。

smoke は 2 か所で走る。ローカルでは、Step 4 のツールがカテゴリ C として回す。CI では、`Smoke` workflow（`smoke.yml`）が回す。依存更新は `Cargo.lock` / `package-lock.json`（+ manifest）を変える。これらは `smoke.yml` の paths に当たるので、smoke が自動で起動する（#145 Phase 3。ラベルの付与は要らない）。通常の PR CI（`ci.yml`）では smoke は走らない。検証コマンドの文字列は `docs/build-commands.md` を参照する（二重にメンテしないため、この SKILL には書かない）。Step 2 の `cargo update` / `npm update` は検証ではなく、本スキル固有の操作である。そのため SSOT の対象外である。

検証が失敗した場合:

- エラー出力を読み、原因を特定する
- 修正し、失敗したステップから検証し直す
- clippy 警告・型エラー・軽い API 追従は、このスキルで直す

以下のいずれかに当たったら中止する:

- 実ロジックの大幅な書き換えを要する破壊的変更に当たった
- 5サイクル後もエラーが残る

中止する場合は、診断サマリー（試みた内容・残存エラー・推定根本原因）を書いて停止する。PR は作らない（Step 4・5 へ進まない）。調査用にブランチは残す。

## Step 4 — コミット & PR

- このスキルで生じた変更（`Cargo.lock`・`package-lock.json`・修復した実コード等）だけをステージする
- `chore(deps): ...` の conventional commit を作る。何を更新したかの短い要約を含める
- コミットしてから `npm run verify:tree -- run --sha HEAD --categories A,C` を回し、green を確かめる。PR 前ゲート（`.claude/hooks/pre-bash.mjs`）は、ツールの要約が無いブランチの `gh pr create` を止める（#1299）
- push して、`gh pr create` で PR を作る
- PR 本文に更新一覧を載せる。major bump は ⚠ で明示する

## Step 5 — CI ポーリング

- `gh pr checks` で CI の完了まで待つ
- コマンド自体がエラーになる場合（PR がまだ index されていない等）は、少し待って再試行する
- 依存更新は paths に当たるので、`Smoke` workflow が自動で起動する。`ci.yml` とは別の workflow だが、`gh pr checks` に両方が現れる。その完了も待つ

## Step 6 — 報告

以下を報告する:

1. 更新されたクレート・パッケージの一覧（minor/patch と major を区別する）
2. ローカル検証の結果（修復サイクルがあればその内容）
3. PR の URL
4. CI の結果。緑なら「マージ可能」、赤なら失敗したチェックとログ
5. open な Dependabot PR の吸収状況（#1226）。`gh pr list --state open` で列挙する。各 PR が狙う版に `Cargo.lock` / `package-lock.json` が既に達しているかを見て、吸収された（＝この更新で不要になった）ものと、残るものに分ける。`cargo update` は semver 互換の範囲しか上げない。そのため `0.x` の x 更新や major bump は残る側であり、別途 `Cargo.toml` の変更が要る。残る側については、上流の変更がこちらの文書の前提を壊していないかまで見る。例として、2026-09-02 の `fontdb` 0.24 は `ttf-parser` への依存をやめていた。そのため、`src-tauri/Cargo.toml` に書かれていた直接依存の根拠が偽になった（#1225）

吸収された PR を、このスキルで閉じない。Dependabot が自分で閉じることがある（2026-09-02 実測: #1225 のマージから約 2 分後に #1213 が自動でクローズされた）。閉じる主体を増やすと、どちらが閉じたのかが記録から読めなくなる。報告に留め、判断は人が行う。

いずれの場合もマージはせずに停止する。マージはユーザーが判断する。

Step 3 で検証が中止された場合（破壊的変更、または5サイクル超過）は、PR の代わりに診断サマリー（試みた内容・残存エラー・推定根本原因）を出力する。
