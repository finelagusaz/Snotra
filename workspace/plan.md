# plan — #934 処置を返す純粋核の `#[must_use]` を揃える

## 目的

`egui_shell/` の「純粋核が処置を返し driver が実行する」契約で、**返り値を落とすと副作用が
黙って消える**関数群の強制を `#[must_use]`（＝階梯の「コンパイルで検出」段）へ揃える。
是正の方向は #745 側（`BlurGrace::observe`）が正しく、**先例に足す**。

## 受け入れ条件

1. 下表の「対象 ○」の全行に `#[must_use = "<失うものの名前>"]` が付いている
2. 素の文で戻り値を捨てているテスト 4 件が **assert へ変わっている**（`let _ =` を新設しない）
3. `cargo clippy --workspace --all-targets -- -D warnings` が緑
4. **故障注入で赤を実測している**——production 側へ素の drop を注入し、診断に現れることを確認して
   revert（**実行では 3 形ではなく 8 シンボル全数へ広げた**。理由は Phase 4）
5. 規則と配置規約が `src-tauri/CLAUDE.md` の `egui_shell/` 節に 1 行で在る
6. **主張を機構より強く書いていない**——受容残余が文書に在る（**計画時 2 件 → 実行で 4 件**:
   型段の射程と `double_must_use` の限界が測定とレビューで加わった）

## 対象の全数と在否の判断

母集団は `workspace/research.md`「列挙」の 11 件（3 枠一致で確定）。

| # | シンボル | 返り値 | 対象 | 配置 | 判断の理由 |
|---|---|---|---|---|---|
| 2 | `search_state::SearchState::on_escape` | `EscapeOutcome` | ○ | **型段** | issue 本命。型を本 crate が所有する |
| 3 | `layout::Debouncer::on_input` | `bool` | ○ | メソッド段 | `bool` に型段は書けない |
| 4 | `layout::Debouncer::poll` | `bool` | ○ | メソッド段 | 同上 |
| 5 | `notify::NoticeSlot::poll` | `bool` | ○ | メソッド段 | 同上 |
| 6 | `notify::UpdaterUi::try_begin_install` | `Option<U>` | ○ | メソッド段 | `Option` は型に `#[must_use]` を持たない |
| 7 | `notify::UpdaterUi::dismiss` | `bool` | ○ | メソッド段 | 同上 |
| 8 | `launcher_controller::consume_reset_pending` | `bool` | ○ | メソッド段 | driver 側だが**doc が既に「返り値を落としてはならない」と散文で命じている**（`:914`）＝階梯の最下段に在る規則を 1 段上げる |
| 1 | `lifecycle::BlurGrace::observe` | `BlurAction` | ○ | **型段へ移設** | 既に有（メソッド段）。型段へ移すと free fn `blur_grace_action` も覆い、配置規約が一様になる |
| 9 | `launcher_controller::on_escape_pressed` | `bool` | — | 既済（#840） | 変更しない |
| 10 | `search_state::SearchState::enter_folder` | `u64` | ✕ | — | **production で token を消費するのは `spawn_folder_load` だけ**で、落とせば spawn の呼び忘れとして同じ数行の中で目に見える。（**計画は「落として害が出る形が構築できない」と書いたが偽**——`self.state.enter_folder(dir);` は素の文でコンパイルが通る。型が塞ぐのは「token 無しに load を呼ぶこと」だけで「load を呼び忘れること」ではない。レビュー L5 で訂正） |
| 11 | `search_state::SearchState::navigate_folder` | `u64` | ✕ | — | 同上 |

**在否の判断基準は「処置かデータか」ではなく「下流が値を構造的に要求するか」である。**
#10/#11 はテストで 13 件が素で捨てているが、それは規則の**帰結**であって除外の理由ではない
（除外の理由は上記の型強制）。

**母集団を `&mut self` に絞る理由**（issue の文言「処置を返す純粋核」より狭いのは意図である）:
状態を進めずに導くもの——自由関数 `lifecycle::plan_hotkey → HotkeyPlan`・`notify::overlay_kind`・
`layout::present_results`・`lifecycle::blur_should_hide`・`search_state::interpret` と、`&self`
メソッド（`interp` は `QueryIntent` の導出・`accept_folder_result` は token の照合述語。**計画は
`interp` を「自由関数」と誤って分類していた**——実体は `&self`、自由関数は `interpret`。レビュー
L4 / round 2 L3 で 2 度訂正）——は名前の上では処置を返すが、**同じ入力で呼び直せる**ので
失われるものが無い。**状態を進めてから返すものだけが「落とすと
回復できない」。** なお `lifecycle::blur_grace_action`（自由関数だが `BlurAction` を返す）は
#1 の型段移設で**付随的に覆われる**——型段を選ぶことが射程を買う唯一の場所である。

