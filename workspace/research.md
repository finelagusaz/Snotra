# research — issue #1241

## issue の要約

`.claude/rules/snotra-core-search.md` L17 が `snotra-core/CLAUDE.md` の「文字ビットマスク」節を指すが、その名の見出しは無い（現在の見出しは `### `char_bitmask` は `query.rs` に一元化済み`・L111）。「」だけの散文形は正準形（`` `<対象>`「<見出し>」 ``）ではないため G-heading-refs の射程外で、改名が捕まらなかった。対処案: L17 の節名を正準形へ書き直し、以後は機構が見るようにする。同ファイルの他行も grep する。

ラベル: type:fix / size:S。#1240 の worktree 委譲が見つけた既存の腐りで、#1240 の差分では生じていない。

## 関連ファイル・シンボル（すべて実在確認済み・2026-09-06 main `e1531e0`）

- `.claude/rules/snotra-core-search.md` L17（対象行）。同ファイルで「」を含む行は L9 と L17 の 2 行だけで、L9 は節名ではなく散文の引用（「どこを読むか・何を実行するか」）
  - L17 が名指す 3 節: `search.rs` 節 / 「文字ビットマスク」節 / 「incremental cache とパスクエリの非互換」節
  - **同ファイル L24**（`Ord` / `BinaryHeap` の行・ファイルは全 24 行）も `snotra-core/CLAUDE.md` 実装前チェック を散文形で指す。「」は無いが同じ型（機構の射程外の節名参照）
- `snotra-core/CLAUDE.md` の見出し（実在）:
  - L8 `## モジュール構成` — `search.rs` の記述はこの節内の箇条書き `- `search.rs` — …`（L?・見出しではない）
  - L79 `## 実装前チェック（必須）`
  - L111 `### `char_bitmask` は `query.rs` に一元化済み` ← 「文字ビットマスク」の現在名
  - L125 `### incremental cache とパスクエリの非互換` ← 現存・名前一致
- 判定機構（`scripts/governance/`）:
  - `lib.mjs` `REF_HEAD` = `` `([^`\n]+)`\s*(?:§\s*[\d.]*\s*)? `` — 対象バッククォートの直後（空白・§ のみ許容）に `「` が続く形だけが正準形
  - `lib.mjs` `HEADING_REF` — ラベル内に 1 段の入れ子「」を許す。バッククォートは許容
  - `lib.mjs` `normAnchor` = `` s.replace(/[`*「」\s]/g, "") `` — 照合前にバッククォート・`**`・「」・空白を剥ぐ。照合は**前方一致**（`G-heading-refs.mjs` `scanHeadingRefs`）
  - `lib.mjs` `ANCHOR_SPECS` — 着地先は ATX 見出し / 番号付きリスト / `**太字リード**` の箇条書き / `describe`・`it` の 4 種。**`- `search.rs` — …` の形（バッククォート始まりの箇条書き）は着地先にならない**
  - `G-near-heading-refs.mjs` — 窓幅 8・着地必須。L17 の 2 件はどちらも `` `snotra-core/CLAUDE.md` `` から `「` までの隔たりが 14 文字で `NEAR_REF`（gap 1〜8）に一致せず、着地判定に到達する前に沈黙する（3b で gap を 16 まで緩めて初めて一致することを実測）
  - `.claude/rules/*.md` は走査元に含まれる（`headingRefDocs` は `.md` 全般から `docs/adr/` 等を除外する形。ベースライン出力「md 48 件」に rules 8 件が入る）

## 再利用できる既存パターン

- ラベル内にバッククォートを持つ正準形の実例: `docs/adr/ADR-autostart-state-ownership.md` L53 `` `snotra-settings/CLAUDE.md`「本体との連携は `config.toml` ファイル1点のみ」 ``、`.claude/skills/*/SKILL.md` L165 の `docs/build-commands.md` 参照。**normAnchor がバッククォートを剥ぐので、`「`char_bitmask` は `query.rs` に一元化済み」` は照合可能**
- 見出しの後置注記を省いた前方一致の実例: `AGENTS.md`「条件別チェック（トリガー → 参照先）」を `AGENTS.md`「条件別チェック」で指す箇所（`.claude/rules/src-tauri.md` L27 等）。→ 「実装前チェック（必須）」は `「実装前チェック」` で着地する
- 節ではなく箇条書きを指したいときの既存の書き方: 正準形で節を指し、行は散文で添える（例: `snotra-core/CLAUDE.md` L? 「依存の向きは下の `opener.rs` 節」は同一文書内の散文形）

