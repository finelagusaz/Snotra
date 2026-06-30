# research — docs/ の as-built 監査 (#410)

## issue の要約

#404 は SPEC.md のみを as-built 監査した。同種の陳腐化が `docs/` にも潜む可能性。`docs/architecture.md` / `docs/development-principles.md` / `docs/build-commands.md` を #404 と同じ二枠組み（ファイル分割監査 + 独立再導出）で監査し、実装と矛盾する記述・陳腐化を是正する。**実装事実の SSOT はコード**。

## 監査手法（issue 指定の二枠組み）

3 体の Explore を並列起動:
- **A**: architecture.md を claim-by-claim 監査（成果物監査）
- **B**: development-principles.md + build-commands.md を claim-by-claim 監査
- **C**: **独立再導出**（コードから as-built 事実を先に列挙 → docs と照合。枠組みの独立）

→ A が「一致」と判定した 3 箇所（アイコン形式・instant 実行経路・状態モード）を **C が回収**。#404 の「3 分割監査が clean と報告した範囲を独立再導出が回収」と同一構造（AGENTS.md Step 2「枠組みの独立 > 実行の独立」の再実証）。全 finding は主エージェントがコードで裏取り済み。

## 検出結果サマリ

| # | doc 位置 | 内容 | 検出 | 裏取り | 確度 |
|---|---|---|---|---|---|
| F1 | architecture.md:96 | `shouldShowResults` 式が旧 1 式（2 軸 `switch(viewKind())` を欠く） | 主/A/C | search.ts:82-95 ✓ | 明確 |
| F2 | architecture.md:14,20 | 「About タブ統合」は虚偽（About はサイドバー・タブは 7） | A | app.rs:424-441 + TabId 7値 ✓ | 明確 |
| F3 | architecture.md:186 | mermaid 図注 `requestAnimationFrame`（実体 `setTimeout` leading+trailing 50ms） | C | search.ts:144-159 ✓ | 明確（図注） |
| F4 | architecture.md:127 | アイコンバッチ形式が None 条件分岐を欠く（`png_len/bytes` は status==1 のみ） | C | icon.rs:101-132 ✓ | 明確（ワイヤー形式） |
| F5 | architecture.md:142 | instant 実行が Exec 種別（`launch_exec_core`）を欠落（URL/Legacy のみ記述） | C | instant.rs:58-71 ✓ | 明確（種別欠落） |
| F6 | architecture.md:220-228 | 状態図が `IndexingMode` を排他モードとして併置（実体はオーバーレイ）+ 2 軸混在 | C | search.ts:67-95 ✓ | 軽微（概要図・SPEC §8.6 参照） |
| F7 | architecture.md:105 | `get_bootstrap_payload` のフィールド列挙が不完全（4 項目のみ） | A | config.rs:21-52 ✓ | 軽微（不完全列挙） |

**dev-principles.md / build-commands.md は明確な矛盾 0 件**（B が i18n 例・instant ラッチ廃止・HistoryStore.top_n live-read・npm/cargo コマンド実在・CI 対応表まで全件一致を確認）。

## 関連コード（裏取り根拠）

- `ui/src/stores/search.ts:61-95`（isInstantPrefix / viewKind / interpKind / shouldShowResults switch）、`:144-159`（setTimeout デバウンス）
- `src-tauri/src/icon.rs:101-132`（encode_batch_binary・コード自身のフォーマットコメントが `If status == 1:` を明記）
- `src-tauri/src/commands/instant.rs:58-71`（Url→launch_item_core / Exec→launch_exec_core / Legacy→launch_item_core）
- `src-tauri/src/commands/config.rs:21-52`（BootstrapPayload の全フィールド）
- `snotra-settings/src/app.rs:424-441`（版数/About のサイドバー描画）+ `:51-71`（TabId 7値・About タブなし）

## 技術的制約

- doc-only（コード・テスト・IPC への影響なし）。修正対象は **architecture.md のみ**（他 2 ファイルは clean）。
- 横断パターン・データフロー記述の as-built 訂正。挙動変更なし。

## 未解決の疑問

なし。全 7 件コードで裏取り済み。C の盲点自己申告（SPEC §8.6 本文・release.yml・updater エンドポイント等）は今回の docs/ 監査スコープ外、または factual 矛盾の積極証拠なし。
