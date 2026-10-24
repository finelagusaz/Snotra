# 調査: issue #1276 — フォルダ絞り込みの Fuzzy が `Utf32Str::new` で照合器へ渡す

## issue の要約

- `snotra-core/src/folder.rs` の `matches_filter`（Fuzzy の腕）は haystack（`to_lower_folded(name)`）と needle（`to_lower_folded(filter)`）を `nucleo_matcher::Utf32Str::new` で組む
- `nucleo-matcher` 0.3.1 の `Utf32Str::new` は、非 ASCII でも書記素の先頭がすべて ASCII の文字列に `Ascii(str.as_bytes())` を返す（結合文字のバイトを含んだまま）。`Utf32String::from(&str)` は `Unicode`（書記素ごとの先頭の文字）を返す
- 検索側は #1278 で `utf32_haystack`（`Utf32String::from(..).slice(..)` と同値）へ移った。フォルダ側は未対応
- 追記: フォルダの入口は呼び出しのたびに `Matcher::new`（`alloc_zeroed` 1 回・133,120 B）を払う。照合器を使うのは Fuzzy の腕だけ
- あるべき姿: フォルダ絞り込みも検索と同じ規則で照合器へ渡す。**規則は 1 か所に置き、検索と同じ関数を通す**

## バグか仕様変更か（`AGENTS.md`「開発ワークフロー」手順 1）

- `SPEC.md` §4.1「スキップマッチング（ファジー）: `nucleo-matcher` 相当」・§4.2「クエリおよびエントリ名は以下で正規化（**全検索モード・フォルダ展開共通**）」・§6.3「検索方式は『フォルダ展開時』の設定に従う」
- SPEC は検索とフォルダで照合の規則を分けていない。**記述に合わせる＝バグ**。SPEC の文言は変えない

## 実測（2026-10-24・scratch の probe・`nucleo-matcher` =0.3.1）

`to_lower_folded` と同じ畳み（`to_lowercase` → `chars::normalize`）を通した名前 10 × needle 20 = 200 組で、旧（両辺 `Utf32Str::new`）と新（両辺 `Utf32String::from`）を比べた。

- **真偽（一致する/しない）が変わったのは 1 組だけ**: 名前 `Re\u{301}sume\u{301}.docx` × needle `x\u{308}`（旧 `None` / 新 `Some(16)`）。**真偽を変えたのは needle 側**である——needle に結合文字があると旧は `Ascii` の生バイト（`x 0xCC 0x88`）として照合し、`0xCC` `0x88` が haystack に無いので外れる。新は書記素の先頭 `x` だけで照合する
- **条件は「非 ASCII の haystack × 結合文字を含む needle」に限られる**。nucleo 0.3.1 の `fuzzy_match` は `(Utf32Str::Ascii(_), Utf32Str::Unicode(_))` の腕で無条件に `None` を返す（`src/lib.rs` 261 行付近「a purely ascii haystack can never be transformed to match a needle that contains non-ascii chars」）。追加の測定:
  - `resume` × `e\u{301}`: 旧 `None` / 新 `None`（純 ASCII 名は修正後も一致しない）
  - `plain.txt` × `a\u{308}`: 旧 `None` / 新 `None`
  - `日本語 x.txt` × `x\u{308}`: 旧 `None` / 新 `Some(36)`（haystack は両経路とも `Unicode`——**needle だけで真偽が変わる**）
  - `Cafe\u{301}.txt` × `e\u{308}`: 旧 `None` / 新 `Some(16)`
