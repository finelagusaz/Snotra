# 調査 — issue #1000 計器: 起動の端から端までを測る器が無い

## issue の要約

起動段の計測は `LoadOrScanStats`（`load_or_scan_with_stats` の中）しか無く、その外側（`main.rs` の直列部分・`scan_path_env`・engine 構築・tauri 初期化・窓生成・フォント解決）がどのフェーズにも現れない。**区間の和ではなく全体を 1 つの器で測り、残余を明示する**計器を置く。

## issue の前提の検算（着手前に必ず行う・`AGENTS.md`「検証の作法」の全称否定）

**issue 本文の「起動の端から端までを測る器が無い」は、そのままでは偽である。** 起票時に `scripts/` を grep していなかった。実在するものを列挙する。

| 既存 | 測るもの | 測らないもの |
|---|---|---|
| `scripts/bench-startup.ps1` | プロセスツリーの `WorkingSet64` とプロセス数（5 回平均） | **時間を 1 つも測らない**。`SNOTRA_TRACE=1` を立てて stderr をファイルへ落とすが、**そのファイルを一度も読まない** |
| `scripts/smoke-startup.ps1` | `first_trace_ms` = `Start-Process` から**最初の `[trace]` 行**までの壁時計 | 内訳。1 つの塊としてしか出ない |
| `LoadOrScanStats`（`indexer.rs`） | hash / cache_load / cache_read / digest / scan / sort / cache_save / total | `load_or_scan_with_stats` の外側すべて |

### 訂正後の正しい問題文（3 点）

1. **起動の時間軸を刻む trace が 1 つも無い。** `src-tauri` の trace イベント名を全列挙したところ、起動経路に在るのは `hotkey:registered` / `hotkey:listener_enter` だけで、**`startup:*` は存在しない**（`egui_*` は show 以降・`index*` は indexing 状態）
2. **`LoadOrScanStats` は release では出力すらされない。** `main.rs` の `eprintln!("[index-load] ...")` は `#[cfg(debug_assertions)]` の中に在る。**製品ビルドで起動段の内訳を得る手段が現在ゼロである**
3. **`first_trace_ms` は既に「粗い端から端まで」になっている。** `hotkey:registered` は platform スレッドで `RegisterHotKey` 完了時に出る（`platform/hotkey.rs` の `trace_registration`）＝**setup のほぼ末尾**であり、そこまでの区間は「プロセス開始 → config ロード → 索引ロード → PATH → engine 構築 → tauri 初期化 → 窓生成 → hotkey 登録」を丸ごと含む

### そして、その粗い数字は既に異常を示している

`smoke-startup.ps1` のコメント（L4-6, L67-69）が実測を記録している:

> バイナリで最初の trace までが **0.6s / 5.2s / 8s超** と大きくばらつく
> （L138）**起動レイテンシの分散はまだ原因未解明**であり、予算内に収まっていても数字が残れば、悪化の傾向を人が読める

**本 issue が置くべきものは「新しい総和」ではなく「この既知の分散を説明できる内訳」である。**

**ただし条件を混同しないこと**——この 0.6〜8s は `smoke-startup.ps1` の既定である **debug ビルド**を **CI runner** で走らせた値である。開発機の release とは別の運用点であり、**同じ数字が出る保証は無い**。

## 関連ファイル・モジュール・関数

### 起動の直列区間（`src-tauri/src/main.rs` の `fn main`）

`tauri::Builder` より前に、次が**同期で直列に**走る:

| 位置 | 呼び出し | 既知の額（出典） |
|---|---|---|
| `Config::is_first_run()` / `Config::load_reporting()` | config.toml 読み | 未計測 |
| `indexer::load_or_scan_with_stats` | 索引ロード（or 全走査） | ロード 80 ms（v7・`PERFORMANCE.md`）。`LoadOrScanStats` が内訳を持つ |
| `indexer::scan_path_env` + `extend_with_path_entries` | PATH 併合（`include_path_env` が真のときだけ） | 54〜73 ms（実測・反復 9）。**既定は `false`** |
| `HistoryStore::load()` | history.bin 読み | 未計測 |
| `Engine::from_material` | 索引構築（`PathStore::build` 込み） | 66〜78 ms（実測・反復 3） |
| `tauri::generate_context!()` | コンパイル時生成物の展開 | 未計測 |

