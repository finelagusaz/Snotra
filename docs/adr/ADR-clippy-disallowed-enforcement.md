# ADR-clippy-disallowed-enforcement: 禁止の実効はレベルを構造へ移して守り、内容は静的検査で見張る

#950。#900 が新設した `src-tauri/clippy.toml` は、**自分が死んだことを誰にも告げられない**設定ファイルだった。

## 文脈

`disallowed-methods` が空洞化する経路は 6 本あり、すべて clippy が exit 0 のまま沈黙する（起票者が 2026-08-06 に実測）。ファイルの削除・空配列化・エントリ 1 行の消失は完全な沈黙、メソッド名の書き損じは warning を出すが `-D warnings` でも exit 0、crate 名の書き損じと `egui` 依存の消滅は診断そのものが出ない。**PostToolUse hook は exit code でしか検出しないため、warning はエージェントにも届かない**——沈黙は二重である。

実装レビューの過程で 7 本目が判明した（**沈黙経路 0**）: `disallowed_methods` は warn 既定であり、禁止が赤くなるのは `ci.yml` と `.claude/hooks/post-edit.mjs` が `-D warnings` を渡している間だけだった。**これは `clippy.toml` のテキストが何も変わらないので、内容側の静的検査では原理的に捕まらない。**

実装レビューではさらに 8 本目が出た（**deny の打ち消し**）: `[workspace.lints.clippy]` へ `all = "allow"` を 1 行足すと、`disallowed_methods = "deny"` の行を残したまま禁止が完全に消える（clippy 1.94.0 で実測: exit 0・診断 0 件）。**レベルを構造へ移す決定が、その構造自身に新しい沈黙経路を作った**——ゆえに移した先も検査の対象に含める。

この repo には逆を向く 2 つの先例がある。#706 → `ca8afae`（G-workspace-lints）は「沈黙する経路にはカナリアを置く」を選び、#894 → `b2ff79c` は「捕捉実績 0 の検査は保守費で負ける」として検査を削除した。

## 決定

**レベルと内容を別の手段で守る。**

1. **レベル**: ルート `Cargo.toml` へ `[workspace.lints.clippy] disallowed_methods = "deny"` を置き、`-D warnings` への依存そのものを消す。
2. **内容**: `governance:check` へ `G-clippy-disallowed` を新設し、`clippy.toml` の禁止集合（カナリア 7 件の在否）・`src-tauri` の `egui` 依存・**1 で置いた deny が実在し、かつ同じ節の群 `allow` に打ち消されていないこと**を静的に照合する。

先例の食い違いは「捕捉実績」ではなく「**最終防衛線の有無**」で裁いた。`.githooks/` の取りこぼしは GitHub ruleset が捕まえるが、`clippy.toml` にはその層が無い——ゆえに #706 側へ寄せた。

## 検討した代替案と却下理由

