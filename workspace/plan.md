# plan — issue #930（trace 不変条件 H6 の新設）

## 目的

show の直後に **blur 猶予が hide を決める**——「出したばかりの窓を、猶予が明けたと称して消す」——
という**起きてはならないこと**に、機械的な検知手段を置く。#745（blur 猶予の武装が hide を跨いで
持ち越され、再 show の初フレームで自動 hide される）はこの形で現れる。

**issue の字面（`egui_hide:done` を見る）からは 1 点逸脱する**——判定する事象を「猶予由来の hide 要求」
へ絞る。理由は下の「issue 論点 1 への回答」。issue が求めた判定の形（**区間内に事象が現れないこと**）と
番号（H6・欠番は振り直さない）は保つ。

## 受け入れ条件（issue の 4 条件への対応）

| # | issue の条件 | 本計画での満たし方 |
|---|---|---|
| 1 | `H6` が `Get-SnotraTraceInvariantNames` に含まれ `smoke:egui` の判定に乗る | `$script:Invariants` へ追加。smoke 側は既に**名前の母集団を回して FAIL を集める**（`smoke-egui.ps1:428`）ので配線は自動。加えて肯定的証拠（`Observed.ShowWindow`）を assert する |
| 2 | フォールトインジェクション test が H6 の違反を赤で捕まえる | `SnotraTraceInvariants.Tests.ps1` へ合成事象列で 9 件（下 Phase 3） |
| 3 | `BlurGrace::reset()` の呼び出しを削除すると赤になることを実測で確認 | **実測済み・ただし赤にするのは smoke ではなく clippy だった**（`research.md`「実測」）。行を外すと `dead_code` でコンパイルが落ちる（CI の `cargo clippy -- -D warnings` が同形）。**この事実を doc へ書き戻すことで条件 3 を閉じる**（下 Phase 4・詳細は「条件 3 の扱い」） |
| 4 | `BLUR_GRACE` の値が判定側に写しとして固定されていない | 本体が `egui_show:done` の payload へ `blur_grace_ms` を載せ、判定側はそれを読む。読めなければ SKIP（写しへフォールバックしない） |

## 条件 3 の扱い（実測にもとづく再解釈・**人間レビューの主要論点**）

issue は「smoke が赤になること」を主目的に置いた。実測の結果:

- **削除は CI が既に捕まえる。** `reset()` の非テスト呼び出し点は 1 つなので `dead_code` になる。
  smoke を走らせるには `#[allow(dead_code)]` を併せて注入する必要があった＝**2 段階の変異でしか
  再現しない**。「検知手段が無い」という `BlurGrace::reset` の doc の記述は**今日では偽**である。
- **smoke は原理的に捕まえない。** 武装（`Blurred`）のまま hide を跨ぐ状態を作る操作列が無く、
  再 show の初フレームも実測で常に focus 済み（`window_focused: true` が 1 行目から・3/3 回）。
  捕まえさせるには「フォーカスを奪う窓の用意 + auto_hide の config 書き換え + 再 show」という
  シナリオを smoke へ足すことになる（CI で 3 プロセス目の起動・sleep 依存・flake リスク）。

→ **本計画は smoke へシナリオを足さない。** 条件 3 は「既存機構が満たしていることの実測 + doc の
訂正」で閉じ、H6 は**別の欠落**（下記 M2 のうち trace に現れる部分）に対する検出器として置く。

