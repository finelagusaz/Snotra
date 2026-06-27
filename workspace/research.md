# research — 設定UIデザインの統一 (issue #393)

## issue の要約

- **ペイン**: 設定エディタ（`snotra-settings`, egui 製・7タブ）のタブごとに、使うパーツ・フォントサイズ・余白がバラついている。
- **あるべき姿**:
  1. 使うパーツ・フォントサイズに統一感を持たせる。
  2. デザインガイドライン `SETTINGS-DESIGN.md` を作成し、今後の拡張（新タブ追加）で一貫性が崩れないよう備える。
- **方針（ユーザー合意）**: 共有スタイル module（`style.rs`）を新設し、デザイントークン（spacing / 色 / 幅）と共有ヘルパー（`section_heading()` 等）を集約。全7タブをそれ経由に書き換える。`SETTINGS-DESIGN.md` は実装トークンと**対**で記述する（as-built）。

## 関連コード

| ファイル | 役割 | 本件での扱い |
|---|---|---|
| `snotra-settings/src/app.rs` | chrome（サイドバー・フッター・テーマ・色パレット定義）、`apply_win11_theme()` | 色パレットを `style.rs` へ移動し参照に書換。`TEXT_SECONDARY`（pub）の移動に伴い全参照を更新 |
| `snotra-settings/src/tabs/general.rs` | 全般設定 | `style` 経由に書換（最も規範的＝逸脱少） |
| `snotra-settings/src/tabs/search.rs` | 検索設定 | 同上。migemo ヒントは既に `.small().secondary` |
| `snotra-settings/src/tabs/visual.rs` | テーマ/色/フォント | 同上。`SWATCH_SIZE`・幅リテラルをトークン化検討 |
| `snotra-settings/src/tabs/index.rs` | スキャンパス（リスト+モーダル） | リスト行・モーダルヘッダ・削除ボタンをヘルパー化 |
| `snotra-settings/src/tabs/opener.rs` | オープナー（リスト+モーダル+プリセット） | 同上 |
| `snotra-settings/src/tabs/instant.rs` | インスタントコマンド（リスト+モーダル） | **リスト行レイアウトを index/opener に合わせる**（移動ボタンの位置・サイズ）。説明文・ヒントをヘルパー化 |
| `snotra-settings/src/tabs/backup.rs` | バックアップ（インライン message） | **説明文に `.small()` 付与**・**セクション間 separator 撤去**して section_gap に統一 |
| `snotra-settings/src/tabs/mod.rs` | サブモジュール宣言 | 変更なし |
| `snotra-settings/CLAUDE.md` | モジュール構成・規約 | `style.rs` 追加を反映、`SETTINGS-DESIGN.md` へのポインタ追加 |

## 既存パターン（再利用できるもの／揃っているもの）

- **見出し**: 全タブ `ui.heading()`（egui `TextStyle::Heading`）。統一済み。
- **見出し直後の余白**: 全タブ `ui.add_space(4.0)`。統一済み。
- **2カラムのラベル/値**: `egui::Grid::new(id).num_columns(2).spacing([8.0, 4.0])`。general/search/visual/instant(prefix) で同形。
- **数値入力**: `ui.add_sized([60.0, interact_size.y], DragValue::new(...).range(...))`。幅 60.0 が共通。
- **行高**: 各タブ冒頭で `ui.spacing_mut().interact_size.y = 24.0`。値は同一だが各タブに重複。
- **ScrollArea**: 各タブが `ScrollArea::vertical().auto_shrink([false,false]).scroll_source(ScrollSource{drag:false,..})` を重複記述。
- **モーダルヘッダ**: index/opener/instant が `ui.heading(title); ui.separator(); ui.add_space(4.0)` で同形。
- **削除ボタン**: index/opener/instant が `Button::new(RichText::new(tr.btn_delete()).color(Color32::from_rgb(196,43,28)))` で同形（赤）。
- **モーダル末尾の Cancel/Save**: `with_layout(right_to_left, |ui| { Cancel; Save })` で同形。

## 不統一の事実（catalog）

### A. フォントサイズ（どの TextStyle を当てるか）
- 説明文/ヒント: opener/instant/search は `RichText::new(_).small().color(TEXT_SECONDARY)`（`TextStyle::Small`）。
  **backup は `.small()` を欠き本文サイズ**（`RichText::new(_).color(TEXT_SECONDARY)`、4箇所: export/import/data_folder 説明 + config_dir パス表示）。
