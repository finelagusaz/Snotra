# パフォーマンス最適化プレイブック

> **この節の具体例は WebView2 期のものである（#532 SU7 でフロント撤去済み）。** `clearTimeout`・
> `invoke<ArrayBuffer>`・`URL.createObjectURL`・`results-sync`・`Promise.allSettled` は現行構成に
> 対応物を持たない。**着手の順序（待ち時間 → 転送量 → 描画）は現行でも生きている**——
> 個々の手段は egui 経路の対応物へ読み替える。

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

> **この節は WebView2 期の記録である（#532 SU7 でフロント撤去済み）。** 現行構成のプロセスツリーは
> **1 件**（webview2 子孫はゼロ）で、数値・内訳とも現行には当てはまらない。現行の実測は
> 「egui 期のメモリ実測」節を参照。`EmptyWorkingSet` の採用判断（下記）だけは現行でも生きている。

ランチャーは 99% 非表示常駐のため、アイドル時の物理メモリ（working set / private RSS）の最小化が重要。installed release ビルド（32GB/SSD 機）で WebView2 プロセスツリー（main + webview2 子孫 計 7 プロセス）を実機計測した。

### 実態（Private Working Set = プロセス固有の物理 RAM）

- 非表示アイドルの定常 baseline: **~110MB**。Working Set 表示値 ~390MB との差 ~280MB は Edge ランタイム共有 DLL（`msedge.dll` ~310MB 等）のページで、プロセス間共有・デマンドページゆえ Private には乗らない（「Snotra=390MB」は誤読）
- 内訳（`VirtualQueryEx` で committed をタイプ別分類）: WebView2 エンジン固有 ~64MB（browser 33 + renderer/V8 18 + utility/crashpad 13、アプリ制御不可）+ Rust 本体 ~30MB + GPU プロセス ~18MB
- Rust 本体 ~30MB の正体: index は極小（`index.bin` 数百 bytes）ゆえデータではなく、Tauri/WRY/tokio/serde/windows クレート + アロケータ保持の baseline（committed 42.8MB の内訳はヒープ 40.7MB / スレッドスタック 2.17MB）

### 採用: hide 時の `EmptyWorkingSet`（issue #355 / PR #360）

hide 経路で Win32 `EmptyWorkingSet` をプロセスツリー全体へ能動適用し、アイドル常駐を **110MB → 数MB** へ回収する。実装は `src-tauri/src/working_set.rs`、パターン詳細は `src-tauri/CLAUDE.md`「working set の能動回収」節。

| 状態 | Private WS |
|---|---|
| アイドル baseline | ~110 MB |
| hotkey hide 後（自動 trim） | ~9.4 MB |
| frontend(Escape) hide 後（自動 trim） | ~22.8 MB |

再表示レイテンシは劣化しない（trim 無/有とも ~41ms、メモリ圧迫下でも 44ms、ウィンドウ出現はブロックされない）。ページは standby/pagefile に退避され show 時に OS が透過 re-fault する（圧迫下のみハード読込 ~934/s ≈ 3.6MB/s で SSD では体感不能）。削減対象は物理 working set であって commit（~195MB 不変）ではない。frontend 経路が hotkey より浅い（22.8 vs 9.4MB）のは `notify_main_hidden` が tokio で `win.hide()` と並行するタイミング差（#361 で修正済み、下記）。

### follow-up: frontend hide の trim タイミング修正（issue #361 / 2026-06-02 計測）

frontend hide（`hideMainWindow()`）で `notifyMainHidden()`（trim）を `await win.hide()` の**後**に呼ぶよう順序を入れ替え、可視中の trim でレンダラがページを再 touch する取りこぼしを解消（hotkey 経路と同じ hide→trim 順）。MainApp.tsx のフォーカス喪失・クリック起動経路も `hideMainWindow()` に集約（DRY）。

クリーン再計測（検索なし、`%TEMP%/snotra-mem-measure.ps1`、**プロセスツリー総 WorkingSet64**= 共有ページ込みで上表の Private WS とは別指標）:

| 経路 | #361 前 | #361 後 |
|---|---|---|
| frontend(Escape) hide 後 | ~50 MB（3 回 50-61） | **~27 MB**（3 回 27-30） |
| hotkey hide 後（参照・無変更） | ~10 MB | ~10 MB |

frontend hide の総ツリー WS が**約半減**（frontend↔hotkey 差の ~57% を解消）。残差（~17MB）は frontend 経路が `suspend_webview`（TrySuspend）を行わない設計差に起因（tokio IPC スレッドの `with_webview` 非同期制約のため hotkey 限定。当時の正本だった src-tauri の「TrySuspend / Resume パターン」節は WebView2 層ごと #532 SU7 で消滅した——現行の回収機構は `src-tauri/CLAUDE.md`「working set の能動回収」）。gap は「trim タイミング」成分（#361 で解消）+「suspend 有無」成分（設計上の意図的な差）の和。**（2026-07-17 訂正: 下の follow-up のとおり、当時の TrySuspend は両経路とも同期失敗しており、この残差の「suspend 有無」解釈は成り立たない。実体は trim 実行タイミングの揺らぎだった）**

### follow-up: TrySuspend は一度も成立していなかった（2026-07-17 計測）

trace 計装（`suspend:call_returned` / `suspend:completed`）で、`TrySuspend` が hotkey 経路を含む**全 hide で同期 Err（0x8007139F ERROR_INVALID_STATE）を返し、導入以来一度も suspend が成立していなかった**ことを確認した。原因は `TrySuspend` の前提条件が `ICoreWebView2Controller.IsVisible=false`（HWND 非表示とは独立の controller プロパティ）であり、wry が `hide()` でこれを下げないこと。同期 Err では完了ハンドラが呼ばれないため、既存の握りつぶし（`let _ =`）では観測不能だった。

