# 逆向きの監査 — #745 計画（案 C: 1 フィールドの状態機械）

**枠組み**: 前向きの適合検査はしない。**この計画が削除する行・メソッド・doc が何を保証していたか**を名指し、**計画後にどこで再確立されるか**を `file:シンボル` で示し、**再確立されないもの**を列挙する。

対象コミット: `main` = `9ebf3db`（作業ブランチ `fix/blur-grace-reset-on-show` に差分なし）。

---

## 0. 結論（先出し）

- **ブロッキング 2 件**
  - **B1**: `mod.rs:62` の doc（`blur_should_hide` を出さない理由＝「消費点は `blur_grace_action` に一本化」）が偽になるが、計画の「偽になる散文（全件を上表に反映済み）」表に**無い**。さらに `mod.rs:65` の `blur_grace_action` / `BLUR_GRACE` 再エクスポートが**消費者ゼロ**になる（計画は「据え置き」と書いている）。#711 が型で塞いだ「判定と再要求の 2 経路」が、旧チョークポイントを公開したまま新チョークポイント（`observe`）を足す形になり**弱まる**。
  - **B2**: `view.rs:636` の「現況へ書き換え」が過小指定。書き換えが落としてはならないのは「**入力欄の focus 復帰は blur 状態を読まない**」という恒久制約であり、その理由（stale）が消えても制約は残る。
- **再確立地点が無い不変条件 3 件**: 「唯一の書き手」の単一書き手性（1 → 2 へ後退）／「束ねてよい」判断の根拠（`workspace/plan.md` にしか無く、workspace ライフサイクルで消える）／`new()` の初期 variant（計画が名指していない）。
- **最重要の経緯的所見**: **#745 は 2 変数分解の帰結ではなく、2026-07-22 の SU2 設計書が明記した決定の実装漏れである**（後述 §3.1）。計画 `:40` の因果説明は一次資料と食い違う。ブロッキングではないが、計画の構造的論拠を弱める。

---

## 1. 消える行・doc が保証していたもの（逐条）

### I1. `launcher_controller.rs:1035-1041` `clear_blur_grace_if_focused` 本体 `if focused { self.unfocus_at = None; }`

- **保証**: focus が戻ったフレームで armed 猶予を破棄する（＝以後 `blur_grace_action` が `Rearm(残余)` を返して無駄な `request_repaint_after` を撃ち続けることも、focus 復帰後に猶予明けで hide することも起きない）。
- **再確立**: `lifecycle.rs:BlurGrace::observe`（`_ → Focused` アーム、`Idle` を返す）。計画の遷移表 `:64` と一致。**保証は保たれる。**
- 注: 現行でも `blur_should_hide` の `!focused` 連言があるため hide は二重に塞がれている（`lifecycle.rs:85-87`）。段 14 が実際に消しているのは**無駄な Rearm 予約**である。`observe` の `Idle` がこれも消す。

### I2. `launcher_controller.rs:1036-1037` 本体コメント「emit dedup（`hide_pending`）は `show_egui_main` がクリアするので view では触らない（codex #8）」

- **保証**: view / controller 側に emit dedup フラグを持たない、`hide_pending` に触らない。
- **再確立**: **有り**。`launcher_controller.rs:196-198`（`emit_hide` の doc「多重防止は共有 `EguiShellState.hide_pending`（`show_egui_main` がクリア・codex #8）。view-local フラグだと hide 後 `Focused(true)` 非着信で永久 true 化し、以後の hide を抑止してしまう」）と `launcher_controller.rs:110`（フィールド群のコメント）。**損失なし。**

### I3. `launcher_controller.rs:1033-1034` doc「段 16–17 とは間に Escape ラダー（段 15）を挟んで**別の塊**であり、**束ねると順序が動く**」

- **保証**: 「この 2 つを束ねる編集者は、順序が動くことを再導出せよ」という**手続き上の警告**。ライブな読み書き依存の記録ではない（§3.2 の git 経緯を参照）。
- **再確立**: **無し**。計画は再導出を行い（`plan.md:72-74`）人間の裁定も得ている（`plan.md:188`）が、その導出結果は **`workspace/plan.md` にしか無い**。`workspace/` はサイクルで破棄される領域であり、マージ後にコードから辿れる記録が残らない。
- **推奨**: `observe` の doc か段 16–17 の呼び出し点に 1 行——「段 14（focus 復帰時の破棄）を畳んだ。Escape ラダーとの間に読み書き依存が無いことが根拠（`on_escape_pressed` は両状態を読まない）」。

