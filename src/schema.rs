use std::{collections::HashMap, path::Path};

use pyo3::{
    exceptions::{PyIOError, PyRuntimeError, PyTypeError, PyValueError},
    prelude::*,
    types::{PyDict, PyString},
};

use recad::{
    draw::{At, Attribute, Direction, DotPosition, Drawable, Drawer, LabelPosition, SchemaBuilder},
    netlist::{CircuitGraph, Netlist},
    plot::{Plot, PlotCommand, Plotter, Themes},
    Pt, Schema as CoreSchema,
};

use crate::{circuit::Circuit, PyBomItem, PyERCViolation};

macro_rules! extract_at_pt {
    ($instance:expr, $coord:expr) => {
        if let Ok(Some(pos)) = $coord.extract::<Option<(f64, f64)>>() {
            $instance.at = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok($instance);
        }
    };
}

macro_rules! extract_at_pin {
    ($instance:expr, $pin:expr, $reference:expr) => {
        if let Some(pin_bound) = $pin {
            if let (Ok(r), Ok(p)) = (
                $reference.extract::<String>(),
                pin_bound.extract::<String>(),
            ) {
                $instance.at = Some(At::Pin(r, p));
                return Ok($instance);
            }
        }
    };
}

macro_rules! extract_at_junction {
    ($instance:expr, $reference:expr) => {
        if let Ok(junction) = $reference.extract::<Junction>() {
            if let Some(pos) = junction.pos {
                $instance.at = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            }
            return Ok($instance);
        }
    };
}

macro_rules! extract_tox_pin {
    ($instance:expr, $pin:expr, $reference:expr) => {
        if let Some(pin_bound) = $pin {
            if let (Ok(r), Ok(p)) = (
                $reference.extract::<String>(),
                pin_bound.extract::<String>(),
            ) {
                $instance.tox = Some(At::Pin(r, p));
                return Ok($instance);
            }
        }
    };
}

macro_rules! extract_toy_pin {
    ($instance:expr, $pin:expr, $reference:expr) => {
        if let Some(pin_bound) = $pin {
            if let (Ok(r), Ok(p)) = (
                $reference.extract::<String>(),
                pin_bound.extract::<String>(),
            ) {
                $instance.toy = Some(At::Pin(r, p));
                return Ok($instance);
            }
        }
    };
}

macro_rules! extract_return {
    ($reference:expr, $name:expr) => {
        Err(PyTypeError::new_err(format!(
            "Unknown type for '{}': '{}'",
            $name,
            $reference
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "Unknown".to_string())
        )))
    };
}

enum SchemaState {
    /// A loaded, read-only schema. No builder overhead.
    Static(CoreSchema),
    /// An active drawing session using the builder.
    Active(SchemaBuilder),
}

/// The Schema
#[pyclass]
#[derive(Default)]
pub struct Schema {
    state: Option<SchemaState>,
    pub pos_stack: Vec<(f64, f64)>,
}

impl Schema {
    /// Helper to cleanly get a reference to the underlying schema regardless of state
    pub fn schema_ref(&self) -> &CoreSchema {
        match self.state.as_ref().unwrap() {
            SchemaState::Static(s) => s,
            SchemaState::Active(b) => &b.schema,
        }
    }

    /// Helper to ensure the builder is active. Promotes Static -> Active if needed.
    fn ensure_builder(&mut self) -> &mut SchemaBuilder {
        // If it's static, we take it out, wrap it in a builder, and put it back
        if let SchemaState::Static(_) = self.state.as_ref().unwrap() {
            let core_schema = match self.state.take().unwrap() {
                SchemaState::Static(s) => s,
                _ => unreachable!(),
            };

            self.state = Some(SchemaState::Active(SchemaBuilder {
                schema: core_schema,
                last_pos: At::default(),
                grid: 2.54,
            }));
        }

        // Now we safely return the mutable builder
        match self.state.as_mut().unwrap() {
            SchemaState::Active(b) => b,
            _ => unreachable!(),
        }
    }

    /// Helper to trigger the builder's finalize() logic before exporting/saving
    fn finalize_if_active(&mut self) -> PyResult<()> {
        if let SchemaState::Active(b) = self.state.as_mut().unwrap() {
            b.finalize()
                .map_err(|e| PyRuntimeError::new_err(format!("Finalize failed: {}", e)))?;
        }
        Ok(())
    }
}

#[pymethods]
impl Schema {
    /// Create a new Schema
    ///
    /// :param project: the project name
    /// :param library_path: the path to the symbol library.
    #[new]
    #[pyo3(signature = (project, library_path = None))]
    fn new(project: &str, library_path: Option<Vec<String>>) -> Self {
        let mut builder = SchemaBuilder::new(project);
        if let Some(paths) = library_path {
            builder.schema.library_paths =
                paths.into_iter().map(std::path::PathBuf::from).collect();
        }
        Schema {
            state: Some(SchemaState::Active(builder)),
            pos_stack: Vec::new(),
        }
    }

    /// Load a new Schema from a file.
    ///
    /// :param path: the file path
    #[staticmethod]
    pub fn load(path: &str) -> PyResult<Schema> {
        match CoreSchema::load(Path::new(path), None) {
            Ok(s) => Ok(Schema {
                state: Some(SchemaState::Static(s)), // Starts as Static (no builder)
                pos_stack: Vec::new(),
            }),
            Err(_) => Err(PyIOError::new_err(format!(
                "unable to open schema file '{}'",
                path
            ))),
        }
    }

