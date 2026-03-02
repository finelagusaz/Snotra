# パフォーマンス最適化プレイブック

検索/表示の体感遅延を改善するときは、次の順で着手すると最短で効果が出やすい。

1. 待ち時間を潰す（体感改善の即効枠）
   - 入力デバウンス見直し
   - 古い非同期リクエスト結果の破棄（request id / generation）
   - `show` / `setSize` / `setPosition` などウィンドウ操作の不要呼び出し削減
   - OS 呼び出し待機を伴う処理（例: `launch_item`）は `timeout` を明示し、UI 側で `launching` と失敗通知を表示して「無反応」に見せない
   - 失敗通知の自動クリアは単一タイマーで管理し、再通知時は `clearTimeout` してから再設定する
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
  `entry_view(i)` が 6 本の並列 Vec の参照を束ねて返すことで、スコアリングループの可読性を向上させた（6 行 → 1 行）。
  `#[inline]` により性能への影響はゼロ（メモリレイアウト不変）。`new()` 末尾の `debug_assert!` で全 Vec 長の同期を検証する。

## 計測と受け入れ基準

- 変更ごとに「入力 → 検索結果反映」までの遅延を観測し、体感を先に確認する
- 体感改善後、必要なら p50/p95 を追加計測して次のボトルネックを特定する
- 原則として「待ち時間」「重複」「計算量」「描画」の順を崩さない
- 計測ログ実装（`ui/src/lib/perf.ts`）は恒久保持し、削除しない
- 計測は **DEV かつ `localStorage.snotra_perf === "1"`** のときのみ有効化する
- 有効化手順: DevTools で `localStorage.setItem("snotra_perf","1")` → アプリ再起動
- 無効化手順: `localStorage.removeItem("snotra_perf")` → アプリ再起動
- Tauri Driver E2E でウィンドウ可視性を判定するとき、`document.visibilityState` は誤判定し得るため性能判定の根拠に使わない。`plugin:window|is_visible` を優先する
- E2E 全体実行時間はテスト待機タイムアウトの影響を強く受ける。性能評価では、E2E 所要時間だけでなく `perf.ts` の p50/p95 と trace を併用する
