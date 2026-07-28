# plan — issue #803: `SNOTRA_CONFIG_DIR` の env 上書き

`Config::config_dir()` に env 上書きを入れ、`scripts/visual-check-colors.ps1` を
「実 config を壊して戻す」形から「使い捨てプロファイルを指して起動する」形へ移す。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `snotra-core/src/config.rs` | `ENV_CONFIG_DIR` 定数、純粋関数 `config_dir_from`、`config_dir()` を薄いラッパへ。ユニットテスト 4 本 |
| `scripts/visual-check-colors.ps1` | 退避/復元・`-Restore`・二重退避エラー・実 config への `show_on_startup` 書き込みを撤去し、`SNOTRA_CONFIG_DIR` を指す seed 済みプロファイルで起動する形へ |
| `docs/build-commands.md` | `-Restore` の行を削除、「実 config を退避して書き換える」bullet をプロファイル分離の説明へ差し替え、`SNOTRA_CONFIG_DIR` を env として記載 |
| `SPEC.md` §13 冒頭 + §13.1 | 保存先が `%APPDATA%\Snotra` **固定である**という全称表現を、**文書全体へのスコープ宣言 1 つ**で直す（下記「SPEC.md 更新要否」） |
| `docs/architecture.md:104` | 別文書ゆえ SPEC のスコープ宣言が届かない。参照を 1 つ足す |
| `src-tauri/src/icon.rs:178` `///` | 「`%APPDATA%` への**ローカル** `remove_file`」という性能根拠に「既定では」を足す |
| `src-tauri/src/commands/window.rs` `launch_settings_process` の `///` | 「子は env を継承するので同じプロファイルを見る」を 1 行（`.env_clear()` を将来足すと沈黙して壊れる） |
| `snotra-core/CLAUDE.md` | `config.rs` の節に env 上書きの不変条件（既定と上書きの導出の非対称）を 1 行 |
| `package.json` | 変更なし（`check:colors` の script 定義は不変。`-Restore` は script ではなく引数） |

## 実装順序

### Phase 1 — `snotra-core` の env seam（Red → Green）

- [x] `config.rs` に失敗するテストを 4 本先に書く（Red を確認する）
- [x] `const ENV_CONFIG_DIR: &str = "SNOTRA_CONFIG_DIR";` を追加する（**private**。読む消費者は `config_dir()` 1 つだけで、外部 crate から名前で参照する予定は無い）
- [x] 純粋関数 `fn config_dir_from(override_dir: Option<OsString>, base: Option<PathBuf>) -> Option<PathBuf>` を追加する
- [x] `config_dir()` を `Self::config_dir_from(std::env::var_os(ENV_CONFIG_DIR), dirs::config_dir())` の 1 行へ書き換える
- [x] `config_dir` / `config_dir_from` の `///` に「上書きは**そのまま**使い、`Snotra` を付けない」「空は未設定扱い」「**展開も絶対化もしない**（`%VAR%` は展開されず、相対パスは CWD 起点になる）」を書く
- [x] `cargo test -p snotra-core` が緑（Green）

形（`config.rs`）:

```rust
/// 上書きは**そのまま**使う（`Snotra` を付け足さない）。既定側だけが `Snotra` を足す
/// この非対称は意図的である——検証スクリプトが渡した temp パスの下に更に階層を作らせないため。
/// 空文字は「未設定」として扱う（`PathBuf::from("")` は相対パス = CWD に config.toml を落とす）。
fn config_dir_from(override_dir: Option<OsString>, base: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = override_dir
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    base.map(|p| p.join("Snotra"))
}
```

**なぜ相対パス・未展開 `%VAR%` を拒否しないか（却下した代替案）**: 「絶対パスでなければ既定へ
落とす」は fail-safe に見えて**この用途では危険な向き**である——検証スクリプトがパスを書き損じた
とき、既定へ落ちれば**ユーザーの実 config を触る**。それはこの issue が消そうとしている当のものだ。
そのまま使えば、書き損じても CWD 配下の変な場所へ隔離されるだけで実データには届かない。
**フォールバックの向きは「安全そうな方」ではなく「壊れたときに何を守るか」で決める。**
（上の `config_dir_from_does_not_expand_or_absolutize_override` がこの選択を固定する）

