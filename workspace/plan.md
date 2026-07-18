# plan: issue #562 — docgen 導入と CLAUDE.md 精密事実の doc コメント移行

前提: `workspace/research.md` の検算・合意事項・範囲測定。挙動変更なし（ドキュメント・ビルド設定のみ）。plan-review（3 体並列 + 独立再導出）反映済み。

## 全体方針

**機構と移行を分離**（検算のスパイン）。Phase 0（機構）は移行と独立に単独で価値があり、それ自体で PR 化できる。Phase 1（Rust `//!` 移行）/ Phase 2（TypeDoc）/ Phase 3（規約同期）は機構の上に載る。各 Phase は検証 green 後にコミット（中断耐性・#431）。PR は Phase 境界で分割推奨。

**移行対象 = 30 モジュール**（3 体の偵察が独立収束）: snotra-core 12 / src-tauri 6 / snotra-settings 12。

## Phase 0 — Rust 機構ベースライン（intra-doc link 検出器）

**目的**: 既存 3 リンク切れを塞ぎ、doc コメント相互参照の切れを機械検査する検出器を CI に据える。

### 変更ファイル
1. `snotra-core/src/search.rs` — 既存 4 警告を修正（前例 `history.rs:188` の `[`Self::prepare_flush`]` で `Self::` 記法の有効性を実証済み）:
   - `346`: `[`new_with_migemo`]` → `[`Self::new_with_migemo`]`
   - `347`: `[`new_with_cached_masks`]` → `[`Self::new_with_cached_masks`]`
   - `440`: `[`search_with_options`]` → `[`Self::search_with_options`]`
   - `149`: `Vec<u64>` → `` `Vec<u64>` ``（rustdoc の help 提案と一致）
2. `Cargo.toml`（root） — 追加（現状 `[lints]` なし＝単純追加）:
   ```toml
   [workspace.lints.rustdoc]
   broken_intra_doc_links = "deny"
   invalid_html_tags = "deny"
   ```
3. `snotra-core/Cargo.toml` / `src-tauri/Cargo.toml` / `snotra-settings/Cargo.toml` — 各に `[lints]\nworkspace = true` を追加（3 crate とも既存 `[lints]` なし）。
4. `.github/workflows/ci.yml` — **`rust-check` ジョブ（windows-latest）** に step 追加。**src-tauri は windows 専用依存ゆえ ubuntu の frontend-check では build 不能——doc 検査は必ず windows の rust-check**。配置は 2 つ目の clippy step（`cargo clippy (e2e-webview-automation feature)`, L79-80）の後:
   ```yaml
   - name: cargo doc (intra-doc link check)
     run: cargo doc --workspace --no-deps --document-private-items
   ```
   `--document-private-items` は**必須**（Phase 1 の 30 個の新規 `//!` は bin の private モジュールに付き、フラグ無しだと link 検査対象外）。

### 実装順序
1. search.rs の 4 警告を修正 → `cargo doc --workspace --no-deps --document-private-items` が 0 警告になることを確認（deny 化の前提）。
2. Cargo.toml に lints 追加 → `cargo doc ...` が exit 0 のままを確認。`cargo check`/`clippy`/`test` が不変（rustdoc lint は rustdoc パスのみ）を確認。
3. **カナリア検証**（破壊不変条件の検知手段）: search.rs のリンク 1 本を故意に壊す → `cargo doc ...` が **error で落ちる**ことを実証 → 戻す。deny が実際に fail させることの一次証拠。
   - **検証済みフォールバック**: Cargo.toml lints が fail させない場合、CI step env に `RUSTDOCFLAGS: "-D rustdoc::broken_intra_doc_links"`（exit 101 実証済み）。ただし narrow flag は `invalid_html_tags` を warn のまま残すため、L149 修正が前提。
4. ci.yml に step 追加。

### 不変条件
- `cargo doc --workspace --no-deps --document-private-items` が **exit 0 かつ 0 警告**（Phase 0 完了時点）。
- deny 設定下でリンク切れが **error**（warn ではなく）（カナリアで実証）。
- rustdoc lint は rustdoc のみに作用し、`cargo check`/`clippy`/`test`・PostToolUse hook（clippy）は不変。

### テスト方針
- 追加テストなし（ビルド設定）。検証はカナリア + `cargo doc` の exit code。
- PostToolUse hook: root `Cargo.toml` 編集 → `cargo-check` + `hook-selftest`（selftest は selectChecks ロジックを検査。lints 追加は selftest を壊さない）。各 crate `Cargo.toml` → `cargo-check` のみ。ci.yml 編集 → 検査なし（沈黙は「走らなかった」＝合格ではない・手動 `cargo doc` で確認）。

