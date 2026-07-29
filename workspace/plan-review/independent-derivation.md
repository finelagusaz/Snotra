# #804 独立再導出 — smoke スクリプトの `SNOTRA_CONFIG_DIR` 化

**担当**: 独立再導出（変更集合を issue とコードだけから導く）
**日付**: 2026-07-29
**根拠**: `gh issue view 804` / `gh issue view 804 --comments`、および以下の一次読解のみ

---

## 0. 独立性の開示

- `workspace/plan.md` / `workspace/research.md` / `workspace/plan-snapshot.md` / `workspace/plan-review/` の他ファイルは**一切読んでいない**。
- grep は `--include` とディレクトリ明示で行い、`workspace/` を明示除外した。ripgrep 側でも `!workspace/**` を指定した。**`workspace/` 配下の行は 1 行も観測していない**。
- ただし `.superpowers/sdd/2026-07-25-pr-a-…/`（#671 サイクル PR A の実装記録）と `docs/superpowers/plans/…` の行は grep 結果に混ざった。これらは**過去サイクルのリポジトリ内成果物**であって本レビューの他レビュア出力ではない。内容は「現行実装がどう作られたか」の裏取りにのみ使い、変更集合の導出は現行ファイルの通読に基づく。
- **本レビューは静的読解のみである。スクリプトを 1 度も実行していない**（§9 に未検証項目を列挙）。

読んだ一次資料: `scripts/smoke-egui.ps1`（全文）/ `scripts/smoke-startup.ps1`（全文）/ `scripts/visual-check-colors.ps1`（全文）/ `.github/workflows/e2e.yml`（全文）/ `.github/workflows/release.yml`（該当部）/ `docs/build-commands.md`（カテゴリ C・D・F・スモーク運用メモ・CI/CD メモ）/ `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md`（全文）/ `docs/development-principles.md`（撤去の作法・列挙の完全性）/ `.claude/rules/safety-nets.md`（全文）/ `AGENTS.md` / `CLAUDE.md` / `snotra-core/src/config.rs`（`config_dir` / `load_from_dir_reporting` / `Default` 群）/ `src-tauri/src/main.rs`（`main` / `setup_first_run`）/ `src-tauri/src/commands/window.rs`（`launch_settings_process`）/ `src-tauri/src/platform/hotkey.rs`（`hotkey:registered`）/ `scripts/governance-check.mjs`（検査見出し・G-stale-identifiers）/ `package.json` / `vitest.config.ts` / `.gitignore`

---

## 1. 変更集合（ファイル → 箇所 → 何をするか）

### A. `scripts/smoke-egui.ps1` — 主戦場

| 箇所（シンボル / 現行行） | 何をするか |
|---|---|
| `param()` `[switch]$SeedConfig`（:5） | **削除**。プロファイルを所有する以上「seed するかどうか」の選択肢が無くなる |
| `param()` `[string]$ResultsQuery = ""`（:13-17） | **削除**（推奨。§5 O4 に代案）。索引は常にこちらが作るので、外部から当てる文字を渡す動機が消える |
| `param()` `[switch]$RequireResults`（:19-22） | **削除**。守っていた性質（results 検査が走る）は分岐の不在で構造的に保証される |
| 冒頭の説明コメント（:43-55、特に :52-54 の `-SeedConfig` 説明） | **書き直し**。「使い捨てプロファイルを毎回作って seed し、実 config は読みも書きもしない」へ。first-run 回避の理由（`snotra-settings --first-run` の spawn がフォーカスを奪う）は**残す**——seed し続ける理由そのものだから |
| `$seededNow = $false` / `if ($SeedConfig) { … } else { … }`（:64-111） | **無条件 seed へ置換**。`$env:APPDATA` の組み立て（:66-67）を消し、`$profileDir` へ書く。`$seededNow` は削除 |
| seed 先ディレクトリの新設 | `$smokeRoot = Join-Path $PSScriptRoot '..\target\smoke\egui'`、`$profileDir = Join-Path $smokeRoot 'profile'`、`$scanDir = Join-Path $smokeRoot 'scan'`。**起動前に `$smokeRoot` を丸ごと `Remove-Item -Recurse -Force`**（§8 罠 2） |
| `$scanDir = Join-Path $env:TEMP "snotra_smoke_scan"`（:73-76） | `$smokeRoot` の**兄弟**へ移す（プロファイル配下に入れない・§5 O1 注記）。ダミー名は `"${ResultsQuery}snotrasmoke.exe"` の形で 1 か所から導き、クエリ文字とファイル名のドリフトを消す |
| seed TOML（:90-104）と :78-89 のコメント | TOML 本体は**そのまま**（`[hotkey]` / `[appearance]` / `[paths]` の必須セクション根拠は不変）。:78-81 の相互参照コメントのうち「共通ヘルパーにしないのは、この smoke が e2e.yml の `-RequireResults` ゲートに載る CI 経路だからである」は**偽になる**ので #843 を指す文へ差し替える |
| `$env:SNOTRA_CONFIG_DIR` の設定 | `$profileFull = (Resolve-Path $profileDir).Path`（**`New-Item` の後**）→ `$savedCfgDirEnv = $env:SNOTRA_CONFIG_DIR` を退避 → 代入 → **`finally` で復元**（`Remove-Item Env:… -ErrorAction SilentlyContinue` は未設定時のため必須）。既存の `SNOTRA_TRACE` の退避・復元イディオム（:211-219）に合わせる |
| `$ResultsQuery` の既定導出（:113-116） | **削除**（クエリは定数化） |
| `-RequireResults` ガード（:118-134・throw ブロック） | **全削除**。`$env:APPDATA` を含む throw 文言（:129-130）ごと消える |
| `Get-LetterVk`（:145-153） | 呼び出しが定数 1 つだけになるので throw 分岐が到達不能になる。**関数を消して `[byte][int][char]'Z'` を直接使う**（推奨）。消すなら `docs/build-commands.md` の「クエリが A-Z 単字でない」＝鳴る経路の記述も同時に消す |
| `$resultsChecked`（:296, :372, :374, :422, :432, :477） | **6 か所すべて削除**し、results 検査・`egui_results:hide` 検査・orphan 検査を**無条件**にする |
| results 検査の条件（:373 `-not [string]::IsNullOrEmpty($ResultsQuery)`） | `if ($failures.Count -eq 0) { … }` へ |
| **新規**: seed 健全性検査 | `visual-check-colors.ps1` の `Test-SeedHealth` と同型。`$errPath` に `[config] ` で始まる行があれば失敗。**stderr と trace は同じ `$errPath` に落ちている**ので追加の配管は不要。根拠: `load_from_dir_reporting` の `[config] ` eprintln は parse 失敗 / 非 UTF-8 / read 失敗の**全 arm に在り、NotFound（first-run）arm と成功 arm には無い**。常時 seed する以上、`[config] ` の出現は「seed が読まれなかった」と同値 |
| **新規**: env が効いた肯定的証拠 | 実行後に `$profileDir` 配下へ `*.bin`（実際は `index.bin`）が**生成されている**こと。プロファイルを起動前に wipe しているので「存在＝今回生成された」が成立する。非フレーク性の根拠: `main()` の索引ロード／保存は `tauri::Builder…run()` **より前**に走り、`hotkey:registered` はその後の setup で出る。よって「trace が 1 行でも出た ⇒ index.bin は書かれ済み」で、`Stop-Process -Force` の打ち切りとは競合しない |
| 末尾の成功メッセージ（:476-482） | 2 分岐を 1 本に。`NOTE: results window coverage was SKIPPED …`（`%APPDATA%` と `-SeedConfig` を含む）を**削除** |

