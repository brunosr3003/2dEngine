//! Desenho 3D com camera de cima. Nenhuma decisao de jogo mora aqui.
//!
//! A vista e' a de MMO top-down (MIR4): camera alta, inclinada, seguindo o
//! player. Como o mundo nao precisa ser complexo, o chao e' reconstruido por
//! quadro so' com os tiles visiveis — algumas centenas de quads, mais barato
//! que manter chunk em cache e invalidar.

use macroquad::prelude::*;
use macroquad::models::{Mesh, Vertex};
use shared::constants::tile_id;

use crate::map::Map;
use crate::vox::VoxCache;
use crate::world::World;

/// Um tile do servidor = uma unidade de mundo 3D.
pub const TILE: f32 = 1.0;
/// Altura da camera sobre o alvo, em tiles, no zoom padrao.
const CAM_HEIGHT: f32 = 14.0;
/// Recuo da camera atras do alvo. Junto com a altura da a inclinacao.
const CAM_BACK: f32 = 10.0;
/// Limites do zoom, como fator sobre altura e recuo.
pub const ZOOM_MIN: f32 = 0.55;
pub const ZOOM_MAX: f32 = 2.2;
/// Metade da largura da area de chao desenhada, em tiles.
const GROUND_RADIUS: i32 = 26;
/// Tamanho de um voxel em unidades de mundo. Um bicho de ~40 voxels de altura
/// fica com ~1,6 tile — a escala que a vista de cima pede.
pub const VOXEL: f32 = 0.04;

/// A camera do jogo: gira em torno do alvo (yaw), aproxima e afasta (zoom), e
/// a INCLINACAO e' fixa.
///
/// Inclinacao fixa nao e' limitacao, e' decisao: com pitch livre o jogador
/// aponta a camera pro horizonte, ve' o mundo inteiro carregando e o jogo
/// vira outra coisa. Travada, o orcamento de pedaco fecha e a leitura de cima
/// — que e' o que faz combate por alvo funcionar — nao se perde.
///
/// `chao` sobe a camera junto com o relevo; sem isso o jogador some dentro do
/// morro assim que o terreno passou a ter 34 unidades.
pub fn camera(target: Vec2, chao: f32, yaw: f32, zoom: f32) -> Camera3D {
    let z = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let recuo = CAM_BACK * z;
    let olho = vec3(
        target.x - yaw.sin() * recuo,
        chao + CAM_HEIGHT * z,
        target.y + yaw.cos() * recuo,
    );
    Camera3D {
        position: olho,
        target: vec3(target.x, chao, target.y),
        up: vec3(0.0, 1.0, 0.0),
        ..Default::default()
    }
}

/// Gira um vetor de input pra que "pra frente" seja **longe da camera**.
///
/// E' o que faz a camera girar sem virar quebra-cabeca: o jogador aperta pra
/// cima e o boneco anda pra cima da TELA, nao pro norte do mundo. A conta
/// mora no cliente e o servidor continua recebendo direcao em espaco de
/// mundo — girar a camera nao concede confianca nenhuma nova.
pub fn input_para_mundo(dir: Vec2, yaw: f32) -> Vec2 {
    // A camera fica em `(-sin·recuo, +cos·recuo)` relativa ao alvo, entao o
    // vetor camera→alvo — que e' o "pra frente" do jogador — e' `(sin, -cos)`.
    // Com W valendo `(0,-1)`, e' exatamente o que esta rotacao devolve.
    //
    // A primeira versao tinha o SINAL TROCADO e passou por um teste que so'
    // olhava se a direcao MUDAVA com o yaw. Mudava — pro lado errado. Daí os
    // asserts abaixo compararem contra a direcao esperada, e nao contra a
    // anterior.
    let (s, c) = (yaw.sin(), yaw.cos());
    vec2(dir.x * c - dir.y * s, dir.x * s + dir.y * c)
}

#[cfg(test)]
mod testes_camera {
    use super::*;

    fn perto(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-5
    }

