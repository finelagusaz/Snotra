# 独立導出 — issue #713（workspace lints の opt-in 漏れを `governance:check` で塞ぐ）

issue #713 とリポジトリのコードのみを入力に、必要な変更集合を独立に導出した。cargo の挙動はすべて**この場で自分で実測**した（`cargo 1.94.0 (85eff7c80 2026-01-15)`・Windows）。

---

## 0. 独立性の汚染についての開示（先に読むこと）

**`workspace/plan.md` と `workspace/research.md` は開いていないが、リポジトリ全体 grep の出力に両ファイルの行が混入した。** 混入した経路と内容:

| 実行したコマンド | 混入した行 |
|---|---|
| `grep -rn "workspace\s*=\s*true" -I .` | `workspace/plan.md:23,24,25,26,33,43,44,48` / `workspace/research.md:5,18,52,53,54,58,63,74` |
| `grep -rn "\[lints" -I .` | `workspace/plan.md:23,33,43,45,46,47,76` |
| `grep -rn "workspace.lints" -I .` | `workspace/plan.md:1,9,10,11,31,34,35,50,59,76,84` / `workspace/research.md:1,5,16,52,53,54,58,74` |

**そこから見えてしまった具体物**（以下は独立導出の成果として数えてはならない）:

- 検査 ID `G-workspace-lints`
- 関数名 `workspaceMembers` / `hasWorkspaceLintsOptIn` / `checkWorkspaceLints`
- 触るファイルの一覧（`governance-check.mjs` / `governance-check.test.mjs` / `docs/build-commands.md`）
- 赤フィクスチャの 4 形（`[lints]` 無し・`[lints.rustdoc]` のみ・`workspace = false`・`[package]` 配下 dotted）
- `research.md` の実験ラベル A〜F とその結論の要約行
- **判定述語の骨格そのもの**（`research.md:74`）: 「`^\[lints\]$` の行を見つけ、次の `[` 見出しまでの範囲に `^workspace\s*=\s*true` があるかを見る」。**§5-a のステップ 3 はこの形と一致する。ゆえに §5-a の骨格は独立収束の証拠として数えてはならない**

**取った対処**: 漏れた実験表を一次証拠として使わず、**cargo の挙動を A〜S の 19 ケースでゼロから測り直した**（§1）。測り直しは無駄ではなく、**漏れた表に無い沈黙経路を 3 件（M・N・R）新たに発見した**（§2）。これが本レビューの主たる独立の寄与である。

**したがって: 検査 ID・関数名・ファイル一覧・§5-a の述語骨格が計画と一致していても、それは独立収束の証拠ではない。** 一致を「2 人が別々に同じ結論へ達した」と読まないこと。

**汚染されていない（＝独立の指摘として扱ってよい）部分**——いずれも grep 出力に対応物が現れなかった:

- **§2**（ルート側の沈黙経路 M・N・R とクラス 2 の提案）
- **§4**（同名・別概念の実 grep 分類。とくに「`version.workspace = true` により字面述語は全 member で常に真になる」）
- **§5-b**（`["lints"]` / `[ lints ]` / `[lints]` 重複 / glob members / CRLF の誤判定分析）と **§5-c**
- **§6-c / §6-d**（既存カナリアとの重複回避・自動追随するので書かないテスト）
- **§7**（間接参照の洗い出し。とくに `governance-check.mjs:269` の未スコープ欠陥と `post-edit.test.mjs` カナリアのメッセージ）
- **§8 の「要らない」表**（検証済みの否定）
- **§1 の実測表そのもの**（漏れた要約行と結論が一致するケースもあるが、値は自分で測り直したものであり、M・N・R・I・J・L・P・Q・R は漏れた行に無い）

---

## 1. cargo の実測（一次証拠）

スクラッチワークスペースを作り、`//! Broken: [\`NoSuchSymbolHere\`]` を持つ member に対して `cargo doc --workspace --no-deps --document-private-items` の exit code を測った（CI の `ci.yml:99-100` と同じコマンド）。

### 1-a. member 側の形

| # | member `Cargo.toml` の形 | exit | 判定 |
|---|---|---|---|
| A | `[lints]` + `workspace = true` | **101** | 正当な opt-in（deny 有効） |
| B | `[lints]` セクションが無い | **0** | **沈黙**（`warning: unresolved link` のみ）＝ #706 の再現形 |
| C | `[lints.rustdoc]` だけを持つ（自前の deny 1 件） | **0** | **沈黙**。`[lints.rustdoc]` は workspace テーブルを**継承しない** |
| D | `[lints]` + `workspace = false` | 101 | cargo が manifest エラー（``error: `workspace` cannot be false``）＝**沈黙しない** |
| E | `[package]` 配下に `lints.workspace = true` | **0** | **沈黙**（``warning: unused manifest key: package.lints``） |
| F | 最初の `[` より前にルート直下 `lints.workspace = true` | **101** | **正当な opt-in**（A と等価） |
| G | `[lints]` にコメント・余分な空白・行末コメント | 101 | 正当な opt-in |
| H | `[lints] workspace = true` と `[lints.rustdoc]` を併記 | 101 | cargo が manifest エラー＝沈黙しない |
| I | `["lints"]`（クォートしたテーブル見出し）+ `workspace = true` | **101** | **正当な opt-in**（TOML としては `[lints]` と同一） |

### 1-b. ルート側の形（member は A の形で固定）

| # | ルート `Cargo.toml` の形 | exit | 判定 |
|---|---|---|---|
| K | `[workspace.lints]` が**無い** | — | ``error: failed to load manifest for workspace member``＝**沈黙しない** |
| M | `[workspace.lints.clippy]` だけ在る（rustdoc の 2 行を削除） | **0** | **沈黙** |
| N | rustdoc の 2 行が `"deny"` → `"warn"` へ降格 | **0** | **沈黙** |
| R | `[workspace.lints]` が**空テーブル** | **0** | **沈黙** |
| O/S | `broken_intra_doc_links = "deny"`（現状） | 101 | 有効 |
| P | `= { level = "deny", priority = 1 }`（テーブル形） | 101 | 有効 |
| Q | `= "forbid"` | 101 | 有効 |

