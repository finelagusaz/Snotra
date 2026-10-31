# plan-review 独立導出 — issue #1309（+ #1310 を同じ変更で）

対象: issue **#1309**（照合用の正規化の入口を 1 つにする・空白だけの入力の SPEC 明文化）と **#1310**（`normalize_query` の借用判定）。ユーザー裁定 3 件（2026-10-31）: (a) #1310 も同じ PR で直す（NFC/NFD #1285 は別）、(b) SPEC §4.2 に「正規化後に空なら、その面の空の扱いに従う」を 1 行足す・挙動は変えない、(c) 冪等性の欠陥（51 字。例 U+0262 `ɢ` → `G`）も直し、入口の冪等性を全コードポイントの検知器で固定する。

`workspace/plan.md`・`research.md`・`adversarial-1309.txt` は読んでいない。ブランチ `fix/match-normalize-entry`（HEAD = main の 85a52750 と同じ。差分は未追跡の `workspace/` だけ）を対象にした。行番号はこの木のもの。

---

## 0. 現状（導出の土台）

正規化器は 2 つあり、どちらもまだ冪等ではない。

| 関数 | 中身 | 呼び出し元（`git grep`） |
|---|---|---|
| `query::normalize_query`（query.rs:162） | trim → 空白を `' '` 1 個へまとめる → 文字ごとに `to_lowercase().map(nucleo_normalize)`。`Cow` を返す。借用で返すかの判定は `ch.is_uppercase() \|\| nucleo_normalize(ch) != ch`（query.rs:181） | query_plan.rs:78（クエリ）、folder.rs:47 / :201 / :231（絞り込みと**名前の両側**・#1284）、`normalize_history_query_key`（query.rs:31）経由で history.rs:154 / :278 / :400 と query_plan.rs:140 |
| `query::to_lower_folded`（query.rs:21） | 文字ごとに `to_lowercase().map(nucleo_normalize)` だけ。**trim も空白の正規化もしない** | build.rs:117（Wave 1 `lower_names`）、`query::lower_file_name`（query.rs:157。Wave 1 と columns.rs:186 の `lower_file_names`）、columns.rs:185（`derive_entry_lowers`、`index.bin` へ書く）、build.rs:139 / :498（kana 名。両方とも `to_kana(&to_lower_folded(..))`）、folder.rs:110（`build_sort_keys`。並べ替えのキー） |

本体検索で SPEC §4.2 に反する点: クエリは `normalize_query` を、名前は `to_lower_folded` を通る。クエリ `a b` は名前 `a  b` や `a　b`（U+3000）に Substring でも Prefix でも一致しない。名前の先頭に空白があると Prefix に一致しない。

---

## 1. 変更が必要なファイルとシンボル

### 1a. 正規化の中心（`snotra-core/src/query.rs`）

| シンボル | 変更の要旨 | 根拠 |
|---|---|---|
| 新設: 文字ごとの折りたたみ（例 `fold_char(ch) -> impl Iterator<Item=char>`） | 小文字化とアクセント折りたたみの**唯一の定義**。冪等になる順で書く（例: `ɢ` → `G` → `g` のように、折りたたんだ後にもう一度小文字化する）。**順は先に書いた全走査テスト（Red）で決める。推論で決めない**。`to_lowercase` は 1 文字を複数文字へ展開しうる（`İ` U+0130 → `i̇`）ので、1 対 1 を前提にしない | 裁定 (c)・query.rs:21-26、:200 |
| `normalize_query`（照合の入口。名前は §2 参照） | (1) **借用判定を `fold_char` から導く**: `!fold_char(ch).eq(std::iter::once(ch))`。`is_uppercase()` という述語で言い換えない（#1310）。(2) 組み立て側も `fold_char` を通す。別に書いた分岐として残るのは空白の腕だけになる | query.rs:177-185 / :193-205・#1310 本文 |
| `to_lower_folded` | 照合の経路からは外す。残すなら `fold_char` の薄い包みにして、並べ替えのキー専用の層にする（§2）。消すなら呼び出し元を全部移す | query.rs:17-26 |
| `lower_file_name` | 名前側の入口を通す（`.map(to_lower_folded)` → 照合の入口 + `into_owned`） | query.rs:157 |
| `normalize_history_query_key` | 中身はそのまま（入口 + 区切りの統一）。入口の名前を変えるなら呼び出しを差し替える。冪等性は全走査の対象にする（§4） | query.rs:31-38 |
| `//!`・doc | §5 を参照 | |
| tests（query.rs:212-）| 全コードポイントの走査（§4）を足す。`to_lower_folded_*` は改名・削除に合わせて直す。`snotra-core/CLAUDE.md`「実装前チェック（必須）」の `query.rs` の項に従い、タブ・全角空白・NBSP を `' '` にそろえるテストと冪等性テストを置く（既存の `requires_alloc_for_tabs` / `requires_alloc_for_fullwidth_space` はある。冪等性テストは**無い**） | CLAUDE.md:24（tool-result 行）= snotra-core/CLAUDE.md の実装前チェック |

