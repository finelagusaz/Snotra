# plan: #804 smoke スクリプトを SNOTRA_CONFIG_DIR で env 化する

**方針**: `smoke-egui` / `smoke-startup` の検証プロファイルを `SNOTRA_CONFIG_DIR` で `target/` 配下へ分離する。**実 config の有無に依存する形が消えるので、そこから派生していた 3 つの制約（`-SeedConfig` の条件分岐・`-RequireResults`・`e2e.yml` の順序）も同時に落とす。** 参照実装は `scripts/visual-check-colors.ps1`（#803 で同じ分離を済ませている）。根拠と実測は `workspace/research.md`。

**seed ヘルパーの共有はしない**——`ADR-config-dir-env-seam-rejected-alternatives.md` §3 の却下理由のうち「2 つの seed は同型ではない」は今も真であり、共有化は #843 の射程である。本 issue は**書き先を変えるだけ**で、seed の中身は各スクリプトに残す。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `scripts/smoke-egui.ps1` | seed 先を `$env:APPDATA\Snotra` → `target/smoke-egui/profile` へ。**常に seed する**（不在時のみの条件を撤去）。`-SeedConfig` / `-RequireResults` の 2 パラメータを撤去し、**results 検査の要求を無条件へ格上げ**。seed 健全性の検査を追加。skip NOTE（`:481`）を撤去 |
| `scripts/smoke-startup.ps1` | 使い捨てプロファイルを 1 つ作り 5 起動で共有する（実 config を汚さない）。seed は**索引 0 件**の最小 TOML |
| `.github/workflows/e2e.yml` | `-SeedConfig -RequireResults` を落とし、順序制約のコメント **7 行（`:67-73`）**と first-run 受容の注記（`:77-80`）を撤去。**`:65-66` は別トピックなので残す**（フェーズ 3 の訂正が正・ラウンド 3 でこの行の「9 行（`:65-73`）」を是正した） |
| `docs/build-commands.md` | 「スモーク運用メモ」の該当 3 bullet と `:161` の順序制約の記述を更新。**`:160` は文の一部差し替えでは済まない**——「索引内容を制御できるときだけ」という条件付きの枠組み全体が偽になるため、bullet ごと書き直す |
| `scripts/visual-check-colors.ps1` | **1 行のみ**——`:93` の相互参照コメントから `-SeedConfig` への言及を外す（下記「触らない」節を参照） |
| `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` | §3 の末尾へ「その後（#804）」を追記（却下判断は書き換えない・未確定 (g)） |
| `.github/workflows/release.yml` | **変更しない・確認のみ**。`:83` の `smoke-startup.ps1` 呼び出しは `-ExePath` だけなので引数の変更は不要（ラウンド 3 実測）。確認対象は first-run を踏んだ場合の設定 GUI 残留（フェーズ 2） |
| `CONTRIBUTING.md` | **変更しない・確認のみ**。`docs/build-commands.md` が言及する参照の実在と整合（フェーズ 4） |

**触らない（根拠つき）**:

- ~~`scripts/visual-check-colors.ps1`~~ — **1 行だけ触る（レビューで是正）**。`:93` が相互参照の片割れ（「`smoke-egui.ps1` の `-SeedConfig` が同型の seed を持つ」）を持っており、`-SeedConfig` を撤去すると**存在しない識別子を指す**。ADR §3 がこの相互参照コメントを「片方だけ直る事故を防ぐ」ために置いた以上、**片方だけ直すのはその機構を壊すことそのものである**。分離の実装（#803 で済み）には触らない
- `scripts/bench-startup.ps1` / `measure-memory*.ps1` — 実 config を触るが**計測スクリプトであり検査ではない**（合否を返さない）。#804 が名指しするのは smoke 2 本。射程を広げない
- `scripts/manual-smoke.ps1` — 人が実 config で見るためのもので、分離すると目視の前提（実際の索引）が変わる
- `smoke-egui.ps1:78-81` の相互参照コメント — **文言だけ更新する**（下記）。ADR §3 は #804 では覆らないので、削除しない
- `SPEC.md` — 製品の挙動を変えない（検証機構のみ）

## 実装順序

### フェーズ 1 — `smoke-egui.ps1` の env 化

- [x] `$cfgDir` を `$env:APPDATA\Snotra` から `Join-Path $PSScriptRoot '..\target\smoke-egui\profile'` へ変える。**`target/` 配下に置く**（`visual-check-colors.ps1:54-58` と同じ理由——`cargo clean` が掃くので新しい後始末機構を足さない。`CARGO_TARGET_DIR` 環境での受容残余は `ADR-config-dir-env-seam-rejected-alternatives.md` §4 に既出）。
      **env へ入れる値は `(Resolve-Path $profileDir).Path` で絶対化する**（ラウンド 3 の独立再導出）——`config.rs:689` は env の値を「**そのまま**保存先にする」と明記しており、`..` を含んだ断片をそのまま渡さない。参照実装も同じ順序である（`visual-check-colors.ps1:132-133`: ディレクトリ作成 → `Resolve-Path` → env 設定）
