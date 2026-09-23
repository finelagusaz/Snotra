# research — #1026 起動計器の event と ok を同じ 1 か所から導く

## issue の要約

起動計器（`src-tauri/src/startup.rs`）の終端では、同じ `outcome: Result<(), StartupFailure>` から次の 3 つを**別々の場所で**導いている。

- trace のイベント名（`startup:ready` / `startup:failed`）
- `data.ok`
- `data.reason`

#1009 はその食い違いを外（`scripts/lib/SnotraStartupContract.psm1`）から監視する検査を足した。issue の要求は次の 3 点である。

1. **導出点を 1 つにする**（`finish` の `if outcome.is_ok()` を消す）
2. **Rust 単体テストで、全 `outcome` について `event` と `ok` の対応を固定する**（ハーネスが実機で踏めない variant を含む）
3. **ハーネス側の検査を残すか外すかを決める**（残す判断もありうる、と issue 自身が書く）

## 前提の裏取り（起票 2026-08-10 → 現在）

- `finish` はいまも `outcome.is_ok()` で 2 分岐してイベント名を作る（`startup.rs:542-546`）
- `to_json` はいまも隣接 2 行で `ok`（`:462`）と `reason`（`:463-468`）を作る
- `StartupFailure` の variant は 7 つ（`startup.rs:196-211`）。`Ok(())` を加えた **`outcome` は 8 通り**で、issue の「8 variant」と一致する
- 実機で踏めるのは成功と `HotkeyRegistration` の 2 つだけ（`scripts/occupy-hotkey.ps1`、`startup.rs` `//!`「受容する残余」）。前提は動いていない
- `ok` の消費者は `SnotraStartupContract.psm1:147-157` だけである
  - `bench-startup.ps1:170` は `$terminal.event` で分岐し、`ok` を読まない
  - `smoke-egui.ps1` の `$hk.ok` は `hotkey:registered` の別フィールドである

## 関連ファイル・シンボル（grep で実在を確認済み）

- `src-tauri/src/startup.rs`
  - `StartupFailure`（`:196`）
  - `StartupFailure::reason`（`:215`）、`StartupFailure::reached_the_arm`（`:230`）
  - `Timeline::to_json`（`:337`）
  - `finish`（`:524`）
  - テスト `outcome_is_carried_in_the_payload`（`:917`）、`failure_reasons_are_stable_and_unique`（`:889`。7 variant の手書き列挙）
- `finish` の呼び出し点: `main.rs:284` / `:394` / `:523`、`platform/mod.rs:352`（いずれも `Result<(), StartupFailure>` を渡すだけで、イベント名は知らない）
- `scripts/lib/SnotraStartupContract.psm1`
  - `Test-SnotraStartupPayload` の `event` / `ok` / `reason` 整合ブロック（`:144-157`）
  - その説明（`:55-62`）。「捕まえるのは `to_json`（`ok`）と `finish`（`event`）という**別の場所の導出が食い違うこと**だけ」と書いており、**この変更で偽になる散文**である
- `scripts/lib/SnotraStartupContract.Tests.ps1:145-165`（Pester。「騙る」2 方向と `reason` の食い違い）
- `scripts/bench-startup.ps1:162-167` のコメント（「`event` と `ok` の整合はここを通らないと…届かない」）
- `docs/adr/ADR-startup-instrument-contract-shape.md` §1（イベント名 2 つを維持する根拠）

## 再利用できる既存パターン

- **1 つの match から組で導く**: `StartupFailure::reason` と `From<BridgeError>` は、ハーネスの契約になる文字列を網羅 match 1 か所に集めている（`:235-240` の doc に理由）。イベント名と `ok` も同じ形にできる
- **純粋核でテストする**: `Timeline` は時計を持たない純粋核で、`to_json` が全出力を組み立てる（`:262-265`）。導出を `finish` の外、純粋側へ置けば単体テストで 8 通りを踏める

## 技術的制約

- **ADR §1 によりイベント名は 2 つのまま**。`startup:ready` に `ok=false` を載せる 1 イベント案は却下済み
- **ハーネスの契約（キー `ok` / `reason`、イベント名の文字列）は変えない**。消費者は `.ps1` と `PERFORMANCE.md`
- **`StartupFailure` は添字を持たない**ので、全 variant の列挙は手書きになる。`failure_reasons_are_stable_and_unique` の doc 自身が「variant を足してここへ書き足さなくても落ちない」と受容している。**「全 outcome を網羅」を型で保証する手段は現状無い**
- `finish` は `FINISHED` の CAS と `with_timeline` を持つ非純粋の外殻であり、単体テストから呼べない（`TIMELINE` は static）
- 検証カテゴリ
  - A: `.rs` の変更
  - C: 起動時のホットキー登録の終端 trace は `smoke:startup` が待つ。イベント名は変えないが、`finish` に触れるので該当とみなす
  - E: `scripts/**` のコメントを直す場合
  - F: 文書

## 敵対的調査（3b）の採否 — `workspace/adversarial-1026.txt`

壊せなかった項目:

- イベント名の生成点は `finish` だけである
- `ok` の生成点は `to_json` だけで、消費者は psm1 だけである
- `outcome` は 8 通りで、実機で踏めるのは 2 通りである
- 純粋核へ移せば単体テストで 8 通りを踏める。ただし「現行テストは 2 通りしか見ていない」という補足は正しい。8 通りを踏むテストは新設する
- ADR §1 には抵触しない
- `smoke-startup.ps1` は event と ok を見ない

壊せた項目:

- **採る — ハーネスの整合検査は PR を止めない。** 実バイナリに当てる経路は `smoke.yml` の `Measure startup timeline`（`bench-startup.ps1`・`continue-on-error: true`、`smoke.yml:135-137` を読んで確認）だけである。PR を止めるのは、合成した入力で検査関数を試す Pester（`ci.yml:239` `test:powershell`）だけである。ゆえに**製品の名前と `ok` の対応を PR 時点で縛る検知器は、今は無い**。この変更で足す Rust 単体テストが、最初の検知器になる
- **所見は採り、機序は採らない — 「導出を 1 つにしてもハーネスにしか届かない層が残る」**
  - 採る部分: ハーネスが実プロセスの出力を見るのは事実である
  - 機序 (a) trace の直列化: `trace.rs:44-` は `event` を文字列のまま載せるだけである。その経路の故障は全イベントに及び、名前と `ok` の組に固有ではない
  - 機序 (b) 並行する `finish`: `FINISHED` の CAS が 1 回に絞る。組は 1 回の呼び出しの中で決まるので、並行性で割れない
  - よって、名前と `ok` の組に関してハーネスが単体テストに足すものは「実機で出た 1 行の観測」だけである

## 未解決の疑問

- **ハーネスの整合検査（`psm1:144-157`）を残すか外すか**（issue の 3 つ目の項目）。層の議論は次のとおりで、残すなら説明の散文（`psm1:55-62`、`bench-startup.ps1:162-167`）を書き換える必要がある
  - Rust 単体テストが守るのは「導出が 1 つで、8 通りすべてで名前と `ok` が対応すること」
  - ハーネスが守るのは「実機で出た 1 行が名前と中身で一致していること」。導出が 1 つになると、ハーネスが捕まえられるのは**その 1 つの match 自体の誤り**に限られ、単体テストと重なる
- **全 `outcome` の列挙をどう持つか**。`failure_reasons_are_stable_and_unique` の手書き列挙と共有するか、網羅 match を証人に置いて variant の足し忘れをコンパイルエラーにするか
