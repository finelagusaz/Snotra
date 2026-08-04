# ADR-blur-grace-single-field-state-machine: blur 猶予を 1 フィールドの状態機械へ畳む

## 文脈

#745 は「blur 猶予の状態（`was_focused: bool` / `unfocus_at: Option<Instant>`）が hide を跨いで次の show へ持ち越され、show 直後の初フレームが `focused == false` だと自動 hide される」欠陥である。

修正そのものは「reset-on-show（`consume_reset_pending`）で両フィールドをクリアする」で足りる。決めるべきだったのは、**そのクリアをどの形で置くか**である。

issue の受け入れ条件が「純粋核のテストで固定できる部分と、view 層の状態遷移（クリア経路）を**区別して検証**する」を要求しており、`launcher_controller.rs` が `tauri::AppHandle` に縛られて `#[cfg(test)]` を 1 つも持てないことが、この判断の分岐点になった。

**根本原因の把握を途中で訂正している。** 当初は「2 変数への分解が欠陥を生んだ」と考えたが、一次資料は違った——SU2 の設計 spec（`docs/superpowers/specs/2026-07-22-su2-window-shell-design.md`）が初日から「show のたびに `was_focused`/`unfocus_at` をリセット」を明記しており、SU2 プランが 5 項目を 1 つの ✓ で束ねたために前半だけの実装が完了扱いになった。**実装漏れと追跡の断絶であって、表現の問題ではない。** この訂正は下の却下理由の重みを変えた。

## 決定

blur 猶予を `lifecycle.rs` の 1 フィールドの状態機械 `BlurGrace { NeverFocused, Focused, Blurred(Instant) }` へ畳み、フレーム内の入口を 4（段 3 / 14 / 16–17 / 34）から 2（段 3 / 16–17）へ減らす。`observe` は時計を読まず、`now` は呼び出し側がフレームに 1 回だけ読んで渡す。

## 検討した代替案と却下理由

- **案 A: `consume_reset_pending` に 2 行足すだけ（型を作らない）**: 却下。差分は最小で欠陥は閉じるが、`launcher_controller.rs` にユニットテストが書けないため**受け入れ条件 2 を満たせない**。「reset でクリアした」を固定する検査が 1 つも置けず、修正の正しさが目視と実機だけに依存する。なお #745 の独立導出（別エージェント）はこの案を導いており、**導出漏れではなく受け入れ条件の読みの差**である。
- **案 B: 2 フィールドのまま `struct BlurGrace { was_focused, unfocus_at }` へ包む**: 却下。テスト可能性は得られるが、**フレーム内の入口が 4 のまま残る**（前フレームの focus を畳む段 34 と、focus 復帰で片方だけ消す段 14 が独立し続ける）。また「reset で両方消したか」という問いが型の中へ移動するだけで消えない。
- **共通 `Deadline` primitive の抽出（4 つの armed 期限を横断）**: 却下——**再確認**。`docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` が「契約③は規範に留める。同型の 4 例目が出た時点で再検討する」と凍結しており、本件はその 4 例目ではない（契約④＝reset backstop 側の話であり、blur 猶予**単体**の凝集である）。
- **`auto_hide` を遅延評価で渡す（`observe(.., || self.auto_hide_enabled())`）**: 却下。現行は `if let Some(at)` のネスト内で読むため armed のときしか engine lock を取らないが、値渡しにすると毎フレーム取る。それを避けるクロージャ形は**borrow checker を通らない**——レシーバが `&mut self.blur_grace`、クロージャが `&self` 全体を捕捉して衝突する。加えて `auto_hide_enabled` の doc は「都度読む（キャッシュしない・#576 と同設計）」であり、armed 限定はネストから落ちた副産物であって意図ではない。engine lock は既に毎フレーム無条件で 2 回（`read_visual` / `lang()`）走っており、2 → 3 に増えるだけである。
- **`observe` の `focused` / `auto_hide` を `enum` で型分けする**: 見送り（却下ではない）。両者とも `bool` であり、**呼び出し点で取り違えてもコンパイル・テスト・smoke がすべて通る**ことを実測で確認した。呼び出し点が 1 か所で局所変数名も一致しているため実害の確率は低いが、**検出手段は存在しない**。塞ぐなら `enum AutoHide { Enabled, Disabled }` が適切で、必要になった時点で行う。
- **段番号（段 14 / 段 34）を振り直す**: 却下。`view.rs` の `read_pre_widget_input` が既に「旧・段 14〜20 相当の位置」と歴史的番号で書いており、振り直すと既存参照が一斉に腐る。欠番を残す。

## 「束ねると順序が動く」の扱い

段 14（`clear_blur_grace_if_focused`）の doc は「段 16–17 とは間に Escape ラダー（段 15）を挟んで別の塊であり、**束ねると順序が動く**」と警告していた。本決定はこの 2 段を合流させている。

根拠: (1) `was_focused` / `unfocus_at` に触るのは `launcher_controller.rs` の 3 メソッドと初期化だけで、段 14〜34 の間に外部の読み手がいない（grep 実測） (2) Escape ラダーは両フィールドを読まず、同一フレームで両方が hide を出しても `emit_hide` の `hide_pending.swap(true)` が潰す (3) `update()` に `return` は 0 件で段 34 は必ず走る (4) **当の doc は `f3c53f1`（#785 の機械的なファイル分割）単独で入っており、分割時の注意書きであってライブな制約の記録ではない**（`git log -S` 実測）。

**それでも文書化された意図的分割を畳む判断であるため、人間の裁定を経ている**（2026-08-04）。

## 帰結

- 到達不能だった `(was_focused=true, unfocus_at=Some)` が**表現不能**になる。到達可能な 3 組と enum は全単射（独立導出 1 体が別経路で確認）。
- `blur_grace_action` / `BLUR_GRACE` は `lifecycle.rs` 内へ閉じた。公開したままだと `observe` を迂回する経路が残り、**#711 が「消費点の一本化を型で塞ぐ」ことで得たものが散文へ戻る**。
- 武装と経過算出が同じ `now` を使うため、`Duration` 減算の underflow 経路が構造的に消える。予約の実効期限は ε だけ遅くなる（安全側・予約が早まらない）。
- **受容残余**: `consume_reset_pending` が `reset()` を呼び続けることに検知手段が無い（`launcher_controller.rs` はユニットテストを持てない）。**これは #745 が最初に失敗した当の経路であり、本決定はそれを塞がない。** 追加した対照テスト 2 本も、呼び出し点の消失ではなく `reset` の**部分実装**を捕まえる（実測）。機械化は #930（trace 不変条件 H6）が追う。
- **受容残余**: 猶予（100ms）以内の人間の操作を要求する実機シナリオは実行不能である（単純反応時間が〜200ms）。注入での自動化も `SetForegroundWindow` の制限と呼び出しコスト（165ms）で届かない。猶予まわりの実機確認を設計する者は、この形のシナリオを書かない。

---

- 決定日: 2026-08-04
- 関連: #745, #711（消費点の一本化）, #746（`!settings_running` の撤去）, #930（H6）, 契約③・契約④
