//! Geometria que nao muda (terreno, mar, casas) guardada NA GPU.
//!
//! O `draw_mesh` da macroquad e' desenho imediato: todo quadro ele copia os
//! vertices pro lote e sobe o lote inteiro pra GPU (`buffer_update`). Com a
//! ilha inteira em cena isso era a thread principal presa em copia de memoria
//! — no emulador de Android, 3,7 quadros por segundo com 52% do tempo em
//! chamada de sistema, e o POCO tambem engasgava. Aqui cada malha sobe UMA
//! vez, na primeira vez que aparece, e dai em diante o quadro so' manda
//! "desenhe o buffer tal".
//!
//! Mesmos shaders e mesmo estado de pipeline do material (`render3d::
//! SOLIDO_*`/`params_solido`, `agua::VERTICE`/`params_agua`): o que se ve'
//! nao muda, so' o caminho.
//!
//! Ordem com o resto da macroquad: antes de desenhar, `flush` manda o lote
//! pendente, entao o que foi pedido antes sai antes.

use std::cell::{Cell, Ref, RefCell};

use macroquad::miniquad::{
    self, Bindings, BufferId, BufferLayout, BufferSource, BufferType, BufferUsage, PassAction,
    Pipeline, RenderingBackend, ShaderMeta, ShaderSource, TextureId, UniformBlockLayout,
    UniformDesc, UniformType, UniformsSource, VertexAttribute, VertexFormat,
};
use macroquad::prelude::{Mat4, Mesh, Vec3};
use macroquad::window::get_internal_gl;

/// Uma malha que sobe pra GPU no primeiro desenho.
pub struct MalhaEstatica {
    /// Os vertices na CPU ate' subir. Depois disso so' ficam se `manter_cpu`
    /// (o teto das casas e' recopiado na transicao de sumir).
    cpu: RefCell<Option<Mesh>>,
    manter_cpu: bool,
    gpu: Cell<Option<(BufferId, BufferId, i32)>>,
}

impl MalhaEstatica {
    pub fn nova(m: Mesh) -> Self {
        Self {
            cpu: RefCell::new(Some(m)),
            manter_cpu: false,
            gpu: Cell::new(None),
        }
    }

    /// Guarda a copia da CPU mesmo depois de subir.
    pub fn mantendo_cpu(m: Mesh) -> Self {
        Self {
            cpu: RefCell::new(Some(m)),
            manter_cpu: true,
            gpu: Cell::new(None),
        }
    }

    /// Os vertices, enquanto ainda estao na CPU (sempre, em teste e com
    /// `mantendo_cpu`).
    pub fn cpu(&self) -> Option<Ref<'_, Mesh>> {
        Ref::filter_map(self.cpu.borrow(), |m| m.as_ref()).ok()
    }

    fn subir(&self, ctx: &mut dyn RenderingBackend) -> Option<(BufferId, BufferId, i32)> {
        if let Some(g) = self.gpu.get() {
            return Some(g);
        }
        let g = {
            let m = self.cpu.borrow();
            let m = m.as_ref()?;
            if m.indices.is_empty() {
                return None;
            }
            let vb = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&m.vertices),
            );
            let ib = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&m.indices),
            );
            (vb, ib, m.indices.len() as i32)
        };
        self.gpu.set(Some(g));
        if !self.manter_cpu {
            // Na GPU ja' basta: no celular a memoria e' a mesma, e guardar as
            // duas copias dobrava o que a ilha ocupa.
            self.cpu.replace(None);
        }
        Some(g)
    }
}

impl Drop for MalhaEstatica {
    fn drop(&mut self) {
        // So' tem buffer quem ja' foi desenhado, e isso so' acontece com a
        // janela viva — entao o contexto existe aqui.
        if let Some((vb, ib, _)) = self.gpu.get() {
            let gl = unsafe { get_internal_gl() };
            gl.quad_context.delete_buffer(vb);
            gl.quad_context.delete_buffer(ib);
        }
    }
}

