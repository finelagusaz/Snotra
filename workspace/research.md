# 調査 — #745 blur 猶予が reset-on-show の backstop の外にいる

## issue の要約

`view.rs` の reset-on-show（`reset_pending` 消費）は `launching` / `notice` / `search_debounce` など hide を跨ぐ時限状態をクリアするが、**blur 猶予の `unfocus_at` と `was_focused` をクリアしていない**。猶予が armed のまま別経路（ホットキーのトグル hide・Escape・トレイ）で hide されると、stale な `unfocus_at` が次の show へ持ち越され、**show 直後の最初のフレームで `focused == false` が観測されると即座に自動 hide される**。

契約④（`docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md`）と `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」は「hide を跨ぐ in-flight 状態は reset-on-show の backstop とセットで設計する」と定めており、`unfocus_at` はその規範の対象でありながら backstop の外にいる。

## issue の行番号は古い（実装が移動済み）

issue 本文は `view.rs:1299-1301` と「`view.rs` の reset-on-show」を指すが、#666 段 3 タスク 2 で `launcher_controller.rs` へ移設済み。**現在地（`9ebf3db` 時点・grep 実測）**:

| 役割 | 現在地 | view.rs の呼び出し点 |
|---|---|---|
| 段 3 reset-on-show | `launcher_controller.rs:917 consume_reset_pending` | `view.rs:323` |
| 段 14 focus 復帰でクリア | `launcher_controller.rs:1035 clear_blur_grace_if_focused` | `view.rs:421` |
| 段 15 Escape ラダー | `launcher_controller.rs:1047 on_escape_pressed` | `view.rs:423` |
| 段 16–17 blur 検知・猶予処置 | `launcher_controller.rs:1075 on_focus_changed` | `view.rs:427` |
| 段 34 focus の畳み込み | `launcher_controller.rs:1304 set_focused` | `view.rs:997` |

フィールド定義は `launcher_controller.rs:104-105`（`was_focused` / `unfocus_at`）、初期化は `:139-140`。

## 危険なのは `unfocus_at` だけで、`was_focused` は逆に masking 側

`on_focus_changed`（`:1078-1079`）は `was_focused && !focused` のとき `unfocus_at` を **`Instant::now()` で上書きする**。

| show 初フレームの状態 | 挙動 |
|---|---|
| `focused == true` | 段 14 が `unfocus_at = None`。安全（**現在の masking**） |
| `focused == false`, `was_focused == true`（stale） | 段 16 が今の時刻で**再武装** → `Rearm(100ms)`。即時 hide しない |
| `focused == false`, `was_focused == false` | 再武装されず stale な `unfocus_at` で `elapsed ≫ 100ms` → **`Hide`（欠陥）** |

猶予 armed 中に別経路 hide したときに残るのは **3 行目**である（blur を観測したフレームの段 34 が `was_focused = false` を書いて終わっているため）。

**ただし「`was_focused` は守っている側」は、シナリオ A の中でだけ真である**（2026-08-04 の独立導出が指摘・以下は再検証済み）。持ち越しうる状態は 2 通りあり、**それぞれ別の欠陥を起こす**:

| 持ち越す状態 | 起きる経路 | 症状 |
|---|---|---|
| **A**: `was_focused=false`, `unfocus_at=Some(古)` | blur で武装 → 100ms 未満に別経路 hide | show 初フレームが `focused==false` なら **即座に** hide（issue 本文が記す欠陥） |
| **B**: `was_focused=true`, `unfocus_at=None` | **focus を持ったまま hide**（Enter での起動成功・ホットキーのトグル・トレイ） | show 初フレームが `focused==false` なら段 16 が**新規に武装**し、**100ms 後に** hide（issue 未記載） |

- A は `unfocus_at` のクリアで閉じる。**B は `was_focused` のクリアでしか閉じない。**
- **B の方が到達しやすい**——A は「猶予中の 100ms に別経路 hide」という狭い窓を要するが、B は「focus を持ったまま hide」という**通常の hide すべて**が前提を満たす。どちらも show 側の `focused==false` を要する点は同じ。
- ゆえに**両フィールドのクリアがそれぞれ独立に必要**であり、`was_focused` を残す設計は選べない。
- 意味論としても整合する — 「focus を持っていた窓が失った」が猶予の前提であり、show 直後は「まだ一度も focus を持っていない」が正しい初期状態。

## masking は「今日でも」成立しない回がある（起票後に判明した最重要事実）

issue 本文は「show 後の初フレームを起こすのは `Focused(true)` 自身だから今は守られている」と書くが、**2026-07-26 のコメント（Codex の敵対的レビュー）がこれを覆している**。

裏取り（`9ebf3db` で実測）:

- `window_coordinator.rs:341` — `let _ = window.set_focus();` **戻り値を捨てている**
- `show_egui_main`（`:243` 開始）は `request_repaint` / `wake_main` を呼ばない。grep 実測で、この関数が起こすのは `window.show()` と `set_focus()` だけ
- `SetForegroundWindow` は部分的に非同期で失敗しうる（`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」が記録する既知の性質）

