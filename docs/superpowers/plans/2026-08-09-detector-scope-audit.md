# 検知器の射程監査 実装計画（#1008）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 「網羅を守る」と読める検査のうち実際には守っていないものを数え上げ、各件の射程を実際の姿へ揃える。

**Architecture:** 母集団は SSOT のツール自身に問うて全列挙する（Rust は `cargo test -- --list`）。
篩は「網羅を主張しているか」という意味の問いではなく「**この一覧の母集団は誰が知っているか**」という
構造の問いで行い、4 分類（コンパイラ / ソーステキスト / ファイルシステム / 外部設定）へ振る。分類が
決まると当てるべき変異の形と倒し先が一意に決まる。

**Tech Stack:** Rust（`cargo test`）・Node（`scripts/governance-check.mjs`）・PowerShell（`scripts/lib/SnotraTraceInvariants.psm1`）

**設計書（SSOT）:** `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md`

## Global Constraints

- **ブランチは `chore/detector-scope-audit`**（作成済み）。`main` へ直接コミットしない
- **変異は絶対にコミットしない。** 各変異の直後に元へ戻し、`git status --short` が**変異ファイルについて空**であることを目で確認する
- **中間成果物は設計書へ追記して commit する。** 候補一覧・分類・変異結果を会話やセッション固有の一時ディレクトリだけに置かない（落ちると実施の有無すら区別できない）
- **各件の射程は当該ソースの doc コメントへ書く。** 設計書へ写しを置かない（`AGENTS.md`「文書に事実の写しを増やす変更」）
- **やらないこと**: 条項の新設・`governance:check` への検査追加（設計書 §1・実測を根拠に否定済み）
- **`*.md` を編集したら `npm run governance:check` を手で走らせる**（PostToolUse hook は `*.md` に走らないため沈黙は「合格」を意味しない）
- **`.rs` を編集したら PostToolUse hook が fmt / clippy / test を自動実行する**（沈黙 = 合格・手動再実行は不要）
- **コミットメッセージは日本語**、本文に `Refs #1008` を含める。末尾に `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`
- **複数行のコミットメッセージは HEREDOC で渡さない**（pre-bash hook が拒否する）。Write ツールで一時ファイルへ書き `git commit -F <path>`。パスの区切りは先頭から末尾まで `/`
- **本計画のチェックボックスは pre-bash hook の管轄外である（意図的な分離）。** hook が未チェック項目で `gh pr create` を拒むのは `workspace/plan.md` だけで、`docs/superpowers/plans/` は対象にしていない——**superpowers 配下と `workspace/` 配下では管轄するスキルが違うため、チェックボックスの管理対象を後者に絞ってある**。ゆえに本計画の完了は機構ではなく **Task 8 Step 1 の受け入れ検算**が担う

---

### Task 1: Rust 905 件の全列挙と篩

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md`（§9 として候補一覧を追記）

**Interfaces:**
- Produces: 設計書 §9「候補一覧」に、Rust の候補が `crate/path.rs::test_name` の形で列挙されている

- [ ] **Step 1: 母集団を cargo に問い、設計書の記載と一致するか確かめる**

```bash
cargo test --workspace -- --list 2>/dev/null | grep -c ': test$'
```

Expected: `905`。**ズレたら設計書 §3 の数字を実測値へ直す**（数字が腐っているのであって、実測が誤りではない）。

- [ ] **Step 2: テスト名と定義位置の一覧を作る**

```bash
cargo test --workspace -- --list 2>/dev/null | grep ': test$' | sed 's/: test$//' | sort > /tmp/all-tests.txt
```

パスは実行セッションの一時ディレクトリを使う（`/tmp` はプレースホルダ）。この一覧が母集団であり、
以降 grep でこれを置き換えない。

- [ ] **Step 3: 一覧を全数読み、一覧・配列・`match` の腕を走査している検査を候補として抜く**

判定材料は次の 3 つ。名前だけで判断がつかないものは**そのテスト本体を読む**。

1. 名前（`every` / `all` / `covers` / `exhaustive` / `unique` / `distinct` / `roundtrip` / 網羅）
2. doc コメント・近傍コメントの文言
3. テスト本体が走査している対象（手書き配列・`match` の腕列挙・`HashMap` リテラル）

**大半の検査は一覧を走査していないので、ここで候補は大きく絞れる。**

- [ ] **Step 4: 候補を設計書 §9 へ追記する**

`## 9. 候補一覧` を新設し、`crate/path.rs::test_name` の形で列挙する。この時点では分類も判定も書かない
（Task 3 / Task 4 で埋める）。**件数を明記する。**

