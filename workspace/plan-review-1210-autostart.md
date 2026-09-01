# 独立導出レビュー: #1210「feat: スタートアップに登録・削除できるようにする」

**導出者**: 独立導出エージェント。`workspace/` 配下の既存調査・計画は一切読んでいない。検索は `git grep ... -- ':!workspace'` に限定し、`.claude/worktrees/` も読んでいない。
**入力**: issue #1210 本文（あるべき姿: 設定アプリから Snotra 本体をスタートアップに登録／削除できること・チェックボックス形式）のみ。
**日付**: 2026-09-01

---

## 0. 一次観察（この導出の根拠になった実測）

| # | 観察 | 出所 |
|---|---|---|
| O1 | `snotra-core/Cargo.toml` は `windows` crate へ **`Win32_System_Registry` を既に有効化**している | `snotra-core/Cargo.toml` `[target.'cfg(windows)'.dependencies]` |
| O2 | `snotra-core/src/indexer/path_env.rs` が既に `RegOpenKeyExW` / `RegQueryValueExW` / `RegCloseKey` を使い、**`RegKeyGuard` という RAII ガードを持つ**（ただし private・当該モジュール内） | `snotra-core/src/indexer/path_env.rs:20-80` |
| O3 | `snotra-settings/Cargo.toml` の `windows` feature は `Win32_Graphics_Gdi` / `Win32_Foundation` / `Win32_System_SystemInformation` のみ。**Registry は無い** | `snotra-settings/Cargo.toml` |
| O4 | `src-tauri/Cargo.toml` は `Win32_System_Com` を持つが、`snotra-core` は持たない | 両 `Cargo.toml` |
| O5 | リポジトリ全体に autostart / スタートアップ登録の既存実装は**無い**（`git grep -i "startup\|autostart\|スタートアップ"` のヒットはすべて `show_on_startup`（起動時のウィンドウ表示）・`smoke-startup.ps1`・`startup.rs`（起動レイテンシ計測）で、別概念） | `git grep` 実測 |
| O6 | 設定アプリと本体の連携は **`config.toml` 1 点のみ・IPC 無し** | `snotra-settings/CLAUDE.md`「アーキテクチャ」/ `main.rs` の `//!` / SPEC §7.1 |
| O7 | `SettingsApp::has_changes()` は `self.draft != self.saved`（Config 全体の `PartialEq`）だけ | `snotra-settings/src/app.rs:196-198` |
| O8 | タブ別ダーティ点は `SECTION_TABLE`（Config セクション → TabId）から導出。**Config 由来でないダーティ源はこの表で表現できない** | `app.rs:113-137` |
| O9 | `reset_to_default()` は `draft = Config::normalized_default()`。Config に載る値はすべて既定へ戻る | `app.rs:237-246` |
| O10 | Backup タブのエクスポートは `config.toml` を**そのままコピー**、インポートは**上書き保存**（SPEC §13.3） | SPEC.md §13.3 / `tabs/backup.rs` |
| O11 | リリース形式は **ポータブル ZIP と NSIS インストーラーの 2 系統** | SPEC §20.5 |
| O12 | `snotra-settings` は「ユニットテストは書かない方針」。例外 3 つ（純ヘルパー境界 / kittest / UI 定数と core 既定の一致） | `snotra-settings/CLAUDE.md`「開発ルール」 |
| O13 | 新規 `.rs` は **2 つの別々の機構**が見る: `G-module-index`（`CLAUDE.md` モジュール構成の索引行）と `G-module-linkage`（crate ルートからの `mod` 到達性） | `scripts/governance/checks/G-module-index.mjs` / `G-module-linkage.mjs` |
| O14 | `TrKey` に variant を足すと `ja()` / `en()` が非網羅コンパイルエラーになる（網羅が強制される） | `snotra-settings/src/i18n.rs` の `//!` |

---

## 1. 問い 2 の結論（先に置く。ここで変更ファイル一覧が分岐するため）

### 結論: **チェックボックスの状態は OS 側（レジストリ `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` の値の有無）が正本であり、`config.toml` にキーを足さない。**

根拠は 4 つで、すべてこのリポジトリの既存の規範・機構から出る。

**根拠 1: config キーは「外部から書き換わる OS 状態の写し」になる（AGENTS.md「文書に事実の写しを増やす変更」の型）**
スタートアップ登録の実体はレジストリ値であり、**タスクマネージャーの「スタートアップ アプリ」タブから利用者が無効化できる**（無効化は `Run` 値を削除せず `StartupApproved` に書く——D1 参照。値そのものの削除は regedit や他ツールから可能）。config.toml に `bool` を持てば、その瞬間に写しがずれる。ずれたとき「どちらが真か」を決める調停規則が必要になり、どの規則を選んでも失敗モードが残る（config が勝てばユーザーの OS 側操作を勝手に取り消し、OS が勝てば config の値が黙って書き換わる）。**正本を 1 か所に定め他は参照へ**というルート規範の直接の適用である。

