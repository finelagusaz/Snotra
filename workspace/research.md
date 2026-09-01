# 調査 — #1210 スタートアップに登録・削除できるようにする

## 1. issue の要約

- 設定アプリ（`snotra-settings`）から本体（`snotra.exe`）を Windows のスタートアップへ登録・削除できること
- UI 形式はチェックボックス
- 本文はこの 2 行のみ。遅延起動・最小化起動・per-machine 登録などの追加要求は無い（**スコープを広げない**）

## 2. 実測した一次証拠（この機体・Windows 11 26200）

`scratchpad/probe-run-key.ps1` を実行して得た値。**推測ではなくこの機体で測った**。

| 測ったこと | 結果 |
|---|---|
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` への書き込み | 成功（**昇格不要**） |
| 書いた値の読み戻し | `"C:\probe\x.exe"` / kind = `String`（REG_SZ）。**引用符を含めて逐語で保存される** |
| 新規に作った Run 値に対する `Explorer\StartupApproved\Run` のレコード | **ABSENT**（自動生成されない） |
| 存在しない値の削除 | **例外**（`PSArgumentException`。Win32 では `ERROR_FILE_NOT_FOUND`） |
| Startup フォルダ | `C:\Users\<user>\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup`（実在） |

現在の `Run`（4 件）と `StartupApproved\Run`（2 件）の実内容も読んだ。**値名そのものは第三者アプリの個人インベントリなのでここへ写さない**（このリポジトリは公開されており、`workspace/` は squash マージで main の履歴に残る・`AGENTS.md` の #999 行）。論証に要るのは所属だけである。

| 所属 | 件数 |
|---|---|
| `Run` と `StartupApproved\Run` の両方 | 1（承認バイト先頭 `02`） |
| `StartupApproved\Run` のみ | 1（承認バイト先頭 `02`） |
| `Run` のみ | 3 |

ここから確実に言えること: **`StartupApproved\Run` のレコードは `Run` 値とは独立に存在し、`Run` 値の有無を意味しない**。逆に、レコードが無いこと（新規作成直後）は「有効」を意味する。

**測れていないこと（未確定へ送る）**: タスクマネージャーの「無効化」が `Run` 値を消すのか、`StartupApproved` 側にマークを置くだけなのか。手元の 2 件はどちらも `02`（有効）であり、無効側の標本が無い。この判定がチェックボックスの意味論（「登録されている」か「実効的に有効」か）を決める。

## 3. 関連ファイル・モジュール・シンボル

すべて `git grep` で実在を確認した（`grep -r` は `.claude/worktrees/` の追跡外を混ぜるため使わない）。

### 再利用できる既存パターン

| 場所 | 何が再利用できるか |
|---|---|
| `snotra-core/src/indexer/path_env.rs` の `RegKeyGuard` | `RegCloseKey` を呼ぶ RAII ガード。**現在 `path_env.rs` 内 private** |
| 同 `read_user_path` | `RegOpenKeyExW` + `RegQueryValueExW`（2 段: サイズ取得 → 値取得）+ `w!()` リテラル + `cfg(not(windows))` スタブの形 |
| 同 `scan_path_dirs` の doc | 「`read_user_path` から分離することでテスト可能性を確保」——**純粋部と OS I/O 部を割る先例** |
| `snotra-core/src/config/location.rs` の `config_dir` / `config_dir_from` | **同じ型のより明示的な先例**: env / OS を読む関数と、読まない判定核（「並列テストから安全に測れる」と doc が名乗る）に割る |
| `src-tauri/src/commands/window.rs` の `open_settings_window`（`current_exe()` の周辺） | `current_exe()` → 同ディレクトリの兄弟 exe を組み立てる形。今回はこの**鏡像**（settings → `snotra.exe`） |
| `snotra-settings/src/tabs/backup.rs` の `//!` | 「他タブと異なり Save/Discard ボタンを表示しない（即時操作のため）」——**即時適用 UI の先例** |
| `snotra-settings/src/i18n.rs` | `TrKey` variant を足すと `ja()` / `en()` が非網羅コンパイルエラーになる（網羅は機構が強制） |

### 依存の状況

