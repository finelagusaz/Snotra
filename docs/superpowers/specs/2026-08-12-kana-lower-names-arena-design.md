# `kana_lower_names` を密な文字列アリーナで持つ（索引メモリ削減・残る唯一の per-entry 確保）

作成日: 2026-08-12 / ブランチ: `perf/kana-lower-names-arena` / issue: #1056 / 前提: #1003（派生文字列 2 列のアリーナ化）

#1003 の反復で `lower_names` / `lower_file_names` をアリーナへ移した結果、実運用点（migemo OFF）の
索引はエントリ数に比例する確保を 1 つも持たなくなった。残っているのは `kana_lower_names`
（`Vec<Box<str>>`）1 本だけで、**migemo 有効時にしか現れない**。

## 1. 対象 — 実測された規模

合成ラダー（`tests/memory_footprint.rs` Phase B・2026-08-12 実測）:

| N | migemo | live | blocks |
|---|---|---:|---:|
| 100,000 | off | +0.04 MiB | -99,990 |
| 100,000 | on | +3.66 MiB | +12 |
| 38,847 | off | +0.01 MiB | -38,837 |
| 38,847 | on | +1.42 MiB | +12 |

**`on` 行の `+12` を「per-entry ではない」と読んではならない。** ラダーの増分は入力
`Vec<AppEntry>`（2 blocks/entry）の解放を含むので、kana の実体は `on` と `off` の**差**に現れる
——N = 100,000 で **100,002 ≒ N**、live で **+3.62 MiB**。

**受け入れの計器には出ない。** 実運用点の計測は migemo OFF ゆえ、この列は `PERFORMANCE.md`
「採用: 派生文字列 2 列も文字列アリーナで持つ」の数字のどこにも現れない。migemo 利用者だけが
払っている額である。

## 2. 読まれ方 — 索引から外して導出へ移せるか（実測して却下した）

`normalized_keys` は索引から外して `target_path` からの導出へ移せた（`2026-08-07-normalized-keys-removal-design.md`）。
同じ判断をここへ当てられるかを、**実装前に**測った。

判断の基準は当該設計が置いたもの——**その読みが早期 return の前にあるか後ろにあるか**。
`kana_lower_names[i]` の読みは `search/scoring.rs` の `score_one_entry` で
`primary_score.is_none() && kana_available` のときに走る。つまり**名前・file name で
マッチしなかった多数派の側**であり、`normalized_keys` の履歴照合（通過後の装飾）とは逆である。

導出案は、読み口に `kana_char_masks` の pre-filter を足して対象を絞る形になる（足すこと自体は
挙動を変えない——`kana_char_mask` は偽陽性しか出さないので、通らない候補は kana substring が
成立しない）。ゆえに導出の実費は**pre-filter の通過数 × `to_kana` の単価**である。

実データ 312,108 件・migemo ON・release・2026-08-12（GPD WIN MINI / G1617-01 / 23.8 GB）:

| クエリ | かな | pre-filter 通過 | 通過率 | 導出（単スレッド） | 実際に一致 |
|---|---|---:|---:|---:|---:|
| onga | おんが | 12,362 | 3.96% | **38.1 ms** | 26 |
| syasin | しゃしん | 8,525 | 2.73% | 21.8 ms | 0 |
| mozi | もじ | 8,374 | 2.68% | 20.8 ms | 0 |
| aidea | あいであ | 6,075 | 1.95% | 14.7 ms | 0 |
| kanri | かんり | 4,169 | 1.34% | 11.2 ms | 5 |
| setutei | せつてい | 2,341 | 0.75% | 6.0 ms | 0 |
| dokyu | どきゅ | 2,220 | 0.71% | 7.8 ms | 7 |
| gemu | げむ | 307 | 0.10% | 1.1 ms | 0 |
| sisutemu | しすてむ | 9 | 0.00% | 0.02 ms | 2 |
| puroguramu | ぷろぐらむ | 3 | 0.00% | 0.01 ms | 2 |

**却下する。** pre-filter の選択率そのものは悪くない（最悪 3.96%）が、`to_kana` の単価が約 3.1 µs/件と
高く、最悪クエリで 38 ms が純増する。上表は単スレッドの計測であり実際は rayon で割れるが、
8 並列でも約 5 ms/打鍵が今日の 0 ms へ上乗せされる計算で、フレーム予算 16.7 ms に対して
割に合わない。pre-filter の精度も低く（12,362 通過に対し一致 26 件）、導出のほとんどは捨てられる。