### 1-c. 粒度（`src/lib.rs` の有無で分岐すべきか）

| # | 形 | exit | 判定 |
|---|---|---|---|
| J | **bin 専用 crate**（`src/lib.rs` 無し・`src/main.rs` の `//!` に切れリンク）+ opt-in | **101** | **bin でも deny は効く**（`error: unresolved link ... --> m\src\main.rs:1:35`） |
| J2 | 同じ bin 専用 crate・opt-in 無し | **0** | 沈黙 |
| L | `members = ["crates/*"]`（glob） | — | cargo は展開する。**スクリプト側が文字列を実パスとして読むと解決に失敗する** |

**粒度の裁定（全 member）は実測で裏づけられる**: `src-tauri/src/` と `snotra-settings/src/` に `lib.rs` は無い（`ls` 実測）。`src/lib.rs` の有無で分岐する案は、**4 member 中 2 件（製品本体の `src-tauri` を含む）を母集団から落とす**。しかも J が示すとおり bin crate でも deny は現に効く。分岐案は採ってはならない。

---

## 2. 独立の発見 — 「member 側の opt-in」だけでは、守りたい命題が閉じない

issue #713 は穴を「各 crate の opt-in 漏れ」と定義しているが、**実測 M・N・R は、member が全員 opt-in していても deny が沈黙で無効化される経路が 3 本あることを示す**。

- M: ルートの `[workspace.lints.rustdoc]` の 2 行を消す（`[workspace.lints.*]` は残る）→ exit 0
- N: `"deny"` を `"warn"` へ書き換える → exit 0
- R: `[workspace.lints]` を空にする → exit 0

いずれも member 側は `[lints] workspace = true` のままなので、**「全 member が opt-in を持つ」だけを見る検査は緑のまま通る**。

これは `.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は、その状態に到達する全経路を列挙する」に正面から当たる。検査を置いた瞬間、`governance:check` が緑であることは読者に「intra-doc の deny は全 crate で効いている」と読まれる。その読みが M/N/R で偽になるなら、**検査を置く前より悪い**（誤った安心が上乗せされる）。

**推奨**: `G-workspace-lints`（名前は §0 のとおり汚染済み）を**2 つの findings クラスを持つ 1 検査**にする。ユーザー裁定の「検査を 1 件足す」を守りつつ M/N/R を塞げる。

- クラス 1（member 側）: すべての member が opt-in を持つか
- クラス 2（ルート側）: **`[workspace.lints.rustdoc]` サブテーブル**が存在し、**非空**で、**その配下の全エントリの level が `deny` / `forbid`** か

**クラス 2 は `rustdoc` サブテーブルに限定する。** 「`[workspace.lints.*]` の全エントリが deny/forbid」という広い形は誤りである——`[workspace.lints.clippy] all = "warn"` はごく普通の設定であり、しかもこのリポジトリは clippy を**コマンドライン側で**昇格させている（`cargo clippy --workspace --all-targets -- -D warnings`・`.claude/hooks/post-edit.mjs:290-293` と `docs/build-commands.md` カテゴリ A）。広い形はこの正当な設定を赤にし、**次の人の最も安い直し方が「検査を緩める」になる**。#562/#706 が守っているのは rustdoc の deny であって lints テーブル一般ではない。

クラス 2 を「`broken_intra_doc_links` と `invalid_html_tags` が `deny`」という**lint 名の名指し**にしないのも意図的である——名指しはルート `Cargo.toml` の**写し**になり、契約冒頭（`scripts/governance-check.mjs:11-17`）の「写しを増やさない」に反する。カテゴリ名（`rustdoc`）1 つの指定なら写しにならず、rustdoc lint を足すときに script を触らずに済む。

**トレードオフ（明記すべき）**: 将来「意図的に warn 止まりにしたい **rustdoc** lint」を置きたくなったとき、この検査は赤になり script の更新を強いる。これは `AREA_BUDGET` と同種の「明示的な合意の摩擦」であり、欠点ではなく設計だが、**そう書いておかないと次の人は検査の方をこっそり緩める**。射程を rustdoc に絞ったことで、この摩擦が発生する頻度は「lints テーブル全般」版より大幅に低い。

**この推奨は scope 拡大なので、独断で入れずユーザーの裁定を仰ぐこと。** クラス 2 を採らない場合でも、**採らなかったことを検査のコメントに「受容する残余」として明記する**のが最低ラインである（issue の対応案がこの経路を見ていないため、書かなければ誰も知らないまま「緑＝安全」と読む）。

---

## 3. 変更集合（ファイル + シンボル粒度）

### 3-1. `scripts/governance-check.mjs`（必須）

| # | 種別 | シンボル / 位置 | 内容 |
|---|---|---|---|
| 1 | 新設 export | `workspaceMembers(snapshot)` | ルート `Cargo.toml` の `[workspace]` セクションにスコープして `members` を配列で返す。戻り値は `{ members, error }` の形にする（`error` は「`[workspace]` が読めない」「`members` 行が読めない」を区別する文字列）。**fail-closed** |
| 2 | **既存の書き換え** | `checkBuildCommands`（`scripts/governance-check.mjs:269-273`） | inline の `members` パースを 1 のヘルパへ載せ替える。**載せ替えないと member 導出が 2 箇所になり、ユーザー裁定の「写しを増やさない」に反する** |
| 3 | 新設 | `hasWorkspaceLintsOptIn(text)`（member manifest テキスト → boolean） | §5 の述語。純関数にしてテストから直接叩けるようにする |
| 4 | 新設（§2 を採る場合） | `rustdocLintsAreDenied(rootText)`（→ `{ ok, reason }`） | ルート側クラス 2 の述語（§5-c）。**`rustdoc` サブテーブルに限定する** |
| 5 | 新設 | `checkWorkspaceLints(snapshot)` | 3・4 を使う検査本体。返すのは `finding()` 配列 |
| 6 | 新設コメント | 5 の直前 | 他検査と同じ様式の `// ---` ブロック。**含める事実**: #706 の実例／実測 B・C・E が沈黙で D・H・K は cargo が落とす（＝守るのは沈黙経路だけ）／J により bin crate でも deny は効くので `src/lib.rs` で分岐しない／I（`["lints"]`）は赤側の偽陽性として受容／§2 を採らないならその残余 |
| 7 | 登録 | `buildChecks()` の配列（`scripts/governance-check.mjs:1358-1376`） | `{ id: "G-workspace-lints", run: () => checkWorkspaceLints(snapshot) }` を 1 行追加。**ここが検査 ID の SSOT**（`scripts/governance-check.mjs:1341-1346`） |
| 8 | 証跡 | `runAll()` の `evidence`（`scripts/governance-check.mjs:1391`） | `workspace member ${n} 件の lints opt-in` を足す。**母集団の接地は契約（:13）が要求している**——「0 件の member を検査して緑」を数字で区別できるようにする |

