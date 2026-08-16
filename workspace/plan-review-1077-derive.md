# #1077 独立導出レビュー（コードと規範だけからの再導出）

対象: (目標 1) results 窓に描かれていない通常結果（plain）の行を Enter / Shift+Enter / クリックで起動させない。(目標 2) 同一フレームの中で Enter の判定と表示ゲートが同じ `indexing` を見る。

既存の `workspace/plan.md` / `workspace/research.md` は開いていない。

## 0. 観測方法の前提（この導出がどこまで硬いか）

- **LSP ツールがこのセッションに無い**——`ToolSearch("select:findReferences,outgoingCalls")` が「No matching deferred tools found」を返した。ゆえに呼び出し点の列挙は `git grep` に落ちている（`AGENTS.md`「条件別チェック（トリガー → 参照先）」の「LSP の無い環境でのみ grep へ落とす」）。**grep は文字列一致ゆえ同名の別物を拾い、re-export 経由を落としうる**——下の列挙は `pub(super)` の内部関数を記号名で引いた結果であり、**「これで尽きている」とは書かない**。実装時に LSP のある環境で `activate_or_execute` / `shift_activate` / `enter_tool` / `push_clicked` の findReferences を取り直すこと。
- 位置はシンボル名で書く。行番号は引用の便宜であって断定の根拠にしない。
- 症状の再現は行っていない（issue も「実際に食い違う窓が開くかは測っていない」と書く）。下の欠陥 1 は**コードの読みからの導出**である。

## 0.1 いま何が起きているか（2 つの欠陥は別物である）

**欠陥 A（目標 1・読みから導出）**: `LauncherController::on_enter` が起動を止めるのは flush 枝の中だけである。`should_flush_on_enter` が偽（＝行が最終クエリを反映済み）のフレームでは flush が走らず、`!self.state.results().is_empty()` だけを見て `activate_or_execute` / `shift_activate` へ進む。`indexing` が真でも `SPEC.md`「4.7 結果表示制御（2 窓構成）」の表示ゲートは**行を消さない**（データと選択は保持する）ので、**画面に何も出ていないのに Enter が起動する**並びが残っている。#1072 の commit message（`28a6342a`）は「flush 枝が空クリアへ落ちて起動しなくなる」と書くが、**その射程は unsettled な Enter だけである**。クリック経路（`view.rs` の `ClickTake::Current`）は `on_enter` を通らないので、flush による救いを最初から持たない。

**欠陥 B（目標 2・issue の主題）**: 同一フレーム内の `indexing` の読み点が複数ある（下の 2.3 の表）。

**両者は独立である。** B を直しても A は残り（読み点を揃えても、settled な Enter を止める判定がどこにも無い）、A を直しても B は残る（ゲートが自分で `indexing` を読めば読み点が 1 つ増える）。ゆえに**ゲートは「渡された `indexing`（もしくはそこから導いた `plain_hidden`）」を受け取る形にする**——これが 2 つの目標を 1 つの差分で満たす結び目である。

## 1. 変更が必要なファイルと対象シンボル

### 必須

| file:シンボル | 何をするか |
|---|---|
| `src-tauri/src/egui_shell/view.rs:EguiShellView::update` | フレーム 1 回の `indexing` 読み（status 行のために既に在る `indexing_raw`）を唯一の読み点にし、`plain_results_hidden` の第 3 引数と `on_enter` の引数へ配る |
| `src-tauri/src/egui_shell/launcher_controller.rs:LauncherController::on_enter` | 引数で `indexing` を受ける（内側で `self.indexing()` を読み直さない）。flush 枝の判定もその値を使う |
| `launcher_controller.rs:LauncherController::activate_or_execute` | 起動ゲート（Enter・クリック・Shift+Enter の tools ≤ 1 委譲が合流する点） |
| `launcher_controller.rs:LauncherController::shift_activate` | 起動ゲート（**`tools >= 2` 枝は `activate_or_execute` を通らない**——2 の候補 A の却下理由） |
| `launcher_controller.rs:LauncherController::indexing` | 呼び出し点が減る。関数自体は残す（`run_search_with` が使う） |
| `src-tauri/src/egui_shell/search_state.rs:plain_results_hidden` | 判定はこれを**再利用する**（コード変更は不要でも doc は変わる——3 参照） |

