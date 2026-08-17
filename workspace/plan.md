# 実装計画: #1076 — UI フレーム内の config 読みを engine lock の外へ出す

調査は `workspace/research.md`、敵対的調査は `workspace/adversarial-1076.txt`。
ブランチ: `chore/instant-prefix-lock-scope`。

## 目的

#1032 の規範（config の live-read は `egui_shell::read_config` を通す）を、**規範自身が挙げる
害の形**——「UI がその錠越しに設定を読むとフレームが走査の完了まで返らない」——に合わせて
実装と文言の両方で整える。

`instant_prefix`（本 issue の対象）は境界事例だが、調査で**分析の軸そのものが違っていた**ことが
分かった。害はフレームに結びついており、頻度（毎フレームか）にもディレクトリ（`commands/` か）にも
結びついていない。ゆえに:

- **egui フレームの中で錠を待つ読みを `read_config` へ移す**（対象の正本は下の「規範の検算」の表）
- **例外文を「egui フレームの外の読み」へ書き直す**（規範文書＝合意済み）

## ユーザーの裁定（2026-08-17・Step 1 の要求判断）

| 問い | 回答 |
|---|---|
| Q1 スコープ | **フレーム内の 4 件を移す**（`instant_prefix` / `auto_hide_enabled` / show 経路 2 件） |
| Q2 規範の文言 | **害に合わせて書き直す**（「egui フレームの外で行う読みは engine lock のままでよい」） |
| Q3 `commands/instant.rs:12` | **本 issue のスコープに含める** |
| **Q1 の改訂**（5c 提示後） | **「6 件すべて移そう」** — `launcher_controller.rs:503` / `:726` の据え置きを撤回 |

→ 移行対象は **7 か所**（`egui_shell/` の 6 件 + `commands/instant.rs` の 1 件）。

**この改訂で受容残余が消える。** 据え置きが無くなるため、規範文へ
「フレームの中に居るが例外」という但し書きを置く必要がなくなった（下の「規範差分」から削除済み）。
**5c で名指した Q1×Q2 の交点の問題は、スコープを広げる側で解消した。**

## 受け入れ条件

1. 移行対象 7 か所が `egui_shell::read_config` を通り、`engine.lock()` を経ない
   （`egui_shell/` の 6 件 + `commands/instant.rs` の 1 件）。**移行後、`egui_shell/` に
   `engine.lock().config()` は 0 件になる**
2. 読む値が移行前後で同一である（同じ `Arc` ゆえ構造的に保証。既存テスト
   `app_state_config_is_the_same_arc_the_engine_holds`（`state.rs:121`）が固定する）
3. `src-tauri/CLAUDE.md` の #1032 条項の例外文が、**弁別子を「egui フレームの外か中か」へ改めて
   おり、かつ改定後の文言に反する箇所が 0 件である**（下の「規範の検算」）
4. `instant_prefix` の doc（`launcher_controller.rs:747-749`）が予約した連動修正が果たされている
5. カテゴリ A（fmt / clippy `-D warnings` / test）・`cargo doc`・`governance:check`（カテゴリ F）が green
6. `/race-check` が並行境界について所見なし
7. カテゴリ C（`smoke:startup` / `smoke:egui`）が passed（show 経路に触るため）

## 規範の検算 — 新しい文言を既存の全事例に当てる

**新設した規約は既存の全事例に当てて検算してから書く**（`AGENTS.md`「検証の作法」）。
改定案「egui フレームの外の読みは engine lock のままでよい」を全 11 か所に当てる。

| # | 位置 | 実行文脈 | 改定後の規範に照らして |
|---|---|---|---|
| 1 | `config_watcher.rs:87` | 適用側 | **射程外**（`update_config` と同じ錠の内側で取る必要がある。読みではなく適用の一部） |
| 2 | `commands/icon.rs:17` | icon worker スレッド | 例外に当たる（据え置き・**適合**） |
| 3 | `commands/instant.rs:12` | **UI フレーム内** | **移行する** |
| 4, 5 | `commands/launch.rs:107,158` | tray スレッド | 例外に当たる（据え置き・**適合**） |
| 6 | `launcher_controller.rs:503` | **UI フレーム内** | **移行する**（Q1 改訂） |
| 7 | `launcher_controller.rs:726` | **UI フレーム内** | **移行する**（Q1 改訂） |
| 8 | `launcher_controller.rs:737` | UI フレーム内・毎フレーム | **移行する** |
| 9 | `launcher_controller.rs:757` | UI フレーム内・エッジ駆動 | **移行する** |
| 10, 11 | `window_coordinator.rs:194,232` | show 経路 | **移行する** |

