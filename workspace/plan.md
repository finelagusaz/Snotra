# 実装計画 — #1210 スタートアップに登録・削除できるようにする

調査は `workspace/research.md`、敵対的調査の全文は `workspace/adversarial-1210.txt`。

## 目的

設定アプリ（`snotra-settings`）の [全般] タブに、本体 `snotra.exe` を Windows のログオン時自動起動へ登録・解除するチェックボックスを 1 つ置く。

## 受け入れ条件

1. [全般] タブにチェックボックスが在り、**設定アプリを開いた時点の実 OS 状態**を反映している
2. チェックを入れると `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` に値名 `Snotra` が作られ、値は**引用符で括った `snotra.exe` の絶対パス**である
3. チェックを外すとその値が消える。**値が既に無くてもエラーにならない**（冪等）
4. 既に別のパスで登録されているとき、チェックを入れ直すと**現在のパスで上書きされる**
5. 操作の成否がその場でインライン表示される。**Save ボタンを押す必要が無い**
6. この設定は `config.toml` に一切書かれない。バックアップの export/import と「初期設定に戻す」の影響を受けない
7. 昇格（管理者権限）を要求しない

## 設計判断（research.md §4 の要約・実装者は再導出しない）

| 判断 | 内容 | 却下した代替 |
|---|---|---|
| 状態の所有 | **レジストリが SSOT。`Config` のフィールドにしない** | `Config` への追加（backup が機体ローカル状態を持ち運ぶ・reset が登録を解除する・乖離を調停できない） |
| 適用タイミング | **即時適用**（`backup.rs` の先例） | Save 適用（`has_changes` / `SECTION_TABLE` / kittest 不変条件へ非 `Config` の dirty 源を配線する費用） |
| 機構 | `HKCU\...\Run` の REG_SZ 値 | Startup フォルダの `.lnk`（COM が要る）・`RunOnce`（1 回で消える）・タスクスケジューラ（機構が重い）・`HKLM`（昇格が要る）・`tauri-plugin-autostart`（本体所有になり IPC が要る） |
| Win32 API | `RegOpenKeyExW(KEY_WRITE)` + `RegSetValueExW` / `RegDeleteValueW` | **`RegCreateKeyExW` は使わない**——`Win32_Security` feature を要求する（実測）。`Run` は well-known key ゆえ作成不要 |
| 置き場 | `snotra-core`（`windows` crate と Registry feature を既に持つ） | `snotra-settings`（feature 追加と `RegKeyGuard` の重複が要る） |

## 変更ファイル一覧と対象シンボル

### 新規

| ファイル | 内容 |
|---|---|
| `snotra-core/src/win_registry.rs` | `pub(crate)` の Win32 レジストリ薄層。`RegKeyGuard`（`path_env.rs` から移設）+ `open_hkcu(subkey, access) -> Option<RegKeyGuard>` |
| `snotra-core/src/autostart.rs` | ログオン時自動起動の登録・解除・状態取得 |
| `snotra-core/src/autostart/tests.rs` | 純粋部の unit test（`#[cfg(test)] mod tests;` 参照） |

### 変更

| ファイル | 対象シンボル / 節 |
|---|---|
| `snotra-core/src/lib.rs` | `pub mod autostart;` と `mod win_registry;` の宣言を追加 |
| `snotra-core/src/indexer/path_env.rs` | `RegKeyGuard` の定義と `impl Drop` を削除し、`crate::win_registry` の `open_hkcu` を使う形へ書き換え（`read_user_path` の中身） |
| `snotra-settings/src/tabs/general.rs` | `GeneralTabState` を新設、`ui()` の引数に `&mut GeneralTabState` を追加、スタートアップ節を追加 |
| `snotra-settings/src/app.rs` | `SettingsApp` に `general_state: GeneralTabState`／`SettingsApp::new` に `autostart_enabled: bool` 引数を追加／`run()` が `autostart::is_enabled()` を読んで渡す／`ui_impl` の `TabId::General` アームで受け渡し／`en_harness`（`#[cfg(test)]`）を新しい引数に合わせる |
| `snotra-settings/src/i18n.rs` | `TrKey` へ 6 variant 追加、`ja()` / `en()` に対応する行を追加 |
| `snotra-core/CLAUDE.md` | ①「モジュール構成」へ `autostart.rs` / `win_registry.rs` / `autostart/tests.rs` の索引行を追加。②「開発ルール」の「この crate は **Win32 非依存**の純ロジック層」の一句を実態へ合わせる（下記） |
| `snotra-settings/CLAUDE.md` | 「アーキテクチャ」の「本体との連携は `config.toml` ファイル1点のみ」の直後に、スタートアップ登録がその射程外である旨を 1 行 |
| `SPEC.md` | §7.2 に項目 1 行、**§7.7 を新設して正本を置く**、§7.3 / §7.5 / §13.3 に §7.7 への参照を 1 行ずつ（**§13.1 には書かない**——config キーを足さないため） |
| `docs/architecture.md` | 「設定管理」節（`:98` の draft/saved 二重状態モデルの行の直後）に 1 行。**`:98` / `:104` / `:41` / `:65` / `:95` はいずれも変更後も literally true である**——`config.toml` は Save 時にしか書かないし、本体との通信は増えない。足すのは「設定アプリには config.toml を経由しない即時適用の項目が 1 つ在り、正本は `SPEC.md` §7.7」という**欠けを埋める 1 行**であって、既存行の訂正ではない |

