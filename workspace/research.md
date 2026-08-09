# 調査 — #996 アイコンキャッシュの剪定そのものを撤去する

## issue の要約

`drain_index` → `icon::sync_with_index` の**アイコンキャッシュ剪定を撤去する**か否かを決める。掃除は `IconCache::enforce_cap` の FIFO に任せる。PR #995（反復 12）のレビューで「そもそも剪定が要るのか」が浮上し、#995 の射程（lock 保持時間を削る）とは別物なので分離された。

**この issue の成果物は採否の決定である**（実装は採る場合のみ従属する）。撤去条件は「採否が決まったら閉じる」。

## issue の前提の裏取り（一次証拠）

| issue の主張 | 裏取り | 結果 |
|---|---|---|
| 剪定は correctness を担わない。stale アイコンの防御は `invalidate_icon_cache` | `src-tauri/src/icon.rs:228` の doc と `main.rs` の呼び出し | **真** |
| `SPEC.md` は剪定を規定していない（§3.4 は遅延ロードと FIFO cap のみ） | `SPEC.md:96-97` を読んだ。`SPEC.md:408` の「剪定」は**履歴剪定**（`result_limit`・#348）で無関係 | **真**。撤去は仕様変更ではない |
| `IconCache::insert` は毎回 `enforce_cap` を呼び、挿入順＝ FIFO で自己収束する | `icon.rs:67-71`（`insert` → `enforce_cap`）・`icon.rs:45-57`（`load` でも適用） | **真** |
| `request_icons_for_results` は行を `is_folder` でも view 種別でも絞らない | `results_view.rs:188-192` は `is_error` と in-flight/重複だけで絞る | **真**。フォルダ階層モードの行のアイコンもキャッシュへ入る |

## 実測（2026-08-09）

### 実運用プロファイルの `icons.bin`

`%APPDATA%/Snotra` 配下を全走査した結果、**`icons.bin` は存在しない**。

| 項目 | 値 |
|---|---|
| `config.toml` の `show_icons` | `true`（アイコンは有効） |
| `index.bin` | 17,321,657 B（2026-08-08 更新） |
| `history.bin` / `window.bin` | 在る |
| **`icons.bin`** | **不在**（`%APPDATA%` / `%LOCALAPPDATA%` 全体を `-Filter icons.bin -Recurse` で走査しても 0 件） |

**読み方**: 永続キャッシュはこの機体で残っていない。ゆえに**次のセッションは必ずコールドスタートであり、アイコンはセッションを跨いで蓄積していない**——剪定が片づける対象（索引に無い古いアイコン）がセッションを越えて溜まる経路は、実運用では成立していない。

**不在の機序は特定していない**（候補は 2 つ以上ある: 背景再スキャンが `Changed` を返して `invalidate_icon_cache` が削除した／最後の実セッションが `save_if_dirty` を通らずに終わった）。**判断に効くのは不在という事実だけであり、どちらの機序でも上の読み方は変わらない。**

### 実機セッションでの実測（2026-08-09・人手操作）

`C:/tmp/snotra-icon-probe`（実 config の複製・実索引 313,028 件）で起動し、1〜2 分の検索と
スクロールの後に `/q` の正規終了経路を通して `icons.bin` を残した。読み取りは
`probe_996_icon_cache_vs_index`（`SNOTRA_CONFIG_DIR` を渡した `#[ignore]` テスト）。

| 項目 | 実測値 |
|---|---:|
| `cap`（`Config::icon_cache_cap()`） | 1,000 |
| キャッシュ件数 | **650** |
| `icons.bin` バイト数 | **304,918**（約 298 KiB） |
| 索引（`index.bin`）件数 | 313,028 |
| `index.bin` に無いキー | 86 |
| **うち PATH 併合配下（＝実行時の木には在る）** | **86 / 86** |
| **剪定が実際に落とすキー** | **0** |

