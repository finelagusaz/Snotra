# research: issue #588 rules ルーター化の試行（snotra-core-search.md）

## issue の要約

`.claude/rules/snotra-core-search.md`（17 行・8 知識項目）を「引き金・正準参照・必須検査」のルーターへ薄化する試行。手順 1 = 落とす知識の正準側実在を確認してから消す（消失ゼロ）。採用ゲート = 読者故障注入（issue 本文の計測設計 1）。試行期間の観測（計測設計 2・3）は PR 後も継続するため **PR で #588 を閉じない**。

## 旧 rule 8 項目 × 正準側の実在確認（すべて実測）

| # | 旧 rule の項目 | 正準側の所在 | 判定 |
|---|---|---|---|
| 1 | 変更後 `/cache-check` | ルーター内容そのもの + `snotra-core/CLAUDE.md:62`（同旨） | 薄 rule に残す |
| 2 | スコア階層不変・score_tier・const _ | `search.rs` `//!`（5 行目）・`mod score_tier` doc（33-41）・`const _` アサーション（57-62、全ビルド強制） | 実在 |
| 2' | 「9000 は score_tier ではない（PREFIX_BASE と混同しない）」 | **`search.rs:672-673` に実在**（`score_one_entry` 内・9000 の使用箇所へ co-location。2026-07-05 の既存コミット由来）。当初調査は score_tier の doc ブロックだけを見て「欠け」と誤認した——plan-review の偵察と独立再導出が**双方独立に**この誤りを検出した | 実在（補填不要） |
| 3 | BinaryHeap の Ord 逆順・into_sorted_vec | `search.rs:503-507`（fold コメント）+ `snotra-core/CLAUDE.md:53`（明記の規律） | 実在 |
| 4 | 新マッチパス → bitmask pre-filter OR 関係・非 ASCII `u64::MAX` | `compute_wave2` doc（275-279: u64::MAX が常時通過する理由まで記述）・`snotra-core/CLAUDE.md:80-82`（ビットマスク集約節） | 実在（「追加時に確認」は引き金として薄 rule に残す） |
| 5 | Wave 1/2 変更 → ヘルパー更新・3 コンストラクタ共有 | `compute_wave1`/`compute_wave2` doc（225-231）・呼び出し元はコンパイラが検出（compile-visible） | 実在（共有関係はコードから導出可能） |
| 6 | kana 条件構築 #337（同時空/同長・空ガード・pre-filter is_empty・IndexInputs） | `search.rs:331-333`（assemble の debug_assert）・499/636/707-709（ガード実装 + コメント）・`compute_wave1` doc（#337 明記・「検索ループ側ガードは search_with_options 参照」）・IndexInputs は engine + `src-tauri/CLAUDE.md` config_watcher 節（単一定義と明記済み） | 実在 |
| 7 | search_with_options 分割 #436・prev_* write は heap 後 | `QueryPlan` doc（197-198）・479-527 コメント・562-564（write 位置の理由）・`decide_incremental` doc（579-587）・`score_one_entry` doc（609-613） | 実在 |
| 8 | has_dot / has_path_sep と incremental の連動 | `decide_incremental` doc（579-587: 両述語の理由を列挙）・597-598（実装）・`snotra-core/CLAUDE.md:92-94`（has_path_sep 非互換の完全な理由） | 実在 |

結論: **8 項目すべて正準に完全実在。補填コミットは不要で、薄化の実質は純粋な重複削除**（旧 rule はほぼ全項目が正準の要約コピーだった）。残余リスクはポインタ追従のみで、それは採用ゲート（読者故障注入）が測る。

補足（独立再導出の発見）: `.claude/rules/snotra-core.md`（`snotra-core/**/*.rs` で配送）にも search.rs 関連の重複項目があり、search.rs 編集時は両 rule が併配送される。薄い rule はこれと重複させない。同 rule の薄化は #588 のスコープ外（横展開判断の材料）。この rule をファイル名で名指しする文書は無く、間接参照の追従は 0 件（grep 実測）。

## 既存パターン

- ルーターの体裁: `.claude/rules/snotra-core.md` 等の「詳細は `CLAUDE.md` を参照 + 太字箇条書き」形式。薄化後は箇条書きから**事実の再記述を排し**、引き金 → 正準参照のみにする
- 読者故障注入: `.claude/rules/safety-nets.md`「規範の故障注入 = 回避しようとする読者」（2 クラス・停止条件事前宣言）。#588 本文の計測設計 1 が採用ゲートとして具体化済み
- governance:check G7 が薄化後も paths glob の有効性を、G3 が正準参照の実在を機械検査する（参照先タイポは PR CI で捕まる）

## 技術的制約

- `search.rs` への 1 行 doc 追記は PostToolUse hook で clippy + snotra-core テストが自動発火（沈黙 = 合格）。コメントのみで挙動不変
- `.claude/rules/` の編集はエージェント設定変更 — issue #588 起票 + 計測設計追記 + ユーザーの着手指示で合意済み。safety-nets.md が自動配送される（paths に `.claude/rules/**` 追加済み・#586）
- **PR に closing keyword を書かない** — 試行期間（search.rs を触る PR 3 件 or 8 週間）中は #588 を open 維持し、計測設計 2 の監査記録簿とする
- 旧 rule 全文は薄化コミットの**前に** issue #588 へコメントとして退避する（計測設計 2 の照合原本）

## 未解決の疑問

なし（読者故障注入の合否基準・停止条件は issue 本文で事前宣言済み）。
