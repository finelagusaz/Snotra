# #711 独立導出（Step 2b） — blur 猶予の再要求経路

- 日付: 2026-07-26
- 対象 issue: #711「blur 猶予 100ms の再要求経路が無い（1 回きりの予約に依存・潜在）」
- 導出の枠組み: issue 本文 + `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` §2/§3 契約③/§5 + コード実測のみ。`workspace/plan.md` / `workspace/research.md` は未読。
- ベース: `fix/711-blur-grace-rearm`（= main `5b5aa8e`）

---

## 1. 要件の理解（WHAT）

### 1.1 いま何が起きているか（コードで確認した事実）

`src-tauri/src/egui_shell/view.rs` の `SearchWindowView::update` に 3 つのブロックが並ぶ（現在 1296〜1338 行・行番号は指しでなく順序の説明）:

1. `if focused { self.unfocus_at = None; }` — focus 復帰で猶予を破棄（stale 猶予の防止・codex #8）
2. `if was_focused && !focused { self.unfocus_at = Some(now); ctx.request_repaint_after(100ms); }` — **1 回きりの予約**
3. `if let Some(at) = self.unfocus_at { … blur_should_hide(…) なら hide }` — 判定。**不成立でも次フレームを要求しない**

3 の不成立には 2 種類が混在している:

- **時間で解消する不成立**: `grace_elapsed == false`（まだ 100ms 経っていない）。次のフレームさえ来れば成立しうる
- **時間では解消しない不成立**: `focused` / `auto_hide == false` / `settings_running == true`。時計を進めても条件は変わらない

現状は両者を区別せず「何もしない」で終わるため、2 の予約フレームが早着・消失すると、時間で解消するはずの不成立が**次の無関係な入力まで宙吊り**になる。

### 1.2 「挙動不変」の精密化

issue が「潜在の頑健化」と呼ぶ性質を、観測可能な粒度で書き下すとこうなる。**この 4 点が今回の受け入れ条件の実質である**:

| 観点 | 修正前 | 修正後 | 判定 |
|---|---|---|---|
| 予約フレームが 100ms **以降**に来る（= `predicted_dt=0` の現行環境。#628 で接地済み） | 1 枚目で hide | 1 枚目で hide（再要求は発行されない——その時点で `grace_elapsed=true`） | **完全に不変**。フレーム数も 0 枚増えない |
| 予約フレームが 100ms **未満**で早着する（coalescing・`predicted_dt` 復帰・他要求との合流） | hide が宙吊り | 残余で再予約 → 100ms 経過後に hide | **変わる**。これが修正の目的そのもの |
| 予約フレームが落ちる（重い描画・OS スケジューリング） | 同上・宙吊り | 猶予中に来た任意のフレームが予約を張り直す | **変わる**（縮退の仕方が変わる） |
| 猶予中に来た「無関係なフレーム」の扱い | 何もしない | 残余を再要求（coalescing で既存 deadline と min を取るだけ） | 実質不変（要求の重複は worker が畳む） |

つまり **「今日の環境で観測できる挙動は 1 ビットも変わらない。変わるのは前提が破れたときの縮退の仕方だけ」** が正しい主張である。逆に言えば、**今日の環境で観測できる差が出たら、それは実装の誤りである**（後述の罠 1・2 はいずれもこの形で露見する）。

### 1.3 SPEC 同期の要否 — 不要（根拠付き）

`AGENTS.md` ワークフロー 1（「fix」でも文書化された挙動を変えたら仕様変更）に当てて検算した:

- `SPEC.md` が blur について書いているのは §363（`auto_hide_on_focus_lost` の毎フレーム live-read）と §451/§494（`SearchVisible --> Standby: focus_lost [auto_hide_on_focus_lost]`）の 3 箇所のみ（grep 実測）。**100ms 猶予も、予約の張り方も SPEC には無い**
- 状態遷移 `SearchVisible → Standby` の成立条件は変わらない

