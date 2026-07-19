# plan.md — issue #384 第三者 GitHub Actions を Node 24 ネイティブへ更新

## ゴール

`.github/workflows/` の第三者 JS アクション 2 種を Node 24 ネイティブへ移行する。挙動は現状維持（機能変更なし・純粋な CI 保守）。

## 変更ファイル一覧

### 1. `.github/workflows/release.yml`（1 行）
- L121: `uses: softprops/action-gh-release@v2` → `@v3`
- with ブロックは変更なし（`tag_name` / `name` / `draft: true` / `prerelease` / `files`。v3 で全入力存置・削除なし）。
- draft 維持コメント（L125-128）はそのまま。

### 2. `.github/workflows/create-release.yml`（1 行）
- L50: `uses: softprops/action-gh-release@v2` → `@v3`
- with ブロックは変更なし（`tag_name` / `name` / `draft: true` / `prerelease` / `generate_release_notes` / `body`）。

### 3. `.github/workflows/label-sync.yml`（Sync labels ステップの置換）
現行:
```yaml
- name: Sync labels
  uses: EndBug/label-sync@v2
  with:
    config-file: .github/labels.yml
    delete-other-labels: true
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```
移行後:
```yaml
- name: Sync labels
  uses: crazy-max/ghaction-github-labeler@v6
  with:
    yaml-file: .github/labels.yml
    # skip-delete を指定しない = default(false) で SSOT 外ラベルを削除。
    # EndBug の delete-other-labels: true と同一挙動（labels.yml が唯一の真実源）。
    github-token: ${{ secrets.GITHUB_TOKEN }}
```
- `config-file` → `yaml-file`。
- `delete-other-labels: true` → 削除は crazy-max の default 挙動ゆえ**指定を削除**（コメントで意図を明示）。
- `env: GITHUB_TOKEN` → `github-token:` 入力。default は `${{ github.token }}` で省略可だが、意図明示のため明示指定を維持。
- SSOT 削除の警告コメント（labels.yml 側 L25-26）は labels.yml に残るため変更不要。

### 4. `.github/labels.yml`（変更なし）
- color は `#` なしのまま crazy-max が受容（自 repo labels.yml も `#` なし・PR #207 でサニタイズ）。書き換え不要。
- dependabot 3 ラベルの明示保護も crazy-max の default 削除下で引き続き有効。

## 実装順序（フェーズ分け）

依存関係なし・独立 3 変更。1 コミットで実施可。
1. release.yml / create-release.yml の `@v2`→`@v3`（低リスク・ドロップイン）
2. label-sync.yml の crazy-max 移行
3. 検証（下記）

## 不変条件

- **INV-1（release critical path）**: `release.yml` の Upload は `draft: true` を保ち、`files` glob（`.zip` / `*-setup.exe` / `latest.json`）が全成果物を添付する。**署名 `.sig` の扱いに注意**: `.sig` ファイル自体は release asset ではない。L95 で `.sig` の内容を読み取り → latest.json の `signature` に埋め込み（L103）→ latest.json を `files:` で添付（L134）。この生成ステップ（L91-108）と `files` 一覧は v2→v3 で不変。v3 は入力不変ゆえ構造は保たれる。**壊れたら署名済み成果物が即時公開 or latest.json 欠落・署名不整合 → 更新配信事故**。
- **INV-2（label 削除挙動の同一性）**: crazy-max default(skip-delete 省略) = EndBug delete-other-labels:true。labels.yml が SSOT で、そこに無いラベルは削除される。**skip-delete を誤って true にすると SSOT 外ラベルが残留し規範が崩れる。逆に labels.yml から誤って行を消すと本番ラベルが削除される**。
- **INV-3（保護ラベルの残存）**: dependabot が使う dependencies/javascript/rust は labels.yml に定義済みゆえ削除されない。
- **INV-4（トークン権限）**: `label-sync.yml` の `permissions: issues: write` はラベル CRUD に十分（GitHub label API は issues スコープ）。変更不要。

### 失敗・異常時の挙動
- action-gh-release@v3 が node24 で起動失敗 → workflow が fail し release が中断（サイレント公開はしない。draft:true 維持ゆえ安全側）。
- crazy-max が labels.yml をパース失敗 → ステップ fail、ラベル未変更（部分適用リスクは dry-run で事前確認）。
- 新規状態フラグ・プロセス・ウィンドウの導入は**なし**（CI 宣言的 YAML のみ）→ ライフサイクル管理の対称ペア論点は発生しない。

## テスト方針・検証

workflow YAML は PostToolUse hook 対象外（自動検査が走らない）。以下を手動で実施:

### 事前ベースライン（実装前に取得済み・2026-07-20）
- `gh label list` = **10 件、labels.yml と名前・color・description が完全一致**（live set == SSOT）。→ 正しい crazy-max 同期は**証明可能な no-op（作成 0 / 更新 0 / 削除 0）**が期待値。これにより「同期済みゆえの no-op」と「何も読まずの no-op」を後段で区別できる。default 削除が触れうる対象も「SSOT 外ラベル = 0 件」と確定。
- major タグ実在確認: `@v3` → v3.0.2（`3d0d9888`）、`@v6` → v6.0.0（`548a7c36`）。いずれも可動 major タグで最新 point release を指す。

### 検証手順
1. **YAML 構文検証**: 3 ファイルを YAML パーサで parse（`node -e` で js-yaml、または `python -c yaml.safe_load`）。actionlint があれば実行。
2. **action-gh-release v3（INV-1）**: 入力存置は action.yml @v3 で確認済みだが、**入力安定 ≠ 挙動安定**。critical path の失敗モード（署名済み成果物の早期公開・draft-reuse 経路のズレ）は `with` diff では見えない。create-release.yml が draft を作り → release.yml が同 tag へ upload する **draft-reuse 経路**は major bump で変わりうる。よって**目視レビューに加え、merge 後に throwaway test バージョンで `create-release.yml` を dispatch** し、(a) draft が 1 つだけ作られ、(b) `.sig` / `.zip` / `*-setup.exe` / `latest.json` が添付され、(c) `draft: true` が維持され即時公開されないこと、を実観測する（本番タグを汚さぬよう test バージョン→検証後に draft 削除）。
3. **label-sync 移行（INV-2/INV-3）— dry-run の機構ギャップに注意**: label-sync.yml は `dry-run` 入力を露出していない（trigger は bare `workflow_dispatch`）。ゆえに「dispatch で dry-run」は直接できない。機構を明示的に選ぶ:
   - **一次検証（採用）**: 上記ベースライン（live == SSOT）を根拠に、正しい同期 = no-op であることを差分で保証する。移行差分は「ランタイム + アクション実装」のみで labels.yml 内容は不変ゆえ、同期対象ラベル集合は変わらない。
   - **追加確認（任意・推奨）**: feature ブランチ上で一時的に `dry-run: true` をハードコードして当該 ref から `workflow_dispatch`、ログで「削除 0 / 作成 0 / 更新 0」を観測 → **merge 前に必ず revert**。恒久 dry-run 入力は YAGNI ゆえ追加しない。
4. PR CI の `governance-check` job（workflow 変更を含むガバナンス検査）が緑であること。

## SPEC.md 更新要否

**不要**。SPEC.md に記載された挙動（フロー・IPC 契約・状態遷移）に変更なし。CI 保守のみで文書化された挙動は不変。docs / rules / スキルへの言及も grep で存在せず追随不要。

## セルフレビュー

### Step 5b — plan-review が扱わない 3 観点

1. **境界条件**:
   - crazy-max の label 同期の境界: (a) SSOT に無い本番ラベル（現状 0 件・ベースラインで確認済み）→ 削除対象、(b) SSOT にあり本番に無いラベル（現状 0 件）→ 作成対象、(c) 名前一致・color/description 差異 → 更新対象。**現在は 3 種すべて 0 件（完全同期済み）**ゆえ no-op が期待値。将来 labels.yml を編集した PR で初めて非 0 になる。
   - color の境界: `#` 付き / なし双方を crazy-max が受容（PR #207 サニタイズ）。現行は `#` なし → 動作確認済みの形式。
   - action-gh-release の境界: `files` glob が 0 マッチのとき v3 も v2 同様に扱う（`fail_on_unmatched_files` は未指定＝default false ゆえ挙動不変）。
2. **シンプル化の挑戦**:
   - 新規状態（AtomicBool / Mutex / 子プロセス）・汎用インターフェースの導入は**ゼロ**。宣言的 YAML の値置換のみ。
   - `github-token:` は default `${{ github.token }}` で省略可能だが、EndBug が明示していた対称性・意図明示のため明示指定を維持（過剰ではなく可読性判断）。恒久 `dry-run` 入力は追加しない（YAGNI・検証時のみ一時使用）。
   - 「この操作が失敗したら」→ いずれも workflow step の fail として顕在化（loud）。draft:true 維持ゆえ release 側は失敗しても安全側（サイレント公開なし）。label 側は fail 時ラベル未変更。
