# plan.md — issue #566: health-check Check 1 のテストファイル照合方針明確化

**方針（合意済み）**: Check 1 を **production モジュール整合チェック**と定義し、`*.test.{ts,tsx}` を照合母集団から除外する。glob だけでなく「目的」を明文化し、除外はファイル種別ベースの規則にする（現存ファイルのハードコードは禁止）。

## 変更ファイル一覧

1. **`.claude/skills/health-check/SKILL.md`**（唯一のコード的変更・Check 1 節 + 出力セクション）
   - Check 1 に**目的の宣言**を追加（production モジュール索引の同期を保証・テスト網羅性検査ではない）
   - 対象定義の ui 行を `*.test.{ts,tsx}` **除外**に更新
   - **除外の根拠**を明記（テスト = 検証手段であって責務単位でない・種別ベースの規則。Rust は独立テストファイルを持たないため実質 UI のみ——1 文に留める）
   - **テスト専用 helper/fixture の扱い**を別途規定（前向き・現状 0 件）
   - **手順に証拠要求**を追加（除外種別 + 照合母集団件数）
   - **出力セクション（行 131–153）に「根拠」の置き場を追加**——サマリーに Check 1 照合母集団の根拠行を設け、**根拠は発見事項ではない**（発見事項カウント・「All checks passed」ゲートに影響しない）旨を注記する（advisor 指摘: `Info` は発見事項カテゴリなので clean run でゲートを壊す。証跡は別カテゴリ）
2. **`.claude/skills/retrospective/SKILL.md`**（Step 2b 独立再導出が拾った漏れ・AC #7 の end-to-end 担保）
   - `/health-check` は `disable-model-invocation: true`（frontmatter 実測）。サイクル末に Check 1〜10 を実行するのは `/retrospective` Step 7（retrospective:118「本スキルの責任で実施する」）。その出力仕様 **retrospective:135**「health-check の結果サマリ（発見事項・`Skipped` とその処理）」は 2 カテゴリの閉じた列挙で、**Check 1 の照合根拠を持ち上げる義務が書かれていない** → cycle-end 経路で AC #7 が黙って落ちうる
   - 修正: retrospective:135 の列挙に「**Check 1 の照合根拠（照合母集団件数・除外種別）**」を追記し、証拠を最終報告へ持ち上げる義務を明示（1 行の同期）
3. `workspace/research.md`, `workspace/plan.md`（本ファイル）

**変更しない（意図的な非変更）**:
- `ui/CLAUDE.md`: 現行「モジュール構成」は既に production 限定で、選択肢 1 では正しい状態（Explore・Plan とも 28 件差分ゼロを実測）
- Check 1 の他対象行（Rust 3 本）: 独立テストファイルが 0 件（`#[cfg(test)]` インライン規約）ゆえ除外規則は不要
- **`.claude/rules/ui.md:3`**（`paths: ui/src/**/*.{ts,tsx}`・同一 glob だが別目的＝テスト編集時にも ui ルールを配送**すべき**）: false friend。ここに除外を適用してはならない
- `vitest.config.ts:8` / `.claude/hooks/post-edit.mjs`: `ui/src/**/*.test.{ts,tsx}` が「何がテストか」の SSOT。除外パターンはこれに**揃える先**であって変更対象ではない
- チェック総数 **10** に依存する参照（`CLAUDE.md:138`・health-check description・`docs/build-commands.md:132`・`AGENTS.md:91`）: Check 1 は**在庫改修**（追加・削除・再番号なし）ゆえ全て有効 → **チェックを増減させない**制約が導かれる

## 実装内容の詳細（SKILL.md Check 1 節の草案）