→ **SPEC.md の更新は不要**。これは仕様変更ではなくバグ修正（潜在の頑健化）である。

---

## 2. 必要な変更集合（ファイル + シンボル）

### 2.1 コード（必須）

| # | ファイル | シンボル | 変更内容 |
|---|---|---|---|
| C1 | `src-tauri/src/egui_shell/lifecycle.rs` | `BLUR_GRACE`（新規 `pub(crate) const Duration`） | 猶予値 100ms の SSOT。純粋核の隣に置く（`LAUNCH_TIMEOUT` / `NOTICE_LAUNCH` が `notify.rs` にあるのと同じ配置規律） |
| C2 | `src-tauri/src/egui_shell/lifecycle.rs` | `blur_grace_remaining(elapsed: Duration) -> Option<Duration>`（新規純関数） | 猶予残余。**未経過のときだけ `Some`**。`grace_elapsed` と残余を**同じ 1 回の観測から**導くための合流点（罠 2 の封じ込め） |
| C3 | `src-tauri/src/egui_shell/lifecycle.rs` | `mod tests` に `blur_grace_remaining` のテストを追加 | 境界（0 / 途中 / ちょうど 100ms / 超過）を固定。既存 `blur_hides_only_when_all_gates_pass` は**無改修** |
| C4 | `src-tauri/src/egui_shell/mod.rs` | `pub(crate) use lifecycle::{…}` の再エクスポート行 | `BLUR_GRACE` / `blur_grace_remaining` を追加（`view.rs` は `crate::egui_shell::` 経由で呼ぶ既存の形に合わせる） |
| C5 | `src-tauri/src/egui_shell/view.rs` | `SearchWindowView::update` の `unfocus_at` 判定ブロック | `else if let Some(remaining) = remaining { ctx.request_repaint_after(remaining); }` を追加。`grace_elapsed` は `remaining.is_none()` から導く |
| C6 | `src-tauri/src/egui_shell/view.rs` | 同・arming エッジ（`was_focused && !focused`） | リテラル `Duration::from_millis(100)` を `BLUR_GRACE` へ置換。**予約自体は残す**（§5.3 参照） |
| C7 | `src-tauri/src/egui_shell/view.rs` | 上記 2 ブロックのコメント | 「1 回きりの予約」を前提にした現行コメントを、契約③（armed の間は毎フレーム再要求）と**再要求しない条件の理由**へ書き換える |

`blur_should_hide` は**無改修**（設計 §5 の判断に独立に同意する。4 ゲートの AND という意味論は変わらない）。

推奨する最終形（罠 1・2 を同時に塞ぐ）:

```rust
// lifecycle.rs
pub(crate) const BLUR_GRACE: Duration = Duration::from_millis(100);

/// blur 猶予の残余。**未経過のときだけ `Some`** を返す（`None` = 猶予明け）。
/// 呼び出し側が `elapsed` を 1 回だけ読み、「経過したか」と「あと何ミリ待つか」を
/// 同じ観測から導けるようにするための形（別々に読むと境界で減算が underflow する）。
pub(crate) fn blur_grace_remaining(elapsed: Duration) -> Option<Duration> {
    BLUR_GRACE.checked_sub(elapsed).filter(|r| !r.is_zero())
}
```

```rust
// view.rs
if let Some(at) = self.unfocus_at {
    let remaining = crate::egui_shell::blur_grace_remaining(at.elapsed());
    if crate::egui_shell::blur_should_hide(
        focused,
        remaining.is_none(), // = grace_elapsed
        self.auto_hide_enabled(),
        self.settings_running(),
    ) {
        self.unfocus_at = None;
        self.emit_hide();
    } else if let Some(remaining) = remaining {
        // 契約③: armed の間は毎フレーム残余を再要求する（予約は「フレーム 1 枚以上」
        // しか約束せず、条件成立は約束しない）。
        // **猶予明け後は再要求しない**——残る不成立要因（focus 復帰・auto_hide=false・
        // 設定起動中）は時間では変わらず、unfocus_at は focus 復帰まで消えないので
        // 無条件の再要求は永久スピンになる。
        ctx.request_repaint_after(remaining);
    }
}
```

