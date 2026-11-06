# 調査 — #1306 自動テストや実機確認の際にプロファイルを分離する

## issue の要約

- ペイン: テストで実プロファイルが上書きされる。確認用のスクリプトを書くとき、実機と検証用を混同する
- あるべき姿: プロファイルが完全に分離されていること
- ユーザーとの合意（2026-11-07・Step 1 の質問）
  - 範囲: **全部分離**。保存先に加えて、単一インスタンス制御・自動起動のレジストリ値・グローバルホットキーも、検証用プロファイルごとに分ける。実機と検証版を同時に動かせるようにする
  - 選び方: **env で明示（現状の拡張）**。`SNOTRA_CONFIG_DIR` を設定したときだけ分離する。既定（未設定）の挙動は変えない

## 現状: 分離済みのもの

- 保存先: `SNOTRA_CONFIG_DIR`（#803・2026-07-28 close）。`snotra-core/src/config/location.rs` の `Config::config_dir` / `config_dir_from` が唯一の導出点。`config.toml` / `history.bin` / `index.bin` / `icons.bin` / `window.bin` はすべてここから派生する
  - 契約: 値をそのまま使う・`Snotra` を付け足さない・空文字は未設定・展開も絶対化もしない（`config_dir_from` の rustdoc・SPEC §13）
- 子プロセス `snotra-settings` は env を継承する（`src-tauri/src/commands/window.rs` の `Command::new(&settings_exe)` は env を消さない）。設定アプリ側も `Config::config_dir()` を使う（`snotra-settings/src/tabs/backup.rs`・`tabs/index.rs`）
- 検証用プロファイルを使うスクリプト: `smoke-egui.ps1` / `smoke-startup.ps1` / `visual-check-colors.ps1` / `bench-startup.ps1 -UseVerificationProfile`（`scripts/lib/SnotraSmoke.psm1` の `New-SnotraVerificationProfile` と `Invoke-SnotraEnvironment`。`SNOTRA_CONFIG_DIR` は予約キー）
- cargo テスト: `snotra-core/CLAUDE.md` の「テストから実 `%APPDATA%\Snotra` に触れない」と、`SettingsApp::new` が `autostart_enabled` を引数で受ける形（`snotra-settings/src/app.rs`）で、テストが実ファイル・実レジストリを読まない形になっている

## 現状: 分離されていないもの（今回の対象）

### 1. 単一インスタンス制御

- `src-tauri/src/main.rs` の `tauri::Builder::default().plugin(tauri_plugin_single_instance::init(...))`
- 版: `tauri-plugin-single-instance` 2.5.5（Cargo.lock）。`semver` feature は無効（`src-tauri/Cargo.toml` は `"2"` のみ）
- Windows 実装（`platform_impl/windows.rs`）は `app.config().identifier` から 3 つの名前を作る: mutex `{id}-sim`・窓クラス `{id}-sic`・窓名 `{id}-siw`。mutex が既にあれば、`FindWindowW` で先発の窓を探し `WM_COPYDATA` で引数を送って終了する
- identifier は `src-tauri/tauri.conf.json` の `"identifier": "com.snotra.app"`。`tauri::generate_context!()` が返す `Context` は `config_mut()` で実行時に書き換えられる（tauri 2.12.5 自身が `src/path/desktop.rs` のテストで `context.config_mut().identifier = ...` としている）
- 現状の症状: `SNOTRA_CONFIG_DIR` で起動しても、実機の Snotra が動いていれば、実機の窓を出すだけで黙って終わる（exit 0・stderr 0 行・`src-tauri/src/startup.rs` の「二重起動は終端を出さない」）。SPEC §13 と `docs/build-commands.md` はこれを「単一インスタンス制御には影響しない」と明記している

### 2. 自動起動のレジストリ値

