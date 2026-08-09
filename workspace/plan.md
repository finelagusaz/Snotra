# 実装計画 — issue #1000 起動の端から端までの計器

## 目的

プロセス作成から `hotkey:registered`（＝ホットキーを押せば窓が出る状態）までを **1 本の時間軸**で刻み、内訳を release ビルドで得られるようにする。

**新しい総和を作るのではない。** 粗い総和は `smoke-startup.ps1` の `first_trace_ms` が既に持っており、それが **0.6s / 5.2s / 8s超**（debug・CI runner での実測）とばらついて**原因未解明**である。この計器が答えるべきは「その分散がどの区間に住むか」である。

## 受け入れ条件

1. release ビルドで `SNOTRA_TRACE=1` を立てると、起動の内訳が 1 行で stderr へ出る。**イベント名が終端の意味を運ぶ**——ホットキー登録に成功したら `startup:ready`、失敗したら `startup:failed`（**内訳の中身は同一**）
2. **`pre_main_ms`（プロセス作成 → `main()` 突入）が独立した項目として出る**——分散がここに住む可能性を排除できないため、測らないと区間の外に落ちる
3. **全区間のキーが必ず出る**（`Phase` enum の全 variant を列挙）。通らなかった区間は `null`、キーの欠落は異常。ハーネスが毎回検査し、**枝フラグが説明しない `null`・キー欠落・post-main の生 ns 検算の不一致**で失敗させる。**`pre_main` は壁時計、以降は単調時計であり、両者をまたぐ `total == pre_main + Σ` を単一時計の厳密な不変条件にはしない**（下の「丸めは表示境界でだけ行う」が正本）
4. **索引ロード区間は `LoadOrScanStats.total_ms` との差を `index_load_unattributed_ms` として出す**——`load_or_scan_with_stats` の中にある未命名の処理を、外側の計器が捕まえられるようにする。**first-run 枝では `LoadOrScanStats` 自体が存在しない**ので、この項目も `index_load` と揃えて `null` にする（**0 にしない**）
5. どの枝を通ったか（`cache_hit` / `first_run` / `include_path_env`）が同じ行に出る——反復 11 の「計器が測る枝と変更が触る枝が同じか」を読み手が毎回確かめられる
6. 計器が無効（`SNOTRA_TRACE` 未設定）のとき、マーク関数のコストが実質ゼロである
7. `scripts/bench-startup.ps1` が時間の内訳を主・メモリを従として N 回まわし、各項目の min / p50 / max を出す
8. 現運用点で **3 標本以上**のベースラインが取れ、`PERFORMANCE.md` へ記録されている

## 設計

### 基準点は 1 つ、以降は単調時計で刻む

```
プロセス作成（GetProcessTimes の lpCreationTime）
   │  ← pre_main_ms（唯一の壁時計の引き算）
main() 突入 = anchor: Instant::now()
   │  ← 以降はすべて anchor.elapsed()（単調・時刻補正の影響を受けない）
   …
hotkey 登録完了 ★終端 → ok なら startup:ready / 失敗なら startup:failed
```

### 終端の名前が「押せる状態」を運ぶ（Codex 敵対的レビューの指摘・high）

当初この計画は、登録失敗でも `startup:ready` を出し `ok: false` を data へ載せる形だった。**それは名前と意味が食い違う。** `hotkey::register` は `RegisterHotKey` 失敗（キー競合・不正設定）で false を返すので、**ホットキーが使えない起動が `startup:ready` として green になる**——ベースライン取得も CI も通ってしまう。

「同じイベントのまま、ハーネスが `data.ok == true` を必須にする」案は採らない。**ハーネスが 1 つ検査を忘れた瞬間に沈黙で通る**形であり、この issue が潰そうとしている false-green そのものだからである（#690 / #786 と同じ型）。

**イベント名で分ける**:

| 終端 | イベント | ハーネス |
|---|---|---|
| platform bridge の初期化・初回 command 送信・`RegisterHotKey` の**すべてが成功** | `startup:ready` | 成功 |
| bridge の spawn / Win32 初期化 / 初期化結果の受信 / bridge state の取得 / 初回 command 送信のいずれかが失敗、または hotkey 登録失敗 | `startup:failed` | **理由つきで失敗させる** |

