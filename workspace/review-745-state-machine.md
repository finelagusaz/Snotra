# #745 独立導出: blur 猶予の状態機械（真理値表と全単射検算）

導出者: 独立レビュー枠（真理値表の再導出）。**手順どおり `workspace/plan.md` の「現行表現との対応」は導出完了後に読んだ**。

> **独立性の申告**: §1–§2 の真理値表と到達可能性は `launcher_controller.rs` / `lifecycle.rs` / `view.rs` の一次証拠のみから導出し、その完了**後**に `gh issue view 745 --comments` と `plan.md` を読んだ。`workspace/research.md` と `workspace/plan-review-745-independent.md` は本レビューの間**一度も開いていない**（`plan.md:197` が記録する前回導出者の汚染とは事情が異なる）。独自に出た所見 ⚠️3（`!focused` 連言が製品経路で死んでいる）は plan にも issue にも無い項目であり、再読ではなく再導出であることの傍証になる。
一次証拠はすべて `file:line`。対象コミット: `9ebf3db`（`fix/blur-grace-reset-on-show`・コード変更なし）。

## 0. 読んだ実装事実（行番号は grep 実測）

- フィールド: `was_focused: bool`（`launcher_controller.rs:104`）/ `unfocus_at: Option<Instant>`（`:105`）。初期化は `:139-140` = `(false, None)`
- 書き手は **4 箇所のみ**（`grep -rn "was_focused\|unfocus_at" src-tauri/src` 実測。他は `view.rs:636` のコメントのみ）:
  - `clear_blur_grace_if_focused`（`:1035-1041`）: `if focused { unfocus_at = None }`
  - `on_focus_changed`（`:1075-1106`）: 武装 `if was_focused && !focused { unfocus_at = Some(now) }`、続いて `if let Some(at) = unfocus_at` で `blur_grace_action(at.elapsed(), focused, auto_hide)` を分岐（`Hide` → `unfocus_at = None` + `emit_hide()`、`Rearm(r)` → `request_repaint_after(r)`、`Idle` → 何もしない）
  - `set_focused`（`:1304-1306`）: `was_focused = focused`
- 純粋核（`lifecycle.rs:60-87`）: `Hide ⇔ !focused && elapsed >= BLUR_GRACE && auto_hide`、それ以外は `elapsed < BLUR_GRACE ? Rearm(BLUR_GRACE - elapsed) : Idle`
- フレーム内の呼び出し順（`view.rs`）: `:323 consume_reset_pending`（両フィールドを触らない = #745 の欠陥）< `:421 clear_blur_grace_if_focused` < 段 15 Escape ラダー（`:422-426`）< `:427 on_focus_changed` < `:997 set_focused`
- **`update()` に `return` は 1 つも無い**（`grep -n "return" src-tauri/src/egui_shell/view.rs` が 0 件）。よって `:427` と `:997` の間に早期脱出は無く、段 34 は毎フレーム必ず走る（後述の「畳んでよい」判断の前提）
- hidden 中は `update()` が走らない（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）

## 1. 真理値表（フレーム 1 枚を通した後の状態と、その回の BlurAction）

記法: 入口状態 `(W, U)` = `(was_focused, unfocus_at)`、入力 `F` = `pre.focused`、`e` = `U` の経過、`A` = `auto_hide`。
出口状態はフレーム末（`:997` 後 = 次フレームの入口）。

| # | 入口 `(W,U)` | `F` | 経過 | 段 14 | 段 16–17 | 出口 `(W,U)` | アクション |
|---|---|---|---|---|---|---|---|
| 1 | `(F,None)` | false | — | no-op | 武装せず・`if let` 不成立 | `(F,None)` | **無し** |
| 2 | `(F,None)` | true | — | `U=None` | 同上 | `(T,None)` | **無し** |
| 3 | `(T,None)` | false | — | no-op | 武装 `U=Some(now)`、`e≈0` → `Rearm(≈GRACE)` | `(F,Some(now))` | `Rearm` |
| 4 | `(T,None)` | true | — | `U=None` | 武装条件偽・`if let` 不成立 | `(T,None)` | **無し** |
| 5 | `(F,Some t)` | false | `e<GRACE` | no-op | `Rearm(GRACE-e)`・**t 保持** | `(F,Some t)` | `Rearm` |
| 6 | `(F,Some t)` | false | `e≥GRACE`, `A=true` | no-op | `Hide` → `U=None` + `emit_hide` | `(F,None)` | **`Hide`** |
| 7 | `(F,Some t)` | false | `e≥GRACE`, `A=false` | no-op | `Idle`（**U は armed のまま残る**） | `(F,Some t)` | `Idle` |
| 8 | `(F,Some t)` | true | 任意 | **`U=None`** | `if let` 不成立 | `(T,None)` | **無し** |
| 9 | `(T,Some t)` | false | 任意 | no-op | **武装が `t` を上書き** `U=Some(now)` → `Rearm(≈GRACE)` | `(F,Some(now))` | `Rearm` |
| 10 | `(T,Some t)` | true | 任意 | `U=None` | 不成立 | `(T,None)` | **無し** |

