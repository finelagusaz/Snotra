# 実装計画 — #745 blur 猶予を reset-on-show の backstop に入れる（案 C: 1 フィールドの状態機械）

## 目的

`unfocus_at` / `was_focused` を hide を跨いで持ち越さない。あわせて、**その保証をユニットテストで固定できる構造**にする。

## 受け入れ条件（issue より）

1. `unfocus_at` / `was_focused` が hide を跨いで持ち越されない
2. 純粋核のテストで固定できる部分（猶予の判定）と、view 層の状態遷移（クリア経路）を**区別して検証**する

## 根本原因（一次資料・当初の記述を訂正）

**#745 は 2 変数分解の帰結ではなく、書かれた決定の実装漏れである。**

- `docs/superpowers/specs/2026-07-22-su2-window-shell-design.md:83` が SU2 の初日から「`focused` のとき `unfocus_at=None`。**加えて show のたびに view 側の `was_focused`/`unfocus_at` をリセット**」を明記
- SU2 プラン `docs/superpowers/plans/2026-07-22-su2-window-shell.md:681` は「spec §blur（100ms・ゲート・サイドカーガード・**stale リセット**・policy を view）→ Task 5（+ codex #8）。**✓**」と、**5 項目を 1 つの ✓ で束ねて**完了扱いにしている
- `68b5f41` が実装したのは前半（`if focused { unfocus_at = None }`）だけ

**情状**: `reset_pending` は SU3 M1（`plans/2026-07-22-su3-m1-core-search.md:1059`）まで存在せず、SU2 時点では掛けるフックが無かった。

**帰結（案 C の正当化の訂正）**: 1 フィールド化しても「`reset()` は show のクリア一覧に入っているか」という問いは残り、**それが初回に失敗したまさにその問いである**。ゆえに案 C の価値は「原因を構造で消す」ことではない。**受け入れ条件 2（テスト可能性）と、入口が 4 → 2 に減ること**である。この区別を曖昧にしない。

## 方針: blur 猶予を 1 フィールドの状態機械にする

```rust
// lifecycle.rs — **時計を読まない**。`now` は呼び出し側が 1 回だけ読んで渡す。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlurGrace {
    /// show 直後。**まだ一度も focus を得ていない**——この状態からは武装しない。
    NeverFocused,
    /// focus を持っている。
    Focused,
    /// focus を失った。猶予の起点を持つ。
    Blurred(Instant),
}

impl BlurGrace {
    /// 段 3: reset-on-show。
    pub(crate) fn reset(&mut self) { *self = Self::NeverFocused; }

    /// 段 16–17: 今フレームの focus を畳み、猶予の処置を返す。
    #[must_use]
    pub(crate) fn observe(&mut self, focused: bool, now: Instant, auto_hide: bool) -> BlurAction;
}
```

**初期値は `NeverFocused`**（`LauncherController::new` の `:139-140` 置換先）。起動直後は窓が hidden で、最初の show が `reset_pending` を立てるため整合する。

**経過の算出は `now.saturating_duration_since(t)` を使う**——`now - t` は `Instant` の `Sub` が panic 経路を持つ。呼び出し側が単調な `now` を渡す限り負にはならないが、**panic 経路を残さない形を選ぶ**（release は `panic = "abort"`）。

| | 現行 | 本案 |
|---|---|---|
| フィールド | 2 | **1** |
| フレーム内の入口 | 4（段 3 / 14 / 16–17 / 34） | **2**（段 3 / 16–17） |
| `view.rs` の呼び出し | 3 か所 | **1 か所** |
| `(true, Some)` の不正状態 | 表現できる（到達不能なだけ） | **表現不能** |

### 現行表現との対応（独立導出 1 体が別経路で全一致を確認）

フレーム境界で到達可能な `(was_focused, unfocus_at)` と enum は全単射:

| 現行（フレーム境界） | enum |
|---|---|
| `(false, None)` | `NeverFocused` |
| `(true, None)` | `Focused` |
| `(false, Some(t))` | `Blurred(t)` |
| `(true, Some(t))` | **到達不能** → 表現不能になるのが正しい |

