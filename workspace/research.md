# research: issue #1201 — ソーステキスト検査の母集団をディレクトリにする

対象 issue: #1201「検討: ソーステキスト検査の母集団をディレクトリにし、「起動の入口を 1 ファイルへ集める」規範を機構へ落とす」
ブランチ: `chore/source-text-probe-population-dir`
調査日: 2026-08-27

## 1. issue の要約

`launcher_controller/activation/tests.rs` の 3 本のソーステキスト検査は `include_str!("../activation.rs")` で母集団を取る。そのため #1200 の分割では「**起動の入口を `activation.rs` の外へ出さない**」という規範を `activation.rs` の `//!` へ置いた。

- 入口を移すと**赤になる**（各検査がアンカーを字下げ 4 のメソッドヘッダとして先に assert する）
- **ただし赤を消す最短手が `include_str!` のパス書き換え**であり、そうすると射程が黙って狭まる

提案: 母集団をファイルではなく**ディレクトリ**にし、規範を機構へ落とす。

**射程外（issue が明示的に受容する死角）**: 別の子モジュールに 4 本目の入口を**新設**する形は、ディレクトリ化しても沈黙する。対象の正本が `tests.rs` の `entry_points` 配列であり、母集団の側から導かれていないため。

## 2. 関連ファイル・シンボル（実在を確認済み）

crate は `snotra`（`src-tauri/Cargo.toml` の `[package] name = "snotra"`）。`CARGO_MANIFEST_DIR` は `src-tauri` を指す。

### 母集団になるディレクトリ

`src-tauri/src/egui_shell/launcher_controller/`（`env!("CARGO_MANIFEST_DIR")` からは `/src/egui_shell/launcher_controller`）

| エントリ | 種別 | 行数 |
|---|---|---|
| `activation/` | ディレクトリ（中は `tests.rs` のみ） | — |
| `activation.rs` | ファイル | 635 |
| `folder_nav.rs` | ファイル | 218 |
| `frame_stages.rs` | ファイル | 138 |
| `hide_request.rs` | ファイル | 106 |
| `search_flow.rs` | ファイル | 311 |
| `updater_toast.rs` | ファイル | 93 |

親モジュールは**ディレクトリの外**の `src-tauri/src/egui_shell/launcher_controller.rs`（13 KB）で、`mod activation; mod folder_nav; mod frame_stages; mod hide_request; mod search_flow; mod updater_toast;`（56〜61 行）を持つ。`activation.rs:635` に `mod tests;`。

### 触る対象（`activation/tests.rs`）

| シンボル | 行 | 役割 |
|---|---|---|
| `method_body(src, anchor, canary) -> String` | 26 | メソッド本体の切り出し（存在形が使う）。アンカー字下げ 4 assert・終端 assert・canary assert の 3 本 |
| `method_header(line) -> Option<&str>` | 172 | 字下げ 4 ちょうど・空白種別まで見る・`pub(...)` / `async` を読み飛ばす |
| `owners_of(src, needle) -> Vec<String>` | 237 | 出現をファイル全体から列挙し直前のヘッダへ帰属（否定形が使う） |
| `activation_uses_frame_values_not_live_reads` | 415 | 否定形。`include_str!` 使用 |
| `activation_entry_points_consult_the_display_gate` | 494 | 存在形（`method_body`）。`include_str!` 使用 |
| `on_enter_delegates_the_flush_decision_to_the_predicate` | 564 | 存在形（`method_body`）。`include_str!` 使用 |

helper の合成 fixture テスト（母集団に依存しない・**今回の変更で影響を受けない**）: `method_body_is_line_ending_agnostic`(75) / `..._rejects_an_anchor_at_the_wrong_indent`(95) / `..._indented_too_deeply`(110) / `..._a_tab_indented_anchor`(125) / `..._a_population_without_a_terminator`(140) / `..._without_the_canary`(154) / `method_header_requires_exactly_four_spaces_of_indent`(272) / `method_header_accepts_visibility_and_async_before_fn`(300) / `owners_of_attributes_a_nested_fn_to_the_outer_method`(320) / `owners_of_drops_occurrences_without_an_indent_four_owner`(337) / `owners_of_is_line_ending_agnostic`(354)。

### 規範の写し（同じ変更で整合させる対象）

