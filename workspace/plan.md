# plan — #863 `docs/hooks.md` の発火一覧を機構で照合する

## 目的

`docs/hooks.md`「PostToolUse（post-edit.mjs）の発火一覧」が `selectChecks` から黙って腐る経路を塞ぐ。

**issue は案 A / 案 B の二択で書かれているが、成果物は A′ と B の両方である。** どちらの機構を作っても覆えない足が残り、それは「なぜ機構化しないか」ではなく「**なぜこの足だけ機構の外に置くか**」として ADR に要る。残余を粗い粒度（「内容整合は機構の外」）で書くことは、この行が一度腐った原因そのものなので、**足を名指しする**。

## 受け入れ条件

1. `selectChecks` へ id を足す／消す／改名する変更を `docs/hooks.md` へ反映し忘れると `npm run governance:check` が赤くなる
2. `selectChecks` の述語（`snotra-core/` prefix 等）を変えて表の行が偽になると赤くなる
3. `fmt` → `clippy` の**順序**を入れ替えて表が追随しないと赤くなる
4. 表のどの行も言及しない id を `selectChecks` が発行しうるようになると赤くなる
5. 上記 4 つを **fake snapshot への変異**（ライブのガードは弱めない）で 1 件ずつ実測し、赤くなることを固定する
6. `docs/adr/ADR-rustfmt-gate.md`「受容する残余」の「照合する機構が無い」が偽のまま残らない

## 設計判断（確定済み・実測に基づく）

### 採る形: `selectChecks` を**注入して代表パスを食わせる**

`G-hook-commands` のソーステキスト抽出ではなく、export 済みの `selectChecks` を import して呼ぶ。

- **理由**: `G-hook-commands` が抽出を採ったのは `cargoSpec` が**非 export** だからであって、import が危険だからではない（`post-edit.mjs` は I13 のガードで import 安全・`pre-bash.test.mjs` が既に import している）。抽出は判定を再実装することになり、閉じたい写しを一段下で作り直す
- **注入の形**: `checkHookFires(snapshot, select = selectChecks)`。既定値付き引数は `checkModuleIndex(snapshot, crates = ...)`・`checkConfigFieldReachability(snapshot, table = ..., expectedStructs = ...)` と同形で、fake を渡した単体テストが書ける
- **書き残すこと**: 上の区別（非 export ゆえの抽出 vs export ゆえの注入）を `checkHookFires` のコメントに置く。書かないと将来の読者が「一貫性の修正」として逆方向へ倒す

### 表の書式: 3 列（代表パス / 検査 id / 補足）

散文を**列で分離**する。「列 2 のバッククォートは検査 id だけ」という 1 規約だけで抽出が確定し、補足列の散文は無制約のまま残る。

- 列 1「編集したファイル（代表パス）」: ツリー相対の**実在する具体パス** 1 件（glob ではない）
- 列 2「走る検査 id」: バッククォート内は検査 id だけ。空集合は `（なし）`（バッククォートを書かない）
- 列 3「補足」: 散文。バッククォート自由

**存在検査は書かない。** `docs/hooks.md` は `governanceDocs` の母集団に入り、G-references が「バッククォート内で `/` を含み `REF_EXTENSIONS` に当たる文字列」の実在を既に要求する（実測）。自前で持つと二重になる。

### 照合の 2 方向

| 方向 | 内容 |
|---|---|
| 行ごと | `select(代表パス)` と列 2 の id 列が**順序込みで**一致する（`fmt` → `clippy` の順は表自身の主張であるため配列比較にする） |
| 母集団 | `post-edit.mjs` のソースから `checks.push("<id>")` のリテラルを抽出し、その全 id が表のどこかに現れる |

逆向き（表にあって発行されない id）は行ごとの一致から導かれるので別に見ない。抽出 0 件・表が見つからない・列が足りないはすべて明示的な finding（母集団の欠落）。

母集団を `Object.keys(BUDGETS)` から取らない: `BUDGETS ⊇ 発行されうる id` であり、どのパスも発火させない id が `BUDGETS` に入ったとき、表に「決して走らない検査」を書かせることになる。

### 表の新しい内容（`selectChecks` を実行して測った値）

| 代表パス | 検査 id |
|---|---|
| `snotra-core/src/lib.rs` | `fmt` `clippy` `core-test` |
| `snotra-egui-runtime/src/lib.rs` | `fmt` `clippy` `egui-runtime-test` |
| `snotra-settings/src/main.rs` | `fmt` `clippy` `settings-test` |
| `src-tauri/src/main.rs` | `fmt` `clippy` `tauri-test` |
| `src-tauri/tauri.conf.json` | `config-warn` |
| `src-tauri/Cargo.toml` | `cargo-check` |
| `Cargo.toml` | `cargo-check` `hook-selftest` |
| `.claude/settings.json` | `hook-selftest` |
| `.githooks/pre-commit` | `githooks-selftest` |
| `docs/hooks.md` | （なし） |

