# settings-test レビュー（ラウンド3・snotra-settings層）

## 1. 例外リストの「2件目」認識

**要対処**（落とし先: 計画の修正、`workspace/plan.md` Phase 3 の該当行と未確定欄の記述）

`snotra-settings/CLAUDE.md:142-144` の実際の記述:

```
142  - ユニットテストは書かない方針（egui UI コードはモック困難）。ロジックのテストは `snotra-core` 側で行う
143    - 例外1: 純粋な非 egui ヘルパー（例 `font.rs` の `face_index_valid`）の境界テストはインラインで置いてよい。...
144    - 例外2: **UI 操作 + 状態観測**は `egui_kittest`（AccessKit）でヘッドレステストできる（下記「ヘッドレス UI テスト」）。...
```

例外リストには既に **2 件**（例外1=`font.rs`の`face_index_valid`、例外2=`egui_kittest`によるUI操作+状態観測）が存在する。計画が言う「`font.rs` の `face_index_valid` に続く2件目」は誤り——今回追加する `preset_matches` の境界テストは例外1と**同種**（純粋な非egui ヘルパー）の2件目ではあるが、リスト全体としては**3件目（例外3）**になる。この曖昧な言い回しのまま実装すると、書き手が「例外2」というラベルを使ってしまい既存の例外2（kittest）と重複ラベルになる、または既存例外2を上書き・混同するリスクがある。計画の当該行（`plan.md:73`「`font.rs` の `face_index_valid` に続く2件目」）を「例外3として追加する（例外1と同種、例外2=kittestとは別枠）」と明示的に修正すべき。

## 2. テストのコンパイル可能性

**問題なし**

- `preset_matches(config: &Config, p: &PresetDef) -> bool`（`visual.rs:202`）は private fn、`PresetDef`（`visual.rs:10-18`）も private struct・private フィールドだが、計画通り `#[cfg(test)] mod tests` を同一ファイル（`visual.rs`）内に新設するため `use super::*;` で解決でき、可視性の問題は無い（`snotra-settings/CLAUDE.md:150-151` の `app.rs` 前例と同型の対処）
- `PRESETS: &[PresetDef]` に対する `PRESETS.iter().find(|p| p.preset == ThemePreset::Obsidian)` は `Option<&PresetDef>` を返す（`find` の Item は `&PresetDef`）。クロージャ引数は `&&PresetDef` だがフィールードアクセスは自動 deref されるためコンパイルは通る。`p.preset` の比較には `ThemePreset` の `PartialEq`（`config.rs:385` で derive 済み）が必要で満たされている
- `Config::default()` は `snotra-settings/Cargo.toml:14` の通常 `[dependencies]`（dev-dependency ではない）にある `snotra-core` から呼べる。`impl Default for Config`（`config.rs:547`）は既存
- 計画は `find()` の返り値を `unwrap()` か `expect()` かまで明記していない（`plan.md:70-72`）。定数配列に対する検索で実質失敗しないため軽微だが、パニックメッセージ次第で「既定色を変えるならObsidianも直せ」という計画の失敗メッセージ意図（`plan.md:72`）とどちらに付くか（`find`側か`assert`側か）を実装時に決める必要がある——**軽微な懸念**として扱う（ブロッキングではない）

## 3. Phase 1 の `preset` 変更によるテスト意味への影響

**問題なし**

- `config.rs:435` の `preset: ThemePreset::Obsidian` は Phase 1 で `default_theme_preset()`（`config.rs:337-339`、同じく `ThemePreset::Obsidian` を返す）へ置換される予定。値は変わらない
- さらに `preset_matches`（`visual.rs:202-213`）は `background_color` / `input_background_color` / `text_color` / `selected_row_color` / `hint_text_color` の5色のみを比較し、`config.visual.preset` フィールドは一切参照しない。ゆえに Phase 1 の変更はこのテストの合否に構造的に影響しない（二重の理由で安全）
- 色5本の既定値（`config.rs:341-359` の `default_background_color` 等）と `PRESETS` の Obsidian エントリ（`visual.rs:20-29`）は現状一致（`#282828` / `#383838` / `#E0E0E0` / `#505050` / `#808080`）していることを直接確認済み
