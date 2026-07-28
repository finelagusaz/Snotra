# #803 独立導出 — `SNOTRA_CONFIG_DIR` の env 上書き

issue #803 とコードベースだけから導出した必要変更集合。`workspace/plan.md` /
`workspace/research.md` は読んでいない（例外は §5 に記す）。列挙はすべて grep / `git ls-files`
の全件出力に対して行い、`head` / `Select-Object -First` で切っていない。

---

## 1. 変更が必要なファイル + シンボルの完全な列挙

### 1.1 実装（`snotra-core`）

| # | 対象 | 根拠 | 変更内容 |
|---|---|---|---|
| 1 | `snotra-core/src/config.rs:656-658` `Config::config_dir()` | 唯一の `dirs::config_dir()` 呼び出し点 | env 上書きを 1 本入れる |
| 2 | `snotra-core/src/config.rs`（新規）`ENV_CONFIG_DIR` 定数 | — | `"SNOTRA_CONFIG_DIR"` の文字列を 1 か所に固定（テスト・doc がここを指す） |
| 3 | `snotra-core/src/config.rs`（新規）`config_dir_from(over, base)` | — | env 読みと合成を分離した純粋関数。テスト可能性の唯一の担保（→ §3） |
| 4 | `snotra-core/src/config.rs:1` `//!` module doc | 「`%APPDATA%\Snotra\config.toml` の読込・保存」 | **無条件の全称表現**。`既定は` を足す |
| 5 | `snotra-core/src/config.rs:656` / `:660` の `///` | 現状 doc コメント無し | 上書きの意味論を明記（下記「決めるべき意味論」） |

**決めるべき意味論（doc に書く。書かないと呼び出し側が推測する）**

- 上書き値は**そのまま**使い、`Snotra` を join しない（`dirs::config_dir()` 側だけが join する）
- **空文字は未設定扱い**。`PathBuf::from("")` は相対パス＝CWD になり、ユーザーの
  カレントディレクトリに `config.toml` / `*.bin` を撒く。**この分岐を落とすと沈黙して壊れる**
- 存在確認はしない（`save_to_dir` の `create_dir_all` が作る。`load` の read 失敗は既存の
  `NotFound` = first-run 経路が受ける）
- **毎回 `var_os` を読む**（キャッシュしない）。プロセス内で誰も `set_var` しない前提では
  値は不変であり、`Config::config_dir()` が 14 箇所・複数スレッドから呼ばれることと整合する
- **`src-tauri/src/trace.rs:18` の `env_flag` は使えない**（レビューで必ず問われる）。
  あれは env **フラグ**（真偽）の受理仕様の SSOT であって値を返さず、しかも `src-tauri` に在る
  ——`snotra-core` は `src-tauri` に依存できない（依存方向が逆）。ゆえに空文字の扱いは
  ここで独自に決める必要があり、それが §3 の「空文字」テストが要る理由でもある

### 1.2 呼び出し側（**変更不要**・自動追従することの確認）

`Config::config_dir()` の呼び出しは env 上書きに自動追従する。実測の全件は **14 箇所**
（`snotra-core` 12 / `snotra-settings` 2）。**issue 本文の「13 箇所（core 11 / settings 2）」とは
core 側で 1 件差がある** — issue の数を写さずに数え直した結果であり、どちらが正しいかは
下の一覧を数えれば決まる（`AGENTS.md`「派生コピー同士の一致を完全性の証拠にしない」）:

- `snotra-core/src/config.rs:661`（`config_path`）/ `:863`（`load_reporting`）/ `:946`（`save`）
- `snotra-core/src/binfmt.rs:30`（`BinFile::new`）
- `snotra-core/src/history.rs:86` / `:154` / `:183`
- `snotra-core/src/indexer.rs:396` / `:463` / `:590`
- `snotra-core/src/window_data.rs:62` / `:86`
- `snotra-settings/src/tabs/backup.rs:104`（表示）/ `:110`（フォルダを開く）
  — `backup.rs:237` は `Config::config_path()` 経由

**`icons.bin` も含めて全部ここから導かれる**（`binfmt.rs:18` の doc が明言）。

### 1.3 ドキュメント（全称表現が偽になる箇所）

