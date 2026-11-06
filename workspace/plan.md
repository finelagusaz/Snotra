# 計画 — #1306 自動テストや実機確認の際にプロファイルを分離する

調査は `workspace/research.md`、敵対的調査は `workspace/adversarial-1306.txt`。

## 目的

`SNOTRA_CONFIG_DIR` を設定した起動（以下「分離プロファイル」）を、保存先だけでなく、単一インスタンス・自動起動のレジストリ・ホットキーについても実機の Snotra から切り離す。実機と検証版を同時に動かしても、互いの状態を変えないようにする。あわせて、テストとスクリプトが実機の状態を書く・落とす・掴む経路を塞ぐ。

既定（env 未設定）の挙動は変えない（ユーザー裁定・2026-11-07「env で明示（現状の拡張）」）。

## 受け入れ条件

1. 実機の Snotra が動いている開発機で、`SNOTRA_CONFIG_DIR` を別のディレクトリへ向けて `snotra.exe` を起動すると、2 つ目のプロセスとして常駐する。実機の窓は出ない
2. 同じ分離プロファイルを指す 2 回目の起動は、従来どおり 1 回目へ転送して終わる。同じディレクトリを別の綴り（相対/絶対・大文字小文字・`/` と `\`・末尾の区切り）で指しても同じ扱いになる
3. `SNOTRA_CONFIG_DIR` が実 `%APPDATA%\Snotra` そのもの（を別の綴りで書いたもの）を指すときは、分離プロファイルとして扱わない（実機と同じ単一インスタンス・自動起動は通常どおり）
4. 分離プロファイルで開いた設定アプリは、スタートアップ登録を読みも書きもしない。チェックボックスは無効で、理由を表示する。`snotra_core::autostart::enable` / `disable` は分離プロファイルでは レジストリに触れずにエラーを返す
5. 分離プロファイルが既定値の config を使うとき（初回起動・壊れた config の復旧・読めないとき）、ホットキーは `Alt+F12` になり、実機の既定（`Alt+Q`）と衝突しない。検証用プロファイルの seed の既定も `Alt+F12`
6. 検証用プロファイルを使うスクリプト（`smoke-egui` / `smoke-startup` / `visual-check-colors` / `bench-startup -UseVerificationProfile`）は、実機のプロセスを落とさず・拒まず・その窓を掴まない
7. 既定の `cargo test` は、実プロファイルに `config.toml` が無い・旧形式・壊れている開発機でも、実プロファイルへ書かない。src-tauri のテスト fixture は実 `history.bin` を読まない
8. SPEC §13・§7.7 と `docs/build-commands.md` が上の挙動を記述する

## 設計

### プロファイル鍵（core）

- `snotra-core/src/config/location.rs` に `pub fn isolated_profile_key() -> Option<String>` を足す（`Config` の関連関数）。分離プロファイルなら `Some(16 桁の小文字 16 進)`、そうでなければ `None`
- 判定核 `fn isolated_profile_key_from(override_dir: Option<OsString>, base: Option<PathBuf>) -> Option<String>`（env を読まない）。`config_dir_from` と同じ形で、結線は env を読むだけの pin テストで固定する
  1. 上書きが `None` か空文字 → `None`（`config_dir_from` と同じ判定。共通化する: 「上書きが有効か」の述語を 1 つにする）
  2. 比較用の鍵 `profile_identity(path) -> String`: `std::path::absolute`（存在を要求しない・`..` と `/` を解決する）→ `to_string_lossy` → 既存の `config/paths.rs` の `normalize_scan_path_key`（`pub(crate)`・trim・`/` → `\`・小文字化・ドライブ直下以外の末尾 `\` を除く）。正規化を書き起こさない（/dry-check）
  3. `base` があり、`profile_identity(上書き) == profile_identity(base.join("Snotra"))` → `None`（実プロファイル）
  4. それ以外 → `Some(format!("{:016x}", fnv1a64(profile_identity(上書き).as_bytes())))`
- `fnv1a64` は location.rs 内の private fn（定数 2 つ・数行）。`DefaultHasher` を使わない理由: 別ビルドのプロセス同士で同じ鍵を導く必要があり、`DefaultHasher` はリリース間で安定しない（`indexer/cache.rs:295`）
- **保存先の契約（`config_dir_from`: そのまま使う・展開も絶対化もしない）は変えない。** 正規化するのは鍵の導出だけである
- `absolute` が失敗した場合（CWD を取れない等）は、正規化せずに文字列のまま鍵を作る（`None` へ落とさない。落とすと実機と同じ単一インスタンスになり、検証が黙って実機へ転送される。`config_dir_from` の「不正な値を既定へ落とさない」と同じ向き）

### 単一インスタンス（src-tauri）

- `src-tauri/src/main.rs`: `let app_context = tauri::generate_context!();` を `let mut` にし、`Config::isolated_profile_key()` が `Some(key)` なら `identifier` を `format!("{}.profile-{key}", 元の identifier)` へ書き換える（`config_mut()`）。書き換えは 3 行で、別関数に括り出さない（`startup.rs` は計測コードの責務なので置かない）。理由はコメントで `SPEC.md` §13 を指す
- 不変条件: 生成される identifier は `[a-z0-9.-]` だけからなる（`\` を含むと plugin の mutex が黙って排他を失う・research A5）。元の identifier（`com.snotra.app`）は固定なので、鍵が `[0-9a-f]{16}` であることを core のテストで固定すれば足りる
- identifier は Windows では plugin の名前（`{id}-sim` / `-sic` / `-siw`）以外に効かない（research 命題 1）

### 自動起動（core + settings）

- `snotra-core/src/autostart.rs`: `AutostartError::IsolatedProfile` を足す。`enable()` / `disable()` は冒頭で `Config::isolated_profile_key().is_some()` なら、レジストリを開かずに `Err(IsolatedProfile)` を返す。`#[cfg(not(windows))]` 版も同じ
  - 書く側の入口で塞ぐのは、呼び出し側（設定アプリ）の UI の分岐を忘れても実機の登録を上書きしないためである
  - `is_enabled()` は変えない（読むだけ）
