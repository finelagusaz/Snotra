# research — issue #985: `temp_dir` テストヘルパーのプロセス一意化（残り 5 件）

## issue の要約

`#[cfg(test)]` のテストヘルパー `temp_dir(tag)` が作業ディレクトリ名を `tag` だけで決めているため、`%TEMP%\<prefix>_<tag>` がプロセス間で一意でない。2 つのテストプロセスが同じ名前を狙うと、片方の `remove_dir_all` がもう片方の `create_dir_all` やファイル書き込みへ割り込み、コード変更と無関係な panic（赤）になる。`INDEX_WRITE_LOCK` のようなプロセス内の `static Mutex` は `%TEMP%` がプロセスをまたぐ共有資源である以上、射程が合わず効かない。

PR #982（issue #978）が `indexer.rs` の 1 件だけを直した。残る 5 件を同じ形へ揃えるのが本 issue。撤去条件は「上表の 5 件すべてがプロセス一意になったら閉じる」。

## 母集団（除外句なしで取り直した全数）

`git grep -n "env::temp_dir" -- '*.rs'`（除外なし）と `git grep -n "temp_dir(" -- '*.rs'` の 2 経路で取り、**11 hit すべてを 1 行ずつ判定した**。`use std::env` は 0 hit（`temp_dir` を裸で呼ぶ形は存在しない）。`tempfile` crate・`TMPDIR` / `"TEMP"` の直読みも 0 hit。

| # | 所在 | 現在の名前 | 判定 |
|---|---|---|---|
| 1 | `snotra-core/src/binfmt.rs:223` | `snotra_binfmt_test_{tag}` | **要修正**（issue 表） |
| 2 | `snotra-core/src/config.rs:3222` | `snotra_config_test_{tag}` | **要修正**（issue 表） |
| 3 | `snotra-core/src/folder.rs:243` | `snotra_test_{tag}` | **要修正**（issue 表・`temp_dir_with_contents`） |
| 4 | `snotra-core/src/history.rs:427` | `snotra_hist_test_{tag}` | **要修正**（issue 表） |
| 5 | `snotra-core/src/window_data.rs:105` | `snotra_window_test_{tag}` | **要修正**（issue 表） |
| 6 | `snotra-core/src/folder.rs:356` | `snotra_test_nonexistent_zzz` | **issue 表の外・要判断**（下記） |
| 7 | `snotra-core/src/indexer.rs:2436` | `snotra_idx_test_{tag}-{pid}` | 修正済み（#982・先例） |
| 8 | `snotra-core/src/config.rs:3824` | `snotra-dedup-{pid}` | 既に pid あり（先例） |
| 9 | `snotra-core/tests/search_frame_cost.rs:75` | `snotra-search-frame-cost-{pid}-unused` | 既に pid あり（先例・**実物を読んで確認済み**） |
| 10 | `src-tauri/src/icon.rs:501` | `snotra_icon_522_{pid}` | 既に pid あり（区切りは `_`） |
| 11 | `src-tauri/src/icon.rs:562` | `snotra_icon_522_det_{pid}` | 既に pid あり（区切りは `_`） |

**#6 は issue の表に無い。** ヘルパー経由でなく直書きで、`list_folder_nonexistent_dir_returns_empty` が「存在しないディレクトリ」を渡すために使う。issue が挙げた「別プロセスの `remove_dir_all` に割り込まれる」機序は当たらない（作らないため）が、**共有 `%TEMP%` 上の固定名に「存在しない」ことを前提させている**点で同じ欠陥クラスに属する。この名前を作る主体はリポジトリ内に無いので現実の赤は観測されていない。issue の撤去条件は #1〜#5 で満たされるため、#6 を含めても含めなくても issue は閉じる。→ 計画で明示的に扱う（勝手に射程を広げない）。

