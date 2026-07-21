# SU1 設計 — snotra-egui-runtime の softbuffer 置換（#532 Phase 2）

- 種別: サブユニット設計（spec）。実装計画は本 spec 承認後に別途 writing-plans で作る
- 日付: 2026-07-21
- 親: `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md`（SU1）／#532
- 履歴: 初版を 3 レンズ敵対的レビュー（描画正当性・境界/API・不変条件）で硬化。反映点は各節に織り込み済み

## 目的

`snotra-egui-runtime` の描画基盤を wgpu → softbuffer + CPU ラスタライズ（`fill_mesh`）へ **in-place 置換**する。`EguiRuntime`/`EguiView` の公開 API と tao + tauri-runtime-wry プラグイン統合を保ち、`EguiView` を差し替えるだけで任意 UI を softbuffer で描けるクレートにする。SU2 以降（製品メインウィンドウ）の描画基盤となる。

## 決定の要石

### 検証済み（この設計の前提）

1. **softbuffer は tao/wry 管理の `tauri::Window` にバインドできる（見込み）。** `Cargo.lock` の `raw-window-handle` は 0.6.2 が単一定義（他は依存からの参照）。softbuffer 0.4.6 は rwh 0.6 の `HasWindowHandle`/`HasDisplayHandle` を要求し、`renderer.rs` は既に `tauri::Window` に対し wgpu の `create_surface` を通している（＝`tauri::Window: HasWindowHandle + HasDisplayHandle + 'static`）。ただし **spike が実証したのは `winit::Window` であって `tauri::Window` ではない**ため、束ねの確定は下記ゲート G1 のコンパイルチェックに委ねる。
2. **テキスト AA の parity 対象は egui-wgpu の linear filter。** `epaint-0.35.0/src/texture_atlas.rs:191` の `texture_options()` は `TextureOptions::LINEAR` を返す。egui-wgpu は atlas を `Rgba8Unorm`（非 sRGB）でアップロードするため、ハードウェア bilinear は **gamma 空間**の premultiplied 値を補間する（sRGB→linear 化は起きない）。spike の `fill_mesh` は `sample_nearest` を使う——これが parity 対象からの既知の乖離源。的を絞った修正は **atlas の bilinear サンプル**（texture の filter を尊重）で、CPU の gamma 空間 bilinear が的と一致する。

### 実装初手で確定させる検証ゲート（崩れると設計が反転する）

- **G1（束ね確定）**: runtime クレート内に `Context::new`+`Surface::new`+`resize`+`present` の 5 行コンパイルチェックを置き、rwh 0.6 束ねを一次証拠にする（初手 Red）。万一 2 メジャー共存や `tauri::Window` 非対応が判明した場合の fallback（`window.window_handle()` から生ハンドル取得）を注記。
- **G2（色空間確定・parity 論の生命線）**: 色 parity は「egui-wgpu が UNORM 形式を選ぶ＝**gamma 空間で over 合成**する」ことに全依存する。egui-wgpu は `preferred_framebuffer_format` で `Bgra8Unorm`/`Rgba8Unorm` を優先し、UNORM なら `fs_main_gamma_framebuffer` を通り blend も gamma 空間で行う（`egui.wgsl:150-162`, `egui-wgpu/renderer.rs:414-425`）——spike の `blend_premultiplied` と完全一致。**しかし surface が sRGB 形式を返すと egui-wgpu は線形空間 blend へ分岐し、spike の gamma 空間 blend は系統的に誤る（テキストの縁が濃く出る＝blocker へ反転）**。Snotra は `renderer.rs:79` で `config.format = target_format` と egui-wgpu にロックステップするため実機で UNORM が出る限り安全だが、これは推論であって実測ではない。**wgpu を撤去する前に**（early gate）、実機で `target_format` を 1 行ログ出力し `Bgra8Unorm`（非 sRGB）であることを一次証拠にする。sRGB なら CPU blend の色空間設計を見直す。

## 境界 — 完全な変更目録

