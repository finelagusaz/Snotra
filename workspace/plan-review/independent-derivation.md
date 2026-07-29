# 独立再導出: #804 smoke スクリプトの `SNOTRA_CONFIG_DIR` 化

対象: `gh issue view 804`（+ コメント 1 件）とコードのみを根拠に、必要な変更集合を独立に導出した。

---

## 0. 汚染の開示（先に読むこと）

**独立性は部分的に破れている。** `SeedConfig|RequireResults|ResultsQuery` を Grep tool でリポジトリ全体に当てたところ、除外指示のあった `workspace/plan.md` / `workspace/plan-snapshot.md` の**本文行がマッチ結果として返った**（ファイルは開いていないが、内容は目に入った）。具体的に見えたのは:

- 未解決点 (a) の裁定「`-SeedConfig` / `-RequireResults` を**両方撤去**し、results 検査の要求を**無条件へ格上げ**する」と、その根拠（`docs/build-commands.md` の予告文・opt-in の前提が偽になること）
- 却下した代替案 (i)「flag を残して既定 ON」(ii)「`-RequireResults` だけ残す」
- 触る面の表（`target/smoke-egui/profile` へ移す・常に seed する・skip NOTE 撤去・`e2e.yml` のコメント 9 行撤去）
- 実装チェックリストの過半と、検証項目の一部（引数なしで両 smoke が緑になること）

**帰結**: 本書のうち「計画と一致した」ことは、独立の裏付けにならない。**読む価値があるのは、上の行に現れなかった事柄だけ**である。よって以下は、一致点を淡々と書き、**差分・追加・反証を前に出す**構成にした。§2 の間接参照、§3 の分岐（smoke-startup を触るか）、§6 の沈黙経路、§7 の罠は、いずれも上の行に無かった内容である。

`workspace/plan-review/ci-workflow.md`（別レビュアの出力と思われる）は**意図的に読んでいない**。本書と併読する側で重複・矛盾を突き合わせること。

---

## 1. 変更集合

### 1.0 独立の再枠付け: これは「1 つの変更」ではなく**独立した 2 つ**である

issue は 4 つの制約が 1 本の根から生えていると書き、触る面に両 smoke を挙げる。しかし導出すると:

> **順序制約を殺すのに必要なのは `smoke-egui.ps1` だけである。**
> smoke-egui が自分のプロファイルを所有した瞬間、smoke-startup が `%APPDATA%\Snotra` に何を書こうと seed の成否に影響しない。`e2e.yml` の順序制約・`-SeedConfig` の条件分岐・`-RequireResults` の 3 つは、**smoke-egui 単独の変更で全部落ちる**。

`smoke-startup.ps1` の env 化は**それとは独立した第 2 の便益**（検証がユーザーの実データを触らない・実行の再現性）で、**独立した費用**（カバレッジが実 config/実索引から自明な seed へ縮む・§3.2 の first-run 問題を新たに背負う）を持つ。

**推奨: 両方やる。ただし別の理由で正当化し、レビュアが片方だけ却下できる形で提示する。** 以下の表は決定 A（smoke-egui・順序制約の解消）と決定 B（smoke-startup・実データ非接触）を分けて並べる。

### 1.1 決定 A: `scripts/smoke-egui.ps1`（順序制約の根を断つ・必須）

**消えるものと、その裏で生き残るものを対で書く。**（過剰削除に対する構造的防御。§6 の沈黙経路はここから導かれる）

| 消える | 生き残る（消してはならない） |
|---|---|
| `param()` の `[switch]$SeedConfig` | **seed する行為そのもの**（無条件化される）。TOML 本文・`[hotkey]`/`[appearance]`/`[paths]` が必須である根拠コメント・`scan = []` と `[[paths.scan]]` を併記してはならない注記は**全部残す**（ADR-config-dir-env-seam-rejected-alternatives の 2 が製品側で緩めない決定を固定している） |
| `param()` の `[switch]$RequireResults` と `if ($RequireResults -and ...)` の guard | **「results 検査は必ず走る」という要求**。guard は削除ではなく**無条件 throw へ格上げ**する（§6-B） |
| `$seededNow` 変数と `if (-not (Test-Path $cfgPath))` / `else { "Config already exists, seed skipped" }` の分岐 | seed の**健全性検査**（新規・§1.1.4） |
| `$ResultsQuery` の条件付き既定（`if (... -and $seededNow) { "z" }`） | `$ResultsQuery` パラメータ本体（開発者が別の文字を渡す口は残ってよい。ただし空は不可） |
| 末尾の `else { NOTE: results window coverage was SKIPPED ... }` ブロックと、それを選ぶ `if ($resultsChecked)` | 成功メッセージ（`egui smoke passed (show/hide + results show/hide observed, webview delta 0).` を**唯一の**成功出力にする） |
| `$resultsChecked` 変数と、それを読む 3 箇所（`egui_results:hide` の対検査・orphan 検査の前段・成功メッセージの分岐） | `egui_results:hide` の対検査と orphan 検査**そのもの**（無条件化する。/symmetric-check の対象） |
| 冒頭ヘッダコメントの `- -SeedConfig（CI 用）: config.toml 不在時のみ…既存 config は決して上書きしない。` | ヘッダの残り。`SNOTRA_CONFIG_DIR` で使い捨てプロファイルを使う旨に**差し替える**（新規記述＝検算対象・`docs/development-principles.md`「撤去（消す変更）の作法」） |
| seed 先を組む `Join-Path $env:APPDATA "Snotra"` | `$env:TEMP` に置く trace の `.err`/`.out`（**移してはならない**・§2.1） |
| `-RequireResults` の throw メッセージ本文（`config path: $(Join-Path $env:APPDATA 'Snotra\config.toml')` と「CI では…**前**に置けば seed が成立する」の指示） | — （この文言は順序制約の唯一の散文表現。**丸ごと消す**） |
| `:78-81` のコメント「共通ヘルパーにしないのは、この smoke が e2e.yml の **-RequireResults ゲート**に載る CI 経路だからである（#803 で分離を判断）」 | visual-check との**相互参照そのもの**（片方だけ直す事故の防止は生き続ける）。理由の記述だけ書き換える（#843 が共通化を引き取る予定である旨に） |

