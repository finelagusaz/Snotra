# 調査: #1076 — `instant_prefix` が engine lock 越しに config を読む（#1032 の射程か）

対象 issue: #1076（検討・rust）。出所は #1073 の調査（2026-08-13）。
ユーザーの指示: 「設計として現状妥当なのか検討から」。

**本文は敵対的調査（3b）の反映後の版である。** 反映前の版が持っていた誤りと、その裁定は §13。

## 1. issue の要約

`egui_shell/launcher_controller.rs` の `instant_prefix` は `instant_command_prefix` を
`engine.lock()` 越しに読む。#1032 の規範は「config の live-read は `egui_shell::read_config`
を通す。`engine.lock()` を経てはならない」と書くが、同じ規範が `commands/` の操作時の読みを
例外として置く。`instant_prefix` の 3 呼び出し点はいずれも毎フレームではないため、
issue は「例外の字面（`commands/`）には当たらないが理由の側には当たる境界事例」と枠づけ、
次の 2 点の裁定を求める。

1. `read_config` へ移すべきか、例外の理由に照らして現状でよいか
2. 移すなら、規範の例外を「`commands/` の」から「毎フレームでない読みは」へ直すのか

**実害は未測定**であることを issue 自身が明記している。

## 2. リポジトリの現状は自己矛盾している

`instant_prefix` 自身の doc（`launcher_controller.rs:747`）は既に断定している。

> **この読みは `engine.lock()` 越しであり、#1032 の規範（…）の未移行の残余である**——#1036 の
> 移設に入らなかった。（…）**「エッジ駆動だから対象外」ではない**（同型の未移行は `egui_shell`
> にほかにもあり、ここだけが例外なのではない）。**新しい読みを足すなら `read_config` へ寄せること。**

出所を実測した（`git log -L 744,749:src-tauri/src/egui_shell/launcher_controller.rs`）。

- 足したのは **28a6342a（2026-08-13・PR #1072「trailing 発火の直後の Enter が最終クエリでない
  行で起動するのを塞ぐ」）**
- **PR #1072 の本文はこの段落を論点として扱っていない。** 本文が `instant_prefix` に触れるのは
  「`on_enter` は判定より前に `instant_prefix` が `engine.lock()` を取るため、走査待ちは本 PR の
  前後どちらでも払っている」の 1 行だけで、費用が不変であることの**根拠として引いている**。
  「未移行の残余である」という断定は、その PR の受け入れ条件の外で付記されたものである

**帰結:**

- **doc-neutral な「現状維持」は存在しない。** keep なら 747 行の doc 自体を書き直す必要がある
- move なら doc は既に正しく、**749 行が予約した連動修正**（`docs/architecture.md` の Enter の
  補足）と `search_state.rs:492` が対象になる
- **doc の断定は所見であって裁定ではない。** 起票したのはユーザー本人であり、裁定の権威は
  そちらに残る。Step 5c の承認がその批准にあたる

## 3. 母集団 — 数え方を 2 度直した

### 3.1 リテラル `config()` の grep では足りない

`src-tauri/src/` の `config()` 出現は 12 件だが、**この数え方は間接読みを落とす**（敵対枠が指摘・
採用）。`Engine` のメソッドが内側で `self.config.read()` するものが 2 つあり、呼び出し元に
リテラル `config()` が現れない。

| 間接読み | 呼び出し元 | スレッド | 判定 |
|---|---|---|---|
| `Engine::capture_folder_list_context`（`engine.rs:163`） | `launcher_controller.rs:818` | folder worker | **射程外** |
| `Engine::recent_history`（`engine.rs:158`） | `launcher_controller.rs:939` | UI フレーム内 | **射程外**（後述） |

**この 2 つは射程外とする。** どちらも「UI が config を読むために錠を取る」形ではなく、
**engine 本体の操作**（`search_engine` への問い合わせ）が自分の config を参照する形である。
錠は操作そのものに要る。`recent_history` は UI フレーム内で呼ばれるが、config だけを外へ
逃がしても錠は残るので、規範が言う移送の対象にならない。

**ただし「リテラル grep で全数」と書いてはならない**（メモリ `literal-grep-misses-constructed-strings`
と同型）。**母集団の定義は「UI 側が config の値を得るためだけに engine lock を取る箇所」**であり、
列挙にはメソッド越しの間接読みの確認が要る。

### 3.2 全数（射程内）

