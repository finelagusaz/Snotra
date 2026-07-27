# independent-derivation

対象: `docs/superpowers/specs/2026-07-27-check-skill-skeleton-design.md`「移行の順序」の **1（ADR + rules ポインタ）と 2（骨格自体への 2 クラス読者）だけ**。3・4（`/race-check` `/symmetric-check` 本文の書き換え）は範囲外として扱う。

導出は設計書・リポジトリのコード・`scripts/governance-check.mjs` の**実行**に基づく。数値はすべて実測（コマンドを併記）。

---

## 必要な変更集合（独立導出）

### `docs/adr/0011-check-skill-skeleton.md`（新設）

- **新設**。番号 `0011` が次番 — 根拠: `ls docs/adr/` の最大が `0010-implement-step4-report-slot.md`。**索引ファイルは存在せず、更新すべき索引側は無い** — 根拠: `docs/superpowers/specs/2026-07-19-doc-minimization-design.md:54`「専用 index は作らない（`ls docs/adr/` で足りる）」＋ `ls docs/adr/ | grep -i readme\|index` が空
- **内容は却下 1〜4 のみ**（設計書 110-124 行）。採用案の再説明を書かない — 根拠: `.claude/skills/implement/SKILL.md:116`「ADR に書くのは**却下した代替案と、却下の理由**である——採用案の再説明を書かない」
- **見出し構造は `# ADR-00NN: <題>` / `## 文脈` / `## 決定` / `## 検討した代替案と却下理由` / `### 却下 N: <題>` / `## 帰結`** — 根拠: `docs/adr/0003-race-check-predicate-and-norm-hardening.md:1,3,9,15,17,21,27,31,43,47`（同ファイルが最も近い先例で、設計書 67・93 行が参照している）
- **却下 1 の「写しの分散先」列挙を、実測で置き換える**（設計書 112 行は 4 箇所を挙げるが不足） — 根拠: 実測で少なくとも 9 箇所ある。`AGENTS.md:54-60`（条件別チェック表 6 行）・`AGENTS.md:40`（`/symmetric-check`）・`.claude/rules/snotra-core.md:21,22`・`.claude/rules/snotra-core-search.md:20`・`.claude/rules/snotra-settings.md:20`・`.claude/rules/src-tauri.md:27`・`.claude/agents/code-reviewer.md:51`（`/symmetric-check`）・`docs/development-principles.md:63`（`/state-check`）・`.claude/skills/implement/SKILL.md:104`（6 件全列挙）・`.claude/skills/retrospective/SKILL.md:63`（6 件全列挙）。**設計書の 4 箇所には `.claude/agents/` と `docs/development-principles.md` と `/retrospective` が入っていない**
- **却下 1 の「影響半径」の主張を `docs/adr/0003-*:50` と整合させる** — 根拠: `docs/adr/0003-race-check-predicate-and-norm-hardening.md:50`「写しを **2 箇所**（`/race-check` の frontmatter / `AGENTS.md`「条件別チェック」表）に絞り、…ルート `CLAUDE.md`・`start-issue`・`.claude/rules/{src-tauri,snotra-settings}.md` は SSOT へのポインタへ置換した」。**既に 1 度絞られた後の状態**なので、「分散している」を無条件に書くと既存の ADR と矛盾する
- **ADR 内の他文書参照はフルパスで書く**（`docs/adr/0003-*` 形を使わない） — 根拠: `scripts/governance-check.mjs:189` `if (/[*?{<>%\\]/.test(t)) continue;` と `:191` `if (!REF_EXTENSIONS.test(t)) continue;`。`docs/adr/0003-*` は glob 文字と拡張子欠落の**二重**で G3 の検査を素通りする。設計書 93 行・133 行が現にこの形を使っている
- **序数で他文書を指さない** — 根拠: `.claude/rules/governance-docs.md:9`「**序数（「ステップ 3」「Step 8」）で他文書を指してはならない**——番号は構造を凍らせ、ずれても誰も気づかない」。設計書 93 行の「`docs/adr/0003-*` 却下 4」がまさにこの形