**追加するもの（新規記述＝すべて検算対象）**

1. **プロファイルの決定**: `$profileDir = Join-Path $PSScriptRoot '..\target\smoke-egui\profile'`。`target/` 配下に置く根拠（`cargo clean` が掃く／`CARGO_TARGET_DIR` 設定時は対象外という受容済み残余）は `visual-check-colors.ps1` と ADR-config-dir-env-seam-rejected-alternatives の 4 が既に持つので**参照で済ませる**（写経しない）。
2. **前回残骸の削除**: `config.toml.bak` と `*.bin` を実行前に削除する。**これは掃除ではなく検査の前提**である（§6-A）。`visual-check-colors.ps1` が同じ理由で同じことをしている。
3. **絶対パス化**: `New-Item -ItemType Directory -Force` → `(Resolve-Path $profileDir).Path` の順で絶対パスにしてから `$env:SNOTRA_CONFIG_DIR` へ入れる。相対値は `SPEC.md`「13. データ保存」の契約により**展開も絶対化もされず**、子プロセスの CWD 起点になる（`Start-Process` の作業ディレクトリは PowerShell の Location と一致するとは限らない）。
4. **seed 健全性の検査**（新規・費用ゼロ）: 本体の stderr は既に `$errPath` へリダイレクトされており、`[config] ` で始まる行が出れば読み込み失敗である。`Select-String -Path $errPath -SimpleMatch '[config] '` が 1 件でもあれば `$failures` に積む。根拠と「`.bak` の不在では証明にならない」理由は ADR-config-dir-env-seam-rejected-alternatives の 5 が持つ。
5. **env が効いたことの肯定的証拠**（新規）: 実行後にプロファイル配下へ `*.bin` が生成されていること。`visual-check-colors.ps1` の実測（索引 0 件でも `index.bin` は書かれる／`Stop-Process -Force` ゆえ `window.bin`・履歴は出ない）がそのまま使える。**smoke-egui は seed で `[[paths.scan]]` を 1 件持つので、index.bin は確実に出る。**
6. **scan ダミーの置き場**: 現行 `$env:TEMP\snotra_smoke_scan\zsnotrasmoke.exe`。`target/smoke-egui/scan/` へ移すと `cargo clean` が掃く対象に揃う（**プロファイル直下には置かない**——索引対象と保存先を同じディレクトリにすると、後で `*.bin` を数える検査 5 の母集団が濁る）。これは必須ではない（§5 で YAGNI 判定を書く）。
7. **env の後始末**: `$savedTraceEnv` と同じ形で退避・復元する。**`SNOTRA_TRACE` と同様に `Start-Process` の直後に戻せる**——env が要るのは子プロセス生成の瞬間だけである。`finally` で戻す形より窓が短く、throw 経路でも漏れない。

### 1.2 決定 B: `scripts/smoke-startup.ps1`（実データ非接触・推奨だが分離可能）

- `$profileDir = Join-Path $PSScriptRoot '..\target\smoke-startup\profile'`（**smoke-egui とは別ディレクトリ**。共有すると順序依存が別の形で復活する）。
- ループの**外**で 1 回作り、残骸を消し、**最小 TOML を seed する**。seed が必須である理由は §3.2（first-run が `snotra-settings` を spawn する）。この smoke は results 窓を出さないので `[[paths.scan]]` は**置かない**（`visual-check-colors.ps1` の seed に近いが `[visual]`/`[general]` も要らない＝**3 つ目の seed 亜種**が生まれる。§3.3 と §5 で扱う）。
- `$env:SNOTRA_CONFIG_DIR` の設定と復元は、**既存の `Restore-TraceEnv` と同じ形**で書ける（このスクリプトは既に env 退避・復元のパターンを持つ）。ループ内で毎回 set → `Restore-TraceEnv` の隣で戻す。
- `-ExePath` の既定が絶対パス `C:\workspace\Snotra\target\debug\snotra.exe` である点は本 issue の対象外だが、プロファイルを `$PSScriptRoot` 起点にすると**同一ファイル内で 2 つのパス導出流儀が並ぶ**。触らない判断でよいが、意識して残すこと。

### 1.3 `.github/workflows/e2e.yml`

| 消える | 生き残る |
|---|---|
| `Run egui smoke` の引数 `-SeedConfig -RequireResults` | `-ExePath target/release/snotra.exe` |
| 「**startup smoke より前に置く**（#686）」のコメント段落（`:65-73` 相当・9 行） | 「flip 済み（#532 SU7 PR2）: env なし＝既定が egui であること自体が検証対象」の 1 行（順序制約とは無関係） |
| — | **`:77-80` の first-run カバレッジ受容の注記**（→ §6-D。**理由が変わるだけで受容は生き残る**。丸ごと消すのは過剰削除） |

- **ステップの順序自体は入れ替えなくてよい**（入れ替える理由がもう無い＝入れ替えないことにも理由が無い。触らないのが最小）。
- **paths の検討**: この変更で smoke は `snotra-core/src/config.rs` の env seam に**機能依存**するようになるが、`e2e.yml` の `paths:` に `snotra-core/**` は無い（`**/Cargo.toml` 経由で偶発的に発火することはある）。#701 で `snotra-egui-runtime/**` を足したのと同型の穴である。→ §3.4 と §5 で判定。

