# #745 独立再導出 — blur 猶予が hide を跨いで持ち越される

作成: 2026-08-04 / ブランチ `fix/blur-grace-reset-on-show`（main = `9ebf3db`）
方法: `workspace/plan.md` / `workspace/research.md` を読まずにコードから導出。

> **開示**: 冒頭の `Grep` が `workspace/research.md` にヒットし、tool 出力のプレビューとして冒頭 ~30 行が視界に入った（`unfocus_at` の 3 分岐表と「欠陥は `unfocus_at` のみ」の 1 行）。以降は `--include`/パス除外で workspace/ を排して導出し、当該内容は下の結論の根拠には使っていない（下の結論は全て `file:line` の一次確認による）。独立性は完全ではない旨を記録する。

---

## 1. 欠陥に関与する状態の現在地

リファクタリング（#666 段 3）で `view.rs` から**検索セッション層へ移動済み**。issue 本文の `view.rs:1299-1301` は現存しない。

| 何 | 現在地 |
|---|---|
| 型 | `LauncherController`（`src-tauri/src/egui_shell/launcher_controller.rs:102`） |
| フィールド | `was_focused: bool`（`:104`） / `unfocus_at: Option<Instant>`（`:105`） |
| 初期値 | `:139` `was_focused: false` / `:140` `unfocus_at: None` |
| 所有者 | `view.rs:78` `controller: LauncherController`（`pub(super)` 型・view の private フィールド） |
| 純粋核 | `lifecycle.rs:60 blur_grace_action` / `:85 blur_should_hide` / `:23 BLUR_GRACE` |

## 2. 読み書きの全列挙（grep 根拠）

`git grep -n -E 'unfocus_at|was_focused' -- '*.rs'` の全ヒット（`docs/superpowers/plans/` の歴史文書を除く）:

### `unfocus_at`

| file:line | 種別 | 内容 |
|---|---|---|
| `launcher_controller.rs:105` | 宣言 | `unfocus_at: Option<Instant>` |
| `launcher_controller.rs:140` | 書き（init） | `None` |
| `launcher_controller.rs:1039` | 書き（クリア） | 段 14 `clear_blur_grace_if_focused`: `if focused { self.unfocus_at = None }` |
| `launcher_controller.rs:1079` | 書き（arm） | 段 16 `on_focus_changed`: `if was_focused && !focused { Some(Instant::now()) }` |
| `launcher_controller.rs:1085` | 読み | 同 `if let Some(at) = self.unfocus_at` → `at.elapsed()` |
| `launcher_controller.rs:1092` | 書き（クリア） | `BlurAction::Hide` 分岐 |

**書き手は 4 点・読み手は 1 点。クリア経路は 2 本のみ**（`focused == true` のフレーム / 猶予明けの hide）——issue 本文の主張と一致する。

### `was_focused`

| file:line | 種別 | 内容 |
|---|---|---|
| `launcher_controller.rs:104` | 宣言 | `was_focused: bool` |
| `launcher_controller.rs:139` | 書き（init） | `false` |
| `launcher_controller.rs:1076` | 読み | 段 16 `let was_focused = self.was_focused;` |
| `launcher_controller.rs:1305` | 書き | 段 34 `set_focused(focused)`（**現在の唯一の実行時書き手**） |

### フレーム内の呼び出し順（`view.rs::update`）

| 段 | view.rs | 呼ぶもの |
|---|---|---|
| 3 | `:323` | `consume_reset_pending()` → `was_reset_frame` |
| 13 | `:419` | `read_pre_widget_input()` → `pre.focused` |
| 14 | `:421` | `clear_blur_grace_if_focused(pre.focused)` |
| 15 | `:427`(前) | `on_escape_pressed`（Escape のフレームのみ） |
| 16–17 | `:427` | `on_focus_changed(pre.focused, &ctx)` |
| 34 | `:997` | `set_focused(pre.focused)` |

**段 3 は段 14/16 より前**である（`:323` < `:421` < `:427`）。ゆえに reset ブロックでのクリアは同一フレームの blur 判定より必ず先に効く。

