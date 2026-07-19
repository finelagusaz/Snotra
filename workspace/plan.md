# plan: issue #589 検証写像の照合（G9）・事故散文の圧縮・superpowers 非規範化

ブランチ: `chore/589-verification-map-and-prose`。コード挙動の変更なし（governance-check への検査追加 + 文書）。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `scripts/governance-check.mjs` | G9 追加: `post-edit.mjs` ソースから `cargoSpec([...])` の引数配列を正規表現抽出 → 出力整形フラグ許容リスト（`--message-format short`）を除去 → 正規化コマンド文字列がカテゴリ A コードブロック（`docs/build-commands.md`）に出現するか照合。抽出 0 件・カテゴリ A ブロック不在は明示 fail。`runAll` と evidence 行に組み込み |
| `scripts/governance-check.test.mjs` | G9 の故障注入（hook 側フラグ変異・SSOT 側欠落・抽出 0 件 fail）+ 正常 + 許容フラグの不混入検算 |
| `docs/superpowers/README.md`（新規） | 「本ディレクトリは歴史資料（過去の設計書・実装計画のスナップショット）であり現在の仕様ではない。鮮度維持・governance:check の対象外。現在の正準は SPEC.md / docs/architecture.md / 各 `//!`」の宣言 |
| ルート `CLAUDE.md` | 「Git/GitHub 運用」節・「フック」節の事故散文を「太字指示 + 理由 1〜2 文 + #番号」へ圧縮。**節見出し・太字規則の文言は不変**（参照腐敗の構造的回避）。マージ手順 1〜4 は現役手順として保持。退去する散文は issue #589 コメントへ先に退避 |
| `.claude/skills/health-check/SKILL.md` | Check 5 残置記述から cargo フラグ照合を G9 へ移し、残置 = npm 部分集合ラッパー等価 + コマンド直書き grep に縮小 |
| `docs/build-commands.md` | L26 付近「フラグ照合は `/health-check`（Check 5 残置部分）」→「cargo フラグ照合は governance:check（G9）・npm 系等価判断は /health-check」へ更新 |

## 実装順序

1. **Phase 1（README・独立で軽い）**: superpowers README 新規作成
2. **Phase 2（G9）**: テスト先行（Red: フラグ変異フィクスチャ）→ 実装（Green）→ dogfood（実リポジトリで hook 5 コマンドとカテゴリ A の一致を実証）→ health-check / build-commands の追従
3. **Phase 3（事故散文圧縮）**: (a) 参照数え上げ（governance-docs.md 手順: 半角 Step/全角ステップ・引用見出し語句・節名の grep）→ (b) 退去散文を issue #589 コメントへ退避 → (c) CLAUDE.md 圧縮 → (d) `npm run governance:check` 緑 + バイト数の前後を記録
4. 各 Phase でコミット

## 不変条件

- **hook（post-edit.mjs）と ci.yml に触れない**（ユーザー決定）
- **G9 の抽出はソーステキスト正規表現**であり、hook のリファクタで抽出が壊れた場合は「抽出 0 件 fail」で loud に落ちる（沈黙経路なし）
- **CLAUDE.md の節見出し・太字規則文言は不変**: 圧縮は太字に続く散文のみ。governance-docs.md の「名前・序数の両方で数え上げ」を実装時に実施し、引用されている文は残す
- **「なぜ」の一文は必ず同居**: 完全退去しない（#588 設計時の決定・issue 本文に明記済み）
- 退去散文は削除前に issue へ退避（復元可能性）

## テスト方針

- G9: 故障注入（hook フラグ変異→赤・SSOT 欠落→赤・抽出 0 件→赤）+ 実リポジトリ dogfood 緑 + `npm test`
- 圧縮: `npm run governance:check` 緑（参照実在 G3 が圧縮後も守られること）・退去前後の CLAUDE.md 全太字規則の同一性を diff で目視確認（規則の集合が不変であることの検算）
- burn-down: CLAUDE.md と rules のバイト数前後を PR 本文へ記録

## SPEC.md 更新要否

不要（開発ガバナンスのみ）。

## スコープ外

