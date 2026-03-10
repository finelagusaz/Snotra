# Retrospective — 設定バックアップ機能 (#226)

## よかったこと

### スコープ絞り込みで YAGNI を徹底した
issue の3段階優先度（トースト通知・ドキュメント・エクスポート/インポート）から「config.toml のみ、バックアップタブ追加」にスコープを絞り、zip/暗号化なしの素の TOML コピーで実装した。

### 既存パターンの再利用が効いた
PickerState（index.rs）、open::that（About タブ）、draft/saved 二重状態モデル（app.rs）を再利用。新規の抽象化やクレート追加なし（GetLocalTime 用の Windows feature 1件のみ）。

### 手動テストで UX 問題を早期に発見・修正した
ユーザーとの手動テスト中にメッセージ表示の問題2件（複数行はみ出し、二重表示）を発見し、即座にインライン表示に統一。フッター vs インラインの設計判断を `snotra-settings/CLAUDE.md` に教訓化した。

---

## 伸びしろ

### デシリアライズ経路の後処理パイプラインを見落とした（P1）
`Config::from_toml_str()` + `validate()` で十分と判断し、`load()` が持つマイグレーションパイプライン（`migrate_additional_to_scan` 等）を考慮しなかった。**修正**: `apply_migrations()` を抽出し共用化。**教訓**: `snotra-core/CLAUDE.md` に追記済み。

### UTC/ローカル時刻の区別を意識しなかった（P3）
`SystemTime::UNIX_EPOCH` からの秒数→日付変換が UTC であることを見落とした。**修正**: `GetLocalTime` Win32 API でローカル時刻を取得。

### ステータスメッセージの表示先を設計段階で決めなかった（P4）
フッターの `status_timer` を安易に流用した結果、複数行はみ出し→インライン追加→二重表示と2回の手戻りが発生。最初から「Backup タブは draft/saved に参加しないからインライン表示」と判断すべきだった。**教訓**: `snotra-settings/CLAUDE.md` に追記済み。

---

## ネクストアクション

- [ ] PR #232 をマージする
