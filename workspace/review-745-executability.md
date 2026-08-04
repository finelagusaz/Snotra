# レビュー: #745 計画の実行可能性（実装者の枠組み）

対象: `workspace/plan.md`（#745 案 C・1 フィールドの状態機械）
枠組み: **計画の是非ではなく「追加判断なしにそのまま実行できるか」**
実施: 2026-08-04・ブランチ `fix/blur-grace-reset-on-show`（コード未変更）

---

## 結論

**そのままでは実行できない。** `-D warnings` で確実に落ちる箇所が 1 件（B1）、SSOT と食い違う検証コマンドが 1 件（B2）あり、加えて実装者が自分で決めねばならない箇所が 8 件ある。名指しされた **ファイル・シンボル・行番号はすべて実在し、ずれは無い**（唯一 `lifecycle.rs:96-98` の範囲がわずかに短い）。

---

## A. 行番号・シンボルの実在確認（全件 grep 実測）

| 計画の記述 | 実測 | 判定 |
|---|---|---|
| `launcher_controller.rs:104-105`（`was_focused` / `unfocus_at` 宣言） | :104 `was_focused: bool` / :105 `unfocus_at: Option<Instant>` | ✅ |
| `:139-140`（初期化） | :139 `was_focused: false` / :140 `unfocus_at: None` | ✅ |
| `consume_reset_pending`（`:917`） | :917 `pub(super) fn consume_reset_pending(&mut self) -> bool` | ✅ |
| `clear_blur_grace_if_focused`（`:1035`） | :1035 | ✅ |
| その doc `:1033-1034` | :1033-1034（「束ねると順序が動く」） | ✅ |
| `on_focus_changed`（`:1075`） | :1075 | ✅ |
| その doc `:1073-1074` | :1073-1074（「`was_focused` フィールドが持ち、更新は段 34」） | ✅ |
| `set_focused`（`:1304`） | :1304 | ✅ |
| その doc `:1302-1303` | :1302-1303（「唯一の書き手」） | ✅ |
| `emit_hide` の `hide_pending.swap(true)`（`:199-208`） | `fn emit_hide` :199、`swap(true, SeqCst)` :204 | ✅ |
| `lifecycle.rs:57-59`（#745 への言及） | :57-59 | ✅ |
| `lifecycle.rs:96-98` の既存 assertion | assertion 本体は **:98-101**（:96 は `let ms = ...`） | ⚠️ 軽微なずれ |
| `mod.rs:62`（`blur_should_hide` 非公開方針） | :62-64 のコメント、:65 が re-export 本体 | ✅ |
| `mod.rs:65`（re-export 行） | :65 `pub(crate) use lifecycle::{BLUR_GRACE, BlurAction, HotkeyPlan, blur_grace_action, plan_hotkey};` | ✅ |
| `view.rs:421`（段 14） | :421 `self.controller.clear_blur_grace_if_focused(pre.focused);` | ✅ |
| `view.rs:427`（段 16–17） | :427 `self.controller.on_focus_changed(pre.focused, &ctx);` | ✅ |
| `view.rs:636`（`was_focused` のコメント） | :636 | ✅ |
| `view.rs:997`（段 34） | :997 `self.controller.set_focused(pre.focused);` | ✅ |
| `view.rs:197`（「旧・段 14〜20 相当の位置」） | :197 | ✅ |
| `src-tauri/CLAUDE.md:36` | :36（`unfocus_at` / `was_focused` は backstop の外） | ✅ |
| 契約設計 spec `:7` | `2026-07-26-frame-scheduling-contract-design.md:7` | ✅ |
| `SPEC.md:589` | :589「**非表示中はフレームが走らない**…hide を跨ぐ状態は再表示時のリセットとセットで設計する」 | ✅ |
| `window_coordinator.rs:341`（`let _ = set_focus()`） | :341 | ✅ |

**実在しないシンボルは 1 件も無い。行番号のずれも 1 件（上記 ⚠️）だけである。**

---

## B. ブロッカー（書いてあるとおりにすると落ちる）

### B1. `mod.rs:65` の re-export が 2 件宙に浮き、`-D warnings` で落ちる 🔴 高確信（実測済み）

計画は `mod.rs` について「`BlurGrace` を re-export（`:65`）」としか書いていないが、移行後に **`blur_grace_action` と `BLUR_GRACE` の 2 つは crate 内から誰も使わなくなる**:

