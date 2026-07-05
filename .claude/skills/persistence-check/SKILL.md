---
name: persistence-check
description: "シリアライズ・on-disk 形式（index.bin / config.toml / history / window.bin 等）の追加・変更時、または計画レビュー時に使用。version バンプ要否・旧形式の後方互換テスト・デコード失敗時のデータ保全を検証する。"
argument-hint: "[対象, 例: 'IndexCache: Cow 統合' / 'HistoryStore: 新フィールド追加' / 'window_data v5 マイグレーション']"
allowed-tools:
  - Read
  - Grep
  - Glob
---

$ARGUMENTS のシリアライズ／永続化ロジックについて、後方互換とデータ保全の安全性を検証する。
$ARGUMENTS が空の場合は、会話の直近の変更内容から対象を推定する。

実装後のコードレビューだけでなく、`workspace/plan.md` の計画レビューにも使える。計画段階で「この永続化変更は既存ユーザーのデータを壊さないか？」を検証し、見落としがあれば計画を更新してから実装に進む。

## 背景

このリポジトリの最頻出の高リスク領域が on-disk 永続化（`index.bin` / `config.toml` / `history.bin` / `window.bin`）で、#338・#343・#394・#461 が繰り返し噛まれてきた。SSOT は `snotra-core/CLAUDE.md`「データ永続化の注意」。このスキルはそのルールを機械的に検証する（`/cache-check` が incremental 単調性を検証するのと同型）。

**永続化バグの典型パターン**:
1. **形式変更で version 未バンプ** → 旧データを新スキーマで読んで破損 or デシリアライズ失敗
2. **セマンティクス変更で version 未バンプ** → バイト列同一でも値の解釈が変わりデータ破損（例: 絶対座標→モニター相対）
3. **後方互換テストが「新形式往復」だけ** → 旧形式が読めない false-green（新形式を書いて新形式で読むテストは、旧形式が壊れても通る）
4. **デコード失敗時の即時上書き保存** → 学習データ（history 等）を空で潰すデータ喪失
5. **`match` 兄弟分岐の保全方針の不揃い** → 1 分岐だけ直しても別分岐に破壊的フォールバックが残る（`Config::load` 型）

## Step 1 — 変更の分類

$ARGUMENTS から対象の永続化構造を特定し、ソースコードを読む。変更を分類する:

```
対象: <struct 名 / ファイル>
変更種別:
  [ ] フィールド追加/削除/型変更（バイト形式が変わる）
  [ ] セマンティクス変更（バイト形式は同一だが値の解釈が変わる）
  [ ] シリアライザ切替（bincode↔postcard 等）
  [ ] リファクタ（バイト形式不変を主張・例: owned/borrowed struct の Cow 統合）
  [ ] 読み込み失敗ハンドリングの変更（load / fallback）
```

## Step 2 — version バンプの要否判定

```
形式が変わる or セマンティクスが変わる → version バンプ必須
  - 該当 version 定数（INDEX_CACHE_VERSION / magic+version ヘッダー / 各 struct の version）をバンプしたか
  - 旧バージョン用フォールバック構造体（IndexCacheVN / Legacy variant 等）を残したか
  - load 経路に「新 → 旧」のフォールバックチェーンを追加したか
バイト形式もセマンティクスも不変（純リファクタ）→ バンプ不要
  - ただし「不変」の主張を Step 3 のテストで証明すること
```

**セマンティクス変更の見落としに注意**: バイト列フォーマットが同一でも、値の意味が変われば version バンプが要る（#338 の座標系変更が例）。「バイトが同じだからバンプ不要」は誤り。

## Step 3 — 後方互換テストの検証（最重要）

後方互換は **旧形式を新コードで読めること** で証明する。以下を確認する:

```
[ ] 旧オンディスク形式（凍結バイト列 or 旧 struct でシリアライズした bytes）を入力に、
    新コードで deserialize できるテストがあるか
[ ] そのテストは「新形式の往復」とは別に存在するか
    （新形式往復だけでは旧形式の読込失敗を検出できず false-green）
```

- **アンチパターン**: 新コードの出力を golden 化するだけ → forward-stability（今後の形式安定）しか保証せず「新出力＝旧形式」を独立に証明しない。形式が既に壊れていても新 golden がそれを凍結して素通りする（#461）
- **正しい向き**: 「旧形式の凍結バイト列 → 新コードで load 成功」を検証する。凍結バイト列は旧ビルド or リファクタ前コードから採取するのが最も厳密
- **serde 表現変更（enum variant / untagged / flatten / tag）**: 旧オンディスク形式が deserialize 失敗すると全設定リセット＝データ損失（#394）。untagged の `Legacy {..}` variant 等で必ず受理し `apply_migrations()` で移行

## Step 4 — デコード失敗時のデータ保全

```
[ ] deserialize 失敗時に空データを即時 save() で上書きしていないか（学習データ喪失・#338）
    → フォールバック読み込みを先に試み、次回の通常 save() で新形式へ昇格
[ ] 読み込み失敗を種類で扱い分けているか（Config::load 型）:
    - 不在（NotFound）= 既定値を生成・保存
    - 内容破損（parse 失敗・InvalidData）= .bak へ退避し既定値・保存しない
    - 一時的失敗（権限・ロック）= 退避も上書きもせず既定値・保存しない
[ ] 同じ match の全兄弟分岐で保全方針が揃っているか
    （1 分岐だけ直しても別分岐に破壊的フォールバックが残る・#343）
```

## Step 5 — 派生・移行経路の整合

```
[ ] IndexCache 系: CLAUDE.md「IndexCache バージョン変更チェックリスト」の全項目を満たすか
    （struct / version / fallback / load / save / CachedMasks / new_with_cached_masks）
[ ] history 系: CLAUDE.md「history.rs のキー正規化チェックリスト」の3者
    （新規記録 / 既存データ移行 / 参照 API）が揃うか
[ ] Config をデシリアライズする新経路を追加した場合、apply_migrations() の適用要否を判断したか
[ ] TOML フィールド移動時、旧フィールドを skip_serializing で残し apply_migrations() で移行したか
```

## Step 6 — 境界条件テストの確認

以下に対応するテストが存在するか grep で確認する:

| 境界条件 | テストの内容 |
|---------|-----------|
| 旧形式の読込 | 旧 version / 旧 struct の凍結バイト列を新コードで deserialize |
| 新形式の往復 | 現行 struct の serialize → deserialize が一致 |
| デコード失敗 | 破損バイト列で既定値フォールバック・実データを潰さない |
| バイト安定 | 固定 fixture の serialize が golden bytes と一致（形式ドリフト検出） |

不足していれば具体的なテストケースを提案する。

## 出力

- version バンプ要否の判定と根拠
- 後方互換テストの有無と、旧形式読込を証明できているか（forward-stability 止まりでないか）
- デコード失敗時のデータ保全の可否
- 問題があれば修正案（version バンプ / 旧形式 fallback / 凍結バイト列テスト追加 / 保全方針の統一）
- 全て安全な場合は「version 判定・後方互換・データ保全すべて満たしており、永続化変更は安全」と明示する