### 案により追加

| file:シンボル | 何をするか |
|---|---|
| `search_state.rs`（新規の純粋関数） | ゲートと「行が空でない」の**合成**を測れる単位へ出す（6 参照）。新設するなら `src-tauri/src/egui_shell/mod.rs` の `pub(crate) use` 行も |
| `src-tauri/src/egui_shell/results_view.rs:ResultsShared::take_clicked_for` | 触らない見込み。**ただし indexing の反転は `rows_generation` を進めない**ので、世代照合はこの欠陥を捕まえない（5 参照） |

### 触らないと決めたもの（根拠つき）

- `search_state.rs:SearchState::set_results` 系——**行はクリアしない**。`SPEC.md`「4.7 結果表示制御（2 窓構成）」が「データと選択は保持」を明記し、`docs/superpowers/specs/2026-07-24-su6-config-glue-design.md` 決定 3 が「クリア案」を 2 レンズの反証で却下している。
- `commands/launch.rs` / トレイ履歴メニュー経路（`launch_item_with_state` 等）——results の行ではないので母集団の外。
- `launcher_controller.rs:LauncherController::on_nav_keys`——**射程の判断が要る**（下の 2.4）。

## 2. ゲートを置く場所と理由

### 2.1 述語は表示ゲートと同じ関数を使う

防ぐべきは「**その時点で results 窓に描かれていない plain 行**を起動すること」であり、「描かれていない」の正本は `search_state::plain_results_hidden`（`layout::present_results` の連言③の材料）である。**別の式を書くと真実が 2 つになる**——#752 が連言②③を融合 bool から生の入力へ割ったのと同じ理由で、ここでも述語を共有する。共有すれば instant / folder / tool の carve-out（`SPEC.md`「4.7 結果表示制御（2 窓構成）」）は**構造で**保たれる: `plain_results_hidden` は `ViewKind::Tool` / `Folder` と `instant_rows` で偽を返す。

### 2.2 候補と採否

**案 A: `activate_or_execute` の入口 1 か所** — **却下**。
`shift_activate` の `tools >= 2` 枝は `activate_or_execute` を通らず `SearchState::enter_tool` を直接呼ぶ（grep 実測。`search_state.rs:SearchState::restored_rows_stale` の doc も「`enter_tool` の production 呼び出しは `shift_activate` の 1 つ」と記す）。隠れた行から tool メニューへ入ると view_kind が `Tool` になり、そこでは述語が偽——**Tool ビューでの Enter は起動する**。タスクが Shift+Enter を名指す以上、案 A 単独では目標 1 を満たさない。

**案 A′: `activate_or_execute` + `shift_activate`（`enter_tool` の直前）の 2 か所** — **推奨**。
`shift_activate` は既に `folder_load_pending` による同型の早期 return を持つ（#636 Finding A）。その隣に同じ形で置ける。ゲートが 2 か所になるが、**呼ぶ述語は 1 つ**である。

**案 B: フレームの入口 2 か所（view.rs の `on_enter` 呼び出しと `ClickTake::Current` の arm）** — 弱く却下。
`plain_hidden` が view.rs に既に在るので配線は短い。しかし **view.rs の `plain_hidden` は `on_enter` より後で算出される**（`drain_search` / `poll_search_debounce` / `on_enter` 自身が view_kind と `instant_rows_query` を書き換えるため、前へ動かすと表示ゲートが古い材料で判定する——2.3）。ゆえに Enter 用と表示用で述語を 2 回評価することになり、**issue が問題にした「別々の用途が別の時点で読む」形をもう一段作る**。加えて、新しい起動経路を将来足したときに素通りする。

