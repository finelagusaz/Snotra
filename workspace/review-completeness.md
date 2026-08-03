# レビュー: 列挙の完全性

対象: `workspace/plan.md` / `workspace/research.md`（#825 + #819）。レンズ = 列挙の完全性。読み取りのみ。

**結論の要約**

- **Phase A の写しの列挙（5 頂点）は、独立に数え直しても増えなかった。** ただし列挙**表**から漏れた真の写しが 1 件（`scripts/visual-check-colors.ps1:7-9`）と、正本の**対側**が母集団に入っていない件（`snotra-core/src/config.rs:366`）がある
- **Phase A の受け入れ条件 1 と 3 は、書かれたままでは計画自身の編集で満たせない**（条件 1 は全称なのに検証 grep が `.ps1` 等を見ない／条件 3 は 2 つの別主張を 1 つの母集団に混ぜている）
- **Phase B の測定表は、実 detector の export 関数で全 8 行を再現できた（照合・finding とも完全一致）。** 表そのものは信用してよい
- **その表の「偽陽性 0」という分類は誤りである。** 採用行に残る `backgroundThrottlingPolicy` は、計画が M 軸を却下した理由（外部語彙）と**同じクラス**である。決定は変わらなくてよいが、根拠と B10 の項目立て・B12 の ADR 記述は書き直しが要る
- **Phase B の母集団は 3 か所欠けている**（`.github/**.md` 9 件・`PERFORMANCE.md` 8 件・`src-tauri/capabilities/README.md` 1 件）。いずれも**どの軸でも一度も測られていない**

---

## 独立に作った母集団（方法と件数）

計画書の列挙を出発点にせず、次の 4 系統で母集団を作った。

### (1) 識別子 grep — `CLEAR_COLOR` 全数

```bash
grep -rn "CLEAR_COLOR" . --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git
```

**92 件。** 内訳: `.superpowers/` 42 / `docs/superpowers/` 12 / `workspace/`（計画・調査書自身）20 / **残り 18 件が実質の母集団**。

### (2) 値 grep — `0x0028_2828` / `0x282828` / `#282828`

```bash
grep -rn "0x0028_2828\|0x282828\|#282828" . --exclude-dir=target --exclude-dir=node_modules \
  --exclude-dir=.git --exclude-dir=.superpowers --exclude-dir=workspace | grep -v "^./docs/superpowers/"
```

**17 件**（うち `snotra-core/src/config.rs` のテスト 4 件・`snotra-settings/src/tabs/visual.rs:24` の PRESETS・バイナリ 1 件を含む）。

### (3) 主張のマーカー語 grep — 計画の 3 語より広い語彙で

計画（A10）は 3 語（`落ちる検査は無い` / `一致は規約` / `機構ではなく規約`）を使う。私は言い換え・比喩・別語彙まで広げ、**`--include` も `.ps1`/`.psm1`/`.toml`/`.yml`/`.json`/`.ts` へ広げた**（計画の A10 は `.rs`/`.mjs`/`.md` の 3 種のみ）。

```bash
# P1（規約・取り決め系・11 語）
grep -rn "規約にすぎ\|一致は規約\|機構ではなく規約\|規約でしかな\|取り決め\|暗黙の前提\|暗黙の規約\|口約束\|紳士協定" . <9 種の --include>
# → 実質 4 件（renderer.rs:12 / CLAUDE.md:39 / window_coordinator.rs:700 / ADR-hook-fires-table-check.md:19 <別主題>）

# P2（検査・検出器・担保の不在系・15 語）
grep -rn "検査は無い\|検査はない\|検出器は無い\|検出器を持たない\|検知できない\|検知しない\|落ちる検査\|テストは無い\|テストはない\|自動検証は無い\|機械照合されない\|担保が無い\|担保はない\|保証されていない\|保証はない\|守られていない" . <9 種の --include>
# → 実質 19 件（うち docs/superpowers/ 4 件を除く 15 件を 1 件ずつ分類）

# P3（消費者ゼロ・死んだ書き込み系・9 語）
grep -rn "消費者ゼロ\|消費者は無\|消費者がゼロ\|誰も読\|読まれていない\|読み手が無\|死んだ書き込み\|死んだ値\|使われていない" . <9 種の --include>
# → 実質 6 件（development-principles.md:57/:63/:67 / governance-check.mjs:1226/:1228 / view.rs:352。他に SPEC.md:620 が「被覆」の文脈で当たるが別主題）

# P4（受容する残余・4 語）— ファイル別に件数を数えた
# → 45 ファイル 96 件。うち #825 の主題は 3 件（renderer.rs / snotra-egui-runtime/CLAUDE.md / visual-check-colors.ps1）
```