| # | 対象 | 現在の記述 |
|---|---|---|
| 6 | `SPEC.md:603` §13.1 | `%APPDATA%\Snotra\config.toml`（TOML） |
| 7 | `SPEC.md:615-618` §13.2 | `index.bin` / `icons.bin` / `history.bin` / `window.bin` の 4 行 |
| 8 | `SPEC.md:211` | 「バイナリ形式で `%APPDATA%\Snotra\` に保存」（**§13 とは別セクション**。見落としやすい） |
| 9 | `SPEC.md:637` §13.3 | 「設定フォルダを開く」: `%APPDATA%\Snotra` をエクスプローラーで開く |
| 10 | `docs/architecture.md:104` | 「`%APPDATA%\Snotra\` にファイル分割保存」 |
| 11 | `src-tauri/src/icon.rs:178` `///` | 「`remove()` は `%APPDATA%` へのローカル `remove_file` 1 回で、lock 内 I/O として軽量」 — **rustdoc の性能根拠**。上書き先がネットワークドライブなら偽になる |
| 12 | `snotra-core/tests/memory_footprint.rs:11` `//!` | 「実運用点（`%APPDATA%\Snotra\index.bin` の実インデックス）」 |
| 13 | `docs/build-commands.md:63-76` `check:colors` 節 | 「実 config を退避して書き換える」「異常終了時は `config.toml.visualcheck-bak`」「`-Restore` で回収」「二重退避は明示エラー」——**節ごと書き換え**。69 行の `-- -Restore` コマンド例も消す |

**最小で済ませる形**: 6〜10・12 は各所を書き換えるより「既定は `%APPDATA%\Snotra\`。
`SNOTRA_CONFIG_DIR` で上書きできる」を **SPEC.md §13 の冒頭に 1 行**置き、他は
それを参照する（`AGENTS.md`「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」）。
ただし §13.1/§13.2 の各行に `%APPDATA%` が**リテラルで**書かれている以上、
「既定は」の語を足さないと行単位では依然偽である。11 は独立した性能主張なので別途直す。

### 1.4 ドキュメント（env ハッチの記載先）

| # | 対象 | 変更内容 |
|---|---|---|
| 14 | `docs/build-commands.md` | `SNOTRA_CONFIG_DIR` を載せる。既存の env ハッチ表（`:82-91` の updater 節）と並べるか、`check:colors` 節に書くか |

**検証した否定**: このリポジトリに **env 変数の中央索引は無い**。`SNOTRA_TRACE` /
`SNOTRA_EGUI_*` / `SNOTRA_ICON_DIAG_PATHS` / `SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE` は
それぞれの使用箇所と `docs/build-commands.md` に散在して書かれているだけで、
双方向照合する検査（governance-check の G1〜G12）は存在しない。
**ゆえに同期先は `docs/build-commands.md` 以外に無い**（＝索引更新漏れの心配は無いが、
逆に「書き忘れても誰も止めない」）。

### 1.5 スクリプト

| # | 対象 | 変更内容 |
|---|---|---|
| 15 | `scripts/visual-check-colors.ps1` | 下記の retire / 新設 |

**retire する 5 つ**（issue の主張どおり。ただし §2.6 の但し書きを見よ）:

- `:105` `Copy-Item` の退避、`:230-236` `finally` の `Restore-SnotraConfig`
- `:91-94` 二重退避の明示 throw、`$backupPath`（`:61`）
- `:56` `-Restore` パラメータ、`:64-71` `Restore-SnotraConfig` 関数、`:73-76` の早期 return
- `:114-131` `Set-TomlKey`（**既存 TOML を書き換える**ための関数。新規に完全な TOML を書けば不要）
- `:60` `$configPath = Join-Path $env:APPDATA 'Snotra\config.toml'`
- doc ブロック: `.DESCRIPTION`（`:11-12`）・`.PARAMETER Restore`（`:36-37`）・`.EXAMPLE -Restore`（`:48-49`）

**残すもの**:

- `:99-102` 単一インスタンスの起動前チェック（**env 上書きは single-instance の識別子を
  変えない**。issue の「正直なコスト 1」。消すと空振りが沈黙する）
- `:78-86` hex パース、`:156-229` キャプチャ・判定、`-KeepShot` / `$shotDir`

**新設する 1 つ**: 検証プロファイルに**有効な `config.toml` を書いてから** env 付きで起動する。
理由は §2.3（first-run のフォーカス奪取）。

**置き場所は `target/visual-check/profile` にする**（`$env:TEMP` ではない）。
スクリプトは既に `$shotDir = Join-Path $PSScriptRoot '..\target\visual-check'`（`:62`）を
使っており、その隣に置けば **`cargo clean` が `config.toml` と 4 つの `*.bin` を掃く**。
新しい後始末機構を足さずに §2.6 の残余へ掃除経路が付く。

書く内容は必須セクションを満たすこと:

```toml
[hotkey]
modifier = "Alt"
key = "Q"

