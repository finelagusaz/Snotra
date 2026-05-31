# plan.md — issue #337 kana_lower_names を migemo 有効時のみ常駐

## ゴール / 受け入れ条件

1. migemo **無効**時、`SearchEngine` の `kana_lower_names` は**空 Vec**（メモリ ~0）。50k 件で ~2.0〜2.7MB 削減
2. migemo **有効**時、従来通り全エントリ分の kana を eager 構築し、ローマ字検索が機能する
3. migemo OFF で構築→ON で検索しても **panic しない**（空ガード）。kana マッチは出ないが安全
4. migemo トグル（config.toml 変更）で kana の構築状態が追従する（reindex 経由）
5. `cargo test -p snotra-core` / `cargo clippy -- -D warnings` green。既存テスト無改変で通る

## 設計判断（research.md の未解決疑問への結論）

### D-1: 構築 API は `new` 据え置き + 明示フラグ版を追加
- `SearchEngine::new(entries)` は **kana を常時構築**（= `new_with_migemo(entries, true)` の薄いラッパー）。テスト・ベンチ・convenience 用として据え置く → **search.rs の ~70 テスト・既存ベンチを無改変で維持**
- 本番経路だけが config 由来の `migemo_enabled` を流す:
  - `SearchEngine::new_with_migemo(entries, migemo_enabled)`（新規 pub）
  - `SearchEngine::new_with_cached_masks(..., migemo_enabled)`（引数追加）
  - `PrebuiltIndex::new(entries, migemo_enabled)`（引数追加）
  - `Engine::new` / `Engine::new_from_cache` は手元の `config.search.migemo_enabled` を導出して渡す
- 理由: 70 箇所の機械的改変はレビュー負荷・取りこぼしリスクが高い。本番経路は必ず明示フラグを通るので最適化は確実に効く。`new` の既定 true は「フル機能エンジン」として妥当で、全既存テスト（migemo ON/OFF 双方）が無改変で通る

### D-2: migemo トグルは reindex に乗せる（対称に閉じる）
- `config_watcher::needs_reindex` に `old.search.migemo_enabled != new.search.migemo_enabled` を追加
- `indexing.rs` のロック内キャプチャタプルに `migemo_enabled` を追加し、`PrebuiltIndex::new` へ渡す。in-flight `needs_rebuild`（L79-87）にも `cfg.search.migemo_enabled != migemo_enabled` を追加
- 理由: 本変更前は migemo トグルが即時反映されていた。eager-at-construction にすると「ON にしても次の reindex まで無反応」というサイレント後退が出る。既存の reindex machinery を再利用して「トグル→再構築→kana 追従」を保証する（新規状態を増やさない＝KISS）。disk 再スキャンを伴うが show_icons / show_hidden_system トグルと同コスト水準で、migemo 切替は稀

## 変更ファイル一覧

### 1. snotra-core/src/search.rs
- `compute_wave1(entries)` → `compute_wave1(entries, migemo_enabled: bool)`。最内 join の kana 側を `if migemo_enabled { entries.iter().map(...to_kana...).collect() } else { Vec::new() }` に変更
- `new(entries)`: `Self::new_with_migemo(entries, true)` に委譲
- `new_with_migemo(entries, migemo_enabled)`（新規 pub）: `compute_wave1(&entries, migemo_enabled)` → `compute_wave2` → `assemble`
- `new_with_cached_masks(..., migemo_enabled: bool)`: v4 パスの kana 再計算（`par_iter`）を `if migemo_enabled { ... } else { Vec::new() }` に。v3 フォールバックは `compute_wave1(&entries, migemo_enabled)`
- `assemble`: kana の `debug_assert!` を `(kana_lower_names.is_empty() || kana_lower_names.len() == entries.len())` に緩和（他 5 Vec は従来通り == entries.len()）
- 検索ループ kana スコア（L564-571）: par_iter 前に `let kana_available = !self.kana_lower_names.is_empty();`（Copy bool）を計算し、`if primary_score.is_none() && kana_available { ... }` でガード。`self.kana_lower_names[i]` へ到達する前に空を弾く
- ドキュメントコメント更新: `kana_lower_names` フィールド（L114-116）・`compute_wave1`（L143-149）に「migemo 有効時のみ構築。無効時は空 Vec」を明記