**ハーネスは両方を「終端が届いた」として待つ。** `startup:ready` だけを待つと、失敗時に**タイムアウト**になり「終端が出なかった」という誤った理由で落ちる——起きたこと（登録失敗）が読めない。両方待って `startup:failed` を明示的な失敗として扱えば、理由がそのまま出る。

**内訳は失敗側でも同じものを出す**——どこまで進んだかは診断に要る。変わるのは名前と、ハーネスの判定だけである。

#### 終端は `RegisterInitialHotkey` の arm だけに閉じない（Codex 敵対的レビュー 2 巡目の指摘・実コードで裏取り済み）

**arm だけに置くと、実在する失敗経路がタイムアウトに化ける。** 裏取りの結果:

| 経路 | 実装の現状 |
|---|---|
| `PlatformBridge::begin` | platform スレッドの `Builder::spawn(...).ok()?`——spawn 失敗で `None` |
| `PlatformBridgePending::wait` | `thread_id_rx.recv().ok()?`、および受信した thread ID が `0` のとき `None`。`platform_thread_loop` は `GetModuleHandleW` / `CreateWindowExW` 失敗時に `send(0)` して return する |
| `setup_platform_thread` | `None` のとき**記録も通知もエラー返却もせず setup を続行する** |
| `setup_hotkey_listener` | bridge state 不在・`lock()` 失敗のとき、**`RegisterInitialHotkey` を送らずに return する** |
| `PlatformBridge::send_command` | **channel の send 失敗を戻り値に出さず捨てる**——bridge が manage 済みでも platform スレッドが死んでいれば command は処理されない |

**ゆえに終端は 3 か所から出す**（すべて同じ `finish(Err(..))` を通す）:

1. `main.rs` の `setup_platform_thread` — `begin` / `wait` の失敗
2. `main.rs` の `setup_hotkey_listener` — bridge state 不在・lock 失敗・初回 command の送信失敗
3. `platform/mod.rs` の `RegisterInitialHotkey` の arm — command が届いた後の登録成否

**失敗理由は安定した文字列で載せる。** `PlatformBridge::begin` / `PlatformBridgePending::wait` は `Option` では原因を失うので `Result<_, PlatformBridgeFailure>` にし、初回 command の送信も成功／切断を呼び出し側へ返す API にする。`startup.rs` の `StartupFailure` がそれを `reason` へ写す——**OS 依存のエラー文字列をハーネスの契約にしない**。

**終端の一度きり性（CAS）がここで効く。** platform 初期化に失敗した後、`setup_hotkey_listener` が bridge 不在を観測して**二つ目の失敗行**を出す経路が実在するため。

**壁時計の引き算を 1 か所（`pre_main_ms`）に閉じる**のが要点である。全マークを `SystemTime` で取ると時刻補正・分解能の影響が全区間に乗る。

### 丸めは表示境界でだけ行う（Codex 敵対的レビュー 2 巡目の指摘・medium）

当初この計画は、区間を `Option<u64>`（ミリ秒）で持ちながら `total == pre_main + Σ` の**厳密な一致**を要求していた。**その 2 つは両立しない。**

- 隣接区間を個別に `as_millis()` へ落とすと、タイミングが正しくても**丸め境界で和が合わない**（例: 各 500,000 ns の 2 区間は ms では 0 + 0、総計は 1）
- かといって `total` を丸め済みの部分和から作れば、検査は**同語反復**になり、本来捕まえたい基準点の取り違え・単位の誤りを 1 つも検出しない

**区間は生の ns（`Duration`）で保持し、`*_ms` は出力時に `ns / 1_000_000` へ切り捨てる。厳密に検算するのは生 ns だけである。**

検算の形: 終端で `anchor.elapsed()` を**直接**読んだ `post_main_elapsed_ns` と、各 `Phase` の境界差分の `Σ phase_ns` を比べる。**前者を部分和から作らない**ことが要点で、そうして初めて開始点・終点の取り違え、単位変換の誤り、先頭／末尾の区間漏れが検出できる（内部マークの取り落としは引き続き `Phase` の網羅列挙と `null` 検査が担う——別の検知器である）。