### (4) Phase B — **実 detector の export 関数そのものに問う**

計画の測定は `scratchpad/measure-stale-axes.mjs`（述語の複製）で行われている。複製は `.claude/rules/safety-nets.md`「複製に変異を当てる」の正しい手順だが、**複製の忠実度は検証されていない**。`makeSnapshot` / `currentVocabulary` / `staleIdentifierDocs` / `staleIdentifierTargets` / `scanStaleIdentifiers` はすべて `export` されているので、稼働中のガードを触らずに本物へ問える（`AGENTS.md`「列挙も SSOT のツール自身に問う」）。

```js
// scratchpad/measure.mjs / camel.mjs（リポジトリ外・成果物ではない）
import * as m from "file:///C:/workspace/Snotra/scripts/governance-check.mjs";
const snap = m.makeSnapshot("C:/workspace/Snotra");
m.scanStaleIdentifiers(snap, m.staleIdentifierTargets(snap)); // ベースライン
m.scanStaleIdentifiers(snap, <各母集団>);                      // 母集団を差し替えて測る
```

**ベースラインが完全一致した**: `targets=25 / 照合=1 / finding=0`（計画表のベースライン行「照合 1・finding 0」と同一）。

SCREAMING_SNAKE 述語（E 軸）と語彙源拡大は本体にまだ存在しないため、その 2 つだけ複製した（`currentVocabulary` は本物を呼び、`.yml`/`.yaml`/`.json` の本文を連結して拡大語彙とした）。

---

## 計画書の列挙に無い箇所

| ファイル:行 | 逐語（抜粋） | 分類 | 根拠 |
|---|---|---|---|
| `scripts/visual-check-colors.ps1:7-9` | 「config の既定色 `#282828` は `snotra-egui-runtime` の `CLEAR_COLOR` と一致するため、**既定のまま起動しても「色が届いていない」欠陥は観測できない**（`docs/development-principles.md`「config の値は到達性の検出器を持たない」）」 | **真の写し**（ただし今日も真・`build-commands.md:77` と同型） | 計画の変更ファイル一覧に**行が無い**。`build-commands.md:77` は「読み直しのみ」として表に在るのに、同じ主張の同じ写しであるこちらは母集団にすら入っていない。A7 が `:67` の主題文を弱めると、この `.ps1` の引用先の意味も動く——`:77` に課した「引用が空振りしないか」の確認（A9）は、こちらにも同じだけ要る |
| `snotra-core/src/config.rs:366-367` | `fn default_background_color() -> String { "#282828".to_string() }`（**doc コメントが 1 行も無い**） | **母集団の欠落**（触るかは計画者の判断） | 計画の正本選定の根拠は「**値を変える人が必ず開くのは定数の定義位置**」である。しかしこの一致には**定義位置が 2 つある**——`renderer.rs:13` と `config.rs:366`。母集団は「値を変える人が開く場所」であって「`CLEAR_COLOR` を含む場所」ではないのに、後者で数えたため片側だけが正本になった。**機構（テスト）が両側を捕まえるので実害は無い**が、`config.rs` 側を開いた人は「なぜ落ちたか」を知る手掛かりを持たない。1 行の doc が候補 |
| `docs/development-principles.md:128` | 「（例: `backgroundThrottlingPolicy` は Windows 非対応でビルドエラーになる）」 | **別クラス**（Phase B の分類誤り。下の独立節で詳述） | 計画は D-+E 採用行を「偽陽性 0」と分類し、B10 でこの語を「SolidJS/WebView2 期の識別子」8 件に含めている。実際はどちらでもない |
| `.github/codex-automation.md:34,38,39,41,54,63,66,121` | `OPENAI_API_KEY` / `CODEX_API_KEY` / `CODEX_RUNNER_COMMAND` / `CODEX_ALLOWED_ACTORS` | **Phase B 母集団の欠落**（9 件） | 実測。下の「Phase B の母集団に足りないもの」 |
| `PERFORMANCE.md:3,13,15,19,83` | `clearTimeout` / `setSize` / `setPosition` / `shouldShow` / `hideMainWindow()` / `notifyMainHidden()` | **Phase B 母集団の欠落**（8 件） | 同上 |
| `src-tauri/capabilities/README.md:18` | `CAPABILITY_FILE_EXTENSIONS` | **Phase B 母集団の欠落**（1 件） | 同上 |

