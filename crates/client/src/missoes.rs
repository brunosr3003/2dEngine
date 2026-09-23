//! Missoes: a janela do Mestre de Missoes (o que da' pra aceitar e o que esta'
//! em andamento), o diario (J), o rastreador no canto e o "!"/"?" sobre o
//! Mestre. Aceitar, progresso e entrega sao do SERVIDOR: aqui so' se mostra e
//! se pede.
use std::collections::HashSet;

use macroquad::prelude::*;
use shared::historia;
use shared::protocol::ClientMessage;
use shared::quests::{objective_kind, quest_status, QuestNet, GIVER_MESTRE_DA_ILHA};
use shared::EntityId;
use std::collections::HashMap;

use crate::hud_estilo::{self as estilo, u};
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
    /// O alfinete: fixa (ou solta) esta missao no topo do rastreador.
    Fixar(u16),
    /// As setas: anda a janela de tres. `true` = pra frente.
    Rolar(bool),
}

/// O CONTADOR de missoes concluidas, no centro superior da tela.
///
/// Aparece quando sobe e desaparece sozinho. Terminar uma missao so' deixava
/// rastro no chat — que rola, some e compete com conversa: o jogador fechava
/// cinco seguidas e nao via nenhuma. Aqui ele ve' o numero subir, fixada ou
/// nao, que foi o que o dono pediu.
///
/// Nao e' painel: e' um numero que pisca e sai. Desenhado por cima de tudo,
/// sem pegar toque — nada aqui e' clicavel, e roubar um toque no meio da luta
/// pra mostrar um placar seria pior que nao mostrar.
pub fn desenha_contador(m: &Missoes, agora: f64) {
    const DURACAO: f64 = 3.2;
    if m.concluidas == 0 {
        return;
    }
    let t = agora - m.concluida_em;
    if t > DURACAO {
        return;
    }
    // Entra depressa, fica, e some no ultimo terco.
    let a = ((DURACAO - t) / (DURACAO * 0.33)).clamp(0.0, 1.0) as f32;
    let sobe = (1.0 - (t / 0.25).clamp(0.0, 1.0) as f32) * 14.0;
    let s = crate::hud_layout::tela_segura();
    let x = s.x + s.w * 0.5;
    let y = s.y + 54.0 - sobe;
    let texto = format!("Missões concluídas: {}", m.concluidas);
    estilo::texto_centro(x + 1.0, y + 1.0, &texto, 18, Color::new(0.0, 0.0, 0.0, 0.55 * a));
    estilo::texto_centro(
        x,
        y,
        &texto,
        18,
        Color::new(estilo::OURO.r, estilo::OURO.g, estilo::OURO.b, a),
    );
}

/// Quanto do item-alvo o jogador carrega (coleta e' conferida na entrega).
pub type Tem<'a> = &'a dyn Fn(u16) -> u32;

/// Quantos de `id` a bolsa tem — so' os sem instancia, que e' o que o
/// servidor aceita na entrega.
pub fn na_bolsa(slots: &[shared::InventorySlot], id: u16) -> u32 {
    slots
        .iter()
        .filter(|s| s.item_id == id && s.instance.is_none())
        .map(|s| s.qty)
        .sum()
}