### I4. `launcher_controller.rs:1302-1303` doc「段 34: 今フレームの focus を次フレームの `was_focused` として畳む（`on_focus_changed` の**唯一の書き手**）」

- **保証**: 「前フレームの focus」の書き手が**フレーム内に 1 つしかない**こと。読み手（段 16–17）から見て値の由来が一意である。
- **再確立**: **無し（後退する）**。計画後、`blur_grace` の書き手は **2 つ**になる——`consume_reset_pending`（段 3・`reset()`）と `on_focus_changed`（段 16–17・`observe()`）。計画の表 `:45` も「フレーム内の入口 2」と認めており、後退は**意図的**である（reset こそが本 issue の修正）。問題は**どこにも書かれないこと**。
- **推奨**: `blur_grace` フィールドに doc を付け、書き手 2 つを段番号つきで名指す（「書くのは段 3 の `reset()` と段 16–17 の `observe()` だけである」）。現行の 2 フィールドは doc を持たないため、これは新規の負債ではなく**移行の機会**である。

### I5. `launcher_controller.rs:1072-1074` doc「前フレームの focus は `was_focused` が持ち、更新は段 34 が行う——**この 2 段の間に書き手は無い**」

- **保証**: 段 16–17 が読む値は、段 34 が前フレーム末に書いたものそのままである（フレーム内の途中書き換えが無い）。
- **再確立**: 計画は「書き換え」とするのみで、置換後の文言を指定していない。**畳んだ後はこの命題自体が消える**（前フレームの値を保持する期間が無くなる）ため、正しくは「消える」であり損失ではない。
- **確認済み（一次証拠）**: `was_focused` に触るのは `:139`（init）/ `:1076`（read）/ `:1078`（read）/ `:1305`（write）の 4 か所のみ。`unfocus_at` は `:140` / `:1039` / `:1079` / `:1085` / `:1092`。**計画の主張「3 メソッドと初期化だけ」は grep で裏付けられる**（下記 §4 の実測）。

### I6. `launcher_controller.rs:1079-1080` 武装アームの `Instant::now()` と `ctx.request_repaint_after(BLUR_GRACE)`

- **保証**: 武装したフレームで猶予境界に着弾する wake を必ず 1 本予約する。
- **再確立**: `observe` の武装アームが `blur_grace_action(ZERO, ..) == Rearm(BLUR_GRACE)` を返し、呼び出し側（`on_focus_changed`）の `BlurAction::Rearm` 分岐が 1 回撃つ。**保証は保たれ、予約回数が 2 → 1 に減る**。
- **独自検算**: 現行は (a) `request_repaint_after(100ms)` を撃ち、(b) 直後の `if let Some(at)` で `at.elapsed() ≈ ε` から `Rearm(100ms - ε)` を撃つ。repaint worker は**最も早い deadline を採る**（`src-tauri/CLAUDE.md`「期限を待つ状態（armed）」）ため実効期限は `t_arm + ε + (100 - ε) = t_arm + 100`。計画は `t_now + 100`、`t_now = t_arm`。**同値**。計画の「差は ε の消失だけ」（`:187`）は正しい。

### I7. `launcher_controller.rs:1092` `Hide` アームの `self.unfocus_at = None`

- **保証**: hide を emit したフレームで武装を解く（次フレーム以降に毎フレーム hide を撃たない）。
- **再確立**: `observe` の `Blurred(t) + !focused → Hide` で `→ NeverFocused`（計画 `:67`）＋テスト 7（`blur_grace_hide_returns_to_never_focused`）。**保証は保たれる。**

### I8. `lifecycle.rs:57-59` doc「`Idle` を返したフレームで `unfocus_at` がクリアされないこと…は別の未解決事項であり #745 が追う」

- **保証（前半）**: **`Idle` は武装を解かない**。これは #745 とは独立に必要な挙動である——`auto_hide` off の間に blur した窓は `Blurred(t)` のまま留まり、`config-applied` wake で `auto_hide` が有効化されれば猶予明け判定に入る。武装を解いてしまうと、この経路が黙って死ぬ。
- **再確立**: 計画の遷移表 `:68`「`Blurred(t)` + `!focused` → `Rearm` / `Idle`: 据え置き」。**保証は保たれる。ただし `observe` の実装が `Idle` で `*self` を変えないことをテストで固定していない**——計画のテスト 1〜7 に `auto_hide=false` を通す行が 1 本も無い（⚠️ §5-W3）。

