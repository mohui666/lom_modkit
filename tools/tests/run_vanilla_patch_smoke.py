"""Run the compiler's patch wrapper in the installed game's MoonSharp interpreter."""
import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dotnet", required=True, type=Path)
    parser.add_argument("--fungus", required=True, type=Path)
    parser.add_argument("--lomc", required=True, type=Path)
    parser.add_argument("--segment-package", type=Path, help="Also execute the showcase's whole-segment insertion")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = root / "out/vanilla-patch-lua"
    output.mkdir(parents=True, exist_ok=True)
    if args.segment_package:
        with zipfile.ZipFile(args.segment_package) as package:
            for entry, name in [("story/original_story_import.json", "import-segment.json"),
                                ("lua/original_story_import.lua", "import-segment.lua"),
                                ("texts.json", "import-segment-texts.json")]:
                (output / name).write_bytes(package.read(entry))
    full = json.loads((root / "rust/lom-core/tests/fixtures/vanilla-import.json").read_text(encoding="utf-8"))
    (output / "import-original.lua").write_text(full["source"],encoding="utf-8",newline="")
    variants = {"baseline":full["baseline"],"edited":full["story"]}
    speaker = copy.deepcopy(full["story"])
    speaker["nodes"][1]["text"] = "原版甲"
    inserted = next(node for node in speaker["nodes"] if node["id"]=="inserted")
    inserted.update(inherit_dialog=False,mode="center")
    variants["speaker"] = speaker
    branch = copy.deepcopy(full["baseline"])
    branch["nodes"][5:5] = [
        {"id":"check","type":"flag_check","source":"condition","flag":"OUTER","success":"inserted","failure":"resume"},
        {"id":"inserted","type":"say","text":"新增对白","inherit_dialog":True},
        {"id":"resume","type":"end","resume_original":True},
    ]
    variants["branch"] = branch
    choice = copy.deepcopy(full["baseline"])
    choice["nodes"][5:5] = [
        {"id":"choose","type":"choice","options":[{"text":"临时选项甲","goto":"resume"},{"text":"临时选项乙","goto":"resume"}]},
        {"id":"resume","type":"end","resume_original":True},
    ]
    variants["choice"] = choice
    terminal = copy.deepcopy(full["baseline"])
    origin = terminal["vanilla_import"]["origins"][0]
    origin["code"] = "local function original_inner()\n" + origin["code"]
    origin["node"]["code"] = origin["code"]
    terminal["nodes"][0]["code"] = origin["code"]
    tail = "end\noriginal_inner()\nmark_progress(999)\n"
    ordinal = len(terminal["vanilla_import"]["origins"])
    terminal["vanilla_import"]["origins"].append({"code":tail,"node":{"id":"native_tail","type":"raw","code":tail}})
    terminal["nodes"].append({"id":"native_tail","type":"raw","code":tail,"_editor":{"vanilla_origin":ordinal}})
    terminal["nodes"].insert(1,{"id":"finish","type":"end"})
    variants["terminal"] = terminal
    for name,story in variants.items():
        path=output/f"import-{name}.json"
        path.write_text(json.dumps(story,ensure_ascii=False),encoding="utf-8")
        subprocess.run([str(args.lomc.resolve()),"compile",str(path),"-o",str(path.with_suffix(".lua"))],check=True,cwd=root)
    fixture = json.loads((root / "rust/lom-core/tests/fixtures/vanilla-patch.json").read_text(encoding="utf-8"))
    (output / "original.lua").write_text(fixture["source"], encoding="utf-8", newline="")
    (output / "story.json").write_text(json.dumps(fixture["story"],ensure_ascii=False),encoding="utf-8")
    subprocess.run([str(args.lomc.resolve()),"compile",str(output/"story.json"),"-o",str(output/"compiled.lua")],check=True,cwd=root)
    subprocess.run([str(args.lomc.resolve()),"compile",str(root/"rust/lom-core/tests/fixtures/message-controls.json"),"-o",str(output/"message-controls.lua")],check=True,cwd=root)
    environment = dict(os.environ, DOTNET_ROOT=str(args.dotnet.resolve().parent))
    subprocess.run([str(args.dotnet.resolve()),"build",str(root/"tools/tests/vanilla_patch_lua/VanillaPatchLua.csproj"),
                    "-p:FungusAssembly="+str(args.fungus.resolve()),
                    "-p:BaseIntermediateOutputPath="+str(output/"obj")+os.sep,
                    "--output",str(output/"bin")],check=True,cwd=root,env=environment)
    subprocess.run([str(args.dotnet.resolve()),str(output/"bin/VanillaPatchLua.dll"),str(args.fungus.resolve()),str(output)],
                   check=True,cwd=root,env=environment)


if __name__ == "__main__":
    main()
