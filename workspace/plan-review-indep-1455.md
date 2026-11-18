# #1455 独立導出レビュー（網羅性）

対象: worktree `issues-1445-1457-c19d64`・ブランチ `perf/1455-parallel-scan`（HEAD = 98ef3f3a、main との差分コミット無し・`workspace/` のみ未追跡）。
`workspace/plan.md` / `research.md` / `adversarial-1455.txt` は読んでいない。コードと issue 本文だけから導出した。

---

## 0. 前提: 実運用点の構成の記述が 2 通りある（⚠️ 結論のいくつかがこれに依存する）

- **A（2026-08・#1002 の時点）**: `PERFORMANCE.md`「採用: `scan_all` の `seen` を根ごとの役割…」は 4 根（Start Menu Programs / Desktop / Dropbox / `C:\`）で、エントリは 311,700 件。`root_roles` / `Dedup::accept` の doc の「30 万件が照合のみ」もこちらに乗っている。つまり **`C:\` が約 30 万件を登録し、そのすべてで照合が走る**構成
- **B（2026-11-15）**: `docs/adr/ADR-scan-progress-unit-and-log-fields.md` は「最後の根 `C:\` が `extensions = []`・`include_folders = false` で 1 件も登録しない」「フォルダ 613,341 に対して登録は 23,869 件」
- issue 本文は「`C:\` 全体・約 31 万件」と書き、A に近い
- **どちらが今の実 `config.toml` かは確かめていない。** 下の落とし穴 4（照合バッファ）と 7（churn）、3.1 の「記録する根」の具体名は、構成によって結論が変わる。構成 A なら照合が 30 万回走るので、**ワーカーごとの照合バッファは実額に効く**（ジョブごとに確保すると 30 万回の確保が戻る＝#1002 の逆戻り）

## 1. 変更すべきファイルとシンボル

### 1.1 コード（製品）

| ファイル | シンボル | 何を変えるか |
|---|---|---|
| `snotra-core/src/indexer/scan.rs:133-166` | `scan_all` | 根のループは逐次のまま、根の内側を専用プールで並列に走査する。シグネチャ `(scan_paths, show_hidden_system, dirs) -> Vec<AppEntry>` は保つ（呼び手 4 か所、下の 1.3） |
| `scan.rs:169-240` | `scan_directory_with_extensions` | `&mut Vec<AppEntry>` / `&mut Dedup` を受ける再帰を、「1 ディレクトリを読んで、子ディレクトリを spawn し、採用したエントリを局所 Vec に返す」形へ割る |
| `scan.rs:87-124` | `Dedup` / `Dedup::accept` | `&mut self` で集合とバッファを一緒に持つ形は、ワーカー間で共有できない。(a) 根ごとに読み取り専用の `Option<&HashSet<String>>` と `RootRole`、(b) ワーカーごとの照合バッファ（`thread_local!` か `for_each_init` の類）、(c) 記録する根の「根が終わってから合流」する関数、の 3 つへ割る |
| `scan.rs:39-46, 70-83` | `RootRole` / `root_roles` | 述語は変えない。doc の「重複排除は先勝ちであり（[`Dedup::accept`]）」（scan.rs:57-58）は、根をまたぐ勝者（先の根が勝つ）では真のまま、**同じ根の中**では偽になる。書き分ける |
| 新設（scan.rs 内） | 専用プールの構築、スレッド数の定数（例 `SCAN_THREADS`）、記録する根の合流関数 | 定数は値の根拠を `PERFORMANCE.md` の節へ指す（`snotra-core/CLAUDE.md` 冒頭「理由・測定値は書かず正本を指す」） |
| ⚠️ 新ファイルにするなら | `indexer/scan/xxx.rs` | `//!` と `snotra-core/CLAUDE.md`「モジュール構成」の `indexer.rs と子 …` 行を足す（`governance:check` が照合する）。scan.rs に収まる規模なら不要 |

`snotra-core/Cargo.toml:15` に `rayon = "1"` は既にある。依存の追加は不要。

### 1.2 並列化で偽になる／古くなる散文（doc コメント）

