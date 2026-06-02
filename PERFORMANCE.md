# パフォーマンス最適化プレイブック

検索/表示の体感遅延を改善するときは、次の順で着手すると最短で効果が出やすい。

1. 待ち時間を潰す（体感改善の即効枠）
   - 入力デバウンスは leading edge（初回即時発火）+ trailing 50ms。旧値は 150ms trailing のみだった
   - 古い非同期リクエスト結果の破棄（request id / generation）
   - `show` / `setSize` / `setPosition` などウィンドウ操作の不要呼び出し削減
   - OS 呼び出し待機を伴う処理（例: `launch_item`）は `timeout` を明示し、UI 側で `launching` と失敗通知を表示して「無反応」に見せない
   - 失敗通知の自動クリアは単一タイマーで管理し、再通知時は `clearTimeout` してから再設定する
   - **Win32 IPC の cold-call を避ける**: Tauri のウィンドウ API（`is_visible()` / `show()` 等）は内部で Win32 IPC を使い、WebView2 への初回アクセスで数十ms のオーバーヘッドが発生する。ホットパス上で状態確認目的に使う場合は `AtomicBool` 等で Rust 側にキャッシュし、Win32 IPC をスキップする。冪等な操作（`show()` 等）は pre-check なしで直接呼ぶ
2. 重複処理を消す（低リスク・高効率）
   - 同一データの二重取得（例: アイコン batch 取得）を責務分離して一本化
   - 同一状態を複数イベントで配信しない。結果表示は `results-sync`（`generation` + `shouldShow`）の単一契約で同期する
   - 画像・バイナリデータの IPC 転送は `tauri::ipc::Response` を使い base64 を排除する（転送量 ~25% 削減、Rust 側 encode コスト削減、フロント側 decode コスト削減）。`invoke<ArrayBuffer>` で受け取り `URL.createObjectURL(new Blob([buf]))` で表示する
3. 計算量を下げる（中〜大規模データ向け）
   - 毎回の全件ソートを top-k 抽出へ置換
   - ループ内の再計算（正規化や同一変換）を事前計算へ移動
   - `Mutex<T>` を保持したまま `SHGetFileInfoW` 等の OS IO を行うと、並列 IPC リクエストが Rust 側で直列化される。フロントの並列性（`Promise.allSettled`）を活かすには、IO 完了後にロックを取得する設計（キャッシュ済みバイト列の返却のみをクリティカルセクションに限定する等）を検討する
4. 描画のマイクロ最適化を行う（仕上げ）
   - 文字幅計測や省略文字列生成のキャッシュ
   - 無意味な再スクロール/再レイアウト抑制

## ビルドプロファイル最適化の知見

### Cargo ワークスペースのプロファイル上書き

クレート単位の `opt-level` を変更するには、ワークスペースルートの `Cargo.toml` に
`[profile.release.package.<name>]` セクションを追記する。**ワークスペースメンバーの
`Cargo.toml` に書いた `[profile.*]` は Cargo に無視される**（ワークスペースでは無効）。

```toml
# Cargo.toml (ルート) — 正しい書き方
[profile.release.package.snotra-settings]
opt-level = "s"
```

### opt-level = "s" 適用結果（issue #138）

| クレート | 変化 | 判断 |
|---------|------|------|
| `snotra-settings` | 4.0 MB → 3.8 MB (−5%) | 採用（低頻度起動） |
| `snotra` (src-tauri) | 6.7 MB → 5.5 MB (−18%) | 採用（ホットパスなし） |
| `snotra-core` | fuzzy +33%, new +11% | **不採用** |

`snotra-core` は `nucleo-matcher` + rayon の並列スコアリングがホットパスであり、
`opt-level = "s"` によるループアンローリング・SIMD 抑制が直撃する。
**`snotra-core` への `opt-level = "s"` / `"z"` 適用は行わない**。

## アイドルメモリ（working set）の削減（2026-06-01〜02 計測）

ランチャーは 99% 非表示常駐のため、アイドル時の物理メモリ（working set / private RSS）の最小化が重要。installed release ビルド（32GB/SSD 機）で WebView2 プロセスツリー（main + webview2 子孫 計 7 プロセス）を実機計測した。

### 実態（Private Working Set = プロセス固有の物理 RAM）

- 非表示アイドルの定常 baseline: **~110MB**。Working Set 表示値 ~390MB との差 ~280MB は Edge ランタイム共有 DLL（`msedge.dll` ~310MB 等）のページで、プロセス間共有・デマンドページゆえ Private には乗らない（「Snotra=390MB」は誤読）
- 内訳（`VirtualQueryEx` で committed をタイプ別分類）: WebView2 エンジン固有 ~64MB（browser 33 + renderer/V8 18 + utility/crashpad 13、アプリ制御不可）+ Rust 本体 ~30MB + GPU プロセス ~18MB
- Rust 本体 ~30MB の正体: index は極小（`index.bin` 数百 bytes）ゆえデータではなく、Tauri/WRY/tokio/serde/windows クレート + アロケータ保持の baseline（committed 42.8MB の内訳はヒープ 40.7MB / スレッドスタック 2.17MB）

