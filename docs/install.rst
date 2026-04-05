Installation
============

The recad package can be installed locally or using the [recad-docker](https://github.com/spielhuus/recad-docker) image.

Install from pypi
-----------------

Ubuntu
^^^^^^

.. code-block:: bash

    apt-get install kicad kicad-symbols kicad-packages3d python3 python3-pip python3-venv


Arch Linux
^^^^^^^^^^

.. code-block:: bash

    pacman -Sy kicad kicad-library kicad-library-3d python python-pip

Install the `recad` package from PyPI

.. code-block:: bash

    python -m venv --system-site-packages .venv
    pip install recad

The `--system-site-packages` option is needed to make recad find the pcbnew packages.

Install the [osifont](https://github.com/hikikomori82/osifont)

.. code-block:: bash

   mkdir -p /usr/local/share/fonts/TT/
   curl -L "https://github.com/hikikomori82/osifont/blob/master/osifont-lgpl3fe.ttf?raw=true" -o /usr/local/share/fonts/TT/osifont-lgpl3fe.ttf


Install from source
-------------------

Ubuntu
^^^^^^

.. code-block:: bash

    apt-get install build-essential git cargo pkg-config libcairo2-dev libpango1.0-dev libngspice0-dev libpoppler-glib-dev libssl-dev libclang-14-dev
    alias python='python3'

Arch Linux
^^^^^^^^^^

.. code-block:: bash
    
    pacman -Sy base-devel git clang python rustup graphite cairo pango ngspice poppler-glib
    rustup default stable

Get and compile the code:

.. code-block:: bash

    git clone https://github.com/spielhuus/recad
    cd recad
    make all

The `make` command will create the executable `recad` in `.venv/bin`.

Example usage
-------------

.. code-block:: bash

    source .venv/bin/activate
    recad plot --input your_schema.kicad_sch --output schema.svg