対処: `suspend_webview` で `SetIsVisible(false)` を自前実行してから `TrySuspend`、`resume_webview` で `SetIsVisible(true)` + `Resume` の対称復帰。同時に `notify_main_hidden` へ `run_on_main_thread` 経由の suspend を拡張（frontend hide も hotkey と同一の suspend → trim 順・同一実行文脈）。

計測（release ビルド・テスト index 数百件・プロセスツリー総 WorkingSet64・各 10 サイクル + アイドルプローブ）:

| 変種 | hide 直後(中央値) | idle 30s | idle 60s | idle 120s | show p50 / max |
|---|---:|---:|---:|---:|---|
| 修正前 Escape | ~30MB | 85.7 | 86.0 | — | 33.3 / 53.4ms |
| 修正前 hotkey | ~18MB | 69.9 | 70.5 | — | 33.9 / 52.3ms |
| 修正後 Escape | ~27MB | **14.0** | **18.9** | 30.7 | 34.6 / 52.3ms |
| 修正後 hotkey | ~37MB | **11.9** | **17.3** | 29.2 | 35.4 / 56.8ms |

- **本質的な効果は hide 直後ではなく定常アイドル**: suspend が成立しないとレンダラーが動き続け、trim の成果がアイドル 30 秒で ~70-86MB へ巻き戻る。成立後は 12-31MB で低空安定（約 55-70MB 削減）。99% 非表示のランチャーでは定常値が真の指標
- show レイテンシは劣化なし（suspend 成功率は両経路 10/10、クエリ入力 + アイコン取得を挟む 4 サイクルでも機能劣化なし）
- #355 の「hotkey hide 9.4MB vs frontend 22.8MB」の差は suspend ではなく trim タイミング差の産物だった

### 効かなかった/見送った手法

- **TrySuspend / MemoryUsageTargetLevel.Low 単独**: 当時「論理目標を下げるだけで物理 working set を回収しない（120 秒放置で 110MB 不変）」と結論したが、**2026-07-17 訂正: この時点の TrySuspend は `SetIsVisible(false)` を欠き同期失敗していた**——「効いているが物理を返さない」のではなく「そもそも動いていなかった」。成立後はアイドル再増殖の防止に有効（上の follow-up）。即時回収に `EmptyWorkingSet` が必要という結論自体は不変（両者は別レイヤーで補完的）
- **tokio / rayon の worker スレッド削減**: Rust 本体は Tauri 既定の multi-thread tokio（worker = CPU コア数）+ rayon プールで ~50 スレッドを抱えるが、スレッドスタックは計 2.17MB（committed 42.8MB の 5%）に過ぎず、worker を絞っても RSS 削減は <1MB で**無意味**。30MB の大半はヒープ/フレームワーク baseline
- **`--disable-gpu --disable-gpu-compositing`**: GPU プロセスは消えない（in-process ソフト合成に切替）が Private 18→6MB・合計 110→99MB。ただしブラウザフラグは Microsoft 非サポート（ランタイム更新で挙動変化）＋ CPU 合成化で描画レイテンシ未検証。限界効用は `EmptyWorkingSet`（~107MB）に対し桁違いに小さく見送り
- **アイコン PNG 圧縮強化（issue #335）**: 16×16 アイコンで実測 ~0.06MB と桁違いに小さく却下（詳細は #335）

## egui 期のメモリ実測（2026-07-25 計測）

> **この節の実運用点は 38,847 エントリである。2026-08-07 に同じ機で測り直したところ
> 312,377 エントリ（`index.bin` 107.0 MiB）に育っており、索引の常駐は 14.96 → 166.08 MiB
> になっていた。** 下の font A/B と「索引の常駐」の絶対値は現運用点に当てはまらない
> ——機構（`from_static` / cmap カバー判定 / Context 数への比例）だけが生きている。
> 現在の値と内訳は「索引の常駐の内訳（2026-08-07 計測・312,377 エントリ）」を見ること。

`#532 SU7` 後の構成（単一プロセス・WebView2 なし）を release ビルドで実機計測した。
計測ハーネスは `snotra-core/tests/memory_footprint.rs`（アロケータ計数・実行コマンドは
`docs/build-commands.md`）と `scripts/measure-memory.ps1`。

### 軸の取り違えに注意 — PrivWS は trim 後の値である

hide 経路の `EmptyWorkingSet`（`src-tauri/src/working_set.rs`）は物理ページをほぼ完全に返す。
**非表示アイドルの PrivWS は 2.3 MiB** で、ここに削る余地は無い。一方 PrivComm は trim で
**動かない**（97.1 → 97.1）。ゆえに**ヒープ調律の軸は PrivComm のみ**であり、PrivWS の
小ささを「メモリを使っていない」と読んではならない。

### font_family A/B（前景計測・`auto_hide_on_focus_lost = false`）

実運用 config（38,847 エントリ・migemo 有効）での PrivComm。B は `font_family` を
解決不能名にして user_font 経路だけを落とした変種。

| 段階 | A: `font_family = "HackGen Console"` | B: 解決不能（jp_font 単一） | 差 |
|---|---:|---:|---:|
| アイドル（一度も表示せず） | 61.9 MiB | 41.3 MiB | **20.6** |
| 表示（空クエリ） | 75.1 MiB | 43.8 MiB | **31.3** |
| 検索実行後 | 97.1 MiB | 56.2 MiB | **40.9** |

- **user_font 経路が最大の単一項目**。窓を一度も出していないアイドル時点で既に 20.6 MiB 効いており、
  フォント解決が表示より前に走ることを示す
