# 段 3: `LauncherController` / `MainView` — `view.rs` の責務分割（#666）

`egui_shell` 責務分離の 3 段の最終段。段 1（#749・`window_coordinator.rs`）と段 2（#752・`layout::present_results`）が完了した上で、残る `view.rs`（1869 行）を分ける。

**挙動は変えない。** 動かすのは所有であって、`update()` 内の文の実行順序ではない（issue 確定事実 1・3・4 がその理由を 3 通り示している）。

## 0. 前提: 分類の母集団

`view.rs` の全項目を数える（実測・`research.md` に内訳）。

| 種別 | 件数 |
|---|---|
| `SearchWindowView` のフィールド | 19 |
| inherent メソッド | 25 |
| 自由関数 | 6（フォント 5 + `draw_toast_button`） |
| 型・静的・定数 | 9 |
| `EguiView` impl | 2（`setup` / `update`） |
| `#[test]` | 7（**全件がフォント群のテスト**） |

## 1. 決定: 規則 R(段 3) を 1 本置く（例外ゼロ）

ADR-0008 は段 1 で「線が 5 つの異なる原理で引かれており、衝突時にどちらが勝つか書かれていない」と落ちた。段 3 は母集団が桁違いに大きいため、規則を先に固定する。

> **規則 R(段 3)**
>
> 1. 項目は、**それが守る不変条件が属する層**へ置く。層は 2 つ——
>    - **検索セッション層**（show を跨ぐ状態・結果・選択・起動・履歴・期限） → `launcher_controller.rs`
>    - **描画面層**（1 フレームの egui pass への入出力・OS 窓への適用と、その適用の冗長回避 memo） → `view.rs`
> 2. **両層に消費者を持つもの**は、依存の向き（`view` → `launcher_controller` の**一方向**）が許す側、すなわち `launcher_controller.rs` へ置く
> 3. **どちらの向きでも到達できない消費者**（`egui_shell` の別 view）を持つものは、独立モジュールへ出す
> 4. **ADR-0008 の規則 R をそのまま継承する**——移設する項目がその中でしか使わないヘルパーは一緒に運ぶ。複数のモジュールから消費されるものは残す（条項 3 はこの後段の言い換えである）

条項 4 を明記しなければ、`font_covers_cjk` / `font_definitions` / `resolve_font_family` / `jp_font_bytes` と静的 3 件（**唯一の消費者が `configure_japanese_font` である 7 項目**）の行き先が条項 1〜3 では決まらない——条項 1 だけを当てると「描画面層 → `view.rs`」になり、`configure_japanese_font` だけが `font_stack.rs` へ行く不合理が生じる。段 1 の先例を引くのは、**同じ規則で 2 段が説明できることを保つため**である。

規則 3 が発火するのは 1 件だけである——`configure_japanese_font` を `results_view.rs` が 2 箇所で呼ぶ（実測）。`view.rs` と `results_view.rs` は互いに依存しないため、フォント群はどちらにも置けない。よって `font_stack.rs` を新設する。**これは規則の帰結であって例外ではない**（ADR-0008 規則 R の「複数のモジュールから消費されるものは残す」と同じ論法）。

### 1.1 分類結果（全 68 項目・例外ゼロ）

**`new` は分類ではなく分割である。** inherent メソッド 25 のうち、規則 R が行き先を決めるのは 24（controller 23 + view 1 = `window_width`）。残る `new` は**両型がそれぞれ持つ**——`SearchWindowView::new(app_handle)` が `LauncherController::new(app_handle)` を包む形になり、「どちらへ置くか」を問う項目ではない。以下の一覧はこの 24 を挙げる。

**`launcher_controller.rs` — `LauncherController`**

