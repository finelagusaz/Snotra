# 調査 — #1309（＋#1310）照合用の正規化の入口を 1 つにする

## issue の要約とユーザー裁定

- **#1309**: `SPEC.md`「4.2 クエリ正規化」は、クエリ**およびエントリ名**に「前後空白除去・小文字化・連続空白の 1 文字化・アクセント折りたたみ」を課す（全検索モード・フォルダ展開共通）。本体検索の名前側は `to_lower_folded`（小文字化と折りたたみだけ）で、空白を正規化していない。#1309 のコメント（#1311 の `/simplify` 由来）は、照合用の正規化の入口が 3 通りに割れていることを根として挙げている
- **#1310**: `normalize_query` の借用判定は `ch.is_uppercase() || nucleo_normalize(ch) != ch` で、**大文字ではないが小文字化で変わる文字**（titlecase の `ǅ` など。全コードポイントで 31 字）がすり抜ける。結果が同じ文字列の中の他の文字に依存して割れる。`snotra-core/CLAUDE.md`「`query.rs` の正規化と `Cow<str>` 遅延アロケーション」の契約（チェック条件とビルド条件が同じ結果を生む）に、この関数自身が反している
- **ユーザー裁定（2026-10-31・#1309 のコメント）**: #1310 も同じ変更で直す（NFC/NFD の #1285 は別扱い）。空白だけの入力は、SPEC §4.2 に「正規化後に空なら、その面の空の扱いに従う」を 1 行足す（挙動は変えない）

## 関連ファイル・シンボル（`git grep` で実在を確認）

### 正規化の定義（`snotra-core/src/query.rs`）
- `to_lower_folded`（:22）: 小文字化と `nucleo_normalize` だけ。空白は触らない
- `normalize_query`（:152）: trim・空白類を `' '` へ統一・連続空白の圧縮・小文字化・折りたたみ。`Cow` の借用判定に #1310 の欠陥がある
- `normalize_history_query_key`（:33）: `normalize_query` ＋パス区切りを `\` へ統一
- `lower_file_name`（:145）: `target_path` のファイル名に `to_lower_folded` を当てる

### 名前側（照合に使う派生文字列）の作り手
- 本体検索・構築時: `search/build.rs` の `compute_wave1`（:117 `lower_names`、:123 `lower_file_names` は `lower_file_name`、:139 `kana_lower_names` は `to_kana(&to_lower_folded(name))`）。木から作る経路（:498）も `to_kana(&to_lower_folded(tree.name_at(i)))`
- 本体検索・記録時: `indexer/columns.rs` の `derive_entry_lowers`（:185）。`index.bin` へ永続化される（`IndexCache` v7 の `lower_names` / `lower_file_names`）
- フォルダ展開の絞り込み: `folder.rs` の `matches_filter` 周辺（:47・:201・:231・:757）は、#1311 以降、両側とも `normalize_query`
- フォルダの並べ替えのキー: `folder.rs` の `build_sort_keys`（:110）は `to_lower_folded`。**照合ではなく整列**に使う

### クエリ側
- `search/query_plan.rs` の `prepare_query_plan`（:78 `normalize_query`、:140 `normalize_history_query_key`）
- 履歴: `history.rs` の `record_launch` などが `normalize_history_query_key`

### 永続化
- `indexer/cache.rs` の `INDEX_CACHE_VERSION = 7`。`lower_names` / `lower_file_names` は v7 の列として保存される
- `snotra-core/CLAUDE.md`「データ永続化の注意」: **データの意味を変える場合も版を上げる**。同「IndexCache バージョン変更チェックリスト」と、版を上げたときに直前の版を受け取る側を数え直す 3 項（breakdown の鎖・`load_cache_in_reports_the_version_it_actually_read`・直前の版の凍結バイト列）

## 再利用できる既存パターン

- 「記録側と照合側が同じ関数を通ることがバイト一致の根拠」の型: `normalize_entry_key_into`・`measure_derived_sharing`（`snotra-core/CLAUDE.md`）。照合用の正規化の入口も同じ型で 1 本にする
- 版を上げるときの旧版枝と形式昇格: `load_cache_in` の旧版枝・`upgrade_legacy_cache_in`（「indexer.rs の索引更新の契機」）
- `query.rs` の正規化を変えるときの必須テスト: タブ・全角空白・NBSP を `' '` へ統一するテストと冪等性のテスト（`snotra-core/CLAUDE.md`「実装前チェック」）

## 技術的制約

1. **名前側の意味が変わる**ので、`index.bin` の v7 に保存された `lower_names` / `lower_file_names` は古い意味のまま。版を 8 へ上げ、v7 を読んだときは派生の列を使わず木から導き直す必要がある（未検証。v7 の枝が派生の列を捨てて導き直す経路を持つかは調査が要る）
2. **#1310 を直すと `normalize_history_query_key` の出力が該当 31 字で変わる**。`history.bin` の既存のクエリキーと照合がずれうる。`migrate_normalize_keys` がクエリキーも読み込み時に正規化し直すかは未確認
3. 借用判定の修正は、ASCII 小文字だけのクエリで借用（確保なし）を保つこと。検索のホットパスにある
4. かなの派生（`kana_lower_names`）とファイル名の派生（`lower_file_names`）にも同じ入口を当てるか。SPEC は「エントリ名」と書き、ファイル名とかなを名指ししていない
5. `measure_derived_sharing` の共有（`lower_name == name`）の割合が、空白を含む名前で変わりうる（正しさではなく削減量に響く）

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1309.txt`。

