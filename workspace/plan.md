# 実装計画 — #1108 `indexing.rs` の検知器は、終端の錨が消えると沈黙して緑になる

調査は `workspace/research.md`。

## 目的

`src-tauri/src/indexing.rs` の `start_index_build_invalidates_the_icon_cache` の母集団を、**終端の錨（後続の `pub(crate) fn`）に依存しない形**へ変え、**終端が見つからなければ赤くする**。現状は錨が 1 つしか無く（`build_index_from_material`）、それが改名・移動・可視性変更・撤去のいずれかで消えると母集団が EOF まで伸び、探す 2 文字列が同じ `mod tests` に実在するため**検査が沈黙して緑になる**。

## 受け入れ条件

1. `build_index_from_material` の可視性・位置・名前を変えても、`start_index_build_invalidates_the_icon_cache` の判定が変わらない（母集団が錨に依存しない）
2. 終端が見つからなければ **assert が赤くなる**（EOF まで伸びた母集団で沈黙しない）
3. 切り出しが **LF / CRLF の両方**で同じ結果を返すことを、作業ツリーの改行コードに依存しない fixture（文字列リテラル）で固定する
4. `invalidate_icon_cache(` の呼び出しを `start_index_build` から削除したとき、**修正後の検査が赤くなる**（検知力が保たれている）
5. 挙動は 1 バイトも変わらない（`#[cfg(test)]` の中だけを触る）

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `src-tauri/src/indexing.rs` | `mod tests` に helper `top_level_fn_body` を新設。`start_index_build_invalidates_the_icon_cache` をそれ経由へ。fixture テスト `top_level_fn_body_is_line_ending_agnostic` を新設。既存テストの doc「残る死角」に母集団の新しい壊れ方を 1 段落 |

**これ 1 ファイルで閉じる。** production コードは触らない。`launcher_controller.rs` の `method_body` は触らない（下記「(B) 共通化を却下した理由」）。

## 実装（確定形）

`mod tests` の先頭へ:

```rust
    /// トップレベル関数の本体を切り出す（**終端は列 0 の閉じ括弧**・内側のブロックは字下げされている）。
    ///
    /// 母集団は狭すぎても広すぎても壊れ、**広すぎる側は沈黙する**——探す文字列はこの `mod tests` に
    /// assert のリテラルとして実在するため、EOF まで伸びた母集団では `contains` が必ず真になる。
    /// ゆえに終端が見つかったことと canary が在ることの**両方**を assert する。行の走査を
    /// `str::lines` で行うのは改行コード非依存にするためである。**原則の正本は
    /// `docs/development-principles.md`「検証の層と、層と層の隙間」**（ここに写しを置かない）。
    ///
    /// `launcher_controller.rs` の `method_body` とは**終端の形が違う**（あちらはメソッドゆえ
    /// 4 スペース字下げの `}`）。共通化しない判断は #1108 の PR 本文に記録した。
    fn top_level_fn_body(src: &str, anchor: &str, canary: &str) -> String {
        let after = src
            .split_once(anchor)
            .unwrap_or_else(|| panic!("{anchor} が見つからない（改名したらこの検査も直す）"))
            .1;
        let mut body = String::new();
        let mut terminated = false;
        for line in after.lines() {
            if line == "}" {
                terminated = true;
                break;
            }
            body.push_str(line);
            body.push('\n');
        }
        assert!(
            terminated,
            "{anchor} の終端（列 0 の `}}`）が見つからない——母集団が EOF まで伸びており、\
             この検査は空虚である"
        );
        assert!(
            body.contains(canary),
            "母集団が {anchor} の本体を含まない——終端の切り出しがずれた。\
             沈黙する検知器は検知器ではない"
        );
        body
    }
```

fixture テスト（**文字列リテラルで書く**——`include_str!` を使うと作業ツリーの改行コードに依存し、手元 LF では CRLF 側を一度も測らない）:

```rust
    #[test]
    fn top_level_fn_body_is_line_ending_agnostic() {
        let lf = "pub fn target() {\n    marker();\n}\npub fn next() {\n";
        let crlf = lf.replace('\n', "\r\n");
        for (label, src) in [("LF", lf), ("CRLF", crlf.as_str())] {
            let body = top_level_fn_body(src, "pub fn target(", "marker(");
            assert!(
                !body.contains("fn next("),
                "{label}: 終端を取り逃して次の関数まで飲み込んでいる"
            );
        }
    }
```

既存テストは母集団の取得だけを差し替える（2 つの `assert!` はそのまま。canary は `try_begin_index_build(`）:

```rust
        let body = top_level_fn_body(
            include_str!("indexing.rs"),
            "pub fn start_index_build(",
            "try_begin_index_build(",
        );
        assert!(body.contains("invalidate_icon_cache("), ...);
```

## 不変条件と異常系

- **母集団は「`pub fn start_index_build(` から列 0 の `}` まで」であり、他の関数の名前・位置・可視性に依存しない**
- 錨（`start_index_build` の名前）が変われば `panic!` で赤（改名者に届く・安全側）
- **列 0 の `}` が本体内に現れれば母集団は狭くなるが、どこで切れても赤になる**（3b が raw string でこの形を構成し、機序を `scratchpad/narrow_probe.rs` で実測した。詳細は `research.md`「敵対的調査（3b）の所見と採否」）
  - canary（`try_begin_index_build(`・28 行）より**前**で切れる → canary の assert が赤
  - canary より**後**・`invalidate_icon_cache(`（45 行）より**前**で切れる → `invalidate_icon_cache` の assert が赤
  - **この安全性は「存在形の assert」に依る**——`!body.contains(...)` の否定形なら、canary が真のまま対象だけが切り捨てられて**沈黙する**（`launcher_controller.rs` の `activation_uses_frame_values_not_live_reads` がその形。本件の射程外につき follow-up 候補として 5c で問う）
  - 現在の本体（22〜103 行）に列 0 の `}` は終端の 1 つしか無いことを実測済み
- **残る死角は変わらない**: 母集団は `start_index_build` のソーステキストだけであり、呼び出しグラフは辿らない（既存 doc の記述をそのまま維持する）

## テスト方針と検証コマンド

**Red は二段変異で測る。** 錨だけを消しても「母集団が広がった」ことしか示せず、**空虚さ**（不変条件が壊れているのに緑）の証明にならない。

| # | 変異 | 現行の検査 | 修正後の検査 |
|---|---|---|---|
| M1a | `invalidate_icon_cache` の呼び出しを `notify_indexing_started` へ**移す**（錨は健在） | **赤**（＝この回帰は本来捕まる） | 赤 |
| M1b | M1a に加えて錨を消す（`build_index_from_material` の `pub(crate)` → `pub`） | **緑（＝空虚さの実証）** | **赤** |
| M2 | 錨だけ消す | 緑 | **緑**（母集団が錨に依存しなくなった証拠。`expect(錨)` 案なら赤くなり、無関係変更でノイズを出す） |
| M3 | 変異なし（クリーンツリー） | 緑 | 緑 |
| M4 | 本体へ列 0 の `}` を含む raw string を仕込む（**終端の偽装**・3b の指摘） | — | **赤**（母集団が狭まり、切れた位置に応じて canary か `invalidate_icon_cache` の assert が発火する。**沈黙しないことの実測**） |

**M1 の変異を「呼び出しの削除」から「別関数への移動」へ組み替えた**（実装中の発見・2026-08-17）。削除では `invalidate_icon_cache` が never used になり、**`-D warnings` の `dead_code` が検査より手前で発火する**——`.claude/rules/safety-nets.md`「注入したことと、注入が正しい強さであることは別である」に当たる強すぎる変異だった。移動なら関数は使われ続けるので `dead_code` は黙り、**壊れるのは「`start_index_build` の本体で無効化する」という不変条件だけ**になる。