| # | 位置 | 読む値 | 実行文脈 |
|---|---|---|---|
| 1 | `config_watcher.rs:87` | 旧 config 全体 | 適用側（**射程外**——`update_config` と同じ錠の内側で取る必要がある） |
| 2 | `commands/icon.rs:17` | `show_icons` / `icon_cache_cap` | icon worker スレッド |
| 3 | `commands/instant.rs:12` | `instant_commands` | **UI フレーム内**（§5） |
| 4 | `commands/launch.rs:107` | `openers`（`resolve_opener`） | tray スレッド |
| 5 | `commands/launch.rs:158` | `openers` | tray スレッド |
| 6 | `launcher_controller.rs:503` | `instant_commands` | UI フレーム内 |
| 7 | `launcher_controller.rs:726` | `openers`（`resolve_tools`） | UI フレーム内 |
| 8 | `launcher_controller.rs:737` | `auto_hide_on_focus_lost` | UI フレーム内・**毎フレーム** |
| 9 | `launcher_controller.rs:757` | `instant_command_prefix`（**本 issue**） | UI フレーム内・エッジ駆動 |
| 10 | `window_coordinator.rs:194` | `visual.background_color` | show 経路（フレーム外・イベントループ） |
| 11 | `window_coordinator.rs:232` | `general.follow_cursor_monitor` | show 経路（同上） |

## 4. issue が見ていない、より強い違反 — `auto_hide_enabled` は毎フレーム走る

`auto_hide_enabled`（#8・`launcher_controller.rs:730`）は `instant_prefix` の**すぐ上**に同じ形で
置かれており、**`view.rs:666` の `on_focus_changed` から `update()` 内の分岐の外で呼ばれる**
（敵対枠が `view.rs` 全文の `return` 0 件を実測し、無条件であることを裏づけた）。

```
view.rs:666  self.controller.on_focus_changed(pre.focused, &ctx);   ← 分岐の外
  → launcher_controller.rs:1236  let auto_hide = self.auto_hide_enabled();
    → 734-739  s.engine.lock().unwrap().config().general.auto_hide_on_focus_lost
```

**これは境界事例ではない。** #1032 が禁じている形そのもの（毎フレームの config live-read が
engine lock を経る）であり、例外の理由 (A)「毎フレームではない」が成立しない唯一の箇所である。
hidden 中は `update()` が走らないので、正確には「**可視中の毎フレーム**」である。

## 5. 分析の軸を直した — 「毎フレームか」でも「`commands/` か」でもない

**規範の例外が挙げる 2 つの事由は、どちらも実際の害の代理として精度が低い。** 各箇所の
呼び出し元をすべて辿って実測した結果、正しい弁別子は**「egui フレームの中で錠を待つか」**である。

規範自身が書く害はこうである——「UI がその錠越しに設定を読むと**フレームが走査の完了まで
返らない**」。害はフレームに結びついており、頻度にもディレクトリにも結びついていない。

### 5.1 事由 (B)「別目的で同じ錠を既に取る」は 2 か所でしか成立しない

| # | (B) の検算（一次証拠） | 成立 |
|---|---|---|
| 2 | `ensure_icon_cache_loaded_if_enabled` の engine lock は config 2 値の読みだけ。次に取るのは**別の Mutex**（`IconCacheState`） | **×** |
| 3 | `get_instant_commands`（`commands/instant.rs:6-17`）は engine lock を `config().instant_commands` の読みにしか使わない | **×** |
| 4, 5 | `launch_item_with_state`（`launch.rs:112`）は `resolve_opener` の後 `record_and_save` → `launch.rs:55` で **engine の書き込み**のため錠を取る | **○**（起動成功時のみ・⚠️ §13) |
| 6 | **instant 起動は履歴を記録しない**（`launcher_controller.rs:317` の `(o, None)` と 322-327 のガードで `record_and_save` へ到達しない）。後続の engine lock は**存在しない** | **×** |
| 7 | `resolve_tools` は Shift+Enter のメニュー表示・クリック解決の時点であり、起動はその後の別操作 | **×** |
| 8, 9, 10, 11 | 後続に engine lock なし（Plain 枝は channel 送信のみ・show 経路は §5.3） | **×** |

**#3 と #6 が効く。** 「別目的で錠を既に取る」の「別目的」が**それ自体ただの config 読み**なら、
(B) は循環する——config 読み 2 つが互いを正当化することはできない。

### 5.2 実行文脈で切り直す