**タグの衝突（プロセス内）は無い。** 各モジュールの `temp_dir(...)` 呼び出しのタグを全列挙し、モジュール内で重複するタグが無いことを確認した（binfmt 14 / config **12** / folder 11 / history 8 / window_data 2、いずれも相異なる）。よってプロセス内の並行実行（`cargo test` のスレッド並列）での衝突は現状無く、本件は**プロセス間**の問題に閉じる。

> **リテラル一致の grep は動的生成のタグを系統的に落とす。二度踏んだ。**
>
> 1. **config**（3b の所見・`config.rs:3349` を自分で確認）: `load_from_dir_repairs_and_saves_invalid_hotkey` が `for (case, modifier, key) in [("unknown_modifier", ...), ("unsupported_key", ...), ("semantic_conflict", ...)]` のループ内で `temp_dir(case)` を呼ぶ。リテラル 9 + 動的 3 = **12**。
> 2. **folder**（Step 2b の独立導出の所見・`folder.rs:460` / `:532` を自分で確認）: `bench_folder_search` が `format!("bench_folder_{}_{}", label, n)`（label 2 種 × n 3 種 = 6 展開）、`bench_folder_topk_sort` が `format!("bench_topk_{n}")`（3 展開）。リテラル 11 + 動的 9 = **20**。どちらも `#[ignore]` だが `-- --ignored` を 2 プロセスで回せば同名を狙う。
>
> **結論（モジュール内で相異なる）は両方とも生き残る**（展開名は `bench_folder_folder_narrow_1000` 等でリテラル群とも検知器タグ `process_unique` とも衝突しない）。修正はヘルパー 1 か所なので**作業量は変わらない**が、「タグを列挙して重複なしと言い切る」根拠の作り方が誤っていた。数ではなく**ヘルパーが 1 か所であること**を根拠にすべきだった（`AGENTS.md`「数え上げも同じ強さである——数ではなく正本を指す」）。

**接頭辞関係では巻き添えは起きない。** `remove_dir_all` は完全パスに対して働くため、`snotra_test_basic` と `snotra_test_basic_x` のような接頭辞関係は無害である。危険なのは完全一致だけ（3b でも独立に同じ結論）。

## 先例と区切り文字の実態

pid を含む既存 5 例のうち、区切りは `-` が 3 例（#7・#8・#9）、`_` が 2 例（#10・#11）で**リポジトリ全体では既に混在している**。したがって本 issue で達成できる一貫性は「リポジトリ全体の統一」ではなく、**`snotra-core/src` の 6 ヘルパー（#1〜#5 + #7）が同じ形になること**である。

- #7（`indexer.rs`）は `snotra_idx_test_{tag}-{pid}` で、**pid の前だけ `-`** にして境界を見せている
- #7 には検知器 `temp_dir_name_contains_process_id`（`indexer.rs:2579`）が付いており、完全一致で名前を pin している

## 再利用できる既存パターン

1. **名前の作り方**: `format!("{prefix}_{tag}-{pid}", pid = std::process::id())` — #7 の逐語形。
2. **検知器**: `indexer.rs:2579` の `temp_dir_name_contains_process_id`。`dir.file_name()` を取り、`format!` で組んだ期待値と `assert_eq!` する 12 行。モジュールごとに prefix が違うので DRY 圧縮の対象にならない。
3. **doc コメント**: #7 のヘルパーに 9 行の理由説明（`INDEX_WRITE_LOCK` が効かない理由・pid を落とすと何が起きるか）がある。**これは正本であり、5 か所へ写さない**（`AGENTS.md`「文書に事実の写しを増やす変更」）。

## 技術的制約

