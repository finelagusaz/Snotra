# ビルド・実行コマンド

**環境を確認の上、実行してください。**

このドキュメントは Snotra のビルド／検証コマンドの単一の情報源（SSOT）です。`AGENTS.md` の開発ワークフローや `.claude/skills/*/SKILL.md` の検証ステップは、コマンド本体をここに集約して参照します。コマンドを追加・変更するときはこのファイルのみを更新してください。

## 変更後の検証チェックリスト（必須・スキップ不可）

変更したファイルの種類に応じて、以下のカテゴリの必須コマンドを実行する。複数カテゴリに該当する場合はすべて実行する。

### A. Rust ファイル（`*.rs`）を変更した場合

```bash
cargo fmt --all -- --check                                                             # 必須: 整形（#858・修復は `cargo fmt --all`）
cargo check --workspace                                                                # 必須: Rust 全 crate 型チェック
cargo clippy --workspace --all-targets -- -D warnings                                 # 必須: lint（全 .rs 変更、テストターゲット含む）
cargo test -p snotra-core                                                              # 必須（snotra-core を変更した場合）: 純ロジック層 TDD
cargo test -p snotra-egui-runtime                                                      # 必須（snotra-egui-runtime を変更した場合）: 入力・IME・Surface の描画失敗リトライ方針
cargo test -p snotra                                                                   # 必須（src-tauri を変更した場合）: Tauri 統合層のユニットテスト
cargo test -p snotra-settings                                                          # 必須（snotra-settings を変更した場合）: 設定 GUI の純ロジックテスト
cargo doc --workspace --no-deps --document-private-items                               # 必須: intra-doc link 切れ検査（#562・CI 発火／hook 非発火）
```

- **`cargo test` の必須/任意**: 変更した crate のテストはローカル**必須**（PostToolUse フックが自動実行）。変更していない crate のテストはローカル任意（CI の rust-check が PR で全 4 crate のテストを常に実行し担保）
- **`cargo fmt` は検査（`--check`）と修復（`cargo fmt --all`）で別コマンドである**（#858）。ローカルでは PostToolUse フックが検査を自動発火するため、赤が届いたら `cargo fmt --all` を実行して直す——**差分を読んで手で直さない**（機械が持つ判定を人が写す作業になる）。整形の様式は rustfmt の既定であり `rustfmt.toml` を置かない（既定が drift 最小であることを 7 設定の比較で実測・`docs/adr/ADR-rustfmt-gate.md`）
- 上記のコマンドはいずれも CI（`ci.yml` rust-check）で PR 自動実行される（「CI/CD メモ」の対応表参照）。PostToolUse フック（`.claude/hooks/post-edit.mjs`）も `*.rs` 編集で fmt と clippy、`snotra-core/**` / `snotra-egui-runtime/**` / `src-tauri/**` / `snotra-settings/**` 編集でその crate のテストを自動発火する。`Cargo.toml` の編集では `cargo check` を自動発火する（ルートの `Cargo.toml` ではさらに hook-selftest = members カナリア）
- **`check` / `clippy` は `--workspace` を使う**（#500）。crate 名を `-p` で列挙すると `Cargo.toml` の `members` の写しになり、5 つ目の crate を追加したとき hook・CI・本ファイルが同じ誤りを共有して気づかれないまま漏れる。`--workspace` は cargo に SSOT を読ませる。一方 `cargo test -p <crate>` は「編集した crate → そのテスト」の写像なので `-p` のまま残す（`--workspace` にすると編集していない crate のテストまで走る）
- **`cargo doc` は CI（rust-check）でのみ発火し、PostToolUse フックは発火しない**（#562・編集レイテンシ回避の設計判断）。deny 化は各 crate の `[lints] workspace = true`（`Cargo.toml`）→ root `[workspace.lints.rustdoc]`（`broken_intra_doc_links` / `invalid_html_tags`）で、既定 warn の素通りを塞ぐ。この 2 段（member 側の opt-in 漏れと、root 側の deny の降格・欠落）はどちらも cargo が exit 0 で沈黙するため、`npm run governance:check`（G-workspace-lints）が検知する（#713）。**沈黙は合格を意味しない**（hook 対象外）ため、doc コメント（`///` / `//!`）を触ったらローカルで上記コマンドを手動実行してリンク切れを確認する
- **フックの検査コマンドと本ファイルの整合規約**: フックの cargo コマンドは、**カテゴリ A のコードブロック**の記載と**合否・検査対象を変えるフラグにおいて一致**させる（`--lib` の付与・`-p` の欠落等を乖離とする）。**出力整形のみのフラグ**（`--message-format short` 等、exit code を変えないもの）は hook 側の証拠予算のための追加として許容する。npm 系検査は SSOT コマンド（`npm test`）の部分集合ラッパー（対象ディレクトリ限定の vitest 実行）を許容する。コマンドの実在は `npm run governance:check`（G-build-commands）が、cargo フラグの乖離は同（G-hook-commands・#589）が検知する。npm 系ラッパーの等価判断のみ `/health-check`（Check 5 残置部分）に残る
- **検査が割り当てられているファイルでは、フックの沈黙は合格を意味する**（#471・前提条件は #497）。検出は exit code で行い、成功した検査は何も出力しない。失敗時のみ再現コマンド付きで会話に届くため、そのコマンドを実行すれば全診断を見られる。**割り当ての無いファイル**（`*.md`・`scripts/`・`.github/workflows/` 等）の沈黙は「何も走らなかった」であり合格ではない。割り当ての SSOT は `post-edit.mjs` の `selectChecks` である
- `snotra-settings` を含めるのは egui ネイティブウィンドウ側の型壊れも検知するため