### 配置規約（この PR で確定させる規則）

> **型段（`enum` / `struct` 宣言）に付けるのは本 crate が型を所有するときだけ。
> `bool` / `u64` / `Option<T>` のような primitive・外来型を返すものはメソッド段に付ける。**

## 変更ファイル一覧と対象シンボル

| ファイル | シンボル（行は現状値・grep で再確認して編集する） |
|---|---|
| `src-tauri/src/egui_shell/search_state.rs` | `EscapeOutcome`（104）へ型段。テスト 951 / 1147 / 1159 を assert 化 |
| `src-tauri/src/egui_shell/layout.rs` | `Debouncer::on_input`（435）・`Debouncer::poll`（442）。テスト 632 を assert 化 |
| `src-tauri/src/egui_shell/notify.rs` | `NoticeSlot::poll`（77）・`UpdaterUi::try_begin_install`（171）・`UpdaterUi::dismiss`（206） |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `consume_reset_pending`（917）＋ doc（914-916）の更新 |
| `src-tauri/src/egui_shell/lifecycle.rs` | `BlurAction`（32）へ型段・`observe`（148）のメソッド段を削除 |
| `src-tauri/CLAUDE.md` | 「モジュール構成」の `egui_shell/` 節へ横断不変条件 1 行 |

## 付ける診断文（失うものを名指しする）

- `EscapeOutcome`（型段）: `driver が処置を実行しないと hide 要求・repaint 予約・ビュー復元が黙って消える（#745 の同型・#934）`
- `Debouncer::on_input`: `leading 発火の信号を落とすとバースト先頭の検索が飛ぶ`
- `Debouncer::poll`: `trailing 発火の信号を落とすと、既に disarm 済ゆえ検索が二度と来ない`
- `NoticeSlot::poll`: `repaint 要の信号を落とすと表示が stale のまま次の無関係な入力まで残る（src-tauri/CLAUDE.md「イベント駆動 wake の不変条件」）`
- `UpdaterUi::try_begin_install`: `Update を取り出し phase を Installing にした後ゆえ、落とすと install 不能で固着する`
- `UpdaterUi::dismiss`: `false（Installing 中の拒否）を区別しないと repaint が落ち、旧 toast が次の無関係な入力まで残る`
- `consume_reset_pending`: `view が reset フレームを知る手段はこの返り値だけである（#749 の位置不変条件）`
- `BlurAction`（型段）: `driver が処置を実行しないと hide 要求・repaint 再予約が黙って消える（#745）`

## 実装順序（各フェーズの終端で木が緑であること）

**テストの assert 化を属性より先に置く。** 逆順だと Phase 間で clippy が赤くなり、PostToolUse
hook の「沈黙 = 合格」の契約が一時的に使えなくなる（hook は `--all-targets -D warnings` を
渡すのでテストコードの `unused_must_use` も赤くする・実測）。

1. **Phase 1** — 素で捨てている 4 件を assert へ（属性なしでも通る＝木は緑のまま）
2. **Phase 2** — 純粋核 6 シンボルへ属性（#2 型段・#3〜#7 メソッド段）
3. **Phase 3** — driver 側 #8 と、#1 の型段への移設
4. **Phase 4** — 故障注入で赤を実測して revert
5. **Phase 5** — `src-tauri/CLAUDE.md` へ規則 1 行
6. **Phase 6** — カテゴリ A + `cargo doc` + `governance:check`

## 不変条件と異常系

### 壊してはならない不変条件

- **挙動は 1 バイトも変わらない**——`#[must_use]` は lint 属性であり生成コードに現れない。
  assert 化も既に成立している値を測るだけで production 経路を触らない
- **`launcher_controller.rs:1045` の `match self.state.on_escape()` は消費点として正しい**——
  型段の属性は match 対象を「使用」と見るので影響しない
- **#10/#11 に属性を付けない**——付けると `search_state.rs` のテスト 13 件（token を素で捨てる
  箇所。受容残余 1 の `let _ = g.observe` 13 件とは**別物で、件数が偶然一致している**）が
  `let _ =` を要求し、issue が警戒した「握り潰し」を自ら 13 件生む

### 受容する残余（**文書に明記する。書かないと機構より強い主張になる**）

1. **`#[must_use]` は `let _ =` で黙らせられる。この逃げ道は仮定ではなく実使用がある**——
   既に属性を持つ `BlurGrace::observe` の呼び出し点で **13 件**（`lifecycle.rs` テストの状態
   セットアップ・実測）。ゆえに「落とせなくなった」とは書けない（`AGENTS.md`「全称表現は
   前提条件とセットで書く」）。`clippy::let_underscore_must_use` は pedantic で有効化していない
   （`src-tauri/clippy.toml` は `disallowed-methods` のみ）が、**足せばその 13 件の正当な
   セットアップを誤爆させる**ので足さない（YAGNI・`docs/development-principles.md`
   「構造的設計原則と強制の階梯」の「やりすぎの境界」）
