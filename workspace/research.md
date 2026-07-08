# research — issue #237 設定画面: 無効化フィールドのホバー説明

## issue の要約

Search タブで無効化（グレーアウト）される数値フィールドに、なぜ操作できないかを説明する
ホバーツールチップを表示する。egui 組み込みの `Response::on_disabled_hover_text()` を使う。

- **当初「保留」だったが、実装方針を確定**（ユーザー判断）: 実装は些細（一行）・リスク皆無で、
  「区別がつく」と「理由が説明される」は別。対称の無効化フィールドも同時に対応する。
- **対象は2フィールド（対称コードパス）**:
  1. `migemo_min_chars` — `migemo_enabled == false` のとき無効（issue 記載）
  2. `fuzzy_history_cap_ratio` — `history_normalization == Disabled` のとき無効（同型・対称）

## 関連コード

- `snotra-settings/src/tabs/search.rs`
  - line 63–81: History score セクション。`cap_enabled = history_normalization != Disabled` で
    `add_enabled_ui(cap_enabled, |ui| { DragValue(fuzzy_history_cap_ratio) })`
  - line 85–102: Migemo セクション。`add_enabled_ui(config.search.migemo_enabled, |ui| {
    DragValue(migemo_min_chars) })`
- `snotra-settings/src/i18n.rs`
  - `TrKey` enum（line ~72–186 付近に Search 系キー）+ 網羅 match の `ja()`（line ~224–375）/
    `en()`（line ~414–565）
  - 新キー追加は `TrKey` に variant を足すだけ → `ja()`/`en()` が非網羅コンパイルエラーで網羅強制
    （`#[deny(clippy::wildcard_enum_match_arm)]` でワイルドアーム逃げも禁止）

## 既存パターン

- **`add_enabled_ui` + 数値フィールドの無効化**: search.rs 内に既に2箇所（migemo / fuzzy cap）。
  完全に同型のため、対応も同一パターンで対称に適用できる。
- **i18n テーブル駆動**: 既存の `HintMigemo` 等と同じく `TrKey` variant + ja()/en() で追加。
- **egui `on_disabled_hover_text`**（context7 で確認）:
  - `pub fn on_disabled_hover_text(self, text: impl Into<WidgetText>) -> Self`（`Response` のビルダー）
  - 無効ウィジェットをホバーしたときに文言を表示する専用機能。まさに本 issue の用途。
  - **適用点**: `add_enabled_ui(enabled, |ui| ui.add_sized(...DragValue...))` は `InnerResponse<Response>`
    を返す。`.inner`（= 内側 DragValue 自身の Response、`enabled == false` を持つ）に
    `.on_disabled_hover_text(...)` を付ける。閉領域の region response（`.response`）でも動くが、
    ウィジェット自身の `.inner` に付けるのが最も正確。
    - 現状クロージャは `add_sized(...)` の後にセミコロンがあり Response を捨てている。
      セミコロンを外してクロージャが Response を返すようにし、`.inner` で受ける。

## 技術的制約

- **Win32 依存なし**: egui UI のみ。SendInput/ウィンドウ系 API は無関係。
- **egui バージョン**: 0.35（snotra-settings/CLAUDE.md 記載）。`on_disabled_hover_text` は安定 API。
- **ウィジェット id 安定性（`warn_if_rect_changes_id`）は非該当**: enabled/disabled は
  ウィジェットの「存在」ではなく「操作可否」の切替であり、フレーム間でウィジェット数・矩形は不変。
  条件付き前置ウィジェットの出現/消失（#456 の事故パターン）とは異なるため auto-id 種はずれない。
  ツールチップは別レイヤ（Area）に描かれ settings_grid の auto-id 列に影響しない。
- **テスト境界**: snotra-settings は egui UI のユニットテストを書かない方針。ツールチップの「表示」は
  レンダリング挙動であり、egui_kittest（AccessKit ツリー操作）では検証対象外
  （CLAUDE.md「レイアウト・レンダリング欠陥は人手の視覚スモークが唯一の検知手段」）。
  → 検証は clippy（コンパイル）+ 視覚スモーク（無効フィールドをホバーして ja/en 両方で文言確認）。

## 未解決の疑問

- なし。API・適用点・対象範囲・検証手段すべて確定済み。
