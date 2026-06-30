# plan — docs/ の as-built 監査・是正 (#410)

## 種別判定（AGENTS.md Step 0）

**doc 同期**（バグでも仕様変更でもない）。実装は正、`docs/architecture.md` が陳腐化。SPEC を実装に寄せた #404 と同系の as-built 訂正。挙動変更なし。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `docs/architecture.md` | 7 件の factual drift を是正（下記 F1-F7） |

他ファイル変更なし（dev-principles.md / build-commands.md は監査の結果 clean）。

## 実装内容（逐語 before→after）

### F1 — architecture.md:96（明確）
- before: `- 結果の表示/非表示は \`shouldShowResults\` メモシグナル（\`results().length > 0 && (!indexing() || interpKind() === "instant")\`）で制御`
- after: `- 結果の表示/非表示は \`shouldShowResults\` メモシグナル（\`results().length > 0\` を前提に \`switch(viewKind())\`: tool/folder は indexing 中でも常に表示、results は \`interpKind() === "instant" || !indexing()\`）で制御`

### F2a — architecture.md:14（明確）
- before: `  snotra-settings/        # egui 設定バイナリ（About タブ統合）`
- after: `  snotra-settings/        # egui 設定バイナリ（版数・About はサイドバー表示）`

### F2b — architecture.md:20（明確）
- before（末尾節）: `設定は egui ベースの別プロセスで（About 情報はタブとして統合）、\`config.toml\` ファイルを介して本体と連携する。`
- after: `設定は egui ベースの別プロセスで（版数・About 情報はサイドバーに表示）、\`config.toml\` ファイルを介して本体と連携する。`

### F3 — architecture.md:186（明確・mermaid 図注）
- before: `    SS->>SS: debouncedRefresh()<br/>requestAnimationFrame で合間`
- after: `    SS->>SS: debouncedRefresh()<br/>setTimeout で leading+trailing 50ms`

### F4 — architecture.md:127（明確・ワイヤー形式）
- before: `- バッチ形式: \`[count:u32 LE]\` + 各アイコン \`[status:u8][png_len:u32 LE][png_bytes]\``
- after: `- バッチ形式: \`[count:u32 LE]\` + 各アイコン \`[status:u8]\`（0=None / 1=Some）、status==1 のときのみ \`[png_len:u32 LE][png_bytes]\` が続く`

### F5 — architecture.md:142（明確・種別欠落）
- before: `- IPC: \`src-tauri/src/commands/instant.rs\` — クリップボード読み取り + \`launch_item_core\`（ShellExecuteW）で実行`
- after: `- IPC: \`src-tauri/src/commands/instant.rs\` — クリップボード読み取り + 種別分岐で実行（URL/Legacy は \`expand_instant_command\` → \`launch_item_core\`（ShellExecuteW）、Exec は \`launch_exec_core\`（exe + args 起動））`

### F6 — architecture.md:220-228 直後（軽微・概要図の内部整合）
状態図フェンスブロック（`└── IndexingMode（構築中）` の次行・既存バレット `- \`Escape\` は内側のモードから...` の前）に 1 バレット追加:
- add: `- 上図の括弧内は実装上 **2 軸 + オーバーレイ**に対応: NormalMode/CommandMode/InstantCommandMode は軸2 \`interpKind\`（plain/command/instant）、FolderExpansionMode/ToolSelectionMode は軸1 \`viewKind\`（folder/tool）。**IndexingMode は排他モードではなくオーバーレイ**（\`indexing\` はどのモードにも重なる）`

### F7 — architecture.md:105（軽微・不完全列挙）
- before: `- 起動時 UI 初期化は \`get_bootstrap_payload\` で \`visual\` / \`auto_hide_on_focus_lost\` / \`indexing\` / \`language\` を一括取得`
- after: `- 起動時 UI 初期化は \`get_bootstrap_payload\` で \`visual\` / \`general\`（auto_hide_on_focus_lost・auto_update）/ \`appearance\`（show_icons・visible_rows）/ \`language\` / \`indexing\` / \`instant_command_prefix\` / \`result_limit\` を一括取得`

## 実装順序

単一フェーズ。architecture.md に 7 箇所の Edit（独立・相互依存なし）。

## 不変条件

1. **コード不変**: 実装は触らない（doc を実装に寄せる）。
2. **逐語一致で編集**: 行番号でなく before テキストの文字列一致で Edit（#404 の教訓・plan の行番号はドリフトしうる）。
3. **過剰是正しない**: clean と判定した記述（i18n・型名・オープナー・自動更新・コヒーレンシ等）は触らない。F6/F7 は軽微だが verified factual のため最小修正で含める。
4. **内部整合**: F1 修正後の line 96 と F6 注記は、dev-principles.md の 2 軸 + オーバーレイ記述と一致させる（docs/ 内の DRY/整合）。

## テスト方針

doc-only のため自動テストなし。検証 = `docs/build-commands.md` カテゴリ外（Rust/TS 変更なし）。目視:
- 7 箇所の after が各コード根拠（research.md の file:line）と一致
- `git diff` で architecture.md のみ変更・他ファイル無改変

## SPEC.md 更新要否

不要（SPEC は #404 で是正済み。本タスクは docs/ のみ）。

## セルフレビュー（start-issue Step 5b）

1. **対称コードパス**: 該当なし（doc-only）。
2. **影響範囲の網羅性**: 3 ファイルを二枠組み（claim-by-claim A/B + 独立再導出 C）で監査。C が A の「一致」判定 3 件を回収＝枠組みの独立で盲点補完済み。dev-principles/build-commands は clean 確認。
3. **境界条件**: F6 は概要図のため最小注記に留める（restructure しない）。F3 は mermaid 図注内の機構名訂正。
4. **リソース管理**: 該当なし。
5. **既存パターンとの整合**: as-built 訂正のみ。新規記述の創作はしない。
6. **YAGNI**: clean 箇所に手を入れない。F6/F7 の軽微 2 件も「verified factual drift」に限定。
7. **シンプル化**: 全て既存記述の置換 + 1 注記追加。
8. **破壊不変条件**: コード不変のため runtime リスクゼロ。

### check スキル判定（plan-review Step 5a）

- `/plan-review`: 本タスクの**完全性が要件の部分は「監査」**であり、それを 3 体（claim-by-claim 2 + 独立再導出 1）で実施済み＝issue 指定の二枠組みが plan-review Step 2b の独立再導出そのもの。FIX plan は各 before→after をコードで裏取り済みの機械的 doc 編集。改めての fan-out は冗長。→ 監査フェーズが独立検証を兼ねる（インライン self-review で代替）。
- `/symmetric-check` `/race-check` `/cache-check` `/state-check`: 非該当（doc-only）。
