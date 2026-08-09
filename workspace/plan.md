# 実装計画 — #996 アイコンキャッシュの剪定を撤去する

**前提**: `workspace/research.md` の実測にもとづき**撤去を採る**。採否そのものが issue の
成果物なので、人間の承認はこの採否に対して求める。

## 目的

`drain_index` → `icon::sync_with_index` の索引照合による剪定を撤去し、掃除を
`IconCache::enforce_cap` の FIFO へ一本化する。剪定の存在から出ていた受容残余 3 つと、
それを支える API・テスト・文書を同時に消す。

**仕様変更である**（訂正・2026-08-09）。当初「`SPEC.md` §3.4 は剪定を規定していない」と
書いていたが**偽だった**——`SPEC.md:95` の「インデックス再構築時はキャッシュをクリア」が
drain 側を指している（経緯と見落とした機序は research.md「訂正」節）。`AGENTS.md` に従い
`SPEC.md` → コード → ドキュメントの順で同期した。**採否そのものは変わらない。**

## 受け入れ条件

1. `drain_index` が索引の全パスとキャッシュのキーを突き合わせなくなる
2. `show_icons=false` でメモリ内キャッシュが `None` になる挙動は**変わらない**（load-bearing・下記）
3. `IconCache::keys` / `IconCache::remove_paths` / `IndexTree::absent_paths` が消え、
   `cargo build --workspace` / `cargo test --workspace` が通る（＝移行漏れが構造的に無い）
4. `cargo doc --workspace --no-deps --document-private-items` が intra-doc link 切れ無しで通る
5. `PERFORMANCE.md` の「採用: アイコン剪定の判定を lock の外へ出す」節が撤去の記録へ
   置き換わり、剪定の存在を前提にした他 2 か所も整合する
6. `npm run governance:check` が通る

## 変更ファイル一覧と対象シンボル

| ファイル | 操作 | 対象シンボル |
|---|---|---|
| `src-tauri/src/indexing.rs` | 変更 | `drain_index` の `icon::sync_with_index(...)` 呼び出し（引数から `material.tree()` が消える） |
| `src-tauri/src/icon.rs` | 変更 | `sync_with_index` → `drop_icon_cache_if_disabled` へ改名・木の引数と照合部を削除 |
| `src-tauri/src/icon.rs` | 削除 | `IconCache::keys` / `IconCache::remove_paths` |
| `src-tauri/src/icon.rs` | 削除 | テスト 4 本（下記）と fixture `material_of` |
| `src-tauri/src/icon.rs` | 変更 | テスト 2 本を残す分岐へ合わせて改名 |
| `snotra-core/src/index_tree.rs` | 削除 | `IndexTree::absent_paths` とテスト 3 本・fixture `tree_with` |
| `snotra-core/src/engine.rs` | 変更 | `IndexInputs` の doc（`show_icons` を含める理由） |
| `PERFORMANCE.md` | 変更 | 3 か所（下記） |

## 残す分岐（消してはならない・根拠は research.md）

`show_icons=false → *icons.lock() = None` は load-bearing である。代替経路
（`ensure_icon_cache_loaded_if_enabled`）は `request_icons_for_results` の早期 return によって
**到達しない**ため、消すと show_icons を false にした後もメモリ内キャッシュが残り、終了時
`save_if_dirty` が `icons.bin` を書く。

## 実装順序

### Phase 1 — production コードの撤去（compile-fail を検出器にする）

- [x] `snotra-core/src/index_tree.rs` から `absent_paths` とそのテスト 3 本・`tree_with` を削除する
- [x] `src-tauri/src/icon.rs` の `sync_with_index` を `drop_icon_cache_if_disabled(icons: &IconCacheState, show_icons: bool)` へ縮める（木の引数・`absent_paths` 呼び出し・2 回目の lock を削除）
- [x] `IconCache::keys` / `IconCache::remove_paths` を削除する
- [x] `src-tauri/src/indexing.rs` の呼び出しを新シグネチャへ合わせる
- [x] `cargo build --workspace` が通ることを確認する（**移行漏れの検出器はこれである**——grep ではない）

### Phase 2 — テストの整理

- [x] 削除: `sync_with_index_keeps_keys_present_in_a_non_empty_tree` / `sync_with_index_removes_keys_absent_from_the_tree` / `remove_paths_preserves_cap_invariant` / `concurrent_insert_during_prune_window_survives` / fixture `material_of`
- [x] 残す 2 本を新しい名前へ合わせる（`..._drops_the_cache_when_icons_are_disabled` / `..._is_a_noop_when_the_cache_is_absent`）
- [x] **残す分岐を守る断言があることを確かめる**——`show_icons=false` でキャッシュが `None` になることと、`show_icons=true` では触らないことの 2 方向
- [x] `cargo test --workspace` が通ることを確認する