### 2.2 文書・コメント（必須）

| # | ファイル | 箇所 | 変更内容 |
|---|---|---|---|
| D1 | `src-tauri/CLAUDE.md` | 「イベント駆動 wake の不変条件（#532 SU5）」 | 契約③を 1 行足す:「`request_repaint_after` は**フレームが 1 枚以上来ること**しか約束しない（早着・合流しうる）。ゆえに条件待ち（armed）の側は、条件が成立するか解除されるまで**毎フレーム残余を再要求する**」。既存の hidden 中/reset-on-show の記述と同じ段落に置く（時限処理の話が 1 か所に集まる） |
| D2 | `snotra-egui-runtime/CLAUDE.md` | 「不変条件」節 | 契約③を runtime 側の言葉で 1 行。**②が既に `repaint.rs` のモジュール行に入っている**ので、体裁を揃える。設計 §8 の 5 番（5 か条の転記）と重複する部分は、5 番でまとめて整えることを許す（この PR では③だけでも整合は壊れない） |
| D3 | `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` | §2 の表・§5・冒頭「**進捗**」行 | blur 行の「脆さ」を解消済みへ更新、§5 に実装差分の errata（§5 のスケッチは `at.elapsed()` を 3 回読む形で、境界で underflow しうる → 1 回読みへ）を追記、進捗行を「残りは 5 番」へ |
| D4 | `src-tauri/src/egui_shell/lifecycle.rs` | `blur_should_hide` の doc コメント | 「猶予タイマの発火・repaint 予約という状態は view 側（update）に残す」は真のまま。残余の算出だけが純粋核へ来たことを 1 行で示す（doc の嘘を作らないための追随） |

`docs/superpowers/plans/*.md`（SU2/SU3 の実装計画）にも `unfocus_at` の旧コードが写っているが、**これらは実施済み計画の歴史記録であり追随しない**（`AGENTS.md`「文書に事実の写しを増やす変更」の裏返し。正本はコード）。

### 2.3 変更しないと決めたもの（根拠付き）

- **`SPEC.md`** — §1.3 のとおり。文書化された挙動は変わらない
- **`blur_should_hide` の signature / 意味論** — 4 ゲートの AND は不変。テストも無改修
- **`results` 窓（`results_view.rs` / `results_window.rs`）** — `focusable(false)` でフォーカスを持ち得ず、blur 判定は main の view にしか無い（#646 PR2 決定 4 で確認済みの前提が今も成立していることを grep で確認: `unfocus_at` の出現は `view.rs` の 6 箇所のみ）
- **`snotra-egui-runtime` のコード** — 契約③は消費側の規範であり、runtime API は変えない
- **`PERFORMANCE.md`** — 数値は変わらない（§1.2 のとおりフレーム数不変）。ただし**検証の錨として使う**（§6）

---

## 3. 同型の間接参照の分類（今回追随が要るもの / 要らないもの）

### 3.1 概念 A: 「1 回きりの予約」（＝この issue の本体）

`request_repaint_after` の全呼び出し点を列挙して分類した（`grep -rn "request_repaint_after" --include=*.rs`・target 除外で 6 件。`request_repaint()`（遅延なし）は deadline を持たないので対象外）:

| 箇所 | 流儀 | 判定 |
|---|---|---|
| `view.rs` `drain_launch`（`LAUNCH_TIMEOUT - elapsed`） | launching 中は毎フレーム残余を再要求 | 契約③に一致・**追随不要** |
| `view.rs` `notice.remaining`（`update` 冒頭） | 表示中は毎フレーム残余を再要求 | 一致・**追随不要** |
| `view.rs` TextEdit `changed` エッジ（`search_debounce.interval()`） | エッジで 1 回だが、下の armed 節が毎フレーム張り直す | ペアで一致・**追随不要**（今回 blur に作る形と同型） |
| `view.rs` `search_debounce.is_armed()` 節 | 毎フレーム残余（コメントで coalescing 対策と明記） | 一致・**手本**（`saturating_sub` の使い方もここが先例） |
| `view.rs` blur 猶予 | **1 回きり** | **今回の対象** |
| `runtime.rs` paint 失敗リトライ（`retry_delay`） | エッジで 1 回だが、paint は**毎フレーム試行される**ので失敗が続けば毎回張り直される（自己回復） | 一致・**追随不要**。ここを「毎フレーム再要求」に変える必要はない |

