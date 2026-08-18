# 計画準拠レビュー — #1133（案 C: instant 行のアイコン抽出キー）

対象: `workspace/plan.md`（2026-08-18 版）。観点は依頼どおり 2 つのみ。

## 要対処（1 件）

### B-1. `wanted_icon_keys` の新設テストリストに `is_error` 除外の検証が無く、実装時に落とすと「エラー行にアイコンを描かない」という不変条件が視覚的に壊れる

**根拠（file:line）**

- 現在の抽出要求ループは `!r.is_error` を**明示的な独立条件**として持つ（`src-tauri/src/egui_shell/results_view.rs:180-186`）:
  ```rust
  if !r.is_error
      && needs_extraction(&r.path, &self.icon_textures, &self.icon_attempts)
      && !self.icon_pending.contains(&r.path)
      && !wanted.contains(&r.path)
  { wanted.push(r.path.clone()); }
  ```
- `draw_result_row` の描画分岐（`results_view.rs:352-371`）は **`Some(tex)` 側に `is_error` ガードを持たない**:
  ```rust
  match icon {
      Some(tex) => { /* 無条件に描画 */ }
      None if !result.is_error => draw_icon_fallback(...),
      None => {}
  }
  ```
  現状「エラー行にアイコンが出ない」ことを担保しているのは**唯一** `wanted` 構築時の `!r.is_error` であり、`icon_for_row`（旧 `icons.get`）側にも `visible_icon_keys` 側にも is_error は関与しない。
- `folder::error_result`（`snotra-core/src/folder.rs:211-218`）が作るエラー行は `path: dir.to_string_lossy()`——**実在するディレクトリの絶対パス**である。Phase 1 の「全構築点を `FromPath` で移行」を字面どおり適用すると、この行にも `icon: IconSource::FromPath` が付き、`icon_key()` は `Some(dir_path)` を返す。ディレクトリパスに対する `SHGetFileInfoW` は高確率で成功する（フォルダアイコンは既定で取得できる）ため、**`wanted_icon_keys` が is_error を見落とすと、フォルダ列挙失敗行に実際のフォルダアイコンが描画される**——is_error 行は「アイコン形の装飾を描かない」という明記済みの設計判断（同ファイル `results_view.rs:367` のコメント）に反する、実害のある回帰である。
- 計画の「不変条件と異常系」節は「`is_error` 行の扱い（placeholder も描かない）は変えない」と明記しているが（`plan.md:69`）、テスト方針の `wanted_icon_keys` の項（`plan.md:102`）には `Skip` / `Explicit` / `have・attempts・pending` の除外 / 重複排除は載っているのに **`is_error` 行の除外は載っていない**。`icon_textures.rs` に `is_error` を扱う既存テストも無い（grep 0 件・`needs_extraction` は `is_error` を知らない純関数）。

**推奨**: `wanted_icon_keys` のシグネチャに `is_error` を読める形を残し（`SearchResult` を渡すか `!r.is_error` を上位で filter してから渡す)、対応する単体テスト（「is_error 行は `Skip` でも `FromPath` でも `wanted` に載らない」）をテスト方針へ追加する。

## 軽微（2 件）

### A-1. 変更ファイル一覧表の `engine.rs` / `index_tree.rs` は `SearchResult` 構築点を持たない

`plan.md:24` の表は「`snotra-core/src/{engine,folder,index_tree}.rs`」を `SearchResult { .. }` 構築点の移行対象として挙げているが、`grep -n "SearchResult" snotra-core/src/engine.rs` は型シグネチャのみ（構築 0 件）、`index_tree.rs` は doc コメント中の言及 1 件のみで構築は無い。実際に構築するのは `folder.rs`（4 件）と `search.rs`（1 件）・`search/scoring.rs`（1 件）・`instant.rs`（1 件）だけである。機能上は無害（コンパイラが実際に落ちる場所を強制するので過剰記載が実装ミスを招くことはない）が、AGENTS.md「文書に事実の写しを増やす変更」の数え上げ規律に照らすと表の記載が不正確。

### A-2. 「構築点は 23 か所」という数え上げは、struct 定義行を 1 件含んだ数である

