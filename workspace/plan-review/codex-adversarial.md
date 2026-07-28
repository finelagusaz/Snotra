[SEVERITY: serious] 相対パスまたは `%VAR%` を `SNOTRA_CONFIG_DIR` に渡すと、設定データが意図せず起動 CWD 配下へ書かれる。  
根拠: plan は上書きを「そのまま」`PathBuf::from(dir)` に渡す実装である（[workspace/plan.md](/C:/workspace/Snotra/workspace/plan.md:34), [workspace/plan.md](/C:/workspace/Snotra/workspace/plan.md:37)）。実行結果: `Path.IsPathRooted('profile')` は `False`、`GetFullPath('profile')` は `C:\workspace\Snotra\profile`、`Path.IsPathRooted('%TEMP%\\Snotra')` は `False`、`GetFullPath('%TEMP%\\Snotra')` は `C:\workspace\Snotra\%TEMP%\Snotra` だった。Windows は `var_os` の `%TEMP%` を展開しない。  
なぜ既存のレビューが見落としたか: 空文字だけを CWD 流出境界として扱い、非空の相対・未展開環境変数を実測していない。

[SEVERITY: minor] `SNOTRA_CONFIG_DIR` が既存ファイルを指す場合、設定は保存できず既定値で動くため、「空文字以外は安全」という境界網羅が崩れる。  
根拠: 上書きは無検証でそのまま使う計画である（[workspace/plan.md](/C:/workspace/Snotra/workspace/plan.md:28)）。`config.toml` は `dir.join("config.toml")` として読む（[config.rs](/C:/workspace/Snotra/snotra-core/src/config.rs:875)）、読込失敗は `ReadFailed`・既定値起動になる（[config.rs](/C:/workspace/Snotra/snotra-core/src/config.rs:912)）。保存側は `create_dir_all(dir)` を要求する（[config.rs](/C:/workspace/Snotra/snotra-core/src/config.rs:952)）。実行結果: `C:\workspace\Snotra\Cargo.toml` は `Exists=True; DirectoryExists=False`。従ってこれを override にするとディレクトリとして利用できない。  
なぜ既存のレビューが見落としたか: override を「通常のディレクトリ」と仮定し、ファイル型の入力を境界表に含めていない。

[SEVERITY: serious] `config.toml.bak` が無いことは seed の parse 成功を証明しない。壊れた seed でも退避 rename に失敗すれば `.bak` なしで既定値起動する。  
根拠: parse 失敗時は `backup_invalid` を呼んだ後、結果に関係なく `RecoveredFromCorrupt` を返す（[config.rs](/C:/workspace/Snotra/snotra-core/src/config.rs:886), [config.rs](/C:/workspace/Snotra/snotra-core/src/config.rs:893)）。`backup_invalid` は `fs::rename` 失敗をログするだけで、元ファイルを残して既定値続行する（[config.rs](/C:/workspace/Snotra/snotra-core/src/config.rs:927), [config.rs](/C:/workspace/Snotra/snotra-core/src/config.rs:938)）。一方 plan は `.bak` 不在を seed 健全性の判定にする（[workspace/plan.md](/C:/workspace/Snotra/workspace/plan.md:60)）。  
なぜ既存のレビューが見落としたか: `.bak` の生成経路だけを見て、退避が best-effort である失敗経路を判定器の反例として扱っていない。

[SEVERITY: minor] `target/visual-check/profile` は常に `cargo clean` で回収されるわけではない。`CARGO_TARGET_DIR` を設定した環境では Cargo の clean 対象が別ディレクトリになる。  
根拠: plan は profile をリポジトリ `target/` に置き、`cargo clean` が掃くと主張する（[workspace/plan.md](/C:/workspace/Snotra/workspace/plan.md:94), [workspace/plan.md](/C:/workspace/Snotra/workspace/plan.md:96)）。実行結果: `CARGO_TARGET_DIR=C:\cargo-target-803-proof` で `cargo metadata --no-deps --format-version 1` の `target_directory` は `C:\cargo-target-803-proof` になった。したがって、この環境の `cargo clean` は `C:\workspace\Snotra\target\visual-check\profile` を対象にしない。  
なぜ既存のレビューが見落としたか: Cargo の target ディレクトリが環境変数で差し替え可能な条件を検査していない。

## 反証できなかったもの

1. `Config::config_dir()` 以外の launcher 永続データ経路: リポジトリ内の Rust 呼出しと `dirs::config_dir` を検索したが、config/history/index/icons/window の別導出は確認できなかった。updater は [tauri.conf.json](/C:/workspace/Snotra/src-tauri/tauri.conf.json:27) に設定があるが、プラグイン自身の保存先をこのリポジトリの証拠だけでは確定できない。

2. seed 後にも別の起動経路が focus を奪う件: first-run の settings 起動は [main.rs](/C:/workspace/Snotra/src-tauri/src/main.rs:331) で確認したが、seed 済み通常起動で別窓を spawn する確実な経路は確認できなかった。破損時のトレイ通知は [main.rs](/C:/workspace/Snotra/src-tauri/src/main.rs:501) にあるが、seed 済み条件の反例にはならない。

3. 空文字以外の境界: 相対パス、未展開 `%VAR%`、既存ファイルは反証できた。UNC・長大パスについて、この実装だけから「必ず危険」と言える一次証拠は得られなかった。

4. `target/visual-check/profile` の安全性: `CARGO_TARGET_DIR` 条件で cleanup 主張を反証した。`.gitignore` の `/target` 指定自体は確認でき、Git 追跡漏れは反証できなかった。

5. 既定保存先の破壊的不変条件の検証: plan 自身が、4 unit test では wrapper の `dirs::config_dir()` 呼出しを検出できず、env なし目視を唯一の検出器として明記している（[workspace/plan.md](/C:/workspace/Snotra/workspace/plan.md:268)）。これは既存レビュー済みのため、新規反証はない。

6. SPEC のスコープ宣言: `SPEC.md` の4箇所と別文書 `architecture.md` の扱いを確認したが、計画どおり文書全体への明示的スコープと別文書の参照を入れるなら、追加の不整合は立証できなかった。

7. `.bak` 不在による seed parse 証明: 退避失敗経路で反証できた。