→ **blur 以外に追随が要る「1 回きりの予約」は無い。** `snotra-settings` 側の `request_repaint()` 3 件はいずれも worker からの即時 wake であり、deadline を持たない。

### 3.2 概念 B: 猶予値の手書き重複

`Duration::from_millis(100)` の全出現（同・grep 実測 5 件）を**語ではなく語義で**分類した:

| 箇所 | 概念 | 判定 |
|---|---|---|
| `view.rs` arming エッジ | blur 猶予 | **同一概念・C1 の const へ寄せる** |
| `view.rs` 猶予明け判定 | blur 猶予 | **同一概念・C1 の const へ寄せる** |
| `config_watcher.rs`（`thread::sleep(100ms)`） | config.toml 変更の debounce | **別概念・触らない** |
| `layout.rs` テスト 2 件（`d.poll(100ms)`） | 検索 debounce のテスト入力 | **別概念・触らない** |

加えて `show_egui_main` の `SendMessageTimeoutW(..., 100, ...)` はフォーカス同期のタイムアウト（ms の生値・型も違う）で、**別概念**。
「同じ表層形が複数の概念を担っていないか」（`AGENTS.md`「検証の作法」）に照らし、統合してよいのは blur の 2 件だけである。

### 3.3 概念 C: doc で blur 猶予の機構に言及している箇所

grep（`blur` / `猶予` / `unfocus` / `100ms`）で拾った全件を、**追随の要否**で分類:

| 箇所 | 性格 | 判定 |
|---|---|---|
| `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」 | 生きた規範（時限処理の作法をここで読む） | **D1・追随必須** |
| `snotra-egui-runtime/CLAUDE.md`「不変条件」 | 生きた規範 | **D2** |
| `docs/.../2026-07-26-frame-scheduling-contract-design.md` §2/§5/進捗 | 生きた設計（本件の親） | **D3** |
| `SPEC.md` §8 系 3 箇所 | 仕様（猶予時間に言及なし） | 追随不要（§1.3） |
| `docs/superpowers/specs/2026-07-22-su2-window-shell-design.md`「blur 自動非表示」 | 実施済み設計の歴史 | 追随不要 |
| `docs/superpowers/plans/2026-07-22-su2-window-shell.md` ほか plans 4 件 | 実施済み計画の歴史（旧コードの写し） | 追随不要 |
| `.claude/skills/{race,state}-check/SKILL.md` の blur 言及 | 検査対象の場所を指すだけ | 追随不要 |

---

## 4. エッジケースの列挙

依頼で名指しされた 6 件 + 独立に見つけた 3 件。各行の「修正後」は §2.1 の実装を前提とする。

| # | ケース | 修正前 | 修正後 | 備考 |
|---|---|---|---|---|
| E1 | **auto_hide off** で blur | 猶予は張られ、100ms 後の 1 枚で hide せず終了。`unfocus_at` は `Some` のまま focus 復帰まで残る | 100ms までは残余を再要求（最大 2〜3 枚）、猶予明け後は**再要求しない**（`remaining == None`） | **最重要**。ここを `else` で一括再要求すると**永久スピン**（罠 1） |
| E2 | **settings 起動中**に blur | 同上（hide せず終了） | 同上（猶予明けで停止） | 設定サイドカーの終了は main を wake しない（`commands/window.rs` の monitor スレッドは `set_always_on_top` を撃つだけで `request_repaint` / `wake_main` を呼ばない・実測）。**ここに再要求を足すと「設定を閉じた瞬間に本体が消える」という新しい挙動を作る**——issue のスコープ外であり、やってはならない |
| E3 | **focus 復帰と同一フレーム** | ブロック 1（`if focused { unfocus_at = None }`）が判定より**前**にあるので、そのフレームで猶予は破棄され判定に入らない | 同じ。再要求も発行されない（`Some` でなくなるため） | 順序不変条件: 「focus クリア → arming → 判定」の並びを崩さないこと。判定を上へ動かすと 1 フレーム遅れて hide が漏れる |
| E4 | **hide を跨ぐ**（猶予 armed のまま Escape / Alt+Q / 起動成功で hide） | `unfocus_at` は `Some` のまま。hidden 中は `update()` が走らないので予約は不発（契約④）。再 show 後、`reset_pending` の消費は `unfocus_at` を**クリアしない**（実測）。show 後の初フレームで `focused == false` なら `grace_elapsed` は当然 true で、**show 直後に hide が飛びうる** | **同じ**（再要求は事態を悪くも良くもしない） | §5.4 で扱う。**契約④に対する既存の非整合**であり、今回の PR に含めるかは判断が要る |
| E5 | **猶予中に config 変更**（auto_hide を切り替え） | `auto_hide_enabled()` は毎フレーム live-read。`config-applied` が `wake_main` を撃つのでフレームは来る | 同じ。そのフレームで残余を再要求するので、猶予の残りは正しく維持される | 修正後の方が素直（wake で来たフレームが予約を張り直す） |
| E6 | **reset-on-show** | `reset_pending` 消費ブロックは `launching` / `notice` / `search_debounce` をクリアするが `unfocus_at` / `was_focused` は触らない | 同じ | E4 と同根。§5.4 |
| E7 | **猶予が 0 に張り付く境界**（`elapsed` がちょうど 100ms） | `>=` で `grace_elapsed = true` | `checked_sub` → `Some(0)` → `filter(!is_zero)` → `None` で **`>=` と同値**。`request_repaint_after(ZERO)`（＝即時再描画の連鎖）は構造上発行不能 | 罠 3 |
| E8 | **blur → 100ms 未満で再 blur**（focus 復帰を挟まない多重 focus-lost） | `was_focused && !focused` はエッジなので再 arm されない（`unfocus_at` は最初の時刻を保持） | 同じ | 意図どおり（猶予は「最初の喪失から」で数える） |
| E9 | **results 窓のクリック**（`WS_EX_NOACTIVATE`） | main の focus は失われないので猶予自体が始まらない | 同じ | #646 PR2 決定 4 の前提が今も成立していることを確認済み |

---

## 5. 落とし穴・注意点

### 5.1 罠 1（致命・挙動を壊す）— 猶予明け後も再要求すると永久スピンになる

契約③を「armed の間は毎フレーム再要求」と読んだとき、`armed` を **`unfocus_at.is_some()`** と取り違えると次を書いてしまう:

```rust
} else {
    ctx.request_repaint_after(BLUR_GRACE.saturating_sub(at.elapsed())); // ← 猶予明けは ZERO
}
```

`unfocus_at` が `None` になるのは **focus 復帰時と hide 発行時だけ**である。auto_hide off（E1）や設定起動中（E2）では、`unfocus_at` は `Some` のまま**無期限に**残る。そこへ無条件の再要求を置くと、残余は常に 0 → `request_repaint_after(ZERO)` の自己永続ループになり、**アプリがバックグラウンドにある間ずっと最大フレームレートで再描画し続ける**。#737 で入れたフレーム上限（モニター Hz）はこれを 144fps に丸めるだけで止めない——**#737 で潰した消費を別の扉から再導入する**ことになる。

→ 正しい `armed` は **`unfocus_at.is_some() && !grace_elapsed`**。§2.1 の `else if let Some(remaining) = remaining` はこれを型で表現している（`None` の枝に再要求を書けない）。
→ この判断は「時間経過で条件が変わるか」で決まる。`focused` / `auto_hide` / `settings_running` はいずれも**時計とは無関係な入力**であり、それらの変化は別の wake 経路（Focused イベント・`config-applied` wake）がフレームを運ぶ。**再要求すべきなのは `grace_elapsed` だけである。**

### 5.2 罠 2（致命・release でプロセス abort）— `elapsed` を 2 回読むと減算が underflow する

設計 §5 のスケッチは `at.elapsed()` を 3 回読む（`grace_elapsed` の判定 / hide 分岐 / `grace - at.elapsed()`）。判定と減算の間に時間が進むため、`grace_elapsed == false` を見た直後に `at.elapsed() > grace` になりうる。`Duration - Duration` は overflow で **panic** し、release は ルート `Cargo.toml` の `panic = "abort"` により **プロセス abort** に化ける（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」の #394 と同じ機序）。

「確率が低いから無視できる」ではない: このフレームは**まさに 100ms 境界に着弾するよう予約されている**ため、`at.elapsed()` は境界の数マイクロ秒以内に落ちる確率が高い。

→ 対処は 2 つで、**両方入れる**: (a) `elapsed` を 1 回だけ読み `blur_grace_remaining` に通す（§2.1）、(b) その中で `checked_sub` を使う。既存の debounce 節が `saturating_sub` を使っているのは同じ理由であり、先例に揃うことでもある。

### 5.3 注意 — arming エッジの予約は**冗長になる**（残すが、そう書き残す）

修正後、arming エッジ（`unfocus_at = Some(now)` + `request_repaint_after(100ms)`）の予約は、直後の判定ブロックが同一フレームで張る `remaining ≈ 100ms` と**同じ deadline を二重に要求する**（間に early return も分岐も無いことをコードで確認済み）。coalescing で 1 本に畳まれるので害は無い。

設計 §5 は「初回予約は残す」と決めており、独立に見てもその判断に同意する（判定ブロックを将来上へ動かしても壊れない、という順序独立性が買える）。ただし**冗長であることを書き残さないと、後の読者がどちらか片方を「重複」として消し、消した方が実は唯一の予約だった、という事故になる**（`AGENTS.md`「重複した読み・冗長に見える状態を束ねる/消す」トリガーが指しているのはまさにこの形）。C7 のコメントで 1 行明記すること。

### 5.4 発見 — reset-on-show が `unfocus_at` をクリアしていない（契約④に対する非整合）

契約④は「hide を跨ぐ時限状態は reset-on-show を backstop にする」と定め、`src-tauri/CLAUDE.md` も同じ規範を持つ。実際 `reset_pending` 消費ブロックは `launching` / `notice` / `search_debounce` / results サイズガードをクリアしている。**しかし `unfocus_at`（と `was_focused`）はクリアしていない**（実測）。SU2 の設計書（`docs/superpowers/specs/2026-07-22-su2-window-shell-design.md`「blur 自動非表示」）は「show のたびに view 側の `was_focused`/`unfocus_at` をリセット」と書いているので、**設計意図に対する実装の取りこぼし**である可能性が高い。

帰結（E4）: 猶予 armed のまま別経路で hide → 再 show したとき、show 後の初フレームで `focused == false` なら猶予は当然明けており、**show 直後に自動 hide が飛ぶ**。`show_egui_main` は `set_focus()` + `SendMessageTimeoutW` で focus を同期待ちするので実際には焦点が来ている公算が大きく、だから顕在化していない——**#711 と全く同じ「1 枚のフレームの内容に賭けている」構造**である。

判断（推奨）:

- **同 PR に含めてよい**が、含めるなら「契約④の適用漏れの是正」として**独立した変更として記述し、コミットも分ける**。`reset_pending` ブロックに `self.unfocus_at = None; self.was_focused = false;` の 2 行 + 理由コメント
- ただしこれは §1.2 の意味で**厳密には挙動不変ではない**（縮退経路の挙動が変わる）。「挙動不変」を PR の看板に掲げるなら、**別 issue に切って本 PR は §2 に留める**のが筋が通る。どちらを採るかは合意事項であり、レビュアーの立場では**決めずに提示する**
- どちらにせよ、**発見したことは #711 の PR 本文か新規 issue に必ず残す**（発見の落とし所を作らないと、契約④の受け入れ条件「既存コードがすべて一致している」が嘘のまま通る）

### 5.5 注意 — この修正は #737 の測定を汚さない（が、逆向きの確認は要る）

§1.2 のとおり通常経路でフレームは増えないので、設計 §8 の「4 番は測定と独立」は正しい。ただし**罠 1 を踏んだ場合の症状は「アイドル時 CPU の上昇」であり、#737 の測定プロトコル（ポインタ移動中 fps）では検出できない**。検出できるのは「blur した状態で放置したときの CPU」だけである。→ §6 の V4 を必ず実行する。

### 5.6 注意 — `ctx` の取り違え

判定ブロックが使う `ctx` は `update` 冒頭で `ui.ctx().clone()` した **main 窓の Context** である。results 窓の Context（`results_view.rs` が別に持つ）へ要求しても main のフレームは来ない。`ctx` を引数で受ける形へリファクタしたくなっても、窓が 2 つあることを忘れないこと（`src-tauri/CLAUDE.md`「外部から窓を起こす経路」）。

---

## 6. テスト方針

### 6.1 ユニット（`cargo test -p snotra`・post-edit hook が自動実行）

| ID | 対象 | 固定する不変条件 |
|---|---|---|
| T1 | `blur_grace_remaining(Duration::ZERO)` | `Some(100ms)`（arming 直後は満額の残余） |
| T2 | `blur_grace_remaining(50ms)` | `Some(50ms)`（残余は単調減少・引き算の向きが逆でないこと） |
| T3 | `blur_grace_remaining(100ms)` | `None`（境界は「明け」側。旧 `>=` と同値であることの固定） |
| T4 | `blur_grace_remaining(10s)` | `None`（**underflow で panic しない**——罠 2 の回帰検出器） |
| T5 | 既存 `blur_hides_only_when_all_gates_pass` | **無改修で通ること**（純粋核の意味論が変わっていないことの証拠） |

**T4 が本命の回帰検出器である。** 実装が `BLUR_GRACE - elapsed` の素の減算へ戻ると、このテストだけが落ちる。

### 6.2 罠 1 を固定するテスト（要判断・推奨する）

上の 5 件は「残余の算出」しか守らず、**「猶予明け後に再要求しない」という今回いちばん壊れやすい判断を 1 つも固定しない**。view 層は `AppHandle` と egui 依存でユニットテスト不能なので、固定したいなら判断を純粋核へ 1 段上げる必要がある:

```rust
pub(crate) enum BlurStep { Hide, Rearm(Duration), Idle }