- [x] **前回の残骸を消す**（`config.toml.bak` と `*.bin`）。`visual-check-colors.ps1:85-87` と同じ——残すと seed 健全性と env 到達性の 2 判定がどちらも古いファイルで空振り合格する
- [x] `$env:SNOTRA_CONFIG_DIR` を設定し、**`finally` で必ず戻す**。**「消す」ではなく「元の値へ戻す」**（ラウンド 3）——`smoke-startup.ps1:32-39` の `Restore-TraceEnv` と同じ形で、設定前の値を退避しておき、`$null` なら `Remove-Item Env:SNOTRA_CONFIG_DIR -ErrorAction SilentlyContinue`、値があれば代入して戻す。**呼び出し元のシェルが既に `SNOTRA_CONFIG_DIR` を持っている場合がある**（`visual-check-colors.ps1:179` が案内する手動ワークフロー）ので、無条件に消すとその値を壊す——不変条件 2 が挙げていたのは漏洩だけで、この上書きは検討されていなかった。**`-ErrorAction SilentlyContinue` を省いてはならない**: 未設定時に `ItemNotFoundException` を投げ、`$ErrorActionPreference = "Stop"`（`:58`）の下では `finally` が元の例外を覆い隠す（ラウンド 3 実測）。
      **既存の `try` を流用してはならない**——`smoke-egui.ps1` の `try` は `:297` から始まり、**seed と `Start-Process` はその前にある**（レビュー実測）。env を設定する行より前から始まる `try` を新設し、その `finally` で戻す。
      **入れ子を最小にする**（ラウンド 2 の訂正）: 同ファイルの `$env:SNOTRA_TRACE` は `Start-Process` の直後に即復元しており `try` を持たない。**`SNOTRA_CONFIG_DIR` も同じ扱いにはできない**が、**理由は「プロセスが生きている間 env を保たねばならない」ではない**（ラウンド 3 の機序是正）——子プロセスは生成時に環境を写すので、`Start-Process` の直後に復元しても本体は影響を受けない。`try` が要るのは**設定してから `Start-Process` へ到達するまでの間に throw しうる**（`Test-Path $ExePath` 等）ためであり、参照実装 `visual-check-colors.ps1:133,329` も `finally` まで保つ形を採っている。ゆえに `try` は要るが、**終端は「アプリを kill し終えた行」までとし、判定ロジック全体を包まない**（PowerShell の `exit` は `try` 内なら `finally` を必ず通ることをレビューが実測済み）。
      **実装時の裁定（ユーザー・2026-07-29）**: 終端は「kill し終えた行」ではなく **`Start-Process` の直後**とした。`smoke-egui.ps1` では env 設定行と kill 行のあいだが約 250 行あり、字面どおり包むと **#804 と無関係な行が全部差分に乗る**（再インデント）。上の機序是正のとおり子は生成時に環境を写すので、`Start-Process` を跨げば十分である。`smoke-startup.ps1` も同じ形で揃えた
- [x] `if (-not (Test-Path $cfgPath))` の条件を外し、**常に seed する**。seed の中身（ダミー exe + 必須セクション + `[[paths.scan]]`）は現行のまま動かさない
- [x] `-SeedConfig` パラメータを撤去する（常に seed するので意味を失う）
- [x] `$seededNow` を撤去し、`ResultsQuery` の既定を無条件で `"z"` にする。**実装の形まで指定する（ラウンド 3 の要対処）**: `param` ブロック（`:17`）の既定値を `""` → `"z"` へ変え、**`:114-115` の埋め戻し（`if ([string]::IsNullOrEmpty($ResultsQuery) -and $seededNow) { $ResultsQuery = "z" }`）はブロックごと消す**。**「`-and $seededNow` だけを落とす」形にしてはならない**——埋め戻しは `:125` の guard **より前**にあるため、`-ResultsQuery ''` が `"z"` へ書き戻されて guard へ到達せず、**下のフォールトインジェクション A が静かに死ぬ**（赤が出るはずの場面でアプリが起動してしまう）
- [x] **first-run の肯定的検査を smoke-egui にも置く**（ラウンド 2 の訂正）。使い捨てプロファイルを使う以上、**同じ経路を踏みうるのは smoke-startup だけではない**。`[config] ` 行の検査は parse 失敗を捕まえるが、**「config.toml が存在しない」分岐は捕まえない**（そのとき `[config] ` は出ない）ので別の検査が要る
- [x] **seed 健全性の検査を追加する**: 起動後、本体 stderr に `[config] ` 前置き行が無いことを確かめる。**`config.toml.bak` の不在を根拠にしない**（退避は best-effort ゆえ parse 失敗でも `.bak` が現れないことがある・`config.rs` の `backup_invalid`）。ログ自体が無い場合も赤にする（「観測できなかった」を合格と読ませない）。`visual-check-colors.ps1:143-159` の `Test-SeedHealth` と同型だが、**共有ヘルパーにはしない**（#843 の射程）
- [x] **env 到達性の肯定的検査を追加する**（ラウンド 3 の要対処——残骸掃除の項目が「seed 健全性と env 到達性の **2 判定**」に言及していたのに、**どの項目もこの判定を作っていなかった**）。起動後、プロファイル配下に `*.bin` が 1 件以上あることを確かめ、0 件なら赤にする。**env が効いていなければ本体は実 config を読んで実プロファイルへ書くので、ここには seed した `config.toml` しか残らない**——「env が届いていない」と「検査対象が出なかった」を切り分ける唯一の行である（参照実装 `visual-check-colors.ps1:292-304`。実測で出るのは `index.bin` で、索引 0 件でも書かれる）。**同じ検査を `smoke-startup.ps1` にも置く**——不変条件 1 の検知を手動の mtime 比較だけに委ねない（共有ヘルパーにはしない・(c)）。
      **`smoke-startup` では「ループ前に 1 回掃除し、ループ後に 1 回検査する」**（ラウンド 3 で明示）——各回で検査すると、`index.bin` が書かれる前に `Stop-Process -Force` された 1 回目が **false red** になりうる。根拠にした `visual-check-colors.ps1:292-304` の実測は**単発起動・強制 kill 後**のものであって、5 起動の各回について測ったものではない
