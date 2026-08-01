# ADR-rustfmt-gate: cargo fmt を PostToolUse hook と CI のゲートにし、rustfmt.toml を置かない

#858。整形を人の習慣ではなく機構に持たせる。

## 文脈

`cargo fmt --all -- --check` が clean な `main` で 460 ハンクの差分を報告する状態が続いていた。CI にも `docs/build-commands.md` にも fmt が無かったため、「ゲートである」とも「ゲートでない」とも記録されておらず、Rust を触った者が個別にこの赤へ出くわす。PR #857 で実際に起き、そのPRが触っていないファイルの差分にもかかわらず本文へ「不合格」の注記が残った。

**drift の由来は「未整形」ではなく「習慣の途切れ」である。** 同一の rustfmt（`1.8.0-stable`）で過去コミットを検査した:

| コミット | 日付 | ハンク数 |
|---|---|---|
| `5589c13` init | 2026-02-16 | **0** |
| `92804be` | 2026-02-17 | **0** |
| `fd6cd42` | 2026-04-02 | 193 |
| `a1a95a7` | 2026-08-01 | **460** |

初期コミットが drift 0 になる。**このリポジトリの様式は最初から rustfmt 既定そのもの**であり、2026-02-17〜04-02 のどこかで `cargo fmt` を回す習慣が途切れ、以後 monotonic に積み上がった。整形は当時**別のマシンで**行われており、`rustfmt.toml` も CI ステップも要らなかったため、**リポジトリには習慣の痕跡が一切残っていなかった**（`cargo fmt` は追跡ファイルに全履歴を通じて 0 件）。

この文脈が本 ADR の決定を規定する——**問題は様式の不在ではなく、様式を保つ主体が人とマシンに載っていたことである。**

## 決定

1. `cargo fmt --all` を単独コミットで当て、`.git-blame-ignore-revs` へ載せる。
2. `ci.yml` の rust-check へ `cargo fmt --all -- --check` を、`Setup Rust toolchain`（`components: rustfmt` を明示）の直後に置く。
3. PostToolUse hook（`.claude/hooks/post-edit.mjs`）の `selectChecks` へ `fmt` を追加し、**`clippy` より前**に置く。
4. `rustfmt.toml` を置かない。`rust-toolchain.toml` も置かない（条件つき・下記）。

## 検討した代替案と却下理由

- **B: 「`cargo fmt` はこのプロジェクトの検査ではない」と明記して終える**: 却下。コストはほぼゼロで、issue #858 の選択肢としては対等に見えていた。しかし**初期コミットが drift 0 である**という実測により、B は「無かったものを導入しない」ではなく「**在ったものを捨てる**」宣言になることが判明した。捨てる理由が「習慣が途切れたから」では、規範として書けない（`AGENTS.md`「全称表現は前提条件とセットで書く」の反対側——書けない理由を書かないために選択肢を選ぶことになる）。
- **A′: 既存の様式に寄せた `rustfmt.toml` を置き、整形コミットを小さくする**: 却下。**7 通りの設定を実測し、既定が最小だった**——tuning で有意に減らせない。

  | 設定 | ハンク数 |
  |---|---|
  | **既定（設定なし）** | **460** |
  | `max_width = 110` | 459 |
  | `style_edition = "2015" / "2018" / "2021"` | 495 |
  | `use_small_heuristics = "Max"` | 527 |
  | `use_small_heuristics = "Off"` | 545 |
  | `use_small_heuristics = "Max"` + `style_edition = "2015"` | 558 |
  | 上記 + `max_width = 120` | 814 |

  設定ファイルを置いても整形コミットは縮まず、**保守すべき設定が 1 つ増えるだけ**になる。