- [ ] **Step 5: 検証とコミット**

```bash
npm run governance:check
```

Expected: 全検査 passed。その後コミット（メッセージは一時ファイル経由）:

```
docs: Rust 905 件から射程監査の候補を抜く（#1008）
```

---

### Task 2: ガバナンス 19 件・スモーク 3 件の全列挙と篩

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md`（§9 へ追記）
- Read: `scripts/governance-check.mjs`, `scripts/lib/SnotraTraceInvariants.psm1`

**Interfaces:**
- Consumes: Task 1 が作った設計書 §9「候補一覧」
- Produces: 同 §9 に `G-*` と `H*` の候補が追記されている

- [ ] **Step 1: G-* 19 件を列挙する**

```bash
grep -n "id: 'G-" scripts/governance-check.mjs
```

Expected: 19 件。ズレたら設計書 §3 を実測値へ直す。

- [ ] **Step 2: 各 G 検査が母集団をどう決めているかを読む**

**この層は「手書きの一覧」と「走査で得た一覧」が混在している。** 例えば `G-references` は対象文書を
35 件持ち、`G-module-index` は各サブディレクトリの `CLAUDE.md` を読む。**対象文書の一覧が手書きなら
それ自体が候補である。**

- [ ] **Step 3: スモークの 3 件を確認する**

```bash
grep -n '\$script:Invariants' scripts/lib/SnotraTraceInvariants.psm1
```

Expected: `@('H1', 'H4', 'H5')`。**この手書き一覧を返す `Get-SnotraTraceInvariantNames` 自身が候補である**
（H2 / H3 が欠番であることも記録する — 撤去されたのか採番の飛ばしなのかは、判定に要るなら git log で確かめる）。

- [ ] **Step 4: 候補を設計書 §9 へ追記し、合計件数を書く**

- [ ] **Step 5: 検証とコミット**

```bash
npm run governance:check
```

コミットメッセージ:

```
docs: ガバナンス 19 件とスモーク 3 件から候補を抜く（#1008）
```

---

### Task 3: 候補の分類と、篩の見落としの検算

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md`（§9 の各候補へ分類を付す）

**Interfaces:**
- Consumes: 設計書 §9「候補一覧」（Task 1 / Task 2 の全候補）
- Produces: 各候補に 4 分類のいずれかが付き、分類ごとの件数が記録されている

- [ ] **Step 1: grep 2 軸を走らせ、篩の見落としを検算する**

```bash
# 構文パターン起点
grep -rn "let all\b\|let ALL\|const ALL\|= \[$\|\.iter()\.map(" --include=*.rs . | grep -v target
# 全称文言起点
grep -rn "網羅\|すべての\|全 variant\|全て" --include=*.rs . | grep -v target
```

**grep の結果を母集団にしてはならない。** 見るのは「§9 に無いのに引っかかったもの」だけで、
それが出たら**篩の基準の側を直して Task 1 Step 3 をやり直す**。差分ゼロなら篩は閉じている。

- [ ] **Step 2: 各候補に「この一覧の母集団は誰が知っているか」を問い、分類する**

| 分類 | 母集団の SSOT | 見分け方 |
|---|---|---|
| **C** | コンパイラ | 走査対象が enum variant / struct field / trait impl |
| **S** | ソーステキスト | 走査対象が `const` の集合、モジュール内の関数など、型を持たない宣言の集まり |
| **F** | ファイルシステム | 走査対象がファイル・ディレクトリ・glob |
| **X** | 外部設定 | 走査対象が CI job・npm script・TOML/JSON の項目 |