#### `snotra-core/CLAUDE.md`「開発ルール」の一句について

現行の文は「**UI 表示文字列を持たない**: この crate は Win32 非依存の純ロジック層。」だが、**この「Win32 非依存」は変更前から偽である**——`indexer/path_env.rs` が既に `RegOpenKeyExW` / `RegQueryValueExW` を呼び、`Cargo.toml` が `[target.'cfg(windows)'.dependencies]` で `windows` crate を持つ。今回 2 つ目の Win32 モジュールを足すので、ここで実態へ合わせる。

- **規範の中身（UI 表示文字列を持たない・エラーは型で返し文言は UI 層が組む）は変えない。** 今回の `AutostartError` はこの規範に従っており、文言は `snotra-settings/src/i18n.rs` が持つ
- 変えるのは「Win32 非依存」という**根拠の一句だけ**である。Win32 を呼ぶ面が既に在り、今回増えるという事実に合わせる
- **これはスコープの拡大ではない**——`AGENTS.md`「検証の作法」item 7（変更で偽になる散文を変更ファイル一覧に載せる）が要求する分であり、触るファイルは既に一覧に在る

### 触らない

- `src-tauri/**`（本体はこの値を読まない。OS が読む）
- `snotra-core/src/config/**`（新しい config キーを作らない）
- `scripts/smoke-*.ps1`・`.github/workflows/**`（本体の起動経路であり射程外）
- NSIS のアンインストール hook（下の「受容する残余」を見よ）

## 公開インターフェース（`snotra-core::autostart`）

```rust
/// `Run` に置く値名。
pub const RUN_VALUE_NAME: &str = "Snotra";

/// 登録する本体の実行ファイル名。
pub const MAIN_EXE_FILE_NAME: &str = "snotra.exe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartError {
    /// 本体の実行ファイルを導けなかった（`current_exe` 失敗・親ディレクトリ不在・不在）。
    MainExeNotFound,
    /// レジストリ操作が失敗した（Win32 のエラーコードを持つ）。
    Registry(u32),
}

/// 判定核（`current_exe` を読まないので並列テストから安全に測れる）。
/// `settings_exe` と同じディレクトリの `snotra.exe` を返す。
fn main_exe_from(settings_exe: &Path) -> Option<PathBuf>;

/// `snotra-settings.exe` の隣にある `snotra.exe` の絶対パス。
pub fn main_exe_path() -> Option<PathBuf>;

/// `Run` 値に書く文字列。**必ず引用符で括る**。
pub fn command_line_for(exe: &Path) -> String;

/// 値名 `Snotra` が `Run` に存在するか。読み取り失敗は `false` として扱う。
pub fn is_enabled() -> bool;

/// 登録する。既存値があっても現在のパスで上書きする。
pub fn enable() -> Result<(), AutostartError>;

/// 解除する。**値が存在しなくても `Ok(())`**（`ERROR_FILE_NOT_FOUND` を成功へ畳む）。
pub fn disable() -> Result<(), AutostartError>;
```

`#[cfg(not(windows))]` では `is_enabled()` が `false`、`enable()` / `disable()` が `Ok(())` を返すスタブを置く（`path_env.rs` の `read_user_path` と同じ形）。

## 実装順序

**新 API の導入と呼び出し点の移行は 1 タスクに束ねる**（`-D warnings` 下で未使用の新 API は `dead_code` で落ちる。`AGENTS.md` 条件別チェック表）。

### Phase 1 — `snotra-core`（レジストリ層）

