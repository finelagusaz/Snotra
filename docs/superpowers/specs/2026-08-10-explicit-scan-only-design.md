# 索引の更新を明示操作だけに一本化する設計（#1001）

キャッシュヒットの起動が毎回 `C:\` を全走査している状態を、**自動の再スキャンを撤去して**解消する。索引が更新されるのは初回構築・`/s`（手動再構築）・設定変更による reindex の 3 つだけになる。

対で、設定アプリのインデックスタブに**最終構築日時**を表示する。

判断の経緯と却下した案（間引き案を含む）は `docs/adr/ADR-rescan-explicit-only.md` が正本である。本書は**何を作るか**だけを書く。

## 1. なぜこの形が成立するか

`/s` は**ランチャーで 2 打鍵**である（SPEC §15.3: コマンド文字列が完全一致した時点で debounce をキャンセルし、Enter なしに即実行）。明示のコストがほぼゼロなので、自動経路が肩代わりしていた価値は「気づかないうちに追いつく」だけに縮む。

残る唯一の穴は「**再構築が要ることに気づけない**」である。これを設定タブの最終構築日時が塞ぐ。

## 2. 撤去するもの

### 2.1 `snotra-core`

- `BackgroundRescanTask` / `RescanOutcome` / `RescanRun` / `IndexMaterial` を運ぶ `material` と `scanned_config_hash`
- `try_background_rescan` / `try_background_rescan_in`
- `entries_digest` / `digest_over` / `DigestSource`（消費点は再スキャンの比較 1 か所だけであることを実測済み）
- `LoadOrScanResult.rescan_task` と `LoadOrScanStats.digest_ms`、cache-hit 路の `digest_over(material.tree())` 計算
- `try_with_index_write_lock`（日和見的書き手が消えるので読者がいなくなる）
- **`INDEX_GENERATION` / `current_index_generation` / `snapshot_index_generation` / `load_with_index_generation`** — 世代機構は「ロード後に権威的ビルドが割り込んだ場合に古い snapshot が新しい `index.bin` を巻き戻さない」ためのものであり、その巻き戻しうる書き手が再スキャンだけである。**撤去の前に読者を grep して確かめる**（読者が他に居れば残す）
- `rescan_log.rs` 全体（SPEC §13.4 が「消えても壊れても振る舞いが変わらない」使い捨てと定めている計器）

`with_index_write_lock`（ブロッキング版）は**残す**。権威的ビルドと、下の形式昇格が共有する。

### 2.2 `src-tauri`

- `setup_background_rescan` と `apply_rescanned_index`（PR #1017）
- `main.rs` の `//!` にある再スキャン経路の説明

### 2.3 死ぬ検知器

`background_rescan_*` 系すべて、`sorted_comparison_ignores_enumeration_order`（digest の消滅と同時に対象を失う）、`try_with_index_write_lock_skips_closure_when_lock_held`、`rescan_generation_is_snapshotted_before_cache_load`、`setup_background_rescan` の本体を文字列で切り出す検査（`main.rs:763`）。**削除する検知器は 1 本ずつ「何を守っていたか」と「その対象が消えたこと」を対で確認する**——守る対象が生きているのに検知器だけ消すのが最悪の形である。

## 3. 移設するもの

### 3.1 形式昇格 → `load_cache_in` の旧版枝

**これを移さないと、旧版 `index.bin` が手動再構築まで永久に残る**（2026-08-07 に実運用点で実測した「v4 が毎起動 `normalized_keys` 35.98 MiB を読んでは捨てる」が恒久化する）。

`snotra-core/CLAUDE.md` は「昇格をロード側に置いてはならない」と書いているが、**その禁止が効くのは v7 の枝だけである**。v7 は木を直読みするので `entries` が存在せず、木から作り直すと反復 6 で消した 62.5 MiB の複製が復活する。一方 v2〜v6 の枝は `cache.entries: Vec<AppEntry>` を手に持っており、現在それを `IndexTree::build(cache.entries)` へ渡している。

```rust
fn save_cache_sorted_in(dir, entries: Vec<AppEntry>, config_hash: u64) -> (IndexTree, CachedMasks)
```

返り値が material を建てるのに必要なものそのものなので、**その 1 行を差し替えるだけ**でよい。**加算ではなく置換であり、複製は発生しない。**

