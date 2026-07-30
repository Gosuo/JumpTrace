use std::path::Path;

use eframe::egui;

mod jump_range;
mod projection;
pub mod universe;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "JumpTrace",
        options,
        Box::new(|_creation_context| Ok(Box::new(JumpTraceApp::default()))),
    )
}

#[derive(Clone, Copy)]
struct NormalizedLine {
    start: egui::Pos2,
    end: egui::Pos2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnnotationTool {
    NorthAxis,
    RightAxis,
    JumpTunnel,
}

struct JumpTraceApp {
    universe: Result<universe::Universe, String>,
    system_query: String,
    current_system_id: Option<u32>,
    jump_ship_class: jump_range::JumpShipClass,
    screenshot: Option<egui::TextureHandle>,
    screenshot_name: Option<String>,
    load_error: Option<String>,
    north_axis: Option<NormalizedLine>,
    right_axis: Option<NormalizedLine>,
    jump_tunnel: Option<NormalizedLine>,
    annotation_tool: AnnotationTool,
    drawing_tool: Option<AnnotationTool>,
    zoom: f32,
    scroll_offset: egui::Vec2,
}

impl Default for JumpTraceApp {
    fn default() -> Self {
        Self {
            universe: universe::Universe::load_embedded(),
            system_query: String::new(),
            current_system_id: None,
            jump_ship_class: jump_range::JumpShipClass::JumpFreighterRorqual,
            screenshot: None,
            screenshot_name: None,
            load_error: None,
            north_axis: None,
            right_axis: None,
            jump_tunnel: None,
            annotation_tool: AnnotationTool::NorthAxis,
            drawing_tool: None,
            zoom: 1.0,
            scroll_offset: egui::Vec2::ZERO,
        }
    }
}

impl JumpTraceApp {
    fn system_selector(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Current system:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.system_query)
                        .hint_text("Type a solar-system name")
                        .desired_width(240.0),
                );

                if response.changed() {
                    self.current_system_id = None;
                }
            });

            let suggestions: Vec<_> = match &self.universe {
                Ok(universe)
                    if self.current_system_id.is_none() && !self.system_query.trim().is_empty() =>
                {
                    universe
                        .search_systems(&self.system_query, 8)
                        .into_iter()
                        .map(|system| (system.id, system.name.clone()))
                        .collect()
                }
                _ => Vec::new(),
            };

            if !suggestions.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt("system_suggestions")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for (id, name) in suggestions {
                            if ui.selectable_label(false, &name).clicked() {
                                self.current_system_id = Some(id);
                                self.system_query = name;
                            }
                        }
                    });
            } else if self.current_system_id.is_none() && !self.system_query.trim().is_empty() {
                ui.small("No matching solar systems.");
            }

            if let (Ok(universe), Some(id)) = (&self.universe, self.current_system_id)
                && let Some(system) = universe.system(id)
            {
                ui.small(format!(
                    "Selected: {} · ID {} · Security {:.1}",
                    system.name, system.id, system.security
                ));
            }
        });
    }

    fn jump_range_selector(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Jump ship:");
                egui::ComboBox::from_id_salt("jump_ship_class")
                    .selected_text(self.jump_ship_class.label())
                    .show_ui(ui, |ui| {
                        for ship_class in jump_range::JumpShipClass::ALL {
                            ui.selectable_value(
                                &mut self.jump_ship_class,
                                ship_class,
                                ship_class.label(),
                            );
                        }
                    });
            });

            ui.small(format!(
                "Maximum range: {:.2} ly (JDC V)",
                self.jump_ship_class.max_range_ly()
            ));
        });
    }

    fn reachable_systems_panel(&self, ui: &mut egui::Ui) {
        let (universe, origin) = match (&self.universe, self.current_system_id) {
            (Ok(universe), Some(id)) => {
                let Some(origin) = universe.system(id) else {
                    return;
                };
                (universe, origin)
            }
            _ => {
                ui.small("Select a current system to calculate one-jump destinations.");
                return;
            }
        };

        let Some(image_size) = self.screenshot.as_ref().map(egui::TextureHandle::size) else {
            ui.small("Open a screenshot to rank possible destinations.");
            return;
        };
        let (Some(north), Some(right_axis), Some(tunnel)) =
            (self.north_axis, self.right_axis, self.jump_tunnel)
        else {
            ui.small("Draw the north, right 200 km, and jump-tunnel arrows to rank destinations.");
            return;
        };
        let calibration = projection::ScreenCalibration {
            north: image_pixel_vector(north, image_size),
            right_axis: image_pixel_vector(right_axis, image_size),
        };
        let tunnel_vector = image_pixel_vector(tunnel, image_size);

        let maximum_range = self.jump_ship_class.max_range_ly();
        let candidates = universe.systems_matching_jump_bearing(
            origin,
            maximum_range,
            calibration,
            tunnel_vector,
        );
        let heading = format!(
            "Ranked destinations from {}: {} within {:.2} ly",
            origin.name,
            candidates.len(),
            maximum_range
        );

        egui::CollapsingHeader::new(heading)
            .id_salt("reachable_systems")
            .default_open(true)
            .show(ui, |ui| {
                ui.small("Ranked by projecting each 3D SDE vector through the manually marked tactical-overlay axes.");
                ui.small("Excludes high-sec, wormhole space, and Pochven; dynamic cyno restrictions are not included.");
                egui::ScrollArea::vertical()
                    .id_salt("reachable_system_list")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for candidate in candidates {
                            ui.label(format!(
                                "{} · error {:.1}° · {:.3} ly · security {:.1}",
                                candidate.system.name,
                                candidate.angular_error_deg,
                                candidate.distance_ly,
                                candidate.system.security
                            ));
                        }
                    });
            });
    }

    fn choose_screenshot(&mut self, context: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp"])
            .pick_file()
        else {
            return;
        };

        match load_texture(context, &path) {
            Ok(texture) => {
                self.screenshot = Some(texture);
                self.screenshot_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned());
                self.load_error = None;
                self.north_axis = None;
                self.right_axis = None;
                self.jump_tunnel = None;
                self.annotation_tool = AnnotationTool::NorthAxis;
                self.drawing_tool = None;
                self.zoom = 1.0;
                self.scroll_offset = egui::Vec2::ZERO;
            }
            Err(error) => {
                self.load_error = Some(error);
            }
        }
    }
}