`grep -c "SearchResult {"` は確かに 23（実測: `ui_types.rs`=1, `search.rs`=1, `instant.rs`=1, `search/scoring.rs`=1, `results_view.rs`=1(テスト), `search_state.rs`=9(本番1+テスト8), `tray.rs`=2(テスト), `folder.rs`=7(本番4+テスト2、うち1件は `collect_by_keyed` の使い捨てプレースホルダ)）。ただしこのうち `ui_types.rs:16` の 1 件は `pub struct SearchResult {` という**型定義行**であり、フィールドを追加するのはこの箇所自身（定義の変更）であって「移行対象の構築点」ではない。実際にコンパイラが `icon: ...` の欠落で落ちる構築リテラルは **22 か所**。functional な影響は無い（コンパイラは実物を見るので実装は正しく進む）が、research.md F7 とアドバーサリアル記録（`adversarial-1133.txt` 内「19 件」という別の数字とも食い違う）の数え上げ自体が AGENTS.md の数え上げ規律に照らして不正確。

## 未検証（なし）

- 依頼の 2 観点（crate 境界の移行漏れ・3 読みの統合による依存破壊）に関連する主要な論点は、上記以外はすべて一次証拠（grep・ファイル読み）で確認できた。時間の都合で見送った項目は無い。

## 確認できた項目（正しいと裏取りできたもの）

- **`IconSource` の `snotra-core/src/ui_types.rs` 配置は責務として正しい**。`String` と enum のみで egui/tauri 依存を持たず、`ui_types.rs` の既存方針（表示に必要な最小の形・Win32/egui 非依存）と整合する。
- **`matching_results` への `env_expand: impl Fn(&str) -> String` 注入は crate 境界を破らない**。`expand_env`（`src-tauri/src/commands/launch.rs:226`、`pub(crate) fn expand_env(input: &str) -> String`）は `fn(&str) -> String` としてそのまま渡せる（`launch_exec_core` が既に同じ形で `expand_exec_args` へ渡している・`launch.rs:261`）。`matching_results` の呼び出し点は `launcher_controller.rs:924-933` の `read_config` クロージャ内 1 か所のみで、`expand_env` は同一クレート内 `pub(crate)` なので可視性の問題は無い。
- **移行漏れがコンパイラで捕まらない経路（`..` / 関数更新構文）は存在しない**。`snotra-core/src` と `src-tauri/src` の両方で `\.\.\w+\s*\}` パターンを grep し、`SearchResult` に限らず構造体更新構文自体が 0 件だった（実測）。`SearchResult` は `Default` を derive していないため `..Default::default()` も言語的に不可能。
- **`snotra-settings` / `snotra-egui-runtime` への波及は無い**。`SearchResult` の構築・参照ともにリポジトリ全体 grep で `snotra-core` と `src-tauri` の 8 ファイルにしか現れない。`matching_results` の呼び出し元も `launcher_controller.rs` の 1 か所のみ。
- **3 か所の読みは同じ生成経路を通ることが構造的に保証される**——`wanted`（要求）→ worker → `IconMsg::Loaded(key, ..)` → `icon_textures.insert(key, ..)` という drain（`results_view.rs:576-595`）は worker が返したキーをそのまま使うだけで独自導出を持たない（plan の主張どおり）。`icon_for_row`（旧 `icons.get`）と `visible_icon_keys`（旧 `visible` 構築）を同じ `icon_key()` 経由へ揃えれば、キーの不一致による「抽出したのに引けない」「可視集合から漏れて往復する」経路は塞がる。
- **`Skip` 行が `visible` に入らなくなること自体は、既存の `icon_textures` / `icon_pending` / `icon_attempts` エントリを奪わない**——`Skip` 行はそもそも `wanted` に載らない（新設分含む）ため、これら 3 map/set に `Skip` 行のキーが入ることが無い。可視集合からの除外は「入っていなかったものが除外される」だけで drop ではない。ただし上記 B-1 のとおり、**`is_error` 行については話が別**（`FromPath` を持つため `Some` キーが生じうる）。
- **`docs/architecture.md:112` の「path キーで stale 無害」行は plan の記載どおり実在する**（该当行を実読）。Phase 4 の更新対象として妥当。

## issue

#1133