### `reset_pending`（backstop の駆動）

- 立てる: `window_coordinator.rs:255`（`show_egui_main` 内・`sh.reset_pending.store(true)`）
- 消費: `launcher_controller.rs:923`（`swap(false)`）→ 同ブロックで `state.reset()` / `folder_cache` / `folder_error` / `instant_rows_query` / `search_debounce` / `launching` / `notice` をクリア（`:925-940`）。**blur 猶予の 2 フィールドはこの列挙に無い**（一次確認）。

## 3. 変更すべきファイルとシンボル

| # | ファイル | シンボル | 変更 | 分類 |
|---|---|---|---|---|
| 1 | `src-tauri/src/egui_shell/launcher_controller.rs` | `consume_reset_pending`（`:917`、追加位置は `:939-940` の `launching`/`notice` クリアと同じ塊） | `self.unfocus_at = None;` と `self.was_focused = false;` を追加 | 要対処 |
| 2 | `src-tauri/src/egui_shell/launcher_controller.rs` | `set_focused` の doc（`:1302-1303`） | 「`on_focus_changed` の唯一の書き手」の**全称が偽になる**ため訂正 | 要対処 |
| 3 | `src-tauri/src/egui_shell/lifecycle.rs` | `blur_grace_action` doc（`:57-59`） | 「hide を跨いで持ち越されうる…#745 が追う」が偽になる | 要対処 |
| 4 | `src-tauri/CLAUDE.md` | 「イベント駆動 wake の不変条件」（`:36` 末尾） | 「**`unfocus_at` / `was_focused` は現在この backstop の外にいる**・#745」が偽になる | 要対処 |
| 5 | `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` | `:7`「残る追跡」・`:100`「backstop の既知の穴」 | 同上（#745 の解消を反映） | 要対処 |
| 6 | `src-tauri/src/egui_shell/launcher_controller.rs` | `on_focus_changed` doc（`:1073-1074`） | 「更新は段 34 が行う」の排他性のみ失効（後続の「この 2 段の間に書き手は無い」は真のまま） | 軽微 |
| 7 | `src-tauri/src/egui_shell/view.rs` | `:636` のコメント | 「hide→reshow で `was_focused` が stale でも確実に戻る」——主張自体は真のまま、前提（stale がありうる）が弱まる | 軽微 |

**変更しない**（根拠つき）:

- `SPEC.md` — §4.7 の `:551` は overlay 3 種（launching / 一時通知 / updater トースト）に射程が閉じており blur 猶予は overlay ではない。§8.7 の `:589`「hide を跨ぐ状態は再表示時のリセットとセットで設計する」は**すでに本修正が満たす向きの規範**であり、修正は SPEC への接近であって変更ではない（→ §6）。
- `docs/superpowers/specs/2026-07-22-su2-window-shell-design.md:83` — 「**show のたびに view 側の `was_focused`/`unfocus_at` をリセット**（再表示直後に前回の stale な猶予で即 hide しない）」と**すでに書いてある**。修正はこの記述を真にする。編集対象ではなく、**実装が設計から drift した証拠**として扱う。
- `.claude/skills/state-check/SKILL.md:27,63` / `race-check/SKILL.md:35` — 「新シグナルが `consume_reset_pending` でクリアされるか」という一般則で、本修正で偽にならない。
- `lifecycle.rs:85 blur_should_hide` 本体・純粋核テスト（`:94-128`）— 判定の入力集合は変わらない。

## 4. 見落とされやすい箇所

### 4-1. 「消える経路」と「立つ経路」の非対称

**(a) hide 側には手が入らない・入れられない（構造的理由）**

`hide_egui_main`（`window_coordinator.rs:434`）は `&tauri::AppHandle` しか持たず、`LauncherController` は `view.rs:78` の private フィールドである（型は `pub(super)`、ctor 呼び出しは `view.rs:105` の 1 点のみ・grep 実測）。**hide 経路からこの 2 フィールドを触る書き方は存在しない**——やるなら `EguiShellState` に共有 AtomicBool を足すことになる。ゆえに `reset_pending`（既存の共有 AtomicBool）を経由する **reset-on-show が唯一の実装可能な hook** であり、これは issue の案 1 と一致する。「hide 側でもクリアすべきでは」は構造上不成立。

