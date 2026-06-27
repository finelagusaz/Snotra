# plan — issue #399 設定UIの見出しに Semibold ウェイトを適用する

## 方針（設計の核）

`apply_type_ramp` の SSOT レバーを再利用する。`TextStyle::Heading.family` を Semibold ファミリへ向けるだけで、`section_heading` と `modal_header`（クレート内で `ui.heading()` を呼ぶ唯一の2箇所）が同時に Semibold 化する。本文・副文（Body/Button/Small）は size のみで Regular のまま。

graceful degrade のため `configure_fonts` が「Semibold フェイスを1つ以上登録できたか」を **bool で返し**、`run()` がそれを `apply_type_ramp(ctx, heading_semibold)` に渡す。フォント不在時は `Heading.family` を切り替えず Proportional（Regular）のまま＝panic なし・退行なし。

## 変更ファイル一覧

### 1. `snotra-settings/src/font.rs`
- `pub const SEMIBOLD_FAMILY: &str = "semibold";` を追加（family キーの SSOT。style.rs が参照）。
- `configure_fonts(ctx) -> bool` にシグネチャ変更（`#[must_use]`）。既存の jp_font 登録は不変。末尾で Semibold ファミリ登録の結果（bool）を返す。
- Semibold ファミリ登録（jp_font 登録の**後**＝Proportional 確定後に実施）:
  - 候補 `&[(path, face_index, FontTweak)]`:
    - `("C:\\Windows\\Fonts\\seguisb.ttf", 0, FontTweak::default())` — Segoe UI Semibold（Latin）
    - `("C:\\Windows\\Fonts\\YuGothB.ttc", 2, FontTweak{ y_offset_factor: 0.3, .. })` — Yu Gothic UI Semibold（JP）
  - 読めた候補を `font_data` に `semibold_0`, `semibold_1`… として登録し、名前を `names` に push。
  - `names` が空 → `false` を返す（ファミリ未登録）。
  - 非空 → `family = names ++ Proportional の fallback チェーン clone` を `FontFamily::Name(SEMIBOLD_FAMILY.into())` として登録し `true` を返す（欠字防止のため Proportional を suffix に連結）。

### 2. `snotra-settings/src/style.rs`
- `apply_type_ramp(ctx: &egui::Context, heading_semibold: bool)` にシグネチャ変更。
- `Heading` ブロックで `f.size = FONT_HEADING` に加え、`if heading_semibold { f.family = egui::FontFamily::Name(crate::font::SEMIBOLD_FAMILY.into()); }`。
- **doc コメント更新（70-76行）**: 現行「Monospace と FontFamily は egui 既定を温存する。」は実装後に不正確になる（Heading.family が変わる）。「**Monospace は egui 既定を温存。Heading のみ Semibold ファミリへ切り替える（`heading_semibold` が false ＝フォント不在時は Regular 据え置き）**」へ修正。

### 3. `snotra-settings/src/app.rs`（`run()`）
- `let heading_semibold = crate::font::configure_fonts(&cc.egui_ctx);`
- `apply_win11_theme(&cc.egui_ctx);`
- `style::apply_type_ramp(&cc.egui_ctx, heading_semibold);`

### 4. `snotra-settings/SETTINGS-DESIGN.md`
- タイポグラフィ節（21行）の「ウェイトは size のみ／Semibold 化は follow-up（#399）」注記を解消し、**「見出し（Heading）は Semibold（Latin: Segoe UI Semibold / JP: Yu Gothic UI Semibold）、本文・副文は Regular。Semibold フォント不在環境では Regular にフォールバック」**へ更新。
- 共有ヘルパー表の `apply_type_ramp` 説明を「Fluent タイプランプ（size）+ 見出し Semibold を登録」へ更新。

### 5. `snotra-settings/CLAUDE.md`
- `style.rs` / `apply_type_ramp` の説明に「見出しは Semibold ファミリ」を追記。
- `font.rs` の説明に「日本語 Regular + 見出し用 Semibold ファミリ（graceful fallback）を登録」を追記。
- スタイルシステム節のタイポグラフィ記述（見出し18 / 本文14 / 副文12）に見出し Semibold を追記。

## 実装順序（フェーズ）

1. **Phase 1 — font.rs**: `SEMIBOLD_FAMILY` const + `configure_fonts -> bool` + Semibold ファミリ登録。`cargo build -p snotra-settings` は caller 未更新で fail（想定）。
2. **Phase 2 — style.rs**: `apply_type_ramp` シグネチャ + family 切り替え + doc。
3. **Phase 3 — app.rs**: `run()` で bool を受け渡し。ここで `cargo check -p snotra-settings` グリーン。
4. **Phase 4 — docs**: SETTINGS-DESIGN.md / CLAUDE.md 同期。
5. **検証**: `docs/build-commands.md` カテゴリ A（check / clippy `--all-targets -D warnings`。core 未変更につき core テストは対象外）+ カテゴリ D（`cargo run -p snotra-settings` 視覚スモーク）。

## 不変条件

