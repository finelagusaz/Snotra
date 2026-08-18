# #1123 独立導出 — config の live-read 規範から例外という装置を無くす

対象 issue: **#1123**（検討: config の live-read 規範から例外を無くす。残る 3 か所も `read_config` へ寄せる）

導出は**コードと規範文書だけ**から行った（`workspace/` 配下は一切読んでいない）。issue 本文が挙げる根拠は逐語追認せず、以下はすべて一次証拠（ファイル・行）で裏を取っている。裏取りの結果 issue 本文と食い違った点は「⚠️ / 訂正」に書く。

---

## 0. 事実確認（issue 本文の根拠のコード裏取り）

| issue の主張 | 裏取り | 判定 |
|---|---|---|
| 例外は 3 か所 | `#[expect(clippy::disallowed_methods)]` は src-tauri に **4 件**（`commands/launch.rs:108`, `commands/launch.rs:163`, `commands/icon.rs:18`, `config_watcher.rs:88`）。うち config_watcher は issue 自身が射程外と宣言 | ✅ ただし**注釈の総数は 4** — 後述の撤去条件判定でこの差が効く |
| 3 か所とも engine 錠を config 読みのためだけに取る | `resolve_opener`（launch.rs:103-114）: `lock()` → `config()` → `find_matching_tools` → 即 return。`resolve_all_openers`（158-172）: 同型。`ensure_icon_cache_loaded_if_enabled`（icon.rs:15-23）: 2 値を読んでブロックを抜け、次は別 Mutex `IconCacheState` | ✅ 実測一致 |
| 履歴の錠は `record_and_save` が別に取り直す | `record_and_save`（launch.rs:51-62）が独立に `state.engine.lock()` を取る。`launch_item_with_state` は `resolve_opener` の guard を跨いでいない | ✅ |
| 3 か所とも `&AppState` を手に持ち `&AppHandle` すら要らない形にできる | `resolve_opener` / `resolve_all_openers` は `&AppState`、`ensure_icon_cache_loaded_if_enabled` は `&State<AppState>`。**ただし呼び出し元 3 本はいずれも `&AppHandle` を持つ**（`platform/tray.rs:52,73`（`handle_menu_command`）/ `tray.rs:426`（`show_recent_history_menu`）/ `egui_shell/results_view.rs:205-214`（spawn 内の `app`）） | ✅ 但し「AppHandle が無い」わけではない。設計の分岐が実在する（§2） |
| `AppState.config` は `Arc<RwLock<Config>>` でスレッドを問わない | `state.rs:24` + `state.rs:79`（`engine.config_handle()`）。同一 Arc であることは `state.rs` の `app_state_config_is_the_same_arc_the_engine_holds` が固定 | ✅ |
| `find_matching_tools` は純 CPU | `snotra-core/src/opener.rs:59-` — `to_lowercase` / `replace` / スライス比較のみ。錠も I/O も無い | ✅ |
| `Config::icon_cache_cap()` は純 CPU | `snotra-core/src/config.rs:667-676` — 算術のみ | ✅ |

---

## 1. 導出したファイル一覧（パス＋変更理由）

### 1-A. コード（挙動の担い手）

| パス | 変更理由 |
|---|---|
| `src-tauri/src/commands/launch.rs` | `resolve_opener` / `resolve_all_openers` の config 読みを engine 錠の外へ。`#[expect]` 2 件を削除（残せば**不履行で赤**）。両関数の `///` doc（#524 の is_dir 規律）を新しい錠に合わせて書き直す |
| `src-tauri/src/commands/icon.rs` | `ensure_icon_cache_loaded_if_enabled` の config 読みを engine 錠の外へ。`#[expect]` 1 件を削除。行 11-14 のブロックコメント（「config は単一の engine ロック内で読み」）が偽になる |
| `src-tauri/src/egui_shell/mod.rs` | **分岐 (a) では変更なし**。**分岐 (b/c) では `read_config` の doc（410-421 行）の「UI が config を読む唯一の口」が偽になる**／(c) では `&AppState` 版の核を切り出し `read_config` をその委譲にする |
| `src-tauri/src/platform/tray.rs` | **分岐 (a) のみ**（`&AppHandle` を渡す形へ呼び出し 2 か所を変更）。分岐 (b/c) では変更なし |
| `src-tauri/src/egui_shell/results_view.rs` | **分岐 (a) のみ**（`load_icon_pngs` へ `&AppHandle` を渡す）。分岐 (b/c) では変更なし |
| `src-tauri/src/config_watcher.rs` | **錠は変えない**（射程外）。ただし **`#[expect]` の `reason` 文字列（89 行）が「弁別子が他の例外と違う」と書いており、他の例外が消えると前提が偽になる**。文言の書き直しが要る（条項への指し `src-tauri/CLAUDE.md「モジュール構成」` は**壊さないこと**。ただし**この綴りは機構に照合されていない**——理由は §4 I-7） |
| `src-tauri/src/commands/instant.rs` | doc 28-31 行が「#1032 条項の**例外**が名指すのは egui フレームの外で行う読み（icon worker・folder worker・tray スレッド）」と書く。例外装置が消えると端的に偽 |

