# independent-derivation: #804 の必要な変更集合（独立再導出）

**前提**: 本書は `workspace/plan.md` / `research.md` / `plan-snapshot.md` / `plan-review/` の既存成果を**読まずに**、`gh issue view 804` とコードベースだけから導出した。既存計画との一致・不一致は検討していない。

**偶発的な露出**: `git grep SNOTRA_CONFIG_DIR` を全ツリーに掛けた際、`workspace/plan.md` / `research.md` / `plan-snapshot.md` の**マッチ行 12 行が grep 出力に混じった**（ファイルは開いていない）。以降の列挙はすべて `':!workspace'` で除外して取り直している。混入した内容は「env を設定して finally で戻す」「参照実装は visual-check-colors.ps1」程度で、本書の結論（`-SeedConfig` / `-ResultsQuery` の撤去、env 窓を `Start-Process` の前後に閉じる、`*.bin` の ∃ 証拠、seed 健全性判定、ADR §3 の再決定）はいずれもその行から導けるものではない。念のため申告する。

---

## 1. 要件の理解

### 何を達成するのか

`scripts/smoke-egui.ps1` と `scripts/smoke-startup.ps1` が **ユーザーの実 config（`%APPDATA%\Snotra`）を共有リソースとして踏んでいる**のをやめ、`SNOTRA_CONFIG_DIR`（#803 で `Config::config_dir()` に入った env seam）で**スクリプトが所有する使い捨てプロファイル**へ隔離する。参照実装は `scripts/visual-check-colors.ps1`（#803 で同じ分離を済ませている）。

### なぜそれで 4 つの制約が消えるのか（因果の連鎖）

現状の連鎖は次のとおりで、根は 1 つである。

1. `smoke-egui.ps1:66-67` が `$env:APPDATA\Snotra\config.toml` を直接組み、**実 config を上書きしないこと**を安全性の担保にしている（`:68` の `if (-not (Test-Path $cfgPath))`）
2. ゆえに `-SeedConfig` は「不在時のみ」seed する。既存 config を持つ開発機では seed が不成立（`:108-110` の `Config already exists, seed skipped`）
3. seed 不成立 → `$seededNow=$false` → `$ResultsQuery` が空のまま（`:114-116`）→ results 窓の検査が**沈黙して skip**（`:373` の条件、`:477-482` の NOTE）
4. その沈黙を CI で失敗に変えるために `-RequireResults`（#686・`:118-134`）が要る
5. `-RequireResults` が守っている前提は「CI では seed が成立する」であり、それは **startup smoke の 5 起動が `config.toml` を作る前に egui smoke を走らせる**ことに依存する → `.github/workflows/e2e.yml:65-73` のステップ順序制約
6. その順序制約を規範として `docs/build-commands.md` が記述している（issue は `:147` と書くが**現在の実体は `:161`**。`:147` は今は `cargo run -p snotra` の行である。序数で編集しないこと）

プロファイルを分離すると (1) が消え、(2)〜(6) が**導出されなくなる**。issue が「4 つはすべて共有リソース制約から派生している」と述べているのはこの連鎖のことで、コードを読んだ限り正しい。

### 達成条件（受け入れ条件として測れる形へ）

- **A. 実 config 不可侵**: 両 smoke の実行前後で `%APPDATA%\Snotra` の中身（ファイル一覧・更新時刻）が変わらない
- **B. 実行結果が履歴に依存しない**: どの smoke を何回どの順で走らせても、各 smoke の検査内容と結果が変わらない（= 順序制約の消滅を**性質として**述べたもの）
- **C. results 検査に skip 経路が存在しない**: 「走らなかったのに緑」という状態が**構造的に表現不能**である（∀ 条件の列挙ではなく、分岐そのものの不在で担保する）
- **D. 隔離が効いたことを肯定的に観測する**: プロファイル配下に本体が実際に書いた痕跡（`*.bin`）があること。効いていなければ本体は実 config を読むので、失敗時に「検査対象が壊れた」と「env が効いていない」を切り分けられない（`docs/build-commands.md:76` が check:colors について既に規範化している）

### 明示的にスコープ外

- `scripts/measure-memory.ps1` / `measure-memory-stages.ps1` / `snotra-core/tests/memory_footprint.rs` — これらは**実運用点（実 index）を測ることが目的**である（`memory_footprint.rs:11`「実運用点（`%APPDATA%\Snotra\index.bin` の実インデックス）」）。env 化すると測っているものが変わる。「smoke と同じ扱いに揃える」誘惑を明示的に退ける
- `scripts/manual-smoke.ps1` — 実 config での人間の目視が目的（`SNOTRA_TRACE` しか触っていない・`:198-204`）
- 並行 smoke 化 — `tauri_plugin_single_instance` は config dir と無関係（`SPEC.md:616` / `visual-check-colors.ps1:74-76`）。**プロファイルを分けても同時起動はできない**。2 つの smoke ステップは同一 job で逐次のままにする
- `CARGO_TARGET_DIR` 設定環境で `target/` が `cargo clean` の対象から外れる件 — `ADR-config-dir-env-seam-rejected-alternatives`「4. 検証プロファイルの掃除のために `cargo metadata` で `target_directory` を引く」で**受容残余として決着済み**。再決定しない

---

## 2. 必要な変更集合

### 2.0 設計判断（実装前に確定しておくもの）