### I9. `view.rs:421` / `view.rs:997` の呼び出し行

- 行そのものにコメントは無い（実測）。位置の意味は §2 で扱う。

---

## 2. 段 14 が Escape ラダーの前・段 34 がフレーム末尾にあることの、データ依存以外の理由

### 2.1 段 14 が段 15 より前

**一次資料に非データ依存の理由は見つからない。**

- `68b5f41`（SU2・初出）では 4 断片が**ひと続きのブロック**だった: `let was_focused = self.was_focused;` → `if focused { unfocus_at = None }` → `if escape { emit_hide() }` → 武装 → 猶予判定 → 末尾 `self.was_focused = focused;`（`git show 68b5f41:src-tauri/src/egui_shell/view.rs` の 125–168 行）。**現在の順序はこのブロック内の並びをそのまま引き継いだものである。**
- 段 14 / 段 34 がメソッドに分かれ、doc「束ねると順序が動く」「唯一の書き手」が付いたのは **`f3c53f1`（#785・`view.rs` の機械的分割）の 1 コミットのみ**（`git log -L 1033,1041:…launcher_controller.rs` → `f3c53f1` 単独、`git log -S '束ねると順序が動く'` → `f3c53f1` 単独）。**分割時に書かれた注意書きであって、ライブな制約の記録ではない。**
- 傍証: `docs/superpowers/specs/2026-07-27-666-launcher-controller-main-view-design.md:96` が「段 14〜20 は `focused` / `Key::Escape` / `ArrowRight` / `ArrowLeft` と時刻・設定しか読まない（**実測**）」と記録している。#785 の設計時点で「段 14 が他と読み書きを共有しない」ことは測られていた。
- 現行コードでの独立確認: `on_escape_pressed`（`:1047-1069`）は `state.on_escape()` と `folder_cache` / `folder_error` / `instant_rows_query` / `emit_hide` にしか触れず、`unfocus_at` / `was_focused` を**読み書きしない**。**計画の主張は正しい。**

**ただし 1 つだけ順序が残る**: 段 15 の `emit_hide()` と段 16–17 の `emit_hide()` が同一フレームで並ぶとき、先着は Escape 側である。`emit_hide` は `hide_pending.swap(true)` で後着を落とすため（`:199-208`）**観測可能な差は無い**——計画 `:73` の主張どおり。畳んだ後も `observe` は段 15 の後に留まるので、この順序自体は変わらない。

### 2.2 段 34 がフレーム末尾

**非データ依存の理由は見つからない。位置は不活性である。** 根拠は 2 つ:

1. **`set_focused(pre.focused)` は段 13 のスナップショットを渡す**（`view.rs:997` が `pre.focused` を渡し、`pre` は `:419` の `read_pre_widget_input` が生成。`focused` の実体は `ctx.input(|i| i.focused)`・`view.rs:206`）。**フレーム末で focus を読み直していない**——ゆえに「フレーム後半で起きたこと（起動・窓駆動）を畳み込む」という意味は最初から無い。
2. `:997` は `update()` の**最後の文**（`:998` が閉じ括弧）であり、その後に読み手が居ない。

補足（全称否定の扱い）: `update()` 本体に `return` は 0 件（`grep -n 'return' src-tauri/src/egui_shell/view.rs` が無出力）だが、**不在の観測 1 つでは確定させない**方針に従い、上の (1)(2) の構造的事実を主根拠とする。なお段 3〜段 34 の span が段 3〜段 17 へ縮むのは**安全側の変化**である（将来 early return が挟まれたときに畳み損ねる区間が短くなる）。

---

## 3. git 経緯 — 当時の書き手の理由で、計画が考慮していないもの

### 3.1 【最重要】reset-on-show は初日の設計決定であり、実装漏れである

`docs/superpowers/specs/2026-07-22-su2-window-shell-design.md:83`（原文）:

> **stale 猶予の防止（codex #8）**: `focused` のとき `unfocus_at=None`。**加えて show のたびに view 側の `was_focused`/`unfocus_at` をリセット**（再表示直後に前回の stale な猶予で即 hide しない）。focus 復帰と多重 focus-loss を**状態として**扱う。

そして `docs/superpowers/plans/2026-07-22-su2-window-shell.md:681`:

> - **spec §blur（100ms・ゲート・サイドカーガード・**stale リセット**・policy を view）** → Task 5（+ codex #8）。✓