移行の難所はラスタライザ移植ではなく、「ランタイムが **tao + tauri-runtime-wry プラグイン** で統合される」境界を保つこと。spike（`soft_host_main.rs`）は winit + egui_winit 直で統合しており、その winit イベントループは持ち込まない。**据え置きは dispatch・入力・IME・repaint（真に renderer 非依存）だが、「renderer のみ変わる」は誤り**——下表が全変更目録。

| 対象 | 扱い |
|---|---|
| `renderer.rs` | 中身を全面置換（wgpu → softbuffer + `fill_mesh`）。型 `EguiRenderer` と `new`/`configure`/`paint` の外形は維持 |
| `raster.rs`（新規・private mod） | spike の純粋ラスタ中核を抽出（`edge`/`blend_premultiplied`/`modulate`/`CpuTexture`/`image_to_pixels`/`apply_texture_delta`/`fill_mesh`）。`pub(crate)` の純関数群としてテスト可能に |
| `lib.rs` | `mod gpu;` 削除・`mod raster;` 追加・`pub use gpu::GpuFaultInjection` 削除・`pub use surface::{SurfaceAction, surface_action}` 削除（`is_renderable_extent` の再エクスポートのみ残す） |
| `surface.rs` | wgpu 状態機械（`SurfaceAction`/`surface_action` とテスト `recoverable_surface_states_have_distinct_actions`）を撤去。**`is_renderable_extent` とテスト `zero_extent_never_reaches_surface_configuration` は残す**（softbuffer は `NonZeroU32` 必須ゆえ 0×0 ガードは wgpu 時代より重要） |
| `gpu.rs` | 撤去。softbuffer に device loss / OOM / validation の概念は無い |
| `runtime.rs` | 骨格（プラグイン・per-window 状態・repaint 配線）は据え置き。**変更は 6 点**: `use crate::gpu::…`/`renderer::PaintOutcome` の import 調整・`RuntimeError` の GPU variant 再形成・`RuntimeFrame` の `gpu_fault_requested` フィールド除去・`RuntimeFrame::inject_gpu_fault` 除去・`render()` の `PaintOutcome::DeviceRecovered` 分岐除去・`apply_frame_commands` の `gpu_fault_requested` 経路除去 |
| `input.rs` / `ime.rs` / `windows_ime.rs` / `repaint.rs` | 無改変で流用（実読で renderer 型への参照なしを確認） |
| `snotra-egui-runtime/Cargo.toml` | `softbuffer = "=0.4.6"`（`default-features = false`）を追加。`wgpu`/`egui-wgpu`/`pollster` を撤去 |
| `snotra-egui-mvp/Cargo.toml` | 撤去する 5 spike bin の `[[bin]]` ブロック削除（`snotra-egui-glow-mvp`/`-glow-lifecycle-mvp`/`-park-host-mvp`/`-soft-mvp`/`-soft-host-mvp`）。`main.rs` の default bin は転用ゆえ残す。**`.rs` だけ消して `[[bin]]` を残すと `cargo check --workspace --all-targets` が「couldn't read src/…」で即 red**（受け入れ #5 と衝突）。5 bin が唯一の消費者だった依存（`eframe`/`egui_glow`/`glutin`/`glutin-winit`/`winit`/`softbuffer`/`raw-window-handle` 等）は `cargo check` を一次証拠に掃除。`egui`/`snotra-core`/`tauri` 系は転用 main.rs が使うため残す |
| ルート `Cargo.toml` | `egui-wgpu`/`wgpu` の唯一の消費者は runtime だったため（`snotra-settings` は glow で wgpu 非依存）、撤去後の dead entry を掃除 |

## レンダリングパイプライン（softbuffer 版 `paint`）

現行 wgpu の `paint` と同じ呼び出し地点（プラグインの `Event::RedrawRequested` → `EguiWindow::render` → `renderer.paint`）を保ち、中身のみ差し替える。softbuffer present は wgpu が present するのと同一地点ゆえ、配送構造は据え置きで成立する。

