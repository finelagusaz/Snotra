# Search Test Heuristics

## 目的

検索レビューで指摘した不変条件を、そのままテストに落とすための観点をまとめる。

## 最低限ほしいテスト

### 1. 逐次入力での正しさ

- 1 文字ずつ入力した結果が、毎回 fresh scan と一致する
- kana 専用ヒット、ASCII 専用ヒット、両方ヒットを分けて fixture を作る

例:
- `d -> do` で `ドキュメント` が欠落しない
- `dok -> dokyu` で今回初めて kana 化できる項目が出る

### 2. 閾値跨ぎ

- `migemo_min_chars - 1 -> migemo_min_chars`
- `migemo_min_chars -> migemo_min_chars - 1`

見る点:
- 閾値前後で incremental と fresh scan の結果差分がないか

### 3. モード切替

- 通常検索 -> slash command
- 通常検索 -> instant command
- モード解除で通常検索へ戻る

見る点:
- 前モードの候補や選択位置を誤再利用しないか

### 4. キャッシュ更新

- 空結果でも `prev_*` が更新される
- early return パス後も次回検索が壊れない
- 設定変更後の初回検索で stale cache を使わない

## テストの書き方

- 逐次入力列を 1 ケースとして持つ
- 各ステップで `incremental result == fresh result` を比較する
- 期待値固定だけでなく「毎回 full scan と一致する」性質テストに寄せる

## fixture の作り方

- ASCII だけでヒットする項目
- kana / migemo だけでヒットする項目
- どちらでもヒットする項目
- prefix を伸ばすと初めて変換可能になる項目

## 指摘文との対応

レビューでこう指摘したら、テストも同じ構造にする。

- 壊れた不変条件
- 具体的な入力遷移
- 欠落する項目
- fresh scan なら出る理由
