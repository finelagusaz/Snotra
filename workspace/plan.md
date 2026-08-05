# 実装計画: #872 残余の再分類と、単一インスタンス衝突の除去

## 目的

#872 の残余を「型 A（遅延）だけ」から実測どおりの 2 つへ戻し、そのうち**構造的に直せる方**（単一インスタンス衝突）を消す。あわせて、次の測定が同じ穴に落ちないよう反復再現ハーネスに自己記述を持たせる。**型 A（待ちの予算切れ）の予算判断は本計画の範囲外**——根拠となる計器なしの実測がまだ無いため（→「範囲外」）。

調査の全文と一次証拠は `workspace/research.md`。

## 今日の時点で確定できる判断（#872 の問いへの回答の一部）

**この検査は `rust-check` に置き続ける。** 候補は次の理由で出揃っている。

- **(b) 注入経路を前面非依存にする** — 不要。#890 が現れ方 1 の機序を本体自身のコンソール窓と特定し、窓を隠して解消した
- **(c) ゲートから外す** — 反対。#938 が示したとおり、この検査だけが起動直後の打鍵喪失（`show_on_startup = true` の実害）を捕まえていた
- **(d) 再試行でくるむ** — 反対。同上の実バグを緑で塗ることになっていた
- **(a) 現状維持** — 「現れ方ごとに塞ぐ」ではなく「原因側を直す」へ既に移行済み（#887 / #889 / #890 / #938）。本計画はその続きである

## 受け入れ条件

1. 同一 Pester 実行の中で、先行する It が起動した本体の終了を待たずに次の It が `Resolve-SnotraExistingProcess` を呼ぶ経路が無くなっている
2. 待たずに終了させてから次を起動する経路（`Resolve-SnotraExistingProcess -Policy Stop` の呼び出し側）にも同じ手当てが入っている
3. 反復再現ハーネスの `summary.md` が、**その run で打鍵の到達計器が実際に有効だったか**を証拠に基づいて記録する（`-InputTrace` の指定ではなく、残った trace の中身から測る）
4. `-InputTrace` を渡さない実行では、計器の env が確実に落ちている（**空文字で残らない**）
5. `snotra-egui-runtime` の trace ハッチが、空文字の env で点灯しない
6. 上記の変更が既存の Pester 55 件（単体 53 + 統合 2）と workspace 全テストを壊していない

---

## Phase 1 — 単一インスタンス衝突を消す

### 既に在る手本（新設ではない）

`scripts/smoke-egui.ps1:458-466` が **#755/#801 是正 B** として同じ機序を既に解いており、コメントが機序を明記している。

> `Stop-Process -Force` は終了を待たない。`tauri_plugin_single_instance` が登録されているため、先発がまだ生きたまま後発を起動すると、後発は先発へ通知して即終了し——trace が 1 行も書かれないまま `hotkey:registered` の待ちが予算を使い切ってから throw する（間欠的な赤）。

**Pester 側にはこの手当てが入っていない。** 本 Phase はその横展開である。

### 変更ファイルと対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `scripts/lib/SnotraSmoke.psm1` | **新規** `Stop-SnotraProcessAndWait` | kill して終了を待つ唯一の実装。`Export-ModuleMember` へ追加 |
| `scripts/lib/SnotraSmoke.psm1` | `Resolve-SnotraExistingProcess`（`383`）の `Stop` 分岐（`398-400`） | 停止した各プロセスの終了を待ってから返す |
| `scripts/lib/SnotraSmoke.Tests.ps1` | seed の It の `finally`（`381`） | `Stop-SnotraProcessAndWait` へ置換 |
| `scripts/lib/SnotraSmoke.Tests.ps1` | キャレットの It の `finally`（`502`） | 同上（**対称性のため**。この It の後に本体を起動する経路は無い——`/symmetric-check` の結果を参照） |
| `scripts/lib/SnotraSmoke.Tests.ps1` | `Describe 'Resolve-SnotraExistingProcess'`（`233`） | 待ちが入ったことの単体検査を追加 |

