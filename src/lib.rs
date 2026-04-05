use pyo3::prelude::*;
use spdlog::{sink::FileSink, LevelFilter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod circuit;
mod pcb;
mod schema;
mod simulation;

/// A wrapper class that implements the Jupyter `_repr_svg_` protocol.
/// When returned at the end of a cell, Jupyter will render the SVG data.
#[pyclass]
struct SvgImage {
    data: String,
}

#[pymethods]
impl SvgImage {
    fn _repr_svg_(&self) -> String {
        self.data.clone()
    }

    fn __repr__(&self) -> String {
        format!("<SvgImage data_len={} bytes>", self.data.len())
    }

    // Debug helper to manually view data in Python
    fn get_data(&self) -> String {
        self.data.clone()
    }
}

/// Check if running specifically in a Jupyter Kernel (ZMQInteractiveShell)
fn is_jupyter(py: Python) -> bool {
    // Method 1: Check for ipykernel in sys.modules (Most reliable for Notebooks/Lab/VSCode)
    if let Ok(sys) = py.import("sys") {
        if let Ok(modules) = sys.getattr("modules") {
            if modules.contains("ipykernel").unwrap_or(false) {
                return true;
            }
        }
    }

    // Method 2: Check for get_ipython in builtins (Fallback for consoles)
    match py.import("builtins") {
        Ok(builtins) => builtins.hasattr("get_ipython").unwrap_or(false),
        Err(_) => false,
    }
}

fn is_neovim() -> bool {
    match std::env::var("LUNGAN") {
        Ok(value) => value == "neovim",
        Err(_) => false,
    }
}

/// recad main function.
#[pyfunction]
pub fn main() -> PyResult<()> {
    Ok(())
}

/// Represents a single line item in a Bill of Materials (BOM).
///
/// This class encapsulates all relevant information about a specific component
/// or part extracted from a schematic design. It includes quantity, reference
/// designators, electrical properties, and supplier information.
///
/// Attributes:
///     amount (int): The total quantity of this component required.
///     references (list[str]): A list of reference designators (e.g., ["R1", "R2"]).
///     value (str): The electrical value of the component (e.g., "10kΩ", "100nF").
///     footprint (str): The PCB footprint package name (e.g., "0805", "SOIC-8").
///     datasheet (str): URL or file path to the component datasheet.
///     description (str): A human-readable description of the component.
///     mouser_nr (str): The manufacturer or supplier part number (e.g., Mouser catalog number).
#[pyclass(name = "BomItem")]
#[derive(Clone)]
pub struct PyBomItem {
    #[pyo3(get)]
    pub amount: usize,
    #[pyo3(get)]
    pub references: Vec<String>,
    #[pyo3(get)]
    pub value: String,
    #[pyo3(get)]
    pub footprint: String,
    #[pyo3(get)]
    pub datasheet: String,
    #[pyo3(get)]
    pub description: String,
    #[pyo3(get)]
    pub mouser_nr: String,
}

impl From<recad::reports::BomItem> for PyBomItem {
    fn from(item: recad::reports::BomItem) -> Self {
        Self {
            amount: item.amount,
            references: item.references,
            value: item.value,
            footprint: item.footprint,
            datasheet: item.datasheet,
            description: item.description,
            mouser_nr: item.mouser_nr,
        }
    }
}

/// Enumeration representing the severity level of an Electrical Rule Check (ERC) violation.
///
/// This enum is used to classify ERC issues by their impact on the design. It allows
/// users to filter, prioritize, or handle violations differently based on whether
/// they are critical errors or non-blocking warnings.
///
/// Values:
///     Error: A critical violation that indicates a definite problem in the schematic.
///         Errors typically prevent successful netlist generation or PCB export and
///         should be resolved before proceeding. Examples: unconnected power pins,
///         shorted nets with different voltages, missing required properties.
///     Warning: A potential issue that may not break functionality but warrants review.
///         Warnings do not block processing but should be evaluated for design quality.
///         Examples: unused input pins, ambiguous net labels, non-standard component values.
#[pyclass(name = "ErcLevel")]
#[derive(Debug, Clone, PartialEq)]
pub enum PyERCLevel {
    Error,
    Warning,
}

impl From<recad::reports::ERCLevel> for PyERCLevel {
    fn from(item: recad::reports::ERCLevel) -> Self {
        match item {
            recad::reports::ERCLevel::Error => PyERCLevel::Error,
            recad::reports::ERCLevel::Warning => PyERCLevel::Warning,
        }
    }
}

/// Represents a single Electrical Rule Check (ERC) violation found during schematic analysis.
///
/// This class encapsulates details about a specific ERC issue, including its severity,
/// description, and location information within the schematic. It is used to report
/// potential design problems such as unconnected pins, conflicting net labels, or
/// power symbol mismatches.
///
/// Attributes:
///     level (ErcLevel): The severity of the violation, either `ErcLevel.Error` or
///         `ErcLevel.Warning`. Errors typically block further processing, while
///         warnings indicate potential issues that should be reviewed.
///     title (str): A concise, human-readable title summarizing the violation
///         (e.g., "Unconnected input pin", "Conflicting net labels").
///     description (str): A detailed explanation of the violation, including
///         context about why it occurred and suggestions for resolution.
///     position (tuple[float, float]): The (x, y) coordinates in schematic space
///         where the violation is located. Useful for highlighting the issue in
///         a graphical viewer.
///     markers (list[tuple[float, float]]): A list of (x, y) coordinate tuples
///         marking additional points of interest related to this violation,
///         such as connected pins or net segments involved in the conflict.
///
/// Example:
///     >>> import recad
///     >>>
///     >>> # Load a schematic and run ERC checks (hypothetical API)
///     >>> schema = recad.Schema.load("design.kicad_sch")
///     >>> violations = schema.run_erc()
///     >>>
///     >>> # Process and display violations
///     >>> for violation in violations:
///     ...     if violation.level == recad.ErcLevel.Error:
///     ...         print(f"❌ ERROR: {violation.title}")
///     ...     else:
///     ...         print(f"⚠️  WARNING: {violation.title}")
///     ...     print(f"   Location: {violation.position}")
///     ...     print(f"   {violation.description}")
///     ...     if violation.markers:
///     ...         print(f"   Related points: {violation.markers}")
#[pyclass(name = "ErcViolation")]
#[derive(Debug, Clone)]
pub struct PyERCViolation {
    #[pyo3(get)]
    pub level: PyERCLevel,
    #[pyo3(get)]
    pub title: String,
    #[pyo3(get)]
    pub description: String,
    #[pyo3(get)]
    pub position: (f64, f64),
    #[pyo3(get)]
    pub markers: Vec<(f64, f64)>,
}

impl From<recad::reports::ERCViolation> for PyERCViolation {
    fn from(item: recad::reports::ERCViolation) -> Self {
        Self {
            level: item.level.into(),
            title: item.title,
            description: item.description,
            position: (item.position.x, item.position.y),
            markers: item.markers.iter().map(|pos| (pos.x, pos.y)).collect(),
        }
    }
}

#[pymethods]
impl PyERCViolation {
    fn __repr__(&self) -> String {
        format!(
            "ErcViolation(level={}, title='{}', description='{}', position=({}, {}), markers={})",
            match self.level {
                PyERCLevel::Error => "Error",
                PyERCLevel::Warning => "Warning",
            },
            self.title,
            self.description,
            self.position.0,
            self.position.1,
            self.markers
                .iter()
                .map(|(x, y)| format!("({}, {})", x, y))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn __str__(&self) -> String {
        format!(
            "[{}] {}: {}\n  Position: ({}, {})\n  Markers: {}",
            match self.level {
                PyERCLevel::Error => "ERROR",
                PyERCLevel::Warning => "WARNING",
            },
            self.title,
            self.description,
            self.position.0,
            self.position.1,
            self.markers
                .iter()
                .map(|(x, y)| format!("({}, {})", x, y))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[pyclass(name = "DrcLevel")]
#[derive(Debug, Clone, PartialEq)]
pub enum PyDRCLevel {
    Error,
    Warning,
}

impl From<recad::reports::DRCLevel> for PyDRCLevel {
    fn from(item: recad::reports::DRCLevel) -> Self {
        match item {
            recad::reports::DRCLevel::Error => PyDRCLevel::Error,
            recad::reports::DRCLevel::Warning => PyDRCLevel::Warning,
        }
    }
}

#[pyclass(name = "DrcViolation")]
#[derive(Debug, Clone)]
pub struct PyDRCViolation {
    #[pyo3(get)]
    pub level: PyDRCLevel,
    #[pyo3(get)]
    pub title: String,
    #[pyo3(get)]
    pub description: String,
    #[pyo3(get)]
    pub position: (f64, f64),
    #[pyo3(get)]
    pub markers: Vec<(f64, f64)>,
}

impl From<recad::reports::DRCViolation> for PyDRCViolation {
    fn from(item: recad::reports::DRCViolation) -> Self {
        spdlog::debug!("DRC Violation: {:?}", item);
        Self {
            level: item.level.into(),
            title: item.title,
            description: item.description,
            position: (item.position.x, item.position.y),
            markers: item.markers.iter().map(|pos| (pos.x, pos.y)).collect(),
        }
    }
}

#[pymethods]
impl PyDRCViolation {
    fn __repr__(&self) -> String {
        format!(
            "DrcViolation(level={}, title='{}', description='{}', position=({}, {}), markers={})",
            match self.level {
                PyDRCLevel::Error => "Error",
                PyDRCLevel::Warning => "Warning",
            },
            self.title,
            self.description,
            self.position.0,
            self.position.1,
            self.markers
                .iter()
                .map(|(x, y)| format!("({}, {})", x, y))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn __str__(&self) -> String {
        format!(
            "[{}] {}: {}\n  Position: ({}, {})\n  Markers: {}",
            match self.level {
                PyDRCLevel::Error => "ERROR",
                PyDRCLevel::Warning => "WARNING",
            },
            self.title,
            self.description,
            self.position.0,
            self.position.1,
            self.markers
                .iter()
                .map(|(x, y)| format!("({}, {})", x, y))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Generate a Bill of Materials (BOM) from a KiCad schematic file.
///
/// This function parses a KiCad schematic file and extracts component information
/// into a structured BOM format. It can optionally group components by their
/// properties to consolidate identical parts.
///
/// Args:
///     input_path (str): Path to the KiCad schematic file (.kicad_sch).
///     group (bool, optional): Whether to group identical components together.
///         Defaults to True. When enabled, components with the same value,
///         footprint, and other properties are merged into a single BOM entry
///         with combined references.
///
/// Returns:
///     tuple: A tuple containing two elements:
///         - list[BomItem]: The primary BOM list (ungrouped or grouped based on parameter).
///         - list[BomItem] | None: Additional grouped BOM data if available, otherwise None.
///
/// Raises:
///     ValueError: If the file extension is not supported or file type cannot be determined.
///     RuntimeError: If the schematic file cannot be loaded or BOM generation fails.
///
/// Example:
///     >>> import your_module
///     >>>
///     >>> # Generate grouped BOM (default)
///     >>> items, grouped = your_module.bom("design.kicad_sch")
///     >>>
///     >>> # Generate ungrouped BOM
///     >>> items, grouped = your_module.bom("design.kicad_sch", group=False)
///     >>>
///     >>> # Access BOM item properties
///     >>> for item in items:
///     ...     print(f"{item.amount}x {item.value} - {item.description}")
///
/// Note:
///     Only .kicad_sch files are currently supported. Other file extensions
///     will raise a ValueError.
#[pyfunction]
#[pyo3(signature = (input, group = true))]
pub fn bom(input: String, group: bool) -> PyResult<(Vec<PyBomItem>, Option<Vec<PyBomItem>>)> {
    use recad::Schema;
    let input_path = Path::new(&input);
    let extension = input_path.extension().and_then(|s| s.to_str());

    match extension {
        Some("kicad_sch") => {
            let schema = Schema::load(input_path, None).map_err(|e| {
                //TODO: handle sheet selection
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to load schema: {}",
                    e
                ))
            })?;

            let (partlist, missing) = recad::reports::bom(&schema, group, None).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to generate BOM: {}",
                    e
                ))
            })?;

            let py_ungrouped: Vec<PyBomItem> = partlist.into_iter().map(Into::into).collect();
            let py_missing: Option<Vec<PyBomItem>> =
                missing.map(|g| g.into_iter().map(Into::into).collect());
            Ok((py_ungrouped, py_missing))
        }
        Some(extension) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "file extension not supported: {}",
            extension
        ))),
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "can not guess file type: {}",
            input
        ))),
    }
}

