# 逆向きの監査（歴史）— `workspace/plan.md` レビュー

**枠の道具**: `git log -S` / `-G` / `git show` / `gh pr view` / `gh issue view` / `gh api`。
**問い**: 計画が変えようとしている各行は、過去に意図してその形で置かれたのではないか。

**総括**: 計画が消そうとしている 3 系統（待たない kill・手書きの env 復元・緩い `var_os`）について、**「そうしないことを意図した」記述は 1 件も見つからなかった**。うち 1 件（`var_os` の流儀）だけは「既存に揃えよ」という伝播の記述が計画書に残っており、無記録ではない。計画の方向そのものへの反対所見は無い。

**一方、計画が触れていない行で、同じ変更によって意味が変わる／壊れるものを 5 件見つけた**（E-1〜E-5）。うち E-3 は実測で確認した破壊であり、Phase 1 の作業項目のままでは既存単体検査が binding error で落ちる。

---

## 分類つき所見一覧

| # | 分類 | 要旨 |
|---|---|---|
| E-3 | **要対処** | `Stop-SnotraProcessAndWait` の型付き `[System.Diagnostics.Process]` 引数が、既存単体検査の偽プロセス fixture を binding error で落とす（**実測済み**） |
| E-1 | **要対処** | `Tests.ps1` の trace 写しが名乗る不変条件（「書き終えたものを写す」）は今日成立していない。Phase 1 で初めて成立する＝**`research.md` の artifact 由来の数値は Phase 1 の前後で比較できない** |
| E-2 | **要対処** | 新ヘルパへ 3 箇所を畳むと、`Policy Stop` の `Stop-Process` の**エラーチャネルが黙る**（現行は `-ErrorAction` 無し・他 2 箇所は `SilentlyContinue`） |
| B-2 | **要対処** | `src-tauri/src/trace.rs:18` の doc が `env_flag` を「受理仕様の **SSOT**」と名乗っており、47bc5b9 の commit message も「集約」と書いている。写しを増やす計画に `trace.rs` が変更対象として入っていない |
| A-2 | **軽微** | `plan.md:222` の「#890 の残余として同じ非対称が既に記録されている」を **PR #890 は記録していない**（記録されているのは `-WindowStyle` / `-NoNewWindow` の別の非対称）。同行の「共有ヘルパも通っていない」も `visual-check-colors.ps1` については誤り |
| C-2 | **軽微** | `repro-pester-flake.ps1` は `Import-Module` を **1 つも持たない**（実測 0 件）。`Invoke-SnotraEnvironment` を「直接使う」案は、この harness に初めてモジュール結合を持ち込む——同スクリプトの `.DESCRIPTION` が明示的に避けている方向である |
| E-4 | **軽微** | `smoke-egui.ps1:139` の `Start-Sleep 300`（#853 以前からの持ち越し）は Phase 1 で二重の待ちになる。計画は触れていない |
| B-3 | **軽微** | `input.rs:29-33` の `OnceLock` は「窓イベントごとに問われるから」という明記された理由で在る。`env_flag`（`std::env::var` = String 割り当て）へ寄せるとき、このキャッシュを保つことが計画に書かれていない |
| E-5 | **⚠️** | `Policy Stop` は `Get-Process -Name 'snotra'` ＝**自分が起動していない他人のプロセス**も掴む。待ちを内側へ入れると N 個 × `TimeoutMs` まで塞ぐ経路が生まれる（現行は fire-and-forget） |
| A-3 | **⚠️** | `smoke-egui.ps1` の `WaitForExit` を入れた **#904 は、その本文でこの待ちに一言も触れていない**。帰属コメント「#755/#801 是正 B」自体は正しい（#904 が両 issue の修正 PR である）が、待ちの根拠は PR 本文ではなくコード内コメントにしか無い |
| D-2 | **⚠️** | `Reject` の厳しさの根拠は throw 文言そのものにしか無い（PR/issue に議論の記録が無い）。計画の判断は正しいが、**根拠は「当時明示された理由」ではなく「文言からの再構成」である** |

---

## A — `Stop-Process -Force` を待たない形は、いつ・なぜ入ったか

### A-1（**発見**）: 待たないことが意図的だった形跡は無い。#904 は同じファイルの同じ It ブロックを触りながら横展開しなかった

**出自の確定**（`-S` は移設と新設を区別しないため、`--diff-filter=A` と実際の hunk で裏を取った）。

```
$ git log --diff-filter=A --oneline -- scripts/lib/SnotraSmoke.Tests.ps1
5079c37 refactor: smoke PowerShell の配管を共有化 (#853)
```

