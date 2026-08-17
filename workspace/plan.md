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
| M1 | `build_index_from_material` の `pub(crate)` → `pub`（錨を消す）**かつ** `invalidate_icon_cache(&icons);` の行を削除 | **緑（＝欠陥の実証）** | **赤**（`invalidate_icon_cache` の assert） |
| M2 | 錨だけ消す（`pub(crate)` → `pub`） | 緑 | **緑**（母集団が錨に依存しなくなった証拠。`expect(錨)` 案なら赤くなり、無関係変更でノイズを出す） |
| M3 | 変異なし（クリーンツリー） | 緑 | 緑 |
| M4 | 本体へ列 0 の `}` を含む raw string を仕込む（**終端の偽装**・3b の指摘） | — | **赤**（母集団が狭まり、切れた位置に応じて canary か `invalidate_icon_cache` の assert が発火する。**沈黙しないことの実測**） |

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

1. **終端が別概念である。** `indexing.rs` はトップレベル関数（列 0 の `}`）、`method_body` はメソッド（4 スペースの `}`）、`view.rs` は `#[cfg(test)]` 分割の出現回数、`startup.rs` は enum 宣言。crate 全域で「1 つに閉じる」は**そもそも達成不能**であり、B を採っても実装は 3 つ残る
2. **稼働中のガードへ爆風が及ぶ。** `method_body` を一般化すれば、それに依存する 2 検査の変異注入を再実測する義務が生じる（`AGENTS.md`「レビュー指摘へ修正を当てた」の再実行則）。issue #1108 が合意を求めた射程は `indexing.rs` である
3. **新ファイルは統治コストを持つ**（`mod` 宣言 + `CLAUDE.md` のモジュール索引 + `G-module-linkage`）
4. issue 本文は `method_body` を「**実装例**」と呼び、抽出・共有を求めていない

**緊張の逐語記録**: `docs/development-principles.md` 183 行は「**ゆえに切り出しの helper は 1 つに閉じる。** 検査ごとに書き写すと、上の 2 方向の壊れ方が**検査ごとに独立に起きる**」と書く。本計画はこれに反して 3 つ目の実装を作る。**ただしこの規範の実証はファイル内の写しである**——由来は `db7f77df`（#1106 サイクルの振り返り・#1111）で、続く文が名指すのは「#1077 で 2 本が別々の状態になった」（同 PR #1107 の変更ファイルに `indexing.rs` は無く、2 本とも `launcher_controller.rs` の中）と「#1106 は 2 本目を書くときに写し、`/simplify` の指摘で 1 つへ畳んだ」（`view.rs` の中）である。本計画は「1 ファイル内では 1 つに閉じる」（`indexing.rs` の検査 2 本が同じ helper を通る）を満たす。

**反転条件**: 次にソーステキスト検査を**新設する** issue が立ったら（＝4 つ目の実装が生まれるなら）、そこで共通化を再検討する。

**レビュー委譲時の申し送り**（`/dry-check`・`code-reviewer` へ先渡しする）: `top_level_fn_body` と `method_body` の重複は**根拠つきで意図的に分けた構造**である。根拠の所在は本節と、両 helper の doc コメント。DRY 違反として挙げないこと。

## フェーズと作業項目

### Phase 1 — 欠陥の実証（Red）

- [ ] M1（二段変異）を当て、`cargo test -p snotra start_index_build_invalidates` が**緑のまま**であることを実測し、出力をこの計画へ追記する
- [ ] **緑になった理由まで一致を確認する** — EOF まで伸びた母集団が飲み込む偽陽性の源は `mod tests` 内の assert メッセージのリテラル（現行 214 行の `try_begin_index_build(` と 219 行の `invalidate_icon_cache(`）である。母集団を `println!` 等で切り出して、その 2 行が実際に含まれることを見る（「緑だった」ではなく「なぜ緑かまで一致した」を Red の証拠にする）
- [ ] 変異を戻し、`git status --short` が空であることを確認する

### Phase 2 — 修正

- [ ] `top_level_fn_body` を `indexing.rs` の `mod tests` へ追加する
- [ ] `start_index_build_invalidates_the_icon_cache` の母集団取得を helper 経由へ差し替える
- [ ] `top_level_fn_body_is_line_ending_agnostic`（LF / CRLF の文字列リテラル fixture）を追加する
- [ ] 既存テストの doc に、母集団の新しい壊れ方を書き足す（終端が見つからなければ赤／列 0 の `}` が本体に現れれば狭くなり、canary より前で切れれば canary が・後で切れれば `invalidate_icon_cache` の assert が赤。**この安全性は存在形の assert に依る**——否定形なら沈黙する〔#1112〕）

### Phase 3 — 検知の実測

- [ ] M1 で**赤**になることを実測する（メッセージが `invalidate_icon_cache` の欠落を名指すことも見る）
- [ ] M2 で**緑**のままであることを実測する（母集団が錨に依存しない証拠）
- [ ] M3（クリーンツリー）で緑を実測する
- [ ] M4（raw string による終端の偽装）で**赤**になることを実測する
- [ ] 各変異の後に `git status --short` が空であることを確認する

### Phase 4 — 検証

- [ ] `docs/build-commands.md` カテゴリ A 全件（fmt / check / clippy / test / doc）を exit 0 まで走らせる
- [ ] `npm run governance:check` を 1 回走らせる

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