#[pyfunction]
pub fn plot(input: String, output: String) -> PyResult<()> {
    #[cfg(feature = "svg")]
    {
        use recad::{
            plot::{Plot, PlotCommand, Plotter},
            Pcb, Schema,
        };

        let input_path = Path::new(&input);
        let extension = input_path.extension().and_then(|s| s.to_str());

        match extension {
            Some("kicad_sch") => {
                let schema = Schema::load(input_path, None).unwrap(); //TODO handle sheet selection
                let mut svg = recad::plot::SvgPlotter::new();
                schema
                    .plot(
                        &mut svg,
                        &recad::plot::PlotCommand::new().border(Some(true)),
                    )
                    .unwrap();
                svg.save(Path::new(&output)).unwrap();
            }
            Some("kicad_pcb") => {
                let pcb = Pcb::load(input_path).unwrap();
                let mut svg = recad::plot::SvgPlotter::new();
                pcb.plot(&mut svg, &PlotCommand::new().border(Some(true)))
                    .unwrap();
                svg.save(Path::new(&output)).unwrap();
            }
            Some(extension) => {
                eprintln!("file extension not supported: {}", extension);
            }
            _ => eprintln!("can not guess file type: {}", input),
        }
        Ok(())
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = (input, output);
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "The 'svg' feature was not enabled at compile time.",
        ))
    }
}

