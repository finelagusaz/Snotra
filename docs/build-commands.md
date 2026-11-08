# ビルド・実行コマンド

このドキュメントは、Snotra のビルド・検証コマンドの単一の情報源（SSOT）である。`AGENTS.md` の開発ワークフローや `.claude/skills/*/SKILL.md` の検証ステップは、コマンドの本体をここに置いて参照する。コマンドを追加・変更するときは、このファイルだけを更新する。

## PowerShell で npm run へ引数を渡すとき

**`npm run <script> -- -Opt …` の `--` は、PowerShell では `'--'` と引用して打つ**（#1296）。

- 素の `--` は、npm.ps1 の経路によっては PowerShell に取り除かれる。すると `-Opt` が npm に吸われ、スクリプトには値だけが届く。npm script の入口の `.ps1` は位置引数を拒むので、名前の付かない値は束縛エラーで止まる（#1366）。スイッチだけの引数（`-PostToPr` など）は値を残さないので、黙って消える
- `'--'` はどのシェルでも同じに届く。`.ps1` の入口（`scripts/*.ps1` を呼ぶ npm script）の例は、この形で書く
- `-- --` と重ねない。`--` を残す経路では、余った `--` がスクリプトへ届く

## 変更後の検証チェックリスト（必須・スキップ不可）

変更したファイルの種類に応じて、対応するカテゴリの必須コマンドを実行する。複数のカテゴリに当たる場合は、すべて実行する。

**各カテゴリの節にある最初の `bash` フェンスは、`npm run verify:tree`（`scripts/verify-tree.mjs`）が読んで、そのまま回す。** コマンドの正本はこのフェンスであり、ツールは写しを持たない。フェンスの外の散文に書いた条件つきのコマンドは、ツールに届かない（`/implement` が `--also` で足す）。

### A. Rust ファイル（`*.rs`）を変更した場合

**これらのコマンドが走る Rust の版は、`rust-toolchain.toml` が決める**（#1161）。ローカルでも CI でも rustup がこのファイルを読んで版を上書きするので、手元の緑は CI の緑と同じ判定器で得たものになる。版を固定していなかった頃は、ローカルの Rust が CI より古いと、手元に存在しない lint で CI だけが赤くなった（2026-08-21 実測）。版を上げる契機は `/deps-update` が持つ。

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

- **変更した crate のテストは、ローカルで必ず走らせる。** 変更していない crate のテストは、ローカルでは任意である。CI の rust-check が、PR ごとに workspace の全 crate のテストを実行する
- **エージェントが確認のために手で打つときは、`-q` を付けてよい**（`cargo test -p snotra -q` 等）
  - 省かれるのは、cargo の進捗行と、成功したテストの逐一の報告だけである。失敗の証拠は一つも落ちない。panic メッセージ・`assert_eq!` の left/right・`failures:` の一覧・`test result: FAILED`・非ゼロの exit は、いずれも残る（2026-08-13 に使い捨ての crate で実測）
  - 出力量は、`cargo test -p snotra` の 276 行 / 22,235 文字に対し、`-q` では 8 行 / 411 文字だった
  - カテゴリ A のコードブロックの必須行へは `-q` を足さない。CI の実行形と同じ形に揃えておくためである
- **fmt が赤いときは `cargo fmt --all` を打つ**（#858）
  - 検査（`cargo fmt --all -- --check`）と修復（`cargo fmt --all`）は別のコマンドで、`--check` は直さない
  - 差分を読んで手で直さない。機械が持つ判定を、人が写す作業になるからである
  - 整形の様式は rustfmt の既定とし、`rustfmt.toml` を置かない（既定が drift 最小であることを、7 つの設定の比較で実測した・`docs/adr/ADR-rustfmt-gate.md`）
- カテゴリ A のコマンドは、いずれも CI（`ci.yml` の rust-check）が PR で自動実行する（「CI/CD メモ」の対応表）。ローカルで自動実行する仕組みは無い（編集後フックは 2026-09-29 に撤去した）
- **`check` / `clippy` には `--workspace` を使う**（#500）
  - crate 名を `-p` で列挙すると、`Cargo.toml` の `members` の写しになる。5 つ目の crate を足したとき、CI と本ファイルが同じ誤りを共有し、気づかれないまま漏れる。`--workspace` なら、cargo に SSOT を読ませられる
  - 一方、`cargo test -p <crate>` は「編集した crate → そのテスト」の対応なので、`-p` のまま残す（`--workspace` にすると、編集していない crate のテストまで走る）
- **既定ビルドから外れる feature は、CI でだけ通す**: `cargo check -p snotra --features heap-trace`（ヒープ計装）
  - カテゴリ A の必須ではない。`.rs` を触るたびに走らせる必要は無く、CI（rust-check）が PR ごとに通す
  - ここに書くのは、誰も通さない経路を作らないための配置の記録であって、手で打つ手順ではない（計測コードの意味と撤去条件は `src-tauri/src/heap_trace.rs` の `//!`）
- **doc コメント（`///` / `//!`）を触ったら、`cargo doc --workspace --no-deps --document-private-items` をローカルで手で実行する**（#562）
  - `cargo doc` が自動で走るのは、CI（rust-check）だけである
  - deny 化は 2 段で行う。各 crate の `[lints] workspace = true`（`Cargo.toml`）から root の `[workspace.lints.rustdoc]`（`broken_intra_doc_links` / `invalid_html_tags`）へつなぎ、既定の warn による見逃しを塞ぐ
  - この 2 段の崩れ（member 側の opt-in 漏れと、root 側の deny の降格・欠落）は、どちらも cargo が exit 0 のまま沈黙する。そのため `npm run governance:check`（G-workspace-lints）が検知する（#713）
- **`src-tauri/clippy.toml` の `disallowed-methods` へ禁止を足したら、`scripts/governance/checks/G-clippy-disallowed.mjs` の `REQUIRED_DISALLOWED_METHODS` へも足す**（#900。禁止集合の正本は `src-tauri/clippy.toml` の冒頭）
  - カナリアへ登録していないパスの書き損じは、適用範囲の外である
  - この lint が赤くなるのは、`-D warnings` のおかげではない（#950）。既定は warn だが、root の `[workspace.lints.clippy]` の `disallowed_methods = "deny"` で昇格させてあるので、コマンドラインのフラグから独立している（`clippy.toml` を持たない crate は、禁止集合が空なので無害である）
  - 次の崩れは、どれも clippy が exit 0 のまま沈黙するので、`npm run governance:check`（G-clippy-disallowed）が検知する
    - 禁止集合の空洞化（ファイルの削除・空配列化・エントリ 1 行の消失・カナリアが名指すパスの書き損じ・コメントアウト）
    - この deny の消失・降格と、同じ節の群 allow による打ち消し
- コマンドが実在することは、`npm run governance:check`（G-build-commands）が検知する
- `snotra-settings` を含めるのは、egui ネイティブウィンドウ側の型の壊れも検知するためである

### B. TypeScript ファイル（`vitest.config.ts` 等）を変更した場合

TS の型検査は、#532 SU7 のフロント撤去で無くなった（`tsconfig.json` ごと削除した）。残る `.ts` は `vitest.config.ts` だけで、その変更は `npm test` が検証する。

### C. ウィンドウ生成／表示順・ホットキー・スラッシュコマンドに触れた場合（A／B に追加）