    /// Write a new Schema from to file.
    ///
    /// :param path: the file path
    pub fn write(&mut self, path: &str) -> PyResult<()> {
        self.finalize_if_active()?; // Execute auto-routing/label collision if drawing happened

        let mut writer = std::fs::File::create(path)
            .map_err(|e| PyIOError::new_err(format!("Failed to create file: {}", e)))?;

        self.schema_ref()
            .write(&mut writer)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to write schema: {}", e)))?;
        Ok(())
    }

    /// Plot a schema
    ///
    /// :param \**kwargs: see below
    ///
    /// :Keyword Arguments:
    ///  * *theme* -- the color theme.
    ///  * *scale* -- Adjusts the size of the final image, considering only the image area without the border.
    ///  * *border* -- draw a border or crop the image.
    #[pyo3(signature = (**kwargs))]
    pub fn plot(
        &mut self,
        py: Python,
        kwargs: Option<Bound<PyDict>>,
    ) -> PyResult<Option<Py<PyAny>>> {
        #[cfg(feature = "svg")]
        {
            self.finalize_if_active()?;
            let mut path: Option<String> = None;
            let mut theme = None;
            let mut scale = None;
            let mut border = None;
            let pages: Option<Vec<u8>> = None;

            spdlog::debug!("plot schema to : {:?}", path);

            if let Some(kwargs) = kwargs {
                if let Ok(Some(raw_item)) = kwargs.get_item("path") {
                    if let Ok(item) = raw_item.extract::<String>() {
                        path = Some(item);
                    }
                }
                if let Ok(Some(raw_item)) = kwargs.get_item("scale") {
                    if let Ok(item) = raw_item.extract::<f64>() {
                        scale = Some(item);
                    }
                }
                if let Ok(Some(raw_item)) = kwargs.get_item("border") {
                    if let Ok(item) = raw_item.extract::<bool>() {
                        border = Some(item);
                    }
                }
                if let Ok(Some(raw_item)) = kwargs.get_item("theme") {
                    if let Ok(item) = raw_item.extract::<String>() {
                        theme = Some(Themes::from(item));
                    }
                }
            }

            if let Some(path) = path {
                let mut svg = recad::plot::SvgPlotter::new();
                self.schema_ref()
                    .plot(
                        &mut svg,
                        &PlotCommand::default()
                            .theme(theme)
                            .scale(scale)
                            .border(border)
                            .pages(pages),
                    )
                    .map_err(|e| PyRuntimeError::new_err(format!("Plot failed: {}", e)))?;
                svg.save(&std::path::PathBuf::from(path))
                    .map_err(|e| PyIOError::new_err(format!("Save failed: {}", e)))?;
                Ok(None)
            } else if crate::is_jupyter(py) {
                let mut svg = recad::plot::SvgPlotter::new();
                self.schema_ref()
                    .plot(
                        &mut svg,
                        &PlotCommand::default()
                            .theme(theme)
                            .scale(scale)
                            .border(border)
                            .pages(pages),
                    )
                    .map_err(|e| PyRuntimeError::new_err(format!("Plot failed: {}", e)))?;
                let mut buffer = Vec::new();
                svg.write(&mut buffer)
                    .map_err(|e| PyIOError::new_err(format!("Write failed: {}", e)))?;

                // Return SvgImage for Jupyter to render
                let svg_data = String::from_utf8(buffer)
                    .map_err(|e| PyValueError::new_err(format!("Invalid UTF-8: {}", e)))?;
                let image = crate::SvgImage { data: svg_data };
                let py_image = Py::new(py, image)?;

                // Try to use IPython.display.display so multiple plots work in one cell
                if let Ok(mod_display) = py.import("IPython.display") {
                    if let Ok(display_func) = mod_display.getattr("display") {
                        let _ = display_func.call1((&py_image,));
                        // If we displayed it manually, return None so the cell doesn't double-render
                        return Ok(None);
                    }
                }

                // Fallback: Return the object if IPython.display failed
                Ok(Some(py_image.into_any()))
            } else if crate::is_neovim() {
                // Neovim logic placeholder
                Ok(None)
            } else {
                Ok(Some(PyString::new(py, "other").into()))
            }
        }
        #[cfg(not(feature = "svg"))]
        {
            let _ = (py, kwargs);
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "The 'svg' feature was not enabled at compile time.",
            ))
        }
    }

    pub fn move_to(mut instance: PyRefMut<'_, Self>, item: (f64, f64)) -> PyRefMut<'_, Self> {
        instance.ensure_builder().move_to(At::Pt(Pt {
            x: item.0,
            y: item.1,
        }));
        instance
    }

    #[pyo3(signature = (**kwargs))]
    pub fn open(&self, _py: Python, kwargs: Option<Bound<PyDict>>) -> PyResult<()> {
        #[cfg(feature = "wgpu")]
        {
            let _ = kwargs;
            let event_loop = winit::event_loop::EventLoop::with_user_event()
                .build()
                .map_err(|e| {
                    PyRuntimeError::new_err(format!("Cannot create event loop: {:?}", e))
                })?;

            let mut app = recad::plot::WgpuPlotter::new(&event_loop);
            self.schema_ref()
                .plot(&mut app, &PlotCommand::new().border(Some(true)))
                .map_err(|e| PyRuntimeError::new_err(format!("Plot failed: {}", e)))?;

            event_loop
                .run_app(&mut app)
                .map_err(|e| PyRuntimeError::new_err(format!("Run failed: {:?}", e)))?;
            Ok(())
        }
        #[cfg(not(feature = "wgpu"))]
        {
            let _ = kwargs;
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "The 'wgpu' feature was not enabled at compile time.",
            ))
        }
    }

    /// Draw a element to the Schema.
    ///
    /// Instead of using `draw` on a schema, you can also add
    /// the element using the `+` function.
    pub fn draw<'a>(
        mut instance: PyRefMut<'a, Self>,
        item: &Bound<PyAny>,
    ) -> PyResult<PyRefMut<'a, Self>> {
        let builder = instance.ensure_builder();
        if let Ok(label) = item.extract::<LocalLabel>() {
            builder
                .draw(recad::draw::LocalLabel::from(label))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(label) = item.extract::<GlobalLabel>() {
            builder
                .draw(recad::draw::GlobalLabel::from(label))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(symbol) = item.extract::<Symbol>() {
            builder
                .draw(recad::draw::Symbol::from(symbol))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(r) = item.extract::<R>() {
            builder
                .draw(recad::draw::Symbol::from(r))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(c) = item.extract::<C>() {
            builder
                .draw(recad::draw::Symbol::from(c))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(p) = item.extract::<Power>() {
            builder
                .draw(recad::draw::Symbol::from(p))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(g) = item.extract::<Gnd>() {
            builder
                .draw(recad::draw::Symbol::from(g))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(wire) = item.extract::<Wire>() {
            builder
                .draw(recad::draw::Wire::from(wire))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(nc) = item.extract::<NoConnect>() {
            builder
                .draw(recad::draw::NoConnect::from(nc))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        if let Ok(feedback) = item.extract::<Feedback>() {
            builder
                .draw(recad::draw::Feedback::from(feedback))
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            return Ok(instance);
        }

        // Junction extracts as mutable reference because it modifies python state
        if let Ok(mut py_junction) = item.extract::<PyRefMut<Junction>>() {
            let final_junction = recad::draw::Junction::new();

            if let Some(at) = &py_junction.at {
                builder.move_to(at.clone());
            }

            let result_junction = builder
                .draw(final_junction)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            py_junction.pos = Some((result_junction.pos.x, result_junction.pos.y));

            if py_junction.pushed {
                instance
                    .pos_stack
                    .push((result_junction.pos.x, result_junction.pos.y));
            }
            return Ok(instance);
        }

        let type_name = item
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Cannot draw unsupported type: '{}'. Expected a valid recad component.",
            type_name
        )))
    }

    pub fn pop(&mut self) -> Option<(f64, f64)> {
        self.pos_stack.pop()
    }

    pub fn peek(&mut self) -> Option<(f64, f64)> {
        self.pos_stack.last().copied()
    }

    pub fn next_reference(&mut self, prefix: String) -> String {
        let builder = self.ensure_builder();
        builder.next_reference(&prefix)
    }

    pub fn last_reference(&mut self, prefix: String) -> Option<String> {
        let builder = self.ensure_builder();
        builder.last_reference(&prefix)
    }

    pub fn circuit(&self, name: String, spice: Vec<String>) -> Circuit {
        let netlist = Netlist::from(self.schema_ref());
        let graph = CircuitGraph::from_netlist(netlist, self.schema_ref());
        Circuit::from(graph.to_circuit(name, spice))
    }

    fn __add__<'a>(
        instance: PyRefMut<'a, Self>,
        item: &Bound<PyAny>,
    ) -> PyResult<PyRefMut<'a, Self>> {
        Schema::draw(instance, item)
    }

    /// Generate a Bill of Materials (BOM) from a KiCad schematic file.
    #[pyo3(signature = (group = true))]
    pub fn bom(&self, group: bool) -> PyResult<(Vec<PyBomItem>, Option<Vec<PyBomItem>>)> {
        let (partlist, missing) = recad::reports::bom(self.schema_ref(), group, None)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to generate BOM: {}", e)))?;
        let py_ungrouped: Vec<PyBomItem> = partlist.into_iter().map(Into::into).collect();
        let py_missing: Option<Vec<PyBomItem>> =
            missing.map(|g| g.into_iter().map(Into::into).collect());
        Ok((py_ungrouped, py_missing))
    }

    /// Run Electrical Rule Check (ERC) on the schematic and return any violations found.
    pub fn erc(&self) -> PyResult<Vec<PyERCViolation>> {
        let erc_list = recad::reports::erc(self.schema_ref());
        let res: Vec<PyERCViolation> = erc_list.into_iter().map(Into::into).collect();
        Ok(res)
    }

    fn __str__(&self) -> PyResult<String> {
        Ok(format!(
            "[recad.Schema] Project: {}",
            self.schema_ref().project
        ))
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "[recad.Schema] Project: {}",
            self.schema_ref().project
        ))
    }
}

