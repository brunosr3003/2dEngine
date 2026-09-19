//! Barra de skills e efeitos confirmados pelo servidor.
use crate::{
    habilidades_input::{Arrasto, Gesto},
    hud_estilo as estilo,
    render3d::Vista,
    world::World,
};
use macroquad::prelude::*;
use shared::skills::{Conjunto, Skill};
use shared::EntityId;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Habilidades {
    pub catalogo: Vec<Skill>,
    recargas: HashMap<u32, f64>,
    ocupada_ate: f64,
    pendente_ate: f64,
    aviso: Option<(String, f64)>,
    efeitos: Vec<Efeito>,
    luz: Option<macroquad::material::Material>,
    pub automaticas: HashSet<u32>,
    /// O jogador LIGOU o automatico de uma skill neste quadro (o passo
    /// tutorial conta o gesto; o `main` le' e zera).
    pub ligou_auto: bool,
    arrasto: Arrasto,
    tentar_apos: HashMap<u32, f64>,
    ultimo_auto: u32,
}

#[derive(Clone, Copy)]
pub struct Contexto {
    pub conjunto: Conjunto,
    pub nivel: u32,
    pub mp: i32,
    pub vivo: bool,
    pub vida_baixa: bool,
    pub distancia_alvo: Option<f32>,
}

struct Efeito {
    skill: Skill,
    dono: EntityId,
    de: Vec2,
    alvo: Vec2,
    alvo_eid: Option<EntityId>,
    inicio: f64,
    impacto: bool,
}

/// O slot `slot` (0 = skill 1) no arco em volta do ATACAR. Ver `hud_layout`.
pub fn retangulo(slot: usize) -> Rect {
    crate::hud_layout::atual().skills[slot.min(3)]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exemplo() -> (Habilidades, Contexto) {
        let mut h = Habilidades::default();
        h.catalogo = shared::skills::playtest();
        h.automaticas.extend(1..=12);
        (
            h,
            Contexto {
                conjunto: Conjunto::Katana,
                nivel: 20,
                mp: 100,
                vivo: true,
                vida_baixa: false,
                distancia_alvo: Some(2.0),
            },
        )
    }
    #[test]
    fn auto_respeita_alvo_recarga_mana_nivel_arma_e_estado() {
        let (mut h, mut c) = exemplo();
        assert_eq!(h.proximo_auto(c, 10.0), Some(4));
        h.recargas.insert(4, 12.0);
        assert_eq!(h.proximo_auto(c, 10.0), Some(5));
        c.distancia_alvo = Some(7.0);
        assert_eq!(h.proximo_auto(c, 10.0), Some(6));
        c.distancia_alvo = None;
        assert_eq!(h.proximo_auto(c, 10.0), None);
        c.distancia_alvo = Some(2.0);
        c.nivel = 1;
        assert_eq!(h.proximo_auto(c, 10.0), None);
        c.nivel = 20;
        c.mp = 0;
        assert_eq!(h.proximo_auto(c, 10.0), None);
        c.mp = 100;
        c.vivo = false;
        assert_eq!(h.proximo_auto(c, 10.0), None);
        c.vivo = true;
        h.pendente_ate = 11.0;
        assert_eq!(h.proximo_auto(c, 10.0), None);
        h.pendente_ate = 0.0;
        h.ocupada_ate = 11.0;
        assert_eq!(h.proximo_auto(c, 10.0), None);
    }
    #[test]
    fn suporte_nao_gasta_mana_sem_necessidade_e_rejeicao_tem_intervalo() {
        let (mut h, mut c) = exemplo();
        c.conjunto = Conjunto::AnelMagico;
        c.distancia_alvo = None;
        assert_eq!(h.proximo_auto(c, 10.0), None);
        c.vida_baixa = true;
        assert_eq!(h.proximo_auto(c, 10.0), Some(10));
        h.tentar_apos.insert(10, 12.5);
        assert_eq!(h.proximo_auto(c, 10.0), Some(11));
        h.tentar_apos.insert(11, 12.5);
        assert_eq!(h.proximo_auto(c, 10.0), None);
        assert_eq!(h.proximo_auto(c, 13.0), Some(10));
        c.conjunto = Conjunto::EspadaEscudo;
        c.vida_baixa = false;
        assert_eq!(h.proximo_auto(c, 13.0), None);
    }
}

impl Habilidades {
    pub fn estado(&mut self, cooldowns: Vec<(u32, f32)>, busy_s: f32) {
        let agora = get_time();
        self.recargas = cooldowns
            .into_iter()
            .map(|(id, t)| (id, agora + t.max(0.0) as f64))
            .collect();
        self.ocupada_ate = agora + busy_s.max(0.0) as f64;
        self.pendente_ate = 0.0;
    }

