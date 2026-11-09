use super::scene::Node;
use serde_json::{json, Value};

pub fn apply(root: &mut Node, dark: bool) {
    root.visit(&mut |n| {
        if n.name == "Ambient light" {
            n.opacity = 0.22;
            n.fills = vec![solid([0.976, 0.973, 0.965], 1.)];
        }
        if n.name == "Click outside to close" {
            n.opacity = 0.20;
        }
        if n.name == "Sidebar"
            || n.name == "Floating player"
            || n.name == "Glass dialog"
            || n.name == "Toast"
        {
            n.fills = vec![solid([1., 1., 1.], 0.88)];
            n.strokes = vec![solid([0.18, 0.16, 0.21], 0.09)];
        }
        if n.name == "Content surface" {
            n.fills = vec![solid([1., 1., 1.], 0.76)];
        }
        if n.name.starts_with("Album/") {
            n.fills = vec![solid([1., 1., 1.], 0.65)];
            n.strokes = vec![solid([1., 1., 1.], 0.90)];
        }
        if n.kind == "TEXT" {
            for fill in &mut n.fills {
                if luminance(&fill["color"]) > 0.64 {
                    fill["color"] = json!({"r":0.13,"g":0.12,"b":0.15});
                }
            }
        }
        if n.asset.as_deref() == Some("wordmark") {
            n.asset = Some("wordmark-light".into());
        }
        if n.asset.is_some() && n.name != "Active like" && luminance(&n.tint) > 0.64 {
            n.tint = json!({"r":0.13,"g":0.12,"b":0.15});
        }
        // Inverse glass surfaces/rails become neutral ink on the light theme.
        if n.kind != "TEXT" && !n.name.starts_with("Aurora/") {
            for fill in &mut n.fills {
                if fill["type"] == "SOLID" {
                    let color = &fill["color"];
                    let lum = luminance(color);
                    let opacity = fill["opacity"].as_f64().unwrap_or(1.);
                    let green =
                        color["g"].as_f64().unwrap_or(0.) - color["r"].as_f64().unwrap_or(0.);
                    if lum < 0.16 && opacity > 0.8 {
                        fill["color"] = json!({"r":0.976,"g":0.973,"b":0.965});
                    } else if lum > 0.95
                        && opacity < 0.5
                        && !matches!(
                            n.name.as_str(),
                            "Sidebar" | "Floating player" | "Glass dialog" | "Toast"
                        )
                        && !n.name.starts_with("Album/")
                    {
                        fill["color"] = json!({"r":0.15,"g":0.13,"b":0.18});
                        fill["opacity"] = (opacity * 0.55).into();
                    }
                    if green > 0.02 && lum > 0.6 {
                        fill["opacity"] = 1.0.into();
                    }
                }
            }
        }
        if n.name == "Progress" || n.name == "Thumb" {
            n.fills = vec![solid([0.16, 0.14, 0.19], 0.85)];
        }
        if n.name == "МУЗЫКА БЛИЖЕ" {
            n.characters = format!("v{}", env!("CARGO_PKG_VERSION"));
            n.natural_width = 0.;
            if n.width > 50. {
                n.width = 136.;
                n.font_size = 11.;
            }
        }
    });
    if dark {
        apply_dark(root, false);
    }
}
fn apply_dark(n: &mut Node, on_accent: bool) {
    let accent = on_accent
        || n.fills.iter().any(|f| {
            f["type"] == "SOLID"
                && f["color"]["g"].as_f64().unwrap_or(0.) - f["color"]["r"].as_f64().unwrap_or(0.)
                    > 0.02
                && luminance(&f["color"]) > 0.6
        });
    if n.kind == "TEXT" {
        for f in &mut n.fills {
            let primary = luminance(&f["color"]) < 0.25;
            f["color"] = if accent {
                json!({"r":0.10,"g":0.09,"b":0.12})
            } else if !primary {
                json!({"r":0.64,"g":0.62,"b":0.68})
            } else {
                json!({"r":0.94,"g":0.93,"b":0.96})
            };
        }
    } else if !n.name.starts_with("Aurora/") {
        for f in &mut n.fills {
            if f["type"] == "SOLID" && !accent {
                let opacity = f["opacity"].as_f64().unwrap_or(1.);
                f["color"] = if opacity < 0.5 {
                    json!({"r":0.90,"g":0.88,"b":0.95})
                } else {
                    if luminance(&f["color"]) > 0.95 {
                        json!({"r":0.075,"g":0.068,"b":0.088})
                    } else {
                        json!({"r":0.12,"g":0.11,"b":0.14})
                    }
                };
            }
        }
    }
    if n.name == "Ambient light" {
        n.opacity = 0.40;
    }
    if n.name == "Content surface" {
        n.fills = vec![solid([0.075, 0.068, 0.088], 0.74)];
    }
    if n.name.starts_with("Album/") {
        n.fills = vec![solid([0.12, 0.11, 0.14], 0.78)];
        n.strokes = vec![solid([1., 1., 1.], 0.15)];
    }
    if matches!(
        n.name.as_str(),
        "Sidebar" | "Floating player" | "Glass dialog" | "Toast"
    ) {
        n.fills = vec![solid([0.10, 0.09, 0.12], 0.90)];
        n.strokes = vec![solid([1., 1., 1.], 0.10)];
    }
    if n.name == "Progress" || n.name == "Thumb" {
        n.fills = vec![solid([0.91, 0.90, 0.94], 0.9)];
    }
    for stroke in &mut n.strokes {
        if stroke["type"] == "SOLID" {
            stroke["color"] = json!({"r":1.,"g":1.,"b":1.});
        }
    }
    if let Some(asset) = &mut n.asset {
        if asset == "wordmark-light" {
            *asset = "wordmark".into();
            n.tint = Value::Null;
        } else if n.name != "Active like"
            && !asset.starts_with("provider-")
            && asset != "app-symbol"
            && !(n.tint["r"].as_f64().unwrap_or(0.) > 0.6
                && n.tint["g"].as_f64().unwrap_or(0.) < 0.3)
        {
            n.tint = if accent {
                json!({"r":0.10,"g":0.09,"b":0.12})
            } else {
                json!({"r":0.94,"g":0.93,"b":0.96})
            };
        }
    }
    for child in &mut n.children {
        apply_dark(child, accent);
    }
}
fn solid(rgb: [f64; 3], opacity: f64) -> Value {
    json!({"type":"SOLID","color":{"r":rgb[0],"g":rgb[1],"b":rgb[2]},"opacity":opacity})
}
fn luminance(color: &Value) -> f64 {
    (color["r"].as_f64().unwrap_or(0.)
        + color["g"].as_f64().unwrap_or(0.)
        + color["b"].as_f64().unwrap_or(0.))
        / 3.
}