3. **破壊不変条件 + 検知手段**:
   - **INV-1（署名済み成果物のサイレント公開防止）**: 検知 = 検証手順 2 の throwaway dispatch で `draft: true` 維持を実観測。「戻ってこない」系の代表リスク。
   - **INV-2（本番ラベルの誤削除防止）**: 検知 = 事前ベースライン（live == SSOT）+ 検証手順 3 の dry-run 観測（削除 0 件）。crazy-max の default 削除が有効化されている以上、labels.yml の 1 行欠落が本番ラベル削除に直結する。
   - **INV-3（保護ラベル残存）**: 検知 = dry-run ログで dependencies/javascript/rust が削除対象に現れないこと。
   - いずれも「テスト（自動）」ではなく「dispatch/dry-run の実観測」で担保。workflow YAML はローカル hook 対象外ゆえ、PR CI の governance-check + 手動観測が検知層。

### Step 5a — plan-review 結果

サブエージェント 2 体（設定レイヤー偵察 Explore + 独立導出 Plan）を並列起動。

#### 問題なし（一次ソースで裏取り済み）
- action-gh-release@v3 の 6〜7 入力（tag_name/name/draft/prerelease/files/generate_release_notes/body）はすべて存置・`runs.using: node24`。両偵察が action.yml で独立確認。
- `permissions: issues: write` は crazy-max のラベル CRUD に十分（現行 EndBug も同権限で過去 5 回 success の実績）。
- `skip-delete` 省略 = default 削除 ≡ EndBug `delete-other-labels: true`。同一挙動。
- color `#` なしは crazy-max で動作（`#` なしは元から動作。PR #207 は `#` 付きを**追加**許容したもの）。labels.yml 書き換え不要。
- 他 workflow から label-sync への依存なし。release.yml を呼ぶのは create-release.yml のみ（対象内）。
- docs/SPEC/CLAUDE/rules に対象アクションの言及なし → 文書追随不要。`docs/build-commands.md` の workflow 対応表は ci.yml/e2e.yml のみで対象外。
- ローカル actionlint 設定なし・governance-check は action バージョン非検証 → 「YAML はローカル hook 対象外」の記述は正確。

#### 軽微な懸念（対処済み／実装時注意）
- **INV-1 の `.sig` 記述精度**（対処済み）: `.sig` は asset 添付ではなく latest.json への内容埋め込み。research.md / plan.md INV-1 を訂正済み。
- **`github-token` は crazy-max action.yml で `required: true`**（default 併記）: 明示指定を維持する計画判断は保険として妥当と確認。→ 独立導出の「env ブロック削除で default 依存」案との差分はこれで解決（**明示 `github-token:` を採用**）。

#### 要対処 → 解決済み
- **3-1 スコープの踏み込み（設定偵察が指摘）**: issue 本文 AC は label-sync「保留可」、コメントは「検討する」で確定実施指示ではない、と偵察が正しく指摘。**ただしこれは着手前に AskUserQuestion で明示確認済み**（2026-07-20・ユーザーが「crazy-max へ移行」を選択）。偵察はこの確認を視界に持たなかった。→ **実施は明示承認済みで解決**。この一段の踏み込みは正当。

### Step 2b — 独立導出との差分

- **漏れ（導出 ∖ plan）→ 反映**:
  - **codex:* ラベルの既存削除リスク（報告のみ・修正しない）**: `.github/codex-automation.md` が `codex:*` ラベル作成を指示するが labels.yml（SSOT）に無い。EndBug でも crazy-max でも同期時に削除される**移行前からの同一挙動**。`exclude` で救うのは out-of-scope の挙動変更ゆえ**本 issue では触れない**。現状ベースラインに codex:* ラベルは存在せず（10 件のみ）、移行による差分ゼロ。→ 報告事項として記録。
  - **README バッジのファイル名制約**: `README.md:17`/`README.en.md:17` が `release.yml` をファイル名参照。→ **release.yml をリネームしない**（今回もしない）。制約として記録。
  - dependabot は github-actions エコシステム未設定（cargo/npm のみ）→ これらの action は手動保守（この issue が必要な理由の裏付け・out-of-scope）。
- **スコープ過剰（plan ∖ 導出）**: なし。plan の `github-token:` 明示は過剰でなく `required: true` に対する保険（上記で解決）。
- **一致（完全性の能動的証拠）**: release.yml/create-release.yml の `@v3` 化、label-sync の yaml-file/default 削除/token、labels.yml 不変、color `#` なし可、node24 確認、検証方針（feature ブランチで一時 dry-run→revert・release は throwaway dispatch）——**主要判断がすべて独立に再一致**。加えて独立導出は `rust-cache@v2`=node24 / `dtolnay/rust-toolchain`=composite を確認し**残る node20 JS action は named 2 つのみ＝スコープ不足なし**を裏取り。

### 総評
- 計画の completeness: **高**（主要判断が独立再一致・スコープ完全性を裏取り・critical path の挙動を一次ソースで確認）
- 実装着手可否: **可**（要対処 3-1 はユーザー承認で解決済み・懸念はすべて対処済み or 記録済み）