### 1b. 名前側の作り手を入口へ寄せる

| ファイル:行 | シンボル | 変更 |
|---|---|---|
| search/build.rs:117 | `compute_wave1` の `lower_names` | `to_lower_folded(&e.name)` → 入口(`&e.name`).into_owned().into_boxed_str() |
| search/build.rs:123 | `compute_wave1` の `lower_file_names` | `lower_file_name` の変更に追従する（コードの変更は無い） |
| search/build.rs:139 | `compute_wave1` の kana | `to_kana(&to_lower_folded(..))` → `to_kana(&入口(..))` |
| search/build.rs:498 | `new_with_cached_masks` の `kana_for_cached` | 同上。**kana は永続化されず毎起動作り直す**ので（build.rs:465「kana は毎起動再計算する」）、版には影響しない |
| indexer/columns.rs:185-186 | `derive_entry_lowers` | `to_lower_folded(&entry.name)` → 入口。記録側（`derive_columns`）も追記側（`extend_cached_masks`）もここを通るので、変える場所は 1 か所で足りる（columns.rs:181-190・:236-） |
| folder.rs:231 / :47 / :201 / :757(テスト) | `matches_filter` / `read_dir_entries` / `filter_sorted` / `read_dir_entries_for_test` | すでに `normalize_query` を通っている。入口を改名するときに差し替えるだけ |
| folder.rs:110 | `build_sort_keys` | §2 で判断（推奨は折りたたみの層） |

### 1c. index.bin（§3 の推奨案 = v8 へ上げる場合）

