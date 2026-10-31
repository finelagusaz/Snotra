# review-1327-r2（sha 4e442afec157c4cff5c81e615d34f4f388192ad6）
align exit 0 / snotra-core/CLAUDE.md の blob 181532ac = アンカー / governance:check 17 件 passed

## 1 巡目の所見の解消（grep とコードの読みで確かめた）
- M1 解消: Collapsed/Raw 枝は compute_wave1/2 を通らない（build.rs:510-545）。L28 の規則も同じ行に入った
- M2 解消: path_store.rs の再実装（:241-245,381-388,472-495）。path_store_cursor_matches_full_rebuild（path.rs:256）と _normalize_entry_key_over_real_index（:397）は実在する。ほかの to_lowercase + / 置換（config/paths.rs:45・opener.rs:64,79）は別のキーなので「だけ」は成り立つ
- F1/F2 解消: Collapsed{LowerNameColumn,LowerFileColumn} と Raw{Vec<String>,Vec<Option<String>>} は型が違い、取り違えはコンパイルを通らない（columns.rs:80-95・:76-77）。v6 が Collapsed であることも columns.rs:81 と一致する
- L34 解消（検知器は columns.rs:197）/ L47 解消 / L33 解消（build.rs:375）/ L9 解消 / scan.rs:106 解消
- root_roles 解消: 両テストは scan/tests.rs:81,113 にあり、親が先・子が先を走査の結果で固定している
- SPEC.md:815 解消: 新 L41 は keys.rs:1 の //!（手順を列挙している）を指すようになった

## 新しい ⚠（すべて Low）
- M1 の文の射程: new_with_cached_masks は compute_wave2 をどの枝でも通らない（マスクは CachedMasks から来る）。compute_wave1 も None 枝（wave1_from_tree・build.rs:552）では通る。「Collapsed/Raw 枝は通らない」は真だが、Wave 2 のマスクの列はどの枝でも通らない。案:「new_with_cached_masks は compute_wave2 を通らず、compute_wave1 も None 枝だけ」
- M1: キャッシュに載らない Wave 1 の列（kana と同じ形）も、Collapsed/Raw 枝で別に建てる必要がある（kana_for_cached の形）。「キャッシュに載る列はチェックリスト」の対だけでは、載らない列の扱いが書かれていない
- M2「ここと tests/path_query_cost.rs の写しも直す」の「ここ」が path_store.rs を指すのか、この文書を指すのか曖昧。案:「search/path_store.rs と tests/path_query_cost.rs の写しも直す」