    /// W tem que apontar pra LONGE da camera em qualquer angulo, e D pra
    /// direita da tela. E' a unica coisa que "camera relativa" quer dizer.
    #[test]
    fn w_aponta_pra_longe_da_camera() {
        let w = vec2(0.0, -1.0);
        let d = vec2(1.0, 0.0);
        for yaw in [0.0f32, 0.7, 1.5707964, 3.14159, -2.1] {
            // Direcao camera→alvo, tirada da MESMA conta que posiciona a
            // camera em `camera()`: se uma mudar sem a outra, isto quebra.
            let cam = camera(Vec2::ZERO, 0.0, yaw, 1.0);
            let frente = (cam.target - cam.position).normalize();
            let frente = vec2(frente.x, frente.z).normalize();
            let obtido = input_para_mundo(w, yaw);
            assert!(perto(obtido, frente), "yaw {yaw}: W deu {obtido:?}, esperado {frente:?}");
            // D e' a direita DA TELA. Com o eixo Z crescendo pra baixo na
            // tela, a direita de `(fx, fz)` e' `(-fz, fx)` — a mao troca em
            // relacao a' convencao 3D, e foi ai' que eu errei o primeiro
            // assert (o codigo estava certo, o teste e' que nao).
            let dir_d = input_para_mundo(d, yaw);
            let direita = vec2(-frente.y, frente.x);
            assert!(perto(dir_d, direita), "yaw {yaw}: D deu {dir_d:?}, esperado {direita:?}");
        }
    }
}

/// Tudo que traduz entre MUNDO e TELA num quadro.
///
/// Existe porque a mesma pergunta estava sendo respondida em tres lugares com
/// contas diferentes, e as tres divergiram em sequencia: a camera do desenho
/// usava um chao, a do clique usava outro; depois a mira projetava a entidade
/// no nivel do mar enquanto o desenho a punha no topo do morro. Cada um desses
/// foi um bug separado com o mesmo formato.
///
/// Agora desenho e mira **chamam a mesma funcao**. Nao e' disciplina: nao ha'
/// como divergirem porque nao existe a segunda conta.
pub struct Vista<'a> {
    pub cam: Camera3D,
    chao: &'a dyn Fn(f32, f32) -> f32,
}

impl<'a> Vista<'a> {
    /// Monta a vista do quadro: camera girada, levantada pra o relevo nao
    /// tapar o jogador, e a funcao de chao que todo o resto vai consultar.
    pub fn nova(
        alvo: Vec2,
        yaw: f32,
        zoom: f32,
        apoio: f32,
        chao: &'a dyn Fn(f32, f32) -> f32,
    ) -> Self {
        let sobe = altura_livre(alvo, apoio, yaw, zoom, chao);
        Self { cam: camera(alvo, apoio + sobe, yaw, zoom), chao }
    }

    pub fn chao_em(&self, x: f32, z: f32) -> f32 {
        (self.chao)(x, z)
    }

    /// Onde a entidade esta', em mundo. **Unico lugar que responde isso.**
    pub fn pos_de(&self, e: &crate::world::Ent) -> Vec3 {
        vec3(e.render_pos.x, e.render_y, e.render_pos.y)
    }

    /// Onde se MIRA na entidade: meio corpo acima dos pes. Clicar nos pes
    /// obriga o jogador a acertar a sombra, nao o bicho.
    pub fn mira_de(&self, e: &crate::world::Ent) -> Vec3 {
        self.pos_de(e) + vec3(0.0, 0.5, 0.0)
    }

    pub fn na_tela(&self, p: Vec3) -> Option<Vec2> {
        world_to_screen(&self.cam, p)
    }

    pub fn raio(&self, tela: Vec2) -> (Vec3, Vec3) {
        raio_da_tela(&self.cam, tela)
    }
}

/// Raio que sai da camera pelo pixel apontado.
///
/// A macroquad nao expoe unproject, entao a conta e' na mao: monta a base da
/// camera e desloca pelo tamanho do plano de projecao no alvo.
pub fn raio_da_tela(cam: &Camera3D, tela: Vec2) -> (Vec3, Vec3) {
    let frente = (cam.target - cam.position).normalize();
    let direita = frente.cross(cam.up).normalize();
    let cima = direita.cross(frente);
    let (lw, lh) = (screen_width(), screen_height());
    // NDC com Y pra cima.
    let nx = tela.x / lw * 2.0 - 1.0;
    let ny = 1.0 - tela.y / lh * 2.0;
    let fov = cam.fovy;
    let alt = (fov * 0.5).tan();
    let larg = alt * (lw / lh);
    let dir = (frente + direita * (nx * larg) + cima * (ny * alt)).normalize();
    (cam.position, dir)
}