**根拠 2: Backup のエクスポート／インポートが機体を跨いで運んでしまう（SPEC §13.3）**
エクスポートは config.toml の逐語コピー、インポートは上書き保存である。config キーにすると、**A 機で作ったバックアップを B 機へインポートした瞬間に B 機のスタートアップ登録が変わる**。しかも登録に必要な exe の絶対パスは機体ごとに違う（ポータブル ZIP なら同一機体内でもフォルダ移動で変わる・O11）。「設定の移送」という利用者の意図に、OS 状態の書き換えは含まれていない。

**根拠 3: 「初期設定に戻す」が OS 状態を黙って書き換える（SPEC §7.3 / O9）**
`reset_to_default()` は Config 全体を既定へ戻す。config キーなら、Reset → Save で**スタートアップ登録が黙って解除される**。SPEC §7.3 は「既定設定相当の値をドラフトに適用する（保存は行わない）」としか言っておらず、OS 状態への波及は仕様の射程外である。同じことが Discard にも当たる（編集の破棄が OS 状態の巻き戻しを意味するのか、規則が無い）。

**根拠 4: config キーはレジストリ実装を「省く」わけではない——厳密な上位集合である**
どちらの設計でも `HKCU\...\Run` へ書く誰かが要る。config キー案は、そのコードに加えて schema フィールド・`field_mutations()`・`SECTION_TABLE` の確認・`save()` の部分失敗（config.toml は書けたがレジストリ書き込みが失敗＝**`.claude/rules/snotra-settings.md` が禁じる部分保存**）・SPEC §7.5 の反映タイミング行・`/persistence-check` の 4 点セット・Backup 汚染を**追加で**背負う。得るものは「Save ボタンの流れに載る」という UI の一貫性だけである。

**⚠️ この結論に伴い受け入れる帰結**（省かず明示する）:
- チェックボックスは Save を待たず**即時に効く**。同じ [全般] タブの他のチェックボックスと挙動が違う（→ 問い 3 で扱う）
- 本体（`src-tauri`）はこの機能に一切関与しない。`config_watcher` / `apply_config_change` は変更不要
- 設定アプリが単独起動されても機能する（本体の生死に依存しない）

---

## 2. 新規作成するファイル

| # | ファイル | 中身（シンボル） |
|---|---|---|
| N1 | `snotra-core/src/autostart.rs` | 新規モジュール。`//!` に責務を書く。<br>・`pub fn is_enabled() -> bool`（`Run` 配下に値が在るか）<br>・`pub fn set_enabled(enabled: bool, exe: &Path) -> Result<(), AutostartError>`（`true` で `RegSetValueExW`、`false` で `RegDeleteValueW`）<br>・`pub enum AutostartError`（またはエラー文字列を返す設計）<br>・`pub const RUN_VALUE_NAME: &str = "Snotra"`（**新しい永続識別子**）<br>・`fn run_key_path()` / 内部の `RegKeyGuard` 相当<br>・非 Windows 向けの `#[cfg(not(windows))]` スタブ（`path_env.rs` が同じ形を取っているか要確認 ⚠️）<br>・`#[cfg(test)] mod tests`（テスト用サブキーを引数で受ける形にする。下の T1 参照） |

**新規ファイルはこの 1 枚だけで足りる。** 設定 UI 側は既存の `tabs/general.rs` に checkbox を 1 つ増やすだけで、新タブも新モジュールも要らない（issue の要求は「[全般] 相当のチェックボックス 1 つ」であり、新タブを作るのは過剰）。

⚠️ **代替として `snotra-core/src/autostart/` ディレクトリ + `tests.rs` 分離もありうる**（`config/` 配下がその形）。ただし `hotkey.rs` / `window_data.rs` / `instant.rs` は単一ファイル + インライン `mod tests` なので、規模から見て単一ファイルが既存の粒度に合う。

---

## 3. 変更するファイル（シンボル付き）