残る欠落の分類（`safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）:

| | 欠陥の形 | 今日の検知手段 |
|---|---|---|
| M1 | `blur_grace.reset()` の呼び出しを消す | **clippy `dead_code`**（実測・ただし「呼び出し点が 1 つである」ことに依存する） |
| M2 | show を跨ぐ**新しい**状態を足して reset 一覧へ入れ忘れる | **無い**（#745 の実際の形）。**H6 が捕まえるのは、その状態が blur 猶予だったとき**（＝猶予由来の hide 要求が禁止区間に出たとき）に限る |
| M3 | `reset()` を残したまま呼び出し位置を誤る | 無い（H6 が結果として捕まえうる——猶予が禁止区間で hide を決めれば同じ形で現れる） |

M2 を構造で塞ぐ（show 跨ぎ状態を 1 つの型へ集約し `*self = Self::new()` で reset＝**足し忘れを
表現不能にする**）のは `launcher_controller.rs` の広い改修であり、**機構の変更と挙動の変更を
1 PR に混ぜない**（#745/#746 の判断）に従って**別 issue へ切り出す**ことを提案する。

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `src-tauri/src/egui_shell/lifecycle.rs` | `blur_grace_ms()` を新設（`const fn`・`pub(crate)`）／`BlurGrace::reset` の doc を実測で訂正 |
| `src-tauri/src/egui_shell/mod.rs` | `blur_grace_ms` を re-export し、「値の観測は出すが**判定**は出さない」区別を既存コメント（`:62`）へ書く |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `egui_show:done` の payload へ `blur_grace_ms` を追加（`:406`） |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `on_focus_changed` の `BlurAction::Hide` arm（`:1077`）で `egui_hide:blur_grace` を trace（**猶予が hide を決めた唯一の点**） |
| `scripts/lib/SnotraTraceInvariants.psm1` | `$script:Invariants` へ `H6`／`$script:EventHideBlurGrace`／`$script:BlurGraceEpsilonMs`／状態機械へ `$showGuard` と H6 判定／`$observed.ShowWindow`／**散文の写し 3 箇所**（冒頭表 `:13-17`・`Test-SnotraTraceInvariants` の doc `:109`・凡例 `:626`） |
| `scripts/lib/SnotraTraceInvariants.Tests.ps1` | H6 の test 群（Phase 3・10 件） |
| `scripts/smoke-egui.ps1` | 肯定的証拠 `Observed.ShowWindow -eq 0` の assert（既存の `ResultsShow` と同型・`:438`） |
| `scripts/manual-smoke.ps1` | 散文の写し **2 箇所**（`:30`「不変条件（合否） — H1 / H4 / H5」・`:433`「合否を名乗れるのは…」）／`:388` の注意行（`ResultsShow` が 0 のとき「H4 / H5 は事実上検査されていない」）の **H6 版**（`ShowWindow` が 0 のとき）を足す。**既存行に H6 を混ぜない**——H6 は `ResultsShow` に依存しないので偽になる |
| `docs/build-commands.md` | `:62` の散文「H1 / H4 / H5 の不変条件」／**直下の SKIP 理由の列挙**（`:64`「…はすべて SKIP として現れ」＝**全称の主張**）へ H6 の新しい SKIP 理由を足す |
| `src-tauri/CLAUDE.md` | 「イベント駆動 wake の不変条件」の「blur 猶予は #745 で `BlurGrace::reset` として合流した。**そこに残る受容残余は同 doc が正本**」——残余が縮むので、指し先の doc の訂正に合わせて主張自体を見直す |
| `scripts/smoke-egui.ps1`（コメント） | `:59-61`「H1（hide 後に results が取り残されない）の判定はここに書かない」が H1 だけを名指ししている／`:418-421`「smoke は最後まで hidden で終わるため H1 の正常値は SKIP」は **H6 にも同じことが言える**（ε=400 の帰結）ので、H6 を含む形へ |
| `src-tauri/src/egui_shell/window_coordinator.rs`（コメント） | `:268-269` の payload 受け皿コメント（`blur_grace_ms` の追加）。**`:499-500` の「trace 不変条件（H1）が区間の判定に使う」は真のまま**——H6 は `egui_hide:done` を判定に使わない（**触らない判断を記録する**） |

## H6 の判定仕様

**不変条件**: `egui_show:done` から `blur_grace_ms + ε` 以内の区間に、**blur 猶予由来の hide 要求**
（新イベント `egui_hide:blur_grace`）が現れたら異常。

### issue 論点 1（正当な即時 hide の除外）への回答——閾値ではなく**事象**で分ける

issue は「show 直後の Escape・hotkey トグルは正当」ゆえの除外を求めている。`egui_hide:done` を
判定対象にすると、この除外は**時間の閾値でしか書けない**。それは smoke（入力を制御できる）では
足りても、**同じ判定器を食う `manual-smoke.ps1` では破れる**——判定器の名前の母集団は
`Get-SnotraTraceInvariantNames` 一本であり（写しを置かない規律）、`Get-SnotraTraceFailureCount` は
H6 の FAIL をそのまま赤に数える。**呼び出し側ごとに H6 を外す**のは母集団の規律を壊すので採らない。

人間の操作が 150ms を下回りうるかには**先行する反対材料がある**——`ADR-blur-grace-single-field-state-machine`
は「猶予（100ms）以内の人間の操作を要求する実機シナリオは実行不能（単純反応時間が〜200ms）」と記録している。
ただしそれが縛るのは**反応**であって、意図した Alt+Q の連打（打鍵の運動プログラム。`plan_hotkey` は
`visible && hotkey_toggle` で即 `HideNow`）は反応時間に縛られない。**この点は未測定である。**
→ **事象で分ける設計の根拠を、この不確かな見積もりに置かない。** 根拠は「人間の速さを推定する必要が
そもそも無くなる」ことであり、閾値方式なら測って裏付けねばならない前提が、事象方式では**消える**。

ゆえに**猶予が hide を決めた瞬間を名指すイベントを本体側に足す**。決定点は
`launcher_controller.rs:1077`（`BlurAction::Hide => self.emit_hide()`）**1 箇所だけ**である。

- 正当な blur 由来の hide は「focus 取得 → blur → `BLUR_GRACE` 経過」を経るため、**show から
  `BLUR_GRACE` 以内には原理的に出ない**（reset-on-show が効いていれば猶予の起点は show より後）。
- Escape・hotkey・起動に伴う hide はこのイベントを出さない＝**構造的に除外される**（誤検出の経路が消える）。
- 判定の形は issue が求めた「**区間内に事象が現れないこと**」のまま。

**`egui-hide-requested` の「値は運ばない」設計（`events.rs:37`）には触れない**——理由を event payload
へ運ぶのではなく、決定点で 1 行 trace するだけなので、`emit_hide` の 6 呼び出し点も
`hide_egui_main` の signature も変えない。

### ε = 400ms（`$script:BlurGraceEpsilonMs`）

閾値 = `blur_grace_ms`（本体が payload で運ぶ）+ ε。**`reset()` の消失は 2 つの経路で現れ、ε はその
遅い方を覆う量である**（独立導出 §2.3 論点 2 の指摘で 50ms から改めた）:

| 経路 | hide 時の状態 | 猶予が hide を決める時刻 |
|---|---|---|
| 強い経路 | `Blurred(t_old)`（武装が hide を跨いだ） | 再 show の**初フレーム**（show から数十 ms） |
| 弱い経路 | `Focused`（持ち越し） | 初フレームで武装 → 猶予明けのフレーム。**#746 実測の 315ms / 483ms がこれ** |

- **弱い経路も #745 の症状である**——正しいコードなら `NeverFocused` から始まるので、focus が
  何 ms 遅れても hide しない。ε = 50 では取り逃がす。
- **偽陽性の条件は「show から 400ms 以内に focus を得て blur し、猶予が明けること」**。
  `ADR-blur-grace-single-field-state-machine` が「猶予以内の人間の操作は実行不能（単純反応時間が
  〜200ms）」を記録しており、窓の出現に反応して別窓へマウスを運ぶ時間を足せばこの帯には収まらない。
  **ただし未測定であるため、判定器の doc に偽陽性の条件を名指しで書く。**
- 捕まえたい側の下界（初フレーム）は**導出であり実測ではない**——実測できたのは同じフレーム位置の
  `egui_input:focus_state` 1 行目までの 11〜33ms で、**不正な hide 要求そのものは一度も観測していない**。

**帰結: `smoke:egui` での H6 の正常値は PASS ではなく SKIP になる。** 閾値が 500ms へ広がると、
シナリオ 1 の trace には区間を閉じる証人（show+500ms より後の事象）が無い——最後の事象は
`egui_hide:done` の +353ms である（実測）。**これは H1 とまったく同じ性質であり**（`smoke-egui.ps1:418-421`
が「smoke は最後まで hidden で終わるため H1 の正常値は SKIP」と書く形）、肯定的証拠は
`Observed.ShowWindow` が担う。観測地平を閉じるための引数（読み取り時刻）を判定器へ足す案は、
**H1 に無い機構を H6 のためだけに増やすので採らない**。

### 状態機械への追加（`Invoke-SnotraTraceJudgement` の 1 パスに合流させる）

- `$script:EventShowDone` の arm: `$showGuard = @{ Seq; Ts; SectionId; GraceMs; Closed = $false }` を
  開く（`Ts` = `ts_ms`、`GraceMs` = `data.blur_grace_ms`）。**開き直しは打ち直す**（新しい show は
  新しい区間）。既存の「hide 窓を閉じる」処理は変えない。
- 新 `$script:EventHideBlurGrace = 'egui_hide:blur_grace'` の arm:
  - 開いている `$showGuard` が無い → Unjudgeable（`先行する egui_show:done が無い`）
  - `Ts` / `GraceMs` / 自身の `ts_ms` のどれかが読めない → Unjudgeable（**どれが読めなかったかを理由に書く**。
    とくに `GraceMs` 欠落は「旧バイナリか payload のドリフト」——**既定値へフォールバックしない**〔条件 4〕）
  - `delta <= GraceMs + ε` → **FAIL**（メッセージに delta・`GraceMs`・ε の内訳を出す）
  - それ以外 → その区間の PASS（正当な猶予 hide を観測した＝区間を閉じる証拠になる）
- **区間を閉じる証人**: `ts_ms` が `Ts + GraceMs + ε` を超える**任意の事象**を見たら `$showGuard.Closed = $true`。
  閉じた区間だけが PASS を名乗り、閉じないまま trace が終わった show は SKIP（「まだ後続が来うる」）。
  **H1 と同じ非対称**（違反は窓が閉じていなくても確定・無違反は閉じて初めて PASS）。
- **`ts_ms` は壁時計であり単調ではない**（`trace.rs:45-48` は `SystemTime::now()`）。NTP 補正等で
  **差分が負になったら SKIP**（判定不能）とする——「判定不能を PASS へ化けさせない」の要石に従う。
  `seq` 順に並べた後の `ts_ms` が非単調でも状態機械が落ちないこと。
- `$observed.ShowWindow`（開いた show 区間の数）を足す——**判定器が実際に見た件数**の帳簿。
- 帰属は **show 側の区間**（殺された show の側で読むほうが意味が通る）。
- **連続する `egui_show:done` は区間を打ち直す**（H1 の hide 窓が「延長」するのと**意図的に非対称**）——
  `show_egui_main` は呼ばれるたび `reset_pending` を立てる（`window_coordinator.rs:255`）ので猶予の起点は
  最後の show である。延長にすると「2 回目の show の 50ms 後の hide」が 1 回目からの経過として PASS に化ける。

**fail-safe / degrade 経路**は `$script:Invariants` を回す既存実装がそのまま面倒を見る
（`New-SnotraTraceFailSafeResult` / D7 の PASS→SKIP 落とし）。写しは足さない。

## 実装順序

### Phase 1 — 本体側（閾値の出所）
- [ ] `lifecycle.rs` に `blur_grace_ms()` を足す。doc に「**判定の消費点ではない**——`blur_grace_action` /
      `blur_should_hide` / `BLUR_GRACE` を外へ出さない規律は維持し、出るのは観測用の数値だけ」と書く
- [ ] `mod.rs:62` のコメントへ同じ区別を 1 文で足し、`blur_grace_ms` を re-export する
- [ ] `window_coordinator.rs:406` の `egui_show:done` payload に `blur_grace_ms` を足す（**追加のみ**——
      既存の `height` / `ms` を読む箇所〔`smoke-egui.ps1:563`〕は壊れない）
- [ ] `launcher_controller.rs:1077` の `BlurAction::Hide` arm で `crate::trace_main("egui_hide:blur_grace", ...)`
      を `emit_hide()` の直前に出す。doc に「**要求レベルである**（`src-tauri/CLAUDE.md`「trace の presence
      検査は状態の検査ではない」——効いたことは意味しない）。H6 が見るのは**この要求が禁止区間に現れないこと**
      であり、要求レベルで正しい」と書く

### Phase 2 — 判定器
- [ ] `$script:Invariants` に `'H6'` を足す（末尾・番号は振り直さない）
- [ ] `$script:BlurGraceEpsilonMs = 50` と、上の「導出」節の要点を doc コメントで置く
- [ ] `$script:EventHideBlurGrace = 'egui_hide:blur_grace'` を既存のイベント名定数の隣へ置く
- [ ] `Invoke-SnotraTraceJudgement` へ `$showGuard` と H6 判定を足す（上の仕様どおり）
- [ ] `$observed` に `ShowWindow` を足す（**`New-SnotraTraceFailSafeResult` のハードコード
      `Observed = @{ ResultsShow = 0; HideWindow = 0 }` にも同キーを置く**——呼び出し側が分岐せずに
      読める形を保つ・既存 test `fail-safe でも Observed のキーが揃う` が縛る）
- [ ] 散文の写し 3 箇所を更新（冒頭表・`Test-SnotraTraceInvariants` の doc・`Format-SnotraTraceVerdictTable` の凡例）

### Phase 3 — Pester（フォールトインジェクション）
- [ ] **前提**: `New-TraceEvent`（`Tests.ps1:7-15`）は `ts_ms = 1000 + $Seq` を自動で入れる。H6 の fixture は
      時刻を明示する必要があるので、省略可能な `[long]$TsMs` パラメータを足す（既定は従来の式＝既存 fixture 不変）
- [ ] **検算（独立導出 §4.4）**: 既存 fixture の show と hide は数 ms しか離れていない。**もし「payload が
      無ければ既定値」を採っていたら `:58-72` の「正常列」が FAIL になって落ちていた**。`blur_grace_ms` 欠落を
      SKIP にする設計が、条件 4 と既存テストの無傷を**同じ 1 つの決定**で満たす（この検算を test の doc に残す）
- [ ] 違反: `show(ts=1000, blur_grace_ms=100)` → `egui_hide:blur_grace(ts=1030)` で H6 が FAIL
      （**変異の強さの根拠**: 武装持ち越しの hide 要求は再 show の初フレームで出る＝同じ位置の
      `focus_state` 1 行目が実測 11〜33ms）
- [ ] **論点 1 の直接の証拠**: `show(ts=1000)` → `egui_hide:done(ts=1030)`（＝人間の即時 Escape /
      hotkey 連打）→ 後続事象(ts=1200) で H6 は **PASS**（正当な即時 hide を誤検出しない）
- [ ] 正常: 同 show → `egui_hide:blur_grace(ts=1400)` で H6 が PASS（猶予の正当な発火）
- [ ] 境界: `egui_hide:blur_grace(ts=1150)`（= 100+50 ちょうど）は FAIL、`(ts=1151)` は PASS
- [ ] 判定不能 1: show の `ts_ms` 欠落 → SKIP + `Unjudgeable` に理由
- [ ] 判定不能 2: show に `blur_grace_ms` が無い → SKIP + 理由（**写しへ落ちないことの証拠**）
- [ ] 判定不能 3: 先行 show の無い `egui_hide:blur_grace` → SKIP + 理由
- [ ] 判定不能 4: 区間を閉じる後続事象が無い show（違反も無い）→ SKIP（H1 と同じ非対称）
- [ ] 判定不能 5: `ts_ms` の差分が負（壁時計の巻き戻り）→ SKIP + 理由
- [ ] 非対称: `show(1000)` → `show(2000)` → `egui_hide:blur_grace(2050)` は **FAIL**（区間を打ち直す。
      延長意味論なら PASS に化ける回帰点）
- [ ] 帰属: 区間 2 つを跨ぐ事象列で、違反が **show のあった区間**へ帰属する
- [ ] `Observed.ShowWindow` が開いた区間の件数を返す
- [ ] イベント名のドリフト（`egui_show:DONE` 等）で `Observed.ShowWindow` が 0 になる
      （`:367-376` の既存 test の H6 版＝**検査が走らなかったことの証拠**）
- [ ] `Format-SnotraTraceVerdictTable` の凡例に H6 の説明が載ることを assert（**凡例だけが写しであり、
      落ちる唯一の場所**である）
- [ ] 既存の「正常列で H1/H4/H5 がすべて PASS」test（`:58`）に H6 を足し、**既存 3 つが引き続き名指しで
      検査されている**ことを保つ（issue「射程の注意」）

### Phase 4 — 呼び出し側と文書
- [ ] `smoke-egui.ps1`: `$invariants.Observed.ShowWindow -eq 0` なら失敗を積む（既存 `ResultsShow` の直後・同じ理由文）
- [ ] `smoke-egui.ps1` のコメント `:59-61` と `:418-421` を H6 を含む形へ（H6 の正常値も SKIP である）
- [ ] **受容する残余を明記する**: 判定ブロック全体が `if ($resultsChecked -and $failures.Count -eq 0)`
      （`smoke-egui.ps1:410`）の内側にあるため、**先行する検査が 1 件でも失敗すると H6 は一度も判定されない**。
      この gate は既存であり本 issue では変えない——変えない選択であることを 1 行残す
- [ ] `manual-smoke.ps1` の写し 2 箇所（`:30` / `:433`）と `:388` の H6 版の注意行
- [ ] `docs/build-commands.md:62` の散文 + `:64` の SKIP 理由の列挙（全称の主張）
- [ ] `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」の受容残余の主張を見直す
- [ ] `lifecycle.rs:201-203`（`blur_grace_resets_stale_arm_across_hide` の対照テストの doc
      「このテストは `consume_reset_pending` の `reset()` 呼び出しが消えたことを検出しない……
      機械化は #930 が追う」）を、決着後の姿へ更新する。**`#930` を名指す生きた散文はこの 2 箇所である**
      （他の 3 箇所は `docs/adr/ADR-blur-grace-single-field-state-machine.md:39,45` と
      `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md:7`＝**凍結された歴史ゆえ触らない**・
      `governance-docs.md`「ADR 本文内の参照は照合されない」）
