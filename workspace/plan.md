# plan — 設定UIデザインの統一 (issue #393)

## ゴールと受け入れ条件

1. `snotra-settings/src/style.rs` を新設し、デザイントークン（spacing / 色 / 幅 / 行高）と共有ヘルパーを集約する。
2. 全7タブ + `app.rs` を `style` 経由に書き換え、直書きの色リテラル・余白・ScrollArea ボイラープレートを排除する。
3. 逸脱を解消する: backup の説明文を `.small()` 化、backup のセクション間 separator を撤去、instant のリスト行を index/opener に揃える。
4. `snotra-settings/SETTINGS-DESIGN.md` を作成し、トークン・タイポグラフィ・レイアウト規約・パーツ選択指針を as-built で記述する。
5. `cargo build -p snotra-settings` と clippy がグリーン。ビルドして 7タブを目視し、レイアウト崩れ（overflow/clipping）が無い。

**受け入れ条件（テスト可能な形）**:
- `grep -rn "Color32::from_rgb" snotra-settings/src/tabs/` が **0 件**（削除/エラー/成功/警告の意味色は全て `style::STATUS_*` トークンへ）。`app.rs` 内の色も `style::` 参照のみ。
  - **例外**: `visual.rs` の `Color32::from_hex(hex)` / `Color32::GRAY` / `Color32::BLACK` は色編集機能（swatch プレビュー・カラーピッカー）の一部でパレットリテラルではない → `from_rgb` grep の対象外。SETTINGS-DESIGN.md に「特殊パーツ」として明記。
- `grep -rn "ScrollArea::vertical" snotra-settings/src/tabs/` が **0 件**（`style::tab_scroll_area` へ集約）。
- `grep -rn "\.small()" snotra-settings/src/tabs/backup.rs` で説明文が Small になっている。
- 全タブのセクション間余白が `style::section_gap`（12.0）に統一されている。
- 空状態ラベル（「該当なし」: index/opener/instant）が secondary 色（`style::hint`）に統一されている。

## style.rs の API 設計（as-built の対象）

### トークン（`pub const`）

```
// 縦リズム（spacing scale）
ROW_HEIGHT: f32 = 24.0          // interact_size.y
SPACE_HEADING: f32 = 4.0        // 見出し→最初のウィジェット
SPACE_HINT: f32 = 4.0           // ウィジェット→ヒント等の小間隔
SPACE_GROUP: f32 = 8.0          // モーダル内フィールド群 / 説明文→リスト
SPACE_SECTION: f32 = 12.0       // セクション間
SPACE_BLOCK: f32 = 16.0         // 大ブロック間（必要時のみ）

// フィールド幅
FIELD_NUMERIC: f32 = 60.0       // DragValue / prefix
FIELD_HEX: f32 = 80.0           // hex 入力

// 色パレット（app.rs から逐語移動。RGB 不変）
SIDEBAR_BG, CONTENT_BG, FOOTER_BG, ACCENT, TAB_HOVER, TAB_SELECTED_BG,
TEXT_PRIMARY, TEXT_SECONDARY, WIDGET_BG, WIDGET_BORDER, WIDGET_ACTIVE_BG(220,220,220)

// セマンティック色（散在を集約）
STATUS_ERROR: (196,43,28)       // 削除/エラー
STATUS_SUCCESS: (16,124,16)     // 成功
STATUS_WARNING: (196,120,28)    // 警告（legacy ⚠ 等）
BANNER_CAUTION_BG: (255,244,206)
BANNER_CAUTION_FG: (140,90,0)
```

### ヘルパー（`pub fn`）

