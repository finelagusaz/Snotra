# research: issue #562 — docgen 導入と CLAUDE.md 精密事実の doc コメント移行

## issue の要約

CLAUDE.md の「モジュール構成」節・API 列挙は実装の写しであり、改名・ファイル追加のたびに手動追従が要る（ドリフトは /health-check が事後検出）。精密事実を doc コメント（`///` / `//!` / TSDoc）へ寄せ、CLAUDE.md を指名参照へ痩せさせる。docgen（rustdoc / TypeDoc）は intra-doc link の切れを機械検査する検出器として移行を支える。

**issue は「実装から入らない・効果/意図を実態で検算し、移行範囲・機構の要否を合意してから計画」を要求**。合意は取得済み（下記）。

## 合意事項（着手時の 3 決定 + 派生）

| 論点 | 決定 | 出所 |
|---|---|---|
| 機構（lint/CI） | **lint deny + CI**（移行と独立に今入れる）。`broken_intra_doc_links` + `invalid_html_tags` を deny、各 crate opt-in、CI に `cargo doc` ジョブ | ユーザー選択 |
| Rust 移行範囲 | **責務一行を一括 `//!` 化**（`//!` を正とし CLAUDE.md 責務一行を痩せさせる） | ユーザー選択 |
| TS 側 | **TypeDoc 実導入**（`validation.invalidLink` + CI） | ユーザー選択 |

派生決定（本 research で確定）:
- **`--document-private-items` を CI で使う**: 一括 `//!` 化は bin crate（src-tauri / snotra-settings）の private モジュールにも `//!` を書く。private モジュールの doc とその intra-doc link を検査するには同フラグが要る。検算で使った権威的 invocation `cargo doc --workspace --no-deps --document-private-items` をそのまま採用。
- **非候補境界を守る**: 一括 `//!` 化でも、単一アイテムに帰属しない横断知識（config_watcher の複数不変条件・engine index_stale ledger・各 CLAUDE.md のクロスモジュール不変条件/チェックリスト）は CLAUDE.md に残す。移動するのは各モジュールの**責務宣言**だけ（判別軸＝密度ではなく単一アイテム帰属性）。

## 検算（仮説 → 実態）

### 仮説 A「CLAUDE.md は写しを持つ」→ 確認、ただし不均質
- `snotra-core/CLAUDE.md:39-54` = instant.rs 公開関数のフルシグネチャ列挙。**これらは既に `///` を持つ**（`split_args` の doc を実在確認）→ 真の重複＝写しの筆頭。移行＝CLAUDE.md 側を消し指名参照化（唯一のクリーンな dedup）。
- モジュール責務一行: 大半のモジュールは `//!` ゼロ → 知識は CLAUDE.md にしか無い → 移行＝`//!` を**新規記述**（重複削除ではない）。

### 仮説 B「docgen が検出器として効く＝実質価値」→ 実証、ただし範囲限定
- `cargo doc --workspace --no-deps --document-private-items` の**クリーン完全実行**（`rm -rf target/doc` 後）= **4 警告、すべて snotra-core**（exit 0、warn 既定ゆえ素通り）:
  1. `search.rs:346` `[`new_with_migemo`]` 未解決 → `[`Self::new_with_migemo`]`
  2. `search.rs:347` `[`new_with_cached_masks`]` 未解決 → `Self::` 前置
  3. `search.rs:440` `[`search_with_options`]` 未解決 → `Self::` 前置
  4. `search.rs:149` `Vec<u64>` を未閉 HTML タグ扱い → バッククォート
  - src-tauri（snotra）・snotra-settings は無警告。
- → 検出器は**実在する腐敗を即座に 3 件捕捉**。search.rs は既に `[`item`]` 記法を使い、`Self::` 前置漏れで静かに腐っていた（lint warn 既定 + CI に cargo doc 無し）。issue の「コンパイラを持たない機構に検出器」の実証であり、機構を「今入れる」の決定的論拠。
- **範囲限定**: intra-doc link は rustdoc レンダ内（`///` / `//!` / `include_str!` で取り込んだ md）でのみ解決。**リポジトリ直下 CLAUDE.md は cargo doc の視界外** → 検出器は **doc コメント→doc コメント**参照をカバー、**CLAUDE.md→コード参照はカバーしない**（`include_str!` の逃げ道は CLAUDE.md がエージェント文脈ガバナンス文書ゆえ非現実的）。
- → 移行の機械的勝ち筋は**検出ではなく co-location**（doc がアイテムと共に rename を旅する・同一 diff に現れ人間レビューに乗る）。intra-doc 検出器は別個の勝ち筋（doc コメント相互参照を守る）。残す指名参照はモジュール（`//!`）粒度に粗く保つ（symbol rename より module rename が稀ゆえ、検出器が届かない残余の腐敗を最小化）。

