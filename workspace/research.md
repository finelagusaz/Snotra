# research — issue #803: `SNOTRA_CONFIG_DIR` の env 上書き

## issue の要約

`Config::config_dir()` が `dirs::config_dir()/Snotra` に固定されているため、config を触る検証は
すべて「ユーザーの実 config を壊して元に戻す」形になる。env 上書きを 1 本入れて、検証を別プロファイル
で走らせられるようにする。`scripts/visual-check-colors.ps1` が抱える後始末の付帯機構 5 つを消す。

## 関連コード（grep で実在確認済み・2026-07-28）

### 絞り点

`snotra-core/src/config.rs:656-658`:

```rust
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("Snotra"))
}
```

- `dirs::config_dir()` の直接呼び出しは**この 1 行だけ**（`rg 'dirs::config_dir'` → 1 件）
- `Config::config_dir()` の呼び出しは **14 箇所**（`config.rs` 内 3 / `history.rs` 3 / `indexer.rs` 3 /
  `window_data.rs` 2 / `binfmt.rs` 1 / `snotra-settings/src/tabs/backup.rs` 2）。issue 本文と
  当初の本ファイルは「13 箇所」と書いていたが**誤り**である（内訳の合計は 14。自分で数え直した:
  `grep -rn 'Config::config_dir()\|Self::config_dir()' --include=*.rs` が 15 行、うち
  `binfmt.rs:18` は rustdoc の言及ゆえ呼び出しは 14）。`config.toml` /
  `history.bin` / `index.bin` / `icons.bin` / `window.bin` のすべてがここから導かれる

### 第二の導出経路は無い

- SPEC.md:637「設定フォルダを開く」の実装は `snotra-settings/src/tabs/backup.rs:104,110` で、
  どちらも `Config::config_dir()` を通る（`%APPDATA%` を組み立て直す経路ではない）
- ゆえに env 上書きは 13 箇所すべてに一様に効き、「導出が 2 経路」の欠陥
  （`docs/development-principles.md`「config の値は到達性の検出器を持たない」）は生じない

### 書き換える検証スクリプト

`scripts/visual-check-colors.ps1`（236 行）の付帯機構:

| 行 | 機構 |
|---|---|
| 61, 105 | 退避 `config.toml.visualcheck-bak` の作成 |
| 64-71, 235 | `Restore-SnotraConfig` と `finally` での復元 |
| 91-94 | 二重退避の明示エラー |
| 55, 73-76 | `-Restore` パラメータ |
| 110-113 | `[visual.custom_theme]` の同名キーまで書き換わる残余の注記 |
| 126-130 | 自動判定のため実 config へ `show_on_startup = true` を書く |

参照元: `package.json:14`（`check:colors`）、`docs/build-commands.md:66-75`（`-Restore` の行と
「実 config を退避して書き換える」の bullet）。他に `-Restore` / `visualcheck-bak` を参照する箇所は
リポジトリに無い（`docs/superpowers/` を除く grep で確認）。

## 既存パターン

- **env ハッチの前例**: `SNOTRA_TRACE`（`src-tauri/src/trace.rs`）・`SNOTRA_EGUI_*_TRACE`・
  `SNOTRA_EGUI_FAKE_UPDATE` / `_FAILED`・`SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE`・
  `SNOTRA_ICON_DIAG_PATHS`。SPEC.md:1101 に `SNOTRA_TRACE` が載っている（SPEC に env を書く前例）
- **注入点の前例**: `Config::load_from_dir_reporting(&dir)` / `save_to_dir(&dir)` が既に
  「`config_dir` を注入可能にして統合テストする」ために在る（`config.rs:869`, `950`）
- **最小の有効 config TOML の前例**: `scripts/smoke-egui.ps1:86-100` の `-SeedConfig`

## 技術的制約（実測・一次資料で確認）

### C1. `Config` の必須セクション — 空/部分 TOML は parse に失敗する

`config.rs:87-101` の `Config` で `#[serde(default)]` が**無い**のは `hotkey` / `appearance` /
`paths` の 3 つ。これらを欠く TOML は `toml::from_str::<Config>` に失敗し、
`load_from_dir_reporting` の parse 失敗 arm（`config.rs:886-895`）へ落ちて
`config.toml.bak` 退避 + 既定値起動になる。**検証用プロファイルの seed は必ずこの 3 セクションを
含める必要がある**（`smoke-egui.ps1:78-85` が同じ理由で注記している。PR #659 レビューで検出）。

### C2. first-run は `snotra-settings` を spawn してフォーカスを奪う

`src-tauri/src/main.rs:331-332` の `setup_first_run` が `is_first_run` のとき
`launch_settings_process(app_handle, &["--first-run"])` を呼ぶ。`Config::is_first_run()` は
`config_path()` 経由なので env 上書きに追従する = **空のプロファイルを指すと必ず first-run になる**。

`general.auto_hide_on_focus_lost` の既定は `true`（`config.rs:117-119`）なので、設定窓に
フォーカスを奪われた main 窓は隠れる。**自動判定は「何も映っていない場所」を撮ることになる。**
→ 検証用プロファイルには**起動前に config.toml を seed する**（first-run を踏ませない）。

### C3. 子プロセスは env を継承する

`src-tauri/src/commands/window.rs:75` は `std::process::Command::new(&settings_exe)` で spawn する
（env をクリアしていない）。ランチャに `SNOTRA_CONFIG_DIR` を与えれば `snotra-settings` の子も同じ
プロファイルを見る。一方 `cargo run -p snotra-settings` の単独起動には効かない（env を渡さない限り）。

### C4. Rust 2024 では `std::env::set_var` が unsafe かつプロセス全域

全 crate が `edition = "2024"`（`snotra-core/Cargo.toml:4` 他）。`cargo test` は同一プロセスの
複数スレッドでテストを並列実行するため、env を書き換えるテストは他のテストへ漏れる。
→ **env 読みを純粋関数へ分離してテストする**（`config_dir_from(override, base)`）。

### C5. `var_os` は空文字を `Some("")` として返す

`SNOTRA_CONFIG_DIR=` のようにシェルで空を渡すと `Some(OsString::new())` になり、
`PathBuf::from("")` は空パス = 相対パス扱いになって `config.toml` がカレントディレクトリに落ちる。
→ 空は「未設定」として既定へ落とす（境界条件）。

### C6. `tauri_plugin_single_instance` はプロファイルと無関係

app identity で単一性を判定するため、プロファイルを分けても 2 つ目のプロセスは既存インスタンスを
show して即終了する。`visual-check-colors.ps1:99-102` の起動前チェックは**プロファイル分離後も必要**。

### C7. ディレクトリは自動生成される

`save_to_dir`（`config.rs:952`）が `fs::create_dir_all(dir)` を呼ぶ。存在しないディレクトリを
`SNOTRA_CONFIG_DIR` に指定しても first-run で作られる（ただし C2 により seed は別途必要）。

## 未解決の疑問

- 空の `[paths]`（scan 0 件）で起動したとき、`apply_migrations` が既定 scan パスを埋め戻すかは
  未確認。**索引件数が変わるだけで背景色の判定には影響しない**ため、実装時に実測して確かめる
  （`smoke-egui.ps1:85` は「`PathsConfig.scan` は `#[serde(default)]` ゆえ空でも可」とだけ書いている）
