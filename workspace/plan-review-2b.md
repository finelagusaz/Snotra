# plan-review Step 2b: #714 独立導出（レビュアー成果物）

日付: 2026-07-26 / 対象 issue: #714「再検索時、結果窓が前のスクロール位置から先頭までアニメーションする（瞬時に先頭であるべき）」

導出の前提: issue 本文 + コードのみから独立に導出した（`workspace/plan.md` / `workspace/research.md` は未読）。

## 1. 要件の理解（WHAT）

- **結果集合が総入れ替えされたフレーム**（= `RowsSnapshot.generation` が進んだフレーム）では、選択行への位置合わせを**アニメーションなしで瞬時に**行う。現状は `scroll_to_me(None)` が egui の `ScrollAnimation`（1000pt/s・0.1〜0.3s）を経由し、前のリストのスクロール位置から視覚的に流れる。
- **選択行の移動（↑↓）でのスクロールはアニメーションのまま**残す。「行の差し替え」と「選択の移動」を同じ経路で扱わないことが要点。
- 経路の分類材料は既に揃っている: `SearchState::rows_generation` は「行の差し替えでだけ進み、`move_selection` / `reset_selection` では進まない」ことがユニットテストで固定済み（`rows_generation_is_stable_on_selection_change`）。つまり「世代が進んだフレーム = 瞬時」「世代同一で selected だけ変わったフレーム = アニメーション」という判別が、追加の状態なしで snapshot から読める。

## 2. 採るべき案とその理由

**結論: (B) の変種 — ただし `Style::scroll_animation` の書き換えではなく、egui 0.35.0 の per-call API `Response::scroll_to_me_animation(None, egui::style::ScrollAnimation::none())` を世代変化フレームだけ使う。**（呼び名: B′）

egui 0.35.0 の実装を確認した根拠（`~/.cargo/registry/.../egui-0.35.0`）:

- `response.rs:822-827` — `scroll_to_me(align)` は `scroll_to_me_animation(align, ctx.global_style().scroll_animation)` の糖衣。**アニメーションを呼び出しごとに指定できる公開 API が既にある。**
- `style.rs:855-861` — `ScrollAnimation::none()`（`points_per_second: INFINITY, duration: 0..=0`）が存在する。
- `scroll_area.rs:1103-1160` — scroll target の消費時に**呼び出し側が渡した animation がターゲットへ焼き込まれる**。ゆえに style を触らずに 1 回の scroll だけ instant にできる。

### 各案の判定

- **(A) 世代変化フレームに `vertical_scroll_offset(0.0)` — 不採用。** 2 つの欠陥がある。
  1. **selected≠0 の世代変化で誤る。** `on_escape`（tool/folder からの復帰）は `restore_selected` を復元しつつ世代を進める。#633 の index 完了再検索も `set_results` が selected をクランプ保持したまま世代を進める。これらは「新しいリスト＋選択は深い行」であり、offset を 0 に固定すると選択行が見えない（そして同フレームの `scroll_to_me` が 0 から選択行までまたアニメーションする＝症状の再生産）。正しい目標は「offset=0」ではなく「**選択行が瞬時に見えている**」である。
  2. `scroll_area.rs:741-742` のとおり builder offset は `state.offset` を書くだけで、**in-flight の `offset_target`（前フレームの ↑↓ アニメーション）を消さない**。直後のフレームで stale なターゲットへ動き直す余地が残る。
- **(B) 世代変化フレームだけ `Style::scroll_animation = 0` にして `scroll_to_me` — 趣旨は正しいが形が悪い。** ctx の style 書き換えは「戻す」帳簿が要り、イベント駆動 runtime（フレームがいつ走るか制御できない）では戻し忘れ・戻し早すぎの窓ができる。per-call API が同じ効果を状態なしで与える。
- **`ScrollArea::animated(false)` を世代変化フレームだけ渡す — 不採用。** `scroll_area.rs:1139-1141` の `!animated` 分岐は新しい delta を即時適用するが、**既存の `offset_target` を更新も破棄もしない**。↑↓ アニメーション中に再検索が完了すると、次フレームで旧リスト向けの stale ターゲットへスクロールする。B′ は同じ状況で `animation.target_offset` が新ターゲットへ更新される（`scroll_area.rs:1145-1148`）ため安全側。

## 3. 必要な変更集合（ファイル + シンボル + 1 行説明）