### 2. snotra-core/src/engine.rs
- `PrebuiltIndex::new(entries)` → `PrebuiltIndex::new(entries, migemo_enabled: bool)` → `SearchEngine::new_with_migemo(entries, migemo_enabled)`
- `Engine::new`: `SearchEngine::new_with_migemo(entries, config.search.migemo_enabled)`
- `Engine::new_from_cache`: `new_with_cached_masks(..., config.search.migemo_enabled)`
- `replace_entries`（test）: `SearchEngine::new(entries)` のまま（kana 構築・テスト用途で問題なし）

### 3. src-tauri/src/indexing.rs
- ロック内キャプチャタプルに `engine.config().search.migemo_enabled` を追加（`migemo_enabled` 変数）
- `PrebuiltIndex::new(entries)` → `PrebuiltIndex::new(entries, migemo_enabled)`
- in-flight `needs_rebuild` に `|| cfg.search.migemo_enabled != migemo_enabled` を追加

### 4. src-tauri/src/config_watcher.rs
- `needs_reindex` に `|| old.search.migemo_enabled != new.search.migemo_enabled` を追加

### 5. ドキュメント同期
- `snotra-core/CLAUDE.md`: モジュール構成 search.rs 節 + 「5 箇所同時更新」節に「kana_lower_names は migemo 有効時のみ構築（無効時は空 Vec、検索ループは空ガード）」を追記
- `.claude/rules/snotra-core-search.md`: 「Wave 1/2 変更」項に kana 条件付き構築を追記
- SPEC.md: **更新不要**（検索時挙動・IPC 契約・incremental 不変条件は不変。kana 常駐の有無・migemo トグルの reindex は実装詳細で SPEC の文書化範囲外）。AGENTS.md step 0 判定: 文書化された挙動を変えないため「バグ/最適化」側、仕様変更ではない

## 実装順序（フェーズ）

1. **Phase 1（snotra-core, TDD）**: search.rs の構築 API・ガード・assert を変更。先に Red テストを追加
   - `migemo_disabled_build_leaves_kana_empty`: `new_with_migemo(entries, false)` 後、migemo ON 検索で空＆ panic なし
   - `migemo_enabled_build_matches_kana`: `new_with_migemo(entries, true)` 後、`dokyu`→`ドキュメント` ヒット
   - `kana_index_follows_migemo_on_off_on`: true→false→true の各構築で kana 挙動が追従（状態遷移、必須条件 #3）
   - engine.rs: `apply_prebuilt_index_rebuilds_kana_per_migemo`: migemo OFF で構築した Engine に `PrebuiltIndex::new(entries, true)` をスワップ→ migemo 有効 config で `search` がローマ字ヒット（必須条件 #1 の eager 経路検証）
2. **Phase 2（src-tauri 配線）**: indexing.rs / config_watcher.rs に migemo を追加
   - config_watcher.rs: `needs_reindex_migemo_change` テスト追加（既存 `needs_reindex_*` パターン）
3. **Phase 3（ベンチ, `#[ignore]`）**: search.rs に migemo on/off 構築コスト比較ベンチ
   - `bench_new_migemo_on_off`: 50k 件で `new_with_migemo(.., true)` vs `(.., false)` の構築時間を Instant で計測（kana スキップ分の差を可視化）
   - `apply_prebuilt_index` のスワップは O(1) ムーブ（ロック保持時間は migemo 状態に依存しない）ことをコメントで明記。「ロック保持時間」観点の検証はこの O(1) 性の確認＋構築コストがロック外であることの再確認
4. **Phase 4**: clippy・ドキュメント同期・全テスト

## 不変条件

