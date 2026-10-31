# 計画 — #1327 snotra-core/CLAUDE.md を、編集の瞬間に要る不変条件だけに縮める

## 目的

`snotra-core/CLAUDE.md`（31,100 字・太字 157）を、編集の瞬間に守る指示だけの文書（約 9,000 字）にする。理由・経緯・測定値は正本（`//!` / `///` / `docs/adr/` / `PERFORMANCE.md`）へ寄せ、ここからは指すだけにする。草案は `workspace/draft.md`、条項の分類は `workspace/classify.md`。

## 受け入れ条件

1. `snotra-core/CLAUDE.md` が草案（下の「草案への修正」を当てたもの）になる
2. 外から正準形で参照されている見出しが、すべて同じ字面で残る（`governance:check` の G-heading-refs が緑）
3. 正本がこの文書にしか無かった内容は、消す前に退去先へ移っている（下の「退去」）
4. 見出しは残るが中身が消える参照元・行き先を失う参照元が、すべて直っている（下の「参照元」）
5. 過去に実害が出た規則（並列の列の同期・`normalize_entry_key` の冪等性契約・index.bin の版変更チェックリスト〔直前の版を受け取る側の数え直しを含む〕・`INDEX_WRITE_LOCK`・Config の読み込み失敗の扱い分け・`HistoryStore::load()` をテストで使わない）が残る
6. 今も偽の記述（「Win32 非依存」・「同じ型で渡すと `None` どうしの一致でフラグが立つ」・`reset_to_default()` の古い規則・「以上 `#[ignore]`」・「入口はそれぞれ 1 つ」）が新版に無い
7. 製品の挙動は変わらない（`.rs` の変更はコメントだけ）

## 退去（CLAUDE.md を書き換える前に行う）

| # | 内容 | 退去先 |
|---|---|---|
| R2 | `scan_all` の退行の射程: `Dedup::accept` の照合枝で確保が戻る退行は走査結果が同じなので CI では捕まらず、`tests/memory_footprint.rs`（`#[ignore]`）の確保回数だけが見る | `snotra-core/src/indexer/scan.rs` の `Dedup::accept` の doc |
| R3 | 計器の罠: PowerShell 7 の `Get-Process` は `ReadOperationCount` を持たず空値が 0 として混ざる／`read_dir` は `OtherOperationCount` 側に立つ／走査の進行は `TotalProcessorTime` と `Threads.Count` で測る／「計器が黙っている ≠ 対象が止まっている」 | `PERFORMANCE.md`「この文書へ記録するときの規約」の節 |
| R4 | 形式昇格は読みがロックの外にあり、内容を決めてから保存するまでの窓が開きうるが、製品で通る呼び出し元が起動段の 1 つだけなので今は開かない（ロード後に走りうる書き手を足すと戻る）。プロセスをまたぐ同時起動はこのロックでは守れない（プロセス大域の `static`） | `snotra-core/src/indexer/cache.rs` の `INDEX_WRITE_LOCK` の doc |

## 草案への修正（`workspace/draft.md` を新版にする前に当てる）

- 冒頭（M-1 の前提・U-1）: 「Win32 を呼ぶのは 3 ファイルだけ」の列挙をやめ、「UI 非依存の純ロジック lib crate。Win32 を使うモジュールもある（表示文字列を持たない理由は UI 非依存であること）」の形にする
- モジュール構成（U-2）: 計器・検知器の行は「`#[ignore]` か CI で走るかは各 `//!`」とし、ファイル名だけを並べる。`tests/dir_stat_cost.rs` を足す
- 冪等性契約（U-3）: 「入口はそれぞれ 1 つ」を「記録側と照合側は同じ関数を通す。別実装を足さない」の規範文にし、パスマッチの系統（`path_query`・`PathStore`）は別の正規化であることを 1 句で明記する
- 実装前チェック: 「派生文字列の潰しはビットマスク確定の後」の 1 行は残し、理由の正本として `indexer/columns.rs` の `derive_entry_lowers` の doc（検知器 `derived_masks_come_from_the_uncollapsed_strings`）を指す（退去は要らない。独立導出で判明）
- 冪等性契約（独立導出）: 正規化の手順（`trim` が抜けていた）と依存するモジュールの列挙（「3 モジュール」は足りない）を書かず、規則の定義は `normalize_entry_key_into` の doc、依存先は `git grep normalize_entry_key` で数える、と指す
- `scan_all` の節（R2）: 退行の射程は `Dedup::accept` の doc を指す 1 文にする
- `INDEX_WRITE_LOCK` の節（M-2・R4）: 窓の説明の出所を `LoadCacheResult::upgrade_save` の doc ではなく `INDEX_WRITE_LOCK` の doc にする
- 索引更新の契機（N-2）: 「現状 v7」を消し、ADR を指す
- Config のデシリアライズ経路（M-1）: `reset_to_default()` の文を「既定へ戻すときは `Config::normalized_default()` を使う」にする