| ファイル | シンボル | 変更 |
|---|---|---|
| indexer/cache.rs:47 | `INDEX_CACHE_VERSION` | 7 → 8。線上の形式は変えない |
| indexer/cache.rs:842- | `load_cache_in` | v7 の枝を足す。**新しい struct は要らない**——`try_deserialize_with_header::<IndexCache<'static>>(&bytes, INDEX_MAGIC, 7)` で読める（v8 とバイト列が同じだから）。中身の扱いは §3 |
| indexer/cache.rs:785- | `LegacyRead` / `finish_legacy_read` | v7 は `entries` を持たないので、`LegacyRead` へ入れるには木を実体化するか（§3 案 A）、木を受ける形を足すしかない。**`masks` の扱い**: 旧い意味のマスクも信用できないので、`Skip` 腕で `None` → `from_tree` にする。そうすると doc「`masks` は v2 だけ `None`」（cache.rs:783）が偽になる |
| indexer/cache/breakdown.rs:259- | `cache_byte_breakdown_in` | v7 の腕を足す（`snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」(a)。版を上げた瞬間、v7 が「全ユーザーの `index.bin` が今まさに置かれている版」になる。breakdown.rs:291 のコメントが同じことを v6 について書いている） |
| indexer/cache/tests.rs:1350 | `load_cache_in_reports_the_version_it_actually_read` | v7 の枝を足す（同チェックリスト (b)）。:1380 のコメント「v6: 実運用点が今まさに置かれている版」は偽になる |
| indexer/cache/tests.rs:263 / :150 | `GOLDEN_V7` / `index_cache_on_disk_format_is_stable` | `GOLDEN_V7` は**本物の凍結された旧形式**として残す（doc「v7 は現行版ゆえ…forward-stability だけ」は偽になる）。v8 の golden（ヘッダーの版の 1 バイトだけが違う）を足して、`index_cache_on_disk_format_is_stable` をそれに向ける |
| indexer/cache/tests.rs（新規） | 凍結 v7 → `load_cache_in` | `GOLDEN_V7` を `load_cache_in` 経由で読む（同チェックリスト (c)。`try_deserialize_with_header` を直に呼ぶのでは枝の選択・`version` の帰属を通らない）。`Write` で v8 へ書き戻り、派生の列が新しい意味になっていることまで見る |
| indexer/cache/tests.rs:515 / :1357 | fixture の `to_lower_folded` | 入口へ差し替える（旧版 fixture は「当時の意味」で作ってよい。直すのは現行版を作る箇所だけ） |
| search/tests/build.rs:884-901 / :957 / :994 | 導出の A/B と kana の検算 | 入口へ差し替える |
| tests/memory_footprint.rs:266 | コメント「v7 と自称する」 | 版の数字を書かない形へ直す |

### 1d. SPEC・文書は §5

---

## 2. 入口を 1 つにするとき、何を寄せて何を寄せないか

### 寄せるもの（照合の両辺）

- **クエリ側**: `prepare_query_plan` の `norm_query`（query_plan.rs:78）。kana クエリ（query_plan.rs:91）は `norm_query` から作るので自動で追従する。
- **履歴のクエリキー**: `normalize_history_query_key`（入口 + 区切りの統一）。SPEC §4.2「履歴のクエリキーも同じ正規化」。
- **本体検索の名前側**: `compute_wave1` の `lower_names`、`derive_entry_lowers`（記録側・追記側の共通点）。
- **ファイル名の派生**: `query::lower_file_name`（Wave 1 と `derive_entry_lowers` が共有する）。忘れると、クエリ `a b.txt` がファイル名の経路で `a  b.txt` に一致しない。
- **かなの派生**: build.rs:139 と :498 の 2 か所。片方だけ直すと、migemo の結果がキャッシュヒットの起動と cache-miss の起動で食い違う。kana クエリは ASCII に限られ、空白は 1 個にまとめられているので、名前の kana に空白の連なりが残ると中間部分一致が外れる。
- **木から作る経路**: `wave1_from_tree` → `compute_wave1`（build.rs:105-108）。木専用の導出は無いので、`compute_wave1` を直せば足りる。build.rs:99「木専用の導出を書き起こさない」を守る。
- **フォルダ絞り込みの両辺**: すでに `normalize_query`。改名に追従するだけ。

### 判断が要るもの

- **フォルダの並べ替えキー**（folder.rs:110 `build_sort_keys`）: これは照合の述語ではなく、並べ替えのキー（「小文字名昇順」）である。AGENTS.md「片方だけが変わる将来を 1 つ挙げられるか」を当てると挙がる。たとえば、並べ替えには空白を保ったまま、照合だけ NFC を足す（#1285）。だから別の概念と見る。
  - **推奨**: 折りたたみの層（`fold_char` を包む `to_lower_folded`）に残す。冪等性の修正は受け取り、trim と空白の正規化は受け取らない。今日の順序を変えない。
  - 代案: 照合の入口へ寄せて `to_lower_folded` を消す（公開関数が 1 つ減る）。その場合、先頭に空白がある名前の順序が変わる（SPEC に順序の空白規定は無い）。
  - どちらでも、`to_lower_folded` を照合へ使う新しいコードを書かせないために、doc に「照合には使わない」と書く。

### 寄せてはいけないもの

- **パスマッチのクエリ** `path_query`（query_plan.rs:120-131）: 生クエリを小文字化し、区切りを統一するだけで、アクセントも連続空白も保つ。SPEC §4.2.1 条件 5 の背景、`snotra-core/CLAUDE.md`「incremental cache とパスクエリの非互換」。
- **`indexer::normalize_entry_key_into` / `normalize_file_name_key_into`**（履歴のパスキー、走査の重複排除、`IndexTree::file_key_into`）: SPEC:813「パスキー: アクセント折りたたみは行わない（パスは識別子）」。
- **`contains_path_sep`**: 区切りの判定であり、正規化ではない。
- **`measure_derived_sharing`**: 正規化ではなく共有の測定。入口を変えても同じ関数を通り続ければよい。**副作用**: 空白の連なりや U+3000 を含む名前は `lower_name != name` になるので潰れなくなる（メモリが少し増える。正しさは変わらない）。

### 名前について

issue は `query::normalize_for_match` を仮名に挙げている。選べる形は 2 つ。
- (i) `normalize_query` を `normalize_for_match` へ改名し、全呼び出し点を移す（AGENTS.md「関数・型を新規定義／改名」: LSP の findReferences で列挙 + `/dry-check`。旧名を消せば、下流の compile-fail が移行漏れを検出する）。
- (ii) `normalize_query` を残し、名前側だけ別名で包む。**(ii) は入口が名前の上で 2 つに見えるので、issue の趣旨に反する。推奨は (i)**。呼び出し点はすべて `snotra-core` 内にある（`git grep` で `src-tauri` には 0 件）。

---

## 3. `index.bin` の扱い

### 版を上げるか: 上げる

`snotra-core/CLAUDE.md`「データ永続化の注意」: 「データの意味（セマンティクス）を変更する場合もバージョン番号をバンプする」。v7 が持つ派生 4 本は、どれも旧い正規化を前提にしている。

- `lower_names` / `lower_file_names`: trim・空白の正規化・冪等性の修正で中身が変わる。
- **`char_masks` / `file_name_char_masks` も変わる**: (1) `ɢ` などは旧い出力が ASCII 大文字（`G`）で、`char_bitmask` は `a-z` しか数えないので、ビットが立たない。新しい出力（`g`）では立つ。(2) U+3000 や NBSP を含む ASCII 名は、旧い出力が非 ASCII なので `u64::MAX` になる。新しい出力は ASCII なのでビット単位で正確になる。旧いマスクを新しい文字列と組み合わせると、(1) で Fuzzy の事前フィルタが偽陰性を出しうる。

版を上げずに v7 の列を読み続けると、クエリ側だけが新しい意味になり、名前側は次の明示的な再構築まで旧い意味のまま残る。索引の更新は明示操作だけなので（`snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」）、旧い意味は何か月でも残りうる。