### B. `scripts/smoke-startup.ps1`

| 箇所 | 何をするか |
|---|---|
| ヘッダ（`param()` の下・:25-28 付近） | **新規コメント**: 使い捨てプロファイルを使う理由と、**空のままにせず seed する理由**（§4 の first-run 連鎖）を書く |
| ループ前 | `$smokeRoot = Join-Path $PSScriptRoot '..\target\smoke\startup'` を wipe → `profile` / `scan` を作成 → 最小 TOML を seed。**`smoke-egui` の seed をそのまま（逐語で）使ってよい**——`[[paths.scan]]` のダミー 1 件は §9-3 の未実測（scan 0 件で `index.bin` が出るか）を避けるために要り、`[general]` を足す必要は**無い**: startup smoke のアサーションは「trace ≥ 1 件」と「`*:error` 不在」だけで**窓の可視性に依存しない**ので、既定の `show_on_startup = false`（hidden 起動）のままでよい |
| ループ前 | `$env:SNOTRA_CONFIG_DIR` を退避 → 代入 |
| **全体を `try { … } finally { … }` で包む** | 現行は try/finally が無く、途中で throw すると env が呼び出し元セッションに残る（`npm run` 経由なら子プロセスだが、`pwsh -File` 直叩きでは漏れる）。`finally` で `SNOTRA_CONFIG_DIR` と `SNOTRA_TRACE` を復元 |
| 各 run の判定 | `[config] ` 行の不在を検査（seed 健全性・A と同型） |
| ループ後 | `$profileDir` に `*.bin` が生成されていることを検査（env の肯定的証拠） |
| **`param()` は増やさない** | `release.yml` が `-ExePath` のみで呼ぶ。必須パラメータを足すとリリースパイプラインがタグを切るまで沈黙して壊れる（§8 罠 10） |

### C. `.github/workflows/e2e.yml`

| 箇所 | 何をするか |
|---|---|
| `Run egui smoke`（:74-75） | 引数から `-SeedConfig -RequireResults` を**削除** → `npm run smoke:egui -- -ExePath target/release/snotra.exe` |
| :65-73 のコメント | **順序制約の段落（:67-73）を全削除**。残すのは「flip 済み＝既定が egui であること自体が検証対象」（:65）だけ。順序が自由になった理由（各 smoke が自分の使い捨てプロファイルを持つ）を 1 文で置く |
| :77-80 のコメント | 「上の egui smoke が seed した config.toml が既に在るため…」は**偽になる**。「自分のプロファイルを seed するので first-run 経路は通らない」へ差し替え。first-run が本 job の検証対象でないことの受容は**残す** |
| `paths:`（:13-27） | **変更不要**（`scripts/smoke-*.ps1` が既に載っている） |
| ステップの順序 | **入れ替えない**（§5 O7）。順不同であることは手元の逆順実行で測る |

### D. `docs/build-commands.md`