### 1.4 `docs/build-commands.md`（散文・コンパイラ無し）

正確な着地点を**行番号ではなく節と主語**で挙げる:

1. 「変更後の検証チェックリスト」C 節の bullet「CI に検証を委ねるなら…」——**`-RequireResults` が機構化した**という引用が偽になる。**#671 の教訓（緑 ≠ 検査が走った）は生き続ける**ので、引用先を新機構（results 検査の無条件化＋ seed 健全性＋ env 有効性の 3 検査）へ**指し直す**。削除は誤り。
2. 「スモーク運用メモ」の `smoke-egui` の bullet（`-SeedConfig` の説明を含む文）——「config.toml 不在時のみ／既存 config は上書きしない」を削り、「`SNOTRA_CONFIG_DIR` で `target/smoke-egui/profile` を指す使い捨てプロファイル」へ差し替える。**空 TOML が破損復旧経路を踏むから使わない**という理由は残す。
3. 同 results 窓の bullet——「索引内容を制御できるときだけ」「どちらも無ければ自動的に skip」「黄色 NOTE」を削る。**CONTRIBUTING.md との対応を述べた括弧書きは残す**（CONTRIBUTING 側は今も真）。
4. `-RequireResults` の bullet（順序制約・沈黙経路の列挙・フォールトインジェクション手順を含む長い 1 本）——**構成要素ごとに仕分ける**:
   - 順序制約の記述・`#803 後もこの順序制約は有効`・`env 化は #804 のスコープ` → 削除（現在形の主張が偽になる）
   - 「skip へ至る経路のうち沈黙するのは 1 本だけ」の**列挙そのもの** → 更新して残す。無条件化で沈黙経路は **0 本**になる、と書けるようにする（§6 が満たされていれば）
   - **「フォールトインジェクションはアプリを起こさずにできる」という性質 → 失われる**。代替手順（§4.3）を書く。**書かずに消すと、safety-nets.md の要求を満たす手順がリポジトリから消える。**
5. 「別プロファイルで起動するための env ハッチ」節——消費者が visual-check だけでなく両 smoke になる。**1 行足すか、あるいは何も足さない**（§5 で判定）。
6. 「CI/CD メモ」対応表——変更不要（コマンド名は変わらない）。ただし表下の（注）は smoke-startup を `-ExePath` 付きで直接呼ぶ旨を書いており、引数を増やさない限り真のまま。**G-ci-table は wrapper パス一致で判定するので、`npm run smoke:egui` の引数変更では落ちない**（＝検査は守ってくれない）。

### 1.5 `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md`

却下 3 が `-RequireResults` ゲートの存在を根拠にしており、末尾に「smoke 側の env 化は #804 のスコープ」という**生きた前方参照**を持つ。

- **却下理由の本文は書き換えない**（当時の決定文脈＝歴史。`docs/development-principles.md`「撤去（消す変更）の作法」の「ADR と設計書は当時の決定文脈ゆえ旧名のままでよい」）。
- **前方参照だけが偽になる**ので、状態行か末尾に 1〜2 行の追記を置く: 「#804 で smoke 側の env 化は完了し、`-RequireResults` は撤去された。共有ヘルパー化の再検討は #843 が引き取る」。
- G-adr-citations は ADR ファイルの**実在**しか見ないので、この腐りは検出されない。

### 1.6 触らないもの（明示）

- `docs/superpowers/plans/*.md`・`specs/*.md`（`-SeedConfig` を多数含む）——**機構的裏付けあり**: `governanceDocs()` と `headingRefDocs()` がどちらも `docs/superpowers/` を除外している（`scripts/governance-check.mjs`）。かつ歴史資料である。
- `CONTRIBUTING.md`「自動回帰 smoke が失敗する」——フラグを名指ししておらず、results 窓の trace 観測は今も真。
- `docs/architecture.md`（`%APPDATA%\Snotra` と env 上書きの記述）・`SPEC.md`「13. データ保存」——契約は変わらない。
- `AGENTS.md`「条件別チェック」の smoke 行——trace イベント名と hotkey の前提を言っており、本変更で偽にならない。
- `.claude/rules/src-tauri.md` のカテゴリ C 誘導——変わらない。

---

## 2. 間接参照の洗い出し

### 2.1 同名・別概念（同じ語が別のものを指す）

| 表層形 | 概念 1 | 概念 2 | 概念 3 |
|---|---|---|---|
| `paths` | `e2e.yml` の CI 発火トリガー | config TOML の `[paths]` / `[[paths.scan]]` セクション | `.claude/rules/*.md` frontmatter の配送 glob |
| config dir | ユーザーの実プロファイル `%APPDATA%\Snotra` | smoke-egui の使い捨てプロファイル | visual-check の使い捨てプロファイル（**別物・共有しない**） |
| seed | smoke-egui の seed（`[[paths.scan]]` 1 件あり） | visual-check の seed（scan 無し・`[visual]`/`[general]` あり） | smoke-startup の seed（**新設・両者と異なる第 3 の形**） |
| temp | trace の `.err`/`.out`（`$env:TEMP`・**移さない**） | scan ダミーの置き場（移してよい） | プロファイル（`target/` へ） |
| skip | results 検査の skip（本変更で**消す**） | `skip-ci` ラベル（無関係） | `Select-Object -Skip`（無関係） |
| `-SeedConfig` | スイッチ（消える） | seed する行為（残る・無条件化） |  |
| `-RequireResults` | スイッチ（消える） | 「results 検査が必ず走る」という要求（残る・格上げ） |  |
| 「順序」 | `e2e.yml` のステップ順（消える制約） | hotkey VK の押下順／解放逆順（無関係・触らない） | `setup_hotkey_listener` の登録順（無関係） |