/// blur 猶予 1 フレーム分の決定。判定自体は `blur_should_hide` に委ねる（意味論は不変）。
pub(crate) fn blur_step(elapsed, focused, auto_hide, settings_running) -> BlurStep
```

- 利点: 「auto_hide=false かつ猶予明け → `Idle`（再要求しない）」「設定起動中かつ猶予明け → `Idle`」「猶予中 → `Rearm(残余)`」がテストで固定でき、**永久スピンの再導入がコンパイル後に必ず落ちる**
- 欠点: 純粋核が 1 つ増える（設計 §5 は「`blur_should_hide` は不変」としか言っておらず、ラッパー追加を禁じてはいない。`blur_should_hide` は内部から呼ぶので既存テストも生きる）
- 判断: **推奨する。** 罠 1 は「実装者が正しく書けたか」ではなく「**将来の編集者が `else` に戻さないか**」の問題であり、規範（コメント）だけで守るのは `docs/development-principles.md`「強制の階梯」で最も弱い層にあたる。ただし設計 §9 決定 2 が「共通 `Deadline` primitive の抽出は行わない」と決めているので、**blur に閉じたラッパー**に留め、他の 3 箇所へ広げないこと
- 採らない場合は、C7 のコメントに「なぜ猶予明けで再要求しないか」を**理由まで**書くこと（`else` の 1 語で消える判断なので、理由が無いと復元できない）

### 6.3 実機スモーク（`docs/build-commands.md` カテゴリ C / D）

blur→hide は `GetAsyncKeyState` / 実フォーカス依存で自動化できない（`.claude/rules/src-tauri.md`「ホットキー・ウィンドウ生成/表示順」トリガー）。以下を人手で:

| ID | 手順 | 期待 |
|---|---|---|
| V1 | 表示 → 別アプリをクリック | 100ms 後に自動非表示（**修正前と体感差が無いこと**が §1.2 の主張の裏取り） |
| V2 | 表示 → 別アプリ → 100ms 未満で戻る | hide しない（E3） |
| V3 | `auto_hide_on_focus_lost = false` → blur | hide しない |
| V4 | **V3 の状態で 60 秒放置し、タスクマネージャで CPU を見る** | **0% 近傍のまま**（罠 1 の実測検出器。`SNOTRA_TRACE` のフレーム系トレースが出るなら件数 0 でも可） |
| V5 | 設定画面を開いた状態で main を blur | hide しない・**設定を閉じた瞬間に main が消えない**（E2 の非退行） |
| V6 | 猶予 armed のまま Alt+Q → 再 Alt+Q | show 直後に消えない（E4。§5.4 を同 PR に含めるなら、含める前後の両方で見る） |

V4 は既存のスモークに無い**新しい観測点**である（`PERFORMANCE.md` のアイドル基準値は「可視・focus あり」の条件で測られており、blur 中の放置を覆っていない）。

### 6.4 検証カテゴリ

- カテゴリ A（clippy + `cargo test -p snotra`）: `*.rs` 編集で PostToolUse hook が自動実行（沈黙 = 合格）
- カテゴリ C（`npm run smoke:egui` / `smoke:startup`）: show/hide 経路に触るため**明示的に実行する**（hook の沈黙は C を含まない）
- カテゴリ F（`npm run governance:check`）: D1〜D3 の文書変更に対して。`docs/` の参照は正準形 `` `<path>.md`「<見出し>」 `` で書く（`.claude/rules/governance-docs.md`）

---

## 7. まとめ（レビュアーとしての結論）

1. 案 A（毎フレーム残余を再要求）は妥当。多数派の流儀（4 箇所中 3 箇所）に揃うだけで、新機構は不要
2. ただし**「armed」の定義を `unfocus_at.is_some()` にすると永久スピンになる**。正しくは `is_some() && !grace_elapsed`。再要求してよいのは**時間経過で解消する不成立**（猶予未経過）だけで、`auto_hide=false` / 設定起動中 / focus 復帰は再要求してはならない
3. 設計 §5 のスケッチは `at.elapsed()` を複数回読んでおり、**境界で `Duration` 減算が underflow → release ではプロセス abort**。`elapsed` の 1 回読み + `checked_sub` を純関数に閉じること
4. 追随が要る同型は**無い**（他の 5 つの `request_repaint_after` はすべて契約③に一致済み）。手書き重複は blur の 100ms リテラル 2 件のみで、他の `from_millis(100)` は別概念
5. **独立に見つけた欠け**: reset-on-show が `unfocus_at` / `was_focused` をクリアしておらず、契約④（hide を跨ぐ時限状態は reset-on-show が backstop）に対する既存の非整合がある。#711 と同型の「1 枚のフレームに賭けている」構造。同 PR に別コミットで入れるか別 issue に切るかは合意事項——**どちらにせよ記録は残すこと**
6. 「挙動不変」は「今日の環境で観測可能な挙動が不変」の意味であり、**今日の環境で差が出たらそれは実装の誤り**（罠 1・2 はいずれもこの形で露見する）。V1・V4 がその 2 面の実測点になる