| 箇所（内容で同定） | 何をするか |
|---|---|
| カテゴリ C の入れ子バレット「**CI に検証を委ねるなら…**」（#671 の 5 run 空振り事例） | **事例は残し、帰属だけ差し替える**。「この 1 事例は `-RequireResults` が機構化した（#686・下記）」→「この 1 事例は検証プロファイルの分離が構造的に解消した（#804・skip 分岐そのものが無い）」。事例を消すと、この事例より長生きする一般則（「緑」≠「検査が走った」）から根拠が抜ける |
| スモーク運用メモ「`scripts/smoke-startup.ps1` は…」バレット | 使い捨てプロファイル（seed 込み）で走ることを 1 文追加 |
| スモーク運用メモ「`scripts/smoke-egui.ps1` は egui 経路の…」バレット中の `-SeedConfig` 説明 | 「毎回プロファイルを作り直して seed する」へ書き換え。「既存 config は上書きしない」は消える |
| スモーク運用メモ「results 窓の表示も検査する」バレットの「索引内容を制御できるとき**だけ**」「どちらも無ければ自動的に skip」 | **skip の記述を全削除**。常に走る旨と、seed した索引 1 件に当たる固定クエリであることへ |
| スモーク運用メモ「**`-RequireResults` は skip を失敗に変える…**」バレット全体 | **削除**し、短い後継を置く: プロファイル分離により skip 経路が存在しないこと／新しい 2 つの検査（`[config] ` 行の不在・プロファイルへの `*.bin` 生成）／それぞれのフォールトインジェクション手順（§6 FI-1/2/3）。**「`e2e.yml` では egui smoke を startup smoke より前に置くこと」と「#803 の後もこの順序制約は有効である」は消える** |
| 「別プロファイルで起動するための env ハッチ（`SNOTRA_CONFIG_DIR`）」節 | 消費者の列挙に smoke 2 本を足す（現状 `check:colors` だけが言及されている） |
| カテゴリ D「`[visual]` の色を変える変更…」の `SNOTRA_CONFIG_DIR` 関連バレット | **変更不要**（`check:colors` 固有の記述） |

面積 ratchet（G-area-budget）は「常時ロード規範」と「rules 合計」の二面で、`docs/**` は母集団外。差し引きは純減なのでどちらにせよ問題にならない。

### E. `scripts/visual-check-colors.ps1`

- `:93`「`scripts/smoke-egui.ps1` の `-SeedConfig` が同型の seed を持つ」 → **`-SeedConfig` という名前が消える**ので参照が空振りする。「`scripts/smoke-egui.ps1` の seed」等へ。
- `:94-95`「あちらは `[[paths.scan]]` にダミーを 1 件置くが、こちらは置かない」は**真のまま**（残す）。
- `:71-79` の single-instance 注記は真のまま。

### F. 触らない（歴史的記述として真のまま）

- `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md`（却下 3 が `-RequireResults` を名指すが、2026-07-28 時点の決定文脈）
- `docs/superpowers/plans/2026-07-25-*.md`（3 本）・`.superpowers/sdd/2026-07-25-pr-a-…/`（当時の計画・実装記録）
- `CONTRIBUTING.md:92`（フラグを名指していない。results 被覆の記述はむしろ**より真になる**）
- `.claude/rules/src-tauri.md:28`・`docs/architecture.md:104`・`SPEC.md` §13（フラグに依存しない）

> `docs/development-principles.md`「撤去（消す変更）の作法」の仕分けに従った: **ADR と設計書は当時の決定文脈ゆえ旧名のままでよく、生きた文書（`docs/build-commands.md` / `e2e.yml` のコメント / 相互参照コメント）は現在形の主張が偽になるので直す。**

---

## 2. 間接参照の洗い出し

### 同名・別概念

| 表層形 | 概念 1 | 概念 2 |
|---|---|---|
| 「seed」 | `smoke-egui` の seed（`[[paths.scan]]` あり・results 窓を出す） | `visual-check-colors` の seed（scan あり・**色**用。`[visual]` / `[general]` を持つ） |
| 「プロファイル」 | `target/visual-check/profile`（既存） | 本 PR で作る `target/smoke/*/profile` |
| `config.toml` | 実ユーザーの `%APPDATA%\Snotra\config.toml` | プロファイル内の seed |
| `*.bin` | 実プロファイルのユーザーデータ | 検証プロファイルの「env が効いた証拠」 |
| `.claude/`（既知の二概念・#500） | 本 PR とは無関係だが、`scripts/` にも同型がある: `scripts/*.mjs`（`npm test` が検査する）と `scripts/*.ps1`（**どの自動検査も見ない**）。§8 罠 6 |

### 同概念・別名（当の識別子を grep しても届かない）

- **順序制約**: `e2e.yml:67-73` は「startup smoke より前に置く」という**日本語の散文**で書かれており、`-RequireResults` の grep では :72 しか当たらない。`docs/build-commands.md` の該当バレットも同様。
- **skip 条件**: 「索引内容を制御できるとき」「no controlled index」「seed が不成立」「開発機では通常 config が存在する」——いずれも `-SeedConfig` を名指さずに同じ条件を指す。
- **first-run 回避の理由**: 「`snotra-settings --first-run` の spawn がフォーカスを奪う」。`-SeedConfig` を消しても**この理由は消えない**（seed し続ける根拠）。消し過ぎに注意。
- **実 config を直接見る形**: `$env:APPDATA` は `smoke-egui.ps1` に **2 か所**（:66 の seed 先、:129 の throw 文言）＋ 末尾 NOTE の `%APPDATA%/Snotra/config.toml` 文字列で計 3 か所。
- **後始末の第 2 の場所**: `$env:TEMP\snotra_smoke_scan`（ダミー exe）と `$env:TEMP\snotra_smoke_egui.{err,out}`。前者を `target/` へ移すなら、旧ディレクトリは誰も掃除しない（無害だが列挙に入れる）。
- **`-RequireResults` が「順序制約を守らせている」という主張**: `e2e.yml:72-73` と `docs/build-commands.md` の 2 か所に**同じ主張が二重に**書かれている（`AGENTS.md`「派生コピー同士の一致を完全性の証拠にしない」）。片方だけ直す事故が起きやすい。

### 機械検査の射程外（＝ドリフトが沈黙する）

