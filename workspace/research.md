# research — issue #835 検索結果ウィンドウの高さを常に visible_rows 分確保する

## issue の要約

`results` 窓の高さが実件数フィット（`min(件数, visible_rows) × 行高 + 8px`）であるため、候補が少ないと窓が 1 行まで縮む。#743 の「`←` が階層を上げない」という誤読の実因がこの激しい伸縮であった。**候補が `visible_rows` 未満でも常に `visible_rows` 分の表示領域を確保する**（= #646 PR2 決定 7 を覆す仕様変更）。

## 「現状でもこの問題は起きているか」（着手前の確認・ユーザー指示）

**起きている。** 一次証拠は 3 点。

1. 高さ算出は今も実件数フィットである — `src-tauri/src/egui_shell/layout.rs:73-80` の `results_window_height` が `n = result_count.min(max_results)` を行数に使う。`#675`（下端クランプ）・`#752`（連言の分解）・`#755/#801`（バー高でのクランプ）といった後続の変更は、いずれもこの式に触れていない
2. その式をテストが固定している — `layout.rs:511`（`results_window_height(3, 8, row) == 3.0 * row + 8.0`「実件数」）。**3 件なら 3 行分**という挙動が回帰テストで守られている
3. 「1 行まで縮む」経路も現存する — `←` は `launcher_controller.rs:1166-1176` で `navigate_folder(parent)` を撃ち、`spawn_folder_load` の列挙結果がそのまま `result_count` になる。件数に下限は設けられていないため、子が 1 件の親フォルダでは 1 行になる

つまり issue 起票（2026-07-29）から今日まで、症状も経路も無傷で残っている。実機再現は不要と判断した（式とテストが一次証拠であり、実機は同じ式の下流を見るだけである）。

## 関連ファイル・シンボル

| 位置 | 役割 |
|---|---|
| `src-tauri/src/egui_shell/layout.rs:73` `results_window_height` | 高さ算出の正本。**変更の中心** |
| `src-tauri/src/egui_shell/layout.rs:249` `present_results` | SPEC §8.6 の 4 連言。`desired_height > 0.0` が連言②「結果が空でない」を代行している |
| `src-tauri/src/egui_shell/layout.rs:198` `ResultsInputs` | 生の入力（`main_visible` / `plain_hidden` / `result_count` / `max_results` / `row_height`） |
| `src-tauri/src/egui_shell/layout.rs:95` `clamp_results_height` | 作業領域下端でのクランプ（#675）。`desired == 0.0` は素通し・床は 1 行 + 8px |
| `src-tauri/src/egui_shell/window_coordinator.rs:811-854` `drive_results_window` | 判定 → 位置決め → クランプ → `set_size` → `show` の driver |
| `src-tauri/src/egui_shell/window_coordinator.rs:729-745` `max_results` | `appearance.effective_visible_rows()` を `u32` で渡す |
| `snotra-core/src/config.rs:357` `effective_visible_rows` | `visible_rows.unwrap_or(default 8)`。**clamp を持たない** |
| `SPEC.md` §4.5（169-173 行） | 「実件数にフィットする」の正本 |
| `SPEC.md` §8.6「検索結果ウィンドウの可視性」（544-562 行） | 連言図と連言項の正本表 |
| `SPEC.md` §4.7（184 行） | 「`results` の高さは実件数フィット（§4.5）」の参照 |

既存テスト（意味が変わるもの）: `layout.rs:511-514`（`results_window_height` の 4 本）・`layout.rs:606`（`3.0 * 37.0 + 8.0` のリテラル）・`layout.rs:650-680`（旧式クロージャで `present_results` と突き合わせる回帰テスト群）。

## 技術的制約

- **`max_results == 0` は到達可能である。** `ResultsInputs.max_results` の doc（`layout.rs:207-211`）が「本体の config 適用経路は `Config::validate()` を通らず、設定 UI の `1..=50` clamp は `config.toml` の手編集を止めない」と明言する。現在は `min(count, 0) = 0 → 高さ 0 → Hidden` で救われているが、固定高さ `max_results × row_height + 8` にすると **0 件でも 8px のスリット窓が出る**
- **「高さ 0 ⇒ hide」の担い手が消える。** 0 件の非表示は今 `present_results` の `desired_height > 0.0`（`layout.rs:251`）が担う。固定高さ化でこの項は常に真になるため、0 件の hide は `result_count == 0` の独立した連言へ移す必要がある。SPEC §8.6 の連言図（549 行）は既に「結果が空でない」を別項として書いており、**実装だけが 2 項を融合していた**。#752 が「②と④を区別できなかった」ことを解いた延長線上にあり、逆行ではない
- **下端クランプ（#675）は位置起因の制約であり、件数起因の伸縮とは別軸である。** `clamp_results_height` は残す。固定高さが作業領域下端に収まらない場合は従来どおり下端で抑え、床（1 行 + 8px）も維持する
- `results` 窓の 3 操作（show / hide / topmost）は raw Win32（`ResultsWindow`）。`set_size` は tao 経由のままでよい（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」）
- **`icon_cache_cap` は影響を受けない。** `snotra-core/src/config.rs:626` の導出は `max(effective_visible_rows, effective_result_limit, effective_recent_limit) × 5` で、**`result_count`（実際の表示件数）を入力に持たない**。本変更が変えるのは「何行分の高さを取るか」だけで `visible_rows` の値そのものは変わらないため、cap の値は変更前後で同一である（issue 論点 4 の回答）

## 再利用できる既存パターン

- 高さ算出・可視性判定はいずれも `layout.rs` の純粋関数であり、Win32 非依存でユニットテストできる（受け入れ条件 4 はここで満たす）
- 過去決定の反転は `docs/adr/ADR-<slug>.md` に「否定の知識」として残す運用（#593）。先例: `ADR-show-path-derives-drawn-height`（#755/#801 で `ADR-results-presentation-two-stage` の一部を反転した記録）

## 未解決の疑問（plan.md の「未確定」へ引き継ぐ）

1. 0 件のときの扱い — issue コメント（2026-07-30）は「0 件: 検索結果ウィンドウ表示しない」と答えているが、**モックを見ながら確定したい**とも書かれている
2. 固定高さが下端に収まらないときの見え方 — クランプで結局縮むなら伸縮の解消にならない、という論点 3 の実体
3. `max_results == 0` を Hidden で守るか、`max(1)` で床を張るか
