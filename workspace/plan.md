# 実装計画: #1077 — 起動ガードを表示ゲートへ合流させ、`indexing` の読みをフレーム内で 1 つにする

前提となる調査は `workspace/research.md`。以下は判断の結論だけを繰り返す。

- issue が名指す「同一フレーム内の 2 読み点の食い違い」**そのもの**の症状は、両方向とも軽微だった（事実 4）
- 一方で調査は**より重い欠陥**を見つけた: **Enter の起動ガードが表示ゲートを一切参照しない**ため、
  `is_unsettled` が偽（打鍵が落ち着いた状態）のとき、`indexing` で results 窓が消えたあとも
  `state.results()` に残った行を Enter が起動する。**同一フレーム内の競合を要さない**（事実 3）
- #1072 が塞いだのは同じ族の unsettled 側の切片だけである

## 目的

1. **画面に出ていない plain 行が Enter / Shift+Enter / クリックで起動しない**ようにする（事実 3 を閉じる）
2. **`on_enter` と表示ゲートが同じ `indexing` の値を見る**ようにし、issue が記録した食い違いを構造で消す

## 受け入れ条件

- **AC1**: `plain_results_hidden` が真の状態（Results ∧ plain 行 ∧ `indexing`）で、`state.results()` が
  非空・settled であっても、Enter は起動しない（`start_launch` へ到達しない）
- **AC2**: 同じ条件で Shift+Enter がツール選択へ入らない（`activate` と同じガードが `shift_activate` にも在る）
- **AC3**: 同じ条件でクリック逆流（`view.rs` の `take_clicked_for` → `activate_or_execute`）も起動しない
- **AC4**: `on_enter` の flush 枝と `view.rs` の表示ゲートが、**同一フレームで同じ `indexing` の値**を使う
- **AC5**: 上記のガード（呼び出し点）が脱落したら検査が落ちる。**変異注入で実測する**
- **AC6**: instant コマンド行・folder 行・tool ビューの起動は**一切変わらない**
  （`plain_results_hidden` が `Results ∧ !instant_rows` を条件に持つため）
- **AC7**: `/r` 履歴行（`QueryIntent::Command`）は**ガードの射程に入る**。`run_search_with:760` が
  `instant_rows_query = None` を置き、`Instant` 枝だけが立て直すため、Command 行の
  `plain_results_hidden` は `indexing` と同値になる。**表示ゲートと同じ射程であり
  （構築中は現に隠れている）、起動側をそれへ合わせるのが本変更の目的そのものである**
- **AC8**: → / ← のフォルダ突入は**ガードしない**（現行のまま）。`on_nav_keys`（`:1182`・`:1205`）は
  `folder_load_pending` も見ておらず、**不可逆な起動だけを止める**という
  `folder_load_pending` の方針と揃う。突入すれば `ViewKind::Folder` になり行は可視へ戻るため
  自己回復する
- **AC9**: issue #1077 が「未検証」として挙げた 2 点——**食い違う窓が実際に開くか**・**症状は何か**——に
  対する答えを PR 本文へ記録する。#1077 は `検討:` issue であり、**答えを残すこと自体が成果物である**。
  記録する内容: (1) 読み点の時刻の食い違いそのものの症状は両方向とも軽微であること、
  (2) より重い欠陥は「起動ガードが表示ゲートを参照していないこと」で**競合を要さない**こと、
  (3) **フェーズ 0 の実機再現の結果**（観測した挙動をそのまま書く。「起動した」も「しなかった」も成果である）

