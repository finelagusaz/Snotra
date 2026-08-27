# plan: issue #1201 — ソーステキスト検査の母集団をディレクトリにする

ブランチ: `chore/source-text-probe-population-dir`
調査: `workspace/research.md`（敵対枠の所見と採否は §9）

## 目的

`launcher_controller/activation/tests.rs` の 3 本のソーステキスト検査の母集団を、`include_str!("../activation.rs")`（1 ファイル・コンパイル時）から `launcher_controller/` ディレクトリの子 `*.rs` 群（実行時）へ移す。

これにより「**起動の入口を `activation.rs` の外へ出さない**」という規範（`activation.rs` の `//!` が正本）を機構へ落とし、入口をどの子モジュールへ移しても検査が生き続けるようにする。

## 受け入れ条件

1. 3 本の検査（`activation_uses_frame_values_not_live_reads` / `activation_entry_points_consult_the_display_gate` / `on_enter_delegates_the_flush_decision_to_the_predicate`）が、`launcher_controller/` 直下の子 `*.rs` を母集団として動く
2. **入口を別の子モジュールへ移しても 3 本とも緑のまま**であり、移した先でゲートを落とすと赤になる（＝規範が機構になったことの実証）
3. **母集団が空になったら 3 本とも赤になる**（実測）
4. `method_body` / `method_header` / `owners_of` の**本体を 1 バイトも変えない**。合成 fixture テスト 11 本は逐語で残る
5. アンカーを含むファイルが**ちょうど 1 枚**であることを 3 本すべてが assert する（`read_dir` 順序への依存と、同名アンカーの取り違えを消す）
6. 規範の写し（`activation.rs` の `//!`・`tests.rs` の `//!`・各検査の doc）が新しい母集団と整合する
7. 変異注入 (a)〜(g)（`research.md` §6）を実測し、期待どおりの赤/緑になる

## 設計（`research.md` §5.2 の Design B）

**issue のスケッチ（子 `*.rs` を連結して 1 つの `src` にする）は採らない。** 連結は issue 自身が挙げた「測るべきこと」のうち 3 つ（境目の改行・`read_dir` の順序依存・ファイル境界を跨ぐ帰属の破れ）を自分で作り出す。**母集団を `Vec<(ファイル名, 中身)>` のまま配れば、3 つとも構造的に起こりえない。**

新設するのは 2 つの関数だけで、どちらも `activation/tests.rs` の中に閉じる（新ファイルを作らないので `ADR-source-text-probe-helper-locality` の却下理由 2 は不成立、反転条件にも該当しない）。

```
fn sources() -> Vec<(String, String)>
    // env!("CARGO_MANIFEST_DIR") + "/src/egui_shell/launcher_controller" を read_dir
    // → is_file() && ends_with(".rs") → ファイル名で sort → read_to_string
    // read_dir / DirEntry / read_to_string の失敗はすべて expect で panic（＝赤）

fn sole_file_with<'a>(sources: &'a [(String, String)], anchor: &str) -> &'a str
    // anchor を含むファイルを列挙し、ちょうど 1 枚であることを assert（0 枚も 2 枚以上も赤）
    // その中身を返す
```

呼び出し側の変形:

| 検査 | 変形 |
|---|---|
| `activation_entry_points_consult_the_display_gate` | `let src = sole_file_with(&sources, anchor);` を `targets` のループ内へ入れ、既存の `method_body(src, anchor, canary)` をそのまま呼ぶ |
| `on_enter_delegates_the_flush_decision_to_the_predicate` | 同上（アンカーは `"fn on_enter("` 1 本） |
| `activation_uses_frame_values_not_live_reads` | ヘッダ assert を `sole_file_with` で置き換え（一意性まで測る）。`owners_of` は**ファイルごとに呼んで `flat_map`** する |

**実装中に helper を「ついでに改良」しない。** 受け入れ条件 4 は、この変更の費用（`ADR-source-text-probe-helper-locality` 却下理由 1 の「稼働中のガードへの爆風」）を合成 fixture 11 本の外へ押し出すための唯一の梃子である。

## 不変条件と異常系