**`pre_main` と post-main は別の時計である。** 前者は creation FILETIME と `SystemTime::now()` の差、後者は `Instant` の差であり、共通の原点も分解能も持たない。`pre_main_ns + post_main_elapsed_ns` は**表示用の複合値にはできても、単一の時計で測った経過時間ではない**。ゆえに両者をまたぐ厳密加法を不変条件にせず、**時刻補正を有限の許容誤差で一般に吸収する契約も置かない**（誤差を許すと、それが基準点ずれを隠す）。

**残余の形が `LoadOrScanStats` とは違う。** あちらは独立に測った区間の和を total から引くので残余が現れる。こちらは 1 つの基準からの累積ゆえ**構造的に残余ゼロ**であり、未命名の処理は「隣り合うマークの間の大きな差」として現れる。

### 総和の検算は「マークの取り落とし」を原理的に検出できない（独立レビューの指摘・要対処）

当初この計画は、受け入れ 3（`total == pre_main + Σ`）を `LoadOrScanStats` の「残余」の代役として置いていた。**それは役目を果たさない。** 累積タイムラインの区間和は telescoping sum であり、マークを 1 つ落とすと**その区間の時間は隣の区間へ吸収されるだけで等式は崩れない**。「検知器を変異で落とす」と書きながら、その変異を落とす手段が計画に無かった。

**構造で解く**: 区間を「取ったマークの列」ではなく、**`Phase` enum の全 variant に対する `Option<u64>` の表**として持つ。出力は全 variant を必ず列挙し、**通らなかった区間は `null` として出す**（欠落キーにしない）。

| 値 | 意味 |
|---|---|
| 数値 | その区間を通り、この時間かかった |
| `null` | **その区間を通っていない**（枝がスキップした・マークを取り落とした） |
| キーが無い | **異常**——ハーネスが失敗させる |

これで「マークを 1 つ落とす」変異は `null` として現れ、ハーネスのキー検査が落ちる。**`null` と `0` を区別する**のが要点である（#690 / #786 が踏んだ「未測定と 0 の混同」と同じ型）。

`null` が出てよいのは、同じ行に出る枝フラグがそれを説明するときだけである:

| `null` になる区間 | 説明する枝フラグ |
|---|---|
| `index_load` | `first_run = true`（`load_or_scan_with_stats` を呼ばない） |
| `path_merge` | `include_path_env = false`（既定） |

ハーネスは**枝フラグが説明しない `null` を失敗として扱う**。

### マーク一覧（この順に単調増加する）

| マーク | 位置 |
|---|---|
| `pre_main` | プロセス作成 → `main()` 突入（DLL ロード・CRT 初期化・AV スキャン） |
| `config_load` | `Config::is_first_run` + `Config::load_reporting` |
| `index_load` | `load_or_scan_with_stats`（内訳は `LoadOrScanStats` を data へ埋める） |
| `path_merge` | `scan_path_env` + `extend_with_path_entries`（**既定 `include_path_env = false` では `null`**——「通っていない」。**0 と書いてはならない**） |
| `history_load` | `HistoryStore::load` |
| `engine_build` | `Engine::from_material` |
| `tauri_init` | `generate_context!` から `.setup` 突入まで |
| `windows_create` | `egui_shell::create`（**フォント解決を含む**） |
| `setup_rest` | listener 登録 → `RegisterInitialHotkey` 送信まで |
| `hotkey_register` | platform スレッドでの `RegisterHotKey` 完了まで |

### 終端は一度きりである