1. `activation.rs` の `//!` の当該 2 段落 — 「起動の入口をこのファイルの外へ出してはならない（この制約の正本はここである）」＋「守られ方は方向で違う。移動は赤になり、追加は沈黙する」。**これが機構へ落ちて不要になる部分**。**行番号で断定せず見出し文で grep すること**（`.claude/rules/src-tauri.md`。3b が「3〜15 行」という当初の記述が 2 行広すぎると実測した——実際は 3〜13 行で、14 行目からは slash コマンドの話に移る）
2. `activation/tests.rs` の `//!` 3〜5 行 — 「母集団は `activation.rs` 1 枚である」
3. `activation_uses_frame_values_not_live_reads` の doc（376〜413）— 「母集団の外に在って初めから見えない」（`search_flow.rs` の `self.indexing()` / `launcher_controller.rs` の `read_config`）、「母集団は production 1 枚（`activation.rs`）で、この `mod tests` は別ファイルだから」、末尾「母集団は `activation.rs` 1 枚である」
4. `method_header_requires_exactly_four_spaces_of_indent` の doc（255 付近）— 「`include_str!` が読む `activation.rs` には字下げ 0 / 8 の `fn ` 行が 1 本も無く」
5. `method_body` の doc（21〜25）・`method_body_is_line_ending_agnostic` の doc（72）— `include_str!` は checkout された実ファイルを読む、の言及

## 3. 実測（このツリー・2026-08-27）

判定ロジックを JS へ写して実データに当てた（`scratchpad/simulate.mjs`）。**移植は逐語ではない**——`method_header` は逐語だが、`owners_of` の `None` 分岐が違う（Rust の `owners.extend(current.map(str::to_string))`（tests.rs:246）は帰属先の無い出現を**捨てる**のに対し、JS は `null` を push する）。**現コーパスでは非発火**（下の 3.3 のとおり全 needle 出現が自ファイルの最初のヘッダより後に在る）ゆえ、下の実測値はこの差の影響を受けていない。**3b の敵対枠が指摘し、Rust 側のソースで裁定した。**

### 3.1 `read_dir` が返すもの

```
read_dir order: [D]activation, activation.rs, folder_nav.rs, frame_stages.rs,
                hide_request.rs, search_flow.rs, updater_toast.rs
files-only *.rs: activation.rs, folder_nav.rs, frame_stages.rs, hide_request.rs,
                 search_flow.rs, updater_toast.rs
```

**`activation/` はディレクトリなので、`is_file()` フィルタだけで `tests.rs` が母集団から落ちる**（再帰しない限り）。これは重要——`tests.rs` は `"fn on_enter("` / `"read_config("` / `"self.indexing()"` を**リテラルで綴る**うえ、その `fn` はすべて字下げ 0 なのでヘッダとして認識されず、混入すれば**直前のファイルの最後のヘッダへ帰属する**。

> **この順序は実測 1 台ぶんであり、OS/FS 依存の主張はしない。** 設計側で `sort` して非決定性を消す（下の 5.1）。

### 3.2 アンカーの所在（**`read_dir` が直接返す 6 枚のうち** `activation.rs` だけ）

> **「配下」を再帰的に読んではならない。** `activation/tests.rs` は 3 アンカーを**文字列リテラルで**綴る（418〜420 / 498〜499 / 566 行）。母集団は `read_dir` が直接返す 6 枚に限られ、そこに tests.rs は入らない（3.1）。3b の敵対枠が、当初の「配下のどのファイルも含まない」という書き方を字面で偽にした。


```
fn on_enter(            -> ["activation.rs"]
fn activate_or_execute( -> ["activation.rs"]
fn shift_activate(      -> ["activation.rs"]
```

字下げ 4 のヘッダは 6 ファイル合計 **31 本**で、3 アンカーはいずれもヘッダとして認識される。

### 3.3 needle の帰属（Design B＝ファイル単位で走査した場合）

| needle | ファイル | 帰属先 |
|---|---|---|
| `self.indexing()` | `search_flow.rs` | `fn run_search_with(` |
| `read_visible_rows(` | （なし） | — |
| `read_config(` | `activation.rs` | `fn execute_instant_selected(` / `fn resolve_tools(` |
| `read_config(` | `hide_request.rs` | `fn auto_hide_enabled(` |
| `read_config(` | `search_flow.rs` | `pub(super) fn instant_prefix(` / `fn run_search_with(` |