/// Qual shader.
#[derive(Clone, Copy, PartialEq)]
pub enum Programa {
    /// O do mundo (`render3d::material_solido`), com o recorte do jogador.
    Solido {
        recorte: Vec3,
        recorte_z: f32,
    },
    Sombra,
    /// O mar (`agua::material`).
    Agua {
        tempo: f32,
        ondas: f32,
    },
}

#[repr(C)]
struct UniformesSolido {
    projection: Mat4,
    model: Mat4,
    recorte: Vec3,
    recorte_z: f32,
    tinta: [f32; 4],
    luz_dia: f32,
    faixas: Faixas,
}

/// As cores das faixas de paleta, por DRAW.
///
/// `a == 0.0` numa faixa = o vertice fica com a cor que veio do arquivo — o
/// caso de bicho, terreno, NPC e tudo o mais. E' o `Default`, e por isso
/// ligar isto nao muda nada no jogo que ja' existe.
///
/// Fica aqui e nao no `render3d` porque e' parte do bloco de uniformes: a
/// ordem dos campos TEM que casar com a lista de `meta(&[...])`, e postcard
/// nao e' quem manda — e' o layout C.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Faixas {
    pub tier: [[f32; 4]; 2],
    pub cabelo: [[f32; 4]; 2],
    pub pele: [[f32; 4]; 2],
}

impl Faixas {
    /// `(claro, escuro)` em 0..1. `None` deixa a faixa desligada.
    pub fn nova(
        tier: Option<[[f32; 3]; 2]>,
        cabelo: Option<[[f32; 3]; 2]>,
        pele: Option<[[f32; 3]; 2]>,
    ) -> Self {
        let par = |o: Option<[[f32; 3]; 2]>| match o {
            Some([c, e]) => [[c[0], c[1], c[2], 1.0], [e[0], e[1], e[2], 1.0]],
            None => [[0.0; 4]; 2],
        };
        Self {
            tier: par(tier),
            cabelo: par(cabelo),
            pele: par(pele),
        }
    }
}

#[repr(C)]
struct UniformesAgua {
    projection: Mat4,
    model: Mat4,
    tempo: f32,
    ondas: f32,
}

struct Programas {
    solido: Pipeline,
    sombra: Pipeline,
    agua: Pipeline,
    branco: TextureId,
}

thread_local! {
    static PROGRAMAS: RefCell<Option<Programas>> = const { RefCell::new(None) };
    static LUZ_DIA: Cell<f32> = const { Cell::new(0.0) };
    /// Buffers das malhas de voxel (`VoxCache`), pela posicao dos vertices na
    /// memoria. O cache de vox carrega uma vez e nunca solta nem troca malha,
    /// entao o endereco e' estavel durante o jogo inteiro.
    static VOXEL: RefCell<std::collections::HashMap<(usize, usize, usize), (BufferId, BufferId, i32)>> =
        RefCell::new(std::collections::HashMap::new());
}

pub fn define_luz_dia(valor: f32) {
    LUZ_DIA.with(|l| l.set(valor));
}

fn atributos() -> [VertexAttribute; 4] {
    // O layout do `Vertex` da macroquad, na ordem dos campos.
    [
        VertexAttribute::new("position", VertexFormat::Float3),
        VertexAttribute::new("texcoord", VertexFormat::Float2),
        VertexAttribute::new("color0", VertexFormat::Byte4),
        VertexAttribute::new("normal", VertexFormat::Float4),
    ]
}