[appearance]
window_width = 600

[visual]
background_color = "<検証色>"

[general]
show_on_startup = true

[paths]
```

- `[hotkey]` / `[appearance]` / `[paths]` は `#[serde(default)]` を持たない必須セクション
  （`config.rs:88`/`:91`/`:94`）。**空 TOML は parse 失敗し破損復旧経路を踏む**
  （`smoke-egui.ps1:78-82` が実証済み）
- `[paths]` を**空ヘッダで置き `[[paths.scan]]` を書かない** → `PathsConfig.scan` は
  `#[serde(default)]`（`config.rs:458`）なので空 Vec になる。**`Config::default_scan_paths()`
  には落ちない**（default は `Config::default()` 経由でしか使われず、TOML の欠落は
  serde の `Default` で埋まる）。index 構築が即終了し、**issue が挙げた「正直なコスト 2」
  （新プロファイルは index を作り直す）が消える**
- `apply_migrations()` が `false` を返す形に揃える（legacy キーを書かない）→ `load` が
  書き戻さない

| # | 対象 | 変更 |
|---|---|---|
| 16 | `package.json` | **変更不要**（`check:colors` の script 定義自体は不変）。`-Restore` は script ではなく引数 |

### 1.6 同名・別概念（**触ってはならない**・grep の母集団に混ざる）

| 対象 | なぜ触らないか |
|---|---|
| `src-tauri/src/config_watcher.rs:29` `let config_dir = config_path.parent()?;` | **ローカル変数**。`Config::config_path()` から導かれるので env に自動追従する。改名も不要 |
| `snotra-core/src/config.rs:634` `dirs::desktop_dir()` | 既定スキャンパスの導出。config の**保存先**ではない |
| `snotra-core/src/opener.rs:295` `std::env::var("LOCALAPPDATA")` | opener プリセット検出（VSCode 等の実在確認）。別概念 |
| `SPEC.md:943-944` `args = "--env %APPDATA%"` / `%APPDATA% = "C:\a b"` | instant command の**変数展開の例**。`%APPDATA%` はここでは「展開されるトークン」であって保存先ではない |
| `scripts/governance-check.test.mjs:194` `"`%APPDATA%/Snotra/icons.bin`"` | G3 の「ランタイム生成物は参照実在検査の対象外」を検証する**fixture 文字列**。実在ファイルを指していない |

---

## 2. 見落とされやすいと考える箇所

### 2.1 間接参照は Rust 側に 1 本も無い（実測）

`rg 'dirs::'` の全件 = `snotra-core/Cargo.toml:15`（依存宣言）/ `config.rs:634`（`desktop_dir`）/
`config.rs:657`。`%APPDATA%` を Rust コードで組み立てる箇所は **0 件**
（`opener.rs:295` は `LOCALAPPDATA` で別概念）。
**issue の「絞り点は 1 つ」は裏が取れている。**

### 2.2 間接参照は PowerShell 側に 3 本ある（コンパイラもテストも捕まえない）

| 箇所 | 内容 |
|---|---|
| `scripts/visual-check-colors.ps1:60` | 本 issue の対象。消える |
| `scripts/smoke-egui.ps1:66-67` | `Join-Path $env:APPDATA "Snotra"` で seed 先を組む |
| `scripts/smoke-egui.ps1:125` | throw メッセージ中の config path 表示 |
| `scripts/smoke-egui.ps1:477` | skip NOTE の文言 `%APPDATA%/Snotra/config.toml` |

**これが本変更で新設される検出器なしの不整合である**: env 上書きを使う経路が増えると、
「スクリプトが `$env:APPDATA\Snotra` を見て config の有無を判定する」場所と
「アプリが実際に読む場所」が食い違いうる。`smoke-egui.ps1` を env 化すれば
`-SeedConfig` の「既存を上書きしない」制約・`-RequireResults`（#686）・
`.github/workflows/e2e.yml:67-78` のステップ順序制約が**まるごと不要になる**が、
これは #803 のスコープ外（issue は「副次的な用途」として挙げるだけ）。
**同 PR に入れないなら、`docs/build-commands.md:147` の順序制約が引き続き有効であることを
残余として明記する**。