## Phase 1 — Rust `//!` 移行 + CLAUDE.md 責務痩せ

**目的**: 各モジュールの責務宣言を `//!` へ寄せ、CLAUDE.md の責務一行を「`//!` を正とする」指名参照へ痩せさせる。

### 判定基準（3 層 canonicality + 第三カテゴリの明示）
- **`//!` = モジュール責務** / **`///` = per-item 契約** / **CLAUDE.md = クロスモジュール不変条件・チェックリスト**（comment-guidelines の配置基準）。
- **移す（→ `//!`）**: 各モジュールの責務宣言（トップ level bullet の「何を担うモジュールか」1〜数行）。
- **残す（CLAUDE.md）— 2 種を区別**:
  1. **横断知識**: 単一アイテムに帰属しない不変条件・チェックリスト（config_watcher の複数不変条件・engine index_stale ledger 等）。
  2. **第三カテゴリ＝単一モジュール実装詳細ネスト bullet**（search.rs「並列 Vec レイアウト」「構築の共通化」、config.rs「件数パラメータ」「旧キー移行」、opener.rs「依存方向」等）。単一アイテムに帰属するが「責務宣言」ではない。**今回のスコープ外＝CLAUDE.md に据え置く**（実装者が過剰に踏み込んで移動/削除しないよう明記）。
- **`///` への短縮（写しの dedup）**: `instant.rs`（`snotra-core/CLAUDE.md:39-54`）と `opener.rs`（`:20-23`）の**フルシグネチャ API 列挙は既存 `///` の写し** → `//!` ではなく `///` への指名参照に短縮（唯一のクリーンな dedup。opener.rs は既に `//!` 済みゆえ CLAUDE.md 側の API 列挙のみ短縮）。
- **`//!` を撃たない**: `*/mod.rs`（宣言のみ）、`commands/*.rs`・`platform/*.rs`（CLAUDE.md が集約バレット記述）、`build.rs`。
- **health-check Check 1 制約**: CLAUDE.md は**ファイル名を残す**（散文のみ痩せる。Check 1 が「記載ファイル名 ↔ 実ファイル」を照合するため）。

### 変更ファイル（crate 別サブフェーズ・計 30 モジュール）
- **1a. snotra-core（12）**: binfmt, config, engine, error, folder, history, indexer, instant, query, search, ui_types, window_data に `//!`。`snotra-core/CLAUDE.md` の各責務一行を痩せさせる（instant.rs シグネチャ列挙 → `///` 指名参照）。lib.rs は crate root（`//!` は任意・deny 属性は Cargo.toml 経由ゆえ不要）。
- **1b. src-tauri（6）**: main, state, icon, indexing, config_watcher, ime に `//!`。`src-tauri/CLAUDE.md` の責務一行を痩せさせ、横断不変条件（config_watcher の 3 不変条件等）は残す。
- **1c. snotra-settings（12）**: main, app, font, hotkey_input, i18n + tabs/{general,search,index,visual,opener,instant,backup} に `//!`。`snotra-settings/CLAUDE.md` を痩せさせる。

### 実装順序
crate 別に 1a → 1b → 1c。各サブフェーズ内で「`//!` 追加 → CLAUDE.md 痩せ（ファイル名保持） → `cargo doc` green → コミット」。1a でパターン確定（後続踏襲）。

### 不変条件
- `//!` は責務宣言（why/役割）に留め、実装の逐語訳を書かない（comment-guidelines 第一原則）。日本語既定・同一ブロック日英混在なし。
- CLAUDE.md「モジュール構成」節の**見出し・構造・ファイル名は保つ**。行内責務散文のみ痩せさせ、節の改名・番号ずらし・横断知識/第三カテゴリの削除はしない。
  - **注**: `.claude/rules/governance-docs.md` は自己申告で root CLAUDE.md 限定（frontmatter `paths: ["CLAUDE.md"]`）。crate CLAUDE.md はスコープ外だが、**同種の理由（節見出しが architecture.md:60/66/88・persistence-check SKILL.md:18・PERFORMANCE.md:62 から外部参照される）で同じ規律を手動適用する**。
- CLAUDE.md 痩せ後も、そのモジュールの横断不変条件・チェックリスト・第三カテゴリ bullet は残存。
- 各サブフェーズ後に `cargo doc ...` が 0 警告（`//!` 内の intra-doc link が新たに切れていない）+ 該当 crate `cargo test` green。

