# code-review — issue #803 `SNOTRA_CONFIG_DIR` の env seam

対象: ブランチ `feat/config-dir-env-seam` の**作業ツリー差分**（`git diff HEAD` の非 `workspace/` 部分）。
`git diff main...HEAD` は `workspace/` の 4 コミットしか含まず、実装は未コミットである（レビューはこの前提で行った）。

判定: **Critical 0 件 / High 2 件 / Medium 2 件 / Low 3 件**。

---

## Critical（必須修正）

**0 件。**

---

## High（修正推奨）

### H1. `-Interactive` の目視項目 3・4 が、この変更で**観測不能**になった

- 場所: `scripts/visual-check-colors.ps1:118`（seed の `[paths]`）と `scripts/visual-check-colors.ps1:151-155`（読み上げる目視項目 3・4）
- 根本原因: seed プロファイルは索引 0 件になるため「results 可視 ⇔ main 可視 ∧ 結果が空でない」（`SPEC.md` §8.6・差分の hunk header に見える行）が**永久に偽**になり、スクリプトが操作者へ指示する項目 3「結果リスト窓（何か入力して出す）の背景も同色である」・項目 4「文字を打って件数を変え続けたとき、results がちらつかない」を実行できない。旧版は**ユーザーの実 config**（実 scan パス付き）を書き換えて起動していたので両項目は観測できた。**この変更で検証能力が落ちている。**
- 索引が 0 件であることの根拠（到達経路を 5 本とも塞いでいる）:
  - `[[paths.scan]]` を書かない（`visual-check-colors.ps1:118`・`PathsConfig.scan` は `#[serde(default)]` ゆえ空 Vec。`Config::default_scan_paths()` には落ちない）
  - `include_path_env` の既定は無効（`SPEC.md:52`）
  - `instant_commands` は `#[serde(default)]` ゆえ**空 Vec** になる（`snotra-core/src/config.rs:104-105`）——`Config::default()` の `g` / `gh`（`config.rs:570-585`）は seed 経由の deserialize では**載らない**ので、`/g` でも results は出ない
  - 新規プロファイルなので履歴も空 → 空クエリの recent 候補も 0 件
  - **フォルダ列挙は索引を経由しない**（`spawn_folder_load` → `ctx.read_dir_entries` が実 FS を読む・`launcher_controller.rs:639-657`）が、その突入口は「選択中の結果行」に対する → / ← である（`launcher_controller.rs:1036-1044, 1071-1078`）。行が 0 件なら `results().get(selected())` が `None` になり folder view へ入れない——**索引が空であることがフォルダ経路も同時に閉じている**
- 沈黙の向き: 操作者は「打っても何も出ない」を見るが、**「results 経路が壊れている」と「索引が空である」を区別できない**。`docs/build-commands.md:75` は「残る 2 点は目視（`-Interactive`）に留まる」と書いており、その 2 点の片方がこの変更で消えている。
- 計画側の抜け: `workspace/plan.md:97-100` の「`[[paths.scan]]` を書かない」根拠は**自動判定経路にしか当たっていない**（「results 窓を出す必要が無く」）。`-Interactive` 経路へ暗黙に波及した。`scripts/visual-check-colors.ps1:94-95` の相互参照コメントも同じ誤りを写している。
- 修正例（`-Interactive` のときだけ smoke-egui と同じレシピを敷く。自動判定は 0 件のまま＝実測済みの緑を動かさない）:

```powershell
# --- seed を組む前 ---
$scanToml = ''
if ($Interactive) {
    # 目視項目 3・4（results 窓）は索引に 1 件以上ないと**到達できない**（SPEC §8.6）。
    # smoke-egui.ps1 の -SeedConfig と同じダミーを使う（indexer は拡張子だけで判定する）。
    $scanDir = Join-Path $env:TEMP 'snotra_visualcheck_scan'
    New-Item -ItemType Directory -Force -Path $scanDir | Out-Null
    $dummy = Join-Path $scanDir 'zsnotracheck.exe'
    if (-not (Test-Path $dummy)) { New-Item -ItemType File -Path $dummy | Out-Null }
    $scanToml = @"

[[paths.scan]]
path = "$($scanDir -replace '\\', '/')"
extensions = [".exe"]
include_folders = false
"@
}
# $seedToml の末尾（`[paths]` の直後）へ $scanToml を連結する
```