- `blur_grace_action` の crate 内消費点は `launcher_controller.rs:1086` の 1 つだけ（grep 実測）。移行後は `BlurGrace::observe` が `lifecycle.rs` **内部で**呼ぶので、`mod.rs` の re-export 経由の参照は消える
- `BLUR_GRACE` の crate 内消費点は `launcher_controller.rs:1080`（`ctx.request_repaint_after(BLUR_GRACE)`）の 1 つだけ。計画はこの行を消す（`observe` が `Rearm` を返す形になるため）。テスト 4・5 が使う `BLUR_GRACE` は `lifecycle.rs` の `#[cfg(test)]` から `super::` で引くので re-export を必要としない

未使用の `pub(crate) use` は `unused_imports` を出す。最小再現で実測済み:

```
warning: unused imports: `K` and `f`
 --> t.rs:6:24
  |
6 | pub(crate) use inner::{K, f};
```

`cargo clippy --workspace --all-targets -- -D warnings`（`ci.yml:109` / `docs/build-commands.md:16`）で **error になる**。PostToolUse hook も同じコマンドを撃つので、実装者は `mod.rs` 編集の直後に赤を受け取る。

**必要な追加判断**: `mod.rs:65` から `blur_grace_action` と `BLUR_GRACE` を**外す**（`lifecycle.rs` 内では `pub(crate) fn` / `pub(crate) const` のまま生きるので `dead_code` にはならない）。計画の「据え置き」を「re-export も据え置き」と読むと落ちる。

### B2. 実機・スモーク前のビルドが `--release` だけでは足りない 🔴 中〜高確信

計画フェーズ 3: 「カテゴリ C（… **実バイナリ検査の前に `cargo build -p snotra --release`**）」

実測したプロファイルの内訳:

| 検査 | 参照する exe |
|---|---|
| `npm run smoke:egui` | `scripts/smoke-egui.ps1:2` → `target/release/snotra.exe` |
| `npm run smoke:startup` | `scripts/smoke-startup.ps1:9` → `target\debug\snotra.exe` |
| `npm run test:powershell` | `SnotraSmoke.psm1:359` `[string]$Profile = 'debug'` → `<target_directory>/debug/snotra.exe` |

**`--release` だけを打つと、`smoke:startup` と Pester 統合テストは古い debug バイナリを測る。** これは `docs/build-commands.md:47` が名指しで警告している事故（「危ないのは『未ビルド』より**古いまま在る**ほう…#835 で 2 時間前のバイナリを測り…」）そのものである。SSOT（`:46-47`）の記述は `cargo build -p snotra`（debug）で、計画の `--release` は SSOT と一致しない。

**必要な追加判断**: `cargo build -p snotra` と `cargo build -p snotra --release` の**両方**を打つ（またはどの検査にどちらが要るかを計画へ書く）。

---

## C. 実装者が追加判断を迫られる箇所（主成果）

### C1. `blur_grace` の初期値が書かれていない 🟡 高確信

計画は「`:139-140` を `blur_grace: BlurGrace` へ置換」としか書かず、**初期値を名指ししていない**。

- 正解はほぼ確実に `BlurGrace::NeverFocused`（現行 `(false, None)` と全単射表の 1 行目が対応し、起動直後＝まだ一度も show していない状態として正しい）
- 起動直後の正しさ: `show_on_startup=true` の経路でも `show_egui_main`（`window_coordinator.rs:255`）が `reset_pending` を立て、段 3 が `reset()` を通す。`show_on_startup=false` なら初フレーム自体が来ない。どちらでも `NeverFocused` で安全
- ただし「`Default` を derive して `#[default] NeverFocused` を付けるか、リテラルで書くか」は実装者の判断。計画は指示していない

### C2. `on_focus_changed` の新シグネチャが書かれていない 🟡 高確信

計画の `observe` は `(&mut self, focused: bool, now: Instant, auto_hide: bool)`。呼び出し側 `on_focus_changed(&mut self, focused: bool, ctx: &egui::Context)` は `now` を受け取れない。`view.rs:427` へ `Instant::now()` を渡すと書いてあるので `on_focus_changed(focused, now, &ctx)` になるはずだが、**引数順・名前は計画に無い**。

### C3. `Blurred(t)` 分岐の elapsed 計算方法が書かれていない 🟠 中確信（安全性に効く）

計画は `now` の 1 回読み契約を強く書いているが、`observe` の中で `t` から経過をどう出すかを書いていない。3 択:

- `now - t` — `Sub<Instant> for Instant` は `now < t` で **panic**（release は `panic = "abort"`）
- `now.duration_since(t)` — Rust 1.60 以降は飽和（panic しない）
- `now.saturating_duration_since(t)` — 明示的に飽和