### 1-B. 規範・散文（この変更で偽になるもの）

| パス | 何が偽になるか |
|---|---|
| `src-tauri/CLAUDE.md` **57 行**（config live-read 条項・**本丸**） | 「**例外は「イベントループスレッドを止めない場所での読み」である**」以降、条項のおよそ後半 2/3 が丸ごと不要になる: 例外の定義／「spawn した worker と platform スレッドは engine lock のままでよい」／**弁別子はディレクトリでも頻度でもなく走るスレッドである**／動機と判定の分離／「どこで走るかは呼び出し元を辿って決めること」／「列挙で覚えない」と非自明ケース 3 つ（`on_event_loop`・tao window-event listener・`app.listen` の emit 元）／`get_instant_commands` の例／hotkey の先例／「例外は `#[expect]` が呼び出し点ごとの分類を記録して開ける」／`ADR-config-read-exception-discriminator` への参照。**残るのは「engine 錠を経ない」＋書きの非対称＋機構（#1122）＋`config_watcher` の射程外**。**削る文ごとに「その事実は今どこに在るか」を逆向きに確かめること**（多くは他に SSOT がある: `app.listen` の同期実行は同ファイル「Win32 メッセージ配送の注意」、`on_event_loop`／listener のスレッドは `snotra-egui-runtime` の `proof.rs`。**確かめずに消すと SSOT ごと消える**） |
| `src-tauri/CLAUDE.md` **24 行**（config_watcher の icon 破棄の不変条件） | 「**engine がまだ `show_icons=true` を返す隙に** icon worker（`ensure_icon_cache_loaded_if_enabled` → `IconCache::load`）がキャッシュを建て直し」——変更後、worker が読むのは engine ではなく `AppState.config` である。**残余（窓は閉じない）という結論は不変だが、機序の記述が腐る** |
| `src-tauri/clippy.toml` **群 3**（86-146 行） | (1) 93-100 行「例外は `#[expect]` で開ける」の**根拠**が「条項は既に『どこで走るかは呼び出し元を辿って決めること』という**呼び出し点ごとの判定義務**を課している」——条項からその文が消えると根拠が宙に浮く。(2) 117-119 行の残余「記録した分類は黙って腐る／同じ関数が両方から呼ばれるようになれば分類は変わる」も同じ。(3) 132-139 行の前提 1（注釈が 1 つ以上残ること）は**成り立ち続ける**が、支えが 1 本になる。(4) **141-146 行の撤去条件は発火しない**（§4 で逐語判定）。(5) 158 行の配列 reason「例外は expect 属性に分類理由を添えて開ける」は config_watcher の 1 件については真のまま |
| `docs/architecture.md` **231 行** | 「**#1032 の残余は #1076 が寄せた**（**例外の弁別子と射程**は `src-tauri/CLAUDE.md`「モジュール構成」の当該条項が正本）」——「例外の弁別子」という装置が無くなる。正本を指す形は保ちつつ語を替える |
| `snotra-core/src/engine.rs` **228-241 行**（`Engine::config` の doc） | 「製品 crate ではこの綴りを `clippy.toml` が禁じている（#1122）…UI は `egui_shell::read_config` を通す」——**禁止は残るので大半は真**。分岐 (b/c) を採ると「`read_config` を通す」が正確でなくなる |
| `snotra-core/CLAUDE.md` **192 行** | 読み/書きの非対称の記述。**この変更で偽にならない**（読みが engine 錠の外に出るのはこの節が既に述べていること）。**変更不要と判定**（列挙の完全性のため明記） |
| `scripts/governance/checks/G-clippy-disallowed.mjs` **65-66 行**のコメント | 「群 3（#1122）: engine 錠越しの config の live-read。**例外は expect 属性が分類を記録して開ける**」——例外という語が残る。`REQUIRED_DISALLOWED_METHODS` の**エントリ自体は変えない**（§4） |

