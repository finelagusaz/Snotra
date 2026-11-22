# 調査: issue #1524（現在形の記述が偽になった 3 箇所）

起点: `origin/main` = `4ba639c0`（ブランチ `fix/1524-stale-present-tense`）。

## issue の要約

振り返りで見つけた、現在形で書かれ今は偽になった記述 3 件を、現在の事実と矛盾しない形（数え上げをやめ、正本を指す形）へ直す。挙動は変えない（コメント・ラベル説明のみ）。

## 関連するファイル・シンボル（現物で確認済み）

1. `snotra-core/src/search/build.rs:413` — `PrebuiltIndex::new_from_tree` の doc。「（かつてそう書いており、`RETROSPECTIVE.md` が前サイクルの取りこぼしとして記録している）」。`RETROSPECTIVE.md` は振り返りのたびに上書きされ（`AGENTS.md`「RETROSPECTIVE.md の運用」）、その記録は今は無い。
   - 同じ文の後半「条件の正本は `indexer::save_cache_sorted` の分岐である」は真（`snotra-core/src/indexer/cache.rs:753` に `fn save_cache_sorted`）。
   - 同じ注意を既に正しく書いている先例: `snotra-core/tests/path_query_cost.rs:275`「`RETROSPECTIVE.md` を参照先にしない——あれはサイクルごとに上書きされる」。
2. `src-tauri/src/egui_shell/window_coordinator.rs:81` — `read_metrics` の doc「**呼び出し元は `show_egui_main` だけである**（#749 で実測）」。実際の呼び出しは 2 つ: `:367`（`show_egui_main` 内）と `:551`（`hide_egui_main` 内・`read_metrics(app).bar_height`、#909 の `815c2a4d` で追加）。`:85-86`「本関数が `read_visual` と別に在るのは、show 経路が高さだけを要り色 parse を払わないため（`mod.rs` の `read_visual` の doc と対）」も show だけを前提にする。
   - 対の写し: `src-tauri/src/egui_shell/mod.rs:479`「**`read_metrics` は残す**（統合しない）——`show_egui_main` が show 経路で高さだけを要り、色 parse を払わせないため」。偽ではないが、理由の主語を show に限る点で同じ前提を写している。
   - `mod.rs:39-40`「`read_metrics` は同モジュール内からしか呼ばれず」は真（呼び出しは 2 つとも `window_coordinator.rs` 内）。
   - 規約: `docs/comment-guidelines.md:30`（呼び出し元の数え上げはコードの写しで禁じる）・`:41`（一意性の宣言は別扱い）。
3. `.github/labels.yml:21` — `skip-ci` の description「通常 PR CI（frontend-check / rust-check）をスキップする」。`.github/workflows/ci.yml` の job は `node-check`（旧 frontend-check の縮退・`:22` のコメント）/ `governance-check`（skip-ci ガードなし）/ `rust-check`。skip-ci の `if` を持つのは `node-check` と `rust-check`。
   - 正本: `ci.yml` の各 job の `if`、その説明は `docs/build-commands.md`「**`skip-ci` ラベルは、ジョブ単位で効く**」（`:375` 付近）。
   - `.github/workflows/label-sync.yml` が `labels.yml` の main への push で GitHub 上のラベルを同期する（マージで description が実際に変わる）。

## 同じパターンの残り（AGENTS.md「バグを見つけたら同じパターンを探す」）

- `frontend-check` の残存（`git grep -l`）: `.claude/hooks/pre-bash.test.mjs:37`「CI は ubuntu(frontend-check) と windows(rust-check) の両方で走る（#509）」— **同型の偽**（今は node-check）。`ci.yml:22`「旧 frontend-check」は経緯で真。`docs/superpowers/specs/2026-07-24-su7-flip-implementation-design.md` は書いた後に更新しない記録で対象外。
- `RETROSPECTIVE.md` を記録の所在として指すコード: `build.rs:413` だけ（`path_query_cost.rs:275` は正しい注意書き）。
- `呼び出し元は…だけ` の形（`*.rs`）: `engine.rs:60`（過去の経緯）、`indexer/cache.rs:930,1283`、`icon.rs:186`。`icon.rs:186` は真（`invalidate_icon_cache(` の製品呼び出しは `indexing.rs:227` だけ）。偽と確認できたのは `read_metrics` だけ。真の数え上げを書き換えるのは本 issue の受け入れ条件（矛盾の解消）の外。

## 再利用できる既存のパターン

- 「正本を指す」書き方: 同ファイル `build.rs:413` 後半の「条件の正本は `indexer::save_cache_sorted` の分岐である」、`window_coordinator.rs:83`「導出式とフォールバックの単一点は `Metrics::from_config`」。

## 技術的な制約

- `.claude/hooks/pre-bash.test.mjs` はセーフティネット（`.claude/rules/safety-nets.md` の `paths` に `.claude/hooks/**`）。含めるなら PR 本文冒頭に「マージは合意してから」。`.github/labels.yml` は `paths` に無い（`.github/workflows/**` のみ）。
- GitHub のラベル description は 100 文字上限。
- `.rs` の doc 変更は挙動を変えないが、`cargo build`/`clippy`（doc リンク）と `governance:check` を回す。

## 未解決の疑問

- なし（3 箇所の直し方はコードと文書から決まる。`pre-bash.test.mjs` を含めるかは「同じパターンをすべて直す」規則で決まる）。

## 敵対的調査（3b）の反映

出力: `workspace/adversarial-1524.txt`（sonnet 1 体）。測定環境: HEAD = origin/main = `4ba639c0`、未追跡は `workspace/` のみ、`git grep` と拡張子を絞らない `grep -r` の両方で同結果。

- 壊せた項目: 0。
- 壊せなかった項目: 争点 1〜5（`read_metrics` の呼び出しは 2 つ／`frontend-check` の偽は 2 箇所／`RETROSPECTIVE.md` を所在として指す偽は `build.rs:413` のみで .md 側にも無し／skip-ci ガードは `ci.yml` の node-check・rust-check のみ／`mod.rs:479` は show 前提の写し・`mod.rs:39` は真）と周辺事実（`save_cache_sorted`・`path_query_cost.rs:275`・`icon.rs:186`・safety-nets の `paths`）。
- ⚠️1（`cache.rs:930,1283` の数え上げは未計数）: **採用して自分で測った**。形式昇格（`LegacyUpgrade::Write`）を通るのは `try_load_with_stats_in`（`cache.rs:579`）で、その製品入口 `load_cache_with_stats` の製品呼び出しは `src-tauri/src/main.rs:226` の 1 つだけ（`load_or_scan_with_stats` の製品呼び出しは無し）。真なので範囲外のまま。
- ⚠️2（label-sync の実走は未確認）: 採用。マージ後に GitHub のラベル description が変わることを PR 本文に書く。実走の確認は本計画の作業項目にしない（コミット以降の事象）。
- ⚠️3（ruleset の required check 名）: 不採用。`docs/build-commands.md` が「ruleset `default` に required status checks の規則が無い・実測」と書いており、job 名を変えない本変更は触れない。
- ⚠️（ラベル description の 100 文字上限は未実測）: 採用。新しい description を 100 文字未満に収める（実害は同期失敗のみで、短く書けば測る必要が無い）。