- `snotra-core/Cargo.toml` は `windows 0.62.2` を `Win32_System_Registry` **feature 込みで既に持つ**（`[target.'cfg(windows)'.dependencies]`）
- **どの Registry API が feature 追加なしで使えるかは実測済み**（windows-0.62.2 のソースを直接読んだ・3b の❌1 を自分で裁定）:

  | API | `Win32_System_Registry` だけで使えるか |
  |---|---|
  | `RegOpenKeyExW` / `RegQueryValueExW` / `RegSetValueExW` / `RegDeleteValueW` | **使える**（`cfg(feature)` ゲート無し） |
  | `RegCreateKeyExW` | **使えない**——`#[cfg(feature = "Win32_Security")]` が付く（`SECURITY_ATTRIBUTES` を引数に取るため） |

  → **`RegCreateKeyExW` を使わない設計にする。** `Run` は Windows が用意する well-known key であり作成の必要が無い。書き込みは `RegOpenKeyExW(HKEY_CURRENT_USER, "Software\\...\\Run", KEY_WRITE)` で足りる（feature 追加 0 という結論は生き残る）
- `snotra-settings/Cargo.toml` の `windows` は `Win32_Graphics_Gdi` / `Win32_Foundation` / `Win32_System_SystemInformation` のみ。**Registry feature を持たない**
- → 新モジュールを `snotra-core` へ置けば依存・feature の追加が 0 になる。`snotra-settings` へ置くなら feature 追加が要る

### 触る候補ファイル

- `snotra-core/src/lib.rs`（`pub mod` 宣言。**索引と `mod` 宣言は別々の機構が見る**——`governance:check` が `mod` 忘れを赤にする）
- `snotra-core/CLAUDE.md`「モジュール構成」（ファイル名の索引行）
- `snotra-settings/src/tabs/general.rs`（チェックボックス）
- `snotra-settings/src/i18n.rs`（`TrKey` + `ja()` / `en()`）
- `SPEC.md` §7.2（全般タブの項目）・§7.3（初期設定に戻す）・§13.1（設定データ）
- `snotra-settings/CLAUDE.md`（必要なら）

## 4. 中核の設計判断 — 状態を誰が所有するか

### 判断: **OS（レジストリ）を SSOT にし、`Config` のフィールドにしない**

`Config` へ `general.launch_at_logon: bool` を足す案を却下する理由（4 つ）:

1. **Backup タブの export/import が機体ローカルの OS 状態を持ち運ぶ**——別マシンで import すると、そのマシンのスタートアップ登録が反転する（`backup.rs` の `BackupResult` を受けて `app.rs` が `saved` を丸ごと差し替えることを 3b が逐語確認）
2. **「初期設定に戻す」（SPEC §7.3）が登録解除の副作用を持つ**——`reset_to_default` は既定値相当をドラフトへ入れる。**「黙って」ではない**（3b の⚠️5 を採用）: ドラフトが汚れればタイトルの `*`・フッターの未保存表示・×ボタンガードで可視化される。それでも**ユーザーが「設定を初期化した」つもりの操作が OS のログオン挙動を変える**という筋の悪さは残る
3. **`config.toml` ↔ レジストリの乖離を誰も調停しない**——タスクマネージャーやレジストリエディタで外から変えられる状態であり、config 側の値は「最後に設定アプリが書いた値」でしかない
4. **新規 config キーを作らなければ `persistence-check` / `migrate.rs` / serde 既定値の面が丸ごと消える**

副次的に、`snotra-settings/CLAUDE.md`「本体との連携は `config.toml` ファイル 1 点のみ」とも整合する——**スタートアップ登録は本体との連携ではない**（本体は一切読まない。OS が読む）。

### 適用タイミング: **即時適用**（Save ボタンを経由しない）

- 先例: `backup.rs` が「即時操作ゆえ Save/Discard を出さない」を既に確立している
- Save 適用にする案は、`app.rs` の `has_changes()`（`draft != saved` の `Config` 単独の `PartialEq`）・`SECTION_TABLE` によるタブ点灯の導出・Discard/Reset の意味・`section_table_*` / kittest の不変条件すべてに**非 `Config` の dirty 源を配線する**必要がある。チェックボックス 1 個に対する費用が釣り合わない
- **上の §4 冒頭の判断（`Config` に入れない）が、この即時適用が既存機構と衝突しない理由そのものである**（3b の⚠️6 を採用して明記）。`has_changes()` も ×ボタンガード（`CloseRequested` 時の未保存チェック）も `draft != saved` だけを見るので、`Config` の外にある状態はどちらの判定にも入らない——**衝突しないのは偶然ではなく、状態を `Config` の外に置いた帰結である**
- 残余: **1 タブ内に「Save が要る項目」と「即時に効く項目」が混在する**。節見出しを分け、操作直後にインライン status を出すことで緩和する（消滅はしない・残余として宣言する）

### 機構: `HKCU\...\CurrentVersion\Run`（Startup フォルダの `.lnk` は却下）

