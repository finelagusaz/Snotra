# 索引メモリの削減 — 実測と最適化のループ規約

作成日: 2026-08-07 / ブランチ: `perf/index-memory-footprint`

この文書が定めるのは**ループの規約**であって、個々の最適化ではない。何を削るかは
測ってから決まるものであり、先に決めれば実測が追認装置に堕ちる。

## 1. 背景 — 重心が索引へ移った

`PERFORMANCE.md`「egui 期のメモリ実測」の数値は 2026-07-25 のもので、実運用点が変わった。
2026-08-07 に release ビルドでアロケータ計数ハーネスを再実行した結果:

| 指標 | 2026-07-25 の記録 | 2026-08-07 実測 | 倍率 |
|---|---:|---:|---:|
| 索引エントリ数 | 38,847 | **312,377** | 8.0× |
| `index.bin` | 6.80 MiB | **107.0 MiB** | 15.7× |
| `SearchEngine` 常駐 | 14.96 MiB | **166.08 MiB** | 11.1× |
| ロードピーク | 19.97 MiB | **273.08 MiB** | 13.7× |
| 1 エントリあたり（常駐） | 404 B / 6.0 blocks | **557 B** / blocks 未報告 | — |

ロード区間（`entries` + `masks` + 下記の再スキャン複製）は **767.2 B/entry・7.00 blocks/entry**。
常駐側の block 数はハーネスが報告しておらず、**7.00 からの引き算で推し量らない**（反復 0 で測る）。

エントリ数の増加は実 `config.toml` の `[[paths.scan]] path = 'C:\'` +
`include_folders = true`（C ドライブ全体のフォルダ索引）による。**設定の事故ではなく
実運用形**であり、これを削減の前提とする。

この結果、`#689`（jp_font 省略・-12.9 MiB）と `#691`（`from_static`・-10.6〜-30.0 MiB）が
狙ったフォント常駐は**全体の 6% 未満**になった。**最適化の順位は絶対量ではなく現プロファイル
での占有率で決まり、それは索引が育つと勝手に入れ替わる。**

### 副産物: `BackgroundRescanTask` の全エントリ複製

Phase A の `drop(rescan_task)` 前後で **62.49 MiB**（210 B/entry）が解放された。
`indexer.rs` の `BackgroundRescanTask` が `cached_entries: Vec<AppEntry>` として全エントリの
複製を持つためで、キャッシュヒットする毎回の起動で背景再スキャンの実行中ずっと常駐する。
**本ループの対象外**（軸が「1 エントリあたりのバイト数」に決まったため）。issue へ振り分ける。

## 2. 機構 — 同じバイト列を 1 エントリで 4 重に持っている

`memory_footprint.rs` の合成分布コメントは folder 率 98.9% を記録するが、**これは 38,847 件
だった 2026-07-25 時点の実測であり、312,377 件の現運用点では測っていない**。以下の重複が
どれだけの entry に当たるかは folder 率に依存するため、率そのものを反復 0 で測る。

フォルダ 1 件 `C:\a\b\Projects` に対する常駐:

| 保持先 | 中身 | 由来 |
|---|---|---|
| `entries[i].target_path` | `C:\a\b\Projects` | 原本 |
| `entries[i].name` | `Projects` | `target_path` の末尾成分（`file_name()`） |
| `lower_names[i]` | `projects` | `to_lower_folded(name)` |
| `lower_file_names[i]` | `Some("projects")` | `to_lower_folded(file_name(target_path))` = **`lower_names[i]` と同一** |
| `normalized_keys[i]` | `c:\a\b\projects` | `normalize_entry_key(target_path)`（小文字化 + `/`→`\`） |

- `lower_file_names[i]` が `lower_names[i]` と一致するのは **`is_folder` のとき**である。
  indexer はフォルダの `name` に `file_name()` を、ファイルの `name` に `file_stem()`（拡張子なし）を
  使うため、ファイル（1.1%）では両者が食い違う
- `normalize_entry_key` は `char::to_lowercase()`（Unicode）を使う。**長さ保存ではない**
  （`İ` → `i̇` は 2 char）。「on-disk バイト数は `target_path` と完全一致する」という
  `PERFORMANCE.md` の記述は一般には成り立たない。導出へ置き換える案はこの点で
  バイト同一性を保証できねば履歴照合が**沈黙で外れる**（クラッシュしない）
- `to_lower_folded`（`query.rs`）と `normalize_entry_key`（`indexer.rs`）は別関数である。
  同一視して統合しない

## 3. 決定 1 — 計器と候補階層の対応

**候補の規模より小さいノイズ床の計器で測ると、何も測れないかノイズを退行と読む。**

| 候補の階層 | 計器 | 分解能 |
|---|---|---|
| エントリ単位・索引の派生 Vec | `snotra-core/tests/memory_footprint.rs`（アロケータ計数） | 決定的・`layout.size()` バイト正確 |
| フォント・egui Context・プロセス全体 | `scripts/measure-memory-stages.ps1`（PrivComm） | 実行間ばらつき **~4 MiB** |

本ループの候補はすべて索引側ゆえ、**アロケータ計数で決着させる**。段階別 PS1 は
実行中の snotra を kill し SendKeys でキーボードを奪うため、原則使わない。

**ハーネスは必ず `--test-threads=1` で実行する。** 計数器は `static AtomicUsize` の
プロセス大域であり、cargo test 既定の並列実行では Phase A / Phase B が計数器を奪い合って
**もっともらしい数値**を出す（2026-08-07 に実測: N=38,847 で 3018 B/entry・N=100,000 で
304 B/entry と単調性が破れ、`live 0.00 MiB` が現れた）。エラーにならず数値として出るため、
内部矛盾でしか気づけない。実行コマンドは `docs/build-commands.md` を SSOT とし、
そこへ `--test-threads=1` を追記する。

## 4. 決定 2 — 各反復の受け入れ条件

1. **A/B は同日・同バイナリで両方測る。** 文書中の数値を A 側に使わない
   （`PERFORMANCE.md`「warm frame は日をまたいで比較しない」をメモリ軸へ適用）
2. **メモリ削減 1 件につきレイテンシ実測 1 件を対にする。** 前例は `#110` —
   同じ並列 Vec 群の AoS 統合が保守性目的で試みられ、fuzzy 全走査 35〜120% 遅化で却下された。
   対にする bench は `bench_fuzzy_search_scaling` / `bench_new_scaling`（`docs/build-commands.md`）