- **欠字ゼロ**: Semibold ファミリは末尾に Proportional の fallback チェーンを連結する。Semibold フェイスに無いグリフは Regular で必ず描画される。
- **graceful degrade / panic なし**: Semibold フェイスが1つも読めない場合、`configure_fonts` は `false` を返し、`apply_type_ramp` は `Heading.family` を切り替えない（Proportional 据え置き）。未定義 `FontFamily::Name` を参照しない＝egui の欠損ファミリ panic を踏まない。
- **Body/Button/Small は不変**: size のみ、Regular のまま。
- **影響範囲の局所性**: `Heading.family` 変更の影響は `ui.heading()` 呼び出し元（section_heading + modal_header）に限定。grep で他に呼び出し元が無いことを確認済み。
- **起動時1回**: フォント登録は `configure_fonts` 内のみ。毎フレーム処理を追加しない。
- **リソース**: フォントは `ctx` がプロセス寿命で保持。明示的な破棄ペアは不要（新規の listener/プロセス/AtomicBool なし）。

## テスト方針

- snotra-settings はユニットテスト非対象（egui・`snotra-settings/CLAUDE.md` 方針）。新規ロジックは純粋関数ではなくフォント I/O のため。
- 検証 = clippy グリーン + 視覚スモーク（acceptance 条件は目視）:
  - 見出し・モーダル題が Semibold で太く描画される。
  - 本文・副文は Regular（変更なし）。
  - 日本語見出し（例「テーマ」「カラー」）と Latin 混在見出し（例「Migemo 検索」）でベースライン整列・欠字なし。

## SPEC.md 更新要否

**不要**。snotra-settings の見た目変更は SPEC の状態遷移・IPC 契約・スコア計算・設定キー・データフォーマットのいずれにも触れない（#398 size ランプも SPEC 非更新）。プレゼンテーション層のみ。

## セルフレビュー

### Step 5a — /plan-review（並列サブエージェント2体: Rust/egui + docs/scope）

- **影響範囲**: [問題なし]。`configure_fonts` / `apply_type_ramp` の呼び出し元は `app.rs` 各1箇所。`TextStyle::Heading` の消費者は `style.rs` の section_heading + modal_header のみ（app.rs サイドバーは `TextStyle::Body`、About は `.small()`、visual theme_card は `TextStyle::Body`）→ 巻き込みゼロ確定。
- **egui API**: [問題なし]。Cargo.lock = egui 0.34.3。`FontData.index`（ttc face 選択）・`FontFamily::Name(_.into())`・`FontId.family` 書き換えすべて利用可。`seguisb.ttf` / `YuGothB.ttc` 両ファイル存在確認済み。
- **不変条件**: [軽微な懸念→対処済み]。(1) Semibold ファミリは jp_font 登録**後**の `Proportional` fallback を clone して suffix 連結する（font.rs:56-60 で Proportional は jp_font を含む状態で確定）。(2) 未定義 `FontFamily::Name` 参照の panic は `heading_semibold=false` のとき family を切り替えないことで回避。両者とも実装時の遵守事項として明記済み。
- **スコープ / YAGNI**: [問題なし]。#399 の要求に厳密に限定。config キー追加・settings.json 変更・後方互換懸念なし。
- **SPEC.md / E2E / docs**: SPEC 非更新は妥当（プレゼンテーション層・#398 先例一致）。E2E は snotra-settings がネイティブ egui で WebDriver 不可視のため無影響。docs 同期は style.rs doc コメント（70-76行）の明示化を反映済み（上記 §2）。

### Step 5b — セルフレビューチェックリスト

1. **対称コードパス**: 該当なし（`apply_type_ramp` は起動時1回の初期化。show/hide 等の対称ペアなし）。
2. **影響範囲の網羅性**: `ui.heading(` / `TextStyle::Heading` / `configure_fonts` / `apply_type_ramp` を grep 済み。呼び出し元はすべて列挙。
3. **境界条件**: フォント不在（0個）/ 片方のみ存在（Latin のみ・JP のみ）/ 両方存在の3ケースを research.md で検証。全ケースで panic なし・欠字なし。
4. **リソース管理**: フォントは `ctx` がプロセス寿命で保持。新規の listener/プロセス/AtomicBool なし＝破棄ペア不要。
5. **既存パターンとの整合**: `apply_type_ramp` の SSOT レバー（#398）と `configure_fonts` の候補ループ + graceful fallback を再利用。新規パターンの導入なし。
6. **YAGNI 違反**: なし。
7. **シンプル化の挑戦**: bool スレッド化は「always-register（無条件登録）」より1行多いが、未定義ファミリ参照 panic への自己防衛と意図の明示性で採用。フォント I/O 失敗時は `false` 返却で Regular に degrade（「失敗したらどうなるか」を設計に明記）。
8. **破壊不変条件**: 「壊れたら即アウト」は **未定義 `FontFamily::Name` 参照 → egui レイアウト panic**。検知手段 = `heading_semibold` ガード（コードレベル）+ 視覚スモーク（実機で Semibold 描画・欠字を目視）。新規ホットキー/フック/IPC は無し。

**判定**: 計画の completeness = 高。実装着手可。