**案 C: 起動可能な行を構造的に空にする（`results()` の代わりに `activatable_rows()` を通す）** — 却下。
`prefer-structural-over-documented-contract` の選好には合う。しかし `state.results()` の読み手は起動以外にも多い（snapshot publish・件数・nav・folder フィルタ）ので射程が広がり、#699 の publish 順序と `set_results` の契約に触れる。この issue の 2 目標より大きい変更である。

**案 D: indexing 中の Enter で行をクリアする（flush 枝の拡張）** — 却下。
`SPEC.md`「4.7 結果表示制御（2 窓構成）」の「データと選択は保持」に反する。#633 の設計で却下済みの案そのもの。

### 2.3 目標 2 の実装——**`indexing` だけを束ね、view_kind / instant の読み点は動かさない**

`view.rs:EguiShellView::update` には既にフレーム 1 回の `indexing_raw` がある（status 行の文言と行の有無で 2 度読むと index build スレッドの `finish_index_build()` が割り込む、という**同型の欠陥**を潰した跡である）。ここを唯一の読み点にする。

同一フレーム内の `indexing` 読み点（`AGENTS.md`「条件別チェック（トリガー → 参照先）」の「重複した読み…を束ねる/消す」に従い、1 つずつ書き出す）:

| 読み点 | 用途 | 処置 | 理由 |
|---|---|---|---|
| `view.rs` の `indexing_raw`（status 行） | 案内の文言・status 行の有無 | **残す（唯一の読み点にする）** | 既にフレーム冒頭に在り、`overlay_kind` と `status_row_present` へ同じローカルを配る形が確立している |
| `view.rs` の `plain_hidden` 算出の第 3 引数 | 表示ゲート（連言③） | `indexing_raw` へ置換 | 目標 2 の片側 |
| `on_enter` の flush 枝の `self.indexing()` | 同期 `engine.search` を撃つか | 引数の値へ置換 | Enter の判定であり目標 2 の射程内 |
| `run_search_with` の `self.indexing()` | plain 検索を撃つか・行をクリアするか | **残す（live-read のまま）** | `run_search` は `on_input_changed` など frame の外の経路からも呼ばれ、値がずれても**クリアする方向（安全側）**にしか倒れない。目標 2 の文言は「Enter の判定と表示ゲート」であり、ここは射程外。**ただしフレーム内に読みが 1 つ残ることは明記する**（`poll_search_debounce` 経由で同じフレームから呼ばれる） |

**view_kind と `instant_rows_query` は現在の読み点のまま**にする。`indexing_raw` の位置（status 行）と `plain_hidden` の算出位置の間で、`drain_search` / `poll_search_debounce`（→ `run_search_with` の Instant 枝が `instant_rows_query` を書く）/ `on_enter`（→ `enter_tool` が view_kind を Tool にする）がいずれもこれらを変えうる。**表示ゲートの材料を前へ寄せると、表示が 1 フレーム古い材料で決まる。** issue が問うているのは `indexing` の読み点であって、材料すべての時刻ではない。

**一貫性の主張は両方向で書ける**（これが目標 2 の受け入れ条件そのものである）:

- snapshot が真・実際が偽（構築が終わった直後）→ 行は隠れ、Enter も止まる。次のフレームで両方戻る。**安全側**
- snapshot が偽・実際が真（構築が始まった直後）→ 行は描かれ、Enter は起動する。**描かれたものを起動する＝整合**

どちらの向きでも「**描かれていないのに起動する**」は成立しない。

### 2.4 射程の判断が要る 1 点——`on_nav_keys` の `→` / `←`

`on_nav_keys` は Results ビューで `→`（選択行がフォルダなら展開）と `←`（選択行の親を展開）を受け、いずれも `state.results().get(selected)` を読む。**隠れている行に対しても成立する**（grep 実測。ガードは `is_error` / `is_folder` / instant 行だけで、`indexing` を見ない）。起動ではなく folder 突入なので、タスクの文言（「起動しない」）には入らない。**同じ族であることを明記した上で、この差分の射程に入れるかをユーザー判断に委ねる**のが妥当である。入れるなら `on_nav_keys` にも同じ述語のゲートが要る。

