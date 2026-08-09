# 独立レビュー — issue #1000 起動の端から端までの計器（`workspace/plan.md`）

観点は 2 つのみ: **A** = 終端マークがスレッドを跨ぐ設計の並行性、**B** = 新設する検知器が false-green を塞ぐか（逆向き監査）。

## 要対処

### [観点 B] 「総和の検算」は`マークを 1 つ落とす`変異を構造的に検出できない

- 対象: `workspace/plan.md`「Phase 1 — Rust 側の時間軸」の `**検知器を変異で落とす**: 一度きり性を外す / マークを 1 つ落とす / 総和の検算を無条件 true にする — それぞれで実際に落ちることを実測する`、および受け入れ条件 3「`total_ms == pre_main_ms + Σ(区間)`。ハーネスが毎回検算し、ずれたら失敗させる」
- 設計は「単一の基準点からの累積」であり、計画自身が明言している通り「**構造的に残余ゼロ**であり、未命名の処理は『隣り合うマークの間の大きな差』として現れる」（`workspace/plan.md`「基準点は 1 つ、以降は単調時計で刻む」節）。これは telescoping sum（隣接差分の総和は端点の差に等しい）そのものであり、**実装から `mark(Phase::史ある1個)` の呼び出しを 1 行削除しても、削られた区間の所要時間はそのまま隣接フェーズの区間へ吸収されるだけで、`total_ms` と `Σ(観測された区間)` の等式は変わらず成立する**。したがって「マークを1つ落とす」変異を注入しても、accept 条件 3 の検算は原理的に赤くならない。
- Phase1/Phase2 の記述には、この変異を捕まえる**別の**検知器（例: 期待するフェーズキー集合が JSON payload に過不足なく揃っているかのスキーマ検証）が明記されていない。ハーネスがフィールドアクセスで例外を出すかどうかに賭ける実装は、`.claude/rules/safety-nets.md`「フォールトインジェクションでは…変異が『実際に起きた回帰の姿』と同じかを確かめる」が要求する**意図した検知器**とは言えない。
- 対処案: 受け入れ条件 3 とは別に、**期待するフェーズキー集合（`Phase` enum の全 variant）が出力 JSON に過不足なく存在すること**をハーネス（または Rust 側の `finish()` 自身）が明示的に検算する一文を計画へ追加する。これなら「マークを1つ落とす」変異が確実に赤くなる。

### [観点 B] `index_load_unattributed_ms` の first-run 枝で「未測定」と「0 ms」が区別されていない

- 対象: `workspace/plan.md` 受け入れ条件 4「索引ロード区間は `LoadOrScanStats.total_ms` との差を `index_load_unattributed_ms` として出す」、および「異常系」節（`pre_main_ms` の null 扱いのみ明記）
- `src-tauri/src/main.rs` の `fn main` で、`is_first_run` 分岐は `load_or_scan_with_stats` を**呼ばない**（`IndexMaterial::from_tree(IndexTree::empty())` で済ませる）。ゆえに first-run では `LoadOrScanStats` そのものが存在せず、`index_load_unattributed_ms`（外側の計器区間 − `LoadOrScanStats.total_ms`）は定義できない。
- 計画は `pre_main_ms` について「測れなかったことと 0 ms は別である」と明記し null 扱いを義務付けているが（`workspace/plan.md`「異常系」）、**同型の状況である first-run の `index_load_unattributed_ms` には同じ扱いが書かれていない**。ここを 0 として出すと、「未命名の処理が無かった（差分ゼロ）」という意味と「そもそも比較対象が存在しない」という意味が区別できなくなる——これは issue 本文・`smoke-startup.ps1` の #690/#786 コメントが記録する「計器が何も出さなかった」と「計器が出したが中身が空/ゼロ」の混同そのものである。
- 対処案: 「異常系」節に first-run 枝を明記し、`index_load_unattributed_ms` を null（またはフィールド省略）として出すこと、およびその場合の受け入れ条件 3 の検算からの除外を書く。

