# plan — #934 処置を返す純粋核の `#[must_use]` を揃える

## 目的

`egui_shell/` の「純粋核が処置を返し driver が実行する」契約で、**返り値を落とすと副作用が
黙って消える**関数群の強制を `#[must_use]`（＝階梯の「コンパイルで検出」段）へ揃える。
是正の方向は #745 側（`BlurGrace::observe`）が正しく、**先例に足す**。

## 受け入れ条件

1. 下表の「対象 ○」の全行に `#[must_use = "<失うものの名前>"]` が付いている
2. 素の文で戻り値を捨てているテスト 4 件が **assert へ変わっている**（`let _ =` を新設しない）
3. `cargo clippy --workspace --all-targets -- -D warnings` が緑
4. **故障注入で赤を実測している**——production 側へ素の drop を注入し、3 形（型段 / メソッド段
   `bool` / メソッド段 `Option<U>`）すべてが診断に現れることを確認して revert
5. 規則と配置規約が `src-tauri/CLAUDE.md` の `egui_shell/` 節に 1 行で在る
6. **主張を機構より強く書いていない**——下の「不変条件と異常系」の受容残余 2 件が文書に在る

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
| 10 | `search_state::SearchState::enter_folder` | `u64` | ✕ | — | **下流が型で強制している**——`spawn_folder_load(tok, …)` は token 無しに呼べない。落として害が出る形が構築できない |
| 11 | `search_state::SearchState::navigate_folder` | `u64` | ✕ | — | 同上 |

**在否の判断基準は「処置かデータか」ではなく「下流が値を構造的に要求するか」である。**
#10/#11 はテストで 13 件が素で捨てているが、それは規則の**帰結**であって除外の理由ではない
（除外の理由は上記の型強制）。

**母集団を `&mut self` に絞る理由**（issue の文言「処置を返す純粋核」より狭いのは意図である）:
引数だけから導く自由関数——`lifecycle::plan_hotkey → HotkeyPlan`・`notify::overlay_kind`・
`layout::present_results`・`search_state::interp`——は名前の上では処置を返すが、落としても
**同じ引数で呼び直せる**ので失われるものが無い。**状態を進めてから返すものだけが「落とすと
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

- [ ] `search_state.rs:951` を `assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);` へ（コメントは維持）
- [ ] `search_state.rs:1147` を `assert_eq!(s.on_escape(), EscapeOutcome::RestoredFromTool);` へ
- [ ] `search_state.rs:1159` を `assert_eq!(s.on_escape(), EscapeOutcome::RestoredSearch);` へ
- [ ] `layout.rs:632` を `assert!(d.on_input(), "leading 有効の初回はバースト先頭");` へ
- [ ] `cargo test -p snotra` が緑（4 件の期待値が実際の返り値と一致することの確認）

### Phase 2 — 純粋核へ属性

- [ ] `search_state.rs` の `EscapeOutcome` 宣言へ型段の `#[must_use = "…"]`
- [ ] `layout.rs` の `Debouncer::on_input` / `Debouncer::poll` へメソッド段
- [ ] `notify.rs` の `NoticeSlot::poll` / `UpdaterUi::try_begin_install` / `UpdaterUi::dismiss` へメソッド段

### Phase 3 — driver 側と先例の配置統一

- [ ] `launcher_controller.rs` の `consume_reset_pending` へメソッド段。doc（914-916）の
      「返り値を落としてはならない」を「機構（`#[must_use]`）が守る」旨へ改める——散文の命令を
      残したまま属性を足すと、階梯の段が 2 つ書かれた状態になる。**変えるのは命令の様態だけで、
      #749 の理由（`ResultsWindow::reset_size_guard()` は view 側に残るので view が reset フレームを
      知る手段はこの返り値だけ）は逐語で残す**——属性が置き換えるのは命令であって、何が壊れるかの
      説明ではない
- [ ] `lifecycle.rs` の `BlurAction` 宣言へ型段を足し、`observe`（148）のメソッド段を削除。
      **#745 の doc コメントは動かさない**（経緯の記録はそこが正本）

### Phase 4 — 故障注入

- [ ] `launcher_controller.rs` へ 4 件の素の drop を注入
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` が **exit != 0**、かつ診断が
      **4 件**（型段 `EscapeOutcome` / メソッド段 `bool` / メソッド段 `Option<U>` /
      型段へ移設した `BlurAction`）そろうことを確認。exit code と診断の要点を控える
- [ ] `git checkout -- src-tauri/src/egui_shell/launcher_controller.rs` で戻し、clippy が緑に復すことを確認

### Phase 5 — 文書

- [ ] `src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 節（`mod.rs` の索引行より後・
      「外部から窓を起こす経路は…」より前）へ横断不変条件 1 行を追加。内容は
      (a) 処置を返す純粋核は `#[must_use]` を持つ、(b) 型段/メソッド段の配置規約、
      (c) token 返し（`enter_folder` / `navigate_folder`）を除く理由、
      (d) 受容残余 2 件（**`let _ =` で黙る——`lifecycle.rs` テストに実使用 13 件**・
      「見たが捨てた」を捕まえない）。**「落とせなくなった」と書かない**

### Phase 6 — 検証

- [ ] `cargo fmt --all -- --check` / `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test -p snotra`
- [ ] `cargo doc --workspace --no-deps --document-private-items`（doc コメントを触ったため・hook 非発火）
- [ ] `npm run governance:check`（`src-tauri/CLAUDE.md` 変更・G-heading-refs の着地を含む）
- [ ] 実装差分を確定させる（`git diff` で意図した 6 ファイル以外に変更が無いことを確認。
      とくに Phase 4 の注入が残っていないこと）

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
