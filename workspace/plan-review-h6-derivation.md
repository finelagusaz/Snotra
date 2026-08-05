# #930（trace 不変条件 H6）の独立導出

対象 issue: **#930** — 「`egui_show:done` の直後 `BLUR_GRACE`（100ms）+ ε 以内に `egui_hide:done` が現れたら異常」を `scripts/lib/SnotraTraceInvariants.psm1` へ新設する。

本ファイルはコードと文書だけからの独立導出である（ビルド・テスト・smoke は 1 つも実行していない）。

> **手続き上の申告（先に書く）**: `workspace/plan.md` / `workspace/research.md` は開いていないが、**リポジトリ全体の grep が両ファイルの行を数行だけ結果に混ぜた**（`BLUR_GRACE` の grep で `plan.md:20,81,82,129,263` と `research.md:7,28-35,39-44,79,140`、`focus_state_traces_left` の grep で `research.md:27-35`）。以降は `workspace/` を除外して調べたが、**上記の断片は目に入っている**。とくに `plan.md:20`（payload へ `blur_grace_ms` を載せる案）と `research.md:79`（`auto_hide=false` では再 show 時も hide しない）は本導出の結論と重なる論点なので、**完全に独立とは名乗れない**。以下の根拠はすべて一次資料（コード）で取り直してある。

---

## 0. 結論の要約（先に読む 3 点）

1. **受け入れ条件 3（`reset()` を削除すると `smoke:egui` が赤になる）は、現行の smoke では成立しない見込みが高い。** 理由は §5.1。H6 の実装そのものより、この 1 点が本 issue の主目的（issue 自身がそう書いている）を左右する。
2. **ε は「小さな許容誤差」ではない。** 違反時の hide は show から **約 315〜483ms 後**に着弾する（issue が #746 実測として記す値）。ε を数十 ms に置くと **H6 は違反を跨いで素通りする**。一方 smoke の正当な show→hide 間隔は 1〜3 秒程度と見積もられ、**閾値の置ける幅は 0.5〜1 秒の狭い帯である**（§3.2）。
3. **`blur_grace_ms` が payload に無ければ SKIP（写しへフォールバックしない）という設計は、受け入れ条件 4 を満たすだけでなく、既存 Pester fixture を 1 件も壊さない**（§4.4）。逆に既定値フォールバックを置くと既存テスト 2 件が落ち、かつ条件 4 に反する。

---

## 1. 変更が必要なファイルの一覧（10 ファイル）

### 1-1. `scripts/lib/SnotraTraceInvariants.psm1`（判定器の本体）

| 触る箇所 | 現状（`file:line`） | 何をするか |
|---|---|---|
| モジュール冒頭の判定表 | `scripts/lib/SnotraTraceInvariants.psm1:13-17` | H6 の行を足す（H1/H4/H5 の 3 行の下） |
| イベント名定数 | 同 `:26-29`（`$script:EventShowDone` は `:27` に既存） | 新しいイベント名は要らない。**ε の既定値**を `$script:BlurGraceEpsilonMs` として足す |
| 不変条件の一覧 | 同 `:30` `$script:Invariants = @('H1', 'H4', 'H5')` | `'H6'` を末尾へ足す（H2/H3 は欠番のまま） |
| 判定関数の synopsis | 同 `:109`「H1 / H4 / H5 を判定し、違反を区間へ帰属させる」 | H6 を含める |
| 状態機械の観測帳簿 | 同 `:271` `$observed = @{ ResultsShow = 0; HideWindow = 0 }` | `ShowWindow = 0` を足す（H6 が実際に何件の show を判定したかの肯定的証拠。smoke 側が読む） |
| `egui_show:done` の arm | 同 `:291-297` | hide 窓を閉じる既存処理に加え、**show 窓を開く**（`Seq` / `TsMs` / `GraceMs` / `SectionId` を持つ） |
| `egui_hide:done` の arm | 同 `:275-290` | hide 窓を開く既存処理に加え、**開いている show 窓を閉じ、H6 を判定する** |
| 判定の締め | 同 `:374-389`（H1 を窓ごとに締めるループ） | H6 用の締めループを足す（観測地平による PASS / SKIP・§3.4） |
| fail-safe の `Observed` | 同 `:219` | `ShowWindow = 0` を足す（キーを揃えないと呼び出し側が分岐を強いられる。`Tests.ps1:378-382` がこれを固定している） |
| 表の凡例 | 同 `:626` | H6 の 1 行説明を足す（**この行は手書きの写しである**——`:616-617` のヘッダと `:592-597` の集計は `$script:Invariants` から作られるので自動追随する） |

**自動追随して手を入れなくてよい箇所**（写しを置かない設計が既に効いている）:
`Get-SnotraTraceInvariantNames`（`:41-45`）・`New-SnotraTraceFailSafeResult` の SKIP 行生成（`:195-211`）・区間ごとの判定ループ（`:398-423`）・`Counts`/`Overall` の集計（`:431-438`）・`Get-SnotraTraceFailedInvariants`（`:523-532`）・`Format-SnotraTraceCountSummary`（`:584-598`）・`Format-SnotraTraceVerdictTable` の表本体（`:614-634`）。

### 1-2. `scripts/lib/SnotraTraceInvariants.Tests.ps1`

- `New-TraceEvent`（`:7-15`）は `ts_ms = 1000 + $Seq` を自動で入れる。**H6 の fixture では `ts_ms` を明示できる必要がある**ため、`[long]$TsMs = -1`（既定なら従来の式）のような省略可能パラメータを足すのが最小の変更。
- H6 の Describe ブロックを新設（§4 に列挙）。

