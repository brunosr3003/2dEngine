//! Missoes: a janela do Mestre de Missoes (o que da' pra aceitar e o que esta'
//! em andamento), o diario (J), o rastreador no canto e o "!"/"?" sobre o
//! Mestre. Aceitar, progresso e entrega sao do SERVIDOR: aqui so' se mostra e
//! se pede.
use macroquad::prelude::*;
use shared::protocol::ClientMessage;
use shared::historia;
use shared::quests::{objective_kind, quest_status, QuestNet, GIVER_MESTRE_DA_ILHA};
use shared::EntityId;
use std::collections::HashMap;

use crate::hud_estilo as estilo;
use crate::render3d::{world_to_screen, Vista};
use crate::world::World;

/// Longe disto do Mestre a janela dele fecha: o servidor ja' recusaria.
const FECHA_LONGE: f32 = shared::INTERACT_RADIUS + 1.5;
/// Status que o servidor manda quando a missao foi abandonada.
const ABANDONADA: u8 = 255;
const LARGURA: f32 = 430.0;
const LINHA_OFERTA: f32 = 112.0;
const LINHA_ATIVA: f32 = 70.0;
const TITULO_SECAO: f32 = 26.0;
/// O que foi clicado no rastreador.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoRastreador {
    /// Uma missao: liga a auto missao.
    Missao(u16),
    /// O titulo: abre o diario.
    Diario,
    /// O rodape: abre todas as missoes.
    Todas,
}

/// Quanto do item-alvo o jogador carrega (coleta e' conferida na entrega).
pub type Tem<'a> = &'a dyn Fn(u16) -> u32;

/// Quantos de `id` a bolsa tem — so' os sem instancia, que e' o que o
/// servidor aceita na entrega.
pub fn na_bolsa(slots: &[shared::InventorySlot], id: u16) -> u32 {
    slots.iter().filter(|s| s.item_id == id && s.instance.is_none()).map(|s| s.qty).sum()
}

#[derive(Default)]
pub struct Missoes {
    /// Ativas e prontas, como o servidor mandou.
    pub log: Vec<QuestNet>,
    /// O que o NPC da janela oferece agora.
    oferta: Vec<QuestNet>,
    /// Cabecalho: nome do NPC, ou "Diario de missoes".
    quem: String,
    /// NPC que abriu a janela. `None` = diario (J): nao fecha por distancia e
    /// nao entrega (entregar e' com o Mestre).
    pub npc: Option<EntityId>,
    pub aberta: bool,
    /// Givers com missao aceitavel agora (`QuestGivers`).
    givers: Vec<u16>,
    /// "Ir" apertado no diario neste quadro: o `main` liga a auto missao.
    pub ir: Option<u16>,
}

/// (feito, total) pra mostrar. Coleta conta o que esta' na bolsa.
pub fn progresso(q: &QuestNet, tem: Tem) -> (u32, u32) {
    if coleta(q) {
        (tem(q.obj_target).min(q.obj_count), q.obj_count)
    } else {
        (q.progress.min(q.obj_count), q.obj_count)
    }
}

fn coleta(q: &QuestNet) -> bool {
    q.obj_kind == objective_kind::COLLECT || q.obj_kind == objective_kind::DELIVER
}

/// Da' pra entregar? Live-track: o servidor marcou READY. Coleta: tem na bolsa.
pub fn pronta(q: &QuestNet, tem: Tem) -> bool {
    q.status == quest_status::READY || (coleta(q) && tem(q.obj_target) >= q.obj_count)
}

fn verbo(q: &QuestNet) -> &'static str {
    match q.obj_kind {
        objective_kind::TALK => "Conversar",
        objective_kind::KILL | objective_kind::PVP_KILL => "Derrotar",
        objective_kind::COLLECT | objective_kind::DELIVER => "Juntar",
        objective_kind::EXPLORE | objective_kind::TRANSPORT => "Chegar",
        objective_kind::GATHER => "Coletar",
        objective_kind::CRAFT => "Criar",
        objective_kind::REFINE => "Refinar",
        objective_kind::LUGAR => "Ir até",
        objective_kind::NIVEL => "Nível",
        objective_kind::VIAGEM => "Viajar",
        _ => "Objetivo",
    }
}