### `Stop-SnotraProcessAndWait` の契約

```
param([System.Diagnostics.Process]$Process, [int]$TimeoutMs = 5000)
```

- `$null` または `HasExited` なら何もせず返す
- `Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue` の後、`$Process.WaitForExit($TimeoutMs)` で待つ
- **期限内に終了しなければ throw せず `Write-Warning` で残す**。呼び出し点が `finally` であり、**`finally` からの throw は元の例外を覆い隠す**（元の失敗こそが読みたいもの）。次の It の `Resolve-SnotraExistingProcess -Policy Reject` が、生き残りを大きな音で落とす役を引き続き担う
- `Resolve-SnotraExistingProcess -Policy Stop` の側は `finally` ではないので、**同じヘルパを使いつつ警告のまま**とする（この関数は「掃除して先へ進む」意味論であり、掃除しきれなかったことは後続の起動が single-instance で沈黙する形で現れる。そこを throw にするかは Phase 1 の射程外＝現状の意味論を変えない）

### 不変条件と異常系

- **`Resolve-SnotraExistingProcess -Policy Reject` の厳しさを緩めない。** 猶予を入れて「少し待ってから諦める」形にすると、実ユーザーのインスタンスを検出する本来の役目が鈍る。直すのは**生産側**（待たずに殺して次へ進む方）である
- `Get-Process` で得たプロセスには `WaitForExit` が使える（`Process` オブジェクト）。`Resolve-SnotraExistingProcess` は自分が起動したのではない他人のプロセスも掴むので、**アクセス拒否で `WaitForExit` が例外を出す経路**を `try/catch` で警告へ倒す
- 既存の単体検査は `Stop-Process` を `Mock` している（`Tests.ps1:236` / `244`）。ヘルパ経由に変えると `Should -Invoke Stop-Process` の検証点が動くため、**Mock の対象と回数の期待を同時に更新する**

### 検証

- `npm run test:powershell`（`docs/build-commands.md` の必須項目）
- フォールトインジェクション: `WaitForExit` が false を返す状況を Mock で作り、警告が出て throw しないことを検査する（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）

### 作業項目

- [ ] `Stop-SnotraProcessAndWait` を `SnotraSmoke.psm1` へ追加し `Export-ModuleMember` に載せる
- [ ] `Resolve-SnotraExistingProcess` の `Stop` 分岐を新ヘルパ経由へ変える
- [ ] `Tests.ps1:381` / `Tests.ps1:502` の `finally` を新ヘルパ経由へ変える
- [ ] 新ヘルパの単体検査（正常終了 / 期限切れで警告 / `$null` と `HasExited` の素通り / `WaitForExit` の例外）を追加する
- [ ] `Describe 'Resolve-SnotraExistingProcess'` の既存 2 件を、Mock 対象の変化に合わせて更新する
- [ ] `npm run test:powershell` が green

---

## Phase 2 — 計器が黙って点灯する経路を断つ

### 機序（特定済み・`workspace/research.md` 発見 3）

`repro-pester-flake.ps1` は反復ごとに env を退避して `finally` で戻す。**退避値が `$null`（元から未設定）のとき、`[Environment]::SetEnvironmentVariable($name, $null, 'Process')` は変数を消さず空文字で作る**（PowerShell 7 で実測: `null=False empty=True Env ドライブに存在=True`）。

その空文字を、2 つの読み手が逆に読む。

- `snotra-egui-runtime/src/input.rs:34` — `var_os(...).is_some()` は `Some("")` ゆえ **true＝計器 ON**
- `scripts/lib/SnotraSmoke.psm1:664` — `if ($env:...)` は空文字が偽ゆえ **OFF**

ゆえに 1 反復目の `finally` を境に、以降すべての反復でアプリだけが計器つきで走る。**両側を直す**（どちらか一方でも塞がるが、片方だけでは同じ形が別の env で再発する）。

