# research — issue #1300

## issue の要約

`/plan-review` の独立レビュー 58 回の実測（issue 本文の表）を受けて、次の 3 点を試行する。

1. Step 1 項目 7（変更で偽になる散文）を、識別子と概念ラベルを受けて**切り詰めずに全件を出す道具**にし、出力件数を `plan.md` に残す。Step 2 の要対処の最多が網羅漏れで、原因に主の grep の切り詰め（`head` / `--include`）が記録された例がある（#1142・#1182）
2. 高リスク計画の既定を Step 2b にし、Step 2 は「issue に書かれていない人間の裁定が計画の前提にある」場合（#917 の理由）に限る
3. 次の 10 件の高リスク計画で、設計変更と主の見落としの件数を記録し、issue の表と比べる

未確定: 試行期間中に Step 2 を選んだ回が 0 なら Step 2 を節ごと畳むか（試行の後に決める事項で、本 PR の範囲外）。

## 関連ファイル・シンボル

| ファイル | 該当 | 関係 |
|---|---|---|
| `.claude/skills/plan-review/SKILL.md` | Step 1 項目 7・「リスク判定」末尾の「Step 2 または Step 2b の**どちらか一方**」・Step 2 冒頭「高リスクのうち、計画の分解を前提に検証する方が適切な場合に使う」・`allowed-tools`（`Bash(gh issue view *)` のみ） | 変更の本体 |
| `.claude/skills/start-issue/SKILL.md` | 5a「網羅性が要件…なら `/plan-review --deep`」・5b セルフレビュー雛形 | 件数を `plan.md` へ残す受け皿の候補（plan-review は `Write` を持たない） |
| `scripts/race-boundaries.mjs` / `.test.mjs` | スキルの母集団を決定的に取る道具の先例。依存ゼロ・純関数を入力注入でテスト・0 件も件数を印字 | 新しい道具の型 |
| `package.json` `scripts` | `race:boundaries` 等 | npm script の追加先 |
| `docs/build-commands.md`「Windows/macOS/Linux で実行可能」 | `race:boundaries` の行 | コマンドの SSOT。`G-build-commands` が npm script の実在を照合する |
| `vitest.config.ts` | `scripts/**/*.test.mjs` を include 済み | テストの追加に設定変更は不要 |
| `docs/check-skill-skeleton-design.md`「必須 1 — 母集団（何を検査するか）」 | 「母集団を得る手順が決定的でないなら結論を持たない」 | 道具化の根拠 |
| `docs/adr/ADR-risk-tiered-plan-review.md` | 却下: 常時 fan-out・計画変更ごとの再導出・独立レビュー全廃 | 本変更は高リスク内の方式選択のみで、どの却下にも触れない（体数は 1 のまま） |
| `AGENTS.md`「条件別チェック」網羅性行・`docs/development-principles.md`・`.claude/rules/governance-docs.md`・各 ADR | `/plan-review`「Step 2b」への参照 | Step 2b の見出しを変えなければ着地は保たれる（G-heading-refs） |

## 再利用できる既存パターン

- `race:boundaries` の契約（依存ゼロ・決定的・0 件の種別も印字・使用法誤りは exit 2・分類本体は純関数）をそのまま型にする
- 散文の判定は `race-boundaries.mjs` の `isCommentLine` と同じ発想（行頭のコメント記号）。ただし同関数は「コメントを**除く**」側で使っており、こちらは「コメント**だけ**を拾う」側。拡張子集合も違う（`.md` を含む）ので共通化はしない（片方だけ変わる将来が挙がる: race 側は `.md` を除外し続ける）
- `git grep -n -F -e <term>...` は追跡ファイルを対象にし、切り詰めない

## 技術的制約

- `/plan-review` の `allowed-tools` に Bash は `gh issue view *` しか無い。道具を Step 1 から打たせるなら `Bash(npm run prose:sweep *)` 相当を足す必要がある（`.claude/` 配下の編集は分類器に flag されうる・メモリ）
- plan-review は `Write` を持たない（自作自演の防止・`ADR-norm-review-seeding.md`）。件数を `plan.md` に残すのは呼び出し側（`/start-issue` 5b のセルフレビュー節）になる
- Step 2b のモデルは親継承（Step 2 は sonnet 固定）。既定を 2b へ寄せると 1 回あたりの費用は上がる。体数は 1 のままで ADR の「常時 fan-out」却下には当たらない
- `--deep` は `argument-hint` と「リスク判定」の 1 条件（高リスクへ昇格）にしか現れず、**高リスクの中で Step 2 / 2b のどちらを選ぶかは定義していない**（#849 以来・3b 所見 1 で訂正）。既定が 2b になると `--deep` と既定の差がさらに読めなくなる
- `workspace/` は `/implement` Step 4 で撤去される。試行の 10 件の記録を各 `plan.md` に置くだけでは、squash 後は PR の head からしか復元できない（issue 本文の実測方法がまさにそれ）
- ガバナンス文書の変更ゆえ `npm run governance:check`（カテゴリ F）。`.mjs` 追加ゆえ `npm test`
- セーフティネット（skills）の変更 — `CLAUDE.md` 最重要ルール 2 の合意は issue 作成者＝ユーザー本人の提案で取れている前提。ただし細部は計画承認で確認する

