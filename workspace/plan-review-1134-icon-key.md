# plan-review — issue #1134

対象: `workspace/plan.md`（ブランチ `fix/error-row-icon-key`、HEAD `c6d1bb50`、**未実装**——`git status --short` は `?? workspace/` のみで `src/` に差分なし）。したがって本レビューは Phase 1〜4 のコードを実際には持たず、現行コード（修正前）をソースで読み、計画が記述する検知器・変異を手でトレースして裏取りした。**`cargo test` は走らせていない**（対象コードが未着地のため）。

## 要対処

なし。

## 軽微

なし（強いて言えば、計画自身が Phase 4-15 で「`results_view.rs` のコメント改稿が SPEC.md の見出し参照を壊さないかは書いた後にしか測れない」と自己申告済みで、対処不要）。

## 未検証

- **観点 2 の Phase 4-16/17（変異注入で実際に赤くなること）は実行していない**——対象コードが未着地のため、実装後に計画どおり `icon_key` の `is_error` 判定を戻して実測する必要がある。以下は手でのトレース（ソースコードを読んで期待値を追った）による裏取りであり、実行結果ではない。

---

## 観点 1 — crate をまたぐ公開 API の意味変更が計画外の下流を壊さないか

**壊れる下流は見つからなかった。**

- `grep -rl "SearchResult" snotra-egui-runtime/ snotra-settings/` は 0 件（実行済み）。`SearchResult` / `icon_key` の消費者は `snotra-core`（定義・テスト）と `src-tauri`（`icon_textures.rs` の 3 箇所 + `results_view.rs` の draw 経路）に閉じている。
- `is_error` の全出現を `src-tauri/src/` と `snotra-core/src/` で洗った（`grep -rn "is_error"`）。アイコンキー導出（`icon_textures.rs`・`results_view.rs:368`）以外の全箇所は launch gating（`launcher_controller.rs:239/499/659/1301/1339`）・hover 抑止（`results_view.rs:266-267/336`）・行のコンストラクタ（すべて `is_error: false` の平文リテラル）であり、`icon_key()` の返り値に依存していない。
- テストの暗黙依存を洗った。**`row()` ヘルパーが `is_error: false` を固定している箇所は影響を受けない**:
  - `snotra-core/src/ui_types.rs` の `mod tests`（`row()` は `is_error: false` 固定・`icon_key_from_path_returns_the_row_path` 等）——is_error=true のケースは 0 件、既存テストに壊れるものは無い
  - `snotra-core/src/instant.rs:1112,1136,1148,1157,1168`（`matching_results` が返す行は instant コマンドの候補で `is_error` を立てる経路が無い）——影響なし
  - `src-tauri/src/egui_shell/icon_textures.rs` の `mod tests`。`row()` ヘルパーは `is_error: false` 固定。**唯一 `is_error: true` を使う既存テストは `wanted_icon_keys_never_loads_icons_for_error_rows`（:259-288）で、これは計画が Phase 2-6 で緑を確認する対象そのもの**（撤去する `continue` の代わりに `icon_key()` の `None` で通ることを計画が明示）
  - `snotra-core/src/folder.rs:388` の `list_folder_nonexistent_dir_returns_empty` は `is_error` だけを見ており `icon_key`/アイコンには触れない——影響なし
- `SPEC.md` §3.4（アイコン）・§6.6（列挙失敗時）を実読した。§3.4 の「列挙失敗行はこの規則の対象外である。`path` が実在ディレクトリを指していても抽出せず、placeholder も描かない」は**観測される挙動の記述**であり、機構が `wanted_icon_keys` の明示 `continue` から `icon_key` の導出へ移っても文の真偽は変わらない。計画の「SPEC.md は変更しない」は妥当。

## 観点 2 — 検知器と変異注入が #1134 の経路を実際に測るか

**Phase 1 の 2 本は現行コード（修正前）で落ちる設計になっている。手でトレースした結果:**

