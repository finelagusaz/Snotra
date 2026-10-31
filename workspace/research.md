# 調査 — #1327 snotra-core/CLAUDE.md を、編集の瞬間に要る不変条件だけに縮める

## issue の要約

`snotra-core/CLAUDE.md` は 31,100 字で、snotra-core のファイルを触ると丸ごと会話へ注入される。条項の分類（`workspace/classify.md`。2026-10-31 に委譲で作成し、正本とテスト名の実在は grep で確認済み）では、K（残す）は 28 条項・10,477 字で、残りは写し（D 30）・機構が守る（M 8）・経緯（H 3）・消してよい（X 6）である。K だけを短く書き直した草案が `workspace/draft.md`（9,138 字・太字 15）。ユーザー合意は 2026-10-31（「Aを起票して進めていい」）。

## 関連ファイル

- 対象: `snotra-core/CLAUDE.md`
- 外から正準形で指されている見出し: 15（一覧は `workspace/classify.md`「外から参照されている見出し」）。参照元は `.claude/rules/snotra-core.md`・`.claude/rules/snotra-core-search.md`・`.claude/rules/src-tauri.md`・`.claude/skills/persistence-check/SKILL.md`・`.claude/skills/race-check/SKILL.md`・`src-tauri/CLAUDE.md`・`SPEC.md`・`PERFORMANCE.md`・`.rs` のコメント（`indexer/keys.rs`・`indexer/cache.rs`・`config/tests.rs`・`config/migrate.rs`・`indexer/cache/tests.rs`・`tests/memory_footprint.rs`）
- 見出しは残るが中身が消える参照元: `snotra-core/src/indexer/cache.rs`（`LoadOrScanStats` の doc の「反復 6 の経緯」）・`snotra-core/src/str_arena.rs`（「`is_folder` から推論してはならない」）・`PERFORMANCE.md`（チェックリストを「8 項目」と数える）・`src-tauri/src/config_watcher.rs`（一時的失敗の保全方針の出所）
- 検査: `scripts/governance/checks/` の G-heading-refs（正準形の見出し参照）・G-module-index（モジュール索引と `.rs` の照合。`MODULE_INDEX_CRATES`）・G-stale-identifiers

## 再利用できる既存パターン

- 文書の縮小と退去の先例: `docs/adr/ADR-doc-promise-over-area-ratchet.md`（書く約束）、メモリの「メタハーネス層の段階撤去」。正本は 1 か所・他は参照（`AGENTS.md`「文書に事実の写しを増やす変更」）
- 組み直しの検算: 旧版と新版のバッククォート語・issue 番号・ADR 名を集合差で突き合わせる（`.claude/rules/governance-docs.md`）
- 撤去の検算: 消した語彙を `git grep` で数え、「撤去を描写している／在る前提で書いている」へ振り分ける（`AGENTS.md`「機構・層・ファイル群を撤去する」）

## 技術的制約

1. 外から正準形で指されている見出しは同じ字面で残す（G-heading-refs が落とす）。消す見出し（hidden/system・`query.rs` の正規化と `Cow<str>`・内部キー形式・`Option<T>` sentinel・Gotcha）は正準形の参照 0 件と分類表が言う
2. G-module-index はモジュール構成節の `.rs` の basename を照合する。草案は snotra-core の全 basename を持つと分類表が言う
3. 正本がこの文書にしか無い内容は、消す前に退去先へ移す（分類表「退去が必要な内容」の 6 件）
4. 今も偽の記述が 2 件ある（冒頭の「Win32 非依存」・「同じ型で渡すと `None` どうしの一致でフラグが立つ」——後者は `search/build.rs` の `DerivedStrings` の doc が否定していることを主が確認した）
5. コードの doc（`.rs`）を書き換えるので、カテゴリ A（`cargo doc` を含む）と F（`governance:check`）が掛かる。製品の挙動は変わらない

## 未解決の疑問