- **haystack だけを直すと、直らないうえに壊れる**: haystack を `Utf32String::from` 相当・needle を `Utf32Str::new` のままにした組は、上の真偽が変わる 3 組すべてで旧と同じ `None` だった。さらに `Re\u{301}sume\u{301}.docx` × `e\u{301}` は旧 `Some(64)`・両辺修正 `Some(16)`・**haystack だけ `None`**（旧は両辺を `Ascii` の生バイトとして突き合わせていた一致を、`(Unicode, Ascii)` の組が落とす。敵対的調査が指摘し、主が probe で再測定した）。検索側と同じ規則にするには **両辺を同時に**——needle も `Utf32String::from` で作る（`query_plan.rs` の形）
- 敵対的調査の fuzz（現実的な候補の母集団・200 万組）では、両辺修正は 偽→真 22,979 件・**真→偽 0 件**。すなわち修正は一致集合を広げるだけで、旧で一致していた行を落とさない（標本であって証明ではない）
- **スコアだけが変わったのは 20 組**（例: `re\u{301}sume\u{301}` × `resume` が 154 → 166）
- haystack 側の誤分類だけでは ASCII の needle の真偽は変わらない（理由: `0x80` 以上のバイトは ASCII の needle 文字と一致せず、間隙として読み飛ばされるだけ。書記素の先頭の ASCII 文字の並びは両経路で同じ）。⚠️ 例外になりうるのは「非 ASCII を含み、かつ ASCII 文字が書記素の先頭でない」形（`\r\n` の `\n`）だけで、ファイル名には現れない
- **フォルダ絞り込みはスコアを使わない**——`matches_filter` は `fuzzy_match(..).is_some()` だけを見る。`filter_sorted` はキャッシュ済みの並びを保ったまま絞り、`read_dir_entries` の後段 `score_entries` は展開回数と名前で並べる。**issue 本文の「絞り込みの順位だけが静かに変わる」はフォルダ側には当たらない**。実害は「needle に結合文字を含むときの一致集合」に限られ、IME/キーボード入力は通常 NFC で、`to_lower_folded` が NFC のラテン系アクセントを ASCII へ畳むため、結合文字が needle に残るのは NFD の文字列を貼り付けたときに限られる

## 関連ファイル・シンボル

- `snotra-core/src/folder.rs`
  - `read_dir_entries`（`Matcher::new` を毎回）・`filter_sorted`（同）・`matches_filter`（Fuzzy の腕で `Utf32Str::new` ×2・候補ごとに `Vec::new()` ×2・`to_lower_folded(name)`）
  - テスト内 `read_dir_entries_for_test`（`Matcher::new` を毎回）・既存テスト `fuzzy_mode_matches_skipped_characters` / `substring_mode_does_not_match_skipped_characters` / `prefix_mode_matches_only_prefix`
- `snotra-core/src/search/scoring.rs`
  - `FuzzyScratch`（`matcher` + `haystack: Vec<char>`）・`thread_local! MATCHER`・`pub(super) fn utf32_haystack`・`fn match_score_single_cached`（Fuzzy の腕が `MATCHER.with` で借りる）
- `snotra-core/src/search/query_plan.rs`: `needle_u32 = Utf32String::from(norm_query.as_ref())`（検索側の needle は検索 1 回に 1 度 `From` で作る）
- `snotra-core/src/search/tests/basic.rs`: `utf32_haystack_matches_utf32_string_from`（合成）/ `..._over_real_index`（実索引・自己スキップ）
- `snotra-core/src/search.rs`: `mod scoring;`（private）。`crate::search::SearchMode` は folder.rs が既に import している
- `snotra-core/src/query.rs`: `to_lower_folded`
- 呼び出し元（製品）
  - `Engine::filter_sorted` → `folder::filter_sorted`: `src-tauri/src/egui_shell/launcher_controller/search_flow.rs:76`（**UI スレッド・打鍵ごと**）
  - `Engine::read_dir_entries`（`FolderListContext`）→ `folder::read_dir_entries`: `folder_nav.rs:76` が **filter `""` で**呼ぶ（ロック外 I/O のスレッド）。filter が空でも `Matcher::new` は払っている
  - `Engine::list_folder` → `folder::list_folder`: 製品の呼び出し元は無い（grep 0 件・テストのみ）

## 再利用できる既存パターン

- 検索側の Fuzzy の腕: `MATCHER.with(|m| { let s = &mut *m.borrow_mut(); let h = utf32_haystack(name, &mut s.haystack); s.matcher.fuzzy_match(h, needle) })`。照合器・展開先 `Vec<char>` の両方をスレッドごとに使い回す
- needle は呼び出し 1 回に 1 度 `Utf32String::from` で作る（`query_plan.rs`）

## 技術的制約

- `utf32_haystack` と `MATCHER` は `search::scoring`（private モジュール）の中にある。folder.rs から使うには可視性を `pub(crate)` へ広げ、`search` から再公開するか、置き場を移す
- `MATCHER` の `borrow_mut` は再入で panic する。folder の呼び出しは入れ子にならない（`filter_sorted` / `read_dir_entries` の中で検索は走らない）
- **スレッドローカルの常駐**: `filter_sorted` は UI スレッドで走るので、検索側の `MATCHER` を借りると UI スレッドに `Matcher`（133,120 B）+ 展開先が初回の Fuzzy 絞り込みから常駐する。検索の照合は rayon の worker で走り、呼び出し側スレッドでは走らない（`into_par_iter` は pool 外のスレッドからは注入して待つだけ）ので、UI スレッドでは新たな常駐になる。索引の常駐（54.92 MiB・`PERFORMANCE.md` の最新の表）に比べ 0.23% 程度
- 規則の検知器（`utf32_haystack_matches_utf32_string_from`）は既にある。フォルダ側が同じ関数を通ることを守る検知器は無い