- 差はファイルサイズ（`HackGenConsole-Regular.ttf` = 10.21 MiB）の約 2 倍。`resolve_font_family` の
  `data.to_vec()` による複製に加え、egui 側の解析・グリフ実体化が上乗せされる。**内訳は未分離**
- **差が段階とともに広がる（20.6 → 31.3 → 40.9）のが要点**。一度きりのバイト複製なら差は一定のはずで、
  広がるということはグリフ実体化の分が乗っているということ。同じクエリ 1 回の検索で A は +22.0 MiB、
  B は +12.4 MiB しか増えない——検索時の増分もアイコンではなく**主にフォント由来**である。
  ゆえに「user_font が CJK をカバーするなら冗長な jp_font を積まない」が具体的な削減候補になる
  （カバー判定の述語が要る）
- `JP_FONT_BYTES: OnceLock<Box<[u8]>>` は `YuGothM.ttc`（**13.26 MiB**）を丸ごと保持し**解放経路が無い**。
  user_font が CJK をカバーする場合（HackGen 等）でも fallback として常駐し続ける
- 実行間のばらつきは ~4 MiB（別実行のアイドルは 65.9 MiB）。背景再スキャンの進行差による。
  **1 MiB 単位の差を有意と読まない**

### 採用: user_font が CJK をカバーするときの jp_font 省略

user_font 自身が CJK をカバーするなら jp_font（`YuGothM.ttc` 13.26 MiB）は一度も glyph を
出さないまま常駐する。**user を先に解決 → cmap 実測でカバー判定 → 必要なときだけ jp を読む**
という順序へ入れ替えた（実装は `egui_shell/font_stack.rs` の `font_covers_cjk` /
`configure_japanese_font`）。判定はかな + JIS 第1水準 + **第2水準・互換漢字**を引き、
かなと常用漢字だけの中途半端な和文フォントを弾く。パース不能は「カバーしていない」＝ jp を積む安全側。

| 段階 | 変更前 | 変更後 | 差 |
|---|---:|---:|---:|
| アイドル | 61.9 MiB | 48.6-49.6 MiB | **-12.9** |
| 表示 | 75.2 MiB | 61.6-62.6 MiB | **-13.3** |
| 検索実行後 | 97.3 MiB | 84.6-84.8 MiB | **-12.6** |

- 削減量が**ファイルサイズとほぼ一致する**のは機構と整合する。user_font が全 glyph を
  出していたため jp_font はラスタライズされておらず、効いていたのは生バイト列と
  eager parse の分だけだった。**glyph 機構まで削れると見積もったのは誤りで、実測が正した**
- `JP_FONT_BYTES` の `OnceLock` は set-once・never-clear（`transmute` による `'static` 化の
  健全性の根拠）。CJK をカバーしないフォントで一度読むと、カバーするフォントへ切り替えても解放されない
  ——**削減は fresh start の値である**
- 残余リスク: プローブは標本であり、カバーしていると判定した後にプローブ外の文字が user_font に
  無ければ豆腐（□）になる。クラッシュしない字単位の静かな欠落ゆえ、判定は厳しい側へ倒してある

### 採用: user_font も `from_static` で積む（epaint の全体複製を止める）

`from_owned` で積んだフォントが epaint 側で丸ごと複製される。**機構の正本は
`egui_shell/font_stack.rs` の `font_definitions` の doc コメント**（epaint のどの関数がどう
複製するかはそこに書く。ここで再記述すると epaint の更新時に二重メンテになる）。
jp_font は元から `from_static` だったが user_font だけ `from_owned` のままで、
上の A/B はこの非対称をそのまま映していた（user 20.6 ≒ **2×** 10.21 / jp 12.9 ≒ **1×** 13.26）。

| 段階 | 変更前 | 変更後 | 差 |
|---|---:|---:|---:|
| アイドル | 48.8 MiB | **38.2 MiB** | -10.6 |
| 表示 | 61.0 MiB | **40.1 MiB** | -20.9 |
| 検索実行後 | 84.3 MiB | **54.3 MiB** | -30.0 |

- **差が 10.2 MiB 刻みで 3 段に増えるのが機構の証明**（ファイルサイズちょうど 3 つ分）。
  変更前の複製は 4 本: `FontData` が 2 Context 分（アイドルで顕在）＋ 各 Context が
  `FontsImpl` を組む時の Blob 深いコピー 2 本（main は表示時・results は検索時に顕在）。
  変更後は leak した 1 本を全員が借りる
- **窓が 2 つある構成では、フォント常駐は Context 数に比例する**。egui の Context は
  窓ごとに独立で `FontsImpl` も別々に持つ——「1 フォント = 1 常駐」ではない
- leak は family 名をキーにしたキャッシュで共有する（`Box::leak` を素で置くと 1 回の
  font_family 変更で 2 回漏れる）。漏れは distinct family 数で頭打ち・解放はされない

### 索引の常駐（アロケータ実測・38,847 エントリ）

| 指標 | 値 |
|---|---:|
| 定常 live | **14.96 MiB**（404 B/entry） |
| ロード時ピーク | 19.97 MiB（定常の 1.33 倍） |
| live ブロック数 | 233,430（6.0 blocks/entry） |

- `index.bin` は **6.80 MiB / 38,847 件**。「index は極小（数百 bytes）」という旧記述は
  WebView2 期の別環境の値で、実運用点とは 4 桁ずれる
