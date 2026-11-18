# 計画: #1451 fill_mesh の内外判定を行ごとの範囲に絞り、白テクセルの標本化を省く

## 目的

`snotra-egui-runtime/src/raster.rs` の `fill_mesh` の所要を、出力を 1 画素も変えずに縮める（`workspace/research.md`）。

## 受け入れ条件

1. 実機で、`SNOTRA_EGUI_PAINT_TRACE` の `raster_ms` を DPI 100% / 150% / 200% で、基準（origin/main）と新で取る。
   p50 / p95 が下がる。再開の条件（#628 の 16.7 ms と、本機 144 Hz の 1 フレーム 6.9 ms）に対する基準の位置も記録する
2. 出力が変わらないことを、今の実装の写しとの比較テストで固定する（ランダムなメッシュと、egui で組んだ実フレーム）
3. 余白を負に壊すと比較テストが落ちる（検知器が効く）
4. 既存のテストが通る

## 設計

`fill_mesh` のシグネチャは変えない。

- 三角形ごとに、今のとおり面積とバウンディングボックス（`min_x..max_x` / `min_y..max_y`）を求める
- 新規 `row_span(..) -> Option<(usize, usize)>`（private・純関数）: 行の画素中心 `py` について、3 つのエッジの境界
  `x* = ax + (bx-ax)(py-ay)/(by-ay)` から、条件を満たす px の半直線を出し、共通部分を `ROW_SPAN_MARGIN_PX`（= 2.0）だけ広げて、
  画素の添字 `x`（中心 `x + 0.5`）の範囲へ変換し、`min_x..max_x` で切る。`by - ay == 0.0` のエッジは範囲を狭めない。
  向きは「面積が正なら `w >= 0`、負なら `w <= 0`」を、`by - ay` の符号と合わせて決める。境界が NaN なら範囲を狭めない
  （保守側に倒す）。範囲が空なら `None`
- 範囲の中の画素では、今の内外判定・重心座標の除算・UV と色の補間・`blend_premultiplied` を一切変えずに行う
- 白テクセル: メッシュごとに 1 回、`texture` が白を返すか（幅か高さが 0、または `pixels[0] == [255; 4]`）を求める。三角形ごとに、
  3 頂点の UV がすべて `WHITE_UV`（`egui::epaint::WHITE_UV`）かを見る。両方なら、画素ごとの `texture.sample` と `modulate` を省き
  `src = color` とする（恒等であることの論証は research.md）

### 変更するファイルとシンボル

- `snotra-egui-runtime/src/raster.rs`: `fill_mesh`・新規 `row_span`・新規 `ROW_SPAN_MARGIN_PX`・新規 `samples_white`（`CpuTexture` のメソッド
  か関数。白を返すかの判定）。テストモジュールに今の `fill_mesh` の写し `fill_mesh_reference`（`#[cfg(test)]`）と、比較テスト
- `PERFORMANCE.md`: 新しい節「採用: `fill_mesh` の内外判定を行ごとの範囲に絞る（#1451）」（実機の A/B・DPI ごと）
- `snotra-egui-runtime/CLAUDE.md`: 変えない（不変条件「カバレッジ AA を持たない二値判定」は保たれる）
- `SPEC.md`: 変えない（描画結果は 1 画素も変わらない）

## 不変条件と異常系

- 出力は 1 画素も変わらない（比較テスト）。範囲の外へ出る画素を判定しないことだけが変わる
- 退化した三角形（面積 0）は今のとおり飛ばす。水平な辺（`by - ay == 0`）は範囲を狭めない。極端に細い三角形・水平に近い辺は
  境界が大きく動くが、余白と `min_x..max_x` の切り詰めで保守側に倒れる
- テクスチャが白を返さない（テキストのグリフなど）三角形は、今のとおり標本化する。UV が 1 つでも (0, 0) でなければ標本化する

## テスト方針

Red → Green: `row_span` をシグネチャだけ（常に `None`）で置き、比較テストが赤になることを見てから実装する。

- `fill_mesh_matches_reference_on_random_meshes`: xorshift でランダムな三角形を作る（両方の巻き方・水平/垂直に近い辺・細い三角形・
  画面外にはみ出す頂点・白 UV とテクスチャ UV の混在・頂点色の混在・clip 矩形・ppp 1.0 / 1.25 / 1.5 / 2.0）。Nearest と Linear の
  テクスチャ、幅 0 のテクスチャで、新旧の buffer が全画素で一致することを見る。件数は CI で数秒以内に収まる量（例 4,000 三角形 × 4 ppp）