**一意に決まらない件は分類を 2 つ付ける**（設計書 §7 の留保）。その場合 Task 4 で変異を分類ごとに 1 つずつ当て、
どちらの足し忘れも検知しないなら**2 件として数える**。

- [ ] **Step 3: 分類ごとの件数を設計書 §9 の冒頭へ書く**

**とくに C（コンパイラ）の件数を明記する** — Task 6 の derive 導入判断がこの数字に依る。

- [ ] **Step 4: 検証とコミット**

```bash
npm run governance:check
```

コミットメッセージ:

```
docs: 候補を母集団の SSOT で 4 分類する（#1008）
```

---

### Task 4: 定型変異で射程不一致を確定する

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md`（§10 として仕分け表を追記）
- 変異は各候補の対象ファイルへ一時的に当て、**必ず戻す**

**Interfaces:**
- Consumes: 設計書 §9（分類済みの候補一覧）
- Produces: 設計書 §10「仕分け表」— 各候補について変異の結果（①②③のいずれか）と、③の件数

- [ ] **Step 1: 既知 2 件を対照として先に測り、レシピの妥当性を確かめる**

**答えが分かっている 2 件でレシピを検算してから本番へ入る**（`AGENTS.md`「計画に書いた判定ロジックは
実装前に代表入力で実行して測る」）。

`src-tauri/src/events.rs` へ変異（分類 S）:

```rust
pub(crate) const AUDIT_PROBE: &str = "audit-probe";
```

Run: `cargo test -p snotra event_names`
Expected: **PASS**（= ③ 射程不一致。doc がそう明記しているとおり）

`src-tauri/src/startup.rs` の `enum Phase` へ変異（分類 C）:

```rust
    AuditProbe,
