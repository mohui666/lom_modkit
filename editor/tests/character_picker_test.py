import os
import sys
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from PySide6.QtWidgets import QApplication, QComboBox
import models
from node_form import NodeForm


class CharacterPickerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_completer_label_with_stale_index_stores_character_id(self):
        form = NodeForm()
        data = {**models.FALLBACK_EDITOR_DATA, "characters": [
            {"id": "player", "name": "赵活", "portraits": ["normal", "handsome"]},
            {"id": "brother4", "name": "四师兄", "portraits": ["normal"]},
        ]}
        form.set_context(data, ["show1"])
        node = {"id": "show1", "type": "show", "character": "player", "portrait": "normal"}
        form.set_node(node)
        combo = next(c for c in form.findChildren(QComboBox) if c.findData("brother4") >= 0)
        combo.setEditText(combo.itemText(combo.findData("brother4")))
        self.assertEqual(node["character"], "brother4")

    def test_filtered_out_completion_uses_remembered_id(self):
        form = NodeForm()
        combo = form._make_combo([(str(i), "人物（%s）" % i) for i in range(20)], "0", True)
        combo._apply_filter("19")
        label = "人物（3）"
        combo.setEditText(label)
        self.assertEqual(form._combo_value(combo, label), "3")

    def test_unknown_typed_id_remains_editable(self):
        combo = QComboBox()
        combo.setEditable(True)
        combo.addItem("赵活（player）", "player")
        combo.setEditText("future_character")
        self.assertEqual(NodeForm._combo_value(combo, combo.currentText()), "future_character")


if __name__ == "__main__":
    unittest.main()