- [ ] `launcher_controller.rs:943` のコメント「**消失に検知手段が無いこと**は `BlurGrace::reset` の doc が
      正本」——`#930` を含まないため識別子の grep では見えず、**概念ラベル「検知手段」の grep で出た 3 番目の
      生きた散文である**。正本を指す形は保ったまま、事実の断定部分を訂正後の姿へ合わせる
- [ ] `lifecycle.rs` の `BlurGrace::reset` doc を実測で訂正:
      「呼び出し点が**この 1 つである限り**、削除は `dead_code` でコンパイルが落ちる（#930 で実測）。
      2 つ目の非テスト呼び出し点を足すとこの検知は消える。**残る欠落は『新しい show 跨ぎ状態の
      reset 忘れ』であり、trace 側は H6 が結果としての即時 hide だけを捕まえる**」

### Phase 5 — 検証（下「テスト方針」の順に実行し、実装差分を確定させる）
- [ ] `npm run test:powershell`（Pester・判定器の全 test）
- [ ] cargo fmt / clippy / test（`.rs` 編集で PostToolUse hook が自動実行・**沈黙 = 合格**）
- [ ] `npm run governance:check`（`docs/build-commands.md` を触るため）
- [ ] `cargo build -p snotra --release` → `npm run smoke:egui`（カテゴリ C。trace payload と scripts の
      両方を変えるので必須）。**H6 が `Overall` で PASS を名乗り、`ShowWindow > 0` であること**を確認