| 判断 | 結論 | 根拠 |
|---|---|---|
| プロファイルの置き場所 | `target/smoke-egui/profile` / `target/smoke-startup/profile`（`$PSScriptRoot` 相対で組み、`New-Item -Force` → `(Resolve-Path …).Path` で**絶対化してから** env へ入れる） | `visual-check-colors.ps1:56-60` の前例（`$env:TEMP` ではなく `target/` 下＝ `cargo clean` が掃く）。絶対化が**load-bearing**: `Config::config_dir_from` は展開も絶対化もしない（`config.rs` の doc・ADR §1）ので、相対値を渡すと本体の CWD 起点で落ち、スクリプトが見ていない場所へデータが行く |
| 2 つの smoke でプロファイルを共有するか | **分ける** | 共有すると「startup が書いた状態を egui が読む」経路が残り、受け入れ条件 B（履歴非依存）が復活する。消そうとしている当のものである |
| smoke-startup も seed するか | **する**（`[hotkey]` / `[appearance]` / `[paths]` の 3 必須セクションのみ。`[[paths.scan]]` は置かない） | seed しないと空プロファイル＝ **first-run** になり `main.rs:141` → `setup_first_run` → `launch_settings_process` を踏む。しかも挙動が環境依存で分岐する（実測: `window.rs:70-77` は `snotra-settings.exe` が exe の隣に無ければ `cmd:launch_settings_process:not_found` を出して `Err` → `main.rs:332` が `start_index_build`（**既定 scan パス**を走査）へフォールバック。e2e.yml は `cargo build --release -p snotra` しかしないので CI では後者、`release.yml` は両方ビルドするので**設定 GUI が実際に起動する**）。さらに 2 回目以降の invocation では `Config::load` が作った config.toml が残って first-run にならない＝**invocation 間で非決定**。受け入れ条件 B に反する |
| first-run を検査対象に戻すか | **戻さない** | `e2e.yml:77-80` が「first-run は本 job の検証対象ではない（アサーションは `*:error` が無いことだけ）ので、カバレッジの縮小として受容する」と**意図的な受容**として記録している。隔離はこの性質を保存するべきで、副作用として first-run を再導入するのは受容の記録を無言で覆すことになる |
| `-SeedConfig` を残すか | **撤去**（issue の「消えるもの」は条件分岐の除去までだが、ここは逸脱する。理由は下記 2.1） | スクリプトがプロファイルを所有した後、「seed しない」実行は空プロファイル＝ first-run ＝ フォーカス奪取であり、**選ぶ理由のある実行モードではない**。YAGNI |
| `-ResultsQuery` を残すか | **撤去**（同じく逸脱） | 索引が常に seed した `zsnotrasmoke.exe` の 1 件だけになるので、`z` 以外の文字は**必ず失敗する**。残すと「渡せば動く」ように見える罠になる |
| seed TOML の 3 つ目の写しをヘルパーへ括るか | **括らない（重複を選ぶ）が、ADR へ 1 行足す** | `ADR-config-dir-env-seam-rejected-alternatives`「3.」の却下理由の一つ（`-RequireResults` ゲートに載る CI 経路ゆえ触るリスク）は**この変更で偽になる**。残る理由（3 つの seed は目的が違い同型ではない・`scripts/` に共有ライブラリの下地が無い）は生きているので結論は維持する。ただし写しが 2→3 になり `docs/development-principles.md`「DRY」の「3 回目で抽出を検討する」に触れるので、**検討したうえで維持した**ことを記録する |
| seed に `[general]` を書くか | **書かない**（現行 seed と同じく 3 セクションのみ） | `visual-check-colors.ps1:104-107` は `show_on_startup` と `auto_hide_on_focus_lost = false` を pin しているが、**あちらは窓のピクセルを撮るスクリプト**で、端末がフォーカスを持つと既定 true では窓が隠れて「窓が在った座標の下」を撮ってしまうから pin している。smoke は trace を観測するのでその必要が無く、pin すると **CI が今日通しているのと違う設定で show/hide を検査する**ことになる。プロファイルを所有したからといって、決定性の名目で既定から離れない |
| e2e.yml のステップ順序 | **据え置く**（変更しない） | 順序制約が消えたことは「順序を変えてよい」であって「変えるべき」ではない。ただし**検証で 1 度だけ入れ替えて緑を実測する**（§4）。diff を最小に保つ |

### 2.1 `scripts/smoke-egui.ps1`（変更の中心）

| 現在の位置 | 現在の内容 | 変更 |
|---|---|---|
| `:5` | `[switch]$SeedConfig,` | **削除** |
| `:13-17` | `-ResultsQuery` の param とコメント（「seed できた場合のみ "z"」「skip する」） | **削除**（内部定数 `$resultsQuery = 'z'` へ。値の出所は seed するダミーのファイル名で、同じ場所に併記する） |
| `:19-22` | `-RequireResults` の param とコメント | **削除** |
| `:43-56` | ヘッダの散文。`:47`「（索引内容を制御できるとき）1 文字クエリを注入して」、`:52-54` の `-SeedConfig` 説明 | **書き換え**。「常に自前プロファイルへ seed し、results 検査は常に走る」へ。`:55`（WebView2 撤去済み）は不変 |
| `:64` | `$seededNow = $false` | **削除**（概念ごと消える。`:106` `:114` `:128` が読み手） |
| `:65-111` | `if ($SeedConfig) { $cfgDir = Join-Path $env:APPDATA "Snotra"; … if (-not (Test-Path $cfgPath)) { … } else { … } }` | **置換**。無条件に (a) プロファイル dir を作成、(b) **`*.bin` と `config.toml.bak` を消す**（理由は `visual-check-colors.ps1:82-85` と同じ——残すと後段の 2 判定〔seed 健全性・env が効いた証拠〕が**古いファイルで空振り合格する**）、(c) スキャン用ダミー dir（`target/smoke-egui/scan/zsnotrasmoke.exe`。現行は `$env:TEMP\snotra_smoke_scan`・`:73-76`。プロファイルと同じ根へ寄せて `cargo clean` に載せる）、(d) seed TOML を書く、(e) `Resolve-Path` で絶対パスを得る |
| `:78-89` | seed TOML のコメント。特に `:81`「この smoke が e2e.yml の -RequireResults ゲートに載る CI 経路だからである（#803 で分離を判断）」 | **書き換え**。`-RequireResults` は消えるので、共有ヘルパー化しない理由を残る根拠（seed の形が違う）へ寄せ、ADR を参照する。`:82-89` の必須セクション/`scan` 併記禁止の根拠は**そのまま残す**（依然真） |
| `:113-116` | `if ([string]::IsNullOrEmpty($ResultsQuery) -and $seededNow) { $ResultsQuery = "z" }` | **削除**（定数化） |
| `:118-134` | `-RequireResults` の guard と throw 文言（`:129` の `$env:APPDATA 'Snotra\config.toml'`、`:130-132` の「startup smoke より前に置け」） | **全削除** |
| `:145-153` | `Get-LetterVk`（A-Z 単字 validation + throw） | クエリが定数になるので **throw は到達不能**。`Get-LetterVk` 自体は残してもよいが、`$queryVk = [byte][char]'Z'` へ畳むほうが「到達しない検査を残さない」。どちらでも可、ただし `docs/build-commands.md:161` の「クエリが A-Z 単字でない」列挙は消えるので**文書側と同時に決める** |
| `:211-219` | `SNOTRA_TRACE` の save → set → `Start-Process` → 即 restore | **同じ形で `SNOTRA_CONFIG_DIR` を並べる**。`try/finally` は不要である——子プロセスは**生成時に環境をコピーする**ので、`Start-Process` の次の行で親を戻しても子には影響しない。env が立っている区間に throw しうるコードが 1 行も無いので、`finally` を置くと守るものが無いまま判定ロジック全体を包む形になる（レビューで必ず「なぜ finally が無いのか」を問われるので、**この機序をコメントに書く**） |
| `:296` `:372` `:373` | `$resultsChecked = $false` / 再代入 / `if ($failures.Count -eq 0 -and -not [string]::IsNullOrEmpty($ResultsQuery))` | **`$resultsChecked` を消し、results 検査を `$failures.Count -eq 0` だけの条件にする**。受け入れ条件 C（skip が表現不能）を∀条件の列挙ではなく**分岐の不在**で満たすのがこの一手である |
| `:422-425` | `if ($resultsChecked -and -not (Wait-TraceEvent … "egui_results:hide"))` | 条件から `$resultsChecked` を落とす |
| `:432` | `if ($resultsChecked -and $failures.Count -eq 0)`（orphan 検査） | 同上 |
| `:476-482` | 成功メッセージの `if ($resultsChecked) {…} else { NOTE: … SKIPPED … }` | **else 節を削除**し、成功行を 1 本に畳む（`:481` は `%APPDATA%` と `-SeedConfig` / `-ResultsQuery` を名指しする最後の 1 行でもある） |
| **新規（`:410` 付近・プロセス kill の後）** | — | **(D) 隔離の肯定的証拠**: プロファイル配下に `*.bin` が 1 つ以上生成されていること。無ければ失敗（`visual-check-colors.ps1:286-303` と同型・`docs/build-commands.md:76` の規範）。**どの `.bin` が出るかは自分で測ること**——visual-check は `index.bin` を実測しているが、あちらは scan 0 件・こちらは 1 件で、かつ両方とも強制 kill ゆえ `window.bin` / `history.bin` は出ない見込み |
| **新規（同じ場所）** | — | **seed 健全性**: `$errPath`（本体 stderr。既に `-RedirectStandardError` で取っている）に `[config] ` で始まる行が無いこと。この判定は `ADR-config-dir-env-seam-rejected-alternatives`「5.」が「読み込み失敗の全 arm に在り成功時には出ない」と確定させた観測点で、**追加コストがほぼゼロ**。無いと、seed の TOML を壊したときアプリは既定値で起動し（hotkey は既定も Alt+Q なので show までは通る）、scan だけが既定に落ちて `egui_results:show not observed` という**原因を指さない赤**になる |

