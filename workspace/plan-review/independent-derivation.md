# #666 段 3 — 独立導出（`egui_shell/view.rs` の責務分割）

作成: 2026-07-27 / 導出者: 独立導出担当（`workspace/plan.md`・`workspace/research.md`・`workspace/plan-review/` の他ファイルは未読で導出）。

> **開示（汚染 1 件）**: 導出中、`git grep` を `workspace/` 除外なしで 1 回実行してしまい、`workspace/research.md` と `workspace/plan.md` の**一部行がツール出力として視界に入った**（ファイル一覧・移設候補の表の断片）。以降は全 grep で `workspace/` を除外した。以下の結論は自分でコードと issue を読んで組み立てたものだが、**「完全に独立」とは主張しない**。特に「font 群を別ファイルへ」「`LauncherController` / view 5 フィールド残置」の方向性は、断片を見る前に自分で立てた仮説と一致していたのか、見たことで補強されたのかを内省では切り分けられない。**一致を独立確認の証拠として扱わないこと。**

---

## 0. 実測の証跡（列挙の SSOT はツールに問うた）

| 問い | コマンド | 結果 |
|---|---|---|
| `egui_shell/` の追跡ファイル数 | `git ls-files src-tauri/src/egui_shell/` | 12 ファイル / 6114 行（`view.rs` = 1869 行 = 30.6%） |
| `view.rs` のトップレベル項目 | `grep -n "^\(pub(crate) \)\?\(static\|const\|fn\|type\|enum\|struct\|impl\|mod\)"` | 19 項目 |
| `SearchWindowView` のフィールド | `sed -n '244,289p' \| grep -c "^    [a-z_]*:"` | **19** |
| インヘレントメソッド | `grep -c "^    \(pub(crate) \)\?fn "` = 34、内訳 = 25 + `EguiView` 2 + テスト 7 | **25** |
| `update()` の行数 | L1033–L1762 | **730 行**（ファイルの 39%） |
| テスト | 7 件（`font_definitions_*` 4 + `font_covers_cjk_*` 3） | **全件がフォント群のテスト。他 18 メソッドにユニットテストは 0 件** |
| `view.rs` が出す trace 名 | `grep -o '"egui_[a-z_]*\(:[a-z_]*\)\?"' … \| sort -u` | **14**（行頭アンカーの `trace_main(\s*"` では 7 件しか出ず、複数行呼び出しを落としていた・5.5-5） |
| `ui.visuals_mut()` の使用 | `grep -rn visuals_mut src-tauri/src/` | **0 件**（issue 事実 5 の「反映境界 4 つ」のうち本 view に在るのは 3 つ） |
| `font.rs` という basename の既存 | `git ls-files \| grep font.rs` | **`snotra-settings/src/font.rs` が存在**（新モジュールを `font.rs` と名付けてはならない・1.2） |
| 間接参照の母集団（**和集合**） | `git grep -n -E "view\.rs\|SearchWindowView\|driver\|egui_shell::view" -- ':!docs/superpowers' ':!workspace' ':!package-lock.json'` | **122 行 / 31 ファイル**（全件裁定は 5.4） |

**個別 grep の件数は母集団の代わりにならない。** 初稿では `view.rs`（69 行）・`SearchWindowView`（11 行）・`driver`（`src-tauri/src/**` に限って 48 行）を**別々に**数え、その結果 3 件が表から落ちた——`strings.rs`（`view.rs` にしか掛からない・3 箇所）、`commands/instant.rs`（4 パターン目にしか掛からない）、`snotra-core/src/{engine,folder}.rs`（`src-tauri/src/**` に絞った集計の外・3 箇所）。**和集合を取り、1 行ずつ三値で閉じる**（5.4）。

**凍結扱いにしたもの**: `docs/superpowers/plans/*` と `docs/superpowers/specs/*`（日付入りの実施記録）、`docs/adr/*`（決定記録）。#749 が `drive_results_window` を移設したとき、これらは更新されていない（`docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md` が今も `view.rs | Modify: drive_results_window（694-734）` と書いている）。**先例に従い本段でも触らない。**

---

## 1. 分割の方針

### 1.1 判別規則（1 本・例外ゼロ）— 規則 S

ADR-0008 が段 1 で規則 R を 1 本に絞った理由（「例外を持つ規則は、次に線を引く人に『今回も例外では』と考える余地を残す」）をそのまま踏襲する。段 3 の線は次の 1 本で引く。

> **規則 S**（射程 = **状態**。`AppHandle` のような能力／依存は射程外）: **main 窓（OS 窓 + その `egui::Context`）へ最後に適用した値の memo と、その適用・描画のコードは `MainView`。それ以外の状態と遷移はすべて `LauncherController`。**

**例外はゼロである。** 射程を「状態」と明示することで `app_handle` は但し書きではなくなる——`tauri::AppHandle` は保持者が増えても増えない `Arc` ハンドルであり、**同期の要る状態ではなく、唯一のアプリへの参照能力**である（1.3）。ADR-0008 が「例外を持つ規則は、次に線を引く人に『今回も例外では』と考える余地を残す」と定めた基準に、この形なら耐える。

適用結果（19 フィールド全件を裁定・三値で閉じる）:

| フィールド | 行き先 | 規則 S の当て方 |
|---|---|---|
| `applied_font_family` | MainView | Context へ `set_fonts` した値の memo |
| `applied_background_hex` | MainView | OS 窓へ `set_background_color` した値の memo |
| `last_set_width` | MainView | OS 窓へ `set_size` した値の memo |
| `last_set_height` | MainView | 同上 |
| `app_handle` | **両方**（clone を 2 本持つ） | **規則 S の射程外**（状態ではなく能力・1.3） |
| `state` / `search_debounce` / `last_input_at` / `folder_tx` / `folder_rx` / `folder_cache` / `folder_error` / `instant_rows_query` / `launching` / `last_seen_index_generation` / `notice` / `notice_base` / `was_focused` / `unfocus_at` | LauncherController | 窓への適用 memo ではない（14 件） |

**規則 S が段 1 の先例と整合することの検算**: `ResultsWindow` は「results 窓へ最後に適用したサイズ」を所有する（#749）。main 窓の同種 memo（`last_set_*`）は ADR-0008 却下 5 で**意図的に view 側へ残された**。規則 S はその 2 つを同じ言葉で説明する——「窓への適用 memo は、その窓を駆動する側が持つ」。段 1 の判断を 1 つも巻き戻さない。

**規則 S が issue の 4 armed 期限と整合することの検算**: `src-tauri/CLAUDE.md` は「armed 期限は 4 つ（検索 debounce・一時通知・起動タイムアウト・blur 猶予）で全数がこの形」と全称で書いている。規則 S を当てると 4 つとも `LauncherController` へ入る（`search_debounce`+`last_input_at` / `notice`+`notice_base` / `launching` / `unfocus_at`）。**4 つが 2 型に割れないことは、この規則の正しさの独立な傍証である**——割れていたら CLAUDE.md の「4 つ」が 2 か所を指す記述になり、issue が警告する「件数の記述が黙って嘘になる」形に落ちていた。

### 1.2 3 つ目のモジュール（`font_stack.rs`）— issue のたたき台に無いが必要

issue の分離図は `LauncherController` / `MainView` の 2 つしか挙げていない。しかし `view.rs` L27–202 のフォント群 9 項目 + テスト 7 件（**計 281 行 = ファイルの 15%**）は**どちらでもない**:

- `LauncherController` ではない（検索・起動・通知のどれでもない）
- `MainView` 専属でもない——**`results_view.rs` が `crate::egui_shell::view::configure_japanese_font` を 2 箇所（L442・L478）から呼んでいる**。つまり results 窓の view が main 窓の view のモジュールに依存している。これは現状の依存の向きの歪みであり、段 3 で view.rs を触る以上ここで直すのが最も安い

規則 R（ADR-0008）の「複数のモジュールから消費されるものは残す」を素直に当てると「親に残す」になるが、親（`view.rs`）自体が解体対象なので、**同じ精神で「両消費者から等距離の兄弟モジュールへ出す」**のが正しい帰結である。新モジュールを 1 つ立てる。

**名前は `font.rs` にしてはならない。`font_stack.rs` を推す**（実測: `git ls-files | grep font.rs` → **`snotra-settings/src/font.rs` が既に存在する**）。理由 3 つ:

1. **`governance:check` G1 が basename 照合であり、衝突すると両 crate で判別力を失う**（`scripts/governance-check.mjs` の注記どおり「wrong-directory 検出は放棄」）。順方向は「`src-tauri/CLAUDE.md` に `font.rs` と書いてあるが実体は snotra-settings 側」でも通り、逆方向は「`snotra-settings/CLAUDE.md` に書いてあるから src-tauri 側の新ファイルも満たされている」でも通る。**本段で唯一の機械ゲートを、自分で盲目にする。**
2. `snotra-settings/CLAUDE.md` が `font.rs` を裸のバッククォートで 3 箇所（L19・L72・L143）参照している。crate 間で読むと指し先が一意でなくなる。
3. `view.rs` L85 のコメント `// TTC face 指定（settings font.rs:138 と同型）` が新ファイルへ移る。**自分のファイル名で他 crate のファイルを指すコメント**になり読めない。移設時に `snotra-settings/src/font.rs:138` へフルパス化する。

