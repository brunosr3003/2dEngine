//! Seleciona apenas monstros perto do ponto onde AUTO foi ligado.
//! Movimento, ataques e dano continuam validados pelo servidor.
use macroquad::prelude::*;
use shared::{EntityId, EntityTag};
use std::collections::HashMap;
use crate::{world::World, hud_estilo as estilo};

const RAIO: f32 = 24.0;

/// Parado por este tempo depois de andar na mao, o AUTO volta a escolher alvo.
const VOLTA_PARADO_S: f64 = 0.4;

#[derive(Default)]
pub struct AutoCombate {
    pub centro: Option<Vec2>,
    observado: Option<(EntityId,f32,u16,f64)>,
    ignorados: HashMap<EntityId,f64>,
    /// Andando por conta propria (teclado ou clique no chao). Andar NAO desliga
    /// o AUTO: a area acompanha o personagem e a escolha de alvo espera ele
    /// parar — senao a aproximacao do alvo atropela a rota do clique.
    manual: bool,
    ultima_pos: Option<Vec2>,
    parado_desde: f64,
}

/// O botao AUTO COMBATE na linha de baixo do cluster (ver `hud_layout`).
pub fn retangulo() -> Rect { crate::hud_layout::atual().auto_combate }
pub fn pega_mouse() -> bool { retangulo().contains(Vec2::from(mouse_position())) }

impl AutoCombate {
    pub fn ativo(&self) -> bool { self.centro.is_some() }
    pub fn ligar(&mut self, p: Vec2) { self.centro=Some(p); self.observado=None; self.ignorados.clear(); }
    pub fn parar(&mut self) { *self=Self::default(); }

    /// O jogador esta' andando na mao neste quadro.
    pub fn andar_manual(&mut self, agora: f64) {
        if self.ativo() { self.manual=true; self.parado_desde=agora; }
    }

    /// Ainda andando na mao? Enquanto sim, a area vem junto e nao se escolhe
    /// alvo. Parou, a area fica onde ele parou e o AUTO retoma dali.
    pub fn segurando(&mut self, pos: Vec2, agora: f64) -> bool {
        let moveu=self.ultima_pos.is_some_and(|u|u.distance(pos)>0.01);
        self.ultima_pos=Some(pos);
        if !self.manual { return false; }
        if moveu { self.parado_desde=agora; }
        self.centro=Some(pos);
        if agora-self.parado_desde>VOLTA_PARADO_S { self.manual=false; self.observado=None; return false; }
        true
    }

    pub fn escolher(&mut self, world: &World, atual: Option<EntityId>, agora: f64) -> Option<EntityId> {
        let centro=self.centro?;
        let eu=world.self_id.and_then(|id|world.ents.get(&id))?;
        if eu.state.hp==0 || eu.state.flags & shared::ent_flags::DOWNED != 0 || eu.render_pos.distance(centro)>RAIO+4.0 {
            self.parar(); return None;
        }
        self.ignorados.retain(|_,ate|*ate>agora);
        let valido=|id: EntityId|world.ents.get(&id).is_some_and(|e|
            e.meta.tag==EntityTag::Enemy && e.state.hp>0 && e.morte.is_none()
            && e.render_pos.distance(centro)<=RAIO && e.render_pos.distance(eu.render_pos)<=RAIO);
        if let Some(id)=atual.filter(|&id|valido(id) && !self.ignorados.contains_key(&id)) {
            let e=&world.ents[&id]; let dist=eu.render_pos.distance(e.render_pos);
            match self.observado {
                Some((ant,d,hp,t)) if ant==id => {
                    if dist<d-0.4 || e.state.hp<hp { self.observado=Some((id,dist,e.state.hp,agora)); }
                    else if agora-t>8.0 { self.ignorados.insert(id,agora+15.0); self.observado=None; }
                    else { return Some(id); }
                }
                _ => self.observado=Some((id,dist,e.state.hp,agora)),
            }
            if !self.ignorados.contains_key(&id) { return Some(id); }
        }
        let escolhido=world.ents.keys().copied().filter(|&id|valido(id) && !self.ignorados.contains_key(&id))
            .min_by(|a,b|world.ents[a].render_pos.distance_squared(eu.render_pos).total_cmp(&world.ents[b].render_pos.distance_squared(eu.render_pos)).then(a.0.cmp(&b.0)));
        self.observado=escolhido.map(|id|(id,eu.render_pos.distance(world.ents[&id].render_pos),world.ents[&id].state.hp,agora));
        escolhido
    }

