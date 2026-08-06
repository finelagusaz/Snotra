# ADR-smoke-build-time: smoke-egui のビルド短縮を、キャッシュの修正とプロファイル緩和の 2 点で行う

`Smoke` workflow の所要 10 分のうち 84% がビルドだった。削り方には複数の候補があり、そのうち「速いが検証対象を変える」ものと「無駄が減るように見えて穴を作る」ものを却下した記録。

## 文脈

run 31070343704（2026-08-06）の実測:

| 区間 | 所要 | 性質 |
|---|---|---|
| job 全体 | 10m18s | 直近 3 run は 10m21s / 11m39s / 10m21s |
| Build release binary | 8m39s | 全体の 84% |
| ├ 依存 357 crate のコンパイル | 5m23s | **キャッシュで消せる** |
| └ 本体の codegen + fat LTO + リンク | 3m15s | **キャッシュでは消せない**（rust-cache は workspace crate を保存しない） |

原因は `Swatinem/rust-cache` の `workspaces: src-tauri` である。このリポジトリはルートが cargo workspace で `src-tauri` はそのメンバーゆえ、`cargo build -p snotra` が書くのは**ルートの `target/`** だった。キャッシュ対象は存在しない `src-tauri/target` を指し、復元していたのは `~/.cargo` のレジストリだけ（132MB。ルート指定の rust-check は 2573MB）。ログには `full match: true` と出るため、**緑が空振りを隠していた**。

制約が 2 つある。**(1) Actions キャッシュの合計が 9.88GB / 10GB** で、PR ごとに GB 級を書くと LRU で rust-check の 2573MB が落ち、CI 全体が今より遅くなる。**(2) `pull_request` 実行のキャッシュ書き込みは PR 自身のスコープに閉じる**——実測で smoke-egui のエントリ 8 件がすべて `refs/pull/*/merge` だった。全 PR が読めるのは base ブランチ（main）のスコープだけである。

## 決定

1. `workspaces:` を指定しない（既定の `.` ＝ルート）。
2. `save-if: ${{ github.ref == 'refs/heads/main' }}`。PR からは保存せず、エントリを 1 個に抑える。
3. `push: branches: [main]` を `paths` 無しで足し、main の run で warm する。`cancel-in-progress` は main だけ外す。
4. "Build release binary" ステップ限定の env で `CARGO_PROFILE_RELEASE_LTO=false` / `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`。

## 検討した代替案と却下理由

- **`--release` をやめて debug でビルドする**: 却下。startup の予算 25s には収まる公算が高いが、`panic = "abort"` → `unwind` の挙動差が**この job の検証対象（起動健全性）と同じ面に乗る**。速さのために検証対象そのものを変える方向であり、削るべきは検証ではなくビルドの無駄である。採った env 上書きは `lto` と `codegen-units` の 2 キーだけを動かし、`panic` / `opt-level` / `[profile.release.package.snotra]` を共有する。
- **`push` の `paths` を `Cargo.lock` / `**/Cargo.toml` / `e2e.yml` に絞る**: 却下。「キャッシュを作り直すべき条件」を言い当てたつもりになるが、**キャッシュキーは Cargo.lock だけでなく rustc のバージョンでも変わり、それは paths で表現できない**（`dtolnay/rust-toolchain@stable` は 6 週ごとに動く）。絞ると rustc が更新された週から次に Cargo.lock が動くまで、全 PR が完全 cold へ戻る——数週間続きうる。
  - **`schedule:` で週 1 warm して穴を埋める**: 却下。機構が 1 つ増えるうえ、rustc 更新直後の最大 1 週間は cold のままで穴が閉じ切らない。main への push は squash マージのみ（直 push は GitHub ruleset が拒否）で、PUBLIC リポジトリゆえ分数課金も無い。**条件で言い当てるより、毎マージで warm し直すほうが穴が無い。**
- **`ci.yml` の rust-check とキャッシュを共有する**（rust-cache の `add-job-id-key` を落としてキーを揃える）: 却下。rust-check は debug、smoke は release でプロファイルが違う。キーを揃えると先に保存した側が後続を `Cache up-to-date` で止めるため、**release の target が一生入らない**。
- **rust-check の job へ smoke を統合する / artifact でバイナリを受け渡す**: 却下。プロファイル不一致は上と同じで、加えて smoke が rust-check 全体（実測 4m）の後ろに直列化される。
- **`Cargo.toml` の `[profile.release]` を直接緩める**: 却下。`release.yml` が建てる**配布物まで**最適化が落ちる。env をステップ限定に置けば、緩むのはこの job のバイナリだけになる。

## 受容する残余

- **rust-cache のキャッシュキーに `CARGO_PROFILE_*` は入らない**（ログの "Environment considered" は `CARGO_HOME` / `CARGO_INCREMENTAL` / `CARGO_TERM_COLOR` のみ・実測）。将来 env を変えてもキーは変わらないが、判定は cargo の fingerprint が行うので**誤ったバイナリは出ない**。現れるのは「キャッシュが効かない run が 1 回増える」だけである。
- **`lto` を切ると依存側の fingerprint も変わる**（`-C lto` が消え `-C embed-bitcode=no` が付くことを最小 bin crate で実測）。ゆえに**この決定を入れた直後の 1 run は必ず cold** であり、その所要を効果の測定に使ってはならない。