### v7 の `index.bin` を読んだときの選択肢

`derive_columns` が取るのは `Vec<AppEntry>`（columns.rs:132。`lower_file_name(&e.target_path)` にフルパスが要る。木を建てるのは導出の**後**: columns.rs:162-165）。v7 は `entries` を持たず、木を直に読む。

| 案 | 中身 | 一度きりの費用 | 毎起動の費用 | 制約との関係 |
|---|---|---|---|---|
| **A. 実体化して昇格** | v7 の枝: `tree.materialize()` → `LegacyRead{entries, masks: None, version: 7}` → `finish_legacy_read`。`Write` なら `upgrade_legacy_cache_in`（整列 → `derive_columns` → v8 で保存・`built_at` は持ち越す） | 昇格する起動 1 回だけ、`materialize` のピーク（`IndexTree::materialize` の doc は約 36 MiB、`snotra-core/CLAUDE.md` は「62.5 MiB の複製」と書く。**どちらもこの変更では測っていない**）と、Wave 1 相当の導出と保存。**全走査（22〜30 秒）は起きない**（`config_hash` は変わらないので cache-miss にならない） | v8 の枝に入るので、今と同じ | `snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」の「**『昇格をロード側に置いてはならない』の射程は v7 の枝だけである**——…木から作り直すと 62.5 MiB の複製が復活する」に**正面から反する**。この文は v7 が旧版になる日のために書かれたと読める。ただし理由として挙がっている費用は、旧版を読む起動 1 回に限られる |
| B. v7 → cache-miss（`None` を返す） | v7 の枝で `return None` | 全ユーザーが 1 回、全走査（22〜30 秒。`compute_config_hash` の doc の値）を払う | 今と同じ | コードは最小。ただし v6 以前のユーザーは安く昇格し、いちばん新しい v7 のユーザーだけが全走査を払うという逆転が出る |
| C. 木から直に導出する | 木からファイル名を復元し（非根は `name + table_str(aux)`、根は `table` のフルパスの末尾。`IndexTree::file_key_into` が正規化キーで同じ形を持つ: index_tree.rs:805-814）、`derive_columns` の木版と、木を受けて保存する経路を足す | 導出と保存だけ。実体化のピークは無い | 今と同じ | build.rs:99 / :103「木専用の導出を書き起こさない…規則が 2 つになると、片方の経路だけが静かにすれる」に反する。ファイル名の抽出規則（`Path::file_name` と木の復元）が 2 つになる。`file_key_matches_normalize_file_name_key_on_both_arms` と同じ形の両腕一致テストが要る |
| D. 版を上げず、毎起動 Wave 1 をやり直す | v7 の派生を捨てる | — | **毎起動**、`wave1_from_tree`（実体化を含む）。キャッシュヒットの起動で Wave 1 を飛ばすという v4 以降の前提を壊す | 却下 |
| E. 版を上げず、旧い列を使い続ける | — | — | — | 「意味を変えたらバンプ」に反する。却下 |

**推奨: A**。費用が一度きりで、全走査を伴わず、新しい導出規則も増えない。ただし `snotra-core/CLAUDE.md` の当該文を書き換えることになり、それは規範の変更にあたるので、ルート `CLAUDE.md`「セーフティネットの変更は合意してから」によりユーザーの合意を要する。合意が得られなければ B（コードが最小で、費用は 1 回の全走査）。A と B のどちらを選んでも、却下した案の理由は否定の知識になるので、新しい ADR（`docs/adr/ADR-<slug>.md`）の候補になる。

**A を採るときに揃える項目**（`snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」と、直前の版を受け取る側の数え直し）:
1. `INDEX_CACHE_VERSION` = 8。
2. v7 の枝は `IndexCache<'static>` を版 7 で読む（新しい struct は不要。線上の形式が同じだから）。
3. `LoadCacheResult.version = 7` にする。
4. breakdown.rs に v7 の腕を足す。
5. 版を報告するテストに v7 の枝を足す。
6. 凍結 v7（`GOLDEN_V7`）を `load_cache_in` 経由で読む。`Write` で書き戻り、`load_cache_does_not_rewrite_when_the_format_is_current` が v8 で成り立つこと。
7. v8 の golden を足す。
8. `legacy_upgrade_sorts_before_saving_so_the_tree_stays_shared` の射程に v7 の枝も入るか確かめる（実体化の出力は木の順に並ぶが、整列は通す）。
9. `LegacyRead.masks` と `Skip` 腕の意味を決める（下の ⚠）。

