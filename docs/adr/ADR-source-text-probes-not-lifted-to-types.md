# ADR-source-text-probes-not-lifted-to-types: ソーステキスト検査を型へ移さない——`Phase` で効いた手が及ばない場所

## 文脈

`include_str!` で自分のソースを読み、目的の綴りが本体に在ることを assert する「ソーステキスト検査」がこの repo に複数ある。テスト席を作れない型の呼び出し点を守る手段になるため増えつつあり、**書き方**の側は `ADR-source-text-probe-helper-locality` が決めた。

このサイクルで `src-tauri/src/startup.rs` の `Phase` を `macro_rules!` から導出し、そこに在ったソーステキスト検査を退役させた。宣言と `COUNT` を同じ引数列から作り、`ALL: [Phase; Self::COUNT]` の型づけで数のずれをコンパイルエラーにしたためである（**引数列を編集する限りで**あり、マクロ本体を書き換える経路は残る——どちらも変異注入で実測した）。

同じ手を残りのサイトへ当てられるかを調べ、当てられないと判断した。**ここに残すのはその理由である。**

**どのサイトも 2026-08-17 時点で露出は 0 である。** これは将来の追加に対する守りをどの層へ置くかの判断であって、欠陥の修正ではない。

**候補と検査は 1 対 1 ではない**——`launcher_controller.rs` には性格の違うソーステキスト検査が並んで在り、`view.rs` の骨格は 2 つの検査から呼ばれる。ゆえに以下はサイトの名前で書く。

## 決定

`Phase` で効いたのは「宣言と `COUNT` を同じ引数列から作る」という手であり、それが要求するのは**守りたい不変条件が 1 つの列挙から機械的に導けること**である。次のサイトはその形をしていないので、当面ソーステキスト検査のまま置く。

## 検討した代替案と却下理由

### 表示ゲートの呼び出し点を証人型で守る（`launcher_controller.rs`）

**証人を構築できる場所が、既にガードが立っている場所と同じである。**

`activation_entry_points_consult_the_display_gate` は、起動の入口（`activate_or_execute` / `shift_activate`）の本体に `plain_results_hidden(` と `results_area_collapsed(` の綴りが在ることを測る。証人型案は「ゲートを通った」ことの証人をフレームの冒頭で構築し、入口へ渡す形にする。

- **ゲートの入力のうち `view_kind` と `instant_rows` はフレームに凍結できない。** `layout::present_results` の doc が、それらは `on_enter` の前後で正当に変わるため凍結できず、読み点を動かさないことでしか守れないと書く。ゆえに構築できるのは入口の冒頭——**既にガードが立っている場所そのもの**である
- **消費点が 1 つに畳めない。** `shift_activate` の `tools >= 2` 枝は `SearchState::enter_tool` を直接呼び、`activate_or_execute` も `start_launch` も通らない。合流点 `start_launch` へ寄せる案は `ADR-activation-gate-placement` が既に閉じている
- **`EventLoopProof` の強さが移植できない。** あちらは別 crate（`snotra-egui-runtime`）の `pub(crate) fn new()` で crate 外から構築できないことと、`PhantomData<*const ()>` で持ち出しもできないことの 2 段で立つ。`snotra` は `[lib]` を持たない単一 bin crate なので前段が使えない
- **型が塞いだのは偽造だけである。** `FrameIndexing` / `FrameVisibleRows` はタプルフィールドが private で、読む関数（`read_indexing` / `read_visible_rows`）は `pub(super)` に閉じている。それでも `LauncherController::indexing()` という第 2 の口が在り、`run_search_with` がそこから読む。読み直しを塞いでいるのは同じファイルの別の検査 `activation_uses_frame_values_not_live_reads` である

**足りなさが構造的である理由**: 証人型が買うのは「その値の出所」であり、ここで守りたいのは「その値を見たか」である。前者が後者の代理になるのは構築点を消費点から離せるときだけで、離せない以上、証人型化は検査の全廃ではなく**縮小**になる。ソーステキスト検査の死角は少なくとも 2 種あり——**綴りが在ることは呼び出しが在ることを意味しない**（説明コメントへ書き残せば通る）／**呼び出しが在ることは委譲が在ることを意味しない**（`let _ =` で返り値を捨てられる。同じファイルの `on_enter_delegates_the_flush_decision_to_the_predicate` の doc が実測として持つ）——**証人型が塞ぐのは後者だけである**。

### アイコンキャッシュの無効化を CAS の成功枝へ融合する（`indexing.rs` / `state.rs`）

**守るべき 2 つ目の経路がまだ無い。**

`start_index_build_invalidates_the_icon_cache` は、`start_index_build` の本体に `invalidate_icon_cache(` の綴りが在ることを測る。融合案は、CAS の成功が無効化を伴うことを型で表す（ガード型を返す、あるいは無効化を `try_begin_index_build` の成功枝の内側へ入れる）。

- **「開始したのに無効化しない」経路が構築されていない。** `try_begin_index_build` を production で呼び出すのは `start_index_build` だけで、無効化はその数行下に隣接する。`start_index_build` の呼び出し元は複数あるが、いずれもこの同じ CAS を通る
- **融合は依存を増やす。** `IconCacheState` は Tauri の managed state で `app.try_state::<…>()` を要するため、`state.rs` から `icon.rs` と `AppHandle` への新しい依存が生じる
- **2 メソッド契約が射程へ入る。** `src-tauri/CLAUDE.md`「実装パターン」が `try_begin_index_build()` と `finish_index_build()` を唯一の正しい経路として固定しており、あいだにガード型を挟むとその契約と `state.rs` の既存テスト群を同時に動かすことになる
- **`#[must_use]` は代わりにならない。** `src-tauri/CLAUDE.md`「モジュール構成」が実測として、捕まえるのは「一度も見なかった」だけで `if ….is_some() { }` のような「見たが捨てた」形は通ると記録している。現状は `if !state.try_begin_index_build() { return false; }` というまさにその形である