/// A `GlobalLabel` is a custom identifier that can be assigned to
/// multiple objects or components across the entire design.
#[pyclass]
#[derive(Clone, Default)]
pub struct GlobalLabel {
    pub name: String,
    pub at: Option<At>,
    pub rotate: f64,
}

#[pymethods]
impl GlobalLabel {
    #[new]
    fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }

    pub fn rotate(mut instance: PyRefMut<'_, Self>, angle: f64) -> PyRefMut<'_, Self> {
        instance.rotate = angle;
        instance
    }
}

impl From<GlobalLabel> for recad::draw::GlobalLabel {
    fn from(label: GlobalLabel) -> Self {
        let mut final_label =
            recad::draw::GlobalLabel::new(&label.name).attr(Attribute::Rotate(label.rotate));
        if let Some(at) = label.at {
            final_label = final_label.attr(Attribute::At(at));
        }
        final_label
    }
}

/// A junction represents a connection point where multiple wires
/// or components intersect, allowing electrical current to
/// flow between them.
#[pyclass]
#[derive(Clone, Default, Debug)]
pub struct Junction {
    pub pushed: bool,
    pub at: Option<At>,
    pub pos: Option<(f64, f64)>,
}

#[pymethods]
impl Junction {
    #[new]
    fn new() -> Self {
        Self {
            pushed: false,
            ..Default::default()
        }
    }

