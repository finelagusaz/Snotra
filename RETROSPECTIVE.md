# Retrospective — ローマ字→かな検索（migemo/kana マッチ）実装

## よかったこと

### 計画→実装→レビューの3段階が機能した
`workspace/plan.md` で Phase 1〜6 を計画し、8コミットで段階的に実装。計画段階で SoA レイアウト（並列 Vec）への kana_lower_names 追加、キャッシュ非永続化（INDEX_CACHE_VERSION バンプ回避）、設定 UI の設計まで決めていたため、実装フェーズで大きな設計変更が不要だった。

### レビューが3段階のバグを段階的に検出した
1. **内部レビュー（Medium）**: コメント不足・技術的負債（HistoryBoostConfig の命名）を検出
2. **P1 レビュー**: incremental cache の None→Some 遷移（min_chars 跨ぎ・ASCII 残留解消）を検出
3. **デザインレビューチェックリスト**: ローマ字→かな変換の非単調性（"kan"→"かん", "kana"→"かな"）を検出

各段階で修正が完了してから次のレビューに進んだため、問題が積み重ならなかった。

### 技術的負債を即時解消した
`HistoryBoostConfig` → `SearchOptions` のリネームを「後で」ではなく同サイクル内で実施。ユーザーの「技術的負債を残すのはやめよう」という判断が正しく、リネームは5ファイル・20箇所程度で収まった。

---

## 伸びしろ

### incremental cache のガード設計に3イテレーション要した
`prev_migemo_enabled: bool` → `prev_had_kana_query: bool` → `prev_kana_query: Option<String>` と3回修正した。根本原因は「ローマ字→かな変換が入力伸長に対して非単調」という性質を初回設計で認識していなかったこと。

**教訓**: キャッシュ再利用ロジックを設計するとき、各述語の「入力が伸びたとき出力はどう変わるか」を最初に分析すべきだった。これは `/cache-check` スキルの Step 2 として定式化済み。

### wana_kana のバージョン・API を事前に確認しなかった
`wana_kana = "0.4"` → 実際は `"4.0"`、`wana_kana::to_hiragana()` → 実際は `ConvertJapanese` trait。crates.io を確認せず計画書のまま実装に入ったため、2回のコンパイルエラーで手戻りが発生した。

**教訓**: 外部クレートを初めて使う場合、cargo add で最新バージョンを確認し、docs.rs で API を確認してから実装に入る。これは一般的な慎重さの問題で CLAUDE.md に追記するほどではない。

### 計画書の Phase 構成にレビュー・キャッシュ検証フェーズがなかった
計画書は実装フェーズ（Phase 1〜6）のみで、incremental cache への影響分析フェーズが含まれていなかった。結果として実装後のレビューで初めて問題が発覚した。

---

## ネクストアクション

- [ ] PR #215 をマージする（全テスト 281 pass、clippy clean 確認済み）
- [ ] 手動確認: migemo_enabled=true で「dokyu」入力 → 「ドキュメント」系エントリがヒットすることを検証
- [ ] 手動確認: migemo_enabled=false（デフォルト）で kana マッチが発動しないことを検証
- [ ] 手動確認: snotra-settings の Migemo セクション（checkbox + DragValue）が正しく表示・保存されることを検証
