# plan.md — issue #561: コメント一括スイープ

前提: `workspace/research.md`（初期走査）＋ plan-review Step 2b 独立再導出の差分裁定（下記「セルフレビュー」）。**挙動不変・コメントのみ**。対象 ~15 ファイル・すべて独立の小粒編集のため 1 PR とする（レビュー負荷が問題になれば項目単位で分割可——issue が許容）。

## 変更ファイル一覧

### 項目 1 — 非定型ラベル → 定型ラベル（2 箇所）

| ファイル | 変更 |
|---|---|
| `snotra-core/src/binfmt.rs:169` | `// NOTE:` → `// 実装メモ:`（本文英語のまま。tests モジュール内の周辺は英語で、ラベル寄せのみが目的） |
| `snotra-core/src/config.rs:959` | `// NOTE:` → `// 保守注意:` |

`src-tauri/src/main.rs:422` の英語 `// Note:` は**対象外**: 英文中の慣用的な談話標識であり、日本語定型ラベル体系の同義分裂ではない。受け入れ grep（`NOTE:` 大文字）にも掛からない。PR 本文に判断を記載。

### 項目 2 — 日英混在ブロックの単一言語化（13 ブロック・全箇所実読済み）

統一先はすべて**日本語**（各ブロックとも日本語文が本体または周辺の支配的言語。API 名・HRESULT・識別子は原語維持）:

| ファイル:行 | 内容 |
|---|---|
| `src-tauri/src/main.rs:141-155` | `suspend_webview` doc。英語本文を和訳（TrySuspend/IsVisible/0x8007139F 等は原語）。**情報は 1 点も落とさない**。導出は英語統一を推したが、同一ファイルの姉妹コメント（167-172 行・`suspend_and_trim_after_hide` doc）と CLAUDE.md 正準節が日本語のため日本語へ（判断理由を PR 本文に記載） |
| `snotra-core/src/config.rs:1106-1108` | テスト内。英語 1 行目を和訳 |
| `snotra-core/src/search.rs:267-271` | `compute_wave2` doc。英語 4 行を和訳 |
| `snotra-core/src/search.rs:476-479` | Phase 4 incremental。英語 2 行を和訳 |
| `snotra-core/src/search.rs:615-622` | bitmask pre-filter。英語 2 行を和訳 |
| `snotra-core/src/search.rs:663-666` | file_name scoring 短絡。英語 2 行を和訳 |
| `snotra-core/src/search.rs:900-904` | `ScoredEntry` doc。英語 1 行目を和訳 |
| `src-tauri/src/icon.rs:35-36, 51-53` | `load` / `get` doc。英語行を和訳（`get` の **read-only 厳守**太字は維持） |
| `src-tauri/src/indexing.rs:10-15` | `start_index_build` doc。英語 2 行を和訳 |
| `src-tauri/src/commands/icon.rs:9-12, 56-57, 63-72` | Step 見出し英語行を和訳 |
| `snotra-settings/src/tabs/backup.rs:111-112` | 英語 1 行を和訳 |
| `src-tauri/src/config_watcher.rs:195-196, 206-207` | 英語行（`Emit visible_rows change.` 等）は直後の emit の**逐語訳でもある**ため削除（項目 4 と交差）。日本語の「IPC 旧名維持」理由は不変 |

`SearchEngine` struct doc（`# Why parallel Vecs`）等の**英語単一言語ブロックは対象外**（混在のみが対象。ガイドラインが正準例に指名しており不変更）。

### 項目 3 — TSDoc 補完（追加 3 / 見送りの分類表を PR 本文へ)

| ファイル | 変更 |
|---|---|
| `ui/src/lib/perf.ts` | 冒頭に `/** */` を追加（`@packageDocumentation` タグ付きのモジュール doc としてファイル冒頭に置く——単一の代表宣言を持たず、浮きブロックは将来の docgen（TypeDoc）が捨てるため。#562 と衝突しない配置。様式は exclusive.ts 準拠の**太字**契約）: 呼び出し順序（`perfMarkInput`→`perfStartSearch`→`perfMarkSearchDone`→`perfMarkRenderDone`、欠落時は黙ってドロップ）/ stale 実行の `perfCancelSearch` 解放義務（怠ると `MAX_PENDING=256` で全 clear・精度劣化）/ requestId は `searchLane.current()` 由来の一意性前提 / `source==="query"` のみ集計 / `ENABLED` ゲート。**全契約は TS 偵察が実コード照合済み**（perf.ts:17-21,74-81,91-92,104-112 / search.ts:186,235 / ResultsSection.tsx:137-139） |
| `ui/src/lib/iconBatch.ts` | 既存 doc（形式説明）に**解放契約**を追記: 返り値 Map の Blob URL 所有権は呼び出し側へ移り、`cache.set()` へ渡すか早期リターン時に全 `revokeObjectURL` しないとリーク（ui/CLAUDE.md の横断規約はそのまま・TSDoc が関数側の正準） |
| `ui/src/lib/lruIconCache.ts` | クラス doc に**解放・staleness 契約**を追記: `set` で所有権が cache へ移る（revoke 責務の所在は**実装時に evict/set/revokeAll の revoke 挙動を実読して**正確に書く）/ `get` は LRU 更新の書き込み相当・`peek` は非更新（26 行目の既存注記をクラス契約へ昇格） |

