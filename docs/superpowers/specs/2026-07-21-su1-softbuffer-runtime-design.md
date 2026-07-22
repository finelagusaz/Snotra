# SU1 設計 — snotra-egui-runtime の softbuffer 置換（#532 Phase 2）

- 種別: サブユニット設計（spec）。実装計画は本 spec 承認後に別途 writing-plans で作る
- 日付: 2026-07-21
- 親: `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md`（SU1）／#532
- 履歴: 初版を 3 レンズ（描画正当性・境界/API・不変条件）+ codex（第 4 レンズ）敵対的レビューで硬化。反映点は各節に織り込み済み

## 目的

`snotra-egui-runtime` の描画基盤を wgpu → softbuffer + CPU ラスタライズ（`fill_mesh`）へ **in-place 置換**する。`EguiRuntime`/`EguiView` の公開 API と tao + tauri-runtime-wry プラグイン統合を保ち、`EguiView` を差し替えるだけで任意 UI を softbuffer で描けるクレートにする。SU2 以降（製品メインウィンドウ）の描画基盤となる。

## 決定の要石

### 検証済み（この設計の前提）

1. **softbuffer は tao/wry 管理の `tauri::Window` にバインドできる（見込み）。** `Cargo.lock` の `raw-window-handle` は 0.6.2 が単一定義。softbuffer 0.4.6 は rwh 0.6 の `HasWindowHandle`/`HasDisplayHandle` を要求し、`renderer.rs` は既に `tauri::Window` に対し wgpu の `create_surface` を通している。ただし spike が実証したのは `winit::Window` であって `tauri::Window` ではないため、束ねの確定はゲート G1 に委ねる。
2. **テキスト AA の parity 対象は egui-wgpu の linear filter。** `epaint-0.35.0/src/texture_atlas.rs:191` の `texture_options()` は `TextureOptions::LINEAR`。egui-wgpu は atlas を `Rgba8Unorm`（非 sRGB）でアップロードし、bilinear は **gamma 空間**の premultiplied 値を補間する。spike の `fill_mesh` は `sample_nearest`——これが乖離源。修正は **atlas の bilinear サンプル**（filter を尊重）で、CPU の gamma 空間 bilinear が的と一致。
3. **`softbuffer::Surface` は Send（`!Sync` のみ）。** softbuffer 自身の `__assert_send`（`softbuffer-0.4.6/src/lib.rs:325-332`）が `is_send::<Surface<(),()>>()` を assert する。`PhantomData<Cell<()>>`（同 78）は Sync を外すマーカーで Send は外さない。ゆえに `Surface<tauri::Window, tauri::Window>` を `Send + 'static` 制約の plugin builder（`pending`）経由で保持しても**コンパイルは破れない**（第 4 レンズが blocker とした懸念を一次証拠で反証）。残る配慮はスレッド親和性のみ（下記ライフサイクル）。

### 実装初手で確定させる検証ゲート（崩れると設計が反転する）

- **G1（束ね確定）**: runtime クレート内に `Context::new`+`Surface::new`+`resize`+`present` の 5 行コンパイルチェックを置き、rwh 0.6 束ねを一次証拠にする（初手 Red）。fallback（`window.window_handle()` から生ハンドル取得）を注記。
- **G2（色空間確定・parity 論の生命線）**: 色 parity は「egui-wgpu が UNORM 形式を選ぶ＝**gamma 空間で over 合成**」に全依存する（`egui.wgsl:150-162`, `egui-wgpu/renderer.rs:414-425` が spike の `blend_premultiplied` と一致）。surface が sRGB を返すと egui-wgpu は線形空間 blend へ分岐し、spike の gamma 空間 blend は系統的に誤る（縁が濃く出る＝blocker へ反転）。**wgpu を撤去する前に**、実機で `target_format`（`renderer.rs:79`）を 1 行ログし `Bgra8Unorm`（非 sRGB）を一次証拠にする。sRGB なら CPU blend の色空間設計を見直す。

## 境界 — 完全な変更目録