**導出された不変条件（重要）**: 段 16–17 に到達した時点で `U.is_some()` ⇒ `F == false`。
段 14 が `F==true` のとき必ず `U` を消し、段 16–17 の武装は `!focused` を要求するため。
ゆえに **`blur_grace_action` の `focused` 引数は製品経路で常に false** であり、
`blur_should_hide` の `!focused` 連言と `Idle`-via-focus-recovery アームは**製品経路では死んでいる**
（`lifecycle.rs:117` のユニットテストだけが通る）。製品で `Idle` が出るのは `auto_hide=false` の行 7 だけ。

## 2. フレーム境界での到達可能性

| `(W,U)` | 到達可能か | 経路 / 理由 |
|---|---|---|
| `(false, None)` | **可** | 初期化 `:139-140`。以後も行 1（据え置き）・行 6（`Hide` 後） |
| `(true, None)` | **可** | 行 2/4/8/10（`F=true` の任意のフレーム） |
| `(false, Some t)` | **可** | 行 3（`Focused` から blur した回）→ 行 5/7 で持続 |
| `(true, Some t)` | **不能** | `U=Some` の唯一の書き手は段 16–17 の武装で、これは `!focused` を要求し、**同じフレームの段 34 が `W=false` を書く**（`:997`）。一方 `W=true` を書くのは `set_focused(true)` のみで、その回は段 14（`:1038-1040`）が既に `U=None` にしており、以後 `U` を立てる書き手は無い（武装は `!focused` 要求・`Hide` は `None` を書く）。**早期 return が無い**（§0）ため段 34 は必ず走り、この含意に穴が無い |

行 9/10（入口 `(T,Some)`）は**到達不能な入口**であり、表の完全性のために書いただけである。

## 3. plan.md「現行表現との対応」との差分

**状態対応表（`plan.md:53-58`）・遷移表（`plan.md:62-68`）とも一致。差分なし。**

- `(false,None)→NeverFocused` / `(true,None)→Focused` / `(false,Some t)→Blurred(t)` / `(true,Some)` 到達不能 — 私の §2 と同一。根拠の言い回しも同型（武装フレームの段 34 が `was_focused=false` を書く）
- 遷移表 `focused == true` → `_ → Focused`、`Idle` — 私の行 2/4/8/10 と一致（下の ⚠️1 に注意）
- `NeverFocused + !focused` → 据え置き・`Idle` — 私の行 1 と一致（⚠️1）
- `Focused + !focused` → `Blurred(now)`・`blur_grace_action(ZERO, ..)` — 私の行 3 と一致（⚠️2）
- `Blurred(t) + !focused → Hide` → **`NeverFocused`** — 私の行 6（出口 `(false,None)`）と一致
- `Blurred(t) + !focused → Rearm / Idle` → 据え置き — 私の行 5/7 と一致（`Idle` で armed が残ることを含め一致。`lifecycle.rs:57-59` の doc とも整合）

私が独自に導いて plan に**明示が無い**のは 2 点:

- §1 末尾の「`focused` 引数は製品経路で常に false（＝`blur_should_hide` の `!focused` は死んでいる）」（→ ⚠️3）。plan は両関数を据え置く（`plan.md:92`）ので挙動差は生じない
- **`auto_hide_enabled()` の評価が条件付き → 無条件になる**（→ ⚠️7）。**状態表・遷移表がその形では表現できない種類の挙動差**であり、表の一致は反証にならない

## 4. 全単射の検算（`enum BlurGrace { NeverFocused, Focused, Blurred(Instant) }`）

- **取りこぼし: 無し**。到達可能な同値類は §2 のとおり厳密に 3 つで、3 変種と 1 対 1
- **新たに表現できる不正状態: 無し**。3 変種はいずれも到達可能な組へ写る。`(true, Some)` は**型で構築不能**になり、これは正しい方向の縮小
- ただし **`NeverFocused` は「一度も focus を得ていない」より広い同値類**である: 行 6（`Hide` 後）と行 1 の据え置きも `(false,None)` に落ちるため、`NeverFocused` は実質「**未 focus または猶予消費済みで、かつ現在 focus 無し**」を意味する。**挙動は完全に同一**（両者とも `!focused` で何もせず、`focused` で `Focused` へ）なので全単射は破れない。名前の含意だけが実態より狭い（⚠️4）

## 5. 遷移の全アーム照合（変わっていないか）

