# research — issue #1239: rust-toolchain.toml の 1.98 固定で rust-analyzer component が無く、Claude Code の LSP が exit 1 で落ちる

調査日: 2026-09-06 / ブランチ: `fix/rust-analyzer-toolchain-component`

## issue の要約

- `LSP` ツールが `plugin:snotra-rust-lsp:rust-analyzer crashed with exit code 1` で毎回失敗する。`~/.cargo/bin/rust-analyzer` は rustup のプロキシで、override された 1.98 toolchain に `rust-analyzer` component が無いため `Unknown binary 'rust-analyzer.exe'` で即終了する（2026-09-06 実測）
- issue の対処案: `rust-toolchain.toml` の `components` に `"rust-analyzer"` を足す。受け入れ条件: `LSP findReferences` が結果を返す・`rustup show` で 1.98 に component が入る・CI 緑

## 調査で判明した前提の変化（issue 本文には無い）

1. **この失敗は既知で、#1177（2026-08-25・commit `8fa8b5f`）が「意図して外す」と決めていた。** `.claude/skills/deps-update/SKILL.md` L40〜46: 「`channel` を上げたら `rustup component add rust-analyzer` を打つ。自動取得されるのは `components` に挙がったものだけであり、rust-analyzer はそこに無い（**足すと CI も毎 run 取りに行くため、意図して外してある**）。…入っていないと findReferences が使えず、しかもどの検査も赤くならない（2026-08-24 に新しい toolchain が実体化して以降、11 時間気づかれなかった・実測）」。つまり採られたのは**機構ではなく引き金**（`/deps-update` の手順 1 行）である
2. **引き金は今回効かなかった。** 本日の `/deps-update`（PR #1238）は「最新 stable 1.98.1 は channel `1.98` の範囲内のため `rust-toolchain.toml` は変更なし」と判断し、`channel` を上げていないので手順が発火しない。**しかしローカルの 1.98 toolchain は本日 13:07 に 1.98.0 → 1.98.1 へ丸ごと入れ替わっている**（`bin/` 配下の全ファイルの CreationTime が 2026-09-06 13:07・`multirust-channel-manifest.toml` の `date = "2026-09-03"`・`rustc +1.98 --version` = 1.98.1）。`lib/rustlib/components` は 5 つ（rustc / cargo / rust-std / rustfmt / clippy）で rust-analyzer が無い。**`channel = "1.98"` はパッチ版を追う channel 名なので、`channel` の行が変わらなくても toolchain は更新される**——引き金の条件（「`channel` を上げたら」）はこの経路を覆っていない
3. **「足すと CI も毎 run 取りに行く」は、現状では追加費用の根拠として弱い。** 本日の CI（run 34020570400・rust-check）のログ: `syncing channel updates for 1.98-x86_64-pc-windows-msvc` → `downloading 5 components`。runner にプリインストールされた `stable` は 1.98.1 だが、pin された `1.98` は別名の toolchain なので**毎 run 5 component を丸ごと取得している**（Setup Rust toolchain ステップは 26 秒）。rust-analyzer を足すと 6 component になるだけで、「取りに行く」こと自体は既に起きている
4. ⚠️ 1.98.0 の時点で rust-analyzer が入っていたか（= 更新で落ちたのか、最初から無かったのか）は確定できない。rustup の update は既存 component を保つ（1.98.1 更新後も 5 個ゆえ、1.98.0 にも無かった可能性が高い）。#1177 の「2026-08-24 に実体化」した toolchain がどれかを示す一次資料（当時のセッション transcript）は手元に無い

## 関連ファイル・モジュール・関数（実在を確認済み）