### 探して 0 件だったもの（方法を明記する）

- **`.claude/**`（skills / rules / agents）に #825 の主張の写しは無い。** P1〜P4 の全パターンを `.claude/` 配下の 24 本の `.md` に当てて **0 件**。`.claude/skills/norm-review/SKILL.md` に「受容する残余」が 1 件あるが別主題
- **`SPEC.md` に `CLEAR_COLOR` も背景色の一致主張も無い**（(1) の grep で 0 件・計画の記載どおり）。`SPEC.md:620` は「受容残余」の語を使うがフォント被覆の話
- **`.yml` / `.toml` / `.json` に主張の写しは無い**（P1〜P4 を `--include` に含めて 0 件）
- **A10 の 2 本目（`消費者ゼロ`）の期待リストは正しい。** P3 を独立に打った結果、`development-principles.md:63`（概念定義）・`:67`（A7 が書き換える）・`governance-check.mjs:1228`（A4 が書き換える）・`view.rs:352`（過去形）の 4 箇所だけが当たった。`:57`・`governance-check.mjs:1226`・`SPEC.md:620` は別の言い回しで当たらない

---

## 計画書が挙げているが、実は対象外だと思うもの

**「対象外」に落ちるものは無かった。** 5 頂点はすべて実在し、すべて真に stale である（`renderer.rs:12` / `snotra-egui-runtime/CLAUDE.md:39` / `window_coordinator.rs:700` / `development-principles.md:67` / `governance-check.mjs:1228` を逐語で確認済み）。

ただし**受け入れ条件そのものが、計画の編集では満たせない形になっている**箇所が 2 つある。

### 受け入れ条件 3 は 2 つの別主張を 1 つの母集団に混ぜている

> 3. 一致を固定する機構の説明の**正本が 1 か所**（…）に定まり、**他 4 箇所はそこか機構そのものを指す参照になっている**

`research.md` 自身が「4 箇所は同一の主張の写しではない」と分離している——主張 A（一致に検査が無い）は `renderer.rs` / `snotra-egui-runtime/CLAUDE.md` の 2 箇所、主張 B（消費者ゼロ）は `development-principles.md:67` / `governance-check.mjs:1228` の 2 箇所である。**A4 と A6 が書き換えるのは主張 B であり、書き換え後も「一致を固定する機構」を指しはしない**（A7 の書き換え案を読むと、末尾に「一致は `src-tauri` のテストが固定するようになった」と括弧で入るだけで、参照ではなく歴史の補足である）。条件 3 を字義どおり検証すると必ず落ちる。

**修正案**: 条件 3 の母集団を主張 A の系（`renderer.rs` 正本 + `snotra-egui-runtime/CLAUDE.md` + `window_coordinator.rs` + `visual-check-colors.ps1` + `build-commands.md`）に限り、主張 B は条件 2 が受け持つ、と分ける。

### 受け入れ条件 1 は全称なのに、検証（A10）が全称を測っていない

> 1. …記述が、`docs/superpowers/` と `workspace/` を除く**リポジトリに 0 件**

A10 の grep は `--include=*.rs --include=*.mjs --include=*.md` の 3 種だけである。`.ps1` / `.psm1` / `.toml` / `.yml` / `.json` / `.ts` は見ない。

