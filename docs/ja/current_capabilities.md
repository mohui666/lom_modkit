# 現在の機能と境界

この文書は現在のリポジトリコードだけを説明します。ノードの正規集合は `editor/models.py` の `NODE_SCHEMAS` で、現在 62 種です。

## 実装済み

- シナリオ、舞台演出、複数章、ローカライズ、カスタム人物/音声/背景/CG/Overlay、カスタム結末。
- `campaign.new_game` の `mod_<id>` 分離セーブスロットと、Manifest の場所/時刻/Flag/好感度トリガー。
- `custom_shop` は原作ショップ在庫を一時置換し、終了時に復元します。
- F5 テスト、ホットリロード/Debugger、Editing/Release 検査、復旧コピー、テンプレート、統計、音声カバレッジ、ローカル Release Builder、インストール診断、Runtime ロールバック。
- 強制的な非公式表示、パッケージ指紋、オフライン画像/動画ウォーターマーク検出。作者署名ではありません。

## 戦闘の境界

`enemy`、`battle_skill`、`goto_scene` の `Combat` / `Battle` は検証済み原作 API を呼びます。高レベル `combat` / `battle` は原作テンプレートを選び、前者は Combat win/lose、後者は finish=true の FriendWin/EnemyWin だけを戻します。独自 Battle Engine ではなく、実機未検証です。

章設定で Battle Preset を管理し、原作 `combat` / `battle` テンプレートと検証済み敵設定を再利用できます。`battle_result` は実 win/lose だけを分岐し、`battle_setup`、`reward`、`activity` は既存 API だけを集約します。`mod_quest` / `quest_check` は Host セッション状態です。`persistent_var` / `persistent_check` は Int32 状態を `mod_<id>` 専用スロットに結び付いた Host sidecar へ原子的に保存し、原作 GameSave は変更しません。任意 Lua オブジェクトは永続化しません。draw/escape、戦闘マップ、モデル、AI、アニメーション、機構のカスタムにも対応しません。

逆コンパイルと実機確認で確定していないゲーム API は推測で実装しません。
