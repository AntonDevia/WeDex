use eframe::egui::{pos2, Pos2, Rect, Vec2};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub relative_transform: Option<[[f32; 3]; 2]>,
    pub fills: Vec<Value>,
    pub strokes: Vec<Value>,
    pub stroke_weight: f32,
    pub stroke_align: String,
    pub corner_radius: f32,
    pub effects: Vec<Value>,
    pub clips_content: bool,
    pub characters: String,
    pub original_characters: String,
    pub natural_width: f32,
    pub font_name: Value,
    pub font_size: f32,
    pub line_height: Value,
    pub letter_spacing: Value,
    pub text_align_horizontal: String,
    pub asset: Option<String>,
    pub tint: Value,
    pub children: Vec<Node>,
    #[serde(skip)]
    pub command: Option<super::Control>,
}
fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}

impl Node {
    pub fn scale_geometry(&mut self, factor: f32) {
        self.width *= factor;
        self.height *= factor;
        self.corner_radius *= factor;
        self.stroke_weight *= factor;
        self.font_size *= factor;
        self.natural_width *= factor;
        self.move_to(self.x * factor, self.y * factor);
        for value in [&mut self.line_height, &mut self.letter_spacing] {
            if value["unit"] == "PIXELS" {
                if let Some(n) = value["value"].as_f64() {
                    value["value"] = (n * factor as f64).into();
                }
            }
        }
        for child in &mut self.children {
            child.scale_geometry(factor);
        }
    }
    pub fn find(&self, name: &str) -> Option<&Self> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|n| n.find(name))
    }
    pub fn find_mut(&mut self, name: &str) -> Option<&mut Self> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter_mut().find_map(|n| n.find_mut(name))
    }
    pub fn visit(&mut self, f: &mut impl FnMut(&mut Self)) {
        f(self);
        for c in &mut self.children {
            c.visit(f);
        }
    }
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
        if let Some(m) = &mut self.relative_transform {
            m[0][2] = x;
            m[1][2] = y;
        }
    }
    pub fn texts(&mut self, values: &[&str]) {
        let mut i = 0;
        self.visit(&mut |n| {
            if n.kind == "TEXT" && i < values.len() {
                n.characters = values[i].into();
                i += 1;
            }
        });
    }
    pub fn set_image(&mut self, source: &str) {
        self.visit(&mut |n| {
            for p in &mut n.fills {
                if p["type"] == "IMAGE" {
                    p["source"] = source.into();
                }
            }
        });
    }
    pub fn text_node(text: &str, x: f32, y: f32, width: f32, inverse: bool) -> Self {
        Self {
            id: format!("label-{text}-{x}-{y}"),
            name: "Runtime label".into(),
            kind: "TEXT".into(),
            visible: true,
            opacity: 1.0,
            x,
            y,
            width,
            height: 44.0,
            font_size: 14.0,
            characters: text.into(),
            font_name: serde_json::json!({"family":"Medeo Sans","style":"Medium"}),
            line_height: serde_json::json!({"unit":"PIXELS","value":22}),
            fills: vec![
                serde_json::json!({"type":"SOLID","color":if inverse {serde_json::json!({"r":0.98,"g":0.973,"b":0.96})}else{serde_json::json!({"r":0.10,"g":0.09,"b":0.11})}}),
            ],
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub x: f32,
    pub y: f32,
}
impl Transform {
    pub fn new(scale: f32, origin: Pos2) -> Self {
        Self {
            a: scale,
            b: 0.,
            c: 0.,
            d: scale,
            x: origin.x,
            y: origin.y,
        }
    }
    pub fn child(self, n: &Node) -> Self {
        let m = n
            .relative_transform
            .unwrap_or([[1., 0., n.x], [0., 1., n.y]]);
        Self {
            a: self.a * m[0][0] + self.c * m[1][0],
            b: self.b * m[0][0] + self.d * m[1][0],
            c: self.a * m[0][1] + self.c * m[1][1],
            d: self.b * m[0][1] + self.d * m[1][1],
            x: self.a * m[0][2] + self.c * m[1][2] + self.x,
            y: self.b * m[0][2] + self.d * m[1][2] + self.y,
        }
    }
    pub fn point(self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.a * x + self.c * y + self.x,
            self.b * x + self.d * y + self.y,
        )
    }
    pub fn inverse(self, p: Pos2) -> Pos2 {
        let v = p - pos2(self.x, self.y);
        let det = self.a * self.d - self.b * self.c;
        pos2(
            (self.d * v.x - self.c * v.y) / det,
            (-self.b * v.x + self.a * v.y) / det,
        )
    }
    pub fn scale(self) -> f32 {
        Vec2::new(self.a, self.b).length()
    }
    pub fn angle(self) -> f32 {
        self.b.atan2(self.a)
    }
    pub fn bounds(self, w: f32, h: f32) -> Rect {
        Rect::from_points(&[
            self.point(0., 0.),
            self.point(w, 0.),
            self.point(w, h),
            self.point(0., h),
        ])
    }
}
