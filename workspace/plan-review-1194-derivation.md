# 独立導出レビュー: #1194（クランプ抑止条件を「ポインタ非押下」→「OS のモーダル移動ループ中でない」へ）

対象 issue: **#1194**

既存の `workspace/plan.md` / `research.md` / `adversarial-1194.txt` および `.claude/worktrees/` は
読んでいない（独立導出の条件）。grep はすべて `git grep` に `':(exclude)workspace'`
`':(exclude).claude/worktrees'` を付けて走らせた。

---

## 0. 現状の一次証拠（読んで実在を確認した箇所）

- `src-tauri/src/egui_shell/view.rs:1279-1281` — 抑止条件の**唯一の実在点**:
  ```rust
  if !ui.input(|i| i.pointer.any_down()) {
      crate::egui_shell::clamp_main_into_work_area(&app, metrics.bar_height);
  }
  ```
- `src-tauri/src/egui_shell/view.rs:1269-1278` — 上のブロックの直前コメント（「**ポインタが押されて
  いないフレームだけ**である」）。**呼び出し側にしか無い制約**（`drive_results_window` より前）も
  ここに在る。
- `src-tauri/src/egui_shell/view.rs:1285-1288` — `check_show_bar_rect` 側のコメント。
  「**クランプの `!any_down()` の外に置く**」と識別子で名指している。
- `src-tauri/src/egui_shell/window_coordinator.rs:817-838` — `clamp_main_into_work_area` 本体
  （`#[cfg(windows)]`）。`840-841` に `#[cfg(not(windows))]` の no-op 版。
- `src-tauri/src/egui_shell/window_coordinator.rs:752-815` — 同関数の doc。#1173 の実測表と、
  代替判定の候補 `GetGUIThreadInfo` の `GUI_INMOVESIZE`（「**実呼び出しは未検証**」）が既に在る。
  **#1194 はこの doc が自分で予告した穴を埋める作業である。**
- `src-tauri/src/egui_shell/window_coordinator.rs:704-717` — `read_bar_anchor`（クランプと hide 保存が
  共有する材料の導出）。
- `src-tauri/src/monitor.rs:36` — `WorkArea::clamp`（純粋核。`monitor.rs:134-183` にユニットテスト 7 件）。
  **算術は変えない。**

### 0.1 Win32 API の実在（`windows` 0.62.2・vendored ソースで確認）

すべて `Win32_UI_WindowsAndMessaging` の下にあり、この feature は
`src-tauri/Cargo.toml` の `[target.'cfg(windows)'.dependencies]` で**既に有効**である
（`Cargo.toml:39-56`）。**新しい feature 追加は不要。**

| シンボル | 所在（vendored） |
|---|---|
| `GetGUIThreadInfo(idthread: u32, pgui: *mut GUITHREADINFO) -> Result<()>` | `windows-0.62.2/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs:871` |
| `GUITHREADINFO`（`cbSize` / `flags` / `hwndMoveSize` 他） | 同 `mod.rs:3696-3706`（`hwndMoveSize` は `:3703`） |
| `GUITHREADINFO_FLAGS`（`contains` を持つ） | 同 `mod.rs:3709-3714` |
| `GUI_INMOVESIZE = GUITHREADINFO_FLAGS(2)` | 同 `mod.rs:3746` |
| `GetWindowThreadProcessId(hwnd, Option<*mut u32>) -> u32` | 同 `mod.rs:1184` |

**`GetGUIThreadInfo(0, …)` は「呼び出しスレッド」ではなく「フォアグラウンドスレッド」を指す。**
ゆえに `0` を渡してはならない。正しい形は 2 つあり、後者を推す:

1. `GetWindowThreadProcessId(main_hwnd, None)` でスレッド id を取って渡す
2. **さらに `gui.hwndMoveSize == main_hwnd` を照合する**——同一スレッドが所有する**別の窓**
   （`results` 窓・`SnotraPlatformWindow`）が移動ループに入っているだけの回で、main のクランプを
   黙って殺すことを防ぐ。`hwndMoveSize` の実在は上表のとおり。

### 0.2 マウスドラッグも同じモーダルループである（一次資料で確認）

これが「`!any_down()` に **AND する**のではなく**置き換える**」ことの根拠である。