**この 3 語のパターンに限れば実害は無い**——私が 9 種の拡張子へ広げて打っても、`.ps1` 等に 3 語のいずれも現れなかった（0 件）。しかし `scripts/visual-check-colors.ps1` は現に同じ主張の写しを持っており、**「その 3 語で書かれていないから当たらなかった」だけである**。`AGENTS.md`「全称表現は前提条件とセットで書く」に照らせば、条件 1 を「リポジトリに 0 件」と書くなら A10 の `--include` を揃えるか、条件を「`.rs`/`.mjs`/`.md` に 0 件」へ弱めるかのどちらかが要る。

---

## 除外判断の検算

| 除外対象 | 計画の理由 | 検算 | 判定 |
|---|---|---|---|
| `.superpowers/` | SDD の作業バッファ・gitignore 済み。「防御的に残す」 | `scripts/governance-check.mjs:37` の `WALK_EXCLUDE_PREFIXES = ["workspace", ".claude/worktrees", ".superpowers"]`。**ツール自身が同じ除外を持つ**（同 :33-36 の doc が「CI のチェックアウトには存在しない＝手元と CI で別の母集団を見る」と理由も書く） | **成立**。計画の「防御的」という説明より強い根拠がある——SSOT のツールが同じ判断をしている |
| `workspace/` | 作業バッファ | 同上（`WALK_EXCLUDE_PREFIXES` に在る）。加えて `governanceDocs`（`:1215-1218`）も `workspace/` を落とす | **成立** |
| `docs/superpowers/` | 履歴資料（#589 で非規範化） | `governanceDocs`（`:1215-1218`）が `docs/superpowers/` を落とす。`CLEAR_COLOR` の 12 件はすべて日付付き plan / spec | **成立** |
| `docs/adr/` | ADR は否定の知識＝もう存在しない案を書く場所 | 実測で裏付けた: **camelCase 27 件・SCREAMING_SNAKE 1 件**（`ADR-area-metric-characters.md:16` `LINE_BUDGET`）で、camelCase 27 件のうち 20 件が `ADR-stale-identifier-detector-scope.md` 自身の却下記録である | **成立。ただし非対称を明記すべき** — `governanceDocs`（G-references の母集団）は `docs/adr/` を**含む**。つまり ADR は「パスの実在は照合されるが、語彙の現行性は照合されない」ことになる。これは正しい非対称（ADR が指すパスは今日も在るべきだが、ADR が語る識別子は消えていてよい）だが、**理由を書かないと後任が不整合と読む**。B12 の ADR へ 1 行 |

### 除外の中に「本当は直すべきもの」が無かったか

- **`docs/adr/ADR-config-default-fallback-references.md:5`**「#795 で…**一致を保つ機構は 1 つも無く（コメントの規範だけ）**、…「既定値の偶然の一致」の実例**だった**」——**stale ではない。** 文末が過去形で #795 当時の状態に錨止めされており、かつ主題は `default_*()` のリテラル写し全般であって `CLEAR_COLOR` ではない。**この 1 件は「ADR 除外が妥当である」ことの証拠でもある**——ADR は当時の判断をそのまま残す場所だから、現在形へ直すよう求める検査を当ててはならない
- `docs/superpowers/specs/2026-07-28-config-background-color-design.md:15,71,111` — `CLEAR_COLOR` の一致と「消費者ゼロ」の両方を述べるが、日付付き設計書＝歴史記録。**触らないでよい**
- `.superpowers/sdd/plan/spec-inventory-duplication.md:451` — 計画が A10 の注記で挙げている「別の残余」の実例。`.superpowers/` は走査対象外なので、そもそも偽の赤にならない。**計画の「防御的に残す」判断は無害だが、除外行が要る理由は「パターンを広げた人のため」ではなく「`.superpowers/` は gitignore 済みで CI に無い」である**——理由を差し替えたほうが、後で除外行を消してよいかの判断ができる

---

## 計画が「触らない」と判断した 4 箇所の再検分