読み上げ側（`:151`）にも「`z` と打つと 1 件出る」を添えると、操作者が「出ない」ことを異常と判定できる。

### H2. 「既定の保存先が変わらない」不変条件に**自動検出器が 1 つも無い**——安全に置ける

- 場所: `snotra-core/src/config.rs:667-668`（`config_dir()`）と `snotra-core/src/config.rs:1176-1235`（テスト 5 本）
- 根本原因: 5 本のテストはすべて `base` を**注入**するため、`config_dir()` が `dirs::config_dir()` を呼び `Snotra` を join していること自体を誰も見ていない。`dirs::data_local_dir()` へ書き換えても 5 本とも緑になる（`workspace/plan.md:302` と `snotra-core/CLAUDE.md` が自認済み）。この不変条件は plan 自身が「**壊れたら即アウト**」に分類しており（`plan.md:300-302`）、そこに残る唯一の検出器が「目視」なのは、この規模の変更に対して弱い。
- **例示の訂正（この報告の副産物）**: `plan.md:302` と `snotra-core/CLAUDE.md` が挙げる「`dirs::data_dir()` に変えても 4 本とも緑」という例示は **Windows では成立しない**——`dirs` は `config_dir()` と `data_dir()` をどちらも `FOLDERID_RoamingAppData` へ写す（`dirs-6.0.0/src/win.rs:8,10` 実測）ので、その差し替えは挙動を変えず、新テストを足しても足さなくても緑である。**実際に別物になるのは `data_local_dir()` / `config_local_dir()`（`%LOCALAPPDATA%`）**で、上のテストはこちらを捕まえる。両文書の例示も併せて直すこと
- 「env はプロセス全域の可変状態ゆえテストできない」は**書き込みにしか当たらない**。`var_os` で**読むだけ**なら並列テストから安全であり、`unsafe` も要らない。次の 1 本で seam の外側が pin できる:

```rust
/// `config_dir()` が既定側で `dirs::config_dir()/Snotra` を返すことを pin する。
/// `config_dir_from` の 5 本は `base` を注入するため、この結線だけは誰も見ていない
/// （`dirs::data_local_dir()` へ差し替えても 5 本は緑になる）。env は**読むだけ**なので
/// 並列テストから安全（`set_var` はしない）。
#[test]
fn config_dir_defaults_to_dirs_config_dir_joined_with_snotra() {
    if std::env::var_os(ENV_CONFIG_DIR).is_some() {
        return; // 上書き中の環境では既定経路を測れない（M2 との相互作用に注意）
    }
    assert_eq!(
        Config::config_dir(),
        dirs::config_dir().map(|p| p.join("Snotra"))
    );
}
```

- **M2 と結合している**: この test は `SNOTRA_CONFIG_DIR` が設定されていると skip する。M2（env の後片付けが無い）を放置すると、スクリプトを自分の pwsh セッションで直接叩いた開発者のシェルでは**この検出器が黙って無効になる**。両方直すこと。
- 併せて `snotra-core/CLAUDE.md`（`config.rs` 節の 2 つ目の bullet）と `workspace/plan.md:302` の「どのテストも見ない」「目視だけである」という記述も更新が要る（現状は真だが、修正すれば偽になる）。

---

## Medium（修正検討）

### M1. `Test-SeedHealth` は**ログ不在を合格にする**——plan が `.bak` について正しく退けた形の再発