| 位置 | 現在の記述 | 何が起きるか |
|---|---|---|
| `scan.rs:1-5`（`//!`） | 役割と正準整列のみ | 「根の内側は専用プールで並列・出力の順序は保証しない」を足す（責務の正本） |
| `scan.rs:57-58`（`root_roles` doc） | 「重複排除は先勝ちであり（`Dedup::accept`）」 | 同じ根の中は「決定的な規則（例: パスが小さい方）」へ替わるので偽になる |
| `scan.rs:85-94`（`Dedup` doc・`buf` フィールド doc） | 「集合・バッファ・根の役割を 1 つに束ねる」「確保を走査あたり 1 回に抑える」 | 構造が割れるので書き直し。バッファはワーカーごとになる（走査あたり 1 回ではなく、スレッドあたり 1 回） |
| `scan.rs:99-109`（`Dedup::accept` doc） | 「`record` が偽の根で `normalize_entry_key` を呼ばないことが本設計の全部」「`scan_all` の結線の退行も同じ」 | 不変条件は残る（照合だけの根で確保しない）。ただし実装の場所が変わるので、doc の所在を移す。`snotra-core/CLAUDE.md`「`scan_all` の重複排除」が「CI が守らない退行の適用範囲は `Dedup::accept` の doc」と名指ししているので、**名指し先を消すならこの CLAUDE.md の行も直す** |
| `scan.rs:128-132`（`scan_all` doc） | 「読み手は走査と並行して別スレッドから読むので、加算は … `Relaxed` の 1 回だけ」 | 依然として正しいが、書き手も複数スレッドになる。「単調に増えるだけ」という論拠は書き手が複数でも成り立つ。一言足す程度。**加えて「返り値の順序は保証しない。呼び手は `sort_entries_canonical` を通す」を書く**（下の 4.1） |
| `snotra-core/tests/memory_footprint.rs:507-510` | 「0 なら `seen` を建てない走査は、建てた走査と件数も**順序も**一致する」 | **順序**は並列化で偽になる（件数は真のまま） |
| `snotra-core/tests/memory_footprint.rs:458-483`（`report_scan_all_cost` doc） | 確保回数を見る区間 | 並列化で `scope.spawn` のジョブ（1 ディレクトリに 1 回の `Box`）と局所 Vec の確保が乗る。#1002 の確保の数字（`PERFORMANCE.md:2720`「確保 5,991,749 → 5,680,415」）と比べると**増えて見える**。#1002 の退行と誤読されないよう、増えた理由を記録する（下の 4.5） |
| `PERFORMANCE.md:2753`（#1002 の節「採用: `scan_all` の `seen` を根ごとの役割…」） | 「重複が 0 なら、旧実装は何も落としていない＝件数も**順序も**一致する」 | `memory_footprint.rs` と同じ主張の写し。並列化後に同じ受け入れ（重複件数 0）を使うなら、「順序」は成り立たない。#1002 の節は歴史として真（当時の実装について述べている）なので、直すより新しい節で「並列化後は順序を保証しない」と書くのが筋。⚠️ 判定は governance-docs.md の「歴史を述べて真のままの記述は残す」 |
| `PERFORMANCE.md:2418` | 「digest は順序に依存する。列挙順を吸収しているのは `sort_entries_canonical`」 | 並列化後も真（むしろ根拠になる）。変更不要 |
| `PERFORMANCE.md:41` | 「`TotalProcessorTime`（走査中は 1 コアを使い切る）と `Threads.Count`（作業スレッドの消滅）」 | 「1 コアを使い切る」は偽になる（最大 8 コア分）。`Threads.Count` の信号は、プールを走査ごとに建てて落とすなら残る。**常駐プール（`static`）にすると、この信号が消える**（判断 3.3 の根拠の 1 つ） |
| `PERFORMANCE.md:1289` / `snotra-core/src/search/build.rs:131-133` | 「`compute_wave1` の kana 分岐は逐次のまま残してある（通るのは cache-miss ＝ 走査 22〜30 秒の内側だけで…）」 | 「走査が 22〜30 秒だから kana の逐次の約 0.96 秒は相対的に無視できる」という比率の論拠。走査が 4.4 倍速くなると比率が変わる。⚠️ 論拠が崩れるかは額次第（数秒に対する 1 秒）。少なくとも「22〜30 秒」の写しは古くなる |
| 「22〜30 秒」の写し | `snotra-core/src/indexer/cache.rs:297`・`cache.rs:546`（「22〜85 秒」）・`cache/tests.rs:746`・`index_tree.rs:953`・`str_arena.rs:454`・`tests/path_query_cost.rs:364`・`PERFORMANCE.md:1907` | 所要の写し。並列化で古くなる。AGENTS.md「数え上げは正本を指す」に沿って、数を直すより正本（`PERFORMANCE.md` の新しい節）を指す形へ寄せるのが筋。⚠️ どこまで直すかは範囲の判断（全部直さなくても挙動は壊れない） |
| `docs/adr/ADR-rescan-explicit-only.md:7, 80`・`ADR-index-v7-cache-miss.md:13`・`ADR-cache-miss-startup-deferred-build.md:5` | 22〜30 秒・1 コア | ADR は書いた後は更新しない記録（AGENTS.md「ドキュメント参照」）。**直さない**。 |
| `docs/adr/ADR-mtime-differential-scan-ceiling.md:65` | 再判断の契機は「`/s` の 22〜30 秒が問題として立ったとき」 | この issue がまさにその契機に当たる。ADR 本体は直さないが、PR 本文か PERFORMANCE.md の新節で「この契機に対し、ハンドル相対の列挙ではなく並列を採った」と一言触れる価値がある。⚠️ 同行が指す `snotra-core/tests/dir_stat_cost.rs` は現存しない（`ls snotra-core/tests` に無い）。#1455 の範囲外だが、参照腐りとして気づいた |