### B. TypeScript ファイル（`vitest.config.ts` 等）を変更した場合

TS の型検査は #532 SU7 のフロント撤去で消滅した（`tsconfig.json` ごと削除・`.ts` 編集時は PostToolUse フックが「検査はありません」の情報行を出す）。残る `.ts` は `vitest.config.ts` のみで、その変更は hook-selftest（PostToolUse 自動発火）と `npm test` が検証する。

### C. ウィンドウ生成／表示順・ホットキー・スラッシュコマンドに触れた場合（A／B に追加）

```bash
npm test                    # 必須: ユニットテスト（Vitest: .claude/hooks + .githooks + scripts）
npm run test:powershell     # 必須: Pester（共有 smoke 配管 + 実バイナリ統合）
npm run smoke:startup       # 必須: 起動時スモーク（trace 出力・検証用プロファイル・非 first-run の検証）
npm run smoke:egui          # 必須: egui show/hide スモーク（hotkey 注入 + trace 検証・#532 SU7）
```

- `test:powershell` は Pester 6.0.1 を `target/pester/` へ固定取得し、グローバル環境を変更しない。統合テストの本体は `cargo metadata` の `target_directory` から導くため `CARGO_TARGET_DIR` に追随する。未ビルドなら先に `cargo build -p snotra` を実行する（別の本体を検査するときだけ `npm run test:powershell -- -ExePath <path>`）
- **実バイナリを起動する検査の前に `cargo build -p snotra` を打つ。`cargo test` では更新されない**——`cargo test` がビルドするのはテストターゲットであって `target/debug/snotra.exe` ではない。危ないのは「未ビルド」より**古いまま在る**ほうで、検査は成功したまま**変更前の挙動を測って赤や緑を出す**（#835 で 2 時間前のバイナリを測り、直したはずの伸縮を FAIL として観測した）
- WebView2 E2E（Playwright + tauri-driver）は #532 SU7 flip で撤去済み。後継は `smoke:egui`（自動回帰の最低線）+ 手動 GUI smoke（カテゴリ D）
- **PR 上の実行責任**: `npm test` は通常 PR CI（`ci.yml`）で自動実行されるが、`smoke:startup` / `smoke:egui` は**通常 PR CI では走らない**。`src-tauri`・`snotra-egui-runtime`（描画ループ所有・#701 で追加）・依存 manifest/lockfile 等を含む変更は `Smoke` workflow（`e2e.yml`）が **paths により自動起動**し両 smoke を実行する。paths 外の変更で回したいときは `workflow_dispatch`（手動実行）。「通常 CI が緑」だけでは smoke 済みを意味しない
  - **CI に検証を委ねるなら、その job が実際に何を実行したかを確かめる**（#671 サイクルで実測: `Smoke` が 5 run 連続で緑のまま results の検証を skip していた）。この 1 事例は #686 が `-RequireResults` で機構化し、#804 が「results 検査を無条件に要求する」形へ格上げした（「スモーク運用メモ」節）が、**「緑」が「検査が走った」を意味しない形は他にも作れる**——委ねる前に、その job のステップと渡す引数を読む

### D. UI のスタイル・レイアウト・テキスト表示に影響する変更（A／B／C に追加）

`cargo run -p snotra` で起動し、目視で overflow／clipping／フォントレンダリングを確認する。PR 作成前に必須。

```powershell
npm run smoke:manual              # 項目の読み上げ・判定の記録・trace の並置（#749）
npm run smoke:manual -- -Only 2,5 # 一部だけ再実施
npm run smoke:manual -- -PostToPr # 記録を PR コメントへ投稿する
```