現行の対象定義 ui 行（行 23）:
```
- `ui/CLAUDE.md` ↔ `ui/src/**/*.{ts,tsx}`（エントリポイント・components・stores・lib セクション）
```
→ 変更後:
```
- `ui/CLAUDE.md` ↔ `ui/src/**/*.{ts,tsx}` から `*.test.ts` / `*.test.tsx` を除外（エントリポイント・components・stores・lib セクション）
```
除外パターン `*.test.{ts,tsx}` は「何がテストか」の SSOT（`vitest.config.ts` の `ui/src/**/*.test.{ts,tsx}`・`ui/CLAUDE.md`「テスト基盤」のテストファイルパターン）と一致させる旨を一言添える（独立恣意的な別パターンを作らない）。

Check 1 節に追加する要素（文面は実装時に簡潔化するが、以下の命題を必ず含める）:

- **目的**: 「Check 1 は各 `CLAUDE.md`『モジュール構成』が production ファイルの網羅的索引であり続けることを保証する（追加・削除と責務索引の同期）。**テストの網羅性検査ではない**」
- **除外の根拠**: 「`*.test.ts` / `*.test.tsx` は対応する production モジュールの**検証手段**であって責務単位ではない。テストの構成・方針は `ui/CLAUDE.md`『テスト基盤』節が方針として受け持つ（個別ファイルは列挙しない）。この除外は**ファイル種別（`*.test.*`）に基づく規則**であり、現存ファイルのハードコードではない——テストの増減に自動追従する。Rust 各 crate はインライン `#[cfg(test)]` 規約で独立テストファイルを持たないため、この除外が実質効くのは UI のみ」（← YAGNI で「将来 Rust に導入された場合も…」の投機条項は削除。UI 限定である旨の 1 文に留める）
- **テスト専用 helper/fixture**: 「production から import されないテスト専用モジュール（`__tests__/` 配下・`*.fixture.ts` 等）を module として管理する場合は、production の『モジュール構成』表ではなく『テスト基盤』節に責務を記す（現状そのようなファイルは存在しない）」
- **手順の更新**:
  - 手順 2 に「テストファイル（`*.test.{ts,tsx}`）を母集団から除外する」を追記
  - 手順 4（新規）「**証拠を添える**: 除外したファイル種別（`*.test.{ts,tsx}`）と実際に照合した母集団の件数（または一覧）を、最終報告の**サマリーに『根拠』として**残す。『発見事項なし』でも件数を添える」

### 出力セクションの変更（AC #7 の受け皿・advisor 指摘の反映）

`Info` は「コードベースに乖離がある」発見事項カテゴリ（行 150）。証拠を `Info` で出すと clean run でも発見事項がゼロにならず、「All checks passed」ゲート（行 152）が成立しなくなり、サマリーの発見事項カウントも水増しされる。ゆえに証拠は**発見事項でない別カテゴリ**として置く:

- サマリー（**常に出力される**・行 153）に**根拠行**を追加:
  ```
  ## サマリー
  - チェック項目数: N（実施: M / Skipped: K）
  - 発見事項: N件（Critical: N / Warning: N / Info: N）
  - 根拠 — Check 1 照合母集団: `ui/CLAUDE.md` production X 件（`*.test.{ts,tsx}` Y 件を除外）
  ```
- 出力セクション末尾の注記に **「根拠 行は実行の証跡であって発見事項ではない。発見事項カウントにも『All checks passed』判定にも算入しない（`[Skipped]` と同じく、発見事項と別カテゴリ）」** を追加する
- **判別式**（advisor）: 追加する行が発見事項カウントか「All checks passed」ゲートを動かすなら誤分類。根拠行はどちらも動かさない
- スコープ限定: 本 issue の対象は Check 1 のため、根拠行は Check 1 の照合母集団に限る（全検査への一般化は YAGNI・別 issue）。ただし Check 7 も「発見事項なしでも根拠を示す」規律（行 89）を既に持つため、注記は「根拠は発見事項でない」という一般原則として書き、将来他検査が根拠を出す余地を塞がない

## 実装順序（フェーズ）