**留意（実装契約）**:

- `finding(file, line, message)` の `file` は**赤になった member の `Cargo.toml`**（例: `snotra-egui-runtime/Cargo.toml`）にする。`Cargo.toml`（ルート）に固めると、`file:line` を頼りに直す読者が誤ったファイルを開く。契約（:14）は `file:line` 付き全件列挙を求めている。
- **`\r` を必ず落とす／`\r?\n` で行分割する。** `.gitattributes` が固定しているのは `.githooks/**` だけで、`Cargo.toml` は CRLF で checkout されうる。`npm test`（= `governance-check.test.mjs`）は `ci.yml` の rust-check（**windows-latest**）でも走る。同じ罠は #587・#589・PR #793 で 3 回踏んでいる（`scripts/governance-check.mjs:498-501, 1015-1017` のコメントが記録）。
- 空母集団（`members` 0 件）は明示 fail（契約 :15）。

### 3-2. `scripts/governance-check.test.mjs`（必須）

- import 追加: `checkWorkspaceLints` / `workspaceMembers` /（採るなら）`hasWorkspaceLintsOptIn` `workspaceLintsAreDenied`
- 新規 `describe("G-workspace-lints — workspace lints opt-in の漏れ（#713）", ...)`（§6）
- 新規 `describe("G-workspace-lints カナリア — 実 Cargo.toml の 4 member が入力に現れる", ...)`（§6-c）

### 3-3. `docs/build-commands.md`（推奨・1 行）

`docs/build-commands.md:26` が opt-in 機構を説明している唯一の文書箇所。ここに「opt-in 漏れは `npm run governance:check`（G-workspace-lints）が検知する（#713）」を**一句だけ**足す。

- 面積 ratchet（`G-area-budget`）の母集団は `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]` + skill description（`scripts/governance-check.mjs:542, 654-675`）。`docs/` は**対象外**なので予算に影響しない。
- **`CLAUDE.md` / `AGENTS.md` へは書かない。** 常時ロード面の余白は現状 88 字（`scripts/governance-check.mjs:611-615`）しかなく、機構で吸収できるものを規範へ書くのは #593 の階梯を逆行する。

### 3-4. ルート `Cargo.toml:19-20` のコメント（任意・1 句）

「各 crate は `[lints] workspace = true` で opt-in」の直後に「（漏れは `governance:check` の G-workspace-lints が検知する）」。**crate を足す人が最初に開くのはこのファイル**なので、義務を行為者の視界へ入れる効果は `docs/` より高い（#778 が「義務が行為者の視界の外」を問題と名指ししている）。ただし `Cargo.toml` の編集は PostToolUse で `cargo-check` + `hook-selftest` を発火させるので、変更するなら hook-selftest が緑であることを確認する。

---

## 4. 同名・別概念（実 grep による分類）

### 4-a. `workspace = true` という字面

`grep -rn "workspace\s*=\s*true"` の**実リポジトリでの全ヒット**（`.superpowers/` と `workspace/` を除く）:

| 位置 | 概念 |
|---|---|
| `snotra-core/Cargo.toml:7` / `snotra-egui-runtime/Cargo.toml:10` / `src-tauri/Cargo.toml:7` / `snotra-settings/Cargo.toml:7` | **lints の opt-in**（探しているもの・4/4 が現状 opt-in 済み） |
| `snotra-core/Cargo.toml:3` / `snotra-egui-runtime/Cargo.toml:3` / `src-tauri/Cargo.toml:3` / `snotra-settings/Cargo.toml:3` | `version.workspace = true` = **`[workspace.package]` の version 継承**（別概念） |
| `snotra-egui-runtime/Cargo.toml:14,17,18,19` / `src-tauri/Cargo.toml:15` | `egui.workspace = true` 等 = **`[workspace.dependencies]` の継承**（別概念） |
| `Cargo.toml:20` / `docs/build-commands.md:26` / `.github/workflows/ci.yml:95` | **散文中の言及**（判定対象にしてはならない） |

**帰結**: `text.includes("workspace = true")` の形の述語は、**全 member で無条件に真になる**（4 member すべてが `version.workspace = true` を持つため）。これは「常に緑」＝最悪の沈黙である。**判定は字面ではなく TOML のセクション位置で行わなければならない**。この 1 点が本検査の中核リスクである。

### 4-b. `[lints` という字面

