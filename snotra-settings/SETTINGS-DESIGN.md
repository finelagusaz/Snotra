# SETTINGS-DESIGN.md — 設定 UI デザインガイドライン

`snotra-settings`（egui 製・7タブの設定エディタ）の見た目を統一するための規約。
**実装の SSOT は `src/style.rs`**（トークン + 共有ヘルパー）。本書はその意図と使い方を記述する as-built ドキュメント。新タブ・新パーツを追加するときは、本書とコードの**両方**を更新する。

## デザイン言語

Windows 11 Settings インスパイア（WinUI / Fluent Design）。サイドバー + コンテンツエリア、左アクセントインジケーター、角丸ウィジェット。色・余白・フォントは下記トークンに集約し、各タブで直書きしない。

## タイポグラフィ（Windows 11 Fluent タイプランプ）

出典: Microsoft Learn「[Typography in Windows](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)」（2026-06-25 時点）。`style::apply_type_ramp(ctx)` が `ctx.all_styles_mut` で TextStyle の `size` を一括登録し、全テキストが継承する（タイポグラフィの SSOT）。

| 役割 | egui TextStyle | サイズ(epx) | Fluent 対応 | トークン |
|------|----------------|-------------|-------------|----------|
| セクション見出し・モーダル題 | `Heading`（`ui.heading()` / `section_heading` / `modal_header`） | **18** | Body Large | `FONT_HEADING` |
| ラベル・値・項目主行・ボタン | `Body` / `Button`（既定の `ui.label()` 等） | **14** | Body | `FONT_BODY` |
| ヒント・メタ・説明文・空状態 | `Small`（`.small()` / `hint`） | **12** | Caption | `FONT_CAPTION` |

- **判読最小の順守**: Fluent ガイドラインは「14px Regular / 12px Regular 未満は一部言語で判読不能」とする。egui 既定（Body 12.5 / Small 9）はこれを割り込んでいたため、本ランプで是正した。**サイズを 12.5/9 に戻さない**（退行）。
- **ウェイト**: 今回は **size のみ**。Fluent は「見出しは Semibold」を推奨するが、egui で Semibold を出すには別 FontFamily の登録が要るため未対応。見出しはサイズで階層を作る。Semibold 化は follow-up 候補。
- **Monospace** は egui 既定を温存（設定タブで可視利用なし）。

## スペーシング（縦リズム）

| トークン | 値 | 用途 |
|----------|----|------|
| `ROW_HEIGHT` | 28.0 | `interact_size.y`。チェックボックス/DragValue の縦整列。`tab_scroll_area` が設定 |
| `SPACE_HEADING` | 4.0 | 見出し → 最初のウィジェット（`section_heading` 内） |
| `SPACE_HINT` | 4.0 | ウィジェット → ヒント等の小間隔・モーダル内フィールド間 |
| `SPACE_GROUP` | 8.0 | 説明文 → リスト / モーダルのフィールド群 → アクション |
| `SPACE_SECTION` | 12.0 | セクション間（`section_gap`） |

## カラートークン

### 構造色（Windows 11 Settings パレット）
`SIDEBAR_BG` / `CONTENT_BG` / `FOOTER_BG` / `ACCENT` / `TAB_HOVER` / `TAB_SELECTED_BG` / `TEXT_PRIMARY` / `TEXT_SECONDARY` / `WIDGET_BG` / `WIDGET_BORDER` / `WIDGET_ACTIVE_BG`。

### セマンティック色（意味で選ぶ）
| トークン | 値 | 意味 |
|----------|----|------|
| `STATUS_ERROR` | (196,43,28) | 削除ボタン・エラーメッセージ |
| `STATUS_SUCCESS` | (16,124,16) | 成功メッセージ |
| `STATUS_WARNING` | (196,120,28) | 警告（legacy 移行 ⚠ 等） |
| `BANNER_CAUTION_BG` / `BANNER_CAUTION_FG` | (255,244,206) / (140,90,0) | 注意バナー（read 失敗時） |

`Color32::from_rgb(...)` をタブに直書きしない。意味色はこのトークンを使う。

### フィールド幅
`FIELD_NUMERIC` = 60.0（DragValue / プレフィックス）、`FIELD_HEX` = 80.0（hex 入力）。

## レイアウト規約