ドキュメント（skill 定義）変更ゆえ最小構成:
- **Phase 1**: `health-check/SKILL.md` の Check 1 節を更新（目的宣言 + 除外規則 + helper 扱い + 手順の証拠要求）+ 出力セクションに「根拠」の置き場を追加
- **Phase 1b**: `retrospective/SKILL.md:135` の出力仕様に「Check 1 の照合根拠」を追記（AC #7 の end-to-end 担保）
- **Phase 2（検証）**: 現行 `ui/CLAUDE.md` に対し新定義で Check 1 を**実際に手動実行**し（机上の想定で済ませない）、以下を確認:
  - 正常系: production glob（test 除外）と ui/CLAUDE.md「モジュール構成」記載名の差分がゼロ（意図しないテスト警告が出ない）
  - **もし production 側に真の乖離が出たら、それは AC #4 未達ではなく実在の finding**——`ui/CLAUDE.md` を直して解消する。テスト除外で production の乖離を隠してはならない（advisor 指摘）
  - 負例 1: production ファイルを一時追加した想定 → 「実ファイルあり・記載なし」を検出できる
  - 負例 2: モジュール構成表から 1 行削除した想定 → 「記載あり・実ファイルなし」を検出できる

## 不変条件

- **目的の単一性**: Check 1 は「production 索引の網羅性」検査であり、テスト網羅性検査ではない。両者を同じ検査に混ぜない
- **種別ベースの規則**: 除外は `*.test.*` というファイル種別で定義し、現存ファイル名の列挙で定義しない → テスト増減（13→18 のような）に自動追従
- **検出能力の保存**: 除外はテストのみ。production モジュールの追加漏れ・削除残りは従前どおり検出できる
- **ui/CLAUDE.md 不変**: 選択肢 1 では ui/CLAUDE.md の内容は正しいため変更しない
- 失敗・異常系: ドキュメント変更のみ。新規の状態フラグ・プロセス・ウィンドウ・リソースを導入しない（異常終了・順序依存の懸念なし）

## テスト方針

- 機械テストは無い（skill はドキュメントで実行可能なテストを持たない）。検証は**手動 Check 1 実行**が唯一の担保:
  - コマンド: `Glob ui/src/**/*.{ts,tsx}` → `*.test.{ts,tsx}` を除外 → `ui/CLAUDE.md`「モジュール構成」記載名と双方向照合
  - 正常系・負例 2 件（上記 Phase 2）
- **PostToolUse hook は SKILL.md に検査を割り当てない**（`selectChecks` に無い）。編集後の沈黙は「未実行」であり合格ではない → 手動検証を必ず実施し、結果を報告に残す

## SPEC.md 更新要否

**不要**。SPEC.md は製品仕様。`/health-check` skill の定義は運用ツールで SPEC の対象外（挙動変更＝製品仕様の変更ではない）。

## 受け入れ条件の対応

| 受け入れ条件 | 対応箇所 |
|---|---|
| Check 1 の目的が明文化 | Phase 1「目的」 |
| `*.test.ts(x)` 包含・除外規則の明記 | Phase 1「除外の根拠」+ ui 行更新 |
| 実際の glob/除外条件が母集団と一致 | ui 行を `*.test.{ts,tsx}` 除外に更新・手順 2 |
| 現行構成でテスト警告が出ない | Phase 2 正常系 |
| production 追加の負例を検出 | Phase 2 負例 1 |
| production 削除の負例を検出 | Phase 2 負例 2 |
| 最終報告に照合件数/一覧を残す | Phase 1 手順 4 + 出力セクションの**サマリー根拠行**（`Info` ではなく発見事項外の「根拠」カテゴリ） |

## セルフレビュー

### Step 5a — plan-review 結果（Explore 監査 + Plan 独立再導出）