### 採用: hide 時の `EmptyWorkingSet`（issue #355 / PR #360）

hide 経路で Win32 `EmptyWorkingSet` をプロセスツリー全体へ能動適用し、アイドル常駐を **110MB → 数MB** へ回収する。実装は `src-tauri/src/working_set.rs`、パターン詳細は `src-tauri/CLAUDE.md`「WebView2 working set の能動回収」節。

| 状態 | Private WS |
|---|---|
| アイドル baseline | ~110 MB |
| hotkey hide 後（自動 trim） | ~9.4 MB |
| frontend(Escape) hide 後（自動 trim） | ~22.8 MB |

再表示レイテンシは劣化しない（trim 無/有とも ~41ms、メモリ圧迫下でも 44ms、ウィンドウ出現はブロックされない）。ページは standby/pagefile に退避され show 時に OS が透過 re-fault する（圧迫下のみハード読込 ~934/s ≈ 3.6MB/s で SSD では体感不能）。削減対象は物理 working set であって commit（~195MB 不変）ではない。frontend 経路が hotkey より浅い（22.8 vs 9.4MB）のは `notify_main_hidden` が tokio で `win.hide()` と並行するタイミング差（#361 で polish）。

### 効かなかった/見送った手法

- **TrySuspend / MemoryUsageTargetLevel.Low 単独**: 論理目標を下げるだけで、メモリ圧迫のない実機では OS が物理 working set を回収しない（表示↔非表示・120 秒放置で 110MB 不変）。主効果は CPU 中断。working set 回収には `EmptyWorkingSet` の能動適用が必要（両者は別レイヤーで補完的）
- **tokio / rayon の worker スレッド削減**: Rust 本体は Tauri 既定の multi-thread tokio（worker = CPU コア数）+ rayon プールで ~50 スレッドを抱えるが、スレッドスタックは計 2.17MB（committed 42.8MB の 5%）に過ぎず、worker を絞っても RSS 削減は <1MB で**無意味**。30MB の大半はヒープ/フレームワーク baseline
- **`--disable-gpu --disable-gpu-compositing`**: GPU プロセスは消えない（in-process ソフト合成に切替）が Private 18→6MB・合計 110→99MB。ただしブラウザフラグは Microsoft 非サポート（ランタイム更新で挙動変化）＋ CPU 合成化で描画レイテンシ未検証。限界効用は `EmptyWorkingSet`（~107MB）に対し桁違いに小さく見送り
- **アイコン PNG 圧縮強化（issue #335）**: 16×16 アイコンで実測 ~0.06MB と桁違いに小さく却下（詳細は #335）

## 試みたが機能しない手法

- **Custom URI Scheme（`snotra-icon://` 等）による画像配信**: WebView2 では `register_uri_scheme_protocol`（WRY/Tauri）で登録したカスタムスキームへのリクエストが、WebView2 環境生成時の `SetCustomSchemeRegistrations` 事前宣言なしにはハンドラーに届かない。WRY 0.54.x では自動的に処理されず、`eprintln` 診断でハンドラーが一切呼ばれないことを確認済み。バイナリ配信の代替は `tauri::ipc::Response`（上記セクション2）を用いること。

- **`SearchEngine` の並列 Vec を `CachedEntry` 構造体に統合**（issue #110、branch `refactor/cached-entry`）:
  保守性改善を目的に、8本の並列 Vec をフィールドごとにまとめた `CachedEntry` 構造体への移行を試みた。
  実測で **Fuzzy full scan が 35〜120% 遅化** し却下。

  | エントリ数 | 並列Vec（現行） | CachedEntry | 増加率 |
  |----------:|---------------:|------------:|-------:|
  |     1,000 |         ~7 ms  |      ~14 ms |  +97%  |
  |    10,000 |        ~14 ms  |      ~21 ms |  +49%  |
  |    50,000 |        ~14 ms  |      ~30 ms | +120%  |
  |   100,000 |        ~22 ms  |      ~29 ms |  +35%  |

  原因: `char_masks`（`Vec<u64>`）は 8エントリ/キャッシュライン で bitmask プリフィルタを高速に走査できる。
  `CachedEntry`（~160 bytes/entry）に埋め込むと同じ走査で **~25倍のキャッシュライン**を消費する。
  並列 Vec のレイアウトはキャッシュ局所性のために意図的に維持している。
  詳細は `snotra-core/src/search.rs` の `SearchEngine` 構造体コメントを参照。

  **採用した別案（branch `refactor/entry-view-accessor`）**: AoS 統合の代わりに `EntryView<'a>` アクセサパターンを導入。
  `entry_view(i)` が 4 本の並列 Vec の参照（`entry` / `lower_name` / `lower_file_name` / `normalized_key`）を束ねて返すことで、スコアリングループの可読性を向上させた（4 行 → 1 行）。`char_masks` / `file_name_char_masks` はプリフィルタのキャッシュ効率を保つため EntryView に含めず SearchEngine から直接アクセスする。
  `#[inline]` により性能への影響はゼロ（メモリレイアウト不変）。`new()` 末尾の `debug_assert!` で全 Vec 長の同期を検証する。

