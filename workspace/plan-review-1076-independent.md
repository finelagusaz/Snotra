# #1076 独立導出レビュー — config live-read の `read_config` 移行

対象 issue: **#1076**（#1032 の規範の未移行残余を潰す）
導出日: 2026-08-17 / 導出者: 独立レビュー担当（作者の `workspace/` 成果物を見ない枠組み）

---

## 0. 汚染の開示（先に書く）

**このレビューは完全な独立導出ではない。** 最初の探索で `Grep`（`pattern: read_config`, `path: C:\workspace\Snotra`）を
リポジトリ全体に当てた結果、`workspace/plan.md` / `workspace/research.md` / `workspace/adversarial-1076.txt` の
**マッチ行が tool result として返り、目に入った**。具体的には plan.md の移行対象表（5 か所・行番号つき）、
research.md の節見出し、adversarial-1076.txt の「命題5」（ソーステキスト検査が赤にならないこと）の断片である。

**取った対処:**

1. 以降のすべての探索は `src-tauri/` / `snotra-core/` / `docs/` / ルート `*.md` へスコープを絞り、`workspace/` を母集団から外した。
2. **列挙の方法を、漏れた一覧に依存しない機械的な母集団へ据えた**（下の A-0）。結論は「誰かの表と突き合わせた」ものではなく、
   `engine.lock()` の全出現を自分で分類した結果である。
3. **漏れた結論はひとつも再利用していない。** とくに adversarial の「ソーステキスト検査は赤にならない」は、
   本レビューでは検査本体（`owners_of` / `activation_uses_frame_values_not_live_reads`）のロジックから
   独立に再導出した（下の C-1）。禁止された一時変更を行えない以上、実測ではなく静的導出である旨も明記した。
4. 本レビューが独立に導出した項目のうち、**漏れた一覧に無かったもの**は下で明示的に印を付けた（A-3 の `get_instant_commands` の
   実行文脈の意味づけ、B-2 / B-3 / B-6、C-2）。

読み手はこの開示を織り込んで採否を判断すること。

---

## A. 変更対象の完全な列挙

### A-0. 母集団の定義（なぜこれで漏れないか）

**リテラル `config()` を狙った grep では母集団にならない。** `Engine` の設定は
`snotra-core/src/engine.rs:109` で `config: Arc<RwLock<Config>>` として持たれており、
`search` / `recent_history` / `capture_folder_list_context` / `list_folder` /
`prepare_history_save_if_dirty` / `prepare_history_flush` / `begin_index_drain` /
`complete_index_drain` は**内側で `self.config.read()` を行う**（engine.rs:148, 159, 164, 219, 224, 284, 302）。
呼び出し元に `config()` のリテラルは出ない。

**しかし本件で問題になるのは内側の `RwLock` ではなく、外側の `Mutex<Engine>` である。**
規範 #1032 が塞ぐのは「検索 worker が `engine.search` の間じゅう握る `Mutex<Engine>` を、UI が config を読むために待つこと」であり、
`Engine` の**どのメソッドを呼ぶにも外側の `Mutex` を取る必要がある**（`AppState.engine: Mutex<Engine>`・`state.rs:80`）。

ゆえに母集団は次のとおり定義できる:

> **P = src-tauri crate 内の `engine.lock()` の全出現 ∪ `Engine` を `Mutex` の外で直接所有/借用する全地点**

この定義は「内側で config を読むメソッド」も自動的に覆う——そのメソッドを呼ぶには必ず `engine.lock()` が要るからである。
`config()` リテラルの有無に依存しない。

**列挙の方法（重要 — 素朴な `engine.lock()` grep では漏れる）:**

最初に打った `grep -rn "engine\.lock()"` は **rustfmt が折り返した複数行のチェーンを構造的に落とす**。
`s.engine` と `.lock()` が別の行に分かれる形（`auto_hide_enabled` / `instant_prefix` / `read_background` /
`follow_cursor` の 4 件がまさにこの形）は 1 行の正規表現に一致しない。
**これらを見つけられたのは偶然である**——たまたま `.config()` が 1 行に現れて別の grep に引っかかっただけで、
**複数行チェーンから config を内側で読むメソッド（`recent_history` 等）を呼ぶ形なら全 grep をすり抜けていた**。

ゆえに母集団は**全 `.lock()` 出現の分類**として取り直した:

```
grep -rn --include=*.rs -o "\.lock()" src-tauri/src/ | wc -l        → 77 件
grep -rn --include=*.rs -B2 "^\s*\.lock()" src-tauri/src/           → 折り返し 8 件
grep -rn "Engine::new|Engine::from_material|: Engine|&Engine|&mut Engine" --include=*.rs src-tauri/
```

77 件のレシーバを分類した内訳: **engine 関連 30 件**（`state.engine` 19 / `s.engine` 4 /
`app_state.engine` 1 / 折り返し 6）、**engine 以外 47 件**（`st.0`=UpdaterUiState 9 / `bridge`=PlatformBridge 7 /
`icons`・`icon_state`・`cache`・`s2` 8 / `proc_state` 4 / `sh.pending_hotkey_failure` 3 /
`shared.snapshot` 2 / `self.last_size` 2 / `self.clicked` 2 / `self.last_background` 1 / `cell` 1 ほか）。