### 1.3 呼び手（`git grep -n "scan_all"` の `*.rs` 全件）

| 呼び手 | 整列するか | 影響 |
|---|---|---|
| `snotra-core/src/indexer/cache.rs:407`（`scan_and_sort_timed`、`rebuild_and_save` と cache-miss 分岐の両方が通る） | する（cache.rs:411） | 影響なし。`index.bin` に届く唯一の経路 |
| `snotra-core/src/search/tests/common.rs:92`（`real_scanned_entries`） | する（common.rs:98） | 影響なし |
| `snotra-core/tests/memory_footprint.rs:489`（`#[ignore]`） | しない | 件数と重複件数しか見ないので結果は不変。doc の「順序も」が偽、確保回数が増える（上表） |
| `src-tauri/src/icon.rs:835`（`icon_pipeline_cost_probe`・`#[ignore]`） | **しない** | `.take(200)` で**整列していない先頭 200 件**を測る。並列化で**どの 200 件を測るかが実行ごとに変わる**。`sort_entries_canonical` は `pub(crate)` で src-tauri から呼べない（`memory_footprint.rs:475-477` が同じ制約を書いている）。`target_path` で `sort` してから `take` するか、非決定を受容して doc に書く |

`SPEC.md` は変えなくてよい: §進捗表示（SPEC.md:75）は「走査したフォルダの数」で、単位は保たれる。走査が並列かどうかは SPEC の語彙に現れない（`grep 走査 SPEC.md` を確認）。`docs/architecture.md` にもスレッドの一覧や走査の直列性の記述は無い（`grep` で確認）。⚠️ 「索引構築は専用プールで並列に走査し、グローバルプールを使わない」は横断パターン（グローバルプールとの分離）なので、architecture.md に 1 行置く判断はありうる。

### 1.4 計測の記録（受け入れ条件そのもの）

- `PERFORMANCE.md`: スパイクの結果（並列度ごとの所要・8 が最良 4.4 倍・16 で悪化）を**まだ記録していない**（`git grep 1455` は PERFORMANCE.md に当たらない）。issue の受け入れ「スパイクの結果を記録し採否を決める」は未達。新しい節（採用: …）を足し、スレッド数の定数の doc はここを指す
- A-B-A の `scan-log.jsonl`（`scan_ms / dirs`）と `index.bin` のバイト一致の結果も同じ節へ

---

## 2. 守るべき不変条件と検知するテスト

