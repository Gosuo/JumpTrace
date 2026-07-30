use std::path::Path;

use eframe::egui;

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
    JumpTunnel,
}

struct JumpTraceApp {
    universe: Result<universe::Universe, String>,
    screenshot: Option<egui::TextureHandle>,
    screenshot_name: Option<String>,
    load_error: Option<String>,
    north_axis: Option<NormalizedLine>,
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
            screenshot: None,
            screenshot_name: None,
            load_error: None,
            north_axis: None,
            jump_tunnel: None,
            annotation_tool: AnnotationTool::NorthAxis,
            drawing_tool: None,
            zoom: 1.0,
            scroll_offset: egui::Vec2::ZERO,
        }
    }
}

impl JumpTraceApp {
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
                AnnotationTool::NorthAxis => "Drag from the axis origin toward north.",
                AnnotationTool::JumpTunnel => "Drag in the direction of the jump tunnel.",
            };
            ui.strong(format!(
                "Active tool: {}",
                match self.annotation_tool {
                    AnnotationTool::NorthAxis => "North axis",
                    AnnotationTool::JumpTunnel => "Jump tunnel",
                }
            ));
            ui.label(instruction);
            ui.small("Zoom with Ctrl/Cmd + mouse wheel or a trackpad pinch over the image.");

            match jump_bearing(self.north_axis, self.jump_tunnel) {
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
                        && completed_tool == AnnotationTool::NorthAxis
                    {
                        self.annotation_tool = AnnotationTool::JumpTunnel;
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
        AnnotationTool::JumpTunnel => &mut app.jump_tunnel,
    }
}

fn jump_bearing(
    north_axis: Option<NormalizedLine>,
    jump_tunnel: Option<NormalizedLine>,
) -> Option<f32> {
    let north = north_axis?.end - north_axis?.start;
    let jump = jump_tunnel?.end - jump_tunnel?.start;

    if north.length_sq() <= f32::EPSILON || jump.length_sq() <= f32::EPSILON {
        return None;
    }

    let cross = north.x * jump.y - north.y * jump.x;
    let dot = north.dot(jump);
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

        assert_eq!(jump_bearing(Some(north), Some(north)), Some(0.0));
        assert_eq!(jump_bearing(Some(north), Some(east)), Some(90.0));
        assert_eq!(jump_bearing(Some(north), Some(west)), Some(270.0));
    }

    #[test]
    fn zero_length_annotation_has_no_bearing() {
        let north = line(egui::vec2(0.0, -1.0));
        let zero = line(egui::Vec2::ZERO);

        assert_eq!(jump_bearing(Some(north), Some(zero)), None);
    }
}