**しかし `68b5f41` が入れたのは前半（`if focused { unfocus_at = None }`）だけで、「show のたびにリセット」は入っていない**（`git show 68b5f41:…/view.rs` の 120–170 行に show フックは無く、`show_egui_main` が触るのは `hide_pending` のみ・SU2 プラン `:384`）。**✓ が付いた spec 項目の半分が落ちた。**

**帰結 1（因果の訂正）**: 計画 `:40` は「**#745 が起きた原因はこの分解そのものである**」と断じるが、一次資料はそう言っていない。**原因は「書かれた決定の実装漏れ＋その追跡の断絶」である。** 2 変数分解はシナリオを A / B の 2 つに見せた（症状の形）が、原因ではない。

**帰結 2（情状）**: 落ちたことは説明可能である——`reset_pending` 機構は SU2 には存在せず、SU3 M1 で入った（`docs/superpowers/plans/2026-07-22-su3-m1-core-search.md:1059`）。SU2 時点で掛ける show フックが無かった。そして SU3 M1 でフックが生まれたとき、blur の 2 フィールドはリセット対象の列挙に追加されなかった。

**帰結 3（計画の構造的論拠が弱まる）**: 「1 フィールドにすれば『両方消したか』という問い自体が消える」（`:40`）は真だが、**残る問いは「`reset()` は show のクリア一覧に入っているか」であり、これは初回に失敗したまさにその問いである**。計画自身が段 3 と段 16–17 の順序を「テスト不能・受容残余」に計上している（`:149`）。→ **推奨**: `BlurGrace` の型 doc に、リセット地点（`launcher_controller::consume_reset_pending`）と**この由来**（SU2 設計 `:83` が最初から要求していたこと）を書く。次に「hide を跨ぐ状態」を足す人がこの前例に当たる。

**帰結 4（計画に有利な材料）**: `:83` 末尾の「focus 復帰と多重 focus-loss を**状態として**扱う」は、案 C の enum がまさに設計者の当初表現に戻ることを意味する。計画はこの系譜を主張してよい。

### 3.2 `view.rs:636` のコメントは SU2 原典の生き残りである

`// was_focused に依存しないので、hide→reshow で was_focused が stale でも確実に戻る。` は `68b5f41` の 163 行目からそのまま生きている。**当時の書き手は「`was_focused` が hide を跨いで stale になりうる」ことを知っていた**（§3.1 のリセットが未実装だったため）。→ B2（§5-C2）。

---

## 4. 計画が「検証済み」と主張する 3 点の独立確認

| 計画の主張 | 判定 | 一次証拠 |
|---|---|---|
| 触るのは controller の 3 メソッドと初期化だけ・段 14〜34 の間に外部の読み手なし | **真** | `grep -rn 'was_focused\|unfocus_at' --include=*.rs .` → `launcher_controller.rs:{104,105,139,140,1039,1073,1076,1078,1079,1085,1092,1302,1305}` と `view.rs:636`（コメント）のみ。`.rs` の他ファイルは 0 件 |
| Escape ラダーは両フィールドを読まない | **真** | `launcher_controller.rs:1047-1069` の全文（`state.on_escape()` / `folder_*` / `instant_rows_query` / `emit_hide` のみ） |
| 同一フレームの二重 hide は `hide_pending` が潰す | **真** | `launcher_controller.rs:199-208`（`swap(true, SeqCst)` → `already` なら早期 return） |

追加で確認した計画外の事実:

- `pre.focused` は段 13 で 1 回だけ読まれ、段 14 / 段 16–17 / `request_focus` / 段 34 の**全員が同じ値**を見る（`view.rs:206, 419, 421, 427, 638, 997`）。**畳んでも入力値は変わらない。**
- `BLUR_GRACE` の crate 内消費点は `launcher_controller.rs:1080` の **1 か所だけ**、`blur_grace_action` は `:1086` の **1 か所だけ**（`grep -rn` 実測・`lifecycle.rs` 内を除く）。→ §5-C1。

---

## 5. 所見一覧（⚠️ は確信度つき）

### C1 【ブロッキング・確信度 高】`mod.rs:62` の doc が偽になる／`:65` の再エクスポートが消費者ゼロになる

`mod.rs:62`（原文）: `// blur_should_hide は re-export しない——消費点は blur_grace_action に一本化され、` / `:65`: `pub(crate) use lifecycle::{BLUR_GRACE, BlurAction, HotkeyPlan, blur_grace_action, plan_hotkey};`

