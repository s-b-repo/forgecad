use crate::session::{PendingAction, export_path, native_save_path};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Key, Layout, Painter, Pos2, Rect, Response, RichText,
    Sense, Shape, Stroke, StrokeKind, Vec2,
};
use forge_core::blocks::ExpandedEntity;
use forge_core::catalog::{self, WorkArea};
use forge_core::dimension::{Dimension, LinearDimension, LinearDimensionKind, RadialDimension};
use forge_core::edit::{SnapOptions, Transform2d};
use forge_core::mesh::{TessellationOptions, TriangleMesh};
use forge_core::{Document, Geometry, Point, PolylineVertex, Solid, SolidKind};
use std::{collections::BTreeSet, path::PathBuf};

const BG: Color32 = Color32::from_rgb(19, 23, 31);
const PANEL: Color32 = Color32::from_rgb(27, 32, 42);
const PANEL_ALT: Color32 = Color32::from_rgb(34, 40, 52);
const BORDER: Color32 = Color32::from_rgb(56, 64, 79);
const TEXT: Color32 = Color32::from_rgb(226, 232, 241);
const MUTED: Color32 = Color32::from_rgb(148, 159, 178);
const ACCENT: Color32 = Color32::from_rgb(105, 190, 255);
const SELECTED: Color32 = Color32::from_rgb(255, 197, 92);

#[path = "building_ui.rs"]
mod building_ui;
#[path = "civil_ui.rs"]
mod civil_ui;
#[path = "drafting_ui.rs"]
mod drafting_ui;
#[path = "layout_ui.rs"]
mod layout_ui;
#[path = "model_ui.rs"]
mod model_ui;
#[path = "review_ui.rs"]
mod review_ui;
#[path = "scene_ui.rs"]
mod scene_ui;
#[path = "terrain_ui.rs"]
mod terrain_ui;
#[path = "workbench_ui.rs"]
mod workbench_ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workspace {
    Draft,
    Model,
    Build,
    Terrain,
    Make,
    Motion,
    Connect,
}

#[derive(Clone, Copy)]
struct WorkbenchInfo {
    workspace: Workspace,
    name: &'static str,
    symbol: &'static str,
    subtitle: &'static str,
    focus: &'static str,
}

const WORKBENCHES: [WorkbenchInfo; 7] = [
    WorkbenchInfo {
        workspace: Workspace::Draft,
        name: "Draft",
        symbol: "2D",
        subtitle: "Drafting & documentation",
        focus: "Plans, details, layers, and annotations",
    },
    WorkbenchInfo {
        workspace: Workspace::Model,
        name: "Model",
        symbol: "3D",
        subtitle: "Product design & 3D modeling",
        focus: "Parametric parts and manufacturing models",
    },
    WorkbenchInfo {
        workspace: Workspace::Build,
        name: "Build",
        symbol: "BIM",
        subtitle: "Architecture & construction",
        focus: "Storeys, walls, slabs, openings and quantities",
    },
    WorkbenchInfo {
        workspace: Workspace::Terrain,
        name: "Terrain",
        symbol: "CIV",
        subtitle: "Civil & infrastructure",
        focus: "Survey points, triangulated surfaces, contours and profiles",
    },
    WorkbenchInfo {
        workspace: Workspace::Make,
        name: "Make",
        symbol: "CAM",
        subtitle: "Manufacturing & simulation",
        focus: "2.5D profile toolpaths, compensation and G-code",
    },
    WorkbenchInfo {
        workspace: Workspace::Motion,
        name: "Motion",
        symbol: "FX",
        subtitle: "Media & entertainment",
        focus: "Primitive scenes, keyframes, timeline and playback",
    },
    WorkbenchInfo {
        workspace: Workspace::Connect,
        name: "Connect",
        symbol: "↗",
        subtitle: "Local project review",
        focus: "Project issues, priorities and resolution status",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftTool {
    Select,
    Line,
    Rectangle,
    Circle,
    Polyline,
    Arc,
    Text,
    Point,
    Dimension,
    RadialDimension,
}

impl DraftTool {
    fn name(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Line => "Line",
            Self::Rectangle => "Rectangle",
            Self::Circle => "Circle",
            Self::Polyline => "Polyline",
            Self::Arc => "Arc",
            Self::Text => "Text",
            Self::Point => "Point",
            Self::Dimension => "Dimension",
            Self::RadialDimension => "Radial dimension",
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            Self::Select => "V",
            Self::Line => "L",
            Self::Rectangle => "R",
            Self::Circle => "C",
            Self::Polyline => "P",
            Self::Arc => "A",
            Self::Text => "T",
            Self::Point => "O",
            Self::Dimension => "D",
            Self::RadialDimension => "B",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragState {
    None,
    Draw,
    Orbit,
    Pan,
}

#[derive(Clone, Copy)]
struct Camera2d {
    pan: Vec2,
    zoom: f32,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl Camera2d {
    fn world_to_screen(self, point: Point, rect: Rect) -> Pos2 {
        rect.center() + self.pan + Vec2::new(point.x as f32, -point.y as f32) * self.zoom
    }

    fn screen_to_world(self, point: Pos2, rect: Rect) -> Point {
        let relative = (point - rect.center() - self.pan) / self.zoom.max(0.001);
        Point::new(relative.x as f64, -relative.y as f64)
    }
}

#[derive(Clone, Copy)]
struct Camera3d {
    pan: Vec2,
    zoom: f32,
    yaw: f32,
    pitch: f32,
}

impl Default for Camera3d {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.9,
            yaw: 0.78,
            pitch: 0.52,
        }
    }
}

impl Camera3d {
    fn project(self, point: [f64; 3], rect: Rect) -> (Pos2, f64) {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let x = point[0] as f32;
        let y = point[1] as f32;
        let z = point[2] as f32;
        let rotated_x = x * cos_yaw - y * sin_yaw;
        let rotated_y = x * sin_yaw + y * cos_yaw;
        let view_y = rotated_y * sin_pitch + z * cos_pitch;
        let depth = z * sin_pitch - rotated_y * cos_pitch;
        let screen = rect.center() + self.pan + Vec2::new(rotated_x, -view_y) * self.zoom;
        (screen, depth as f64)
    }
}

struct PreparedExport {
    path: PathBuf,
    bytes: Vec<u8>,
    losses: Vec<String>,
    replace_existing: bool,
}

pub struct ForgeApp {
    document: Document,
    active_workspace: Workspace,
    selected_product_id: String,
    window_title: String,
    product_search: String,
    show_products: bool,
    draft_tool: DraftTool,
    construction_points: Vec<Point>,
    polyline_closed: bool,
    dimension_kind: LinearDimensionKind,
    dimension_diameter: bool,
    new_block_name: String,
    draft_text: String,
    draft_text_height: f64,
    camera_2d: Camera2d,
    camera_3d: Camera3d,
    drag_state: DragState,
    draft_start: Option<Point>,
    preview_point: Option<Point>,
    selected_entities: BTreeSet<u64>,
    selected_solid: Option<u64>,
    selected_mesh: Option<u64>,
    model_export_format: String,
    model_ui: model_ui::ModelUiState,
    undo_stack: Vec<Document>,
    redo_stack: Vec<Document>,
    show_grid: bool,
    snap_to_grid: bool,
    snap_to_objects: bool,
    fit_requested: bool,
    model_fit_requested: bool,
    selection_translation: [f64; 2],
    selection_pivot: [f64; 2],
    selection_angle: f64,
    selection_scale: f64,
    selection_copy: bool,
    drafting: drafting_ui::DraftingState,
    building_ui: building_ui::BuildingUiState,
    terrain_ui: terrain_ui::TerrainUiState,
    scene_ui: scene_ui::SceneUiState,
    workbench_ui: workbench_ui::WorkbenchUiState,
    review_ui: review_ui::ReviewUiState,
    file_path: String,
    save_destination: String,
    pending_save_path: String,
    status: String,
    dirty: bool,
    status_is_error: bool,
    pending_action: Option<PendingAction>,
    allow_close: bool,
    pending_export: Option<PreparedExport>,
    smoke_frames_remaining: Option<u32>,
}

impl Default for ForgeApp {
    fn default() -> Self {
        Self::starting_at(Workspace::Draft)
    }
}

impl ForgeApp {
    fn starting_at(workspace: Workspace) -> Self {
        let mut app = Self {
            document: Document::default(),
            active_workspace: workspace,
            selected_product_id: default_product_id(workspace).to_owned(),
            window_title: String::new(),
            product_search: String::new(),
            show_products: false,
            draft_tool: DraftTool::Select,
            construction_points: Vec::new(),
            polyline_closed: false,
            dimension_kind: LinearDimensionKind::Horizontal,
            dimension_diameter: false,
            new_block_name: String::new(),
            draft_text: "Text".into(),
            draft_text_height: 10.0,
            camera_2d: Camera2d::default(),
            camera_3d: Camera3d::default(),
            drag_state: DragState::None,
            draft_start: None,
            preview_point: None,
            selected_entities: BTreeSet::new(),
            selected_solid: None,
            selected_mesh: None,
            model_export_format: "stl".to_owned(),
            model_ui: model_ui::ModelUiState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            show_grid: true,
            snap_to_grid: true,
            snap_to_objects: true,
            fit_requested: false,
            model_fit_requested: false,
            selection_translation: [0.0, 0.0],
            selection_pivot: [0.0, 0.0],
            selection_angle: 0.0,
            selection_scale: 1.0,
            selection_copy: false,
            drafting: drafting_ui::DraftingState::default(),
            building_ui: building_ui::BuildingUiState::default(),
            terrain_ui: terrain_ui::TerrainUiState::default(),
            scene_ui: scene_ui::SceneUiState::default(),
            workbench_ui: workbench_ui::WorkbenchUiState::default(),
            review_ui: review_ui::ReviewUiState::default(),
            file_path: "untitled.forge.json".to_owned(),
            save_destination: "untitled.forge.json".to_owned(),
            pending_save_path: "untitled.forge.json".to_owned(),
            status: "Ready · drag to draw, scroll to zoom, middle-drag to pan".to_owned(),
            dirty: false,
            status_is_error: false,
            pending_action: None,
            allow_close: false,
            pending_export: None,
            smoke_frames_remaining: None,
        };
        if workspace != Workspace::Draft {
            app.status = format!("{} workbench selected", workbench(workspace).name);
        }
        app
    }
}

pub fn studio_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.faint_bg_color = PANEL_ALT;
    visuals.extreme_bg_color = BG;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(41, 88, 125);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    visuals.widgets.inactive.bg_fill = PANEL_ALT;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 59, 75);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_fill = Color32::from_rgb(40, 100, 145);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals
}

pub fn launch(start_workspace: Workspace) -> eframe::Result {
    let mut selected_product = None;
    let mut open_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--product" => {
                let Some(id) = args.next() else {
                    eprintln!("--product requires an individual product ID, e.g. autocad");
                    std::process::exit(2);
                };
                let Some(product) = catalog::product(&id) else {
                    eprintln!("Unknown product ID: {id}");
                    std::process::exit(2);
                };
                selected_product = Some(product);
            }
            "--open" => {
                let Some(path) = args.next() else {
                    eprintln!("--open requires a project or exchange file path");
                    std::process::exit(2);
                };
                open_path = Some(PathBuf::from(path));
            }
            "--help" | "-h" => {
                println!(
                    "Forge Craft\nUsage: forge-craft [--product ID] [--open FILE]\n\nIndividual product compatibility targets:"
                );
                for product in catalog::products() {
                    println!(
                        "  {:45} {} [{:?}]",
                        product.id, product.name, product.status
                    );
                }
                return Ok(());
            }
            _ => {
                eprintln!("Unknown argument: {argument}; use --help");
                std::process::exit(2);
            }
        }
    }
    let start_workspace = selected_product
        .map(|product| workspace_for_area(product.area))
        .unwrap_or(start_workspace);
    let title = selected_product
        .map(|product| format!("{} compatibility target · Forge Craft", product.name))
        .unwrap_or_else(|| format!("Forge Craft · {}", workbench(start_workspace).name));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([1050.0, 680.0]),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(studio_visuals());
            let mut app = ForgeApp::starting_at(start_workspace);
            if let Some(product) = selected_product {
                app.selected_product_id = product.id.clone();
            }
            if let Some(path) = open_path {
                app.perform_open_document(path);
            }
            app.smoke_frames_remaining = std::env::var("FORGE_SMOKE_FRAMES")
                .ok()
                .and_then(|value| value.parse::<u32>().ok());
            if app.smoke_frames_remaining.is_some() && app.status_is_error {
                return Err(std::io::Error::other(app.status.clone()).into());
            }
            Ok(Box::new(app))
        }),
    )
}

impl eframe::App for ForgeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(product) = catalog::product(&self.selected_product_id) {
            let title = format!("{} compatibility target · Forge Craft", product.name);
            if self.window_title != title {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
                self.window_title = title;
            }
        }
        if ctx.input(|input| input.viewport().close_requested()) && self.dirty && !self.allow_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending_action.is_none() {
                self.pending_save_path = self.save_destination.clone();
                self.pending_action = Some(PendingAction::Close);
            }
        }
        if self.pending_action.is_none() && self.pending_export.is_none() {
            self.handle_shortcuts(&ctx);
        } else {
            ui.disable();
        }
        self.top_bar(ui);
        self.ribbon(ui);
        self.status_bar(ui);
        if self.active_workspace == Workspace::Draft {
            self.command_bar(ui);
        }
        self.workbench_rail(ui);
        self.tool_panel(ui);
        self.inspector_panel(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(0.0))
            .show(ui, |ui| match self.active_workspace {
                Workspace::Draft => self.drafting_canvas(ui),
                Workspace::Model => self.model_canvas(ui),
                Workspace::Build | Workspace::Terrain | Workspace::Motion | Workspace::Make => {
                    self.workbench_canvas(ui)
                }
                Workspace::Connect => self.review_canvas(ui),
            });
        self.unsaved_changes_dialog(&ctx);
        self.product_selector(&ctx);
        self.export_confirmation(&ctx);
        if let Some(remaining) = &mut self.smoke_frames_remaining {
            if *remaining == 0 {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                *remaining -= 1;
                ctx.request_repaint();
            }
        }
    }
}