- `snotra-core/src/autostart.rs`: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` の値名 `RUN_VALUE_NAME = "Snotra"`。`enable()` / `disable()` / `is_enabled()`
- 呼び出し元は `snotra-settings/src/tabs/general.rs`（トグル）と `snotra-settings/src/app.rs` の `run`（起動時に読む）
- SPEC §7.7: 値名 `Snotra` は凍結（NSIS アンインストーラの `DeleteRegValue ... "${PRODUCTNAME}"` と一致している必要がある）
- 現状の症状: 検証用プロファイルで開いた設定アプリでチェックを入れると、実機の登録を **dev の `snotra.exe` のパスで上書きする**
- 構造上の制約: `Run` 値はコマンドラインしか持てず、env を運べない。SPEC §7.7 は「引数は付けない」と定める。ゆえに検証用プロファイルから登録した値は、サインイン時に**実プロファイルで**起動する。プロファイルごとの値名を作っても、そのプロファイルでは起動しない

### 3. グローバルホットキー

- `src-tauri/src/platform/hotkey.rs` の `RegisterHotKey`。システム全体で 1 つのキーは 1 か所にしか登録できない
- 失敗時の経路は実在する（起動時は `initial-hotkey-failed` で検索 UI を能動表示して通知・SPEC §7.5 / §10・#652。`scripts/occupy-hotkey.ps1` で実測済み・#1009）
- `New-SnotraVerificationProfile` の既定ホットキーは `Alt` + `Q`（`scripts/lib/SnotraSmoke.psm1:229-230`）で、実機の既定（SPEC §7.5 の `Alt+Q`）と同じ。`occupy-hotkey.ps1` はこの既定を AST から読む
- 今は単一インスタンスが先に落とすので衝突が表に出ない。単一インスタンスを分けると、実機が動いている開発機では検証版の `RegisterHotKey` が失敗し、さらに smoke が注入した `Alt+Q` は**実機へ届く**

### 4. スクリプトのプロセス特定（混同の温床）

- `scripts/measure-memory-stages.ps1:121,158,205` は `Stop-Process -Name snotra -Force` で**実機ごと**落とす。検証用プロファイルを使わず、実 `%APPDATA%\Snotra` で起動する
- `scripts/manual-smoke.ps1:230` は `Get-Process -Name snotra` の数で「既に動いている」を判定する（kill はしない）。実プロファイルで起動する
- `scripts/measure-memory.ps1:47` は名前でプロセスを集める
- `smoke-egui.ps1` / `smoke-startup.ps1` は自分が起動した `$proc.Id` で落とす。`smoke-egui.ps1:716-728` のシナリオ 1 終了待ちは、単一インスタンス転送を理由に書かれている（同じプロファイル同士は分離後も排他なので、理由は残る）

### 5. 状態を持つが分離の対象外と判断したもの

- 窓クラス `RegisterClassExW`（`src-tauri/src/platform/mod.rs:200`）: プロセス内でのみ有効。衝突しない
- トレイ: `guidItem` / `NIF_GUID` を使っていない（grep 0 件）。hwnd + uID で識別するので衝突しない
- updater（`tauri-plugin-updater` 2.14.1）: identifier を使わない（一時ファイルの置き場は `tempfile` 経由の一時ディレクトリ）
- tauri の identifier の他の用途（Windows）: `path()` の app_*_dir と IPC の `identifier` コマンド。本リポジトリのコードは `app.path()` も IPC も使わない（grep 0 件・IPC は #532 SU7 で消滅）。`tauri-runtime-wry` に identifier の使用は無い（3b で確認）

## 再利用できる既存のパターン

- 判定核と env 読みの分離: `config_dir_from(override, base)`（env を読まない純粋関数）+ `config_dir()`（env を読む薄い結線）+ 結線の pin テスト（`config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix`）
- 安定ハッシュ: `snotra-core/src/indexer/cache.rs:295` は `DefaultHasher` が Rust のリリース間で安定しないことを明記している。プロセス間（別ビルド間）で同じ名前を導く用途には、自前の安定な関数が要る
- スクリプト側: `New-SnotraVerificationProfile` / `Invoke-SnotraEnvironment` / `Stop-SnotraProcessAndWait`（`scripts/lib/SnotraSmoke.psm1`）

## 技術的な制約

- mutex 名に `\` を含めると名前空間の区切りとして解釈される。パスをそのまま identifier に入れられない。ハッシュ等で `[A-Za-z0-9.-]` に落とす必要がある
- 同じプロファイルを指す 2 プロセスは、同じファイルを書くので**排他のままでなければならない**。異なるプロファイルだけを並走させる
  - 同じディレクトリを別の綴り（相対/絶対・大文字小文字・`/` と `\`・末尾の区切り）で指すと、素朴なハッシュでは別 identifier になり、同じファイルを 2 プロセスが書く
  - `SNOTRA_CONFIG_DIR` に実 `%APPDATA%\Snotra` そのものを渡すと、上書き付きなのに実機と同じファイルを書く
  - `config_dir_from` は「展開も絶対化もしない」契約。保存先の契約は変えずに、identifier を導く側だけで正規化する必要がある
- `config_dir` は起動の時点でディレクトリが存在するとは限らない（first-run で `Config::load` が作る）。`std::fs::canonicalize` は存在しないパスで失敗する
- `RUN_VALUE_NAME` と NSIS の `productName` の結合（SPEC §7.7）を壊さない。既定の値名は変えない

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1306.txt`（sonnet 1 体）。採るのは所見で、機序は一次資料で裁定した。

