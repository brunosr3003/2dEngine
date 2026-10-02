//! Bestiary of the areas visited this session. All the combat numbers come
//! from the server's goal, after the tier's real scaling.
use macroquad::prelude::*;

use crate::{hud_estilo as estilo, world::World};

#[derive(Clone)]
pub struct PedidoIr { pub kind: u16, pub nivel: u16, pub chefe: bool, pub nome: String }

#[derive(Clone)]
struct Registro {
    kind: u16,
    nivel: u16,
    nome: String,
    vida: u32,
    chefe: bool,
    desafio: shared::DesafioMob,
}

#[derive(Default)]
pub struct MobsUi {
    pub aberto: bool,
    registros: Vec<Registro>,
    selecionado: Option<usize>,
    pagina: usize,
}

impl MobsUi {
    pub fn abrir(&mut self) { self.aberto = true; }
    pub fn fechar(&mut self) { self.aberto = false; }

    pub fn catalogo(&mut self, mobs: Vec<shared::protocol::MobNoCatalogo>) {
        let chave = self.selecionado.and_then(|i| self.registros.get(i))
            .map(|r| (r.kind, r.nivel, r.chefe));
        for m in mobs {
            if let Some(r) = self.registros.iter_mut().find(|r|
                r.kind == m.kind && r.nivel == m.nivel && r.chefe == m.chefe) {
                r.nome = m.nome;
                r.vida = m.vida;
                r.desafio = m.desafio;
            } else {
                self.registros.push(Registro {
                    kind: m.kind, nivel: m.nivel, nome: m.nome,
                    vida: m.vida, chefe: m.chefe, desafio: m.desafio,
                });
            }
        }
        self.registros.sort_by_key(|r| (r.nivel, r.chefe, r.kind));
        self.selecionado = chave.and_then(|chave| self.registros.iter()
            .position(|r| (r.kind, r.nivel, r.chefe) == chave));
    }

    pub fn observar(&mut self, mundo: &World) {
        for e in mundo.ents.values() {
            if e.meta.tag != shared::EntityTag::Enemy { continue; }
            let Some(desafio) = e.meta.desafio else { continue; };
            let chefe = e.state.flags & shared::ent_flags::BOSS != 0;
            let nome = e.meta.name.clone().unwrap_or_else(|| format!("Mob {}", e.meta.kind));
            if let Some(r) = self.registros.iter_mut().find(|r|
                r.kind == e.meta.kind && r.nivel == e.meta.nivel && r.chefe == chefe) {
                r.vida = e.meta.hp_max;
                r.desafio = desafio;
                r.nome = nome;
            } else {
                let chave = self.selecionado.and_then(|i| self.registros.get(i))
                    .map(|r| (r.kind, r.nivel, r.chefe));
                self.registros.push(Registro {
                    kind: e.meta.kind, nivel: e.meta.nivel, nome,
                    vida: e.meta.hp_max, chefe, desafio,
                });
                self.registros.sort_by_key(|r| (r.nivel, r.chefe, r.kind));
                self.selecionado = chave.and_then(|chave| self.registros.iter()
                    .position(|r| (r.kind, r.nivel, r.chefe) == chave));
            }
        }
    }