見送り（分類表として PR 本文に記載): `trace.ts`（DEV ガード義務は ui/CLAUDE.md 既載・重複回避）、`commands.ts`/`interpretQuery.ts`/`truncatePath.ts`/`types.ts`/`search.ts`/`instantCommand.ts`/`launchNotice.ts`（契約は既存の関数・変数単位 TSDoc で担保済み）、`folder.ts`/`tool-selection.ts`（状態の器。規律は search.ts の choke point 側）、`invoke.ts`/`theme.ts`（契約なし）。iconBatch/lruIconCache は issue の粗い事前数え上げ 13 に含まれないが「契約を持つのに薄い」の趣旨に合致するため追加（PR 本文に明記）。

### 項目 4 — 履歴ナラティブ・レビュー残骸の現在形化（逐語訳型は 0 件と PR に明記）

方針: 「なぜ今この形か」の核と #NNN 参照は残し、「以前は/旧 X だった」の変更履歴再現とレビュー向け文言だけを現在形へ書き換えまたは削除する。

| ファイル:行 | 変更 |
|---|---|
| `src-tauri/src/commands/search.rs:43-48` | 「以前は Result だったが…揃えた」→ 現在形の理由へ。**wire 互換性の 2 文と契約系統参照・#434 は残す**（導出は全削除を推したが、wire 互換は現在の契約事実として grep 可能な価値があるため維持） |
| `src-tauri/src/commands/system.rs:19-21` | 「以前は bool で…不一致だった」→「open_settings と同一条件を同一契約で表現（#434）」へ縮約 |
| `src-tauri/src/trace.rs:1-13` | `used to each carry` の履歴文と `called out in the PR description`（レビュー残骸）を除去し現在形へ（委譲構造・seq 単一単調列・#433 は残す） |
| `ui/src/lib/commands.ts:64-67` | 「以前は false で…」→「Err(ERR_INDEXING_IN_PROGRESS) は意図的に無視しユーザー可視挙動を変えない（#434）。他エラーは再送出」へ |
| `ui/src/stores/search.ts:63, 72-73, 154, 311-312, 316, 664, 749-750`（+832 付近は実装時に実読） | 「旧 debounceTimer/leadingFired/nextGeneration/searchGeneration/suppressNextQueryEffectRefresh/query effect」への参照を現在形へ（例: 154 行 → 「leading は isPending() から導出する（別フラグを持たない）」）。経路分離の設計理由・#536/#537 参照は残す |
| `ui/src/stores/instantCommand.ts:13, 59` | 「旧 instantCmdDebounceTimer を統合」を現在形へ。59 行は ownedTimer 契約の再掲（逐語訳寄り）のため行ごと削除可 |
| `ui/src/lib/types.ts:19-20` | 「かつて同名 savedQuery が…」→ 名前分離の現在形理由のみに縮約（#538 は残す） |
| `ui/src/lib/folderNav.ts:3` | `Extracted from stores/search.ts for testability.` → 現在形・日本語（純関数・テスト可能のため stores から分離） |
| `ui/src/lib/interpretQuery.ts:24 付近` | 「旧 handleInstantQueryInput…」参照を現在形へ（SSOT の理由は残す）。導出指摘の 31 行付近の逐語訳疑いは実装時に実読して判定 |
| `ui/src/components/ResultsSection.tsx:30 付近` | 「iconCacheVersion…を廃止し…改善」→ 現在形（per-path 通知ゆえ再評価 O(変更行数)）。実装時に実読 |
| `snotra-settings/src/app.rs:277` | 「（`_frame` 未使用のため挙動不変）」を削除（レビュー向け）。eframe 0.35 API 分割の説明と kittest の理由は残す |

**keep（触らない）と裁定した履歴言及**: `main.rs:167-172`（旧実装の安全弁→現機構の設計理由）、`folder.rs:57-58`（symlink 不変条件）、`query.rs:57-58,79`（DRY/SSOT 由来・簡潔）、migration 系の「旧キー」記述（`config.rs:676-748`・`history.rs:16`・`window_data.rs:8` — オンディスク旧形式は**現在の挙動そのもの**）、`config_watcher.rs`/`MainApp.tsx` の IPC 旧名維持（生きた不変条件）、`icon.rs` #522・`search.rs` `# Why parallel Vecs`・`instant.rs` #394（歴史メモの規範例）。