- **タブ本体**は `style::tab_scroll_area(ui, |ui| { ... })` で開始する（標準 ScrollArea + `ROW_HEIGHT`）。`ScrollArea::vertical()` を直書きしない。
- **セクション**は `section_heading(ui, 見出し)` → 内容 → `section_gap(ui)`。
- **水平 `separator`** は (1) モーダル境界（`modal_header` のヘッダ下、アクション行の上）と (2) 結果メッセージ領域の境界（backup のインライン message）にのみ使う。**セクション間の区切りには使わない**（見出し + 余白で区切る）。
- **2カラム設定**（ラベル/値）は `style::settings_grid("id").show(ui, |ui| { ... ui.end_row(); })`。
- **副次テキスト**（説明文・メタ・ヒント・空状態「該当なし」）は `style::hint(ui, text)`。
- **リスト行**（編集可能な一覧）は `style::list_item(ui, body, actions)`: 本文を左 vertical、アクションを右寄せ、末尾に separator。
  - **並び替えボタン**は actions 内で `style::reorder_controls(ui, can_up, can_down)`（▲▼・通常サイズ・右クラスタ）。`right_to_left` 前提で ▼→▲ の add 順により視覚的に ▲▼ に揃う。
  - **プリセット等の二次的な行**（separator 不要）は `list_item` を使わず素の `ui.horizontal` でよい。
- **モーダル**は `modal_header(ui, 題)` → フィールド → `ui.add_space(SPACE_GROUP); ui.separator();` → アクション行。アクション行は `[danger_button(削除)?] [modal_buttons(Cancel/Save)]`。削除と保存の Vec 変更ロジックは各タブに残す（共通化しない）。

## 共有ヘルパー（`src/style.rs`）

| 関数 | 役割 |
|------|------|
| `apply_type_ramp(ctx)` | Fluent タイプランプを登録（`run()` で `apply_win11_theme` の後に呼ぶ） |
| `tab_scroll_area(ui, f)` | タブ本体の標準スクロール領域 + 行高 |
| `section_heading(ui, t)` | 見出し + `SPACE_HEADING` |
| `section_gap(ui)` | `SPACE_SECTION` の余白 |
| `hint(ui, t)` | 副次テキスト（Caption + `TEXT_SECONDARY`） |
| `settings_grid(id) -> Grid` | 2カラムのラベル/値グリッド |
| `modal_header(ui, t)` | モーダル題 + separator + 余白 |
| `danger_button(ui, t) -> Response` | 削除ボタン（`STATUS_ERROR`） |
| `list_item(ui, body, actions) -> R` | リスト行スキャフォールド（本文左・アクション右・末尾 separator） |
| `reorder_controls(ui, can_up, can_down) -> Option<ReorderDir>` | 並び替え ▲▼ の正規形 |
| `modal_buttons(ui, tr) -> ModalButtons` | Cancel/Save 対（`{ cancel, save }`） |

## ヘルパー化しない（YAGNI / egui 制約）

- **数値入力**（DragValue）は range/speed/min_decimals がまちまちのため汎用化せず、幅 `FIELD_NUMERIC` のみ共有する。
- **color_row**（visual）は `color_edit_button_srgba` が**永続変数の `&mut Color32`** を要求する（一時変数だと変更が消える）。ヘルパー化せず `settings_grid` 内で実装する。
- **モーダルの保存ロジック**は対象 Vec（`paths.scan` / `openers` / `instant_commands`）ごとに境界チェック・処理が異なるため共通化しない。共通化するのは視覚スキャフォールド（`modal_header` / `danger_button` / `modal_buttons`）のみ。

## 特殊パーツの例外

- **theme_card**（visual のテーマプリセットカード）と**サイドバータブ描画**（app.rs の painter 直描画）は専用描画。トークンは使うが汎用ヘルパーには寄せない。
- **visual の色編集**（`Color32::from_hex` / `Color32::GRAY` / `Color32::BLACK`）は色編集機能そのものであり、パレットリテラルではない（`from_rgb` 直書き禁止の対象外）。

## 新タブ追加チェックリスト

1. `style::tab_scroll_area` で本体を包む。
2. 見出しは `section_heading`、セクション間は `section_gap`。
3. 色・余白・ScrollArea・フォントサイズを直書きしない（トークン・ヘルパーを使う）。
4. リスト行は `list_item`、並び替えは `reorder_controls`、モーダルは `modal_header` + `modal_buttons` + `danger_button`。
5. 既存の規約で表現できない逸脱が要るときは、**先に本書を更新**してから実装する。