移行の難所はラスタライザ移植ではなく、「ランタイムが **tao + tauri-runtime-wry プラグイン** で統合される」境界を保つこと。spike（`soft_host_main.rs`）の winit イベントループは持ち込まない。**据え置きは dispatch・IME・repaint（真に renderer 非依存）だが、「renderer のみ変わる」は誤り**——下表が全変更目録。

| 対象 | 扱い |
|---|---|
| `renderer.rs` | 中身を全面置換（wgpu → softbuffer + `fill_mesh`）。型 `EguiRenderer` の外形は維持。softbuffer surface は生成時（後述ライフサイクル）に確定 |
| `raster.rs`（新規・private mod） | spike の純粋ラスタ中核を抽出（`edge`/`blend_premultiplied`/`modulate`/`CpuTexture`/`image_to_pixels`/`apply_texture_delta`/`fill_mesh`）。`pub(crate)` 純関数群。`CpuTexture` に filter を追加（bilinear/nearest 切替） |
| `lib.rs` | `mod gpu;` 削除・`mod raster;` 追加・`pub use gpu::GpuFaultInjection` 削除・`pub use surface::{SurfaceAction, surface_action}` 削除（`is_renderable_extent` のみ残す） |
| `surface.rs` | `SurfaceAction`/`surface_action` とテスト `recoverable_surface_states_have_distinct_actions` を撤去。**`is_renderable_extent` とテスト `zero_extent_never_reaches_surface_configuration` は残す**（softbuffer は `NonZeroU32` 必須） |
| `gpu.rs` | 撤去。softbuffer に device loss / OOM / validation は無い |
| `runtime.rs` | プラグイン配線・per-window 状態・repaint は据え置くが、変更は次に及ぶ: `use` 調整・`RuntimeError` の GPU variant 再形成・`RuntimeFrame` の `gpu_fault_requested`/`inject_gpu_fault` 除去・`PaintOutcome::DeviceRecovered` 分岐除去・**softbuffer surface をプラグイン活性化時（イベントループスレッド）に生成**・**visible 状態を持ち非表示中は描画抑止**・**RawInput を live size/ppp で構築**（下記ライフサイクル/不変条件） |
| `input.rs` | ほぼ据え置きだが **`take()` が live physical size + scale_factor を受け取り**、`screen_rect`/viewport ppp をイベント駆動値でなく当該フレームの live 値から作る（不変条件①）。イベント→egui 変換の本体は無改変 |
| `ime.rs` / `windows_ime.rs` / `repaint.rs` | 無改変で流用 |
| `snotra-egui-runtime/Cargo.toml` | `softbuffer = "=0.4.6"`（`default-features = false`）追加。`wgpu`/`egui-wgpu`/`pollster` 撤去 |
| `snotra-egui-mvp/Cargo.toml` | 撤去 5 spike bin の `[[bin]]` 削除（`-glow-mvp`/`-glow-lifecycle-mvp`/`-park-host-mvp`/`-soft-mvp`/`-soft-host-mvp`）。`main.rs` は転用ゆえ残す。**`.rs` だけ消して `[[bin]]` を残すと `cargo check --workspace --all-targets` が即 red**。5 bin が唯一の消費者だった依存（`eframe`/`egui_glow`/`glutin`/`glutin-winit`/`winit`/`softbuffer`/`raw-window-handle` 等）は `cargo check` を一次証拠に掃除。`egui`/`snotra-core`/`tauri` 系は残す |
| ルート `Cargo.toml` | `egui-wgpu`/`wgpu` の唯一の消費者は runtime（`snotra-settings` は glow で wgpu 非依存）だったため dead entry を掃除 |

## レンダリングパイプライン（softbuffer 版 `paint`）

現行 wgpu と同じ呼び出し地点（プラグインの `Event::RedrawRequested` → `EguiWindow::render` → `renderer.paint`）を保つ。