```
tab_scroll_area<R>(ui, add_contents: impl FnOnce(&mut Ui)->R) -> R
    // 標準 ScrollArea(drag:false, auto_shrink false) + interact_size.y = ROW_HEIGHT を1箇所に集約

section_heading(ui, text: &str)
    // ui.heading(text); ui.add_space(SPACE_HEADING)

section_gap(ui)
    // ui.add_space(SPACE_SECTION)

hint(ui, text: &str)
    // ui.label(RichText::new(text).small().color(TEXT_SECONDARY))  ← 説明文/メタ行の規範

settings_grid(id_salt: &str) -> egui::Grid
    // Grid::new(id_salt).num_columns(2).spacing([8.0, 4.0])

modal_header(ui, title: &str)
    // ui.heading(title); ui.separator(); ui.add_space(SPACE_HEADING)

danger_button(ui, text: &str) -> egui::Response
    // ui.add(Button::new(RichText::new(text).color(STATUS_ERROR)))

list_item<R>(ui, body: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui)->R) -> R
    // horizontal{ vertical{body}; right_to_left(Center){actions} } ; ui.separator()
    // → index/opener/instant のリスト行を1つの規範形に統一

reorder_controls(ui, can_up: bool, can_down: bool) -> Option<ReorderDir>  // enum {Up, Down}
    // ▲(up) → ▼(down) の順・通常サイズ・右アクションクラスタ用。クリックされた方向を返す。
    // enabled 条件（can_up/can_down）は呼び出し側が算出（opener=rule内 tool index, instant=flat index vs len）。
    // → opener(現状 ▼▲右/通常) と instant(現状 ▲▼左/small) の並び替えを単一スタイルに正規化。

modal_buttons(ui, tr: &Tr) -> ModalButtons  // { cancel: bool, save: bool }
    // with_layout(right_to_left, |ui| { Cancel; Save }) を1箇所に。3モーダルで完全同一の対を共有。
    // 削除ボタン（danger_button）と Vec 変更ロジックは各モーダルに残す（保存処理が tab ごとに異なるため）。
```

**設計判断（やりすぎ回避）**:
- 数値入力は config ごとに range/speed/min_decimals が異なるため、汎用 `numeric()` は作らず **幅トークン `FIELD_NUMERIC` のみ**共有（`add_sized` 呼び出しは各所に残す）。
- color_row（visual）は `color_edit_button_srgba` の `&mut Color32` 永続変数制約があるため**ヘルパー化しない**。`settings_grid` の中で従来どおり実装。
- モーダルのフッタ（Cancel/Save + 条件付き Delete）は tab ごとに保存処理が異なるため**フルヘルパー化せず**、`danger_button`（削除赤）と `modal_header` のみ共有。Cancel/Save の `right_to_left` ブロックは各モーダルに残す（SETTINGS-DESIGN.md にパターンとして明記）。

## 実装順序（フェーズ）

### Phase 1: style.rs 新設（トークン + ヘルパー）
- `snotra-settings/src/style.rs` を作成。上記トークン・ヘルパーを実装。
- `main.rs` に `mod style;` を追加（`app` より前でよい）。
- この時点では誰も使っていないので `#[allow(dead_code)]` は付けない（Phase 2 以降で全消費されるため。未使用が残れば clippy が teach してくれる＝消費漏れ検出器）。
- **mid-verify**: `cargo build -p snotra-settings`。

### Phase 2: app.rs を style:: へ移行（色パレットの SSOT 化）
- app.rs の色 `const` 定義（10色 + active 220）を削除し、`use crate::style;` で参照。
- `apply_win11_theme` 内の色参照を `style::ACCENT` 等に置換。`WIDGET_ACTIVE_BG` を新トークンで参照。
- read-failed バナーの色を `style::BANNER_CAUTION_BG / BANNER_CAUTION_FG` に置換。
- `pub const TEXT_SECONDARY` を app.rs から削除（style.rs が SSOT）。**全参照（search/index/opener/instant/backup の `crate::app::TEXT_SECONDARY`）は Phase 3 で `style::TEXT_SECONDARY` か `style::hint` へ移す**。
- **mid-verify**: `cargo build -p snotra-settings` → `crate::app::TEXT_SECONDARY` 参照箇所が compile-fail で列挙される（改名検出器）。Phase 3 で解消。

### Phase 3: 各タブを style:: へ移行
タブ単位で順に書換 → 各タブ完了ごとに `cargo build -p snotra-settings`。

- **general.rs**: `tab_scroll_area` / `section_heading` / `section_gap` / `settings_grid` 適用。逸脱なし（規範形の確認）。
- **search.rs**: 同上 + migemo ヒントを `style::hint` 化。`crate::app::TEXT_SECONDARY` import 削除。FIELD_NUMERIC 適用。
- **visual.rs**: `tab_scroll_area`/`section_heading`/`section_gap`/`settings_grid` 適用。color_row はヘルパー化せず内側で `FIELD_HEX` 使用。theme_card は専用描画のため温存（SETTINGS-DESIGN.md に「特殊パーツ」として記載）。
- **index.rs**: `tab_scroll_area`/`section_heading`/`list_item`/`modal_header`/`modal_buttons`/`danger_button`/`hint` 適用。空状態「該当なし」を `style::hint`（secondary）に。
- **opener.rs**: 同上。リスト行を `list_item` に。並び替え ▼▲ を **`reorder_controls`（▲▼順・右・通常サイズ）に正規化**（矢印順の統一を含む）。modal は `modal_header`/`modal_buttons`/`danger_button`。presets セクションも section_heading/hint。空状態「該当なし」を `style::hint`。
- **instant.rs**: `tab_scroll_area`/`section_heading`/`section_gap`/`settings_grid`(prefix)/`hint`/`list_item`/`modal_header`/`modal_buttons`/`danger_button` 適用。
  - **リスト行の統一（意図的な見た目変更）**: 移動 ▲▼ を**左側 vertical・`.small()` から `reorder_controls`（右アクションクラスタ・通常サイズ・▲▼順）へ**移設。`list_item` の actions 内で `[reorder_controls][Edit][Duplicate]` を構成。legacy ⚠ は `style::STATUS_WARNING`。空状態「該当なし」を `style::hint`。