/// Sobe a camera ate' o jogador ficar visivel.
///
/// Com a camera girando, qualquer morro entre ela e o jogador tapa a vista, e
/// perder o boneco atras do relevo e' pior que qualquer outra falha de camera.
/// Em vez de raycast contra a malha, amostra a ALTURA ao longo da linha —
/// o campo de altura ja' esta' ali, e e' consulta O(1) por amostra.
pub fn altura_livre(
    target: Vec2,
    chao: f32,
    yaw: f32,
    zoom: f32,
    altura_em: &dyn Fn(f32, f32) -> f32,
) -> f32 {
    let z = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let recuo = CAM_BACK * z;
    let mut extra: f32 = 0.0;
    const AMOSTRAS: i32 = 10;
    for i in 1..=AMOSTRAS {
        let t = i as f32 / AMOSTRAS as f32;
        let x = target.x - yaw.sin() * recuo * t;
        let zz = target.y + yaw.cos() * recuo * t;
        // Altura da linha camera→alvo neste ponto, se a camera nao subisse.
        let linha = chao + CAM_HEIGHT * z * t;
        let solo = altura_em(x, zz);
        // Uma folga acima do solo: rasar o morro deixa a camera dentro dele.
        extra = extra.max(solo + 1.5 - linha);
    }
    extra.max(0.0)
}

pub fn clear() {
    // Ceu, e nao quase-preto. O fundo aparece em todo horizonte e em todo vao
    // do relevo; escuro ele le' como buraco na malha — foi exatamente o que me
    // fez cacar bug de geometria por um bom tempo.
    clear_background(Color::from_rgba(150, 186, 214, 255));
}

fn tile_color(id: u16) -> Color {
    match id {
        tile_id::FLOOR => Color::from_rgba(58, 82, 48, 255),
        tile_id::WALL => Color::from_rgba(44, 40, 44, 255),
        tile_id::DIRT => Color::from_rgba(104, 88, 62, 255),
        tile_id::WATER => Color::from_rgba(28, 52, 88, 255),
        tile_id::DUNGEON_FLOOR => Color::from_rgba(52, 46, 58, 255),
        _ => Color::from_rgba(20, 20, 24, 255),
    }
}

/// Chao dos tiles em volta do alvo. Parede sobe um bloco pra ler como parede.
pub fn draw_ground(map: &Map, center: Vec2) {
    let cx = center.x.floor() as i32;
    let cy = center.y.floor() as i32;
    let mut verts: Vec<Vertex> = Vec::new();
    let mut idx: Vec<u16> = Vec::new();

    // A macroquad corta o desenho em 10.000 vertices por chamada (o aviso
    // "geometry() exceeded max drawcall size" some o resto do chao). Entao a
    // malha e' despejada em pedacos — e GPU de celular prefere lote menor.
    const FLUSH: usize = 2000;
    let mut quad = |verts: &mut Vec<Vertex>, idx: &mut Vec<u16>, p: [Vec3; 4], c: Color| {
        if verts.len() + 4 > FLUSH {
            draw_mesh(&Mesh {
                vertices: std::mem::take(verts),
                indices: std::mem::take(idx),
                texture: None,
            });
        }
        let b = verts.len() as u16;
        let color = [
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            255,
        ];
        for v in p {
            verts.push(Vertex { position: v, uv: vec2(0.0, 0.0), color, normal: Vec4::ZERO });
        }
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    };

    for ty in (cy - GROUND_RADIUS)..(cy + GROUND_RADIUS) {
        for tx in (cx - GROUND_RADIUS)..(cx + GROUND_RADIUS) {
            let id = map.tile(tx, ty);
            let c = tile_color(id);
            let (x0, z0) = (tx as f32 * TILE, ty as f32 * TILE);
            let (x1, z1) = (x0 + TILE, z0 + TILE);
            let h = if id == tile_id::WALL { 1.0 } else { 0.0 };
            quad(
                &mut verts,
                &mut idx,
                [
                    vec3(x0, h, z0),
                    vec3(x1, h, z0),
                    vec3(x1, h, z1),
                    vec3(x0, h, z1),
                ],
                c,
            );
            // Lateral norte da parede, so' pra ela nao parecer um decalque.
            if h > 0.0 {
                let side = Color::new(c.r * 0.7, c.g * 0.7, c.b * 0.7, 1.0);
                quad(
                    &mut verts,
                    &mut idx,
                    [
                        vec3(x0, 0.0, z1),
                        vec3(x1, 0.0, z1),
                        vec3(x1, h, z1),
                        vec3(x0, h, z1),
                    ],
                    side,
                );
            }
        }
    }
    if !verts.is_empty() {
        draw_mesh(&Mesh { vertices: verts, indices: idx, texture: None });
    }
}

/// Modelo `.vox` de cada entidade.
///
/// Orcamento de arte: **mob comum e' barato, boss e' caro**. O `lobo.vox`
/// original da 14.112 triangulos — isso e' modelo de boss, do qual ha um na
/// tela. Mob comum usa a versao reduzida (`tools/voxrender/voxsimplify.py`),
/// menor e com uma fracao dos triangulos, porque ha dezenas deles.
fn model_for(tag: shared::EntityTag, boss: bool) -> Option<&'static str> {
    use shared::EntityTag as T;
    match tag {
        T::Player | T::Npc => Some("player"),
        T::Enemy if boss => Some("lobo"),
        T::Enemy => Some("lobo_pequeno"),
        _ => None,
    }
}