### `.claude/rules/safety-nets.md`

- **check 系スキルの新設・変更をトリガーとするポインタを追加**（設計書 132 行）。配送は既存 frontmatter で足りる — 根拠: `:8` `- ".claude/skills/**"`
- **既存 4 節のどれの下に置くか、または新規 `##` 節を立てるかを決める必要がある。この決定が下の `AREA_BUDGET` 変更の要否を決める**（実測は次項）。既存節は `:16`「効いていることは、フォールトインジェクションで一度は実測する」/ `:20`「フォールトインジェクションでは、稼働中のガードを弱めない」/ `:24`「セーフティネットが「規範」…の場合、フォールトインジェクションとは「回避しようとする読者」である」/ `:30`「検査の入力集合を、具体対象で検算する」/ `:34`「これまで無意味だった状態に意味を与える変更は…」。**`:30` の節が骨格スロット 1（母集団）と同じことを別の語（「検査の入力集合」）で既に言っている**（下の「見落とされやすい」①）
- **本 PR が新設する正準形の見出し参照はすべて（`safety-nets.md` からのものも、新設 ADR の中のものも）、em dash に U+2014 をそのまま使う** — `docs/adr/*.md` も `headingRefDocs`（`scripts/governance-check.mjs:817`）に入るため同じ検査が掛かる。根拠: `scripts/governance-check.mjs:727` `normAnchor = (s) => s.replace(/[\`*「」\s]/g, "")` は em dash を落とさない。実測: 設計書の見出しは `"## 骨格 — 4 スロット"` = `U+9aa8,U+683c,U+2014,U+30b9,…`。`「骨格 - 4 スロット」`（ASCII ハイフン）と書くと G11 が着地しない

### `scripts/governance-check.mjs`

- **`AREA_BUDGET.rules`（:604）の引き上げが要るかどうかは、ポインタの書き方で決まる**（下表 A/E）。設計書 135 行の「**`AREA_BUDGET` の引き上げは不要である**」は、その根拠となる字数見積もりが誤っている — 根拠: 実測。現在 `rules 7956/8056`（`node scripts/governance-check.mjs`）で余白 100 字。設計書は「約 55 字」と見積もるが、**参照先のパス `docs/superpowers/specs/2026-07-27-check-skill-skeleton-design.md` だけで 64 字**（`[...p].length` 実測）。実挿入をシミュレートした結果:

  | 案 | delta | 合計 | 判定（`:678` は `rules.total > AREA_BUDGET.rules`） |
  |---|---|---|---|
  | A: 箇条 1 行（正準形の見出し参照 `「骨格 — 4 スロット」` あり） | 101 | 8057 | **FAIL（1 字超過）** |
  | B: `##` 見出し + 箇条（ファイルの既存様式） | 130 | 8086 | **FAIL（30 字超過）** |
  | C: ADR パス（37 字）を指す | 61 | 8017 | ok |
  | D: 骨格の正本を `docs/check-skill-skeleton.md` へ移設 | 65 | 8021 | ok |
  | E: 箇条 1 行・**見出し参照を書かない**（ファイル名まで） | 88 | 8044 | ok（余白 12 字） |

  **超過の原因はポインタでも散文でもなく、`「骨格 — 4 スロット」` という 13 字の G11 検査可能な見出し参照である**（A と E の差が 13 字）。ゆえに選択肢は 3 つで、二択ではない:
  1. **E を採る** — 今日の予算に収まる。ただし**ポインタがどの検出器にも検査されなくなる**。`.claude/rules/governance-docs.md:9`「この形だけが `governance:check` の G11 で照合され」の外に出るうえ、`docs/superpowers/` は `scripts/governance-check.mjs:817` により G11 の**参照元**母集団からも外れているので、他のどこからも腐敗を捕まえられない
  2. **D（移設）を採る** — 正本を `docs/` へ移すとパスが 64 字 → 32 字になり、見出し参照を保ったまま収まる。⑥ の非規範ディレクトリ問題も同時に解ける
  3. **`AREA_BUDGET.rules` を上げる** — 見出し参照を保ち、正本の置き場も変えない
  **C は意味的に成立しない**（ADR は却下の記録であって骨格の正本ではない・設計書 3 行）。いずれにせよ設計書 135 行の「約 55 字なので引き上げ不要」という**見積もりの根拠は誤り**である（パス単独で 64 字）
