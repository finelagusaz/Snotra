# research — #934 `search_state::EscapeOutcome` へ `#[must_use]` を足し、処置を返す純粋核の強制を揃える

## issue の要約

`egui_shell/` には「純粋核が処置を `enum` で返し、driver がそれを実行する」同型の契約が 2 つあり、
`lifecycle::BlurGrace::observe`（→ `BlurAction`）は `#[must_use]` を持つのに
`search_state::SearchState::on_escape`（→ `EscapeOutcome`）は持たない。どちらも「返した処置を
driver が取り落とすと副作用が黙って消える」同じ壊れ方をする。是正は「戻す」ではなく
**先例（#745 側）に足す**。あわせて同ディレクトリの他の「処置を返す純粋核」を数え上げる。

- 出所: #745 の `/simplify`（2026-08-04・再利用の観点）。別ファイルの既存コードゆえ当該 PR の射程外として見送られた
- ラベル: `type:refactor`

## 関連ファイル・モジュール・関数

すべて `src-tauri/src/egui_shell/`。

| ファイル | 関わるシンボル |
|---|---|
| `search_state.rs` | `EscapeOutcome`（104）・`SearchState::on_escape`（355）・`enter_folder`（240）・`navigate_folder`（254） |
| `lifecycle.rs` | `BlurAction`（32）・`blur_grace_action`（68）・`BlurGrace::observe`（149、`#[must_use]` は 148） |
| `layout.rs` | `Debouncer::on_input`（435）・`Debouncer::poll`（442） |
| `notify.rs` | `NoticeSlot::poll`（77）・`UpdaterUi::try_begin_install`（171）・`UpdaterUi::dismiss`（206） |
| `launcher_controller.rs` | driver 側の消費点（`on_escape_pressed` 1042 は `#[must_use]` 済・`consume_reset_pending` 917） |

## 列挙: `egui_shell/` の `&mut self` かつ非 unit 返り値（全数）

**枠組みを 2 つ通して件数を突き合わせた**（`AGENTS.md`「検索パターンは『概念』ではなく
『自分が想定した書き方』を列挙する」）。

1. 単一行前提の締まったパターン `fn \w+\(&mut self[^)]*\) -> ` → **11 件**
2. 複数行を許す緩いパターン（`multiline`）`fn \w+\s*\([^)]*&mut self[^;{]*\)\s*->` → **11 件**（同一集合）
3. 上の 2 つが原理的に落としうる形（複数行シグネチャ）を別の綴りで直接列挙: `&mut self,\s*$`
   → 3 件（`launcher_controller.rs:1103` `on_nav_keys` / `results_view.rs:175`
   `request_icons_for_results` / `search_state.rs:311` `enter_tool`）。**3 件とも返り値は `()`**
   ゆえ 1・2 の集合に入らないのが正しい

→ 3 枠が一致。母集団は下の 11 件で確定。

| # | シンボル | 返り値 | 現状 `#[must_use]` | 素で捨てる呼び出し点 |
|---|---|---|---|---|
| 1 | `lifecycle::BlurGrace::observe` | `BlurAction` | **有**（メソッド段） | 0 |
| 2 | `search_state::SearchState::on_escape` | `EscapeOutcome` | 無 | **3**（`search_state.rs` テスト 951 / 1147 / 1159） |
| 3 | `layout::Debouncer::on_input` | `bool` | 無 | **1**（`layout.rs` テスト 632） |
| 4 | `layout::Debouncer::poll` | `bool` | 無 | 0 |
| 5 | `notify::NoticeSlot::poll` | `bool` | 無 | 0 |
| 6 | `notify::UpdaterUi::try_begin_install` | `Option<U>` | 無 | 0 |
| 7 | `notify::UpdaterUi::dismiss` | `bool` | 無 | 0 |
| 8 | `launcher_controller::consume_reset_pending` | `bool` | 無 | 0 |
| 9 | `launcher_controller::on_escape_pressed` | `bool` | **有**（メソッド段・#840） | 0 |
| 10 | `search_state::SearchState::enter_folder` | `u64` | 無 | 13（`search_state.rs` テスト。10 と 11 の合算） |
| 11 | `search_state::SearchState::navigate_folder` | `u64` | 無 | ↑に含む |

素の文で捨てている呼び出し点の内訳（`grep "^\s*[a-z_]*\.<fn>()\s*;"` で確定）:

- `on_escape`: `search_state.rs:951`（`escape_invalidates_gen_so_late_nav_result_is_dropped`・
  folder 離脱 → `RestoredSearch`）/ `:1147`（`folder_results_rejected_while_tool_is_open`・
  tool 解除 → `RestoredFromTool`）/ `:1159`
  （`stale_token_rejected_after_escape_and_reenter_while_folder_is_some`・folder 離脱 → `RestoredSearch`）
- `Debouncer::on_input`: `layout.rs:632`（`cancel_disarms_pending_trailing`・
  `Debouncer::new(50ms, leading=true)` の初回ゆえ `true`）
