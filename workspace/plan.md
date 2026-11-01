# 計画 — issue #1328 永続化の規則の正本を 1 つに決める（案 c）

## 目的と受け入れ条件

規則（編集時に守る命令）の正本を `snotra-core/CLAUDE.md`「データ永続化の注意」、検証の仕方と根拠の正本を `/persistence-check` に分け、互いに指し合う。

受け入れ条件:
1. スキル `.claude/skills/persistence-check/SKILL.md` の「背景」の宣言が、役割ごとの正本を述べる（「SSOT は CLAUDE.md」の一文が無い）
2. スキル本文に、CLAUDE.md「データ永続化の注意」の規則の本文・帰結を言い直した文が無い。対象: 旧 :52、:80 の「→」行、:82-84 の 3 区分、:73 の「Legacy で受理し移行」、:97 の TOML 移動手順（research.md「重複の対応表」）
3. スキル固有の検査項目は残る: Step 1 分類、:45-47、:58-68、:70-72、:74、:96、Step 6、出力。**残す基準**: 検査の問い（`[ ]`）・失敗の型（「〜すると〜が起きる」＋インシデント）・検証の仕方として書かれた行は残し、命令形で規則を述べる行は残さない。:45-47（版を上げたか等の問い）、:58-63（問い）、:70（失敗の型・#461）、典型パターン 1〜5（失敗の型）はこの基準で残る。:72「正しい向き」は、CLAUDE.md :60 に無い検証の仕方（凍結バイト列は旧ビルドかリファクタ前のコードから採るのが最も厳密）を持つので残す
4. Step 5 の IndexCache・history の参照が、**フェンスの外**に**正準形**（`` `snotra-core/CLAUDE.md`「…」 ``）で書かれ、項目の要約（7 項目の列挙・3 者の列挙）が無い。history の見出しは実見出し「history.rs のキー正規化に関するチェックリスト」
5. `snotra-core/CLAUDE.md`「データ永続化の注意」の 6 行の規則は**字面を変えない**（外部参照元 7 件が腐らないため）。見出しの直下に、根拠と検証の手順は `/persistence-check` が持つという 1 行を足す
6. `npm run governance:check` が green
7. 変異: Step 5 の history の参照ラベルを旧い誤ラベルへ戻した複製で、`governance:check` が「見出し参照が着地しない」で赤になる

## 変更ファイル一覧と対象

| ファイル | 対象 |
|---|---|
| `.claude/skills/persistence-check/SKILL.md` | 「背景」:18 の宣言／Step 2 :52／Step 3 :73／Step 4 :79-86 のフェンス／Step 5 :91-98 のフェンス |
| `snotra-core/CLAUDE.md` | 「データ永続化の注意」の見出し直下（:53 の後）に 1 行足す |

触らない: `AGENTS.md`、`.claude/rules/snotra-core.md`（`/persistence-check` を指すだけで写しを持たない）、外部参照元（コード 4 件・`PERFORMANCE.md:816,993`・`config/io.rs:59`・ADR）、典型パターン 1〜5（根拠としてスキルに残す）。

## 実装の中身（文案）

- :18 → 「このリポジトリの最頻出の高リスク領域が on-disk 永続化（…）で、#338・#343・#394・#461 が繰り返し噛まれてきた。**規則（編集時に守る命令）の正本は `snotra-core/CLAUDE.md`「データ永続化の注意」**であり、ここには写さない。**このスキルが正本として持つのは、その規則を満たしたかの検証の仕方と、規則の根拠になった失敗の型**（下の典型パターン・各 Step のアンチパターン）である。」
- :52 → 削除（典型パターン 2 の「バイト列同一でも値の解釈が変わりデータ破損」と Step 2 フェンス内の「セマンティクスが変わる」で足りる。規則は CLAUDE.md）。**:52 の「#338 の座標系変更」は誤った番号である**——#338 は「Config::load() が TOML parse エラーを黙殺し全設定を初期値にフォールバックする」（`gh issue view 338`）で、座標系の変更ではない（スキル新設の #464 から誤っていた）。番号は移さず、典型パターン 2 の例「絶対座標→モニター相対」に正本 `snotra-core/src/window_data.rs` の `//!` を添える。:79 の #338（デコード失敗・データ喪失）は正しい用法なので残る
- :73 → 「**serde 表現変更（enum variant / untagged / flatten / tag）**: 旧形式が deserialize に失敗すると全設定リセット＝データ損失（#394）。旧形式の受理の仕方は `snotra-core/CLAUDE.md`「データ永続化の注意」」（「`apply_migrations()` で移行」は CLAUDE.md :59 に無く、:59 は逆に「`apply_migrations()` では救えない」と言うので、移行の段は書かない）
- Step 4 フェンス: `[ ]` の問い 3 つは残し、:80 の「→」行と :82-84 の 3 区分を削る。:81 は末尾のコロンで 3 区分を導いているので、「[ ] 読み込み失敗を種類で扱い分けているか（`Config::load` 型）」と言い切る形にする。フェンスの直後（外）に「区分と各枝の保全の正本は `config/io.rs` の `Config::load_from_dir_reporting` の doc」
- Step 5: フェンスをやめて箇条書きにする（フェンス外へ出す）。
  - IndexCache 系: `snotra-core/CLAUDE.md`「IndexCache バージョン変更チェックリスト」の全項目を満たすか
  - history 系: `snotra-core/CLAUDE.md`「history.rs のキー正規化に関するチェックリスト」の全項目が揃うか（数を写さない）
  - 各参照は 1 物理行に収める（折ると G-folded-heading-refs が赤になる）
  - Config を deserialize する新経路を足したなら、`apply_migrations()` の適用要否を判断したか（:96 そのまま）
  - TOML フィールドを移したなら、`snotra-core/CLAUDE.md`「データ永続化の注意」の移し方に従ったか