### 2.3 first-run のフォーカス奪取（検出器なし・判定を静かに壊す）

新しい `SNOTRA_CONFIG_DIR` に config が無い状態で起動すると:

`Config::is_first_run()`（`config.rs:649`）→ `main.rs:331-334` `setup_first_run` →
`commands::launch_settings_process(app_handle, &["--first-run"])` が
**`snotra-settings.exe` を spawn しフォーカスを奪う**。

`docs/superpowers/specs/2026-07-24-su7-flip-implementation-design.md:36` が
まさにこれで観測が壊れた事例を記録している。
`visual-check-colors.ps1` の判定は `FindWindowW(null, 'Snotra')` + `CopyFromScreen`
（スクリーン座標からの取得）なので、**設定窓が前面に来ると `(3, h/2)` は別の窓のピクセルを
読む**。コンパイラもテストも trace も捕まえない。

→ **env を立てるだけでは不十分で、起動前に config を書くのが必須**（§1.5 の「新設する 1 つ」）。

### 2.4 子プロセスへの env 継承は暗黙の依存

`src-tauri/src/commands/window.rs:75` の `Command::new(&settings_exe).args(...).spawn()` は
env を明示せず、Rust の既定で**親の環境を継承する**。ゆえに `snotra-settings` も同じ
上書き dir を見る（これは望ましい挙動）。
ただし `snotra-settings/CLAUDE.md`「本体との連携は `config.toml` ファイル1点のみ」という
記述には、**「同一 dir を見るのは env 継承ゆえ」という前提が新たに乗る**。
将来 `.env_clear()` / `.env_remove()` を足すと沈黙して壊れる。

### 2.5 `config_watcher` が黙って起動しない経路

`config_watcher::start`（`config_watcher.rs:59`）は
`watcher.watch(config_dir, RecursiveMode::NonRecursive).ok()?` で、
**dir が存在しなければ `None` を返して監視なしで起動する**（`main.rs:466` の `if let Some`）。
起動順序上は `Config::load` が setup より前に走り、first-run 経路の `save_to_dir` が
`create_dir_all` するので通常は dir が在る。
ただし `LoadOutcome::ReadFailed`（権限・ロック）では保存しないため、
**上書き dir が読めない状態だと監視なしで静かに起動する**。既存経路と同型の残余だが、
env で任意のパスを指せるようになると踏む確率が上がる。

### 2.6 「5 つが全部消える」は正確でない（自分の主張の全称表現）

retire されるのは §1.5 の 5 つで正しい。しかし:

- **1 つの setup が増える**（temp プロファイルへ config を書く）
- **後始末は消えるのではなく、対象が変わる**: 検証プロファイルの `config.toml` +
  `index.bin` / `icons.bin` / `history.bin` / `window.bin` が残る

残るのは**ユーザー資産ではない**。さらに `target/visual-check/profile` に置けば
`cargo clean` が掃くので、**掃除経路はある**（機構は足さない）。
それでも「後始末がゼロになる」と書くと偽なので、
**「5 → setup 1 + `cargo clean` が掃く残余」**と書くべき。

### 2.7 single-instance は「別プロファイルで並行起動できる」を意味しない

`tauri_plugin_single_instance` は config dir と無関係。env 上書きは**識別子を変えない**ので、
「プロファイルごとに 1 インスタンス」にはならない。
**「検証用プロファイルと実プロファイルの分離ができる」と無条件に書くと偽**である
（分離できるのは*データ*であって*同時起動*ではない）。issue 自身が「正直なコスト 1」で
認めているが、SPEC / docs に書くときに落ちやすい。

### 2.8 governance-check への影響（実測: なし）

G12（config フィールドの到達性・`governance-check.mjs:936`）の母集団は
`Deserialize` を derive する `pub struct` のフィールドだけ（`:874-881` のコメントが明言）。
新設する `ENV_CONFIG_DIR` const と `config_dir_from` fn は母集団に入らない。
`productionOnly` は `^#[cfg(test)]` で切るので、新規テストを既存 `mod tests`
（`config.rs:1142`）内に置けば影響なし。
G5（npm script 実在）も `check:colors` の定義が残るので影響なし。
**ただし SPEC.md / docs を触るので G3・G4・G10・G11 は走らせて確認する**（→ §4）。

### 2.9 触れる必要が無いことの確認（実測）