| # | ファイル | 変更するシンボル・箇所 | 理由 |
|---|---|---|---|
| C1 | `snotra-core/src/lib.rs` | `pub mod autostart;` を追加（`pub mod binfmt;` … のアルファベット順の位置＝`binfmt` の前） | **`G-module-linkage` が見る**。忘れると `governance:check` だけが赤（cargo も rust-analyzer も報せない・#1085） |
| C2 | `snotra-core/CLAUDE.md` | 「モジュール構成」節にファイル名行 `- autostart.rs — …（責務は //!）` を追加 | **`G-module-index` が見る**。C1 とは別機構（O13） |
| C3 | `snotra-settings/src/tabs/general.rs` | `pub fn ui(...)` のシグネチャに `state: &mut GeneralTabState` を追加。「-- Behavior --」節に `ui.checkbox(&mut ..., tr.t(TrKey::CbStartWithWindows))` を追加し、`.changed()` で `snotra_core::autostart::set_enabled` を呼ぶ。新規 `pub struct GeneralTabState { enabled: bool, message: String, message_is_error: bool }` とその `new()` を定義。`//!` の「起動時表示・トレイ・IME・ホットキー」にスタートアップを追記 | チェックボックス本体。**ステートを持つのはフレームごとの `RegQueryValueExW` を避けるため**（`snotra-settings/CLAUDE.md`「フレームごとの重い処理を避ける」——`list_system_fonts()` と同じ型の罠） |
| C4 | `snotra-settings/src/app.rs` | ① `struct SettingsApp` に `general_state: tabs::general::GeneralTabState` フィールドを追加 ② `SettingsApp::new` で `GeneralTabState::new()`（＝起動時に 1 度だけ `autostart::is_enabled()` を読む）を初期化 ③ `ui_impl` の `match self.active_tab` の `TabId::General` アームへ `&mut self.general_state` を渡す | C3 の結線 |
| C5 | `snotra-settings/src/i18n.rs` | `TrKey` に `CbStartWithWindows` と、失敗時メッセージ用（例 `StatusAutostartFailed`）を追加。`ja()` / `en()` の両テーブルへ訳文を追加 | variant 追加で両テーブルが非網羅エラーになり網羅が強制される（O14） |
| C6 | `SPEC.md` | §7.2 `[全般]` タブの箇条書きに 1 行追加（下の問い 4 参照）。§13.3 に「エクスポート／インポートはスタートアップ登録を運ばない」旨を追記。§7.5 に「この設定は config.toml を経由せず即時に OS へ適用する（config_watcher の対象外）」を追記 | 挙動を変える変更ゆえ SPEC を同じ変更で整合させる（AGENTS.md 3 層分担） |
| C7 | `snotra-settings/CLAUDE.md` | 「アーキテクチャ」の「本体との連携は config.toml ファイル1点のみ」の**射程が偽になる可能性がある**——連携先が増えるわけではない（本体は関与しない）が、**設定アプリが config.toml 以外の永続状態（レジストリ）を書く**という事実は書かれていない。1 行足す | 全称に近い記述が実装より強くなるのを防ぐ（AGENTS.md「検証の作法」） |
| C9 | `docs/architecture.md` | 「設定管理」節の「Save 時にのみ `config.toml` に書き込み」の射程を補う 1 行 + 「データ永続化」節に「OS 側の状態（スタートアップ登録）の正本は `snotra-core::autostart`」の参照 1 行 | 実測で判明（§9b D5）。**数え上げを増やすのではなく正本を指す** |
| C8 ⚠️ | `snotra-core/CLAUDE.md` | 「Run 値名 `Snotra` は永続識別子であり改名すると旧値が孤児として残る」旨を、既存の永続キー（history キー等）の節の近くに 1 行 | 下の問い 5「永続形式・識別子/キー形式を変更」トリガーへの応答。**⚠️ 既存の節構成を読み切っていないので、置き場は要確認** |

### 変更しないと判断したもの（根拠つき）

- **`snotra-core/src/config/schema.rs`（`GeneralConfig`）** — 問い 2 の結論により config キーを足さない
- **`snotra-settings/src/app.rs` の `SECTION_TABLE` / `has_changes()` / `save()` / `reset_to_default()` / Discard / × ボタンガード** — 即時適用ゆえ draft/saved に参加しない（問い 3 で詳述）
- **`app.rs` の `#[cfg(test)] mod tests` の `field_mutations()`** — Config を変えないので `..` なし destructure は E0027 にならない
- **`src-tauri/src/config_watcher.rs`（`apply_config_change`）・`src-tauri` 全体** — 本体は関与しない
- **`snotra-settings/Cargo.toml`** — Registry feature は `snotra-core` 側で足りる（O3）。ロジックを core に寄せる既存ルールとも一致
- **`snotra-core/Cargo.toml`** — `Win32_System_Registry` は既に有効（O1）。**依存追加ゼロ**

---

## 4. 問い 3: Save ボタンの流れに載せるか

### 結論: **載せない（即時適用）。** 理由と、3 案それぞれの追加変更点を列挙する。

#### 案 A（推奨・上の一覧が前提とするもの）: 即時適用・draft/saved に参加しない

**追加で変更が要るもの**: C3 / C4 / C5 のみ。`app.rs` の以下は**変更不要**であることを、実際に読んで確認した:

| `app.rs` の機構 | 変更要否 | 根拠（行） |
|---|---|---|
| `has_changes()` = `draft != saved` | 不要 | `app.rs:196`。Config を変えないので dirty にならない |
| `SECTION_TABLE` / `TabId::has_changes`（タブ点 `•`） | 不要 | `app.rs:106-137`。表は `Config` セクションの差分関数の配列であり、Config 外の値は**そもそも表現できない**（型 `fn(&Config,&Config)->bool`） |
| Save ボタンの `can_save` | 不要 | `app.rs:532` |
| Discard（`draft = saved.clone()`） | 不要 | `app.rs:550`。即時適用済みなので巻き戻す対象が無い |
| `reset_to_default()` | 不要 | `app.rs:237`。OS 状態を触らないのが正しい（§1 根拠 3） |
| Escape / × ボタン（`CancelClose`）ガード | 不要 | `app.rs:383-397`。未保存の変更が生じないため |
| ウィンドウタイトルの `*` | 不要 | `app.rs:311` |
| 既存テスト `section_table_covers_all_config_fields` / `..no_false_positive..` / `backup_tab_never_shows_dirty_dot` | 不要（無傷で通る） | `app.rs:761-805`。Config を変えないため |
| kittest 4 本 | 不要 | `app.rs:846-1027`。ただし T1（下）の危険がある |

**フィードバック経路**: 失敗時（権限エラー等）のメッセージは**フッターではなくタブ内インライン**に出す。`snotra-settings/CLAUDE.md`「フッター vs インラインの使い分け」が「draft/saved に参加しないタブでフィードバックが必要ならタブ固有の state にメッセージを持たせてタブ内にインライン表示する。フッターを流用すると永続性の要件やエラーとの衝突が起きる」と明示しており、Backup タブが既に先例である。ゆえに `GeneralTabState` にメッセージを持たせる（C3）。

**受け入れる残余 ⚠️**: [全般] タブで唯一「Save を要さないチェックボックス」になる。UI 上の緩和は checkbox の直後に補助文（「すぐに反映されます」）を置くこと。**これは UI 文言の話であり、機構の話ではない。**

#### 案 B: config.toml にキーを足して Save 流に載せる

**追加で変更が要るもの**（案 A に対する差分。すべて実際に読んで導出した）:

1. `snotra-core/src/config/schema.rs` — `GeneralConfig` に `pub start_with_windows: bool` + `fn default_start_with_windows() -> bool { false }` + `#[serde(default = "...")]` + `impl Default for GeneralConfig` への追加（schema の `//!` が「新しいセクション・設定キーには serde の既定を付ける」を不変条件として要求）
2. `snotra-settings/src/app.rs` `field_mutations()` — `Config { ... }` destructure が **E0027** になる。`start_with_windows: _,` の 1 行を足すだけでコンパイルは通るが、それだと `vec!` にも足さないまま緑になる（`app.rs:694-757` の doc がこの穴を #1008 実測として明記）。**`vec!` にも mutation を足す必要がある**
3. `app.rs` の `SECTION_TABLE` — 追加不要（`d.general != s.general` が既に覆う）。ただし**それを確認した**という判断が要る
4. `app.rs` `save()` — `config.save()` の**後**に `autostart::set_enabled()` を呼ぶ必要がある。ここで**部分保存**が起きる: config.toml は書けたがレジストリが失敗したとき、`saved = config` を進めてよいか。`.claude/rules/snotra-settings.md` は「save 成功・完全ロード時のみ `saved` を進める・部分保存禁止」を明示しており、**この案はその不変条件と正面から衝突する**
5. `SettingsApp::new()` — 起動時に config の値と実レジストリ状態が食い違う場合の調停が要る（外部で無効化されたときの表示。無ければチェックボックスが嘘をつく）
6. `SPEC.md` §7.5 に反映タイミングの行、§13.1 の設定キー、§7.2
7. `/persistence-check`（AGENTS.md の「永続形式・識別子/キー形式を変更」行）の 4 点セット: 新規記録・既存移行・外部参照 API
8. Backup の export/import が運ぶことへの手当て（SPEC §13.3 の追記か、import 時の除外実装）
9. Reset to default が OS 状態を巻き戻すことへの手当て
10. **レジストリ操作コード（N1）は案 A と同じだけ要る**

#### 案 C: draft/saved に参加させるが config.toml には書かない（レジストリ用の draft を別に持つ）