- フィールド 15: `app_handle` / `was_focused` / `unfocus_at` / `state` / `search_debounce` / `last_input_at` / `folder_tx` / `folder_rx` / `folder_cache` / `folder_error` / `instant_rows_query` / `launching` / `last_seen_index_generation` / `notice` / `notice_base`
- メソッド 23: `emit_hide` `activate` `start_launch` `finish_launch` `drain_launch` `clear_search` `execute_slash` `execute_instant_selected` `activate_or_execute` `shift_activate` `execute_tool_selected` `record_folder_expansion` `resolve_tools` `auto_hide_enabled` `settings_running` `instant_prefix` `indexing` `lang` `spawn_folder_load` `run_search` `run_search_with` `handle_toast_action` `spawn_install`
- 型 5: `FolderMsg` `LaunchWork` `LaunchTag` `LaunchInFlight` `ToastAction`

`app_handle` は規則 2 で決まる（両層が要る）。`view` は `self.controller.app()` で借り、**clone を 2 本持たない**。

**`view.rs` — `SearchWindowView`（名前は現行のまま・§3.1）**

- フィールド 4: `applied_font_family` / `applied_background_hex` / `last_set_width` / `last_set_height`（いずれも「前回どう適用したか」の memo。守る不変条件は egui pass / OS 窓への冗長適用の回避であって、検索の正しさではない）
- フィールド 1: `controller: LauncherController`
- メソッド 1: `window_width`（消費者は `set_size` と `drive_results_window` の 2 か所で、**どちらも描画面層**。ADR-0008 が段 1 で view に残した判断と整合する）
- 自由関数 1: `draw_toast_button`
- `EguiView` impl 2: `setup` / `update`

**`font_stack.rs`（新設）**

- 関数 5・静的/型/定数 4・テスト 7（フォント群の全体）

### 1.2 規則 R が説明する境界事例

| 項目 | 一見の行き先 | 規則の帰結 | 根拠 |
|---|---|---|---|
| `last_seen_index_generation` | 「前回見た」= memo → view | **controller** | 守るのは再検索の正しさ（検索セッション層） |
| `applied_font_family` | 「config 追従」= セッション → controller | **view** | 守るのは `set_fonts` の冗長適用回避（描画面層） |
| `notice` / `notice_base` | 描画されるので view | **controller** | 期限（deadline）を張る主体であり、書き手は起動結果（確定事実 4a） |
| `handle_toast_action` / `spawn_install` | toast の描画に付随 → view | **controller** | `UpdaterUiState` の状態遷移と install spawn。描画は `draw_toast_button` が担う |
| `window_width` | config 読み → controller | **view** | 消費者 2 件がどちらも描画面層（規則 1 で決まり、規則 2 に落ちない） |
| `lang` / `indexing` / `instant_prefix` | 同上 | **controller** | 消費者が両層に散る → 規則 2 |

## 2. 決定: `update()` の**副作用**の実行順序を 1 つも動かさない

確定事実 1・3・4 はいずれも「順序が load-bearing である」ことの別々の証拠である。抽出は**連続する文の塊に名前を付ける操作**に限り、塊の並べ替え・分割位置の変更を行わない。

**「1 行も動かさない」とは書けない**——§2.2 が入力の**読み** 4 件を前へ寄せることを認めており、全称表現が自らの後段で破れる。動かないのは**副作用**（状態変更・OS 窓操作・spawn・emit・repaint 要求）であって、行ではない。前提条件を付けられない全称表現は書かない（`AGENTS.md`「検証の作法」）。

### 2.1 検証方法（自動検出器が無いことへの手当て）

`cargo test` にはこのクラスの回帰を捕らえる検査が無い（`view.rs` のテスト 7 件は全てフォント）。trace の presence を見るスモークも緑のまま通した実例がある（#671 PR A′）。ゆえに次の 2 つを検知手段とする:

1. **`git diff` 上で `update()` の副作用文の列が並べ替わっていないこと**を目視照合する（本設計 §5 の 34 段一覧が照合表）
2. **`docs/build-commands.md` カテゴリ D の実機目視**（`cargo run -p snotra`）。フォント群を動かすため `src-tauri/CLAUDE.md`「フォント登録」節が別途これを必須にしている

### 2.2 入力の読みを 2 段へまとめることの安全性