- **`ci.yml` と `post-edit.mjs` の clippy 起動行が `-D warnings` を含むことを検査する（issue コメントの選択肢 1）**: 却下。守れているのは `clippy.toml` ではなく**起動形**であり、`G-hook-commands` と同じくツール側の判定を文書へ写す形になる。レベルを workspace lints へ移せば依存そのものが消えるので、この検査は**構造で消えた問題の見張り**になる。「写しを増やす案より、写しが要らなくなる案を採る」。
- **記録に留める（同・選択肢 3）**: 却下。#900 は既に `clippy.toml` 冒頭へ沈黙経路を書き込んであり、規範としては完備している。しかしこの設定が守るのは #751 の視覚バグ——**症状が「入力欄だけが旧色で残る」という、テストが赤くならない種類の欠陥**であり、規範が読まれなかったときの回復経路が無い。`.githooks/` に相当する最終防衛線が存在しない点が #894 の状況と決定的に違う。
- **deny 化だけを行い、静的検査は置かない**: 却下（実測で否定）。禁止集合が空なら deny にしても禁止するものが無い。`clippy.toml` を持たない 3 crate へ deny が無害であることの実測（`-D warnings` 無しで exit 0・診断なし）が、そのまま「空の禁止集合は何も落とさない」ことの実測でもある。
- **禁止集合そのものを `[workspace.lints.clippy]` へ移す**: 却下。`disallowed-methods` は lint のレベルではなく**設定値**であり、`clippy.toml`（または `CLIPPY_CONF_DIR`）が唯一の口である。仮に置けても workspace lints は全 member 共通ゆえ `snotra-settings` の正当な使用を巻き込む——crate ごとに分けたいという要件そのものが workspace lints と反対を向いている。**移せるのはレベルだけである。**
- **`G-workspace-lints` を拡張して clippy カテゴリも見る**: 却下。あちらの母集団は「ルート + 全 member の `Cargo.toml`」で、こちらは「`src-tauri` の `clippy.toml` + `src-tauri/Cargo.toml` + ルート」である。1 検査に 2 母集団を持たせると finding が**どちらの母集団が欠けたか**を言えなくなる（`ADR-hook-fires-table-check` で同じ理由により別検査を選んでいる）。member 側の opt-in だけは重複させず、あちらに委ねた。
- **`G-references` の実在検査に委ねる**: 却下。守るのはファイルの実在までで、内容は見ない。しかもそれは `src-tauri/CLAUDE.md` が `` `src-tauri/clippy.toml` `` とルート相対形で書いていることに依存する**文言依存の結合**であり、`` `clippy.toml` `` と縮めた瞬間に黙って消える。
- **既存の `tomlLine` を再利用する**: 却下（実装前の実測で否定）。`raw.replace(/#.*$/, "")` は引用符の中を見ないため、`reason` に含まれる `（#751）` で実データの行が切れる。いま `path` が生き残るのは `reason` より前に書かれているという**順序に依存した偶然**であり、順序を入れ替えた形で崩れることを測った。引用符を意識した除去を別に持つ。
- **`path` の抽出を per-line の単発 `match` で行う**: 却下（同上）。1 行形の配列で先頭 1 件しか拾わず、さらに**コメント除去を通さない実装は `#` でコメントアウトされたエントリを「在る」と数える**。後者はこのファイルの様式（コメントで長く説明する）では最も自然な一時無効化の形であり、**issue が塞ごうとしている空洞化そのもの**だった。全域 match + 引用符を意識した除去を採った。

## 受容する残余

**足ごとに名指しする**（`.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」）。

1. **cargo の fingerprint に `clippy.toml` は入らない。** ここだけを変えて同じコマンドを打つとキャッシュ replay で設定を適用せず exit 0 を返す。静的検査はテキストを見るので**この足には無関係**であり、依然として `.rs` を触るか `cargo clean -p snotra` を挟む必要がある。
2. **カナリアが見るのは既知 7 件の在否だけであって、それが解決することは見ない。** 8 件目として足したパスの書き損じは、`does not refer to a reachable function` の warning が exit 0 で流れる元の沈黙のままである。**既知 7 件も同じ穴を持つ**——上流 egui のピンを動かして API が消えても、`clippy.toml` のテキストは 1 文字も変わらないので検査は緑を返す。`clippy.toml` 冒頭の「違反を注入して赤くなることを測れ」という規範が、どちらに対しても唯一の防御である。
3. **打ち消しうる lint group の名指しは、上流の群構成に追随している間だけ正しい。** `disallowed_methods` を含む群は `clippy::all` と `clippy::style` の 2 つで（`clippy-driver -W help` で数え上げ）、検査はこの 2 つの `allow` だけを打ち消しと見る。上流が 3 つ目の群へ入れたら、配列が更新されるまで沈黙する。**群を ∀ で塞がない**（隣の `rustdocLintsAreDenied` はそうしている）のは、`[workspace.lints.clippy]` が唯一の設定面だからである——member 側の `[lints]` での局所上書きは cargo が `cannot override 'workspace.lints' in 'lints'` で拒むため、正当な `allow` もここにしか書けない。∀ にすると、その正当な用途が検査を緩める圧力に変わる。
4. **`reason` 文言の変更と `#[allow]` による迂回は射程外である**（lint に内在する性質）。
5. **`disallowed_methods` 以外の clippy lint のレベルは、どの検査も見ていない。** 1 で置いた deny は `G-clippy-disallowed` が見張るが、他の clippy lint を `[workspace.lints.clippy]` へ足しても降格を捕まえる機構は無い。
6. **deny が実効するのは member 側の opt-in が在る間だけである。** `src-tauri` の `[lints] workspace = true` は `G-workspace-lints` が全 member について見ており、両検査は**組で 1 つの命題を守る**。片方だけを消す変更は、もう片方の緑に隠れる。
7. **`disallowed_methods` をハイフンで書いた形は非実効と判定する（赤に倒れる）。** 向きが赤なので受容するが、次の人の最も安い直し方が「検査を緩める」にならないよう、直し方をソースのコメントへ書いた。