2. **`#[must_use]` は「見たが捨てた」を捕まえない。** #6 の危険形
   `if u.try_begin_install().is_some() { }` は「使用」と判定され通り、`Update` は失われる。
   この属性が捕まえるのは「一度も見なかった」だけである
3. **（実行中に測定で追加）型段はメソッド段より射程が狭い。** `&T`（参照）・`impl Trait`（不透明型）・
   ジェネリック包み（`Option<T>` / `Vec<T>` 等）では発火しない——**覆われる例外は `Box<T>` / tuple /
   配列の 3 つだけ**（rustc がそこだけ再帰する。`Result<T, _>` が発火するのは `Result` 自身が
   `#[must_use]` だからで中身のおかげではない）。probe crate で 2 度実測した。**危ういのは
   「新しい関数を足す」より「既存関数の返り値型をそう編集する」経路である**——メソッド段なら
   属性が関数に付いて追随するが、型段は編集の瞬間にガードだけが黙って消える
4. **（実行中にレビューで追加）`clippy::double_must_use` を当てにできない。** 移設で古いメソッド段を
   消し忘れる誤りに当たることはあるが、捕まえるのは**メッセージ無しの残存だけ**である（診断文自身が
   `with no message` と名乗る）。**本 PR が確立した `#[must_use = "…"]` 規約どおりの残存は沈黙する。**
   deny の出所も `[workspace.lints.clippy]`（`disallowed_methods` の 1 行のみ）ではなく
   `ci.yml` と post-edit hook が渡す `-D warnings` である。**移設のときは目で確かめる**

### 却下した代替案（1 行ずつ）

- `#![warn(unused_results)]`: `HashMap::insert` / `mem::replace` 等の正当な破棄で大量に発火する
- `governance:check` へ G-ルールを新設: 検出器は clippy が既に持つ（**新設前に発火しうるかを
  測る**・#930 の判断と同型）
- ADR を起こす: 否定の知識は上の 3 行に収まり、単独の文書を要しない

## テスト方針と検証コマンド

### 新規テストは書かない（根拠つき）

`#[must_use]` の効きを測るのはユニットテストではなく**コンパイル自体**である。ゆえに検証は
「故障注入で clippy が赤くなること」であり、テストターゲットには表現できない。
assert 化した 4 件は既存テストの強化であって新規追加ではない。

### 故障注入（production 側・4 つの**配置**すべて）

**production へ入れる**——本来の回帰の姿は「driver が消費を忘れる」であってテスト側の drop では
ない（`docs/development-principles.md`「構造的設計原則と強制の階梯」の注入の強さ）。

**4 件に分ける理由は lint 機構の違いではない**——4 件とも発火するのは同じ `unused_must_use`
である。測っているのは**属性を書いた場所が効いているか**（項目を間違えた・型段が想定どおり
射程を持たない・移設で落ちた）であり、配置が 4 通りあるから 4 件要る。
`launcher_controller.rs` へ次の 4 行を素の文として一時的に足す:

| 注入 | 場所 | 測る配置 |
|---|---|---|
| `self.state.on_escape();` | `on_escape_pressed` の `match` の直前 | 型段（`EscapeOutcome`） |
| `self.notice.poll(self.notice_base.elapsed());` | `:1005` の `if` の直前 | メソッド段 `bool` |
| `st.0.lock().unwrap().try_begin_install();` | `:866` の `let taken` の直前 | メソッド段 `Option<U>` |
| `self.blur_grace.observe(focused, Instant::now(), auto_hide);` | `on_focus_changed` の `match` の直前 | **型段への移設**（#1） |

**4 行目が最も重要である。** Phase 3 の `BlurAction` 移設は、本 PR で唯一
**既にある強制を削除する**変更（`observe` のメソッド段）であり、型段が効かなければ #745 の
ガードを黙って弱めたまま「強めた」と主張することになる（`AGENTS.md`「差分が消した行の
不変条件を名指しし、再確立地点を探す」）。

```bash
cargo clippy --workspace --all-targets -- -D warnings   # 期待: exit != 0 かつ 4 診断すべてが現れる
```

- **4 件同時に注入し、診断が 4 件そろうことを見る。** 足りないなら早期打ち切りを疑い
  1 件ずつに分けて測り直す
- 測ったら **`git checkout -- src-tauri/src/egui_shell/launcher_controller.rs` で戻し、
  clippy が緑に復すことを確認する**（注入の残留を防ぐ）
