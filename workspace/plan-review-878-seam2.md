# plan-review: #878 継ぎ目 2 — 観点 A（検出器の妥当性）・観点 B（散文の網羅）

対象: `workspace/plan.md`（基点 `main` = `6638f7f9`）。以下は 2 つの観点だけを検証した結果。

## 要対処

### A-1. Phase 2 の検出器は「バー矩形が作業領域より幅広い」残余で偽陽性を出しうる

`show_egui_main` の `position_on_target_monitor` は基準モニターを `cursor_monitor_work_area` /
`primary_monitor_work_area`（`window_coordinator.rs:230-234`）で決め、`target_wa.clamp(x, y,
win_w, win_h)`（`window_coordinator.rs:251`）で置く。一方 `clamp_main_into_work_area` は
`read_bar_anchor` が計算するバー矩形の**中心**から `point_monitor_work_area`
（`window_coordinator.rs:634-635`）で基準モニターを選び直す——**この 2 つは別関数であり、
一致は構造で担保されていない**。

`WorkArea::clamp`（`monitor.rs:36-42`）は `win_w`/`win_h` がその work area の幅/高さを
超えるとき、`max_x = (right - win_w).max(left)` により**左上へ寄せるだけで、右端/下端の
はみ出しは許容する**（`clamp_window_wider_than_work_area_aligns_to_left` が実測済み）。この
とき置かれたバー矩形の**中心**は、`target_wa` の隣のモニターへ越境しうる。越境すれば
`point_monitor_work_area(cx, cy)` は show が使った `target_wa` とは**別のモニター**を返し、
`clamp_main_into_work_area` はその別モニター基準で `nx != a.pos.x` を計算して
**実際に動かし**、Phase 2 の `egui_main:position_clamped_after_show` が発火する——
**位置決めが正しく動いていても、である**。

この残余は `SPEC.md:483`「バー矩形が作業領域より幅広い場合」として**既に宣言済み**
（`appearance.window_width` に上限が無く、低解像度・高 DPI・狭いモニターで到達可能）。
既存の `egui_main:height_mismatch` 検出器は 4 連言（indexing/toast/launching/notice の一致）
でこの種の「正常だが疑わしい」状態を除外しているが（`view.rs:1217-1225`）、Phase 2 の
位置検出器の発火条件（`was_reset_frame && 動いた`）には**この除外が無い**。

smoke の既定プロファイルは単一モニター・`window_width=600` なのでこの経路には到達せず、
**CI 上は緑のまま推移する**が、実運用（多モニター + 広い `window_width` 設定）でこの trace が
「継ぎ目 2 の退行」として誤って読まれる可能性が残る。height_mismatch 検出器が持つ除外の
設計思想（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）
に照らすと、**この既知の残余を検出器の doc（Phase 3 の新規 ADR か
`egui_main:position_clamped_after_show` の呼び出し点コメント）へ明示するか、発火条件から
除外する**必要がある。少なくとも計画へ「この検出器が偽陽性を出しうる既知の残余」として
一文を足すべき。

### A-2. `docs/architecture.md:82` が変更後に偽になる（計画の grep 網羅から漏れている）

計画の未確定 5 は `grep -n "outer_size\|position_on_target_monitor\|bar_rect"
docs/architecture.md docs/build-commands.md` を実行し「0 件・更新不要」と判定しているが、
実際に該当する記述は**別の語彙**で存在する：

```
docs/architecture.md:82:
「...show 時に bar_height（`font_size + bar_padding`・既定 43px）へリセットする」
```

この一文は現行の「1 手目 `set_size`（バー高）→ 位置 → 2 手目 `set_size`（実高）」という
**物理的な collapse** を指している。計画の Phase 1 は 1 手目の `set_size` を撤去し、
show は実高を**一度だけ** `set_size` するため、main の高さが物理的に `bar_height` へ
リセットされる瞬間はもう存在しない。この行は変更後に偽になる。

計画の変更ファイル一覧・`SPEC.md`・関連文書の更新要否表のどちらにも `docs/architecture.md`
は挙がっていない。Phase 3 のタスクへ本行の更新（またはこの文の削除）を足す必要がある。

## 軽微

- **`was_reset_frame` は「clamp が呼ばれる条件」を拡張しない。** `clamp_main_into_work_area`
  の呼び出し自体は `!ui.input(|i| i.pointer.any_down())` だけで決まり（`view.rs:1279`）、
  `was_reset_frame` は trace を出すかどうかにしか関わらない。ゆえに show 直後の最初の
  フレームでポインタが押されている（例: トレイアイコンクリックで表示した瞬間、ボタンが
  まだ down）場合、その回だけは clamp 自体が呼ばれず、Phase 2 の不変条件チェックは
  **その回に限り黙ってスキップされる**（偽陽性にも偽陰性にもならないが、検証機会を
  1 回落とす）。実害は小さいが計画のどこにも触れられていない。
- **config（`bar_height` 等）が show と最初のフレームの間で変わる経路**（`config_watcher` の
  100ms debounce 適用）は、既存の `height_mismatch` 検出器にも対称的に存在するリスクであり、
  本計画が新規に持ち込むものではない。ただし Phase 2 の位置検出器も同じ窓を共有するため、
  ここでも偽陽性の芽になりうる——height_mismatch 側で既に受容されている残余と同種として
  扱ってよい（新規の対処は不要、計画に追記するかは任意）。

## 未検証

- **Phase 2 の故障注入（`derive_bar_rect_phys` の height を実高へ差し替える）が、守りたい
  退行と同じ強さかは実装前には測れない。** 計画自身も「実装時の実測」と明記しており
  （セルフレビュー・未確定リスト）、コードだけを読んだ限りでは注入の形（バー高の代わりに
  実高でクランプする）は #904 コメントが記録した実際の退行（「実高でクランプしていたが、
  それは挙動の後退だった」）と一致する形に見えるが、位置ズレの実測値までは確認していない。
- **非クライアント差分（`outer.height - inner.height` / `outer.width - inner.width`）が
  show 直前の `read_frame_geom` 呼び出しと、reset-on-show 消費フレームの `read_bar_anchor`
  呼び出しのあいだで同一であることは未実測。** 理屈上は同じ hidden→visible 直後の窓で
  DWM シャドウ量が変わる理由が無いため成立するはずだが、実機での固定は確認していない。