`hotkey:registered` は config 変更による**再登録でも出る**（`platform/hotkey.rs` の `trace_registration` は初回専用ではない）。終端マークが 2 度目を出すと、起動と無関係な行が `startup:ready` として現れる。**一度きり性を型と検知器の両方で持つ。**

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 種別 |
|---|---|---|
| `src-tauri/src/startup.rs` | 新規。`mark()` / `finish()` / `Phase`・プロセス作成時刻の取得 | 追加 |
| `src-tauri/src/main.rs` | `mod startup;` + `fn main` / `.setup` 内の各マーク。加えて `setup_platform_thread`（bridge の `begin` / `wait` 失敗）と `setup_hotkey_listener`（bridge state 不在・lock 失敗・初回 command の送信失敗）を `finish(Err(StartupFailure))` へ渡す | 変更 |
| `src-tauri/src/platform/mod.rs` | `PlatformBridge::begin` / `PlatformBridgePending::wait` を `Result<_, PlatformBridgeFailure>` へ（`Option` は原因を失う）・`send_command` の初回送信結果を返す・`RegisterInitialHotkey` の arm に終端（**`platform/hotkey.rs` の `trace_registration` ではない**——あれは `SetHotkey` の再登録と共有され、config 変更のたびに通る） | 変更 |
| `scripts/bench-startup.ps1` | 全面書き換え（時間を主・メモリを従・WebView2 期の子孫走査を撤去） | 変更 |
| `src-tauri/CLAUDE.md` | モジュール構成へ `startup.rs` の行（`AGENTS.md`「条件別チェック」の `.rs` 追加トリガー） | 変更 |
| `docs/build-commands.md` | 実行コマンド（コマンド文字列の SSOT） | 変更 |
| `PERFORMANCE.md` | 「計測と受け入れ基準」へ計器の所在 + ベースライン | 変更 |

## 実装順序

### Phase 1 — Rust 側の時間軸

- [ ] `src-tauri/src/startup.rs` を追加する（`//!` に責務・基準点の設計・一度きり性の理由を書く）
- [ ] プロセス作成時刻を `GetProcessTimes` で取る（`Win32_System_Threading` は `Cargo.toml` に既存・確認済み）。非 Windows は `pre_main` を持たない形に落とす
- [ ] `mark(Phase)` を `trace_enabled()` の早期 return で守る（無効時のコストを消す）
- [ ] `main.rs` の各地点へマークを置く（**並びを変えない**——マークを足すだけ）
- [ ] 区間を**生の ns（`Duration`）で保持**し、`*_ms` は出力時にだけ切り捨てる。終端で `anchor.elapsed()` を**直接**読んだ `post_main_elapsed_ns` を、部分和とは別に出す
- [ ] `platform/mod.rs` の `PlatformBridge::begin` / `PlatformBridgePending::wait` を `Result<_, PlatformBridgeFailure>` へ変え、初回 command の送信結果を呼び出し側へ返す
- [ ] 終端を **3 か所**から呼ぶ（`setup_platform_thread` / `setup_hotkey_listener` / `RegisterInitialHotkey` の arm）。**登録の成否でイベント名を分けて** 1 行出す（`startup:ready` / `startup:failed`。内訳は同一・`reason` は `StartupFailure` が安定文字列へ写す）
- [ ] **`SystemTime::now()` の分解能を Rust 側で 1 度実測する**（下の未確定 1 の残り。誤差上限では押さえてあるので、値を記録するだけでよい）
- [ ] **実装差分へ `/race-check` を当てる**（計画段階では起動しない設計のため・#784）
- [ ] ユニットテスト: (a) マークが単調増加する (b) 終端が二度目を出さない (c) 無効時に何も出さない
- [ ] `Phase` を enum で持ち、出力は**全 variant を網羅列挙**する（`..` を書かない——`SearchEngine::footprint_rows` と同じ形で、区間を足したときの漏れをコンパイラに捕まえさせる）
- [ ] **検知器を変異で落とす**: (a) 一度きり性を外す (b) **マークを 1 つ落とす**（→ 当該区間が `null` になりキー検査が落ちる。**総和の検算は落ちない**ことも同時に確かめ、なぜ別の検査が要るかを doc に残す） (c) 枝フラグを固定値にする (d) 総和の検算を無条件 true にする (e) **登録失敗でも `startup:ready` を出す**（→ ハーネスが失敗すること。**登録を実際に失敗させて**測る——占有済みホットキーを config に置けば `RegisterHotKey` が false を返す） (f) **`include_path_env = false` で `path_merge` を 0 にする**（→ 枝フラグ整合検査が落ちること）
- [ ] **丸め・基準点の変異**: (g) 各 500,000 ns の 2 区間 + 終端 1,000,000 ns の fixture（→ **生 ns 検算は通り、ms 和の厳密一致を要求する誤ったハーネス検査は落ちる**。この 1 本が「丸めを表示境界に閉じる」の根拠である） (h) `post_main_elapsed_ns` を `Σ phase_ns` から代入する（同語反復化。→ fixture で終端値を意図的にずらすとこの変異は**不正に通る**——「終端値を直接取らない実装」を落とすテスト） (i) `post_main_elapsed_ns` の基準を anchor 以外のマークへ差し替える (j) 終端値を command **送信**時点で固定し platform 側の登録完了を含めない (k) ns→ms の除数を `1_000` にする (l) `total == pre_main + Σ phase_ms` を再導入する（→ (g) の fixture で落ちる）
- [ ] **失敗経路の変異**: (m) `begin` を `Err(Spawn)` に (n) `platform_thread_loop` の初期化通知を `Err(CreateWindow)` に (o) managed bridge を取得できなくする (p) 初回 command の send を失敗させる (q) `setup_platform_thread` 側の失敗終端を削除する（→ 「終端なしのタイムアウト」がテスト成功扱いにならないこと） (r) 失敗終端の CAS を外す（→ 初期化失敗と bridge 不在を同時に模擬したとき一度きり検査が落ちる）——**いずれもタイムアウトではなく `startup:failed` と安定した `reason` で落ちること**
- [ ] **上のすべてで実際に落ちることを実測する**（反復 8 で 3 本中 1 本が落ちなかった教訓）
- [ ] **⚠ 由来の検証項目**（Codex が確信を持てないと明示した 3 点。**未確定として残さず、測る対象に変える**）
  - [ ] `PlatformBridgePending::wait` の **channel 切断が本番でどう起きるか**は特定できていない（`recv().ok()?` の失敗経路は実在するが、thread panic 等の原因は未確定）。**原因の特定は成立条件にしない**——変異 (n) / (p) で経路を模擬し、終端が出ることだけを測る。特定できなかったことは `startup.rs` の `//!` へ受容する残余として書く
  - [ ] `Mutex<PlatformBridge>` の **poison を作る既知経路は未確認**。人為的に poison させて変異 (o) の経路が `startup:failed` を出すことを測る（`setup_hotkey_listener` の `let Ok(b) = bridge.lock()` が明示的に無視している経路である）
  - [ ] **`GetProcessTimes` の creation 取得と `SystemTime::now()` を採る順序**を決め、`pre_main_ns` が**負にならない**ことと、順序による誤差の向き・大きさを測る（どちらの順でも `Instant` と同一時計にはならないので、上の「異時計をまたぐ厳密加法を契約にしない」判定は変わらない）