**却下。** 追加で要るもの:
- `SettingsApp::has_changes()` を `self.draft != self.saved || self.autostart_draft != self.autostart_saved` へ変える → `app.rs:196` の 1 行が「Config の比較」でなくなり、`saved` / `draft` 二重状態モデルの記述（`snotra-settings/CLAUDE.md`）が偽になる
- タブ点は `SECTION_TABLE` で表現できない（型が `fn(&Config,&Config)->bool`）ため、`TabId::has_changes` に General 専用の追加項を書く → **`SECTION_TABLE` が SSOT である**という `app.rs:113-123` の doc が偽になる
- `section_table_no_false_positive_when_unchanged`（`app.rs:781`）の前提が変わる
- Discard / Reset で OS 状態を巻き戻すか決める必要が再来する
- `save()` の部分保存問題は案 B と同じ

得るものは「Save 流の一貫性」だけで、壊すものが多すぎる。

---

## 5. 問い 4: `SPEC.md` の更新箇所（節番号）

| 節 | 変更 | 要否 |
|---|---|---|
| **§7.2 タブ構成と設定項目**の `[全般]` タブ箇条書き | 「Windows のログオン時に自動起動する（スタートアップ登録）」を 1 行追加。既存の「起動時にウィンドウ表示するか」（＝`show_on_startup`）と**別概念であることが読者に判る書き方にする**——この 2 つは名前が近く、混同すると仕様が壊れる | **必須** |
| **§7.5 設定反映タイミング** | 「スタートアップ登録: `config.toml` を経由せず、チェックボックス操作時に即時 OS へ適用する（`config_watcher` の対象外・本体は関与しない）」を追加 | **必須**（この節は「何がどう反映されるか」の一覧であり、載らないと読者は config 経由だと推測する） |
| **§13.1 設定データ** | 変更なし（config.toml にキーを足さないため） | 不要 |
| **§13.3 設定バックアップ** | 「エクスポート／インポートはスタートアップ登録を含まない（OS 側の状態であり config.toml に無い）」を追記 | **必須**（利用者から見える挙動であり、書かないと「バックアップしたのに復元されない」が不具合に見える） |
| **§14 実行仕様（起動）** | ⚠️ 判断保留。この節は「起動要求 → 結末」の契約であり、ログオン時自動起動は射程が違うように読める。**新規に §14.3 を足すより §7 側で完結させるほうが節の責務に合う** | 不要と判断 |
| **§20.5 リリース形式** | ⚠️ ポータブル ZIP でフォルダを移動すると登録した絶対パスが陳腐化する。仕様として書くなら §7.2 の当該行の注か、§7.5 | ⚠️ 要判断（下の「⚠️ 一覧」D3） |

**⚠️ `G-spec-sections` は SPEC.md の節番号の連続性を機械照合する。** 新しい `## N.` / `### N.x` を足すなら番号が飛ばないこと。**既存節への箇条書き追加だけなら番号は動かない**——上の推奨はすべて既存節への追記で、新節を作らない。

---

## 6. 問い 5: `AGENTS.md`「条件別チェック」で当たる行

| 当たる行（逐語） | 何を実行するか |
|---|---|
| **「ファイル（`.rs`）を追加/削除」** | `autostart.rs` の責務は `//!` に書く。`snotra-core/CLAUDE.md` のモジュール構成節にファイル名行を足す（C2）。**索引と `mod` 宣言は別々の機構が見る**（C1・O13）。当該行が名指しするとおり、**索引の reminder が鳴らなかったことを「`mod` も足りている」と読まない** |
| **「関数・型を新規定義／改名／導入」** | `is_enabled` / `set_enabled` / `AutostartError` / `GeneralTabState` が該当。呼び出し元の列挙は LSP の `findReferences`（grep へ落とさない）。**`/dry-check`** を走らせる——具体的な当たりが 1 件ある（`indexer/path_env.rs` の `RegKeyGuard` と、`RegOpenKeyExW` の呼び出し定型）。**新 API の導入と呼び出し点の移行を 1 タスクに束ねる**（`-D warnings` 下で未使用の新 API は `dead_code` で落ちる——`autostart.rs` を先に入れて `general.rs` を後にすると赤くなる） |
| **「対称ペア（…生成/破棄・フラグ真偽）を変更」** | `set_enabled(true)`（`RegSetValueExW`）と `set_enabled(false)`（`RegDeleteValueW`）が対称ペアそのもの。**`/symmetric-check`**。とくに「削除側が『値が無い』を成功として扱うか」（`ERROR_FILE_NOT_FOUND` を成功に畳むか）を明示的に決める |
| **「永続形式・識別子/キー形式を変更」** | ⚠️ config キーは足さないが、**Run 値名 `"Snotra"` は新しい永続識別子である**（一度書いたら改名時に旧値が孤児として残り、二重起動の原因になる）。`/persistence-check` の射程に入るかは判断が要るが、**少なくとも「識別子を後から変えられない」ことを doc に書く**（C8） |
| **「ガバナンス文書（`*.md`…）を変更」** | `SPEC.md` / 2 つの `CLAUDE.md` を触るので **`npm run governance:check`**（`docs/build-commands.md` カテゴリ F）。PR では `governance-check` job が常時実行 |
| **「アーキ・横断パターン…に影響」** | `docs/architecture.md` に「設定アプリが書く永続先」の記述が在れば触る範囲に含める ⚠️（未確認・下の D5） |
| **「各言語ファイルを編集」** | `.claude/rules/` が自動配送される（`snotra-core.md` / `snotra-settings.md` / `comments.md`）。**`//!` / `///` を書いたら `cargo doc` を手で走らせる**——intra-doc link 切れは PostToolUse hook が沈黙し CI でのみ発火する |
| **「調査・測定のための一時的な足場」** | 当たらない（製品機能であり足場ではない） |
| **「機能削除・trace イベント名／hotkey 登録・表示経路の変更」** | 当たらない（hotkey 登録＝`RegisterHotKey` であり、レジストリ登録とは別物） |
| **「UI モード・ガード条件を追加/変更」** | ⚠️ 弱く当たる。チェックボックス 1 つはモードでもガードでもないが、**初回起動フロー**（`--first-run` で Index タブが開く）との相互作用は一度だけ問う価値がある——初回起動時に [全般] タブは開かれないので、実質当たらないと判断 |