## 変更ファイルと対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/launcher_controller.rs` | `activate_or_execute` | 冒頭に `plain_results_hidden` の early return を追加 |
| 同上 | `shift_activate` | 同上（`tools >= 2` 枝は `activate_or_execute` を通らず `SearchState::enter_tool` を直接呼ぶため個別に要る） |
| 同上 | `activate_or_execute` / `on_enter` / `activate` / `shift_activate` | `FrameIndexing` を引数で受け取る（フェーズ 2。素の `bool` にしない理由は「不変条件と異常系」） |
| 同上 | 新規 `#[cfg(test)] mod` | 呼び出し点の検知器（`include_str!("launcher_controller.rs")` の自己参照。**`search_state.rs` ではなくこのファイルに置く**——「テスト席が無い」のは controller を**構築**できないことであって、ソーステキスト検査は `AppHandle` を要さない） |
| `src-tauri/src/egui_shell/view.rs` | `update()` | `indexing_raw`（現 `:920`）を `on_enter` とクリック逆流の `activate_or_execute` へ渡し、表示ゲート（現 `:1100`）の読み直しを止める |
| 同上 | `plain_hidden` の導出 | `self.controller.indexing()` を `indexing_raw` へ差し替え |
| `src-tauri/src/egui_shell/mod.rs` | `plain_results_hidden` の re-export コメント（`:80-82`） | 消費者に `launcher_controller.rs` を加える |
| `src-tauri/src/egui_shell/search_state.rs` | `plain_results_hidden` の doc（`:716-721`） | 「driver（view.rs）は……表示分岐に組み込む」が消費者を 1 つしか挙げていない。起動ガードも同じ述語を使うことを書く |
| 同上 | `mod tests` | 検知器（下記）とガードの述語テスト |
| `src-tauri/src/egui_shell/layout.rs` | `present_results` の doc（`:390-391`） | 「`plain_results_hidden` を前後で 2 回読んでもならない」が**規範のまま**か、フェーズ 2 で構造が担うようになったかを書き分ける |
| `SPEC.md` | §4.7 末尾 | 隠れている通常結果からは Enter / Shift+Enter / クリックによる起動もツール選択への遷移も成立しないことを 1 行（判定の経緯は下記） |
| `docs/architecture.md` | `:81` | 「結果の表示/非表示は……indexing 表示ゲートで制御」が消費者を表示だけに限っている。起動側も同じ述語を使うことを含める |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `instant_rows_query` の doc（`:196`） | 「表示ゲートの連言②」が消費者を 1 つしか挙げていない |

**変更しないと確かめた文書**（`/plan-review` Step 1 の項目 7 で概念ラベル grep）:

- `docs/adr/ADR-results-presentation-two-stage.md` — 決定は「pre-click で `plain_results_hidden` を
  1 回だけ評価し、post-click では件数だけ再評価する」。フェーズ 2 が動かすのは `indexing` の**読み時刻**だけで、
  述語の評価点は pre-click のまま・件数は post-click のままゆえ、この決定は成立し続ける
- `docs/superpowers/plans/` `docs/superpowers/specs/` — 過去サイクルの計画・設計の記録であり現在形の正本ではない

**新しい述語は作らない。** `plain_results_hidden` をそのまま起動ガードでも使う——
「表示ゲートと起動ガードが同じ述語である」ことが受け入れ条件そのものだからで、
別名の述語を作ると 2 つが食い違う将来を再び作る。

## より単純な代替案と、それを採らない理由

**「`indexing` が立ったら行をクリアする」——採らない。上流で既に却下されている。**
行が空なら起動ガードは要らず、これが最も単純な解に見える。しかし
`docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md:36` が
「#633 は表示ゲート + `index_generation` 世代カウンタ（**『クリア』案はレビューで却下**——
SolidJS 非 parity・instant carve-out 破壊・bool エッジのパルス見逃し）」と記録しており、
`SPEC.md` §4.7 と `view.rs` の該当コメントも「データと選択は保持——クリアしない」を明記する。
**この計画はその決定を維持したまま、起動側だけを表示側へ合わせる。**

## 実装順序

### フェーズ 0 — 修正前の挙動を実機で 1 回測る（コードを 1 行も変えない）

**修正が入ると測れなくなる**ため、必ずフェーズ 1 より前に行う。ユーザーの同意済み（2026-08-16）。

- [ ] `%APPDATA%\Snotra\config.toml` をバックアップする（復元できることを先に確かめる）
- [ ] `SNOTRA_TRACE` を有効にしてアプリを起動し、検索して plain 行を出す（settled にする＝打鍵後に少し待つ）
- [ ] 別プロセスから `config.toml` の `[[paths.scan]]` を書き換えて `IndexInputs` 差分を立てる
      （`auto_hide_on_focus_lost = false` は実値なので main は可視のまま残る）