**足りなさが構造的である理由**: 融合が買うのは「開始と無効化が離れること」への守りだが、離れる余地は呼び出し点が増えたときに初めて生まれる。同じ関数の隣接する数行に留まるあいだ、費用（新しい依存・2 メソッド契約の改訂）だけが先に立つ。**なお呼び出しの単純な削除は `-D warnings` の `dead_code` が先に捕まえる**が、これは条件つきの守りである——検査自身の doc が、呼び出し点が増えればその二重の守りは消えると書いている。**便益が立つ瞬間と、二重の守りが消える瞬間は同じである。**

### `view.rs` のフレーム内 1 回読みを消費トークンで守る

**正当な第 2 の読み口が既に在るので、トークンにバイパスが要る。そのバイパスこそ検査が禁じているものである。**

`assert_read_once_in_this_file` は `indexing_is_read_exactly_once_per_frame` と `visible_rows_is_read_exactly_once_per_frame` が共有する骨格で、`view.rs` 全体で当該の綴りがちょうど 1 回現れることを測る。トークン案は、読みが 1 回きりの消費トークンを返す形にして 2 回目をコンパイルエラーにする。

- **`Option::take()` は構造ではない。** 2 回目は `None` を返すだけで、panic か silent skip になる。コンパイルエラーではない
- **構築点を `view.rs` の外へ置く候補として挙がったのは `RuntimeFrame` である**（別 crate の汎用型で Snotra に依存しない）。`update` はそれを `&mut` で受けるので、by-value で取り出す形は `Option::take()` へ帰着する
- **`indexing` には正当な第 2・第 3 の読み口が実在する。** `show_egui_main` の `read_indexing(app)` はフレームの外であり、`run_search_with` の `self.indexing().get()` は用途の違う live-read で、その凍結を却下して意図的な live-read と決めたのは `ADR-activation-gate-placement` である。トークンで覆えないのでバイパスが要る
- **既に守られている部分が多い。** 構築の一意性は private なタプルフィールドが型で担い、「読みが消える」向きは rustc の束縛が支える（`assert_read_once_in_this_file` の doc が明言する）。消費側の読み直しは `activation_uses_frame_values_not_live_reads` が守る。**残る隙は「`view.rs` の中で本物をもう 1 回呼ぶ」形である**

**足りなさが構造的である理由**: トークンは「読める回数」を型で数える道具であり、正当な読みが型の外側に在るならバイパスを開けるしかない。開けたバイパスは「2 つ目の読み口」そのものであって、検査が禁じている当のものである。**`visible_rows` 側だけなら障害は小さい**——production の読みは `view.rs` の 1 か所で、フレーム外の読みも意図的な live-read も無い——が、買えるのは「読みを別のヘルパーへ移す」死角だけであり、それは検査の doc が既に受容残余として名指している。

## 却下理由として使わなかったもの

記録として残す。いずれも成立しない。

- **「マクロで倒せなかったから」**——どのサイトも列挙の問題ではなく、マクロは候補ですらない。理由は「1 つの列挙から機械的に導ける不変条件」という要件を満たさないことであって、手そのものの失敗ではない
- **「依存を増やせないから」**——アイコン無効化案の依存増は費用の 1 つとして数えたが、どの案も新しい外部依存を要さない。依存の禁止は決定の根拠に無い
- **「今の実装が壊れているから直せない」**——どのサイトも 2026-08-17 時点で露出は 0 である。守りの層を選ぶ判断であって、欠陥の扱いではない

## 受容した残余

**ソーステキスト検査のまま置くとは、その死角を引き受けることである。** 各サイトの死角はそれぞれの検査の doc が正本として持つ（`activation_entry_points_consult_the_display_gate` の「残る死角」・`start_index_build_invalidates_the_icon_cache` の同節・`assert_read_once_in_this_file` の「≧ 1 側」）——ここでは列挙しない。

`ADR-source-text-probe-helper-locality` が「元より統合の候補ではない」と名指したうちの `startup.rs` の切り出しは、この変更で消えた。

## 反転条件

- **表示ゲート**: `snotra` が `[lib]` を得た日、または `FrameIndexing` の読み口が 1 つへ閉じた日
- **アイコン無効化**: `try_begin_index_build` の production 呼び出し点が 2 つ以上になった日（`dead_code` の二重の守りが消えるのと同じ瞬間に便益が立つ）
- **1 回読み**: `indexing` の正当な第 2 の読み口のどちらかが消えた日、または `visible_rows` 側だけを先に倒す実利が出た日
- **横断**: 「守りたい不変条件が 1 つの列挙から導ける」形の新しいサイトが現れた日——そこは `Phase` と同じ手が当たる

## 隣接する決定との境界

- `ADR-activation-gate-placement` — あちらが決めたのは**ガードをどこへ置くか**（判定の層）、こちらが決めるのは**置いたガードの呼び出し点をどう守るか**（検知の機構）
- `ADR-no-test-only-injection-in-product-code` — 射程が違う。あちらは計測・検査のための**注入点**、こちらは**製品の型そのもの**を変える案である。「本来不要なコードがトラブルの原因を作り込む理由にはならない」という勘定は通底するが、決定は重ねない
- `ADR-source-text-probe-helper-locality` — あちらは**検査の書き方**（母集団の切り出しをどこへ置くか）、こちらは**検査を置くか型にするか**である

---

status: Accepted
関連: `ADR-activation-gate-placement` ・`ADR-no-test-only-injection-in-product-code` ・`ADR-source-text-probe-helper-locality`
