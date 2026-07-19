# plan: issue #587 governance:check の導入

ブランチ: `chore/587-governance-check`。設計書 §2（PR #590 合意済み）の器: `scripts/` + CI 独立 job + `npm run governance:check`。hook は触らない。

## 検査一覧（スクリプトが実装する決定的検査）

| ID | 検査 | 元の health-check |
|---|---|---|
| G1 | 各サブディレクトリ `CLAUDE.md`「モジュール構成」↔ 実ファイルの双方向照合。**basename 包含方式**（順方向: 節内のバッククォート付きソースファイル名の basename が tracked ファイルに実在 / 逆方向: production ファイルの basename が CLAUDE.md 本文に出現）——scout 実測の罠（`commands/` 集約行のベア名列挙・`tabs/` プレフィックス省略・1 行複数バッククォート・`../main.html`）をディレクトリ解決なしで回避する意図的な弱化。ui は `*.test.{ts,tsx}` を母集団から除外（vitest.config.ts include と一致） | Check 1 |
| G2 | `docs/architecture.md` に先頭セルがバッククォート付き `*.rs/*.ts/*.tsx` の表行が再導入されていないか | Check 2 |
| G3 | ガバナンス文書群の参照実在（2 系統）。(i) Markdown リンク `[..](相対パス)`（`#anchor` 除去・`://` 除外）。(ii) バッククォート内パス様参照——**検査するのは次をすべて満たすトークンのみ**: `/` を含む・glob 文字（`*` `?` `{`）なし・`<>` なし・`%` なし・`://` なし・拡張子が既知ソース系 {md,rs,ts,tsx,mjs,json,toml,yml,ps1,html,css}・`workspace/` 配下でない。実在は「リポジトリルート基準 or 当該文書のディレクトリ基準」のどちらかで成立すれば可。**ベア名（`SPEC.md` 等）・ランタイム生成物（`config.toml`・`*.bin`・`*.bak`）・`%APPDATA%` は述語が構造的に除外**（受容する偽陰性。境界はフィクスチャで固定）。対象 = ルート `CLAUDE.md` / `AGENTS.md` / `CONTRIBUTING.md` / `SPEC.md` / `docs/*.md`（`docs/superpowers/` 除外）/ 各モジュール `CLAUDE.md` / `.claude/rules/*.md` / `.claude/skills/*/SKILL.md` | Check 3, 6 の一般化 |
| G4 | `SPEC.md` の `## N.` / `### N.x` 連続性（**コードフェンス内は除外**——SPEC.md:737 の TOML コメント `#` が誤検出源・scout 実測）+ **`SPEC` / `SPEC.md` 前置の `§N(.x)` 参照のみ**実在照合（裸の `§N` は各文書自身の節参照ゆえ対象外） | Check 4 + issue (f) |
| G5 | `docs/build-commands.md` の `npm run X` → package.json scripts、`cargo test -p <crate>` → **workspace members の各 Cargo.toml の `[package] name`**（`-p snotra` = `src-tauri/`。ディレクトリ名と不一致・scout 実測） | Check 5 の決定的部分 |
| G6 | `docs/build-commands.md`「CI/CD メモ」対応表 ↔ `.github/workflows/*.yml`: 表のコマンド（または wrapper が呼ぶスクリプトパス）が表記 workflow の `run:` に出現するか・workflow ファイルが実在するか | Check 10 の機械部分 |
| G7 | `.claude/rules/*.md` の `paths:` glob が tracked ファイルに 1 件以上マッチ（bare 名 = ルート直下のみ・`**` = 階層横断・`{a,b}` ブレースの documented 意味論で自前変換。harness の完全再現ではなく「マッチ 0 件検知」に限定） | Check 8 |
| G8 | ルート `CLAUDE.md`「利用できるスキル」表 ↔ `.claude/skills/*/SKILL.md` の双方向照合 | Check 9 |

共通契約: 依存ゼロ（Node fs/path のみ）・検査関数はスナップショット注入（root/readFile/listFiles）で純関数化・findings は `file:line` 付きで全列挙し exit 1・緑は照合母集団の件数（根拠）を印字し exit 0・**空母集団（対象 md 0 件・rules 0 件・skills 0 件）は明示 fail**（沈黙経路の閉塞）・免除注記の機構は設けない・母集団の列挙は `git ls-files` でなく fs 走査（pathspec `**` の取りこぼし・Check 1 注記）。

