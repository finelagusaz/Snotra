# plan-review: 参照の行き先の完全性 / 命題の落ち（#1336）

対象 issue: #1336。対象計画: workspace/plan.md。観点は 2 つだけ（参照の行き先・命題の落ち）。

## 要対処

1. **SPEC.md:627 が計画の表に無い。** 「`src-tauri/CLAUDE.md`「可視性を変える操作はイベントループスレッドに閉じてある」の bullet 群が正本」（SPEC.md:627）。計画の SPEC 行は `SPEC.md:767` だけ。この見出しは本書から消えるので参照が行き先を失う。X9 を `hide_egui_main` doc へ移すなら、`state.rs:37`・`layout.rs:453`・`results_window.rs:28,122` と同じ行に足す。「触るファイルの全パス」の SPEC.md 行にも理由が増える。
   - これは bullet 群をリードにした正準形。G-heading-refs が拾えるかは未確認（拾うなら CI で赤になる。拾わなければ沈黙で壊れる）。どちらでも付け替えは要る。
2. **PERFORMANCE.md:108 が計画の表に無い。** 「現行の回収機構は `src-tauri/CLAUDE.md`「working set の能動回収」」。計画は `PERFORMANCE.md:87` だけ。節を消すなら :108 も `src-tauri/src/working_set.rs` の `//!` へ付け替える。
3. **docs/architecture.md:247 が計画の表に無い。** 「射程と、規範を守る機構は `src-tauri/CLAUDE.md`「モジュール構成」の当該条項が正本」。計画は `architecture.md:61`（パスだけ）しか扱っていない。
   - 「モジュール構成」見出しは残るので、G-heading-refs は緑のまま。指す中身（#1032 条項）が消えるので、沈黙で壊れる典型。
   - `AppState::read_config` の doc へ付け替える。:61 の「モジュール構成は `src-tauri/CLAUDE.md`」は索引の指しなので残してよい。
4. **`hide_request.rs:5-6` の行き先が半分失われる。** 「`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」の「可視性を変える操作は…」」。計画は hide_request.rs を「残る見出しを指す、触らない」とするが、後半の「可視性を変える操作は…」は X9 の bullet のこと（実際は「Win32 / Tauri 注意事項」の下にあり、元から誤帰属）。
   - 見出し「ウィンドウ生成の制約」は残るので緑のまま、行き先の無い断片が残る。
   - `hide_egui_main` doc への付け替えか、断片の削除が要る。ファイルは「触る」側へ移す。
5. **命題の落ち（X8 / 「新しい可視性操作にも `&EventLoopProof` を要求する」）。** 本書 185 行の「新しい可視性操作にも `&EventLoopProof` を要求する／証人を引数から外してはならない（外した瞬間に…が再び構築可能になる）」。
   - research は X8 を「1（変異で確認: E0277）」とした。しかし変異は「別スレッドから呼ぶ」だけで、「証人を外す」「新操作に証人を付け忘れる」は型で捕まらない（引数を消せばそのままコンパイルが通る）。
   - 計画に X8 の行動が無く、命題が消える。`show_egui_main` の doc に「外してはならない」があるとの記述が research にあるが、「新しい可視性操作にも要求する」という新規側の指示は未確認（`window_coordinator.rs:273` 付近は証人の説明のみ）。
   - X9 の移設先 `hide_egui_main` doc に 1 文足すか、4 で残すかを決める。
6. **命題の落ち（X5 の「表現不能化ではない」側）。** 計画の X5 行は「全部 tao か全部 raw・main=tao/results=raw・main の show だけ raw は禁止」のみ。
   - `results_window.rs:12-13` の `//!` に「Manager から生ハンドルで `.hide()` は no-op」はあるので 3= で乗る。ただし移設行に書かれていない。
   - 計画に「既にある」と明記するか、確認済みと書く。
   - 同様に `apply_diff` が `VISIBLE` 無しのウィンドウへ `SW_HIDE` を撃つ機序は `results_window.rs:242-246` が持つので問題ない。

## 軽微

1. **R4 削除の確認。** 裁定済みだが、本書 102 行の「`launcher_controller` の `/history` 枝と `record_folder_expansion` が錠越しに待つ」という実在事実は、どこにも残らなくなる。裁定どおり受け入れるが、削除台帳に「消してよい理由」を書くこと。
2. **I5 のフィールド doc 範囲。** 計画は「書くのは `try_begin_index_build` / `finish_index_build` / `AppState::new` だけ」と M1 だけ。
   - 本書 115 行の「2 フラグは別物（`indexing` は first-run でビルドスレッド不在でも true になる UI 表示用／`index_build_started` は CAS 専用）」は、`state.rs:252-254` のテスト内コメントと `commands/window.rs:33` にしか無い。フィールド doc へ 1 句足すと確実。
   - 現在 `pub indexing` / `pub index_build_started`（state.rs:26-27）にフィールド doc は無い。
