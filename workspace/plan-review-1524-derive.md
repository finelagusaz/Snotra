# #1524 独立導出レビュー（要求だけからの導出）

- 対象 issue: #1524（現在形で書かれ、今は偽になった記述）
- 基点: worktree `issue-1504-organization-035b58`、ブランチ `fix/1524-stale-present-tense`、HEAD 4ba639c0
- **計画を目にしたか: 目にしていない。** `workspace/plan.md`・`workspace/research.md` は読んでいない・検索していない。検索はすべて具体ディレクトリ（`snotra-core` `src-tauri` `snotra-egui-runtime` `snotra-settings` `.github` `.claude` `.githooks` `scripts` `docs` とルート個別ファイル）に限り、`':!workspace'` を付けた。
- 読んだだけ。編集・コミットはしていない（このファイルを書いたのみ）。

## 直すべき箇所（偽）

| # | ファイル:行 | シンボル | 判定 | 根拠 |
|---|---|---|---|---|
| 1 | `snotra-core/src/search/build.rs:413` | `SearchEngine::new_from_tree` の doc | **偽**（issue 項目 1） | `RETROSPECTIVE.md` に `cache-miss` / `new_from_tree` の記述は 0 件（grep rc=1）。記述の由来は #977（4e424b7c）が「cache-miss はもうここを通らない」と書き、#990（f1827b08）が直したとき今の文を足した |
| 2 | `src-tauri/src/egui_shell/window_coordinator.rs:81` | `read_metrics` の doc | **偽**（issue 項目 2） | 呼び出しは `window_coordinator.rs:367`（`show_egui_main` 内）と `:551`（`hide_egui_main` 内）の 2 か所 |
| 2b | `src-tauri/src/egui_shell/window_coordinator.rs:85` | 同 doc の続き「本関数が `read_visual` と別に在るのは、show 経路が高さだけを要り」 | **不完全（⚠️ 偽寄り）** | hide 経路も `bar_height` だけを要る（`:551`）。理由づけとしては今も成り立つが、show 経路だけを名指している |
| 2c | `src-tauri/src/egui_shell/mod.rs:479-480` | `read_visual` の doc「`read_metrics` は残す——`show_egui_main` が show 経路で高さだけを要り」 | **不完全（⚠️）** | 2b と同じ前提の写し（`read_metrics` の doc に「`mod.rs` の `read_visual` の doc と対」とある対の片割れ）。hide も同じ理由で使う |
| 3 | `.github/labels.yml:21` | label `skip-ci` の description | **偽**（issue 項目 3） | `.github/workflows/ci.yml` の job は `node-check` / `governance-check` / `rust-check`。`frontend-check` は無い（`ci.yml:22` に「旧 frontend-check の縮退」）。skip-ci の `if` を持つのは `node-check`（`ci.yml:27-29`）と `rust-check`（`:78-80`） |
| 3b | `.claude/hooks/pre-bash.test.mjs:37` | ファイル先頭のコメント「CI は ubuntu(frontend-check) と windows(rust-check) の両方で走る（#509）」 | **偽**（同じ消えた名前） | ubuntu 側の job は `node-check`（`ci.yml:26`）。**セーフティネットのパス（`.claude/hooks/**`）なので、マージはユーザー合意が要る** |

## 文案

### 1. `build.rs:413`（`new_from_tree`）

```rust
    /// **「cache-miss はもうここを通らない」と無条件に書いてはならない**（#977 がそう書き、#990 で直した）。保存側が派生データを返さなかった cache-miss は今もここへ来る——条件の正本は `indexer::save_cache_sorted` の分岐である。
```

- 消えた記録への参照をやめ、`docs/comment-guidelines.md`「歴史メモの様式」（経緯は `#NNN` で辿れる形）に合わせる。簡潔にするなら同ファイル `:400` と同じ「（かつてそう書いていた）」でもよい。
- 正本の指名（`save_cache_sorted` の分岐）は真: `snotra-core/src/indexer/cache.rs:753-758` で `Config::config_dir()` が `None` のとき `IndexMaterial::from_tree`（派生データ無し）を返す。同 doc（`:749`）が「この分岐がこの性質の正本である」と自称しており一致。

### 2. `window_coordinator.rs:78-88`（`read_metrics`）

