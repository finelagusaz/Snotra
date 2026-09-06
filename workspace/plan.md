# plan — issue #1241: `snotra-core-search.md` の散文形の節名参照を正準形へ直す

## 目的と受け入れ条件

`.claude/rules/snotra-core-search.md` が `snotra-core/CLAUDE.md` の節を**機構の射程外の散文形**で指しており、うち 1 件（「文字ビットマスク」）は既に消滅した見出しを指している。正準形 `` `<対象>`「<見出し>」 `` へ書き直し、以後の改名を G-heading-refs が捕まえる状態にする。

受け入れ条件:

1. `.claude/rules/snotra-core-search.md` に、`snotra-core/CLAUDE.md` の節を「」だけ／散文だけで指す行が残っていない（grep で確認）
2. 直した参照はすべて現存の見出しへ着地する（`npm run governance:check` 緑）
3. 直した参照が機構に見られている——次の 3 点を**揃えて**成立とする（`checked` は `isRefTargetSpelling` 通過時に数えるだけで着地の証拠にならない・3b ⚠️9）: (a) `見出し参照 N 件` が 371 → 375 に増える、(b) 全検査 passed のまま、(c) 着地先の見出しを一時的に壊すと G-heading-refs が当該行を名指して赤になる（測ったら元へ戻す）
4. rule の判定・射程・行動形（何をしたらどこを読むか）は変えない。変えるのは参照の綴りだけ

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `.claude/rules/snotra-core-search.md` | L17（横断不変条件の行） | 3 つの節名を正準形へ |
| `.claude/rules/snotra-core-search.md` | L24（`Ord` / `BinaryHeap` の行） | `snotra-core/CLAUDE.md` 実装前チェック → 正準形へ |

`snotra-core/CLAUDE.md` は触らない（見出しはすべて現存。変えるのは参照側だけ）。

## 実装（書き換えの逐語）

### L17

現行:

```
- 横断不変条件（並列 Vec レイアウト・ビットマスク一元化・has_path_sep 非互換）: `snotra-core/CLAUDE.md` の search.rs 節・「文字ビットマスク」節・「incremental cache とパスクエリの非互換」節
```

書き換え後（1 物理行に収める・G-folded-heading-refs 対策）:

```
- 横断不変条件（並列 Vec レイアウト・ビットマスク一元化・has_path_sep 非互換）: `snotra-core/CLAUDE.md`「モジュール構成」の `search.rs` 行・`snotra-core/CLAUDE.md`「`char_bitmask` は `query.rs` に一元化済み」・`snotra-core/CLAUDE.md`「incremental cache とパスクエリの非互換」
```

理由:
- `search.rs` の記述は `## モジュール構成` 内の箇条書き `- `search.rs` — …` で、`ANCHOR_SPECS` のどの種にも当たらない（太字リードではない）。節は正準形で機構に見せ、行は散文で添える
- 「文字ビットマスク」は現在の見出し `### `char_bitmask` は `query.rs` に一元化済み` を逐語で写す。`normAnchor` がバッククォートを剥ぐので照合に支障は無い（同型の実例: `docs/adr/ADR-autostart-state-ownership.md` L53）
- 「incremental cache とパスクエリの非互換」は現存の見出しと一致

### L24

現行:

```
- **`Ord` / `BinaryHeap` / top-k に触れたら**: `snotra-core/CLAUDE.md` 実装前チェックの規律（先頭が最良/最悪の明記・入力順不変テスト）に従う
```

書き換え後:

```
- **`Ord` / `BinaryHeap` / top-k に触れたら**: `snotra-core/CLAUDE.md`「実装前チェック」の規律（先頭が最良/最悪の明記・入力順不変テスト）に従う
```

理由: 見出しは `## 実装前チェック（必須）`。照合は前方一致なので後置注記を省いてよい（既存例: `AGENTS.md`「条件別チェック」）。

## 実装順序