```
paint(ctx, full_output):
  size = window.inner_size()                     // ← live 読み（下記不変条件①）
  is_renderable_extent(size) が false → PaintOutcome::Skipped
  paint_jobs = ctx.tessellate(shapes, ppp)
  textures_delta.set を CpuTexture へ apply（delta.pos で全面/部分更新、delta.options の filter を保持）
  surface.resize(NonZero(size.w), NonZero(size.h))   // ← live size へ
  buffer = surface.buffer_mut()
  buffer.fill(CLEAR_COLOR)                        // 製品ダーク背景色（フラッシュ回避）
  for mesh in paint_jobs:
      clip_min/clip_max = clip_rect の 4 値を round()（下記不変条件④）
      fill_mesh(buffer, w, h, verts, indices, texture, clip, ppp)  // filter は texture に従う
  buffer.present()
  textures_delta.free を破棄
  PaintOutcome::Presented                          // request_repaint しない（不変条件②）
```

**この paint の不変条件（全面書き直しゆえ明記する）:**

1. **resize のサイズ SSOT は live `inner_size()`。** softbuffer には wgpu の Outdated/Lost に相当する「サイズ不整合」信号が無く、config が client area とずれた瞬間、softbuffer は GDI blit で**無言でスケーリング**する（エラー無し・#579 と同一クラスの視覚のみバグ）。spike は毎フレーム live 読みする（`soft_host_main.rs:601,655`）——それに倣う。`configure(w,h)` は「次フレームで描くべきか」の extent ゲート（0×0 判定）に役割を縮め、実際の寸法は paint が live で読む。
2. **`Presented` 経路は `request_repaint` を呼ばない。** egui 自身の repaint コールバック（`runtime.rs:213`）が scheduler を駆動する定常系で、Presented が「念のため」の `request_repaint` を足すと scheduler が即再発火し max rate の自己ループになる。現行 wgpu paint も Presented は呼ばずに返る（`renderer.rs:237`）。request_repaint は skip/エラー等の非定常経路のみ。（注: mvp CLAUDE.md:29 の 2000fps 事故は `RedrawRequested` を `on_window_event` へ渡す別機構で、tao は別 arm ゆえ runtime は構造的に回避済み。ここで塞ぐのは paint 内 repaint という別経路。）
3. **Context は保持せず Surface のみ保持する。** `softbuffer::Surface<tauri::Window, tauri::Window>` は自前の display handle を所有し `Context` の生存に依存しない（spike も Context を local で drop し Surface のみ格納・`soft_host_main.rs:448-467`）。`Context` を struct に保持すると不要な上に、宣言順によっては危険な drop 順序を作りうる。`EguiRenderer` は Surface のみ保持し、Drop 順序問題を構造的に消す。
4. **clip 端は 4 値とも `round()`。** egui-wgpu の ScissorRect は min/max **4 値すべて `round()`**（`egui-wgpu/renderer.rs:1145`）。spike は floor(min)/ceil(max) で、クリップ境界に接した内容が最大 1px はみ出す（顕在化は**スクロール端**とクリップ境界に接した**選択行・行背景 rect**——製品の結果リストが直撃対象）。egui-wgpu に合わせ 4 値 round にする。

補足:
- `softbuffer::Context`/`Surface` は `EguiRenderer::new` で `tauri::Window` から生成（Context は生成後 drop、Surface のみ保持）。
- **復旧モデルの縮退**: `PaintOutcome` は `Presented` / `Skipped` の 2 値へ縮む。wgpu の `SurfaceRecovered`/`DeviceRecovered` は消滅。softbuffer の失敗（surface 生成・resize・buffer borrow・present）は `RuntimeError` として上へ返し、`runtime.rs` の既存エラーログ経路（`SNOTRA_EGUI_RENDER_ERROR`）に載せて次 RedrawRequested で再試行する（texture は CPU 側に在るため再登録不要）。
- `max_texture_side()` は wgpu device limit の代わりに CPU 適正の**固定値**を返す。値は egui のフォントアトラス需要を下回らず（下回ると glyph atlas が黙って clamp される）、かつ CPU メモリを律速しない範囲で選ぶ（例: 8192。8192² RGBA=256MB を上限の目安に plan で根拠付け）。
- **`Primitive::Callback` は無視する**（spike と同じく `Mesh` 以外は捨てる）。現行 wgpu 経路は Callback を描くが softbuffer 経路は描かない。ランチャー用途では未使用ゆえ実害なしだが、**沈黙の能力縮小**として記録する。