**Q1 の改訂により、改定後の規範に適合しない箇所は 0 件になった。** UI フレーム内・show 経路の
読みはすべて `read_config` を通るため、規範文へ「フレームの中に居るが例外」という但し書きを
置く必要がない——**受容残余の名指しは不要になり、削除した**。

**射程外の 1 件（`config_watcher.rs:87`）だけは規範文で位置づける**——あれは読みではなく適用手続きの
一部で、`update_config` と同じ錠の内側で取ることに意味がある（独立導出レビュー・未検証 4）。

## 変更ファイルと対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/launcher_controller.rs` | `execute_instant_selected` 内の読み（500-508） | `read_config` へ + **doc 484 の訂正**（下記） |
| 〃 | `resolve_tools`（721-727） | `read_config` へ + **doc 717-720 の訂正**（下記） |
| 〃 | `auto_hide_enabled`（730-742） | `read_config` へ |
| 〃 | `instant_prefix`（744-763） | `read_config` へ + doc 747-749 の書き直し |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `read_background`（180-201） | `read_config` へ + doc 183-186 の訂正 |
| 〃 | `position_on_target_monitor` 内の `follow_cursor` 読み（225-236） | `read_config` へ |
| `src-tauri/src/commands/instant.rs` | `get_instant_commands`（6-17） | `read_config` へ + doc 追加 |
| `src-tauri/CLAUDE.md` | 「モジュール構成」内の #1032 条項 | 例外文の書き直し + 受容残余の名指し |
| `docs/architecture.md` | 228 行の Enter の補足 | 根拠の差し替え |
| 〃 | **231 行の #1032 の bullet** | 「UI は `read_config` から読む」が全称に読める。受容残余の所在（`src-tauri/CLAUDE.md`）を指す形へ |
| `src-tauri/src/egui_shell/search_state.rs` | 492-493 の doc | 「実際の費用」の記述を**縮める**（消さない——#1079 の費用訂正の根拠。残るのは Plain 腕だけ） |
| `src-tauri/src/egui_shell/launcher_controller.rs` | **1845-1848**（`activation_uses_frame_values_not_live_reads` の doc） | **数え上げ「この 2 つが対象外」が移行の瞬間に腐る**（下記） |

**新規ファイルなし・削除ファイルなし**（モジュール索引の更新は不要）。

**`docs/adr/ADR-blur-grace-single-field-state-machine.md` は触らない**——理由は次節。

## `auto_hide_enabled` の毎フレーム読みは意図的な設計であり、その前提が既に腐っている

**Step 1 の点 7（概念ラベルでの grep）が掘り当てた。** `ADR-blur-grace-single-field-state-machine`
の「検討した代替案と却下理由」に、この読みを**毎フレーム無条件にした判断が明記されている**。

> **`auto_hide` を遅延評価で渡す**: 却下。現行は `if let Some(at)` のネスト内で読むため armed の
> ときしか engine lock を取らないが、値渡しにすると毎フレーム取る。（…）engine lock は既に
> **毎フレーム無条件で 2 回（`read_visual` / `lang()`）走っており、2 → 3 に増えるだけである。**

**この却下理由が乗る前提は、6 日後に偽になった。**

| 事象 | 日付 |
|---|---|
| ADR（`3def220a` #932）——「`read_visual` / `lang()` が毎フレーム engine lock を取る」を前提に「2 → 3」と評価 | **2026-08-05** |
| #1036（`d23c062f`）——`read_visual` と `lang()` を `read_config` へ移す | **2026-08-11** |

**今日の値は 2 → 3 ではなく 0 → 1 である。** `read_visual` も `lang()` も engine lock を経ておらず
（`mod.rs:442` / `launcher_controller.rs:781`）、**`auto_hide_enabled` は毎フレーム無条件に
engine lock を取る唯一の箇所**になっている。ADR が「安い」と評価した根拠がそのまま消えた。

### 帰結 2 点

1. **今回の変更は ADR の決定を覆さない。** ADR が却下したのは**遅延評価（クロージャ渡し）**で、
   採ったのは**値渡し・毎フレーム無条件**である。`read_config` への移行は値渡し・毎フレーム無条件を
   **そのまま保つ**——変わるのは読みの費用だけである。ルート `CLAUDE.md`「意図的なリファクタリングの
   結果を元に戻さない」に抵触しない（意図は導入コミット `3def220a` と当該 ADR で確認済み）
