# 調査: #872 残余（「型 A（遅延）だけが残った」の検算）

## issue の要約

#872 は「実機配管の Pester 統合検査 `フォルダ復帰後の次打鍵を復元クエリの末尾へ追加する` を CI ゲート（`rust-check`）に置き続けるか」を決める検討 issue である。候補は (a) 現状維持 / (b) 注入経路を前面非依存にする / (c) Smoke ワークフローへ移す / (d) 再試行でくるむ、および後から挙がった第 5 の候補「観測予算を実測へ合わせて広げる」。

直近コメント（2026-08-05・PR #938）は次のように整理している。

- 根本原因はアプリ側の実バグ（`view.rs` が `TextEdit` 構築**後**に `request_focus()` を撃ち、そのフレームに載った文字が捨てられていた）だった
- 同日・同 runner で **25/30 失敗 → 8/30 失敗**
- **残る 8 件は文字が 1 つも消えていない**＝「型 B（喪失）が消え、型 A（遅延）だけが残った」
- ゆえに「ゲートの是非を判断する対象がここで初めて 1 つになる」

**本調査の結論は、この最後の一文が成り立たないことである。** 残余は 1 つではなく、8 件は 2 つの異なる故障へ分かれる。

---

## 一次証拠

いずれも run [30967971392](https://github.com/finelagusaz/Snotra/actions/runs/30967971392)（修正後・30 反復）と [30967425958](https://github.com/finelagusaz/Snotra/actions/runs/30967425958)（修正前・30 反復）の artifact を実際に取得して読んだ（retention 14 日・本日時点で取得可能）。

### 発見 1 — 残余 8 件は 1 つの故障ではなく 2 つである

失敗した 8 反復の `::group::Message` を全件読んだ結果:

| 故障 | 件数 | 例外・assertion | 位置 |
|---|---|---|---|
| **単一インスタンス衝突** | **3**（iter-008 / 021 / 029） | `RuntimeException: Snotra が既に起動しています（pid=…）` | `Resolve-SnotraExistingProcess`（`SnotraSmoke.psm1:395`）← `Tests.ps1:441` |
| 待ちの予算切れ（＝型 A） | 5（iter-014 / 017 / 020 / 024 / 027） | `Expected a value, but got $null or empty.` | `Tests.ps1:460`（3 件）/ `467`（2 件） |

**3 件は遅延でも喪失でもない。検査は 1 打鍵も注入せずに終わっている**——`Tests.ps1:441` は `try` の最初の行であり、本体の起動より前である（iter-008 の失敗ダンプが `(stderr file not found: …\caret.err)` を出しているのが、本体が起動していないことの直接の証拠）。

したがって型 A の率は最大で **5/30 = 16.7%** であり、コメントが型 A に帰した 26.7% ではない。

### 発見 2 — 単一インスタンス衝突は「同一 Pester 実行の直前の It が残した自プロセス」である（＝`rust-check` に届く）

反復をまたいだ残骸ではないことが、テスト自身の構造から確定できる。

- `Tests.ps1:345` の It（`生成した seed を本体が parse して…`）も、**自分の `try` の先頭で `Resolve-SnotraExistingProcess -Policy Reject` を呼ぶ**（`Tests.ps1:355`）。3 件とも**この It は pass している**（ログで確認）＝その時点で生存プロセスは無かった
- その It は `finally`（`Tests.ps1:381`）で `Stop-Process -Id $proc.Id -Force` を呼ぶだけで、**終了を待たない**
- 直後にキャレット検査が `Tests.ps1:441` で同じ `Reject` を呼び、生存プロセスを見つけて throw する

**2 つの `Reject` の間に起動されるプロセスは直前の It のものだけである**——ゆえに掴まれた pid はそれである。**この経路は反復再現ハーネスに固有の要素を 1 つも含まない**（`-FailureGraceMs` も `SNOTRA_PESTER_TRACE_DIR` も直前の It には関与しない）。すなわち **`rust-check` の 1 回きりの実行でも同じ確率で起こる**。

修正前の run にも 1 件ある（25 件中 24 件が待ちの予算切れ、1 件がこれ）。**新種ではなく、喪失の陰に隠れて 7 件の記録の中で一度も分類されなかったものである。** #872 本文の失敗表にこの assertion は 1 行も無い。

### 発見 3 — 比較に使われた 2 つの run は、いずれも打鍵の到達計器が有効な状態で走っていた

`scripts/repro-pester-flake.ps1` は「**率を測る回と機序を測る回は別の回にすること**」「立てた回の失敗率は、立てない回と比較してはならない」と自ら明記し、計器つきの回では host と `summary.md` に ⚠️ を出す設計である。

- 両 run とも `workflow_dispatch` ではなく **push で起動**しており、job ログのコマンド行は `-InputTrace` を含まない
- `summary.md` にも ⚠️ 行は無い
- **しかし artifact の `caret.err` には `SNOTRA_EGUI_INPUT` 行が実在する** — 修正後 26/27 反復、修正前 28/29 反復（どちらも `iter-001` だけが計器なし）

観測されたのはアプリ側が出す種類（`rx_key` / `push_key` / `rx_text` / `push_text` / `take`）だけで、PowerShell 側が同じ env を見て出す `inject` 行は 0 件だった。**同じ env について、Rust 側は「有効」・PowerShell 側は「無効」と読んでいる。**

#### 機序（特定済み・実測）

`repro-pester-flake.ps1` は反復ごとに env を退避して `finally` で戻す（`143-145`）。**退避値が `$null`（＝元から未設定）のとき、この復元は変数を消さずに空文字で作る。** PowerShell 7 で実測した:

```
$saved = [Environment]::GetEnvironmentVariable('SNOTRA_PROBE_UNSET','Process')   # → $null
[Environment]::SetEnvironmentVariable('SNOTRA_PROBE_X', $saved, 'Process')
→ null=False  empty=True  PS truthy=False  Env ドライブに存在=True
```

`$null` は `[string]` 引数へ束縛される時点で空文字になり、変数は**残る**。そこから先で読み手が割れる:

- **Rust**: `input.rs:34` は `std::env::var_os("SNOTRA_EGUI_INPUT_TRACE").is_some()` — 空文字でも `Some("")` ゆえ **true＝計器 ON**
- **PowerShell**: `SnotraSmoke.psm1:664` の `if ($env:SNOTRA_EGUI_INPUT_TRACE)` — 空文字は偽ゆえ **inject 行を出さない**

これで観測が過不足なく説明できる:

| 観測 | 説明 |
|---|---|
| `iter-001` だけ計器なし | 1 反復目の実行時点では変数がまだ存在しない |
| `iter-002` 以降すべて計器あり | 1 反復目の `finally` が空文字で変数を作り、以降の子 pwsh がそれを継承する |
| アプリ側の行だけ在り `inject` 行が無い | 上記の読み手の非対称そのもの |
| host にも `summary.md` にも ⚠️ が無い | 警告は `$InputTrace`（意図）を見ており、env の実態を見ていない |

**修正前 run で `iter-001` だけが計器なしなのも同じ形である**（28/29）。

### 発見 5 — 同じ形が 4 + 7 箇所にある（同一パターンの全コードパス検索）

**復元側**（`$null` を渡して空文字を作る）: `repro-pester-flake.ps1:143` / `144` / `145`、`run-pester.ps1:49` の 4 箇所。ただし実害があるのは 145 だけである——残る 3 つ（`SNOTRA_PESTER_TRACE_DIR` / `SNOTRA_PESTER_FAILURE_GRACE_MS` / `SNOTRA_PESTER_EXE`）は**読み手も PowerShell** であり、空文字は一貫して偽に落ちる。

**読み手側**（空文字を「有効」と読む）: `snotra-egui-runtime` の 7 箇所。

| 位置 | env |
|---|---|
| `input.rs:34` | `SNOTRA_EGUI_INPUT_TRACE` |
| `renderer.rs:76` | `SNOTRA_EGUI_PAINT_TRACE` |
| `repaint.rs:197` | `SNOTRA_EGUI_WAKE_TRACE` |
| `runtime.rs:279` | `SNOTRA_EGUI_WAKE_TRACE` |
| `runtime.rs:456` | `SNOTRA_EGUI_REPAINT_TRACE` |
| `windows_ime.rs:100` / `209` | `SNOTRA_EGUI_IME_TRACE` |

対して `src-tauri/src/trace.rs:20` の `env_flag` は `1|true|yes|on` だけを真とする厳しい形で、`SNOTRA_TRACE` はこちらを通る。**同じリポジトリの中に 2 つの意味論があり、緩い側だけが空文字で点灯する。**

### 発見 6 — 予算超過の警告は鳴っているが、単体検査のノイズに埋もれている（U3）

`Wait-SnotraTraceCondition` は事象が期限後に届いたときも成立させ、`予算 Xms を過ぎた評価で成立しました` を警告する（`SnotraSmoke.psm1:574`）。修正後 run のログを全件 grep した結果:

- **本物の遅着（`予算 5000ms`）は 4 反復で鳴っていた** — iter-001 / 012 / 019（**いずれも pass**）と iter-014（fail）。実測の一例は `5251ms・7 周`
- **同時に、全 30 反復に `予算 0ms を過ぎた評価で成立しました（2ms・1 周）` が 1 件ずつ出ている** — これは `Tests.ps1:186` の単体検査（`-TimeoutMs 0` で期限跨ぎの取りこぼしを検査する）が意図的に鳴らしているものである

**帰結は 2 つある。**

1. 第 5 の候補（予算を広げる）が要求する「広げても退行が読める」機構は**既に在って鳴っている**。新設は要らない
2. **ただし今の形では読めない。** 同一文言の警告が正常時に毎回 1 件出るため、grep しても本物と区別がつかない。#872 本文が (a) の欠点として挙げた「赤が常態化すると本物を見落とす」と同じ摩耗が、警告の層で既に起きている

なお **3/30 は「予算を超えて届いたが pass した」反復である**。5 件の fail と合わせると、**この run の 8/30 = 26.7% で 5,000ms の予算が足りていない**（fail と pass の別を問わず）。ただし発見 3 のとおり計器つきの run ゆえ、絶対率としては使えない。

この計器の増悪は測定済みである: `repro-pester-flake.ps1` の doc が失敗率 36.7%（計器なし）→ 63.3% → **100%** を記録し、runner では stderr 1 行が 17〜56ms かかると書いている。今回の trace はそれより悪く、**連続する 2 行の間隔が 3.0〜3.6 秒**に達している箇所がある（iter-020: `ts_ms` 244844 → 248470）。

**帰結:**

- **#938 の主張（25/30 → 8/30 の改善）は生き残る**——両 run が同条件（同日・同 runner・同じく計器つき）だからである
- **8/30 も 5/30 も絶対率としては使えない。** ゆえに「型 A をどう扱うか」を今日の数字だけで決めることはできない

### 発見 4 — 待ち側の周回数が、予算を広げる案の意味を変える

`Wait-SnotraTraceCondition` が不成立時に残す診断を 5 件すべて読んだ（`PollMs 100` / `TimeoutMs 5000`）。

| 反復 | 経過 | 周回 | 1 周あたり | 最後に見たもの |
|---|---|---|---|---|
| iter-014 | 5090ms | **2 周** | 約 2545ms | trace 行 7 / 事象 6 / 捨てた行 1 |
| iter-017 | 6052ms | 7 周 | 約 865ms | trace 行 7 / 事象 7 |
| iter-020 | 6980ms | **3 周** | 約 2327ms | trace 行 2 / 事象 2 |
| iter-024 | 5390ms | 11 周 | 約 490ms | trace 行 2 / 事象 2 |
| iter-027 | 6269ms | 8 周 | 約 784ms | trace 行 2 / 事象 2 |

`PollMs 100` の意図に対し実際は **1 周 0.5〜2.5 秒**である。5,000ms の予算の内側で trace を **2〜11 回しか見ていない**。

`Read-SnotraTraceSnapshot`（`SnotraSmoke.psm1:459`）は毎周 `Get-Content` でファイル全体を読み、全行を `ConvertFrom-SnotraTraceLine` へ通す。ただし iter-020 の `caret.err` は最終的にも 20 行しかなく、**行数だけでは 2.3 秒を説明できない**。同じ反復でアプリ側の stderr も数秒間隔まで伸びている（発見 3）ので、**観測側の遅さとアプリ側の遅さは、このデータでは分離できない**。

これが「予算を広げる」案に対して効く: 広げて買えるのは**高価な読みの周回数**であり、しかもその高価さの一部は計器が作ったものである。**広げる幅の根拠は、計器なしの run を 1 回採るまで書けない。**

---

## 関連ファイル・シンボル（grep で実在を確認済み）

| パス | シンボル・位置 | 役割 |
|---|---|---|
| `scripts/lib/SnotraSmoke.Tests.ps1` | `345`（seed の It）/ `381`（その `finally`） | 待たない `Stop-Process -Force` の出どころ |
| `scripts/lib/SnotraSmoke.Tests.ps1` | `386`（キャレットの It）/ `441` / `460` / `467` / `472` | 衝突の被害側・3 つの待ち |
| `scripts/lib/SnotraSmoke.Tests.ps1` | `501`（キャレットの `finally`） | 同じく待たない `Stop-Process -Force` |
| `scripts/lib/SnotraSmoke.psm1` | `Resolve-SnotraExistingProcess`（`383`）/ throw は `395` | 単一インスタンス衝突の判定点 |
| `scripts/lib/SnotraSmoke.psm1` | `Read-SnotraTraceSnapshot`（`459`） | 毎周の全文読み |
| `scripts/lib/SnotraSmoke.psm1` | `Wait-SnotraTraceCondition`（`537`） | 待ちループの唯一の実装。予算超過での成立を警告する経路を既に持つ（`574`） |
| `scripts/repro-pester-flake.ps1` | `$InputTrace`（`66`）/ 集計（`175`〜） | 測定器。計器の状態を `summary.md` へ出すのは `$InputTrace` を見た分岐だけ |
| `.github/workflows/pester-flake-repro.yml` | `input_trace`（`32`）/ 起動行（`73`） | push 起動では入力が空になる |
| `snotra-egui-runtime/src/input.rs` | `input_trace_enabled`（`29`）/ `input_trace`（`49`） | アプリ側の計器の門 |

## 再利用できる既存パターン

- **不成立の理由を区別して残す**——`Wait-SnotraTraceCondition` が既に「予算切れ / 本体終了 / 読み取り失敗」を書き分けている（#872 で導入）。新しい待ちを足すならこの関数を通す（写しを置かない規律がコメントに明記されている）
- **予算超過での成立を警告する経路が既に在る**（`SnotraSmoke.psm1:574`）。第 5 の候補が要求する「広げても退行が読める」機構は、新設ではなく**この既存経路が `rust-check` の出力に現れるかの確認**で足りる可能性がある（→ 未確定 U3）
- **プロセス終了を待つ形の手本**——`Tests.ps1:220` の単体検査が `$dead.WaitForExit()` を使っている。共有ヘルパは無い

## 技術的制約

- **CI の実測は PR が在って初めて行える**（`ci.yml` は `pull_request` 起動）。ゆえに CI 上での検証項目は計画の作業項目ではなく PR 本文のチェックリストへ送る（`.claude/rules/safety-nets.md`）
- **反復再現ワークフローは `workflow_dispatch` と `chore/pester-flake-**` / `exp/pester-flake-**` への push でしか起動しない**。既定ブランチ以外で dispatch はできない
- `repro-pester-flake.ps1` の `.NOTES` は「**#872 / #936 が閉じたら一式を撤去する**」を撤去条件として持つ。本 issue はまだ閉じないので、道具は残る
- 反復再現 job の 1 回は 30 反復で約 12 分（実測: 02:09:08 → 02:21:03）+ ビルド。外向きの CI 資源を使う

## 未解決の疑問

- **U1**（計器の env がどこから立ったか）— **解決**（発見 3）。`$null` 復元が空文字を作り、Rust 側の `var_os().is_some()` がそれを「有効」と読む
- **U3**（予算超過の警告が現れるか）— **解決**（発見 6）。鳴っているが単体検査の同一文言に埋もれている
- **U2**: 計器なしの条件での、#938 修正後の型 A（待ちの予算切れ）の率と所要の分布 — **未解決**。反復再現 job を計器なしで 1 回回すまで測れない

## この調査が言っていないこと

- **型 A（遅延）の機序は特定していない。** 分かったのは「待ちが 5,000ms を使い切った」ことと「その 5,000ms の中で 2〜11 回しか観測していない」ことまでである
- **単一インスタンス衝突の 3 件が `rust-check` で過去に何回起きたかは数えていない。** 構造上届くことを示したのみで、#872 の記録済み 7 件のどれがこれだったかは、当時のログを引かない限り決まらない
- **1 run の率は率ではない。** 本調査が使った 2 run はどちらも同一日・同一 runner の 30 反復である
