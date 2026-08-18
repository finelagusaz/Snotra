# #1123（射程拡大版）— 独立導出による変更範囲

**導出条件**: `workspace/` 配下を一切読まず、コード・規範文書・`gh issue view 1123` のみから導出した。issue が「別勘定」として射程外にしている `config_watcher` の旧 config 読みも**今回の対象に含める**前提で書いている。

**やること（前提として与えられた WHAT）**
1. `Engine::config` を呼ぶ src-tauri の 4 か所を engine 錠を経ない読みへ移す
2. `#[expect(clippy::disallowed_methods, …)]` を 4 件すべて削除
3. `clippy.toml` の群 3（`Engine::config` の禁止）とガバナンス検査側のカナリア登録を撤去
4. `Engine::config` の可視性を落として製品コードから呼べなくする
5. 条項から「例外」「射程外」「弁別子」を消す

挙動は変えない。性能改善も狙わない。

---

## 0. 着手前に必ず通す関門（機構ではなく規範）

- **これはセーフティネットの変更である。** ルート `CLAUDE.md`「最重要ルール 2」により、Claude が単独で判断してはならない。撤去するもの: `clippy.toml` 群 3（lint による禁止）・`REQUIRED_DISALLOWED_METHODS` のカナリア 1 件・`#[expect]` 4 件（これ自身が「禁止が実効を失ったら鳴る」カナリアである）。**代わりに入るのはコンパイルエラー**だが、置換であって単純な撤去ではないことを合意の文面に含めること。
- **`AGENTS.md`「条件別チェック」の該当トリガーは 4 本立つ**: セーフティネット変更 / ガバナンス文書変更 / 対称ペアではないが「重複した読み・冗長に見える状態を束ねる」 / **`/race-check`**（`resolve_opener` が錠を替える。issue 本文も明示している）。
- ⚠️ `.claude/rules/safety-nets.md` の frontmatter `paths` は `scripts/*.mjs` と `scripts/lib/**` を持つが **`scripts/governance/checks/**` を持たない**。`G-clippy-disallowed.mjs` を編集しても自動配送されない（`scripts/governance-check.mjs` を触れば配送される）。手動参照すること。

---

## 1. 変更が必要なファイル（パス + 変更理由）

### 1-A. コード（挙動を伴う移設）

| # | パス | シンボル | 変更理由 |
|---|---|---|---|
| 1 | `src-tauri/src/commands/launch.rs:105-113` | `resolve_opener` | `state.engine.lock()` → 廃止。`cfg` を engine 錠を経ない読みへ。`#[expect]` 削除。**doc（`is_dir()` は必ず engine ロックの外、#524）の書き直しが必須**——engine 錠がこの関数から消えるので「ロック内は純 CPU に保つ」の文が指す対象が無くなる。ただし**「is_dir を guard より前に置く」制約は残す**（下記 5-C） |
| 2 | `src-tauri/src/commands/launch.rs:158-171` | `resolve_all_openers` | 同上。doc「`resolve_opener` と対称。理由はそちらの doc 参照、#524」も 1 と同時に直す |
| 3 | `src-tauri/src/commands/icon.rs:14-23` | `ensure_icon_cache_loaded_if_enabled` | 同上。行内コメント「config は単一の engine ロック内で読み、icon cache のロックを取る前に解放する（engine ロックを跨いで I/O しない）」が偽になる。**「icon cache 錠を取る前に config の guard を落とす」という制約自体は残す**（下記 5-B） |
| 4 | `src-tauri/src/config_watcher.rs:87-91` | `apply_config_change` の `old_config` | 同上。`#[expect]` の reason は「弁別子が他の例外と違う——スレッドではなく手続きゆえの射程外」と書いており、**弁別子ごと消える今回はこの文字列が丸ごと死ぬ** |
| 5 | `snotra-core/src/engine.rs:228-241` | `Engine::config` | 可視性を落とす（判定は §2）。doc 234-238 行の「製品 crate（`src-tauri`）ではこの綴りを `clippy.toml` が禁じている（#1122）……改名・削除するならその禁止パスも同じ変更で直すこと……例外地点の `#[expect]` が鳴りうる」は**全文が偽になる**ので書き直す |

### 1-B. 機構（lint とガバナンス）

| # | パス | 箇所 | 変更理由 |
|---|---|---|---|
| 6 | `src-tauri/clippy.toml:86-145` | 群 3 のコメントブロック全体 | 撤去 |
| 7 | `src-tauri/clippy.toml:158` | `disallowed-methods` の `Engine::config` エントリ | 撤去 |
| 8 | `src-tauri/clippy.toml:37` | **群 1 のコメント内の相互参照**「**群ごとに違う**——群 3 は `#[expect]` を要求する（理由は同群のコメント）」 | 群 3 が消えるので参照先が消える。**`.md` だけ見ると落ちる典型**（`.toml` のコメント） |
| 9 | `src-tauri/clippy.toml:157` 直前 | 区切りコメント「--- ここから下は上の 2 群とも別の関心である（engine 錠の待ちがフレームに乗ること） ---」 | 群ごと消える |
| 10 | `scripts/governance/checks/G-clippy-disallowed.mjs:65-66` | `REQUIRED_DISALLOWED_METHODS` の群 3 コメント + `"snotra_core::engine::Engine::config"` | カナリア撤去 |
| 11 | `scripts/governance/checks/G-clippy-disallowed.mjs:20-22` | ヘッダの「群 2・3 は snotra-core 側の改名が契機になる」「群 3（#1122）だけは前提 (3) を例外地点の `#[expect]` が補うが、それが成り立つ条件は clippy.toml の群 3 が正本である」 | 前提 (3) を補う足が消える。**この 3 行を残すと、緑の意味を偽って強く見せる**（`.mjs` の規範言い換え） |
| 12 | `scripts/governance/checks/G-clippy-disallowed.test.mjs:34-35` | fixture `CLIPPY_OK` の群 3 行 + コメント | fixture の据え置きはカナリアテスト（`clippyDisallowedCount(base) === REQUIRED_DISALLOWED_METHODS.length` と、実リポジトリ照合の `toHaveLength`）で**赤くなる**＝沈黙しない。ただし直さないと `npm test` が落ちる |

