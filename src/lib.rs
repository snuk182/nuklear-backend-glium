#![cfg_attr(feature = "cargo-clippy", allow(redundant_field_names))] // for clarity

#[macro_use]
pub extern crate glium;

use nuklear::{Buffer, Context, ConvertConfig, DrawVertexLayoutAttribute, DrawVertexLayoutElements, DrawVertexLayoutFormat, Handle, Vec2};

#[derive(Debug, Copy, Clone)]
struct Vertex {
    pos: Vec2,
    tex: Vec2,
    col: [u8; 4],
}

impl glium::vertex::Vertex for Vertex {
    fn build_bindings() -> glium::vertex::VertexFormat {
        use std::mem::transmute;

        unsafe {
            let dummy: &Vertex = ::std::mem::transmute(0usize);
            ::std::borrow::Cow::Owned(vec![
                ("Position".into(), transmute(&dummy.pos), <(f32, f32) as glium::vertex::Attribute>::get_type(), false),
                ("TexCoord".into(), transmute(&dummy.tex), <(f32, f32) as glium::vertex::Attribute>::get_type(), false),
                ("Color".into(), transmute(&dummy.col), glium::vertex::AttributeType::U8U8U8U8, false),
            ])
        }
    }
}

impl Default for Vertex {
    fn default() -> Self {
        unsafe { ::std::mem::zeroed() }
    }
}

const VS: &str = "#version 150
        uniform mat4 ProjMtx;
        in vec2 Position;
        in vec2 TexCoord;
        in vec4 Color;
        out vec2 Frag_UV;
        out vec4 Frag_Color;
        void main() {
           Frag_UV = \
                          TexCoord;
           Frag_Color = Color / 255.0;
           gl_Position = ProjMtx * vec4(Position.xy, 0, 1);
        }";
const FS: &str = "#version 150
        precision mediump float;
	    uniform sampler2D Texture;
        in vec2 Frag_UV;
        in vec4 Frag_Color;
        out vec4 Out_Color;
        void main(){
           Out_Color = Frag_Color * \
                          texture(Texture, Frag_UV.st);
		}";

pub struct Drawer {
    cmd: Buffer,
    prg: glium::Program,
    tex: Vec<glium::Texture2d>,
    vbf: Vec<Vertex>,
    ebf: Vec<u16>,
    vbo: glium::VertexBuffer<Vertex>,
    ebo: glium::IndexBuffer<u16>,
    vle: DrawVertexLayoutElements,
}

impl Drawer {
    pub fn new(display: &mut glium::Display, texture_count: usize, vbo_size: usize, ebo_size: usize, command_buffer: Buffer) -> Drawer {
        Drawer {
            cmd: command_buffer,
            prg: glium::Program::from_source(display, VS, FS, None).unwrap(),
            tex: Vec::with_capacity(texture_count + 1),
            vbf: vec![Vertex::default(); vbo_size * ::std::mem::size_of::<Vertex>()],
            ebf: vec![0u16; ebo_size * ::std::mem::size_of::<u16>()],
            vbo: glium::VertexBuffer::empty_dynamic(display, vbo_size * ::std::mem::size_of::<Vertex>()).unwrap(),
            ebo: glium::IndexBuffer::empty_dynamic(display, glium::index::PrimitiveType::TrianglesList, ebo_size * ::std::mem::size_of::<u16>()).unwrap(),
            vle: DrawVertexLayoutElements::new(&[
                (DrawVertexLayoutAttribute::Position, DrawVertexLayoutFormat::Float, 0),
                (DrawVertexLayoutAttribute::TexCoord, DrawVertexLayoutFormat::Float, 8),
                (DrawVertexLayoutAttribute::Color, DrawVertexLayoutFormat::R8G8B8A8, 16),
                (DrawVertexLayoutAttribute::AttributeCount, DrawVertexLayoutFormat::Count, 32),
            ]),
        }
    }