## 不変条件と異常系

- **判定不能を PASS へ化けさせない**（モジュールの要石）。H6 の 4 つの判定不能はすべて SKIP + 理由。
- **閾値の写しを作らない**。`blur_grace_ms` が読めないときに 100 へフォールバックしない（条件 4）。
- **既存 H1 / H4 / H5 の判定を変えない**。H6 は `egui_show:done` の arm への追記と**新しい arm**で閉じ、
  `egui_hide:done` の arm（hide 窓の開閉）には触れない。
- **`$script:Invariants` を回す既存経路（fail-safe・degrade・表整形・exit code）に写しを足さない。**
- **trace payload の追加は破壊的でない**（既存の読み手は名前で引く）。

## テスト方針と検証コマンド

| 対象 | コマンド | 何を測るか |
|---|---|---|
| 判定器 | `npm run test:powershell` | H6 の FAIL / PASS / 3 つの SKIP / 境界 / Observed |
| 本体 | PostToolUse hook（fmt・clippy・test） | `blur_grace_ms` の追加がビルドと既存 test を壊さない |
| ガバナンス | `npm run governance:check` | 見出し参照・索引 |
| 実機 | `npm run smoke:egui` | H6 が実 PASS を名乗り、判定器が show 区間を 1 件以上見たこと |

