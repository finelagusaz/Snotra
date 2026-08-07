# `normalized_keys` の廃止（索引メモリ削減・反復 2）

作成日: 2026-08-07 / ブランチ: `perf/index-normalized-keys` / 前提: `2026-08-07-index-memory-footprint-design.md`（ループの規約）

反復 1（`shrink_to_fit`・PR #961）で索引の常駐は 166.08 → 137.83 MiB になった。
残る最大の単一項目が `normalized_keys` である。

## 1. 対象 — 実測された規模

| 項目 | 実測 |
|---|---:|
| `normalized_keys` の文字列 | **35.56 MiB**（119.3 B/entry） |
| 同 Vec 本体（`Box<str>` 16 B × 312,377） | shrink 後の実測値を再取得する |
| `normalized_keys[i] == normalize_entry_key(target_path)` | **312,377 / 312,377（100.0%）** |

**廃止の論拠は「実データで 100% 一致した」ではない。** それはこの索引の性質であって
関数の性質ではない。正しい論拠は**同じ関数に同じ入力を与えれば同じ結果になる**ことであり、
実測はその裏取りにすぎない。この区別は §3 の設計を決める。

## 2. 読まれ方 — コストが 2 つに分かれる

`search/scoring.rs` の `score_one_entry` に `let base_score = score?;` があり、
マッチしなかったエントリはそこで早期 return する。`normalized_key` の読みは
**その前と後に分かれて存在する**:

| 読み | 位置 | 掛かる件数 |
|---|---|---|
| `history.get_global_stats_normalized` | 早期 return の**後ろ** | マッチした件数のみ |
| `history.query_count_pre_normalized` | 同上 | 同上 |
| `history.folder_expansion_count_normalized` | 同上 | 同上 |
| パス部分一致 `normalized_key.find(pq)` | 早期 return の**前** | `path_query` が Some のときの全候補 |

**同じフィールドでも、フィルタとして使われる読みと通過後の装飾として使われる読みでは
コストの桁が違う。** 派生データを消して再計算に替える判断は、平均コストではなく
この位置で決まる。

さらに `has_path_sep` のクエリは Fuzzy ビットマスク pre-filter を**スキップする**
（`snotra-core/CLAUDE.md`「モジュール構成」の search.rs 節）。つまりパスクエリは
もともと全走査であり、そこへ 1 エントリあたりの正規化が上乗せされる。

## 3. 設計 — スレッドローカルの再利用バッファ

`normalized_keys` を持たず、必要な地点で `target_path` から詰め直す。

```
thread_local! { static KEY_BUF: RefCell<String> }
// buf.clear() してから normalize_entry_key_into(&mut buf, &entry.target_path)
```

**手書きの「大文字小文字を無視した比較器」を作らない。** これが本設計の要である:

- `normalize_entry_key` は `char::to_lowercase()`（Unicode）を使う。1 文字が N 文字へ
  展開されうるため長さ保存ではない
- 畳み込み比較を自作すると、1 バイトでもずれた瞬間に**履歴照合が沈黙で外れる**。
  クラッシュせず、検索結果も返り、ブーストだけが効かなくなる——気づく手段が無い
- 同じ関数へ同じ入力を通すなら、バイト一致は**構成から**保証される。実測に頼らない

バッファは容量を再利用するため、暖まったあとの確保はゼロ。正規化そのものの計算量は残る。

### 却下: ハッシュキーへの置き換え

`FxHashMap<String, _>` を `FxHashMap<u64, _>` にすればキー文字列が要らなくなるが、
2 つの理由で成立しない:

- `history.rs` の `recent_launches(&self, max) -> Vec<&str>` が**キーそのものをパス文字列
  として返す**。履歴のキーは索引ではなくデータである
- ハッシュ衝突は 2 つのパスが起動回数を共有する形の**沈黙したデータ汚染**になる

### 却下: `raw_entry` によるアロケーションなし照合

`rustc_hash::FxHashMap` は std の `HashMap` の別名で、`raw_entry` は安定化されていない。
`hashbrown` を直接依存に足せば可能だが、§3 の「同じ関数を通す」論拠が使えなくなり
（eq クロージャが手書きの畳み込み比較になる）、上と同じ沈黙する誤りの余地が生まれる。

## 4. 実装前に測る — 本設計の唯一の未知数

**パスクエリの全走査に正規化を乗せたときのコスト。** 実装してから測るのでは、
測った時点で後戻りの費用が乗っている。実 `index.bin`（312,377 件）に対し、
実装前に次の 2 つを同条件で測る:

- (a) 現行: `normalized_keys[i].find(pq)` を全件
- (b) 変更後相当: 再利用バッファへ `normalize_entry_key_into` してから `find(pq)` を全件

**判定**: (b) − (a) がパスクエリ 1 打鍵あたりの追加コストである。

- 許容できるなら **全廃**（設計どおり）
- 許容できないなら **部分廃止へ後退**: 履歴照合だけ再計算に替え、パス照合用の
  `normalized_keys` は残す。削減量は落ちるが波及は最小

「許容できる」の閾値はこの文書では固定しない——測った値を見て決める。**先に閾値を
書くと、測定が閾値を正当化する作業に化ける。**

## 5. 波及範囲

- `IndexCache` を **v5 へバンプ**し `normalized_keys` フィールドを落とす。v4 フォールバック鎖を
  追加（v4 は `normalized_keys` を持つが読み捨てる）。golden bytes 更新。
  手順は `snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」の 7 点、
  `/persistence-check` を起動する
- `CachedMasks.normalized_keys` を削除
- `SearchEngine.normalized_keys` を削除。`assemble` の `debug_assert!` と `shrink_to_fit` から外す
- `EntryView` から `normalized_key` を外す（`search/scoring.rs`）
- `compute_wave1` の出力が 4 要素 → 3 要素
- `search/tests/build.rs` の容量検査から 1 本外す
- **`history.bin` は無変更**（キー形式を変えない）

## 6. 受け入れ条件（ループの規約 §4 より）

1. A/B は同日・同バイナリ。文書中の数値を A 側にしない
2. メモリ削減 1 件につきレイテンシ実測 1 件（`bench_fuzzy_search_scaling` /
   `bench_new_scaling` に加え、**パスクエリの実測を必ず含める**——本設計で唯一
   コストが増える経路であり、既存 bench はパスクエリを覆っていない）
3. 旧形式（v4）の凍結バイト列から deserialize できるテスト
4. 1 反復 = 1 PR

## 7. 残余リスク

- **削減量は測ってから書く。** `Box<str>` の Vec 本体ぶんを算術で足した値を成果として
  報告しない（反復 1 で 2 回踏んだ導出と測定の混同）
- v4 キャッシュを持つ既存ユーザーは、v5 初回起動で `normalized_keys` を読み捨てる。
  Wave 1 の再計算は走らない（他の派生 Vec は v4 に揃っている）ことを確認する——
  ここを取りこぼすと初回起動が遅くなる
