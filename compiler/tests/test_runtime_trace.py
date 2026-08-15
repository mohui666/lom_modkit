# -*- coding: utf-8 -*-
import unittest

from lomc import compile_story


class RuntimeTraceCodegenTest(unittest.TestCase):
    def test_codegen_emits_optional_node_hook_without_changing_flow_body(self):
        story = {"id": "main", "start": "c1", "nodes": [
            {"id": "c1", "type": "choice", "options": [
                {"text": "检查", "goto": "b1"}, {"text": "结束", "goto": "e1"}]},
            {"id": "b1", "type": "branch", "source": "mod", "flag": "READY", "cases": [
                {"value": 1, "goto": "e1"}, {"value": 2, "goto": "e2"}]},
            {"id": "e1", "type": "end"},
            {"id": "e2", "type": "end"},
        ]}
        lua = compile_story(story, mod_info={"id": "trace", "name": "Trace", "version": "1"})
        self.assertIn('if mod_trace_node then mod_trace_node("c1", "choice") end', lua)
        self.assertIn('\tif choice1 == 1 then\n\t\treturn node_b1()', lua)
        self.assertIn('\tif modflags["READY"] then\n\t\treturn node_e1()', lua)


if __name__ == "__main__": unittest.main()