- [ ] results 窓が消えることを目視し、その状態で Enter を押して**起動するか**を見る
- [ ] 観測結果（起動した / しなかった・trace の `egui_launch` の有無）を本ファイルへ記録する
- [ ] `config.toml` を復元し、復元できたことを内容の照合で確かめる
- [ ] **再現しなかったら実装へ進まない**——コード上の裏取り 2 系統と矛盾する。
      行が消えている／ガードが既に効いている経路を先に特定する

### フェーズ 1 — 起動ガード（事実 3 を閉じる）

- [ ] `activate_or_execute` 冒頭に `plain_results_hidden(self.state.view_kind(), self.instant_rows_query.is_some(), self.indexing())` の early return を足す
- [ ] `shift_activate` の `folder_load_pending` チェックの隣に同じガードを足す
- [ ] 両方の doc に**理由**を書く（`folder_load_pending` の doc と同じ形式——「行は残っているが画面に出ていない」「不可逆な起動だけを止める」）
- [ ] **`on_enter` の flush 枝は 1 行も変えない。** 冒頭での早期 return も採らない——unsettled な Enter が行を
      クリアするのは #1072 の意図した処置であり、settled な Enter が行を保つのは §4.7「データと選択は保持」と
      整合する。ガードは**起動だけ**を止める
- [ ] 呼び出し元の列挙を **LSP の findReferences** で取り直す（`activate_or_execute` / `shift_activate` /
      `enter_tool` / クリック逆流）——独立導出レビューは LSP を持たない環境で走り `git grep` に落ちていた
- [ ] `search_state.rs` の `mod tests` に、`plain_results_hidden` が真になる 1 組と偽になる各組を
      **起動ガードの視点で**固定するテストを足す（既存 `plain_results_hidden_only_for_plain_results_view` は表示側の名前ゆえ別に置く）
- [ ] **呼び出し点の検知器**を `launcher_controller.rs` の新規 `#[cfg(test)] mod` へ足す
      （`indexing.rs:201` `start_index_build_invalidates_the_icon_cache` の形・自己参照 `include_str!`）:
      `activate` / `shift_activate` の本体を切り出し（切り出しの錨は `fn activate(` と `fn shift_activate(`。
      **`fn activate_or_execute(` とは衝突しない**——`(` が直後に続く形で切るため）、
      **2 つとも母集団が空でないことを先に assert してから** `plain_results_hidden(` の実在を assert する
- [ ] **変異注入**: ガード 1 行を削って `cargo test -p snotra` が exit 101 になることを実測し、結果を本ファイルへ記録する（AC5）

### フェーズ 2 — フレーム内で 1 つの `indexing`（AC4）

- [ ] `FrameIndexing(bool)` newtype を `search_state.rs` か `layout.rs` へ足す（`on_enter` の隣接 `bool` 取り違え対策・「不変条件と異常系」参照）
- [ ] `on_enter` / `activate_or_execute` / `activate` / `shift_activate` の署名へ `FrameIndexing` を足し、内部の `self.indexing()` をその値へ置き換える
- [ ] `view.rs` の `update()` で `indexing_raw`（`:920` の既存の読み）を `on_enter` とクリック逆流の `activate_or_execute` へ渡す
- [ ] 表示ゲートの `self.controller.indexing()`（`:1100`）を `indexing_raw` へ差し替える
- [ ] `view.rs:914-919`（#752 F2 のコメント）を更新し、**1 回読みの射程が status 行から表示ゲートと起動ガードまで広がった**ことを書く。射程を数えず「この `indexing_raw` を配る先」を指す形にする
- [ ] `run_search_with` の読み（`launcher_controller.rs:778`）は**触らない**。理由（順序不変条件・打鍵時点で判断する）を当該箇所か `view.rs` のコメントへ 1 行残す

### フェーズ 3 — 文書と検証