- [x] `-RequireResults` パラメータを撤去し、**その guard を無条件の要求へ格上げする**（`ResultsQuery` が空なら常に throw）。理由は不変条件 3
- [x] **guard の「アプリを起動せずに赤を出せる」性質を保つ。** `docs/build-commands.md:161` はこれを**フォールトインジェクションの手順として明文化**している（`-RequireResults -ExePath <任意の既存ファイル>` で実機に触らず赤を出す）。無条件化後の注入口は **`-ResultsQuery ''` を明示的に渡すこと**——判定は起動前のままなので性質は失われない。**docs の手順もこの形へ書き換える**（フェーズ 4）
- [x] **撤去する変数を参照している文字列を全部書き換える**: throw メッセージ（`:126-133`）が `$seededNow` / `-SeedConfig` / APPDATA パスを埋め込んでいる。`Set-StrictMode -Version Latest` 下では**未定義変数の参照自体が別のエラーになる**ため、消し残すと「results 検査の要求」ではなく無関係な失敗が出る
- [x] skip の黄色 NOTE（`:481`）を撤去する（到達不能になるため）。
      **実装時の追加**: NOTE を消すだけだと `$resultsChecked` が偽の分岐が**黙って緑を返す**形で残る（#686 が塞いだ当の沈黙経路と同型）。到達不能なので実害は無いが、**到達したら赤で落とす**形に置き換えた
- [x] **ヘッダのコメントブロック（`:43-55`）を更新する**——`-SeedConfig` を「CI 用・config.toml 不在時のみ」と説明しており、撤去で偽になる
- [x] 冒頭の相互参照コメント（`:78-81`）を更新する。「あちらは検証用プロファイルへ書くので既存 config を気にせず」は**両方がプロファイルへ書くようになるため偽になる**。残す事実は「必須セクションの根拠が共通」と「同型ではない（`[[paths.scan]]` の有無）」＋ 共有化は #843 の射程であること

### フェーズ 2 — `smoke-startup.ps1` の env 化

- [x] 使い捨てプロファイル（`target/smoke-startup/profile`）を**ループの前に 1 回だけ** seed する。5 起動で共有する——**現在の CI の意味論を保つため**（いまも egui smoke が作った config があるので 5 起動とも first-run ではない。毎回作り直すと 5 回すべてが first-run になり、索引構築ぶん遅くなるうえ検証対象でない経路を測ることになる）
- [x] seed は**索引 0 件**の最小 TOML。**空ヘッダにしてはならない**——`HotkeyConfig` の `modifier` / `key` と `AppearanceConfig` の必須フィールドは `#[serde(default)]` を持たず（`config.rs:109-112` 実読・`Default` 実装の doc も「必須」と明記）、**空ヘッダは parse に失敗して破損復旧経路へ落ちる**。
      **「`visual-check-colors.ps1:108-130` をそのまま写す」と書いてはならない**（ラウンド 2 の訂正）——あの範囲は `$showOnStartup` / `$Color` という**あちらのスクリプト固有の変数**を埋め込んでおり、字面どおり写すと未定義変数参照になる。**写すのは構造だけ**: 値を持つ `[hotkey]`（`modifier = "Alt"` / `key = "Q"`）と `[appearance]`（`window_width = 600`）＋ **空の `[paths]` ヘッダ**（`PathsConfig.scan` は `#[serde(default)]` ゆえ空で索引 0 件になる）。`[general]` は**省略する**——既定値がそのまま望ましい挙動になる（レビュー実測）