### 2.2 `scripts/smoke-startup.ps1`

| 現在の位置 | 変更 |
|---|---|
| `param()`（`:1-10`） | 変更なし（`-ProfileDir` のような口を足さない・YAGNI） |
| `:28-39` 付近（ループ前） | **新規**: プロファイル dir の作成 → `*.bin` / `config.toml.bak` の掃除 → 最小 seed TOML の書き込み → `Resolve-Path` で絶対化。**seed はループの外で 1 回**（5 起動が同じプロファイルを共有するのは現行と同じ形。索引キャッシュが効いて 2 回目以降が速いのも現行と同じ性質） |
| `:30`, `:32-39`, `:49`, `:77` | `$savedTraceEnv` / `Restore-TraceEnv` / `$env:SNOTRA_TRACE = "1"` / `Restore-TraceEnv -Saved …` | **`SNOTRA_CONFIG_DIR` を同じ形で並べる**。ただし現行の `Restore-TraceEnv` はループ**末尾**（`:77`・kill の後）にあり、`Start-Process`（`:50`）から離れている。子は生成時にコピーするので `Start-Process` の直後へ寄せてよい。**現行の形に合わせて末尾で戻す**か**両方 `Start-Process` 直後へ寄せる**かは選べるが、**2 つの env の扱いを揃えること**（片方だけ別の寿命にすると、読み手が「なぜ違うのか」を毎回推測する） |
| `:25-27` のコメント | 変更なし（2 窓アーキの説明・真のまま） |
| **新規（ループ後 or 各 run 後）** | プロファイル配下の `*.bin` の ∃ 検査（egui 側と同じ理由）。**5 run 分まとめて 1 回で足りる** |

### 2.3 `.github/workflows/e2e.yml`

| 位置 | 変更 |
|---|---|
| `:65-73` | コメント全体（flip 済みの 1 行は残す）。「startup smoke より前に置く（#686）」「`-SeedConfig` は不在時のみ」「job は緑のまま results の検証が消える」「順序制約を守らせるのは `-RequireResults`」を**削除**し、「両 smoke は `target/` 下の自前プロファイルを持ち、互いの実行順に依存しない（#804）」へ置き換える |
| `:75` | `run: npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig -RequireResults` → `run: npm run smoke:egui -- -ExePath target/release/snotra.exe` |
| `:77-80` | 「上の egui smoke が seed した config.toml が既に在るため、ここでの 5 起動は first-run 経路を通らない（順序を入れ替える前は 1 回目が first-run だった）」——**プロファイルが分かれた後は偽**（config.toml を作るのは自分自身の seed）。first-run を検証対象にしない受容は**残す**ので、その根拠だけ書き換える |
| `:13-27`（paths） | **変更なし**。ただし新規スクリプトファイルを 1 つでも足すなら paths への追加が必須（`scripts/smoke-*.ps1` は個別列挙であり `scripts/**` ではない）。**この漏れを検出する機構はリポジトリに無い**（§3.4） |

### 2.4 `docs/build-commands.md`

| 位置 | 変更 | 種別 |
|---|---|---|
| `:45` | 「この 1 事例は `-RequireResults` が機構化した（#686・**下記**）が、…」 | **最重要**。識別子（`-RequireResults`）と**序数的な指し（「下記」＝ `:161` の消えるブロック）**の二重の腐り。`-RequireResults` を grep しても「下記」は出てこない。保証が階梯を 1 段上がった（CI フラグ → 構造的に表現不能）と書き直す |
| `:159` | `-SeedConfig` の説明（「config.toml 不在時のみ…既存 config は上書きしない」） | 書き換え。「自前プロファイル（`target/smoke-egui/profile`）へ常に seed する。実 config は読みも書きもしない」 |
| `:160` | results 検査の説明（「索引内容を制御できるときだけ」「`-SeedConfig` で新規 seed できた場合」「`-ResultsQuery <letter>`」「自動的に skip され、黄色 NOTE」） | 書き換え。**skip 経路が無くなる**ので条件節ごと消える。末尾の「CONTRIBUTING.md の『results 窓 show/hide の trace 観測』と対応」は維持できる |
| `:161` | `-RequireResults` のブロック全文（skip 経路の列挙・フォールトインジェクション手順・**e2e.yml の順序制約**・「#803 が入った後もこの順序制約は有効である」） | **全削除**。issue が `:147` と呼んでいるのはこれ（行番号がずれている）。代わりに 1 行だけ「両 smoke は自前プロファイルを持ち、実行順・実 config の有無に依存しない（#804）」を置く |
| `:158` | smoke-startup の説明 | プロファイル分離の一言を足す（`*:error` と trace 0 件の話は不変） |
| `:74-77` / `:79-91` | check:colors と env ハッチの節 | **本文は変えない**。`:79-91` の env ハッチ節が `SNOTRA_CONFIG_DIR` の SSOT 的な位置にあるので、smoke 側の記述はここを参照する形にして事実の写しを増やさない（`AGENTS.md`「文書に事実の写しを増やす変更」） |
| `:182-185` | CI 対応表と（注） | **変更不要**。表の `npm run smoke:egui` は e2e.yml に verbatim で残り、`smoke:startup` は wrapper パス一致で通る（`governance-check.mjs` の `checkCiTable`・`:454-461`） |