**なぜ純粋関数へ割るか**: env は プロセス全域の可変状態で、`cargo test` は同一プロセスの複数
スレッドで並列実行する。`set_var` するテストは他のテストへ漏れる（Rust 2024 では `unsafe` でもある）。
判定そのものを引数で受ければ、env に触れずに全分岐を測れる（research C4）。

### Phase 2 — `scripts/visual-check-colors.ps1` の書き換え

- [x] `-Restore` パラメータ（`:55`）・早期 return（`:73-76`）・`Restore-SnotraConfig` 関数（`:64-71`）を削除する
- [x] 退避（`:105` `Copy-Item`）・`$backupPath`（`:61`）・二重退避エラー（`:91-94`）・`finally` の復元（`:235`）を削除する
- [x] `$configPath`（`:60`・`$env:APPDATA` を組む唯一の行）と、実 config の存在ガード（`:88-90`）を削除する
- [x] `Set-TomlKey`（`:114-131`）を削除する（既存 TOML を書き換える関数。完全な TOML を新規に書くので不要）
- [x] プロファイル **`target/visual-check/profile`** を作り、そこへ最小の有効 TOML を seed する
- [x] seed 前にプロファイル配下の `config.toml.bak` **と `*.bin` を削除する**（プロファイルは実行間で再利用するため、**前回の残骸が残っていると下の 2 つの判定がどちらも空振りで合格する**）
- [x] `$env:SNOTRA_CONFIG_DIR` を設定してから `cargo run -p snotra` する（`-Interactive` / 自動判定の両経路）
- [x] **seed の健全性を判定に組み込む**: 本体の stderr を `-RedirectStandardError` でファイルへ取り、`[config] ` で始まる行が 1 つも無いことを確認する（現れたら赤）。**`config.toml.bak` の不在では証明にならない**——`backup_invalid` の `fs::rename` が失敗すると `.bak` は作られないまま `RecoveredFromCorrupt` を返す（`config.rs:938`・codex の敵対レビューが指摘し実測で確認）。`[config] ` の eprintln は**失敗 4 arm すべてに在り、成功時には出ない**（`config.rs:892`, `:908`, `:918`, `:934-941`）ので、これが唯一の健全な観測点である
- [x] **env が効いたことの肯定的証拠を取る**: 実行後にプロファイル配下へ `*.bin` が生成されていることを確認する。**どのファイルが確実に出るかは実装時に実測して決める**——本体は `Stop-Process -Force` で殺すので、正常終了で書かれるもの（`window.bin` の `on_exit`・履歴 flush）は出ない。`[paths]` が空で索引 0 件のとき `index.bin` が書かれるかも自明でない。**どれも確実に出ないなら、この判定は置かずにその事実をスクリプトのコメントへ書く**（在るとは限らないファイルに賭けない）
- [x] 単一インスタンスの起動前チェック（`:99-102`）は**残し**、コメントに「プロファイルを分けても single-instance の識別子は変わらない」理由を書く
- [x] `finally` は本体プロセスの kill だけを残す
- [x] `.SYNOPSIS` / `.DESCRIPTION` / `.PARAMETER Restore`（`:36-37`）/ `.EXAMPLE -Restore`（`:48-49`）を新しい形へ更新する
- [x] **【実装中に発見・計画外】`FindWindowW($null, ...)` を `[NullString]::Value` へ直す**。PowerShell は `$null` を `[string]` 引数へ渡すとき**空文字へ変換する**ため `FindWindowW("", "Snotra")` になり、クラス名 `""` に一致せず常に 0 を返していた。**PR #802 で追加されて以来、自動判定は一度も動いていない**（毎回 300 秒待って throw していた。前サイクルの緑は `-Interactive` の目視によるもの）。実測: 同一プロセス・同一時刻に `$null` → `0` / `[NullString]::Value` → `1509320`

