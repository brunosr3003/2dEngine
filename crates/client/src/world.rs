//! Estado visual do mundo entre snapshots.
//!
//! O servidor tica a 30Hz e o cliente desenha a 60+. Desenhar direto a posicao
//! do snapshot faz tudo andar aos trancos, entao cada entidade guarda uma
//! posicao RENDERIZADA que persegue a autoritativa.
//!
//! Nada aqui e' simulacao: a posicao alvo vem sempre do servidor, o cliente so'
//! suaviza o caminho ate ela. Sem prediction, sem regra de jogo.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::{ent_flags, EntityId, EntityMeta, EntityState};

/// A macroquad embute a propria glam e reexporta o nome no prelude, entao os
/// dois `Vec2` sao tipos distintos pro compilador. A conversao fica so' na
/// fronteira com o `shared`.
#[inline]
fn mq(v: ::glam::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

/// Constante da suavizacao exponencial. Maior = mais colado no servidor e mais
/// tranco; menor = mais macio e mais atrasado. 18 da ~90% do caminho em 130ms,
/// que cobre o intervalo de 33ms entre snapshots com folga.
const SMOOTH_K: f32 = 18.0;
/// Abaixo disto a entidade e' considerada parada.
const MOVING_EPS: f32 = 0.05;

pub struct Ent {
    /// Dado estavel, recebido uma vez quando a entidade entrou no AOI.
    pub meta: EntityMeta,
    /// Ultimo estado autoritativo.
    pub state: EntityState,
    /// Posicao desenhada, em tiles. Persegue a do servidor.
    pub render_pos: Vec2,
    /// Angulo em torno de Y, em radianos. Em 3D a direcao e' continua — nao
    /// ha 4 ou 8 sprites pra escolher —, entao ela persegue a velocidade.
    pub yaw: f32,
    /// Altura desenhada. Persegue o apoio em vez de saltar pra ele: degrau de
    /// meio metro trocado de uma vez faz o modelo piscar pra cima.
    /// `f32::MIN` = ainda nao apoiado (o primeiro quadro assenta sem animar).
    pub render_y: f32,
}

impl Ent {
    pub fn is_self(&self) -> bool {
        self.state.flags & ent_flags::SELF != 0
    }

    pub fn walking(&self) -> bool {
        mq(self.state.vel_f32()).length_squared() > MOVING_EPS * MOVING_EPS
    }
}

#[derive(Default)]
pub struct World {
    pub ents: HashMap<EntityId, Ent>,
    /// Ordem estavel de desenho (y crescente) recalculada por quadro.
    order: Vec<EntityId>,
    pub self_id: Option<EntityId>,
}

impl World {
    /// Aplica um tick em DELTA.
    ///
    /// `entered` traz o dado estavel de quem acabou de entrar no campo de
    /// visao; `states` traz so' quem mudou; `removed` quem saiu. Entidade
    /// ausente das tres listas esta parada — nao sumiu.
    pub fn apply(
        &mut self,
        entered: Vec<EntityMeta>,
        states: Vec<EntityState>,
        removed: &[EntityId],
    ) {
        for meta in entered {
            let id = meta.id;
            // O estado real vem no mesmo pacote, logo abaixo.
            let state = EntityState { id, pos: [0, 0], vel: [0, 0], hp: 0, flags: 0 };
            self.ents.entry(id).or_insert(Ent {
                meta,
                state,
                render_pos: Vec2::ZERO,
                yaw: 0.0,
                render_y: f32::MIN,
            });
        }
        for st in states {
            let Some(ent) = self.ents.get_mut(&st.id) else { continue };
            // Entidade recem-criada nasce ja na posicao certa, senao ela
            // desliza do canto do mundo ate o lugar dela.
            if ent.state.pos == [0, 0] {
                ent.render_pos = mq(st.pos_f32());
            }
            if st.flags & ent_flags::SELF != 0 {
                self.self_id = Some(st.id);
            }
            ent.state = st;
        }
        for id in removed {
            self.ents.remove(id);
            if self.self_id == Some(*id) {
                self.self_id = None;
            }
        }
    }

    /// `chao` da' a altura de apoio: e' o mesmo campo de altura que o servidor
    /// usa pra colisao, entao a entidade pisa exatamente onde ela pisa la'.
    pub fn tick(&mut self, dt: f32, chao: &dyn Fn(f32, f32) -> f32) {
        // Peso da suavizacao independente do frame rate.
        let a = 1.0 - (-SMOOTH_K * dt).exp();
        // O apoio troca de bloco de uma vez; seguir mais rapido que a posicao
        // faz o degrau subir junto com o passo em vez de depois dele.
        let ay = 1.0 - (-14.0 * dt).exp();
        for ent in self.ents.values_mut() {
            let target = mq(ent.state.pos_f32());
            ent.render_pos += (target - ent.render_pos) * a;

            let apoio = chao(ent.render_pos.x, ent.render_pos.y);
            ent.render_y = if ent.render_y == f32::MIN {
                // Primeiro quadro assenta sem animar, senao a entidade cai do
                // ceu ao entrar no AOI.
                apoio
            } else {
                ent.render_y + (apoio - ent.render_y) * ay
            };

            let vel = mq(ent.state.vel_f32());
            if vel.length_squared() > MOVING_EPS * MOVING_EPS {
                // O modelo nasce olhando pro +Z do mundo.
                let want = vel.x.atan2(vel.y);
                // Caminho mais curto no circulo, senao ele gira 350 graus pra
                // virar 10.
                let mut d = want - ent.yaw;
                while d > std::f32::consts::PI { d -= std::f32::consts::TAU; }
                while d < -std::f32::consts::PI { d += std::f32::consts::TAU; }
                ent.yaw += d * a;
            }
        }
    }

    pub fn self_pos(&self) -> Option<Vec2> {
        self.self_id.and_then(|id| self.ents.get(&id)).map(|e| e.render_pos)
    }

    /// Entidades ordenadas por y — quem esta mais ao sul desenha por cima.
    pub fn draw_order(&mut self) -> &[EntityId] {
        self.order.clear();
        self.order.extend(self.ents.keys().copied());
        let ents = &self.ents;
        self.order.sort_by(|a, b| {
            let ya = ents[a].render_pos.y;
            let yb = ents[b].render_pos.y;
            ya.partial_cmp(&yb).unwrap_or(std::cmp::Ordering::Equal)
        });
        &self.order
    }
}
