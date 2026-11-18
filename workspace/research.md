# 調査: #1451 fill_mesh が三角形の外側の画素も判定し、白テクセルも双線形補間している

## issue の要約

`snotra-egui-runtime/src/raster.rs` の `fill_mesh` は、三角形ごとにバウンディングボックスの全画素で内外判定（エッジ関数 3 つ）を
行う。単色の図形も `WHITE_UV` のテクセルを標本化してから変調する。出力を 1 画素も変えない改良が 2 つある:

1. 行ごとにエッジの式から保守的な x の範囲（余白 2 px）を求め、その中だけで今の内外判定をそのまま行う
2. 3 つの UV がすべて `WHITE_UV` で、そのテクセルが白なら、標本化と変調を省く（恒等なので）

issue の試作（Linux VM・ベンチ）は 1.25〜1.45 倍（DPI 100〜200%）。受け入れは次の 3 つ:

- 先に実機で `SNOTRA_EGUI_PAINT_TRACE` の `raster_ms` を DPI 100% / 150% / 200% で取る（再開の条件の確認）
- 出力が変わらないことを、今の実装との比較テスト（ランダムなメッシュと実フレーム）で固定する
- `raster_ms` の p50 / p95 が下がる

## 関連するファイル・シンボル

- `snotra-egui-runtime/src/raster.rs`
  - `fill_mesh(buffer, width, height, vertices, indices, texture, clip_min, clip_max, pixels_per_point)`: 三角形ごとに
    `edge` で面積、バウンディングボックス（`clip` と画面で切る）、全画素で `w0/w1/w2` の符号判定 → 重心座標 `w/area` →
    UV と色の補間 → `modulate(color, texture.sample(u, v))` → 全 0 なら飛ばす → `blend_premultiplied`
  - `edge`・`modulate`・`blend_premultiplied`・`CpuTexture::{sample, sample_bilinear}`・`TexFilter`
  - テスト: `raster_core_matches_soft_probe_invariants`（2 画素だけ固定）・`bilinear_interpolates_with_half_texel_and_clamps_edges` ほか
- `snotra-egui-runtime/src/renderer.rs` の `EguiRenderer::paint`: `fill_mesh` の唯一の呼び手。`SNOTRA_EGUI_PAINT_TRACE` で
  `tess_ms` / `raster_ms` / `total_ms` / `px` を出す。テクスチャが無いメッシュには `width: 0` の `CpuTexture`（`sample` は白を返す）を渡す
- `snotra-egui-runtime/CLAUDE.md`「不変条件」: 「視覚欠陥を追うときは、まず自作 `fill_mesh` がカバレッジ AA を持たないことを疑う」
- `PERFORMANCE.md`「計測と受け入れ基準」: `SNOTRA_EGUI_PAINT_TRACE` の説明
- #628 の決定（issue コメント「項目 1（`fill_mesh` の全画面塗り最適化）— 実施しない」）: 理由 3 は「単色 fast path は浮動小数の
  切り捨てで画素を変える」。今回の 2 つは重心座標の除算と色の補間を残すので、この理由に当たらない

## 出力が変わらないことの論証

- 行の範囲: 各エッジ `w_i(px) = (bx-ax)(py-ay) - (by-ay)(px-ax)` は、行を固定すると px の 1 次式である。`by-ay` が 0 でなければ
  境界 `x* = ax + (bx-ax)(py-ay)/(by-ay)` の片側だけが条件を満たす（向きは `by-ay` と面積の符号で決まる）。`by-ay` が 0 なら
  px に依らないので範囲を狭めない。3 つの半直線の共通部分を余白 2 px だけ広げ、`min_x..max_x` で切る。範囲の中では今の内外判定を
  そのまま行うので、範囲が真に内側の画素をすべて含む限り、出力は同じ。境界の計算の誤差（座標は数千 px・f32）は 2 px より桁で小さい
- 白テクセル: `WHITE_UV = (0, 0)`（epaint 0.37.0 `lib.rs`）。3 頂点の UV がすべて (0, 0) なら、補間した `u = 0·b0 + 0·b1 + 0·b2` は
  ±0 である（面積が 0 でない三角形だけを描くので `b` は有限）。`sample` は Nearest でも Linear でも `pixels[0]` をそのまま返す
  （Linear は `fx = clamp(-0.5) = 0`・`tx = 0` で `c00·1 + c10·0`）。幅 0 のテクスチャは白を返す。`pixels[0] = [255; 4]` なら
  `modulate(c, 白) = (c·255 + 127) / 255 = c`（整数除算）なので、標本化と変調を省いても `src` は同じ