- [ ] `SPEC.md` §4.7 末尾へ 1 行足す（内容と根拠は下記「`SPEC.md`・関連文書の更新要否」）
- [ ] `mod.rs` の `plain_results_hidden` re-export コメントの消費者を直す
- [ ] `docs/build-commands.md` カテゴリ A 全件（fmt / check / clippy / test / **`cargo doc`**）を、編集のたびに実行する
- [ ] `npm run governance:check`（カテゴリ F）を実行する
- [ ] **`/race-check` をここで実行する**——同スキルは「**計画段階では起動しない**」（#784）と定めており、
      母集団は `npm run race:boundaries` が差分から決める。計画レビューでは起動していない
- [ ] **`/dry-check` を実行する**（`AGENTS.md`「関数・型を新規定義／改名／導入」——フェーズ 2 で
      `FrameIndexing` を新規定義するため。呼び出し元の列挙は LSP の findReferences で行う）
- [ ] `/state-check` / `/symmetric-check` を**実装差分にも再実行する**（`AGENTS.md`「レビュー指摘へ
      修正（fix-forward）を当てた」——修正は指摘箇所へ注意が集中し周辺に新しい誤りを生む）

## 不変条件と異常系

- **凍結してよいのは `indexing` だけである。** `view_kind` / `instant_rows` / `result_count` は
  `on_enter` の前後で正当に変わる（#752 F2 の読み点の非対称）。これらを引数で凍結してはならない
- **`consume_external_pending`（`view.rs:622`）より前へ読みを寄せない。** 完了フレームがフリッカーする
  （`launcher_controller.rs:1023-1026`）。`indexing_raw` の位置（`:920`）はそれより後なので安全
- **`run_search_with:778` の読みは統合しない。** 用途が違い（クリアするか）、到達経路ごとに
  その時点で判断するのが正しい
- **`plain_results_hidden` の引数の順序を取り違えても型が通る**（`bool, bool`）。呼び出し点を増やすので、
  テストで各引数の効きを個別に固定する
- **フェーズ 2 は同型ペアの取り違えを 1 つ作る**（`/symmetric-check` Step 2c）。
  `on_enter(shift_held: bool, ctx)` へ `indexing: bool` を足すと**隣り合う 2 つの `bool`** になり、
  呼び出し点 `self.controller.on_enter(post.shift, indexing_raw, &ctx)` で入れ替えても
  **コンパイルが通る**。`on_enter` にテスト席が無いため、**取り違えを区別できる観測が無い**。
  ゆえにフェーズ 2 を実施するなら `#[derive(Clone, Copy)] struct FrameIndexing(bool)` の
  newtype で包む（起点が別の式ゆえ、ここでは newtype が実際に閉じる——Step 2c が警告する
  「起点が同型」の形には当たらない）。**包まないなら フェーズ 2 は実施しない**
- **起動ガードの置き場所は `activate_or_execute` / `shift_activate` である。** 候補を 3 つ検討した:
  - **`start_launch`（`:259`）** — 3 経路の合流点（`activate:247` / `execute_instant_selected:515` /
    `execute_tool_selected:598`）。**採らない**（`/symmetric-check` Step 3）——ガードの意味は
    「選んだ行が画面の行ではない」であって行の**選択**の性質であり、`start_launch` に届く時点では
    `LaunchWork`（path/query/tools）へ解決済みで行は消えている
  - **`activate`** — `folder_load_pending` の隣。当初これを採った。**却下**——`activate` は plain 枝
    だけで、行番号を解決する層の**合流点ではない**
  - **`activate_or_execute`（採用）** — Enter・クリック逆流・`shift_activate` の `tools <= 1` 委譲が
    合流する唯一の入口で、行の index を受け取る層そのもの。`plain_results_hidden` は tool ビューでも
    instant 行でも**構造的に偽**（`Results ∧ !instant_rows` を条件に持つ）ため、そこへ置いても
    それらの経路を阻害しない。将来の行種別も自動で覆う。**独立導出レビューも独立に同じ場所を推した**
  - **`shift_activate` は どの案でも別に要る**——`tools >= 2` 枝は `activate_or_execute` を通らず
    `SearchState::enter_tool` を直接呼ぶ（`search_state.rs:SearchState::restored_rows_stale` の doc も同じ事実を記す）