fn cria(ctx: &mut dyn RenderingBackend) -> Programas {
    let meta = |extras: &[(&str, UniformType)]| {
        let mut uniforms = vec![
            UniformDesc::new("Projection", UniformType::Mat4),
            UniformDesc::new("Model", UniformType::Mat4),
        ];
        uniforms.extend(extras.iter().map(|(n, t)| UniformDesc::new(n, *t)));
        ShaderMeta {
            images: vec!["Texture".to_string()],
            uniforms: UniformBlockLayout { uniforms },
        }
    };
    let solido = ctx
        .new_shader(
            ShaderSource::Glsl {
                vertex: crate::render3d::SOLIDO_VERTICE,
                fragment: crate::render3d::SOLIDO_FRAGMENTO,
            },
            meta(&[
                ("Crop", UniformType::Float3),
                ("RecorteZ", UniformType::Float1),
                ("Tinta", UniformType::Float4),
                ("LuzDia", UniformType::Float1),
                ("TierClaro", UniformType::Float4),
                ("TierEsc", UniformType::Float4),
                ("CabeloClaro", UniformType::Float4),
                ("CabeloEsc", UniformType::Float4),
                ("PeleClaro", UniformType::Float4),
                ("PeleEsc", UniformType::Float4),
            ]),
        )
        .expect("world shader (gpu)");
    let agua = ctx
        .new_shader(
            ShaderSource::Glsl {
                vertex: crate::agua::VERTICE,
                fragment: crate::agua::FRAGMENTO,
            },
            meta(&[
                ("Tempo", UniformType::Float1),
                ("Ondas", UniformType::Float1),
            ]),
        )
        .expect("water shader (gpu)");
    Programas {
        solido: ctx.new_pipeline(
            &[BufferLayout::default()],
            &atributos(),
            solido,
            crate::render3d::params_solido(),
        ),
        sombra: ctx.new_pipeline(
            &[BufferLayout::default()],
            &atributos(),
            solido,
            crate::render3d::params_sombra(),
        ),
        agua: ctx.new_pipeline(
            &[BufferLayout::default()],
            &atributos(),
            agua,
            crate::agua::params_agua(),
        ),
        branco: ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]),
    }
}

/// Desenha as malhas com a camera atual da macroquad. Devolve quantas foram.
pub fn desenha<'a>(
    programa: Programa,
    malhas: impl IntoIterator<Item = &'a MalhaEstatica>,
) -> usize {
    desenha_com_modelo(programa, malhas, Mat4::IDENTITY)
}

/// Reusa buffers estáticos com transformação por instância (nuvens ao vento).
pub fn desenha_com_modelo<'a>(
    programa: Programa,
    malhas: impl IntoIterator<Item = &'a MalhaEstatica>,
    modelo: Mat4,
) -> usize {
    let mut gl = unsafe { get_internal_gl() };
    // O lote pendente sai antes: o que foi pedido antes aparece antes.
    gl.flush();
    let projection = gl.quad_gl.get_projection_matrix();
    let passe = gl.quad_gl.get_active_render_pass();
    let viewport = viewport_da_camera(gl.quad_gl.get_viewport());
    let ctx = gl.quad_context;
    PROGRAMAS.with(|p| {
        let mut p = p.borrow_mut();
        let p = p.get_or_insert_with(|| cria(ctx));
        let prontas: Vec<(BufferId, BufferId, i32)> =
            malhas.into_iter().filter_map(|m| m.subir(ctx)).collect();
        if prontas.is_empty() {
            return 0;
        }
        abre_passe(ctx, passe, viewport);
        match programa {
            Programa::Solido { recorte, recorte_z } => {
                ctx.apply_pipeline(&p.solido);
                ctx.apply_uniforms(UniformsSource::table(&UniformesSolido {
                    projection,
                    model: modelo,
                    recorte,
                    recorte_z,
                    tinta: [0.0; 4],
                    luz_dia: LUZ_DIA.with(|l| l.get()),
                    // O lote de malha ESTATICA (terreno, vegetacao) nunca
                    // tinge por faixa: ela e' do personagem.
                    faixas: Faixas::default(),
                }));
            }
            Programa::Sombra => {
                ctx.apply_pipeline(&p.sombra);
                ctx.apply_uniforms(UniformsSource::table(&UniformesSolido {
                    projection,
                    model: modelo,
                    recorte: Vec3::ZERO,
                    recorte_z: 0.0,
                    tinta: [0.0; 4],
                    luz_dia: 0.0,
                    faixas: Faixas::default(),
                }));
            }
            Programa::Agua { tempo, ondas } => {
                ctx.apply_pipeline(&p.agua);
                ctx.apply_uniforms(UniformsSource::table(&UniformesAgua {
                    projection,
                    model: modelo,
                    tempo,
                    ondas,
                }));
            }
        }
        for &(vb, ib, n) in &prontas {
            ctx.apply_bindings(&Bindings {
                vertex_buffers: vec![vb],
                index_buffer: ib,
                images: vec![p.branco],
            });
            ctx.draw(0, n, 1);
        }
        ctx.end_render_pass();
        prontas.len()
    })
}