- **上の 3 のみ、`:544-603` の JSDoc へ日付入りの理由段落を追記する** — 根拠: `:530`「基準の引き上げは AREA_BUDGET を理由コメント付きで更新すること（= 明示的な合意の摩擦）」。同 JSDoc には 2026-07-26〜27 の 6 件の先例が同じ様式で並ぶ。`:598`「**引き上げは失敗ではない。**」が向きを明示している
- **`AREA_BUDGET.alwaysLoaded` は触らない**（下の「変更不要」①）

### `docs/superpowers/specs/2026-07-27-check-skill-skeleton-design.md`

- **131 行「`docs/adr/` の次番（`check-skill-skeleton`）」は ADR 作成後に宙に浮く**。実番号へ確定するか、確定しない方針なら「次番」を残す理由を明記する — 根拠: `:131`。序数を確定しないまま残すのは `.claude/rules/governance-docs.md:9` の趣旨と衝突する
- **135 行「`AREA_BUDGET` の引き上げは不要である」は実測で偽になる**（上記）。同 PR で直さないと、規範の正本が誤った実測値を主張したまま残る — 根拠: `:135` と上表
- **3 行「却下した統廃合案は実装 PR で `docs/adr/` へ回収する」の帰結として、110-124 行を ADR へ移してポインタに畳むかを決める必要がある** — 根拠: `AGENTS.md:64`「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」。**この写しは本 PR が作る**ので規則は本 PR に発火する。`:3` が「回収」と書いている以上、既定は ADR 側が正本
- **（D 案を採るなら）122・124 行のバッククォート参照を書き換える必要がある** — 根拠: 実測で `checkReferences(snap, [設計書])` が 2 件返す: `:122` `_shared/check-skeleton.md`、`:124` `references/rationale.md`。どちらも**却下案の中の仮想パス**であり実在しない。現在は `governanceDocs()`（`scripts/governance-check.mjs:803` が `docs/superpowers/` を除外）から外れているため検出されないが、`docs/` へ移せば **G3 が即赤になる**

### `workspace/`（移行の順序 2 の成果物）

- **`/norm-review` の 2 クラス読者の出力を、巡ごと・読者ごとの別絶対パスへ書かせる** — 根拠: `.claude/skills/norm-review/SKILL.md:36`「渡すパスは**巡ごと・読者ごとに別**にし、**抜け道は 1 件 1 番号の箇条**で書かせる」。既定 2 巡（`:21`）＝ 4 ファイル
- **読者へ渡すのは「対象の文言だけ」＝骨格の定義文**（設計書 20-93 行 + 新設 ADR）。実装（`scripts/governance-check.mjs` や各 SKILL.md）を読ませない — 根拠: `.claude/skills/norm-review/SKILL.md:36`
- **Step 1 の停止条件 4 点（合格条件・上限巡数・受容する残余・塞ぎ 1 件 1 文の予算）を着手前に書き出す** — 根拠: 同 `:18-23`。**予算 1 文は `AREA_BUDGET.rules` の余白と直結する**（塞ぎが rules へ落ちるなら上表の余白を食う）
- **plan.md のチェックリストは全件 `- [x]` にしてから `workspace/` を削除する** — 根拠: `.claude/skills/implement/SKILL.md:117`。未チェックが残ると `gh pr create` が PreToolUse hook に拒否される（`CLAUDE.md`「フック」表）

