# plan: #804 smoke スクリプトを SNOTRA_CONFIG_DIR で env 化する

**方針**: `smoke-egui` / `smoke-startup` の検証プロファイルを `SNOTRA_CONFIG_DIR` で `target/` 配下へ分離する。**実 config の有無に依存する形が消えるので、そこから派生していた 3 つの制約（`-SeedConfig` の条件分岐・`-RequireResults`・`e2e.yml` の順序）も同時に落とす。** 参照実装は `scripts/visual-check-colors.ps1`（#803 で同じ分離を済ませている）。根拠と実測は `workspace/research.md`。

**seed ヘルパーの共有はしない**——`ADR-config-dir-env-seam-rejected-alternatives.md` §3 の却下理由のうち「2 つの seed は同型ではない」は今も真であり、共有化は #843 の射程である。本 issue は**書き先を変えるだけ**で、seed の中身は各スクリプトに残す。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `scripts/smoke-egui.ps1` | seed 先を `$env:APPDATA\Snotra` → `target/smoke-egui/profile` へ。**常に seed する**（不在時のみの条件を撤去）。`-SeedConfig` / `-RequireResults` の 2 パラメータを撤去し、**results 検査の要求を無条件へ格上げ**。seed 健全性の検査を追加。skip NOTE（`:481`）を撤去 |
| `scripts/smoke-startup.ps1` | 使い捨てプロファイルを 1 つ作り 5 起動で共有する（実 config を汚さない）。seed は**索引 0 件**の最小 TOML |
| `.github/workflows/e2e.yml` | `-SeedConfig -RequireResults` を落とし、順序制約のコメント 9 行（`:65-73`）と first-run 受容の注記（`:77-80`）を撤去 |
| `docs/build-commands.md` | 「スモーク運用メモ」の該当 3 bullet と `:161` の順序制約の記述を更新 |

**触らない（根拠つき）**:

- `scripts/visual-check-colors.ps1` — 既に分離済み（#803）。本 issue で触る理由が無い
- `scripts/bench-startup.ps1` / `measure-memory*.ps1` — 実 config を触るが**計測スクリプトであり検査ではない**（合否を返さない）。#804 が名指しするのは smoke 2 本。射程を広げない
- `scripts/manual-smoke.ps1` — 人が実 config で見るためのもので、分離すると目視の前提（実際の索引）が変わる
- `smoke-egui.ps1:78-81` の相互参照コメント — **文言だけ更新する**（下記）。ADR §3 は #804 では覆らないので、削除しない
- `SPEC.md` — 製品の挙動を変えない（検証機構のみ）

## 実装順序

### フェーズ 1 — `smoke-egui.ps1` の env 化

- [ ] `$cfgDir` を `$env:APPDATA\Snotra` から `Join-Path $PSScriptRoot '..\target\smoke-egui\profile'` へ変える。**`target/` 配下に置く**（`visual-check-colors.ps1:54-58` と同じ理由——`cargo clean` が掃くので新しい後始末機構を足さない。`CARGO_TARGET_DIR` 環境での受容残余は `ADR-config-dir-env-seam-rejected-alternatives.md` §4 に既出）
- [ ] **前回の残骸を消す**（`config.toml.bak` と `*.bin`）。`visual-check-colors.ps1:85-87` と同じ——残すと seed 健全性と env 到達性の 2 判定がどちらも古いファイルで空振り合格する
- [ ] `$env:SNOTRA_CONFIG_DIR` を設定し、**`finally` で必ず戻す**（`Remove-Item Env:SNOTRA_CONFIG_DIR`）。スクリプトが途中で throw しても環境変数を残さない
- [ ] `if (-not (Test-Path $cfgPath))` の条件を外し、**常に seed する**。seed の中身（ダミー exe + 必須セクション + `[[paths.scan]]`）は現行のまま動かさない
- [ ] `-SeedConfig` パラメータを撤去する（常に seed するので意味を失う）
- [ ] `$seededNow` を撤去し、`ResultsQuery` の既定を無条件で `"z"` にする
- [ ] **seed 健全性の検査を追加する**: 起動後、本体 stderr に `[config] ` 前置き行が無いことを確かめる。**`config.toml.bak` の不在を根拠にしない**（退避は best-effort ゆえ parse 失敗でも `.bak` が現れないことがある・`config.rs` の `backup_invalid`）。ログ自体が無い場合も赤にする（「観測できなかった」を合格と読ませない）。`visual-check-colors.ps1:143-159` の `Test-SeedHealth` と同型だが、**共有ヘルパーにはしない**（#843 の射程）
- [ ] `-RequireResults` パラメータを撤去し、**その guard を無条件の要求へ格上げする**（`ResultsQuery` が空なら常に throw）。理由は不変条件 3
- [ ] skip の黄色 NOTE（`:481`）を撤去する（到達不能になるため）
- [ ] 冒頭の相互参照コメント（`:78-81`）を更新する。「あちらは検証用プロファイルへ書くので既存 config を気にせず」は**両方がプロファイルへ書くようになるため偽になる**。残す事実は「必須セクションの根拠が共通」と「同型ではない（`[[paths.scan]]` の有無）」＋ 共有化は #843 の射程であること