| # | 不変条件 | 既存の検知器 | 足すべき検知器 |
|---|---|---|---|
| I1 | 根は宣言順に処理し、根をまたぐ重複では**先の根が勝つ**（`target_path` の字面・`name`・その根の `extensions` / `include_folders`） | `scan_all_dedups_when_roots_are_nested` / `..._child_root_comes_first`（tests.rs:82, 160）——件数だけを見る | 勝った側が先の根のものであることを `target_path` の字面で見るテスト（例: 根 `X` と `x\SUB` のように字面を変えた 2 根で、残る `target_path` が先の根の綴りか）。現行のテストは件数 1 しか見ないので、後の根が勝つ退行を検出しない |
| I2 | `root_roles` の述語は不変 | `root_roles_*` 7 本（tests.rs:21-77） | 不要（述語に触らない） |
| I3 | 照合だけの根は集合へ書かず、エントリごとの `String` を確保しない（#1002） | CI には無い（`Dedup::accept` doc 自身が書いている）。`memory_footprint` の `#[ignore]` 計測だけ | CI で守るなら、照合だけの根の走査中に集合が変わらないことを型で保つ（`&HashSet` を渡す＝借用検査が守る）。確保の側は従来どおり手動計測。並列化で spawn の確保が乗るので、**比較の基準は「並列化後の値」に取り直す** |
| I4 | 同じ根の中の同一キーの勝者が決定的（スレッド数とスケジュールに依らない） | 無い（今は逐次ゆえ問題にならなかった） | (a) 合流関数を純関数に切り出し、合成したエントリ列を**入力順を変えて**与えても同じ勝者になる単体テスト（`snotra-core/CLAUDE.md`「top-k 更新を変えたら入力順を変えても結果が不変なテスト」と同型）。(b) ⚠️ 実ファイルで衝突を作る fixture: 末尾空白のフォルダ（`\\?\` 経由で `sub ` を作り、`include_folders = true` の記録する根で `sub` と衝突させる。`normalize_entry_key_into` は全体を `trim` する・keys.rs:30）か、NTFS の upcase 表で別名・`to_lowercase` で同じになる組（U+212A KELVIN SIGN と `k`）。どちらも実機で作れるかは未確認 |
| I5 | 出力（整列後）がスレッド数に依らず同じ | 無い | `scan_all` の内部に「スレッド数を受け取る私的な関数」を置き、1 本と N 本で**多数のディレクトリ**を持つ fixture を走査し、`sort_entries_canonical` 後の Vec が一致するテスト（`search/tests/ranking.rs:287` の「プールのスレッド数と入力順に依らず」と同型）。**fixture はスレッド数を超える数の兄弟ディレクトリと深さを持たせる**（`snotra-core/CLAUDE.md`「定数で分岐する実装では fixture がその定数を跨ぐ」）。逐次の閾値（浅い所だけ spawn する等）を置くなら、その閾値も跨ぐ |
| I6 | `dirs` は `read_dir` に成功したディレクトリの数（根を含む）で、登録件数と独立 | `scan_all_counts_every_directory_it_reads_even_when_it_registers_nothing`（tests.rs:118、期待値 6） | 既存で足りる。I5 の多ディレクトリ fixture でも `dirs` が 1 本と N 本で一致することを見ると、並列での加算漏れ（spawn した側で数え忘れる）を拾える |
| I7 | 検索・kana のグローバル rayon プールを使わない | 無い | ⚠️ テストで直接は測りにくい。`rayon::current_thread_index()` / プールの名前（`thread_name` を付けて走査中のスレッド名を観測する）で、走査の葉がグローバルプールで走っていないことを見る手はある。最低限、`rayon::scope` / `par_iter` を素のまま（`pool.install` の外で）呼ばない構造にしておく |
| I8 | 拡張子・隠し/システム・フォルダ名の導出（`file_stem` / `file_name`）は不変 | `scan_with_extensions_*` 4 本・`scan_keeps_same_name_different_paths`・`scan_extensions_case_insensitive`（tests.rs:190-360） | 既存。ただし**これらは `scan_directory_with_extensions` と `Dedup` の構造体リテラルを直接呼んでいる**（下の 4.1） |
| I9 | `index.bin` のバイト列が不変（同じファイルシステムなら） | `index_cache_on_disk_format_is_stable`（形式）——走査の結果は見ない | I5 で整列後の一致を CI で固定し、実機は A-B-A で確かめる |
| I10 | 読めないディレクトリは黙って飛ばす（中断しない） | `scan_all_counts_…`（存在しない根） | 既存。並列でも「失敗で他の枝を止めない」ことを保つ |

---

## 3. 設計上の判断と根拠

### 3.1 重複排除をどこで当てるか

- **根のあいだ**: 根は逐次（issue）。根 `i` の走査が終わるまで集合を変えない。照合だけの根（`check && !record`）では集合を `&HashSet<String>` としてワーカーへ共有し、各ワーカーが自分のバッファで照合する。借用検査がこの読み取り専用を保証する
- **同じ根の中（記録する根のみ）**: ワーカーは「先の根に既出か」の照合だけを行い（`check` が真なら）、採ったエントリとそのキーを局所に集める。根が終わったら合流し、**同一キーの中から決定的な勝者を 1 つ選び**、勝者のキーを集合へ入れる
- **同じ根の中の重複は記録する根でしか除かれない**（scan.rs:114-121: `(true,false)` は照合のみ、`(false,false)` は素通し）。この非対称は今も同じで、並列化で変えない。決定的な規則が要るのは記録する根の中だけ
- **勝者の規則**: `target_path` のバイト順で小さい方。同じ根の中で同一パスは二度現れない（ジャンクション/シンボリックリンクは `DirEntry::metadata` が辿らず `is_dir()` が偽——⚠️ std の Windows 実装の挙動として理解しているが、このリポジトリで実測した記録は見ていない）ので、`target_path` だけで全順序になる。`sort_entries_canonical`（scan.rs:288）の第 1 キーと同じなので、**比較子を写さずに同じ関数の順序と揃える**のがよい（同関数の doc「写しを書くと…」）。`name` / `is_folder` の第 2・第 3 キーは同一 `target_path` が無い以上効かない
- **今との差**: 今の勝者は「DFS で先に列挙した方」で、NTFS の列挙順（upcase 表による照合順）に従う。バイト順では `'A'(0x41) < 'a'(0x61)` なので、大文字小文字だけ違う兄弟では今と同じとは限らない。衝突は大文字小文字を区別するディレクトリ・末尾空白・Unicode の小文字化の非対称でしか起きず、実運用点の記録する根（構成 A では Start Menu Programs / Desktop / Dropbox。節 0）ではまず無い（⚠️ 実データで衝突 0 件かは `memory_footprint` の「正規化キーの重複」が全件について測るが、同じ根の中の衝突だけを数えているわけではない）。issue のとおり差を記録する（`root_roles` doc と PERFORMANCE.md の新節）
- ⚠️ 代案: 記録する根を逐次のまま走査し、**記録しない根**（`(true,false)` の照合だけの根と `(false,false)` の素通しの根）だけ並列にする。実運用点では所要の大半が最後の `C:\`（照合のみ）なので効果はほぼ同じで、勝者の規則を変えずに済む。**ただし構成（根の順序）に効果が依存する**（`root_roles` doc が「額は根の順序に依存する」と書く理屈と同じ）。単純さを取るなら有力。採否は計画の側で比べてほしい

### 3.2 出力の順序

- 返り値の順序は保証しない、と `scan_all` の doc に書く。`index.bin` に届く経路は `scan_and_sort_timed`（cache.rs:405-421）だけで、必ず `sort_entries_canonical` を通る。全順序（`target_path` → `name` → `is_folder`）なので、勝者が決定的なら整列後の Vec は同じになる
- 整列しない呼び手は `icon.rs:835` と `memory_footprint.rs:489` の 2 つ（1.3）

### 3.3 プールの寿命とスレッド数

- **寿命は `scan_all` 1 回ごと（建てて、終わったら落とす）を推す**。根拠: (1) 走査は `/s`・設定変更・cache-miss の時だけで、建てる費用（ミリ秒）は数秒の走査に対して無視できる。(2) 常駐させると 8 本のスレッドが idle のまま残り、常駐メモリを測っているこのリポジトリ（`memory_footprint`）の数字を動かす。(3) `PERFORMANCE.md:41` の「`Threads.Count`（作業スレッドの消滅）」という外からの観測信号が、常駐プールでは失われる
- **スレッド数**: 上限 8 の定数（スパイクの最良値）。⚠️ `available_parallelism()` との `min` を取るかは判断が要る。走査は CPU（カーネル内の開く処理）が支配的という issue の見立てなら、コア数を超えて立てても縮まず、UI スレッド（egui）や hotkey の応答を圧迫しうる。一方スパイクは 1 台の測定で、その機械のコア数がここからは分からない。最低でも「上限 8・下限 1」をコードで保ち、根拠は PERFORMANCE.md を指す
- プールにスレッド名（`thread_name`）を付ける。トレースと I7 の検知に使える
- 呼び出し側スレッドは `snotra-index-build`（`src-tauri/src/indexing.rs:65-66`）で、`INDEX_WRITE_LOCK` を持ったまま走査する（cache.rs:791）。プールのワーカーはこのロックに触れないので、デッドロックの経路は無い

### 3.4 失敗時の扱い

- `read_dir` / `metadata` の失敗: 今と同じく黙って飛ばす（I10）
- **プールの構築失敗**（`ThreadPoolBuilder::build` の `Err`、スレッドを作れない）: ⚠️ 判断が要る。選択肢は (a) 呼び出し側スレッドで逐次に落ちる（同じ「1 ディレクトリを処理する関数」を明示スタックで回せば写しを作らずに済む）、(b) `expect` で panic。release は `panic = "abort"`（indexing.rs:72-74 のコメント）なので (b) はプロセス終了になる。(a) を推す。**グローバルプールへ落ちる（`rayon::scope` を素で呼ぶ）のは要求違反なので不可**
- ワーカー内の panic: `rayon::scope` は panic を呼び出し側へ伝播する。release では abort、debug/test では `indexing.rs` の `catch_unwind` が受ける。今の逐次実装と同じ帰結になる
- `dirs` の最終値: `rebuild_and_save` は走査の後に `dirs.load(Relaxed)` する（cache.rs:808）。プールの `scope` の終了がワーカーの書き込みとの happens-before を作るので、`Relaxed` の読みでも全加算が見える

---

## 4. 見落としやすい落とし穴

1. **テストが内部関数と構造体リテラルを直接呼んでいる。** `scan/tests.rs` の 5 本（190-360 行）が `Dedup { set, buf, role: RootRole { .. } }` を組み、`scan_directory_with_extensions(&dir, &exts, …, &mut entries, &mut dedup, &AtomicUsize)` を直に呼ぶ。シグネチャを変えると compile-fail する（それ自体は良い検知）。直すときに**テストの意図（拡張子・フォルダ・同名別パス・大文字拡張子）を `scan_all` 経由で保つ**か、新しい「1 ディレクトリ処理」関数に合わせるか。後者なら再帰を辿らない形になり、`scan_keeps_same_name_different_paths`（`a\tool.exe` と `b\tool.exe`）は子ディレクトリへ降りる経路を通らなくなる——**テストが走っても見ていない**状態になりうる
2. **`icon_pipeline_cost_probe`（`src-tauri/src/icon.rs:835`・`#[ignore]`）は整列せずに先頭 200 件を採る。** 並列化で標本が実行ごとに変わり、`egui_shell/layout.rs` が根拠に引く測定値（同 doc）の再現性が落ちる。CI は走らないので誰も気づかない
3. **`memory_footprint.rs` の確保回数が増える。** `scope.spawn` は 1 ジョブにつき 1 回 `Box` を確保する（実運用点で約 61 万ディレクトリ・ADR-scan-progress-unit-and-log-fields.md:11 の 613,341）。局所 Vec も確保する。#1002 の「確保 5,991,749 → 5,680,415」（構成 A・`scan_all` 区間）と並べると増えて見え、#1002 の機構が壊れたと誤読される。基準を取り直し、PERFORMANCE.md に「並列化による増分」として記録する。⚠️ 増分の額は測っていない（構成も #1002 の時点と違いうるので、割合での比較はしない）
4. **照合だけの根のバッファ。** `accept` は登録候補（拡張子が合う・フォルダを含める）にしか呼ばれない。構成 B（節 0）なら `C:\` で照合は 1 回も走らず、バッファの持ち方の退行は実運用点の確保回数にすら映らない。**構成 A なら照合が約 30 万回走り、エントリごと・ジョブごとに確保する形は #1002 の削減を戻す。** 構成によらず、ワーカーごとの `thread_local!`（またはワーカー寿命のバッファ）にしておくのが安全
5. **スタックの深さ。** `rayon::join` や再帰的な `par_iter` で降りると、ワーカーのスタックに木の深さぶん積まれ、さらに work-stealing で待ちの上に別のジョブが積まれる。`scope.spawn` で 1 ディレクトリ 1 ジョブにすれば、ジョブはヒープに置かれ深さはスタックに乗らない。今の逐次実装は `snotra-index-build` スレッドの既定スタックで再帰している（`stack_size` の指定はリポジトリに無い・`git grep stack_size`）
6. **保留中のジョブの数。** spawn 方式は幅優先寄りになり、未処理のディレクトリのパスが一時的に大量に積まれる（逐次 DFS はハンドルを深さぶんしか開かない代わりに、パスも深さぶんしか持たない）。ピークメモリの一時的な増加。⚠️ 額は未測定
7. **`index.bin` のバイト一致を別時刻の 2 回の走査で比べると、churn で偽陽性が出る。** `memory_footprint.rs:478-481` と `PERFORMANCE.md` の #1002 の節が同じことを書いている（A と B は別プロセス・別時刻）。構成 B なら `C:\` は 0 件で churn は小さいが、構成 A なら `C:\` 全体の temp・ログが入るので churn は避けられない。どちらでも、確かめるなら同一プロセスで 1 本と 8 本を続けて走らせ整列後の Vec を比べるのが堅い
8. **`(true, true)` の根（先とも後とも重なる中間の根）。** 照合（先の根に対して）と記録（後の根のため）の両方をする。今は `set.insert` 1 回が両方を兼ねる（scan.rs:116）。並列化後は「ワーカーで読み取り専用の照合 → 合流で同じ根の中の重複を除いて集合へ挿入」の 2 段になる。照合を忘れると先の根との重複が通る。3 根の入れ子（`X`・`X\a`・`X\a\b`）で中間の根を通すテストが無い（既存は 2 根のみ）
9. **`needs_set == false`（重なる根が無い）では集合を建てない**（scan.rs:141）。並列化後もこの分岐を保つ。記録する根が無いのに合流のためにキーを作ると、全件の `String` 確保が戻る
10. **AGENTS.md のトリガー**: worker spawn・スレッドをまたぐ共有状態 → `/race-check`。プールの生成/破棄 → `/symmetric-check`。関数の新設・改名 → 呼び手の列挙（LSP）と `/dry-check`（比較子を写さない）。挙動を変えない最適化 → 代表的な入出力をベースラインに取る（AGENTS.md「事前調査」）
12. **`cache/tests.rs` の `a_cache_hit_startup_does_not_scan`（689 行付近）** は結果（材料に新ファイルが現れないか）と `stats.scan == ZERO` で測る。`dirs` もソーステキストも見ないので、並列化と干渉しない（読んで確認）
13. **検証**: doc を触るので `docs/build-commands.md` カテゴリ A の `cargo doc` を走らせる（`[`Dedup::accept`]` への intra-doc link は `scan.rs` の `//!` 4 行目と `root_roles` doc 58 行目にもあり、`Dedup` を割ると切れる）。`.md` を触るので `npm run governance:check`
11. **`snotra-core/CLAUDE.md`「`scan_all` の重複排除」** は「CI が守らない退行の適用範囲は `Dedup::accept` の doc」と名指しする。`Dedup::accept` を消す・改名するなら、この行も同じ変更で直す（散文の参照は識別子の検索で拾えても、意味のずれは拾えない）