| 不変条件 | 守り |
|---|---|
| 母集団が空でも沈黙しない | `sole_file_with` の「ちょうど 1 枚」assert が 0 枚を赤にする。変異 (e) で実測 |
| `read_dir` の順序に結果が依存しない | ファイル名で `sort` ＋ `sole_file_with` の一意性 assert（「最初に見つかった方」への依存が構造的に無い） |
| 帰属がファイル境界を跨がない | `owners_of` をファイルごとに呼ぶ（`current` がファイルごとにリセットされる）。変異 (f) で実測 |
| 改行コード非依存 | 連結しないので境目が無い。各ファイルは `str::lines` を通る（`method_body` / `owners_of` の既存の性質が不変）。`fs::read_to_string` は `include_str!` と同じくバイト列を変換しない |
| I/O 失敗が沈黙しない | `read_dir` / `DirEntry` / `read_to_string` の 3 経路すべてを `expect` で panic（＝赤） |

**受容する死角（塞がない・doc へ宣言する）**:

1. **4 本目の入口の新設は沈黙する**（issue が明示的に射程外とした）。対象の正本が `entry_points` 配列であり母集団から導かれていないため
2. **inline `#[cfg(test)] mod tests { … }` の混入**（`research.md` §5.2）。曝露面が 1 枚 → 6 枚へ広がる。倒れ方 3 通りのうち沈黙するのは 1 形だけ
3. **入口を親 `launcher_controller.rs`（ディレクトリの外）へ戻す形は赤になる**。規範は消えるのではなく「1 ファイルへ集める」→「`launcher_controller/` の中に置く」へ**弱まる**
4. **`mod` 宣言の無い野良 `.rs`** はコンパイルされないまま母集団へ入る（誤爆＝赤方向。`governance:check` の `G-module-linkage` が別に捕まえる）

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `src-tauri/src/egui_shell/launcher_controller/activation/tests.rs` | `sources` / `sole_file_with` を新設。3 検査の母集団取得を差し替え。`//!` と 4 か所の doc を新しい母集団へ整合 |
| `src-tauri/src/egui_shell/launcher_controller/activation.rs` | `//!` の規範 2 段落（見出し文 `**起動の入口をこのファイルの外へ出してはならない**` で grep）を、機構化された旨と残る死角へ書き換え |
| `docs/development-principles.md`「検証の層と、層と層の隙間」 | 母集団の写しを持つ 3 文を整合（下表） |
| `src-tauri/CLAUDE.md` 41 行 / 47 行 | **41**: 「**起動の入口をこのファイルの外へ出さないこと**——ソーステキスト検査の母集団がここに縛られている」＝**降格される規範そのもの**。**47**: 「`include_str!("../activation.rs")` が母集団であり」＝機構名が変わる。どちらも見出し文で grep して直す |

**`SPEC.md` は更新しない**——挙動（仕様）を変えず、検査の母集団だけを変えるため。

**ADR は追記も修正もしない**——凍結された歴史（`AGENTS.md`「意思決定記録」）。`ADR-source-text-probe-helper-locality` の決定は不変で反転条件にも該当せず、`ADR-source-text-probes-not-lifted-to-types` は射程外。**両 ADR が当該サイトを分割前の `launcher_controller.rs` と呼ぶ doc drift（#1200 由来）もそのまま置く。**

### `docs/development-principles.md` で偽になる文（生きた層ゆえ直す）

| 箇所 | 何が偽になるか | 直し方 |
|---|---|---|
| 「母集団を持つ検査では、母集団そのものが壊れる」の定義文 | ソーステキスト検査を「`include_str!` で自分のソースを読み」と定義している。1 サイトが `read_dir` になる | 定義を機構名から外す（「自分のソースを読み」で足りる） |
| **「ゆえに否定形からは切り出しを取り除ける」の段落**（最も効いている） | 「守りたい本体が `include_str!` の読む**当のファイル**に在る自己検査なら、**ファイル全体**は B の上位集合である——（本体が別ファイルへ移りうる検査ではこの前提が外れる）」。**この括弧書きの但し書きが、まさに今回の変更で生きる**。下界を構成で満たすのは「ファイル全体」ではなく「母集団＝ディレクトリの子 `*.rs` 全体」になる | 「当のファイル」→「母集団に入るファイル群」、「ファイル全体」→「母集団全体」。**但し書きは消さずに、外れる条件を「本体が母集団の外へ移りうる検査では」へ書き換える**（独立導出 M4 は「但し書きが強すぎる」＝解けたと読んだが、**解けたのは『別ファイル』の版だけで、母集団の外へ移る形では依然として外れる**。所見は採り、下限を保つ形で直す） |
| 「切り出しを持つ形は checkout の改行コードに依存する」の段落 | 「`include_str!` が読むのは checkout された実ファイルなので」 | `fs::read_to_string` も同じく checkout された実ファイルを読むので**主張の実質は変わらない**。機構名だけ両対応へ直す |