- [x] `snotra-core/src/win_registry.rs` を新設し、`RegKeyGuard` を `path_env.rs` から移設する。`open_hkcu(subkey: PCWSTR, access: REG_SAM_FLAGS) -> Option<RegKeyGuard>` を置く
- [x] `path_env.rs` の `read_user_path` を `open_hkcu` 経由へ書き換え、`RegKeyGuard` の定義を削除する
- [x] `snotra-core/src/autostart.rs` を新設し、上のインターフェースを実装する。`//!` に責務・受容する残余（OS I/O 部は untested・その理由）を書く
- [x] `snotra-core/src/autostart/tests.rs` に純粋部のテストを書く（下の「テスト方針」）
- [x] `lib.rs` に `pub mod autostart;` / `mod win_registry;` を追加する
- [x] `cargo test -p snotra-core` と `cargo clippy --workspace --all-targets -- -D warnings` が緑

### Phase 2 — `snotra-settings`（UI）

- [x] `i18n.rs` に `TrKey` の 6 variant と `ja()` / `en()` の行を追加する（文言は下の表）
- [x] `general.rs` に `GeneralTabState`（`enabled: bool` / `message: String` / `message_is_error: bool`）を定義し、`ui()` の引数へ追加する
- [x] `general.rs` に「スタートアップ」節とチェックボックスを追加し、`.changed()` で `enable()` / `disable()` を呼び、**その直後に `is_enabled()` を読み直して `state.enabled` を更新する**（失敗時に UI が嘘をつかないため）
- [x] `app.rs` の `SettingsApp::new` に **`autostart_enabled: bool` 引数を足し**、`general_state` をその値で初期化する。**`new()` の中で `autostart::is_enabled()` を呼んではならない**（下記）
- [x] `app.rs` の `run()` が `autostart::is_enabled()` を読み、`SettingsApp::new` へ渡す。**実レジストリを読む地点は `run()` の 1 箇所だけにする**
- [x] 既存 kittest の `en_harness` は `SettingsApp::new(config, false, None, LoadOutcome::Loaded, /* autostart_enabled */ false)` と固定値で構築する
- [x] `cargo test -p snotra-settings` と clippy が緑

### Phase 3 — 文書

- [x] `SPEC.md` §7.7 を新設し、§7.2 / §7.3 / §7.5 / §13.3 に参照行を置く
- [x] `docs/architecture.md`「設定管理」に 1 行追加する
- [x] `snotra-core/CLAUDE.md` の索引に 3 行追加し、「Win32 非依存」の一句を直す
- [x] `snotra-settings/CLAUDE.md` に 1 行追加する
- [x] `npm run governance:check` が緑

### Phase 4 — 検証

- [x] カテゴリ A のコマンドをすべて実行する（+ カテゴリ F。委譲側が `45c3534` と `b0a48f0` で、主エージェントが `2c51905` で実行し全て exit 0）
- [x] `/symmetric-check` を実行する（登録/解除の対称ペア）
- [ ] カテゴリ D の目視を実施し、結果を PR 本文に残す（下の「目視項目」）

## 文言（i18n）

| TrKey | 日本語 | English |
|---|---|---|
| `HeadingStartup` | スタートアップ | Startup |
| `CbLaunchAtLogon` | Windows のサインイン時に Snotra を起動する | Start Snotra when you sign in to Windows |
| `StatusAutostartEnabled` | スタートアップに登録しました | Added to startup |
| `StatusAutostartDisabled` | スタートアップから削除しました | Removed from startup |
| `ErrAutostartExeNotFound` | snotra.exe が見つかりません | snotra.exe was not found |
| `ErrAutostartRegistry` | レジストリの操作に失敗しました（コード {code}） | Registry operation failed (code {code}) |

**既存の `CbShowOnStartup`（「起動時にウィンドウを表示」）とは直交する別概念である。** 文言でこれを取り違えさせない——新しい方は「Windows のサインイン時に」で始め、`show_on_startup` の方は現行文言のまま変えない。

## 不変条件と異常系