| 実行文脈 | 箇所 | フレームを止めるか |
|---|---|---|
| icon worker スレッド | #2 (`commands/icon.rs`) | **×** |
| tray スレッド（`platform/tray.rs:67,69,76` が唯一の呼び出し元） | #4, #5 (`commands/launch.rs`) | **×** |
| **UI フレーム内** | **#3 (`commands/instant.rs`)**, #6, #7, #8, #9 | **○** |
| show 経路（イベントループ・フレーム外） | #10, #11 | △（窓が出るまでを止める） |

**例外が `commands/` で正しく切れていたのは、頻度ではなくスレッドの偶然である。**
`commands/` の 4 件のうち 3 件はそもそも egui フレームの外（icon worker / tray スレッド）に居る。

**そして残る 1 件が穴である**——**`commands/instant.rs:12` は例外の字面に守られながら、
UI フレームの中で毎打鍵走る**（`run_search_with` の Instant 枝から）。`instant_prefix` より
**頻度が高く**、(B) も成立しない。規範の字面はこれを免除してしまっている。

### 5.3 show 経路（#10 / #11）は同じ関数の中で規範の両側が混在している

`window_coordinator.rs` の show 経路は `read_metrics`（53 行）と `ime_control`（428 行）が
既に `read_config` を通り、`read_background`（187 行）と `follow_cursor_monitor`（226 行）だけが
engine lock に残る。`read_background` の doc（183-186 行）は自分を
「`follow_cursor_monitor` / `ime_off_on_show` の読みと同じ層である」と書くが、
**その `ime_off_on_show` はもう `read_config` 側に居る。doc の主張は既に事実と食い違っている。**

敵対枠が `show_egui_main` / `hide_egui_main` / `emit_hide` の全文を読み、show/hide 経路に
他の engine lock が無いことを確かめた（(B) 不成立）。

## 6. #9（本 issue）の 3 呼び出し点

`instant_prefix` の呼び出し点は 3 つ（LSP `findReferences` と grep が一致）。

### (a) 打鍵の changed エッジ（`launcher_controller.rs:1373`）

`interp` の結果で 3 枝に分かれる。**どの枝でも `instant_prefix` の (B) は成立しない**
（§5.1）——Instant 枝が次に取る `get_instant_commands` の錠は、それ自体がただの config 読み
だからである。

| 枝 | `instant_prefix` の後に UI スレッドが取る engine lock |
|---|---|
| **Plain**（通常検索・最頻） | **無し**（`search_tx.send()` で worker へ投げるだけ） |
| Instant | `get_instant_commands` = **もう 1 つの裸の config 読み**（#3） |
| Command(/r) | `engine.recent_history()` = 真の engine 操作（射程外・§3.1） |

**注意（自己訂正）**: 反映前の版は「Plain の打鍵エッジで UI スレッドがそのフレームで取る
engine lock は `instant_prefix` の 1 個だけ」と書いたが、**これは偽である**——同じフレームの
`view.rs:666` で `auto_hide_enabled` が既に 1 個取っている（§4 と正面から矛盾していた）。
正しくは「**その打鍵で dispatch 以降に取る錠は `instant_prefix` だけであり、(B) はどの枝でも
成立しない**」である。

### (b) `run_search`（trailing poll / folder drain・846 行）

`poll_search_debounce`（`view.rs:1101`）と folder drain から。Plain の trailing は (a) と同じ。

### (c) `on_enter`（1438 行）

flush する枝では直後に同期 `engine.search`（1453 行）が錠を取り、flush しない枝でも起動後の
履歴記録が worker 側で取る。**ここだけは (B) が成立する**——ただし「どのみち待つ」のであって
「待たない」ではない。

`on_enter` は **`view.rs` のフレーム順で `poll_search_debounce`（1101 行）より後（1110 行）**に
居る。trailing がそのフレームで発火して worker へ投げた直後の `instant_prefix` は、worker が
掴んだばかりの錠を待つ——**#1072 が扱ったのと同じ並びである。**

## 7. フレーム内の位置と、#1036 が測った値との関係

`PERFORMANCE.md`「フレーム後半の帰属」の 2 巡目で単独に囲んだ engine lock の取得点は **4 つ**。

| 計装した読み | 実測（µs） | 位置 |
|---|---:|---|
| `read_window_width` | **911〜43,939** | mainwin（**dispatch より後**） |
| `max_results` | 5,462 / 20,467 | drive（**dispatch より後**） |
| `read_visual` | ≤ 24 | head（**dispatch より前**） |
| `window_gap` | ≤ 1 | drive |