## 実装順序

1. **Phase 1 — Rust**: 項目 1（2 箇所）→ 項目 2 の Rust 分 → 項目 4 の Rust 分。ファイルごとに編集し hook（clippy＋crate テスト）の沈黙を確認
2. **Phase 2 — TS**: 項目 4 の TS 分 → 項目 3（perf.ts / iconBatch.ts / lruIconCache.ts）。hook（typecheck）の沈黙を確認
3. **Phase 3 — 受け入れ検証**（下記）→ コミット

## 不変条件

- **コードの挙動・シグネチャ・テスト・文字列リテラルに一切触れない**: diff がコメント行（`//` `///` `//!` `/** */`）のみ。新たな状態・リソースは導入しない（失敗モードなし）
- **情報を落とさない**: 和訳・現在形化は様式変換であり、issue 番号・実測値と条件・契約要素は変換後も残る（keep 一覧が対照表）
- **正準指名されたコメントを壊さない**: ガイドラインが実例指名する `SearchEngine` struct doc・config.rs:568 既存 `保守注意:`・icon.rs #522 群・app.rs/common.rs のラベル実例は不変更（独立導出が重ならないことを確認済み）
- **識別子・イベント名・エラー定数はコメント内でも正確に保つ**（grep 検索性）

## テスト方針（受け入れ条件の検証）

1. 項目 1: `NOTE:|HACK:|XXX:|FIXME:` grep 0 件（`*.{rs,ts,tsx,mts,cts}`）
2. 項目 2: 列挙 13 ブロックが単一言語（編集後に各ブロック実読）
3. 項目 4: 対象箇所の書き換え後、`以前は|used to|previously|かつて|旧 \`` の grep 再走査で残余ゼロ（keep 裁定分を除く）
4. diff 検査: `git diff` に非コメント行の変更が無いことを目視確認（最重要ゲート）
5. hook: `.rs` → clippy＋crate テスト、`.ts`/`.tsx` → typecheck。**沈黙 = 合格**。追加の手動コマンドは不要

## SPEC.md 更新要否

**不要**。挙動変更なし。SPEC.md は対象コメントを正準参照していない（Rust 偵察が grep で確認済み）。

## セルフレビュー

### plan-review 結果の統合（Step 2 + 2b）

- Rust 偵察: 要対処なし。軽微 1 件（main.rs:167 の keep 一覧漏れ）→ research.md へ追記済み
- TS 偵察: 要対処なし。軽微 1 件（perf.ts TSDoc の配置様式）→ 上記表で「ファイル冒頭のモジュール概説」と明示して解消
- 独立再導出との差分:
  - **漏れ（導出 ∖ plan・採用)**: 混在ブロック 12 件・履歴ナラティブ約 10 件・TSDoc 候補 2 件（iconBatch/lruIconCache）→ 全箇所をメインエージェントが実読裏取りの上で本計画に反映（初期走査エージェントの網より導出の走査設計——コメント行ブロック化スクリプト＋語彙 grep「旧」——が細かかった）
  - **不一致（裁定)**: perf.ts の契約有無 → TS 偵察の実コード照合を採り**追加**。main.rs doc の統一先 → **日本語**（姉妹コメント・CLAUDE.md 整合）。commands/search.rs の wire 互換 2 文 → **残す**。main.rs:422 `Note:` → **対象外**
  - **一致（完全性の証拠)**: 項目 1 の 2 箇所・trace.ts 見送り・invoke/theme 契約なし・trace.rs/system.rs/commands.ts の残骸判定・keep 裁定の大半・SPEC 更新不要・スコープ外遵守は独立に再一致

### 5b の 3 観点

1. **境界条件**: コメントのみの変更ゆえ実行時境界はない。編集上の境界は (a) doc コメントのコードフェンス・`#[doc]` 混入 → Rust 偵察が grep で 0 件を確認済み、(b) コメント行とコード行の隣接（commands.ts:68 等）→ テスト方針 4 の diff 検査がゲート、(c) 正準指名コメントとの重なり → 不変条件に列挙・確認済み
2. **シンプル化の挑戦**: 新規状態・機構の導入はゼロ。「lruIconCache の契約文」は revoke 挙動の実読前に書かない（嘘のコメントは無いより悪い——実装時実読を明記）。導出の弱い候補（folder.ts/tool-selection.ts の module doc）は重複リスクに対し価値が薄く**削った**
3. **破壊不変条件 + 検知手段**: 「挙動不変」が唯一の破壊不変条件。検知手段: PostToolUse hook の clippy＋全 crate テスト＋typecheck（自動・沈黙=合格）と `git diff` の非コメント行ゼロ目視（手動・Phase 3）。「戻ってこない」系リスクは該当なし
