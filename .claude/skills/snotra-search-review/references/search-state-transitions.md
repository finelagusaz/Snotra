# Search State Transitions

## 目的

検索変更を文字列比較ではなく状態遷移として見る。
レビューでは「前回状態 -> 今回状態」で候補集合が縮むか広がるかを判定する。

## 状態軸

最低限、次の軸を追う。

- query
- active mode
- kana query の有無と値
- migemo の有無と値
- indexing 中かどうか
- slash command / instant command の解釈結果

## 主要遷移

### 1. 通常の逐次入力

- `d -> do`
- `do -> dok`
- `dok -> doky`

見る点:
- 文字数閾値を跨いでいないか
- kana / migemo 述語が今回初めて有効にならないか
- 前回候補の再利用で専用ヒットを落とさないか

### 2. バックスペース

- `dokyu -> dok`
- `do -> d`

見る点:
- 候補集合が広がるのが普通なので full scan が必要ではないか
- キャッシュが「絞り込み専用」なのに逆方向で使われていないか

### 3. モード切替

- 通常検索 -> slash command
- 通常検索 -> instant command
- slash command -> 通常検索
- instant command -> 通常検索

見る点:
- 別モードの候補や selection state を誤再利用していないか
- parser 結果が変わったら fresh path に落ちるか

### 4. 設定変更を伴う遷移

- `migemo_min_chars` 変更
- instant prefix 変更
- 言語や extensions 設定変更

見る点:
- 既存キャッシュを無効化しているか
- watcher 反映後に古い解釈で検索しないか

### 5. ライフサイクル遷移

- window shown
- input clear
- Escape
- indexing complete

見る点:
- 表示状態のリセットと検索キャッシュのリセットが揃っているか
- 一方だけ初期化して stale state を残していないか

## レビュー用マトリクス

各変更について次を埋める。

```text
前回状態:
- mode:
- query:
- kana:
- migemo:

今回状態:
- mode:
- query:
- kana:
- migemo:

候補集合:
- 狭まる / 広がる / 不明

結論:
- incremental safe / full scan required
```

## 壊れた不変条件の書き方

所見では次の形を使う。

`検索対象集合を広げうる遷移では、前回候補を母集団として再利用してはいけない。`