- `snotra-settings/src/app.rs` の `run`: 分離プロファイルなら `is_enabled()` を呼ばず `false` を渡し、`GeneralTabState` に分離プロファイルであることを渡す（`SettingsApp::new` は引数で受ける形を保つ・#963 と同じ理由）
- `snotra-settings/src/tabs/general.rs`: 分離プロファイルではチェックボックスを無効にし、説明文を出す。`autostart_message` に `IsolatedProfile` の腕を足す
- `snotra-settings/src/i18n.rs`: 説明文とエラー文の `TrKey` を ja / en で足す。文言: 「検証用プロファイル（SNOTRA_CONFIG_DIR）で起動しているため、スタートアップ登録は変更できません」/ "Startup registration can't be changed while running with a verification profile (SNOTRA_CONFIG_DIR)."

### テストが実プロファイルを書かない

- `snotra-core/src/search/tests/common.rs` の `real_index_entries` / `real_scanned_entries` と `search/tests/path.rs:510`: `Config::load()` をやめ、テスト専用の読み取り関数 `read_real_config_read_only() -> Option<Config>` を common.rs に置く。`Config::config_path()` のファイルを読み、`Config::from_toml_str` → `apply_migrations()`（メモリ上のみ）。ファイルが無い・読めない・壊れているときは `None`（呼び出し側は既存のスキップ経路へ）
- `src-tauri/src/state.rs:151` と `src-tauri/src/commands/system.rs:37` の fixture: `HistoryStore::load()` → `HistoryStore::empty()`
- `snotra-core/tests/` の `#[ignore]` 計測コードは触らない（`snotra-core/CLAUDE.md` が許している）

### ホットキー（製品・分離プロファイルの既定値）

ユーザー裁定（2026-11-07・承認の問いへの回答「初回のホットキーも分ける」）: 分離プロファイルの初回起動は、実機の既定（`Alt+Q`）と違うホットキーを書く。