- **seed の It の `finally`（現 `Tests.ps1:378-383`）** — `5079c37`（#853・2026-07-30 17:17）で**新規に書かれた**。`git show 5079c37 -- scripts/lib/SnotraSmoke.Tests.ps1` の該当 hunk（行 239-245）はすべて `+` である
- **キャレットの It の `finally`（現 `Tests.ps1:500-503`）** — `6e7ce13`（#852・2026-07-30 18:02）で**新規に書かれた**。`git show 6e7ce13 -- scripts/lib/SnotraSmoke.Tests.ps1` の行 105-109 がすべて `+`

どちらのコミットメッセージにも、`Stop-Process` の待ち・single-instance・終了同期への言及は無い。両サイトに**コメントは 1 行も無い**（`sed -n 378,383p` / `sed -n 500,503p` で確認）。

**当時のリポジトリの習慣はむしろ「殺したら少し寝る」だった。** #853 以前の姿:

```
$ git show 5079c37^:scripts/smoke-egui.ps1 | grep -n -A2 'Get-Process snotra'
209:# 既存インスタンスは single-instance 転送で smoke を汚すため停止（smoke-startup.ps1 と同じ前提）
210:Get-Process snotra -ErrorAction SilentlyContinue | Stop-Process -Force
211:Start-Sleep -Milliseconds 300
```

```
$ git show 5079c37^:scripts/smoke-startup.ps1 | grep -n -A3 'Stop-Process -Id'
142:    Stop-Process -Id $proc.Id -Force
143:  }
145:  Start-Sleep -Milliseconds 120
```

つまり **#853/#852 で新設された Pester 側だけが、sleep も待ちも持たずに生まれた**。「待たない」を選んだのではなく、既存 2 本が持っていた粗い待ちが移植されなかった。

### `WaitForExit` がなぜ Pester へ横展開されなかったか — 読み取れる

`smoke-egui.ps1:463` の待ちは `0083620`（#904・2026-08-04）で入った（`git log -S'scenario1ExitWaitMs' -- scripts/smoke-egui.ps1` の唯一のヒット。`-S'WaitForExit(5000)'` は 0 件——実コードが変数参照だからである）。**コメントの帰属「#755/#801 是正 B」は正しい**——#904 の本文が「#755 …と #801 …は同じ食い違いの 2 分岐」と述べており、#904 は両 issue の修正 PR である。

`gh pr view 904` の本文（= 0083620 のコミットメッセージ）を `single-instance` / `WaitForExit` / `終了を待` で grep して **0 件**。つまり**この待ちは PR 本文で一度も説明されていない**。diffstat が読み方を裏づける:

```
$ git show 0083620 --stat
 SPEC.md / docs/adr/… / src-tauri/src/egui_shell/*.rs（6 ファイル）
 scripts/smoke-egui.ps1                            | 182 ++++++++
 scripts/lib/SnotraSmoke.Tests.ps1                 |  21 +-
```

**#904 は描画の修正であり、`smoke-egui.ps1` へシナリオ 2 を足すために「シナリオ 1 の終了を待つ」必要が副次的に生じたもの**である。対称性の走査ではないので、他所へ配る動機が無かった。

**決定的な証拠**: その `0083620` は **`SnotraSmoke.Tests.ps1` を触っており、しかも触ったのは問題の 2 つの It ブロックそのものである**。

```
$ git show 0083620 -- scripts/lib/SnotraSmoke.Tests.ps1
@@ -332,8 +345,7 @@ Describe '実機配管' -Tag Integration -Skip:$sessionLocked {
     It '生成した seed を本体が parse して…'
-        $created = New-SnotraVerificationProfile -ProfileDir $profile -AdditionalSections @'
-[general]
+        $created = New-SnotraVerificationProfile -ProfileDir $profile -GeneralSection @'
@@ -383,8 +395,7 @@ auto_hide_on_focus_lost = false
```

同じコミットで `smoke-egui.ps1` に機序のコメントつきの待ちを書きながら、**同じ機序を持つ 2 つの `finally` の数十行上を編集していて、そこへは配らなかった**。これは意図的な非対称ではなく、注意が向かなかったことの直接の痕跡である。

**→ 計画 Phase 1 は、意図して置かれた形を覆すものではない。**

### A-2（**軽微**）: `plan.md:222` の #890 引用が、#890 の記録と一致しない

計画 222 行:

> `bench-startup.ps1` / `measure-memory-stages.ps1` / `visual-check-colors.ps1` は CI のゲートではなく共有ヘルパも通っていないため今回は触らない（#890 の残余として同じ非対称が既に記録されている）

`gh issue view 890`（PR #890・`comments: 0`・レビューコメント 0 件）の本文が記録している非対称は**別物**である:

> `-WindowStyle` と `-NoNewWindow` は `Start-Process` の排他な引数なので分岐する。`-NoNewWindow`（`visual-check-colors.ps1` のみ）は…**同じ形の経路として列挙したが、欠陥は持たないので変更していない**