seed する TOML（`smoke-egui.ps1:86-100` と同型・必須 3 セクションを含む）:

```toml
[hotkey]
modifier = "Alt"
key = "Q"

[appearance]
window_width = 600

[general]
show_on_startup = true   # 自動判定のときだけ。-Interactive では hotkey で出す

[visual]
background_color = "<Color>"

[paths]
```

**この TOML は parse できることを実測済み**（2026-07-28・`Config` として `toml::from_str` が成功し、
`general.show_on_startup = true` / `visual.background_color = "#4A2B5C"` が載ることを確認した。
`AGENTS.md`「計画に書いた…テスト fixture は、実装前に代表入力で実行して測る」）。

**`[paths]` は空ヘッダで置き `[[paths.scan]]` を書かない。** `PathsConfig.scan` は
`#[serde(default)]` なので空 Vec になり、`Config::default_scan_paths()` には落ちない
（default は `Config::default()` 経由でしか使われない）。索引構築が即終了するので、
issue が「正直なコスト 2（新プロファイルは index を作り直す）」として挙げた費用は**消える**。

**プロファイルの置き場所は `target/visual-check/profile`**（`$env:TEMP` ではない）。
スクリプトは既に `$shotDir = ..\target\visual-check`（`:62`）を使っており、その隣に置けば
**`cargo clean` が config.toml と `*.bin` を掃く**——新しい後始末機構を足さずに掃除経路が付く。

**なぜ seed するか（first-run に任せない）**: 空のプロファイルは `is_first_run` を真にし、
`setup_first_run` が `snotra-settings --first-run` を spawn してフォーカスを奪う。
`auto_hide_on_focus_lost` の既定は `true` なので main 窓が隠れ、**自動判定は何も映っていない場所を
撮る**（research C2）。

**なぜ `smoke-egui.ps1` と seed を共有しないか（却下した代替 2 案）**:

1. **共有ヘルパー（`scripts/lib/*.ps1` を dot-source）** — `smoke-egui.ps1` は e2e.yml の
   `-RequireResults` ゲートに載る CI 経路であり、本 issue と無関係な理由でそこを触るリスクを負う。
   scripts/ に共有ライブラリの下地も無い（現状は各スクリプトが自己完結）
2. **`Config` の `hotkey` / `appearance` / `paths` に `#[serde(default)]` を付けて seed 自体を不要にする**
   — 必須セクション欠落が「破損復旧経路」を踏む現在の挙動は、`config.toml` の部分的な破損を
   黙って既定値で塗り潰さないための設計である（`snotra-core/CLAUDE.md`「読み込み失敗は種類で扱いを
   分ける」）。検証スクリプトの都合で製品のデータ保全方針を緩めない

→ **重複させたうえで、両方の seed に相互参照コメントを置く**（片方だけ直る事故を防ぐ）。

### Phase 3 — 文書の同期

- [x] `SPEC.md` §13 の冒頭に**文書全体へのスコープ宣言**を 1 つ置く（下記「SPEC.md 更新要否」）
- [x] `docs/architecture.md:104` に SPEC §13 への参照を足す（別文書ゆえスコープ宣言が届かない）
- [x] `src-tauri/src/icon.rs:178` の `///`「`%APPDATA%` への**ローカル**」に「既定では」を足す
- [x] `src-tauri/src/commands/window.rs` の `launch_settings_process` の `///` に env 継承の依存を 1 行足す
- [x] `docs/build-commands.md` の `check:colors` ブロックから `-Restore` の行を削除する
- [x] `docs/build-commands.md` の「実 config を退避して書き換える」bullet を、プロファイル分離の説明へ差し替える
- [x] `docs/build-commands.md` に `SNOTRA_CONFIG_DIR` の説明を置く（env ハッチとして）
- [x] `snotra-core/CLAUDE.md` の `config.rs` 節に、上書きの非対称（`Snotra` を付けない）と空文字の扱いを 1 行足す
- [x] `scripts/smoke-egui.ps1` の seed に、`visual-check-colors.ps1` への相互参照コメントを 1 行足す
- [x] `docs/build-commands.md:147` の順序制約が**引き続き有効である**ことを残余として明記する（下の「触らないと決めたもの」の送り先を文書に接地させる）