### PR 本文（ファイルではないが変更集合の一部）

- **`#781` を閉じる語（`close`/`fix`/`resolve` 系 9 形）を書いてはならない** — 根拠: 設計書 155 行「**`#781` は本設計の PR では閉じない。**」＋ `CLAUDE.md`「Git/GitHub 運用」手順 1-4。PR テンプレートが `Closes` を自動で埋めるため、**書いた覚えが無くても残る**
- マージ直前に `gh pr view <PR> --json closingIssuesReferences` を確認し、マージ後に `gh issue view 781 --json state` が `OPEN` であることを確認する — 根拠: 同上手順 4

---

## 見落とされやすいと判断した箇所

### ① 同概念・別名 — `safety-nets.md` に「母集団」の節が既にある

- **箇所**: `.claude/rules/safety-nets.md:30`「検査の入力集合を、具体対象で検算する」
- **なぜ見落とされるか**: 骨格スロット 1 の語は「母集団」、既存節の語は「**検査の入力集合**」。**「母集団」で grep しても当の節の見出しには当たらない**（本文 `:32` にだけ現れる）。しかも既存節は「母集団は**派生した参照集合ではなく、起きた事実**から取り」と、スロット 1 の要石（「空の母集団は結果ではない」）と**同じ失敗様態**を別の切り口で塞いでいる。ポインタを無自覚に足すと、同一ファイル内に同概念が 2 節できる — 根拠: `.claude/rules/safety-nets.md:30-32` と設計書 24-37 行

### ② 同名・別概念 — 「アンカー」が repo 内で既に別の意味を持つ

- **箇所**: 設計書 39-45 行「アンカーの型（構造物 / 差分 / 列挙）」 ↔ `scripts/governance-check.mjs:707`「アンカーは ATX 見出し・番号付きリスト項目・太字リード の 3 種」・`docs/adr/0004-canonical-heading-references.md:15` 同旨・`:471,489` 「抽出アンカー」・`.claude/hooks/post-edit.mjs:55`「basename でアンカーする」
- **なぜ見落とされるか**: どちらも**ガバナンス機構の文脈**で、どちらも「3 種」と数える。しかも骨格を書き込む先の `.claude/rules/safety-nets.md` は G11（見出しアンカー）の主要な利用者である。ADR 内で「アンカー」と無限定に書くと、読者は G11 のアンカーだと読む — 根拠: 上記 grep（`grep -rn "アンカー"`）

### ③ 同名・別概念 — 「スロット」が repo の主要な実装語である

- **箇所**: 設計書の「4 スロット」 ↔ `src-tauri/CLAUDE.md:34,37`・`snotra-egui-runtime/CLAUDE.md:25`・`.claude/skills/race-check/SKILL.md:77,79,81,83,135`・`.claude/rules/snotra-settings.md:20`（いずれも「共有スロット」「単一スロット」＝並行性の共有状態）・`docs/adr/0010-implement-step4-report-slot.md`（「report slot」）
- **なぜ見落とされるか**: **`/race-check` の本文自身が「共有スロット」を 5 箇所で使っている**。骨格を `/race-check` へ適用する段（3・4）で「スロット 1 を埋める」と書くと、同じスキルの中で 2 意味の「スロット」が同居する。ADR-0010 が既に別の意味で「slot」を題名に使っている点も、ADR 一覧を眺めるだけの読者を誤らせる — 根拠: 上記 grep（`grep -rn "スロット"`）

### ④ 同名・別概念 — 「対称」が `/symmetric-check` の中心語である

- **箇所**: 設計書 78-93 行「費用対称性」 ↔ `AGENTS.md:54`「対称ペア（clicked/double-clicked・show/hide・enter/exit・生成/破棄・フラグ真偽）」・`.claude/agents/code-reviewer.md:47,51`「2b. 対称コードパスチェック」
- **なぜ見落とされるか**: 骨格を適用する 2 スキルのうち片方が `/symmetric-check` である。「対称性を満たす」がコードパスの対称なのか費用の対称なのか、**同じスキル本文の中で曖昧になる**

