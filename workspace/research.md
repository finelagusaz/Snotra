# research — issue #359 RETROSPECTIVE.md 未完了ネクストアクションの追跡先移行

## issue の要約

`RETROSPECTIVE.md` は **上書き運用**（サイクル末に前回内容を新サイクルの振り返りで置換）。ところが「ネクストアクション」欄に**未完了タスク**が蓄積されており、上書きのたびに失われうる。AGENTS.md は「実装・大規模ドキュメントは GitHub Issues に起票」と既定しているが、上書き時点でその振り分けを**強制する機構が無い**。

受け入れ条件:
1. 未完了ネクストアクションが、上書きされても失われない場所で追跡されている
2. #347 Phase 3 の残作業が issue として明示、または既存 issue に集約されている
3. サイクル末 health-check の実行責任が曖昧でない

## 確定した事実（調査結果）

- **受け入れ条件②は既に充足済み**。`#347`（StaleSet 契約）・`#348`（下流対症療法）とも **CLOSED**。設計メモ `docs/design/2026-05-31-coherence-staleset.md` は実在し、`docs/architecture.md:114,240` から参照済み。新規 issue 化は不要。
  - 残る `.claude/rules/*` 同期は grep で痕跡なし。#347 クローズ時に不要と判断されたものとみなす（本 issue のスコープ外）。
- **`RETROSPECTIVE.md` は issue 作成後すでに二度上書きされている**（`e36db9a`#347 → `63c6f8b`#362 → `d2468c3`#374）。issue が懸念した #347 Phase 3 の項目は現行ファイルから既に消失済み（git に残るのみ）= 損失リスクが現実化していた実例。
- 現行 `RETROSPECTIVE.md`（#374 サイクル）のネクストアクション残項目:
  - `[ ] push して PR 作成 … e2e ラベル … マージは手動` → #374 は merged 済み = **完了**。振り分け不要（除去のみ）。
  - `[ ] （任意 follow-up・別 issue 候補）instantCommandMode ラッチの除去` → **サイクルを越える follow-up**。新ルールでは issue 行き。

## 関連ファイル・モジュール

| ファイル | 役割 | 本 issue での扱い |
|---|---|---|
| `.claude/skills/retrospective/SKILL.md` | サイクル末振り返りの手順（Step 1〜5 + 出力） | **主変更**: 書式から「ネクストアクション」欄廃止、タスク振り分けステップ追加、末尾に health-check ステップ追加 |
| `AGENTS.md`（72〜78 行: RETROSPECTIVE.md の運用節） | retrospective 運用の SSOT | **変更**: フォーマット 3→2 セクション、持続タスクは置かない不変条件、振り分け基準、health-check 責任の明記 |
| `CLAUDE.md`（ルート, 67 行 スキル表 /retrospective 行） | スキル一覧の説明文 | **変更**: 説明文を新フロー（教訓抽出→タスク振り分け→health-check→教訓のみ上書き）へ更新 |
| `.claude/skills/health-check/SKILL.md` | 衛生チェック（報告のみ・修正しない） | **変更なし**。retrospective から起動されるだけ。報告のみ契約は維持 |
| `RETROSPECTIVE.md` | サイクル末教訓スナップショット | **変更**: 現行残項目を振り分け、ネクストアクション欄を撤去し新フォーマット（教訓のみ）へ整える（ブートストラップ） |

## 既存パターン（再利用）

- **スキルからのスキル/エージェント起動**: `start-issue`（`allowed-tools` に `Skill`）が `/plan-review` 等を起動、`implement`（`Skill` + `Agent`）が `/health-check` 相当や code-reviewer を起動する既存パターンあり。`/retrospective` も `allowed-tools` に `Skill`（health-check 起動用）+ `Agent`（health-check が内部で使用）を追加すれば同型で動く。
- **タスクの永続トラッカー**: GitHub Issues（既に AGENTS.md が一次トラッカーと規定）。PR 本文チェックリストは PR ライフサイクル内タスクの追跡先として標準的。

## 技術的制約

- 本変更は **エージェント運用プロセス**（skill + AGENTS.md）の変更。CLAUDE.md 憲章「エージェント設定の更新は相談してから」に該当 → 方向性はユーザー合意済み（下記「決定事項」）。
- Win32 / IPC / リアクティビティ制約は**無関係**（コード変更ゼロ、`.md` のみ）。
- `/retrospective` から `/health-check` を**ネスト起動**するには `allowed-tools` の拡張が必要（既存スキルと同型）。実装時にネスト起動が実際に機能するか動作確認する。

## 決定事項（ユーザー合意済み）

- **RETROSPECTIVE.md は教訓のみ**にし、ネクストアクション欄を廃止。タスクは「起票するか PR に書く」。
- **振り分け境界**:
  - PR のライフサイクル内で閉じるもの（push/ラベル/CI 確認/手動マージ 等）→ **PR 本文のチェックリスト**
  - サイクル/PR を越えて生き残るもの（follow-up・別リファクタ・繰越）→ **GitHub issue 起票**
- **health-check（受け入れ条件③）** → `/retrospective` の**末尾に組み込む**（サイクル末という実行点へ結合）。

## 未解決の疑問

- なし（方向性・境界・health-check 置き場はユーザー合意済み）。
- 判断項目（plan で扱う）: 現行 RETROSPECTIVE.md の `instantCommandMode ラッチ除去`（任意・本質的に不要と注記済み）を実際に issue 起票するか。新機構の最初の適用例として起票を推すが、低優先/任意を明記。plan レビューで最終確認。