## AA 戦略（bilinear now / 図形は verify-then-add）

- **テキスト（now）**: `CpuTexture` に filter 情報（`delta.options.magnification`/`minification`）を持たせ、`fill_mesh` のサンプルを nearest/bilinear で切替える。フォントアトラスは `TextureOptions::LINEAR` ゆえ bilinear が効き、glyph の sub-pixel 位置品質を回復する。**bilinear は half-texel 規約 `uv*size - 0.5` を厳密再現する**（egui-wgpu の predictable 経路が明示・`egui.wgsl:109`）——これを外すと glyph が半 texel ずれる。atlas 端は egui-wgpu のサンプラに合わせ **ClampToEdge**（隣接 glyph の texel bleed を防ぐ）。
- **図形エッジ（verify-then-add）**: **根拠の訂正**——egui-wgpu も既定 **MSAA=1** で、tessellator の feather geometry（頂点 alpha に ~1px の傾斜 AA を焼き込む）に縁 AA を依存している。spike も同じ feather 三角形を重心 alpha 補間で描くため**同等の縁 AA を得る**（「自作ラスタは被覆 AA を全く持たない」は誤り）。残差は fill-rule のタイブレーク（spike は 3 辺 `>=0` の包含、GPU は top-left rule——共有辺の稀な二重被覆）と細帯サンプリングのみ。**製品規模スモークでこの残差が目視できたときのみ** supersample か top-left rule 化を追加する。
- **実装時に織り込む minor 差**（spec を止めない）: dithering 既定差（egui-wgpu は `RendererOptions::default().dithering=true` で ±1LSB のノイズを撒く・spike は撒かない——平坦色ではほぼ不可視）／重心色補間の `as u8`（floor）による下方バイアス（テキストが僅かに細く/暗く出うる——round にすれば減る）。

## harness と受け入れ実証（main.rs 転用）

softbuffer 化した runtime を end-to-end で動かし、受け入れ条件を自証する主体が要る。runtime を `EguiView`/`EguiRuntime` 経由で駆動する唯一の現行 probe は `snotra-egui-mvp/src/main.rs`（wgpu 経路）であり、winit soft spike 群は runtime クレートを経由しない。ロードマップ決定1は `main.rs` を撤去対象に列挙していたが、素直に削除すると softbuffer **ランタイム**を通しで動かすものが消え、SU1 が AA 目視 parity・IME を自証できなくなる（この穴はロードマップ策定時に見落とされていた）。

- **決定（本 brainstorm で改定）**: `main.rs` は撤去せず、`MvpView` を softbuffer 化した runtime 経由で駆動する形へ書き換えて **runtime 駆動 probe を 1 本維持**する。ロードマップ決定1の「`main.rs` 撤去」は本 spec でこの転用へ置き換える（ロードマップ本体の決定1 にも override 注記が要る——別途）。転用に要る具体の手当ては下記 3 点。
  1. **`GpuFaultInjection`/`inject_gpu_fault` 依存を除去**（main.rs:18/52/86/178-184/385-392/805-819 が使用）。GPU fault 注入 UI/経路を落とす。
  2. **可視起動へ固定**: 転用元 main.rs は既定で**非表示起動**（`start_visible = env_flag("SNOTRA_EGUI_MVP_START_VISIBLE")`、未設定=false）。隠れた HWND には `RedrawRequested` の配送保証が無く、初回フレームが届かず SU1 が視覚/IME を自証できない。SU1 の harness は可視起動に固定する（または注入 redraw の非表示配送を別途検証項目に立てる）。
  3. **フォントを先頭配置へ反転**: 現行 `configure_japanese_font`（main.rs:674-682）は `push`（末尾 fallback）——被覆 AA を持たない softbuffer では #399/#579 のベースラインずれを顕在化させる当のパターン。`insert(0, …)`（先頭）へ反転する。