- 場所: `scripts/visual-check-colors.ps1:135`（`if (-not (Test-Path $LogPath)) { return $true }`）
- 根本原因: stderr ログは**このスクリプト自身が** `-RedirectStandardError`（`:195`）で作らせるものであり、`Start-Process` は出力が 1 バイトも無くても空ファイルを作る。したがって「ファイルが無い」は正常状態ではなく**リダイレクトが成立しなかった**状態であり、そこを `return $true`（＝seed は健全）へ写像すると、観測手段が失われたことが合格として通る。これは plan が `.bak` について正しく退けた「**副作用の不在で処理の成功を測る**」（`plan.md:256-259`）と同じ形が、1 段小さい所で再発したものである（`.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は、その状態に到達する全経路を列挙する」に直接当たる）。
- 修正例:

```powershell
function Test-SeedHealth {
    param([string]$LogPath)
    # **不在は合格ではない。** このログはスクリプトが -RedirectStandardError で作らせたもので、
    # 出力が空でも空ファイルが残る。無いのはリダイレクトが成立しなかったということであり、
    # 観測手段の喪失を合格へ写像しない（`.bak` の不在を使わない理由と同じ）。
    if (-not (Test-Path $LogPath)) {
        Write-Host ''
        Write-Host "判定: 赤 — stderr ログが存在しません（$LogPath）。seed の健全性を観測できていません。"
        return $false
    }
    ...
}
```

### M2. `$env:SNOTRA_CONFIG_DIR` に**戻す経路が無い**（set/restore の対が欠けている）

- 場所: `scripts/visual-check-colors.ps1:123`（set）に対し `:303-308` の `finally` は `Stop-Process` だけ。`-Interactive` は `:163` の `return` で抜けるため、そちらにも復元点が無い
- 根本原因: プロセス全域の可変状態を生成して破棄していない（`AGENTS.md`「リソース管理・状態フラグは生成/破棄・真偽のペアで計画する」／`/symmetric-check` の対象）。`npm run check:colors` は `pwsh -NoProfile -File`（`package.json:14`）で別プロセスなので被害は出ないが、**スクリプトを自分の pwsh セッションで直接叩くと**（`.SYNOPSIS` はその形も想定している）そのシェルの残り全体に env が残る。以後の `cargo run -p snotra` は使い捨てプロファイルを読み、`snotra-core/tests/memory_footprint.rs:11` が基準にする「実運用点（`%APPDATA%\Snotra\index.bin`）」も**別の場所を指す**。H2 の新テストも黙って skip する。
- 併せて、**開発者が意図して設定していた値を上書きして戻さない**（別プロファイルで作業中のシェルで走らせるとその設定が消える）。
- 修正例:

```powershell
$prevConfigDir = $env:SNOTRA_CONFIG_DIR   # set の前に退避（未設定なら $null）
...
} finally {
    if ($proc -and -not $proc.HasExited) { ... }
    # **set したら戻す。** 直接叩かれたセッションに env が残ると、以後の cargo run が
    # 使い捨てプロファイルを読み、memory_footprint の「実運用点」も別の場所を指す。
    if ($null -eq $prevConfigDir) { Remove-Item Env:SNOTRA_CONFIG_DIR -ErrorAction SilentlyContinue }
    else { $env:SNOTRA_CONFIG_DIR = $prevConfigDir }
}
```

`-Interactive` は現在 `try` の外で `return` するので、この経路も同じ復元を通すこと（`try` / `finally` の中へ入れるか、`return` の直前で復元する）。

---

## Low（改善余地）

### L1. `docs/build-commands.md:88` の「`cargo run -p snotra-settings` の単独起動には効かない」が誤読を招く

`snotra-settings` も `Config::config_dir()`（`snotra-settings/src/tabs/backup.rs:104,110`）を通るので、そのシェルで env を設定すれば**単独起動でも効く**。書き手の意図は「（親からの継承が）単独起動には効かない」だが、文面は「env ハッチが効かない」と読める。`AGENTS.md`「全称表現は前提条件とセットで書く」に照らすと、「単独起動では**継承ではなく**、そのシェルの env が効く」と書き換えるのが正確。

### L2. `SPEC.md:601` のスコープ宣言が、それが修飾する `:211` の **390 行後**に置かれている

宣言文は「本書で」と明示しているので**内容としては真**（§5.2 も §13.x も §13.3 も対象）。ただし §5.2 を読む読者は、修飾を見る前に無条件の「`%APPDATA%\Snotra\` に保存」に出会う。`:211` へ「（保存先の上書きは §13 冒頭）」の 1 参照を足すと、写しを増やさずに読み順の穴が閉じる。

### L3. `GetPixel` の 2 重ループは窓 1 枚あたり数万回の相互運用呼び出しになる

`scripts/visual-check-colors.ps1:247-255`。600×118 なら約 7 万回で、PowerShell 経由の `GetPixel` は 1 回あたり数十 µs のため秒オーダーを消費する。最頻色判定という設計自体は正しい（1 点サンプルより堅い）。窓が大きくなると効いてくるので、`LockBits` + `Marshal.Copy` で 1 枚のバイト配列にしてから数える形が素直。検証補助ゆえ優先度は低い。

---

## 4 観点への回答（積極的な確認）

### 観点 1: `config_dir_from` の判定と呼び出し元 14 箇所

- 分岐は 4 つとも doc と一致し、テストも 5 本そろっている（`config.rs:680-687`・`:1176-1235`）: `Some(非空)` → そのまま／`Some("")` → 既定／`None` → 既定／`base=None` → `None`。相対パスと未展開 `%VAR%` を**そのまま返す**選択も `config_dir_from_does_not_expand_or_absolutize_override` で固定されており、「フォールバックの向きは壊れたときに何を守るかで決める」という理由づけは妥当（既定へ落とすと実 config を触るので目的が裏返る）。
- 呼び出し元は **14 箇所すべてが `Config::config_dir()` 経由**で、追加の分岐は無い: `binfmt.rs:30`／`config.rs:690, 892, 975`／`history.rs:86, 154, 183`／`indexer.rs:396, 463, 590`／`window_data.rs:62, 86`／`snotra-settings/src/tabs/backup.rs:104, 110`。
- `dirs::config_dir()` の直接呼び出しは `config.rs:668` の**ちょうど 1 件**（rustdoc・`snotra-core/CLAUDE.md` の全称表現はスコープどおり真）。`config_watcher.rs:28-30` は `config_path().parent()` から導くので自動追従する。`launch_settings_process`（`src-tauri/src/commands/window.rs:80`）は `.env_clear()` を持たず、追記した `///` の依存記述は実装と一致している。
- `Set-Content -Encoding utf8` が **BOM なし** TOML を出すことは判定に効く前提（BOM 付きだと parse が落ちて破損復旧経路）。これを保証しているのは `scripts/visual-check-colors.ps1:1` の `#Requires -Version 7` のみ——存在は確認した。

