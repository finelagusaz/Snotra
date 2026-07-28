# research — issue #713: workspace lints の opt-in 漏れを機構で塞ぐ

## issue の要約

ルート `Cargo.toml` の `[workspace.lints.rustdoc]` は intra-doc link 検出器を `deny` 化しているが、各 crate 側の `[lints] workspace = true` は **opt-in** である。opt-in を忘れた crate では検出器が黙って無効になり、「CI 緑 = link 切れなし」がその crate に限って成り立たない。#706（`snotra-egui-runtime` が #627 から #700 の検証中まで素通り）が実例。`governance:check` に「全 workspace member が `[lints] workspace = true` を持つ」検査を足して再発を止める。

**ユーザー裁定（2026-07-28）**: 機構化する。粒度は「全 member」。

## 現状の事実

### 母集団と opt-in の実態

`Cargo.toml`（ルート）:

- `members` = `snotra-core` / `snotra-egui-runtime` / `src-tauri` / `snotra-settings` の 4 件（すべて文字列リテラル。glob エントリは無い）
- `[workspace.lints.rustdoc]` に `broken_intra_doc_links = "deny"` / `invalid_html_tags = "deny"`

**4 member すべてが現時点で `[lints] workspace = true` を持つ**（`snotra-core/Cargo.toml:6-7`, `snotra-egui-runtime/Cargo.toml:9-10`, `src-tauri/Cargo.toml:6-7`, `snotra-settings/Cargo.toml:6-7`）。ゆえに新検査は**今在る赤を直すものではなく、次に crate を足したときの再発だけを止める純粋な回帰カナリア**である。

### CI 側に member の写しは無い

`.github/workflows/ci.yml:99-100` = `cargo doc --workspace --no-deps --document-private-items`。`--workspace` ゆえ member 列挙は cargo が `Cargo.toml` から読む。**doc job 側に第二の沈黙母集団は存在せず、opt-in の有無が唯一の担保**である。`docs/build-commands.md:20,26,180` も同じコマンドを SSOT として記述済み。

### 既に members を導出している箇所（写しを増やさないための接続点）

| 箇所 | 用途 | 形 |
|---|---|---|
| `scripts/governance-check.mjs:268-272` | G-build-commands が `cargo test -p <crate>` の実在を見るため、member ディレクトリ → `[package] name` を導出 | `members\s*=\s*\[([^\]]*)\]` を全文から取り、`"..."` を拾う。読めない member は `if (name)` で**黙って落とす** |
| `scripts/governance-check.test.mjs:82-113` | #701 の G-module-index/G-references 母集団カナリア | `[workspace]` セクションを切ってから `^members` を取り、非マッチを `expect(...).not.toBeNull()` で **fail-closed** にする |
| `.claude/hooks/post-edit.test.mjs:609-628` | #500 の members ドリフト検出カナリア | 上と同型（`post-edit.mjs` 本体は `--workspace` ゆえ写しを持たない） |

→ 新検査は `governance-check.mjs` 内に **member 導出の共有ヘルパ**を置き、G-build-commands の inline 導出をそれに載せ替えるのが自然（issue が言う「写しを増やさない」の実装形）。

### 検査の登録・列挙面

- `buildChecks`（`scripts/governance-check.mjs:1358-1376`）が**検査 ID の SSOT**。件数はこの配列から計算されるので、件数を手で書く面は存在しない
- ID は `G-<name>` 形（連番を振らない・`.claude/rules/governance-docs.md`）
- `runAll` の `evidence` 文字列（`:1391`）に母集団件数を足す慣習がある
- `scripts/governance-check.test.mjs:942` に「検査 ID の形」テストが在る（`G-` 形の固定）
- `.claude/skills/health-check/references/mechanized-checks.md` は**旧 Check → G の移行記録**であり、旧 Check を持たない新設検査は載せない

### テストの形