確定事実 1 のとおり入力変換は pre-widget / post-widget の 2 段に割れる。実装ではこの 2 段を `view.rs` の私的関数 2 本（+ 小さな struct 2 つ）にする。

このとき **Escape / ↑↓ / →← の「読み」だけが現行の位置（34 段の 13・18・19・20）より前へ寄る**。

**「egui の入力はフレーム内で不変だから読む順序は関係ない」という一般命題を根拠にしてはならない——偽である。** `InputState::key_pressed()` は `num_presses()` を経て **`self.events` を走査する**（`egui-0.35.0/src/input_state/mod.rs:743,750-760` を一次資料で確認）。`ctx.input_mut` の `events.retain` はその `events` を書き換えるため、**除いたキーに対する以後の `key_pressed()` は `false` を返す**。`view.rs` の Enter 後置ブロックのコメントが同趣旨の一般化を書いているが、それが成り立つのは **Enter イベントを retain する箇所が無い**からであって、順序が無関係だからではない。

安全である根拠は、一般命題ではなく次の 2 つの**局所的な事実**である:

- **`retain` が除くのは `Key::ArrowUp` / `ArrowDown` の `Event::Key` だけである**（`view.rs` の当該クロージャ・実測）。→← / Escape / Enter のイベントは 1 つも除かれない
- **寄せた先（段 13）から TextEdit 構築（段 21）までの間に、ArrowUp/Down を `events` から読む箇所が 1 つも無い**（段 14〜20 は `focused` / `Key::Escape` / `ArrowRight` / `ArrowLeft` と時刻・設定しか読まない・実測）

**この 2 つ目は「今そうである」という事実であって、構造的な保証ではない。** ゆえに `read_pre_widget_input` の doc には「**この関数より後で `key_pressed(ArrowUp/ArrowDown)` を読んでも常に `false` である**」と書く——将来 ↑↓ を読む文を段 14〜20 に足した編集者が、沈黙して `false` を受け取る罠に落ちないため。

**処置（`move_selection` / folder 展開 / blur 判定）は 1 つも動かさない。** 動くのは読みだけである。

## 3. 検討した代替案と却下理由

### 3.1 `view.rs` を `main_view.rs` へ改名し、型も `MainView` にする

issue のたたき台は `MainView` と書く。却下した。

`view.rs` はこのモジュールで既に「main 窓の view」を意味する（対の `results_view.rs` があるため曖昧さが無い）。改名すると `.rs` 内の `view.rs` 参照 **47 件**（`results_view.rs` を指すものを除いて 37 件・実測）と `docs/architecture.md`（4 箇所）・`PERFORMANCE.md`（3 箇所）が**挙動と無関係に**動く。分割そのものが既にこの母集団の一部を動かすため、改名は**同じ母集団を二度動かす**ことになり、`snotra-egui-runtime/src/repaint.rs:301,307` の**テスト fixture リテラル `"view.rs"`** のような一括置換で壊れる例を踏む機会も倍になる。

得られるはずだった対称性（`ResultsView` / `MainView`）は、`SearchWindowView` が「検索窓の view」と読めるため実害を生んでいない。

**なお `.claude/skills/state-check/SKILL.md` の 1 行更新は分割の側で必要になり、本 PR に含めた**（`CLAUDE.md` 最重要ルール 2 に従って差分を提示し合意を得た・2026-07-27）。改名を却下する理由からこの項目は差し引いてあり、上の母集団の大きさだけで判断が立つ。

### 3.2 `view.rs` を残さず、`launcher_controller.rs` + `main_view.rs` の 2 ファイルへ全量を移す

却下した。`EguiView` を実装する型は `runtime.attach` へ move される 1 つでなければならず（`mod.rs:308`）、その型は結局どちらかのファイルに住む。ファイルを 2 つとも新設すると、`view.rs` の削除 + 新設 2 件で `git` の履歴追跡が切れ、`git log --follow` での経緯参照が効かなくなる。**残せるものは残す**。