- **合成を純粋関数へ切り出すことはしない。** #1072 は `is_unsettled` で合成を測れる単位にしたが、
  **今回は合成が無い**——ガードは `plain_results_hidden` 単体で、「行が空でない」の判定は別の層
  （`on_enter:1355` と `activate` の `results().get(index)`）にある。無い合成に名前を付けると
  真実が 2 つになる。呼び出し点の脱落はソーステキスト検知器が担う
- **異常系**: `indexing` が真の間に Enter を押しても、行のクリアは行わない（現行の flush 枝が担う）。
  ガードは**起動を止めるだけ**である——`folder_load_pending` と同じ「前フレーム結果の保持は
  意図的設計ゆえ温存し、不可逆な起動だけを止める」方針に揃える
- **「どの分岐が選ばれるかを決める値の出所」を変える変更である**（`AGENTS.md` の該当トリガー）。
  フェーズ 2 は `indexing` の出所を live-read から引数へ移す。**diff に現れない下流を 1 段辿り
  「この値で初めて走る行」を列挙する**——`activate` / `shift_activate` は 1 行も変えていないのに
  ガード追加後は「plain 行 ∧ indexing」で初めて early return を通る。ゆえに検知器は
  **その組み合わせを実際に走らせる**こと（述語テストだけでは呼び出し点を測らない）
- **フェーズ 1 と 2 は達成する不変条件が違う。** フェーズ 1 だけだと、起動ガードは表示ゲートより
  **後の時刻**の値を読む——「前フレームで見えていた行に Enter」は保守側（飲まれる）へ倒れるので
  安全だが、**同一フレームでの一致は保証されない**。フェーズ 2 は両者へ同じ値を配ることで
  AC4 を**構造で**満たす。ゆえにフェーズ 2 は装飾ではない
- **残余（受容）1**: フェーズ 2 の後、表示ゲートは最大 1 フレーム古い値で判定する
  （`view.rs:920` で凍結するため）。`on_enter` の同期 `engine.search` は engine lock を
  40〜95 ms 握る（#1032 実測）ので、その間に `indexing` が立つ余地は実在する。
  帰結は「results 窓が隠れるのが 1 フレーム遅れる」だけで、**起動と表示は同じ値を見るまま**である
- **残余（受容）2**: フェーズ 2 の後も、`view.rs:920` より後・`on_enter` より前に走る
  `poll_search_debounce` → `run_search_with:778` は live-read のままである。凍結値と食い違うと
  「Enter が 1 フレーム飲まれる」または「行が空で何も起きない」のいずれかになる。**どちらも軽微**で、
  次フレームの再検索が回復する。残余 1・2 とも `view.rs` のコメントへ明記する

## テスト方針と検証コマンド

- **純粋述語**: `search_state.rs` の `mod tests`（既存の `plain_results_hidden_only_for_plain_results_view` の隣）
- **呼び出し点**: 上記のソーステキスト検知器。**死角は `indexing.rs` の先例と同じ**——母集団は
  当該関数のソーステキストだけで、呼び出しグラフは辿らない。その死角を doc に書く
- **`on_enter` にテスト席は無い**（`launcher_controller.rs` に `mod tests` が無く `AppHandle` と
  engine lock を要求する。#1072 実測）。ゆえに AC1〜AC3 は「述語テスト + 検知器 + 変異注入」で担保する
- 検証コマンドの正本は `docs/build-commands.md`。カテゴリ A 全件 + F。カテゴリ C（smoke）の要否は下記の未確定

## `SPEC.md`・関連文書の更新要否

**`SPEC.md` §4.7 の末尾へ 1 行足す。§8.6 は変更しない。**

判定は 2 度動いた。経緯ごと残す（結論だけでは、次に触る者が同じ往復をする）。

1. 当初「更新しない」——先例 `folder_load_pending`（同型のガード）が SPEC に記述を持たないため
2. `/state-check` Step 5 で「§8.6 の遷移ルール要約へ」——§8.6 の図が
   `NormalMode --> ToolSelectionMode: Shift+Enter [tools >= 2]` を持ち、AC2 が**この文書化された遷移に
   ガードを足す**ため（`AGENTS.md`「『fix』でも文書化された挙動を変えたら仕様変更」）