```
render():
  if !visible: return                              // 不変条件⑥（hide 抑止）
  size = window.inner_size(); ppp = window.scale_factor()   // live・同一フレーム
  is_renderable_extent(size) が false → return
  raw_input = input.take(max_texture_side, size, ppp)       // 不変条件①: screen_rect/ppp は live
  full_output = ctx.run_ui(raw_input, |ui| view.update(ui, frame))
  paint_jobs = ctx.tessellate(full_output.shapes, ppp)      // 同じ ppp
  texture store へ set/free を apply（free は present 成否に依らず・不変条件⑤）
  surface.resize(NonZero(size))                             // 同じ size
  buffer = surface.buffer_mut(); buffer.fill(CLEAR_COLOR)
  各 Mesh: clip の 4 値を round() し fill_mesh（filter は texture に従う・不変条件④）
  buffer.present()  失敗時 → ログ + request_repaint(バックオフ)（不変条件⑤）
  // Presented: request_repaint しない（不変条件②）
```

**この paint の不変条件（全面書き直しゆえ明記する）:**

1. **surface サイズと tessellation は同一フレームの同一ソース（全 live）。** surface だけ live にし tessellation を `InputState` のイベント駆動 `screen_rect`/ppp（`input.rs:28-49,61-70`）のままにすると、`RedrawRequested` が `Resized` より先に来る/resize・DPI 途中でイベントを取りこぼすと、旧 logical rect・旧 ppp で mesh を作り新 physical buffer に描き、右下端の欠落・clip ずれ・IME rect 不一致を生む。ゆえに **paint が live physical size + scale_factor を読み、surface resize も RawInput の `screen_rect`/ppp も tessellate の ppp も同じ値から導く**（#579 DPI 試験・3000 耐久を通した spike が egui_winit 経由でこの live 一貫性を持つ）。これは初版 Lens 3 の「surface だけ live inner_size を読む」を supersede する（衝突を明示）。
2. **`Presented` 経路は `request_repaint` を呼ばない。** egui 自身の repaint コールバック（`runtime.rs:213`）が scheduler を駆動する定常系で、Presented が「念のため」の `request_repaint` を足すと即再発火の自己ループになる。request_repaint は失敗再試行のみ（不変条件⑤）。（mvp CLAUDE.md:29 の 2000fps は `RedrawRequested` を `on_window_event` へ渡す別機構で tao は別 arm ゆえ回避済み。）
3. **Context は保持せず Surface のみ保持する。** `Surface<tauri::Window, tauri::Window>` は自前の display handle を所有し `Context` 生存に依存しない（spike も Context を local で drop・`soft_host_main.rs:448-467`）。`Context` 保持は不要かつ危険な drop 順序を生む。
4. **clip 端は 4 値とも `round()`。** egui-wgpu の ScissorRect は 4 値すべて `round()`（`egui-wgpu/renderer.rs:1145`）。spike の floor/ceil はクリップ境界に接した内容を最大 1px はみ出す（スクロール端・選択行・行背景 rect——製品結果リストが直撃）。
5. **描画失敗は能動再試行・free は present 非依存。** resize/buffer_mut/present の失敗はログ後 `request_repaint`（バックオフ付き）で再試行を能動要求する（「次 RedrawRequested 待ち」は egui が求めなければ停止する）。永続失敗は上限回数でエラー。`textures_delta.free` は CPU 側 HashMap から除くだけゆえ **present 成否に依らず処理**し、失敗時に free を落として CPU texture を残留させない。
6. **非表示中は描かない。** runtime に visible 状態を持ち、非表示中は paint/present をスキップ、show 時に 1 回再描画する（spike の Suspended 相当・`soft_host_main.rs:588-590`）。`RuntimeFrame::hide_window` は既存公開 API で harness も使うため、隠しウィンドウへの無駄 present と失敗再試行の結合を SU1 で断つ。