### 3.3 フォント群を `view.rs` に残す（`font_stack.rs` を作らない）

却下した。規則 3 に反する——`results_view.rs` が `configure_japanese_font` を 2 箇所で呼んでおり、`view.rs` に残すと「main の view が results の view にフォント解決を供給する」依存が残る。現状それが許されているのは両者が同一ファイル群だからで、責務を分けると説明できなくなる。

副次的な利得: `src-tauri/CLAUDE.md`「フォント登録」節が「規則の正本はテストと `view.rs` の `//!`」と書いている指し先が、**フォントだけを担う `//!` とテスト 7 件**になる。

**名前は `font.rs` ではなく `font_stack.rs` にする。** `snotra-settings/src/font.rs` が既に存在し（`git ls-files` 実測）、`governance:check` の G1 は **basename 包含方式で wrong-directory 検出を意図的に放棄している**（`scripts/governance-check.mjs:77-78`）。同じ basename を 2 crate に置くと、**この変更に対する唯一の機構的ゲートが両 crate で盲になる**——索引から落ちても G1 が別 crate の同名ファイルで満たされてしまう。`font_stack.rs` は責務（user_font 先頭 + jp_font fallback という**スタックの組み立て**）をより正確に言い当ててもいる。

### 3.4 全域 `Effect` enum を導入し、controller が effect 列を返して view が dispatch する

確定事実 7 のとおり却下（issue で決着済み）。順序制約が `Vec<Effect>` の並びへ移るだけで、variant と dispatcher が肥大する。型を導入する基準は「呼び出し側が処理を忘れると不変条件が破れること」に置く。

**ただし本設計は `EscapeOutcome` 型の既存パターンを維持する**——これは基準を満たす成功例である。

### 3.5 `Option` 群を ADT（`enum ViewState`）へ置き換える

確定事実 6 のとおり却下（可読化のみを目的にするには費用対効果が低い、と Codex が評価済み）。`ViewKind` / `QueryIntent` の**導出**も現行のまま残す——明示 enum フィールドを重複させると config hot-reload された prefix と不整合になる。

### 3.6 7 箇所の外部イベント消費を 1 つの関数へ束ねる

却下した。確定事実 2 のとおり `take_clicked_for` は他 6 つと同カテゴリの破壊的 take でありながら **snapshot publish の後**という別フェーズに固定されている（#699）。「pending event は全部ここ」と読める包括名を付けると、この例外が不可視化される。

**冒頭の 6 つも 1 つに束ねない**——確定事実 3 のとおり `SearchState::reset()` の `rows_generation` bump が末尾のクリック照合と結ばれており、束ねた関数の doc がその制約を運べたとしても、名前は運べない。

### 3.7 `state: SearchState` を `pub(super)` にして view から直接読む

却下した。アクセサを 10 本以上書く手間を避けられるが、view が `&mut` を得られる形になり「遷移は controller だけが起こす」が規約に落ちる。

**代わりに `pub(super) fn state(&self) -> &SearchState` の 1 本にする**——`SearchState` の read メソッド（`view_kind` / `results` / `selected` / `query` / `folder_filter` / `tool_frame` / `rows_generation`）は全て `&self` で、mutator は `&mut self` を要る。共有参照 1 本で読みを全て通し、変更を型で不能にできる。

### 3.8 #751（`ctx.set_visuals` が現在の pass に届かない）をこの段で直す

却下した。確定事実 5 が機構を確定させているが、issue は #751 を別 issue として OPEN のまま置いている。**挙動を変えない移設**という本段の枠を外れる。

**ただし `view.rs` に残る誤ったコメント**（「`set_visuals` はウィジェット描画より前である必要がある」）は、移設に伴って書き直す位置に来る。**削除も修正もせず、確定事実 5 の内容（このバージョンでは成立していない・#751）へ差し替える**——移設で文言を運ぶとき、既知の誤りをそのまま運ぶのは避ける。

## 4. 新モジュールの `//!` が明記すべき「不在」

`window_coordinator.rs` の様式（ADR-0008 決定 3）に倣い、全称表現を前提条件なしで書かない。