### フェーズ 2 — `smoke-startup.ps1` の env 化

- [ ] 使い捨てプロファイル（`target/smoke-startup/profile`）を**ループの前に 1 回だけ** seed する。5 起動で共有する——**現在の CI の意味論を保つため**（いまも egui smoke が作った config があるので 5 起動とも first-run ではない。毎回作り直すと 5 回すべてが first-run になり、索引構築ぶん遅くなるうえ検証対象でない経路を測ることになる）
- [ ] seed は**索引 0 件**の最小 TOML（`[hotkey]` / `[appearance]` / `[general]` / `[paths]` の空ヘッダ）。startup smoke の判定は `*:error` の不在と trace ≥1 件だけで、索引に依存しない
- [ ] `$env:SNOTRA_CONFIG_DIR` の設定と `finally` での復元。**既存の `Restore-TraceEnv`（`:32-39`）と同じ形**で書く（このスクリプトは既に env の退避・復元パターンを持っている）
- [ ] **#786（待機ループが `[index-load]` 行で空振りする）は直さない**——本 issue の射程外。プロファイル分離で悪化も改善もしない（`[index-load]` は cache_hit=false でも出る）。ただし**悪化させていないことを実測で確認する**（検証フェーズ）

### フェーズ 3 — `e2e.yml`

- [ ] `Run egui smoke` の引数から `-SeedConfig -RequireResults` を落とす
- [ ] 順序制約のコメント 9 行（`:65-73`）を撤去する。**代わりに「プロファイルが分離されたので順序は自由である」ことを 1 行残す**——コメントごと消すと、次の人が「なぜ以前は順序が要ったのか」を git 履歴からしか辿れなくなる
- [ ] `Run startup smoke` の first-run 受容の注記（`:77-80`）を更新する。**プロファイルが分かれたので「egui smoke が作った config がある」という前提が消える**
- [ ] **ステップの順序は入れ替えない**（順序が自由になったこと自体は、入れ替えなくても成立する。無関係な差分を作らない）

### フェーズ 4 — `docs/build-commands.md`

- [ ] 「スモーク運用メモ」の `smoke-egui` の bullet から `-SeedConfig` の説明（「config.toml 不在時のみ」「既存 config は上書きしない」）を落とし、使い捨てプロファイルの記述へ差し替える
- [ ] results 検査の bullet から「どちらも無ければ results 検査は自動的に skip され、黄色 NOTE で理由を報告する」を落とす（skip が到達不能になる）
- [ ] `:161` の `-RequireResults` bullet を書き換える。**順序制約と `-RequireResults` の記述は撤去し、「results 検査は無条件に要求される」へ**。#804 のスコープを名指ししている文（「env 化は #804 のスコープ」）も、本 issue で実現するので現在形へ直す
- [ ] `CONTRIBUTING.md` に「results 窓 show/hide の trace 観測」への参照がある（`docs/build-commands.md` が言及）。**実在と整合を確認し、必要なら直す**

