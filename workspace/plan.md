# 実装計画: #1079 folder 往復で `is_unsettled` が偽になる

## 目的

`SearchState::is_unsettled` を、自分の doc が述べる意味——「最終クエリの結果がまだ行へ反映されていないか」——に**全域で**一致させる。folder を `→` / `←` で往復して Escape で戻った直後、行が復元 query の最終結果でないのに偽を返す状態を閉じる。

**判定は「バグ」である**（`AGENTS.md` 開発ワークフロー 1）。`SPEC.md` にこの述語・flush-on-Enter の記述は無く（`grep -n "is_unsettled\|should_flush\|#631\|最終クエリ" SPEC.md` が 0 件・2026-08-14 実測）、**述語の doc がここでの意図の正本**である。ゆえにコードを意図へ合わせる。**`SPEC.md` の同期は不要**。

## 受け入れ条件

1. `issue_search → enter_folder → on_escape` の 3 段の後、`is_unsettled(false)` が**真**を返す
2. 何も in-flight でない状態で folder へ入って戻った場合は**偽**のまま（過剰近似しない）
3. 復帰後に行が実際に差し替わったら（`set_results` / `reset` / worker 採り込み）偽へ戻る
4. `is_unsettled` の新しい合成が、`consume_reset_pending` の一覧へ入れ忘れる形の残余を**新設しない**
5. 修正前に落ち、修正後に緑になる検知器がユニットテストとして在る
6. 検知器が「呼び忘れ」の変異で落ちることを実測済みである

## 採る案と、案 A / 案 B を採らない理由

**案 C（復帰時に「行が query と対応していない」ことを覚え、`is_unsettled` に合成する）を採る。**

issue が案 C へ付けた懸念——「`SearchState` に show を跨ぐフラグが 1 つ増える（`consume_reset_pending` の一覧へ入れ忘れる形の残余を新設する）」——は、**フラグの clear を `put_rows` の内側へ置くことで消える**。`reset()` は `put_rows(Vec::new(), 0)` を通る（`search_state.rs:469`）ため、show を跨ぐクリアが**構造で**付いてくる。これは #1039（`1fac4e6`）が「行の差し替えに伴う義務を `put_rows` 1 か所へ集める」として立てた設計そのものである。

- **案 A（`on_escape` で `run_search()`）を採らない理由**: 検知器が置けない。修正が controller 側にあるため、`SearchState` のユニットテストは修正後も偽を返し続け、緑にならない。`launcher_controller.rs` に `#[cfg(test)]` は 0 件・`tests/` 無し・tauri の test feature 未宣言（3b 実測）ゆえ、検知器は smoke の trace 不変条件の新設になり費用が跳ね上がる。加えて `run_search_with` の Plain 腕は `indexing()` が真なら復帰行を空にする（`launcher_controller.rs:778-781`）ためガードが要り、`run_search()` 入口の `instant_prefix()` が `engine.lock()` を取る（同 :660）ため #1032 の待ちが Escape のフレームへ乗る。**なお issue と `on_escape` の doc が書く却下理由「同期 `engine.search`」は現在のコードに当たらない**——`run_search` の Plain 腕は worker への `send` である（Phase 4 で訂正する）
- **案 B（nav キーの前に flush）を採らない理由**: 同じく controller 側で検知器が置けないうえ、`→` / `←` の押下ごとに同期 `engine.search` が乗る。Enter（1 回きり・結果を待っている）で正当化されている同期を、連打しうるナビキーへ広げる

**却下した案 C の変種**: 「folder から復帰したら**無条件に**フラグを立てる」。1 フィールドだけで済み `enter_folder` の signature も変えずに済むが、in-flight が無いまま folder を往復した場合まで Enter に同期検索を乗せる。**より重いのは、述語が今度は逆向きに doc と食い違うことである**——#1079 は述語が意味に反することそのものが主題であり、良性の向きであれ新しい不一致を導入するのは筋が悪い。

## 設計

`enter_folder` の時点の `is_unsettled` を `FolderFrame` へ控え、`on_escape` の復帰でそのまま戻す。**述語を folder の往復のあいだ保存する**、という形である。