### 1-C. 変更**しない**と判定したもの（根拠つき）

- `docs/adr/ADR-config-read-exception-discriminator.md` — **凍結された歴史。編集してはならない**（`ADR-adr-frozen-history`：「歴史は消えることに対してだけ守り、変わることに対しては守らない」）。32 行の「#1123 で評価する」も**そのまま残す**
- `docs/adr/ADR-adr-frozen-history.md` 他の既存 ADR — 同上
- `src-tauri/src/egui_shell/launcher_controller.rs` — `resolve_tools` / `instant_prefix` / `auto_hide_enabled` の doc は「#1076 で `read_config` へ移した」という**過去形の記録**であり、いずれも真のまま。ソーステキスト検査 `activation_uses_frame_values_not_live_reads`（1878 行〜）は**同ファイルのみを母集団**とするため `commands/` の変更に反応しない
- `src-tauri/src/state.rs` — テスト 2 本（`ui_reads_config_while_the_engine_lock_is_held` / `app_state_config_is_the_same_arc_the_engine_holds`）は変更後も同じ命題を守る。doc も真のまま
- `SPEC.md` — **挙動を変えないため同期不要**。`AGENTS.md`「3層分担」の対象は「挙動を変える変更」であり、本件は読み口の付け替え（同じ値・同じ分岐・同じ順序）
- `docs/hooks.md` / `.claude/rules/safety-nets.md` / `AGENTS.md` / ルート `CLAUDE.md` — 当該条項への言及なし（grep 実測）
- `PERFORMANCE.md` — #1032 の A/B は過去の実測記録。本件は性能を測らないので追記しない
- `scripts/` の `.ps1` / `.psm1` — config live-read 規範の言い換えは無い（grep 実測）

---

## 2. 設計の分岐（**計画で先に決める必要がある。黙って選ばない**）

3 か所は `&AppState` を持つが `read_config` は `&AppHandle` を要求する。ここで枝が分かれ、**枝ごとに直す散文が違う**。

- **分岐 (a): 呼び出し元から `&AppHandle` を通し、`read_config` をそのまま使う。**
  - 変えるシグネチャ: `resolve_opener` / `resolve_all_openers` / `launch_item_with_state` / `load_icon_pngs` / `ensure_icon_cache_loaded_if_enabled`（`pub` を含む＝tray 側 3 呼び出し点も追随）
  - 得: 条項の第 1 文「`read_config` を通す」が**そのまま真**で、読み口が構造的に 1 つに保たれる
  - 損: `read_config` は `fallback` クロージャを要求する。**tray スレッドと icon worker は setup 完了後にしか走らない**（`.manage` は `.setup` より前・窓生成後に worker が spawn される）ので、**到達しない fallback を 3 つ捏造することになる**。`openers` の fallback は `Vec::new()`（＝オープナー無しで起動）で意味が付いてしまい、`icon_cache_cap` は `Config::default()` を建てる I/O を招く（`instant.rs` の doc が既にその費用を名指して「他の fallback へこの形を写さないこと」と書いている）
- **分岐 (b): 3 か所で `state.config.read().unwrap()` を直に読む。**
  - 得: シグネチャ不変・fallback 不要・issue 本文の想定どおり
  - 損: **読み口が 2 つになる**。`read_config` の doc「UI が config を読む唯一の口」と条項第 1 文「`read_config` を通す」が偽になるので、両方を「engine 錠を経ない」へ書き直す（＝ issue が予告している形そのもの）