**86 件の分類は測って出した**——各キーの親ディレクトリを `$env:PATH` の全項目と突き合わせ、
86 件すべてが一致した（`.cargo/bin`・`scoop/shims`・`WindowsApps`・`Chocolatey/bin`・
`.local/bin` 等）。`index.bin` は PATH 併合**前**に書かれるがこの config は
`include_path_env = true` で、**剪定は併合の後に走る**（`drain_index` の順序:
`rebuild_and_save` → `extend_with_path_entries` → `sync_with_index`。
`PERFORMANCE.md:595` も「剪定はその後」と記録している）。ゆえに 86 件は実行時の木に在り、
`absent_paths` は**空を返す**——`sync_with_index` の `!dead.is_empty()` ガードにより
`remove_paths` は呼ばれもしない。

**読み方**:

1. **剪定はこのセッションで 1 件も落としていない。** 撤去してもキャッシュの中身は変わらない
2. **cap には届きうる**（1〜2 分の操作で 650 / 1,000 = 65%）。ただし上の 1 により、
   撤去しても cap を占める内訳は変わらないので、**押し出しは増えない**
3. `icons.bin` の定常サイズは cap 1,000 件で約 460 KiB 前後と見込める（650 件で 298 KiB）。
   issue が「概ね ≤1 MiB」と構造から導いていた見積もりと整合する

**この 1 セッションが覆わない範囲**（受容する残余）: フォルダ階層モードで索引外のパスを
掘った場合は「索引に無いキー」が実際に生じうる。**その場合こそ issue の反対材料が言うとおり、
現行の剪定は FIFO より悪い順序でそれらを選択的に捨てている**——撤去を支持する向きに働く。

### 自動化での測定 — **6 回試行して収束せず（人手操作へ切り替えた・切り出し先は #999）**

実 config を複製した使い捨てプロファイル（`C:/tmp/snotra-icon-probe`・実索引 17.3 MB）で
起動し、クエリ × スクロールでアイコンを積んでから `/q` の正規終了経路（`flush_persistent_state`
→ `save_if_dirty`）で `icons.bin` を残す設計にした。**`icons.bin` は一度も生成できていない。**

**観測された壁**: どの実行でも `egui_results:show`（rows=200）までは正常に進み、**その直後から
trace が完全に沈黙する**——以降に注入した Down キーも Escape も 1 つも届かず、`/q` にも到達
しないため強制終了（＝ flush なし）へ落ちる。

切り分け（`diag-996.ps1`・3 検証とも合格）で分かったこと:

| 検証 | 結果 |
|---|---|
| results 窓なしでの Escape | hide **した** |
| results 窓あり・Down 0 回での Escape | hide **した** |
| results 窓あり・Down 10 回（40/120ms）後の Escape | hide **した** |
| 前面窓は 3 時点とも main のまま | **奪われていない** |

**この切り分けが確かめたのは Escape の到達だけである。** 選択の移動を出す trace イベントが
無いため、**Down キーがアプリへ届いたことはどの実行でも確かめていない**——文字（`egui_input:changed`）
だけが到達を名乗れる。到達の測り方は `Send-SnotraKey` の doc が持つ `SNOTRA_EGUI_INPUT_TRACE`
（注入時刻と本体の `rx_key` を突き合わせる）であり、**今回はこれを有効にしていない。**

つまり切り分けスクリプトでは同じ操作が通るのに、測定スクリプトでは通らない。**再現条件を
特定できていない**（打鍵速度を 18ms → 40/120ms へ落としても、スクロールを 200 → 10 行へ
減らしても、同じ地点で沈黙した）。

**この沈黙自体が製品の欠陥かどうかは、この issue の射程では判定していない。** 注入経路
固有の問題（`keybd_event` と egui のイベント駆動 wake の相互作用）である可能性が高く、
実ユーザーの手入力で同じことが起きる証拠は無い。**#996 の採否とは別の論点なので、
ここでは事実の記録に留める。**

**ゆえに issue の「決める前に測ること」2 点のうち、`icons.bin` の定常サイズは
上の実運用観測（不在）で答えが出ているが、cap 近傍の押し出し頻度は未測定である。**