| アーム | 現行 | 本案 | 判定 |
|---|---|---|---|
| `Hide` を返した後の状態 | `U=None` + 段 34 で `W=false` ＝ `(false,None)` | `NeverFocused` | **一致**。`emit_hide` の位置も不変（多重は `hide_pending.swap` が潰す・`:198-208`） |
| `focused == true` の回 | 段 14 が `U=None`、段 34 が `W=true` → `(true,None)`。**match には入らず、アクションは発生しない** | `_ → Focused` を返し `Idle` | **等価**（呼び出し側 `:1103` が `Idle => {}` ゆえ観測不能。⚠️1） |
| `Focused + !focused` | 武装 → `Rearm(GRACE - ε)` | `Blurred(now)` + `Rearm(GRACE)` | **ε 差のみ**（⚠️2） |
| `Blurred + !focused, e<GRACE` | `t` 保持・`Rearm(GRACE-e)` | 同 | 一致 |
| `Blurred + !focused, e≥GRACE, auto_hide=false` | `Idle`・**armed 継続** | 同 | 一致（`lifecycle.rs:57-59` の前半は真のまま。plan `:106` の処置と整合） |
| `Blurred + focused` | 段 14 でクリア → `Focused` へ | `→ Focused` | 一致。**`NeverFocused` ではない**ことに注意（plan の `_ → Focused` は正しい） |
| 段 14/34 を畳む（`:421` と `:997` を `:427` へ集約） | Escape ラダー（段 15）を跨ぐ | 集約 | **安全**と判定。段 15 は両フィールドを読まず（`on_escape_pressed` `:1047-1070` 実測）、両者が同フレームで hide を出しても `hide_pending` が潰す。段 34 の前倒しは `update()` に早期 return が無い（§0 実測）ため観測差ゼロ |
| `consume_reset_pending`（`:917`）に `reset()` 追加 | 触らない（欠陥） | `→ NeverFocused` | 意図した挙動変更。reset は `:323` で段 16–17 より**前**に走るので、show 直後フレームは必ず「未武装」から始まる（⚠️5） |

## 6. ⚠️ 確信の持てない所見（確信度つき）

1. **⚠️（確信度: 高／影響: 無）** 現行は `focused==true` の回に `match` へ入らず「アクション無し」、本案は `BlurAction::Idle` を返す。呼び出し側が `Idle => {}` を守る限り等価だが、**`observe` の返り値を将来 trace/計装に使うと差が顕在化する**（「Idle が毎フレーム出る」）。plan の遷移表は両者を同じ `Idle` と書いており、この非対称を記していない。
2. **⚠️（確信度: 高／影響: 極小）** 現行の武装フレームは `Instant::now()`（`:1079`）と `at.elapsed()`（`:1087`）で**時計を 2 回読む**ため実際の残余は `GRACE - ε`。本案は単一 `now` ゆえ `Rearm(GRACE)` ちょうど。**唯一検出できた数値の遷移差**で、方向は安全側（猶予がわずかに長い）かつ underflow クラスを構造で消す。plan `:82` の時計契約はこれを正しく扱っているが、「ε だけ値が変わる」とは書いていない。
3. **⚠️（確信度: 中）** §1 のとおり `blur_should_hide` の `!focused` 連言は**製品経路で常に真**（死んだ連言）。据え置きなので本 PR に影響しないが、`observe` が `Blurred` アームからのみ `blur_grace_action` を呼ぶ形になると、`focused` 引数は**呼び出し側で定数 false**になる。将来「引数を消す」判断をするなら、`lifecycle.rs:48-59` の doc（`focused` は時計と無関係な入力ゆえ再要求しない、の説明）が同時に意味を失う点に注意。**今回は触らないのが正しい**と考える。
4. **⚠️（確信度: 中／命名のみ）** `NeverFocused` は `Hide` 発火後の状態も兼ねる（§4）。「never」は実態より狭い名前で、`Hide → NeverFocused` を読む人が「一度も focus していない状態へ戻す」と誤読しうる。`Idle` / `Unarmed` 等の候補もあるが、**reset-on-show の意図（初期状態と同一へ戻す）を名前で伝える利点**があり、私は改名を推さない。指摘だけ残す。
5. **⚠️（確信度: 中→高に更新／意図的な挙動変更・実機確認の範囲が広がる）** reset 後 `NeverFocused` にすると、**一度も focus を得なかった表示回は、以後どれだけ外をクリックしても自動 hide しない**（focus を失いようがないため）。現行は stale な `was_focused=true` により（誤って）hide していた回がある。SPEC §8.6 の `focus_lost` 解釈（plan `:109`）に従えば新挙動が正しい。
   **追加で確認した事実**: `show_egui_main` は `reset_pending` を**無条件で**立て（`window_coordinator.rs:255`）、`main.rs:421-427` は `plan_hotkey` が `ShowNow` を返す限りそれを呼ぶ。`plan_hotkey(visible=true, alt=false, hotkey_toggle=false) == ShowNow`（`lifecycle.rs:14-20` / テスト `:135`）ゆえ、**`hotkey_toggle=false` の設定では「既に可視な窓へ Alt+Q」で reset が走る**。つまり `reset() → NeverFocused` は hide を跨ぐ場面だけでなく**可視中にも設定次第で発火する**。
   - 害の向き（恒久的に auto-hide 不能）が現実化するには、その reset フレーム以降ずっと `focused == false` である必要があるため、依然 `set_focus()` 失敗と条件を共有する（そこは確信度中のまま）
   - 逆に**利益の側**が広がる: 同じ経路は**今日の #745 バグの再現路でもある**——可視かつ blur 武装中（`Blurred(t)`, `e≥GRACE`, auto_hide=on）に Alt+Q を押して `set_focus()` が失敗すると、hide を一度も挟まずに**再表示の直後に自動 hide される**。plan の異常系（`:151`）は hide 跨ぎだけを挙げているが、この「hide を跨がない」経路も同じ修正で閉じる
   - **実機確認の推奨**: `hotkey_toggle=false` で ①可視中に Alt+Q → バーが残り、外クリックで正常に消えること ②外部アプリに focus を渡した状態で Alt+Q → 100ms 以内に消えないこと、の 2 手を追加する