**`instant_prefix` も `auto_hide_enabled` も `get_instant_commands` もこの 4 点に含まれない。**
issue が言うとおり**実害は未測定**である。ただし #1036 は同じ錠・同じ保持者（worker の
`engine.search`・実運用点で 40〜95 ms）を測っており、**待ちの機序と規模は分かっている。
取得点が違うだけ**である。

`PERFORMANCE.md:531` は `read_visual` が ≤24 µs だった理由を
**「dispatch より前ゆえ打鍵フレームでは競合しない」**と書く。当てはめると:

- `auto_hide_enabled`（`view.rs:666`）は **dispatch より前** → `read_visual` と同位置。
  **ただしそれは「同じフレームの worker とは競合しない」であって、前のフレームが投げた走査
  とは競合しうる。#1036 の 3 標本がその重なりを踏んだ保証は無い**
- **(a) の changed エッジと (c) `on_enter` は dispatch と同じかそれより後**。`on_enter` は
  `poll_search_debounce` の後ろ＝ **43,939 µs を実測した `read_window_width` と同じ側**に居る

**この節の主張は「構造上そこに待ちが乗りうる」までであって、ms を測ったものではない。**

## 8. `read_config` へ移すことの意味論 — 値は同一である

- `AppState.config` は `Arc<RwLock<Config>>` で、**`Engine` が持つのと同じ `Arc`**
  （`state.rs:79` の `engine.config_handle()`。写しでないことは
  `app_state_config_is_the_same_arc_the_engine_holds`（`state.rs:121`）が測る——**本文の再読で確認済み**）
- 書き手は `engine.lock().update_config(..)` の **1 本だけ**（`state.rs:22-23`）
- ゆえに **単発読みの値は engine 越しでも `read_config` 越しでも同一**
- **一貫性の要件も掛かっていない**: 3 呼び出し点はいずれも prefix を 1 回読んで引き回す形
  （#637 finding 9 で 1 回へまとめた経緯そのもの）。複数値の同時一貫性（#673 決定 4）は無関係
- 実装形の先例は同ファイルの `lang()`（780 行）——`read_config` + 型からの既定値
  （`ADR-config-default-fallback-references`）。現在の `unwrap_or_else(|| SearchConfig::default()…)`
  はそのまま fallback へ移る
- **逆順ロックの新設も無い**（#1036 の `/race-check` が「逆順 0 件」を実測済み。移行はその向きを増やさない）

## 9. 制約 — 移行差分が触れうる検知器と doc

### ソーステキスト検査（`launcher_controller.rs:1856` `activation_uses_frame_values_not_live_reads`）

`["read_visible_rows(", "read_config("]` を禁止語として、**起動の入口 3 本**
（`fn on_enter(` / `fn activate_or_execute(` / `fn shift_activate(`）へ帰属する出現が 1 つも
無いことを測る。帰属は「出現の直前の字下げ 4 のメソッドヘッダ」で決まる。

**赤にならないことを実測で確かめた**——敵対枠が実際に `instant_prefix` を `read_config` へ
移して `cargo test -p snotra activation_uses_frame_values` を走らせ green を確認、
`git checkout --` で復元した（作業ツリーが clean であることは本調査でも確認）。

ただし **`instant_prefix` の本体を `on_enter` へインライン展開する形は赤になる**——移行の実装形を
「ヘルパーの中身だけ差し替える」に保つことが条件である。この検査の doc は `lang()` が
`read_config` を正当に使うことを**受け入れ条件として明記**しており、同型のヘルパーが増えることは
設計の想定内である。

### 連動して直す doc（移行する場合）

| 文書 | 何が書いてあるか |
|---|---|
| `launcher_controller.rs:747-749` | 「未移行の残余」の断定と、architecture.md への連動予約 |
| `docs/architecture.md:228` | 「`on_enter` は判定より前に `instant_prefix` が `engine.lock()` を取る」——**移行するとこの根拠が消える**（引用文言が現行コードと一致することは敵対枠が確認） |
| `search_state.rs:492-493` | 「実際の費用は `run_search` 入口の `instant_prefix` が `engine.lock()` を取ること（#1032）」 |
| `window_coordinator.rs:183-186` | `read_background` の「`ime_off_on_show` と同じ層」——**既に事実と食い違っている**（§5.3） |
| `src-tauri/CLAUDE.md` #1032 条項 | 例外の文言（Q2 の対象） |