`Stop-Process` の終了待ちについては #890 は一言も書いていない。加えて:

```
$ grep -ln "SnotraSmoke" scripts/*.ps1
scripts/manual-smoke.ps1
scripts/repro-pester-flake.ps1
scripts/run-pester.ps1
scripts/smoke-egui.ps1
scripts/smoke-startup.ps1
scripts/visual-check-colors.ps1
```

`visual-check-colors.ps1` は**共有ヘルパを通っている**（`Invoke-SnotraEnvironment` を `visual-check-colors.ps1:204` で呼ぶ）。計画自身も 201 行の `/symmetric-check` 表で `visual-check-colors.ps1:225` を `Start-SnotraProcess` の呼び出し点として列挙しており、222 行と矛盾する。

**判定の是非は変わらない**（起動 1 回ゆえ衝突相手が居ない、は独立に正しい）。**誤っているのは根拠の帰属**である。「既に記録されている」は不在の記録を根拠にした主張なので、書き換えるか落とすのが筋。

### A で実行して見つからなかったもの

| 探したもの | コマンド | 結果 |
|---|---|---|
| 待たないことを意図した記述 | `git show 5079c37 -- scripts/lib/SnotraSmoke.Tests.ps1` / `git show 6e7ce13 -- …` の全文 | **0 件**（コメントも無い） |
| PR 本文での言及 | `gh pr view 853` / `gh pr view 904` を `WaitForExit` / `single-instance` / `終了を待` で grep | **0 件** |
| issue 側の記録 | `gh issue view 872` / `936` を `既に起動\|single-instance\|Stop-Process` で grep | **0 件**（`research.md` 発見 2 の「一度も分類されなかった」を裏づける） |

---

## B — `var_os().is_some()` と `env_flag` の非対称は意図か歴史か

### 前後関係（**`env_flag` が 7 箇所すべてより先に存在した**）

```
$ git log --diff-filter=A --oneline -- src-tauri/src/trace.rs
fbb124d refactor(src-tauri): main.rs setup の分割と重複ロジックの集約 (#449)   2026-07-05

$ git log --oneline -S'env_flag' | tail -1
47bc5b9 fix(webview2): TrySuspend を SetIsVisible(false) で成立させ frontend hide にも拡張 (#556)   2026-07-18
```

`47bc5b9` は `trace_enabled` の内側から `1|true|yes|on` の判定を切り出して `env_flag` にした（`git show 47bc5b9 -- src-tauri/src/trace.rs` の行 38-62）。そのコミットメッセージが**意図を明記している**:

> `SNOTRA_DISABLE_SUSPEND=1`: E2E 専用エスケープハッチ…**真偽 env 解析は trace::env_flag に集約**

対して `snotra-egui-runtime` 側で `var_os` に触れたコミットは:

```
$ git log --oneline -S'var_os' -- snotra-egui-runtime/
8a03252 (#938)  2026-08-05   input.rs      SNOTRA_EGUI_INPUT_TRACE
0e10ea8 (#741)  2026-07-26
a21d816 (#709)  2026-07-26   runtime.rs    SNOTRA_EGUI_REPAINT_TRACE
d1bd98c (#655)  2026-07-24   renderer.rs   SNOTRA_EGUI_PAINT_TRACE
b59f1fe (#627)  2026-07-22   （最古）
```

**最古のヒット `b59f1fe`（2026-07-22）でも `env_flag`（2026-07-18）より後である。** ゆえに「厳しい形が後から来て緩い形を残した」ではなく、**厳しい形が既にあるところへ緩い形が 5 回積み増された**。

### 「値は何でもよい」を意図した記述 — 見つからなかった

7 箇所すべてのコメントを現在形と導入時の両方で読んだ。

- `input.rs:26-33`（#938・最新）— **`trace.rs` を名指ししている**: 「キャッシュの形は `src-tauri/src/trace.rs` の `trace_enabled` と同じ」。**キャッシュの形は写したが、値の意味論は写さなかった**。差があることへの言及は無い
- `renderer.rs:74-76`（#655）— 「env **未設定**なら Instant も取らない」。書き手の心象は「設定 / 未設定」の二値であり、値の受理仕様は考慮の外
- `repaint.rs:192-197` / `runtime.rs:276-279` / `runtime.rs:448-456` / `windows_ime.rs:92-100` / `:201-209` — いずれも計器の**意味**の説明のみ。env の受理値には一言も触れていない

**唯一の「意図の記録」は伝播の指示であって、値の意味論の判断ではない。**

