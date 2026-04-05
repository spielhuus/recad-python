use std::path::Path;

use pyo3::{
    exceptions::PyIOError,
    prelude::*,
    types::{PyDict, PyString},
};
use recad::plot::{Plot, PlotCommand, Plotter, Themes};

use crate::{schema::Schema, PyDRCViolation};

/// The Pcb
#[pyclass]
#[derive(Default)]
pub struct Pcb {
    pub pcb: recad::Pcb,
    pub pos_stack: Vec<(f32, f32)>,
}

#[pymethods]
impl Pcb {
    /// Create a new Pcb
    ///
    /// :param project: the project name
    /// :param library_path: the path to the symbol library.
    #[new]
    #[pyo3(signature = (_project, _library_path = None))]
    fn new(_project: &str, _library_path: Option<Vec<String>>) -> Self {
        todo!("Pcb::new not implemented");
        // Pcb {
        //     schema: recad_core::Pcb::new(project, library_path),
        //     pos_stack: Vec::new(),
        // }
    }

    /// Load a new Pcb from a file.
    ///
    /// :param path: the file path
    #[staticmethod]
    pub fn load(path: &str) -> PyResult<Pcb> {
        if let Ok(s) = recad::Pcb::load(Path::new(path)) {
            Ok(Pcb {
                pcb: s,
                ..Default::default()
            })
        } else {
            Err(PyErr::new::<PyIOError, _>(format!(
                "unable to open PCB file '{}'",
                path
            )))
        }
    }

    /// Plot a PCB
    ///
    /// :param \**kwargs: see below
    ///
    /// :Keyword Arguments:
    ///  * *theme* -- the color theme.
    ///  * *scale* -- Adjusts the size of the final image, considering only the image area without the border.
    ///  * *border* -- draw a border or crop the image.
    #[pyo3(signature = (**kwargs))]
    pub fn plot(&self, py: Python, kwargs: Option<Bound<PyDict>>) -> PyResult<Option<Py<PyAny>>> {
        #[cfg(feature = "svg")]
        {
            let mut path: Option<String> = None;
            let mut theme = None;
            let mut scale = None;
            let mut border = None;
            let pages: Option<Vec<u8>> = None;

            if let Some(kwargs) = kwargs {
                if let Ok(Some(raw_item)) = kwargs.get_item("path") {
                    let item: Result<String, PyErr> = raw_item.extract();
                    if let Ok(item) = item {
                        path = Some(item.to_string());
                    }
                }
                if let Ok(Some(raw_item)) = kwargs.get_item("scale") {
                    let item: Result<f64, PyErr> = raw_item.extract();
                    if let Ok(item) = item {
                        scale = Some(item);
                    }
                }
                if let Ok(Some(raw_item)) = kwargs.get_item("border") {
                    let item: Result<bool, PyErr> = raw_item.extract();
                    if let Ok(item) = item {
                        border = Some(item);
                    }
                }
                if let Ok(Some(raw_item)) = kwargs.get_item("theme") {
                    let item: Result<String, PyErr> = raw_item.extract();
                    if let Ok(item) = item {
                        theme = Some(Themes::from(item));
                    }
                }
            }

            Ok(if let Some(path) = path {
                let mut svg = recad::plot::SvgPlotter::new();
                self.pcb
                    .plot(
                        &mut svg,
                        &PlotCommand::default()
                            .theme(theme)
                            .scale(scale)
                            .border(border)
                            .pages(pages),
                    )
                    .unwrap();
                svg.save(&std::path::PathBuf::from(path)).unwrap();
                None
            } else if crate::is_jupyter(py) {
                spdlog::debug!("render jupyter: {:?}", self.pcb);
                let mut svg = recad::plot::SvgPlotter::new();
                self.pcb
                    .plot(
                        &mut svg,
                        &PlotCommand::default()
                            .theme(theme)
                            .scale(scale)
                            .border(border)
                            .pages(pages),
                    )
                    .unwrap();
                let mut buffer = Vec::new();
                svg.write(&mut buffer).unwrap();

                // Return SvgImage for Jupyter to render
                let svg_data = String::from_utf8(buffer.clone()).unwrap();
                let image = crate::SvgImage { data: svg_data };
                let py_image = Py::new(py, image)?;

                // Try to use IPython.display.display so multiple plots work in one cell
                if let Ok(mod_display) = py.import("IPython.display") {
                    if let Ok(display_func) = mod_display.getattr("display") {
                        let _ = display_func.call1((&py_image,));
                        return Ok(None);
                    }
                }
                Some(py_image.into_any())
            } else if crate::is_neovim() {
                // Neovim logic placeholder
                None
            } else {
                Some(PyString::new(py, "other").into())
            })
        }
        #[cfg(not(feature = "svg"))]
        {
            let _ = (py, kwargs);
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "The 'svg' feature was not enabled at compile time.",
            ))
        }
    }

    /// Run Design Rule Check (DRC) on the PCB and return any violations found.
    pub fn drc(&self, schema: &Schema) -> PyResult<Vec<PyDRCViolation>> {
        let drc_list = recad::reports::drc(&self.pcb, schema.schema_ref());
        let res: Vec<PyDRCViolation> = drc_list.into_iter().map(Into::into).collect();
        Ok(res)
    }
}
