# TODO

未解決の既知問題。解決したら削除する。

## M5: App.tsx の listen / onCleanup 非対称

**場所**: `ui/src/App.tsx` 複数箇所
**影響**: HMR（ホットリロード）のみ。本番ビルドでは `App.tsx` がアンマウントされないため実害なし
**原因**: `onMount` が `async` であるため、`listen()` の呼び出し前に同期リアクティブコンテキストで `onCleanup` を登録できない
**修正方針**: cleanup 関数を収集する配列を `onMount` の同期部分で宣言し、`onCleanup` で一括解除する構造に変更する
**対応タイミング**: `App.tsx` を大きく変更する際に同時対応するのが合理的