### フェーズ 5 — 検証

- [ ] カテゴリ F: `npm run governance:check`（`docs/*.md` を編集するため。**`*.md` の編集で PostToolUse は沈黙する＝「何も走らなかった」**）
- [ ] `npm test`（vitest: `.claude/hooks` + `.githooks` + `scripts`）——`.ps1` は対象外だが、`scripts/` 配下を触るため回して回帰が無いことを見る
- [ ] **カテゴリ C（本命）**: `npm run smoke:egui -- -ExePath target/debug/snotra.exe` と `npm run smoke:startup -- -ExePath target/debug/snotra.exe -WaitMs 5000` を**実 config が在る開発機で**実行し、**両方が引数なしで緑になる**ことを確かめる（これが #804 の成果そのもの——従来は `-SeedConfig` が空振りして results 検査が skip されていた）
- [ ] **実 config が汚れていないことを確かめる**: 実行前後で `%APPDATA%\Snotra\config.toml` の mtime とサイズが変わらないこと
- [ ] **フォールトインジェクション**（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）: `smoke-egui.ps1` を**一時ディレクトリへ複製し**、seed の `[[paths.scan]]` を空ディレクトリへ向けて実行 → `egui_results:show` が観測されず**赤になる**ことを確認する。**稼働中のスクリプトを弱めない**（同 rule）
- [ ] **#786 を悪化させていないことの確認**: `smoke:startup` を既定引数で 1 回回し、失敗の様態が従来と同じ（`first_trace_ms` は埋まるが `event_count` が 0）であることを見る。**直さないが、変えてもいないことを実測で残す**
- [ ] カテゴリ A・B・D・E は**該当なし**（`.rs` を 1 行も触らない・`.ts` を触らない・UI の見た目を変えない・`.githooks/` を触らない）

## 不変条件

1. **実ユーザーの `config.toml` を読みも書きもしない。** 分離の目的そのもの。**退避も復元も持たない**——持たないことが、異常終了しても実 config が壊れない構造的な保証である（`visual-check-colors.ps1:11-13` と同じ設計）。検知手段: 検証フェーズの mtime/サイズ確認
2. **`$env:SNOTRA_CONFIG_DIR` は `finally` で必ず戻す。** スクリプトが throw しても呼び出し元のシェルへ漏らさない。漏らすと**後続の無関係な操作が使い捨てプロファイルを見る**（同一シェルで `cargo run -p snotra` を打つ人が踏む）
3. **results 検査の skip は到達不能にする。** `-RequireResults` を撤去できるのは、**skip へ至る経路が構造的に消えるからである**——`ResultsQuery` の既定は無条件に `"z"` になり、seed は常に成立する。**「flag を消す」ことと「検出器を弱める」ことは別であり、本変更は後者ではない**: 従来はローカルで既定 skip（緩和）だったものが、**ローカルでも無条件の要求になる＝検出は強くなる**。緩和の前提（「ローカルでは索引を制御できないのが普通」）が、プロファイル分離で偽になったための格上げである
4. **seed が読めなかったことを、results 検査の失敗と区別できる。** seed が parse に失敗すると既定 config で起動し、索引が空になって `egui_results:show` が出ない——**症状は「results 検査の失敗」と同じだが原因が違う**。`[config] ` 行の検査を先に置くことで、赤の理由が正確になる（`Test-SeedHealth` と同じ思想）
5. **`smoke-startup` のプロファイルは 5 起動で共有する。** 毎回作り直すと 5 回すべてが first-run になり、**現在 CI が測っているもの（first-run でない起動）と別のものを測り始める**。カバレッジの変更は本 issue の目的ではない
6. **新しい状態・プロセス・リソースを導入しない。** 追加するのはディレクトリ 1 つと env 変数の設定/復元だけ。env は `finally` で戻す（不変条件 2）。プロファイルディレクトリは `cargo clean` が掃く

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

## 未確定（実装前に潰す）— ラウンド 1

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

## セルフレビュー

（収束または打ち切りの後に 1 度だけ記入する）