### 1-C. 散文（この変更で偽になるもの）

| # | パス | 箇所 | 変更理由 |
|---|---|---|---|
| 13 | `src-tauri/CLAUDE.md:57` | **条項本体（正本）** | 「例外」「射程外」「弁別子」「動機と判定の区別」「非自明なケースの列挙（`on_event_loop` / tao リスナー / `app.listen` / `get_instant_commands` / hotkey 先例）」「機構がある（#1122）」「除外したメソッドと残余の正本は clippy.toml 群 3」「却下した弁別子は ADR」がすべて対象。**§4 の「残さなければならないもの」を参照** |
| 14 | `docs/architecture.md:231` | 「**#1032 の残余は #1076 が寄せた**（例外の弁別子と射程は `src-tauri/CLAUDE.md`「モジュール構成」の当該条項が正本——ここに言い換えを置かない）」 | 「例外の弁別子と射程」という指し先が消える。**参照の形を保ったまま指す語を変える**（言い換えを持ち込まない） |
| 15 | `docs/build-commands.md:30` | 末尾「**この独立性は禁止本体の話である**——群 3（#1122）が例外地点の `#[expect]` に持たせた「禁止が実効を失ったら鳴る」足のほうは `-D warnings` に依存する（条件と理由の正本は `clippy.toml` の群 3）」 | 群 3 も `#[expect]` も消えるので全文が死ぬ。**`.md` だがカテゴリ F 検査の射程外の意味的整合**（G-references はパス実在しか見ない） |
| 16 | `src-tauri/src/commands/instant.rs:26-31` | 「**#1032 条項の例外が名指すのは egui フレームの外で行う読み（icon worker・folder worker・tray スレッド）であって `commands/` というディレクトリではない——弁別子はフレームを止めるかである**」 | 例外も弁別子も消える。**この doc の本来の主張（「`commands/` に在るが毎打鍵フレームの中で走るので `read_config` を通す」）は残す**——移設後は「そもそも全部 engine 錠を経ない」で足りるので、根拠の説明だけ縮む |
| 17 | `snotra-core/src/engine.rs:228-241` | `config` の doc（1-A #5 と同一箇所） | 上記のとおり |
| 18 | `docs/adr/ADR-config-read-exception-discriminator.md` | — | **編集しない（測って確定した）**。`ADR-adr-frozen-history.md` を読んだ: 「ADR 本文は決定日時点の世界の記述として凍結」し、決定を覆すときも**元 ADR は編集しない**（同 ADR は `ADR-stale-identifier-detector-scope` の正当化を覆したうえで「同 ADR は凍結ゆえ編集しない——それ自体が本契約の初適用である」と明記している）。ゆえに案 G の「可視性を狭めるのは不可」が偽になっても**そのまま残す**。→ 代わりに §1-E の判断が要る |
| 19 | `src-tauri/src/egui_shell/mod.rs:410-421` | `read_config` の doc「UI が config を読む唯一の口（#1032）」 | §4-B の設計判断次第。`state.config` を直読みする形を採るなら「唯一の口」が偽になる |

### 1-E. 新しい ADR を書くか（判断が要る・独立導出では決められない）

**この変更は `ADR-config-read-exception-discriminator` を生きた層から完全に孤立させる。** 当該 ADR への短縮引用は全リポジトリで **2 件しか無く**（`src-tauri/CLAUDE.md:57` の条項末尾 / `src-tauri/clippy.toml:91`）、**今回の変更はその両方を消す**（条項から弁別子を消し、群 3 を撤去するため）。

- **`npm run governance:check` は赤にならない。** `G-adr-citations` が守るのは「生きた層 → ADR の引用が実在の ADR を指すこと」の一方向だけで、**被引用ゼロの ADR を咎めない**（`ADR-adr-frozen-history` の「実在の辺だけを守る」設計そのもの）。⚠️ **つまりこの孤立は沈黙する。**
- **`AGENTS.md`「ドキュメント参照」は ADR を「否定の知識（なぜ B を却下したか）が生じた決定のみ」と定める。** #1123 の決定は「例外という装置を置かない」であり、却下されるのは**弁別子で切るという方式そのもの**（＝旧 ADR の決定）である。これは新しい否定の知識に当たるように見えるが、**新 ADR を書くか / 条項の 1 行で足りるかは合意が要る。**
- 新 ADR を書くなら、そこに残すべき否定の知識の候補: (a) 4 か所を移すのに `read_config` の口へ統一せず直読みを許した理由（§4-C の β・γ の却下）、(b) 可視性を `pub(crate)` ではなく `#[cfg(test)]` にした理由（§2）、(c) compile-fail テストを置かない理由（§7）、(d) 撤去条件が `pub(crate)` と書いていた欠陥（§3 欠陥 1）。
- 新 ADR を書かないなら、**旧 ADR は「もう誰も指さない歴史」として残る**——それは凍結契約と整合するが、**読者が旧 ADR に辿り着く経路が消える**ことは明記に値する。

