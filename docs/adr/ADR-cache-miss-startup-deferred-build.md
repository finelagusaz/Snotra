# ADR-cache-miss-startup-deferred-build: `index.bin` を読めない起動は、読みだけを起動に残し、走査を背景へ回す

## 文脈

#1448 で、`index.bin` を読めない起動（版上げ・破損・止めている間の走査対象の変更）が、起動の中で全走査と保存（22〜85 秒）を終えるまでトレイもホットキーも出さないことを直した。決定（2026-11-17）は「待たせてよい。トレイとホットキーを先に出し、索引は背景で構築する」。

## 決定

`main()` には `index.bin` の読みだけを残し（外れても走査しない `load_cache_with_stats`）、外れたら空の索引で起動して、setup が `start_index_build(Rescan)` で背景の構築を起こす。起動時の索引の状態は `InitialIndex { Loaded, FirstRun, Deferred }` の 1 つの値で表す。

## 検討した代替案と却下理由

- **読みも含めて、索引のロード全体を背景へ移す**: 却下。旧版の `index.bin` の昇格は、読みがロック（`INDEX_WRITE_LOCK`）の外にあり、そのウィンドウは「昇格は `main` の起動段だけで、索引ビルドのスレッドはまだ居ない」という順序で閉じている。ロードを背景へ移すと、昇格と背景の構築が並びうる。読みを `main` に残せば、昇格は今と同じ位置で起き、cache-miss のときは昇格そのものが起きない。
- **cache-miss を first-run の起動形（`setup_first_run`）に畳む**: 却下。first-run は設定アプリを `--first-run` で開いて走査対象を確かめさせるが、cache-miss の利用者は走査対象を既に決めている。関数の名前も嘘になる。起動の計測でも、`index_load` が null でよいのは first-run だけ（`scripts/lib/SnotraStartupContract.psm1`）なので、first-run と区別して刻む必要がある。
- **構築の完了を知らせる trace を製品に足し、計測スクリプトがそれを待つ**: 却下。`scripts/measure-memory-stages.ps1` が待つべきものは「次の起動がキャッシュに当たり、記録に終わりの無い行が残らない」ことで、それは `scan-log.jsonl` の end 行がそのまま示す。計測のために製品の trace を足さずに済む。
- **`load_or_scan_with_stats` に「外れても走査しない」引数を足す**: 却下。計測ハーネス（`snotra-core/tests/`）は外れたら走査する形を使い続けるので、呼び出しのたびに意図を引数で書かせることになる。cache-hit の分岐を `try_load_with_stats_in` へ切り出し、走査する入口と走査しない入口を別の関数にした。

## 帰結

- cache-miss の起動は、初回起動と同じ構築中の表示とガードで始まる（トレイの「設定」「終了」のグレーアウト・`/o` と `/s` の拒否）。構築中に終了すると、次の起動もまた背景で構築する。
- 「cache-miss の起動は構築を起こす」は、純関数（`InitialIndex::initial_build`）のテストと、setup の呼び出しを縛るソーステキスト検査と、`smoke-startup.ps1` の「`*.bin` が生成された」検査の 3 段で守る。経緯と額は `PERFORMANCE.md`「採用: `index.bin` を読めない起動は走査を待たず、背景で構築する（#1448）」にある。