遷移（全アーム照合済み）:

| 入力 | 現行 | 本案 |
|---|---|---|
| `focused == true` | 段 14 が `unfocus_at=None`、段 34 が `was_focused=true`、アクション無し | `_ → Focused`、`Idle` |
| `NeverFocused` + `!focused` | 何もしない | 据え置き、`Idle` |
| `Focused` + `!focused` | 武装 → `Rearm(GRACE−ε)` | `→ Blurred(now)`、`Rearm(GRACE)` |
| `Blurred(t)` + `!focused` → `Hide` | `unfocus_at=None` + 段 34 で `(false, None)` | `→ NeverFocused`、`Hide` |
| `Blurred(t)` + `!focused` → `Rearm` / `Idle` | 据え置き | 据え置き |

**ε の向きは安全側**である。予約の実効期限は `runtime.rs:318` → `repaint.rs:50-52`（`Instant::now() + delay`）で決まり、現行 = 末尾 +(100ms−ε)、本案 = 末尾 +100ms。**本案が ε だけ遅く、予約が早まらない。**

### 段 14 / 段 34 を畳んでよい根拠

- **読み書きの全列挙**: `was_focused` / `unfocus_at` の `.rs` 内出現は `launcher_controller.rs` の 13 行と `view.rs:636`（コメント）のみ。段 14〜34 の間に外部の読み手はいない
- Escape ラダー（段 15）は両フィールドを読まない。同一フレームで両方が hide を出しても `emit_hide` の `hide_pending.swap(true)`（`:199-208`）が潰す
- **`update()` に `return` は 0 件**——段 34 は必ず走る（`(true, Some)` が到達不能である根拠でもある）
- **「束ねると順序が動く」の doc は `f3c53f1`（#785 の機械的な分割）単独で入った**（`git log -S` 実測）。**分割時の注意書きであって、ライブな制約の記録ではない**。段 14 が段 15 より前にある非データ依存の理由は一次資料に無い
- 段 34 がフレーム末尾にあることは不活性——`set_focused(pre.focused)` は段 13 のスナップショットを渡しており、**末尾で focus を読み直していない**。span が段 3〜34 → 段 3〜17 へ縮むのは安全側

### `blur_grace_action` / `BLUR_GRACE` を lifecycle 内へ閉じる

移行後、両者の crate 内消費点は**削除対象の `:1080` / `:1086` だけ**になる（grep 実測）。`mod.rs:65` の re-export に残すと `unused_imports` が `-D warnings` で error になる。

**それ以上に重要なのは設計上の理由である**——`blur_grace_action` を公開したままにすると `observe` を迂回する経路が残り、**#711 が「消費点の一本化を型で塞ぐ」ことで得たものが散文へ戻る**。`blur_should_hide` と同じ扱い（モジュール外へ出さない）にする。

`mod.rs:65` は `pub(crate) use lifecycle::{BlurAction, BlurGrace, HotkeyPlan, plan_hotkey};` になる（`BlurAction` は呼び出し側が match するので残す）。

### 契約③の凍結判断には抵触しない