- `.lnk` 生成は COM（`IShellLink` + `IPersistFile`）が要り、依存と unsafe 面が増える
- Run 値は REG_SZ 1 本で、読み・書き・削除が既存の `windows` crate feature だけで閉じる
- 値名は `Snotra`、値は **絶対パスを引用符で括った文字列**
- **この機体の実インストール先は `C:\Users\<user>\AppData\Local\Snotra\`（per-user・空白なし）**（3b の⚠️2 を採用して修正。当初 `C:\Program Files\Snotra\` と書いたのは per-machine の思い込みだった）。`snotra.exe` / `snotra-settings.exe` / `uninstall.exe` の 3 本が同居することを実測した
- **それでも引用符は必要である**——理由は「インストール先が空白を含む」ではなく、**ユーザー名に空白を含みうる**（既定インストール先が `%LOCALAPPDATA%` 配下ゆえパスにユーザー名が入る）。引用符無しの `Run` 値は最初の空白で切られる

### 却下する代替案（否定の知識として計画へ残す）

- **`tauri-plugin-autostart`**: 却下。本体（`src-tauri`）側に状態を持たせることになり、設定アプリからのトグルに IPC が要る。「連携は `config.toml` 1 点」という設計に反する。設定アプリは Tauri プロセスではない
- **`HKLM\...\Run`（per-machine）**: 却下。昇格が要る。設定アプリは非昇格で動く。実インストールも per-user である（上記実測）
- **`RunOnce`**: 却下。1 回だけ実行して値が消える機構であり、「毎回のログオンで起動する」という要求と意味が違う
- **タスクスケジューラ（ログオントリガー）**: 却下。COM（`ITaskService`）か `schtasks.exe` の起動が要り、`Run` 値 1 本に対して機構が重い。遅延起動・最上位特権といった追加の利得は今回の要求（チェックボックス 1 個）に含まれない
  - **上 2 つは 3b の⚠️3 を受けて追記した。** それまで RunOnce とタスクスケジューラは「却下」ですらなく**無評価**だった
- **`snotra-settings` 側にモジュールを置く**: 却下寄り。`windows` の Registry feature 追加が要り、`RegKeyGuard` 相当の重複が生まれる（`/dry-check` が挙げる形）

## 5. 技術的制約

- **`current_exe()` は `snotra-settings.exe` を返す**。登録するのは兄弟の `snotra.exe`。導出に失敗した場合（exe 不在）はチェックボックスを無効化するか、トグル時にエラー status を出す
- **登録時は既存値があっても現在パスで上書きする**——移動したポータブル版の stale なエントリを自然に治す
- **削除は値不在でも成功扱いにする（冪等）**——`RegDeleteValueW` の `ERROR_FILE_NOT_FOUND` を成功へ畳む。実測で「存在しない値の削除は例外」を確認済み
- **`ADR-no-test-only-injection-in-product-code.md`**: 変異試験・計測のためだけの注入点（env ハッチ等）を製品コードへ足さない。**純粋部と OS I/O 部を割る（`path_env.rs` の先例）のは注入点ではない**——分岐も env 読みも増えないため、この ADR の射程外
- **`cargo test` が実レジストリを触らないこと**。OS I/O は薄く保ち untested にする（`read_user_path` と同じ理由を `//!` へ書く）。**kittest でこのチェックボックスをクリックさせない**——即時適用なので、開発者のローカル `cargo test` が dev ビルドのパスを実スタートアップへ登録してしまう
- **ラベルの混同**: 既存 `CbShowOnStartup`（「起動時にウィンドウを表示」）と直交する別概念。新キーの文言でこれを取り違えさせない
- **`[lints] workspace = true` により `-D warnings`**。新 API を導入して呼び出し点を移行しないと `dead_code` で落ちる → **導入と結線を 1 タスクに束ねる**

## 6. 検証手段の見通し

| 層 | 何を守るか |
|---|---|
| unit test（純粋部） | コマンドライン値の組み立て（引用符付け）・登録済み判定のパス比較 |
| unit test（i18n） | `TrKey` の網羅はコンパイラが強制（テスト不要） |
| `governance:check` | `snotra-core/CLAUDE.md` の索引行と `lib.rs` の `mod` 宣言 |
| 目視（カテゴリ D） | 実際にチェック → レジストリに値が出る → 外す → 消える。タスクマネージャーでの見え方 |
| `/symmetric-check` | 登録/削除の対称ペア（条件別チェック表の該当行） |