- **trace には性質の違う 2 つが載る。混ぜて読まない**（#757）。**presence（イベントが出たか）は診断であって合否ではない**（#671 PR A′: `egui_results:hide` は出たのに窓は残り、presence を見る smoke は緑のまま通した）。一方 **H1 / H4 / H5 の不変条件は「起きてはならないことが起きていないか」を見るので合否を名乗れる**——判定は `scripts/lib/SnotraTraceInvariants.psm1`（Pester で実測。`smoke:egui` の orphan 検出と同じ導出を共有する）
- **項目の合否は目視と trace の両方が決める。** trace が緑でも目視が赤なら赤であり、逆も同じ。記録は不一致を専用の節で名指しする
- **SKIP は「判定できなかった」であって合格ではない。** 「該当イベントが無い」「`rows` が読めない」「main の可視状態が未観測」「hide 窓が閉じていない」「parse できなかった行がある」はすべて SKIP として現れ、理由が記録に併記される
- **`smoke:manual` はエージェントが実行できない**（`Read-Host` で判定を採るため）。人間が自分の端末で走らせる。実施の有無が会話にしか残らないと「検証されていない」と「問題が無かった」が区別できなくなるため、`-PostToPr` か出力ファイルの貼り付けで PR に残す。**カテゴリ D 全体がそうではない**——`check:colors`（下記）は exit code で自動判定するのでエージェントが実行でき（**画面ロック中を除く**・下記）、目視項目も打鍵注入 + 窓矩形キャプチャで実施した実績がある（#836 / #870）
- **`$items` の常設 13 項目が SSOT であり、PR 本文の目視表とは別の母集団である**（写しではない・`docs/adr/ADR-folder-location-display-surface.md`「却下 6」）。`$items` に載るのは**どの変更でも壊れうる横断不変条件**、PR 本文の表はその PR 限りの受け入れ確認。**新機能のために先回りで `$items` へ足さない**——足す条件は「その表示が実際に一度回帰したとき」である

- **既定が egui（#532 SU7 flip 済み・env フラグ不要）**。`cargo run`（`-p` 欠落）は**ルートでは bin を決められずエラーになり**（`snotra` / `snotra-settings` の 2 本。実測: `error: cargo run could not determine which binary to run`）、cwd が crate 配下ならその crate の bin が起動する。必ず `-p snotra` を付ける

#### `[visual]` の色を変える変更は、**非既定色で**目視する

```powershell
npm run check:colors                      # 自動判定: main/results の背景ピクセルを実測し exit code で返す
npm run check:colors -- -Color '#FFF'     # 3 桁 hex の受理（#680 の 1・パーサ統合の回帰）
npm run check:colors -- -Interactive      # 判定せず起動し、目視項目を読み上げる
```