```
enter_folder(dir, armed)                            // controller は armed だけを渡す
  → unsettled_at_entry = self.is_unsettled(armed)   // 合成は SearchState の内側で行う
  → FolderFrame { .., unsettled_at_entry }
on_escape() の folder 枝:
  put_rows(f.restore_results, f.restore_selected)   // ここで restored_rows_stale = false
  self.restored_rows_stale = f.unsettled_at_entry;  // put_rows の後に立てる（順序が意味を持つ）
is_unsettled(armed) = armed || pending_seq() != 0 || self.restored_rows_stale
```

- **`armed` も捕まる**——`enter_folder` の内側で `is_unsettled(armed)` を撃つので、debounce が予約を持ったまま folder へ入った場合も真になる
- **合成を呼び出し側に書かせない**（`/symmetric-check` Step 2c の所見・採用）。初稿は controller に `is_unsettled(self.search_debounce.is_armed())` を書かせる設計だったが、**誤って `is_armed()` だけを渡しても型は通り、ユニットテストは値を直接渡すので検知できない**。`armed` だけを受け取れば、呼び出し側の義務は `is_unsettled` の既存の呼び出し（`launcher_controller.rs:1328`）と**同じ形**になり、新しい規約が増えない
- **`navigate_folder` は触らない**——frame を作り直さず `current_dir` を書き換えるだけ（`search_state.rs:305-313`）なので、控えた値はそのまま生き続ける。folder 内で `→` / `←` を重ねても根拠は保たれる
- **フラグの clear は `put_rows` の内側 1 か所**。`set_results` / `reset` / `enter_tool` / `accept_worker_rows` はすべてそこを通る

## 変更ファイルと対象シンボル

| ファイル | シンボル・箇所 | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/search_state.rs` | `SearchState`（フィールド追加 `restored_rows_stale: bool`） | 追加 |
| 〃 | `FolderFrame`（フィールド追加 `unsettled_at_entry: bool`） | 追加 |
| 〃 | `enter_folder`（:288） | signature に `armed: bool` を追加し、内側で `is_unsettled(armed)` を frame へ格納 |
| 〃 | `put_rows`（:226） | `self.restored_rows_stale = false;` を追加 |
| 〃 | `on_escape`（:430）folder 枝 | `put_rows` の後にフラグ代入 |
| 〃 | `is_unsettled`（:568） | 第 3 の disjunct |
| 〃 | 上記 5 つの doc（:209-225 / :285-287 / :411-429 / :524-567） | 同期 |
| 〃 | `#[cfg(test)]` | 検知器 + 境界 4 本を追加 |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `on_nav_keys` の `enter_folder` 呼び出し 2 箇所（:1192 `→` / :1224 `←`） | 引数追加 |
| `src-tauri/src/egui_shell/results_view.rs` | `RowsSnapshot::input_idle` の doc（:36） | 「食い違うのは `armed == false ∧ pending != 0` のとき」の式が不完全になる |
| `docs/architecture.md` | :228（flush が「発火しうる窓」の記述） | 窓が広がることを反映 |

**`is_unsettled` の production 消費者は `launcher_controller.rs:1328` の 1 つだけである**（grep 実測・2026-08-14）。`SPEC.md` は変更しない（上記「目的」の実測）。`docs/architecture.md:210` の mermaid はシンボル名だけで式を持たないため変更不要（実読）。

### `docs/adr/ADR-row-replacement-choke-point.md` は変更しない（plan-review の要対処 1 件・採用）

同 ADR の :21 と :34 は `armed || pending != 0` を逐語で写しており、第 3 disjunct を足すと古くなる。初稿はこれを変更ファイル一覧に載せていたが、**`docs/adr/ADR-adr-frozen-history.md` がそれを禁じている**（一次証拠を自分で読んで裁定した）:

> **「歴史は、消えることに対してだけ守り、変わることに対しては守らない。」** ADR 本文は決定日時点の世界の記述として凍結し、そこから外への参照（パス・見出し・SPEC 節）は生きた層の改名・移動に追随させない

