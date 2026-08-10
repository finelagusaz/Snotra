# 調査: 起動計器の未確認な検知器と残置（issue #1009）

## issue の要約

#1000（PR #1006・`1133fa6`）で起動計器（`src-tauri/src/startup.rs` + `scripts/bench-startup.ps1`）が
入り、受け入れ 3 件は満たした。**残ったのは「やらないと決めたこと」ではなく「やっていないこと」**で、
#1009 はその 12 項目を 4 群に分けて持つ。

1. **検知器の未確認**（変異 (a)(c)(d)(e)(i)(j)(l) — フォールトインジェクション未実施）
2. **測って記録するだけのもの**（`GetProcessTimes` と `SystemTime::now()` の順序・`SystemTime` の分解能）
3. **軽微な欠陥と doc の不足**（L-6 / M-5 / L-4）
4. **判断が要るもの**（M-2 — 二重起動が終端を出さずに `exit(0)` する経路）

**射程外は ADR が固定している**（`ADR-no-test-only-injection-in-product-code`）: 製品コードへ測定専用の
注入点を足さないため、変異 (m)〜(r) と `PlatformBridgePending::wait` の channel 切断は**永久に測らない**。

## 関連ファイル・シンボル（すべて grep で実在を確認した）

| 対象 | 役割 |
|---|---|
| `src-tauri/src/startup.rs`（831 行） | 計器本体。`Phase` / `StartupFailure` / `Timeline` / `begin` / `mark` / `finish` / `pre_main_elapsed` |
| `scripts/bench-startup.ps1` | ハーネス。`Test-StartupPayload`（検査 1〜4）・終端待ち・min/p50/max |
| `scripts/lib/SnotraSmoke.psm1` | `New-SnotraVerificationProfile`（`-HotkeyModifier` 既定 `Alt` / `-HotkeyKey` 既定 `Q`）・`Wait-SnotraTraceCondition`・`Start-SnotraProcess` |
| `src-tauri/src/main.rs` | マーク 8 点（164〜504 行）と失敗終端 3 点（289 / 398 / 522 行） |
| `src-tauri/src/platform/mod.rs`（343 行付近） | `RegisterInitialHotkey` の arm。成功側の終端 |
| `src-tauri/src/platform/hotkey.rs`（155 行付近） | `RegisterHotKey(None, 1, MOD_NOREPEAT\|MOD_ALT, VK_Q)` |
| `snotra-core/src/indexer.rs`（666 / 731 行） | `LoadOrScanStats.total_ms = total_started.elapsed().as_millis()` |
| `PERFORMANCE.md`「起動の端から端まで（2026-08-09 計測・release・7 標本）」 | 測定値の記録先（開発機・runner の 2 表） |
| `docs/adr/ADR-startup-instrument-contract-shape.md` | イベント名が意味を運ぶ設計（`ok` の見忘れを沈黙で通さないため） |

**#1023（`ae3335d`・背景再スキャン撤去）が `main.rs` を 187 行変えたが、計器の呼び出し点は無傷である**——
マーク 8 点・終端 3 点・`set_index_load_stats_ms` すべての実在を grep で確認した（`git show ae3335d --
src-tauri/src/main.rs` にマーク行の差分は 1 行も無い）。**「無傷」の射程は呼び出し点までである**——同じ
コミットが `indexer.rs` を 2117 行書き換えており、内側の測り方は動いた（上の L-6 の項）。

### 記録面の乖離 — 起動の表だけが計器の着地以来動いていない（2026-08-10 実測）

`git log 1133fa6..HEAD -- PERFORMANCE.md` は #1023 と #1010 を返すが、**どちらも「起動の端から端まで」の
表（現 1547 行）を触っていない**——ハンクは #1023 が 874 行付近（`index.bin` の版の注意書き）、#1010 が
1443 行付近（「索引の常駐の内訳」節の中）で、いずれもメモリ計測側である（`git show <sha> --
PERFORMANCE.md | grep "^@@"` で範囲を確認した）。同じファイルを開いて別の節を直した者が、起動の表は
直していない。

**乖離の実体は「記録が在ること」ではなく「起動経路が変わったときに bench を測り直す引き金が無いこと」で
ある**——#1023 は `perf(core)!` で索引経路を作り直しながら、起動の数値を測り直していない。**計画の
Phase 0 がその再測定を既に含む**ので、実行すれば #1023 を挟んだ A/B がただで手に入る（計器の `//!` が
自称する存在理由「上流の改修の前後で同じ器を当てられること」の一発目の実証になる）。

## 前提の裏取り（issue の記述と実際が食い違う点）

### L-6「`index_load_unattributed_ms` が負になりうる」は、現在の呼び出し形では起こらない

issue は「**実在の欠陥である**」と書くが、両辺とも切り捨てであり、外側の区間が内側を包む:

- 外側 `to_ms(measured)` = `as_nanos() / 1_000_000` → 切り捨て（`startup.rs:400`）
- 内側 `total_ms` = `total_started.elapsed().as_millis()` → 切り捨て（`indexer.rs:666,688,731,755,780`。
  **`total_started` は `load_or_scan_with_stats_in` の入口で起きる**——#1023 以前は
  `load_or_scan_with_stats` そのものの入口だった。#1023 が委譲の形へ変えたため、`Config::config_dir()`
  の呼び出しが内側の外へ出ている。**包みが広がる向きなので非負性は保たれた**が、**前提が実際に動いた
  実例である**〔2026-08-10 のブレストで実測。L-6 を β へ倒す根拠になった〕）
- 外側の区間は `ConfigLoad` のマーク〜`load_or_scan_with_stats` の呼び出し**後**（`main.rs:168`〜`202`）で、
  内側の全体を真に含む