- `governance-check.mjs` の **G-stale-identifiers** の母集団は `.claude/{skills,rules,agents}/**.md` に限られ、判定対象は**バッククォート内の camelCase 識別子**のみ。`-SeedConfig` / `-RequireResults` は camelCase でなく、`docs/**` は母集団外。**docs に残った腐りを捕まえる検査は存在しない。**
- **G-references** はパスの実在しか見ない（`scripts/smoke-egui.ps1` は存在し続けるので緑）。
- **G-build-commands** は npm script 名と `cargo test -p <crate>` の実在のみ。フラグは見ない。
- `.claude/rules/safety-nets.md` の `paths` は `scripts/*.mjs` までで **`scripts/*.ps1` を含まない** → 本 PR では rules が自動配送されない（#843 のスコープ・§7）。
- PostToolUse hook は `scripts/` 配下の非 TS ファイルに検査を割り当てない → **`.ps1` 編集時の沈黙は「何も走らなかった」**。

---

## 3. 消し忘れると沈黙する箇所（名指し）

「沈黙する」＝ 消し忘れても赤くならず、誤情報／死んだ分岐として残るもの。

1. **`$resultsChecked` の 6 か所**（:296, :372, :374, :422, :432, :477）。1 つでも残ると「results を検査しない場合がある」という前提が構造に残り、後の編集者がその分岐を復活させうる。特に :422（`egui_results:hide` の対検査）と :432（orphan 検出）は**検査を条件付きにしている当の場所**である。
2. **末尾の skip NOTE**（:476-482）。到達不能になるだけで実行時は無音。残ると「`-SeedConfig` を渡せ」「`%APPDATA%/Snotra/config.toml` を消せ」という**存在しない手順**を利用者へ配り続ける。
3. **`docs/build-commands.md` の 4 か所**（カテゴリ C の入れ子バレット／smoke-egui バレット／results 被覆バレット／`-RequireResults` バレット）。**どの機械検査にも掛からない**（§2）。ここが本 PR で最も沈黙しやすい。
4. **`e2e.yml:67-73` と `:77-80` のコメント**。引数（`:75`）の消し忘れは pwsh が未知パラメータで**エラーになって鳴る**が、コメントの消し忘れは鳴らない。**この非対称を実装者に明示すること**——「CI が緑だから消し忘れは無い」は成立しない。
5. **`scripts/visual-check-colors.ps1:93` の相互参照**。「片方だけ直さないこと」を担保しているのが**コメントによる相互参照だけ**なので、片側の名前が消えた瞬間にリンクが切れる（次に seed を直す人が対を見つけられない）。
6. **`Get-LetterVk`**（:146-153）。呼び出しを定数化して関数を残すと、PowerShell は未使用関数を咎めない。同時に `docs/build-commands.md` の「クエリが A-Z 単字でない（`Get-LetterVk` が throw）」という**鳴る経路の列挙**も腐る。
7. **`$env:TEMP\snotra_smoke_scan`**。scan dir を移すなら、旧パスに触れる記述（`smoke-egui.ps1` :73-76 のコメント）を残さない。
8. **最大**: `-RequireResults` が守っていた性質（「results 検査が必ず走る」）を、**別の何かが守っていることを確かめずに削除すること**。後継は「分岐が存在しない」という構造であり、その成立条件は「seed が常に成功し、索引が常に 1 件以上ある」である。したがって **§1 の 2 つの新規検査（`[config] ` 行の不在・`*.bin` の生成）を同じ変更で入れないと、ガードだけが消えた状態になる。** `.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は、その状態に到達する全経路を列挙する」の逆向き（意味を与えていた状態を消す）に当たる。

---

## 4. この変更で新たに踏みうる製品側の経路（コードで確認）

### 4.1 「検証プロファイルを空にして起動する」と何が起きるか（＝ seed を省く案の検算）

`src-tauri/src/main.rs` の `main()` を上から追った実測（読解）:

1. `Config::is_first_run()` → `config_path()` が `SNOTRA_CONFIG_DIR/config.toml` を返し、不在なので **true**。
2. `Config::load_reporting()` → `load_from_dir_reporting` の `NotFound` arm → `Config::default()` を `save_to_dir(dir)`。**`save_to_dir` は `fs::create_dir_all` を先に呼ぶ**ので、ディレクトリを作らずに env を渡しても config.toml は作られる。
3. **書かれる既定は空ではない**。`impl Default for Config` は `paths: PathsConfig { scan: Self::default_scan_paths() }` で、`default_scan_paths()` は共通/ユーザーのスタートメニューと**デスクトップ（`.lnk`）**を返す。
   - **紛らわしい非対称を明記する**: `PathsConfig.scan` フィールドには `#[serde(default)]` が付いているので、seed TOML に空の `[paths]` ヘッダだけを書いた場合は `Vec::new()`（＝ scan 0 件）になる。`visual-check-colors.ps1:96-97` のコメント（「`default_scan_paths()` には落ちない」）はこの**デシリアライズ経路**の話であって、`Default` impl の話ではない。**両者は別物**であり、first-run はデシリアライズを通らない（`Default` impl を直接使う）ので**スタートメニューが入る**。
4. `is_first_run` ゆえ索引は作らず `initial_indexing = true`、`entries` は空。
5. setup の中で `setup_first_run(&app_handle, true)` → `commands::launch_settings_process(app, &["--first-run"])`。ここで**分岐が 2 本**:
   - **分岐 A: `snotra-settings.exe` が `snotra.exe` の隣にある** → `Command::spawn` → trace `cmd:launch_settings_process:spawned` → main / results 窓の `alwaysOnTop` を一時解除 → **設定 GUI が前面に出てフォーカスを奪う**。**smoke スクリプトが kill するのは `Get-Process snotra` だけ**なので、**この子プロセスは残る**。`smoke-startup.ps1` は 5 回起動するので **5 個残る**。
     - 該当する実行環境: 開発機（`target/debug/snotra-settings.exe` はワークスペースビルドでほぼ確実に存在）と、**`.github/workflows/release.yml`**（`snotra-settings.exe` を明示的にビルドして `src-tauri/binaries` へコピーした後で `smoke-startup.ps1` を走らせる）。**リリースパイプラインが最悪のケースである。**
   - **分岐 B: 隣に無い** → trace `cmd:launch_settings_process:not_found` → `Err` → `indexing::start_index_build(app_handle)` → **既定 scan（スタートメニュー全体＋デスクトップ）を索引**。
     - 該当: `e2e.yml`（`cargo build --release -p snotra` のみで、`snotra-settings` をビルドしない）。索引時間と `index.bin` の肥大を CI に持ち込む。