変更は **`src-tauri/src/egui_shell/results_view.rs` 1 ファイルに閉じる**（`draw_result_row` の呼び出し元はこのファイルの `update()` 1 箇所のみ・grep で確認済み）。

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/results_view.rs` | 新規 `enum ScrollRequest { None, Animated, Instant }`（名称任意・`pub(crate)`） | 「scroll しない / 選択移動の追従 / リスト切替の瞬時位置合わせ」の 3 値を型で区別する。bool 2 個（`scroll` + `instant`）だと不正組合せ（false,true）が表現可能になるため enum を推す |
| 〃 | `draw_result_row`（`scroll: bool` 引数） | `ScrollRequest` を受け、`Animated` → `response.scroll_to_me(None)`（現行どおり）、`Instant` → `response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none())`。align は両方 `None`（`block:"nearest"` parity・#532 SU6.5 を維持） |
| 〃 | `ResultsView::update()` の世代検知ブロック（`snapshot.generation != self.last_generation`） | 検知結果を `generation_changed: bool` としてフレーム内に保持し、行ループで `sel && do_scroll` のとき `if generation_changed { Instant } else { Animated }` を渡す |
| 〃 | （推奨・任意）gate の純粋核抽出: `ScrollGate { last_scrolled_selected: Option<usize>, last_generation: u64 }` + `fn decide(&mut self, generation: u64, selected: usize) -> ScrollRequest` + `fn reset_for_hide()` | 現在 `update()` にインラインの gate 判定（`last_scrolled_selected` / `last_generation` の 2 フィールド）を 1 型へ寄せ、下の判定表をユニットテストで固定する（このリポジトリの「純粋核 + テストが唯一の担保」流儀。実機ではアニメーション有無を自動検証できない） |
| 〃 | `tests` モジュール | gate 判定のテスト追加: 世代変化×selected 同値 → Instant（#632 Fix 3 と #714 の重ね合わせ）・世代変化×selected 変化 → Instant・世代同一×selected 変化 → Animated・両方同一 → None・hide リセット後の再判定 |
| 〃 | doc コメント（`RowsSnapshot.generation`・`last_scrolled_selected`・`last_generation`・`draw_result_row` の rustdoc、`update()` 内 #632 Fix 3 コメント） | 「世代リセット = scroll gate 再発火」に「世代変化フレームは instant（#714）」の一文を追随させる |
| `src-tauri/src/egui_shell/search_state.rs` | `rows_generation` フィールドの doc コメント（144-153 行付近）のみ・コード変更なし | 「`move_selection` / `reset_selection` で進めてはならない」理由に #714 を追記（進めると ↑↓ でも instant になりアニメーション要件が黙って壊れる——この不変条件が本修正の要件その 2 を担う耐久線になった）。既存テスト `rows_generation_is_stable_on_selection_change` がそのまま検知器 |

### 変更不要と判断した箇所（読んだ上での「触らない」列挙）

- `view.rs`（snapshot 発行・`take_clicked_for` 消費・1159 行の gate 所在コメント）: 発行側は世代を既に運んでおり不変。1159 行のコメントも真のまま。
- `RowsSnapshot` / `ResultsShared` / `ClickTake`: 構造変更なし（世代は既に snapshot に載っている——このバグの検出材料は #699 で整備済み）。
- `SPEC.md`: スクロール挙動への言及は 172-173 行（高さ・スクロールバー）のみで、アニメーション有無は文書化されていない。文書化された挙動を変えないため SPEC 同期は必須ではない（AGENTS.md「fix でも文書化された挙動を変えたら仕様変更」に該当しない）。任意で §8 の results 窓記述に 1 行足すのは可だが、必須にはしない。
- `src-tauri/CLAUDE.md`: `results_view.rs` の責務行にスクロールの記述なし → 追随不要。
- `docs/superpowers/specs/*` / `plans/*` の `scroll_to_me` 言及（SU4/SU6.5 設計記録）: 日付付きの歴史記録であり書き換えない。

### 同概念・別名の間接参照の分類（洗い出し結果）

「世代」を名乗るカウンタは 4 つあり、**今回触るのは rows 世代だけ**である:

1. **rows 世代**（`SearchState::rows_generation` / `RowsSnapshot.generation` / results の scroll gate・クリック照合）— 本修正の対象概念。上表のとおり doc 追随あり。
2. `EguiShellState.hotkey_generation` — alt 解放待ち show の無効化。無関係・触らない。
3. `AppState.index_generation` — index build 完了検知（#633）。触らないが、**この経路が `run_search` → `set_results` を呼び rows 世代を進める**ため、下のエッジケース 6 として挙動確認の対象。
4. `SearchState::folder_gen` — folder ナビの遅着結果 token。無関係・触らない。

「snapshot」も 2 概念（`RowsSnapshot` / `VisualSnapshot`）あるが後者は無関係。egui 側の「ScrollAnimation」はグローバル style（`ctx.global_style().scroll_animation`）を**変更しない**（B′ の要点）。

## 4. エッジケース（世代が進む全経路と判定）

`rows_generation` を進めるのは `SearchState` の 4 メソッドのみ（フィールド private・型の外から `results` を差し替える経路なし）:

| # | 経路 | selected | 期待挙動（B′ での結果） |
|---|---|---|---|
| 1 | `set_results`（打鍵再検索。driver が `view.rs:1513` で毎打鍵 `reset_selection()`） | 0 | **issue の本命**。瞬時に先頭。 |
| 2 | `set_results`（folder 一覧到着・folder filter・instant 行・空クリア） | 0（navigate/enter で 0 化） | 瞬時に先頭。 |
| 3 | `enter_tool`（ツール一覧への総入れ替え） | 0 | 瞬時に先頭。 |
| 4 | `on_escape`（tool → 退避行復帰） | **restore_selected ≠ 0 がありうる** | 瞬時に**復元選択行**へ（offset 0 ではない——(A) 却下の根拠）。 |
| 5 | `on_escape`（folder → 展開前復帰） | 同上 | 同上。 |
| 6 | `index_generation` 検知 → `run_search` → `set_results`（#633。selected はクランプ保持） | **≠ 0 がありうる** | 瞬時に現選択行へ。行構成がほぼ同じなら delta≈0 で無スクロール（現状と同じ見え方）。 |
| 7 | `reset`（resetForShow） | rows 空 → results 窓非表示 | 描画なし。次に rows が非空になるのは経路 1〜3 のいずれかで、必ず世代が進んでいる → 再表示初回は Instant。#632 の「再表示後に確実に一度 scroll し直す」不変条件は維持される。 |
| 8 | **hidden 中の世代進行**（hidden 中は results の `update()` が走らない・SU5 要石。複数回 bump が未観測のまま溜まる） | 任意 | 再表示初回フレームで `snapshot.generation != last_generation` が 1 回だけ成立 → Instant が 1 回。差分比較（`!=`）なので取りこぼしも二重発火もない。 |
| 9 | 世代同一・↑↓ で selected のみ変化 | 任意 | **Animated のまま**（要件その 2）。`rows_generation_is_stable_on_selection_change` が構造的に保証。 |
| 10 | 世代同一・ユーザーのホイールスクロール | 選択不変 | gate 不成立（`last_scrolled_selected == selected`）→ scroll_to_me 自体が発火しない。手動スクロールの尊重は現状維持。egui 側もホイール入力で `offset_target` を破棄する（`scroll_area.rs:1248`）。 |
| 11 | **↑↓ アニメーション進行中に再検索が完了**（世代変化が in-flight アニメーションと重なる） | 0 | `offset_target` が Some のため egui は既存アニメーションの残り時間スパンを保ちつつ target だけ新位置へ更新（`scroll_area.rs:1145-1148`）。**残り ≤0.1〜0.3s の減衰が理論上残る**が、目標位置は正しく、発生条件（↑↓ 押下から ~0.1s 以内に debounce+検索完了）も狭い。受容する残余として記録する。 |
| 12 | 世代変化と selected 変化が同一フレームに同時発生 | 任意 | Instant が勝つ（リスト切替が支配）。issue の分類「別のリストへの切り替えは持ち越さない」と整合。 |
| 13 | `show_results` ゲートだけが false→true に戻る（世代不変の再表示があれば） | 不変 | 空 rows フレームで gate がリセット済みのため Animated で 1 回寄せ直す。#714 の射程外（リスト切替ではない）・現状挙動維持。 |

## 5. 落とし穴・注意点

1. **`scroll_to_me` の効果はもともと 1 フレーム遅れで現れる**（target 登録はフレーム末尾、offset 適用は次フレーム冒頭・`scroll_area.rs:904-926`）。Instant でも「新リストを旧 offset で描いた 1 フレーム」が挟まるが、これは現行アニメーション経路と同一の機構であり `request_repaint` が即時に次フレームを起こす。0 フレーム化を狙って `vertical_scroll_offset` に手を出さないこと（(A) の欠陥に逆戻りする）。
2. **`ctx.style_mut` で `scroll_animation` を書き換えない**（(B) の字義どおりの実装をしない）。戻し忘れると ↑↓ のアニメーションが恒久に消え、要件その 2 を静かに破る。per-call API で状態を持たないのが B′ の核。
3. **`move_selection` / `reset_selection` で世代を進めない**という既存不変条件が、#699（クリック照合）に加えて #714（アニメーション温存）の耐久線になる。`search_state.rs` の doc に理由を 1 つ追記して将来の変更者に見せる。
4. `draw_result_row` の引数追加は既存の `#[allow(clippy::too_many_arguments)]` の範囲内だが、bool を並べず enum で渡す（不正状態の排除と呼び出し側の可読性）。
5. **検証は視覚スモークが必須**（アニメーションの有無はユニットテストで観測できない）。`cargo run -p snotra` で (a) スクロールを要する位置まで ↓ → 別キーワード再検索 → 瞬時に先頭、(b) ↑↓ でのスクロールが従来どおり滑らか、(c) folder/tool から Escape 復帰時に復元選択行が瞬時に見える、の 3 点を確認する（`docs/build-commands.md` カテゴリ D）。純粋核（gate 判定）はユニットテストで固定する。
6. 副次効果の検証（任意）: issue が挙げる #710（高フレームレート区間）は、再検索直後の 0.1〜0.3s 連続描画が消えることで一部説明がつくはず。SNOTRA_TRACE でフレーム数を前後比較すれば裏が取れるが、#714 の完了条件には含めない。