### ⑤ 骨格内部の「停止」が 2 意味を持つ

- **箇所**: 設計書 62-69 行（スロット 3 =「探索の打ち切り」） ↔ 74 行（スロット 4 =「スキル自身が**停止権**を持つのか」＝ワークフローを止める権限） ↔ `.claude/skills/norm-review/SKILL.md:16`「停止条件」（レビュー巡の打ち切り）
- **なぜ見落とされるか**: **移行の順序 2 は `/norm-review` を骨格に当てる**ので、「停止条件」という同じ語が「レビューの停止条件」と「骨格スロット 3」の両方で走る。2 クラス読者への指示文中で衝突する

### ⑥ 規範の正本が、リポジトリが「非規範」と宣言したディレクトリに置かれる

- **箇所**: 設計書 3 行「本書は**骨格の定義の正本**であり、`.claude/rules/safety-nets.md` から参照される」 ↔ `docs/superpowers/README.md:1,3`「歴史資料（非規範）… **現在の仕様ではない**（#589 で宣言）」・`docs/adr/0005-area-metric-characters.md:35`「歴史側の資料であり、**追随させない**（#589 で非規範化済み）」
- **なぜ見落とされるか**: 設計書を書いている当人からは「今書いたばかりの正本」に見え、ディレクトリの分類は視界に入らない。**`docs/adr/0005:35` は「その資料の記述には追随しない」という*裁定*である** — 常時配送される `.claude/rules/` が、追随しないと裁定されたディレクトリの文書を正本として名指すことになる
- **機構的な帰結（実測）**: `scripts/governance-check.mjs:803` が `docs/superpowers/` を G3/G4 の母集団から、`:817` が G11 の**参照元**母集団から外す。ゆえに骨格の正本の内部参照は腐っても検出されない。実測で既に 2 件壊れている（`:122` `_shared/check-skeleton.md`、`:124` `references/rationale.md`）
- **①ではなく⑥が判断を要求する形**: 「常時配送される規範が、非規範と裁定された文書を正本にしてよいか」。**この問いと `AREA_BUDGET.rules` 超過は同じ 1 手（正本を `docs/` へ移設）で同時に解ける** — 移設で 64 字 → 32 字になり余白に収まり（上表 D）、同時に G3/G4/G11 の対象へ入る。ただし移設すると上記 2 件が即赤になるので、同 PR で書き換えが要る

### ⑦ 却下 3 が依存している事実を、ADR に書く前に検算する必要がある

- **箇所**: 設計書 120 行「`AGENTS.md` の条件別チェック表は両者を別トリガーで振り分けており、並行性の変更で `/symmetric-check` が起動される保証は無い」
- **なぜ見落とされるか**: 表だけを見ると正しい（`AGENTS.md:54` と `:60` は別行）。しかし **`.claude/agents/code-reviewer.md:51` が「対称ペアの類型は `AGENTS.md`…が持つ。検査そのものは `/symmetric-check`」と書いており、`/implement`「4b」は code-reviewer を無条件に起動する**（`.claude/skills/implement/SKILL.md:107`）。さらに `:104` は「`/symmetric-check` はコードパス変更・バグ修正で**ほぼ常に該当**」と書く。「起動される保証は無い」は**実装より強い全称主張**になりうる — `AGENTS.md`「検証の作法」1 点目（全称表現は前提条件とセットで書く）に当たる

### ⑧ `/implement`「4a」の列挙は既知の未同期点であり、issue が立っている

