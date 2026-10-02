//! The gathering bar: "Gathering · Blue stone · 2.4 s", filling up to the
//! next cycle. The server sends the interval and the progress on starting, on
//! every cycle and on stopping (`ColetaEstado`); between one message and the
//! next the client only runs the clock.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;
use crate::hud_layout::Zonas;

#[derive(Default)]
pub struct BarraDeColeta {
    tipo: Option<u8>,
    intervalo: f32,
    base: f32,
    desde: f64,
    /// Center of the gathered node (to turn the character towards it).
    pub centro: Option<Vec2>,
    /// Bag full: the bar stops and warns, without disappearing.
    pub pausado: bool,
}

impl BarraDeColeta {
    pub fn recebe(
        &mut self,
        tipo: u8,
        intervalo_s: f32,
        progresso: f32,
        centro: Option<[f32; 2]>,
        pausado: bool,
        agora: f64,
    ) {
        if tipo == shared::protocol::COLETA_PARADA || intervalo_s <= 0.0 {
            *self = Self::default();
            return;
        }
        if !pausado && progresso < 0.1 && tipo < shared::forte::TIPO_COLETA { crate::sons::tocar(if tipo == 0 {crate::sons::Som::Machado} else {crate::sons::Som::Picareta}); }
        *self = Self {
            tipo: Some(tipo),
            intervalo: intervalo_s,
            base: progresso.clamp(0.0, 1.0),
            desde: agora,
            centro: centro.map(|c| vec2(c[0], c[1])),
            pausado,
        };
    }

    pub fn ativa(&self) -> bool {
        self.tipo.is_some()
    }

    /// 0..1 until the next cycle.
    pub fn progresso(&self, agora: f64) -> f32 {
        if self.intervalo <= 0.0 || self.pausado {
            return 0.0;
        }
        (self.base + ((agora - self.desde) as f32 / self.intervalo)).clamp(0.0, 1.0)
    }

    /// Seconds until the next cycle.
    pub fn restante_s(&self, agora: f64) -> f32 {
        (1.0 - self.progresso(agora)) * self.intervalo
    }

    pub fn desenha(&self, z: &Zonas, agora: f64) {
        let Some(tipo) = self.tipo else { return };
        let r = z.coleta;
        estilo::painel(r);
        let dentro = Rect::new(r.x + 6.0, r.y + r.h - 9.0, r.w - 12.0, 5.0);
        draw_rectangle(
            dentro.x,
            dentro.y,
            dentro.w,
            dentro.h,
            Color::new(0.0, 0.0, 0.0, 0.6),
        );
        draw_rectangle(
            dentro.x,
            dentro.y,
            dentro.w * self.progresso(agora),
            dentro.h,
            estilo::AUTO,
        );
        if self.pausado {
            draw_rectangle(
                dentro.x,
                dentro.y,
                dentro.w,
                dentro.h,
                Color::new(0.85, 0.30, 0.25, 0.9),
            );
            estilo::texto_centro(
                r.center().x,
                r.y + r.h * 0.5 + 1.0,
                "Bag full — gathering paused",
                13,
                estilo::TEXTO,
            );
            return;
        }
        let verbo = if tipo > shared::forte::TIPO_COLETA { "Opening" } else { "Gathering" };
        let texto = format!(
            "{verbo} · {} · {:.1} s",
            shared::nome_do_no(tipo),
            self.restante_s(agora)
        )
        .replace('.', ",");
        estilo::texto_centro(
            r.center().x,
            r.y + r.h * 0.5 + 1.0,
            &texto,
            13,
            estilo::TEXTO,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enche_ate_o_ciclo_e_some_ao_parar() {
        let mut b = BarraDeColeta::default();
        assert!(!b.ativa());
        b.recebe(3, 2.0, 0.0, Some([4.0, 5.0]), false, 10.0);
        assert!(b.ativa());
        assert_eq!(b.centro, Some(vec2(4.0, 5.0)));
        assert!((b.progresso(11.0) - 0.5).abs() < 1e-5);
        assert!((b.restante_s(11.0) - 1.0).abs() < 1e-5);
        assert_eq!(
            b.progresso(99.0),
            1.0,
            "nao passa de cheio antes do proximo aviso"
        );
        // Bag full: stays active, stopped.
        b.recebe(3, 2.0, 0.0, Some([4.0, 5.0]), true, 12.0);
        assert!(b.ativa() && b.pausado);
        assert_eq!(b.progresso(20.0), 0.0, "pausada nao anda");
        b.recebe(shared::protocol::COLETA_PARADA, 0.0, 0.0, None, false, 12.0);
        assert!(!b.ativa());
    }

    /// The gathering gesture that comes off the wire picks the tool by TYPE, the
    /// clock runs while it lasts and zeroes when it goes out.
    #[test]
    fn gesto_de_coleta_leva_o_tipo_e_o_relogio() {
        use shared::components::acao;
        use shared::{EntityId, EntityMeta, EntityState, EntityTag};
        let mut w = crate::world::World::default();
        let meta = EntityMeta { skins: 0,
            pk: Default::default(),
            auras: 0,
            id: EntityId(1),
            tag: EntityTag::Player,
            name: None,
            hp_max: 10,
            faction: None,
            kind: 0,
            nivel: 1,
            desafio: None,
            aparencia: 0,
        };
        let mut st = EntityState::quantize(
            EntityId(1),
            ::glam::Vec2::new(1.0, 1.0),
            ::glam::Vec2::ZERO,
            10,
            0,
        );
        st.acao = acao::monta_coleta(0, false, 3);
        w.apply(vec![meta.clone()], vec![st], &[]);
        for _ in 0..60 {
            w.tick(1.0 / 30.0, &|_, _| 0.0);
        }
        let e = &w.ents[&EntityId(1)];
        assert_eq!(e.coleta, Some(3));
        assert!(
            (e.coleta_t - 2.0).abs() < 0.05,
            "relogio do gesto: {}",
            e.coleta_t
        );
        assert_eq!(crate::rig::ferramenta_de(e.coleta.unwrap()), "picareta_3");
        // Switching to wood: zeroes the clock and takes the axe.
        st.acao = acao::monta_coleta(0, false, 0);
        w.apply(vec![meta.clone()], vec![st], &[]);
        assert_eq!(w.ents[&EntityId(1)].coleta, Some(0));
        assert_eq!(w.ents[&EntityId(1)].coleta_t, 0.0);
        st.acao = acao::monta(0, false, acao::NADA, 0);
        w.apply(vec![meta], vec![st], &[]);
        w.tick(1.0 / 30.0, &|_, _| 0.0);
        assert!(w.ents[&EntityId(1)].coleta.is_none(), "gesto apagou: para");
    }
}