### 1-D. 触らないと判定したもの（根拠つき）

- **`docs/adr/ADR-clippy-disallowed-enforcement.md`** — 全文を読んだ。守る命題は「レベルは構造へ、内容は静的検査へ」であり、名指しは群 1 のカナリア 7 件と受容残余 2 の `#1067`（群 2）まで。**群 3 に一度も言及していない**ので偽にならない。
- **`docs/adr/ADR-race-check-predicate-and-norm-hardening.md`** / **`ADR-two-class-reader-discrimination.md`** / **`docs/design/2026-05-31-coherence-staleset.md`** — `live-read` の語で引っ掛かるが、指すのは述語の書き方・規範の計測・StaleSet 契約であって本条項ではない。
- **`PERFORMANCE.md`（509-577 の #1032 節）** — A/B の実測記録。過去の測定であり、今回は性能を変えないので偽にならない。
- **`SPEC.md:413-425`** — 「live-read で即時反映」という**挙動**の記述。読みの経路を変えるだけで挙動は同じなので同期不要（＝これは仕様変更ではなく内部規範の変更である）。
- **`docs/superpowers/plans/**` と `specs/**`（日付つき）** — 触らない。**慣行は明文である**（測って確定した）: `scripts/governance/checks/G-adr-citations.mjs` のヘッダが「`docs/superpowers/` は歴史資料（#589 で非規範化）ゆえ母集団外である。旧番号のパスが残るが、**その時点の事実の記録であり、書き換えると当時を偽ることになる**」と逐語で書いている。ADR と同じ凍結契約。
- **`RETROSPECTIVE.md` / `README.md` / `README.en.md` / `CONTRIBUTING.md`** — `1122|1123|1126|弁別子|expect\(clippy|Engine::config|live-read|clippy.toml` の 8 語で走査して **0 件**（root 直下は他の走査の母集団から漏れやすいので独立に測った）。
- **`.claude/rules/src-tauri.md` / `snotra-settings.md` / `race-check/SKILL.md` / `code-reviewer.md`** — `live-read` は並行境界の述語として出るだけで、本条項の言い換えを持たない（写しを置かない設計が効いている）。
- **`src-tauri/src/egui_shell/launcher_controller.rs` / `view.rs` / `indexing.rs` / `startup.rs` のソーステキスト検査** — `include_str!` の母集団は**各自のファイル**に閉じており、`launch.rs` / `icon.rs` / `config_watcher.rs` を読まない。今回の移設で赤にも緑にもならない。
- **`src-tauri/src/egui_shell/view.rs:516` の `#[allow(clippy::disallowed_methods)]`** — 群 1（`apply_exact_hit_test_style`）。無関係。

### 導出したシンボル一覧

移す読み: `resolve_opener` / `resolve_all_openers` / `ensure_icon_cache_loaded_if_enabled` / `apply_config_change`
可視性を変える: `snotra_core::engine::Engine::config`
消す注釈: 上記 4 関数の `#[expect(clippy::disallowed_methods, reason = …)]`
消す機構: `clippy.toml` 群 3 エントリ / `REQUIRED_DISALLOWED_METHODS` の `"snotra_core::engine::Engine::config"` / test fixture の同行
影響を受けるが変えないシンボル: `Engine::config_handle`（`AppState` 構築の口・そのまま）/ `Engine::update_config`（書きは engine 錠の内側のまま）/ `egui_shell::read_config`

---

## 2. `Engine::config` の可視性 — 判定と根拠

### 呼び出し元の全列挙（2 綴りで自分で測った）

`.config(` と `Engine::config`（UFCS / パス式）の 2 通りを `src-tauri/` `snotra-core/` `snotra-egui-runtime/` `snotra-settings/` の `--include=*.rs` へ当てた。**7 件しかない。**

| 位置 | 種別 |
|---|---|
| `src-tauri/src/commands/icon.rs:21` | 製品（今回消える） |
| `src-tauri/src/commands/launch.rs:111` | 製品（今回消える） |
| `src-tauri/src/commands/launch.rs:166` | 製品（今回消える） |
| `src-tauri/src/config_watcher.rs:91` | 製品（今回消える） |
| `snotra-core/src/engine.rs:476` | `#[cfg(test)] mod tests`（`config_returns_current_config`） |
| `snotra-core/src/engine.rs:485` | 同（`update_config_changes_config`） |
| `snotra-core/src/engine.rs:590` | 同（config を書き換えて検索を見る test） |

`mod tests` は同ファイル 333 行から。**残る 3 件はすべて `engine` モジュールの子モジュールである。**
- `snotra-core/tests/`（4 本: `dir_stat_cost` / `memory_footprint` / `path_query_cost` / `search_frame_cost`）に `.config()` は **0 件**（`update_config` は使うが、それは別シンボルで `pub` のまま）。
- `snotra-core/benches/` は存在しない。`src-tauri/tests/` も存在しない。
- 他 crate（`snotra-egui-runtime` / `snotra-settings`）は `snotra-core::engine::Engine` の config を一度も読まない。