3. **`index.bin` に触るなら 7 点チェックリストを通す。** `IndexCache`(v4) は派生 Vec 5 本
   （`char_masks` / `file_name_char_masks` / `lower_names` / `lower_file_names` / `normalized_keys`）を
   すべてディスクに持つ。手順は `snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」、
   `INDEX_CACHE_VERSION` バンプ・v3 フォールバック鎖・golden bytes 更新を含む。`/persistence-check` を起動する
4. **1 反復 = 1 候補 = 1 PR**、`perf/` ブランチ。複数候補を束ねると、どれが効いたか
   分離できずレイテンシ退行の原因も特定できない

## 5. 反復 0 — 内訳の分離計測（本 PR の範囲）

現行ハーネスは `SearchEngine` 全体を 1 つの数字で返す。166.08 MiB の内訳が無いままでは
候補の順位が §2 の機構からの推測に留まる。**実装は製品コードに触れない。**

`measure_real_index_footprint` の Phase A で、`SearchEngine` 構築の**前**に手元にある
`entries` と `cached_masks` を直接走査し、Vec ごとに次を報告する:

- 文字列バイト数の合計（`len()` の総和）
- 確保容量の合計（`capacity()` の総和）
- 要素数と非 None 数（`lower_file_names` の `Option` 内訳）

**`len` と `capacity` を分けて出すことが要点である。** `CachedMasks` は `Vec<String>` で届き
`SearchEngine` は `Vec<Box<str>>` で持つため、`into_boxed_str()` が縮小しているのか、
postcard の deserialize が過剰容量を残しているのかが、この 2 値の差で初めて分離できる。
557 B/entry のうち「文字列そのもの」と「容量の遊び」の比が分からないままでは、
候補 A〜C（文字列を消す）と別の手（容量を詰める）のどちらが効くかを選べない。

同時に測る 3 点:

- `is_folder` の率（§2 の重複がどれだけの entry に当たるかの係数）
- フォルダ entry における `lower_names[i] == lower_file_names[i]` の一致率
  （§2 は機構からの導出であり、実データでは未測定）
- 常駐時の live ブロック数（候補 D の規模。ロード区間の 7.00 blocks/entry から引き算しない）

反復 0 の成果は**数値と、それに基づく候補の順位**であって、削減ではない。

## 6. 候補一覧（機構由来の暫定順位・反復 0 の実測で確定する）

| # | 候補 | 機構上の削減 | 主なリスク |
|---|---|---|---|
| A | `lower_file_names` をフォルダで `lower_names` と共有 | 1 block + name 長 / entry（98.9%） | `Option<Box<str>>` の型変更が `EntryView` と 3 コンストラクタへ波及。`index.bin` 形式 |
| B | `normalized_keys` の廃止（`target_path` から導出） | path 長 / entry | Unicode 小文字化の非長さ保存。履歴照合ホットパスの再計算コスト。`index.bin` 形式 |
| C | `entries[i].name` の廃止（`target_path` から導出） | name 長 / entry | `file_stem` / `file_name` の分岐を全参照点へ。`index.bin` 形式 |
| D | 派生 Vec のアリーナ化（連結バッファ + オフセット） | ブロック数 5 → 2 前後 | 削減分は `layout.size()` に**現れない**（Windows ヒープのブロックヘッダ・サイズクラス丸め）。効果の検証手段を別途要する |

**D の注意**: アロケータ計数は `layout.size()` の集計であり、ブロックあたりの実オーバーヘッドを
含まない。312,377 entry × 5 block = 156 万ブロックゆえ実 RSS への寄与は無視できないが、
**本ループの主計器では見えない**。D を採るなら PrivComm 側の計器が要り、その分解能は
~4 MiB である。順位は最後に置く。

## 7. 却下・保留した選択肢

- **フォント常駐（`jp_font` 13.26 MiB の `OnceLock`）**: 絶対量は大きいが、set-once・never-clear は
  `'static` 化（`transmute`）の健全性の根拠であり、解放はライフタイム模型の変更になる。
  索引が 166 MiB の現プロファイルでは占有率が低い。**本ループの対象外**
- **エントリ数そのものの削減（`C:\` 走査を絞る）**: 効果は最大だが、検索でヒットしなくなる
  ものが出る＝仕様変更であり `SPEC.md` 同期が要る。ユーザーが軸として選ばなかった
- **`BackgroundRescanTask` の複製（62.5 MiB）**: 一時ピークの軸であり、選ばれた軸
  （1 エントリあたりのバイト数）に属さない。issue へ振り分ける

## 8. 残余リスク

- **`index.bin` 形式を変える候補（A/B/C）はすべて後方互換の負債を持つ。** 旧形式の凍結
  バイト列からの deserialize テストが要る（`snotra-core/CLAUDE.md`「オンディスクの
  シリアライズ struct をリファクタするとき」）
- **`PERFORMANCE.md` の egui 期の節は現運用点と 1 桁ずれている。** 反復 0 の完了時点で
  実測値を更新する。更新しないと、次に読む者が §1 と同じ誤読を繰り返す
- 反復 0 は製品コードに触れないため挙動退行の余地は無いが、ハーネスが Phase A で
  実 `index.bin` を読む点は変わらない。実インデックスの無い環境では従来どおり自動スキップする