| 位置 | 概念 |
|---|---|
| `snotra-core/Cargo.toml:6` / `snotra-egui-runtime/Cargo.toml:9` / `src-tauri/Cargo.toml:6` / `snotra-settings/Cargo.toml:6` | member の `[lints]` テーブル（探しているもの） |
| `Cargo.toml:21` | `[workspace.lints.rustdoc]` = **ルート側の定義**（member の opt-in ではない） |
| `docs/build-commands.md:26` / `.github/workflows/ci.yml:95` / `Cargo.toml:20` | 散文 |

**帰結**: ルート `Cargo.toml` を member と同じ述語にかけると `[workspace.lints.rustdoc]` が `[lints` に引っかかる。**member の manifest とルートの manifest は別の述語で読むこと**（ルートは `members` に現れないので構造的に混じらないが、将来ルートが member を兼ねる形〔ルート `[package]` を持つ workspace〕になったら混じる。今はそうでない: `Cargo.toml` に `[package]` は無い・実測）。

### 4-c. `[workspace` という字面

`Cargo.toml` 内で `[workspace]` / `[workspace.dependencies]:10` / `[workspace.package]:16` / `[workspace.lints.rustdoc]:21`。**`members` をスコープせずに探すと `[workspace.metadata.*]` 等の別セクションを誤読する**——既存の `scripts/governance-check.mjs:269` はまさに**スコープしていない**（§7-a）。

---

## 5. 判定述語の仕様（実測 A〜I から導出）

### 5-a. member 側 `hasWorkspaceLintsOptIn(text)`

真を返すべきは A・F・G・I。偽を返すべきは B・C・E。D・H は cargo 自身が落とすのでどちらでもよい（赤に倒すのが自然）。

推奨の形（正規表現ベース・TOML パーサは依存ゼロ契約〔`scripts/governance-check.mjs:12`〕により使えない）:

1. テキストから `\r` を除去する。
2. 行を走査し、**最初の `[` 始まり行より前**（＝ TOML のルートテーブル）に `lints.workspace\s*=\s*true` があれば真（実測 F）。
3. `^\[lints\]\s*$` に一致する行を見つけ、**次の `^\[` 行またはファイル末尾までの範囲**に `^\s*workspace\s*=\s*true\b` があれば真（実測 A・G）。
4. それ以外は偽。

この形が実測に一致することの検算:

- B（`[lints]` 無し）→ 2 も 3 も当たらず偽。**正しい**
- C（`[lints.rustdoc]` のみ）→ `^\[lints\]$` に一致しない（`[lints.rustdoc]` は別行）ので偽。**正しい**（cargo も継承しない）
- E（`[package]` 配下の `lints.workspace = true`）→ `[package]` セクション内なので 2 の「最初の `[` より前」に当たらず、3 の `[lints]` も無いので偽。**正しい**（cargo は `unused manifest key` で無視する）
- D（`workspace = false`）→ 3 の値が `true` でないので偽＝赤。cargo も落ちるので二重に安全
- I（`["lints"]`）→ `^\[lints\]$` に一致せず**偽＝赤**。cargo 上は正当な opt-in なので**偽陽性**である

### 5-b. 誤判定しうる入力の全列挙と向き

| 入力 | 述語 | cargo | 向き |
|---|---|---|---|
| `["lints"]` クォート見出し（I） | 偽（赤） | 有効 | **偽陽性**。赤で騒ぐ＝沈黙しない。受容してコメントに明記する |
| `[ lints ]`（見出し内空白）| 偽（赤） | TOML 上は有効 | 同上・受容 |
| `[lints]` が複数回現れる | 最初の 1 つだけ見る実装だと取りこぼす | cargo は重複テーブルでエラー | 実装は「全 `[lints]` セクションのいずれかに真があれば真」にしておけば無害 |
| `workspace=true`（空白なし）| `\s*` で吸収され真 | 有効 | 一致 |
| `workspace = true # コメント` | `\b` 終端で真（実測 G） | 有効 | 一致 |
| `members = ["crates/*"]` glob（L） | `crates/*/Cargo.toml` が読めず **finding**（fail-closed） | cargo は展開 | **赤**。将来 glob を採るなら検査を直せと言う形＝正しい |
| member ディレクトリに `Cargo.toml` が無い | read が null → finding | cargo はエラー | 一致 |
| CRLF checkout | `\r` を落とさないと `^...\s*$` / `\b` が壊れる | — | **`\r` を落とさなければ沈黙側へ倒れうる。必ず落とす** |

**「実在する `Cargo.toml` の表記の揺れ」の実測**: 4 member とも `[lints]` が単独行・直下に `workspace = true`（`snotra-core/Cargo.toml:6-7`, `snotra-egui-runtime/Cargo.toml:9-10`, `src-tauri/Cargo.toml:6-7`, `snotra-settings/Cargo.toml:6-7`）。**`snotra-egui-runtime` だけ `[lints]` の直前に 2 行の `#` コメントが入る**（:8-9）——セクション見出しの直前にコメントがある形は現に在るので、「`[lints]` の直前行」に依存する実装を書いてはならない。

### 5-c. ルート側 `rustdocLintsAreDenied(rootText)`（§2 を採る場合）

1. `^\[workspace\.lints\.rustdoc\]\s*$` のセクションを集める。**1 件も無ければ finding**（実測 M・R をどちらもここで捕らえる）。
2. セクション配下（次の `[` 行まで）の `key = value` 行が 0 件なら finding。
3. 各行の level を `= "<level>"` または `= { ... level = "<level>" ... }` から取る。`deny` / `forbid`（実測 P・Q が有効形）以外があれば finding（実測 N）。
4. **`[workspace.lints.clippy]` など他サブテーブルは一切見ない**（上記トレードオフの理由）。

### 5-d. 述語そのものの実測（`AGENTS.md`「判定の中核は自分で測る」）

§1 は *cargo* の挙動の測定であり、上の述語が *その形* に一致するかは別の主張である。**§5-a / §5-c / §7-a のヘルパを Node で実装し、実ファイルと A〜I の各フィクスチャに当てて実行した**（`cargo` 実測と同一のテキスト）。以下は出力そのもの。

