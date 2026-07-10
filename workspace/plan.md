# plan — issue #488: (A2) の残りを hook で守らず、Layer 0 で断つ

## 決定（ユーザー合意済み・2026-07-10）

| 論点 | 決定 |
|---|---|
| `gh pr merge` / `gh issue close` を hook で守るか | **守らない。** 受け入れ条件 3 を採り、根拠を `CLAUDE.md` に記録して #488 を閉じる |
| Channel 2（コミット本文 → squash 本文）を残すか | **Layer 0 で消す。** `squash_merge_commit_message` を `COMMIT_MESSAGES` → **`PR_BODY`** |

根拠は `workspace/research.md`（結論 A〜E）。要点のみ:

- **結論 A**: `--subject`/`--body-file` は auto-close を制御していない。実際に issue を閉じているのは **PR 本文のリンク**（`closingIssuesReferences`）である（PR #491 を `--body-file` 付きでマージ → squash commit に `Closes` 無し → #489 が 1 秒後に close、`commit_id: null`）。**`CLAUDE.md` の現行手順は事実誤り**
- **結論 C**: 文書が制御している Channel 2 による誤 close は歴史上 0 件（30 PR で `Channel 2 ∖ Channel 1 = ∅`）。引用された #480 の「実害」も再現しない（コミット本文は `Refs` のみ）
- **結論 D**: hook は「何が閉じるか」を計算できるが「それが誤りか」は判定できない（意図の真実源が無い）→ `deny` 不可、`ask` 止まり。`ask` は stdout JSON + **exit 0** を要求し `pre-bash.mjs` の fail-closed 骨格を壊す。さらに hook は Web UI / ユーザー端末からのマージを見ない
- **結論 E**: `gh issue close` は既存の権限プロンプトが提示する情報を超えられない

**KISS/YAGNI**: コード 0 行。API 1 コール（可逆）と文書 3 箇所の訂正のみ。

---

## 変更ファイル一覧

| 対象 | 変更 | 理由 |
|---|---|---|
| GitHub repo 設定（ファイルではない） | `squash_merge_commit_message`: `COMMIT_MESSAGES` → `PR_BODY` | Channel 2 を全経路（Web UI 含む）から断つ |
| `CLAUDE.md` L42–44「Git/GitHub 運用」 | squash auto-close 手順を実測に合わせて書き換え | **結論 A**: 現行手順は制御していない経路を制御していると主張している |
| `CLAUDE.md`「フック」節 | 「(A2) のうち hook が守るのは `gh pr create` だけ」の判断根拠を追記 | 受け入れ条件 3 |
| `docs/superpowers/specs/2026-07-09-hook-responsibility-layers-design.md` §2 | 「`gh pr merge --squash` の誤 close も `gh issue close` も同じ」に実測による訂正を併記 | **結論 D**: (A2) の内部は一様でない。この文書は改訂欄を持つ as-built 追従型 |

**触らない**（根拠付き）:

- `.claude/hooks/pre-bash.mjs` / `pre-bash.test.mjs` — hook を作らない決定。`decide` の分岐表は不変
- `.claude/settings.json` — `matcher` は既に `Bash|PowerShell`
- `SPEC.md` — 製品挙動ではない（エージェント運用）。§ 番号のずれも生じない
- `snotra-core` / `src-tauri` / `ui` / `snotra-settings` — 無関係
- `.github/workflows/**` — squash 本文に依存する workflow は無い（grep 済み）
- `docs/superpowers/plans/2026-07-09-hook-responsibility-layers.md` — 実行済み計画の歴史記録。as-built 追従の対象外

**他所からの参照**（序数腐敗の確認・`AGENTS.md`「順序に依存する参照」）:

- `grep -rniE '(手順|ステップ)\s*[12１２]'` は追跡ファイルで **5 件ヒットする**（`start-issue/SKILL.md` の「手動確認手順」、`deps-update` 計画の「ステップ1〜6」、`snotra-settings/CLAUDE.md` の「固定ステップ」等）。**いずれも CLAUDE.md L42–44 を参照していない。** 当該節を**番号で**参照する箇所は 0 件（序数腐敗なし）
- 当該節を**名前で**参照する箇所が 1 件ある: `docs/superpowers/plans/2026-07-09-hook-responsibility-layers.md:1465`「`--subject` に `(#PR)` を付け、本文で `Closes` を制御する（CLAUDE.md「Git/GitHub 運用」）」。**触らない** — `plans/` は実行済み計画の凍結スナップショットであり as-built 追従の対象外（`specs/` と扱いが異なる）
- **この 1 件は、わたくしの grep（`body-file` / `COMMIT_MESSAGES` / `squash.*本文`）が落としていた。** 独立再導出が `--subject` と「本文で `Closes` を制御」という別の語で拾った。検索語は「既に思いついている概念の像」でしかない（`RETROSPECTIVE.md`「検出器を書くたび、また同じ罠を踏んだ」の再演）
- 本 issue の変更でルール番号・章番号は増減しない

**`.github/pull_request_template.md`（触らない・ただし文書化する）**:

```
## 対応Issue
- Closes #<issue_number>
```

**このリポジトリは Channel 1 を既定で供給している。** 全 PR の本文は closing keyword を持って始まる。テンプレートは正しく、Layer 0 変更後は「閉じる集合の唯一の源」として機能する。Phase 2 の手順はこの事実を前提に書く（`gh pr view --json closingIssuesReferences` が読むのは、まさにこの行）。

---

## 実装順序（フェーズ）

### Phase 1 — Layer 0: `squash_merge_commit_message` を `PR_BODY` へ

```
gh api -X PATCH repos/:owner/:repo -f squash_merge_commit_message=PR_BODY
```

- **未知**: GitHub は `squash_merge_commit_title` との組み合わせを検証する。現在は `COMMIT_OR_PR_TITLE`。422 が返ったら `-f squash_merge_commit_title=PR_TITLE` を併せて送る。**推測で先回りせず、まず単独で送って応答を測る**（`AGENTS.md`「判定の中核は自分で測る」）
- **read-back で確認する**: `gh api repos/:owner/:repo --jq '{squash_merge_commit_title, squash_merge_commit_message}'`
- **エスケープハッチ（可逆）**: `gh api -X PATCH repos/:owner/:repo -f squash_merge_commit_title=COMMIT_OR_PR_TITLE -f squash_merge_commit_message=COMMIT_MESSAGES`（現在値と完全一致することを read-back で確認済み）
- 権限は確認済み（`.permissions.admin = true`）
- **422 が返って `title` も変える羽目になったら、Phase 2〜4 の文面に `squash_merge_commit_title` の変更も書く。** 既定タイトルが「単一コミットならそのタイトル、複数なら PR タイトル」から「常に PR タイトル」へ変わる。CI・リリースは squash タイトルに依存しない（grep 済み）ため実害は無いが、**設定と文書がずれることを許さない**（不変条件 I2）

**Phase 1 を先に行う理由**: Phase 2 の文書が「本リポジトリは `PR_BODY`」と as-built を書く。設定変更前に文書を書くと、`AGENTS.md`「SPEC・docs は as-built を記述する」に反する。

### Phase 2 — `CLAUDE.md`「Git/GitHub 運用」の手順を訂正

L42–44 を差し替える。新しい文面が伝えるべき事実:

1. **auto-close の経路は 2 本あり、制御点が違う**
   - PR 本文の closing keyword → GitHub が「リンク」として計算。`gh pr view <N> --json closingIssuesReferences` と PR ページに現れる。**マージした瞬間に閉じる。`--subject`/`--body-file` は無関係**（実測: PR #491 / issue #489）
   - squash commit 本文の closing keyword → main に載った時点で閉じる。既定本文は `squash_merge_commit_message` が決め、**本リポジトリは `PR_BODY`**（#488 で `COMMIT_MESSAGES` から変更。ブランチのコミット本文が流入する経路を断った）