- **箇所**: `.claude/skills/implement/SKILL.md:104`（6 件を列挙）・`:128`（「この母集団が `AGENTS.md`…表と一致するのは、4a の列挙が表と同期している間だけである」）
- **なぜ見落とされるか**: 今回（1・2）はスキルを増減しないので**変更不要**だが、却下 1 の根拠として「写しが `/implement`「4a」にある」と書く以上、**その写しの同期義務が既に issue 化されている事実**（`#778`「4a の check スキル列挙と AGENTS.md 表の同期義務が、表を編集する人の視界の外にある」）に触れないと、ADR が既存の未解決課題を再発見しただけに見える — 根拠: `gh issue list`

### ⑨ 移行の順序 2 の成果物には、恒久的な置き場が無い

- **箇所**: `.claude/skills/norm-review/SKILL.md:36`（絶対パスへ書かせる）↔ `.claude/skills/implement/SKILL.md:117`（`workspace/` は実装完了で削除）
- **なぜ見落とされるか**: 設計書 140 行は「今日の 102 件が『骨格が無かったとき』のベースラインになる」と、**前後比較を成功判定の唯一の根拠**（148 行）に据えている。ところが 2 巡分の指摘ファイルは `workspace/` に置かれ、同じ PR の `/implement` Step 5 で削除される。**次の PR（3・4）が「前」の数字を参照する先が git 履歴しか無くなる**。ベースライン 61/41 は設計書 7-9 行に残るが、**骨格自体に当てた結果（＝移行 2 の産物）の残し先はどこにも定義されていない**

### ⑩ ADR に `SPEC §N` 形の文字列を書くと G4 が発火する

- **箇所**: `scripts/governance-check.mjs:242` `/SPEC(?:\.md)?\`?(?: の)? ?§(\d+(?:\.\d+)?)/g`
- **なぜ見落とされるか**: ADR は `governanceDocs()`（`:802` の `f.startsWith("docs/")`）に入るので G4 の対象。骨格の説明で例示として `SPEC §8.6` 等を書くと（`/state-check` の説明で自然に出る語）、実在しない番号なら赤になる。`docs/superpowers/` に居る設計書では発火しなかったため、ADR へ写す段で初めて出る

---

## 変更不要と判断した箇所（根拠付き）

1. **`AGENTS.md`「条件別チェック（トリガー → 参照先）」に新しい行を足す必要は無い** — 根拠: `AGENTS.md:62` が既に「セーフティネット（hook・CI・`.githooks/`・`.claude/settings.json`・rules・**skills**・規範）を新設/変更 → `.claude/rules/safety-nets.md`」を持ち、`.claude/rules/safety-nets.md:8` の `paths: ".claude/skills/**"` が自動配送する。check 系スキルは skills の部分集合であり、トリガーは既存行に含まれる
2. **`AREA_BUDGET.alwaysLoaded`（13274/13374・余白 100 字）は触らない** — 根拠: 1 により `CLAUDE.md`・`AGENTS.md` を編集しない。`scripts/governance-check.mjs:541,631` より常時ロード面の母集団は「`CLAUDE.md` + `AGENTS.md` + `disable-model-invocation: true` でない skill の `description`」。本変更は skill を新設せず既存 `description` も変えない。ADR・spec は `:536-538` のコメントどおり明示的に対象外
3. **`CLAUDE.md`「利用できるスキル」表（G8）は変更不要** — 根拠: `scripts/governance-check.mjs:445` の G8 は `disable-model-invocation: true` の skill だけを索引する。本変更は skill を増やさない
4. **`.claude/settings.json` の matcher・`.claude/hooks/post-edit.mjs` の `selectChecks` は変更不要** — 根拠: `scripts/governance-check.mjs:6` のコメントどおり `.md`・rules・skills に検査は割り当てられておらず、今回の変更で新しいファイル種別は増えない（`docs/adr/*.md` は既存の `.md` 経路）
5. **`docs/build-commands.md` は変更不要** — 根拠: カテゴリ F（`npm run governance:check`）の発火条件は既に「`*.md`・`.claude/rules/`・`.claude/skills/`」を覆う（`.claude/skills/implement/SKILL.md:91` が見出しを SSOT として引用）。新しいコマンドは増えない
6. **新しい `G` 番号の検査は追加しない**（序数の新設なし） — 根拠: 設計書 157 行「**骨格の遵守を測る検出器は置かない。**… `governance:check` が測れるのは「見出しが存在するか」までで、中身の妥当性は機械化できない」。ゆえに `runAll`（`scripts/governance-check.mjs:821`）の呼び出し列も、evidence 文字列の `G1..G11`（`:856`）も変更不要。`scripts/governance-check.test.mjs` も同様（新しい純関数を足さないため）
7. **`/race-check` `/symmetric-check` `/cache-check` `/state-check` `/persistence-check` `/dry-check` の SKILL.md は変更不要** — 根拠: タスク定義により 3・4 は範囲外。ただし**却下 4**（設計書 122-124 行）が「骨格は執筆規約でありスキル本文には埋めた結果だけを残す」と決めているので、1・2 の段でスキル本文へ骨格の説明を書き足してはならない