- **`normalized_keys` が 3.05 MiB**（索引の 22%）。`normalize_entry_key` = 小文字化 + `/`→`\` の
  長さ保存変換ゆえ、on-disk バイト数は `target_path` と完全一致する——原文パスと二重に持っている
- migemo の限界費用は 10k/38.8k/100k で一貫して **+28.6%**（133 → 171 B/entry）
- アロケータ実測は `layout.size()` の集計であり、Windows ヒープのブロックヘッダ・
  サイズクラス丸めを**含まない**。233,430 ブロックゆえ実 RSS はこれより大きい

## 索引の常駐の内訳（2026-08-07 計測・312,377 エントリ）

実 config の `[[paths.scan]] path = 'C:\'` + `include_folders = true`（C ドライブ全体の
フォルダ索引）による現運用点。`index.bin` = **107.0 MiB**。ハーネスは
`snotra-core/tests/memory_footprint.rs`。**`--test-threads=1` が必須**——計数器が
`static AtomicUsize` のプロセス大域であり、並列実行では Phase A/B が奪い合って
エラーにならず**もっともらしい数値**を出す（実測: 単調性の破れと `live 0.00 MiB`）。

| 区間 | live | peak | blocks/entry |
|---|---:|---:|---:|
| `index.bin` ロード（+ 再スキャン複製） | 228.57 MiB | 273.08 MiB | 7.00 |
| **`SearchEngine` 常駐** | **166.08 MiB** | 166.08 MiB | **5.00** |

内訳は **100% 帰属済み（未帰属 0.00 MiB）**。557.4 B/entry の行き先:

| 項目 | 確保 | B/entry | 性質 |
|---|---:|---:|---|
| `entries[].target_path`（文字列） | 35.56 MiB | 119.3 | 原本 |
| `normalized_keys`（文字列） | 35.56 MiB | 119.3 | **`target_path` から 100% 再現可能** |
| `entries` Vec 本体 | 32.00 MiB | 107.4 | 実使用は 16.68 MiB |
| `lower_file_names`（文字列） | 10.47 MiB | 35.1 | **folder は 100% が `lower_names` と同一** |
| `entries[].name`（文字列） | 10.25 MiB | 34.4 | `target_path` の末尾成分 |
| `lower_names`（文字列） | 10.25 MiB | 34.4 | |
| `lower_names` / `lower_file_names` / `normalized_keys` の Vec 本体 | 各 8.00 MiB | 各 26.9 | |
| `char_masks` / `file_name_char_masks` | 各 4.00 MiB | 各 13.4 | 実使用は各 2.38 MiB |

### 重複の実測（機構からの導出ではなく実データの率）

- `is_folder` = 255,961 / 312,377（**81.9%**）
- `lower_file_names[i] == lower_names[i]`: folder **255,961/255,961（100.0%）** / file **0/56,416**。
  indexer が folder の `name` に `file_name()`、file に `file_stem()`（拡張子なし）を使う
  規則どおりで、例外は 1 件も無い
- `normalized_keys[i] == normalize_entry_key(target_path)`: **312,377/312,377（100.0%）**。
  `normalize_entry_key` は Unicode 小文字化ゆえ長さ保存ではないが、**この索引には
  例外が 1 件も無い**（他の索引で成り立つ保証ではない）

### 採用: `assemble` で全並列 Vec を `shrink_to_fit`（-28.25 MiB）

上表は反復前の値である。`assemble`（3 コンストラクタの唯一の合流点）で 8 本の Vec を
`shrink_to_fit` した結果:

| 指標 | 変更前 | 変更後 |
|---|---:|---:|
| `SearchEngine` 常駐 | 166.08 MiB | **137.83 MiB**（**-28.25**・-17.0%） |
| ロード時ピーク | 273.08 MiB | 273.08 MiB（不変・縮小は確保後） |
| live ブロック数 | 1,561,891 | 1,561,891（不変） |
| 構築の壁時計（実 `index.bin`） | 1 / 2 / 3 ms | 2 / 3 / 3 ms |

- **3 回の実行でバイト数・ブロック数とも完全に一致した**。アロケータ計数は決定的で、
  PrivComm の ~4 MiB のばらつきとは別の計器である
- **ブロック数は動かない**（`shrink_to_fit` はブロックの数ではなくサイズを変える・`allocs` は
  6 増える）。**ブロック数だけを見ていたら「何も起きていない」と読める**
- 対のレイテンシ実測（同日・同セッション A/B。設計書 §4.2・前例は #110）:
  `bench_new_scaling` 300k が 544 → 532 ms、`bench_fuzzy_search_scaling` 300k が
  3,829 → 3,469 µs。**どの規模でも向きが一貫せず、退行なし**（メモリレイアウトは不変で、
  変わるのは末尾の未使用容量だけゆえ機構とも整合する）
- 検知器は `snotra-core/src/search/tests/build.rs`。**余剰容量は検索結果を変えないため
  挙動テストでは捕まらない**——`shrink_to_fit` を 8 箇所とも外すと落ちることを実測した

### Vec 本体の確保が実使用の約 2 倍あった（反復 1 で解消）

Vec 本体の合計は **64.00 MiB**（確保）。`char_masks` は 524,288 = 2^19 要素分を確保して
312,377 しか使っていない。serde の `size_hint` は DoS 防止のため 4,096 要素で頭打ちにし、
以降は Vec の倍々成長に委ねるため、`index.bin` から読んだ全 Vec が成長の踊り場を抱える。

**この余剰は `SearchEngine` へそのまま持ち越される**（実測: 構築前に走査した内訳の合計が
構築後の常駐と一致し、未帰属 0.00 MiB）。機序は `new_with_cached_masks` の
`Vec<String>` → `Vec<Box<str>>` 変換が in-place collect（16 B ≤ 24 B・align 一致）で
確保ブロックを再利用し、要素サイズが縮んでも `layout.size()` が動かないためと**推定される**
——`allocs = 0` からの導出であって、in-place collect は std の特殊化ゆえ保証ではない。
**持ち越しの事実はバイト合計の一致が支えており、この機序の推定には依存しない。**

### パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか

計器は `snotra-core/tests/path_query_cost.rs`。パスクエリは Fuzzy ビットマスク pre-filter を
スキップする＝全走査であり、索引規模がそのまま毎打鍵に乗る唯一の経路である。

実 `index.bin`（312,377 件）・release・16 コア・**rayon 並列（製品と同じ形）**・各 3 回の最小値:

| needle | 保持（現行） | 導出: 素 | 導出: ASCII 高速路 | hits |
|---|---:|---:|---:|---:|
| `\workspace\` | 3.1 ms | 69.6 ms | **6.9 ms** | 8,370 |
| `\users\` | 2.2 ms | 56.5 ms | **4.0 ms** | 97,538 |
| `\node_modules\` | 2.2 ms | 54.0 ms | **4.7 ms** | 6,322 |
| `\zzz-no-such-path\` | 2.4 ms | 52.5 ms | **4.0 ms** | 0 |

- **計器の形を製品に合わせないと採否が反転する。** 同じ変更を単スレッドで測ると
  保持 28-43 ms / 導出:ASCII 50-75 ms（+20〜35 ms）で「見送り」に見えるが、
  製品の走査は rayon 並列であり、実際の追加コストは **+1.6〜3.8 ms**（60 Hz の 1 フレーム
  16.7 ms に対し十分小さい）
- **支配項は `char::to_lowercase()`（Unicode・テーブル参照つきイテレータ）である。**
  ASCII 高速路（`is_ascii()` なら bytewise `to_ascii_lowercase`）で 9.2-11.7× → 1.7-2.3× になる。
  **ASCII 範囲では Unicode 小文字化と ASCII 小文字化の結果が一致する**ためバイト一致は保たれ、
  実インデックスの全 312,377 パスで `normalize_entry_key` との一致を固定してある
- 実運用点で非 ASCII を含むパスは **5,432 件（1.7%）** だけで、そこだけ低速路へ落ちる
- **Unicode 小文字化は並列化の利きが悪い**（単スレッド 148 ms → 16 コア 55 ms で 2.7× 止まり）。
  ASCII 路は 75 → 4 ms（19×）と素直に効く。**倍率だけを見て絶対値を推定してはならない**

### 採用: `normalized_keys` の廃止（-40.32 MiB・IndexCache v5）

索引に持っていた `normalized_keys` を落とし、必要な候補についてだけ `target_path` から
導出する形へ移した（`search/scoring.rs` の `with_normalized_key`）。

| 指標 | 変更前 | 変更後 |
|---|---:|---:|
| `SearchEngine` 常駐 | 137.83 MiB | **97.51 MiB**（**-40.32**・-29.3%） |
| live ブロック数 | 5.00 / entry | **4.00 / entry**（1,561,891 → 1,249,513） |
| `recent_history`（窓を開くたび） | 65.4 ms | **9.8 ms**（-85%） |
| パスクエリ全走査 | 2.1-3.1 ms | 4.5-7.4 ms |

対のレイテンシ実測（同日・同セッション・**各 3 回**）:

| bench（300k・中央値） | 変更前 | 変更後 |
|---|---:|---:|
| `bench_fuzzy_search_scaling` | 3,652 µs | 3,993 µs（**+9.3%**） |
| `bench_new_scaling` | 530 ms | 529 ms（ノイズ内） |

- **1 標本ずつの比較は +29% を示した。3 回ずつ取り直すと +9.3% だった**——A 側の下振れと
  B 側の上振れを突き合わせていた。**退行の疑いこそ標本を増やす**（1 回の比較で
  「#110 の領域」と判断しかけた）
- fuzzy の +9.3% は 1 打鍵あたり +0.34 ms。マッチ**成立後**の履歴照合が導出を要求するため、
  マッチ件数に比例して乗る
- `recent_history` の改善は `normalized_keys` とは独立の欠陥の是正である。高々 8 件を探すのに
  312,377 件ぶんの照合表を毎回組んでいた——**探す側で表を組む**形に反転させ、走査 1 パスにした
- v4 キャッシュを持つ既存ユーザーは、`normalized_keys` を読んで捨てる経路を通る。復元する
  4 本は v5 と同じでどれも v4 に揃っているため **Wave 1 はスキップされたまま**であり、
  **常駐の削減は v5 への書き換えを待たずに効く**（ディスクとロード時ピークだけが据え置き）

### 採用: `target_path` のフォルダ木接頭辞共有（-42.59 MiB・実装後の実測）

| 指標 | 変更前 | 変更後 |
|---|---:|---:|
| `SearchEngine` 常駐 | 97.51 MiB | **54.92 MiB**（**-42.59**・-43.7%） |
| live ブロック数 | 1,249,513（4.00/entry） | **937,402（3.00/entry）** |
| `SearchEngine` 構築 | 2〜3 ms | **55〜96 ms** |
| ロード時ピーク | 122.52 MiB | 138.40 MiB（構築中の一時 Vec ぶん） |

- **投影 55.02 MiB / 3.00 blocks に対し実測 54.92 MiB / 3.00 blocks**。バイトとブロックの
  両方が一致した——ブロック数は、うっかり per-entry の `Box<str>` が 1 本混ざるとバイトでは
  気づけずに崩れる側の指標である
- **3 回の実行でバイト数・ブロック数とも完全に一致した**（アロケータ計数は決定的）

#### 構築コストの見積もりが 4〜5 倍外れた（計器が払っていなかったもの）

実装前の計器は `build()` を 19.6 ms と測っていたが、実装後の `SearchEngine` 構築は
**55〜96 ms**（3 標本で 96 / 86 / 87、別セッションで 57）である。内訳を計時して測ると
**導出 + intern が約 30 ms、組み立てが約 25 ms**。

差の実体は**計器が `entries` を生かしたままだったこと**である。製品は組み替えの最後に
`Vec<AppEntry>` を消費し、**312,377 個の `String`（`target_path`）を解放する**。
`blocks` が -312,111 動くことがその実体で、`allocs` は +5,723 しか増えない
（`into_boxed_str` は確保し直さない）。**確保を測っていて解放を測っていなかった。**

計器が対象でないものを測っていた件は、この反復で通算 5 件目である（前 4 件は
「`target_path` のフォルダ木接頭辞共有」節の末尾）。**5 件のうち 4 件は「製品の形に
合わせる」で解け、この 1 件だけは「製品と同じ寿命で測る」でしか解けなかった**——
確保と解放は別の軸であり、片方だけを写した計器は嘘をつく。

#### 対のレイテンシ実測（同日・同セッション A/B・各 3 回・300k）

| bench | main | 本改修 |
|---|---|---|
| `bench_fuzzy_search_scaling` | 4082 / 3844 / 3706 µs（最小 3706） | 3500 / 3804 / 4154 µs（最小 3500） |
| `bench_new_scaling` | 510 / 502 / 515 ms（最小 502） | 618 / 593 / 586 ms（最小 **586**） |

- **検索はノイズの範囲で不変**（レンジが重なる）。`entries` が 56 → 32 B/要素になったぶん
  走査が触るキャッシュラインは減るが、それが測れるほどの差にはならない
- **1 標本目だけを見ると検索が 14〜20% 速く見えた**。3 回ずつ取るとレンジが重なる。
  #962 で「1 標本ずつの比較は +29%、3 回ずつだと +9.3%」を踏んだのと同じ形であり、
  **改善の疑いも退行の疑いと同じだけ標本を要する**
- 構築の +84 ms（502 → 586 ms）は実インデックスで測った構築コストと整合する

**`bench_*_scaling` は合成索引であり、全件走査 2 経路を 1 つも含まない**（パス区切りを
含むクエリが無く、`recent_history` も呼ばない）。そのうち `recent_history` だけは製品経路を
実 `index.bin` で測れる（`tests/path_query_cost.rs` の `measure_recent_history_cost` は
実 engine を呼ぶ）:

| `recent_history`（実 index.bin・製品経路・各 3 回） | 最小 |
|---|---:|
| main | 9.9 / 9.9 / 12.3 ms → **9.9** |
| 本改修 | 17.3 / 18.3 / 17.6 ms → **17.3**（**+7.4**） |

**計器の見積もり（+4.2 ms）より製品のほうが悪い。** 計器は `parent` を独立した `Vec<i32>` に
置いており、製品は `CompactEntry` の中に置く——「同じキャッシュラインに載るので製品のほうが
有利」と見込んだが、実測は逆だった。

#### パスクエリのフレームコスト — **フレーム予算を超える**（`measure_path_query_frame_cost`）

実 `index.bin` × `Engine::search`（config 実効 limit・履歴ブースト・top-k 組立込み）。
`has_path_sep` は incremental cache を無条件で無効化するため、**パスを打っている間は
毎打鍵が全件走査**になる。20 回の min/p50/max（µs）:

| query | main p50 | 本改修 p50 | Δ |
|---|---:|---:|---:|
| `users`（区切り無し） | 844 | 983 | +139 µs（+16%） |
| `c:\` | 21,445 | 37,877 | **+16.4 ms（+77%）** |
| `c:\users` | 15,384 | 27,116 | **+11.7 ms（+76%）** |
| `c:\users\` | 15,057 | 28,334 | **+13.3 ms（+88%）** |
| `\program files\` | 13,115 | 20,855 | **+7.7 ms（+59%）** |
| `\zzz-no-such-path\` | 13,278 | 21,332 | **+8.1 ms（+61%）** |

- **main の時点で既にフレーム予算（60fps = 16,700 µs）に届いている**（p50 13〜21 ms）。
  本改修はそれを **1.6〜1.9 倍**にし、1 打鍵あたり 1.3〜2.3 フレームになる。
  **体感できる領域である**
- **計器の見積もり（+3 ms）を製品が 3〜5 倍上回った。** 走査だけを切り出した写しでは
  なく `Engine::search` を通すと、スコアリング・履歴照合・top-k 組立が同じ再構築の上に乗る
- 区切りを含まないクエリの +139 µs は `heap_into_results` が `target_path.clone()` から
  `to_path()`（木を辿って組み立て）へ変わったぶんで、`result_limit`（既定 200）件に比例する。
  1 フレームの 1% であり体感には乗らないが、**`result_limit` を上げると比例して増える**

**計器の見積もりが製品より甘かったのはこれで 3 経路すべてである**（構築 4〜5 倍・
`recent_history` 1.8 倍・パスクエリ 3〜5 倍）。走査だけを切り出した写しは、その走査に
乗っている他の処理を数えない。

#### 残余

- `drop` 後の残留が 0.00 → **0.11 MiB**。3 回とも同値で、索引規模に対して固定量である。
  v5 キャッシュヒット経路は Wave 1/2 をスキップするため従来 rayon を使っておらず、
  `PathStore::build` が**グローバルスレッドプールを初めて起動する**ためと**推定される**
  （プールの per-worker 状態は解放されない）。**推定であって実測ではない。**

### `BackgroundRescanTask` の全エントリ複製

ロード区間 228.57 MiB と常駐 166.08 MiB の差 **62.49 MiB**（210 B/entry）は
`indexer.rs` の `BackgroundRescanTask.cached_entries`。キャッシュヒットする毎回の起動で
背景再スキャンの実行中ずっと常駐する。

### `target_path` のフォルダ木接頭辞共有 — 構造前提と 3 つのコスト（反復 3・実装前）

`entries[].target_path`（35.56 MiB・常駐の 36.5%）を「親 index + 末尾成分」で持つ案。
計器は `tests/memory_footprint.rs`（構造前提）と `tests/path_query_cost.rs`（再構築コスト）。

**構造の前提は実データで成立する**（312,377 エントリ）。機構からは導けないので測った
——scan の順序・`show_hidden_system` フィルタ・ドライブ直下・UNC・区切りの揺れが
それぞれ例外を作りうる:

| 前提 | 実測 |
|---|---:|
| 親が索引に居る | 312,281 / 312,377（99.97%・孤児 96） |
| 親 + 区切り + 末尾 == `target_path`（バイト一致） | 不一致 **0** |
| 末尾成分 == `name`（folder） | 255,948 / 255,948（**100.0%**） |
| `name` が末尾成分の接頭辞（file） | 56,333 / 56,333（**100.0%**） |
| `parent_index < self_index` | 違反 **0**（循環を表現不能にできる） |
| 深さ | 平均 6.05 段 / 最大 17 段 |

`target_path` のバイトは folder が **87.4%**（件数比 81.9% とずれる——削減量の係数に
なるのは件数ではなくバイトのほうである）。

**2 案の比較はバイトだけでは決まらない。** 1 エントリあたりの小さな `Box<str>` の数が
アロケータ由来のオーバーヘッドを決め、`layout.size()` しか数えない計器には映らない:

| | 現在 | 末尾成分案 | 拡張子 intern 案 |
|---|---:|---:|---:|
| `target_path` 文字列 | 35.56 MiB | 0.76 | **0.22** |
| `entries` Vec 本体 | 16.68 MiB（56 B/要素） | 14.30（48 B） | **9.53（32 B）** |
| 確保ブロック | 312,377 | 56,429 | **98** |
| 常駐合計の投影 | 97.51 MiB | 60.33 | **55.02（-43.6%）** |

拡張子 id を `AppEntry` に埋める案が per-entry の確保を消す。ただし **distinct 拡張子 2 は
この config の産物**（scan が拡張子を絞っている）であり、intern 表を固定長で持つ根拠には
使えない。

払うコスト（各 3 回の最小値・同一セッション・rayon 並列＝製品と同じ形）:

| 経路 | 頻度 | 現在 | 木表現 |
|---|---|---:|---:|
| **木の構築**（`build()` 実測） | 起動・索引再構築のたび | 2〜3 ms | 19.6〜21.6 ms（**実装後は 55〜96 ms**・下の「採用」節） |
| `recent_history` の形 | 窓を開くたび | 11.3 ms | 15.5 ms（+4.2） |
| パスクエリ全走査 | `\` を含む打鍵ごと | 3.9〜5.0 ms | 6.9〜8.4 ms（+3） |

**構築の内訳は親解決の手段で 7 倍動く**（4 手段が同じ木を作ることをアサートした上での比較）:
HashMap 152.2 / 二分探索 112.4 / 二分探索 + rayon 並列 30.0 / **同上 + 直前の親の使い回し
23.0 ms**。使い回しが効くのはソート順で兄弟がほぼ連続するためで、段数も実測で決めた
（2 段 25.3 / 4 段 23.8 / 8 段 25.6 ms）。**スタックによる 1 パスは採れない**——区切り
`\`(0x5C) は `-`(0x2D) より大きく、`C:\a` / `C:\a-x` / `C:\a\b` の順に並ぶため子孫が
ソート順で連続しない。

**整列済みかの事前走査は要らない**（6.5 ms・全体の 4 分の 1 を占めていた）。`binary_search_by`
が `Ok` を返すのは `entries[pi].target_path == par` のときだけであり、未整列の配列でも
**別の親を返すことは起こりえない**——起こるのは取りこぼしだけで、取りこぼしたエントリは
側テーブル行き（フルパス保持）になり結果は正しいままである。残る唯一の危険（循環）は
`pi < i` の 1 比較で構造的に潰せる。**O(n) の走査が 1 比較のガードに置き換わった。**

**再構築は 2 種類要る。** `with_normalized_key` が作る小文字キー（履歴照合・パスマッチ）に
対して、tie-break（`ScoredEntry::cmp`）と `SearchResult.path` の clone は原文のバイトを
要求する。どちらも 312,377 件でバイト一致を固定した。

**`IndexCache` v6 では構築コストは消えない。** `entries_equal`・`try_background_rescan`・
アイコンキャッシュの剪定キーがロード結果のフルパスを要求するため、`index.bin` から
`target_path` を落としても結局どこかで再構築が要る。本当に消すには再スキャンの比較と
アイコンキーまで木へ移す必要があり、それは別の反復である。

#### 計器が対象でないものを測っていた 3 件（すべて「製品の形に合わせる」で解けた）

判定は測り方で 2 度ひっくり返った。前回の反復 2（3 回ひっくり返った）と同じ機序である:

1. **セグメントの追記を 1 文字ずつの `push` で書いていた**（走査 2.7〜3.4x → 1.48〜1.79x）。
   `normalize_entry_key_into` 自身が「`String: Extend<char>` が毎回 UTF-8 符号化の分岐を
   通り実測で 2.5-3 倍遅くなる」と明記している落とし穴を、計器が踏んでいた
2. **1 エントリごとに `String` を確保して連結していた**（構築 249.6 → 167 ms）
3. **`std::path::Path` を通していた**（構築 167 → 117 ms）。Windows では `OsStr`（WTF-8）
   → `str` の検証走査が全件に乗る
4. **製品が呼ぶ `build()` ではなく `build_with()` を測っていた**。両者の差である
   `choose_lookup`（6.5 ms）が計測区間の外に落ち、構築コストを 4 分の 3 に見せていた。
   **測り直したことが `pi < i` ガードへの置き換えを生んだ**——外にある額を計器へ入れると、
   その額を消す道が見える

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
  `entry_view(i)` が並列 Vec の参照（`entry` / `lower_name` / `lower_file_name`。当時は `normalized_key` も含んでいたが、2026-08-07 に索引ごと廃して導出へ移した）を束ねて返すことで、スコアリングループの可読性を向上させた。`char_masks` / `file_name_char_masks` はプリフィルタのキャッシュ効率を保つため EntryView に含めず SearchEngine から直接アクセスする。
  `#[inline]` により性能への影響はゼロ（メモリレイアウト不変）。`new()` 末尾の `debug_assert!` で全 Vec 長の同期を検証する。

