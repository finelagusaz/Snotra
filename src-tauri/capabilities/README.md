# capabilities/（意図的に空）

**このディレクトリは capability を 1 つも持たないが、存在し続ける必要がある。** 消すと `snotra`（`src-tauri`）が**ローカルで毎回フルリビルドされる**（release で 1 回 3 分 20 秒・#701 の follow-up として計測）。

## 機構

`build.rs` の `tauri_build::build()` は、`capabilities_path_pattern` 属性を渡さない既定経路で次を**無条件に**出す（`tauri-build-2.6.3/src/acl.rs:427`）:

```
cargo:rerun-if-changed=capabilities
```

**cargo は `rerun-if-changed` の対象が存在しないとき、その crate を毎回 dirty と判定する。** ゆえにディレクトリ不在は「ビルドが壊れる」ではなく「**毎回 3 分余計にかかる**」という形で現れ、CI（毎回クリーン checkout ゆえ常に 1 回だけビルド）では**永久に顕在化しない**。実際、`capabilities/main.json` を削除した #662（#532 SU7 PR3・IPC とフロントの撤去で ACL が不要になった）から数サイクル、ローカルだけがこのコストを払っていた。

## この README がここに在っても無害な理由

capability の走査は `parse_capabilities("./capabilities/**/*")` だが、glob の結果は
`CAPABILITY_FILE_EXTENSIONS`（`json` / `toml`、feature 次第で `json5`）で絞られる
（`tauri-utils-2.8.3/src/acl/build.rs`）。**`.md` は読まれない**ので、ACL の意味は
「capability ゼロ」のまま変わらない。

## capability が再び必要になったら

このディレクトリへ `*.json` を置けばよい（tauri の標準レイアウトを保っているのはそのため）。その時点で `cargo:rerun-if-changed` が本来の役割——capability 変更時の再ビルド——を果たすようになる。