補足:
- **softbuffer surface の生成スレッド**: Surface は Send（要石3）だが GDI サーフェスはスレッド親和性を持つため、**present するスレッド（tao イベントループ）で生成する**——プラグインの `attach_pending_windows`（活性化時・イベントループスレッド）で作り `active` に閉じ込め、`pending` には surface を持たせない。これに伴い attach() の同期エラー契約は狭まり、surface 生成失敗はイベントループ側で `SNOTRA_EGUI_RENDER_ERROR` としてログ（生成は DC 取得のみで失敗は稀）。
- **復旧モデルの縮退**: `PaintOutcome` は `Presented` / `Skipped` の 2 値。wgpu の `SurfaceRecovered`/`DeviceRecovered` は消滅（texture は CPU 側ゆえ device 復旧不要）。
- `max_texture_side()` は wgpu device limit の代わりに CPU 適正の**固定値**を返す。8192 は atlas 上限で 256MB 相当ゆえランチャーには過大——**2048〜4096 を目安**に、egui フォントアトラス需要を下回らない値を plan で根拠付ける。`EguiView` が大きい user texture を登録する経路の総量方針も plan で決める（固定最大辺だけでは CPU メモリ上限にならず、softbuffer のメモリ削減目的と衝突するため）。
- **`Primitive::Callback` は無視する**（spike と同じく `Mesh` 以外は捨てる）。現行 wgpu は Callback を描くが softbuffer 経路は描かない。ランチャー用途では未使用ゆえ実害なしだが、**沈黙の能力縮小**として記録する。

## AA 戦略（bilinear now / 図形は verify-then-add）

- **テキスト（now）**: `CpuTexture` に filter 情報（`delta.options.magnification`/`minification`）を持たせ `fill_mesh` を nearest/bilinear で切替。atlas は LINEAR ゆえ bilinear が効き glyph の sub-pixel 位置品質を回復。**bilinear は half-texel 規約 `uv*size - 0.5` を厳密再現**（`egui.wgsl:109`）——外すと glyph が半 texel ずれる。atlas 端は **ClampToEdge**（隣接 glyph の texel bleed を防ぐ）。
- **図形エッジ（verify-then-add）**: **根拠訂正**——egui-wgpu も既定 **MSAA=1** で tessellator の feather geometry（頂点 alpha の ~1px 傾斜 AA）に縁 AA を依存。spike も同 feather 三角形を重心 alpha 補間で描くため**同等の縁 AA を得る**（「被覆 AA を全く持たない」は誤り）。残差は fill-rule タイブレーク（spike は 3 辺 `>=0` 包含、GPU は top-left rule）と細帯サンプリングのみ。**製品規模スモークで不足が目視できたときのみ** top-left rule 化/supersample を追加。
- **実装時に織り込む minor 差**: dithering 既定差（egui-wgpu は `dithering=true`・spike は無し・平坦色ではほぼ不可視）／重心色補間の `as u8`（floor）による下方バイアス（round にすれば減る）。

## harness と受け入れ実証（main.rs 転用）

runtime を `EguiView`/`EguiRuntime` 経由で駆動する唯一の現行 probe は `snotra-egui-mvp/src/main.rs`（wgpu 経路）で、winit soft spike は runtime を経由しない。ロードマップ決定1は `main.rs` を撤去対象に列挙していたが、素直に削除すると softbuffer ランタイムを end-to-end で動かすものが消え、SU1 が AA/IME を自証できない（ロードマップ本体にも override 注記済み）。

- **決定**: `main.rs` は撤去せず `MvpView` を softbuffer runtime 駆動へ書き換え、runtime 駆動 probe を 1 本維持。転用に要る手当ては 3 点。
  1. **`GpuFaultInjection`/`inject_gpu_fault` 依存を除去**（main.rs:18/52/86/178-184/385-392/805-819）。
  2. **可視起動へ固定**: 転用元は既定で非表示起動（`start_visible = env_flag(...)`、未設定=false）。隠れた HWND には `RedrawRequested` の配送保証が無く初回フレームが届かない。SU1 harness は可視起動に固定。
  3. **フォントを先頭配置へ反転**: 現行 `configure_japanese_font`（main.rs:674-682）は `push`（末尾）——softbuffer で #399/#579 のベースラインずれを顕在化させる当のパターン。`insert(0, …)` へ反転。