    /// O botao. A tecla (Z) so' aparece com Alt; o estado vai pra faixa unica.
    pub fn desenha(&self) {
        let r=retangulo(); let c=r.center(); let raio=r.w*0.49;
        let cor=if self.ativo() {estilo::AUTO} else {estilo::OURO};
        let e=estilo::estado_de(r,false,self.ativo());
        estilo::botao_redondo(c,raio,cor,e,self.ativo());
        if self.ativo() { estilo::arco(c,raio+4.0,get_time() as f32*0.8,0.20,2.0,cor); }
        if !crate::icones_ui::ui("auto_combate",c-vec2(0.0,raio*0.16),raio*1.0,cor) {
            estilo::icone(2,c-vec2(0.0,raio*0.16),raio*0.46,cor);
        }
        estilo::texto_centro_forte(c.x,c.y+raio*0.62,if self.ativo() {"AUTO"} else {"COMBATE"},10,cor);
        crate::hud_layout::chip(r,"Z");
    }

    /// O texto da faixa de estado, com o AUTO ligado.
    pub fn faixa(&self, tem_alvo: bool) -> Option<&'static str> {
        self.ativo().then_some(if tem_alvo {"AUTO COMBATE · ATACANDO"} else {"AUTO COMBATE · BUSCANDO"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mundo() -> World {
        let mut w=World::default();
        let mut metas=vec![];let mut estados=vec![];
        for (id,tag,x,hp) in [(1,EntityTag::Player,0.0,100),(2,EntityTag::Player,1.0,100),
            (3,EntityTag::Enemy,3.0,100),(4,EntityTag::Enemy,8.0,100),(5,EntityTag::Enemy,30.0,100),(6,EntityTag::Enemy,2.0,0)] {
            metas.push(shared::EntityMeta{id:EntityId(id),tag,name:None,hp_max:100,faction:None,kind:0,nivel:1});
            estados.push(shared::EntityState::quantize(EntityId(id),::glam::Vec2::new(x,0.0),::glam::Vec2::ZERO,hp,if id==1 {shared::ent_flags::SELF} else {0}));
        }
        w.apply(metas,estados,&[]); w
    }
    #[test]
    fn escolhe_monstro_vivo_proximo_e_troca_apos_morte() {
        let mut w=mundo();let mut a=AutoCombate::default();a.ligar(Vec2::ZERO);
        assert_eq!(a.escolher(&w,None,0.0),Some(EntityId(3)));
        w.ents.get_mut(&EntityId(3)).unwrap().state.hp=0;
        assert_eq!(a.escolher(&w,Some(EntityId(3)),1.0),Some(EntityId(4)));
        w.ents.get_mut(&EntityId(4)).unwrap().state.hp=0;
        assert_eq!(a.escolher(&w,Some(EntityId(4)),2.0),None);
        assert!(a.ativo()); // Aguarda respawn, sem escolher player ou sair da area.
    }
    #[test]
    fn abandona_alvo_inacessivel_e_para_quando_personagem_cai() {
        let mut w=mundo();let mut a=AutoCombate::default();a.ligar(Vec2::ZERO);
        a.escolher(&w,None,0.0);
        assert_eq!(a.escolher(&w,Some(EntityId(3)),9.0),Some(EntityId(4)));
        w.ents.get_mut(&EntityId(1)).unwrap().state.flags|=shared::ent_flags::DOWNED;
        assert_eq!(a.escolher(&w,Some(EntityId(4)),10.0),None);assert!(!a.ativo());
    }
    #[test]
    fn andar_nao_desliga_e_a_area_vem_junto() {
        let mut a=AutoCombate::default();a.ligar(Vec2::ZERO);
        a.segurando(Vec2::ZERO,0.0);
        // Anda 40 unidades — bem alem do raio de onde ligou.
        for k in 1..=40 { a.andar_manual(k as f64*0.1); assert!(a.segurando(vec2(k as f32,0.0),k as f64*0.1)); }
        assert!(a.ativo());
        assert_eq!(a.centro,Some(vec2(40.0,0.0)));
        // Solta a tecla: ainda segura um instante, depois volta a caçar dali.
        assert!(a.segurando(vec2(40.0,0.0),4.2));
        assert!(!a.segurando(vec2(40.0,0.0),4.5));
        assert!(a.ativo());
    }
}
