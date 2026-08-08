# #984 独立導出レビュー（`rebuild_and_save` → `drain_index` の CachedMasks 接続）

対象: `snotra-core::indexer::rebuild_and_save` が捨てている `CachedMasks` を、
`src-tauri::indexing::drain_index` が索引の表現としてそのまま使うようにする変更。
`workspace/plan.md` / `workspace/research.md` は読まず、コードと git 履歴のみから独立導出した。

## 要対処

1. **`rebuild_and_save` のシグネチャ変更が必要**
   `snotra-core/src/indexer.rs:900` `pub fn rebuild_and_save(...) -> IndexTree` は
   `save_cache_sorted(entries, config_hash).0`（`indexer.rs:907`）でタプルの `CachedMasks` 側
   を握りつぶしている。戻り値を `(IndexTree, Option<CachedMasks>)` へ広げ、`.0` を外す必要が
   ある。呼び出し元は `src-tauri/src/indexing.rs:102` の 1 箇所のみ（grep で確認、他に無し）。

2. **`rebuild_and_save` 直上の doc が変更後に偽になる**
   `snotra-core/src/indexer.rs:894-899`:
   > `保存が返す CachedMasks をここでは捨てている（意図的・受容する残余）。下流の
   > PrebuiltIndex::from_tree が木しか取らないため、この経路は今も Wave 1/2 を建て直す
   > （issue #984・繋ぎ方は PERFORMANCE.md「次の反復の候補」の該当行）。`
   この変更が入った瞬間、この段落は文字通り偽になる（捨てなくなる・#984 も閉じる）。書き換え
   必須。

3. **`PrebuiltIndex` に「マスク込みで建てる」コンストラクタが存在しない**
   `snotra-core/src/engine.rs` の `PrebuiltIndex` は `new`（`Vec<AppEntry>` から）と
   `from_tree`（`IndexTree` のみ、`SearchEngine::new_from_tree` を呼ぶ）の 2 本しか持たない
   （`engine.rs:49-61`）。`Engine::new_from_cache`（`engine.rs:142-156`、
   `SearchEngine::new_with_cached_masks` を呼ぶ）に相当する `PrebuiltIndex::from_cache(tree,
   masks, migemo_enabled)` が無いため、drain 経路はこれを新設しないと `CachedMasks` を渡す先が
   無い。
   - 併せて `from_tree` の doc（`engine.rs:53-54`）「**製品はこちらを通る。**」は、
     `from_cache` が主経路になった後は「masks が `None`（`Config::config_dir()` が引けない等）
     のときのフォールバック」に意味が変わるため書き換えが要る。

4. **`drain_index` に PATH マージ後の `extend_cached_masks` 呼び出しが無い（この変更の核心）**
   `src-tauri/src/indexing.rs:93-158` の現状:
   ```
   let mut tree = indexer::rebuild_and_save(&inputs.scan, inputs.show_hidden_system);
   if inputs.include_path_env {
       let path_entries = indexer::scan_path_env(&tree, inputs.show_hidden_system);
       tree.extend_with_roots(path_entries);
   }
   ...
   let new_index = snotra_core::engine::PrebuiltIndex::from_tree(tree, inputs.migemo_enabled);
   ```
   起動経路 `src-tauri/src/main.rs:193-204` は同じ状況で
   ```
   if config.search.include_path_env {
       let path_entries = indexer::scan_path_env(&tree, config.search.show_hidden_system);
       if !path_entries.is_empty() {
           if let Some(ref mut masks) = cached_masks {
               indexer::extend_cached_masks(masks, &path_entries);
           }
           tree.extend_with_roots(path_entries);
       }
   }
   ```
   としている。`drain_index` にはこの `extend_cached_masks` 呼び出しが**丸ごと存在しない**。
   `extend_cached_masks(masks: &mut CachedMasks, new_entries: &[AppEntry])`（借用）を
   `tree.extend_with_roots(path_entries: Vec<AppEntry>)`（move）より**先に**呼ぶ順序制約がある
   （`indexer.rs:1624`, `index_tree.rs:403`）。この呼び出しを足し忘れると `CachedMasks` の各
   Vec は元の長さのまま、`tree`/`entries` は PATH 分だけ伸びる。

5. **(4) の破損は debug_assert でしか捕まらず、release では黙って通る**
   `snotra-core/src/search/build.rs:184-187`（`derived.len() == tree.len()`）と
   `:260-266`（`char_masks.len() == entries.len()` 等）はどちらも `debug_assert!`。
   `Cargo.toml:37-42` の `[profile.release]` は `debug-assertions` を明示していないため release
   では両方とも消える。**実際に何が起きるか**まで追った: `Collapsed` 枝（v6/v7 で save 側が返す
   のは常にこれ）では `needs_measuring = false` になり `assemble` 内の測定ループも回らないため、
   `assemble` はそのまま短い `char_masks`/`file_name_char_masks`/`lower_names`/
   `lower_file_names` を `SearchEngine` へ格納する。実際の破綻は検索ホットパス
   `snotra-core/src/search/scoring.rs:331-332`
   （`let name_mask = self.char_masks[i];` / `let fn_mask = self.file_name_char_masks[i];`）
   で `i` が PATH 分の添字域に達したときの **Vec 添字外 panic** として出る（Vec の境界検査は
   debug_assert 非依存で常に効くため、release でもここは必ず panic → `panic = "abort"`
   （`Cargo.toml:42`）でプロセスごと落ちる）。つまり「ビルド時は無症状、`include_path_env=true`
   でスキャンした PATH が 1 件でもあるユーザーが drain 完了後に検索した瞬間クラッシュ」という
   形で現れる。**ビルド時は完全に沈黙する**という要求 3 の対象そのもの。