**この一覧が母集団のすべてだとは主張しない。** `AGENTS.md` の表を上から順に当てて拾ったものであり、実装中に触る隣接ファイルによって増えうる。

---

## 7. 問い 6: ガバナンス機構が要求する、忘れやすいもの

**新規 `.rs` を 1 枚足すとき、2 つの別々の機構が別々のものを見る**（`G-module-index.mjs` / `G-module-linkage.mjs` の冒頭コメントが逐語でこう書いている）:

1. **`G-module-index`** — `snotra-core/CLAUDE.md`「モジュール構成」節に `autostart.rs` の**索引行**が在るか（実ファイル ↔ 索引の双方向照合）
2. **`G-module-linkage`** — `snotra-core/src/lib.rs` に **`pub mod autostart;`** が在るか（crate ルートからの `mod` 到達性）

**この 2 つは編集時の見え方が非対称である**（ルート `CLAUDE.md` が #1139 として明記）:
- 索引漏れは**編集直後に reminder が鳴る**
- **`mod` 忘れは `governance:check` だけが赤にする**——`cargo fmt/clippy/test` は未リンクの `.rs` を見ず（PostToolUse hook は沈黙）、rust-analyzer も `unlinked-file` を publish しない（#1085 実測）。最悪の帰結は **`#[cfg(test)] mod tests` が 1 度もコンパイルされずテストが黙って走らない**こと

**ゆえに「索引の reminder が鳴らなかったこと」を「`mod` も足りている」と読んではならない。** 実行すべきは `npm run governance:check` を PR 前に 1 度打つこと（`pr-governance-check-before-pr` の型の事故が #629/#630 で再発している）。

その他、この変更で当たるもの:
- **`G-spec-sections`** — SPEC.md の節番号連続性（既存節への追記だけなら安全）
- **`G-heading-refs` / `G-references`** — `CLAUDE.md` / `SPEC.md` に節見出し参照やファイル参照を書くなら、参照先が実在すること
- **`G-stale-identifiers`** — 撤去した識別子の残存（今回は撤去が無いので当たらない）

---

## 8. 問い 7: Windows で「ログオン時に起動する」機構の候補と、追加コスト最小のもの

