# plan.md — issue #357: Rust 変更時の必須検証を SSOT とスキルで揃える

採用方針: **Option A**（SSOT を必須に昇格・ユーザー承認済み）。`type:docs`/`chore`（コード変更なし）。

## 変更ファイル一覧

### 1. `docs/build-commands.md` カテゴリ A（11-18行）— 必須に昇格

clippy・`cargo test -p snotra-core` を「追加検証」から「必須」へ格上げし、criterion 3 の判断理由を明記。

変更後（イメージ）:

```
### A. Rust ファイル（`*.rs`）を変更した場合

​```bash
cargo check -p snotra-core -p snotra -p snotra-settings                  # 必須: Rust 全 crate 型チェック
cargo clippy -p snotra-core -p snotra -p snotra-settings -- -D warnings  # 必須: lint（全 .rs 変更）
cargo test -p snotra-core                                                # 必須（snotra-core を変更した場合）: 純ロジック層 TDD
​```

- **`cargo test -p snotra-core` の必須/任意**: `snotra-core`（純ロジック層）を変更した場合は**必須**（TDD 重視。PostToolUse フックも snotra-core 編集時に自動実行）。`snotra` / `snotra-settings` のみの変更ではローカル任意（純ロジック不変のため。CI の rust-check が PR で常に実行し担保）
- 上記 3 コマンドはいずれも CI（`ci.yml` rust-check）で PR 自動実行される（「CI/CD メモ」の対応表参照）。PostToolUse フック（`.claude/settings.json`）も `.rs` 編集で clippy、`snotra-core` 編集で core テストを自動発火する
- `snotra-settings` を含めるのは egui ネイティブウィンドウ側の型壊れも検知するため
```

### 2. `.claude/skills/implement/SKILL.md` Step 4（51行）— 入口固有の上書きを削除

旧:
```
- カテゴリ A（Rust 変更）の追加検証（`cargo clippy` / `cargo test`）は本スキルでは**必須として扱う**（最初の失敗で停止するチェーン実行を推奨）
```
新:
```
- カテゴリ A（Rust 変更）の clippy・`cargo test -p snotra-core` も SSOT 上「必須」（最初の失敗で停止するチェーン実行を推奨）
```
→ 「本スキルでは必須として扱う」（SSOT を上書きする入口固有の表現）を撤去し、「SSOT 上『必須』」（権威を SSOT に委譲）へ。criterion 2 を満たす。

### 3. `.claude/skills/deps-update/SKILL.md`（39行）— 用語を新表現に合わせる

旧: `- カテゴリ A: 必須＋追加検証（Rust 全 crate チェック・clippy・core テスト）`
新: `- カテゴリ A: 必須（Rust 全 crate チェック・clippy・core テスト）`
→ 廃止語「追加検証」を除去。挙動不変（deps-update は依存更新が全 crate に波及しうるため三つとも常時実行で正しい）。

## 実装順序（依存なし・並行可）

1. `docs/build-commands.md`（SSOT 本体）を先に確定 → これが他の参照元
2. `.claude/skills/implement/SKILL.md`
3. `.claude/skills/deps-update/SKILL.md`

## 不変条件

- **SSOT 単一性**: コマンド本体は `docs/build-commands.md` のみが保持。スキル側は名前で参照するのみ（重複定義を作らない）。implement/deps-update のいずれも具体的なコマンド引数を再掲しない。
- **入口非依存（criterion 2）**: implement・deps-update・フック・CI の各入口が同一の「必須」水準を指す。どの入口から読んでも Rust 変更時の必須 = check + clippy +（snotra-core 変更時）core test。
- **後方互換（壊しても回復可能な性質）**: 本変更は文書のみ。実行系（フック・CI・workflow）に変更を加えないため、誤記しても CI 実体は不変＝検証ゲートは壊れない。最悪ケースでも「文書の誤り」に留まり、システム不変条件（ビルド・リリース）には波及しない。
- **health-check Check 10 整合**: 昇格後に「必須」となる clippy・core テストは `build-commands.md:92` の対応表で `ci.yml` rust-check に既に紐づく。Check 10（必須コマンド↔workflow ドリフト検知）は引き続き通る。

## テスト方針

- コード変更なし（`.rs`/`.ts`/`.tsx` 非該当）。`docs/build-commands.md` カテゴリ A〜D の自動検証は発火しない。
- **検証＝記述の自己整合（目視 + grep）**:
  1. `grep -n '追加検証'` で「追加検証」という廃止語が build-commands.md カテゴリ A / implement / deps-update から消えたことを確認（design/plan の歴史記録 `docs/superpowers/**` には残るが意図的に不変）。
  2. implement Step 4 から「本スキルでは必須として扱う」が消え、「SSOT 上『必須』」に置換されたことを確認。
  3. build-commands.md カテゴリ A に clippy・core テストが「必須」コメント付きで列挙され、criterion 3 の判断理由（snotra-core 変更時必須／他 crate 任意）が明記されていることを確認。
  4. 受け入れ条件 3 項目との突き合わせ（下記）。

### 受け入れ条件チェック
- [ ] 必須検証コマンドが build-commands.md と implement/SKILL.md で一致（clippy + core test が両者で「必須」を指す）
- [ ] 入口によって検証水準が変わらない（implement・deps-update・フック・CI が同一水準）
- [ ] `cargo test -p snotra-core` の必須/任意の判断理由が build-commands.md に明示

## SPEC.md 更新要否

**不要**。SPEC.md はプロダクト挙動（IPC 契約・状態遷移）を規定する文書であり、検証コマンドの運用基準（build-commands.md / スキル）は対象外。本変更はエージェント運用ドキュメントの整合であり、プロダクト挙動に一切影響しない。

## セルフレビュー

### 5a. plan-review（Explore サブエージェント並列検証）
- **影響範囲**: 3 ファイルで完全網羅（Agent 1）。`追加検証`/`必須として扱う`/`カテゴリ A` の全出現を分類し、計画外の入口なしを確認。B/C/D には「追加検証」概念が存在せず横断的問題ではない。
- **事実主張**: 4 点すべて実ファイルと一致（Agent 2）。フック粒度（settings.json 23-28）、CI rust-check（ci.yml 66/69/72）、health-check Check 10 非破壊、deps-update 挙動不変。
- 要対処なし → 計画修正なし。

### 5b. セルフレビューチェックリスト
1. **対称コードパス**: ドキュメント変更で対称ペアなし（5a で確認）。N/A
2. **影響範囲の網羅性**: grep で全入口を確認、3 ファイルで網羅（Agent 1）。✓
3. **境界条件**: 「snotra-core 変更時＝必須／他 crate のみ＝任意」の境界を criterion 3 で明示。✓
4. **リソース管理**: 新規リソース・フラグ・プロセスなし。N/A
5. **既存パターン整合**: SSOT 単一参照パターンを踏襲。新パターン導入なし。✓
6. **YAGNI**: 3 ファイルの最小変更。要求範囲を超えない。✓
7. **シンプル化**: 新状態なし。むしろ implement スキルの「入口固有の上書き」を撤去して単純化（SSOT へ権威を一本化）。✓
8. **破壊不変条件**: 変更は文書のみ。実行系（フック・CI・workflow）は不変＝検証ゲート自体は壊れない。誤記しても CI 実体は不変のため最悪でも「文書の誤り」に留まる。検知手段: grep（廃止語の消失確認）＋ 受け入れ条件 3 項目チェック ＋ 実装時の code-reviewer。✓