6. **上記の組み合わせを検知する仕組みが現状ゼロ**
   `snotra-core/src/search/tests/build.rs:247` の
   `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree` は
   コメント（同ファイル 288 行目）で明示されている通り「**B: 現行の起動経路**。順序も
   `main.rs` に合わせる」——`derive_columns → extend_cached_masks → extend_with_roots →
   new_with_cached_masks` の**起動経路の並び**だけを検証しており、`rebuild_and_save`/
   `drain_index` 側の並びは対象外。`src-tauri` 側には `tests/` ディレクトリが存在せず
   （`Glob src-tauri/tests/**/*.rs` = 0 件）、`drain_index` 自体を単体で検証する手段も無い。
   **この変更で新しく生きる組み合わせ（drain × PATH merge）に対する検知器が 1 つも無い**——
   これは issue #984 の本文自身が「実装時の注意」として名指ししている落とし穴と一致する
   （`gh issue view 984` 実測、下記引用）。

   > `drain_index` 側の PATH マージは、起動経路と違って今 `extend_cached_masks` を呼んで
   > いない。`from_tree` に留まっているので今は要らないだけで、繋いだ日に呼び忘れると
   > マスクだけが短くなる。`assemble` の長さ検証は `debug_assert` ゆえ release では消える。

   `PERFORMANCE.md:638-641` にも同種の警告が cache-miss 版として先に書かれている
   （「追記の呼び忘れは添字 panic か沈黙の食い違いになる」）。**新しい検知器（snotra-core
   レベルでの同種テスト、または両呼び出し元が共有する 1 関数へ括り出して構造的に呼び忘れを
   表現不能にする）を追加しないまま実装を終えると、このタスクの要求「検知手段が現に存在する
   か」に対して「無い」が答えになる。**

7. **`PERFORMANCE.md` の候補行・残余段落が、この変更の完了で自己矛盾する**
   - `PERFORMANCE.md:588`（「次の反復の候補」表の `PrebuiltIndex` を `CachedMasks` 込みで
     建てる行）
   - `PERFORMANCE.md:663-665`（「**残余（意図的）**: `rebuild_and_save` → `drain_index` の
     枝は `PrebuiltIndex::from_tree` のままで、返るマスクを捨てている」）
   同ファイル 582-584 行が自ら定める撤去条件「反復 10 以降でどれかを採ったらその行を『採用』
   節へ移し...どちらでもないまま残してはならない」に従えば、この 2 箇所は変更完了と同時に
   「採用」節（または不採用なら「試みたが機能しない手法」）へ移す義務がある。これは
   `AGENTS.md`「文書に事実の写しを増やす変更」トリガーが指す「正本を 1 か所に定め他は参照へ」
   の逆——ここでは「候補が実現したら候補表から除く」という、このファイル自身が課す規約。

8. **`snotra-core/CLAUDE.md:46` の「保護範囲の列挙」が変更後に不完全になる**
   同行は「両経路が同じ表現へ着地することを CI で守るのは
   `save_side_collapse_and_assemble_measurement_agree_at_entry_view` の 1 本だけである」
   「cache-miss の直後に PATH エントリを併合する経路は
   `path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree` が別に守る」と、
   現存する保護を名指しで列挙している。この変更で「drain 直後に PATH エントリを併合する経路」
   という**新しい組み合わせ**が生まれるが、上記 6 の通りそれを守る検知器は無い。読者はこの行を
   読むと「PATH 併合は守られている」と誤読する——数え上げ列挙が変更のたびに腐るという
   `AGENTS.md`「検証の作法」の警告が指す典型パターン。

## 軽微

1. `src-tauri/src/indexing.rs:41` のコメント「主な panic 発火点（rebuild_and_save /
   PrebuiltIndex::from_tree）はロック外で engine ロックを保持しないため poison しない」は、
   変更後に `PrebuiltIndex::from_cache` と `extend_cached_masks` も同じ性質を持つ panic 発火点
   に加わるので、列挙を更新した方がよい（catch_unwind の設計自体は変わらないため致命的ではな
   い）。

2. `src-tauri/CLAUDE.md`「`indexing.rs`」節の一文
   「ロック外で `rebuild_and_save` / `PrebuiltIndex::new`」は、**現行コードが実際には
   `PrebuiltIndex::from_tree` を呼んでいる**ため、この変更以前から既にシンボル名がずれている
   （`PrebuiltIndex::new` は製品コードから呼ばれない設計・`engine.rs:44-48` の doc 参照）。
   この変更でさらに `from_cache` が主経路に加わるため、ついでに直すとよい。