- `snotra-core/CLAUDE.md` :53 の後: 「根拠（どの失敗から来た規則か）と、満たしたかの検証の手順は `/persistence-check` が持つ。」

## 実装順序

1. SKILL.md を編集（上の文案）
2. snotra-core/CLAUDE.md に 1 行足す
3. `npm run governance:check`
4. 変異（複製）で受け入れ条件 7 を測る

## 不変条件と異常系

- CLAUDE.md の 6 行の字面不変 — 検知: `git diff main -- snotra-core/CLAUDE.md` が追加 1 行だけ
- スキルの母集団（対象 struct）・Step の構成（1〜6）・出力節を変えない（`docs/check-skill-skeleton-design.md`）— 検知: diff の目視、見出し行の差分が無いこと
- 正準形の参照は governance:check が着地を照合する — 検知: 受け入れ条件 7 の変異

## テスト方針と検証コマンド

- カテゴリはガバナンス文書のみ（`docs/build-commands.md`「変更後の検証チェックリスト」の該当カテゴリ）。`npm run governance:check`
- 変異は `/implement` の委譲先が複製（一時ディレクトリへ `git worktree` か、`scripts/governance` の in-memory snapshot）に当てる: Step 5 の history ラベルを「history.rs のキー正規化チェックリスト」へ戻す → 赤、戻す → 緑。赤の理由が「見出し参照が着地しない」であることを出力で読む。照合は前方一致なので、ラベルを短く切る変異は緑のまま通る——**誤ラベルへ戻す形で測る**

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（製品の挙動を変えない）
- `AGENTS.md`・`.claude/rules/snotra-core.md`: 不要（写しを持たない。指す先の名前は変わらない）

## 作業項目

### Phase 1 — スキル
- [x] :18 の宣言を役割ごとの正本の文へ書き換える
- [x] :52 を削り、典型パターン 2 の例へ `window_data.rs` の `//!` を添える（誤った #338 は移さない）
- [x] :73 の規則部分を「旧形式の受理の仕方は CLAUDE.md」への参照に替える
- [x] Step 4 フェンスから「→」行と 3 区分を削り、:81 を言い切る形にし、フェンスの外に doc への参照を足す
- [x] Step 5 をフェンスの外の箇条書きにし、正準形の参照 2 つ（history は実見出し）と :96、TOML 移動の参照にする

### Phase 2 — CLAUDE.md
- [x] 「データ永続化の注意」直下に `/persistence-check` を指す 1 行を足す

### Phase 3 — 検証
- [x] `npm run governance:check` green
- [x] 変異で受け入れ条件 7 を確かめる（委譲先）
- [x] 受け入れ条件 2 を grep で確かめる（「次の通常 save」「NotFound」「Legacy {..}」「skip_serializing」がスキルに無い）

### Phase 4 — 1 巡目のレビュー対応（`workspace/review-1328-r1.md`・sha ade83abf）
- [x] M1: 「受理した旧形式を `apply_migrations()` で移す」を Step 3 の検査として戻す（`migrate.rs:187` `migrate_instant_legacy_commands` で実在を確認。計画が根拠にした「CLAUDE.md :59 と食い違う」は誤りで、:59 が言うのは parse に失敗した場合だけ）
- [x] M2・L2: Step 4 の参照を、区分は `LoadOutcome`、保全は `load_from_dir_reporting` の doc と分け、ルートからのパスで書く
- [x] L1: CLAUDE.md に足した 1 行を全称でない形（「検証は `/persistence-check` で行う」）へ弱める
- [x] L3: 典型パターン 2 の出典に述語を足す
- [x] L4: Step 5 の Config の新経路の行は、CLAUDE.md「Config のデシリアライズ経路」の写しだった（敵対的調査 B3 で「スキル固有」として採ったのは誤り。B3 は CLAUDE.md の 6 行だけを見ていた）。正準形の参照にする
- [x] 変異（受け入れ条件 7）: 誤ラベルで EXIT=1（G-heading-refs の「見出し参照が着地しない」）、フェンス内の対照は EXIT=0、巻き戻し後の hash は sha の版と一致