```rust
/// 実行中 config から Metrics を導出する(#646)。毎フレーム/毎 show の live-read で
/// キャッシュしない。
///
/// view は `read_visual` → `visual::visual_snapshot` 経由で同じ `layout::Metrics::from_config`
/// に到達するため、**導出式とフォールバックの単一点は `Metrics::from_config` であって本関数ではない**
/// (#646: 独立実装 2 箇所でフォールバックが 52.0/43.0 に乖離した)。
/// 本関数が `read_visual` と別に在るのは、フレームの外（`update()` の外）の読みが高さだけを要り、
/// 色 parse を払わないため（`mod.rs` の `read_visual` の doc と対）。
/// AppState 不在(setup 完了前の理論経路のみ)は `visual::default_visual()` から導出——
/// 理由は本体のコメントに置く。
```

- 「呼び出し元は `show_egui_main` だけである（#749 で実測）」の 1 文を削る（`docs/comment-guidelines.md:30` が禁じる「呼び出し元」の数え上げ。`:41` の「一意性の宣言」には当たらない——崩れても事故にならない）。
- 「show 経路」を「フレームの外の読み」へ。呼び出し元を名指さず性質で書く（show / hide と並べると数え上げに戻る）。

### 2c. `mod.rs:479-480`（`read_visual`）

```rust
/// **`read_metrics` は残す**（統合しない）——フレームの外の読みが高さだけを要り、
/// 色 parse を払わせないため。両者とも `Metrics::from_config` を正本とするので導出は 1 つ。
```

### 3. `.github/labels.yml:21`

```yaml
  description: skip-safe な PR の CI をスキップする（対象 job は ci.yml の if・可否は docs/build-commands.md）
```

- job 名を数え上げず、正本（`ci.yml` の `if` と `docs/build-commands.md`「`skip-ci` ラベルは、ジョブ単位で効く」「`skip-ci` を貼ってよいのは、skip-safe な変更だけである」）を指す。
- 旧文の「通常 PR CI」は governance-check（skip されない）を含むようにも読めたので、それも解消する。
- ⚠️ GitHub のラベル description は 100 文字上限。上の文案は約 60 文字で収まる見込みだが、label-sync が通るかは CI で確認のこと。

### 3b. `.claude/hooks/pre-bash.test.mjs:37`

```js
// CI は ubuntu と windows の両方で npm test を走らせる（#509・job は ci.yml）。どちらでも
// 落ちないよう、OS 依存のリテラルパスを書かず path.join / tmpdir から組み立てる。
```

- job 名を消せば次の改名でも古くならない。最小差分を好むなら `frontend-check` → `node-check` の置換でもよい。

## 同じパターンを探して「真」と判定した箇所（直さない）

### 消えた記録（`RETROSPECTIVE.md` / 前サイクル）への参照
- 検索: `RETROSPECTIVE` と `前サイクル|振り返り(が|で|の記録)|retrospective` を全対象ディレクトリで。コード中の該当は `build.rs:413` だけ。
- `snotra-core/tests/path_query_cost.rs:275` —「`RETROSPECTIVE.md` を参照先にしない」は上書きの性質を述べる規律で**真**。
- `.claude/rules/governance-docs.md:32` — #1222 当時の行為（過去形）の記述で**真**。
- `.claude/skills/retrospective/SKILL.md`・`AGENTS.md:83`・`CLAUDE.md:53` — 上書き運用そのものの記述で**真**。

### 消えた名前 `frontend-check`
- `.github/workflows/ci.yml:22`「旧 frontend-check の縮退」— 経緯の記述で**真**。
- `docs/superpowers/specs/2026-07-24-su7-flip-implementation-design.md:69,78` — 凍結記録なので対象外（当時の記述として真）。
- `docs/build-commands.md:352-385` の skip-ci 説明は `node-check` / `rust-check` / `governance-check` で書かれており**真**。`scripts/governance/checks/G-module-index.mjs:32`「`skip-ci` ラベルの付いた PR では走らない」も **真**（`npm test` は node-check / rust-check のみ）。
- 他の workflow（`smoke.yml` / `release.yml` / `label-sync.yml` / `create-release.yml`）は skip-ci を参照しない。

