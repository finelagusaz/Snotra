# research: issue #589 検証写像の照合・事故散文の圧縮・superpowers 非規範化

## issue の要約と方針決定

3 部構成。第 1 部「検証写像の共通定義」は、ユーザーへの確認で**「共通定義フル導入」ではなく「照合を 1 本足す」（G9）に決定**（#593 の機構最小化と整合。ci.yml は静的 YAML ゆえデータ駆動化は生成器という新ドリフト面を作る・hook の fail-closed 骨格に触れない）。

## 第 1 部: G9（hook ↔ build-commands の機械照合）

- 現状の未照合辺は「hook の `buildCommand` ↔ `docs/build-commands.md`」のみ（build-commands ↔ package.json は G5、↔ workflows は G6 で照合済み・#587）
- `post-edit.mjs` の `buildCommand`（L256-299）は **非 export**。モジュール import は末尾の main 実行（stdin 読取）が走る危険があるため不可。**hook は触らない**（ユーザー決定の選択肢に明記）
- → G9 は post-edit.mjs の**ソーステキスト**から `cargoSpec([...])` の case 節を正規表現で抽出し、出力整形フラグの許容リスト（`--message-format short` — exit code を変えない・build-commands L26 の既存規約）を除去した上で、カテゴリ A コードブロックに同一コマンドが在ることを照合する
- 対象は **cargo 系のみ**（#476 の事故クラス = フラグドリフトはここ）。vitest/tsc 系は「npm SSOT の部分集合ラッパー許容」という意味判断が要るため Check 5 残置のまま
- 抽出 0 件は明示 fail（沈黙経路の閉塞。リファクタで抽出が壊れたら loud に落ちる）
- 現状の hook cargo コマンド 5 件: clippy（--workspace --all-targets --message-format short -- -D warnings）/ test -p snotra-core / test -p snotra-settings / test -p snotra / check --workspace。正規化後はいずれもカテゴリ A ブロックと一致するはず（dogfood で実証する）
- 追従: health-check SKILL.md の Check 5 残置記述（cargo フラグ照合を G9 へ移す）・build-commands L26 の「フラグ照合は /health-check」

## 第 2 部: 事故散文の圧縮（ルート CLAUDE.md）

- 対象: 主に「Git/GitHub 運用」節（マージ auto-close の実測ナラティブ）と「フック」節（#482/#488 の論証エッセイ）。方針は合意済みの「圧縮して同居」= 太字指示 + 理由 1〜2 文 + issue 番号
- **残すもの**: 太字の指示・マージ手順 1〜4（現役の運用手順であり事故散文ではない）・「なぜこの規則があるか」の一文・issue 番号
- **退去するもの（手順ログ級）**: 実測の時系列（「PR #491 を --body-file 付きでマージ → 1 秒後に close、commit_id は null」等）・多段の論証エッセイ（A2 の 3 理由の詳細展開等）・設定コマンドの復元手順のような逆引きレシピ
- 退去先: issue #589 のコメント（#588 の退避パターンと同じ。「行き先を明示」の要件を満たす）
- **governance-docs.md の義務が発火する**（CLAUDE.md 構造改変）: 参照側を名前と序数の両方で数え上げる。リスク最小化のため**節見出し・太字規則の文言は一切変えず、後続散文だけを圧縮する**（見出し・引用語句への参照は自動的に無傷）。半角 Step / 全角ステップ・引用見出し語句の grep を実装時に実施し、圧縮で消す文が他所から引用されていないか確認する
- burn-down 計測: ルート CLAUDE.md のバイト数を前後で記録（#593 指標。現状を実装時に実測）

## 第 3 部: docs/superpowers 非規範化 README

- `docs/superpowers/README.md` を新規作成: 「歴史資料・現在の仕様ではない・鮮度維持対象外」宣言 + 現在の正準への誘導（SPEC.md・docs/architecture.md・設計書 specs/）
- governance:check の G3 母集団は `docs/superpowers/` を除外済みのため干渉なし。README 自身も検査対象外（意図どおり）

## 技術的制約

- governance-check.mjs / そのテストの編集 → safety-nets.md 配送・vitest（scripts include）・カテゴリ F。ルート CLAUDE.md 編集 → governance-docs.md 配送・検査割当なし（governance:check を手動実行）
- health-check SKILL.md 編集 = スキル編集（エージェント設定）。issue 本文スコープ内 + ユーザーの着手指示で合意済みだが PR レビューが最終確認点
- PR は #589 を **Closes してよい**（3 部とも PR 内で完結する。#588 と違い試行期間を持たない）

## 未解決の疑問

なし（第 1 部の形式はユーザー確認済み。第 2 部の残す/退去の線引きは上記で確定）。