6. **どちらの分岐も `smoke-startup.ps1` を赤くしない**。アサーションは「trace が 1 件以上」と「`*:error` が無い」だけで、`cmd:launch_settings_process:not_found` は `*:error` に一致しない。

**結論: 検証プロファイルを空のままにする案は採らない。両スクリプトとも seed する。** 最も強い根拠は開発機ではなく **release.yml**（残存 GUI プロセス 5 個）である。

### 4.2 seed した場合に踏む経路（＝推奨案）

- first-run に**ならない**（config.toml が在る）。現行 CI（egui smoke が seed した後に startup smoke が走る形）と同じ経路。
- `[general]` を書かないので `show_on_startup = false`（hidden 起動）／`show_tray_icon = true`（トレイが出る）。**現行と同一**。
- 索引は seed した 1 ファイルのみ。`index.bin` はキャッシュミス時に書かれ、これが env の肯定的証拠になる。
- `window.bin` / `history.bin` は不在 → 既定位置・履歴なしで起動。**現行の CI と同じ**（開発機での実行だけが「実ユーザーの窓位置・履歴を使わなくなる」という差分を持つ）。

### 4.3 **新しい沈黙経路**（seed が壊れた場合）— 対策必須

`load_from_dir_reporting` の parse 失敗 arm は `.bak` 退避 + **in-memory の `Config::default()`** で続行する（保存はしない）。ここで:

- 既定 hotkey は **Alt+Q** で、seed TOML の hotkey と**同一**。よって `hotkey:registered` も `egui_show:done` も**普通に出る**（ここまでは確実に素通りする）。
- 既定 scan は**スタートメニュー**であり、seed が指すダミー（`target/smoke/egui/scan/zsnotrasmoke.exe`）は**索引に入らない**。したがって `z` の打鍵が `egui_results:show` を出すかどうかは、**その機械のスタートメニューに `z*` の項目が在るかに依存する**。
- つまり結果は緑とも赤とも決まらず、**操作者の環境で判定が変わる非決定な検査**になる。これは安定した赤より悪い——CI で通って手元で落ちる（あるいはその逆）ので、原因が seed の破損だと誰も気づかない。
- **`*.bin` の生成検査はこの経路を捕まえない**（`index.bin` はどちらの経路でも書かれる）。

→ したがって **`[config] ` 行の不在検査（`visual-check-colors.ps1` の `Test-SeedHealth` と同型）を同じ変更で入れる**。前提は確認済み: `[config] ` の eprintln は parse 失敗 / 非 UTF-8 / read 失敗の全 arm に在り、**NotFound（first-run）arm と成功 arm には無い**。常時 seed する構成では「`[config] ` が出た ⇒ seed が読まれなかった」が成立する。

### 4.4 変わらないもの

- **単一インスタンス**: `tauri_plugin_single_instance` の識別子は app identity であって config dir ではない（`SPEC.md` §13・`visual-check-colors.ps1:71-79`）。プロファイルを分けても**同時起動はできない**。CI は逐次なので影響なし。**「プロファイルごとに並行 smoke」は不可能**（issue の「残る費用」と一致）。
- 両 smoke が冒頭で `Get-Process snotra | Stop-Process -Force` する点（開発機では利用者の常駐インスタンスを落とす）は不変。

### 4.5 副次的に良くなること（記録しておく価値がある）

- 開発機で `-HotkeyVks "17,75"` のような**実機 hotkey の明示指定が不要になる**（seed の Alt+Q を `hotkey:registered` から読むため）。`CONTRIBUTING.md:92` と `docs/build-commands.md` の「`-HotkeyVks` は override として残す」記述は真のまま。
- 開発機でも **results 被覆が常に走る**ようになる（今までは常に skip）。CI だけが持っていた被覆がローカルに広がる。
- `smoke-startup.ps1` が実ユーザーの `%APPDATA%\Snotra\config.toml` を**作らなくなる**（現状、config を持たない機械で 5 起動すると実 config が生える）。

---

## 5. 設計上の選択肢と推奨

