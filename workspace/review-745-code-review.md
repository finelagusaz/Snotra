# code-reviewer 所見 — #745 blur 猶予の reset-on-show 合流（`fix/blur-grace-reset-on-show`）

対象: `git diff main` の未コミット分（`workspace/` 配下を除く）。分岐元 `9ebf3db`。

**件数: Critical 0 / High 1 / Medium 3 / Low 4 / ⚠️ 3**

---

## 実測した検証（結果は良好・以下の所見の前提）

すべて本セッションで実行した一次観測。

| 検証 | コマンド / 手段 | 結果 |
|---|---|---|
| カテゴリ A | `cargo fmt --check` / `cargo clippy -p snotra --all-targets -- -D warnings` / `cargo test -p snotra` | green（205 passed / 2 ignored） |
| カテゴリ F | `npm run governance:check` | 全検査 passed（検査 18 件） |
| 残存識別子 | `was_focused` / `unfocus_at` / `set_focused` / `clear_blur_grace` の全文 grep | **コードに 0 件**（`docs/superpowers/` の歴史記述と、`lifecycle.rs:99` / `launcher_controller.rs:105` の意図的な言及のみ） |
| 段 3 < 段 16–17 | `view.rs:323`（`consume_reset_pending`）< `view.rs:436`（`on_focus_changed`） | 成立 |
| `update()` の早期 return | `grep -c "return" src-tauri/src/egui_shell/view.rs` | **0 件**（計画の主張を再実測） |
| `reset_pending` の writer | `grep -rn "reset_pending" src-tauri/src/` | **唯一 `window_coordinator.rs:255`**。`window.show()` より前・同一イベントループスレッドゆえフレームは割り込めない |
| 段 15 の非依存 | `launcher_controller.rs:1042-1065`（`on_escape_pressed`）を全文読了 | blur 状態を読み書きしない → 段 14 を段 15 の後ろへ動かした合流は安全 |
| `#930` の実在 | `gh issue view 930` | OPEN・内容一致（受容残余の受け皿として正しく機能する） |

### 観点 1: 状態遷移の網羅性 — **一致を確認**

旧 `(was_focused, unfocus_at)` のフレーム境界での到達可能状態と `BlurGrace` の全単射、および全アームを独立に再導出し、計画の表と一致した。

- `Hide` を返した直後 → 旧は `unfocus_at=None`（`Hide` アーム）＋ 段 34 で `was_focused=false` → `(false, None)` = `NeverFocused`。**新の `NeverFocused` 復帰は正しい**
- `Idle` が武装を解かない → 旧も `unfocus_at` を触らないまま次フレームへ持ち越す。**一致**
- `focused == true` の早期 return（旧・段 14 ＋ 段 34 の合流）→ 旧は段 14 が `unfocus_at=None`、段 34 が `was_focused=true` = `Focused`。**一致**
- 武装フレームの `Rearm` は旧 `GRACE−ε` → 新 `GRACE`。**予約が早まらない側へのずれ**であり安全側

### 観点 2: `reset()` の位置 — **正しい**（上表の 4 行目・5 行目）

### 観点 3: 削除した 2 メソッドの不変条件 — **`observe` で再確立されている**

段 14 = `observe` 冒頭の `if focused { *self = Focused; return Idle }`、段 34 = 各アームの `*self` への代入。span が段 3〜34 から段 3〜17 へ縮むのは安全側。

### リソース／対称性（Phase 2b・2d）

**4 つの armed 期限が全数 reset-on-show でクリアされる**ようになった（`src-tauri/CLAUDE.md`「期限を待つ状態」が数える 4 つ）: 検索 debounce（`Debouncer::new` で作り直し）／一時通知（`notice.clear()`）／起動タイムアウト（`launching = None`）／blur 猶予（`blur_grace.reset()`・本 PR）。**対称は本 PR で完成した。**

### SPEC 同期（Phase 2e）

- SPEC 同期: blur 猶予が hide を跨いで持ち越されなくなる／SPEC 該当節: §8.7「hide を跨ぐ状態は再表示時のリセットとセットで設計する」（`SPEC.md:589` 付近）／判定: **SPEC 対象外**（現行コードが SPEC の違反側にいたバグ修正。§8.6 の `focus_lost [auto_hide_on_focus_lost]` 遷移は不変）
- ただし ⚠️1 を参照

---

## High

### H1. 対照テスト 2 本の doc が偽である（実測で確定）

