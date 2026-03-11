# 実装計画: Issue #238 ダーティインジケーター

## 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `snotra-settings/src/app.rs` | `TabId::has_changes()` 追加 + サイドバーラベル描画に `•` を付与 |

## 実装内容

### TabId::has_changes() 追加

```rust
fn has_changes(self, draft: &Config, saved: &Config) -> bool {
    match self {
        TabId::General => draft.general != saved.general || draft.hotkey != saved.hotkey,
        TabId::Search => draft.search != saved.search,
        TabId::Index => draft.paths != saved.paths,
        TabId::Visual => draft.visual != saved.visual || draft.appearance != saved.appearance,
        TabId::Opener => draft.openers != saved.openers,
        TabId::InstantCommand => draft.instant_commands != saved.instant_commands,
        TabId::Backup => false,
    }
}
```

### サイドバーのラベル描画を修正

```rust
let label = if tab.has_changes(&self.draft, &self.saved) {
    format!("{} •", tab.label(&self.tr))
} else {
    tab.label(&self.tr).to_string()
};
```

## 不変条件

- Backup タブは `has_changes` が常に `false` → `•` は表示されない
- 保存成功時: `saved = draft.clone()` で全フィールドが揃うため全タブの `•` が消える
- 破棄時: `draft = saved.clone()` で全フィールドが揃うため全タブの `•` が消える
- 各フィールドは `PartialEq` を実装済み（`Config` に `#[derive(PartialEq)]` あり）

## テスト方針

- ユニットテストなし方針（snotra-settings は egui UI コード）
- 検証コマンド: `cargo check -p snotra-settings`
- 手動確認: 各タブで値を変更 → そのタブのラベルに `•` が出る、保存/破棄で消える

## セルフレビュー

1. **対称コードパス**: 保存・破棄の両パスで `•` が消えることを確認済み
2. **影響範囲の網羅性**: 変更は `app.rs` 1ファイルのみ
3. **境界条件**: `Backup` タブの `false` ケースを明示
4. **リソース管理**: 新たなリソース生成なし
5. **既存パターンとの整合**: `has_changes()` は既存の `SettingsApp::has_changes()` と同じ比較パターン
6. **YAGNI 違反**: なし（issue 要件のみ実装）
7. **シンプル化**: `format!` での文字列生成は最小コスト。新たな状態なし
8. **破壊不変条件**: なし（純粋な表示追加）
