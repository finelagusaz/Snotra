# research: #804 smoke スクリプトを SNOTRA_CONFIG_DIR で env 化し、e2e.yml の順序制約を解消する

## issue の要約

smoke スクリプトが `$env:APPDATA\Snotra` を直接見て「実 config の有無」に依存している。実 config を壊さないための安全側の設計から、`-SeedConfig` の条件分岐・`-RequireResults`・`e2e.yml` のステップ順序制約という 4 つの制約が芋づるで生えている。**検証プロファイルを `SNOTRA_CONFIG_DIR` で分離すれば根が消える。**

## リポジトリ自身が #804 の射程を明記している（正本引用）

`docs/build-commands.md:161`（**#804 が本文で挙げる「:147」は古い**——ファイルが伸びており、行番号ではなく概念で探して到達した）:

> **`e2e.yml` では egui smoke を startup smoke より前に置くこと**——後者の 5 起動が `config.toml` を作り seed を不成立にする（順序制約を守らせているのは規約ではなくこの flag である）。**#803 で `SNOTRA_CONFIG_DIR` が入った後もこの順序制約は有効である**——smoke スクリプトは依然 `$env:APPDATA\Snotra` を直接見ており、**env 化は #804 のスコープ（env 化すれば `-SeedConfig` の「既存を上書きしない」制約・`-RequireResults`・この順序制約がまとめて不要になる）**

つまり「何を消せるか」は既に文書化されている。判断が要るのは**消し方**（後述の未確定）。

## 現状の依存関係（実測した行）

| 箇所 | 内容 |
|---|---|
| `scripts/smoke-egui.ps1:66-67` | `$cfgDir = Join-Path $env:APPDATA "Snotra"` / `$cfgPath = ...\config.toml` |
| 同 `:68` | **`if (-not (Test-Path $cfgPath))`** — 不在時のみ seed。既存 config を持つ開発機では seed が不成立 |
| 同 `:106,114-116` | `$seededNow` が true のときだけ `ResultsQuery` に既定 `"z"` を入れる。false なら空のまま＝results 検査 skip |
| 同 `:125-134` | `-RequireResults` の guard（#686）。**アプリ起動前に確定するのでプロセスを起こさずに落ちる** |
| 同 `:481` | skip 時の黄色 NOTE（`%APPDATA%/Snotra/config.toml` を文言に含む） |
| `.github/workflows/e2e.yml:65-75` | egui smoke を **startup smoke より前**に置く制約と、その理由（#686 のコメント 9 行） |
| 同 `:77-82` | startup smoke は egui smoke が作った config.toml があるため **first-run 経路を通らない**——「カバレッジの縮小として受容する」と明記 |
| `docs/build-commands.md:161` | 上記引用（順序制約の規範） |
| `scripts/smoke-startup.ps1` | **config を一切扱わない**。ただし 5 起動が実 config（または CI の APPDATA）に `config.toml` を作る＝egui smoke の seed を潰す当の原因 |

## 既存パターン（`visual-check-colors.ps1` が参照実装）

| 要素 | 実装 |
|---|---|
| プロファイルの置き場 | `target/visual-check/profile`（`:54-58`）。**`$env:TEMP` ではなく `target/` の下**——スクリーンショットと同じ場所に集まり、`cargo clean` が config.toml も `*.bin` も掃く（新しい後始末機構を足さない） |
| 前回の残骸の除去 | `:85-87` で `config.toml.bak` と `*.bin` を消す。**残すと「seed の健全性」と「env が効いた証拠」の 2 判定がどちらも古いファイルで空振り合格する** |
| env の設定と後始末 | `:133` で設定、`:179`/`:329` の `finally` で `Remove-Item Env:SNOTRA_CONFIG_DIR` |
| **seed の健全性検査** | `Test-SeedHealth`（`:143-159`）。**`config.toml.bak` の不在を根拠にしない**——退避は best-effort で `fs::rename` が失敗すれば parse 失敗でも `.bak` は現れない（`config.rs` の `backup_invalid`）。健全な観測点は本体 stderr の **`[config] ` 前置き行**だけ。ログ自体の不在も「観測できなかった＝赤」として扱う |
| env が効いた証拠 | `:301-304` — プロファイルに `*.bin` が生成されたかを見る |

