# 現在の機能と境界

この文書は現在のリポジトリコードだけを説明します。ノードの正規集合は `editor/models.py` の `NODE_SCHEMAS` で、現在 62 種です。

## 実装済み

- シナリオ、舞台演出、複数章、ローカライズ、カスタム人物/音声/背景/CG/Overlay、カスタム結末。
- タイトルの原作風「MOD キャンペーン開始」ロード画面。MOD 手動スロット、三種の自動スロット、Universe 最近スロット、永続変数を原作から分離。Manifest の場所/時刻/Flag/好感度トリガー。
- `custom_shop` は原作ショップ在庫を一時置換し、終了時に復元します。
- F5 テスト、ホットリロード/Debugger、Editing/Release 検査、復旧コピー、テンプレート、統計、音声カバレッジ、ローカル Release Builder、インストール診断、Runtime ロールバック。
- 強制的な非公式表示、パッケージ指紋、オフライン画像/動画ウォーターマーク検出。作者署名ではありません。

## 戦闘の境界

`combat` は原作一対一キャラクター／場面テンプレートを使い、今回の相手 HP、スタミナ、能力、才能、奥義、行動確率を上書きできます。`battle` は原作の味方・敵・中立 roster を別々に再利用し、人数、NPC HP、プレイヤースキルを設定します。前者は Combat win/lose、後者は finish=true の FriendWin/EnemyWin だけを返します。原作 asset は変更せず、独自 Battle Engine でもありません。実機検証は必要です。

低レベルの `enemy`、`battle_skill`、`goto_scene` は互換性と高度な構成向けに維持します。

`combat` の人物は四種類の戦闘アニメーションだけを決め、体力、能力、スキル、行動確率は自由に設定します。`battle` は味方・敵の陣営、総人数、確認済みの公式名付き人物だけを設定し、名付き人物も総人数に含まれます。旧プリセットと `battle_setup` は削除済みです。永続状態は `mod_campaign_<campaign_id>` 専用スロットに結び付き、原作 GameSave は変更しません。

低レベルの `enemy` / `battle_skill` は独立して利用でき、Combat / Battle の結果は `reward`、`result_screen`、`custom_shop`、`mod_quest` などの既存ノードと組み合わせられます。
長期整数状態には `persistent_var` / `persistent_check` を使います。

逆コンパイルと実機確認で確定していないゲーム API は推測で実装しません。