### 観点 2: 既定の保存先が変わっていないこと

`config_dir_from` の既定分岐は正しく、テストもある。**穴は「`config_dir()` の結線」の 1 点だけ**で、これは H2 のとおり env を**読むだけ**のテストで安全に塞げる。plan が挙げた「目視が唯一の検出器」は**現状の記述としては正確**だが、より安い自動検出器が存在するのに採らなかったのは計画判断の抜けと見る（穴の**認識と明文化**自体は適切だった）。

### 観点 3: 2 つの判定の意味が成り立たない到達経路

- 「seed が読めた」⇔ `[config] ` 行の不在: `Select-String -SimpleMatch`（`:136`）で `[` の正規表現解釈は回避済み。失敗 4 arm すべてに `[config] ` の `eprintln` があり成功時には出ないことも実装で確認した（`config.rs:921, 937, 947-950` と `backup_invalid` の `:963, :967`）。**残る穴はログ不在の 1 経路のみ（M1）。**
- 「env が効いた」⇔ プロファイル配下の `*.bin`: 前回の残骸は `:85-87` で消しており、空振り合格は塞がれている。env 無効時は本体が実 config 側へ書くので 0 件＝赤になり、向きも正しい。timing 依存（索引書き込みが観測時点までに完了する保証は無い）だが、外れる向きは**赤**なので沈黙しない。
- **未実行の経路がある（検証の穴）**: `:280` と `:285` の 2 つの `exit 1` は、実施済みの 2 回の緑ではどちらも通っていない。両方 `try` の中にあり、子プロセスの kill は `:303` の `finally` に依存している。1 回だけフォールトインジェクション（起動後に `target/visual-check/snotra-stderr.log` へ `[config] test` を書き足す／`$profileDir` を空ディレクトリへ向ける）して、**赤で抜けたときに `snotra.exe` が残らないこと**を確認しておくのを勧める。残ると次回実行が `:77` の single-instance ガードで落ち、無関係な環境問題に見える。

### 観点 4: SPEC §13 のスコープ宣言が 4 箇所を真にするか

**真である。** 4 箇所すべて `Config::config_dir()` から派生する:

| SPEC | 記述 | 実装の導出元 |
|---|---|---|
| `:211` §5.2 | 履歴を `%APPDATA%\Snotra\` へ | `history.rs:86, 154, 183` → `Config::config_dir()` |
| `:605` §13.1 | `config.toml` | `config.rs:690` `config_path()` |
| `:617-620` §13.2 | `index.bin` / `icons.bin` / `history.bin` / `window.bin` | `binfmt.rs:30`・`indexer.rs`・`window_data.rs` すべて `Config::config_dir()` |
| `:639` §13.3 | 「設定フォルダを開く」 | `snotra-settings/src/tabs/backup.rs:104, 110` → `open::that(dir)` |

`SPEC.md:945-946` の `%APPDATA%`（instant command の変数展開の例）は `%APPDATA%\Snotra` 表記ではないので宣言のスコープ外——正しく除外されている。読み順の問題だけ L2 に残る。

---

## Phase 2 の各サブチェック（記録）

- **2a. plan.md 不変条件との照合**: 9 件中 8 件は実装で守られている。不変条件 5「seed が parse される」の検知手段（stderr `[config] ` 不在）は **M1 のログ不在経路で 1 か所破れている**。不変条件 1 は実装として守られているが、plan が自認するとおり検出器が目視のみ（**H2**）。不変条件 6・8（single-instance）は `:71-79` のガードと理由コメントで維持。不変条件 9（env 継承）は `commands/window.rs` の `///` に明記され、実装に `.env_clear()` は無い。
- **2b. 対称コードパス**: 生成/破棄の対で 1 件欠落（**M2**: `$env:SNOTRA_CONFIG_DIR` の set に対する restore）。それ以外の対称ペアは成立——`Remove-Item` による前回残骸の掃除（`:85-87`）は 2 つの判定と対、`Stop-Process`（`:304-307`）は `Start-Process`（`:195`）と対。NSIS アンインストーラが触るのは**既定の** `%APPDATA%\Snotra` のみだが、これは正しい挙動（アンインストールが env 上書き先を追いかけるべきではない）で、対称修正の漏れではない。
- **2c. DRY / 関数カバレッジ**: `Config::config_dir()` を迂回して保存先を組む箇所は無い（`dirs::config_dir()` は 1 件のみ・`%APPDATA%` のリテラル結合も Rust 側に無い）。seed TOML の 2 重化は plan が理由つきで選び相互参照コメントも両側にある（`smoke-egui.ps1:92-95` / `visual-check-colors.ps1:93-97`）——**H1 を直すと両者の差は `[general]` / `[visual]` の有無だけになる**ので、そのときコメントの差分説明も更新すること。
- **2d. リソースライフサイクル**: 生成 `$env:SNOTRA_CONFIG_DIR`（`:123`）→ 破棄 **無し** → 欠落（**M2**）。生成 `$proc`（`:195`）→ 破棄 `:304-307` → ペア成立。生成 `$bmp` / `$gfx`（`:237-238`）→ `Dispose`（`:240, :269`）→ ペア成立（`:264-268` で throw した場合のみ `$bmp` が漏れるが、GC が回収するので実害なし）。Rust 側は新規リソース無し。
- **2e. SPEC.md 同期**: 差分は「保存先の導出」という §13 記載の挙動を変えており、SPEC.md が同じ差分に含まれている（`:601`）→ **同期済み**。§13.1 / §13.2 / §13.3・§5.2 は宣言 1 つでカバーされ、写しは増えていない（`AGENTS.md`「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」に適合）。`docs/architecture.md:104` の参照追加も適切。
- **2f. 「変更不要」判断の再評価**: `snotra-core/tests/memory_footprint.rs:11`（実運用点）と `smoke-egui.ps1` の `$env:APPDATA` 起点の参照 3 本を #804 へ送る判断は妥当——ただし前者は **M2 の env 漏れがあると前提が崩れる**（漏れたセッションでベンチを走らせると使い捨てプロファイルを測る）。M2 を直せば成立する。`package.json` 変更不要も正しい（フラグを埋め込んでいない）。

## Phase 3 パフォーマンス

`search.rs` / `folder.rs` / 検索結果レンダリング / アイコンキャッシュには触れていない。Rust 側の追加は `config_dir()` あたり `var_os` 1 回で、呼ばれるのは起動・保存・設定画面の各イベント時のみ（ホットパスではない）。スクリプト側の `GetPixel` ループのみ L3 に記載。
