# plan — issue #359 RETROSPECTIVE.md 未完了ネクストアクションの追跡先移行

## 設計の核（一段抽象化）

`RETROSPECTIVE.md` の欠陥は「**上書きファイル（単一サイクルの教訓スナップショット）の中に持続状態（サイクルを越えるタスク）を同居**させている」こと。性質の違う二つを同じ器に入れているため上書きで道連れになる。
→ 解は「ゲートで守る」ではなく「**器を分ける**」: タスクを永続トラッカー（issue）か PR スコープ（PR 本文）へ追い出し、`RETROSPECTIVE.md` は教訓だけにする。

## 変更ファイル一覧

### 1. `.claude/skills/retrospective/SKILL.md`（主変更）
- **frontmatter `allowed-tools`**: `Skill`（/health-check 起動用）**のみ**を追加。`Agent` は**追加しない**——health-check は Check 1〜10 すべて Read/Grep/Glob/Bash で完結しサブエージェントを起動しない（frontmatter の `Agent` は死権限）。既存の `start-issue`（他スキル起動に `Skill` のみ付与）と同型。過剰付与回避。
- **frontmatter `description`**: 「…教訓の抽出とドキュメント反映 → 残タスクを issue/PR へ振り分け → health-check → RETROSPECTIVE.md（教訓のみ）を上書き」へ更新。
- **Step 4「RETROSPECTIVE.md の上書き」**: 書式を **2 セクション（よかったこと・伸びしろ）** に変更。「ネクストアクション」セクションを書式テンプレートから削除。
- **新 Step（タスクの振り分け / Step 4 の直前か直後）**: 残タスクを以下へ振り分け、`RETROSPECTIVE.md` には残さない:
  - PR ライフサイクル内で閉じる（push/ラベル/CI 確認/手動マージ 等）→ **PR 本文のチェックリスト**
  - サイクル/PR を越える（follow-up・別リファクタ）→ **GitHub issue 起票**
- **新 Step 6「サイクル末 health-check」**: `/health-check` を実行し、発見事項を Step 3（doc 改善）/ タスク振り分け（issue・PR）へ流す。
- **出力セクション**: 「ネクストアクションの一覧」→「振り分けたタスク（issue 番号 / PR チェックリスト）の一覧」+ 「health-check 結果サマリ」へ更新。

### 2. `AGENTS.md`（72〜78 行: RETROSPECTIVE.md の運用節）
- **フォーマット**: 「よかったこと・伸びしろ・ネクストアクション」の **3 セクション → 2 セクション（よかったこと・伸びしろ）**。
- **新・不変条件**: 「`RETROSPECTIVE.md` は上書きファイル。**持続タスクを置かない**（上書きで失われるため）」。
- **タスクの扱い（旧「ネクストアクション」箇条書きを置換）**: 教訓は doc へ即反映、実行タスクは PR ライフサイクル内＝PR 本文 / サイクル越え＝issue へ振り分ける。
- **サイクル末 health-check の責任明記**: `/retrospective` の末尾で `/health-check` を実行する旨（受け入れ条件③）。

### 3. `CLAUDE.md`（ルート, 67 行 /retrospective スキル表の行）
- 説明文を新フローへ更新: 「サイクル終了後: 教訓を AGENTS.md/各 CLAUDE.md に抽出 → 残タスクを issue/PR へ振り分け → /health-check → RETROSPECTIVE.md（教訓のみ）を更新、メモリ鮮度チェック」。

### 4. `RETROSPECTIVE.md`（ブートストラップ: 新機構の最初の適用）
- 現行ネクストアクション残項目を振り分け:
  - `push して PR 作成 …`（#374 merged 済み = 完了）→ **除去のみ**。
  - `instantCommandMode ラッチ除去（任意 follow-up）` → **GitHub issue 起票**（低優先・任意を明記）。【判断項目: 起票要否は実装前に最終確認】
- ネクストアクション欄を撤去し、新フォーマット（よかったこと・伸びしろのみ）へ整える。

## 実装順序（依存順）

1. **AGENTS.md**（運用の SSOT を先に確定）
2. **`.claude/skills/retrospective/SKILL.md`**（SSOT に合わせて手順を更新）
3. **ルート CLAUDE.md**（スキル表の説明を同期）
4. **RETROSPECTIVE.md ブートストラップ**（残項目振り分け → 必要なら issue 起票 → 欄撤去）
5. **検証**: `/health-check`（Check 9 スキル整合ほか）を実行し、ドリフトゼロを確認（= 新 Step 6 のドッグフード）

## 不変条件

