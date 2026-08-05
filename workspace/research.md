# research — issue #930（trace 不変条件 H6 の新設）

## issue の要約

`scripts/lib/SnotraTraceInvariants.psm1` へ不変条件 **H6** を足す:

> `egui_show:done` の直後 `BLUR_GRACE`（100ms）+ ε 以内に `egui_hide:done` が現れたら異常。

既存 H1 / H4 / H5 と同じ形（`$script:Invariants` への追加・`Invoke-SnotraTraceJudgement` での判定・
`SnotraTraceInvariants.Tests.ps1` のフォールトインジェクション test）。番号は振り直さない（H2 / H3 は欠番）。

**受け入れ条件 3 が主目的**である: `consume_reset_pending` から `BlurGrace::reset()` の呼び出しを
削除したとき `smoke:egui` が赤になることを実測する。

## 関連ファイル・シンボル（すべて grep で実在確認済み）

| 対象 | 位置 | 役割 |
|---|---|---|
| `$script:Invariants` | `scripts/lib/SnotraTraceInvariants.psm1:30` | 判定名の SSOT（`@('H1','H4','H5')`） |
| `Invoke-SnotraTraceJudgement` | 同 `:225` | 1 パスの状態機械（`$mainState` / `$openWindow` / `$resultsShown`） |
| `$script:EventShowDone` / `EventHideDone` | 同 `:26-27` | `egui_show:done` / `egui_hide:done` |
| 判定表の凡例（H1/H4/H5 の散文） | 同 `:626` | **写しである。H6 追加時に手で直す必要がある** |
| モジュール冒頭の一覧表 | 同 `:13-17` | 同上（写し） |
| `Test-SnotraTraceInvariants` の doc | 同 `:109` | 「H1 / H4 / H5 を判定し」——同上（写し） |
| フォールトインジェクション test | `scripts/lib/SnotraTraceInvariants.Tests.ps1:75-113` | H1/H4/H5 各 1 件の違反列 |
| 判定の呼び出し（smoke） | `scripts/smoke-egui.ps1:410-447` | シナリオ 1 の末尾のみ。`Get-SnotraTraceInvariantNames` を回して FAIL を集める |
| 判定の呼び出し（手動） | `scripts/manual-smoke.ps1:250-252, 331-441` | 区間つき。`:433` に H1/H4/H5 の**散文の写し** |
| `BLUR_GRACE` | `src-tauri/src/egui_shell/lifecycle.rs:28` | `Duration::from_millis(100)`。**crate 外へ出さない**（`egui_shell/mod.rs:62`） |
| `BlurGrace::reset` | 同 `:132` | doc が「呼び出し点は `consume_reset_pending`・検知手段が無い（機械化は #930）」と明記 |
| `consume_reset_pending` | `src-tauri/src/egui_shell/launcher_controller.rs:917-949` | `self.blur_grace.reset()` は `:944` |
| `on_focus_changed` | 同 `:1072-1086` | 毎フレーム `observe(pre.focused, Instant::now(), auto_hide)` |
| `egui_show:done` 発火点 | `src-tauri/src/egui_shell/window_coordinator.rs:406` | payload に `height` |
| `egui_hide:done` 発火点 | 同 `:501` | payload は空 `{}` |
| `egui_input:focus_state` | `src-tauri/src/egui_shell/view.rs:748-760` | **show ごとに先頭 5 フレーム**だけ `window_focused` 等を出す（#938/#939） |
| `focus_state_traces_left = 5` | 同 `:427` | `was_reset_frame` の中＝show 直後の判定点 |

## 技術的制約（一次資料で確認済み）

1. **trace 行は `ts_ms` を持つ**（`src-tauri/src/trace.rs:45-56`・`SystemTime` の epoch ms）。
   時間差の判定に使える唯一のフィールド。`seq` は全順序を与えるが時間を与えない。
2. **`Read-SnotraTraceSnapshot` は JSON をそのまま parse する**ので `ts_ms` は既に読める
   （新しい parse は要らない）。欠落・非数値は `ConvertTo-SnotraTraceInt64` が `$null` を返す形が既にある。
