#!/usr/bin/env python3
"""Run generated parallel groups in the installed game's MoonSharp only.

No game/Unity method is invoked: Lua presentation globals are deterministic fakes.
Usage: python tools/tests/run_parallel_lua_smoke.py --dotnet <dotnet> --fungus <Fungus.dll>
"""
import argparse
import json
from pathlib import Path
import subprocess
import os

PRELUDE = r'''
clock = 0
events = {}
local loaded = {}
local function event(kind, id)
    events[#events + 1] = {kind = kind, id = id, time = clock}
end
function wait(seconds)
    local finish = clock + seconds
    while clock + 0.000000001 < finish do coroutine.yield() end
end
function runwait(task)
    if task.load then
        wait(0.1)
        loaded[task.load] = true
        event('loaded', task.load)
    else
        if task.id then event('start', task.id) end
        wait(task.seconds or 0)
    end
end
characters = {
    LoadCharacterAsset = function(id) return {load = id} end,
    LoadCharacterPortrait = function(id, portrait) assert(loaded[id]) end,
    Get = function(id) assert(loaded[id]); return id end,
    GetPortrait = function(id, portrait) return portrait end,
    Focus = function(id) end,
    Rotate = function(id, angle, seconds) event('start', id) end
}
stage = {
    FadeDuration = 0.2,
    show = function(options) event('start', options.character) end,
    hide = function(options) event('start', options.character) end
}
function mod_set_mood(value) end
function mod_hide_mood() end
function mod_hide_all() end
function mod_stop_voice() end
function mod_char_hide(id, fade) event('start', id) end
function mod_char_move(id, from, to, duration) event('start', id) end
effects = {SetupEffect = function(name, ...)
    event('start', name)
    if name == 'FORCE_FAIL' then error('fixture branch failure') end
end}
transitionblack = {
    TransitionIn = function(dir) return {id='transition', seconds=0.7} end,
    TransitionOut = function(dir) return {id='transition', seconds=0.7} end
}
luamanager = {ChangeScene = function(...) event('next', 'next') end}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dotnet", required=True, type=Path)
    parser.add_argument("--fungus", required=True, type=Path)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = (args.output_dir or root / "out/parallel-lua-smoke").resolve()
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(Path.home()/".cargo/bin/cargo"), "build", "--locked", "-p", "lomc"], cwd=root, check=True)
    scenarios = {
        "joined": [
            {"type":"move", "character":"player", "from":"L", "to":"M", "duration":0.4},
            {"type":"move", "character":"brother1", "from":"R", "to":"M", "duration":0.9},
            {"type":"hide", "character":"user:fixture.hero", "fadeDuration":0.6}],
        "instant": [{"type":"effect", "name":"Hit_001"}, {"type":"effect", "name":"Blood_002"}],
        "failure": [{"type":"effect", "name":"FORCE_FAIL"}, {"type":"wait", "seconds":0.5}],
        "portraits": [{"type":"show", "character":"player", "position":"L", "fadeDuration":0.8},
                      {"type":"hide", "character":"brother1", "fadeDuration":0.3}],
        "transition": [{"type":"transition", "phase":"out"}, {"type":"effect", "name":"Hit_001"}],
    }
    for name, actions in scenarios.items():
        story = {"id":"main", "story_schema":2, "start":"together", "nodes":[
            {"id":"together", "type":"parallel", "actions":actions}, {"id":"end", "type":"end"}]}
        source = output / (name+".json")
        source.write_text(json.dumps(story, ensure_ascii=False), encoding="utf-8")
        subprocess.run([str(root/"target/debug/lomc"), "build", str(source), "-o", str(output/(name+".lua")), "--json"], check=True, cwd=root)
    (output/"prelude.lua").write_text(PRELUDE, encoding="utf-8")
    env = os.environ.copy()
    env["DOTNET_ROOT"] = str(args.dotnet.resolve().parent)
    subprocess.run([str(args.dotnet.resolve()), "build", str(root/"tools/tests/parallel_lua/ParallelLua.csproj"),
                    "-p:FungusAssembly="+str(args.fungus.resolve()),
                    "-o", str(output/"bin"),
                    "-p:BaseIntermediateOutputPath="+str(output/"obj")+os.sep,
                    ], env=env, cwd=root, check=True)
    subprocess.run([str(args.dotnet.resolve()), str(output/"bin/ParallelLua.dll"), str(args.fungus.resolve()), str(output)], env=env, cwd=root, check=True)


if __name__ == "__main__":
    main()