    pub fn push(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.pushed = true;
        slf
    }

    /// Place the junction.
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }

    fn __str__(&self) -> PyResult<String> {
        Ok(format!("{:?}", self))
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("{:?}", self))
    }
}

/// A `LocalLabel` refers to an identifier assigned to individual
/// Components or objects within a specific grouping on
/// the same schema page.
#[pyclass]
#[derive(Clone, Default)]
pub struct LocalLabel {
    name: String,
    rotate: f64,
    pub at: Option<At>,
}

#[pymethods]
impl LocalLabel {
    #[new]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            rotate: 0.0,
            ..Default::default()
        }
    }

    /// Rotate the label
    ///
    /// :param angle: rotation angle in degrees
    pub fn rotate(mut instance: PyRefMut<'_, Self>, angle: f64) -> PyRefMut<'_, Self> {
        instance.rotate = angle;
        instance
    }

    /// place the label.
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }
}

impl From<LocalLabel> for recad::draw::LocalLabel {
    fn from(label: LocalLabel) -> Self {
        let mut final_label =
            recad::draw::LocalLabel::new(&label.name).attr(Attribute::Rotate(label.rotate));
        if let Some(at) = label.at {
            final_label = final_label.attr(Attribute::At(at));
        }
        final_label
    }
}

/// A schematic `Symbol` representing an instance from the [`symbols`] library.
#[pyclass]
#[derive(Clone, Default, Debug)]
pub struct Symbol {
    pub reference: String,
    pub value: String,
    pub lib_id: String,
    pub rotate: f64,
    pub anchor: Option<String>,
    pub mirror: Option<String>,
    pub tox: Option<At>,
    pub toy: Option<At>,
    pub at: Option<At>,
    pub unit: Option<u8>,
    pub properties: HashMap<String, String>,
    pub label: Option<LabelPosition>,
}

#[pymethods]
impl Symbol {
    #[new]
    fn new(reference: &str, value: &str, lib_id: &str) -> Self {
        Self {
            reference: reference.to_string(),
            value: value.to_string(),
            lib_id: lib_id.to_string(),
            ..Default::default()
        }
    }

    /// Rotate the symbol
    ///
    /// :param angle: rotation angle in degrees
    pub fn rotate(mut instance: PyRefMut<'_, Self>, angle: f64) -> PyRefMut<'_, Self> {
        instance.rotate = angle;
        instance
    }

    /// Set an anchor Pin.
    ///
    /// :param pin: the anchor pin, can be a string or an integer.
    pub fn anchor<'py>(
        mut instance: PyRefMut<'py, Self>,
        _py: Python,
        pin: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(s) = pin.extract::<String>() {
            instance.anchor = Some(s);
            return Ok(instance);
        }

        if let Ok(i) = pin.extract::<i64>() {
            instance.anchor = Some(i.to_string());
            return Ok(instance);
        }
        extract_return!(pin, "anchor")
    }

    /// Mirror the symbol
    ///
    /// :param axis: the mirror axis ['x', 'y', 'xy']
    pub fn mirror(mut instance: PyRefMut<'_, Self>, axis: String) -> PyRefMut<'_, Self> {
        instance.mirror = Some(axis);
        instance
    }

    /// place the symbol.
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_junction!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }

    /// Set the x coordinate of the symbol.
    #[pyo3(signature = (reference, pin = None))]
    pub fn tox<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = reference.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                instance.tox = Some(At::Pt(Pt { x, y }));
                return Ok(instance);
            }
        }

        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            instance.tox = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(instance);
        }

        extract_tox_pin!(instance, pin, reference);
        extract_return!(reference, "tox")
    }

    /// Set the y coordinate of the symbol.
    #[pyo3(signature = (reference, pin = None))]
    pub fn toy<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = reference.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                instance.toy = Some(At::Pt(Pt { x, y }));
                return Ok(instance);
            }
        }

        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            instance.toy = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(instance);
        }

        extract_toy_pin!(instance, pin, reference);
        extract_return!(reference, "toy")
    }

    /// Select the unit of a symbol
    ///
    /// :param unit: the Symbol unit number.
    pub fn unit(mut instance: PyRefMut<'_, Self>, unit: u8) -> PyRefMut<'_, Self> {
        instance.unit = Some(unit);
        instance
    }

    /// Set a property for the symbol
    ///
    /// :param key: the property key
    /// :param value: the property value
    pub fn property(
        mut instance: PyRefMut<'_, Self>,
        key: String,
        val: String,
    ) -> PyRefMut<'_, Self> {
        instance.properties.insert(key, val);
        instance
    }

    /// Place property, possible values are offset tuple or position by name: north, n, northeast, ne...
    pub fn label<'py>(
        mut instance: PyRefMut<'py, Self>,
        _py: Python,
        pos: &Bound<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(direction) = pos.extract::<String>() {
            let label_pos = direction
                .try_into()
                .map_err(|e| PyValueError::new_err(format!("Invalid label direction: {}", e)))?;
            instance.label = Some(label_pos);
            return Ok(instance);
        }
        if let Ok(offset) = pos.extract::<(f64, f64)>() {
            instance.label = Some(LabelPosition::Offset(offset.0, offset.1));
            return Ok(instance);
        }
        let type_name = pos
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Unknown type for label position: '{}'. Expected string or (float, float) tuple.",
            type_name
        )))
    }
}

