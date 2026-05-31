# 設計メモ: config↔派生状態コヒーレンシ所有の一元化（StaleSet 契約）

- **status**: Draft（合意待ち）
- **issue**: #347（構造）/ #348（下流症状の対症療法を本設計に統合）/ 経緯 #337
- **scope**: `size:L` / `type:refactor`。**設計先行** — 本メモの合意後に実装サイクルへ
- **date**: 2026-05-31

> 本メモは AGENTS.md「横断パターンの変更は相談してから」に従う設計先行成果物。
> 合意された設計を実装サイクルで忠実に実装する。

---

## 1. 問題（なぜ直すか）

config は単一の source of truth。しかし派生消費者が **2 つの整合契約**に分かれ、束ねる所有者がいない。

- **カテゴリ A（live-read）**: 毎操作で config を読み直す。設定変更が自動で即時整合。**正しく、壊さない。**
- **カテゴリ B（構築時焼き込み）**: config 値が長命オブジェクトに凍結される。整合に再構築が必要。**ここが無主地。**

`Engine`（`config` + `search_engine` + `history` を単一 Mutex で共同所有）が唯一の自然な所有者だが、
`update_config` は config を差し替えるだけで派生状態に触れない（`engine.rs:148-150`）:

```rust
pub fn update_config(&mut self, config: Config) {
    self.config = config; // search_engine も history も再評価しない
}
```

理由は正当（ロック内で秒単位の再構築をすればロック最小化原則が壊れる）。しかし結果として
コヒーレンシ所有権が Engine の外（config_watcher の非同期ループ）へ追放され、3 つの弊害を生む:

1. **網羅漏れ**: index 由来 B（entries+kana）だけが整合される。**`HistoryStore.top_n` は漏れ**（#348 欠陥 B）
2. **lost-update 窓**: 整合の正しさが engine Mutex 軸と `indexing` AtomicBool 軸の**両方**に依存し、軸間にすき間がある（#348 欠陥 A）
3. **キー集合の対称二重メンテ**: index 入力キー集合が `needs_reindex` と in-flight `needs_rebuild` の 2 箇所に重複。新キー追加（#337 の migemo）が対称漏れリスクを生む

### 1.1 決定的観察: stale は「config キー単位」でなく「派生オブジェクト単位」

一つの config キーが両カテゴリにまたがる:

| config キー | カテゴリ A としての顔 | カテゴリ B としての顔 |
|---|---|---|
| `migemo_enabled` | `kana_query` を作るかの即時フラグ（`search.rs:70`） | `kana_lower_names` の構築入力（`compute_wave1`） |
| `top_n_history` | 検索取得上限 `fetch_limit`（live-read、`engine.rs:83`） | `HistoryStore.top_n` = `prune()` 容量（`history.rs`） |

→ 整合判断は **派生オブジェクト**（SearchEngine / HistoryStore）を単位に行う。これが「StaleSet」の名の所以。

---

## 2. 契約（StaleSet）

### 2.1 中核原則

> **`update_config` を、全カテゴリ B 派生物に対する単一のコヒーレンシ・チョークポイントにする。**
> config を差し替えた瞬間、各カテゴリ B 派生物について `update_config` が次のいずれかを行う:
> - **(軽量) インライン reconcile**: その場で派生物を再評価（O(1)〜O(top_n) の安価なもの）
> - **(重量) staleness を記録**: ロック外 drain のために stale ledger に印を付ける（秒単位の重いもの）

`StaleSet` は **(重量) の ledger**。「どのカテゴリ B 派生物が再構築待ちか」を表す。

### 2.2 派生物ごとの分類

| 派生物 | 入力 config | reconcile コスト | drain モード |
|---|---|---|---|
| `SearchEngine` | `scan` / `show_hidden_system` / `include_path_env` / `migemo_enabled` | 重（O(N) scan + 並列構築、秒単位） | **重量**: ロック外ビルド + atomic swap |
| `HistoryStore.top_n` | `top_n_history` | 軽（容量設定 O(1) + prune O(top_n)） | **軽量**: インライン reconcile |