## 3. この変更で偽になる散文

grep した語（識別子だけでなく概念のラベルも）: `plain_results_hidden` / `表示ゲート` / `通常結果` / `indexing` / `起動` / `activate`。

| ファイル:位置 | どう偽になるか |
|---|---|
| `search_state.rs:FolderFrame::unsettled_at_entry` の doc | 「空クエリ・`indexing()` 中は `set_results(Vec::new())` で**起動そのものが止まる**——後者は flush の既定の扱いである」。起動を止める主体がゲートへ移る。`on_enter` 冒頭で早期 return する形を採ると **indexing 側は端的に偽**になる（flush 自体が走らない） |
| `search_state.rs:plain_results_hidden` の doc | 「§4.7 表示ゲート」と名乗り、`driver（view.rs）は Task 4 で表示分岐に組み込む` と書く。**表示だけでなく起動可否の正本**になり、呼び出し点も増える |
| `src-tauri/src/egui_shell/mod.rs` の `pub(crate) use search_state::{needs_index_refresh, plain_results_hidden}` の直前コメント | 「`plain_results_hidden` は …」と**消費者を名指している**。案 A′ では `launcher_controller.rs` が新しい消費者になるので、この帰属が古くなる |
| `layout.rs:present_results` の doc | 「読み点の非対称は呼び出し側の責務である」の段落と「`plain_results_hidden` を前後で 2 回読んでもならない——`indexing` は `AtomicBool` の live-read で…」。`indexing` の読みが view.rs の 1 か所へ移り、`plain_hidden` の材料の**一部だけ**が共有される形になるので、禁止の射程を書き直す必要がある（`AGENTS.md`「検証の作法」の全称表現の条項がここに直接当たる） |
| `view.rs` の `plain_hidden` 算出直前のコメント | 「連言③は**1 フレーム 1 回だけ**読む…ここで得た値を snapshot 用と `drive_results_window` の両方へ配る」。**配り先が増える**（Enter の判定へも配る）。`indexing` を 1 回だけ読む理由の記述も status 行側へ寄る |
| `launcher_controller.rs:on_enter` の flush 枝のコメント | 「**どちらの枝でも同期で行を差し替える**——空クエリ・indexing 中にクリアを落とすと、古い行が残ったまま直後の `activate_or_execute` がそれを起動する」。indexing 側の危険はゲートが引き受けるので、理由が空クエリ側だけになる |
| `launcher_controller.rs:run_search_with` の worker 死亡枝のコメント | 「`on_enter` の flush が『どちらの枝でもクリアする』理由と同じ危険である」。上の行を直すなら参照先として追随を確認する |
| `launcher_controller.rs:activate_or_execute` の doc | 「Enter/クリックの単一 dispatch」。ゲートが増えるので責務の記述が変わる |
| `launcher_controller.rs:shift_activate` の doc | 「folder ロード未確定窓は activate と同じ理由で入場もしない」。**同じ形の理由がもう 1 つ増える** |
| `docs/architecture.md`「検索フロー（入力 → 結果表示）」 | シーケンス図の `Enter の起動（activate_or_execute）→ 可視性判定` の並び（Enter が可視性の材料を先に見るようになる）と、補足の Enter の項の「1 回あたりの費用は変わらない——`on_enter` は判定より前に `instant_prefix` が `engine.lock()` を取る」。**冒頭 return を採ると、ブロックされる Enter はこの錠待ちを払わなくなる**（文が偽になるのではなく、根拠に使われている事実が枝で分かれる。同じ申し送りが `launcher_controller.rs:instant_prefix` の doc にも書かれている） |
| `docs/architecture.md`「ウィンドウ管理」 | 「結果の表示/非表示は `search_state.rs` の純粋核（… + indexing 表示ゲート）で制御」——述語の役目が「表示/非表示」を越える |
| `SPEC.md`「4.7 結果表示制御（2 窓構成）」 | 4 の判定次第で**追記**が要る（偽になるのではなく、不足する） |
| **PR 本文** | `AGENTS.md`「条件別チェック（トリガー → 参照先）」の「文書に事実の写しを増やす変更」——**数え上げの母集団に PR 本文を含める**（squash で main の commit message になる）。#1072 の commit message は「表示ゲートで画面に出ていない行が Enter で起動していたが…起動しなくなる」と書いており、**settled な場合を覆っていない**。過去の commit message は直せないので、今回の PR 本文で射程を明示する |