## 関連ファイル・シンボル（撤去した場合に消えるもの）

すべて grep で実在と消費者を確認済み。

### production コード

| 場所 | 消えるもの | 他の消費者 |
|---|---|---|
| `src-tauri/src/indexing.rs:112` | `icon::sync_with_index(...)` の呼び出し | — |
| `src-tauri/src/icon.rs:196` | `sync_with_index` の**索引照合部分**（`show_icons=false` 分岐は残す・下記） | — |
| `src-tauri/src/icon.rs:100` | `IconCache::keys()` | production は `sync_with_index` のみ（実測） |
| `src-tauri/src/icon.rs:136` | `IconCache::remove_paths()` | production は `sync_with_index` のみ（実測） |
| `snotra-core/src/index_tree.rs:287` | `IndexTree::absent_paths()` | production は `sync_with_index` のみ（実測） |
| `snotra-core/src/engine.rs:65` | `IndexInputs.show_icons` の**コメントの根拠**（「index ビルドのついでに prune するため含める」） | フィールド自体は残る（下記） |

### テスト

| 場所 | 消えるもの |
|---|---|
| `icon.rs` | `sync_with_index_keeps_keys_present_in_a_non_empty_tree` / `sync_with_index_removes_keys_absent_from_the_tree` / `remove_paths_preserves_cap_invariant` / `concurrent_insert_during_prune_window_survives` / `material_of` fixture |
| `icon.rs` | `sync_with_index_drops_the_cache_when_icons_are_disabled` / `sync_with_index_is_a_noop_when_the_cache_is_absent` は**残す**（下記の残す分岐を守る） |
| `index_tree.rs` | `absent_paths_returns_only_keys_the_tree_does_not_have` / `absent_paths_compares_full_paths_verbatim` / `absent_paths_is_empty_without_keys` / `tree_with` fixture（`tree_with` の消費者はこの 3 本だけ・実測） |

### ドキュメント（`grep 剪定|remove_paths|absent_paths|sync_with_index` を `**/*.md` へ当てた全数）

| 場所 | 要る手当て |
|---|---|
| `PERFORMANCE.md:609` の「採用: アイコン剪定の判定を lock の外へ出す」節 | **撤去の記録へ書き換える**（issue の撤去条件が名指し） |
| `PERFORMANCE.md:595` の候補表「アイコン剪定の照合を二分探索へ」行 | 前提（剪定の存在）が消えるので行ごと撤去 |
| `PERFORMANCE.md:636` の「試みたが機能しない: 篩へ通す」節 | 「再び測る値打ちが出る 2 通り」のうち剪定側が消える |
| `PERFORMANCE.md:25` のプレイブック §3（述語の向き） | **残す**——一般則であり実例への参照だけ直す |
| `PERFORMANCE.md:1185` | `IndexCache` v6 でフルパスを要求する消費者 3 つのうち「アイコンキャッシュの剪定キー」が消える |
| `snotra-core/src/index_tree.rs:284` の doc | `absent_paths` ごと消える |

## 残す分岐と、その根拠（「後で読まれるか」の 1 行ずつ）

**`sync_with_index` の `show_icons=false → cache = None` 分岐は load-bearing であり、撤去してはならない。**

経路を辿った結果:

1. `show_icons` を false へ変更 → `config_watcher` が `IndexInputs` の差分を検知（`show_icons` は `IndexInputs` のフィールド）→ `start_index_build` → `drain_index` → `sync_with_index(false)` → メモリ内キャッシュが `None` になる
2. 代替経路として `ensure_icon_cache_loaded_if_enabled`（`commands/icon.rs:21-24`）も `!show_icons` で `None` にするが、**呼ばれない**——その唯一の呼び出し元 `load_icon_pngs` を起こすのは `request_icons_for_results` であり、そこが `!show_icons` で早期 return する（`results_view.rs:184-186`）
3. ゆえに分岐を消すと、show_icons を false にした後もメモリ内キャッシュが残り、終了時 `save_if_dirty` が `icons.bin` を書く