**壊せなかった**:
- 命題 1（作り手の網羅）: 名前側の作り手は `compute_wave1`・`kana_for_cached`（木の経路）・`derive_entry_lowers`・`lower_file_name`・`folder.rs` で全部。他 crate は 0 件
- 命題 3: `history.rs` の `from_loaded` が `migrate_normalize_keys` を無条件に呼び、外側のクエリキーを `normalize_history_query_key` へ通し直す。正規化を直すと、既存のキーは読み込みのときに新しい規則へ収束する
- 命題 5: 借用判定に「小文字化で変わるか」を足せば、#1310 の 31 字のずれは 0 件になる。ASCII を先に分岐させれば確保なしを保てる
- `has_path_sep`・パスクエリとの相互作用・incremental cache の単調性（読解のみ）

| 所見 | 採否 | 反映 |
|---|---|---|
| v7 の枝は派生の列を必ず `CachedLower::Collapsed` で渡し、木から導き直す経路を持たない（`lower: None` は v3 だけ）。部品はある（v7 は木を持つので `lower: None` なら `wave1_from_tree` で導き直せる）。ただし `upgrade_save: None` なので書き戻しを足さないと、毎起動 Wave 1 を払う | **採る** | v7 の扱いを未確定として計画へ置く。`snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」は、v7 の枝で昇格をロード側に置くことを、62.5 MiB の複製が復活するとして戒めている |
| 「変わるのは空白を含む名前と 31 字だけ」は偽。名前側では、タブ・NBSP・U+3000 の `' '` への統一と前後空白の除去（Prefix 判定が変わる）も起き、空白だけの名前は空になる。非空白の文字では 2 関数の差は 0 件で、31 字は借用判定だけの欠陥 | **採る** | 受け入れ条件とテストに、U+3000 を含む名前・前後空白を含む名前を入れる |
| `normalize_query` と `to_lower_folded` は 51 字で冪等でない（小文字化→折りたたみの順で、結果が大文字になる。U+0262 など） | **採る**（ユーザー裁定で、この PR で直す） | 折りたたんだあとにもう一度小文字化する。全コードポイントの冪等性の検知器を置く |
| U+3000 を含む CJK 名は `lower_name_is_name` の共有が偽になる | 採る（記録のみ） | 削減量だけが下がる。正しさには響かない |
| パスマッチ（`path_query`・`path_store.rs`・`indexer/keys.rs`）は `normalize_query` を通らない別系統 | **採る** | 範囲外と計画に明記する |
| ⚠ 実機の `index.bin` は版 7。`migemo_enabled = true` でかなの経路は生きている。実機の索引に連続空白・U+3000 の名前は 0 件 | 採る | 実機での回帰確認は、空白を含む名前を一時的に用意して行う |

## 未解決の疑問

1. 入口の形: `normalize_query` をそのまま共有の入口にするか、`normalize_for_match` のような新しい名前を立てて `normalize_query` をそれへ寄せるか
2. 版を上げたときの v7 の扱い: 派生の列だけを導き直す（木は使う）か、cache-miss として全走査させるか
3. かな・ファイル名・フォルダの並べ替えキーに同じ入口を当てるか
4. `history.bin` のクエリキーの移行が要るか（#1310 の 31 字のみ）
5. #1310 の欠陥の型（チェック条件とビルド条件の不一致）を、全コードポイントで両経路を突き合わせる検知器で固定できるか