`font_stack.rs` を推すのは、この module が組み立てるものが**まさに family fallback スタック**であり（テストが「jp_font 単一スタック」「user_font 先頭」を固定している）、`icon_textures.rs` / `results_window.rs` / `window_coordinator.rs` の「概念 snake_case」命名と揃うため。次点は `fonts.rs`（複数形で衝突は避けられるが、1 文字差は誤読を招く）。

### 1.3 `app_handle` の 2 本持ち（規則 S の射程外であって、例外ではない）

`MainView` は `read_visual` / `get_window("main")` / `ResultsShared` / `ResultsWindow` / `UpdaterUiState` / `drive_results_window` / `wake_results` のために、`LauncherController` は `AppState` 読み・`emit`・worker spawn のために、それぞれ `AppHandle` を要る。

`MainView` が `self.controller.app()` 越しに借りる形は**借用検査で詰む**——同一文で `self.controller` を不変借用しつつ `self.controller.drain_launch(&mut ...)` を呼ぶ箇所が `update()` に複数ある。ゆえに **`MainView` も `app_handle` の clone を 1 本持つ**。

これは規則 S の例外に見えるが、そうではない: `tauri::AppHandle` は `Arc` ベースのハンドルであり、clone は**同一のアプリを指す**（`mod.rs:289` が `tauri::Window` について同じ性質を実測済みと記録している）。**状態を 2 本持つのではなく、同じ 1 つへの参照を 2 本持つ**——ゆえに規則 S（射程 = 状態）の外にある。この一文を `MainView` のフィールド doc に必ず書く（書かないと「同期が要る 2 つの状態」に見える）。

### 1.4 `update()` をどう割るか — issue の 7 事実への応答

`update()` の 730 行は **1 本の関数のまま残す**。ただし本文を「順序が意味を持つ 20 ステップの背骨」へ縮め、各ステップを名前付きメソッド 1 呼び出しにする。

- **事実 1（pre-widget / post-widget の 2 段）への応答**: 入力処理を `consume_pre_widget_keys`（Escape・↑↓ の**消費**・→←）と `handle_post_widget_keys`（Enter・Shift+Enter）の **2 メソッドに割り**、それぞれの doc に「TextEdit より前でなければならない理由（#700・キャレット飛び）」「TextEdit より後でなければならない理由（`response.changed()` 依存・IME 確定/paste の同一 pass）」を書く。**冒頭の 1 reducer へ畳まない。**
- **事実 3（最長の順序制約が `update()` の全長にわたる）への応答**: `SearchState::reset()` が `rows_generation` を進め、末尾の `take_clicked_for` がその世代で照合する——この 2 端点を**同時に含むスコープは `update()` しかない**。ゆえに `update()` 自身の doc コメントに「この関数の冒頭の reset と末尾のクリック照合は 1 本の制約で結ばれている」と明記し、**冒頭の消費群を 1 つの抽出関数へまとめない**（まとめると制約の片端が関数の外へ落ちる）。
- **事実 2（外部イベントの破壊的 take は 6 ではなく 7）への応答**: 包括的な名前（`consume_pending_events` 等）を**作らない**。7 つ目（`take_clicked_for`）が snapshot publish の後という別フェーズに固定されている以上、「全部ここ」と読める名前はその例外を不可視化する。**件数を doc に書かない**（issue の警告どおり、分割で数が変わると黙って嘘になる）。書くなら「`take_clicked_for` だけは publish の後（#699）」という**位置の主張**にする。
- **事実 4（load-bearing に見えない 2 つの順序）への応答**: `poll_notice` の doc に「`drain_launch` の timeout/Failed/Disconnected 3 分岐は自前の `request_repaint` を持たない。**main の deadline を張る唯一の主体はこのブロックである**」と書く。`drain_folder` の doc に「後ろへは #699 のクリック逆流の世代照合、前へは `reset()` の `folder_gen` bump が `accept_folder_result` の stale 棄却を成立させている」と書く。
- **事実 5（反映境界は 4 つ）への応答**: `apply_visuals` に**まとめてよいのは 3 つだけ**（`ctx.set_visuals` / `ctx.set_fonts` / `window.set_background_color`）。`ui.visuals_mut()` はこの view では 1 箇所も使っていない（実測: `grep -c visuals_mut src-tauri/src/egui_shell/view.rs` → **0**、`grep -rn visuals_mut src-tauri/src/` → **0 件**）ので、**「4 つの境界を扱う」と書いてはならない**（`AGENTS.md`「検証の作法」の全称表現則）。さらに **L1139–1140 の既存コメント「`set_visuals` はウィジェット描画より前である必要がある」は egui 0.35.0 では成立していない**（issue 事実 5・root `Ui` が pass 冒頭で `global_style()` を `Arc` snapshot する）。移設時にこのコメントをそのまま運ぶと**誤りを新しいファイルへ焼き直す**ことになる。「位置は #751 の解決まで動かさない（現行挙動の保存）。理由は当初の説明とは別である」へ書き換える。**これは挙動変更ではなく記録の訂正である。**
- **事実 6（モードは導出のまま）への応答**: `ViewKind` / `QueryIntent` を `LauncherController` のフィールドに**しない**。`in_tool` / `in_folder` は `update()` のローカルのまま（今と同じく `controller.view_kind()` から毎回導出）。`enum ViewState` への代数化は本段でやらない。
- **事実 7（全域 Effect enum は作らない）への応答**: 新設する outcome 型は **`ToastAction` の移設のみ**（既存型・新規ゼロ）。`Vec<Effect>` 型の dispatcher は作らない。

---

## 2. 新規ファイル（2 件）

### 2.1 `src-tauri/src/egui_shell/font_stack.rs`（新規・約 300 行）

**内容**: `view.rs` L27–202 の 9 項目 + L1765–1868 のテスト 7 件を**中身を 1 字も変えずに**移す（例外は L85 のコメントのフルパス化・1.2 の理由 3）。

`//!` は `view.rs` の現 `//!` L5–8（フォント登録の 3 枝の説明）を**そのまま引き取る**。加えて「main / results の 2 Context から呼ばれる」ことを明記する（現状 `USER_FONTS` の doc L114–115 にしか書かれていない事実で、モジュールの存在理由そのもの）。

**移設で崩れないことの確認点**: `USER_FONTS` / `JP_FONT_BYTES` はプロセス全体で 1 本の `OnceLock` である。ファイルが変わってもシングルトン性は変わらない（`static` はモジュールパスに依らずプロセス唯一）。**ただし「誤って両ファイルに残す」と 2 本になり、`jp_font_bytes()` の `transmute` 健全性の根拠（set-once・never-clear）が静かに壊れる。** 移設は copy ではなく move であることを PR diff で確認する（`view.rs` 側の削除行数 = `font_stack.rs` 側の追加行数）。

**`snotra-settings/src/font.rs` と統合してはならない**: 両者とも「CJK フォントを egui Context へ登録する」が、`egui_shell` 側は `FontData::from_static`（#689: `from_owned` だと epaint が全体を深くコピーし常駐が 2 倍）、settings 側は `from_owned` + Semibold ファミリ登録 + `face_index_valid` 検証と、**要件も戦略も異なる**。`/dry-check` が「同等ロジックの重複」として拾いうる形をしているので、`//!` に非統合の理由を 1 行置く（`AGENTS.md`「消す/共通化する前に、同じ表層形が複数の概念を担っていないか分類する」）。

### 2.2 `src-tauri/src/egui_shell/launcher_controller.rs`（新規・約 700 行）

**内容**: `LauncherController` 型（15 フィールド）+ 23 メソッド + private 型 4（`FolderMsg` / `LaunchWork` / `LaunchTag` / `LaunchInFlight`）+ `ToastAction`。

`//!` に書くべき責務: 「main 窓の検索セッションを駆動する imperative shell。検索状態・folder ロード・起動 worker・一時通知・4 つの armed 期限を所有する。**egui の描画は 1 行も持たない**（`&egui::Context` は `request_repaint*` のためだけに受ける）。**窓への適用 memo は持たない**（規則 S・`view.rs` 側）」。

**`&egui::Context` を受けることについて**: これは描画ではなく wake である。`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」が「自窓の Context を持っている場所では `ctx.request_repaint()` が正しい」と定めており、controller は view から自窓の Context を渡される。**`WindowWaker` へ差し替えてはならない**（あちらは外部スレッド・別窓用）。この一文を `//!` に置く。

---

## 3. 修正ファイル

### 3.1 コード（コンパイラが漏れを捕まえる）