10 行で 10 個の id をすべて覆う。**ルート `Cargo.toml` が両方を発火する**ことは現在の表からは読めない（2 行に分かれている）——機構を入れると表現せざるを得なくなる、実測で見つかった既存の不正確さである。

### 覆わない足（受容する残余・ADR へ名指しで書く）

1. **代表パスが実在を要求されるため、実在しない入力を行にできない**——4 crate の外に置いた `.rs`（現在 0 件）と `config.toml`（ランタイムのユーザー領域ファイル）。これらは補足列の散文だけが記述する
2. **補足列の散文の意味整合**（「この順は報告の並びであって実行順の打ち切りではない」が事実か）は機構の外
3. **代表パスは標本であって全域の等価性証明ではない**——ただし `selectChecks` の述語ごとの挙動は `post-edit.test.mjs` が既に 1 件ずつ固定している（`vitest.config.ts` / `package.json` / `.githooks/**` / `.claude/settings.json` / 負例）。**この検査の担当は「文書が腐らないこと」であり、述語の正しさではない**

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 |
|---|---|
| `scripts/governance-check.mjs` | `checkHookFires(snapshot, select = selectChecks)` を新設。`selectChecks` の import。`buildChecks` の registry へ `G-hook-fires` を 1 行追加（`G-hook-commands` の直後） |
| `scripts/governance-check.test.mjs` | `describe("G-hook-fires ...")` を追加。fake snapshot への変異で受け入れ条件 1〜4 を 1 件ずつ赤にする |
| `docs/hooks.md` | 「PostToolUse（post-edit.mjs）の発火一覧」節を 3 列表へ書き換え、書式規約 1 段落と残余 1 行を置く |
| `docs/adr/ADR-rustfmt-gate.md` | 「受容する残余」の当該行を、機構が入った事実と #863 への参照へ差し替える |
| `docs/adr/ADR-hook-fires-table-check.md`（新規） | 却下した代替（案 B 全面受容 / ソース抽出 / glob 列の機械照合 / 存在検査の自前実装 / `BUDGETS` を母集団にする案）と、覆わない足 3 件 |
| `.claude/hooks/post-edit.mjs` | `selectChecks` の doc コメントへ「発火一覧は G-hook-fires が `docs/hooks.md` と照合する」の 1 行（変更者がまずここを読むため） |

## 実装順序（Red → Green を機構で実演する）

**Phase 1 で検査を入れた時点で、現行の表に対して `governance:check` は赤くなる**（旧表はヘッダが一致せず 3 列も持たない）。Phase 2 で緑へ戻る。これは fake への変異ではなく**現に腐りうる状態の実データ**に対する検出であり、機構が効いていることの一次証拠になる。

### Phase 1 — 検査とテスト

- [ ] `scripts/governance-check.mjs` に `checkHookFires` を実装（import・注入・2 方向・母集団欠落ガード・CRLF 耐性 `\r?\n`）
- [ ] registry へ `{ id: "G-hook-fires", run: () => checkHookFires(snapshot) }` を追加
- [ ] `npm run governance:check` を実行し、**現行の表に対して赤くなる**ことを確認（出力を記録）
- [ ] `scripts/governance-check.test.mjs` に FI テストを追加（下の「テスト方針」の 7 本）
- [ ] `npx vitest run scripts/governance-check.test.mjs` が緑

### Phase 2 — 表の書き換え

- [ ] `docs/hooks.md`「PostToolUse（post-edit.mjs）の発火一覧」を 3 列 10 行へ書き換え、書式規約と残余を置く
- [ ] `npm run governance:check` が緑（G-references が代表パスの実在を通すことも同時に確認）

### Phase 3 — 文書の整合

- [ ] `.claude/hooks/post-edit.mjs` の `selectChecks` コメントへ 1 行
- [ ] `docs/adr/ADR-rustfmt-gate.md`「受容する残余」の当該行を差し替え
- [ ] `docs/adr/ADR-hook-fires-table-check.md` を新設
- [ ] `npm test` と `npm run governance:check` が緑

## 不変条件と異常系

- **fail-closed**: 表が見つからない / 列が足りない / `checks.push` の抽出が 0 件 / `post-edit.mjs` が読めない / `docs/hooks.md` が読めない は、いずれも**沈黙せず finding を出す**。既存の全検査と同じ規律（`G-hook-commands` の「母集団の欠落」文言に倣う）
- **ライブのガードを弱めない**: FI は fake snapshot と fake `select` に対して行う。`post-edit.mjs` にも `governance-check.mjs` にも一時的な変異を入れない
- **CRLF 耐性**: 行分割は `/\r?\n/`。`split("\n")` は Windows CI で列末に `\r` を残す（#587/#589 で二度踏んでいる）
- **`G-<name>` 命名**: 連番を振らない（`.claude/rules/governance-docs.md`）
- 検査数のハードコードは無い（`runAll` の evidence は `checks.length` から動的に組む・実測）ので、追随して直す文書は無い

## テスト方針と検証コマンド