### Phase 5 — 2 巡目（`workspace/review-1328-r2.md`・sha 536b58c5）
- [x] 1 巡目の 6 件の解消を委譲先が同じ方法で再測定（全件解消）。L4 と同じ形の写しは、スキル全行を CLAUDE.md 全体と突き合わせて無し（最大の重なり 25%、既知の写しは 50%）。「Config のデシリアライズ経路」の変異で EXIT=1、巻き戻し後の hash 一致
- [x] L5 ⚠️（M1 で戻した行が規則の中身を持つ）は採らない: 直すには CLAUDE.md の 6 行の字面を変える必要があり、受け入れ条件 5 に反する。また `validate.rs:77` が受理後も `InstantAction::Legacy` を扱っており、移行が必須とは言い切れない。「見る」という問いの形にとどめる
- [x] 却下した案 (a)(b)(d) を `docs/adr/ADR-persistence-rule-ownership.md` へ回収

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-11-01 / 問い: "計画の草案ができました。ご承認か、`workspace/plan.md` への注釈をお願いいたします。" / 回答: "承認します"

## 散文の全件列挙の裁定（prose:sweep）

`npm run prose:sweep -- --plan workspace/plan.md "データ永続化の注意" "persistence-check" "キー正規化" "load_from_dir_reporting"` → 語 4 / 総件数 61 行 / ★未掲載 26。いずれも計画へ足す必要は無い:
- `.claude/skills/implement/SKILL.md:94`・`.claude/skills/retrospective/SKILL.md:64`: スキル名を列挙するだけ。名前は変わらない
- コード（`config/tests.rs:311`・`indexer/cache/tests.rs:267,396,457`）: 「データ永続化の注意」の規則を引用している。6 行の字面を変えないので腐らない
- `config/io.rs`・`config/io/tests.rs`・`migrate.rs` ほか: `load_from_dir_reporting` の実体と呼び出し。シンボルは変えない
- `paths.rs`・`history.rs`・`query.rs` の「キー正規化」: 別概念の一般名詞
- 凍結された歴史（ADR・`docs/superpowers/`）: 凍結。`opus5-skill-tuning-design.md:17` が挙げる番号（#338/#343/#394/#461/#646）は、:79・:86・:73・:70・:74 に残るので、どれもスキルから消えない

## セルフレビュー

- リスク: 高（skills・ガバナンス文書を変更）
- 照合 1〜5: 1 issue の 2 要件（正本の決定＝案 c で合意済み、寄せる＝Phase 1・2）に作業項目あり／2 境界: フェンス内外・正準形の有無 → 受け入れ条件 4・7 で検証／3 新しい状態・リソースは無し／4 より単純な案: (d) は AGENTS.md に反するので却下済み／5 不変条件（6 行の字面・Step 構成）に diff の検知あり
- plan-review: 計画準拠の独立レビュー 1 体（Step 2。案 c は issue に無い人間の裁定なので Step 2b ではない）
- エージェント数: 1（plan-review）＋ 3b の 1 体
- 要対処: 3 件（出典 `workspace/plan-review-1328-ownership.md`）すべて反映。(1) 残す基準を受け入れ条件 3 に明文化し、:72 を残す理由を書いた (2) :73 の参照を「受理の仕方」へ弱め、CLAUDE.md :59 と食い違う「移行」を落とした (3) Step 4 の :81 を言い切る形にした。軽微のうち「3 者」→「全項目」、変異は誤ラベルへ戻す形、1 物理行の制約を反映。⚠️「#338 の 2 か所は同じ事象か」は自分で `gh issue view 338` を引いて裁定した。:52 の用法が誤りなので、#338 を典型パターン 2 へ移す計画を撤回し、`window_data.rs` の `//!` を添える形へ変えた
- 未検証: 計画どおりに書いた版での `governance:check` の実走と、変異（受け入れ条件 7）。どちらも実装時に測る（Phase 3）
- 散文の全件列挙: 語 4 / 総件数 61 行 / ★未掲載 26（計画へ足した 0）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 3 / 主の見落とし 3