→ 現状の StaleSet 有効ビットは実質 `INDEX_STALE` の 1 つ。history は軽量インラインで足り、ledger に載せなくてよい
（§6 で粒度の意思決定）。だが **概念契約は同一**: 「update_config が全 B の整合を所有する」。

### 2.3 不変条件（StaleSet の真偽ペア）

- **set（誰が立てる）**: `update_config` が old/new config を diff し、index 入力が変われば `INDEX_STALE` を **単調 OR** で立てる
- **clear（誰が・どの条件で戻す）**: drain が**完了時に、ビルド開始時スナップショットと現在 config を re-diff し、一致したときだけ** `INDEX_STALE` を落とす
- **戻せない経路を作らない**: ビルド失敗時は bit を **残す**（stale のまま）。次の drain 契機が拾う（→ §5 失敗モード）
- set/clear は**同一の engine Mutex 上**で起きる（§3 の核心）

---

## 3. 所有者の置き場所と 3 同期軸の統合

### 3.1 レイヤー境界の制約

`snotra-core` は Tauri 非依存。スレッド spawn・`AppHandle`・イベント emit は `src-tauri` の責務。
→ **判断と実行を分離する**:

- **コヒーレンシ判断 = Engine（snotra-core）**: 「何が stale か」「ループ要否」「atomic swap」「軽量インライン reconcile」
- **重い drain の駆動 = src-tauri（indexing.rs）**: スレッド spawn・ロック外 `PrebuiltIndex::new`・UI イベント・CAS ガード

Engine が **stale ledger と判断プリミティブ**を所有し、src-tauri は**いつ・どこで重い仕事をするか**だけを担う。
これで「コヒーレンシ所有権を Engine に戻す」と「重い再構築はロック外」を両立する。

### 3.2 3 同期軸の「統合」の正体 — 物理統合ではなく判断軸の一元化

現状、整合の正しさは **軸1（engine Mutex）と軸2（`indexing` AtomicBool）の両方**に依存している。
これが lost-update 窓の根。

**設計の核心**: stale ledger を **engine Mutex（軸1）上の状態**として置く。すると:

- **変更検出**（`update_config` が bit set）も **整合判断**（drain 完了時の re-diff で bit clear）も **すべて軸1で起きる**
- **軸2（AtomicBool）はコヒーレンシの正しさから外れる** — 二重ビルド防止（CAS）と UI 表示専用に降格
- **軸3（INDEX_WRITE_LOCK）も無関係**のまま（index.bin 単一書き手の安全装置）

つまり 3 つのロックを物理的に 1 つに merge するのではない（それはロック最小化を壊す）。
**コヒーレンシの正しさの議論を軸1 だけに閉じる**のが「統合」。軸2/軸3 は性能・安全のための独立した装置に純化する。

### 3.3 lost-update 窓が閉じる理由（厳密）

現状の交錯（#348 欠陥 A）:

| 時刻 | 動作 | 問題 |
|---|---|---|
| t1 | in-flight B1: 完了後 needs_rebuild を**古い config**で算定 → false。ロック解放 | |
| t2 | config_watcher: `update_config` で設定反転（軸1） | |
| t3 | config_watcher: `indexing.load()`=true（軸2）→ start_index_build **見送り** | 変更が軸1 にあるのに軸2 を見て握りつぶす |
| t4 | B1: finish_index_build（indexing=false） | |
| t5 | B1: `if needs_rebuild`(false) → 再ビルドせず | **lost update** |

新設計（set/clear が同一 engine ロック上の臨界区間）:

```
// update_config（engine ロック内）
self.config = new;
self.stale |= diff_index_keys(&old, &new);   // 単調 OR
// → ロック解放後に drain を kick（CASで二重起動防止）

// drain 完了（engine ロック内、同一臨界区間で swap→re-diff→loop判断）
self.apply_prebuilt_index(new_index);
if index_keys(&self.config) == snapshot.index_keys {  // 開始時スナップショットと re-diff
    self.stale.remove(INDEX_STALE);                    // 一致したときだけ clear
}
let need_more = self.stale.contains(INDEX_STALE);      // 同一ロック内で判断
```

`update_config` の bit-set と drain 完了の bit-clear は **engine Mutex で相互排他**。よって全順序は
「update_config が drain 完了臨界区間より前か後か」の 2 ケースのみ:

- **前**: drain の re-diff で `config != snapshot` → clear しない → `need_more=true` → ループ。**取りこぼさない** ✓
- **後**: drain が clear（その時点で一致）→ finish。直後の update_config が再び set + kick。kick の CAS は
  B1 が finish 済み（`index_build_started=false`）なので成功 → 新ビルド。**取りこぼさない** ✓

軸2 のすき間（旧 t3-t4）は無関係になる: config_watcher は `indexing` を**見ずに**「常に bit set + kick」する。
kick は CAS 成功なら自分でビルド、失敗（in-flight）なら何もしないが **bit は立っている**ので
in-flight ビルドの完了 re-diff が必ず拾う。

### 3.4 収束性（停止性）

drain は「完了時に config がビルド開始時から変わっていればループ」する。各反復は最新 config のスナップショットを取る。
config 変更は有限（ユーザーがトグルを止める）。よって **config が落ち着いた後、ある反復のスナップショットが
完了時の config と一致 → clear → 停止**。現行の eventual-consistency と同じ収束性を、取りこぼしなしで達成する。

---

## 4. API スケッチ（実装サイクルで確定。ここでは形のみ）

`snotra-core::Engine`:

```rust
// 軽量カテゴリ B はインライン reconcile、重量は stale 記録
pub fn update_config(&mut self, new: Config) {
    let old = std::mem::replace(&mut self.config, new);
    // 軽量: history 容量を即追従（#348 欠陥 B 解消）
    if self.config.search.effective_top_n_history() != old.search.effective_top_n_history() {
        self.history.set_top_n(self.config.search.effective_top_n_history()); // 必要なら prune
    }
    // 重量: index 入力が変われば stale 記録（needs_reindex 相当を 1 箇所に集約）
    if index_keys_differ(&old, &self.config) {
        self.stale.insert(Stale::INDEX);
    }
}

// drain（src-tauri が駆動）。snapshot を返し、なければ None
pub fn begin_index_drain(&self) -> Option<IndexInputs>; // INDEX_STALE 時に index 入力スナップショット
pub fn complete_index_drain(&mut self, built: PrebuiltIndex, from: &IndexInputs) -> bool; // swap→re-diff→残 stale を返す
```

`src-tauri::indexing.rs`（drain ドライバ。CAS は据え置き）:

```rust
// config_watcher は indexing を見ず、常に kick
fn kick_index_drain(app) {
    if !state.try_begin_index_build() { return; } // in-flight なら bit が拾う
    spawn(move || loop {
        let Some(inputs) = engine.lock().begin_index_drain() else { break };
        let built = PrebuiltIndex::new(rebuild(...inputs...), inputs.migemo_enabled); // ロック外
        let more = engine.lock().complete_index_drain(built, &inputs);   // swap+re-diff
        if !more { break; }
    });
    state.finish_index_build();
}
```

**キー集合 `index_keys_differ` / `IndexInputs` が唯一の定義**になり、`needs_reindex` と in-flight
`needs_rebuild` の二重メンテ（migemo 特別扱い）が解消する。

**re-diff のコストモデル**: `IndexInputs` は index 入力（`scan: Vec<ScanPath>` / 3 bool）の固定サイズ snapshot。
これは **現状の `indexing.rs:36` で既にやっている `paths.scan.clone()` と同コスト**であり、新たなオーバーヘッドではない。
完了後 re-diff の比較は O(scan paths)（数件〜数十件）で安価。drain ループは収束性（§3.4）により通常 1〜2 反復で停止するため、
snapshot copy の蓄積コストは無視できる。

---

## 5. 失敗モードと不変条件

| 事象 | 設計上の扱い |
|---|---|
| ビルド中 panic / `PrebuiltIndex::new` 失敗 | stale bit は **残る**（clear しない）。データ損失なし。次の drain 契機で再試行 |
| thread spawn 失敗 | 現行同様 `finish_index_build` でフラグを戻す。bit は残るので次の config 変更で回復 |
| 唯一の再試行契機が「次の config 変更」 | **要意思決定**: 透過的回復には起動時 + 明示リトライが要るか（§6 Q4）。最低限「次回起動で必ず最新 config で構築」は現状維持 |
| drain ループ中に何度も config 変更 | 収束性（§3.4）で停止保証。各反復は最新スナップショット |