### 1-3. `scripts/smoke-egui.ps1`

| 触る箇所 | 現状 | 何をするか |
|---|---|---|
| モジュール import の直前コメント | `scripts/smoke-egui.ps1:59-61`「H1（hide 後に results が取り残されない）の判定はここに書かない」 | H1 だけを名指ししている。H6 を含む形へ（もしくは不変条件一般の言い方へ） |
| 不変条件ブロック | 同 `:410-447` | **FAIL の集計ループ（`:428-432`）は `Get-SnotraTraceInvariantNames` を回すので H6 を自動で拾う**。手を入れるのは肯定的証拠（下） |
| 肯定的証拠 | 同 `:438-440`（`Observed.ResultsShow -eq 0` で赤） | **H6 用の対**を足す: `$invariants.Observed.ShowWindow -eq 0` なら「判定器は `egui_show:done` を 1 件も見ていない」で赤。`Wait-SnotraTraceEvent` が `:288` で show を観測済みゆえ、0 件は判定器側の欠陥である |
| ブロックのゲート | 同 `:410` `if ($resultsChecked -and $failures.Count -eq 0)` | H6 は results と無関係だが、`$resultsChecked` は #804 以降 常に真（`:674-680` が到達不能と断じている）ゆえ実害は無い。**変えないことを明示的に選ぶ**なら理由を 1 行残す |
| シナリオ 2 | 同 `:476-636` | **判定器を 1 度も呼んでいない**。受け入れ条件 3 のためにここが要る（§5.1） |

### 1-4. `scripts/manual-smoke.ps1`（散文のみ・判定は自動追随）

- `:30`「**不変条件（合否）** — H1 / H4 / H5」 → H6 を含める。
- `:433`「合否を名乗れるのは H1 / H4 / H5 の不変条件」 → 同上。
- `:388` の注意文言「H4 / H5 は事実上検査されていない」は `Observed.ResultsShow` を根拠にしており、H6 は `ResultsShow` に依存しない。**H6 を巻き込まないよう文言をそのまま残すか、`ShowWindow` を見る別行を足すかを決める必要がある**（現状の文言は H6 を足した瞬間に「H4/H5/H6」と読み替えられがちだが、H6 については偽）。
- `:60` `$script:InvariantNames = Get-SnotraTraceInvariantNames` は自動追随（写しなし）。
- `$items`（`:75-`）へ H6 用の項目を足す必要は**無い**——`:66-67` が「新機能のために先回りで足さない」と定めている。

### 1-5. `src-tauri/src/egui_shell/lifecycle.rs`

- `:28` `const BLUR_GRACE: Duration = Duration::from_millis(100);` — **値を観測用に外へ出す口**を足す（例: `pub(crate) const fn blur_grace_millis() -> u64`）。doc に「判定のためではなく trace payload へ載せる観測値であること」「唯一の消費者は `window_coordinator::show_egui_main` の payload と H6 であること」を書く。
- `:129-134` `BlurGrace::reset` の doc — 「**検知手段が無い**（受容残余・機械化は #930）」が**偽になる**。H6 が検知手段であること、およびその**射程の限界**（§5.1）へ書き換える。
- `:201-203` テスト `blur_grace_survives_hide_without_reset`（相当）の doc — 「機械化は #930 が追う」が偽になる。

### 1-6. `src-tauri/src/egui_shell/mod.rs`

- `:62-69`「`blur_should_hide` / `blur_grace_action` / `BLUR_GRACE` は re-export しない」——**新しい観測用アクセサを `pub(crate) use lifecycle::{...}`（`:69`）へ足すなら、この規律の射程を明示的に書き直す必要がある**（判定の消費点の一本化は維持、数値の観測は射程外、という切り分け）。⚠️ この切り分けが妥当かは §7-5。

### 1-7. `src-tauri/src/egui_shell/window_coordinator.rs`

- `:406-411` `egui_show:done` の payload（現状 `{ "ms": ..., "height": ... }`）へ `"blur_grace_ms"` を足す。
- `:268-269` の受け皿コメント（payload が読む値の説明）を更新。
- `:501` `egui_hide:done` の payload は空 `{}` のまま**変えなくてよい**（H6 は `ts_ms` しか要らない・§2.1）。`:499-500` のコメント「trace 不変条件（… の H1）が区間の判定に使う」は **H6 も使う**ので更新対象。

### 1-8. `src-tauri/src/egui_shell/launcher_controller.rs`

- `:941-944`（`consume_reset_pending` 内の `self.blur_grace.reset()`）のコメント「（消失に検知手段が無いことは `BlurGrace::reset` の doc が正本）」が**偽になる**。

### 1-9. `src-tauri/CLAUDE.md`

- 「イベント駆動 wake の不変条件（#532 SU5）」の bullet 末尾「blur 猶予は #745 で `BlurGrace::reset` として合流した。**そこに残る受容残余は同 doc が正本**」——残余が縮むので、指し先の doc を直せば整合するが、**「残余がある」という主張自体を見直す**必要がある。
- 「trace の presence 検査は状態の検査ではない」の bullet（`egui_shell/` 節の末尾）は H6 の形（区間内に事象が現れないこと）とそのまま整合するので、**変更不要だが H6 の先例として引用できる**。
- 「Win32 / Tauri 注意事項」の「この事故は presence 検査では捕まらない。検出器は … H1」は **results の事故についての記述であり真のまま**。H6 を混ぜないこと。