#[derive(Default)]
pub struct Missoes {
    /// Ativas e prontas, como o servidor mandou.
    pub log: Vec<QuestNet>,
    /// Lupa num objetivo de juntar item: o item pro "Onde obter".
    pub onde_obter: Option<u16>,
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
    /// As missoes FIXADAS: ficam no topo do rastreador, logo abaixo da
    /// historia. Vivem so' na sessao — fixar e' uma decisao do momento.
    pub fixadas: HashSet<u16>,
    /// Primeira missao mostrada no rastreador. So' tres cabem por vez
    /// (`NO_RASTREADOR`); as setas andam com isto.
    pub desloca_rastreador: usize,
    /// Quantas missoes foram CONCLUIDAS nesta sessao, e quando a ultima.
    ///
    /// O dono pediu o contador no centro superior "subindo ao fazer qualquer
    /// missao, mesmo nao fixada": a recompensa de terminar uma missao estava
    /// so' no chat, que rola e some.
    pub concluidas: u32,
    pub concluida_em: f64,
    /// O giver do NPC que abriu a janela (o Mestre ou um oficio da vila):
    /// so' as missoes DELE se entregam aqui.
    giver: Option<u16>,
    /// Ofertas + ativas passam da tela (o Mestre oferece varias de uma vez).
    rolagem: crate::rolagem::Rolagem,
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

/// Quantas missões cabem no rastreador de uma vez. As outras vêm rolando.
///
/// Três, e não as quatro ou cinco que a tela comportava: o dono pediu
/// "mostrar 3 por vez, mas daí com scroll para ver as que não couberem". O
/// rastreador é canto de olho, não leitura — quanto mais linha, menos ele é
/// lido.
pub const NO_RASTREADOR: usize = 3;

/// Altura de uma linha. Era 52; o dono: "tem que ser menor, mais resumido,
/// só tipo urso 1/10". Uma linha só, então cabe em 30.
const LINHA: f32 = 30.0;

/// O RESUMO de uma missão, do jeito que ele cabe de canto de olho.
///
/// "Urso 1/10", e não "Derrotar  1/10" sobre o título em outra linha. O que o
/// jogador precisa saber num relance é O QUE e QUANTO FALTA; o título inteiro
/// ele lê no menu, onde há espaço.
pub fn resumo(q: &QuestNet, tem: Tem) -> String {
    if pronta(q, tem) {
        return format!("{} · pronta", curto(&q.title));
    }
    let (feito, total) = progresso(q, tem);
    if total <= 1 {
        return curto(&q.title);
    }
    format!("{} {feito}/{total}", curto(&q.title))
}

/// O título encurtado: a primeira parte antes de pontuação, com teto.
///
/// Títulos de missão são frases ("A caçada do Bosque Sombrio"); no rastreador
/// elas viram uma tira de texto ilegível.
fn curto(titulo: &str) -> String {
    let base = titulo
        .split(['·', ':', '—', ','])
        .next()
        .unwrap_or(titulo)
        .trim();
    if base.chars().count() <= 18 {
        return base.to_string();
    }
    let mut s: String = base.chars().take(17).collect();
    s.push('…');
    s
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
        objective_kind::DUNGEON => "Vencer",
        objective_kind::TUTORIAL => "Aprender",
        _ => "Objetivo",
    }
}

/// O log na ordem do rastreador: o passo da HISTORIA sempre no topo.
///
/// As DIARIAS nao entram: tem painel proprio (icone no topo e Menu), e no
/// canto elas empurravam a historia e a cadeia pra fora da lista.
pub fn ordem_do_rastreador(log: &[QuestNet]) -> Vec<&QuestNet> {
    ordem_com_fixadas(log, &HashSet::new())
}

/// A mesma ordem, com as FIXADAS na frente.
///
/// O rastreador mostra as primeiras N e o resto some. Com a historia sempre no
/// topo e o log cheio, a missao que o jogador esta' de fato fazendo caia pra
/// fora da lista — e ele nao tinha como dizer "esta aqui eu quero ver". O dono
/// pediu: "escolher quais missoes ficam mostrando no menu de missoes na
/// esquerda o tempo todo. Fixadas."
///
/// A HISTORIA continua na frente de tudo: ela e' o fio, e perde-la de vista e'
/// o jeito mais rapido de travar sem saber por que.
pub fn ordem_com_fixadas<'a>(log: &'a [QuestNet], fixadas: &HashSet<u16>) -> Vec<&'a QuestNet> {
    let mut v: Vec<&QuestNet> = log.iter().filter(|q| !q.daily).collect();
    v.sort_by_key(|q| {
        (
            !historia::e_da_historia(q.id),
            !fixadas.contains(&q.id),
            q.id,
        )
    });
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
        // O Capitao leva; o botao e' MINHA ILHA, e nao Embarcar.
        // Tutorial de GESTO nao tem contagem (obj_count 1): "1/1" nao diz
        // nada. O da Energia pede uma quantia, e ai' o quanto falta e' a
        // informacao principal.
        objective_kind::TUTORIAL => {
            let o = shared::quests::tutorial::instrucao(q.obj_target);
            if q.obj_count > 1 {
                format!("{o}  {}/{}", q.progress.min(q.obj_count), q.obj_count)
            } else {
                o.into()
            }
        }
        objective_kind::DUNGEON => match shared::dungeon::conteudo(q.obj_target) {
            Some(c) => format!("Vencer {} · toque para abrir", c.nome),
            None => "Vencer uma dungeon · toque para abrir".into(),
        },
        objective_kind::TALK => {
            let nome = shared::quests::PAPEIS_DE_CONVERSA
                .iter()
                .find(|p| **p as u16 == q.obj_target)
                .map_or("?", |p| p.nome());
            format!("Conversar com {nome}")
        }
        _ => format!(
            "{}  {}/{}",
            verbo(q),
            q.progress.min(q.obj_count),
            q.obj_count
        ),
    }
}