**実機で H6 の FAIL は作れない**（武装持ち越しを smoke で作れないことは実測済み）。**赤の実測は
Pester の合成事象列が担う**——fixture の位置（show+30ms）は「不正な hide 要求は再 show の初フレームで
出る」という**導出**と、同じフレーム位置に出る `focus_state` 1 行目の実測（11〜33ms）に接地している。
**不正な hide 要求そのものは一度も観測していない**——この射程を判定器の doc に明記する。

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要。** 挙動を変えない（trace payload の追加は観測であって仕様の状態遷移ではない）。
- `docs/build-commands.md`（H6 の名前）・`manual-smoke.ps1`（同）・`lifecycle.rs` の doc（訂正）は上に含む。
- `docs/adr/`: 新しい否定の知識は「smoke へシナリオを足す案を却下した」——**PR 本文に理由を書き、
  ADR は作らない**（#812 の運用: 否定の知識が生じた決定のみ・この判断は issue 単位で閉じる）。

## 実装時の申し送り（承認後に追記・要件は変えない）

1. **Phase 1 の 4 項目は 1 つの編集列として進める**（分割コミットにしない）。`mod.rs` の re-export だけが
   先に入ると、消費点の無い `pub(crate) use` が `-D warnings` 下で落ちる——`reset()` の削除で実際に踏んだ
   `dead_code` と同じ形である（`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）。
2. **`Format-SnotraTraceVerdictTable` の凡例は Phase 2 の最後に書く。** 他の 2 つの写しは名前の列挙だが、
   凡例だけは**意味**を書く行であり、issue の字面（`egui_hide:done` が現れたら異常）を写すと
   **実際に走っている判定と違う説明**が表に出る。実装した `$script:EventHideBlurGrace` の意味論から書く。
3. **trace イベント名の追加としての確認は済んでいる**（`AGENTS.md`「機能削除・trace イベント名／hotkey 登録・
   表示経路の変更」）: `SnotraSmoke.psm1` の待ち受けは**イベント名を引数で受ける**形（`:663`, `:695`）で
   列挙を持たず、`docs/build-commands.md:180-181` の smoke 前提は観測する事象を述べるだけで
   **事象集合の網羅を主張していない**。新しい名前で偽になる散文は `:62` の H1/H4/H5 一覧のみ（Phase 4 に計上済み）。

## セルフレビュー

- リスク: **高**（`docs/build-commands.md` = ガバナンス文書の変更 + セーフティネット〔検出器〕の変更）
- plan-review: **自己レビューのみ**（Step 1 の 7 点を主エージェントが照合）
- エージェント数: **0** — 本セッションには「ユーザーが要求しない限り Agent を起動しない」という
  規約があり、高リスクでも独立レビュー 1 体を自動起動しなかった。承認時に選択肢として提示したが
  **ユーザーは言及せず＝要求されなかった**ので起動しない
- 要対処: **3 件**（すべて計画へ反映済み）
  0. **issue 論点 1（正当な即時 hide の除外）が未処理だった。** `egui_hide:done` を判定対象にすると、
     同じ判定器を食う `manual-smoke.ps1` で人間の Alt+Q 連打が FAIL になりうる（呼び出し側ごとに
     不変条件を外すと母集団の規律が壊れる）。→ 判定する**事象**を `egui_hide:blur_grace` へ変え、
     除外を構造で行う形へ設計変更（**issue の字面からの逸脱・承認の論点**）
  1. `#930` を名指す生きた散文が `lifecycle.rs` に**2 箇所**あった（`:131` の `reset` doc に加えて
     `:203` の対照テストの doc）。片方だけ直すと「機械化は #930 が追う」が残る → Phase 4 へ追加
  2. 先行 show の無い hide 事象（trace が途中から始まった等）で判定対象が空になる経路の扱いが
     未定義だった → Unjudgeable の理由として明記（**沈黙経路を作らない**）