- `snotra-core/src/ui_types.rs` の `icon_key_is_none_for_error_rows`（計画予定）: 現行 `icon_key`（`ui_types.rs:52-58`）は `is_error` を一切見ないため、`is_error: true` かつ `IconSource::FromPath`/`Explicit` の行で `Some(...)` を返す。計画のアサート（`None` 期待）は**現行コードで失敗する**——Red は成立。
- `icon_textures.rs` の連鎖検知器（(a)(b)(c)(d)）:
  - (a) `visible_icon_keys(&rows)`: 現行実装（:123-128）は `r.icon_key()` を無条件に呼ぶ。修正前は `icon_key()` が `Some(path)` を返すため、`visible_icon_keys` は当該キーを**含んでしまう**——「含まない」ことを期待するアサートは修正前に失敗する。Red 成立。
  - (b) `retain_visible` 後に `icons` が空: (a) より前提の `icons` に事前挿入したダミー値のキーが `visible` 集合に残るため `retain_visible`（:68-70）は落とさない——「空である」の期待は修正前に失敗する。Red 成立。
  - (c) 剪定を通さない別マップからの `icon_for_row`: `icon_for_row`（:132-137）は `icons.get(row.icon_key()?)`。修正前は `icon_key()` が `Some(path)` を返すのでキーが存在するマップからは `Some(&V)` を返す——「`None` である」の期待は修正前に失敗する。Red 成立。
  - (d) `Explicit` 版も同型（`icon_key()` の `Explicit` 分岐は `is_error` を見ない）——同じ理由で Red 成立。

**Phase 4-16 の変異（`icon_key` の `is_error` 判定を戻す）で 2 本とも赤に戻ることも、上と対称にトレースできる**——修正後の `icon_key` から `is_error` チェックを外せば、上の (a)〜(d) は修正前と同じ理由でそれぞれ失敗に戻る。片方だけしか赤くならない設計にはなっていない（両方とも `icon_key` の同じ 1 行の分岐に依存している）。

**Phase 4-17（既存テスト `wanted_icon_keys_never_loads_icons_for_error_rows` が導出経由で通ることの確認）も妥当**——撤去後の `wanted_icon_keys` は `let Some(key) = r.icon_key() else { continue }`（:107）で is_error 行を弾く。この 1 行が唯一のガードになるため、同じ「`icon_key` の `is_error` 判定を戻す」変異で当該テストも赤くなるはずで、16 と 17 が同一の注入で同時に観測できるという計画の記述は成立する。

**不動点（自己参照）の懸念は当たらない**——検知器 (a)(b)(c)(d) は `icons`/`rows` を検知器自身が構成した fixture として持ち、テスト対象の実装（`visible_icon_keys` / `retain_visible` / `icon_for_row`）を直接呼ぶ。検知器と被検知コードが同じ導出（`icon_key`）を共有するのは意図的かつ計画が明言する設計（#1133 の「キーの単一導出」の踏襲）であり、これは「測る対象と測る道具が同じ導出を共有し、変えても常に緑になる」不動点とは違う——**(a)〜(d) はいずれも `icon_key` の中身が変われば挙動が変わる**ことを上のトレースで確認済み（`icon_key` を経由しない裏口が無い）。

**5b「層の境界に残る隙間」の扱いは妥当**——`icon_for_row` の `None` が `draw_result_row` の `None if !result.is_error` 枝へ届くことを検知器で測らない判断は、`results_view.rs:484` が `icon_for_row` の唯一の呼び出し点であること（`grep -n "icon_for_row" src-tauri/src/egui_shell/results_view.rs` で確認済み・:281 `draw_result_row` 内の :484 のみ）で受容されており、同じ正当化のパターン（呼び出し点の一意性を根拠に検知器を置かない）は同ファイルの `icon_gate_keeps_input_idle_semantics` の設計思想（ソーステキスト検査・不動点回避の議論）とも整合する。描画層に kittest を置かない判断も同様に妥当。

## 参考: 手でトレースした一次証拠のファイル/行

- `snotra-core/src/ui_types.rs:52-58`（`icon_key` 現行実装、is_error 無視）
- `src-tauri/src/egui_shell/icon_textures.rs:96-116`（`wanted_icon_keys`）/ `:123-128`（`visible_icon_keys`）/ `:132-137`（`icon_for_row`）/ `:68-70`（`retain_visible`）/ `:259-288`（既存テスト）
- `src-tauri/src/egui_shell/results_view.rs:350-370`（`Some(tex)`/`None if !result.is_error` 分岐）/ `:484`（`icon_for_row` 唯一の呼び出し点）
- `snotra-core/src/folder.rs:214-222`（`error_result`）/ `:388`（既存テスト）
- `SPEC.md:83-97`（§3.4）