**(b) `unfocus_at` と `was_focused` は非対称に危険である**

hide を跨いで残る組み合わせは 2 通りしかない（段 34 が毎フレーム `was_focused = pre.focused` を書くため、最後のフレームの focus がそのまま残る）:

| 持ち越し状態 | 再 show 初フレームが `focused == false` のとき |
|---|---|
| `was_focused=false`, `unfocus_at=Some(古)` （＝blur を観測したフレームで hide された） | 段 16 の再 arm 条件 `was_focused && !focused` が**偽**→ 再武装されない → `:1085` が古い `at` で `elapsed ≫ 100ms` → `Hide`。**これが issue の欠陥** |
| `was_focused=true`, `unfocus_at=None` （＝focus を持ったまま Enter/トレイ/Escape で hide） | 段 16 が `Instant::now()` で**新規に arm** → 100ms 後に `Hide` |

**片方だけ扱うと壊れる形**: `unfocus_at` だけをクリアして `was_focused` を放置すると、上表 2 行目のシナリオ（**issue 本文が書いていない方**）が残る。逆に `was_focused` だけを立てる方向（「show したのだから focused だろう」と `was_focused = true` にする）は 2 行目を**必ず**発火させるので明確に誤り。**`false` へ倒すこと**（＝ `new()` の初期値と同じ）が正しい向きである。

**(c) クリアで blur エッジを取り落とさないか**

段 34（`view.rs:997`）が同じフレームの末尾で `was_focused = pre.focused` を書き戻すため、reset フレームが `focused == true` なら次フレームには `was_focused == true` が復活し、以後の blur 検知は無傷。エッジが失われるのは **reset フレーム自身が `focused == false` のとき**だけで、そのときの「猶予 arm → 100ms 後 hide」は**出したばかりの窓を消す動作**なので、抑止こそが望ましい。どちらの変更も fail-safe 方向（窓が残る側）に倒れる。

### 4-2. この変更で偽になる散文（識別子でなく概念ラベルでも grep）

`grep -rn "745\|unfocus_at\|blur 猶予\|猶予" --include=*.md --include=*.rs`（node_modules / target / workspace を除外）の全ヒットを当たった結果:

| 場所 | 現在の記述 | 偽になる部分 |
|---|---|---|
| `src-tauri/CLAUDE.md:36` | 「**`unfocus_at` / `was_focused` は現在この backstop の外にいる**・#745」 | 全体 |
| `lifecycle.rs:57-59` | 「`Idle` を返したフレームで `unfocus_at` がクリアされないこと（＝猶予が armed のまま残り、**hide を跨いで持ち越されうる**こと）は別の未解決事項であり #745 が追う」 | 括弧内の後半と最終節。**前半（`Idle` フレームでクリアされないこと自体）は可視セッション内では真のまま**なので、丸ごと消さず「hide を跨ぐ持ち越しは reset-on-show が塞ぐ」へ書き換える |
| `launcher_controller.rs:1302-1303` | 「段 34: …（`on_focus_changed` の**唯一の書き手**）」 | **最も見落としやすい全称**。`consume_reset_pending` が 2 人目の `was_focused` 書き手になる。`AGENTS.md`「検証の作法」の全称表現の規律に直接当たる |
| `launcher_controller.rs:1073-1074` | 「更新は段 34（`set_focused`）が行う——この 2 段の間に書き手は無い」 | 前半の排他性のみ。後半は段 3 < 段 14 < 段 16 ゆえ真のまま（軽微） |
| `frame-scheduling-contract-design.md:7` | 「**残る追跡**: #745（`unfocus_at` が契約④の backstop の外にいる）」 | 全体。同 `:7` に「#746 は解消済み——項を撤去し」とある通り、**この文書は解消時に保守する前例がある** |
| `frame-scheduling-contract-design.md:100` | 「**backstop の既知の穴**: `unfocus_at` / `was_focused` … backstop の外にいる（#745）」 | 全体 |
| `view.rs:636` | 「was_focused に依存しないので、hide→reshow で was_focused が stale でも確実に戻る」 | 主張は真のまま・前提が弱まるだけ（軽微） |
| `su2-window-shell-design.md:83` | 「show のたびに `was_focused`/`unfocus_at` をリセット」 | **偽にならない——真になる**（drift の証拠・編集不要） |