```

Run: `cargo test -p snotra count_matches_the_enum_declaration`
Expected: **FAIL**（= ② 検査が守っている）。ただし `index()` が網羅 match ゆえ**コンパイルが先に落ちる
可能性がある** — その場合は ① であり、`index()` / `key()` の腕も同時に足してから測り直す。

**両方が期待どおりでなければ、レシピが誤っている。** 先へ進まず Step 1 を直す。

- [ ] **Step 2: 変異を戻し、作業ツリーが clean であることを確認する**

```bash
git status --short
```

Expected: 変異ファイルが 1 件も現れない。

- [ ] **Step 3: 分類ごとの定型変異を全候補へ当てる**

| 分類 | 当てる変異 | 走らせるもの |
|---|---|---|
| **C** | 対象 enum へ `AuditProbe,` を足す（網羅 `match` がコンパイルを止めたら腕も足す） | `cargo test -p <crate>` |
| **S** | 対象モジュールへ `pub(crate) const AUDIT_PROBE: &str = "audit-probe";` 等、同種の宣言を 1 つ足す | `cargo test -p <crate>` |
| **F** | 対象ディレクトリへダミーファイルを 1 つ置く | `npm run governance:check` |
| **X** | 対象設定へ項目を 1 つ足す | `npm run governance:check` / Pester |

**いずれの変異も「一覧の側には足さない」。** それが足し忘れの再現である。

- [ ] **Step 4: 結果を 3 分岐で記録する**

| 結果 | 意味 | 手当て |
|---|---|---|
| **①コンパイルが通らない** | コンパイラが守っている | 不要（検査の射程が狭くても害が無い） |
| **②テスト/検査が落ちる** | その検査が守っている | 不要 |
| **③どちらも通る** | **射程不一致** | Task 5 か Task 6 で倒す |

**①を「守られている」と数えてよいのは、コンパイラが止めたのが変異そのものだったときだけである。**
無関係な箇所でコンパイルが落ちた場合は変異の当て方が誤っており、測り直す。

- [ ] **Step 5: 各候補の変異を戻し、作業ツリーが clean であることを確認する**

```bash
git status --short
```

- [ ] **Step 6: 設計書 §10 へ仕分け表を書き、③ の件数を明記する**

**③ が 0 件なら「既知の 2 件で全部だった」と記録してここで終える**（設計書 §6・それも成果である）。
その場合 Task 5 / Task 6 / Task 7 はスキップし、Task 8 へ進む。

- [ ] **Step 7: 検証とコミット**

```bash
npm run governance:check
git status --short   # 変異が残っていないことの最終確認
```

コミットメッセージ:

```
docs: 定型変異で射程不一致を確定する（#1008）
```

---

### Task 5: 射程を doc へ書く（既定の倒し先）

**Files:**
- Modify: ③ と判定された各候補の**対象ソースファイル**（doc コメント）

**Interfaces:**
- Consumes: 設計書 §10 の ③ 群
- Produces: 各対象ソースの doc に射程が明記されている

**このタスクは ③ が 1 件以上あるときだけ実行する。**

- [ ] **Step 1: ③ の各件について「足し忘れが製品の欠陥になるか」を判定する**

- **ならない** → このタスクで doc へ射程を書く
- **なる** → Task 6 で機構へ倒す

判定の根拠を 1 行で書けること。書けないなら doc へ倒す（狭い保証で十分、が既定である）。

- [ ] **Step 2: 射程を doc コメントへ書く**

`src-tauri/src/events.rs` の形を範とする。**書くのは 3 点**:

1. その検査が実際に見ているもの（例: 「ここに並べた 9 種が互いに異なることだけを見る」）
2. 見ていないもの（例: 「定数を新設してもこの配列へ足さなければ検査対象にならない」）
3. **将来の追加を守る機構ではない**という否定を明示すること

**「〜を守る」「網羅する」と読める表現を残さない。**

- [ ] **Step 3: doc を変更したので rustdoc を走らせる**

```bash
cargo doc --workspace --no-deps --document-private-items
```

Expected: 警告なし（intra-doc link 切れは CI でのみ発火し PostToolUse hook は沈黙する・`.claude/rules/comments.md`）

- [ ] **Step 4: コミット**

```
docs: 射程不一致の N 件へ実際の保証範囲を明記する（#1008）
```

---

### Task 6: 機構へ倒す（足し忘れが製品の欠陥になる件のみ）

**Files:**
- Modify: Task 5 Step 1 で「機構へ倒す」と判定された候補の対象ファイル
- Modify: `Cargo.toml`（derive crate を導入する場合のみ）

**Interfaces:**
- Consumes: Task 5 Step 1 の判定
- Produces: 足し忘れが検知される状態、およびそれを実証するテスト

**このタスクは「機構へ倒す」と判定された件が 1 件以上あるときだけ実行する。**

- [ ] **Step 1: derive 導入の是非を、Task 3 Step 3 の C 件数を根拠に判断する**

- **C が少数（目安 1〜2 件）** → `include_str!` 走査（`startup.rs` の姿）で個別に倒す。依存を増やさない
- **C が多数** → `strum` 等の derive 導入を検討する。**ただしこの issue で入れるとは限らない** —
  依存追加はワークスペースに前例が無く（実測）、射程を超えるなら**別 issue へ切り、この issue では
  Task 5 の doc 明記で止める**。その判断と根拠を設計書 §10 へ書く

- [ ] **Step 2: 失敗するテストを書く（Red）**

倒す件ごとに、**足し忘れを再現する変異が入った状態で落ちるテスト**を書く。`startup.rs` の
`count_matches_the_enum_declaration` が範である（`include_str!` で自分のソースを走査し、宣言と
`COUNT` を照合する）。

- [ ] **Step 3: テストが落ちることを確認する（Red の実測）**

変異を当てた状態で走らせる:

```bash
cargo test -p <crate> <test_name>
```

Expected: **FAIL**。落ちなければ検査が機能していない — 先へ進まない。

- [ ] **Step 4: 変異を戻し、テストが通ることを確認する（Green）**

```bash
git status --short          # 変異が残っていないこと
cargo test -p <crate> <test_name>
```

Expected: PASS

- [ ] **Step 5: 新しい検査自身の射程を doc へ書く**

**機構へ倒しても射程は残る。** `startup.rs` の doc が「改名したらこの検査も直す」と脆さを明記している
のと同じく、**その検査が守らないもの**を書く。

- [ ] **Step 6: コミット**

```
test: 足し忘れを検知する機構へ倒す（#1008）
```

---

### Task 7: 倒した後の再変異で実測する

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md`（§10 へ再変異の結果を追記）