同 ADR は「受容する残余」として「ADR 本文から生きた層への参照は今後黙って腐る（設計どおり）」と明記し、**自分自身がその初適用として `ADR-stale-identifier-detector-scope` を編集せずに覆した**と書く。ゆえに `ADR-row-replacement-choke-point.md` は**編集しない**——反転は生きた層（`is_unsettled` の doc）に書く。

**これは `AGENTS.md`「文書に事実の写しを増やす変更」の一般則の例外ではなく、その母集団の定義である**——`docs/adr/` は「写しを直す」対象ではない。

## 実装順序

**Phase 2 で core の変更と呼び出し点の移行を 1 タスクに束ねる**（`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」——`enter_folder` の signature を変えると `-D warnings` 下で呼び出し点が即 compile-fail になるため、分けると中間状態が存在しない）。

### Phase 1 — 検知器を置き、落ちることを測る（Red）

- [x] `search_state.rs` の `#[cfg(test)]` へ検知器を追加する: `issue_search` → `enter_folder` → `on_escape` → `assert!(s.is_unsettled(false))`。**この時点では `enter_folder` は現行 signature のまま**書き、Phase 2 で引数を足す
- [x] `cargo test -p snotra -q folder` を実行し、**この 1 本が落ちること**を出力ごと記録する（緑のまま通ったら検知器が並びを再現していない）

### Phase 2 — 修正（core + 呼び出し点を束ねる）

- [x] `SearchState` へ `restored_rows_stale: bool` を追加する。**`SearchState` は `Default` を導出していない**（実測）——`new()` の手書き構造体リテラル（`search_state.rs:179-186`）へ `restored_rows_stale: false,` を足す。足し忘れは compile-fail になる
- [x] `FolderFrame` へ `unsettled_at_entry: bool` を追加する
- [x] `enter_folder` の signature へ `armed: bool` を足し、**内側で** `self.is_unsettled(armed)` を撃って frame へ格納する（frame 構築より前に `let` で束縛する——`&mut self` の中で `&self` メソッドを呼ぶため）
- [x] `put_rows` に `self.restored_rows_stale = false;` を足す
- [x] `on_escape` の folder 枝で、`put_rows` の**後に** `self.restored_rows_stale = f.unsettled_at_entry;` を置く
- [x] `is_unsettled` へ第 3 の disjunct を足す
- [x] `launcher_controller.rs:1192` / `:1224` の呼び出しへ `self.search_debounce.is_armed()` を渡す（`self.state` と別フィールドゆえ two-phase borrow で通る。`self.state.is_unsettled(..)` を引数に置く形は採らない——上記「合成を呼び出し側に書かせない」）
- [x] **既存テスト内の `enter_folder` 呼び出しにも引数を足す**——`search_state.rs` の `enter_folder(` は 26 ヒットで、うち 2 つが production（`launcher_controller.rs`）、残りはテストである（grep 実測）。**compile-fail が移行漏れの検出器になる**ので、列挙は `cargo build -p snotra --all-targets` に任せてよい（`AGENTS.md`「改名・旧 API の削除は下流の compile-fail を移行漏れ検出器に」）。テストが渡す値は各ケースの意図に合わせる（in-flight を仕込んだケースは真、そうでなければ偽）
- [x] Phase 1 の検知器が緑になることを確認する

### Phase 3 — 境界条件のテストと変異検査

- [x] 境界テストを **5 本**追加する（下記「境界条件と検証」の表 #2〜#6 と一対一。#1 は Phase 1 の検知器）
- [x] **変異 3 種を一時的に入れ、それぞれで検知器か境界テストが落ちることを実測する**（`AGENTS.md`「検知器を置き、呼び忘れを再現する変異で落ちることまで確かめる」）:
  (a) `put_rows` の clear を消す → 境界テスト「復帰後に行が差し替わったら偽へ戻る」が落ちる
  (b) `on_escape` の代入を消す → 検知器が落ちる
  (c) `is_unsettled` の第 3 disjunct を落とす → 検知器が落ちる
- [x] 変異をすべて戻し、`cargo test -p snotra -q` が緑であることを確認する

### Phase 4 — 散文の同期

