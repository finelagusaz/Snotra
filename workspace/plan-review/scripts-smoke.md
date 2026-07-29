## 問題なし

- 参照の全列挙: `smoke-egui.ps1` 内の `$env:APPDATA`/`$cfgPath`/`$cfgDir`/`$seededNow`/`SeedConfig`/`RequireResults`/`ResultsQuery` を grep した結果は param 宣言（5, 11-22）・冒頭ヘッダコメント（52-54）・seed ブロック（64-116）・ResultsQuery 既定ロジック（113-116）・`-RequireResults` guard 本体とメッセージ（118-134）・`Get-LetterVk` エラー文（150）・results 検査本体（373-401）・skip NOTE（481）の全 8 箇所。plan.md の変更ファイル一覧・実装順序はこのうち大半をカバーするが、52-54（ヘッダコメント）と 126-133（guard のメッセージ本文）が未itemize（詳細は「要対処」）
- `scripts/bench-startup.ps1` / `measure-memory*.ps1` / `manual-smoke.ps1` を grep しても `APPDATA`/`config.toml`/`smoke-egui`/`smoke-startup` への依存記述は無い（`measure-memory.ps1:24` は config.toml の推奨設定を書くだけで smoke の副作用への依存ではない）。plan の「触らない」判断（bench-startup/measure-memory は計測でありスコープ外、manual-smoke は実 config 前提が目的）は妥当
- `CONTRIBUTING.md:92` の「results 窓 show/hide の trace 観測」の記述は `-SeedConfig`/`-RequireResults` を名指ししておらず、#804 後も文として真であり続ける。plan の「実在と整合を確認し、必要なら直す」という慎重な言い方と整合し、実際には直す必要が無いことを確認した
- `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` §3 の「`-RequireResults` ゲート」への言及は決定時点の記録であり、ADR は他の SSOT と異なり後追い更新を前提としない文書である。plan.md:21 が明記する「ADR §3 は #804 では覆らないので、削除しない」は妥当
- PowerShell の `exit` が `finally` を飛ばすかを実機で検証した（`pwsh -NoProfile -File` 経由・pwsh 7.6.4）: `try { try { exit 1 } finally { ... } } finally { ... }` で両方の `finally` が実行されてから終了することを確認した。**この環境では `exit` は `finally` を飛ばさない**——ただし「要対処」に書く通り、問題は `exit`/`finally` の相互作用ではなく、`finally` の**スコープが env-set の時点を覆っていない**という構造の方にある
- `smoke-startup.ps1` の seed 案のうち `[paths]` 空ヘッダは妥当（`PathsConfig.scan` は `#[serde(default)]`・`snotra-core/src/config.rs` 実測）。`[general]` も全フィールドに `#[serde(default = ...)]` があるため空ヘッダ/省略のどちらでも安全（`config.rs:151-166`）

## 軽微な懸念

- `docs/build-commands.md:45` の「この 1 事例は `-RequireResults` が機構化した（#686・**下記**）」は `:161` の bullet への前方参照。phase 4 のチェックリストは `:161` の書き換えだけを名指ししており（plan.md:57）、`:45` のこの一文は未itemize。`:161` が `-RequireResults` という名前を持たない記述へ変わると、「下記」を読みに行った読者が該当する解説を見つけられなくなる
- `smoke-egui.ps1:43-55` の冒頭ヘッダコメント（特に 52-54 行「- -SeedConfig（CI 用）: config.toml 不在時のみ...」）は `-SeedConfig` の挙動を説明しているが、plan の実装順序が明示的に更新対象として挙げているのは「冒頭の相互参照コメント（`:78-81`）」だけ（plan.md:37）。78-81 とは別の箇所であり、このヘッダも同時に古くなる
- `smoke-egui.ps1:477-482` の `if ($resultsChecked) {...} else {...}` について、plan は `:481` の NOTE 行だけを撤去対象に挙げる（plan.md:36）。`-RequireResults` の guard が無条件化され `ResultsQuery` が常に非空になった後は、この成功パスに到達する時点で `$resultsChecked` は必ず true になり、`else` 分岐（480-482、NOTE を除いた残り）が到達不能な死コードとして残る。動作に影響は無いが、分岐ごと整理するかどうかは plan で判断されていない
- seed 健全性の検査（不変条件 4）を追加する具体的な挿入位置が plan で確定していない（plan.md:34 は「起動後」としか書かない）。`hotkey:registered` 待ち・`egui_show:done` 待ち・results 検査のどの前に置くかで、「seed 不成立」と「results 検査失敗」を区別できる範囲が変わる——最終の失敗一覧の末尾でしか見ないなら、既に他の失敗理由（例: results 未観測）が `$failures` に積まれた後になり、不変条件 4 が意図する「原因の正確な切り分け」が弱まる

