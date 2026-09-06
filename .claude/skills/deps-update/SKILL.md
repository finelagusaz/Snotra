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

cargo / npm の依存関係を一括更新し、ローカル検証 → PR 作成 → CI グリーン確認まで行う。マージはしない（人間が判断する）。

対象: $ARGUMENTS（`cargo` / `npm` / 空=両方）

## Step 1 — 前提確認

- `git status` で作業ツリーが clean か確認する。未コミットの変更がある場合、停止してユーザーに整理を促す
- `git fetch` して `main` を最新化する
- `main` から `chore/deps-YYYYMMDD` ブランチを作成する（`YYYYMMDD` は当日の日付。`CLAUDE.md` の「`main` 直コミット禁止」を遵守）

## Step 2 — 更新

- `$ARGUMENTS` の対象に応じて依存を更新する:
  - `cargo` または空: `cargo update`
  - `npm` または空: `npm update`
- 更新後に `git diff Cargo.lock` / `git diff package-lock.json` で差分を確認し、更新されたクレート・パッケージを列挙する
- 各更新を **minor/patch と major に分類**して記録する（major = メジャーバージョン番号が変わったもの）
- **Rust の toolchain も見る**（`cargo` または空のとき・#1161）。`rust-toolchain.toml` が版を固定して
  いるため、**CI の赤はもう更新を報せない**——固定はその合図を消す変更だった。ここが代わりの契機である:
  - `rustup check` で最新 stable を見る
  - `rust-toolchain.toml` の `channel` と差があれば上げる。**ローカルと CI はこの 1 行で揃う**
    （rustup が両方で override し、未インストールなら自動取得する）
  - **rust-analyzer は `rust-toolchain.toml` の `components` が保証する**（#1239。手で
    `rustup component add` を打つ手順は撤去した——`channel` の行を変えないパッチ更新でも toolchain は
    入れ替わり、手順では覆えなかった）。toolchain が入れ替わった後は、**セッションが既に LSP の
    起動失敗を数え切っていると component が入り直してもそのセッションでは復帰しない。Claude Code を
    再起動すれば戻る**（2026-08-25 に両方向とも実測）。再起動の直後は索引が冷えており、最初の 1 回は
    「見つからない」を返しうる——クラッシュとは別物なので、温まるまで待って測り直す
  - **新しい lint が出ても、この PR では直さず別 PR へ切り出す。** toolchain を上げた差分へ無関係な
    lint 対応が紛れるうえ、**切り出さないと他の PR はブロックされたままになる**（2026-08-21 に
    #1160 / #1159 でこの分離を実際に行った）。**clippy はエラーの出た crate でコンパイルを止めるので、
    CI のログだけでは全数が見えない**——`git grep` で母集団を数え、新しい版の clippy をローカルへ
    入れて（`rustup toolchain install <版> --profile minimal --component clippy`・既定は変えない）、
    `docs/build-commands.md` カテゴリ A の clippy 行を `cargo +<版>` で走らせて検算する

## Step 3 — ローカル検証（最大5サイクル）

`docs/build-commands.md` を SSOT として参照し、以下のカテゴリを実行する:

- カテゴリ A（Rust ファイル）: 必須
- カテゴリ B（TypeScript ファイル）: 必須
- カテゴリ C（ウィンドウ生成・ホットキー・スラッシュコマンド）: 依存更新が触れた場合のみ

カテゴリの中身は `docs/build-commands.md`「変更後の検証チェックリスト」が SSOT である。**ここに写しを置かない**——写しを持った結果 SSOT と食い違った実例が #736 にある。

E2E・スモークテストはローカルで実行せず `Smoke` workflow（`smoke.yml`）に委ねる。依存更新は `Cargo.lock` / `package-lock.json`（+ manifest）を変えるため、これらは `smoke.yml` の paths に該当し **smoke/E2E が自動起動**する（#145 Phase 3。ラベル付与は不要）。**通常 PR CI（`ci.yml`）では smoke/E2E は走らない**。**検証コマンドの文字列**は `docs/build-commands.md` を参照する（二重メンテを避けるためこの SKILL に書かない）。Step 2 の `cargo update` / `npm update` は検証ではなく本スキル固有の操作であり、SSOT の対象外である。

検証が失敗した場合:

- エラー出力を読み、原因を特定する
- 修正し、失敗したステップから再検証する
- clippy 警告・型エラー・軽い API 追従はこのスキルで直す

以下のいずれかに当たったら**中止する**:

- **実ロジックの大幅な書き換えを要する破壊的変更**に当たった
- 5サイクル後もエラーが残る

中止する場合は、診断サマリー（試みた内容・残存エラー・推定根本原因）を書いて停止する。**PR は作らない（Step 4・5 へ進まない）。調査用にブランチは残す。**

## Step 4 — コミット & PR

- このスキルで生じた変更（`Cargo.lock`・`package-lock.json`・修復した実コード等）のみをステージする
- `chore(deps): ...` の conventional commit を作成する。何を更新したかの簡潔な要約を含める
- push して `gh pr create` で PR を作成する
- PR 本文に更新一覧を載せる。**major bump は ⚠ で明示する**

## Step 5 — CI ポーリング

- `gh pr checks` で CI の完了まで待機する
- コマンド自体がエラーになる場合（PR がまだ index されていない等）は少し待って再試行する
- 依存更新は paths 該当ゆえ `Smoke` workflow が自動起動する。`ci.yml` とは別 workflow だが `gh pr checks` に両方が現れるため、その完了も待つ

## Step 6 — 報告

以下を報告する:

1. 更新されたクレート・パッケージ一覧（minor/patch と major を区別）
2. ローカル検証の結果（修復サイクルがあればその内容）
3. PR の URL
4. CI の結果 — 緑なら「マージ可能」、赤なら失敗したチェックとログ
5. **open な Dependabot PR の吸収状況**（#1226）。`gh pr list --state open` で列挙し、各 PR が狙う版に `Cargo.lock` / `package-lock.json` が既に達しているかを見て、**吸収された（＝空振りになった）もの**と**残るもの**に分ける。**`cargo update` は semver 互換の範囲しか上げない**ので、`0.x` の x 更新や major bump は残る側であり、`Cargo.toml` の変更が別途要る。**残る側は、上流の変更がこちらの文書の前提を壊していないかまで見る**——2026-09-02 の `fontdb` 0.24 は `ttf-parser` への依存をやめており、`src-tauri/Cargo.toml` に書かれていた直接依存の根拠が偽になった（#1225）

**吸収された PR をこのスキルで閉じない。** Dependabot が自分で閉じることがあり（2026-09-02 実測: #1225 のマージから約 2 分後に #1213 が自動クローズ）、閉じる主体を増やすと**どちらが閉じたのかが記録から読めなくなる**。報告に留め、判断は人が行う。

**いずれの場合もマージはせず停止する。** マージはユーザーが判断する。

Step 3 で検証が中止された場合（破壊的変更、または5サイクル超過）は、PR の代わりに診断サマリー（試みた内容・残存エラー・推定根本原因）を出力する。
