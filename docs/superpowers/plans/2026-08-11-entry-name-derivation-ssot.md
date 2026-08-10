# エントリ名導出規則を現状維持する実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** #997 の起票後に消えた fixture を現在形で再確認し、共通関数を作らない理由と再検討条件を `snotra-core/CLAUDE.md` に残す。

**Architecture:** production コードには触れず、エントリ名導出を各生成点の制御フローに隣接したインライン実装として維持する。判断の根拠は現在のコード検索と `tree_with` / `absent_paths` の生成・撤去履歴で再導出し、モジュール不変条件の文書へ記録する。

**Tech Stack:** Markdown / Git history / ripgrep / Node.js governance checker

**設計書:** `docs/superpowers/specs/2026-08-11-entry-name-derivation-ssot-design.md`

## Global Constraints

- 共通関数を追加しない
- Rust ファイルを変更しない
- エントリ名の既存規則、非 UTF-8・空名の除外方針を変更しない
- `SPEC.md` を変更しない（挙動変更がないため）
- `folder.rs` のフォルダ内列挙を本件へ含めない（拡張子付きの名前を使う別概念）
- 実装箇所の個数を恒久文書へ書かない。現在の母集団はコード検索へ問い合わせる
- `snotra-core/CLAUDE.md` の変更後に `npm run governance:check` を必ず実行する

---

## File Structure

| ファイル | 役割 | 変更 |
|---|---|---|
| `snotra-core/CLAUDE.md` | `snotra-core` の横断不変条件と再検討条件 | 「エントリ名の導出ルール」へ不採用理由を一段落追加 |

`snotra-core/src/indexer.rs`、`snotra-core/src/index_tree.rs`、`snotra-core/src/query.rs`、`SPEC.md` は読み取りだけに使い、変更しない。

---

## Task 1: 現在形を再検算し、共通化を見送る理由を記録する

**Files:**
- Modify: `snotra-core/CLAUDE.md`（「エントリ名の導出ルール」）
- Reference: `docs/superpowers/specs/2026-08-11-entry-name-derivation-ssot-design.md`
- Inspect only: `snotra-core/src/indexer.rs`
- Inspect only: `snotra-core/src/index_tree.rs`
- Inspect only: `snotra-core/src/query.rs`
- Inspect only: `snotra-core/src/folder.rs`

**Interfaces:**
- Consumes: `e72276a` が追加し `0bb7b11` が削除した `index_tree.rs::tree_with` / `IndexTree::absent_paths` の履歴、および現在の `indexer.rs` にある `Path::file_name()` / `Path::file_stem()` の呼び出し
- Produces: `snotra-core/CLAUDE.md` の不採用理由と再検討条件。production API・型・挙動は何も生成しない

- [ ] **Step 1: 実装前に現在の母集団と撤去履歴を再確認する**

Run:

```powershell
rg -n -C 4 "file_name\(\)|file_stem\(\)" --glob '*.rs' .
```

Expected: `indexer.rs` では通常スキャンの folder/file と PATH スキャンに対象の導出があり、`temp_dir_name_contains_process_id` の `file_name()` はテスト用ディレクトリ名の検査なので対象外。その他のヒットは周囲の関数と代入先を読み、`indexer` のスキャンが作る `AppEntry.name` へ代入していないことを確認する。`query.rs::lower_file_name` と `folder.rs::read_dir_entries` は設計書 §2 に記した別概念である。

Run:

```powershell
git log --all -S 'fn tree_with' --oneline -- snotra-core/src/index_tree.rs
git log --all -S 'absent_paths' --oneline -- snotra-core/src/index_tree.rs
```

Expected: どちらにも追加側 `e72276a` と削除側 `0bb7b11` が出る。

Run:

```powershell
rg -n "tree_with|absent_paths" snotra-core/src/index_tree.rs
```

Expected: 該当なしで exit code 1。これは「現在のファイルに存在しない」という期待結果であり、検査失敗として扱わない。

上のどれかが期待と異なる場合は、`snotra-core/CLAUDE.md` を編集せず停止する。現在形が設計承認時から変わったため、共通化の採否を再判断する必要がある。

- [ ] **Step 2: 「エントリ名の導出ルール」へ不採用理由を追加する**

`snotra-core/CLAUDE.md` の「エントリ名の導出ルール」で、既存の file/folder 規則と `folder.rs` の意図的な差の後ろへ次の段落を追加する。

```markdown
**エントリ名導出は共通関数へ括り出さない**（issue #997）: #995 で生じた `index_tree.rs` の fixture は `0bb7b11` で利用対象と一緒に撤去済み。現行の導出は `indexer.rs` 内で拡張子照合・空名除外・再帰継続・重複排除の各処理に隣接し、共通関数はそれらを束ねないためインラインを維持する。別モジュールに実行可能な消費者が再び生じたら再検討する。
```

既存の file/folder 規則と `folder.rs` の説明は変更しない。`0bb7b11` は現在形を作った履歴の識別子であり、実装箇所の個数は書かない。

- [ ] **Step 3: 差分が判断記録だけであることを確認する**

Run:

```powershell
git diff -- snotra-core/CLAUDE.md
git diff --check
```

Expected: `snotra-core/CLAUDE.md` に Step 2 の一段落だけが追加され、空白エラーはない。Rust ファイル、`SPEC.md`、既存のエントリ名規則には差分がない。

Run:

```powershell
git diff --name-only
```

Expected: 実装中に新しく変更したファイルは `snotra-core/CLAUDE.md` だけ。実行前から未コミットだったファイルがあれば、その所有者と内容を確認し、本タスクのコミットへ混ぜない。

- [ ] **Step 4: ガバナンス検査を実行する**

Run:

```powershell
npm run governance:check
```

Expected: `governance:check — 全検査 passed`。失敗した場合は出力が名指しする参照・識別子・面積制約だけを直し、同じコマンドを再実行する。

Rust ファイルを変更しないため、`cargo test` / `cargo check` / `cargo clippy` / `cargo doc` は本タスクでは実行しない。

- [ ] **Step 5: 判断記録を単独コミットする**

```powershell
git add snotra-core/CLAUDE.md
git diff --cached --check
git diff --cached --stat
git commit -m "docs(core): エントリ名導出の共通化を見送る (#997)"
```

Expected: `snotra-core/CLAUDE.md` だけを含むコミットが作成される。設計書・計画書が未コミットなら別コミットとして扱い、この判断記録へ混ぜない。

- [ ] **Step 6: コミット後の状態を確認する**

Run:

```powershell
git status --short --branch
git show --stat --oneline HEAD
```

Expected: HEAD は `docs(core): エントリ名導出の共通化を見送る (#997)`。作業ツリーには本タスクの未コミット差分が残っていない。

---

## 検証の射程

`npm run governance:check` が検査するのは参照・識別子・文書構造の決定的な整合であり、「共通化しない」という判断の妥当性そのものではない。判断の根拠は Task 1 / Step 1 の現在コードと Git 履歴の照合である。

将来、別モジュールに実行可能な消費者が生じた場合は、この文書の再検討条件に到達する。その変更では production と新しい消費者を同じ関数へ集約し、規則そのものを独立した単体テストで固定する。