## 軽微

### [観点 A] `RegisterInitialHotkey` の「一度だけ」は型ではなくコードの現状に依存している

- 対象: `workspace/plan.md`「終端は一度きりである」節・未確定 3 番目の項目
- 実測（grep）: `PlatformCommand::RegisterInitialHotkey` を送る呼び出しは `src-tauri/src/main.rs` の `setup_hotkey_listener` 内 1 箇所のみ（`b.send_command(PlatformCommand::RegisterInitialHotkey);`）。計画が挙げた 4 経路をすべて確認した:
  - single-instance の second-instance コールバック（`main.rs` の `tauri_plugin_single_instance::init` クロージャ）は `egui_shell::show_egui_main` を直接呼ぶだけで、`setup` を再実行せず `RegisterInitialHotkey` も送らない。
  - `config_watcher.rs` の `apply_config_change` はホットキー変更を `PlatformCommand::SetHotkey`（**別 variant**）で送る（`config_watcher.rs:104` 付近）。
  - 登録失敗後のリトライは無い——`platform/mod.rs` の `RegisterInitialHotkey` arm は失敗時に `INITIAL_HOTKEY_FAILED` を emit するだけで、`egui_shell::register_initial_hotkey_failure_listener`（`egui_shell/mod.rs`）は窓を表示するのみで再送しない。
  - `snotra-settings` からの再適用も `config.toml` 経由で `config_watcher` → `SetHotkey` に合流する、同じ別 variant。
  - `.setup()` 自体も `RuntimeRunEvent::Ready` の処理として 1 回しか呼ばれない（`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」で tauri 2.11.4 の一次資料確認済みと記録）。
- ゆえに計画の裁定（終端を `RegisterInitialHotkey` arm へ置く）は**現状のコードに対しては正しい**。ただし、この「一度だけ」性は Rust の型やコンパイラでは保証されておらず、**将来 `INITIAL_HOTKEY_FAILED` を受けて再登録を試みる機能が足された場合**（自然な次の一歩に見える）、`RegisterInitialHotkey` の 2 度目の送信が黙って再導入されうる。CAS による一度きり性ガードはこの場合も `startup:ready` の**二重出力だけ**を防ぐので、実害は表面化しない（計測が壊れるだけで、他機能は壊れない）が、計器の正しさが**将来のコード変更を追跡する仕組みを持たずに**現状のみへ依存している。
- 対処案: `platform/mod.rs` の `PlatformCommand::RegisterInitialHotkey` variant の doc コメントへ「この variant は起動時に厳密に 1 回だけ送られる前提で終端計測を担っている。新しい送信箇所（リトライ等）を追加する前に `startup.rs` の一度きり性設計（`//!`）を確認すること」という制約を明記する（`AGENTS.md`「条件別チェック」の粒度に合わせた自己文書化）。実装を妨げるものではないため軽微とした。

### [観点 A] マーク 1〜9 のスレッド間受け渡し機構が計画に明記されていない

- 対象: `workspace/plan.md`「変更ファイル一覧と対象シンボル」の `src-tauri/src/startup.rs`（新規。`mark()` / `finish()` / `Phase`）
- `finish()` は platform スレッドの `RegisterInitialHotkey` arm から呼ばれ、1 行の `startup:ready` に main スレッドで記録されたマーク 1〜9 の内訳を含めて出す設計（受け入れ条件 1・「マーク一覧」表）。この「main スレッドで書いた値を platform スレッドが読む」経路が **どんな共有データ構造**（`Mutex<Vec<_>>` か `OnceLock` か等）を使うかが計画に書かれていない。
- happens-before 自体は `PlatformBridge::send_command`（`platform/mod.rs`）が `command_tx.send()` を `PostThreadMessageW` より先に呼ぶ構造になっており、`std::sync::mpsc` の send/recv 対が同期を提供するため、**送信直前までの main スレッドの書き込みは受信後の platform スレッドから可視である**——これは実測（`platform/mod.rs` を読む限り）健全に見える。ただし共有データ構造自体の選択を誤ると（例えば `static mut` を素朴に使う等）、この happens-before の恩恵を受けられない実装になりうる。
- 計画は「実装差分へ `/race-check` を当てる」（Phase 1 チェックリスト）と明記しており、この点は意図的に実装段階へ委ねられている（`AGENTS.md`「条件別チェック」の worker/channel/共有状態トリガーに一致）。プロセスとしては妥当なので軽微とした——ただし `/race-check` を当てる際に「マーク 1〜9 のストレージが `send_command` の happens-before エッジを跨いで安全か」を明示的な検査項目にするとよい。