- [x] `on_escape` の doc（:411-429）の受容残余の記述を、#1079 で閉じた旨へ書き換える。**同時に、案 A の費用記述「Escape のたびに同期 `engine.search` をフレームに乗せる」が現在のコードに当たらないことを記す**（`run_search` の Plain 腕は worker への `send`。却下の実際の理由は「検知器が置けない」である）
- [x] `is_unsettled` の doc（:524-567）へ第 3 disjunct の意味を書き、**clear が `put_rows` にあるため reset 経路が構造で覆われる**ことを記す（:546-558 が `armed` について警告している懸念が、このフラグには当たらない理由）
- [x] **`is_unsettled` の doc :561-562「この述語が自分の意味に反して偽を返す既知の状態が 1 つある——folder を往復した直後である（受容する残余…）」を消す**（この一文が #1079 そのものである）
- [x] **`is_unsettled` の doc :566-567 が極性の理由づけの中で写している式「食い違うのは `armed == false ∧ pending != 0` のとき」を同期する**（`results_view.rs:36` と対になっている写し）
- [x] `enter_folder` の doc（:285-287）へ新引数の意味を書く
- [x] `put_rows` の doc（:209-225）へ clear を足す（「行の差し替えに伴う義務」の一覧に載る）
- [x] `results_view.rs:36` の「食い違うのは `armed == false ∧ pending != 0` のときである」を、第 3 disjunct を含む形へ直す
- [x] `docs/architecture.md:228` の記述を直す。**対象は 2 つある**——(a) flush が「発火しうる窓」の上端の記述、(b) 同行の「**Escape**・起動突入・空クエリ・reset は行を同期で差し替えることによってこれより早く閉じる」という言い切り。**(b) は folder からの Escape については成立しなくなる**（本変更が窓を跨いで持ち越すため）

### Phase 5 — 検証

- [x] `cargo test -p snotra -q`（**`--lib` を付けない**——`src-tauri` は `[lib]` を持たない）
- [x] `cargo clippy -p snotra --all-targets -- -D warnings`
- [x] `cargo doc --workspace --no-deps --document-private-items`（intra-doc link 切れは PostToolUse hook が沈黙し CI でのみ発火する・`.claude/rules/comments.md`）
- [x] `npm run governance:check`（`.rs` の doc とガバナンス文書を変更したため・`AGENTS.md` 条件別チェック）
- [x] `/symmetric-check`（フラグの真偽ペア）と `/state-check`（flush のガード条件）を実装差分へ当てる

### Phase 6 — 実装中に判明した作業

- [x] **tool が folder の上に積まれた状態での Escape 2 回にテストが無かった**（4a の `/state-check` を実装差分へ当てて発見）。SPEC §18.5 が直交と定める組み合わせであり、`on_escape` の tool 枝は `self.tool.take()` で早期 return して `self.folder` に触らない——控えた値が tool の出入りで失われないことを `tool_stacked_on_folder_still_restores_the_captured_unsettled` で固定した

### Phase 7 — code-reviewer 指摘への fix-forward

- [x] **[High] `enter_folder` が合成する `armed` の項に検知器が無かった**。変異 `let unsettled_at_entry = self.dispatch.pending_seq() != 0;` が **271 passed で素通りすることを自分で実測**（Phase 3 の変異 (a)(b)(c) がこの 1 種を落としていた＝受け入れ条件 6 が `armed` の項について未達だった）。テスト `folder_entry_captures_armed_not_only_in_flight` を追加し、**変異 (d) でこの 1 本だけが落ちることを実測**（271 passed; 1 failed → 復元後 272 passed）
- [x] **[Medium] `on_escape` の doc「Plain 腕は `search_tx.send` を撃つだけである」が偽**。Plain 腕の `set_results` は 2 件（実測）。「同期 `engine.search` を含まない」へ書き換え——**言いたかったことは正しいが、書いた主張が実装より強かった**
- [x] **[Low] 同 doc の `#[cfg(test)]` 不在の主張へ「2026-08-14 時点で」を付与**（`docs/comment-guidelines.md`「第一原則」が名指す「他のコードの現在の状態を主張する根拠」）
- [x] **[Low] `FolderFrame::unsettled_at_entry` の doc「突入の時点でしか判らない」を精確化**（厳密には folder の列挙結果が届くまでも観測できる。「常に観測できる点は突入時だけ」が正しい）
- [x] **[Low] 控えた `armed` が減衰しないことを受容残余として明記**（`layout.rs:451-465` を自分で読み機序を裁定。窓 50 ms・害 1 フレーム・過剰近似の向きは安全側）
- [x] **採らなかった指摘 2 件を記録**: [Medium] `ToolFrame` に対称のフィールドが無い / [Low] `architecture.md` の「folder からの Escape だけは閉じない」が tool 枝の閉じ方に依存する。**この 2 件はラウンド 2 で決着した**（Phase 8）——添えられていた機序「クリック経路から踏める」は誤りで、`enter_tool` は `on_enter` の flush の下流にしか無く実質到達不能である。所見 7 は取り下げ、ToolFrame の非対称は理由を doc へ残した

