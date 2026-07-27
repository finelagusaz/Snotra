# L3 文書同期レビュー — #749 段1 WindowCoordinator

対象: `workspace/plan.md`（`src-tauri/CLAUDE.md` / `docs/architecture.md` / `visual.rs` doc コメント / SPEC.md 更新要否判断）

## 問題なし

1. **`mod.rs::` 名指しの数え上げは完全一致**。リポジトリ全体を grep した結果、production コード中で `mod.rs::` 形式のモジュール名指しは 3 か所のみで、plan.md が挙げる箇所と一字一句一致する。
   ```
   src-tauri\src\egui_shell\visual.rs:5://! （`mod.rs::position_results_below_main`）は main の `update()` と `Moved` リスナーの
   src-tauri\src\egui_shell\layout.rs:102:/// results 窓の上端の**物理** y（#752 C1）。`mod.rs::position_results_below_main` の算術部。
   src-tauri\src\egui_shell\layout.rs:118:/// `mod.rs::results_available_height` の算術部。
   ```
   （`workspace/plan.md` / `workspace/research.md` 自身のヒットは検証対象の計画書自体であり除外）。`docs/` `.claude/` `SPEC.md` 各 `CLAUDE.md` にはこの形式の名指しは 0 件（同 grep で確認）。**plan.md の 3 か所列挙は網羅的である。**

2. **`drive_results_window` の md 参照も plan.md の列挙どおり**。`SPEC.md` は 430 行の 1 か所のみ、`docs/architecture.md` は 83 行（駆動主体の散文）と 172 行（mermaid シーケンス図内、`linesOutsideFences` の対象外＝governance G2 の対象にもならない）の 2 か所のみ。他に `.claude/` 配下・ルート `CLAUDE.md` / `AGENTS.md` には出現しない。
   - `docs/superpowers/` 配下 5 ファイル（`specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` 等）にも出現するが、**同ディレクトリの `README.md` が「歴史資料であり、現在の仕様ではない」「`governance:check` はこのディレクトリを検査しない」「ここの記述と実装の乖離は欠陥ではない」と明記**（#589）。実際、これらの歴史文書はすでに `view.rs:788` のような当時の行番号や `last_results_visible`（#671 PR A′ で撤去済み）等、現在の実装と乖離した記述を残したまま更新されていない。plan.md の「歴史記録ゆえ更新しない」判断は既存の運用と完全に一致する前例がある。

3. **SPEC.md §8.5/§8.6/§8.7 は挙動記述として不変**。実際に読んだところ:
   - §8.5 (430行): 「`results` は... `main` の毎フレーム更新（`drive_results_window`）が駆動する」— plan は関数名を変えず、呼び出し元（`view.rs` の `update()` 内、Phase 4 で `crate::egui_shell::drive_results_window(&self.app_handle, ...)` として同一箇所から呼ぶ）も変えないため、記述は文字どおり真のまま
   - §8.6 4連言（511行 `results 可視 ⇔ main 可視 ∧ ... ∧ 窓高さ > 0`）: 判定関数 `layout::present_results` は plan で無変更（Phase 1 で追加する `size_delta_exceeds` は判定式ではなく `set_size` 呼び出し要否のデルタガードであり、4連言のどの項にも属さない別概念）
   - §8.7 ライフサイクル表: 生成/表示/非表示/破棄の各段階の記述に影響する変更は plan に無い（reset-on-show の呼び出し位置は Phase 2 で `view.rs` 内に残す方針であり、スレッド同一性を含め現状維持と明記されている）
   
   **SPEC.md 更新不要の判断は妥当。**

4. **観点7（受け皿issue）**: `gh issue view 666` で本文を実読した。「- 責務に応じて分割して、見通しをよくする」の 1 行のみで、research.md の要約（「モジュール割り・ファイル名の指定は無い」）と一致。plan.md が段3へ委ねる「managed state の構成」以外に、名指しの無い deferred 項目は plan.md 中に見当たらない。

5. **カテゴリ D の位置づけ**: `docs/build-commands.md` のカテゴリ D トリガー文言（「UI のスタイル・レイアウト・テキスト表示に影響する変更」）とは字面上一致しないが、plan.md は「issue が『カテゴリ D の目視を必須とし、見るべき項目を PR 本文に列挙する』と要求している」と根拠を issue 側に明示しており、誤ったカテゴリ帰属ではなく issue 由来の上乗せ要求として透明に扱われている。

## 軽微な懸念

1. **「ここはファイル名の索引」という記述は既存慣行よりやや控えめ**。`src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 段落は、実際にはディレクトリ集約カッコ内の裸のファイル名列挙（`mod.rs` + `lifecycle.rs` / ... / `visual.rs`）に加え、**同じ一続きの文中で各ファイルへ一言責務要約を必ず添えている**（例: `results_window.rs` は results 窓の所有型（生 Win32 の show/hide/topmost と可視フラグを 1 つの型が同時に持つ・#671 PR A′)）。段2（#752・PR #756・commit a98312c）の実差分でも同形式でこの一文全体を書き換えている（`layout.rs` の要約に `present_results` 等を追記）。plan.md の「ファイル名の索引」という表現は G1（バッククォート basename の存在）だけを満たせばよいと読めてしまうが、既存慣行に倣うなら `window_coordinator.rs` にも一言要約（Phase 3 で書く `//!` の要約でよい）を同じ文中に添えるべきである。実装時に既存文の書式を見ればほぼ自明に倣えるはずだが、plan.md の文言だけでは「バッククォートで名前を置くだけ」と誤読されうる。

