# ADR-claude-code-ra-lsp-plugin-delivery: Claude Code の rust-analyzer 設定を repo 所有の LSP plugin で配送し、抑制の検知は自前カナリアに置く

#1083。Claude Code が起動する rust-analyzer（RA）へ、そのインスタンスにだけ効く設定を渡したい。VS Code 側の RA は巻き込まない。

## 文脈

issue は「repo 所有の plugin root に `.lsp.json` を置き、`initializationOptions` で `checkOnSave=false` と `diagnostics.enable=false` を渡す」という方針を、公式 docs（ChatGPT 経由の URL）を根拠に提案していた。**形式も、前提も、一次証拠で確かめ直す必要があった**——実際に確かめると前提が 2 つひっくり返った。

- **`.lsp.json` の形自体は現行仕様に実在した**（`claude.exe` 2.1.232 の zod スキーマ）。ただし公式 `rust-analyzer-lsp` は `.lsp.json` を持たず、宣言は marketplace エントリ側の `lspServers` に在る。
- **`rust-analyzer.toml` は Claude Code の RA に届いていない**（独立な 2 キーの不発で実測: flycheck は実走して `target/debug/` を使い、`workspaceSymbol("config")` は既定の `only_types` のまま）。ゆえに issue が心配した「ratoml は両クライアントに掛かる」という非対称性の問題は、**すでに逆向きに存在していた**。機序は未確定。

## 決定

`.claude/lsp/` に repo 所有の marketplace + plugin を置き、`.claude/settings.json` の `extraKnownMarketplaces`（`directory` source・**相対パス**）と `enabledPlugins` で配送する。公式 plugin は project scope で `false` にして二重宣言を防ぐ。抑制設定の検知は `.claude/hooks/lsp-config.mjs` のカナリア 1 枚に置く。