- **分岐 (c): `read_config` を `&AppState` を取る核＋`&AppHandle` の薄い包みへ分ける。**
  - 得: fallback は「AppState 不在」＝ `AppHandle` 側の関心だけに閉じ、`&AppState` を持つ 3 か所は fallback を書かない。構造的な読み口は依然 1 つ（包みが核へ委譲する）
  - 損: `egui_shell/mod.rs` に関数が 1 本増え、doc の言い回しを直す必要がある

**分岐を切る制約**は「到達しない fallback を捏造させるか」である。(a) はそれを強い、(b)/(c) は強いない。**(c) は (b) の利点を取りつつ「唯一の口」の性質を構造で保つ**——ただしこれは所見であって決定ではない。**決めるのは計画の担当者であり、本レビューは選ばない。**

---

## 3. 導出したシンボル一覧

**移設対象（config 読みが engine 錠の外へ出る 3 シンボル）**
- `src-tauri/src/commands/launch.rs::resolve_opener`（private fn）
- `src-tauri/src/commands/launch.rs::resolve_all_openers`（`pub`）
- `src-tauri/src/commands/icon.rs::ensure_icon_cache_loaded_if_enabled`（`pub(crate)`）

**削除する注釈（3 件・残すと `#[expect]` 不履行で赤）**
- `launch.rs:107-110` / `launch.rs:162-165` / `icon.rs:17-20` の `#[expect(clippy::disallowed_methods, reason = …)]`

**分岐 (a) でシグネチャが変わる（＝呼び出し点の追随が要る）**
- `launch.rs::launch_item_with_state`（`pub`・呼び出し点 `platform/tray.rs:76`）
- `icon.rs::load_icon_pngs`（`pub(crate)`・呼び出し点 `egui_shell/results_view.rs:214`）
- `platform/tray.rs::handle_menu_command`（内部で `state` を引く形が変わる）
- `platform/tray.rs::TrayIcon::show_recent_history_menu`（`resolve_all_openers` を包むクロージャ）

**分岐 (c) で新設/改める**
- `egui_shell::read_config`（`pub(crate)`）＋ `&AppState` を取る新しい核

**触るが錠は変えない**
- `config_watcher::apply_config_change` — `#[expect]` の `reason` 文字列のみ

**触らないことを確認したシンボル**
- `Engine::config` / `Engine::config_handle` / `Engine::update_config`（`snotra-core`）— 可視性も本体も不変
- `record_and_save` / `launch_item_core` / `launch_with_tool_core` / `launch_default_with_state` / `launch_with_tool_with_state`
- `LauncherController::{resolve_tools, instant_prefix, auto_hide_enabled}`

**呼び出し元の列挙手段**: 上記は grep で得たが、**実装時は LSP の findReferences で取り直すこと**（`pub` 関数のため re-export 経由・同名別物を落としうる。ルート `CLAUDE.md` の方針）。

---

## 4. 壊してはならない不変条件と、それぞれを何が検知するか

### I-1. `is_dir()` は config の読み**guard の外**で行う（#524 の規律の引き継ぎ）
- **現状**: `resolve_opener` / `resolve_all_openers` とも `Path::is_dir()` を `engine.lock()` の**前**に置く。死んだ UNC で最大 21 秒ブロックする実測（launch.rs:98-102 の doc）
- **変更後の危険は同じではなく、爆風が変わる**: `config.read()` の guard 内で is_dir すると、待つのは engine 錠ではなく **`update_config` の `config.write()`**（`engine.rs:249`）である。すなわち**設定の適用が最大 21 秒止まる**
- **検知**: **doc コメントだけ**である。コンパイラも clippy も型も検知しない。**受容する残余として明記すること**

### I-2. 読みのクロージャ／guard 保持区間で錠も I/O も取らない
- `find_matching_tools` は純 CPU（`opener.rs` 実測）、`icon_cache_cap()` は算術のみ（`config.rs:667`）——**両方この条件を満たす**
- **`IconCache::load(cap)` は guard の外に残すこと**（ファイル I/O）。現状も engine 錠の外にある
- **`icons.lock()`（`IconCacheState`）を config の read guard の内側へ入れないこと**
- **検知**: 条項本文（`read_config` の doc「`read` の中で lock を取る操作を書かないこと」）＋ `launcher_controller::resolve_tools` の doc が同じ規律を先例として書いている。**機構は無い**