6. **⚠️（確信度: 低）** 行 9（入口 `(true, Some t)`）で現行は `t` を**黙って上書き**する。到達不能なので現状は無害だが、これは「現行実装が到達不能状態に対して定義済み（かつ安全側）の振る舞いを持っていた」ことを意味する。enum 化で構築不能になる以上検討不要だが、**もし将来 `Focused` と `Blurred` の間に第 3 の書き手（例: hide 経路からのクリア）を足すなら、この上書きが担っていた「常に最新の blur 時刻を使う」性質を誰が担うかを再確認**する必要がある。
7. **⚠️（確信度: 高／遷移表では表現できない挙動差・plan に記載なし）** **`auto_hide_enabled()` の評価タイミングが変わる。**
   現行は `self.auto_hide_enabled()`（`launcher_controller.rs:1089`）が `if let Some(at) = self.unfocus_at`（`:1085`）の**内側の実引数**であり、engine mutex を取るのは**武装中のフレームだけ**である（本体は `:633-643` で `s.engine.lock()`）。
   plan の署名は `observe(&mut self, focused: bool, now: Instant, auto_hide: bool)`（`plan.md:32`）で **`auto_hide` を値で受ける**ため、呼び出し側は武装の有無を知らずに**毎フレーム無条件に engine ロックを取る**ことになる（`BlurGrace` が `is_armed()` を漏らさない限り条件化できない）。
   - **重大度は低い**: `read_visual`（`mod.rs:390`・`view.rs:318`）と `lang()`（`:676`）が既に毎フレーム同じ engine ロックを取っており、増えるのは同種の短いロック 1 回である。新しい競合クラスではない
   - **とはいえ「実質同じコード」と言い切れる変更ではない**（`.claude/rules/src-tauri.md` のロック最小化・`read_visual` の 1 フレーム 1 回規律と同じ土俵）。選択肢は (a) 受容して doc に一行、(b) `auto_hide: impl FnOnce() -> bool` で遅延評価、(c) 呼び出し側で `Blurred` のときだけ読む（型が armed を漏らすので非推奨）。**私は (a) か (b)、plan で明示することを推す**
8. **⚠️（確信度: 低／範囲外）** `view.rs:636` のコメント「`was_focused` に依存しないので、hide→reshow で `was_focused` が stale でも確実に戻る」は、本案後は「stale が起こらない」ため前提が変わる（plan `:95` / `:208` が処置済み）。**主張自体は真のまま**なので、書き換えの必然性は低い。消すのではなく現況へ寄せる plan の判断に同意する。

## 7. 結論

- 私の独立導出と `workspace/plan.md`「現行表現との対応」の**状態表・遷移表は完全一致**（差分なし）
- `BlurGrace` は現行の到達可能状態と**全単射**。取りこぼし無し・新規の不正状態無し・`(true, Some)` の表現不能化は正しい縮小
- 全アームで遷移は保存される。**状態と遷移の中に差は無い**。表の外に出る差は 2 つだけで、⚠️2（`Rearm` 残余の ε）と ⚠️7（`auto_hide_enabled()` の engine ロックが毎フレームへ）。前者は無害、後者は plan に明示を求める
- 段 14/34 の畳み込みは、`update()` に早期 return が無いことと段 15 が両フィールドを読まないことから**安全**（両方を一次証拠で確認）