**Interfaces:**
- Consumes: Task 5 / Task 6 の変更
- Produces: 設計書 §10 に「倒した後の挙動が doc どおりであること」の実測が記録されている

**このタスクは Task 5 か Task 6 を実行したときだけ行う。**

- [ ] **Step 1: doc へ射程を書いた群へ、Task 4 と同じ変異を当て直す**

Expected: **依然として通る**（射程を書いただけで挙動は変えていないため）。**ここで落ちたら、doc を書く
ついでに挙動を変えてしまっている** — `AGENTS.md`「レビュー指摘へ修正を当てた」の同型（修正が周辺に
新しい誤りを生む）。

- [ ] **Step 2: 機構へ倒した群へ、Task 4 と同じ変異を当て直す**

Expected: **落ちる**。落ちなければ機構が効いていない（`.claude/rules/safety-nets.md`「効いていることは、
フォールトインジェクションで一度は実測する」）。

- [ ] **Step 3: 変異を戻し、作業ツリーが clean であることを確認する**

```bash
git status --short
```

- [ ] **Step 4: 結果を設計書 §10 へ追記し、コミット**

```bash
npm run governance:check
```

コミットメッセージ:

```
docs: 倒した後の挙動をフォールトインジェクションで実測する（#1008）
```

---

### Task 8: 設計書の仕上げと PR

**Files:**
- Modify: `docs/superpowers/specs/2026-08-09-detector-scope-audit-design.md`

**Interfaces:**
- Consumes: Task 1〜7 のすべての記録
- Produces: 受け入れ条件（設計書 §6）を満たす設計書と、PR

- [ ] **Step 1: 設計書 §6 の受け入れ 3 条件を 1 つずつ確かめる**

1. 母集団が数え上げられ、件数が記録されている（0 件でも成果）
2. 各件が「射程を書く」「機構へ倒す」のどちらかに倒れている
3. 倒した後で変異を当て、実際に落ちる/落ちないことを確かめている

**満たせていない条件があれば、該当タスクへ戻る。**

- [ ] **Step 2: 設計書に残った「実装で追記する」の記述を実際の結果へ置き換える**

§5 の「仕分け結果（実装で追記する）」が残っていないか確認する。

- [ ] **Step 3: 検証**

```bash
npm run governance:check
cargo test --workspace
git status --short
```

- [ ] **Step 4: push して PR を作る**

**`cd` を鎖に含めない**（pre-bash hook が対象リポジトリを判定できず拒否する）。未 push の状態で
`gh pr create` を打つと空 PR として拒否されるため、`&&` で繋ぐ:

```bash
git push -u origin HEAD && gh pr create --title "..." --body-file <path>
```

PR 本文には**母集団の件数と ③ の件数**を書く。`Closes #1008` を含める。

---

## 自己レビュー結果

**Spec coverage:** 設計書 §3（母集団 3 層）→ Task 1・2。§4 Phase 2（SSOT 分類）→ Task 3。§4 Phase 3
（定型変異）→ Task 4。§4 Phase 4（倒す）→ Task 5・6。§6 の受け入れ 3 条件 → Task 8 Step 1 で明示的に検算。
§7 の留保（分類が一意に決まらない件）→ Task 3 Step 2 に処理を明記。

**分岐の扱い:** ③ が 0 件のとき Task 5〜7 をスキップする経路を Task 4 Step 6 に明記した。derive 導入を
別 issue へ切る経路を Task 6 Step 1 に明記した。**どちらも「やらずに閉じる」ではなく「そう記録して閉じる」**
として書いてある。