use pyo3::types::PyModule;

#[pymodule]
fn recad_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let file_sink = Arc::new(
        FileSink::builder()
            .path(PathBuf::from("recad.log"))
            .truncate(false)
            .build()
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?,
    );

    let logger = Arc::new(
        spdlog::Logger::builder()
            .sink(file_sink)
            .level_filter(LevelFilter::All)
            .build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?,
    );

    logger.set_flush_level_filter(LevelFilter::All);
    spdlog::set_default_logger(logger.clone());
    match spdlog::init_log_crate_proxy() {
        Ok(_) => {
            spdlog::info!("Proxy initialized successfully.");
        }
        Err(e) => {
            // Log this to the file directly using spdlog, so we see the error!
            spdlog::error!(
                "FAILED to init log proxy: {}. `log::info!` will NOT work.",
                e
            );
        }
    }

    log::set_max_level(log::LevelFilter::Debug);

    m.add_function(wrap_pyfunction!(main, m)?)?;
    m.add_function(wrap_pyfunction!(bom, m)?)?;
    m.add_function(wrap_pyfunction!(plot, m)?)?;
    m.add_class::<PyBomItem>()?;
    m.add_class::<PyERCLevel>()?;
    m.add_class::<PyERCViolation>()?;
    m.add_class::<PyDRCLevel>()?;
    m.add_class::<PyDRCViolation>()?;
    m.add_class::<schema::GlobalLabel>()?;
    m.add_class::<schema::Junction>()?;
    m.add_class::<schema::LocalLabel>()?;
    m.add_class::<schema::Schema>()?;
    m.add_class::<schema::Symbol>()?;
    m.add_class::<schema::Wire>()?;
    m.add_class::<schema::R>()?;
    m.add_class::<schema::C>()?;
    m.add_class::<schema::Power>()?;
    m.add_class::<schema::Gnd>()?;
    m.add_class::<schema::Feedback>()?;
    m.add_class::<schema::NoConnect>()?;
    m.add_class::<circuit::Circuit>()?;
    m.add_class::<pcb::Pcb>()?;
    m.add_class::<simulation::PySimulation>()?;
    Ok(())
}