- `snotra-core/src/hotkey.rs`: 分離プロファイルの既定ホットキーを定数で持つ（`ISOLATED_PROFILE_HOTKEY_MODIFIER = "Alt"`・`ISOLATED_PROFILE_HOTKEY_KEY = "F12"`）。`HotkeyConfig::isolated_profile_default()` を足す。値の選び方は下の「ホットキー（スクリプト）」と同じ理由
  - `hotkey.rs:30-32` の「既定ホットキーのリテラルは `default_hotkey_modifier` / `default_hotkey_key` の各 1 か所だけ」は偽になる。「実プロファイルの既定」と「分離プロファイルの既定」の 2 組がそれぞれ 1 か所、と書き直す（plan-review 2 回目の要対処 2）
  - 実測（2026-11-07・この機体）: `RegisterHotKey(NULL, id, MOD_ALT|MOD_NOREPEAT, VK_F12)` は成功（err 0）。修飾なしの `VK_F12` は err 1409（`ERROR_HOTKEY_ALREADY_REGISTERED`・デバッガ予約）で失敗した。Alt 付きならデバッガ予約に当たらない
- `snotra-core/src/config/io.rs` の `//!` と `load_from_dir_reporting` の rustdoc に「既定値は呼び出し側が組んで渡す（分離プロファイルではホットキーが違う）」を足す（plan-review 2 回目の要対処 2）
- `snotra-core/src/config/io.rs`: `load_reporting()` は `isolated_profile_key().is_some()` を見て、既定値の `Config` を 1 つ組む（分離プロファイルなら `hotkey` を `isolated_profile_default()` に差し替えたもの、そうでなければ `Self::default()`）。`load_from_dir_reporting(dir, defaults: Config)` へ渡し、**既定値を返す 4 つの腕（FirstRun の生成・保存、parse 失敗、非 UTF-8、その他の read 失敗）すべてで `Self::default()` の代わりにこれを使う**。腕ごとに分けないのは、同じ `match` の兄弟の腕で既定値が食い違うと、壊れた config の復旧時だけ実機と衝突するため（`snotra-core/CLAUDE.md`「読み込み失敗は種類で扱いを分ける」の兄弟分岐を揃える要求）
  - 判定核は env を読まない: `load_from_dir_reporting` は引数で既定値を受けるので、テストは env を書かずに分離プロファイルの既定を注入して測れる
  - `Loaded` の腕（ファイルが読めた）は変えない。書かれた値を尊重する
- 対象外（残余として SPEC に書く）: 既存の `config.toml` に `[hotkey]` が無いときの serde の既定（`default_hotkey_modifier` / `default_hotkey_key`）、移行の `fallback_invalid_hotkey`（不正なホットキーの自動修復）、設定アプリの「初期設定に戻す」（§7.3）とインポート（§13.3）は、分離プロファイルでも `Alt+Q` に戻す。これらは「書かれた値が無い・壊れている・利用者が既定へ戻した」場合で、初回起動ではない
  - `fallback_invalid_hotkey`（`migrate.rs:200-213`）を兄弟の腕と同じ扱いにしない理由（plan-review 2 回目の要対処 1 への裁定）: io.rs の 4 つの腕は「書かれた config を使えない」ときに丸ごと既定値で置き換える経路で、どれを通るかは利用者に見えない。`fallback_invalid_hotkey` は読めた config の中の不正な 1 値を直す移行で、SPEC §7.5 が「既定ホットキー（`Alt+Q`）へ自動修復」と規定している。分離の既定を渡すには `apply_migrations()` の署名（呼び出し元はインポートやテストを含む）を変えることになり、初回起動を分けるという裁定の範囲を超える。SPEC §13 に残余として明記する
  - 帰結（SPEC に書く）: 分離プロファイルでこれらを通ると `Alt+Q` が保存され、`config_watcher` が再登録を試み、実機が `Alt+Q` を握っていれば既存の登録失敗通知が出る（登録は旧キーのまま維持される・§7.5）。設定アプリの「初期設定に戻す」（`snotra-settings/src/app.rs:255`）も同じ
- seed（`New-SnotraVerificationProfile`）の既定も同じ `Alt` + `F12` にする（下）。Rust の定数と PowerShell の既定は別の言語にあるため写しになる。正本は Rust の定数とし、seed の既定の隣のコメントでそれを指す