- これは稼働中のガードを弱める操作ではない（違反を足して拒否させる＝ガードの**行使**・
  `.claude/rules/safety-nets.md`）

### 検証コマンド（`docs/build-commands.md` が SSOT）

```bash
cargo fmt --all -- --check                                  # カテゴリ A
cargo check --workspace                                     # カテゴリ A
cargo clippy --workspace --all-targets -- -D warnings       # カテゴリ A（本 PR の主検証）
cargo test -p snotra                                        # カテゴリ A（src-tauri 変更）
cargo doc --workspace --no-deps --document-private-items    # カテゴリ A（doc コメントを触るため。hook 非発火）
npm run governance:check                                    # カテゴリ F（src-tauri/CLAUDE.md 変更）
```

- カテゴリ C（`smoke:startup` / `smoke:egui`）は**不要**: ウィンドウ生成・表示順・ホットキー・
  スラッシュコマンドのコードパスを触らない（属性追加とテストの assert 化のみ）
- **`governance:check` のベースラインは実測済み**（全検査 passed・常時ロード 13596/15500 字）。
  `src-tauri/CLAUDE.md` は `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]` に含まれない
  （`scripts/governance-check.mjs:1052`・モジュール CLAUDE.md は対象外と `:1047` が明記）ため
  面積 ratchet には当たらない。当たるのは見出し参照の着地（G-heading-refs）である

## `SPEC.md`・関連文書の更新要否

| 文書 | 要否 | 根拠 |
|---|---|---|
| `SPEC.md` | **不要** | `#[must_use]` はコンパイル時 lint で実行時挙動を持たない。assert 化した 4 件は Escape ラダーの**既定の**帰結（`RestoredSearch` / `RestoredFromTool`）を測るだけで、SPEC のフロー・状態遷移を一切変えない。「fix でも文書化された挙動を変えたら仕様変更」（`AGENTS.md`）の条件に当たらない |
| `src-tauri/CLAUDE.md` | **要** | `egui_shell/` 節へ横断不変条件 1 行（規則・配置規約・#10/#11 の除外理由・受容残余 2 件） |
| `docs/development-principles.md` | 不要 | 「強制の階梯」は既に `#[must_use]` を名指ししている。本 PR はその適用であって原則の追加ではない |
| `AGENTS.md` / ルート `CLAUDE.md` | 不要 | モジュール固有の不変条件であり、常時ロード面に置く対象ではない（同じ判断が `:1047` の設計意図） |
| ADR | 不要 | 上の「却下した代替案」3 行に収まる |

## 作業項目

### Phase 1 — 素で捨てている 4 件を assert へ

**`let _ =` ではなく assert を選ぶ理由**（先例の `lifecycle.rs` は `let _ = g.observe(…)` を 13 件
使っており、写せば `let _ =` になる）: `observe(true, …)` は `focused` 分岐ゆえ常に
`BlurAction::Idle` で、返り値はテストの主題ではない。対して `on_escape` / `on_input` の返り値は
**当のテストが依存している前提そのもの**（「folder を離脱した」「leading が発火した」）であり、
assert にすると前提が固定される。**判定の分かれ目は返り値がテストの前提かどうかである。**

- [x] `search_state.rs:951` を `assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);` へ（コメントは維持）
- [x] `search_state.rs:1147` を `assert_eq!(s.on_escape(), EscapeOutcome::RestoredFromTool);` へ
- [x] `search_state.rs:1159` を `assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);` へ
- [x] `layout.rs:632` を `assert!(d.on_input(), "leading 有効の初回はバースト先頭");` へ
- [x] `cargo test -p snotra` が緑 → **218 passed / 0 failed**（4 件の期待値は実際の返り値と一致）

### Phase 2 — 純粋核へ属性

- [x] `search_state.rs` の `EscapeOutcome` 宣言へ型段の `#[must_use = "…"]`
- [x] `layout.rs` の `Debouncer::on_input` / `Debouncer::poll` へメソッド段
- [x] `notify.rs` の `NoticeSlot::poll` / `UpdaterUi::try_begin_install` / `UpdaterUi::dismiss` へメソッド段

### Phase 3 — driver 側と先例の配置統一

- [x] `launcher_controller.rs` の `consume_reset_pending` へメソッド段。doc の
      「返り値を落としてはならない」を「機構（`#[must_use]`）が守る」旨へ改めた。**#749 の理由
      （`ResultsWindow::reset_size_guard()` は view 側に残るので view が reset フレームを知る手段は
      この返り値だけ）は逐語で残した**
- [x] `lifecycle.rs` の `BlurAction` 宣言へ型段を足し、`observe` のメソッド段を削除。
      **#745 の doc コメントは動かしていない**