---

## 未検証（理由）

- **harness が `.claude/rules/` を「ファイルの新規作成」で配送するか** — 骨格のトリガーは「check 系スキルを**書く**とき」だが、配送は harness ネイティブでリポジトリ内からは決定不能（`MEMORY.md` の `reference_claude_rules_paths_matcher.md` が同じ結論を記録）。既存スキルの編集では配送されることが `paths` から読めるが、**まだ存在しないファイルを書き始める瞬間**に配送されるかは測れなかった。骨格が最も要る場面がここである
- **`AREA_BUDGET.rules` の実挿入後の値** — 上表はシミュレーション（`node -e` で文字数を合算）であり、実ファイルを編集して `node scripts/governance-check.mjs` を再実行してはいない。書き込み対象が `workspace/plan-review/independent-derivation.md` の 1 ファイルに限られているため。**A 案が 1 字だけ超過する**という結果は境界そのものなので、実挿入で必ず再測すること
- **`/norm-review` を骨格に当てた結果、何件出て何件が rules へ落ちるか** — 移行の順序 2 の実行結果は本導出の前提に含められない。塞ぎが `.claude/rules/` へ落ちるなら余白（引き上げ後の値）を追加で食う。`/norm-review` Step 1-4 の「塞ぎ 1 件あたり 1 文」予算が効くかは実測待ち
- **`docs/superpowers/` 移設（D 案）の全影響** — G3 の 2 件（`:122`・`:124`）は実測したが、移設が `docs/superpowers/README.md` の「歴史資料」宣言の一貫性に与える影響（specs の一部だけが `docs/` に居る状態が許容されるか）は判断していない。また `docs/adr/0005-area-metric-characters.md:35` が名指しする別 spec との整合も未確認
- **却下 1〜4 の内容的な妥当性** — 本導出は「変更集合の列挙」であり、統廃合を却下した判断自体の是非は評価していない（`CLAUDE.md`「分析・調査・助言を求められたら、調査結果のみを報告する」）
- **並行セッションによる ADR 番号の競合** — `git branch -a` で `chore/check-skill-skeleton-design` 以外に ADR を追加しそうなブランチは見えなかったが、`MEMORY.md` が「並行セッションが 3 度観測されており `main` の pull で他人の変更が入る前提」と記録している。`0011` の確定は push 直前に `ls docs/adr/` で再確認すること
- **設計書 106 行「測定済みは `/race-check`（61 件）と `/symmetric-check`（41 件）の 2 つだけ」の元データ** — 61 件・41 件の一次記録（読者ごとのファイル）は `workspace/` にも `.superpowers/` にも見当たらず（`ls -R workspace/` は `plan.md` と `plan-review/rules-delivery.md` のみ）、設計書の記述以外に接地点を確認できなかった。これは⑨の問題が**既に一度起きている**可能性を示すが、断定できない
