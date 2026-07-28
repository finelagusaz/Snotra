# 設計メモ: config↔派生状態コヒーレンシ所有の一元化（StaleSet 契約）

- **status**: Agreed（2026-05-31 合意: Q1 同意 / Q2 `top_n` フィールド削除に同意 / Q3 Phase 1 先行マージ / Q4 `docs/design/` 採用）
- **rev**: 2026-05-31 archaeology による精緻化（カテゴリ B を既約核へ縮小・history を live-read 化・StaleSet は単一 `index_stale` bit に収斂）
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

## 1.5 なぜ現状がこの形か — 設計を形作った力（archaeology）

git 史実調査（commit 証拠付き）で、現状の複雑さを「**本質的制約**（理由が正当・維持する）」と
「**偶発的複雑さ**（惰性で積もった・理想で消える）」に切り分けた。これが「削ってよい線」を引く。

### 本質的制約（維持する）

| 構造 | 理由（証拠） | あるべき姿での扱い |
|---|---|---|
| 単一 `Mutex<Engine>` を全検索が共有 | 3重ロック→1本化で lock 管理を単純化（`ca9a0f7` Phase 2.3「simplifying lock management」） | 維持。ロック保持時間＝全 IPC ブロック時間という構造的事実 |
| 重い再構築は必ずロック外（PrebuiltIndex） | SearchEngine 構築 50k=42ms / 100k=124ms、検索は 1.3–2.1ms（PERFORMANCE.md）。30–60倍。`2c6ea0f`/#116 H-1 で off-lock 化 | 維持。off-lock + atomic swap は不可侵 |
| snotra-core は Tauri 非依存 | レイヤー境界。スレッド/AppHandle/emit は src-tauri | 維持。判断=Engine / 駆動=src-tauri の分離 |
| 2 つの AtomicBool | first-run で `indexing=true`（ビルドスレッド不在）だが `index_build_started=false` で CAS 成立（`d2da6a5`、#325 で formalize） | 維持。ただし**コヒーレンシ判断からは外す**（UI+CAS 専用に純化） |
| `INDEX_WRITE_LOCK` | index.bin.tmp の単一書き手（`70e2a8f`/#325、coherence と直交） | 維持。純粋な file 安全装置 |

### 偶発的複雑さ（理想で消える）

| 症状 | 由来（証拠） | なぜ偶発か |
|---|---|---|
| `update_config` がコヒーレンシ非所有 | 初出 `ca9a0f7` から差し替えのみ（`git log -S "self.config = config"` が**1件のみ**＝変遷なし） | 「重い再構築をロック外へ」は正当だが「出した先の所有者を Engine に残す」を忘れた。所有権の config_watcher への"追放"は**決定でなく欠落** |
| `needs_reindex` キーの漸増・二重メンテ | 3キー（`8678a78`）→ +include_path_env（`f964fb7`/#264）→ +migemo（`16ede58`/#337）。inline `\|\|` 連鎖で機能ごとに堆積 | 設計でなく機能追加の堆積。in-flight `needs_rebuild` と同一集合の二重化は #264 で生まれ #337 で初めて認識された |
| in-flight `needs_rebuild` | `f964fb7`/#264「ビルド中の設定変更を完了後に再反映」 | lost-update を*部分的に*塞ぐ patch。cross-axis 窓（#348-A）は残った |
| `HistoryStore.top_n` 漏れ | setter は**史上一度も存在しない**（`git log -S "set_top_n"` は設計メモのみ）。`main.rs:404` で起動時に1度焼くだけ | **純粋な見落とし**。top_n_history の別の顔（fetch_limit）が live-read で"だいたい効く"ため、prune 容量のドリフトが顕在化しなかった |

### archaeology が教える核心

