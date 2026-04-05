use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::collections::HashMap;

use recad::simulation as recad_simulation;

use crate::circuit::Circuit;

#[pyclass(name = "Simulation")]
pub struct PySimulation {
    simulation: recad_simulation::Simulation,
}

#[pymethods]
impl PySimulation {
    #[new]
    pub fn new(circuit: &Circuit) -> Self {
        // We now accept `circuit` by reference and clone its inner data
        // for the new Simulation instance.
        Self {
            simulation: recad::simulation::Simulation::new(circuit.circuit.clone()),
        }
    }

    /// Run the stored commands.
    ///
    /// Returns:
    ///     dict[str, dict[str, list[float]]]: A dictionary of plot results.
    pub fn run(&self) -> PyResult<HashMap<String, HashMap<String, Vec<f64>>>> {
        self.simulation
            .run()
            .map_err(|err| PyValueError::new_err(format!("Failed to run simulation: {}", err)))
    }

    /// Operating Point Analysis.
    ///
    /// Computes the DC operating point of the circuit.
    ///
    /// Returns:
    ///     dict[str, list[float]]: A dictionary of operating point values.
    pub fn op(&mut self) -> PyResult<HashMap<String, Vec<f64>>> {
        self.simulation
            .op()
            .map_err(|err| PyValueError::new_err(format!("Failed to perform OP analysis: {}", err)))
    }

    /// Transient analysis.
    ///
    /// Args:
    ///     step (str): The time step.
    ///     stop (str): The final time.
    ///     start (str): The start time.
    ///
    /// Returns:
    ///     dict[str, list[float]]: A dictionary of transient analysis results.
    ///
    /// Reference in the ngspice Documentation in chapter 15.3.10.
    #[pyo3(text_signature = "(step, stop, start)")]
    pub fn tran(
        &mut self,
        step: &str,
        stop: &str,
        start: &str,
    ) -> PyResult<HashMap<String, Vec<f64>>> {
        self.simulation.tran(step, stop, start).map_err(|err| {
            PyValueError::new_err(format!("Failed to perform transient analysis: {}", err))
        })
    }

    /// Small-Signal AC Analysis.
    ///
    /// Args:
    ///     start_frequency (str): The starting frequency (e.g., "1k").
    ///     stop_frequency (str): The final frequency (e.g., "100Meg").
    ///     number_of_points (int): Number of points.
    ///     variation (str): The sweep variation type, one of ["dec", "oct", "lin"].
    ///
    /// Returns:
    ///     dict[str, list[float]]: A dictionary of AC analysis results.
    ///
    /// Reference in the ngspice Documentation in chapter 15.3.1.
    #[pyo3(text_signature = "(start_frequency, stop_frequency, number_of_points, variation)")]
    pub fn ac(
        &mut self,
        start_frequency: &str,
        stop_frequency: &str,
        number_of_points: u32,
        variation: &str,
    ) -> PyResult<HashMap<String, Vec<f64>>> {
        self.simulation
            .ac(start_frequency, stop_frequency, number_of_points, variation)
            .map_err(|err| PyValueError::new_err(format!("Failed to perform AC analysis: {}", err)))
    }
}