「残余の内訳と反証の実測は `…/activation/tests.rs` の `owners_of` の doc を正本とする」の行は**パスも正本も変わらないので触らない**。

## doc 整合の内訳（`research.md` §2 の写し一覧）

| 場所 | 何が偽になるか |
|---|---|
| `activation.rs` の `//!` 当該 2 段落 | 「このファイルの外へ出してはならない」が不要になる。「移動は赤になり、追加は沈黙する」の**移動の側**が消え、追加の側だけが残る |
| `tests.rs` の `//!` 3〜5 行 | 「母集団は `activation.rs` 1 枚である」 |
| `activation_uses_frame_values_not_live_reads` の doc | (1) 「`search_flow.rs` の `run_search_with` の live-read はこの検査はもう見ていない」が**偽になる**（母集団へ復帰し、帰属で緑になる）。(2) 「母集団は production 1 枚で、この `mod tests` は別ファイルだから母集団の外」→ **`activation/` がサブディレクトリで `read_dir` が非再帰**であることが新しい構造的根拠。(3) 末尾「母集団は `activation.rs` 1 枚である」 |
| `method_header_requires_exactly_four_spaces_of_indent` の doc | 「`include_str!` が読む `activation.rs` には字下げ 0 / 8 の `fn ` 行が 1 本も無く」→ **ディレクトリ 6 枚でも 0 本**（`research.md` §3.5 で再測済み）。射程を書き換えれば主張は成立する |
| `method_body` / `method_body_is_line_ending_agnostic` の doc | 「`include_str!` は checkout された実ファイルを読む」→ `fs::read_to_string` へ。**CRLF 非依存の根拠は変わらない** |

## 実装順序

### Phase 1 — 母集団の差し替え（挙動を変えない）

- [x] `sources()` / `sole_file_with()` を `activation/tests.rs` へ新設する（helper 3 本には触れない）
- [x] `activation_entry_points_consult_the_display_gate` を `sole_file_with` 経由へ差し替える
- [x] `on_enter_delegates_the_flush_decision_to_the_predicate` を `sole_file_with` 経由へ差し替える
- [x] `activation_uses_frame_values_not_live_reads` を、`sole_file_with` の一意性 assert ＋ ファイルごとの `owners_of` の `flat_map` へ差し替える
- [x] `cargo test -p snotra` が緑であることを確認する（3.3 の帰属表どおり、入口へ帰属する needle は 0 件のはず）

### Phase 2 — 変異注入の実測（`.claude/rules/safety-nets.md`）

**稼働中のガードは弱めない**——変異は作業ツリーへ一時的に当て、測ったら必ず戻す（コミットしない）。

> **変異 (e) は `sources()` 自身を壊す形だが、「稼働中のガードを弱めない」に違反しない。** 壊すのは**まだ構築中の版**であって、main に在る稼働中のガードは依然 `include_str!` の形である（`.claude/rules/safety-nets.md`「複製に変異を当てる」の趣旨は満たされる——複製を作る代わりに、まだ稼働していない実装を測っている）。

再実測（既存の守りが同じ強さで残ること）:

- [ ] (a) `on_enter` の本体へ `self.indexing()` を 1 行挿す → **赤**
- [ ] (b) `activate_or_execute` から `plain_results_hidden(` の呼び出しを消す → **赤**
- [ ] (c) `on_enter` から `if crate::egui_shell::should_flush_on_enter(` の行を消す → **赤**

新機構の証明（ディレクトリ化で初めて生きる枝）:

- [ ] (d) `shift_activate` を丸ごと `folder_nav.rs` へ移す → **3 本とも緑**。続けて移した先で `results_area_collapsed(` を落とす → **赤**
- [ ] (e) 母集団を空にする（`sources()` のフィルタを一時的に不成立にする）→ **3 本とも赤**
- [ ] (f) 2 枚目のファイルの最初のヘッダより前へ `read_config(` を置く → **緑**（`owners_of` が帰属先の無い出現を捨てる。`fn on_enter(` へ跨いで帰属しないことの実測）
- [ ] (g) 2 枚のファイルへ同じアンカー（`fn shift_activate(`）を置く → **`sole_file_with` が赤**
- [ ] 各変異について「本来の回帰と同じ強さか」を 1 行で判定し、結果を PR 本文へ書ける形でメモする

### Phase 3 — doc 整合

- [ ] `activation.rs` の `//!` の規範 2 段落を書き換える（残る死角＝4 本目の入口の新設は**残す**）
- [ ] `tests.rs` の `//!` を新しい母集団へ書き換える
- [ ] **今日すでに偽の 2 か所を直す**（独立導出 P4 / P8。自分で実測して確認した）: `method_header_accepts_visibility_and_async_before_fn` の doc（295〜297）と `activation_uses_frame_values_not_live_reads` の内部コメント（424〜425）が「起動の入口に `pub(super) ` が挟まる」「2 形は `pub(super) fn` / 素の `fn`」と書くが、**実際の入口は `pub(in crate::egui_shell) fn`（`activation.rs:397,589`）と素の `fn`（:472）**で、`pub(super)` を持つのは入口ではない `drain_launch`(:247) / `execute_slash`(:294) である。**機構（2 形に分かれるのでヘッダ assert が読み飛ばしの破損を捕まえる）は真のまま、名指しだけが誤っている**
- [ ] **`method_header_accepts_visibility_and_async_before_fn` の fixture へ `pub(in crate::egui_shell) fn` を 1 本足す**（独立導出 M2）——上の訂正で「load-bearing な形が fixture に無い」ことが露見したため。**helper の本体は変えない**（受け入れ条件 4 は維持される。増えるのは fixture の行だけ）。**これは issue #1201 の射程からの拡張なので、人間レビューで明示的に承認を得る**
- [ ] `activation_uses_frame_values_not_live_reads` の doc の 3 点を書き換える（`search_flow.rs` の live-read が母集団へ復帰したこと・`read_dir` 非再帰という新しい構造的根拠・末尾の「1 枚」）
- [ ] `method_header_requires_exactly_four_spaces_of_indent` の doc の射程をディレクトリへ書き換える
- [ ] `method_body` / `method_body_is_line_ending_agnostic` の doc の `include_str!` 言及を `fs::read_to_string` へ改め、CRLF 非依存の根拠が変わらないことを明記する
- [ ] 受容する死角 2（inline `#[cfg(test)]` の曝露面拡大）を `sources()` の doc へ宣言する
- [ ] **母集団を散文で名乗るとき、「この母集団に入らないもの」を同時に名指す**（`docs/development-principles.md`「列挙の完全性」——名指せないなら測っていない）。名指すのは **`activation/tests.rs`（サブディレクトリ・`read_dir` は非再帰）** と **親 `launcher_controller.rs`（ディレクトリの外）** の 2 つ
- [ ] **再帰化したときの倒れ方が非対称であることを `sources()` の doc へ書く**（独立導出 R4）——将来 `read_dir` を再帰へ広げると、`method_body` 側は `tests.rs` の字下げ 8 のアンカー文字列で字下げ assert が落ちて**赤**（安全側）、`owners_of` 側は `tests.rs` の `fn` が字下げ 0 でヘッダを持たずリテラル出現が**黙って捨てられて緑**になる。**片側だけが気づけるので、非再帰であることは意図であって偶然ではないと書く**
- [ ] `src-tauri/CLAUDE.md` の 2 行（モジュール索引の `activation.rs` 行と「ソーステキスト検査は対象モジュールの子として置く」行）を整合させる。**41 行の「このファイルの外へ出さないこと」は降格される規範そのもの**なので、`launcher_controller/` の中に置くこと＋残る死角（4 本目の新設）へ書き換える
- [ ] `docs/development-principles.md`「検証の層と、層と層の隙間」の 3 文を上表のとおり整合させる（**「本体が別ファイルへ移りうる検査ではこの前提が外れる」の但し書きを消さない**——今回の変更はこの但し書きを**発火させる**側であり、下界の根拠がファイル単位から母集団単位へ移ることを書く）
- [ ] **全称表現を検算する**——書いた doc の各断定について「何が増えたら偽になるか」を 1 つ挙げ、挙がったら下限の主張へ弱める