- **IME（経路を名指しで確認）**: runtime の IMM32 経路（`ime.rs`/`windows_ime.rs`）は renderer 非依存で swap を生き延びる。**spike の winit `Ime` 検証（#582）は runtime の経路とは別物**（spike=winit `WindowEvent::Ime`、runtime=tao `ReceivedImeText`+IMM32）で転移しない。二重投入禁止の SSOT は **`input.rs`**（`KeyEvent::text` を egui へ回さず、確定は `ReceivedImeText`→`ImeEvent::Commit` のみ・input.rs:118-122,177-178）。**swap はこの境界を変えない**ため、検証は「input.rs の `ReceivedImeText` vs `KeyboardInput.text` 境界で二重投入回帰が無いこと」の確認的スモーク（preedit/候補/確定/Esc キャンセル/通常文字の非二重を実操作トレース）。
- **フォント機構化の scope（b 案）**: フォント設定は view 側 `EguiView::setup` の責務で runtime クレートはフォントを設定しない（view が UI/状態/フォントを所有する境界を保つ）。ゆえに **runtime レベルの font helper は作らない**。config テストは転用 harness の**実 `EguiView::setup` 経路を駆動して読み戻し**、jp_font が Proportional/Monospace の index 0 にあることを固定（`push` なら index>0 で fail するカナリア）。1 view にしか掛からないため、**「各 `EguiView` 実装は自前の font-first テストを持つ」を SU2 申し送りに明記**（SU2 の製品 view が `push` を再導入すると glow の sub-pixel AA 隠蔽が剥がれ #579 再発）。
- **winit soft spike 群**（`soft_main.rs`/`soft_host_main.rs`）は撤去候補。役割が runtime + 転用 main.rs probe に包摂される。ラスタ中核テストを `raster.rs` へ移植後に撤去し、**索引同期**（`snotra-egui-mvp/CLAUDE.md` モジュール構成・`docs/build-commands.md:100-101`・README・RETROSPECTIVE.md）を行う。

## 公開 API の変更（縮小を明示）

- `lib.rs` から撤去: `GpuFaultInjection`、`SurfaceAction`、`surface_action`。維持: `is_renderable_extent`、`key_from_tao`、`modifiers_from_tao`、`EguiRuntime`、`EguiView`、`RuntimeError`、`RuntimeFrame`。
- `RuntimeFrame::inject_gpu_fault` を除去。`input.rs::take()` はクレート内部 API（`pub(crate)`）ゆえ live size/ppp 引数追加に外部影響なし。
- `RuntimeError` を再形成: wgpu 由来の `GpuInitialization`/`SurfaceValidation`/`GpuOutOfMemory` を softbuffer の失敗モード（surface 生成・resize・present 等）へ置換。`Tauri`/`ImeInitialization`/`NotInstalled`/`DuplicateWindow` は維持。GPU variant の構築は renderer.rs のみ・外部 `match` 無し——縮小は他を壊さない。
- 現行 API の外部消費者は転用する `snotra-egui-mvp`（path-dep 1 本）のみ。SU2 以降の製品消費者は未存在ゆえ外部契約破壊は無い。

## テスト計画

