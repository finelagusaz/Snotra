# 独立導出 — #891 G-stale-identifiers の射程拡大

**成立条件の申告**: `workspace/plan.md` と `workspace/research.md` は開いていない。#891 の issue 本文も読んでいない（`gh issue list --search` で**タイトルだけ**を見た: 「腐り検出器 G-stale-identifiers の射程を docs/** と SCREAMING_SNAKE へ広げる（#819 案 B）」）。

**汚染の申告（2 件）**——どちらも独立導出を弱める。該当箇所は私自身の実測で置き換えてある:

1. `docs/adr/ADR-canonical-source-without-pointer-indirection.md:35`（HEAD にコミット済み）が **#891 の結論を先に書いている**: 「モジュール `CLAUDE.md`。#891 が測定のうえ『足さない』と決めた面」「`docs/adr/` は #891 が明示的に除外」。**この 2 つは私が独立に測って同じ結論に達したが、先に読んでしまった以上、独立の証拠ではない**。§2 に私自身の照合件数と finding を出す。
2. `grep -rn 'G-stale-identifiers' --include='*.md' .` の結果に `.superpowers/sdd/plan/*.md`（gitignore 済み・別サイクルの作業バッファ）が混じり、`audit-references.md:30/318` の行が**母集団と述語の両方を広げる案**として grep 出力に現れた。ファイルは開いていないが、grep 1 行分は目に入った。