**壊してはならないシステム不変条件**:
- ロック最小化（重い build は engine ロック外）— 維持
- `INDEX_WRITE_LOCK` 単一書き手 — 維持（drain も `rebuild_and_save` 経由）
- CAS 二重ビルド防止（`try_begin_index_build`）— 維持
- カテゴリ A live-read — 不変
- **新しい同期軸を増やさない**: stale ledger は engine Mutex 上の状態であり、新ロックを作らない（むしろ軸2 を正しさから外す）

---

## 6. 意思決定が必要な設計分岐（レビューで veto 可能）

- **Q1 StaleSet の粒度**: 現状 index-stale 1 つ。**推奨: 最小実装**（`bool index_stale` + `IndexInputs` スナップショット）。
  bitflags 型は派生物が 3 つ以上になってから（YAGNI）。概念契約「update_config が全 B を所有」は型に依らず成立
- **Q2 `show_icons` の扱い**: 現状 `needs_reindex` に含まれ index ビルドのついでにアイコンを prune。厳密には **icon-stale（src-tauri 所有）であって index-stale でない**。
  **推奨: 現状維持**（show_icons → index 再構築のまま、スコープ膨張を避ける）。ただし「これは概念的には別カテゴリ」とコメントで明示
- **Q3 history の drain モード**: **推奨: インライン reconcile**（update_config 内で `set_top_n` + 必要時 prune）。
  軽量（O(top_n)、top_n は数百）なのでロック内で許容。StaleSet の重量 ledger に載せない
- **Q4 失敗時リトライ**: **推奨: 暫定は「次の config 変更 / 次回起動」で回復**（現状と同等、データ損失なし）。明示リトライは別 issue
- **Q5 SPEC.md 更新**: 挙動の**外形**（設定変更が反映されるタイミング）は #348 修正で「即時反映」に揃う。
  top_n_history 変更が再起動を要しなくなるのは挙動変更 → **SPEC.md の設定反映に関する記述を要同期**（実装サイクルで）

---

## 7. 代替案（なぜ StaleSet か）

| 案 | 内容 | 評価 |
|---|---|---|
| **Alt-1 対症療法のみ（#348 単独）** | pending-reindex ラッチ + `history.set_top_n` を個別に足す | ✗ migemo 二重メンテ残存・所有権散在・次の B 追加で同じ問題再発。構造を直さない |
| **Alt-2 同期再構築** | `update_config` がロック内で再構築 | ✗ ロック最小化を壊す（秒単位ロック保持）。却下 |
| **Alt-3 StaleSet（推奨）** | update_config を単一チョークポイント化、stale ledger を engine ロック軸に置く | ✓ 網羅・lost-update 解消・migemo 特別扱い解消・top_n 解消が**ひとつの機構の instance**に。`size:L` の設計投資に見合う |

**結論**: Alt-3。ただし実装は **Q1/Q3 の推奨（最小実装・history インライン）**で開始し、過剰な抽象化を避ける。
StaleSet の価値は bitflags 型ではなく「**update_config が全カテゴリ B のコヒーレンシを所有する**」という契約の一元化にある。

### 7.1 「これは Alt-1 の偽装では？」への回答（最小実装でも成立する 3 つの構造デルタ）

最小実装（index-stale bool + history インライン）にすると重量 ledger メンバーは index 1 つだけになり、
「bitflags 型は不要 → 結局 Alt-1 + `set_top_n` と同じでは？」という批判が成立しうる（レビュー指摘）。
**回答: No。** #347 の構造的成果は **bitflags 型ではなく、型に依存しない次の 3 デルタ**であり、Alt-1 はどれも達成しない:

| # | 構造デルタ | Alt-1 | Alt-3（本設計・最小実装でも） |
|---|---|---|---|
| D1 **所有権の帰属** | `needs_reindex` が config_watcher（**Engine の外**）に居続ける。所有権散在のまま | `index_keys_differ` が **`update_config`（Engine = 所有者）内**へ移動。「設定変更⇒派生 reconcile」が Engine の責務になる |
| D2 **キー集合の単一定義** | `needs_reindex` と in-flight `needs_rebuild` の **2 箇所**が残る（migemo 二重メンテ継続） | set（update_config）と re-diff（complete_index_drain）が **同一の `index_keys_differ` を参照** = 1 定義 |
| D3 **窓の閉じ方** | pending-reindex ラッチを **軸2 に新規追加**して塞ぐ（状態を増やす） | コヒーレンシ判断を **軸1 に閉じる**（軸2 を正しさから外す）。**新しい状態を足さずに**塞ぐ |

→ history がインラインでも、これは「ledger から外れて散在」ではなく **「update_config が所有する軽量 reconcile」**であり、
D1 の所有権一元化に**含まれる**。「配置だけ一元化・機構は散在」批判は、history を config_watcher 等の外部に置いた場合に当たるが、
本設計は update_config 内に置くため当たらない。**StaleSet は契約名（update_config = 単一チョークポイント）であって容器型ではない。**

→ ただしレビュー指摘どおり、**bitflags 容器型を導入するのは「3 つめの重量 B 派生物が現れたとき」**（YAGNI）。
現時点で 3 つめの候補は無い（migemo/scan/path_env は全て SearchEngine = INDEX_STALE に集約、show_icons は icon-stale で別所有）。
よって実装は bool で開始し、型昇格は将来の判断とする（§6 Q1）。

---

## 8. 段階的実装ロードマップ（合意後の別サイクル用・概略）

> 本サイクルでは実装しない。合意後に `/start-issue` 系で着手する際の起点。

- **Phase 1 — history インライン reconcile（#348 欠陥 B）**: `HistoryStore::set_top_n` 追加（`#[must_use]` 不要、容量設定）+
  `update_config` で追従。TDD: top_n 縮小→prune 容量追従 / 拡大→深さ追従のユニットテスト（`history.rs` / `engine.rs`）。**最小・独立・先行可能**
- **Phase 2 — index-stale ledger 化（#347 中核 + #348 欠陥 A）**: Engine に `index_stale` + `IndexInputs` snapshot、
  `begin_index_drain` / `complete_index_drain` を追加。`needs_reindex` / in-flight `needs_rebuild` をこの 1 機構に統合。
  config_watcher は `indexing` を見ず常に set+kick。TDD: lost-update 窓の状態遷移テスト（`state.rs` フラグ + Engine stale 機構、AppHandle 非依存）。
  着手時チェックリスト（レビュー指摘の反映）:
  - **`needs_reindex` の全 caller を grep 列挙**し、削除順序の依存を確認してから段階的に置換（一括置換しない）
  - **新しい同期軸を増やさないことの形式確認**: 変更後の `state.rs` / `engine.rs` の全フィールドを列挙し、stale 判断が `engine.stale`（軸1）のみに依存・新 `AtomicBool`/`Mutex` が増えていないことを確認
  - `IndexInputs` の snapshot/比較が現状の `indexing.rs:36` 開始時キャプチャと同コストであることを確認（§4 コストモデル）
- **Phase 3 — ドキュメント同期**: `snotra-core/CLAUDE.md`（migemo 二重メンテ記述の更新）/ `src-tauri/CLAUDE.md`（drain 機構）/
  `.claude/rules/*` / **`SPEC.md`（設定の即時反映に関する記述。特に top_n_history 変更が再起動不要になる挙動変更を同期）** /
  `docs/architecture.md`（「設定管理」節に StaleSet 契約を追記）。`docs/architecture.md` に本設計メモへの参照を追加

**検証**: 各 Phase で `docs/build-commands.md` の該当カテゴリ（A: snotra-core test/clippy、B: src-tauri test）を実行。

---

## 9. オープンクエスチョン（合意時に確認したい点）

1. Q1〜Q5 の推奨（最小実装・show_icons 現状維持・history インライン・暫定リトライ・SPEC 同期）に同意か
2. Phase 1（history）を #347 本体と分けて先行マージするか、#347 の一括 PR にまとめるか
3. `docs/design/` を本リポジトリの設計メモ常設ディレクトリとして採用してよいか（本ファイルが初出）
