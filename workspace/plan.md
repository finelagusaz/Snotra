# plan: issue #588 rules ルーター化の試行

ブランチ: `chore/588-rules-router-trial`。コード挙動の変更なし（doc コメント 1 行 + rule 1 ファイル）。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `.claude/rules/snotra-core-search.md`（**唯一の変更ファイル**） | 17 行 → ルーター化。構成: (1) 必須検査「incremental・キャッシュ再利用に触れたら `/cache-check`」、(2) 正準参照「責務・スコア階層 = `//!` + `mod score_tier` doc + `SearchEngine` struct doc / incremental 述語 = `decide_incremental` doc / 横断不変条件 = `snotra-core/CLAUDE.md` search.rs 節・ビットマスク節・has_path_sep 節」、(3) 引き金「新マッチパス追加 → pre-filter の doc（`compute_wave2`・false-negative 不変条件）」「Wave/kana 構築変更 → `compute_wave1` doc（#337）」。**事実の再記述ゼロ**（スコア値・Ord 向き・述語条件を書かない）。**参照はセクション名・シンボル名のみで行番号を書かない**（governance-docs.md の序数ドリフト警告）。併配送される `snotra-core.md` の既存項目とも重複させない |

（plan-review で確定: search.rs への補填は**不要**——9000 混同警告は `search.rs:672-673` に既存。当初計画の Phase 1 は削除）

## 実装順序

1. **Phase 1（退避 → 薄化）**: 旧 rule 全文を issue #588 へコメント退避（計測設計 2 の照合原本）→ rule をルーターへ書き換え → `npm run governance:check`（G3 参照実在・G7 glob）緑を確認
2. **Phase 2（採用ゲート = 読者故障注入・issue 計測設計 1）**: サブエージェント読者で thin/thick を比較する
   - 代表タスク 2 件: (T1)「新しいマッチ種別（ローマ字頭文字マッチ）を追加する計画を立てよ」(T2)「`has_dot` の判定を変更する計画を立てよ」
   - 読者クラス 2 種 × thin rule = 4 体（手抜き読者: 最小の読取で計画を出す指示 / 忠実読者: 規範に従い正準を読む指示）。対照: 忠実読者 × thick rule（旧全文）= 2 体
   - 合否（事前宣言済み）: thin の各読者が (a) 正準（`//!`・score_tier・decide_incremental・CLAUDE.md 該当節）へ到達したか、(b) 当該タスクの不変条件（T1: pre-filter OR 関係 + スコア階層 / T2: incremental ガード連動）が計画に現れたか。**thick 対照に現れて thin に現れない不変条件が 1 件でもあれば不採用**（rule を復元し、欠落項目を分析して issue に記録）
   - 判定はサブエージェントの自己申告でなく、出力された計画文面をオーケストレーターが照合する
3. **Phase 3（コミット・PR）**: 検証 → 故障注入の結果を issue へ記録 → コミット → PR 作成。**PR 本文に closing keyword を書かない**（試行期間中 #588 は open 維持。テンプレートの `Closes` 行を必ず除去し、マージ直前に `closingIssuesReferences` が空であることを確認する）。試行期間の満了目安 = search.rs を触る PR 3 件 or 2026-09-13（8 週間）の早い方

## 不変条件

- **消失ゼロ**: 落とす 8 項目すべてに正準側の実在確認済み（research.md の表・8/8 実在。偵察と独立再導出が独立に一致）
- **コード変更なし**: 変更は rule 1 ファイルのみ
- **薄 rule は事実を再記述しない**: 引き金・参照・検査のみ（ルーターの定義。再記述を 1 つ許すと薄化の意味が崩れる）
- **#588 は PR で閉じない**: 試行期間の観測（計測設計 2・3）の器として open 維持
- **不採用条件を先に固定**: Phase 3 の合否基準・停止条件は issue 本文の事前宣言に従い、実験後に動かさない

## テスト方針

- 機械検査: `npm run governance:check`（薄化後の参照実在 G3・glob 有効性 G7。`.claude/rules/**` は selectChecks 対象外＝沈黙は未検査のため手動実行必須・build-commands カテゴリ F）
- 行動検証: Phase 2 の読者故障注入（thin 4 体 + thick 忠実対照 2 体、計画文面をオーケストレーターが照合。自己申告を根拠にしない）
- burn-down 記録: rules 合計行数の前後（147 行 → 実測値）を PR 本文に記す（#593 指標）

## セルフレビュー

### plan-review 結果（Step 5a）

- **要対処 1 件（反映済み）**: research.md の「9000 混同警告が欠け」は誤り——`search.rs:672-673` に co-location 済みで、**偵察と独立再導出が独立に同一の誤りを検出**（枠組みの独立が作者の見落としを拾った実例）。当初 Phase 1（正準補填）を削除し、変更ファイルは rule 1 つに縮小
- **独立再導出の追加発見（反映済み）**: 参照は行番号でなくシンボル名・セクション名で書く（governance-docs.md の序数ドリフト警告）／併配送される `snotra-core.md` と重複させない／間接参照の追従 0 件（この rule を名指しする文書なし・grep 実測）／試行満了の絶対日付 2026-09-13
- **一致（完全性の証拠）**: 8 項目の正準所在（下位主張含め file:line 単位）・採用ゲートの位置（コミット前）・PR で issue を閉じない判断・退避先（issue コメント）——すべて独立に一致

### 5b の 3 観点

1. **境界条件**: 読者故障注入の合否は「thick 対照に現れて thin に現れない不変条件が 1 件でもあれば不採用」という差分基準で、判定の曖昧さを排除。タスク 2 件は旧 rule の知識クラスタ（マッチパス追加系・incremental 連動系）を両方跨ぐ
2. **シンプル化**: 変更 1 ファイル・コード変更ゼロまで縮んだ。故障注入 6 体は「これ以上減らすと 2 クラス × 2 タスクの比較が成立しない」最小構成（thick 手抜き対照は採用判断に寄与しないため置かない）
3. **破壊不変条件 + 検知手段**: (a) 消失ゼロの誤認 → 偵察 + 独立再導出の二重照合済み + 旧全文の issue 退避（復元可能）。(b) 参照タイポ → governance:check G3（バッククォートパス）。ただし**シンボル名参照（`decide_incremental` 等）の腐敗は G3 の対象外**——これは薄 rule が新たに抱える残余であり、改名時は AGENTS.md「関数を新規定義・改名」行の compile-fail 検出器と grep が守る（rule 側は grep 対象に入る）。(c) 採用ゲート失敗時 → rule を git revert で復元（1 コミットで完結させる）

## SPEC.md 更新要否

不要（開発ガバナンスのみ）。

## スコープ外

- 他 rules（src-tauri.md・ui.md 等）への横展開 — 試行期間の観測結果（issue 計測設計 3 の停止条件）を待つ
- `//!` 群の再構成・snotra-core/CLAUDE.md の改稿（欠け 1 行の補填を除く）
- 計測設計 2 の実運用監査 — PR 後の継続タスク（issue が器）