- [ ] カテゴリ A の検証（`docs/build-commands.md`）

### Phase 2 — ハーネス

- [ ] `scripts/bench-startup.ps1` を書き換える。`scripts/lib/SnotraSmoke.psm1` の待ち合わせ・trace パースを再利用する
- [ ] **`startup:ready` と `startup:failed` の両方**を終端として待ち、どちらも出なければ**失敗させる**（沈黙を合格と読ませない・#471 / #690 の型）。`startup:failed` が来たらそれを理由つきの失敗として扱う——**`startup:ready` だけを待つとタイムアウトになり、「終端が出なかった」という誤った理由で落ちる**
- [ ] 受け入れ 3 の 3 検査をハーネス側で行い、いずれかが破れたら失敗させる——**キーの過不足** / **枝フラグが説明しない `null`** / **`post_main_elapsed_ns` と `Σ phase_ns` の不一致**（**ms 表示値の和は検査しない**）。JSON の欠落フィールドを PowerShell が黙って `$null` に落とす経路（`Set-StrictMode` の効き方）を実測してから書く
- [ ] `startup:failed` の `reason` を**そのまま**出力へ載せる（ハーネス側で分類名を書き起こさない——写しが 2 部になる）
- [ ] 各項目の min / p50 / max を出す。**分散こそが観測対象**なので最小値だけに畳まない
- [ ] メモリは従として残し、**子孫プロセス走査を撤去**する（現構成のプロセスツリーは 1 件）
- [ ] env の復元が空文字を作らないこと（#872 の実測。`SNOTRA_TRACE` は `env_flag` ゆえ空文字は「無効」に落ちるが、復元の形は smoke 群と揃える）

