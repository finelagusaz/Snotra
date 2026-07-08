# plan — issue #237 設定画面: 無効化フィールドのホバー説明

## ゴール

Search タブの2つの無効化数値フィールドに `on_disabled_hover_text()` を付け、無効な理由を
ホバーで説明する。対称の2フィールド（migemo / fuzzy cap）を同時に対応する。

## 変更ファイル一覧

### 1. `snotra-settings/src/i18n.rs` — 新 i18n キー2つ

- `TrKey` enum に variant を2つ追加:
  - `TooltipMigemoDisabled`
  - `TooltipFuzzyCapDisabled`
- `ja()` に翻訳を追加:
  - `TooltipMigemoDisabled` → `"ローマ字検索（Migemo）を有効にすると設定できます"`
  - `TooltipFuzzyCapDisabled` → `"正規化を「Fuzzy 相対キャップ」にすると設定できます"`
- `en()` に翻訳を追加:
  - `TooltipMigemoDisabled` → `"Available when Romaji Search (Migemo) is enabled"`
  - `TooltipFuzzyCapDisabled` → `"Available when normalization is set to Fuzzy relative cap"`

  文言根拠: fuzzy cap の有効化条件は `history_normalization != Disabled`。現状 normalization の
  非 Disabled 選択肢は「Fuzzy 相対キャップ」のみのため、その選択を促す文言が最も具体的で actionable。

### 2. `snotra-settings/src/tabs/search.rs` — 2箇所に `.inner.on_disabled_hover_text(...)`

- **fuzzy cap（line 71–79 付近）**: `add_enabled_ui(cap_enabled, |ui| { ui.add_sized(...) })` の
  クロージャ末尾セミコロンを外して Response を返し、`.inner.on_disabled_hover_text(tr.t(
  TrKey::TooltipFuzzyCapDisabled))` を付ける。
- **migemo（line 95–100 付近）**: 同様に `add_enabled_ui(config.search.migemo_enabled, |ui| {
  ui.add_sized(...) })` → `.inner.on_disabled_hover_text(tr.t(TrKey::TooltipMigemoDisabled))`。

  適用イメージ:
  ```rust
  ui.add_enabled_ui(config.search.migemo_enabled, |ui| {
      ui.add_sized(
          [style::FIELD_NUMERIC, ui.spacing().interact_size.y],
          egui::DragValue::new(&mut config.search.migemo_min_chars).range(1..=10),
      )          // ← セミコロンを外し Response を返す
  })
  .inner
  .on_disabled_hover_text(tr.t(TrKey::TooltipMigemoDisabled));
  ```

## 実装順序（フェーズ）

依存は「i18n キー定義 → search.rs で参照」の一方向のみ。1フェーズで完結する規模。

1. **i18n.rs**: `TrKey` variant 2つ + `ja()`/`en()` 各2エントリを追加。
   - この時点で `ja()`/`en()` の網羅 match が新 variant を要求してコンパイルエラー → 翻訳漏れ検出。
2. **search.rs**: 2箇所の `add_enabled_ui` に `.inner.on_disabled_hover_text(...)` を付与。
3. **検証**: clippy（PostToolUse フックが自動実行）+ 手動ビルド + 視覚スモーク。

## 不変条件

- **有効時の挙動は不変**: `on_disabled_hover_text` は「無効かつホバー」時のみ発火。フィールドが
  有効（migemo_enabled / cap_enabled が true）のときは一切の見た目・操作変化なし。
- **ウィジェット id 安定性を壊さない**: enabled↔disabled はウィジェットの存在ではなく操作可否の
  切替であり、フレーム間でウィジェット数・矩形・auto-id 種は不変。#456 型の
  `warn_if_rect_changes_id` 回帰は発生しない。ツールチップは別 Area レイヤに描かれ grid の
  auto-id 列に混入しない。
- **翻訳網羅**: `TrKey` variant を足すと `ja()`/`en()` が非網羅コンパイルエラーになる（両言語の
  翻訳が揃わないとビルドが通らない）。
- **失敗経路**: `on_disabled_hover_text` は純粋な表示ビルダーで副作用・失敗経路なし。返り値の
  `Response` を文として破棄するのは egui の通常パターン（`ui.label(...)` と同様）。

## テスト方針

- **自動テストは追加しない**（snotra-settings の egui UI テスト非採用方針に準拠）。ツールチップの
  「表示」はレンダリング挙動で、egui_kittest（AccessKit）の検証対象外。i18n キーと egui API 使用は
  コンパイル（clippy）で担保される。
- **検証コマンド**（`docs/build-commands.md` カテゴリ準拠）:
  - clippy（`.rs` 編集で PostToolUse フックが自動実行）
  - `cargo build -p snotra-settings`（手動ビルドで警告確認）