`Instant` は単調なので実運用では `now >= t` だが、計画が `blur_grace_action` の doc（`lifecycle.rs:41-46`）を引いて「underflow を構造で消す」と主張している以上、**どれを書くかは実装者が決めることになっている**。推奨は `saturating_duration_since`（意図が読める）。

### C4. `self.blur_grace.observe(..., self.auto_hide_enabled())` の借用 🟠 中確信

`auto_hide_enabled`（`launcher_controller.rs:633`）は `&self` を取る。`self.blur_grace.observe(focused, now, self.auto_hide_enabled())` は two-phase borrow で通る**はず**だが（`vec.push(vec.len())` と同型）、レシーバがフィールド・引数が `self` 全体という組み合わせなので確信度は中。

**推奨**: `let auto_hide = self.auto_hide_enabled();` を 1 行前に置く。計画はこの形を指定していないので、実装者が E0502 に当たってから直す形になる。

### C5. `mod.rs:62-64` のコメントが偽になる（計画の「偽になる散文」表に無い） 🟡 高確信

```
62: // `blur_should_hide` は re-export しない——消費点は `blur_grace_action` に一本化され、
63: // 判定そのものは純粋核の内部で生きている（#711）。2 経路を並走させないための意図的な非公開。
64: // blur 猶予の 3 件は launcher_controller.rs が、plan_hotkey は main.rs 側が消費する。
```

- `:62`「消費点は `blur_grace_action` に一本化」→ 移行後の消費点は `BlurGrace::observe` であり、`blur_grace_action` はその内部呼び出しになる。**表現を直す必要がある**
- `:64`「blur 猶予の **3 件**は launcher_controller.rs が…消費する」→ B1 のとおり 3 件（`BLUR_GRACE` / `BlurAction` / `blur_grace_action`）が **`BlurAction` と `BlurGrace` の 2 件**へ変わる。**件数も名前も偽になる**

計画の「この変更で偽になる散文（独立導出が捕捉・**全件を上表に反映済み**）」という全称主張は、この 1 件で破れている。

### C6. 契約設計 spec の更新箇所が `:7` だけではない 🟡 高確信

`docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`（計画は `:7` のみ指定）:

| 行 | 内容 | 移行後 |
|---|---|---|
| `:7` | 「残る追跡: #745（`unfocus_at` が契約④の backstop の外にいる）」 | 計画が扱う ✅ |
| `:55` | 表「blur 猶予（`view.rs` の `unfocus_at` → `lifecycle::blur_grace_action`）」 | **シンボル・場所とも偽**（`unfocus_at` は消え、駆動は `launcher_controller` の `BlurGrace`） |
| `:100` | 「**backstop の既知の穴**: `unfocus_at` / `was_focused`（blur 猶予）は reset-on-show でクリアされておらず、この条項の対象でありながら backstop の外にいる（#745）」 | **命題ごと偽になる**（本 issue が閉じる当の穴） |
| `:148` `:165-176` | 「`view.rs` の `unfocus_at` 節を…」＋当時のコードスケッチ | 実装案の記録なので歴史として残してよいが、`:148` の「`view.rs` の」は #666 で既に `launcher_controller.rs` へ移っており二重に古い |

**`:100` は少なくとも必須である。** 計画の対応表がここを落としている。

### C7. `2026-07-27-666-launcher-controller-main-view-design.md` の 2 か所 ⚠️ 中確信（射程判断が要る）

- `:43`「フィールド **15**: `app_handle` / **`was_focused`** / **`unfocus_at`** / …」→ 名前も件数も偽になる
- `:203`「| 34 | `controller.set_focused()` |」→ 段 34 ごと消える

計画はこの文書に一切触れていない。**歴史文書として凍結する（＝直さない）判断もありうる**が、計画は `2026-07-26` の spec は直すと決めているので、実装者は「なぜ片方だけ直すのか」を自分で決めることになる。`docs/adr/ADR-...` は凍結された歴史だが `docs/superpowers/specs/` はそうではない（計画自身が spec を直す前提で書かれている）。

**注意**: G-stale-identifiers は camelCase と SCREAMING_SNAKE しか見ない（`ADR-stale-identifier-detector-scope.md:107,185`）。`was_focused` / `unfocus_at` / `set_focused` / `clear_blur_grace_if_focused` はすべて snake_case なので、**`npm run governance:check` はこれらの腐りを 1 件も捕まえない**。手作業の grep が唯一の検出手段である。

### C8. フェーズ 1 と 2 の境界が重なっている 🟡 高確信