**要対処（反映済み）**:
- **retrospective:135 の漏れ**（Plan 独立再導出）: `/health-check` は `disable-model-invocation` で、cycle-end に checks を実行するのは `/retrospective`。その出力仕様（retrospective:135）が「発見事項・Skipped」の 2 カテゴリ閉列挙のため、Check 1 の照合根拠が黙って落ちうる → **Phase 1b で retrospective:135 に照合根拠を追記**。Explore は「ゆるい人間要約ゆえ腐らない」と読んだが、`disable-model-invocation` 実測 + retrospective:118「本スキルの責任で実施」より Plan の読みを採用（severity の最上位は実証）。

**軽微な懸念（実装時に留意・反映済み）**:
- 除外パターンをテスト SSOT（`vitest.config.ts` / `ui/CLAUDE.md`「テスト基盤」）に揃える → 実装内容に明記済み
- `.claude/rules/ui.md:3`（同一 glob・別目的）は false friend ゆえ非変更 → 「変更しない」に明記済み
- ui 対象行が他 3 行（Rust）と表記不揃い（「〜から除外」節が付く）→ 体裁のみ・機能問題なし（UI のみ除外が要るため不可避）

**問題なし（完全性の能動的証拠）**:
- ui/CLAUDE.md ↔ production 28 件: Explore・Plan 双方が独立に**差分ゼロ**を実測（AC #4 は現行構成で clean）
- Rust 3 crate に独立テストファイル 0 件（除外は UI 固有で足りる）
- Check 7 の証跡規律（行 89）と「根拠は発見事項でない」注記は矛盾せず補完
- SKILL.md 編集は PostToolUse hook 非対象（沈黙=未実行）→ 手動検証が必須（計画に明記済み）
- チェック総数 10 に依存する参照は在庫改修ゆえ全て有効（チェックを増減しない制約）

**独立導出との差分（Step 2b）**:
- **漏れ（導出 ∖ plan）**: retrospective:135（上記・反映済み）
- **スコープ過剰（plan ∖ 導出）**: なし（両者とも「health-check/SKILL.md + retrospective 出力仕様」に収束）
- **一致（完全性の証拠）**: 選択肢 1・ui/CLAUDE.md 非変更・Rust 対称性不要・証拠は発見事項でない第3カテゴリ・現存ファイルのハードコード禁止 → 独立に再一致

### Step 5b — plan-review が扱わない 3 観点

1. **境界条件**: (a) `vite-env.d.ts` は `.d.ts` だが glob `*.{ts,tsx}` にマッチし、かつ ui/CLAUDE.md に記載あり（照合対象・実測一致）。(b) production 名だが `*.test.*` 命名でないテスト専用ファイルが将来足された場合は「実ファイルあり・記載なし」で正しく検出される（helper/fixture 条項が受け皿）。(c) 空 UI（production 0 件）は現実に起きないが、その場合も差分ゼロで正常。
2. **シンプル化の挑戦**: 新規状態・汎用インターフェースの導入なし（ドキュメント変更のみ）。「根拠」カテゴリ追加と retrospective 同期は AC #7 を end-to-end で満たす**最小**手段——health-check 側だけに留めると cycle-end 経路で AC が半分しか満たされない。`Info` で代用する単純案は「All checks passed」ゲートを壊すため不可（advisor 指摘）。これ以上単純な代替は AC を割る。
3. **破壊不変条件 + 検知手段**:
   - 不変条件①「Check 1 の production ドリフト検出力が保たれる」（除外はテストのみ）→ **検知**: Phase 2 負例 1（production 追加→検出）・負例 2（削除→検出）
   - 不変条件②「証拠追加が『All checks passed』ゲート（発見事項ゼロ ∧ Skipped ゼロ）を変えない」→ **検知**: 出力セクション編集後、ゲート式が不変であること・根拠行が発見事項カウントに算入されないことを目視確認（retrospective:120 の参照が腐らない条件）
   - いずれも Win32/IPC/プロセスの「戻ってこない」系リスクは無し（ドキュメント変更）