## 要対処

1. **`src-tauri/CLAUDE.md` の `view.rs` 要約が「results 窓 driver」を残したまま stale になる**。現行の「モジュール構成」段落は次のとおり `view.rs` の責務に明示的に「results 窓 driver」を含めている:
   > `view.rs` は検索 view（TextEdit・キーボードナビ・起動・RowsSnapshot 発行・**results 窓 driver**・indexing 案内overlay）

   plan.md の Phase 4 で `drive_results_window`（= results 窓 driver 本体）を `view.rs` から `window_coordinator.rs` へ移す（`view.rs` に残るのは 1 行の呼び出しのみ）。しかし plan.md の Phase 5「文書同期」は
   > 「モジュール構成」の `egui_shell/` 一覧に `window_coordinator.rs` を追加...**`mod.rs` の説明から**移した責務を落とす

   と **`mod.rs` の記述訂正しか明示していない**。`view.rs` からも同じ責務（driver 本体）が出ていくため、`view.rs` の要約からも「results 窓 driver」を落とす（または「results 窓 driver の呼び出し」等へ弱める）必要があるが、plan.md はこれを名指ししていない。governance:check の G1 はバッククォート basename の存在しか見ないため、この種の「文中の役割語が実体と食い違う」drift は機械検査に掛からず、実装者が Phase 5 の文言（「mod.rs の説明から」）を字面どおり読むと `view.rs` 側の訂正を見落とす経路がある。

2. **`mod.rs` 自身の `//!`（module doc・#562 の正本）が更新対象に入っていない**。現在の `mod.rs` 冒頭:
   ```rust
   //! egui/softbuffer メインウィンドウの外殻（#532 SU2〜SU7・唯一の UI 経路）。
   //! window 生成・show/hide・blur 自動非表示・位置永続。
   ```
   Phase 3 で `show_egui_main` / `hide_egui_main` / `save_placement_relative` / `register_hide_listener` を `mod.rs` から `window_coordinator.rs` へ移すと、上記 `//!` が主張する「show/hide」「位置永続」「（blur 自動非表示の同期先である）hide listener 登録」の実体が `mod.rs` から失われる。`src-tauri/CLAUDE.md`「モジュール構成」冒頭は「責務を持つ個別モジュールの責務宣言は各ファイルの `//!`（module doc）を正本とする」と明記しており、CLAUDE.md 側の要約文（「`mod.rs` の説明から移した分を落とす」で対処予定）と `mod.rs` 自身の `//!` の**両方**が同じ内容を主張しているため、**CLAUDE.md だけ直して `mod.rs` の `//!` を直さないと、正本であるはずの `//!` が古い記述のまま残り、CLAUDE.md との間に新たな不整合が生まれる**。plan.md のファイル一覧では `mod.rs` は「上記 8 関数を削除し `mod window_coordinator;` + `pub(crate) use` の re-export を足す」としか書かれておらず、`//!` の更新はどのフェーズにも明示されていない。governance:check（G1/G2/G3/G11 いずれも）は `//!` の文言内容までは検査しないため、これも機械検査に掛からない drift になる。

## 未検証（理由）

- **Phase 3 で新設する `window_coordinator.rs` の `//!`（plan.md 記載の草稿）が `src-tauri/CLAUDE.md` の一言要約と最終的にどう整合するか**は、実装時の具体的な文言確定を待たないと判定できない（草稿自体は #562 の慣行に沿っており妥当に見えるが、CLAUDE.md 側の一文への反映は「要対処 1」の訂正と合わせて実装時に確認が要る）
- **`npm run governance:check` を実際に実行して G1（モジュール索引）が新規ファイル追加後に green になるか**は、実装前の現時点ではコード変更が無いため実行確認していない（plan.md が Phase 5 で明示的に実行を予定しており、手順自体は妥当）

## チェックリスト（観点1〜8）

1. 参照の数え上げ（`mod.rs::`） — 確認済み・plan.md の3か所列挙で全件（問題なし1）
2. `drive_results_window` の全参照 — 確認済み・SPEC.md:430 / architecture.md:83,172 で全件、`docs/superpowers/` は前例どおり除外妥当（問題なし2）
3. SPEC 更新要否の妥当性 — §8.5/8.6/8.7 を実読し、挙動記述に影響なしを確認（問題なし3）
4. モジュール索引の規約 — 追加自体は妥当だが、`view.rs` 側の訂正漏れと `mod.rs` 自身の `//!` 未更新を検出（要対処1・2、軽微な懸念1）
5. `governance-check.mjs` の検査対象 — G1（モジュール索引双方向）・G2（architecture.md のファイル表再導入検査、対象外を確認）を実読して具体的に説明（問題なし相当。ただし `//!` の文言正確性そのものは G1〜G11 いずれの対象でもないことを「要対処」の根拠に使用）
6. 検証カテゴリの写像 — A(cargo doc必須)/C(smoke)/D(issue要求)/F(governance:check) いずれも `docs/build-commands.md` の記述と整合、D のみ字面上のトリガーとは別根拠だが透明に説明されている（問題なし5・軽微な懸念は無し）
7. スコープの受け皿 — `gh issue view 666` で実読・plan.md/research.md の要約と一致、他に無名の deferred 項目なし（問題なし4）
8. 未検証 — 上記2件を記載