/// O log na ordem do rastreador: o passo da HISTORIA sempre no topo.
///
/// As DIARIAS nao entram: tem painel proprio (icone no topo e Menu), e no
/// canto elas empurravam a historia e a cadeia pra fora da lista.
pub fn ordem_do_rastreador(log: &[QuestNet]) -> Vec<&QuestNet> {
    let mut v: Vec<&QuestNet> = log.iter().filter(|q| !q.daily).collect();
    v.sort_by_key(|q| !historia::e_da_historia(q.id));
    v
}

/// A linha de estado de um passo da historia no rastreador.
fn estado_da_historia(q: &QuestNet, nivel: u32) -> String {
    if q.status == quest_status::READY {
        return "Concluindo…".into();
    }
    match q.obj_kind {
        objective_kind::NIVEL => format!("Alcance o nível {} · você: {nivel}", q.obj_count),
        objective_kind::LUGAR => format!("Ir até {}", historia::ponto::nome(q.obj_target)),
        objective_kind::VIAGEM => "Fale com o Capitão do Porto".into(),
        objective_kind::TALK => {
            let nome = shared::quests::PAPEIS_DE_CONVERSA.iter().find(|p| **p as u16 == q.obj_target).map_or("?", |p| p.nome());
            format!("Conversar com {nome}")
        }
        _ => format!("{}  {}/{}", verbo(q), q.progress.min(q.obj_count), q.obj_count),
    }
}

/// Barra da trava de nivel: quanto do caminho ate' o nivel pedido, contando a
/// fracao de XP do nivel atual.
pub fn fracao_da_trava(alvo: u32, nivel: u32, fracao_xp: f32) -> f32 {
    if nivel >= alvo {
        return 1.0;
    }
    ((nivel.saturating_sub(1) as f32 + fracao_xp.clamp(0.0, 1.0)) / (alvo.saturating_sub(1).max(1)) as f32).clamp(0.0, 1.0)
}

pub fn recompensa(q: &QuestNet, nomes: &HashMap<u16, String>) -> String {
    let mut partes = Vec::new();
    if q.reward_gold > 0 {
        partes.push(format!("{} ouro", q.reward_gold));
    }
    if q.reward_xp > 0 {
        partes.push(format!("{} XP", q.reward_xp));
    }
    if q.reward_item != 0 && q.reward_item_qty > 0 {
        let nome = nomes.get(&q.reward_item).cloned().unwrap_or_else(|| format!("item {}", q.reward_item));
        partes.push(format!("{}x {nome}", q.reward_item_qty));
    }
    if q.reward_item2 != 0 && q.reward_item2_qty > 0 {
        let nome = nomes.get(&q.reward_item2).cloned().unwrap_or_else(|| format!("item {}", q.reward_item2));
        partes.push(format!("{}x {nome}", q.reward_item2_qty));
    }
    partes.join("  ·  ")
}

/// Quebra `s` em linhas que cabem em `largura`, no maximo `max` linhas.
pub(crate) fn quebra(s: &str, largura: f32, tam: u16, max: usize) -> Vec<String> {
    let mut linhas: Vec<String> = Vec::new();
    let mut atual = String::new();
    for palavra in s.split_whitespace() {
        let tenta = if atual.is_empty() { palavra.to_string() } else { format!("{atual} {palavra}") };
        if estilo::medir(&tenta, tam) > largura && !atual.is_empty() {
            linhas.push(std::mem::take(&mut atual));
            if linhas.len() == max {
                return linhas;
            }
            atual = palavra.to_string();
        } else {
            atual = tenta;
        }
    }
    if !atual.is_empty() && linhas.len() < max {
        linhas.push(atual);
    }
    linhas
}

impl Missoes {
    /// `QuestOffer`: abre a janela do NPC com o que ele oferece.
    pub fn abre_oferta(&mut self, npc: Option<EntityId>, nome: String, quests: Vec<QuestNet>) {
        // Repetivel em cooldown vem como TURNED_IN: nao ha' o que aceitar.
        self.oferta = quests.into_iter().filter(|q| q.status != quest_status::TURNED_IN).collect();
        self.quem = nome;
        self.npc = npc;
        self.aberta = true;
    }