impl eframe::App for JumpTraceApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("JumpTrace");
        ui.label("Jump destination analysis for EVE Online");
        match &self.universe {
            Ok(universe) => {
                ui.small(format!(
                    "Embedded universe data: {} solar systems",
                    universe.systems.len()
                ));
            }
            Err(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Could not load embedded universe data: {error}"),
                );
            }
        }

        self.system_selector(ui);
        self.jump_range_selector(ui);
        self.reachable_systems_panel(ui);

        ui.horizontal(|ui| {
            if ui.button("Open screenshot…").clicked() {
                self.choose_screenshot(ui.ctx());
            }

            if let Some(name) = &self.screenshot_name {
                ui.label(name);
            }
        });

        if let Some(error) = &self.load_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }

        ui.separator();

        if let Some(texture) = self.screenshot.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Draw:");
                ui.selectable_value(
                    &mut self.annotation_tool,
                    AnnotationTool::NorthAxis,
                    "North axis",
                );
                ui.selectable_value(
                    &mut self.annotation_tool,
                    AnnotationTool::RightAxis,
                    "Right axis (200 km)",
                );
                ui.selectable_value(
                    &mut self.annotation_tool,
                    AnnotationTool::JumpTunnel,
                    "Jump tunnel",
                );

                ui.separator();
                ui.label("Zoom:");
                ui.add(egui::Slider::new(&mut self.zoom, 0.5..=4.0).logarithmic(true));
                if ui.button("Reset zoom").clicked() {
                    self.zoom = 1.0;
                    self.scroll_offset = egui::Vec2::ZERO;
                }

                ui.separator();
                if ui
                    .add_enabled(self.north_axis.is_some(), egui::Button::new("Clear north"))
                    .clicked()
                {
                    self.north_axis = None;
                }
                if ui
                    .add_enabled(self.right_axis.is_some(), egui::Button::new("Clear right"))
                    .clicked()
                {
                    self.right_axis = None;
                }
                if ui
                    .add_enabled(
                        self.jump_tunnel.is_some(),
                        egui::Button::new("Clear tunnel"),
                    )
                    .clicked()
                {
                    self.jump_tunnel = None;
                }
            });

            let instruction = match self.annotation_tool {
                AnnotationTool::NorthAxis => {
                    "Drag from the overlay center to the north 200 km marker."
                }
                AnnotationTool::RightAxis => {
                    "Drag from the same center to the right-side 200 km marker."
                }
                AnnotationTool::JumpTunnel => "Drag in the direction of the jump tunnel.",
            };
            ui.strong(format!(
                "Active tool: {}",
                match self.annotation_tool {
                    AnnotationTool::NorthAxis => "North axis (200 km)",
                    AnnotationTool::RightAxis => "Right axis (200 km)",
                    AnnotationTool::JumpTunnel => "Jump tunnel",
                }
            ));
            ui.label(instruction);
            ui.small("Zoom with Ctrl/Cmd + mouse wheel or a trackpad pinch over the image.");

            match jump_bearing(self.north_axis, self.jump_tunnel, texture.size()) {
                Some(bearing) => {
                    ui.label(format!("Jump bearing: {bearing:.1}° clockwise from north"));
                }
                None => {
                    ui.label("Jump bearing: draw both arrows");
                }
            }

            let viewport_size = ui.available_size();
            let image_size = texture.size_vec2();
            let fit_scale = (viewport_size.x / image_size.x)
                .min(viewport_size.y / image_size.y)
                .min(1.0);
            let display_size = image_size * fit_scale.max(0.0) * self.zoom;

            let zoom_factor = ui.input(|input| input.zoom_delta());
            let scroll_source = egui::scroll_area::ScrollSource {
                drag: egui::scroll_area::DragScroll::Never,
                ..Default::default()
            };
            let scroll_output = egui::ScrollArea::both()
                .id_salt("screenshot_scroll")
                .scroll_offset(self.scroll_offset)
                .scroll_source(scroll_source)
                .show(ui, |ui| {
                    let response = ui.add(
                        egui::Image::new(&texture)
                            .fit_to_exact_size(display_size)
                            .sense(egui::Sense::drag()),
                    );

                    let zoom_anchor = response
                        .hovered()
                        .then(|| ui.input(|input| input.pointer.hover_pos()))
                        .flatten()
                        .map(|pointer| (pointer, response.rect));

                    if response.drag_started()
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        let point = normalize_point(pointer, response.rect);
                        let tool = self.annotation_tool;
                        self.drawing_tool = Some(tool);
                        *line_for_tool(self, tool) = Some(NormalizedLine {
                            start: point,
                            end: point,
                        });
                    }

                    if response.dragged()
                        && let (Some(pointer), Some(tool)) =
                            (response.interact_pointer_pos(), self.drawing_tool)
                        && let Some(line) = line_for_tool(self, tool)
                    {
                        line.end = normalize_point(pointer, response.rect);
                    }

                    if response.drag_stopped()
                        && let Some(completed_tool) = self.drawing_tool.take()
                    {
                        self.annotation_tool = match completed_tool {
                            AnnotationTool::NorthAxis => AnnotationTool::RightAxis,
                            AnnotationTool::RightAxis => AnnotationTool::JumpTunnel,
                            AnnotationTool::JumpTunnel => AnnotationTool::JumpTunnel,
                        };
                    }

                    if let Some(axis) = self.north_axis {
                        paint_annotation(
                            ui.painter(),
                            response.rect,
                            axis,
                            "North",
                            egui::Color32::LIGHT_GREEN,
                        );
                    }
                    if let Some(axis) = self.right_axis {
                        paint_annotation(
                            ui.painter(),
                            response.rect,
                            axis,
                            "Right 200",
                            egui::Color32::YELLOW,
                        );
                    }
                    if let Some(tunnel) = self.jump_tunnel {
                        paint_annotation(
                            ui.painter(),
                            response.rect,
                            tunnel,
                            "Jump",
                            egui::Color32::LIGHT_BLUE,
                        );
                    }

                    zoom_anchor
                });

            self.scroll_offset = scroll_output.state.offset;
            if zoom_factor != 1.0
                && let Some((pointer, image_rect)) = scroll_output.inner
            {
                let new_zoom = (self.zoom * zoom_factor).clamp(0.5, 4.0);
                let effective_factor = new_zoom / self.zoom;
                let image_position = pointer - image_rect.min;
                let viewport_position = pointer - scroll_output.inner_rect.min;
                let desired_offset = image_position * effective_factor - viewport_position;
                let new_content_size = display_size * effective_factor;
                let max_offset = new_content_size - scroll_output.inner_rect.size();

                self.zoom = new_zoom;
                self.scroll_offset = egui::vec2(
                    desired_offset.x.clamp(0.0, max_offset.x.max(0.0)),
                    desired_offset.y.clamp(0.0, max_offset.y.max(0.0)),
                );
            }
        } else {
            ui.label("Open an image file to begin.");
        }
    }
}