**`docs/architecture.md:228` は注意を要する。** そこは「#1038 の前後で Enter の費用は変わらない」
の**根拠**に `instant_prefix` の錠待ちを使っている。移行しても結論自体は保たれる（flush 枝は
同期 `engine.search` で錠を取る）が、**「判定より前に払っている」という理由づけは偽になる**。
文だけ消さず、根拠を差し替えること。

## 10. Q2 — issue の提案文言は採れず、字面維持も支持できない

issue は「移すなら例外を『毎フレームでない読みは』へ直すのか」と問う。

**(i) その文言は採れない。** `instant_prefix` は毎フレームではないので、
「毎フレームでない読みは engine lock のままでよい」は**移した当の対象を免除する例外**になる。

**(ii) 字面（`commands/` の）維持も支持できない**（反映前の版はこれを推していた・§13）。
§5.2 が示すとおり、`commands/` の 4 件のうち 3 件はフレーム外だから無害なのであって
ディレクトリだからではなく、**残る 1 件（`commands/instant.rs:12`）はフレーム内で毎打鍵走る**。
字面は**穴を持つ**。

**(iii) 害に合わせて書き直すのが正確である。** 例外は「**egui フレームの外で行う読み**
（worker スレッド・tray スレッド）は engine lock のままでよい」と書くのが、規範自身が挙げる
害（フレームが走査の完了まで返らない）と一致する。この形なら:

- `commands/icon.rs`（icon worker）と `commands/launch.rs`（tray スレッド）は例外に入る
- `commands/instant.rs:12` は例外から外れ、移行対象になる
- `egui_shell/` の #6〜#9 も外れる
- show 経路（#10 / #11）はフレーム外だが**窓が出るまでを止める**ので、別途「show 経路も
  `read_config` を使う」と明記するのが現状（`read_metrics` / `ime_control`）とも整合する

**規範文書＝セーフティネットの変更**であり、ルート `CLAUDE.md`「最重要ルール 2」により
**単独で決めない**。Step 5c の問いに差分を名指しで含める。

## 11. 未解決の疑問

1. **裁定そのもの**（ユーザーが決める）: move か keep か。move ならスコープをどこまで取るか
2. **Q2 の文言**: (iii) 害に合わせて書き直すか、規範に触れず実装だけ直すか（規範変更＝要合意）
3. **実測を取るか**: 本 issue は「規範との関係を裁定する」ことを求めており、実害の測定は
   要求していない。#1036 の計器は撤去済みで、入れ直す費用は移行そのものより大きい。
   **移行の費用がほぼゼロ（読み口を替えるだけ）である以上、測ってから決める必要は無い**
   ——というのが本調査の見立てだが、これも裁定の一部である
4. **`commands/instant.rs:12` を含めるか**は本 issue の射程外だが、§5.2 の穴は本 issue の
   裁定に直結する（例外の書き方を決める根拠がここにある）

## 12. 本調査の推奨裁定（ユーザーの承認を要する）

1. **Q1 は「射程に入る・移す」。** 理由:
   - **(B) はどの枝でも成立しない**（Instant 枝が次に取る錠は、それ自体が裸の config 読み・§5.1）
   - 打鍵エッジと Enter は**ユーザーが待っている経路**で、`on_enter` は #1036 が 43,939 µs を
     実測した位置と同じ側に居る（§7）
   - 移行の費用と危険がほぼ無い（同じ `Arc`・単発読み・一貫性要件なし・先例あり・検査 green 実測）
2. **スコープに #8 `auto_hide_enabled` を必ず含める。** 境界事例ですらない毎フレームの違反であり、
   #9 だけ直して隣を残すのは「同じ是非で動くもの」を割ることになる（§4）
3. **#10 / #11（show 経路）も同じ束に入れることを推奨する。** 同じ関数の中で規範の両側が混在し、
   doc の主張が既に偽になっている（§5.3）
4. **#6 / #7 も移行対象である**（反映前の版は「例外に当たる」としていた・§13 で訂正）。
   ただし頻度は起動時・Shift+Enter 時であり、**優先度は #8 / #9 より低い**
5. **#3（`commands/instant.rs:12`）は本 issue の射程外だが、穴として issue 化を推奨する**
   ——例外の字面がこれを免除している事実が、Q2 の答えを決める根拠そのものである