- **視覚スモーク（PR 前の目視・必須）**:
  1. 設定を起動し Search タブを開く。
  2. `migemo_enabled` チェックを外す → `最小文字数` フィールドがグレーアウト → ホバーで
     「ローマ字検索（Migemo）を有効にすると設定できます」が出る。
  3. `正規化:` を「無効」にする → `Fuzzy 履歴キャップ比率` がグレーアウト → ホバーで
     「正規化を「Fuzzy 相対キャップ」にすると設定できます」が出る。
  4. 言語を英語に切り替え、2・3 の英語文言を確認。
  5. 各フィールドを有効化するとツールチップが出ないことを確認（無効時のみ発火）。

## SPEC.md 更新要否

- **不要**。本変更は無効フィールドのホバー説明という UI ポリッシュのみで、検索挙動・IPC 契約・
  状態遷移・config スキーマのいずれも変更しない。SPEC.md はこれらフィールドの tooltip 有無を
  記述していない（§7.1 は設定項目の存在のみ）。→ AGENTS.md ステップ0「挙動変更を伴うか」判定で
  「文書化された挙動の変更なし」= バグでも仕様変更でもない純粋な UX 追加。

## 影響範囲外（触らない）

- `snotra-core`（config スキーマ）: 変更なし（既存フィールドを参照するだけ）。
- 本体 `src-tauri` / 検索ロジック: 無関係。
- SETTINGS-DESIGN.md: 新デザイントークン・新パーツを導入しないため更新不要
  （`on_disabled_hover_text` は標準 egui 機能）。「全無効フィールドに hover hint を付ける」という
  規約化は本 issue のスコープ外（YAGNI。必要なら別途合意の上で SETTINGS-DESIGN に追記）。
- モジュール構成（snotra-settings/CLAUDE.md）: 新規ファイルなし → 更新不要。

## セルフレビュー（Step 5）

### 5a. check スキル

- **`/symmetric-check`（該当・インライン実施済み）**: search.rs の `add_enabled_ui` を全 grep し、
  無効化数値フィールドが **migemo / fuzzy cap の2箇所**であることを確認。片方（issue 記載の
  migemo）だけでなく対称の fuzzy cap も対象に含めた。これが本計画の対称ペア対応の根拠。
- **`/plan-review`（右サイズ判断で省略）**: 本変更はロジック・状態・永続化・データフロー・async・
  キャッシュのいずれも持たない純加算 UI ヒント（2フィールド × 1行 + i18n キー2つ）。並列サブ
  エージェントによる影響範囲 fan-out は変更規模に対して不均衡なため、インラインのセルフレビュー
  （下記 1–8）+ symmetric-check で代替する。**過剰と判断した根拠を明示**（CLAUDE.md「やりすぎを
  歓迎」）。full fan-out が必要ならユーザー指示で追加実行可能。
- `/cache-check` `/persistence-check` `/state-check` `/race-check`: いずれも非該当
  （キャッシュ/on-disk 形式/UI モード遷移/async の変更なし）。

### 5b. チェックリスト

1. **対称コードパス**: ✅ 5a で全 `add_enabled_ui` を列挙。migemo + fuzzy cap の両方を対象化。
2. **影響範囲の網羅性**: ✅ `add_enabled_ui` を search.rs 全体で grep。他タブに無効化数値
   フィールドはない（本 issue は Search タブが対象）。
3. **境界条件**: ✅ 有効/無効 両状態、ja/en 両言語を視覚スモークに列挙。
4. **リソース管理**: 非該当（listen/Observer/子プロセス等のライフサイクル資源を生成しない）。
5. **既存パターンとの整合**: ✅ 既存 `add_enabled_ui` + i18n テーブル駆動をそのまま踏襲。新パターン
   なし。
6. **YAGNI 違反**: ✅ なし。規約化（全無効フィールド一律対応）や SETTINGS-DESIGN 追記には踏み込
   まず、issue スコープ（Search タブの2フィールド）に限定。
7. **シンプル化の挑戦**: ✅ 新状態フラグ・Mutex・子プロセスの導入なし。egui 組み込みメソッド1つ
   で完結。これ以上単純化する余地なし。
8. **破壊不変条件**: ✅ 「戻ってこない」系リスク（Win32 フック・ホットキー・IPC）なし。唯一の
   回帰候補だった `warn_if_rect_changes_id`（#456）は「ウィジェット存在は不変・操作可否のみ切替」
   のため非該当と論証済み。検知手段は視覚スモーク（無効時ホバーで文言表示 / 有効時は非表示）。