- `README.md` / `README.en.md` / `CONTRIBUTING.md` — `%APPDATA%` / `config.toml` の言及は **0 件**
- `.github/workflows/*.yml` — `$env:APPDATA` / `Snotra\config` の組み立ては **0 件**
  （`e2e.yml:67-78` は順序制約のコメントで、パスを組んでいない）
- `docs/superpowers/plans/` / `specs/` — 歴史資料（governance-check G3/G4 の対象外・#589）。
  `2026-07-25-pr-a-...md` に `$env:APPDATA "Snotra/config.toml"` が 4 箇所あるが**当時の記録**

---

## 3. テスト方針についての結論

**結論: 環境変数を読む関数そのものをテストしない。env の読みと合成を分離し、純粋関数だけをテストする。**

### 根拠

1. **`snotra-core` は `edition = "2024"`**（`snotra-core/Cargo.toml:4`）。
   `std::env::set_var` / `remove_var` は `unsafe`。安全条件は「他スレッドが env に触っていないこと」。
2. **`cargo test` は既定でスレッド並列**であり、この crate には env を**読む**製品コードが実在する
   （`config.rs:621` の `ProgramData`、`opener.rs:295` の `LOCALAPPDATA`）。
   同一プロセス内でこれらを読むテストと並走した set_var は UB。理論上の危険ではない。
3. **`--test-threads=1` を要求できない**。`docs/build-commands.md` カテゴリ A と
   `.claude/hooks/post-edit.mjs` の `selectChecks` が `cargo test -p snotra-core` を素で走らせる。
   引数を要求するテストは hook・CI 双方から外れる。
4. `serial_test` 等の dev-dependency 追加は、関数 1 本のために依存を増やす。

### 置くテスト（`config.rs` の既存 `mod tests` 内に 4 本）

| テスト | 何を固定するか |
|---|---|
| 上書きあり → **そのまま**返る | `Snotra` を join し**ない**こと（付けると既存プロファイルの二重ネストになる） |
| 上書きなし → `base.join("Snotra")` | **既定の保存先が変わらない**回帰ガード。既存ユーザーの移行が発生しない証拠 |
| 上書きが空文字 → base へ落ちる | `PathBuf::from("")` = CWD 流出の防止。**この境界を落とすと沈黙して壊れる** |
| 上書きなし + `base = None` → `None` | `load_reporting`（`config.rs:862-867`）の early-return 契約を保つ |

### 受容する残余（明記すべき）

`config_dir()` 本体（`var_os` を読んで `config_dir_from` へ渡す 1 行）が
**実際に `config_dir_from` を呼んでいることは、型でもテストでも保証できない**。
唯一の検証は §4 の実機実行。

### 既存の永続テストは変更不要

`load_from_dir_reporting`（`:875`）/ `save_to_dir`（`:951`）は既に
「`config_dir` を注入可能にし統合テストする」ために在り、テスト群（`:2891` 以降）も
dir を渡す形。**env 上書きはその注入点をプロセス外へ延ばすだけ**なので、
永続経路の後方互換テストは要らない。

### `/persistence-check` は発火しない（1 行の判断）

永続**形式**（TOML の構造・`*.bin` の magic + version・キー正規化）は一切変わらず、
変わるのは**置き場所**だけ。`AGENTS.md` の条件別チェック表のトリガーは
「永続形式・識別子/キー形式を変更」なので当たらない。version バンプも移行も不要。

### `/race-check` も発火しない

env はプロセス内で誰も書かない前提で不変。`Config::config_dir()` が 13 箇所・
複数スレッドから呼ばれても値は同一。**この前提を壊すのが env を書くテストであり、
それを書かないという §3 の結論と同じ根拠に立つ**。

---

## 4. 実行すべき検証コマンド（カテゴリ A〜F）

### A（`*.rs` を変更）— 全部必須

```
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core
cargo doc --workspace --no-deps --document-private-items
```

`cargo doc` は **`//!`（`config.rs:1`）と `///` を触るので必須**。
PostToolUse hook は `cargo doc` を発火しない（`docs/build-commands.md:26`）ため、
**この 1 本だけは沈黙が合格を意味しない**。

### F（`SPEC.md` / `docs/*.md` を変更）— 必須

```
npm run governance:check
```

G3（参照実在）・G4（SPEC 番号）・G10（恒久規範の面積 ratchet）・G11（見出し参照の着地）に当たる。
特に **G10 は SPEC.md への加筆が面積予算に触れうる**ので、実行で判定する。