### Phase 4 — 検証

- [ ] `cargo test -p snotra` 緑
- [ ] `cargo clippy --all-targets -- -D warnings` 緑（`docs/build-commands.md` カテゴリ A）
- [ ] `cargo fmt --check` 緑
- [ ] **`cargo doc` を手で走らせる**（`docs/build-commands.md` カテゴリ A）——**intra-doc link 切れは CI でのみ発火し PostToolUse hook は沈黙する**（`partial-automation-habituates` の実測）。`tests.rs` の doc は `[`method_body`]` 等の intra-doc link が密で、今回そこを大量に書き換える
- [ ] `npm run governance:check` 緑（`.rs` の見出し参照と `src-tauri/CLAUDE.md` を触るため・カテゴリ F）
- [ ] **撤去の語彙を数え上げる**（`AGENTS.md`「機構・層・ファイル群を撤去する」）——規範 1 本を撤去するので、`git grep` で「外へ出」「1 枚」「母集団がここに縛られている」「移動は赤・追加は沈黙」「`include_str!("../activation.rs")`」を数え、残った出現を「撤去を描写している / 撤去されたものが在る前提で書いている」へ振り分ける。**識別子の残存 0 件を根拠にしない**（散文の語彙が射程外に落ちる）
- [ ] 実装差分を確定させる

## テスト方針と検証コマンド

コマンドの正本は `docs/build-commands.md`。この計画で使うのはカテゴリ A（Rust）とカテゴリ F（ガバナンス）。

**新しいテストは足さない。** `sources()` / `sole_file_with()` の正しさは合成 fixture ではなく**変異注入 (e)(f)(g)** で測る——母集団の取り方は実ディレクトリに対してしか意味を持たず、合成 fixture では「fixture を読めること」しか測れない。

## 未確定（実装前に潰す）

すべて解消済み。判断と根拠は下に残す。

- [x] **`sources()` の失敗経路の扱い** — **決定: 3 経路（`read_dir` / `DirEntry` / `read_to_string`）すべてを `expect` で panic ＝赤へ倒す。** どれも「母集団が取れていない」を意味し、沈黙すれば検査が空虚に緑になる。文言は既存の assert の様式（何が起きたか＋なぜ沈黙が危険か）に合わせて Phase 1 で書く——**文言は実装の細部であって未確定の判断ではない**。3 経路目（`read_to_string`）は 3b の ⚠️4 が指摘した追加分
- [x] **`activation.rs` の `//!` の当該段落の扱い** — **決定: 丸ごと削らず、書き換える。** 移動の側の制約（「このファイルの外へ出してはならない」）だけを消し、**「4 本目の入口の新設は沈黙する」という受容残余は残す**（issue が射程外と明示。消すと死角が文書のどこにも無くなる）。同じ判断を `src-tauri/CLAUDE.md`:41 にも当てる
- [x] **変異 (d) の移し先** — **決定: `folder_nav.rs`。** 6 枚すべてが `impl LauncherController` ブロックを 1 つずつ持つことを実測した（`grep -c "^impl LauncherController"` が 6 ファイルとも 1）。ゆえに「移せる子モジュールが 1 つも無い」は起こりえず、必要な `use` は移動先へ足せる。第一候補で通らなければ別の子へ替えるが、**それは実装中の手当てであって設計の分岐ではない**
- [x] **母集団の下限 assert（「2 枚以上」等）** — **決定: 置かない。** 数え上げは足すたびに腐る（`AGENTS.md`「検証の作法」——正本は分岐そのものであって数ではない）。代わりに `sole_file_with` の「ちょうど 1 枚」assert が空母集団を赤にする。**この代替が効くことは変異 (e) で実測する**（Phase 2 の作業項目。赤にならない検査が 1 本でもあれば、そのときはその検査にだけ下限を置く——判定は測ってから）

## plan-review 結果

- **リスク: 高**（`/plan-review`「リスク判定」の「hook、CI、rules、skills、ガバナンス文書を変更する」に当たる。加えて issue #1201 自身が「検査（セーフティネット）の設計変更であり、ルート `CLAUDE.md` 最重要ルール 2 に当たる」と宣言している）
- **レビュー方式: 独立導出 1 体**（Step 2b。issue の WHAT だけを渡し、`workspace/` を読ませずコードと規範から独立に導出させた。走査範囲も指定して `workspace/` の内容が tool result 経由で混入するのを防いだ）
- **エージェント数: 2**（3b の敵対枠 1 体 + 本レビュー 1 体）
- 成果物: `workspace/plan-review-population-dir.md`