### 2.5 `scripts/visual-check-colors.ps1`（相互参照の維持）

- `:93-95`「`scripts/smoke-egui.ps1` の **`-SeedConfig`** が同型の seed を持つ（…片方だけ直さないこと）」— **`-SeedConfig` が消えるので識別子が宙に浮く**。「`smoke-egui.ps1` の seed」へ言い換える。あわせて smoke-startup の seed が 3 つ目として増えるので、相互参照を**三角**にする（片方向だけだと新しい写しが孤立する）
- `:11-14`（`SNOTRA_CONFIG_DIR` で使い捨てプロファイル…実 config は読みも書きもしない）— 真のまま。**ただし読者への含意が変わる**（「このスクリプトだけが安全」→「smoke も同じ形」）。触らなくても偽にはならないので、変更は任意
- `:21-23`（results 窓は `smoke-egui.ps1` の入力注入機構の複製になるので目視へ残した）— 真のまま・変更不要

### 2.6 `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md`

- `:31-39`「3. `scripts/smoke-egui.ps1` と seed TOML を共有ヘルパーへ括り出す」の却下理由が **`-RequireResults` ゲートの存在**を現在形で述べている（`:33`）。この変更で偽になる。
- 判断: **ADR の記述は当時の決定文脈ゆえ本文は書き換えない**が、`docs/development-principles.md`「撤去（消す変更）の作法」の「自分の変更が偽にした記述・作った矛盾は範囲外に置けない」に従い、**末尾に 1〜2 行の追記**を置く: 「#804 で `-RequireResults` は撤去され、seed の写しは 3 つになった。共有ヘルパー化を再検討したうえで、残る理由（3 つの seed は目的が違い同型でない・`scripts/` に共有ライブラリの下地が無い）により却下を維持する。」
- `:39`「smoke 側の env 化は #804 のスコープ」— この追記で自然に決着する。

### 2.7 変更しないと判断したもの（根拠つき）

| 対象 | 判断 |
|---|---|
| `CONTRIBUTING.md:92` | `smoke:egui` を「egui show/hide + results 窓 show/hide の trace 観測」と紹介するだけで、seed・skip・`-SeedConfig` に一切触れていない。**変更後も真**。ただし `docs/build-commands.md:160` がこの文との対応を宣言しているので、`:160` を書き換えるときに対応が壊れていないか読む |
| `.github/workflows/release.yml:83` | `pwsh -NoProfile -File scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe`。**引数は変わらないので編集不要**だが、**挙動は変わる消費者である**（今日はリリース runner の実 `%APPDATA%` に対して first-run で走り、`snotra-settings.exe` が隣にあるので設定 GUI が起動している。変更後は自前プロファイルで seed 済み起動になる）。§4 で「release.yml 経路が壊れないか」を明示的に確認対象へ入れる |
| `AGENTS.md:66` | 「trace イベント名・hotkey の前提が壊れないか確認する」——前提は不変。真のまま |
| `.claude/rules/src-tauri.md:28` | カテゴリ C の誘導のみ。不変 |
| `scripts/measure-memory-stages.ps1:23` | 「`smoke-egui.ps1` と同じくローカル実行時は注意（実行中の snotra を kill する）」——kill は残るので真のまま |
| `SPEC.md:616` / `docs/architecture.md:104` / `snotra-core/CLAUDE.md:17` / `snotra-core/src/config.rs` | env seam 側の契約。**Rust 側の変更はゼロ**。`config_dir_from` の非対称（上書きに `Snotra` を足さない）は本変更が依存する性質で、`config.rs:1200` のテストコメントが「検証スクリプトが渡した temp パスの下に更に階層ができ、seed が読まれない」と、まさにこの用途を守る形で固定している |
| `docs/superpowers/plans/**` / `specs/**` の 29 箇所 | 当時の計画・設計の記録。`docs/development-principles.md`「撤去（消す変更）の作法」の「ADR と設計書は当時の決定文脈ゆえ旧名のままでよい」に該当。**触らない** |

---

## 3. 間接参照の洗い出し

### 3.1 列挙の方法（母集団の宣言）

`docs/development-principles.md`「列挙の完全性」に従い、(a) 列挙は git 自身に問い、(b) `head` / `Select-Object -First` / `head_limit` で**切らない**。

```
git grep -n -E "SeedConfig|RequireResults|ResultsQuery" -- . ':!workspace'   → 60 行
git grep -c -E "SeedConfig|RequireResults|ResultsQuery" -- . ':!workspace'   → 9 ファイル
```

`':!workspace'` を付けるのは、本タスクが `workspace/plan*.md` の閲覧を禁じているためであり、`workspace/` は成果物ではなく作業領域ゆえ変更集合にも入らない。

**全 60 行（1 行も落とさない。長い行は読みやすさのため 200 桁で右を切っているが、行そのものは全数ある）**:

```
.github/workflows/e2e.yml:67:      # **startup smoke より前に置く**（#686）: `-SeedConfig` は config.toml **不在時のみ** seed する
.github/workflows/e2e.yml:72:      # この順序制約を守らせるのは規約ではなく `-RequireResults` である——skip を失敗に変えるので、
.github/workflows/e2e.yml:75:        run: npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig -RequireResults
docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md:33:**却下（重複を選ぶ）。** `smoke-egui.ps1` は `e2e.yml` の `-RequireResults` ゲートに載る CI 経路であり、
docs/build-commands.md:45:  - **CI に検証を委ねるなら、その job が実際に何を実行したかを確かめる**（#671 サイクルで実測: `Smoke` が 5 run 連続で緑のまま res…
docs/build-commands.md:159:- `scripts/smoke-egui.ps1` は egui 経路の自動回帰の最低線（#532 SU7・e2e/ 撤去後の後継）: `SNOTRA_TRACE=1` で起動 → keybd_event で hotkey（起動…
docs/build-commands.md:160:- `scripts/smoke-egui.ps1` は results 窓の表示も検査する（#671/#673 サイクル PR A）: `egui_show:done` の後、索引内容を制御できるときだけ 1 …
docs/build-commands.md:161:- **`-RequireResults` は skip を失敗に変える（CI 専用・#686）**: 既定の skip は「ローカルでは索引を制御できないのが普通」ゆえの緩…
docs/superpowers/plans/2026-07-25-646-pr2-results-window-split.md:779:Run: `pwsh -NoProfile -File scripts/smoke-egui.ps1 -HotkeyVks "17,75"`(実機 config は Ctrl+K。CI は既定 Alt+Q + `-SeedConfi…
docs/superpowers/plans/2026-07-25-pr-a-prime-results-window-newtype.md:29:3. **`npm run smoke:egui -- -ResultsQuery <letter>`（実機）** — PR A が構築した results 被覆の最初の顧客。
docs/superpowers/plans/2026-07-25-pr-a-prime-results-window-newtype.md:531:Run: `npm run smoke:egui -- -ResultsQuery <索引に当たる 1 文字>`
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:41:1. **`smoke-egui.ps1` は CI でも走る。** `.github/workflows/e2e.yml:67` が `npm run smoke:egui -- -ExePath ta…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:42:2. **`-SeedConfig` は config が既に存在するとき seed しない**（既存 config を決して上書きし…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:65:- Produces: 新パラメータ `-ResultsQuery <string>`（既定 `""`）。seed 済みでない環境で results 検…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:72:- [ ] **Step 2: `param()` ブロックに `-ResultsQuery` を追加する**
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:79:  #   -SeedConfig で実際に seed できた場合のみ "z"（seed した zsnotrasmoke.exe に一致）を使う…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:80:  #   seed しなかった場合（既存 config あり / -SeedConfig なし）は results 検査を skip する。
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:82:  [string]$ResultsQuery = ""
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:87:現行の `if ($SeedConfig) { ... }` ブロック（27-53 行）を次で置き換える。`$seededNow` は「この…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:91:if ($SeedConfig) {
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:98:    # 名前は既存の索引と衝突しにくい接頭辞にし、-ResultsQuery 既定の "z" で引けるよう…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:136:if ([string]::IsNullOrEmpty($ResultsQuery) -and $seededNow) {
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:137:  $ResultsQuery = "z"
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:153:    throw "ResultsQuery must be a single A-Z letter, got: '$Ch'"
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:166:  if ($failures.Count -eq 0 -and -not [string]::IsNullOrEmpty($ResultsQuery)) {
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:168:    $queryVk = Get-LetterVk $ResultsQuery
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:182:      $failures += "egui_results:show not observed within ${ObserveTimeoutMs}ms x2 after typing '$ResultsQuery'"
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:210:  Write-Host "NOTE: results window coverage was SKIPPED (no controlled index). Pass -SeedConfig on a machine withou…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:229:npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:358:npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:372:Run（config を退避せず、`-SeedConfig` も付けない）: `npm run smoke:egui -- -ExePath target/release/sno…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:847:**このステップは実機 config（hotkey = 実際の設定値）のまま実行する。** `-SeedConfig` も `…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:924:Run（config 退避 → 実行 → 復元。Task 2 Step 6 と同じ手順）: `npm run smoke:egui -- -ExePath targe…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:949:- [ ] `npm run smoke:egui -- -ExePath target/release/snotra.exe -SeedConfig`（config を退避した状態）— *…
docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md:962:- skip した検証と理由: 開発機（既存 config あり）での results 被覆は skip される。CI（`e2…
docs/superpowers/plans/2026-07-25-pr-b-read-visual-snapshot.md:40:4. `npm run smoke:egui -- -ResultsQuery <letter>`（実機・非回帰）
docs/superpowers/plans/2026-07-25-pr-b-read-visual-snapshot.md:525:Run: `npm run smoke:egui -- -ResultsQuery <索引に当たる 1 文字>`
scripts/smoke-egui.ps1:5:  [switch]$SeedConfig,
scripts/smoke-egui.ps1:14:  #   -SeedConfig で実際に seed できた場合のみ "z"（seed した zsnotrasmoke.exe に一致）を使う。
scripts/smoke-egui.ps1:15:  #   seed しなかった場合（既存 config あり / -SeedConfig なし）は results 検査を skip する。
scripts/smoke-egui.ps1:17:  [string]$ResultsQuery = ""
scripts/smoke-egui.ps1:22:  [switch]$RequireResults
scripts/smoke-egui.ps1:52:# - -SeedConfig（CI 用）: config.toml 不在時のみ最小の有効 TOML を seed し first-run 経路
scripts/smoke-egui.ps1:54:#   seed できたときは results 検証用の索引対象も 1 件同梱する（-ResultsQuery 既定の導出元）。
scripts/smoke-egui.ps1:65:if ($SeedConfig) {
scripts/smoke-egui.ps1:72:    # 名前は既存の索引と衝突しにくい接頭辞にし、-ResultsQuery 既定の "z" で引けるようにする。
scripts/smoke-egui.ps1:81:    # この smoke が e2e.yml の -RequireResults ゲートに載る CI 経路だからである（#803 で分離を判断）。
scripts/smoke-egui.ps1:114:if ([string]::IsNullOrEmpty($ResultsQuery) -and $seededNow) {
scripts/smoke-egui.ps1:115:  $ResultsQuery = "z"
scripts/smoke-egui.ps1:122:# 未観測 / `ResultsQuery` が A-Z 単字でない（`Get-LetterVk` が throw）。ゆえにこの 1 箇所で
scripts/smoke-egui.ps1:125:if ($RequireResults -and [string]::IsNullOrEmpty($ResultsQuery)) {
scripts/smoke-egui.ps1:127:results window coverage would be SKIPPED but -RequireResults was passed.
scripts/smoke-egui.ps1:128:  seeded now : $seededNow (-SeedConfig は config.toml **不在時のみ** seed する)
scripts/smoke-egui.ps1:132:-ResultsQuery <letter> に既存索引と一致する文字を渡す。
scripts/smoke-egui.ps1:150:    throw "ResultsQuery must be a single A-Z letter, got: '$Ch'"
scripts/smoke-egui.ps1:373:  if ($failures.Count -eq 0 -and -not [string]::IsNullOrEmpty($ResultsQuery)) {
scripts/smoke-egui.ps1:375:    $queryVk = Get-LetterVk $ResultsQuery
scripts/smoke-egui.ps1:400:      $failures += "egui_results:show not observed within ${ObserveTimeoutMs}ms x2 after typing '$ResultsQuery'"
scripts/smoke-egui.ps1:481:  Write-Host "NOTE: results window coverage was SKIPPED (no controlled index). Pass -SeedConfig on a machine without %APPDATA%/Snotra/config.toml, or pass -ResultsQuery <let…
scripts/visual-check-colors.ps1:93:# `scripts/smoke-egui.ps1` の `-SeedConfig` が同型の seed を持つ（必須セクションの根拠は共通・
```