- `snotra-egui-runtime/src/runtime.rs:544` — `apply_frame_commands` が `frame.drag_requested` で
  `self.window.start_dragging()` を呼ぶ（SPEC.md §8.3「入力欄以外の全域をドラッグ検出し OS の
  ウィンドウ移動へ委譲」の実体）。
- tao 0.35.3 `src/platform_impl/windows/window.rs:529-558` — `drag_window` →
  `handle_os_dragging(HTCAPTION)` は `ReleaseCapture()` の後 `PostMessageW(hwnd, WM_NCLBUTTONDOWN,
  HTCAPTION, …)` を投げる。`DefWindowProc` はこれを受けて**同じモーダル移動ループ**
  （`WM_ENTERSIZEMOVE` … `WM_EXITSIZEMOVE`）へ入る。`Alt+Space` → `M`（`SC_MOVE`）も同じループ。

⚠️ **ただし `PostMessageW` は非同期である**——ドラッグ開始フレームと、ループが実際に始まって
`GUI_INMOVESIZE` が立つ瞬間の間に**隙がある**（下の所見 2）。

---

## 1. 変更の分類（workflow 上の位置）

**これはバグ修正ではなく「仕様変更」である。** `AGENTS.md`「開発ワークフロー」1 の 2 参照で決まる:

- `SPEC.md:478` が現在の挙動（キーボード移動は拘束される）を**as-built として明示的に記述している**
  （#1173 実測）。#1194 はその記述に**合わせる**のではなく**変える**。
- ゆえに更新順は **`SPEC.md` → コード → ドキュメント**。「fix」というコミット種別を選んでも
  SPEC 同期は免除されない（`AGENTS.md` 同項）。

---

## 2. 導出した変更ファイルの一覧

| # | path | 何を変えるか | なぜ必要と判断したか（根拠） |
|---|---|---|---|
| 1 | `src-tauri/src/egui_shell/view.rs` | `:1279` の `!ui.input(\|i\| i.pointer.any_down())` を新しい述語（モーダル移動ループ中でない）へ**置換**。`:1269-1278` のコメントを書き直す。`:1285` の「クランプの `!any_down()` の外に置く」を新しい識別子へ改める | 抑止条件の唯一の実在点。`git grep -n "any_down" -- ':(exclude)workspace'` のヒットは製品コードではここ 1 行だけ（他は doc / ADR / 開発原則の散文） |
| 2 | `src-tauri/src/egui_shell/window_coordinator.rs` | 新しい述語関数を**追加**（`#[cfg(windows)]` 本体 + `#[cfg(not(windows))]` スタブ）。`:752-815` の `clamp_main_into_work_area` doc を全面改訂——「`view.rs` が『ポインタが押されていないフレーム』でのみ呼ぶ」（`:754-755`）／#1173 の穴の記述（`:768-792`）／「代替判定はまだ無い」（`:794-797`）が偽になる。計装の trace 発火点もここか `view.rs` に置く | `read_bar_anchor` / `read_frame_geom` と同じ「Win32 を読む」層であり、`#[cfg(windows)]` 二重定義の先例（`:817` / `:841`）がそのまま使える。doc の当該行は上の grep で実在確認済み |
| 3 | `SPEC.md` | §8.2「表示中の作業領域への復帰（#738）」の 4 か所: `:476`（「ポインタが押されていないフレームでは」）・`:477`（「ドラッグしている間は拘束しない」→ 保証をキーボードへ拡張）・`:478`（**#1173 の as-built 記述を丸ごと差し替え**）・`:481`（「クランプはポインタが押されていないフレームで常に働き」） | §8.2 が挙動の意図の正本。`git grep -n` で 4 行とも実在確認 |
| 4 | `src-tauri/CLAUDE.md` | 「モジュール構成」の `window_coordinator.rs` の項（`:51`）にある「**ポインタ非押下のフレームに限る**」を改める | 同 grep でヒット。`.rs` の doc と同じ命題の**写し**であり、片方だけ直すと `AGENTS.md`「文書に事実の写しを増やす変更」の失敗形になる |
| 5 | `scripts/manual-smoke.ps1` | `:114` の「#738 で『**ポインタを離した時点で**バー矩形を作業領域内へ戻す』を入れた」を改める。あわせて**キーボード移動での目視項目を足すか**を判断する | 概念ラベルでの grep がヒット。`scripts/` の説明文も母集団である（`AGENTS.md`「機構・層・ファイル群を撤去する」行の趣旨） |
| 6 | `docs/adr/ADR-<新 slug>.md`（**新規**。例: `ADR-main-window-clamp-outside-modal-move-loop`） | 採らなかった実装候補と却下理由。issue が明示的に要求している成果物 | **既存の `ADR-main-window-clamp-on-pointer-release` は編集しない**——`docs/adr/ADR-adr-frozen-history.md` が ADR を凍結された歴史と定める（`AGENTS.md`「ドキュメント参照」の ADR 行も「凍結された歴史ゆえ書式を定めない」）。新 ADR が supersede する形にし、コード doc からの既存の名指し（`layout.rs:307`・`window_coordinator.rs:736,766,796`）は**歴史への参照として残す** |
| 7 | `docs/superpowers/plans/<日付>-1194-*.md` ないし `workspace/plan.md` | 計装の設計（下の §5）。**足場を新設するなら撤去条件を成果物自身の doc へ書く** | `AGENTS.md`「調査・測定のための一時的な足場（…**製品コード内の計装**）を新設**または撤去**」行。`heap_trace.rs` の `//!` が先例 |