- [x] **計画外の発見**: バッチ編集の中間状態（型段を足した直後・メソッド段を消す前）で
      `clippy::double_must_use` が exit 101 になった。**当初これを「移設の消し忘れを捕まえる機構」と
      書いたが、レビュー H1/H2 で 2 点とも過大と判明して撤回した**（下の「レビュー結果」）

### Phase 4 — 故障注入

- [x] **4 配置ではなく 8 シンボル全数へ注入した**（計画からの拡大）。`.claude/rules/safety-nets.md`
      「検出器のカバー範囲は、欠落のパターンごとに検算する」＝足ごとに壊す。**属性を書いた項目が
      正しいことは書き写しの一致では測れない**（隣の関数に付いていても同じ文字列に見える）。
      各行へ `// FAULT-INJECTION-934` のマーカーを付けた
- [x] `cargo clippy --workspace --all-targets -- -D warnings` が **exit 101**・診断 **8/8**:

      launcher_controller.rs:861:17  UpdaterUi::<U>::dismiss
      launcher_controller.rs:867:17  UpdaterUi::<U>::try_begin_install
      launcher_controller.rs:1009:9  NoticeSlot::poll
      launcher_controller.rs:1050:9  EscapeOutcome                       ← 型段
      launcher_controller.rs:1083:9  BlurAction                          ← 型段へ移設（最重要）
      launcher_controller.rs:1218:21 Debouncer::on_input
      launcher_controller.rs:1247:9  Debouncer::poll
      view.rs:485:9                  LauncherController::consume_reset_pending

      診断はいずれも**こちらが書いたメッセージを表示した**（＝メッセージとシンボルの取り違えも
      同時に検査できた）。**rustc 自身の修正提案が `use let _ = ...` である**——受容残余 1 は
      コンパイラが能動的に案内する逃げ道であり、仮定ではない
- [x] マーカー行を削除して撤去 → **残存 0 件**（`grep -rn "FAULT-INJECTION" --include=*.rs` が空）・
      clippy **exit 0** に復帰・`view.rs` は `git diff` が空（注入前と完全に同一）
- [x] **計画の撤去手順に欠陥があったので直した。** 計画は
      `git checkout -- …/launcher_controller.rs`（初版）→ `git diff --stat` が空（改訂版）と
      書いていたが**どちらも誤り**である: 前者は同ファイルに同居する**正当な変更**
      （`consume_reset_pending` の属性と doc）を巻き戻す（実行していれば Phase 3 の半分が黙って
      消えていた）。後者は「他のすべてがコミット済み」のときしか空にならず、注入と正当な変更を
      区別しない。**正しい判定は注入固有のマーカーの不在である**

### Phase 5 — 文書

