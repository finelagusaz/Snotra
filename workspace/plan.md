# plan — issue #713: G-workspace-lints（rustdoc deny の実効性を検査する）

前提は `workspace/research.md`。ユーザー裁定は 3 件（いずれも 2026-07-28）:

1. **機構化する**。粒度は「全 member」（`src/lib.rs` の有無で分岐しない）
2. **root 側も見る** — 検査は 2 findings クラスを持つ（member 側の opt-in + root の `[workspace.lints.rustdoc]` の実効性）。`/plan-review` の独立導出が root 側の沈黙経路を発見し、主エージェントも実測で裏を取った
3. **`.claude/hooks/post-edit.test.mjs` の members カナリアの失敗メッセージに 1 行足す**（判定は変えず文言のみ）

## 守りたい命題

**`governance:check` が緑 ⇒ ルート `[workspace.lints.rustdoc]` の deny が全 workspace member で実効している。**

この命題を破る沈黙経路は実測で 5 つあり（cargo 1.94.0）、**そのすべてを塞ぐ**のがこの検査の射程である:

| # | 形 | exit | クラス |
|---|---|---|---|
| B | member に `[lints]` が無い（#706 の再現形） | 0 | 1 |
| C | member が `[lints.rustdoc]` だけを持つ（継承しない） | 0 | 1 |
| E | member の `[package]` 配下に `lints.workspace = true`（cargo が黙って無視） | 0 | 1 |
| N | root の `broken_intra_doc_links` が `"deny"` → `"warn"` へ降格 | 0 | 2 |
| R/R2 | root の `[workspace.lints]` が空 / `[workspace.lints.rustdoc]` から lint 行が消える | 0 | 2 |

**射程外（沈黙しないので検査しない）**: root に `[workspace.lints]` が無い（cargo が manifest エラー・実測 A/K）、member の `workspace = false`（同）、`[lints]` に他 lint を併記（同）。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `scripts/governance-check.mjs` | (1) member 導出の共有ヘルパ `workspaceMembers(snapshot)` を新設、(2) G-build-commands の inline 導出をヘルパへ載せ替え、(3) 述語 `hasWorkspaceLintsOptIn` / `rustdocLintsAreDenied` と検査 `checkWorkspaceLints` を新設、(4) `buildChecks` に `G-workspace-lints` を登録、(5) `runAll` の `evidence` に member 件数を追加 |
| `scripts/governance-check.test.mjs` | `G-workspace-lints` の赤/緑/不混入フィクスチャと実リポジトリ カナリアを追加。`checkWorkspaceLints` / `workspaceMembers` を import |
| `.claude/hooks/post-edit.test.mjs` | `:634` の members カナリア失敗メッセージに「新 crate には `[lints] workspace = true` を入れること」を追記（**文言のみ・判定は変えない**） |
| `docs/build-commands.md` | `:26`（opt-in の仕組みを説明している箇所）に「opt-in 漏れと deny の降格は `governance:check`（G-workspace-lints）が検知する」の一句を足す |

**SPEC.md 更新: 不要**（製品の挙動を一切変えない。`SPEC.md` を `lint|governance|workspace.lints|Cargo\.toml` で grep して 0 件・plan-review 文書レイヤーが実測）。

## 実装順序

### Phase 1 — 共有ヘルパと述語（検査本体の前）

- [ ] `workspaceMembers(snapshot)` を `governance-check.mjs` に追加する。返り値は `{ members: string[], error: string | null }`
  - ルート `Cargo.toml` を読み、`[workspace]` セクションを切ってから `^members\s*=\s*\[([^\]]*)\]` を取る（`scripts/governance-check.test.mjs:88-95` と同じ fail-closed の形）。**現 `:269` が `[workspace]` にスコープしていないのは既存の潜在欠陥である** — `default-members = [...]` を足した瞬間に先に現れた方を拾う（独立導出 §7-a）。ヘルパ化のついでに是正する
  - `error` を返す条件は 5 つ: ルート `Cargo.toml` が読めない / `[workspace]` セクションが無い / **`members` 行が無い** / 要素 0 件 / 要素に `*` を含む（glob は展開器を持たないので母集団の欠落として扱う＝ fail-closed。展開器は YAGNI）
