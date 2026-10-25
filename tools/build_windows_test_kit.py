#!/usr/bin/env python3
"""Build editable Windows regression fixtures without copying any game assets."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import wave

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def node(node_type, **fields):
    return dict(type=node_type, **fields)


def story(sid, title, nodes):
    for index, item in enumerate(nodes):
        item.setdefault("id", f"n{index + 1:03}")
    return dict(story_schema=2, id=sid, title=title, start=nodes[0]["id"], nodes=nodes)


def say(text, **fields):
    return node("say", text=text, **({"mode": "narrative"} if "character" not in fields else {}), **fields)


def show(character, portrait="normal", position="M", **fields):
    return node("show", character=character, portrait=portrait, position=position,
                facing="right", fadeDuration=0, moveDuration=0, **fields)


def manifest(pid, name):
    return dict(format=3, package_format=3, story_schema=2, content_schema=1,
                min_host_version="1.1.2", id=pid, campaign_id=pid, name=name,
                version="0.0.1", author="lom_modkit", description="发布基线与新增功能的人工测试样例；不是游戏验收结果。",
                entry="main", campaign={"new_game": True})


def make_art(path, number, color, wide=False):
    size = (960, 540) if wide else (300, 520)
    image = Image.new("RGBA", size, color)
    draw = ImageDraw.Draw(image)
    draw.rectangle((15, 15, size[0]-15, size[1]-15), outline="white", width=6)
    draw.ellipse((size[0]//3, 55, size[0]*2//3, 175), fill="white")
    draw.text((30, size[1]-65), f"TEST {number}", fill="black", font_size=32)
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path)


def assets(project):
    for key, color, number in [("actor_a", "#cb6f64", "A"), ("actor_b", "#6b9dc7", "B")]:
        folder = project / f"assets/user/character/testkit.{key}"
        make_art(folder / "normal.png", number+" NORMAL", color)
        make_art(folder / "happy.png", number+" HAPPY", "#e7bf58")
        write_json(folder / "content.json", dict(schema=1, content_schema=1, id="testkit."+key,
                   type="character", name="测试演员"+number, files={"main":"normal.png"},
                   portraits={"normal":"normal.png", "happy":"happy.png"}, scale=65, art_facing="right"))
    for key, color in [("room1", "#e2b67d"), ("room2", "#7ab2ca")]:
        folder = project / f"assets/user/image/testkit.{key}"
        make_art(folder / "main.png", key.upper(), color, wide=True)
        write_json(folder / "content.json", dict(schema=1, content_schema=1, id="testkit."+key,
                   type="image", name="测试背景"+key[-1], files={"main":"main.png"}))
    folder = project / "assets/user/audio/testkit.chime"
    folder.mkdir(parents=True, exist_ok=True)
    with wave.open(str(folder / "chime.wav"), "wb") as sound:
        sound.setparams((1, 2, 22050, 0, "NONE", "not compressed"))
        sound.writeframes(b"".join(struct.pack("<h", int(6000*math.sin(2*math.pi*660*i/22050))) for i in range(11025)))
    write_json(folder / "content.json", dict(schema=1, content_schema=1, id="testkit.chime",
               type="audio", name="半秒测试音", audio_kind="sound", files={"main":"chime.wav"}))


def project(output, folder, name, stories, with_assets=False):
    dest = output / "projects" / folder
    write_json(dest / "manifest.json", manifest("wt_20261020_"+folder, name))
    for item in stories:
        write_json(dest / "story" / (item["id"]+".json"), item)
    if with_assets:
        assets(dest)
    return dest


def actor_gallery(rows, label):
    nodes = [node("scene", view="black"), say(label)]
    for row in rows:
        cid = row["id"]
        nodes.append(show(cid))
        portraits = sorted(row["portraits"], key=lambda p: (p != "normal", p))
        for portrait in portraits:
            nodes.append(say(f"{label} | {row.get('appearance_name', row['name'])} | {cid} / {portrait}",
                             character=cid, portrait=portrait))
        nodes.append(node("hide", character=cid, fadeDuration=0))
    nodes.append(node("end"))
    return nodes


def build(output, cli, editor):
    if output.exists():
        raise ValueError("输出目录必须尚未存在，避免覆盖测试记录")
    output.mkdir(parents=True)
    catalog = json.loads((ROOT / "data/editor_data.json").read_text(encoding="utf-8"))
    rows = {row["id"]: row for row in catalog["characters"]}
    projects = []
    quick = [node("scene", view="black"), say("P01 入魔小梅六个表情选项，对应五幅不同图片；逐个按下一步观察。"), show("girl5_9")]
    for emotion in ["normal", "special1", "special2", "sad2", "kick", "caught"]:
        quick.append(say("P01 入魔小梅："+emotion, character="girl5_9", portrait=emotion))
    quick += [node("hide", character="girl5_9", fadeDuration=0), show("girl5")]
    for emotion in ["normal", "special5", "special9", "special10", "special11", "special14", "gloomy2"]:
        quick.append(say("P02 普通小梅补齐表情："+emotion, character="girl5", portrait=emotion))
    quick += [node("hide", character="girl5", fadeDuration=0), node("end")]
    costumes = [rows[c] for c in ["girl5", "girl5_2", "girl5_3", "girl5_4", "girl5_5", "girl5_6", "girl5_7", "girl5_8", "girl5_9"]]
    costume_nodes = [node("scene", view="black"), say("P03 各外观逐一登场；每次先退旧外观再入新外观。")]
    for row in costumes:
        costume_nodes += [show(row["id"]), say(row["appearance_name"]+" / "+row["id"], character=row["id"], portrait="normal"), node("hide", character=row["id"], fadeDuration=0)]
    costume_nodes.append(node("end"))
    player = [node("scene", view="black")]
    for appearance in ["original", "beautified", "game"]:
        player += [show("player", appearance=appearance), say("P04 赵活外观："+appearance, character="player", portrait="normal"), node("hide", character="player", fadeDuration=0)]
    player.append(node("end"))
    projects.append(project(output, "01_portraits", "Windows测试01·立绘快速检查", [story("main", "P01 入魔与新增表情", quick), story("costumes", "P03 小梅外观逐项", costume_nodes), story("player", "P04 赵活外观恢复", player)]))

    a, b = "user:testkit.actor_a", "user:testkit.actor_b"
    parallel = [node("scene", view="black"), node("background", action="show", image="user:testkit.room1", fade=0),
                node("parallel", id="parallel_arrival", actions=[show(a, position="L1"), show(b, position="R1")]),
                say("G01 两位演员已经同时登场。下一步同时移动并发出半秒测试音。"),
                node("parallel", id="parallel_moves", actions=[node("move", character=a, **{"from":"L1"}, to="LM1", duration=0.4), node("move", character=b, **{"from":"R1"}, to="RM1", duration=1.2), node("sound", name="user:testkit.chime", kind="sound", op="play")]),
                say("G02 移动完成才出现此句：B 的移动约 1.2 秒，不能把两人时长累加。"),
                node("parallel", id="parallel_adjust", actions=[node("offset", character=a, x=10, y=5, duration=0.8), node("rotate", character=b, angle=15, duration=0.8)]),
                say("G03 A 偏移与 B 旋转同时完成。"),
                node("parallel", id="parallel_hides", actions=[node("hide", character=a, fadeDuration=0.25), node("hide", character=b, fadeDuration=1.0)]),
                say("G04 此句应在 B 完全退场后出现。"), node("end")]
    official = [node("scene", view="black"), show("player", position="L1", appearance="original"), show("girl5", position="R1"),
                say("G05 接下来对两位官方人物同时移动；检查资源预载不会卡住。"),
                node("parallel", id="official_moves", actions=[node("move", character="player", **{"from":"L1"}, to="LM1", duration=0.5), node("move", character="girl5", **{"from":"R1"}, to="RM1", duration=1.0)]),
                node("parallel", id="official_hides", actions=[node("hide", character="player", fadeDuration=0.5), node("hide", character="girl5", fadeDuration=0.5)]), say("G05 官方人物同时动作结束。"), node("end")]
    projects.append(project(output, "02_parallel", "Windows测试02·同时执行", [story("main", "G01 自制演员同时动作", parallel), story("official", "G05 官方人物预载", official)], True))

    blackouts = []
    for direction, sid in [("lr", "main"), ("rl", "reverse")]:
        blackouts.append(story(sid, "K01 黑幕方向 "+direction, [node("scene", view="black"), node("background", action="show", image="user:testkit.room1", fade=0), show(a, position="L1"),
             node("overlay", action="show", slot="testkit_badge", image="user:testkit.room1", position="center", scale=25, opacity=100, layer="front", fade=0),
             say("K01 现在是暖色背景。接下来盖黑幕、换蓝背景、摆两人，再揭幕。"),
             node("transition", id="cover", phase="in", dir=direction),
             node("background", id="change_background", action="show", image="user:testkit.room2", fade=0),
             node("parallel", id="black_setup", actions=[show(a, position="L1"), show(b, position="R1"), node("wait", seconds=1)]),
             node("transition", id="reveal", phase="out", dir=direction),
             say("K02 揭幕以后才应看到蓝背景、两人和前景；黑幕内不能先闪出新画面。"),
             node("overlay", action="hide", slot="testkit_badge", fade=0), node("hide", character=a, fadeDuration=0), node("hide", character=b, fadeDuration=0), node("end")]))
    projects.append(project(output, "03_blackout", "Windows测试03·黑幕持续与中途试玩", blackouts, True))

    edit = story("main", "同名章节", [say("E01 供保存、撤销、跨章复制、流程图及结果窗口检查。"),
         node("choice", id="choose", dialog="Options", options=[{"text":"分支 A","goto":"path_a"},{"text":"分支 B","goto":"path_b"}]),
         say("走到分支 A", id="path_a", goto="join"), say("走到分支 B", id="path_b", goto="join"),
         say("两条路径在这里汇合", id="join"), node("end", next_script="second")])
    edit["_editor"] = {"tests":[{"name":"choose_a","story":"main","initial":{"variables":{},"flags":{}},"actions":{"choices":[{"node":"choose","option":0}]},"assert":{"reaches_ending":True}},
                                {"name":"choose_b","story":"main","initial":{"variables":{},"flags":{}},"actions":{"choices":[{"node":"choose","option":1}]},"assert":{"reaches_ending":True}}]}
    second = story("second", "同名章节", [node("scene", view="black"), show(a), say("E02 第二章：用于检查同名章节编号、内容保存和音频试听。", character=a, portrait="happy"), node("sound", name="user:testkit.chime", kind="sound", op="play"),
                     node("intro", intro_source="custom", name="测试介绍卡", title="自制素材", text="与官方人物资料无关的测试文字。", image="assets/user/image/testkit.room2/main.png", image_scale=100, image_x=0, image_y=0), node("hide", character=a, fadeDuration=0), node("end")])
    projects.append(project(output, "04_editor", "Windows测试04·编辑流程回归", [edit, second], True))
    gallery = []
    for group in sorted({row["appearance_group"] for row in rows.values() if row.get("appearance_group")}):
        members = [row for row in rows.values() if row.get("appearance_group") == group]
        sid = "main" if group == "player" else group
        gallery.append(story(sid, "全量·"+rows[group]["name"], actor_gallery(members, rows[group]["name"])))
    projects.append(project(output, "05_gallery", "Windows测试05·重要人物完整画廊", gallery))
    showcase = output / "projects/06_showcase"
    showcase.mkdir()
    for name in ["manifest.json", "README.md"]:
        shutil.copyfile(ROOT / "samples/showcase3" / name, showcase / name)
    for name in ["story", "assets"]:
        shutil.copytree(ROOT / "samples/showcase3" / name, showcase / name)
    projects.append(showcase)

    invalid = {
        "N01_one_action": ([node("wait", seconds=1)], "2~32"),
        "N02_same_actor": ([node("hide", character="player"), node("move", character="player", **{"from":"L1"}, to="R1", duration=1)], "都会控制人物"),
        "N03_dialog_inside": ([say("组内不允许对白"), node("wait", seconds=1)], "不支持类型"),
        "N04_nested": ([node("parallel", actions=[]), node("wait", seconds=1)], "不支持类型"),
        "N05_negative_time": ([node("wait", seconds=-1), node("wait", seconds=1)], "非负"),
        "N06_player_switch": ([show("player", appearance="beautified"), node("wait", seconds=1)], "外观切换"),
        "N07_33_actions": ([node("wait", seconds=0) for _ in range(33)], "2~32"),
        "N08_child_goto": ([node("wait", seconds=1, goto="finish"), node("wait", seconds=1)], "id/goto"),
    }
    negative_cases = []
    for name, (actions, needle) in invalid.items():
        item = story("main", name, [node("parallel", actions=actions), node("end", id="finish")])
        path = output / "negative" / (name+".json");write_json(path, item)
        negative_cases.append({"id":name,"path":str(path.relative_to(output)).replace("\\","/"),"expect_error_contains":needle})
    for name, item, needle in [
        ("N09_wrong_portrait", show("girl5_9", "special5"), "没有表情"),
        ("N10_missing_position", node("show", character="player"), "position"),
        ("N11_wrong_appearance", show("sister1", appearance="beautified"), "appearance"),
    ]:
        path=output/"negative"/(name+".json");write_json(path,story("main",name,[item,node("end")]))
        negative_cases.append({"id":name,"path":str(path.relative_to(output)).replace("\\","/"),"expect_error_contains":needle})
    write_json(output/"negative-cases.json", negative_cases)

    fixtures=output/"fixtures/import_images"
    for i, name in enumerate(["26_04_23_17_53_52.png","中文立绘.png","123.png","normal.png","happy.png","portrait_1.png","face-smile.png"]):
        make_art(fixtures/name,str(i+1),["#ca7368","#72a5c8","#e6bf60"][i%3])
    make_art(fixtures/"A/normal.png","A NORMAL","#cb6f64")
    make_art(fixtures/"B/normal.png","B NORMAL","#6b9dc7")
    (output/"fixtures/not_a_library").mkdir(parents=True)
    (output/"fixtures/not_a_library/保留.txt").write_text("不是素材库，选择后应提示重新提取；不能替换已有素材库。",encoding="utf-8")
    write_json(output/"fixtures/incomplete_library/preview_map.json",{"characters":{"girl5_9":{"first":"normal","portraits":{"normal":"assets/missing.png"}}},"views":{}})
    (output/"fixtures/broken.wav").write_bytes(b"This is deliberately not audio.")
    shutil.copyfile(projects[1]/"assets/user/audio/testkit.chime/chime.wav",output/"fixtures/valid.wav")
    (output/"fixtures/external-preserve.txt").write_text("外部文件：保存、撤销、恢复和另存不应删改此标记。",encoding="utf-8")

    logs=output/"validation";logs.mkdir()
    results=[]
    def run(label, args, expected=0, needle=None):
        result=subprocess.run([str(cli),*map(str,args),"--json"],capture_output=True,text=True)
        text=result.stdout+result.stderr
        (logs/(label+".log")).write_text(text,encoding="utf-8")
        ok=(result.returncode==0 if expected==0 else result.returncode!=0) and (needle is None or needle in text)
        results.append({"check":label,"passed":ok,"exit_code":result.returncode,"expected":"success" if expected==0 else "rejection"})
        if not ok:raise RuntimeError(label+": "+text[:1500])
    packages=output/"packages";packages.mkdir()
    for folder in projects:
        for file in (folder/"story").glob("*.json"):
            run(folder.name+"_"+file.stem,["check",file])
        package=packages/(folder.name+".lommod")
        run(folder.name+"_pack",["pack",folder,"-o",package])
        run(folder.name+"_inspect",["inspect",package])
        result=subprocess.run([str(editor),"--smoke-preview",str(folder)],capture_output=True,text=True)
        (logs/(folder.name+"_preview.log")).write_text(result.stdout+result.stderr)
        if result.returncode:raise RuntimeError(result.stderr)
        results.append({"check":folder.name+"_preview","passed":True,"result":json.loads(result.stdout)})
    run("editor_offline_tests",["test",projects[3]])
    for case in negative_cases:
        run(case["id"],["check",output/case["path"]],1,case["expect_error_contains"])
    write_json(output/"test-projects.json",[{"id":p.name,"path":str(p.relative_to(output)).replace("\\","/"),"package":"packages/"+p.name+".lommod"} for p in projects])
    write_json(logs/"local-verification.json",{"platform":"macOS/local","game_started":False,"windows_tested":False,"results":results})
    for path in (ROOT / "tools/windows_test_kit").iterdir():
        if path.is_file():
            encoding = "utf-8-sig" if path.suffix == ".ps1" else "utf-8"
            (output/path.name).write_text(path.read_text(encoding="utf-8-sig"), encoding=encoding)
    shutil.copyfile(ROOT / "docs/chs/windows_postrelease_test.md", output / "Windows测试清单.md")
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()
    write_json(output/"baseline.json", {
        "release_tag": "v1.2.1", "release_commit": git("rev-parse", "refs/codex/test-baseline-v1.2.1"),
        "checkout_head": git("rev-parse", "HEAD"),
        "source_state": "v1.2.1 fixes restored into working tree; newer features preserved; uncommitted",
        "game_assets_included": False, "windows_binaries_included": False,
        "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "windows_tested": False, "game_started": False,
    })
    write_json(output/"SHA256SUMS.json", {str(p.relative_to(output)).replace("\\","/"):hashlib.sha256(p.read_bytes()).hexdigest()
               for p in sorted(output.rglob("*")) if p.is_file()})
    return projects


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    parser.add_argument("--cli",type=Path,default=ROOT/"target/debug/lomc")
    parser.add_argument("--editor",type=Path,default=ROOT/"target/debug/lom-editor")
    args=parser.parse_args()
    build(args.output.resolve(),args.cli.resolve(),args.editor.resolve())
    print(args.output.resolve())