- **偽になる散文（計画の表に無い 5 か所目）**: 計画後の消費チョークポイントは `BlurGrace::observe` であり、`blur_grace_action` ではない。計画の「**この変更で偽になる散文（…全件を上表に反映済み）**」は**全称表現として破れる**。
- **不変条件の後退**: `:62` が語る「判定と再要求を別々に呼ぶ 2 経路が生まれるのを**型で塞ぐ**」は、旧チョークポイント `blur_grace_action` を `crate::egui_shell::` から見えたままにすると**塞がらない**。`launcher_controller` から `observe` を迂回して直接呼ぶ書き方がコンパイルを通り続ける。**#711 が型で得たものを、計画は散文に戻す。**
- **さらにビルドの問題（予測・要実測）**: `BLUR_GRACE` と `blur_grace_action` の crate 内消費点は削除対象の `:1080` / `:1086` のみ。削除後は再エクスポート 2 名が未使用になる。計画の「`blur_grace_action` / `blur_should_hide` / `BlurAction` / `BLUR_GRACE` は**据え置き**」は少なくとも文言として誤り。`-D warnings` 下で `unused_imports` が `pub(crate) use` に発火するかは**断定しない**——`launcher_controller.rs` の編集直後に `cargo build -p snotra` で測ること。発火するなら `:65` から 2 名を落とす修正が**同じコミットに要る**（`BlurAction` は controller が match し続けるので残す）。
- **推奨**: `blur_grace_action` を `lifecycle.rs` の private へ落とし、`:65` から `blur_grace_action` / `BLUR_GRACE` を外し、`:62` の doc を「消費点は `BlurGrace::observe` に一本化」へ書き換える。`:62` が意図的な除外を明記していること自体が、`:65` が**吟味されたリスト**である証拠である。

### C2 【ブロッキング・確信度 高】`view.rs:636` の「現況へ書き換え」が過小指定

- **落としてはならない制約**: 「**入力欄の focus 復帰（`response.request_focus()`）は blur 状態を読まない**」。
- 理由が変わるだけで制約は残る: 計画後、`reset()` 直後の状態は `NeverFocused` である一方、**窓は実際に focus を持ちうる**。`request_focus` を `blur_grace == Focused` で門にすると、**show 後の最初のフレームで打鍵できない**（SU2 が `Alt+Q 表示直後に打てる` として入れた挙動・`view.rs:635`）。
- **計画の指示**（`:95`「`:636` のコメント（`was_focused` が stale でも、の前提）を現況へ」）は、stale が消えたことだけを書き換えの目標にしており、**制約自体が消えるリスクを言語化していない**。書き換え後の文が何を禁じ続けるかを計画に明記すること。

### C3 【非ブロッキング・確信度 高】因果の説明を訂正すべき

§3.1。計画 `:40` の「#745 が起きた原因はこの分解そのものである」は一次資料と食い違う。実装は残してよいが、PR 本文・commit message・`RETROSPECTIVE` へ運ぶ教訓は「**書かれた決定が、機構の到着待ちのあいだに追跡から落ちた**」である。

### C4 【非ブロッキング・確信度 中】`specs/2026-07-26-frame-scheduling-contract-design.md:100` が計画の更新対象に無い

- 計画は同ファイルの `:7` だけを挙げる。`:100` は「**backstop の既知の穴**: `unfocus_at` / `was_focused`（blur 猶予）は reset-on-show でクリアされておらず、この条項の対象でありながら backstop の外にいる（#745）」——`:7` と**同じ種類の文**（未解決 issue を名指す追跡）である。
- ⚠️ **留保**: 同ファイルの `:55` / `:148` / `:165` / `:176` は `0d6e564` で「歴史記録」へ転じた文書内のコード草案であり、更新不要と読める。`:100` がその線のどちら側かは**計画の持ち主が裁定すべき**で、ここでは断定しない。

### C5 【非ブロッキング・確信度 高】「唯一の書き手」の再確立地点が無い（I4）

書き手は 1 → 2（段 3 `reset()` / 段 16–17 `observe()`）。**推奨**: `blur_grace` フィールドに doc を新設し 2 つを段番号で名指す。

### C6 【非ブロッキング・確信度 高】「束ねてよい」判断の記録が `workspace/plan.md` にしかない（I3）

`workspace/` はサイクルで破棄される。**推奨**: `observe` の doc か段 16–17 の呼び出し点に 1 行残す。

### C7 ⚠️ 【確信度 中】`new()` の初期 variant を計画が名指していない