### 判定: **`#[cfg(test)] pub(crate) fn config(...)`**（`#[cfg(test)]` を付けることが要点）

**根拠 1 — `pub(crate)` だけでは `-D warnings` の下で `dead_code` に落ちる（実測した）。**
使い捨て crate（scratchpad）で測った:

```rust
pub struct E{v:u32}
impl E{ pub fn new()->Self{Self{v:1}}
        pub(crate) fn cfg(&self)->u32{self.v} }
#[cfg(test)] mod t{ use super::*; #[test] fn a(){ assert_eq!(E::new().cfg(),1); } }
```
→ `cargo build` が `warning: method `cfg` is never used`（`#[warn(dead_code)]` … on by default）。
ルート `Cargo.toml` の `[workspace.lints]` は **`rustdoc` と `clippy` しか持たず `[workspace.lints.rust]` が無い**（#1126 のコミットメッセージが「新設は見送った」と明記）ので、`dead_code` は warn 既定のまま。**赤くなるのは `ci.yml` と `.claude/hooks/post-edit.mjs` が渡す `-D warnings` の下**——つまり `cargo clippy --workspace --all-targets -- -D warnings`（カテゴリ A の必須行）が落ちる。

**根拠 2 — `#[cfg(test)]` を足すと消える（同じ crate で実測した）。**
同じ scratch crate で `#[cfg(test)] pub(crate) fn cfg`（＋非 test の利用者を 1 本）に変え、`RUSTFLAGS=-Dwarnings cargo build` と `cargo test` の**両方が緑**であることを測った。

**根拠 3 — 同一 crate に先例が 3 つあり、うち 1 つは `clippy.toml` 自身が「禁止を足すより公開面を減らすほうが強い」と名指している。**
- `PrebuiltIndex::new`（`engine.rs:54`）— `#[cfg(test)] pub fn`。doc が「かつては『統合テストが外部クレートとしてリンクするため締められない』と書いていたが、**その呼び出し元は 1 つも存在しなかった**（grep 実測）」と記す。**今回とまったく同じ形の判断である。**
- `Engine::replace_entries`（`engine.rs:265`）— `#[cfg(test)] pub(crate) fn`。**綴りまで一致する先例。**
- `sorted_prefix_len` — `clippy.toml` 群 2 のコメントが「`#[cfg(test)]` で閉じてあり、製品からは**呼べない**（禁止を足すより公開面を減らすほうが強い）」。

**根拠 4 — フィールド経由の第 3 の綴りは元から塞がっている（測った）。** `Engine` の `config` フィールドは `snotra-core/src/engine.rs:109` で `config: Arc<RwLock<Config>>` と**私有**で宣言されている（`pub` が無い）。ゆえに `engine.lock().unwrap().config.read()` は crate 外からコンパイルが通らず、**メソッドの可視性を落とせば「値としての config を engine 越しに読む綴り」は塞がる**。残る抜け道は `config_handle()`（`Arc` を返す・`AppState` 構築が通るので禁止できない）**1 本だけ**である。

**`pub(super)` / 素の private を採らない理由**: `engine.rs` は `snotra-core/src/engine.rs` ＝ crate 直下モジュールなので `pub(super)` は `pub(crate)` と同義になり、区別が読者に何も伝えない。素の private でも `mod tests` は子モジュールゆえ到達するが、**先例（`replace_entries`）が `pub(crate)` を綴っているので揃える**——分岐を増やす理由が無い。どちらでも `dead_code` の問題は `#[cfg(test)]` が解いている。

**帰結として書いてよいこと / 書いてはならないこと**
- 書いてよい: 「`src-tauri`（および `snotra-egui-runtime` / `snotra-settings` / `snotra-core/tests/`）からこのシンボル名は**呼べない**」。`#[cfg(test)]` は依存として link されるときには立たないので、これはコンパイラが拒む。
- **書いてはならない**: 「engine 錠越しの config 読みはもう書けない」。`PrebuiltIndex::new` の doc が明示的に戒めている型の全称であり、**偽である**——`engine.lock().unwrap().config_handle().read()` は今も書けて同じだけ待つ。**塞いだのは 1 つの綴りだけである。**

---

## 3. `clippy.toml` の撤去条件 — 逐語判定

### 逐語（`src-tauri/clippy.toml` 群 3 末尾・141-145 行）

> **この群には撤去条件がある。** `.config()` と UFCS の 2 通りで全 crate を走査した範囲では、`Engine::config` の呼び出しは src-tauri の例外地点と snotra-core 自身のテストにしか無い（2026-08-18 実測）。ゆえに**最後の `#[expect]` が消える変更**は、同じコミットでこの群のエントリと `REQUIRED_DISALLOWED_METHODS` の行を消し、`Engine::config` を `pub(crate)` へ落とすこと——そこから先は lint ではなく**コンパイルエラー**が規範を守り、この群も注釈も弁別子も要らなくなる。合図はマージ済みの事象（最後の注釈が消えること）であって、issue の開閉ではない。

（関連する前提条件・134-139 行）