- 確認済み: `SPEC.md` は trace に一切触れていない（grep 0 件）＝ SPEC 同期は不要。
  `.superpowers/sdd/` の H1/H4/H5 言及は**git 管理外の作業記録**（`git log` 0 件）ゆえ更新対象外
- 未検証: (1) **実機での H6 の FAIL**——武装持ち越しを smoke で作れないことが実測で確定しているため、
  赤の実測は Pester の合成事象列が担う。**fixture の位置は導出上の下界であって実測ではない**
  （実測したのは同じフレーム位置の `focus_state` 11〜33ms）。(2) **CI 上での H6 の挙動**
  ——`ci.yml` / `e2e.yml` は `pull_request` でしか起動しないので PR 本文のチェックリストへ送る
  （`safety-nets.md`「CI の実測は PR が在って初めて行える」）

## plan-review 結果（2026-08-05・承認後にユーザーが `/plan-review` を明示起動）

- リスク: **高**
- レビュー方式: **独立導出 1 体**（Step 2b・成果物 `workspace/plan-review-h6-derivation.md`）
- エージェント数: **1**（`workspace/` を読ませない条件で issue の WHAT のみを渡した。
  導出側の申告どおり `BLUR_GRACE` の grep が `plan.md` / `research.md` の数行を混入させたため、
  **完全な独立ではない**——重なった論点は payload 案と `auto_hide=false` の閉塞の 2 つ）