### Phase 3 — doc コメントの整理

- [x] `IconCache` の doc から剪定由来の記述（受容残余 3 つ・述語の向きの要石）を削除する。**`enforce_cap` の FIFO が唯一の掃除である**ことを 1 行で書く
- [x] `snotra-core/src/engine.rs` の `IndexInputs` doc を直す——`show_icons` を含める理由は「prune のついで」ではなく「無効化時にキャッシュを落とすため」である
- [x] `cargo doc --workspace --no-deps --document-private-items` を**手で**走らせる（PostToolUse hook は intra-doc link を見ない）

### Phase 4 — `PERFORMANCE.md` の整合

- [x] 「採用: アイコン剪定の判定を lock の外へ出す」節（:609）を**撤去の記録へ書き換える**（issue の撤去条件が名指し）。実測値（剪定が落としたのは 0 件）を根拠として残す
- [x] 候補表の「アイコン剪定の照合を二分探索へ」行（:595）を削除する（前提が消えた）
- [x] 「試みたが機能しない: 篩へ通す」節（:636）の「再び測る値打ちが出る 2 通り」から剪定側を落とす
- [x] :1185 の「フルパスを要求する消費者」から「アイコンキャッシュの剪定キー」を落とす
- [x] :25 のプレイブック §3（述語の向き）は**一般則なので残す**。実例への参照だけ直す
- [x] **（実装中に判明・計画外）** `docs/comment-guidelines.md:29` が `tree_with` を「書き写したと
      自ら名乗る」模範例として名指していた。**計画の doc 掃討が漏らした**——`grep 剪定|remove_paths|
      absent_paths|sync_with_index` には fixture 名が入っていない。`governance:check` が捕捉した
      （散文に現行語彙に無い識別子）。同等の生きた実例は他に無い（grep 実測: 他はすべて「書き写すな」
      の禁止側）ため、規則本体を残して参照だけ落とした

### Phase 5 — 調査用の足場の撤去（**この計画のコミット前に完了済み**）

- [x] `src-tauri/src/icon.rs` の `probe_996_icon_cache_vs_index`（`#[ignore]`）を削除した。
      撤去条件「採否が決まったら」が承認の時点で満たされたため、計画のコミット前に実施した
      （`git checkout -- src-tauri/src/icon.rs`）。**数値は `workspace/research.md` が正本である**

### Phase 6.5 — レビュー指摘への対応（計画外・code-reviewer round 1）

- [x] **High-1**: `SPEC.md:95` を現況へ同期した（インデックス再構築そのものでは掃除しない・
      丸ごと捨てる 2 契機を明記）。`invalidate_icon_cache` の呼び出し点が 1 か所であることを
      grep で確かめてから「2 つ」と書いた
- [x] **High-1 波及**: `PERFORMANCE.md` の「§3.4 は遅延ロードと FIFO cap しか規定していない」
      という**偽の主張**を訂正し、見落とした機序（`grep アイコン` で節を読んだ）まで記録した
- [x] **High-1 波及**: `plan.md` / `research.md` の誤った裏取りを訂正した（記録を偽のまま残さない）
- [x] **Low-1**: `search/tests/performance.rs:350` の「再び測る値打ちが出る条件」への参照を、
      受け皿が消えたので撤去後の扱いへ差し替えた
- [x] **Low-2**: 「掃除は `enforce_cap` ただ 1 つ」を「**キーを選び取って間引く機構は**ただ 1 つ」
      へ限定した（丸ごとの破棄が 2 経路あるので字義どおりには偽だった）
- [x] **Low-3**: 「そちらは到達しない」→「寄りかかれない」。`show_icons` の出所が
      live-read と `VisualSnapshot` で違うため、切り替えを跨いだ要求は偽のまま着きうる。
      **当初「1 フレームだけ」と数を書いたが自分で検算して撤回した**——`spawn_icon_load` は
      切り離したスレッドを起こすので、engine を読む時刻はフレーム数で押さえられない
      （`results_view.rs:210-216`）。最終形は「窓の長さは書かない」＋機序