---

## 4. #1310 の直し方と検知器

### 直し方（型を断つ）

不具合の型は「チェック条件を、組み立てが変える文字の**言い換え**（`is_uppercase()`）で書いた」ことにある。AGENTS.md「代理条件も同じ型である」に当たる。直し方は、**判定を組み立てと同じ関数から導く**こと。

```rust
// 判定（非空白の腕）
if !fold_char(ch).eq(std::iter::once(ch)) { needs_alloc = true; break; }
// 組み立て
out.extend(fold_char(ch));
```

これで、別に書いた分岐として残るのは空白の腕（`prev_space || ch != ' '` と、組み立ての `' '` へまとめる処理）だけになる。trim（`str::trim` = `char::is_whitespace`）と組み立ての `is_whitespace` は同じ述語なので、そろっている。

### 検知器 1: 借用と所有が一致すること

テスト専用の参照実装 `reference(s)` を置く。trim → 空白をまとめる → `fold_char`、を**常に所有で**行うもの。すべての Unicode スカラー値 `c`（サロゲートを除く `0..=0x10FFFF`）について、次の各文脈で 2 点を確かめる。

- 文脈: #1310 の表の 4 形（`c`、`a{c}`、`A{c}x`、`x  {c}`）+ `{c} {c}`
- `normalize(s) == reference(s)`
- `matches!(normalize(s), Cow::Borrowed(_)) == (reference(s) == s.trim())`。所有で返る必要がないときは借用で返る、という性能側の契約。