**触らないと決めたもの（根拠つき）**:

- `snotra-core/tests/memory_footprint.rs:11` の `//!`「実運用点（`%APPDATA%\Snotra\index.bin`）」——
  ベンチが基準にするのは**実運用のプロファイル**であり、検証用プロファイルは「実運用点」ではない。
  env 上書きが入ってもこの記述は真のままである
- `scripts/smoke-egui.ps1` の `$env:APPDATA\Snotra` 参照 3 本（`:66-67`, `:125`, `:477`）と
  `.github/workflows/e2e.yml` のステップ順序制約——env 化すれば `-SeedConfig` の制約も
  `-RequireResults`（#686）も順序制約も丸ごと不要になるが、**#803 のスコープ外**
  （issue 本文は「副次的な用途」に置いている）。CI ゲートを本 PR の爆風半径に入れない。
  → **送り先は #804**（起票済み。`docs/build-commands.md:147` の順序制約が引き続き有効である
  ことは上のチェックリスト項目で文書へ明記する）

### Phase 4 — 検証

- [x] `cargo test -p snotra-core`（カテゴリ A）
- [x] `cargo test -p snotra`（カテゴリ A・`src-tauri` の `///` を触るため）
- [x] `cargo clippy --workspace --all-targets -- -D warnings`（カテゴリ A）
- [x] `cargo check --workspace`（カテゴリ A）
- [x] `cargo doc --workspace --no-deps --document-private-items`（カテゴリ A・**必須**。`///` を触るのに **hook は発火しない**＝沈黙は合格ではない）
- [x] `npm run governance:check`（カテゴリ F・`*.md` を変更するため）
- [x] `npm test`（`scripts/` 配下を変更するため。既存の vitest が緑であること）
- [x] `npm run check:colors`（本 PR の対象そのもの。緑 = 紫が届いている）
- [x] `npm run check:colors -- -Color '#FFF'`（3 桁 hex の受理・#680 の 1 の回帰）
- [x] 実行後に**ユーザーの実 config が変更されていないこと**を確認する（`%APPDATA%\Snotra\config.toml` の更新時刻）
- [x] **env を設定せずに `cargo run -p snotra` を起動し、既存の設定・履歴・索引がそのまま見えることを確認する**（破壊不変条件の検知手段。下記セルフレビュー 3）
- [x] `npm run smoke:egui`（seed のコメントを触るため。`-SeedConfig` 経路が壊れていないこと）

## 不変条件

1. **既定の保存先は変わらない。** env 未設定・空文字のとき `config_dir()` は
   `dirs::config_dir()/Snotra` を返す。既存ユーザーのデータ移行は発生しない
   （検知: `config_dir_from_falls_back_to_base_when_env_absent` / `..._when_env_is_empty`）
2. **上書きはそのまま使い、`Snotra` を付け足さない。** この非対称が壊れると、検証スクリプトが
   渡した temp パスの下に更に階層ができ、seed した config が読まれなくなる
   （検知: `config_dir_from_uses_override_verbatim`）
3. **13 箇所すべてが一様に追従する。** `config.toml` / `history.bin` / `index.bin` / `icons.bin` /
   `window.bin` はすべて `Config::config_dir()` から導かれ、`dirs::config_dir()` の直接呼び出しは
   この 1 行だけである（検知: `rg 'dirs::config_dir'` が 1 件であること・実装時に再確認）
4. **検証スクリプトはユーザーの実 config を読みも書きもしない。** 退避が無くなるということは、
   異常終了しても実 config が検証色で固定される経路が**構造的に消える**ということである
   （検知: Phase 4 の「実 config の更新時刻」確認）