| # | 選択肢 | 推奨 | 理由 |
|---|---|---|---|
| O1 | プロファイル置き場: (a) `target/smoke/…` / (b) `$env:TEMP` / (c) 毎回 GUID 付き一時ディレクトリ | **(a)** | #803 の前例（`target/visual-check/profile`）に揃う・`cargo clean` が掃く・`/target` は `.gitignore` 済み。残余は ADR 却下 4 と同じ（`CARGO_TARGET_DIR` 環境では掃除対象外）。**(c) の「毎回まっさら」という利点は、(a) + 起動前 wipe で得られる**（rust-cache が `target/` を復元しうるので wipe は必須・§9） |
| O1' | scan 用ダミーの置き場 | **プロファイルの兄弟**（`target/smoke/egui/scan/`）。**プロファイル配下に入れない** | 索引の背景再スキャン（SPEC §3.3）はディレクトリのハッシュを取る。`index.bin` を書き込む先を scan 対象にすると、拡張子フィルタとは別軸で自己参照が生じうる。兄弟なら 1 回の wipe で両方片付く |
| O2 | `smoke-startup.ps1` も seed するか | **する** | §4.1。空プロファイルは release.yml で GUI プロセス 5 個を残す |
| O3 | 2 本のスクリプトでプロファイルを共有 / 分離 | **分離**（`target/smoke/egui`・`target/smoke/startup`） | 共有すると「先に走った方が後の入力を決める」＝**今回消したい結合の再生産**になる |
| O4 | `-ResultsQuery` を残す / 消す | **消す**（定数 `'z'` へ） | 存在理由（「開発機の既存索引に当たる文字を渡す」）が索引を所有した時点で消滅する。残すなら害は無いが、**残す限り「索引を制御できない場合がある」という前提も残る**。消すなら `Get-LetterVk` と、docs の「A-Z 単字でない場合は throw」の記述も同時に |
| O5 | `-SeedConfig` を「既定 on の switch」として残す | **残さない** | 呼び出し元は `e2e.yml` 1 か所のみ。`-SeedConfig:$false` に意味のあるユースケースが無い（実 config を読ませたい局面は「プロファイルを使わない」ことを意味し、それは今回消す当のもの） |
| O6 | seed TOML を共有ヘルパーへ括り出す | **やらない** | ADR 却下 3 ＋ issue コメントの裁定（#843 が共有モジュール本体を持つ）。ただし**相互参照コメントは 3 者（smoke-egui / smoke-startup / visual-check）へ張り直す**——現在は 2 者間のリンクしかなく、3 本目を足せばリンクが不完全になる |
| O7 | `e2e.yml` のステップ順を入れ替えて独立性を示す | **入れ替えない** | 片方の順序で緑でも「順不同」は示せない（対称の証明にならない）。証明は §6-1 の**手元での逆順実行**で取り、CI は現行順のまま置く。churn も減る |
| O8 | `[config] ` 検査を `smoke-startup.ps1` にも入れるか | **入れる** | 費用は 3 行。入れないと、startup 側は seed 破損に対して**完全に無言**（アサーションが「trace ≥ 1 件」だけのため） |

---

## 6. 検証手順（フォールトインジェクション込み）

**順序に意味がある。1 が issue の主目的そのものの受け入れ、4〜6 がセーフティネットの実測（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）。**

1. **逆順実行（headline claim の直接証明・最優先）**
   ```powershell
   cargo build --release -p snotra
   pwsh -NoProfile -File scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe   # 先に 5 起動
   npm run smoke:egui -- -ExePath target/release/snotra.exe                              # 後から egui
   ```
   → **egui smoke が results 被覆込みで PASS すること。** 「順序制約が消えた」という主張は**この変更で新しく書く一文**であり、`docs/development-principles.md`「撤去の作法」に従って測ってから書く。
2. **正順でも PASS**（現行 CI と同じ並び）。
3. **実 config へ「書いていない」こと**: 実行前後で `%APPDATA%\Snotra\config.toml` の `LastWriteTime` とハッシュが不変（存在しない機械では「実行後も存在しない」）。`%APPDATA%\Snotra\*.bin` の更新時刻も不変であること。
   - **これは env が効いたことの証明にはならない。** `SNOTRA_CONFIG_DIR` が届かなければアプリは実 config を**読む**が、読むだけならハッシュも更新時刻も変わらない。**env が効いたことを示す唯一の観測は、プロファイル側に `*.bin` が生成されること**（§1 の肯定的証拠）である。2 つの検査は交換可能ではなく、荷重を持つのは後者だけである。
4. **FI-1（`SNOTRA_CONFIG_DIR` が効かない場合に赤くなるか）**
   - **ライブのスクリプトは変異させない**（safety-nets「稼働中のガードを弱めない——複製に変異を当てる」）。`scripts/smoke-egui.ps1` をスクラッチパッドへコピーし、**コピー側だけ** `$env:SNOTRA_CONFIG_DIR` の代入先を**別の使い捨てディレクトリ**へ差し替える（assert 側は元の `$profileDir` を見たまま）。
   - 期待: `*.bin` が生成されていない旨で **exit≠0**。
   - **`SNOTRA_CONFIG_DIR` を未設定にする形の変異を選ばない**——それだとアプリが実 config を読み書きしてしまい、検証が実データに触れる（この PR が消そうとしている当のもの）。
5. **FI-2（seed が壊れたときに赤くなるか）**
   - コピー側の seed TOML から `[hotkey]` セクションを落とす → `[config] failed to parse …` が stderr に出る → **`[config] ` 検査で赤**。
   - **この FI を省くと、§4.3 の沈黙経路に対する検査が「書いただけ」になる。**
6. **FI-3（results 被覆が失われたときに赤くなるか＝`-RequireResults` の後継の実測）**
   - コピー側の seed から `[[paths.scan]]` を落とす（索引 0 件）→ `egui_results:show` が観測されず **exit≠0**。
   - 見るべきは「**skip ではなく赤**」であること。旧設計ならここが黄色 NOTE + exit 0 だった。