### 2.1 変えないと判断したもの（根拠つき）

- `src-tauri/src/monitor.rs` の `WorkArea::clamp` と 7 件のユニットテスト（`:134-183`）——
  **算術は一切変わらない**。抑止条件だけを差し替える。
- `docs/development-principles.md:262` — #738 の**過去の**裁定と対照実験を引く歴史記述。命題は
  「対処が人間裁定に触るなら測り直す」であって「クランプは押下で抑止される」ではない。**真のまま**。
- `docs/adr/ADR-main-window-clamp-on-pointer-release.md` — 凍結（上記 #6 の理由）。
- `docs/superpowers/specs/2026-07-22-*.md` / `2026-07-24-*.md` / `plans/2026-07-25-*.md` の
  「ドラッグ中」ヒット — results 窓の追従と `Moved` の話で、クランプの抑止条件ではない。
  **ただし `2026-07-24-646-two-window-ui-design.md:98`「ドラッグ中はネイティブ移動ループが回り
  egui フレームが止まる」は #1194 の前提と食い違う**（下の所見 5）。凍結文書なので直さないが、
  **前提の裁定材料として読む**。
- `SPEC.md:396`（`Alt+Space` をホットキーとしてブロックする表）——ホットキー登録の話であり、
  ウィンドウ移動の拘束とは別命題。**真のまま**。
- `snotra-egui-runtime/` — 述語は `src-tauri` 側の Win32 読みで完結する。runtime に
  `drag_requested` 以上の配線を足す必要は導出されなかった（**AND ではなく置換**にする限り）。

---

## 3. 導出した対象シンボルの一覧（実在確認済み）

### 3.1 既存（触る）

| シンボル | file:line | 役割 |
|---|---|---|
| `clamp_main_into_work_area` | `src-tauri/src/egui_shell/window_coordinator.rs:817`（win）/ `:841`（non-win） | クランプ本体。**中身は変えない**（呼び出し条件だけが動く）。doc は全面改訂 |
| `check_show_bar_rect` | 同 `:883` | 隣接する不変条件検出器。**`!any_down()` の外に置く**という配置理由のコメントが `view.rs:1285` に在り、識別子が消えるので文言が動く |
| `read_bar_anchor` | 同 `:704` | クランプの材料。**触らない** |
| `read_frame_geom` | 同 `:687` | Win32 の読みの唯一点。新しい述語を**ここへ混ぜない**（あちらは「幾何」、こちらは「入力状態」で責務が違う） |
| `WorkArea::clamp` | `src-tauri/src/monitor.rs:36` | 算術の純粋核。**触らない** |
| `crate::trace::trace(event, data)` | `src-tauri/src/trace.rs:44` | 計装の唯一の口。プロセス全体で単調な `seq`（`AtomicU64`）を持つ |
| `"egui_frame"` | `src-tauri/src/egui_shell/view.rs:1321` | 既存のフレーム trace。**時刻密度の材料としてそのまま使える**（新設不要かの判断材料） |
| `Window::hwnd()` | 使用例 `src-tauri/src/egui_shell/mod.rs:420`, `results_window.rs:290` | main の HWND 取得の既存作法（`let Ok(hwnd) = window.hwnd() else { return }` → `HWND(hwnd.0)`） |
| `start_dragging` | `snotra-egui-runtime/src/runtime.rs:544` | マウスドラッグが OS のモーダル移動ループへ入る唯一の経路 |