**折り返し 8 件のうち engine のものは 6 件**で、**うち 3 件は最初の grep が実際に落としていた**（A-3 #25〜#27）。
いずれも分類は据え置きで、**移行対象 5 件の一覧は変わらなかった**が、
「その定義で漏れないこと」の示し方としては取り直した後のものが正しい。

`Mutex` 外の `Engine` 所有点は 3 件（`main.rs:237` 起動時の構築、`state.rs:77` /
`commands/system.rs:43` はどちらもテスト・既定構築）。
`snotra-egui-runtime` には `engine` が 1 件も無い（別 crate で `AppState` を知らない）。

**呼び出し元の追跡について**: 本環境では LSP ツール（`findReferences`）が deferred で、
プロジェクト規範（`AGENTS.md`「関数・型を新規定義／改名／導入」）は LSP を既定とする。
ここで grep へ落としたのは、追跡対象がいずれも `pub(crate)` / `fn`（private）で
**re-export 経路を持たず、同名の別物が crate 内に無い**ことを個別に確認できたためである
（`instant_prefix` は `search_state.rs` の `is_instant_prefix` と綴りが違い、
`resolve_tools` は `platform/tray.rs:177` の同名**引数**とヒットするが定義ではない——目視で分離済み）。

### A-1. 移行対象（要件 1・2 に該当）

| # | ファイル:行 | シンボル | 読む値 | 実行文脈（呼び出し元まで追跡） | 分類 |
|---|---|---|---|---|---|
| 1 | `src-tauri/src/egui_shell/launcher_controller.rs:730-742` | `LauncherController::auto_hide_enabled` | `general.auto_hide_on_focus_lost` | **egui フレームの中**。呼び出し元は `on_focus_changed:1236`（`let auto_hide = self.auto_hide_enabled();`）のみ。#745 の `BlurGrace` 合流以降、**armed かどうかに関わらず毎フレーム無条件で走る**（値渡しへ変わったため） | **要件 1 → 移行** |
| 2 | `src-tauri/src/egui_shell/launcher_controller.rs:750-763` | `LauncherController::instant_prefix` | `search.instant_command_prefix` | **egui フレームの中**。呼び出し元 3 本: `run_search:846`（folder drain / trailing poll）・`on_key_changed:1373`（**毎打鍵の changed エッジ**）・`on_enter:1438`（Enter の flush 判定前）。3 本のうち 2 本はフレーム内の毎打鍵・毎 poll であり、**起動 1 回きりではない** | **要件 1 → 移行**（ヘルパー本体を差し替えるので Enter 経路も同時に移る。要件の「据え置く」は禁止ではないので副作用として許容） |
| 3 | `src-tauri/src/commands/instant.rs:11-12` | `get_instant_commands` | `instant_commands`（`filter_instant_commands` へ渡す） | **egui フレームの中**。`commands/` に住むが、唯一の呼び出し元は `launcher_controller.rs:904`（`run_search_with` の `QueryIntent::Instant` 腕、コメントに「**毎打鍵同期**」と明記）。IPC 経路は #532 SU7 で消滅しており、**フレーム外の呼び出し元は現時点で 0 件** | **要件 1 → 移行。かつ要件 3 の弁別子をディレクトリからフレーム内外へ変えるべき理由の一次証拠**（`commands/` に在りながらフレーム内で毎打鍵走る） |
| 4 | `src-tauri/src/egui_shell/window_coordinator.rs:187-201` | `read_background` | `visual.background_color` | **show 経路**（フレーム外・イベントループスレッド）。呼び出し元は `show_egui_main:372` のみ | **要件 2 → 移行** |
| 5 | `src-tauri/src/egui_shell/window_coordinator.rs:226-236` | `position_on_target_monitor` 内の `follow_cursor` 読み | `general.follow_cursor_monitor` | **show 経路**（フレーム外）。`position_on_target_monitor` の呼び出し元は `show_egui_main:348` のみ（doc も「`show_egui_main` is the only caller」と明記） | **要件 2 → 移行** |

**移行対象は 5 件。** 同じ `show_egui_main` の中で `read_metrics`（:52）と `ime_off_on_show`（:428）は
**既に `read_config` 側に居る**——#1036 が同関数内で 2 件だけ移し、残り 2 件（#4・#5）を置いていったのが未移行の実体である。
`position_results_below_main:723` と `max_results:775` も既に `read_config`。

### A-2. 据え置き（要件 1 の例外「起動操作の 1 回きり」）

| # | ファイル:行 | シンボル | 読む値 | 実行文脈 | 据え置く理由 |
|---|---|---|---|---|---|
| 6 | `launcher_controller.rs:501-507` | `execute_instant_selected` | `instant_commands` の該当 action | **egui フレームの中だが Enter/クリックの 1 回きり** | 起動操作。要件 1 の明示的例外 |
| 7 | `launcher_controller.rs:725-726` | `resolve_tools` | `openers` | **egui フレームの中だが起動操作の 1 回きり**。呼び出し元は `activate:245`（Enter/クリック）と `shift_activate:656`（Shift+Enter）の 2 本のみ | 起動操作。要件 1 の明示的例外 |

### A-3. 据え置き（engine のデータを要る／フレーム外／config 適用側）