| ファイル | 変更 |
|---|---|
| `src-tauri/src/egui_shell/view.rs` | 1869 → 約 600 行。`SearchWindowView` → `MainView`（3.4 参照）。4 フィールド + `app_handle`。`window_width` / `draw_toast_button` / `ToastAction` 以外の 24 メソッドを移設。`//!` を全面改稿（フォント 3 枝の段落は `font_stack.rs` へ移す） |
| `src-tauri/src/egui_shell/mod.rs` | `mod font;` `mod launcher_controller;` を追加。`use crate::egui_shell::view::SearchWindowView;` → `MainView`。L308 の `SearchWindowView::new` → `MainView::new`。**再エクスポート 12 行のコメントの消費者名を全件更新**（3.3 で全件列挙） |
| `src-tauri/src/egui_shell/results_view.rs` | L442・L478 の `crate::egui_shell::view::configure_japanese_font` → `crate::egui_shell::font_stack::configure_japanese_font`（**コード 2 箇所**）。L1 の `//!` の `SearchWindowView` → `MainView`。L475 のコメント「view.rs の `applied_font_family` 比較と同型を複製」は `view.rs` のままで正しい（memo は MainView に残るため）——**検算済み・編集不要** |
| `src-tauri/src/egui_shell/window_coordinator.rs` | L9・L405 の `SearchWindowView` → `MainView`。L405 の「呼び出し元は…**1 か所**である」は件数の主張——`drive_results_window` の呼び出し点が 1 のままかを実測してから残す。L19 の `` `view.rs` にある `` は**ファイル名を変えないので真のまま**（検算済み・編集不要） |
| `src-tauri/src/egui_shell/results_window.rs` | L7・L54 の `SearchWindowView` → `MainView`。L4・L37・L163 の `` `view.rs` `` は真のまま（検算済み・編集不要） |
| `src-tauri/src/commands/launch.rs` | L50 の `egui_shell::view::SearchWindowView::activate` / `execute_tool_selected` → `egui_shell::launcher_controller::LauncherController::activate` / `execute_tool_selected`（**モジュールもパスも両方変わる**。バッククォート内なので `cargo doc` の intra-doc link 検査には掛からない = **compile-fail しない**） |
| `src-tauri/src/commands/instant.rs` | L19 の `egui_shell::view::execute_instant_selected` → `egui_shell::launcher_controller::...`（同上・非 compile-fail） |
| `src-tauri/src/egui_shell/visual.rs` | L86 の doc「`configure_japanese_font` を呼び、`applied` を更新する」——モジュール修飾を `font_stack::configure_japanese_font` に明示。L170 の「`view.rs` の `hex_color_parses_and_falls_back` の後継」は**既に消えたテストへの歴史的参照**ゆえ据え置き（検算済み・編集不要） |
| `src-tauri/src/egui_shell/strings.rs` | **doc 3 箇所**。L5 `//!`「言語は呼び出しごとに引数で受ける——**view.rs の `lang()`** が…」→ `lang()` は `LauncherController` へ移るのでモジュール名を更新。L109・L126「呼び出し側で整形すると **`view.rs` のインライン処理**になり、検知手段が視覚スモークだけになる」→ 呼び出し側が 2 つに割れる（notice 文言 = controller、hint/overlay/toast = view）。**「`view.rs` の」を落として「呼び出し側のインライン処理」とするのが最小かつ正しい**（規範の主張は「どのファイルか」ではなく「整形をテーブルの外へ出すな」であるため） |
| `src-tauri/Cargo.toml` | L15 のコメント「`SearchWindowView` が `EguiView` を実装するため」→ `MainView`（3.4 を採る場合のみ） |

### 3.2 `.md` / スキル（コンパイラを持たない機構 — false green で残る）

| ファイル | 変更 | 漏らすと何が起きるか |
|---|---|---|
| `src-tauri/CLAUDE.md`「モジュール構成」`egui_shell/` 行 | **`launcher_controller.rs` と `font_stack.rs` をバッククォート付きで追記**（`mod.rs` + 12 → 14 ファイルの列挙）。`view.rs` の責務散文を改稿 | **`governance:check` の G1 が落とす**（`scripts/governance-check.mjs:117` の逆方向照合: `src-tauri/src/` 配下の全 production `.rs` の basename が本文のバッククォートに出現すること）。**ここは唯一の機械ゲートである** |
| `src-tauri/CLAUDE.md`「フォント登録（混在スクリプトのベースライン）」 | 「規則自体は `egui_shell/view.rs` の `font_definitions_*` テスト群が固定している…規則の正本はテストと `view.rs` の `//!`」→ `font_stack.rs` ×2 | 「正本はここ」の指し先が空を指す。G1 は basename しか見ないので**この誤りは検出されない** |
| `src-tauri/CLAUDE.md` 「armed 期限は 4 つ」「`unfocus_at` / `was_focused` は…backstop の外」 | **検算した結果、編集不要**（どちらもファイル名・型名を名指していない。4 つが 1 型に収まる） | — |
| `docs/architecture.md` L125 | 「search_state.rs の `interpret` でモード判定・**view.rs が直呼び実行**」→ `launcher_controller.rs` が直呼び実行 | instant/slash の実装位置を誤って教える |
| `docs/architecture.md` L153–178 の mermaid 検索フロー図 | `participant View as egui_shell/view.rs (main)` が **`interpret` / `Debouncer` / `engine.search` / クリック消費起動**の 4 メッセージを担っている。これらは全て `LauncherController` へ移る。**participant を 2 本に割る**（`View as egui_shell/view.rs (main 描画)` + `Ctl as launcher_controller.rs`）か、participant のラベルを両方指す形へ改める | 図が実装と食い違う。**序数・件数ではないが「誰が誰を呼ぶか」の構造の写しであり、分割で最も静かに腐る種類** |
| `docs/architecture.md` L80 / L147 / L156 / L174 | **検算した結果、L80・L147 は編集不要**（「描画」の主体は今も `view.rs`）。L174 は歴史的記述（「#646 PR2 で view.rs から移管」）ゆえ据え置き | — |
| `PERFORMANCE.md` L157 / L179 | 「実装は `egui_shell/view.rs` の `font_covers_cjk` / `configure_japanese_font`」「`egui_shell/view.rs` の `font_definitions` の doc コメント」→ `font_stack.rs` ×2 | 性能改善の一次記録が空を指す |
| `.claude/skills/state-check/SKILL.md` L40 | 「`src-tauri/src/egui_shell/view.rs`（キー入力分岐・driver）」→ キー入力分岐は `view.rs`、driver は `launcher_controller.rs` の 2 項に分ける | **`/state-check` は「対象コードを読む」の置き場所リストでこの行だけを頼る**。driver が指す先が消えると、次に `/state-check` を回した人が検索状態の遷移を読まずに合格判定を出す。**スキルは沈黙で false green を作る典型** |
| `snotra-core/tests/search_frame_cost.rs` L3 | `//!` の「`src-tauri/src/egui_shell/view.rs` の `run_search_with`」→ `launcher_controller.rs` | 別 crate のテストの `//!` にある**クロス crate の所在主張**。`src-tauri` を grep しても出ない |
| `snotra-core/src/engine.rs` L32・L158 / `snotra-core/src/folder.rs` L140 | 「呼び出し側（**src-tauri driver**）へ露出させないため」「**driver がキャッシュし**、」「**driver がキャッシュして**打鍵ごとに `filter_sorted` で同期に絞る」 | **`folder_cache` は `LauncherController` へ移る。この 3 行はその所在を概念ラベルで指すクロス crate 参照であり、`src-tauri/` を grep しても・`SearchWindowView` で grep しても・`view.rs` で grep しても 1 件も出ない。** 「driver」を `src-tauri/src/**` に限って数えた最初の集計（48 行）はこれを取りこぼしていた |
| `SPEC.md` L199 | 「（**driver** が ↑↓ のキーイベントを消費して入力欄へ渡さないことで担保する）」（#700） | **検算した結果、編集不要**。SPEC は意図の文書で「driver」は役割語として使われており、消費の実体は `MainView::consume_pre_widget_keys` に残る（規則 S）。**ただし挙動を変えないので `SPEC.md` 同期は不要**という判断を PR 本文に書く（`AGENTS.md`「『fix』でも文書化された挙動を変えたら仕様変更」の裏返しの明示） |

**凍結（触らない）と判断したもの**: `docs/adr/0003`・`docs/adr/0008`・`docs/development-principles.md` L42（いずれも #749 当時の記録として `view.rs` を名指す歴史的記述）、`docs/superpowers/plans|specs/*`（30 ファイル）。

### 3.3 `mod.rs` の再エクスポートコメント（12 行）— 分割で最も落ちやすい