### ホットキー（スクリプト）

- `scripts/lib/SnotraSmoke.psm1` の `New-SnotraVerificationProfile`: 既定を `HotkeyModifier = 'Alt'`（不変）・`HotkeyKey = 'F12'` へ
  - 修飾を `Alt` 単独のまま保つ理由: smoke-egui の Alt 解放の扱い（`ShowAfterAltRelease`）と「押しっぱなしの Z」検査（`smoke-egui.ps1:540-600`）が、実機の既定と同じ修飾の形で通り続けるため。`Ctrl+Alt` は show 時に Ctrl が押下中に残りうるうえ、AltGr として扱う配列がある（plan-review U1）
  - `Alt+Shift` を選ばない理由: Windows の入力言語切り替えの既定の組み合わせで、注入すると IME/レイアウトが切り替わりうる
  - SPEC §7.4 のブロック表・Win 修飾に当たらない。`F12` を素で登録できないのはデバッガ予約の修飾なしの場合だけである
- `scripts/occupy-hotkey.ps1`: キーの写像を「1 文字の英数字 → ASCII 大文字」に加えて `F1`〜`F12` → `0x70 + (n-1)` へ広げる。修飾が `Alt` 以外・写せないキーは従来どおり throw。表示文言のリテラル `Alt+Q` / `VK_Q` を変数へ
- `scripts/smoke-egui.ps1`: `-HotkeyVks` の既定 `"18,81"` を `"18,123"` へ。コメントの `Alt+Q` を直す（117・329 行）。`smoke-startup.ps1:115`・`visual-check-colors.ps1:231` のコメント/表示も同様
- `scripts/measure-memory-stages.ps1:45` の例示は実プロファイルを使うスクリプトなので、そのまま（`Alt+Q` は実機の既定として正しい）

### スクリプトのプロセス特定

- `Resolve-SnotraExistingProcess` に `-BuildRoot [string]` を足す。指定されたら、`Get-Process -Name` の結果を `.Path` がそのディレクトリの配下にある（大文字小文字を無視した前方一致・区切りを付けて比べる）ものに絞る。未指定なら従来どおり名前の全部
  - 絞る理由: 分離後に検証の邪魔になるのは「同じ分離プロファイルの残留プロセス」だけで、それはこのリポジトリのビルド成果物から起動されている。実機（インストール先の exe）は無関係
  - exe 1 本の一致にしない理由: プロファイルのパスは固定だが、exe は debug / release・`-ExePath` で変わる。別ビルドの残留が絞り込みから漏れると、single-instance で後発が黙って終了する（#872 と同型・plan-review M4）
  - `BuildRoot` は `cargo metadata` の `target_directory`（`CARGO_TARGET_DIR` を尊重する）。既存の `Resolve-SnotraCargoExecutable`（`SnotraSmoke.psm1`）が内部でこれを導いている（`$metadata.target_directory`）ので、その部分を `Resolve-SnotraCargoTargetDirectory` として括り出し、`Resolve-SnotraCargoExecutable` はそれを呼ぶ形にする（導出を 2 か所に書かない）
  - `.Path` が `$null`（権限で読めないプロセス）は、絞り込みで落とさず候補に含め、警告を出す。黙って外すと、読めない残留が single-instance で後発を沈黙させる（#872 と同型）
- 呼び出し: `smoke-egui.ps1:151`・`smoke-startup.ps1:64`・`visual-check-colors.ps1:92` は `-BuildRoot`。`bench-startup.ps1:117` は `-UseVerificationProfile` のときだけ渡す（実 config で測る既定の経路は、実機と同じ単一インスタンスなので、従来どおり名前で落とす）
- `-ExePath` にリポジトリ外の exe を渡した場合は、残留が絞り込みから漏れうる。残余として関数の doc に書く
- `Wait-SnotraWindow`: `-Process` が渡されたら、`FindWindowExW(0, $prev, null, $Title)` で同じタイトルの窓を順に辿り、owner が一致する窓を探す（最初の 1 窓で止まらない）。interop に `FindWindowExW` を足す
- Pester（`scripts/lib/SnotraSmoke.Tests.ps1`）: 既存の `Describe 'Resolve-SnotraExistingProcess'`（564 行〜）と同じく `Mock -ModuleName SnotraSmoke Get-Process` へ `Path` を持つ偽物を返させ、プロセスを起動せずに書く。固定するもの: `-BuildRoot` の配下は対象・配下でない（実機のインストール先）は対象外・区切りの無い兄弟ディレクトリ（`target2`）は対象外・`Path` が `$null` は対象で警告・未指定は従来どおり全部。`Describe 'New-SnotraVerificationProfile'`（67 行〜）に seed の既定ホットキーが `Alt` + `F12` であることを足す