| # | ファイル:行 | シンボル | lock の目的 | 実行文脈 | 据え置く理由 |
|---|---|---|---|---|---|
| 8 | `launcher_controller.rs:708-710` | `record_folder_expansion` | `record_folder_expansion` + `prepare_history_save_if_dirty`（**内側で config を読む**・engine.rs:219） | egui フレームの中（→ 展開） | **history の書き込みが要る**。config だけを外へ出しても lock は消えない |
| 9 | `launcher_controller.rs:818` | `spawn_folder_load` の `capture_folder_list_context` | 内側で config を読む（engine.rs:164） | egui フレームの中 | **engine の history/index が要る**。移行不能 |
| 10 | `launcher_controller.rs:939-940` | `run_search_with` の `/r` 腕 `recent_history` | 内側で `effective_recent_limit` を読む（engine.rs:159） | egui フレームの中 | **history 本体が要る**。移行不能 |
| 11 | `launcher_controller.rs:1453-1454` | `on_enter` の flush `engine.search` | 索引の走査 | **egui フレームの中・Enter の 1 回きり** | 索引が要る。#1038 で受容済み（`docs/architecture.md`「検索フロー（入力 → 結果表示）」） |
| 12 | `egui_shell/search_worker.rs:58` | 検索 worker | `engine.search` + `entry_count` | **検索 worker スレッド**（＝この錠を 40〜95 ms 握る当人） | 移行対象ではない。**#1032 の原因側** |
| 13 | `commands/icon.rs:16-18` | `ensure_icon_cache_loaded_if_enabled` | `appearance.show_icons` + `icon_cache_cap()` | **icon worker スレッド**（`results_view.rs:201 spawn_icon_load` → `load_icon_pngs:50`）。フレーム外 | フレーム外の worker。移行しても UI フレームは早くならない。ただし B-6 参照 |
| 14 | `commands/launch.rs:55-57` | `record_and_save` | history 書き込み + `prepare_history_save_if_dirty` | launch worker スレッド | history が要る |
| 15 | `commands/launch.rs:105-110` | `resolve_opener` | `openers` | launch worker スレッド（フレーム外） | フレーム外。かつ現規範の例外文が名指す当の関数 |
| 16 | `commands/launch.rs:156-161` | `resolve_all_openers` | `openers` | **トレイスレッド**（Win32 メッセージループスレッド） | フレーム外 |
| 17 | `platform/tray.rs:34` | `recent_history_items` | 内側で `effective_recent_limit`（engine.rs:159） | **トレイスレッド** | history 本体が要る |
| 18 | `config_watcher.rs:87` | `apply_config_change` の `old_config` clone | `config().clone()` | **config 監視スレッド（適用側）** | **適用側**。#1032 の射程外（射程は「読み」だが、これは差分判定のための適用手続きの一部） |
| 19 | `config_watcher.rs:142` | `update_config` | **書き込み** | config 監視スレッド | **書き込みは engine lock の内側に残す**（規範が明示・`complete_index_drain` の原子性が依存） |
| 20 | `indexing.rs:26` | `start_index_build` の `mark_index_stale` | ledger 書き込み | 各種（config 適用・first-run・手動 rebuild） | 書き込み |
| 21 | `indexing.rs:137-139` | `drain_index` の `begin_index_drain` | 内側で `IndexInputs::from_config`（engine.rs:284） | **index build スレッド** | **snapshot の原子性が外側の錠に依存する**。移行禁止 |
| 22 | `main.rs:553-555` | `flush_persistent_state` の `prepare_history_flush` | 内側で config を読む（engine.rs:224） | 終了時（任意スレッド） | history が要る |
| 23 | `main.rs:237` | `Engine::from_material` | 起動時の構築 | **setup 前**（`Mutex` に入る前） | `AppState` 成立前。錠が存在しない |
| 24 | `state.rs:77` / `commands/system.rs:43` | `Engine::new` | テスト用・既定構築 | `#[cfg(test)]` / テストヘルパー | 対象外 |
| **25** | `launcher_controller.rs:829-833` | `spawn_folder_load` の `finalize_folder_list_unlimited` | 内側で config を読む（engine.rs:187） | **folder load worker スレッド**（per-nav `std::thread::spawn`）。フレーム外 | **最初の grep が落としていた**（複数行チェーン）。history でソートするため engine 本体が要る。移行不能 |
| **26** | `indexing.rs:74-78` | build スレッド完了後の `is_index_stale` | ledger の読み（config ではない） | **index build スレッド** | **最初の grep が落としていた。** config を読んでいないので #1032 の射程外 |
| **27** | `indexing.rs:156-160` | `drain_index` の `complete_index_drain` | 索引 swap + `IndexInputs` re-diff（内側で config を読む・engine.rs:302） | **index build スレッド** | **最初の grep が落としていた。** **swap と照合の原子性が外側の `Mutex<Engine>` に依存する**（engine.rs:294 の doc・#347/#348-A）。**移行禁止** |

**既に錠の外に居る読み（参考・母集団の補完）:** `AppState.config`（`state.rs:16`・`main.rs:242` /
`state.rs:79` が `engine.config_handle()` から受け取る同一 `Arc`）を通る `read_config` の呼び出し点は
`mod.rs:233`（`spawn_update_check`）・`mod.rs:443`（`read_visual`）・`main.rs:461`（hotkey）・
`font_stack.rs:198`・`window_coordinator.rs:53, 89, 428, 723, 776`・`launcher_controller.rs:781`（`lang()`）の 11 か所。

---

## B. 変更で偽になる散文の完全な列挙

