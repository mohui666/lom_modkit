import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from story_route_reader import parse_script, resolve_returns


class StoryRouteTests(unittest.TestCase):
    def test_choice_branches_merges_and_comments(self):
        parsed = parse_script("ENTRY", '''
-- luamanager.SetNextScript("COMMENT")
local quoted = 'luamanager.SetNextScript("QUOTED")'
local options = {}
options[1] = "O_LEFT"
options[2] = "O_RIGHT"
local choice = choose(options)
if choice == 1 then
    luamanager.SetNextScript("LEFT")
elseif choice == 2 then
    statmodifymanager.Mission("Main", "RIGHT")
else
    luamanager.SetNextScript("RETURN")
end
statmodifymanager.Mission("Side", "NEXT")
''', {"O_LEFT": "向左", "O_RIGHT": "向右"})
        self.assertEqual([link["target"] for link in parsed["links"]], ["LEFT", "RETURN"])
        self.assertEqual(parsed["links"][0]["condition"], "选项：向左")
        self.assertEqual(parsed["missions"][0]["condition"], "否则若 选项：向右")
        self.assertEqual(parsed["missions"][1]["condition"], "")

    def test_dynamic_or_reassigned_targets_are_not_guessed(self):
        parsed = parse_script("ENTRY", '''
local fixed = "KNOWN"
luamanager.SetNextScript(fixed)
if flag then fixed = "OTHER" end
luamanager.SetNextScript(fixed)
luamanager.SetNextScript(compute())
luamanager.ChangeScene("Battle", "MAP", "Free")
''')
        self.assertEqual([link["target"] for link in parsed["links"]], ["KNOWN"])
        self.assertEqual(parsed["unknown_jumps"], 2)

    def test_fast_route_scan_preserves_multiline_literals_and_nested_calls(self):
        parsed = parse_script("ENTRY", '''
say([=[
if fake then luamanager.SetNextScript("WRONG") end
]=])
--[[ luamanager.SetNextScript("COMMENT") ]]
local target = [=[RIGHT]=]
runwait(wrapper(luamanager.SetNextScript(target)))
''')
        self.assertEqual([link["target"] for link in parsed["links"]], ["RIGHT"])

    def test_position_results_use_configured_candidates(self):
        parsed = parse_script("ENTRY", '''
var1 = checkpointmanager.Position("TEST")
luamanager.SetNextScript(var1)
''', positions={"TEST": ["event_001", "event_501"]}, include_dynamic=True)
        self.assertEqual([link["target"] for link in parsed["links"]], ["event_001", "event_501"])
        self.assertTrue(all("配置候选" in link["condition"] for link in parsed["links"]))

    def test_return_slots_and_global_variables_follow_the_calling_branch(self):
        sources = {
            "ENTRY": '''
if flag then
    luamanager.SetTempScript(1, "LEFT")
    var1 = checkpointmanager.Position("TEST")
    luamanager.SetNextScript("CHILD")
else
    luamanager.SetTempScript(1, "RIGHT")
    luamanager.SetNextScript("OTHER_CHILD")
end
''',
            "CHILD": "luamanager.SetNextScript(var1)",
            "EVENT": "var2 = luamanager.GetTempScript(1)\nluamanager.SetNextScript(var2)",
            "OTHER_CHILD": "var1 = luamanager.GetTempScript(1)\nluamanager.SetNextScript(var1)",
            "LEFT": "", "RIGHT": "",
        }
        routes = resolve_returns([parse_script(name, code, positions={"TEST": ["EVENT"]}, include_dynamic=True)
                                  for name, code in sources.items()])
        routes = {route["script"]: route for route in routes}
        self.assertEqual([link["target"] for link in routes["CHILD"]["links"]], ["EVENT"])
        self.assertEqual([link["target"] for link in routes["EVENT"]["links"]], ["LEFT"])
        self.assertEqual([link["target"] for link in routes["OTHER_CHILD"]["links"]], ["RIGHT"])
        self.assertTrue(all(route["unknown_jumps"] == 0 for route in routes.values()))
        self.assertTrue(all(not any(key.startswith("_") for key in route) for route in routes.values()))

    def test_travel_returns_keep_all_known_candidates_when_call_context_is_absent(self):
        sources = {
            "SETUP": 'luamanager.SetCurrentTravelScript("TRAVEL")',
            "EVENT": 'var1 = luamanager.GetCurrentTravelScript()\nluamanager.SetNextScript(var1)',
            "TRAVEL": "",
        }
        routes = resolve_returns([parse_script(name, code, include_dynamic=True) for name, code in sources.items()])
        event = next(route for route in routes if route["script"] == "EVENT")
        self.assertEqual(event["links"][0]["target"], "TRAVEL")
        self.assertIn("当前游历章节", event["links"][0]["condition"])
        self.assertIn("调用上下文未记录", event["links"][0]["condition"])
        self.assertEqual(event["unknown_jumps"], 0)

    def test_conditional_global_assignment_merges_and_concatenations_stay_unknown(self):
        route = parse_script("ENTRY", '''
var1 = "LEFT"
if flag then var1 = "RIGHT" end
luamanager.SetNextScript(var1)
''', include_dynamic=True)
        routes = resolve_returns([route, parse_script("LEFT", "", include_dynamic=True),
                                  parse_script("RIGHT", "", include_dynamic=True)])
        self.assertEqual([link["target"] for link in routes[0]["links"]], ["LEFT", "RIGHT"])
        unknown = parse_script("ENTRY", 'local target = "PREFIX" .. compute()\nluamanager.SetNextScript(target)')
        self.assertEqual(unknown["links"], [])
        self.assertEqual(unknown["unknown_jumps"], 1)

    def test_unknown_runtime_write_never_uses_other_callers_as_a_fallback(self):
        sources = {
            "SETUP": 'luamanager.SetTempScript(1, "KNOWN")',
            "ENTRY": 'luamanager.SetTempScript(1, compute())\nluamanager.SetNextScript("CHILD")',
            "CHILD": 'var1 = luamanager.GetTempScript(1)\nluamanager.SetNextScript(var1)',
            "KNOWN": "",
        }
        routes = resolve_returns([parse_script(name, code, include_dynamic=True) for name, code in sources.items()])
        child = next(route for route in routes if route["script"] == "CHILD")
        self.assertEqual(child["links"], [])
        self.assertEqual(child["unknown_jumps"], 1)


if __name__ == "__main__":
    unittest.main()