    pub fn desenha(
        &mut self, stats: Option<&shared::PlayerStats>,
        equip: &shared::Equipment, proficiencia: Option<u32>,
        vox: &crate::vox::VoxCache, solido: &Material,
    ) -> Option<PedidoIr> {
        if !self.aberto { return None; }
        let seguro = crate::hud_layout::tela_segura();
        let p = Rect::new(seguro.x + 12.0, seguro.y + 12.0,
            seguro.w - 24.0, seguro.h - 24.0);
        crate::hud_layout::escurece(0.6);
        estilo::painel(p);
        let m = Vec2::from(mouse_position());
        let clique = crate::foco::clique();
        estilo::texto_forte(p.x + 18.0, p.y + 31.0, "MOBS", 22, estilo::OURO);
        let x = Rect::new(p.x + p.w - 47.0, p.y + 8.0, 36.0, 34.0);
        estilo::botao(x, "X", estilo::estado_de(x, false, false), false);
        if clique && x.contains(m) { self.fechar(); return None; }
        estilo::texto(p.x + 18.0, p.y + 52.0,
            "Inimigos das áreas visitadas · atributos reais do servidor", 12, estilo::SUAVE);
        let lista = Rect::new(p.x + 12.0, p.y + 64.0, p.w * 0.42, p.h - 76.0);
        let detalhe = Rect::new(lista.x + lista.w + 10.0, lista.y,
            p.x + p.w - 12.0 - (lista.x + lista.w + 10.0), lista.h);
        estilo::cartao(lista, false, false);
        estilo::cartao(detalhe, false, false);
        if self.registros.is_empty() {
            estilo::texto(lista.x + 12.0, lista.y + 32.0,
                "Visit an island to see its mobs.", 14, estilo::SUAVE);
            return None;
        }
        let por_pagina = ((lista.h - 58.0) / 42.0).floor().max(1.0) as usize;
        let max_pagina = (self.registros.len() - 1) / por_pagina;
        self.pagina = self.pagina.min(max_pagina);
        for (linha, i) in (self.pagina * por_pagina..self.registros.len())
            .take(por_pagina).enumerate() {
            let r = &self.registros[i];
            let area = Rect::new(lista.x + 7.0, lista.y + 8.0 + linha as f32 * 42.0,
                lista.w - 14.0, 37.0);
            estilo::cartao(area, area.contains(m), self.selecionado == Some(i));
            estilo::texto_ajustado(&format!("{}  Nv. {}", r.nome, r.nivel),
                area.x + 8.0, area.y + 24.0, area.w - 16.0, 14,
                if r.chefe { estilo::OURO } else { estilo::TEXTO });
            if clique && area.contains(m) { self.selecionado = Some(i); }
        }
        let rodape_y = lista.y + lista.h - 22.0;
        let anterior = Rect::new(lista.x + 8.0, rodape_y - 8.0, 35.0, 27.0);
        let proximo = Rect::new(lista.x + lista.w - 43.0, rodape_y - 8.0, 35.0, 27.0);
        estilo::botao(anterior, "<", estilo::estado_de(anterior, self.pagina == 0, false), false);
        estilo::botao(proximo, ">", estilo::estado_de(proximo, self.pagina >= max_pagina, false), false);
        estilo::texto_centro(lista.center().x, rodape_y + 11.0,
            &format!("{} / {}", self.pagina + 1, max_pagina + 1), 12, estilo::SUAVE);
        if clique && anterior.contains(m) { self.pagina = self.pagina.saturating_sub(1); }
        if clique && proximo.contains(m) { self.pagina = (self.pagina + 1).min(max_pagina); }
        let Some(r) = self.selecionado.and_then(|i| self.registros.get(i)) else {
            estilo::texto(detalhe.x + 14.0, detalhe.y + 32.0,
                "Tap a mob to see the details.", 14, estilo::SUAVE);
            return None;
        };
        let d = r.desafio;
        estilo::texto_ajustado(&format!("{}  ·  Nv. {}", r.nome, r.nivel),
            detalhe.x + 14.0, detalhe.y + 31.0, detalhe.w - 28.0, 19, estilo::OURO);
        let palco = Rect::new(detalhe.x + detalhe.w * 0.50, detalhe.y + 38.0,
            detalhe.w * 0.46, (detalhe.h * 0.27).clamp(75.0, 150.0));
        crate::render3d::vitrine_mob(vox, r.kind, r.chefe, palco,
            get_time() as f32 * 0.5, solido);
        let ir = Rect::new(detalhe.x + 14.0, detalhe.y + 45.0,
            (detalhe.w * 0.36).max(65.0), 34.0);
        estilo::botao(ir, "Ir", estilo::estado_de(ir, false, false), false);
        let pedido = (clique && ir.contains(m)).then(|| PedidoIr {
            kind: r.kind, nivel: r.nivel, chefe: r.chefe, nome: r.nome.clone(),
        });
        let pecas = pecas_adequadas(equip, &d);
        let refino = equip.weapon_inst.map_or(0, |i| i.refinement);
        let cor_de = |ok: Option<bool>| match ok {
            Some(true) => estilo::VERDE,
            Some(false) => estilo::OURO,
            None => estilo::SUAVE,
        };
        let linhas = [
            ("Health", r.vida.to_string(), estilo::TEXTO),
            ("Mob attack", d.ataque.to_string(), estilo::TEXTO),
            ("Mob defence", d.defesa.to_string(), estilo::TEXTO),
            ("Suggested power", format!("{} · yours {}", d.poder_recomendado,
                stats.map_or_else(|| "—".into(), |s| shared::dungeon::poder_de_stats(s).to_string())),
                cor_de(stats.map(|s| shared::dungeon::poder_de_stats(s) >= d.poder_recomendado as i32))),
            ("Suggested attack", format!("{} · yours {}", d.ataque_recomendado,
                stats.map_or_else(|| "—".into(), |s| s.attack_damage.to_string())),
                cor_de(stats.map(|s| s.attack_damage >= d.ataque_recomendado as i32))),
            ("Suggested defence", format!("{} · yours {}", d.defesa_recomendada,
                stats.map_or_else(|| "—".into(), |s| s.defense.to_string())),
                cor_de(stats.map(|s| s.defense >= d.defesa_recomendada as i32))),
            ("Proficiency", format!("{} · yours {}", d.proficiencia_minima,
                proficiencia.map_or_else(|| "—".into(), |p| p.to_string())),
                cor_de(proficiencia.map(|p| p >= d.proficiencia_minima as u32))),
            ("Set pieces", format!("{} {} Lv{} · yours {}", d.pecas_minimas,
                nome_do_grau(d.grau_minimo), d.item_level_minimo, pecas),
                cor_de(Some(pecas >= d.pecas_minimas as usize))),
            ("Weapon refine", format!("+{} · yours +{}", d.refino_arma_minimo, refino),
                cor_de(Some(refino >= d.refino_arma_minimo))),
        ];
        let topo = palco.y + palco.h + 10.0;
        let passo = ((detalhe.y + detalhe.h - topo - 12.0) / linhas.len() as f32).clamp(16.0, 30.0);
        for (i, (rotulo, valor, cor)) in linhas.iter().enumerate() {
            let y = topo + i as f32 * passo;
            estilo::texto(detalhe.x + 14.0, y, rotulo, 12, estilo::SUAVE);
            estilo::texto_ajustado(valor, detalhe.x + detalhe.w * 0.47, y,
                detalhe.w * 0.49, 13, *cor);
        }
        pedido
    }
}