2. **`.github/pull_request_template.md` が `- Closes #<issue_number>` を既定で入れる。** 閉じる集合はここから生まれる
3. **手順**
   1. マージ前に `gh pr view <N> --json closingIssuesReferences` で閉じる集合を確認する
   2. 閉じたくない issue があれば **PR 本文を編集する**（`gh pr edit <N> --body-file <tmp>`）。マージ時の `--body-file` では止まらない
   3. `--subject` / `--body-file` は squash commit のメッセージを整えるために使う（そこに `Closes` を書けば、それも閉じる）
   4. マージ後に `gh issue view <N> --json state` で意図どおりか検証する
4. **副作用を明記する**: `--body-file` を省くと squash 本文が **PR 説明文まるごと**（表・チェックリスト込み）になる。整った履歴が欲しいときは従来どおり `--body-file` を渡す

**手順 4 を残す理由**: Layer 0 は「コミット本文由来の close」を断つが、PR 本文由来の close を消すわけではない（消せない・消すべきでもない）。事後検証は最後の観測点として残す。

### Phase 3 — `CLAUDE.md`「フック」節に (A2) の非対称性を記録

`gh pr create` を守り、`gh pr merge` / `gh issue close` を守らない理由を 3 点で書く（結論 D・E）。加えて、**この非対称は意図的であり、将来「対称にせよ」という指摘が来たときの答えである**ことを明示する。

「安全網の不在を検知する安全網」は作らない（設計文書 §2 の非目標に既出）。`squash_merge_commit_message` の read-back を CI で監視する検知器は置かない — ruleset 設定と同格に扱う。

### Phase 4 — 設計文書 §2 に実測による訂正を併記

L82「**(A2) は git にも CI にも原理的に観測できず、Claude Code の hook だけが見える領域である。** `gh pr merge --squash` の誤 close も `gh issue close` も同じ。」の直後に訂正ブロックを足す。**元の文は消さない**（合意時点の記録であり、訂正の対象が読めなくなる）。

併せて header の「関連:」に `#488` を足す（この文書を訂正した issue へ辿れるように）。

---

## 不変条件

| # | 不変条件 | 壊れたときの症状 | 検知手段 |
|---|---|---|---|
| I1 | `pre-bash.mjs` の fail-closed 骨格（既定 `exitCode = 2`）は変わらない | `gh pr create` の空 PR ガードが fail-open | `pre-bash.test.mjs` の骨格カナリア（既存・**本 issue では 1 行も触らない**ことが最良の担保） |
| I2 | 文書の主張と実測が一致する（as-built） | 誤った信念の上に次の PR が積まれる（#471 の失敗様式） | Phase 1 の read-back。V2 の実地マージ |
| I3 | Layer 0 の変更は可逆で、エスケープハッチが文書に残る | 設定を戻せず、squash 本文の既定が固定される | Phase 1 に rollback コマンドを記載。実行はしない |
| I4 | `--subject` / `--body-file` による明示は従来どおり効く | マージ時にメッセージを整えられなくなる | V2 で既定を測る。明示経路は #493 までの全マージで実績あり |
| I5 | Channel 1（PR 本文リンク）は消えない・消さない | 「マージで issue が閉じる」という前提が崩れ、手順 4 が無意味になる | Phase 2 の手順 1・4 が前提として明記する |

**新たな状態フラグ・プロセス・リソース・子プロセスを導入しない。** ゆえに「失敗・異常終了・予期しない順序」で壊れる対象は無い。Phase 1 の API コールが失敗した場合、設定は変わらず現状維持（COMMIT_MESSAGES）— **fail-safe な方向**であり、その場合は Phase 2〜4 の文面を「`COMMIT_MESSAGES` のまま・2 段チェック」へ倒して整合を保つ（設定と文書がずれることを許さない）。