**入口 3 本へ帰属する出現は 1 つも無い**＝ディレクトリ化しても現在のツリーは緑。

### 3.4 素朴な連結（issue のスケッチ）も**今日は**壊れない

```
self.indexing():    owners=1 attributed-to-entry-point=0
read_config(:       owners=5 attributed-to-entry-point=0
```

すべての needle 出現が、**自ファイル内の**字下げ 4 ヘッダより後に在るため。ただし `activation.rs` の**最後のヘッダは `pub(in crate::egui_shell) fn on_enter(`**（589 行）であり、`activation.rs` の直後に来るファイルが「最初のヘッダより前に needle を持つ」形になった瞬間、その出現は `fn on_enter(` へ帰属して**偽陽性で赤くなる**。しかも `read_dir` の順序に依存するので**赤くなるかどうかが非決定的**になる。**潜在であって現存の欠陥ではない**（今日 0 件）。

### 3.5 字下げの国勢調査（母集団をディレクトリへ広げても変わらない）

6 ファイルすべてで col0 / col8 / tab 字下げの `fn ` 行は **0 本**。`endsWithNewline=true`、この作業ツリーは全ファイル LF（`crlf=false`）。

→ **`method_header` の字下げ述語は、母集団を広げても依然コーパスからは測れない**（合成 fixture でしか固定できない）。tests.rs:255 の doc の主張は**射程を `activation.rs` からディレクトリへ書き換えたうえで、なお成立する**。

### 3.6 親 `launcher_controller.rs`（ディレクトリの外）

`read_config(` が 194 行に 1 つ、直前のヘッダ `pub(super) fn lang(`（193）へ帰属する。アンカーは 0 本、`mod tests` は無い。**含めても含めなくても現在のツリーは緑。**

### 3.7 リポジトリ内の他のソーステキスト検査（**射程外**と確認）

`grep -rn "include_str!" src-tauri/src/` の結果、コード上の使用は次の 4 サイト。

| サイト | 行 | 今回の射程 |
|---|---|---|
| `egui_shell/icon_textures.rs` | 427, 433 | **外**（`view.rs` / `results_view.rs` を読む別概念） |
| `egui_shell/view.rs` | 1421 | **外**（フレーム内 1 回読みの骨格） |
| `indexing.rs` | 317 | **外**（`top_level_fn_body`。列 0 側の別実装） |
| `egui_shell/launcher_controller/activation/tests.rs` | 416, 495, 565 | **今回の対象** |

## 4. 却下済み ADR との関係（読み直して確認）

### `ADR-source-text-probe-helper-locality`（Accepted）

- **却下理由 2（新ファイルの統治コスト）** — 不成立。今回は新ファイルを作らない
- **反転条件**「次にソーステキスト検査を**新設する** issue が立ったら（＝3 つ目の局所実装が生まれるなら）」— **該当しない**。今回は既存 3 本の母集団の取り方を変えるだけで、局所実装は 2 つ（`method_body` / `top_level_fn_body`）のまま
- **却下理由 1（稼働中のガードへの爆風）は当たる** — 「`method_body` を一般化すれば、それに依存する検査の変異注入を再実測する義務が生じる」。issue の記述どおり
  - **memory の警告に従い、却下理由の失効を確認した**: 却下理由 1 が前提にしている「変異注入の再実測義務」は `AGENTS.md`「レビュー指摘へ修正（fix-forward）を当てた」行として現存する（実在を確認）。失効していない

### `ADR-source-text-probes-not-lifted-to-types`（Accepted）

型化の可否の話で、今回はソーステキスト検査の**ままの**変更ゆえ射程外。issue の判断どおり。

> **副次的に見つけた doc drift（今回の射程外）**: この ADR は当該サイトを `launcher_controller.rs` と呼ぶが、#1200 の分割で検査は `launcher_controller/activation/tests.rs` へ移った。ADR は凍結された歴史なので**直さない**（`AGENTS.md`「意思決定記録」）。

## 5. 設計の候補と、採る形

### 5.1 Design A（issue のスケッチ）— 連結してから走査

```rust
// read_dir → 子 *.rs を連結して src とする
```

**採らない。** 連結は issue が挙げた 4 つの「測るべきこと」のうち 3 つを**自分で作り出す**:

- 連結の境目（改行の有無）
- `read_dir` の順序への依存
- `owners_of` の帰属がファイル境界を跨いで壊れる（3.4 の潜在欠陥）

### 5.2 Design B（採る）— **母集団を `Vec<(ファイル名, 中身)>` にし、helper は逐語で据え置く**

`method_body` / `method_header` / `owners_of` の**本体を 1 バイトも変えない**。変えるのは母集団の配り方だけ。

```
fn sources() -> Vec<(String, String)>   // 新設: read_dir → is_file() && ends_with(".rs") → sort → read_to_string
fn sole_file_with(sources, anchor) -> &str   // 新設: アンカーを含むファイルが「ちょうど 1 枚」であることを assert し、その中身を返す
```

- **存在形**（`method_body` を使う 2 本）: `sole_file_with` で 1 枚に絞ってから、既存の `method_body` をそのまま呼ぶ
- **否定形**（`owners_of` を使う 1 本）: ファイルごとに既存の `owners_of` を呼んで `flat_map` する

**この形が買うもの**:

| issue が挙げた「測るべきこと」 | Design B での扱い |
|---|---|
| 連結の境目の改行コード非依存 | **連結しないので問題が消える**（各ファイルを個別に `str::lines` へ通す。`method_body` の CRLF 非依存はそのまま） |
| `read_dir` の順序の非決定性 | **`sort` で消す**。加えて `sole_file_with` の一意性 assert が「最初に見つかった方」への依存を消す |
| ファイル境界を跨ぐ帰属の破れ（3.4） | **構造的に起こりえない**（`owners_of` の `current` がファイルごとにリセットされる） |
| ディレクトリが空・読めないときの沈黙 | `read_dir` 失敗は `unwrap` で panic ＝赤。空なら 3 本とも「アンカーがちょうど 1 枚に無い」「ヘッダとして見つからない」で赤。**実測で確かめる**（§6 の変異 e） |
| 子を 1 枚足したときの自動編入と誤爆しないこと | 3.3 の帰属表がその実測（現存 5 ファイルぶんの needle がすべて非入口へ帰属）。**変異 f で境界の形も測る** |

**helper を据え置くことが、ADR 却下理由 1（爆風）を最小化する**——合成 fixture の 11 本は逐語で生き、再実測が要るのは「母集団の配り方」と「3 本の検査」だけになる。**実装中に helper を「ついでに改良」しないこと**が、この費用勘定を支えている。

**issue のスケッチからの逸脱である**（連結しない）。理由は上表。人間レビューで承認を得る。

**この設計が広げる曝露面（3b の指摘・受容する残余として宣言する）**: `fs::read_to_string` は `#[cfg(test)]` 属性を見ないので、**将来どれかの子モジュールへ inline の `#[cfg(test)] mod tests { … }` を書くと、その中身が母集団へ入る**。現行の `include_str!` 設計も `activation.rs` 1 枚について同じ性質を持つが、**曝露面は 1 枚から 6 枚へ広がる**。今日の実測では 6 枚のいずれにも inline test は無い（`activation.rs:634-635` の `#[cfg(test)] mod tests;` は外部ファイルへの宣言のみ）。

倒れ方は 2 方向で、**片方だけが沈黙する**:

- inline test が**アンカーの綴りを持つ** → `sole_file_with` が 2 枚を見つけて**赤**（誤検出だが気づける）
- inline test が**needle を持つ** → その test 関数のヘッダへ帰属し、入口ではないので**緑**（無害）
- **沈黙する形は 1 つだけ**: production の入口が消え、同名のアンカーを持つ inline test だけが残ると、`sole_file_with` は 1 枚を返して**テスト側のコピーを測る**。ただしその形は入口の削除を伴うので、production 側が先にコンパイルエラーになる経路が実在する（この検査の外の守り）

**塞がない。** 検知器は必要な分だけ縛る（`detector-scope-only-as-tight-as-needed`）——inline test を排除する述語は「`#[cfg(test)]` から先を読み飛ばす」パーサを要し、道具立てが検査対象より複雑になる（`owners_of` の doc が同じ判断を既に記録している）。**この issue のスコープでは死角として宣言する。**

### 5.3 `activation_uses_frame_values_not_live_reads` にも一意性 assert を足す