### 3.2 新設（名前は提案・**未実在**）

| 提案シンボル | 置き場所（提案） | 備考 |
|---|---|---|
| `in_os_move_loop(window: &tauri::Window) -> bool`（仮） | `window_coordinator.rs`、`clamp_main_into_work_area` の直前 | `#[cfg(windows)]` 本体 + `#[cfg(not(windows))]` は `false` を返すスタブ（クランプ側と同じ二重定義の形） |
| trace イベント名 `"egui_main:move_loop"`（仮）等 | 同上ないし `view.rs` | 既存の命名は `egui_<面>:<事象>`（`egui_main:height_mismatch` `egui_main:bar_rect_mismatch` `egui_show:done`） |

**述語の置き場所について**——`.claude/rules/src-tauri.md`「Win32 を呼ぶ経路の新設は `PlatformBridge`
経由を既定とする」に**逆らう提案である**ので理由を書く: (a) 問われているのは
**イベントループスレッド自身の状態**であり、別スレッドへ委ねると答えが変質する。
(b) 同期の即答が要る（このフレームでクランプするかを決める）。(c) クランプ経路は既に
`read_bar_anchor` / `read_frame_geom` で Win32 を直読みしており、既存の直呼びの理由づけと同型。
**この逸脱は新 ADR に却下案として明記すべきである**（`PlatformBridge` 経由を却下した理由）。

---

## 4. この変更で「散文が偽になる」ファイルの一覧

概念ラベル（「ポインタが押されていない」「ポインタ非押下」「離した時点」「ドラッグ中」
「離したら戻る」「any_down」）で `git grep`（`workspace` / `.claude/worktrees` 除外）した全ヒットを
振り分けた。

### 4.1 偽になる（直す）

| file:line | 現在の文言（要旨） |
|---|---|
| `SPEC.md:476` | 「ポインタが押されていないフレームでは…ボタンを離した時点で復帰する」 |
| `SPEC.md:477` | 「ドラッグしている間は拘束しない」——**保証の射程がキーボードへ広がるので書き換え** |
| `SPEC.md:478` | 「キーボードによるウィンドウ移動は、ドラッグと違って拘束される（#1173 実測）」——**丸ごと反転** |
| `SPEC.md:481` | 「クランプはポインタが押されていないフレームで常に働き」 |
| `src-tauri/CLAUDE.md:51` | 「呼ぶのは `view.rs` だが**ポインタ非押下のフレームに限る**」 |
| `src-tauri/src/egui_shell/view.rs:1269-1272` | 「**ポインタが押されていないフレームだけ**である」 |
| `src-tauri/src/egui_shell/view.rs:1285` | 「クランプの `!any_down()` の外に置く」——**識別子が消える** |
| `src-tauri/src/egui_shell/window_coordinator.rs:754-755` | 「`view.rs` が『ポインタが押されていないフレーム』でのみ呼ぶ」 |
| 同 `:761-766` | `was_reset_frame` backstop 却下の段。**純置換なら `any_down()` 固着の受容残余そのものが消える**——「固着は受容残余である」が偽になる。**ただし所見 2 の対処 (a)（`ループ中 OR ポインタ押下` で抑止）を採ると `any_down()` は条件に残り、受容残余も残る**。どちらを採ったかを新 ADR に明記しないと、この行の真偽が読者に判定できない |
| 同 `:768-792` | #1173 の穴の記述と実測表。**症状が消えるので「対照実験で分離した」の現在形が偽になる**（歴史として残すなら時制を変える） |
| 同 `:794-797` | 「**代替判定はまだ無い**」「実呼び出しは**未検証**」——本 PR が両方を偽にする |
| 同 `:803-805` | 再測手順の「対照は `view.rs` の `any_down()` を見る分岐を短絡させて」——識別子が消える |
| `scripts/manual-smoke.ps1:114` | 「#738 で『ポインタを離した時点でバー矩形を作業領域内へ戻す』を入れた」 |

