# -*- coding: utf-8 -*-
"""把 free_trigger 节点写进四语言的 mod_format 契约（只跑一次的工具脚本）。

做两件事：
1. 在 §3.1 的节点表里 `end` 之后插入 `free_trigger` 一行；
2. 在 campaign.triggers 的条件清单后补一条：触发器也可以写在 story 里。
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

ROWS = {
    "chs": (
        "| `free_trigger` | `position`(地图位置 id：Center/Mall/Alchemy/Forge/"
        "BackMountain/Room1/Room2/Door/Study/Kitchen/Secret)、`script`(同包脚本 id)；"
        "可选 `when_month`(`any` 或 `\"1\"`~`\"12\"`)、`when_stage`(`any`/`\"1\"`/`\"2\"`/`\"3\"`)、"
        "`when_flag_set`、`when_flag_clear`、`when_affinity`(人物 id，留空=不判定)、"
        "`when_affinity_min`、`note`(只给自己看的备注) | "
        "**声明型节点**：本身不产生任何运行时指令，只登记一条自由模式触发器；打包时自动汇总进 "
        "`manifest.campaign.triggers`（清单里手写的排在前面，其次按「文件名序 → 节点顺序」）。"
        "因此它可以放在 `end` 之后而不影响收尾校验——判「末节点能否收尾」时会跳过声明型节点。"
        "为避免「导入自己打好的包再导出」时触发器越滚越多，与节点声明结构完全相同的清单项会在汇总时被替换掉。"
        "放在剧情中间时按普通节点顺延到下一个节点 |"
    ),
    "cht": (
        "| `free_trigger` | `position`(地圖位置 id：Center/Mall/Alchemy/Forge/"
        "BackMountain/Room1/Room2/Door/Study/Kitchen/Secret)、`script`(同包腳本 id)；"
        "可選 `when_month`(`any` 或 `\"1\"`~`\"12\"`)、`when_stage`(`any`/`\"1\"`/`\"2\"`/`\"3\"`)、"
        "`when_flag_set`、`when_flag_clear`、`when_affinity`(人物 id，留空=不判定)、"
        "`when_affinity_min`、`note`(只給自己看的備註) | "
        "**宣告型節點**：本身不產生任何執行階段指令，只登記一條自由模式觸發器；打包時自動彙總進 "
        "`manifest.campaign.triggers`（清單裡手寫的排在前面，其次按「檔名序 → 節點順序」）。"
        "因此它可以放在 `end` 之後而不影響收尾驗證——判「末節點能否收尾」時會跳過宣告型節點。"
        "為避免「匯入自己打好的包再匯出」時觸發器越滾越多，與節點宣告結構完全相同的清單項會在彙總時被取代。"
        "放在劇情中間時按普通節點順延到下一個節點 |"
    ),
    "ja": (
        "| `free_trigger` | `position`(マップ位置 id：Center/Mall/Alchemy/Forge/"
        "BackMountain/Room1/Room2/Door/Study/Kitchen/Secret)、`script`(同梱スクリプト id)；"
        "任意 `when_month`(`any` または `\"1\"`~`\"12\"`)、`when_stage`(`any`/`\"1\"`/`\"2\"`/`\"3\"`)、"
        "`when_flag_set`、`when_flag_clear`、`when_affinity`(キャラ id、空=判定しない)、"
        "`when_affinity_min`、`note`(自分用メモ) | "
        "**宣言型ノード**：実行時命令を一切生成せず、フリーモードのトリガーを 1 件登録するだけです。"
        "パッケージ時に `manifest.campaign.triggers` へ自動集約されます（マニフェストに手書きしたものが先、"
        "次に「ファイル名順 → ノード順」）。そのため `end` の後ろに置いても末尾判定に影響しません"
        "（末尾判定では宣言型ノードを飛ばします）。「自分で作ったパッケージを読み込んで再書き出し」で"
        "トリガーが増え続けないよう、ノード宣言と完全に同一のマニフェスト項目は集約時に置き換えます。"
        "ストーリー途中に置いた場合は通常ノードとして次のノードへ順送りします |"
    ),
    "ko": (
        "| `free_trigger` | `position`(맵 위치 id: Center/Mall/Alchemy/Forge/"
        "BackMountain/Room1/Room2/Door/Study/Kitchen/Secret), `script`(같은 패키지 스크립트 id); "
        "선택 `when_month`(`any` 또는 `\"1\"`~`\"12\"`), `when_stage`(`any`/`\"1\"`/`\"2\"`/`\"3\"`), "
        "`when_flag_set`, `when_flag_clear`, `when_affinity`(캐릭터 id, 비우면 판정 안 함), "
        "`when_affinity_min`, `note`(자신만 보는 메모) | "
        "**선언형 노드**: 실행 시 아무 명령도 만들지 않고 자유 모드 트리거를 한 건 등록할 뿐입니다. "
        "패키징할 때 `manifest.campaign.triggers` 로 자동 취합됩니다(매니페스트에 직접 쓴 것이 먼저, "
        "그다음이 '파일명 순 → 노드 순'). 따라서 `end` 뒤에 두어도 마지막 노드 판정에 영향을 주지 않습니다"
        "(마지막 노드 판정에서 선언형 노드는 건너뜁니다). '자기가 만든 패키지를 다시 불러와 재내보내기'할 때 "
        "트리거가 계속 늘어나지 않도록, 노드 선언과 완전히 같은 매니페스트 항목은 취합 시 교체합니다. "
        "스토리 중간에 두면 일반 노드처럼 다음 노드로 이어집니다 |"
    ),
}

NOTES = {
    "chs": "    - 也可以不写在清单里，而是在 story 中用 `free_trigger` 节点登记：打包时自动汇总进这里（" \
           "清单里手写的排在前面，其次按文件名序 → 节点顺序；详见 §3.1）。",
    "cht": "    - 也可以不寫在清單裡，而是在 story 中用 `free_trigger` 節點登記：打包時自動彙總進這裡（" \
           "清單裡手寫的排在前面，其次按檔名序 → 節點順序；詳見 §3.1）。",
    "ja": "    - マニフェストに書かず、story 内の `free_trigger` ノードで登録することもできます："
          "パッケージ時にここへ自動集約されます（手書きが先、次にファイル名順 → ノード順。§3.1 参照）。",
    "ko": "    - 매니페스트에 쓰지 않고 story 안의 `free_trigger` 노드로 등록할 수도 있습니다: "
          "패키징할 때 여기로 자동 취합됩니다(직접 쓴 것이 먼저, 그다음 파일명 순 → 노드 순. §3.1 참조).",
}


def main() -> int:
    for locale, row in ROWS.items():
        path = ROOT / "docs" / locale / "mod_format.md"
        if not path.is_file():
            print(f"[跳过] 缺少 {path}", file=sys.stderr)
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        if any(line.startswith("| `free_trigger` |") for line in lines):
            print(f"[跳过] {locale}: 已经有 free_trigger 行了")
            continue
        out: list[str] = []
        done_row = done_note = False
        for line in lines:
            out.append(line)
            if not done_row and line.startswith("| `end` |"):
                out.append(row)
                done_row = True
            # 各语言的冒号不同（：/ :），只按反引号里的字段名匹配
            elif (
                not done_note
                and "`when_affinity`" in line
                and line.lstrip().startswith("-")
            ):
                out.append(NOTES[locale])
                done_note = True
        if not (done_row and done_note):
            print(
                f"[警告] {locale}: 节点表行={done_row} 条件清单注释={done_note}，请人工检查",
                file=sys.stderr,
            )
            continue
        path.write_text("\n".join(out) + "\n", encoding="utf-8")
        print(f"[OK ] {locale}/mod_format.md 已写入节点行与说明")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