```
$ grep -n 'SNOTRA_EGUI_PAINT_TRACE' docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md
859: … 既存の `SNOTRA_EGUI_IME_TRACE`（`windows_ime.rs`）と同じ env + `eprintln!` の流儀に揃える。
866:        let trace = std::env::var_os("SNOTRA_EGUI_PAINT_TRACE").is_some();
919: $env:SNOTRA_EGUI_MAIN=1; $env:SNOTRA_EGUI_PAINT_TRACE=1; $env:SNOTRA_TRACE=1
```

計画書が `var_os(...).is_some()` を**逐語で指定**しており、その理由は「既存（`windows_ime.rs`）に揃える」である。`src-tauri` 側との比較は行われていない。

**→ 判定: 意図的な非対称ではない。`windows_ime.rs`（SU1 期）の流儀が 4 回コピーされた歴史の産物である。** ただし「無記録」ではなく「揃えよ」の記録が 1 件在るので、計画は「揃え先を `src-tauri` へ変える」ことになる——その旨を書いておくと後任が迷わない。

### B-2（**要対処**）: `trace.rs` の「SSOT」表明が写しの追加で嘘になる

`src-tauri/src/trace.rs:17-19`（現在形）:

```
/// 真偽 env フラグの共通解析（受理値: `1`/`true`/`yes`/`on`、trim + ASCII 小文字化）。
/// `trace_enabled`（`SNOTRA_TRACE`）等の env フラグが共有する受理仕様の SSOT。
```

計画 `plan.md:97-102` の変更ファイル表に **`src-tauri/src/trace.rs` が無い**。`plan.md:108` は「双方の doc に互いを名指しで書く」と言うが、変更対象に入っていないので**書かれない**。結果、写しを 2 つ持ちながら片方が「SSOT」を名乗り続ける。`AGENTS.md`「検証の作法」の全称表現の則（「実装より強い主張になった瞬間に嘘になる」）に真正面から当たる。

**→ `trace.rs` を変更ファイル表へ足し、SSOT 表明を「受理仕様の正本。`snotra-egui-runtime` に依存辺を避けるための写しが 1 つある（`input.rs`）」の形へ直す。**

### B-3（**軽微**）: `input.rs` の `OnceLock` は理由つきで在る

`input.rs:30-33` が理由を明記している:

> **一度だけ読む**。この述語は窓イベントごと（マウス移動を含む）とフレームごとに問われるため、`env::var_os` の割り当てを毎回払うと、計器を**切っていても**出荷バイナリのホットパスに費用が残る。

`env_flag` は `std::env::var`（`String` 割り当て）を無条件に行う。計画 `plan.md:102` の「7 箇所を新 `env_flag` へ寄せる」は、この `OnceLock` を保つのか包み直すのかを書いていない。**`input_trace_enabled` の `OnceLock` の内側だけを差し替える**旨を作業項目へ明記すべき。

（`windows_ime.rs:100` / `:209` は元から毎回 `var_os` を呼んでおり、`env_flag` へ寄せても割り当て回数は変わらない——退行にはならない。）

### B で見つからなかったもの

```
$ grep -rn "SNOTRA_EGUI_\(INPUT\|PAINT\|WAKE\|REPAINT\|IME\)_TRACE" \
    --include=*.md --include=*.ps1 --include=*.yml --include=*.mjs --include=*.rs .
```

18 件のヒットをすべて読んだ。**「値は何でもよい」「存在だけを見る」と述べた記述は 0 件。** 逆に、値を明示している箇所はすべて `=1` である（`docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:919`、`scripts/repro-pester-flake.ps1:129`）。`PERFORMANCE.md:251-253` は 3 つの env を人間向けに文書化しているが受理値を書いていない。**計画 `plan.md:114` の「現行で `=1` を渡している呼び出し側は挙動が変わらない」は、記録に照らして成立する。**

---

## C — `repro-pester-flake.ps1` が `Invoke-SnotraEnvironment` を使わなかった理由の記録

### C-1: 理由の記録は**無い**（探索は尽くした）

```
$ git log --oneline -- scripts/repro-pester-flake.ps1
8a03252 fix(egui): 入力欄の focus を TextEdit の構築前へ移し、起動直後の打鍵の喪失を止める (#938)
```

コミットは 1 つだけなので、記録がありうる場所は #938 の本文・レビュー・スクリプト自身の doc に限られる。

| 場所 | コマンド | 結果 |
|---|---|---|
| PR 本文 | `gh pr view 938` 全文 | env の退避・復元にも `Invoke-SnotraEnvironment` にも**言及なし** |
| issue コメント | `gh pr view 938 --comments` | **出力なし（0 件）** |
| インラインレビュー | `gh api repos/finelagusaz/Snotra/pulls/938/comments` | **出力なし（0 件）** |
| スクリプトの doc | `sed -n 1,40p scripts/repro-pester-flake.ps1`（`.NOTES` / `.DESCRIPTION` 全文） | 退避・復元への言及なし |
| 当該行の直上コメント | `repro-pester-flake.ps1:121` | 「子プロセスへ渡す env（呼び出し元の環境を汚さないよう、後で必ず戻す）。」— **目的のみ。手法の選択理由は無い** |