- **CI では走らない。** GUI を要するため `ci.yml` にも `e2e.yml` にも入っておらず、明示的に起動したときだけ動く（**エージェントも起動できる**——下記のロック中を除く）。ゆえに `[visual]` の色に効く変更は、**CI が緑でも未検証である**
- **既定色での確認はこの検証にならない。** config の既定 `#282828` は `snotra-egui-runtime` の `CLEAR_COLOR` と一致するため、色が届いていなくても正常に見える（原理は `docs/development-principles.md`「config の値は到達性の検出器を持たない」）
- **自動判定するのは main と results の定常背景である。** results は専用 scan 3 件を seed し、キー注入で表示して実ピクセルを測る。残る 2 点は目視（`-Interactive`）に留まる——**show の一瞬のフラッシュ**は present 前の 1 フレーム未満で連写しても捉えられず、**results のリサイズ時のちらつき**は入力と描画のタイミングに依存して単一キャプチャでは不在を証明できない
- **trace は判定に使わない。** 「`set_clear_color` を呼んだ」というログは、その色が画面へ出たことを意味しない（`src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」）。判定の根拠は描かれたピクセルだけである
- **画面がロックされていると実行できない**（#866）。ロック中は窓が可視のままでも画面に合成されず、`CopyFromScreen` は**ロック画面の中身を持つ有効な Bitmap** を返す（`IsWindowVisible` も矩形も DPI も真っ当な値なので、他のガードは全部通る）。`Get-SnotraWindowCapture` が起動前に名指しして止める。**ロック状態を判定できなかったときは警告のみで続行する**——読めないホストで実行そのものを拒めば、情報を足さずに道具を失うため
- **ユーザーの実 config は読みも書きもしない。** `SNOTRA_CONFIG_DIR`（下記）で `target/visual-check/profile` を指し、そこへ検証用の config を 1 枚書いて起動する。退避も復元も無いので、異常終了しても実 config が検証色のまま残る経路が**構造的に無い**（#803）。残るのは使い捨てプロファイルだけで、`cargo clean` が掃く（`CARGO_TARGET_DIR` を設定している環境では対象外）
- **seed が読めたかは本体の stderr で確かめる。** `[config] ` で始まる行が出たら赤にする。**`config.toml.bak` の不在では証明にならない**——退避は best-effort で、`fs::rename` が失敗すれば parse 失敗でも `.bak` は現れない（`snotra-core/src/config.rs` の `backup_invalid`）。seed が読めていないと既定色で起動するため、「色が届いていない」と誤読される
- **`SNOTRA_CONFIG_DIR` が効いたことは肯定的に確かめる。** 実行後にプロファイル配下へ `*.bin` が生成されていることを見る。効いていなければ本体は実 config を読むため、ピクセルが赤いときに「色が届いていない」と「env が効いていない」を切り分けられない
- 判定が赤のとき（`-KeepShot` なら緑でも）`target/visual-check/` へスクリーンショットを残す。観測点が背景でない場所を指していないかは、この画像で確認する

#### 別プロファイルで起動するための env ハッチ（`SNOTRA_CONFIG_DIR`）

保存先ディレクトリを差し替える（#803。契約の正本は `SPEC.md` §13 と `Config::config_dir` の rustdoc）。`config.toml` / `history.bin` / `index.bin` / `icons.bin` / `window.bin` の**すべて**がこの 1 点から導かれるので、検証が実ユーザーのデータに触れずに済む。

```powershell
$env:SNOTRA_CONFIG_DIR = "C:/tmp/snotra-profile"; cargo run -p snotra
```

- 値は**そのまま**保存先になる（`Snotra` を付け足さない）。空文字は未設定と同じ
- **展開も絶対化もしない。** `%TEMP%\Snotra` のような値は展開されず、相対パスは CWD 起点になる。不正な値を既定へ落とさないのは意図的で、書き損じたときに**実 config を触らせない**ため（フォールバックの向きは「安全そうな方」ではなく「壊れたときに何を守るか」で決める）
- 子プロセスの `snotra-settings` は env を継承するので同じプロファイルを見る。`cargo run -p snotra-settings` の単独起動には効かない
- **単一インスタンス制御には影響しない。** プロファイルを分けても同時起動はできない

#### updater トーストを出すための env ハッチ

実 release への到達を要さずに updater トーストを描かせる（`egui_shell/mod.rs` の `spawn_update_check` 冒頭・**`auto_update` の設定に依らず効く**——判定より前に置いてある）。

| フラグ | 出る局面 | 何を見るためのものか |
|---|---|---|
| `SNOTRA_EGUI_FAKE_UPDATE` | `Available`（v9.9.9・[今すぐ更新] 付き） | 通常のトースト表示。install 実体は無く押しても no-op |
| `SNOTRA_EGUI_FAKE_UPDATE_FAILED` | `InstallFailed`（長い理由を注入） | **失敗理由の併記と末尾省略（`…`）の唯一の観測点**（#654）。実 install 失敗は再現できない |

```powershell
$env:SNOTRA_EGUI_FAKE_UPDATE_FAILED = "1"; cargo run -p snotra
```

両方立てたときは `FAILED` が勝つ（先に `return` する）。

### E. git hook（`.githooks/**`）を変更した場合

```bash
npm test    # 必須: 使い捨て repo で hook を実測する（.githooks/githooks.test.mjs）
```

- PostToolUse フックが `.githooks/**` の編集で `vitest run .githooks` を自動発火する（#484）。`.claude/hooks/**` と同じ理由 — セーフティネットそのものを編集したら、セーフティネットが生きているか確かめる
- `.githooks/` は **main 保護のローカル層**。commit / merge / rebase / push の各操作で git が直接呼ぶため、ツール・シェル・worktree・`git -C` のいずれにも依存しない
- **bootstrap**: `npm install` / `npm ci` が `prepare` スクリプトで `git config core.hooksPath .githooks` を実行する。worktree は `.git/config` を共有するため一度で全 worktree に効く
- この層は best-effort。`core.hooksPath` が外れても **GitHub ruleset（`default`）が main への直接 push を拒否する**ため、外れたことを検知する仕組みは意図的に設けていない

### F. ガバナンス文書（`*.md`・`.claude/rules/`・`.claude/skills/`・workflow）を変更した場合

```bash
npm run governance:check    # 必須: ガバナンス文書の決定的検査（参照実在・モジュール索引・スキル表・SPEC 番号・rules glob・コマンド写像・恒久規範の面積の火災報知器・見出し参照の着地。#587/#593）
```

- **`.rs` のコメントに正準形の見出し参照を書く／その参照先の見出しを改題する変更もこのカテゴリである**（#925。G-heading-refs / G-near-heading-refs の走査元に `.rs` が入っている）。`.rs` では PostToolUse フックが走るが、その沈黙は fmt / clippy / test の合格であって見出し参照の着地を含まない
- PostToolUse フックは `.md` に検査を割り当てない（#497 の受容を維持）ため、**編集時の沈黙は「何も走らなかった」である**。ローカルで本コマンドを実行するか、PR CI の `governance-check` job（skip-ci 非対象・常時実行）に委ねる
- 検査の実体は `scripts/governance-check.mjs`（**検査の一覧は同ファイルのコメント見出しが SSOT**——ここに範囲で写すと黙って腐る。実際「G-module-index〜G-config-reachability」と書いたまま G-check-skill-enumeration まで増えていた・#812）。面積 ratchet の文字数指標は `docs/adr/ADR-area-metric-characters.md`、見出し参照の着地は `docs/adr/ADR-canonical-heading-references.md`、config フィールドの到達性は `docs/development-principles.md`「config の値は到達性の検出器を持たない」。意味判断（責務の妥当性・npm ラッパー等価・メモリ整合）は `/health-check` に残る

## Windows/macOS/Linux で実行可能

```bash
npm test                          # ユニットテスト（Vitest: .claude/hooks + .githooks + scripts）
npm run clean:worktrees          # Agent 委譲で残った worktree/ブランチを掃除（dirty はスキップ、-- --force で強制）
```

## Windows のみ実行可能（`windows` クレートや Win32 API・実行バイナリに依存）

```bash
npm ci                           # 依存インストール（初回セットアップ・CI）
npm run test:powershell         # Pester（cargo metadata の debug 本体を使用。未ビルド時は先に cargo build -p snotra）
cargo test -p snotra-core        # ユニットテスト（純ロジック層）
cargo test -p snotra-egui-runtime # ユニットテスト（egui入力・IME・Surface の描画失敗リトライ方針）
cargo test -p snotra             # ユニットテスト（Tauri 統合層: state/indexing/config_watcher 等）
cargo test -p snotra-settings    # ユニットテスト（設定 GUI の純ロジック: font face 検証・TOML エラーローカライズ）
cargo test --release -p snotra-core bench_ -- --ignored --nocapture  # 検索パフォーマンス計測（詳細: PERFORMANCE.md）
cargo test --release -p snotra-core --test memory_footprint -- --ignored --nocapture  # 索引の常駐メモリ実測（アロケータ計数・詳細: PERFORMANCE.md）
cargo check --workspace          # Rust 全 crate 型チェック
cargo clippy --workspace --all-targets -- -D warnings  # lint チェック（カテゴリ A と同じ）
cargo fmt --all                  # 整形の**修復**（検査は カテゴリ A の `cargo fmt --all -- --check`・#858）
cargo run -p snotra-settings     # snotra-settings（egui ネイティブ設定 GUI）の単独起動
cargo run -p snotra              # 製品メインウィンドウ（egui 既定・#532 SU7 flip 済み。視覚スモークはこれ）
npm run verify                   # Rust + node 一括検証（cargo check --workspace + npm test）
npm run smoke:startup             # 起動時スモーク（trace 出力・検証用プロファイル・非 first-run の検証）
npm run smoke:egui                # egui 経路の show/hide スモーク（keybd_event 注入 + trace検証・#532 SU7。既定 ExePath = target/release）
npm run measure:memory            # メモリ実測（PrivWS 軸・ツリー合算・#532 flip 基準 3）
npm run measure:memory:stages     # メモリ実測（起動→表示→検索→hide の段階別・前景計測。実行中の snotra を kill する）
npm run tauri build              # リリースビルド（NSIS バンドル。`prepare:sidecar` で binaries/ を用意してから）
```

## git blame と整形コミット

`.git-blame-ignore-revs` は `cargo fmt --all` の一括整形コミット（#858）を blame から隠す。**GitHub の blame ビューは設定不要で自動適用する**（ルート直下に置くことが条件・実測）。**ローカルの `git blame` は自動ではない**ため、使うなら次を 1 度実行する（任意・機構では強制しない）:

```bash
git config blame.ignoreRevsFile .git-blame-ignore-revs
```

## スモーク運用メモ

- `scripts/lib/SnotraSmoke.psm1` は 3 本の検証スクリプトに共通する config seed の骨格、env の設定/復元、Cargo target の本体導出、既存プロセス方針、起動、窓/trace 待機、キー注入、DWM/DPI 対応キャプチャを所有する。各 smoke の合否条件と固有の TOML 節は呼び出し側に残す。`scripts/lib/SnotraSmoke.Tests.ps1` は env の正常/例外時復元、Cargo target 導出、trace parse、既存プロセス方針を単体検査し、実バイナリで seed parse・意図したプロファイルへの `index.bin` 生成・窓矩形＝キャプチャ寸法に加え、**起動後の最初のフレームで入力欄が打鍵を受け取れる状態になっていること**を統合検査する（#872 で機序を再設計。キャレットの断言は `src-tauri/src/egui_shell/view.rs` の kittest が実コードの並びごと縛るので、実機側は打鍵を注入しない）
- `scripts/smoke-startup.ps1` は `SNOTRA_TRACE=1` で起動し、trace が 1 件以上出ることを要求する。**検証用プロファイル（`SNOTRA_CONFIG_DIR` で `target/smoke-startup/profile` を指す・#804）をループ前に 1 回だけ seed し、5 起動で共有する**——実 config には触れず、毎回作り直さないのは「first-run でない起動」を測る現在の意味論を保つため。**seed が正常に読まれたこと、first-run を踏んでいないこと、env が効いたこと（プロファイルに `*.bin` が生成されたこと）も肯定的に検査する**。trace の失敗名には統一分類がなく、正常系でも起こりうる best-effort の失敗も含まれるため、汎用的な「起動エラー不在」は保証しない（#845）。trace 0 件は起動経路を何も観測できていないため失敗にする（#690 follow-up）。実際に冷えた CI runner の初回起動で trace 0 行を実測しており、その状態でも旧 smoke は緑を返していた。サマリ表の `event_count` は成功時にも出す（検査が実際に何かを見たことを示す肯定的報告）。**待ち方は「最初の trace を待ってから観測時間 `WaitMs` を開始する」**（`-FirstTraceTimeoutMs`・既定 12s）——固定待機だけだと遅い側に振れた起動が丸ごと無音になる（実測: 同一 runner・同一バイナリで最初の trace までが 0.6s〜8s 超とばらつき、5 回中 3 回が無音だった）。固定待機を一律に伸ばす案を採らないのは、速い起動まで毎回待つことになるため。`first_trace_ms` も成功時に出す（**分散の原因は未解明**ゆえ、予算に触れる前に悪化を読めるようにする。`n/a` は予算内に 1 行も出なかったことを表す）
- `scripts/smoke-egui.ps1` は egui 経路の自動回帰の最低線（#532 SU7・e2e/ 撤去後の後継）: `SNOTRA_TRACE=1` で起動 → keybd_event で hotkey（起動時の `hotkey:registered` trace から導出した VK 列を注入。対応表の SSOT は `src-tauri/src/platform/hotkey.rs` の `injection_vks`。押下順で押し、解放込み）→ `egui_show:done` 観測 → Escape → `egui_hide:done` 観測 → `msedgewebview2` のグローバル増分 0 を検証する。`-HotkeyVks` を明示指定すると trace より優先される（trace を出さない旧バイナリの検証など）。**検証用プロファイル（`SNOTRA_CONFIG_DIR` で `target/smoke-egui/profile` を指す・#804）へ最小の有効 TOML を常に seed する**——実ユーザーの `%APPDATA%\Snotra` は読みも書きもしない（退避も復元も持たないことが、異常終了しても実 config が壊れない構造的な保証である）。seed を置く理由は「ファイルが無いと first-run になり `Config::default()` の探索パスシードで実マシンを索引しうる」ことであり、#824 以降は空 TOML でも parse 自体は通る（かつては必須セクション欠落で parse 失敗し破損復旧経路を踏んだ）。**seed が読めたこと・first-run を踏んでいないこと・env が効いたこと（プロファイルに `*.bin` が生成されたこと）を 3 つとも肯定的に検査する**——「観測できなかった」を合格と読ませないため。実行中の snotra を kill するためローカル実行時は注意。網羅は担わず、視覚・操作列は手動 GUI smoke（カテゴリ D）が補完する
- `scripts/smoke-egui.ps1` は results 窓の表示も検査する（#671/#673 サイクル PR A）: `egui_show:done` の後、1 文字クエリを注入して `egui_results:show` を観測し、Escape 後の `egui_hide:done` に続けて `egui_results:hide` も観測する。**#804 以降 skip は無い**——検証用プロファイルを常に seed するので、既定クエリ `"z"` が seed した索引 1 件に必ず一致する（`-ResultsQuery <letter>` は残るが、開発機の既存索引に合わせる用途は消えた。空文字を渡す用途については次の bullet）（CONTRIBUTING.md の「results 窓 show/hide の trace 観測」と対応）
- **results 検査は無条件に要求される（#686 の `-RequireResults` を #804 が格上げ）**: 旧 flag が opt-in だったのは「ローカルでは索引を制御できないのが普通」ゆえの緩和だったが、検証用プロファイルを常に seed する以上その前提は偽になったので、flag ごと撤去して**常に要求する**形にした。**これは検出器の削除ではなく格上げである**——従来はローカルで既定 skip（緩和）だったものが、ローカルでも赤になる。**判定はアプリ起動前に確定する**ため、この guard はプロセスを起こさずに落ちる（`pwsh -File scripts/smoke-egui.ps1 -ResultsQuery '' -ExePath <任意の既存ファイル>` でフォールトインジェクション可能・実機に触らない。**空文字の明示がその注入口である**）。skip へ至る沈黙経路は構造的に消えており、他（実行ファイル不在・`hotkey:registered` 未観測・`egui_show:done` 未観測・`egui_results:show` 未観測・クエリが A-Z 単字でない）はいずれも exit≠0 で鳴る。**`e2e.yml` のステップ順序も自由になった**——両 smoke が自分の検証用プロファイルを持つため、startup smoke の 5 起動が egui smoke の seed を壊すことはない（#803 で `SNOTRA_CONFIG_DIR` が入り、#804 が smoke 2 本を env 化した）
- **smoke が赤いときは `--- 失敗時の証拠 ---` ブロックを先に読む**（#690 follow-up）: **プロセスの生死**（既に終了 = 起動途中で落ちた / 生存中 = 起動はしたが未到達・遅延）と **trace 行数**が出る。`trace 行数: 0` は単体では「起動していない」とも「イベントが出ていない」とも読めるため、**必ず生死と併せて読む**。失敗チャネルは `throw`（前提崩壊）と検査項目の不合格の 2 本あり**どちらも同じ証拠を出す**——以前は後者にしか証拠が無く、`throw` は手掛かりを残さず終了していた（`-StartupWaitMs 0 -ObserveTimeoutMs 1` でフォールトインジェクション可能）
- **`hotkey:registered` だけ別予算**（`-StartupObserveTimeoutMs`・既定 25s）: 起動後最初の観測だけが cold start を含む。以降（show/hide/results）はアプリが温まった後ゆえ `ObserveTimeoutMs` のまま（一律に広げると本来速い検査の失敗検出まで鈍る）。CI では「起動後 12,000ms 経っても trace 0 行・プロセスは生存」を 3 回、「起動から 0.6s で観測」を 2 回実測しており、**この二極の原因は未解明**。この予算は原因究明までの緩和であって修正ではない
- **壁時計から起動レイテンシを推定してはならない**（実際に一度誤った）: seed の print から hotkey 観測までの壁時計には、`Add-Type -MemberDefinition`（**実行時 C# コンパイル**。冷えた runner で 7〜25s 変動）を含む**起動前**の時間が乗る。アプリの遅延を表すのは起動起点の計測だけで、両者を混同すると「境界に乗っていた」のような誤った結論に達する
- **成功時にも観測レイテンシを出す**: 予算を広げただけだと、起動が遅くなっても予算内に収まる限り緑で**気づけない**。毎回数字が出れば退行を人が読める。値は「観測できたのが何 ms 後か」であって**アプリの準備時間ではない**（下限が `StartupWaitMs` で頭打ち）ため、固定待機を併記する
- **0 行かつプロセス生存のときは事後観測に入る**（`-PostMortemWaitMs`・既定 30s・失敗時のみ）: 最初の 1 行が出れば「**遅延**（起動から約 N ms）」、出なければ「**未到達**」と報告する。**観測時間を延ばす前にこの数字を取ること**——遅延なら延長の根拠になり、未到達なら延ばしても解決しない。合否には影響しない（失敗は失敗のまま）
- **ビルド済みバイナリの手動 smoke では「変更が含まれているか」を確認する**: `cargo` は `target/debug/deps/<crate>-<hash>.exe` を `target/debug/<crate>.exe` に hardlink し直すため、ソース変更後でも（fingerprint 上「最新」と判断され再リンクされず）`<crate>.exe` のタイムスタンプが更新されないことがある。タイムスタンプを信用せず、変更固有の文字列でバイナリを grep して目的の変更が入っているか確認する（例: `[Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe)).Contains('<変更固有の文字列>')`）。#343 の balloon smoke で実際に踏んだ
- **debugネイティブGUIの`Process.MainWindowHandle`を画面証拠に使わない**: console subsystemのdebugバイナリは親Windows TerminalのHWNDを返す場合がある。対象PIDのトップレベルウィンドウを列挙し、PIDと期待タイトルの両方でネイティブウィンドウを特定してから可視性・スクリーンショットを検証する

## CI/CD メモ

### 検証コマンド ↔ GitHub Actions workflow の対応

「変更後の検証チェックリスト」の必須コマンドが、どの workflow でどのトリガーで自動実行されるかの対応表。エージェントが「PR CI 緑」だけで全検証済みと誤認しないための SSOT。

| 検証コマンド | workflow | トリガー |
|---|---|---|
| `npm test`（Vitest: hooks/githooks/scripts） | `ci.yml`（node-check=ubuntu / rust-check=windows） | PR 自動（`skip-ci` は下記ノート参照） |
| `npm run test:powershell`（Pester: 共有 smoke 配管 + 実バイナリ統合） | `ci.yml`（rust-check） | PR 自動（`skip-ci` は下記ノート参照） |
| `cargo fmt --all -- --check`（#858・整形ゲート） | `ci.yml`（rust-check） | PR 自動 |
| `cargo check` / `cargo test -p snotra-core` / `cargo test -p snotra-egui-runtime` / `cargo test -p snotra` / `cargo test -p snotra-settings` / `cargo clippy` | `ci.yml`（rust-check） | PR 自動 |
| `cargo doc --workspace --no-deps --document-private-items`（#562・intra-doc link 検査） | `ci.yml`（rust-check） | PR 自動 |
| `npm run governance:check`（#587・ガバナンス文書検査） | `ci.yml`（governance-check） | PR 自動（**`skip-ci` 非対象** — if ガードを持たず常時実行） |
| `npm run smoke:startup`（注） | `e2e.yml`（smoke-egui job） | 対象 paths を含む PR（自動）/ 手動 dispatch |
| `npm run smoke:egui`（#532 SU7・egui 経路の自動回帰） | `e2e.yml`（smoke-egui job） | 対象 paths を含む PR（自動）/ 手動 dispatch |

（注）CI では smoke-egui job がビルドした release バイナリを共有するため、`npm run smoke:startup`（既定 ExePath = debug）ではなく `scripts/smoke-startup.ps1 -ExePath target/release/snotra.exe` を直接実行する。検証する起動経路は同じ（release バイナリが trace を出し、seed 済み検証用プロファイルで非 first-run 起動すること）。これは smoke 用ビルドの起動健全性検証であり、配布バンドル（`tauri build`）の検証ではない。

- `npm test` は ubuntu（node-check）と windows（rust-check）の両方で走る（#509）。`.githooks` / `.claude/hooks` の selftest は実運用が Windows でのみ起きるセーフティネットであり、hook 実行機構（Git-for-Windows の shebang 経由 sh 起動・パス/クォート境界）が本番と一致する OS で回帰検査する。ubuntu 側は実行ビット・POSIX sh 厳密性を相補的に担保する。CRLF 由来の fail-open は `.gitattributes` の `.githooks/** text eol=lf` で両 OS 回避済みで、かつ dash 側の故障モードなので windows 固有ではない。
- **`skip-ci` ラベルはジョブ単位で効く** — node-check / rust-check の `if` が同一のため、貼ると cargo 系を含む**両方まるごと**スキップする（表の各行に個別注記はしない）。**`governance-check` job は `if` ガードを持たず、`skip-ci` を貼っても走る**（#587。skip-safe と定義された Markdown-only 変更こそが検査対象のため、意図的にガードしない）。CI は required status check ではない（ruleset `default` に `required_status_checks` 規則が無い・実測）ためマージは通り、main への push（マージ後）では `github.event_name == 'push'` により**ラベル無関係に必ず走る**。
- **`skip-ci` を貼ってよいのは skip-safe な変更のみ** — node-check / rust-check がテスト対象に持たない `.claude/skills/**`・`.claude/rules/**`・`.claude/agents/**`・`docs/**`・`**/*.md` だけ（これらの決定的検査は skip されない governance-check が担う・#587）。**貼ってはならない**: `.claude/hooks/**`・`.githooks/**`・`scripts/**`・`.claude/settings.json` — これらは `npm test` が両 OS でセルフテストを回す（`vitest.config.ts` の `include`・上の #509）。「`.claude`-only だから安全」と一括りにしない（同じ表層形 `.claude/` が「Claude が読むだけの設定」と「CI が検査するセーフティネット」の二概念を担うため・#500）。
- カテゴリ C（ウィンドウ生成・ホットキー・スラッシュコマンド）相当の変更や依存更新を含む PR は、対象 paths（`src-tauri/**`・`**/Cargo.toml`・`Cargo.lock`・`package.json`・`package-lock.json` 等）に該当するため `Smoke` workflow が自動起動する。paths 外の変更で手動実行するには `workflow_dispatch`。
- この対応関係のドリフト（必須コマンドに対応 workflow が無い等）は `npm run governance:check`（G-ci-table）が検出する（#587。旧 `/health-check` Check 10）。

### その他

- **`GITHUB_TOKEN` では他のワークフローをトリガーできない**: tag push や `workflow_dispatch` を `GITHUB_TOKEN` で発火させても、別ワークフローは起動しない（GitHub の仕様）。ワークフロー間の連鎖には `workflow_call`（呼び出し元から直接呼ぶ）を使う