| 箇所 | 計画の判断 | 独立判断 |
|---|---|---|
| `docs/build-commands.md:77` | 読み直しのみ（引用の空振り確認） | **同意。ただし不足。** 逐語「config の既定 `#282828` は…`CLEAR_COLOR` と一致するため、色が届いていなくても正常に見える」は今日も真。引用先の見出し「config の値は到達性の検出器を持たない」は A7 で見出しを変えないので G-heading-refs は緑。**ただし同じ確認が `scripts/visual-check-colors.ps1:9` にも要る**——同じ見出しを引いており、しかも `.ps1` は `governanceDocs` の母集団外なので G-heading-refs にも G-references にも守られていない（`governanceDocs` は `.md` のみ・`:1215-1218` 実測）。A9 を 2 箇所へ広げるべき |
| `src-tauri/src/egui_shell/mod.rs:290` | 値の写し（#795 の類）ゆえ #825 の射程外 | **同意。** 逐語は「softbuffer の `CLEAR_COLOR`（renderer.rs=0x282828）に合わせて config テーマ色にし、白→暗の点滅を消す（既定は従来どおり 0x282828）」。主張ではなく由来注記であり、`:279` が「`#282828` のリテラルをここへ再手打ちしない（spec 決定 4）」と別途規範を持つ。**なお同クラスの値の写しは他に 2 件ある**（`snotra-settings/src/tabs/visual.rs:24` の PRESETS `bg: "#282828"`・`scripts/visual-check-colors.ps1:287`）。いずれも射程外だが、「値の写しは mod.rs:290 の 1 件」と読めないよう research の表現に注意 |
| `src-tauri/src/egui_shell/view.rs:352` | 過去形・撤去済みの記録ゆえ真 | **同意。** 逐語「**`panel_fill` / `window_fill` はここに無い**——読む egui コンテナ…がリポジトリに 1 つも無く、消費者ゼロの死んだ書き込み**だった**（spec 決定 2）」。過去形で確定しており、A10 の 2 本目の期待リストにも正しく載っている |
| `docs/development-principles.md:63` | 3 形の名前は概念定義ゆえ変えない | **同意。ただし A7 の副次項目（`:61` の書き換え）は必須であって任意ではない。** `:61` の逐語は「この欠如は 3 つの形で**現れる**」。A7 が唯一の現行実例を歴史へ移すと、`:61-63` は「3 形が今も生きている」と主張したまま実例を失う。計画は A7 の bullet 内でこれを「弱めるか 1 節足す」と書いているが、**チェックリスト項目としては A7 に埋め込まれており独立していない**。実装者が A7 の差し替え文だけを貼って `:61` を読み飛ばす経路が開いている。`- [ ]` を 1 つ足すべき |

---

## Phase B の分類誤り（決定は変えなくてよいが、根拠と項目立ては書き直しが要る）

### 測定表そのものは全行再現した

実 detector の export 関数で全 8 行を再現した（照合数は `staleIdentifierTargets` のベースライン 1 件を含む数え方で完全一致）。

| 述語 | 計画の照合 / finding | 実測の照合 / finding | 一致 |
|---|---|---|---|
| ベースライン | 1 / 0 | 1 / 0 | ✔ |
| E 単独 | 7 / 0 | 7 / 0 | ✔ |
| D 単独 | 69 / 35 | 69 / 35 | ✔ |
| D- 単独 | 18 / 8 | 18 / 8 | ✔ |
| M 単独 | 9 / 2 | 9 / 2 | ✔ |
| D+E | 107 / 40 | 107 / 40 | ✔ |
| D-+E | 43 / 12 | 43 / 12 | ✔ |
| **D-+E + 語彙 `.yml`/`.json`（採用）** | 43 / **9** | 43 / **9** | ✔ |
| D-+M+E + 同語彙 | 73 / 13 | 73 / 13 | ✔ |

語彙拡大が消す 3 件も一致した: `GITHUB_TOKEN` ×2（`docs/build-commands.md:215`）・`CLAUDE_PROJECT_DIR`（`docs/hooks.md:67`）。**「真の腐りを 1 件も沈黙させない」も再現した**（camelCase 8 件・`G12_NO_LAUNCHER_READ` 1 件は拡大語彙でも残る）。

**表は信用してよい。** 以下は表の数値ではなく、**その 9 件の「真の腐り／偽陽性」への振り分け**への異議である。

### `backgroundThrottlingPolicy` は「真の腐り」ではない

`docs/development-principles.md:128` の逐語:

> - `tauri.conf.json` や platform 固有ファイルに設定を追加する際は、その設定が Windows でサポートされているか事前に確認する（例: `backgroundThrottlingPolicy` は Windows 非対応でビルドエラーになる）

これは **Tauri の設定スキーマのキーであり、リポジトリに現れてはならないことを述べるために名指しされている**。現行語彙に無いのは腐ったからではなく、**在ってはならないから**である。検出器が「現行語彙に載せろ」と要求する向きが逆になっている。

**計画自身の M 却下理由と同じクラスである**:

> M（モジュール `CLAUDE.md`）は採らない。真 1（`iconCacheSize`）に対し偽 3——`WM_SETCURSOR`（Win32 メッセージ）・`MARKER_DONT_FOCUS`（tao 内部定数）・`numFonts`（TTC ヘッダのフィールド）。**いずれもソースのコメントにしか現れない外部語彙である**

`backgroundThrottlingPolicy` は同じ外部語彙で、しかも**より極端**（ソースのコメントにすら現れず、現れてはならない）。**同じ現象に、軸によって別の分類規則が当たっている。**

同じ構造の実例をもう 1 件見つけた: **`PERFORMANCE.md:3` の `clearTimeout` は、「この節の具体例は WebView2 期のものである…`clearTimeout`・`invoke<ArrayBuffer>`・… は現行構成に対応物を持たない」という免責注記の中に在る。** つまり**「もう無い」と宣言するために名指しした語**が、その宣言を理由に鳴る。

**共通の構造**: この検出器は「**うちの死んだ識別子**」と「**外部の／消えたことを述べるために名指した識別子**」を区別できない。バッククォート内の識別子が「今も在るはず」という前提でしか読まれていないからである。

### 帰結（計画のどこを直すか）

1. **採用行の「偽陽性 0」は成立しない**（≥1）。**採用の決定自体は変えなくてよい**が、「除外リストを置かずに構造で偽陽性が消える」という Phase B の売りの根拠は 1 件分弱まる。B12 の ADR にはこの弱まりを書く
2. **B10 の「finding 8 件 / 相異なる識別子 6 個」は正しい**（実測で再現: `:39` `shouldShowResults` / `:78` `viewKind()` `interpKind()` / `:81` `assertNever` / `:83` `viewKind()` / `:84` `isInstantPrefix` `interpKind` / `:128` `backgroundThrottlingPolicy`）。**ただし内訳の性質が 5 + 1 である。** 前 5 個には計画が書いた処方（「バッククォートを外して散文にする」か「現行の等価物へ差し替える」）が効くが、`backgroundThrottlingPolicy` には**現行の等価物が存在しない**（存在してはならない）。散文化は可能だが、`tauri.conf.json` のキー名を散文で書くと読者が検索できなくなる。**処方が違うことを B10 に書き分ける**
3. **M 却下の根拠から `backgroundThrottlingPolicy` クラスを分離する。** 今の書き方だと「外部語彙は `docs/**` には出ない」という前提が暗黙に入っているが、**実測で出ている**。M を却下する本当の理由は「モジュール文書はラップ対象の外部 API を語る場所だから外部語彙の**密度**が高い」であって「`docs/**` には無い」ではない。B12 はこの区別を書く

---

## Phase B の母集団に足りないもの

計画は「検査対象に `docs/**.md` を足す（`superpowers/` と `adr/` を除く）」と決めた。M 軸で測ったのは**モジュール `CLAUDE.md` + ルート `CLAUDE.md`/`AGENTS.md` だけ**である。つまり下の 3 群は**どの軸でも一度も測られていない**。実 detector で測った結果:

| 母集団 | 文書数 | camelCase 照合 / finding | SNAKE 照合 / finding | 語彙拡大で消えるか | 性質 |
|---|---|---|---|---|---|
| **`.github/**.md`** | 4 | 0 / 0 | 9 / **9** | **消えない** | **外部語彙クラス。** `.github/codex-automation.md` が `OPENAI_API_KEY` / `CODEX_API_KEY` / `CODEX_RUNNER_COMMAND` / `CODEX_ALLOWED_ACTORS` を語る。これらは **GitHub の repository secret / variable** であり、リポジトリのどのファイルにも実体が無い（`.yml`/`.json` を語彙へ足しても消えない）。足すなら 9 件をどう扱うかの答えが要る |
| **`PERFORMANCE.md`（+ `RETROSPECTIVE.md` / `CONTRIBUTING.md` / `README*.md`）** | 5 | 8 / **8** | 5 / 0 | **消えない** | **免責注記つき歴史クラス。** 8 件すべて `PERFORMANCE.md` で、`clearTimeout` / `setSize` / `setPosition` / `shouldShow` / `hideMainWindow()` / `notifyMainHidden()`。**同ファイル `:3-6` に「この節の具体例は WebView2 期のものである（#532 SU7 でフロント撤去済み）」という免責が既に在り、そこで名指しされた語が鳴る**。`docs/adr/` と同じ扱い（除外）か、免責を散文化するかを決める必要がある |
| **`src-tauri/capabilities/README.md`** | 1 | 0 / 0 | 1 / **1** | 消えない | 外部語彙（`CAPABILITY_FILE_EXTENSIONS` は Tauri のビルド時定数） |
| `snotra-settings/SETTINGS-DESIGN.md` | 1 | 0 / 0 | 31 / **0** | — | **無害。** 照合 31 件で finding 0。**足すコストがゼロで、しかも設定 UI のデザイン文書＝識別子が腐りやすい面である。足さない理由が無い** |
| ルート `CLAUDE.md` / `AGENTS.md` | 2 | 4 / 0 | 0 / 0 | — | 無害（M 軸で測定済み・finding 0）。計画は M ごと却下したが、**ルート 2 本だけなら偽陽性ゼロで足せる**——M の偽陽性 3 件はすべて**モジュール** `CLAUDE.md` 側に出ている（`snotra-egui-runtime/CLAUDE.md:40` `WM_SETCURSOR` / `src-tauri/CLAUDE.md:101` `MARKER_DONT_FOCUS` / `snotra-settings/CLAUDE.md:72` `numFonts`）。**M を 1 つの軸として却下したことで、無害な半分まで一緒に落ちている** |

### D- の内部にも、除外理由が同じだけ当たる文書がある

D- が拾う 7 本は `architecture.md` / `build-commands.md` / `check-skill-skeleton-design.md` / `comment-guidelines.md` / `design/2026-05-31-coherence-staleset.md` / `development-principles.md` / `hooks.md`。

このうち **`docs/design/2026-05-31-coherence-staleset.md` は日付付き設計書**であり、`docs/superpowers/`（除外）と同じ性質を持つ。`docs/check-skill-skeleton-design.md` も設計書である。**今日はどちらも finding 0 なので実害は無いが、Phase B が決めるのは恒久の母集団**であり、日付付き設計書は放っておけば死んだ識別子を溜める。除外の述語を「`docs/adr/`」というパスで書くか、「歴史記録」という性質で書くかを B12 で決めておかないと、次に同じ議論を再導出することになる。

### 母集団欠落の fail-closed について（B1 の確認）

計画 B1 の「`staleIdentifierDocs` へは入れず `STALE_EXTRA_DOCS` の経路で足す」は**正しい**。`runAll` の `staleDocs.length === 0`（`:1725`）は `staleIdentifierDocs` を見ており、その doc コメント（`:1432-1434`）が「`STALE_EXTRA_DOCS` を混ぜると長さが常に 1 以上になり、その検知が永久に沈黙する」と明記している。実測で `staleIdentifierDocs` = 24 本・`staleIdentifierTargets` = 25 本（+`SPEC.md`）を確認した。

**ただし B1 の書き方だと `STALE_EXTRA_DOCS` が定数配列から動的リストへ変わる。以下は実測ではなく、まだ書かれていない B1 の実装形についての推論である**（実装形によっては collapse しない）。 現在は `export const STALE_EXTRA_DOCS = ["SPEC.md"]` で、`staleIdentifierTargets` が単純な spread をする。`docs/**` の glob を足すと `STALE_EXTRA_DOCS` は snapshot を引数に取る関数になり、**`SPEC.md` の「実在を問わず加える」性質**（`:1437-1438` の doc: 読めなければ母集団欠落として鳴る）が、フィルタ由来の空リストと**同じ表現になる**——`docs/**` が 0 件になっても `SPEC.md` が 1 件残るので鳴らない。**`docs/**` 側の母集団欠落を検知する経路を別に置くか、受容する残余として明記するかを B1/B8 で決める必要がある**（計画は「`.claude/**` の消滅」だけを守る話として書いており、新設する母集団の欠落は視界に入っていない）。