**→ 「手書きを選んだ理由」はどこにも残っていない。**

### C-2（**軽微**）: ただし構造上の理由は在る。計画の「写しを置いた」という読みは半分しか正しくない

```
$ grep -c "Import-Module" scripts/repro-pester-flake.ps1
0
```

**このスクリプトは `SnotraSmoke.psm1` を一度も import していない。** `Invoke-SnotraEnvironment` は最初から手の届く所に無かった。しかもそれは事故ではなく、スクリプト自身の `.DESCRIPTION`（`21-23` 行）が述べる設計方針である:

> **各反復は `scripts/run-pester.ps1` を子プロセスとして呼ぶ**——CI の `rust-check` が呼ぶのと同じ入口である。Pester の設定・タグ・実行ファイルの解決を写し取ると、写しの側だけが陳腐化して「CI とは違うものを N 回測った」になる。

`plan.md:107` は「`repro-pester-flake.ps1` はこれを再利用せず手書きの写しを置き、**写しの側だけが壊れていた**」と書き、続けて「`Invoke-SnotraEnvironment` を直接使えるなら使う」としている。**「直接使う」は、この harness に初めて `Import-Module SnotraSmoke.psm1` を持ち込むことを意味する**——doc が避けると宣言している方向の結合である。

計画の最小の直し（`Exists` を別に持ち `Remove-Item` する）はモジュール結合なしで実現でき、そちらなら doc と衝突しない。**「直接使えるなら使う」の分岐を採る場合は、`.DESCRIPTION` の方針との関係を PR 本文で明示するか、doc 側を書き換えること。**

### C-3: `run-pester.ps1:49` は事情が違う（こちらは純粋な取りこぼし）

```
$ grep -n "Import-Module" scripts/run-pester.ps1
17:Import-Module $smokeModulePath -Force
```

`run-pester.ps1` は**モジュールを import している**（`Resolve-SnotraCargoExecutable` を使うため）。しかも `git log -S'Invoke-SnotraEnvironment'` と `git log --diff-filter=A -- scripts/run-pester.ps1` は**どちらも `5079c37`（#853）**である——**正しいヘルパと手書きの復元が、同じ 1 コミットで同時に書かれた**。ここには C-2 のような構造的な言い訳は無い。計画が対象へ足した判断（`plan.md:100`）は妥当。

---

## D — `Reject` / `Stop` の 2 方針の経緯と、`Reject` が厳しくあるべき理由

### D-1: `Reject` は #853 で新設された。それ以前は `Stop` しか無かった

```
$ git log --oneline -S'Resolve-SnotraExistingProcess' -- scripts/lib/SnotraSmoke.psm1
5079c37 refactor: smoke PowerShell の配管を共有化 (#853)
```

`git show 5079c37 -- scripts/lib/SnotraSmoke.psm1` の行 259-276 が関数の全文（すべて `+`）で、**コメントは 1 行も無い**。現在も無い（`sed -n 388,401p scripts/lib/SnotraSmoke.psm1`）。

#853 以前の 2 本のスクリプトが持っていたのは `Stop` 相当だけである（A-1 に引用した `git show 5079c37^:scripts/smoke-egui.ps1` の 209-211 行）。**`Reject` は Pester の統合検査と同時に生まれた**——ローカル実行者の実インスタンスを殺さないための新しい方針である。

`gh pr view 853` 本文の該当箇所は列挙のみ:

> config seed の必須骨格、env の設定/復元、Cargo target の本体導出、**既存プロセス方針**、起動と stderr、trace parse、…を共有モジュールへ抽出

**2 方針を置いた理由の説明は無い。**

### D-2（**⚠️**）: 「厳しいままであるべき理由」は当時明示されていない。ただし throw 文言が実質的にそれを担っている

見つかった最も強い一次証拠は、`SnotraSmoke.psm1:395` の throw 文言そのものである。

```
throw "Snotra が既に起動しています（pid=…）。single-instance により検証が空振りするため、
       終了してから再実行してください。"
```

**「終了してから再実行してください」は人間への指示である。** つまりこの方針の宛先は CI ではなくローカル実行者であり、猶予を入れて「少し待ってから諦める」形にすると、実インスタンスの検出という本来の役目が鈍る——**計画 `plan.md:62` の判断は、この文言から再構成できる。**