**触らないと判断したもの**: `docs/adr/ADR-results-presentation-two-stage.md`（凍結された歴史。`.claude/rules/governance-docs.md`「ガバナンス文書の参照と命名のルール」）。ただし同 ADR の帰結が指す `present_results` の doc は生きた層なので、そちらで整合を取る。`src-tauri/CLAUDE.md`「モジュール構成」は `layout.rs` / `search_state.rs` の索引行に「表示ゲート」の語を持たない（grep 実測）ため、この差分では影響を受けない見込み。

## 4. `SPEC.md` の更新要否

`AGENTS.md`「開発ワークフロー」の 2 参照で判定する。

- **当該挙動の記述が SPEC にあるか → 無い。** §4.7 は表示だけを規定し、§4.8 は「シングルクリック: アイテムを起動する」で**描かれている行**を前提にしている。§8.6 の連言表は results 窓の可視性であって起動可否ではない。「表示ゲートで隠れている行を Enter が起動するか」を決める文は、`Enter` / `indexing` / `インデックス構築中` / `起動` の grep では見つからなかった（**「無い」の全称は grep 4 語ぶんの範囲での結論である**）。
- ゆえに既存記述に**合わせる**のではなく**新しい不変条件を足す**変更であり、`AGENTS.md` の判定に照らすと**仕様変更＝ SPEC を更新する**。
- 置き場所は §4.7 の末尾（表示ゲートの帰結として「隠している間はその行が Enter / Shift+Enter / クリックの起動対象にもならない」を 1 行）。§8.6 の連言表は可視性の従属軸なので不適。**1 行で足りる**（`.claude/rules/governance-docs.md` の「必要なことだけ」）。

**対立する先例があるので明記する**: #1072（`28a6342a`）は同じ `indexing` の組み合わせの挙動を変えたが、`--stat` の実測では `docs/architecture.md` を触って **`SPEC.md` は触っていない**。これを先例と読めば「SPEC 不要」も成り立つ。私の判断が「更新する」に傾く理由は、#1072 の挙動変化が flush の**副作用**だったのに対し、今回は**不変条件そのものを新設する**点である。

## 5. 不変条件・異常系

### 壊してはならないもの

1. **instant コマンドは構築中も使える**（`SPEC.md`「19.7 状態モデル」——「インデックス構築中でもインスタントコマンドは使用可能」「indexing ガードはプレフィックス判定より**後**に置く」）。`plain_results_hidden` の `!instant_rows` が担う——**同じ述語を使う限り構造で保たれる**。別の式を書けば壊れる
2. **folder 展開・tool 選択は構築中も表示・操作できる**（§4.7 carve-out）。同上
3. **行データと選択は構築中もクリアしない**（§4.7 / #633 決定 3）。ゲートは**起動を止めるだけで行を触らない**
4. **初回起動フローを塞がない**——`commands/window.rs` の first-run 経路は `indexing == true` のまま長時間走る。`/state-check` のバグパターン 4（ガードと初回フローの衝突）に正面から当たるので、計画時に明示的に検算する。**導出上は塞がない**（ゲートが止めるのは「そもそも描かれていない plain 行」だけで、初回に使える経路＝instant・設定サイドカーは述語が偽）
5. **`results 可視 ⇒ main 可視` を含む §8.6 の 4 連言**（`present_results` の入力に触るので回帰させない）
6. **起動ロジックは main の 1 か所という設計**（`docs/architecture.md`「検索フロー（入力 → 結果表示）」）——ゲートを view.rs 側へ散らすと薄れる（案 B の短所）
7. **`#[must_use]` の規約**（`src-tauri/CLAUDE.md`「モジュール構成」の「処置を返す純粋核の強制」）——`&mut self` で状態を進めてから `bool` / `Option` を返すメソッドを新設するならメソッド段の `#[must_use]` が要る。`&self` メソッドと自由関数は同 doc の「対象外が 2 種ある」の (2) に当たる