### 4-3. 検証手段の非対称（issue の受け入れ条件に直結）

- `launcher_controller.rs` に **`#[cfg(test)]` モジュールが 1 つも無い**（grep 0 件）。`LauncherController::new` は `tauri::AppHandle` を要求し、ctor 呼び出し点は `view.rs:105` のみ。**view 層のクリア経路はユニットテストで固定できない**。
- 純粋核側（`blur_grace_action` / `blur_should_hide`）は**入力集合が変わらない**ので、`lifecycle.rs:94-128` の既存 4 テストで足りる。**新しい純粋核テストを足す余地は無い**——「猶予の判定」は既に固定済みで、今回変わるのは view 層の状態遷移だけ。
- ゆえに issue の受け入れ条件「純粋核で固定できる部分と view 層の状態遷移を区別して検証する」への回答は「**純粋核は変更なし・既存テストで据え置き / view 層は実機確認**」になる。実機の代替として `scripts/smoke-egui.ps1` の trace（`egui_show:done` と hide 系イベントの間隔）で「show 直後 100ms 以内の hide が無いこと」を見る形は取りうるが、**現状そのような不変条件は `SnotraTraceInvariants.psm1` に無い**（新設するかは別判断）。

### 4-4. その他

- `mod.rs:62-65` は `blur_should_hide` を re-export しない設計（#711）。今回の変更は `lifecycle.rs` の可視性に触れない。
- `consume_reset_pending` の返り値 `bool` は `view.rs:323/915/969` が使う。フィールドを 2 行足すだけなら返り値契約は不変。
- 順序不変条件（`launcher_controller.rs:952/966/997` の「reset_pending 消費より後」）は追加行が同ブロック内なので影響なし。

## 5. 挙動は変わるか — 変わる。2 シナリオ

**issue 本文が挙げるのは A のみ。B は `was_focused` クリアが引き起こす別シナリオで、issue は `was_focused` のクリアを「整合」と説明しており、B を根拠として持っていない。**

### A（issue の失敗経路・`unfocus_at` クリアが閉じる）
1. auto_hide=true。ユーザーが別ウィンドウをクリック → 段 16 が `unfocus_at = Some(T)`、段 34 が `was_focused = false`
2. 100ms 未満のうちにホットキートグル / Escape / トレイで hide（`plan_hotkey` → `HideNow`）
3. 後で再 show。`window.set_focus()` は `window_coordinator.rs:341` で `let _ =` により**失敗が握り潰される**
4. 初フレームが `focused == false` で走ると（config-applied 等の別 wake が先着した場合を含む）、`blur_grace_action(T.elapsed() ≫ 100ms, false, true)` → `Hide` → **表示直後に自動 hide**

修正後: 段 3 が `unfocus_at = None` にするので `:1085` の `if let Some` に入らない。

### B（`was_focused` クリアが閉じる・issue 未記載）
1. focus を持ったまま hide（Enter で起動成功 / トレイ / ホットキートグル）→ `was_focused = true`, `unfocus_at = None` が残る
2. 再 show で `set_focus()` が着地しない → 初フレーム `focused == false`
3. 段 16 の `was_focused && !focused` が真 → **新規に猶予を arm** → 100ms 後のフレームで `Hide`

修正後: `was_focused = false` により arm されない。

### 変わらないもの
- 可視中の通常の blur → 100ms → hide（`was_focused` は前フレームで true に戻っている）
- auto_hide=false のとき（`blur_should_hide` の第 3 項）
- 同じ show の 2 フレーム目以降（reset は 1 フレームだけ）
- 純粋核の判定（入力も出力も不変）

**両シナリオとも fail-safe 方向**（窓が消えなくなる）であり、「窓が残るべきときに消える」の逆は生まれない。