`a > b ⇒ floor(a) ≥ floor(b)` ゆえ差は非負である。**ただしこれは 2 つの前提に乗った結論であり**
（外側が内側を包むこと・両者が切り捨てであること）、**どちらも機構で守られていない**——マークを呼び出しの
前へ動かす、内側を四捨五入へ変える、のどちらでも黙って負に振れる。`json!` は `i64` で引くので**負値は
panic せず出力に現れる**（現在は誰も見ていない）。

### (e) に既存のハッチを使ってはならない

`platform/mod.rs` に `SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE` が在るが、これは**登録を成功させたまま
失敗イベントだけを流す**（同ファイルのコメントが明記）。`RegisterHotKey` 自体は成功しているので、
これで測るのは代理である（`AGENTS.md`「検証の作法」の「主張は代理ではなく対象そのもので測る」・
`startup.rs` の `//!`「既存のハッチを増やす方向も採らない」）。**issue の指定どおり、他プロセスに
`Alt+Q` を握らせて実際に失敗させる。**

### `Wait-SnotraTraceCondition` は終端の重複を畳む

`SnotraSmoke.psm1:630-640` は `$matched | Select-Object -Last 1` を返す（既定 `MinMatchCount = 1`）。
**終端が 2 行出ても、ハーネスは最後の 1 行だけを見て素通りする**——(a)（一度きり性）の検知器は
ハーネス側にも存在しない。

## 検知器の構造（どの変異を誰が捕まえうるか）

**ハーネスの検査 4 つ**（`Test-StartupPayload`）:

1. キーの過不足（`*_ns` / `*_ms` と 11 個のスカラー）
2. `null` の規則（双方向）。説明者は `first_run` / `include_path_env` / `reached_phase` の 3 つ。
   **`cache_hit` はどの判定にも使われていない**（出力するだけ）
3. 恒等式 `post_main_ns == sum_phase_ns + unmarked_tail_ns`。**構成上ほぼ常に真**で、実際に捕まえるのは
   `sum_phase > post_main`（飽和側）だけ
4. 外部の壁時計との突き合わせ。**上限だけ**（`claimed > observed` で落ちる）。**下限は意図的に置いていない**
   ——trace の到着がポーリング間隔ぶん遅れるため

**帰結**: 検査 4 が上限しか縛らないので、**内側の申告が実際より小さくなる変異（(j)）は原理的に素通りする**。
検査 2 が `ok` / `reason` の**値**を一度も見ないので、**`startup:ready` を失敗時に出す変異（(e)）も素通りする**
——ADR が「イベント名が意味を運ぶ」設計を選んだのに、**その名前を検査する側が値と突き合わせていない**。

## 再利用できる既存パターン

- **変異は複製へ当てる**（`.claude/rules/safety-nets.md`）。製品コード側の変異は作業ツリーの使い捨てビルド、
  ハーネス側の変異（(l)）は `Test-StartupPayload` の複製に当て、稼働中の `bench-startup.ps1` は弱めない
- **検証用プロファイルの既定ホットキーは `Alt+Q`**（`New-SnotraVerificationProfile`）。`bench-startup.ps1`
  の `-UseVerificationProfile` はこれを毎回作り直すので、**占有側が `Alt+Q` を握るだけで (e) の実機失敗を作れる**
  （プロファイルを別に用意する必要が無い）
- 占有側は `hotkey::register` と同じ引数を渡す: `RegisterHotKey(IntPtr.Zero, 1, MOD_NOREPEAT|MOD_ALT = 0x4001, VK_Q = 0x51)`
- **測定値の記録先は `PERFORMANCE.md`「起動の端から端まで（2026-08-09 計測・release・7 標本）」**（既に
  開発機・runner の 2 表と A/B がある）

## 技術的制約

- **`finish()` は単体テストを持てない**——`FINISHED` はプロセス大域の `AtomicBool` で、呼んだテストが
  それを消費して他から観測不能になる。ゆえに (a)(e)(i)(j) は外からしか測れない
- **製品側の変異はそのつど release ビルドが要る**（`cargo build --release -p snotra`）。まとめて当てると
  片方の赤がもう片方を隠すので、変異は 1 つずつ当てる
- **(a) の二重終端を起こす経路は、ADR が「永久に測らない」と決めた経路と同一である**——`//!` が挙げる唯一の
  二重終端（platform 初期化失敗 → `setup_hotkey_listener` が bridge 不在を再観測）は `BridgeError` を
  要求し、注入点なしでは作れない。窓生成失敗は早期 return ゆえ 2 度目が無く、登録失敗は arm の 1 か所だけ
- **CI の実測は PR が在って初めて行える**（`.claude/rules/safety-nets.md`）。runner 側の値が要る項目は
  PR 本文のチェックリストへ送る

## 未解決の疑問（計画で潰す）

1. **`SystemTime::now()` の分解能を Rust 側でどう測るか** — issue が「代理では測らない」と名指ししている
   （PowerShell の 0.0015 ms は .NET の時計）。tight loop で相異なる値の最小差を取る形になるが、
   **常設のテストにはしない**（環境依存の値を assert すると間欠的に赤くなる）
2. **足場（占有スクリプト・分解能の測定バイナリ）をコミットするか** — コミットすれば `AGENTS.md`
   「調査・測定のための一時的な足場」が撤去条件の明記を要求する。scratchpad に置けばその義務は生じないが、
   再現性は doc に書いた手順だけが担う
3. **素通りが実測された変異（(e)(j) が候補）へ検知器を足すか** — ハーネスの変更はセーフティネットの変更で
   あり、`.claude/rules/safety-nets.md` の手順（複製への変異・足ごとの検算）が乗る
