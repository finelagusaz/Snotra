# review-1309 r2（code-reviewer・変異注入 2 巡目）

- align が名乗った sha: d98150549d7b94d2cebb42fd0ba81ff7959ba750（HEAD 一致。対象差分は fefe7d85..d9815054）
- ベースライン: lib 638 passed / 0 failed / 12 ignored。cargo test -p snotra-core --no-run も exit 0（memory_footprint.rs の変更がコンパイルされることを確認）
- Critical 0 / High 0 / Low 2 / ⚠ 3

## 1. 前巡の生存変異の当て直し（cargo test -p snotra-core --lib --no-fail-fast）

| 変異 | 結果 | 落ちたテスト（assert） | 巻き戻し後 hash（=アンカー） |
|---|---|---|---|
| M5b2 derive_entry_lowers を旧 to_lower_folded | 検知 | save_side_collapse_and_assemble_measurement_agree_at_entry_view（build.rs:99 lower_name ずれ: "app" vs " app"） | columns.rs c3c8f7a3 OK |
| M5c2 build.rs:498 木の経路のかなを旧 fold | 検知 | 同上（build.rs:143 migemo=true kana_lower_name: "あpp" vs " あpp"） | build.rs f1d7f681 OK |
| M5e build.rs:139 Wave1 のかなを旧 fold | 検知 | 同上（build.rs:143: " あpp" vs "あpp"） | build.rs f1d7f681 OK |
| M5f folder.rs 並べ替えキーを旧 fold | 検知 | sort_key_trims_leading_whitespace_like_the_filter（folder.rs:768: [" b","a"] vs ["a"," b"]） | folder.rs fa2cb8da OK |
| M5g lower_file_name を旧 fold | 検知 | entry_name_whitespace_is_normalized_like_the_query（basic.rs:635: ファイル名 a  b.lnk がクエリ a b.lnk に一致しない） | query.rs c085186f OK |
| 追加 M5ce かな 2 か所（build.rs:139 と :498）を同時に旧 fold | 生存 | なし | build.rs f1d7f681 OK |

最後の git status は .delegate/ だけ（出力先）。

## 2. 対処差分のレビュー

### Low
- R2-L1 snotra-core/tests/memory_footprint.rs:299-306（L5 の新しい腕）: 条件は !cache_hit だが、文言は原因を「製品が読まない旧版である」と断定する。cache-miss になる旧版には v7 のほかに、v2〜v6 で config_hash が合わない（設定を変えた）場合・破損がある。そのときも同じ文言が出て、読めるはずの v6 を「読まない版」と名乗る。修正例: 「このロードは当たらなかった（製品が読まない版・config の不一致・破損のいずれか）」と、条件と同じ粒度で書く。版のリテラルで分けない判断自体は妥当。
- R2-L2 同 :307-313（差分の外・既存の腕）: cache-hit の旧版腕は「昇格させてから測り直すこと（アプリを 1 回起動すれば…書き戻す）」と出すが、load_or_scan_with_stats は Write で読むので、この実行自体がすでに昇格を書き戻している（同ファイル上部の doc と cache.rs:500-505 がそう書く）。新しい腕の「この実行が現行版を書き出したので、もう 1 回」と並べると両腕で指示が食い違う。修正例: 「この実行が昇格を書き戻したので、もう 1 回走らせて測り直すこと」。

### 問題なしとした点
- A/B テストの「潰れの 4 種」の前提: collapse_census の非空虚検査は各カウンタ > 0 だけで、件数を足しても崩れない（ベースライン緑）。新しい 3 件はいずれも lower_name が name と異なり（Some 側）、file name が拡張子つき（Text 側）になる。
- 新しい fixture の効き方: M5b2/M5c2/M5e はいずれも index 2（" App"）で最初にずれた（実測）。
- basic.rs の追加: has_dot は norm_query の . で決まる（query_plan.rs:83）。make_entries は C:/fake/<name>.lnk を作るので、「名前では一致しえず、ファイル名でだけ一致する」形になっている。M5g で実際に落ちた。
- folder.rs の新テストのコメント「空白が英字より前に来て b が a より先になる」は M5f の出力（[" b","a"]）と一致する。
- query.rs:194-197 の新しいコメント: 「表が変われば、ここで扱わない限り 2 回目の適用で結果が変わる——冪等性の検知器が落ちる」は、空白へ写る文字 X があれば "aX" が 1 回目で "a " になり、2 回目の trim で "a" になる。冪等検知器の文脈 a{c} がそれを拾うので真。
- history.rs の内側キー・query.rs のエスケープ・cache/tests.rs の v7/v6 の散文 3 か所・GOLDEN_V7 の doc（GOLDEN_V8 とは添字 4 の版バイトだけ違うことを前巡で実測済み）: いずれも正しい。

## 3. ⚠ の現状
- ⚠1（前巡から継続）tests/search_allocations.rs はクエリ側の借用の退行（M3b）を見ない。差分はここに触れていないので状態は変わらない。借用は query.rs の単体テスト 3 本が捕まえる。
- ⚠2（前巡 L7）GOLDEN_V7 の doc に「差は版のバイトだけ」と明記された。v7 の扱いを守るのは版の判定のテスト（前巡 M6 で検知）だけである、という事実が文書化されたので解消とみなす。
- ⚠3（新規）かなの空白の正規化は、2 経路の突き合わせ（A/B）だけが守っている。かな 2 か所を同時に旧い写像へ戻すと検知されない（M5ce 生存）。挙動として現れるのは migemo 有効時に、空白を持つ名前がローマ字クエリのかなと一致するか。名前側のかなだけが連続空白・先頭空白を残すことになる（具体的なかな文字列は実測していない）。計画の「かなの派生に空白の正規化を当てる」を挙動で固定するなら、migemo の検索テストを 1 本足す（search/tests/migemo.rs）。片側だけの退行は A/B が捕まえるので、優先度は低い。