**副産物**: `invalidate_icon_cache` の呼び出し点は現在ここが**唯一**であり、単純な削除は clippy が二重に守っている（`cargo clippy` が `icon.rs:191` を name で指す）。この検査が単独で守るのは「呼び出しが他所へ移る」形である。

コマンド: `cargo test -p snotra start_index_build_invalidates` / `cargo test -p snotra top_level_fn_body`（`src-tauri` は `[lib]` を持たないため `--lib` は使えない・`src-tauri/CLAUDE.md`）。

**変異は稼働中のガードを弱めるので、必ず `git diff` で戻したことを確認してから次へ進む**（`.claude/rules/safety-nets.md`「複製に変異を当てる」の趣旨。ここでは in-tree で当てるため、各変異の直後に `git checkout -- src-tauri/src/indexing.rs` で戻し `git status --short` が空であることを見る）。

変更後の検証は `docs/build-commands.md` カテゴリ A 全件（fmt / check / clippy / test / doc）。`.rs` のみの変更ゆえ B〜F は非該当（`workspace/*.md` はガバナンス文書ではないが、`governance:check` は安いので 1 回走らせる）。

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要。** `#[cfg(test)]` の中だけを触り、製品の挙動・フロー・状態遷移を 1 つも変えない。**実測**: `grep -c "invalidate_icon_cache\|start_index_build" SPEC.md` = **0**（2026-08-17）
- **`docs/development-principles.md`: 不要。** 149〜183 行は `indexing.rs` を名指ししておらず、この修正で偽になる記述を持たない。183 行「ゆえに切り出しの helper は 1 つに閉じる」との関係は下記に記録し、規範そのものを書き換えるかは 5c で人間へ提案する（規範の編集はそれ自体が別のセーフティネット変更であり、射程の書き直しは偽の全称を再生産しやすい・#1091）
- **`src-tauri/CLAUDE.md`: 不要。** ファイルの追加・削除が無く、`indexing.rs` の節が持つ不変条件は変わらない

## (B) 共通化を却下した理由（反転条件つき）

**却下した案**: `method_body` を終端の形で引数化し、crate 共通の `#[cfg(test)]` helper（新ファイル）へ抽出して `indexing.rs` と `launcher_controller.rs` が共有する。

**採らなかった理由**:

1. ~~**終端が別概念である。** crate 全域で「1 つに閉じる」は達成不能であり、B を採っても実装は 3 つ残る~~ — **この論証は誤りであり、レビューで覆した（2026-08-17）。** `view.rs`（production 部を切る）と `startup.rs`（enum 宣言の走査）は**元より統合の候補ではなく、候補でないものを数えて「減らない」と言っていた**。統合しうるのは 2 本で、差は終端リテラルだけである（構造・assert・メッセージまで一致）。**達成可能な削減は 2 → 1 であり、規範が対象とするのはまさにその 2 本である**
2. **稼働中のガードへ爆風が及ぶ。** `method_body` を一般化すれば、それに依存する 2 検査の変異注入を再実測する義務が生じる（`AGENTS.md`「レビュー指摘へ修正を当てた」の再実行則）。issue #1108 が合意を求めた射程は `indexing.rs` である
3. **新ファイルは統治コストを持つ**（`mod` 宣言 + `CLAUDE.md` のモジュール索引 + `G-module-linkage`）
4. issue 本文は `method_body` を「**実装例**」と呼び、抽出・共有を求めていない

**緊張の逐語記録**: `docs/development-principles.md` 183 行は「**ゆえに切り出しの helper は 1 つに閉じる。** 検査ごとに書き写すと、上の 2 方向の壊れ方が**検査ごとに独立に起きる**」と書く。本計画はこれに反して 3 つ目の実装を作る。**ただしこの規範の実証はファイル内の写しである**——由来は `db7f77df`（#1106 サイクルの振り返り・#1111）で、続く文が名指すのは「#1077 で 2 本が別々の状態になった」（同 PR #1107 の変更ファイルに `indexing.rs` は無く、2 本とも `launcher_controller.rs` の中）と「#1106 は 2 本目を書くときに写し、`/simplify` の指摘で 1 つへ畳んだ」（`view.rs` の中）である。本計画は「1 ファイル内では 1 つに閉じる」（`indexing.rs` の検査 2 本が同じ helper を通る）を満たす。