impl From<Symbol> for recad::draw::Symbol {
    fn from(sym: Symbol) -> Self {
        // Initialize the core symbol with the required fields
        let mut final_symbol = recad::draw::Symbol::new(&sym.reference, &sym.value, &sym.lib_id);

        // Apply all the attributes using the Drawable builder pattern
        final_symbol = final_symbol.attr(Attribute::Rotate(sym.rotate));

        if let Some(anchor) = sym.anchor {
            final_symbol = final_symbol.attr(Attribute::Anchor(anchor));
        }
        if let Some(mirror) = sym.mirror {
            final_symbol = final_symbol.attr(Attribute::Mirror(mirror));
        }
        if let Some(tox) = sym.tox {
            final_symbol = final_symbol.attr(Attribute::Tox(tox));
        }
        if let Some(toy) = sym.toy {
            final_symbol = final_symbol.attr(Attribute::Toy(toy));
        }
        if let Some(at) = sym.at {
            final_symbol = final_symbol.attr(Attribute::At(at));
        }
        if let Some(unit) = sym.unit {
            final_symbol = final_symbol.attr(Attribute::Unit(unit));
        }
        if let Some(label_pos) = sym.label {
            final_symbol = final_symbol.attr(Attribute::LabelPosition(label_pos));
        }
        if !sym.properties.is_empty() {
            final_symbol = final_symbol.attr(Attribute::Property(sym.properties));
        }

        final_symbol
    }
}

#[pyclass]
#[derive(Clone, Default)]
pub struct Wire {
    pub direction: Direction,
    pub length: f64,
    pub tox: Option<At>,
    pub toy: Option<At>,
    pub atdot: Option<Junction>,
    pub atref: Option<(String, String)>,
    pub pos: Option<(f64, f64)>,
    pub toxref: Option<(String, String)>,
    pub toyref: Option<(String, String)>,
    pub toxdot: Option<String>,
    pub dot: Option<Vec<String>>,
}

/// Wires represent electrical connections between components or points,
/// showing the circuit's interconnections and paths for electric current flow.
#[pymethods]
impl Wire {
    #[new]
    fn new() -> Self {
        Self {
            direction: Direction::Left,
            length: 1.0,
            ..Default::default()
        }
    }

    /// Draw wire to the left.
    pub fn left(mut instance: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        instance.direction = Direction::Left;
        instance
    }

    /// Draw wire to the right.
    pub fn right(mut instance: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        instance.direction = Direction::Right;
        instance
    }

    /// Draw wire upward.
    pub fn up(mut instance: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        instance.direction = Direction::Up;
        instance
    }

    /// Draw a line downwards.
    pub fn down(mut instance: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        instance.direction = Direction::Down;
        instance
    }

    /// The length of the wire
    pub fn length(mut instance: PyRefMut<'_, Self>, length: f64) -> PyRefMut<'_, Self> {
        instance.length = length;
        instance
    }

    ///Draw the line from the position.
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut slf: PyRefMut<'py, Self>,
        _py: Python,
        reference: &'_ Bound<PyAny>,
        pin: Option<&'_ Bound<PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(dot) = reference.extract::<Junction>() {
            slf.atdot = Some(dot);
            return Ok(slf);
        }

        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            slf.pos = Some(pos);
            return Ok(slf);
        }
        if let Some(pin_bound) = pin {
            if let (Ok(r), Ok(p)) = (reference.extract::<String>(), pin_bound.extract::<String>()) {
                slf.atref = Some((r, p));
                return Ok(slf);
            }
        }
        extract_return!(reference, "at")
    }

    ///Draw the line to the X position.
    #[pyo3(signature = (element, pin = None))]
    pub fn tox<'py>(
        mut slf: PyRefMut<'py, Self>,
        _py: Python,
        element: &'_ Bound<PyAny>,
        pin: Option<&'_ Bound<PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = element.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                slf.tox = Some(At::Pt(Pt { x, y }));
                return Ok(slf);
            }
        }
        if let Ok(Some(pos)) = element.extract::<Option<(f64, f64)>>() {
            slf.tox = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(slf);
        }
        if let Some(pin_bound) = pin {
            if let (Ok(r), Ok(p)) = (element.extract::<String>(), pin_bound.extract::<String>()) {
                slf.toxref = Some((r, p));
                return Ok(slf);
            }
        }
        extract_return!(element, "tox")
    }

    ///Draw the line to the Y position.
    #[pyo3(signature = (element, pin = None))]
    pub fn toy<'py>(
        mut slf: PyRefMut<'py, Self>,
        _py: Python,
        element: &'_ Bound<PyAny>,
        pin: Option<&'_ Bound<PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = element.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                slf.toy = Some(At::Pt(Pt { x, y }));
                return Ok(slf);
            }
        }
        if let Ok(Some(pos)) = element.extract::<Option<(f64, f64)>>() {
            slf.toy = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(slf);
        }
        if let Some(pin_bound) = pin {
            if let (Ok(r), Ok(p)) = (element.extract::<String>(), pin_bound.extract::<String>()) {
                slf.toyref = Some((r, p));
                return Ok(slf);
            }
        }
        extract_return!(element, "toy")
    }

    /// Add dots to the wire
    pub fn dot<'py>(
        mut instance: PyRefMut<'py, Self>,
        _py: Python,
        dots: &Bound<'py, PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(vec_dots) = dots.extract::<Vec<String>>() {
            instance.dot = Some(vec_dots);
            return Ok(instance);
        }
        if let Ok(single_dot) = dots.extract::<String>() {
            instance.dot = Some(vec![single_dot]);
            return Ok(instance);
        }
        let type_name = dots
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Unknown type for 'dot': '{}'",
            type_name
        )))
    }
}

