# スクリプト / API ドキュメント

「ヘルプ → ドキュメント → スクリプト / API」は実際の schema から生成され、62 ノードすべてについて JSON キー、UI の意味、必須/任意、型・列挙、既定値、最小例、Runtime API を表示します。

- [Mod v3 形式とコンパイル契約](mod_format.md)
- [story_api / CLI](ai_cli.md)
- [多言語契約](i18n.md)
- [現在の機能と境界](current_capabilities.md)

`combat` の人物は決闘名と戦闘アニメーションだけを決め、背景は公式 `views` から独立して選択し、他の値は自由に設定します。`battle` は味方・敵の陣営、総人数、確認済みの公式名付き人物だけを設定します。旧プリセットと `battle_setup` は削除されました。ユーザーコンテンツは `user:<id>` を使います。