### I-3. 錠の順序を逆転させない（deadlock）
- `update_config` は **engine `Mutex` → `config.write()`** の順で取る（`&mut Engine` が engine 錠を含意）
- ゆえに **`config.read()` を保持したまま `engine.lock()` を要求する形は順序逆転**である。3 か所とも変更後にそれを作らないこと（`resolve_opener` の後段 `record_and_save` は guard が落ちた**後**に engine 錠を取る＝現状の構造を保てば安全）
- **同一スレッドでの自己デッドロック**も別に在る（`Engine::config` の doc:「保持したまま `update_config` を呼ばないこと」）——本変更では `Engine::config` を使わなくなるので該当しない
- **検知**: 実行時のハングのみ（テストは無い）。**`/race-check` がこの枠を持つ**

### I-4. `show_icons` と `icon_cache_cap()` は**同一の読みから**取る（一貫性）
- 現状は 1 つの engine guard から 2 値を読む。変更後も **1 つの `config.read()` guard（または 1 回の `read_config` クロージャ）**から両方読むこと。2 回に割ると `config_watcher` の適用が間に挟まって新旧が混ざる（条項「同じ値の一貫性が要る読みは 1 回にまとめること」）
- **検知**: `read_config` の doc の規約のみ。**機構は無い**

### I-5. icon キャッシュ破棄の受容残余（`src-tauri/CLAUDE.md` 24 行）は**閉じない／広がらない**
- 現状: `update_config` の**直前**に `show_icons=true` を読んだ worker は、`drop_icon_cache` の**後**にキャッシュを挿入しうる。`ensure_…` が config 読みと icon lock を別々に取ることが原因で、これは受容残余
- **変更後も同じ**: 読みが engine `Mutex` から `config` `RwLock` へ替わっても、**2 つの錠を別々に取る**という構造は不変であり、読める値の集合（旧 or 新）も不変。窓は**閉じないが広がりもしない**
- ⚠️ **これは推論であって実測ではない**（§7 参照）
- **検知**: 無い（元から検知手段の無い受容残余）。**doc の機序の記述を実装に合わせること**が唯一の担保

### I-6. `#[expect]` の不履行が赤になる足を殺さない
- 3 件の `#[expect]` を消し、**`config_watcher.rs` の 1 件を残す**。これで `clippy.toml` 群 3 の前提 1（「注釈を持つ地点が 1 つ以上残ること」）は成り立ち続ける
- **検知**: `cargo clippy --workspace --all-targets -- -D warnings`（注釈を消し忘れれば不履行で赤。**`-D warnings` に依存する**——clippy.toml 群 3 の残余がそう書いている）

### I-7. 条項を指す参照が宛先を失わないようにする
- `config_watcher.rs:89` の reason 文字列は `src-tauri/CLAUDE.md「モジュール構成」` を含む。**`.rs` も G-heading-refs の走査元ではあるが、この綴りは照合されない**——`G-heading-refs.mjs:26` の `HEADING_REF` は ``/`([^`\n]+)`\s*(?:§…)?「([^「」\n]+)」/`` でパスの**バックティック括りを要求**し、reason 文字列はバックティック無しで書いている（`G-near-heading-refs.mjs:45` の `ADJACENT_REF` も同じくバックティックを要求するので、そちらにも掛からない）。**同じ形が `launch.rs` / `icon.rs` の消す 3 件にも在る**
- **帰結**: この 4 件の参照は**どの機構も見ていない**。見出し「モジュール構成」は本件で改名しないので実害は無いが、**「機構が守っている」と読んではならない**
- **検知**: **無い**（人が守る）。`.md` 側で正準形（バックティック括り）を使っている参照だけが `npm run governance:check`（G-heading-refs）／PR CI の governance-check job で照合される

### I-8. 挙動不変（読む値・分岐・順序）
- 3 か所とも**同じ `Arc` から同じフィールドを読む**（`state.rs` のテストが固定）。分岐も順序も変わらない
- **検知**: `cargo test -p snotra`（`app_state_config_is_the_same_arc_the_engine_holds`）＋ `cargo test -p snotra-core`

---

## 5. 検証コマンドとテスト要否の判断

### 必要なコマンド（SSOT: `docs/build-commands.md`）

**カテゴリ A（`.rs` を変更）— すべて必須**
```
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra
cargo doc --workspace --no-deps --document-private-items
```
- `cargo clippy` が**この変更の中核の検知器**である（`#[expect]` の不履行・禁止の実効）
- **`cargo doc` は手で打つこと**——`///` を大幅に書き換えるのに **PostToolUse hook は発火しない**（同ファイル 29 行が明記。intra-doc link は `[`crate::egui_shell::read_config`]` の形で実在する）
- `cargo test -p snotra-core` は **snotra-core を触らない限り不要**（本件は触らない見込み）

