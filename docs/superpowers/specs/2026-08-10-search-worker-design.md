# 検索をフレームの外へ出す — 単一 worker と seq 照合

**欠陥は走査が遅いことではなく、20 ms をフレームの中で払っていることである。** `PERFORMANCE.md`「パスクエリのフレームコスト」の実測（`c:\` で p50 21,445 µs・60 fps の予算は 16,700 µs）は、走査を 2 倍速くしても 7〜10 ms がフレームに残ることを意味する。索引が育てば戻ってくる（実運用点は 38,847 → 312,377 件へ育った実績がある）。

本書は #1004 の設計である。走査そのものの費用は #1003 が扱い、**本書とは独立である**——lock とスレッドの問題は走査が速くなっても残る。

## 1. 前提の裏取り（2026-08-10 にコードで確かめた事実）

- `run_search_with` の `QueryIntent::Plain` 枝が `engine.lock()` → `engine.search()` をフレーム内で同期実行している（`launcher_controller.rs`）。issue の記述どおり
- **ただし「毎打鍵が全件走査」は厳密には誇張である。** `search_debounce` は `Debouncer::new(50ms, leading=true)` で、走るのはバースト先頭と静止 50 ms 後の 2 回である。人の打鍵間隔は 50 ms を超えるので実質は毎打鍵に近く、結論は変わらない。**この事実を裏取りせずに「debounce が効いていない」と読むと、次節の判断を誤る**（debounce を撤去する方向へ倒れる）
- 既存の worker は 2 例とも**都度 spawn** である。folder は `spawn_folder_load`（token 照合）、アイコンは `results_view.rs` の抽出スレッド（in-flight 集合で照合）。issue が言う「新しい機構ではない」は channel + 照合の型については正しいが、**長寿命 worker の前例はこの crate に無い**

## 2. 段取り — 計器が先である

**A 側の標本は改修の前にしか取れない**（`PERFORMANCE.md`「計測と受け入れ基準」）。ゆえに 2 PR に分け、順序は動かせないものとして扱う。

| | 中身 | 閉じる条件 |
|---|---|---|
| PR 1 | 打鍵→ピクセルの計器・フレーム間隔の不変条件・A 側ベースライン | 検知器が **A 側で赤い**ことを実測し、標本 3 つ以上を `PERFORMANCE.md` へ記録 |
| PR 2 | 単一 worker + seq 照合 | 同じ検知器が緑へ倒れる |

**PR 1 の時点で不変条件が赤いのが正しい姿である。** これは事故ではなく、検知器が発火しうることの実測である——#930 は「発火しうるかを測ったら発火しなかった」検知器を作りかけて閉じた。赤いことを確かめてから緑へ倒す順序でなければ、PR 2 の緑が「直った」を意味しない。

## 3. PR 1 — 計器

### 3.1 受け入れ 1 と 2 は別の量である

**打鍵→ピクセルのレイテンシは worker 化で改善しない**（採り込みが次フレームへ回るぶん、むしろ 1 フレーム増える）。改善するのは 1 フレームの長さ、すなわち UI が固まらないことである。**合否を決めるのはフレーム間隔の側であり、打鍵→ピクセルは内訳を読むための従である。**

### 3.2 trace を 2 本足す

| イベント | 出すもの |
|---|---|
| `egui_frame` | `update_us`（そのフレームの `update()` 全体）/ `interval_us`（前フレームからの間隔） |
| `egui_search:settled` | `seq` / `frames`（打鍵フレームから採り込みフレームまでの枚数）/ `since_key_us` / `since_dispatch_us` |

`egui_search:settled` の経過は**打鍵起点と dispatch 起点の両方を出す**。打鍵起点には 50 ms の trailing debounce 待ちが必ず入るため、片方だけでは worker 往復の費用を読み取れない。

`seq` は打鍵で振り、結果を描いた paint 完了で締める。**同期版でも非同期版でも同じ器が当たる形にすること**——同期なら `frames = 1`、worker 化後は 2 以上になるだけである。#1000 が「上流の改修の前後で同じ器を当てられること」と戒めたのは、反復 11 で計器が測る枝と変更が触る枝が食い違った事故（`measure_real_index_footprint` が cache-HIT 枝しか測らなかった）による。

既存の `egui_search:dispatch`（`Engine::search` の区間だけ）は**置き換えず外側に足す**。内訳——lock 待ちと走査の比——を読む材料として残す。

### 3.3 判定は smoke の不変条件へ載せる

`scripts/smoke-egui.ps1` にパスクエリ（`c:\users` 等）の打鍵注入区間を足し、`scripts/lib/SnotraTraceInvariants.psm1` へ H6 を置く:

> **H6**: 打鍵注入区間内で、`egui_frame` の `update_us` がリフレッシュレートの逆数を超えるフレームが現れたら異常

**測るのはフレームの所要時間であって、フレーム間隔ではない。** このランタイムはイベント駆動で、通常フレームは勝手に回らない（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）。打鍵中の repaint 予約は `search_debounce.interval()`（50 ms）であり、打鍵そのものの間隔は 100〜200 ms である。**健全な状態でも間隔は常にリフレッシュレートの逆数を超える**ので、間隔で判定すると H6 は PR 2 の後も永久に赤いままになり、赤→緑の反転が「直った」ことの証拠にならない。

**`PERFORMANCE.md`「操作中の上限」をそのまま当てられない**ことも同じ理由による。あの節の読み方は「ポインタ移動中の p50 がリフレッシュレートの逆数を**大きく下回ったら**回帰を疑う」——過剰再描画を捕まえる**逆向き**の判定であり、連続再描画が前提のポインタ移動区間に固有である。#1004 の受け入れ 2 が「同じ読み方」と言うのは閾値の由来（リフレッシュレートの逆数）についてであって、向きと対象までは同じにならない。**受け入れ 2 はこの解釈で実装する**。

**trace の presence 検査にしてはならない。** 「worker へ投げた」というログは「フレームが落ちなかった」を意味しない——#671 PR A′ では `egui_results:hide` が出ているのに窓が残り、`smoke:egui` が緑のまま通した。H6 は「起きてはならないこと（予算超過のフレーム）が区間内に現れないこと」を測る形であり、既存の H1 / H4 / H5 と同じ性格である。

リフレッシュレートは `snotra-egui-runtime/src/monitor.rs` が取得している値を使う。**固定の 16.7 ms を書き込まない**——高リフレッシュレート環境で緩すぎる判定になる。

## 4. PR 2 — 単一 worker + seq 照合

### 4.1 トポロジー: プロセス寿命の worker 1 本 + 最新クエリ勝ち

```
update() ──SearchRequest{seq, query}──▶ [worker]
                                          try_recv で溜まった要求を吸い、最後だけ採用
                                          engine.lock() → search() → 解放
   drain ◀──SearchMsg::Done{seq, rows}──┘  wake_main()
   seq == pending_seq のときだけ set_results
