use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass]
#[derive(Clone)]
pub struct Circuit {
    pub circuit: recad::simulation::Circuit,
}

impl Circuit {
    pub fn from(circuit: recad::simulation::Circuit) -> Self {
        Self { circuit }
    }
}

#[pymethods]
impl Circuit {
    //Add a resistor to the netlist.
    pub fn resistor(&mut self, reference: String, n0: String, n1: String, value: String) {
        self.circuit.resistor(reference, n0, n1, value);
    }

    //Add a capacitor to the netlist.
    pub fn capacitor(&mut self, reference: String, n0: String, n1: String, value: String) {
        self.circuit.capacitor(reference, n0, n1, value);
    }

    //Add a diode to the netlist.
    pub fn diode(&mut self, reference: String, n0: String, n1: String, value: String) {
        self.circuit.diode(reference, n0, n1, value);
    }

    //Add a bjt transistor to the netlist.
    pub fn bjt(&mut self, reference: String, n0: String, n1: String, n2: String, value: String) {
        self.circuit.bjt(reference, n0, n1, n2, value);
    }

    //Add a bjt transistor to the netlist.
    pub fn jfet(&mut self, reference: String, n0: String, n1: String, n2: String, value: String) {
        self.circuit.jfet(reference, n0, n1, n2, value);
    }

    pub fn circuit(&mut self, reference: String, n: Vec<String>, value: String) -> PyResult<()> {
        self.circuit
            .circuit(reference, n, value)
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }

    // TODO mixed up the circuit types
    // pub fn subcircuit(
    //     &mut self,
    //     name: String,
    //     n: Vec<String>,
    //     Circuit,
    // ) -> Result<(), recad_core::Error> {
    //     self.circuit.subcircuit(name, n, circuit)
    // }

    pub fn voltage(&mut self, reference: String, n1: String, n2: String, value: String) {
        self.circuit.voltage(reference, n1, n2, value);
    }

    pub fn option(&mut self, option: String, value: String) {
        self.circuit.option(option, value);
    }

    pub fn control(&mut self, control: String) {
        self.circuit.control(control);
    }

    pub fn save(&self, filename: Option<String>) -> PyResult<()> {
        self.circuit
            .save(filename)
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }

    pub fn set_value(&mut self, reference: &str, value: &str) -> PyResult<()> {
        self.circuit
            .set_value(reference, value)
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }

    /// Returns a string representation of the circuit (for `str(circuit)` / `print(circuit)`).
    pub fn __str__(&self) -> PyResult<String> {
        Ok(self.circuit.to_str(true).unwrap().join("\n"))
    }

    /// Returns an unambiguous representation (for `repr(circuit)` / `print(repr(circuit))`).
    pub fn __repr__(&self) -> PyResult<String> {
        Ok(self.circuit.to_str(true).unwrap().join("\n"))
    }
}