**カテゴリ F（ガバナンス文書を変更）— 必須**
```
npm run governance:check
```
- `src-tauri/CLAUDE.md` / `docs/architecture.md` / `clippy.toml` / `.rs` コメントの見出し参照を触るため

**カテゴリ C / D — 不要**
- trace イベント名・hotkey 登録・表示経路は変えない。UI のスタイル・レイアウト・文言も変えない
- `npm run smoke:egui` / `smoke:startup` / 目視は**この変更では要らない**（判定根拠: `AGENTS.md`「機能削除・trace イベント名／hotkey 登録・表示経路の変更」トリガーに当たらない）

**スキル**
- **`/race-check` は必須**（`AGENTS.md` の該当行「スレッド/窓をまたぐ共有状態」＋ ADR 自身が「`resolve_opener` は錠を替える変更なので `/race-check` の対象になる」と予告）
- `/symmetric-check` — **不要**（生成/破棄・対称ペアを触らない）
- `/persistence-check` — **不要**（永続形式に触れない）
- `/dry-check` — 分岐 (c) を採るなら該当（関数を新規定義するため）

**手動の 1 手（強く推奨）**
- **禁止に違反を注入して赤くなることを測る**——`clippy.toml` 冒頭が「パスを足す・変えるときは必ず測ること」と要求している。今回はパスを**変えない**ので厳密には対象外だが、**注釈の数が 4 → 1 に減る**ため、残る 1 件が本当に足として効いているかを（禁止行を一時的に壊す変異で）確かめる価値がある。`clippy.toml` の経路 3（cargo fingerprint）を避けるため `.rs` を touch するか `cargo clean -p snotra` を挟むこと

### 新しいテストの要否 → **要らない**（根拠つき）

1. **値の同一性は既存テストが固定済み**: `state.rs::app_state_config_is_the_same_arc_the_engine_holds` が「engine への `update_config` が `AppState.config` の読みへ届く」を測る。移設先が同じ Arc であることの証拠はここに在る
2. **非干渉も既存テストが固定済み**: `state.rs::ui_reads_config_while_the_engine_lock_is_held` が「engine 錠の保持中に別スレッドが config を読み切れる」を測る。これは移設の**受け入れ条件そのもの**であり、3 か所が新たに乗る性質と同一
3. **古い分類の残存は自己強制される**: `#[expect]` は違反が消えれば不履行で赤になる。「注釈を消し忘れる」形のテストを書く必要が無い（それが `#[allow]` でなく `#[expect]` を選んだ理由である）
4. **`AGENTS.md`「`Option`／フラグ／enum variant など**どの分岐が選ばれるかを決める値**の出所を変更」トリガーの逆向き検算**: 本件は分岐を決める値の**出所（読み口）**を変えるが、**読む Arc も値も同一**であり「この値で初めて走る行」は**ゼロ**である（`find_matching_tools` の入力も `show_icons` の真偽も、取りうる集合が変わらない）。ゆえに「新しく生きた組み合わせ」は生じない
5. **書けないもの**: I-1〜I-4（guard 内で I/O・錠を取らない／順序逆転）は**構造の規律**であり、決定的なテストを書く手が無い（タイミング依存・`#[cfg(windows)]`）。`ADR-config-read-exception-discriminator` の案 G が「否定形の述語は型で表せない」と既に裁定している

**ただし 1 点だけ、テストではなく検算を要求する**: I-5（icon キャッシュ破棄の窓）が広がっていないことは**推論で導いた**。実測していない（§7 ⚠️-2）。

---

## 6. `clippy.toml` 群 3 の扱い（撤去条件の**逐語判定**）

