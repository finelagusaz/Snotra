# plan-review: #666 段3 `view.rs` 分割 — レイヤー A（egui_shell Rust 実装）

## 1. 問題なし

- **68 項目の分類・母集団は grep で実測一致した**。フィールド 19（`view.rs:244-289`）・inherent メソッド 25（`view.rs:291-970`、`new`〜`spawn_install`）・自由関数 6（フォント 5 + `draw_toast_button`・`view.rs:49,66,135,166,187,987`）・型/静的/定数 9（`JP_FONT_BYTES`/`CJK_PROBE`/`ResolvedFont`/`USER_FONTS` + `FolderMsg`/`LaunchWork`/`LaunchTag`/`LaunchInFlight`/`ToastAction`）・`EguiView` impl 2（`view.rs:1018,1019,1033`）・テスト 7（`view.rs:1765-1868`、全件フォント）= 68。計画の内訳と 1 件も食い違わない。
- **launcher_controller.rs 行き 23 メソッド + view.rs 残留 1（`window_width`）+ new の特殊化で 25 総数と整合**（23+1+1=25）。フィールド 15（controller）+ 4（view: `applied_font_family`/`applied_background_hex`/`last_set_width`/`last_set_height`）= 19 も整合。
- **view.rs 外の消費者は grep で全件捕捉済み**（`mod.rs:77,308`／`results_view.rs:1,442,478`／`window_coordinator.rs:9,405`／`results_window.rs:7,54`／`visual.rs:86`／`commands/launch.rs:50`／`commands/instant.rs:19`）。計画の「変更ファイル一覧」に漏れなし。
- **`reset_size_guard`（`view.rs:1096`）は同一フレームの `drive_results_window`（`view.rs:1751`）より前**。不変条件 5 は実コードで成立。
- **`drain_launch`（`view.rs:1174`）は `reset_pending` 消費ブロック（`view.rs:1062-1098`）の後**。不変条件 6 は実コードで成立。
- **`take_clicked_for`（`view.rs:1711`）は snapshot publish（`view.rs:1684-1705`）の後**。不変条件 4 は実コードで成立。
- **`folder_gen` bump（`state.reset()`・`view.rs:1065`）は folder drain の `accept_folder_result`（`view.rs:1189`）より前**。不変条件 8 は実コードで成立。
- **SPEC.md 未言及の確認は正確**: `SPEC.md` を grep すると `egui_shell` は L92（`icon_textures.rs`）・L420（`create`）の 2 箇所のみで、`view.rs` も `SearchWindowView` も 0 件。「SPEC.md 更新要否＝不要」は妥当。
- **#666・#751・#752・#749 の issue 状態は claim と一致**（`gh issue view` 実測: #666 OPEN・#751 OPEN・#752 CLOSED・#749 CLOSED）。issue #666 のコメント本文と research.md/plan.md の「確定事実 7 件」は逐語で一致し、スコープの水増し・過小のいずれも無い。
- **`configure_japanese_font` の可視性**: `font.rs` も `view.rs` と同じく `mod.rs` 内で private `mod` 宣言される想定であり、`results_view.rs` は両モジュールの共通の親 `egui_shell` の descendant のため、現行と同じ到達性が保たれる（現に `view` も private だが `results_view.rs` から到達できている・`mod.rs:23,21`）。visibility 設計に問題なし。

## 2. 軽微な懸念

- **Phase 1 の「`mod` 宣言はアルファベット順の既存並びに合わせる」という前提が事実と異なる**。`mod.rs:8-25` の実際の宣言順は `icon_textures, lifecycle, search_state, layout, notify, strings, results_view, results_window, view, visual, window_coordinator` であり、`search_state` の後に `layout` が来る時点でアルファベット順になっていない（s → l で逆行）。「既存の並びに合わせる」という指示は実装者に「今はアルファベット順である」という誤った前提を与える。実害は小さい（`mod font;` をどこに置いても挙動は変わらない）が、指示の根拠が崩れているため実装時に迷いが生じうる。
- **不変条件 7 の「`drain_launch` の 3 分岐に `request_repaint` が無い」という文言は、字面どおり読むと不正確**。`drain_launch`（`view.rs:472-512`）の `Empty` 枝は内部でさらに if/else に分かれ、タイムアウト未達側で `ctx.request_repaint_after(...)`（`view.rs:497`）を呼んでいる。これは notice の deadline とは別目的（起動タイムアウトのポーリング用）であり、研究のいう「3 分岐」は notice.set を呼ぶ 3 経路（timeout 到達・Ok→Failed・Disconnected）を指すと解釈すれば矛盾はないが、plan.md の文言だけを読むと match の 3 arm 全体を指しているように読め、実装者が「あれ、request_repaint があるぞ」と誤検知しうる。`launcher_controller.rs` の `//!` を書く際は「3 分岐」の指す対象（notice を張る経路であって Empty match arm 全体ではないこと）を明示した方がよい。
- **PERFORMANCE.md の 2 箇所が計画の「変更ファイル一覧」から漏れている**（詳細は「3. 要対処」参照。実害が軽微なドキュメント散逸のみのため軽微側に分類する余地もあるが、実測件数の少なさから要対処へ計上する）。