- [x] **first-run 経路を踏ませない。** `Config::is_first_run()` は `!config_path.exists()`（`main.rs:141`）で、真なら `setup_first_run` が `launch_settings_process(--first-run)` を呼ぶ（`main.rs:331-332`）。**seed を起動より前に置くことがこれを防ぐ唯一の手段である**——順序を崩すと設定 GUI が spawn され、`Get-Process snotra` は**プロセス名が完全一致でないため `snotra-settings` を取り残す**
- [x] **first-run が起きていないことを肯定的に検査する**: trace に `cmd:launch_settings_process:` を含むイベントが 0 件であること。**既存の `*:error` フィルタでは構造的に見えない**——実際のイベント名は `:not_found` / `:spawned` / `:already_running` / `:exited` で（`commands/window.rs:53,74,87,123` 実読）**どれも `:error` で終わらない**。CI は `cargo build --release -p snotra` しか走らせず `snotra-settings.exe` が無いため `:not_found` になり、**false green のまま通る**
- [x] `$env:SNOTRA_CONFIG_DIR` の設定と `finally` での復元（`try` を新設する。`Restore-TraceEnv` はループ末尾の直接呼び出しで `try` を持たないため、形を写すだけでは不変条件 2 を満たさない）
- [x] **`release.yml` の消費を壊さないことを確認する**（ラウンド 2 の独立再導出）。`release.yml:83` が `smoke-startup.ps1 -ExePath target/release/snotra.exe` を呼び、その手前の `:53-54` で **`snotra-settings.exe` を同じ `target/release/` へビルドしている**（実読）。**first-run を踏むとここでだけ設定 GUI が実際に spawn され、5 起動ぶん残る**（CI の e2e では `snotra-settings.exe` が無いので `:not_found` に留まる）。不変条件 6 の検知手段は、この最悪ケースを想定して置く
- [x] **#786（待機ループが `[index-load]` 行で空振りする）は直さない**——本 issue の射程外。プロファイル分離で悪化も改善もしない（`[index-load]` は cache_hit=false でも出る）。ただし**悪化させていないことを実測で確認する**（検証フェーズ）

### フェーズ 3 — `e2e.yml`

- [x] `Run egui smoke` の引数から `-SeedConfig -RequireResults` を落とす
- [x] 順序制約のコメント**を撤去する。範囲は `:67-73` であって `:65-73` ではない**（ラウンド 2 の訂正）——`:65-66` は「flip 済み・env なしで良い理由（既定が egui であること自体が検証対象）」という**別トピック**で、巻き込んで消してはならない。**代わりに「プロファイルが分離されたので順序は自由である」ことを 1 行残し、そこに `#686` と `#804` の番号を含める**——コメントごと消すと、次の人が「なぜ以前は順序が要ったのか」を git 履歴からしか辿れなくなる（レビュー指摘: 撤去する 9 行は #686 を 2 箇所で参照しており、番号を引き継がないと辿れる先が消える）
- [x] `Run startup smoke` の first-run 受容の注記（`:77-80`）を更新する。**プロファイルが分かれたので「egui smoke が作った config がある」という前提が消える**
- [x] **ステップの順序は入れ替えない**（順序が自由になったこと自体は、入れ替えなくても成立する。無関係な差分を作らない）

### フェーズ 4 — `docs/build-commands.md`

- [x] 「スモーク運用メモ」の `smoke-egui` の bullet から `-SeedConfig` の説明（「config.toml 不在時のみ」「既存 config は上書きしない」）を落とし、使い捨てプロファイルの記述へ差し替える
- [x] results 検査の bullet から「どちらも無ければ results 検査は自動的に skip され、黄色 NOTE で理由を報告する」を落とす（skip が到達不能になる）
- [x] `:161` の `-RequireResults` bullet を書き換える。**順序制約と `-RequireResults` の記述は撤去し、「results 検査は無条件に要求される」へ**。#804 のスコープを名指ししている文（「env 化は #804 のスコープ」）も、本 issue で実現するので現在形へ直す
- [x] **フォールトインジェクションの手順を書き換える**（同 bullet）。現行は `-RequireResults -ExePath <任意の既存ファイル>` で「実機に触らず赤を出す」手順を明文化している。**この性質は保つが入口が変わる**ので `-ResultsQuery '' -ExePath <任意の既存ファイル>` へ差し替える
- [x] `CONTRIBUTING.md` に「results 窓 show/hide の trace 観測」への参照がある（`docs/build-commands.md` が言及）。**実在と整合を確認し、必要なら直す**
- [x] **`docs/build-commands.md:45`（カテゴリ C 節・「スモーク運用メモ」節の外）を直す**（ラウンド 2 の要対処）。「この 1 事例は `-RequireResults` が機構化した（#686・下記）」と書いており、フラグ撤去で**存在しない識別子を指す**。**同文の「下記」という序数的な指し先も同時に失効する**（ラウンド 3 の独立再導出——`-RequireResults` を grep しても、この指しが宙に浮くことには到達しない。`docs/development-principles.md`「列挙の完全性」の序数参照クラス）。**文ごと書き直す**。**`G-stale-identifiers` の母集団は `.claude/**` の md だけで `docs/**` を見ないため、`governance:check` では捕まらない**——手で直すしかない
- [x] **`docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` §3 を更新する**（ラウンド 2 の要対処・未確定 (g) の裁定に従う）