## 未検証

### [観点 B] ⚠ ハーネスの「フィールド欠落」時の挙動が未確定で、要対処①の深刻度に影響する

- 要対処①で指摘した「マークを1つ落とす」変異について、ハーネス（`scripts/bench-startup.ps1`、未実装）が JSON の欠落フィールドへ厳格アクセス（存在しなければ例外で落ちる）を採用するか、`?? 0` のような寛容なデフォルトを採用するかは実装時点で決まる。前者なら**意図せず**この変異を検出できてしまう可能性があり、後者なら要対処①がそのまま顕在化する。どちらになるかは計画からは読み取れないため、要対処①の指摘（専用のスキーマ完全性検査を置くべき）は実装方針に関わらず有効だが、「現状の設計で本当に赤くならないか」は実装を見るまで確信が持てない。

### [観点 A] ⚠ `SystemTime::now()` 分解能の実測値が計画にまだ記載されていない

- `workspace/plan.md`「未確定（実装前に潰す）」の 2 番目の項目は「実際の値は Phase 1 で 1 度記録する」としており、現時点では記録されていない（`git diff` 等では確認できない未実装段階）。観点 A の直接の争点ではないが、`pre_main_ms` の壁時計精度が最終的に受け入れ判定へ効く経路のため、実装後に値が計画通りの範囲（0.2〜2.6%）に収まるかは未検証のまま残す。

---

## 返り値要約

**要対処（2件・いずれも観点B）**
1. 「総和の検算」（`total_ms == pre_main_ms + Σ`）は累積タイムライン設計の telescoping 性質により、マークを1つ落とす変異を原理的に検出できない——専用のスキーマ完全性検査（期待するフェーズキー集合の過不足なき存在）を別途置く必要がある。
2. first-run 枝では `LoadOrScanStats` が存在せず `index_load_unattributed_ms` を計算できないが、計画の異常系節は `pre_main_ms` の null 扱いしか明記しておらず、この枝で 0 ms と未測定が区別されない設計のままになっている。

**軽微（2件・いずれも観点A）**
1. `RegisterInitialHotkey` が起動時に一度だけ送られる、という終端設計の前提は現状コード（grep実測・single-instance/config_watcher/リトライ/snotra-settings の4経路すべて確認）に対しては正しいが、型で保証されておらず、将来リトライ機能が足されると黙って崩れうる——variant の doc に制約を明記すべき。
2. マーク1〜9をmainスレッドからplatformスレッドへ渡す共有データ構造が計画に未記載。mpsc channelのsend/recv自体はhappens-beforeを提供するが、格納方式次第では恩恵を受けられない実装になりうる（計画は実装段階の`/race-check`へ意図的に委ねており、プロセスとしては妥当）。

**未検証（2件）**
1. [観点B] ハーネスがJSON欠落フィールドを厳格アクセスするか寛容にデフォルトするかで、要対処①の顕在化有無が変わる（実装を見るまで未確定）。
2. [観点A] `SystemTime::now()`分解能の実測値がまだ記録されておらず、`pre_main_ms`の精度が計画の見積り（0.2〜2.6%）内に収まるかは未検証。
