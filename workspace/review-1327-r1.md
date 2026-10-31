# review-1327 — snotra-core/CLAUDE.md 縮約（sha a4f2ac22a5edcd02a7142e70bf82167983003881）

align: sha=a4f2ac22a5edcd02a7142e70bf82167983003881 / base=ca30b1fd2913a633363f55a6e4dfee75baf569cd / exit 0
snotra-core/CLAUDE.md の blob 7f13e2256369585b915e5d28a3e7110867e31181 = アンカー（変異の各巻き戻し後も一致）

## 1. 失われた規則
High/Critical は無し。
- Low: 旧 L229 scan_all の両順序の検知器（scan_all_dedups_when_roots_are_nested / _the_child_root_comes_first）を名指す doc が無い → root_roles の doc に 1 文足す
- Low⚠: 旧 L76 検索スコアは search.rs・フォルダ列挙は folder.rs に集める → 弱い (b)（各 //! だけ）
- Low⚠: 旧 L28 エントリ数に比例する確保を持つ列を戻さない → 指示としては無い。実装前チェックへ 1 句足す
(b) の file:line、(c) R2 scan.rs:100-103 / R3 PERFORMANCE.md:41 / R4 cache.rs:1047-1055（どれも食い違いなし）、(d) 世代機構・背景再スキャン → 呼び出し側への返り値に載せた

## 2. 偽の文
- Medium 新 L32「全コンストラクタが共有する」: new_with_cached_masks の Collapsed/Raw 枝は compute_wave1/2 を通らない（search/build.rs:510-545。wave1_from_tree は None 枝 :552 だけ）
- Medium 新 L42 末尾: search/path_store.rs は normalize_entry_key の規則の再実装（path_store.rs:241-245,381-388,472-495）。照合側について「同じ関数を通す」は字義どおりには成り立たない。一致は search/tests/path.rs の 2 本が守る
- Low 新 L34: 検知器 derived_masks_come_from_the_uncollapsed_strings を名指すのは derive_entry_collapsed の doc（columns.rs:197）であって derive_entry_lowers ではない
- Low 新 L47: kana_char_mask は search.rs:292 にある
- Low 新 L33: SearchEngine::new は kana を常に建てる（build.rs:375）
- Low⚠ F1 cache.rs:881 / F2 cache/tests.rs:332:「Raw にすると結果は変わらず」— Raw は Vec<String> などで SameAsLowerName を表せず写像が要る（columns.rs:76-77）
- Low⚠ 新 L9: インラインの mod tests も多い / scan.rs:102「CI が守るのは…まで」は単独で読むと強すぎる

## 3. 参照
正準形はすべて着地（governance:check 緑）。⚠ SPEC.md:815 は手順の内訳の正本をこの節としているが、新版が送る normalize_entry_key_into の doc は手順を列挙しない（keys.rs:1 の //! は列挙する）。cache.rs:347 の「プロセスをまたぐ」の話は INDEX_WRITE_LOCK の doc 経由でのみ着地する

## 4. 計器の検算
| 変異 | 結果 |
|---|---|
| 「engine.rs のロック最小化パターン」を改名 | 赤（G-heading-refs 3 件） |
| 「エントリ名の導出ルール」を改名（参照 0） | 緑 ⚠ 計器なし |
| モジュール構成から binfmt.rs を削除 | 赤（G-module-index） |
| モジュール構成から tests/dir_stat_cost.rs を削除 | 緑 ⚠ crate ルートの tests/ は母集団の外 |
巻き戻しのたびに blob 7f13e225 と一致