### 1-10. `docs/build-commands.md`

- カテゴリ D の bullet「**H1 / H4 / H5 の不変条件は…**」（`:62` 相当・長行）→ H6 を含める。
- 直下の bullet「SKIP は『判定できなかった』…『該当イベントが無い』『`rows` が読めない』『main の可視状態が未観測』『hide 窓が閉じていない』『parse できなかった行がある』」——**H6 が新しい SKIP 理由を 2 つ増やす**（`blur_grace_ms` が読めない／観測時間が閾値に満たない）。この列挙は網羅を装っているので追随が要る。

---

## 2. 判定ロジックの設計

### 2.1 判定に必要な入力が trace のどこから来るか（すべて実在を確認済み）

| 入力 | 出所 | 確認した根拠 |
|---|---|---|
| 時刻 | 各 trace 行の `ts_ms` | `src-tauri/src/trace.rs:45-56` — `SystemTime::now()` の epoch ミリ秒を**全イベントに無条件で**載せる。`seq`（`:43-44`）は全順序を与えるが時間を与えない |
| `ts_ms` の読み | `Read-SnotraTraceSnapshot` が `ConvertFrom-Json` で丸ごと parse | `scripts/lib/SnotraSmoke.psm1:514-544` と `ConvertFrom-SnotraTraceLine`（`:484-`）。**新しい parse は要らない**。数値化は既存の `ConvertTo-SnotraTraceInt64`（`SnotraTraceInvariants.psm1:75-81`）を通す |
| show の事象 | `egui_show:done` | `src-tauri/src/egui_shell/window_coordinator.rs:406-411`。判定器側の定数は `$script:EventShowDone`（`SnotraTraceInvariants.psm1:27`）に**既存** |
| hide の事象 | `egui_hide:done` | 同 `:501`。定数は `$script:EventHideDone`（`:26`）に**既存** |
| 閾値 `BLUR_GRACE` | **現状 trace に無い。新設が要る** | `src-tauri/src/egui_shell/lifecycle.rs:28` の private const。`egui_shell/mod.rs:62` が re-export を禁じている |
| ε | 判定器側の定数（新設） | `BLUR_GRACE` の写しではないので判定器が持ってよい（§3.2） |

**`BLUR_GRACE` の届け方は 3 案あり、payload 案を採る。**

- **案 A（採用）: `egui_show:done` の payload へ `blur_grace_ms` を本体が載せ、判定側はそれを読む。読めなければ SKIP。**
  - 利点: **実際に走っているバイナリの値**が届く（ソースと exe が食い違う状況でも正しい）。show ごとに届くので、将来 config 化されても追随する。判定側に写しが 1 つも生まれない（条件 4 を構造で満たす）。
  - 代償: `lifecycle.rs` の値を `mod.rs:62` の規律を跨いで外へ出す（§7-5 で ⚠️）。
- **案 B（却下）: 判定器または smoke が `-BlurGraceMs` を引数で受ける。** 呼び出し側が値を書くので、条件 4 が禁じている写しがスクリプト側に生まれるだけである。
- **案 C（却下）: PowerShell が `lifecycle.rs` を正規表現で parse する。** (1) 走っている exe ではなくソースを見るので、古いバイナリに対して静かに嘘をつく (2) `AGENTS.md`「検証の作法」が「計画に書いた判定ロジック（正規表現…）は実装前に代表入力で実行して測る」と要求する類の脆さを、恒久の検査に埋め込む。

### 2.2 判定の形（状態機械への入れ方）

既存の状態機械（`SnotraTraceInvariants.psm1:273-372`）は 1 パスで `seq` 昇順に舐める。H6 はその中に **「show 窓」** を足す形で入る——H1 の「hide 窓」の**鏡像**である。

```
egui_show:done を見たら:
    （既存）開いている hide 窓を閉じ、$mainState = 'visible'
    （新規）show 窓を **常に開き直す**:
        @{ SectionId; Seq; TsMs; GraceMs = data.blur_grace_ms; Closed = $false; Violated = $false }
        $observed.ShowWindow++

egui_hide:done を見たら:
    （新規）show 窓が開いていれば、それを閉じて H6 を判定:
        TsMs / GraceMs のどちらかが読めない → Unjudgeable（SKIP）
        else 差分 = hide.TsMs - show.TsMs
             差分 <= GraceMs + ε → Violation（H6）
             それ以外           → Pass（H6）
    （既存）hide 窓を開く／$mainState = 'hidden'
```

**H1 との非対称を 2 つ、意図として書き残すこと**:

1. **連続する `egui_hide:done` は hide 窓を「延長」する**（`:276-288` の既存コメント）が、**連続する `egui_show:done` は show 窓を「打ち直す」**。理由: `show_egui_main` は呼ばれるたびに `reset_pending` を立てる（`window_coordinator.rs:255`）ので、**猶予の起点は最後の show である**。延長側の意味論にすると、2 回目の show の 50ms 後の hide が「1 回目の show から 550ms」として PASS に化ける。
2. **1 つの show 窓に対する 2 発目以降の hide は数えない**（`Closed` で閉じる）。`hide_egui_main` は可視性ガードの無い listener からも呼ばれて無条件に trace を出す（`:276-278` の既存コメント）ため、違反 1 件が 2 件に増える。