固定している検知手段も同じコミット由来である（`Tests.ps1:234-239`・`Should -Invoke Stop-Process -Times 0`）。**計画 `plan.md:188` の「既存単体検査が守る」は正しい。**

**ただし「当時明示されていたか」への答えは「否」である。** PR/issue に議論の記録は無く、根拠は文言からの再構成である。計画の結論は変えなくてよいが、**「当時の議論に根拠がある」とは書かないこと**。

---

## E — 計画が触れていないが、同じ変更で壊れうる不変条件

手順: 計画が消す・変える行を名指し → その行が守っていた不変条件を履歴から特定 → 計画のどこで再確立されるかを探す。**再確立地点が無いものだけを挙げる。**

### E-1（**要対処**）— trace 写しの不変条件は今日成立していない。Phase 1 で初めて成立し、その結果 `research.md` の数値の比較可能性が切れる

**計画が変える行**: `Tests.ps1:500-503`（キャレットの `finally` の `Stop-Process`）。
**その直後の行が名乗る不変条件**（`Tests.ps1:495-499`・`8a03252`/#938 で追加）:

```
$ git show 8a03252 -- scripts/lib/SnotraSmoke.Tests.ps1
@@ -488,6 +501,16 @@
             if ($null -ne $proc -and -not $proc.HasExited) {
                 Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
             }
+            # **成否によらず trace を残す** …
+            # kill の後に写すのは、書き終えたものを写すためである。
+            if ($env:SNOTRA_PESTER_TRACE_DIR -and (Test-Path -LiteralPath $stderr)) {
+                Copy-Item -LiteralPath $stderr … -Destination (… 'caret.err')
```

**「kill の後に写すのは、書き終えたものを写すためである」は、待たない kill の直後では成立しない。** `Stop-Process -Force` は制御を即返すので、`Copy-Item` は本体がまだ終了処理中の `caret.err` を写しうる。

**（誠実のため明記する）機序は在るが、切り詰めが実際に起きたことは測っていない。** 既存 artifact のどの行が欠けたかを示す証拠は無く、ここで言えるのは「コメントが名乗る不変条件を機構が保証していない」ことまでである。**下の帰結 2 はそれでも成立する**——Phase 1 は写しと終了の**前後関係そのもの**を変えるので、切り詰めの有無に関わらず前後の artifact は同条件でなくなる。

**帰結（計画に一言も無い）**:

1. **Phase 1 は、このコメントが名乗る不変条件を初めて成立させる**——良い変化だが、計画はそれを利得として挙げていない。挙げれば `Tests.ps1:502` の変更が「対称性のためのみ」（`plan.md:46` / `197`）ではなく**実質的な効果を持つ**ことになり、`/symmetric-check` 表の [適用] 判定の根拠が強くなる
2. **より重いのはこちら**——`workspace/research.md` の定量的証拠は、この写しが生んだ artifact に載っている。発見 4 の「iter-020 の `caret.err` は最終的にも **20 行しかなく**、行数だけでは 2.3 秒を説明できない」、発見 6 の「連続する 2 行の間隔が **3.0〜3.6 秒**」、発見 3 の「26/27 反復」——**これらはすべて、切り詰められうる写しから読んだ数である。** Phase 1 の後に採る artifact は完全になるので、**Phase 1 をまたいだ artifact 同士の比較は同条件ではない**

**再確立地点**: 無い。**対処**: (a) `plan.md` Phase 1 の効果に「trace 写しの不変条件が初めて成立する」を足す、(b) 「範囲外」または U2 の記述へ「Phase 1 前後の artifact は行数・間隔の比較に使えない」を明記する。とくに `research.md` 発見 4 の「行数だけでは説明できない」は、切り詰めの可能性を排除してから再主張すべき論点である。

### E-2（**要対処**）— 3 箇所を 1 つのヘルパへ畳むと、`Policy Stop` のエラーチャネルが黙る

**現在の 3 箇所は同じ形ではない**（`grep -rn "Stop-Process" scripts/` の実出力）:

```
scripts/lib/SnotraSmoke.psm1:399:        Stop-Process -Id $process.Id -Force
scripts/lib/SnotraSmoke.Tests.ps1:381:  Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
scripts/lib/SnotraSmoke.Tests.ps1:502:  Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
```

**`psm1:399` にだけ `-ErrorAction` が無い。** 由来は `5079c37`（`git log -S'Stop-Process -Id $process.Id -Force' -- scripts/lib/SnotraSmoke.psm1` の唯一のヒット）で、理由のコメントは無い。`psm1:3` は `Set-StrictMode -Version Latest`。

計画の契約（`plan.md:56`）は `Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue` を**ヘルパの内側に固定**し、`plan.md:44` は `Policy Stop` 分岐もそこへ通す。**結果、アクセス拒否・既終了で `Policy Stop` が今まで上げていたエラーが黙る。** 計画は `WaitForExit` のアクセス拒否（`plan.md:63`）には触れているが、`Stop-Process` 自体のエラーチャネルには触れていない。