3. **`BLUR_GRACE` は crate 外へ出ていない**。`egui_shell/mod.rs:62` が
   「`blur_should_hide` / `blur_grace_action` / `BLUR_GRACE` は re-export しない」と定める。
   ゆえに判定側が値を得る経路は「ソースを読む」か「本体に trace で吐かせる」しかない
   （受け入れ条件 4 が「写しとして固定しない」を要求する）。
4. **`config-applied` に trace は無い**（`events.rs:29` の Tauri イベントのみ）。
   smoke から config 適用の完了を観測する手段は現状ない。
5. **smoke:egui が判定器を回すのはシナリオ 1 の末尾 1 回だけ**（`smoke-egui.ps1:410`）。
   シナリオ 2（toast・2 ラウンドの show/hide）は判定器を一切通していない。
6. **CI で走る**（`.github/workflows/e2e.yml:71-72`・`scripts/lib/**` の変更でも起動する）。

## 核心の未解決点 — 現行 smoke は当の改変を捕まえない（分析。実測は未）

`BlurGrace` の状態遷移（`lifecycle.rs:142-167`）から、`reset()` の削除が挙動差を生むのは
**hide の時点で状態が `Blurred(t)`（武装）である**場合に限られる。現行 smoke の操作列では:

- シナリオ 1: hotkey show → 打鍵 → Escape hide。**Escape を処理するフレームは `focused == true`**
  なので `observe(true, ..)` が `Focused` を書き込む → hide 時の状態は `Focused`。
- シナリオ 2: 同じく Escape hide × 2 ラウンド。同上。
- どちらにも**フォーカスを奪う操作が無い**ため、`Blurred` へ入る経路そのものが踏まれない。

`Focused` のまま再 show した場合（＝改変後）は、初フレームが `focused == false` なら
`Blurred(now)` へ武装して `Rearm(100ms)`。**focus が 100ms 以内に届けば `Focused` へ戻って何も起きない。**
届かなければ 100ms 後に hide する——つまり**改変の可視性が focus 到達レイテンシに依存する**。
正しいコード（`reset()` あり）では `NeverFocused` から始まるので、focus が何 ms 遅れても hide しない。

→ **現状のままでは、受け入れ条件 3 は「実測で赤」を安定に示せない**（赤くなるとしても間欠的）。

補足: 初フレームが `focused == false` になりうること自体は設計が想定している
（`BlurGrace::NeverFocused` の doc・`SnotraSmoke.Tests.ps1:525-536` が「`window_focused` が真になる
**最初の**フレーム」を選ぶ形で書かれていることが傍証）。

## 武装状態を hide へ持ち越す条件（列挙）

| # | 条件 | smoke で作れるか |
|---|---|---|
| 1 | blur → 100ms 以内に別経路で hide | **不可**（100ms の競争になる。CI で間欠化） |
| 2 | `auto_hide_on_focus_lost = false` で blur → 武装が恒久化（`Idle` は武装を解かない・`lifecycle.rs:106`）→ hide | 作れる。**ただし再 show 時も `auto_hide` が false なので hide しない**（`blur_should_hide` の第 3 連言） |
| 3 | 2 の後、hidden の間に config を `true` へ書き換えてから再 show | 作れる。**決定的**（時間の競争が無い）。ただし config 適用の観測手段が無い（制約 4） |

いずれも**フォーカスを奪う窓**が要る。`SnotraSmoke.psm1` には `Set-SnotraForegroundWindow` が既にある
（`SnotraSmoke.Tests.ps1:515` で使用）が、**奪う先の窓を smoke 自身が用意する必要がある**
（CI runner に前提できる他窓は無い）。

## 再利用できる既存パターン

- **判定の非対称**（H1）: 違反は窓が閉じていなくても FAIL、無違反は「まだ後続が来うる」ので SKIP。
  H6 は逆に**閉じた show → hide の対が取れたときだけ PASS**を名乗れる（smoke は show 1 回 + hide 1 回で
  終わるので、H6 は実 PASS を出せる＝「検査が走った」ことが表に現れる）。
- **`Observed` 帳簿**（`psm1:271`）: 判定器が何を実際に見たかを返し、呼び出し側が数え直さない。
  H6 では「評価した show→hide 対の件数」を足すのが同型。