- 5 ヘルパーはすべて `#[cfg(test)] mod tests` 内のモジュール私的関数。共有ヘルパーへ寄せるには `pub(crate)` な `#[cfg(test)]` モジュールを新設する必要があり、prefix がモジュールごとに違うため引数が 1 つ増える。
- 片付け作法はモジュールで揃っていない（`binfmt` / `window_data` / `indexer` は末尾で `remove_dir_all`、`config` / `history` / `folder` は呼ばない箇所がある）。共有化は「置き場所」に加えてこの差の整理も要求する → issue が「確かめてから決める」とした点。
- 挙動変更は無い（テスト専用コード）。`SPEC.md` はテストヘルパーの命名を規定しておらず、更新不要。
- 規範文書に `temp_dir` の命名規約は無い（`git grep "temp_dir" -- '*.md'` の hit はすべて `docs/superpowers/plans/` 配下の過去の計画書＝履歴であり、規範ではない）。よって規約の写しを直す必要は無い。

## 未解決の疑問（計画で決める）

1. 区切り文字を `-`（#7 に合わせる）と `_`（既存 prefix の系統）のどちらへ倒すか。
2. 共有ヘルパーへ寄せるか。
3. 検知器を 5 モジュールそれぞれに置くか、置かないか。
4. 母集団 #6（`folder.rs:356` の直書き固定名）を射程に入れるか。

## 欠陥の実測（自分で測った一次証拠）

`cargo test -p snotra-core --no-run` でテストバイナリを建て、**同じバイナリを 2 プロセス同時に**起動して赤が出るかを測った（`cargo test` を 2 本打つとビルドロックで直列化されるため、バイナリを直接叩く形にした）。

| 側 | 対象 | 条件 | 結果 |
|---|---|---|---|
| A | `binfmt::`（pid **なし**・21 テスト） | 2 プロセス並行 × 13 ラウンド、`--test-threads=4` | **1 ラウンドで赤**（`exit=(0,101)` / `20 passed; 1 failed`）。残り 12 ラウンドは緑 |
| B | `indexer::`（pid **あり**・60 テスト） | 同上 × 13 ラウンド | **13 ラウンドとも緑** |

- **欠陥は実在する**——A 側で、コード変更と無関係な赤を直接観測した。
- **発火率は条件に強く依存する。** 最初の 3 ラウンド（コールドに近い状態）で 1 件出たあと、続けて打った 10 ラウンド（ウォーム）では 0 件だった。binfmt の 21 テストは 0.02〜0.07 秒で終わるため衝突の窓が極端に狭い。**「緑だったこと」は欠陥の不在の証拠にならない**（`AGENTS.md`「検証の作法」の「不在の観測 1 つで確定させない」）。
- 数え上げの集計ロジック自体は `false`/`true` を流す sanity check（期待 2 / 実測 2）で検算済み。0/10 は集計の壊れではない。

> **3b は同じ実験で「10/10 回赤」と報告した。自分の実測（13 ラウンド中 1 回）と桁が違う。** 条件（テストスレッド数・ファイルシステムのウォーム度）の差と見るが、**裁定していない**。どちらの数も計画の判断を変えない（欠陥の実在と修正の妥当性は A 側 1 件の観測だけで足りる）ので、**率は research にも PR にも書かない**——下限主張（「並行実行で赤が出ることがある」）だけを主張する。

## 規範文書との関係（3b の ⚠️ 所見を受けて自分で読み直した）

- `snotra-core/CLAUDE.md`「Gotcha（計測の罠）」に、**同じ欠陥クラス**（テストがプロセス大域の共有資源を汚す）の既存規範がある: 直し方は **dir 注入の入口**（`load_cache_in` / `save_cache_sorted_in` のように `dir: &Path` を取る形）へ寄せること、`SNOTRA_CONFIG_DIR` での迂回は**禁止**（プロセス大域の env ゆえ並列実行中の他テストの保存先まで動かす）。
  - **判定**: 本件の 5 ヘルパーは**すでにこの規範に従った形**である（`&Path` を作って `*_in` 系へ渡す）。規範が言うのは「どこへ書くか」であって「ヘルパーをどこに置くか」ではないので、**共有ヘルパーへ寄せるかの判断は動かさない**。3b は「設計判断に直結する precedent」と述べたが、その機序は採らない（所見＝引用漏れは採り、機序＝判断を変えるという主張は却下）。