### 4.2 偽にならない（残す）

`docs/development-principles.md:262`、`docs/adr/ADR-main-window-clamp-on-pointer-release.md`（全体・凍結）、
`docs/adr/ADR-show-path-derives-bar-rect.md:13,27`（バー高の導出の話）、
`src-tauri/src/egui_shell/layout.rs:307`（同）、`PERFORMANCE.md:25`（「lock を離した時点」——別語義）、
`snotra-egui-runtime/src/monitor.rs:12` と `runtime.rs:177,255`（`Moved` の連発＝results 追従）、
`docs/superpowers/` 配下の凍結された plan / spec、`SPEC.md:396`。

### 4.3 母集団に入れ忘れやすいもの（明示）

- **PR 本文**（squash で main の commit message になるがファイルの grep には入らない・#1056）。
  「ポインタを離したら戻る」を PR 本文へ書くと、それが 6 枚目の写しになる。
- **新 ADR 自身**——書いている最中に「押下で抑止していた」を現在形で書かないこと。

---

## 5. 計装（issue が要求する 2 つの実測）の導出

`AGENTS.md`「調査・測定のための一時的な足場（…製品コード内の計装）を新設**または撤去**」行が適用される。

### 5.1 何を刻めば 2 つの問いに答えられるか

**問い A: `Enter` 後に 27〜28 px 上へ動かしているのは誰か。**
1 フレームぶんの組で刻む: `(seq, ts_ms, GUI_INMOVESIZE の真偽, クランプ前の outer_position,
クランプが撃ったか, クランプ後の outer_position)`。**「前フレームの clamp 後」と「今フレームの
clamp 前」が食い違い、その間に自分の `set_position` が無い**なら、動かしたのは自分ではない
（OS ないし tao の `WM_EXITSIZEMOVE` 後処理）と**名指せる**。差が出ないなら自分が動かしている。
`trace.rs:44` の `seq` は単一 `AtomicU64` ゆえ**全順序が無料で付く**——順序を刻む計器を別に作らない。

**問い B:「競り合い」か「フレームが疎」か。**
同じ組の `ts_ms` の**密度**が答える。移動ループ中のフレーム間隔が数十〜数百 ms なら「疎」、
1 打鍵あたり複数フレームが回っていて位置が往復しているなら「競り合い」。**どちらかを棄却する**には
打鍵の注入時刻（`Send-SnotraKey` 側のログ）と突き合わせる必要がある。

### 5.2 制約（見落とすと事故になる）

- **計装は述語の導入と同じ PR に入れる**が、**撤去条件を計装自身の doc へ書く**
  （PR 本文は merge 後に読まれない）。`heap_trace.rs` の `//!` が先例。
  **撤去条件を「#1194 が閉じたら」にしてはならない**——閉じるのがこの PR なら自己参照で
  発火しない（`scaffold-removal-condition-self-reference` と同型）。
- **生ログの置き場をリポジトリの外にする**（`AGENTS.md` 同行）。本件が刻むのは
  ウィンドウ座標とフラグだけで**個人のファイルパスは載らない**が、コミットしてよいのは
  派生表だけという規則は変わらない。
- **撤去する時**は、この計装が出した値のうち引用されうるものが `PERFORMANCE.md` へ
  出所つきで着地しているかを確かめてから消す。ただし本件の値は**性能ではなく挙動**なので、
  着地先は `PERFORMANCE.md` ではなく新 ADR ないし `clamp_main_into_work_area` の doc が適切
  （`ADR-measurement-canon-in-code-doc` の趣旨）。

---

## 6. 検証手段の導出（`AGENTS.md`「条件別チェック」/ `docs/build-commands.md`）

### 6.1 カテゴリ（`docs/build-commands.md`「変更後の検証チェックリスト」）