/// Barra da trava de nivel: quanto do caminho ate' o nivel pedido, contando a
/// fracao de XP do nivel atual.
pub fn fracao_da_trava(alvo: u32, nivel: u32, fracao_xp: f32) -> f32 {
    if nivel >= alvo {
        return 1.0;
    }
    ((nivel.saturating_sub(1) as f32 + fracao_xp.clamp(0.0, 1.0))
        / (alvo.saturating_sub(1).max(1)) as f32)
        .clamp(0.0, 1.0)
}

pub fn recompensa(q: &QuestNet, nomes: &HashMap<u16, String>) -> String {
    let mut partes = Vec::new();
    if q.reward_cobre > 0 {
        partes.push(format!("{} cobre", q.reward_cobre));
    }
    if q.reward_xp > 0 {
        partes.push(format!("{} XP", q.reward_xp));
    }
    if q.reward_item != 0 && q.reward_item_qty > 0 {
        let nome = nomes
            .get(&q.reward_item)
            .cloned()
            .unwrap_or_else(|| format!("item {}", q.reward_item));
        partes.push(format!("{}x {nome}", q.reward_item_qty));
    }
    if q.reward_item2 != 0 && q.reward_item2_qty > 0 {
        let nome = nomes
            .get(&q.reward_item2)
            .cloned()
            .unwrap_or_else(|| format!("item {}", q.reward_item2));
        partes.push(format!("{}x {nome}", q.reward_item2_qty));
    }
    partes.join("  ·  ")
}