**反転条件**: 次にソーステキスト検査を**新設する** issue が立ったら（＝3 つ目の局所実装が生まれるなら）、そこで共通化を再検討する。

**この判断は `docs/adr/ADR-source-text-probe-helper-locality.md` へ回収した**（2026-08-17）——`plan.md` はサイクル末に撤去されるため、却下理由と反転条件がここにしか無い状態を残さない。

**レビュー委譲時の申し送り**（`/dry-check`・`code-reviewer` へ先渡しする）: `top_level_fn_body` と `method_body` の重複は**根拠つきで意図的に分けた構造**である。根拠の所在は本節と、両 helper の doc コメント。DRY 違反として挙げないこと。

## フェーズと作業項目

### Phase 1 — 欠陥の実証（Red）

- [x] M1b（移動 + 錨消し）で `cargo test -p snotra start_index_build_invalidates` が**緑のまま**であることを実測 — `test indexing::tests::start_index_build_invalidates_the_icon_cache ... ok` / `1 passed` / exit 0
- [x] **緑になった理由まで一致を確認した** — `scratchpad/population_probe.rs` を `rustc -O` で実ファイルへ当てた実測:
  - 母集団 = **12117 バイト / after 全体 = 12117 バイト**（`終端を取り逃した（EOF まで伸びた）= true`）
  - 偽陽性の源は予告どおり `mod tests` の assert リテラル（**215 行 `body.contains("try_begin_index_build(")` / 220 行 `body.contains("invalidate_icon_cache(")`**）。加えて、伸びた母集団が**移動先の実コード（165 行）まで飲み込んでいた**
- [x] **対照（M1a）で赤になることを実測** — 錨だけ戻すと同じ回帰が捕まる: `start_index_build がアイコンキャッシュを無効化していない……` / `1 failed`。**「錨が消えると同じ回帰を見逃す」が対照つきで確定した**
- [x] 変異を戻し、`git status --short` と `git diff --stat` がともに空であることを確認

### Phase 2 — 修正

- [x] **TDD: fixture テストの検知力を先に実証した** — 素朴な `find("\n}\n")` 実装へ fixture を当て、**CRLF ケースが `pub fn target( の終端が見つからない` で落ちる**ことを実測（`1 failed`）。そのうえで `lines()` 版へ直して緑にした
- [x] `top_level_fn_body` を追加（`src-tauri/src/indexing.rs:200`）
- [x] `start_index_build_invalidates_the_icon_cache` の母集団取得を helper 経由へ差し替え（同 `:267`）
- [x] `top_level_fn_body_is_line_ending_agnostic` を追加（同 `:232`）
- [x] 既存テストの doc に母集団の壊れ方を書き足した。**加えて「残る死角」の記述を Phase 1 の実測に合わせて訂正した**——旧文は「この関数の外のヘルパー経由で無効化する形へ変えると、母集団の外なので捕まらない」だったが、M1a の実測では**移すこと自体は赤になる**（本体から綴りが消えるため）。捕まらないのは「**移した先で**無効化が落ちる」退行の方である。単純な削除は `dead_code` が先に捕まえることも併記した

### Phase 3 — 検知の実測

- [x] M1a（移動のみ・錨健在）: **赤** ／ M1b（移動 + 錨消し）: **赤** — **修正前は緑だった同じ変異である**（本件の成果）
- [x] M2（錨だけ消す）: **緑**（`2 passed`）— 母集団が錨に依存しなくなった証拠。`expect(錨)` 案ならここが赤くなる
- [x] M3（クリーンツリー）: **緑**（`2 passed`）
- [x] M4（raw string で列 0 の `}` を偽装）: **赤**（`invalidate_icon_cache` の assert が発火）— 狭まる方向でも沈黙しないことの実測
- [x] 各変異の後に差分を確認。最終状態の `git diff` のハンクは `@@ -186,6 +186,61 @@` と `@@ -195,25 +250,24 @@` の 2 つだけで、**production コード（1〜186 行）は 1 バイトも変わっていない**（受け入れ条件 5）