```

**都度 spawn を採らない理由。** `spawn_folder_load` の doc が記す「per-nav spawn は意図的」の根拠は、1 つの hung `read_dir`（dead UNC）が後続の全ロードを塞ぐことの回避である。**この根拠は検索へ転移しない**——`engine.search` は共有 Mutex 上の CPU 仕事であり、hang しない代わりに必ず lock を要求する。打鍵ごとに spawn すれば、古いクエリの走査が lock を握って新しいクエリを待たせる列ができ、捨てるとわかっている結果のために CPU と lock を払う。**単一 worker が捨てるのは走らせる前の要求であり、都度 spawn が捨てるのは走らせた後の結果である。**

### 4.2 wake は `wake_main` を使う。worker は `egui::Context` を持たない

`window_coordinator::wake_main(&AppHandle)` を送信ごとに呼ぶ。

**長寿命 worker が `egui::Context` の clone を握ってはならない。** Context の clone は repaint callback ごと複製し、callback が握る `RepaintScheduler` の Arc が窓の `Destroyed` を越えて worker の停止・join を止める（#671 PR D の不変条件・`snotra-egui-runtime/CLAUDE.md`）。folder / icon の worker が ctx clone を渡せるのは**都度 spawn で寿命が短いから**であって、そこを根拠に「検索 worker も ctx を持てる」と推論してはならない。

**main を起こせば足りる。** results 窓は `drive_results_window` が main の `update()` から駆動されるため、`wake_results` は要らない。

### 4.3 世代には触らない

`rows_generation` の加算は `set_results` が持ったままにする（#699）。dispatch の `seq` は**新しいカウンタ**であり、両者は別の量である——`seq` は「どの要求か」、`rows_generation` は「行が差し替わったか」を指す。`run_search_with` へ無条件加算を戻すと、`set_results` を呼ばずに返る経路（folder cache 未着）で空撃ちになる。

### 4.4 in-flight 中の表示は前の行を保持する

folder の cache 未着枝（「set しない」）と同じ扱いにする。空クエリ（`query().trim().is_empty()`）と indexing 中は**従来どおり同期で即クリア**する——worker を経由させると、消した文字が 1 フレーム残る。

### 4.5 in-flight の失効 — 同期で行を差し替える経路はすべて `pending_seq` を進める

**規則はこう置く: `set_results` を同期で呼ぶ出所は、同じ場所で `pending_seq` を bump する。** hide / reset-on-show / フォルダ遷移だけを列挙すると穴が残る——`c:\u` まで打って worker が 20 ms の走査に入った直後にクエリを空へ消すと、§4.4 の同期クリアが空行を置いた後で `Done{seq}` が届き、`seq` は現在の `pending_seq` と一致するので**空クエリの下に古い行が生え直す**。Instant / Command への遷移も同型である（どちらも同期で行を置く）。

列挙で守ると列挙が腐るので、**「同期で行を差し替えたら in-flight は必ず古い」を不変条件として書く**。

hide については加えて、**hidden 中は `update()` が走らない**（`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」）ため、hide を跨いだ in-flight は reset-on-show の backstop とセットで設計する。既存の `reset_for_show` は `search_debounce` を作り直しているので、そこへ `pending_seq` の bump を並べる。