### 逐語（`src-tauri/clippy.toml` 141-146 行）

> **この群には撤去条件がある。** `.config()` と UFCS の 2 通りで全 crate を走査した範囲では、`Engine::config` の呼び出しは src-tauri の例外地点と snotra-core 自身のテストにしか無い（2026-08-18 実測）。ゆえに**最後の `#[expect]` が消える変更**は、同じコミットでこの群のエントリと `REQUIRED_DISALLOWED_METHODS` の行を消し、`Engine::config` を `pub(crate)` へ落とすこと——そこから先は lint ではなく**コンパイルエラー**が規範を守り、この群も注釈も弁別子も要らなくなる。合図はマージ済みの事象（最後の注釈が消えること）であって、issue の開閉ではない。

### 判定: **発火しない**

- 条件節は「**最後の** `#[expect]` が消える変更」である。#1123 が消すのは **4 件中 3 件**（`launch.rs` × 2 ／ `icon.rs` × 1）で、`config_watcher.rs:88` の 1 件が残る
- 残ることは issue 本文自身が保証している（「`config_watcher` の旧 config 読みは別勘定である…**本 issue の対象ではない**」）
- **整合の裏取り**: 仮に `Engine::config` を `pub(crate)` へ落とすと、`src-tauri` の `config_watcher.rs:91`（`state.engine.lock().unwrap().config().clone()`）が**コンパイル不能**になる。つまり撤去条件は、config_watcher が射程外である限り**構造的に発火しえない**
- **`.config()` の全走査**（`src-tauri/src` ／ `snotra-core/src` ／ `snotra-core/tests` ／ 他 2 crate）で得た呼び出しは 7 件: `launch.rs:111` / `launch.rs:166` / `icon.rs:21` / `config_watcher.rs:91` / `engine.rs:476,485,590`（snotra-core 自身のテスト）。**clippy.toml の 141-143 行の記述は変更後も真のまま**

### ゆえに #1123 で行うこと

- ✅ `disallowed-methods` 配列の **`snotra_core::engine::Engine::config` エントリはそのまま残す**（158 行）
- ✅ `G-clippy-disallowed.mjs` の **`REQUIRED_DISALLOWED_METHODS` はそのまま**（`"snotra_core::engine::Engine::config"` を消さない）
- ✅ `Engine::config` の可視性は **`pub` のまま**
- ✅ 群 3 の**コメント本文だけ**を直す（例外装置の消滅に伴い偽になる箇所 — §1-B）
- ⚠️ **前提 1（注釈が 1 つ以上残ること）の支えが 1 本になる**ことを群 3 のコメントへ書き足すこと。**その 1 本を将来 `read_config` 側へ移す人が、撤去条件の全文（エントリ削除 + カナリア削除 + `pub(crate)` 化）を丸ごと相続する**。今その旨を書かないと、次の人は「3 件消して何も起きなかった」という前例だけを見る

---

## 7. ADR の要否

### 判定: **新しい ADR を 1 本書くことを推奨する**（`docs/adr/ADR-config-live-read-without-exceptions.md` 等・slug は担当者が決める）

**根拠**
1. **否定の知識が実際に生じる**（`AGENTS.md`「否定の知識が生じた決定のみ」）: (i) #1076 が採った「例外を残す」という現状維持を**なぜ今覆すか**、(ii) **`config_watcher` をなぜ移さないか**（弁別子がスレッドではなく手続きだから）＝それゆえ群 3 が注釈 1 本で生き延びること、(iii) 分岐 (a)/(b)/(c) のうち採らなかった枝とその理由（fallback の捏造）
2. **`ADR-config-read-exception-discriminator` が明示的に「#1123 で評価する」と書いており、その評価結果が着地する先が要る**。着地先が PR 本文や issue だけだと、**リポジトリの grep に入らない**（squash で commit message にはなるが、写しの数え上げの母集団外である）
3. **既存 ADR は編集しない**（`ADR-adr-frozen-history`: ADR は凍結された歴史。「変わることに対しては守らない」）。32 行の「#1123 で評価する」も**そのまま残す**——凍結の初適用がこの契約自身であった