---

## 導出したファイル・シンボルの一覧（要約）

- 製品: `snotra-core/src/indexer/scan.rs` — `scan_all`・`scan_directory_with_extensions`・`Dedup`（+`accept`）・`root_roles` の doc・`//!`・新設のプール構築/スレッド数定数/記録する根の合流関数
- テスト: `snotra-core/src/indexer/scan/tests.rs` — 内部直呼びの 5 本の追従、I1（勝者の字面）・I4（合流の決定性）・I5（スレッド数非依存）・3 根の入れ子のテストを新設
- 計測コード: `snotra-core/tests/memory_footprint.rs`（「順序も」の doc・確保の基準）、`src-tauri/src/icon.rs:835`（未整列の take(200)）
- 散文: `PERFORMANCE.md`（スパイクと A-B-A の新節・41 行の「1 コア」・1289 行）、`snotra-core/src/search/build.rs:131`、`snotra-core/CLAUDE.md`「`scan_all` の重複排除」、「22〜30 秒」の写し（cache.rs:297/546、cache/tests.rs:746、index_tree.rs:953、str_arena.rs:454、tests/path_query_cost.rs:364、PERFORMANCE.md:1907）。ADR は直さない
- 変えない: `SPEC.md`（進捗の単位は保たれる）、`src-tauri/src/state.rs` / `indexing.rs`（シグネチャ不変なら）、`cache.rs` の `scan_and_sort_timed` / `rebuild_and_save`