- [ ] `hasWorkspaceLintsOptIn(text)`（クラス 1 の述語）を追加する。**行を舐めて現在のセクションを追う**形にする
  - `[lints]` セクション配下に `workspace = true` → opt-in（実測 C: `[lints.rustdoc]` は継承しない）
  - **最初の `[` 見出しより前**（ルート直下）の `lints.workspace = true` → opt-in（実測 F）
  - それ以外は非 opt-in。特に `[package]` 配下の `lints.workspace = true`（実測 E）
  - **セクション見出し行・キー行の双方で、行末コメント（`#` 以降）を落としてから trim して比較する** — `[lints]  # opt-in` はどちらも有効な TOML であり、厳密文字列比較は false negative を生む（plan-review 実装レイヤー）
  - この構造の理由をコメントに書く: `version.workspace = true`・`egui.workspace = true` が同じ字面で全 member に現れるため、**字面ではなく構文的位置で判定する**（`docs/development-principles.md` §6）
- [ ] `rustdocLintsAreDenied(rootText)`（クラス 2 の述語）を追加する
  - ルート `Cargo.toml` の `[workspace.lints.rustdoc]` セクションを切り、**非空**かつ**全エントリの level が `deny` / `forbid`** なら true
  - 値は 2 形を受ける: 文字列（`= "deny"`）と**テーブル形**（`= { level = "deny", priority = 1 }`・実測 P で有効）。テーブル形は `level = "..."` を読む
  - **lint 名を名指ししない**（`broken_intra_doc_links` 等を書くとルート `Cargo.toml` の写しになる）。カテゴリ `rustdoc` の指定だけなら写しにならず、rustdoc lint を足すときに script を触らずに済む
  - **`rustdoc` サブテーブルだけを見る**。`[workspace.lints.clippy] all = "warn"` はごく普通の設定であり、このリポジトリは clippy をコマンドライン側（`cargo clippy ... -- -D warnings`）で昇格させている（`.claude/hooks/post-edit.mjs:290-293`）。`[workspace.lints.*]` 全般へ広げると正当な設定が赤になり、**次の人の最も安い直し方が「検査を緩める」になる**

### Phase 2 — 検査本体と登録

- [ ] `checkWorkspaceLints(snapshot)` を追加する。**クラス 2 → クラス 1 の順**で見る（root が壊れていれば member 側の合否に意味が無いため、両方報告する）
  - クラス 2: ルート `Cargo.toml` が読めない → 「母集団の欠落」。`rustdocLintsAreDenied` が false → finding「`[workspace.lints.rustdoc]` が空か、deny/forbid でない level を含む（全 member が opt-in していても intra-doc の検出が黙って無効になる・#713）」
  - クラス 1: `workspaceMembers` の `error` があれば「母集団の欠落」の finding を返して終わる。各 member の `<dir>/Cargo.toml` を読み、読めなければ「母集団の欠落」、`hasWorkspaceLintsOptIn` が false なら finding「`[lints] workspace = true` が無い（ルート `[workspace.lints]` の deny がこの crate だけ黙って無効になる・#713）」
- [ ] 検査関数の直前に、**なぜこの検査が要るか**と**受容する残余**をコメントで書く（他の検査と同じ様式）。含める事実:
  - #706 の実例（`snotra-egui-runtime` が #627 から #700 の検証中まで素通りした）
  - 塞ぐ 5 経路（B/C/E/N/R）と、**射程外にした 3 経路**（root テーブルの消失・`workspace = false`・lint 併記 — いずれも cargo が manifest エラーにする＝沈黙しない）
  - **`[workspace.lints.clippy]` 等の他カテゴリは見ない**こと（clippy はコマンドライン側で昇格させており workspace テーブルが担っていない）。「lints 全般が守られている」と読める書き方をしない（`AGENTS.md`「全称表現は前提条件とセットで書く」）
  - 「意図的に warn 止まりにしたい rustdoc lint」を置くときこの検査は赤になり、script の更新を強いる。これは `AREA_BUDGET` と同種の**明示的な合意の摩擦**であって欠点ではない
  - **実測した cargo のバージョン（1.94.0）**