## 計測と受け入れ基準

- 変更ごとに「入力 → 検索結果反映」までの遅延を観測し、体感を先に確認する
- 体感改善後、必要なら p50/p95 を追加計測して次のボトルネックを特定する
- 原則として「待ち時間」「重複」「計算量」「描画」の順を崩さない
- 計測ログ実装（`ui/src/lib/perf.ts`）は恒久保持し、削除しない
- 計測は **DEV かつ `localStorage.snotra_perf === "1"`** のときのみ有効化する
- 有効化手順: DevTools で `localStorage.setItem("snotra_perf","1")` → アプリ再起動
- 無効化手順: `localStorage.removeItem("snotra_perf")` → アプリ再起動
- コアロジックのマイクロベンチは `ignored` テストとして保持する。実行コマンドは [docs/build-commands.md](docs/build-commands.md) の「検索パフォーマンス計測」を SSOT として参照
- フォルダ列挙のホットパス確認には bench フィルタを絞る: `cargo test -p snotra-core bench_folder_ -- --ignored --nocapture`（SSOT の bench コマンドに対するフィルタ違いとして個別記載）
- `bench_folder_narrow_filter` は「大量エントリ + 狭いフィルタ」で、非一致エントリに不要な文字列化や属性判定をしていないかを確認する
- `bench_folder_hidden_filter_all` は `show_hidden_system = false` 相当で、`metadata()` を伴う属性判定コスト込みの回帰を確認する
- Tauri Driver E2E でウィンドウ可視性を判定するとき、`document.visibilityState` は誤判定し得るため性能判定の根拠に使わない。`plugin:window|is_visible` を優先する
- E2E 全体実行時間はテスト待機タイムアウトの影響を強く受ける。性能評価では、E2E 所要時間だけでなく `perf.ts` の p50/p95 と trace を併用する

## 計測ベースライン（2026-03-06）

環境: Windows 11 Home, release ビルド（`cargo test --release -p snotra-core bench_ -- --ignored --nocapture`）

### ファジー検索（`bench_fuzzy_search_scaling`）

| エントリ数 | 平均レイテンシ |
|----------:|-------------:|
|     1,000 |       54 µs  |
|    10,000 |      378 µs  |
|    50,000 |    1,314 µs  |
|   100,000 |    2,088 µs  |
|   300,000 |    6,923 µs  |

### Engine 初期化（`bench_new_scaling`）

| エントリ数 | 平均時間 |
|----------:|---------:|
|     1,000 |    < 1 ms |
|    10,000 |     9 ms  |
|    50,000 |    42 ms  |
|   100,000 |   124 ms  |
|   300,000 |   202 ms  |

### フォルダ列挙（`bench_folder_*`、max_results=50）

| ベンチ | エントリ数 | 平均時間 |
|--------|----------:|---------:|
| topk_sort | 1,000 | 2,677 µs |
| topk_sort | 5,000 | 7,491 µs |
| topk_sort | 10,000 | 14,199 µs |
| folder_narrow | 1,000 | 3,043 µs |
| folder_narrow | 5,000 | 6,279 µs |
| folder_narrow | 10,000 | 10,011 µs |
| folder_hidden_all | 1,000 | 3,251 µs |
| folder_hidden_all | 5,000 | 8,899 µs |
| folder_hidden_all | 10,000 | 14,829 µs |

### ホットキー表示レイテンシ（`show_main_and_emit`、SNOTRA_TRACE=1 計測、2026-03-07）

Win32 IPC cold-call 最適化（`is_visible()` → `AtomicBool`）適用後の値。

| 条件 | `show_main:total` | 備考 |
|------|------------------:|------|
| cold（初回） | 77ms | うち ~41ms はトレース I/O オーバーヘッド |
| warm（2回目以降） | ~38ms | |
| cold（トレースなし推定） | ~36ms | トレース I/O 分を差し引いた推定値 |

最適化前の cold は 191ms だった（`is_visible()` pre-check 61ms + ギャップ 71ms + `is_visible_after_show` 39ms）。