`mod.rs` は各 `pub(crate) use` の直前に「誰が消費するか」をコメントで書く規約を持つ。分割後、その「誰が」の大半が `view.rs` から `launcher_controller.rs` へ移る。**シンボル名の grep では 1 件も掛からない**（コメントは `view.rs（driver）` という略記）。`docs/development-principles.md` L42 が #749 で同型の漏れ（4 箇所直して実際は 6 箇所）を記録している、まさにその再発点である。

| mod.rs 行 | 対象 | 分割後の正しい消費者（実測で確定させること） |
|---|---|---|
| L13 | `LAUNCH_TIMEOUT` / `NOTICE_LAUNCH` / `NoticeSlot` | `launcher_controller.rs` |
| L15 | `NOTICE_HOTKEY` | `launcher_controller.rs`（hotkey 失敗の `notice.set`） |
| L15 | `OverlayKind` / `overlay_kind` | **`view.rs`**（status 行の描画）——**同じ 1 行に消費者が割れる。行を分ける必要がある** |
| L18 | `ToastKind` / `UpdaterPhase` / `UpdaterUi` | `ToastKind` は `view.rs`（描画）、`UpdaterPhase` は `launcher_controller.rs`（`spawn_install`）。**ここも割れる** |
| L27–32 | `window_coordinator::{...}` | `drive_results_window` / `wake_results` は `view.rs`、`hide_egui_main` / `show_egui_main` / `wake_main` は `mod.rs` + `main.rs`（view からは呼ばない・emit 経由） |
| L38–43 | `ClickTake` / `ResultsShared` / `RowsSnapshot` | `view.rs`（publish + take）——変わらず |
| L46 | `ResultsWindow` | `view.rs`（`reset_size_guard`）——変わらず |
| L49 | `RowTheme` / `VisualApplied` / `VisualSnapshot` | `view.rs`——変わらず |
| **L52** | `IconMsg` / `needs_extraction` / `png_to_color_image` / `retain_visible` | **既に嘘**。コメントは「view.rs の icon texture driver が消費する」だが、実測（`git grep`）では消費者は `results_view.rs` のみ（#646 PR2 で移管済み）。本段の対象外だが**同一クラスの現存ドリフトなので、監査した以上ここで直す** |
| L54–56 | `BLUR_GRACE` / `BlurAction` / `blur_grace_action` | `launcher_controller.rs`。`HotkeyPlan` / `plan_hotkey` は `main.rs`——変わらず |
| L57–61 | `search_state::{EscapeOutcome, QueryIntent, SearchState, ViewKind, compute_parent_dir, folder_load_pending, should_flush_on_enter}` | 大半 `launcher_controller.rs`。**`ViewKind` だけは `view.rs` も読む**（TextEdit の `in_tool` / `in_folder`） |
| L62 | `SlashCmd` / `find_slash_command` | `launcher_controller.rs` |
| L64 | `needs_index_refresh` / `plain_results_hidden` | `needs_index_refresh` は `launcher_controller.rs`、`plain_results_hidden` は `view.rs`（snapshot publish の直前・#752 F2 の 1 回読み）。**割れる** |
| L66 | `Debouncer` | `launcher_controller.rs` |
| L67 | `strings as ui_strings` | **両方**（notice 文言 = controller、hint / overlay / toast = view） |

**`ViewKind` / `strings` / `OverlayKind` / `plain_results_hidden` のように消費者が 2 モジュールに割れるものが 5 グループある。** 「view.rs（driver）が消費する」の 1 語を機械置換すると全部誤りになる。**1 行ずつ裁定すること。**

### 3.4 型名 `SearchWindowView` → `MainView`（推奨・ただし分離可能）

**推奨する。** issue が `MainView` を成果物の名前として指定しており、`results_view.rs` の `ResultsView` と対になる。リネームは compile-fail で全コード参照を挙げるため安全（コード 4 箇所: `mod.rs` ×2、`view.rs` ×3）。手作業が要るのは doc の 7 行（`window_coordinator.rs` ×2、`results_window.rs` ×2、`results_view.rs` ×1、`commands/launch.rs` ×1、`Cargo.toml` ×1）。

**ファイル名 `view.rs` は変えない。** 理由 3 つ:
1. 分割後の `view.rs` は「main 窓の view」そのものになる——名前は**分割前より正確になる**（今は controller と font も抱えている）
2. `window_coordinator.rs` L19 の `//!` と `src-tauri/CLAUDE.md` の「main のサイズは 2 か所に分かれる——…毎フレームの動的高さは `view.rs`」は、ファイル名を保てば**真のまま**。ADR-0008 却下 5 が「移設では説明文のほうが先に古くなる」と警告している当の記述であり、動かさずに済むならその方がよい
3. `docs/superpowers` の凍結文書 30 件がこのパスを指す。ファイル名を変えると、凍結文書が一斉に空パスを指す（型名の変更では起きない——凍結文書は当時の型名を書いていて、それは当時正しい）

**リネームを丸ごと落としても、責務分割の価値は 1 mm も減らない。** 反対されたら切り離して構わない。

---

## 4. 移動するシンボルの全一覧

### 4.1 `view.rs` → `font_stack.rs`（16 項目）

| # | シンボル | 種別 | 元 | 先 |
|---|---|---|---|---|
| 1 | `JP_FONT_BYTES` | `static OnceLock<Box<[u8]>>` | view.rs L27 | font_stack.rs |
| 2 | `CJK_PROBE` | `const &[char]`（18 文字） | view.rs L35 | font_stack.rs |
| 3 | `font_covers_cjk` | `fn` | view.rs L49 | font_stack.rs |
| 4 | `font_definitions` | `fn` | view.rs L66 | font_stack.rs |
| 5 | `ResolvedFont` | `type` | view.rs L110 | font_stack.rs |
| 6 | `USER_FONTS` | `static OnceLock<Mutex<HashMap<..>>>` | view.rs L127 | font_stack.rs |
| 7 | `resolve_font_family` | `fn` | view.rs L135 | font_stack.rs |
| 8 | `jp_font_bytes` | `fn`（`unsafe transmute` を含む） | view.rs L166 | font_stack.rs |
| 9 | `configure_japanese_font` | `pub(crate) fn` | view.rs L187 | font_stack.rs |
| 10–13 | `font_definitions_fallback_is_jp_single_stack` / `_honor_puts_user_first_jp_fallback` / `_covered_user_font_omits_jp_entirely` / `_registers_both_fonts_as_borrowed` | `#[test]` | view.rs L1775/1787/1803/1820 | font_stack.rs `mod tests` |
| 14–16 | `font_covers_cjk_rejects_unparsable_bytes` / `_rejects_latin_only_font` / `_accepts_japanese_system_font` | `#[test]` | view.rs L1838/1845/1858 | font_stack.rs `mod tests` |

**テストのアサーション文言も含めて 1 字も変えない。** テスト名の一致が「移設であって書き換えではない」ことの唯一の証拠になる（この 7 件が `view.rs` に存在する唯一のユニットテストであり、`src-tauri/CLAUDE.md` が「規則の正本はテスト」と宣言している対象そのものである）。

移設に伴う import の移動: `std::collections::HashMap` / `std::sync::{Mutex, OnceLock}` は `view.rs` からは**不要になる**（`-D warnings` の `unused_imports` が compile-fail で教える）。

### 4.2 `view.rs` → `launcher_controller.rs`（27 項目）

**private 型 4**

| シンボル | 種別 | 元 |
|---|---|---|
| `FolderMsg` | `enum`（`Loaded` / `Failed`） | L206 |
| `LaunchWork` | `enum`（`Normal` / `Tool` / `Instant`） | L216 |
| `LaunchTag` | `enum`（`Normal` / `Tool` / `Instant`・`#[derive(Clone, Copy)]`） | L228 |
| `LaunchInFlight` | `struct`（`started` / `rx` / `tag`） | L238 |

**フィールド 15**（`SearchWindowView` → `LauncherController`）

`app_handle`（clone を保持）/ `was_focused` / `unfocus_at` / `state` / `search_debounce` / `last_input_at` / `folder_tx` / `folder_rx` / `folder_cache` / `folder_error` / `instant_rows_query` / `launching` / `last_seen_index_generation` / `notice` / `notice_base`

**メソッド 23**（すべて `impl SearchWindowView` → `impl LauncherController`）