失敗は全件を集めて 1 度に報告する（folder.rs の既存テストと同じ作法）。**変異の検算**: 判定を `is_uppercase() || nucleo_normalize(ch) != ch` へ戻すと、`ǅ` などで落ちることを確かめる（AGENTS.md「呼び忘れを再現する変異で落ちることまで」）。

### 検知器 2: 冪等であること

同じ走査で次を確かめる。

- `f(f(s)) == f(s)`（`f` = 入口）
- `normalize(f(s))` が `Cow::Borrowed` であること。不動点なら借用で返る。検知器 1 と組み合わせると、出力に「fold が変える文字」が残っていないことが言える。
- `normalize_history_query_key` についても `k(k(s)) == k(s)`。区切り統一の前に入口が `¥` などをどう扱うかに依存する（query_plan.rs:39 に「`¥` が畳まれる版では」とある）。
- 並べ替えのキー用に `to_lower_folded` を残すなら、それにも `g(g(c)) == g(c)` を確かめる。

**Red を先に置く**: 今のコードで検知器 2 が 51 字で落ち、検知器 1 が titlecase 等で落ちることを見てから、折りたたみの順を決める。

### 永続化側（`/persistence-check`）

- `history.bin`: `migrate_normalize_keys`（history.rs:387-）は**読み込みのたびに無条件で**走り、クエリキーを正規化し直して、衝突したキーはカウントを加算する。だから新しい正規化で記録したキーと旧いキーは、次の読み込みで統合される。`HISTORY_VERSION` を上げる必要は無い。`snotra-core/CLAUDE.md`「history.rs のキー正規化に関するチェックリスト」の 3 者（新規記録・既存の移行・参照 API）は、どれも `normalize_history_query_key` を通るので自動でそろう。旧い正規化で記録したキーが新しい正規化で 1 つにまとまることを、移行のテストで固定する（例: `ǅ` を含むキーと `ǆ` を含むキーが加算される、`ɢ` と `g`）。
- `index.bin`: §3。

---

## 5. 偽になる（または今すでに偽の）文書・doc・SPEC

### G-stale-identifiers の母集団について（scripts/governance/lib.mjs:678 / :701、checks/G-stale-identifiers.mjs のヘッダ）

母集団は `.claude/{skills,rules,agents}` の md、`docs/**`（`docs/superpowers/` と `docs/adr/` を除く）、ルートの `SPEC.md` / `CLAUDE.md` / `AGENTS.md` / `snotra-settings/SETTINGS-DESIGN.md` である。**`snotra-core/CLAUDE.md`・`PERFORMANCE.md`・`.rs` の doc コメントは母集団の外にある。** `git grep "to_lower_folded\|normalize_query"` を `*.md` に当てた結果、母集団の中での出現は 0 件だった。したがって `to_lower_folded` / `normalize_query` を改名・削除しても governance は赤にならない。**下の項目はすべて手で grep して直す範囲である**。

### SPEC.md

- §4.2（SPEC.md:117-129）: 裁定 (b) の 1 行「正規化後に空なら、その面の空の扱いに従う」を足す。§4.2 の他の行は、修正後の実装と一致する（本体検索の名前側が「エントリ名も」を満たすようになる）。冪等性や折りたたみの順を SPEC に書く必要は無い（語彙層）。