| 不変条件 | 守る手段 |
|---|---|
| `Run` 値は常に引用符で括られる | `command_line_for` の unit test（空白を含むパスを含む） |
| 本体は `snotra-settings.exe` の兄弟である | `main_exe_from` の unit test。**実配置は 3b が実機で確認済み**（`%LOCALAPPDATA%\Snotra\` に 3 本同居） |
| 解除は冪等 | `disable()` が `ERROR_FILE_NOT_FOUND` を `Ok` へ畳む。**検知器は置かない**（OS I/O 部ゆえ・下の残余） |
| この設定は `config.toml` に現れない | **構造的に表現不能**——`Config` に対応するフィールドが無いので serde が書きようがない |
| チェックボックスの表示が OS の実状態からずれない | 操作直後に `is_enabled()` を読み直す。**起動中の外部変更には追随しない**（下の残余） |

| 異常系 | 扱い |
|---|---|
| `current_exe()` 失敗・親不在・`snotra.exe` 不在 | `MainExeNotFound` → `ErrAutostartExeNotFound` を赤で表示し、チェック状態は元に戻す |
| `RegOpenKeyExW` / `RegSetValueExW` / `RegDeleteValueW` 失敗 | `Registry(code)` → `ErrAutostartRegistry` にコードを載せて表示し、`is_enabled()` の読み直しで真の状態へ戻す |
| 読み取り（`is_enabled`）失敗 | `false` を返す。**このとき UI は「未登録」と表示する**——登録済みなのに未登録と出る向きの誤りだが、逆（未登録を登録済みと出す）より害が小さい（ユーザーがチェックを入れれば上書きで治る） |

## テスト方針と検証コマンド

### unit test（`snotra-core/src/autostart/tests.rs`）

純粋部だけを測る。**OS I/O 部（`is_enabled` / `enable` / `disable`）はテストしない**——理由を `//!` に書く。

- `command_line_for` が引用符で括る
- `command_line_for` が空白を含むパスで壊れない
- `main_exe_from` が兄弟の `snotra.exe` を返す
- `main_exe_from` が親を持たないパスで `None` を返す

### 明示的に置かないもの

- **`snotra-settings` の kittest でこのチェックボックスをクリックさせない。** 即時適用ゆえ、開発者のローカル `cargo test` が dev ビルドのパスを実スタートアップへ登録してしまう。既存の `kittest_*` テストはこの節に触れない
- **`SettingsApp::new()` からレジストリを読ませない**（**書き込み経路だけでなく読み取り経路も塞ぐ**）。`en_harness` は `SettingsApp::new` を呼ぶので、`new()` の中で `is_enabled()` を叩くと**既存 kittest 4 本の初期状態が開発機のレジストリ内容に依存する**。これは `snotra-core/CLAUDE.md` が #963 で禁じた `HistoryStore::load()` を fixture に使う形と同型であり、**CI のランナーには Snotra が登録されていないので `false`、開発機では `true` になりえて、食い違いは CI では緑のまま開発機でだけ現れる**。ゆえに `run()` が読んで `new()` へ渡す
  - これは `ADR-no-test-only-injection-in-product-code` の禁じる注入点ではない——分岐も env 読みも増えず、`config` / `first_run` / `load_outcome` を渡しているのと同じ**引数**である