## 未解決の疑問

1. 道具の母集団: `.md` 全行 + コード中のコメント行でよいか。ADR・`docs/superpowers/`（凍結された歴史）を除外するか、分類して出すか（「切り詰めない」と矛盾しない形は分類）
2. `plan.md` の変更ファイル一覧との突き合わせを道具がするか（`--plan` で未掲載ファイルを印す）、一覧を出すだけか
3. 試行 3（次の 10 件）の記録をどこへ置くか・#1300 を本 PR で閉じるか
4. `--deep` の意味を本 PR で定義し直すか、触らないか

## 敵対的調査（3b）の採否 — `workspace/adversarial-1300.txt`

### 壊せた項目
| # | 所見 | 採否 | 理由（自分で一次資料を当てた結果） |
|---|---|---|---|
| 1 | `--deep` は「リスク判定」で高リスクへ昇格させると定義されている。未定義なのは 2 / 2b のどちらを選ぶかだけ | 採る（表現の訂正） | SKILL.md:38 で確認。ユーザー裁定「触らない」により本 PR は `--deep` を変えず、start-issue 5a の文も残す（「高リスクへ昇格」の意味は既定 2b 下でも真） |
| 2 | `allowed-tools` は制限ではなく事前許可。足さなくても実行不能ではない | 採る（理由の差し替え） | 足す理由は「実行不能」ではなく先例（race-check は `Bash` を宣言）どおり**宣言と実行を一致させる**ため。`Bash(npm run prose:sweep *)` に限定して足す |
| 3 | race-boundaries から流用できるのは契約の殻だけ（全語の印字・0 件の明示・exit 2・純関数） | 採る | 母集団が差分ではなく全コーパスである点で設計が違う。`--base` と diff 解析は持たない |
| 4 | コメント行で絞ると、道具の中に切り詰めを再導入する（行末コメント・文字列中の散文が落ちる） | 採る（設計変更） | **行の種類で絞らず全ヒットを出す**。代わりに面で分類する。`isCommentLine` 相当は持たない |
| 5 | 雛形に「設計変更」「主の見落とし」の欄が無く、試行 3 を記録できない | 採る | 5b 雛形へ試行欄を足し、撤去条件を隣に書く |
| 6 | 偽・曖昧になる散文: plan-review :40「どちらか一方」・:44 Step 2 冒頭、start-issue 5b 雛形。ADR-risk-tiered「観点を限定した1体」 | 一部採る | :40・:44 は書き換える。:79（Step 3 の方式欄）は両方式を並べるだけで偽にならない。ADR は凍結された歴史ゆえ書き換えない（当時の決定の記述として真） |

### 壊せなかった項目
G-heading-refs の着地（見出し文言を保てば成立）／`git grep` の日本語ラベル（3 経路で 70 件一致）／共通化しない結論／vitest include・build-commands・モデル指定の事実／ADR の「常時 fan-out」却下に当たらないこと。

### ⚠️
| # | 所見 | 採否 |
|---|---|---|
| 1 | `allowed-tools` の効力は公式文書未確認 | 上の 2 で「一致させる」を理由にしたので結論は効力に依存しない |
| 2 | race:boundaries は未追跡ディレクトリを `?? dir/` に畳んで落とすはず | 本 issue の範囲外。実測していないので計画に入れず、報告で挙げる |
| 3 | 概念ラベルが物理改行を跨ぐと 0 件 | 採る。語ごとの 0 件印字を「ラベルが外れている」合図にし、SKILL.md 項目 7 に一言添える |
| 4 | 「#917 の理由」の出所がリポジトリに無い | 一部採る。#917 は PR（#835 の固定高化）で、本文の「人間が確定した設計判断（2026-08-04）」表が issue #835 に書かれていない裁定の実例。条件文には番号でなく述語を書く |
| 5 | 既定 2b に寄せると同難度の Step 2 の比較対象が消える | 記録のみ。issue 本文が「平均の差は難度と交絡」と既に認めており、試行は件数を表と比べる設計 |

## 設計の確定（未解決の疑問への答え）

1. 母集団: `git grep --untracked -n -F` で追跡＋未追跡（`.gitignore` 尊重）、`workspace/` を除く（計画自身が語を含むため）。**行の種類で落とさない**。面で分類する: 常時ロード規範（ルート `CLAUDE.md`・`AGENTS.md`）／skills・rules・hooks（`.claude/`・`.githooks/`）／生きた文書（`SPEC.md`・各 `CLAUDE.md`・`docs/` の ADR・superpowers 以外・その他 `.md`）／凍結された歴史（`docs/adr/`・`docs/superpowers/`）／コード（それ以外）
2. `--plan <path>` で、ヒットのあったファイルのうちパス文字列が plan に現れないものに印を付け、未掲載件数を出す（部分一致に留める）
3. 試行 3: 本 PR は `Refs #1300`。#1300 を台帳として開けたまま、記録は各 `plan.md` のセルフレビュー節（ユーザー裁定 2026-10-25）
4. `--deep`: 触らない（ユーザー裁定 2026-10-25）