| # | 機構 | 実体 | このリポジトリでの追加コスト |
|---|---|---|---|
| M1 | **`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` のレジストリ値** | 値名 = 任意（`Snotra`）、値 = exe の絶対パス（引数を付けるなら引用符必須） | **依存追加ゼロ。** `snotra-core` は既に `windows` crate の `Win32_System_Registry` を有効化済み（O1）で、`indexer/path_env.rs` が同じ API 群（`RegOpenKeyExW` / `RegCloseKey`）を現に使っている（O2）。要るのは `RegSetValueExW` / `RegDeleteValueW` / `RegQueryValueExW` で、**すべて同じ feature の中にある**。管理者権限不要 |
| M2 | **スタートアップフォルダに `.lnk` を置く** | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\Snotra.lnk` | `.lnk` の作成には **COM（`IShellLinkW` + `IPersistFile`）が要る**。`snotra-core` の `windows` feature に `Win32_System_Com` / `Win32_UI_Shell` が**無い**（O4——`src-tauri` にはあるが core には無い）。feature 追加＋COM 初期化の作法が要る。**M1 より明確に高い** |
| M3 | **タスクスケジューラ（`schtasks` / Task Scheduler COM）** | ログオントリガーのタスク | COM か外部プロセス起動が要る。遅延起動や昇格実行が要るときの選択肢だが、今回は要件に無い。**過剰** |
| M4 | **`HKLM\...\Run`（全ユーザー）** | 同上を HKLM に | **管理者権限が必要**。設定アプリは非昇格で動く前提であり、書き込みが失敗する。要件（設定アプリから登録・削除）と両立しない |
| M5 | **`tauri-plugin-autostart`** | Tauri プラグイン | **構造的に当たらない。** プラグインは本体（`src-tauri`）のランタイムに属し、設定アプリ（`snotra-settings`）は**別プロセスで Tauri を持たない**（O6）。使うなら「設定アプリ → config.toml → 本体が適用」という経路を**強制**され、問い 2 で却下した config キー設計に固定される。さらに**本体が動いていないと適用されない**。新規依存 1 本の追加でもある |
| M6 | **NSIS インストーラーのオプション** | インストール時に登録 | issue は「設定アプリから登録・削除」を求めており、インストール時の 1 回きりでは要件を満たさない。**ポータブル ZIP 版（O11）には効かない** |

### 結論: **M1（HKCU\...\Run のレジストリ値）**

決め手は 2 つ。**(a) 依存追加ゼロ**——必要な API がすべて既に有効な feature の中にあり、同じ crate の隣のモジュールに呼び出しの定型と RAII ガードの先例がある（O1・O2）。**(b) 設定アプリ単独で完結する**——M5 と違って本体のランタイムに依存せず、`config.toml` 1 点連携という既存アーキテクチャ（O6）を一切変えない。

**書き込む exe パス**: `std::env::current_exe()` は `snotra-settings.exe` を返すので、**その親ディレクトリの `snotra.exe`** を導く（issue が言う「Snotra 本体」は `snotra.exe`）。⚠️ 兄弟に居ることは §20.5 の 2 形式（ポータブル ZIP・NSIS）どちらでも成り立つが、**実測で確かめる価値がある**。

---

## 9. ⚠️ 確信が持てない項目（省かず全件）

| # | 項目 | なぜ確信が持てないか |
|---|---|---|
| D1 | **`StartupApproved` の存在**（死角） | タスクマネージャーの「無効にする」は `Run` の値を**削除せず**、`HKCU\...\Explorer\StartupApproved\Run` に無効フラグを書く。**`Run` 値の有無だけを見るチェックボックスは「チェック済みなのに Windows は起動しない」状態を表示しうる。** 対処は 2 つ（`StartupApproved` も読む／死角として宣言して止める）で、`detector-scope-only-as-tight-as-needed` の型に従えば**死角として宣言する**ほうが既存の選好に合う。**どちらを採るかは実装前に決める必要がある。** バイナリ形式も未実測 |
| D2 | **テストのレジストリ汚染** | `snotra-core` 側にテストを置くなら、**開発機の実 `HKCU\...\Run` を書き換えるテストになりうる**。`ADR-no-test-only-injection-in-product-code` が製品コードへのテスト専用 seam を禁じているので、逃げ道は「サブキーのパスを引数に取る純関数」（本番は `Run` を、テストはスクラッチのサブキーを渡す）だと思うが、**その形が当該 ADR の禁止に当たるかは読み切れていない**。`snotra-settings` 側は「ユニットテストを書かない方針」（O12）ゆえテストを置かない。**kittest でこのチェックボックスをクリックするテストを書いてはならない**（実 HKCU を書く） |
| D3 | **ポータブル ZIP でのパス陳腐化** | フォルダを移動すると登録済みの絶対パスが死ぬ。**再チェックで登録し直せば直る**が、利用者にはそれが判らない。仕様に書くか、起動時に本体が自己修復するか（後者は本体を関与させることになり §1 の結論と衝突する）。**未決** |
| D4 | **`snotra-core` の非 Windows ビルド** | `path_env.rs` が `#[cfg(windows)]` / `#[cfg(not(windows))]` の対をどう作っているか末尾まで読んでいない。`autostart.rs` も同じ形を踏襲すべきだが、**その形を実測していない** |
| D5 | **`docs/architecture.md` の該当節** | 「設定アプリが書く永続先」に相当する記述が在るかを確認していない。在れば触る範囲に入る |
| D6 | **`RegKeyGuard` の重複** | `indexer/path_env.rs:20` の `RegKeyGuard` は private。`autostart.rs` で同じものを書くと `/dry-check` が必ず挙げる。**共通ヘルパーへ持ち上げるか、10 行の重複を理由つきで受け入れるかを実装前に決める**（`AGENTS.md`「消す/共通化する前に『片方だけが変わる将来を 1 つ挙げられるか』を問う」——読み専用ガードと読み書きガードで将来が分かれるかは、私には判断材料が無い） |
| D7 | **UI の配置** | [全般] タブの「-- Behavior --」節に置くのが自然に見えるが、`SETTINGS-DESIGN.md`（新タブ・新パーツ追加チェックリスト）を読んでいない。**新しい節見出し（「-- スタートアップ --」）を作るべきかは同書が決める** |
| D8 | **訳語** | 「スタートアップ」はカタカナで確定（issue タイトルもそう）。UI 文言案は ja「Windows 起動時に自動的に開始する」/ en「Start with Windows」だが、**既存の文言の語調に合わせる確認をしていない** |
| D9 | **`Run` 値に引数を付けるか** | `snotra.exe` を素で起動するのか、`--minimized` 相当を渡すのか。**`show_on_startup = false` が既定**（`schema.rs`）なので素の起動で非表示常駐になり、引数は不要に見える。ただし SPEC §8.4 を精読していない |
| D10 | **`SECTION_TABLE` を触らない判断の裏取り** | 案 A では Config を変えないので表は無関係——これは型（`fn(&Config,&Config)->bool`）から出る構造的な結論であり確度は高い。**ただし「タブ点が点かないこと」が利用者にとって正しいか**は仕様判断であり、私は「即時適用ゆえ未保存状態が存在しない → 点かないのが正しい」と判断した |