- **backup.rs**: `tab_scroll_area`/`section_heading`/`section_gap` 適用。
  - **説明文を `style::hint` 化**（`.small()` 付与）。
  - **セクション間 `add_space(16)+separator+add_space(8)` を `section_gap()` に統一**（水平 separator 撤去）。
  - 末尾の message 区切り separator は**残す**（結果領域の境界＝モーダル/結果の区切りは許容、SETTINGS-DESIGN.md に明記）。config_dir パスは `style::hint`、その直前の `add_space(2.0)` は `SPACE_HINT(4.0)` に寄せる。
  - error/success の色を `style::STATUS_ERROR / STATUS_SUCCESS` に。
  - `config_error_message` は app.rs から import 継続（ロジック、移動しない）。
- **mid-verify（Phase 3 完了）**: `cargo build -p snotra-settings` グリーン。`grep` 受け入れ条件（色 0 件 / ScrollArea 0 件）を確認。

### Phase 4: SETTINGS-DESIGN.md 作成
`snotra-settings/SETTINGS-DESIGN.md` を作成。内容（as-built）:
- **デザイン言語**: Windows 11 Settings インスパイア（app.rs の既存方針を明文化）。
- **タイポグラフィスケール**: 役割→TextStyle 対応表（見出し=Heading / ラベル・値・項目主行=Body / ヒント・メタ=Small+TEXT_SECONDARY）。ピクセル指定はしない＝egui 既定に委ねる旨。
- **スペーシングトークン**: ROW_HEIGHT/SPACE_* の表と用途。
- **カラートークン**: 構造色 + セマンティック色の表（赤=削除/エラー, 緑=成功, オレンジ=警告, バナー）。
- **レイアウト規約**: セクション=`section_heading`+内容+`section_gap`。水平 separator はモーダル境界と結果領域のみ。リスト行=`list_item`（本文左・アクション右）。2カラム設定=`settings_grid`。
- **共有ヘルパー一覧**: style.rs の関数とシグネチャ・使いどころ。
- **新タブ追加チェックリスト**: tab_scroll_area で開始 / section_heading で見出し / 直書き色・余白・ScrollArea 禁止 / 逸脱が要るなら本ドキュメントを先に更新。
- **特殊パーツの例外**: theme_card（visual）・サイドバータブ描画（app.rs の painter 直描画）は専用描画として許容、ただしトークン色を使う。

### Phase 5: ドキュメント同期 + 検証
- `snotra-settings/CLAUDE.md`「モジュール構成」に `style.rs` を追加（AGENTS.md: 新規 .rs はモジュール構成同期）。「スタイルシステム」小節を追加し `SETTINGS-DESIGN.md` へのポインタ + 不変条件「色/余白/ScrollArea は直書きせず `style` のトークン・ヘルパーを使う」を1行追記。
- `AGENTS.md`「ドキュメント参照」に `SETTINGS-DESIGN.md`（設定UIデザインガイドライン）への軽量ポインタを1行追加（discoverability。既存の粒度に合わせ簡潔に）。
- カテゴリ別検証（`docs/build-commands.md`）: `.rs` 編集 → clippy（フック自動）+ `cargo build -p snotra-settings`。`.md` のみは検証不要。
- **ビルドして 7タブを目視**（UI 変更はレイアウト崩れ観点で検証する＝AGENTS.md）。特に instant のリスト行（移動ボタン移設後）・backup（separator 撤去後）・各タブの余白・空状態ラベルの色。

## 不変条件

