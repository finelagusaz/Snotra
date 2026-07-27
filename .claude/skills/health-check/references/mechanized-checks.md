# 機械化済みチェックの対応表（旧 Check → G 番号）

`/health-check` の本体から退去させた履歴。**読むのは「Check N はどこへ行ったか」を問うときだけでよい**——通常の実行は `npm run governance:check` を回せば足りる。**Check 番号は振り直さない**（序数参照の腐敗を避けるため）。欠番の行き先はこの表である。

| 旧 Check | 行き先 | 備考 |
|---|---|---|
| Check 2 — `docs/architecture.md` にファイル単位モジュール表が再導入されていないか | G2（#587） | 責務宣言の正本は `//!` / TSDoc、CLAUDE.md は索引 + 横断不変条件（#562）という設計の回帰検知 |
| Check 3 — `AGENTS.md` ドキュメント参照の実在性 | G3（#587） | 対象はガバナンス文書群全体に一般化された |
| Check 4 — `SPEC.md` セクション番号の連続性 | G4（#587） | 番号連続性に加え、リポジトリ内の `SPEC §N.x` 参照の実在も検査対象 |
| Check 6 — `docs/development-principles.md` 参照の実在性 | G3（#587） | Check 3 と同様に一般化された |
| Check 8 — `.claude/rules/` パスパターンの有効性 | G7（#587） | マッチ 0 件の検知。glob 意味論が harness の配送判定の近似であることはスクリプト側に明記済み |
| Check 9 — スキル定義の整合性 | G8（#587） | #767 で母集団を `disable-model-invocation: true` の skill へ絞った |