- フェーズ 1 のチェックリスト: 「偽になる散文（上表）を直す」——上表には `src-tauri/CLAUDE.md:36` と 契約設計 spec `:7` の行が**含まれている**
- フェーズ 2: 「`src-tauri/CLAUDE.md` の #745 の記述を更新する」「契約設計 spec `:7` の『残る追跡』を更新する」

**同じ 2 か所が両フェーズに現れる。** さらにフェーズ 1 の検証は「カテゴリ A を実行する」だけだが、フェーズ 1 で `CLAUDE.md` を触るならカテゴリ F（`npm run governance:check`）も該当する（`docs/build-commands.md:127`）。実装者はどちらのコミットへ入れるかを自分で決めねばならない。

### C9. テストの `auto_hide` 引数が書かれていない 🟢 低リスクだが判断は要る

テスト 1〜7 の記述はすべて `observe(true, T0)` の 2 引数形で、`observe` の第 3 引数 `auto_hide` が省略されている。全テストで `true` を渡すのが自然だが、テスト 2・4 の対照性（`Hide` / `Rearm→Hide` を出す）は `auto_hide == true` に依存するので、実装者が黙って `false` を選ぶと対照が崩れる。

---

## D. 検証できた項目（追加判断は不要）

### D1. `BlurGrace` はそのまま書ける ✅

- **`Instant` は `Copy` + `Eq` + `PartialEq` + `Debug`** を実装するので、計画の `#[derive(Clone, Copy, Debug, Eq, PartialEq)]` はそのまま通る
- `match (*self, focused)` は `Self: Copy` を要求し、それは上の derive が満たす
- **`blur_should_hide` の可視性方針とは衝突しない**——`observe` は `lifecycle.rs` の中にあるので `blur_grace_action` を直接呼べる。`mod.rs:62-63` の「外へ出さない」方針は無傷（ただし C5 の文言修正は要る）
- `BlurAction` は `observe` の戻り値として `launcher_controller.rs` が引き続き match するので、re-export は生きたまま

### D2. テストで `Instant` を注入できる ✅

`Instant` は任意の時刻を構築できないが、`let t0 = Instant::now(); let t1 = t0 + Duration::from_secs(10);` で相対時刻を作れる（`Add<Duration> for Instant`）。テスト 1〜7 の要求（`T0` / `T0+10s` / `T1+150ms`）はすべてこの形で書ける。
※ `lifecycle.rs` の `#[cfg(test)] mod tests` は現在 `use std::time::Duration;` のみ（`:92`）なので、`Instant` の import 追加が要る。

テスト 7（`Hide` 後の状態が `NeverFocused`）は `PartialEq` + `Debug` derive があるので `assert_eq!` で書ける。

### D3. フェーズ分割は `-D warnings` の中間状態を作らない ✅

