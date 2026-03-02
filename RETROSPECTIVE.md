# Retrospective — 大規模インデックス対応 3フェーズ最適化

## よかったこと

### 3フェーズを独立して検証しながら段階的に実装できた

Phase 1（Lazy UTF-32）→ Phase 2（並列構築）→ Phase 3（マスクキャッシュ）の順に、
各フェーズが独立して `cargo check / cargo test` を通過することを確認しながら進められた。
"最小限の変更で最大の効果を得る順" という計画の意図をコードに反映できた。

### `Option<Utf32String>` パターンが "possibly uninitialized" 問題を回避した

最初の実装案は `let name_u32_owned; name_u32_owned = ...` の条件付き初期化パターンだったが、
Rust のコンパイラが "possibly uninitialized" として拒否しうることに気づき、
`Option<Utf32String>` で保持して `.as_ref()` で借用する安全なパターンに置き換えた。
問題に気づいた時点で即座にリファクタしたため、コンパイルエラーを踏まず済んだ。

### DRY 原則のトレードオフを明示的に判断した

`char_bitmask` ロジックを `search.rs` と `indexer.rs` の2箇所に持つことは「2回まで許容」
の範囲内であると判断し、`query.rs` への抽出や `search.rs` → `indexer.rs` の依存追加を
避けた。KISS を優先した合理的な判断として記録しておく。

---

## 伸びしろ

### v2 キャッシュヒット時のアップグレードパスがない

v2 フォーマットが残っている間は起動のたびに `SearchEngine::new()` がマスクを計算する。
エントリに変更がなければ background rescan は保存しないため、v2 → v3 自動昇格は起きない。
長期的には「v2 ヒット時に v3 で上書き保存する」パスを追加すると完全に解消できる。

### `char_bitmask_for_cache` と `search.rs::char_bitmask` の二重管理

同一ロジックが2ファイルに存在する。3箇所目が増えたら `query.rs` に集約する。

---

## ネクストアクション

- [ ] 動作確認: `npm run tauri dev` で通常起動・インデックス再構築・検索動作を手動確認
- [ ] `cargo test --release -p snotra-core bench_ -- --ignored --nocapture` でパフォーマンス回帰がないことを確認
- [ ] `PERFORMANCE.md` に Phase 1-3 の効果（メモリ / 起動時間 / 検索遅延）を追記