### 変更ファイルと対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `scripts/repro-pester-flake.ps1` | env の退避（`122-124`）と復元（`143-145`） | **`Invoke-SnotraEnvironment` と同じ形へ寄せる**（下記） |
| `scripts/run-pester.ps1` | `SNOTRA_PESTER_EXE` の退避・復元（`49` 近傍） | 同上（実害は無いが同型・研究 発見 5） |
| `snotra-egui-runtime/src/input.rs` | **新規** `env_flag`（private） | `1｜true｜yes｜on` だけを真とする。`src-tauri/src/trace.rs:20` と同じ意味論 |
| `snotra-egui-runtime/src/{input,renderer,repaint,runtime,windows_ime}.rs` | `var_os(...).is_some()` の 7 箇所 | 新 `env_flag` へ寄せる |
| `scripts/repro-pester-flake.ps1` | 集計（`170`〜）・`$summary`（`175`〜）・`.NOTES` | 証拠に基づく計器の有無を `summary.md` へ出す |

### 設計判断

- **正しい手本は既にリポジトリの中に在る**（`/symmetric-check` の所見）。`Invoke-SnotraEnvironment`（`SnotraSmoke.psm1:267-286`）は**存在の有無（`Exists`）を値とは別に記録し、元が未設定なら `Remove-Item` する**。`repro-pester-flake.ps1` はこれを再利用せず手書きの写しを置き、**写しの側だけが壊れていた**。実装は「値だけを退避する」形をやめ、`Exists` を持つ形へ寄せる（`Invoke-SnotraEnvironment` を直接使えるなら使う——ただし同関数は `ScriptBlock` を包む形なので、反復ループの構造に合うかは実装時に判断する）
- **`env_flag` を共有せず `snotra-egui-runtime` に置く**——`snotra-egui-runtime` は `snotra-core` に依存しておらず（`Cargo.toml` 実測）、8 行の述語のために依存辺を増やさない。**双方の doc に互いを名指しで書く**ことで写しであることを明示する（`docs/comment-guidelines.md` の定型に従う）。※ 依存辺を増やして `snotra-core` へ寄せる案もある。**レビューで選び直してよい点である**
- **意図ではなく対象を測って報告する**——現在の ⚠️ は `$InputTrace`（意図）を見ており、実態と食い違ったときに沈黙した。反復ごとに `caret.err` の `SNOTRA_EGUI_INPUT` 行の有無を数え、`summary.md` へ「計器つきの反復: N / M（証拠）」を出す。**N > 0 なら `-InputTrace` の有無に関わらず ⚠️ を出す**
- `caret.err` が無い反復（本体が起動しなかった＝Phase 1 が直す衝突など）は「証拠なし」であり、「計器なし」と混同しない

### 不変条件と異常系

- **7 箇所の意味論を変えると、これまで空文字や任意値で点いていた計器が消える。** いずれも診断用の trace ハッチであり、`SNOTRA_TRACE`（`env_flag` 経由）とは別系統である。**現行で `=1` を渡している呼び出し側は挙動が変わらない**——変わるのは空文字・`0`・`false` を渡していた経路だけである。`scripts/` 内の設定箇所を grep して 1 件ずつ確認する
- **このスクリプトは測定器であって検出器ではない**（冒頭 doc）。⚠️ を増やしても exit code は 0 のまま

### 検証

- `cargo clippy --workspace --all-targets -- -D warnings` / workspace 全テスト（`docs/build-commands.md` カテゴリ A）
- `env_flag` の単体検査（`1`/`true`/`yes`/`on`/`ON ` が真、**空文字**・`0`・`false`・未設定が偽）
- **フォールトインジェクション**: 反復 1 → 2 の境界を再現する。`-InputTrace` なしで 2 反復回し、(a) 1 反復目の後に `Env:SNOTRA_EGUI_INPUT_TRACE` が存在しないこと、(b) 2 反復目の `caret.err` に `SNOTRA_EGUI_INPUT` 行が無いこと、(c) `-InputTrace` ありでは両方が逆になること——**両方向**を実測する

### 作業項目