### 異常系と検知手段

- **ゲートが carve-out を誤って塞ぐ**（構築中に instant が起動できない）→ 述語のユニットテスト `plain_results_hidden_only_for_plain_results_view` が既に固定している。**ただし合成（ゲートが実際に配線されていること）は測らない**（6 参照）
- **ゲートが黙って落ちる**（呼び出しを消す変異）→ **検知器は無い。** 最小の診断手段は、ゲートで落とした起動要求に観測点（`trace_main`）を置くこと。先例は `view.rs` の `ClickTake::Stale` の trace で、そこには「**診断用であって不変条件の担保ではない**」という断り書きが既にある。同じ断り書きを添える
- **境界の並び**: クリックは「可視のフレームで積まれ、`indexing` が立った次のフレームで消費される」形が実在する（`ResultsShared::push_clicked` は `rows_generation` を刻むが、**`indexing` の反転は世代を進めない**）。ゲートはこの 1 クリックを落とす。**落とすのが正しい**（ユーザーが見た画面はもう無い）が、**世代照合はこの欠陥を捕まえない**ことを明記する
- **回復不能な状態にならないこと**: ゲートは早期 return であり、`start_launch` の single-flight フラグにも `search_debounce` にも触れない。構築完了後は次のフレームで通常どおり起動できる

## 6. テスト方針（測れない部分を含む）

コマンドは `docs/build-commands.md`「変更後の検証チェックリスト（必須・スキップ不可）」カテゴリ A。**`cargo test -p snotra --lib` は使えない**（`src-tauri` は `[lib]` を持たない・`src-tauri/CLAUDE.md`）。

### 測れるもの

- **述語の carve-out**: 既存テストがカバー（Results×plain×indexing だけ真、instant / Folder / Tool / 非 indexing は偽）
- **合成**: #1072 が自分の commit message で書いた理由をそのまま適用する——「合成を名前のある純粋関数へ出したのは、受け入れ条件を測れる単位を作るためである」。**ゲートと「行が空でない」の合成を純粋関数へ出す**（例: Enter の dispatch 判断を `enum` で返す純粋関数）。そうすれば「ゲートの連言を外す」変異でテストが落ちる
- **変異注入で確かめる**: ゲートの連言を外して `cargo test -p snotra` が落ちること。**落ちないなら、そのテストはゲートを測っていない**（`memory: mutation-blocked-by-earlier-path` の型——対照の差が証拠であって、緑であること自体は証拠ではない）

### 測れないもの（正直に書く）

- **`LauncherController` にテストモジュールが無い**（`git grep -l "mod tests" -- src-tauri/src/egui_shell/` の 13 件に `launcher_controller.rs` は入らない。入らないのは他に `mod.rs` と `results_window.rs`）。`AppHandle` を要求するため、`activate_or_execute` / `shift_activate` / `on_enter` の**呼び出し点まで含めた**検証は `cargo test` の射程外である
- **view.rs の kittest ハーネスが駆動できるのは `search_input_ui` だけ**（同関数の doc が「`RuntimeFrame` にも `LauncherController` にも触らない」と明記）。`update()` の並びは駆動できないので、「view.rs が同じ値を配っていること」はテストで観測できない
- **目標 2 は構造でしか示せない**——`indexing` は `AppState` の `AtomicBool` で、フレーム内の読み点が 1 つであることを外から測る手段が無い。**ゲートが「渡された bool」を受け取る形にすれば、読みを内側でやり直さないことが signature から読める**——これが測定の代わりに置ける唯一のもの
- **smoke（`scripts/smoke-egui.ps1`）での実測は可否が未測定**である。「indexing 区間に `egui_launch` が現れない」という H1 型の否定形不変条件（`scripts/lib/SnotraTraceInvariants.psm1` の先例）は書けるが、**index build の窓を決定的に作れるかを測っていない**（区間長は索引の規模に依存する）。計画には「調査 → 不可なら受容する残余」と書くこと。**やる前に「発火しうるか」を測る**（`memory: measure-whether-detector-can-fire`）