この検査は `method_body` を使わないので、5.2 の `sole_file_with` を通らない。現状の assert は「**どこかのファイルに**アンカーがヘッダとして在る」だけなので、`activation.rs` の `on_enter` を改名しつつ別の子モジュールへ同名の `fn on_enter(` が生まれると、**ヘッダ assert は緑のまま検査が別のメソッドを見る**。3 アンカーすべてに `sole_file_with` の一意性 assert を当てて対称にする。

### 5.4 親 `launcher_controller.rs` を母集団に含めるか

**含めない**（issue の「子 `*.rs`」に従う）。含めた場合との差は 3.6 のとおり今日は無い。

**残余**: 入口を親 `launcher_controller.rs` へ戻す形は、アンカーが 1 枚にも見つからず**赤になる**（沈黙ではない）。規範は「1 ファイルへ集める」から「`launcher_controller/` ディレクトリの中に置く」へ**弱まる**のであって、消えるわけではない。これは issue が「消すのは『移動で射程が狭まる』の方だけ」と書いた射程と整合する。

## 6. 変異注入の一覧（**設計を確定する前に書き下ろす**・`.claude/rules/safety-nets.md`）

**再実測**（既存の守りが同じ強さで残ることの確認）:

- (a) `on_enter` の本体へ `self.indexing()` を 1 行挿す → 赤（tests.rs:412 が実測済みと記録する変異）
- (b) `activate_or_execute` から `plain_results_hidden(` の呼び出しを消す → 赤
- (c) `on_enter` から `if crate::egui_shell::should_flush_on_enter(` の行を消す → 赤

**新機構の証明**（ディレクトリ化で初めて生きる枝）:

- (d) `shift_activate` を丸ごと別の子モジュール（例: `folder_nav.rs`）へ移す → **3 本とも緑のまま**（これが issue の便益。`include_str!` 版なら赤）。続けて移した先でゲートを 1 つ落とす → 赤
- (e) 母集団が空（フィルタを一時的に `false` にする等、**稼働中のガードには触らずメモリ上の複製で**）→ 3 本とも赤
- (f) 2 枚目のファイルの**最初のヘッダより前**に needle を置く → 帰属先が無いので `owners_of` が出現ごと**捨てる**（tests.rs:246 の `current.map(...)`。既存の `owners_of_drops_occurrences_without_an_indent_four_owner` が固定している挙動）＝緑。**ファイル境界を跨いで `fn on_enter(` へ帰属しない**ことの実測
- (g) 2 枚のファイルに同じアンカーを置く → `sole_file_with` が赤

**変異が「実際に起きた回帰の姿」と同じか**（`.claude/rules/safety-nets.md`）: (a)〜(c) は既存 doc が記録する回帰の姿そのもの。(d)〜(g) は**回帰ではなく機構の性質**を測るので、赤/緑の期待値を上に明記して対照とする。

## 7. 技術的制約

- **`include_str!` → `fs::read_to_string` で性質が変わる**: コンパイル時 → 実行時。パスは `env!("CARGO_MANIFEST_DIR")`（コンパイル時定数）を基点にするので CWD には依存しない
- **再コンパイル契機が変わる**: `include_str!` は読んだファイルの変更で再ビルドを誘発するが、`read_dir` は誘発しない。ただし対象はすべて同 crate の `mod` ソースなので、cargo は通常どおり再ビルドする。**`mod` 宣言の無い野良 `.rs` を置くと、コンパイルされないまま母集団へ入る**（誤爆＝赤方向。`governance:check` の `G-module-linkage` が別に捕まえる）
- **`str::lines` は据え置き**（改行コード非依存の根拠）。`fs::read_to_string` は `include_str!` と同じくバイト列をそのまま返す（改行変換をしない）ので、CRLF checkout でも既存の非依存性がそのまま効く
- **`cargo fmt` が走ることへの暗黙の依存**（`owners_of` の doc が正本）は変わらない
- **セーフティネットの変更**ゆえ、ルート `CLAUDE.md`「最重要ルール」2 に当たり**人間の合意を要する**

## 8. 未解決の疑問（plan.md の未確定欄へ送る）