impl From<Wire> for recad::draw::Wire {
    fn from(wire: Wire) -> Self {
        let mut final_wire = recad::draw::Wire::new();

        final_wire = match wire.direction {
            Direction::Left => final_wire.attr(Attribute::Direction(Direction::Left)),
            Direction::Right => final_wire.attr(Attribute::Direction(Direction::Right)),
            Direction::Up => final_wire.attr(Attribute::Direction(Direction::Up)),
            Direction::Down => final_wire.attr(Attribute::Direction(Direction::Down)),
        };

        final_wire = final_wire.attr(Attribute::Length(wire.length));

        // Handle At
        if let Some(at) = wire.atdot {
            if let Some((x, y)) = at.pos {
                final_wire = final_wire.attr(Attribute::At(At::Pt(Pt { x, y })));
            }
        } else if let Some((r, p)) = wire.atref {
            final_wire = final_wire.attr(Attribute::At(At::Pin(r, p)));
        } else if let Some((x, y)) = wire.pos {
            final_wire = final_wire.attr(Attribute::At(At::Pt(Pt { x, y })));
        }

        // Handle Tox
        if let Some(tox) = wire.tox {
            final_wire = final_wire.attr(Attribute::Tox(tox));
        } else if let Some((r, p)) = wire.toxref {
            final_wire = final_wire.attr(Attribute::Tox(At::Pin(r, p)));
        }

        // Handle Toy
        if let Some(toy) = wire.toy {
            final_wire = final_wire.attr(Attribute::Toy(toy));
        } else if let Some((r, p)) = wire.toyref {
            final_wire = final_wire.attr(Attribute::Toy(At::Pin(r, p)));
        }

        // Handle Dots
        if let Some(dots) = wire.dot {
            let mut dot_positions = Vec::new();
            for d in dots {
                if d == "start" {
                    dot_positions.push(DotPosition::Start);
                }
                if d == "end" {
                    dot_positions.push(DotPosition::End);
                }
            }
            if !dot_positions.is_empty() {
                final_wire = final_wire.attr(Attribute::Dot(dot_positions));
            }
        }

        final_wire
    }
}

/// A `R` utility function for creating a Resistor
#[pyclass]
#[derive(Clone)]
pub struct R {
    pub reference: String,
    pub resistance: String,
    pub rotate: f64,
    pub label: Option<LabelPosition>,
    pub at: Option<At>,
    pub tox: Option<At>,
    pub toy: Option<At>,
}

#[pymethods]
impl R {
    #[new]
    fn new(reference: String, resistance: String) -> Self {
        Self {
            reference,
            resistance,
            rotate: 0.0,
            label: None,
            at: None,
            tox: None,
            toy: None,
        }
    }

    /// Rotate the Resistor
    pub fn rotate(mut instance: PyRefMut<'_, Self>, angle: f64) -> PyRefMut<'_, Self> {
        instance.rotate = angle;
        instance
    }

    /// Place the Resistor
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            instance.at = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(instance);
        }

        extract_at_junction!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }

    /// Set the x coordinate of the resistor.
    #[pyo3(signature = (reference, pin = None))]
    pub fn tox<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = reference.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                instance.tox = Some(At::Pt(Pt { x, y }));
                return Ok(instance);
            }
        }

        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            instance.tox = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(instance);
        }

        extract_tox_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }

    /// Set the y coordinate of the resistor.
    #[pyo3(signature = (reference, pin = None))]
    pub fn toy<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = reference.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                instance.toy = Some(At::Pt(Pt { x, y }));
                return Ok(instance);
            }
        }

        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            instance.toy = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(instance);
        }

        if let Some(pin_bound) = pin {
            if let (Ok(r), Ok(p)) = (reference.extract::<String>(), pin_bound.extract::<String>()) {
                instance.toy = Some(At::Pin(r, p));
                return Ok(instance);
            }
        }

        extract_return!(reference, "at")
    }

    /// Place property, possible values are offset tuple or position by name: north, n, northeast, ne...
    pub fn label<'py>(
        mut instance: PyRefMut<'py, Self>,
        _py: Python,
        pos: &Bound<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(direction) = pos.extract::<String>() {
            let label_pos = direction
                .try_into()
                .map_err(|e| PyValueError::new_err(format!("Invalid label direction: {}", e)))?;

            instance.label = Some(label_pos);
            return Ok(instance);
        }

        if let Ok(offset) = pos.extract::<(f64, f64)>() {
            instance.label = Some(LabelPosition::Offset(offset.0, offset.1));
            return Ok(instance);
        }

        let type_name = pos
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Unknown type for label position: '{}'. Expected string or (float, float) tuple.",
            type_name
        )))
    }
}

impl From<R> for recad::draw::Symbol {
    fn from(r: R) -> Self {
        let mut sym = recad::draw::Symbol::new(&r.reference, &r.resistance, "Device:R");
        sym = sym.attr(Attribute::Rotate(r.rotate));
        if let Some(at) = r.at {
            sym = sym.attr(Attribute::At(at));
        }
        if let Some(tox) = r.tox {
            sym = sym.attr(Attribute::Tox(tox));
        }
        if let Some(toy) = r.toy {
            sym = sym.attr(Attribute::Toy(toy));
        }
        if let Some(label) = r.label {
            sym = sym.attr(Attribute::LabelPosition(label));
        }
        sym
    }
}