6. **Q2 は (iii)「egui フレームの外で行う読みは engine lock のままでよい」への書き直しを推奨する。**
   ただし規範文書の変更ゆえ、ユーザーの明示的な合意を要する

---

## 13. 敵対的調査（3b）の反映

`workspace/adversarial-1076.txt`（general-purpose / sonnet 1 体）。
**所見は採るが、機序は一次証拠で自分で裁定した**（ルート `CLAUDE.md`「レビューの委譲」）。

### 壊せた項目（採用 4 件・却下 1 件）

| # | 所見 | 裁定 | 一次証拠 |
|---|---|---|---|
| A1 | 「Plain の打鍵エッジで engue lock は `instant_prefix` の 1 個だけ」は §4 と自己矛盾 | **採用** | `view.rs:666` が分岐の外。同じフレームで `auto_hide_enabled` が既に 1 個取る。§6 で主張を「dispatch 以降に取る錠」へ直した |
| A2 | 「#6 `instant_commands` は (B) 成立（起動 → 履歴記録）」は事実誤認 | **採用** | `launcher_controller.rs:317` の `(o, None) // instant は履歴を記録しない` と 322-327 のガード。自分で読み直して確認 |
| A3 | `commands/icon.rs:17` は (B) を満たさない | **採用** | `commands/icon.rs:15-19` の engine lock は config 2 値の読みのみ。次は別 Mutex（`IconCacheState`） |
| A4 | `commands/instant.rs:12` も (B) 不成立、しかも**毎打鍵・UI フレーム内** | **採用（最も価値が高い）** | `commands/instant.rs:10-12` が config 読みだけに錠を使う。`run_search_with` の Instant 枝から毎打鍵。**§5 の軸そのものを書き直す根拠になった** |
| A5 | 母集団は間接読み（`capture_folder_list_context` / `recent_history`）を落とす | **採用（数え方）／却下（射程）** | `engine.rs:158-170` で `self.config.read()` を確認。**列挙方法の不備は正しい**が、両者は engine 本体の操作であり錠は操作に要る——**射程外**と裁定した（§3.1） |

**A4 の機序は自分で裁定した。** 敵対枠は「未検証の未移行候補」と述べるにとどまるが、
一次証拠を辿ると**規範の例外の字面が穴を持つ**ことの証明になっている。§5.2 と §10 はこの
裁定に基づく（レビュアの言葉の写しではない）。

### 壊せなかった項目

- 命題 2（`auto_hide_enabled` が可視中の毎フレーム）——`view.rs` 全文の `return` 0 件で攻撃、**生存**
- 命題 5（移行はソーステキスト検査を赤にしない）——**実際に移行を当てて `cargo test` を実行**し
  green を実測、`git checkout --` で復元。**生存**（§9 へ反映）
- #10 / #11 は show/hide 経路に他の engine lock を持たない——`show_egui_main` / `hide_egui_main` /
  `emit_hide` の全文で攻撃、**生存**
- Plain 分岐（`run_search_with`）は channel 送信のみ——**生存**
- `docs/architecture.md:228` の引用は現行コードと一致（doc は腐っていない）——**生存**

### ⚠️ 確信の持てない所見（返り値に含まれたもの）

| 所見 | 裁定 |
|---|---|
| `launch.rs` の (B) は起動が**成功**する場合に限る（`if result.is_ok()`）。tools≥2 / 中断では厳密には不成立 | **正しい。** ただし §5.2 の軸（フレーム外）では結論が変わらないので、(B) の弱さは推奨に影響しない |
| `auto_hide_enabled` の錠が実測可能な性能影響を持つかは未計測 | **正しい。** §7 に明記済み。測らないという判断は §11-3 でユーザーへ渡す |
| 間接読み 2 件が #1032 の射程内かは未判定 | **本調査で裁定した**（射程外・§3.1） |
| `AppState.config` と `Engine` の config が同一 `Arc` という §8 の主張は未検証 | **本調査で検証した**——`state.rs:79` / `state.rs:121` のテスト本文を再読（§8） |

### 測定環境の検算について

敵対枠へ「実 `config.toml` を読みに行き、計器が測る枝と変更が触る枝の食い違いを疑え」と
渡したが、**その報告は返り値に現れなかった**。本調査の結論は config の**値**に依存しない
（どの値でも `instant_prefix` は engine lock を取る）ため、**受容する残余**とする。
値に依存するのは「Plain 枝が主経路か」だけであり、これは推奨の理由 3 本のうち §7 の
優先度づけにしか効かない。