1. 草案の新しい全称表現が真か（例: 冒頭の「Win32 を呼ぶのは `indexer/path_env.rs` / `win_registry.rs` / `autostart.rs` だけ」）
2. 草案で消した条項のうち、他に正本が無いのに D・X とされたものは無いか（分類表の ⚠ 4〜7: 件数パラメータの索引・`hotkey.rs` の複製禁止・`update_config` と再構築・空白統一テストの検知器名）
3. 退去先 6 件の置き場（コードの doc か `PERFORMANCE.md` か `docs/development-principles.md` か）
4. 草案に載った参照（`INDEX_WIRE_SINCE` の doc・`LoadCacheResult::upgrade_save` の doc・`config/io.rs` の `LoadOutcome` / `load_from_dir_reporting` の doc・`normalized_default_resolves_all_migration_sentinels`）が、草案の言う中身を本当に持つか
5. `.claude/rules/snotra-core.md` と `snotra-core-search.md` の裸形の見出し参照が、草案でも着地するか

## 敵対的調査（3b）の所見と採否

全文は `workspace/adversarial-1327.txt`。草案を本番の位置に置いた複製（`git archive HEAD` を展開して差し替え）で `governance:check` を回し、全検査 passed。計器の確認として、同じ複製で見出しを 1 つ改名すると 7 件が赤になった。

**壊せなかった**: 正準形・裸形の見出し参照はすべて着地する／消した規則のうち folder のソート順・incremental cache の述語・`normalize_for_match` と `Cow`・内部キー形式・`sort_entries_canonical` と `BuiltAt` は正本が実在する／「共有治具は 1 つ」「マスク系は `query.rs` だけ」「opener の逆向きは 1 本」は真／草案が指す doc と検知器は言うとおりの中身を持つ／コードに触れる必要は無い。

| 所見 | 採否 | 反映 |
|---|---|---|
| M-1: 草案の「`reset_to_default()` も `apply_migrations()` を通してから保存」は古い。今は `Config::normalized_default()`（`config/migrate.rs`）が正規化済みの既定値を返す（`snotra-settings/src/app.rs:255` で主も確認） | 採る | 草案の文を `normalized_default()` を使う形へ直す |
| M-2: 草案が「昇格のウィンドウは起動順序だけで閉じている」の出所にした `LoadCacheResult::upgrade_save` の doc は、計測の待ちの話であり、読んでから保存するまでに別の書き手が書く窓の説明は `cache.rs` に無い（主も確認: 「ウィンドウ」0 件、`:692-697` は待ちの説明） | 採る | 窓の説明とプロセスをまたぐ残余を、`INDEX_WRITE_LOCK` の doc へ退去する |
| U-1: 「Win32 を呼ぶのは 3 ファイルだけ」は偽。`indexer/scan.rs:7-9` が cfg なしで `windows::` を use する（主も確認） | 採る | 列挙をやめ、「UI 非依存の純ロジック。Win32 を使うモジュールもある」の形にする |
| U-2: 「以上 `#[ignore]`」は偽（`tests/path_query_cost.rs` の `derives_same_bytes_as_normalize_entry_key` は既定で走る）。`tests/dir_stat_cost.rs` が索引に無い（主も確認） | 採る | 計器・検知器の区別は各 `//!` に任せ、索引はファイル名だけにして `dir_stat_cost.rs` を足す |
| U-3: 「入口はそれぞれ 1 つ」は事実として偽（`query_plan.rs` の `path_query`・`path_store.rs` の `push_segment` が別実装） | 採る | 「記録側と照合側は同じ関数を通す」の規範文に寄せ、パスマッチの系統は別の正規化だと明記する |
| N-1: `indexer/cache.rs:418` の「(#1013 の Gotcha)」が、Gotcha 節を消すと行き先を失う（正準形でないので緑のまま） | 採る | 直す参照元に足す（「開発ルール」へ） |
| N-2: 草案の「現状 v7」は次の版上げで偽になる数え上げ | 採る | 版の番号を書かず ADR を指す |
| 補足: PS7 の計器の罠は、退去しないと走査外の `docs/superpowers/` にしか残らない | 採る | 退去を CLAUDE.md の書き換えより先に行う |
| 反復 6 の経緯（`cache.rs:62-63` が CLAUDE.md を指す） | 主が確認 | `PERFORMANCE.md`「採用: 背景再スキャンの比較を digest へ（ロード 395 → 312 ms・反復 6）」に既在。参照をそちらへ向け直す |

**計器が見ない枝**（委譲の指摘を採る）: 「見出しは残るが中身が消えた参照」は governance:check では緑のまま。G-module-index は basename の包含照合で `tests/` を見ない。ゆえに直す参照元は手の一覧で直し、最後に `git grep` で確かめる。