fn normalize_point(point: egui::Pos2, rect: egui::Rect) -> egui::Pos2 {
    let point = rect.clamp(point);
    egui::pos2(
        (point.x - rect.left()) / rect.width(),
        (point.y - rect.top()) / rect.height(),
    )
}

fn denormalize_point(point: egui::Pos2, rect: egui::Rect) -> egui::Pos2 {
    rect.min + egui::vec2(point.x * rect.width(), point.y * rect.height())
}

fn line_for_tool(app: &mut JumpTraceApp, tool: AnnotationTool) -> &mut Option<NormalizedLine> {
    match tool {
        AnnotationTool::NorthAxis => &mut app.north_axis,
        AnnotationTool::RightAxis => &mut app.right_axis,
        AnnotationTool::JumpTunnel => &mut app.jump_tunnel,
    }
}

fn image_pixel_vector(line: NormalizedLine, image_size: [usize; 2]) -> [f64; 2] {
    [
        f64::from(line.end.x - line.start.x) * image_size[0] as f64,
        f64::from(line.end.y - line.start.y) * image_size[1] as f64,
    ]
}

fn jump_bearing(
    north_axis: Option<NormalizedLine>,
    jump_tunnel: Option<NormalizedLine>,
    image_size: [usize; 2],
) -> Option<f64> {
    let north = image_pixel_vector(north_axis?, image_size);
    let jump = image_pixel_vector(jump_tunnel?, image_size);
    let north_length = north[0].hypot(north[1]);
    let jump_length = jump[0].hypot(jump[1]);

    if north_length <= f64::EPSILON || jump_length <= f64::EPSILON {
        return None;
    }

    let cross = north[0] * jump[1] - north[1] * jump[0];
    let dot = north[0] * jump[0] + north[1] * jump[1];
    Some(cross.atan2(dot).to_degrees().rem_euclid(360.0))
}