> 1. **注釈を持つ地点が 1 つ以上残ること。** 例外がすべて `read_config` へ移れば注釈も（不履行に追われて）消えるので、この群は群 1・2 と同じ沈黙へ戻る。**規範が成功した瞬間に計器が黙る形であり、改善の向きにも嘘をつく**

### 手順が指定する内容（列挙）

1. 同じコミットで行うこと（3 点の同時性）
2. 群 3 のエントリを消す
3. `REQUIRED_DISALLOWED_METHODS` の行を消す
4. `Engine::config` を `pub(crate)` へ落とす
5. 合図は「最後の注釈が消えるという**マージ済みの事象**」（issue の開閉ではない）

### 欠陥・漏れ（4 件）

- **欠陥 1（実害あり）: 「`pub(crate)` へ落とす」だけでは `-D warnings` の下で赤くなる。** §2 の実測のとおり `dead_code` が立つ。撤去条件を字面どおり実行すると、**カテゴリ A の必須コマンドが落ちて実装が止まる**。正しくは `#[cfg(test)] pub(crate)`。**同じファイルの群 2 が `sorted_prefix_len` について正解（`#[cfg(test)]` で閉じる）を書いているのに、群 3 の撤去条件はそれを引き継いでいない。**
- **欠陥 2（沈黙しない漏れ）: テスト fixture が列挙から漏れている。** `G-clippy-disallowed.test.mjs` の `CLIPPY_OK` は群 3 の行をリテラルで持ち、カナリアテストが `toHaveLength(REQUIRED_DISALLOWED_METHODS.length)` と実リポジトリ照合を行う。**据え置くと `npm test` が赤になる**ので沈黙はしないが、手順の列挙としては不完全。
- **欠陥 3（沈黙する漏れ）: 散文の掃除が一切列挙されていない。** §1-C の 7 か所（条項・`clippy.toml` 群 1 の相互参照・`G-clippy-disallowed.mjs` ヘッダ・`build-commands.md:30`・`architecture.md:231`・`instant.rs:28-31`・`engine.rs:234-238`）はどの機構も見ない。**とくに `clippy.toml:37` と `G-clippy-disallowed.mjs:20-22` は「`.md` だけを見る掃除」では原理的に落ちる。**
- **欠陥 4（射程の穴）: 「最後の `#[expect]` が消えること」を合図にしているが、その 4 件は自分で消しに行くものであって受動的に消えるものではない。** #1123 が示すとおり移設は**能動的な判断**であり、`#[expect]` の不履行が鳴って初めて気づく類ではない。合図の設計としては、**issue の開閉を避けた点は正しい**（`scaffold-removal-condition-self-reference` の型の自己参照を避けている）が、**駆動力を持たない**——誰も #1123 を評価しなければ永久に発火しない。これは撤去条件の欠陥というより「統治だけを得る変更に自動発火の合図は原理的に置けない」という限界であり、**受容が妥当**。

---

## 4. 条項の書き直し — 残すもの・消えるもの・判断が要る点

### 4-A. 消える（指示どおり）

「例外」という装置・「射程外」（`config_watcher`）・「弁別子は走るスレッドである」・「動機と判定を分けよ」・非自明なケースの列挙（`on_event_loop` / tao window-event リスナー / `app.listen` の emit 元 / `get_instant_commands` / hotkey 先例）・「どこで走るかは呼び出し元を辿って決めること」・「機構がある（#1122）」・「`#[expect]` で開ける」・ADR への却下弁別子の参照。

### 4-B. **残さなければならないもの（消すと情報が失われる）**

- **害の記述**: worker は `engine.search` の間じゅう `Mutex<Engine>` を握る（実運用点で 40〜95 ms）/ `read_window_width` 単独 43,939 µs / 60fps 超過フレーム 11 本 / A/B は `PERFORMANCE.md`。
- **「射程は読みだけである」**: 書き込み（`update_config`）は engine 錠の内側に残す。理由は `complete_index_drain` の原子性（`snotra-core/CLAUDE.md`）。**これは今回変えない不変条件そのものなので、条項が失うと守る者が居なくなる。**
- **「読みの中で lock を取る操作を書かないこと」**: `read_config` の doc にも在るが、条項側の 1 文も生きている（`read_config` を経ない直読みが増えるなら、むしろ条項側が要る）。
- **`clippy.toml` 群 3 が持っていた「規範は機構より広い」の残余**（群ごと消えるので**行き場を失う**——条項へ移すのが自然）:
  - `engine.lock()` 越しに `config_handle()` を取り直す形は同じだけ待つのに何も鳴らない
  - `Engine` の内側で config を読む他のメソッド（`search` / `recent_history` / `begin_index_drain`）を錠越しにイベントループから呼ぶ形も同じ害を持つ。**これは仮定ではない**——`launcher_controller` の `/history` 枝（`recent_history`）と `record_folder_expansion` が現にその形で、`on_enter` の同期 `search` は #1004 が明示的に正当化している
  - **可視性を落としても塞がるのは 1 つの綴りだけである**（§2 の「書いてはならないこと」）
- **1 フレーム 1 回の読み**（`read_visual`）の申し合わせは別条項（56 行）が持つので重複させない。

### 4-C. ⚠️ 決めなければならない設計判断（黙って選ばないこと）