    pub fn add_texture(&mut self, display: &mut glium::Display, image: &[u8], width: u32, height: u32) -> Handle {
        let image = glium::texture::RawImage2d {
            data: std::borrow::Cow::Borrowed(image),
            width: width,
            height: height,
            format: glium::texture::ClientFormat::U8U8U8U8,
        };
        let tex = glium::Texture2d::new(display, image).unwrap();
        let hnd = Handle::from_id(self.tex.len() as i32 + 1);
        self.tex.push(tex);
        hnd
    }

    pub fn draw(&mut self, ctx: &mut Context, cfg: &mut ConvertConfig, frame: &mut glium::Frame, scale: Vec2) {
        use glium::uniforms::MagnifySamplerFilter;
        use glium::Surface;
        use glium::{Blend, DrawParameters, Rect};

        let (ww, hh) = frame.get_dimensions();

        let ortho = [
            [2.0f32 / ww as f32, 0.0f32, 0.0f32, 0.0f32],
            [0.0f32, -2.0f32 / hh as f32, 0.0f32, 0.0f32],
            [0.0f32, 0.0f32, -1.0f32, 0.0f32],
            [-1.0f32, 1.0f32, 0.0f32, 1.0f32],
        ];

        cfg.set_vertex_layout(&self.vle);
        cfg.set_vertex_size(::std::mem::size_of::<Vertex>());

        {
            self.vbo.invalidate();
            self.ebo.invalidate();

            let mut rvbuf = unsafe { ::std::slice::from_raw_parts_mut(self.vbf.as_mut() as *mut [Vertex] as *mut u8, self.vbf.capacity()) };
            let mut rebuf = unsafe { ::std::slice::from_raw_parts_mut(self.ebf.as_mut() as *mut [u16] as *mut u8, self.ebf.capacity()) };
            let mut vbuf = Buffer::with_fixed(&mut rvbuf);
            let mut ebuf = Buffer::with_fixed(&mut rebuf);

            ctx.convert(&mut self.cmd, &mut vbuf, &mut ebuf, &cfg);

            self.vbo.slice_mut(0..self.vbf.capacity()).unwrap().write(&self.vbf);
            self.ebo.slice_mut(0..self.ebf.capacity()).unwrap().write(&self.ebf);
        }

        let mut idx_start = 0;
        let mut idx_end;

        for cmd in ctx.draw_command_iterator(&self.cmd) {
            if cmd.elem_count() < 1 {
                continue;
            }

            let id = cmd.texture().id().unwrap();
            let ptr = self.find_res(id).unwrap();

            idx_end = idx_start + cmd.elem_count() as usize;

            let x = cmd.clip_rect().x;
            let y = cmd.clip_rect().y;
            let w = cmd.clip_rect().w;
            let h = cmd.clip_rect().h;

            frame
                .draw(
                    &self.vbo,
                    &self.ebo.slice(idx_start..idx_end).unwrap(),
                    &self.prg,
                    &uniform! {
                        ProjMtx: ortho,
                        Texture: ptr.sampled().magnify_filter(MagnifySamplerFilter::Linear),
                    },
                    &DrawParameters {
                        blend: Blend::alpha_blending(),
                        scissor: Some(Rect {
                            left: (if x < 0f32 { 0f32 } else { x }) as u32,
                            bottom: (if y < 0f32 { 0f32 } else { hh as f32 - y - h }) as u32,
                            width: (if x < 0f32 { w + x } else { w }) as u32,
                            height: (if y < 0f32 { h + y } else { h }) as u32,
                        }),
                        backface_culling: glium::draw_parameters::BackfaceCullingMode::CullingDisabled,

                        ..DrawParameters::default()
                    },
                )
                .unwrap();
            idx_start = idx_end;
        }
    }

    fn find_res(&self, id: i32) -> Option<&glium::Texture2d> {
        if id > 0 && id as usize <= self.tex.len() {
            Some(&self.tex[(id - 1) as usize])
        } else {
            None
        }
    }
}