```bash
cargo build -p snotra       # 必須: 実バイナリを起動する検査の前に本体を建てる（下の箇条・`cargo test` では更新されない）
npm test                    # 必須: ユニットテスト（Vitest: .claude/hooks + .githooks + scripts）
npm run test:powershell     # 必須: Pester（共有 smoke 配管 + 実バイナリ統合）
npm run smoke:startup       # 必須: 起動時スモーク（trace 出力・検証用プロファイル・非 first-run の検証）
npm run smoke:egui          # 必須: egui show/hide スモーク（hotkey 注入 + trace 検証・#532 SU7）
```

- `test:powershell` は、Pester 6.0.1 を `target/pester/` へ固定の版で取得し、グローバル環境を変えない。統合テストが使う本体は `cargo metadata` の `target_directory` から導くので、`CARGO_TARGET_DIR` に追随する。未ビルドなら、先に `cargo build -p snotra` を実行する。別の本体を検査するときだけ、`npm run test:powershell '--' -ExePath <path>` と打つ
- **実バイナリを起動する検査の前に、`cargo build -p snotra` を打つ。** `cargo test` がビルドするのはテストターゲットであって、`target/debug/snotra.exe` ではない。危ないのは「未ビルド」より「古いまま在る」ほうである。検査は成功したまま、変更前の挙動を測って赤や緑を出す（#835 では 2 時間前のバイナリを測り、直したはずの伸縮を FAIL として観測した）
- WebView2 の E2E（Playwright + tauri-driver）は、#532 SU7 の flip で撤去した。後継は `smoke:egui`（自動回帰の最低線）と、手動の GUI smoke（カテゴリ D）である
- **「通常の CI が緑」だけで、smoke 済みと読まない**
  - `npm test` は通常の PR CI（`ci.yml`）で自動実行されるが、`smoke:startup` / `smoke:egui` は通常の PR CI では走らない
  - `src-tauri`・`snotra-egui-runtime`（描画ループを所有する・#701 で追加）・依存の manifest / lockfile などを含む変更では、`Smoke` workflow（`smoke.yml`）が paths によって自動起動し、両 smoke を実行する。paths の外の変更で回したいときは、`workflow_dispatch`（手動実行）を使う
  - **CI に検証を委ねるなら、その job が実際に何を実行したかを確かめる。** #671 のサイクルでは、`Smoke` が 5 run 続けて緑のまま、results の検証を skip していた。この事例は #686 が `-RequireResults` で機構にし、#804 が「results の検査を無条件に要求する」形へ格上げした（「スモーク運用メモ」節）。それでも、「緑」が「検査が走った」を意味しない形は、ほかにも作れる。委ねる前に、その job のステップと渡す引数を読む

### D. UI のスタイル・レイアウト・テキスト表示に影響する変更（A／B／C に追加）

`cargo run -p snotra` で起動し、overflow・clipping・フォントのレンダリングを目視で確かめる。PR を作る前に必須である。

**該当するかは、「表示に関わるコードを触ったか」ではなく、「変えたのが表示の見た目か」で決める**（判定を拡張子で決めない・#558 の裏返し）。該当の判定は広げる向きにも外れ、そちらは安全側ではない。エージェントは `smoke:manual` を実行できないので、該当と読めば、そのまま人間への申し送りになる。過大な申し送りは、本当に要る項目を埋もれさせる。#1106 は起動を止めるゲートを足しただけで、描画を 1 ピクセルも変えていない。それなのに「表示経路を触った」ことを理由に該当と誤認し、PR 本文とマージ後の commit message へ「未実施」と書き残した（この節の発火条件を読み直すまで気づかなかった）。

```powershell
npm run smoke:manual              # 項目の読み上げ・判定の記録・trace の並置（#749）
npm run smoke:manual '--' -Only 2,5 # 一部だけ再実施
npm run smoke:manual '--' -PostToPr # 記録を PR コメントへ投稿する
```

- **trace には性質の違う 2 つが載る。混ぜて読まない**（#757）
  - presence（イベントが出たか）は診断であって、合否ではない（#671 PR A′ では、`egui_results:hide` が出たのにウィンドウは残り、presence を見る smoke は緑のまま通した）
  - H1 / H4 / H5 の不変条件は、「起きてはならないことが起きていないか」を見るので、合否の判定に使える。判定は `scripts/lib/SnotraTraceInvariants.psm1` が持つ（Pester で実測。`smoke:egui` の orphan 検出と同じ導出を共有する）
- **項目の合否は、目視と trace の両方で決める。** trace が緑でも目視が赤なら赤であり、逆も同じである。記録は、両者の不一致を専用の節で名指しする
- **SKIP を見たら合格と読まず、記録に併記された理由を確かめる。** SKIP は「判定できなかった」であって、合格ではない。「該当イベントが無い」「`rows` が読めない」「main の可視状態が未観測」「hide ウィンドウが閉じていない」「parse できなかった行がある」は、すべて SKIP として現れる
- **`smoke:manual` は人間へ依頼する**（`Read-Host` で判定を採るので、エージェントには実行できない）
  - 実施の有無が会話にしか残らないと、「検証されていない」と「問題が無かった」を区別できなくなる。`-PostToPr` か、出力ファイルの貼り付けで PR に残す
  - カテゴリ D の全体がそうなのではない。`check:colors` は exit code で自動判定するので、エージェントも実行できる（画面がロックされているときを除く）。目視項目も、打鍵注入とウィンドウ矩形のキャプチャで実施した実績がある（#836 / #870）
- **新機能のために、先回りして `$items` へ項目を足さない。** 足す条件は「その表示が実際に一度回帰したとき」である
  - `scripts/manual-smoke.ps1` の `$items`（常設の項目）が SSOT であり、PR 本文の目視表とは別の母集団である（写しではない・`docs/adr/ADR-folder-location-display-surface.md`「却下 6」）
  - `$items` に載るのは、どの変更でも壊れうる横断的な不変条件である。PR 本文の表は、その PR 限りの受け入れ確認である
- **`cargo run` には必ず `-p snotra` を付ける**（既定は egui・#532 SU7 の flip 済み・env フラグは不要）。`-p` を欠くと、ルートでは bin を決められずにエラーになる（候補は `snotra` / `snotra-settings`。実測: `error: cargo run could not determine which binary to run`）。cwd が crate の配下なら、その crate の bin が起動する

#### `[visual]` の色を変える変更は、**非既定色で**目視する

```powershell
npm run check:colors                      # 自動判定: 各ウィンドウが宣言した色の占有率を実測し exit code で返す
npm run check:colors '--' -Color '#FFF'   # 3 桁 hex の受理（#680 の 1・パーサ統合の回帰）
npm run check:colors '--' -Interactive    # 判定せず起動し、目視項目を読み上げる
```