**`launcher_controller.rs`**

- **フレームを所有しない**——`update()` の順序を決めるのは `view.rs` であり、ここにあるのは呼ばれる側の遷移である
- **7 番目の外部イベント消費（`take_clicked_for`）はここに無い**（確定事実 2・#699）。`view.rs` が snapshot publish の後で行う
- **`reset()` の `rows_generation` bump は末尾のクリック照合と結ばれている**（確定事実 3）
- **`drain_launch` の 3 分岐は自前の `request_repaint` を持たない**——deadline を張る唯一の主体は `notice.remaining()` ブロック（`view.rs`）である（確定事実 4a）
- **folder drain の前後関係**: 後ろは #699 の世代照合、前は `reset()` の `folder_gen` bump（`accept_folder_result` の stale 棄却が成立する根拠）（確定事実 4b）

**`font_stack.rs`**

- **`OnceLock` は set-once・never-clear**（`transmute` による `'static` 化の健全性がそれに依存する。既存 doc を移設）
- 消費者は `view.rs` と `results_view.rs` の**両方**である（だから独立モジュールである）

**`view.rs`（`//!` を更新）**

- 反映境界は 4 つあり 1 つの名前に畳んでいない（確定事実 5）
- 入力変換は pre-widget / post-widget の 2 段である（確定事実 1・#700）

## 5. 照合表: `update()` の 34 段と抽出後の対応

`research.md`「`update()` の文の実行順序」の 34 段を、抽出後にどの呼び出しが担うかで並べる。**この表が §2.1 の検証 1 の照合表である。**

| 段 | 抽出後の担い手 |
|---|---|
| 1 | `view`（drag・`frame.drag_window()`） |
| 2 | `view`（`read_visual`） |
| 3 | `controller.consume_reset_pending()` + `view`（`ResultsWindow::reset_size_guard`） |
| 4 | `view`（`ctx` clone） |
| 5–6 | `controller`（index 世代検知 / hotkey 失敗の消費） |
| 7–9 | `view`（反映境界 3 つ・個別に残す） |
| 10–12 | `controller`（`drain_launch` / notice 期限 / folder drain） |
| 13 | `view`（`read_pre_widget_input`・**消費もここ**） |
| 14–17 | `controller`（focus / Escape ラダー / blur 猶予） |
| 18–20 | `controller`（`move_selection` / → / ←） |
| 21 | `view`（TextEdit 構築） |
| 22 | `controller.on_input_changed()` |
| 23 | `view`（`request_focus`） |
| 24–25 | `view`（status 行 / toast 描画） |
| 26 | `controller.handle_toast_action()` |
| 27 | `controller`（trailing debounce） |
| 28 | `view`（`read_post_widget_input`）→ `controller`（flush / activate） |
| 29–30 | `view`（`plain_results_hidden` / snapshot publish） |
| 31 | `view`（`take_clicked_for`）→ `controller.activate_or_execute()` |
| 32 | `view`（main のサイズ） |
| 33 | `view`（`drive_results_window`） |
| 34 | `controller.set_focused()` |

## 6. スコープ外（この段で**やらない**と決めたこと）

- #751 の修正（§3.8）
- 全域 `Effect` enum（§3.4）・`Option` 群の ADT 化（§3.5）・明示 mode enum の追加（確定事実 6）
- `lang()` / `indexing()` / `instant_prefix()` の**呼び出し回数の削減**——確定事実 4 が示すとおり「弱く見える」読みが load-bearing でありうる。本段は所有を動かすだけで、読みの束ね直しは行わない
- z-order・窓の show/hide（段 1 の `window_coordinator.rs` の領分）
- `SPEC.md` の更新——挙動を変えないため。加えて `SPEC.md` は `view.rs` も `SearchWindowView` も 1 度も名指ししていない（grep 実測。`egui_shell` の言及は L92 の `icon_textures.rs` と L420 の `create` の 2 箇所のみで、どちらも本段が触らない）