2. **ADR 本文は直さない。** ADR は凍結された歴史であり腐るに任せる
   （`.claude/rules/governance-docs.md`・`ADR-adr-frozen-history`）。**生きた層へ書く**——
   `auto_hide_enabled` の doc に「毎フレーム無条件で読むのは意図的（`ADR-blur-grace-single-field-state-machine`）
   であり、その費用評価の前提は #1036 で変わったため `read_config` を通す」を 1 文で置く

## 実装形（移行対象すべてで共通）

`lang()`（`launcher_controller.rs:780`）と同型にする。`ADR-config-default-fallback-references`
の規律に従い、既定値は**型から導く**（リテラルを置かない）。

```rust
crate::egui_shell::read_config(
    &self.app_handle,
    |c| c.search.instant_command_prefix.clone(),
    || SearchConfig::default().instant_command_prefix,
)
```

**`read` クロージャの中は純粋なフィールド取り出し（+ `clone`）に留める**——`read_config` の doc が
「`read` の中で lock を取る操作を書かないこと」を要求する。移行対象はどれも現状が既にその形で
あり、条件を満たす（`resolve_tools` が呼ぶ `find_matching_tools` の純粋性は下の専用節で実測済み）。

**`instant_prefix` / `auto_hide_enabled` の本体を呼び出し元へインライン展開しない**——
`activation_uses_frame_values_not_live_reads`（`launcher_controller.rs:1856`）が起動の入口 3 本へ
帰属する `read_config(` を禁じている。ヘルパーの中身だけを差し替える形なら帰属先は
`fn instant_prefix(` になり green（敵対枠が実際に当てて `cargo test -p snotra
activation_uses_frame_values` で実測・復元済み）。

## Q1 改訂で新たに偽になる doc 2 件（自分で実測して見つけた）

据え置きを撤回したことで、**その 2 件が自分の錠の在り方を doc に書いていた**ことが問題になる。

| 位置 | 現在の記述 | なぜ偽になるか |
|---|---|---|
| `launcher_controller.rs:484` | 「action 抽出はここ（UI スレッド・**engine ロック内**）で行い、clipboard 読み + 実行は worker 側」 | 移行後は engine ロック内ではない。**ロック内/外の対比がこの doc の主題**なので、字面だけ直すと対比が壊れる——「config の読み（`read_config`）→ clipboard 読みと実行は worker」へ立て直す |
| `launcher_controller.rs:717-720` | 「lock は解決の間だけ保持（**ロック内純 CPU** → clone）」 | 同上。`find_matching_tools` が純 CPU であること自体は真のまま（**実測**: `snotra-core/src/opener.rs:59` は `to_lowercase` / `starts_with` / `rfind` のみで錠も I/O も無い）。「engine lock」の部分だけが偽 |

**`find_matching_tools` を `read_config` のクロージャ内で呼ぶことは規範に反しない**——
`read_config` の doc が禁じるのは「`read` の中で lock を取る操作」であり、当該関数は錠を取らない。
確保（`to_lowercase().replace()`）は現行でも engine lock 内で払っており、より軽い錠へ移るだけである。

## ソーステキスト検査の doc が持つ数え上げが、移行の瞬間に腐る

`activation_uses_frame_values_not_live_reads` の doc（`launcher_controller.rs:1845-1848`）は書く。

> **`run_search_with` は対象外である**（意図的）。（…）同様に `lang()` は `read_config` を正当に
> 使う（…）。**この 2 つが対象外のままであることが、この設計の受け入れ条件である。**

**移行すると `instant_prefix` と `auto_hide_enabled` も `read_config(` の対象外の出現になる**
——帰属先が起動の入口 3 本ではないので、検査は緑のまま、しかし「この 2 つ」という**数え上げが
偽になる**。検査は赤くならないので**沈黙で腐る**。

**`AGENTS.md`「検証の作法」の「数え上げも同じ強さである——数ではなく正本（分岐そのもの）を指す」
に従い、件数を書かない形へ書き直す。** メモリ `universal-claim-fix-regenerates-itself` の教訓
（数え上げをやめ「〜だけではない」の下限主張へ倒すと止まる）をそのまま当てる。

**doc を直すときの注意**（独立導出レビュー・軽微 6）: 訂正文の中に**字下げ 4 の `fn ` で始まる行**を
書かないこと——`owners_of` の帰属が偽ヘッダを掴む残余に触れる。canary の綴りにも触れない。

## 不変条件と異常系

1. **読む値は移行前後で同一である。** `AppState.config` は `Engine` と同じ `Arc<RwLock<Config>>`
   （`state.rs:79` の `engine.config_handle()`）。書き手は `engine.lock().update_config(..)` の
   1 本だけ（`state.rs:22-23`）
