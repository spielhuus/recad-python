``recad``
=========

.. _`KiCad`: http://kicad.org [KiCad]

With ``recad``, users can seamlessly craft electronic schematics using Python code, simulate circuits for testing and validation, and export them to `KiCad`_ for further refinement or PCB layout. Moreover, ``recad`` simplifies the production process by generating production files directly from KiCad files. To enhance workflow management, ``recad`` enables users to create notebooks for organizing and documenting their designs, which can be effortlessly transformed into web pages for easy sharing and collaboration.

.. toctree::
   :maxdepth: 2
   :caption: Contents:
   :hidden:

   install
   cli
   schema
   changelog
   api


Features
--------

* Draw schematic diagrams with python code.
* Export diagrams to KiCad format.
* Export diagrams to a spice netlist.
* Run ngspice simulation.
* Plot schema and pcb from KiCad files.
* Output BOM in JSON or Excel file format, usable for import to mouser.
* Run ERC and DRC checks for KiCad files.
* Convert markdown notebooks and execute commands.

Why consider another program when there are already several excellent options available? For instance, schemdraw offers a pleasant interface for drawing schematics, while PySpice facilitates circuit simulation. Additionally, numerous projects support working with KiCad production files. However, despite these offerings, workflow integration often remains cumbersome. Here's where recad steps in. Built around the KiCad data model, recad streamlines the process. Now, the circuit snippet created within recad's notebook can seamlessly transition to simulation and export within KiCad.

Getting started
---------------

1. Install recad software
2. Work with the command-line utility
3. Create Python development projects
4. Generate notebooks for organizing and documenting design processes



.. Indices and tables
.. ==================
..
.. * :ref:`genindex`
.. * :ref:`modindex`
.. * :ref:`search`
..
..