**再確立地点**: 無い。既存単体検査（`Tests.ps1:248` の `-ParameterFilter { $Id -eq 123 -and $Force }`）はこの差を見ていないので、検知もされない。

**対処**: ヘルパに `-ErrorAction` の扱いを引数（例 `[switch]$IgnoreStopErrors`）で出すか、`Policy Stop` の側は現行どおり throw させて待ちだけをヘルパに借りる形にする。**どちらでもよいが、「意図して黙らせた」と書くこと**——不記載のまま黙るのが最も悪い。

### E-3（**要対処**）— 型付き `[System.Diagnostics.Process]` 引数が、既存単体検査の偽プロセス fixture を壊す（**実測済み**）

**計画が変える行**: `Resolve-SnotraExistingProcess` の `Stop` 分岐（`psm1:398-400`）。
**その行が依存している不変条件**（`5079c37` 由来・`Tests.ps1:243-248`）:

```powershell
Mock -ModuleName SnotraSmoke Get-Process { @([pscustomobject]@{ Id = 123 }) }
Mock -ModuleName SnotraSmoke Stop-Process {}
Resolve-SnotraExistingProcess -Policy Stop
Should -Invoke -ModuleName SnotraSmoke Stop-Process -Times 1 -ParameterFilter { $Id -eq 123 -and $Force }
```

**この検査は `Id` しか持たない偽オブジェクトで成立する**——実プロセスを起動せずに方針の分岐を固定できることが、この fixture の設計上の値打ちである。

計画の契約（`plan.md:52`）は `param([System.Diagnostics.Process]$Process, [int]$TimeoutMs = 5000)`。ここへ偽オブジェクトを渡すとどうなるかを**実測した**:

```
$ pwsh -NoProfile -Command "
    function Stop-Test { param([System.Diagnostics.Process]$Process, [int]$TimeoutMs = 5000) 'bound' }
    try { Stop-Test -Process ([pscustomobject]@{ Id = 123 }) } catch { 'ERROR: ' + $_.Exception.Message }"

ERROR: Cannot process argument transformation on parameter 'Process'.
       Cannot create object of type "System.Diagnostics.Process". "Id" is a ReadOnly property.
```

**`Should -Invoke` の期待値の問題ではない。呼び出しがパラメータ束縛で例外になる。** `plan.md:64` は「`Should -Invoke Stop-Process` の検証点が動くため、**Mock の対象と回数の期待を同時に更新する**」としか書いておらず、fixture 自体が使えなくなることを見ていない。

**「引数の型を外せば済む」は誤りである（これも実測した）。** ヘルパ本体は `$Process.HasExited` と `$Process.WaitForExit($TimeoutMs)` を呼ぶ。`psm1:3` は `Set-StrictMode -Version Latest` なので、型を外しても失敗が束縛から**最初のメンバアクセスへ移るだけ**である:

```
$ pwsh -NoProfile -Command "
    Set-StrictMode -Version Latest
    $fake = [pscustomobject]@{ Id = 123 }
    try { $null = $fake.HasExited } catch { 'ERROR(HasExited): ' + $_.Exception.Message }"

ERROR(HasExited): The property 'HasExited' cannot be found on this object. Verify that the property exists.
```

**再確立地点**: 無い。実測で成立を確かめた選択肢は次の 3 つ。どれを採るかは計画に書くべき:

1. **fixture にメンバを足す** — 最も安い。同じ実測で成立を確認した:
   ```
   $fake2 | Add-Member -MemberType NoteProperty -Name HasExited -Value $false
   $fake2 | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($ms) $true }
   → fake2: HasExited=False WaitForExit=True
   ```
   引数の型は外す必要がある（`[System.Diagnostics.Process]` のままでは束縛で落ちる）。**期限切れ経路の単体検査（`plan.md:76`）も、この形なら `WaitForExit` に `$false` を返させるだけで作れる**
2. 単体検査を実プロセス（`Start-Process cmd.exe /c exit 0`）へ書き換える — `Tests.ps1:219-220` に前例がある。ただし `HasExited=$false` のまま期限切れさせる経路は実プロセスでは作りにくい
3. `Policy Stop` はヘルパを通さず、待ちだけを別途書く

**併せて**: `plan.md:76` の新ヘルパ単体検査（`$null` / `HasExited` / 期限切れ / 例外の 4 経路）も、`[System.Diagnostics.Process]` を要求する限り**すべて実プロセスを要する**。`HasExited=$true` の偽物も、期限切れを起こす偽物も、この型では作れない。作業項目の見積もりが変わる。