2. **逆順ロックを新設しない。** `read_config` の guard を保持したまま `engine.lock()` を要求する
   形を書かない。移行対象はどれも読み切って `clone` / `Copy` / `to_vec` で返す
3. **一貫性の要件は無い。** `instant_prefix` の 3 呼び出し点はいずれも prefix を 1 回読んで
   引き回す形（#637 finding 9）。複数値の同時一貫性（#673 決定 4）は掛かっていない
4. **`get_instant_commands` の異常系が変わる（意図的・挙動不変の唯一の破れ）。**
   現状は `app.state::<AppState>()` で AppState 不在なら **panic** する。`read_config` は
   `try_state` ゆえ fallback へ落ちる。

   **実装中に、同型の非対称がもう 1 件あることが判明した**（計画外・Phase 1 で発見）:
   `execute_instant_selected` は移行前、`try_state` が `None` を返すと**その場で黙って
   return** していた（497-499 のガード）。`read_config` の fallback `|| None` へ移ったことで、
   「見つからない」と同じ処置——`egui_instant_error` の trace を残して return——へ合流する。
   **どちらも到達不能な経路であり、変化の向きは診断可能性が上がる側である。**
   残る移行対象は現行も `try_state` なので fallback の有無が変わらない。

   **`AGENTS.md` 条件別チェックの「どの分岐が選ばれるかを決める値の出所を変更」に当たるため、
   「この値で初めて走る行」を列挙する**（差分に現れない下流を 1 段辿る・#977）:

   | 経路 | 現在 | 移行後 |
   |---|---|---|
   | AppState 在り（実運用の全経路） | config の `instant_commands` | **同一**（同じ `Arc`） |
   | AppState 不在（`.manage` は `.setup` より前ゆえ**到達しない**） | **panic** | `filter_instant_commands` が `Config::default().instant_commands`（g / gh の 2 件）で走る |

   **新しく生きるのは下段だけであり、そこは production から到達しない。**
   fallback は `Config::default().instant_commands`（型から導く・`ADR-config-default-fallback-references`
   準拠。ほかの移行対象および `lang()` と同じ流儀）。**`Vec::new()` にしない**——既定値を型から導く規律を
   ここだけ破ると、既定の出所が 2 つになる。**panic が消える方向の変更である**ことを doc に書く
5. **`read_background` は show 経路（フレーム外）だが移す。** 理由は「窓が出るまでを止める」ことと、
   **同じ関数の中で `read_metrics`（52 行）と `ime_off_on_show`（430 行）が既に `read_config` 側に
   居り、doc の主張（183-186）が既に事実と食い違っている**こと

## テスト方針と検証コマンド

**新しいテストは足さない。** 受け入れ条件 2 は既存の `state.rs:121`
（`app_state_config_is_the_same_arc_the_engine_holds`）が既に固定しており、同じ事実を測る
テストを増やすのは写しになる。移行が値を変えないことは型と `Arc` の共有で構造的に成立する。

**検知器も新設しない——ただし残余が広がることを明記する。** #1032 の規範は「**機構は無い**
——`engine.lock().config()` は今もコンパイルが通るので、UI 層へ新しい読みを足すときは規範として
守る」を**受容残余として明記済み**であり、本 issue はその判断を覆す根拠を持たない（覆すなら別 issue）。

**ただし Q2 の書き直しは規範の射程を広げる**——これまで `egui_shell/` だけを見ていた条項が、
`commands/` のフレーム内の読みも覆うようになる。**機構が無いという残余は、そのぶん広い面を覆う
ことになる。** これを黙って引き継がず、規範文へ射程が広がった事実とともに書く
（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」——
「既存の機構が捕まえるから要らない」を検査を書かない理由に使わないこと）。

| 検査 | コマンド | 実行 |
|---|---|---|
| カテゴリ A（fmt / clippy `-D warnings` / test） | `docs/build-commands.md` カテゴリ A | PostToolUse hook が自動（沈黙 = 合格） |
| `cargo doc` | `docs/build-commands.md` カテゴリ A の `cargo doc` 行 | **手動**（hook は沈黙する・`.claude/rules/comments.md`） |
| `governance:check` | `npm run governance:check` | **手動**（`*.md` を触るため・カテゴリ F） |
| `/race-check` | skill | **手動**（フレーム内 live-read の変更・`AGENTS.md` トリガー） |
| カテゴリ C | `smoke:startup` / `smoke:egui` | **手動**（show 経路に触るため・`.claude/rules/src-tauri.md`） |