**機構面の裏取り**
- `src-tauri/CLAUDE.md` 57 行から `ADR-config-read-exception-discriminator` への短縮引用を落としても **G-adr-citations は赤にならない**——同検査が測るのは「引用された slug が実在するか」であって「ADR が引用されているか」ではない（`G-adr-citations.mjs` 実測）。ADR ファイルを**消すのではない**ので `docs/adr/` は無傷
- ただし**新 ADR は旧 ADR を `ADR-config-read-exception-discriminator` の形で文脈として引くこと**を推奨する（歴史の辺を実在検査に載せ、読者が前サイクルへ辿れる）
- `clippy.toml` 91 行の ADR 引用は **`.toml` ゆえ G-adr-citations の母集団外**（同ファイルが 4-5 行で自ら「.toml は照合されない」と書いている）。人が直す

**ADR を書かない選択もありうる根拠（両論併記）**: 本件は「規則を 1 文へ縮める」だけで新しい設計判断が少ない、と読むこともできる。ただし上記 (ii) の「config_watcher を残す判断」は**次の人が必ず問い直す論点**であり、これ 1 つで否定の知識の要件を満たす。**推奨は「書く」だが、決定はチームに属する**（規範＝セーフティネットの変更なのでルート `CLAUDE.md`「最重要ルール 2」により合意が要る——**本件はコード変更より前に、条項そのものの改訂への合意が要る**）。

---

## 8. ⚠️ 確信が持てない点

1. **⚠️ 設計分岐を決めていない。** §2 の (a)/(b)/(c) は**どれも成立する**。枝によって直す散文が違う（(b)/(c) は `read_config` の doc「唯一の口」と条項第 1 文を追加で書き換える）。**この列挙は「枝が決まるまで完全にならない」**——実装前に枝を確定させること
2. **⚠️ I-5（icon キャッシュ破棄の窓）が広がらないことは推論であり、実測していない。** 「2 つの錠を別々に取る構造は不変だから読める値の集合も不変」という論証で、`update_config` が engine 錠を取ってから `config.write()` を取るまでの微小区間で **read 側が旧値を読める**という新しい相対順序が生じないかを、コードの逐語読みだけで判断した。**`/race-check` の枠でここを名指しで検算すること**
3. **⚠️ `config_watcher.rs:89` の reason 文字列をどう書き直すかで、条項の残る形が決まる。** 「弁別子が他の例外と違う」を消したあと、config_watcher の読みを何と呼ぶか（「射程外」「適用手続きの一部」）が条項本文の書き方と噛み合う必要がある。**本レビューは文言案を持たない**
4. **⚠️ 条項から削る文の「今どこに在るか」を全数は確かめていない。** `app.listen` の同期実行（同ファイル「Win32 メッセージ配送の注意」）と `on_event_loop` のスレッド（`proof.rs`）は他に SSOT があることを確認したが、**「動機と判定を分ける」「列挙で覚えない」という方法論そのものの SSOT は確かめていない**。条項から消えると、その教訓が凍結 ADR にしか残らなくなる可能性がある（ADR は生きた層ではない）。**削る前に 1 文ずつ着地先を書き出すこと**
5. **⚠️ `resolve_all_openers` は `pub` である。** crate 外からの呼び出しは無い（`[lib]` を持たない bin crate なので構造的に無い）はずだが、**LSP の findReferences では取り直していない**（grep のみ）。分岐 (a) を採る場合、シグネチャ変更の影響範囲は findReferences で確定させること
6. **⚠️ 性能は測らないと決めているが、`config` `RwLock` の read 競合が増える影響を評価していない。** 3 か所が新たに `config.read()` を取る。`config_watcher` の `config.write()` は稀なので実害は無いはずだが、**測っていない**。issue 自身が「性能は変わらない・得るのは統治だけ」と宣言しているので**測らない判断は妥当**だが、「変わらない」と**書く**なら測ってからにすること（全称表現は前提条件とセットで）
7. **⚠️ `governance:check` の面積計器（恒久規範の面積）に条項の縮小がどう出るかは見ていない。** 合否は持たない計器なので障害にならないが、報告値が動く
8. **⚠️ 本件はセーフティネット（規範文書）の変更である。** ルート `CLAUDE.md`「最重要ルール 2」により **Claude が単独で判断しない**。この導出は判断材料であって決定ではない