| # | メソッド | 元 | 備考 |
|---|---|---|---|
| 1 | `emit_hide` | L320 | |
| 2 | `activate` | L339 | `commands/launch.rs` L50 の doc が名指し |
| 3 | `start_launch` | L370 | |
| 4 | `finish_launch` | L431 | |
| 5 | `drain_launch` | L472 | **事実 4: 3 分岐が自前 repaint を持たない** |
| 6 | `clear_search` | L518 | |
| 7 | `execute_slash` | L528 | |
| 8 | `execute_instant_selected` | L565 | `commands/instant.rs` L19 の doc が名指し |
| 9 | `activate_or_execute` | L604 | |
| 10 | `shift_activate` | L617 | |
| 11 | `execute_tool_selected` | L658 | `commands/launch.rs` L50 の doc が名指し |
| 12 | `record_folder_expansion` | L682 | |
| 13 | `resolve_tools` | L700 | |
| 14 | `auto_hide_enabled` | L709 | |
| 15 | `settings_running` | L724 | |
| 16 | `instant_prefix` | L733 | |
| 17 | `indexing` | L741 | **`view.rs` も呼ぶ**（status overlay）→ `pub(crate)` 相当の可視性が要る |
| 18 | `lang` | L750 | **`view.rs` も呼ぶ**（hint / toast 文言）→ 同上 |
| 19 | `spawn_folder_load` | L787 | |
| 20 | `run_search` | L813 | |
| 21 | `run_search_with` | L818 | `snotra-core/tests/search_frame_cost.rs` L3 の doc が名指し |
| 22 | `handle_toast_action` | L918 | 引数 `ToastAction` を伴う |
| 23 | `spawn_install` | L944 | `.await` を含む唯一の経路 |

**`ToastAction`（`enum`・L973）も `launcher_controller.rs` へ**（消費者が語彙を定義する。`view.rs` の描画が生成し controller が消費する）。`view.rs` からは `use super::launcher_controller::ToastAction;`。

**新設アクセサ（`view.rs` が読むために必要・すべて `&self`）**: `view_kind()` / `results()` / `selected()` / `rows_generation()` / `query()` / `folder_filter()` / `tool_frame()` / `notice_message()` / `is_launching()` / `is_debounce_armed()` / `instant_rows_query_is_some()` / `lang()` / `indexing()`。**これらは `SearchState` の既存 accessor への薄い委譲であって、新しい状態ではない。** 数が多いのがこの分割の実費用である（下記 7.3）。

### 4.3 `view.rs` に残る（`MainView`）

**フィールド 5**: `app_handle` / `applied_font_family` / `applied_background_hex` / `last_set_width` / `last_set_height` + `controller: LauncherController`

**メソッド**: `new`（L292 を controller 委譲へ書き換え・**初期値を 1 つも変えない**: `last_set_height: 52.0` / `Debouncer::new(50ms, true)` / `last_set_width: 0.0` / `notice_base: Instant::now()`）、`window_width`（L764）、`setup`（L1019）、`update`（L1033）

**自由関数**: `draw_toast_button`（L987）

**新設 private メソッド（`update()` から抽出・すべて `MainView`）**: `begin_drag` / `consume_reset_for_show` / `apply_visuals` / `consume_pre_widget_keys` / `draw_search_field` / `draw_status_row` / `draw_toast` / `handle_post_widget_keys` / `publish_rows_snapshot` / `apply_main_window_size`

---

## 5. 間接参照の一覧（対象名の素朴な grep では到達しない）

### 5.1 同名・別概念（同じ語が別のものを指す — 機械置換で壊れる）

| # | 語 | 概念 A | 概念 B（別物） | 危険 |
|---|---|---|---|---|
| 1 | **`view.rs`** | `src-tauri/src/egui_shell/view.rs` | **`snotra-egui-runtime/src/repaint.rs` L301・L307 のテスト fixture 文字列**（`egui::RepaintCause { file: "view.rs", line: 439 }` と期待値 `"…; view.rs:439 state changed"`）。egui が返す任意のファイル名を整形する検査であって、本件のファイルとは無関係 | `"view.rs"` の一括置換で**テストが壊れる**（`assert_eq!` の両辺を同時に書き換えれば通ってしまい、意味が失われたことに気づけない） |
| 2 | **`controller`** | 本段で新設する `LauncherController` | **サブエージェント運用の「コントローラ」**。`view.rs` L1691「（Task 5 concern 2 の fix・controller 依頼）」、`results_view.rs` L34「（#532 SU4 の系譜・controller fix 依頼）」 | `LauncherController` 導入後、この 2 行が型名を指すと誤読される。**触らずに残すと意味が変質する**——`launcher_controller.rs` へ移る L1691 は特に危険 |
| 3 | **`view`** | egui の view（`SearchWindowView` / `ResultsView`） | **`ViewKind`（Results / Folder / Tool）= 検索モード**。`search_state.rs` の `view_kind()` / `EscapeOutcome` の doc「直下ビュー（folder/results）」 | `MainView` を導入すると「view」が 3 概念（main view / results view / view kind）になる。**`LauncherController` に `view_kind()` accessor を置くこと自体が語の衝突**——doc で「モード」と言い換える |
| 4 | **`state`** | `SearchWindowView.state: SearchState` | `AppState` / `EguiShellState` / `UpdaterUiState` / `run_search_with` 内のローカル `let state = ...AppState` | 移設時に `self.state` と `state` を取り違えると**別のロックを取る**。L849・L894 に実在 |
| 5 | **`reset`** | `SearchState::reset()`（rows_generation を bump） | `reset_selection` / `ResultsWindow::reset_size_guard`（性能ガード・correctness ではない・#671 決定 2） / `EguiShellState.reset_pending` | 「reset を 1 つの関数へまとめる」は 3 概念を混ぜる |
| 6 | **`launch`** | `start_launch` / `LaunchWork` / `LAUNCH_TIMEOUT` | `spawn_install`（updater の async・別の spawn）/ `launch_query`（履歴記録用の文字列） | `LaunchTag::Instant` と `QueryIntent::Instant` も別概念 |
| 7 | **`generation`** | `rows_generation`（行の総入れ替え） | `folder_gen`（folder ロードの token）/ `index_generation`（AppState の索引ビルド）/ `hotkey_generation`（alt 解放待ち show） | **4 つある。** 事実 3 の順序制約が結ぶのは 1 番目だけ |
| 8 | **`main`** | main 窓 / `MainView` | `main.rs`（エントリポイント）/ `main` ブランチ | 「main が…」の散文（下記 5.2）は 1 番目を指す |
| 9 | **`font.rs`** | 本段で新設しようとしたモジュール | **`snotra-settings/src/font.rs`（実在）**——同じ「CJK フォントを egui Context へ登録する」だが `from_owned` + Semibold ファミリ + `face_index_valid` 検証と、要件も戦略も別（`egui_shell` 側は #689 の常駐 2 倍回避で `from_static` 必須） | **新設をこの名前にすると `governance:check` G1（basename 照合）が両 crate で判別力を失う**。加えて `view.rs` L85 の `// settings font.rs:138 と同型` が自己言及に化ける。→ `font_stack.rs` へ改名（1.2）。**`/dry-check` による統合提案にも `//!` で先回りする**（2.1） |
| 10 | **`driver`** | `SearchWindowView`（検索を駆動する側） | **tauri-driver**（`e2e.yml` L11・`docs/build-commands.md` L43）/ **GPU driver**（`snotra-settings/CLAUDE.md` L167）/ `C:\Windows\System32\drivers\etc\hosts`（`icon.rs` L664 のテスト用パス）/ `webdriverio`（`package-lock.json`） | 概念ラベルとして最も広く効く grep だが、**6 件の無関係 hit を含む**。機械置換は不可 |

### 5.2 同概念・別名（対象名の grep では到達しない参照）

**(a) 概念ラベル「driver」— 48 行 / 9 ファイル。最大の落とし穴。**

`SearchWindowView` を「driver（view.rs）」という**略記**で指す doc が全域に散っている。`SearchWindowView` でも `view.rs` でもなく「driver」で grep して初めて全数が出る。

| ファイル | 件数 | 分割後に指すべき先 |
|---|---|---|
| `search_state.rs` | **21 行** | ほぼ全て `launcher_controller.rs`（L260「view.rs（driver）は現状 `parent_dir()` 越しに…」は特にパス+概念の複合） |
| `mod.rs` | 7 行 | 3.3 の表のとおり 1 行ずつ裁定 |
| `icon_textures.rs` | 5 行 | **全件が既にドリフト**（#646 PR2 で `results_view.rs` へ移管済み。L3 の `//!` が「worker spawn / load_texture の driver は view.rs が持つ」と書くが実測の消費者は results_view.rs のみ） |
| `layout.rs` | 5 行 | `launcher_controller.rs`（`Debouncer` 系）。ただし L7 は下記 (d) |
| `notify.rs` | 2 行 | `launcher_controller.rs`（`notice_base` の注入元） |
| `results_view.rs` | 2 行 | 「窓の可視性・サイズ・位置の driver は main 側」= `view.rs`（変わらず） |
| `window_coordinator.rs` | 2 行 | `view.rs`（`drive_results_window` の呼び出し元） |
| `view.rs` | 3 行 | 自身 |
| **`snotra-core/src/engine.rs`** | **2 行** | **クロス crate。`launcher_controller.rs`**（L32「src-tauri driver へ露出させない」・L158「driver がキャッシュし」） |
| **`snotra-core/src/folder.rs`** | **1 行** | **クロス crate。`launcher_controller.rs`**（L140「driver がキャッシュして…`filter_sorted`」） |
| **`SPEC.md`** | **1 行** | L199（#700 の ↑↓ 消費）。役割語ゆえ**据え置き**（3.2 参照） |