## 6. `SPEC.md` の更新は不要（根拠つき）

`AGENTS.md` 開発ワークフロー 1 の「バグか仕様変更か」は**バグ**に落ちる:

1. `SPEC.md:589`（§8.7）が既に「**非表示中はフレームが走らない**。時限処理…は可視中しか進まないため、**hide を跨ぐ状態は再表示時のリセットとセットで設計する**」と定めている。現行コードはこの規範に**違反している側**であり、修正は SPEC への適合である。
2. `docs/superpowers/specs/2026-07-22-su2-window-shell-design.md:83` が SU2 設計時点で「show のたびに `was_focused`/`unfocus_at` をリセット」を明示していた。**実装が設計から落とした drift** であって、意図の変更ではない。
3. SPEC が blur について述べる 2 か所（`:433`「フォーカス喪失時の自動非表示（設定で切替・100ms 猶予付き）」、`:585` の状態遷移表）は、猶予の**発火条件**を述べており、猶予の**寿命**には言及していない。修正で偽にならない。
4. `SPEC.md:551` の「表示時リセットで launching と一時通知はクリアされ」は §4.7 の **overlay 3 種**についての記述で、blur 猶予は overlay ではない。列挙の追加は不要。

ただし `AGENTS.md`「『fix』でも文書化された挙動を変えたら仕様変更」の但し書きに従い、**§5 の A/B は「SPEC 記載のフロー・状態遷移の変更」に当たらないこと**を明記しておく——A も B も `SearchVisible --> Standby: focus_lost [auto_hide_on_focus_lost]`（`:585` / §8.6）の**成立条件を変えない**。変わるのは「前回の可視期間で観測した focus_lost が次の可視期間へ漏れるか」であり、SPEC はそれを漏れない前提で書いている（`:589`）。

---

## 分類まとめ

### 要対処
1. `launcher_controller.rs:917 consume_reset_pending` に `unfocus_at = None` / `was_focused = false` を追加（`:939-940` の隣）
2. `launcher_controller.rs:1302-1303` `set_focused` doc の全称「唯一の書き手」を訂正 ← **最も落としやすい**
3. `lifecycle.rs:57-59` `blur_grace_action` doc の #745 追跡記述を書き換え（前半は残す）
4. `src-tauri/CLAUDE.md:36` の「backstop の外にいる・#745」を削除／更新
5. `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md:7, :100` の #745 記述を解消済みへ（#746 と同じ保守前例）
6. `was_focused` も併せてクリアすること（§4-1(b) のシナリオ B。`unfocus_at` だけでは残る）

### 軽微
- `launcher_controller.rs:1073-1074`「更新は段 34 が行う」の排他性（後続文は真のまま）
- `view.rs:636` のコメント（主張は真・前提が弱まる）
- 純粋核テストの追加は不要（入力集合不変・既存 4 テストで据え置き）
- `su2-window-shell-design.md:83` は編集不要（修正で真になる）

### 未検証
- **`window.set_focus()` が実際に失敗する頻度**。`window_coordinator.rs:341` が `let _ =` で握り潰す事実と、`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」の「`SetForegroundWindow` は部分的に非同期」は一次確認したが、**失敗の実観測は無い**。A も B も**コード上の非整合であって実機で観測されていない**（issue 本文も「実機未観測」と明記）。
- **show 後の初フレームを何が起こすか**。`egui-window-ownership...design.md` §2.5（`:47`）が「`Focused(true)` が唯一の起動源」と記録するが、これは 2026-07-25 時点の記述で、その後の #880 サイクルで `show_egui_main` から `SendMessageTimeoutW` が撤去されている。**再確認していない**。
- **実機での blur 猶予確認**（issue の受け入れ条件）。view 層はユニットテスト不能（§4-3）ゆえ `docs/build-commands.md` カテゴリ C/D が唯一の検証手段。
- **smoke の trace 不変条件**（「show 直後 100ms 以内に hide が来ない」）を新設できるか。`SnotraTraceInvariants.psm1` の既存 H 系を読んでいない。