- [ ] `buildChecks` の配列に `{ id: "G-workspace-lints", run: () => checkWorkspaceLints(snapshot) }` を足す
- [ ] `runAll` の `evidence` に `workspace member <N> 件の lints opt-in` を足す
- [ ] G-build-commands（`checkBuildCommands`）の inline 導出（現 `:268-272`）を `workspaceMembers` の `members` へ載せ替える。**`error` はここでは報告しない**（同じ欠落を 2 件の finding にしない。members が空なら `crateNames` も空になり `cargo test -p` の行がすべて赤くなるので、この検査は従来どおり fail-closed のまま）

### Phase 3 — テスト（フォールトインジェクション）

`snap()` で注入する最小フィクスチャに対して行う。**稼働中の `Cargo.toml` は一切変異させない**（`.claude/rules/safety-nets.md`）。

クラス 1（member 側）:

- [ ] 緑: `[lints]` + `workspace = true` を持つ member 2 件 → findings 0 件
- [ ] 緑: ルート直下の dotted `lints.workspace = true`（実測 F の形）
- [ ] 緑: `[lints]  # 行末コメント` + `workspace = true  # コメント` → opt-in と判定される
- [ ] 赤: member に `[lints]` セクションが無い（実測 B・#706 の再現形）
- [ ] 赤: member が `[lints.rustdoc]` だけを持つ（実測 C・継承しない）
- [ ] 赤: member が `[lints]` に `workspace = false` を持つ
- [ ] 赤: `[package]` 配下の `lints.workspace = true`（実測 E・cargo が黙って無視する形を緑と誤判定しない）
- [ ] 不混入: `version.workspace = true`（`[package]`）と `egui.workspace = true`（`[dependencies]`）**だけ**を持つ member は opt-in と見なさない（＝赤になる）
- [ ] 不混入: ルート `Cargo.toml` の `[workspace.lints.rustdoc]` の本文は、member 側の判定に一切混入しない（読み取りが `<dir>/Cargo.toml` に閉じていること。member が非 opt-in なら赤のまま）

クラス 2（root 側）:

- [ ] 緑: `[workspace.lints.rustdoc]` に `= "deny"` が 2 件（現状の形）
- [ ] 緑: テーブル形 `= { level = "deny", priority = 1 }`（実測 P）と `= "forbid"`（実測 Q）
- [ ] 赤: `broken_intra_doc_links = "warn"`（実測 N・降格）
- [ ] 赤: `[workspace.lints]` は在るが `[workspace.lints.rustdoc]` が無い（実測 R/R2）
- [ ] 赤: `[workspace.lints.rustdoc]` が空テーブル
- [ ] 不混入: `[workspace.lints.clippy] all = "warn"` が在っても、rustdoc が deny なら**緑**（他カテゴリを射程に入れないことの固定）

母集団の欠落（`workspaceMembers` の 5 分岐を 1 つずつ踏む）:

- [ ] 赤: ルート `Cargo.toml` が読めない
- [ ] 赤: `[workspace]` セクションが無い
- [ ] 赤: `[workspace]` は在るが **`members` 行が無い**（`[workspace]\nresolver = "2"\n`。**この分岐は既存の空スナップショットでは踏まれない** — より手前の「読めない」ガードで return するため。`m[1]` への未処理アクセスで `throw` する回帰を検知する唯一の経路）
- [ ] 赤: `members` の要素 0 件
- [ ] 赤: `members` に glob 要素（`"crates/*"`）
- [ ] 赤: member の `Cargo.toml` が読めない

回帰・カナリア:

- [ ] CRLF フィクスチャ: `\r\n` 改行の member / root で判定が変わらない。**同じ「行を舐める」設計の `checkHookCommands` が CRLF で実際に壊れた実例がある**（`governance-check.test.mjs:413-419`・PR #595。CI は Windows checkout の autocrlf=true）
- [ ] glob 混入時の `checkBuildCommands` 挙動を固定する（載せ替えで**変わる**方向の唯一の入力。不変条件 5 の前提条件を機械で留める）
- [ ] カナリア（実リポジトリ）: `makeSnapshot(リポジトリルート)` に対して `checkWorkspaceLints` が 0 件であり、かつ **`workspaceMembers` の返り値に `snotra-egui-runtime` が現れる**（守りたい対象 1 件が実際に入力に現れることの検算・`.claude/rules/safety-nets.md`「検査の入力集合を、具体対象で検算する」）
- [ ] 既存の G-build-commands テスト（`:254-279`）が無改変で緑のまま（載せ替えが判定を変えていないことの固定）

