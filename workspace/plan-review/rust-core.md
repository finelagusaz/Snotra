# plan-review: rust-core（issue #803）

## 問題なし

- `Config::config_dir()` は絞り点として単一（`config.rs:656-658` の `dirs::config_dir()` 呼び出しがこの1箇所のみ）。`rg 'dirs::config_dir'` 実行結果もこの1件のみで、plan 不変条件3の検知手段は妥当。
- `config_path()`（`config.rs:660-662`）・`is_first_run()`（`config.rs:649-654`）はいずれも `Self::config_dir()` / `Self::config_path()` 経由のラッパーであり、env 上書きに自動追従する。対称ペアとして別途の分岐追加は不要（plan の想定通り）。
- `load_from_dir_reporting`（`config.rs:875`）・`save_to_dir`（`config.rs:951`）は既に `dir: &Path` を引数で受ける注入済み関数。既存テスト（例: `config.rs:2894` `load_from_dir_parse_failure_backs_up_and_does_not_save`）は env に一切触れず temp dir を直接渡しており、新規4テストがこれらへ影響しない設計は妥当（既存テストと非重複、`config_dir_from` という新規テスト名で衝突なし・`grep -n "config_dir_from" config.rs` は定義前は0件）。
- `if let Some(x) = ... && cond` の let-chain パターンは `config.rs:634-636`（`dirs::desktop_dir()` の既存コード）で既に使用実績があり、edition 2024 で新規に導入するリスクはない。
- `src-tauri/src/commands/window.rs:75` `Command::new(&settings_exe).args(extra_args).spawn()` は `.env_clear()` / `.env_remove()` を呼んでいない。子プロセス（`snotra-settings.exe`）は親の env を継承するため、research C3 の主張（launcher 経由なら `SNOTRA_CONFIG_DIR` が settings 側にも効く）は実測で裏付けられる。
- テスト方針（`config_dir_from` を純粋関数化し env に触れない4テスト）は C4（Rust 2024 `set_var` の unsafe化・プロセス全域可変状態・並列テストへの漏れ）を正しく回避する設計。既存の `load_from_dir_reporting` 系テストとも独立に測れる。
- 不変条件1・4（`config_dir_from_falls_back_to_base_when_env_absent` / `_when_env_is_empty`）は `load_reporting()`（`config.rs:862-867`）の `None` 早期return契約と整合する形でテスト設計されている（4本目 `config_dir_from_is_none_without_override_or_base` が base=None のケースをカバー）。

## 軽微な懸念

- 呼び出し件数の記載が実測と食い違う。research.md:22 と issue 本文はいずれも「13箇所」と主張するが、`grep -n "Config::config_dir()" -r` で実際に数えると config.rs 3（660行台/863/946）+ history.rs 3（86/154/183）+ indexer.rs 3（396/463/590）+ window_data.rs 2（62/86）+ binfmt.rs 1（30）+ snotra-settings/backup.rs 2（104/110）= **14箇所**。research.md 自身が挙げた内訳（3+3+3+2+1+2）を合計しても14であり、見出しの「13」と本文内訳が既に矛盾している。実害は小さい（絞り点が1つである不変条件自体・`rg 'dirs::config_dir'` の検知手段は正しく、影響範囲の網羅性は損なわれない）が、AGENTS.md「全数を数え上げる」原則には反しており、plan.md 不変条件3の文言（「13箇所すべてが」）も同じ数字を引き継いでいるため、実装時に `rg -c 'Config::config_dir\(\)'` で再カウントし文言を14へ修正することを推奨する。

## 要対処

（なし）

## 未検証（理由）

- `config_dir_from` の `base` パラメータ型が `Option<PathBuf>`、`dirs::config_dir()` の戻り値型と完全一致するかはコンパイラが担保するため未実装の現時点では実行検証していない（型不一致があれば `cargo check` で即座に検出されるため、plan の質としては問題視しない）。
- `snotra-settings` プロセス自体が `Config::config_dir()`（同一 `snotra_core::config` 経由）を呼んでいることは `snotra-settings/src/tabs/backup.rs:104,110` の grep で確認したが、`snotra-settings.exe` 単独起動（`cargo run -p snotra-settings` 等、親プロセス非経由）時の env 伝播はスクリプト/プロセス起動経路の話であり rust-core の担当範囲外のため未検証（research C3 に記載あり、他レイヤーの検証観点）。
