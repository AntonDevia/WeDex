//! Figma applies group opacity after compositing children. Applying opacity to
//! each child separately makes overlapping cover cards visibly too transparent.
use eframe::{
    egui::{self, epaint::PaintCallback},
    egui_glow::CallbackFn,
    glow::{self, HasContext},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub struct Groups {
    pool: Arc<Mutex<HashMap<usize, Layer>>>,
}
struct Layer {
    before: glow::Texture,
    after: glow::Texture,
    program: glow::Program,
    vao: glow::VertexArray,
    size: [i32; 2],
}
impl Layer {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let program = gl.create_program()?;
        for (kind,source) in [(glow::VERTEX_SHADER,"#version 330 core\nvoid main(){vec2 q=vec2((gl_VertexID==1||gl_VertexID==4||gl_VertexID==5)?1.:0.,(gl_VertexID==2||gl_VertexID==3||gl_VertexID==5)?1.:0.);gl_Position=vec4(q*2.-1.,0.,1.);}"),(glow::FRAGMENT_SHADER,"#version 330 core\nuniform sampler2D u_before;uniform sampler2D u_after;uniform vec2 u_size;uniform float u_alpha;out vec4 color;void main(){vec2 uv=gl_FragCoord.xy/u_size;color=vec4(mix(texture(u_before,uv).rgb,texture(u_after,uv).rgb,u_alpha),1.);}")] {
            let shader=gl.create_shader(kind)?;gl.shader_source(shader,&crate::platform::shader_source(source));gl.compile_shader(shader);if !gl.get_shader_compile_status(shader){let e=gl.get_shader_info_log(shader);gl.delete_shader(shader);gl.delete_program(program);return Err(e);}gl.attach_shader(program,shader);gl.delete_shader(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        let before = gl.create_texture()?;
        let after = gl.create_texture()?;
        for texture in [before, after] {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
        }
        Ok(Self {
            before,
            after,
            program,
            vao: gl.create_vertex_array()?,
            size: [0, 0],
        })
    }
    unsafe fn capture(&mut self, gl: &glow::Context, info: &egui::PaintCallbackInfo, before: bool) {
        let size = info.screen_size_px.map(|v| v as i32);
        gl.active_texture(glow::TEXTURE0);
        if self.size != size {
            for tex in [self.before, self.after] {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    size[0],
                    size[1],
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            self.size = size;
        }
        gl.bind_texture(
            glow::TEXTURE_2D,
            Some(if before { self.before } else { self.after }),
        );
        gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, size[0], size[1]);
    }
    unsafe fn blend(&self, gl: &glow::Context, info: &egui::PaintCallbackInfo, alpha: f32) {
        gl.use_program(Some(self.program));
        gl.bind_vertex_array(Some(self.vao));
        gl.disable(glow::BLEND);
        gl.disable(glow::CULL_FACE);
        gl.disable(glow::DEPTH_TEST);
        gl.viewport(0, 0, self.size[0], self.size[1]);
        let clip = info.clip_rect_in_pixels();
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(
            clip.left_px,
            clip.from_bottom_px,
            clip.width_px,
            clip.height_px,
        );
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.before));
        gl.uniform_1_i32(
            gl.get_uniform_location(self.program, "u_before").as_ref(),
            0,
        );
        gl.active_texture(glow::TEXTURE1);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.after));
        gl.uniform_1_i32(gl.get_uniform_location(self.program, "u_after").as_ref(), 1);
        gl.uniform_2_f32(
            gl.get_uniform_location(self.program, "u_size").as_ref(),
            self.size[0] as f32,
            self.size[1] as f32,
        );
        gl.uniform_1_f32(
            gl.get_uniform_location(self.program, "u_alpha").as_ref(),
            alpha,
        );
        gl.draw_arrays(glow::TRIANGLES, 0, 6);
        gl.active_texture(glow::TEXTURE0);
    }
    unsafe fn destroy(self, gl: &glow::Context) {
        gl.delete_program(self.program);
        gl.delete_vertex_array(self.vao);
        gl.delete_texture(self.before);
        gl.delete_texture(self.after);
    }
}
impl Groups {
    pub fn capture(&self, p: &egui::Painter, depth: usize, alpha: Option<f32>) {
        let pool = self.pool.clone();
        p.add(PaintCallback {
            rect: p.ctx().screen_rect(),
            callback: Arc::new(CallbackFn::new(move |info, painter| unsafe {
                let mut pool = pool.lock().unwrap_or_else(|e| e.into_inner());
                if let std::collections::hash_map::Entry::Vacant(entry) = pool.entry(depth) {
                    match Layer::new(painter.gl()) {
                        Ok(layer) => {
                            entry.insert(layer);
                        }
                        Err(error) => {
                            eprintln!("Group compositor: {error}");
                            return;
                        }
                    }
                }
                let layer = pool.get_mut(&depth).unwrap();
                layer.capture(painter.gl(), &info, alpha.is_none());
                if let Some(alpha) = alpha {
                    layer.blend(painter.gl(), &info, alpha);
                }
            })),
        });
    }
    pub fn destroy(&self, gl: &glow::Context) {
        for (_, layer) in self.pool.lock().unwrap_or_else(|e| e.into_inner()).drain() {
            unsafe {
                layer.destroy(gl);
            }
        }
    }
}