### フェーズ 5 — 検証

- [x] カテゴリ F: `npm run governance:check`（`docs/*.md` を編集するため。**`*.md` の編集で PostToolUse は沈黙する＝「何も走らなかった」**）
- [x] `npm test`（vitest: `.claude/hooks` + `.githooks` + `scripts`）——`.ps1` は対象外だが、`scripts/` 配下を触るため回して回帰が無いことを見る
- [x] **カテゴリ C（本命）**: `npm run smoke:egui -- -ExePath target/debug/snotra.exe` と `npm run smoke:startup -- -ExePath target/debug/snotra.exe -WaitMs 5000` を**実 config が在る開発機で**実行し、**両方が引数なしで緑になる**ことを確かめる（これが #804 の成果そのもの——従来は `-SeedConfig` が空振りして results 検査が skip されていた）
- [x] **実 config が汚れていないことを確かめる**: 実行前後で `%APPDATA%\Snotra\config.toml` の mtime とサイズが変わらないこと
- [x] **フォールトインジェクション A（起動を要さない）**: `pwsh -File scripts/smoke-egui.ps1 -ResultsQuery '' -ExePath <任意の既存ファイル>` が**アプリを起こさずに赤**を出すこと。撤去する `-RequireResults` が持っていた性質がそのまま残ることの実測（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）
- [x] **フォールトインジェクション B（seed 健全性）**: `smoke-egui.ps1` を **`target/fault-inject/` へ複製し**、seed の TOML を必須セクション欠落へ変異させて実行 → **`[config] ` を理由とする赤**が出ること。**稼働中のスクリプトを弱めない**（同 rule・複製に変異を当てる）。
      **複製先を `%TEMP%` にしてはならない（ラウンド 3 の要対処・実測）**——プロファイルは `$PSScriptRoot` 起点で決まるため、複製先が `%TEMP%\<dir>\` だとプロファイルは `%TEMP%\target\smoke-egui\profile` へ落ち、**リポジトリの `target/` の外に残って `cargo clean` が掃かない**（不変条件 7 の根拠が、この手順でだけ破れる）。`target/fault-inject/` へ置けば `..\target` は `target/target/` へ解決され、`target/` 配下に留まりつつ本番プロファイルとも衝突しない。`-ExePath` は cwd 基準（`:60` の `Test-Path`）なのでリポジトリルートから打つ限り複製後も無傷（実測）
- [ ] **本 PR 自身の CI（`e2e.yml` の smoke job）が緑になることを確認し、ログの中身まで読む**（緑が「検査が走った」を意味しない・#686）。`e2e.yml` の `paths:` は個別ファイル名の列挙で、本 PR が触る 3 ファイルを含むため自動起動する（レビュー実測）
- [x] **#786 を悪化させていないことの確認**: `smoke:startup` を既定引数で 1 回回し、失敗の様態が従来と同じ（`first_trace_ms` は埋まるが `event_count` が 0）であることを見る。**直さないが、変えてもいないことを実測で残す**
      **実測（2026-07-29）**: 既定引数でも 5 起動とも緑（`event_count=1`・`first_trace_ms` 130〜245ms）。計画が予期した失敗様態（`event_count` 0）は**再現しなかった**。ただし `[index-load] cache_hit=true` が stderr の 1 行目に出ており待機ループはそこで抜けている（#786 の症状はそのまま）——**悪化も改善もしていない**。
- [x] カテゴリ A・B・D・E は**該当なし**（`.rs` を 1 行も触らない・`.ts` を触らない・UI の見た目を変えない・`.githooks/` を触らない）

## 不変条件

1. **実ユーザーの `config.toml` を読みも書きもしない。** 分離の目的そのもの。**退避も復元も持たない**——持たないことが、異常終了しても実 config が壊れない構造的な保証である（`visual-check-colors.ps1:11-13` と同じ設計）。検知手段: 検証フェーズの mtime/サイズ確認
2. **`$env:SNOTRA_CONFIG_DIR` は `finally` で必ず戻す。** スクリプトが throw しても呼び出し元のシェルへ漏らさない。漏らすと**後続の無関係な操作が使い捨てプロファイルを見る**（同一シェルで `cargo run -p snotra` を打つ人が踏む）
3. **results 検査の skip は到達不能にする。** `-RequireResults` を撤去できるのは、**skip へ至る経路が構造的に消えるからである**——`ResultsQuery` の既定は無条件に `"z"` になり、seed は常に成立する。**「flag を消す」ことと「検出器を弱める」ことは別であり、本変更は後者ではない**: 従来はローカルで既定 skip（緩和）だったものが、**ローカルでも無条件の要求になる＝検出は強くなる**。緩和の前提（「ローカルでは索引を制御できないのが普通」）が、プロファイル分離で偽になったための格上げである
4. **seed が読めなかったことを、results 検査の失敗と区別できる。** seed が parse に失敗すると既定 config で起動し、索引が空になって `egui_results:show` が出ない——**症状は「results 検査の失敗」と同じだが原因が違う**。`[config] ` 行の検査を先に置くことで、赤の理由が正確になる（`Test-SeedHealth` と同じ思想）
5. **`smoke-startup` のプロファイルは 5 起動で共有する。** 毎回作り直すと 5 回すべてが first-run になり、**現在 CI が測っているもの（first-run でない起動）と別のものを測り始める**。カバレッジの変更は本 issue の目的ではない
6. **first-run 経路を踏まない。** seed は必ず起動より前に置く。踏むと `launch_settings_process(--first-run)` が走り、(a) ローカルでは設定 GUI が spawn されて `Get-Process snotra`（完全一致）に掛からず取り残される (b) CI では `snotra-settings.exe` が無いので `cmd:launch_settings_process:not_found` が出るが、**このイベント名は `:error` で終わらないため `smoke-startup` の `*:error` フィルタから構造的に見えず false green になる**。検知手段: trace に `cmd:launch_settings_process:` が 1 件も無いことの肯定的検査（フェーズ 2）
7. **新しい状態・プロセス・リソースを導入しない。** 追加するのはディレクトリ 1 つと env 変数の設定/復元だけ。env は `finally` で戻す（不変条件 2）。プロファイルディレクトリは `cargo clean` が掃く

## テスト方針

| 対象 | 手段 |
|---|---|
| 実 config を汚さないこと（不変条件 1） | 実行前後の mtime/サイズ比較（手動・検証フェーズ） |
| results 検査が実際に走ること | `smoke:egui` を**引数なしで**実行し、skip NOTE が出ずに緑になること。**これが従来との差そのもの** |
| skip 到達不能（不変条件 3） | 引数なし実行で `ResultsQuery` が空にならないこと＝ throw しないこと |
| 検査が赤を出せること | **フォールトインジェクション**（複製へ変異・空の scan ディレクトリ）で `egui_results:show` 未観測の赤を実測 |
| seed 健全性の検査（不変条件 4） | 同じ複製で seed を壊した TOML（必須セクション欠落）に差し替え、**`[config] ` を理由とする赤**が出ること |
| env の後始末（不変条件 2） | スクリプト実行後に `$env:SNOTRA_CONFIG_DIR` が残っていないこと |
| 既存の smoke が壊れていないこと | `smoke:startup` / `smoke:egui` の両方を実行（カテゴリ C） |

**ユニットテストは追加しない**——`.ps1` に対するテスト機構が現時点で無い（Pester 導入は #843 の射程・ローカルは Pester 3.4.0 で Pester 5 が要る）。本 issue の検証は上記の実行とフォールトインジェクションで行う。

## SPEC.md 更新要否

**不要。** 製品の挙動を 1 つも変えない（検証スクリプトと CI の変更のみ）。`SPEC.md` に smoke の記述は無い。

## 未確定（実装前に潰す）— ラウンド 3

- [x] **(a) `-SeedConfig` / `-RequireResults` を撤去するか残すか** — **裁定: 両方撤去し、results 検査の要求を無条件へ格上げする。**
      **根拠**: `docs/build-commands.md:161` が「env 化すれば `-SeedConfig` の制約・`-RequireResults`・この順序制約がまとめて不要になる」と**リポジトリ自身の記述として**予告している。かつ `-RequireResults` が opt-in だった理由は「ローカルでは索引を制御できないのが普通だから」であり、**プロファイル分離でその前提が偽になる**。
      **却下した代替案**: (i) flag を残して既定 ON にする——到達不能な分岐を残すだけで、読者に「skip がありうる」と誤解させる (ii) `-RequireResults` だけ残す——守る対象が無い検出器になる。
      **これは検出器の削除ではなく格上げである**（不変条件 3）。ローカル実行でも skip が赤になるため、検出は強くなる
- [x] **(b) `smoke-startup` のプロファイルを 5 起動で共有するか毎回作り直すか** — **裁定: 1 回 seed して 5 起動で共有する。**
      **根拠**: 現在 CI では egui smoke が作った config があるため 5 起動とも first-run ではない（`e2e.yml:77-80` が明記）。毎回作り直すと**測る対象が変わる**（5 回すべて first-run）。#804 の目的は実 config への依存を切ることであって、カバレッジの変更ではない。
      **却下した代替案**: 毎回作り直して first-run を 5 回測る——`e2e.yml:80` が「first-run は本 job の検証対象ではない」と明記しており、目的外
- [x] **(c) seed 健全性の検査を共有ヘルパーにするか** — **裁定: しない。各スクリプトにインラインで置く。**
      **根拠**: `ADR-config-dir-env-seam-rejected-alternatives.md` §3 が seed の共有ヘルパー化を却下しており、その却下理由の 1 つ（2 つの seed は同型でない）は今も真。共有は #843 の射程で、そこで `check:colors` を含む 3 本まとめて扱う方が「同型でないものをどう畳むか」を 1 度で決められる。
      **受容する残余**: `Test-SeedHealth` 相当が一時的に 2 箇所になる（#843 が畳む）
- [x] **(d) `e2e.yml` のステップ順序を入れ替えるか** — **裁定: 入れ替えない。** 順序が自由になったことは、順序を変えなくても成立する。無関係な差分を作らない（レビューの読み手に「なぜ入れ替えたのか」を考えさせない）
- [x] **(e) #786 を本 issue で直すか** — **裁定: 直さない。** 待機ループの空振りはプロファイル分離と独立で、分離しても `[index-load]` は出続ける。ただし**悪化していないことは実測で確かめる**（検証フェーズ）。修正は #843 の trace 収集共有時に自然に入る形

- [x] **(f) 順序制約の解消に `smoke-startup` の env 化は要るか**（独立再導出の再枠付け） — **裁定: 要らないが、やる。**
      **指摘の内容**: 順序制約を殺すのに必要なのは `smoke-egui` が自分のプロファイルを持つことだけである。`smoke-startup` が `%APPDATA%` に何を書こうと、`smoke-egui` には無関係になる。ゆえに `smoke-startup` の env 化は**独立した第 2 の便益**（実データ非接触）であり、**独立した費用**（first-run の罠・カバレッジが実 config から自明な seed へ縮む）を持つ。
      **裁定の理由**: #804 の表題が「smoke スクリプトを env 化し」であり、本文の「触る面」に `smoke-startup.ps1` を明記している。かつ**実 config を汚さないことが issue の主目的**（5 起動が `config.toml` を作るのが元凶）。費用の側は不変条件 6 と肯定的検査で塞いだ。
      **記録の意図**: 2 つの便益は分離可能なので、レビューで `smoke-startup` 側だけを差し戻す判断が取れる形にしておく

- [x] **(g) `ADR-config-dir-env-seam-rejected-alternatives.md` §3 をどう扱うか**（ラウンド 2 の要対処） — **裁定: §3 に「その後」の 1 段落を追記する。却下の判断は書き換えない。**
      **問題**: §3 は seed 共有ヘルパー化を却下した理由の 1 つとして **「`-RequireResults` ゲートに載る CI 経路」** を名指ししている。本 issue でそのフラグが消えるため、**理由文が存在しない識別子を指す**。ラウンド 1 では「同型でない」という**もう一方の理由**だけを検算して「§3 は覆らない」と結論しており、この面を見落としていた。
      **裁定**: ADR は決定の当時の文脈を残す文書なので**却下の判断そのものは書き換えない**（`docs/development-principles.md`「撤去（消す変更）の作法」の「ADR と設計書は当時の決定文脈ゆえ旧名のままでよい」）。代わりに §3 の末尾へ「**その後（#804）**: `-RequireResults` は撤去され、この理由は失効した。ただし『2 つの seed は同型でない』は今も真で、共有化の判断は #843 が引き取る」を追記する。
      **却下した代替案**: (i) 何もしない——現在形で偽の識別子を指す文が残る (ii) §3 を書き換える——当時の判断が読めなくなり、ADR の目的（否定の知識の保存）を損なう
- [x] **(h) `*:error` が 1 件も存在しない件をどうするか** — **裁定: 本 issue では扱わず、別 issue にする。**
      **実測**: 全 crate の `src/` を探して **`:error` で終わる trace イベント名は 0 件**（自分で再照合）。`smoke-startup.ps1:100` の `$_.event -like "*:error"` は**空回りしており、実効的な検査は #690 が足した「trace ≥ 1 件」だけ**である。
      **本 issue で扱わない理由**: 爆風半径が違う（trace の命名規約か smoke の判定基準の設計）。#804 は env 化であり、この空回りは env 化の前後で変わらない。
      **送り先: #845 として起票済み**（ユーザー裁定「issue 化で」・2026-07-29）。**#804 の実装で `smoke-startup.ps1` を触るが、`:100` の述語には手を入れない**——直すのは #845 の仕事であり、env 化のついでに変えると 2 つの変更が 1 つの diff に混ざる

## 実装中の是正（`/dry-check` と `code-reviewer`・2026-07-29）

- [x] **(H1) `Restore-ConfigDirEnv` の `$null` 分岐が到達不能だった** — `param([string]$Saved)` は束縛時に `$null` を `''` へ変換するため `if ($null -eq $Saved)` が常に偽になり、**未設定で呼ばれると空文字の env が残っていた**（自分で再実測）。`[string]::IsNullOrEmpty` へ変更。**同型の欠陥を持つ既存の `Restore-TraceEnv` も同時に直した**（同じ関数の隣にある対称の片割れ）。修正後、未設定ケースで env が残らないことを実測
- [x] **(M2) `smoke-startup` だけ seed 健全性の検査が無かった** — seed が parse に失敗しても、この smoke の他の判定（`*:error` 不在・trace ≥ 1・first-run 不発・`*.bin` の ∃）は**すべて通って緑になる**。ループ内へ `[config] ` 行の検査を追加し、**フォールトインジェクションで効いていることを実測**（他の判定が緑のまま、この 1 本だけが赤にした）
- [x] **(M3) 「`[config] ` は成功時には出ない」は全称表現として偽** — `config.rs:790`（duplicate instant command）と `:885`（system shortcut fallback）は parse 成功後に出る。現行 seed は踏まないが、**前提条件つきの記述へ書き換えた**（`AGENTS.md`「全称表現は前提条件とセットで書く」）
- [x] **(dry-check) ADR §3 の相互参照が片側だけだった** — 新設した `*.bin` 判定 2 箇所は `visual-check-colors.ps1` を指すが**逆向きの参照が無かった**（原本を直す人に写しが見えない）。seed 側にも同じ穴があった（`smoke-egui.ps1` の相互参照が `smoke-startup.ps1` を知らない）。双方向へ是正

**受け取らなかった指摘（記録）**: `visual-check-colors.ps1:179,331` は `SNOTRA_CONFIG_DIR` を**無条件に消して**おり、本 PR が「呼び出し元の値を壊さない」根拠として引いた相手方自身が壊している。計画が同ファイルを「相互参照コメントのみ」に限っているため**本 PR では直さない**——#843 の材料とする。

## セルフレビュー

**3 ラウンドで打ち切り**（収束条件は「未確定ゼロ **かつ** 差分ゼロ」。ラウンド 3 は**未確定ゼロを満たし、差分ゼロを満たさなかった**——12 行の変更が出た）。ラウンド 4 は行わない。

**各ラウンドが拾ったもの**:

| ラウンド | 主な拾い物 | 性質 |
|---|---|---|
| 1 | 相互参照コメント（`visual-check-colors.ps1:93`）・撤去識別子を埋め込む throw メッセージ | 影響範囲の漏れ |
| 2 | `try` の開始位置（`:297` は seed より後）・`visual-check-colors.ps1:108-130` の字面複製の罠・`e2e.yml:65-66` を巻き込む範囲指定・`release.yml` の設定 GUI 残留・ADR §3 の失効（未確定 (g)）・`*:error` 空回り（未確定 (h) → #845） | 実装手順の誤り + 送り先の確定 |
| 3 | `ResultsQuery` 埋め戻しの実装曖昧さ（FI-A が静かに死ぬ）・FI-B の `$PSScriptRoot` が `target/` の外へ落ちる・**宙に浮いていた「env 到達性」判定**・`Resolve-Path` 絶対化・`-ErrorAction SilentlyContinue`・表と本文の不整合（9 行/`:65-73`）・表に無い 3 ファイル | 実装の形の確定 + 自己整合 |

**打ち切り時点の残余（実装者が引き受けるもの）**:

1. **ラウンド 3 の 12 行はレビューを受けていない。** すべて「答えの決まった機械的訂正」（新しい未確定を 1 件も生んでいない）だが、**訂正そのものに対する独立検証は無い**。特に env 到達性の検査（`*.bin` の ∃）と FI-B の複製先（`target/fault-inject/`）は**ラウンド 3 で初めて計画に入った項目**であり、実装時に実測で確かめること
2. **`e2e.yml` の順序制約が本当に消えたことは、入れ替えて CI を緑にするまでは主張であって測定ではない**（独立再導出の指摘）。フェーズ 3 は「順序を入れ替えない」を選んでおり（未確定 (d)）、**この測定は行われない**。受容する残余として記録する
3. **`docs/superpowers/plans/*.md` に撤去識別子が約 25 件残る**が、`docs/superpowers/README.md` が当該ディレクトリを「歴史資料・鮮度維持の対象外」と宣言しているため更新しない（ラウンド 3 実測）
4. **`plan.md:33` の機序（「アプリのプロセスが生きている間は有効でなければならず」）はラウンド 3 で是正済み**——子プロセスは生成時に環境を写すので、その必要は無い。`try` が要る本当の理由（設定から `Start-Process` へ到達するまでの間に throw しうる）へ書き換えた。**結論（`finally` まで保つ）は変えていない**（参照実装 `visual-check-colors.ps1:133,329` と同形）
5. **独立再導出の「`-ResultsQuery` もパラメータごと撤去する」提案は却下した**（判断を会話にだけ残さないため明記する・#725）。**理由**: `-ResultsQuery ''` は `-RequireResults` を置き換える**フォールトインジェクションの注入口**であり（フェーズ 1・フェーズ 5 A・`docs/build-commands.md:161` にも手順として書く）、撤去すると「アプリを起こさずに赤を出す」性質を失う。提案の全文は `workspace/plan-review/independent-derivation.md`
6. **`target/fault-inject/` からのプロファイル解決は実測で確定した**（ラウンド 3）: `C:\workspace\Snotra\target\target\smoke-egui\profile`——リポジトリの `target/` 内に留まり、本番プロファイル（`target/smoke-egui/profile`）とも衝突しない