**性能は測らない。** #1036 の計器は撤去済みで、入れ直す費用は移行そのものより大きい。
本 issue は「規範との関係を裁定する」ことを求めており実測を要求していない（issue 本文）。
**ゆえに PR 本文にも `PERFORMANCE.md` にも「速くなった」と書かない**——書けば未測定の主張になる。

## SPEC.md・関連文書の更新要否

**`SPEC.md` の更新は不要。** 本変更は config の**読み口**を替えるだけで、`SPEC.md` が記述する
挙動・フロー・状態遷移を 1 つも変えない（値は同一・§不変条件 1）。`AGENTS.md` の判定に照らすと
「バグ（記述に合わせる）」でも「仕様変更（記述を変える）」でもなく、**内部構造の是正**である。

更新する文書は「変更ファイルと対象シンボル」の表の 3 件（`src-tauri/CLAUDE.md` /
`docs/architecture.md` / `search_state.rs` の doc）。

## 作業項目

### Phase 1 — `launcher_controller.rs` の 4 件

- [x] `execute_instant_selected` 内の読み（500-508）を `read_config` へ移す
- [x] `execute_instant_selected` の doc 484 を訂正する（ロック内/外の対比を立て直す）
- [x] `resolve_tools`（721-727）を `read_config` へ移す
- [x] `resolve_tools` の doc 717-720 を訂正する（「engine lock」の部分のみ。純 CPU の記述は真のまま）
- [x] `auto_hide_enabled`（730-742）を `read_config` へ移す
- [x] `auto_hide_enabled` の doc に、毎フレーム無条件の読みが意図的であること
      （`ADR-blur-grace-single-field-state-machine`）と、その費用評価の前提が #1036 で変わったことを
      1 文で置く（**ADR 本文は直さない**——凍結された歴史）
- [x] `instant_prefix`（750-763）を `read_config` へ移す
- [x] `instant_prefix` の doc 747 を書き直す（「未移行の残余である」→ 移行済みの事実と、
      **なぜここが `read_config` を通すのか**（フレーム内・エッジ駆動でも worker と重なる））
- [x] `instant_prefix` の doc 749（`docs/architecture.md` への連動予約）を、Phase 4 で果たす前提の
      記述へ改める（予約が果たされたら文自体を落とす） — **予約は Phase 4 で果たすため文ごと落とした**
- [x] `activation_uses_frame_values_not_live_reads` の doc 1845-1848 の**数え上げを外す**
      （「この 2 つが対象外のままであることが受け入れ条件」→ 件数を持たない形。上記専用節）
- [x] カテゴリ A が green（hook の沈黙で確認）+ `cargo doc` を手で走らせて green —
      fmt / check / clippy `-D warnings` / `cargo test -p snotra -q`（**292 passed**）/ `cargo doc` すべて exit 0
- [x] **（実装中に判明）** `execute_instant_selected` の AppState 不在時の挙動が変わったことを
      不変条件へ追記する——現状は 497-499 の `try_state` ガードで**黙って return** していたが、
      `read_config` の fallback `|| None` へ移ったため `egui_instant_error` の trace が残るようになった。
      到達不能な経路（`.manage` は `.setup` より前）だが、`get_instant_commands` と同型の非対称である
- [x] Phase 1 をコミット

### Phase 2 — `window_coordinator.rs` の 2 件（show 経路）

- [x] `read_background`（187-201）を `read_config` へ移す
- [x] `position_on_target_monitor` 内の `follow_cursor` 読み（226-236）を `read_config` へ移す
- [x] `read_background` の doc 183-186 を訂正する（**`ime_off_on_show` はもう `read_config` 側に
      居る**ため「同じ層である」が偽。`read_visual` と統合しない理由は保つ）——
      **「同じ層である」は移行で真になった**ので消さず、層が engine lock を持たなくなったことを
      書き足す形にした。併せて「1 フレーム 1 lock の規律」を「1 フレーム 1 読みの規律」へ直した
      （層に lock が無くなったため字面が腐る）
- [x] カテゴリ A が green + `cargo doc` green — fmt / check / clippy / `cargo test -p snotra -q`
      （**292 passed**）/ `cargo doc` すべて exit 0
- [x] Phase 2 をコミット

### Phase 3 — `commands/instant.rs` の 1 件

- [x] `get_instant_commands`（10-12）を `read_config` へ移す（fallback は
      `Config::default().instant_commands`）
- [x] doc を足す——**この関数は `commands/` に居るが UI フレームの中で毎打鍵走る**こと、
      および `app.state()` → `try_state` で panic が消える方向であること