/// Quebra `s` em linhas que cabem em `largura`, no maximo `max` linhas.
pub(crate) fn quebra(s: &str, largura: f32, tam: u16, max: usize) -> Vec<String> {
    let mut linhas: Vec<String> = Vec::new();
    let mut atual = String::new();
    for palavra in s.split_whitespace() {
        let tenta = if atual.is_empty() {
            palavra.to_string()
        } else {
            format!("{atual} {palavra}")
        };
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
        self.giver = None;
        // Repetivel em cooldown vem como TURNED_IN: nao ha' o que aceitar.
        self.oferta = quests
            .into_iter()
            .filter(|q| q.status != quest_status::TURNED_IN)
            .collect();
        self.quem = nome;
        self.npc = npc;
        self.aberta = true;
        self.rolagem.zera();
    }

    /// Guarda a oferta do NPC SEM abrir a janela: quem mostra e' o dialogo.
    /// Precisa ficar guardada pra o `QuestUpdate` de aceite achar a missao.
    pub fn guarda_oferta(
        &mut self,
        npc: Option<EntityId>,
        giver: u16,
        nome: String,
        quests: Vec<QuestNet>,
    ) {
        self.giver = Some(giver);
        self.oferta = quests
            .into_iter()
            .filter(|q| q.status != quest_status::TURNED_IN)
            .collect();
        self.quem = nome;
        self.npc = npc;
    }

    /// Abre a janela com a oferta ja' guardada.
    pub fn abre_janela(&mut self) {
        self.aberta = true;
        self.rolagem.zera();
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
        self.marcador_de(GIVER_MESTRE_DA_ILHA, tem)
    }

    /// O mesmo pra qualquer NPC que da' missao (`giver`).
    pub fn marcador_de(&self, giver: u16, tem: Tem) -> Option<&'static str> {
        if self.log.iter().any(|q| q.giver == giver && pronta(q, tem)) {
            Some("?")
        } else if self.givers.contains(&giver) {
            Some("!")
        } else {
            None
        }
    }

    /// Altura do conteudo, na escala atual (`u`).
    fn altura(&self) -> f32 {
        let mut h = u(58.0);
        if !self.oferta.is_empty() {
            h += u(TITULO_SECAO) + self.oferta.len() as f32 * u(LINHA_OFERTA);
        }
        h += u(TITULO_SECAO) + self.log.len().max(1) as f32 * u(LINHA_ATIVA);
        h + u(10.0)
    }

    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA * 2.0, 620.0)
    }

    fn painel(&self) -> Rect {
        estilo::no_painel(Self::escala(), || {
            let t = crate::hud_layout::tela_segura();
            let h = self.altura().min(t.y + t.h - screen_height() * 0.16 - 16.0);
            Rect::new(t.x + u(24.0), screen_height() * 0.16, u(LARGURA), h)
        })
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
    /// (sem rodape: "Todas as missões" ja' esta' no Menu).
    pub fn rastreador_rect(&self) -> Rect {
        let z = crate::hud_layout::atual();
        let n = ordem_do_rastreador(&self.log).len().min(NO_RASTREADOR).max(1);
        // MAIS ESTREITO (o dono: "tá muito grande lateralmente"), TRÊS linhas
        // e SEM RODAPÉ: o link "Todas as missões" saiu porque a mesma coisa
        // já está no Menu, e um atalho repetido só ocupa a tela.
        Rect::new(
            z.rastreador.x,
            z.rastreador.y,
            z.rastreador.w * 0.62,
            (34.0 + LINHA * n as f32 + 6.0) * z.s,
        )
    }

    /// Desenha a janela e devolve os pedidos do quadro.
    pub fn desenha(&mut self, nomes: &HashMap<u16, String>, tem: Tem) -> Vec<ClientMessage> {
        if !self.aberta {
            return Vec::new();
        }
        estilo::no_painel(Self::escala(), || self.desenha_na_escala(nomes, tem))
    }

    fn desenha_na_escala(&mut self, nomes: &HashMap<u16, String>, tem: Tem) -> Vec<ClientMessage> {
        let p = self.painel();
        let mut saida = Vec::new();
        estilo::painel(p);
        estilo::texto_ajustado(
            &self.quem,
            p.x + u(16.0),
            p.y + u(30.0),
            p.w - u(70.0),
            22,
            estilo::OURO,
        );
        if crate::ui::botao(
            Rect::new(p.x + p.w - u(44.0), p.y + u(8.0), u(32.0), u(28.0)),
            "x",
            true,
        ) {
            self.fecha();
            return saida;
        }
        draw_line(
            p.x + u(12.0),
            p.y + u(44.0),
            p.x + p.w - u(12.0),
            p.y + u(44.0),
            1.0,
            estilo::BORDA,
        );
        let texto_w = p.w - u(44.0);
        // Ofertas e ativas rolam (dedo, roda ou barra) em vez de sumir quando
        // nao cabem; botao dentro da lista vale no SOLTAR.
        let area = Rect::new(p.x + u(2.0), p.y + u(48.0), p.w - u(4.0), p.h - u(52.0));
        let total = self.altura() - u(58.0) + u(6.0);
        let clique = self.rolagem.quadro(area, total, u(LINHA_ATIVA));
        let tocou = |r: Rect| clique.is_some_and(|c| r.contains(c) && area.contains(c));
        let fora = |y: f32, h: f32| y + h < area.y || y > area.y + area.h;
        crate::rolagem::recortar(Some(area));
        let mut y = area.y + u(4.0) - self.rolagem.pos;

        if !self.oferta.is_empty() {
            estilo::texto(p.x + u(16.0), y + u(18.0), "Disponíveis", 15, estilo::SUAVE);
            y += u(TITULO_SECAO);
            for q in &self.oferta {
                if fora(y, u(LINHA_OFERTA)) {
                    y += u(LINHA_OFERTA);
                    continue;
                }
                estilo::texto_ajustado(
                    &q.title,
                    p.x + u(16.0),
                    y + u(20.0),
                    texto_w - u(100.0),
                    18,
                    estilo::TEXTO,
                );
                for (k, linha) in quebra(&q.desc, texto_w, 14, 3).iter().enumerate() {
                    estilo::texto(
                        p.x + u(16.0),
                        y + u(42.0) + k as f32 * u(18.0),
                        linha,
                        14,
                        estilo::SUAVE,
                    );
                }
                estilo::texto_ajustado(
                    &recompensa(q, nomes),
                    p.x + u(16.0),
                    y + u(100.0),
                    texto_w,
                    14,
                    estilo::OURO,
                );
                let b = Rect::new(p.x + p.w - u(120.0), y + u(4.0), u(92.0), u(26.0));
                let _ = crate::ui::botao(b, "Aceitar", true);
                if tocou(b) {
                    saida.push(ClientMessage::AcceptQuest { quest_id: q.id });
                }
                y += u(LINHA_OFERTA);
            }
        }

        estilo::texto(p.x + u(16.0), y + u(18.0), "Em andamento", 15, estilo::SUAVE);
        y += u(TITULO_SECAO);
        if self.log.is_empty() {
            let dica = if self.npc.is_some() {
                "Nenhuma missão ativa."
            } else {
                "Nenhuma missão ativa. Fale com o Mestre de Missões na praça."
            };
            estilo::texto_ajustado(dica, p.x + u(16.0), y + u(22.0), texto_w, 15, estilo::SUAVE);
        }
        let com_o_mestre = self.npc.is_some();
        let mut ir = None;
        for q in &self.log {
            if fora(y, u(LINHA_ATIVA)) {
                y += u(LINHA_ATIVA);
                continue;
            }
            let (feito, total) = progresso(q, tem);
            let ok = pronta(q, tem);
            estilo::texto_ajustado(
                &q.title,
                p.x + u(16.0),
                y + u(20.0),
                texto_w - u(200.0),
                17,
                estilo::TEXTO,
            );
            let estado = if ok {
                let quem = shared::quests::papel_do_giver(q.giver)
                    .map_or("quem deu a missão", |p| p.nome());
                format!("Pronta — entregue: {quem}")
            } else {
                format!("{}  {feito}/{total}", verbo(q))
            };
            let cor = if ok { estilo::AUTO } else { estilo::SUAVE };
            estilo::texto_ajustado(&estado, p.x + u(16.0), y + u(42.0), texto_w - u(110.0), 14, cor);
            // Barrinha de progresso.
            let barra = Rect::new(p.x + u(16.0), y + u(52.0), texto_w - u(110.0), u(4.0));
            draw_rectangle(
                barra.x,
                barra.y,
                barra.w,
                barra.h,
                Color::new(1.0, 1.0, 1.0, 0.08),
            );
            draw_rectangle(
                barra.x,
                barra.y,
                barra.w * feito as f32 / total.max(1) as f32,
                barra.h,
                estilo::OURO,
            );
            let mut bx = p.x + p.w - u(28.0);
            if ok && com_o_mestre && Some(q.giver) == self.giver {
                bx -= u(92.0);
                let b = Rect::new(bx, y + u(4.0), u(92.0), u(26.0));
                let _ = crate::ui::botao(b, "Entregar", true);
                if tocou(b) {
                    saida.push(ClientMessage::TurnInQuest { quest_id: q.id });
                }
                bx -= u(8.0);
            }
            if !com_o_mestre {
                let b = Rect::new(bx - u(92.0), y + u(4.0), u(92.0), u(26.0));
                let _ = crate::ui::botao(b, "Ir", true);
                if tocou(b) {
                    ir = Some(q.id);
                }
            }
            if coleta(q) && q.obj_target != 0 && !ok {
                let lx = if com_o_mestre {
                    bx - u(40.0)
                } else {
                    bx - u(92.0) - u(40.0)
                };
                let lupa = Rect::new(lx, y + u(2.0), u(34.0), u(30.0));
                let _ = crate::onde_obter::botao(lupa);
                if tocou(lupa) {
                    self.onde_obter = Some(q.obj_target);
                }
            }
            // A historia nao se abandona.
            if !historia::e_da_historia(q.id) {
                let b = Rect::new(bx - u(92.0), y + u(34.0), u(92.0), u(24.0));
                let _ = crate::ui::botao(b, "Abandonar", true);
                if tocou(b) {
                    saida.push(ClientMessage::AbandonQuest { quest_id: q.id });
                }
            }
            y += u(LINHA_ATIVA);
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if ir.is_some() {
            self.ir = ir;
            self.fecha();
        }
        saida
    }

    /// O rastreador na esquerda. Clicar numa missao liga a auto missao; o
    /// titulo "Missões ›" abre o diario. `auto`
    /// e' a que esta' em auto agora (marcada).
    pub fn desenha_rastreador(
        &self,
        tem: Tem,
        auto: Option<u16>,
        nivel: u32,
        fracao_xp: f32,
    ) -> Option<NoRastreador> {
        let z = crate::hud_layout::atual();
        let s = z.s;
        let r = self.rastreador_rect();
        let todas = ordem_com_fixadas(&self.log, &self.fixadas);
        let total_missoes = todas.len();
        let n = total_missoes.min(NO_RASTREADOR);
        // A ROLAGEM é por deslocamento, não por barra: três linhas num canto
        // de tela não comportam trilho nem polegar, e o gesto que sobra é a
        // setinha. `desloca` já vem cortado pra nunca passar do fim.
        let desloca = self
            .desloca_rastreador
            .min(todas.len().saturating_sub(NO_RASTREADOR));
        let mouse = Vec2::from(mouse_position());
        let clique = crate::foco::clique();
        let mut saida = None;
        estilo::painel(r);
        // Abas: Missões (ativa) e Grupo (em breve).
        let titulo = Rect::new(r.x, r.y, r.w * 0.5, 36.0 * s);
        let sobre = titulo.contains(mouse);
        draw_rectangle(
            r.x + 10.0,
            r.y + 30.0 * s,
            estilo::medir("Missões ›", 16),
            2.0,
            estilo::OURO,
        );
        estilo::texto(
            r.x + 10.0,
            r.y + 24.0 * s,
            "Missões ›",
            16,
            if sobre { estilo::OURO } else { estilo::TEXTO },
        );
        estilo::texto(r.x + r.w * 0.5, r.y + 24.0 * s, "Grupo", 15, estilo::SUAVE);
        if sobre && clique {
            saida = Some(NoRastreador::Diario);
        }
        draw_line(
            r.x + 8.0,
            r.y + 36.0 * s,
            r.x + r.w - 8.0,
            r.y + 36.0 * s,
            1.0,
            estilo::BORDA,
        );
        if n == 0 {
            estilo::texto_ajustado(
                "Nenhuma missão em andamento",
                r.x + 12.0,
                r.y + 66.0 * s,
                r.w - 24.0,
                14,
                estilo::SUAVE,
            );
        }
        for (i, q) in todas.into_iter().skip(desloca).take(n).enumerate() {
            let y = r.y + 34.0 * s + i as f32 * LINHA * s;
            let linha = Rect::new(r.x + 2.0, y, r.w - 4.0, LINHA * s);
            let principal = historia::e_da_historia(q.id);
            if principal {
                // A história: uma tira dourada à esquerda. O losango e o
                // fundo inteiro comiam a linha que agora é uma só.
                draw_rectangle(linha.x, linha.y + 4.0 * s, 3.0 * s, linha.h - 8.0 * s, estilo::OURO);
            }
            // O ALFINETE: fixa a missão no topo. Só pra quem não é história —
            // ela já está sempre em primeiro, e um alfinete que não muda nada
            // é um botão que mente.
            let fixada = self.fixadas.contains(&q.id);
            let alfinete = Rect::new(linha.x + linha.w - 22.0 * s, y + 4.0 * s, 20.0 * s, 20.0 * s);
            if !principal {
                estilo::texto_centro(
                    alfinete.center().x,
                    alfinete.center().y + 5.0 * s,
                    "*",
                    if fixada { 16 } else { 14 },
                    if fixada { estilo::OURO } else { estilo::SUAVE },
                );
            }
            if linha.contains(mouse) {
                draw_rectangle(
                    linha.x,
                    linha.y,
                    linha.w,
                    linha.h,
                    Color::new(1.0, 1.0, 1.0, 0.05),
                );
            }
            // UMA LINHA SÓ: "Urso 1/10". O título inteiro se lê no menu, que
            // é onde há espaço; aqui é canto de olho.
            let cor = if pronta(q, tem) {
                estilo::AUTO
            } else if principal {
                estilo::OURO
            } else if auto == Some(q.id) {
                estilo::TEXTO
            } else {
                estilo::SUAVE
            };
            estilo::texto_ajustado(
                &resumo(q, tem),
                linha.x + if principal { 12.0 * s } else { 8.0 },
                y + 20.0 * s,
                linha.w - 34.0 * s,
                14,
                cor,
            );
            // A trava de nível ganha a barrinha, que é o único jeito de ver
            // que ela anda.
            if principal && q.obj_kind == objective_kind::NIVEL {
                let b = Rect::new(linha.x + 8.0, y + LINHA * s - 5.0 * s, linha.w - 40.0 * s, 3.0 * s);
                draw_rectangle(b.x, b.y, b.w, b.h, Color::new(1.0, 1.0, 1.0, 0.10));
                draw_rectangle(
                    b.x,
                    b.y,
                    b.w * fracao_da_trava(q.obj_count, nivel, fracao_xp),
                    b.h,
                    estilo::OURO,
                );
            }
            if clique && linha.contains(mouse) {
                saida = Some(if !principal && alfinete.contains(mouse) {
                    NoRastreador::Fixar(q.id)
                } else {
                    NoRastreador::Missao(q.id)
                });
            }
        }
        // AS SETAS, só quando há mais do que cabe. Elas ficam no topo, à
        // direita do título, que é o único canto livre numa caixa de três
        // linhas.
        if total_missoes > NO_RASTREADOR {
            let sobe = Rect::new(r.x + r.w - 44.0 * s, r.y + 4.0 * s, 20.0 * s, 22.0 * s);
            let desce = Rect::new(r.x + r.w - 22.0 * s, r.y + 4.0 * s, 20.0 * s, 22.0 * s);
            for (caixa, glifo, pode) in [
                (sobe, "‹", desloca > 0),
                (desce, "›", desloca + NO_RASTREADOR < total_missoes),
            ] {
                estilo::texto_centro(
                    caixa.center().x,
                    caixa.center().y + 6.0 * s,
                    glifo,
                    16,
                    if pode { estilo::TEXTO } else { estilo::SUAVE },
                );
                if pode && clique && caixa.contains(mouse) {
                    saida = Some(NoRastreador::Rolar(caixa == desce));
                }
            }
        }
        saida
    }

    /// "!" ou "?" flutuando sobre cada NPC da vila que tem missao pra dar ou
    /// receber (o Mestre e os oficios das cadeias), projetado da camera.
    pub fn desenha_marcador(&self, world: &World, vista: &Vista, tem: Tem) {
        let mestre = shared::construcao::Papel::Missoes.nome();
        for e in world.ents.values() {
            if e.meta.tag != shared::EntityTag::Npc {
                continue;
            }
            let giver = if e.meta.name.as_deref() == Some(mestre) {
                Some(GIVER_MESTRE_DA_ILHA)
            } else {
                shared::quests::giver_do_npc(shared::npc_papel_de_kind(e.meta.kind) as u16)
            };
            let Some(sinal) = giver.and_then(|g| self.marcador_de(g, tem)) else {
                continue;
            };
            let balanco = (get_time() as f32 * 3.0).sin() * 0.08;
            let topo = vista.pos_de(e) + vec3(0.0, 2.55 + balanco, 0.0);
            let Some(c) = world_to_screen(&vista.cam, topo) else {
                continue;
            };
            let cor = if sinal == "?" {
                estilo::AUTO
            } else {
                Color::new(1.0, 0.84, 0.2, 1.0)
            };
            for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0)] {
                estilo::texto_centro(c.x + dx, c.y + dy, sinal, 34, Color::new(0.0, 0.0, 0.0, 0.85));
            }
            estilo::texto_centro(c.x, c.y, sinal, 34, cor);
        }
    }
}