| ファイル | 役割 | 触るか |
|---|---|---|
| `rust-toolchain.toml` | `channel = "1.98"`・`components = ["rustfmt", "clippy"]`。ヘッダコメント L17「`components` をここに書くのは `ci.yml` の component 明示（#858 / #859）と同じ要求」 | **触る**（component 追加と理由のコメント） |
| `.claude/skills/deps-update/SKILL.md` L40〜46 | 手動の `rustup component add rust-analyzer` 手順と「意図して外してある」の理由・再起動の注意 | **触る**（手順を「toml が保証する」形へ改め、再起動と索引の温まりの注意は残す） |
| `.github/workflows/ci.yml` L150〜161 | `dtolnay/rust-toolchain@stable` + `components: rustfmt, clippy`。L157 コメント「override 後に実際に走る toolchain の component は `rust-toolchain.toml` の `components` が決める（そちらが正本）」 | 触らない（正本は toml。ただし CI 側の `components:` は preinstalled `stable` 用で、pin 側は toml が決める。rust-analyzer を CI の `components:` に足す必要は無い） |
| `.claude/lsp/snotra-rust-lsp/.lsp.json` | `"command": "rust-analyzer"`（PATH の rustup shim を呼ぶ）。`args` 欄は無い | 触らない |
| `.claude/hooks/lsp-config.mjs` `checkLspConfig` | LSP 設定の不変条件（ratoml・配送経路・`initializationOptions`）。**バイナリの実在は見ない** | 選択肢 B で触る（下） |
| `.claude/hooks/lsp-config.test.mjs` | 実リポジトリの緑 + 故障注入 15 本（複製に当てる） | 選択肢 B で触る |
| `docs/hooks.md` L69〜98「Claude Code の RA インスタンスと hook の分担」 | 沈黙する壊れ方の分類表（L89〜90）。「バイナリ不在」は表に無い | **触る**（表へ 1 行: 「component 不在」は沈黙する側・守り手は toml の `components`） |
| `docs/adr/ADR-claude-code-ra-lsp-plugin-delivery.md` | 配送経路の決定と却下案 | 触らない（凍結） |
| `docs/build-commands.md` L13 | 「コマンドが走る Rust の版は `rust-toolchain.toml` が決める」 | 触らない |

## 再利用できる既存パターン

- `rust-toolchain.toml` の `components` が「runner のプリインストールに暗黙依存しない」ための名指しである、という #858/#859 と同じ要求（toml ヘッダ L17〜20）。rust-analyzer を同じ要求で足す
- `lsp-config.mjs` の「複製に変異を当てる」故障注入テストの型（選択肢 B の検知器を足すならこの型）
- `.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」

## 技術的制約

1. **`rust-toolchain.toml` の `components` は rustup が toolchain を解決するたびに保証する**（未インストールなら取得）。toml を編集した後、`cargo` / `rustc` を 1 回叩けば `info: installing component 'rust-analyzer'` が出て入るはず——**実装時に実測する**（rustup 1.29.1）
2. **CI の追加費用**: 1.98 は毎 run 取得（前提の変化 3）。rust-analyzer component 1 つ分のダウンロード（数十 MB・数秒）が乗る。`Rust cache`（Swatinem 等）は target を対象にし toolchain は覆わない（ログ実測: 毎 run `downloading 5 components`）
3. **セッション側の復帰条件**: 「セッションが既に LSP の起動失敗を数え切っていると、component を入れてもそのセッションでは復帰しない。Claude Code を再起動すれば戻る」（#1177 実測）。受け入れ条件 1（`LSP findReferences` が結果を返す）の実測は**再起動後**に行う必要がある
4. **壊れ方は沈黙する**: component が無くても cargo も clippy も test も緑で、LSP の失敗は `LSP` ツールを呼んだ時だけ error が出る。`pluginUsage` の 0 も報せない（診断は `checkOnSave: false` で意図的に切ってある）
5. `lsp-config.mjs` に「toml の `components` に `rust-analyzer` が在るか」を足す場合、母集団は `rust-toolchain.toml` 1 枚（`fs.readFileSync` + `#` コメント除去 + 正規表現）。**`rustup component list` のような環境の実測は CI（runner に無い）で赤になるので置けない**——検査できるのは「保証する宣言が toml に在るか」までで、「実際に入っているか」は射程の外（宣言は在るが rustup が失敗した形は沈黙する。受容する残余として書く）