- **RETROSPECTIVE.md は上書きファイルのまま**。新たに「持続タスクを入れない」不変条件を文書化する（これ自体が再発防止）。
- **health-check の「報告のみ・修正しない」契約を維持**。retrospective 側が発見事項を doc 改善 / タスク振り分けへ流す責務を持つ（health-check は判断しない）。
- **責務境界**: retrospective → health-check の一方向起動のみ（循環なし。health-check は retrospective を呼ばない）。
- **allowed-tools 整合**: `/retrospective` に **`Skill` のみ**追加（health-check 起動用）。既存ステップ（git/gh/Read/Edit/Write/Grep/Glob）の権限は維持。`Agent` は health-check が使わないため付与しない（過剰付与回避＝この不変条件自体の遵守）。
- **異常系**: health-check がネスト起動に失敗した場合でも retrospective の主目的（教訓反映・上書き）は完了済み（Step 6 は末尾）。失敗は報告に明記し、サイクルを wedge させない。

## テスト方針

コード変更ゼロ（`.md` のみ）のため AGENTS.md Step 8 カテゴリ A〜D（cargo/npm）は非該当。検証は **`/health-check` のドッグフード**で行う:
- **Check 9（スキル定義整合）**: CLAUDE.md スキル表 ↔ `.claude/skills/*/SKILL.md` が一致（/retrospective 説明更新後）。
- **Check 3（AGENTS.md ドキュメント参照実在性）** / 参照リンク健全性: 編集で死リンクを生まない。
- **Check 5（SSOT ドリフト）**: skill にコマンド本体を直書きしない（health-check 起動は「`/health-check` を実行」という参照のみ。コマンド本体ではない）。
- **手動確認**: AGENTS.md ↔ retrospective skill ↔ CLAUDE.md の三者で「フォーマット = 2 セクション」「タスク振り分け境界」「health-check 末尾実行」の記述が矛盾しないか目視。

## SPEC.md 更新要否

**不要**。本変更はプロダクト挙動（IPC 契約・状態遷移・UI フロー）ではなく**エージェント運用プロセス**。SPEC.md の管轄外。

## セルフレビュー

### /plan-review 結果（Explore ×2）
- **影響範囲（Agent 1）**: 計画の 4 ファイル以外に、retrospective の「3 セクション/ネクストアクション欄」を前提にした **第 5 のファイルは存在しない**。docs/・.claude/skills(12)・.claude/agents・ルート MD を網羅検査済み。漏れゼロ。
- **スキル機構（Agent 2）— 要対処を 1 件検出・反映済み**: health-check は `allowed-tools` に `Agent` を持つが Check 1〜10 で**実際には使わない**（死権限）。よって `/retrospective` に `Agent` を足すのは根拠なき過剰付与。→ **`Skill` のみ追加**に修正済み（上記 1・不変条件に反映）。
- health-check「報告のみ・修正しない」契約、retrospective→health-check 一方向（循環なし）は確認済みで矛盾なし。

### 5b チェックリスト
1. **対称コードパス**: 該当なし（コード変更ゼロ。issue↔PR の振り分けはポリシー分岐であり対称コードパスではない）。
2. **影響範囲の網羅性**: Agent 1 が第 5 ファイル不在を確認。✓
3. **境界条件**: 振り分け境界（PR ライフサイクル内＝PR 本文 / サイクル越え＝issue）を定義。判断に迷うタスクは**永続側＝issue** をデフォルトにする。health-check ネスト起動失敗時は retrospective 主目的（教訓反映・上書き）が既に完了済み（Step 6 末尾）＝ wedge しない。
4. **リソース管理**: listen/子プロセス/AtomicBool 等のライフサイクル資源は導入しない。`allowed-tools` は権限であり破棄対象なし。該当なし。
5. **既存パターン整合**: ネスト起動は `start-issue`（`Skill` のみ）と同型に統一。新規パターンを導入しない。
6. **YAGNI**: スコープは受け入れ条件①②③ + ブートストラップに限定。`Agent` 過剰付与を除去して YAGNI 遵守。
7. **シンプル化**: 「ゲートで守る」より「器を分ける」を採用（持続状態を上書きファイルから追い出す）＝より単純で根を断つ。`allowed-tools` は最小（`Skill` のみ）。
8. **破壊不変条件**: Win32 フック/ホットキー/IPC など「戻ってこない系」リスクは皆無（.md のみ）。唯一の失敗系（health-check ネスト起動失敗）は wedge せず、検知は Step 5 の `/health-check` ドッグフードで担保。

### 判断項目（実装前に最終確認）
- 現行 `RETROSPECTIVE.md` の `instantCommandMode ラッチ除去`（任意・本質的に不要と注記済み）を issue 起票するか。新機構の最初の適用例として**低優先 issue 起票を推奨**するが、ユーザーが「不要」と判断すれば除去のみでも受け入れ条件①は機構側で満たされる。

### 総評
- completeness: **高**（影響範囲網羅確認・要対処 1 件反映済み）
- 着手可否: **可**（plan 修正済み。`/implement` で着手可能）