## 計測と受け入れ基準

- 変更ごとに「入力 → 検索結果反映」までの遅延を観測し、体感を先に確認する
- 体感改善後、必要なら p50/p95 を追加計測して次のボトルネックを特定する
- 原則として「待ち時間」「重複」「計算量」「描画」の順を崩さない
- ランタイムの計測は `SNOTRA_TRACE=1` の構造化トレース（`src-tauri/src/trace.rs`）で行う
- egui/softbuffer の計器は 5 つの env（いずれも未設定なら計器のコストは 0）。**このリストが計器の正本である**——`docs/build-commands.md` には置かない
  - **受理値: 空でなければ何でもよい**（`=1` でも `=0` でも点く）。**空文字は「未設定」として扱う**——判定は `snotra-egui-runtime/src/env.rs` の 1 箇所に集約してある（#872: PowerShell の env 復元が空文字を作り、測定ハーネスの全反復が黙って計器つきで走っていた）。**`SNOTRA_TRACE` だけは別の意味論である**（`1｜true｜yes｜on` のみ・`src-tauri/src/trace.rs` の `env_flag`）
  - `SNOTRA_EGUI_PAINT_TRACE`: paint フェーズ（`tess_ms` / `raster_ms` / `total_ms` / `meshes` / `px`）。#532 SU6.5 の flip ゲート G3(b) の主判定に使った
  - `SNOTRA_EGUI_REPAINT_TRACE`: フレームの到着（`window` / `focused` / `since_prev_ms` / egui の repaint 原因 `file:line`）。**「なぜ再描画が止まらないか」を推測せず原因に名乗らせる**ための計器（#628）
  - `SNOTRA_EGUI_WAKE_TRACE`: `RequestRedraw` の送信（repaint worker・`SEND`）と受信（イベントループの `RedrawRequested` arm・`RECV`・引き当て結果付き）。hidden 中にどの層が配送を抑止しているかの切り分けに使う（#697）
  - `SNOTRA_EGUI_INPUT_TRACE`: 打鍵の到達（注入 → tao の配送 → egui への push → フレーム）。**この計器は系を乱す**——runner では stderr 1 行が 17〜56ms かかり、最初のフレームの到来を押し下げる。**率を測る回と機序を測る回は別の回にすること**（#872/#936）
  - `SNOTRA_EGUI_IME_TRACE`: IMM32 の preedit 取得と候補窓位置（`windows_ime.rs`）