- **G1 バインド確定（初手 Red）**: `Context::new`+`Surface::new`+`resize`+`present` の最小コンパイルチェック。fallback 注記。
- **G2 色空間確定（wgpu 撤去前）**: 実機で `target_format`=`Bgra8Unorm` を確認。sRGB なら CPU blend 設計を見直す。
- **ラスタ中核の不変条件**: `raster_core_matches_soft_probe_invariants`（`edge`/`blend`/`modulate`/`fill_mesh` 代表入力）を `raster.rs` へ移植。
- **texture store の状態遷移**（新規・独立テスト）: `apply_texture_delta` の `pos=None`（全面）→`Some`（部分上書き）・free 後の更新・未登録 ID への部分更新（無言破棄）・範囲外 delta（切り詰め）・nearest/linear filter の保持を固定。移植は「そのままの spike」でなく filter を足すため、部分更新破損がテストを素通りしないようにする。
- **bilinear**: filter=LINEAR で分数 uv の補間値、**half-texel 規約 `uv*size-0.5`**、**atlas 端 ClampToEdge**（端で隣接 glyph を bleed しない）を pin。nearest との差分も。
- **size 同一フレーム同期**（新規）: `Resized`/`ScaleFactorChanged` 直後の最初の redraw で、surface size・RawInput `screen_rect`・tessellate ppp が同一 live 値になることを、イベント列トレースまたは fractional DPI ケースで固定。
- **hide 抑止**（新規）: 非表示中は paint/present がスキップされ、show で 1 回再描画されることを固定（visible 状態遷移の単体）。
- **0×0 ガード**: `zero_extent_never_reaches_surface_configuration` を残す。
- **フォント先頭配置**: 転用 harness の実 `EguiView::setup` を駆動し jp_font が両 family index 0 にあることを固定（`push` で fail）。
- **視覚 + IME スモーク**: 可視起動の転用 main.rs で、製品規模テキスト（日本語 + 長パス）の AA 目視 parity と、input.rs 境界での IME 非二重投入をトレース確認。

## 受け入れ条件（SU1）

1. `EguiView` を差し替えるだけで任意 UI を softbuffer で描画できる（可視起動の転用 main.rs が実証）。
2. G2（`target_format`=UNORM）確認、フォント単一化が実 setup 駆動テストで機構化、bilinear（half-texel/端 clamp 込み）適用後のテキストが製品規模で egui-wgpu と目視 parity。図形エッジ残差（fill-rule/細帯）は不足が目視できなければ defer と記録。
3. IME preedit/候補/確定が softbuffer 上（input.rs 境界経由）で正しく二重投入回帰が無い。
4. paint 不変条件①〜⑥（live 同期・Presented 非 repaint・Surface のみ・clip round・失敗再試行と free・hide 抑止）が実装に反映され、`gpu.rs`/`SurfaceAction`/`GpuFaultInjection` 撤去、texture store 状態遷移テスト緑、`cargo clippy --workspace --all-targets`・runtime テスト緑。
5. 撤去 bin の `.rs`/`[[bin]]`/依存/索引参照が同期、`main.rs` 転用、ルート Cargo.toml の dead entry 掃除、`cargo check --workspace` 緑。

## リスク

- **図形エッジ残差（fill-rule/細帯）**: verify-then-add で吸収。追加（top-left rule/supersample）は非自明。SU1 完結か SU3 送りかはスモーク結果で判断。
- **G2 の色空間反転**: 実機 `target_format` が sRGB なら gamma 空間 CPU blend の前提が崩れる。early gate で潰す。
- **size 同期の取りこぼし**: イベント順序に依存しない全 live 設計で吸収するが、fractional DPI の最初の redraw をテストで固定する。
- **hide/repaint の結合**: 非表示中の無駄 present と失敗再試行の結合を SU1 の hide 抑止で断つ。Alt+Q の完全な状態機械は SU2。
- **font-first の SU2 再発**: 機構化は 1 view のみ。SU2 申し送りで各 view の自前テストを義務化。
- **Tauri 内部 API 追随**: `tauri-runtime-wry` の unstable feature 依存は #532 既知。SU1 で新たな追随は増えない。

## スコープ外（SU1 では触らない）

- ウィンドウシェル・**Alt+Q の完全な表示/非表示状態機械**（blur/フォーカス列/位置永続/初回フロー）は SU2。SU1 が持つのは runtime レベルの「非表示なら描かない」最小ガード（不変条件⑥）のみ。hide→show の初回フレーム提示検証も SU2。
- 検索体験・直 `Engine`・IPC 撤去は SU3。アイコンは SU4。updater は SU5。config 反映・終了保存は SU6。切替・配布は SU7。
- 再変換（IME reconvert）は WANT。SU1 で低コストに載らなければ触らない。
- ルート `CLAUDE.md`/`AGENTS.md` 等の規範文書は変更しない。