- **場所**: `src-tauri/src/egui_shell/lifecycle.rs:188-189`（`blur_grace_without_reset_would_hide_on_stale_arm` の doc）、`src-tauri/src/egui_shell/lifecycle.rs:218-219`（`blur_grace_without_reset_would_arm_on_stale_prior_focus` の doc）
- **根本原因**: 両テストは `reset()` を一度も呼ばず「reset が無い場合の挙動」を assert するため、**`reset()` の呼び出しが消えたときにこそ通る**。「このテストが落とす」という帰属が逆である。

**実測（本セッション）**:

| 変異 | 結果 |
|---|---|
| `launcher_controller.rs:945` の `self.blur_grace.reset();` を削除 | `cargo test -p snotra blur` → **12 passed / 0 failed**（1 本も落ちない） |
| `BlurGrace::reset` を部分実装へ（`Blurred` のときだけ `NeverFocused` にする） | **`blur_grace_resets_prior_focus_across_hide`（テスト 3）が FAILED**。テスト 4 は pass |

正しい帰属は次のとおり:

- テスト 1（`blur_grace_resets_stale_arm_across_hide`）が **no-op な `reset()`** を捕まえる
- テスト 3（`blur_grace_resets_prior_focus_across_hide`）が **片方だけ消す `reset()`** を捕まえる（実測で確認）
- テスト 2 / 4 は 1 / 3 の **vacuity guard**（`observe` 側が壊れてテストが空虚に緑になる改変を捕まえる）——価値はあるが、doc のラベルが誤っている

- **危険な向き**: 同じ diff の 4 か所（`lifecycle.rs:119-121`・`launcher_controller.rs:941-944`・`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」・契約 spec `:7`）は「**この呼び出しの消失を捕まえる検査は無い**」と正しく書いている。テスト doc だけがそれと矛盾し、**読者に「呼び出し点は守られている」と誤信させる**。`AGENTS.md`「実装より強い主張になった瞬間に嘘になり、規範を忠実に守る読者を誤りへ導く」に該当する。
- **修正例**:

```rust
/// A の対照（**vacuity guard**）。テスト 1 が空虚に緑にならないことを固定する
/// ——`observe` が stale な武装から `Hide` を出せなくなる改変をこのテストが落とす。
/// **`consume_reset_pending` からの `reset()` 呼び出しの消失は落とさない**（実測:
/// 呼び出しを削っても 12/12 pass・検知手段が無いことの正本は `BlurGrace::reset` の doc・機械化は #930）。
```

`lifecycle.rs:218-219` も同様に、「片方だけ消す実装を落とす」の主体を**テスト 3**へ書き換える。

- **併記**: `workspace/plan.md:168` / `workspace/plan.md:176` が同じ誤帰属を持つ（レビュー対象外だが、コミット前に直す価値がある）。

---

## Medium

### M1. `observe` の 2 つの `bool` 引数の取り違えに検出手段が無い（観点 4 への回答）

- **場所**: `src-tauri/src/egui_shell/lifecycle.rs:133`（定義）／`src-tauri/src/egui_shell/launcher_controller.rs:1079`（唯一の呼び出し点）
- **根本原因**: `observe(focused: bool, now: Instant, auto_hide: bool)` は第 1・第 3 引数がどちらも `bool` で、`Instant` を挟んでいても入れ替えが型で弾かれない。壊れる不変条件は「`focused` を渡す先が focus であること」で、取り違えると `blur_should_hide` の `!focused` へ `auto_hide` が入り、**自動 hide が恒久的に無効化される**（気づかれにくい方向の故障）。
- **実測（本セッション）**: `observe(auto_hide, Instant::now(), focused)` へ入れ替えた状態で

  - `cargo clippy -p snotra --all-targets -- -D warnings` → **通る**
  - `cargo test -p snotra` → **205 passed / 0 failed**

  加えて smoke / trace 側にも受け皿が無い（`scripts/smoke-egui.ps1` の blur / auto_hide / focus → 0 件、`scripts/lib/SnotraTraceInvariants.psm1` の blur / auto_hide → 0 件、blur 経路に `crate::trace::trace` の呼び出し無し）。**独立に 2 方向から不在を観測したうえで、検出手段は無いと報告する。**
- **修正例**（引数順の入れ替えでは解けない。型を分ける）:

```rust
/// `auto_hide_on_focus_lost` の live-read 値。`focused` との取り違えを型で塞ぐ。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutoHide { Enabled, Disabled }