3. 独立導出レビューが**独立に「更新要」と判定し、置き場所を §4.7 と特定した**。両者を突き合わせて §4.7 を採る

§4.7 を採る根拠:

- **述語の正本が §4.7 にある。** 新しい規範は「§4.7 の carve-out で隠れた行は起動にも使えない」であり、
  正本の隣に置くのが `AGENTS.md`「正本を 1 か所に定め他は参照へ」に従う形である
- **§8.6 は個々の辺に overlay 条件を注記しない書き方を採っている。** 同節は
  「`indexing`（インデックス構築中）・`launching`（起動 in-flight）……は状態ノードではなく、
  どのモードにも重なる直交 boolean（overlay）である」と宣言し、`§4.7 の表示判定は indexing を……
  独立した第 3 の入力として扱っており」と §4.7 を指している。`launching` も個別の辺に書かれていない。
  ゆえに §8.6 へ書くと**写しになる**

**対立する先例を 1 つ記録する**（実測・`gh pr view 1072 --json files`）: #1072 は同じ `indexing` との
組み合わせで挙動を変えたが、触ったのは `docs/architecture.md` ほか 4 ファイルで **`SPEC.md` は含まない**。
それでも今回更新するのは、#1072 が変えたのは**既に在る判定述語の中身**であるのに対し、今回は
**新しいガードを起動経路へ足す**——§4.7 が現に規定していない事実を足す——ためである。

実行はフェーズ 3 の作業項目に置く。

## 未確定（実装前に潰す）

- [x] **実機での 1 回の再現を行う** — 2026-08-16 にユーザーが「行う」と裁定した（実 `config.toml` を
      一時的に書き換えることへの同意を含む）。**実施はフェーズ 0**（修正前の挙動しか測れないため）。
      **再現しなかった場合は実装へ進まない**——コード上の裏取り 2 系統と矛盾するので、
      見落とした解除経路を先に探す
- [x] **カテゴリ C（`smoke:egui`）は手元では不要** — `scripts/smoke-egui.ps1` を読んで判定した。
      同スクリプトが検証するのは hotkey 注入と `hotkey:registered` 等の trace 観測・窓の出没であり、
      **Enter による起動経路を一度も通らない**（`grep -n "Enter" scripts/smoke-egui.ps1` は 0 件）。
      本変更は trace イベント名・hotkey 登録・窓生成順のいずれにも触れない。
      なお `src-tauri/**` を変更するため **PR では `Smoke` workflow が paths 一致で自動起動する**
      （`docs/build-commands.md`「カテゴリ C」）
- [x] **フェーズ 2 を実施する** — 2026-08-16 にユーザーが「実施する（推奨）」を選んだ。
      AC4・`FrameIndexing`・フェーズ 2 の作業項目はすべて計画に残る（削除する枝は無い）

## plan-review 結果

- リスク: **高**
- レビュー方式: 独立導出 1 体（Step 2b。`workspace/plan-review-1077-derive.md`）
- エージェント数: 1

### 要対処

いずれも**主エージェントがコードで再照合してから**採った。

- **ガードの置き場所を `activate` → `activate_or_execute` へ** — 計画修正済み。
  再照合: `activate` の呼び出し元は `activate_or_execute:539` の 1 か所のみで（`git grep "self\.activate("`）、
  `activate_or_execute` は Enter（`:1359`）・クリック逆流（`view.rs:1146`）・`shift_activate` の
  `tools <= 1` 委譲（`:549` `:581`）が合流する入口である。`plain_results_hidden` は
  tool / instant 経路で構造的に偽ゆえ阻害しない
- **`SPEC.md` の更新先を §8.6 → §4.7 へ** — 計画修正済み。
  再照合: §8.6 は「`indexing`……は状態ノードではなく、どのモードにも重なる直交 boolean（overlay）」と
  宣言して §4.7 を指しており、個々の辺に overlay 条件を注記しない（`launching` も同様）。
  ゆえに §8.6 へ書けば写しになる
- **`on_enter` の flush 枝を触らない／冒頭で早期 return しない** — 作業項目として明文化。
  再照合: #1072 PR 本文が「flush 枝の本体を 1 行も変えていないこと」を受け入れ条件の担保に使っており、
  settled 側で行を保つのは §4.7「データと選択は保持」と整合する
- **呼び出し元の列挙を LSP で取り直す** — 作業項目に追加。
  独立導出レビューは LSP が使えず `git grep` に落ちたと自己申告している

### 軽微

- **合成を純粋関数へ切り出す提案** — 採らない。再照合の結果**合成が存在しない**（ガードは
  `plain_results_hidden` 単体で、「行が空でない」は別の層にある）。理由を「不変条件と異常系」へ記録
- **`on_nav_keys` の → / ← を射程に入れるか** — 双方が「同族だが起動ではない」で一致。AC8 に記録済み
- **tool メニュー入場を起動と見なすか** — 双方「見なす」で一致。AC2 に記録済み
- **`run_search_with` の live-read を残すか** — 双方「残す（クリア方向＝安全側）」で一致。記録済み

### 未検証

- **事実 3 の実機再現** — 未実施。ユーザー同意のうえ**フェーズ 0 で測る**（実装前でなければ測れない）
- **`on_enter` の呼び出し点そのもの** — テスト席が無い（`launcher_controller.rs` の `mod tests` は
  **0 件**・実測。`mod tests` を持つのは 25 ファイル）。ソーステキスト検知器と変異注入で代替する
- **smoke の H1 型否定形不変条件**（indexing 区間に `egui_launch` が現れない） — 独立導出レビューが
  候補として挙げたが、**index build の窓を決定的に作れるかが未測定**である。今回は採らない

### 判断

- 実装着手: **可**（未確定欄は空・人間レビュー承認済み。フェーズ 2 は実施、実機再現はフェーズ 0 で行う）

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の 2 条件に該当——**状態遷移を変更する**〔§8.6 の
  `Shift+Enter [tools >= 2]` 辺にガードが増える〕、**複数モジュール間のインターフェースを変更する**
  〔`view.rs` ↔ `launcher_controller.rs` の署名〕）
- plan-review: 独立レビュー 1 体（Step 2b・独立導出）
- 実行した check スキル: `/state-check`（要対処 1 件・整合 4 件）、`/symmetric-check`（要対処 1 件・
  判定 1 件）。`/race-check` は**同スキルの規定により計画段階では起動しない**（#784）——フェーズ 3 で実行する
- エージェント数: 2（Step 3b の敵対的調査 1 体 + `/plan-review` Step 2b 1 体）
- 要対処と反映:
  - `/state-check` Step 5 → **`SPEC.md` の更新要否が「不要」から「要」へ反転**（置き場所は独立導出
    レビューとの突き合わせで §8.6 → §4.7 へ再確定）。変更ファイル一覧とフェーズ 3 へ反映済み
  - `/state-check` Step 2 → `/r` 履歴行（Command intent）がガードの射程に入ることを AC7 に明示
  - `/state-check` Step 4 → → / ← のフォルダ突入はガードしないという判断を AC8 に明示
  - `/symmetric-check` Step 2c → `on_enter` の隣接 `bool` 取り違えに検出手段が無い。
    `FrameIndexing` newtype をフェーズ 2 の作業項目へ追加
  - `/symmetric-check` Step 3 → 置き場所を `start_launch` にしない判断とその根拠を明記
- 未検証: 事実 3 の実機再現（未確定欄で決める）。`on_enter` は依然テスト席を持たない
  （AC1〜AC3 は述語テスト + ソーステキスト検知器 + 変異注入で担保する）

## 人間レビュー

- [x] 承認済み — 2026-08-16 / 問い: "workspace/plan.md をこの内容で承認いただけますか？" / 回答: "承認する"
- 併せて裁定された 2 点:
  - 問い: "フェーズ 2（indexing をフレーム内で 1 回だけ読み、FrameIndexing newtype で on_enter / activate_or_execute へ配る）を実施いたしますか？" / 回答: "実施する（推奨）"
  - 問い: "事実 3（画面に出ていない行を Enter が起動する）の実機再現を行いますか？ 手順上、ユーザーさまの実 config.toml を一時的に書き換えます（事前にバックアップし、確認後に復元いたします）。" / 回答: "行う"