### テスト方針
- 追加テストなし（doc コメント + 散文）。検証は `cargo doc` 0 警告 + `cargo test`（PostToolUse hook が rs 編集で clippy + 該当 crate test を発火）。
- `*.md`（CLAUDE.md）編集は PostToolUse 検査なし。整合は目視 + /health-check Check 1（ファイル名照合・サイクル末）。

## Phase 2 — TypeDoc 導入 + 機会的 TS thinning

**目的**: TS 側 doc コメント相互参照の `{@link}` 切れを CI で検査（将来ガード。現状 `{@link}` 0 件ゆえカナリア対象が無く、**設定の正しさだけが guard の生死を決める**）。

### 事前確認（済）
- **TypeDoc ↔ TypeScript 6.0 互換 = OK**: typedoc `0.28.20`（latest）の peerDeps が `... || 6.0.x`、root `package.json` は `typescript ^6.0.2`（実インストール 6.0.3）→ 範囲内。**blocker なし**（research のリスク解消）。

### 変更ファイル
1. `package.json` — devDependencies に `typedoc`（`^0.28`）、scripts に `"docs:check": "typedoc"`。
2. `typedoc.json`（新規）:
   ```json
   {
     "entryPoints": ["ui/src/lib", "ui/src/stores"],
     "entryPointStrategy": "expand",
     "emit": "none",
     "validation": { "invalidLink": true },
     "treatValidationWarningsAsErrors": true
   }
   ```
   - **`entryPointStrategy: "expand"` 必須**: ディレクトリ渡しは既定 `resolve` が `<dir>/index.ts` を探し失敗（`ui/src/lib/index.ts`・`stores/index.ts` は不在）。
   - **`treatValidationWarningsAsErrors: true` 必須**: `validation.invalidLink` は検査を**有効化するだけ**で warn 止まり（exit 0）。これが無いと `{@link}` 切れで CI が落ちず**張り子の検出器**になる（Rust の warn/deny と同型・二体独立収束・context7 確認）。`--emit none` は validation を無効化しない。
3. `.github/workflows/ci.yml` — **`frontend-check` ジョブ（ubuntu）** に `npm run docs:check` step 追加（npm ci の後）。
4. （機会的・任意）`ui/CLAUDE.md` lib/ 節のうち、既存 TSDoc（`exclusive.ts`/`ownedTimer.ts` 等 = comment-guidelines 模範例）が既に語る契約の写しを指名参照へ痩せさせる。**横断規約（状態モデル 2 軸・Blob URL 不変条件 = 別セクション）は残す**。

### 実装順序
typedoc devDep + config → `npm run docs:check` が green を確認（現状 `{@link}` 0 ゆえ即 green の想定）→ **カナリア**（一時的に不正な `{@link}` を書く→exit 非 0 を確認→戻す。設定の正しさを実証）→ CI step → 機会的 thinning。

### 不変条件
- `npm run docs:check`（`treatValidationWarningsAsErrors`）が正常時 exit 0、`{@link}` 切れで exit 非 0（カナリアで実証）。
- `package.json` 編集は PostToolUse hook-selftest を発火（typedoc devDep 追加は無害）。typedoc.json は検査対象外（手動確認）。
- TypeDoc は typecheck/build/test の既存 CI に干渉しない（`emit: none`）。

## Phase 3 — 規約同期（独立再導出が検出した欠落）

**目的**: 移行後の SSOT（`//!` = 責務の正）を規約に反映し、`/health-check` のドリフト検知を Warning させない。

### 変更ファイル
1. **`AGENTS.md` L71**（条件別チェック表の `| ファイル（.rs/.ts/.tsx）を追加/削除 | 対応するサブディレクトリ CLAUDE.md のモジュール構成を更新する |`）— issue の 期待効果 #3 そのもの。「`//!` に責務を書き、CLAUDE.md はファイル名 + 指名参照を維持」へ改める。
   - **注**: AGENTS.md 編集は `.claude/rules/governance-docs.md` を正当に自動配送（paths に `AGENTS.md`）→ 名前 + 序数で参照を数える。当該は表の 1 行で構造リスク低（Step 番号ずらしなし）。
2. **`docs/comment-guidelines.md` L40**（`//!` 節）— 「CLAUDE.md のモジュール責務は `//!` を正準とする」を追記。L35（散文→doc コメント指名参照）は既に本移行を支持。
3. **`docs/build-commands.md`（必須）** — 検証コマンドの SSOT。`cargo doc --workspace --no-deps --document-private-items` を該当カテゴリへ、typedoc コマンドを該当カテゴリへ追記。CI/CD 対応表に 2 行追加。**未更新だと `/health-check` Check 5（カテゴリブロック照合）・Check 10（必須コマンド ↔ workflow）が Warning**。
4. （任意・要合意）`.claude/rules/{snotra-core,snotra-settings,src-tauri}.md` に「モジュール責務は `//!` を正とする」一行（.rs 編集時に自動配送され発見性が上がる）。**rules はエージェント設定ゆえチーム憲章に従い合意してから**。