```
===== (1) 実 root Cargo.toml の members 抽出（members は 2〜7 行に跨る） =====
{"members":["snotra-core","snotra-egui-runtime","src-tauri","snotra-settings"],"error":null}

===== (2) 実 4 member の opt-in 述語 =====
  snotra-core            -> true
  snotra-egui-runtime    -> true
  src-tauri              -> true
  snotra-settings        -> true
  (参考) ルート Cargo.toml 自身 -> false  ※ [workspace.lints.rustdoc] を opt-in と誤読しないこと

===== (3) A〜I フィクスチャ（cargo 実測と同じテキスト） =====
  A: [lints] workspace = true           -> true  期待=true  OK
  B: [lints] 無し                        -> false 期待=false OK
  C: [lints.rustdoc] のみ                -> false 期待=false OK
  D: [lints] workspace = false           -> false 期待=false OK
  E: [package] 配下 dotted               -> false 期待=false OK
  F: ルート直下 dotted（最初の [ より前） -> true  期待=true  OK
  G: コメント・空白・行末コメント         -> true  期待=true  OK
  I: ["lints"] クォート見出し             -> false 期待=false OK
  X: version.workspace/egui.workspace のみ -> false 期待=false OK

===== (4) CRLF 版（赤 B / 緑 A） =====
  A(CRLF) -> true 期待=true
  B(CRLF) -> false 期待=false
  members(CRLF root) -> {"members":["snotra-core","snotra-egui-runtime","src-tauri","snotra-settings"],"error":null}

===== (5) ルート側クラス2（rustdoc スコープ版）を M/N/R/現状 で測る =====
  現状（deny 2 件）                        -> {"ok":true,"reason":"2 件すべて deny/forbid"}
  M: rustdoc 削除+clippy warn 追加         -> {"ok":false,"reason":"[workspace.lints.rustdoc] が無い"}
  N: deny -> warn                         -> {"ok":false,"reason":"broken_intra_doc_links の level が warn"}
  R: [workspace.lints] 空                  -> {"ok":false,"reason":"[workspace.lints.rustdoc] が無い"}
  P: テーブル形 level=deny                  -> {"ok":true,"reason":"1 件すべて deny/forbid"}
  Q: forbid                               -> {"ok":true,"reason":"1 件すべて deny/forbid"}
  誤爆確認: rustdoc deny + clippy warn 併存 -> {"ok":true,"reason":"1 件すべて deny/forbid"}
```

**読み取れること（すべて実測であり、推論ではない）**:

1. **述語の真理値ベクタは §1-a の cargo 実測表と完全に一致する**（A・F・G が真、B・C・D・E・I が偽）。ミスマッチ 0。
2. **`[workspace]` スコープの `members` 抽出は、実ファイルの 2〜7 行に跨る複数行 `members` を正しく読む**。`governance-check.mjs:269` が唯一スコープを持たない箇所であり（§7-a）、テスト側 2 本のカナリアが持つ形はこれで再現できる。
3. **X 行が本検査の生命線である**: `version.workspace = true` と `egui.workspace = true` **だけ**を持つ manifest は `false`（＝赤）になる。§4-a のとおり字面一致の述語ならここが真になり、検査は永久に緑で沈黙する。
4. **CRLF でも全ケースが同じ値を返す**（`\r` 除去を入れた場合）。除去を落とすと `^\[lints\]\s*$` は通るが `workspace = true\r` の `\b` 判定や section 分割が壊れうるので、除去は必須のまま。
5. **rustdoc スコープのクラス 2 は M・N・R の 3 経路すべてで赤になり、かつ「rustdoc deny + clippy warn 併存」という正当な設定では緑のまま**である（誤爆しない）。

---

## 6. テスト（フォールトインジェクション）

`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」。本件の述語はソース述語なので、**メモリ上の `snap()` フィクスチャに変異を当てる**（`scripts/governance-check.test.mjs:50-53` の既存ヘルパ）。実 `Cargo.toml` を書き換えてはならない。

### 6-a. `describe("G-workspace-lints — …", ...)` の it 一覧

**赤（＝実測で cargo が沈黙する入力そのもの）— これが必須の中核**

| it | フィクスチャ | 根拠 |
|---|---|---|
| 赤: member に `[lints]` セクションが無い | 実測 B | #706 の再現形 |
| 赤: member が `[lints.rustdoc]` だけを持つ | 実測 C | 継承しない |
| 赤: `[package]` 配下の `lints.workspace = true` | 実測 E | `unused manifest key` |
| 赤: **4 member 中 1 件だけ**が漏れている（残り 3 件は正常） | B の部分適用 | **findings が漏れた 1 件だけを名指しする**ことまで assert する（全件赤や 0 件にならない） |
| 赤（§2）: ルートの rustdoc エントリが消えている | 実測 M | |
| 赤（§2）: ルートが `deny` → `warn` | 実測 N | |
| 赤（§2）: `[workspace.lints]` が空 | 実測 R | |

**緑（＝誤爆しないこと）**

| it | フィクスチャ | 根拠 |
|---|---|---|
| 緑: 全 member が `[lints]` + `workspace = true` | 実測 A | |
| 緑: ルート直下 dotted `lints.workspace = true` | 実測 F | **`[lints]` テーブル形と等価な正当形。落とすと「正しい書き方が赤になる」** |
| 緑: `[lints]` 直前にコメント行がある | `snotra-egui-runtime` の実形 | |
| 緑: `workspace  =   true  # コメント` | 実測 G | |
| 緑（§2）: ルートに `[workspace.lints.rustdoc]` の deny と `[workspace.lints.clippy] all = "warn"` が併存 | §5-d (5) | **正当な clippy 設定で誤爆しないことの固定**。この it が無いと、射程を rustdoc に絞った理由が実装から失われる |
| 緑（§2）: level がテーブル形 `{ level = "deny", priority = 1 }` / `"forbid"` | 実測 P・Q | |