### 対象外（残余として文書に書く）

- 単一インスタンス判定より前に config / index / history の読み込みと保存が走る（research A4）。同じプロファイルの 2 回目の起動は今と同じく、終了の前に保存しうる
- 分離プロファイルでも `Alt+Q` へ戻る経路（`[hotkey]` の欠落・不正なホットキーの自動修復・「初期設定に戻す」・インポート）。上の「ホットキー（製品・分離プロファイルの既定値）」の対象外
- 実機のホットキーを利用者が `Alt+F12` に変えていると、分離プロファイルの初回起動と衝突する（既存の登録失敗通知が出る）
- 分離プロファイルの Snotra から起動したアプリは `SNOTRA_CONFIG_DIR` を継承する（既存の性質・`launch.rs` は env を消さない）
- 同じディレクトリをジャンクション・シンボリックリンク・8.3 名・`\\?\` 接頭辞付きで指すと、鍵が別になる（plan-review M6）
- 分離プロファイルでも自動更新は止めない。手動起動で `config.toml` が無いと `auto_update` の既定（`Full`）で実リリースを確かめにいき、トーストから更新するとインストール先（実機）を上書きしうる。更新はプロファイルではなくインストールに対する操作であり、seed は `auto_update = "disabled"` を入れる（plan-review M3）。`docs/build-commands.md` の env ハッチ節に書く
- 実プロファイルを使うスクリプト（`measure-memory-stages.ps1` / `manual-smoke.ps1` / `measure-memory.ps1` / `bench-startup.ps1` 既定）は、実データの規模で測る意図で実プロファイルを使っている。プロファイルの選び方は変えない

## 変更するファイル

| ファイル | シンボル |
|---|---|
| `snotra-core/src/config/location.rs` | `Config::isolated_profile_key`・`isolated_profile_key_from`・`profile_identity`・`fnv1a64`・上書き有効の述語 |
| `snotra-core/src/config/location/tests.rs` | 新テスト |
| `snotra-core/src/hotkey.rs` | 分離プロファイルの既定ホットキーの定数・`HotkeyConfig::isolated_profile_default` |
| `snotra-core/src/config/io.rs` / `config/io/tests.rs` | `load_reporting`・`load_from_dir_reporting` の既定値の引数 |
| `snotra-core/src/autostart.rs` / `autostart/tests.rs` | `AutostartError::IsolatedProfile`・`enable`・`disable` |
| `snotra-core/src/search/tests/common.rs`・`search/tests/path.rs` | `read_real_config_read_only`・呼び出し 3 か所 |
| `src-tauri/src/main.rs` | identifier の差し替え |
| `src-tauri/src/state.rs`・`src-tauri/src/commands/system.rs` | テスト fixture |
| `snotra-settings/src/app.rs`・`tabs/general.rs`・`i18n.rs` | 分離プロファイルの表示・`TrKey` |
| `scripts/lib/SnotraSmoke.psm1`・`SnotraSmoke.Tests.ps1` | seed 既定・`Resolve-SnotraExistingProcess`・`Wait-SnotraWindow`・interop |
| `scripts/occupy-hotkey.ps1`・`smoke-egui.ps1`・`smoke-startup.ps1`・`visual-check-colors.ps1`・`bench-startup.ps1` | 上記 |
| `SPEC.md` | §13 の「単一インスタンス制御には影響しない」を書き換え・§7.7 に分離プロファイルの扱い |
| `docs/build-commands.md` | 「別プロファイルで起動するための env ハッチ」節・smoke の前提 |

変更しない散文（`prose:sweep` の ★未掲載を判定した結果・語 11 / 287 行）:

- `README.md` / `README.en.md` / `docs/architecture.md` / `snotra-settings/CLAUDE.md` / `SPEC.md:420` の `Alt+Q`: 実機の既定として正しい。検証用 seed の既定ではない
- `CONTRIBUTING.md:92`・`docs/build-commands.md:304`: 注入キーを trace から読む説明で、seed の既定に依らない
- `PERFORMANCE.md` の `SNOTRA_CONFIG_DIR`: 過去の測定条件の記録
- `.github/workflows/smoke.yml:104`: 両 smoke が自分のプロファイルを持つという記述で、変更後も真
- `src-tauri/src/commands/window.rs:26`（設定アプリが env を継承する）・`src-tauri/src/icon.rs:206`: 変更後も真
- `scripts/visual-input-metrics.ps1`: `Wait-SnotraWindow` の呼び出しだけで、ヘルパの変更で直る。スクリプト本体は触らない
- `docs/adr/`・`docs/superpowers/`・`docs/design/`: 凍結された歴史
- `snotra-core/tests/` の `HistoryStore::load()`: `#[ignore]` 計測コードで許されている