### Phase 3 — 測定と記録

- [ ] release ビルドで現運用点のベースラインを **3 標本以上**取る（同日・同セッション）
- [ ] **`SNOTRA_TRACE` の有無で `first_trace_ms` を比較**し、計器自身が系を乱していないことを測る（`SNOTRA_EGUI_INPUT_TRACE` は 1 行 17〜56ms の前例がある）
- [ ] `PERFORMANCE.md`「計測と受け入れ基準」へ計器の所在とベースラインを書く
- [ ] `docs/build-commands.md` へ実行コマンドを足す
- [ ] `src-tauri/CLAUDE.md` のモジュール構成へ `startup.rs` の行を足す
- [ ] `npm run governance:check` を通す

## 不変条件と異常系

| 不変条件 | 壊れたときの症状 | 検知手段 |
|---|---|---|
| マークは単調増加する | 負の区間が出る | ユニットテスト + ハーネスの検算 |
| 終端は一度きり | config 変更のたびに `startup:ready` が出て、起動の記録が上書きされて見える | ユニットテスト（変異で落ちることを実測） |
| 全区間のキーが出る（`Phase` を網羅列挙） | **マークの取り落としが隣の区間へ吸収され、誰にも見えない** | ハーネスのキー検査（**総和の検算では原理的に捕まらない**——telescoping sum ゆえ等式は崩れない） |
| `post_main_elapsed_ns == Σ(非 null の phase_ns)` | 基準点・終点の取り違え、区間の単位誤り、先頭／末尾の区間漏れ | 終端で anchor から**直接**取った生 ns と、境界差分の部分和をハーネスが厳密検算する。**ms 表示値の和は検査しない**（丸め境界で正しくても合わない） |
| `pre_main` と post-main を異時計のまま混同しない | 時刻補正や丸め差を計器の不良と誤認する／逆に許容誤差が基準点ずれを隠す | `pre_main_ns` を独立項目として出し、複合表示値を厳密検算の対象から外す |
| `null` と `0` を混同しない | 「通らなかった」が「0 ms で通った」に見える。**`path_merge` が典型**——既定 `include_path_env = false` はスキップであって 0 ms ではない | 枝フラグが説明しない `null` をハーネスが失敗させる（逆に、枝フラグが `null` を要求するのに数値が来ても失敗させる） |
| 終端の名前が readiness を運ぶ | **ホットキーが使えない起動が green になる** | イベント名を分ける（`startup:ready` / `startup:failed`）。ハーネスは両方待ち、後者を失敗として扱う |
| 終端がどの失敗経路でも出る | **bridge の初期化失敗が「タイムアウト」に化け、診断したい相手が読めない** | 3 か所（`setup_platform_thread` / `setup_hotkey_listener` / arm）から同じ `finish(Err(..))` を通す。変異試験で経路ごとに落とす |
| 無効時にコストゼロ | 製品の起動が計器のぶん遅くなる | `trace_enabled()` の早期 return + Phase 3 の A/B |
| 枝が出力に現れる | cache-miss の標本を cache-hit の基準と比べる | 出力に `cache_hit` / `first_run` / `include_path_env` |

**異常系**: `GetProcessTimes` が失敗したら `pre_main_ms` を `null` として出す（**0 にしない**——測れなかったことと 0 ms は別である）。総和の検算はそのとき pre_main を除いて行う。**同じ規則が全区間に当たる**（上の「`null` と `0`」の表）——`null` は「通っていない・測れていない」、`0` は「通ったが 1 ms 未満」である。

**`RegisterInitialHotkey` の一度きり性は型では保証されていない**（独立レビューの軽微 1）。現状のコードでは 4 経路（single-instance の second-instance コールバック / `config_watcher` / hotkey 登録失敗のリトライ / `snotra-settings` からの再適用）のいずれもこの variant を送らないことを grep で実測したが、**将来リトライが足されれば黙って崩れる**。ゆえに CAS による一度きり性を**二重の守り**として残し、その理由を `startup.rs` の `//!` に書く。

## テスト方針と検証コマンド