測定ハーネスは `C:\Users\Eoh\AppData\Local\Temp\claude\C--workspace-Snotra\2b2184b6-6635-4cc7-862f-51ae76be5b70\scratchpad\`（`stale-scope.mjs` = `scripts/governance-check.mjs` から `makeSnapshot` / `governanceDocs` / `currentVocabulary` を import し、判定部だけを verbatim 複製して変異させたもの / `m1`〜`m7`）。**稼働中の `scripts/governance-check.mjs` は 1 文字も変更していない**（`.claude/rules/safety-nets.md`「複製に変異を当てる」）。

## ベースライン（実測）

```
npm run governance:check
→ 全検査 passed（… 散文の識別子 1 件を 25 文書から照合 …）
```

**照合しているのは 1 件だけである。** 25 文書のうち finding が出うるスパンは repo 全体で 1 か所しかない。「腐りゼロ」は測れていないに等しい。

---

## 導出した変更ファイルとシンボル

| file:line | シンボル | なぜ触るか（1 行） |
|---|---|---|
| `scripts/governance-check.mjs:1427` | `STALE_IDENT` | camelCase 専用。#819 の `G12_NO_LAUNCHER_READ` はここで落ちる（軸 2） |
| `scripts/governance-check.mjs:1427` 付近（新設） | `STALE_IDENT_SCREAMING`（仮） | SCREAMING_SNAKE 用の**別定数**。1 本の正規表現に `|` で畳むと後述の捕獲群ずれで沈黙する（§4） |
| `scripts/governance-check.mjs:1479` | `raw.match(STALE_IDENT)` | 述語 1 つ前提の分岐。述語配列を順に当てて最初に当たったものを採る形へ |
| `scripts/governance-check.mjs:1435` | `staleIdentifierDocs` | `.claude/**` 限定の母集団。**この関数は同時に「空検知の判定器」でもある**（`runAll:1725`）ので、新群を混ぜてはならない（§4） |
| `scripts/governance-check.mjs:1441` | `staleIdentifierTargets` | 検査対象の合成点。新群 B と拡張した `STALE_EXTRA_DOCS` をここで足す（重複除去つき） |
| `scripts/governance-check.mjs:1441` 付近（新設） | `staleGuideDocs`（仮・群 B = `/^docs\/.*\.md$/` − `docs/adr/` − `docs/superpowers/`） | glob 由来の新群。**opt-out 形にする**（深さ指定は新サブディレクトリを黙って落とす・§2）。**glob 由来ゆえ独自の空検知が要る**（§4 で実測） |
| `scripts/governance-check.mjs:1425` | `STALE_EXTRA_DOCS` | リテラル群。`[...ALWAYS_LOADED_FILES, "SPEC.md", "snotra-settings/SETTINGS-DESIGN.md"]` へ（`ALWAYS_LOADED_FILES` は :824 の既存 export） |
| `scripts/governance-check.mjs:1419` | `VOCAB_SOURCE_EXT` | 語彙源の拡張子 allowlist。`yml|yaml|json` を足す（軸 3） |
| `scripts/governance-check.mjs:1423` 付近（新設） | `VOCAB_GENERATED_FILE`（仮） | `VOCAB_TEST_FILE` と**同じ「ファイルの形」による構造的除外**。lock / 生成物 / CI 不在ファイルを語彙から外す（§5 で実測。除外リストではなく形） |
| `scripts/governance-check.mjs:1453` | `currentVocabulary` の `#` コメント剥がし分岐 | `/\.(ps1|toml)$/` → `yml|yaml` を同じ分岐へ（YAML も `#` コメント） |
| `scripts/governance-check.mjs:1684-1689` | `buildChecks` の `sink`（`staleDocs` / `staleTargets`） | 群 B を `sink` へ載せないと `runAll` が空検知できない |
| `scripts/governance-check.mjs:1725` | `runAll` の空検知 | **群ごとに 1 本ずつ**必要（単一の長さ検査では沈黙する・§4 実測） |
| `scripts/governance-check.mjs:1371-1416` | G-stale-identifiers 節コメント | 自称スコープの SSOT。「camelCase で書かれた再発だけ」「`.claude/**` の散文と `SPEC.md` だけ」が偽になる |
| `scripts/governance-check.test.mjs:929-932` | 「母集団は skills / rules / agents の md に限る」 | `docs/d.md` の**除外**を固定している。設計上落ちる（§4） |
| `scripts/governance-check.test.mjs:944-952` | 「検査対象は規範の散文 + SPEC.md」 | `staleIdentifierTargets` の**完全一致**を固定。落ちる（§4） |
| `scripts/governance-check.test.mjs:934-942` | 「語彙は production のソースだけ」 | `.json`/`.yml` の可否を 1 件も見ていない。追記が要る |
| `scripts/governance-check.test.mjs:959-978` | 配線 describe | SPEC.md 分しか無い。**新群ごとに 1 本ずつ足さないと母集団を戻しても緑**（§4） |
| `scripts/governance-check.test.mjs:709-715` | `runAll` 空母集団 | `snap({})` で全部空なので「どの guard が鳴ったか」を区別しない。新群の guard 欠落を捕まえない（§4） |
| `docs/development-principles.md:39,78,81,83,84,128` | 散文 8 スパン | 射程拡大で赤になる既存の腐り。**同じ変更で是正しないと dogfood テスト（`test.mjs:1304`）が赤のまま**（§3） |
| `docs/adr/ADR-stale-identifier-detector-scope.md` 末尾 | 「新しい射程」「残る残余」節 | 「述語は camelCase しか見ない」「検査対象は `.claude/**` + `SPEC.md`」が偽になる。ADR の追記形式（`## その後`）が既にある |

**触らない**と判断したもの: `EXTERNAL_CMD_LINE:1429`（SCREAM 追加で挙動は変わるが、変える必要はない——`gh`/`npm` 行の env 名を外す向きは正しい）、`linesOutsideFences:67`、`stripRustComments:1299`、`.claude/rules/*`、`AGENTS.md`「条件別チェック」表（トリガーは既存の「セーフティネットを新設/変更」が当たる）。

---

## 母集団の採否（境界例を 1 つずつ・実測つき）

### 明文化した基準

**その文書は「現在」を記述しているか。** 記述しているなら母集団、記録（過去の決定・過去の計画・過去の計測）なら母集団外。副次に 2 つ:

- **(i) 免罪の可能性がない外部語彙が支配的な文書は外す。** 外部 API / OS 定数 / 他所に保管された秘密の名前は、どの語彙源を足しても現行語彙に入らない。免除注記の機構を置かない契約（`scripts/governance-check.mjs:14`）の下では、赤を消す手段が「正しい引用からバッククォートを剥がす」しか無く、**文書を劣化させないと緑にできない**。
- **(ii) `checked` が増えない群は採らない。** 保護が増えないのに残余リスクだけ増える。

**この基準は「腐りが在るか」ではなく「文書のジャンル」で切る。** 腐りの有無は測れば分かるが、腐りが 0 の文書を外す理由にはならない（将来の再発防止こそが目的）。

### 採否（すべて camel+SCREAM 述語・語彙は現行のまま計測）

| 群 | files | checked | findings | 採否 | 理由 |
|---|---|---|---|---|---|
| `.claude/{skills,rules,agents}/**.md`（現行） | 24 | 1 | 0 | **維持** | 現行の母集団 |
| `SPEC.md`（現行） | 1 | 6 | 0 | **維持** | 意図の SSOT。軸 2 だけで checked 0 → 6 |
| **`docs/**.md` − `adr/` − `superpowers/`（opt-out 形）** | 7 | 35 | 11 | **採用** | `AGENTS.md`「ドキュメント参照」が列挙する現行の横断ガイド。#819 の実物がここに在った（`docs/development-principles.md:71`）。**形は opt-out にする**——下の「述語の形」参照 |
| **ルート `CLAUDE.md` / `AGENTS.md`** | 2 | 4 | 0 | **採用** | `ALWAYS_LOADED_FILES:824` そのもの。常時ロード＝最も読まれる規範面。実測 finding 0 で無料 |
| **`snotra-settings/SETTINGS-DESIGN.md`** | 1 | **31** | **0** | **採用** | **単一文書として最大の `checked` 増（+31）が finding 0 で手に入る**。デザイントークン名を大量に引用する現行の設計規約で、`AGENTS.md`「ドキュメント参照」に載る |
| `docs/adr/**.md` | 29 | 70 | **28** | **却下** | ジャンルが記録。28 件中 **21 件が `ADR-stale-identifier-detector-scope.md` 自身**の却下案節（`folderState` / `resetForShow` / `toolSelectionState` / `createObjectURL`）——**死んだ名前を書くことがこの文書の仕事**である。入れると ADR が自分を赤にする |
| `docs/superpowers/**.md` | 53 | 221 | **145** | **却下** | ジャンルが記録（過去の plan / spec）。FP 率 66%。既存 2 母集団（`governanceDocs:1202`・`headingRefDocs:1216`）が**どちらも明示除外**しており、3 つ目の母集団だけ入れる理由が無い |
| `docs/design/**.md`（1 本・`2026-05-31-coherence-staleset.md`） | 1 | 2 | 0 | **採用へ翻す** | 基準（ジャンル）では記録（`status: Agreed` + `date:` + `rev:` を持つ日付つき設計メモ）だが、**述語の形の議論が勝つ**（下記）。費用は +2 checked / 0 finding |
| **モジュール `CLAUDE.md` ×4** | 4 | 25 | 4 | **却下** | 4 件中 **3 件が免罪不能な外部語彙**（基準 i）: `WM_SETCURSOR`（Win32 メッセージ）・`numFonts`（TrueType Collection ヘッダのフィールド）・`MARKER_DONT_FOCUS`（tao 内部定数）。いずれも repo のソースには**コメントにしか現れず**（`snotra-egui-runtime/src/runtime.rs:493,500` / `snotra-settings/src/font.rs:171,239,273` / `src-tauri/src/egui_shell/results_window.rs:95`）、ADR「却下 5」がコメントを語彙に入れることを禁じている以上、構造的に緑にできない。**残る 1 件 `iconCacheSize` は真の腐りなので、母集団に入れなくても直す**（§3） |
| `PERFORMANCE.md` | 1 | 13 | 8 | **却下（最も際どい）** | 8 件すべてが WebView2 期の識別子で、**文書自身が冒頭 3-6 行で「この節の具体例は WebView2 期のものである（#532 SU7 でフロント撤去済み）」と開示済み**。是正の形は「歴史の識別子からバッククォートを剥がす」（`.claude/rules/governance-docs.md` の既存規範と同形）で機械的だが、8 スパンの書き換えが本 PR の diff の過半を占め、#819 型の再発防止に 1 件も寄与しない。**follow-up 候補**（+13 checked） |
| `.github/**.md`（4 本） | 4 | 9 | **9** | **却下** | FP 率 **100%**。9 件すべて `.github/codex-automation.md` の GitHub Secrets / Variables 名（`OPENAI_API_KEY` / `CODEX_API_KEY` / `CODEX_RUNNER_COMMAND` / `CODEX_ALLOWED_ACTORS`）で、**値も名前も GitHub 側に保管されリポジトリのどのファイルにも無い**（実測: `.github/workflows/` に該当 workflow が無い）。どの語彙源を足しても免罪できない（基準 i） |
| `src-tauri/capabilities/README.md` | 1 | 1 | **1** | **却下** | FP 率 100%・`checked` 増は 1。唯一の識別子 `CAPABILITY_FILE_EXTENSIONS` は **tauri-utils crate の定数**で、文書自身が出典を `tauri-utils-2.8.3/src/acl/build.rs` と書いている。基準 (i)(ii) の両方に当たる |
| `CONTRIBUTING.md` | 1 | 0 | 0 | **却下** | `checked` 0（基準 ii）。`ALWAYS_LOADED_FILES` にも `AGENTS.md`「ドキュメント参照」にも無い |

### 述語の形 — 深さ指定（opt-in）か opt-out か

**基準はジャンルなのに、書ける述語はパスの形しかない。** ここに乖離がある:

| 形 | 群 B | 文書 | checked | findings | 新しい `docs/<新ディレクトリ>/` が生えたら |
|---|---|---|---|---|---|
| 深さ指定 `/^docs\/[^/]+\.md$/` | 6 | 34 | 75 | 8 | **黙って母集団に入らない**（群 B は 6 本のまま＝空検知も鳴らない） |
| **opt-out `/^docs\/.*\.md$/` − `adr/` − `superpowers/`** | **7** | **35** | **77** | **8** | **自動で入る**（fail-toward-inclusion） |

差は `docs/design/2026-05-31-coherence-staleset.md` 1 本・checked +2・finding 増ゼロ（実測）。

**opt-out を推す。** 理由は費用ではなく方向である——深さ指定は「新しいサブディレクトリ」という**沈黙する漏れ口**を作り、その漏れは空検知にも `checked` にも現れない（`AGENTS.md`「消す/共通化する前に、後で読まれることに依存していないか」の裏返し）。しかも既存の `governanceDocs:1202` が**まさにこの形**（`docs/` から `superpowers/` だけを引く）を採っており、形を揃えれば読み手が 2 つの規則を憶えずに済む。**除外を 2 件明示することが、ジャンル判断を文書化する場所になる**（`docs/adr/` = 却下案の記録・`docs/superpowers/` = 過去の plan/spec）。

**この結果、`docs/design/` は却下から採用へ翻る**——「ジャンルは記録だが、記録ジャンルを名指しで除外する形にすると、名指ししていない記録ジャンルが将来生えても入ってしまう」というトレードオフを、finding 0 の実測を根拠に**入る側**で受けた。

### 採用案（軸 1+2+3）の総計 — 実測

| セル | 文書 | checked | findings |
|---|---|---|---|
| ベースライン（現行そのもの） | 25 | **1** | 0 |
| 軸 2 のみ（述語に SCREAM） | 25 | 7 | 0 |
| 軸 3 のみ（語彙 +yml+json） | 25 | 1 | 0 |
| 軸 1 のみ（母集団 A+B+C・深さ形） | 34 | 22 | 8 |
| 軸 1+2（深さ形） | 34 | 75 | 11 |
| 軸 1+2+3（深さ形） | 34 | 75 | **8** |
| **軸 1+2+3（採用案・opt-out 形）** | **35** | **77** | **8** |

**`checked` 1 → 77（77 倍）。** 群別内訳（深さ形での測定）: A（`.claude/**`）1 / B（`docs/` 直下 6 本）33 / C（リテラル 4 本）41。opt-out 形は B が +2（`docs/design/` 1 本）。

**軸 2 と軸 3 は単独では 1 件も動かさない。** 軸 2 単独が 0 なのは「`.claude/**` と `SPEC.md` に SCREAMING の腐りが今は無い」から（`SPEC.md` の checked が 0 → 6 に増えることは効いている）、軸 3 単独が 0 なのは「軸 1 が無ければ `.yml`/`.json` 由来の語を引用する文書が母集団に入らない」から。**3 軸は分離可能だが、価値は結合したときにしか出ない。**

---

## 射程拡大で出る finding 全件（分類と現行の等価物）

採用案（母集団 A+B+C / 述語 camel+SCREAM / 語彙 +yml+json−生成物）の finding は **8 件・全件が `docs/development-principles.md`**。

| # | file:line | 識別子 | 分類 | 現行の等価物（grep 実結果） |
|---|---|---|---|---|
| 1 | `docs/development-principles.md:39` | `shouldShowResults` | **真の腐り**（SolidJS 期・#532 SU7 でフロント消滅） | `present_results`（`src-tauri/src/egui_shell/layout.rs:211`）。旧名 `results_should_show` も #752 で消滅（`layout.rs:156,535,573,591,602` に**コメントとしてのみ**残る＝語彙に入らない） |
| 2 | `:78` | `viewKind` | **真の腐り** | `view_kind()` / `ViewKind`（`src-tauri/src/egui_shell/launcher_controller.rs:527,540,759,768,771`） |
| 3 | `:78` | `interpKind` | **真の腐り** | `QueryIntent` + `interpret()`（`src-tauri/src/egui_shell/search_state.rs:19,36`）。`search_state.rs:464` にコメントとして残るのみ |
| 4 | `:81` | `assertNever` | **真の腐り（言語ごと死んだ）** | TypeScript の網羅性イディオム。Rust の等価物は網羅 `match`（コンパイラが直接落とす）。repo に定義は無い（grep 0 件） |
| 5 | `:83` | `viewKind` | **真の腐り** | #2 に同じ。**この行は現在形の規範**（「整合検証は `/state-check` で行う」の例示）なので #2 より重い |
| 6 | `:84` | `isInstantPrefix` | **真の腐り** | `instant_prefix()`（`src-tauri/src/egui_shell/launcher_controller.rs:657,747`） |
| 7 | `:84` | `interpKind` | **真の腐り** | #3 に同じ |
| 8 | `:128` | `backgroundThrottlingPolicy` | **外部語彙**（Tauri の `tauri.conf.json` キー） | 無い。しかも文脈が「Windows 非対応でビルドエラーになる」＝**採用しなかったキーの例**。是正は「歴史ゆえバッククォートを外す」（`.claude/rules/governance-docs.md`「歴史を書くならバッククォートを外して散文にする」と同形） |

**真の腐り 7 / 外部語彙 1。** 7 件はすべて **WebView2 + SolidJS 期の識別子**で、`#532` SU7 のフロント撤去後も規範文書が現在形で指し続けていたもの。**この 7 件は #819 と同じクラス**（一括改名・一括撤去のときにしか生えない低頻度事象で、生えたら誰も気づかない）。

### 母集団に入れないが直すべき腐り（1 件）

| file:line | 識別子 | 現行の等価物 |
|---|---|---|
| `snotra-core/CLAUDE.md:21` | `iconCacheSize` | `Config::icon_cache_cap()`（`snotra-core/src/config.rs:626`）——**同じ行に既に書かれている**。「フロント `iconCacheSize` と `Config::icon_cache_cap()` はこれらから派生」の**フロント側が丸ごと消滅済み** |

モジュール `CLAUDE.md` を母集団に入れないので**機構は再発を捕まえない**（受容する残余）。

### フォールトインジェクション（守りたい対象の検算・両方向）

複製に変異を当てて実測（`m7-inject.mjs`。稼働中のガードは無改変）:

| 種 | 拡大後 | 現行検査 |
|---|---|---|
| `docs/development-principles.md:71` の `` `NO_LAUNCHER_READ` `` → `` `G12_NO_LAUNCHER_READ` ``（**#819 の実物**） | **鳴る**（findings 8 → 9・`G12_NO_LAUNCHER_READ` が名指しで出る） | **沈黙**（findings 0 / checked 1） |
| 同・無変異の対照（`` `NO_LAUNCHER_READ` `` のまま） | **鳴らない**（8 件のまま。判定対象外の不混入） | 沈黙 |
| `snotra-settings/SETTINGS-DESIGN.md` へ `` `deadCamelSeed` `` を蒔く（**新群 C の検算**） | **鳴る**（12 箇所） | **沈黙** |

---

## 壊れうる不変条件と検知手段の有無（実測）

### (1) 空母集団の fail-closed —— **これが最大のリスク**

現行コードは**同じ「空検知」を 2 つの別機構で**やっている（`m3-failclosed.mjs` 実測）:

| 消したもの | staleDocs | staleTargets | 鳴ったか |
|---|---|---|---|
| 何も消さない | 24 | 25 | 沈黙（正常） |
| `.claude/{skills,rules,agents}/**.md` を全消し | 0 | 1 | **鳴る**（`.:1 G-stale-identifiers の対象 md が 0 件`）← `runAll:1725` の長さ検査 |
| `SPEC.md` を消す | 24 | 25 | **鳴る**（`SPEC.md:1 対象文書が読めない`）← `scanStaleIdentifiers:1471` の read-null |
| `docs/**.md` を全消し（今は母集団外） | 24 | 25 | G-stale-identifiers は**沈黙**（別検査が鳴っただけ） |
| `snotra-settings/SETTINGS-DESIGN.md` を消す（今は母集団外） | 24 | 25 | **沈黙** |

つまり **glob 由来の群は長さ検査で、リテラル群は read-null で守られている**。`:1723-1725` のコメントが説明しているのは前者だけで、後者が無料で成り立っていることは書かれていない。

**射程を広げた後、新群が丸ごと消えたら鳴るか——実測:**

| 消したもの | 単一の長さ検査（素朴な実装） | 群ごとの検査 |
|---|---|---|
| `.claude/**` の散文を全消し | **沈黙** | 鳴る（prose） |
| `SPEC.md` + `docs/**` + ルート/モジュール md を全消し | **沈黙** | 鳴る（gov） |
| `SETTINGS-DESIGN.md` を消す | **沈黙** | 鳴る（design） |

**単一の長さ検査は 3 ケースすべてで沈黙する。** 現行の分離（`staleIdentifierDocs` ≠ `staleIdentifierTargets`）が守っていた不変条件が、群を足した方向へそのまま再発する。

**導出される必須要件（2 つ）:**

- **glob 由来の新群（B = `docs/` 直下）には、`runAll` に自分の長さ検査を 1 本足す。** `staleIdentifierDocs` に混ぜてはならない——混ぜると `.claude/**` の消滅が `docs/` 6 本に埋もれて沈黙する（`STALE_EXTRA_DOCS` について `:1433-1434` が書いているのと同じ機序）。
- **リテラルの新群（C）は `STALE_EXTRA_DOCS` へ入れれば追加の仕掛けが要らない。** read-null が個別に鳴ることを上表で実測済み（`SPEC.md` の行）。

### (2) 既存テストのうち**落ちるもの**

| test.mjs | 何を固定しているか | なぜ落ちるか |
|---|---|---|
| `:929-932`「母集団は skills / rules / agents の md に限る」 | `staleIdentifierDocs` が `docs/d.md` を**除外**することの完全一致 | 母集団を広げる以上、設計上落ちる。**この失敗は仕様変更の証拠であって、テストを消してはならない**——`staleIdentifierDocs` は空検知の判定器として `.claude/**` 限定であり続けるべきなので、**このテストは修正せず通るはずである**。落ちるのは新群を `staleIdentifierDocs` に混ぜた場合だけ＝**設計判断の検出器として機能する** |
| `:944-952`「検査対象は規範の散文 + SPEC.md」 | `staleIdentifierTargets(withProse)` の完全一致（`[".claude/rules/b.md", "SPEC.md"]`）と `staleIdentifierTargets(noProse) === ["SPEC.md"]` | `STALE_EXTRA_DOCS` が 4 本になるので**必ず落ちる**。更新が要る |
| `:1304-1311` dogfood「現在のリポジトリで全検査が緑」 | 実リポジトリの findings === [] | §3 の 8 件で落ちる。**これが「是正を同じ変更に含めないと分離できない」の実体** |
| `:1299` evidence「検査 N 件」 | 検査数は不変 | 落ちない（検査を増やさないため） |

### (3) 既存テストのうち**落ちるべきなのに落ちないもの**

| test.mjs | 何を見逃すか |
|---|---|
| `:709-715`「`runAll` 空母集団」 | `snap({})` で**全部**空にして `findings.length > 0` を見るだけ。どの guard が鳴ったかを区別しないので、**新群の空検知を書き忘れても緑**。→ 群ごとに「その群だけ空」のケースを足す |
| `:959-978` 配線 describe（SPEC.md 分のみ） | `:955-958` のコメントが自分で書いているとおり、**配線を戻しても関数単体テストは緑**。群 B / 群 C の配線を固定するテストが無いので、`staleIdentifierTargets` から新群を落としても実リポジトリの finding は 0 のまま＝**気づけない**。→ **新群ごとに配線テストを 1 本**（`buildChecks` 経由で赤フィクスチャが鳴ることを見る） |
| `:934-942`「語彙は production のソースだけ」 | `.json` / `.yml` の可否を 1 件も見ていない。`VOCAB_SOURCE_EXT` から `yml`/`json` を落としても緑。また**生成物除外（`VOCAB_GENERATED_FILE`）を落としても緑** → §5 のリスクが無検知で復活する |
| （全体） | **SCREAMING 述語の赤フィクスチャが 1 件も無い。** `STALE_IDENT_SCREAMING` を消しても既存テストは全部緑 |

### (4) 述語を 2 つにしたときに壊れうるもの

- **捕獲群のずれ（最大の落とし穴）。** `:1479-1482` は `im[1]` を使う。2 つの述語を**1 本の正規表現に `|` で畳む**と第 2 選択肢の捕獲群が `im[2]` になり、`im[1]` は `undefined` → `new RegExp("\\bundefined\\b")` が語彙に当たるか否かで**全件が黙って緑か全件が赤**になる。**述語は配列で持ち、それぞれ独立に `match` する**こと。
- **2 述語の交わりは空である**（実測: camel は小文字始まり、SCREAM は大文字始まり + `_` 必須）。ゆえに `checked` の**二重計上は起きない**。`seen` キャッシュは識別子文字列をキーにするので述語をまたいでも安全。
- **`checked` の意味が変わる。** 1 → 75 になるので、`runAll` の evidence 行（`:1731`）の数字を根拠にした記述が他所にあれば腐る（grep したかぎり `.md` 側に数字の写しは無い）。
- **`EXTERNAL_CMD_LINE`（`:1429`）の効きが広がる。** SCREAM を足すと `gh` / `npm` 等を含む行の env 名（`GITHUB_TOKEN` 等）も一括で外れる。向きは偽陰性側。今日の 8 件には影響しない。
- **単語 1 つの除外は SCREAM 側にも要る。** 私の `/^([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)$/` は `_` を 1 つ以上要求するので `TODO` / `SPEC` / `ADR` / `HTML` は当たらない（camel 側が「こぶ 1 つ以上」を要求するのと同じ形）。`_` を任意にすると却下 3 の問題（harness 語・散文語の大量流入）が SCREAM 側で再発する。
- **`raw.includes(".")` の早期 `continue`（`:1478`）は SCREAM でも効く。** `WM_IME_COMPOSITION` は通るが `tauri.conf.json` は通らない。意図どおり。

### (5) この変更が**閉じないクラス**（全称表現の管理）

- **命題の腐りは捕まらない。** `docs/adr/ADR-canonical-source-without-pointer-indirection.md:36` が明示している——#825 が腐らせたのは識別子ではなく「その一致に検査は無い」という**主張**であり、射程をどれだけ広げても検出器は命題の真偽を見ない。
- **rustdoc（`//!` / `///`）は 3 軸のどれにも入らない。** #882 が見つけた `apply_main_background`（実体は `apply_native_background`・`src-tauri/src/egui_shell/view.rs` の `//!`・80 コミット生存）は、**母集団が `.md` だけである以上、述語を snake_case へ広げても届かない**。→ §6。
- **節コメント `:1377-1383` の自称スコープが偽になる。** 「見るのは `.claude/**` の散文と `SPEC.md` の中の camelCase だけ」を、拡大後の実態へ書き換えること（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

### (6) **この変更の是非を書いた文書自身が、検知器を持たない**

`docs/adr/ADR-canonical-source-without-pointer-indirection.md:35-36`（HEAD にコミット済み）は **#891 の結論を事実として断言している**——「モジュール `CLAUDE.md`（#891 が測定のうえ『足さない』と決めた面）」「`docs/adr/` は #891 が明示的に除外」「`G-stale-identifiers` の述語に snake_case を足しても、4 箇所のうち 1 つも検査対象にならない」。

私の導出はこの 3 つと**一致した**（§2）ので今は真である。**しかし実装者が別の判断（モジュール `CLAUDE.md` を採用する / `docs/adr/` を採用する）を採れば、この ADR は偽になり、それを検知する機構は無い**——`docs/adr/` は私自身の推奨で母集団外であり、G-references はパスの実在しか見ず、G-heading-refs は見出ししか見ない。**これは「不在の主張は、書いた瞬間から腐り始める」（同 ADR「帰結」）の再演である。** 判断を翻すなら、同じ変更で `ADR-canonical-source-without-pointer-indirection.md:35-36` を直すこと。**検知手段は無い（規範だけが頼り）。**

---

## 語彙源の拡大で免罪される語

### 実効（実測・母集団 A+B+C・述語 camel+SCREAM）

| 語彙源 | checked | findings | 消えた finding |
|---|---|---|---|
| 現行 `rs|ts|tsx|mjs|ps1|toml` | 75 | 11 | — |
| `+ yml|yaml` | 75 | 9 | `GITHUB_TOKEN` ×2（`docs/build-commands.md:216`） |
| `+ yml|yaml + json` | 75 | **8** | `CLAUDE_PROJECT_DIR`（`docs/hooks.md:67`） |

### 新しく現行語彙に入る語（全件）

**`.yml`/`.yaml`（7 本・13 語）**: `BUILD_DATE` `GITHUB_ENV` `GITHUB_OUTPUT` `GITHUB_TOKEN` `SNOTRA_CONFIG_DIR` `TAG_NAME` `TAURI_SIGNING_PRIVATE_KEY` `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` `frontendDist` / ノイズ 4 語 `ddTHH` `ssZ` `yyyyMMddHHmm` `zA`（日付フォーマット断片）。

**`.json`（生成物除外後・6 本・16 語）**: `CLAUDE_PROJECT_DIR` `allTargets` `cachePriming` `createUpdaterArtifacts` `devDependencies` `enabledPlugins` `externalBin` `hooksPath` `installMode` `procMacro` `productName` `removeUnusedCommands` `semanticHighlighting` `unwantedRecommendations` `watcherExclude` / minisign 公開鍵の base64 塊 1 語。

### **入るべきでないもの（実測で発見・これが軸 3 の本体）**

`.json` を**素朴に**足すと、次の 3 種が語彙に混ざる:

| 混ざるもの | 実測 |
|---|---|
| `package-lock.json` | **単独で 142 語**を寄付。ほぼ全部が integrity ハッシュの base64 断片（`yByuxyS7BlSNRDOMLMlROYtjYdIAuBmJssVz1UJDSeYxLrdizhXCFYhedC5bqd` `hFPAhMUjJD9BSyCANEISPOogeXC9Zo9ZQl7L6vKnaVsMkCtzznaW` …）で、**camelCase 述語に当たる乱数**である。「偶然の免罪」を無限に量産する面 |
| `src-tauri/gen/schemas/*.json` | 306KB の**生成物**（`desktop-schema` / `windows-schema` / `acl-manifests`）。`macOS-schema.json` は gitignore 済み＝**プラットフォームで母集団が変わる** |
| `.claude/settings.local.json`・`test-results/.last-run.json` | **gitignore 済み＝CI のチェックアウトに不在**。手元と CI で語彙が食い違う（`makeSnapshot` の `.superpowers/` 除外コメント `:34-35` が名指ししている禍と同型）。実測: `test-results/.last-run.json` は `failedTests` を寄付する（`settings.local.json` は今日は 0 語） |

### 語彙源に**入らないまま残る**実装（測って発見・軸 3 の取りこぼし候補）

**`.githooks/` は語彙を 1 語も寄付しない。** 実測（`m9-optout.mjs`）:

- `.githooks/` の中身は `pre-commit` / `pre-merge-commit` / `pre-push` / `pre-rebase`（**拡張子なし**）・`_lib.sh`（**`.sh`**）・`githooks.test.mjs`。
- `VOCAB_SOURCE_EXT`（現行 + `yml|json`）に当たるのは `githooks.test.mjs` **1 本だけ**で、それは `VOCAB_TEST_FILE:1423` が外す。**残り 5 本は拡張子 allowlist に当たらない。**
- ゆえに `PROTECTED_BRANCH` / `PROTECTED_REF`（`.githooks/_lib.sh:12-13` で定義。ルート `CLAUDE.md` が「**main 保護の実体**」と呼ぶ層）は現行語彙に**存在しない**。

**今日の影響は 0 件**（採用母集団のどの文書もこれらをバッククォートで引用していない。バッククォート引用は `docs/superpowers/**` にしか無く、そこは母集団外）。**しかし穴は開いている**——`.claude/rules/safety-nets.md` や `docs/hooks.md` が `PROTECTED_BRANCH` を引用した瞬間、**確実な偽陽性**になる。ADR「受容する残余」の書式（「穴が無いのではなく、まだ何も落ちていない」）でそのまま明記できる形である。

**この 3 種を外す根拠は ADR 自身が持っている**——「テストコードを外す」の理由（検出器のフィクスチャが偽陰性を作った・`createObjectURL`）と**完全に同型**である。免除注記ではなく**ファイルの形**で外す（`VOCAB_TEST_FILE:1423` と同じ構え）。

**今日の finding は生成物を入れても外しても 8 件で同じ**（実測）。**採るべき理由は今日の数字ではなく、142 語の乱数が偽陰性を作る経路を先に塞ぐことである。**

---

## やらないほうがよい拡大

| 拡大 | 実測 | 却下理由 |
|---|---|---|
| **述語に PascalCase を足す** | **採用母集団で** checked 75 → 150（+75）、findings 8 → **39**（+31） | Pascal 単独 31 件の内訳が **外部 API 名**（`SendMessage` ×3・`ShellExecute`・`CopyFromScreen`・`IsWindowVisible`）と **SPEC 自身の状態名**（`ToolSelectionMode` / `FolderExpansionMode` / `NormalMode` / `CommandMode` / `InstantCommandMode` / `SearchVisible` / `LauncherStopped` — **SPEC の語彙であって Rust の型ではない**）。後者は `SPEC.md` と `state-check/SKILL.md` の**両方**に現れるので、ADR「却下 4」が消したはずの「SSOT と写しのどちらを直すか」が別の形で戻る。**却下 3（単語 1 つの識別子）が型名の水準で丸ごと再発する。** #891 も要求していない |
| **述語に snake_case を足す** | **採用母集団で** checked 75 → 270（+195）、findings 8 → **17**（+9） | **実測は「偽陽性が爆発する」を支持しない**（FP 率 4.6%）——却下の理由は費用ではなく**射程の外**である。9 件の内訳: 外部ライブラリ API 5 件（`push_id`=egui・`spawn_blocking`=tokio ×2・`idle_notification`=harness・`required_status_checks`=GitHub API）+ **明示的に「撤去済み」と書かれた歴史 4 件**（`compute_window_height` @ `docs/architecture.md:82` / `SPEC.md:184`・`get_bootstrap_payload` @ `SPEC.md:399`・`focus_lost` @ `SPEC.md:515`）。後者は**文書が正しく歴史として書いている**のでバッククォートを外す是正になる。#891 が挙げた 3 軸に無く、是正件数を倍にする。加えて `ADR-canonical-source-without-pointer-indirection.md:35` が実測済み——**snake_case を足しても `runtime_fallback_matches_config_default_background` の 4 箇所は 1 つも母集団に入らない**（拘束しているのは述語ではなく母集団）。**別 issue で検討する価値はある** |
| **`docs/superpowers/**` を母集団へ** | 53 文書 / checked 221 / findings **145** | 過去の plan・spec は死んだ識別子を書くのが仕事。既存 2 母集団がどちらも除外している |
| **`docs/adr/**` を母集団へ** | 29 文書 / checked 70 / findings **28** | 21 件が `ADR-stale-identifier-detector-scope.md` 自身の却下案節。**検出器が自分の設計記録を赤にする** |
| **`.github/**.md` を母集団へ** | checked 9 / findings 9（FP 率 100%） | 名前が GitHub 側に保管され、リポジトリのどのファイルにも無い |
| **`.json` を素朴に（生成物込みで）語彙へ** | 今日の finding は不変だが 142 語の乱数が語彙に入る | 上記 §5 |
| **rustdoc（`//!` / `///`）を母集団へ** | 未計測（§7） | **#882 の `apply_main_background` を捕まえる唯一の道**だが、#891 の 3 軸のどれでもない。射程が桁違いに大きく（Rust 全ソースの doc コメント）、`stripRustComments` を語彙側で使っている以上、**同じテキストが「検査対象」かつ「語彙の除外対象」になる**という設計上の緊張がある。**別 issue** |

---

## 要対処 / 軽微 / 未検証

### 要対処

1. **`runAll`（`scripts/governance-check.mjs:1723-1725`）の空検知を、群ごとに 1 本ずつ置く。** 単一の長さ検査は実測で 3 ケース**すべて沈黙**した（§4-(1) の表）。`staleIdentifierDocs:1435` に新群を混ぜてはならない——`:1433-1434` が `STALE_EXTRA_DOCS` について書いているのと同じ機序で `.claude/**` の消滅が沈黙する。
2. **`scripts/governance-check.test.mjs:709-715` を群ごとの空ケースへ分ける。** 現状 `snap({})` で全部空にしているため、**新群の空検知を書き忘れても緑**（§4-(3)）。
3. **配線テスト（`test.mjs:959-978` と同型）を新群ごとに 1 本足す。** `:955-958` のコメントが自ら書いているとおり、関数単体テストは配線を戻しても緑になる。群 B（`docs/` 直下）と群 C（`SETTINGS-DESIGN.md` 等）にそれぞれ赤フィクスチャが要る。
4. **2 述語を 1 本の正規表現へ `|` で畳まない。** `scripts/governance-check.mjs:1479-1482` が `im[1]` 固定なので、捕獲群がずれると `inVocab(undefined)` になり**全件が黙って緑か全件が赤**になる（§4-(4)）。配列で持ち独立に `match` する。
5. **語彙源の `.json` は生成物・lock・gitignore 済みファイルを形で外す。** `package-lock.json` 単独で **142 語**の乱数トークンが語彙に入る（実測）。`.claude/settings.local.json` / `test-results/.last-run.json` は gitignore 済み＝**CI に不在**で、`scripts/governance-check.mjs:34-35` が `.superpowers/` について名指しした「同じコマンドが手元と CI で別の母集団を見る」に当たる。
6. **`docs/development-principles.md` の 8 スパンを同じ変更で是正する。** 未修正だと dogfood テスト（`test.mjs:1304-1311`）が赤のままで分離できない。7 件は現行の Rust 等価物が在る（§3 の表）。`:83` は**現在形の規範**なので優先度が高い。
7. **`snotra-core/CLAUDE.md:21` の `iconCacheSize` を直す**（母集団に入れない判断とは独立に、真の腐り）。等価物は同じ行の `Config::icon_cache_cap()`（`snotra-core/src/config.rs:626`）。
8. **`scripts/governance-check.mjs:1377-1383` の自称スコープと `docs/adr/ADR-stale-identifier-detector-scope.md` の「新しい射程」「残る残余」を更新する。** 「見るのは `.claude/**` の散文と `SPEC.md` だけ」「述語は camelCase しか見ない」が偽になる。ADR は `## その後` 形式の追記の先例を持つ。
9. **私の §2 の採否と違う判断を採るなら、`docs/adr/ADR-canonical-source-without-pointer-indirection.md:35-36` を同じ変更で直す。** 同 ADR は #891 が `docs/adr/` とモジュール `CLAUDE.md` を除外することを**事実として断言**しており、**それを検知する機構は無い**（§4-(6)）。

### 軽微

- `PERFORMANCE.md`（checked +13 / findings 8）は**採用の最有力な次候補**。8 件は文書が冒頭で「WebView2 期」と自己開示済みで、是正の形は「歴史の識別子からバッククォートを外す」（`.claude/rules/governance-docs.md` の既存規範と同形）。本 PR に入れると diff の過半を占め、#819 型の再発防止に 1 件も寄与しないため見送りを推す。
- `docs/design/**`（checked +2 / findings 0）はジャンルでは記録だが、opt-out 形を採る帰結として母集団へ入る（§2「述語の形」）。**基準と述語が一致しない唯一の 1 本**なので、実装時に一言添えるとよい。
- `CONTRIBUTING.md` は checked 0 で保護が増えない。将来識別子を引用し始めたら `STALE_EXTRA_DOCS` へ足せばよい。
- **群 B の述語は opt-out 形（`docs/**.md` − `adr/` − `superpowers/`）にする。** 深さ指定 `/^docs\/[^/]+\.md$/` との差は 1 本・checked +2・finding 増ゼロ（実測）だが、深さ指定は将来の `docs/<新ディレクトリ>/` を**空検知にも `checked` にも現れない形で**落とす。既存の `governanceDocs:1202` と形が揃う副次効果もある。
- **`.githooks/**` は語彙を 1 語も寄付しない**（実測: 5 本が拡張子 allowlist 外、1 本が `.test.mjs`）。今日の finding は 0 件動かないが、`PROTECTED_BRANCH` / `PROTECTED_REF` は現行語彙に無い。ADR の「受容する残余」へ「穴が無いのではなく、まだ何も落ちていない」形で 1 行明記する価値がある。
- `scripts/governance-check.test.mjs:929` のテスト名「母集団は skills / rules / agents の md に限る」は、群が 3 つになると**検査全体の母集団についての主張と読める**。実際に固定しているのは `staleIdentifierDocs`（＝空検知の判定器）なので、同じ変更で「空検知の判定器は `.claude/**` の散文に限る」等へ改名する。
- `EXTERNAL_CMD_LINE:1429` は SCREAM 追加で効きが広がる（`gh`/`npm` 行の env 名が一括で外れる）。向きは偽陰性側で、今日の 8 件には影響しない。変更不要。
- `runAll` の evidence（`:1731`）の「散文の識別子 N 件を M 文書から照合」が 1/25 → 75/34 になる。`.md` 側にこの数字の写しは grep で 0 件（腐る面は無い）。

### 未検証

- **CI での実測**。`ci.yml` の `governance-check` job で `.claude/settings.local.json` / `test-results/` が不在になることは gitignore から**推論**しており、実際の CI ログでは確認していない。`.claude/rules/safety-nets.md`「CI の実測は PR が在って初めて行える」に当たるので、**PR 本文のチェックリストへ送る**べき項目。
- ~~`.vscode/*.json` の扱い~~ **解決済み**: `.gitignore` の `/.vscode/` は**既に追跡済みのパスを untrack しない**ため、`git ls-files` が tracked と答えた時点で CI のチェックアウトに存在することが確定する。語彙源に入れてよい（`VOCAB_GENERATED_FILE` の対象外）。
- **`docs/*.md` 8 件の是正後に `checked` が減らないこと**。バッククォートを外す形の是正は `checked` を減らす（照合スパンが消える）。是正の形を「現行の等価物へ書き換える」に寄せれば `checked` は保たれる——**どちらを選ぶかは 8 件それぞれで別**であり、私はここまで測っていない。
- **rustdoc を母集団へ入れたときの件数**。§6 で却下したが、費用の見積もりは取っていない（#882 の `apply_main_background` を捕まえる唯一の道であることだけが分かっている）。
- **snake_case 述語の是正コストの内訳**。9 件のうち「歴史ゆえバッククォートを外す」で済むのが 4 件、外部 API ゆえ構造的に免罪できないのが 5 件と分類したが、**5 件それぞれについて「どの語彙源を足せば入るか」は測っていない**（egui / tokio は依存 crate のソースで、`target/` 配下＝`makeSnapshot` の走査外）。
- **2 件目の「実際に起きた腐り」の同定**。1 件目は #819（`G12_NO_LAUNCHER_READ` @ `docs/development-principles.md:71`・拡大後に鳴ることを実測済み）。2 件目は #882 の `apply_main_background`（`src-tauri/src/egui_shell/view.rs` の `//!`）だと推定するが、issue 本文を読んでいないため確定していない。**もしこれが 2 件目なら、#891 の 3 軸では捕まらない**（`.md` 母集団に `.rs` の doc コメントは入らず、述語 snake_case も要る）。この点は実装前に依頼側へ確認する価値がある。
