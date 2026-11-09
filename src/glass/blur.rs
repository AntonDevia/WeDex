//! Backdrop blur operates on the actual preceding framebuffer, including covers.
//! This is a separable Gaussian pass, not an opaque imitation of glass.
use super::scene::{Node, Transform};
use eframe::{
    egui::{self, epaint::PaintCallback},
    egui_glow::CallbackFn,
    glow::{self, HasContext},
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Backdrop {
    state: Arc<Mutex<Option<Gaussian>>>,
}
struct Gaussian {
    program: glow::Program,
    vao: glow::VertexArray,
    source: glow::Texture,
    temp: glow::Texture,
    fbo: glow::Framebuffer,
    size: [i32; 2],
}
#[derive(Clone, Copy)]
struct Surface {
    t: Transform,
    size: egui::Vec2,
    radius: f32,
    blur: f32,
    alpha: f32,
}
const VERTEX: &str = r#"#version 330 core
uniform vec2 u_screen; uniform vec2 u_size; uniform mat3 u_transform; uniform int u_pass;
out vec2 local;
void main(){
 vec2 q=vec2((gl_VertexID==1||gl_VertexID==4||gl_VertexID==5)?1.:0.,(gl_VertexID==2||gl_VertexID==3||gl_VertexID==5)?1.:0.);
 local=q*u_size;
 if(u_pass==0){gl_Position=vec4(q*2.-1.,0.,1.);}
 else {vec2 p=(u_transform*vec3(local,1.)).xy;gl_Position=vec4(p.x/u_screen.x*2.-1.,1.-p.y/u_screen.y*2.,0.,1.);}
}"#;
const FRAGMENT: &str = r#"#version 330 core
uniform sampler2D u_source; uniform vec2 u_target; uniform float u_sigma; uniform float u_radius; uniform float u_alpha;
uniform vec2 u_size; uniform int u_pass; in vec2 local; out vec4 color;
void main(){
 vec2 uv=gl_FragCoord.xy/u_target;
 vec2 direction=u_pass==0?vec2(u_sigma/u_target.x,0.):vec2(0.,u_sigma/u_target.y);
 vec3 sum=vec3(0.);float total=0.;
 for(int i=-8;i<=8;i++){float x=float(i)*.375;float weight=exp(-.5*x*x);sum+=texture(u_source,uv+direction*x).rgb*weight;total+=weight;}
 if(u_pass==0){color=vec4(sum/total,1.);}
 else {vec2 d=abs(local-u_size*.5)-(u_size*.5-vec2(u_radius));float distance=length(max(d,vec2(0.)))+min(max(d.x,d.y),0.)-u_radius;float mask=1.-smoothstep(-.5,.5,distance);float a=mask*u_alpha;color=vec4(sum/total*a,a);}
}"#;
impl Gaussian {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let program = gl.create_program()?;
        for (kind, source) in [
            (glow::VERTEX_SHADER, VERTEX),
            (glow::FRAGMENT_SHADER, FRAGMENT),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &crate::platform::shader_source(source));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let error = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                gl.delete_program(program);
                return Err(error);
            }
            gl.attach_shader(program, shader);
            gl.delete_shader(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let e = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(e);
        }
        let source = gl.create_texture()?;
        let temp = gl.create_texture()?;
        for texture in [source, temp] {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
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
            program,
            vao: gl.create_vertex_array()?,
            source,
            temp,
            fbo: gl.create_framebuffer()?,
            size: [0, 0],
        })
    }
    unsafe fn draw(
        &mut self,
        gl: &glow::Context,
        info: egui::PaintCallbackInfo,
        painter: &eframe::egui_glow::Painter,
        surface: Surface,
    ) {
        let Surface {
            t,
            size,
            radius,
            blur,
            alpha,
        } = surface;
        let [width, height] = info.screen_size_px.map(|v| v as i32);
        let down = [(width / 4).max(1), (height / 4).max(1)];
        gl.active_texture(glow::TEXTURE0);
        if self.size != [width, height] {
            for (texture, w, h) in [(self.source, width, height), (self.temp, down[0], down[1])] {
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w,
                    h,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            self.size = [width, height];
        }
        gl.bind_texture(glow::TEXTURE_2D, Some(self.source));
        gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, width, height);
        gl.bind_vertex_array(Some(self.vao));
        gl.use_program(Some(self.program));
        let uniform = |name| gl.get_uniform_location(self.program, name);
        gl.uniform_1_i32(uniform("u_source").as_ref(), 0);
        gl.uniform_2_f32(uniform("u_screen").as_ref(), width as f32, height as f32);
        let pixels = info.pixels_per_point;
        gl.uniform_2_f32(uniform("u_size").as_ref(), size.x, size.y);
        gl.uniform_1_f32(uniform("u_radius").as_ref(), radius);
        gl.uniform_1_f32(uniform("u_alpha").as_ref(), alpha);
        let matrix = [
            t.a * pixels,
            t.b * pixels,
            0.,
            t.c * pixels,
            t.d * pixels,
            0.,
            t.x * pixels,
            t.y * pixels,
            1.,
        ];
        gl.uniform_matrix_3_f32_slice(uniform("u_transform").as_ref(), false, &matrix);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(self.temp),
            0,
        );
        gl.viewport(0, 0, down[0], down[1]);
        gl.uniform_1_i32(uniform("u_pass").as_ref(), 0);
        gl.uniform_2_f32(uniform("u_target").as_ref(), down[0] as f32, down[1] as f32);
        // The target is quarter resolution; express sigma in its pixel coordinates.
        gl.uniform_1_f32(uniform("u_sigma").as_ref(), blur * t.scale() * pixels / 8.);
        gl.draw_arrays(glow::TRIANGLES, 0, 6);
        gl.bind_framebuffer(glow::FRAMEBUFFER, painter.intermediate_fbo());
        gl.viewport(0, 0, width, height);
        let clip = info.clip_rect_in_pixels();
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(
            clip.left_px,
            clip.from_bottom_px,
            clip.width_px,
            clip.height_px,
        );
        gl.enable(glow::BLEND);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.temp));
        gl.uniform_1_i32(uniform("u_pass").as_ref(), 1);
        gl.uniform_2_f32(uniform("u_target").as_ref(), width as f32, height as f32);
        gl.uniform_1_f32(uniform("u_sigma").as_ref(), blur * t.scale() * pixels / 2.);
        gl.draw_arrays(glow::TRIANGLES, 0, 6);
    }
    unsafe fn destroy(self, gl: &glow::Context) {
        gl.delete_program(self.program);
        gl.delete_vertex_array(self.vao);
        gl.delete_texture(self.source);
        gl.delete_texture(self.temp);
        gl.delete_framebuffer(self.fbo);
    }
}
impl Backdrop {
    pub fn paint(&self, painter: &egui::Painter, n: &Node, t: Transform, alpha: f32, blur: f32) {
        let state = self.state.clone();
        let size = egui::vec2(n.width, n.height);
        let radius = n.corner_radius.min(n.width / 2.).min(n.height / 2.);
        painter.add(PaintCallback {
            rect: painter.ctx().screen_rect(),
            callback: Arc::new(CallbackFn::new(move |info, painter| {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                unsafe {
                    if state.is_none() {
                        match Gaussian::new(painter.gl()) {
                            Ok(shader) => *state = Some(shader),
                            Err(error) => {
                                eprintln!("Backdrop shader: {error}");
                                return;
                            }
                        }
                    }
                    if let Some(shader) = state.as_mut() {
                        shader.draw(
                            painter.gl(),
                            info,
                            painter,
                            Surface {
                                t,
                                size,
                                radius,
                                blur,
                                alpha,
                            },
                        );
                    }
                }
            })),
        });
    }
    pub fn destroy(&self, gl: &glow::Context) {
        if let Some(shader) = self.state.lock().unwrap_or_else(|e| e.into_inner()).take() {
            unsafe {
                shader.destroy(gl);
            }
        }
    }
}