ゆえに `set_focus()` が失敗した回は `Focused(true)` が来ず初フレームも起きない。**その後の無関係な wake（`config-applied` 等）で走る最初のフレームは `focused == false`** になり、上の 3 行目に落ちる。

**帰結**: 本件は「将来の変更で壊れる潜在」ではなく、**低頻度ながら今日でも到達しうる欠陥**である。優先度の見積もりと、修正の正当化の強さが変わる。

## SU2 の設計 spec は既にこのリセットを要求していた（実装漏れである）

`docs/superpowers/specs/2026-07-22-su2-window-shell-design.md:83`（grep 実測）:

> **stale 猶予の防止（codex #8）**: `focused` のとき `unfocus_at=None`。加えて **show のたびに view 側の `was_focused`/`unfocus_at` をリセット**（再表示直後に前回の stale な猶予で即 hide しない）。

**前半だけが実装され、後半（show のたびのリセット）が実装されていない。** 現在の `clear_blur_grace_if_focused` が前半に当たる。

**帰結**: #745 は「新たに設計判断を要する変更」ではなく、**明示された設計要求の実装漏れ**である。設計意図の再検討は不要で、当時の文言どおり両フィールドをリセットすればよい。案 B の型への凝集は、その実装漏れが**二度と起きない構造**（reset の入口を型が持つ）を足す部分に当たる。

なお同 spec の `:82` / `:84` は `SettingsProcessState` 非起動を判定条件に含める設計（「サイドカーガード必須」）を記すが、これは #746 で SPEC 逸脱として撤去済みである。`docs/superpowers/` は非規範の歴史資料（同ディレクトリの `README.md`）ゆえ更新義務は無い。

## `launcher_controller.rs` はユニットテストを持たない

`egui_shell/` の 12 ファイル中 11 に `#[cfg(test)]` があるが、**`launcher_controller.rs` には 1 つも無い**（grep 実測）。`tauri::AppHandle` に縛られた imperative shell だからである。

**ゆえに、構造を変えない限り本件の修正はテスト段へ上げられない** — 「reset でクリアしたこと」を固定する検査が書けない。これが案 A / 案 B の分岐点になる（`docs/development-principles.md`「強制の階梯」）。

## 段の分離は意図された不変条件である

`clear_blur_grace_if_focused` の doc（`:1033-1034`）が明示する:

> 段 14: focus が戻っていれば blur 猶予を捨てる。**段 16–17（`on_focus_changed`）とは間に Escape ラダー（段 15）を挟んで別の塊であり、束ねると順序が動く。**

`on_focus_changed` の doc（`:1072-1074`）も「前フレームの focus は `was_focused` が持ち、更新は段 34 が行う——この 2 段の間に書き手は無い」と記す。

**帰結**: 新しい型を作っても **4 つの入口（段 3 / 14 / 16–17 / 34）を 1 メソッドへ束ねてはならない**。

## 時計の扱い（既存の安全性）

`blur_grace_action` の doc（`lifecycle.rs:39-46`）は、`elapsed` を呼び出し側が 1 回だけ読んで渡すことが `Duration` 減算の underflow（release は `panic = "abort"`）を防いでいると記す。現在は `on_focus_changed` が `at.elapsed()` で 1 回読む形で守っている——**散文による保証である**。

なお `on_focus_changed` は armed になったフレームで `request_repaint_after` を **2 回**撃つ（`:1080` の武装時と、`Rearm(remaining)` アームの `:1096`）。同じ deadline（≈100ms）なので挙動は同じだが、冗長である。

## 再利用できる既存パターン

- **純粋核 + 副作用の分離**: `search_state.rs`（`SearchState` / `EscapeOutcome`）が同型の先例。判定を型に閉じ、副作用は `launcher_controller` に残す
- **`#[must_use]` による取り落とし検出**: `on_escape_pressed`（`:1046`）と `.claude/rules/src-tauri.md`「状態フラグを true にしたら false に戻す経路とセットで設計する」の `launch_settings_process` が先例
- **実機確認の配管**: #746 で確立した `SNOTRA_CONFIG_DIR` の使い捨てプロファイル + `SNOTRA_TRACE` + `scripts/lib/SnotraSmoke.psm1` の `New-SnotraVerificationProfile`。実 config を触らずに `auto_hide_on_focus_lost` を切り替えられる

## 技術的制約

- **契約③の凍結判断**: 設計 spec が「共通 `Deadline` primitive の抽出は行わない（同型の 4 例目が出た時点で再検討）」と記録している。本件は契約④（reset backstop）側の話であり、**4 つの armed 期限を横断する抽象化ではない**ため抵触しない。ただし計画で明示すること
- **SPEC 変更は不要**: `SPEC.md` §8.6 の `SearchVisible --> Standby: focus_lost` は「focus を失う」遷移であり、**一度も focus を得ていない窓に `focus_lost` は起きない**。修正後の挙動（show 後に focus を得られなかった窓は自動 hide しない）は SPEC と整合する
- `BlurAction` / `BLUR_GRACE` / `blur_grace_action` は `mod.rs:65` で re-export 済み。`blur_should_hide` は**外へ出さない**（`mod.rs:62` の明示）

## 未解決の疑問

計画の「未確定」欄へ送る。
