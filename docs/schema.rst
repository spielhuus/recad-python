Schema
------

.. toctree::
   :hidden:
   :maxdepth: 2
   :caption: Contents:

   elements

The `recad.Schema` module allows for drawing circuits. The drawing follows the basic structure containing pre-defined elements for use in a drawing. A common import structure is:

.. code-block:: python

   from recad import Schema

.. autoclass:: recad.Schema

.. autofunction:: recad.Schema.new

.. autofunction:: recad.Schema.load

.. autofunction:: recad.Schema.write

.. autofunction:: recad.Schema.plot

Example Usage:

.. exec_code::
   :linenos:

   from recad import *

   schema = Schema("test-project")
   schema.move_to((50.8, 50.8))
   schema = (schema
       + LocalLabel("Vin").rotate(180) 
       + Wire().right()
       + Symbol("R1", "100k", "Device:R").rotate(90)
       + Junction()
       + Symbol("U1", "TL072", "Amplifier_Operational:LM2904").anchor("2").mirror("x")
       + Junction()
       + Wire().up().length(5)
       + Symbol("R2", "100k", "Device:R").rotate(270).tox("U1", "2")
       + Wire().toy("U1", "2")
       + Symbol("GND", "GND", "power:GND").at("U1", "3")
       + LocalLabel("Vout").at("U1", "1")
   )
   
   schema.plot(path = 'docs/_static/schema_opamp.svg', scale = 10)


.. image:: _static/schema_opamp.svg
   :alt: first circuit
   :width: 800


