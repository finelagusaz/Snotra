# 計画: issue #1524（現在形の記述が偽になった箇所を直す）

調査: `workspace/research.md`。起点 `origin/main` = `4ba639c0`。

## 目的と受け入れ条件

- 目的: 現在形で書かれ今は偽の記述を、現在の事実と矛盾しない形（数え上げをやめ、正本を指す形）へ直す。挙動は変えない。
- 受け入れ条件:
  1. issue の 3 箇所（`build.rs` の `new_from_tree` の doc・`window_coordinator.rs` の `read_metrics` の doc・`labels.yml` の `skip-ci` の description）が現在の事実と矛盾しない。
  2. 同じパターンの偽（`pre-bash.test.mjs` の `frontend-check`、`mod.rs` の `read_visual` の doc の show 前提の写し）も同じ変更で直っている（`AGENTS.md`「開発ワークフロー」の「同じパターンを持つコードパスをすべて探す」）。
  3. 検証: `git grep -n "frontend-check"` の残りが `ci.yml` の「旧 frontend-check」（経緯）と `docs/superpowers/` だけ／`git grep -n "RETROSPECTIVE" -- '*.rs'` が `path_query_cost.rs` の注意書きだけ／`git grep -n "show_egui_main\` だけ"` が 0 件。

## 変更するファイル

1. `snotra-core/src/search/build.rs` — `PrebuiltIndex::new_from_tree` の doc。「（かつてそう書いており、`RETROSPECTIVE.md` が前サイクルの取りこぼしとして記録している）」を「（かつてそう書いていた）」へ替える（同ファイル `from_material` の doc と同じ形）。後半の「条件の正本は `indexer::save_cache_sorted` の分岐である」は残す。
2. `src-tauri/src/egui_shell/window_coordinator.rs` — `read_metrics` の doc。
   - 「**呼び出し元は `show_egui_main` だけである**（#749 で実測）。」を削り、次の「view は `read_visual` → …」から段落を始める。
   - 「本関数が `read_visual` と別に在るのは、show 経路が高さだけを要り色 parse を払わないため」を「本関数が `read_visual` と別に在るのは、高さだけを要る経路に色 parse を払わせないため」へ替える（呼び出し元を列挙しない）。
3. `src-tauri/src/egui_shell/mod.rs` — `read_visual` の doc の「**`read_metrics` は残す**（統合しない）——`show_egui_main` が show 経路で高さだけを要り、色 parse を払わせないため。」を「**`read_metrics` は残す**（統合しない）——高さだけを要る経路に色 parse を払わせないため。」へ替える。
4. `.github/labels.yml` — `skip-ci` の description を「PR CI の一部の job をスキップする（対象の正本は ci.yml の各 job の if）」へ替える（100 文字未満。job 名を列挙しない）。
5. `.claude/hooks/pre-bash.test.mjs` — 37 行目付近のコメント「CI は ubuntu(frontend-check) と windows(rust-check) の両方で走る（#509）。」を「CI は ubuntu と windows の両方でこのテストを走らせる（#509。job は `.github/workflows/ci.yml`）。」へ替える。**セーフティネット**（`.claude/hooks/**`）なので、PR 本文の冒頭に「マージは合意してから」と書く。

`SPEC.md` は変えない。

## 実装の順序

1. 上の 5 ファイルを編集する（相互に依存しない）。
2. 訂正した段落を diff ではなく現物で読み直す（`.claude/rules/governance-docs.md`「条項を足す・消す・移すとき」）。コードスパンが行をまたいでいないこと（`docs/comment-guidelines.md`「日本語の折返し」）。
3. 検証を走らせる。

## 不変条件と異常系

- 挙動を変えない: 変更はコメント・doc・YAML の description 文字列だけ。`labels.yml` の `name` / `color` を変えない（label-sync が SSOT 外ラベルを削除するため、`name` を変えると既存ラベルが消える）。
- doc の intra-doc link を増やさない・壊さない（`cargo doc` で検知）。
- マージ後の副作用: `label-sync.yml` が GitHub 上の `skip-ci` ラベルの description を書き換える（意図どおり）。

## テスト方針と検証コマンド

