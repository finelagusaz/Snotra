# plan-review: rust-search-state（ラウンド 3・最終）

担当: `src-tauri/src/egui_shell/search_state.rs` の純粋核テスト（Phase 2）。SPEC.md 文言（Phase 1）は担当外。

## 問題なし

- **観点1（0 件到着時の挙動）を一時テストで実測した**: `set_results` を 3 件 → `move_selection(2)`（selected=2）→ `set_results(vec![])` の順で呼ぶと、`selected()==0`・`results().len()==0` になることを `cargo test -p snotra search_state::tests::tmp_probe_zero_arrival` で確認（緑）。機構は `clamp_selected`（search_state.rs:374-380）の `len==0` 分岐。計画が書こうとしている「0 件なら選択対象なし」というアサーションは成立する。一時テストは削除済みで `git status --short` は `?? workspace/` のみ（追跡ファイルへの差分なし）。
- **既存テストとの重複なし（0 件ケース）**: `set_results(vec![])` を呼ぶ既存テストは grep 0 件。`move_selection_on_empty_is_zero`（search_state.rs:617-622）は `move_selection` 自身の空早期 return（`if self.results.is_empty() { self.selected = 0; return; }`）を測るだけで、`set_results`/`clamp_selected` の `len==0` 分岐は通らない。`set_results_clamps_selection`（search_state.rs:595-603）は 3→1 の縮小のみで 0 件遷移を含まない。→ 0 件ケースは真に新しいブランチカバレッジであり重複ではない。
- `launcher_controller.rs` に `mod tests` が 0 件であることを grep で再確認（前ラウンドの確認は変わらず妥当）。

## 軽微な懸念

- テスト4（`arriving_rows_clamp_selection_without_resetting_it`）に 0 件ケースを同居させると、「非ゼロ→非零へクランプ（縮小）」と「非ゼロ→0（0 件到着）」が同一テスト内に混在する。機構は同じ `clamp_selected` 関数のため実装上は自然だが、後述「要対処」のとおり命名との整合は要検討。
- テスト方針の境界条件 (e) は「行が減らない場合／減る場合／0 件」の 3 パターンを挙げるが、Phase 2 のテスト4項目の文面は「戻らずクランプされるだけ」としか書いておらず、3 パターンすべてを 1 テストに書くのか代表 1〜2 パターンで足りるのかは実装者判断に委ねられている。ただし後述の「要対処」（数値設計）を満たせば実質的にこの懸念は解消する。

## 要対処

1. **テスト4の数値設計が SPEC の主張を証明しない組み方に流れうる**: テスト4の存在意義は「常に先頭行（＝毎回 0 へ戻る）」が偽であることの証明（doc コメントにもそう明記される計画）。しかし縮小の組み方次第では証明にならない——既存 `set_results_clamps_selection` と同じ「3 件→1 件」パターンを流用すると、`clamp_selected(1, 2) == 0` となり、selected は 0 になる。これは「クランプ」でも「無条件リセット」でも結果が同じ 0 になるため、**「常に先頭行ではない」ことの反証として機能しない**（既存テストの計算を踏襲すると無自覚に証明力のないテストを書いてしまう）。
   - 代案: 「5 件中 selected=3 → 4 件到着（クランプ後も 3 のまま残る）」のように、**クランプ後も非ゼロで残る組み合わせ**を最低 1 つ含めるよう Phase 2 の当該項目に明記する（`clamp_selected(4, 3) == 3` を数値的に確認済み）。縮小して 0 件未満まで落ちる「減る場合」パターン（例: 3→2 で selected 2→1）も併記すれば (e) の 3 パターンを 1 テストで包含できる。
2. **名実の不一致（テスト名 `..._without_resetting_it` に 0 件ケースを同居させる件）**: 0 件到着では `selected()` は必然的に 0 になる。これは「reset_selection のような無条件 0 復帰」ではなく「クランプ対象自体が存在しない」という別の理由による 0 だが、テスト名だけを読む後続の読者には「結局 0 に戻るケースがあるのか」と読める——ファイル内の他のテストが際どい非対称を doc コメントで明示する慣習（例: `rows_generation_is_stable_on_selection_change` の「進めすぎの側」注記）に照らすと、素通りしにくい。
   - 代案A: 0 件ケースを 5 本目のテスト（例: `arriving_zero_rows_leaves_no_selection`）に分離し、doc コメントに「これは reset_selection の無条件 0 復帰ではなく、選択対象が存在しないことによる 0 である」と明記する。この場合 Phase 2 冒頭・テスト方針・不変条件で計画中に複数箇所ある「4 本」という表記（plan.md 20 行目・51 行目・90 行目）を「5 本」へ直す必要がある。
   - 代案B: 同居させたまま、0 件ケースの assert 直前に同旨のコメントを 1 行添えて名実の不一致を文書で埋める（テスト本数は 4 本のまま）。
   - どちらでも良いが、**計画には現状この対処がなく実装時の場当たり判断に委ねられている**ため、ラウンド3の指摘として残す。

## 未検証

- SPEC.md §6.1 の文言そのもの（Phase 1・「先頭の候補」という語の選び方・§6.4 との語衝突回避）— 担当レイヤー外（`spec-doc` の担当）。
- テスト1〜3（`left_twice_climbs_two_levels` / `left_at_drive_root_is_noop` / `folder_navigation_resets_selection_to_first_row`）の実装可否 — 前ラウンドで一時実装 46 件緑と `←` 分岐を殺しても緑という射程確認が既に実測済みとの前提のため、本ラウンドでは差分（0 件ケース）にのみ再実測を絞った。この3本を今回改めて一時実装して再実測してはいない。
- Phase 4（`cargo test -p snotra` / `npm run governance:check`）は実装後にしか回せない検証であり、計画の記述の妥当性以上は確認していない。