概念ラベル（`engine lock` / `engine.lock` / `錠` / `live-read` / `毎フレーム` / `#1032` / `#1036` /
`instant_prefix` / `40〜95` / `43,939`）で `.md` と `.rs` の doc コメント両方を走査した結果。

| # | ファイル:行 | 現在の記述（要旨） | なぜ偽になるか | 重み |
|---|---|---|---|---|
| B-1 | `src-tauri/src/egui_shell/launcher_controller.rs:747` | 「**この読みは `engine.lock()` 越しであり、#1032 の規範の未移行の残余である**——#1036 の移設に入らなかった」 | 移行すると `engine.lock()` を経なくなる。**段落ごと役目を終える**。同 749 行が「この関数を移設するときは `docs/architecture.md`「検索フロー（入力 → 結果表示）」の Enter の補足も直すこと」と**自分で申し送りを持っている** | **要対処** |
| B-2 | `docs/architecture.md:228` | 「`on_enter` は判定より前に `instant_prefix` が **`engine.lock()` を取る**ため、**worker の走査待ちは #1038 の前後どちらでも払っている**（2026-08-13 にコードで確認）。#1038 が足すのは同期 `engine.search` 1 回ぶんだけである」 | `instant_prefix` が `read_config` へ移ると**錠待ちを払わなくなる**。すると「#1038 の前後で費用は変わらない」という**結論そのものの根拠が消える**——#1038 が足すのは `engine.search` 1 回ではなく「これまで払っていなかった錠待ち + `engine.search`」に変わる。B-1 の doc が名指しているのはこの行 | **要対処**（結論が反転しうる。単なる字面の訂正ではない） |
| B-3 | `src-tauri/src/egui_shell/search_state.rs:492` | 「**実際の費用は `run_search` 入口の `instant_prefix` が `engine.lock()` を取ること（#1032）と**、Plain 腕が `indexing()` 中に復帰行を空にすること**であって、doc が名指していたものではない**」 | `on_escape` の doc が #1079 の費用見積もりを訂正するために書いた一次の根拠。移行後は前半が偽になり、**訂正の根拠が半分崩れる**（残るのは Plain 腕の方だけ） | **要対処** |
| B-4 | `src-tauri/src/egui_shell/launcher_controller.rs:1845-1848`（`activation_uses_frame_values_not_live_reads` の doc） | 「`run_search_with` は対象外である（意図的）…同様に `lang()` は `read_config` を正当に使う…**この 2 つが対象外のままであることが、この設計の受け入れ条件である**」 | 移行後、`read_config(` の正当な出現は `lang()` だけでなく `auto_hide_enabled` / `instant_prefix` にも生じる。「**この 2 つ**」という数え上げが**その場で腐る**。`AGENTS.md`「検証の作法」の「数え上げも同じ強さである——数ではなく正本を指す」に正面から当たる | **要対処** |
| B-5 | `src-tauri/CLAUDE.md:57`（規範本体） | 「**`commands/` の操作時の読みは engine lock のままでよい**（毎フレームではなく、`resolve_opener` のように別目的で同じ錠を既に取るものがある）」 | 要件 3 が書き直す当の条項。**現時点で既に偽に近い**——`commands/instant.rs` の `get_instant_commands` は `commands/` に在りながら egui フレームの中で毎打鍵走る（A-1 #3）。ディレクトリを弁別子にしたことが誤りだった一次証拠 | **要対処**（要件 3 そのもの） |
| B-6 | `src-tauri/src/egui_shell/window_coordinator.rs:183-186`（`read_background` の doc） | 「**`read_visual` と統合しない**: こちらは show 経路の読みで…同じ関数内の `read_metrics` や **`follow_cursor_monitor` / `ime_off_on_show` の読みと同じ層である**」 | 「同じ層」の例として並べた 3 つのうち `read_metrics`（:52）と `ime_off_on_show`（:428）は**既に `read_config` 側に居る**——**この記述は #1036 の時点で既に古い**。今回 `read_background` と `follow_cursor_monitor` も移ると 4 つ全部が同じ側になり、「同じ層」という語の指す内容が変わる（層＝フレーム外であることは真のまま。**読み口が違うという含意だけが偽**） | **軽微**（既に古い。今回の移行でむしろ整合する） |
| B-7 | `docs/adr/ADR-blur-grace-single-field-state-machine.md:22` | 却下理由に「現行は…**armed のときしか engine lock を取らない**が、値渡しにすると毎フレーム取る…**engine lock は既に毎フレーム無条件で 2 回（`read_visual` / `lang()`）走っており、2 → 3 に増えるだけである**」 | **2 つの意味で既に偽**: (a) `read_visual`（`mod.rs:443`）も `lang()`（`launcher_controller.rs:781`）も**もう engine lock を取らない**（#1032/#1036 で `read_config` へ移った）。(b) 実装は値渡しを採用済み（`:1236`）なので「毎フレーム取る」側が現行である。今回 `auto_hide_enabled` が移ると**残る 1 回も消える** | **軽微**（→ ADR の扱いは B-9 で判定） |
| B-8 | `docs/development-principles.md:184` | 「#1112 の当該ファイルには **`read_config(` が対象外の経路（`lang()`）で**…在り」 | 移行後は対象外の正当な出現が 3 経路になる。**存在形（下限主張）なので厳密には真のまま**だが、`lang()` を唯一例として読ませる書き方は誤読を招く | **軽微** |
| B-9 | `docs/adr/` の判定 | — | **移行対象の箇所を「意図的にそう設計した」と記録している ADR は存在しない。** `docs/adr/*.md` を `1032` / `read_config` / `engine.lock` / `engine lock` / `錠` で走査したヒットは 2 本のみ: `ADR-activation-gate-placement.md:38`（engine lock を**テスト席が無い理由**として挙げるだけ。移行と無関係）と `ADR-blur-grace-single-field-state-machine.md:22`（B-7）。後者は**却下した代替案の費用見積もり**として engine lock に言及するが、**採用した決定（単一フィールドの状態機械・値渡し）は本変更で覆らない**——費用の前提が消えるだけで、却下の主論拠（borrow checker を通らない・`auto_hide_enabled` の doc が「都度読む」と言う）は独立に生きている。かつ `ADR-adr-frozen-history` が「ADR は凍結された歴史であり腐るに任せる」と定めているため、**この ADR を書き換えてはならない** | **未検証ではない・結論: ADR の書き換え不要／決定の覆しなし** |

