# plan-review: scripts レイヤー（issue #803）

## 問題なし

- seed TOML の parse 可否: plan.md 60-77 行目の seed TOML（`[hotkey]`/`[appearance]`/`[general]`/`[visual]`/`[paths]`）を `snotra-core/src/config.rs` に一時テストとして追加し `cargo test -p snotra-core` で実測 green を確認済み（`toml::from_str::<Config>` 成功・`general.show_on_startup==true`・`visual.background_color=="#4A2B5C"`）。テストは検証後 `git checkout -- snotra-core/src/config.rs` で復元済み（差分は残していない）。
- 必須セクションの根拠: `Config`（config.rs:87-101）で `#[serde(default)]` が無いのは `hotkey`/`appearance`/`paths` の3つのみ。`AppearanceConfig`（302-323行）内で唯一 default 無しなのが `window_width`。plan の seed はこの3点を全て満たす。既存の `from_toml_str_fills_defaults` テスト（config.rs:3068-3089）・`visual_padding_defaults_for_missing_keys`（1147-1157）が同型の最小 TOML を使っており独立に裏付ける。
- `[general] show_on_startup = true` / `[visual] background_color` のキー名: `GeneralConfig.show_on_startup`（config.rs:140）・`VisualConfig.background_color`（config.rs:404）と一致。
- 変更ファイル一覧の網羅性: `grep -rn "visualcheck-bak|-Restore|check:colors"` を repo 全体（`docs/superpowers/` 含む）に対して実行した結果は `workspace/plan.md` / `workspace/research.md` / `docs/build-commands.md` / `package.json` / `scripts/visual-check-colors.ps1` の5件のみ。`.github/` と `CONTRIBUTING.md` に一致なし。plan.md の変更ファイル一覧（6-16行目）はこの5件のうちスクリプト本体以外の4件すべてを覆っている。
- 単一インスタンス起動前チェックの存置根拠: `tauri_plugin_single_instance::init`（src-tauri/src/main.rs:210）はプロセス/アプリ識別子ベースで判定しており config dir と無関係。プロファイル分離後も `visual-check-colors.ps1:99-102` のチェックは必要という plan の判断（research C6・plan 不変条件6）は実装を見て妥当。
- 自動判定の観測点（`$px=3, $py=[int]($h/2)`）: `$py` は `GetWindowRect` で得た実測 `$h` から毎回動的に算出しており、window.bin 不在で既定サイズ/位置になっても観測点自体は追従する。既存ユーザー環境でも window.bin の実際のサイズは毎回異なるため、プロファイル分離固有の新規リスクではない。
- `finally` を本体プロセス kill のみに絞る判断: `$env:SNOTRA_CONFIG_DIR` はスクリプトを実行する pwsh プロセスのローカル環境であり、プロセス終了と共に消える。使い捨てプロファイル（`$env:TEMP\snotra-visual-check\`）は次回実行時に上書きされるため、異常終了時に「生成したが破棄しない」リソース（実 config・プロセス以外）は残らない。
- `smoke-egui.ps1` への相互参照コメント追加が CI に無影響: `.github/workflows/e2e.yml:75` は `npm run smoke:egui -- ... -SeedConfig -RequireResults` を呼ぶだけで、`$seedToml`（smoke-egui.ps1:86-100）の内容差分ではなくコマンドライン引数で駆動されるため、コメント1行の追加は CI の合否に影響しない。
- `smoke:egui`/`smoke:startup`/`e2e.yml` の順序制約をスコープ外にする判断: `scripts/smoke-startup.ps1` に `APPDATA`/`config.toml`/`SNOTRA_CONFIG_DIR` への参照は無し（grep 0件）。issue #803 本文も「副次的な用途」として列挙するのみで必須要求にしていない。plan がこの2スクリプトを不変のまま残す判断は issue のスコープと整合する。

## 軽微な懸念

- 実 config 存在チェックの削除漏れの疑い: 現行 `visual-check-colors.ps1:88-90`（`if (-not (Test-Path $configPath)) { throw "config.toml が見つかりません..." }`）は退避対象とは別の「実 config が既に存在すること」を要求するガードだが、research.md の「付帯機構」表（35-44行）にも plan.md の Phase 2 チェックリスト（50-58行）にも明示的な削除対象として挙がっていない。新設計では実 config を読まないため、このガードは意味を失う（または `$configPath` 変数の再定義次第で無意味な no-op になる）。変数 `$configPath`/`$backupPath` 自体を作り直す過程で自然に消える可能性は高いが、明示チェック漏れとして記録する。
- `smoke-egui.ps1` の seed 文字列は `@"..."@`（expandable here-string、86-100行目）であり、追加する相互参照コメントに `$` を含む語（例: `$env:SNOTRA_CONFIG_DIR` をそのまま書く等）を入れると無警告で変数展開され空文字になりうる。コメント文言に `$` を含める場合は `` ` `` エスケープが必要（動作には影響しない可能性が高いが、追加時に一考の余地あり）。
- Phase 2 の seed TOML（plan.md 60-77行）は `-Interactive` と自動判定で `[general] show_on_startup` の要否が異なる（コメントで「自動判定のときだけ」と書かれているのみ）。旧実装は `if (-not $Interactive) { Set-TomlKey ... }` で分岐していたが、新設計でこの分岐をどう TOML 生成に反映するかの具体手順が plan に書かれていない（実装時の詳細に委ねられている）。

## 要対処

（該当なし）

## 未検証（理由）

- `config.toml.bak` 不在チェックが「env が効かず実 config を読んだ」経路を検出できるかの動的実測: 静的には、bak チェック対象パスが使い捨てプロファイル側（`$env:TEMP\snotra-visual-check\config.toml.bak`）である場合、env 未反映で実プロファイルが読まれても新プロファイル側には何も生成されないため bak 不在のまま「健全」に見えうる。ただし本体の背景色判定（ピクセル比較）が独立した安全網として働き、実プロファイルの色が偶然検証色と一致しない限り赤判定になるため、実害は低いと判断した。実際に `SNOTRA_CONFIG_DIR` を設定して `cargo run -p snotra` を起動し、新プロファイル配下に `config.toml`/`window.bin` 等が生成されることを実機で確認する検証は、実装が Phase 2 着手前のため未実施（plan レビュー段階では snotra-core 側の env フック自体が未実装）。
- `docs/build-commands.md`・`SPEC.md`・`snotra-core/CLAUDE.md` の記述整合性は担当レイヤー外（docs/rust-core 担当）のため本ファイルでは検証していない。