    pub fn aviso(&mut self, msg: String) {
        self.aviso = Some((msg, get_time() + 3.0));
        self.pendente_ate = 0.0;
    }

    pub fn rejeitada(&mut self, id: u32, msg: String) {
        self.tentar_apos.insert(id, get_time() + 2.5);
        self.aviso(msg);
    }

    pub fn cancela_arrasto(&mut self) {
        self.arrasto.cancela();
    }

    /// O aviso das skills ainda valendo — vai pra faixa de estado unica, com
    /// prioridade sobre as outras.
    pub fn aviso_ativo(&self) -> Option<&str> {
        self.aviso
            .as_ref()
            .filter(|(_, ate)| get_time() < *ate)
            .map(|(m, _)| m.as_str())
    }

    pub fn ocupada(&self) -> bool {
        get_time() < self.ocupada_ate || get_time() < self.pendente_ate
    }

    /// O ponto `p` cai num botao de skill (ou no de pulo)? Sem olhar arrasto
    /// em andamento — e' o que o joystick pergunta pro OUTRO dedo.
    pub fn botao_em(&self, p: Vec2) -> bool {
        (0..4).any(|i| retangulo(i).contains(p))
    }

    pub fn pega_mouse(&self) -> bool {
        let (x, y) = mouse_position();
        // 0..4: o quarto e' o botao de PULO, e tocar nele tambem nao pode
        // virar clique no mundo.
        self.arrasto.inicio.is_some() || (0..4).any(|i| retangulo(i).contains(vec2(x, y)))
    }