**不混入（＝判定対象外が入力に混じらない・両方向の入力集合検算）**

| it | 主張 |
|---|---|
| 不混入: `version.workspace = true` / `egui.workspace = true` だけを持つ member は**赤**になる | §4-a の同名別概念。**この 1 件が無いと、字面一致の述語が「常に緑」で通ってしまう** |
| 不混入: ルート `Cargo.toml` の `[workspace.lints.rustdoc]` は、どの member の opt-in にもならない | ルートの定義が member 側の判定へ漏れない |
| 不混入: `members` に無いディレクトリの `Cargo.toml`（例 `tools/x/Cargo.toml`）は母集団に入らない | 母集団がルート `members` に閉じている |

**母集団の欠落（契約 :15「空母集団は明示 fail」）**

| it | 主張 |
|---|---|
| `[workspace]` セクションが無い → finding | fail-closed |
| `members = []`（0 件）→ finding | 「0 件検査して緑」を潰す |
| member の `Cargo.toml` が読めない（glob 含む）→ finding | 実測 L |

### 6-b. CRLF

上記の赤・緑各 1 件を `\r\n` 版でも回す。`scripts/governance-check.test.mjs` は windows-latest でも走る（`ci.yml` rust-check）。

### 6-c. 実リポジトリ カナリア（`.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」）

`describe("G-workspace-lints カナリア — 実 Cargo.toml の member が入力に現れる", ...)`:

- 実ファイルから `workspaceMembers` で member を取り、**`.length > 0` を assert**（母集団の欠落を緑に見せない）。
- **守りたい対象を 1 件名指しする**: `snotra-egui-runtime` が `workspaceMembers` の返り値に含まれること（#706 で実際に漏れた当の crate）。
- 4 member それぞれについて `hasWorkspaceLintsOptIn` が真であること（＝現状の緑を固定）。

**既存カナリアとの重複を作らないこと**: `scripts/governance-check.test.mjs:82-113` の `describe("G-module-index/G-references 母集団カナリア — #701")` と `.claude/hooks/post-edit.test.mjs:609-641` の `describe("Cargo.toml members ドリフト検出カナリア — #500")` は、**それぞれ独自の正規表現で `members` を再パースしている**。新カナリアは `workspaceMembers` を import して使い、4 つ目のパーサを作らない。

### 6-d. 自動で追随するので**書かなくてよい**テスト

- `describe("検査 ID の形（#812 …）")`（`scripts/governance-check.test.mjs:942-958`）は `buildChecks` から ID を取るので、新 ID の形チェック・重複チェック・`検査 N 件` の件数一致は**自動で新検査を含む**。手で足さない。
- `describe("実リポジトリ スモーク（dogfood）")`（:960-967）も同様に自動で新検査を回す。**4 member が現状すべて opt-in 済みなので、この dogfood は追加変更なしで緑になる**（＝本検査は今在る赤を 1 件も直さない、純粋な回帰カナリアである）。

---

## 7. 同概念・別名（間接参照の洗い出し）

「workspace member の一覧」に相当する情報が、別名・別経路で参照されている箇所。**これが本件で最も見落としやすい面である。**

### 7-a. member 一覧の再導出（既存 3 箇所）

| 位置 | 導出の形 | スコープ |
|---|---|---|
| `scripts/governance-check.mjs:269` | `/members\s*=\s*\[([^\]]*)\]/` | **`[workspace]` にスコープしていない** |
| `.claude/hooks/post-edit.test.mjs:623-631` | `[workspace]` セクションへスコープ + `/^members\s*=\s*\[([^\]]*)\]/m` | スコープあり |
| `scripts/governance-check.test.mjs:88-96` | 上と同一の 2 段 | スコープあり |

**既存の潜在欠陥**: `scripts/governance-check.mjs:269` だけがスコープしていない。現状のルート `Cargo.toml` には `default-members` も `exclude` も無く（`grep -n "default-members\|exclude" Cargo.toml` → 0 件・実測）、`[workspace.metadata.*]` も無いので**今は誤読しない**。しかし `default-members = [...]` を足した瞬間に `members\s*=\s*\[` が**先に現れた方**を拾う（`default-members` の方が先に来れば誤読）。ヘルパへ載せ替える際に `[workspace]` スコープへ揃えるのが正しい。

**載せ替えの安全性は確認済み**: `checkBuildCommands` を叩くテストのフィクスチャは `scripts/governance-check.test.mjs:256` の `cargoRoot` **1 つだけ**で、`'[workspace]\nmembers = [...]\n'` と `[workspace]` 見出しを持つ（実測: `grep -n "members\|cargoRoot" scripts/governance-check.test.mjs` の全ヒットを列挙して確認）。スコープ付きヘルパへ載せ替えても既存の緑は赤にならない。

### 7-b. member 一覧の**派生した写し**（別名で在るもの）

