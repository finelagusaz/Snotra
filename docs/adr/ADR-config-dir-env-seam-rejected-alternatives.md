# ADR-config-dir-env-seam-rejected-alternatives: `SNOTRA_CONFIG_DIR` の seam で却下した代替案

- 状態: 採択（2026-07-28・#803）
- 文脈: 検証・デバッグのために「実 config を壊さず別プロファイルで起動する」手段を入れるにあたり、
  4 つの代替案を検討して却下した。採用案そのもの（`Config::config_dir()` の env 上書き）は
  コードと `SPEC.md` §13 が持つので、ここには**却下した案と理由だけ**を残す。

## 1. 不正な上書き値（相対パス・未展開の `%VAR%`）を既定へフォールバックさせる

**却下。** 一見 fail-safe だが、**この用途では危険な向き**である。

`SNOTRA_CONFIG_DIR` を使う主な消費者は検証スクリプトであり、パスを書き損じたときに既定へ落ちると
**検証がユーザーの実 config を触る**——この seam が消そうとしている当のものが復活する。
そのまま使えば、書き損じても CWD 配下の変な場所へ隔離されるだけで実データには届かない。

**フォールバックの向きは「安全そうな方」ではなく「壊れたときに何を守るか」で決める。**
挙動は `config_dir_from_does_not_expand_or_absolutize_override` が固定する。

（Windows は `std::env::var_os` の `%VAR%` を展開しない。実測: `%TEMP%\Snotra` は
`Path.IsPathRooted` が `False` で、`GetFullPath` すると CWD 起点になる。）

## 2. `Config` の `hotkey` / `appearance` / `paths` に `#[serde(default)]` を付け、seed 自体を不要にする

**却下。** 必須セクションの欠落が「破損復旧経路」（`.bak` 退避 + 既定値起動）を踏む現在の挙動は、
`config.toml` の部分的な破損を黙って既定値で塗り潰さないための設計である
（`snotra-core/CLAUDE.md`「読み込み失敗は種類で扱いを分ける」）。

**検証スクリプトの都合で製品のデータ保全方針を緩めない。** 代わりに、検証プロファイルへ
必須セクションを含む最小の有効 TOML を書く。

## 3. `scripts/smoke-egui.ps1` と seed TOML を共有ヘルパーへ括り出す

**却下（重複を選ぶ）。** `smoke-egui.ps1` は `e2e.yml` の `-RequireResults` ゲートに載る CI 経路であり、
背景色検証と無関係な理由でそこを触るリスクを負う。`scripts/` に共有ライブラリの下地も無い。
さらに 2 つの seed は目的が違って**同型ではない**——smoke 側は results 窓を出すため
`[[paths.scan]]` にダミーを 1 件置き、visual-check 側は索引 0 件で即終了させたいので置かない。

代わりに**両方の seed に相互参照コメントを置く**（片方だけ直る事故を防ぐ）。
smoke 側の env 化は #804 のスコープ。

**その後（#804）**: `-RequireResults` は撤去され（results 検査は無条件の要求へ格上げされた）、
上の却下理由のうち**「`-RequireResults` ゲートに載る CI 経路だから」は失効した**。ただし
**「2 つの seed は同型ではない」は今も真**であり、`smoke-startup.ps1` の env 化で seed は 3 つに
増えた（索引 0 件・`[[paths.scan]]` なし）。共有化するかどうかの判断は #843 が引き取る。
**却下の判断そのものは書き換えない**——当時の文脈を残すのが ADR の目的である。

**その後（#843）**: 3 本で増殖した config seed・env 復元・Win32/DWM 配管を
`scripts/lib/SnotraSmoke.psm1` へ共有したため、この却下は現在の実装判断としては失効した。
ただし共有するのは必須セクションの骨格とライフサイクルだけである。`smoke-egui` の scan、
`smoke-startup` の 5 起動共有、`visual-check-colors` の色と専用 scan は各呼び出し側が渡し、
**3 つの seed は同型ではない**という制約は維持する。

## 4. 検証プロファイルの掃除のために `cargo metadata` で `target_directory` を引く

**却下。** `CARGO_TARGET_DIR` を設定した環境ではリポジトリの `target/` が `cargo clean` の対象から
外れる（実測: `CARGO_TARGET_DIR` を渡すと `cargo metadata` の `target_directory` が移る）。
これを機構で解こうとすると、掃除のためだけに cargo 呼び出しが 1 つ増える。

スクリーンショット置き場（`$shotDir`）が既に同じ前提の上に在るので、**前提条件を明示して
受容する**方を選んだ（`AGENTS.md`「全称表現は前提条件とセットで書く」）。残るのはユーザー資産では
ないので、掃除が漏れても害が小さいことが判断を支えている。

## 5. seed の健全性を `config.toml.bak` の不在で測る

**却下（当初案の訂正）。** 退避は best-effort で、`backup_invalid` の `fs::rename` が失敗すると
**parse 失敗でも `.bak` は現れないまま** `RecoveredFromCorrupt` を返す（`snotra-core/src/config.rs`）。
「副作用の不在」で「処理の成功」を測っていたのが誤りだった。

判定は本体 stderr の `[config] ` 行の不在へ置き換えた——この eprintln は読み込み失敗の**全 arm に在り、
成功時には出ない**ので、診断そのものを見ることになる。

## 6. `-RequireResults` を残して既定 ON にする（#804）

**却下。** 検証用プロファイルを常に seed する以上、skip へ至る経路は構造的に消える。flag を残せば
**到達不能な分岐が残り、読者に「skip がありうる」と誤解させる**。「`-RequireResults` だけ残す」案も
同じ理由で却下した——守る対象の無い検出器になる。

flag ごと撤去して**無条件の要求へ格上げ**した。これは検出器の削除ではない: 従来はローカルで既定
skip（緩和）だったものが、ローカルでも赤になる。緩和の前提（「ローカルでは索引を制御できないのが
普通」）がプロファイル分離で偽になったための格上げである。

## 7. `-ResultsQuery` もパラメータごと撤去する（#804）

**却下。** 計画レビューの独立再導出が「seed した索引 1 件に対して `z` 以外は必ず失敗するので、
選ぶ理由の無い罠である」として撤去を推奨した。理由は正しいが、**このパラメータは撤去した
`-RequireResults` の後継として、フォールトインジェクションの注入口を担っている**——`-ResultsQuery ''`
を明示的に渡すと results 検査の guard が起動前に落ちる。撤去すると「アプリを起こさずに赤を出す」
性質（`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」）
の入口が無くなる。

## 8. `smoke-startup` のプロファイルを 5 起動それぞれで作り直す（#804）

**却下。** 毎回作り直すと 5 回すべてが first-run になり、**いま CI が測っているもの（first-run でない
起動）とは別のものを測り始める**。`e2e.yml` は first-run を本 job の検証対象外として明示的に受容して
おり、#804 の目的は実 config への依存を切ることであってカバレッジの変更ではない。

ループ前に 1 回 seed して 5 起動で共有する形を採り、**first-run を踏んでいないことは肯定的に検査する**
（`cmd:launch_settings_process:*` は `*:error` で終わらないため、既存のフィルタからは構造的に見えない）。