ファイル別件数（**全数**・上のブロックの要約）:

| ファイル | 件数 | 分類 |
|---|---|---|
| `scripts/smoke-egui.ps1` | 22 | **対象**（定義そのもの） |
| `.github/workflows/e2e.yml` | 3 | **対象**（呼び出し + 順序制約コメント） |
| `docs/build-commands.md` | 4 | **対象**（規範） |
| `scripts/visual-check-colors.ps1` | 1 | **対象**（相互参照コメント・`:93`） |
| `docs/adr/ADR-config-dir-env-seam-rejected-alternatives.md` | 1 | **対象（追記のみ）**（`:33` の却下理由が現在形で偽になる） |
| `docs/superpowers/plans/2026-07-25-pr-a-smoke-coverage-and-hide-window-removal.md` | 24 | **対象外**（歴史的記録） |
| `docs/superpowers/plans/2026-07-25-pr-a-prime-results-window-newtype.md` | 2 | **対象外**（同上） |
| `docs/superpowers/plans/2026-07-25-pr-b-read-visual-snapshot.md` | 2 | **対象外**（同上） |
| `docs/superpowers/plans/2026-07-25-646-pr2-results-window-split.md` | 1 | **対象外**（同上） |

### 3.2 識別子の grep では届かない参照（**概念ラベル・序数・散文**）

識別子を消すだけでは腐りが残る箇所を、別の語で数え直した。

```
git grep -n -E "startup smoke より|順序制約|順序を入れ替え|入れ替えたら" -- . ':!workspace'
git grep -n -E "SKIPPED|索引内容を制御|自動 skip|skip され" -- . ':!workspace' ':!docs/superpowers'
git grep -n -iE "seed" -- . ':!workspace' ':!docs/superpowers' | grep -v SeedConfig
git grep -n "APPDATA" -- . ':!workspace' ':!docs/superpowers'
```

| 箇所 | 実際の文言 | 判定 |
|---|---|---|
| `docs/build-commands.md:45` | 「この 1 事例は `-RequireResults` が機構化した（#686・**下記**）」 | **同概念・別名（序数的な指し）＝対象**。「下記」は削除するブロックを指す。フラグ名で grep しても「下記」の指し先が消えることには到達しない。**この列挙で最も落としやすい 1 件** |
| `.github/workflows/e2e.yml:79` | 「（**順序を入れ替える前は** 1 回目が first-run だった）」 | **同概念・別名＝対象**。`-SeedConfig` も `-RequireResults` も含まないが、順序制約の存在を前提にした文 |
| `.github/workflows/e2e.yml:68-70` | 「seed が成立しないと results 窓の検証が自動 skip される」「**job は緑のまま results の検証が消える**」 | **同概念・別名＝対象** |
| `scripts/smoke-egui.ps1:130-131` | throw 文言中の「CI では、この smoke を config.toml を作る他のステップ（例: startup smoke のアプリ起動）より**前**に置けば seed が成立する」 | **同概念・別名＝対象**。**throw メッセージの中に規範が埋まっている**例（issue の指摘どおり） |
| `scripts/smoke-egui.ps1:47` | ヘッダの「（**索引内容を制御できるとき**）1 文字クエリを注入して」 | **同概念・別名＝対象** |
| `scripts/smoke-egui.ps1:371` | 「results 窓の検証（…）。**索引内容を制御できるときだけ**実行する。」 | **同概念・別名＝対象** |
| `scripts/smoke-egui.ps1:118` | 「results の検証が skip されるなら、ここで落とす（#686）」 | **対象**（ブロックごと削除） |
| `scripts/smoke-egui.ps1:66, 129, 481` | `$env:APPDATA` の 3 箇所（seed 先・throw 文言・NOTE 文言） | **対象**。`smoke-startup.ps1` には `APPDATA` の**明示的な参照が 1 つも無い**（実 config を暗黙に使っているだけ）——**識別子の grep では smoke-startup が変更対象だと分からない**。issue の「触る面」に載っていることと、共有リソースを踏む経路を概念で追ったことだけが根拠になる |
| `scripts/smoke-egui.ps1:107, 109` | `Write-Host "Seeded minimal config: …"` / `"Config already exists, seed skipped: …"` | **対象**（後者は概念ごと消える） |
| `docs/build-commands.md:189` | 「`skip-ci` を貼ってよいのは…**貼ってはならない**: … `scripts/**`」 | **同名・別概念＝対象外**。ここの「skip」は CI ラベルであって results 検査の skip ではない。ただし**運用上は当たる**——本 PR は `scripts/**` と `.github/workflows/**` を触るので `skip-ci` を貼ってはならない |
| `docs/build-commands.md:75` | 「**seed が読めたかは本体の stderr で確かめる**」 | **同概念・別名だが check:colors についての記述＝対象外**。ただし smoke 側に同じ判定を足すなら、規範の SSOT はここになる（写しを作らず参照する） |
| `scripts/visual-check-colors.ps1:84, 139-160, 289-304` | 掃除の理由 / `Test-SeedHealth` / `*.bin` の ∃ 検査 | **対象外（参照実装）**。ただし §2.1 の新規 2 判定はここを写すのではなく、**同じ理由で同じ形を再導出**したものであることをコメントで示す |
| `.githooks/githooks.test.mjs:54-56`, `scripts/clean-worktrees.test.mjs:34-36`, `snotra-core/src/search/tests/ranking.rs:241-243` | `seed.txt` / `seed: &[(i64, usize)]` | **同名・別概念＝対象外**（テスト fixture の「種」・ランキングの初期値） |
| `docs/adr/ADR-norm-review-seeding.md` ほか | 「種蒔き（フォールトインジェクション）」の seeding | **同名・別概念＝対象外** |
| `PERFORMANCE.md:83`, `docs/adr/ADR-results-presentation-two-stage.md`, `src-tauri/CLAUDE.md:49` ほか多数 | 「順序制約」「順序を入れ替え」 | **同名・別概念＝対象外**（show の高さ→位置→show 順、repaint の worker 生成順など。`e2e.yml` のステップ順とは無関係） |
| `snotra-core/tests/memory_footprint.rs:11`, `src-tauri/src/icon.rs:178-179` | `%APPDATA%` を実運用点として名指す記述 | **同名・別概念＝対象外**（実 config を**意図して**使う面。§1 のスコープ外宣言と対応） |
| `SPEC.md:212/616/620-635/654`, `docs/architecture.md:104` | `%APPDATA%\Snotra` の保存先記述 | **対象外**。`SPEC.md:616` が「本書で `%APPDATA%\Snotra` と表記するパスはすべてこの上書きに従う」と既に宣言しており、smoke の env 化はこの契約の**消費**であって変更ではない |