3. **DRY / 構造的リスク**: PATH マージ後に「`extend_cached_masks` を先に、`extend_with_roots`
   を後に」という順序制約を守る責務が、`main.rs`（既存）と `indexing.rs`（この変更で新設）の
   **2 箇所に複製される**ことになる。この issue そのものが「保存側の返り値を捨てて建て直す」
   という 2 箇所の重複の後始末であることを踏まえると、同じ順序制約を 2 箇所で「守る」規約に
   頼るのではなく、`snotra-core` 側に共有ヘルパー（例: `tree` と `Option<CachedMasks>` と
   `Vec<AppEntry>` を受けて内部で順序を固定する関数）を新設し、2 呼び出し元がそれを呼ぶだけに
   すれば、呼び忘れ・順序反転のクラスを構造的に表現不能にできる。要求では「どう作るか」は
   問われていないため要対処ではなく軽微に置くが、上記 6 の検知器が無い問題を最も安く解決する
   のがこの形である可能性が高い。

4. `docs/design/2026-05-31-coherence-staleset.md:135,224,235,257,264` は疑似コードで
   `PrebuiltIndex::new` を drain 経路のビルダーとして書いているが、実装は元々
   `PrebuiltIndex::from_tree` であり、この変更後は `from_cache`/`from_tree` の条件分岐になる。
   ただしこれは #347/#348-A 完了当時の設計記録であり、既にコードとシンボル名がずれている
   （この変更が原因ではない）。生きた仕様として同期すべき文書か、凍結された設計記録として
   対象外かは未確定（→ ⚠ 参照）。

## 未検証

1. 提案した「共有ヘルパーへ括り出す」形にした場合、`path_merge_after_cache_miss_agrees_with_deriving_over_the_extended_tree`
   と同型のテストを drain 側にも書けば実際に呼び忘れ変異を検出できるかは、自分で実装・実行して
   いないため確認できていない。メカニズム（`char_masks[i]` の添字 panic）はコード読解で追った
   のみで、実際に変異を注入して再現はしていない。
2. このブランチで `cargo test -p snotra-core` / `cargo test -p snotra` が現状 green かは実行して
   いない（禁止事項によりファイル変更はしていないが、テスト実行自体は読み取り専用のため許容
   範囅内だったかもしれない。時間の都合で見送った）。
3. `Config::config_dir()` が実運用の Windows 環境で `None` を返すケース（`rebuild_and_save` が
   `Option<CachedMasks>::None` を返す唯一の分岐、`indexer.rs:764-770`）がどの程度現実的かは
   確認していない。`PrebuiltIndex::from_tree` フォールバックが実際に踏まれる頻度は不明。

## ⚠ 確信が持てない所見

1. `docs/design/2026-05-31-coherence-staleset.md` が「凍結された設計記録」（ADR 相当・同期不要）
   なのか、`AGENTS.md`「ガバナンス文書」トリガーの対象として同期が要る「生きた文書」なのか、
   確信が持てない。前者だとしても、`PrebuiltIndex::new` という実在しない API 名を drain 経路の
   説明として使っている点は、この変更をきっかけに気づかれる形で表面化する可能性がある。
2. `snotra-core/src/indexer.rs:44-51` の `CachedMasks` の doc「出所は 2 つある...
   `save_cache_sorted_in` が書いたその足で返したもの（cache-miss・反復11）」という記述は、
   この変更後は「cache-miss」だけでなく「force-rebuild（`rebuild_and_save`/drain）」もこの
   出所の実例になる。文としては偽ではない（cache-miss は今も実例の 1 つ）が、括弧内の例示が
   唯一の実例であるかのように読める点が気になる——確信度が低いため軽微ではなくここに置く。
3. `PrebuiltIndex::from_cache` の想定シグネチャ（`tree: IndexTree, masks: CachedMasks,
   migemo_enabled: bool`）は `Engine::new_from_cache` との対称性から推測したもので、実装計画
   （読んでいない `workspace/plan.md`）が実際にどう設計するかは分からない。
4. 「masks 長さ不一致 → 添字 panic」という帰結は `scoring.rs:331-332` の 1 箇所を読んで追った
   ものであり、他の検索経路（フォルダ列挙・パスマッチ等）に `.get(i)` 等のガードがあって
   一部だけ「結果が静かに欠ける」false-negative 型の破損になる経路が別に存在する可能性を
   排除できていない。全経路を読み切ってはいない。
5. migemo 有効時、`kana_for_cached(&tree)` は拡張後の `tree`（PATH 込み）から**フルサイズ**の
   `kana_lower_names`/`kana_char_masks` を作る一方、`char_masks` 等は短いままになるため、
   `SearchEngine` 内の並列 Vec 群は「一部だけフルサイズ・一部だけ元サイズ」という不揃いな
   状態になるはずだと読解したが、実際にそのオブジェクトを構築して検証してはいない
   （静的読解のみ）。