#[cfg(test)]
mod tests {
    /// O RESUMO é curto de verdade, e diz o que falta.
    ///
    /// O dono: "tem que ser menor, mais resumido, só tipo urso 1/10". Um
    /// título de missão é uma frase ("A caçada do Bosque Sombrio") e no
    /// rastreador vira uma tira ilegível.
    #[test]
    fn o_resumo_do_rastreador_e_curto() {
        assert_eq!(curto("Urso"), "Urso");
        assert_eq!(curto("A caçada · Bosque Sombrio"), "A caçada");
        assert_eq!(curto("Caçar ursos: a temporada"), "Caçar ursos");
        // Comprido sem pontuação: corta e avisa que cortou.
        let longo = curto("Uma missão com um nome absurdamente comprido");
        assert!(longo.chars().count() <= 18, "{longo:?} tem {} chars", longo.chars().count());
        assert!(longo.ends_with('…'), "{longo:?} não avisa que foi cortado");
        // E TODO título do jogo cabe: se algum não couber, ele é cortado, mas
        // o teste existe pra ninguém achar que o rastreador mostra o nome
        // inteiro.
        for q in shared::quests::QUESTS.iter().take(60) {
            let c = curto(q.title);
            assert!(!c.is_empty(), "{}: resumo vazio", q.title);
            assert!(c.chars().count() <= 18, "{}: resumo com {} chars", q.title, c.chars().count());
        }
    }