**「engine 錠を経ない読み」の綴りをどうするか。** 現状 `AppState.config` を読む製品コードは **`egui_shell::read_config` の 1 本だけ**（`mod.rs:429`）で、`read_config` は `&tauri::AppHandle` を要求する。移す 4 か所のうち:

- `resolve_opener` / `resolve_all_openers`: 引数は `&AppState`。**`AppHandle` を持たない**
- `ensure_icon_cache_loaded_if_enabled`: 引数は `&State<AppState>`。**`AppHandle` を持たない**
- `apply_config_change`: `&AppHandle` を持つ（`read_config` を使える唯一の 1 件）

選択肢は 3 つ。**どれを採るかで条項の文面が変わる。**

| 案 | 内容 | 得 | 損 |
|---|---|---|---|
| **α** | 4 か所とも `state.config.read().unwrap()` 直読み | シグネチャ変更なし・最短 | 条項の「`read_config` を通す」が偽になる（**両方の綴りを条項が名指す必要がある**）。`read_config` の doc「UI が config を読む唯一の口」も偽 |
| **β** | `AppState` に読みヘルパー（例: `AppState::read_config` / `config_snapshot`）を足し、`egui_shell::read_config` をその薄いラッパーにする | 口が 1 つに戻る・条項が 1 文で済む | 新 API の導入＝`AGENTS.md`「関数・型を新規定義」トリガー（`/dry-check` + 呼び出し点の移行を 1 タスクに束ねる）。**「挙動を変えない」以上の変更になる** |
| **γ** | 4 か所すべてに `AppHandle` を通して `read_config` を使う | 口が 1 つのまま | `resolve_opener` 系 3 本のシグネチャ変更＝**呼び出し元（tray・egui 経路）まで波及**。挙動不変の変更としては最も大きい |

**推奨は α**（挙動を変えない・射程が最小・issue の「`&AppHandle` すら要らない形にできる」という観測と一致）。**ただし条項は「`read_config` を通す」ではなく「engine 錠を経ない（`AppState.config` を読む）」へ書き直すことが前提**——これは指示 5 の「条項が 1 文になる」とも整合する。**β を採るなら別 issue に切ることを勧める**（統治の変更に API 追加を混ぜない）。

---

## 5. 不変条件と、それぞれを何が検知するか

| # | 不変条件 | 何が検知するか |
|---|---|---|
| A | **書き込み（`update_config`）は engine 錠の内側に残る** — `complete_index_drain` の原子性がそこに依る | **型**（`&mut Engine` を要求するので `Mutex<Engine>` 経由でしか呼べない）。今回触らない |
| B | **`AppState.config` と `Engine` の config は同じ `Arc`** | `src-tauri/src/state.rs` の `app_state_config_is_the_same_arc_the_engine_holds` テスト（既存・今回も走る） |
| C | **UI の読みは engine 錠の保持中に完了する** | `state.rs` の `ui_reads_config_while_the_engine_lock_is_held` テスト（既存） |
| D | **`ensure_icon_cache_loaded_if_enabled` は config の guard を落としてから icon cache 錠を取る** | **誰も見ていない（規範のみ）**。移設後も「2 値を読んで guard を落とす」ブロック構造を保つこと。⚠️ 検知器なし |
| E | **`resolve_opener` / `resolve_all_openers` は `is_dir()`（最大 21 秒の SMB 待ち）を guard より前で行う（#524）** | **誰も見ていない（規範のみ）**。engine 錠から config の `RwLock` read guard へ替わっても**制約は消えない**——read guard を握ったまま 21 秒止まると `update_config` の writer（config_watcher）が待つ。⚠️ 検知器なし・**doc の書き直しでこの理由を失わないこと** |
| F | **`icons_off` の破棄は `update_config` の後に撃つ（順序が correctness）** | `config_watcher.rs:148-157` の長文コメントが正本。**今回は触らない**——可視性が伝わる経路は `RwLock` の write（`update_config` の内側）であって engine の `Mutex` ではないので、旧 config の読みを移しても順序制約と受容残余の窓は変わらない |
| G | **`ReadFailed` では何も適用しない** | `should_apply_config_change` + そのユニットテスト。今回触らない |

### 新しい競合・窓・順序制約は生じるか — **生じない。根拠 3 点**

1. **同じ `Arc` を読む。** `state.config` と `engine.config` は同一の `RwLock<Config>`（B のテストが測っている）。読む値は 1 ビットも変わらない。
2. **旧 config の読みと `update_config` は、今日すでに別々の錠取得である。** 条項自身が「**錠は共有していない**（旧 config の読みは `.clone()` の一時値ゆえ文末で guard が落ち、書き込みは同じ関数の後段で取り直す）・2026-08-18 実測」と書いており、両者の間には**hotkey 往復の `recv_timeout(2s)` すら挟まる**（`config_watcher.rs:112-124`）。移設で失われる原子性は**元から無い**。
3. **製品の書き手は 1 か所しかない。** `update_config` の呼び出しを全 crate で数えると製品は `config_watcher.rs:146` の 1 件のみ（他は `state.rs` / `engine.rs` / `snotra-core/tests/path_query_cost.rs` のテスト）。旧 config を読んでから書くまでの間に別の書き手が割り込む経路が無い。

**むしろ減る**: `resolve_opener` は engine `Mutex` を取らなくなるので、tray からの起動が検索 worker の走査（40〜95 ms）を待たなくなる。**これは性能改善だが、狙って測る対象ではない**（フレームに乗らない経路なので `PERFORMANCE.md` へ書く値ではない）。

