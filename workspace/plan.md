# plan: issue #522 — アイコン無効化をロック内で原子化

## 変更ファイル一覧

2 ファイル: `src-tauri/src/icon.rs` + `src-tauri/CLAUDE.md`

1. `invalidate_icon_cache_with`（156-163）: guard を先に取得し、保持中に `bf.remove()` → `*g = None`。doc コメント（147-150, 155）を「lock 保持中に削除まで行う理由 = #522 の TOCTOU」で更新
2. 決定論テスト追加（独立導出の提案を採用）: temp dir に旧データ入り icons.bin → `invalidate_icon_cache_with(Some(bf))` → 「ファイル不在 かつ メモリ None」を assert
3. 回帰テスト追加: issue #522 の実証プローブを恒久化した `invalidate_is_atomic_with_concurrent_load`
   - temp dir の `BinFile::new_in` で実 `icons.bin` に非接触。**dir 名に `std::process::id()` を含めて並列テストとの衝突を回避**（Explore 監査の指摘）
   - loader スレッドは `ensure_icon_cache_loaded_if_enabled` の手順を **temp BinFile 上で再構成**する（本物は `icon_bin_file()` 固定パスのため直接呼ばない — 同監査の指摘）
   - loader（lock → None 検知 → ロード）と invalidate を並走 × 1000 回、「ファイル削除済み かつ メモリに旧キー残存」が 0 回であることを assert（修正前の実測ヒット率 0.85%/回 → 1000 回で検出率 ~99.98%）
4. `src-tauri/CLAUDE.md` の icon.rs 説明行: 「両方無効化する」に「両操作は単一 lock 内で原子的に行う（lock 外削除だと並行ロードで旧ファイルが戻る TOCTOU、#522）」を追記（**独立導出が検出した漏れ** — ドキュメント側の同期）

## 実装順序

Red → Green: 先に回帰テストを追加して失敗（数回のヒット）を確認 → 修正 → 合格。単一フェーズ。

## 不変条件

- **「メモリ None」の観測は常に「icons.bin 削除完了後」**（本修正が確立する不変条件）: None 化と削除が同一 critical section に入るため、None を見てロードする側（`ensure_icon_cache_loaded_if_enabled`）が旧ファイルを読む interleaving が存在しなくなる
- 削除 → None の順序: 逆（None → 削除）でも lock 内なら安全だが、「None を書いた時点でファイルは無い」を単文で言える削除先行を採る
- 失敗モード: `bf.remove()`（`fs::remove_file`）失敗は従来どおり黙殺（`let _ =`）。失敗してもメモリは None になり、次のロードは古いファイルを読む——これは**修正前から存在する残余**（削除失敗時）で本 issue のスコープ外。頻度の高い「削除成功時の順序競合」を塞ぐのが本修正
- `bin_file=None` 経路（既存テスト）: guard 内で remove をスキップして None 化のみ。既存テストは変更なしで通る
- デッドロック: guard 保持中に取得する他ロックなし（`fs::remove_file` のみ）。呼び出し元（`main.rs:747`、再スキャンスレッド）も他ロック非保持

## テスト方針

- 追加: `invalidate_is_atomic_with_concurrent_load`（上記。Red で失敗確認 → Green）
- 既存: `invalidate_icon_cache_clears_in_memory_state`（None 経路）が退行しないこと
- 検証: カテゴリ A（clippy + `cargo test -p snotra`、PostToolUse hook）

## SPEC.md 更新要否

不要（内部の無効化順序の是正。外部挙動・IPC 契約に変更なし）。

## plan-review 結果の反映（Step 5a）

- **漏れ（導出 ∖ plan・反映済み）**: `src-tauri/CLAUDE.md` の icon.rs 行更新、決定論テストの追加
- **一致（完全性の能動的証拠）**: 修正方針（lock 内 削除→None）・呼び出し元 1 箇所・デッドロック非成立（`IconCacheState` lock 全 6 箇所を両者が独立監査、engine→icon のネスト保持なし）・remove 失敗時の残余整理・SPEC/e2e 更新不要・**同型横断調査**（`Mutex<Option<T>>` lazy reload + lock 外削除の組み合わせは icon 固有。index.bin/history.bin/window.bin は非該当、プロダクションの `bf.remove()` は icon.rs:161 のみ）— すべて独立に再一致
- **race-check 相当（独立導出が実施）**: 修正後の全 lock 交差地点（ensure_loaded / get_icons_batch Step1/3 / retain_paths / 終了時 save_if_dirty）で状態競合なし

## セルフレビュー

1. 対称コードパス: 無効化の対称は「ロード」（`ensure_icon_cache_loaded_if_enabled`）— 相手方は同一 lock 内で完結しており変更不要（research 記載）。invalidate の呼び出し元は 1 箇所のみ（grep 確認）
2. 影響範囲: `invalidate_icon_cache` 系の全出現（定義 2 + production 呼び出し 1 = main.rs:747 + テスト内 3）を grep で確認
3. 境界条件: bin_file=None / remove 失敗 / 並行ロードの 3 つを不変条件に記載
4. リソース管理: 新規リソースなし。guard は関数スコープ
5. 既存パターン整合: lock 内ファイル I/O は `IconCache::load`（commands/icon.rs:24）で既存。新パターン導入なし
6. YAGNI: 世代カウンタ案（issue 記載の代替）は採らない — lock 内削除で必要十分
7. シンプル化: 3 行の順序変更 + テスト。これ以上削れない
8. 破壊不変条件: 「None 観測 = ファイル削除済み」を doc コメントに明文化し、回帰テストが機構として守る（規範でなく機構）