- [x] `src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 節（`mod.rs` の索引行より後・
      「外部から窓を起こす経路は…」より前）へ横断不変条件 1 行を追加。太字リードは
      **「処置を返す純粋核の強制（#934）」**（`.rs` から正準形で 2 件が参照する着地点）
- [x] **8 件の名前一覧は書かなかった**（計画からの判断）——検知器の無い写しは 9 件目で黙って腐る。
      規則と判定基準（`&mut self` かつ非 unit）と、名指しが要る箇所（型段の 2 件・除外の 2 種）
      だけを書いた。`.rs` 側の doc は「そのシンボルで何が失われるか」だけを持つ
- [x] 受容残余は**4 件**（`let _ =` で黙る・「見たが捨てた」形・**型段の射程**・
      **`double_must_use` を当てにしない**）。**「落とせなくなった」と書いていない**

### Phase 6 — 検証

- [x] `cargo fmt --all -- --check` / `cargo check --workspace` /
      `cargo clippy --workspace --all-targets -- -D warnings` → 全件 exit 0
- [x] `cargo test -p snotra` → **218 passed / 0 failed / 4 ignored**
- [x] `cargo doc --workspace --no-deps --document-private-items`（doc コメントを触ったため・hook 非発火）。
      **沈黙を合格と読む前に `target/doc/snotra/` に `notify` のページが在るかを見る**——`snotra` は
      `[lib]` を持たない bin crate ゆえ、bin ターゲットの private item が実際に文書化されるかは
      測るまで分からない。→ **exit 0 かつ生成を確認**（`egui_shell/notify/` が在り、編集した 6 シンボルの
      ページ `struct.UpdaterUi.html` / `struct.NoticeSlot.html` / `struct.Debouncer.html` /
      `enum.EscapeOutcome.html` / `enum.BlurAction.html` / `struct.LauncherController.html` が全数存在）。
      **懸念は解消**——intra-doc link 検査は編集したコメントを実際に見ている
- [x] `npm run governance:check` → 全 **19 検査 passed**。**見出し参照が 152 → 155 件**
      （`.rs` へ書いた正準形 3 件が母集団に入り着地。`lifecycle.rs:35` / `search_state.rs:106` が
      「処置を返す純粋核の強制」へ、`notify.rs:77` が「イベント駆動 wake の不変条件」へ）
- [x] 実装差分を確定させた → 変更は 6 ファイル（`src-tauri/CLAUDE.md` + `egui_shell/` 5 ファイル）。
      `grep -rn "FAULT-INJECTION\|PROBE-" src-tauri/src/` が **0 件**
- [x] **fix-forward の再検証**（`AGENTS.md`「レビュー指摘へ修正を当てた」）: 指摘 18 件を当てた後、
      カテゴリ A・F を全件再実行して緑を確認し、**指摘を出した枠（code-reviewer）へ修正差分を
      再投入した**（round 2 = Critical / High / Medium いずれも 0）
- [x] **この plan.md 自身を一度失った**（下の「事故」節）。実行記録は会話から再構成したもので、
      測定値はすべて当時の実出力である

## 未確定（実装前に潰す）

- [x] **`BlurAction` を型段へ移して壊れる呼び出し点があるか** — `blur_grace_action` / `BlurAction`
      を `src-tauri/src` 全体で grep（**41** ヒット）。**素の文で捨てている箇所は 0 件**
      （`^\s*(blur_grace_action|.*\.observe)\([^;]*\)\s*;\s*$` が 0 ヒット）。残りは `assert_eq!` の
      引数・match arm の末尾式・`let action =` 束縛・`use` 文と、**`let _ = g.observe(…)` の 13 件**
      （既に属性を黙らせている側）。型段へ移しても 13 件は同じく黙るだけで挙動は変わらない。
      ゆえに移設は安全（#1 を対象 ○ とした根拠）
- [x] **列挙の母集団が閉じているか** — 3 枠（締まったパターン / `multiline` の緩いパターン /
      複数行シグネチャを別綴りで直接列挙）で件数が一致（11 / 11 / 追加 3 件はすべて返り値 `()`）。
      詳細は `workspace/research.md`「列挙」
- [x] **PostToolUse hook がテストコードの `unused_must_use` を見るか** — `.claude/hooks/post-edit.mjs:308-312`
      が `clippy --workspace --all-targets --message-format short -- -D warnings` を渡すことを実測。
      **見る**ゆえ Phase 1（assert 化）を Phase 2（属性）より先に置く順序が要る
- [x] **`src-tauri/CLAUDE.md` の追記が面積 ratchet に当たるか** — `ALWAYS_LOADED_FILES` はルート
      `CLAUDE.md` と `AGENTS.md` の 2 件のみ（`scripts/governance-check.mjs:1052`）。**当たらない**
- [x] **`「イベント駆動 wake の不変条件」` を診断文から引いて G-heading-refs が着地するか** —
      `src-tauri/CLAUDE.md:49` に `- **イベント駆動 wake の不変条件（#532 SU5）**:` として実在。
      同じ参照形が既に妥当な先例として `governance-check.mjs:1164` に記録されている
- [x] **issue が予測した「`let _ =` の握り潰し」が在るか** — **これから属性を足す 7 シンボルには
      0 件**（`let _ =` + 対象関数名の grep）。捨てているのは素の文 4 件（すべてテスト内）。
      **ただし先例（#745）の側には 13 件ある**——`let _ = g.observe(…)` の状態セットアップ呼び出し。
      これが「`#[must_use]` は落とせなくする機構ではない」ことの実証であり、受容残余 1 の根拠に
      なった（`egui_shell/` の `let _ =` は全体 48 件だが、残り 35 件は `Result` を返す
      tauri/Win32 API の best-effort 破棄で対象外）。もう一つの収穫は
      **#6 `try_begin_install` が「取り出して遷移させた後」なのに無防備であること**
      （列挙 11 件で唯一「回復不能」な壊れ方）

## レビュー結果（Step 4）

### 4a. check スキル

| 枠 | 値 | 根拠 |
|---|---|---|
| `/symmetric-check` | **実施** — 適用漏れ 0・取り違え 0・**⚠ 1 件**（型段の射程・反映済み） | 触れた 5 型の全メソッドを列挙して母集団を閉じた。確認漏れを 1 件疑って潰した（`accept_folder_result` は `&self` と実測）。8 件の属性↔シンボル対応を全数照合（取り違え 0） |
| `/race-check` | **該当なし** | 母集団を決めるのは `npm run race:boundaries -- --base main`（skill の SSOT）。判定対象 4 行・8 種別すべて **0 件**を実測 |
| `/dry-check` | **該当なし** | トリガーは「関数・型を新規定義／改名／導入」。関数も型も定義・改名していない |
| `/persistence-check` | **該当なし** | シリアライズ・on-disk 形式に触れていない |
| `/state-check` | **該当なし** | UI モード・状態遷移・ガード条件を 1 つも追加/変更していない |