凍結されたのは「4 つの armed 期限を**横断する**共通 `Deadline` primitive の抽出」であり、本案は blur 猶予**単体**の凝集である。

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `lifecycle.rs` | `BlurGrace` enum と 2 メソッドを追加。`blur_grace_action` / `BLUR_GRACE` / `blur_should_hide` は**モジュール内に閉じる**。`BlurAction::Idle` の doc へ「猶予明けに」の限定を足す（猶予**中**は auto_hide off でも `Rearm` が返るため現文言は不正確）。`:57-59` の #745 への言及の**後半**を書き換え（前半＝`Idle` でクリアしないは真のまま） |
| `mod.rs` | `:62-64` のコメントを書き換え（「消費点は `blur_grace_action` に一本化」「blur 猶予の 3 件」がともに偽になる）。`:65` の re-export を `{BlurAction, BlurGrace, HotkeyPlan, plan_hotkey}` へ |
| `launcher_controller.rs` | `:104-105` / `:139-140` を `blur_grace: BlurGrace`（初期 `NeverFocused`）へ。`consume_reset_pending`（`:917`）へ `self.blur_grace.reset()`。**`clear_blur_grace_if_focused`（`:1035`）と `set_focused`（`:1304`）を削除**。`on_focus_changed`（`:1075`）を `observe` への委譲へ書き換え、doc を更新。**`auto_hide_enabled()` の結果は `let` へ束縛してから渡す**（`self.blur_grace.observe(…, self.auto_hide_enabled())` は two-phase borrow に依存する形になるため） |
| `view.rs` | `:421`（段 14）と `:997`（段 34）の呼び出しを削除。`:427` へ `Instant::now()` を渡す。**`:332-337` のコメントへ「この位置は #745 の backstop 本体でもある」を追記**（現在は #749 の理由だけで固定されており、#749 が失効すると沈黙で #745 が再発する）。`:636` のコメントを書き換え——**残すべき制約は「入力欄の focus 復帰は blur 状態を読まない」である**（`blur_grace == Focused` で門にすると show 直後に打鍵できなくなる。SU2 が入れた挙動） |
| `src-tauri/CLAUDE.md` | `:36` の「**`unfocus_at` / `was_focused` は現在この backstop の外にいる**・#745」を更新 |
| `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` | `:7`（残る追跡）と **`:100`（「backstop の既知の穴」＝本 issue が閉じる当の命題）** を更新。`:55` の表行は既に stale な歴史記述（`view.rs` の `unfocus_at` を指す）ゆえ触らない |

**`SPEC.md` の変更は不要（バグ側である根拠）**: §8.7 が既に「hide を跨ぐ状態は再表示時のリセットとセットで設計する」と定めており（`SPEC.md:589` 実測）、**現行コードがその違反側にいる**。

**`docs/superpowers/specs/2026-07-27-666-launcher-controller-main-view-design.md`（`:43` のフィールド列挙・`:203` の段 34）は触らない**——`docs/superpowers/README.md` が全体を非規範の歴史資料と宣言している。

**この改名は `governance:check` で検出されない**——`G-stale-identifiers` は camelCase と SCREAMING_SNAKE しか見ない（`ADR-stale-identifier-detector-scope`）。`was_focused` / `unfocus_at` / `set_focused` はすべて snake_case ゆえ、**手 grep が唯一の検出手段**である。実装後に 3 語で全文 grep して残存 0 を確認する。

## 実装順序

**新型の追加と呼び出し点の移行を 1 タスクに束ねる**（`-D warnings` 下で未使用の新 API は `dead_code` で落ちる）。

### フェーズ 1 — 状態機械の追加と移行（1 コミット）

- [x] `lifecycle.rs` に `BlurGrace` を実装し、**同じ編集でテストも書く**。**「テストだけ先に書いて Red を見る」形は取らない**——型が存在しない段階ではコンパイルエラーであって失敗するテストではない（Red の意味を持たない）。テストが落ちることは、実装後に意図的に条件を反転させて 1 度だけ確認する
- [x] `mod.rs` の re-export とコメントを更新する
- [x] `launcher_controller.rs` の 2 フィールドを `blur_grace` へ置換し、`on_focus_changed` を `observe` への委譲へ書き換える
- [x] `clear_blur_grace_if_focused` と `set_focused` を削除する
- [x] `consume_reset_pending` へ `self.blur_grace.reset()` を追加する（**本 issue の本体**）
- [x] `view.rs` の段 14 / 段 34 の呼び出しを削除し、段 16–17 へ `Instant::now()` を渡す
- [x] 偽になる散文（上表の 6 か所）を直す
- [x] `was_focused` / `unfocus_at` / `set_focused` を全文 grep し、残存 0 を確認する
- [x] カテゴリ A を実行する

### フェーズ 2 — 文書同期

- [x] `src-tauri/CLAUDE.md:36` を更新する
- [x] 契約設計 spec の `:7` と `:100` を更新する
- [x] カテゴリ F（`npm run governance:check`）を実行する

### フェーズ 3 — 検証