**「driver」で grep すると 6 件の無関係な hit が混ざる（同名・別概念）**: `.github/workflows/e2e.yml` L11 と `docs/build-commands.md` L43 の **tauri-driver**（Playwright）、`snotra-settings/CLAUDE.md` L167 の **GPU driver**、`src-tauri/src/icon.rs` L664 の `C:\Windows\System32\drivers\etc\hosts`（テストのパスリテラル）、`package-lock.json` の `webdriverio` ×2。**機械置換は不可。**

**(b) 概念ラベル「main が / main 側 / main のフレーム / main の update()」— 20 行以上**

`results_view.rs`（L4・L19・L27・L34・L463・L551・L573・L599）、`results_window.rs`（L18）、`window_coordinator.rs`（L228・L230・L234・L249・L300・L315・L416）、`layout.rs`（L159・L189・L463）、`mod.rs`（L274）。

**分割後、これらの「main」が `MainView` を指すのか `LauncherController` を指すのかが割れる。** 例:
- `results_view.rs` L19「main が毎フレーム発行する描画用スナップショット」→ `MainView`（publish は view 側）
- `results_view.rs` L27「結果集合が総入れ替えされるたびに main が加算するカウンタ」→ **`LauncherController`**（`SearchState::set_results`）
- `results_view.rs` L573「main が同フレームで行を差し替えていれば」→ **`LauncherController`**
- `results_view.rs` L34「main が代わりに live 値を snapshot に載せて運ぶ」（`settled`）→ **両方**（値は controller の `search_debounce`、載せるのは view）

**「main」を機械的に片方へ寄せると必ず誤る。** ただし**「main」は窓の名前でもある**ので、多くはそのまま真である。**1 行ずつ「窓を指すのか型を指すのか」を裁定する**のが正しい（全部書き換えるのも、全部据え置くのも誤り）。

**(c) doc コメント内のフルパス参照（コンパイラは見ない）**

`commands/launch.rs` L50 と `commands/instant.rs` L19 は `` `egui_shell::view::SearchWindowView::activate` `` の形。**バッククォート内なのでリンクではなく**、CI の `cargo doc --workspace --no-deps --document-private-items`（intra-doc link 検査・#562）は**これを検出しない**。手で直すしかない。

**(d) 既に嘘になっている参照（本段の監査で発見・先例の再現）**

| 場所 | 記述 | 実測 |
|---|---|---|
| `mod.rs` L52 | 「view.rs の icon texture driver（worker spawn / load_texture 適用）が消費する」 | `git grep` の消費者は `results_view.rs` のみ（#646 PR2 で移管） |
| `icon_textures.rs` L3・L9・L19・L65 | 「driver（view.rs）が消費する」×4 | 同上 |
| `layout.rs` L7 | 「view.rs `RowTheme::path_size` と同係数」 | `RowTheme` は #673 で `visual.rs` へ移設済み |

**この 3 件は本段の変更対象ではないが、`docs/development-principles.md` L42 が警告する当のパターンの現存例である。** 監査でこの目で見た以上、同じ PR で直すのが `AGENTS.md`「照合は SSOT に対して行う」の趣旨に沿う。**放置するなら「見つけたが範囲外として残す」と PR 本文に明記する**（黙って残すと次の監査者が同じ発見コストを払う）。

### 5.3 序数・順序・件数に依存する参照

| 場所 | 記述 | 分割後の扱い |
|---|---|---|
| issue コメント事実 2 | 「外部イベントの消費は **6 箇所ではなく 7 箇所**」 | **この件数を新しい doc に書き写さない。** 抽出で 1 メソッド 1 消費になると数え方が変わる。書くなら「`take_clicked_for` だけが publish の後（#699）」という位置の主張 |
| `src-tauri/CLAUDE.md` | 「**armed 期限は 4 つ**（検索 debounce・一時通知・起動タイムアウト・blur 猶予）で全数がこの形」 | 4 つとも `LauncherController` に収まるので**真のまま**（1.1 の検算）。**ただし片方でも view 側に残す設計を採ると、この全称が黙って壊れる** |
| `src-tauri/CLAUDE.md` | 「**main のサイズは 2 か所に分かれる**」（ADR-0007） | `view.rs` に `last_set_*` を残すので真のまま |
| `window_coordinator.rs` L405 | 「呼び出し元は `SearchWindowView::update()` の末尾 **1 か所**である」 | 型名は変わる。件数は変えない設計だが**実測で確認してから残す** |
| `window_coordinator.rs` L300・L315 | 「main の update() 内 **2 箇所**」「**2 つ**——main の update() と main の Moved リスナー」 | `wake_results` / `position_results_below_main` の呼び出し点。本段で増減しない前提だが**実測で確認** |
| `view.rs` `//!` | 「フォント登録は **3 枝**」 | `font_stack.rs` の `//!` へそのまま移す（枝の数は変えない） |
| `results_view.rs` L1 | `//!` が `SearchWindowView` を名指し | 型名リネームで更新 |
| `docs/architecture.md` mermaid | participant の**本数と矢印の向き**が実装構造の写し | 5.2(b) と同種。participant を割るか、ラベルで両方を指す |

### 5.4 母集団の全件裁定（4 パターンの和集合 — 122 行 / 31 ファイル）

**個別の grep を並べて件数だけ記録すると、1 つの grep にしか掛からないファイルが表から落ちる。** 実際この導出でも初稿で `strings.rs` が落ちた（`view.rs` の grep には出るが `SearchWindowView` にも `driver` にも出ない）。**和集合を取り、1 行ずつ三値で閉じる**のが正しい。

```
git grep -n -E "view\.rs|SearchWindowView|driver|egui_shell::view" \
  -- ':!docs/superpowers' ':!workspace' ':!package-lock.json'
# → 122 行 / 31 ファイル
```

**4 パターン目（`egui_shell::view`）が必要な理由**: `commands/instant.rs` L19 は `` `egui_shell::view::execute_instant_selected` `` と書いており、`view.rs` にも `SearchWindowView` にも `driver` にも掛からない。**3 パターンでは母集団から漏れる。**