### snotra-core/CLAUDE.md（母集団の外）

- :125「`query.rs` の正規化と `Cow<str>` 遅延アロケーション」: 所有に切り替わる条件を「大文字・アクセント・連続空白がある場合のみ」と列挙している。titlecase（`ǅ`）や、単独のタブ・U+3000 を拾えておらず、すでに不正確。「`fold_char` が変える文字と空白の規則」へ書き換え、検知器（全走査）を指す。`normalize_query()` を改名するなら名前も直す。
- :156「history.rs のキー正規化に関するチェックリスト」: 「（`normalize_query` + パス区切り…）」の関数名（改名するなら直す）。
- :223「indexer.rs の索引更新の契機」:「**『昇格をロード側に置いてはならない』の射程は v7 の枝だけである**…」は、§3 で A を採ると偽になる（規範の変更なので合意が要る）。B なら「v7 は cache-miss へ落とす」と書き換える。
- :187「IndexCache バージョン変更チェックリスト」の続き: 「線上表現が v7 と同一」は真のまま。ただし版の番号を散文で書いている。
- 「`char_bitmask` は `query.rs` に一元化済み」節:「target_path から小文字 file_name を導出する処理は `query::lower_file_name()`」は真のまま（中身の正規化だけが変わる）。

### `.rs` の doc とコメント（母集団の外）

- query.rs:1-5 `//!`:「正規化は冪等」は**今は偽**（51 字）で、修正後に真になる。`normalize_query` の名前（改名するなら直す）。
- query.rs:17-20 `to_lower_folded` の doc:「the result is equivalent to nucleo's internal normalize→casefold」という順序の主張は、`ɢ` で偽。
- query.rs:153-156 `lower_file_name` の doc:「lowercase, accent-folded」は、入口へ寄せたら「照合の入口を通した」へ直す。
- query.rs:103 `DerivedSharing::lower_name_is_name` の doc:「`to_lower_folded` が恒等関数になる名前」。
- history.rs:384:「normalize_entry_key / normalize_query は冪等なので」は**今は偽**で、修正後に真になる（ここが冪等性に依存している）。
- search.rs:124 `SearchEngine::lower_names` の doc:「`to_lower_folded` が恒等関数になる名前」。
- search/build.rs:151-154 `compute_wave2` の doc:「to_lower_folded が主要な Latin アクセントを ASCII へ折り畳み済み」。build.rs:21-23 の import。
- indexer/cache.rs:135-137 `IndexCache` の doc:「v5 フォーマット…Wave 1（to_lower_folded）」（版の番号も古い）。cache.rs:783「`masks` は v2 だけ `None`」（§3 A で偽）。
- indexer/cache/tests.rs:263-270 `GOLDEN_V7` の doc:「v7 は現行版ゆえ」（v8 で偽）。:1380「v6: 実運用点が今まさに置かれている版」（すでに偽。v7 が現行なので）。
- indexer/cache/breakdown.rs:291-293:「v7 が現行になった瞬間、v6 は…」（v8 で、同じ論理を v7 について足す）。
- indexer/columns.rs:80-82 `CachedLower::Collapsed` の doc:「v6 以降」。v8 以降、v6 / v7 の Collapsed は旧い意味の列であり、`Write` では使われない。
- folder.rs:224-231 `matches_filter` の doc（`normalize_query` の名前）、:685 のコメント（「to_lower_folded で同キー化」は並べ替えのキーの話。§2 で折りたたみの層に残すなら真のまま）、:866 のテストの doc（「片方だけを `to_lower_folded` へ戻しても落ちる」。名前が変われば直す）。
- tests/memory_footprint.rs:266:「v7 と自称する」。

### その他