測定環境（その枠が実測）: `SNOTRA_CONFIG_DIR` は未設定。実 `%APPDATA%\Snotra` には `index.bin` と有効な `config.toml` があり、Run の `Snotra` はインストール版 `C:\toolbox\Snotra\snotra.exe`、実機が稼働中。ゆえに既定の cargo test は「config 既存・cache-HIT」の分岐しか通らない。

### 壊せた／不正確（採る）

- **A1 スクリプトの列挙が不完全（命題 4）**: 採る。主が `scripts/lib/SnotraSmoke.psm1:543-566` を読んで確かめた。`Resolve-SnotraExistingProcess -Policy Stop` は `Get-Process -Name snotra` の**全部**を落とす。呼び出しは `smoke-egui.ps1:151`・`smoke-startup.ps1:64`・`bench-startup.ps1:117` で、検証用プロファイルを使う smoke でも実機を落とす。`visual-check-colors.ps1:92` の `-Policy Reject` は実機が動いていると拒む。`visual-input-metrics.ps1` も検証用プロファイルを使う（列挙漏れ）
- **A2 `Wait-SnotraWindow` は並走で壊れる**: 採る。`SnotraSmoke.psm1:926` は `FindWindowW(null, $Title)` で**最初の 1 窓**だけを取り、owner が違えば見送るだけで次の窓を探さない。実機の窓が先に見つかると、検証版の窓が出ていてもタイムアウトする。呼び出しは `smoke-egui.ps1:670,821`・`visual-check-colors.ps1:269,298`・`visual-input-metrics.ps1:184`。実機の窓タイトルが `Snotra` であることは主が未測定（⚠️）だが、本体は 1 種類の exe なので同じタイトルを持つと見るのが自然で、計画は PID で探す形にして測定に依らなくする
- **A3 テストは実プロファイルを読み、条件次第で書く（命題 5）**: 採る。主が確かめた
  - `snotra-core/src/search/tests/common.rs:51,74` の `real_index_entries` / `real_scanned_entries` と `search/tests/path.rs:510` は `Config::load()` を呼ぶ。`Config::load` は `load_from_dir_reporting`（`config/io.rs`）を通り、**first-run なら既定で `config.toml` を作り、migration で変化があれば保存し、壊れていれば `.bak` へ rename する**。`#[ignore]` の付かない `*_over_real_index`（`search/tests/basic.rs:706`・`build.rs:573,911`・`path.rs:321` ほか）がこれを通る。実 config が既に新形式の開発機では書かないので、いまの開発機では差分ゼロに見える（その枠の実測: 既定の `cargo test --workspace` の前後で実ファイル 12 件と Run 値の差分ゼロ。空の `SNOTRA_CONFIG_DIR` では `config.toml` が 1 本できた）
  - `src-tauri/src/state.rs:151` と `src-tauri/src/commands/system.rs:37` のテスト fixture は `HistoryStore::load()` を使う（`snotra-core/CLAUDE.md` の #963 規約に反する）。読むだけだが、テストの初期状態が開発機に依存する
  - `snotra-core/tests/` の `#[ignore]` 計測コードの `Config::load()` / `HistoryStore::load()` は `snotra-core/CLAUDE.md` が明示的に許している。範囲外とする