7. **構造の確認（消し忘れの数え上げ）**
   ```powershell
   Select-String -Path scripts/*.ps1, .github/workflows/*.yml, docs/*.md, docs/adr/*.md, CONTRIBUTING.md `
     -Pattern 'SeedConfig|RequireResults|ResultsQuery|resultsChecked|seededNow'
   ```
   → 残ってよいのは `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` と `docs/superpowers/plans/**` のみ（§1-F）。**`head` / `Select-Object -First` を使わない**（`docs/development-principles.md`「列挙の完全性」）。
   同様に `Select-String -Pattern 'APPDATA' -Path scripts/*.ps1` が 0 件であること。
8. **`release.yml` の呼び出し互換**: `pwsh -NoProfile -File scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe`（**`-ExePath` のみ**）で PASS。必須パラメータを増やしていないことの実測。
9. **PowerShell 構文検査**（PostToolUse は `.ps1` に検査を割り当てない＝沈黙は「走らなかった」）
   ```powershell
   $errs=$null; [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path ./scripts/smoke-egui.ps1), [ref]$null, [ref]$errs) | Out-Null; $errs
   ```
   `smoke-startup.ps1` も同様。
10. **`npm run governance:check`**（ガバナンス文書の変更・G-references / G-ci-table / G-build-commands）。
11. **`npm test`**（`scripts/**/*.test.mjs` は `.ps1` を見ないが、CI スコープの非回帰確認）。
12. **PR 上で `Smoke` workflow が自動起動すること**（`paths` に `scripts/smoke-*.ps1` と `.github/workflows/e2e.yml` が在るので発火するはず）を**ジョブのログで**確認する。`docs/build-commands.md`「CI に検証を委ねるなら、その job が実際に何を実行したかを確かめる」に従い、**ステップに渡された引数**を CI ログで読む。
13. **2 回連続実行**（wipe の実測）: 続けて 2 回走らせ、2 回目も同じ結果になること。1 回目の `index.bin` が残って 2 回目の肯定的証拠が**空振りで合格**していないことを、wipe のログか `LastWriteTime` で確かめる。

---

## 7. やりすぎ（YAGNI）と判定したもの

| 判定 | 対象 | 理由 |
|---|---|---|
| やらない | **seed / プロファイル配管の共有モジュール化** | issue コメントの裁定で **#843** が持つ。ADR 却下 3 も現時点では重複を選んでいる |
| やらない | **Pester テストの新設** | 同上（#843） |
| やらない | `.claude/rules/safety-nets.md` の `paths` に `scripts/*.ps1` を足す | #843 の受け入れ条件に明記されている。**ただし本 PR ではこの rule が自動配送されないので、実装者は手動で読む必要がある**（§8 罠 7） |
| やらない | `-ProfileDir` / `-KeepProfile` 等の新パラメータ | 呼び出し元は `e2e.yml` と `release.yml` の 2 か所だけ。可変にする顧客がいない |
| やらない | 並行 smoke（プロファイルごとの同時実行） | `tauri_plugin_single_instance` が構造的に禁じる。issue も「残る費用」として受容済み |
| やらない | **first-run 経路を smoke で被覆する** | `snotra-settings.exe` の同梱ビルドが要り、GUI プロセスの後始末機構も要る。`e2e.yml:77-80` が既に「first-run はこの job の検証対象ではない」と受容している。別 issue 相当 |
| やらない | `scripts/measure-memory*.ps1` / `manual-smoke.ps1` / `bench-startup.ps1` の env 化 | issue の「触る面」外。`measure-memory.ps1:24` は逆に**実 config の設定を推奨**しており、目的が違う |
| やらない（**別 issue を推奨**） | **`smoke-startup.ps1` の `*:error` 検査の是正** | 全 crate の `src/` を literal `:error` で検索した結果、**トレースイベント名で `:error` を末尾に持つものは 1 件も存在しない**（ヒットは `std::error::Error` / `log::error!` / `crate::error` / `folder::error_result` のみ）。つまり `$_.event -like "*:error"` は**現状どのイベントにも一致しない**。#690 が足した「trace ≥ 1 件」の検査だけが実質のアサーションである。**本 PR の射程外**（爆風半径を広げない）だが、#804 の作業中に見えた事実として記録する |
| やらない | `e2e.yml` のステップ順の入れ替え | §5 O7 |

---

## 8. 実装者が踏みうる最大の罠と、それを避ける手続き

**罠 1（最大）— seed が壊れても緑になる。** §4.3。既定 hotkey が seed と同一（Alt+Q）なので show/hide は通り、開発機では既定 scan（スタートメニュー）に `z` が当たって results まで通る。
→ **手続き**: `[config] ` 行の不在検査を**同じコミットで**入れ、**FI-2 を実装直後に走らせる**。「書いた」で止めない。

**罠 2 — 古いプロファイルで肯定的証拠が空振りする。** `index.bin` はキャッシュミス時にしか書かれないので、前回の残骸が在れば「存在する」は自明に成立する（`visual-check-colors.ps1:83-87` が `*.bin` を消してから起動しているのは、まさにこの空振りを潰すため）。
→ **手続き**: 起動前に `$smokeRoot` を `Remove-Item -Recurse -Force`。**wipe を先に書き、assert を後に書く**。§6-13 で 2 回連続実行して確かめる。

**罠 3 — 「プロファイルを分けたのだから空でよい」と考える。** §4.1。release.yml で GUI プロセスが 5 個残る。
→ **手続き**: seed を省く案を検討したら、必ず `setup_first_run` → `launch_settings_process` を**コードで**追う。issue には書かれていない。

**罠 4 — `SNOTRA_CONFIG_DIR` の値がそのまま使われる（展開も絶対化もしない）。** `%TEMP%\…` のような値は展開されず、相対パスは CWD 起点になり、**既定へフォールバックしない**（ADR 却下 1・意図的）。`Resolve-Path` は存在しないパスで throw する。
→ **手続き**: `New-Item -ItemType Directory -Force` → `(Resolve-Path …).Path` の順で**絶対パス化してから**代入する（`visual-check-colors.ps1:82,132-133` と同じ順）。

**罠 5 — 「順序制約は消えた」という新しい一文を測らずに書く。** 「消す変更の中で新しく書いた 1 文は、削除ではなく新規記述である」（`docs/development-principles.md`「撤去（消す変更）の作法」・#660 で実際に踏んだ）。
→ **手続き**: §6-1 の逆順実行を**先に**やり、その結果を PR 本文に貼ってから文を書く。

**罠 6 — `.ps1` の編集は PostToolUse が何も走らせない。** CLAUDE.md「沈黙が『合格』なのは `selectChecks` に検査が割り当てられたファイルだけである」。`scripts/` 配下の非 TS ファイルは対象外。
→ **手続き**: §6-9 の構文検査を手で打つ。加えて**両スクリプトを実際に 1 回ずつ走らせる**まで完了としない。

**罠 7 — safety-nets rule が自動配送されない。** `paths` は `scripts/*.mjs` まで。**CI ゲートに載るスクリプトを触るのに、それを守る rule が届かない。**
→ **手続き**: 実装開始時に `.claude/rules/safety-nets.md` を**手動で開く**。`.github/workflows/e2e.yml` を触った時点で配送はされるが、`.ps1` を先に触ると届かない順序がありうる。

**罠 8 — 消し忘れの鳴り方が非対称。** `e2e.yml` の**引数**の消し忘れは pwsh が未知パラメータで落として鳴るが、**コメントと docs** の消し忘れは永久に鳴らない（§2 の機械検査射程）。
→ **手続き**: §6-7 の grep を**完了ゲート**に置く（`head` を使わない全件）。

**罠 9 — 相互参照コメントが 2 者から 3 者になる。** 現在は `smoke-egui` ↔ `visual-check` の双方向リンクだけ。`smoke-startup` が 3 本目の seed を持つと、リンクが不完全になる。
→ **手続き**: 3 本すべてに「他 2 本にも同型の seed がある」と書くか、**seed の必須セクション根拠の正本を 1 か所（例: ADR 却下 2）に定めて 3 本から参照する**（`AGENTS.md`「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」）。後者を推す。

**罠 10 — `release.yml` を忘れる。** `smoke-startup.ps1` の呼び出し元は `e2e.yml` だけではない。`release.yml` の "Run startup smoke on release binary" ステップが `-ExePath` のみで呼ぶ。
→ **手続き**: 新しい必須パラメータを足さない。§6-8 で実測する。

**罠 11 — env の後始末。** `smoke-startup.ps1` には現在 `try/finally` が無い。途中 throw で `SNOTRA_CONFIG_DIR` が呼び出し元セッションへ漏れると、以後の `cargo run -p snotra` と `measure:memory` が使い捨てプロファイルを指し続ける（`visual-check-colors.ps1:325-329` が同じ危険を注記している）。
→ **手続き**: `try/finally` で包み、`Remove-Item Env:SNOTRA_CONFIG_DIR -ErrorAction SilentlyContinue` ではなく**退避した値を復元**する（元から設定していた開発者を壊さない）。

---

## 9. 自信の低い箇所・未検証の観点

**本レビューはスクリプトを 1 度も実行していない。** 以下は特に未実測。

1. **`Swatinem/rust-cache@v2` が `target/smoke/` を復元するか未実測。** `workspaces: src-tauri` の指定と、ルートの `target/` がキャッシュ対象かどうかを読み切れていない。**だから毎回 wipe する**（wipe すれば真偽に関係なく安全）。逆に言えば、wipe を省く設計はこの未検証点に依存してしまう。
2. **CI runner の `target/release/snotra-settings.exe` の有無。** `e2e.yml` は `-p snotra` しかビルドしないが、キャッシュ復元で過去の成果物が残る可能性を排除できていない。§4.1 の分岐 A/B の**どちらに落ちるかが CI では不確定**。→ seed する設計ならどちらでも踏まないので、この不確実性は無害化される。
3. **「scan 0 件でも `index.bin` が書かれる」は #803 のコメント由来で、自分では測っていない。** そのため `smoke-startup.ps1` の seed にも `[[paths.scan]]`（ダミー 1 件）を置く案を推奨した。0 件で書かれることを実測できたなら、startup 側は scan なしでもよい。
4. **`index.bin` の書き込みが `hotkey:registered` より先である**という主張は `main.rs` の読解（索引ロードが `Builder…run()` より前、hotkey 登録は setup 内）に基づく。**タイミングは実測していない**。もし外れると `Stop-Process -Force` の打ち切りとレースし、肯定的証拠がフレークする。→ 実装後に「`*.bin` 検査が 3 回連続で緑」を確かめること。
5. **キーストローク注入が前景ウィンドウに奪われるリスク。** 開発機でも results 検査が常時走るようになるため、**この既存リスクの露出が増える**（今までローカルでは skip されていた）。既存の 2 回リトライ + Backspace 機構がどこまで吸収するかは未測。ローカルで赤が増えるようなら、リトライ回数の見直しが follow-up になりうる。
6. **`smoke-startup.ps1` を seed する設計は issue が明示していない**（「触る面」に挙げるだけ）。§4.1 の実測（コード読解）に基づく私の判断であり、**裁定が要る**。「startup は env 化だけして seed しない」を選ぶなら、release.yml の GUI プロセス残存を別途処理する必要がある。
7. **`docs/build-commands.md` の書き換え分量**を見積もっていない。純減だが、`-RequireResults` バレットの後継に「新しい 2 つの検査 + FI 手順」を書くと**足す方が多くなる可能性**がある（「移設は削除ではなく差し引きである」）。削減幅を主張するなら実測してから。
8. **`Get-LetterVk` を消すかどうか**は好みの範囲。消せば docs の「鳴る経路」の列挙も直す必要があり、残せば死んだ throw 分岐が残る。どちらも小さい。
9. **`e2e.yml` の `paths` を増やす必要があるか**は「不要」と判断したが、`scripts/visual-check-colors.ps1` を触る（§1-E）ので、**その変更だけでは Smoke workflow が発火しない**組み合わせがありうる（他のファイルも触るので実際には発火する）。
10. **`*:error` に一致するイベントが 0 件**という §7 の観察は、全 4 crate の `src/` に対する literal `:error` 検索に基づく。ビルドスクリプトや `snotra-egui-runtime` 外のマクロ展開で生成される名前までは追っていない。**中程度の確信**。