**区間への帰属は show 側の `SectionId`** とする（操作が起きたのは show の区間である）。H1 が hide 窓の `SectionId` へ帰属させているのと同じ考え方の鏡像。

### 2.3 issue の「設計上の論点」1〜4 への、コードから導かれる答え

#### 論点 1: 正当な即時 hide をどう除外するか

**除外できない——trace は hide の理由を持たない。** 一次資料:

- `LauncherController::emit_hide`（`launcher_controller.rs:199-209`）は `EGUI_HIDE_REQUESTED` を emit するだけで trace を出さない。blur 由来（`on_focus_changed` の `BlurAction::Hide`・`:1077`）も Escape 由来も launch 完了由来も**同じ 1 本の関数**を通る。
- `hide_egui_main` が出す `egui_hide:done`（`window_coordinator.rs:501`）の payload は空 `{}` で、由来を持たない。
- 全 trace 呼び出し点を数え上げても、hide の理由を運ぶイベントは存在しない（`trace_main(` の grep で 9 箇所。`egui_show:no_window` / `egui_show:ime_control` / `egui_show:done` / `egui_hide:done` / `egui_results:show` / `egui_results:hide` / `egui_tool_launch` / `egui_update_install_noop` / `egui_update_install_returned` / `hotkey:listener_enter`）。

ゆえに issue が指示するとおり**「区間内に事象が現れないこと」の形**で書くしかなく、正当な即時 hide の排除は**判定ではなく入力（smoke の操作列）が担う**。`smoke:egui` は Escape を show の数秒後に送る（`:385-387`）ので区間には入らない。**この依存を判定器の doc に書くこと**——「H6 は『show 直後に人が hide しない』という入力側の前提の上で意味を持つ」。

⚠️ `manual-smoke.ps1` は人間が操作するため、この前提が保証されない（§7-6）。

#### 論点 2: ε をいくつにするか

**ε は「小さな許容誤差」ではなく「猶予明けフレームのスケジューリング遅延」を覆う量である。** 導出:

- 違反時の hide が着弾する時刻は 2 つの経路で異なる:
  - **強い経路**（hide 時の状態が `Blurred(t_old)`）: 再 show 後の最初の非 focus フレームで `elapsed = now - t_old` が 100ms を遥かに超え、**その場で** `BlurAction::Hide`（`lifecycle.rs:157-165`）。show からミリ秒〜数十 ms。
  - **弱い経路**（hide 時の状態が `Focused`）: 最初の非 focus フレームで `Blurred(now)` へ武装（`:150-156`）→ `Rearm(100ms)` → 猶予明けのフレームで Hide。**issue が #746 実測として記す 315ms / 483ms がこれ**（100ms の猶予 + 215〜383ms のフレーム到来遅延）。
- ゆえに **ε < 約 400ms では弱い経路を取り逃がす**。
- 上限は smoke の正当な show→hide 間隔。`smoke-egui.ps1` シナリオ 1 では show 観測（`:288`）から Escape（`:385`）までに seed 健全性検査・first-run 検査・1 文字打鍵と `egui_results:show` の待ち・`Get-Process msedgewebview2` が挟まる。**1〜3 秒と見積もられるが実測していない。**
- **推奨: ε = 400ms（閾値 = `blur_grace_ms` + 400 = 500ms）を既定とし、smoke に「観測した show→hide の差分」を毎回表示させる**（`smoke-egui.ps1:266-275` が起動レイテンシに対して既に採っている流儀。予算に触れる前に人が読める）。

⚠️ **閾値の置ける帯は 0.5〜1 秒程度しかなく、両端とも未実測である**（§7-4）。実装前に 1 度、通常の `smoke:egui` で show→hide の実差分を測ることを強く勧める。

#### 論点 3: どの smoke で走らせるか

- **シナリオ 1（`smoke-egui.ps1:231-448`）は判定器を既に呼んでいる**（`:416`）ので、H6 は `$script:Invariants` へ足すだけで自動的に乗る（FAIL の集計ループ `:428-432` が `Get-SnotraTraceInvariantNames` を回す）。**受け入れ条件 1 はこれで満たされる。**
- ただし**シナリオ 1 は show を 1 回しか行わない**。`BlurGrace` の初期値は `NeverFocused`（`lifecycle.rs:117-121`）で、hidden 中は `update()` が走らない（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）ため、**プロセス起動後の最初の show では `reset()` の有無が観測上まったく区別できない**。→ 受け入れ条件 3 には**シナリオ 2（show 2 回・`:543-629`）が要る**。だがシナリオ 2 は別の trace ファイル（`$toastErrPath`）を使い、**判定器を 1 度も呼んでいない**。
- issue が書く「現状この smoke は設定サイドカーの起動経路を踏まない設計」は正しい（`:336-340` が `cmd:launch_settings_process:` の 0 件を逆に検査する）。**射程の限定（H6 が捕まえるのは reset の消失であって `set_focus()` 失敗そのものではない）は、判定器の doc・`BlurGrace::reset` の doc・issue の 3 箇所で一致させること。**

#### 論点 4: フォールトインジェクションで赤を確認する

- Pester 側（`SnotraTraceInvariants.Tests.ps1`）の合成事象列による赤は**確実に作れる**（§4）。
- 実機側（受け入れ条件 3）は**そのままでは赤にならない見込み**（§5.1）。ここが本 issue の急所である。

---

## 3. 判定の細部（実装者が落としやすい分岐）

### 3.1 `blur_grace_ms` が読めないとき