### 検証コマンド（`docs/build-commands.md` が SSOT）

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo test -p snotra-settings
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
```

カテゴリ C（`npm test` / `smoke:*`）は**該当しない**——ウィンドウ生成・ホットキー・スラッシュコマンドのいずれにも触れない。

### 目視項目（カテゴリ D・`docs/build-commands.md`「エージェントが目視項目を自分で実施するとき」）

`scripts/manual-smoke.ps1` の項目は本体の trace 不変条件に紐づいており、設定アプリ単独のこの機能は載らない。以下をアドホックに実施し結果を PR 本文へ残す。

**実施者の列を持つ。** `docs/build-commands.md` は「`smoke:manual` は人間へ依頼する」を既に確立しており、**エージェントに実施できない項目が 2 つある**。

| # | 項目 | 実施者 | 結果 |
|---|---|---|---|
| 1 | 設定アプリを開き、チェックボックスが**未チェック**であること | エージェント | **PASS** — 起動直後の `Run` 値 ABSENT。起動しただけでは何も書かない |
| 2 | チェックを入れる → `HKCU\...\Run` に `Snotra` が現れ、値が引用符付きの `snotra.exe` 絶対パスであること | エージェント | **PASS** — `"C:\workspace\Snotra\target\debug\snotra.exe"` / kind=`String`（REG_SZ）。両端が引用符・絶対パス・`snotra.exe` を指す。**dev ビルドの本体を指したことが兄弟導出の実証**でもある |
| 3 | チェックを外す → 値が消えること | エージェント | **PASS** — ABSENT |
| 4 | 値が無い状態で外す操作をしてもエラーが出ないこと（冪等） | エージェント | **PASS** — 再登録 → 外部から `Remove-ItemProperty` → **開いたままの**アプリでチェックを外す → エラーにならず ABSENT のまま |
| 5 | **サインアウト → サインインで実際に本体が起動すること**（この経路は誰もまだ測っていない） | **人間**——実行中のセッションを落とすため | 未実施 |
| 6 | タスクマネージャーの「スタートアップ アプリ」に `Snotra` が現れること、および**そこで無効化したときにチェックボックスがどう見えるか**（残余 1 の実測を兼ねる） | **人間**——UI 操作の自動化手段が無い | 未実施 |

**1〜4 の実施方法**（`docs/build-commands.md`「エージェントが目視項目を自分で実施するとき」）: `cargo build -p snotra -p snotra-settings` → `SNOTRA_CONFIG_DIR` を検証用プロファイルへ向けて設定アプリを起動 → `SetForegroundWindow` + `SendKeys` で Tab を 1 つずつ増やしながら Space を打ち、**レジストリが変化した回数で当たりを特定**（Tab×6 が当該チェックボックス。外れたときは何も起きないので誤検知しない）→ 各段で `HKCU\...\Run` を実測。**終了状態は ABSENT へ戻した**（実測で確認）。

**副産物**: この実施が `/symmetric-check` 2c で挙げた死角（`KEY_READ` / `KEY_WRITE` の取り違えに検知手段が無い）を実際に潰している——取り違えていれば 1〜4 のどれかが必ず落ちる。

**目視 4 の再現手順**（「開き直す」では作れない——開き直すとチェックボックスは未チェックで起動し、「外す」操作そのものが存在しなくなる）: チェックを入れる → **設定アプリを開いたまま**外部から `reg delete` で値を消す → アプリのチェックを外す → `ERROR_FILE_NOT_FOUND` が `Ok` へ畳まれて成功表示になる。**受容する残余 2（起動中の外部変更に追随しない）がこの手順を可能にしている。**

**人間へ渡すときの順序**（目視 5 は**値が在る状態**で行う必要がある。エージェントが 1〜4 を終えた時点で値は消えている）: チェックを入れる → 6（タスクマネージャー）→ 5（サインアウト/サインイン）→ 起動を確認 → 原状復帰は任意。

**`/implement` への引き渡しでは、5 と 6 が人間依頼であることを明示する。** 実施の有無が会話にしか残らないと「検証されていない」と「問題が無かった」が区別できなくなるため、結果は PR 本文へ残す。

## 受容する残余（宣言して止める）

1. **タスクマネージャーで無効化されると、チェックボックスは「登録済み」のまま実際には起動しない。** Windows は `Run` 値を消さずに `Explorer\StartupApproved\Run` へ無効マークを置く機構を持ち、この実装はそちらを読まない。**チェックボックスの意味論は「`Run` 値が存在する」であって「実効的に有効」ではない。** 追随させるには承認バイト列の未文書な形式に依存することになるため採らない（目視 6 でどう見えるかだけ観測する）
2. **設定アプリの起動中に外部からレジストリを変えても追随しない。** 読むのは起動時と操作直後だけである（毎フレームのレジストリ読みを避けるため）
3. **アンインストールしても `Run` 値が残る。** NSIS のアンインストール hook（`bundle.windows.nsis.installerHooks`）で塞げるが、この issue の要求（設定アプリのチェックボックス）の外側であり、**別 issue へ切り出した（#1211）**。残っても実害はログオン時に存在しない exe を起動しようとして無視されるだけである
4. **ポータブル版のフォルダを移動すると、登録済みの絶対パスが死ぬ。** チェックを入れ直せば現在のパスで上書きされて直るが（受け入れ条件 4）、**利用者にはチェックが入ったままなので気づく契機が無い**。本体側に自己修復させる案は「レジストリが SSOT・本体は読まない」という設計判断と衝突するため採らない。SPEC §7.7 に明記して止める
5. **`enable()` / `disable()` / `is_enabled()` には検知器が無い。** `ADR-no-test-only-injection-in-product-code.md` に従い、測定のためだけの注入点を製品コードへ足さない。守っているのは純粋部（引用符付け・パス導出）と目視だけである

## 未確定（実装前に潰す）

- [x] **`RegCreateKeyExW` が使えるか** — 使えない（`#[cfg(feature = "Win32_Security")]`。windows-0.62.2 のソースを直接読んで実測）。**`RegOpenKeyExW(KEY_WRITE)` を使う設計にして回避した**（`Run` は well-known key ゆえ作成不要）。`RegSetValueExW` / `RegDeleteValueW` / `RegOpenKeyExW` / `RegQueryValueExW` はゲート無し
- [x] **チェックボックスの意味論を「登録されている」に置くか「実効的に有効」に置くか** — **「`Run` 値が存在する」に置く。** 「実効的に有効」にするには `StartupApproved` の承認バイト列（未文書）に依存する必要があり、この機体には無効化された標本が無く形式を実測できない。**測れないものを判定の根拠にしない**——採らなかった側は「受容する残余 1」として宣言した
- [x] **NSIS アンインストーラが `Run` 値を残すか** — 残る（アンインストーラは自分が作っていない HKCU 値を消さない）。**この issue のスコープ外とし、別 issue へ切り出した（#1211）**（受容する残余 3）。実インストールに `uninstall.exe` が在ることは実測で確認した
- [x] **`RegKeyGuard` を共有するか重複させるか** — **共有する。** `RegCloseKey` を呼ぶだけの型であり「片方だけが変わる将来」を挙げられない（`AGENTS.md`「検証の作法」の判定）。`snotra-core/src/win_registry.rs` へ移設し、`path_env.rs` と `autostart.rs` の両方が使う
- [x] **`StartupApproved` のレコード不在がタイミング依存か** — **実装に影響しない。** この実装は `StartupApproved` を読みも書きもしない。影響するのは残余 1 の文言だけであり、そこでは「Windows がこの機構を持つ」としか主張していない（レコードがいつ作られるかには依存しない）
- [x] **カテゴリ C（smoke）が該当するか** — 該当しない。`docs/build-commands.md` のカテゴリ C は「ウィンドウ生成／表示順・ホットキー・スラッシュコマンド」であり、いずれにも触れない。`smoke.yml` の paths（`src-tauri/**`・`**/Cargo.toml`・lockfile 等）にも当たらない見込みだが、**`Cargo.toml` を触らない**ことがその前提である（feature 追加 0 の設計ゆえ触らない）