- **IME（経路を名指しで確認）**: runtime 自身の IMM32 経路（`ime.rs`/`windows_ime.rs`）は renderer 非依存ゆえ swap を生き延びる。**spike の winit `Ime` イベント検証（#582）は runtime の経路とは別物**（spike=winit `WindowEvent::Ime`、runtime=tao `ReceivedImeText` + IMM32 subclass）で転移しない。二重投入禁止の SSOT は **`input.rs`**（`KeyEvent::text` を egui へ回さず、確定は `ReceivedImeText`→`ImeEvent::Commit` のみ）。**swap はこの境界を変えない**ため、検証は「input.rs の `ReceivedImeText` vs `KeyboardInput.text` 境界で二重投入回帰が無いこと」の確認的スモーク（転用 harness で preedit/候補/確定/Esc キャンセル/通常文字の非二重を実操作トレース）。
- **フォント機構化の scope（b 案）**: フォント設定は view 側 `EguiView::setup` の責務で、runtime クレート自身はフォントを設定しない（view が UI/状態/フォントを所有する境界を保つ）。ゆえに **runtime レベルの font helper は作らない**。config テストは SU1 の転用 harness の**実 `EguiView::setup` 経路を駆動して読み戻し**、jp_font が Proportional/Monospace の index 0 にあることを固定する（`push` なら index>0 で fail するカナリア＝「起動しただけの緑」にならない）。ただしこのテストは 1 view にしか掛からないため、**「各 `EguiView` 実装は自前の font-first テストを持つ」を SU2 申し送りに明記**する（SU2 の製品 view が `push` を再導入すると glow の sub-pixel AA 隠蔽が剥がれて #579 が再発する——mvp CLAUDE.md:30 の再発構造）。
- **winit soft spike 群**（`soft_main.rs`/`soft_host_main.rs`）は撤去候補。役割（softbuffer ラスタが動く実証）が runtime + 転用 main.rs probe に包摂される。ラスタ中核テストを `raster.rs` へ移植した後に撤去し、**撤去に伴う索引同期**（`snotra-egui-mvp/CLAUDE.md` モジュール構成・`docs/build-commands.md:100-101` の実行例・README・RETROSPECTIVE.md の参照）を行う（governance-check が PR CI で拾うが、受け入れの一部として先に潰す）。

## 公開 API の変更（縮小を明示）

- `lib.rs` から撤去: `GpuFaultInjection`、`SurfaceAction`、`surface_action`。
- 維持: `is_renderable_extent`、`key_from_tao`、`modifiers_from_tao`、`EguiRuntime`、`EguiView`、`RuntimeError`、`RuntimeFrame`。
- `RuntimeFrame::inject_gpu_fault` を除去。
- `RuntimeError` を再形成: wgpu 由来の `GpuInitialization`/`SurfaceValidation`/`GpuOutOfMemory` を softbuffer の失敗モード（surface 生成・resize・present 等）を表す variant へ置換。`Tauri`（`#[from] tauri::Error`）/`ImeInitialization`/`NotInstalled`/`DuplicateWindow` は維持。GPU variant の構築箇所は renderer.rs のみ（全て置換対象）で、外部 `match` は無く消費側は Display でログするだけ——縮小は他の match を壊さない。
- 現行 API の外部消費者は転用する `snotra-egui-mvp`（`Cargo.toml` の path-dep 1 本）のみ。SU2 以降の製品消費者は未存在ゆえ、この縮小に外部契約破壊は無い。

## テスト計画