- フォルダ側が `MATCHER` を借りると、「rayon の worker ごとに 1 組」と書いた文が偽になる（UI スレッドにも 1 組置かれる）。該当は 4 か所: `search/scoring.rs` の `FuzzyScratch` の `///`・`MATCHER` 直上の「D-5」コメント・`search/path_store.rs` の `CURSOR` の `///`（「`MATCHER` と同じ形」）・`tests/path_query_cost.rs` 164 行のコメント（同）。後 2 つは `CURSOR` 側の性質としては真のままなので、「`MATCHER` と同じ形」の比喩だけを外す

## 敵対的調査（3b）の所見と採否

出典は `workspace/adversarial-1276.txt`（sonnet 1 体）。

- **壊せなかった**: 命題 1（`(Ascii, Unicode)` は等長の `exact_match_impl` 経路も含め `None`）・2a（haystack だけの修正で偽→真になる組は無い ⚠️ 個別に数えてはいない）・2c（ASCII needle では haystack の誤分類だけで真偽が変わるのは `\r\n` の形だけ・ファイル名に現れない ⚠️ fuzz は標本）・3（フォルダはスコアを消費しない）・4a（UI スレッドに新たな常駐・Enter の同期 `search` も rayon が pool へ注入する）・4b（`read_dir_entries` の製品呼び出しは filter `""` だけ・`list_folder` は製品の呼び出し元なし）・6a（再入なし）・測定環境（probe の畳みは `to_lower_folded` と同一・nucleo は同じ 0.3.1・同じ `Config::DEFAULT`・needle も畳んだ filter）
- **壊せた → 採る**
  - 2b「haystack だけでは直らない」は弱すぎた——haystack だけの修正は旧の一致を落とす。上の「実測」節へ反映。計画は両辺を同時に替える
  - 6b 書き換えるコメントは 1 か所でなく 4 か所。上へ反映し、計画の変更ファイルへ足す
  - 常駐比の分母が古かった（97.51 → 54.92 MiB）。反映
- **⚠️ → 範囲外として記録のみ（今回の差分では直さない）**
  - SPEC §4.2 は「前後空白除去・連続空白の 1 文字化」もフォルダ展開に課すが、`folder.rs` は `to_lower_folded` だけで両方とも行わない。既存の食い違いであり、照合器への入力の規則とは別の問題。今回は触らない（報告で別 issue の候補として挙げる）
  - SPEC は書記素・結合文字について何も言っていない。「SPEC に合わせる」の実体は §4.2 の「全検索モード・フォルダ展開共通」を介した**検索側との一致**である。判定（バグ）は変えない
  - NFC の needle `が` × NFD の名前 `か\u{3099}`（macOS 由来）は修正の前後とも `None`（主が再測定）。今回の issue が狙う形より実際には多いかもしれないが、正規化（NFC/NFD）の問題でありこの差分の射程外
  - probe は `folder::filter_sorted` 自身を呼んでいない——計画の Red テストが実関数で測る

## 未解決の疑問 → 判断

- 置き場: (a) `scoring.rs` に Fuzzy の照合を包む関数を 1 つ置き `pub(crate)` で出す / (b) `utf32_haystack` だけを `pub(crate)` にし `Matcher` は folder 側で遅延生成する / (c) `query.rs` へ移す
  - **(a) を採る**。検索の Fuzzy の腕とフォルダの Fuzzy の腕が同じ関数を呼べば、規則（haystack の組み方）と照合器の借用が 1 か所に閉じる。既存契約「借用は Fuzzy の腕の中に閉じる」もそのまま保てる。(b) は規則を共有しても照合器の置き場が 2 つ残る。(c) は照合器を持たない正規化モジュールへ nucleo の照合の知識を持ち込む
- `to_lower_folded(name)` の候補ごとの `String` を使い回すか
  - **落とす**。新しい `_into` API が要るうえ額は未測定で、issue も「直すなら」としか言っていない