### 要対処 — 反映内容

| # | 所見 | 反映 |
|---|---|---|
| **R1** | 母集団を `mod` 宣言と機械照合しないと、新しい子モジュールが黙って母集団の外に落ちる。「壊れたとき緑が緑のまま推移する」ので機構を置くべき | **不採用（機序を一次証拠で裁定した）。** この所見は「`include_str!` はパスを計算できないので母集団は**リテラルの配列**にならざるをえない」という前提の上に立つが、**Design B は `read_dir` で母集団をファイルシステムから導く**ので前提が成立しない。子モジュールを足せば `mod` 宣言を書かなくても自動で母集団へ入る。**ずれの向きも逆**——`mod` 宣言のある `.rs` の欠落は起こりえず（コンパイルエラー）、`mod` 宣言の無い `.rs` の混入は誤爆＝赤方向で `G-module-linkage` が別に捕まえる。**採るのは所見であって添えられた機序ではない**（`AGENTS.md`「レビューの委譲」） |
| **R2** | 連結すると `owners_of` の帰属がファイル境界を越える。`activation.rs` の最後のヘッダが `fn on_enter(` なので、隣接ファイルの module doc 1 行で恒久的な偽陽性が出る | **採用済み**（Design B が既に「ファイル単位で `owners_of` を回す」形。**独立導出が同じ危険を独立に発見した**——`activation.rs` の最後のヘッダが `fn on_enter(` である点まで一致） |
| **R3** | 連結すると `method_body` の `split_once` が並び順依存になり、doc コメント経由のアンカー横取りがファイルをまたいで効く。「ちょうど 1 枚が含む」assert が要る | **採用済み**（`sole_file_with` がまさにそれ。**これも独立に一致した**） |
| **R4** | 母集団を「直下のみ」に限ることが `activation/tests.rs` を母集団外に保つ唯一の根拠になる。理由とセットで doc へ書く。**再帰へ広げると片側が赤・片側が緑で気づけない** | **採用済み＋強化**。Phase 3 の「母集団に入らないものを名指す」項目がこれ。**再帰化したときの倒れ方が非対称である**という指摘を `sources()` の doc へ書き足す |
| **R5** | フォールトインジェクションは (a) 既存発火の生存だけでなく **(b) 入口を兄弟ファイルへ移した状態での発火**を必ず測る | **採用済み**（Phase 2 の変異 (d)。「移しただけでは緑・移した先でゲートを落とすと赤」の対照まで一致） |
| **R6** | 偽になる散文 P1〜P13 を漏れなく直す | **採用**。P1〜P3・P5〜P7 は Phase 3 に既出、P9〜P12 も既出。**P4 / P8（`pub(super)` の名指しが今日すでに偽）は新規発見**で Phase 3 へ追加した（自分で `activation.rs:247,294,397,472,589` を読んで確認済み）。P13 は下の M4 |

### 軽微 — 反映内容

| # | 所見 | 反映 |
|---|---|---|
| M1 | `tests.rs`:297-298 / :424-425 の `pub(super)` 名指しが今日すでに偽 | **採用**（R6 と同件。Phase 3 へ） |
| M2 | `method_header_accepts_visibility_and_async_before_fn` に `pub(in crate::egui_shell)` の fixture が無い | **採用（issue の射程からの拡張として、人間の承認にかける）**。M1 の訂正で「load-bearing な形が fixture に無い」ことが露見したため、訂正と同じ差分で閉じるのが自然。**helper 本体は変えないので受け入れ条件 4 は維持される** |
| M3 | 3 検査の母集団取得の写しを 1 つの関数へ寄せる。ただし `owners_of` / `method_body` は統合しない | **採用済み**（`sources()` が 1 か所。helper 統合を禁じる点も設計と一致） |
| M4 | `docs/development-principles.md`:213 の但し書き「本体が別ファイルへ移りうる検査ではこの前提が外れる」は、**外れない構成が可能になった**ので強すぎる | **採用（ただし書き換え方を変える）**。但し書きは**消さない**——下界の根拠が「ファイル全体」から「母集団全体」へ移るだけで、**母集団の外へ移りうる検査では依然として外れる**。解けた実例がある旨を添えて射程を書き直す |
| M5 | `tests.rs`:255-260 の「字下げ 0 / 8 の `fn ` 行が 1 本も無い」は母集団 6 枚でも成立する | **採用済み**（Phase 3。`research.md` §3.5 と独立導出の 2 回、別々に再測した） |