## 選択肢

- **A: toml の `components` へ足す + 文書 2 枚（deps-update / hooks.md）を直す。** 機構は rustup 自身。引き金（手順）は撤去し、再起動の注意だけ残す
- **B: A + `lsp-config.mjs` に「toml が rust-analyzer を宣言しているか」の検査を足す**（故障注入 1 本つき）。守るのは「誰かが `components` から外す」形だけ。壊れ方が沈黙する（制約 4）ので `.claude/rules/safety-nets.md`「足す前に『壊れたとき緑が緑のまま推移するか』を問う」の答えは「推移する」＝機構を置く側に倒れる。費用は関数 1 つ・テスト 2 本
- **C: 何も変えず `rustup component add` を手で打つ**（#1177 の現状維持）。引き金がパッチ更新を覆わないことが今回判明したので却下候補

推奨は **B**。A との差は検査 1 本で、「宣言が消える」形だけを塞ぐ（「宣言は在るが入っていない」は残余）。

## 未解決の疑問

1. 選択肢 A / B のどちらか（要求判断・人間へ）
2. ⚠️ 1.98.0 に rust-analyzer が入っていたか（前提の変化 4）——結論は変わらない（どちらでも toml が保証する形へ移す）が、#1177 の記述「再起動すれば戻る」の実測が今も真かは実装時に確かめる
3. `rustup` が toml の `components` 追加を既存 toolchain へ即時反映するか（制約 1）——実装時に `cargo --version` の出力で実測

## 敵対的調査（3b）の所見と採否

枠: general-purpose / sonnet ×1。出力 `workspace/adversarial-1239.txt`。

| 所見 | 判定 | 採否 |
|---|---|---|
| 壊せた項目: **0 件** | — | — |
| 壊せなかった: toml の `components`・1.98 = 1.98.1 で 5 component・CreationTime 13:07・CI run 34020570400 の `downloading 5 components` / 26 秒・**追加 2 run でも 1.98 側は毎回ダウンロード**（stable 側はキャッシュで出ない回あり）・`8fa8b5f` の内容・PR #1238 は toml 無変更・`checkLspConfig` はバイナリ実在を見ない・`docs/hooks.md` の表に「component 不在」が無い・`settings.json` の有効化状態 | 実測で裏付き | 維持。「CI は毎 run 取得」は 3 run で強化 |
| 未検査: rustup が toml の `components` 追加を既存 toolchain へ反映するか（環境を変えないため） | — | **主エージェントが scratch ディレクトリで実測**（下） |
| ⚠️ 「1.98.0 → 1.98.1 へ丸ごと入れ替わった」は CreationTime からの推定 | 中確信 | 採る（機序は「本日更新された」までに弱め、結論には影響しない） |
| ⚠️ 「Rust cache は toolchain を覆わない」はログからの傍証 | 中確信 | 採る（3 run とも `downloading` が出ていることが実効的な根拠） |

### 主エージェントの実測（2026-09-06・未解決の疑問 3）

scratch ディレクトリ（リポジトリ外）に `channel = "1.98"` / `components = ["rustfmt", "clippy", "rust-analyzer"]` の `rust-toolchain.toml` だけを置き、そこで `cargo --version` を 1 回実行 → 1.98 toolchain に `rust-analyzer-x86_64-pc-windows-msvc` が入り、`rust-analyzer --version` = `rust-analyzer 1.98.1 (48a229ce 2026-09-01)`。**rustup 1.29.1 は toml の `components` 追加を既存 toolchain へ、次の呼び出しで反映する**（技術的制約 1 は真）。この時点でローカルの 1.98 には component が入っている（リポジトリの toml はまだ触っていない）。

**測定環境の注意**: Bash ツールのサンドボックス内では rustup の HTTPS 取得が `received corrupt message of type InvalidContentType` で失敗した（`static.rust-lang.org`）。サンドボックスを外して再実行すると成功。`/implement` で toml を編集した後の実測も同じ条件が要る。