### D（UI 目視）— 該当する

```
npm run check:colors                       # 書き換えたスクリプトが緑になること
npm run check:colors -- -Color '#FFF'      # 3 桁 hex 経路の回帰
cargo run -p snotra                        # env を立てない通常起動（既定経路の回帰）
```

**`npm run check:colors` の実行そのものが本変更の唯一の受け入れテストである**
（スクリプトを書き換えるので、スクリプトが動くこと以外に検証手段が無い）。
**エージェントは実行できない**（実機 GUI）ので人間に依頼する。

追加の手動確認（検出器がない部分・すべて §2 に対応）:

- 実行後に **`%APPDATA%\Snotra\config.toml` の更新時刻と内容が変わっていないこと**
  （＝実 config を壊していないことの直接の証拠）
- **設定窓が前面に来ないこと**（§2.3 の first-run 経路を踏んでいない証拠）
- `$env:SNOTRA_CONFIG_DIR` を立てて `cargo run -p snotra-settings` を起動し、
  「設定フォルダを開く」（`backup.rs:110`）と表示 dir（`:104`）が上書き先を指すこと

### B / C / E — 非該当

- B: `.ts` を触らない
- C: ウィンドウ生成・表示順・ホットキー・スラッシュコマンドに触れない。
  `smoke:startup` / `smoke:egui` は本変更で挙動が変わらない（env 未設定なので既定経路）
- E: `.githooks/**` を触らない

### CI 上の注意（規約）

**`skip-ci` ラベルを貼ってはならない** — `scripts/**` を変更するため
（`docs/build-commands.md:175`）。`npm test` は `vitest.config.ts` の include に
`scripts` を含むが、`visual-check-colors.ps1` に対応する `*.test.mjs` は存在しない
（`scripts/` のテストは `governance-check.test.mjs` と `clean-worktrees.test.mjs` の 2 本のみ）
ので実質 no-op。それでも `npm test` は走らせる。

---

## 5. 未検証（理由）

1. **`workspace/` の非読取が完全ではない。** 最初の 2 回の grep（`config_dir` /
   `APPDATA|AppData|appdata`、いずれもリポジトリ全体が対象）の出力に
   `workspace/plan.md` の 14 行と `workspace/research.md` の 6 行が混ざって表示された。
   ファイルを開いてはいないが、視界には入った。3 回目以降は `!workspace/**` で除外した。
   **§3 のテスト方針は `snotra-core/Cargo.toml` の `edition = "2024"` と
   `config.rs:621` / `opener.rs:295` の env 読みを自分で確認して導いたもの**だが、
   独立性は完全ではないので開示する。

2. **実機での動作は未確認。** 本作業は読み取り専用の調査であり、
   `cargo` も `npm run check:colors` も実行していない。§4 の D はすべて未実施。

3. **G10（恒久規範の面積 ratchet）が `SPEC.md` を対象に含むかを未確認。**
   `governance-check.mjs:526` 付近の実装本体（`checkNormativeAreaBudget`）は読んでいない。
   SPEC.md への加筆が予算に触れるかは `npm run governance:check` の実行で判定する。

4. **`Config::load` → `setup_config_watcher` の起動順序は「setup が load 済みの
   `is_first_run` / `load_outcome` を受け取る」ことから推論した**（`main.rs:275` /
   `:465-470`）。`main` 関数冒頭で `Config::load_reporting()` を呼んでいる行そのものは
   読んでいない。§2.5 の「通常は dir が在る」はこの推論に依る。

5. ~~`docs/superpowers/specs/2026-07-28-config-background-color-design.md` の保存先記述~~
   → **閉じた**。`APPDATA|AppData|appdata`（`-i`・`head_limit 0`・リポジトリ全体）の全件出力に
   このファイルは現れないので、`%APPDATA%` 形の保存先記述は**無い**。
   `docs/superpowers/` 配下の他の言及（`2026-07-25-pr-a-...md` 等）は歴史資料であり
   governance-check G3/G4 の対象外（#589）ゆえ更新しない。

6. **`icons.bin` を含む `*.bin` 群が上書き dir で再生成されるときの実挙動は未確認。**
   `binfmt.rs:30` が `Config::config_dir()` から導くことはコードで確認したが、
   存在しない dir に対する `BinFile::load` の失敗経路が「静かに再生成」で済むかは
   実行していない（`SPEC.md:624`「読み込み失敗時は当該ファイルのみ再生成」に依拠）。