**通過率はクエリごとに 3 桁ばらつく**（0.00%〜3.96%）。`kana_char_mask` は Unicode スカラ値を
`& 63` で 64 bit へ折り畳むので、かな 1 文字が ASCII 文字と bit を共有するかどうかで決まる。
**平均では判断できない**ため、上表の最悪ケースで判断した。

なお導出案が消すのは常駐だけで、**構築時の `to_kana` は残る**——`kana_char_masks` は
kana 文字列から導出するので、migemo ON の起動では全件の変換が今までどおり走る。

### 却下: `lower_names` と同じ共有鎖で束ねる

`lower_names` は「表示名と一致したら `None`」で潰してある。同じ形を kana にも当てられるなら
blob ごと消えるが、**成立しない**——`to_kana` は `wana_kana` の `to_hiragana()` であり、
カタカナだけでなく**ローマ字もひらがなへ変換する**（`to_kana("dokyu") == "どきゅ"`・
`query.rs` の `to_kana_converts_romaji_to_hiragana`）。ASCII 名でも
`kana_lower_name != lower_name` になるため、共有が成立する要素はごく少ない。

## 3. 設計 — `index_tree::NameArena` をそのまま使う

`SearchEngine.kana_lower_names: Vec<Box<str>>` → `NameArena`（`blob: String` + `offsets: Vec<u32>`）。

**要素は全て実体を持つ**（`Option` ではない）ので、疎な列向けの `str_arena::OptionalStrArena` は
当たらない。当たるのは表示名が使っている密なアリーナである。

### 新しい型を作らない

`NameArena` の線上表現は `arena_wire_format_is_identical_to_vec_of_string` が固定しており、
**その核が kana 側の都合と独立に動く将来を挙げられない**（「片方だけが変わる将来を 1 つ
挙げられるか」の検算）。serde impl が kana の消費者から使われないだけである。密な核を
`str_arena.rs` へ抽出する案は、線上表現に最も敏感なコードを触る churn に対して挙動上の利得が
ゼロで、#997（実行可能な消費者が生じるまで共通関数へ括り出さない）と同じ向きで採らない。

代償は 1 つ、`index_tree.rs` の `//!` が言う「オンディスクと索引が共有する表現」という位置づけが
少し伸びること。**`//!` に一行足して明示する**——メモリ専用の消費者が付いたこと、その消費者は
serde を通らないこと。

### 並列構築を落とさない（設計制約）

**アリーナへの push は逐次だが、逐次化してはならない。** kana の構築は 2 経路あり、
片方は**並列である**:

| 経路 | 現行 | 通る場面 |
|---|---|---|
| `compute_wave1` の kana 枝 | `iter().map().collect()`（逐次・`rayon::join` の 3 本のうち 1 本） | cache-miss（走査 22〜30 秒の内側） |
| `new_with_cached_masks` の `kana_for_cached` | `into_par_iter().map().collect()`（**並列**） | **キャッシュヒット起動＝毎回** |

`to_kana` の単価は §2 で 3.08 µs/件と実測した。312,108 件を逐次で回せば **約 0.96 秒**であり、
毎起動に乗る。**素直に push へ書き換えると、常駐 3.5 MiB と引き換えに起動 1 秒を払う。**

ゆえに `kana_for_cached` は「添字の塊ごとに並列で局所アリーナを組み、順に併合する」形にする:

1. 添字を塊へ分け、塊ごとに `(String, Vec<u32>)`（連結バイト列と要素末尾オフセット）を並列に組む
2. 順序を保ったまま集め（indexed parallel iterator の `collect` は順序を保つ）、
   連結バイト列を繋いでオフセットを塊の先頭ぶん底上げする

**併合のずれは既存の A/B 突き合わせが捕まえる。** `search/tests/build.rs` は
`compute_wave1`（逐次）と `kana_for_cached`（並列）を同じ入力で突き合わせており、
**2 実装のままにしておくことがこの検知器の効力の源である**——両方を同じ併合へ寄せると、
オフセットの底上げを間違えても両側が同じようにずれて素通りする。

### 確保の見込み

塊ごとの局所バッファは伸長に任せ、併合先だけ総バイト数（塊の合計）で 1 度確保する。
`compute_wave1` 側は逐次のまま `with_capacity(n, 0)` でオフセット列だけ確保する
（`lower_names` 列と同じ形）——**総バイト数を先に知る手段が「全件を `to_kana` してから
数える」しかなく**、cache-miss 経路では 1 パス増やす対価のほうが大きい。