## 技術的制約

- 正準形にできるのは**アンカー種に当たる行**だけ。`search.rs` 行は箇条書きの本文なので、`` `snotra-core/CLAUDE.md`「モジュール構成」の `search.rs` 行 `` の形にする（節は機構が見て、行は散文で補う）
- ベースライン: `npm run governance:check` は main `e1531e0` で全 24 検査 passed・見出し参照 371 件照合。編集後は**件数が 371 → 371+N（N = 新設した正準形の数）に増えること**を「機構が見るようになった」の証拠にする（G-heading-refs は `checked` を返す・#497）
- `.claude/rules/*.md` の編集は PostToolUse 検査ゼロ（`selectChecks` 空集合）。reminder は鳴りうるが沈黙は「何も走らなかった」（`.claude/rules/governance-docs.md` L21）。**決定的な検査は `npm run governance:check` を手で走らせる**
- rules は「セーフティネット」母集団（`AGENTS.md` 条件別チェック表）——ただし今回は参照の綴りの訂正のみで、判定・射程・行動形は変えない。`safety-nets.md` のフォールトインジェクション条項は「機構が効いているか」に当たるもので、参照の綴り直しには適用外。**ただし「直した参照が本当に機構に見られているか」は、見出しを一時的に壊して赤くなることで 1 度測る**（費用は 1 コマンド）
- 訳語: 「文字ビットマスク」は旧見出し名の残骸であり、正準形へ直す際は現在の見出しを逐語で写す

## 未解決の疑問

- **Q1. L24（`実装前チェック` の散文形）も同 issue で直すか。** issue 本文は「「」形の節名を grep する」だが、腐る機序は同じ（機構の射程外の節名参照）。同ファイル内・1 行・同型ゆえ**直す側へ倒す**（size:S の範囲内。plan で明示）
- **Q2. `.claude/rules/snotra-core.md` L12〜L18・L24 の「」だけの節名（L8 で `snotra-core/CLAUDE.md` を宣言し、以降は 「見出し」 のみ）は同型の腐りを持ちうるが、issue の対象は `snotra-core-search.md` 同ファイルに限られる。** 現時点で全 7 件が実在の見出しに着地することを目視で確認済み（「実装前チェック（必須）」「`normalize_entry_key` の冪等性契約」「history.rs の…」「`char_bitmask` は…」「index.bin 書き込みの排他」「indexer.rs の索引更新の契機」「`scan_all` の重複排除」「開発ルール」）。**今回は触らず、別 issue に切り出す候補として plan に記す**（構造上「1 文書を宣言して以後は見出しだけ」という書式そのものが射程外で、直すなら書式の議論が要る）
- Q3. 正準形の行が長くなり、物理改行を跨ぐと G-folded-heading-refs の対象になる。1 物理行に収める（`.claude/rules/governance-docs.md` L18）

## 敵対的調査（3b）の反映

sonnet 1 体・出力 workspace/adversarial-1241.txt。

| # | 所見 | 判定 | 採否 |
|---|---|---|---|
| 1 | Ord / BinaryHeap の行は L27 ではなく L24（ファイルは 24 行） | 壊せた | 採用・訂正済み（自分で行数を数えて確認） |
| 2 | G-near-heading-refs の沈黙は 2 件とも「窓幅超過」で、「着地しないため」の書き分けは誤り | 壊せた | 採用・訂正済み（NEAR_REF の gap 1〜8 を自分で読んで確認。所見の結論と機序が一致） |
| 3〜8 | rules が走査元に含まれる / バッククォート付きラベルが着地する / search.rs 行は着地先にならない / ベースライン一致 / snotra-core.md の 7 件は現存 / 「」行は L9・L17 のみ | 壊せなかった | そのまま |
| 9 ⚠️ | checked の増分は isRefTargetSpelling 通過時に数えるだけで、着地の証拠にはならない | 確信なし | 採用——受け入れ条件は「件数増 ＋ 全検査 passed ＋ フォールトインジェクションで赤」の 3 点を揃えて初めて成立と plan に明記 |
| 10 ⚠️ | G-folded-heading-refs の懸念は未実測 | 確信なし | 1 物理行に収めるので測らない（懸念を避ける側へ倒す） |