**SKIP（Unjudgeable）にし、既定値へフォールバックしない。** 受け入れ条件 4 の要求そのものであり、副次的な利益として既存 Pester fixture を 1 件も壊さない（§4.4）。理由文字列は `H4` の rows 欠落（`:341-346`）の形に揃える。

### 3.2 ε の所在

`$script:BlurGraceEpsilonMs`（判定器のモジュール変数）。**`BLUR_GRACE` とは別の量である**ことを doc に書く（前者は本体の契約値、後者は観測の遅延に対する余裕）。テストから上書きできるよう `Test-SnotraTraceInvariants` に `[int]$BlurGraceEpsilonMs = $script:BlurGraceEpsilonMs` を足すのが素直だが、**呼び出し側（smoke）からは渡さない**（渡すと写しが smoke へ生まれる）。

### 3.3 `ts_ms` が読めないとき

show 側・hide 側のどちらが欠けても SKIP。`ConvertTo-SnotraTraceInt64` を通す（`:75-81` の doc が「裸のキャストを書かない」理由を持つ）。epoch ミリ秒は約 1.78e12 で `[long]` に収まる。

### 3.4 hide が観測されないまま trace が終わるとき（**設計判断が要る**）

H1 が「違反は窓が閉じていなくても FAIL、無違反は SKIP」という非対称を持つ（`:374-389`）のと同型の判断が H6 にも要る。ただし H6 は**逆向きに強く言える**:

- show の後、**`ts_ms` が `show.TsMs + GraceMs + ε` を超える事象を 1 件でも観測した**なら、その間に hide が無かったことは確定する（`seq` は単一 `AtomicU64` の全順序・`trace.rs:43-44`）。→ **PASS**。
- そこまでの観測が無い（trace がその前に途切れた）なら → **SKIP**。

この「観測地平」規則を置かないと、シナリオ 2 の最終 round のように **hide で終わらない show がすべて SKIP へ落ちる**。⚠️ 前提は `ts_ms` の単調性（§7-3）。

### 3.5 `Observed.ShowWindow` を足す理由