5. **seed が parse される。** seed が必須セクションを欠くと破損復旧経路へ落ち、既定色（`#282828`）で
   起動する——`background_color` が届いていても**赤**になる向きなので沈黙はしないが、原因が
   「色が届いていない」と誤読される。**判定を本体 stderr の `[config] ` 行の不在と併せる**ことで
   この 2 つを区別する（検知: Phase 2 の stderr チェック）。
   **`.bak` の不在を使ってはならない**——退避は best-effort で、`fs::rename` が失敗すれば
   parse 失敗でも `.bak` は現れない（`config.rs:938`）
6. **単一インスタンス衝突は依然として沈黙する。** プロファイル分離では解消しない（research C6）。
   起動前チェックを消してはならない（検知: 起動前チェックの存置とコメント）
7. **異常終了時に残るのは使い捨てプロファイルだけである。** `target/visual-check/profile` は
   次回実行で上書きされる。**`CARGO_TARGET_DIR` を設定していない既定の構成なら** `cargo clean` が
   掃く（設定している環境では対象外。実測: `CARGO_TARGET_DIR` を渡すと `cargo metadata` の
   `target_directory` がそちらへ移る）。既存の `$shotDir` も同じ前提の上に在るので、置き場所を
   揃えるのが一貫している。回収コマンド（旧 `-Restore`）は不要になる。
   **ただし「後始末がゼロになる」は偽である**——retire する 5 つに対し **setup が 1 つ増え**、
   `config.toml` + `*.bin` の残余が（ユーザー資産ではない場所に）残る。PR 本文・docs では
   「5 つが全部消える」ではなく「**5 つ retire / setup 1 / 残余は `cargo clean` が掃く**」と書く
8. **`SNOTRA_CONFIG_DIR` は single-instance の識別子を変えない。** データは分離できるが**同時起動は
   できない**。docs に「検証用と実用のプロファイルを分離できる」と無条件に書くと偽になる
   （検知: 不変条件 6 の起動前チェックが残っていること）
9. **`snotra-settings` が同じプロファイルを見るのは env 継承ゆえである。**
   `launch_settings_process` に `.env_clear()` / `.env_remove()` を足すと沈黙して壊れる
   （検知: `commands/window.rs` の `///` に依存を明記する。コード側に置くので腐らない）

## テスト方針

`snotra-core/src/config.rs` の `#[cfg(test)]` に 4 本（env に触れず純粋関数を測る）:

| テスト名 | 検証する不変条件 |
|---|---|
| `config_dir_from_uses_override_verbatim` | 不変条件 2（`Snotra` を付け足さない） |
| `config_dir_from_falls_back_to_base_when_env_absent` | 不変条件 1（既定の保存先） |
| `config_dir_from_falls_back_to_base_when_env_is_empty` | 不変条件 1 + 境界条件 C5（空文字 → CWD 流出の防止） |
| `config_dir_from_is_none_without_override_or_base` | `base` が解決できない極端な環境で `None` を返す（`load_reporting` の early-return 契約を保つ） |
| `config_dir_from_does_not_expand_or_absolutize_override` | 相対パス・未展開の `%VAR%` を**そのまま返す**（codex の指摘。挙動を明文化して固定する。実測: `%TEMP%\Snotra` は展開されず `GetFullPath` すると CWD 起点になる） |

スクリプト側は自動テストを持たない（実機 GUI が対象）。**接地は Phase 4 の実行**であり、
`config.toml.bak` の不在（seed の健全性）と実 config の更新時刻（不変条件 4）が観測点である。

## SPEC.md 更新要否

**要**。`%APPDATA%\Snotra` を**無条件で**保存先と書いている箇所は SPEC.md に 4 つある
（`:211` §5.2 履歴 / `:603` §13.1 / `:615-618` §13.2 / `:637` §13.3「設定フォルダを開く」）。
env 上書きの導入でこれらが一斉に偽になる（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

