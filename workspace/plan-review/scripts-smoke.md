## 問題なし

- `smoke-egui.ps1` の行番号引用は全件一致した: `:43-55`（ヘッダ）/ `:78-81`（相互参照コメント）/ `:126-133`（throw メッセージ）/ `:297`（`try {`）/ `:481`（skip NOTE `Write-Host "NOTE: results window coverage was SKIPPED..."`）。1 件のズレも無い。
- `visual-check-colors.ps1` の行番号引用も全件一致: `:54-58`（`$shotDir`/`$profileDir`・`target/` 配置理由）/ `:85-87`（`config.toml.bak`・`*.bin`・stderrLog の残骸削除）/ `:93`（`-SeedConfig` を指す相互参照コメント本体）/ `:108-130`（seed TOML の here-string〜`Set-Content`）/ `:143-159`（`Test-SeedHealth` 関数）。
- `smoke-startup.ps1` の行番号引用も一致: `:32-39`（`Restore-TraceEnv`、`try` を持たず直接呼び出し）/ `:100`（`$_.event -like "*:error"`）。
- `.github/workflows/e2e.yml`: `:65-66`（flip 済みの別トピック、消してはならない範囲）/ `:77-80`（first-run 受容の注記）は引用通り。
- `docs/build-commands.md`: `:45`（`-RequireResults` が機構化したという既存の言及）/ `:159-161`（skip NOTE・`-SeedConfig`・`-RequireResults` の運用メモ）は引用通り。
- `.github/workflows/release.yml:53-54`（`Build snotra-settings (release)`）/ `:83`（`Run startup smoke on release binary` が `target/release/snotra.exe` に対して実行）を実読で確認。不変条件 6 が想定する「CI では `snotra-settings.exe` が無いが release では在る」という非対称は実際に成立する。
- `main.rs:141` の `Config::is_first_run()` は `Config::load()` より前に呼ばれ（`main.rs:141-142`）、`config_dir()`（`SNOTRA_CONFIG_DIR` を読む唯一の導出点）を経由するため、**seed を起動（env 設定込み）より前に置けば first-run を確実に回避できる**という不変条件 6 の前提は成立する。
- `src-tauri/src/commands/window.rs:53,74,87,123` の trace イベント名は `:already_running` / `:not_found` / `:spawned` / `:exited` の 4 種のみで、**1 件も `:error` で終わらない**ことを確認した。plan の不変条件 6・既決 (h) の実測根拠と一致する。
- `snotra-core/src/config.rs` で `HotkeyConfig`（:109-112）・`AppearanceConfig.window_width`（:329, `#[serde(default)]` 無し）・`PathsConfig.scan`（:498-499, `#[serde(default)]` 有り）・`Config.general`（:94, 構造体レベル `#[serde(default)]`）を実読し、smoke-startup の新 seed TOML 設計（`[hotkey]`/`[appearance]`必須・`[paths]`空ヘッダ可・`[general]` 省略可）が正しいことを確認した。
- grep `-SeedConfig|-RequireResults|\$seededNow` を `scripts/` `package.json` `.github/workflows/` に対して実行した結果、現存コードでの参照は `.github/workflows/e2e.yml:75`（`-SeedConfig -RequireResults`）のみで、plan のフェーズ 3 がこれを撤去対象として挙げており漏れは無い。`docs/superpowers/plans/2026-07-25-*.md` にも多数の参照が残るが、これらは実行済み過去計画のスナップショットであり（ADR と同様「当時の決定文脈を残す」対象）、plan が更新対象に含めていないことは妥当。
- `CONTRIBUTING.md:92` は smoke 2 本の説明を持つが `-SeedConfig`/`-RequireResults` へは言及していないため、フェーズ4の「実在と整合を確認し、必要なら直す」は「変更不要」で解決する。
- 「触らない」対象（`bench-startup.ps1` / `measure-memory*.ps1` / `manual-smoke.ps1`）を grep したが、`SNOTRA_CONFIG_DIR` / `-SeedConfig` / `-RequireResults` への参照は無い（`measure-memory-stages.ps1:23` に `smoke-egui.ps1` への散文言及が 1 件あるのみで機能結合ではない）。「触らない」根拠は成立する。
- issue #804 本文（`gh issue view 804`）と plan の変更ファイル一覧を突き合わせたが、過不足は無い（触る面: `smoke-egui.ps1` / `smoke-startup.ps1` / `e2e.yml` / `docs/build-commands.md` を issue が明記し、plan はそれに `visual-check-colors.ps1` の 1 行修正と ADR §3 追記を正当な副作用として追加している）。YAGNI 逸脱・要求未達のいずれも見当たらない。
- 不変条件 3（skip の到達不能化）の設計そのもの（`-RequireResults` 撤去 → guard 無条件化）は正しい方向であることを確認した。ただし**実装の詳細**にあいまいさがあり、下記「要対処」を参照。

## 軽微な懸念