- **色は RGB 逐語移動で見た目不変**: パレット移動で値を変えない（diff で RGB 一致を確認）。意図的変更は instant リスト行と backup 余白/separator のみ。
- **draft/saved モデル不変**: スタイル変更は config 読み書きに触れない。`saved` 更新タイミング・保存フローは現状維持。
- **モーダルの境界チェック不変**: `if idx < vec.len()` ガード（index/opener/instant）は `list_item`/`danger_button` 化後も保持。削除・保存ロジックは1行も変えない。
- **PickerState の active リセット不変**: index/opener の picker ポーリング・`active=false` リセットは触れない。
- **opener ターゲット変更 remove→add 不変**: `save_opener` のロジックは変更しない。
- **ヘルパー導入で挙動が変わらないこと**: `tab_scroll_area` は従来の ScrollArea 設定（drag:false, auto_shrink [false,false]）と `interact_size.y=24.0` を完全再現する。`list_item` は従来の `horizontal{vertical;right_to_left}` + 末尾 separator と等価。
- **失敗・異常系**: 新規の状態フラグ・プロセス・リソースは導入しない（純粋な描画ヘルパーのみ）。ヘルパーはパニックし得る配列アクセスを含まない（境界チェックは呼び出し側のクロージャに残る）。

## テスト方針

- snotra-settings はユニットテスト非対象（CLAUDE.md 方針）。**`cargo build -p snotra-settings` + clippy をグリーンゲート**とする。
- backup.rs の既存ロジックテスト（`localize_toml_error_*`, `extract_backtick_*`）は**触らない**（スタイル変更はこれらの不変条件に無関係）。改名・転用でテストを壊さない。
- **受け入れ条件の grep**（色 0 件 / ScrollArea 0 件 / backup `.small()` 付与）を検証ステップで実行。
- **手動 smoke**: `cargo run -p snotra-settings` で 7タブ + 各モーダルを開き、レイアウト崩れ・色の退行が無いことを目視。

## SPEC.md 更新要否

- **不要**（裏取り済み）。SPEC §7（設定画面）は**サイドバー+コンテンツのレイアウト構造・タブ構成・各タブの設定項目・ダーティ「•」・↑↓タブ移動・二段階リセット**を規定し、§19.8 はインスタントの**フォーム項目/種別ラジオ/プレビュー/移行ヒント**を規定する。いずれも**フォントサイズ・余白px・色リテラル・リスト行のボタン配置といったスタイルトークンは規定していない**。本件は presentation（トークン化・ヘルパー抽出）のみで、どのウィジェットが存在するか・挙動・IPC 契約・状態遷移を変えない → AGENTS.md Step 0 の「文書化された挙動を変えたら仕様変更」に**該当しない**。
- **触れてはいけない SPEC 契約（現状維持を保証）**: ダーティ「•」(§7.1)・二段階リセット(§7.3)・↑↓タブ移動とサイドバー sentinel フォーカス機構(app.rs)・並び替え/複製の**機能の有無**(§7.2 opener, §19.8 instant)。「スタイル統一」は機能の追加削除をしない。
- **out-of-scope の観察**: §19.8 は instant の「追加/編集/削除/複製」を列挙するが**並び替え（▲▼）の存在を記載していない**（コードには存在）。これは本件と無関係な既存の as-built ギャップ。本サイクルでは**触れない**（design 統一の scope 外。必要なら follow-up issue）。
- 更新する docs: `snotra-settings/CLAUDE.md`（モジュール構成 + スタイルシステム小節 + 不変条件）、`AGENTS.md`（ドキュメント参照に軽量ポインタ）、新規 `snotra-settings/SETTINGS-DESIGN.md`。`docs/architecture.md` はモジュール列挙を CLAUDE.md に委譲しているため必須更新でない＝触らない。
- **SETTINGS-DESIGN.md の置き場所**: `snotra-settings/SETTINGS-DESIGN.md`（crate 同居）。理由: (1) 統治対象が snotra-settings に閉じる module-local 文書 (2) ユーザーが option B 選択時に承認したプレビューのパスと一致。独立導出は「repo 直下」を提案したが、承認済みパス + module 局所性を優先し co-location を採る。discoverability は AGENTS.md/CLAUDE.md のポインタで担保。

## セルフレビュー

### 5a. /plan-review（Explore×3 成果物監査 + Plan×1 独立導出）の結果