### 要対処（すべて反映済み・一次資料で再照合した）

1. **ε が 1 桁小さかった** — 導出 §2.3 論点 2。`reset()` の消失は**弱い経路**（`Focused` 持ち越し →
   初フレームで武装 → 猶予明けに hide）でも現れ、着弾は show+315〜483ms（#746 実測）。ε=50 では
   取り逃がす。**400ms へ改めた**（帰結として smoke での正常値が PASS → SKIP に変わる。H1 と同性質）
2. **散文の写しが 4 箇所漏れていた** — `manual-smoke.ps1:30`（`:433` しか挙げていなかった）・
   `docs/build-commands.md:64` の SKIP 理由の列挙・`src-tauri/CLAUDE.md` の受容残余・
   `smoke-egui.ps1:59-61`。**`:64` が全称（「…はすべて SKIP として現れ」）であることは一次資料で確認した**
3. **`manual-smoke.ps1:388` の注意行に H6 を混ぜてはならない** — あれは `ResultsShow` が 0 のときの文言で、
   H6 は `ResultsShow` に依存しない。H6 版（`ShowWindow`）を別に足す
4. **`New-TraceEvent` が `ts_ms` を自動生成する**（`1000 + $Seq`）— H6 の fixture には明示パラメータが要る
5. **`ts_ms` は壁時計で単調でない** — 差分が負のときの扱いが未定義だった → SKIP（判定不能）
6. **テストが 4 件足りなかった** — 区間の打ち直し・帰属・名前ドリフトでの `ShowWindow=0`・負の差分
7. **`$failures.Count -eq 0` の gate** — 先行検査が失敗すると H6 は一度も判定されない。既存の gate ゆえ
   本 issue では変えないが、**変えない選択であることを残す**

