# #745 計画レビュー — 時間・フレーム・コストのレンズ

対象: `workspace/plan.md`（案 C: 1 フィールドの状態機械）
正本: `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」/ 同「モジュール構成」/ `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` 契約①〜⑤
枠組み: 時間・フレーム・コスト（このレンズ以外の観点＝表現の妥当性・テスト設計・散文同期は本書の射程外）

**結論: 計画を止める所見は無い。要対処 1 件（`view.rs:332-337` のコメント）。**

---

## 1. 争点 — `auto_hide` を引数で受けると毎フレーム lock を取るのではないか

### 判断: 回帰ではない。計画どおり `auto_hide: bool` を引数で受けてよい（軽微・要記述）

**(a) 事実確認 — 現行は確かに armed のときだけ lock を取る。**
`launcher_controller.rs:1085-1091` の `blur_grace_action(...)` 呼び出しは `if let Some(at) = self.unfocus_at` の内側にあり、`self.auto_hide_enabled()`（`:633-645` — `try_state::<AppState>` + `engine.lock()`）はその中でだけ評価される。指摘は事実として正しい。

**(b) しかしその armed-only 性は「意図」ではなく `if let` のネストから落ちた副産物である。**
`on_focus_changed`（`:1073-1101`）にも `auto_hide_enabled`（`:632-645`）にも、lock 頻度についての意図を述べたコメントは 1 行も無い。むしろ `auto_hide_enabled` の doc は「**auto_hide_on_focus_lost を実行中 config から都度読む（キャッシュしない・#576 と同設計）**」（`:632`）であり、**文書化された意図は「毎回読む」側である**。毎フレーム読む形は #576 の設計意図に対して後退ではなく、より素直な適合である。

**(c) コストは無視できる。同種の読みが既に無条件で毎フレーム走っている。**

| 毎フレーム無条件に engine lock を取る箇所 | 位置 |
|---|---|
| `read_visual`（テーマ値の 1 フレーム 1 lock） | `view.rs:317` |
| `lang()`（status 行の文言） | `view.rs:473` → `launcher_controller.rs:673-686` |

つまり `auto_hide_enabled` の追加は 2 → 3 への増分である。1 回あたりは `try_state`（TypeId ルックアップ）+ 非競合 `Mutex::lock` + `bool` コピー ≈ 10^2 ns 級。契約②（`repaint.rs` の `pace`）でフレームは monitor リフレッシュレート（144Hz 実機）に上限されるので上界は **~1.4×10^4 ns/秒 = 0.0014%/コア**。#737 が問題にした paint コストは 1.64ms/frame＝これより 4〜5 桁大きい。**測定可能な回帰にならない。**

なお高頻度フレーム（144fps）が出るのはポインタ移動中＝ `Focused` のときであり、その場合 `observe` は `auto_hide` を消費しないまま `Idle` を返す。「使われないのに読む」は成立するが、上の見積りのとおり値札が無い。

**(d) 自己デッドロックの懸念は無い。** `view.rs:473` の `lang()` が同一フレームの後段で engine lock を無条件に取れている以上、フレーム本体を通して lock を保持している経路は無い。段 16–17（`view.rs:427`）でもう 1 回取ることは新しい危険を作らない。

**(e) 遅延評価案は borrow checker で成立しない（考慮のうえ却下）。**
`self.blur_grace.observe(focused, now, || self.auto_hide_enabled())` は**コンパイルできない**——レシーバが `&mut self.blur_grace`、クロージャがメソッド呼び出しのために `&self` 全体を捕捉するため衝突する。成立させるには `auto_hide_enabled` を `&tauri::AppHandle` を取る自由関数へ降格し、クロージャを `|| auto_hide_enabled(&self.app_handle)` と書いてフィールド単位の disjoint capture に頼る必要がある（edition 2021）。**キャッシュ化ではないので #576 には抵触しない**が、「lock 1 回/フレームを節約する」ためだけに既存メソッドの形と可読性を崩す取引であり、(c) の見積りに見合わない。

**(f) `read_visual` の `VisualSnapshot` へ相乗りする案も却下。** `src-tauri/CLAUDE.md`「モジュール構成」が `VisualSnapshot` の射程を `[visual]`（色・font・padding + `show_icons`）と明示し、`window_gap` を意図的に除外している。`general.auto_hide_on_focus_lost` を載せるのは同節の分担宣言を壊す。

**計画への要望（軽微）**: `on_focus_changed` 書き換え後の doc に「`auto_hide` は毎フレーム live-read になる（#576 の設計どおり・armed 限定だった旧形は `if let` のネストの副産物であって意図ではない）」を 1 行残すこと。次に「なぜ毎フレーム lock するのか」を問う読者がこの導出をやり直さずに済む。

---

## 2. `Instant::now()` の呼び出し位置

### 判断: 毎フレーム 1 回読む計画で正しい。コストは無視してよい。ε の向きは安全側で確定（軽微）

**(a) 毎フレーム必要か → 「armed のときだけ」に絞る価値は無い。**
`Instant::now()` 相当（`Instant::elapsed()`）は既に**毎フレーム無条件に 3 回**走っている:

| 呼び出し | 位置 | 条件 |
|---|---|---|
| `self.notice.poll(self.notice_base.elapsed())` | `launcher_controller.rs:1000` | 無条件（段 10–12） |
| `self.notice.remaining(self.notice_base.elapsed())` | `launcher_controller.rs:1003` | 無条件（段 10–12） |
| `self.search_debounce.poll(self.last_input_at.elapsed())` | `launcher_controller.rs:1258` | 無条件（段 27） |

計画の 1 回読みは 3 → 4 への増分。Windows の `Instant::now()` は `QueryPerformanceCounter`（~20–30ns、syscall ではない）。144fps で ~4µs/秒。**無視してよい。**

しかも現行は armed フレームで `Instant::now()`（武装時・`:1079`）と `at.elapsed()`（`:1087`）の**2 回**読んでいるので、armed フレームに限れば計画のほうが安い。

**(b) 型の中で時計を読まない契約は維持すべきである。** これは性能ではなく安全性の条項で、`lifecycle.rs:41-46` の doc が記録する underflow → `panic = "abort"` の再来を構造で塞ぐ。計画の「`now` は `view.rs` で 1 回だけ読む」は正しい。武装アームで `blur_grace_action(ZERO, ..)` を渡す形も、`now - now` を計算させないぶん更に安全側である。

**(c) ε の向き — 計画の主張は正しく、機構まで確定できる。**
`snotra-egui-runtime/src/runtime.rs:318` が `callback_scheduler.request(info.delay)` を egui の repaint callback（pass 末尾に 1 回・`info.delay` は当該 pass の全 `request_repaint_after` の最小値）で撃ち、`repaint.rs:50-52` の `request` がその時点で `Instant::now() + delay` へ**絶対時刻化する**。すなわち **duration → deadline の変換は 1 フレーム 1 回・pass 末尾**である。

したがって:

- 現行: pass 末尾 + (100ms − ε)
- 本案: pass 末尾 + 100ms

**本案のほうが ε だけ遅い＝予約が早まらない。** 計画の「差は ε の消失だけで向きは安全側」は正しい（機構の根拠は上記 2 ファイル）。

**(d) 契約③と噛み合って何か起きるか → 起きない。**

- `elapsed < BLUR_GRACE ⇒ remaining > 0` が厳密に成り立つため（`lifecycle.rs:67-68`）、`Rearm(ZERO)` の永久スピンは構造的に不能。契約③の但し書き（時計と無関係な条件で再要求するな）にも抵触しない——`Rearm` は時間経過で解消する不成立にしか出ない。
- ε が遅くなる帰結の最大値は「境界フレームで `elapsed` が 100ms にわずかに届かず `Rearm(数十µs)` をもう 1 周する」だけ。これはまさに契約③が想定する形であり、次フレームで `Hide` に着地する。契約②の `pace`（`repaint.rs:116-117`）が `deadline.max(gate)` で遅らせる側にしか働かないので、そこでも取りこぼしは起きない。
- 逆に `request_repaint_after` の 2 重発火（現行は `:1080` の `BLUR_GRACE` と `:1094` の `Rearm(100ms−ε)`）が 1 本に畳まれる。`repaint.rs:180-186` の coalescing が最小を採る現行では実効差が無いので、これは挙動不変の整理である（計画「未確定」1 の実測と一致）。

---

## 3. 契約③・契約④との整合

### 判断: 3 アームとも wake の責務に抜けは無い。`Idle` の `Blurred(t)` 残留も現行同値で問題なし。契約④の穴は #745 とは独立の既知残余（軽微）

**(a) 次フレームを起こす責務**

| アーム | 誰が起こすか | 抜け |
|---|---|---|
| `Hide` | **誰も起こさない。それが正しい。** `emit_hide()`（`launcher_controller.rs:199-209`）→ `EGUI_HIDE_REQUESTED` → `mod.rs:414-419` の listener が `on_event_loop(&handle, hide_egui_main)`。以後 hidden なのでフレームは要らない（契約④） | 無し |
| `Rearm(d)` | 呼び出し側の `ctx.request_repaint_after(d)`。armed の間毎フレーム再要求（契約③） | 無し |
| `Idle` | 誰も起こさない。残る入力は `focused`（tao の `on_window_event`）と `auto_hide`（`config-applied` の `wake_main`）で、どちらも wake の持ち主がいる（`lifecycle.rs:53-55` が正本） | 無し |

**計画が `Idle` の返る場面を増やしても、新しい wake 義務は生まれない。** 増えるのは (i) `focused == true` の全フレーム (ii) `NeverFocused + !focused` の 2 つで、いずれも現行は「段 14 が `unfocus_at` を消す」「`if let Some` が不成立」で**同じく何もしていない**（`launcher_controller.rs:1035-1041` / `:1085`）。挙動は同値。

**(b) `Idle` を返したフレームで `Blurred(t)` が残ることの是非 → 問題なし。**

- 現行と同値である（現行も `unfocus_at = Some(t)` が残る）。#745 が塞ぐのは hide を跨ぐ持ち越しだけ、という前提は正しい。
- 残留が意味を持つ唯一の生き筋は「auto_hide を off → on にする」。このとき `config_watcher` の `config-applied` が `wake_main` を撃ち（`src-tauri/CLAUDE.md`「モジュール構成」`config_watcher.rs` 節）、走ったフレームで `elapsed >> BLUR_GRACE` → `Hide` になる。**猶予明けの blur 状態で auto_hide を有効化すると即 hide する**——これは現行の挙動でもあり、#745 は変えない。挙動として妥当（focus は実際に失われている）。
- 猶予**中**に auto_hide が off でも `Rearm` が返り最大 100ms は再要求が続く（`lifecycle.rs:65-71` の分岐順）。無駄フレームだが有界かつ現行同値。

**(c) 契約④と reset-on-show の完全性 — 穴はあるが #745 の射程外の既知残余である。**

- **show 経路は 1 本に収束している。** `show_egui_main`（`window_coordinator.rs:243-256`）が**無条件に** `reset_pending.store(true)` を撃ち、呼び出し点は `main.rs:242 / :426 / :444 / :577` と `mod.rs:482` の全 5 か所（grep 実測）。ゆえに「show されたら必ず次の最初のフレームで `reset()` が走る」は成立する。
- **ただし構造的な保証ではない。** `src-tauri/CLAUDE.md`「モジュール構成」（`.md:114`）が記すとおり、`Manager` から main のハンドルを引いて `.show()` を呼ぶ書き方は**任意のスレッドからコンパイルが通り、main では実際に効く**。#880 サイクル段 2 時点で呼び出し点は無いと grep 実測済みの**受容する残余**である。reset-on-show の完全性はこの grep 済みの性質に乗っている。**#745 はこの穴を広げも狭めもしない**——修正後の記述で「reset-on-show が backstop である」と書くのはよいが、「唯一の backstop であり穴が無い」とは書かないこと（全称表現は前提とセット・`AGENTS.md`「検証の作法」）。
- **起動時の初回 show は `reset_pending` に依存しない。** `window_coordinator.rs:250` の store は `if let Some(sh) = app.try_state::<EguiShellState>()` の内側にあり、managed state 未搭載なら黙って落ちる。しかし `BlurGrace::NeverFocused` が `LauncherController::new`（`:139-140` の置換先）の初期値になるので、初回 show では reset の有無に依らず正しい。**この安全性は初期値が `NeverFocused` であることに依存する**——計画表の `:139-140` 置換にその意図を doc として残すとよい。
- **hide 側にクリアを置けないことの確認**: `hide_egui_main`（`window_coordinator.rs:434`）は `&AppHandle` と `EventLoopProof` しか持たず `LauncherController` に触れない。計画「軽微」節の主張は正しい。

---

## 4. reset の位置（段 3 と段 16–17）

### 判断: 順序依存は**実在し、load-bearing である**。現状その依存を記録するコメントが無い → **要対処**

**(a) 段 3 と段 16–17 の間に blur 状態を触るものは無い（実測）。**
`view.rs:323`（段 3）と `:427`（段 16–17）の間にあるのは `reset_size_guard` / `last_set_*` リセット / `consume_external_pending` / `set_clear_color` / `visuals_mut` / `configure_japanese_font` / `poll_async` / `read_pre_widget_input`（段 13）/ `clear_blur_grace_if_focused`（段 14・**本計画で削除**）/ `on_escape_pressed`（段 15）。本計画の後に `blur_grace` へ触るのは `reset()` と `observe()` の 2 つだけになるので（計画の入口 2 個の主張）、間に書き手はいない。

なお段 15 の Escape は同一フレームで `emit_hide()` を撃ちうるが、`hide_pending.swap(true)`（`:199-208`）が重複を潰すため段 16–17 の `Hide` と衝突しない。

**(b) 順序そのものは load-bearing である。**
段 3 を段 16–17 の後ろへ動かすと、show 直後の `focused == false` フレームが stale な `Blurred(t)` に対して `observe` を走らせ **`Hide` を返す＝ #745 がそのまま再発する**。

**(c) 要対処: その依存を記録する場所が計画に無い。**
`view.rs:332-337` のコメントは、`consume_reset_pending` の呼び出し位置を **#749 の理由**（同一フレームの `drive_results_window` より前でなければならない）**だけ**で固定している。本計画の後、この位置は #745 の理由も同時に背負う。計画の `view.rs` 行（変更ファイル表）は `:421` / `:427` / `:997` / `:636` しか挙げておらず、**`:332-337` が対象に入っていない**。

将来 #749 の制約が失効した編集者は、そこに書かれた唯一の理由が消えたと見て呼び出し点を動かせてしまう。**`:332-337` へ「段 16–17（`observe`）より前であること＝ #745 の backstop 本体」を 1 行足すこと。** 計画の不変条件表が「段 3 と段 16–17 の呼び出し順序 = テスト不能・doc コメントが担う受容残余」と書いている、その doc コメントの**置き場所が確定していない**のが唯一の実質的な欠落である。1 行の編集で、受容残余が「記録された依存」へ降格する。

**(d) reset フレーム自体が `focused == false` だった場合 → 期待どおり `Idle`。**
段 3 で `reset()` → `NeverFocused`、段 13 で `pre.focused == false`、段 16–17 の `observe(false, now, _)` は `NeverFocused` 据え置き + `Idle`。**武装しない**。これがシナリオ B の閉じ方そのものであり、計画のテスト 3 が固定する。副作用として、その窓は一度も focus を得ない限り永久に auto-hide しない——`set_focus()` が恒久的に失敗する環境では「blur で消えない窓」になるが、**それは「focus を失う」遷移が起きていない以上 SPEC §8.6 と整合する**（計画の SPEC 不要論と同じ根拠）。

---

## ⚠️ 確信の持てない所見

| # | 所見 | 確信度 | 影響 |
|---|---|---|---|
| ⚠️1 | show 後の**最初のフレーム**で `ctx.input(\|i\| i.focused)`（`view.rs:207`）が `true` を返すか `false` を返すかは未確認（tao の `Focused(true)` が最初の `RedrawRequested` より前に配送されるかに依る） | 中（未実測） | **どちらでも安全**。`true` → `Focused`（正常な武装元）、`false` → `NeverFocused` のまま `Idle`。**この 1 点のために実機スモークを 1 回使う価値は無い** |
| ⚠️2 | `Instant::now()` のコスト見積り（QPC ~20–30ns）は一般知識であり本機で実測していない。engine `Mutex` の非競合 lock コスト（~10^2 ns）も同様 | 中 | 結論（無視してよい）は 3〜4 桁の余裕があるので、係数が 10 倍ずれても覆らない |
| ⚠️3 | `lifecycle.rs` は現在 `Instant` を import しておらず `std::time::Duration` を完全修飾で使っている。`BlurGrace` 追加時に import 方針（完全修飾 `std::time::Instant` を貫くか `use` を入れるか）が要る | 高（事実）／低（重要度） | 純粋な様式。ビルドで即分かる |
| ⚠️4 | 猶予中に auto_hide が off でも `Rearm` が返り最大 100ms 分の無駄フレームが出る（`lifecycle.rs:65-71` の分岐順序の帰結）。現行同値なので #745 では触らない判断でよいが、「`Idle` は auto_hide off で返る」と読める計画の記述（`BlurAction` 表の `Idle` = 「auto_hide off / focus 復帰」）は**猶予明け後に限る**という限定が落ちている | 高 | 文言のみ。実装は変わらない |
| ⚠️5 | `config-applied` の `wake_main` が **hidden 中にも撃たれる**（可視性を見ない・設計 spec §3 契約③の具体例が明記）ため、hidden 中の wake は tao/OS 層で落ちる。よって auto_hide の変更が hidden 中に起きても、次の show の reset で `NeverFocused` になる以上取りこぼしは無い——と読んだが、この経路は #745 の射程外で追試していない | 中 | 無し（reset-on-show が上流で吸収する） |

---

## 分類

### 要対処（1 件）

1. **`view.rs:332-337` のコメントに #745 の順序依存を追記し、計画の変更ファイル表（`view.rs` 行）へその箇所を加える。** 現状このコメントは #749 の理由だけで呼び出し位置を固定しており、#749 が失効したときに #745 が沈黙で再発する経路が開く。計画が「受容残余」と呼んでいる doc コメントの置き場所は、ここ以外に無い。

### 軽微（5 件）

2. `on_focus_changed` 書き換え後の doc へ「`auto_hide` は毎フレーム live-read になる（#576 の設計どおり・armed 限定は `if let` の副産物であり意図ではない）」を 1 行。遅延評価案を borrow checker の事実（(1)(e)）とともに却下した記録も添えると、次の読者が再導出しない。
3. ε の記述（計画「未確定」2）に機構の根拠を添える: duration → deadline の変換は `runtime.rs:318` → `repaint.rs:50-52` で **pass 末尾に 1 回**行われるので、本案は現行より ε だけ**遅い**＝予約が早まらない側で確定。
4. `LauncherController::new` の初期値が `NeverFocused` であることが、`reset_pending` の store が `try_state` ガードの内側にある（`window_coordinator.rs:250`）起動時 show の安全性を担っている。置換行の doc に一言。
5. 契約④の記述を更新するとき「reset-on-show が**唯一の**backstop」と全称で書かないこと。`Manager` からの `.show()` 直呼びは今もコンパイルが通り main では効く受容残余（`src-tauri/CLAUDE.md:114`）で、#745 はそれを狭めない。
6. `BlurAction::Idle` の説明（計画・`lifecycle.rs:35` の既存 doc とも）に「**猶予明けに**」の限定が要る（⚠️4）。

### 未検証（3 件）

7. show 後初フレームの `i.focused` の値（⚠️1）。**両値とも安全なので追試不要**と判断する。
8. `Instant::now()` / `Mutex::lock` のコスト実測（⚠️2）。結論に 3〜4 桁の余裕があるため追試不要。
9. `set_focus()` の実失敗頻度（計画が既に「未検証」へ挙げている）。本レンズからは追加所見なし——時間・フレーム側の帰結（reset フレームが `focused == false` になること）は (4)(d) のとおり閉じている。