| カテゴリ | 該当するか | 根拠 |
|---|---|---|
| **A. Rust ファイルを変更** | **該当** | `view.rs` / `window_coordinator.rs`。PostToolUse hook が fmt/clippy/test を自動実行し**沈黙 = 合格**。ただし **`cargo doc` は手で走らせる**——doc コメントを大幅に書き換えるので intra-doc link 切れは CI でしか出ない（`.claude/rules/comments.md` の「トリガー → 検査」） |
| **B. TypeScript** | 非該当 | `.ts` を触らない |
| **C. ウィンドウ生成／表示順・ホットキー・スラッシュコマンド** | **条件つき該当** | `.claude/rules/src-tauri.md`「検証カテゴリは拡張子でなく変更が触れるコードパスの意味で決める（#558）」。**表示経路に触る**ので `smoke:egui` / `smoke:startup` を走らせる側へ倒すべき。**`scripts/lib/SnotraTraceInvariants.psm1` の不変条件は H1/H4/H5/H7 の 4 つで、いずれも `egui_results:show` / `egui_results:hide` / `egui_search:settled` を見る**（`:14-17,31` で確認）——新しい `egui_main:*` イベントを足しても既存判定は壊れないが、**既存イベント名を変えるなら壊れる**。判定は「新規追加のみか」で分かれる |
| **D. UI のスタイル・レイアウト・テキスト表示** | **該当（本件の主戦場）** | 挙動は実アプリでしか見えない。`clamp_main_into_work_area` doc `:799-805` の再測手順（release + `SNOTRA_CONFIG_DIR` の使い捨てプロファイル + `Send-SnotraKey` + `GetWindowRect`）がそのまま治具。**DPI awareness を先に確立する**（同 doc の注意）。**対照が要る**——単独の系列では頭打ちがクランプかカーソル限界かを分離できない |
| **E. git hook** | 非該当 | |
| **F. ガバナンス文書（`*.md`）** | **該当** | `SPEC.md` / `src-tauri/CLAUDE.md` / 新 ADR / `scripts/manual-smoke.ps1` の散文。`npm run governance:check`。PR では CI の `governance-check` job が常時実行 |

### 6.2 check スキル

| スキル | 該当 | 根拠 |
|---|---|---|
| `/state-check` | **該当** | 条件別チェック表「UI モード・**ガード条件**を追加/変更」。本件はガード条件の置換そのもの |
| `/symmetric-check` | **該当** | 同表「対称ペア（…enter/exit…）」。`WM_ENTERSIZEMOVE` / `WM_EXITSIZEMOVE` に対応する「ループへ入る／出る」の対称であり、**出る側でクランプが 1 度だけ走ること**が保証の実体 |
| `/race-check` | **条件つき** | 同表の述語（worker spawn・channel・drain・**Tauri listener**・スレッド/窓をまたぐ共有状態・フレーム内 live-read・paint 後の遅延処理・async）。**同一スレッドの `GetGUIThreadInfo` 読みだけなら 1 つも当たらない**。計装で `Moved` listener を足すなら**該当**（`src-tauri/CLAUDE.md`「listener を足すことは worker を足すことと同じ」） |
| `/persistence-check` | 非該当 | 永続形式・キー形式を変えない。**ただし所見 4 を参照**——位置の永続（`read_placement_relative`）が保存する値の**内容**は変わりうる |
| `/dry-check` | **該当（軽）** | 述語関数を新規定義するため（同表「関数・型を新規定義」行）。呼び出し元の列挙は LSP の `findReferences` で行う（grep は同名の別物を拾う） |
| `/plan-review`「Step 2b」 | 本レビュー自身がそれ | |

### 6.3 条件別チェック表のうち、追加で当たる行

- **「`Option` / フラグ / enum variant など**どの分岐が選ばれるかを決める値**の出所を変更」**——
  **本件そのものである**。「diff に現れない下流を 1 段辿り『この値で初めて走る行』を列挙する」が
  要求される（下の所見 3）。
- **「レビュー指摘へ修正（fix-forward）を当てた」**——指摘を出した枠組みを修正差分にも再実行する。
- **「ガバナンス文書を変更、または `.rs` のコメントの見出し参照（正準形）とその参照先を変更」**——
  doc の見出し参照を触るので `governance:check`。

---

## 7. 所見の 3 分類

### 7.1 要対処