pub(crate) fn observe(&mut self, focused: bool, now: Instant, auto_hide: AutoHide) -> BlurAction
```

呼び出し点は `if self.auto_hide_enabled() { AutoHide::Enabled } else { AutoHide::Disabled }`。`blur_grace_action` / `blur_should_hide` の内部は `bool` のまま（`matches!` で 1 回変換）でも、外向きの取り違えは塞がる。

### M2. `.claude/settings.json` の `codex@openai-codex` 有効化が #745 の差分に混ざっている

- **場所**: `C:/workspace/Snotra/.claude/settings.json:29`
- **根本原因**: エージェント設定（プラグイン有効化）は #745 と無関係で、ルート `CLAUDE.md`「最重要ルール」2「エージェント設定の変更は合意してから」の対象。セッション開始時点で既に `M` だったため実装者の作り込みではない可能性が高いが、**このまま全体を `git add` すると #745 のコミットに同乗する**。
- **修正例**: コミット前に `git restore .claude/settings.json`、または別途合意のうえ独立コミットへ切り出す。

### M3. `BlurAction::Idle` の doc が `observe` 経由の 2 つの `Idle` を説明していない

- **場所**: `src-tauri/src/egui_shell/lifecycle.rs:38`（「猶予明けだが条件不成立（auto_hide off / focus 復帰）→ 何もしない。」）
- **根本原因**: `observe` が唯一の消費点になったことで `Idle` の意味が広がった。`lifecycle.rs:134-136`（`focused == true` の早期 return）と `lifecycle.rs:140`（`NeverFocused` かつ `!focused`）はどちらも「**猶予明け**」ではなく「猶予がそもそも無い」。現 doc は `blur_grace_action` の返り値としては正しいが、型 doc としては不正確。
- **併記**: `workspace/plan.md:106` はこの doc の修正を予定していたが、diff に含まれていない（`lifecycle.rs` の `BlurAction` 定義部は無変更）。計画項目の取りこぼしか、不要と判断したかを明示すべき。
- **修正例**:

```rust
    /// 何もしない。`blur_grace_action` からは「猶予明けだが条件不成立（auto_hide off）」、
    /// `BlurGrace::observe` からはそれに加えて「focus を持っている」「一度も focus を
    /// 得ていない（＝猶予が無い）」でも返る。**いずれも再要求してはならない**
    /// （時間経過で解消しないため・永久スピンになる）。
    Idle,