### 3.3 コンパイラを持たない機構での参照（全数）

| 機構 | 参照 | 検出器 |
|---|---|---|
| PowerShell（`smoke-egui.ps1` / `smoke-startup.ps1` / `visual-check-colors.ps1`） | param 名・throw 文言・`Write-Host` 文言・コメント | **無し**。`.ps1` は PostToolUse hook の `selectChecks` に検査が割り当てられておらず（`post-edit.mjs` に `scripts` の文字列が 1 つも無い）、`vitest.config.ts` の `include` は `scripts/**/*.test.mjs` のみ。**編集後の沈黙は「何も走らなかった」** |
| YAML（`e2e.yml` / `release.yml`） | 引数文字列・コメント・`paths` の自己参照 | **無し**（`e2e.yml` の `paths` にスクリプトが列挙されているかを照合する検査は `governance-check.mjs` に存在しない） |
| Markdown（`docs/build-commands.md` / `CONTRIBUTING.md` / ADR） | フラグ名・skip 経路の列挙・順序制約 | **ほぼ無し**。`G-stale-identifiers` の母集団は `.claude/{skills,rules,agents}/*.md` **だけ**で、述語は camelCase（`/^[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+$/`）。`-SeedConfig` / `-RequireResults` は先頭が大文字で、そもそも `docs/` は母集団外——**二重に届かない**。`G-ci-table` は「表の `npm run X` が workflow に現れるか」しか見ず、引数の腐りは見ない。`G-build-commands` は npm script の実在のみ |
| npm scripts（`package.json:13-14`） | `smoke:startup` / `smoke:egui` のラッパー | **変更不要**（引数を持たない）。ただし `G-ci-table` はこのラッパーのパス一致で `smoke:startup` 行を通しているので、**script 名を変えてはならない** |

**結論**: 本変更で腐りうる記述のうち、**機械が捕まえるものはゼロである**。`npm run governance:check` が緑でも「言及が全部直った」ことの証拠にはならない（参照実在・見出し着地・面積 ratchet は見る）。§3.1〜3.2 の手の列挙が唯一の網である。

---

## 4. 検証手順

`.ps1` に自動検査が無い（§3.3）ので、**すべて実測で接地する**。

### 4.1 issue の主張そのものを測る（これが本丸）

1. **`e2e.yml` の 2 ステップを入れ替えて 1 回だけ CI を回し、緑を観測する**（PR の一時コミットか `workflow_dispatch`）。#804 の看板は「順序制約が消えた」であり、**入れ替えて緑を見るまでは主張であって測定ではない**。確認後、diff を最小に保つため順序は元へ戻す
2. **実 config を持つ開発機で `npm run smoke:egui`（引数なし）** → results 検査が走り、`results window coverage was SKIPPED` の NOTE が**出ない**。これは変更前には原理的に観測できなかった状態で、before/after が最も明瞭に出る 1 点

### 4.2 受け入れ条件 A〜D の直接観測

| 条件 | 測り方 |
|---|---|
| A. 実 config 不可侵 | 両 smoke の前後で `Get-ChildItem $env:APPDATA/Snotra \| Select-Object Name,LastWriteTime` が一致する（**両スクリプトそれぞれで**測る。smoke-startup は今日 5 起動で `index.bin` / `window.bin` を実際に書いている） |
| B. 履歴非依存 | `smoke-startup` → `smoke-egui` の順で走らせて両方緑。続けて逆順でも両方緑。さらに `smoke-startup` を 2 回連続で走らせて**出力（`event_count` を除く判定部分）が同一** |
| C. skip 経路の不在 | ソース上 `$resultsChecked` / `IsNullOrEmpty($ResultsQuery)` の分岐が 1 つも残っていないこと（`git grep -n "resultsChecked\|ResultsQuery" scripts/` が 0 件）。**列挙ではなく不在で示す**。ただしこれはレビュー時点の検査であって機構ではない——**将来また「索引に依存する skip」を書き足す変更を止める検出器は無い**（`.ps1` は hook 対象外・vitest 対象外・§3.3）。それを受容できるのは、`-RequireResults` が守っていた**状態そのもの（実 config の有無で seed が成立しない）が消える**からであって、ガードで押さえるからではない。`docs/build-commands.md:45` の書き換えではこの点を書く（「何が `-RequireResults` の代わりをするのか」への答えがこれである） |
| D. 隔離の肯定的証拠 | 実行後に `target/smoke-egui/profile` / `target/smoke-startup/profile` へ `*.bin` が在ること。**どの `.bin` が出るかを実測して**スクリプトの判定とコメントへ書く（visual-check の `index.bin` を推論で流用しない） |

### 4.3 フォールトインジェクション（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）

**稼働中のガードを弱めない**——スクリプトを一時コピーして変異させる。

1. **env が効かない場合**: コピーへ `$env:SNOTRA_CONFIG_DIR` の代入を消す変異を当て、`*.bin` の ∃ 判定が**赤くなる**ことを見る。赤くならなければ判定 D は空振りしている（実 config 側に `.bin` があっても、見ているのはプロファイル配下なので赤になるはず——これも実測で確かめる）
2. **seed が壊れた場合**: コピーの seed TOML から `[hotkey]` セクションを落とし、`[config] ` 行の検知が**赤くなる**ことを見る（`ADR-config-dir-env-seam-rejected-alternatives`「5.」が根拠づけた観測点の行使）
3. **掃除が効いていること**: 2. の実行後に生じた `config.toml.bak` が、次の正常実行の**開始時に消える**こと（残ると判定が古いファイルで空振り合格する）

### 4.4 隣接する消費者の非回帰

- `.github/workflows/release.yml:83` — 引数は変えないが、走る条件が変わる（実 config・first-run → 自前プロファイル・seed 済み）。**リリース workflow を通す前に、同じコマンド行をローカルで release バイナリに対して実行**して緑を見る
- `npm run check:colors` — `visual-check-colors.ps1` のコメントを触るので、実行して非回帰（プロファイル・判定の形は変えない）
- **stray プロセスの確認**: `Get-Process snotra-settings` が実行後に**存在しないこと**。`Get-Process snotra` は `snotra-settings` にマッチしない（ワイルドカード無しの `-Name` は完全一致）ので、first-run を踏むとどのスクリプトも掃除しない子プロセスが残る。seed によってそもそも踏まないことの裏取りになる

### 4.5 文書・ガバナンス

