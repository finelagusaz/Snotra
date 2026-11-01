# review-1328-r2 (sha 536b58c5dbc38a365ee46f0c58f4f6d3c668190a)

状態: 完了（align exit 0・変異は巻き戻し済み・作業ツリーは clean。残るのは .delegate/ だけ）

## 1 巡目の指摘の解消（見つけたときと同じ方法で測り直した）

- M1 解消。Step 3 :71 に「受理した旧形式を `apply_migrations()` で現行の形へ移しているかも見る（例: `snotra-core/src/config/migrate.rs` の `migrate_instant_legacy_commands`）」が入った。`migrate.rs:187` に実体があり、`apply_migrations`（:64）が :76 で呼ぶことを grep で確かめた。
- M2 解消。`snotra-core/src/config/io.rs` の `//!` の割り振り（区分＝`LoadOutcome`、保全＝`load_from_dir_reporting` の rustdoc）と一致した。`LoadOutcome`（:14）は 4 variant の doc を持つ。fn の doc（:40-45）は 5 枝の保全を持つ。
- L2 解消。完全パス `snotra-core/src/config/io.rs` になった。
- L1 解消。新しい文「満たしたかの検証は `/persistence-check` で行う（失敗の型もそこが持つ）」は、規則ごとの根拠を主張しなくなった。
- L3 解消。「出典は」が入った。
- L4 解消。見出し「Config のデシリアライズ経路」は CLAUDE.md:118 に実在し、着地する。変異で赤になることも確かめた（下）。

識別子の集合差を取り直した（元の版 ade83abf~1 → 536b58c5 のスキル。バッククォート語と #番号）。落ちたのは `Legacy {..}` だけで、CLAUDE.md に `Legacy { .. }` として残る。足されたのは `//!`・`Config::load_from_dir_reporting`・`LoadOutcome`・`migrate_instant_legacy_commands`・`snotra-core/src/config/{io,migrate}.rs`・`snotra-core/src/window_data.rs`。

## 修正差分へ同じ枠組みを当てた所見

Critical / High / Medium: 0 件

- L5 ⚠️ M1 で戻した行は、問いの形だが規則の中身を持つ（「受理したら移す」）。CLAUDE.md「データ永続化の注意」には「Legacy で受理する」までしか無い。規則はスキル、検証もスキルという形になり、#1328 の役割分担（規則の正本は CLAUDE.md）から見ると半歩はみ出している。ただし `InstantAction::Legacy` は `validate.rs:77` で受理後もそのまま扱える。移行は必須の規則とは言い切れないので、「見る」という弱い形にとどめるのは妥当とも読める。直すなら CLAUDE.md の serde の行に「受理後に `apply_migrations()` で現行形へ移す」を足して、スキルは参照にする。裁定は呼び出し側へ。

## L4 と同じ形（「データ永続化の注意」以外の節の写し）の全行突き合わせ

方法: スキルの全行を CLAUDE.md の全行と突き合わせた。比べたのは 6 文字 shingle の重なり率で、記号と空白は除いた。比べる相手は各行で最も重なる 1 行。
校正: 既知の写しである ade83abf の Step 5 :89（L4）は、CLAUDE.md:120（Config のデシリアライズ経路）と 50% だった。

現版で「データ永続化の注意」以外の節に当たった行は次のとおり。50% 級のものは無く、写しは無い。
- Step 2 :45-47 ↔「IndexCache バージョン変更チェックリスト」2・3・4（23%・16%・12%）: スキル側は全 struct へ一般化した問いである（magic+version ヘッダー・Legacy variant を含む）。IndexCache の手順の写しではない
- Step 6 表の行 ↔ IndexCache 8・9（25%・13%）: 境界条件の種類の表で、手順の写しではない
- Step 5 :87-89: 正準形の参照そのもの（参照の語が重なっているだけ）
- 出力節 :109 ↔ IndexCache 4（15%）: 偶然の語の一致

「データ永続化の注意」に当たる行（:58・:68・:71・:77・:79・:83・:90・:110）は、意図的に分けた構造（問い・失敗の型・参照）なので所見にしない。

## 変異（新しい正準形「Config のデシリアライズ経路」）

| 状態 | SKILL.md の hash-object | 結果 |
|---|---|---|
| 変異前 | 1c127c546b93fe068f3239fc5c01563d4c891402（`git rev-parse 536b58c5:…SKILL.md` と一致） | — |
| :89 を「Config のデシリアライズの経路」へ | 8a1f1a0a165b365c241a9998fe8204fc0d630427 | EXIT=1 |
| 巻き戻し（git checkout --） | 1c127c546b93fe068f3239fc5c01563d4c891402 | EXIT=0「全検査 passed（検査 17 件）」 |

赤の出力（全文）:
```
governance:check — 1 件の不整合:
  .claude/skills/persistence-check/SKILL.md:89  見出し参照が着地しない: `snotra-core/CLAUDE.md`「Config のデシリアライズの経路」（snotra-core/CLAUDE.md に該当する見出し・リード文が無い）
```
文言の出所は `scripts/governance/checks/G-heading-refs.mjs:62`（1 巡目に grep で確定）。照合は前方一致なので、切り詰める形の変異は避け、中間に語を挟む誤ラベルにした。