### Phase 8 — code-reviewer ラウンド 2 への fix-forward

**ラウンド 2 は「解消したか」を私の報告ではなく自分の道具で測って返した**（変異 (d) を自分で当てて 271 passed; 1 failed を再現し、`cmp` で復元の byte 一致まで確認）。加えて**修正の周辺に新しい誤りが 3 件生じていた**——`AGENTS.md`「修正は指摘箇所へ注意が集中し、周辺に新しい誤りを生む」の実例である。

- [x] **[Medium] 正本を直して写しを 2 か所落としていた**（#977 の型）。`FolderFrame::unsettled_at_entry` の doc（正本）を「常に観測できる点は突入時だけ」へ直したのに、`on_escape` の doc が「**なぜ突入時点でしか判らないかは**（正本）が正本」、インライン `//` が「突入時点でしか判らないので」のまま残っていた。**正本を名指しながら、その正本が明示的に否定した表現で要約していた**ので、リンクを辿った読者は矛盾に当たる。`//` は rustdoc が読まないため `cargo doc` も `governance:check` も緑のまま素通りした
- [x] **[Low] 「その `set_results` は必ず来る」が不要な全称だった**。反例 2 件（`spawn_folder_load` は `try_state` が `None` なら何も send せず return / dead・slow UNC の滞留窓は `folder_load_pending` の存在が認めている）。**そもそも論証に要らない**ので「届いたかどうかが Escape の時点で判らない」へ置き換えた
- [x] **[Low] 偽陽性の「安全側」の説明が `indexing()` を落としていた**。「行を余計に作り直す side へ倒れ」は空クエリ・`indexing()` 中に成り立たない（flush の `None` 枝が `set_results(Vec::new())` を撃ち、起動そのものが止まる）。plan.md が「挙動が変わる 1 点（意図的）」として自分で名指している事象を、doc の言い回しが落としていた
- [x] **[Low] 「trailing 発火で interval のうちに落ちる」に「最後の打鍵から」が無かった**（同ファイルの既存 doc は「**最後の**打鍵から interval」と書いており不一致だった）
- [x] **`ToolFrame` に対称のフィールドを置かない理由を doc へ残した**（否定の知識）。**ラウンド 1 で添えられた機序「production で踏めるのはクリック経路だけ」はレビュア自身が撤回し、私も一次証拠で裁定した**——`enter_tool`(:570) ← `shift_activate`(:546) ← `on_enter`(:1357) の 1 本鎖で、クリック(`view.rs:1146`)は `activate_or_execute` を直呼びして `enter_tool` へ届かない。ルート `CLAUDE.md`「所見が正しくても、そこに添えられた機序の説明は独立に誤りうる」の実例
- [x] **[取り下げ] 所見 7（`architecture.md:228`）**: 所見 2 が「到達不能」で決着したため、「folder からの Escape だけは閉じない」は但し書き無しで真。現行文のままとする