- 挙動不変のため新規テストは書かない（Red を作れる対象が無い）。検知は既存の検査で行う。
- `npm run verify:tree`（`docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A の fmt/check/clippy/test/doc と、E・F を回す）。`.claude/hooks/` の変更は `npm test` が対象にする（verify:tree に含まれなければ `--also "npm test"` で足す）。
- `npm run governance:check`。
- 受け入れ条件 3 の `git grep` 3 本。

## SPEC.md・関連文書の更新の要否

- 不要（記述を事実へ合わせるだけで、仕様・アーキテクチャ・規範は変えない）。

## 作業項目

### フェーズ 1: 編集

- [x] `build.rs` の `new_from_tree` の doc を直す
- [x] `window_coordinator.rs` の `read_metrics` の doc を直す
- [x] `mod.rs` の `read_visual` の doc を直す
- [x] `labels.yml` の `skip-ci` の description を直す
- [x] `pre-bash.test.mjs` のコメントを直す

### フェーズ 2: 検証

- [x] 訂正した段落を現物で読み直す
- [x] `npm run verify:tree`（と必要なら `npm test`）が緑（R2 `a7868306` で A・E・F green。`npm test` は E が回した）
- [x] `npm run governance:check` が緑（手元・13 件 passed）
- [x] 受け入れ条件 3 の `git grep` 3 本の結果が期待どおり
- [x] （計画外・レビュー指摘）`read_metrics` の doc 冒頭「毎フレーム/毎 show の live-read」も同じ前提の偽（毎フレーム呼ぶ箇所は #673 以降無く、hide からも呼ぶ）。「呼ぶたびに live-read し、キャッシュしない」へ直す

### ラウンド記録

- R1: sha `91c4e394` / verify:tree green（A・E・F 全 exit 0） / 委譲レビュー High 1（上の計画外項目・採って修正）・Low⚠️ 1（`pre-bash.test.mjs` の「両方で走らせる」が skip-ci 等の例外を書かない。旧文と同じ形で、このファイルを変える PR は例外に当たらない。挙動を変えないので捨てる） / 変異注入: 該当なし（実行できる検知器の述語・入力に触れない。変えた文言を読む検査は無い・`git grep` 確認） / 実地の確かめ: 該当なし（手順を述べない。CI の挙動を述べる 2 文は `ci.yml` と照合、`npx vitest list --filesOnly` に `pre-bash.test.mjs` が出ることを実測）
- R2: sha `a7868306` / verify:tree green（A・E・F 全 exit 0） / 委譲レビュー 0 件（High 1 の解消を現物で確認）・⚠️ 1（R1 記録の「実地の確かめ: 該当なし」は見出しの語が不正確で、正しくは「CI の挙動を述べる 2 文を照合・vitest 収集を実測・CI 実走と label-sync はマージ後の事象で未確認」。挙動を変えないので捨てる） / 変異注入: 該当なし（同上） / 実地の確かめ: 上記のとおり
- 軸の違うレビュー（3c）: 該当なし — (1) 採らなかったのは挙動を変えない Low⚠️ だけで、委譲先も R2 で異論なしと返した (2) 安全機構・回復機構を足していない (3) 規範・ADR の前提を変えていない

## 未確定（実装前に潰す）

- [x] `pre-bash.test.mjs`（セーフティネット）を範囲に含めるか — `AGENTS.md`「開発ワークフロー」の「バグを見つけたら、同じパターンを持つコードパスをすべて探す」で決まる。含める。セーフティネットの変更はマージ前の合意が要る（ルート `CLAUDE.md` 最重要ルール 2）ので、PR 本文冒頭に明記する。
- [x] 他の「呼び出し元は…だけ」の数え上げ（`icon.rs:186`・`indexer/cache.rs:930,1283`）を直すか — 測った結果どれも真（`invalidate_icon_cache(` の製品呼び出しは `indexing.rs:227` だけ、形式昇格の製品入口 `load_cache_with_stats` は `main.rs:226` だけ）。受け入れ条件は「矛盾の解消」なので範囲外。

## 承認

- [x] 自己承認 — 2026-11-22 / 仕様変更: なし — `SPEC.md` を変更ファイルに含まない（コメント・ラベル説明を事実へ合わせるだけ） / 戻せない変更: なし — 永続形式・識別子/キー形式に触れない（`labels.yml` の `name` は変えない） / 人の裁定: なし

## plan-review 結果

- リスク: 高（`.claude/hooks/pre-bash.test.mjs` と CI のラベル定義に触れる。中身はコメントと説明文だけ）
- レビュー方式: 独立導出1体（Step 2b。成果物 `workspace/plan-review-1524-derive.md`。計画・調査は目にしていないと明記）
- エージェント数: 1

### 要対処
- なし。導出 ∖ plan = 空、plan ∖ 導出 = 空（両者とも `build.rs`・`window_coordinator.rs`（:81 の削除と :85 の show 前提）・`mod.rs:479`・`labels.yml`・`pre-bash.test.mjs`）。

### 軽微
- 文案の違い（`labels.yml` の description、`read_metrics` の :85 を「フレームの外の読み」と書く案）: 計画の文案を採る。どちらも job 名・呼び出し元を列挙しない点で同じで、計画案のほうが短く、主張を増やさない（「フレームの外」は新しい事実の主張になり、検算が要る）。
- 導出が挙げた真の数え上げ 5 件（`window_coordinator.rs` の `position_on_target_monitor`・`trim_idle_working_set`・`read_window_width`、`indexer/columns.rs` の `collapse_lower_pair`、`search/path_store.rs` の `with_cursor`）: 真なので範囲外（受け入れ条件は矛盾の解消）。
- `build.rs:413` の `indexer::save_cache_sorted` は private fn を指す書き方: `engine.rs` も同じ書き方で、偽ではない。範囲外。

### 未検証
- GitHub のラベル description の 100 文字上限は未実測。計画の文案は 40 字程度で、上限に近づかない。

### 判断
- 実装着手: 可

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体（Step 2b）
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 0 件
- 未検証: ラベル description の文字数上限（上記）
- check スキル: なし（`AGENTS.md`「条件別チェック」の該当は、セーフティネットの行（`.claude/rules/safety-nets.md` を読んだ。機構を足さないので「壊れたとき緑のまま推移するか」は該当しない）と、ガバナンス文書の行に準じる `npm run governance:check` の実行（作業項目に置いた））
- 散文の全件列挙: 語 4 / 総件数 80 行 / ★未掲載 4（計画へ足した 0：`G-module-index.mjs:32`・`path_query_cost.rs:275`・`visual.rs:10,87`・`retrospective/SKILL.md` はいずれも真） / ☆行未掲載 0（計画へ足した 0）