`.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」。`smoke-egui.ps1:438-440` が `ResultsShow` に対して既にやっていることの H6 版であり、**イベント名がドリフトしたとき H6 が黙って 0 件判定になるのを検出する唯一の手段**である。判定器が数えること（呼び出し側が生イベントから数え直さないこと）は `:268-270` の既存コメントが理由を持つ。

---

## 4. 追加すべきテスト（`scripts/lib/SnotraTraceInvariants.Tests.ps1`）

既存の形（`New-TraceEvent` で合成 → `Test-SnotraTraceInvariants` → `Get-Verdict`）に合わせる。`New-TraceEvent`（`:7-15`）へ `ts_ms` 明示のパラメータを足す前提。

### 4.1 `Describe 'Test-SnotraTraceInvariants — H6（show 直後の hide）'`

| # | It | 事象列の骨子 | 期待 |
|---|---|---|---|
| 1 | `H6: show の直後 BLUR_GRACE + ε 以内の egui_hide:done は FAIL` | show(ts=1000, `blur_grace_ms`=100) → hide(ts=1150) | `H6` = FAIL / `Violations` に 1 件 |
| 2 | `H6: 閾値より後の egui_hide:done は PASS（正当な Escape / hotkey トグル）` | show(ts=1000, grace=100) → hide(ts=3000) | `H6` = PASS / `Violations` 0 件 |
| 3 | `H6: 境界のちょうど上は PASS、ちょうど下は FAIL` | 閾値 = 100 + ε の直前・直後 2 本 | 片側ずつ |
| 4 | `H6: payload に blur_grace_ms が無ければ SKIP（既定値へフォールバックしない）` | show(ts=1000, payload 空) → hide(ts=1010) | `H6` = SKIP / `Unjudgeable` に H6 が 1 件 / `Violations` 0 件 |
| 5 | `H6: blur_grace_ms が非数値でも例外にならず SKIP` | `blur_grace_ms = 'ひゃく'` | `JudgeFailed` = false / `H6` = SKIP |
| 6 | `H6: ts_ms が読めない show（または hide）は SKIP` | `ts_ms` 欠落 or 非数値 | `H6` = SKIP |
| 7 | `H6: 連続する egui_show:done は猶予の起点を打ち直す（H1 の hide 窓の延長とは非対称）` | show(1000) → show(2000) → hide(2050) | `H6` = FAIL（延長意味論なら PASS になる回帰点） |
| 8 | `H6: 1 回の show に対する連続 hide の違反は 1 件（窓を打ち直さない）` | show(1000) → hide(1050) → hide(1060) | `Violations` の H6 が 1 件 |
| 9 | `H6: hide が来なくても、閾値を越える事象を観測していれば PASS（観測地平）` | show(1000, grace=100) → `egui_results:show`(ts=3000) | `H6` = PASS |
| 10 | `H6: 観測が閾値に届かないまま終われば SKIP（時間が足りないことを合格と読ませない）` | show(1000) だけ | `H6` = SKIP |
| 11 | `H6: 違反は show のあった区間へ帰属する` | 区間 2 つ・境界を跨ぐ show/hide | `Violations[0].SectionId` が show の区間 |
| 12 | `H6: 判定した show の件数が Observed.ShowWindow に載る` | show 2 件 | `Observed.ShowWindow` = 2 |
| 13 | `H6: イベント名がドリフトすれば Observed.ShowWindow が 0 になる` | `egui_show:DONE` 等 | 0（`:367-376` の既存テストの H6 版） |

### 4.2 既存 Describe への追記

- `'Test-SnotraTraceInvariants — 正常列'`（`:57-73`）: **H6 の期待値を足すなら fixture に `blur_grace_ms` と離れた `ts_ms` を入れる必要がある**。足さない場合は現状 SKIP のまま（§4.4）。
- `'Observed（判定器が実際に何を見たか）'` の fail-safe テスト（`:378-382`）: `$r.Observed.ShowWindow | Should -Be 0` を足す。
- `'Get-SnotraTraceInvariantNames'`（`:29-40`）・`Get-SnotraTraceFailureCount` 群（`:456-499`）・`Get-SnotraTraceFailedInvariants`（`:502-519`）・`Format-SnotraTraceCountSummary`（`:385-397`）は **`Get-SnotraTraceInvariantNames` から母集団を引いているので自動追随**。手を入れない。

### 4.3 `Format-SnotraTraceVerdictTable`（`:521-533`）

現行の assert は `\|\s*H1\s*\|` と `FAIL` のみ。凡例行（`psm1:626`）に H6 の説明を足したことを固定する assert を 1 本足すとよい（凡例だけが写しなので、ここが落ちる唯一の場所である）。

### 4.4 既存 fixture が壊れないことの検算（重要）

`New-TraceEvent` は `ts_ms = 1000 + $Seq`（`:11`）を入れるため、既存 fixture の show と hide は**数ミリ秒しか離れていない**。素朴に「payload が無ければ 100ms を既定にする」設計を採ると:

- `:58-72`「正常列」: show(seq1, ts1001) → hide(seq4, ts1004) = 3ms → **H6 FAIL** → `$r.Violations.Count | Should -Be 0`（`:71`）が落ちる。
- `:115-127`「hide 側の非対称」: `$r.Violations.Count | Should -Be 0`（`:126`）が同様に危うい（この列に `egui_hide:done` は無いので実際には落ちないが、余白が無い）。

**`blur_grace_ms` 欠落 → SKIP という設計はこの 2 件を構造的に無傷にする。** 条件 4 を満たすことと、既存テストを壊さないことが同じ 1 つの決定から出る。

---

## 5. 見落とされやすい点（「作れば動く」が実際には壊れる／空振りする箇所）

### 5.1 【最重要】受け入れ条件 3 は現行の smoke では成立しない見込みが高い

「`consume_reset_pending` から `BlurGrace::reset()` を削除すると `smoke:egui` が赤になる」を、コードから追うと 3 段の関門がある。

**関門 A: show が 1 回では原理的に区別できない。**
`BlurGrace` の初期値は `NeverFocused`（`lifecycle.rs:115-121`・`LauncherController::new` は `BlurGrace::default()`・`launcher_controller.rs:140`）で、`reset()` も `NeverFocused` へ戻す（`:132-134`）。hidden 中は `update()` が走らない（`src-tauri/CLAUDE.md`）ので、**最初の show の時点で両者は同じ状態である**。シナリオ 1（show 1 回）はこの理由で赤にならない。→ **判定器をシナリオ 2（show 2 回・`smoke-egui.ps1:543-629`）にも回す必要がある。**

**関門 B: hide 時に持ち越される状態が `Focused` だと、再 show でも違反は起きない。**
シナリオ 2 の round 1 は「show → 窓が focus を得る → Escape」なので、hide の時点で `BlurGrace::Focused`。round 2 の show では:

- `show_egui_main` が `window.show()` の後に `set_focus()` を呼ぶ（`window_coordinator.rs:335-341`）。
- `raw.focused` は `WindowEvent::Focused` だけが書き換え（`snotra-egui-runtime/src/input.rs:166-168`）、**`RawInput::take()` を跨いで持ち越される**（egui 0.35 `src/data/input/raw_input.rs:134`）。既定値も `true`（同 `:99`）。
- WM_SETFOCUS は sent メッセージなので `set_focus()` の内側で wndproc が走り、次の `RedrawRequested` フレームより前に `raw.focused = true` になる公算が高い。
- ゆえに round 2 の最初のフレームは `observe(true, ..)` → `Focused` → `Idle`（`lifecycle.rs:143-146`）。**`reset()` の有無で挙動が変わらない。**

**関門 C: 持ち越しが `Blurred` になる状況を smoke が作れない。**
違反が確実に出るのは「hide の時点で `Blurred(t_old)`」の場合（再 show の最初の非 focus フレームで即 `Hide`・`lifecycle.rs:157-165`）。それを作るには:

- 案 (i) blur させてから **100ms 以内に** hotkey で hide する。猶予明けの hide が 315〜483ms 後という実測（issue）を信じれば ~300ms の窓はあるが、**競走に勝つことを前提にした検査**になる。
- 案 (ii) `auto_hide_on_focus_lost = false` の profile で blur させ（`Idle` は武装を解かない・`lifecycle.rs:106-107`）、armed のまま hide → 再 show。**だが再 show 時も `auto_hide` が false なので `blur_should_hide` の第 3 連言（`:89-91`）が false になり hide しない。** この案は閉じている。
- 案 (iii) 途中で config を書き換えて `auto_hide` を true に戻す。`config_watcher` の適用は hidden 中でもフレームを起こさない（`update()` が走らない）ので状態は保たれるが、**関門 B が残る**（再 show で focus を得れば `Focused` へ倒れて違反は消える）。

**ゆえに、条件 3 を素直に満たすには「再 show で `set_focus()` が効かない」状況が要る。** これは #745 の実際の再現条件（サイドカーが前景を握る）そのものであり、issue 自身が「smoke はこれを踏めない」と書いている条件である。取りうる道は 3 つ:

- **(a) テスト用ハッチを本体へ入れる**（例: env で `set_focus()` を抑止する／最初の N フレームの `pre.focused` を false に倒す）。`SNOTRA_EGUI_FAKE_UPDATE`（`smoke-egui.ps1:537`）の先例がある。`.claude/rules/safety-nets.md`「**故障注入は回復機構ごと巻き戻して行う**」がまさにこの形を指示している——ここでの「回復機構」は `set_focus()` の成功である。ハッチ有り + `reset()` あり = 緑（`NeverFocused` は武装しない）、ハッチ有り + `reset()` 削除 = **確実に赤**、という対照が取れる。
- **(b) round 2 の show を hotkey ではなく single-instance 転送で起こす**（`snotra.exe` を再実行する。`src-tauri/src/main.rs:236-244` が `show_egui_main` を呼ぶ）。前景を握っていない側の `SetForegroundWindow` は Windows に拒否されうるので focus を得ない可能性が高いが、**未実測であり「可能性が高い」で機構を組むべきではない**。
- **(c) 条件 3 を「Pester のフォールトインジェクションで赤を確認する」へ緩める。** issue は条件 3 を「本 issue の主目的」と明記しているので、**緩めるならユーザーの裁定が要る**。

**この 3 択は実装着手前に決めるべき分岐である。** 私の推奨は (a)。

### 5.2 シナリオ 2 に判定器を回すと、他の検査が先に落ちて H6 の赤が見えない可能性

`reset()` を削除した round 2 では show の直後に窓が消えるため、`Wait-SnotraWindow`（`:556`）・DWM 高さ測定（`:574-587`）・`egui_show:done` の height 断言（`:563-567`）のどれかが先に赤くなり、**「H6 が捕まえた」と言えなくなる**。`.claude/rules/safety-nets.md`「**注入したことと、注入が正しい強さであることは別である**」の型に当たる。→ **H6 の FAIL が `$failures` に載っていることを名指しで確認する**（他の失敗と一緒くたにしない）。

### 5.3 `$script:Invariants` へ足すだけでは smoke に乗らない経路がある

- `smoke-egui.ps1:428-432` の FAIL 集計は `Get-SnotraTraceInvariantNames` を回すので**乗る**。
- しかし判定ブロック全体が `if ($resultsChecked -and $failures.Count -eq 0)`（`:410`）と `Start-Sleep 1500`（`:411`）の内側にある。**`$failures` が既に 1 件でもあると H6 は 1 度も判定されない。** 「results 検査の失敗」が「H6 が走らなかった」を巻き込む形は残る（受容するなら明記する）。
- `manual-smoke.ps1` は `$InvariantNames`（`:60`）を通すので自動追随。

### 5.4 「hide が来ない show」の扱いを決めないと、smoke の実 trace で H6 が常時 SKIP になる

シナリオ 1 は最後まで hidden で終わるが、**H6 が見るのは show→hide の対**であり、シナリオ 1 には対が 1 つある（show → Escape → hide）。ただし判定は `Start-Sleep 1500` の後の 1 回だけ（`:411-416`）で、その時点で hide は既に観測済み（`:389`）なので対は閉じている。→ **シナリオ 1 の H6 は PASS になるはず**であり、H1 のような「正常値が SKIP」の罠には落ちない。ここは H1 と違うので、`smoke-egui.ps1:418-421` の「smoke は最後まで hidden で終わるため H1 の正常値は SKIP」というコメントを H6 へ拡張してはならない（**偽になる**）。

### 5.5 `mod.rs:62` の規律に触れる

§7-5 参照。

### 5.6 `governance:check` への影響

`.claude/rules/governance-docs.md` の正準形（`` `<対象>`「<見出し>」 ``）は**コードコメントの参照も検査する**（#925）。`src-tauri/CLAUDE.md` の見出しを名指しするコメントを新しく書くなら G-heading-refs に載る。⚠️ 本導出では `npm run governance:check` を実行していないので、既存参照が壊れるかは未確認。

---

## 6. 受け入れ条件との突き合わせ

| 条件 | 導出上の見通し |
|---|---|
| 1. `H6` が `Get-SnotraTraceInvariantNames` に含まれ `smoke:egui` の判定に乗る | **満たせる**（`$script:Invariants` へ足すだけで `:428-432` が拾う）。ただし §5.3 の gate は残る |
| 2. フォールトインジェクション test が H6 の違反を赤で捕まえる | **満たせる**（§4.1 の #1） |
| 3. `reset()` 削除で `smoke:egui` が赤になることを実測 | **そのままでは満たせない見込み**（§5.1）。ハッチ (a) か裁定 (c) が要る |
| 4. `BLUR_GRACE` の値が判定側に写しとして固定されていない | **満たせる**（payload 案 A + 欠落時 SKIP）。ただし §7-5 の代償を伴う |

---

## 7. ⚠️ 確信の持てない所見（12 件）

1. ⚠️ **§5.1 の関門 B（`set_focus()` が最初のフレームより前に効く）は推論である。** `raw.focused` の持ち越しと既定値 `true` は egui 0.35 のソースで確認した（`raw_input.rs:99,134`）が、**tao が WM_SETFOCUS をユーザーコールバックへ届けるタイミング（再入か、キュー経由か）は読んでいない**。ここが逆なら関門 B は消え、条件 3 はハッチ無しで満たせる。**実装前に `egui_input:focus_state`（`view.rs:748-760`・show ごとに先頭 5 フレームの `window_focused` を出す）を実 smoke の trace で読めば、1 回の実行で決着する。**
2. ⚠️ **ε = 400ms は issue が引く #746 実測（315ms / 483ms）からの逆算であり、私は測っていない。** 483ms が上限だという保証も無い（CI runner はより遅い）。
3. ⚠️ **`ts_ms` は `SystemTime::now()` の壁時計であり単調ではない**（`trace.rs:45-48`）。NTP 補正や手動の時刻変更が入ると差分が負や巨大値になる。負の差分をどう扱うか（SKIP か FAIL か）を決める必要がある。**推奨は SKIP**（判定不能）。実行中に時刻が飛ぶ確率は低いが、「判定不能を PASS へ化けさせない」というモジュールの要石（`psm1:19-21`）に照らせば黙って PASS にはできない。
4. ⚠️ **閾値の上限（smoke の正当な show→hide 間隔）を測っていない。** 1〜3 秒という見積もりはスクリプトの構造からの推定であり、遅いマシンで下振れすれば偽陽性になる。
5. ⚠️ **`BLUR_GRACE` の数値を `egui_shell` の外（同 crate 内だが別モジュール）へ出すことが `mod.rs:62-64` の規律に反するかは、規律の読み方次第である。** 当該コメントは「判定の消費点を `BlurGrace::observe` に一本化する」ことを目的として掲げており、**数値の観測用アクセサはその目的を侵さない**と読めるが、条文は `BLUR_GRACE` を名指しで禁じている。ルート `CLAUDE.md`「意図的なリファクタリングの結果を元に戻さない」に触れうるので、**規律の射程を書き換えるならユーザーの確認を取るのが安全**。
6. ⚠️ **`manual-smoke.ps1` での偽陽性**: 人間が hotkey トグルを素早く 2 回押すと show→hide が 500ms 以内になりうる。`manual-smoke` は exit code を持つ（`Get-SnotraTraceFailureCount`・`:367`）ので、H6 の FAIL は記録と exit code の両方へ出る。**「人間の操作では偽陽性がありうる」を記録の文言へ入れるか、`manual-smoke` では H6 を除外するかの判断が要る**（後者は写しを作るので推奨しない）。
7. ⚠️ **観測地平による PASS（§3.4）の健全性**は `ts_ms` の単調性に依存する（同 3 と同根）。また `seq` 順に並べた後の `ts_ms` が非単調でも状態機械は落ちない設計にすること。
8. ⚠️ **シナリオ 2 に判定器を回すと、判定器の呼び出し点が 3 箇所（シナリオ 1・シナリオ 2・manual）になる。** `/dry-check` の対象になりうるが、`Read-SnotraTraceSnapshot` + `Test-SnotraTraceInvariants` + 集計ループという 10 行程度の塊であり、共通ヘルパーへ括るか逐語で置くかは判断が割れる。**`smoke-egui.ps1:466-469` が「共有ヘルパと意図的に別である」と書く先例がある**ので、括らない選択も正当化できる。
9. ⚠️ **`egui_show:done` は「既に可視の窓」に対しても出る**（`plan_hotkey` の `ShowNow` は `visible && !hotkey_toggle` でも来る・`lifecycle.rs:15-23`）。打ち直し意味論（§2.2）はこの場合も正しいはずだが、**`show_egui_main` が可視の窓に対して呼ばれたときも `reset_pending` を立てる**（`window_coordinator.rs:255`）ことを根拠にしている。この経路を smoke は踏まない。
10. ⚠️ **launch 完了による hide が show 直後に走る経路**（`finish_launch` → `emit_hide`）が理論上あるが、smoke は起動を行わないので踏まない。実利用者の trace を H6 で判定する場面は現状無い。
11. ⚠️ **`npm run test:powershell` が `SnotraTraceInvariants.Tests.ps1` を確かに拾うことを実測していない**（`package.json` を読んでいない）。既存テストが在るので拾っているはずだが、断定はしない。
12. ⚠️ **`docs/build-commands.md` の SKIP 理由の列挙が「網羅」を主張しているかは読み方次第**（「はすべて SKIP として現れ」と書いてある）。H6 の新しい SKIP 理由を足さないと全称主張が偽になる、というのが私の読みだが、**この列挙が例示のつもりで書かれた可能性はある**。

---

## 8. 実装順序の提案（依存関係だけ）

1. `lifecycle.rs` の観測用アクセサ → `mod.rs` の re-export と規律コメント → `window_coordinator.rs` の payload。**ここまでで `blur_grace_ms` が trace に出る。**
2. `SnotraTraceInvariants.psm1` の H6 判定 → `Tests.ps1`（**赤を先に見る**）。
3. `smoke-egui.ps1` の肯定的証拠（`Observed.ShowWindow`）とコメント修正。
4. §5.1 の分岐を裁定してから、シナリオ 2 への判定器導入 or ハッチ導入。
5. 散文の追随（`lifecycle.rs` の 2 箇所・`launcher_controller.rs`・`src-tauri/CLAUDE.md`・`docs/build-commands.md`・`manual-smoke.ps1`）→ `npm run governance:check`。