### Phase 4 — 検証

- [x] カテゴリ A 全件 exit 0 — `cargo fmt --check` ／ `cargo check --workspace` ／ `cargo clippy --workspace --all-targets -- -D warnings` ／ `cargo test -p snotra`（**279 passed / 0 failed**）／ `cargo doc --workspace --no-deps --document-private-items`（intra-doc link 切れなし）
- [x] `npm run governance:check` — **全検査 passed**（検査 19 件 / 見出し参照 219 件）

### Phase 5 — レビュー対応（実装中に追加したフェーズ）

- [x] `/dry-check` — 手書き重複 **0 件**（4 候補すべて [維持]。`method_body` は別ファイルの `mod tests` 内 private 関数どうしで相互に呼べず、共通化は新モジュールを要する設計判断＝置換ではない）
- [x] `code-reviewer` ラウンド 1（Critical 0 / High 1 / Medium 3 / Low 4）— **全 8 件へ修正**。うち機序を自分で測り直したもの 2 件:
  - **M-1**: `cargo doc` は `#[cfg(test)] mod tests` を丸ごと見ない。対照で実測（production 側の壊れリンク → **exit 101** ／ `cfg(test)` 内 → **exit 0**）。**Phase 4 の「`cargo doc` もリンク切れなし」は当該 2 本について無効だった**
  - **H-1**: 対称修正の漏れ（`launcher_controller.rs:1604` に同じ偽の文が残存）。写しの母集団は自分でも grep で掃き、`.rs` 全体で残存 0 を確認
- [x] `code-reviewer` ラウンド 2（Medium 2 / Low 4）— **全 6 件へ対応**。最重要は **R2-1**（硬化が `top_level_fn_body` 側にだけ入り、鏡像の `method_body` に同型の穴が残った）。機序を `scratchpad/anchor_probe.rs` で両方向とも実測（列 0 → 黙って狭まる／8 スペース → 黙って広がる。既存 2 assert は 1 つも発火しない）
  - **この修正で私自身が新しい誤りを 1 つ作った** — `before.ends_with("\n    ")` ではアンカーに可視性修飾が挟まる呼び出し（`pub(super) fn on_enter(` 等）で誤発火し 3 本が赤になった。**字下げ幅**を見る形へ直した
  - R2-4（BOM・追跡下の `*.rs` に 0 件）は受容、R2-6（ADR の未追跡）はコミット時に確認
- [x] ラウンド 3 の 4 点は**自分の道具で測った**（「解消した」の判定は再実行の結論を受け取らない・`AGENTS.md`）:
  - `method_body` の全アンカー 3 つが 4 スペース字下げであることを実ファイルで確認（`536:    pub(super) fn activate_or_execute(` / `611:    fn shift_activate(` / `1429:    pub(super) fn on_enter(`）
  - 追加した `should_panic` が**真の検知器**であることを変異で確認（assert を `true` にすると `test did not panic as expected` で赤。clippy も `this assertion is always true` で二重に捕捉）
  - 定式化の残余（同じ字下げの doc 行にアンカーが先行出現すると通る）を doc へ明記し、両 helper の非対称が意図であることを書いた

## 未確定（実装前に潰す）

- [x] 敵対的調査（3b・`workspace/adversarial-1108.txt`）の所見の採否 — **完了（2026-08-17）**。命題 3 が壊れ（raw string で列 0 の `}` を偽装できる）、機序を `scratchpad/narrow_probe.rs` の `rustc` 実測で裁定した。**本計画への影響**: 不変条件の記述を 2 分岐へ精密化し、M4 を検証へ追加。本件は存在形 assert ゆえ狭まる方向でも沈黙しない。採否の全文は `research.md`
（未チェック項目は無い）