### B-10. 偽にならないと判定したもの（誤って直さないため）

- `docs/architecture.md:231`（#1032 の bullet）: 「UI が同じ錠越しに設定を読む箇所——`read_window_width` と `max_results`——が**そこで待っていた**」は**過去形の帰属記録**であり、A/B を測った当時の 2 か所を指す。移行で偽にならない。**ただし「設定の読みを錠の外へ出して解いた」は全称に読めるので、B-2 を直すついでに「残っていた分を #1076 で寄せた」旨を足すと整合する**（軽微）。
- `PERFORMANCE.md:557-580`（「設定の読みを engine lock の外へ出す — #1032 の A/B」）: **測定値の記録であり、測った 2 指標（`read_window_width` / `max_results`）はいずれも既に移行済み**。今回の 5 件はこの表に登場しない。**要件が「性能は測らない」なので、この表へ行を足してはならない**（測っていない値を A/B 表へ書くと器が腐る）。**偽にならない。**
- `snotra-core/CLAUDE.md:192`（「engine.rs のロック最小化パターン」・規範 B-5 が参照先に指す）: 読み/書きの非対称を述べており、**射程は `snotra-core` 側**。移行で偽にならない。
- `snotra-core/src/engine.rs` の `//!` :7,10 と `config` フィールド doc :101 / `config_handle` :244: 契約側の記述。偽にならない。
- `src-tauri/src/state.rs:88-95`（`ui_reads_config_while_the_engine_lock_is_held` の doc）: 「UI の live-read は engine lock の外で完了する」——移行で**より真になる**方向。
- `src-tauri/src/main.rs:457-459`: 既に `read_config` を使っており正しい。
- `commands/launch.rs:106, 157`（`#1032 で config() が RwLockReadGuard を返すようになった`）: guard を束ねる理由の記述。据え置き対象（A-3 #15/#16）なので偽にならない。
- `SPEC.md`: `engine lock` / `read_config` / `Mutex<Engine>` の記述は **0 件**（grep 実測）。SPEC 同期は不要。

---

## C. 壊しうる検知器・検査