- `PERFORMANCE.md`:2444（「`to_lower_folded` が恒等関数になる名前…86.6%」）と :2509: 採用時の記録。86.6% は空白を含む名前の分だけ下がりうる。記録なので、書き換えるかは判断（下の ⚠）。
- `docs/adr/ADR-folder-filter-normalize-both-sides.md` の「帰結」: #1309 / #1310 を残余として書いている。**凍結された歴史なので触らない**（`ADR-adr-frozen-history`）。
- `docs/superpowers/**` の `to_lower_folded`: 非規範なので触らない。

### AGENTS.md「条件別チェック」で当たる行

- 関数の改名・新設 → LSP の findReferences と `/dry-check`。旧名を消して compile-fail を移行漏れの検出に使う。
- 永続形式（`index.bin` の意味、history キー）→ `/persistence-check`。
- 分岐を決める値の出所（`Cow` の借用か所有かの判定、`CachedLower` の variant と版の枝）→ 下流を 1 段たどる。
- 件数・導出の入力（派生の列 → 永続化する消費者 `index.bin`・共有の潰し）→ 下流を全段たどる。
- 文書に写しを増やす変更 → 正本は 1 か所に（SPEC §4.2 の 1 行と ADR）。

---

## 6. ⚠ 確信のない所見

1. **§3 案 A は規範文の書き換えを伴う。** `snotra-core/CLAUDE.md` の「射程は v7 の枝だけ」は、v7 が旧版になる日を見越して「ロード側で昇格させるな」と書いたと読める。その費用が「毎起動」を想定していたのか「昇格する 1 回」を想定していたのかは文面から決められない。ユーザー裁定の対象。
2. **旧版（v2〜v6）の `Skip` 腕は旧い意味の列とマスクを返し続ける。** `Write` 腕は捨てて導出し直すので製品は正しい。`Skip` を通るのは corpus テストの入口（`load_cached_entries`）だけ。`Skip` でも全旧版を `masks: None` にそろえるなら、`LegacyRead.masks` はどこからも読まれなくなり、フィールドごと消せる可能性がある（`upgrade_legacy_cache_in` は `masks` を受け取らない: cache.rs:738-745）。corpus テストが Wave 1 の費用を払うようになる点は測っていない。
3. **どの折りたたみの順が冪等になるかは測っていない。** 「折りたたんだ後に小文字化」で `ɢ` は直るが、51 字すべてと、`to_lowercase` が複数文字へ展開する文字（`İ`）の後段まで閉じるかは、全走査で決めるべきである。nucleo の表が非空白の文字を空白へ写すことがあれば、空白の腕の後ろに空白が生まれて冪等性が崩れる。この点は検知器 2 の `{c} {c}` 文脈で捕まる想定だが、未確認。
4. **本体検索のスコアが変わる。** Prefix のスコアは `PREFIX_BASE - lower_name.len()`（scoring.rs:227）で、tie-break は `lower_name` の辞書順（scoring.rs:194）。空白を含む名前は長さも順序も変わる。U+3000 を含む日本語の名前は実データでありうるので、「実用上ほぼ現れない」（#1310）とは影響の規模が違うかもしれない。実データでの件数は測っていない。
5. **Fuzzy モードでは nucleo 自身も大文字小文字とアクセントを正規化する**（`MatcherConfig::DEFAULT`・scoring.rs:73）。そのため Fuzzy の照合の食い違いは一部が隠れ、事前フィルタのマスク（§3）のほうが効く。検知器は Substring / Prefix で書くほうが確実。
6. **並べ替えのキー（§2）の推奨は、AGENTS.md の問いへの私の答えにすぎない。** ユーザーが「入口を 1 つ」を文字どおり「公開の正規化関数を 1 つ」と取るなら、代案（寄せて `to_lower_folded` を消す）が裁定の趣旨に合う。
7. `PERFORMANCE.md` の 86.6% など採用時の数値を書き換えるかは、`PERFORMANCE.md`「この文書へ記録するときの規約」に従う。読んでいないので判断しない。
8. `snotra-core/CLAUDE.md` の「62.5 MiB」と `IndexTree::materialize` の doc の「約 36 MiB」は、同じ実体化の費用を別の数字で書いているように見える（写しのずれの可能性）。測り直していない。