---

## 参考: 探して 0 件だった同型欠陥クラスの候補（すべて「現に在る機構を『無い』と述べてはいない」）

| 箇所 | 逐語（抜粋） | 判定 |
|---|---|---|
| `scripts/manual-smoke.ps1:21` | 「自動検出器を持たない不変条件（読み点の非対称・hide の順序・visual-only 変更の再描画）の**唯一の検出器**」 | **真。** `check:colors` が判定するのは main / results の**定常背景の最頻色**であって、読み点の非対称・hide の順序・visual-only 変更の再描画のいずれでもない（`scripts/visual-check-colors.ps1:16,167,304` 実測）。全称（「唯一の」）だが前提が併記されている |
| `docs/adr/ADR-folder-location-display-surface.md:69` | 「現在地表示は自動検出器を持たないため、常設の目視チェックリストへ項目を追加する案」 | **真。** かつ却下記録＝歴史。`docs/adr/` は除外対象 |
| `src-tauri/src/egui_shell/view.rs:542` | 「広く測れば…狭く測れば…——**どちらも検出器を持たない**（型でもテストでも捕まらず、カテゴリ D の目視だけが見る受容残余である）」 | **真。** フォント測定と描画の食い違いの話で、背景色とは無関係 |
| `scripts/governance-check.mjs:1219` | 「モジュール索引は G-module-index、文書参照は G-references が捕まえる（lib crate の `pub` 項目は `dead_code` の対象外なので、この検出器は関数にも穴を持つ）」 | **真。** 穴の申告が現況と一致 |
| `docs/development-principles.md:78-79`「既定値のリテラルを写さない」 | 「`.unwrap_or(600.0)` のような再手打ちは今日たまたま一致しているだけである」 | **真。** #795 / #824 で読み元ごと寄せた実例は ADR が持つ |
| `snotra-core/CLAUDE.md:17` | 「`dirs::config_dir()` を直接呼ぶ箇所は他に無い」 | **真。** `grep -rn "dirs::config_dir()" --include=*.rs`（`target/` 除く）は 5 件で、production の呼び出しは `snotra-core/src/config.rs:685`（`Config::config_dir()` 本体）1 件のみ。`:1245` は `#[cfg(test)]` 内の結線 pin テスト、残る 3 件は doc コメント |

---

## 実装者への申し送り（優先順）

1. **[Phase B・決定に効く]** `backgroundThrottlingPolicy` を「真の腐り」から外し、外部語彙クラスとして B10 の処方と B12 の ADR を書き分ける。採用行の「偽陽性 0」を訂正する
2. **[Phase B・母集団]** `.github/**.md`（9 件）・`PERFORMANCE.md`（8 件）・`src-tauri/capabilities/README.md`（1 件）・`snotra-settings/SETTINGS-DESIGN.md`（0 件）を測ったうえで、入れる／入れないを ADR に書く。**今の計画には「測っていないから入れない」しか根拠が無い**
3. **[Phase B・fail-closed]** `STALE_EXTRA_DOCS` を動的化したとき、新設母集団（`docs/**`）の欠落を誰が検知するか
4. **[Phase A・受け入れ条件]** 条件 3 の母集団を主張 A の系に限る。条件 1 の全称を A10 の `--include` と揃える
5. **[Phase A・列挙]** `scripts/visual-check-colors.ps1:7-9` を表へ追加し、A9 の引用確認を 2 箇所へ広げる
6. **[Phase A・列挙]** `snotra-core/src/config.rs:366` に 1 行の doc を置くか、置かない理由を書く（正本の対側）
7. **[Phase A・順序]** `docs/development-principles.md:61` の書き換えを A7 の bullet から独立した `- [ ]` へ出す