### ⚠️ 残る不確かさ（§8 にも再掲）

- `notify::recommended_watcher` のコールバックが**並行に配送されうるか**を測っていない。並行なら `apply_config_change` が 2 本同時に走る窓が**今日から**在り、今回の移設はそれを増やしも減らしもしない（今日も apply 全体は原子的でない）が、「変わらない」と言うために測る価値はある。

---

## 6. `cargo doc`（intra-doc link）は壊れるか — **壊れない（測った）**

- `[`…config…`]` のブラケット形（intra-doc link）を `src-tauri/src/` と `snotra-core/src/` 全体で列挙した。ヒットは `[`crate::egui_shell::read_config`]`（5 か所）・`[`should_apply_config_change`]`・`[`crate::config::normalize_scan_path_key`]` のみ。**`Engine::config` をリンクしている箇所は 0 件**（散文中の `` `Engine::config_handle` `` は素のインラインコードでリンクではない）。
- カテゴリ A は `cargo doc --workspace --no-deps --document-private-items` を必須にしており、`--document-private-items` は private を文書化するが **`cfg(test)` の項目は rustdoc の対象外**（`--cfg test` を渡していない）。よって `#[cfg(test)]` 化で `Engine::config` は doc から消えるが、**リンク元が 0 件なので `broken_intra_doc_links = "deny"`（ルート `[workspace.lints.rustdoc]`）は鳴らない。**
- 逆向きの注意: 書き直す doc の中で `Engine::config` を**ブラケットでリンクしないこと**——`#[cfg(test)]` の項目へのリンクは `cargo doc` で切れて deny に当たる。**素のインラインコードで書く。**

---

## 7. 検証コマンドと、新しいテストの要否

### 必須（`docs/build-commands.md` が SSOT）

**カテゴリ A（`.rs` を変更するので必須）**
```
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra-core        # Engine::config の可視性を変えるので必須
cargo test -p snotra             # src-tauri の 4 か所を変えるので必須
cargo doc --workspace --no-deps --document-private-items
```
**カテゴリ F（`.md` とガバナンス文書を変更するので必須）**
```
npm run governance:check
```
**カテゴリ C 相当ではないが必須になるもの**: `G-clippy-disallowed.test.mjs` を変えるので
```
npm test                          # vitest（.claude/hooks + .githooks + scripts）
```

### 注意点（沈黙の読み方）

- `clippy.toml` / `*.md` / `scripts/**` の編集では **PostToolUse hook は何も走らせない**。沈黙は「合格」ではなく「何も走らなかった」。上の 3 本は**手で打つ**。
- **`clippy.toml` だけを変えて `cargo clippy` を打つと、cargo の fingerprint に入らないためキャッシュ replay で exit 0 が返りうる**（`clippy.toml` 冒頭の死経路 3。2026-08-18 の clippy 0.1.97 では再現しなかったが、手順は安全側の上位集合として残っている）。**`.rs` を触るか `cargo clean -p snotra` を挟む**——今回は `.rs` も同時に触るので実務上は満たされるが、**分割コミットで `.toml` だけを触る回があるなら注意**。
- `smoke:startup` / `smoke:egui`（カテゴリ C）は**不要**と判定する: 窓生成・表示順・hotkey・trace イベント名・機能削除のいずれにも当たらない。ただし `src-tauri/**` を含む変更なので **`Smoke` workflow（`e2e.yml`）が paths で自動起動する**——CI 側では結局走る。

### スキル

- **`/race-check` は必須**（issue 本文が名指し・`AGENTS.md` のトリガー「スレッド/窓をまたぐ共有状態」に当たる）。争点は §5 の D・E・F。
- **`/plan-review`** — 網羅性が要件（規範の全文監査 + 機構の撤去）なので Step 2b（独立再導出）が当たる。本ファイルがその 1 体分に相当する。
- **`/symmetric-check`** は不要（対称ペアの生成/破棄を触らない。`icons_off` の破棄は今回の差分の外）。
- **`/persistence-check`** は不要（on-disk 形式に触らない）。

### 新しいテストの要否 — **恒久テストは不要**

- **可視性はコンパイラが守る。** compile-fail テスト（`trybuild` 等）は**この repo に前例が無く**、入れると「コンパイラが自分の仕事をすること」を検査する層が増える。`ADR-no-test-only-injection-in-product-code` の向きとも合わない。
- **既存テストで足りるもの**: `state.rs` の 2 本（B・C）が「同じ Arc」「engine 錠の外で読み切れる」を測り続ける。`engine.rs` の 3 本（`config_returns_current_config` 他）は `#[cfg(test)]` 化後もそのまま走る。
- **`npm test` の fixture 修正は「新しいテスト」ではなく既存 fixture の追随**（§1-B #12）。
- ⚠️ **1 つだけ検討の余地**: 「`resolve_opener` が engine 錠を取らない」ことを固定する検査は**どの層も持たない**（ソーステキスト検査を足す手はあるが、`launcher_controller.rs` の前例が示すとおり帰属の濾過層が要り、費用が見合わない）。**受容する残余として明記することを勧める。**

---

## 8. 新機構（コンパイルエラー）が効くことの実測手順案