- [x] **`cargo build -p snotra`（debug）と `cargo build -p snotra --release` の両方**を打つ。`smoke:startup` と `test:powershell` は **debug** を、`smoke:egui` は **release** を見る（`docs/build-commands.md`「実バイナリを起動する検査の前に…古いまま在るバイナリを測る」・#835）
- [x] カテゴリ C（`npm test` / `test:powershell` / `smoke:startup` / `smoke:egui`）
- [ ] 実機確認（下記）を実施し、trace と突き合わせる
- [x] `/symmetric-check` / `/state-check` / `/race-check` と `code-reviewer` を実装差分に当てる
- [ ] **`workspace/` を消す前に ADR を起こす**（`ADR-blur-grace-single-field-state-machine`）——却下した案 A（`consume_reset_pending` に 2 行）と案 B（2 フィールドのまま struct 化）の理由は計画にしか無く、削除で失われる

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| `(was_focused=true, unfocus_at=Some)` の不正状態が作れない | **構造**（enum に variant が無い） |
| 判定と再要求の 2 経路が生まれない | **構造**（`blur_grace_action` をモジュール内へ閉じる。#711 が型で得たものを維持） |
| `observe` は型の中で時計を読まない | 構造＋テストが `now` を注入して固定 |
| `BlurAction` の取り落としが起きない | `#[must_use]`（コンパイル検出） |
| reset 後の初フレームが `focused == false` でも hide しない | 新規ユニットテスト（**本 issue の本体**） |
| `Idle` は武装を解かない（auto_hide を後から有効化して hide できる） | 新規ユニットテスト（テスト 8） |
| **`consume_reset_pending` が `reset()` を呼び続けること** | **検知手段なし**——受容残余。緩和は `view.rs:332-337` の doc 追記のみ。**これは #745 が最初に失敗した当の経路であり、案 C はこれを塞がない**（機械的な検出は「show 直後 100ms 以内に hide 無し」の trace 不変条件が要る。別 issue へ切り出す） |
| 段 3 と段 16–17 の呼び出し順序 | テスト不能（`AppHandle` 依存）。doc が担う受容残余——入口が 4 → 2 に減るぶん縮む |

**「唯一の backstop」「穴なし」と全称で書かない**——`Manager` からの `.show()` 直呼びは今もコンパイルが通り main では効く受容残余であり（`src-tauri/CLAUDE.md`）、#745 はこれを広げも狭めもしない。

## テスト方針

**純粋核（`lifecycle.rs` の `#[cfg(test)]`）**。`Instant` は `t0 + Duration::from_millis(..)` で作る（`Add<Duration> for Instant`。test mod へ `Instant` の import 追加が要る——現在 `Duration` のみ）。**`auto_hide` 引数は明記する**（対照テストは全件 `true` でないと成立しない）。

1. `blur_grace_resets_stale_arm_across_hide` — **シナリオ A**。`observe(true, T0, true)` → `observe(false, T0, true)` で武装 → `reset()` → `observe(false, T0+10s, true)` が **`Idle`**
2. `blur_grace_without_reset_would_hide_on_stale_arm` — 1 の `reset()` を抜くと **`Hide`**（対照）
3. `blur_grace_resets_prior_focus_across_hide` — **シナリオ B**。`observe(true, T0, true)` → `reset()` → `observe(false, T1, true)` が **`Idle`**
4. `blur_grace_without_reset_would_arm_on_stale_prior_focus` — 3 の `reset()` を抜くと `Rearm(BLUR_GRACE)`、続く `observe(false, T1+150ms, true)` が **`Hide`**（対照）
5. `blur_grace_arms_on_focus_loss_edge` — `observe(true, T0, true)` → `observe(false, T0, true)` が `Rearm(BLUR_GRACE)`
6. `blur_grace_drops_pending_when_focus_returns` — 武装後に `observe(true, T, true)` → `Idle`、以後 `observe(false, T+10s, true)` が **`Rearm`**（`Idle` ではない——`Focused` からの新規武装）
7. `blur_grace_hide_returns_to_never_focused` — `Hide` の後に `observe(false, T+1s, true)` が `Idle`（現行の `(false, None)` との対応を固定）
8. `blur_grace_idle_keeps_arm_when_auto_hide_off` — 武装 → `observe(false, T0+150ms, **false**)` が `Idle`、続けて `observe(false, T0+160ms, **true**)` が **`Hide`**（**`Idle` が武装を解かない**ことの固定。auto_hide を後から有効化して hide できる経路が生きている）