### 軽微

- 導出の推奨する実装順序（§8）は本計画の Phase 順と一致した（本体 → 判定器 → 呼び出し側 → 散文）
- 導出 §4.4 の検算（`blur_grace_ms` 欠落を SKIP にする設計が既存 fixture を無傷にする）を Phase 3 へ取り込んだ

### 判断の不一致（本計画の側を採る・根拠を突き合わせた）

- **導出は「論点 1 は解決不能」と結論した**（§2.3 論点 1: 「trace は hide の理由を持たない」）が、
  それは**既存のイベントだけを母集団にした結論**である。導出自身が ⚠️6 で「`manual-smoke.ps1` は人間が
  操作するため前提が保証されない」を挙げており、**本計画の `egui_hide:blur_grace` 新設はその ⚠️ を
  構造的に消す**（決定点は 1 箇所・`launcher_controller.rs:1077`）。ゆえに本計画の設計を維持する。
- 導出は受け入れ条件 3 に**テスト用ハッチの本体投入**（(a) 案）を推奨するが、**clippy が既に当の改変を
  捕まえる**という実測（導出はビルド禁止ゆえ到達できない事実）により、ハッチは不要と判断する。

### 未検証

- ε=400 の上限側（正当な猶予 hide が show+400ms 以内に起きうるか）は**未測定**。`ADR-blur-grace-single-field-state-machine`
  の反応時間の記録が傍証だが実測ではない → 判定器の doc に偽陽性の条件を名指しで書くことで受ける
- CI 上での挙動（`pull_request` でしか起動しない）→ PR 本文のチェックリストへ

### 判断

- **実装着手: 可**（要対処 7 件はすべて計画へ反映済み。要件・対象ファイル集合は変わったが、
  設計の骨格〔判定する事象・閾値の出所・番号〕は承認時のまま）

## 未確定（実装前に潰す）

- [x] 現行 smoke は当の改変を捕まえるか — **捕まえない**。注入版 release ビルドで `smoke:egui` が
      exit 0（2026-08-05 実測・`research.md`「測定 2」）
- [x] 改変が他の機構に捕まるか — **clippy `-D warnings` が `dead_code` で落とす**（同「測定 1」）
- [x] 正当な即時 hide をどう除外するか（issue 論点 1）— **閾値ではなく事象で分ける**。猶予が hide を
      決めた点（`launcher_controller.rs:1077`・1 箇所）に `egui_hide:blur_grace` を出し、H6 はそれだけを
      判定する。閾値方式は `manual-smoke.ps1`（人間の Alt+Q 連打）で破れることを確認した
      （`plan_hotkey` が `visible && hotkey_toggle` で即 `HideNow`・判定器の母集団は呼び出し側で絞れない）
- [x] ε の値 — **50ms**。事象で分ける形にしたので ε が吸収するのは trace 書き込み位置のずれだけ。
      下界（初フレーム）は**導出**であり実測ではない（実測は同位置の `focus_state` 11〜33ms）
- [x] `BLUR_GRACE` の届け方 — **`egui_show:done` の payload へ本体が載せる**。ソースの正規表現 parse は
      採らない（`lifecycle.rs` の書式に判定器が結合する）。既定値フォールバックは条件 4 に反するので置かない
- [x] 帰属先の区間 — show 側（殺された show の側）

## 人間レビュー

- [x] 承認済み — 2026-08-05 / 問い: "① 受け入れ条件3 の決着（Rust 側の新規作業はゼロにし、M2 は別 issue へ切り出す）
  ② issue の字面から 1 点逸脱します（判定対象を `egui_hide:blur_grace` にする）……
  `workspace/plan.md` へ注釈を書き足していただくか、**この計画を承認**とお伝えください。" / 回答: "1 OK / 2 OK"
