# plan.md — issue #144: snotra-settings を CI の cargo check / clippy 対象に追加

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `.github/workflows/ci.yml` | `cargo check` と `cargo clippy` に `-p snotra-settings` を追加 |

## 実装順序

単一フェーズ。`ci.yml` の2行を変更するだけ。

### 変更内容

```diff
- cargo check -p snotra-core -p snotra
+ cargo check -p snotra-core -p snotra -p snotra-settings

- cargo clippy -p snotra-core -p snotra -- -D warnings
+ cargo clippy -p snotra-core -p snotra -p snotra-settings -- -D warnings
```

## 不変条件

- `cargo test` に `snotra-settings` は追加しない（ユニットテストが存在しない）
- `frontend-check` ジョブには影響なし

## テスト方針

CI が通ることで検証完了。ローカルでは macOS のため `cargo check -p snotra-settings` は実行不可（Windows 依存）。

## SPEC.md 更新要否

不要。CI 設定の変更のみ。

---

## セルフレビュー

1. **対称コードパス**: `cargo check` と `cargo clippy` の両方に追加 — 対称性 OK
2. **影響範囲の網羅性**: `ci.yml` の2行のみ。`release.yml` は `snotra-settings` を既に個別ビルドしており変更不要
3. **境界条件**: N/A
4. **リソース管理**: N/A
5. **既存パターンとの整合**: 既存の `-p` フラグ列挙パターンに合致
6. **YAGNI 違反**: なし
7. **シンプル化**: これ以上シンプルにできない
8. **破壊不変条件**: なし。CI 設定のみの変更で本番コードに影響しない