### Phase 4 — 文書・フック・検証

- [ ] `.claude/hooks/post-edit.test.mjs:634` の失敗メッセージへ「新 crate には `[lints] workspace = true` を入れること」を追記する。**判定（`toEqual` の 4 件）は変えない**。crate を足す人の視界に最も近い面がこのメッセージであり、現在そこに lints が入っていない（#778 の「義務が行為者の視界の外」と同型）
- [ ] hook のテストが緑であることを確認する。`vitest.config.ts:8-12` の `include` が `.claude/hooks` を覆うので **`npm test` で足りる**（`.claude/hooks/**` の編集では PostToolUse hook も自動発火する・`docs/build-commands.md:113`）
- [ ] `docs/build-commands.md:26` に「opt-in 漏れと deny の降格は `npm run governance:check`（G-workspace-lints）が検知する（#713）」を一句足す
- [ ] `npm test` と `npm run governance:check` を実行し、両方 exit 0 を確認する（**パイプで `tail` に繋がない** — exit code がパイプ末尾のものに置き換わる・`docs/development-principles.md` §6）
- [ ] `node scripts/governance-check.mjs` の印字に `検査 18 件`（現 17 + 1）と member 件数が出ることを目視で確認する

## 不変条件

1. **検査は依存ゼロ・決定的・スナップショット注入の純関数である**（`governance-check.mjs:11-17` の契約）。TOML パーサを足さない・`fs` を検査関数から直接触らない
2. **母集団はルート `Cargo.toml` の `members` だけである**。crate 名・ディレクトリ名を検査側に列挙しない（`MODULE_INDEX_CRATES` や `governanceDocs` の crate 名列挙を母集団に流用しない — あれらは CLAUDE.md を持つ crate に暗黙に依存した派生の写しである）
3. **母集団が壊れた形（読めない・セクション無し・members 行無し・0 件・glob）は明示 fail する**。「member 0 件を舐めて findings 0 件」＝緑、という沈黙経路を作らない
4. **判定は構文的位置で行う**。`workspace = true` の字面一致で opt-in と見なさない（実測 E/F がこの不変条件を破る入力）
5. **G-build-commands の findings は、`members` に glob 要素が無い限り載せ替えの前後で変わらない**（前提条件つき。glob 混入時は新ヘルパが母集団全体を欠落と見なすため、旧実装の「読めない member だけを個別に落とす」挙動とは異なる。現リポジトリは glob 0 件・Phase 3 で機械的に留める）
6. **クラス 2 が保証するのは rustdoc カテゴリだけである**。他カテゴリ（clippy 等）の降格はこの検査では鳴らない
7. 検査 ID は `G-<name>` 形・連番を持たない（`.claude/rules/governance-docs.md`）。件数は `buildChecks` から計算されるので手書きの範囲表記を作らない

## 破壊不変条件と検知手段

| 壊れたら即アウトな不変条件 | 検知手段 |
|---|---|
| **「G-workspace-lints が緑 ⇒ 全 member で rustdoc の deny が実効している」** — 偽なら #706 が再発し、しかも「検査した」という誤った安心が上乗せされる（検査を置く前より悪い） | Phase 3 の赤フィクスチャが、**実測で cargo が沈黙した入力そのもの**（B/C/E/N/R）を 1 対 1 で写していること。緑側は実リポジトリのカナリアが担保する |
| **検査が母集団 0 件を舐めて緑になる** | Phase 3 の「母集団の欠落」赤フィクスチャ 6 形（`workspaceMembers` の 5 分岐 + member 不読）。特に「`members` 行が無い」は他の分岐では踏めない |
| **G-build-commands が載せ替えで壊れる** | 既存テストの無改変緑 + glob 混入フィクスチャ + `npm run governance:check` の実リポジトリ dogfood |
| **`.claude/hooks/` の変更が hook を壊す** | Phase 4 の hook テスト実行（判定は変えず文言のみの変更だが、実行して確かめる） |