- [x] **Low-4**: 「剪定は 1 件も落としていなかった」→「**測ったセッションでは**〜」
- [x] **Low-5**: 全称否定を日付つきの観測へ倒した
- [x] **Low-6**: `show_icons` を `IndexInputs` から外す案を候補表へ置いた（この反復では対処しない）
- [x] **Nit-1**: 「引数を取り違える」を落とした（型が違うのでコンパイルが通らない）
**Nit-2 は作業項目に置かない**（射程外ゆえ、この計画が所有しない）: `index_tree.rs:264` の
`path_into` doc「crate の外から使う口」は外部呼び出し 0 件（grep 実測）だが、**差分前から偽**
である（`absent_paths` も同 crate 内だったため、この撤去が生んだものではない）。直すなら
doc ではなく `pub` 自体の是非になるので、報告に回して人の判断を仰ぐ。

### Phase 6.6 — レビュー指摘への対応（code-reviewer round 2・High 0 件）

- [x] **Low-A**: 「丸ごとの破棄は `invalidate_icon_cache` と `drop_icon_cache_if_disabled` が
      持つ」という**数え上げが偽**だった（実際は 3 か所——`commands/icon.rs:22` が漏れており、
      **同じ doc の 10 行下で自分が言及している当の関数**だった）。数を落として「別経路が持つ」
      へ倒した。`IconCacheState` へ `None` を書く箇所を grep して 3 か所と実測してから直した
- [x] **Low-B**: `icon.rs` を「当てにできる」へ緩めたのに、**それを正本として指す
      `engine.rs` が「しか無い」の強いまま残っていた**（修正が指摘箇所へ集中して周辺に残った形）。
      `engine.rs` と候補表の引用を同時に揃えた
- [x] **Low-C**: 候補表の「実測されているもの」列が、テストの射程を超えて「再構築を kick する
      ことを固定している」と書いていた。実際に `index_inputs_differ_on_each_index_key`
      （実名で確認）が固定するのは `IndexInputs` の不一致までで、kick は `config_watcher` の
      不変条件が持つ。2 層に分けて書き直した
- [x] **nit ×4**: `performance.rs` の括弧の写し／`research.md` 結論 #1 の全称／`plan.md` の
      Low-3 行が実装の 1 つ前の版を指していた点／テスト doc の折返し

**⚠ Low-D（方法論・SPEC の記述は正しいので修正しない）**: SPEC の「丸ごと破棄するのは
2 つの場合に限る」の裏取りに `invalidate_icon_cache` の呼び出し点 grep（1 か所）を挙げたが、
**それが支えるのは箇条書き 1 つ目の呼び出し点であって、網羅そのものではない**。網羅を測る
grep は `IconCacheState` へ `None` を書く 3 か所の側で、契機へ畳むと 2 つになる（3 つ目は
「アイコン表示を無効へ切り替えたとき」に吸収される）。**結論は正しいが、示した証拠は主張と
別物だった**——High-1 を生んだのと同じ機序（`AGENTS.md`「照合は SSOT に対して行う」）の再発
であり、記録として残す。

### Phase 6 — 検証

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] `cargo doc --workspace --no-deps --document-private-items`
- [x] `npm run governance:check`
- [x] 実装差分を確定させる（Phase 1〜5 がすべて反映されている）

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| `show_icons=false` でメモリ内キャッシュが `None` になる | 残すテスト 1 本（断言つき） |
| キャッシュ件数が `cap` を超えない | 既存の `enforce_cap` のテスト群（無傷） |
| `icons.bin` の形式が変わらない | **バージョンバンプ不要**——`IconCacheData` に触れない（`/persistence-check` 対象外） |
| 削除した pub API の呼び出しが残らない | `cargo build --workspace` の compile-fail |
| intra-doc link が切れない | `cargo doc`（CI でのみ発火するため手で走らせる） |

**異常系**: 撤去により「索引に無いキー」が cap 件まで残りうる。**これは受容する**——害は
cap で有界であり、correctness は `invalidate_icon_cache` が担う。実測で剪定が落としたのは
0 件であり、撤去による差分は観測されていない。

## テスト方針と検証コマンド

- **新規テストは足さない。** 撤去する挙動を守るテストを消し、残す分岐のテストを保つだけである
  （検知器を足す局面ではない——守るべき不変条件が 1 つ減る変更である）
- コマンドは `docs/build-commands.md` カテゴリ A（fmt / clippy / test）＋ `cargo doc` ＋
  カテゴリ F（governance:check）。**カテゴリ C（smoke）は不要**——trace イベント名・hotkey 登録・
  表示経路のいずれにも触れない

## `SPEC.md`・関連文書の更新要否