### 4b. code-reviewer（2 巡）

**round 1**: Critical **0** / High **2** / Medium **1** / Low **8**（うち ⚠ 3）。
**round 2**（fix-forward 差分への再実行）: Critical **0** / High **0** / Medium **0** / Low **7**（うち ⚠ 2）。
**計 18 件すべて修正した。**

**H1/H2 は「機構より強い主張」＝受け入れ条件 6 に直接当たる指摘で、どちらも自分で一次資料に
当たって再現した**（H1: `[workspace.lints.clippy]` を読んで `disallowed_methods` 1 行だけを確認 /
H2: `observe` へメッセージ付き属性を戻して clippy が exit 0 で沈黙することを実測 ＋ レビュアーの
probe crate を独立に再実行）。

| 巡 | # | 指摘 | 対応 |
|---|---|---|---|
| 1 | H1 | `double_must_use` の deny 出所を設定ファイルへ誤帰属（#950 と同型の罠） | 修正 |
| 1 | H2 | 同 lint の射程が過大（メッセージ付きの残存は沈黙） | 修正 |
| 1 | M1 | `dismiss` のメッセージが荷を負う値を取り違え（`false` ではなく `true`） | 修正 |
| 1 | L1 | `Debouncer::poll`「二度と来ない」が全称的 | 修正 |
| 1 | L2 ⚠ | `NoticeSlot::poll` の引用した不変条件の前提が現行呼び出し点で不成立 | 修正（前提を書く形へ） |
| 1 | L3 | 型段の射程の記述が過大かつ危険側に不足 | 修正（`&T` / `impl Trait` 追加・復帰手順に「既存シグネチャの編集」） |
| 1 | L4 | `interp` を「自由関数」と誤分類 | 修正 |
| 1 | L5 | 「下流が型で既に強制している」が偽 | 修正 |
| 1 | L6 | `on_escape_pressed` だけメッセージ無しで残る | 修正（下記の逸脱申告） |
| 1 | L7 ⚠ | `consume_reset_pending`「知る手段はこの返り値だけ」が全称的 | 修正（属性へ「消費した後に」・#749 の doc 逐語は不変） |
| 1 | L8 ⚠ | `try_begin_install`「唯一」の条件不足 | 修正。**round 2 で reviewer が `UpdaterPhase` の書き込み点を全数列挙して真と決着**（`spawn_update_check` は `main.rs:305` の setup 1 回だけで定期再 check が無く、`Installing` から phase を動かすものが存在しない） |
| 2 | L1 | `Option`/`Vec` を「ユーザ定義のジェネリック」と誤記 | 修正（「覆われる例外は `Box`/tuple/配列の 3 つだけ」へ反転） |
| 2 | L2 | 「token の唯一の消費者」はテスト 8 か所で反証される | 修正（「production で消費するのは」へ） |
| 2 | L3 | `interp` は `QueryIntent` を返すので「照合述語」ではない | 修正（導出と述語を分離） |
| 2 | L4 | 残余が 4 つ目になったのに「3 件」が追随していない | 修正 |
| 2 | L5 | `plan.md` に撤回済みの主張が未マークで 3 か所 | 修正 |
| 2 | ⚠1 | `dismiss` が呼び出し点依存の事実を無条件形で述べ `NoticeSlot::poll` と非対称 | 修正（両方を条件形へ揃えた） |
| 2 | ⚠2 | 「落ちるのは冗長な 1 フレーム」は推論（窓高変化のリサイズ再描画は未測） | 修正（「この呼び出し点では stale は起きない」に留め未測を明記） |

**承認範囲からの小さな逸脱を 1 件申告する（round 1 L6）**: 承認された表は #9 `on_escape_pressed` を
「既済（#840）・変更しない」としていたが、**メッセージ無しの属性 1 件だけが残る**状態は、本 PR が
CLAUDE.md へ書いた「メッセージで失うものを名指す」規約と噛み合わない。1 行でメッセージを足した
（発火集合は不変で、診断文が読めるようになるだけ）。**承認した 8 件の集合は変わっていない。**
round 2 で reviewer も「範囲は 1 件も動いていない・戻す必要なし」と判定した。

## 事故 — この plan.md を一度失った

コミット直前、`git rm -r workspace/` が plan.md の未コミット変更を理由に停止したのを受けて、
ステージを戻すために `git reset HEAD workspace/ && git checkout-index -f -- workspace/plan.md` を
打った。**`checkout-index -f` は index（= HEAD）の内容で作業ツリーを上書きする**ため、消し込みと
故障注入ログとレビュー記録がすべて失われた（控えは無し）。