### E-4（**軽微**）— `smoke-egui.ps1:139` の `Start-Sleep 300` が二重の待ちになる

```
scripts/smoke-egui.ps1:137: # 既存インスタンスは single-instance 転送で smoke を汚すため停止（smoke-startup.ps1 と同じ前提）
scripts/smoke-egui.ps1:138: Resolve-SnotraExistingProcess -Policy Stop
scripts/smoke-egui.ps1:139: Start-Sleep -Milliseconds 300
```

コメントも sleep も **#853 以前からの逐語の持ち越し**である（`git show 5079c37^:scripts/smoke-egui.ps1` の 209-211 行と同一）。**この 300ms が、`Policy Stop` に対する事実上の待ちだった。** Phase 1 が関数の内側に本物の待ちを入れると、これは説明のつかない固定遅延として残る。

対して `smoke-startup.ps1:55` の `Policy Stop` には sleep が**無い**（`sed -n 48,70p`）——#853 以前の同じ位置にも無かった。**同じ関数の 2 つの呼び出し点が、待ちの持ち方で最初から非対称である。**

**再確立地点**: 無い（壊れはしない）。**対処**: Phase 1 で `smoke-egui.ps1:139` を消すか、消さない理由を 1 行書く。どちらでもよいが、放置すると「どちらが効いているのか」が次の読者に分からなくなる。

### E-5（**⚠️**）— `Policy Stop` の待ちは、自分が起動していないプロセスを N × `TimeoutMs` 塞ぐ

`Resolve-SnotraExistingProcess` は `Get-Process -Name 'snotra'`（`psm1:396`）で**プロセス名で掴む**。`5079c37` 以来この経路は fire-and-forget であり、待ちは呼び出し側の固定 sleep が担っていた（E-4）。

計画は待ちを関数の内側へ入れる。**掴んだプロセスが開発者自身の実インスタンスだった場合、最大 `TimeoutMs`（既定 5000ms）× 件数まで塞ぐ。** `plan.md:63` はアクセス拒否での例外を警告へ倒す手当てを持つが、**「待ち切れずに時間を使う」経路の総量には触れていない**。

⚠️ とするのは、実害が出る条件（ローカルで複数の snotra が居る × 終了しない）を実測していないためである。**上限（全体の予算・件数の上限）を契約へ書くだけで消える。**

### E で探して**見つからなかった**もの（不在の観測）

| 探したもの | コマンド | 結果 |
|---|---|---|
| `Reject` を緩めた／緩めようとした過去 | `git log --oneline -S'Resolve-SnotraExistingProcess' -- scripts/lib/SnotraSmoke.psm1` | ヒットは `5079c37` の 1 件のみ＝**一度も変更されていない**。計画の「緩めない」は既存の実績と整合 |
| 空文字 env に依存した過去 | `git log --oneline -S'var_os' -- snotra-egui-runtime/` の全 5 コミットの hunk を通読 | 空文字・未設定の区別を論じた記述 **0 件** |
| `$dead.WaitForExit()`（`Tests.ps1:220`）が新ヘルパと衝突するか | `git log --oneline -S'WaitForExit()' -- scripts/lib/SnotraSmoke.Tests.ps1` → `8da3b6a`（#889）／`sed -n 212,232p` で本文確認 | **衝突しない**。これは `cmd.exe /c exit 0` を確実に終了させる**検査の前提づくり**であり、kill+wait ではない。`/dry-check` が実装後に指す可能性はあるが、ヘルパの適用対象ではない |

---

## 計画への総合的な立場

- **A / B / C / D いずれについても、「計画が変えようとしている形が意図して置かれた」証拠は見つからなかった。** 3 系統とも、既存の正しい形（`smoke-egui.ps1` の待ち・`Invoke-SnotraEnvironment`・`trace.rs` の `env_flag`）が**先に存在していたのに配られなかった**という同じ形をしている。計画の方向に反対する所見は無い
- **反対に、A-1 で見つけた「#904 が同じ It ブロックを触りながら配らなかった」という痕跡は、計画の Phase 1 を強く支持する**（意図的な非対称ではないことの直接証拠）
- **要対処 4 件（E-1 / E-2 / E-3 / B-2）は、いずれも計画の作業項目のままでは通らないか、通っても記録が嘘になる。** とくに **E-3 は実測済みの破壊**であり、着手前に契約（`plan.md:52`）を決め直す必要がある
- **⚠️ 3 件（E-5 / A-3 / D-2）は確信が持てないので判断は委ねる。** A-3（`WaitForExit` の根拠が PR 本文に無い）と D-2（`Reject` の根拠が throw 文言からの再構成）は、いずれも「意図の記録が想定より薄い」型であり、計画の結論そのものは変えない