- **CI では走らない。** GUI を要するので、`ci.yml` にも `smoke.yml` にも入っておらず、明示的に起動したときだけ動く（エージェントも起動できる。画面がロックされているときを除く）。そのため、`[visual]` の色に効く変更は、CI が緑でも未検証である
- **既定色での確認は、この検証にならない。** config の既定 `#282828` は `snotra-egui-runtime` の `CLEAR_COLOR` と一致するので、色が届いていなくても正常に見える（原理は `docs/development-principles.md`「config の値は到達性の検出器を持たない」）
- **自動判定するのは、各ウィンドウが「見せねばならない」と宣言した色である**（#953）。main は背景と入力欄の背景、results は背景を宣言する
  - 判定は「ウィンドウ全体の最頻色が期待色か」ではない。その前提（背景はウィンドウで最も広い色）は、main ウィンドウでは偽である。入力欄の塗りが背景より広く（実測 37.6% > 33.3%）、旧来の判定では、main はどんな背景色を指定しても赤だった
  - 下限の占有率は、宣言ごとに持つ（main 15% / results 50%・実測に対して 1.5〜2.5 倍の余裕）。「最頻である」は「下限以上」を含意するが、逆は成り立たない。一律の小さな下限にすると、results の検出力が落ちる
  - 合否に依らず、色ごとの実測の占有率を出す。そのため、レイアウトのドリフトは、失敗になる前に「余裕の縮小」として見える
  - 判定の述語は `scripts/lib/SnotraWindowColors.psm1` が持つ（Pester が、述語と配備される下限の両方を測る）。宣言を呼び出すのは `scripts/visual-check-colors.ps1` である。却下した代替案は `docs/adr/ADR-declared-colors-over-modal-color.md`
- results は、専用の scan 3 件を seed し、キー注入で表示して、実ピクセルを測る。残る 2 点は目視（`-Interactive`）に留まる
  - show の一瞬のフラッシュ: present の前の 1 フレーム未満なので、連写しても捉えられない
  - results のリサイズ時のちらつき: 入力と描画のタイミングに依存するので、単一のキャプチャでは不在を証明できない