| 位置 | 別名 | 本件との関係 |
|---|---|---|
| `scripts/governance-check.mjs:83-88` `MODULE_INDEX_CRATES` | crate → `src/`・拡張子 の写像。キーが実質 member 一覧 | 新検査の母集団に**使ってはならない**（CLAUDE.md を持つ crate に暗黙に依存した表。`scripts/governance-check.test.mjs:82-113` のカナリアがその依存を固定している） |
| `scripts/governance-check.mjs:907` `governanceDocs` の `/^(snotra-core\|snotra-egui-runtime\|src-tauri\|snotra-settings)\/CLAUDE\.md$/` | crate 名のハードコード列挙 | 同上・使わない |
| `scripts/governance-check.mjs:971` `LAUNCHER_PREFIXES` | crate ディレクトリの部分集合 | 別概念（config 到達性） |
| `.claude/hooks/post-edit.mjs:121-125` `selectChecks` の接頭辞分岐 | ディレクトリ名で crate を判定 | crate 追加時に更新が要る面（`post-edit.test.mjs:609` のカナリアが強制） |
| `.claude/hooks/post-edit.mjs:297-304` `buildCommand` の `cargo test -p <name>` | **package name**（ディレクトリ名ではない。`src-tauri` → `snotra`） | 同上 |
| `.github/workflows/ci.yml:70-90` 付近の `cargo test -p ...` 4 行 | package name の列挙 | 同上 |
| `docs/build-commands.md:14-17, 24, 179` | package name / ディレクトリ名の列挙 | 同上 |
| `.claude/rules/*.md` の `paths`（`snotra-core.md` `snotra-settings.md` `src-tauri.md`） | crate ディレクトリ | **`snotra-egui-runtime` 用の rules は存在しない**（実測: rules 7 本の frontmatter を全列挙）。本件とは無関係だが、crate 追加時の同期面としては同族 |
| `.github/workflows/e2e.yml:18` | `snotra-egui-runtime/**` の paths | 同上 |

**帰結**: crate を足したときに更新が要る面は少なくとも 8 面あり、そのうち**唯一鳴る仕掛けは `.claude/hooks/post-edit.test.mjs:609-641` の members カナリア**（ルート `Cargo.toml` を編集すると `hook-selftest` 経由で走り、ハードコードされた 4 件と不一致で落ち、`"members が変わった。selectChecks の接頭辞・buildCommand の test case・ci.yml・docs/build-commands.md を更新すること"` と言う）。

**このメッセージに lints opt-in は入っていない。** 新検査を入れるなら、このカナリアのメッセージへ「`[lints] workspace = true` を新 crate へ入れること」を足すのが**最も行為者の視界に近い**（#778 の「義務が行為者の視界の外」の構図そのもの）。ただしこれは `.claude/hooks/` の変更＝セーフティネットの変更なので、やるなら hook-selftest を回して確認する。**必須ではないが、費用 1 行に対して効果が大きいので推奨する。**

### 7-c. 沈黙の確認（なぜ機構が要るか）

- **`.claude/rules/` に `Cargo.toml` にマッチする `paths` は 1 本も無い**（rules 7 本の frontmatter を全列挙・実測）。新 crate の `Cargo.toml` を書いても rules は 1 本も配送されない。
- `selectChecks`（`.claude/hooks/post-edit.mjs:117-143`）で新 member の `Cargo.toml` が得るのは `cargo-check` **だけ**（`CARGO_MANIFEST = /(^|\/)Cargo\.toml$/`）。`hook-selftest` は `CHECK_DEFINITION` に**ルートの `"Cargo.toml"` しか無い**（:67-71）ため発火しない。
- そして `cargo check --workspace` は opt-in 漏れに対して exit 0（実測 B）。
- `cargo doc` は CI（`ci.yml:99-100`）でしか走らず、そこでも exit 0（実測 B・J2）。

**＝ opt-in 漏れは、hook・rules・CI・cargo のどれからも一切の出力を出さない。** `.claude/rules/safety-nets.md`「カナリアで守るのは沈黙する経路だけでよい」に照らして、**機構化は該当する**。issue の「そもそも機構化するか」への私の答えは**する**である（ユーザー裁定と一致するが、これは独立に導ける結論である）。

---

## 8. 同期が要る面 / 要らない面（検証済みの否定を含む）

### 要る

| 面 | 根拠 |
|---|---|
| `buildChecks` 登録表（`scripts/governance-check.mjs:1358-1376`） | **検査 ID の SSOT**（:1341-1346） |
| `runAll` の `evidence`（:1391） | 契約 :13「照合母集団の件数を印字」 |
| `scripts/governance-check.test.mjs` | 契約 :16-17「各検査はスナップショット注入の純関数で、テストが赤/緑/不混入を検証する」 |
| `docs/build-commands.md:26`（推奨・1 句） | opt-in 機構を説明する唯一の文書箇所 |

### 要らない（列挙して根拠を裏づける — `AGENTS.md`「「変更なし」と判断するときは、影響するケースを列挙して根拠を裏付ける」）

| 面 | なぜ不要か |
|---|---|
| **検査の件数を書いた面** | **リポジトリ内に手書きの件数・範囲は 1 件も無い**（`grep -rn "G-[a-z][a-z-]*"` の全ヒットを目視。`docs/build-commands.md` の「G1〜G12」は #812 で除去済み）。`evidence` は `checks.length` から計算され（:1391）、テストが `検査 ${ids.length} 件` を assert する（`scripts/governance-check.test.mjs:955-957`）。同期面が構造的に存在しない |
| `.claude/skills/health-check/references/mechanized-checks.md` | 旧 `Check N` → `G-*` の**移管表**（:7-12）。新設の検査には前身の Check 番号が無いので行が無い。追加すると「移管していないものを移管表に書く」ことになる |
| `.claude/skills/health-check/SKILL.md` | 各節が「この判定は `governance:check` が持つので実行しない」と書く形。lints opt-in は元々 `/health-check` の項目に無い（`grep -n "lints" .claude/skills/health-check/SKILL.md` → 0 件） |
| ルート `CLAUDE.md`「利用できるスキル」表 | `disable-model-invocation: true` の skill だけが対象（`scripts/governance-check.mjs:439-466`）。検査の追加とは無関係 |
| `AGENTS.md`「条件別チェック」表 | 「ガバナンス文書を変更 → `npm run governance:check`」の行が既に在り、新検査はその中に含まれる。**行を足すと G-area-budget の常時ロード面（余白 88 字）を食う** |
| `.claude/skills/implement/SKILL.md` の 4a 列挙 | `G-check-skill-enumeration`（:1196-1234）が見るのは `/…-check` **スキル**であって `G-*` 検査 ID ではない。`governance:check` は skill ではない |
| `.github/workflows/ci.yml` | `governance-check` job は `node scripts/governance-check.mjs` を丸ごと走らせる（:57-58）。検査の増減で workflow は変わらない |
| `docs/build-commands.md` の CI 対応表（:179-183） | `npm run governance:check` の行が既に在る。`G-ci-table`（:293-342）は行のコマンドが workflow に現れるかだけを見るので影響なし |
| `.claude/rules/safety-nets.md` / `governance-docs.md` | どちらも `paths` に `scripts/governance-check.mjs` を持つ（実測）ので、**実装者には自動配送される**。rules 本文の変更は不要（＝ rules 面の面積予算 8678 字にも影響しない） |
| `SPEC.md` | 製品の意図ではなく開発機構の話。`G-spec-sections` にも無関係 |
| `docs/adr/` | 却下した選択肢が生じるなら書く。§2 のクラス 2 を**却下する**判断をした場合は、その否定の知識に ADR の価値がある（`AGENTS.md`「意思決定記録（否定の知識＝なぜ B を却下したか）」）。クラス 2 を採るなら ADR は不要（否定の知識が生じない） |