    /// A CAIXA do rastreador cabe no canto, e as linhas cabem nela.
    ///
    /// Ela era larga demais ("tá muito grande lateralmente") e tinha rodapé
    /// com um atalho que o Menu já dá. Agora são três linhas de 30 e nada
    /// mais — e este teste é o que impede a caixa e as linhas de se
    /// separarem de novo.
    #[test]
    fn a_caixa_do_rastreador_cabe_nas_tres_linhas() {
        for s in [0.8f32, 1.0, 1.5, 2.2] {
            for n in 1..=NO_RASTREADOR {
                let altura = (34.0 + LINHA * n as f32 + 6.0) * s;
                let ultima = 34.0 * s + (n as f32 - 1.0) * LINHA * s + LINHA * s;
                assert!(
                    ultima <= altura + 0.01,
                    "s={s} n={n}: a última linha acaba em {ultima:.0} e a caixa \
                     tem {altura:.0}"
                );
            }
        }
        assert_eq!(NO_RASTREADOR, 3, "o dono pediu três por vez");
    }

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
        m.abre_oferta(
            Some(EntityId(9)),
            "Mestre".into(),
            vec![q(501, quest_status::ACTIVE, 0)],
        );
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
        m.abre_oferta(
            None,
            "Mestre".into(),
            vec![
                q(501, quest_status::TURNED_IN, 1),
                q(502, quest_status::ACTIVE, 0),
            ],
        );
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
        assert_eq!(
            m.marcador(&nada),
            Some("?"),
            "entrega pronta vence missao nova"
        );
        // Coleta: pronta quando a bolsa tem o bastante.
        m.define_log(vec![q(503, quest_status::ACTIVE, 0)]);
        let cobre = |id: u16| {
            if id == shared::constants::item_id::COPPER {
                30
            } else {
                0
            }
        };
        assert_eq!(m.marcador(&cobre), Some("?"));
        assert_eq!(progresso(&m.log[0], &cobre), (30, 30));
    }

    #[test]
    fn o_rastreador_nao_mostra_diarias() {
        use shared::quests::quest_by_id;
        let log = vec![
            QuestNet::from_def(quest_by_id(601).unwrap(), quest_status::ACTIVE, 3),
            QuestNet::from_def(quest_by_id(502).unwrap(), quest_status::ACTIVE, 1),
            QuestNet::from_def(
                historia::def_da_historia(historia::PRIMEIRO_ID).unwrap(),
                quest_status::ACTIVE,
                0,
            ),
        ];
        let ids: Vec<u16> = ordem_do_rastreador(&log).iter().map(|q| q.id).collect();
        assert_eq!(
            ids,
            vec![historia::PRIMEIRO_ID, 502],
            "diaria fora, historia no topo"
        );
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