3. **X4 の一般則。** 「宣言的属性で挙動を代替する判断は、その属性を読む側の全分岐を確かめてから」という一般則は、コード側では `results_window.rs:108-111` が機序（`SW_SHOW` が活性化する・`MARKER_DONT_FOCUS`）として持つだけ。research の 3= は機序については正しい。一般則は消える（5 相当）。台帳に明記するとよい。
4. **S2 の付け替え先。** `window_coordinator.rs:530` は「trim は hide 前後どちらで走っても無害」の出典として CLAUDE.md を指す。付け替え先の `working_set.rs` `//!` にはその 1 文が無い（`window_coordinator.rs:545` の本文は自前で持つ）。530 は指しを消して本文に任せるのが自然。計画の「`working_set.rs` の `//!` へ」は :530 では内容が来ない。
5. **M14 の末尾の命題。** 「バー矩形の物理サイズは `read_frame_geom` が導く差分の上で `logical_to_phys` が導く／show が `outer_size()` を読み戻す経路は消えた」の置き場が計画に無い。`ADR-show-path-derives-bar-rect` と `show_egui_main` 本体コメント（`window_coordinator.rs:295-300`）が経緯を持つので、5 でよい。
6. **`docs/development-principles.md:76`。** 例示として「`config の読みは read_config を通す`」と本書の見出し語を引く。参照ではないが、見出しが消えると例の出典が無くなる。触らなくてよいが、sweep に出るなら「触らない」側へ一覧する。
7. **`.claude/rules/src-tauri.md` の PlatformBridge 行。** 「個別の理由 → 各直呼び箇所の doc」と計画は書く。直呼び箇所ごとに理由 doc が実在するかは未確認（下の未検証 2）。

## 未検証

1. ⚠️ SPEC.md:627 の参照（bullet リード形）を G-heading-refs が拾うか。`npm run governance:check` を実際に回していない。
2. ⚠️ rules の `PlatformBridge` 行の「各直呼び箇所の doc」に理由が書かれているか。`PlatformBridge` を避けた直呼び箇所を列挙して確かめていない。
3. ⚠️ M3 の移設（`build_index_from_material` doc）、V1〜V4 の `visual.rs` `//!`、F3 のテストメッセージ、M11 の `activation/tests.rs` `//!` が、research の言うとおり逐語で持つか。research の「確認済み」を信用し、再確認していない。
4. ⚠️ `snotra-egui-runtime/CLAUDE.md:34` に W2（`Context` の clone を managed state に置かない）があることは grep で確認した。ただし本文全体が W2 の命題（Arc が `Destroyed` を越える機序）を持つかは未読。
5. ⚠️ 書き直し案の見出しの字面は、現行と照合して逐語に残る（`window_coordinator.rs:848` の「イベント駆動 wake の不変条件（#532 SU5）」、`lifecycle.rs:35`・`search_state.rs:129` の「処置を返す純粋核の強制」、「ウィンドウ生成の制約」、「trace の presence 検査は状態の検査ではない」）。ただし G-heading-refs の前方一致規則（長い見出しとの照合）の細部は未読。現状も長い見出しで通っているので、下げ幅は小さいと見る。

## 確認済み（問題なし）

- 正準形の参照は計画の表で網羅されていた（上の要対処 1〜4 を除く）: `activation.rs:348`・`search_flow.rs:44`・`layout.rs:394,453`・`egui_shell/mod.rs:21`・`results_window.rs:28,122,151,337`・`view.rs:972,1205,1239`・`window_coordinator.rs:26,192,412,476,530,545,557,848`・`state.rs:37,101`・`working_set.rs:5`・`config_watcher.rs:8,23`・`commands/system.rs:19`・`main.rs:302,342,413,616`・`code-reviewer.md:68,89,124`・`rules/src-tauri.md`・`SnotraTraceInvariants.psm1:8`・`SPEC.md:767`・`PERFORMANCE.md:87`。
- `race-check` SKILL・`snotra-egui-runtime/CLAUDE.md:24`・`build-commands.md:92`・`smoke-egui.ps1:701`・`visual-check-colors.ps1:32`・`notify.rs:84`・`heap_trace.rs:23` は、残る見出しを指す。
- ほかの md・`.claude/`・`scripts/` に、`src-tauri/CLAUDE.md` を指す生きた参照の取りこぼしは無い（`git grep` 全件を突き合わせた。ADR・`docs/superpowers/`・`docs/design/` は除外）。
- パス無しの見出し参照は `main.rs:302`・`hide_request.rs:6`・`rules/src-tauri.md:12-15` を拾い、`hide_request.rs:6` 以外は計画が扱っていた。`notify.rs:185` の「処置を返す純粋核」は一般名で参照ではない。
- 命題の落ちの調査: 「モジュール構成」の各 bullet、X4〜X12、「config の読み」、I7（`window_coordinator.rs:293-300,359-363` に順序・バー高/実高・`SWP_NOMOVE`・2 回撃っていた経緯がある）は、`config_watcher.rs:150-162` の M5/M6 本体コメントを含め、乗っていた。