---

## 9. 受容する残余（検査コメントに明記すべきもの）

1. **`["lints"]` / `[ lints ]` の異形は赤に倒れる**（実測 I）。向きは赤＝沈黙しないので受容。直し方は「`[lints]` と書く」で、実リポジトリの 4 member はすべてその形。
2. **`[lints]` に `workspace = true` 以外の lint 設定を併記する形**は cargo 側でエラーになる（実測 H）ので検査の射程外。
3. **member が opt-in しているのにルートの `[workspace.lints]` が無い形**（実測 K）は cargo が manifest エラーにする＝沈黙しないので射程外。
4. **§2 のクラス 2 を採らない場合**: ルート側の M・N・R が沈黙経路として残る。**「G-workspace-lints が緑 ⇒ 全 member で intra-doc の deny が効いている」は偽になる**。この場合、`AGENTS.md`「全称表現は前提条件とセットで書く。書けないなら書かない」に従い、コメント・文書のどちらにも「全 crate で deny が効いていることを保証する」と書いてはならない。書けるのは「全 member が opt-in の**字面**を持つ」までである。
5. **`members` の glob 形**（実測 L）は fail-closed の finding になる。将来 glob を採るならヘルパを直す必要がある——これは沈黙ではなく赤なので受容。
6. **クラス 2 を採る場合でも、見るのは `[workspace.lints.rustdoc]` だけである**（§2 のトレードオフ）。`[workspace.lints.clippy]` 等の他カテゴリが空になっても降格されても、この検査は緑のままである。clippy は `cargo clippy ... -- -D warnings` がコマンドライン側で昇格させており（`.claude/hooks/post-edit.mjs:290-293`）、workspace テーブルが担っていないため。**「lints 全般が守られている」と読める書き方をしてはならない**（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

---

## 10. 「この検査は不要」という反論への私の答え

issue が明示的に問うている「そもそも機構化するか（crate 追加は年に数回で費用対効果はどうか）」について。

**機構化する。ただし理由は「頻度」ではなく「沈黙の深さ」である。** §7-c で確認したとおり、opt-in 漏れは hook・rules・CI・cargo のいずれからも出力を出さない。#706 は #627 から #700 の検証中まで気づかれなかった。一方、**頻度が低いことは検査を不要にするどころか、必要にする**——低頻度の作業は手順が記憶に残らず、規範に書いても読み返されない。年 1 回の作業こそ機構が要る。

費用側の実測も小さい: 述語は 10 行程度、母集団は既に 3 箇所で再パースされている情報の 4 箇所目を**作らずに済ませる**（既存を 1 箇所へ寄せるので、正味の重複はむしろ減る）。文書の追加は 1 句で、面積予算の掛かる面（`CLAUDE.md` / `AGENTS.md` / rules）を 1 文字も増やさない。

**ただし §2 を無視して member 側だけ入れるなら、価値は目減りする。** その形は「#706 と同一の再発」1 パターンだけを止め、同じ結果をもたらす別の 3 パターン（M・N・R）を素通しにしたうえで、読者には「lints は検査済み」という印象を与える。**それをやるなら、残余を明記することが実装の一部である。**

---

## 11. 未検証・引き継ぎ事項

- cargo の挙動は **1.94.0 の 1 バージョンでのみ**測った。`[lints]` の継承規則は安定機能なので変わりにくいが、「実測した cargo のバージョン」を検査コメントに書いておくのが誠実である。
- §2 のクラス 2 は**scope の拡大**なので、ユーザーの裁定（「検査を 1 件足す」）の範囲内と読めるかは私の判断ではなく**ユーザーに確認すべき**である。
- §7-b の「`post-edit.test.mjs` の members カナリアのメッセージへ 1 行足す」案は `.claude/hooks/` の変更にあたる。`CLAUDE.md`「最重要ルール」2（エージェント設定の変更は合意してから）に触れうるので、実施する場合は明示的に提案してから行う。

### 付随して見つけた既存の腐り（#713 の射程外・別 issue 向き）

`docs/development-principles.md:70` が `scripts/governance-check.mjs` の定数を **`G12_NO_LAUNCHER_READ`** と書いているが、実際の名前は `NO_LAUNCHER_READ` である（`scripts/governance-check.mjs:982`）。検査 ID を連番から `G-<name>` へ移した #814 の取りこぼしと思われる。`G-stale-identifiers`（:1070-1161）が拾えないのは、母集団が `.claude/**` に限られ（`staleIdentifierDocs`・:1107-1109）、かつ述語が camelCase 限定（`STALE_IDENT`・:1102）で SCREAMING_SNAKE を見ないため。**#713 では触らない**（#713 のブランチに無関係な修正を混ぜない）が、記録として残す。