- `enter_folder` / `navigate_folder`: 13 件（621/635/824/827/839/844/855/857/875/906/925/969/1084）

## 本 issue の実際の収穫（issue の予測とは形が違う）

### (1) `let _ =` の握り潰しは在る——ただし**先例（#745）の側に 13 件**

issue は「`#[must_use]` を付けた結果 `let _ =` で握り潰している呼び出し点が無いか」を問うたが、
**これから付ける 7 シンボルには 0 件**である（`let _ =` + 対象関数名の grep で確定）。捨てているのは
素の文 4 件（すべてテスト内）だけ。

**代わりに、既に `#[must_use]` を持つ `BlurGrace::observe` の側に `let _ =` が 13 件ある**
（`lifecycle.rs` テスト 194/195/217/218/233/250/269/279/280/297/298/317/318。`src-tauri/src` 全体で
`let _ = .*\.observe(` は 13 件で全数）。いずれも「focus を得る / blur で武装する」という
**状態セットアップ**の呼び出しであり、返り値（`focused=true` 分岐は常に `BlurAction::Idle`）は
テストの主題ではない。**#745 の先例が採った逃げ道は `let _ =` である**——ゆえに
「`#[must_use]` を付ければ落とせなくなる」と書くことはできない（機構より強い主張になる）。

なお `egui_shell/` の `let _ =` は全体で 48 件あるが、残り 35 件は `Result` を返す tauri/Win32 API
（`window.hide()` / `SetWindowPos` / `emit` / `tx.send` 等）の best-effort 破棄であり本 issue の
対象外である。

### (2) 本命の欠陥は `try_begin_install` の「取り出して遷移させた後」の無防備さ

もう一つの収穫は **`UpdaterUi::try_begin_install` が無防備であること**。この関数は
`std::mem::replace` で `phase` から `Update` を**取り出して** `Installing` へ遷移させる
（`notify.rs:175-184`）。返り値を落とすと phase は `Installing` のまま `Update` が失われ、
**install が二度とできない状態で固着する**——列挙 11 件のうち唯一「回復不能」な壊れ方であり、
`on_escape`（次の Escape で再試行できる）より重い。

## 再利用できる既存パターン

- **先例①（#745）**: `lifecycle.rs:148` の `#[must_use]`（メソッド段）。doc コメントが直上にあり、
  #745 の経緯（reset-on-show を落とした実装）がそこに固定されている
- **先例②（#840）**: `launcher_controller.rs:1041` の `#[must_use]`（driver 側・キャレット同期の信号）
- **既存の否定判断**: `snotra-egui-runtime/src/runtime.rs:111` は「**`#[must_use]` は付けない**——
  wake を要さない窓では戻り値を落とせる設計にしてある」と明記する。**別 crate かつ意図的な非対称**
  ゆえ本 issue の射程外（「揃え忘れ」ではない）
- **`Option` の性質**: `window_coordinator.rs:677` が記すとおり `Option` は型に `#[must_use]` を
  持たない（std 実測: `Result` にはあり `Option` には無い）。ゆえに #6 はメソッド段しか選べない
- **`ResultsPresentation`（`layout.rs:364`）は対象外**: `present_results` は `&mut self` を取らない
  純関数であり、落としても状態は進まない（`layout.rs:363` が `EscapeOutcome` を「不正状態を
  構築できない enum」の先例として引いているのは別の論点）

## 技術的制約（実測）

- **`unused_must_use` は rustc の warn 既定 lint**。赤くするのは `-D warnings` であり、
  それを渡すのは 2 経路: `.github/workflows/ci.yml:126`
  （`cargo clippy --workspace --all-targets -- -D warnings`）と
  `.claude/hooks/post-edit.mjs:308-312`（`clippy --workspace --all-targets --message-format short
  -- -D warnings`）。**どちらも `--all-targets` を持つ**ゆえテストコードの `unused_must_use` も
  赤くなり、PostToolUse hook がローカルで即座に会話へ届ける（＝CI まで持ち越さない）
- **`#[must_use]` は `let _ =` で黙らせられる**。`clippy::let_underscore_must_use` は pedantic で
  有効化されていない（`src-tauri/clippy.toml` は `disallowed-methods` のみ）。**この逃げ道は
  仮定ではなく実使用がある**——上の「収穫 (1)」の 13 件
- **`#[must_use]` は「見たが捨てた」を捕まえない**。#6 の危険形
  `if u.try_begin_install().is_some() { }` は「使用」と判定され通る
- 型段の `#[must_use]` は「その型を返す全関数」を覆う（将来足す関数も含む）。`bool` / `u64` /
  `Option<U>` のような外来・primitive 型には書けないためメソッド段しか選べない

## 未解決の疑問（plan.md の「未確定」へ引き継ぐ）

- 対象範囲を #2 だけに絞るか、同型の #3〜#8 まで広げるか（issue は #3〜#5 を「対象になりうる」と
  だけ書き、判断を委ねている）
- `BlurAction`（#1）の `#[must_use]` をメソッド段から型段へ移すか（一様な配置規則を得るため）