---

## テスト方針 / 検証

コード変更が無いため、`docs/build-commands.md` の検証カテゴリ A〜E はいずれも該当しない（`*.rs` / `ui/src/**` / ウィンドウ・ホットキー / UI スタイル / `.githooks/**` のどれにも触れない）。PostToolUse hook も `.md` では発火しない。

**ゆえに検証は「実測」と「規範への故障注入」で行う。**

| # | 検証 | 方法 | 合格条件 |
|---|---|---|---|
| V1 | Layer 0 設定が変わった | `gh api repos/:owner/:repo --jq .squash_merge_commit_message` | `PR_BODY` |
| V2 | Channel 2 が実際に死んでいる | **この PR 自身を `--body`/`--body-file` を渡さずにマージ**し、squash commit 本文と PR 本文を突き合わせる | squash 本文が **PR 本文と一致**し、かつ**ブランチのコミット本文の連結（`* <headline>` 形式）ではない** |
| V3 | 新しい手順に抜け道が無い | 「怠惰な読者」を演じるサブエージェントに Phase 2・3 の**文面だけ**を読ませ、下表 E1〜E5 を通せるか試させる | E1〜E5 のいずれも成立させられない。かつ**新規の抜け道の報告が 2 巡連続で 0** |
| V4 | 既存 hook が無傷 | `npx vitest run .claude/hooks` | 全 pass。かつ `git diff main...HEAD --stat` に `.claude/hooks/` が現れない |

### V2 の識別力 — 「秘密のトークン」設計は PR_BODY と相性が悪い

当初は「ブランチのコミット本文にしか無い一意文字列」を探す設計だった。**これは自壊する。** `PR_BODY` の下では squash 本文 == PR 本文であり、その一意文字列を**説明のために PR 本文へ書いた瞬間に squash 本文へ混入し、V2 が「Channel 2 生存」と偽陽性を出す**。検査対象と判定に使う情報がまた癒着している（本 issue が根治した当のパターン）。

代わりに **本文全体の一致**で測る。候補の 2 つの本文（PR 説明文 / コミット本文の連結）は互いにまったく別のテキストであり、トークンを仕込まなくても識別できる:

```
sha=$(gh pr view <N> --json mergeCommit --jq .mergeCommit.oid)
git log -1 --format=%b "$sha" > /tmp/squash-body
gh pr view <N> --json body --jq .body   > /tmp/pr-body
diff /tmp/squash-body /tmp/pr-body        # 一致 → PR_BODY が効いている
git log -1 --format=%b "$sha" | grep -c '^\* '   # 0 → 連結ではない
```

**`Closes #<閉じたくない issue>` を故障注入に使ってはならない** — 外れたときに本物の issue が閉じる（`AGENTS.md`「稼働中のガードを弱めて測ってはならない。複製に変異を当てる」の同型。ここでは「複製」に当たるものが無いので、**そもそも危険な変異を作らない**）。

V2 はマージ時にしか測れないため、**PR 本文のチェックリストに置く**（`AGENTS.md`「PR のライフサイクル内で閉じるタスク」の振り分け）。

### V3 の合格条件 — 「抜け道が無くなるまで」を客観化する

「怠惰な読者」に渡すのは Phase 2・3 で書いた**文面だけ**（`plan.md` / `research.md` / issue を読ませない）。読者が次のいずれかを**文面から正当化できたら不合格**:

| # | 抜け道 |
|---|---|
| E1 | 「`--body-file` で `Closes` を消したから、この issue は閉じない」と結論できる |
| E2 | 「PR 本文に `Closes` が無ければマージ前チェックは省いてよい」と読める |
| E3 | 「閉じたくない issue は `gh pr merge --body-file` で守れる」と読める |
| E4 | 「hook が誤 close を止めてくれる」と読める |
| E5 | 「PR テンプレートの `Closes #<issue_number>` を消し忘れても無害」と読める |