`scripts/governance-check.test.mjs` に fake snapshot（既存ヘルパに倣う）と fake `select` を渡す 7 本:

| # | 変異 | 期待 |
|---|---|---|
| 1 | 正しい表 + 正しい `select` | findings 0 件 |
| 2 | `select` が id を 1 つ追加（doc 未更新）| 赤（受け入れ条件 1） |
| 3 | `select` が id を 1 つ削除 | 赤（受け入れ条件 1） |
| 4 | 表の id を改名 | 赤（受け入れ条件 1） |
| 5 | `select` の述語を変え、ある行だけ別の id を返す | 赤（受け入れ条件 2） |
| 6 | `fmt` / `clippy` の順序を表側で入れ替え | 赤（受け入れ条件 3） |
| 7 | ソースに新 id の `checks.push` があるが表のどの行にも無い | 赤（受け入れ条件 4） |

加えて母集団欠落 3 本（表が無い / `post-edit.mjs` が読めない / `checks.push` が 0 件）。

検証コマンド（`docs/build-commands.md` カテゴリ F と hook 自己検査）:

- `npm run governance:check`（**パイプへ繋がない**——`docs/development-principles.md`「構造的設計原則と強制の階梯」）
- `npm test`
- `npx vitest run scripts/governance-check.test.mjs .claude/hooks`

Rust コードは触らないのでカテゴリ A〜E は該当なし。

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**。製品の挙動を変えない（開発ツールの検査追加）
- `AGENTS.md`「条件別チェック」: **不要**。既存行「ガバナンス文書を変更 → `npm run governance:check`」「セーフティネットを新設/変更 → `.claude/rules/safety-nets.md`」が既に覆う。新しいトリガーは生じない
- `docs/build-commands.md`: **不要**。カテゴリ F のコマンドは変わらない
- `.claude/skills/health-check/references/mechanized-checks.md`: **不要**。「旧 Check N → G-name」の履歴表であり、health-check に前身を持たない新設検査は行を持たない

## 未確定（実装前に潰す）

（なし）

## セルフレビュー

- リスク: **通常**
- plan-review: 未実施（通常リスク）。`/plan-review`「リスク判定」の高リスク条件（永続形式・並行性・網羅性が要件・ガバナンス文書の移動/圧縮/分割）に当たらない——本件は文書の**書式変更と検査 1 本の新設**であり、拘束力のある機構は fake への変異で全数実測できる
- エージェント数: 0
- 自己レビュー（`/start-issue`「5a. check スキルによる計画検証」の 5 項目）:
  1. **issue の全要件に作業項目が対応する** — 案 A（機構）は Phase 1・2、案 B（ADR へ判断を残す）は Phase 3。issue が二択としたものを両方の成果物として扱う理由を「目的」に明記した
  2. **境界条件と検証** — 表が無い / 列不足 / 抽出 0 件 / 読めない の 4 つを「不変条件と異常系」に列挙し、母集団欠落 3 本のテストで覆う
  3. **新しい状態・リソース・プロセス** — 無い（純関数の検査 1 本。プロセスも永続状態も増えない）
  4. **より単純な既存パターンで置き換えられないか** — `G-hook-commands` へ相乗りする案（issue の案 A 原形）は、あちらが `docs/build-commands.md` のコードブロックに特化しており、表の列抽出とは別の形になる。`checkCiTable` の列抽出と `checkSkillTable` の集合比較を組み合わせる形が最小
  5. **壊してはならない不変条件に検知手段がある** — 「沈黙は合格」の契約（`docs/hooks.md`）は本変更で触らない。表の腐敗は本検査が、代表パスの実在は G-references が、`selectChecks` の述語は `post-edit.test.mjs` が持つ
- 条件別チェック（`AGENTS.md`）の該当:
  - 「セーフティネットを新設/変更」→ `.claude/rules/safety-nets.md`（`scripts/*.mjs` を触るので自動配送）。「FI で一度は実測する」「複製に変異を当てる」「検出器のカバー範囲を欠落のパターンごとに検算する」の 3 項を、テスト方針の 7 本 +「覆わない足」3 件で満たす
  - 「ガバナンス文書を変更」→ `npm run governance:check`
  - 「関数・型を新規定義」→ 呼び出し元は registry 1 箇所（同一コミットで束ねる）
  - `/symmetric-check`・`/race-check`・`/persistence-check`・`/cache-check`・`/state-check`・`/dry-check`: **該当なし**（対称ペアなし・並行性なし・永続形式なし・キャッシュなし・UI モードなし・重複排除なし）
- 要対処: 0 件
- 未検証: **なし**（受け入れ条件 1〜4 は fake への変異で単体テストとして測れる。CI 側の実測を要する項目は無い）

## 人間レビュー

- [x] 承認済み — 2026-08-01 / 問い: "この計画を承認しますか" / 回答: "承認する"
- [x] 表の形 — 2026-08-01 / 問い: "表の形をどうしますか（覆える足の広さと読みやすさのトレードオフです）" / 回答: "3 列・代表パス（推奨）"