**但し書きを 4 か所へ写さない。** §13 の冒頭に**文書全体へのスコープ宣言**を 1 つ置く:

> データの保存先は既定で `%APPDATA%\Snotra\` である。環境変数 `SNOTRA_CONFIG_DIR` を設定すると
> その値がそのまま保存先になる（未設定・空文字なら既定）。**本書で `%APPDATA%\Snotra` と表記する
> パスはすべてこの上書きに従う。** 検証・portable 運用のための開発向けハッチであり、
> **単一インスタンス制御には影響しない**（プロファイルを分けても同時起動はできない）。

これで §5.2 / §13.1 / §13.2 / §13.3 の 4 箇所が一度に真になる（`AGENTS.md`「文書に事実の写しを
増やす変更 → 正本を 1 か所に定め他は参照へ」）。**別文書の `docs/architecture.md:104` には
スコープ宣言が届かない**ので、そこへは参照を 1 つ足す。

## セルフレビュー

### `/plan-review` の結果を反映した点

台帳 4 件すべて実在（rust-core / scripts / docs-governance / independent-derivation）。
「要対処」1 件・独立導出の漏れ 6 件を計画へ反映済み:

1. **`Config::config_dir()` は 13 箇所ではなく 14 箇所**（issue 本文と research.md の誤り。自分で数え直した）
2. **SPEC.md の全称表現は §13.1 だけではなく 4 箇所**（`:211` / `:603` / `:615-618` / `:637`）。
   さらに `docs/architecture.md:104` / `src-tauri/src/icon.rs:178`。→ スコープ宣言 1 つで直す形へ変更
3. **プロファイルの置き場所を `$env:TEMP` から `target/visual-check/profile` へ**（`cargo clean` が掃く）
4. **`[paths]` を空ヘッダで置けば索引構築が即終了**（issue の「正直なコスト 2」が消える）
5. **`cargo doc` がカテゴリ A の必須で hook 非発火**（`///` を触るため。Phase 4 へ追加）
6. **`.env_clear()` を将来足すと沈黙して壊れる**（`commands/window.rs` の `///` へ依存を明記）
7. **削除対象の列挙漏れ**: `$configPath`（`:60`）・実 config の存在ガード（`:88-90`）・`Set-TomlKey`

`/plan-review` Step 2 が ①対称コードパス ②影響範囲 ③リソース管理 ④既存パターン整合 ⑤YAGNI を
検証済みのため 5b では再実行しない。**「独立レビュー不成立」のエントリは無い**（4/4 実在・内容も充実）。

### codex の敵対レビューを反映した点（`workspace/plan-review/codex-adversarial.md`）

`codex exec --sandbox read-only` で「同意ではなく反証」を求めた。4 件のうち **serious 2 件は
自分の実測で再照合して成立**を確認し、計画を直した:

8. **`.bak` の不在は seed の parse 成功を証明しない**（serious）——`backup_invalid` の `fs::rename` が
   失敗すれば `.bak` は現れないまま `RecoveredFromCorrupt` になる（`config.rs:938`）。
   → **判定を stderr の `[config] ` 行の不在へ差し替えた**（失敗 4 arm すべてに eprintln があり、
   成功時には出ない）。**「副作用の不在」で「処理の成功」を測っていた**のが誤りだった
9. **`cargo clean` が掃くとは限らない**（minor→計画の全称表現の誤り）——`CARGO_TARGET_DIR` を
   設定した環境では対象外（自分でも実測: `target_directory` が移る）。→ 前提条件を付けた
10. **相対パス・未展開 `%VAR%` の境界が空文字しか見えていなかった**（serious）——
    → 挙動を固定するテストを 1 本足し、**拒否しない理由**（フォールバックの向き）を明文化した
11. **上書きが既存ファイルを指す場合**（minor）→ 境界表に足した（8 の stderr チェックが捕まえる）

**codex が反証できなかったもの**（7 claim 中 4 つ）は、その旨が成果物の末尾に記録されている。