変更する散文（上の表に加えて）:

- `scripts/visual-check-colors.ps1:91` のコメント（「単一インスタンス衝突は沈黙する」→ 同じ exe の残留だけを拒む理由へ）と `:92` の呼び出し・`:231` の表示
- `scripts/bench-startup.ps1:114` のコメント（実 config の既定経路では名前で落とす理由）
- `docs/build-commands.md:187`（「単一インスタンスの制御には影響しない」）
- `SPEC.md:715`（同上）

`.rs` ファイルの追加は無い（`//!` と各 `CLAUDE.md` のモジュール構成は変わらない）。

## 新しい用語

- 「分離プロファイル」: `SNOTRA_CONFIG_DIR` が実 `%APPDATA%\Snotra` 以外を指している起動。判定 (A)（標準の語で言える: 「プロファイル」+「分離」は issue の語そのもの）。初出の SPEC §13 で定義を 1 文添える。既存の「検証用プロファイル」（スクリプトが作るもの）は分離プロファイルの一種として使い分ける。複数ファイルに書くので `docs/glossary.md` へ足す
- 「プロファイル鍵」: コード内の識別子 `isolated_profile_key` の説明にのみ使い、散文の文書には出さない（文書では「分離プロファイルごとの識別子」と書く）。glossary には足さない

## 実装の順序

### Phase 1 — core
- [x] `location.rs`: 上書き有効の述語を共通化し、`isolated_profile_key_from` / `profile_identity` / `fnv1a64` / `isolated_profile_key` を足す（Red → Green）
- [x] `location/tests.rs`: 未設定・空文字 → None／実プロファイルの別綴り（大文字・`/`・末尾 `\`）→ None／別ディレクトリ → Some／同じディレクトリの別綴り（相対と絶対・大文字小文字・`/`・末尾）→ 同じ鍵／違うディレクトリ → 違う鍵／鍵が `[0-9a-f]{16}`／`base` が None でも上書きがあれば Some／結線の pin（env を読むだけ）
- [x] `autostart.rs`: `IsolatedProfile` と `enable` / `disable` のガード。判定核をテストできる形にする（ガードの述語を引数に取る内側の fn に分け、テストは env を書かずに測る）
- [x] `search/tests/common.rs`・`path.rs`: `read_real_config_read_only` へ置き換え
- [x] `hotkey.rs`: 分離プロファイルの既定ホットキーの定数と `HotkeyConfig::isolated_profile_default()`。テスト: 値が意味 parser を通り、システムショートカットと Win 修飾に当たらず、`HotkeyConfig::default()` と違う
- [x] `config/io.rs`: `load_from_dir_reporting(dir, defaults)` と `load_reporting()` の既定値の組み立て。テスト（`config/io/tests.rs`・temp dir）: 分離の既定を渡したとき、FirstRun は保存された `config.toml` のホットキーが `Alt+F12`／parse 失敗・非 UTF-8 の腕も `Alt+F12` を返す／`Loaded` は書かれた値を返す／既定を渡さない（実プロファイル）ときは従来どおり `Alt+Q`

### Phase 2 — src-tauri
- [x] `main.rs` の identifier 差し替え
- [x] `state.rs` / `system.rs` の fixture を空の履歴へ（`HistoryStore::empty()` は core の `#[cfg(test)] pub(crate)` で src-tauri から見えないので、`snotra-core/tests/search_allocations.rs` と同じく存在しないパスの `load_in`）

