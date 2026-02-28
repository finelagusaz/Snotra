# Retrospective — 最適化サイクル（perf/v0_9）

## よかったこと

### 調査→計画→実行の3段階フロー

`deep-research-report.md` → `research.md`（現状照合）→ `plan.md`（フェーズ分割）→ 実装の流れが機能した。計画段階で「実装済み」「非該当」を明確に仕分けたことで、不要な作業をゼロにできた。

### Windows 非対応を原典で即確認

`backgroundThrottlingPolicy` がスキーマエラーになった時点で推測を重ねず、`~/.cargo/registry` の `tauri-utils` ソースを直接参照して "Linux / Windows / Android: Unsupported" を確認した。ビルドエラー1回で結論が出た。

### 計測コマンドの絞り込み

最初の `Get-Process msedgewebview2` がシステム全体のプロセスを拾う問題を、CommandLine フィルタ（`*snotra*`）で即座に修正できた。WebView2 はブローカー経由で起動するため PID ツリーでは追えないという構造的な理由まで説明できた。

### CLAUDE.md の定期メンテナンス

セクション並び替え・文言短縮・冗長バレット削除を「分析→提案→承認→実施」の流れで進め、160 行をコンパクトに保てた。

---

## 伸びしろ

### API の配置場所をビルド前に確認する

`backgroundThrottlingPolicy` を `app` 直下に書いてビルドエラーを出す前に、JSON スキーマ（`$schema` 参照先）かソースコードで正しい配置場所を確認すれば1ビルド分の時間を節約できた。Tauri の設定項目は `app` / `app.windows[]` / `app.security` / `bundle` など階層が深く、ドキュメントより `gen/schemas/` や `tauri-utils/src/config.rs` が信頼できる。

---

## ネクストアクション

なし（全フェーズ完了。フェーズ 4 は Windows 非対応のため中止・結論確定）