- [x] **（実装中に判明）** 実装形が「純粋なフィールド取り出し + `clone`」に収まらなかった——
      `filter_instant_commands` が返すのは `Vec<&InstantCommand>` で config を借りたままの参照ゆえ、
      **DTO 化まで読みの中で終える**必要がある。絞り込みと DTO 化を `matching_dtos` へ 1 つに束ね、
      正常系と fallback の両方がそれを通る形にした（所有へ移す一手と DTO 変換が同じ仕事なので、
      余分な確保は増えない）。行うのは文字列の確保までで I/O も錠も無い
- [x] カテゴリ A が green + `cargo doc` green — fmt / check / clippy / `cargo test -p snotra -q`
      （**292 passed**）/ `cargo doc` すべて exit 0
- [x] **受け入れ条件 1 を実測** — multiline grep で `egui_shell/` の `engine.lock().config()` が
      **0 件**。残余は `config_watcher.rs:87`（適用側）・`commands/icon.rs:17`（icon worker）・
      `commands/launch.rs:107,158`（tray スレッド）の 4 件で、**新しい規範文の例外と一致する**
- [x] Phase 3 をコミット

### Phase 4 — 規範と文書

- [x] `src-tauri/CLAUDE.md` の #1032 条項の例外文を書き直す（下記「規範差分（逐語案）」）
- [x] ~~同条項に据え置き 2 件を受容残余として名指す~~ — **Q1 の改訂（6 件すべて移行）により不要**。
      移行後 `egui_shell/` の `engine.lock().config()` は 0 件で、規範文は全称のまま真である
- [x] `docs/architecture.md:231` の #1032 の bullet を見る——「UI は `egui_shell::read_config` から
      読む」が全称に読める。**#1032 が移したのは実測で名指した読みまでで残りは #1076 が寄せた**旨と、
      射程の正本の所在を書き足した
- [x] `docs/architecture.md:228` の Enter の補足を**再導出する** —
      **勘定は反転した。** 独立導出レビュー要対処 1 の警告どおりで、「1 回あたりの費用は変わらない」は
      #1076 以降成り立たない。無条件の走査待ち（`instant_prefix`）が消えたため、いまは flush の
      有無で分かれる: **flush へ倒れない Enter は錠を一切待たず、flush へ倒れる Enter だけが同期
      `engine.search` の錠待ちと走査を負う**。#1038 が広げたのは flush へ倒れる窓であるから、
      **その広がったぶんの Enter は #1076 以降、走査待ちを新たに負う**（以前はどちらでも払って
      いたので差が出なかった）。旧記述が何を根拠にしていたかも併記した
- [x] `src-tauri/src/egui_shell/search_state.rs:492-493` の doc を現況へ直す（消さず縮めた——
      #1079 の費用訂正の根拠は「Plain 腕が `indexing()` 中に復帰行を空にすること」として残る）
- [x] `npm run governance:check` が全検査 passed — **19 検査 passed**（見出し参照 221 件・ADR 短縮引用 274 件）
- [x] `cargo doc` green
- [x] Phase 4 をコミット

### Phase 5 — 検証

- [ ] `/race-check` を実行し所見なしを確認
- [ ] カテゴリ C（`smoke:startup` / `smoke:egui`）を実行し passed を確認
- [ ] `docs/build-commands.md` カテゴリ A を通しで再実行し green を確認
- [ ] 実装差分を確定させる（レビュー指摘があれば当て、**指摘を出した枠組みを修正差分にも
      再実行してから閉じる**・`AGENTS.md` 条件別チェック）

## 規範差分（逐語案・Step 5c で承認を求める対象）