fn nome_do_grau(grau: u8) -> &'static str {
    match grau { 2 => "verdes", 3 => "azuis", 4 => "roxas", _ => "cinzas" }
}

fn pecas_adequadas(equip: &shared::Equipment, d: &shared::DesafioMob) -> usize {
    use shared::EquipSlot::*;
    [Weapon, Offhand, Armor, Earring, Necklace, Bracelet, Belt]
        .into_iter()
        .filter(|&slot| equip.get_inst(slot).is_some_and(|i|
            i.rarity >= d.grau_minimo && i.item_level >= d.item_level_minimo))
        .count()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn catalogo_mostra_os_atributos_do_servidor() {
        let mut ui = MobsUi::default();
        let stats = shared::PlayerStats {
            hp_max: 500, attack_damage: 40, defense: 12,
            ..shared::base_player_stats()
        };
        let desafio = shared::desafio_do_mob(&stats, 30, false);
        ui.catalogo(vec![shared::protocol::MobNoCatalogo {
            kind: 1, nivel: 30, nome: "Bear".into(), vida: 500,
            chefe: false, desafio,
        }]);
        assert_eq!(ui.registros.len(), 1);
        assert_eq!(ui.registros[0].desafio, desafio);
        ui.catalogo(vec![shared::protocol::MobNoCatalogo {
            kind: 1, nivel: 30, nome: "Bear".into(), vida: 600,
            chefe: false, desafio,
        }]);
        assert_eq!(ui.registros.len(), 1);
        assert_eq!(ui.registros[0].vida, 600);
    }
}