- **測れないもの（受容する残余）**: 位置に紐付かない存在検査なので、背景と入力欄の色が入れ替わる「クロス配線」は検出できない（両色とも下限を超えて緑になる）。選択色・hint 色も測らない
- **trace は判定に使わない。** 「`set_clear_color` を呼んだ」というログは、その色が画面へ出たことを意味しない（`src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」）。判定の根拠は、描かれたピクセルだけである
- **画面がロックされていると実行できない**（#866）
  - ロック中は、ウィンドウが可視のままでも画面に合成されない。`CopyFromScreen` は、ロック画面の中身を持つ有効な Bitmap を返す（`IsWindowVisible` も矩形も DPI も真っ当な値なので、ほかのガードはすべて通る）
  - `Get-SnotraWindowCapture` が、起動の前にロックを名指しして止める
  - ロック状態を判定できなかったときは、警告だけを出して続行する。読めないホストで実行そのものを拒めば、情報を足さずにツールを失うためである
- **ユーザーの実 config は、読みも書きもしない。** `SNOTRA_CONFIG_DIR`（env ハッチ）で `target/visual-check/profile` を指し、そこへ検証用の config を 1 枚書いて起動する。退避も復元も無いので、異常終了しても実 config が検証色のまま残る経路は、構造的に無い（#803）。残るのは使い捨てのプロファイルだけで、`cargo clean` が消す（`CARGO_TARGET_DIR` を設定している環境では対象外）
- **seed が読めたかは、本体の stderr で確かめる。** `[config] ` で始まる行が出たら赤にする
  - `config.toml.bak` の不在は、証明にならない。退避は best-effort で、`fs::rename` が失敗すれば、parse に失敗しても `.bak` は現れない（`snotra-core/src/config.rs` の `backup_invalid`）
  - seed が読めていないと既定色で起動するので、「色が届いていない」と誤読される
- **`SNOTRA_CONFIG_DIR` が効いたことは、肯定的に確かめる。** 実行後に、プロファイルの配下へ `*.bin` が生成されていることを見る。効いていなければ本体は実 config を読むので、ピクセルが赤いときに「色が届いていない」と「env が効いていない」を切り分けられない
- 判定が赤のとき（`-KeepShot` なら緑でも）、`target/visual-check/` へスクリーンショットを残す。観測点が背景でない場所を指していないかは、この画像で確かめる

#### 入力欄の縦位置が気になる変更では、フォント別に実ピクセルを測る

```powershell
npm run check:input-metrics                                  # 既定の 8 フォント（日本語 4 + 英語 4）
pwsh -File scripts/visual-input-metrics.ps1 -Fonts 'Segoe UI;Arial'   # `;` 区切りで絞る
pwsh -File scripts/visual-input-metrics.ps1 -KeepShots        # 撮ったウィンドウを target/ へ残す
```

- **合否を出さない。ツールであって、検査ではない。** 出るのは「欄の内側 / キャレットの高さ / 上下余白 / Skew」の表で、読むのは人間である
  - 毎回の作業では走らせない。使う契機は、egui / epaint を上げたとき・入力欄の描画や行高やフォント登録に触ったとき・「見え方が変わった気がする」と報告を受けたときである
- **判定を持たせない理由**: フォント依存の 1px の非対称が、構造的に残る（#1045）。閾値を置けば、「常に緑」か「常に赤」のどちらかになる。永久に赤いゲートは、ゲートが無いのと機能的に同じである（`check:colors` が #953 で旧述語を捨てたのと同じ形）
- **Rust 側の検査では届かない領域を見る。** `view.rs` の `input_text_sits_vertically_centered_for_both_body_and_hint` は、egui の論理座標で galley を縛る。しかし、実機のフォント解決・softbuffer のラスタライズ・DPI は通らない。2026-08-11 の実測では、論理座標では 8 フォントすべてが対称だったのに、実機では 4 種が 1px ずれていた（Windows 既定の Yu Gothic UI と Segoe UI が、両方ともずれる側だった）
- **変更の前後で撮り、突き合わせる。** egui 0.36.1 への更新では、8 フォントの値が 0.35.0 と寸分違わないことを、この形で確かめた
- 判定ロジックは `scripts/lib/SnotraInputMetrics.psm1`（純関数・Pester が測る）が、起動とキャプチャは `scripts/visual-input-metrics.ps1` が持つ。値の意味と読み方は、モジュールの doc が正本である
- `check:colors` と同じく、CI では走らない・ユーザーの実 config には触れない（`SNOTRA_CONFIG_DIR` で `target/visual-input-metrics/` を指す）・画面ロック中は実行できない

#### エージェントが目視項目を自分で実施するとき

`smoke:manual` を実行できないのは、合否の記録に `Read-Host` を使うからである。目視項目の実体（アプリを操作して表示を観測する）は、エージェントも実施できる（#836 で 11 項目・#870 で日英 2 本の実績）。実施するなら、以下に従う。

- **`scripts/lib/SnotraSmoke.psm1` の関数だけで組む。**
  - `New-SnotraVerificationProfile` / `Start-SnotraProcess` / `Wait-SnotraWindow` / `Set-SnotraForegroundWindow` / `Send-SnotraKey` / `Get-SnotraWindowCapture` で、使い捨てプロファイルの seed から、打鍵注入・ウィンドウ矩形のキャプチャまでが完結する
  - この経路だけが、上の画面ロックの検出（#866）に守られる。モジュールに無い操作（マウスの `SetCursorPos` + mouse_event 系。いずれも Win32 API で、今もモジュールに無い）を自前の P/Invoke で足すと、その実行は検出の外へ出る。ロック中に走らせればロック画面を撮り、`check:colors` のような判定が無いぶん、誰も気づかない
- **撮る前に、その入力が分岐へ入っているかを確かめる**（#872）。中間省略・overflow・clipping は、入力が短ければ起きず、正常に見える画像が撮れてしまう。分岐へ確実に入る fixture を用意して、初めて測ったことになる（`AGENTS.md`「検証の作法（全タスク共通）」の「観測形が対象を含むか」の視覚版）
- **高さの判定に `GetWindowRect` を使わない。** 不可視のリサイズ枠を含むので、2 行のウィンドウが 1 行の 2 倍にならない（実測 118 / 64 に対し、`DwmGetWindowAttribute` の `EXTENDED_FRAME_BOUNDS` は 110 / 56）。位置の判定は `GetWindowRect` でよい（クランプが渡す物理 outer 座標系と同じ）。作業領域は、プロセスを PER_MONITOR_AWARE_V2 にしてから、`MonitorFromWindow` + `GetMonitorInfoW` の `rcWork` を読む
- **人間の反応時間より短い時限を、手作業のシナリオに要求しない。**
  - 単純反応時間は約 200ms である。それより短い時限（#745 の blur 猶予は 100ms）の内側に手作業を差し込むシナリオは、原理的に実行できない。#745 では、計画レビュー 5 体を通してなお 3 本がこの形で書かれ、実機で初めて不能と分かった
  - 注入での代替も当てにしない。`SetForegroundWindow` は、バックグラウンドからの呼び出しを OS が制限する。PowerShell からの呼び出し自体にも 165ms かかる（実測）
  - 時限より短いウィンドウが要るなら、その不変条件は純粋核のテストへ降ろす（実機は回帰確認だけを担う、と適用範囲を明記して分担する）
- **ウィジェットの位置を知らないまま操作するなら、当たり判定は「副作用が起きたか」で行う。** Tab の回数を 1 つずつ増やして Space を打ち、観測できる副作用（レジストリ値・ファイル・trace）が変化した回数を当たりとする。外れたときは何も起きないので、誤検知が構造的に起きない（#1210 で設定アプリのチェックボックスへ到達した）。座標のクリックと違い、レイアウトが動いても壊れない
- **「プロセスが動いている」は、起動経路の証拠にならない。** どの契機で立ち上がったかを主張するなら、その契機の時刻との差を測る（#1210 のサインイン起動は、対話ログオン `Win32_LogonSession` の `StartTime` と `Process.StartTime` の差 53.9 秒で裏づけた）。存在だけを見ると、別の経路で立ち上がった同名のプロセスと区別できない
- **OS のモーダルループ中（`frame.drag_window()` のドラッグ等）の値を、単発の観測で判定しない。** 合成したマウス移動への追従が不安定で、同じ手順・同じバイナリでも、ウィンドウの top が 956 と 1050 の間で揺れた（最終位置は毎回安定していた）。差を示せるのは、実装の有無を切り替える対照実験だけである

判定者は分ける。ピクセル値やウィンドウの可視性のように決定的なものは、exit code を返す検査へ寄せる。「読めるか」のような非決定的な判断は、調査のツールに留める。実行の前に、フォアグラウンドウィンドウへ入力を送ることと、キーボードから手を離してほしい時間を、人へ明示的に伝える。

#### 別プロファイルで起動するための env ハッチ（`SNOTRA_CONFIG_DIR`）

保存先のディレクトリを差し替える（#803。契約の正本は `SPEC.md` §13 と `Config::config_dir` の rustdoc）。`config.toml` / `history.bin` / `index.bin` / `icons.bin` / `window.bin` のすべてがこの 1 点から導かれるので、検証が実ユーザーのデータに触れずに済む。

```powershell
$env:SNOTRA_CONFIG_DIR = "C:/tmp/snotra-profile"; cargo run -p snotra
```

- 値は、そのまま保存先になる（`Snotra` を付け足さない）。空文字は未設定と同じである
- **展開も絶対化もしない。** `%TEMP%\Snotra` のような値は展開されず、相対パスは CWD を起点にする。不正な値を既定へ落とさないのは意図的で、書き損じたときに実 config を触らせないためである（フォールバックの向きは、「安全そうな方」ではなく「壊れたときに何を守るか」で決める）
- 子プロセスの `snotra-settings` は env を継承するので、同じプロファイルを見る。`cargo run -p snotra-settings` で単独起動したときには効かない
- **実プロファイル以外を指すと分離プロファイルになり、普段使いの Snotra と同時に動かせる。** 単一インスタンス・スタートアップ登録・既定ホットキーが普段使いの Snotra から分かれる（契約の正本は `SPEC.md` §13・#1306）。同じディレクトリを指す 2 回目の起動は、綴りが違っても 1 回目へ転送されて終わる
- 分離プロファイルの初回起動は、ホットキーに `Alt+F12` を書く（実プロファイルの既定 `Alt+Q` と衝突しない）。ただし、不正なホットキーの自動修復・「初期設定に戻す」・インポート・`[hotkey]` の欠落では `Alt+Q` に戻る。普段使いの Snotra が `Alt+Q` を握っていれば、登録失敗の通知が出る
- 自動更新は止まらない。`config.toml` を置かずに起動すると `auto_update` の既定で実リリースを確かめにいき、トーストから更新するとインストール先の実行ファイル（普段使いの Snotra が動かしているもの）を上書きしうる。検証では `auto_update = "disabled"` を書いた `config.toml` を先に置く（`New-SnotraVerificationProfile` の seed はそうしている）

#### updater トーストを出すための env ハッチ

実際の release へ到達しなくても、updater のトーストを描かせる（`egui_shell/mod.rs` の `spawn_update_check` の冒頭。判定より前に置いてあるので、`auto_update` の設定に依らず効く）。

| フラグ | 出る局面 | 何を見るためのものか |
|---|---|---|
| `SNOTRA_EGUI_FAKE_UPDATE` | `Available`（v9.9.9・[今すぐ更新] 付き） | 通常のトースト表示。install 実体は無く押しても no-op |
| `SNOTRA_EGUI_FAKE_UPDATE_FAILED` | `InstallFailed`（長い理由を注入） | **失敗理由の併記と末尾省略（`…`）の唯一の観測点**（#654）。実 install 失敗は再現できない |

```powershell
$env:SNOTRA_EGUI_FAKE_UPDATE_FAILED = "1"; cargo run -p snotra
```

両方を立てたときは、`FAILED` が勝つ（先に `return` する）。

### E. セーフティネットの実装（`.githooks/**`・`.claude/hooks/**`・`scripts/**`）を変更した場合

```bash
npm test    # 必須: セーフティネット自身の回帰テスト（使い捨て repo での hook 実測を含む）
```

**母集団の正本は、`vitest.config.ts` の `include` である。** `npm test` が何を走らせるかは、`include` だけが決める。このカテゴリの見出しに挙げた 3 つは索引であり、`include` から写した一覧ではない。テスト対象のツリーが増減したら、見出しではなく `include` を見る。

- **`scripts/**` は、このカテゴリでしか拾われない**（#1220）。ほかのカテゴリの引き金に当たらないので、このカテゴリを飛ばすと、ガバナンス検査自身の回帰テストが一度も走らない。`governance:check` は代わりにならない。リポジトリの現状を照合するだけなので、判定を壊しても、現行のツリーがたまたま緑なら通る
- **`scripts/lib/**` の PowerShell を触ったら、`npm run test:powershell` も打つ**（母集団は、`scripts/run-pester.ps1` が `scripts/lib` を見て決める）。`npm test` の vitest は `.ps1` を見ない
- `.githooks/` は、main 保護のローカル層である。commit / merge / rebase / push の各操作で git が直接呼ぶので、ツール・シェル・worktree・`git -C` のどれにも依存しない
- **bootstrap**: `npm install` / `npm ci` が、`prepare` スクリプトで `git config core.hooksPath .githooks` を実行する。worktree は `.git/config` を共有するので、一度で全 worktree に効く
- この層は best-effort である。`core.hooksPath` が外れても、GitHub ruleset（`default`）が main への直接 push を拒む。そのため、外れたことを検知する仕組みは意図的に置いていない

### F. ガバナンス文書（`*.md`・`.claude/rules/`・`.claude/skills/`・workflow）を変更した場合

```bash
npm run governance:check    # 必須: ガバナンス文書の決定的検査（参照実在・モジュール索引・SPEC 番号・コマンドマッピング・見出し参照の着地。#587/#593）
```

- **`.rs` のコメントへ正準形の見出し参照を書いたとき、またはその参照先の見出しを改題したときも、`npm run governance:check` を打つ**（#925。G-heading-refs / G-near-heading-refs の走査元に `.rs` が入っている）。fmt / clippy / test は、見出し参照の着地を見ない
- ローカルでこのコマンドを実行するか、PR CI の `governance-check` job（skip-ci の対象外・常時実行）に委ねる
- 検査の実体は、`scripts/governance-check.mjs`（facade）と、`scripts/governance/checks/` 配下の各ファイルである
  - **検査の一覧は、`checks/` ディレクトリの走査が SSOT である**（`scripts/governance/registry.mjs`）。ファイルを置けばそのまま検査になり、範囲を手で書く面が無い。旧来はファイル冒頭のコメント見出しへ範囲を手で書いていたので、黙って古くなった（「G-module-index〜G-config-reachability」と書いたまま、実際は G-check-skill-enumeration まで増えていた・#812。per-check への分割（#1088）が、範囲の手書きそのものを消した）
  - 関連する判断の在りか: 面積 ratchet の文字数指標は `docs/adr/ADR-area-metric-characters.md`、見出し参照の着地は `docs/adr/ADR-canonical-heading-references.md`、config フィールドの到達性は `docs/development-principles.md`「config の値は到達性の検出器を持たない」
  - 意味の判断（責務の妥当性・npm ラッパーの等価性・メモリの整合）は、`/health-check` に残る

## Windows/macOS/Linux で実行可能

```bash
npm test                          # ユニットテスト（Vitest: .claude/hooks + .githooks + scripts）
npm run clean:worktrees          # Agent 委譲で残った worktree/ブランチを掃除（dirty はスキップ、-- --force で強制）
npm run race:boundaries -- --base <分岐元ブランチ>   # /race-check の母集団を列挙（廃止・移行の検査には --include-removed を足す）
npm run prose:sweep -- --plan workspace/plan.md <語>...  # /plan-review の「変更で偽になる散文」を切り詰めずに全件列挙
npm run merge:pr -- <PR> --closes <N,...|none> --body-file <path>  # /merge-pr の squash マージ（閉じる issue をマージの前後で照合）
```

- **`race:boundaries` の `--base` は省略できない。** 差分と未追跡のファイルがともに空なら、非ゼロで止まる。未コミット分が落ちる素の `git diff` を許さないためである（`..` / `...` の 2 点形が作業ツリーを見ない罠は、「検証の作法」の一般則と同根・#922）。出力の仕様（種別の定義・検出パターン・0 件の明示）は `scripts/race-boundaries.mjs` が SSOT で、候補の読み方と 0 件の意味は `/race-check` が持つ
- **`prose:sweep` は、失敗を 0 件として印字しない**（`git grep` の失敗は exit 1、引数の誤りは exit 2）。出力の仕様（面の分類・語ごとの 0 件・★未掲載）は `scripts/prose-sweep.mjs` が SSOT で、★未掲載の裁き方は `/plan-review` が持つ。PowerShell から打つときは、「PowerShell で npm run へ引数を渡すとき」に従う（#1300 では `--plan` が npm に吸われて黙って消えた）。見出し行に `--plan なし` と出るので、落ちたことは出力で分かる
- **`merge:pr` は書き込み（マージ）を伴う。** ただし `--dry-run` を付けたときは、マージ前の検査だけを行い、マージせずに止まる（#1318）。人が `/merge-pr` で起動するもので、終了コードの意味と次の一手は `/merge-pr` が、使い方と構造は `scripts/merge-pr.mjs` が持つ

## Windows のみ実行可能（`windows` クレートや Win32 API・実行バイナリに依存）

```bash
npm ci                           # 依存インストール（初回セットアップ・CI）
npm run test:powershell         # Pester（cargo metadata の debug 本体を使用。未ビルド時は先に cargo build -p snotra）
cargo test -p snotra-core        # ユニットテスト（純ロジック層）
cargo test -p snotra-egui-runtime # ユニットテスト（egui入力・IME・Surface の描画失敗リトライ方針）
cargo test -p snotra             # ユニットテスト（Tauri 統合層: state/indexing/config_watcher 等）
cargo test -p snotra-settings    # ユニットテスト（設定 GUI の純ロジック: font face 検証・TOML エラーローカライズ）
cargo test --release -p snotra-core bench_ -- --ignored --nocapture  # 検索パフォーマンス計測（詳細: PERFORMANCE.md）
cargo test --release -p snotra-core --test memory_footprint -- --ignored --nocapture --test-threads=1  # 索引の常駐メモリ実測（詳細: PERFORMANCE.md）
cargo test --release -p snotra-core --test path_query_cost -- --ignored --nocapture --test-threads=1   # パスクエリ全走査のコスト実測（詳細: PERFORMANCE.md）
cargo test --release -p snotra-core --test path_query_cost at_operating_point -- --ignored --nocapture --test-threads=1  # path_query_cost のうち実運用点（PATH マージ + 実 config の normal_mode）を 2×2 で測る（#1067）
cargo test --release -p snotra-core --test path_query_cost across_settings -- --ignored --nocapture --test-threads=1     # 設定の組み合わせ 36 構成で c:\ のコストを掃く（改修の適用範囲を決める・#1067）
cargo test --release -p snotra-core --test match_cost -- --ignored --nocapture --test-threads=1  # 名前の照合の壁時計と結果のダイジェスト（全件走査になる打鍵・実 index.bin と合成・詳細: PERFORMANCE.md）
cargo test --release -p snotra-core --test search_allocations -- --ignored --nocapture  # 検索 1 回あたりの一時確保（回数・バイト数。計数器込みゆえ µs は壁時計ではない・詳細: PERFORMANCE.md）
cargo check --workspace          # Rust 全 crate 型チェック
cargo clippy --workspace --all-targets -- -D warnings  # lint チェック（カテゴリ A と同じ）
cargo fmt --all                  # 整形の**修復**（検査は カテゴリ A の `cargo fmt --all -- --check`・#858）
cargo run -p snotra-settings     # snotra-settings（egui ネイティブ設定 GUI）の単独起動
cargo run -p snotra              # 製品メインウィンドウ（egui 既定・#532 SU7 flip 済み。視覚スモークはこれ）
npm run verify                   # Rust + node 一括検証（cargo check --workspace + npm test）
npm run smoke:startup             # 起動時スモーク（trace 出力・検証用プロファイル・非 first-run の検証）
npm run smoke:egui                # egui 経路の show/hide スモーク（keybd_event 注入 + trace検証・#532 SU7。既定は debug 本体を導く・CI は release を明示）
npm run bench:startup             # 起動の端から端までの内訳実測（区間ごとの min/p50/max・詳細: PERFORMANCE.md）
pwsh -NoProfile -File scripts/occupy-hotkey.ps1 -DurationSeconds 120  # 検証用プロファイルの既定ホットキーを先に握り、本体の RegisterHotKey を実際に失敗させる（startup:failed の検算・#1009。用途の正本はスクリプトの .NOTES）
npm run measure:memory            # メモリ実測（PrivWS 軸・ツリー合算・#532 flip 基準 3）
npm run measure:memory:stages     # メモリ実測（起動→表示→検索→hide の段階別・前景計測。実行中の snotra を kill する）
npm run tauri build              # リリースビルド（NSIS バンドル。`prepare:sidecar` で binaries/ を用意してから）
```

`tests/memory_footprint.rs` の `--test-threads=1` は外せない。計数アロケータは `static AtomicUsize` のプロセス大域であり、並列に実行すると、2 つのテストが計数器を奪い合う。失敗にはならず、もっともらしい数値が出るので、内部矛盾（規模に対する単調性の破れ・`live 0.00 MiB`）に気づかなければ、そのまま結論に使ってしまう（2026-08-07 実測。詳細は PERFORMANCE.md の「索引の常駐の内訳」）。

`npm run bench:startup` は、release の本体を測る。先に `cargo build --release -p snotra` を打つこと。`cargo test` はテストターゲットしかビルドせず、古い本体が在ると、検査は成功したまま変更前の挙動を測る（#835 で 2 時間前のバイナリを測った）。出力は区間ごとの min / p50 / max で、最小値にまとめない。このハーネスが答える問いは、「起動が何 ms か」ではなく「分散がどの区間にあるか」だからである（詳細は PERFORMANCE.md）。

`-ExePath` の既定を持つ起動ハーネスも、既定の本体を `test:powershell` と同じ形で導く（対象の正本は `scripts/lib/SnotraSmoke.Tests.ps1` の既定 ExePath の検査、debug か release かは各スクリプトの既定）。起点はスクリプトが置かれたリポジトリであって、cwd ではない。そのため、worktree から既定のまま回せば、worktree の本体を測る（#1179。以前は絶対パスが直書きされており、別の作業コピーを黙って測りながら緑を返していた）。測った本体の出し方は、`Resolve-SnotraCargoExecutable` の doc が正本である。別の本体を指すときだけ `-ExePath <path>` を渡す。明示した相対パスは、これらのハーネスでは（`test:powershell` と違い）cwd 相対のまま解釈される。

## git blame と整形コミット

`.git-blame-ignore-revs` は、`cargo fmt --all` の一括整形コミット（#858）を blame から隠す。GitHub の blame ビューは、設定なしで自動適用する（ルート直下に置くことが条件・実測）。ローカルの `git blame` は自動ではないので、使うなら次を 1 度実行する（任意。機構では強制しない）。

```bash
git config blame.ignoreRevsFile .git-blame-ignore-revs
```

## スモーク運用メモ

- `scripts/lib/SnotraSmoke.psm1` は、各検証スクリプトに共通する部品を所有する。config seed の骨格・env の設定と復元・Cargo target からの本体の導出・既存プロセスの方針・起動・ウィンドウと trace の待機・キー注入・DWM/DPI に対応したキャプチャである。各 smoke の合否条件と固有の TOML 節は、呼び出し側に残す
- `scripts/lib/SnotraSmoke.Tests.ps1` が検査するもの
  - 単体: env の正常時・例外時の復元、Cargo target からの導出、trace の parse、既存プロセスの方針
  - 実バイナリでの統合: seed の parse・意図したプロファイルへの `index.bin` の生成・ウィンドウ矩形とキャプチャ寸法の一致。加えて、起動後の最初のフレームで、入力欄が打鍵を受け取れる状態になっていること（#872 で機序を再設計した。キャレットの断言は `src-tauri/src/egui_shell/view.rs` の kittest が実コードの並びごと縛るので、実機側は打鍵を注入しない）
- `scripts/smoke-startup.ps1` は `SNOTRA_TRACE=1` で起動し、trace が 1 件以上出ることを要求する
  - **プロファイル**: 検証用プロファイル（`SNOTRA_CONFIG_DIR` で `target/smoke-startup/profile` を指す・#804）を、ループの前に 1 回だけ seed し、5 回の起動で共有する。実 config には触れない。毎回作り直さないのは、「first-run でない起動」を測る今の意味論を保つためである
  - **肯定的な検査**: seed が正常に読まれたこと、first-run を踏んでいないこと、env が効いたこと（プロファイルに `*.bin` が生成されたこと）を確かめる
  - **保証しないもの**: 汎用的な「起動エラーが無いこと」。trace の失敗名には統一した分類がなく、正常系でも起こりうる best-effort の失敗も含むためである（#845）
  - **trace 0 件は失敗にする**（#690 follow-up）。起動経路を何も観測できていないからである。CI runner のコールドスタートで実際に trace 0 行を観測しており、旧 smoke はその状態でも緑を返していた
  - **待ち方**: 最初の trace を待ってから、観測時間 `WaitMs` を始める（`-FirstTraceTimeoutMs`・既定 12s）。固定待機だけだと、遅い側に振れた起動が丸ごと無音になる（同じ runner・同じバイナリで、最初の trace までが 0.6s〜8s 超とばらつき、5 回中 3 回が無音だった）。固定待機を一律に伸ばさないのは、速い起動まで毎回待つことになるためである
  - **成功時にも出す値**: サマリ表の `event_count`（検査が実際に何かを見たことを示す肯定的な報告）と、`first_trace_ms`（分散の原因が未解明なので、予算に触れる前に悪化を読めるようにする。`n/a` は予算内に 1 行も出なかったことを表す）
- `scripts/smoke-egui.ps1` は、egui 経路の自動回帰の最低線である（#532 SU7・e2e/ 撤去後の後継）
  - **流れ**: `SNOTRA_TRACE=1` で起動し、keybd_event などで注入して、trace のイベントを観測する。起動は 2 回（既定のプロファイルと toast あり）である。検査の一覧と順序の正本は、同スクリプトの `# --- … ---` の見出し行である（ここに写すと、検査を足すたびに古くなる）
  - **注入する hotkey**: 起動時の `hotkey:registered` trace から導いた VK 列を、押下順に押し、解放まで含めて注入する（対応表の SSOT は `src-tauri/src/platform/hotkey.rs` の `injection_vks`）。`-HotkeyVks` を明示すると、trace より優先される（trace を出さない旧バイナリの検証など）
  - **プロファイル**: 検証用プロファイル（`SNOTRA_CONFIG_DIR` で `target/smoke-egui/profile` を指す・#804）へ、最小の有効な TOML を常に seed する。実ユーザーの `%APPDATA%\Snotra` は、読みも書きもしない。退避も復元も持たないことが、異常終了しても実 config が壊れない構造的な保証である。seed を置く理由は、`scripts/lib/SnotraSmoke.psm1` の `New-SnotraVerificationProfile` のコメントが正本である
  - **肯定的な検査**: seed が読めたこと・first-run を踏んでいないこと・env が効いたこと（プロファイルに `*.bin` が生成されたこと）を、3 つとも確かめる。「観測できなかった」を合格と読ませないためである
  - 実行中の snotra を kill するので、ローカルで実行するときは注意する。網羅は担わず、視覚と操作列は手動の GUI smoke（カテゴリ D）が補う
- `scripts/smoke-egui.ps1` は、results ウィンドウの表示も検査する（#671/#673 サイクル PR A）。`egui_show:done` の後に 1 文字のクエリを注入して `egui_results:show` を観測し、Escape 後の `egui_hide:done` に続けて `egui_results:hide` も観測する。#804 以降、skip は無い。検証用プロファイルを常に seed するので、既定のクエリ `"z"` は、seed した索引の 1 件に必ず一致する（`-ResultsQuery <letter>` は残るが、開発機の既存の索引に合わせる用途は無くなった。空文字を渡す用途は次の項）（CONTRIBUTING.md の「results ウィンドウ show/hide の trace 観測」と対応）
- **results の検査は、無条件に要求される（#686 の `-RequireResults` を #804 が格上げした）**
  - 旧 flag が opt-in だったのは、「ローカルでは索引を制御できないのが普通」という理由の緩和だった。検証用プロファイルを常に seed する以上、その前提は偽になったので、flag ごと撤去し、常に要求する形にした
  - これは検出器の削除ではなく、格上げである。従来ローカルでは既定で skip（緩和）だったものが、ローカルでも赤になる
  - 判定はアプリの起動前に確定するので、このガードはプロセスを起こさずに落ちる。`pwsh -File scripts/smoke-egui.ps1 -ResultsQuery '' -ExePath <任意の既存ファイル>` でフォールトインジェクションでき、実機には触らない（空文字の明示が、その注入口である）
  - skip へ至る見逃しの経路は、構造的に無くなった。ほかの経路（実行ファイルの不在・`hotkey:registered` の未観測・`egui_show:done` の未観測・`egui_results:show` の未観測・クエリが A-Z の単字でない）は、いずれも exit≠0 で赤になる
  - `smoke.yml` のステップの順序も自由になった。両 smoke が自分の検証用プロファイルを持つので、startup smoke の 5 回の起動が egui smoke の seed を壊すことはない（#803 で `SNOTRA_CONFIG_DIR` が入り、#804 が smoke 2 本を env 化した）
- **索引の規模に依存する性能は、smoke では測れない**（#1004）
  - `smoke-egui.ps1` は scan 対象を上書きする引数を持たず、seed する索引の規模は固定である（正本は同スクリプトの seed）。そのため、索引の件数をゲートに持つ検知器を置くと、CI でもローカルでも永久に SKIP になる。#930 が戒めた「発火しえない検出器」そのものである
  - 規模に依存しない検知器なら置ける（`scripts/lib/SnotraTraceInvariants.psm1` の H7 は seq の大小しか見ない）
  - 索引の規模に依存するフレーム性能は、実運用点での手動計測が唯一の観測手段である。その読み方は `PERFORMANCE.md`「計測と受け入れ基準」が持つ
- **smoke が赤いときは、`--- 失敗時の証拠 ---` ブロックを先に読む**（#690 follow-up）
  - プロセスの生死（既に終了 = 起動の途中で落ちた / 生存中 = 起動はしたが未到達・遅延）と、trace の行数が出る
  - `trace 行数: 0` は、それだけでは「起動していない」とも「イベントが出ていない」とも読める。必ず生死と併せて読む
  - 失敗の経路は `throw`（前提の崩壊）と検査項目の不合格の 2 本あり、どちらも同じ証拠を出す。以前は後者にしか証拠が無く、`throw` は手掛かりを残さずに終了していた（`-StartupWaitMs 0 -ObserveTimeoutMs 1` でフォールトインジェクションできる）
- **`hotkey:registered` だけは別の予算を持つ**（`-StartupObserveTimeoutMs`・既定 25s）。起動後の最初の観測だけが、cold start を含むからである。以降（show / hide / results）はアプリの起動が済んだ後なので、`ObserveTimeoutMs` のままにする（一律に広げると、本来速い検査の失敗検出まで鈍る）。CI では、「起動から 12,000ms 経っても trace 0 行・プロセスは生存」を 3 回、「起動から 0.6s で観測」を 2 回実測しており、この二極の原因は未解明である。この予算は原因究明までの緩和であって、修正ではない
- **壁時計から起動レイテンシを推定してはならない**（実際に一度誤った）。seed の print から hotkey の観測までの壁時計には、`Add-Type -MemberDefinition`（実行時の C# コンパイル。コールドスタート直後の runner で 7〜25s 変動する）を含む、起動前の時間が乗る。アプリの遅延を表すのは、起動を起点にした計測だけである。両者を混同すると、「境界に乗っていた」のような誤った結論に達する
- **成功時にも観測レイテンシを出す。** 予算を広げただけだと、起動が遅くなっても、予算内に収まる限り緑で気づけない。毎回数字が出れば、退行を人が読める。値は「観測できたのが何 ms 後か」であって、アプリの準備時間ではない（下限が `StartupWaitMs` で頭打ちになる）ので、固定の待機時間を併記する
- **0 行かつプロセスが生存しているときは、事後観測に入る**（`-PostMortemWaitMs`・既定 30s・失敗時のみ）。最初の 1 行が出れば「遅延（起動から約 N ms）」、出なければ「未到達」と報告する。観測時間を延ばす前に、この数字を取ること。遅延なら延長の根拠になり、未到達なら延ばしても解決しない。合否には影響しない（失敗は失敗のまま）
- **ビルド済みのバイナリで手動 smoke をするときは、変更が含まれているかを確かめる。** `cargo` は `target/debug/deps/<crate>-<hash>.exe` を `target/debug/<crate>.exe` へ hardlink し直す。そのため、ソースを変えた後でも（fingerprint の上で「最新」と判断されて再リンクされず）、`<crate>.exe` のタイムスタンプが更新されないことがある。タイムスタンプを信用せず、変更に固有の文字列でバイナリを grep し、目的の変更が入っているかを確かめる（例: `[Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe)).Contains('<変更固有の文字列>')`）。#343 の balloon smoke で実際に踏んだ
- **debug ビルドのネイティブ GUI では、`Process.MainWindowHandle` を画面の証拠に使わない。** console subsystem の debug バイナリは、親の Windows Terminal の HWND を返すことがある。対象 PID のトップレベルウィンドウを列挙し、PID と期待するタイトルの両方でネイティブウィンドウを特定してから、可視性とスクリーンショットを検証する

## CI/CD メモ

### 検証コマンド ↔ GitHub Actions workflow の対応

「変更後の検証チェックリスト」の必須コマンドが、どの workflow で、どのトリガーで自動実行されるかの対応表である。エージェントが「PR CI が緑」だけで全検証済みと誤認しないための SSOT である。

| 検証コマンド | workflow | トリガー |
|---|---|---|
| `npm test`（Vitest: hooks/githooks/scripts） | `ci.yml`（node-check=ubuntu / rust-check=windows） | PR 自動（`skip-ci` を貼ると job ごとスキップ） |
| `npm run test:powershell`（Pester: 共有 smoke 配管 + 実バイナリ統合） | `ci.yml`（rust-check） | PR 自動（`skip-ci` を貼ると job ごとスキップ） |
| `cargo fmt --all -- --check`（#858・整形ゲート） | `ci.yml`（rust-check） | PR 自動 |
| `cargo check` / `cargo test -p snotra-core` / `cargo test -p snotra-egui-runtime` / `cargo test -p snotra` / `cargo test -p snotra-settings` / `cargo clippy` | `ci.yml`（rust-check） | PR 自動 |
| `cargo check -p snotra --features heap-trace`（ヒープ計装・既定ビルド外の feature） | `ci.yml`（rust-check） | PR 自動 |
| `cargo doc --workspace --no-deps --document-private-items`（#562・intra-doc link 検査） | `ci.yml`（rust-check） | PR 自動 |
| `npm run governance:check`（#587・ガバナンス文書検査） | `ci.yml`（governance-check） | PR 自動（**`skip-ci` 非対象** — if ガードを持たず常時実行） |
| `npm run smoke:startup`（注） | `smoke.yml`（smoke-egui job） | 対象 paths を含む PR（自動）/ main への push（**全マージ**・rust-cache の warm が目的）/ 手動 dispatch |
| `npm run smoke:egui`（#532 SU7・egui 経路の自動回帰・注） | `smoke.yml`（smoke-egui job） | 対象 paths を含む PR（自動）/ main への push（**全マージ**・rust-cache の warm が目的）/ 手動 dispatch |
| `npm run smoke:startup`（**配布前の起動ゲート**） | `release.yml`（build-and-release job） | `create-release.yml` の手動 dispatch からの `workflow_call` |

（注）CI では、smoke-egui job がビルドした release バイナリを共有する。そのため、`npm run smoke:startup` / `npm run smoke:egui`（既定の ExePath は debug）ではなく、`pwsh -NoProfile -File scripts/<name>.ps1 -ExePath target/release/snotra.exe` を直接実行する（npm を経由しない理由は、`smoke.yml` の smoke-egui job のコメント）。検証する起動経路は同じである（release バイナリが trace を出し、seed 済みの検証用プロファイルで非 first-run 起動すること）。これは smoke 用ビルドの起動の健全性の検証であり、配布バンドル（`tauri build`）の検証ではない。その帰結として、この job のバイナリだけは、`[profile.release]` の `lto` / `codegen-units` を env で緩めてビルドする（根拠は `smoke.yml` の "Build release binary" ステップのコメント。`Cargo.toml` は変えないので、`release.yml` がビルドする配布物は fat LTO のまま）。`panic = "abort"` と `opt-level` は共有するので、検証対象である起動時の挙動は、配布バンドルと同じ経路を通る。

（注2）`release.yml` での同じスクリプトの実行は、配布前のゲートである。成果物の存在の検証だけでは起動できるかを保証しないので、build 済みの `target/release/snotra.exe`（NSIS インストーラーではない）へ同じ起動スモークを当て、アップロードの前に起動の失敗を検知する（理由の正本は当該ステップのコメント）。表の 2 行が同じコマンドを指すのは、走る局面が別だからである。

- `npm test` は、ubuntu（node-check）と windows（rust-check）の両方で走る（#509）
  - `.githooks` / `.claude/hooks` の selftest は、実運用が Windows でだけ起きるセーフティネットである。hook の実行機構（Git-for-Windows の shebang 経由の sh 起動・パスとクォートの境界）が本番と一致する OS で、回帰を検査する
  - ubuntu 側は、実行ビットと POSIX sh の厳密性を、補う形で担保する
  - CRLF 由来の fail-open は、`.gitattributes` の `.githooks/** text eol=lf` で両 OS とも回避済みである。また dash 側の故障の形なので、windows 固有ではない
- **`skip-ci` ラベルは、ジョブ単位で効く**
  - node-check / rust-check の `if` が同じなので、貼ると cargo 系を含む両方がまるごとスキップされる（表の各行に個別の注記はしない）
  - `governance-check` job は `if` ガードを持たず、`skip-ci` を貼っても走る（#587）。skip-safe と定義された Markdown だけの変更こそが検査対象なので、意図的にガードしない
  - CI は required status check ではない（ruleset `default` に required status checks の規則が無い・実測）ので、スキップしてもマージは通る
  - スキップした node-check / rust-check は、マージ後の main への push では必ず走る。両 job の `if` が `github.event_name == 'push' ||` で始まり、push ではラベルを評価しないためである。そのため、表のトリガー列の「PR 自動」は PR 側の記述であって、push 側を否定しない（列に push を書き足さないのは、6 行が同じ 1 つの事実の写しになるからである）
  - `governance-check` が push で走る理由は、これとは別である。そもそも `if` を持たないことによるもので、同じ `github.event_name == 'push'` で説明してはならない。ただし、それは job 自体の話である。job の中の `governance manifest delta` step だけは、別に `if: github.event_name == 'pull_request'` を持ち、push ではこの 1 step だけが走らない
- **`skip-ci` を貼ってよいのは、skip-safe な変更だけである**
  - skip-safe なのは、node-check / rust-check がテスト対象に持たない `.claude/skills/**`・`.claude/rules/**`・`.claude/agents/**`・`docs/**`・`**/*.md` だけである（これらの決定的な検査は、skip されない governance-check が担う・#587）
  - 貼ってはならないのは、`.claude/hooks/**`・`.githooks/**`・`scripts/**`・`.claude/settings.json` である。これらは、`npm test` が両 OS でセルフテストを回す（`vitest.config.ts` の `include`・上の #509）
  - 「`.claude` だけの変更だから安全」と一括りにしない。同じ `.claude/` という見た目が、「Claude が読むだけの設定」と「CI が検査するセーフティネット」の 2 つの概念を担っているためである（#500）
- カテゴリ C（ウィンドウ生成・ホットキー・スラッシュコマンド）に相当する変更や、依存の更新を含む PR は、対象の paths（`src-tauri/**`・`**/Cargo.toml`・`Cargo.lock`・`package.json`・`package-lock.json` 等）に当たるので、`Smoke` workflow が自動起動する。paths の外の変更で手動実行するには、`workflow_dispatch` を使う
- **`Smoke` は main への push でも起動する。そちらは `paths` を持たず、全マージで走る。** 目的は回帰の検出ではなく、rust-cache の warm である
  - Actions のキャッシュは、`pull_request` の実行による書き込みが PR 自身のスコープに閉じる。base ブランチ（main）のスコープだけが、全 PR から読める
  - `paths` で絞らないのは、キャッシュキーが Cargo.lock だけでなく rustc のバージョンでも変わり、後者を paths で表せないからである（`dtolnay/rust-toolchain@stable` は 6 週ごとに動く。絞ると、更新の週から次の Cargo.lock の変更まで、全 PR が完全なコールドへ戻る）
  - PUBLIC リポジトリなので、Actions の分数課金は無い
  - main の run だけ `cancel-in-progress` を外してあるのも、同じ目的による（キャンセルされると、キャッシュを保存する Post ステップに到達しない。理由と実測は `smoke.yml` のコメント）
- 表のコマンドが、対応する workflow のどの `run:` にも現れないドリフトは、`npm run governance:check`（G-ci-table）が検出する（#587）。ただし、トリガー列は照合されない。記述が実際の `on:` / `paths` と食い違っても緑のままで、判定は `/health-check`「Check 10 — docs/build-commands.md ↔ .github/workflows/\*」の意味判断に残る。これは理屈の上だけの穴ではない。2026-08-16 まで、smoke の 2 行が main への push を「依存 manifest の変更時」と限定していたが、実際の `push:` は `paths` を持たない（全マージ）

### その他

- **ワークフロー間の連鎖には、`workflow_call`（呼び出し元から直接呼ぶ）を使う。** `GITHUB_TOKEN` では、ほかのワークフローを起動できない。tag の push や `workflow_dispatch` を `GITHUB_TOKEN` で発火させても、別のワークフローは起動しない（GitHub の仕様）