検知器を H7 として置く:

> **H7**: 失効した `seq` の採り込み（`egui_search:settled` が `seq < pending_seq` で現れる）があったら異常

`egui_search:settled` に採り込み時点の `pending_seq` を載せることで判定材料が揃う。これが受け入れ 4 に対応する。

### 4.6 debounce は維持する

50 ms・leading のまま変えない。**worker 化は debounce を代替しない**——debounce は「打鍵を間引く」、worker 化は「間引いても残る 1 回をフレームから外す」であり、別の問題である。worker 化後は debounce の役割が「フレームを守る」から「engine lock の占有回数を減らす」へ変わるだけで、消す理由にはならない。

### 4.7 worker の停止

`Sender` が drop されれば `recv` がエラーで返り、ループが終わる。`LauncherController` はプロセス終了まで生きるので、実質はプロセス終了と同時である。**join はしない**（best-effort）。§4.2 のとおり worker が Context を持たないため、窓の `Destroyed` を妨げない。

## 5. 受容する残余

1. **lock 競合は消えない。** アイコン取得・config 適用・index build の swap は依然として最大 ~20 ms 待つ。本書が消すのは UI フレームの停止だけであり、待ち時間そのものを縮めるのは #1003 である。**この残余を書かずに「lock の問題を解いた」と書いてはならない**
2. **走り出した走査は止まらない。** cancel flag を `snotra-core` の走査ループへ通す案は採らない——core の改修になり本 issue の射程を超える。残る症状は「最新クエリの表示が最大 20 ms 遅れる」であり、debounce がバースト中の発行を先頭 1 回に抑えるため実害は小さい
3. **木を使った索引化は本書の前提にしない**（#1004 本文の判断をそのまま引き継ぐ）。パスマッチは正規化済みフルパスへの部分文字列照合で、スコアが `PATH_BASE - min(byte_pos, PATH_POS_CAP)` とバイト位置に依存する以上、ノードを辿る形は意味論の変更である

## 6. 検証

- **`/race-check`**（`AGENTS.md`「条件別チェック」の worker spawn・channel・フレーム drain の行に逐語で該当）。計画時のゲートとして回す
- 受け入れ 3（結果が同期版と一致すること）は、seq 照合の採否ロジックを純粋核として切り出し単体テストで測る。**実結果の一致を smoke の件数比較で代替しない**——件数は一致しても順序が違いうる
- `SPEC.md` の同期要否: 検索の**結果**は変わらず、変わるのは反映のタイミングである。§ の記述が「同期で反映する」と読める箇所があれば同期する（実装時に確認する）