- 共通定義ファイル・生成器の導入（ユーザー確認で不採用）
- AGENTS.md・モジュール CLAUDE.md の散文圧縮（今回はルート CLAUDE.md のみ。効果を見てから拡張）
- vitest/tsc 系コマンドの機械照合（意味判断ゆえ Check 5 残置）
- 設計書 §4 の「写しを増やす変更 → 責務分担表で正準確認」の AGENTS.md 追記は**本 PR に含める**（設計書が #589 と同時実施を指定）→ 変更ファイル一覧に AGENTS.md を追加する

## 追記: AGENTS.md（変更ファイル一覧へ）

条件別チェック表に 1 行を追加（設計書 §4 の合意事項・どの issue にも未割当のため #589 で同時実施）。文言は原理を 1 文で同居させ、歴史資料化される設計書は記録として参照する: 「文書に事実の写しを増やす変更 → 正準を 1 か所に定め他は参照へ（分担の記録は `docs/superpowers/specs/2026-07-19-doc-governance-design.md` §1）」

## plan-review 統合（Step 5a の反映）

- **G9 実装詳細（両者一致 + scout 実測）**: clippy の配列は 3 行折返し＝抽出は `cargoSpec\(\[([\s\S]*?)\]\)` の dotall 必須。照合はトークン列比較（hook 側に `cargo` を前置・出力整形フラグは arity 付き除去リスト `--message-format`+1）。母集団はカテゴリ A の bash フェンス内 `cargo ` 行（行末 `# コメント` 除去）。片方向照合（hook 各コマンド → docs 行の完全一致）で docs 側の変更も一致喪失として検出される。沈黙経路 3 本（post-edit 読取不能・抽出 0 件・カテゴリ A 不在）は明示 fail。現行 5 コマンドは机上照合で全一致（dogfood で実証）
- **間接参照の追従 6 箇所（独立再導出の数え上げ）**: governance-check.mjs ヘッダコメント（「フラグの意味論は /health-check」の cargo 部分）と出力文字列 `G1..G8`→`G1..G9` / build-commands L26 と L74 / health-check SKILL.md の description・冒頭 2 項・Check 5 本文 / ルート CLAUDE.md スキル表 138 行
- **CLAUDE.md 圧縮の防御（scout の参照数え上げ + 独立再導出）**: 外部参照はすべて**節見出し単位**（CONTRIBUTING:16・AGENTS:53・safety-nets:39 等）＝見出し一字一句維持で全生存。**`## 利用できるスキル` は G8 パーサの split アンカー**（改名すると G8 が赤）。節内ラベル（手順 1〜4・(A2)・Layer 0/1・最重要ルール番号）は本文内から序数参照されるため維持。「設計文書 §2」参照 2 箇所は superpowers 歴史資料化に合わせ **#488 参照へ差し替える**
- **退去散文の行き先**: issue #589 へ 1 コメントに集約（#588 の退避パターン。圧縮後の各行は元 incident の #番号を保持し「行き先明示」を満たす）
- **PR**: scripts/** を触るため skip-ci 禁止（build-commands L140）。#589 は Closes してよい（試行期間なし）

## セルフレビュー（5b の 3 観点）

1. **境界条件**: G9 の境界（複数行配列・行末コメント・nodeSpec/vitestSpec の不混入・許容フラグ）はすべてテストフィクスチャ化。圧縮の境界（残す/退去の三値: 維持・圧縮・退去+行き先）は実装時に旧本文の全命題インベントリで検算
2. **シンプル化**: 共通定義・生成器を捨て照合 1 本に（ユーザー決定）。G9 は cargo 系のみ（意味判断の残余を無理に機械化しない）。圧縮はルート CLAUDE.md のみ（横展開は効果を見てから）
3. **破壊不変条件 + 検知手段**: (a) G9 抽出アンカー腐敗 → 抽出 0 件 fail（loud）。(b) 圧縮による規則消失 → 太字規則集合の前後 diff 検算 + governance:check G3/G8。(c) 見出し・ラベル破壊 → 参照は節見出し単位と実測済み・見出し不変を不変条件化。(d) health-check の記述矛盾 → 6 箇所の追従リストを実装チェックリスト化