- [ ] env の退避・復元 4 箇所（`repro-pester-flake.ps1` の 3 つ + `run-pester.ps1` の 1 つ）を、`Invoke-SnotraEnvironment` と同じ「`Exists` を別に持ち、未設定なら `Remove-Item`」の形へ変える
- [ ] `snotra-egui-runtime` に private `env_flag` を追加し、単体検査（空文字を含む）を置く
- [ ] `var_os(...).is_some()` の 7 箇所を `env_flag` へ寄せる
- [ ] `scripts/` 内で上記 6 種の env を設定している箇所を grep し、挙動が変わらないことを 1 件ずつ確認する
- [ ] 反復ごとに `caret.err` から計器の有無を測り、`summary.md` へ「計器つきの反復: N / M（証拠）」と食い違いの ⚠️ を出す
- [ ] `.NOTES` / `.DESCRIPTION` を実態へ合わせる
- [ ] ローカル 2 反復 × 2 条件で両方向を実測し、結果を本ファイル末尾の「実測ログ」へ書く
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` と workspace 全テストが green

---

## Phase 3 — 記録の訂正

### 変更ファイル

| ファイル | 変更 |
|---|---|
| `docs/build-commands.md`（`210`） | 反復再現 workflow の bullet に、計器の自己記述に触れる 1 文を足す（面積を増やさないよう既存文を書き換える） |

`SPEC.md` の更新は**不要**——本計画はテストハーネスと測定器だけを触り、製品の挙動・状態遷移を変えない。

### 作業項目

- [ ] `docs/build-commands.md` の該当 bullet を更新する
- [ ] `npm run governance:check` が green

---

## 範囲外（明示）

- **型 A（待ちの予算切れ）の `TimeoutMs` / `PollMs` の変更**——根拠となる計器なしの実測が無い（U2）。#872 の第 5 の候補はこの計画では判断しない
- **遅着警告のノイズ分離**（研究 発見 6）——`予算 Xms を過ぎた評価で成立しました` は本物の遅着で 4 回鳴っていたが、`Tests.ps1:186` の単体検査が同一文言を毎回 1 件出すため区別できない。**これは第 5 の候補（予算を広げる）の前提条件**であり、その判断と同じ枝で直すのが筋なので今回は入れない。**#872 のコメントへ記録する**
- **`Read-SnotraTraceSnapshot` の増分読みへの作り替え**——1 周 0.5〜2.5 秒の内訳が観測側とアプリ側に分離できていない（研究 発見 4）。分離する前の最適化は当て推量になる
- **`smoke-startup.ps1` の 5 回ループの終了待ち**（`/symmetric-check` の [要確認]）——同じ機序の候補だが実測していない。**#786（`smoke:startup` の別 flake）が既に OPEN でそちらの担当**であり、本 issue の枝で混ぜると 2 つの検査の赤が同じ PR に乗る。**#786 へ機序の候補として記録する**
- **反復再現 job の dispatch**——外向きの CI 資源を使う。承認とは別に、実行の可否をユーザーへ確認してから行う

---

## 未確定（実装前に潰す）

- [x] **U1** — `-InputTrace` を渡していない run で計器の env がどこから立ったか。**解決**: `[Environment]::SetEnvironmentVariable($name, $null, 'Process')` が変数を消さず**空文字で作る**（PowerShell 7 で実測）。Rust 側の `var_os(...).is_some()` がそれを真と読み、PowerShell 側の `if ($env:...)` は偽と読む。1 反復目の `finally` が境界になる。→ Phase 2 が両側を直す
- [x] **U3** — 遅着の警告が実際に現れるか。**解決**: 現れる。修正後 run で `予算 5000ms を過ぎた評価で成立`（例: `5251ms・7 周`）が 4 反復（iter-001 / 012 / 019 = pass、iter-014 = fail）。**ただし全 30 反復に単体検査由来の `予算 0ms …（2ms・1 周）` が 1 件ずつ混じり、grep では区別できない**。機構は在るが読めない → 範囲外へ記録し #872 へ残す
- [x] **U4**（実装中に発生・解決）— `env_flag` を crate 間で共有するか。**解決**: `snotra-egui-runtime` は `snotra-core` に依存していない（`Cargo.toml` 実測）ため、依存辺を増やさず private な写しを置き、双方の doc で互いを名指しする。**レビューで選び直してよい設計判断として Phase 2 に明記した**

**U2**（計器なしでの型 A の率と分布）は本計画の範囲外へ送ったため、未確定ではなく #872 の次の測定として残す。

---

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の「hook、CI、rules、skills、ガバナンス文書を変更する」に該当——`rust-check` が実行する Pester 配管と測定用 workflow のスクリプトを変更する）
- plan-review: **自己レビューのみ**（高リスクだが、独立レビュー 1 体の起動はユーザーの指示を待つ——本セッションはサブエージェント委譲を既定で行わない設定である）
- 実行した check スキル: `/symmetric-check`（結果は下記）
- エージェント数: 0
- 要対処: **3 件、いずれも計画へ反映済み** —— (1) `Invoke-SnotraEnvironment` という正しい手本の再利用（Phase 2 設計判断）、(2) `run-pester.ps1:49` の同型を対象へ追加、(3) `smoke-startup.ps1` の [要確認] を範囲外＋#786 へ送出
- 未検証: U2（範囲外へ送出。本計画の作業項目はすべて検証手段を持つ）

### 自己照合（`/start-issue` Step 5a の 5 点）

1. **issue の全要件に作業項目が対応するか** — #872 の問い（ゲートに置き続けるか）には「今日の時点で確定できる判断」節が答え、残余の扱いは Phase 1 が 1 つを消し、もう 1 つ（型 A）は範囲外として明示した
2. **境界条件と検証** — `$null` / `HasExited` / 期限切れ / アクセス拒否の 4 経路を `Stop-SnotraProcessAndWait` の作業項目に列挙し、それぞれ単体検査を置く
3. **新しいリソース・プロセスの正常/失敗/破棄経路** — 新規リソースは作らない。既存プロセスの**破棄経路**に待ちを足すのが本計画である
4. **より単純な既存パターンで置き換えられないか** — **2 軸とも置き換えた。**プロセスの終了待ちは `smoke-egui.ps1:458-466`（#755/#801 是正 B）、env の退避・復元は `Invoke-SnotraEnvironment`（psm1:`267-286`）が既存の手本である。**新しい発明は `env_flag` の写し 1 つだけ**で、それも `src-tauri/src/trace.rs:20` の既存実装と同じ意味論である
5. **壊してはならない不変条件に検知手段があるか** — 「`Reject` の厳しさを緩めない」は既存単体検査（`Tests.ps1:234-249`）が守る。「測定器は exit 0 のまま」はスクリプト末尾の `exit 0` と冒頭 doc が守る

### `/symmetric-check` の結果（実施済み・全判定に根拠あり）

**軸 1: プロセスの生成/破棄**——`Start-SnotraProcess` の呼び出し点 6 箇所を全件評価した。

| 生成 | 破棄 | 後続の起動 | 終了待ち | 判定 |
|---|---|---|---|---|
| `Tests.ps1:356`（seed の It） | `381` | **有り**（`442`） | 無し | **[適用]** Phase 1（実測 3/30 の赤） |
| `Tests.ps1:442`（キャレットの It） | `502` | **無し**（`実機配管` 最後の It。後続の `SnotraTraceInvariants.Tests.ps1` は `Start-Process` 0 件・実測） | 無し | **[適用]** Phase 1（**対称性のためのみ**。この経路に実害は無い） |
| `smoke-egui.ps1:148` | `453` | 有り（`528`） | **`462` `WaitForExit(5000)`** | **[不要]** 既に対称（#755/#801 是正 B） |
| `smoke-egui.ps1:528`（toast） | `627` | **無し**（以降 `Start-SnotraProcess` 無し・集計のみ） | 無し | **[不要]** 衝突相手が居ない |
| `smoke-startup.ps1:63`（**5 回ループ**） | `91` + `Start-Sleep 120` | **有り**（次の反復） | **固定 120ms のみ** | **[要確認]** 下記 |
| `visual-check-colors.ps1:225` | `299` | 無し（起動 1 回） | 無し | **[不要]** |

**[要確認] `smoke-startup.ps1` — #786 へ渡す候補機序。** ループの各反復は `Policy Stop`（`55`）→ 起動（`63`）→ `Stop-Process`（`91`）→ 固定 `120ms`（`92`）で、**待ちが固定時間しかない**。先発が生きたまま後発を起動すると single-instance により**後発は即終了し trace を 1 行も書かない**（機序の正本は `smoke-egui.ps1:458-461` のコメント）。この smoke が「原因未解明」として記録している分散——**5 回中 3 回が丸ごと無音**（`smoke-startup.ps1:66-71`）——は「遅い」ではなく「書かなかった」であり、この機序と整合する。**実測していないので [適用] とはしない。** #786 へ機序の候補として渡す

**軸 2: 計器フラグの真偽（env の設定/復元）**

| 設定 | 復元 | 元が未設定のとき | 判定 |
|---|---|---|---|
| `Invoke-SnotraEnvironment`（psm1:`274`） | `281` / `283` | **`Exists` を別に記録し `Remove-Item`** | **[不要]** 正しい手本 |
| `repro-pester-flake.ps1:126-129` | `143-145` | **`$null` → 空文字で残る** | **[適用]** Phase 2 |
| `run-pester.ps1:49` | 同左 | 同左 | **[適用]** Phase 2（実害なし・同型） |

**軸 3: 同型ペアの取り違え（swap）**——**該当なし**。今回の変更に「同じ型の値を対称な 2 対象へ配線する」箇所は無い（プロセスは 1 つずつ扱われ、env は名前で区別される）。grep パターン: `main.*results` / `tx.*rx` / `from.*to` — 変更対象ファイル内に該当なし

### 適用したトリガー（`AGENTS.md`「条件別チェック」）

- **対称ペア（生成/破棄・フラグ真偽）を変更** → `/symmetric-check` **実施済み**（結果は上記・要対処 3 件を反映）
- **セーフティネットを変更** → `.claude/rules/safety-nets.md`（配送済み・フォールトインジェクションを Phase 1 / Phase 2 の検証へ**両方向で**入れた）
- **関数・型を新規定義** → `Stop-SnotraProcessAndWait` と `env_flag`。**新 API の導入と呼び出し点の移行を 1 タスクに束ねる**（`-D warnings` 下で未使用の新 API は `dead_code` で落ちる）。重複の探索は `/symmetric-check` の過程で手作業により実施し、**既存の同等ロジックを 2 件特定した**（`Invoke-SnotraEnvironment` / `src-tauri` の `env_flag`）——いずれも計画へ反映済み。`/dry-check` は**実装後**（関数が存在する状態）に `/implement` の中で走らせる
- **ガバナンス文書を変更** → `npm run governance:check`（Phase 3）
- **バグ発見時は同一パターン全コードパス検索** → 2 系統とも実施済み
  - `Stop-Process`: `scripts/` の全 12 箇所を列挙。`Resolve-SnotraExistingProcess -Policy Stop` の呼び出し側（`smoke-egui.ps1:138` / `smoke-startup.ps1:55`）が同じ形であることを確認して受け入れ条件 2 に入れた。`bench-startup.ps1` / `measure-memory-stages.ps1` / `visual-check-colors.ps1` は CI のゲートではなく共有ヘルパも通っていないため今回は触らない（#890 の残余として同じ非対称が既に記録されている）
  - 空文字 env: 復元側 4 箇所（`repro-pester-flake.ps1:143-145` / `run-pester.ps1:49`）と読み手側 7 箇所（`snotra-egui-runtime`）を列挙。**実害があるのは読み手が Rust の 1 組だけ**であることを確認した（残る 3 つは読み手も PowerShell で、空文字は一貫して偽）。詳細は `workspace/research.md` 発見 5

---

## 人間レビュー

- [ ] 承認待ち