- **操作中の上限（2026-07-26・release・#737 実測）**: `RequestRedraw` の配送は窓が載るモニターのリフレッシュレートで頭打ち（取得失敗時 60Hz・contract-design spec 契約②）。144Hz 機の A/B でポインタ移動中の results 間隔 p50 が 3.5ms → **7.1ms（≈141fps）**、>200fps 相当の間隔（<5ms）が 4,829 → 170（−96%・残余は event queue のジッタとみられる——`SNOTRA_EGUI_WAKE_TRACE` の送信間隔での裏取りは未実施）、paint 占有 17.6% → 8.9%。**ポインタ移動中の p50 がリフレッシュレートの逆数を大きく下回ったら回帰を疑う**
- **可視アイドルの基準値（2026-07-26・release・#628 実測）**: main 2.0 fps / results 2.0 fps（＝ egui のキャレット点滅の下限）・paint 平均 0.96ms・**CPU 0.59%（1 コア比）**。修正前は 11.9 / 19.8 fps・5.1% だった。差は `RawInput::predicted_dt` の既定値（1/60 秒）が短い repaint 予約を「即時再描画」へ飽和させ、点滅の遷移ごとに約 26ms スピンしていたことによる（`snotra-egui-runtime/src/input.rs` の `take` を参照）。**アイドルで 2 fps を超えていたら回帰を疑う**
- コアロジックのマイクロベンチは `ignored` テストとして保持する。実行コマンドは [docs/build-commands.md](docs/build-commands.md) の「検索パフォーマンス計測」を SSOT として参照
- フォルダ列挙のホットパス確認には bench フィルタを絞る: `cargo test -p snotra-core bench_folder_ -- --ignored --nocapture`（SSOT の bench コマンドに対するフィルタ違いとして個別記載）
- `bench_folder_narrow_filter` は「大量エントリ + 狭いフィルタ」で、非一致エントリに不要な文字列化や属性判定をしていないかを確認する
- `bench_folder_hidden_filter_all` は `show_hidden_system = false` 相当で、`metadata()` を伴う属性判定コスト込みの回帰を確認する
- **warm frame は日をまたいで比較しない**——同一ホスト・同一バイナリでも日によって 3 倍変わる。構成 A / B の比較は必ず**同日・同条件で両方を測る**（#532 Phase 1 の検証バイナリで実測: 2026-07-17 に 26-30ms、7/14 は 8-10ms。変動の原因は未解明ゆえ、日をまたいだ数値の差を改善・退行と読んではならない）

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