- `snotra-core/CLAUDE.md` には「**プロセスをまたぐ同時起動は世代機構でも守れていなかった——世代は `INDEX_WRITE_LOCK` と同じくプロセス大域の `static` であり、射程が同じ**」という記述があり、本 issue の機序（射程の不一致）と同型の判断が既に製品側で記録されている。
- ただし**`temp_dir` の命名規約そのものを持つ規範文書は無い**（探し方を `temp_dir` 以外へ広げても、テストヘルパーの命名を規定する節は見つからなかった）。→ 文書更新は不要という結論は維持する。

## 敵対的調査（3b）の所見と採否

出力: `workspace/adversarial-985.txt`。サブエージェント 1 体（general-purpose / sonnet）。

### 壊せた項目

| # | 所見 | 採否 |
|---|---|---|
| 1 | `config.rs` のタグ数「9」は誤り。ループで動的生成される 3 タグを見落としており実際は 12 | **採用**（`config.rs:3349` を自分で読んで確認）。結論「相異なる」は生き残る。数え方の穴として本文へ記録 |
| 2 | 「pid を足すだけで欠陥は消える」は全称として過大。pid 付きの `indexer` 由来ディレクトリが `%TEMP%` に 8/9〜8/12 付で 30 件以上残存しており、衝突は消えてもゴミの蓄積は消えない | **採用**（計画の「残余」節へ明記）。ただし**射程外**——残骸の蓄積は変更前後で同じであり、本 issue の撤去条件に含まれない |

### 壊せなかった項目

| # | 主張 | 検証のされ方 |
|---|---|---|
| 1 | 母集団 11 hit が `.rs` 全数である | 独立に 2 grep を取り直し完全一致。加えて `TempDir` 型 / `CARGO_TARGET_TMPDIR` / `env::var("TEMP"\|"TMP"\|"TMPDIR")` / `GetTempPath` / `dirs::` を個別検索、すべて 0 hit |
| 2 | binfmt 14 / folder 11 / history 8 / window_data 2、モジュール内で重複なし | 完全一致（config のみ上記のとおり訂正） |
| 3 | pid の先例は 5 件、区切りは `-` 3 / `_` 2 の混在 | 実物再読で完全一致 |
| 4 | `SPEC.md` にテスト規約なし・`.md` の hit はすべて `docs/superpowers/plans/` 配下 | 独立 grep で 0 hit 確認 |
| 5 | 欠陥の機序（プロセス間衝突で赤になる） | 2 プロセス同時実行で**再現**（`research.md` 自身が未実施だった点を突かれた）。→ 上の「欠陥の実測」で**自分でも測り直した** |

### ⚠️（確信の持てない所見）

| # | 所見 | 採否 |
|---|---|---|
| 1 | `snotra-core/CLAUDE.md`「Gotcha（計測の罠）」の同一欠陥クラスの規範を一度も引用していない（調査範囲の甘さ） | **所見は採用**（上の「規範文書との関係」で引用を補った）。**「共有ヘルパーの設計判断に直結する precedent」という機序は却下**——規範が規定するのは書き込み先であって置き場所ではない |
| 2 | CI は windows-latest の使い捨て VM でジョブごとに隔離されるため CI 同士の衝突は起きにくく、現実的なトリガーは同一開発機での並行実行（複数 worktree 等）である | **採用**（issue 自身の記述とも整合）。計画の判断は変わらない。self-hosted runner の有無は 3b も未確認 |

### 3b が未実施と申告した範囲

`folder.rs` / `config.rs` / `history.rs` / `window_data.rs` での並行再現実験（`binfmt.rs` からの類推）と、self-hosted runner の有無。**どちらも計画の判断を変えない**——修正は 5 件一律で、再現の有無で分岐する作業は無い。