    /// Guarda a oferta do NPC SEM abrir a janela: quem mostra e' o dialogo.
    /// Precisa ficar guardada pra o `QuestUpdate` de aceite achar a missao.
    pub fn guarda_oferta(&mut self, npc: Option<EntityId>, nome: String, quests: Vec<QuestNet>) {
        self.oferta = quests.into_iter().filter(|q| q.status != quest_status::TURNED_IN).collect();
        self.quem = nome;
        self.npc = npc;
    }

    /// Abre a janela com a oferta ja' guardada.
    pub fn abre_janela(&mut self) {
        self.aberta = true;
    }

    pub fn oferta(&self) -> &[QuestNet] {
        &self.oferta
    }

    /// `QuestLog`: o estado inteiro das ativas.
    pub fn define_log(&mut self, quests: Vec<QuestNet>) {
        self.log = quests;
    }

    /// `QuestUpdate`. Aceitou: sai da oferta e entra no andamento. Entregou ou
    /// abandonou: some.
    pub fn atualiza(&mut self, id: u16, progress: u32, status: u8) {
        if status == ABANDONADA || status == quest_status::TURNED_IN {
            self.log.retain(|q| q.id != id);
            self.oferta.retain(|q| q.id != id);
            return;
        }
        if let Some(q) = self.log.iter_mut().find(|q| q.id == id) {
            q.progress = progress;
            q.status = status;
            return;
        }
        if let Some(i) = self.oferta.iter().position(|q| q.id == id) {
            let mut q = self.oferta.remove(i);
            q.progress = progress;
            q.status = status;
            self.log.push(q);
            return;
        }
        // Missao que ninguem ofereceu — o passo da historia que o servidor da'
        // sozinho: a definicao sai do shared, o mesmo indice dos dois lados.
        if let Some(d) = shared::quests::quest_by_id(id) {
            self.log.push(QuestNet::from_def(d, status, progress));
        }
    }

    /// `QuestGivers`.
    pub fn define_givers(&mut self, givers: Vec<u16>) {
        self.givers = givers;
    }

    /// J: abre o diario (so' o andamento) ou fecha o que estiver aberto.
    pub fn alterna_diario(&mut self) {
        if self.aberta {
            self.fecha();
        } else {
            self.oferta.clear();
            self.npc = None;
            self.quem = "Diário de missões".into();
            self.aberta = true;
        }
    }

    pub fn fecha(&mut self) {
        self.aberta = false;
        self.npc = None;
        self.oferta.clear();
    }

    /// Tudo zerado: troca de zona, reconexao, sair.
    pub fn limpa(&mut self) {
        *self = Self::default();
    }

    /// A janela de um NPC fecha longe dele (ou se ele sumiu). O diario, nao.
    pub fn conferir_distancia(&mut self, eu: Option<Vec2>, npc: Option<Vec2>) {
        if !self.aberta || self.npc.is_none() {
            return;
        }
        match (eu, npc) {
            (Some(a), Some(b)) if a.distance(b) <= FECHA_LONGE => {}
            _ => self.fecha(),
        }
    }