- **スコア階層不変**: Prefix(10000) > Substring(5000) > Kana(4500) > Path(3000) > Fuzzy。kana を「作らない」だけで、作ったときのスコアリングは不変
- **並列 Vec 長**: kana 以外（lower_names / lower_file_names / normalized_keys / char_masks / file_name_char_masks）は常に == entries.len()。kana のみ {0, entries.len()} を許す。これは `assemble` の debug_assert と検索ループの空ガードの 2 点で担保
- **kana_available の単調性（リソース・状態の対称性）**: kana は構築時に一度だけ {空, full} が決まり、SearchEngine の生存中は変化しない（`update_config` は再構築しないため）。空ガードは「構築時の migemo 状態」を反映する。migemo 状態の変化は SearchEngine の再構築（reindex / 再起動）でのみ kana に反映される ← D-2 でトグル→reindex を保証
- **異常系**: 
  - migemo ON 構築中に panic/中断 → 次回起動で config に従い再構築（kana は永続化されないため毎回再計算）。回復不能状態に固まる経路はない
  - reindex 中に migemo トグル → indexing.rs in-flight needs_rebuild が拾い再ビルド。config_watcher 側は indexing_in_progress=true で start を見送るが、in-flight 検出が補完する（二重起動しない）
- **incremental cache 非干渉**: `kana_monotonic` / `prev_kana_query` は `kana_query`（クエリ文字列）のみ参照し `kana_lower_names` に触れない。kana 空でも incremental 述語の単調性は保たれる（kana マッチが 0 件になるだけで候補集合は縮小方向）

## テスト方針

- snotra-core: 上記 Phase 1 ユニットテスト（状態遷移含む）。既存 ~70 テストは無改変で通ること（D-1 の検証）
- src-tauri: config_watcher needs_reindex の migemo テスト。indexing.rs は Win32 / AppHandle 依存でユニットテスト前提にしない（AGENTS.md 環境制約）→ ロジック（needs_reindex）は config_watcher 側でテスト
- ベンチ: `cargo test -p snotra-core bench_new_migemo -- --ignored --nocapture` で手動確認
- 検証コマンド（docs/build-commands.md カテゴリ準拠）:
  - Rust 変更 → `cargo test -p snotra-core`、`cargo clippy --all-targets -- -D warnings`、`cargo build`（src-tauri 含む）
  - 全体ビルド: `cargo build` で src-tauri 配線の型整合を確認

## SPEC.md 更新要否

**不要**。理由は「変更ファイル一覧 5」に記載。検索アルゴリズム・スコア・incremental 不変条件・IPC 契約は不変。kana 常駐有無は内部メモリ最適化、migemo トグル→reindex は既存 reindex トリガー群への追加（SPEC §6 はトグルの存在を述べるが反映機構は規定しない）。

## 実装チェックリスト（落とし穴 — plan-review/symmetric-check 由来）

実装時に以下の「同時修正が必要な組」を取りこぼさない:

- [ ] **kana 構築ゲートの2サブパス**: `new_with_cached_masks` は v4（`par_iter` 再計算）と v3 フォールバック（`compute_wave1`）の**両方**を migemo ゲートする。v4 だけ直して v3 を忘れると非対称
- [ ] **indexing.rs の開始↔完了の対**: ロック内キャプチャタプル（L32-41）に migemo 追加 ⇔ in-flight `needs_rebuild`（L79-87）の比較に migemo 追加。**両方そろえる**（片方だけだとビルド中トグルを取りこぼす）
- [ ] **reindex トリガーの2サイト**: `config_watcher::needs_reindex`（L208-213）⇔ indexing.rs in-flight `needs_rebuild`（L79-87）。両方に migemo
- [ ] **assemble の assert**: kana のみ `{0, entries.len()}` 許容に緩和。他 5 Vec は `== entries.len()` のまま（文字列メッセージも整合）
- [ ] **kana_available の借用**: `let kana_available = !self.kana_lower_names.is_empty();` を par_iter 前に計算し Copy bool としてクロージャに move。`self.kana_lower_names` への可変借用をクロージャに持ち込まない（borrow checker で compile-time 検出されるが意識する）

## IndexCache / CachedMasks への影響