/// A `C` utility function for creating a Capacitor
#[pyclass]
#[derive(Clone)]
pub struct C {
    pub reference: String,
    pub capacitance: String,
    pub rotate: f64,
    pub label: Option<LabelPosition>,
    pub at: Option<At>,
    pub tox: Option<At>,
    pub toy: Option<At>,
}

#[pymethods]
impl C {
    #[new]
    fn new(reference: String, capacitance: String) -> Self {
        Self {
            reference,
            capacitance,
            rotate: 0.0,
            label: None,
            at: None,
            tox: None,
            toy: None,
        }
    }

    /// Rotate the Capacitor
    pub fn rotate(mut instance: PyRefMut<'_, Self>, angle: f64) -> PyRefMut<'_, Self> {
        instance.rotate = angle;
        instance
    }

    /// Place the Capacitor
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_junction!(instance, reference);
        extract_at_pin!(instance, pin, reference);

        if reference.is_none() {
            return Ok(instance);
        }

        let type_name = reference
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Unknown type for 'at': '{}'",
            type_name
        )))
    }

    /// Set the x coordinate of the capacitor.
    #[pyo3(signature = (reference, pin = None))]
    pub fn tox<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = reference.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                instance.tox = Some(At::Pt(Pt { x, y }));
                return Ok(instance);
            }
        }

        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            instance.tox = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(instance);
        }

        extract_tox_pin!(instance, pin, reference);
        extract_return!(reference, "tox")
    }

    /// Set the y coordinate of the capacitor.
    #[pyo3(signature = (reference, pin = None))]
    pub fn toy<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(junction) = reference.extract::<Junction>() {
            if let Some((x, y)) = junction.pos {
                instance.toy = Some(At::Pt(Pt { x, y }));
                return Ok(instance);
            }
        }

        if let Ok(Some(pos)) = reference.extract::<Option<(f64, f64)>>() {
            instance.toy = Some(At::Pt(Pt { x: pos.0, y: pos.1 }));
            return Ok(instance);
        }

        if let Some(pin_bound) = pin {
            if let (Ok(r), Ok(p)) = (reference.extract::<String>(), pin_bound.extract::<String>()) {
                instance.toy = Some(At::Pin(r, p));
                return Ok(instance);
            }
        }

        extract_return!(reference, "toy")
    }

    /// Place property
    pub fn label<'py>(
        mut instance: PyRefMut<'py, Self>,
        _py: Python,
        pos: &Bound<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(name) = pos.extract::<String>() {
            let label_pos = name
                .try_into()
                .map_err(|e| PyValueError::new_err(format!("Invalid label direction: {}", e)))?;
            instance.label = Some(label_pos);
            return Ok(instance);
        }
        if let Ok(offset) = pos.extract::<(f64, f64)>() {
            instance.label = Some(LabelPosition::Offset(offset.0, offset.1));
            return Ok(instance);
        }

        let type_name = pos
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Unknown type for label position: '{}'. Expected string or (float, float) tuple.",
            type_name
        )))
    }
}

impl From<C> for recad::draw::Symbol {
    fn from(c: C) -> Self {
        let mut sym = recad::draw::Symbol::new(&c.reference, &c.capacitance, "Device:C");
        sym = sym.attr(Attribute::Rotate(c.rotate));
        if let Some(at) = c.at {
            sym = sym.attr(Attribute::At(at));
        }
        if let Some(tox) = c.tox {
            sym = sym.attr(Attribute::Tox(tox));
        }
        if let Some(toy) = c.toy {
            sym = sym.attr(Attribute::Toy(toy));
        }
        if let Some(label) = c.label {
            sym = sym.attr(Attribute::LabelPosition(label));
        }
        sym
    }
}

/// A `Power` utility function for creating a power source
#[pyclass]
#[derive(Clone)]
pub struct Power {
    pub voltage: String,
    pub rotate: f64,
    pub at: Option<At>,
}

#[pymethods]
impl Power {
    #[new]
    fn new(voltage: String) -> Self {
        Self {
            voltage,
            rotate: 0.0,
            at: None,
        }
    }

    /// Rotate the Power source
    pub fn rotate(mut instance: PyRefMut<'_, Self>, angle: f64) -> PyRefMut<'_, Self> {
        instance.rotate = angle;
        instance
    }

    /// Place the Power source
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "rotate")
    }
}

impl From<Power> for recad::draw::Symbol {
    fn from(p: Power) -> Self {
        let lib_id = format!("power:{}", p.voltage);
        let mut final_symbol =
            recad::draw::Symbol::new(&format!("#PWR{}", p.voltage), &p.voltage, &lib_id);
        final_symbol = final_symbol.attr(Attribute::Rotate(p.rotate));
        if let Some(at) = p.at {
            final_symbol = final_symbol.attr(Attribute::At(at));
        }
        final_symbol
    }
}

/// A `GND` utility function for creating GND reference
#[pyclass]
#[derive(Clone)]
pub struct Gnd {
    pub rotate: f64,
    pub at: Option<At>,
}

#[pymethods]
impl Gnd {
    #[new]
    fn new() -> Self {
        Self {
            rotate: 0.0,
            at: None,
        }
    }