**スクリプトに含めない**（意味判断のためスキルに残す）: Check 5 の hook `buildCommand` 照合（フラグの意味論）とコマンド直書き grep、Check 7（メモリ・リポジトリ外）、Check 10 のトリガー散文照合、責務記述の妥当性。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `scripts/governance-check.mjs`（新規） | G1〜G8。上記共通契約 |
| `scripts/governance-check.test.mjs`（新規） | vitest（include 済み・`npm test` で両 OS CI に自動編入）。検査ごとに (i) 故障注入フィクスチャで赤（守りたい対象 1 件が入力に現れる）、(ii) 正常フィクスチャで緑、(iii) 判定対象外（glob・URL・`%APPDATA%`・`config.toml` 等ランタイム生成物・superpowers/workspace 配下）の不混入検算、(iv) 空母集団 fail、(v) G7 glob 変換の代表入力（bare 名 / `**` / `*.ext` / `{ts,tsx}`）固定 |
| `package.json` | `"governance:check": "node scripts/governance-check.mjs"`（`clean:worktrees` と同形。編集で hook-selftest 自動発火・沈黙=合格） |
| `.github/workflows/ci.yml` | 独立 job `governance-check`（ubuntu-latest・checkout + setup-node のみ・`npm ci` 不要）。**`skip-ci` if ガードを意図的に付けない**——skip-safe と定義された集合（docs/rules/skills/*.md）がまさに本検査の対象であり、ガードは規範読者の抜け道になる（独立再導出の指摘を採用）。paths フィルタも付けない（軽量ゆえ常時実行・「走らなかった」経路を作らない） |
| `docs/build-commands.md` | (a) カテゴリ F「ガバナンス文書（`*.md`・`.claude/rules/`・`.claude/skills/`・workflow）変更時: `npm run governance:check` 必須」追加。(b)「CI/CD メモ」表に行追加（`ci.yml`（governance-check）/ PR 自動・**skip-ci 非対象**と注記）。(c) 129-130 行の skip-ci 段落を改稿——「CI がテスト対象に持たない」という前提が governance-check 導入で偽になる（skip-ci を貼っても governance は走る、へ）。(d) 26 行「乖離は `/health-check` Check 5 が検知」→ 実在照合は governance:check・フラグ照合は health-check に残置の旨へ。(e) 132 行「Check 10 で検出」→ governance:check へ |
| `.claude/skills/health-check/SKILL.md` | 機械化された Check 1・2・3・4・6・8・9・10 の本文を「→ `npm run governance:check`（G n・#587）が機械検査。ここでは実行しない」stub へ置換。**Check 番号は振り直さない**（Check 5/7 への序数参照を腐らせない・governance-docs.md ルール）。frontmatter `description` の「10項目」・出力節の「Check 1〜6・8〜10 は前提が常に満たされる」列挙・サマリー根拠行（Check 1 母集団 → governance:check 出力参照）も改稿。冒頭に「決定的検査の SSOT は governance:check」1 行追加 |
| ルート `CLAUDE.md` | (a) フック節 82 行の #497 残余記述 →「決定的項目は PR CI の governance-check job が捕捉（skip-ci 非対象）。編集時の即時性は無く、governance:check の対象外の記述は依然残余」へ（全称は前提条件とセットで書き直す）。(b) スキル表 138 行 `/health-check` の「10項目で検証」→ 実態（意味判断のみ・決定的照合は governance:check）へ |
| `AGENTS.md` | 条件別チェック表に引き金行 1 行「ガバナンス文書・スキル表・モジュール索引・rules を変更 → `npm run governance:check`（`docs/build-commands.md` カテゴリ F）」を追加（引き金の正準は同表——載せないと編集時の発火経路が無い） |
| `.claude/rules/safety-nets.md` | `paths` に `"scripts/governance-check.mjs"` を追加（安全網の実体が scripts/ に増える。`scripts/**` 全体は過剰） |
| `.claude/rules/governance-docs.md` | 「完全性は機構では担保されない」段落へ追記: 参照実在・スキル表・SPEC § 参照は governance:check が PR CI で事後検知する。ただし Step 番号・引用見出し語句の概念参照は依然検知不能（残余を明記） |
| `.claude/skills/retrospective/SKILL.md` | 118 行「Check 1〜10 を本スキルの責任で実施する」→「`npm run governance:check` を実行し赤を発見事項として扱う + health-check に残る検査（Check 5 の意味判断部分・Check 7）を実施する」へ**名前ベース**で改稿。135 行の根拠例（Check 1 母集団）→ governance:check 出力へ。Check 7 への序数参照（106/109/120 行）は番号維持により無傷 |

変更しないもの（明示）: PostToolUse hook（`post-edit.mjs` / `selectChecks`——issue の明示制約）、`docs/superpowers/` 配下の旧 Check 序数参照（歴史資料）、`docs/development-principles.md`（health-check への直接参照なし・grep 実測）。

## 実装順序

1. **Phase 1**: `governance-check.mjs` 骨格 + G1〜G8 を 1 検査ずつ Red（故障注入フィクスチャ）→ Green。`npm test` 緑
2. **Phase 2**: 実リポジトリへ実行し dogfood。実ドリフトが出れば列挙してから修正（#586 済みだが G3/G4 は初走査）。ローカル緑を確認
3. **Phase 3**: 配線・文書更新（package.json / ci.yml / build-commands / SKILL.md ×2 / CLAUDE.md / AGENTS.md / rules ×2）
4. **Phase 4（故障注入の実測・safety-nets.md 手順）**: (a) ローカル: 実ファイルに一時ドリフトを入れ exit 1 + `file:line` 出力を実測→戻す。(b) CI: PR ブランチへ故意の破れを一時コミット→ governance-check job の赤を実測→ revert。(c) **skip-ci ラベルを貼った状態で governance-check job が走ることを 1 度実測**（抜け道封じの実証）→ ラベル除去。結果は PR 本文へ記録
5. 各 Phase 完了時にコミット（中断耐性）

## 不変条件

- **exit code 契約**: findings ゼロ = exit 0 + 母集団件数印字。findings あり = exit 1 + 全件列挙。Warning 段階なし（CI ゲートは二値）
- **沈黙経路の全列挙**（「job 緑 = 合格」の意味づけに対する #471 型義務）: スクリプト crash → Node 非零で赤 / 空母集団 → 明示 fail / job 不起動 → if・paths を持たず PR で常時起動（workflow 構文破壊は Actions が startup_failure で可視）/ script 名 typo → G5 が build-commands 行経由で自己照合（自己言及カナリア）
- **決定的**: ネットワーク・時刻・環境変数に依存しない
- **自己整合**: governance:check 自身の配線（package.json・ci.yml・build-commands 行）が G5/G6 の検査対象
- **health-check の Check 番号は不変**（序数参照の腐敗防止。改稿は名前ベース）
- **既存 CI job に触れない**
- G3 の受容する偽陰性（ベア名・ランタイム生成物）はフィクスチャとスクリプトコメントの両方に明文化する

## テスト方針

- `scripts/governance-check.test.mjs`: 上記 (i)〜(v)。判定ロジック（glob 変換・参照抽出述語・見出しパーサ）は実装前に代表入力で測る（AGENTS.md 検証の作法。サブエージェントの報告した罠——SPEC.md:737 フェンス内 `#`・`commands/` 集約行・`{param}`——を代表入力に含める）
- 実測: Phase 2 ローカル緑、Phase 4 ローカル赤 + CI 赤 + skip-ci 貫通の 3 点
- 検証カテゴリ: `npm test` / `npm run governance:check` / PR CI 実走。package.json 編集の hook-selftest は自動発火（沈黙=合格）

## SPEC.md 更新要否

不要（開発ガバナンスのみ・ユーザー観測可能な挙動に変更なし）。

## スコープ外

- PostToolUse hook への検査追加（#497 受容の維持）
- Check 5 意味判断部分・Check 7・Check 10 トリガー散文の機械化
- rules ルーター化（#588）・検証写像の共通定義（#589。G5/G6 は照合であり生成は #589）
- 設計書 §4 の「写しを増やす変更」行の AGENTS.md 追加（#589 の事故散文圧縮と同時に行う。今回追加する governance:check 引き金行とは別物）

## セルフレビュー

### plan-review 結果（Step 5a）

- **要対処 1 件（反映済み）**: G3 の除外述語境界が未確定（scout-targets）。`/` 必須 + glob/`<>`/`%`/URL 除外 + 拡張子 allowlist + workspace 除外で確定し、境界をフィクスチャ固定・受容する偽陰性を明文化
- **設計変更 1 件（独立再導出の指摘を採用）**: governance job に skip-ci ガードを**付けない**。skip-safe 集合 = 本検査の対象そのものであり、ガードは抜け道。build-commands の skip-ci 段落改稿と Phase 4(c) の貫通実測を追加
- **漏れ（導出 ∖ plan・反映済み）**: retrospective SKILL.md:118（「Check 1〜10 を実施」——scout-wiring は見落とし、独立再導出が検出、主エージェントが grep で実証）、build-commands 26/129-130/132 行、governance-docs.md 追記、safety-nets.md paths、AGENTS.md 引き金行、health-check frontmatter description（scout-wiring も検出）、空母集団 fail・沈黙経路列挙
- **スコープ過剰（plan ∖ 導出）**: なし
- **一致（完全性の証拠）**: 検査項目の分割（8〜9 検査・機械化不能 4 項目の分類）・依存ゼロ・スナップショット注入・故障注入 2 段（fixture + CI 実測）・Check 番号の扱い（独立再導出は改番を提案したが、governance-docs.md の序数腐敗ルールにより番号維持 + 名前ベース改稿を採る——Check 7 参照 4 箇所が無傷になる方を優先）

### 5b の 3 観点

1. **境界条件**: パーサ境界（コードフェンス内 `#`・1 行複数バッククォート・集約行・ブレース glob・ベア名 paths）はいずれも scout の実測行を代表入力としてテストに固定。空母集団・リポジトリ外参照（`~/`）も検証ケースあり
2. **シンプル化**: G1 を「ディレクトリ解決付き完全照合」でなく basename 包含に弱化（誤検出ゼロを優先し、wrong-directory 検出は放棄——その検出が要る事故が起きてから強化する。YAGNI）。免除注記・Warning 段階・設定ファイルを持たない
3. **破壊不変条件 + 検知手段**: (a) 「governance job 緑 = ガバナンス文書整合」の意味づけ → 沈黙経路 4 本の列挙と各検知手段（上記不変条件）。(b) 偽陽性が多発すると skip-ci 圧力・無視の常態化を招く → Phase 2 の dogfood で実リポジトリ緑を確認してから配線。(c) ci.yml 破損 → Actions の startup_failure + PR の必須確認で loud