- ユニットテスト（`startup.rs` の `#[cfg(test)]`）: 単調性・一度きり性・無効時の沈黙
- **Win32 依存部（`GetProcessTimes`）はユニットテスト前提にしない**（`.claude/rules/src-tauri.md`）——測るのは Phase 3 の実起動
- 検証コマンドは `docs/build-commands.md` カテゴリ A（fmt / clippy / test）を SSOT として参照する。**`cargo test -p snotra --lib` は使わない**（`src-tauri` は `[lib]` を持たない・`src-tauri/CLAUDE.md`）
- ガバナンス文書に触るので `npm run governance:check`（カテゴリ F）

## SPEC.md・関連文書の更新要否

- **`SPEC.md` は更新しない。** 計器の追加であって、文書化された挙動（フロー・状態遷移）を変えない。`SNOTRA_TRACE` が無効な製品の挙動は 1 バイトも変わらない
- **この計器は常設である**（`AGENTS.md`「調査・測定のための一時的な足場」の撤去条件を持たない）。上流の改修（#1001 / #1003 / #1004）の前後で同じ器を当てることが存在理由であり、issue が閉じても残る。**その判断を `startup.rs` の `//!` に書く**——足場と常設を読み手が区別できるようにする

## 未確定（実装前に潰す）

- [x] **`GetProcessTimes` の creation FILETIME → UNIX epoch 換算が正しいか** — **実測した**。既知プロセスの `StartTime` と、`(FILETIME - 116444736000000000) / 10000` の逆変換を突き合わせ、差は **0.716 ms**（ms へ切り捨てたぶんだけ）。定数は正しい
- [x] **`SystemTime::now()` の分解能が `pre_main_ms` に足りるか** — **誤差上限で押さえて可とした**。壁時計の引き算は `pre_main_ms` の 1 か所だけであり、Windows の最悪粒度 15.6 ms を仮定しても、観測対象（`first_trace_ms` の実測 0.6〜8 s の分散）に対して **0.2〜2.6%** で判定に影響しない。**PowerShell で測った 0.0015 ms は .NET の時計であって Rust std の値ではない**ので根拠に使わない（代理で測らない・`AGENTS.md`「主張は代理ではなく対象そのもので測ってから書く」）。実際の値は Phase 1 で 1 度記録する
- [x] **`hotkey:registered` が本当に「押せる状態」の成立点か** — **成立点である**。根拠 3 点: (a) `platform/hotkey.rs` の `register_prepared` は `RegisterHotKey(Some(HWND::default()), …)`＝platform スレッドのメッセージキューへ登録し、そのスレッドの `GetMessageW` ループは既に回っている (b) `HOTKEY_PRESSED` の listener は `RegisterInitialHotkey` 送信**より前**に登録される（`setup_hotkey_listener` の doc が「Order must not change」と明記） (c) 窓の生成（`egui_shell::create`）は setup の中で hotkey listener **より前**にある。ゆえに登録成功の直後に押せば窓が出る。**登録失敗のときは終端の名前を変える**（`startup:failed`）——当初は「`ok: false` を載せて `startup:ready` を出す。押せないことは別の欠陥である」と書いていたが、それだと**ホットキーが使えない起動が計器の上では成功に見える**（Codex 敵対的レビューの high。上の「終端の名前が『押せる状態』を運ぶ」が正本）
- [x] **終端がスレッドをまたぐことの安全性** — **構造で解いた**。`PlatformCommand::RegisterInitialHotkey` は `SetHotkey`（config 変更の再登録）と**別の variant** であり、送られるのは起動時の 1 回だけである。終端の**成功側**をこの arm に括れば、config 変更による再登録は終端に触れない。happens-before は platform bridge のチャネルが持つ（マーク 9 の後にコマンドを送り、platform スレッドがそれを受けてマーク 10 を書く）ので、順序に追加のガードは要らない。**ただし終端は arm だけには閉じない**——bridge の初期化失敗など arm 自体が実行されない経路が実在するため、`main.rs` の 2 か所からも `finish(Err(..))` を通す（上の「終端は `RegisterInitialHotkey` の arm だけに閉じない」が正本）。CAS による一度きり性は**二重の守りではなく必須**になった——初期化失敗の後に bridge 不在をもう一度観測して二つ目の失敗行を出す経路が実在する。**`/race-check` は計画段階では起動しない設計**（母集団が差分・#784）ゆえ、実装差分へ当てる（Phase 1 の項目）

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の 2 条件に該当——共有状態をスレッドが跨いで書く / ガバナンス文書（`PERFORMANCE.md`・`docs/build-commands.md`・`src-tauri/CLAUDE.md`）を変更する）
- plan-review: 独立レビュー1体（Step 2・観点を 2 個に絞る）＋ **Codex 敵対的レビュー 1 回**（ユーザー起動・`/codex:adversarial-review`）
- エージェント数: 1（+ 外部レンズ 1）
- 要対処: **8 件を計画へ反映済み**
  - 自己照合 2 件: (1) 終端を `trace_registration` から `RegisterInitialHotkey` の arm へ移した（再登録との共有を断つ） (2) `/race-check` を計画段階の検査から実装差分の検査へ移した
  - 独立レビュー 2 件（**どちらも観点 B**）: (3) **総和の検算は telescoping sum ゆえマークの取り落としを原理的に検出できない**——区間を `Phase` の網羅列挙 + `Option` の表にし、通らなかった区間を `null` として必ず出す形へ設計変更した（キー検査が別途要る） (4) **first-run 枝では `LoadOrScanStats` が存在せず `index_load_unattributed_ms` が定義不能**——`null` と `0` の区別を全区間の規則へ格上げした
  - Codex 敵対的レビュー 2 件: (5) **[high] ホットキー登録失敗でも `startup:ready` として green になる**——終端をイベント名で分けた（`startup:ready` / `startup:failed`）。「同じイベント + ハーネスが `ok` を検査」案は**検査を忘れた瞬間に沈黙で通る**ため採らなかった (6) **[medium] `path_merge` の既定枝が「`null`」と「ゼロ」で計画内矛盾**——`null` に統一し、`0` は「実行したが 1 ms 未満」に限定した。**この矛盾は 2 つの内部レビューを通り抜けた**（自己照合と独立レビューはどちらも表と表の突き合わせをしていない）