    /// Sobre o Mestre: "?" tem entrega pronta, "!" tem missao nova.
    pub fn marcador(&self, tem: Tem) -> Option<&'static str> {
        if self.log.iter().any(|q| q.giver == GIVER_MESTRE_DA_ILHA && pronta(q, tem)) {
            Some("?")
        } else if self.givers.contains(&GIVER_MESTRE_DA_ILHA) {
            Some("!")
        } else {
            None
        }
    }

    fn altura(&self) -> f32 {
        let mut h = 58.0;
        if !self.oferta.is_empty() {
            h += TITULO_SECAO + self.oferta.len() as f32 * LINHA_OFERTA;
        }
        h += TITULO_SECAO + self.log.len().max(1) as f32 * LINHA_ATIVA;
        h + 10.0
    }

    fn painel(&self) -> Rect {
        let t = crate::hud_layout::tela_segura();
        let h = self.altura().min(t.y + t.h - screen_height() * 0.16 - 16.0);
        Rect::new(t.x + 24.0, screen_height() * 0.16, LARGURA, h)
    }

    /// Com o mouse em cima da janela (ou do rastreador) o clique e' dela, e nao
    /// do mundo. Com a janela aberta o rastreador fica escondido (ela cobre o
    /// mesmo canto).
    pub fn pega_mouse(&self) -> bool {
        let m = Vec2::from(mouse_position());
        if self.aberta {
            self.painel().contains(m)
        } else {
            self.rastreador_rect().contains(m)
        }
    }

    /// O rastreador na esquerda, abaixo da ficha (docs/HUD.md, B). Cresce com
    /// as missoes ate' o que o layout reserva; sem missao fica so' o cabecalho
    /// e o link "Todas as missões".
    pub fn rastreador_rect(&self) -> Rect {
        let z = crate::hud_layout::atual();
        let n = ordem_do_rastreador(&self.log).len().min(z.missoes_no_rastreador).max(1);
        Rect::new(z.rastreador.x, z.rastreador.y, z.rastreador.w, (40.0 + 52.0 * n as f32 + 28.0) * z.s)
    }

    /// Desenha a janela e devolve os pedidos do quadro.
    pub fn desenha(&mut self, nomes: &HashMap<u16, String>, tem: Tem) -> Vec<ClientMessage> {
        if !self.aberta {
            return Vec::new();
        }
        let p = self.painel();
        let mut saida = Vec::new();
        estilo::painel(p);
        estilo::texto_ajustado(&self.quem, p.x + 16.0, p.y + 30.0, p.w - 70.0, 22, estilo::OURO);
        if crate::ui::botao(Rect::new(p.x + p.w - 44.0, p.y + 8.0, 32.0, 28.0), "x", true) {
            self.fecha();
            return saida;
        }
        draw_line(p.x + 12.0, p.y + 44.0, p.x + p.w - 12.0, p.y + 44.0, 1.0, estilo::BORDA);
        let texto_w = p.w - 32.0;
        let fim = p.y + p.h;
        let mut y = p.y + 52.0;

        if !self.oferta.is_empty() {
            estilo::texto(p.x + 16.0, y + 18.0, "Disponíveis", 15, estilo::SUAVE);
            y += TITULO_SECAO;
            for q in &self.oferta {
                if y + LINHA_OFERTA > fim {
                    break;
                }
                estilo::texto_ajustado(&q.title, p.x + 16.0, y + 20.0, texto_w - 100.0, 18, estilo::TEXTO);
                for (k, linha) in quebra(&q.desc, texto_w, 14, 3).iter().enumerate() {
                    estilo::texto(p.x + 16.0, y + 42.0 + k as f32 * 18.0, linha, 14, estilo::SUAVE);
                }
                estilo::texto_ajustado(&recompensa(q, nomes), p.x + 16.0, y + 100.0, texto_w, 14, estilo::OURO);
                if crate::ui::botao(Rect::new(p.x + p.w - 108.0, y + 4.0, 92.0, 26.0), "Aceitar", true) {
                    saida.push(ClientMessage::AcceptQuest { quest_id: q.id });
                }
                y += LINHA_OFERTA;
            }
        }

        estilo::texto(p.x + 16.0, y + 18.0, "Em andamento", 15, estilo::SUAVE);
        y += TITULO_SECAO;
        if self.log.is_empty() {
            let dica = if self.npc.is_some() { "Nenhuma missão ativa." } else { "Nenhuma missão ativa. Fale com o Mestre de Missões na praça." };
            estilo::texto_ajustado(dica, p.x + 16.0, y + 22.0, texto_w, 15, estilo::SUAVE);
        }
        let com_o_mestre = self.npc.is_some();
        let mut ir = None;
        for q in &self.log {
            if y + LINHA_ATIVA > fim {
                break;
            }
            let (feito, total) = progresso(q, tem);
            let ok = pronta(q, tem);
            estilo::texto_ajustado(&q.title, p.x + 16.0, y + 20.0, texto_w - 200.0, 17, estilo::TEXTO);
            let estado = if ok {
                "Pronta — entregue ao Mestre de Missões".to_string()
            } else {
                format!("{}  {feito}/{total}", verbo(q))
            };
            let cor = if ok { estilo::AUTO } else { estilo::SUAVE };
            estilo::texto_ajustado(&estado, p.x + 16.0, y + 42.0, texto_w - 110.0, 14, cor);
            // Barrinha de progresso.
            let barra = Rect::new(p.x + 16.0, y + 52.0, texto_w - 110.0, 4.0);
            draw_rectangle(barra.x, barra.y, barra.w, barra.h, Color::new(1.0, 1.0, 1.0, 0.08));
            draw_rectangle(barra.x, barra.y, barra.w * feito as f32 / total.max(1) as f32, barra.h, estilo::OURO);
            let mut bx = p.x + p.w - 16.0;
            if ok && com_o_mestre && q.giver == GIVER_MESTRE_DA_ILHA {
                bx -= 92.0;
                if crate::ui::botao(Rect::new(bx, y + 4.0, 92.0, 26.0), "Entregar", true) {
                    saida.push(ClientMessage::TurnInQuest { quest_id: q.id });
                }
                bx -= 8.0;
            }
            if !com_o_mestre && crate::ui::botao(Rect::new(bx - 92.0, y + 4.0, 92.0, 26.0), "Ir", true) {
                ir = Some(q.id);
            }
            // A historia nao se abandona.
            if !historia::e_da_historia(q.id) && crate::ui::botao(Rect::new(bx - 92.0, y + 34.0, 92.0, 24.0), "Abandonar", true) {
                saida.push(ClientMessage::AbandonQuest { quest_id: q.id });
            }
            y += LINHA_ATIVA;
        }
        if ir.is_some() {
            self.ir = ir;
            self.fecha();
        }
        saida
    }

    /// O rastreador na esquerda. Clicar numa missao liga a auto missao; o
    /// titulo "Missões ›" abre o diario; o rodape abre todas as missoes. `auto`
    /// e' a que esta' em auto agora (marcada).
    pub fn desenha_rastreador(&self, tem: Tem, auto: Option<u16>, nivel: u32, fracao_xp: f32) -> Option<NoRastreador> {
        let z = crate::hud_layout::atual();
        let s = z.s;
        let r = self.rastreador_rect();
        let n = ordem_do_rastreador(&self.log).len().min(z.missoes_no_rastreador);
        let mouse = Vec2::from(mouse_position());
        let clique = is_mouse_button_pressed(MouseButton::Left);
        let mut saida = None;
        estilo::painel(r);
        // Abas: Missões (ativa) e Grupo (em breve).
        let titulo = Rect::new(r.x, r.y, r.w * 0.5, 36.0 * s);
        let sobre = titulo.contains(mouse);
        draw_rectangle(r.x + 10.0, r.y + 30.0 * s, estilo::medir("Missões ›", 16), 2.0, estilo::OURO);
        estilo::texto(r.x + 10.0, r.y + 24.0 * s, "Missões ›", 16, if sobre { estilo::OURO } else { estilo::TEXTO });
        estilo::texto(r.x + r.w * 0.5, r.y + 24.0 * s, "Grupo", 15, estilo::SUAVE);
        if sobre && clique {
            saida = Some(NoRastreador::Diario);
        }
        draw_line(r.x + 8.0, r.y + 36.0 * s, r.x + r.w - 8.0, r.y + 36.0 * s, 1.0, estilo::BORDA);
        if n == 0 {
            estilo::texto_ajustado("Nenhuma missão em andamento", r.x + 12.0, r.y + 66.0 * s, r.w - 24.0, 14, estilo::SUAVE);
        }
        for (i, q) in ordem_do_rastreador(&self.log).into_iter().take(n).enumerate() {
            let y = r.y + 40.0 * s + i as f32 * 52.0 * s;
            let linha = Rect::new(r.x + 2.0, y, r.w - 4.0, 52.0 * s);
            let principal = historia::e_da_historia(q.id);
            if principal {
                // A historia: fundo dourado e losango, sempre a primeira.
                draw_rectangle(linha.x, linha.y, linha.w, linha.h, Color::new(1.0, 0.78, 0.25, 0.10));
                let c = vec2(r.x + 14.0, y + 15.0 * s);
                draw_poly(c.x, c.y, 4, 6.0 * s, 0.0, estilo::OURO);
                draw_poly(c.x, c.y, 4, 3.0 * s, 0.0, Color::new(1.0, 0.95, 0.7, 1.0));
            }
            if linha.contains(mouse) {
                draw_rectangle(linha.x, linha.y, linha.w, linha.h, Color::new(1.0, 1.0, 1.0, 0.06));
                if clique {
                    saida = Some(NoRastreador::Missao(q.id));
                }
            }
            if auto == Some(q.id) {
                draw_rectangle(linha.x, linha.y + 4.0, 3.0, linha.h - 8.0, estilo::AUTO);
                estilo::texto(r.x + r.w - 58.0, y + 20.0 * s, "› AUTO", 12, estilo::AUTO);
            }
            let (tx, cor_titulo) = if principal { (r.x + 26.0 * s, estilo::OURO) } else { (r.x + 12.0, estilo::TEXTO) };
            estilo::texto_ajustado(&q.title, tx, y + 20.0 * s, r.w - 80.0, 15, cor_titulo);
            if principal {
                let txt = estado_da_historia(q, nivel);
                let cor = if q.status == quest_status::READY { estilo::AUTO } else { estilo::SUAVE };
                estilo::texto_ajustado(&txt, r.x + 12.0, y + 38.0 * s, r.w - 24.0, 13, cor);
                if q.obj_kind == objective_kind::NIVEL {
                    let b = Rect::new(r.x + 12.0, y + 44.0 * s, r.w - 24.0, 4.0 * s);
                    draw_rectangle(b.x, b.y, b.w, b.h, Color::new(1.0, 1.0, 1.0, 0.10));
                    draw_rectangle(b.x, b.y, b.w * fracao_da_trava(q.obj_count, nivel, fracao_xp), b.h, estilo::OURO);
                }
                continue;
            }
            let (feito, total) = progresso(q, tem);
            let (txt, cor) = if pronta(q, tem) {
                ("Pronta: volte ao Mestre".to_string(), estilo::AUTO)
            } else {
                (format!("{}  {feito}/{total}", verbo(q)), estilo::SUAVE)
            };
            estilo::texto_ajustado(&txt, r.x + 12.0, y + 40.0 * s, r.w - 24.0, 13, cor);
        }
        let link = Rect::new(r.x, r.y + r.h - 28.0 * s, r.w, 28.0 * s);
        let sobre = link.contains(mouse);
        estilo::texto_centro(link.center().x, link.y + link.h * 0.5 + 5.0, "Todas as missões", 13, if sobre { estilo::OURO } else { estilo::SUAVE });
        if sobre && clique {
            saida = Some(NoRastreador::Todas);
        }
        saida
    }

    /// "!" ou "?" flutuando sobre o Mestre de Missoes, projetado da camera.
    pub fn desenha_marcador(&self, world: &World, vista: &Vista, tem: Tem) {
        let Some(sinal) = self.marcador(tem) else { return };
        let nome = shared::construcao::Papel::Missoes.nome();
        let Some(e) = world.ents.values().find(|e| e.meta.tag == shared::EntityTag::Npc && e.meta.name.as_deref() == Some(nome)) else {
            return;
        };
        let balanco = (get_time() as f32 * 3.0).sin() * 0.08;
        let topo = vista.pos_de(e) + vec3(0.0, 2.55 + balanco, 0.0);
        let Some(c) = world_to_screen(&vista.cam, topo) else { return };
        let cor = if sinal == "?" { estilo::AUTO } else { Color::new(1.0, 0.84, 0.2, 1.0) };
        for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0)] {
            estilo::texto_centro(c.x + dx, c.y + dy, sinal, 34, Color::new(0.0, 0.0, 0.0, 0.85));
        }
        estilo::texto_centro(c.x, c.y, sinal, 34, cor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::quests::quest_by_id;

    fn q(id: u16, status: u8, progress: u32) -> QuestNet {
        QuestNet::from_def(quest_by_id(id).unwrap(), status, progress)
    }
    fn nada(_: u16) -> u32 {
        0
    }

    #[test]
    fn aceitar_tira_da_oferta_e_poe_no_andamento() {
        let mut m = Missoes::default();
        m.abre_oferta(Some(EntityId(9)), "Mestre".into(), vec![q(501, quest_status::ACTIVE, 0)]);
        assert!(m.aberta);
        m.atualiza(501, 0, quest_status::ACTIVE);
        assert_eq!(m.log.len(), 1);
        assert!(m.oferta.is_empty());
        m.atualiza(501, 1, quest_status::READY);
        assert_eq!(m.log[0].status, quest_status::READY);
        m.atualiza(501, 1, quest_status::TURNED_IN);
        assert!(m.log.is_empty(), "entregue some do andamento");
    }

    /// O passo da historia que o servidor da' entra no log sem oferta e fica
    /// no topo do rastreador; a trava mostra quanto falta.
    #[test]
    fn historia_entra_sozinha_e_fica_no_topo() {
        let mut m = Missoes::default();
        m.define_log(vec![q(502, quest_status::ACTIVE, 1)]);
        m.atualiza(historia::PRIMEIRO_ID, 0, quest_status::ACTIVE);
        assert_eq!(m.log.len(), 2);
        let ordem: Vec<u16> = ordem_do_rastreador(&m.log).iter().map(|q| q.id).collect();
        assert_eq!(ordem, vec![historia::PRIMEIRO_ID, 502]);
        m.atualiza(historia::PRIMEIRO_ID, 1, quest_status::TURNED_IN);
        m.atualiza(historia::PRIMEIRO_ID + 1, 0, quest_status::ACTIVE);
        assert_eq!(ordem_do_rastreador(&m.log)[0].id, historia::PRIMEIRO_ID + 1);
        assert_eq!(fracao_da_trava(5, 5, 0.0), 1.0);
        assert!((fracao_da_trava(5, 3, 0.5) - 0.625).abs() < 1e-4);
        assert_eq!(fracao_da_trava(5, 1, 0.0), 0.0);
    }

    #[test]
    fn abandonar_some() {
        let mut m = Missoes::default();
        m.define_log(vec![q(502, quest_status::ACTIVE, 2)]);
        m.atualiza(502, 0, ABANDONADA);
        assert!(m.log.is_empty());
    }

    #[test]
    fn oferta_ignora_as_em_cooldown() {
        let mut m = Missoes::default();
        m.abre_oferta(None, "Mestre".into(), vec![q(501, quest_status::TURNED_IN, 1), q(502, quest_status::ACTIVE, 0)]);
        assert_eq!(m.oferta.len(), 1);
        assert_eq!(m.oferta[0].id, 502);
    }

    #[test]
    fn marcador_do_mestre() {
        let mut m = Missoes::default();
        assert_eq!(m.marcador(&nada), None);
        m.define_givers(vec![GIVER_MESTRE_DA_ILHA]);
        assert_eq!(m.marcador(&nada), Some("!"));
        m.define_log(vec![q(502, quest_status::READY, 6)]);
        assert_eq!(m.marcador(&nada), Some("?"), "entrega pronta vence missao nova");
        // Coleta: pronta quando a bolsa tem o bastante.
        m.define_log(vec![q(503, quest_status::ACTIVE, 0)]);
        let cobre = |id: u16| if id == shared::constants::item_id::COPPER { 30 } else { 0 };
        assert_eq!(m.marcador(&cobre), Some("?"));
        assert_eq!(progresso(&m.log[0], &cobre), (30, 30));
    }

    #[test]
    fn o_rastreador_nao_mostra_diarias() {
        use shared::quests::quest_by_id;
        let log = vec![
            QuestNet::from_def(quest_by_id(601).unwrap(), quest_status::ACTIVE, 3),
            QuestNet::from_def(quest_by_id(502).unwrap(), quest_status::ACTIVE, 1),
            QuestNet::from_def(historia::def_da_historia(historia::PRIMEIRO_ID).unwrap(), quest_status::ACTIVE, 0),
        ];
        let ids: Vec<u16> = ordem_do_rastreador(&log).iter().map(|q| q.id).collect();
        assert_eq!(ids, vec![historia::PRIMEIRO_ID, 502], "diaria fora, historia no topo");
    }

    #[test]
    fn janela_do_npc_fecha_longe_e_o_diario_nao() {
        let mut m = Missoes::default();
        m.abre_oferta(Some(EntityId(9)), "Mestre".into(), Vec::new());
        m.conferir_distancia(Some(vec2(0.0, 0.0)), Some(vec2(4.0, 0.0)));
        assert!(m.aberta);
        m.conferir_distancia(Some(vec2(0.0, 0.0)), Some(vec2(5.0, 0.0)));
        assert!(!m.aberta);
        m.alterna_diario();
        m.conferir_distancia(Some(vec2(0.0, 0.0)), None);
        assert!(m.aberta, "diario nao depende de NPC");
        m.alterna_diario();
        assert!(!m.aberta);
    }
}