1. L17 を書き換える
2. L24 を書き換える
3. `npm run governance:check` を走らせ、緑かつ件数が 371 → 375（新設 4 件）になることを確認する
4. フォールトインジェクション 1 回: `snotra-core/CLAUDE.md` L111 の見出しを一時的に別名へ変え、`governance:check` が `snotra-core-search.md` L17 を名指して赤になることを見てから `git checkout -- snotra-core/CLAUDE.md` で戻す
5. 同ファイルを grep し、`snotra-core/CLAUDE.md` の節を散文形で指す行が残っていないことを確認する（`Select-String -Pattern 'CLAUDE\.md`[^「]'`）

## 不変条件と異常系

- 1 物理行に収める（折れると G-folded-heading-refs の対象・`.claude/rules/governance-docs.md` L18）
- 消滅した節名を正準形で書かない（`.claude/rules/governance-docs.md` L20）——「文字ビットマスク」は正準形にせず、現在名へ置き換える
- rules は `AGENTS.md` 条件別チェック表のセーフティネット母集団だが、今回の変更は判定・射程・行動形を変えない綴りの訂正である。`safety-nets.md` の測り直し条項は「機構自身の配置を変える」引き金に当たらない。**それでも受け入れ条件 3 で 1 度は赤を見る**（費用は 1 コマンド）
- PostToolUse hook は `.claude/rules/*.md` に検査を割り当てない。沈黙は「何も走らなかった」。決定的な検査は手で `governance:check`

## テスト方針と検証コマンド

```
npm run governance:check
Select-String -Path .claude/rules/snotra-core-search.md -Pattern 'CLAUDE\.md`[^「]|「[^」]+」節'
```

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（挙動変更なし）
- `docs/`: 不要
- `.claude/rules/snotra-core.md` の「」だけの節名（L12〜18・L24）は同型だが今回の対象外。**別 issue に切り出す**（PR 本文に「残余」として記す。書式「1 文書を宣言して以後は見出しだけ」自体が射程外で、直すなら書式の議論が要る）

## 作業項目

### Phase 1 — 書き換え

- [x] `.claude/rules/snotra-core-search.md` L17 を上記の逐語へ書き換える
- [x] `.claude/rules/snotra-core-search.md` L24 を上記の逐語へ書き換える

### Phase 2 — 検証

- [x] `npm run governance:check` 緑・見出し参照件数 371 → 375（実測: 全検査 passed・375 件）
- [x] フォールトインジェクション（L111 見出しの一時改名 → 赤 → 戻す）で L17 が名指されることを確認（実測: `.claude/rules/snotra-core-search.md:17 見出し参照が着地しない`。復元後 `git status` は rule 1 枚のみ）
- [x] 同ファイルの grep で散文形の節名参照が残っていないことを確認（0 件）

## 未確定（実装前に潰す）

- [x] 敵対的調査（3b）の所見を反映する — 壊せた 2 件（L27→L24・G-near-heading-refs の沈黙の機序）を自分で再測して採用、⚠️9 を受け入れ条件 3 へ反映。採否表は `research.md`「敵対的調査（3b）の反映」

## セルフレビュー

- リスク: 通常（コード・永続形式・並行性・状態遷移に触れない。ガバナンス文書の見出しの移動・圧縮・分割も無い——参照側の綴りだけ）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 要対処: 3 件（L27→L24 訂正・沈黙の機序訂正・受け入れ条件 3 を 3 点セットへ）——すべて反映済み
- 5a 照合: (1) issue の要件（L17 正準形化・同ファイル grep）に Phase 1/2 が対応 (2) 境界条件＝バッククォート入りラベル・後置注記の前方一致・箇条書きは着地先不可、の 3 つに検証あり (3) 新しい状態・リソースなし (4) より単純な既存パターン＝正準形そのもの (5) 不変条件（1 物理行・消滅節名を正準形にしない）は G-folded-heading-refs / G-heading-refs が検知
- 未検証: なし

## 人間レビュー

- [x] 承認済み — 2026-09-06 / 問い: "次のどちらかをお願いいたします。1. `workspace/plan.md` へ注釈を追加する 2. 計画を明示的に承認する（例:「承認」）" / 回答: "承認"