**2・4 は対照である**——片方だけだとシナリオ A だけ塞いで B を素通しする実装が緑で通る。

**view 層の状態遷移**: `AppHandle` 依存でテスト不能。実機確認と doc に委ねる（受容残余）。

## 実機確認（挙動変更ゆえ必須）

#746 で確立した配管を再利用する——`SNOTRA_CONFIG_DIR` の使い捨てプロファイル ＋ `SNOTRA_TRACE=1`。**実ユーザーの config は読みも書きもしない。**

1. **回帰なし（auto_hide 有効）**: 通常の blur → 100ms → hide が従来どおり
2. **回帰なし（focus 復帰）**: blur から 100ms 以内に main をクリックし直すと hide されない
3. **本件の修正**: blur で武装し、100ms 未満にホットキーで hide → 再 show。**show 直後に hide されない**
4. **Escape / トレイ hide でも同じ**
5. **`hotkey_toggle = false` の 2 手**: 可視のまま Alt+Q を押すと `ShowNow` → `show_egui_main` が `reset_pending` を無条件に立てるため、**可視中の再 show でも reset が走る**（`window_coordinator.rs:255`）。(a) 可視かつ武装中に Alt+Q → 保留中の自動 hide が取り消されること（**挙動変更**。意味としては正しい——利用者が明示的に窓を呼び戻したため） (b) その後に改めて blur すると通常どおり 100ms で hide されること

**欠陥そのものの再現は実機では不安定である**（`set_focus()` の失敗を強制できない）。**欠陥の固定は純粋核テスト 1〜4 が担い、実機確認は「回帰が無いこと」を担う**——この分担を報告に明記する。

## 未確定（実装前に潰す）

- [x] **武装フレームの `request_repaint_after` 二重発火** — 統合後は問題ごと消える。実測: `blur_grace_action(ms(0), false, true) == Rearm(ms(100))`（`lifecycle.rs:98-101` の既存 assertion・`cargo test -p snotra blur_grace` で 3 passed）
- [x] **同一 `now` で武装しても `Rearm(≈100ms)` が保たれるか** — 保たれる。差は ε の消失のみで**向きは安全側**（機構は `runtime.rs:318` → `repaint.rs:50-52` で確認）
- [x] **段 14 / 段 34 を畳んでよいか** — 畳んでよい。読み書きの全列挙・Escape の非依存・`update()` に `return` 0 件を実測。**加えて「束ねると順序が動く」の doc は `f3c53f1`（機械的分割）単独の注意書きであり、ライブな制約の記録ではない**（`git log -S` 実測）。人間の裁定も得た（2026-08-04）
- [x] **現行の到達可能な状態を取りこぼさないか** — 独立導出 1 体が別経路で導出し、状態表・遷移表とも**完全一致**
- [x] **`auto_hide` が毎フレーム lock になるのは回帰か** — **回帰ではない**。(i) `auto_hide_enabled` の doc は「都度読む（キャッシュしない・#576 と同設計）」であり armed 限定は `if let Some` のネストから落ちた副産物 (ii) engine lock は既に毎フレーム無条件で 2 回（`read_visual` / `lang()`）、2 → 3 になるだけ (iii) **遅延評価案は borrow checker で不成立**——`observe(…, || self.auto_hide_enabled())` はレシーバが `&mut self.blur_grace`、クロージャが `&self` 全体を捕捉するためコンパイルできない
- [x] **`mod.rs:65` の re-export をどうするか** — `blur_grace_action` / `BLUR_GRACE` を lifecycle 内へ閉じる。`unused_imports` の回避であると同時に、**#711 の「消費点の一本化」を型で維持するため**

## multi-perspective review 結果