**不変（バージョンバンプ不要）**。理由: `kana_lower_names` は**永続化されない**派生データで、起動時に entries から毎回再計算される（`new_with_cached_masks` の v4 パス）。本変更は「再計算するか否か」を migemo で分岐するのみで、IndexCache のフィールド・`CachedMasks` 構造体・`INDEX_CACHE_VERSION` は変えない。新規フィールド追加ではないため snotra-core/CLAUDE.md の「IndexCache バージョン変更チェックリスト」「5 箇所同時更新」（=フィールド追加時のルール）は非該当。

## migemo ランタイムトグルの確証（要対処解消）

snotra-settings は migemo を UI 公開している（`snotra-settings/src/tabs/search.rs:97` の `checkbox(&mut config.search.migemo_enabled)`）。よってユーザーは UI からトグルでき、config.toml 書込→config_watcher 検知→（D-2 により）reindex→kana 追従という経路が実在シナリオ。**D-2 は実在の UX 後退（チェックを入れても無反応／外しても省メモリが効かない）を防ぐ正当な修正**であることを確認済み。

## 追加テスト（cache-check 由来）

Phase 1 に追加:
- [ ] `incremental_with_kana_disabled_build_no_panic`: `new_with_migemo(entries, false)` 構築 → `migemo_config()` で "do"→"doku" 逐次検索。panic せず、各結果が kana-off fresh エンジンと一致。**新状態「kana 空 + migemo-on-search + incremental」の安全性を固定**

## セルフレビュー

1. **対称コードパス**: /symmetric-check 実施。(A) kana 構築の全経路（new/new_with_migemo/v4/v3）一貫ゲート (B) 構築側↔検索側ガード (C) needs_reindex↔in-flight needs_rebuild (D) 開始キャプチャ↔完了比較 — 4 ペル全て [適用] として計画に反映。kana Vec のライフサイクルは Rust 所有権で自動対称 [不要]。ON→OFF 方向（メモリ即時回収）も `!=` 比較でカバー
2. **影響範囲の網羅性**: grep 済み。`PrebuiltIndex::new` は indexing.rs:72 のみ。`apply_prebuilt_index` も同所のみ。ライブエンジン構築は「起動時 + start_index_build」の2系統のみ（背景再スキャン main.rs:656 は index.bin 保存+アイコン無効化のみでライブエンジン非再構築）。`start_index_build` の全呼び出し元は indexing.rs 単一実装に集約
3. **境界条件**: kana 空（0 件 entries で 0==0）/ migemo off構築→on検索（panic ガード）/ incremental×kana空（追加テスト）/ ON→OFF→ON 状態遷移（Phase 1 テスト）
4. **リソース管理**: kana Vec は所有権で自動 drop。新規 AtomicBool・listen・子プロセスなし。reindex 中トグルの二重起動は `try_begin_index_build` CAS で防止
5. **既存パターンとの整合**: needs_reindex への設定追加は show_icons/show_hidden_system と同パターン。eager 構築は既存 compute_wave1 の延長。新規状態を増やさない
6. **YAGNI 違反**: なし。必須条件 1〜4 に過不足なし。lazy 化（issue が否定）を避け eager のまま。D-1 で汎用化せず最小コンストラクタ追加
7. **シンプル化の挑戦**: D-1 で「`new` シグネチャ変更（~70 テスト改変）」より「`new` 据え置き+本番経路に明示版」を選択し churn とリスクを最小化。D-2 は新規状態を導入せず既存 reindex 機構を再利用。「migemo OFF 構築 → ON 検索」の失敗系は空ガードで panic 回避、reindex で追従と明記済み
8. **破壊不変条件**: 「`kana_lower_names[i]` への空 Vec index アクセス」が唯一の panic 経路 → `kana_available` ガード（Phase 1 テスト `migemo_disabled_build_leaves_kana_empty` で検証）。Win32 フック等の「戻ってこない」系リスクはなし（純ロジック + config 配線のみ）

**plan-review 総評**: snotra-core completeness 高、src-tauri completeness 中→（本セクションの追補で）高。実装着手可。