## 要対処

- **`smoke-startup.ps1` の seed TOML 案はそのままでは parse に失敗する。** plan.md:42「`[hotkey]` / `[appearance]` / `[general]` / `[paths]` の空ヘッダ」は `[hotkey]` と `[appearance]` も含めて空ヘッダとしているが、`snotra-core/src/config.rs` を実測すると `HotkeyConfig { modifier: String, key: String }`（109-112 行、フィールドに `#[serde(default)]` 無し）と `AppearanceConfig.window_width: u32`（329 行、同じく無し）は必須フィールドで、`Default` 実装のドキュメントコメント自身が「同フィールドは serde の既定関数を持たない（`[appearance]` に無い TOML は parse 失敗 →`.bak` 退避経路へ落ちる」と明記する（347-349 行）。空の `[hotkey]`/`[appearance]` ヘッダは必須フィールド欠落で parse 失敗し、`smoke-egui.ps1:82-89` と `visual-check-colors.ps1:89-97` が明示的に避けている「破損復旧経路」（`.bak` 退避＋`Config::default()`）をまさに踏む。さらに `Config::default_scan_paths()`（config.rs:647-661）は実在する Start Menu の `.lnk` を実際にスキャンするため、parse 失敗時は「索引 0 件」どころか実ファイルシステムを走査する既定パスへ落ちる。加えてこの失敗は `eprintln!("[config] ...")`（config.rs:943/959/970/986/990）であり `[trace]` 形式ではないため、`smoke-startup.ps1:100` の `*:error` イベント検査はこれを一切捕捉できず、**バグが沈黙で通る**。修正: 他 2 スクリプトと同じ最小有効値（`modifier="Alt"` / `key="Q"` / `window_width=600`）を含める
- **`finally` による env 復元が、実際にリスクがある区間を構造的にカバーしていない。**
  - `smoke-egui.ps1`: seed（常時実行化される 65-111 行相当）と `$env:SNOTRA_CONFIG_DIR` の設定は、唯一存在する `try` ブロック（297 行開始）より**前**に来る必要がある（`Start-Process` は 213 行で `try` の外）。213 行の `Start-Process`・202 行の `Stop-Process`・136 行の `Add-Type` はいずれも現在 `try`/`finally` の外にあり、ここで例外が出れば env は復元されない。plan.md:30「`finally` で必ず戻す」は、**env を設定する箇所より前から `try` のスコープを広げる**ことを明示していない
  - `smoke-startup.ps1`: plan.md:43「既存の `Restore-TraceEnv`（`:32-39`）と同じ形で書く」とあるが、そのパターン自体が `try`/`finally` を使っていない——`Restore-TraceEnv` は `for` ループ本体（77 行）からの単なる関数呼び出しで、ファイル全体に `try` ブロックが 1 つも無い。「同じ形」で `SNOTRA_CONFIG_DIR` を書けば、不変条件 2 が要求する例外安全性を満たさないまま実装が完了してしまう。ループ全体（または最低でもスクリプト本体）を `try { ... } finally { Remove-Item Env:SNOTRA_CONFIG_DIR ... }` で包む、既存パターンとは別の構造が必要である
- **撤去される変数・パラメータを参照するメッセージ文字列が itemize されていない。** `-RequireResults` guard の throw メッセージ（`smoke-egui.ps1:126-133`）は `$seededNow`・`-SeedConfig`・`$(Join-Path $env:APPDATA 'Snotra\config.toml')` を参照する。plan.md:35 は guard 本体を「無条件の要求へ格上げする」とだけ書き、このメッセージ文字列の書き換えを明示しない。`Set-StrictMode -Version Latest`（smoke-egui.ps1:57）下で `$seededNow`/`$SeedConfig` が未定義のまま参照されると、`-ResultsQuery ""` を明示指定したときにこの throw が「変数が設定されていません」エラーで落ち、本来伝えるべき診断（skip が要求されたが到達不能）を隠してしまう

## 未検証

- 実機での `npm run smoke:egui` / `npm run smoke:startup` 実行結果（plan フェーズ5の検証項目） — 実装がまだ無いため確認していない
- `e2e.yml` 変更後の GitHub Actions 上の実際のジョブ挙動（ステップ順序自由化の効果） — ローカルのファイル読解のみで、CI 実行では確認していない
- `docs/build-commands.md` 全体で `:45`/`:159-161` 以外に `-SeedConfig`/`-RequireResults` を暗黙の前提とする記述が無いか — grep によるキーワード一致は確認済みだが、キーワードを含まない意味的な前提依存（例: 「順序に注意」的な言い回し）までは網羅していない
