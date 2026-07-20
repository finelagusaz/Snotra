# docs/superpowers — 歴史資料（非規範）

このディレクトリは**歴史資料**であり、**現在の仕様ではない**（#589 で宣言）。

- 内容は各時点の設計書（`specs/`）と実装計画（`plans/`）のスナップショット。「実装中」「計画待ち」等の状態記述は**書かれた当時のもの**で、更新されない
- **鮮度維持の対象外**: `/health-check`・`governance:check` はこのディレクトリを検査しない（`scripts/governance-check.mjs` の母集団除外）。ここの記述と実装の乖離は欠陥ではない
- 現在の正本はこちら: 仕様 = `SPEC.md` / アーキテクチャ = `docs/architecture.md` / 精密な実装契約 = 各ファイルの `//!`・`///`・TSDoc / 運用 = ルート `CLAUDE.md`・`AGENTS.md`
- 起票済み issue へ反映された設計記録（例: `specs/2026-07-19-doc-governance-design.md` → #586〜#589）を含む。決定の経緯を辿るときの入口として残す（削除しない）