- `npm run governance:check`（カテゴリ F・`docs/**` と `.github/workflows/**` を触るため必須）。期待は緑で、面積 ratchet の方向は**純減**
- `npm test`（`scripts/**` を触るため。ただし `.ps1` を検査するテストは無い＝ここでの緑は「壊していない」しか意味しない）
- `skip-ci` ラベルを**貼らない**（`docs/build-commands.md:189`——`scripts/**` は貼ってはならない側）
- `$env:SNOTRA_CONFIG_DIR` が実行後に残っていないこと。**対話 pwsh から `pwsh -File scripts/…` を直接叩いて測る**（`npm run` 経由は子プロセスなので、漏れていても親では観測できない＝空振りの合格になる）

---

## 5. リスク・落とし穴

1. **`Resolve-Path` の順序と絶対化の必須性**。`config_dir_from` は展開も絶対化もしない。`New-Item -Force` で dir を作る**前**に `Resolve-Path` を呼ぶと throw、呼ばずに相対パスを env へ入れると**本体の CWD 起点**にデータが落ちて、スクリプトが見ている場所には何も現れない（判定 D が赤くなるので沈黙はしないが、原因を探す時間を失う）。`visual-check-colors.ps1:80/131` の順序をそのまま採る。
2. **`try/finally` を足したくなる誘惑**。狭い env 窓が安全である根拠は 2 段ある。**(a) 子は生成時に環境ブロックをコピーする**（`Start-Process` はストリームをリダイレクトする＝ `UseShellExecute=false` の経路で、親の環境の**写し**を渡す）。ゆえに次の行で親を戻しても子には届かない。**(b) 本体は `Config::config_dir()` を呼ぶたびに `std::env::var_os` を読む**（`config.rs` の `config_dir()` は毎回 `var_os(ENV_CONFIG_DIR)` を評価する）——読むのは**子プロセス自身の写し**なので、起動後の再読込（config_watcher）でも、本体が spawn する `snotra-settings`（親は本体であってシェルではない）でも、プロファイルを指し続ける。**(b) を書かないと「起動後の動作には狭い窓では足りないのでは」と押し戻される**。env が立っている区間に throw する行は無い。ここに `finally` を置くと「判定ロジック全体を包む巨大な `try`」へ育ちやすく、既存の `SNOTRA_TRACE` の扱い（`smoke-egui.ps1:211-219`）と非対称になる。**機序をコメントに書かないと、レビューで必ず逆方向へ押される**。
3. **smoke-startup を seed し忘れる**（issue が smoke-startup で何を変えるか書いていないため最も踏みやすい）。空プロファイル → first-run → `snotra-settings.exe` の有無で挙動が二分岐（CI: 既定 scan パスの索引構築へフォールバック / リリース runner・開発機: 設定 GUI が起動してフォーカスを奪う）。しかも 2 回目以降の invocation では起きない＝**再現しない失敗**になる。
4. **`*:error` の母集団が今日ゼロである**（実測: trace を出す 2 crate（`src-tauri/src` / `snotra-egui-runtime/src`）に `*:error` で終わる trace イベント名は 1 つも無い。動的に組み立てたイベント名も、同じ 2 crate では見つからなかった——`trace_main` / `trace_command` の引数はすべてリテラルである。他 crate は未測）。したがって smoke-startup が実際に守っているのは「trace が 1 件以上出る」（#690）であって `*:error` 不在ではない。**「first-run を踏んでも赤くならないから seed は不要」と推論しない**——赤くならないことは正しいが、それは検査が薄いからであって安全だからではない。
5. **∃ 判定を足すなら掃除が前提**。`*.bin` の存在確認は、前回実行の残骸があると**常に緑**になる。`visual-check-colors.ps1:82-85` が同じ罠を明記している。掃除と判定は 1 つの変更で入れる。
6. **`docs/build-commands.md:45` の「下記」**。フラグ名の grep では到達しない指し先。ここを落とすと、削除したブロックを指す文が残る。
7. **行番号で編集しない**。issue の `docs/build-commands.md:147` は現在 `:161` を指しているが、`:147` には今 `cargo run -p snotra` の行がある——**そのまま開くと無関係な行に当たる**。`smoke-egui.ps1` の 3 つは `:66-67`（seed 先）と `:125`（`-RequireResults` guard）が一致し、`:477`（skip NOTE）だけが 4 行ずれている（実体は `:481`。`:477` は `if ($resultsChecked)` の行）。**当たっている引用が混じることが、行番号を信じてよい根拠にはならない**——見出し・シンボル名で特定する（`docs/development-principles.md`「撤去（消す変更）の作法」）。
8. **`-SeedConfig` / `-ResultsQuery` の撤去は issue の literal からの逸脱である**。issue の「消えるもの」は `-SeedConfig` の**条件分岐**までしか書いていない。撤去まで進めるなら、費用（`docs/build-commands.md` 2 行 + `visual-check-colors.ps1:93` + ヘッダ散文の書き直し）と便益（選ぶ理由の無い実行モードと、必ず失敗する引数を消す）を PR 本文に書いて**明示的に承認を取る**。逆に「switch は残して常に seed」を選ぶ場合、`-SeedConfig` を付けない実行は空プロファイル＝ first-run になるので、**その経路の挙動を決めて書く義務が生じる**（黙って残すのが最悪）。
9. **消す変更の中で新しく書く 1 文は、削除ではなく新規記述である**（`docs/development-principles.md`）。「両 smoke は互いの実行順に依存しない」「実 config は読みも書きもしない」は**全称の主張**であり、書いた以上は §4.2 の A・B で測ってから書く。特に「実 config に触れない」は、`Get-Process snotra` による**実インスタンスの kill**（`smoke-egui.ps1:202`・`smoke-startup.ps1:42`）が残ることに注意——データには触れないが、ユーザーが起動中のアプリは落とす。「触れない」の射程を config データに限定して書く。
10. **seed の写しが 3 つになる**。`AGENTS.md`「文書に事実の写しを増やす変更」と DRY の「3 回目で抽出を検討」に当たる。抽出しない結論でよいが、**検討した記録を ADR へ残さないと、次に触る人が同じ検討を最初からやる**（そのために既存 ADR が在る）。
11. **新規ファイルを足すなら `e2e.yml` の `paths` へ**。`paths` はスクリプトを個別列挙しており（`:21-22`）、`scripts/**` ではない。漏れても検出器が無く、**そのファイルだけを直した PR で smoke が回らない**（`e2e.yml:15-17` が `snotra-egui-runtime` で実際に踏んだ形と同型）。共有ヘルパーを作らない結論はこのリスクも下げる。
12. **`Get-LetterVk` の throw が到達不能になる**。残すなら「到達しないが将来のための検査」ではなく、単に畳む（`docs/development-principles.md`「7. 失敗方向は既定値に埋め込む」の裏返しで、**到達しない検査は読み手に誤った安心を与える**）。文書側 `:161` の「クエリが A-Z 単字でない」列挙と同時に決める。