    /// Rotate the Gnd symbol
    pub fn rotate(mut instance: PyRefMut<'_, Self>, angle: f64) -> PyRefMut<'_, Self> {
        instance.rotate = angle;
        instance
    }

    /// Place the GND symbol
    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }
}

impl From<Gnd> for recad::draw::Symbol {
    fn from(g: Gnd) -> Self {
        let mut final_symbol = recad::draw::Symbol::new("#PWRGND", "GND", "power:GND");
        final_symbol = final_symbol.attr(Attribute::Rotate(g.rotate));
        if let Some(at) = g.at {
            final_symbol = final_symbol.attr(Attribute::At(at));
        }
        final_symbol
    }
}

/// NoConnect, used to satisfy the ERC check.
#[pyclass]
#[derive(Clone, Default)]
pub struct NoConnect {
    pub at: Option<At>,
}

#[pymethods]
impl NoConnect {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    #[pyo3(signature = (reference, pin = None))]
    pub fn at<'py>(
        mut instance: PyRefMut<'py, Self>,
        reference: &Bound<'py, PyAny>,
        pin: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        extract_at_pt!(instance, reference);
        extract_at_pin!(instance, pin, reference);
        extract_return!(reference, "at")
    }
}

impl From<NoConnect> for recad::draw::NoConnect {
    fn from(nc: NoConnect) -> Self {
        let mut final_nc = recad::draw::NoConnect::new();
        if let Some(at) = nc.at {
            final_nc = final_nc.attr(Attribute::At(at));
        }
        final_nc
    }
}

///Feedback
#[pyclass]
#[derive(Debug, Clone, Default)]
pub struct Feedback {
    pub atref: Option<(String, String)>,
    pub toref: Option<(String, String)>,
    pub with: Option<Symbol>,
    pub height: f64,
    pub dot: Option<Vec<String>>,
}

#[pymethods]
impl Feedback {
    #[new]
    pub fn new() -> Self {
        Self {
            atref: None,
            toref: None,
            with: None,
            height: 5.0 * 2.54,
            dot: None,
        }
    }

    pub fn start<'py>(
        mut slf: PyRefMut<'py, Self>,
        _py: Python,
        reference: &'_ Bound<PyAny>,
        pin: Option<&'_ Bound<PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Some(pin_bound) = pin {
            if let (Ok(r), Ok(p)) = (reference.extract::<String>(), pin_bound.extract::<String>()) {
                slf.atref = Some((r, p));
                return Ok(slf);
            }
        }
        extract_return!(reference, "start")
    }

    pub fn end<'py>(
        mut slf: PyRefMut<'py, Self>,
        _py: Python,
        reference: &'_ Bound<PyAny>,
        pin: Option<&'_ Bound<PyAny>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Some(pin_bound) = pin {
            if let (Ok(r), Ok(p)) = (reference.extract::<String>(), pin_bound.extract::<String>()) {
                slf.toref = Some((r, p));
                return Ok(slf);
            }
        }

        extract_return!(reference, "start")
    }

    pub fn height(mut slf: PyRefMut<'_, Self>, h: f64) -> PyRefMut<'_, Self> {
        slf.height = h;
        slf
    }

    ///Draw a dot at the start or end of the line
    pub fn dot<'py>(
        mut slf: PyRefMut<'py, Self>,
        _py: Python,
        position: &'_ Bound<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(dot_vec) = position.extract::<Vec<String>>() {
            slf.dot = Some(dot_vec);
            return Ok(slf);
        }
        if let Ok(dot_str) = position.extract::<String>() {
            slf.dot = Some(vec![dot_str]);
            return Ok(slf);
        }

        let type_name = position
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Unknown type for 'dot': '{}'",
            type_name
        )))
    }

    /// Add a component into the feedback loop (e.g., R, C, Symbol)
    #[pyo3(signature = (component))]
    pub fn component<'py>(
        mut slf: PyRefMut<'py, Self>,
        _py: Python,
        component: &'_ Bound<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if let Ok(sym) = component.extract::<Symbol>() {
            slf.with = Some(sym);
            return Ok(slf);
        }
        if let Ok(r) = component.extract::<R>() {
            let mut sym = Symbol::new(&r.reference, &r.resistance, "Device:R");
            sym.rotate = r.rotate;
            sym.at = r.at.clone();
            sym.tox = r.tox.clone();
            sym.toy = r.toy.clone();
            sym.label = r.label;
            slf.with = Some(sym);
            return Ok(slf);
        }
        if let Ok(c) = component.extract::<C>() {
            let mut sym = Symbol::new(&c.reference, &c.capacitance, "Device:C");
            sym.rotate = c.rotate;
            sym.at = c.at.clone();
            sym.tox = c.tox.clone();
            sym.toy = c.toy.clone();
            sym.label = c.label;
            slf.with = Some(sym);
            return Ok(slf);
        }

        let type_name = component
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        Err(PyTypeError::new_err(format!(
            "Unknown type for 'component': '{}'",
            type_name
        )))
    }
}

impl From<Feedback> for recad::draw::Feedback {
    fn from(f: Feedback) -> Self {
        let mut dots = Vec::new();
        if let Some(d) = &f.dot {
            if d.iter().any(|x| x == "start") {
                dots.push(recad::draw::DotPosition::Start);
            }
            if d.iter().any(|x| x == "end") {
                dots.push(recad::draw::DotPosition::End);
            }
        }

        recad::draw::Feedback {
            atref: f.atref,
            toref: f.toref,
            with: f.with.map(recad::draw::Symbol::from),
            height: f.height,
            dot: if dots.is_empty() { None } else { Some(dots) },
            ..Default::default()
        }
    }
}