**3 つの軸は装置としては本質的だが、「コヒーレンシの正しさが軸1＋軸2 にまたがる」のは偶発**。
各軸は別時期に別目的で生まれ（軸1=facade 統合 / 軸2=first-run UI+CAS / 軸3=file 安全）、
`update_config` が reconcile 所有を放棄したために整合ロジックが軸をまたぐループへ散らばった。
→ あるべき姿は「軸を物理統合する」のではなく「**コヒーレンシ所有を `update_config` に戻し、
判断を軸1 に閉じ、軸2/軸3 を本来の単機能に純化する**」。

---

## 2. 契約（StaleSet）

### 2.1 中核原則 — カテゴリ B は「config 由来キャッシュ」である

カテゴリ A/B の違いの正体は **キャッシュの有無**。派生値はすべて config（+データ）の純関数であり、
問題は「いつ materialize するか」だけ:
- **A = live-read**: 毎回計算し直す（安いのでキャッシュ不要）
- **B = キャッシュ**: 高価なので長命オブジェクトに焼き込む

→ **バグの本質は「config 由来キャッシュに無効化プロトコルが無い」こと**（cache invalidation の所有者不在）。

原則（PERFORMANCE.md の実測で根拠づけ）:

> **(1) 既定は live-read（A）。キャッシュ（B）にするのは「計測して高価」なものだけ。**
> SearchEngine 構築は 50k=42ms / 100k=124ms ≫ 検索 1.3–2.1ms（30–60倍）→ キャッシュ必須。
> 一方 prune は O(top_n)（top_n≈数百）で安価 → キャッシュ不要、live-read でよい。
>
> **(2) すべてのカテゴリ B キャッシュの無効化を `update_config` が所有する。**
> config を差し替えた瞬間に各 B キャッシュを無効化（重量は stale 記録）。軽量なものは**そもそも B にしない**。

### 2.2 あるべき姿の 2 手 — B を既約核へ縮小し、残った 1 つを正しく無効化する

