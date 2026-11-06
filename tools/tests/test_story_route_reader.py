import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from story_route_reader import parse_script


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


if __name__ == "__main__":
    unittest.main()