## 4. 波及範囲

- `index_tree.rs`: `NameArena::{push, with_capacity}` を `pub(crate)` へ、`is_empty()` と
  §3 の併合の口（塊の `(String, Vec<u32>)` から組む）を追加、`//!` に一行。
  **serde impl は触らない**（線上表現は無変更）
- **`to_kana` は `String` を返すので、要素ごとの一時確保は残る**（`*_into` 版が無い）。
  消すのは**常駐**の per-entry 確保であって、構築中の一時確保ではない
- `search/build.rs`: `Wave1Strings` の 3 要素目を `NameArena` へ。kana 枝を push へ。
  `compute_kana_char_masks(&NameArena)`。`assemble` の `shrink_to_fit` と
  「両方空 or 両方 `entries.len()`」の `debug_assert` を追随（**不変条件そのものは変えない**）
- `search.rs` / `search/scoring.rs`: `kana_available = !kana_lower_names.is_empty()`、読みは `.get(i)`
- `search/footprint.rs`: `boxed_strs` + `vec_body::<Box<str>>` の 2 行を **blob / offsets の 2 行**へ。
  **束ねて 1 行にしない**——呼び出し側は「1 行 = 1 確保」でブロックを数えるので、
  束ねた瞬間に 1 つ数え落とす（#1003 で未帰属 +1 blocks として実測済み）。
  migemo OFF で 0 のまま行を残す「消さない」注記は維持する
- `snotra-core/CLAUDE.md`: 並列レイアウト節の kana 例外の記述（`Vec` である前提の文）
- `PERFORMANCE.md`: 採用の項目を追加

**触らないもの**: `index.bin`（この列は保存しない）、`kana_char_masks`、migemo ON/OFF の
条件付き構築という不変条件。**保存しない列ゆえ、線上表現の一致を証明する検知器も
`INDEX_CACHE_VERSION` のバンプも要らない**——#1003 でいちばん硬かった部分が丸ごと無い。

## 5. 受け入れ条件

1. 合成ラダーの `migemo=on` 行で、`off` 行との blocks の差が N から**わずかな定数**へ落ちる。
   issue の「0 付近」はこの意味であり、**リテラルの 0 ではない**。
   **実測は +2**（blob と `kana_char_masks`）——オフセット列は migemo OFF でも 1 ブロック在る
   （空のアリーナが番兵 `vec![0]` を持つ）ので差には現れない。**実装前に「3 前後」と書いたのは
   この番兵を数えていなかったからで、値は `PERFORMANCE.md` の実測が正本である**
2. 3 回の実行でバイト数・ブロック数が完全に一致する
3. 対のレイテンシ実測（**migemo ON で**・同日・同セッション・各 3 標本以上）で検索が退行していない
4. `PERFORMANCE.md` へ **migemo ON** の実測として記録し、**測った機体名を書く**
   （開発機は 2 台あり、「開発機」とだけ書くと過去の表を現在値の基準に使えなくなる）

**計測を migemo ON で取らないと、この反復は自分の額を測れない。** 実運用点（migemo OFF）だけを
見ると「何も変わらない」と出る。

## 6. 残余リスク

- **構築時間の退行を必ず測る。** §3 の並列を保つ形が効いているかは
  `bench_new_migemo_on_off`（`compute_wave1` 側）だけでは見えない——毎起動の経路は
  `kana_for_cached` である。**キャッシュヒット起動の実測（`cache_load_ms` を含む起動段の計測）を
  対で取る**
- **`shrink_to_fit` の対を落としても検索結果は変わらない**（余剰容量が最後まで常駐するだけ）。
  検知器は `search/tests/build.rs` の容量検査で、kana 列にも同じ検査を足す
- §2 の導出案の却下は**この索引・この機体の実測**に基づく。pre-filter の通過率はクエリ依存で
  3 桁ばらつくため、別の索引形状で結論が変わりうる。単価（約 3.1 µs/件）は名前の長さ程度に
  しか依らないので桁は動かないが、**それも測った 1 台での話である**
- **migemo OFF の利用者は +1 blocks（4 B）を払う**——空の `Vec<Box<str>>` は 0 ブロックだが、
  空の `NameArena` は番兵オフセットの 1 ブロックを持つ。実運用点（Phase A）の常駐ブロックが
  112 → 113 になったのはこれである。番兵を捨てる表現は `len()` が `0 - 1` で溢れるので採らない