pub fn draw_entities(
    world: &mut World,
    vox: &VoxCache,
    target: Option<shared::EntityId>,
    vista: &Vista,
) {
    let order: Vec<_> = world.draw_order().to_vec();
    for id in order {
        let Some(e) = world.ents.get(&id) else { continue };
        let p = vista.pos_de(e);

        // Marca do alvo: anel no chao, que e' como MMO de target sinaliza.
        if Some(id) == target {
            draw_ring(p, 0.55, Color::from_rgba(241, 200, 112, 255));
        }

        let boss = e.state.flags & shared::ent_flags::BOSS != 0;
        let drawn = model_for(e.meta.tag, boss)
            .and_then(|name| vox.peek(name))
            .map(|meshes| {
                // A malha nasce centrada em X/Z e apoiada em Y=0; girar em
                // torno de Y bastaria, mas a macroquad nao transforma malha —
                // entao a rotacao vira quando houver malha por direcao.
                for m in meshes {
                    draw_mesh_at(m, p, e.yaw);
                }
            })
            .is_some();

        if !drawn {
            use shared::EntityTag as T;
            let c = match e.meta.tag {
                T::Enemy => Color::from_rgba(200, 85, 61, 255),
                T::Projectile => Color::from_rgba(255, 240, 150, 255),
                T::Loot => Color::from_rgba(150, 220, 120, 255),
                _ => Color::from_rgba(160, 160, 170, 255),
            };
            draw_cube(p + vec3(0.0, 0.35, 0.0), vec3(0.6, 0.7, 0.6), None, c);
        }
    }
}

/// Desenha uma malha girada em Y e deslocada.
///
/// A macroquad nao tem transform por malha, entao a matriz e' aplicada nos
/// vertices na CPU. Cabe porque os modelos sao pequenos depois do greedy
/// meshing (o player tem 356 triangulos); se o bestiario crescer, o caminho e'
/// um shader com uniform de modelo.
fn draw_mesh_at(m: &Mesh, at: Vec3, yaw: f32) {
    let (sin, cos) = yaw.sin_cos();
    let moved = Mesh {
        vertices: m
            .vertices
            .iter()
            .map(|v| {
                let q = v.position;
                Vertex {
                    position: vec3(
                        q.x * cos + q.z * sin + at.x,
                        q.y + at.y,
                        -q.x * sin + q.z * cos + at.z,
                    ),
                    ..*v
                }
            })
            .collect(),
        indices: m.indices.clone(),
        texture: None,
    };
    draw_mesh(&moved);
}

/// Projeta um ponto do mundo pra pixel de tela.
///
/// Necessario pra mirar com o mouse: a selecao de alvo compara a distancia em
/// PIXELS entre o cursor e cada entidade, que e' o que o jogador enxerga.
pub fn world_to_screen(cam: &Camera3D, p: Vec3) -> Option<Vec2> {
    let clip = cam.matrix() * p.extend(1.0);
    if clip.w <= 0.0 {
        return None; // atras da camera
    }
    let ndc = clip.truncate() / clip.w;
    Some(vec2(
        (ndc.x * 0.5 + 0.5) * screen_width(),
        (1.0 - (ndc.y * 0.5 + 0.5)) * screen_height(),
    ))
}

/// Entidade mais perto do cursor, dentro de um raio em pixels.
///
/// Ignora o proprio player: em MMO de target, clicar em si mesmo nunca e' a
/// intencao de atacar.
pub fn pick(world: &World, vista: &Vista, mouse: Vec2, raio_px: f32) -> Option<shared::EntityId> {
    let mut melhor: Option<(f32, shared::EntityId)> = None;
    for (id, e) in &world.ents {
        if e.is_self() {
            continue;
        }
        let Some(sp) = vista.na_tela(vista.mira_de(e)) else { continue };
        let d = sp.distance(mouse);
        if d <= raio_px && melhor.map_or(true, |(bd, _)| d < bd) {
            melhor = Some((d, *id));
        }
    }
    melhor.map(|(_, id)| id)
}

fn draw_ring(center: Vec3, r: f32, color: Color) {
    const N: usize = 24;
    for i in 0..N {
        let a0 = i as f32 / N as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / N as f32 * std::f32::consts::TAU;
        draw_line_3d(
            center + vec3(a0.cos() * r, 0.02, a0.sin() * r),
            center + vec3(a1.cos() * r, 0.02, a1.sin() * r),
            color,
        );
    }
}