## 参照元（見出しは残るが中身が消える・行き先を失う）

| # | 参照元 | 直し方 |
|---|---|---|
| P1 | `snotra-core/src/indexer/cache.rs` の `LoadOrScanStats` の doc（「反復 6 で実際に踏んだ経緯は `snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」が持つ」） | `PERFORMANCE.md`「採用: 背景再スキャンの比較を digest へ（ロード 395 → 312 ms・反復 6）」を指す |
| P2 | `snotra-core/src/str_arena.rs`（「`snotra-core/CLAUDE.md` が「`is_folder` から推論してはならない」で戒めた」） | `query::measure_derived_sharing` の doc を指す |
| P3 | `PERFORMANCE.md`（「IndexCache バージョン変更チェックリスト」の **8 項目**） | 数を消し、見出しだけを指す |
| P4 | `src-tauri/src/config_watcher.rs`（「`Config::load` の『一時的失敗は退避も上書きもしない』保全方針（snotra-core/CLAUDE.md）」） | `config/io.rs` の `LoadOutcome` / `load_from_dir_reporting` の doc を指す |
| P5 | `snotra-core/src/indexer/cache.rs`（「(#1013 の Gotcha)」） | `snotra-core/CLAUDE.md`「開発ルール」を指す |
| P6 | `snotra-core/tests/memory_footprint.rs`（昇格の機序の出所を「index.bin 書き込みの排他」としている。見出しの取り違え） | `snotra-core/CLAUDE.md`「indexer.rs の索引更新の契機」を指す |
| P7 | `snotra-core/src/indexer/keys.rs` の `//!`（「小文字化 + `/` → `\`」「依存する 3 モジュールの一覧は CLAUDE.md」） | 手順に `trim` を足す。依存先の一覧を CLAUDE.md が持つとは書かない |

## コードの側の同じ偽（独立導出で判明・主が確認）

`search/build.rs` の `DerivedStrings` の doc は「`None` どうしの一致でフラグが立つ危険は、判定を一元化する前に固有のものだった」と否定している。同じ機序を理由に書いているコメントを、「測り直しは無駄（理由は `DerivedStrings` の doc）」の形へ直す。

| # | 箇所 |
|---|---|
| F1 | `snotra-core/src/indexer/cache.rs`（現行版の枝の「`Collapsed` で渡す」のコメント） |
| F2 | `snotra-core/src/indexer/cache/tests.rs`（v6 の凍結バイト列のテストの doc「`Raw` で返すと…フラグが立つ」） |
| F3 | `snotra-core/src/search/build.rs`（`new_with_cached_masks` 付近の「ここで測り直すと…誤ったフラグが立つ」） |

## 実装順序

1. 退去 R2〜R4（コードの doc と `PERFORMANCE.md`）とコードの側の偽 F1〜F3
2. 草案へ修正を当て、`snotra-core/CLAUDE.md` を置き換える
3. 参照元 P1〜P7
4. 検算: 旧版と新版のバッククォート語・issue 番号・ADR 名の集合差を取り、落ちた語ごとに「正本がある／撤去済み／退去した」を確かめる。消した見出しと語彙（`Gotcha`・「内部キー形式」・「hidden/system」・`query.rs` の正規化と `Cow<str>`）を `git grep` で数え、在る前提で書いている出現を直す

## 不変条件と異常系

- 正準形の見出し参照はすべて着地する（G-heading-refs）。G-heading-refs が見ない形（散文形・中身の消滅）は手の一覧と `git grep` で見る
- G-module-index: snotra-core の `.rs` の basename がモジュール構成節に全部残る
- `.rs` の変更はコメントだけ（`cargo build` の結果が変わらない）。doc コメントを触るので `cargo doc` を通す

## テスト方針と検証

- 製品の挙動は変わらないのでテストは足さない
- 検証カテゴリ: A（`.rs` のコメント・`cargo doc`）・E・F（`governance:check`）。C・D は該当しない
- 委譲（`/implement` 3b）には、旧版の各条項が新版のどこに着地したか（または正本・退去先・削除理由）を、旧版を正本にして独立に照合させる

## フェーズ 1 — 退去

- [x] R2 `Dedup::accept` の doc
- [x] R3 `PERFORMANCE.md`「この文書へ記録するときの規約」
- [x] R4 `INDEX_WRITE_LOCK` の doc
- [x] F1〜F3 コードの側の同じ偽

## フェーズ 2 — 書き換え

- [x] 草案へ修正を当て、`snotra-core/CLAUDE.md` を置き換える
- [x] 参照元 P1〜P7 を直す
- [x] 検算（集合差・消した語彙の `git grep`・`governance:check`・`cargo doc`） — 集合差で落ちた識別子 253 語は、正本がコード側・退去先・撤去済みのいずれか（敵対的調査が確かめていなかった hidden/system の単一定義は `indexer/scan.rs` の `is_hidden_or_system` の doc にあることを主が確認）。消した見出しの語彙は凍結層の外で 0 件。`governance:check` 全 17 件 passed・`cargo doc` 警告 0・fmt 差分なし

