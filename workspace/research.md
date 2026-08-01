# research — #863 `docs/hooks.md` の発火一覧に照合機構が無い

## issue の要約

`docs/hooks.md`「PostToolUse（post-edit.mjs）の発火一覧」の表は `selectChecks` の写しでありながら、内容を照合する機構が無い。同ファイル自身が「正本は `selectChecks` である」と自己申告しているが、索引が正本とずれても `npm run governance:check` は緑のままである。

同型のドリフトはルート `CLAUDE.md` で一度起きており（#474〜#497）、その退去先が `docs/hooks.md` である。#858 で `selectChecks` へ `fmt` を足したとき、この行は実際に計画の変更ファイル一覧から漏れていた（独立レビューが発見）。

issue は **案 A（`G-hook-commands` を広げる）** と **案 B（受容を ADR へ）** の二択として書かれている。

## 関連ファイル・シンボル（すべて grep で実在確認済み）

| パス | 対象 |
|---|---|
| `docs/hooks.md` | `:40-53`「PostToolUse（post-edit.mjs）の発火一覧」節。`:42` の自己申告、`:44-51` の 2 列表 |
| `.claude/hooks/post-edit.mjs` | `selectChecks`（`:122-153`・**export 済み**）、`BUDGETS`（`:35-48`）、`CARGO_MANIFEST`（`:61`）、`CHECK_DEFINITION`（`:70-75`）、I13 のガード（`:末尾`） |
| `scripts/governance-check.mjs` | `checkHookCommands`（`:614`・G-hook-commands）、`checkSkillTable`（`:580`・双方向集合比較の手本）、`checkCiTable`（`:428`・表の列抽出の手本）、`checkReferences`（`:152`・G-references）、`governanceDocs`（`:1051`）、registry（`:1512-1531`） |
| `scripts/governance-check.test.mjs` | 既存の fake snapshot による検査テスト（`:454` に G-skill-table の例） |
| `docs/adr/ADR-rustfmt-gate.md` | `:60`「受容する残余」に本件が事実として記載されている |
| `.claude/rules/safety-nets.md` | 「検出器のカバー範囲は、欠落のパターンごとに検算する」（#858 で新設・本件の判断にそのまま効く） |

## 実測（一次証拠）

### 1. `selectChecks` は import 安全である

`post-edit.mjs` 末尾に I13 のガード（`invokedDirectly` = `import.meta.url === pathToFileURL(process.argv[1]).href`）があり、import しただけでは `main()` は走らない。`pre-bash.test.mjs` が既に `buildCommand` / `BUDGETS` を import している。

**`G-hook-commands` がソーステキスト抽出を採ったのは `cargoSpec` が非 export だからであって、import が危険だからではない。**（`:604-611` のコメントは「非 export・import は main 実行の副作用があるため」と両方を挙げているが、後者は I13 導入後は成り立たない。）→ 本件で import を採るなら、この区別を書き残さないと将来の読者が逆方向へ「一貫性の修正」をしうる。

### 2. 代表パスに対する `selectChecks` の返り値（実行して測った）

```
"snotra-core/src/lib.rs"               ["fmt","clippy","core-test"]
"snotra-egui-runtime/src/lib.rs"       ["fmt","clippy","egui-runtime-test"]
"snotra-settings/src/main.rs"          ["fmt","clippy","settings-test"]
"src-tauri/src/main.rs"                ["fmt","clippy","tauri-test"]
"scripts/x.rs"                         ["fmt","clippy"]
"Cargo.toml"                           ["cargo-check","hook-selftest"]
"src-tauri/Cargo.toml"                 ["cargo-check"]
"src-tauri/tauri.conf.json"            ["config-warn"]
".claude/settings.json"                ["hook-selftest"]
".claude/hooks/post-edit.mjs"          ["hook-selftest"]
"package.json"                         ["hook-selftest"]
"vitest.config.ts"                     ["hook-selftest"]
".githooks/pre-commit"                 ["githooks-selftest"]
"docs/hooks.md"                        []
```