- リスク: **高**（状態遷移・フレームを跨ぐ共有状態・ガバナンス文書・モジュール間インターフェースの新設）
- 方式: **独立導出 1 体（Step 2b）＋ 枠組みを分けた 4 体**（逆向きの監査 / 状態機械の独立導出 / 時間・フレーム / 実装者の実行可能性）
- エージェント数: **5**
- 成果物: `workspace/plan-review-745-independent.md`・`review-745-reverse-audit.md`・`review-745-state-machine.md`・`review-745-timing.md`・`review-745-executability.md`

### 複数の枠組みが独立に指した所見（強い信号）

- **`mod.rs:62-65`**（逆向きの監査 B1 ＋ 実行可能性 B1）— コメントが偽になり、re-export が未使用になる。**計画の「偽になる散文は全件反映済み」という全称が破れていた**。反映済み
- **契約 spec `:100`**（逆向きの監査 C4 ＋ 実行可能性 #6）— 「backstop の既知の穴」＝本 issue が閉じる当の命題。反映済み
- **`auto_hide` の毎フレーム lock**（主エージェントが争点として提示 ＋ 状態機械 ⚠️7 が独立発見 ＋ 時間レンズが裁定）— 回帰ではないと決着。反映済み
- **可視のまま再 show で reset が走る**（状態機械 ⚠️5 ＋ 逆向きの監査 C9）— 実機確認 5 を追加

### 単独の枠組みが捕まえた所見（他の枠組みの盲点）

- **根本原因の誤り**（逆向きの監査のみ）— SU2 spec が初日から reset を明記し、プランの粗い ✓ が実装漏れを隠した。**計画の中心的な主張を一次資料で否定**。訂正済み。**`git log` / `git blame` という他 3 体が使わない道具を渡したことが効いた**
- **「束ねると順序が動く」は機械的分割時の注意書き**（逆向きの監査のみ）— 合流のリスクが見積もりより低いことが判明
- **`view.rs:332-337` が #749 の理由だけで固定されている**（時間レンズのみ）— 本計画後は #745 の backstop 本体でもある。反映済み
- **`view.rs:636` の書き換えが過小指定**（逆向きの監査 B2 のみ）— 残すべき制約は「入力欄の focus 復帰は blur 状態を読まない」。反映済み
- **debug / release の両ビルドが要る**（実行可能性 B2 のみ）— `smoke:startup` と `test:powershell` は debug を見る。反映済み
- **`G-stale-identifiers` は snake_case を見ない**（実行可能性のみ）— 手 grep が唯一の検出手段。反映済み
- **`Idle` が武装を解かないことを固定するテストが無い**（逆向きの監査 C8 のみ）— テスト 8 を追加
- **TDD の Red がコンパイルエラーになる形**（実行可能性のみ）— フェーズ 1 の手順を訂正
- **`now - t` の panic 経路**（実行可能性 #3）— `saturating_duration_since` へ

### 受容する残余（明記して残す）

- **`consume_reset_pending` が `reset()` を呼び続けることに検知手段が無い**。案 C はこれを塞がない——**#745 が最初に失敗した当の経路である**。緩和は `view.rs:332-337` の doc 追記のみ。機械化には「show 直後 100ms + ε 以内に hide 無し」の trace 不変条件が要り、**セーフティネットの変更ゆえ本 issue の射程を超える**（別 issue へ切り出す）
- `set_focus()` の実失敗頻度は未観測。シナリオ A・B とも実機未観測
- `blur_should_hide` の `!focused` 連言と focus 復帰由来の `Idle` は製品経路で死んでいる（ユニットテストだけが通る）。据え置きゆえ本 PR に影響なし

### 判断

- 実装着手: **人間の承認待ち**

## 人間レビュー

- [x] 承認済み — 2026-08-04 / 問い: "この計画で実装に入ってよろしいでしょうか。" / 回答: "OK"

なお段 14 / 段 34 の合流（文書化された意図的分割を畳む判断）については、同日先行して別途裁定を得ている — 問い: "blur 猶予の状態表現をどうしますか。" / 回答: "1 フィールドの enum へ（推奨）"。