- `sorted_by_path` は外部の事前条件ではなく `derived.tree.sorted_by_path`（`IndexTree::build` が entries から判定する）なので、満たすべき前提は無い
- 書き込みは **`with_index_write_lock` 経由**にする（`index.bin` を書く経路の標準契約）
- **保存に失敗してもロードは成功のまま返す。** 昇格は最適化であって、失敗が索引の可用性を落としてはならない
- 代価は一回性である。v4〜v6 は旧キャッシュが既に持っているマスクを `derive_columns` が計算し直し、加えて 17 MiB を書く（実測 `save_ms` = 458 / 683 ms）。**昇格する起動でだけ払い、以後は v7 の枝に入る**

### 3.2 アイコンキャッシュの無効化 → 権威的ビルドの単一入口

**無言で消してはならない。** #996 が再構築時のキャッシュ掃除を撤去したため、**エントリ集合が変わったときの無効化は `RescanOutcome::Changed` が唯一の担い手**である（SPEC §3.4）。再スキャンを消すと担い手が 0 になり、アイコンは FIFO 上限まで古いまま残る。

移設先は `src-tauri/src/indexing.rs` の `start_index_build`——**ビルド要求の全経路（config 変更 reindex / first-run / 手動 rebuild / 自己再 kick）が通る単一入口**である。判定は要らず、無条件に呼ぶ。

**今日の機構が何を守っていたかを正確に書いておく**（移設先の妥当性はこれで決まる）:

- キーは**エントリの `target_path`**（`.lnk` ならショートカット自身のパス）
- `Changed` はエントリ**集合**が変わったときだけ立つ。`.lnk` の張り替えやアプリ更新で**アイコンだけ**変わった場合は集合が同じなので `Unchanged` になり、**今日も無効化されない**
- ゆえに生き残ったキーの刷新は「他の場所で何かが増減したときの**巻き添え**」であり、正しさの機構というより粗い GC である

移設後の引き金は「ユーザーが再構築を要求したとき」になる。**巻き添えより狙いが良く、しかもユーザーが既に「インデックス構築中...」を見て待っている時刻に落ちる。**

## 4. 追加するもの

### 4.1 `snotra-core`: `index_built_at_in`

```rust
/// `index.bin` の先頭から `built_at`（UNIX 秒）だけを読む。
pub fn index_built_at_in(dir: &Path) -> Option<u64>
```

`index.bin` のヘッダは 8 バイト（magic 4 + version 4）で、その直後の最初のフィールドが `built_at: u64`（postcard の varint）である。**先頭の数十バイトを読んで varint を 1 個デコードすれば済み、17 MiB を読む必要は無い。**

- **`built_at` が全版で先頭フィールドであることは、誰かが保証した契約ではなく観測された性質である**（v2〜v7 の 6 版すべてで確認した）。依存する以上、`index_cache_on_disk_format_is_stable`（golden bytes）へ「offset 8 の varint が `built_at` と一致する」assertion を 1 本足して固定する
- magic を検証し、version が既知（2〜7）でなければ `None` を返す
- ファイル不在・読めない・デコード不能はすべて `None`

### 4.2 `snotra-settings`: 最終構築日時の表示

インデックスタブに 1 行。**表示だけで、ボタンも `/s` への誘導文も置かない。**

```
最終構築: 2026-08-04 09:12
```

- `snotra-settings` は `snotra-core` に依存済みなので、`index_built_at_in` を直接呼べる
- `index.bin` が無い・読めないときの表示（「未構築」相当）を決め、**ja / en 双方の `TrKey` を用意する**
- 設定アプリは別プロセスであり、本体との通信路は `config.toml` と `config_watcher` だけである。**この表示は通信路を新設しない**

## 5. 文書の同期

**すべて実装と同じコミットで直す。** 写しを取りこぼすのは写しを直す当のコミットである（#977）。