| 派生物 | 現状 | あるべき姿 | 理由 |
|---|---|---|---|
| `SearchEngine`（entries/masks/**kana**） | B（キャッシュ） | **B のまま + 無効化プロトコル**（`index_stale` + `IndexInputs` snapshot） | 構築 42–124ms。真に高価＝キャッシュ不可避。無効化を update_config が所有 |
| `HistoryStore.top_n` | B（焼き込み・setter 無し・ドリフト） | **live-read 化して B から除外**（フィールド削除、save/prune に引数渡し） | prune は O(top_n) で安価。fetch_limit と同じ live-read にすれば setter も stale も不要 |

- **手1（縮小）**: history を live-read 化。`HistoryStore.top_n` フィールドを消し、`save` / `prune` が config から `top_n` を受け取る。
  → **#348-B（top_n ドリフト）は構造的に発生不能**になる（焼き込む場所が無い）。category B から history が消える
- **手2（保護）**: 唯一残る重量キャッシュ SearchEngine に、update_config 所有の無効化（§3）を付ける

→ 結果、**重量カテゴリ B は SearchEngine ただ 1 つ**。「StaleSet」は単一の `index_stale` bool に収斂する（§7.1）。
これは縮退ではなく **B を既約核まで減らした帰結＝意図した最小終端形**。将来 2 つめの高価な派生キャッシュが現れたとき初めて集合へ育つ。

### 2.3 不変条件（StaleSet の真偽ペア）

- **set（誰が立てる）**: `update_config` が old/new config を diff し、index 入力が変われば `index_stale` を **単調に立てる**（false→true のみ）
- **clear（誰が・どの条件で戻す）**: drain が**完了時に、ビルド開始時スナップショットと現在 config を re-diff し、一致したときだけ** `index_stale` を false に戻す
- **戻せない経路を作らない**: ビルド失敗時は bit を **残す**（stale のまま）。次の drain 契機が拾う（→ §5 失敗モード）
- set/clear は**同一の engine Mutex 上**で起きる（§3 の核心）

---

## 3. 所有者の置き場所と 3 同期軸の統合

### 3.1 レイヤー境界の制約

`snotra-core` は Tauri 非依存。スレッド spawn・`AppHandle`・イベント emit は `src-tauri` の責務。
→ **判断と実行を分離する**:

- **コヒーレンシ判断 = Engine（snotra-core）**: 「何が stale か」「ループ要否」「atomic swap」「history の live-read（save/prune 時に config から top_n 渡し）」
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
if index_keys_differ(&old, &self.config) { self.index_stale = true; }  // 単調 set（false→true）
// → ロック解放後に drain を kick（CASで二重起動防止）

// drain 完了（engine ロック内、同一臨界区間で swap→re-diff→loop判断）
self.apply_prebuilt_index(new_index);
if index_keys(&self.config) == snapshot.index_keys {  // 開始時スナップショットと re-diff
    self.index_stale = false;                          // 一致したときだけ clear
}
let need_more = self.index_stale;                      // 同一ロック内で判断
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
// 重量カテゴリ B（SearchEngine）のみ stale 記録。
// 軽量（history）は live-read 化したので update_config は一切触れない（手1）。
pub fn update_config(&mut self, new: Config) {
    let old = std::mem::replace(&mut self.config, new);
    // index 入力が変われば stale 記録（needs_reindex 相当を 1 箇所に集約）
    if index_keys_differ(&old, &self.config) {
        self.index_stale = true;   // 単調 OR（bool 1 つ。集合型は不要 = §7.1）
    }
}

// history は live-read: prepare/prune は呼ばれた時点で config から top_n を渡す（焼き込まない＝手1）。
pub fn prepare_history_save_if_dirty(&mut self, threshold: u32) -> Option<PreparedHistorySave> {
    let top_n = self.config.search.effective_top_n_history();
    self.history.prepare_save_if_dirty(threshold, top_n) // 内部 prune(top_n) で容量適用
}

// drain（src-tauri が駆動）。snapshot を返し、なければ None
pub fn begin_index_drain(&self) -> Option<IndexInputs>; // index_stale 時に index 入力スナップショット
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
| ビルド中 panic / `PrebuiltIndex::new` 失敗（**panic 戦略依存**） | **release（`panic="abort"`、Cargo.toml 既定）**: build スレッド panic はプロセスを abort（明示的なクラッシュ）。finish も catch_unwind も走らないが、プロセスごと終了するため **silent wedge にはならず**、次回起動で fresh build される。**unwind ビルド（debug/test、または `panic="unwind"`）**: `catch_unwind` で捕捉し `finish_index_build` で flag を戻す（wedge 防止）。stale bit は残り、次の config 変更 / 手動 rebuild で回復。どちらの戦略でも「flag 固着で UI 永久構築中」は起きない（Codex 実装後レビュー反映: catch_unwind は unwind 限定で効くため、release は abort で wedge 回避） |
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

- **Q1 StaleSet の粒度**: **推奨: 単一 `bool index_stale` + `IndexInputs` スナップショット**。
  手1（history を live-read 化）で重量 B が SearchEngine 1 つに固定されるため、集合型は不要。これは縮退でなく**意図した終端形**（§7.1）。
  bitflags 型は 2 つめの高価な派生キャッシュが現れてから（YAGNI）
- **Q2 `show_icons` の扱い**: 現状 `needs_reindex` に含まれ index ビルドのついでにアイコンを prune。厳密には **icon-stale（src-tauri 所有）であって index-stale でない**。
  **推奨: 現状維持**（show_icons → index 再構築のまま、スコープ膨張を避ける）。ただし「これは概念的には別カテゴリ」とコメントで明示
- **Q3 history の扱い**: **推奨: live-read 化してカテゴリ B から除外**（`HistoryStore.top_n` フィールドを削除し、`save`/`prune` に config から `top_n` を引数渡し）。
  fetch_limit と同じ live-read になり、**setter も stale も reconcile も不要**——#348-B は構造的に発生不能になる。`set_top_n`（reconcile）案より単純で正しい（archaeology の知見）
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

**結論**: Alt-3。ただし実装は **Q1/Q3 の推奨（単一 bool・history は live-read 化して B から除外）**で開始し、過剰な抽象化を避ける。
StaleSet の価値は bitflags 型ではなく「**update_config が（残った）カテゴリ B キャッシュの無効化を所有する**」という契約の一元化にある。
そして手1 で B を SearchEngine 1 つに縮めるため、その「契約」は単一 bit の無効化に収斂する。

### 7.1 「これは Alt-1 の偽装では？」への回答（最小実装でも成立する 3 つの構造デルタ）

history を live-read 化（手1）すると重量カテゴリ B は SearchEngine 1 つだけになり、`index_stale` は単一 bool で済む。
「ならば結局 Alt-1 + `set_top_n` と同じでは？」という批判が成立しうる（レビュー指摘）。
**回答: No。** #347 の構造的成果は **bitflags 型ではなく、型に依存しない次の 3 デルタ**であり、Alt-1 はどれも達成しない:

| # | 構造デルタ | Alt-1 | Alt-3（本設計・最小実装でも） |
|---|---|---|---|
| D1 **所有権の帰属** | `needs_reindex` が config_watcher（**Engine の外**）に居続ける。所有権散在のまま | `index_keys_differ` が **`update_config`（Engine = 所有者）内**へ移動。「設定変更⇒派生 reconcile」が Engine の責務になる |
| D2 **キー集合の単一定義** | `needs_reindex` と in-flight `needs_rebuild` の **2 箇所**が残る（migemo 二重メンテ継続） | set（update_config）と re-diff（complete_index_drain）が **同一の `index_keys_differ` を参照** = 1 定義 |
| D3 **窓の閉じ方** | pending-reindex ラッチを **軸2 に新規追加**して塞ぐ（状態を増やす） | コヒーレンシ判断を **軸1 に閉じる**（軸2 を正しさから外す）。**新しい状態を足さずに**塞ぐ |

→ history は **live-read 化してカテゴリ A になる**（手1）。つまり「ledger から外れて散在」ではなく **そもそも B でない**——
fetch_limit と同じく毎回 config から読むだけで、無効化の対象ですらない。「配置だけ一元化・機構は散在」批判は
history を B のまま外部 reconcile に置いた場合に当たるが、本設計は history を B から除外するため当たらない。
**StaleSet は契約名（update_config = カテゴリ B キャッシュ無効化の単一所有者）であって容器型ではない。**

→ ただしレビュー指摘どおり、**bitflags 容器型を導入するのは「2 つめの高価な派生キャッシュが現れたとき」**（YAGNI）。
現時点で候補は無い（migemo/scan/path_env は全て SearchEngine = `index_stale` に集約、show_icons は icon-stale で別所有、history は live-read で B ですらない）。
よって実装は bool で開始し、型昇格は将来の判断とする（§6 Q1）。

→ **結論（あるべき姿）**: 手1 で history を B から外すため、重量 B は SearchEngine に固定され、`index_stale` が単一 bool なのは
**設計上の終端形**（縮退ではない）。それでも D1–D3 は成立し、しかも history は『散在』ですらない（そもそも B でない）。
「配置だけ一元化・機構は散在」批判はこれで完全に解ける。

---

## 8. 段階的実装ロードマップ（合意後の別サイクル用・概略）

> 本サイクルでは実装しない。合意後に `/start-issue` 系で着手する際の起点。

- **Phase 1 — history を live-read 化（#348 欠陥 B を構造的に消す）**: `HistoryStore.top_n` フィールドを削除し、
  `save` / `prepare_save_if_dirty` / `prune` が `top_n` を引数で受け取る。`Engine::prepare_history_save_if_dirty` / `prepare_history_flush` が
  `self.config.search.effective_top_n_history()` を渡す（全 history mutation は Engine 経由＝config 保有を確認済み:
  `commands/launch.rs`・`commands/system.rs`・`main.rs`）。`HistoryStore::load(top_n)` も引数不要化。
  TDD: top_n 縮小→prune 容量追従 / 拡大→深さ追従（`history.rs` + `engine.rs`、Win32 非依存）。
  **最小・独立・先行可能。setter も stale も不要**
- **Phase 2 — index-stale ledger 化（#347 中核 + #348 欠陥 A）**: Engine に `index_stale` + `IndexInputs` snapshot、
  `begin_index_drain` / `complete_index_drain` を追加。`needs_reindex` / in-flight `needs_rebuild` をこの 1 機構に統合。
  config_watcher は `indexing` を見ず常に set+kick。TDD: lost-update 窓の状態遷移テスト（`state.rs` フラグ + Engine stale 機構、AppHandle 非依存）。
  着手時チェックリスト（レビュー指摘の反映）:
  - **`needs_reindex` の全 caller を grep 列挙**し、削除順序の依存を確認してから段階的に置換（一括置換しない）
  - **新しい同期軸を増やさないことの形式確認**: 変更後の `state.rs` / `engine.rs` の全フィールドを列挙し、stale 判断が `engine.stale`（軸1）のみに依存・新 `AtomicBool`/`Mutex` が増えていないことを確認
  - `IndexInputs` の snapshot/比較が現状の `indexing.rs:36` 開始時キャプチャと同コストであることを確認（§4 コストモデル）
  - **実装済み（2026-05-31, `refactor/index-stale-ledger`）。スケッチ §4 からの確定点（マルチパースペクティブレビュー反映）**:
    ① bit を立てるのは `update_config` ではなく **`start_index_build`**（config 変更 reindex / first-run / 手動 rebuild / 自己再 kick の全経路を統一。`update_config` を呼ぶ経路は config_watcher 唯一と確認済みなので取りこぼしなし）
    ② **finish 後に `is_index_stale` を再チェックして再 kick**し、complete clear〜finish の窓を閉じる
    ③ build 本体を **`catch_unwind` で包み panic でも必ず `finish_index_build`**（panic wedge 対策＝レビュー Agent 1 検出。panic 経路は再 kick せず無限リトライ回避、`index_stale` は次の契機で回復）
    ④ 単一定義は `IndexInputs`（config_watcher の kick 判定 + `complete_index_drain` の re-diff で共有）。`needs_reindex` と in-flight `needs_rebuild` を削除
- **Phase 3 — ドキュメント同期**: `snotra-core/CLAUDE.md`（migemo 二重メンテ記述の更新）/ `src-tauri/CLAUDE.md`（drain 機構）/
  `.claude/rules/*` / **`SPEC.md`（設定の即時反映に関する記述。特に top_n_history 変更が再起動不要になる挙動変更を同期）** /
  `docs/architecture.md`「設定管理」節に StaleSet 契約を追記。`docs/architecture.md` に本設計メモへの参照を追加

**検証**: 各 Phase で `docs/build-commands.md` の該当カテゴリ（A: snotra-core test/clippy、B: src-tauri test）を実行。

---

## 9. オープンクエスチョン（合意時に確認したい点）

1. Q1〜Q5 の推奨（単一 bool・show_icons 現状維持・**history を live-read 化して B から除外**・暫定リトライ・SPEC 同期）に同意か
2. **手1（history live-read 化）の API 形**: `save`/`save_if_dirty`/`prune` への `top_n` 引数渡し（フィールド削除）でよいか。
   `HistoryStore::load(top_n)` も引数不要化する破壊的シグネチャ変更を伴う（既存テスト ~数件の更新）
3. Phase 1（history）を #347 本体と分けて先行マージするか、#347 の一括 PR にまとめるか
4. `docs/design/` を本リポジトリの設計メモ常設ディレクトリとして採用してよいか（本ファイルが初出）