| # | 検知器 | 所在 | 判定 |
|---|---|---|---|
| C-1 | `activation_uses_frame_values_not_live_reads` | `launcher_controller.rs:1856-1905` | **赤にならない（静的導出）。** この検査は `owners_of(src, "read_config(")` でファイル全体から出現を列挙し、**直前の字下げ 4 の `fn` ヘッダへ帰属**させ、帰属先が `fn on_enter(` / `fn activate_or_execute(` / `fn shift_activate(` の 3 本のいずれでもないことを assert する。移行で `read_config(` が入るのは `fn auto_hide_enabled(&self) -> bool {`（:730）と `fn instant_prefix(&self) -> String {`（:750）の本体であり、**どちらも起動の入口ではない**。`on_enter:1438` にあるのは `self.instant_prefix()` という**呼び出し**で、綴り `read_config(` は現れない。**doc コメントの帰属にも注意が要る**——`owners_of` はヘッダ行より前の doc コメントを**直前の別ヘッダ**へ帰属させるので、`instant_prefix` の doc（:744-749）に `read_config(` と括弧つきで書くと `fn auto_hide_enabled(` へ帰属する（同じく入口でないので緑）。**⚠ 実測ではない**——本レビューは一時的なソース変更を禁じられているため、検査本体のロジックからの導出である。**実装者は移行後に `cargo test -p snotra` を走らせて確認すること** |
| C-2 | `activation_uses_frame_values_not_live_reads` の**アンカー assert**（:1869-1876） | 同上 | **赤にならない。** 3 本のヘッダが字下げ 4 で実在することを要求するが、移行はヘッダを触らない。**ただし B-4 の doc 訂正でこの assert の直上の doc を編集するので、`fn on_enter(` 等の綴りを doc へうっかり字下げ 4 の行として書かないこと**（`owners_of` の doc が記録する「偽ヘッダによる帰属の横取り」の残余に触れる） |
| C-3 | `activation_entry_points_consult_the_display_gate` | `launcher_controller.rs:1935-1957` | **無関係。** `method_body` で `activate_or_execute` / `shift_activate` を切り出し `plain_results_hidden(` / `results_area_collapsed(` の**存在**を測る。移行はこれらの綴りに触れない。**ただし `method_body` は canary（`execute_tool_selected(` / `folder_load_pending(`）で母集団の非空を測るので、それらの綴りも触らないこと** |
| C-4 | `on_enter_delegates_the_flush_decision_to_the_predicate` | `launcher_controller.rs:2005-2020` | **無関係だが要注意。** `method_body(src, "fn on_enter(", "self.activate_or_execute(")` の中に `if crate::egui_shell::should_flush_on_enter(` の綴りがあることを測る。**移行で `on_enter` の本体テキストは変わらない**（`self.instant_prefix()` の行はそのまま）ので緑。doc が「並びを変えるなら canary も動かすこと」と警告している点だけ留意 |
| C-5 | `assert_read_once_in_this_file` | `view.rs:1408` 周辺 | **無関係。** 母集団は `view.rs` 全体で、needle は `concat!` で組む `read_indexing` / `read_visible_rows`。**`view.rs` は本移行の対象外**（A の表に 1 件も無い）。`view.rs` を触らない限り安全。**触るなら「≦1 側は doc コメントに綴りが出ただけで赤になる」ことに注意** |
| C-6 | `state.rs` の `ui_reads_config_while_the_engine_lock_is_held` / `app_state_config_is_the_same_arc_the_engine_holds` | `state.rs:99-133` | **無関係。** `AppState` の構造を測るもので、呼び出し側の移行に影響されない |
| C-7 | `snotra-core` の engine テスト群（`config_returns_current_config` 等） | `engine.rs:467-479` | **無関係。** `Engine::config()` は残る（A-2 / A-3 が使い続ける） |
| C-8 | `governance:check` の 8 検査（`G-adr-file-names` / `G-clippy-disallowed` / `G-heading-refs` / `G-hook-commands` / `G-hook-fires` / `G-references` / `G-stale-identifiers` / `G-workspace-lints`） | `scripts/governance-check.mjs` | **`G-heading-refs` が本命。** B-1〜B-5 の訂正で `.md` と **`.rs` の doc コメント**（#925 で `.rs` も走査対象）に正準形 `` `<path>.md`「<見出し>」 `` を書くことになる。**現存する見出しを正確に綴ること**——とくに `docs/architecture.md`「検索フロー（入力 → 結果表示）」（B-1 が既に指している）と `PERFORMANCE.md`「設定の読みを engine lock の外へ出す」（`src-tauri/CLAUDE.md:57` が指す・要件 3 で条項を書き直すときに落とさないこと）。**`G-stale-identifiers`** は doc 中の識別子の実在を見るため、`read_background` / `instant_prefix` 等の綴りを消す・変える場合に当たりうる |
| C-9 | CI `rust-check` job | `.github/workflows/` | 全 4 crate の `cargo test` を常時実行。C-1 の静的導出が誤っていればここで赤くなる（**最後の網**） |
| C-10 | `scripts/smoke-egui.ps1`（`smoke:egui`） | — | **要注意。** #4 `read_background` と #5 `follow_cursor` は **show 経路**を触る。smoke は `egui_show:done` / `egui_hide:done` の trace を観測する。**trace イベント名は触らないので緑のはずだが、show 経路のコード変更はカテゴリ C のトリガーに当たる**（D-5） |

**壊しうるが今回は当たらないもの**: `src-tauri/clippy.toml` の `disallowed-methods`（`ctx.set_visuals` 等 7 メソッド。config 読みは対象外）。

---

## D. 実行すべき検証

### D-1. 当たるトリガー（`AGENTS.md`「条件別チェック（トリガー → 参照先）」）

| トリガー行 | 当たる理由 | 参照先 / 実行 |
|---|---|---|
| **「worker spawn・channel・…**フレーム内 live-read**・…を追加/変更」** | 移行対象 5 件すべてがフレーム内 or show 経路の live-read の**読み口の変更**。`src-tauri.md` rule の「トリガー → 検査」も同じ行を持つ | **`/race-check`** |
| **「ガバナンス文書（`*.md`・…）を変更、または `.rs` のコメントの見出し参照（正準形）とその参照先を変更」** | 要件 3 で `src-tauri/CLAUDE.md` の条項を書き直す。B-1〜B-4 で `.rs` の doc コメントも直す | **`npm run governance:check`**（`docs/build-commands.md` カテゴリ F）。PR では `governance-check` job が常時実行 |
| **「セーフティネット（…規範）を新設/変更」** | **規範文書（`src-tauri/CLAUDE.md`）の変更**。ルート `CLAUDE.md`「最重要ルール」2 の「セーフティネットの変更は合意してから」に当たる | `.claude/rules/safety-nets.md`（**モジュール `CLAUDE.md` は自動配送されるが、規範性の観点で手動参照も要る**）。**要件 3 は既にユーザーの決定なので合意は取れている** |
| **「各言語ファイルを編集」** | `.rs` 5 ファイル | `.claude/rules/src-tauri.md` / `comments.md`（読取で自動配送） |
| **「機能削除・trace イベント名／hotkey 登録・**表示経路**の変更」** | #4/#5 が show 経路（`show_egui_main`） | `scripts/smoke-egui.ps1` と smoke 前提が壊れないか確認 → **`npm run smoke:egui`**（カテゴリ C） |
| **「`Option` / フラグ / enum variant など**どの分岐が選ばれるかを決める値**の出所を変更」** | **弱く当たる**（D-3 参照）。`follow_cursor_monitor` は `cursor_monitor_work_area` / `primary_monitor_work_area` の分岐を決める値で、その**出所**（engine lock 越し → `read_config`）が変わる | 「diff に現れない下流を 1 段辿り『この値で初めて走る行』を列挙する」 → D-3 で実施済み |
| **「レビュー指摘へ修正（fix-forward）を当てた」** | 本レビューの指摘に修正を当てるなら発火 | 指摘を出した枠組み（本レビュー）を**修正差分にも**再実行してから閉じる |