impl ForgeApp {
    fn product_selector(&mut self, ctx: &egui::Context) {
        if !self.show_products {
            return;
        }
        let mut open = self.show_products;
        let mut chosen = None;
        egui::Window::new("Individual product targets")
            .open(&mut open)
            .default_size([660.0, 600.0])
            .show(ctx, |ui| {
                ui.label("Each product remains a distinct compatibility target. Status describes implementation progress.");
                ui.add(egui::TextEdit::singleline(&mut self.product_search).hint_text("Search AutoCAD, Revit, Maya, Civil 3D…").desired_width(f32::INFINITY));
                let results = catalog::search(&self.product_search);
                ui.label(format!("{} of {} targets", results.len(), catalog::products().len()));
                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    for product in results {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(self.selected_product_id == product.id, &product.name).clicked() {
                                chosen = Some(product.id.clone());
                            }
                            ui.label(RichText::new(format!("{:?} · {:?} · {:?}", product.status, product.kind, product.lifecycle)).size(9.0).color(MUTED));
                        });
                    }
                });
            });
        self.show_products = open;
        if let Some(id) = chosen {
            if let Some(product) = catalog::product(&id) {
                self.switch_workspace(workspace_for_area(product.area));
                self.selected_product_id = id;
                self.status = format!(
                    "{} compatibility target · {:?}",
                    product.name, product.status
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "{} compatibility target · Forge Craft",
                    product.name
                )));
                self.show_products = false;
            }
        }
    }

    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("main_menu")
            .exact_size(48.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(23, 28, 37)))
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(11.0);
                    ui.label(RichText::new("F").strong().size(18.0).color(ACCENT));
                    ui.label(RichText::new("FORGE CRAFT").strong().size(13.0).color(TEXT));
                    ui.add_space(11.0);
                    ui.separator();
                    if ui.small_button("Products").clicked() {
                        self.show_products = true;
                    }
                    if let Some(product) = catalog::product(&self.selected_product_id) {
                        ui.label(
                            RichText::new(format!("{} · {:?}", product.name, product.status))
                                .size(10.0)
                                .color(ACCENT),
                        );
                    }
                    if ui.small_button("New").clicked() {
                        self.request_action(PendingAction::New);
                    }
                    if ui.small_button("Open").clicked() {
                        self.open_document();
                    }
                    if ui.small_button("Save").clicked() {
                        self.save_document();
                    }
                    if ui.small_button("Export").clicked() {
                        self.export_current();
                    }
                    ui.separator();
                    ui.label(RichText::new(self.document.name.as_str()).color(TEXT));
                    if self.dirty {
                        ui.label(RichText::new("●").color(SELECTED));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("LOCAL PROJECT · NO ACCOUNT REQUIRED")
                                .size(9.0)
                                .color(MUTED),
                        );
                        ui.add_space(14.0);
                    });
                });
            });
    }

    fn ribbon(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("ribbon")
            .exact_size(76.0)
            .frame(egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, BORDER)))
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    if self.active_workspace == Workspace::Draft {
                        self.ribbon_tool(ui, DraftTool::Select, "V", "Select");
                        ui.separator();
                        self.ribbon_tool(ui, DraftTool::Line, "/", "Line");
                        self.ribbon_tool(ui, DraftTool::Rectangle, "[ ]", "Rectangle");
                        self.ribbon_tool(ui, DraftTool::Circle, "( )", "Circle");
                        ui.separator();
                        if ui.toggle_value(&mut self.show_grid, "▦  Grid").changed() {
                            self.status = if self.show_grid { "Grid shown" } else { "Grid hidden" }.to_owned();
                        }
                        if ui.toggle_value(&mut self.snap_to_grid, "⌗  Snap").changed() {
                            self.status = if self.snap_to_grid { "Grid snap enabled" } else { "Grid snap disabled" }.to_owned();
                        }
                        if ui.button("Fit view").clicked() {
                            self.fit_requested = true;
                        }
                        ui.separator();
                        if ui.button("↶  Undo").clicked() {
                            self.undo();
                        }
                        if ui.button("↷  Redo").clicked() {
                            self.redo();
                        }
                    } else if self.active_workspace == Workspace::Model {
                        if ui.button("Fit model").clicked() {
                            self.model_fit_requested = true;
                        }
                        ui.vertical(|ui| {
                            if ui.button("▣  Box").clicked() {
                                self.add_solid(SolidKind::Box);
                            }
                            ui.label(RichText::new("Primitive").size(9.0).color(MUTED));
                        });
                        ui.vertical(|ui| {
                            if ui.button("◉  Cylinder").clicked() {
                                self.add_solid(SolidKind::Cylinder);
                            }
                            ui.label(RichText::new("Primitive").size(9.0).color(MUTED));
                        });
                        ui.vertical(|ui| {
                            if ui.button("●  Sphere").clicked() {
                                self.add_solid(SolidKind::Sphere);
                            }
                            ui.label(RichText::new("Primitive").size(9.0).color(MUTED));
                        });
                        ui.separator();
                        if ui.button("↶  Undo").clicked() {
                            self.undo();
                        }
                        if ui.button("↷  Redo").clicked() {
                            self.redo();
                        }
                        ui.separator();
                        ui.label(RichText::new("Right-drag to orbit · scroll to zoom").size(11.0).color(MUTED));
                    } else if self.active_workspace != Workspace::Connect {
                        if ui.button("Fit view").clicked() { self.model_fit_requested = true; }
                        if ui.button("Undo").clicked() { self.undo(); }
                        if ui.button("Redo").clicked() { self.redo(); }
                        ui.label(RichText::new("Right-drag to orbit · middle-drag to pan · scroll to zoom").size(11.0).color(MUTED));
                    } else {
                        if ui.button("Undo").clicked() { self.undo(); }
                        if ui.button("Redo").clicked() { self.redo(); }
                        ui.label("Local project review · issues and resolution status");
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        if ui.small_button("?  Shortcuts").clicked() {
                            self.status = "V select · L line · R rectangle · C circle · Ctrl+S save · Ctrl+Z undo".to_owned();
                        }
                    });
                });
            });
    }

    fn ribbon_tool(&mut self, ui: &mut egui::Ui, tool: DraftTool, symbol: &str, label: &str) {
        let active = self.draft_tool == tool;
        let text = format!("{symbol}  {label}\n{}", tool.shortcut());
        if ui
            .add_sized(
                [78.0, 56.0],
                egui::Button::new(RichText::new(text).size(11.0)).selected(active),
            )
            .clicked()
        {
            self.set_draft_tool(tool);
            self.status = format!("{} tool selected", tool.name());
        }
    }

    fn status_bar(&self, root: &mut egui::Ui) {
        let inventory = match self.active_workspace {
            Workspace::Build => format!(
                "{} walls · {} slabs · {} openings",
                self.document.building.walls.len(),
                self.document.building.slabs.len(),
                self.document.building.openings.len()
            ),
            Workspace::Terrain => format!(
                "{} points · {} terrain faces",
                self.document.terrain.points.len(),
                self.document.terrain.triangles.len()
            ),
            Workspace::Motion => format!(
                "{} scene objects · {:.2} s",
                self.document.scene.objects.len(),
                self.scene_ui.time
            ),
            Workspace::Make => format!(
                "{} machining profiles",
                self.document.manufacturing.profiles.len()
            ),
            Workspace::Connect => format!("{} review issues", self.document.review.items.len()),
            Workspace::Model => format!(
                "{} primitives · {} extrusions · {} meshes",
                self.document.solids.len(),
                self.document.extrusions.len(),
                self.document.meshes.len()
            ),
            _ => format!(
                "{} entities · {} solids",
                self.document.entities.len(),
                self.document.solids.len()
            ),
        };
        egui::Panel::bottom("status_bar")
            .exact_size(28.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(24, 29, 38)))
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(if self.status_is_error { "!" } else { "●" })
                            .color(if self.status_is_error {
                                Color32::LIGHT_RED
                            } else {
                                ACCENT
                            })
                            .size(10.0),
                    );
                    ui.label(RichText::new(&self.status).size(10.0).color(
                        if self.status_is_error {
                            Color32::LIGHT_RED
                        } else {
                            MUTED
                        },
                    ));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(11.0);
                        ui.label(RichText::new(inventory).size(10.0).color(MUTED));
                        ui.add_space(12.0);
                        if self.active_workspace == Workspace::Draft {
                            ui.label(
                                RichText::new(format!("{:>3.0}%", self.camera_2d.zoom * 100.0))
                                    .size(10.0)
                                    .color(MUTED),
                            );
                        } else {
                            ui.label(
                                RichText::new(format!("{:>3.0}%", self.camera_3d.zoom * 52.0))
                                    .size(10.0)
                                    .color(MUTED),
                            );
                        }
                    });
                });
            });
    }

    fn workbench_rail(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("workbench_rail")
            .exact_size(82.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(22, 27, 36))
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(root, |ui| {
                ui.add_space(10.0);
                for info in WORKBENCHES {
                    let selected = self.active_workspace == info.workspace;
                    let label = RichText::new(format!("{}\n{}", info.symbol, info.name))
                        .size(10.0)
                        .color(if selected { ACCENT } else { MUTED });
                    let response = ui
                        .add_sized(
                            Vec2::new(74.0, 48.0),
                            egui::Button::new(label).selected(selected),
                        )
                        .on_hover_text(format!("{}\n{}", info.subtitle, info.focus));
                    if response.clicked() {
                        self.switch_workspace(info.workspace);
                    }
                    ui.add_space(4.0);
                }
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("v0.1 · Rust").size(8.0).color(MUTED));
                });
            });
    }

    fn tool_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("tool_panel")
            .default_size(206.0)
            .size_range(176.0..=270.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.active_workspace {
                    Workspace::Draft => self.draft_tools(ui),
                    Workspace::Model => self.model_browser(ui),
                    Workspace::Build => self.building_tools(ui),
                    Workspace::Terrain => self.terrain_tools(ui),
                    Workspace::Motion => self.scene_tools(ui),
                    Workspace::Make => self.cam_tools(ui),
                    Workspace::Connect => self.review_tools(ui),
                });
            });
    }

    fn draft_tools(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "DRAFT TOOLS", "Local geometry");
        ui.add_space(8.0);
        for (tool, symbol) in [
            (DraftTool::Select, "V"),
            (DraftTool::Line, "/"),
            (DraftTool::Rectangle, "[ ]"),
            (DraftTool::Circle, "( )"),
            (DraftTool::Polyline, "PL"),
            (DraftTool::Arc, "ARC"),
            (DraftTool::Text, "T"),
            (DraftTool::Point, "+"),
            (DraftTool::Dimension, "DIM"),
            (DraftTool::RadialDimension, "RAD"),
        ] {
            let active = self.draft_tool == tool;
            let label = format!("{symbol}   {}", tool.name());
            if ui
                .add(
                    egui::Button::new(label)
                        .selected(active)
                        .min_size(Vec2::new(ui.available_width(), 30.0)),
                )
                .clicked()
            {
                self.set_draft_tool(tool);
            }
        }
        if self.draft_tool == DraftTool::Polyline {
            ui.checkbox(&mut self.polyline_closed, "Closed polyline");
            if ui.button("Finish polyline · Enter").clicked() {
                self.finish_polyline();
            }
        }
        if self.draft_tool == DraftTool::Text {
            ui.add(egui::TextEdit::singleline(&mut self.draft_text).hint_text("Text contents"));
            ui.add(
                egui::DragValue::new(&mut self.draft_text_height)
                    .speed(1.0)
                    .range(0.01..=1e9)
                    .prefix("Height: "),
            );
        }
        if self.draft_tool == DraftTool::Arc {
            ui.label(
                RichText::new("Click center, start, then end angle. Arc runs counterclockwise.")
                    .size(10.0)
                    .color(MUTED),
            );
        }
        if self.draft_tool == DraftTool::Dimension {
            ui.label(RichText::new("LINEAR TYPE").size(9.0).strong().color(MUTED));
            for (kind, label) in [
                (LinearDimensionKind::Horizontal, "Horizontal"),
                (LinearDimensionKind::Vertical, "Vertical"),
                (LinearDimensionKind::Aligned, "Aligned"),
            ] {
                ui.selectable_value(&mut self.dimension_kind, kind, label);
            }
            ui.label(
                RichText::new("Click two reference points, then a point on the dimension line.")
                    .size(10.0)
                    .color(MUTED),
            );
        }
        if self.draft_tool == DraftTool::RadialDimension {
            ui.checkbox(&mut self.dimension_diameter, "Diameter dimension");
            ui.label(
                RichText::new("Click the circle center, then a point on the circle.")
                    .size(10.0)
                    .color(MUTED),
            );
        }
        ui.add_space(15.0);
        ui.separator();
        ui.add_space(8.0);
        section_header(ui, "DRAWING AIDS", "View controls");
        ui.checkbox(&mut self.show_grid, "Show grid");
        ui.checkbox(&mut self.snap_to_grid, "Snap to 10 units");
        ui.checkbox(&mut self.snap_to_objects, "Object snaps");
        if ui.button("Fit drawing  F").clicked() {
            self.fit_requested = true;
        }
        ui.add_space(10.0);
        ui.label(RichText::new("NAVIGATION").size(9.0).strong().color(MUTED));
        ui.label(
            RichText::new(
                "Scroll   Zoom around cursor\nMiddle-drag   Pan view\nClick-drag   Draw shape",
            )
            .size(10.0)
            .color(MUTED),
        );
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);
        section_header(ui, "DOCUMENT", "Current file");
        ui.label(
            RichText::new(self.document.name.as_str())
                .size(11.0)
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.small_button("Undo").clicked() {
                self.undo();
            }
            if ui.small_button("Redo").clicked() {
                self.redo();
            }
        });
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(
                RichText::new("Native files stay on this device.\nOpen-source MIT license.")
                    .size(9.0)
                    .color(MUTED),
            );
        });
    }

    fn model_browser(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "MODEL TREE", "Parametric solids · early access");
        self.extrusion_tools(ui);
        ui.add_space(9.0);
        if !self.document.has_model_geometry() {
            ui.label(
                RichText::new("This design is empty.")
                    .size(11.0)
                    .color(MUTED),
            );
            ui.label(
                RichText::new("Add a primitive from the toolbar to get started.")
                    .size(10.0)
                    .color(MUTED),
            );
        } else {
            let solids = self.document.solids.clone();
            for solid in solids {
                let selected = self.selected_solid == Some(solid.id);
                if ui
                    .selectable_label(selected, format!("▣  {}", solid.name))
                    .clicked()
                {
                    self.selected_solid = Some(solid.id);
                    self.selected_mesh = None;
                }
                ui.indent(solid.id.to_string(), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{:?} · {} × {} × {}",
                            solid.kind, solid.size[0], solid.size[1], solid.size[2]
                        ))
                        .size(9.0)
                        .color(MUTED),
                    );
                });
            }
        }
        for object in &self.document.meshes {
            if ui
                .selectable_label(
                    self.selected_mesh == Some(object.id),
                    format!("△  {}", object.name),
                )
                .clicked()
            {
                self.selected_mesh = Some(object.id);
                self.selected_solid = None;
            }
            ui.label(
                RichText::new(format!(
                    "{} vertices · {} triangles",
                    object.mesh.vertices.len(),
                    object.mesh.triangles.len()
                ))
                .size(9.0)
                .color(MUTED),
            );
        }
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(10.0);
        section_header(ui, "TOOLS", "Modeling actions");
        if ui.button("＋  Add box").clicked() {
            self.add_solid(SolidKind::Box);
        }
        if ui.button("＋  Add cylinder").clicked() {
            self.add_solid(SolidKind::Cylinder);
        }
        if ui.button("＋  Add sphere").clicked() {
            self.add_solid(SolidKind::Sphere);
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new("Mouse controls")
                .size(10.0)
                .strong()
                .color(MUTED),
        );
        ui.label(
            RichText::new(
                "Right-drag orbit · middle-drag pan\nScroll to zoom · click a solid to select",
            )
            .size(10.0)
            .color(MUTED),
        );
    }

    fn inspector_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .default_size(260.0)
            .size_range(210.0..=340.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.active_workspace {
                    Workspace::Draft => self.draft_inspector(ui),
                    Workspace::Model => self.model_inspector(ui),
                    Workspace::Build | Workspace::Terrain | Workspace::Motion | Workspace::Make => {
                        match self.active_workspace {
                            Workspace::Build => self.building_inspector(ui),
                            Workspace::Terrain => self.terrain_inspector(ui),
                            Workspace::Motion => self.scene_inspector(ui),
                            Workspace::Make => self.cam_inspector(ui),
                            _ => unreachable!(),
                        }
                        self.project_controls(ui);
                        self.workbench_export_controls(ui);
                    }
                    Workspace::Connect => {
                        self.review_inspector(ui);
                        self.project_controls(ui);
                        self.workbench_export_controls(ui);
                    }
                });
            });
    }

    fn draft_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "INSPECTOR", "Selection & layers");
        ui.add_space(12.0);
        if let Some(id) = self.selected_entities.iter().next().copied() {
            if let Some(entity) = self.document.entities.iter().find(|entity| entity.id == id) {
                ui.label(
                    RichText::new(entity_type(&entity.geometry))
                        .size(12.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("Object  #{id}"))
                        .size(10.0)
                        .color(MUTED),
                );
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(7.0);
                let mut explode_available = false;
                match &entity.geometry {
                    Geometry::Insert {
                        block_name,
                        position,
                        scale,
                        rotation_deg,
                    } => {
                        explode_available = true;
                        ui.label(format!("Block  {block_name}"));
                        ui.label(format!("Insertion  {:.2}, {:.2}", position.x, position.y));
                        ui.label(format!("Scale  {scale:.3} · Rotation  {rotation_deg:.2}°"));
                        let count = self
                            .document
                            .block(block_name)
                            .map(|block| block.entities.len())
                            .unwrap_or(0);
                        ui.label(
                            RichText::new(format!("Definition holds {count} entities"))
                                .size(10.0)
                                .color(MUTED),
                        );
                    }
                    Geometry::Dimension { dimension } => {
                        ui.label(format!(
                            "Measurement  {}",
                            dimension
                                .text()
                                .unwrap_or_else(|_| "unavailable".to_owned())
                        ));
                        ui.label(
                            RichText::new(
                                "Semantic dimension · annotation is derived when displayed",
                            )
                            .size(10.0)
                            .color(MUTED),
                        );
                    }
                    _ => {}
                }
                ui.label(RichText::new("Layer").size(10.0).color(MUTED));
                let layer_name = self
                    .document
                    .layer(entity.layer_id)
                    .map(|layer| layer.name.as_str())
                    .unwrap_or("0");
                ui.label(layer_name);
                ui.add_space(8.0);
                if explode_available && ui.button("Explode reference").clicked() {
                    self.explode_selected_references();
                }
                if ui.button("Delete selected  ⌫").clicked() {
                    self.delete_selected_entities();
                }
            }
        } else {
            ui.label(RichText::new("Nothing selected").size(11.0).color(MUTED));
            ui.label(
                RichText::new("Choose an object in the canvas to inspect it.")
                    .size(10.0)
                    .color(MUTED),
            );
        }
        if !self.selected_entities.is_empty() {
            self.selection_controls(ui);
        }
        ui.add_space(17.0);
        ui.separator();
        ui.add_space(9.0);
        ui.horizontal(|ui| {
            section_header(ui, "LAYERS", "Visibility");
            if ui.small_button("＋").on_hover_text("Add a layer").clicked() {
                let number = self.document.layers.len();
                self.record_change();
                self.document
                    .add_layer(format!("Layer {}", number), [112, 190, 255]);
                self.mark_dirty("Layer added");
            }
        });
        let layers = self.document.layers.clone();
        for layer in layers {
            let is_active = self.document.active_layer_id == layer.id;
            ui.horizontal(|ui| {
                let mut visible = layer.visible;
                if ui.checkbox(&mut visible, "").changed() {
                    self.record_change();
                    if let Some(target) = self
                        .document
                        .layers
                        .iter_mut()
                        .find(|target| target.id == layer.id)
                    {
                        target.visible = visible;
                    }
                    self.mark_dirty("Layer visibility changed");
                }
                let swatch = Color32::from_rgb(layer.color[0], layer.color[1], layer.color[2]);
                let (swatch_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter()
                    .circle_filled(swatch_rect.center(), 4.0, swatch);
                let response = ui.selectable_label(is_active, layer.name.clone());
                if response.clicked() {
                    self.document.active_layer_id = layer.id;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let count = self
                        .document
                        .entities
                        .iter()
                        .filter(|entity| entity.layer_id == layer.id)
                        .count();
                    ui.label(RichText::new(count.to_string()).size(9.0).color(MUTED));
                });
            });
        }
        ui.add_space(15.0);
        ui.separator();
        ui.add_space(9.0);
        section_header(ui, "PROJECT FILE", "Local storage");
        ui.add(
            egui::TextEdit::singleline(&mut self.file_path)
                .desired_width(f32::INFINITY)
                .hint_text("design.forge.json"),
        );
        ui.horizontal(|ui| {
            if ui.small_button("Save").clicked() {
                self.save_document();
            }
            if ui.small_button("Open").clicked() {
                self.open_document();
            }
            if ui.small_button("DXF").clicked() {
                self.export_current();
            }
        });
        self.plot_controls(ui);
    }

    fn selection_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new(format!("{} selected", self.selected_entities.len())).strong());
        let length: f64 = self
            .document
            .expanded_entities()
            .unwrap_or_default()
            .iter()
            .filter(|entity| self.selected_entities.contains(&entity.source_id))
            .map(|entity| entity.geometry.length())
            .sum();
        ui.label(format!("Curve length: {length:.3}"));
        ui.checkbox(&mut self.selection_copy, "Create a copy");
        ui.horizontal(|ui| {
            ui.label("ΔX");
            ui.add(egui::DragValue::new(&mut self.selection_translation[0]).speed(1.0));
            ui.label("ΔY");
            ui.add(egui::DragValue::new(&mut self.selection_translation[1]).speed(1.0));
        });
        if ui.button("Move / copy").clicked() {
            self.apply_selection_transform(Transform2d::translation(
                self.selection_translation[0],
                self.selection_translation[1],
            ));
        }
        ui.horizontal(|ui| {
            ui.label("Pivot X");
            ui.add(egui::DragValue::new(&mut self.selection_pivot[0]).speed(1.0));
            ui.label("Y");
            ui.add(egui::DragValue::new(&mut self.selection_pivot[1]).speed(1.0));
        });
        if ui.small_button("Use selection center").clicked() {
            if let Some(bounds) = self.selection_bounds() {
                self.selection_pivot = [
                    bounds.min.x / 2.0 + bounds.max.x / 2.0,
                    bounds.min.y / 2.0 + bounds.max.y / 2.0,
                ];
            }
        }
        let pivot = Point::new(self.selection_pivot[0], self.selection_pivot[1]);
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.selection_angle)
                    .speed(1.0)
                    .suffix("°"),
            );
            if ui.button("Rotate").clicked() {
                self.apply_selection_transform(Transform2d::rotation(self.selection_angle, pivot));
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.selection_scale).speed(0.05));
            if ui.button("Scale").clicked() {
                self.apply_selection_transform(Transform2d::uniform_scale(
                    self.selection_scale,
                    pivot,
                ));
            }
        });
        self.advanced_selection_controls(ui);
        if ui.button("Assign current layer").clicked() {
            self.record_change();
            for entity in &mut self.document.entities {
                if self.selected_entities.contains(&entity.id) {
                    entity.layer_id = self.document.active_layer_id;
                }
            }
            self.mark_dirty("Selection moved to current layer");
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(7.0);
        ui.label(RichText::new("BLOCKS").size(9.0).strong().color(MUTED));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_block_name)
                    .desired_width(96.0)
                    .hint_text("Block name"),
            );
            if ui.button("Define block  ⌃G").clicked() {
                self.define_block_from_selection();
            }
        });
        if ui.button("Explode selected references").clicked() {
            self.explode_selected_references();
        }
        ui.label(
            RichText::new(
                "Defining replaces the selection with one reusable reference. Exploding restores its geometry.",
            )
            .size(9.0)
            .color(MUTED),
        );
    }

    fn selection_bounds(&self) -> Option<forge_core::edit::Bounds2d> {
        self.document
            .expanded_entities()
            .ok()?
            .iter()
            .filter(|entity| self.selected_entities.contains(&entity.source_id))
            .filter_map(|entity| entity.geometry.bounds())
            .reduce(|mut total, item| {
                total.include(item.min);
                total.include(item.max);
                total
            })
    }

    /// Replaces the selected entities with a reusable block reference. The whole
    /// operation is validated before any change is kept.
    fn define_block_from_selection(&mut self) {
        let ids: Vec<u64> = self.selected_entities.iter().copied().collect();
        if ids.is_empty() {
            self.status = "Select objects before defining a block".to_owned();
            self.status_is_error = true;
            return;
        }
        let name = if self.new_block_name.trim().is_empty() {
            format!("Block {}", self.document.blocks.len() + 1)
        } else {
            self.new_block_name.trim().to_owned()
        };
        let base = ids
            .iter()
            .filter_map(|id| {
                self.document
                    .entities
                    .iter()
                    .find(|entity| entity.id == *id)
            })
            .find_map(|entity| entity.geometry.bounds())
            .map(|bounds| Point::new(bounds.min.x, bounds.min.y))
            .unwrap_or_default();
        let before = self.document.clone();
        match self
            .document
            .define_block_from_selection(name.clone(), base, &ids)
        {
            Ok(id) => {
                self.record_snapshot(before);
                self.selected_entities.clear();
                self.selected_entities.insert(id);
                self.mark_dirty(format!("Block {name} defined"));
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }

    /// Explodes every selected block reference into ordinary entities. All
    /// references explode together or none do.
    fn explode_selected_references(&mut self) {
        let references: Vec<u64> = self
            .selected_entities
            .iter()
            .copied()
            .filter(|id| {
                self.document.entities.iter().any(|entity| {
                    entity.id == *id && matches!(entity.geometry, Geometry::Insert { .. })
                })
            })
            .collect();
        if references.is_empty() {
            self.status = "Select at least one block reference to explode".to_owned();
            self.status_is_error = true;
            return;
        }
        let before = self.document.clone();
        let mut candidate = self.document.clone();
        let mut created = Vec::new();
        let mut failure = None;
        for id in &references {
            match candidate.explode_insert(*id) {
                Ok(ids) => created.extend(ids),
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = failure {
            self.status = format!("Explode refused: {error}");
            self.status_is_error = true;
            return;
        }
        match candidate.validate() {
            Ok(()) => {
                self.document = candidate;
                self.record_snapshot(before);
                self.selected_entities = created.into_iter().collect();
                self.mark_dirty("Block references exploded");
            }
            Err(error) => {
                self.status = format!("Explode refused: {error}");
                self.status_is_error = true;
            }
        }
    }

    fn apply_selection_transform(&mut self, transform: Transform2d) {
        let ids: Vec<u64> = self.selected_entities.iter().copied().collect();
        let before = self.document.clone();
        let transformed = if self.selection_copy {
            self.document
                .duplicate_entities(&ids, transform)
                .map(|new_ids| {
                    self.selected_entities = new_ids.iter().copied().collect();
                    new_ids.len()
                })
        } else {
            self.document.transform_entities(&ids, transform)
        };
        match transformed {
            Ok(count) if count > 0 => {
                self.record_snapshot(before);
                self.mark_dirty(format!("Transformed {count} object(s)"));
            }
            Ok(_) => {}
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }

    fn model_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "INSPECTOR", "Solid parameters");
        if self
            .selected_solid
            .is_some_and(|id| self.document.extrusions.iter().any(|p| p.id == id))
        {
            self.extrusion_inspector(ui);
            self.project_controls(ui);
            return;
        }
        ui.add_space(12.0);
        if let Some(id) = self.selected_mesh {
            if let Some(object) = self.document.meshes.iter().find(|object| object.id == id) {
                ui.label(RichText::new(&object.name).strong());
                ui.label(format!(
                    "{} vertices\n{} triangles",
                    object.mesh.vertices.len(),
                    object.mesh.triangles.len()
                ));
                if let Some((min, max)) = object.mesh.bounds() {
                    ui.label(format!(
                        "Bounds\nX {:.3} to {:.3}\nY {:.3} to {:.3}\nZ {:.3} to {:.3}",
                        min[0], max[0], min[1], max[1], min[2], max[2]
                    ));
                }
            }
            if ui.button("Delete mesh").clicked() {
                self.record_change();
                self.document.meshes.retain(|object| object.id != id);
                self.selected_mesh = None;
                self.mark_dirty("Mesh removed");
            }
            self.project_controls(ui);
            return;
        }
        let Some(id) = self.selected_solid else {
            ui.label(
                RichText::new("Select a solid in the model tree or viewport.")
                    .size(10.0)
                    .color(MUTED),
            );
            self.project_controls(ui);
            return;
        };
        let Some(index) = self.document.solids.iter().position(|solid| solid.id == id) else {
            self.selected_solid = None;
            self.project_controls(ui);
            return;
        };
        let solid = &self.document.solids[index];
        ui.label(RichText::new(&solid.name).strong().size(13.0));
        ui.label(
            RichText::new(format!("{:?} · solid #{id}", solid.kind))
                .size(10.0)
                .color(MUTED),
        );
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(9.0);
        let mut changed = false;
        for (axis, label) in [(0, "Width / X"), (1, "Depth / Y"), (2, "Height / Z")] {
            let before = self.document.clone();
            ui.label(RichText::new(label).size(10.0).color(MUTED));
            let response = ui.add(
                egui::Slider::new(&mut self.document.solids[index].size[axis], 1.0..=300.0)
                    .suffix(" mm"),
            );
            if response.drag_started() || (response.changed() && !response.dragged()) {
                self.record_snapshot(before);
            }
            changed |= response.changed();
        }
        if changed {
            self.mark_dirty("Solid dimensions updated");
        }
        ui.add_space(8.0);
        for axis in 0..3 {
            let before = self.document.clone();
            let response = ui.add(
                egui::DragValue::new(&mut self.document.solids[index].center[axis])
                    .speed(1.0)
                    .prefix(format!("{}: ", ["X", "Y", "Z"][axis])),
            );
            if response.drag_started() || (response.changed() && !response.dragged()) {
                self.record_snapshot(before);
            }
            if response.changed() {
                self.mark_dirty("Solid position updated");
            }
        }
        ui.add_space(10.0);
        if ui.button("Delete solid").clicked() {
            self.record_change();
            self.document.remove_solid(id);
            self.selected_solid = None;
            self.mark_dirty("Solid deleted");
        }
        ui.add_space(12.0);
        ui.separator();
        self.project_controls(ui);
        ui.label(
            RichText::new(
                "STL and OBJ exports contain tessellated solid and imported mesh geometry.",
            )
            .size(9.0)
            .color(MUTED),
        );
    }

    fn project_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(13.0);
        section_header(ui, "PROJECT FILE", "Shared document");
        ui.add_space(5.0);
        ui.add(egui::TextEdit::singleline(&mut self.file_path).desired_width(f32::INFINITY));
        if self.active_workspace == Workspace::Model {
            egui::ComboBox::from_id_salt("model_export_format")
                .selected_text(&self.model_export_format)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.model_export_format,
                        "stl".to_owned(),
                        "STL binary",
                    );
                    ui.selectable_value(
                        &mut self.model_export_format,
                        "obj".to_owned(),
                        "OBJ geometry",
                    );
                });
        }
        ui.horizontal(|ui| {
            if ui.small_button("Save").clicked() {
                self.save_document();
            }
            if ui.small_button("Open").clicked() {
                self.open_document();
            }
            if ui.small_button("Export").clicked() {
                self.export_current();
            }
        });
    }

    fn drafting_canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);

        if self.fit_requested {
            if let Some(bounds) = self.document.drawing_bounds(true) {
                let width = (bounds.max.x - bounds.min.x).abs().max(10.0) as f32;
                let height = (bounds.max.y - bounds.min.y).abs().max(10.0) as f32;
                self.camera_2d.zoom = ((rect.width() - 80.0).max(40.0) / width)
                    .min((rect.height() - 80.0).max(40.0) / height)
                    .clamp(0.00001, 1000.0);
                let center = Point::new(
                    (bounds.min.x + bounds.max.x) * 0.5,
                    (bounds.min.y + bounds.max.y) * 0.5,
                );
                self.camera_2d.pan =
                    Vec2::new(-center.x as f32, center.y as f32) * self.camera_2d.zoom;
            }
            self.fit_requested = false;
        }

        self.interact_drafting(ui, rect, &response);
        if self.show_grid {
            paint_drafting_grid(&painter, rect, self.camera_2d);
        }
        // Block references are displayed as their expanded geometry so nested
        // INSERT chains appear exactly as the document's derived view defines.
        // An invalid document keeps the plain entity view instead.
        if let Some(leaves) = self.expanded_for_display() {
            for leaf in &leaves {
                if !leaf.visible {
                    continue;
                }
                let selected = self.selected_entities.contains(&leaf.source_id);
                let color = if selected {
                    SELECTED
                } else {
                    self.document
                        .layer(leaf.layer_id)
                        .map(|layer| {
                            Color32::from_rgb(layer.color[0], layer.color[1], layer.color[2])
                        })
                        .unwrap_or(TEXT)
                };
                paint_geometry(
                    &painter,
                    rect,
                    self.camera_2d,
                    &leaf.geometry,
                    color,
                    selected,
                );
            }
        } else {
            for entity in &self.document.entities {
                let visible = self
                    .document
                    .layer(entity.layer_id)
                    .map(|layer| layer.visible)
                    .unwrap_or(true);
                if visible {
                    let selected = self.selected_entities.contains(&entity.id);
                    let color = if selected {
                        SELECTED
                    } else {
                        self.document
                            .layer(entity.layer_id)
                            .map(|layer| {
                                Color32::from_rgb(layer.color[0], layer.color[1], layer.color[2])
                            })
                            .unwrap_or(TEXT)
                    };
                    paint_geometry(
                        &painter,
                        rect,
                        self.camera_2d,
                        &entity.geometry,
                        color,
                        selected,
                    );
                }
            }
        }
        if let (Some(start), Some(end)) = (self.draft_start, self.preview_point) {
            if matches!(
                self.draft_tool,
                DraftTool::Line | DraftTool::Rectangle | DraftTool::Circle
            ) {
                let geometry = self.geometry_from_points(start, end);
                paint_geometry(&painter, rect, self.camera_2d, &geometry, ACCENT, false);
            }
        }
        self.paint_construction_preview(&painter, rect);
        paint_axis_badge(&painter, rect);
        self.canvas_overlay(&painter, rect, response.hovered());
    }

    fn interact_drafting(&mut self, ui: &egui::Ui, rect: Rect, response: &Response) {
        if response.hovered() {
            if let Some(pointer) = response.hover_pos() {
                self.preview_point = Some(self.world_point(pointer, rect));
            }
            let wheel = ui.input(|input| input.smooth_scroll_delta.y);
            if wheel.abs() > 0.0 {
                if let Some(pointer) = response.hover_pos() {
                    let before = self.camera_2d.screen_to_world(pointer, rect);
                    let scale = (1.0 + wheel * 0.001).clamp(0.2, 5.0);
                    self.camera_2d.zoom = (self.camera_2d.zoom * scale).clamp(0.00001, 1000.0);
                    let after = self.camera_2d.world_to_screen(before, rect);
                    self.camera_2d.pan += pointer - after;
                }
            }
        }

        if response.drag_started_by(egui::PointerButton::Middle) {
            self.drag_state = DragState::Pan;
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.camera_2d.pan += ui.input(|input| input.pointer.delta());
        }
        if response.drag_stopped_by(egui::PointerButton::Middle) {
            self.drag_state = DragState::None;
        }

        if matches!(
            self.draft_tool,
            DraftTool::Polyline
                | DraftTool::Arc
                | DraftTool::Text
                | DraftTool::Point
                | DraftTool::Dimension
                | DraftTool::RadialDimension
        ) {
            if response.clicked_by(egui::PointerButton::Primary) {
                if let Some(pointer) = response.interact_pointer_pos() {
                    self.construction_click(self.world_point(pointer, rect));
                }
            }
            return;
        }

        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pointer) = ui
                .input(|input| input.pointer.press_origin())
                .or(response.interact_pointer_pos())
            {
                let world = self.world_point(pointer, rect);
                if self.draft_tool == DraftTool::Select {
                    self.select_entity_at(world, ui.input(|input| input.modifiers.shift));
                    self.drag_state = DragState::None;
                } else {
                    self.draft_start = Some(world);
                    self.preview_point = Some(world);
                    self.drag_state = DragState::Draw;
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) && self.drag_state == DragState::Draw {
            if let Some(pointer) = response.interact_pointer_pos() {
                self.preview_point = Some(self.world_point(pointer, rect));
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if self.drag_state == DragState::Draw {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let end = self.world_point(pointer, rect);
                    self.finish_draft(end);
                }
            }
            self.drag_state = DragState::None;
        }
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let point = self.world_point(pointer, rect);
                if self.draft_tool == DraftTool::Select {
                    self.select_entity_at(point, ui.input(|input| input.modifiers.shift));
                } else if self.draft_start.is_some() {
                    self.finish_draft(point);
                } else {
                    self.draft_start = Some(point);
                    self.preview_point = Some(point);
                    self.status =
                        "Start point set · click the other end or drag to finish".to_owned();
                }
            }
        }

        if response.double_clicked_by(egui::PointerButton::Primary) {
            self.draft_start = None;
            self.preview_point = None;
        }
    }

    fn canvas_overlay(&self, painter: &Painter, rect: Rect, hovered: bool) {
        let title = match self.active_workspace {
            Workspace::Draft => "MODEL SPACE",
            Workspace::Model => "3D MODEL SPACE",
            _ => "WORKSPACE",
        };
        painter.text(
            rect.left_top() + Vec2::new(16.0, 13.0),
            Align2::LEFT_TOP,
            title,
            FontId::proportional(9.0),
            MUTED,
        );
        if self.active_workspace == Workspace::Draft {
            let text = if hovered {
                self.preview_point
                    .map(|point| format!("X {:>7.1}   Y {:>7.1}", point.x, point.y))
                    .unwrap_or_else(|| "X   —     Y   —".to_owned())
            } else {
                "X   —     Y   —".to_owned()
            };
            painter.text(
                rect.right_bottom() - Vec2::new(15.0, 14.0),
                Align2::RIGHT_BOTTOM,
                text,
                FontId::monospace(10.0),
                MUTED,
            );
        }
    }

    fn model_canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);
        let parts = match self.cached_extrusions() {
            Ok(parts) => parts,
            Err(error) => {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    error.to_string(),
                    FontId::proportional(14.0),
                    Color32::LIGHT_RED,
                );
                return;
            }
        };
        if self.model_fit_requested {
            let unit_camera = Camera3d {
                zoom: 1.0,
                pan: Vec2::ZERO,
                ..self.camera_3d
            };
            let mut bounds = Rect::NOTHING;
            for (_, mesh) in parts.iter() {
                for vertex in &mesh.vertices {
                    bounds.extend_with(unit_camera.project(*vertex, rect).0);
                }
            }
            for object in &self.document.meshes {
                for vertex in &object.mesh.vertices {
                    bounds.extend_with(unit_camera.project(*vertex, rect).0);
                }
            }
            for solid in &self.document.solids {
                for sx in [-0.5, 0.5] {
                    for sy in [-0.5, 0.5] {
                        for sz in [-0.5, 0.5] {
                            bounds.extend_with(
                                unit_camera
                                    .project(
                                        [
                                            solid.center[0] + sx * solid.size[0],
                                            solid.center[1] + sy * solid.size[1],
                                            solid.center[2] + sz * solid.size[2],
                                        ],
                                        rect,
                                    )
                                    .0,
                            );
                        }
                    }
                }
            }
            if bounds.is_finite() && bounds.is_positive() {
                self.camera_3d.zoom = ((rect.width() - 80.0).max(40.0) / bounds.width().max(1.0))
                    .min((rect.height() - 80.0).max(40.0) / bounds.height().max(1.0))
                    .clamp(0.00001, 1000.0);
                self.camera_3d.pan = (rect.center() - bounds.center()) * self.camera_3d.zoom;
            }
            self.model_fit_requested = false;
        }
        self.interact_model(ui, rect, &response);
        paint_model_grid(&painter, rect, self.camera_3d);
        for (id, mesh) in parts.iter() {
            paint_triangle_mesh(
                &painter,
                rect,
                self.camera_3d,
                mesh,
                self.selected_solid == Some(*id),
            );
        }
        for solid in &self.document.solids {
            paint_solid(
                &painter,
                rect,
                self.camera_3d,
                solid,
                self.selected_solid == Some(solid.id),
            );
        }
        for object in &self.document.meshes {
            paint_triangle_mesh(
                &painter,
                rect,
                self.camera_3d,
                &object.mesh,
                self.selected_mesh == Some(object.id),
            );
        }
        paint_axis_badge(&painter, rect);
        painter.text(
            rect.left_top() + Vec2::new(16.0, 13.0),
            Align2::LEFT_TOP,
            format!(
                "ISOMETRIC VIEW · {}",
                match self.document.units {
                    1 => "INCHES",
                    4 => "MILLIMETRES",
                    6 => "METRES",
                    _ => "MODEL UNITS",
                }
            ),
            FontId::proportional(9.0),
            MUTED,
        );
        let cube =
            Rect::from_min_size(rect.right_top() + Vec2::new(-64.0, 15.0), Vec2::splat(48.0));
        painter.rect_filled(cube, 6.0, PANEL_ALT);
        painter.rect_stroke(cube, 6.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
        painter.text(
            cube.center(),
            Align2::CENTER_CENTER,
            "ISO",
            FontId::proportional(10.0),
            TEXT,
        );
    }

    fn interact_model(&mut self, ui: &egui::Ui, rect: Rect, response: &Response) {
        if response.hovered() {
            let wheel = ui.input(|input| input.smooth_scroll_delta.y);
            if wheel.abs() > 0.0 {
                self.camera_3d.zoom =
                    (self.camera_3d.zoom * (1.0 + wheel * 0.001)).clamp(0.00001, 1000.0);
            }
        }
        if response.drag_started_by(egui::PointerButton::Middle) {
            self.drag_state = DragState::Pan;
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.camera_3d.pan += ui.input(|input| input.pointer.delta());
        }
        if response.drag_stopped_by(egui::PointerButton::Middle) {
            self.drag_state = DragState::None;
        }
        if response.drag_started_by(egui::PointerButton::Secondary) {
            self.drag_state = DragState::Orbit;
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = ui.input(|input| input.pointer.delta());
            self.camera_3d.yaw += delta.x * 0.008;
            self.camera_3d.pitch = (self.camera_3d.pitch + delta.y * 0.008).clamp(-1.2, 1.2);
        }
        if response.drag_stopped_by(egui::PointerButton::Secondary) {
            self.drag_state = DragState::None;
        }
        if self.active_workspace == Workspace::Model
            && response.clicked_by(egui::PointerButton::Primary)
        {
            if let Some(pointer) = response.interact_pointer_pos() {
                let mut nearest = self
                    .document
                    .solids
                    .iter()
                    .map(|solid| {
                        let (screen, _) = self.camera_3d.project(solid.center, rect);
                        (solid.id, screen.distance(pointer))
                    })
                    .filter(|(_, distance)| *distance < 100.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((_, parts)) = &self.model_ui.cache {
                    for (id, mesh) in parts.iter() {
                        if let Some((min, max)) = mesh.bounds() {
                            let center =
                                std::array::from_fn(|axis| min[axis] / 2.0 + max[axis] / 2.0);
                            let distance = self.camera_3d.project(center, rect).0.distance(pointer);
                            if distance < 100.0 && nearest.is_none_or(|(_, d)| distance < d) {
                                nearest = Some((*id, distance));
                            }
                        }
                    }
                }
                self.selected_solid = nearest.map(|(id, _)| id);
                self.selected_mesh = None;
                let nearest_mesh = self
                    .document
                    .meshes
                    .iter()
                    .filter_map(|object| {
                        let (min, max) = object.mesh.bounds()?;
                        let center = [
                            (min[0] + max[0]) * 0.5,
                            (min[1] + max[1]) * 0.5,
                            (min[2] + max[2]) * 0.5,
                        ];
                        let distance = self.camera_3d.project(center, rect).0.distance(pointer);
                        (distance < 100.0).then_some((object.id, distance))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((id, distance)) = nearest_mesh {
                    if nearest.is_none_or(|(_, solid_distance)| distance < solid_distance) {
                        self.selected_mesh = Some(id);
                        self.selected_solid = None;
                    }
                }
            }
        }
    }

    fn set_draft_tool(&mut self, tool: DraftTool) {
        if self.draft_tool != tool {
            self.draft_start = None;
            self.preview_point = None;
            self.construction_points.clear();
            self.drag_state = DragState::None;
        }
        self.draft_tool = tool;
    }

    fn commit_geometry(&mut self, geometry: Geometry) -> bool {
        if let Err(error) = geometry.validate() {
            self.status = error.to_string();
            self.status_is_error = true;
            return false;
        }
        self.record_change();
        let id = self.document.add_geometry(geometry);
        self.selected_entities.clear();
        self.selected_entities.insert(id);
        self.mark_dirty(format!(
            "{} created on current layer",
            self.draft_tool.name()
        ));
        true
    }

    fn construction_click(&mut self, point: Point) {
        match self.draft_tool {
            DraftTool::Point => {
                self.commit_geometry(Geometry::Point { position: point });
            }
            DraftTool::Text => {
                if self.draft_text.trim().is_empty() {
                    self.status = "Enter text contents before placing text".into();
                    self.status_is_error = true;
                    return;
                }
                self.commit_geometry(Geometry::Text {
                    position: point,
                    text: self.draft_text.clone(),
                    height: self.draft_text_height,
                    rotation_deg: 0.0,
                });
            }
            DraftTool::Polyline => {
                if self.construction_points.first() == Some(&point)
                    && self.construction_points.len() >= 3
                {
                    self.polyline_closed = true;
                    self.finish_polyline();
                    return;
                }
                if self.construction_points.last() != Some(&point) {
                    self.construction_points.push(point);
                }
                self.status = format!(
                    "{} polyline vertices · Enter to finish · Escape to cancel",
                    self.construction_points.len()
                );
                self.status_is_error = false;
            }
            DraftTool::Arc => {
                if self.construction_points.len() < 2 {
                    if self.construction_points.first() == Some(&point) {
                        self.status = "Arc start must differ from its center".into();
                        self.status_is_error = true;
                        return;
                    }
                    self.construction_points.push(point);
                    self.status = if self.construction_points.len() == 1 {
                        "Arc center set · choose start point"
                    } else {
                        "Arc start set · choose counterclockwise end angle"
                    }
                    .into();
                    self.status_is_error = false;
                } else {
                    let center = self.construction_points[0];
                    let start = self.construction_points[1];
                    if point == center {
                        self.status = "Choose an end angle away from the center".into();
                        self.status_is_error = true;
                        return;
                    }
                    let start_angle = (start.y - center.y)
                        .atan2(start.x - center.x)
                        .to_degrees()
                        .rem_euclid(360.0);
                    let end_angle = (point.y - center.y)
                        .atan2(point.x - center.x)
                        .to_degrees()
                        .rem_euclid(360.0);
                    if (end_angle - start_angle).rem_euclid(360.0) < 1e-10 {
                        self.status =
                            "Choose a different end angle; use Circle for a full circle".into();
                        self.status_is_error = true;
                        return;
                    }
                    if self.commit_geometry(Geometry::Arc {
                        center,
                        radius: center.distance(start),
                        start_angle_deg: start_angle,
                        end_angle_deg: end_angle,
                    }) {
                        self.construction_points.clear();
                    }
                }
            }
            DraftTool::Dimension => {
                if self.construction_points.len() < 2 {
                    if self.construction_points.first() == Some(&point) {
                        self.status = "Dimension references must be distinct points".into();
                        self.status_is_error = true;
                        return;
                    }
                    self.construction_points.push(point);
                    self.status = if self.construction_points.len() == 1 {
                        "First reference set · choose the second reference point"
                    } else {
                        "References set · choose a point on the dimension line"
                    }
                    .into();
                    self.status_is_error = false;
                } else {
                    let dimension = LinearDimension::new(
                        self.dimension_kind,
                        self.construction_points[0],
                        self.construction_points[1],
                        point,
                    );
                    match dimension {
                        Ok(dimension) => {
                            if self.commit_geometry(Geometry::Dimension {
                                dimension: Dimension::Linear(dimension),
                            }) {
                                self.construction_points.clear();
                            }
                        }
                        Err(error) => {
                            self.status = error.to_string();
                            self.status_is_error = true;
                        }
                    }
                }
            }
            DraftTool::RadialDimension => {
                if self.construction_points.is_empty() {
                    self.construction_points.push(point);
                    self.status = "Center set · choose a point on the circle".into();
                    self.status_is_error = false;
                } else {
                    let center = self.construction_points[0];
                    if point == center {
                        self.status = "Choose a point on the circle away from its center".into();
                        self.status_is_error = true;
                        return;
                    }
                    match RadialDimension::new(center, point, self.dimension_diameter) {
                        Ok(dimension) => {
                            if self.commit_geometry(Geometry::Dimension {
                                dimension: Dimension::Radial(dimension),
                            }) {
                                self.construction_points.clear();
                            }
                        }
                        Err(error) => {
                            self.status = error.to_string();
                            self.status_is_error = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn finish_polyline(&mut self) {
        let minimum = if self.polyline_closed { 3 } else { 2 };
        if self.construction_points.len() < minimum {
            self.status = format!("Choose at least {minimum} distinct polyline vertices");
            self.status_is_error = true;
            return;
        }
        let geometry = Geometry::Polyline {
            vertices: self
                .construction_points
                .iter()
                .map(|point| PolylineVertex::new(*point))
                .collect(),
            closed: self.polyline_closed,
        };
        if self.commit_geometry(geometry) {
            self.construction_points.clear();
        }
    }

    fn paint_construction_preview(&self, painter: &Painter, rect: Rect) {
        let Some(cursor) = self.preview_point else {
            return;
        };
        let mut preview: Vec<Geometry> = Vec::new();
        match self.draft_tool {
            DraftTool::Polyline if !self.construction_points.is_empty() => {
                let mut points = self.construction_points.clone();
                if points.last() != Some(&cursor) {
                    points.push(cursor);
                }
                preview.push(Geometry::Polyline {
                    vertices: points.into_iter().map(PolylineVertex::new).collect(),
                    closed: false,
                });
            }
            DraftTool::Arc if self.construction_points.len() == 1 => {
                preview.push(Geometry::Line {
                    start: self.construction_points[0],
                    end: cursor,
                });
            }
            DraftTool::Arc if self.construction_points.len() == 2 => {
                let center = self.construction_points[0];
                let start = self.construction_points[1];
                preview.push(Geometry::Arc {
                    center,
                    radius: center.distance(start),
                    start_angle_deg: (start.y - center.y).atan2(start.x - center.x).to_degrees(),
                    end_angle_deg: (cursor.y - center.y)
                        .atan2(cursor.x - center.x)
                        .to_degrees(),
                });
            }
            DraftTool::Text => preview.push(Geometry::Text {
                position: cursor,
                text: self.draft_text.clone(),
                height: self.draft_text_height,
                rotation_deg: 0.0,
            }),
            DraftTool::Dimension => match self.construction_points.len() {
                1 => preview.push(Geometry::Line {
                    start: self.construction_points[0],
                    end: cursor,
                }),
                2.. => {
                    let first = self.construction_points[0];
                    let second = self.construction_points[1];
                    preview.push(Geometry::Line {
                        start: first,
                        end: second,
                    });
                    preview.push(Geometry::Line {
                        start: second,
                        end: cursor,
                    });
                    if let Ok(dimension) =
                        LinearDimension::new(self.dimension_kind, first, second, cursor)
                    {
                        // The live preview dimension shows exactly what a click commits.
                        preview.push(Geometry::Dimension {
                            dimension: Dimension::Linear(dimension),
                        });
                    }
                }
                _ => {}
            },
            DraftTool::RadialDimension => {
                if let Some(center) = self.construction_points.first() {
                    preview.push(Geometry::Line {
                        start: *center,
                        end: cursor,
                    });
                    if let Ok(dimension) =
                        RadialDimension::new(*center, cursor, self.dimension_diameter)
                    {
                        preview.push(Geometry::Dimension {
                            dimension: Dimension::Radial(dimension),
                        });
                    }
                }
            }
            _ => {}
        }
        for geometry in &preview {
            paint_geometry(painter, rect, self.camera_2d, geometry, ACCENT, false);
        }
    }

    fn world_point(&self, screen: Pos2, rect: Rect) -> Point {
        let mut point = self.camera_2d.screen_to_world(screen, rect);
        if self.draft_tool == DraftTool::Select {
            return point;
        }
        if self.snap_to_objects {
            if let Some(snap) = self.document.object_snap(
                point,
                8.0 / self.camera_2d.zoom.max(0.00001) as f64,
                SnapOptions::default(),
            ) {
                return snap.point;
            }
        }
        if self.snap_to_grid {
            point.x = (point.x / 10.0).round() * 10.0;
            point.y = (point.y / 10.0).round() * 10.0;
        }
        point
    }

    fn geometry_from_points(&self, start: Point, end: Point) -> Geometry {
        match self.draft_tool {
            DraftTool::Line
            | DraftTool::Select
            | DraftTool::Polyline
            | DraftTool::Arc
            | DraftTool::Text
            | DraftTool::Point
            | DraftTool::Dimension
            | DraftTool::RadialDimension => Geometry::Line { start, end },
            DraftTool::Rectangle => Geometry::Rectangle { a: start, b: end },
            DraftTool::Circle => Geometry::Circle {
                center: start,
                radius: start.distance(end),
            },
        }
    }

    fn finish_draft(&mut self, end: Point) {
        let Some(start) = self.draft_start.take() else {
            return;
        };
        self.preview_point = None;
        if start.distance(end) < 0.01 {
            self.status = "Shape too small · choose a wider point".to_owned();
            return;
        }
        let geometry = self.geometry_from_points(start, end);
        self.commit_geometry(geometry);
    }

    fn select_entity_at(&mut self, point: Point, extend: bool) {
        let tolerance = 8.0 / self.camera_2d.zoom.max(0.00001) as f64;
        let selected = self.nearest_visible_leaf(point, tolerance);
        if !extend {
            self.selected_entities.clear();
        }
        if let Some((id, _)) = selected {
            if extend && self.selected_entities.contains(&id) {
                self.selected_entities.remove(&id);
            } else {
                self.selected_entities.insert(id);
            }
            self.status = format!("Selected object #{id}");
        } else {
            self.status = "Nothing selected".to_owned();
        }
    }

    /// Nearest visible derived geometry mapped to its top-level selectable
    /// entity. Block references are picked through their expanded contents so
    /// clicking a nested line selects the outermost INSERT.
    fn nearest_visible_leaf(&self, point: Point, tolerance: f64) -> Option<(u64, f64)> {
        let candidates: Vec<(u64, f64)> = if let Some(leaves) = self.expanded_for_display() {
            leaves
                .iter()
                .filter(|leaf| leaf.visible)
                .map(|leaf| (leaf.source_id, leaf.geometry.distance_to(point)))
                .collect()
        } else {
            self.document
                .entities
                .iter()
                .filter(|entity| {
                    self.document
                        .layer(entity.layer_id)
                        .map(|layer| layer.visible)
                        .unwrap_or(true)
                })
                .map(|entity| (entity.id, entity.geometry.distance_to(point)))
                .collect()
        };
        candidates
            .into_iter()
            .filter(|(_, distance)| *distance <= tolerance)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Derived display/picking view of the whole document, or `None` when the
    /// document cannot be expanded (invalid content keeps the flat view).
    fn expanded_for_display(&self) -> Option<Vec<ExpandedEntity>> {
        self.document.expanded_entities().ok()
    }

    fn add_solid(&mut self, kind: SolidKind) {
        self.record_change();
        let count = self
            .document
            .solids
            .iter()
            .filter(|solid| solid.kind == kind)
            .count()
            + 1;
        let label = match kind {
            SolidKind::Box => format!("Box {count}"),
            SolidKind::Cylinder => format!("Cylinder {count}"),
            SolidKind::Sphere => format!("Sphere {count}"),
        };
        let id = self.document.add_solid(label, kind);
        let ordinal = self.document.solids.len().saturating_sub(1);
        if let Some(solid) = self.document.solids.iter_mut().find(|solid| solid.id == id) {
            solid.center[0] = (ordinal % 5) as f64 * 100.0;
            solid.center[1] = (ordinal / 5) as f64 * 100.0;
        }
        self.selected_solid = Some(id);
        self.selected_mesh = None;
        self.mark_dirty(format!("{kind:?} added to model"));
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (ctrl_s, undo, redo, delete, escape, v, l, r, c) = ctx.input(|input| {
            (
                input.modifiers.command && input.key_pressed(Key::S),
                input.modifiers.command && !input.modifiers.shift && input.key_pressed(Key::Z),
                input.modifiers.command
                    && (input.key_pressed(Key::Y)
                        || (input.modifiers.shift && input.key_pressed(Key::Z))),
                input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace),
                input.key_pressed(Key::Escape),
                input.key_pressed(Key::V),
                input.key_pressed(Key::L),
                input.key_pressed(Key::R),
                input.key_pressed(Key::C),
            )
        });
        if ctrl_s {
            self.save_document();
        }
        // Text fields own editing keys while focused. A filename containing
        // "r", "c", or Backspace must never change tools or delete geometry.
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if delete
            && !self.construction_points.is_empty()
            && self.active_workspace == Workspace::Draft
        {
            self.construction_points.pop();
            self.status = "Last construction point removed".into();
        } else if delete && self.active_workspace == Workspace::Draft {
            self.delete_selected_entities();
        } else if delete && self.active_workspace == Workspace::Model {
            if let Some(id) = self.selected_mesh {
                self.record_change();
                self.document.meshes.retain(|object| object.id != id);
                self.selected_mesh = None;
                self.mark_dirty("Mesh removed");
            } else if let Some(id) = self.selected_solid {
                self.record_change();
                self.document.remove_solid(id);
                self.selected_solid = None;
                self.mark_dirty("Solid deleted");
            }
        }
        if escape {
            self.draft_start = None;
            self.preview_point = None;
            self.set_draft_tool(DraftTool::Select);
        }
        if self.active_workspace == Workspace::Draft {
            if ctx.input(|input| input.modifiers.command && input.key_pressed(Key::A)) {
                self.selected_entities = self
                    .document
                    .entities
                    .iter()
                    .filter(|entity| {
                        self.document
                            .layer(entity.layer_id)
                            .map(|layer| layer.visible)
                            .unwrap_or(true)
                    })
                    .map(|entity| entity.id)
                    .collect();
            }
            if ctx.input(|input| input.modifiers.command && input.key_pressed(Key::G)) {
                self.define_block_from_selection();
            }
            if ctx.input(|input| {
                input.modifiers.command || input.modifiers.ctrl || input.modifiers.alt
            }) {
                return;
            }
            if ctx.input(|input| input.key_pressed(Key::F)) {
                self.fit_requested = true;
            }
            if ctx.input(|input| input.key_pressed(Key::Enter))
                && self.draft_tool == DraftTool::Polyline
            {
                self.finish_polyline();
            }
            for (key, tool) in [
                (Key::P, DraftTool::Polyline),
                (Key::A, DraftTool::Arc),
                (Key::T, DraftTool::Text),
                (Key::O, DraftTool::Point),
                (Key::D, DraftTool::Dimension),
                (Key::B, DraftTool::RadialDimension),
            ] {
                if ctx.input(|input| input.key_pressed(key)) {
                    self.set_draft_tool(tool);
                }
            }
            if v {
                self.set_draft_tool(DraftTool::Select);
            } else if l {
                self.set_draft_tool(DraftTool::Line);
            } else if r {
                self.set_draft_tool(DraftTool::Rectangle);
            } else if c {
                self.set_draft_tool(DraftTool::Circle);
            }
        }
    }

    fn request_action(&mut self, action: PendingAction) {
        if self.dirty {
            self.pending_save_path = self.save_destination.clone();
            self.pending_action = Some(action);
        } else {
            self.execute_action(action, None);
        }
    }

    fn execute_action(&mut self, action: PendingAction, ctx: Option<&egui::Context>) {
        match action {
            PendingAction::New => self.new_document(),
            PendingAction::Open(path) => self.perform_open_document(path),
            PendingAction::Close => {
                self.allow_close = true;
                if let Some(ctx) = ctx {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_action.clone() else {
            return;
        };
        let mut decision = None;
        egui::Window::new("Save current design?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("This design has unsaved changes.");
                ui.label("Save the current design before continuing, or discard its edits.");
                ui.add_space(8.0);
                ui.label("Native project destination");
                ui.add(egui::TextEdit::singleline(&mut self.pending_save_path).desired_width(380.0));
                ui.horizontal(|ui| {
                    if ui.button("Save and continue").clicked() {
                        let destination = native_save_path(&self.pending_save_path);
                        let collides_with_open = match (&action, &destination) {
                            (PendingAction::Open(source), Ok(destination)) => source == destination,
                            _ => false,
                        };
                        if collides_with_open {
                            self.status = "Choose a different save destination to preserve the project being opened.".to_owned();
                            self.status_is_error = true;
                        } else {
                            let old_input = self.file_path.clone();
                            self.file_path = self.pending_save_path.clone();
                            if self.save_document() {
                                decision = Some(true);
                            } else {
                                self.file_path = old_input;
                            }
                        }
                    }
                    if ui.button("Discard edits").clicked() {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
                if self.status_is_error {
                    ui.colored_label(Color32::LIGHT_RED, &self.status);
                }
            });
        if let Some(continue_action) = decision {
            self.pending_action = None;
            if continue_action {
                self.execute_action(action, Some(ctx));
            }
        }
    }

    fn new_document(&mut self) {
        self.construction_points.clear();
        self.document = Document::default();
        self.reset_workbench_views();
        self.selected_entities.clear();
        self.selected_solid = None;
        self.selected_mesh = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.file_path = "untitled.forge.json".to_owned();
        self.save_destination = self.file_path.clone();
        self.draft_start = None;
        self.preview_point = None;
        self.drag_state = DragState::None;
        self.dirty = false;
        self.status_is_error = false;
        self.status = "New design created".to_owned();
    }

    fn open_document(&mut self) {
        self.request_action(PendingAction::Open(PathBuf::from(&self.file_path)));
    }

    fn perform_open_document(&mut self, path: PathBuf) {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_exchange = extension != "json";
        let loaded = match extension.as_str() {
            "dxf" => Document::import_dxf(&path),
            "json" => Document::load_json(&path),
            "stl" | "obj" => {
                let mesh = if extension == "stl" {
                    TriangleMesh::read_stl(&path)
                } else {
                    TriangleMesh::read_obj(&path)
                };
                mesh.and_then(|mesh| {
                    let name = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Imported mesh");
                    let mut document = Document::new(name);
                    document.add_mesh(name, mesh)?;
                    Ok(document)
                })
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Open supports native .forge.json, supported ASCII DXF entities, and STL/OBJ mesh geometry. DWG drawings use the forge native commands.",
            )),
        };
        match loaded {
            Ok(document) => {
                self.construction_points.clear();
                self.document = document;
                self.reset_workbench_views();
                self.selected_entities.clear();
                self.selected_solid = None;
                self.selected_mesh = None;
                self.draft_start = None;
                self.preview_point = None;
                self.drag_state = DragState::None;
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.dirty = is_exchange;
                self.status_is_error = false;
                if is_exchange {
                    self.file_path = path
                        .with_extension("forge.json")
                        .to_string_lossy()
                        .into_owned();
                    self.status = format!(
                        "Imported {} · Save writes a separate native project",
                        path.display()
                    );
                    if extension == "obj" {
                        self.status.push_str(
                            " · Geometry only; materials/textures/scene metadata are unsupported",
                        );
                    }
                } else {
                    self.file_path = path.to_string_lossy().into_owned();
                    self.status = format!("Opened {}", path.display());
                }
                let workspace = self.document_workspace();
                if workspace != self.active_workspace {
                    self.active_workspace = workspace;
                    self.selected_product_id = default_product_id(workspace).into();
                }
                self.selected_mesh = self.document.meshes.first().map(|object| object.id);
                self.fit_requested = true;
                self.model_fit_requested = true;
                self.save_destination = self.file_path.clone();
            }
            Err(error) => {
                self.status = format!("Could not open {}: {error}", path.display());
                self.status_is_error = true;
            }
        }
    }

    fn save_document(&mut self) -> bool {
        let path = match native_save_path(&self.file_path) {
            Ok(path) => path,
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
                return false;
            }
        };
        match self.document.save_json(&path) {
            Ok(()) => {
                self.dirty = false;
                self.file_path = path.to_string_lossy().into_owned();
                self.save_destination = self.file_path.clone();
                self.status_is_error = false;
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(error) => {
                self.status = format!("Could not save {}: {error}", path.display());
                self.status_is_error = true;
                false
            }
        }
    }

    fn prepare_export(&self) -> std::io::Result<PreparedExport> {
        if matches!(
            self.active_workspace,
            Workspace::Build
                | Workspace::Terrain
                | Workspace::Motion
                | Workspace::Make
                | Workspace::Connect
        ) {
            return self.prepare_workbench_export();
        }
        self.document.validate()?;
        let mut losses = Vec::new();
        if !self.document.layouts.is_empty() {
            losses.push("Saved print layouts and paper notes are omitted from neutral export. Native Save retains them; Export saved layout produces their SVG sheets.".into());
        }
        let (path, bytes) = if self.active_workspace == Workspace::Model {
            if self.document.has_workbench_data() {
                losses.push("Semantic workbench models are omitted from Model mesh export; export each workbench separately to include its derived geometry.".to_owned());
            }
            let path = export_path(&self.file_path, &self.model_export_format);
            if !self.document.entities.is_empty() {
                losses.push(
                    "2D drawing entities and their layers are omitted from mesh export.".to_owned(),
                );
            }
            if !self.document.solids.is_empty() || !self.document.extrusions.is_empty() {
                losses.push("Primitive parameters become triangle surfaces without editable feature history.".to_owned());
            }
            if self.document.solids.len()
                + self.document.meshes.len()
                + self.document.extrusions.len()
                > 1
            {
                losses.push("Individual object identities are merged into one mesh.".to_owned());
            }
            if self.document.units != 0 {
                losses
                    .push("The mesh file does not retain the native project unit code.".to_owned());
            }
            let mesh = self.document.model_mesh(TessellationOptions::default())?;
            if mesh.triangles.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "There is no 3D triangle geometry to export.",
                ));
            }
            let bytes = if self.model_export_format == "obj" {
                mesh.to_obj(&self.document.name)?.into_bytes()
            } else {
                if mesh
                    .vertices
                    .iter()
                    .flatten()
                    .any(|value| (*value as f32) as f64 != *value)
                {
                    losses.push("Binary STL rounds coordinates to float32 precision.".to_owned());
                }
                mesh.to_stl_binary()?
            };
            (path, bytes)
        } else {
            let mut drawing = self.document.clone();
            drawing.layouts.clear();
            (
                export_path(&self.file_path, "dxf"),
                forge_core::dxf::write(&drawing)?.into_bytes(),
            )
        };
        let replace_existing = std::fs::symlink_metadata(&path).is_ok();
        Ok(PreparedExport {
            path,
            bytes,
            losses,
            replace_existing,
        })
    }

    fn export_current(&mut self) {
        match self.prepare_export() {
            Ok(job) if job.replace_existing || !job.losses.is_empty() => {
                self.pending_export = Some(job)
            }
            Ok(job) => self.publish_export(job),
            Err(error) => {
                self.status = format!("Export failed: {error}");
                self.status_is_error = true;
            }
        }
    }

    fn publish_export(&mut self, job: PreparedExport) {
        match forge_core::storage::write_file(&job.path, &job.bytes, job.replace_existing) {
            Ok(()) => {
                self.status = format!("Exported {}", job.path.display());
                self.status_is_error = false;
            }
            Err(error) => {
                self.status = format!("Export failed: {error}");
                self.status_is_error = true;
            }
        }
    }

    fn export_confirmation(&mut self, ctx: &egui::Context) {
        let Some(job) = self.pending_export.as_ref() else {
            return;
        };
        let mut decision = None;
        egui::Window::new("Export details")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(job.path.to_string_lossy());
                if job.replace_existing {
                    ui.colored_label(SELECTED, "The existing output file will be replaced.");
                }
                for loss in &job.losses {
                    ui.label(format!("• {loss}"));
                }
                ui.label("Save the native project to preserve its editable data.");
                ui.horizontal(|ui| {
                    if ui.button("Export").clicked() {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
            });
        if let Some(export) = decision {
            let job = self.pending_export.take().expect("pending export");
            if export {
                self.publish_export(job);
            }
        }
    }

    fn delete_selected_entities(&mut self) {
        if self.selected_entities.is_empty() {
            return;
        }
        self.record_change();
        let ids = std::mem::take(&mut self.selected_entities);
        let count = ids
            .iter()
            .filter(|id| self.document.remove_entity(**id))
            .count();
        self.mark_dirty(format!("Deleted {count} object(s)"));
    }

    fn switch_workspace(&mut self, workspace: Workspace) {
        self.construction_points.clear();
        self.active_workspace = workspace;
        self.model_fit_requested = true;
        if catalog::product(&self.selected_product_id)
            .is_none_or(|product| workspace_for_area(product.area) != workspace)
        {
            self.selected_product_id = default_product_id(workspace).to_owned();
        }
        self.drag_state = DragState::None;
        self.draft_start = None;
        self.preview_point = None;
        self.status_is_error = false;
        self.status = format!("{} workbench selected", workbench(workspace).name);
    }

    fn record_change(&mut self) {
        self.record_snapshot(self.document.clone());
    }

    fn commit_workbench_document(&mut self, candidate: Document, label: impl Into<String>) -> bool {
        if let Err(error) = candidate.validate() {
            self.status = error.to_string();
            self.status_is_error = true;
            return false;
        }
        if candidate == self.document {
            return true;
        }
        self.record_change();
        self.document = candidate;
        self.model_fit_requested = true;
        self.mark_dirty(label);
        true
    }

    fn reset_workbench_views(&mut self) {
        self.building_ui = building_ui::BuildingUiState::default();
        self.terrain_ui = terrain_ui::TerrainUiState::default();
        self.scene_ui = scene_ui::SceneUiState::default();
        self.workbench_ui = workbench_ui::WorkbenchUiState::default();
        self.review_ui = review_ui::ReviewUiState::default();
        self.model_ui = model_ui::ModelUiState::default();
        self.model_fit_requested = true;
    }

    fn document_workspace(&self) -> Workspace {
        let has_content = |area| match area {
            Workspace::Draft => !self.document.entities.is_empty(),
            Workspace::Model => self.document.has_model_geometry(),
            Workspace::Build => !self.document.building.is_empty(),
            Workspace::Terrain => !self.document.terrain.is_empty(),
            Workspace::Motion => self.document.scene != forge_core::scene::Scene::default(),
            Workspace::Make => self.document.manufacturing != forge_core::cam::CamJob::default(),
            Workspace::Connect => !self.document.review.is_empty(),
        };
        if has_content(self.active_workspace) {
            return self.active_workspace;
        }
        [
            Workspace::Draft,
            Workspace::Model,
            Workspace::Build,
            Workspace::Terrain,
            Workspace::Motion,
            Workspace::Make,
            Workspace::Connect,
        ]
        .into_iter()
        .find(|area| has_content(*area))
        .unwrap_or(self.active_workspace)
    }

    fn record_snapshot(&mut self, snapshot: Document) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn undo(&mut self) {
        if let Some(previous) = self.undo_stack.pop() {
            self.redo_stack.push(self.document.clone());
            self.document = previous;
            self.selected_entities.clear();
            self.selected_solid = None;
            self.selected_mesh = None;
            self.dirty = true;
            self.status_is_error = false;
            self.status = "Undo complete".to_owned();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.document.clone());
            self.document = next;
            self.selected_entities.clear();
            self.selected_solid = None;
            self.selected_mesh = None;
            self.dirty = true;
            self.status_is_error = false;
            self.status = "Redo complete".to_owned();
        }
    }

    fn mark_dirty(&mut self, status: impl Into<String>) {
        self.dirty = true;
        self.status = status.into();
        self.status_is_error = false;
    }
}

fn workbench(workspace: Workspace) -> WorkbenchInfo {
    WORKBENCHES
        .into_iter()
        .find(|info| info.workspace == workspace)
        .unwrap_or(WORKBENCHES[0])
}

fn default_product_id(workspace: Workspace) -> &'static str {
    match workspace {
        Workspace::Draft => "autocad",
        Workspace::Model => "inventor",
        Workspace::Build => "revit",
        Workspace::Terrain => "civil-3d",
        Workspace::Make => "inventor-cam",
        Workspace::Motion => "maya",
        Workspace::Connect => "autodesk-docs",
    }
}

fn workspace_for_area(area: WorkArea) -> Workspace {
    match area {
        WorkArea::Draft => Workspace::Draft,
        WorkArea::Model => Workspace::Model,
        WorkArea::Build => Workspace::Build,
        WorkArea::Terrain => Workspace::Terrain,
        WorkArea::Make => Workspace::Make,
        WorkArea::Motion => Workspace::Motion,
        WorkArea::Connect => Workspace::Connect,
    }
}

fn paint_triangle_mesh(
    painter: &Painter,
    rect: Rect,
    camera: Camera3d,
    mesh: &TriangleMesh,
    selected: bool,
) {
    let projected: Vec<(Pos2, f64)> = mesh
        .vertices
        .iter()
        .map(|vertex| camera.project(*vertex, rect))
        .collect();
    let mut faces = Vec::with_capacity(mesh.triangles.len());
    for triangle in &mesh.triangles {
        let [Some(a), Some(b), Some(c)] =
            triangle.map(|index| projected.get(index as usize).copied())
        else {
            continue;
        };
        let points = [a.0, b.0, c.0];
        let bounds = Rect::from_points(&points);
        if rect.intersects(bounds) {
            faces.push(((a.1 + b.1 + c.1) / 3.0, points));
        }
    }
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    let fill = if selected {
        Color32::from_rgb(121, 106, 70)
    } else {
        Color32::from_rgb(65, 114, 145)
    };
    let edge = if selected {
        SELECTED
    } else {
        Color32::from_rgb(111, 162, 190)
    };
    for (_, points) in faces {
        paint_mesh_face(painter, points, fill, Stroke::new(0.5, edge));
    }
}

fn paint_mesh_face(painter: &Painter, points: [Pos2; 3], fill: Color32, stroke: Stroke) {
    // Joined strokes/AA polygon fringes produce long miters on skinny cap
    // triangles. Fill the exact triangle and stroke its three edges separately.
    let mut face = egui::Mesh::default();
    for point in points {
        face.colored_vertex(point, fill);
    }
    face.add_triangle(0, 1, 2);
    painter.add(Shape::mesh(face));
    for edge in [
        [points[0], points[1]],
        [points[1], points[2]],
        [points[2], points[0]],
    ] {
        painter.line_segment(edge, stroke);
    }
}

fn section_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(title.to_uppercase())
            .size(9.0)
            .strong()
            .color(MUTED),
    );
    ui.label(RichText::new(subtitle).size(10.0).color(MUTED));
}

fn entity_type(geometry: &Geometry) -> &'static str {
    match geometry {
        Geometry::Line { .. } => "Line",
        Geometry::Rectangle { .. } => "Rectangle",
        Geometry::Circle { .. } => "Circle",
        Geometry::Arc { .. } => "Arc",
        Geometry::Point { .. } => "Point",
        Geometry::Polyline { .. } => "Polyline",
        Geometry::Text { .. } => "Text",
        Geometry::Insert { .. } => "Block reference",
        Geometry::Dimension { .. } => "Dimension",
    }
}

fn paint_drafting_grid(painter: &Painter, rect: Rect, camera: Camera2d) {
    let step = if camera.zoom < 0.35 { 50.0 } else { 10.0 };
    let top_left = camera.screen_to_world(rect.left_top(), rect);
    let bottom_right = camera.screen_to_world(rect.right_bottom(), rect);
    let min_x = (top_left.x.min(bottom_right.x) / step).floor() as i32;
    let max_x = (top_left.x.max(bottom_right.x) / step).ceil() as i32;
    let min_y = (top_left.y.min(bottom_right.y) / step).floor() as i32;
    let max_y = (top_left.y.max(bottom_right.y) / step).ceil() as i32;

    for index in min_x..=max_x {
        let x = index as f64 * step;
        let start = camera.world_to_screen(Point::new(x, min_y as f64 * step), rect);
        let end = camera.world_to_screen(Point::new(x, max_y as f64 * step), rect);
        let major = index % 5 == 0;
        painter.line_segment(
            [start, end],
            Stroke::new(
                1.0,
                if major {
                    Color32::from_rgb(42, 52, 67)
                } else {
                    Color32::from_rgb(31, 39, 51)
                },
            ),
        );
    }
    for index in min_y..=max_y {
        let y = index as f64 * step;
        let start = camera.world_to_screen(Point::new(min_x as f64 * step, y), rect);
        let end = camera.world_to_screen(Point::new(max_x as f64 * step, y), rect);
        let major = index % 5 == 0;
        painter.line_segment(
            [start, end],
            Stroke::new(
                1.0,
                if major {
                    Color32::from_rgb(42, 52, 67)
                } else {
                    Color32::from_rgb(31, 39, 51)
                },
            ),
        );
    }

    let x_axis = camera.world_to_screen(Point::new(0.0, 0.0), rect);
    let x_end = camera.world_to_screen(Point::new(0.0, 1.0), rect);
    let y_end = camera.world_to_screen(Point::new(1.0, 0.0), rect);
    painter.line_segment(
        [
            Pos2::new(rect.left(), x_axis.y),
            Pos2::new(rect.right(), x_axis.y),
        ],
        Stroke::new(1.1, Color32::from_rgb(142, 85, 86)),
    );
    painter.line_segment(
        [
            Pos2::new(x_axis.x, rect.top()),
            Pos2::new(x_axis.x, rect.bottom()),
        ],
        Stroke::new(1.1, Color32::from_rgb(83, 135, 104)),
    );
    painter.circle_filled(x_axis, 2.4, Color32::from_rgb(192, 198, 205));
    let _ = (x_end, y_end);
}

fn paint_geometry(
    painter: &Painter,
    rect: Rect,
    camera: Camera2d,
    geometry: &Geometry,
    color: Color32,
    selected: bool,
) {
    let stroke = Stroke::new(if selected { 2.2 } else { 1.5 }, color);
    match geometry {
        Geometry::Line { start, end } => {
            painter.line_segment(
                [
                    camera.world_to_screen(*start, rect),
                    camera.world_to_screen(*end, rect),
                ],
                stroke,
            );
        }
        Geometry::Rectangle { a, b } => {
            let screen_a = camera.world_to_screen(*a, rect);
            let screen_b = camera.world_to_screen(*b, rect);
            let bounds = Rect::from_two_pos(screen_a, screen_b);
            painter.rect_stroke(bounds, 0.0, stroke, StrokeKind::Inside);
        }
        Geometry::Circle { center, radius } => {
            painter.circle_stroke(
                camera.world_to_screen(*center, rect),
                (*radius as f32 * camera.zoom).abs(),
                stroke,
            );
        }
        Geometry::Arc { .. } | Geometry::Polyline { .. } => {
            for (start, end) in geometry.layer_lines() {
                painter.line_segment(
                    [
                        camera.world_to_screen(start, rect),
                        camera.world_to_screen(end, rect),
                    ],
                    stroke,
                );
            }
        }
        Geometry::Point { position } => {
            let point = camera.world_to_screen(*position, rect);
            painter.circle_stroke(point, 3.0, stroke);
            painter.line_segment(
                [point - Vec2::new(5.0, 0.0), point + Vec2::new(5.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [point - Vec2::new(0.0, 5.0), point + Vec2::new(0.0, 5.0)],
                stroke,
            );
        }
        Geometry::Text {
            position,
            text,
            height,
            rotation_deg,
        } => {
            let point = camera.world_to_screen(*position, rect);
            let size = (*height as f32 * camera.zoom).clamp(1.0, 2048.0);
            let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(size), color);
            let angle = -(*rotation_deg as f32).to_radians();
            let offset = Vec2::new(-angle.sin() * size, angle.cos() * size);
            painter
                .add(egui::epaint::TextShape::new(point - offset, galley, color).with_angle(angle));
        }
        Geometry::Insert { position, .. } => {
            // Standalone marker. The drafting canvas paints expanded block
            // contents so instances appear as their real geometry.
            let point = camera.world_to_screen(*position, rect);
            painter.circle_stroke(point, 3.0, stroke);
            painter.circle_filled(point, 1.2, color);
        }
        Geometry::Dimension { dimension } => {
            // Annotation strokes and text are derived from the semantic dimension.
            if let Ok(primitives) = dimension.annotation_geometry() {
                for primitive in &primitives {
                    paint_geometry(painter, rect, camera, primitive, color, selected);
                }
            }
        }
    }
}

fn paint_axis_badge(painter: &Painter, rect: Rect) {
    let origin = rect.left_bottom() + Vec2::new(34.0, -34.0);
    painter.circle_filled(origin, 2.5, Color32::from_rgb(207, 216, 226));
    painter.line_segment(
        [origin, origin + Vec2::new(21.0, 0.0)],
        Stroke::new(2.0, Color32::from_rgb(201, 104, 107)),
    );
    painter.line_segment(
        [origin, origin + Vec2::new(0.0, -21.0)],
        Stroke::new(2.0, Color32::from_rgb(107, 186, 137)),
    );
    painter.text(
        origin + Vec2::new(24.0, 0.0),
        Align2::LEFT_CENTER,
        "X",
        FontId::proportional(8.0),
        MUTED,
    );
    painter.text(
        origin + Vec2::new(0.0, -24.0),
        Align2::CENTER_BOTTOM,
        "Y",
        FontId::proportional(8.0),
        MUTED,
    );
}

fn paint_model_grid(painter: &Painter, rect: Rect, camera: Camera3d) {
    let color = Color32::from_rgb(36, 45, 59);
    for index in -12..=12 {
        let offset = index as f64 * 40.0;
        let a = camera.project([offset, -480.0, 0.0], rect).0;
        let b = camera.project([offset, 480.0, 0.0], rect).0;
        painter.line_segment([a, b], Stroke::new(1.0, color));
        let c = camera.project([-480.0, offset, 0.0], rect).0;
        let d = camera.project([480.0, offset, 0.0], rect).0;
        painter.line_segment([c, d], Stroke::new(1.0, color));
    }
    let origin = camera.project([0.0, 0.0, 0.0], rect).0;
    painter.circle_filled(origin, 2.5, Color32::from_rgb(206, 215, 226));
}

fn paint_solid(painter: &Painter, rect: Rect, camera: Camera3d, solid: &Solid, selected: bool) {
    match solid.kind {
        SolidKind::Box => paint_box(painter, rect, camera, solid, selected),
        SolidKind::Cylinder => paint_cylinder(painter, rect, camera, solid, selected),
        SolidKind::Sphere => paint_sphere(painter, rect, camera, solid, selected),
    }
}

fn paint_box(painter: &Painter, rect: Rect, camera: Camera3d, solid: &Solid, selected: bool) {
    let [cx, cy, cz] = solid.center;
    let [sx, sy, sz] = solid.size;
    let [hx, hy, hz] = [sx.abs() / 2.0, sy.abs() / 2.0, sz.abs() / 2.0];
    let vertices = [
        [cx - hx, cy - hy, cz - hz],
        [cx + hx, cy - hy, cz - hz],
        [cx + hx, cy + hy, cz - hz],
        [cx - hx, cy + hy, cz - hz],
        [cx - hx, cy - hy, cz + hz],
        [cx + hx, cy - hy, cz + hz],
        [cx + hx, cy + hy, cz + hz],
        [cx - hx, cy + hy, cz + hz],
    ];
    let projected: Vec<(Pos2, f64)> = vertices
        .into_iter()
        .map(|point| camera.project(point, rect))
        .collect();
    let faces: [[usize; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let shades = [
        Color32::from_rgb(45, 89, 120),
        Color32::from_rgb(78, 147, 190),
        Color32::from_rgb(55, 110, 148),
        Color32::from_rgb(47, 98, 134),
        Color32::from_rgb(37, 77, 106),
        Color32::from_rgb(50, 105, 143),
    ];
    let mut ordered: Vec<(f64, usize)> = faces
        .iter()
        .enumerate()
        .map(|(index, face)| {
            let depth = face.iter().map(|vertex| projected[*vertex].1).sum::<f64>() / 4.0;
            (depth, index)
        })
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, index) in ordered {
        let points: Vec<Pos2> = faces[index]
            .iter()
            .map(|vertex| projected[*vertex].0)
            .collect();
        let fill = if selected {
            Color32::from_rgb(124, 106, 63)
        } else {
            shades[index]
        };
        let edge = if selected {
            SELECTED
        } else {
            Color32::from_rgb(144, 202, 235)
        };
        painter.add(Shape::convex_polygon(
            points.clone(),
            fill,
            Stroke::new(1.0, edge),
        ));
        painter.add(Shape::closed_line(points, Stroke::new(1.0, edge)));
    }
}

fn paint_cylinder(painter: &Painter, rect: Rect, camera: Camera3d, solid: &Solid, selected: bool) {
    let [cx, cy, cz] = solid.center;
    let radius_x = solid.size[0].abs() / 2.0;
    let radius_y = solid.size[1].abs() / 2.0;
    let half_height = solid.size[2].abs() / 2.0;
    let color = if selected {
        SELECTED
    } else {
        Color32::from_rgb(116, 184, 221)
    };
    let segments = 40;
    let mut lower = Vec::with_capacity(segments + 1);
    let mut upper = Vec::with_capacity(segments + 1);
    for step in 0..=segments {
        let angle = std::f64::consts::TAU * step as f64 / segments as f64;
        let x = cx + angle.cos() * radius_x;
        let y = cy + angle.sin() * radius_y;
        lower.push(camera.project([x, y, cz - half_height], rect).0);
        upper.push(camera.project([x, y, cz + half_height], rect).0);
    }
    painter.add(Shape::line(lower.clone(), Stroke::new(1.5, color)));
    painter.add(Shape::line(upper.clone(), Stroke::new(1.7, color)));
    for index in (0..segments).step_by(5) {
        painter.line_segment([lower[index], upper[index]], Stroke::new(1.3, color));
    }
}

fn paint_sphere(painter: &Painter, rect: Rect, camera: Camera3d, solid: &Solid, selected: bool) {
    let [cx, cy, cz] = solid.center;
    let radius = solid.size[0]
        .abs()
        .min(solid.size[1].abs())
        .min(solid.size[2].abs())
        / 2.0;
    let color = if selected {
        SELECTED
    } else {
        Color32::from_rgb(145, 177, 205)
    };
    let segments = 48;
    for plane in 0..3 {
        let points = (0..=segments)
            .map(|step| {
                let angle = std::f64::consts::TAU * step as f64 / segments as f64;
                let u = angle.cos() * radius;
                let v = angle.sin() * radius;
                let point = match plane {
                    0 => [cx + u, cy + v, cz],
                    1 => [cx + u, cy, cz + v],
                    _ => [cx, cy + u, cz + v],
                };
                camera.project(point, rect).0
            })
            .collect();
        painter.add(Shape::line(
            points,
            Stroke::new(if plane == 0 { 1.8 } else { 1.0 }, color),
        ));
    }
}

#[cfg(test)]
mod application_tests {
    use super::*;
    #[test]
    fn model_projection_places_positive_z_up_and_nearer_the_viewer() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let camera = Camera3d::default();
        let origin = camera.project([0.0, 0.0, 0.0], rect);
        let top = camera.project([0.0, 0.0, 100.0], rect);
        assert!(top.0.y < origin.0.y);
        assert!(top.1 > origin.1);
    }
    #[test]
    fn polyline_authoring_is_a_single_undoable_geometry_operation() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::Polyline);
        for point in [
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 30.0),
        ] {
            app.construction_click(point);
        }
        assert!(app.document.entities.is_empty());
        app.polyline_closed = true;
        app.finish_polyline();
        assert!(matches!(
            app.document.entities[0].geometry,
            Geometry::Polyline { closed: true, .. }
        ));
        assert_eq!(app.undo_stack.len(), 1);
        assert!(app.construction_points.is_empty());
        app.undo();
        assert!(app.document.entities.is_empty());
        app.redo();
        assert_eq!(app.document.entities.len(), 1);
    }
    #[test]
    fn arc_authoring_uses_center_radius_and_counterclockwise_end_angle() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::Arc);
        for point in [
            Point::new(10.0, 10.0),
            Point::new(20.0, 10.0),
            Point::new(10.0, 30.0),
        ] {
            app.construction_click(point);
        }
        assert_eq!(
            app.document.entities[0].geometry,
            Geometry::Arc {
                center: Point::new(10.0, 10.0),
                radius: 10.0,
                start_angle_deg: 0.0,
                end_angle_deg: 90.0
            }
        );
        assert!(app.construction_points.is_empty());
    }
    #[test]
    fn text_and_point_authoring_validate_before_adding_history() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::Text);
        app.draft_text = "Room 101".into();
        app.draft_text_height = 12.0;
        app.construction_click(Point::new(10.0, 20.0));
        assert!(
            matches!(&app.document.entities[0].geometry,Geometry::Text{text,height,..} if text=="Room 101" && *height==12.0)
        );
        app.draft_text_height = 0.0;
        app.construction_click(Point::default());
        assert_eq!(app.document.entities.len(), 1);
        assert_eq!(app.undo_stack.len(), 1);
        app.set_draft_tool(DraftTool::Point);
        app.construction_click(Point::new(5.0, 7.0));
        assert_eq!(app.document.entities.len(), 2);
        app.construction_click(Point::new(f64::NAN, 0.0));
        assert_eq!(app.document.entities.len(), 2);
    }
    #[test]
    fn changing_tools_cancels_uncommitted_construction() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::Polyline);
        app.construction_click(Point::default());
        app.set_draft_tool(DraftTool::Circle);
        assert!(app.construction_points.is_empty());
        assert!(app.document.entities.is_empty());
        assert!(app.undo_stack.is_empty());
    }
    #[test]
    fn selection_edits_have_one_reversible_history_entry() {
        let mut app = ForgeApp::default();
        let id = app.document.add_geometry(Geometry::Line {
            start: Point::default(),
            end: Point::new(10.0, 0.0),
        });
        app.selected_entities.insert(id);
        app.apply_selection_transform(Transform2d::translation(12.0, 5.0));
        assert_eq!(app.undo_stack.len(), 1);
        assert!(app.dirty);
        assert_eq!(
            app.document.entities[0].geometry.bounds().unwrap().min,
            Point::new(12.0, 5.0)
        );
        app.undo();
        assert_eq!(
            app.document.entities[0].geometry.bounds().unwrap().min,
            Point::default()
        );
        app.redo();
        assert_eq!(
            app.document.entities[0].geometry.bounds().unwrap().min,
            Point::new(12.0, 5.0)
        );
    }
    #[test]
    fn invalid_edit_leaves_document_and_history_unchanged() {
        let mut app = ForgeApp::default();
        let id = app.document.add_geometry(Geometry::Circle {
            center: Point::default(),
            radius: 2.0,
        });
        app.selected_entities.insert(id);
        let before = app.document.clone();
        app.apply_selection_transform(Transform2d::uniform_scale(0.0, Point::default()));
        assert_eq!(app.document, before);
        assert!(app.undo_stack.is_empty());
        assert!(app.status_is_error);
    }
    #[test]
    fn new_action_waits_for_unsaved_decision() {
        let mut app = ForgeApp::default();
        app.document.add_geometry(Geometry::Point {
            position: Point::new(7.0, 3.0),
        });
        app.dirty = true;
        app.request_action(PendingAction::New);
        assert!(matches!(app.pending_action, Some(PendingAction::New)));
        assert_eq!(app.document.entities.len(), 1);
    }
    #[test]
    fn mesh_export_cannot_silently_discard_drawing_or_primitive_history() {
        let mut app = ForgeApp::starting_at(Workspace::Model);
        app.document.add_solid("Part", SolidKind::Box);
        app.document.add_geometry(Geometry::Point {
            position: Point::default(),
        });
        app.file_path = "target/ui-export-tests/unwritten.forge.json".into();
        app.export_current();
        let export = app
            .pending_export
            .as_ref()
            .expect("loss confirmation is required");
        assert!(export.losses.iter().any(|s| s.contains("2D")));
        assert!(export.losses.iter().any(|s| s.contains("parameters")));
        assert!(!export.path.exists());
        assert_eq!(app.document.entities.len(), 1);
    }
    #[test]
    fn individual_named_targets_keep_distinct_identity_and_routing() {
        for (id, area) in [
            ("autocad", Workspace::Draft),
            ("inventor", Workspace::Model),
            ("revit", Workspace::Build),
            ("maya", Workspace::Motion),
            ("civil-3d", Workspace::Terrain),
        ] {
            let product = catalog::product(id).unwrap();
            assert_eq!(workspace_for_area(product.area), area);
        }
        assert_ne!(
            catalog::product("autocad").unwrap().id,
            catalog::product("autocad-lt").unwrap().id
        );
    }

    fn authored_dimension(app: &ForgeApp, index: usize) -> Dimension {
        match &app.document.entities[index].geometry {
            Geometry::Dimension { dimension } => dimension.clone(),
            other => panic!("expected a dimension, found {other:?}"),
        }
    }

    #[test]
    fn linear_dimension_authoring_records_the_semantic_measurement() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::Dimension);
        for point in [
            Point::new(0.0, 0.0),
            Point::new(40.0, 7.0),
            Point::new(0.0, 12.0),
        ] {
            app.construction_click(point);
        }
        assert_eq!(app.document.entities.len(), 1);
        assert!(app.construction_points.is_empty());
        assert_eq!(app.undo_stack.len(), 1);
        let dimension = authored_dimension(&app, 0);
        assert_eq!(dimension.measurement().unwrap(), 40.0);
        assert_eq!(dimension.text().unwrap(), "40.00");
        assert!(!dimension.annotation_geometry().unwrap().is_empty());
        app.undo();
        assert!(app.document.entities.is_empty());
        app.redo();
        assert_eq!(app.document.entities.len(), 1);
    }

    #[test]
    fn dimension_kind_selects_the_measured_projection() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::Dimension);
        app.dimension_kind = LinearDimensionKind::Vertical;
        for point in [
            Point::new(0.0, 0.0),
            Point::new(40.0, 30.0),
            Point::new(10.0, 0.0),
        ] {
            app.construction_click(point);
        }
        assert_eq!(authored_dimension(&app, 0).measurement().unwrap(), 30.0);
    }

    #[test]
    fn radial_dimension_authoring_measures_radius_and_diameter() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::RadialDimension);
        app.construction_click(Point::new(0.0, 0.0));
        app.construction_click(Point::new(12.0, 0.0));
        assert_eq!(authored_dimension(&app, 0).text().unwrap(), "R12.00");
        app.dimension_diameter = true;
        app.construction_click(Point::new(0.0, 0.0));
        app.construction_click(Point::new(12.0, 0.0));
        assert_eq!(app.document.entities.len(), 2);
        // A diameter dimension measures the full circle, not the picked radius.
        assert_eq!(authored_dimension(&app, 1).text().unwrap(), "⌀24.00");
    }

    #[test]
    fn degenerate_dimension_references_are_refused_without_history() {
        let mut app = ForgeApp::default();
        app.set_draft_tool(DraftTool::Dimension);
        app.construction_click(Point::new(0.0, 0.0));
        app.construction_click(Point::new(0.0, 0.0));
        assert!(app.document.entities.is_empty());
        assert!(app.status_is_error);
        app.set_draft_tool(DraftTool::RadialDimension);
        app.construction_click(Point::new(3.0, 3.0));
        app.construction_click(Point::new(3.0, 3.0));
        assert!(app.document.entities.is_empty());
        assert!(app.status_is_error);
        assert!(app.undo_stack.is_empty());
    }

    #[test]
    fn block_definition_picks_and_explodes_through_expanded_geometry() {
        let mut app = ForgeApp::default();
        let line = app.document.add_geometry(Geometry::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(20.0, 0.0),
        });
        let circle = app.document.add_geometry(Geometry::Circle {
            center: Point::new(10.0, 10.0),
            radius: 5.0,
        });
        app.selected_entities = [line, circle].into_iter().collect();
        app.new_block_name = "Chair".into();
        app.define_block_from_selection();
        assert_eq!(app.document.entities.len(), 1);
        assert_eq!(app.document.blocks.len(), 1);
        assert_eq!(app.document.blocks[0].entities.len(), 2);
        let reference = app.document.entities[0].id;
        assert!(
            matches!(&app.document.entities[0].geometry, Geometry::Insert { block_name, .. } if block_name == "Chair")
        );
        // Clicking the expanded line selects the block reference itself.
        app.selected_entities.clear();
        app.select_entity_at(Point::new(10.0, 0.0), false);
        assert_eq!(
            app.selected_entities.iter().copied().collect::<Vec<_>>(),
            vec![reference]
        );
        assert!(!app.expanded_for_display().unwrap().is_empty());
        app.explode_selected_references();
        assert_eq!(app.document.entities.len(), 2);
        assert_eq!(app.selected_entities.len(), 2);
        app.document.validate().unwrap();
        app.undo();
        assert_eq!(app.document.entities.len(), 1);
    }

    #[test]
    fn invalid_block_definitions_leave_the_document_and_history_unchanged() {
        let mut app = ForgeApp::default();
        app.define_block_from_selection();
        assert!(app.status_is_error);
        assert!(app.undo_stack.is_empty());
        let first = app.document.add_geometry(Geometry::Line {
            start: Point::default(),
            end: Point::new(10.0, 0.0),
        });
        app.selected_entities.insert(first);
        app.new_block_name = "Part".into();
        app.define_block_from_selection();
        assert_eq!(app.document.blocks.len(), 1);
        let second = app.document.add_geometry(Geometry::Line {
            start: Point::default(),
            end: Point::new(0.0, 10.0),
        });
        app.selected_entities.clear();
        app.selected_entities.insert(second);
        app.define_block_from_selection();
        assert_eq!(app.document.blocks.len(), 1);
        assert_eq!(app.document.entities.len(), 2);
        assert!(app.status_is_error);
        app.explode_selected_references();
        assert!(app.status_is_error);
        assert_eq!(app.document.entities.len(), 2);
    }
}
