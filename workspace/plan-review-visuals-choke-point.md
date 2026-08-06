# plan-review: #949（計画準拠レビュー・観点 = 検知器の実効性 / 消える散文の着地）

## 要対処

- **移設で「未テーマ化の前置き」区間が `ui.interact` 1 箇所から `update()` のほぼ全域へ広がるが、この拡大した危険域に対する in-code の警告文の着地先が計画に無い（観点1の偽陰性 = 観点2の消える散文の未着地）**
  — 根拠: 現行の受容残余の警告（`src-tauri/src/egui_shell/view.rs:494-497`「**破っても検知されない**……上へ何かを挿入するときは『visuals を読まないこと』を確かめること」）は、適用点（`view.rs:503-510`）の**直前**というごく狭い区間（実質 `ui.interact`〔`view.rs:397`〕1 箇所だけ）を守るために、その適用点の**すぐ隣**に置かれていた。plan.md はこの適用点を `search_input_ui` の入口（呼び出しは現行 `view.rs:688`）へ移す。移設後、「テーマ未適用のまま `update()` が走る区間」は `ui.interact`（:397）から `search_input_ui` 呼び出し（:688）までの約 290 行（`consume_reset_pending` 処理・`consume_external_pending`・`frame.set_clear_color`・font hot-reload・`poll_async`・`read_pre_widget_input` 等）へ広がる——plan.md 自身が「不変条件と異常系」節（plan.md:65-69）でこれを「到達範囲を縮める（新しい受容残余）」と自認し、「現在の前置きは `ui.interact`（visuals を読まない）だけなので実害はゼロ」（現時点の事実としては grep 実測どおり正しい）と書く。しかし plan.md の変更ファイル一覧（plan.md:39-53）には、この**拡大した区間に対する in-code の警告**（＝将来 `update()` にこの区間で新しいウィジェットを足す編集者へ向けた、旧 `:494-497` と同格の注意書き）を**どこに書くか**が無い。挙げられている 3 箇所の整理対象（`:409-413` の列挙更新・`:660` の一語置換・`:480-502` 全体を `search_input_ui` の doc へ移設）はいずれも「search_input_ui 自身の内部順序」を語る場所であり、`update()` 側のこの拡大した区間そのものに立つ警告ではない。`src-tauri/CLAUDE.md:51`（「帰結が 2 つある」の段）も同じ `ui.interact` 例外を持つが、plan.md:52 は「検知器の所在（`search_input_ui` の doc）へ書き換え」としか書いておらず、この段が拡大した区間の警告を保つかは未確定。新設した検知器（5 変異表）はいずれも `search_input_ui` を `ctx.run_ui` で単独に駆動するテストであり、`update()` 全体を駆動しないため、**`update()` 側でこの区間に新しいウィジェットを足すことによる退行は原理的に捕まえられない**（5 変異のどれとも一致しない偽陰性）。
  — 推奨する修正: Phase 3 の作業項目へ「`update()` 内、`search_input_ui` 呼び出しの直前（現行 :687 相当）に、`旧 view.rs:494-497 相当の警告——ここより前で新しいウィジェット/子 Ui を作るなら、visuals を読まないことを確認するか、自分で visuals を渡すこと` を明記する」を追加する。CLAUDE.md の書き換え（plan.md:52）でも、`ui.interact` 例外の一文を「例外は `ui.interact` だけではなく、`search_input_ui` 呼び出し前の `update()` 全域である」と明示的に更新することを work item へ書き足す。

- **`:499-502`（`panel_fill` / `window_fill` 不使用の理由・同じ grep が `ctx.set_visuals` 撤去の根拠にもなっている、という 2 命題）の着地先が計画に無い**
  — 根拠: `view.rs:499-502`「**`panel_fill` / `window_fill` はここに無い**——読む egui コンテナ……が 1 つも無く、消費者ゼロの死んだ書き込みだった（spec 決定 2）。同じ grep が `ctx.set_visuals` を落としてよい根拠にもなっている」は、`:480-502` 全体を削除して `search_input_ui` の doc へ移す対象（plan.md:46）に含まれるが、plan.md の「search_input_ui の doc へ移す」内容（plan.md:148「#751 の機序・観測点の代価・到達範囲が縮んだこと」）にも、他のどの work item にもこの 2 命題は挙がっていない。うち「`ctx.set_visuals` を落としてよい根拠」は #900（`clippy.toml` の `disallowed-methods`）で禁止自体が機構化された今は実質的に無用の命題（コンパイルが先に赤くなるため、grep 根拠を読み返す必要がなくなった）だが、「`panel_fill`/`window_fill` を意図的に設定していない」という決定record（spec 決定 2 由来）はテーマ 3 値の話とは独立の事実であり、`search_input_ui` の doc という着地先には元々そぐわない（`search_input_ui` は `panel_fill`/`window_fill` を扱わない）。着地先が無いまま削除すると、将来 `panel_fill`/`window_fill` を「揃えるために」書き足す編集（このコメントが名指しで防いでいた事故）を止める記述がコードから消える。
  — 推奨する修正: この 2 命題は `search_input_ui` の doc ではなくモジュール doc `//!`（plan.md が既に touch 対象としている `:9-24`、work item は plan.md:48）または `visual.rs` 側へ移すことを明記する。「`ctx.set_visuals` の根拠」部分は #900 で機構化済みゆえ削除してよいと明示し、「panel_fill/window_fill 不使用」部分だけを移設対象として残す旨を work item へ追記する。

## 軽微

- 5 変異の iii（適用を `search_input_ui` から `update()` へ戻す）は、新テストが `search_input_ui` を `ctx.run_ui` で**直接**駆動する構造（`update()` を経由しない）ゆえ、実測上は ii-a + ii-b + ii-c を同時に当てた状態（3 値とも `search_input_ui` 内で適用されない）と機械的に区別が付かない。実際の編集ミスとしては ii 系と異なる意味（移設の巻き戻しという意図）を持つため記載する価値はあるが、「5 変異が 5 通りの異なるコードパスを検査している」という含意はやや過大——テストの discriminate power としては実質 4 種類である旨を「テスト方針」節に一言添えるとよい。

## 未検証
- なし