**この節より上の実行記録は会話から再構成したものである。測定値はいずれも当時の実出力**（218 passed /
exit 101 と診断 8 件 / 見出し参照 155 件 / probe の発火表）だが、**再構成である事実は残す**。

教訓が 2 つある。(1) **`git rm` が「local modifications」で止まったのは保護であって障害ではない**——
そこで force するか順序を変えるかの分岐に、`checkout-index` という第 3 の（破壊的な）道を選んだ。
(2) **スキルの「`workspace/` は git 履歴から復元可能」が成り立つのは最終状態をコミットした場合だけ
である**——未コミットの実行記録を持つ plan.md には当てはまらない。**削除の前にコミットする順序が
正しい**（このサイクルでは実装コミットに plan.md を含め、撤去を次のコミットに分ける形を採った）。

## セルフレビュー

- リスク: 通常
- plan-review: 未実施（通常リスク）。永続形式・並行性・状態遷移・ガバナンス文書の移動/圧縮・
  網羅性要件のいずれにも当たらない（`/plan-review`「リスク判定」の高リスク条件に非該当）。
  網羅性は本計画内で独立再導出（3 枠一致）を済ませてある
- エージェント数: 0
- 主エージェントの自己照合（Step 5a の 5 点）:
  1. **issue の全要件に作業項目が対応** — 対応①（`EscapeOutcome` へ属性）= Phase 2 /
     対応②（`let _ =` の確認）= 未確定の最終項で解消（**新たに属性を足す 7 シンボルには 0 件**・
     ただし先例の側に 13 件あり受容残余 1 の根拠になった）/
     対応③（同ディレクトリの数え上げ）= 「対象の全数」表 11 件
  2. **境界条件と検証** — 境界は「属性を付けたとき既存の呼び出し点が壊れるか」であり、
     4 件（assert 化）+ 0 件（他 5 シンボル）+ 13 件（除外理由つき）を全数列挙して検証を割り当てた
  3. **新しい状態・リソース・プロセス** — 増やさない（lint 属性のみ）
  4. **より単純な既存パターンで置き換えられないか** — 「却下した代替案」3 行で検討済み
  5. **壊してはならない不変条件に検知手段** — 「挙動不変」は `cargo test -p snotra` が、
     「属性が効くこと」は Phase 4 の故障注入が検知する
- 要対処: 0 件（自己照合で新たな要対処なし）
- 未検証: **CI（`ci.yml` rust-check）での緑は PR 作成後にしか測れない**——`ci.yml` は
  `pull_request` でのみ起動する（`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて
  行える」）。PR 本文のチェックリストへ送る

### AGENTS.md「条件別チェック」の該当判定

| トリガー | 該当 | 適用 |
|---|---|---|
| 関数・型を新規定義／改名／導入 | **一部** | 新規定義・改名なし。**属性追加で下流の compile-fail を移行漏れ検出器に使う**形は Phase 4 の故障注入が実体 |
| セーフティネットを新設/変更 | **該当** | `.claude/rules/safety-nets.md` の「フォールトインジェクションで一度は実測する」「稼働中のガードを弱めない」を Phase 4 が満たす |
| ガバナンス文書（`*.md`）を変更 | **該当** | `npm run governance:check`（Phase 6） |
| `.rs` を追加/削除 | 非該当 | ファイルの増減なし（G-module-index の索引は不変） |
| 対称ペア / UI モード・ガード / 永続形式 / 並行境界 / 件数・上限 | 非該当 | いずれも触らない |
| レビュー指摘へ fix-forward | 非該当（現時点） | 指摘が来たら該当する |

## 人間レビュー

- [x] 承認済み — 2026-08-07 / 問い: "`#[must_use]` を足す範囲をどうしますか。" /
      回答: "同型の 7 件すべて（推奨）"
- [x] 承認済み — 2026-08-07 / 問い: "`BlurAction` の `#[must_use]` をメソッド段（`observe`）から
      型段（enum 宣言）へ移しますか。" / 回答: "型段へ移す（推奨）"

**確定した範囲**（上の 2 択の帰結・「対象の全数」表の ○ 全行）:

- 純粋核 6 件（`EscapeOutcome` 型段 / `Debouncer::on_input` / `Debouncer::poll` /
  `NoticeSlot::poll` / `try_begin_install` / `dismiss`）
- driver 側 1 件（`consume_reset_pending`）
- 先例の配置統一 1 件（`BlurAction` を型段へ移設・`observe` のメソッド段を削除）
- 除外 2 件（`enter_folder` / `navigate_folder`——`u64` token・下流が型で強制）

**注釈による計画変更は無い**（推奨どおりの選択ゆえ、要件・対象ファイル/シンボル・
インターフェース・不変条件・テスト期待値のいずれも動いていない）。ゆえに `/plan-review` の
追加実行は不要（Step 5c の条件に非該当）。