フェーズ 1 は「新型の追加 + 呼び出し点の移行 + 旧メソッド削除」を **1 コミット**に束ねており、`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」に適合する。フェーズ 2（文書）・3（検証）はコードを触らない。

⚠️ ただしフェーズ 1 内部の TDD 順序（「テストを**先に**書き、落ちることを確認する（Red）」）は、`BlurGrace` 不在のためテストがコンパイル**不能**になる——`lifecycle.rs` の既存テストごと build error になり、PostToolUse hook が赤を返す。「Red = テスト失敗」ではなく「Red = コンパイル失敗」である点を実装者は飲み込む必要がある（進行は妨げないが、hook の赤を見て手戻りする可能性がある）。

### D4. 段 14 / 段 34 を畳んでも他の読み手はいない ✅（再検証済み）

`was_focused` / `unfocus_at` を読み書きするのは `launcher_controller.rs` の 4 か所（:104-105 宣言・:139-140 初期化・:1035-1039・:1076-1105・:1305）と `view.rs:636` のコメントのみ（全リポジトリ grep 実測）。計画の主張どおり、段 14〜34 の間に外部の読み手はいない。

### D5. 挙動の等価性（arm 側） ✅

`auto_hide == false` のときの武装フレーム: 現行は `unfocus_at = Some(now)` + `request_repaint_after(BLUR_GRACE)` を撃ち、続けて `blur_grace_action(≈0, false, false)` → `Rearm(≈100ms)` でもう 1 回撃つ。新案は `blur_grace_action(ZERO, false, false)` → `Rearm(100ms)` で 1 回。**回数が 2→1 に減るだけで向きは同じ**（計画の未確定 1 の主張は正しい）。

### D6. 削除するメソッドの呼び出し元は網羅されている ✅

`clear_blur_grace_if_focused` → `view.rs:421` のみ。`set_focused` → `view.rs:997` のみ。どちらも計画が名指ししている。`pre.focused` は削除後も `view.rs:427` と `:638` で使われるため、`PreWidgetInput.focused` が未使用になることはない。

### D7. 検証カテゴリの選択は妥当 ✅

計画「A / C / F を実行。B・E は該当なし」は `docs/build-commands.md` と一致する:
- A（`*.rs` 変更）✅
- C（「ウィンドウ生成／表示順」＝ hide の発火条件に触る）✅ 意味で該当という計画の説明も妥当
- F（`*.md` 変更）✅
- B（TS 無し）✅ / E（`.githooks/` 無し）✅
- D（UI スタイル・レイアウト・テキスト表示）は非該当で妥当。ただし計画の「実機確認」は `smoke:manual` を使わない手組み手順なので、D 相当を自前でやっている形になる（意図的と読める）

---

## E. ⚠️ 確信の持てない所見

| # | 所見 | 確信度 |
|---|---|---|
| E1 | C4 の two-phase borrow。`self.blur_grace.observe(f, now, self.auto_hide_enabled())` は `vec.push(vec.len())` と同型で通ると考えるが、レシーバがフィールドでの実例を確認していない。**手元でコンパイルしていない** | 中（通る側 70%） |
| E2 | C7 の `2026-07-27-666-...design.md` を直すべきかどうか。`docs/superpowers/specs/` の凍結ポリシーが明文化されていない（ADR は凍結と明記があるが specs は無い）。計画が `2026-07-26` spec を直すと決めていることから「specs は生きている」と読んだが、逆の読みもありうる | 中 |
| E3 | B2 の重大度。`smoke:startup` / Pester がこの変更の**挙動差を実際に測る**かは未確認（測らないなら古い debug バイナリでも偽の緑は出ない）。ただし `docs/build-commands.md:47` は「古いまま在る」を名指しで戒めており、SSOT との不一致であること自体は動かない | 中〜高 |
| E4 | ~~`BlurGrace::reset()` に clippy の追加 lint が当たるか~~ **解消**: `Cargo.toml:21-23` の `[workspace.lints]` は `rustdoc` の 2 件のみ（`broken_intra_doc_links` / `invalid_html_tags`）で clippy の追加は無い。既定セットに `missing_const_for_fn` は入らない | 高（解消） |
| E5 | `docs/superpowers/plans/2026-07-22-su2-window-shell.md:180-181,560-580` と `2026-07-22-su3-m1-core-search.md:598-614` も `was_focused` / `unfocus_at` を含む。**plans は実行済み計画の記録＝歴史**と読んで対象外としたが、計画は plans に一切触れていないので明示の裁定が無い | 中（対象外で正しい側 75%） |
| E6 | テスト 6 の期待（武装後に `observe(true, T)` → `Idle`、続く `observe(false, T+10s)` が `Rearm`）は計画の遷移表と整合するが、これは**現行との挙動差**である——現行も段 14 が `unfocus_at` を消すので `Focused` 相当へ戻り同じ。差は無いと読んだが、`(true, Some(t))` 到達不能の主張に寄りかかっている | 中〜高 |
| E7 | `observe` の `now` 引数が `NeverFocused` / `Focused`+`focused==true` のアームで使われないことによる `unused_variables` 警告は出ない（他アームで使うため）と考えるが未確認 | 高（出ない側 90%） |

---

## F. 実装者への最小の追加指示（これがあれば実行できる）

1. `mod.rs:65` の re-export から **`blur_grace_action` と `BLUR_GRACE` を外し**、`BlurGrace` を足す。`:62-64` のコメントを「消費点は `BlurGrace::observe`」「blur 猶予の 2 件」へ書き換える（B1・C5）
2. `blur_grace: BlurGrace::NeverFocused` を初期値とする（C1）
3. `on_focus_changed(&mut self, focused: bool, now: Instant, ctx: &egui::Context)` とし、`auto_hide` は直前の `let` へ束縛する（C2・C4）
4. `Blurred(t)` の経過は `now.saturating_duration_since(t)` で出す（C3）
5. 契約設計 spec は `:7` に加え **`:100` と `:55`** も直す（C6）
6. `2026-07-27-666-...design.md:43` / `:203` を直すか凍結扱いにするかを先に決める（C7）
7. フェーズ 1 で文書を触るなら、フェーズ 1 の検証に **カテゴリ F を足す**（C8）
8. フェーズ 3 の実バイナリ検査前は **`cargo build -p snotra` と `cargo build -p snotra --release` の両方**（B2）
9. テストの `auto_hide` は全件 `true`（C9）