1. `sources()` の失敗経路を `unwrap`（panic ＝赤）でよいか、明示の `expect` メッセージを付けるか — **付ける**方向で書くが、実装時に文言を決める。**失敗経路は 2 つあり、`read_dir` の失敗だけでは足りない**（3b の指摘）: (1) `read_dir` / `DirEntry` の失敗、(2) **`read_to_string` の失敗**（非 UTF-8 バイトの混入等）。どちらも panic ＝赤へ倒す。今日の 6 枚はすべて有効な UTF-8（3b が `file` コマンドで実測）
2. 母集団が空でないことの下限 assert（例: 「2 枚以上」）を置くか — **置かない**方向（数え上げは足すたびに腐る・`AGENTS.md`「検証の作法」）。アンカーの一意性 assert が空を赤にすることを (e) で実測して代替とする
3. `activation.rs` の `//!` の当該段落は**丸ごと削るか、機構化された旨へ書き換えるか** — 「4 本目の入口の新設は沈黙する」という受容残余は**残す**必要がある

## 9. 敵対的調査（3b）の結果

`general-purpose` / `sonnet` 1 体。母集団は本文書の全主張。全文は `workspace/adversarial-1201.txt`。

### 壊せた項目（1 件）

| 所見 | 採否 | 理由・反映先 |
|---|---|---|
| 命題 (ii)「`launcher_controller/` **配下**のどのファイルもアンカーを含まない」は字面で偽——`activation/tests.rs` が 3 アンカーを文字列リテラルで綴る（418〜420 / 498〜499 / 566） | **採用** | 一次資料で確認。母集団は「`read_dir` が直接返す 6 枚」であって再帰的な子孫ではない。§3.2 の冒頭へ限定を明記した。**設計は無傷**（tests.rs は構造的に列挙されない） |

### ⚠️ 確信の持てない所見（5 件）

| # | 所見 | 採否 | 裁定 |
|---|---|---|---|
| 1 | `simulate.mjs` の `owners_of` が Rust と非逐語（`None` 分岐で Rust は捨て、JS は `null` を push） | **採用** | **Rust 側 tests.rs:246 の `owners.extend(current.map(str::to_string))` で自分で裁定した**（機序まで一次証拠で確認）。現コーパスでは非発火ゆえ実測値は無傷。§3 冒頭の「逐語で移植」を訂正し、§6 (f) の期待挙動も `<NONE>` から「捨てる」へ直した |
| 2 | `activation.rs` の `//!` の引用行範囲「3〜15 行」が 2 行広い（実際は 3〜13） | **採用** | `sed -n '1,16p' \| cat -n` で自分で実測。14 行目から slash コマンドの話。§2 を行番号ではなく**見出し文で grep する**指示へ直した |
| 3 | 将来 inline `#[cfg(test)] mod tests { … }` を子モジュールへ書くと母集団へ混入する。曝露面が 1 枚 → 6 枚へ広がる | **採用（塞がずに宣言）** | `grep -n "cfg(test)" launcher_controller/*.rs` で今日 0 件（`activation.rs:634` は外部ファイル宣言）を自分で確認。**倒れ方を 3 通りへ分解し、沈黙するのは 1 形だけ**であることを §5.2 へ書いた。塞ぐ述語は道具立てが検査対象より複雑になるので採らない |
| 4 | `fs::read_to_string` の失敗経路（非 UTF-8 等）が §5.2 / §8 のどこにも無い | **採用** | §8 の疑問 1 を「失敗経路は 2 つ」へ拡張 |
| 5 | 変異 (d)〜(g) は Design B 未実装ゆえ論理検証のみで実測不能 | **前提として既出** | §6 冒頭が「設計を確定する前に書き下ろす」と位置づけており（`.claude/rules/safety-nets.md` の要求）、実装後の再実測は plan.md のフェーズへ置く |

### 壊せなかった項目（3b が検算方法つきで宣言）

needle の帰属表（3.3）／ヘッダ 31 本（独立 grep で二重検算・ファイル別内訳 11+4+3+4+7+2）／6 ファイルの行数・バイト数／`include_str!` の 4 サイト一覧（3.7）／`read_dir` 順序と改行コードの機体スコープ限定に過大な一般化が無いこと（`core.autocrlf=input` と `.gitattributes` が `.githooks/**` にしか `eol=lf` を掛けないことまで確認）／両 ADR の引用（却下理由・反転条件・doc drift の指摘とも原文一致）／issue #1201 本文との整合／§5.4 の親ファイル差分無しの推論／命題 (v)（helper 本体不変）。