**この分類の実用的帰結**: 表の各行で「概念 1 を消して概念 2 を残す」ことが変更の本体である。**表層形で grep して一括削除すると、必ず概念 2 まで消える。**

### 2.2 同概念・別名（grep に掛からない間接参照）— **本変更の主戦場**

1. **`scripts/smoke-startup.ps1` は `APPDATA` を 1 文字も含まない。** それでも実 config dir に**完全に依存**している（アプリの既定保存先を暗黙に使う）。issue が挙げる「`$env:APPDATA\Snotra` を直接見ている」という記述は smoke-egui にしか当たらず、**smoke-startup の依存は grep では発見できない**。
2. **`e2e.yml` のステップ順序制約には文字列表現が無い。** 制約はコメントで説明されているが、制約そのものは「2 つの `- name:` の並び順」という**構造**である。順序を守らせる機構が `-RequireResults` だと `e2e.yml` 自身が書いているので、**flag を消すと同時に順序も自由になる**——このカップリングは、どちらのファイルを grep しても出てこない。
3. **`snotra-core/src/config.rs` の `Config::config_dir()` / `is_first_run()`** — smoke スクリプトはこの関数名を書かないが、**変更後は機能的に依存する**。`e2e.yml` の `paths:` に `snotra-core/**` が無いことと合わせて、「依存はあるが CI トリガーには現れない」形になる（§3.4）。
4. **`launch_settings_process` の存在**（`src-tauri/src/commands/window.rs`）— smoke のどこにも現れないが、プロファイルを空にした瞬間に**新しい実行経路として現れる**（§3.2・§7）。issue にも一切書かれていない。
5. **`Get-Process snotra` は `snotra-settings` にマッチしない**（`-Name` の完全一致・ワイルドカード無し）。両 smoke の「既存インスタンスを止める」前提は、**子プロセスを射程に持たない**。これも grep 対象語（`snotra-settings`）が smoke に存在しないので見えない。
6. **`smoke-egui.ps1:78-81` ⇄ `visual-check-colors.ps1:93` の相互参照コメント**は互いの**引数名**（`-SeedConfig`）と**ゲート名**（`-RequireResults`）で相手を指している。片方の引数を消すと**もう片方のコメントが偽になる**が、visual-check 側は本変更の触る面リストに載らない。
7. **`docs/build-commands.md` C 節の #671 教訓の引用**は `-RequireResults` を「その事例を機構化したもの」として引く。flag 名の grep では当たるが、**教訓の側を残して引用先だけ差し替える**という操作は、grep 結果を機械的に処理すると出てこない。
8. **`e2e.yml:77-80` の first-run 受容**は「上の egui smoke が seed した config.toml が既に在るため」という**順序への依存として書かれている**。順序制約の削除に巻き込まれて消えやすいが、受容の中身（first-run はこの job の検証対象ではない）は生き残る（§6-D）。
9. **`ADR-config-dir-env-seam-rejected-alternatives` の却下 3** が `-RequireResults` の存在を前提に「共有ヘルパー化を却下」している。ADR は `governanceDocs` の対象だが、**この種の意味的陳腐化を検出する検査は無い**。

### 2.3 概念は同じだが**意図的に対象外**とするもの

`scripts/measure-memory.ps1` / `measure-memory-stages.ps1` / `bench-startup.ps1` / `manual-smoke.ps1` は、いずれも実 config・実索引でアプリを起動する。**同じ「検証が実データに触れる」概念だが、切り離す理由がある**:

- メモリ実測とベンチは**現実的な索引規模が測定の前提**である（seed した索引 1 件で測っても意味がない）。`visual-check-colors.ps1` の `finally` コメントが既にこの点を認めている（env を戻さないと memory_footprint の「実運用点」が使い捨てプロファイルを指す）。
- `manual-smoke.ps1` は人間が実 config で操作する記録ツールである。

**列挙して除外した**ことを書き残す（除外の記録が無いと、次の担当者が同じ列挙をやり直す）。

### 2.4 コンパイラの射程（task の前提への訂正）

task は「この変更にはコンパイラが 1 つも無い」と述べるが、**検出器は 1 つだけ在る**:

- `governance-check.mjs` の **G-stale-identifiers** は、`currentVocabulary()` に **`.ps1` を含む**ソース全文を入れ、`.claude/{skills,rules,agents}/*.md` に現れる識別子がそこに無ければ「腐り」として落とす。つまり `.claude/**` の規範文書が `-SeedConfig` / `-RequireResults` を名指ししていれば、**スクリプトから消した時点で赤くなる**。
- 実測: `.claude/**` にこの 2 語は無い（Grep で 0 件）。**よってこの検査は本変更では何も鳴らさない**——が、「鳴らなかった」ことと「検査が無い」ことは違う。
- **正確な言い方**: コンパイラを持たない面は **`docs/**`・`CONTRIBUTING.md`・`AGENTS.md`・`*.yml`・`*.ps1` 同士の相互参照コメント**である。`G-build-commands` / `G-ci-table` はコマンド名と workflow の存在しか見ないので、**引数の削除は検出しない**。

---

## 3. 設計上の選択肢と推奨

### 3.1 results 検査の要求をどう表現するか