**打ち切り規則**: 最大 3 巡。3 巡目でなお新規の抜け道が出るなら、**塞いだと書かずに「受容する穴」として文面に明記する**（#489 で「自己申告は文書では反証できない」を正直に残したのと同じ規律）。規範は機構ではないので、完全性を主張してはならない。

---

## SPEC.md 更新要否

**不要。** 本変更は製品（Snotra アプリ）の挙動・IPC 契約・状態遷移・設定キー・データフォーマットのいずれにも触れない。エージェントの運用規範とリポジトリ設定のみ。`SPEC.md` のセクション番号も増減しない。

---

## issue のクローズ

- 受け入れ条件 1（原理的可否を実測で判断）→ `research.md` §4・結論 D
- 受け入れ条件 2（守るなら統合）→ 該当せず
- 受け入れ条件 3（守らないなら根拠を `CLAUDE.md` に記録して閉じる）→ Phase 3

PR 本文に `Closes #488` を書く（Channel 1）。**コミット本文には `Refs #488` を書く**（V2 の識別力を保つため、かつ Channel 2 が死んでいれば流入しないことの実証を兼ねる）。

---

## セルフレビュー

1. **対称コードパス** — コードパスに触れないため N/A。ただし *概念上の* 対称ペア `gh pr create` ↔ `gh pr merge` / `gh issue close` は `research.md` §7 で明示的に判定した。**分類（外部 API の不可逆呼び出し）は対称だが、性質は非対称**: 前者の誤りはリポジトリ状態として観測でき、後者の誤りは人の意図であり真実源が無い。この非対称を Phase 3 で文書化するため、「対称にし忘れた」ではなく「対称にしない」と読める
2. **影響範囲の網羅性** — `COMMIT_MESSAGES` / `squash.*本文` / `body-file` を repo 全体で grep し、追跡ファイル上の該当は `CLAUDE.md` L42–43 のみと確認。`docs/superpowers/plans/` の 1 件は `gh pr create` の例示で無関係。序数参照の腐敗 0 件
3. **境界条件** — (a) Phase 1 の API が 422 を返す（title との組み合わせ制約）→ 測ってから対処、(b) API が失敗する → 設定は現状維持なので文書を `COMMIT_MESSAGES` 側へ倒す、(c) `--body-file` を渡すマージでは既定値が使われない → V2 は既定経路で測る、(d) PR 本文に `Closes` が無い PR（#490/#493 のような振り返り PR）→ 閉じる集合は空。手順 1 がそれを示す
4. **リソース管理** — 生成/破棄ペアなし（プロセス・リスナ・ファイルハンドルを作らない）。唯一の「状態」は GitHub のリポジトリ設定であり、Phase 1 に rollback を明記した（I3）
5. **既存パターンとの整合** — Layer 0 に機構を置くのは GitHub ruleset（#480）と同じパターン。read-back のみで検知器を置かないのも ruleset と同じ扱い。新規パターンを導入していない
6. **YAGNI 違反** — hook を作らない（コード 0 行）。検知器・カナリア・CI チェックも作らない。設計文書 §2 の非目標「安全網の不在を検知する安全網を作らない」に従う
7. **シンプル化の挑戦** — 「Phase 1（設定変更）は本当に要るか？」実害 0 件なら Phase 2 の文書訂正だけでもよい。**しかし Phase 1 が無いと Phase 2 の手順が 2 段チェックになり、片方（コミット本文の走査）は人間が忘れる方である。** 設定 1 つで手順が 1 段になり、Web UI からのマージにも効く。API 1 コール・可逆・コード 0 行という代償で、規範を機構に置き換えられる。よって残す。逆に **Phase 4 は削れるか？** 削れない — 設計文書は本 issue が引用する一次情報であり、訂正しなければ次の読者が同じ誤りを継ぐ
8. **破壊不変条件の明示** — 「壊れたら即アウト」は I1（`pre-bash.mjs` の fail-closed 骨格）のみ。本 issue はそのファイルを 1 行も触らないことで守る。検知手段は V4（既存テストの pass）と、diff に `.claude/hooks/` が現れないこと。Win32 フック・ホットキー・IPC など「戻ってこない」系のリスクは無い