- **判定不能を PASS へ化けさせない**: `ts_ms` が読めない・閾値が得られないときは SKIP + `Unjudgeable`。
- **写しを置かない**（`psm1:38-39, 192-194, 616`）: 名前の母集団は `Get-SnotraTraceInvariantNames`。
  ただし**散文の凡例（`:626`）と冒頭の表は写しのまま**なので H6 追加時に手で直す（機構ではない）。

## 実測（2026-08-05・フォールトインジェクション）

`consume_reset_pending` から `self.blur_grace.reset();` を外し（`launcher_controller.rs:944`）、
release ビルド（3m39s）→ `npm run smoke:egui` を実行した。**変更は実測後に `git checkout` で復元済み**
（`git status` で確認）。

**測定 1 — コンパイラが先に捕まえた。** 行を外した瞬間に PostToolUse hook の clippy が落ちた:

```
src-tauri/src/egui_shell/lifecycle.rs:132:19: error: method `reset` is never used
```

`reset()` の**非テスト呼び出し点はこの 1 箇所だけ**なので、削除は `dead_code` になる。
CI も同じ形で走る（`.github/workflows/ci.yml:109` = `cargo clippy --workspace --all-targets -- -D warnings`）。
**受け入れ条件 3 が名指しする当の改変は、今日すでに CI が赤にする。** smoke を走らせるために
`#[allow(dead_code)]` を足す必要があった＝**注入は 2 段階でしか成立しない**。

（#745 当時にこの信号が立たなかった理由も一致する: 当時は `was_focused` / `unfocus_at` の 2 フィールドで、
どちらも他所から読まれていたため「片方の reset を書き忘れる」は dead_code にならなかった。
**#745 の状態機械化そのものが階梯を 1 段上げていた**。）

**測定 2 — smoke は緑のまま通った。** `allow` を添えた注入版で `npm run smoke:egui` が
`egui smoke passed` で終了（exit 0）。**現行 smoke は当の改変を捕まえない**（分析どおり）。

**測定 3 — 再 show の初フレームは focus を持っていた。** `egui_input:focus_state` は
計測した 3 回の show すべてで **1 行目から `window_focused: true`**。
ゆえに「`Focused` を持ち越して初フレームが unfocused なら武装する」経路も踏まれない
（分析では「focus 到達レイテンシ次第で間欠的に赤」と見ていたが、**実測では一度も武装しない**）。

**測定 4 — 正当な show→hide の間隔（H6 の閾値の上限を決める材料）。**

| 経路 | `egui_show:done` → `egui_hide:done` |
|---|---|
| シナリオ 1（show → 打鍵 → results → Escape） | **353 ms** |
| シナリオ 2 round 1（show → 300ms 待ち → DWM 実測 → Escape） | **487 ms** |

対して、武装持ち越しによる不正な hide は**再 show の初フレーム**で出る（`focus_state` の実測では
show から 1 行目まで 33ms / 11ms）。**200ms 前後の閾値なら両者を 100ms 以上の余裕で分けられる。**

## 未解決の疑問（plan の未確定欄へ引き継ぐ）

1. 受け入れ条件 3 を満たすために smoke へ「武装を跨がせる」シナリオを足すか（上表 #3）、
   別の検知手段（Rust 側の構造分割）へ振るか、条件 3 を降ろすか。**要求判断ゆえユーザーへ問う。**
2. `BLUR_GRACE` の値を判定側へどう届けるか（写しを作らない形）。
   候補 a: 本体が trace に吐く（例 `egui:blur_grace` / 既存 payload への追加）。
   候補 b: PowerShell が `lifecycle.rs` を正規表現で読む。
   候補 c: 引数の既定値として置き doc で出所を書く（＝写し。条件 4 に反する）。
3. ε（frame レイテンシ + trace 出力の遅れ）の値。#746 の実測では blur→hide が 315 / 483ms。
   ただし H6 が測るのは show→hide であり、武装持ち越しの hide は**初フレーム**で出る（≒数十 ms）。

## 参照した規範

- `.claude/rules/safety-nets.md`（フォールトインジェクションで一度は実測・稼働中のガードを弱めない・
  検出器のカバー範囲を欠落パターンごとに検算・CI 実測は PR 本文のチェックリストへ送る）
- `src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」
- `AGENTS.md`「条件別チェック」→ セーフティネットの変更・件数/導出の入力の変更