**自己検算で 1 件、レビュアの指摘より先に自分で見つけて直した**: 「窓が 50 ms」は `launcher_controller.rs` の 2 箇所にあるリテラルの写しで、interval を変えれば黙って腐る（`AGENTS.md`「数ではなく正本を指す」）。**全称表現を訂正する当のコミットで、別の数え上げを新しく書いていた。**

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| 行を差し替えたらフラグは必ず落ちる（生成 1 : 破棄 1） | `put_rows` の内側に clear を置く＝機構。境界テストと変異 (a) |
| show を跨いでフラグが残らない | `reset()` が `put_rows` を通る＝機構。`consume_reset_pending` は `state.reset()` を呼ぶ（`launcher_controller.rs:989` 実読）ので、issue が案 C へ懸念した「一覧へ入れ忘れる形の残余」は成立しない。境界テスト |
| in-flight が無いまま folder を往復しても偽のまま | 境界テスト（受け入れ条件 2） |
| `is_unsettled` の production 消費者は `on_enter` の 1 つ | grep 実測済み。増えたら `should_flush_on_enter` の射程を再検討する |

**異常系**: フラグが真のまま Enter が来ると `on_enter` が同期 `engine.search` を 1 回撃つ（:1337-1341）。空クエリ・indexing 中は `None` 枝を通り `set_results(Vec::new())` になる——これは flush の既存の挙動であり、本変更が新設する経路ではない。

**挙動が変わる 1 点（意図的）**: folder を往復して戻った直後、**indexing 中**の Enter は、これまで復元行を起動していたが、今後は flush の `None` 枝が行をクリアするため起動しなくなる（`:1349` の `!results.is_empty()` ガードで止まる）。**これは flush が意図した挙動そのものである**——`:1342` のコメントが「空クエリ・indexing 中にクリアを落とすと、古い行が残ったまま直後の `activate_or_execute` がそれを起動する」と、まさにその危険を名指している。本変更は folder 往復の経路をその既定の扱いへ合流させる。

**`/state-check` の直交性判定**（全項目 [整合]・根拠つき）:

| 組み合わせ | 判定 | 根拠 |
|---|---|---|
| × `FolderExpansionMode` | 直交・整合 | folder 中は `should_flush_on_enter` が `ViewKind::Results` を要求するため読まれない（`search_state.rs:590`）。真のまま再度 `→` を押すと `is_unsettled(armed)` が真を返し、2 周目の frame へ正しく伝播する |
| × `ToolSelectionMode` | 直交・整合 | 突入は `on_enter` の flush 後（`launcher_controller.rs:1325-1351`）ゆえフラグは false。tool 枝の Escape は `put_rows` を通って clear し、その後の folder 枝の Escape が frame から立て直す |
| × `QueryIntent`（Command / Instant） | 排他・整合 | `should_flush_on_enter` の `is_plain` が既存のガードとして効く。フラグは残るが、打鍵で `set_results` を通って落ちる |
| × `indexing` | 直交・整合 | 上記「挙動が変わる 1 点」 |
| × `launching` | 直交 | flush は起動より前に完了する（`:1325-1348` → `:1349`） |
| SPEC §8.6 の状態遷移図 | **更新不要** | 図が持つのは UI モードであり、この述語は遷移ガードに現れない。`FolderExpansionMode --> NormalMode: Escape` は無条件のまま（`SPEC.md:501-548` 実読） |

**受容する残余（新設ではなく、射程外として残すもの）**: 復帰直後の**クリック**起動は flush を経ない（`view.rs:1146` が `activate_or_execute` を直呼び）。これは `pending != 0` の場合も同じで #1079 以前から在り、本 issue の並び（`→`/`←` → Escape → Enter）の外である。3b が ⚠️ として挙げたが、ここでは裁定しない。

## 境界条件と検証

| # | 条件 | 期待 | テスト |
|---|---|---|---|
| 1 | in-flight のまま folder 突入 → Escape | `is_unsettled(false) == true` | 検知器（Phase 1） |
| 2 | in-flight 無しで folder 突入 → Escape | `is_unsettled(false) == false` | 境界テスト |
| 3 | Escape で真になった後 `set_results` | 偽へ戻る | 境界テスト（変異 (a) の受け皿） |
| 4 | Escape で真になった後 `reset()` | 偽へ戻る（show を跨がない） | 境界テスト |
| 5 | folder 内で `navigate_folder` を重ねてから Escape | 控えた値が保たれる | 境界テスト |
| 6 | Escape で真になった後に**打鍵して worker へ dispatch** | フラグは**真のまま残り**、worker 結果の採り込み（`accept_worker_rows` → `put_rows`）で落ちる | 境界テスト |