### 呼び出し元の数え上げ（同じ禁じられた形）— すべて現時点で真
検索語: `呼び出し元は…(だけ|のみ|1 つ|唯一)` / `唯一の呼び出し元` / `から(だけ|のみ)呼ばれ` / `only caller` など（`snotra-core` `src-tauri` `snotra-egui-runtime` `snotra-settings` `scripts` `.claude` `.githooks`）。

| ファイル:行 | 記述 | 実測 | 判定 |
|---|---|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs:268` | `position_on_target_monitor`: "`show_egui_main` is the only caller." | 呼び出しは `:397`（`show_egui_main` 内）のみ | 真。ただし同じ禁じられた形（⚠️ 直すかは判断に委ねる） |
| `src-tauri/src/egui_shell/window_coordinator.rs:593` | `trim_idle_working_set` について「main の hide 経路の合流点＝ここが唯一の呼び出し元」 | `src-tauri/src` 中の呼び出しは `:596` のみ | 真。合流点の不変条件として読める（`:41` の一意性の宣言寄り） |
| `snotra-core/src/indexer/columns.rs:206` | `collapse_lower_pair`:「唯一の呼び出し元は `derive_entry_collapsed`」 | 呼び出しは `:200` のみ | 真。直後に「別実装で書き起こすとずれる」とあり、一意性の宣言寄り |
| `snotra-core/src/search/path_store.rs:496-497` | `with_cursor`:「`with_normalized_key` が唯一の呼び出し元で、`score_one_entry` と `recent_history` が共有」 | `with_cursor` 呼び出しは `scoring.rs:142` のみ。`with_normalized_key` は `scoring.rs:588`・`search.rs:478` | 真（⚠️ 後半の 2 消費者の列挙は数え上げ） |
| `src-tauri/src/egui_shell/window_coordinator.rs:107-108` | `read_window_width`:「ウィンドウ生成後の 2 経路（`show_egui_main` と `view.rs` の `window_width`）が共有する唯一の実装」 | 呼び出しは `window_coordinator.rs:362`・`view.rs:147` | 真（⚠️ 経路の数え上げの形） |
| `src-tauri/src/egui_shell/mod.rs:38-42` | re-export しないもの（`read_metrics` 等）は同モジュール内からしか呼ばれない | `read_metrics` の呼び出しは 2 か所とも `window_coordinator.rs` 内 | 真 |
| `src-tauri/src/egui_shell/window_coordinator.rs:236` / `visual.rs:10,87` | 「`read_metrics` と同方針」 | 方針（色 parse を払わない／既定の正本）への言及 | 真 |
| `snotra-core/src/engine.rs:60` | 「その呼び出し元は 1 つも存在しなかった」 | 過去形の経緯 | 真（対象外） |

## 確信の持てない所見（⚠️）

- ⚠️ 2b・2c は「偽」ではなく「理由の主語が狭い」。issue の受け入れ条件（数え上げをやめる）を `read_metrics` の doc だけに当てるなら 2 の 1 文削除で足りるが、対の片割れ（`mod.rs:479`）が同じ「`show_egui_main` が要る」を写しているので、片側だけ直すと対の記述が非対称になる。
- ⚠️ 1 の文案の `#977` / `#990` の帰属は `git log -S"もうここを通らない"` で確かめた（4e424b7c が追加、f1827b08 が削除して現文を追加）。
- ⚠️ `build.rs:413` の正本指名 `indexer::save_cache_sorted` は、実体が `indexer/cache.rs` の private fn で `indexer` から re-export されていない。ただし `engine.rs:178`・`query.rs:139` も同じ `indexer::` 前置きの慣行で書いており、#1524 の対象外と判断。
- ⚠️ 表「呼び出し元の数え上げ」の真の 5 件は、`docs/comment-guidelines.md:30` の禁じる形に当たりうる（`:41` の一意性の宣言として読めるものもある）。issue の受け入れ条件は 3 箇所なので、範囲を広げるかは判断に委ねる。検索は語句ベースで、言い回しの違う数え上げ（「〜からしか呼ばれない」等）は漏れうる。
- ⚠️ `pre-bash.test.mjs`（3b）はセーフティネットのパス。ルート `CLAUDE.md` 最重要ルール 2 により、PR 本文冒頭に「マージは合意してから」が要る。`.github/labels.yml` はセーフティネットの `paths` に含まれない（`.github/workflows/**` のみ）。
