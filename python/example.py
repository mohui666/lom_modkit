"""Run after SDK export: python example.py PATH/TO/lom-editor.exe OUTPUT_DIR"""
from pathlib import Path
import sys
from lom_modkit import CLI, Chapter, Choice, End, Option, Project, Say

cli = CLI(sys.argv[1])
output = Path(sys.argv[2]).resolve()
chapter = Chapter(id="main", title="山门初见", nodes=[
    Say(id="opening", mode="narrative", text="山雨初歇，远处传来钟声。"),
    Choice(id="choose", options=[
        Option(text="上前问路", goto="ask"),
        Option(text="先歇一会", goto="rest"),
    ]),
    Say(id="ask", mode="narrative", text="你循着钟声，沿石阶向上。", goto="finish"),
    Say(id="rest", mode="narrative", text="你在檐下稍作休息。", goto="finish"),
    End(id="finish"),
])
project = Project(id="python_story", name="山门初见", author="剧本作者", chapters=[chapter])
project.write(output.parent / "story-recipe.json")
print(cli.build(project, output))
print(cli.export(output, output.parent / "山门初见.lommod"))