**所見 1 — 述語が失敗したときの倒し方が挙動を決める。両方向とも既知の欠陥へ落ちる。**
`GetGUIThreadInfo` は `Result` を返す（`mod.rs:871`）。失敗時に
「移動ループ中とみなす」と倒すと**クランプが黙って死ぬ**——これは
`ADR-main-window-clamp-on-pointer-release` 却下 5 が `any_down()` の固着として警戒した
まさにその失敗形であり、しかも**検知手段が無い**。逆に「移動ループ中でないとみなす」と倒すと、
そのフレームだけ移動中に引き戻されうる（ゴムバンド）。
`clamp_main_into_work_area` の既存の作法は「取得に失敗したら**クランプしない側へ倒す**」
（`:815`）だが、それは**材料が無い**場合の話で、**述語が読めない**場合とは別の判断である。
**決め手は「その失敗が一過性か恒久的か」**——恒久的（API が常に失敗する環境）なら
クランプを殺す側は致命的なので「ループ中でない」へ倒すべきである。
**計装でこの分岐の発火回数を刻むこと**を推す。

**所見 2 — マウスドラッグ開始の隙は 2 段ある（egui の drag しきい値 + `PostMessageW` の非同期性）。**
経路の全長を実在確認した: `view.rs:551-558` が `ui.interact(…, Sense::drag())` の
**`drag_started_by(Primary)`** で `frame.drag_window()` を呼ぶ（`frame.drag_window()` の
呼び出し点は**リポジトリ全体でこの 1 行だけ**）→ `runtime.rs:38-40` が `drag_requested` を立て
→ `runtime.rs:543-545` が `start_dragging()` → tao `handle_os_dragging`
（tao 0.35.3 `window.rs:539-550`）が `WM_NCLBUTTONDOWN` を **`PostMessageW` で投げる**。
ゆえに隙は 2 段: (1) 押下から egui が**ドラッグ開始と判定するまで**（しきい値ぶんの移動が要る＝
複数フレーム）、(2) `PostMessageW` が dequeue されて `GUI_INMOVESIZE` が立つまで。
置換（AND ではない）だとこの隙のフレームでクランプが撃つ。
通常は窓がまだ動いていないので no-op だが、**#1194 の後は「作業領域の外に確定した窓」が
実在しうる**——その状態から掴んだ瞬間に内側へ跳ね、以後ユーザーは外へ戻せない、という形になる。
**これは #1194 が新しく作る組み合わせであり、既存の検査は 1 つも見ていない。**
対処案は (a) 隙のあいだ `any_down()` を OR で足す（＝実質 AND ではなく和集合の抑止条件）
(b) `drag_requested` を撃ったフレームからの猶予を持つ。**どちらも新しい状態を増やすので、
実測で隙の長さを測ってから決めるべきである。**

**所見 3 — 新しく生きる分岐: ポインタ押下中の非移動フレーム。**
`!any_down()` を落とすと、**入力欄のテキスト選択中・クリック保持中**の
フレームでクランプが**初めて走る**。これらのフレームで窓は動いていないので
ほぼ no-op だが、**1 つだけ差が出る**: 押下中に作業領域が縮んだ（タスクバー移動・
モニター構成変更）とき、従来は離すまで待ったのが**即座に引き戻る**。
`AGENTS.md`「どの分岐が選ばれるかを決める値の出所を変更」行が名指す
**「1 行も変えていないのに初めて走る下流」**（#977 と同型）である。害は無いと見込むが、
**見込みであって実測ではない**——計装でこの経路の発火を刻めば裁定できる。

**所見 4 — issue の受け入れ文言「確定後の最初のフレームで 1 度だけ戻る」が偽になりうる。**
`window_coordinator.rs:790-792` が記録した ⚠️——**確定（`Enter`）後に窓が 27〜28 px 上がる
（5/5・理由未特定）**。この動かし手が「確定後の最初のクランプフレーム」より**後**に効くなら、
ユーザーは**2 回の跳ね**を見ることになり、最終位置は「クランプ後の位置 − 27 px」＝
**作業領域の内側からさらにずれた位置**になる。**つまり保証が「1 度だけ」ではなくなる。**
issue が計装で「誰が動かしているか名指す」ことを求めているのは、まさにこの順序が
受け入れ条件の真偽を決めるからである。**述語を直すだけでは閉じない可能性がある。**
→ **これが本レビューで最も重大な 1 件である。**

### 7.2 軽微