- **A4 単一インスタンス判定より前に保存が走る**: 所見は事実（`src-tauri/src/main.rs:167-228` で `Config::load_reporting` / `load_or_scan_with_stats` / `HistoryStore::load` が `tauri::Builder` より前）。ただし**今回の変更で悪化しない既存の性質**であり、同じプロファイルの 2 プロセス目が起きる状況は今と同じ。範囲外として残余に記す
- **A5 mutex 名に `\` を含めると排他が黙って消える**: 採る（制約として）。plugin は `CreateMutexW` の NULL を確かめない。identifier を `[a-z0-9.-]` に限る不変条件をテストで固定する
- **A6 updater の一時ディレクトリ・WebView2 の記述**: 結論は同じ。記述だけ直す（下の「状態を持つが分離の対象外」）

### 壊せなかった

- 命題 1（identifier だけで名前が決まり、`config_mut` で並走できる。Windows で identifier を読むのは IPC の `identifier` コマンドと `path()` の `app_*_dir` だけで、本リポジトリはどちらも使わない。AUMID は tauri / tao / wry / 本リポジトリに 0 件）
- 命題 2（Run 値は env を運べない）。修飾: 引数は運べるが SPEC §7.7 が禁じている。より重い害は「実機の登録が dev exe のパスへ差し替わる」こと
- 命題 3（seed の既定は `Alt` + `Q`。引数で上書きできる）
- 保存先の導出点が唯一・子プロセスの env 継承・`SNOTRA_CONFIG_DIR` が予約キー・`NIF_GUID` 0 件・`DefaultHasher` の注意書き・`config_mut` の用例・SPEC §7.7 の値名凍結

### ⚠️ 確信なし（記録のみ）

- 二重起動時の「stderr 0 行」: 既存記述（2026-08-10 実測として `src-tauri/src/startup.rs` の `//!` にある）。今回は触らない
- `snotra-core/src/opener.rs:312` の `LOCALAPPDATA` 直読み: 主が確かめた。既定の opener プリセットの探索（読むだけ）で、プロファイルの状態ではない
- NSIS インストーラが実行中の `snotra.exe` を名前で扱う挙動: updater を検証版から実走させる場面は範囲外
- `canonicalize` の `\\?\` 前置き: 計画は `canonicalize` を使わない（下の Q1）ので該当しない
- mutex 名の長さ: identifier は固定長のハッシュにするので該当しない

## インデックス・キャッシュ（ユーザーの問い・2026-11-07）

- `index.bin`・`icons.bin`・`history.bin`・`window.bin` はすべて `Config::config_dir()` から導かれ、#803 で分離済み
- 製品コードで他の場所に永続ファイルを置く経路は無い: `dirs::` の呼び出しは `config/location.rs`（保存先）と `config/paths.rs:138`（既定のスキャン先にデスクトップを足す・読むだけ）だけ。`temp_dir()` はテストのみ
- 既定のスキャン先（スタートメニュー・デスクトップ）は実機と検証版で共有するが、読むだけの入力である

## 未解決の疑問

- Q1: 同じディレクトリの別綴りを、identifier の導出でどこまで同一視するか（canonicalize するか、文字列の正規化で済ますか、ディレクトリを先に作るか）
- Q2: 検証用プロファイルで開いた設定アプリのスタートアップ節をどう見せるか（無効化して説明を出す／隠す）。`is_enabled()` を読むかどうか
- Q3: ホットキーの「分離」を何で達成するか（検証用プロファイルの seed を実機の既定と違うキーにする／製品側で何かする）。手で `cargo run` したプロファイルの first-run は `Alt+Q` を書くので、実機と衝突して失敗通知が出る
- Q4: `measure-memory-stages.ps1` / `manual-smoke.ps1` / `measure-memory.ps1` を検証用プロファイルへ移すか。`measure-memory-stages` は実索引の規模で測る意図があるかもしれない（`PERFORMANCE.md` の測定条件の記述を確かめる）
- Q5: 「テストで上書きされる」が、いま現在の cargo テストで実際に起きているか（フルテストの前後で実 `%APPDATA%\Snotra` の mtime と HKCU Run が変わらないか）
- Q6: identifier を書き換えると、tauri の内部で他に効く所（Windows の AUMID・通知など）は無いか