| ファイル | 行 | 変更 | 検算済み据え置き | 凍結 | 参照先 |
|---|---:|---:|---:|---:|---|
| `src-tauri/src/egui_shell/search_state.rs` | 23 | 23 | 0 | 0 | 5.2(a) |
| `src-tauri/src/egui_shell/mod.rs` | 17 | 17 | 0 | 0 | 3.3 |
| `src-tauri/src/egui_shell/results_view.rs` | 12 | 5 | 7 | 0 | 3.1・5.2(b) |
| `docs/architecture.md` | 8 | 2 | 5 | 1 | 3.2 |
| `src-tauri/src/egui_shell/layout.rs` | 7 | 7（L7 は**既存ドリフト**・7.4 で同時修正する場合） | 0 | 0 | 5.2(a)(d) |
| `src-tauri/src/egui_shell/view.rs` | 7 | 7 | 0 | 0 | 4 |
| `src-tauri/src/egui_shell/icon_textures.rs` | 5 | 5（**全件が既存ドリフト**） | 0 | 0 | 5.2(d)・7.4 |
| `src-tauri/src/egui_shell/results_window.rs` | 5 | 2 | 3 | 0 | 3.1 |
| `src-tauri/src/egui_shell/window_coordinator.rs` | 5 | 2 | 3 | 0 | 3.1・5.3 |
| `src-tauri/CLAUDE.md` | 3 | 3 | 0 | 0 | 3.2 |
| `src-tauri/src/egui_shell/strings.rs` | 3 | 3 | 0 | 0 | 3.1 |
| `docs/adr/0008-*.md` | 3 | 0 | 0 | 3 | 0 節 |
| `PERFORMANCE.md` | 2 | 2 | 0 | 0 | 3.2 |
| `src-tauri/src/egui_shell/notify.rs` | 2 | 2 | 0 | 0 | 5.2(a) |
| `snotra-core/src/engine.rs` | 2 | 2 | 0 | 0 | **クロス crate**・3.2 |
| `snotra-egui-runtime/src/repaint.rs` | 2 | 0 | 2 | 0 | **同名・別概念**・5.1-1 |
| `docs/adr/0007-*.md` | 2 | 0 | 0 | 2 | 0 節 |
| `snotra-core/src/folder.rs` | 1 | 1 | 0 | 0 | **クロス crate**・3.2 |
| `snotra-core/tests/search_frame_cost.rs` | 1 | 1 | 0 | 0 | 3.2 |
| `.claude/skills/state-check/SKILL.md` | 1 | 1（**要合意**） | 0 | 0 | 3.2・5.5-2 |
| `src-tauri/src/commands/launch.rs` | 1 | 1 | 0 | 0 | 3.1 |
| `src-tauri/src/commands/instant.rs` | 1 | 1 | 0 | 0 | 3.1（**4 パターン目でのみ出る**） |
| `src-tauri/src/egui_shell/visual.rs` | 1 | 1 | 0 | 0 | 3.1 |
| `src-tauri/Cargo.toml` | 1 | 1（型リネームを採る場合） | 0 | 0 | 3.4 |
| `SPEC.md` | 1 | 0 | 1 | 0 | 3.2 |
| `.github/workflows/e2e.yml` | 1 | 0 | 1 | 0 | **同名・別概念**（tauri-driver） |
| `docs/build-commands.md` | 1 | 0 | 1 | 0 | **同名・別概念**（tauri-driver） |
| `snotra-settings/CLAUDE.md` | 1 | 0 | 1 | 0 | **同名・別概念**（GPU driver） |
| `src-tauri/src/icon.rs` | 1 | 0 | 1 | 0 | **同名・別概念**（`System32\drivers\`） |
| `docs/adr/0003-*.md` | 1 | 0 | 0 | 1 | 0 節 |
| `docs/development-principles.md` | 1 | 0 | 0 | 1 | 0 節 |
| **合計** | **122** | **89** | **25** | **8** | — |

**この母集団に入らない参照が 1 クラス残る**: 5.2(b) の「main が / main 側 / main のフレーム」（20 行以上）。「main」は窓の名前でもあるため上の 4 パターンには含めず、**別の grep（`main が|main 側|main の update|main のフレーム`）で独立に裁定する**。含めると同名・別概念の hit 率が高すぎて表が機能しなくなる。

### 5.5 コンパイラを持たない機構（false green で残る）

1. **`governance:check` G1**（`scripts/governance-check.mjs`）: `src-tauri/src/**/*.rs` の全 basename が `src-tauri/CLAUDE.md` にバッククォート付きで出現すること。**`font_stack.rs` / `launcher_controller.rs` の追記漏れは CI の `governance-check` job が捕まえる**（`skip-ci` 非対象・常時実行）。**これが本段で唯一の機械ゲートである。**
2. **`.claude/skills/state-check/SKILL.md` L40**: driver の置き場所リスト。漏れると `/state-check` が検索状態の遷移を読まずに合格を出す。**スキル変更はルート `CLAUDE.md` 最重要ルール 2「エージェント設定（スキル・フック・rules）の変更は合意してから」の対象**——PR に含める前にユーザーの同意を取る。`.claude/rules/safety-nets.md` も配送される。
3. **`e2e.yml` の paths**: `src-tauri/**` グロブなので新規ファイルは自動で載る。**検算済み・変更不要。**
4. **`.claude/rules/src-tauri.md` の paths**: `src-tauri/**/*.rs` グロブ。**検算済み・変更不要。**
5. **smoke script の前提**: `scripts/smoke-egui.ps1` が観測する trace 名は `hotkey:registered` / `egui_show:done` / `egui_results:show` / `egui_hide:done` / `egui_results:hide` の 5 つ。

   **`view.rs` が出す trace は 14 個**（実測: `grep -o '"egui_[a-z_]*\(:[a-z_]*\)\?"' src-tauri/src/egui_shell/view.rs | sort -u` → 14 件）: `egui_instant` / `egui_instant_error` / `egui_launch` / `egui_launch_done` / `egui_results:click_stale` / `egui_search:dispatch` / `egui_slash` / `egui_slash_error` / `egui_tool_enter` / `egui_tool_launch` / `egui_update_install_begin` / `egui_update_install_failed` / `egui_update_install_noop` / `egui_update_install_returned`。**smoke が見る 5 つとは 1 つも重ならない**（= 本段の移設で `smoke:egui` の緑/赤は動かない。裏返せば **`smoke:egui` は本段の回帰を 1 件も検出しない**）。

   > **列挙方法の注記**: 最初に `grep -n 'trace_main(\s*"'` で数えて **7 件**を得たが、これは行頭アンカーで**複数行にまたがる 7 件の呼び出しサイトを取りこぼしていた**。上の `-o` によるトークン抽出が正しい母集団である。**「件数」を書くときは、その件数を出したコマンドが取りこぼしうる形を先に確かめる**——これは issue が警告する「件数の記述が黙って嘘になる」の、本導出内での実例である。

   **trace 名は 1 つも変えない**（移設でモジュールが変わっても文字列は同一に保つ——変えると smoke ではなく手動診断が黙って壊れる）。
6. **`docs/build-commands.md` カテゴリ D の目視**: issue が明言するとおり、フレーム順序の不変条件に**自動検出器は無い**。

---

## 6. 検証手順（「挙動が変わっていない」と言える根拠）

### 6.1 移設の忠実性（最初にやる・最も安い）

1. **`git diff --stat` で加減が釣り合うか**: `view.rs` の削除行 ≒ `font_stack.rs` + `launcher_controller.rs` の追加行。**大きく増えていたら書き換えが混ざっている。**
2. **`update()` の文の順序を機械的に突き合わせる**: 分割前後の `update()` を並べ、**副作用を持つ文の並び**（`ctx.request_repaint*` / `state.*` の変更 / channel の `try_recv` / `swap` / `take` / `set_*` / `emit` / `spawn`）が**1 つも入れ替わっていない**ことを目視で 1 対 1 対応させる。抽出メソッド境界は入れてよいが、境界をまたいだ移動は不可。
   - **具体的に確認する 5 点**（事実 1〜5 に対応）: (i) Escape・↑↓・→← が TextEdit より前、(ii) Enter が TextEdit より後、(iii) `drain_launch` が `reset_pending` 消費より後、(iv) `take_clicked_for` が snapshot publish より後、(v) `drive_results_window` の `result_count` 読みが `take_clicked_for` より後
3. **`indexing()` が 1 フレームに 1 回しか読まれないこと**（#752 F2）: `plain_hidden` を**返り値で運ぶ**設計になっているか。抽出で再読が入ると `cargo test` では落ちない回帰になる。
4. **フォントの `static` が 1 本であること**: `git grep -c "JP_FONT_BYTES\|USER_FONTS"` が `font_stack.rs` 以外で 0 件。

### 6.2 自動検証（`docs/build-commands.md` カテゴリ A + F）

- `cargo build -p snotra`（`-D warnings`。`unused_imports` が `view.rs` の取り残しを compile-fail で挙げる）
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test -p snotra`（**フォントテスト 7 件が `font_stack.rs` で同名のまま緑になること**。テスト名の一致が移設の証拠）
- `cargo doc --workspace --no-deps --document-private-items`（intra-doc link・#562）
- `npm run governance:check`（**G1 が新規 2 ファイルの索引記載を検査する**。カテゴリ F）

**PostToolUse hook の沈黙は `*.rs` については合格を意味する**が、**`*.md` の沈黙は「何も走らなかった」である**（`src-tauri/CLAUDE.md` / `docs/architecture.md` / `PERFORMANCE.md` / `SKILL.md` の編集は hook では検査されない）。`governance:check` を手で回すこと。

### 6.3 手動 GUI smoke（カテゴリ D・**この分割では省略できない**）

issue が明言する: **フレーム順序の不変条件に自動検出器は無い**。trace presence を見る smoke はこのクラスの回帰を緑のまま通した実例がある（#671 PR A′）。加えて `src-tauri/CLAUDE.md`「フォント登録」は**フォント登録に触る変更では `cargo run -p snotra` の目視を省略してはならない**と定めており、本段は font 群をファイルごと動かす。

`npm run smoke:manual` で最低限これを見る（分割で壊れうる順序に対応させた項目）:

| 見るもの | 壊れていたら何が見えるか | 対応する事実 |
|---|---|---|
| 検索ワードを打って ↑↓ で選択 → さらに打鍵 | キャレットがクエリ先頭/末尾へ飛ぶ（`abc` → ↑ → `x` が `xabc`） | 事実 1（pre-widget 消費） |
| IME で変換確定した直後に Enter | 旧クエリの結果で起動する | 事実 1（post-widget） |
| 起動失敗 → 通知が数秒で消えるか | 通知が消えない / 次の入力まで残る | 事実 4（`notice.remaining` が唯一の deadline 主体） |
| 起動して 4 秒放置（dead UNC 等） | 「結果不明」が出ない | 事実 4（`drain_launch` timeout） |
| → でフォルダ展開 → 即打鍵 | ロード中の打鍵が消える | 事実 4（folder drain の前後関係） |
| 結果行をクリックして起動 | 古い行が 1 フレーム描かれる / 誤った行が起動する | 事実 2・3（#699・#752 F2） |
| config の `font_family` を実行中に変更 | 新フォントが 1 イベント遅れる / 適用されない | 事実 5（`set_fonts` の pending） |
| 和文 + Latin 混在行のベースライン | 段差が出る（#399/#579 の再発） | font 群の移設 |
| Escape ラダー（tool → folder → results → hide） | 段が飛ぶ / 復帰しない | 事実 3（`reset()` と世代） |
| Alt+Q で hide → 再 show | 前回のクエリ・通知・in-flight 起動が残る | reset-on-show の backstop |