## plan-review 結果

- リスク: 高（ガバナンス文書の圧縮・`--deep`）
- レビュー方式: 独立導出 1 体（Step 2b。全文は `workspace/plan-review-1327-derive.md`）
- エージェント数: 1（ほかに敵対的調査 1 体）

### 要対処（計画を修正した・主が一次資料で再照合済み）
- 導出 ∖ plan: コードの側の同じ偽 3 か所（F1〜F3）——`DerivedStrings` の doc（`search/build.rs`）が否定した機序を、`cache.rs`・`cache/tests.rs`・`search/build.rs` のコメントが理由に書いている
- 導出 ∖ plan: 参照元 P6（`memory_footprint.rs` の見出しの取り違え）・P7（`keys.rs` の `//!` の手順の欠け〔`normalize_entry_key_into` は `trim` する〕と依存先の数え上げ）
- plan ∖ 導出: 退去 R1 は不要——`indexer/columns.rs` の `derive_entry_lowers` の doc が既に正本で、検知器 `derived_masks_come_from_the_uncollapsed_strings` もある。計画から外した
- 草案の冪等性契約の節は、手順と依存先を列挙しない形にする（列挙が既に偽だった）

### 軽微（採らなかった・理由つき）
- `.claude/skills/persistence-check/SKILL.md` が「データ永続化の注意」をほぼ全文持ち、「SSOT は CLAUDE.md」と言いながら SKILL 側のほうが詳しい——どちらを正本にするかは、この issue の範囲（`snotra-core/CLAUDE.md` を縮める）の外。別 issue に起こす
- `SPEC.md` の 2 行（パスキー・クエリキーの正規化の内訳の正本をこの文書の見出しとする）——見出しは残り、そこから関数の doc を指すので鎖として着地する。SPEC は変えない
- `LoadCacheResult.version` を「昇格判定の唯一の入力」とする現行の記述（チェックリスト (b)）——草案はその句を持たないので、新版で消える
- `cache.rs` の他の参照（「index.bin 書き込みの排他」へ 2 か所・「索引更新の契機」へ 1 か所）・`columns.rs`・`config/io.rs` の参照——指す見出しも中身も新版に残るので直さない

### 未検証
- なし（草案を本番の位置に置いた複製での `governance:check` は敵対的調査が実施し緑。修正後の版は実装時に本物で回す）

### 判断
- 実装着手: 可（2026-10-31 承認）

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー1体
- エージェント数: 1（ほかに敵対的調査 1 体）
- 要対処: 4 件を計画へ反映
- 未検証: なし
- 散文の全件列挙: 語 3 / 総件数 137 行 / ★未掲載 56（計画へ足した 0。未掲載は残る見出しを指す参照・別文脈の同語・凍結層）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 1（退去 R1 を外した） / 主の見落とし 3（コードの側の同じ偽・見出しの取り違え・正規化の手順の欠け）

## 検証とレビュー（/implement Step 3）

- [x] 1 巡目（a4f2ac22）: 道具 green（A・E・F）。委譲は旧版を正本に規則の着地を照合し、失われた規則は Low 3（うち 2 を戻した・1 は不要と判断）。偽の文 Medium 2（`compute_wave1/2` を全コンストラクタが共有するという持ち越しの偽・`path_store.rs` がパスキーの規則を再実装していることの欠落）・Low 6（うち 2 件は前巡で主が直したコメント F1・F2 が別の偽の機序だった——`Raw` は型が違い変換なしに入らない）。全件対処。計器の検算: 参照された見出しの改名と `src/` の basename の欠落は赤、参照されていない見出しの改名と `tests/*.rs` の索引からの欠落は緑（計器の射程の外・記録のみ）。報告は `workspace/review-1327-r1.md`・`verify-1327-r1.json`

## 未確定（実装前に潰す）

- [x] **反復 6 の経緯の置き場** — `PERFORMANCE.md`「採用: 背景再スキャンの比較を digest へ（ロード 395 → 312 ms・反復 6）」に既在（`PERFORMANCE.md:2161`）。退去は要らず、P1 で指し先を替える
- [x] **v7 の昇格禁止の本文をコードへ移すか** — 移さない。`indexer/cache.rs` と `ADR-index-v7-cache-miss` がこの文書を規範として引いており、編集の瞬間に要る禁止なので K で残す（版の番号は書かない）
- [x] **PS7 の計器の罠の置き場** — `PERFORMANCE.md`「この文書へ記録するときの規約」。計測の記録規約の隣が、計器を使う人の目に入る位置である（`docs/development-principles.md` は局面 → 問いの形で書く文書で、具体の計器の罠は置かない）

## 人間レビュー

- [x] 承認済み — 2026-10-31 / 問い: "この計画を承認いただけますか？" / 回答: "ok"