| 文書 | 要否 | 理由 |
|---|---|---|
| `SPEC.md` | **要**（当初「不要」→ レビューで訂正） | §3.4:95「インデックス再構築時はキャッシュをクリア」が drain 側を規定していた。`grep アイコン` で節を読んだため当の 1 行が母集団から落ちていた |
| `PERFORMANCE.md` | **要**（Phase 4） | 剪定の存在を前提にした記述が 4 か所 |
| `src-tauri/CLAUDE.md` | **不要** | `icon.rs` の記述は `invalidate_icon_cache` の TOCTOU のみで剪定に触れていない（grep 実測） |
| `snotra-core/CLAUDE.md` | **不要** | 「剪定」の記述は履歴剪定（`top_n`）であって無関係（grep 実測） |
| `docs/architecture.md` | **不要** | 剪定への言及なし（grep 実測） |
| ADR | **不要** | 否定の知識が生じない（B 案を却下する決定ではなく、機構を 1 つ消す決定である） |

## 未確定（実装前に潰す）

- [x] 剪定が実際に何件落とすか — 実機セッションで実測。**650 件中 0 件**（「索引に無い」86 件は
      86/86 が PATH 併合エントリで、剪定は併合の後に走るため木に在る）。research.md が正本
- [x] `icons.bin` の定常サイズ — 650 件で 304,918 B を実測。cap 1,000 で約 460 KiB 前後の見込み
- [x] `SPEC.md` が剪定を規定していないか — §3.4 と `SPEC.md:408` を読んで確認。**規定していない**
- [x] `show_icons=false` 分岐を消してよいか — 経路を辿って**消せない**と判定（代替経路は到達しない）
- [x] 削除する 3 API に他の消費者がいないか — grep で production 消費者は `sync_with_index` のみと確認
- [x] intra-doc link の切れ先があるか — 削除するブロック内で閉じていることを grep で確認

## セルフレビュー

- リスク: 通常
- plan-review: 未実施（通常リスク）
- エージェント数: 0
- 要対処: 0 件
- 未検証: フォルダ階層モードを掘ったセッションでの「索引に無いキー」件数（研究 §未解決の疑問 1）。
  **採否を変えないため未検証のまま進む**——索引外のパスが生じる場合、現行の剪定はそれらを
  FIFO より悪い順序で捨てるので、どちらに転んでも撤去を支持する

### 主エージェントによる自己照合（5a の 5 点）

1. **issue の全要件に作業項目が対応する** — issue の要求は「採否を決める」＋「採るなら
   `PERFORMANCE.md` の該当節を撤去の記録へ書き換える」。前者は research.md の結論、
   後者は Phase 4 の第 1 項目
2. **境界条件を列挙し、各条件に検証がある** — `show_icons` の真偽 2 分岐（残すテスト 2 本）、
   キャッシュ不在（残すテスト 1 本）、cap 超過（既存の `enforce_cap` テスト群）
3. **新しい状態・リソース・プロセスに正常/失敗/破棄経路がある** — 新設は無い（撤去のみ）
4. **より単純な既存パターンで置き換えられないか** — これ自体が「より単純へ寄せる」変更である
5. **壊してはならない不変条件に検知手段がある** — 上の表のとおり全 5 項目に検知手段がある

### `AGENTS.md` 条件別チェックの該当判定

| トリガー | 該当 | 対応 |
|---|---|---|
| 関数・型を改名／**旧 API の削除** | **該当** | compile-fail を移行漏れ検出器にする（Phase 1 末尾） |
| 重複した読み・冗長に見える状態を束ねる/消す | **該当** | 「後で読まれるか」を消す各箇所について書き出した（残す分岐の節） |
| ガバナンス文書（`*.md`）を変更 | **該当** | `npm run governance:check`（Phase 6） |
| ファイル（`.rs`）を追加/削除 | 非該当 | ファイルの増減は無い |
| 永続形式・識別子/キー形式を変更 | 非該当 | `IconCacheData` に触れない（`/persistence-check` 不要） |
| 並行境界（worker・channel・listener・共有状態） | 非該当 | lock を**減らす**のみで、新しい並行経路を作らない |
| 対称ペア（生成/破棄・フラグ真偽） | 非該当 | 残す分岐は既存の対称のまま（`None` 化の経路は変えない） |
| 件数 N・上限パラメータ・導出の入力を変更 | 非該当 | `cap` の導出には触れない |
| 機能削除・trace イベント名／hotkey・表示経路 | 非該当 | trace・hotkey・表示経路のいずれにも触れない（smoke 前提は無傷） |
| 網羅性が要件 | 非該当 | 削除対象はコンパイラが数え上げる（`/plan-review --deep` 不要） |

## 人間レビュー

- [x] 承認済み — 2026-08-09 / 問い: "**`workspace/plan.md` への注釈、または明示的な承認をお願いします。** 承認いただければ workspace をコミットし、`/implement` で実装へ渡せます。" / 回答: "承認 /implement にいこう"