- `Set-StrictMode -Version Latest` かつ `$ErrorActionPreference = "Stop"` 下で `Remove-Item Env:<未設定の変数名>`（`-ErrorAction` 省略）を実測すると **エラーになる**（`pwsh -NoProfile` で実測: `Cannot find path 'Env:\...' because it does not exist.`）。既存の `$env:SNOTRA_TRACE` 復元パターン（`smoke-egui.ps1:216`）は `-ErrorAction SilentlyContinue` を明示している一方、plan の不変条件 2 の記述（`Remove-Item Env:SNOTRA_CONFIG_DIR`）は同フラグの明記を欠く。**新設する `try` が env 変数の代入行を最初の文として持つ設計であれば実害は無い**（代入前に例外が起きる余地が無いため）が、その保証は plan の文章からは読み取れず実装依存になる。`-ErrorAction SilentlyContinue` を明記するよう一文足すことを推奨する。
- `e2e.yml` フェーズ 3 の記述に内部的な数値の食い違いがある。ラウンド 2 の訂正で削除範囲を「`:67-73` であって `:65-73` ではない」と確定させたが（実際 67-73 は 7 行）、直後の括弧書き「撤去する **9 行** は #686 を 2 箇所で参照しており」は旧範囲 `:65-73`（9 行）のままの数字で、訂正後の 7 行と食い違う。変更ファイル一覧の表（plan.md:13）も「順序制約のコメント **9 行**（`:65-73`）」のままで、ラウンド 2 の訂正が伝播していない。削除すべき範囲自体（`:67-73`）は明記されており実装を誤らせる実害は小さいが、行数の記述は 1 か所整合させたほうがよい。

## 要対処

- **`$ResultsQuery` の既定を "z" にする実装方法があいまいで、誤読するとフォールトインジェクション A が機能しなくなる。** plan は「`$seededNow` を撤去し、`ResultsQuery` の既定を無条件で `"z"` にする」（plan.md:36）と書くのみで、(a) パラメータ宣言（`smoke-egui.ps1:17` 相当）の既定値そのものを `""` → `"z"` に変更し、現行の実行時フォールバック（`:113-116` の `if ([string]::IsNullOrEmpty($ResultsQuery) -and $seededNow) { $ResultsQuery = "z" }`）を丸ごと削除する、(b) その `if` ブロックを残したまま `-and $seededNow` 節だけを外す、のどちらとも読める。**(b) で実装すると `-ResultsQuery ''` を明示的に渡しても `[string]::IsNullOrEmpty('')` は真になるため、無条件で `"z"` に上書きされてしまう**（`$PSBoundParameters.ContainsKey('ResultsQuery')` のようなガードが無い限り「明示的な空文字」と「未指定」を区別できない）。これは不変条件 3 の検出器格上げそのものとは別に、フェーズ 1 の「無条件化後の注入口は `-ResultsQuery ''` を明示的に渡すこと」（plan.md:40）およびフェーズ 5 フォールトインジェクション A（`-ResultsQuery '' -ExePath <任意の既存ファイル>` が**アプリを起こさずに赤**を出す・plan.md:80）と直接矛盾する——(b) 実装では guard を素通りして実際にプロセスが起動してしまい、「起動を要さない」というフォールトインジェクションの性質が失われる。plan は (a)（パラメータ既定値そのものを変更し、実行時フォールバックのロジックを削除する）を明示すべき。
- **フォールトインジェクション B でプロファイルディレクトリがリポジトリの `target/` の外へ漏れることを実測で確認した（指定された確認）。** `smoke-egui.ps1` を一時ディレクトリへ複製して実行すると `$PSScriptRoot` はその複製先ディレクトリを指すため、`Join-Path $PSScriptRoot '..\target\smoke-egui\profile'` は最終的に **複製先ディレクトリの「兄弟」である `%TEMP%\target\smoke-egui\profile`**（リポジトリの `target/` の外）に解決される。実測（`pwsh -NoProfile`）:
  ```
  $tmp = %TEMP%\smoke-fi-test2-<guid>
  $cfgDir = Join-Path $PSScriptRoot "../target/smoke-egui/profile"   # $PSScriptRoot = $tmp
  New-Item -ItemType Directory -Force -Path $cfgDir
  → 実際に作成された場所: C:\Users\Eoh\AppData\Local\Temp\target\smoke-egui\profile
  → C:\workspace\Snotra\target\smoke-egui\profile は Test-Path で False（触られない）
  ```
  これは不変条件 7 の根拠「プロファイルディレクトリは `cargo clean` が掃く」と矛盾する——`cargo clean` はリポジトリの `target/` しか掃除せず、`%TEMP%\target\...` は複製ディレクトリを消しても道連れにならない（`..` で 1 段上がった先の兄弟パスであり、複製ディレクトリの子ではないため）。plan はこの経路（フォールトインジェクション B の profile 漏れ）を扱っておらず、`-ProfileDir` のような明示上書きパラメータも存在しない。対処案: (i) フェーズ 5 のフォールトインジェクション B の手順に「実行後 `%TEMP%\target` を削除する」を明記する、(ii) `$cfgDir` の基準を `$PSScriptRoot` 相対ではなく複製後も安定する形に変える、のいずれかを plan に追記すべき。
  なお `-ExePath` の相対パス解決（`smoke-egui.ps1:60,213` はいずれも `$PSScriptRoot` を経由せず、CWD 起点でそのまま `Test-Path`/`Start-Process` に渡す）は**この問題を持たない**——複製後も「リポジトリルートを CWD にして実行する」運用であれば `-ExePath` は正しく解決される。ただし plan はこの運用前提（CWD=リポジトリルート）を明示していない。

## 未検証

- フェーズ 5 の「実 config が汚れていないことを確かめる」（mtime/サイズ比較）・「本 PR 自身の CI が緑になることを確認しログを読む」・「#786 を悪化させていないことの確認」は、実装後の実行を伴う検証であり、計画レビュー段階（コード未変更）では実測できないため未検証とする。
- `smoke-startup.ps1` に新規追加される「`cmd:launch_settings_process:` を含むイベントが 0 件」の肯定的検査は、trace イベントの JSON フィールド名 (`event`) がパターンマッチに使えることは `trace.rs:54` で確認したが、**実装後の実機測定**（実際に first-run を踏ませた場合に赤くなること）は行っていない——計画レビューの対象はコード変更前のためフォールトインジェクションを実行できない。