- `fill_mesh_matches_reference_on_an_egui_frame`: `egui::Context` で結果リスト相当の UI（ラベルの 2 段・選択行の角丸の矩形・
  アイコン相当の矩形・枠線）を組み、`tessellate` したメッシュを ppp 1.0 / 1.25 / 1.5 / 2.0 で新旧に描いて全画素の一致を見る。
  フォントのテクスチャは `apply_texture_delta` で作る
- `row_span` の単体テスト: 水平な辺・垂直な辺・巻き方の両方で、真に内側の画素が範囲に入ること
- 変異注入（委譲先）: 余白を -1.5 にする（比較テストが落ちる）・白テクセルの判定で `pixels[0]` を見ずに常に真にする（テキストの
  グリフで落ちる）・`by - ay == 0` の扱いを「行を捨てる」にする

検証: カテゴリ A。カテゴリ D の目視は不要（出力が 1 画素も変わらないことを比較テストが固定する）。

### 実機の計測の手順（受け入れ条件 1）

1. 基準 = origin/main（この計画の分岐元）、新 = 実装後、の release exe を scratchpad へ別名で置く
2. 実 config を写した分離プロファイル（`SNOTRA_CONFIG_DIR`・`scripts/lib/SnotraSmoke.psm1` の `New-SnotraVerificationProfile`）で、
   `SNOTRA_EGUI_PAINT_TRACE=1` を立てて起動する。ホットキーで表示し、クエリを数種打って結果リストを出し、Escape で隠す、を
   繰り返す（入力の注入には同モジュールの `Send-SnotraKeyChord` / `Send-SnotraUnicodeText` を使う）。stderr の
   `SNOTRA_EGUI_PAINT` 行を `win` ごとに集め、`raster_ms` の p50 / p95 を出す。基準と新を交互に 2 回ずつ（A-B-A-B）
3. 表示倍率 100% で 2 を行ったあと、**ユーザーが表示倍率を 150%、200% に切り替え**、それぞれで 2 を行う。最後に 100% へ戻してもらう。
   計測の間は入力を注入するので、ユーザーには PC に触れないでもらう
4. 計測の道具は scratchpad の一時スクリプトにし、測り終えたら消す

## 作業項目

### Phase 1: 実装

- [ ] `row_span`（常に `None`）・`ROW_SPAN_MARGIN_PX`・`fill_mesh_reference`・比較テスト 2 本と `row_span` の単体テストを書き、赤を見る
- [ ] `row_span` と白テクセルの省略を実装し、緑にする
- [ ] 2 つの改良の寄与を分ける: egui で組んだ実 config 相当のフレーム（10 行・幅 600・アイコン付き）を ppp 1.0 / 1.5 / 2.0 で、
  3 版（今・範囲だけ・両方）の `fill_mesh` に通して所要を比べる（scratchpad の一時の計測。製品コードには残さない）。
  片方が効かなければ（差がぶれの中に収まれば）、その片方を外して計画とコードを直す
- [ ] snotra-egui-runtime の fmt / clippy / test を手元で通す

### Phase 2: 実測と記録

- [ ] 実機の計測（DPI 100 / 150 / 200%・基準と新）を取る
- [ ] `PERFORMANCE.md` に「採用: `fill_mesh` の内外判定を行ごとの範囲に絞る（#1451）」を書く（DPI ごとの p50 / p95・基準の再開の条件との位置・
  2 つの改良の寄与・`raster_ms` には `fill_mesh` の外の固定費も入ること・却下した逆数の乗算（#628 の指定・issue の測定で 1〜2 LSB の差）・
  測っていないもの〔144 Hz の枠を超えたときの落ちフレーム〕）

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-11-18 / 問い: "この計画で実装へ進めてよろしいですか？" / 回答: "承認する"

## セルフレビュー

- リスク: 通常（永続形式・並行性・状態遷移・ガバナンスに触れない。描画の純関数 1 つ）
- plan-review: 未実施（通常リスク）
- エージェント数: 0（ほかに 3b の敵対的調査 1 体）
- 要対処: 0 件
- 未検証: 実機の DPI 150% / 200% の所要（ユーザーが表示倍率を切り替えて測る）
- 散文の全件列挙: 未実施（通常リスク）。`git grep fill_mesh` の 6 行（CLAUDE.md・font_stack.rs・results_view.rs ほか）は、
  カバレッジ AA を持たない二値判定と関数の形を述べており、変更で偽にならない
