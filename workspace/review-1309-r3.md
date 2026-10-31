# review-1309 r3（code-reviewer・3 巡目）

- align が名乗った sha: a3d851547877b7b42f7a5827bfac1aa9301cf7fb（HEAD 一致。対象差分は d9815054..a3d85154）
- Critical 0 / High 0 / Low 0 / ⚠ 2

## 1. M5ce の当て直し

| 変異 | 結果 | 落ちたテスト（assert） | 巻き戻し後 hash（=アンカー） |
|---|---|---|---|
| M5ce かな 2 か所（build.rs:139 と :498）を同時に旧 fold（小文字化+nucleo・空白は触らない）へ | 検知 | kana_search_matches_entry_with_fullwidth_space（migemo.rs:57 assert_eq: left [] / right ["ドキュメント U+3000 メモ"]）。他は全緑（638 passed / 1 failed） | build.rs f1d7f681 OK |

最後の git status は .delegate/ だけ（出力先）。

## 2. 対処差分のレビュー

### 新しいテストのかなの文字列（一時テストで実測し、巻き戻した。query.rs c085186f OK）
- クエリ: normalize_for_match = "dokyumento memo"、to_kana = "どきゅめんと めも"。ASCII の英字は残っていない。
- 名前（現行）: normalize_for_match = "ドキュメント メモ"、to_kana = "どきゅめんと めも"。クエリのかなを含む（true）。
- 名前（旧い写像）: to_kana = "どきゅめんと" + U+3000 + "めも"。クエリのかなを含まない（false）。
- 正規化した名前は正規化したクエリを含まない（false）。直接一致では成立せず、照合はかなの経路で成り立っている。検知器として空虚ではない。M5ce で実際に落ちた。
- 射程: このテストは SearchEngine::new を通るので、かなは Wave 1 の経路（build.rs:139）で作られる。木の経路（:498）だけの退行は、前巡どおり A/B テスト（save_side_collapse_..._agree_at_entry_view）が捕まえる。両方が揃って全体を守る。

### memory_footprint.rs の文言と条件
- 2 本目の腕（!cache_hit）: 文言「旧版で、ロードが当たらなかった（製品が読まない版・config の不一致・破損のいずれか）」は条件の粒度と一致する。全走査の枝である、この実行が現行版を書き出した、の 2 点も cache-miss 枝（load_or_scan_with_stats_in の save_cache_sorted_in）と一致する。
- 3 本目の腕（cache_hit かつ版が現行と違う）: 製品の Write 経路で旧版枝を通ったときだけ到達する。「この実行のロードの旧版枝が現行版へ書き戻した」は upgrade_legacy_cache_in と一致する。2 本目と指示（もう 1 回走らせる）が揃った。

## 3. ⚠
- ⚠1（前巡から継続・変化なし）tests/search_allocations.rs はクエリ側の借用の退行を見ない。借用は query.rs の単体テストが捕まえる。
- ⚠2（新規・軽微）memory_footprint.rs の 2 本目・3 本目の腕は v != 現行 を一律に「旧版」と名乗る。on-disk が現行より新しい版（より新しいブランチのビルドが書いた index.bin）のときも「旧版」と出る。手元の計器だけの話で、製品の挙動には関わらない。気にするなら v > 現行 を別の腕にする。
- 前巡の ⚠3（かなの空白の正規化を A/B だけが守る）は、M5ce が検知されたので解消。