**帰結**: `sync_with_index` は「索引と揃える」関数ではなくなり、木を引数に取らなくなる。名前と doc を実体へ合わせる（`IndexInputs.show_icons` も残るが、コメントの理由が「prune のため」から「無効化時にキャッシュを落とすため」へ変わる）。

## 再利用できる既存パターン

- **旧 API の削除は下流の compile-fail を移行漏れ検出器にする**（`AGENTS.md` 条件別チェック）。`IconCache::keys` / `remove_paths` / `IndexTree::absent_paths` はいずれも `pub` なので、消して `cargo build -p snotra` / `cargo test --workspace` が通れば移行漏れゼロが構造的に言える
- **`PERFORMANCE.md` の「試みたが機能しない」節の書き方**（反復 12 が直前に 2 節書いた）をそのまま踏襲できる

## 技術的制約

- `IndexTree::absent_paths` は `snotra-core`、`IconCache` は `snotra`（src-tauri）にあり、**crate をまたぐ**。`snotra-core` 側の削除は `snotra` のビルドで検証される
- `src-tauri` は `[lib]` を持たないため `cargo test -p snotra --lib` は常に失敗する（`src-tauri/CLAUDE.md`）。テストは `cargo test -p snotra`
- doc コメントの intra-doc link（`[`IconCache::remove_paths`]` 等）は **PostToolUse hook では検出されず CI でのみ落ちる**（`.claude/rules/comments.md`）。リンク元が複数あるため `cargo doc --workspace --no-deps --document-private-items` を手で走らせる必要がある

## 未解決の疑問

1. **フォルダ階層モードを掘ったセッションでの「索引に無いキー」の件数は測っていない。**
   上の 1 セッションは検索結果の閲覧のみで、そこでは 0 件だった。ただし**この未測定は採否を
   変えない**——索引外のパスが生じる場合、issue の反対材料が示すとおり現行の剪定はそれらを
   FIFO より悪い順序で捨てるので、**どちらに転んでも撤去を支持する**
2. 測定スクリプトで観測した「`egui_results:show` 直後の打鍵不達」の再現条件は
   **#999 へ切り出した**（2026-08-09）。#996 の採否には効かない

## 結論（採否の推奨）

**撤去を推奨する。** 根拠は次の 4 点で、うち 2 点は実測である。

1. **実測: 剪定は 0 件しか落としていない**（650 件のキャッシュに対し、索引に無いキーは
   PATH 併合を勘定すると 0）。撤去してもキャッシュの中身は変わらない
2. **実測: 実運用プロファイルに `icons.bin` が存在しない**——セッションを跨いだ蓄積が
   起きていないので、剪定が防ぐはずの「古いアイコンの堆積」は実際には生じていない
3. **構造: 害は cap で有界**（`insert` が毎回 `enforce_cap` を呼ぶ）。撤去で失うのは衛生であって
   correctness ではない（stale の防御は `invalidate_icon_cache`）
4. **構造: 撤去で受容残余 3 つが同時に消える**（正本は `IconCache::remove_paths` の doc）。
   加えて `keys()` / `remove_paths()` / `absent_paths()` の 3 API と、それらを説明する
   doc・テスト・`PERFORMANCE.md` の 3 節が消える

**却下側の材料は残っていない**——issue が挙げた反対材料（フォルダ階層モードの行を選択的に
捨てる）は、issue 自身が言うとおり撤去を**支持する**向きに働く。

## 調査用の一時的な計器（撤去条件つき）

| 足場 | 撤去条件 |
|---|---|
| `src-tauri/src/icon.rs` の `probe_996_icon_cache_vs_index`（`#[ignore]`） | #996 の採否が決まったら削除する。数値は本ファイルが正本 |
| scratchpad の `probe-996-session.ps1` | 同上（リポジトリ外なのでコミットされない） |
| `C:/tmp/snotra-icon-probe/` の使い捨てプロファイル | 同上 |