**受容した残余（`/plan-review` の設計上の既知残余が顕在化した）**: scripts スカウトが契約に反して
`snotra-core/src/config.rs` へ検証用テストを書き込んだ（`general-purpose` 型は `Write`/`Edit` を持ち、
プロンプト契約でしか縛れない）。`git checkout --` で復元済み。**同じ測定は自分で実行して一次証拠を取った**。

### 5b の 3 観点

**1. 境界条件** — 列挙と検証ケースの対応:

| 境界 | 検証ケース |
|---|---|
| env 未設定 | `config_dir_from_falls_back_to_base_when_env_absent` |
| env が空文字 | `config_dir_from_falls_back_to_base_when_env_is_empty` |
| env あり（通常） | `config_dir_from_uses_override_verbatim` |
| `dirs::config_dir()` が `None` | `config_dir_from_is_none_without_override_or_base` |
| 上書き先が存在しない | `save_to_dir` の `create_dir_all` が作る（`config.rs:952`）。Phase 2 の実行で通る |
| seed が parse 不能 | Phase 2 の stderr `[config] ` 不在チェック |
| 前回実行の `.bak` / `*.bin` 残骸 | seed 前に削除する（Phase 2） |
| 上書きが相対パス | `config_dir_from_does_not_expand_or_absolutize_override`（そのまま使う＝実データに届かない） |
| 上書きが未展開の `%VAR%` | 同上（Windows は `var_os` の `%VAR%` を展開しない・実測） |
| 上書きが既存**ファイル**を指す | `create_dir_all` が失敗 → `save` がエラー → 既定値で起動。stderr に `[config] ` が出るので Phase 2 のチェックが捕まえる |
| single-instance 衝突 | 起動前チェック（存置・不変条件 6） |
| env が効かず実 config を読む | プロファイル配下の `*.bin` 生成チェック（Phase 2） |

**2. シンプル化の挑戦** — 新たな状態は増やさない（`AtomicBool` も子プロセスもタイマーも無い）。
`config_dir()` は毎回 env を読み、キャッシュしない——env はプロセス起動時に確定し実行中は変わらない
ので、キャッシュは「起動時と終了時で違う dir を見る」危険を足すだけで何も買わない。
`ENV_CONFIG_DIR` は**私有**にした（消費者が 1 つしかないものを `pub` にする理由が無い）。
**この操作が失敗したら**: `var_os` は失敗しない。上書き先が読めないときは既存の `LoadOutcome::ReadFailed`
経路（既定値で起動・退避も上書きもしない）にそのまま乗る——新しい失敗様態を作らない。

**3. 破壊不変条件 + 検知手段**:

| 壊れたら即アウト | 検知手段 |
|---|---|
| **既定の保存先が変わる**（全ユーザーの config / 履歴 / 索引が黙って別の場所へ移り、データ喪失に見える） | **env なしで起動して既存データが見えることの目視**（Phase 4）が唯一の検出器である。ユニットテスト 4 本は**これを検出できない**——`config_dir_from(None, Some(base))` は `base` を**注入**するので、`config_dir()` が `dirs::config_dir()` を呼んでいること自体を誰も見ていない。**純粋関数へ割ると、測れない部分は消えずに seam の外側へ移動する。** → code-reviewer の High 2 を受けて **`config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix` を追加**（env を読むだけで結線を pin する）。目視ゲートはその上の追加確認として残す。**なお `dirs::data_dir()` に変えても検出できないのは正しい**——Windows では `config_dir()` と同一（RoamingAppData・`dirs-6.0.0/src/win.rs` 実測）。危険な取り違えは `data_local_dir()` 系である |
| **検証スクリプトが実 config を書く**（この issue が消そうとしている当のもの） | 実 config の更新時刻確認（Phase 4）。加えて、退避コードが**存在しない**こと自体が構造的な保証になる |
| **seed が parse されず既定色で起動し、原因を誤読する** | `config.toml.bak` の不在チェック（Phase 2）。plan の seed が parse できることは実測済み |