## 再利用できる既存のパターン

- テスト用のランダム: crate に乱数の依存は無い（`Cargo.toml`）。テストに小さな xorshift を置く
- 実フレーム: `egui::Context` で UI を組み、`tessellate(shapes, ppp)` でメッシュを得られる（`renderer.rs` と同じ手順）。
  フォントのテクスチャは `apply_texture_delta` で `CpuTexture` にできる。egui は `default_fonts` を有効にしている（ルート `Cargo.toml`）

## 実機の前提（2026-11-18 に確かめた）

- 本機は 1920×1080・144 Hz（RTX 4060 Ti）・表示倍率 100%（`GetDpiForSystem` = 96）
- 表示倍率の変更は OS の設定であり、エージェントからは変えない。DPI 150% / 200% の計測は、ユーザーが表示倍率を切り替えて行う
- 計測の手順の前例: `SNOTRA_EGUI_PAINT_TRACE` を立てて起動し、ホットキーで表示し、クエリを打って結果リストを出す（入力の注入が要るので、
  その間はユーザーに PC に触れないでもらう）

## 技術的な制約

- `fill_mesh` の出力は 1 画素も変えない（#628 の理由 3）。比較テストの基準は今の実装の写しをテストに置く
- `snotra-egui-runtime/CLAUDE.md` の不変条件（カバレッジ AA を持たない二値判定）はそのまま
- 計測コード（`SNOTRA_EGUI_PAINT_TRACE`）は変えない

## 敵対的調査（3b）の所見と採否（`workspace/adversarial-1451.txt`）

- 壊せた:
  - 「`raster_ms` は `fill_mesh` のループだけを測る」は偽（`renderer.rs` の区間には `surface.resize`・`buffer_mut`・`buffer.fill`・
    テクスチャの引きが入る）。**採る**: 受け入れの判定は `raster_ms` のまま行い、速度比は固定費で薄まることを PERFORMANCE.md に書く
  - 実 config（`visible_rows = 10`・`window_width = 600`・`show_icons = true`）は issue のベンチ（8 行・640 pt）と違う。アイコンの
    テクスチャは白 UV ではない。**採る**: 効果の額は実機（実 config を写した分離プロファイル）の計測で言い、issue の 1.25〜1.45 は引用しない
  - 本機の 100% では、G3(b) の p95 4.68 ms は 144 Hz の 1 フレーム（6.9 ms）に収まる。**採る**: 再開の条件は 150% / 200% の実測で確かめる
- 壊せなかった: 行の範囲（余白 2 px）で出力が変わらない（f32 で 80 万三角形・約 61 億画素を、敵対的な近水平の辺と大きな座標を含めて
  突き合わせ、漏れ 0）・白 UV の省略は恒等（epaint 0.37.0 の `TextureAtlas::new` は (0, 0) に白を書く）・#628 の理由 3 は当たらない・
  `fill_mesh` の呼び手は 1 か所
- ⚠️ を採る:
  - 2 つの改良の寄与が分かれていない → 実装中に、egui で組んだフレームで 3 版（今・範囲だけ・両方）の所要を一時の計測で比べ、
    PERFORMANCE.md に書く。片方が効かなければ、その片方を入れない
  - #628 は「まず逆数の乗算だけを入れて測り直す」と指定していた。issue はそれを測ったうえで、1〜2 LSB の差が出るので採らなかった
    → PERFORMANCE.md に却下として書く
  - 「乱数の依存が無い」「`default_fonts` が有効」→ 確認済み（`snotra-egui-runtime/Cargo.toml` に乱数の依存は無い。ルート
    `Cargo.toml` は `egui = { version = "=0.37.0", features = ["default_fonts"] }`）
- ⚠️ を採らない: 144 Hz の枠を超えると落ちフレームになるか（REPAINT_TRACE が要る）→ この issue では測らない。PERFORMANCE.md の
  「測っていないもの」に残す

## 未解決の疑問

- 実機で DPI 100 / 150 / 200% の `raster_ms` はいくつか（再開の条件。G3(b) は既定 config の 1 点だけ）
- 144 Hz の 1 フレーム（6.9 ms）を基準に効果を言うか、G3(b) の 16.7 ms を基準にするか