`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」と「稼働中のガードを弱めない——複製に変異を当てる」に従う。**変異は「本来防ぎたい回帰の姿」と同じ形にする**こと。

1. **正の注入（本命）**: 移設後の `src-tauri/src/commands/launch.rs` の `resolve_opener` へ、**#1032 が防ぎたい回帰そのものの形**を書き戻す:
   ```rust
   let cfg = state.engine.lock().unwrap().config();
   ```
   → `cargo check -p snotra` が **E0599（`config` というメソッドが無い）**で落ちることを確認する。
   **`E0624`（private で呼べない）ではなく `E0599` が出るのが `#[cfg(test)]` を選んだ証拠である**——依存としてリンクされる側では cfg が立たないのでシンボル自体が存在しない。**診断コードまで読むこと**（「赤くなった」だけでは private 化との区別が付かない）。
2. **強度の確認（機構の強化を測る）**: 同じ注入を**旧機構**（lint）で測った場合と比べる。`clippy.toml` の記録によれば、旧機構は **`cargo check` では診断そのものが評価されない**（rustc は `clippy::` ツール lint の expectation を見ない・#1126 実測）。新機構は `cargo check` で落ちる＝**発火する層が増えている**。この差は「弱くなっていないこと」の主張に必要なので、**両方を測って書く**。
3. **負の対照（誤爆していないこと）**: `cargo test -p snotra-core` が緑であること（`mod tests` の 3 件は `#[cfg(test)]` の下なので通る）。および `cargo doc --workspace --no-deps --document-private-items` が緑（§6）。
4. **別綴りが塞がっていないことの実測（規範 > 機構 を偽で書かないため）**: `state.engine.lock().unwrap().config_handle().read().unwrap()` は**今も通る**ことを 1 度測り、その結果を条項の残余として書く。**「engine 錠越しの config 読みは書けなくなった」と書かないための実測である。**
5. **ガバナンス側の変異（複製へ当てる）**: `G-clippy-disallowed` の撤去が「群 1・2 の見張りを弱めていない」ことを、`REQUIRED_DISALLOWED_METHODS` から**群 1 の 1 件**を消す変異を**複製（`CLIPPY_CONF_DIR` ではなく snapshot 入力）**へ当てて赤を確認する。稼働中のカナリアは触らない。**`npm test` を複製で走らせるなら `node_modules` を junction で張る**（張らないと vitest が起動前に exit 1 を返し、変異検知と区別できない）。
6. **注入は必ず巻き戻す。** 1 と 4 の変異は作業ツリーに残さない（`git status` で確認）。

---

## 9. ⚠️ 確信が持てない点

1. **（解消済み）ADR の扱い。** `ADR-adr-frozen-history.md` を読んで確定した——**旧 ADR は編集しない**（§1-C #18）。残る判断は「新 ADR を書くか」であり、そちらは合意事項として §1-E に切り出した。**⚠️ として残るのは 1 点だけ**: 旧 ADR が被引用ゼロになることを `governance:check` が咎めないため、**この孤立は誰にも報せない**。
2. **⚠️ `notify` のコールバック並行性**（§5 末尾）。測っていない。
3. **（解消済み）`docs/superpowers/**` を触らない判断。** `G-adr-citations.mjs` のヘッダが「歴史資料ゆえ母集団外／書き換えると当時を偽る」と逐語で持っていた（§1-D）。⚠️ として残るのは「ヒットした 12 本の中身を読んでいない」ことだけだが、凍結契約の下ではそれが偽になっても直す対象ではない。
4. **⚠️ 呼び出し元の列挙に LSP `findReferences` を使っていない。** grep の 2 綴り（`.config(` と `Engine::config`）で数えた。`MEMORY.md` の `prefer-lsp-references-over-grep` はこの用途で LSP を既定とする。**ただし #1126 が同一の 2 綴り走査で撤去条件を書いており、結果も一致している**（src-tauri の例外地点 + snotra-core のテストのみ）ので、独立に同じ数へ到達した点は交差検証になっている。それでも **re-export 経由や型エイリアス越しの綴りを落としうる**ことは残余として認める。
5. **⚠️ 「条項が 1 文になる」と書けるか。** §4-B に残すべき事実が 5 項目あり、**実際には 1 文にはならない**（害の記述・射程は読みだけ・読みの中で錠を取らない・機構より広い残余 3 点）。issue 本文の「条項は 1 文になる」は**楽観的である**——短くはなるが、消えるのは「弁別子という装置」であって条項全体ではない。**この期待値の差を合意の時点で共有すべき。**
6. **⚠️ `read_config` の doc「UI が config を読む唯一の口」の扱い。** §4-C で α を採ると偽になる。「UI の」に限れば真のままとも読めるが、**`resolve_opener` は UI ではない**ので、どこまでが「UI」かの線引きが曖昧になる。条項側で「engine 錠を経ない」を主語にすれば解けるが、`read_config` の doc も同時に直す必要がある。
7. **⚠️ 性能の副作用を「無い」と書けるか。** `resolve_opener` が engine `Mutex` を取らなくなるのは**測れば差が出うる**（tray 起動が worker の 40〜95 ms を待たなくなる）。「挙動は変えない」は真だが「何も変わらない」は偽。**`PERFORMANCE.md` へ書く値ではない**と判断したが、A/B を取れという指摘は正当でありうる。