## 条件別チェック（`AGENTS.md` の表）の当たり判定

| トリガー行 | 当たるか | 実行するもの |
|---|---|---|
| 対称ペア（生成/破棄）を変更 | **当たる** | `/symmetric-check`（Phase 4） |
| ファイル（`.rs`）を追加/削除 | **当たる** | 索引行（`snotra-core/CLAUDE.md`）と `mod` 宣言（`lib.rs`）を**別々の機構が見る**。索引漏れは編集直後に reminder が鳴るが、**`mod` 忘れは `governance:check` だけが赤にする**（cargo も rust-analyzer も報せない・#1085）。最悪の帰結は `#[cfg(test)] mod tests` が一度もコンパイルされず**テストが黙って走らない**こと。**「索引の reminder が鳴らなかったこと」を「`mod` も足りている」と読まない** |
| 関数・型を新規定義／導入 | **当たる** | 呼び出し元は LSP の findReferences で列挙 + `/dry-check`。**新 API の導入と呼び出し点の移行を 1 タスクに束ねる**（Phase 1・2 の分割は crate 境界であって API と呼び出し点の分割ではない——Phase 1 の時点で `autostart` は `pub` なので `dead_code` にならない） |
| ガバナンス文書（`*.md`・モジュール索引）を変更 | **当たる** | `npm run governance:check`（Phase 3） |
| 永続形式・識別子/キー形式を変更 | **当たる（判定して手で潰す）** | 下記 |
| UI モード・ガード条件を追加/変更 | **当たらない** | 新しいモードも遷移も増えない（チェックボックス 1 つ・状態はレジストリ） |
| worker/channel/listener/共有状態/async | **当たらない** | スレッドを跨がない。`ui()` の同期呼び出しだけ |
| 網羅性が要件 | **当たらない** | |
| 件数 N・上限パラメータ・導出の入力を変更 | **当たらない** | |
| 機能削除・trace イベント名・hotkey 登録・表示経路 | **当たらない** | |

### 「永続形式・識別子/キー形式」トリガーの扱い

`Run` の値名 `"Snotra"` は**新しい永続識別子**なので、このトリガーには当たる。ただし `/persistence-check` は自身の description で射程を「シリアライズ・on-disk 形式（index.bin / config.toml / history / window.bin 等）」と宣言しており、その 5 つの典型パターン（version 未バンプ・セマンティクス変更・往復のみの後方互換テスト・デコード失敗時の上書き・`match` 兄弟分岐の不揃い）は**どれも当たらない**——version フィールドもデコードも既存データも無い、schema を持たない単一の文字列である。**スキルを起動する代わりに、その 4 点セットを手で潰す**:

| 問い | 答え |
|---|---|
| 新規記録の形式 | REG_SZ 1 本。値は「引用符で括った絶対パス」のみ。引数を付けない（付けると将来の意味変更が旧エントリと食い違う） |
| 既存記録の移行 | **不要**——この値名は本リポジトリで初出であり、読むべき旧形式が存在しない |
| 外部から参照される API | Windows のログオン処理だけが読む。**値名 `"Snotra"` を後から変えると既存エントリが孤児になる**ので、`SPEC.md` §7.7 で凍結する |
| 壊れた値を読んだとき | `is_enabled()` は**値の中身を解釈しない**（存在だけを見る）ため、壊れようがない。中身が古いパスでも「登録済み」と表示し、チェックし直せば上書きで治る（受け入れ条件 4） |

## セルフレビュー

- リスク: **高**（`snotra-core` に新しい公開モジュールを足し `snotra-settings` から消費する＝複数モジュール間のインターフェースの新設。加えて `SPEC.md` / `CLAUDE.md` というガバナンス文書を変更する）
- plan-review: 独立レビュー 1 体（Step 2b・独立導出）
- エージェント数: 2（3b の敵対枠 1 + plan-review 1）
- 要対処: 3 件（すべて計画へ反映済み・下記）
- 未検証: 2 件（下記）

### plan-review 結果（Step 2b・独立導出 1 体）

独立導出は `workspace/plan-review-1210-autostart.md`。**中核 3 判断は独立に一致した**——レジストリが正本・`config.toml` にキーを足さない・Save 流に載せない・`HKCU\...\Run`。向こうは `app.rs` の該当行を逐語で読んで「`SECTION_TABLE` の型が `fn(&Config,&Config)->bool` なので `Config` 外の値は**そもそも表現できない**」まで到達しており、こちらの「構造的に衝突しない」という主張を独立に裏づけた。

#### 導出 ∖ 計画（漏れ候補）→ 要対処

| 項目 | 再照合した根拠 | 反映 |
|---|---|---|
| `docs/architecture.md` が変更ファイル一覧に無い | `:98`「Save 時にのみ `config.toml` に書き込み」・`:104` の永続先の列挙を自分で読んだ。**どちらも変更後も literally true**（config.toml は書かない／`%APPDATA%` 配下も増えない）——**向こうの「射程補足が要る」は結論としては採るが、「偽になる」という機序は採らない**。真の問題は「設定管理の節が即時適用の項目を 1 つも持たない」という**欠け**である | 変更ファイル一覧へ追加。足すのは 1 行、既存行は直さない |
| SPEC §7.5（設定反映タイミング）が抜けていた | §7.5 は設定ごとに反映経路を列挙する節であり、config_watcher を経由しない項目が黙って抜けると**列挙の穴**になる | Phase 3 と一覧へ追加。§13.1 は逆に**外した**（config キーを足さないため書くことが無い） |
| `/persistence-check` トリガー（`Run` 値名 = 新しい永続識別子） | `AGENTS.md` の当該行を読み直し、当たると判定した | 上の節で 4 点セットを手で潰した |

#### 計画 ∖ 導出（スコープ過剰候補）→ 裁定

- **`win_registry.rs` の新設と `path_env.rs` の改修**（向こうは `autostart.rs` 内に `RegKeyGuard` 相当を置く案で、共有か重複かは D6 として未決のまま渡してきた）。**共有を維持する。** 判定は `AGENTS.md`「検証の作法」の「片方だけが変わる将来を 1 つ挙げられるか」——`RegCloseKey` を呼ぶだけの型に、それは挙げられない。重複させれば `/dry-check` と `code-reviewer` が毎回挙げ、採否の費用が乗り続ける。**ただしこれは計画が issue の外へ 1 歩出る唯一の箇所であり、人間の裁定に載せる**（下の「人間レビュー」の問い）

#### 判断の不一致 → 裁定

- **SPEC の書き方**: 向こうは「§7.2 + §7.5 + §13.3 の 3 箇所へ箇条書きを足し、新節を作らない（`G-spec-sections` の番号連続性を動かさないため）」。**採らない。** 3 箇所へ同じ事実を書けばそれは写しであり、`AGENTS.md`「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」に反する。**新節が番号連続性を壊さないことは自分で測った**——`scripts/governance/checks/G-spec-sections.mjs` の判定は `### N.x` について `x === prevSub + 1` であり、§7.6 の次に §7.7 を置けば通る（**向こうの懸念は成立しない**）
- **タブ点（dirty dot）が点かないこと**: 向こうは「私の仕様判断」と留保付きで報告。**こちらは仕様として確定させる**——即時適用に未保存状態は存在せず、`SECTION_TABLE` の型が `Config` 外の値を表現できない以上、点けようがない（構造からの帰結であって選択ではない）

#### 未検証（残す）