`.setup(...)` の中（tauri のイベントループの 1 イテレーション内・`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」）:

| 順 | 呼び出し | 備考 |
|---|---|---|
| 1 | `setup_platform_thread` | Win32 初期化を並列化する意図（SPEC §8.5） |
| 2 | `egui_shell::create` | 窓生成 + **フォント解決**（`font_stack.rs`。`PERFORMANCE.md` の A/B ではアイドル時点で 20.6 MiB 効いており、表示より前に走ることが判明している） |
| 3 | 各種 listener 登録 | |
| 4 | `setup_hotkey_listener` | `RegisterInitialHotkey` を platform bridge へ送る → **`hotkey:registered` がここで出る** |
| 5 | `setup_config_watcher` | |
| 6 | `setup_background_rescan` | 低優先度スレッドを spawn（**この中身は #1001 の射程**） |
| 7 | `setup_tray` | SPEC §7.5 でここが最後 |

### 計器の既存パターン

- **ランタイムの計測は `SNOTRA_TRACE=1` の構造化トレース**（`PERFORMANCE.md`「計測と受け入れ基準」が SSOT として明記）。`src-tauri/src/trace.rs` の `trace(event, data)` が `seq`（プロセス大域の単調カウンタ）+ `ts_ms`（UNIX epoch ミリ秒）+ event + data を JSON 1 行で stderr へ出す
- **egui/softbuffer 側の計器は 5 つの env** で、**受理値の判定は `snotra-egui-runtime/src/env.rs` の 1 箇所**に集約（空文字は未設定）。`SNOTRA_TRACE` だけは `trace.rs` の `env_flag`（`1|true|yes|on`）と別の意味論
- **PowerShell ハーネスの型**: `scripts/lib/SnotraSmoke.psm1` を import し、`Start-Process` + stderr リダイレクト + `[trace]` 行を JSON パースして集計（`smoke-startup.ps1` が手本）
- **A/B の作法**: 同日・同セッション・各 3 回以上・最小値どうしを比較（`PERFORMANCE.md` 反復 9 の規約）。**受け入れ判定は確保回数を主、壁時計を従とする**

## 技術的制約

### 1. `Instant::now()` を `main()` の先頭で取っても「プロセス開始」ではない

`main()` に入る前に、**DLL ロード・CRT 初期化・（環境によっては）AV スキャン**が走る。`smoke-startup.ps1` が記録した 0.6〜8s の分散が**そこに丸ごと住んでいる可能性を排除できない**。

`main()` 基準で刻むと、分散が計測区間の外に落ち、**「内訳の和は 300 ms なのに外から見ると 8 s」という読めない結果**になる。これは反復 6 が踏んだ「どのフェーズにも現れない処理」と同じ形である。

**対処**: `GetProcessTimes(GetCurrentProcess(), ...)` の `lpCreationTime`（FILETIME）を基準に取る。`Win32_System_Threading` は `src-tauri/Cargo.toml` の `windows` feature に**既に在る**（確認済み）。

### 2. 計器自身が系を乱す

`SNOTRA_EGUI_INPUT_TRACE` は「runner では stderr 1 行が 17〜56ms かかる」と実測されている（`PERFORMANCE.md`「計測と受け入れ基準」）。起動段に足す行数は一桁の見込みだが、**行数を増やすほど測っている対象が動く**。

### 3. 分岐で通る経路が違う

| 分岐 | 通る区間 |
|---|---|
| first-run（`config.toml` 不在） | 索引ロードごと skip・`initial_indexing = true` |
| cache-hit | ロード + 背景再スキャン spawn |
| cache-miss | 全走査 + save（**秒のオーダー**・#1001） |
| `include_path_env` | 既定 `false`。真のとき PATH 区間が乗る |

**反復 11 の教訓（計器が測る枝と変更が触る枝が同じか先に確かめる）**がそのまま当たる。どの枝を測ったのかが出力に現れないと、cache-miss の標本を cache-hit の基準と比べる事故が起きる。

### 4. `main.rs` の `[index-load]` は debug 限定

release で内訳を得るには trace 側へ載せる必要がある（上の「訂正後の正しい問題文」2）。

## 再利用できる既存パターン

- `trace.rs` の `trace()`（`ts_ms` が既に在るので、区間の差分は後段で計算できる）
- `scripts/lib/SnotraSmoke.psm1` の trace 行パース・プロセス起動・待ち合わせ
- `LoadOrScanStats` の「**残余を出す**」型（`total - Σphases`。反復 6 が `digest_ms` を足して塞いだときの形。`tests/memory_footprint.rs` が同じ形で残余を出している）

## 未解決の疑問

1. **終点をどこに置くか**（`hotkey:registered` = 押せる状態 / 窓が出て検索できる状態）——後者は #1004 の射程と重なる → **ユーザーへ確認する**
2. **`bench-startup.ps1` をどうするか**——memory 専用で、WebView2 期の子孫プロセス走査（現構成では常に 1 プロセス）を抱えたまま。`SNOTRA_TRACE` を立てて stderr を捨てている → **ユーザーへ確認する**
3. プロセス作成時刻を基準にしたとき、`ts_ms`（UNIX epoch）との突き合わせ精度が足りるか——**実装前に代表値で測る**（`AGENTS.md`「計画に書いた判定ロジックは実装前に代表入力で実行して測る」）
4. 0.6〜8s の分散が pre-main に住むのか main 以降なのか——**この計器が答えるべき問いであって、前提にしてはならない**