| 案 | 内容 | 判定 |
|---|---|---|
| (a) | `-RequireResults` を撤去し、results 検査を**無条件**にする（空の `ResultsQuery` を到達不能にする） | **推奨** |
| (b) | flag を残し既定 ON | 却下。到達不能な分岐が残り、読者に「skip がありうる」と誤読させる |
| (c) | `-RequireResults` だけ残す | 却下。守る対象（skip 経路）が消えるので、常に真を検査する空の検出器になる |

**(a) に付ける独立な追記（計画に無かった論点）**: `-RequireResults` は**アプリを起動する前に判定が確定する**という性質を持ち、`docs/build-commands.md`「スモーク運用メモ」がそれを**フォールトインジェクションの手順として明文化している**（`-RequireResults -ExePath <任意の既存ファイル>` で実機に触れずに赤を出せる）。撤去はこの安価な注入点を失わせる。

`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」は、**新しい不変条件についても 1 度の実測を要求する**。よって (a) は正しいが**不完全**であり、**代替の注入手順を同じ変更で用意する義務を負う**（§4.3）。この義務は、計画の裁定文からは読み取れない。

### 3.2 smoke-startup のプロファイルを seed するか（**最重要の分岐**）

**ソースからの分岐解析**（実測ではなくコードからの導出）:

- `Config::is_first_run()` = `!config_path.exists()`、`config_path()` は env を見る `config_dir()` から導かれる（`snotra-core/src/config.rs`）。
- ⇒ **空のプロファイルを指した瞬間、毎回 first-run になる。**
- `main.rs` の `setup_first_run` は `launch_settings_process(app_handle, &["--first-run"])` を呼ぶ。ここで 2 つの枝に分かれる:

| 枝 | 条件 | 起きること |
|---|---|---|
| CI | `cargo build --release -p snotra` は snotra-settings をビルドしない ⇒ `exe_dir.join("snotra-settings.exe")` が不在 | trace `cmd:launch_settings_process:not_found` を出して `Err` ⇒ フォールバックで `indexing::start_index_build`（**`Config::default_scan_paths()` で索引を作る**）。**このイベント名は `:error` で終わらないので、smoke-startup の `*:error` フィルタからは見えない** |
| ローカル | ワークスペースをビルドしていれば `snotra-settings.exe` は隣に在る | **設定 GUI が spawn される**。`Get-Process snotra` は `snotra-settings` にマッチしないので、smoke の後片付けをすり抜けて**残る**。加えて `set_always_on_top(false)` が走り、監視スレッドが立つ |

**どちらの枝が CI で起きるかを確定させる必要は無い——seed すれば問いが消えるからである。**

**推奨: smoke-startup も最小 TOML を seed する（first-run 経路に入らせない）。** 副次的に、`e2e.yml` の first-run 受容注記が**理由を変えて生き残る**（§6-D）。

**代替案（却下）**: 「空プロファイルにして first-run カバレッジを取り戻す」——一見魅力的（#686 の順序変更で失われたカバレッジが自然に戻る）だが、(i) ローカルで毎回 GUI が湧いて残る、(ii) CI ではフォールバックが**ユーザーの既定 scan パス**を索引しに行く、(iii) `*:error` フィルタが `:not_found` を見ないので**カバレッジが増えたように見えて何も検査していない**。**「検査が増えたつもりの false green」という、この issue が消そうとしている当のもの**なので却下。

**観測可能性の補い（新規提案）**: seed は first-run を**防ぐ**が、防げたことを**示さない**。最も安価な肯定的証拠は「trace に `cmd:launch_settings_process:` で始まるイベントが 1 件も無いこと」である。これは `visual-check-colors.ps1` の `*.bin` アサーションと同じ役割（既にリポジトリで採用済みのパターンの再利用であって、新機構ではない）。**smoke-startup は既に全 trace 行を `ConvertFrom-Json` して `$events` に持っているので、追加コストは 1 行の `Where-Object` である。**

### 3.3 seed TOML の重複をどうするか

本変更で seed 亜種が **2 → 3** になる（smoke-egui / visual-check / smoke-startup）。共通化は ADR-config-dir-env-seam-rejected-alternatives の 3 が却下済みで、issue コメントの裁定により **#843 が引き取る**と明示されている。

**推奨: 共通化しない。** ただし **3 者すべてに相互参照コメントを置く**（現在は 2 者間の相互参照しかない）。3 つ目を黙って足すと、`[hotkey]`/`[appearance]`/`[paths]` が必須である根拠が**片方だけ直る**事故の面が 1.5 倍になる。

**もう一段安い代替**: smoke-startup に seed を書かず、**smoke-egui と同じ TOML 文字列を持たない**形にする——例えば `[paths]` を空にした最小 3 セクションだけ。これは実質「3 つ目」だが**最も短い**ので、#843 の共通化時に統合しやすい。

### 3.4 `e2e.yml` の `paths:` に `snotra-core/**` を足すか

**推奨: 足さない。理由を `e2e.yml` のコメントに 1 行残す。**

- 足す論拠: 本変更で smoke は env seam に機能依存する（#701 で `snotra-egui-runtime/**` を足したのと同型）。
- 足さない論拠: smoke は元々アプリ全体に依存しており、`snotra-core/**` を入れると 30 分 job の発火が大幅に増える。かつ **env seam が壊れれば `*.bin` 不在アサーション（§1.1 追加 5）が job 内で赤くなる**——検出器は job の中に在る。`paths` は「いつ回すか」の発見的規則であって完全性の主張ではない。
- **受容する残余として明示する**（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

### 3.5 env の設定タイミング

**推奨: `Start-Process` の直前に set し、直後に退避値へ戻す**（`SNOTRA_TRACE` が既に採っている形）。`finally` で戻す visual-check の形は、あちらが `cargo run` の全期間 env を要するためであり、smoke には当てはまらない。窓が短いほど「呼び出し元シェルへ漏れる」経路が減る。

---

## 4. 検証手順

### 4.1 決定的な検査（コマンド）

1. `npm run governance:check` — ガバナンス文書（`docs/build-commands.md`・ADR・`CONTRIBUTING.md`）を触るので**必須**（カテゴリ F）。G-build-commands / G-ci-table / G-heading-refs / G-adr-citations / G-stale-identifiers が回る。
2. `npm test` — `scripts/**/*.test.mjs` が対象。**`.ps1` にテストは無いので、本変更の中核は npm test の射程外**（`vitest.config.ts` の `include` が SSOT）。ここは「走ったが何も見ていない」ことを自覚する。
3. `cargo check --workspace` — Rust に触らないなら不要。触らない想定。

### 4.2 本命（カテゴリ C・実機）

**実 config を持つ開発機で、引数なしで両方が緑になること。** これが #804 の成果そのもの（従来は `-SeedConfig` が空振りして results 検査が skip されていた）。

```powershell
npm run smoke:egui -- -ExePath target/debug/snotra.exe
npm run smoke:startup -- -ExePath target/debug/snotra.exe -WaitMs 5000
```

**併せて確認する（合格の中身）**:

- 出力に `results show/hide observed` が含まれる（skip NOTE が**存在しない**こと）
- `target/smoke-egui/profile/` に `config.toml` と `index.bin` が在る
- `%APPDATA%\Snotra\config.toml` の**更新時刻が変わっていない**（実 config 非接触の直接確認）
- 実行後のシェルで `$env:SNOTRA_CONFIG_DIR` が**未設定**である
- `Get-Process snotra*` が**何も返さない**（`snotra-settings` の取り残しが無い・§3.2）

### 4.3 フォールトインジェクション（safety-nets.md の要求・**この変更の負債**）

`-RequireResults` が持っていた「実機に触れずに赤を出せる」性質が消えるので、**代替の注入点を用意し、1 度実測して手順を `docs/build-commands.md` に残す**。稼働中のガードは弱めず、複製に変異を当てる（`.claude/rules/safety-nets.md`）:

| 検査（新設） | 注入 | 期待 |
|---|---|---|
| seed 健全性 | プロファイルの `config.toml` を壊れた TOML に差し替えてから smoke を走らせる（プロファイルは使い捨てなので**実データに触れない＝安全に変異できる**） | `[config] ` 行を検出して赤 |
| env 有効性 | スクリプトの複製で `SNOTRA_CONFIG_DIR` を存在しないドライブ等へ向ける、あるいは set 行をコメントアウトした複製を走らせる | プロファイルに `*.bin` が出ず赤 |
| results 無条件化 | `-ResultsQuery ''` を渡す（§6-B の対策が入っていれば） | 起動前に throw |
| first-run 不在（採用するなら） | 複製で seed 行を落とす | `cmd:launch_settings_process:` イベントを検出して赤 |

**注**: 使い捨てプロファイルは「稼働中のガードを弱めない」規則の**適用が容易になる**面である——実 config を触らないので、変異が誰の資産も壊さない。これは本変更の副次的な便益として書き残す価値がある。

### 4.4 CI での確認

`e2e.yml` は PR で自動発火する（`scripts/smoke-egui.ps1` / `smoke-startup.ps1` / `.github/workflows/e2e.yml` がいずれも `paths:` に載っている＝**この PR は自己検証する**）。ログで確認する:

- egui smoke のログに results 検査の観測行が在る
- **順序制約が消えたことの主たる証拠は §4.2 の 2 点である**——`target/smoke-egui/profile/index.bin` が在ることと `%APPDATA%\Snotra\config.toml` の mtime が変わっていないこと。この対が「2 つのプロファイルが交わらない」ことを示し、それこそが順序制約の守っていた当のものである。
- **ステップの順序を入れ替えた commit を作って緑を見るのは任意**（§8.7 の緊張はここで解ける）。入れ替えた 1 回の緑は「順序制約が死んだ」とも「seed がたまたまその回は噛まなかった」とも読めるので、**単独では判別力を持たない**。上の 2 点が緑なら追加情報は小さい。

---

## 5. YAGNI 判定

**やりすぎと判定するもの**:

1. **seed TOML の共有ヘルパー化** — ADR で却下済み、issue コメントで #843 へ割り当て済み。本 PR でやると爆風半径が CI ゲートに広がる。**相互参照コメント 3 点で足りる。**
2. **`docs/superpowers/**` の歴史文書の一括更新** — `governanceDocs()` / `headingRefDocs()` が明示的に除外しており（機構的裏付け）、`development-principles.md` が「ADR と設計書は当時の決定文脈ゆえ旧名のままでよい」と規定する。**触らない。**
3. **`e2e.yml` の `paths:` へ `snotra-core/**` 追加** — §3.4。受容する残余として 1 行書くだけにする。
4. **smoke 用の Pester テスト新設** — `vitest.config.ts` の include に `.ps1` は無く、PowerShell テスト機構がリポジトリに無い。**新しい検査機構を 1 つ立てる費用**が本 issue の便益を超える。#843 が Pester を受け入れ条件に持つ（issue コメント）ので、そちらへ寄せる。
5. **`-ResultsQuery` パラメータ自体の撤去** — 開発者が別プロファイル・別索引で試す口として意味が残る。**空を禁止すれば沈黙経路は閉じる**ので、パラメータごと消す必要はない。（ただし「seed が固定なら常に "z" で足りる」という反論もあり得る。§8 に自信度を書く。）
6. **`smoke-egui` と `smoke-startup` でプロファイルを共有する** — ディスク節約にもならず、順序依存を別の形で復活させる。
7. **`$env:TEMP` の trace `.err`/`.out` を `target/` へ移す** — 「temp を target へ」という表層の連想で巻き込みやすいが、trace ログはプロファイルとは別概念（§2.1）。移す理由が無く、`Remove-Item` の対象や失敗時の証拠出力パスまで書き換える差分が増える。
8. **`measure-memory*.ps1` / `bench-startup.ps1` の env 化** — §2.3。測定の前提が実索引なので、むしろ**やってはならない**。

**やりすぎに見えて必要なもの**（逆方向の判定）:

- **seed 健全性検査と env 有効性検査の 2 つ**。「seed するのだから読めるはず」は副作用の不在で成功を測る形であり、ADR の却下 5 が既に一度否定している。**skip NOTE を消して沈黙経路を 0 にすると主張する以上、これらが無いと主張が嘘になる。**
- **smoke-startup の seed**（§3.2）。無しで済ませると first-run 経路が新規に開く。

---

## 6. 消し忘れると沈黙する箇所（名指し）

**前提**: 本変更は「skip を大声で報告していた仕組み」を「skip がそもそも無い仕組み」へ置き換える。**報告器を消した後に残る skip 経路は、すべて沈黙する。**

### A. プロファイルの残骸を消し忘れる（両 smoke）

前回実行の `index.bin` / `config.toml.bak` が残っていると、**新設した 2 つの検査（seed 健全性・env 有効性）が古いファイルで空振り合格する**。`visual-check-colors.ps1:83-87` が同じ理由で `Remove-Item` を持っている——**参照実装からコピーするときに「掃除だから後回し」と判断すると落ちる**。`.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は、その状態に到達する全経路を列挙する」の直撃事例。

### B. `-ResultsQuery ''` の沈黙 skip

`param([string]$ResultsQuery = "z")` にして `-RequireResults` の guard と末尾 NOTE を消すと、**`-ResultsQuery ''` を渡した場合だけ results 検査が完全に沈黙して skip される**（`if (... -not [string]::IsNullOrEmpty($ResultsQuery))` が false になり、成功メッセージは分岐が消えて「passed」と出る）。

対策: `[ValidateNotNullOrEmpty()]` を付けるか、無条件の `if ([string]::IsNullOrEmpty($ResultsQuery)) { throw ... }` を置く（= `-RequireResults` の guard を**削除ではなく無条件化**する形）。

### C. `$resultsChecked` を残す

常に `$true` になる変数と、それを読む 3 つの `if` を残すと、**分岐は死ぬが読者には生きて見える**——「skip がありうる」という誤った不変条件を後続の変更者へ伝える。`egui_results:hide` の対検査と orphan 検査は**無条件へ**書き換える。

### D. `e2e.yml:77-80` の first-run 受容を巻き込んで消す

順序制約のコメントと物理的に隣接しているので一緒に消えやすい。**受容そのものは生き残る**（seed によって first-run に入らない・first-run はこの job の検証対象ではない）。理由が「前のステップが config を作るから」→「自分で seed するから」に変わるだけである。消すと、**なぜ first-run が検査されていないのかを誰も説明できない状態**になる。

### E. `e2e.yml` から引数を消して script のパラメータを残す（**方向によって騒がしさが違う**）

- **script からパラメータを消し、yml に残す → 大声で落ちる**（`pwsh`: "A parameter cannot be found that matches parameter name 'SeedConfig'"）。安全側。
- **yml から引数を消し、script にパラメータを残す → 完全に沈黙する**（未指定の `[switch]` は `$false`＝ seed もせず、旧コードなら results 検査が黙って skip される）。**危険な向きはこちら。**

⇒ **実装順序の指針**: `e2e.yml` を先に直してはならない。**script を先に完成させ、yml は最後に合わせる。**

### F. `visual-check-colors.ps1:93` の相互参照コメント

`-SeedConfig` を名指ししているので、flag を消すと**存在しない引数を指す**。visual-check は本 issue の触る面リストに無いため、**チェックリストからも漏れやすい**。

### G. `docs/build-commands.md` の「フォールトインジェクション可能」の記述

`-RequireResults` の bullet ごと消すと、**safety-nets.md が要求する実測手順がリポジトリから消える**。代替手順（§4.3）を書かずに消してはならない。

### H. 取り残された `snotra-settings` が smoke-egui の失敗を**別の欠陥に見せかける**

`Get-Process snotra` は `snotra-settings` にマッチしない（§2.2-5）。前回実行や first-run で設定 GUI が残っていると、smoke-egui のキー注入は**フォーカスを持つそちらへ飛ぶ**。症状は `egui_results:show not observed ... after typing 'z'` ——**製品の回帰にしか見えない**。沈黙ではなく**誤帰属**だが、対策が同じ（seed で spawn させない ＋ `cmd:launch_settings_process:` の不在検査）なのでここに置く。§7 の手続き 4・5 と同じ対策で閉じる。

### I. `smoke-egui.ps1` のヘッダコメント（`:52-54`）

`param()` の説明コメント（`:13-22`）ばかりに目が行き、**ファイル冒頭のヘッダ散文に書かれた `-SeedConfig` の説明**が残る。同一ファイル内に 2 箇所ある。

---

## 7. 最大の罠（1 つ）

> **空のプロファイルを指した瞬間、アプリは first-run 経路に入り `snotra-settings` を spawn しようとする。この経路は issue にも参照実装にも現れず、CI とローカルで別の形で現れ、CI 側の症状は既存の `*:error` アサーションから原理的に見えない。**

**なぜこれが最大か**:

- §6-A（残骸による空振り）と「相対パスの `SNOTRA_CONFIG_DIR`」は、どちらも `visual-check-colors.ps1` が既に解決済みで、参照実装をなぞれば**ただで付いてくる**。
- §6-B（残る沈黙 skip）は撤去作業そのものの直接的帰結なので、計画にも独立導出にも載りやすい。
- first-run だけは、**「config を seed する」という発想の外側**にある。issue は「実 config の有無に依存する形をやめる」と書くが、**実 config が「在る」ことに依存していたのは smoke-egui の seed 判定だけでなく、`is_first_run()` もだった**——後者はコードにしか書かれていない。
- そして CI での症状は `cmd:launch_settings_process:not_found` という trace イベントで、**`*:error` で終わらないため smoke-startup のフィルタが構造的に見落とす**。つまり「壊れたのに緑」になる。これは #686・#690 が繰り返し潰してきた false green と同じ族である。

**避ける手続き（順に実行する）**:

1. **プロファイルを空にする前に、`Config::is_first_run()` の定義を読む。** `!config_path.exists()` であり、`config_path()` は `config_dir()` 経由で env を見る——**プロファイル分離は first-run 判定を必ず動かす**、という一行を計画に書く。
2. **`is_first_run` の唯一の消費者を追う。** `src-tauri/src/main.rs` の `let is_first_run = Config::is_first_run();` → `setup_first_run` → `launch_settings_process(&["--first-run"])`。**呼び出し先のソースを読むまで、経路を「無害」と判定しない**（`docs/development-principles.md`「受け手のソースを読むまでは、渡した値が届いたことにならない」）。
3. **両 smoke に seed を置き、first-run 経路へ入らせない。** smoke-egui は元々 seed するので追加費用ゼロ。smoke-startup は新規。
4. **防いだことを観測可能にする。** trace に `cmd:launch_settings_process:` で始まるイベントが 0 件であることを検査に加える（`*:error` フィルタでは見えないため、**別の述語が要る**）。
5. **後片付けの射程を確認する。** `Get-Process snotra` は `snotra-settings` を含まない。seed で spawn を防ぐのが本筋だが、実測時に `Get-Process snotra*` で取り残しが無いことを 1 度確かめる（§4.2）。
6. **ローカルと CI の両方で走らせる。** この罠は「ローカルでは GUI が湧く／CI では黙って別経路を通る」と**環境で症状が変わる**ので、片方だけの緑を根拠にしない。

---

## 8. 自信の低い箇所

1. **§0 の汚染。** 最も重要な留保。`workspace/plan.md` / `plan-snapshot.md` の本文が grep 結果として目に入っており、**設計判断 (a) の裁定・却下案・チェックリストの過半を見た後で本書を書いている**。§1 の変更集合と §3.1 の推奨は、独立の証拠として扱えない。**独立性が保たれているのは §2（間接参照）・§3.2（first-run）・§3.4・§4.3・§6・§7** である（これらに対応する内容は見えた行に無かった）。次回同種の作業では、grep の対象から `workspace/` を除外するパターンを最初のコマンドに入れるべきだった。
2. **`workspace/plan-review/ci-workflow.md` を読んでいない**（別レビュアの出力と推定）。本書と統合していないので、重複・矛盾の解消は呼び出し側の仕事。
3. **CI で first-run 経路がどちらの枝を通るかは実測していない。** `cargo build --release -p snotra` が snotra-settings を成果物に含めないという読みはコマンドから導いた推論で、rust-cache が復元する `target/` の中身までは確認していない。**seed すれば問い自体が消える**ので推奨は変わらないが、「CI ではこう動く」と断定して書かないこと。
4. **未確認なのは「フォールバック側が**追加で**何かを出すか」だけである。** `cmd:launch_settings_process:not_found` が CI 側の枝で発火することは `window.rs` を直接読んで確定しており、ローカル側の枝（GUI が spawn され `Get-Process snotra` の射程外に残る）も同様である。**§7 の罠はこの 2 点だけで成立する。** `indexing::start_index_build` のフォールバックが `*:error` を出すかは追っていない——出れば CI に第 2 の信号が増えるだけで、**ローカルの取り残しと seed の必要性は変わらない**。
5. **`-ResultsQuery` を残すか消すか**（§5 の 5）。seed が固定なら常に `"z"` で足り、パラメータは「使われない口」になる。残す判断は「別索引で試したい開発者」という**仮想の利用者**に依っており、YAGNI 的には消す側もあり得る。どちらでも沈黙経路は §6-B の対策で閉じる。
6. **seed TOML の 3 つ目を作ることの是非**（§3.3）。#843 が共通化を引き取る前提での「短い 3 つ目」を推したが、「smoke-startup では seed せず first-run を受け入れる」案を完全に排除できたわけではない——§3.2 の (i)(ii)(iii) は費用の議論であって不可能性の証明ではない。
7. **`e2e.yml` のステップ順序を実験的に入れ替えて緑を見る**（§4.4）という検証は、CI 実行 1 本ぶんの費用がかかる。順序制約が消えたことの直接証拠としては最良だが、「seed が自分のプロファイルへ行くのだから当然」と論証で済ませる判断もあり得る。**論証で済ませるなら、safety-nets.md が求める「期待と測定結果は違う」に反する**という緊張は残る。
8. **`docs/build-commands.md` の該当箇所を節と主語で指した**（行番号は本書に書いていない）が、実際の編集では複数 bullet にまたがる。**編集後に `-SeedConfig` / `-RequireResults` / `%APPDATA%` を再 grep して 0 件（歴史文書と `SPEC.md`/`architecture.md` の正当な言及を除く）を確認する**手順を、実装者のチェックリストに入れること。