`smoke-egui.ps1` / `smoke-startup.ps1` は本体の起動経路であり、この変更の射程外。

## 7. 未解決の疑問（計画の「未確定」へ送る）

1. **タスクマネージャーの「無効化」が `Run` 値を消すか、`StartupApproved` にマークを置くだけか**——チェックボックスの意味論を決める。手元に無効化された標本が無い
2. **NSIS アンインストーラが `Run` 値を残すか**——残るならスコープ内で塞ぐか follow-up issue にするかの判断が要る
3. **`RegKeyGuard` を共有するか重複させるか**——`path_env.rs` 内 private。移設（`snotra-core` の共通 private helper 化）と重複のどちらを採るか
4. **`StartupApproved\Run` のレコード不在がタイミング依存か**（3b の⚠️4 を採用して追加）——プローブは値を作った**直後・同一セッション内**でしか測っていない。Explorer が次のログオン時に記録を作る可能性を否定も肯定もできていない。否定するには explorer.exe の再起動かログオフ/ログオンが要る

## 8. 敵対的調査（3b）の所見と採否

出力の全文は `workspace/adversarial-1210.txt`（324 行）。

### 採用した所見

| # | 重大度 | 所見 | 採否と反映先 |
|---|---|---|---|
| 1 | ❌ | `RegCreateKeyExW` は `Win32_Security` feature が要る（`RegSetValueExW` / `RegDeleteValueW` は不要） | **採用。§3「依存の状況」へ API 別の表を追加。** 機序は windows-0.62.2 のソースを自分で読んで裁定した（`#[cfg(feature = "Win32_Security")]` が `RegCreateKeyExW` にだけ付くこと・引数に `SECURITY_ATTRIBUTES` を取ることを逐語確認）。**結論（feature 追加 0）は生き残る**——`Run` は well-known key なので作成が不要 |
| 2 | ⚠️ | インストール先の例（`C:\Program Files\Snotra\`）が実機と食い違う | **採用。§4 を実測値へ修正。** 引用符が要る**理由**も「インストール先の空白」から「ユーザー名の空白」へ書き直した（結論は同じでも根拠が偽だった） |
| 3 | ⚠️ | RunOnce・タスクスケジューラが「却下」ですらなく無評価 | **採用。§4 の却下一覧へ 2 項追加。** |
| 4 | ⚠️ | `StartupApproved` の実測がタイミング依存かもしれない | **採用。§7 の未解決へ 4 番目として追加。** |
| 5 | ⚠️ | 「黙って登録解除する」は強すぎる（UI 上は可視化される） | **採用。§4 の却下理由 2 を書き直した。** ただし**却下そのものは維持する**——可視化されることは「設定初期化が OS のログオン挙動を変える」筋の悪さを解消しない |
| 6 | ⚠️ | §4 の設計判断が「即時適用が衝突しない理由」であることが本文に無い | **採用。§4「適用タイミング」へ依存関係を明記。** |
| — | 軽微 | `path_env.rs` のテスト可能性 doc の行番号が 262 → 実際は 267 | **採用したうえで、行番号引用そのものをやめた**（行番号は編集のたびに腐る）。§3 の表はシンボル名で指す形へ書き換えた |

### 壊せなかった項目（3b の✅・こちらも宣言する）

- `externalBin` 同梱でも `snotra.exe` と `snotra-settings.exe` は同一ディレクトリに同居する（実機で確認。**兄弟導出の前提は生きている**）
- **この機体に Snotra は実際にインストールされている**（`winget list` で v0.19.0）——§2 で「`Run` に Snotra が居ない」と読んだのが「未インストールだから」ではなく「未登録だから」であることの裏づけ。**この確認は §2 の暗黙の前提を初めて接地させたものであり、壊せなかった項目のうち最も価値が高い**
- Backup import が `saved` を丸ごと差し替える（`app.rs` 逐語確認）
- 却下理由 3（config ↔ レジストリ非調停）・4（新規 config キーの面が消える）
- `RegKeyGuard` の private 性・`read_user_path` の形・`CbShowOnStartup` の実在と文言・`TrKey` 網羅強制の機序・SPEC §7.2 / §7.3 / §13.1 の実在・各 ADR の実在・smoke script が射程外であること・workspace lints の存在

### 3b が触らなかった面（射程の宣言）

- NSIS **アンインストーラ**が `Run` 値を残すかは 3b も測っていない（§7 の未解決 2 のまま）
- 実機での**チェック → 登録 → 再ログオンで実際に起動する**ところまでは誰も測っていない（カテゴリ D の目視で潰す）