fn paint_annotation(
    painter: &egui::Painter,
    image_rect: egui::Rect,
    line: NormalizedLine,
    label: &str,
    color: egui::Color32,
) {
    let start = denormalize_point(line.start, image_rect);
    let end = denormalize_point(line.end, image_rect);
    let stroke = egui::Stroke::new(3.0, color);

    painter.circle_filled(start, 5.0, color);
    painter.arrow(start, end - start, stroke);
    painter.text(
        end,
        egui::Align2::LEFT_BOTTOM,
        label,
        egui::FontId::proportional(16.0),
        color,
    );
}

fn load_texture(context: &egui::Context, path: &Path) -> Result<egui::TextureHandle, String> {
    let image = image::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());

    Ok(context.load_texture(
        path.to_string_lossy(),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(vector: egui::Vec2) -> NormalizedLine {
        NormalizedLine {
            start: egui::Pos2::ZERO,
            end: egui::Pos2::ZERO + vector,
        }
    }

    #[test]
    fn bearing_is_clockwise_from_north() {
        let north = line(egui::vec2(0.0, -1.0));
        let east = line(egui::vec2(1.0, 0.0));
        let west = line(egui::vec2(-1.0, 0.0));

        let image_size = [100, 100];
        assert_eq!(
            jump_bearing(Some(north), Some(north), image_size),
            Some(0.0)
        );
        assert_eq!(
            jump_bearing(Some(north), Some(east), image_size),
            Some(90.0)
        );
        assert_eq!(
            jump_bearing(Some(north), Some(west), image_size),
            Some(270.0)
        );
    }

    #[test]
    fn zero_length_annotation_has_no_bearing() {
        let north = line(egui::vec2(0.0, -1.0));
        let zero = line(egui::Vec2::ZERO);

        assert_eq!(jump_bearing(Some(north), Some(zero), [100, 100]), None);
    }

    #[test]
    fn bearing_accounts_for_image_aspect_ratio() {
        let north = line(egui::vec2(0.0, -0.1));
        let jump = line(egui::vec2(0.1, -0.1));
        let bearing =
            jump_bearing(Some(north), Some(jump), [200, 100]).expect("lines should have a bearing");

        assert!((bearing - 63.434_948_822_922_01).abs() < 1e-9);
    }
}