**現在の表が持つ不正確さが 1 件見つかった**: ルート `Cargo.toml` は `cargo-check` と `hook-selftest` の**両方**を発火するが、表は 2 行に分かれており「両方走る」とは読めない。機構を入れると表現せざるを得ない。

### 3. 代表パスの実在（`git ls-files` で確認）

`snotra-core/src/lib.rs` / `snotra-egui-runtime/src/lib.rs` / `snotra-settings/src/main.rs` / `src-tauri/src/main.rs` / `Cargo.toml` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` / `.claude/settings.json` / `.claude/hooks/post-edit.mjs` / `package.json` / `vitest.config.ts` / `.githooks/pre-commit` / `docs/hooks.md` はすべて実在する。

**crate 外の `.rs` は 1 件も存在しない**（`git ls-files '*.rs'` から 4 crate を除くと 0 件）。

### 4. G-references が代表パスの実在を無料で保証する

`governanceDocs`（`:1051`）は `docs/` 配下の `.md` を含むので `docs/hooks.md` は G-references の母集団に入る。G-references（`:186-196`）は**バッククォート内で `/` を含み `REF_EXTENSIONS`（`.md|rs|ts|tsx|mjs|json|toml|yml|ps1|html|css`）に当たる文字列**の実在を要求する。

→ 表の代表パス列に書いた `.rs` / `.toml` / `.json` パスは、**新しい検査が存在検査を持たなくても G-references が実在を強制する**。自前で書くと二重になる。
→ 逆に、**実在しない入力（crate 外の `.rs`・`config.toml`）は代表パスとして表に書けない**。これは覆えない足として残る。

### 5. 検査数のハードコードは無い

`grep -rn "18 検査\|検査数"` は 0 件。`runAll` の evidence は `checks.length` から動的に組む（`:1546`）。→ 検査を 1 本足しても文書の数字を直す必要は無い。

`.claude/skills/health-check/references/mechanized-checks.md` は「旧 Check N → G-name」の**履歴表**であり、health-check に前身を持たない新設検査は行を持たない。

## 再利用できる既存パターン

- **双方向の集合比較**: `checkSkillTable`（`:580`）が「表の backtick トークン ↔ コード由来の集合」を双方向で見る。射程を規範ではなく判定で固定する設計。
- **表の列抽出**: `checkCiTable`（`:442-444`）が `lines[i].split("|").map(c => c.trim())` で列を取る。ヘッダ行を `findIndex` で見つけ、`+2` から `|` 始まりの間を走る。
- **依存の注入**: `checkModuleIndex(snapshot, crates = ...)` / `checkConfigFieldReachability(snapshot, table = ..., expectedStructs = ...)` が既定値付き引数で注入する形を持つ。fake での単体テストが書ける。
- **母集団欠落の明示的な赤**: 全検査が「抽出 0 件 → finding」を持つ（`:623` / `:524` / `:588` 等）。
- **CRLF 耐性**: 行分割は `/\r?\n/`（`:633` のコメントに #587/#589 で二度踏んだ記録）。

## 技術的制約

- `docs/adr/ADR-rustfmt-gate.md:60`「受容する残余」は「**内容を照合する機構が無い**」と断言している。機構が入ればこの文は**偽になる**——同じ族の腐った写しなので、同じ変更で直す必要がある。
- `.claude/rules/governance-docs.md`: 検査名は `G-<name>`（連番を振らない）。他文書を指すときは正準形 `` `<対象>`「<見出し>」 ``。
- `.claude/rules/safety-nets.md` が `scripts/*.mjs` で自動配送される。「効いていることはフォールトインジェクションで一度は実測する」「複製に変異を当てる（ライブのガードを弱めない）」「検出器のカバー範囲は欠落のパターンごとに検算する」の 3 項が本件に効く。
- `AREA_BUDGET` は `docs/` を対象外とするので、`docs/hooks.md` を厚くしても ratchet に当たらない。

## 未解決の疑問（→ plan.md「未確定」で潰す）

なし。設計判断は plan.md 側で確定させる。