```

---

## Low

### L1. `blur_grace_action` の `focused` 引数が製品経路で死んだ

- **場所**: `src-tauri/src/egui_shell/lifecycle.rs:146` / `src-tauri/src/egui_shell/lifecycle.rs:151`（`observe` からの 2 つの呼び出しがどちらもリテラル `false`）、および `lifecycle.rs:88-90` の `blur_should_hide` の `!focused` 連言
- **根本原因**: `observe` が `focused == true` を早期 return で処理するようになり、`blur_grace_action` は常に `focused: false` で呼ばれる。連言と引数はユニットテスト（`lifecycle.rs:327` / `lifecycle.rs:364`）だけが生かしている。
- **判断**: `workspace/plan.md:231` が受容残余として明記済み。**撤去を提案しない**（`blur_should_hide` は SPEC §8.6 のゲートと一対一に対応する正本であり、連言を削ると SPEC との対応が読めなくなる）。記録として残す。

### L2. 契約 spec の `:55` と 666 spec が、コードに存在しない識別子を名指ししたまま

- **場所**: `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md:55`（「blur 猶予（`view.rs` の `unfocus_at` → `lifecycle::blur_grace_action`）」）、同 §5 のコード塊（`:148-176`）、`docs/superpowers/specs/2026-07-27-666-launcher-controller-main-view-design.md:43`（フィールド列挙）・`:203`（段 34 = `set_focused`）
- **根本原因**: 同じ diff で `:7` と `:100` は更新されたのに `:55` は据え置かれ、`unfocus_at` は**コードから完全に消えた**（grep 実測 0 件）。文書の冒頭が「本書はこれ以降、導出の歴史記録である」と宣言しているため誤りとまでは言えないが、`:100` が採った「（当時）」の目印が `:55` に無いのは非対称。
- **修正例**: `:55` の行頭へ `:100` と同じ「（当時）」を付し、§5 のコード塊冒頭の「出荷形（#711 **当時**…）」の但し書きへ「#745 で `BlurGrace::observe` へ畳んだ」を 1 行足す。**修正は任意**——当該文書は冒頭で全体を「導出の歴史記録である」と自称しており、`:100` との非対称は化粧である。

### L3. `lifecycle.rs` の `std::time::Duration` 完全修飾と `use` の混在

- **場所**: `src-tauri/src/egui_shell/lifecycle.rs:3`（`use std::time::{Duration, Instant};` を新設）に対し、`lifecycle.rs:29`（`BLUR_GRACE`）・`lifecycle.rs:37`（`BlurAction::Rearm`）・`lifecycle.rs:67`（`blur_grace_action` のシグネチャ）が `std::time::Duration` のまま
- **修正例**: 3 か所を `Duration` へ揃える。

### L4. `workspace/plan.md:145` の実機確認が未完了

- **場所**: `C:/workspace/Snotra/workspace/plan.md:145`（未チェックの「実機確認（下記）を実施し、trace と突き合わせる」）
- **根本原因**: 計画自身が「**実機確認（挙動変更ゆえ必須）**」（`plan.md:180`）と定めており、とくに項目 5（`hotkey_toggle=false` の可視中再 show）は本 PR で新たに生じた挙動である。ユニットテストは欠陥の固定を担い、回帰の不在は実機確認しか担えない（計画 `plan.md:190` の分担）。
- **判断**: 完了ゲートとして残す（`gh pr create` は未チェックのチェックボックスを hook が拒むため機構でも捕捉される）。

---

## ⚠️ 確信の持てない所見

### ⚠️1. 「可視中の再 show が保留中の自動 hide を取り消す」は SPEC 記述の射程外か

`hotkey_toggle=false` で可視かつ武装中に Alt+Q を押すと `show_egui_main` が `reset_pending` を無条件に立てる（`src-tauri/src/egui_shell/window_coordinator.rs:255`）ため、**保留中の自動 hide が取り消される**。SPEC §8.6 の状態機械は 100ms 猶予を状態としてモデル化していないので「SPEC 対象外」と判定したが、SPEC §8.7 の表「非表示 … フォーカス喪失（設定で切替・100ms 猶予）」をどこまで規範と読むかで判断が変わりうる。意味としては新挙動の方が正しい（利用者が明示的に窓を呼び戻した）ので、**SPEC 追記は不要と考えるが、人間の確認を推奨する**。

### ⚠️2. `auto_hide_enabled()` の毎フレーム engine lock

旧実装は `unfocus_at` が `Some` のときだけ読んでいたが、新実装は毎フレーム無条件に読む（`src-tauri/src/egui_shell/launcher_controller.rs:1078` → `launcher_controller.rs:633-645` で `AppState.engine` の `Mutex` を取る）。`workspace/plan.md:198` が「回帰ではない」と裁定済み（既に `read_visual` / `lang()` で 2 回取っており 2→3・イベント駆動ゆえフレームレート上限あり）。**本レビューでは実測していない**ため、裁定を追認する形で ⚠️ として残す。Phase 3 のホットパス（`search.rs` / `folder.rs` / 結果レンダリング / アイコンキャッシュ）には触れていない。

### ⚠️3. borrow まわりのコメントが 2 か所にあり、隣り合わせに読むと同じ話に見える

`src-tauri/src/egui_shell/launcher_controller.rs:1076-1078`（「`self.auto_hide_enabled()` を直接渡すと **two-phase borrow に依存する形**になり意図が読み取りにくい」）と `src-tauri/src/egui_shell/lifecycle.rs:128-131`（「**クロージャ渡しは borrow checker を通らない**」）は**別の形についての別の主張**でありどちらも正しい（前者はコンパイルは通る／後者は通らない）。ただし隣接して読むと矛盾に見える恐れがあり、前者に「（コンパイル自体は通る）」の一句を足すと誤読が消える。M1 の enum 化を採るなら前者は書き換え対象になる。

---

## 過去パターン再発の疑い

**該当なし。** `src-tauri/CLAUDE.md` が記録する既知パターン（tao と raw 操作の混在／`ctx.set_visuals`／`MonitorFromWindow`／presence 検査の誤用／`request_repaint_after` の 1 回きり予約）のいずれにも触れていない。契約③（armed の間は毎フレーム残余を再要求）は `BlurAction::Rearm` 経由で維持されており（`launcher_controller.rs:1086`）、契約④（reset-on-show の backstop）は本 PR がまさに満たしにいっている。