**所見 5 — 「ドラッグ中は egui フレームが止まる」という凍結文書の前提と食い違う。**
`docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md:98` は
「ドラッグ中はネイティブ移動ループが回り egui フレームが止まる」と書く。一方
`ADR-main-window-clamp-on-pointer-release` 却下 5 の実測表は**ドラッグ中に毎フレーム
クランプが走った**ことを示し、`window_coordinator.rs:769` は移動ループ中も
「フレームは回る」と実測している。**実測が凍結文書の予測を覆している。**
凍結文書は直さないが、**#1194 の設計を「フレームが止まる」前提で組んではならない**。

**所見 6 — 述語を `read_frame_geom` / `read_bar_anchor` へ混ぜたくなる誘惑。**
どちらも「Win32 を読む唯一点」と doc で宣言している（`:643-645`）。だが**あちらの射程は
『窓の幾何』であって『入力状態』ではない**。混ぜると `read_bar_anchor` の
「クランプと hide 保存が同じ材料を通ることの担保」という設計意図（`:618-622`）が薄まる。
**別関数に分けるべきである。**

**所見 7 — 非 Windows スタブの向き。**
`clamp_main_into_work_area` の非 Windows 版（`:841`）は**何もしない**。新しい述語の
非 Windows 版は `false`（＝移動ループ中でない＝クランプする）を返すのが素直だが、
そもそも本体が no-op なので**どちらでも観測差は出ない**。`-D warnings` 下で
未使用引数の警告が出ない形（`_window`）にすること。

**所見 8 — `check_show_bar_rect` の配置理由コメントが識別子で書かれている。**
`view.rs:1285`「クランプの `!any_down()` の外に置く」。**命題（クランプが走ったかと無関係な
不変条件である）は変わらない**ので、識別子だけを差し替えれば済む。**命題ごと書き換えないこと**
（`CLAUDE.md`「意図的なリファクタリングの結果を元に戻さない」）。

### 7.3 未検証（見なかった／見られなかったもの）

1. **`GetGUIThreadInfo` / `GUI_INMOVESIZE` の実呼び出し**——コードの実在（vendored ソース）は
   確認したが、**実際に Snotra の main 窓で `SC_MOVE` 中にフラグが立つかは測っていない**。
   `window_coordinator.rs:796-797` 自身が「実呼び出しは未検証」と書いている。
   **実装計画は述語を確定する前にプローブを含めるべきである。** レビュー（本タスク）では
   実機を起動していないので測れない。
2. **フラグが降りるのと 27〜28 px の移動の前後関係**（所見 4 の核心）。順序は計装でしか出ない。
3. **キーボード移動での多モニター封鎖**——`window_coordinator.rs:787-788` が
   「多モニターでの封鎖そのものも未測定である」と明示。本レビューでも測っていない。
   単一モニター機しか手元に無いかどうかも確認していない。
4. **`GUI_INMOVESIZE` がリサイズでも立つかは測っていない**（フラグ名からすると立つはず）。
   ただし**実害は無い見込みである**——main / results とも `.resizable(false)` で作られており
   （`src-tauri/src/egui_shell/mod.rs:351` と `:369`）、ユーザーがリサイズループへ入る経路が無い。
   **これは grep による否定であって実測ではない**（`Alt+Space` メニューの「サイズ変更」が
   本当に無効かは確かめていない）。
5. **`results` 窓が同一スレッドの移動ループへ入る経路は見つからなかった**——
   `frame.drag_window()` の呼び出し点はリポジトリ全体で `view.rs:557`（main）の 1 行だけ
   （`git grep -n "drag_window()"` で実測）。ゆえに `hwndMoveSize` 照合は現時点では
   「効かない保険」だが、`SnotraPlatformWindow` を含む同一スレッドの他窓を将来足したときに効く。
   **述語をスレッド id だけで書くと、その将来に黙って壊れる。**
6. **egui 側に既存の代替手段があるか**——`ui.input(|i| …)` から「OS の移動ループ中」を
   知る術が egui 0.36 にあるかは調べていない（無いと見込むが、`viewport` 情報を確認していない）。
7. **`workspace/` 配下の既存の計画・調査・敵対レビュー**——独立導出の条件により**意図的に
   読んでいない**。既に測られている項目が上の「未検証」に含まれている可能性がある。
8. **`gh issue view 1194` の本文**——読んでいない。入力はチームリードのブリーフの
   「やること」節のみ。issue にある受け入れ条件・コメントの追加情報を見落としている可能性がある。