**当たらないと判定したトリガー**（列挙して根拠を裏付ける・`AGENTS.md`「変更なしと判断するときは」）:
`/persistence-check`（on-disk 形式に触れない）・`/state-check`（UI モード・ガード条件を足さない）・
`/symmetric-check`（対称ペア・生成/破棄に触れない）・`/dry-check`（新規関数を定義しない。既存ヘルパーの本体差し替えのみ）・
`/plan-review`「Step 2b」の独立再導出（**本レビューがそれである**）・
モジュール索引の更新（ファイルの追加/削除が無いので `G-module-linkage` 相当は無関係）。

### D-2. 走らせる検査（`docs/build-commands.md`）

**カテゴリ A（`*.rs` 変更・必須）** — PostToolUse hook が fmt/clippy/test を自動実行し**沈黙 = 合格**だが、`cargo doc` は hook が走らせない:

```
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
```

- **`cargo doc` を手で走らせることが必須である**（`.claude/rules/comments.md`）。B-1 の書き直しで
  `` [`crate::egui_shell::read_config`] `` の intra-doc link を触る。**リンク切れは CI でのみ発火し hook は沈黙する**。
- `snotra-core` は変更しないので `cargo test -p snotra-core` はローカル任意（CI が担保）。

**カテゴリ C（表示経路・#4/#5）**: `npm run smoke:egui`

**カテゴリ F（ガバナンス文書）**: `npm run governance:check`

**skill**: `/race-check`

### D-3. 「この値で初めて走る行」— fallback の非対称（**独立導出・注意点**）

要件 4 は「挙動は変えない（読む値は同一）」だが、**fallback の枝だけは同一ではない**。

- `AppState.config` と `Engine.config` は**同じ `Arc`** である（`state.rs:79` / `main.rs:242` が
  `engine.config_handle()` から受け取る。`state.rs:116` の
  `app_state_config_is_the_same_arc_the_engine_holds` が測っている）。ゆえに**正常系の値は完全に同一**。
- **しかし fallback の到達条件が違う。** 現行の 5 件はいずれも `app.try_state::<AppState>()` の
  `.map(...).unwrap_or_else(...)` であり、`read_config` も同じ `try_state` で分岐する（`mod.rs:428`）。
  **`AppState` 不在という条件も、そのときの既定値も同一である**（`GeneralConfig::default()` /
  `SearchConfig::default()` / `visual::default_visual()` をそのまま持ち込めば）。
- **⚠ ただし #3 `get_instant_commands` だけは違う。** 現行は `app.state::<AppState>()`（`instant.rs:10`）で、
  **`AppState` 不在なら panic する**。`read_config` は `try_state` なので **fallback へ落ちる**。
  移行すると「これまで panic していた経路が黙って空の instant コマンド一覧を返す」ようになる——
  **1 行も変えていない下流（`filter_instant_commands`）が初めて `&[]` で走る**組み合わせである。
  実運用では `AppState` は Builder 段階で manage されるため到達しないが、
  **既定値の選び方を明示的に決めて doc に書くこと**（空 `Vec` を返すのが妥当）。

### D-4. 実装順の助言（構造的な注意点）

- **`read_config` の中で lock を取る操作を書かないこと**（規範が明示・`mod.rs:418`）。
  #3 は `read` クロージャの中で `filter_instant_commands` まで走らせたくなるが、
  **read guard を持ったまま重い処理を書かない**——`instant_commands` を clone して guard を落としてから filter する形が安全。
- **#5 は `read_config` の返り値を `if` の条件に使うだけなので guard は即座に落ちる**（現行と同じ形）。

---

## 所見の 3 分類

### 要対処

1. **B-2 `docs/architecture.md:228`** — `instant_prefix` の錠待ちを根拠に「#1038 の前後で Enter の費用は変わらない」と結論している。
   移行でこの根拠が消え、**結論が反転しうる**。字面の訂正ではなく、費用の主張そのものを再導出して書き直すこと。
   `launcher_controller.rs:749` が自分でこの申し送りを持っている（**移行者が読む位置に在るので、機構としては働く**）。
2. **B-4 `launcher_controller.rs:1845-1848`** — 「`run_search_with` と `lang()` の **2 つ**が対象外のままであることが、
   この設計の受け入れ条件である」という**数え上げ**が移行の瞬間に腐る（対象外の正当な出現が 3 経路以上になる）。
   `AGENTS.md`「検証の作法」の「数ではなく正本を指す」に従い、**件数を書かない形へ倒すこと**
   （「起動の入口の外の `read_config(` はすべて対象外である」など）。
3. **B-5 `src-tauri/CLAUDE.md:57` の例外文** — 要件 3 の当の対象。**ディレクトリを弁別子にしたことが誤りだった一次証拠は
   `commands/instant.rs` の `get_instant_commands` である**（`commands/` に在りながら egui フレームの中で毎打鍵走る・A-1 #3）。
   書き直すときはこの実例を根拠として残すこと。また例外文が名指す `resolve_opener` は
   **フレーム外（launch worker）だから据え置く**のであって「`commands/` だから」ではない、と理由を付け替えること。