## 7. 当たるトリガー（`AGENTS.md`「条件別チェック（トリガー → 参照先）」）

| トリガー | 参照先 | この差分での当たり方 |
|---|---|---|
| UI モード・ガード条件を追加/変更 | `/state-check` | ゲートの新設そのもの。**初回起動フロー（first-run の `indexing == true`）との相互作用を明示的に検算する** |
| 重複した読み・冗長に見える状態を束ねる/消す | 各箇所を 1 行ずつ書き出してから着手 | `indexing` の 4 読み点（2.3 の表がその書き出しである） |
| フレーム内 live-read・スレッドをまたぐ共有状態を変更 | `/race-check` | `AtomicBool` の live-read をフレーム snapshot へ替える |
| 分岐を決める値（フラグ）の出所を変更 | 下流を 1 段辿り「この値で初めて走る行」を列挙 | live-read → 引数へ替えることで**新しく生きる組み合わせ**（settled × indexing × Enter、settled × indexing × click、tools ≥ 2 × indexing × Shift+Enter）を数える |
| 対称ペア（show/hide） | `/symmetric-check` | 「見せない」と「起動させない」の対。**片方だけが変わる将来を 1 つ挙げられるか**を問う（挙がらないなら同概念＝述語を共有する根拠になる） |
| 関数・型を新規定義 | 呼び出し元の列挙 + `/dry-check` | 純粋関数を足す案を採る場合。**LSP が無ければ grep へ落ちることを明記する**（0 節） |
| ガバナンス文書（`*.md`）・`.rs` の見出し参照を変更 | `npm run governance:check`（カテゴリ F） | SPEC / architecture / doc コメントを触る |
| doc コメント（`///` / `//!`）を追加・変更 | `.claude/rules/comments.md` の `cargo doc` | **PostToolUse hook は intra-doc link 切れに沈黙する**——手で走らせる |
| ホットキー・表示経路 | `docs/build-commands.md` カテゴリ C / D | 起動経路の挙動を変えるので、smoke と目視の要否を計画時に判断する |

## 8. 判断が割れうる論点（実装前に決めること）

1. **ゲートの置き場所**——案 A′（`activate_or_execute` + `shift_activate`）か案 B（view.rs の 2 入口）か。**`enter_tool` を覆えるか**が分岐点である
2. **`on_enter` の冒頭で早期 return するか**——するとブロックされる Enter は flush も `instant_prefix` の `engine.lock()` も払わない。しないと **unsettled な Enter は今までどおり行をクリアし、settled な Enter は行を保つ**という非対称が残る。`SPEC.md`「4.7 結果表示制御（2 窓構成）」の「データと選択は保持」に照らせば**保つ側が素直**だが、既存の flush 枝と食い違う
3. **`SPEC.md` を更新するか**——4 の判定は「する」。ただし #1072 が SPEC を触らずに同種の挙動を変えた先例がある
4. **`run_search_with` の `self.indexing()` を live のまま残すか**——残す推奨。安全側にしか倒れないが、**フレーム内に 3 つ目の読みが残る**
5. **`on_nav_keys` の `→` / `←`（隠れた行から folder へ突入する）を射程に入れるか**——起動ではないので文言上は外だが同じ族である
6. **tool メニューへの「入場」を起動と見なすか**——**見なす推奨**（対象の行をユーザーは見ていない）。見なさないなら案 A′ の 2 つ目のゲートは不要になり、代わりに「見ていない行から tool メニューへ入れる」ことを受容する残余として記録する