/// O viewport da camera atual, ou `None` se ela nao tem um: a macroquad
/// devolve a tela em pontos quando nao ha', e isso nao e' pixel.
fn viewport_da_camera(v: (i32, i32, i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let tela = (
        0,
        0,
        macroquad::window::screen_width() as i32,
        macroquad::window::screen_height() as i32,
    );
    (v != tela).then_some(v)
}

/// Comeca o passe onde a macroquad desenharia (tela ou render target), no
/// viewport da camera — o retrato do personagem usa um.
fn abre_passe(
    ctx: &mut dyn RenderingBackend,
    passe: Option<miniquad::RenderPass>,
    viewport: Option<(i32, i32, i32, i32)>,
) {
    let (w, h) = match passe {
        Some(rp) => ctx.texture_size(ctx.render_pass_texture(rp)),
        None => {
            let (w, h) = miniquad::window::screen_size();
            (w as u32, h as u32)
        }
    };
    match passe {
        Some(rp) => ctx.begin_pass(Some(rp), PassAction::Nothing),
        None => ctx.begin_default_pass(PassAction::Nothing),
    }
    let (x, y, vw, vh) = viewport.unwrap_or((0, 0, w as i32, h as i32));
    ctx.apply_viewport(x, y, vw, vh);
    ctx.apply_scissor_rect(0, 0, w as i32, h as i32);
}

/// Uma malha de voxel (bicho, boneco, arma, saque) com matriz de modelo e
/// tinta no shader do mundo. Recorte desligado: bicho nao e' obstaculo (ver o
/// passe do mundo em `main`).
pub fn desenha_voxel(m: &Mesh, modelo: Mat4, tinta: [f32; 4], faixas: Faixas) {
    if m.indices.is_empty() {
        return;
    }
    let mut gl = unsafe { get_internal_gl() };
    gl.flush();
    let projection = gl.quad_gl.get_projection_matrix();
    let passe = gl.quad_gl.get_active_render_pass();
    let viewport = viewport_da_camera(gl.quad_gl.get_viewport());
    let ctx = gl.quad_context;
    let chave = (
        m.vertices.as_ptr() as usize,
        m.vertices.len(),
        m.indices.len(),
    );
    let (vb, ib, n) = VOXEL.with(|c| {
        *c.borrow_mut().entry(chave).or_insert_with(|| {
            let vb = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&m.vertices),
            );
            let ib = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&m.indices),
            );
            (vb, ib, m.indices.len() as i32)
        })
    });
    PROGRAMAS.with(|p| {
        let mut p = p.borrow_mut();
        let p = p.get_or_insert_with(|| cria(ctx));
        abre_passe(ctx, passe, viewport);
        ctx.apply_pipeline(&p.solido);
        ctx.apply_uniforms(UniformsSource::table(&UniformesSolido {
            projection,
            model: modelo,
            recorte: Vec3::ZERO,
            recorte_z: 0.0,
            tinta,
            luz_dia: LUZ_DIA.with(|l| l.get()),
            faixas,
        }));
        ctx.apply_bindings(&Bindings {
            vertex_buffers: vec![vb],
            index_buffer: ib,
            images: vec![p.branco],
        });
        ctx.draw(0, n, 1);
        ctx.end_render_pass();
    });
}