失敗・異常時の振る舞い: 新しい状態・プロセス・スレッド・ファイルを一切導入しない（純関数 3 本 + ヘルパ 1 本）。異常系は「finding を返して exit 1」に一本化され、途中で `throw` する経路を作らない（`snapshot.read` は失敗時に `null` を返す契約）。

## テスト方針

- 追加: `scripts/governance-check.test.mjs` に `describe("G-workspace-lints …")` 1 ブロック（クラス 1: 緑 3 / 赤 4 / 不混入 2、クラス 2: 緑 2 / 赤 3 / 不混入 1、母集団の欠落 6、回帰・カナリア 4）
- 検証コマンド: `npm test`（`docs/build-commands.md` カテゴリ B）と `npm run governance:check`（同カテゴリ F）、および hook テスト
- `.rs` を触らないので cargo 系の検証は不要。`scripts/*.mjs`・`.claude/hooks/*.mjs` の編集では PostToolUse hook が vitest を走らせる（沈黙 = 合格）

## セルフレビュー

### `/plan-review` の結果（4 レイヤー・全件着地）

- **要対処 2 件を反映済み**: (a)「`members` 行が無い」赤フィクスチャの欠落（テスト層。実装宣言 5 条件に対しテストが 4 形しか無く、`throw` する回帰を検知できなかった）、(b) root 側の沈黙経路 N/R（独立導出。主エージェントが cargo 1.94.0 で再実測して確認 → ユーザー裁定で射程に入れた）
- **軽微な懸念 3 件を反映済み**: `[lints]` 判定の行末コメント・トリム、CRLF フィクスチャ（#595 の実例）、不変条件 5 の前提条件明記 + glob 混入の機械的固定
- **独立導出との一致（完全性の証拠）**: 粒度「全 member」（bin crate でも deny が効くこと・`src-tauri` と `snotra-settings` に `lib.rs` が無いことを独立に実測）、字面述語が必ず沈黙すること、`checkBuildCommands:269` のスコープ欠陥、同期不要な面 8 つの根拠が、いずれも独立に再一致した
- **独立性の汚染の開示**: 独立導出は grep 出力経由で `plan.md` / `research.md` の行（検査 ID・関数名・述語の骨格）を目に入れており、それらの一致は独立収束の証拠に数えていない。**汚染されていない節（cargo の再実測・§7 の間接参照・§8 の同期面）から出た指摘だけを上の「一致」に数えた**
- **`/norm-review` は起動しない**: 本変更の主体は `scripts/governance-check.mjs`（コード＝機構）であり、`docs/build-commands.md` への一句は「検知経路が存在する」という事実の追記であって読者に新しい判断基準を課さない（`.claude/rules/safety-nets.md` の条項は規範へ**判定を足す変更**で起動する）。加えて `/norm-review` の指摘は採用率が低いという裁定が 2026-07-27 に出ている
- **`/dry-check`**: `checkBuildCommands:269` の members 導出は [置換]（Phase 2 で実施）。`governance-check.test.mjs:88-95` と `post-edit.test.mjs:622-631` のカナリア 2 本は [維持] — カナリアは本体の導出が壊れていないことを**独立に**測る装置であり、本体ヘルパへ寄せると検査対象と検査手段が同一になって共倒れする

### 5b の 3 観点

1. **境界条件**: TOML の表記の揺れ（行末コメント・余分な空白・CRLF・BOM・`["lints"]` のクォート形）を列挙し、前 4 つはフィクスチャで踏む。`["lints"]`（クォートしたテーブル見出し）は **cargo 的には有効だがこの述語は非 opt-in と判定する**＝赤に倒れる。向きが赤（沈黙しない）ので受容し、検査コメントに残余として書く
2. **シンプル化の挑戦**: 新しい状態・プロセス・汎用インターフェースを一切導入しない（純関数のみ）。TOML パーサ導入は「依存ゼロ」契約に反するため却下。glob 展開器は YAGNI（現 0 件・入れば赤で気づく）。**クラス 2 を lint 名の名指しではなくカテゴリ単位にしたのは、写しを作らないための最小形**である
3. **破壊不変条件 + 検知手段**: 上の表のとおり 4 件を検知手段とセットで記述した。「戻ってこない」系のリスク（Win32 フック・プロセス間通信）はこの変更に無い
