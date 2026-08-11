# 調査 — issue #1034（#1004 で判明した計器の制約 2 件を横断的な正本へ吸収する）

## 1. issue の要約

#1004（検索を worker へ出す）で計器を作って実測した結果、**次に計器を作る人が必ず踏む制約が
2 つ**、および**故障注入の作法に関する教訓が 1 つ**得られた。いまそれらは #1004 の spec
（`docs/superpowers/specs/2026-08-10-search-worker-design.md`）にしか無く、あれは #1004 の
判断の記録であって横断的な正本ではない。**置き場を決めて吸収する**のが本 issue である。

| # | 事実 | issue が挙げる候補 |
|---|---|---|
| 制約 1 | trace の書き込みは 1 本あたり約 10 ms（同期 write） | `PERFORMANCE.md`「計測と受け入れ基準」/ `docs/build-commands.md` / `src-tauri/src/trace.rs` の `//!` |
| 制約 2 | smoke は常に 1 件の索引を seed する | `docs/build-commands.md` の smoke の節 |
| 教訓 3 | debounce が worker の追い越しを構造的に防ぐ（＝検知器は人工的にでも一度発火させないと発火しうるか分からない） | `docs/development-principles.md` か `.claude/rules/safety-nets.md`（どちらが正本かを決める） |

加えて **#1004 の spec からは参照へ置き換える**（同じ事実を 2 か所へ書かない）。

## 2. 一次証拠の確認（2026-08-11 にこのセッションで測った）

### 2.1 制約 2 の裏取り — smoke の索引は 1 件で、上書きする引数は無い

- `scripts/smoke-egui.ps1` の `param()` は **`ExePath` / `StartupWaitMs` /
  `ObserveTimeoutMs` / `HotkeyVks` / `ResultsQuery` / `PostMortemWaitMs` /
  `StartupObserveTimeoutMs` の 7 つ**。scan 対象を上書きする引数は無い（実読）
- scan は `scripts/smoke-egui.ps1:98` の `$scanDir = Join-Path $env:TEMP "snotra_smoke_scan"`
  で固定され、その下に `zsnotrasmoke.exe` を 1 つだけ置く（`:100`）。TOML へは
  `[[paths.scan]] path = "$scanDirToml"` として渡る（`:114-115`）
- **「索引は 1 件」は既に 2 か所に書かれている**——`scripts/smoke-egui.ps1:109` のコメント
  （「scan は上の 1 ファイルだけを対象にする（索引は 1 件・ビルドは即座に終わる）」）と、
  `docs/build-commands.md`「スモーク運用メモ」の「既定クエリ `"z"` が seed した索引 1 件に
  必ず一致する」

→ **制約 2 で新しく書くのは事実ではなく帰結だけである。** 事実を書き足せば 3 枚目の写しになる。

### 2.2 制約 1 の写しの母集団（生きた文書に限る）

想定した書き方（`同期 write` / `1 本あたり` / `10〜18 ms` / `17〜56`）と、緩いパターン
（`trace` と `ms` の共起・`eprintln|stderr`）の 2 通りで数え、突き合わせた。件数は一致した。

| 所在 | 内容 | 扱い |
|---|---|---|
| `PERFORMANCE.md:484-488`（A 側の但し書き） | 「1 本あたり約 10 ms・同期 write・10〜18 ms」全文 | **参照へ落とす**（表を読む但し書きは 1 文残す） |
| `PERFORMANCE.md:550`（#1032 の帰属） | 「（1 本約 10 ms）」 | **数値を落とし参照にする**（50〜96% は #1032 固有ゆえ残す） |
| `docs/superpowers/specs/2026-08-10-search-worker-design.md:49, :53` | 判断（H6 取り下げ）の根拠として全文 | **結論と理由の骨を残し、数値は参照へ** |
| `PERFORMANCE.md:1668`（`SNOTRA_EGUI_INPUT_TRACE` の項） | runner で 17〜56 ms/行 | **触らない**（別の計器・別の観測点。新しい正本から相互参照する） |
| `PERFORMANCE.md:1707` | 上の項を引く | 触らない（1668 を残すので真のまま） |
| `snotra-egui-runtime/src/input.rs:26, :136` | runner 17〜56 ms（全フレームに出さない理由） | 触らない（当該コードの「なぜ」であり、run 番号つきの一次証拠） |
| `docs/superpowers/plans/2026-08-10-search-worker.md:478, 676, 684, 1170` | 同じ数値 | **触らない**（→ §5 の前提） |

`.superpowers/sdd/**`（過去セッションの作業成果物）と `target/fault-inject/**`（ビルド生成物）
にも当たるが、いずれも生きた文書ではないので母集団外。