- **edition 2021→2024 移行が drift の原因なので `style_edition` を固定する**: 却下（仮説の否定）。`fd6cd42`（edition 混在）で `style_edition = "2021"` を強制すると **202**（既定 193 より悪化）。現在も 2015 / 2018 / 2021 がいずれも 495 で、既定の 2024（460）より悪い。**移行は原因ではない。**
- **`rust-toolchain.toml` で toolchain を固定する**: **条件つき却下**。`style_edition` が rustfmt の安定性機構であり、edition 2024 から既に決まる。固定すると rustc の更新まで手動になる（現在は `dtolnay/rust-toolchain@stable` で流動）。**前提は「CI の `@stable` とローカルの rustfmt が同じ style_edition で一致すること」であり、その前提が破れればこの却下も破れる。** 判定の観測点は整形直後の初回 CI であり、そこで fmt step だけが赤くなったらこの決定を改める。
- **PostToolUse hook を「検査」ではなく「自動整形」（`cargo fmt` 書き込み）にする**: 却下。赤が構造的に消えるので魅力的だが、**既存のフックはすべて「検出して報告する」側**であり、書き込むフックは先例がない。Edit 直後にファイルが黙って変わると、エージェントが直前に読んだ内容とディスクがずれる（`docs/development-principles.md`「構造的設計原則と強制の階梯」の 6——検出は構造化信号で行い、テキストは証拠に留める——という設計から外れる）。
- **差分限定の fmt 検査（変更されたファイルだけ `--check` する）**: 却下。整形コミットを打つ前なら意味があったが、打った後は全体が整形済みなので利得が無い。加えて**`cargo fmt` が持つ「どこが対象か」の判定を、自作の差分スコープで写す**形になる——ツールが SSOT を持つ判定を手書きで再現する方向であり、`docs/development-principles.md`「KISS」に反する。
- **hook でも `cargo fmt -- <編集ファイル>` と対象を絞る**: 却下。`--all` でも 0.69s でありビルドを要さない（実測）ため絞る利得が無い。さらに `G-hook-commands` が hook の cargo コマンドと `docs/build-commands.md` カテゴリ A のトークン列一致を要求するため、**hook だけ別形にすると照合が壊れる**。
- **`cargo clippy` も `components:` へ明示する**: 却下（射程）。同じ理由で clippy も runner イメージへの暗黙依存だが（action は `--profile minimal`）、5 か月間顕在化しておらず #858 は fmt の issue である。1 語の追加で済むため **#859 で扱う**（記録された既知リスクに所有者が無い状態を残さない）。

- **整形と機構を 1 つの PR で出す**: 却下。**このリポジトリは squash 専用**（`allow_merge_commit: false` / `allow_rebase_merge: false` を実測）なので、1 PR にまとめるとブランチ全体が 1 コミットへ畳まれる。その squash コミットは CI・hook・docs を同梱するため**意味を変える変更を含み**、`.git-blame-ignore-revs` に載せられない（載せてよいのは機械的整形だけ）。一方、feature ブランチ上の整形コミットの SHA は **main の blame グラフに現れない**ので、それを記録しても no-op になる——しかも**存在しない rev を無視しても git は静かに成功する**ため、腐っても誰も気づかない（沈黙する写しが 1 つ増える）。
  → **整形だけの PR を先にマージし、その squash SHA を後続 PR の `.git-blame-ignore-revs` へ書く**。SHA は PR1 がマージされるまで観測できないため、この順序は避けられない。
  - **受容する残余**: PR1 と PR2 の間、main は「整形済みだがゲート未設置」になる。この窓で `.rs` を触る PR が入れば drift が再び入りうるので、PR2 は続けて出す。

## 受容する残余

- **`.git-blame-ignore-revs` はローカル `git blame` では自動適用されない**（実測: `blame.ignoreRevsFile` は既定で未設定）。GitHub の blame ビューは設定不要で自動適用する。ローカル側の 1 行設定は `docs/build-commands.md` に置くが、**設定するかは各人の裁量**であり機構では強制しない。
- **`docs/hooks.md` の PostToolUse 発火一覧は `selectChecks` の写しであり、内容を照合する機構が無い。** 今回 fmt の追加に伴って手で同期したが、次回も同じ手当てが要る（`G-hook-commands` が見るのは `docs/build-commands.md` だけ）。同型のドリフトはルート `CLAUDE.md` で一度起きており、その退去先が `docs/hooks.md` である。