## 関連コード（実在確認済み）

### 機構
- `Cargo.toml`（workspace root）: `[lints]` 設定なし → `[workspace.lints.rustdoc]` 新設先。
- 各 crate Cargo.toml（`snotra-core/` `src-tauri/` `snotra-settings/`）: `[lints] workspace = true` の opt-in が必要（workspace 継承は自動でない）。
- `.github/workflows/ci.yml` の `rust-check` ジョブ（windows-latest）: `cargo doc` ステップ追加先。既存 step は check/test×3/clippy×2 + hooks selftest。
- 修正対象: `snotra-core/src/search.rs` の 4 箇所（上記）。

### Rust `//!` 移行surface
`//!` 無し = **44 rs ファイル**（再帰集計）。ただし移行対象は「CLAUDE.md モジュール構成に責務一行を持ち `//!` を欠くモジュール」に絞る:
- **snotra-core（12）**: binfmt, config, engine, error, folder, history, indexer, instant, query, search, ui_types, window_data（opener は `//!` 済み。lib.rs はクレート root で別扱い）
- **src-tauri（6）**: main, state, icon, indexing, config_watcher, ime（trace/monitor/working_set は `//!` 済み。commands/・platform/ は CLAUDE.md 集約バレット記述ゆえ個別 `//!` は任意）
- **snotra-settings（5 + tabs 7）**: main, app, font, hotkey_input, i18n（style/common は `//!` 済み）+ tabs/{general,search,index,visual,opener,instant,backup}。`*/mod.rs` は「宣言のみ」ゆえ最小 `//!` か skip
- 実質対象 ≒ 30 モジュール。**mod.rs・aggregate 記述の個別ファイルには価値の薄い `//!` を撃たない**（Phase 1 の判定基準）。

### TypeScript
- `ui/src` の `{@link}` 使用 = **0 件** → TypeDoc `validation.invalidLink` は純粋な将来ガード（Rust 側と非対称: 即時に捕まる腐敗が無い）。
- TSDoc `/** */` ブロック保有 = 19 ファイル（`lib/` プリミティブ中心。`exclusive.ts` / `ownedTimer.ts` は comment-guidelines の模範例）→ 機会的 thinning の素材はあるが TypeDoc 導入に必須ではない。
- package.json: `typescript ^6.0.2`、scripts に `typecheck`(tsc) / `build`(vite) / `test`(vitest)。typedoc は未導入。

## 技術的制約・リスク

- **intra-doc link の解決範囲**（上記 B）: CLAUDE.md 参照は機械検査されない。移行後も CLAUDE.md→コードの粗い指名参照は /health-check の事後検出に依存（受容する残余）。
- **TypeDoc ↔ TypeScript 6.0 互換**: TypeDoc は typescript に peer 範囲を張る。TS 6.0.2 は最新のため、**導入前に TypeDoc の対応バージョンを context7 / npm で確認**（未対応なら Phase 2 は blocker）。start-issue Step 3 の外部依存前提確認に該当。
- **`[lints.rustdoc] deny` が cargo doc を実際に fail させるか**: Cargo.toml の rustdoc lint は cargo doc に渡る（Rust 1.74+）。実装時にカナリア（意図的にリンクを壊す→cargo doc が error）で実証する（破壊不変条件の検知手段）。
- **ガバナンス文書の構造改変**: CLAUDE.md 責務一行を痩せさせる編集は `.claude/rules/governance-docs.md` の対象になりうる（セクション名・Step 番号の参照が腐る）。各 CLAUDE.md「モジュール構成」節の見出しは保ち、行内の責務記述のみ痩せさせる（節の改名・番号ずらしはしない）。
- **PostToolUse hook は cargo doc を走らせない**: ユーザーは CI を選択（hook のレイテンシ回避）。ゆえに編集時の即時ローカル検査は無く、リンク切れは CI（PR 時）で捕捉。開発者はローカルで `cargo doc` を手動実行可能。

## 未解決の疑問（plan で解消 or ユーザー確認）

- **範囲差分（23→44/実質~30）**: ユーザーの「一括 `//!` 化」決定は「23 ファイル」の枠組み。実数は倍近い。Phase 分割で吸収するが、Phase 1 を core のみに絞る余地あり（報告で off-ramp を提示）。
- **TypeDoc entryPoints 粒度**: `ui/src` 全体か `lib/` + `stores/` に絞るか（plan で既定を置く）。