`CARGO_TARGET_DIR` を設定した環境では `target/` が `cargo clean` の対象外になる（受容する残余として `ADR-config-dir-env-seam-rejected-alternatives.md` §4 に記録済み）。

## 2 つの seed は同型ではない（ADR の生きている却下理由）

`ADR-config-dir-env-seam-rejected-alternatives.md` §3 は seed の共有ヘルパー化を**却下**しており、理由の 1 つは今も真:

> 2 つの seed は目的が違って**同型ではない**——smoke 側は results 窓を出すため `[[paths.scan]]` にダミーを 1 件置き、visual-check 側は索引 0 件で即終了させたいので置かない

**本 issue（#804）は seed を共有しない**——`smoke-egui` の seed 先を APPDATA から使い捨てプロファイルへ変えるだけで、ヘルパーの共有は #843 の射程である。ゆえに **ADR §3 は #804 では覆らない**（`smoke-egui.ps1:78-81` の相互参照コメントも生きたまま。ただし「あちらは検証用プロファイルへ書くので既存 config を気にせず」という記述は**両方がプロファイルへ書くようになるため要更新**）。

## `-SeedConfig` の seed 内容（そのまま移せる）

`smoke-egui.ps1:70-104`: `$env:TEMP\snotra_smoke_scan` にダミー `zsnotrasmoke.exe` を 1 件置き、`[hotkey]`/`[appearance]`/`[paths]` + `[[paths.scan]]` を書く。既定クエリ `"z"` がこの 1 件に一致する。

- **空 TOML は使えない**（必須セクション欠落で parse 失敗 → 破損復旧経路）
- **`scan = []` と `[[paths.scan]]` を併記してはならない**（同一キー再定義で parse 落ち）
- hotkey は Alt+Q＝`-HotkeyVks` の既定 `18,81` と一致

## #786 との関係（隣接するが別件）

`smoke-startup.ps1:64-71` の「最初の trace を待つ」ループは**行の存在だけ**を見ており（`@(Get-Content $errPath).Count -gt 0`）、`[trace]` 行かを判定しない。アプリが最初に出すのは `[index-load]` 行なのでそこで抜け、観測窓が trace 0 件のまま開く（#786・OPEN）。

**#804 で startup smoke に使い捨てプロファイルを与えても、この bug は消えない**——`[index-load]` は cache_hit=false でも出力される。ただし**悪化もしない**。修正は 1 行（待機条件に `-match '^\[trace\] '` を足す）で、#843 の trace 収集共有時に自然に入る形でもある。

## 技術的制約

- **単一インスタンス制御はプロファイルで分けられない**（`tauri_plugin_single_instance` は app identity で識別）。CI は逐次実行なので影響しないが、「プロファイルごとに並行 smoke」はできない（#804 本文が「残る費用」として明記）
- **e2e.yml の startup smoke は first-run 経路を通らない**（`:77-80`）——egui smoke が作った config があるため。**プロファイルを分ければ first-run が戻る**（カバレッジの回復だが、索引構築ぶん遅くなる）
- `-RequireResults` の guard は**アプリ起動前に確定する**ため、フォールトインジェクションが実機に触らずに済む（`docs/build-commands.md:161`）。この性質は撤去の判断材料になる

## 未解決の疑問（Step 4 の未確定欄で潰す）

- `-RequireResults` と `-SeedConfig` を**撤去するか残すか**。文書は「不要になる」と書くが、#686 が入れた検出器を消すことでもある
- startup smoke のプロファイルを **5 起動で共有するか毎回作り直すか**（first-run の扱いが変わる）