| 場所 | 何を直すか |
|---|---|
| `SPEC.md` §3.3 | 通常起動の背景処理そのものを削除し、更新契機を 3 つ（初回・`/s`・設定変更）として書き直す。**「設定画面から手動再構築可能」は現在の実装と食い違っている**（`snotra-settings` に再構築を撃つ経路は無い）ので、同じ変更で直す |
| `SPEC.md` §3.4 | アイコンキャッシュ破棄の 2 条件のうち「`RescanOutcome::Changed`」を「権威的ビルドの開始」へ書き換える |
| `SPEC.md` §13.4 | 節ごと削除（計器の撤去） |
| `snotra-core/CLAUDE.md`「indexer.rs の背景再スキャン」 | **節ごと書き直し**（最大の写し）。形式昇格の置き場所が変わるので「昇格をロード側に置いてはならない」の射程を v7 の枝へ限定する |
| `snotra-core/CLAUDE.md`「index.bin 書き込みの排他」 | 日和見的書き手が消えるので、書き手の一覧を直す |
| `docs/adr/ADR-mtime-differential-scan-ceiling.md` | 「帰結」の**効く順**（間引き → watch → USN）と「受容する残余」の再測定条件（間引き後の代金）が偽になる。両方に新 ADR への指しを入れる |
| `docs/superpowers/specs/2026-08-10-rescan-applies-its-result-design.md`（#1017） | 「本 ADR で撤去」の 1 行 |
| `docs/superpowers/specs/2026-08-09-rescan-in-situ-instrument-design.md` | 同上（計器も撤去される） |
| `tests/memory_footprint.rs` の残余記述 | `digest_ms` を足して塞いだ複製の話が対象を失う |

## 6. 検知器

撤去が主体なので、**新設は少なく、確認は「消えた対象が本当に消えたか」に寄る**。

| 検知器 | 固定する事実 |
|---|---|
| `index_built_at_in` の単体テスト | v7 / 旧版 / ファイル不在 / magic 不正 / 切り詰めの 5 点。dir 注入 |
| `index_cache_on_disk_format_is_stable` への追加 assertion | offset 8 の varint が `built_at` と一致する（4.1 の依存を固定する） |
| `load_cache_in` の昇格テスト | 旧版を置いて読むと、**返る material が正しく、かつ `index.bin` が現行版になっている**。保存に失敗させてもロードは成功する |
| アイコン無効化の移設先テスト | `start_index_build` を通ると無効化が撃たれる |
| 起動経路の回帰 | キャッシュヒットの起動で**走査が 1 回も起きない**ことを測る。**これが受け入れの本体である** |

**変異で落ちるところまで確かめる。** とくに最後の 1 本は、撤去し忘れた経路が残っていれば落ちなければならない。

## 7. 受容する残余

- **索引は明示的に再構築するまで古いままである。** 新しく入れたアプリは `/s` を打つまで検索に出ない。これは設計の選択であって欠陥ではないが、**沈黙して古い**という失敗の形は残る（設定タブを開かないユーザーには最終構築日時が届かない）。ユーザーの判断で表示は設定タブのみに絞った
- **アイコンは権威的ビルドまで古いままである。** 「同じパスで中身が変わった」場合の刷新は今日も担保されていない（§3.2）ので、これは新しい残余ではなく既存の残余の可視化である
- **`/s` を打つと 22〜30 秒かかる。** 走査そのものは速くなっていない。#1002（`scan_all` の `seen`）はこの代金に直接効く

## 8. この変更が #1001 の受け入れに対して何を満たすか

- **受け入れ 1（計器）**: 満たした後に**撤去する**。計器は「毎起動の全走査が何をしているか」を測るためのもので、その全走査自体が無くなる
- **受け入れ 2（USN 可否）**: 満たしている（#1001 のコメント）。結論は変わらない
- **受け入れ 3（頻度が毎起動でなくなり、取りこぼしの検知器がある）**: **文言どおりには満たさない。** 自動の変更検出そのものを撤去するので「取りこぼしの検知器」は対象を失う。頻度側は満たす（0 回になる）
- **受け入れ 4（SPEC と実装が同じことを言う）**: 満たす

再スコープの根拠は ADR に書く。issue へどう反映するかは `/merge-pr` の手順に乗せる。

## 9. 別 issue の候補（本書では作らない）

- **watch 枝**（`notify` / USN）。明示のみに倒したことで価値が上がる——全走査なしに鮮度を上げる唯一の道である
- **設定アプリからの再構築ボタン**。設定 → 本体の通信路を新設する話で、本書とは独立した機構である

## 10. 触らないもの

- `scan_all` の中身（#1002）
- `sort_entries_canonical`
- `start_index_build` の drain ループと panic 戦略
- 検索・履歴・アイコン抽出のパイプライン