`src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 内、#1032 条項。

**現在**（該当部分）:

> **`commands/` の操作時の読みは engine lock のままでよい**（毎フレームではなく、
> `resolve_opener` のように別目的で同じ錠を既に取るものがある）。

**改定案**:

> **例外は「egui フレームの外で行う読み」である**——icon worker・folder worker・tray スレッド
> （`commands/launch.rs` の `resolve_opener` 系）は engine lock のままでよい。
> **弁別子はディレクトリでも頻度でもなく、フレームを止めるかである**——規範が挙げる害が
> それだからである。**どちらかは呼び出し元を辿って判定すること**（同じ関数が両方から呼ばれる
> ようになれば分類は変わる）。`commands/` に在っても
> `get_instant_commands` は `run_search_with` の Instant 枝から**毎打鍵 UI フレームの中で**
> 走るため `read_config` を通す。**show 経路（`window_coordinator.rs`）もフレームの外だが、
> 窓が出るまでを止めるので `read_config` を通す**（`read_metrics` / `ime_off_on_show` と揃える）。
> **`config_watcher` が適用の前に読む旧 config は射程外である**——あれは読みではなく適用手続きの
> 一部で、`update_config` と同じ錠の内側で取ることに意味がある。**機構は無い**——
> `engine.lock().config()` は今もコンパイルが通るので、UI 層へ新しい読みを足すときは規範として
> 守る。**この条項が `commands/` も覆うようになったぶん、その残余は以前より広い面に掛かる。**

**この差分はセーフティネットの変更**（ルート `CLAUDE.md`「最重要ルール 2」）であり、
Q2 で「害に合わせて書き直す」の合意を得ている。**Q1 の改訂（6 件すべて移行）により、
5c で問題として名指した受容残余の但し書きは不要になり削除した**——規範文が全称のまま真になる。

## 未確定（実装前に潰す）

- [x] `get_instant_commands` の呼び出し元が UI 経路だけか（tray から呼ばれないか） —
      **実測: 呼び出し元は `launcher_controller.rs:904` の 1 本のみ**（`grep -rn get_instant_commands
      src-tauri/src`）。tray は `launch_*_with_state` を呼ぶ別経路
- [x] `read_config` が `commands/` から呼べるか — **実測: `pub(crate)`**（`mod.rs:423`）
- [x] `ime_off_on_show` が本当に `read_config` 側に居るか（doc 訂正の根拠） —
      **実測: `window_coordinator.rs:430` で `read_config` を使用**
- [x] 移行がソーステキスト検査を赤にしないか — **実測: 敵対枠が実際に移行を当てて
      `cargo test -p snotra activation_uses_frame_values` を green で確認し `git checkout --` で復元。
      作業ツリーが clean であることを本調査でも確認**
- [x] `Config::default().instant_commands` が空か — **実測: 空ではない（g / gh の 2 件・
      `snotra-core/src/config.rs:621`、テスト `:1954`）。到達しない経路の fallback なので
      ADR 準拠（型から導く）を優先する**
- [x] `docs/architecture.md:228` の引用が現行コードと一致するか（doc が腐っていないか） —
      **敵対枠が確認・一致**

## plan-review 結果

- リスク: **高**（規範文書＝セーフティネットの変更を含む）
- レビュー方式: **独立導出 1 体**（Step 2b。成果物 `workspace/plan-review-1076-independent.md`）
- エージェント数: **2**（3b の敵対的調査 1 体 + 独立導出 1 体）

**独立性は完全ではない（レビュア自身の開示）。** 最初の探索で `read_config` をリポジトリ全体へ
grep した結果、`workspace/plan.md` の移行対象表・`research.md` の節見出し・
`adversarial-1076.txt` の断片が tool result として目に入った。以降は `workspace/` を全除外し、
**列挙を漏れた一覧に依存しない機械的な母集団**（`.lock()` 全 77 件のレシーバ分類）へ据え直している。
**ゆえに移行 5 件の一致は汚染に依存しない**——母集団の取り方が本計画と独立であり、かつ
リテラル `config()` 狙いの grep が落とす 3 件を実際に拾い直している。**ただし「完全な独立導出」
として扱わない。**

**導出の突き合わせ:**

- **導出 ∖ plan（漏れ候補）: 移行対象については 0 件。** 独立導出は母集団を
  `.lock()` 全 77 件のレシーバ分類で取り（リテラル `config()` 狙いの grep が rustfmt の折り返しで
  3 件落とすことを実測）、移行 5 件・据え置きの分類が本計画と**完全に一致**した。
  落ちていた 3 件（`launcher_controller.rs:829` folder worker / `indexing.rs:74` / `indexing.rs:156`）は
  いずれも据え置き・射程外であり、移行集合を変えない
- **plan ∖ 導出（スコープ過剰候補）: 0 件**
- **判断の不一致: 0 件**

### 要対処（4 件・すべて計画へ反映済み）

- **ソーステキスト検査の doc の数え上げ**（`launcher_controller.rs:1845-1848`）——移行で「この 2 つが
  対象外」が偽になるが**検査は緑のまま腐る**。専用節を置き Phase 1 の作業項目へ追加 —
  根拠: 当該 doc を再読して確認（帰属先が `fn instant_prefix(` / `fn auto_hide_enabled(` になり
  `entry_points` に一致しない）
- **`get_instant_commands` の fallback 非対称**——「この値で初めて走る行」の列挙を不変条件 4 へ追加 —
  根拠: `commands/instant.rs:10` が `app.state()`（panic）、ほかの移行対象はすべて `try_state`
- **`search_state.rs:492` は消さず縮める**（#1079 の費用訂正の根拠）——変更ファイル表へ明記 —
  根拠: 当該 doc の文脈を再読
- **`docs/architecture.md:228` は字面訂正でなく再導出が要る**——Phase 4 に既載（強調を追加）

### 軽微

- `docs/development-principles.md:184` の `read_config(` 対象外経路の言及（`lang()`）——
  **変更しない。** 本文を読んだところ #1112 の状況を記す**存在形**であり、唯一例を主張していない
  （「`read_config(` が対象外の経路（`lang()`）で（…）在り」）。移行後も真
- `ADR-blur-grace-single-field-state-machine.md:22` の「毎フレーム 2 回」は既に偽——
  **直さない**（凍結された歴史・`ADR-adr-frozen-history`）。生きた層の doc へ書く（専用節）
- `window_coordinator.rs:183-186` は既に古く、移行でむしろ整合する——Phase 2 に既載
- **`PERFORMANCE.md` の A/B 表に行を足さない**——測っていない値を入れると器が腐る

### 未検証

- **ソーステキスト検査が緑であること**——独立導出は静的導出にとどまるが、**3b の敵対枠が実際に
  移行を当てて `cargo test -p snotra activation_uses_frame_values` を実行し green を実測**、
  `git checkout --` で復元済み。**別枠組み 2 つが同じ結論**（片方は実測）
- `G-heading-refs` / `G-stale-identifiers` が新しい規範文面に当たるか——文面未確定のため未検証。
  Phase 4 の `governance:check` が捕捉する
- `smoke:egui` が show 経路の変更後も緑か——Phase 5 で実測する
- **性能（意図的）**——本 issue は実測を要求せず、#1036 の計器は撤去済み。移行の費用がほぼゼロ
  である以上、測ってから決める必要がないと裁定した（research.md §11-3）
- **解消済み**: `Mutex<Engine>` の guard を返すヘルパーの不在（母集団の取りこぼし懸念）——
  `grep -rn MutexGuard src-tauri/src snotra-core/src` が **0 件**で確認

### スコープ改訂（5c 提示後）への追随

ユーザーが据え置き 2 件の撤回を指示した（「6 件すべて移そう」）。**対象シンボルが変わったため
`/plan-review` の再実行要否を判定した——再実行しないと決めた。**

- **理由**: 追加された 2 件（`launcher_controller.rs:503` / `:726`）は、独立導出レビューが
  **A の表で既に列挙し、実行文脈（egui フレーム内）と呼び出し元（`activate:245` /
  `shift_activate:656` / Enter）まで特定していた**。新しいファイルもシンボルも増えておらず、
  変わったのは「移行するか据え置くか」の**partition だけ**である
- **代わりに主エージェント自身が測った 3 点**（一次証拠）:
  1. **ソーステキスト検査の帰属** — 2 件の錠は `fn execute_instant_selected(`（489 行）と
     `fn resolve_tools(`（721 行）の中に在り、**どちらも `entry_points` の 3 本ではない**。
     `read_config(` を書いても帰属は起動の入口へ落ちない
  2. **`find_matching_tools` の純粋性** — `snotra-core/src/opener.rs:59` を読み、
     `to_lowercase` / `replace` / `rfind` / `starts_with` のみで**錠も I/O も無い**ことを確認。
     `read_config` のクロージャ内で呼んでよい
  3. **新たに偽になる doc 2 件** — `launcher_controller.rs:484`（「engine ロック内」）と
     `:717-720`（「lock は解決の間だけ保持」）。**専用節を置き Phase 1 の作業項目へ追加した**

### 判断

- 実装着手: **人間の裁定待ち**（規範差分の逐語）

## 人間レビュー

- [x] 承認済み — 2026-08-17 / 問い: "移行対象を 7 か所（`egui_shell/` の 6 件 + `commands/instant.rs`）とし、上の規範差分を当てる形で `workspace/plan.md` を承認いただけますか。" / 回答: "承認"

先行して、スコープの改訂も同じ経路で受けている。

- 問い: "この規範差分の逐語案と、受容残余 2 件（`execute_instant_selected` / `resolve_tools`）を据え置く形で、`workspace/plan.md` を承認いただけますか。" / 回答: "6件すべて移そう。ユーザーからみた挙動に影響はないよね？"
  → 据え置きの撤回として反映し（移行 5 → 7 か所）、受容残余の但し書きを規範差分から削除した。
  併せて問われた挙動への影響は「読む値は同一・待ち時間は変わる（未測定ゆえ改善とは書かない）・
  `get_instant_commands` の panic → fallback は到達不能経路のみ」と回答済み。
