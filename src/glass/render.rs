use super::{
    assets,
    scene::{Node, Transform},
    Control,
};
use eframe::egui::{
    self,
    epaint::{Mesh, Vertex},
    pos2, vec2, Color32, ColorImage, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke,
    TextureHandle, TextureId, TextureOptions,
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
};

pub struct Hit {
    pub transform: Transform,
    pub size: egui::Vec2,
    pub clip: Rect,
    pub control: Control,
}

impl Hit {
    pub fn contains(&self, p: Pos2) -> bool {
        self.clip.contains(p)
            && Rect::from_min_size(Pos2::ZERO, self.size)
                .expand(if crate::platform::mobile() { 4. } else { 0. })
                .contains(self.transform.inverse(p))
    }
}
#[derive(Default)]
pub struct Renderer {
    textures: HashMap<String, TextureHandle>,
    pub hits: Vec<Hit>,
    pub regions: HashMap<String, (Rect, Rect)>,
    pub ambient_phase: f32,
    backdrop: super::blur::Backdrop,
    groups: super::composite::Groups,
    group_depth: usize,
    pending_text: Vec<PendingText>,
    font_jobs: HashSet<u64>,
    font_texture: Option<TextureHandle>,
    font_dirty: bool,
    font_size: [usize; 2],
    font_dpi: f32,
}
struct PendingText {
    painter: Painter,
    index: egui::layers::ShapeIdx,
    galley: Arc<egui::Galley>,
    origin: Pos2,
    angle: f32,
    color: Color32,
    x_scale: f32,
    mesh_scale: f32,
}
struct RenderPass {
    parent: Transform,
    opacity: f32,
    root: bool,
    interactive: bool,
    isolated: bool,
}