## 3. 要対処

- **`PERFORMANCE.md` が `egui_shell/view.rs` を 2 箇所で名指ししており、計画の「変更ファイル一覧」に含まれていない**。`PERFORMANCE.md:157`「実装は `egui_shell/view.rs` の `font_covers_cjk` / `configure_japanese_font`」、`PERFORMANCE.md:179`「機構の正本は `egui_shell/view.rs` の `font_definitions` の doc コメント」。Phase 1 でこの 3 関数を `font.rs` へ移すと、この 2 行は実態と食い違う。`PERFORMANCE.md` は `.superpowers/sdd/**` や `docs/superpowers/plans|specs/**`（過去機能のスナップショットで対象外・研究側で正しく除外済み）と違い、「パフォーマンス最適化プレイブック」という現役の参照文書であり、`.claude/rules/governance-docs.md` 自身が「`PERFORMANCE.md` が消滅済みの節を指したまま腐っていた実例」を過去の教訓として名指ししている——今回まさに同型の腐り方が発生する。正準形（`` `<file>`「<見出し>」 ``）を使っていないため `governance:check` の G11 では検出されない。Phase 4（文書同期）の対象へ `PERFORMANCE.md:157,179` の参照更新を追加すべき。

## 4. 未検証（理由）

- **borrow checker の実現可能性（実コンパイルでの検算なし）**: `update()` 内で `self.controller.state()`（`&SearchState`）を保持したまま `self.controller` への `&mut` 呼び出しが必要になる箇所が実装時に借用エラーを起こすかどうかは、目視での NLL（non-lexical lifetime）推論に留まり、実際に `cargo check` は実行していない（計画のコード自体がまだ存在しないため実行不能）。手動解析では、疑わしい箇所（snapshot publish 直後の `rows: &[SearchResult]` を保持したまま `take_clicked_for` 経由で `activate_or_execute`（`&mut self.controller` 相当）を呼ぶ `view.rs:1684-1721`、および `self.shift_activate(self.state.selected(), &ctx)` のような「同じ受け手からの読みを引数に渡す」パターン `view.rs:1655-1661`）はいずれも (a) 借用の最終使用が可変呼び出しより前で終わる、または (b) two-phase borrow が救う構造になっており、素朴な分割では顕在化しないと考えられる。ただし `LauncherController`/`SearchWindowView` という 2 段の間接を挟むことで、現行の「同一 struct 内のフィールド単位の disjoint borrow」が「`self.controller` という 1 つの place を介した borrow」に変わる箇所があり、これは実装前の静的解析だけでは 100% の確証が持てない種類の変化である。**Phase 2 着手直後、`LauncherController` の骨格 + アクセサ 1 本を書いた時点で早めに `cargo check -p snotra` を回す**ことを推奨する（計画は Phase 2 全体の完了後にしか clippy/test を回さないため、借用エラーが出た場合の手戻り範囲が Phase 2 全体〔最大フェーズ〕に及ぶ）。
- **カテゴリ D の実機目視（5 件の破壊不変条件検知表）は本レビューでは実施していない**。コードがまだ書かれていないため対象が無く、静的なレビューの範囲外（実装後に人間が実施する前提が計画に明記されている）。
- **`cargo doc --workspace --no-deps --document-private-items` によるドキュメント内 intra-doc link 切れの実測は行っていない**（`//!`/`///` の移設を大量に含むため計画側もリスクとして認識済みだが、実際に壊れる箇所の特定はコード変更後でないと機械的に検出できない）。