- 軽微（独立レビュー・**どちらも観点 A**、反映済み）: (1) `RegisterInitialHotkey` の一度きり性は 4 経路の grep で現状は正しいが型では保証されない → CAS を二重の守りとして残す理由を doc へ書く (2) マーク 1〜9 を渡す共有データ構造が計画未記載 → Phase 1 の実装差分へ `/race-check` を当てる（プロセスとして妥当と評価された）
  - Codex 敵対的レビュー **2 巡目** 2 件（**fix-forward への再実行で出た**——`AGENTS.md`「修正は指摘箇所へ注意が集中し、周辺に新しい誤りを生む」の実例。うち (8) はまさに (5)(6) の修正で導入した検算の周辺だった）: (7) **[medium] platform bridge の初期化失敗に終端が無く、タイムアウトに化ける**——終端を 3 か所へ広げ、`begin` / `wait` を `Result` 化し `reason` を安定文字列にした（実コードで 5 経路を裏取り済み） (8) **[medium] ミリ秒の丸め契約が未定義**——区間を生 ns で保持し丸めを表示境界へ閉じた。検算は `post_main_elapsed_ns`（終端で anchor から**直接**取る）対 `Σ phase_ns` で行い、**`pre_main` と post-main が異時計であることを明示**して両者をまたぐ厳密加法を契約から外した
- 未検証: (a) `SystemTime::now()` の実分解能（誤差上限で押さえ、Phase 1 で記録する） (b) ⚠ ハーネスの JSON 欠落フィールド処理（`Set-StrictMode` 下で欠落キーがどう落ちるか）——Phase 2 で実測してから書く
- **Codex が ⚠ として返した 3 点は「未確定」ではなく Phase 1 の検証項目へ落とした**（channel 切断の本番原因・`Mutex` poison の既知経路・creation 取得と `SystemTime::now()` の順序）。**原因の特定を成立条件にしない**——変異で経路を模擬し、終端が出ることを測る形に変えてある

## 人間レビュー

- [x] 承認済み — 2026-08-09 / 問い: "承認する旨をひと言いただければ、次の形へ更新して Step 6（workspace のコミット・push）へ進み、そのうえで `/implement` を回します。" / 回答: "承認する"