pub fn rgba(c: &Value, alpha: f32) -> Color32 {
    let f = |k: &str, default: f64| c[k].as_f64().unwrap_or(default) as f32;
    Color32::from_rgb(
        (f("r", 1.) * 255.).round() as u8,
        (f("g", 1.) * 255.).round() as u8,
        (f("b", 1.) * 255.).round() as u8,
    )
    .gamma_multiply((f("a", 1.) * alpha).clamp(0., 1.))
}
fn color_image(size: [usize; 2], bytes: &[u8]) -> ColorImage {
    ColorImage {
        size,
        pixels: bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| Color32::from_rgb(v[0], v[1], v[2]).gamma_multiply(v[3] as f32 / 255.))
            .collect(),
    }
}
fn points(t: Transform, w: f32, h: f32, r: f32, ellipse: bool) -> Vec<Pos2> {
    if ellipse {
        return (0..64)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 64.;
                t.point(w / 2. + a.cos() * w / 2., h / 2. + a.sin() * h / 2.)
            })
            .collect();
    }
    let r = r.min(w / 2.).min(h / 2.).max(0.);
    if r < 0.01 {
        return vec![
            t.point(0., 0.),
            t.point(w, 0.),
            t.point(w, h),
            t.point(0., h),
        ];
    }
    let mut out = vec![];
    for (x, y, start) in [
        (w - r, r, -90.),
        (w - r, h - r, 0.),
        (r, h - r, 90.),
        (r, r, 180.),
    ] {
        for i in 0..=12 {
            let a = (start + i as f32 * 7.5).to_radians();
            out.push(t.point(x + r * a.cos(), y + r * a.sin()));
        }
    }
    // Fully rounded capsules share arc endpoints. Near-zero segments produce
    // invalid antialias normals and stray pixels in the stroked outline.
    out.dedup_by(|a, b| a.distance_sq(*b) < 0.0001);
    if out.len() > 1 && out[0].distance_sq(*out.last().unwrap()) < 0.0001 {
        out.pop();
    }
    out
}
fn textured(
    p: &Painter,
    t: Transform,
    dimensions: egui::Vec2,
    r: f32,
    texture: TextureId,
    size: egui::Vec2,
    tint: Color32,
) {
    let (w, h) = (dimensions.x, dimensions.y);
    let mut mesh = Mesh::with_texture(texture);
    let polygon = points(t, w, h, r, false);
    let aspect = (w / h) / (size.x / size.y);
    let (ux, uy) = if aspect > 1. {
        (1., 1. / aspect)
    } else {
        (aspect, 1.)
    };
    let uv = |q: Pos2| {
        let local = t.inverse(q);
        pos2(
            0.5 + (local.x / w - 0.5) * ux,
            0.5 + (local.y / h - 0.5) * uy,
        )
    };
    mesh.vertices.push(Vertex {
        pos: t.point(w / 2., h / 2.),
        uv: pos2(0.5, 0.5),
        color: tint,
    });
    for q in &polygon {
        mesh.vertices.push(Vertex {
            pos: *q,
            uv: uv(*q),
            color: tint,
        });
    }
    for i in 0..polygon.len() {
        mesh.indices
            .extend([0, 1 + i as u32, 1 + ((i + 1) % polygon.len()) as u32]);
    }
    // A one-pixel coverage fringe smooths rotated and rounded cover edges.
    let count = polygon.len();
    for i in 0..count {
        let tangent = polygon[(i + 1) % count] - polygon[(i + count - 1) % count];
        let normal = vec2(tangent.y, -tangent.x).normalized() / p.ctx().pixels_per_point();
        mesh.vertices.push(Vertex {
            pos: polygon[i] + normal,
            uv: uv(polygon[i]),
            color: Color32::TRANSPARENT,
        });
        let a = 1 + i as u32;
        let b = 1 + ((i + 1) % count) as u32;
        let c = a + count as u32;
        let d = b + count as u32;
        mesh.indices.extend([a, c, b, b, c, d]);
    }
    p.add(mesh);
}
impl Renderer {
    pub fn preload_covers<'a>(
        &mut self,
        ctx: &egui::Context,
        tracks: impl Iterator<Item = &'a crate::model::Track>,
    ) {
        for track in tracks {
            if let Some(source) = &track.cover {
                let _ = self.texture(ctx, source);
            }
        }
    }
    #[cfg(test)]
    pub fn cache_counts(&self) -> (usize, usize) {
        (
            self.textures
                .keys()
                .filter(|k| k.starts_with("shadow/"))
                .count(),
            self.font_jobs.len(),
        )
    }
    pub fn begin(&mut self) {
        self.pending_text.clear();
        self.hits.clear();
        self.regions.clear();
        self.font_dirty = false;
    }
    pub fn finish(&mut self, ctx: &egui::Context) {
        let size = ctx.fonts(|f| f.font_image_size());
        if self.font_dirty
            || self.font_texture.is_none()
            || size != self.font_size
            || self.font_dpi != ctx.pixels_per_point()
        {
            let atlas = ctx.fonts(|f| f.image());
            let image = ColorImage {
                size: atlas.size,
                pixels: atlas.srgba_pixels(Some(1.0)).collect(),
            };
            if let Some(texture) = &mut self.font_texture {
                texture.set(image, TextureOptions::LINEAR);
            } else {
                self.font_texture =
                    Some(ctx.load_texture("medeo-figma-coverage", image, TextureOptions::LINEAR));
            }
            self.font_size = size;
            self.font_dpi = ctx.pixels_per_point();
        }
        let Some(texture) = &self.font_texture else {
            return;
        };
        let normalize = vec2(1. / size[0] as f32, 1. / size[1] as f32);
        for text in self.pending_text.drain(..) {
            let mut mesh = Mesh::with_texture(texture.id());
            let rotation = egui::emath::Rot2::from_angle(text.angle);
            let origin = text.origin;
            for row in &text.galley.rows {
                let offset = mesh.vertices.len() as u32;
                mesh.indices
                    .extend(row.visuals.mesh.indices.iter().map(|i| i + offset));
                mesh.vertices
                    .extend(row.visuals.mesh.vertices.iter().map(|vertex| Vertex {
                        pos: origin
                            + rotation
                                * vec2(vertex.pos.x * text.x_scale, vertex.pos.y)
                                * text.mesh_scale,
                        uv: (vertex.uv.to_vec2() * normalize).to_pos2(),
                        color: if vertex.color == Color32::PLACEHOLDER {
                            text.color
                        } else {
                            vertex.color
                        },
                    }));
            }
            text.painter.set(text.index, Shape::mesh(mesh));
        }
    }
    pub fn destroy(&self, gl: &eframe::glow::Context) {
        self.backdrop.destroy(gl);
        self.groups.destroy(gl);
    }
    fn texture(&mut self, ctx: &egui::Context, source: &str) -> Option<(TextureId, egui::Vec2)> {
        if source.starts_with("https://") || source.starts_with("http://") {
            return match ctx.try_load_texture(
                source,
                TextureOptions::LINEAR,
                egui::load::SizeHint::Size(600, 600),
            ) {
                Ok(egui::load::TexturePoll::Ready { texture }) => Some((texture.id, texture.size)),
                _ => None,
            };
        }
        if !self.textures.contains_key(source) {
            let bytes = assets::bytes(source)?;
            let mut image = image::load_from_memory(bytes).ok()?.into_rgba8();
            if source == "wordmark" {
                image = image::imageops::resize(
                    &image,
                    544,
                    153,
                    image::imageops::FilterType::Lanczos3,
                );
            }
            let size = [image.width() as usize, image.height() as usize];
            let texture = ctx.load_texture(
                source,
                color_image(size, image.as_raw()),
                TextureOptions::LINEAR,
            );
            self.textures.insert(source.into(), texture);
        }
        self.textures.get(source).map(|t| (t.id(), t.size_vec2()))
    }
    fn blob(&mut self, ctx: &egui::Context, n: &Node) -> Option<(TextureId, egui::Vec2, f32)> {
        let key = format!("ambient/{}", n.name);
        let blur = n.effects.iter().find(|v| v["type"] == "LAYER_BLUR")?["radius"].as_f64()? as f32;
        let pad = blur * 1.5;
        if !self.textures.contains_key(&key) {
            let factor = 4.;
            let w = ((n.width + pad * 2.) / factor).ceil() as u32;
            let h = ((n.height + pad * 2.) / factor).ceil() as u32;
            let paint = n.fills.first()?;
            let stops = paint["gradientStops"].as_array()?;
            let color = &stops.first()?["color"];
            let mut image = image::RgbaImage::new(w, h);
            for (x, y, p) in image.enumerate_pixels_mut() {
                let dx = ((x as f32 + 0.5) * factor - pad - n.width / 2.) / (n.width / 2.);
                let dy = ((y as f32 + 0.5) * factor - pad - n.height / 2.) / (n.height / 2.);
                let a =
                    (1. - dx.hypot(dy)).clamp(0., 1.) * color["a"].as_f64().unwrap_or(0.9) as f32;
                *p = image::Rgba([255, 255, 255, (a * 255.) as u8]);
            }
            // Blur alpha independently: straight-alpha RGB must retain the original colour.
            let mut alpha =
                image::GrayImage::from_fn(w, h, |x, y| image::Luma([image.get_pixel(x, y)[3]]));
            alpha = image::imageops::blur(&alpha, blur / (2. * factor));
            for (x, y, p) in image.enumerate_pixels_mut() {
                p[3] = alpha.get_pixel(x, y)[0];
            }
            self.textures.insert(
                key.clone(),
                ctx.load_texture(
                    &key,
                    color_image([w as usize, h as usize], image.as_raw()),
                    TextureOptions::LINEAR,
                ),
            );
        }
        let texture = self.textures.get(&key)?;
        Some((
            texture.id(),
            vec2(n.width + 2. * pad, n.height + 2. * pad),
            pad,
        ))
    }
    fn shadow(&mut self, p: &Painter, n: &Node, t: Transform, alpha: f32, e: &Value) {
        let radius = e["radius"].as_f64().unwrap_or(0.) as f32;
        if radius < 1. {
            return;
        }
        let pad = radius * 1.5;
        let factor = 3.;
        let key = format!(
            "shadow/{:0.0}/{:0.0}/{:0.1}/{radius}",
            n.width, n.height, n.corner_radius
        );
        if !self.textures.contains_key(&key) {
            let w = ((n.width + 2. * pad) / factor).ceil() as u32;
            let h = ((n.height + 2. * pad) / factor).ceil() as u32;
            let mask = image::GrayImage::from_fn(w, h, |x, y| {
                let q = vec2(
                    (x as f32 + 0.5) * factor - pad,
                    (y as f32 + 0.5) * factor - pad,
                );
                let r = n.corner_radius.min(n.height / 2.);
                let c = vec2(q.x.clamp(r, n.width - r), q.y.clamp(r, n.height - r));
                image::Luma([
                    if q.x >= 0.
                        && q.y >= 0.
                        && q.x <= n.width
                        && q.y <= n.height
                        && (q - c).length() <= r
                    {
                        255
                    } else {
                        0
                    },
                ])
            });
            let mask = image::imageops::blur(&mask, radius / (2. * factor));
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for v in mask.pixels() {
                rgba.extend([255, 255, 255, v[0]]);
            }
            self.textures.insert(
                key.clone(),
                p.ctx().load_texture(
                    &key,
                    color_image([w as usize, h as usize], &rgba),
                    TextureOptions::LINEAR,
                ),
            );
        }
        let tex = &self.textures[&key];
        let mut tr = t;
        let offset = vec2(
            e["offset"]["x"].as_f64().unwrap_or(0.) as f32 - pad,
            e["offset"]["y"].as_f64().unwrap_or(0.) as f32 - pad,
        );
        let origin = t.point(offset.x, offset.y);
        tr.x = origin.x;
        tr.y = origin.y;
        textured(
            p,
            tr,
            vec2(n.width + 2. * pad, n.height + 2. * pad),
            0.,
            tex.id(),
            tex.size_vec2(),
            rgba(&e["color"], alpha),
        );
    }
    pub fn paint(
        &mut self,
        p: &Painter,
        n: &Node,
        parent: Transform,
        opacity: f32,
        root: bool,
        interactive: bool,
    ) {
        if !n.visible || n.opacity <= 0. {
            return;
        }
        let isolated = n.opacity < 0.999 && !n.children.is_empty() && !n.name.starts_with("Album/");
        if isolated {
            self.groups.capture(p, self.group_depth, None);
            self.group_depth += 1;
        }
        self.paint_content(
            p,
            n,
            RenderPass {
                parent,
                opacity,
                root,
                interactive,
                isolated,
            },
        );
        if isolated {
            self.group_depth -= 1;
            self.groups.capture(p, self.group_depth, Some(n.opacity));
        }
    }
    fn paint_content(&mut self, p: &Painter, n: &Node, pass: RenderPass) {
        let RenderPass {
            parent,
            opacity,
            root,
            interactive,
            isolated,
        } = pass;
        if !n.visible || n.opacity <= 0. {
            return;
        }
        let mut t = if root { parent } else { parent.child(n) };
        self.regions
            .insert(n.name.clone(), (t.bounds(n.width, n.height), p.clip_rect()));
        let alpha = opacity * if isolated { 1. } else { n.opacity };
        if n.name.starts_with("Aurora/") {
            // Figma's two seven-second Smart Animate phases form one continuous loop.
            let phase = self.ambient_phase;
            let i = n
                .name
                .rsplit('/')
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            // Endpoint values are the original WeDex/Ambient Bloom variant.
            let ends = [
                (440., 70., 880., 690., [215., 47., 35.]),
                (850., 130., 650., 700., [232., 171., 37.]),
                (200., 650., 1000., 520., [147., 45., 68.]),
                (330., -120., 760., 550., [88., 35., 49.]),
            ];
            let (x, y, w, h, end_color) = ends[i.min(3)];
            let q = vec2(x - n.x, y - n.y) * phase * t.scale();
            t.x += q.x;
            t.y += q.y;
            if let Some((texture, texture_size, pad)) = self.blob(p.ctx(), n) {
                let color = &n.fills[0]["gradientStops"][0]["color"];
                let component = |name: &str, end: f32| {
                    let start = color[name].as_f64().unwrap_or(0.) as f32 * 255.;
                    (start + (end - start) * phase).round() as u8
                };
                let tint = Color32::from_rgb(
                    component("r", end_color[0]),
                    component("g", end_color[1]),
                    component("b", end_color[2]),
                )
                .gamma_multiply(alpha);
                let origin = t.point(-pad, -pad);
                let tr = Transform {
                    x: origin.x,
                    y: origin.y,
                    ..t
                };
                textured(
                    p,
                    tr,
                    vec2(
                        n.width + (w - n.width) * phase + 2. * pad,
                        n.height + (h - n.height) * phase + 2. * pad,
                    ),
                    0.,
                    texture,
                    texture_size,
                    tint,
                );
            }
            return;
        }
        let bounds = t.bounds(n.width, n.height);
        if !bounds.intersects(p.clip_rect()) && !root {
            return;
        }
        if interactive {
            if let Some(control) = &n.command {
                self.hits.push(Hit {
                    transform: t,
                    size: vec2(n.width, n.height),
                    clip: p.clip_rect(),
                    control: control.clone(),
                });
            }
        }
        for effect in &n.effects {
            if effect["type"] == "DROP_SHADOW" && effect["visible"] != false {
                self.shadow(p, n, t, alpha, effect);
            }
        }
        for effect in &n.effects {
            if effect["type"] == "BACKGROUND_BLUR" && effect["visible"] != false {
                self.backdrop.paint(
                    p,
                    n,
                    t,
                    alpha,
                    effect["radius"].as_f64().unwrap_or(28.) as f32,
                );
            }
        }
        if let Some(asset) = &n.asset {
            if let Some((texture, size)) = self.texture(p.ctx(), asset) {
                let tint = if asset == "wordmark" {
                    Color32::WHITE.gamma_multiply(alpha)
                } else {
                    rgba(&n.tint, alpha)
                };
                textured(p, t, vec2(n.width, n.height), 0., texture, size, tint);
            }
            return;
        }
        if n.kind == "TEXT" {
            let font = match (
                n.font_name["family"].as_str(),
                n.font_name["style"].as_str(),
            ) {
                (Some("Medeo Sans Display"), Some("SemiBold")) => "medeo-display-semibold",
                (Some("Medeo Sans Display"), _) => "medeo-display",
                (_, Some("SemiBold")) => "medeo-semibold",
                (_, Some("Bold")) => "medeo-bold",
                (_, Some("Medium")) => "medeo-medium",
                _ => "medeo-regular",
            };
            let color = n
                .fills
                .first()
                .map(|f| {
                    rgba(
                        &f["color"],
                        alpha * f["opacity"].as_f64().unwrap_or(1.) as f32,
                    )
                })
                .unwrap_or(Color32::WHITE);
            // Rasterize at a stable 2x size, then transform the mesh. Moving a card
            // must not allocate new font atlas entries on each animation frame.
            let scale = 2.;
            let line_height = n.line_height["value"]
                .as_f64()
                .unwrap_or((n.font_size * 1.4) as f64) as f32;
            let spacing = n.letter_spacing["value"].as_f64().unwrap_or(0.) as f32
                * if n.letter_spacing["unit"] == "PERCENT" {
                    n.font_size / 100.
                } else {
                    1.
                };
            let format = egui::TextFormat {
                font_id: FontId::new(n.font_size * scale, FontFamily::Name(font.into())),
                color,
                line_height: Some(line_height * scale),
                extra_letter_spacing: spacing * scale,
                ..Default::default()
            };
            let mut job = egui::text::LayoutJob::single_section(n.characters.clone(), format);
            job.wrap.max_width = n.width * scale + 8.;
            job.wrap.max_rows = if n.name == "Lyric line" {
                (n.height / line_height).round().max(1.) as usize
            } else if n.characters.contains('\n') {
                3
            } else {
                1
            };
            job.wrap.break_anywhere = n.name != "Lyric line";
            let centered_lyrics = n.name == "Lyric line" && n.text_align_horizontal == "CENTER";
            if centered_lyrics {
                job.halign = egui::Align::Center;
            }
            let galley = p.layout_job(job);
            let x_scale = if n.natural_width > 0. && n.characters == n.original_characters {
                n.natural_width * scale / galley.size().x.max(0.01)
            } else {
                1.
            };
            let x = if centered_lyrics {
                n.width / 2.
            } else {
                match n.text_align_horizontal.as_str() {
                    "CENTER" => (n.width - galley.size().x * x_scale / scale) / 2.,
                    "RIGHT" => n.width - galley.size().x * x_scale / scale,
                    _ => 0.,
                }
            };
            // Match Figma's half-leading around the font's 1.3 em ascent/descent.
            let leading = (line_height - n.font_size * 1.3) / 2.;
            let origin = t.point(x, leading);
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            n.characters.hash(&mut hash);
            font.hash(&mut hash);
            (n.font_size * scale).to_bits().hash(&mut hash);
            p.ctx().pixels_per_point().to_bits().hash(&mut hash);
            self.font_dirty |= self.font_jobs.insert(hash.finish());
            self.pending_text.push(PendingText {
                painter: p.clone(),
                index: p.add(Shape::Noop),
                galley,
                origin,
                angle: t.angle(),
                color,
                x_scale,
                mesh_scale: t.scale() / scale,
            });
            return;
        }
        if n.kind == "VECTOR" || n.kind == "BOOLEAN_OPERATION" {
            return;
        }
        for fill in &n.fills {
            if fill["visible"] == false {
                continue;
            }
            let a = alpha * fill["opacity"].as_f64().unwrap_or(1.) as f32;
            if fill["type"] == "SOLID" {
                p.add(Shape::convex_polygon(
                    points(t, n.width, n.height, n.corner_radius, n.kind == "ELLIPSE"),
                    rgba(&fill["color"], a),
                    Stroke::NONE,
                ));
            } else if fill["type"] == "IMAGE" {
                let source = fill["source"].as_str().unwrap_or("missing");
                if let Some((texture, size)) = self.texture(p.ctx(), source) {
                    textured(
                        p,
                        t,
                        vec2(n.width, n.height),
                        n.corner_radius,
                        texture,
                        size,
                        Color32::WHITE.gamma_multiply(a),
                    );
                } else {
                    p.add(Shape::convex_polygon(
                        points(t, n.width, n.height, n.corner_radius, false),
                        Color32::from_rgb(40, 38, 43).gamma_multiply(a),
                        Stroke::NONE,
                    ));
                }
            }
        }
        for stroke in &n.strokes {
            if stroke["visible"] == false {
                continue;
            }
            let color = rgba(
                &stroke["color"],
                alpha * stroke["opacity"].as_f64().unwrap_or(1.) as f32,
            );
            let inset = if n.stroke_align == "INSIDE" {
                n.stroke_weight / 2.
            } else {
                0.
            };
            let origin = t.point(inset, inset);
            let tr = Transform {
                x: origin.x,
                y: origin.y,
                ..t
            };
            p.add(Shape::closed_line(
                points(
                    tr,
                    n.width - inset * 2.,
                    n.height - inset * 2.,
                    (n.corner_radius - inset).max(0.),
                    n.kind == "ELLIPSE",
                ),
                Stroke::new(n.stroke_weight * t.scale(), color),
            ));
        }
        let painter = if n.clips_content && !root {
            p.with_clip_rect(p.clip_rect().intersect(bounds))
        } else {
            p.clone()
        };
        for child in &n.children {
            self.paint(&painter, child, t, alpha, false, interactive);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapped_lyrics_center_every_rendered_row() {
        let ctx = egui::Context::default();
        crate::branding::configure(&ctx);
        let mut renderer = Renderer::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            renderer.begin();
            let mut node = Node::text_node(
                "An invented lyric line that wraps onto several centered rows",
                0.,
                0.,
                200.,
                false,
            );
            node.name = "Lyric line".into();
            node.text_align_horizontal = "CENTER".into();
            node.font_size = 28.;
            node.height = 300.;
            node.line_height = serde_json::json!({"value":40.,"unit":"PIXELS"});
            let painter = ctx.layer_painter(egui::LayerId::background());
            renderer.paint(
                &painter,
                &node,
                Transform::new(1., pos2(0., 0.)),
                1.,
                true,
                false,
            );
            let text = &renderer.pending_text[0];
            assert!(text.galley.rows.len() > 1);
            for row in &text.galley.rows {
                assert!(
                    row.rect.center().x.abs() < 0.01,
                    "Wrapped row must stay centered"
                );
            }
            assert_eq!(text.origin.x, 100.);
        });
    }
}