### Phase 3 — settings
- [x] `run` / `SettingsApp::new` / `GeneralTabState` に分離プロファイルを渡す
- [x] `general.rs` の無効化と説明・`autostart_message` の腕
- [x] `i18n.rs` の `TrKey`（ja / en）。説明文とエラー文は同じ文言なので `NoteAutostartIsolatedProfile` の 1 キーにした
- [x] kittest で分離プロファイルのとき無効表示になることを固定（既存の `en_harness` の形）

### Phase 4 — scripts
- [ ] seed の既定ホットキー・`occupy-hotkey.ps1` の写像・`smoke-egui.ps1` の `-HotkeyVks` 既定とコメント
- [ ] `Resolve-SnotraCargoTargetDirectory` の括り出し・`Resolve-SnotraExistingProcess -BuildRoot` と 4 か所の呼び出し
- [ ] `Wait-SnotraWindow` の PID 探索と `FindWindowExW`
- [ ] Pester の追加

### Phase 5 — 文書
- [ ] SPEC §13（分離プロファイルの定義・単一インスタンス・既定ホットキー `Alt+F12` と、`Alt+Q` へ戻る経路の残余）・§7.7（分離プロファイルでは変更できない）。§7.5 の「既定ホットキー（`Alt+Q`）へ自動修復」は分離プロファイルでも `Alt+Q` のままなので文言は変えず、§13 から参照する
- [ ] `load_from_dir_reporting` の既存の呼び出し（`config/io/tests.rs` ほか）を新しい引数へ移す（コンパイルで漏れが出る）
- [ ] `docs/build-commands.md`（env ハッチ節・smoke のプロファイル節・残余）
- [ ] `docs/glossary.md` に「分離プロファイル」
- [ ] `location.rs` の `//!` に、保存先の導出に加えてプロファイル鍵の導出を持つことを足す（`src-tauri/src/startup.rs` の「二重起動は終端を出さない」は、同じプロファイルの 2 回目の起動について変更後も真なので触らない）
- [ ] `git grep` で「単一インスタンスの制御には影響しない」「同時起動はできない」「Alt+Q」の写しを数え、分離プロファイルに関する記述を直す

### Phase 6 — 検証
- [ ] `docs/build-commands.md`「変更後の検証チェックリスト」の該当カテゴリ（cargo test / clippy / fmt / Pester / governance:check）
- [ ] 実機スモーク A: 実機の Snotra を動かしたまま、`SNOTRA_CONFIG_DIR` を temp へ向けて `snotra.exe` を起動し、2 プロセスが並ぶこと・実機の PID が変わらないことを測る
- [ ] 実機スモーク B: 同じ temp を別の綴り（`/` 区切り・大文字）で 2 回目に起動し、1 回目へ転送されて終わること（プロセス数が増えない）
- [ ] 実機スモーク A2: 実機（`Alt+Q`）を動かしたまま、空の `SNOTRA_CONFIG_DIR` で起動し、生成された `config.toml` のホットキーが `Alt+F12` であること・登録失敗の通知が出ないこと
- [ ] 実機スモーク C: 分離プロファイルの設定アプリでスタートアップのチェックボックスが無効であること。前後で `HKCU\...\Run\Snotra` の値が変わらないこと
- [ ] 実機スモーク D: 実機を動かしたまま `scripts/smoke-egui.ps1` と `scripts/smoke-startup.ps1` を回し、緑であること・実機の PID が変わらないこと
- [ ] cargo テストの前後で実 `%APPDATA%\Snotra` の mtime と Run の値が変わらないこと。加えて空のディレクトリを `SNOTRA_CONFIG_DIR` にして既定の `cargo test --workspace` を回し、そのディレクトリに何も作られないこと（research A3 の再現手順で、変更前に 1 本作られることを先に確かめる）