- **問題なし（一致＝完全性の証拠）**: style.rs 新設・色SSOT化・`TEXT_SECONDARY` 移動（他クレート参照ゼロ→compile-fail 検出器有効）・`tab_scroll_area` への ScrollArea/行高集約（全7タブ同一）・backup の `.small()` 欠落が最大不統一・色は逐語移動で見た目不変・SPEC 同期不要・color_row 非ヘルパー化・モーダル保存ロジック非共通化・border ガード/picker reset/draft-saved 不変。主要判断が独立に再一致。
- **挙動保存（behavior agent 確認）**: `list_item` の actions クロージャは各イテレーションで drop されるため `action: Option<_>` の `&mut` キャプチャは borrow checker を満たす。境界ガード `if idx < vec.len()` はクロージャ外の後処理に残る。instant の `len` スナップショット + 処理時再チェックは移設後も成立。
- **反映した漏れ（導出 ∖ plan）**: ①空状態「該当なし」を secondary(`hint`) に統一 ②`reorder_controls` で ▲▼ のサイズ・順序・配置を正規化 ③`modal_buttons` で Cancel/Save 対を共有 ④`AGENTS.md` ドキュメント参照にポインタ追加。すべて上の API/Phase/docs に反映済み。
- **不一致の解決**: 独立導出は「万能 list_item は YAGNI」と主張。私の `list_item` は中身をクロージャに委ねる薄いスキャフォールド（horizontal+左vertical+右actions+separator の4行）で、3タブの構造が同一なため採用。ただし**実装で借用摩擦が出たら inline + トークンへフォールバック**し、統一は `reorder_controls`/`secondary_label`/トークンで担保する（list_item は強制しない）。
- **ハルシネーション棄却**: docs-sync agent が style.rs を「ThemePreset/ColorScheme 型・apply_theme()/load_system_fonts()」と記述したが誤り。`ThemePreset` は `snotra-core::config`、フォント読込は `font.rs` のまま。style.rs は**トークン + UI 描画ヘルパーのみ**。

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: モーダルの Create/Edit、リスト行の Edit/Delete、show/hide picker は対称ペア。本件はスタイル統一でロジックを変えないため、対称性は現状を保持（5a で確認）。`/symmetric-check` 相当は「ロジック変更なし」のため不要と判断。
2. **影響範囲の網羅性**: `crate::app::TEXT_SECONDARY`(16) / `Color32::from_rgb`(tabs 5) / `ScrollArea::vertical`(7) / `interact_size.y=24.0`(7) を grep 列挙済み。compile-fail を Phase 2→3 の検出器に使う。
3. **境界条件**: 空リスト（空状態ラベル）、Edit モードのみ Delete 表示、reorder の端（can_up/can_down=false で disabled）、color hex 不正（black フォールバック）— いずれも既存ロジックを温存し、ヘルパーは描画のみ。
4. **リソース管理**: 新規リソース（listen/プロセス/ObjectURL/AtomicBool）を**導入しない**。picker スレッド/`active` リセットは既存のまま不変。純粋描画ヘルパーに破棄ペアは不要。
5. **既存パターンとの整合**: egui の標準ウィジェット + 既存 painter 直描画（サイドバー/theme_card）を踏襲。新規描画パラダイムを導入しない。
6. **YAGNI 違反**: 汎用 `numeric()`/万能リスト行は作らない（幅トークン + 薄い list_item に留める）。color_row はヘルパー化しない（egui の `&mut Color32` 永続変数制約）。モーダル保存ロジックは共通化しない（Vec ごとに異なる）。`§19.8 reorder` の SPEC 補完は scope 外に置く。
7. **シンプル化の挑戦**: 新たな状態（AtomicBool/Mutex/子プロセス）はゼロ。`tab_scroll_area`/`list_item`/`reorder_controls`/`modal_buttons` は**戻り値で結果を返すだけの純関数的ヘルパー**で、暗黙の状態を持たない。「この操作が失敗したら」→ 描画ヘルパーは失敗経路を持たない（パニックし得る配列アクセスは呼び出し側クロージャに残す）。
8. **破壊不変条件の明示**: 「壊れたら即アウト」は (a) 色の逐語移動で見た目が変わらないこと（検知: diff で RGB 一致 + 目視）、(b) draft/saved と保存フローが不変であること（検知: ロジック無変更 + cargo build + 目視で Save/Discard 動作）、(c) サイドバー sentinel フォーカス機構が無傷（検知: ↑↓/Tab のタブ移動を目視）。Win32 フック・ホットキー・IPC は本件で**触れない**ため wedge リスクなし。

### 結論
- 計画の completeness: **高**（独立導出と主要判断が再一致、漏れ4点を反映済み）。
- 実装着手可否: **可**。Phase 1→5 を順に、各 Phase 末に `cargo build -p snotra-settings` を改名/消費漏れ検出器として回す。
