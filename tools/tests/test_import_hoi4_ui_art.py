"""No proprietary game assets are needed to test the HOI4 UI-art parser."""
import importlib.util
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "hoi4_ui_art", Path(__file__).resolve().parents[1] / "import_hoi4_ui_art.py"
)
art = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(art)


class TestLocalArtImporter(unittest.TestCase):
    def test_nested_paradox_character_blocks(self):
        source = '''
        characters = {
            GER_test = {
                name = GER_test
                portraits = {
                    army = { large = "GFX_portrait_GER_test" }
                    civilian = { large = "GFX_portrait_GER_test_civ" }
                }
            }
            GER_other = { portraits = { army = { large = "GFX_other" } } }
        }
        '''
        root = dict(art.blocks(source))
        self.assertIn("characters", root)
        characters = dict(art.blocks(root["characters"]))
        self.assertEqual(set(characters), {"GER_test", "GER_other"})
        portraits = dict(art.blocks(characters["GER_test"]))["portraits"]
        self.assertEqual(art.field(dict(art.blocks(portraits))["army"], "large"),
                         "GFX_portrait_GER_test")

    def test_sprite_identifiers_and_paths(self):
        source = '''
        spriteType = {
          name = "GFX_infantry_equipment_1_medium"
          texturefile = "gfx/interface/equipment/infantry_equipment_1.dds"
          noOfFrames = 1
        }
        spriteType = {
          name = GFX_basic_machine_tools_medium
          texturefile = "gfx/interface/technologies/basic_machine_tools.dds"
        }
        '''
        found = list(art.sprite_definitions(source))
        self.assertEqual(len(found), 2)
        self.assertEqual(found[0][2], 1)
        self.assertIn("infantry_equipment_1",
                      art.aliases("GFX_infantry_equipment_1_medium"))
        self.assertEqual(art.categories(found[1][1]), ("research",))

    def test_rejects_local_path_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertIsNone(art.source_path(root, "../../private.dds"))
            self.assertIsNone(art.source_path(root, "gfx/../private.dds"))


if __name__ == "__main__":
    unittest.main()