変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う。候補: 実プロファイルの比較を外す／`absolute` を外す／`to_lowercase` を外す／autostart のガードを外す／identifier の差し替えを外す／`-BuildRoot` の絞り込みを外す。

## SPEC・文書の更新

要（Phase 5）。SPEC §13 は「単一インスタンス制御には影響しない」を偽にする変更なので仕様変更である。

## 未確定（実装前に潰す）

（なし）

## セルフレビュー

- リスク: 高（公開 API `Config::isolated_profile_key`・`AutostartError` の variant 追加、モジュール間のインターフェース、CI が走らせる smoke スクリプト）
- plan-review: 独立レビュー1体（Step 2・範囲と選び方がユーザー裁定のため）＋ 5c の裁定差分の再レビュー1体
- エージェント数: 2（3b を含めると 3）
- 要対処: 0 件（`workspace/plan-review-profile-isolation.md`）。軽微のうち 3 件を計画へ反映: M4（残留の絞り込みを exe 1 本からビルド成果物の配下へ）・M3（自動更新を残余へ）・M6（`\\?\` を残余へ）。未検証 U1 を受けて seed のホットキーを `Ctrl+Alt+Q` 案から `Alt+F12` へ変更（修飾の形を実機の既定と揃える）。M1・M2・M5・M7 は計画の裏付けで変更なし
- 未検証: U2（src-tauri のテストが `RegisterHotKey` を撃つか）は主が確かめた: `register_prepared`（`platform/hotkey.rs:160`）の呼び出しは製品経路の 2 か所（176・203 行）だけ。U3 は Phase 6 の空ディレクトリでの `cargo test --workspace` が検出器。U4 は対象外（research A4）。mutex 名前空間の実測は Phase 6 の実機スモーク A・B が兼ねる
- 散文の全件列挙: 語 12 / 総件数 313 行 / ★未掲載 55（最終の計画で再実行。足した語 `Resolve-SnotraCargoExecutable` の ★ 3 件は ADR（凍結）と、署名を変えない呼び出し元 `manual-smoke.ps1`・`run-pester.ps1` で、変更不要。計画へ足した 4: `visual-check-colors.ps1:91,92,231`・`bench-startup.ps1:114`・`build-commands.md:187`・`SPEC.md:715`。残りは上の「変更しない散文」で判定）
- 5c の再レビュー（初回のホットキーを分ける裁定の差分・`workspace/plan-review-profile-isolation-2.md`）: 要対処 2。(1) `fallback_invalid_hotkey` は計画どおり対象外とし、理由と帰結を計画と SPEC の作業へ足した（降格ではなく裁定。根拠は上の節）。(2) `hotkey.rs:30-32` と `io.rs` の doc を計画へ足した。未検証の Alt+F12 の登録は主が実測して成功（上の節）。seed が `[hotkey]` を必ず出すことは `SnotraSmoke.psm1:253-255` で確かめた
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）:
  - 方式 Step 2 / 設計変更 1+⚠1（M4・U1） / 主の見落とし 1（M3）
  - 方式 Step 2 / 設計変更 0 / 主の見落とし 2（`fallback_invalid_hotkey` の理由づけ・`hotkey.rs:30-32` の doc）

## 人間レビュー

- [x] 承認済み — 2026-11-07 / 問い: "初回のホットキーも分けた #1306 の計画（workspace/plan.md）を承認されますか？ 分離プロファイルが既定値を使うとき（初回起動・壊れた config の復旧・読めないとき）は Alt+F12 にします（この機体で登録できることを実測済み）。一方、書かれたホットキーが不正なときの自動修復・「初期設定に戻す」・インポート・[hotkey] の欠落では、分離プロファイルでも Alt+Q に戻ります（実機が動いていれば登録失敗の通知が出ます）。これらは初回起動ではなく、分けるには移行の署名まで変えることになるため、残る問題として SPEC に書きます。自動起動は分離プロファイルでは無効のままです。" / 回答: "承認する"