- **`StartupApproved` の承認バイト列の形式**（向こうの D1・こちらの残余 1 と同じ穴に独立に到達した）。無効化された標本がこの機体に無く、形式を実測できない。**死角として宣言して止める**（`detector-scope-only-as-tight-as-needed`）
#### 台帳の事後修正（plan-review 完了後・再レビューは不要と判断）

- **Phase 2 の「`SettingsApp::new` で `is_enabled()` を読む」を撤回し、`run()` が読んで引数で渡す形へ直した。** `en_harness` が `SettingsApp::new` を呼ぶため、元の案では**既存 kittest 4 本すべての初期状態が開発機のレジストリに依存**していた（#963 の `HistoryStore::load()` と同型）。自己チェックの「kittest でクリックさせない」は**書き込み経路しか塞いでおらず、構築時の読み取り経路が漏れていた**
- **再レビューを起こさない判断の根拠**: 両レビューが既に D2（テストのレジストリ汚染）として名指しした領域を閉じる修正であり、新しい面を開かない。要件・対象ファイル・不変条件・テスト期待値のいずれも変わっていない（`SettingsApp::new` の引数が 1 つ増えるだけで、その呼び出し元は `run()` と `en_harness` の 2 箇所である）

- **`ADR-no-test-only-injection-in-product-code` が「サブキーのパスを引数に取る純関数」を禁じるか**（向こうの D2）。**こちらは禁じないと裁定した**——ADR が禁じるのは「計測・検査のためだけの**注入点**」であり、分岐も env 読みも増やさない純粋部/IO 部の分割は `path_env.rs` の `scan_path_dirs` が既に採っている形である。ただし**そもそもサブキーを引数に取らない設計にした**（`enable` / `disable` は引数を取らず、テストは `command_line_for` / `main_exe_from` だけを測る）ので、この論点は実装に現れない

### 主エージェント自身の照合（5a の 1〜5）

1. **issue の全要件に作業項目が対応する** — 要件は「登録できる」「削除できる」「チェックボックス形式」の 3 つ。Phase 1 が前 2 つ、Phase 2 が 3 つ目を持つ
2. **境界条件と検証** — 空白を含むパス（unit test）／既存値の上書き（受け入れ条件 4・目視 2）／値不在の削除（受け入れ条件 3・目視 5）／exe 不在（異常系の表）
3. **新しい状態・リソースの正常/失敗/破棄経路** — `RegKeyGuard` が `Drop` で `RegCloseKey`（既存の実績ある形）。`GeneralTabState` はプロセス寿命と一致し破棄経路を持たない。レジストリ値の生成/破棄は `enable` / `disable` の対称ペアで、`/symmetric-check` を Phase 4 に置いた
4. **より単純な既存パターンで置き換えられないか** — 置き換えた。状態を `Config` に持たせる「素直な」案を却下し、レジストリを SSOT にすることで migrate / persistence / serde 既定値／backup 除外の 4 面を**足すのではなく消した**
5. **壊してはならない不変条件に検知手段がある** — 「`config.toml` に現れない」は構造的に表現不能（検知器不要）。「引用符で括る」「兄弟から導く」は unit test。**`enable` / `disable` の OS I/O には検知器が無く、残余 4 として宣言した**

## 人間レビュー

- [x] 承認済み — 2026-09-01 / 問い: "上記を踏まえて、workspace/plan.md の計画を承認して /implement へ渡してよいですか？" / 回答: "承認する"

同時に裁定された 3 点（いずれも計画の記述どおりで確定）:

| 問い（逐語） | 回答（逐語） | 帰結 |
|---|---|---|
| "チェックボックスを Save ボタン経由にせず、切り替えた瞬間にレジストリへ効かせる設計にしています。[全般] タブ内で「Save が要る項目」と混在しますが、この方針でよいですか？" | "即時適用でよい（推奨）" | 計画のまま。Save 流へ載せる案は確定的に却下 |
| "レジストリキーの RAII ガード（RegKeyGuard）は現在 indexer/path_env.rs 内で private です。新モジュールでも要るため、共有するか重複させるかを決める必要があります。" | "snotra-core/src/win_registry.rs へ抽出して共有（推奨）" | 計画のまま。`path_env.rs` を触ることが承認された |
| "アンインストールしても HKCU の Run 値が残ります（NSIS アンインストーラは自分が作っていない値を消さないため）。この issue のスコープ外と判断しましたが、follow-up issue の扱いはどうしますか？" | "承認後に私が issue を作る" | **この計画の作業項目ではない。** 承認直後に別途 issue を作成し、番号を受容する残余 3 へ追記する |