計画 `:94` は「`was_focused` / `unfocus_at`（`:104-105` / `:139-140`）を `blur_grace: BlurGrace` へ置換」とだけ書く。**`:139-140` の現行初期値 `(false, None)` に対応するのは `NeverFocused`** であり（計画自身の対応表 `:55`）、`Focused` で初期化すると起動直後の 1 回目の blur で武装しうる。実装時に取り違える確率は低いが、計画のチェックリストに文言が無い。

### C8 ⚠️ 【確信度 中】`Idle` が武装を解かないことを固定するテストが無い（I8）

計画のテスト 1〜7 に `auto_hide == false` を渡す行が 1 本も無い。`Blurred(t)` + `!focused` + `auto_hide=false` → `Idle` かつ**状態が `Blurred(t)` のまま**、を固定する 1 本を推奨する。これが破れると「auto_hide を後から有効化しても hide されない」が静かに入る（`config-applied` wake 経路・`lifecycle.rs:49-55`）。

### C9 ⚠️ 【確信度 低〜中】再 show（既に可視の窓）での `reset()` が live な武装を落とす

`show_egui_main` は無条件に `reset_pending` を立てる（`window_coordinator.rs:255`）。`plan_hotkey` の `visible && !hotkey_toggle → ShowNow`（`lifecycle.rs:13-21`）やトレイ経由の show は**可視のまま**この経路を通りうる。そのとき現行は `unfocus_at` が生き残るが、計画後は `NeverFocused` へ落ちて武装が消える。
- 挙動としては**改善側**（明示的な show 要求の直後に自動 hide しない）と読めるが、**計画はこのケースを列挙していない**。実機確認の項目 1〜4 にも「可視のまま再 show」が無い。
- ⚠️ 確信度が低いのは、その経路で実際に武装が生きているフレーム窓（blur 中に hotkey_toggle=false で Alt+Q）が狭く、実観測していないため。実機確認に 1 項目足すのが安価。

### C10 【損失なしとして記録】codex #8（view は `hide_pending` を触らない）

I2 のとおり `launcher_controller.rs:196-198` と `:110` で再確立済み。**この項は「消えるが失われない」例であり、上の損失一覧が網羅的に選別されたことの対照である。**

---

## 6. 不変条件 → 再確立地点 対応表

| # | 消える行/doc | 保証していたもの | 再確立地点 |
|---|---|---|---|
| I1 | `launcher_controller.rs:1038-1040` | focus 復帰で武装破棄（無駄な Rearm を止める） | `lifecycle.rs:BlurGrace::observe`（`_ → Focused` / `Idle`） |
| I2 | `launcher_controller.rs:1036-1037` | view は `hide_pending` を触らない（codex #8） | `launcher_controller.rs:196-198` + `:110` |
| I3 | `launcher_controller.rs:1033-1034` | 「束ねる者は順序を再導出せよ」 | **無し**（`workspace/plan.md` のみ・破棄される） |
| I4 | `launcher_controller.rs:1302-1303` | 前フレーム focus の**単一書き手** | **無し**（1 → 2 書き手へ意図的に後退・doc 未設） |
| I5 | `launcher_controller.rs:1072-1074` | 段 16–17 と段 34 の間に書き手なし | 命題ごと消える（損失ではない） |
| I6 | `launcher_controller.rs:1079-1080` | 武装フレームで猶予境界の wake を予約 | `observe` 武装アーム → `BlurAction::Rearm` 分岐（実効期限は同値・§I6 検算） |
| I7 | `launcher_controller.rs:1092` | hide 後に武装を解く | `observe`（`Hide → NeverFocused`）＋ 計画テスト 7 |
| I8 | `lifecycle.rs:57-59` 後半 | `Idle` は武装を解かない | 計画 `:68`「据え置き」。**テストで固定されていない**（C8） |
| — | `mod.rs:62`（計画外） | 「消費点は 1 つ」を**型で**塞ぐ（#711） | **無し**（C1・旧チョークポイントが公開のまま残る） |
| — | `view.rs:636`（書き換え） | 入力欄 focus 復帰は blur 状態を読まない | **書き換え文言が未指定**（C2） |

---

## 7. 実装時に測ること（この監査が根拠を持てなかった点）

1. `launcher_controller.rs` の 2 メソッド削除**直後**に `cargo build -p snotra` を撃ち、`mod.rs:65` の未使用再エクスポートが `-D warnings` で落ちるかを実測する（C1）。
2. C9 の「可視のまま再 show」を実機確認の 5 項目目として追加するか、明示的に受容と記録する。