**エージェントは `smoke:manual` を実行できない**（対話入力が要る）。人間に依頼し、`-PostToPr` か出力の貼り付けで PR に残す。**実施の有無が会話にしか無いと「検証されていない」と「問題が無かった」が区別できなくなる。**

### 6.4 `npm run smoke:egui`（自動回帰の最低線）

`src-tauri/**` を触るので `e2e.yml` が自動起動する（paths は検算済み・変更不要）。**ただし観測するのは 5 つの trace（`hotkey:registered` / `egui_show:done` / `egui_results:show` / `egui_hide:done` / `egui_results:hide`）の presence + orphan 検査だけで、`view.rs` が出す 14 個の trace とは 1 つも重ならない**（5.5-5 で実測）。**すなわち本段の回帰を 1 件も検出しない。** 起動・hide の配線を壊していないことの sanity にはなるが、**緑を 6.3 の代わりにしない**。

### 6.5 ユニットテストを足さないことの明示

`LauncherController` と `MainView` はどちらも `tauri::AppHandle` / `egui::Ui` に依存し、`.claude/rules/src-tauri.md`「Win32 依存モジュールはユニットテスト前提にしない」の層にある。**新規テストは足さない。これは見落としではなく層の性質である**——PR 本文にそう書く（書かないと reviewer が「テストが無い」を指摘として起票する）。移設したフォント 7 件が唯一の自動カバレッジであり、それは維持される。

---

## 7. 落とし穴・リスク

### 7.1 挙動を壊す（優先度 高）

1. **順序制約の 5 点（6.1-2）のどれかが抽出で動く。** `cargo test` では 1 件も落ちない。**この分割で最も起きやすく最も高くつく事故。**
2. **`indexing()` の 2 回読み**（#752 F2）。抽出関数が「必要な値を自分で読む」自然な形にすると再発する。**返り値で運ぶ設計を規律として書く。**
3. **`reset_size_guard` の呼び出し位置**。ADR-0008 却下 6 が「`show_egui_main` は egui イベントループとは別スレッドから走りうる（5 経路実測）」を根拠に**view の reset-on-show に残す**と決めた。`consume_reset_for_show` を `LauncherController` へ丸ごと押し込むと、controller が `ResultsWindow` を触ることになる——**規則 S 違反であり、段 1 の判断を巻き戻す。** view 側に残す。
4. **`ctx.set_visuals` のコメントを訂正せずに運ぶ**。issue 事実 5 のとおり現行コメントは egui 0.35.0 では**成立していない**。移設は「誤った説明を新居へ移す」機会になる。**位置は動かさず、理由の記述だけ訂正する**（#751 を参照）。
5. **`ToastAction` の遅延 dispatch**。`handle_toast_action` は「borrow 外で処理するため」に遅延している。controller へ移すと `self.controller.handle_toast_action(action, &ctx)` になり borrow は自然に解ける——**が、呼び出し位置（toast 描画の直後・snapshot publish より前）を動かさないこと**。
6. **`emit_hide` の dedup が `EguiShellState.hide_pending` にあること**。controller が `app_handle` clone を持つので到達性は変わらないが、**view-local フラグを新設して「controller が持つべき」と誤って移さない**（L317–319 の doc がその危険を名指ししている）。

### 7.2 記録を壊す（優先度 高・検出器が無い）

7. **概念ラベル参照の裁定漏れ**（「driver」51 行 = `src-tauri` 48 + `snotra-core` 3、「main が」20 行以上）。#749 が同型で失敗した（4 箇所直して実際は 6 箇所）。**シンボル名 grep では 1 件も出ない。** 5.4 の和集合で 1 行ずつ「窓を指すのか型を指すのか」を裁定する。
7b. **母集団の取り方そのものが漏れる。** 本導出の初稿は 3 パターンを**別々に**数えて `strings.rs` / `commands/instant.rs` / `snotra-core` の計 3 グループを落とした（0 節）。**個別 grep の件数を並べて「網羅した」と読まない**——`AGENTS.md`「照合は SSOT に対して行う。派生コピー同士の一致を完全性の証拠にしない」の、grep 版の適用である。
8. **`src-tauri/CLAUDE.md` の索引追記漏れ** → `governance-check` job が落とす（唯一の機械ゲート・救い）。**ただし新モジュールを `font.rs` と名付けるとこのゲートが盲目になる**（1.2・basename 照合が `snotra-settings/src/font.rs` と衝突する）。**命名が検出可能性そのものを左右する。**
9. **`.claude/skills/state-check/SKILL.md` の更新漏れ** → **誰も落とさない。** `/state-check` が driver を読まないまま合格を出す形で残る。**かつスキル変更には事前合意が要る**（最重要ルール 2）——PR に含めるか別 issue にするかを先に決める。
10. **`docs/architecture.md` の mermaid の participant** → 誰も落とさない。構造の写しなので最も静かに腐る。

### 7.3 設計としての費用（この分割が「やりすぎ」になりうる点）

11. **accessor が 13 本増える**（4.2 末尾）。`LauncherController` の中身を `MainView` が読むための薄い委譲であり、**責務の分離を「型の壁」で表現した代償**である。壁が薄すぎると、次の編集者は「壁を越えるより controller にメソッドを足す」方を選び、描画ロジックが controller へ漏れ始める。
    - **緩和**: `LauncherController` の `//!` に「**egui の描画は 1 行も持たない**」を不変条件として書く。これは検算可能な全称である（`ui.` / `painter()` / `egui::Frame` / `TextEdit` が 0 件であること）。
12. **`launcher_controller.rs` が約 700 行になる。** `view.rs` の 1869 行よりはよいが、依然大きい。さらに「起動 worker（`LaunchWork` / `LaunchInFlight` / `start_launch` / `finish_launch` / `drain_launch` / `spawn_install`）」を 4 つ目のモジュールへ割ることは**できる**が、**やらない**——issue は 2 つ（+ 本導出で font の 1 つ）を指定しており、順序制約（`drain_launch` の 3 分岐が notice の deadline に依存する等）が 3 ファイルに散ると、事実 4 が守れなくなる。**過分割は順序制約の可視性を下げる。**
13. **型名リネームは価値の中核ではない。** 反対されたら真っ先に落とす（3.4）。

### 7.4 スコープ外だが同時に判断が要るもの

14. **既存ドリフト 3 件**（5.2(d)）: `mod.rs` L52 + `icon_textures.rs` ×4（消費者は `results_view.rs`）、`layout.rs` L7（`RowTheme` は `visual.rs`）。**直すか、明示的に残すかを PR 本文で表明する。黙って残さない。**
15. **`workspace/plan.md` の未チェック `- [ ]`**: `pre-bash.mjs` が `gh pr create` を未チェック項目で拒む（#749）。PR 作成前にチェックを閉じること。

---

## 8. 一枚まとめ

| 項目 | 結論 |
|---|---|
| 新規ファイル | **2**: `egui_shell/font_stack.rs`（16 項目）・`egui_shell/launcher_controller.rs`（27 項目） |
| 削除ファイル | **0** |
| 修正ファイル（`src-tauri` コード） | **9**: `view.rs`・`mod.rs`・`results_view.rs`・`window_coordinator.rs`・`results_window.rs`・`visual.rs`・`strings.rs`・`commands/launch.rs`・`commands/instant.rs`（+ 型リネームを採るなら `Cargo.toml`） |
| 修正ファイル（別 crate のコード doc） | **3**: `snotra-core/src/engine.rs`・`snotra-core/src/folder.rs`・`snotra-core/tests/search_frame_cost.rs`（**いずれも概念ラベル「driver」でしか届かない**） |
| 修正ファイル（`.md` / skill） | **4**: `src-tauri/CLAUDE.md`・`docs/architecture.md`・`PERFORMANCE.md`・`.claude/skills/state-check/SKILL.md`（**skill は事前合意が要る**） |
| 間接参照の母集団 | **122 行 / 31 ファイル**（4 パターンの和集合）。内訳 = 変更 89 / 検算済み据え置き 25 / 凍結 8（5.4） |
| 判別規則 | **規則 S**（射程 = 状態）: 窓への適用 memo とその適用・描画 = `MainView` / それ以外の状態と遷移 = `LauncherController`。**例外ゼロ**（`AppHandle` は状態ではなく能力ゆえ射程外） |
| `update()` | **1 本のまま**。20 ステップの背骨 + 名前付きメソッドへ委譲。入力は pre-widget / post-widget の **2 段**（事実 1） |
| 新規型 | **0**（`ToastAction` は移設。全域 `Effect` enum を作らない・事実 7） |
| 新規テスト | **0**（層の性質。フォント 7 件は名前ごと維持） |
| 唯一の機械ゲート | `governance:check` G1（`src-tauri/CLAUDE.md` の索引） |
| 唯一の実検出器 | `docs/build-commands.md` カテゴリ D の**目視**（人間が実行・PR へ記録） |
