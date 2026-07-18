# Retrospective — WebView2 150 High IL E2E 復旧 (#555)

## よかったこと

### 上流 Issue の最終回答と実行コードを照合し、原因を訂正できた

#555 と WebView2Feedback#5640 の初期説明は「High IL で DevTools の loopback socket が壊れる」という仮説だったが、Microsoft の最終回答は異なっていた。WebView2 150 は昇格ホストで、ユーザーが書き換え可能な `WEBVIEW2_*` 環境変数と HKCU policy の browser arguments を意図的に無視する。一方、アプリ API の `CoreWebView2EnvironmentOptions.AdditionalBrowserArguments` は有効である。Tauri から Wry、WebView2 API まで値の到達経路を実装で追い、旧回避策が msedgedriver capability にしか届いていなかったことを確認したため、de-elevation や上流待ちをせず根本原因へ直接対応できた。

### production のセキュリティ境界を構造で維持した

E2E 専用 Cargo feature とセッション環境変数の両方が揃ったときだけ trusted app API へ `--remote-debugging-port=0` と隔離 data directory を設定する構造にした。通常ビルドではユーザーが書き換え可能な環境変数から browser argument を有効化できない。さらに data directory 名を単一の安全な相対 component に制限し、driver とアプリが同一ディレクトリを使い、正常・異常終了の両方で削除する不変条件をテストと E2E ハーネスで固定した。

### High IL の真の実行環境で復旧を証明した

ローカルの E2E 16 件を 2 回通したうえで、GitHub-hosted Windows runner の `E2E & Smoke` workflow を手動実行した。startup smoke 5/5 と Playwright E2E 16/16 が成功し、従来の `DevToolsActivePort file doesn't exist` が High IL 環境で解消したことを確認できた。ローカルの sandbox 起因 `EPERM` は権限付き再実行で環境要因と切り分け、コード回帰として扱わなかった。

## 伸びしろ

### Issue 本文の「根本原因」を一度は確定情報として扱いかけた

Issue 本文や上流 Issue の冒頭説明は、その後の maintainer コメントで訂正されうる。今回は実装前に最終コメントまで読み直して是正できたが、初期段階では trusted-origin / loopback 仮説を前提に調査していた。根本原因の証拠は Issue の要約ではなく、上流の最終回答、依存ライブラリの実装、失敗・成功する実行経路の三点で確定する必要がある。この教訓は既存の「issue 前提のコード裏取り」と一致し、今回固有の High IL E2E 境界は `src-tauri/CLAUDE.md` に配置した。

### driver 側設定とアプリ側設定を同じ概念として見ない精度が必要だった

`tauri:options.webviewOptions.additionalBrowserArguments` という名前だけを見ると、WebView2 アプリ生成時の AdditionalBrowserArguments と同じ経路に見える。しかし実際は前者が msedgedriver capability、後者が Wry 経由のアプリ API であり、WebView2 150 のセキュリティ境界では結果が分かれた。層をまたぐ不具合では、設定名の類似ではなく「どのプロセスが、どの API を、どの整合性レベルで呼ぶか」まで追跡する必要がある。