### 5a. check スキルの適用判断

| スキル | 判断 |
|---|---|
| `/plan-review` | **実行する**（常時） |
| `/symmetric-check` | 対称ペアを持つ**コードパス**に触れないため不要。概念上の対称性は §7 とセルフレビュー 1 で判定済み |
| `/cache-check` / `/persistence-check` / `/state-check` / `/race-check` | いずれもコード変更が無く該当せず |

### 5a 実施結果 — `/plan-review`

2 体を並列起動した。**枠組みの独立**を得るため、片方には `plan.md` / `research.md` を読ませていない（`AGENTS.md` Step 2b）。

**要対処: 0 件。** 両者とも結論 A〜E を自分の測定で再現し、真であることを確認した（監査側は `pre-bash.mjs` を stdin 実行して `gh pr merge` → exit 0 / `gh pr create` → exit 2 の対照まで取った）。不変条件 I1〜I5 成立、受け入れ条件 3 項目を充足、`SPEC.md` / `e2e/` / `.github/workflows/**` / `.githooks/**` への影響なし、Phase 1 は可逆。

計画へ反映した指摘:

| 出所 | 指摘 | 反映 |
|---|---|---|
| 監査 | **V2 の「一意文字列」設計が自壊する。** `PR_BODY` 下では PR 本文＝squash 本文なので、説明のために PR 本文へ書いた識別子が混入し偽陽性になる | V2 を**本文全体の一致**で測る設計へ差し替え |
| 監査 | V3 の「抜け道が無くなるまで」に客観的な合格条件も裁定者も無い | E1〜E5 を列挙し、打ち切り規則（最大 3 巡・残れば「受容する穴」と明記）を定めた |
| 監査 | 「序数参照 0 件」の根拠 grep が実際は 5 件ヒットする（結論は正しいが証拠が誤り） | 「5 件ヒット・うち当該節参照は 0」へ訂正 |
| 監査 | 422 で `title` も変えることになったら、文書に title 変更も書く必要がある | Phase 1 に明記 |
| 監査 | `PR_BODY` は squash 本文を冗長化する（git log 肥大） | Phase 2 手順 4 に副作用として明記 |
| 独立導出 | **`.github/pull_request_template.md` が `- Closes #<issue_number>` を既定供給している** | 「触らない」に加え、Phase 2 の手順が前提として明記 |
| 独立導出 | `docs/superpowers/plans/…layers.md:1465` が当該節を**名前で**参照（わたくしの grep は落としていた） | 「触らない」（凍結された実行済み計画）と根拠を記録 |
| 独立導出 | 設計文書 header の「関連:」に `#488` を足すべき | Phase 4 に追加 |

**両者が独立に一致した点**（完全性の能動的証拠）: 変更集合は `CLAUDE.md` 2 箇所 + 設計文書 §2 のみ。CI・リリース・`.githooks/` は squash 本文に非依存。`gh issue close` に hook を足す余地は無い（`settings.local.json` に `gh` の許可が 1 件も無く、既に権限プロンプトが出る）。

**残る未実測（正直に残す）**: `permissionDecision: "ask"` の JSON パース失敗時挙動は未文書・未測定（`research.md` §8）。ただし「hook を作らない」判断は、それとは独立に「hook は Web UI / ユーザー端末からのマージを見ない」（設計文書 §2 三層表・実運用で PR #487 は手動マージ）で支えられており、未実測の主張に依存していない。