### 不変条件
- 規約と実装の SSOT が一致（`//!` = 責務の正）。`/health-check` Check 5/10 が Warning しない。
- `AGENTS.md` の Step 番号・節見出しの序数参照を壊さない（governance-docs.md）。

### テスト方針
- `*.md` 編集は PostToolUse 検査なし。検証は /health-check（サイクル末）+ 目視。AGENTS.md 編集は governance-docs.md 自動配送で参照数え上げ。

## SPEC.md 更新要否

**不要**。挙動変更なし（IPC 契約・状態遷移・スコア計算・設定キー・データフォーマットに変化なし）。ドキュメント配置・ビルド設定のみ。

## CI コスト注記

- `package.json` / `**/Cargo.toml` の変更は `e2e.yml`（E2E & Smoke workflow）をトリガー。設定のみの docgen 変更でも full E2E が起動する（想定内・skip-ci ラベルは使わない）。

## セルフレビュー

### plan-review（Step 5a）要対処の解消
1. **TypeDoc warn/fail 罠** → Phase 2 に `treatValidationWarningsAsErrors: true` を必須明記。解消。
2. **entryPoints 解決失敗** → Phase 2 に `entryPointStrategy: "expand"` を必須明記。解消。
3. **第三カテゴリの宙吊り** → Phase 1 判定基準に「単一モジュール実装詳細ネスト bullet はスコープ外＝据え置く」を明記。解消。
4. **規約同期の欠落**（独立再導出が検出）→ Phase 3 新設（AGENTS.md L71 / comment-guidelines L40 / build-commands.md 必須 / rules 任意）。解消。
- 軽微: CI 配置（2 つ目 clippy 後・windows rust-check・--document-private-items）、governance-docs 引用訂正、health-check Check 1 ファイル名保持、e2e トリガー、RUSTDOCFLAGS フォールバック — すべて本文へ織り込み済み。

### 独立導出との差分（Step 2b）
- **一致（完全性の証拠）**: 30 モジュール集合・4 警告・Self:: 修正・treatValidationWarningsAsErrors 罠・ネスト不変条件据え置き が独立に再一致。
- **漏れ（導出∖plan・反映済）**: Phase 3 規約同期一式、opener.rs の `///` 写し、health-check Check 1 制約、e2e トリガー。
- **スコープ過剰（plan∖導出）**: なし。

### 5b 3 観点（plan-review 非対象）
1. **境界条件**: (a) 責務一行 + ネスト不変条件を両持ちのモジュール（config.rs/search.rs）= トップ bullet のみ痩せ、ネストは据え置き（検証: 痩せ後も CLAUDE.md にネスト bullet 残存を目視）。(b) 純責務一行のみ（folder.rs/ime.rs）= クリーン移行。(c) 機構のカナリア = リンクを故意に壊し fail を確認（両検出器の境界テスト）。(d) health-check Check 1 = ファイル名保持を Phase 1 各 crate 後に確認。
2. **シンプル化の挑戦**: 機構は Cargo.toml lints + CI が最小（RUSTDOCFLAGS はフォールバックに留める）。`//!` 一括は合意済みだが mod.rs/aggregate/build.rs を除外して過剰を削減済み。TypeDoc は `{@link}` 0 で即時価値ゼロだが合意済みの将来ガード——`treatValidationWarningsAsErrors` で「張り子」化を回避し、最小構成（emit none・entryPoints 2 ディレクトリ）に留める。第三カテゴリ据え置きで CLAUDE.md 側の変更面を最小化。
3. **破壊不変条件 + 検知手段**:
   - 検出器が実際に fail するか（張り子でないか）→ **カナリア**（Rust: リンク破壊で cargo doc error / TS: 不正 `{@link}` で exit 非 0）。RUSTDOCFLAGS exit 101 実証済み。
   - CLAUDE.md がファイル名を失う → /health-check Check 1（Phase 1 各 crate 後）。
   - build-commands.md ドリフト → /health-check Check 5/10（サイクル末）。
   - 挙動変更の混入 → cargo test / vitest 全 green（コードロジック不変）。

### 着手可否: **可**（要対処 4 件を plan へ反映済み）。Phase 0 から実装開始。