**境界 6 の根拠**（`/state-check` Step 4 で発見）: `run_search_with` の Plain 腕は、送信できたら `set_results` を呼ばず前の行を保つ（`launcher_controller.rs:785`）。ゆえに `put_rows` を通らず**フラグは消えない**——そして行はまだ古いのだから、それが正しい。落ちるのは結果が届いて行が実際に差し替わる瞬間である。**この性質は「clear を `put_rows` に置く」という設計の帰結であり、偶然ではない。**

## 未確定（実装前に潰す）

*なし*（下記 2 件は本計画の作成中に潰した）

- [x] `SPEC.md` の同期要否 — `grep -n "is_unsettled\|should_flush\|#631\|最終クエリ" SPEC.md` が 0 件（2026-08-14 実測）。**同期不要**
- [x] `docs/architecture.md:210` の mermaid 図の変更要否 — 実読の結果シンボル名（`should_flush_on_enter ∘ SearchState::is_unsettled`）だけで式の写しを持たない。**変更不要**。散文の :228 のみが対象

## セルフレビュー

- リスク: **高**（`plan-review`「リスク判定」の「状態遷移を変更する」「公開 API を変更する」＝ `enter_folder` の signature に該当）
- plan-review: 独立レビュー1体（Step 2・計画準拠）
- 実行した check スキル: `/symmetric-check`（トリガー「対称ペア＝フラグ真偽・生成/破棄」）と `/state-check`（トリガー「ガード条件を追加/変更」）。どちらも `AGENTS.md`「条件別チェック」の該当行から
  - `/symmetric-check`: 要対処 1 件 — **合成を呼び出し側に書かせる設計は検知できない誤配線を生む**。採用し、`enter_folder` が `armed` だけを受け取る形へ設計変更した。終端は 生成 1（`:289`）: 破棄 2（`:438` 消費 / `:459` 破棄）で、破棄 2 が消費しない理由も根拠つきで判定済み
  - `/state-check`: 不整合なし。直交性マトリクス 5 組 + SPEC §8.6 を全項目 [整合] と判定（表は「不変条件と異常系」節）。境界条件 #6 を新たに発見し、テストへ追加した
- エージェント数: 2（Step 3b の敵対的調査 1 体 + plan-review Step 2 の 1 体。check スキル 2 本はインライン実行でエージェントを起動していない）
- 要対処:
  - 3b（調査）: 1 件 — 並びが 5 段でなく 3 段＝過剰仕様。**採用**（`research.md` へ反映。機序は `search_dispatch.rs:66-68` を自分で読んで裁定）。加えて ⚠️ 2 件（1 件採用・1 件は射程外として保留）と追加指摘 1 件（却下・理由は `research.md`）
  - plan-review（計画）: 1 件 — **`docs/adr/` は編集しない規約に反していた**。`ADR-adr-frozen-history.md` を自分で読んで裁定し、**採用**して ADR を変更ファイル一覧から外した。軽微 2 件（`architecture.md:228` の Escape の言い切り / `is_unsettled` doc の「既知の状態が 1 つ」の明示削除）も**両方を Phase 4 の作業項目へ昇格**させた（いずれも grep でなく実読で確認済み）
- 未検証:
  - 変異検査（Phase 3）は実装時に実施する。計画段階では検知器が落ちることを未測定である（Phase 1 がそれを測る作業項目そのものである）
  - plan-review が `search_state.rs` の 1100 行以降（テストモジュールの後半）を未読と申告している。既存テストとの命名衝突は実装時に compile-fail で分かるため、ここでは受容する

## 人間レビュー

- [x] 承認済み — 2026-08-14 / 問い: "`workspace/plan.md` をご確認ください。注釈を追記していただくか、この計画を承認していただけますか？" / 回答: "OK"