分担の正本は `docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」。

## 検討した代替案と却下理由

- **`claude plugin marketplace add <path> --scope project` で配線する**: 却下。CLI は入力を `path.resolve()` して**絶対パスで書き込む**（バイナリから逐語抽出）。絶対パスは他マシンと agent worktree で壊れる。読み手側も `path.resolve()`（cwd 基準）なので、`.claude/settings.json` へ**相対パスを手書きする**ほうが可搬性が高い。
- **inline `settings` marketplace source を使う**（`{"source":"settings", "plugins":[…]}`）: 却下。パスを一切持たないので魅力的に見えるが、合成 marketplace の root が**キャッシュ配下**（`path.join(<cache>, name)`）に書かれるため、plugin entry の相対 `source` はリポジトリではなくキャッシュを指す。plugin entry の source union に `directory` も絶対パス形も無い（`npm` / `archive` / `github` / `git` / `command` と marketplace root 相対のパスのみ）。**リポジトリ内の plugin を指せない。**
- **user ratoml（`%APPDATA%/rust-analyzer/rust-analyzer.toml`）でクライアント間の非対称を作る**: 却下。全プロジェクトに掛かる／repo 外所有で機械検査ができない／`checkOnSave`・`diagnostics.*` は workspace・local 水準ゆえ**クライアント設定より下**で、plugin を入れた瞬間に無効化される二重機構になる。
- **`command` を stdio proxy へ向け、`initialize` の params へ設定を書き足す shim**: 却下。スキーマが `initializationOptions` を正規に持つ以上、**写しを自作する理由が無い**。
- **`--plugin-dir` を常用運用にする**: 却下（issue の判断を追認）。起動手段に依存し、clone 後の再現性が無い。
- **`claude plugin validate --strict` をマニフェスト以外の検査にも使う**: 却下。**この検証器は `.lsp.json` を視界に入れない**——JSON として壊しても抑制キーを消しても `✔ Validation passed` / exit 0 を返す（スクラッチ plugin へ 3 パターンの変異を注入して実測。呼び出し側とレビュア側の 2 者が独立に再現）。マニフェスト 2 枚の妥当性はこの検証器に委ね、`.lsp.json` の意味的整合は自前カナリアが持つ、と層を分けた。
- **検査 id を新設する（`lsp-validate` 等）**: 却下。`BUDGETS` + `buildCommand` の `case` + `repro` + `docs/hooks.md` の新行 + `REPRESENTATIVE_EDITS` の 5 点セットが同時に要るのに、得られるのはマニフェスト検証だけで、それは `claude plugin validate` が既に持つ。`hook-selftest` へ相乗りする形が最小。
- **検査層を 2 枚にする（`governance-check.mjs` へ `G-lsp-config` を足す）**: 却下。独立導出レビューの推奨だったが、(i) ファイル削除は CI の `npm test` が回すカナリアが捕まえる（カナリアは実ファイルを読むので、消えれば落ちる）、(ii) `G-stale-identifiers` への語彙供給は判定を非 test の `.mjs` へ置くことで果たされる、(iii) 残る差は `skip-ci` ラベル付き PR だけである。設定 JSON だけを触る変更に `skip-ci` を付ける動機は薄く、層を 1 枚増やす費用に見合わない。
- **`selectChecks` の ratoml 分岐を basename でアンカーし、判定は repo 直下の 1 枚だけを読む**: 却下（**レビューの修正案をそのまま採らなかった**）。それだと `snotra-core/rust-analyzer.toml` を編集したとき **hook は走るのに判定は別のファイルを見て緑**になる。割り当てられたファイルの緑は「合格」を意味するので、**沈黙より悪い**。発火と判定の母集団を揃えるため `findRatomlFiles` でツリー全体を走査する側へ倒した。
- **ratoml 走査で `readdirSync` の throw を握って続行する**: 却下。読めない枝を黙って飛ばすのは fail-open であり、throw は vitest がエラーで落ちる＝沈黙しない。**受容する残余として doc に書く**側を採った。
- **`diagnostics` の抑制を同じサイクルで入れる**: 却下（先送り・ユーザー判断）。抑制の機構は 2 層ある——Claude Code 側の `diagnostics: false`（publishDiagnostics の**注入だけ**を止め、navigation は保つ）と、RA 側の `initializationOptions.diagnostics.enable=false`（**計算しない**）。issue は後者しか想定していなかった。一方 2026-08-14 の自前実測では `<new-diagnostics>` の量は編集由来 3〜9 件に対し底値 96 件で、**設定で消せるのは実セッションに出ていない底値のほう**であり、しかも `unlinked-file`（`.rs` を作って `mod` 忘れ）は cargo から見えないので失うと代替検知が無い。**先に実効設定を実測してから決める。**

## 受容する残余

**足ごとに名指しする。**

1. **`known_marketplaces.json` はマシン全体で marketplace 名をキーに持つ。** 宣言した相対パスが存在しないツリー（この変更より前の枝から作った worktree）では、reconciler が debug log に `keeping materialized entry` と書いて**別のツリーの plugin を使い続ける**。カナリアはそのツリーのファイルを読むので、この乖離は検知できない。検知するにはユーザーマシンの状態（CI に無い）を読む必要があり、かつファイル作成から次のセッション再起動までの**正当な過渡状態で赤くなる**。実挙動を測ってから決める。
2. **`.claude/settings.local.json` は project より優先順位が高い**（user < project < local < flag < policy）。そこへ `enabledPlugins` を書けば plugin を無効化できるが、gitignore 済みでリポジトリからは守れない。
3. **`skip-ci` ラベル付き PR ではカナリアが走らない。** 層を 1 枚に留めた代償。
4. **除外は名前一致・全階層である。** `crates/dist/` のような名前のディレクトリができれば ratoml を取りこぼす（現存しない）。
5. **`toPosixPath` の使用は検知器で縛れない。** Windows では `path.sep` が `\` ゆえ `replaceAll("\\","/")` と同値で、POSIX でも差が出るのはファイル名にバックスラッシュを含む場合だけである。読解で担保する。

## 検知器自身のカバー範囲を検算して見つかった足

**「不変条件を壊す」向きの変異だけでは足りなかった。** 各検査を `if (false)` で無力化して「どの変異も落ちなくなる検査」を探す向き（＝検査を殺す変異）を外部レビューが回したところ、素通りが 14 中 3 件見つかった。とくに `extraKnownMarketplaces` の検査は、`path` を壊す変異が**下流の別メッセージ**（`marketplace.json を読めない`）で赤くしていたため、**誰にも縛られていなかった**——変異を足す側からは原理的に見えない死角である。

素通りは 3 → 1 → 0 へ収束した。途中で 1 件、**修正そのものが新しい欠陥を作った**: ratoml 検査を配送経路より前へ移したことで、`settings.json` の読み取り失敗が返す `return [新しい配列]` が初めて `violations` を捨てる意味になった（**当該行は 1 文字も変えていない**）。差分レビューでは変わった行が無いので原理的に見つけにくい。