### 未検証 — 潰した結果

| # | 所見 | 結果 |
|---|---|---|
| U1 | `scripts/` 配下にこの母集団を名指す記述が無いか未走査 | **潰した。** `scripts/` を `activation\.rs\|activation/tests\|include_str!\("\.\./\|launcher_controller` で走査し、ヒット 2 件はいずれも無関係（`manual-smoke.ps1:13` のバイナリ目印文字列と `G-folded-code-spans.mjs:63` の死角の例示） |
| U2 | PR 本文は数え上げの母集団に入らない | **PR 作成時の項目として送る**（計画では閉じられない。`pr-body-is-outside-the-grep-population`） |
| U3 | `mod` 宣言スキャナが ADR の反転条件に当たるかの裁定 | **消滅。** R1 不採用でスキャナ自体を置かない |
| U4 | `include_str!("../../launcher_controller.rs")` の相対解決が未実測 | **消滅。** Design B はそのパスを使わない |
| U5 | 「直下の子 `*.rs` 群」が実装時点でも 6 枚である保証は無い | **消滅。** `read_dir` が実行時に列挙するので枚数は結果であって前提ではない。**計画のどこにも「6 枚」を不変条件として書かない** |

### 判断

- **実装着手: 可**（未確定欄を潰し、人間の承認を得たあと）

## セルフレビュー

Step 1 の 5 点を主エージェント自身で照合した。

1. **issue の全要件に作業項目が対応する** — 母集団のディレクトリ化（Phase 1）／規範の降格（Phase 3）／issue が要求した 4 つの実測（Phase 2 の (e)(f) と設計で消した 2 つ）。issue が射程外と宣言した「4 本目の入口の新設」は**死角として明示的に残す**
2. **境界条件と検証の対応** — 空母集団 (e)／ファイル境界の帰属 (f)／アンカー重複 (g)／改行コード（連結を作らないので境目が無い）／`read_dir` 順序（`sort` ＋一意性 assert）
3. **新しい状態・リソースの正常/失敗経路** — `sources()` の I/O は 3 経路すべて `expect` で赤へ倒す（未確定欄 1）
4. **より単純な既存パターンで置き換えられないか** — issue のスケッチ（連結）の方が短いが、**issue 自身が挙げた測るべき 4 点のうち 3 点を自分で作り出す**ので採らない。判断の根拠は `research.md` §5.1／§5.2 の表
5. **壊してはならない不変条件に検知手段がある** — 変異 (a)(b)(c) が既存 3 検査の発火を、(d) が新機構を、(e)(f)(g) が母集団の取り方を測る

**条件別チェックの振り分け**: `.claude/rules/safety-nets.md`（手動参照・実施済み）／`docs/development-principles.md`「検証の層と、層と層の隙間」（読み込み、`docs` 側の doc-sync 3 文を発見）／`npm run governance:check`（Phase 4）。`/symmetric-check`・`/state-check`・`/persistence-check`・`/race-check` はいずれのトリガーにも当たらない。

**未検証**: 変異 (d)〜(g) は Design B 実装前ゆえ論理検証のみ（3b の ⚠️5）。Phase 2 で実測する。

## 人間レビュー

- [x] 承認済み — 2026-08-27 / 問い: "`workspace/plan.md` の計画を承認し、`/implement` で実装へ進んでよろしいでしょうか（射程拡張 2 も含めて）。" / 回答: "OK"

承認に含まれるもの（問いで名指しした 3 点）:

1. **issue のスケッチからの逸脱** — 連結せず母集団を `Vec<(名前, 中身)>` のまま配る（Design B）
2. **射程の拡張** — `method_header_accepts_visibility_and_async_before_fn` の fixture へ `pub(in crate::egui_shell)` を 1 本追加する
3. **セーフティネットの変更への合意** — ルート `CLAUDE.md`「最重要ルール」2
