import recad

import unittest

class TestSchemaLoad(unittest.TestCase):
    def test_load_normal(self):
        self.assertTrue(recad.Schema.load("samples/nuco-v/nuco-v.kicad_sch"))
    
    def test_plot_svg(self):
        schema = recad.Schema.load("samples/nuco-v/nuco-v.kicad_sch")
        self.assertTrue(schema.plot(path = "target/nuco-v.svg") == None)

