#!/usr/bin/env python

import io

import matplotlib.pyplot as plt

import recad

# from IPython.display import SVG, display
# from PIL import Image


schema = recad.Schema("")
schema.move_to((50.8, 50.8))
schema = (
    schema
    + recad.LocalLabel("Vin").rotate(180)
    + recad.Wire().right()
    + recad.Symbol("R1", "100k", "Device:R").rotate(90)
    + recad.Junction()
    + recad.Symbol("U1", "TL072", "Amplifier_Operational:LM2904")
    .property("Sim.Pins", "1=5 2=2 3=1 4=4 8=3")
    .anchor("2")
    .mirror("x")
    + recad.Junction()
    + recad.Wire().up().length(4)
    + recad.Symbol("R2", "100k", "Device:R").rotate(270).tox("U1", "2")
    + recad.Wire().toy("U1", "2")
    + recad.Symbol("GND", "GND", "power:GND").at("U1", "3")
    + recad.LocalLabel("Vout").at("U1", "1")
)
schema.move_to((101.6, 50.8))
schema = (
    schema
    + recad.Symbol("+15V", "+15V", "power:+15V")
    + recad.Symbol("U1", "TL072", "Amplifier_Operational:LM2904")
    .unit(3)
    .anchor("8")
    .property("Sim.Pins", "1=1 2=5 3=2 4=4 8=3")
    + recad.Symbol("-15V", "-15V", "power:-15V").at("U1", "4").rotate(180)
)


svg = schema.plot(scale=10)
path = "py_opamp.svg"
schema.write("opamp.kicad_sch")

circuit = schema.circuit("", ["../recad_core/tests/spice"])
circuit.voltage("1", "+15V", "GND", "DC 15V")
circuit.voltage("2", "-15V", "GND", "DC -15V")
circuit.voltage("3", "Vin", "GND", "DC 5V AC 5V SIN(0, 5V, 100)")

sim = recad.Simulation(circuit)
res = sim.tran("100ns", "20ms", "0ms")
keys_list = list(res.keys())

schema.open(scale=10, path=path)

# Plot
t = res["time"]
plt.figure(figsize=(10, 5))
plt.plot(t, res["vin"], label="Vin", alpha=0.7)
plt.plot(t, res["vout"], label="Vout", alpha=0.7)
plt.xlabel("Time [s]")
plt.ylabel("Voltage [V]")
plt.title("Transient Analysis: Op-Amp Response")
plt.grid(True)
plt.legend()
plt.tight_layout()
plt.show()