---

## 9b. ⚠️ のうち、追加の実測で解決したもの

| # | 解決 |
|---|---|
| **D4 解決** | `path_env.rs:113-116` に `#[cfg(not(windows))] fn read_user_path() -> Option<String> { None }` がある。**同名関数の cfg 対で、非 Windows は無害な既定を返す**形。`autostart.rs` もこれに倣う（`is_enabled() -> false` / `set_enabled() -> Ok(())` または明示エラー） |
| **D5 解決（変更対象が 1 つ増える）** | `docs/architecture.md`「設定管理」節に **「`snotra-settings` の設定編集は draft/saved 二重状態モデル（Save 時にのみ `config.toml` に書き込み）」**という行がある。案 A（即時適用）はこの記述の射程を超える——設定アプリが Save を経ずに OS 状態を書く経路が生まれる。**同節に 1 行足す必要がある（C9）**。加えて「データ永続化」節は永続先を `%APPDATA%\Snotra\` のファイル 5 種で数え上げており、**レジストリという新しい永続先がそこに載らない**。数え上げを増やすのではなく「OS 側の状態（スタートアップ登録）は `snotra-core::autostart` が正本」と参照で書く（AGENTS.md「数え上げは偽になる時点が確定している——数ではなく正本を指す」） |
| **D7 解決** | `SETTINGS-DESIGN.md`「新タブ追加チェックリスト」は**新タブを作るときの規約**であり、今回は当たらない（既存 [全般] タブへの追加）。既存の「-- Behavior --」節に `ui.checkbox` を並べるのは他の 5 つと同形で規約内。**補助文（「すぐに反映されます」）を置くなら `style::hint(ui, text)` を使う**（副次テキストの正規形。色・サイズを直書きしない）。失敗メッセージのインライン表示も同様で、backup タブが `separator` + message の先例を持つ |
| **D9 解決** | SPEC §8.4 は「`show_on_startup = false` の場合は非表示常駐でホットキー待ち」とし、`schema.rs` の `default_show_on_startup()` は `false`。**ゆえに `Run` 値は `snotra.exe` を素で起動すればよく、引数は不要**。⚠️ 残る判断: `show_on_startup = true` の利用者はログオンのたびに検索ウィンドウが開く。これは 2 つの設定の合成として説明可能な挙動であり、`--minimized` 相当を足して打ち消すのは**設定を無視することになる**ので、素の起動が正しいと判断した |

**C9（追加）**: `docs/architecture.md`「設定管理」節 +「データ永続化」節に各 1 行。

## 10. 実装順序の提案（`-D warnings` の `dead_code` を踏まないため）

1. `snotra-core/src/autostart.rs` 新設 **と** `lib.rs` の `pub mod` **と** `general.rs` の呼び出し点を**同一コミットに束ねる**（AGENTS.md「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）
2. `i18n.rs` の TrKey 追加（コンパイルエラーが網羅を強制する）
3. `app.rs` の結線
4. `snotra-core/CLAUDE.md` 索引行・`SPEC.md` §7.2/§7.5/§13.3
5. `npm run governance:check` → `cargo doc`（`//!` を書いたため）→ カテゴリ A
6. `/symmetric-check`・`/dry-check`

---

*（本ファイルは独立導出の結果であり、他エージェントの調査・計画とは照合していない。食い違いがあれば、それは独立性が保たれた証拠として扱ってよい。）*