## 5c で人間へ問う 3 点（**本計画の差分には影響しない**）

いずれも「本 PR の射程外とする」判断そのものは確定済みで、どちらへ転んでも実装差分は同一である。回答は別 issue を立てるかどうかだけを決めた。**3 件とも「切る」の回答を得て、作成済みである。**

1. **#1112** — `launcher_controller.rs` の `activation_uses_frame_values_not_live_reads`（**否定形 assert**）に、母集団が canary と forbidden の間で切れると沈黙する残余が実在する（`narrow_probe.rs` で実測）
2. **#1114** — `scripts/governance/checks/G-check-skill-enumeration.mjs` の `sectionOf`（EOF フォールバック）— 同族であり同型ではない
3. **#1113** — `docs/development-principles.md` 176 行「狭すぎる（空）— canary が母集団に在ることで捕まえられる」は**否定形 assert には当てはまらない**（同上の実測）。183 行の射程（ファイル内か crate 横断か）も併せて

## 人間レビュー

- [x] 承認済み — 2026-08-17 / 問い: "この計画で実装へ進んでよろしいでしょうか（/implement へ渡します）" / 回答: "承認する"
- 併せて、上の 3 点はすべて follow-up issue を切る回答を得た（"scripts の sectionOf, 規範 176 行の射程, 否定形 assert の沈黙"）

## セルフレビュー

- リスク: 通常
  - `/plan-review`「リスク判定」の 6 条件に照らす: 永続形式・設定キー・公開 API・状態遷移＝**変えない**／worker・channel・listener・共有状態・非同期＝**触らない**／hook・CI・rules・skills・ガバナンス文書＝**触らない**（変更は `.rs` の `#[cfg(test)]` のみで、`.claude/rules/safety-nets.md` の `paths` にも該当しない）／網羅性が要件＝**否**（4 サイトの掃引は調査であって成果物ではない）／モジュール間インターフェースの新設・変更＝**否**（`mod tests` 内に閉じる）／`--deep` 指定＝**無し**
  - **該当判定を広げる側へ外さない**（#1106 の伸びしろ）。issue 本文が「セーフティネットの変更にあたる」と言うのは合意取得の要否についてであり、それは Step 5c の人間承認が満たす
- plan-review: 未実施（通常リスク）／自己レビューのみ
- エージェント数: 1（3b の敵対的調査のみ）
- 自己レビュー 5 点の照合:
  1. issue の全要件に作業項目が対応する — 提案 (1) `lines` → Phase 2、(2) 終端の assert → Phase 2、(3) LF/CRLF fixture → Phase 2。合意取得 → 5c
  2. 境界条件と検証 — 終端あり／終端なし／LF／CRLF／canary 欠落の 5 つに、M1〜M3 と fixture テストが対応する
  3. 新しい状態・リソース・プロセス — **無し**（純粋関数 1 つ）
  4. より単純な既存パターンで置き換えられないか — 検討し却下: `find("\npub(crate) fn ")` を `expect` にするだけの案は、**無関係な変更（錨の可視性・位置）で赤くなる**（M2 が緑にならない）。母集団を錨に依存させない方が強い
  5. 壊してはならない不変条件に検知手段があるか — 「`start_index_build` がアイコンキャッシュを無効化する」の検知手段が本件の対象そのもので、M1 が検知力を実測する
- 要対処: **1 件**（3b の命題 3 — 終端の偽装）。反映: 不変条件を 2 分岐へ精密化・検証に M4 を追加・射程外の残余 1 件を 5c の質問へ回した。軽微 3 件（全称の書き方の限定）は `research.md` へ反映済み
- 未検証: CI（CRLF checkout）での実挙動 — `ci.yml` は `pull_request` でのみ起動するため計画段階では測れない（`.claude/rules/safety-nets.md`）。**PR 本文のチェックリストへ送る**