    pub fn pedido(&mut self, contexto: Contexto) -> Option<u32> {
        let teclas = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3];
        let mouse = Vec2::from(mouse_position());
        let agora = get_time();
        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(s) = self.catalogo.iter().find(|s| {
                s.conjunto == contexto.conjunto
                    && retangulo(s.ordem.saturating_sub(1) as usize).contains(mouse)
            }) {
                self.arrasto.pressiona(s.id, mouse);
            }
        }
        self.arrasto.move_para(mouse);
        let mut gesto = None;
        if let Some((id, _)) = self.arrasto.inicio {
            if let Some(s) = self
                .catalogo
                .iter()
                .find(|s| s.id == id && s.conjunto == contexto.conjunto)
            {
                if is_mouse_button_released(MouseButton::Left) {
                    gesto = self.arrasto.solta(mouse, retangulo(s.ordem as usize - 1));
                } else if !is_mouse_button_down(MouseButton::Left) {
                    self.arrasto.cancela();
                }
            } else {
                self.arrasto.cancela();
            }
        }
        if let Some(Gesto::Auto(id, ligar)) = gesto {
            if self
                .catalogo
                .iter()
                .any(|s| s.id == id && s.destravada(contexto.nivel))
            {
                if ligar {
                    self.automaticas.insert(id);
                    self.ligou_auto = true;
                } else {
                    self.automaticas.remove(&id);
                }
                self.aviso(
                    if ligar {
                        "Uso automático ativado"
                    } else {
                        "Uso automático desativado"
                    }
                    .into(),
                );
            } else {
                self.aviso("Esta skill ainda está bloqueada".into());
            }
            return None;
        }
        let manual = self
            .catalogo
            .iter()
            .find(|s| {
                s.conjunto == contexto.conjunto
                    && is_key_pressed(teclas[s.ordem.saturating_sub(1).min(2) as usize])
            })
            .map(|s| s.id)
            .or(match gesto {
                Some(Gesto::Usar(id)) => Some(id),
                _ => None,
            });
        if let Some(id) = manual {
            let s = self.catalogo.iter().find(|s| s.id == id)?;
            let erro = if !s.destravada(contexto.nivel) {
                Some(format!("Libera no nível {}", s.nivel_necessario()))
            } else if !contexto.vivo {
                Some("Personagem incapacitado".into())
            } else if self.ocupada() {
                Some("Aguarde terminar a ação atual".into())
            } else if self.recargas.get(&id).is_some_and(|&t| t > agora) {
                Some("Skill em recarga".into())
            } else if contexto.mp < s.custo_mp {
                Some("Mana insuficiente".into())
            } else if s.dano > 0 && contexto.distancia_alvo.is_none() {
                Some("Selecione um inimigo para usar esta skill".into())
            } else {
                None
            };
            if let Some(msg) = erro {
                self.aviso(msg);
                return None;
            }
            self.pendente_ate = agora + 1.5;
            return Some(id);
        }
        if self.arrasto.inicio.is_some() {
            return None;
        }
        let id = self.proximo_auto(contexto, agora)?;
        self.ultimo_auto = id;
        self.pendente_ate = agora + 1.5;
        self.tentar_apos.insert(id, agora + 2.5);
        Some(id)
    }

    fn proximo_auto(&self, c: Contexto, agora: f64) -> Option<u32> {
        if !c.vivo || agora < self.ocupada_ate || agora < self.pendente_ate {
            return None;
        }
        self.catalogo
            .iter()
            .filter(|s| {
                s.conjunto == c.conjunto
                    && self.automaticas.contains(&s.id)
                    && s.destravada(c.nivel)
                    && c.mp >= s.custo_mp
                    && self.recargas.get(&s.id).copied().unwrap_or(0.0) <= agora
                    && self.tentar_apos.get(&s.id).copied().unwrap_or(0.0) <= agora
                    && if s.dano > 0 {
                        c.distancia_alvo.is_some_and(|d| d <= s.alcance_alvo())
                    } else if s.cura > 0 {
                        c.vida_baixa
                    } else {
                        c.distancia_alvo.is_some_and(|d| d <= 8.0)
                    }
            })
            .min_by_key(|s| {
                (
                    if s.cura > 0 && c.vida_baixa { 0 } else { 1 },
                    if s.id > self.ultimo_auto { 0 } else { 1 },
                    s.id,
                )
            })
            .map(|s| s.id)
    }

    pub fn cancelar(&mut self, dono: EntityId, id: u32, proprio: bool) {
        self.efeitos
            .retain(|e| e.dono != dono || e.skill.id != id || e.impacto);
        if proprio {
            self.recargas.remove(&id);
            self.ocupada_ate = 0.0;
            self.pendente_ate = 0.0;
            self.tentar_apos.insert(id, get_time() + 2.5);
            self.aviso("Skill interrompida".into());
        }
    }

    pub fn efeito(
        &mut self,
        id: u32,
        dono: EntityId,
        de: Vec2,
        alvo: Vec2,
        alvo_eid: Option<EntityId>,
        impacto: bool,
    ) {
        let Some(skill) = self.catalogo.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if impacto {
            self.efeitos
                .retain(|e| e.dono != dono || e.skill.id != id || e.impacto);
        }
        self.efeitos.push(Efeito {
            skill,
            dono,
            de,
            alvo,
            alvo_eid,
            inicio: get_time(),
            impacto,
        });
    }

    pub fn acompanhar_alvos(&self, world: &mut World) {
        for e in self
            .efeitos
            .iter()
            .filter(|e| !e.impacto && get_time() - e.inicio < e.skill.impacto_em() as f64)
        {
            let Some(alvo) = e
                .alvo_eid
                .filter(|id| *id != e.dono)
                .and_then(|id| world.ents.get(&id))
                .map(|p| p.render_pos)
            else {
                continue;
            };
            if let Some(dono) = world.ents.get_mut(&e.dono) {
                dono.mira = Some((alvo, 0.3));
            }
        }
    }

    pub fn barra(&self, conjunto: Conjunto, nivel: u32, mp: i32) {
        let agora = get_time();
        // O slot 4 e' o PULO (`hud::draw_pulo`): era um disco cinza sem
        // funcao, e no celular nao ha' tecla de espaco pra pular.
        for i in 0..3 {
            let r = retangulo(i);
            let skill = self
                .catalogo
                .iter()
                .find(|s| s.conjunto == conjunto && s.ordem as usize == i + 1);
            let Some(s) = skill else { continue };
            let cd = (self.recargas.get(&s.id).copied().unwrap_or(0.0) - agora).max(0.0);
            let livre = s.destravada(nivel);
            let pronto = livre && cd == 0.0 && mp >= s.custo_mp && !self.ocupada();
            let auto = self.automaticas.contains(&s.id);
            let c = vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
            let raio = r.w * 0.5;
            let cor = if livre {
                estilo::cor_skill(s.id)
            } else {
                estilo::SUAVE
            };
            let sobre_botao = r.contains(Vec2::from(mouse_position()));
            let e = estilo::estado(
                sobre_botao,
                is_mouse_button_down(MouseButton::Left),
                !livre,
                false,
            );
            estilo::botao_redondo(
                c,
                raio,
                if auto { estilo::AUTO } else { cor },
                e,
                pronto && auto,
            );
            let alfa = if pronto { 1.0 } else { 0.40 };
            // Arte colorida da skill no disco da arma; o vetor so' se faltar.
            if !crate::icones_ui::skill(s.id, c, raio * 1.5, if livre { alfa } else { 0.30 }) {
                estilo::icone(s.id, c, raio * 0.58, Color::new(cor.r, cor.g, cor.b, alfa));
            }
            if cd > 0.0 {
                let f = (cd as f32 / s.espera_s.max(0.01)).min(1.0);
                estilo::setor(c, raio - 3.0, f, Color::new(0.0, 0.0, 0.0, 0.62));
                estilo::arco(
                    c,
                    raio - 2.0,
                    -std::f32::consts::FRAC_PI_2,
                    f,
                    2.0,
                    estilo::OURO,
                );
                let t = format!("{cd:.1}");
                estilo::texto_sombra(
                    c.x - estilo::medir_forte(&t, 22) * 0.5,
                    c.y + 8.0,
                    &t,
                    22,
                    WHITE,
                    true,
                );
            } else if !livre {
                estilo::texto_centro(
                    c.x,
                    c.y + 7.0,
                    &format!("Lv {}", s.nivel_necessario()),
                    18,
                    estilo::TEXTO,
                );
            }
            crate::hud_layout::chip(r, &(i + 1).to_string());
            if auto {
                let pilula = Rect::new(c.x - 22.0, r.y + r.h - 10.0, 44.0, 17.0);
                estilo::ret_arredondado(pilula, 8.5, estilo::FUNDO_BAIXO);
                estilo::borda_arredondada(pilula, 8.5, 1.0, estilo::alfa(estilo::AUTO, 0.7));
                estilo::texto_centro_forte(c.x, r.y + r.h + 3.0, "AUTO", 11, estilo::AUTO);
            }
            estilo::texto_centro(
                c.x,
                r.y + r.h + 23.0,
                &format!("{} MP", s.custo_mp),
                13,
                if mp < s.custo_mp {
                    ORANGE
                } else {
                    estilo::SUAVE
                },
            );
            let arrastando = self.arrasto.inicio.is_some_and(|(id, _)| id == s.id);
            if arrastando {
                let (_, inicio) = self.arrasto.inicio.unwrap();
                let p = Vec2::from(mouse_position());
                let texto = if p.y > inicio.y + 20.0 {
                    "Solte: MANUAL"
                } else {
                    "Solte: AUTO"
                };
                estilo::painel(Rect::new(c.x - 82.0, r.y - 74.0, 164.0, 31.0));
                estilo::texto_centro(c.x, r.y - 53.0, texto, 15, estilo::AUTO);
                draw_line(c.x, r.y - 8.0, c.x, r.y - 35.0, 2.0, estilo::AUTO);
            } else if r.contains(Vec2::from(mouse_position())) && self.arrasto.inicio.is_none() {
                let w = 380.0_f32.min(screen_width() - 24.0);
                let x = (r.x + r.w - w).clamp(12.0, screen_width() - w - 12.0);
                estilo::painel(Rect::new(x, r.y - 122.0, w, 88.0));
                estilo::texto(x + 12.0, r.y - 98.0, &s.nome, 19, cor);
                estilo::texto_ajustado(
                    s.descricao(),
                    x + 12.0,
                    r.y - 77.0,
                    w - 24.0,
                    14,
                    estilo::TEXTO,
                );
                estilo::texto(
                    x + 12.0,
                    r.y - 53.0,
                    &format!(
                        "Lv {}  ·  {} MP  ·  {:.0}s  ·  {}  ·  pra cima: AUTO · pra baixo: manual",
                        s.nivel_necessario(),
                        s.custo_mp,
                        s.espera_s,
                        if auto { "AUTO" } else { "MANUAL" }
                    ),
                    13,
                    estilo::SUAVE,
                );
            }
        }
    }

    pub fn desenha_efeitos(&mut self, world: &World, vista: &Vista) {
        let agora = get_time();
        self.efeitos.retain(|e| {
            agora - e.inicio
                < if e.impacto {
                    crate::habilidades_vfx::duracao(e.skill.id) as f64
                } else {
                    e.skill.impacto_em() as f64 + 0.5
                }
        });
        if self.efeitos.is_empty() {
            return;
        }
        let luz = self
            .luz
            .get_or_insert_with(crate::habilidades_vfx::material);
        for e in &self.efeitos {
            let dono = world.ents.get(&e.dono);
            let pos = dono.map_or(e.de, |p| p.render_pos);
            let alvo = e
                .alvo_eid
                .and_then(|id| world.ents.get(&id))
                .map_or(e.alvo, |p| p.render_pos);
            let ponto = |p: Vec2| vec3(p.x, vista.chao_em(p.x, p.y), p.y);
            let de = if e.impacto && e.skill.dano > 0 {
                ponto(e.de)
            } else {
                ponto(pos)
            };
            let maos = dono.map_or([de + Vec3::Y; 2], |d| d.emissores);
            crate::habilidades_vfx::desenha(
                &crate::habilidades_vfx::Cena {
                    id: e.skill.id,
                    de,
                    alvo: ponto(alvo),
                    maos,
                    t: (agora - e.inicio) as f32,
                    impacto: e.impacto,
                    atraso: e.skill.impacto_em(),
                    raio: e.skill.raio,
                    frente: dono.map_or(Vec3::Z, |d| vec3(d.yaw.sin(), 0.0, d.yaw.cos())),
                },
                luz,
            );
        }
    }
}