- **G1 バインド確定（初手 Red）**: `Context::new`+`Surface::new`+`resize`+`present` の最小コンパイルチェックを runtime クレート内に置き、rwh 0.6 束ねを一次証拠にする。fallback を注記。
- **G2 色空間確定（wgpu 撤去前）**: 実機で egui-wgpu の `target_format` をログし `Bgra8Unorm`（非 sRGB）を確認。sRGB なら CPU blend 設計を見直す。
- **ラスタ中核の不変条件**: `soft_host_main.rs` の `raster_core_matches_soft_probe_invariants`（`edge`/`blend_premultiplied`/`modulate`/`fill_mesh` の代表入力）を `raster.rs` へ移植。移植で失われる命題を孤立させない。
- **bilinear**: filter=LINEAR の texture を分数 uv でサンプルしたとき隣接 texel の補間値が出ることを固定。**half-texel 規約 `uv*size-0.5` を pin**し、**atlas 端の ClampToEdge ケース**（端で隣接 glyph を bleed しない）を含める。nearest との差分も明示。
- **0×0 ガード**: `is_renderable_extent` のテスト（`zero_extent_never_reaches_surface_configuration`）を残す。
- **フォント先頭配置**: 転用 harness の実 `EguiView::setup` を駆動し、`FontDefinitions` で jp_font が Proportional/Monospace の index 0 にあることを固定（`push` で fail するカナリア）。
- **視覚 + IME スモーク**: 可視起動の転用 main.rs で、製品規模テキスト（日本語 + 長パス）の AA 目視 parity と、input.rs 境界での IME 非二重投入（preedit/候補/確定/Esc キャンセル/通常文字）をトレースで確認。

## 受け入れ条件（SU1）

1. `EguiView` を差し替えるだけで任意 UI を softbuffer で描画できる（可視起動の転用 main.rs がその実証）。
2. G2（`target_format`=UNORM）が確認され、フォント単一化（jp_font 先頭）が実 setup 駆動の config テストで機構化され、bilinear（half-texel/端 clamp 込み）適用後のテキストが製品規模で egui-wgpu と目視 parity。図形エッジ AA の残差（fill-rule/細帯）は、不足が目視できなければ defer と記録。
3. IME preedit/候補/確定が softbuffer 上（runtime の input.rs 境界経由）で正しく、二重投入回帰が無い。
4. `gpu.rs`/`SurfaceAction`/`GpuFaultInjection` が撤去され、paint の不変条件①〜④が実装に反映され、`cargo clippy --workspace --all-targets`・runtime のテストが緑。
5. 撤去 bin（`glow_main`/`glow_lifecycle_main`/`glow_park_host_main` と winit soft spike）の `.rs` と `[[bin]]` と依存と索引参照が同期され、`main.rs` は転用され、`cargo check --workspace` が緑。ルート Cargo.toml の wgpu/egui-wgpu dead entry が掃除済み。

## リスク

- **図形エッジ残差（fill-rule/細帯）**: verify-then-add で吸収。製品規模で不足が判明した場合の追加（top-left rule 化/supersample）は非自明。SU1 内で完結させるか SU3 の結果表示へ送るかはスモーク結果で判断。
- **G2 の色空間反転**: 実機 `target_format` が sRGB だった場合、gamma 空間 CPU blend の前提が崩れる。early gate で潰す。
- **softbuffer present の隠しウィンドウ配送保証**: 隠れた HWND には `RedrawRequested` の配送保証が無い（spike の知見）。runtime は `RedrawRequested` を入力へ回さない構造で自己ループを回避済み、SU1 harness は可視起動で足る。hide→show の初回フレーム提示は SU2 のウィンドウライフサイクルと合わせて確認する（申し送り）。
- **font-first の SU2 再発**: 機構化は 1 view にしか掛からない。SU2 申し送りで各 view の自前テストを義務化。
- **Tauri 内部 API 追随**: `tauri-runtime-wry` の unstable feature 依存は #532 既知。SU1 で新たな追随は増えない（renderer の内側のみ変更）。

## スコープ外（SU1 では触らない）

- ウィンドウシェル・状態機械（Alt+Q/blur/フォーカス列/位置永続/初回フロー）は SU2。hide→show の初回フレーム提示検証も SU2。
- 検索体験・直 `Engine`・IPC 撤去は SU3。アイコンは SU4。updater は SU5。config 反映・終了保存は SU6。切替・配布は SU7。
- 再変換（IME reconvert）は WANT。SU1 で低コストに載らなければ触らない。
- ルート `CLAUDE.md`/`AGENTS.md` 等の規範文書は変更しない。ロードマップ決定1 の override 注記のみ別途。
