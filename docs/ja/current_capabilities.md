# 現在の機能と境界

この文書は現在のリポジトリコードだけを説明します。ノードの正規集合は `editor/models.py` の `NODE_SCHEMAS` で、現在 47 種です。

## 実装済み

- シナリオ、舞台演出、複数章、ローカライズ、カスタム人物/音声/背景/CG/Overlay、カスタム結末。
- `campaign.new_game` の `mod_<id>` 分離セーブスロットと、Manifest の場所/時刻/Flag/好感度トリガー。
- F5 テスト、ホットリロード/Debugger、Editing/Release 検査、復旧コピー、テンプレート、統計、音声カバレッジ、ローカル Release Builder、インストール診断、Runtime ロールバック。
- 強制的な非公式表示、パッケージ指紋、オフライン画像/動画ウォーターマーク検出。作者署名ではありません。

## 戦闘の境界

`enemy`、`battle_skill`、`goto_scene` の `Combat` / `Battle` は、検証済みの原作《活侠传》API を呼びます。高レベル `combat` は原作 Combat key と敵設定を組み合わせ、`CombatManager.GameOver(bool)` の実際の win/lose を作者ノードへ戻します。独自 Battle Engine ではなく、新しい流れは実機未検証です。

高レベル `battle`、Battle Preset、draw/escape コールバック、`reward` 集約ノード、任意商品 Custom Shop、独立 `mod_quest`、任意の永続 Mod 変数は未実装です。`modflags` / `modvars` は Story セッション限定で、`game_flag` は原作に存在する FlagData だけを書きます。戦闘マップ、モデル、AI、アニメーション、機構のカスタムにも対応しません。

逆コンパイルと実機確認で確定していないゲーム API は推測で実装しません。