`scripts/governance-check.test.mjs` は `snap(contents, extraFiles)`（`:49-52`）で最小スナップショットを注入する純関数テスト。各 describe が「赤（フォールトインジェクション）」「緑」「判定対象外の不混入」を持つ。**実ファイルを変異させる形は存在しない**（`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない」に適合）。

## 実測（判定の中核は自分で測る）

スクラッチ workspace（member 1 件・意図的に切れた intra-doc link `[NoSuchSymbol]`）で `cargo doc --workspace --no-deps --document-private-items` の exit code を測った:

| 条件 | 結果 |
|---|---|
| A: member が `[lints] workspace = true`・root に `[workspace.lints]` **無し** | `cargo metadata` が即エラー（``error inheriting `lints` ... `workspace.lints` was not defined``）＝ **compile-fail であり沈黙しない** |
| B: member に `[lints]` **無し**（opt-in 漏れ・lib crate） | warning 1 件を出して **exit 0**（沈黙） |
| C: member が `[lints.rustdoc]` **だけ**を持つ（`invalid_html_tags = "deny"`） | warning のまま **exit 0**。`[lints.rustdoc]` は workspace テーブルを**継承しない** |
| D: **bin-only** crate（`src/main.rs` のみ）+ opt-in あり | **exit 101**（`error: could not document`）＝ deny が効く |
| B′: bin-only crate + opt-in 漏れ | **exit 0**（沈黙） |
| E: `[package]` 配下に `lints.workspace = true`（dotted key） | **exit 0**（`package.lints` になるだけで cargo は警告も出さず無視する＝**opt-in ではない**） |
| F: ルート直下（最初の `[` より前）に `lints.workspace = true` | **exit 101**＝ `[lints]` テーブル形と等価な**正当な opt-in** |

この 5 件から確定する設計上の帰結:

1. **root テーブルの消失は検査対象外でよい**（A・compile-fail ゆえ沈黙経路ではない。`/retrospective` SKILL.md「カナリアで守るのは沈黙する経路だけでよい」に照らして対象外）
2. **`[lints.rustdoc]` の存在を opt-in と見なしてはならない**（C・継承しないので黙って無効のまま）
3. **粒度「全 member」は測定で裏付く**（D/B′・bin-only crate でも deny は効き、opt-in 漏れは沈黙する。`src/lib.rs` の有無で分岐させる理由が無い）
4. **判定は「どのセクションに現れたか」で定義する**（E/F・同じ `lints.workspace = true` が、ルート直下なら opt-in、`[package]` 配下なら**何もしないゴミ**。字面一致では両方を取り違える。`docs/development-principles.md` §6「判定単位は文字列が現れたかではなく、どの構文的位置に現れたか」の実例）

## 述語の罠（naive scan が壊れる形）

`workspace = true` という文字列は member の `Cargo.toml` に**大量に現れる**:

- `version.workspace = true`（`[package]` 配下・4 member すべて）
- `egui.workspace = true` / `tauri.workspace = true` / `tauri-runtime.workspace = true`（`[dependencies]` 配下）

また `[lints]` という文字列はルート `Cargo.toml` の `[workspace.lints.rustdoc]` にも現れる。ゆえに判定は**セクション境界を持つ形**で書く: `^\[lints\]$` の行を見つけ、次の `[` 見出しまでの範囲に `^workspace\s*=\s*true` があるかを見る。`[lints.rustdoc]` は `^\[lints\]$` に一致しないので自然に「非 opt-in」へ落ちる（実測 C と一致）。

## 引用の腐り（issue 本文の前提の訂正）

issue は根拠として `.claude/rules/safety-nets.md` の「カナリアで守るのは沈黙する経路だけでよい」を引くが、**その文は safety-nets.md（全 37 行）に存在しない**。実在するのは `.claude/skills/retrospective/SKILL.md:61`（「機構が吸収できるか（最上段）」）である。主張そのものは生きており、上の実測 A/B/C/D と合わせて「機構化する」判断を支える。**この訂正を採用する**（引用先を `/retrospective` として扱う）。

## 技術的制約

- `governance-check.mjs` の契約: 依存ゼロ（Node 標準のみ）・決定的・スナップショット注入の純関数・**空母集団は明示 fail**（`:11-17`）。TOML パーサは使えないので正規表現でセクションを切る
- `makeSnapshot` は全ファイルを歩き `read(rel)` で任意パスを読めるため、member の `Cargo.toml` はそのまま読める（`REF_EXTENSIONS` は G-references 専用のフィルタであり、`read` には掛からない）
- セーフティネットの変更ゆえ `.claude/rules/safety-nets.md` が自動配送される（`paths` に `scripts/governance-check.mjs` が入っている）。フォールトインジェクションはフィクスチャに対して行う

## 未解決の疑問

- `members` に glob エントリ（`crates/*`）が入る将来形はこの正規表現で展開できない。**現時点では 0 件**なので、glob を含む要素を見つけたら「母集団の欠落」として fail させる（fail-closed）方針で塞ぐ — 展開器は YAGNI