4. **B-1 `launcher_controller.rs:747`** と **B-3 `search_state.rs:492`** — 移行で偽になる。B-3 は
   `on_escape` の #1079 費用見積もりの**訂正の根拠**として使われているので、消すのではなく
   「残るのは Plain 腕の方だけ」へ縮めること。
5. **D-3 の fallback 非対称** — `get_instant_commands` は `app.state()`（panic）から `try_state`（fallback）へ変わる。
   **既定値を明示的に決めて doc に書くこと。** 「挙動は変えない」の前提が唯一崩れる箇所である。
6. **`cargo doc` を手で走らせること** — B-1 の書き直しで intra-doc link を触るのに、PostToolUse hook は沈黙する。

### 軽微

1. **B-6 `window_coordinator.rs:183-186`** — 「`read_metrics` / `follow_cursor_monitor` / `ime_off_on_show` と同じ層」は
   **#1036 の時点で既に古い**（前者 2 つはもう `read_config` 側）。今回の移行で 4 つ揃うのでむしろ整合する。ついでに直すとよい。
2. **B-7 `ADR-blur-grace-single-field-state-machine.md:22`** — 「engine lock は既に毎フレーム無条件で 2 回
   （`read_visual` / `lang()`）走っており」は**既に偽**（両者とも `read_config` へ移行済み）。
   **ただし `ADR-adr-frozen-history` により ADR は凍結された歴史であり、書き換えてはならない。** 記録として指摘するのみ。
3. **B-8 `docs/development-principles.md:184`** — `read_config(` の対象外の経路として `lang()` だけを挙げる。
   存在形なので厳密には真のままだが、唯一例と読ませる。触るなら「〜だけではない」の形へ。
4. **B-10 `docs/architecture.md:231`** — 「設定の読みを錠の外へ出して解いた」は全称に読める。
   B-2 を直すついでに「残っていた分を #1076 で寄せた」旨を足すと整合する。
5. **`PERFORMANCE.md`「設定の読みを engine lock の外へ出す」の表に行を足さないこと** — 要件が「性能は測らない」以上、
   測っていない値を A/B 表へ書くと器が腐る（`fixing-instrument-invalidates-ab-comparison` の類型）。
6. **C-2 / C-3 / C-4 の canary と綴りに触れないこと** — doc を直すときに字下げ 4 の `fn ` で始まる行を
   文字列/コメントへ書かない（`owners_of` の「偽ヘッダ」残余）。

### 未検証

1. **C-1（ソーステキスト検査が赤にならないこと）は静的導出であって実測ではない。** 一時的なソース変更が禁じられているため、
   `owners_of` / `method_header` のロジックから導いた。**実装者は移行後に `cargo test -p snotra` を実行して確認すること。**
   導出の根拠は「`read_config(` の新しい出現が帰属するヘッダは `fn auto_hide_enabled(` と `fn instant_prefix(` であり、
   どちらも `entry_points` の 3 本に `contains` で一致しない」である。
2. **`G-heading-refs` / `G-stale-identifiers` が新しい doc 文面に当たるかは、文面が確定していないため未検証。**
   `governance-check.mjs` の 8 検査は確認したが、書き直し後の正準形が既存見出しに一致するかは実際に
   `npm run governance:check` を走らせるまで分からない。
3. **`smoke:egui` が show 経路の変更で緑のままかは未検証。** trace イベント名（`egui_show:done` / `egui_hide:done`）に
   触れないので緑のはずだが、`read_background` は下地色を、`follow_cursor` はモニター選択を決める値であり、
   **実際に走らせるまで断定しない**。
4. **`config_watcher.rs:87` の `old_config` clone を「適用側ゆえ射程外」と判定したが、
   これは #1032 の規範文の「射程は読みだけである」の解釈である。** 規範は「読み」と「書き」しか二分しておらず、
   「適用手続きの中の読み」がどちらかを明示していない。**要件 3 で条項を書き直すなら、この 3 つ目の類を
   明示的に位置づけるとよい**（ただし config 監視スレッドはフレーム外なので、新しい弁別子では自動的に据え置きへ落ちる）。
5. ~~母集団の取りこぼし~~ → **解消済み（検証した）。** 当初は「`engine.lock()` の 1 行 grep」を母集団の根拠に
   していたが、**rustfmt の折り返しで `.lock()` が別行に落ちる形を構造的に落としていた**（実際に 3 件落としていた・
   A-3 #25〜#27）。**全 `.lock()` 出現 77 件のレシーバ分類**へ取り直して再導出した結果、engine 関連は 30 件で、
   **移行対象 5 件・据え置き 22 件の分類は変わらなかった**（新たに見つかった 3 件はいずれも据え置き）。
   `let eng = state.engine.lock()` のようにレシーバを束縛し直す形も、この 77 件の分類で覆われている
   （`.lock()` の出現自体は必ず現れるため）。
6. **`Mutex<Engine>` の guard を返すヘルパー関数が存在しないことは、`state.rs` の目視でのみ確認した。**
   `engine` フィールドは `pub` で各所から直接 `lock()` されており、guard を返す関数は見当たらないが、
   これは全称否定なので LSP の `findReferences` で `AppState::engine` の参照を列挙すれば確実になる。