- `font.rs` は日本語フォントの family fallback のみ設定。TextStyle のサイズは egui 既定（Heading≒18 / Body / Small≒9）。→ **サイズ不統一の実体は「どの TextStyle を当てるかの不統一」**であり、ピクセル指定の不統一ではない。

### B. セクション間の縦余白（vertical rhythm）
`grep add_space` 集計: `4.0`×38, `8.0`×11, `12.0`×12, `16.0`×4, `2.0`×1。
- general/search/visual: セクション間 `add_space(12.0)`。
- instant: prefix→commands 間 `add_space(16.0)`。
- backup: セクション間 `add_space(16.0) + ui.separator() + add_space(8.0)`（別リズム＋水平線）。
- backup の `add_space(2.0)`（config_dir パスの直前）は単発の例外。

### C. リスト行レイアウト
- index: `horizontal{ vertical{ label(path); hint(meta) }  right_to_left{ Edit } }` + `separator`。
- opener: `horizontal{ vertical{ label("[target] name"); hint(exe) }  right_to_left{ Edit; ▼; ▲ } }` + `separator`（▲▼は通常サイズ・右側）。
- instant: `horizontal{ vertical{ ▲.small(); ▼.small() }  vertical{ label(name); hint(desc); hint(display); ⚠ }  right_to_left{ Edit; Duplicate } }` + `separator`。
  → **移動ボタンが左側・`.small()`** で index/opener と非対称。

### D. セマンティック色（直書きリテラルの散在）
| 色 | 値 | 出現 |
|---|---|---|
| 削除/エラー赤 | `(196,43,28)` | backup(error), index/opener/instant(delete) ＝4箇所で一致 |
| 成功緑 | `(16,124,16)` | backup |
| 警告オレンジ | `(196,120,28)` | instant(legacy ⚠) |
| バナー注意（文字） | `(140,90,0)` | app.rs(read-failed banner) |
| バナー注意（背景） | `(255,244,206)` | app.rs |

→ 赤は値こそ一致するが各ファイルに直書き。緑/オレンジ/バナーは一意で散在。**セマンティック色の SSOT が無い**。

### E. 幅・サイズのマジック数値
- 数値入力幅 `60.0`（search×3, visual×2, instant prefix は `desired_width(60.0)`）。
- hex 入力幅 `desired_width(80.0)`（visual）。
- swatch `SWATCH_SIZE=16.0`（visual、定数化済み）。

## 技術的制約

- **egui 即時モード**: ヘルパーは `&mut egui::Ui` を取り副作用で描画する関数として設計する（戻り値が要る場合は `Response`/ジェネリック `R` を返す）。
- **`color_edit_button_srgba` は永続変数の `&mut Color32` を要求**（visual.rs の color_row）。ヘルパー化してもこの制約は保つ。
- **`crate::app::TEXT_SECONDARY` は pub で多数参照**（search/index/opener/instant/backup）。`style.rs` へ移すなら全参照を同時更新。**compile-fail を改名検出器として使う**（`cargo build -p snotra-settings`）。
- **ユニットテスト非対象**: snotra-settings は egui UI でモック困難のためユニットテストを書かない方針（CLAUDE.md）。検証は `cargo build/clippy` + ビルドして目視。backup.rs の既存ロジックテスト（`localize_toml_error` 等）は触らない。
- **Win32 依存なし**（本件はスタイルのみ）。`list_system_fonts`（GDI）には触れない。
- **挙動の同一性**: 色は RGB を逐語移動するため見た目不変。唯一の意図的な見た目変更は instant のリスト行（移動ボタン位置）と backup の余白/separator。SPEC.md は設定UIのレイアウトを規定していない（grep 該当なし）ため SPEC 同期は不要。スタイル変更は presentation であり IPC/状態遷移を変えない。

## SETTINGS-DESIGN.md の置き場所

- 既存 `docs/design/` は**日付き ADR 形式の設計メモ**（`2026-05-31-coherence-staleset.md`）用。生きたガイドラインとは性質が異なる。
- 統治対象 crate と同居させ **`snotra-settings/SETTINGS-DESIGN.md`** に置く（新タブ追加者が見つけやすい）。`snotra-settings/CLAUDE.md` からポインタを張る。

## 未解決の疑問

- なし（深さはユーザー合意済み＝option B）。リスト行の移動ボタンを「右・通常サイズ」へ寄せる点は、index/opener（3タブ中2タブ）の多数派に instant を合わせる判断。SETTINGS-DESIGN.md に規約として明記する。