### 2.3 教訓 3 の隣接記述

- `docs/development-principles.md`「構造的設計原則と強制の階梯」に
  **「故障注入は、本来の回帰より強い変異にしてはならない。注入が赤くなったことは、検査が当の
  回帰を捕まえる証拠にならない」**（#872）がある。今回の教訓は**その逆向き**（注入が赤く
  ならなかったことは、検査が縛れていない証拠にならない）であり、同じ bullet の対になる
- `.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」は
  **強さの話の本文を `docs/development-principles.md` へ委譲するルーター**として書かれている
  （「本文は `docs/development-principles.md`「構造的設計原則と強制の階梯」」と明記）
- `scripts/lib/SnotraTraceInvariants.psm1` の H7 の arm には既に
  **「射程は狭い。健全な実装では H7 は構造的に発火しえない」**（`SearchDispatch::accept` が
  pending を take するため）と**「故障注入で発火を実測済み・#1004 PR 2」**がある。
  **これは検知器側の機序であって、debounce が追い越しを防ぐという系側の機序とは別である**
  （issue が記す 2 度の失敗は後者に由来する）

## 3. 関連ファイル・シンボル（実在を確認済み）

- `PERFORMANCE.md`「計測と受け入れ基準」（`:1652`）— 計器の横断規範が既に集まっている節。
  「ランタイムの計測は `SNOTRA_TRACE=1` の構造化トレース（`src-tauri/src/trace.rs`）で行う」
  「egui/softbuffer の計器は 5 つの env …**このリストが計器の正本である**」を持つ
- `docs/build-commands.md`「スモーク運用メモ」（`:208`）— `smoke-egui.ps1` の契約の集約点
- `docs/development-principles.md`「構造的設計原則と強制の階梯」（`:86`）
- `.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」
- `src-tauri/src/trace.rs` の `//!`（`trace()` は `eprintln!` の同期 write・`ts_ms` を出す）
- `.claude/rules/governance-docs.md` — 参照の正準形 `` `<path>.md`「<見出し>」 `` の規約

## 4. 再利用できる既存パターン

- **正本 1 か所 + 参照**: `PERFORMANCE.md`「計測と受け入れ基準」が既に
  「**このリストが計器の正本である**——`docs/build-commands.md` には置かない」と射程を宣言して
  いる。制約 1 をここへ置くのは、その宣言の延長線上にある
- **ルーター + 本文**: `.claude/rules/safety-nets.md` が本文を
  `docs/development-principles.md` へ委譲する形が既にある。教訓 3 も同じ分担に載せる
- **コードからの正準形参照**: `.rs` のコメントに書いた `` `<path>.md`「<見出し>」 `` は
  `governance:check` の G-heading-refs が照合する（#925）。`trace.rs` の `//!` へ置く 1 行は
  この形にする

## 5. 技術的制約・前提

- **`docs/superpowers/plans/` は触らない。** issue の「やること」が名指すのは spec だけであり、
  plans は当時の実行記録である（`ADR-adr-frozen-history` は ADR を対象とする規則なので、
  plans を凍結扱いにする根拠にはならない——**issue の射程に無いことを理由とする**）
- **10 ms の主張には前提条件が要る**: 実測条件は「`SNOTRA_TRACE` 有効・stderr をファイルへ
  リダイレクト」である。前提を落とすと偽の全称になる（`AGENTS.md`「検証の作法（全タスク共通）」）
- **開発機の 10〜18 ms と runner の 17〜56 ms を 1 つの数へ併合しない**（観測点も計器も違う）
- **セーフティネットの変更に当たる**: `.claude/rules/safety-nets.md` と規範文書
  （`docs/development-principles.md`）の変更は、ルート `CLAUDE.md` の最重要ルール 2
  （合意してから）の母集団に入る。/start-issue の人間承認がそのゲートである
- **検証**: `*.md` 編集は PostToolUse hook の対象外（沈黙は「何も走らなかった」）。
  `npm run governance:check`（カテゴリ F）をローカルで走らせる。`trace.rs` を触るなら
  カテゴリ A に加え `cargo doc --workspace --no-deps --document-private-items` を手で走らせる
  （`.claude/rules/comments.md` のトリガー・hook は intra-doc link に沈黙する）

## 6. 未解決の疑問（→ plan.md の「未確定」へ）

1. `/plan-review`「リスク判定」に照らして本計画が高リスクか（ガバナンス文書の変更を含む）
2. 制約 1 の正本を `PERFORMANCE.md` に置いたとき、`src-tauri/src/trace.rs` の `//!` へ
   ポインタ 1 行を足すのは「必要なことだけ」を満たすか（＝やりすぎでないか）
