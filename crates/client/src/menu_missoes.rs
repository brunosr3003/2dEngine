//! Menu de TODAS as missoes (rodape do rastreador ou Menu): cada uma com o estado pro jogador —
//! disponivel, em andamento, pronta, concluida ou bloqueada — e o "Ir".
//!
//! O estado e' calculado AQUI com o `QuestLog`, o `QuestEstado` (entregues e
//! faccao) e o nivel; quem valida aceitar e entregar continua sendo o servidor.
//! Bloqueada mostra o cadeado e, no hover, os pre-requisitos exatos.
use macroquad::prelude::*;
use shared::historia;
use shared::quests::{faction_id, zona_da_missao, QuestDef, QuestNet, QUESTS};
use std::collections::HashMap;

use crate::hud_estilo as estilo;
use crate::missoes::{progresso, pronta, Tem};

const LARGURA: f32 = 540.0;
const LINHA: f32 = 56.0;
const TITULO_GRUPO: f32 = 30.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Estado {
    Disponivel,
    EmAndamento {
        feito: u32,
        total: u32,
    },
    Pronta,
    Concluida,
    /// Os motivos, ja' em texto ("Requer nível 3").
    Bloqueada(Vec<String>),
}

/// O que o clique numa missao pede ao `main`.
#[derive(Debug, Clone, PartialEq)]
pub enum Clique {
    /// Disponivel aqui: ir ate' quem da' e abrir a oferta.
    IrAoGiver(u16),
    /// Em andamento ou pronta: a auto missao que ja' existe.
    AutoMissao(u16),
    /// Nao anda: so' avisa.
    Aviso(String),
}

/// O que o calculo precisa saber do jogador.
pub struct Contexto<'a> {
    pub log: &'a [QuestNet],
    /// Entregues: id -> fim do cooldown (unix, 0 = sem).
    pub entregues: &'a HashMap<u16, i64>,
    pub nivel: u32,
    pub faccao: u8,
    /// Zona (ilha) em que o jogador esta'.
    pub zona: Option<&'a str>,
    pub agora_unix: i64,
    pub tem: Tem<'a>,
}

fn nome_da_zona(z: &str) -> String {
    shared::terreno::def_da_zona(z).map_or_else(|| z.to_string(), |d| d.nome.to_string())
}

fn nome_da_faccao(f: u8) -> &'static str {
    match f {
        faction_id::MORGANEERS => "Morganeers",
        faction_id::PEACEMAIN => "Peacemain",
        _ => "?",
    }
}

/// Estado de `d` pro jogador.
pub fn estado(d: &QuestDef, c: &Contexto) -> Estado {
    if let Some(q) = c.log.iter().find(|q| q.id == d.id) {
        if pronta(q, c.tem) {
            return Estado::Pronta;
        }
        let (feito, total) = progresso(q, c.tem);
        return Estado::EmAndamento { feito, total };
    }
    let mut motivos = Vec::new();
    if let Some(&cd) = c.entregues.get(&d.id) {
        if !d.repeatable {
            return Estado::Concluida;
        }
        if cd > c.agora_unix {
            let min = ((cd - c.agora_unix) as f32 / 60.0).ceil() as i64;
            motivos.push(format!("Disponível de novo em {min} min"));
        }
    }
    if d.em_breve {
        motivos.push("Em breve".to_string());
    }
    match zona_da_missao(d.id) {
        Some(z) if Some(z) != c.zona => motivos.push(format!("Na ilha {}", nome_da_zona(z))),
        None => motivos.push("Indisponível nesta versão".to_string()),
        _ => {}
    }
    if d.min_level > c.nivel {
        motivos.push(format!("Requer nível {}", d.min_level));
    }
    if d.requires != 0 && !c.entregues.contains_key(&d.requires) {
        let t = shared::quests::quest_by_id(d.requires).map_or("?", |r| r.title);
        motivos.push(format!("Conclua: {t}"));
    }
    if d.faction != faction_id::NONE && d.faction != c.faccao {
        motivos.push(format!("Só para {}", nome_da_faccao(d.faction)));
    }
    if motivos.is_empty() {
        Estado::Disponivel
    } else {
        Estado::Bloqueada(motivos)
    }
}

/// O que clicar numa missao nesse estado faz.
pub fn clique_de(d: &QuestDef, e: &Estado) -> Clique {
    match e {
        Estado::Disponivel => Clique::IrAoGiver(d.id),
        Estado::EmAndamento { .. } | Estado::Pronta => Clique::AutoMissao(d.id),
        Estado::Concluida => Clique::Aviso(format!("\"{}\" já foi concluída.", d.title)),
        Estado::Bloqueada(m) => {
            Clique::Aviso(format!("\"{}\" bloqueada: {}.", d.title, m.join(" · ")))
        }
    }
}

/// Quanto falta pro reset das diarias (meia-noite UTC), "5h 07min".
pub fn reset_em(agora_unix: i64) -> String {
    let s = shared::quests::proxima_meia_noite(agora_unix) - agora_unix;
    format!("{}h {:02}min", s / 3600, (s % 3600) / 60)
}

/// Posicao na cadeia: quantas missoes vem antes pelo `requires`.
fn profundidade(d: &QuestDef) -> u32 {
    let mut n = 0;
    let mut atual = d.requires;
    while atual != 0 && n < 64 {
        n += 1;
        atual = shared::quests::quest_by_id(atual).map_or(0, |r| r.requires);
    }
    n
}

/// A primeira missao da cadeia de `d` (ela mesma, se nao pede nenhuma).
fn raiz(d: &QuestDef) -> u16 {
    let mut id = d.id;
    let mut atual = d.requires;
    for _ in 0..64 {
        if atual == 0 {
            break;
        }
        id = atual;
        atual = shared::quests::quest_by_id(atual).map_or(0, |r| r.requires);
    }
    id
}

/// As missoes do menu, por ilha e na ordem da cadeia. As do mapa de tiles
/// antigo (sem giver nas ilhas) ficam FORA: nao ha' como fazer nenhuma. As
/// DIARIAS tambem: tem painel proprio (`diarias`).
pub fn todas() -> Vec<&'static QuestDef> {
    let mut v: Vec<&'static QuestDef> = QUESTS
        .iter()
        .filter(|d| !d.daily && zona_da_missao(d.id).is_some())
        .collect();
    v.sort_by(|a, b| {
        zona_da_missao(a.id)
            .cmp(&zona_da_missao(b.id))
            // Cadeia por cadeia: com varias correndo em paralelo, ordenar so'
            // pela profundidade intercalava todas.
            .then(raiz(a).cmp(&raiz(b)))
            .then(profundidade(a).cmp(&profundidade(b)))
            .then(a.id.cmp(&b.id))
    });
    v
}

/// Indice do passo atual da historia, pelo passo que esta' no log.
pub fn atual_da_historia(log: &[QuestNet]) -> Option<u32> {
    log.iter().find_map(|q| historia::indice(q.id))
}

/// Estado de um passo da historia: concluido (antes do atual), o atual, ou
/// futuro com o motivo — a trava de nivel no caminho, o passo anterior, a ilha.
pub fn estado_da_historia(d: &QuestDef, c: &Contexto) -> Estado {
    if let Some(q) = c.log.iter().find(|q| q.id == d.id) {
        if pronta(q, c.tem) {
            return Estado::Pronta;
        }
        let (feito, total) = progresso(q, c.tem);
        return Estado::EmAndamento { feito, total };
    }
    let i = historia::indice(d.id).unwrap_or(0);
    let atual = atual_da_historia(c.log);
    if atual.is_some_and(|a| i < a) {
        return Estado::Concluida;
    }
    let mut motivos = Vec::new();
    let de = atual.unwrap_or(0);
    for k in de..i {
        let Some(p) = historia::id_do_passo(k).and_then(historia::def_da_historia) else {
            break;
        };
        if p.obj_kind == shared::quests::objective_kind::NIVEL && p.obj_count > c.nivel {
            motivos.push(format!("Requer nível {}", p.obj_count));
            break;
        }
    }
    if let Some(p) = i
        .checked_sub(1)
        .and_then(historia::id_do_passo)
        .and_then(historia::def_da_historia)
    {
        motivos.push(format!("Conclua: {}", p.title));
    }
    if let Some(z) = zona_da_missao(d.id).filter(|z| Some(*z) != c.zona) {
        motivos.push(format!("Na ilha {}", nome_da_zona(z)));
    }
    Estado::Bloqueada(motivos)
}

/// Quantas cronicas do epilogo o menu mostra a partir da atual.
const CRONICAS_NO_MENU: u32 = 2;

/// A lista do menu: a HISTORIA primeiro (os capitulos escritos e as cronicas
/// da atual e da seguinte — o resto nao tem fim), depois as missoes das ilhas.
pub fn lista_do_menu(log: &[QuestNet]) -> Vec<&'static QuestDef> {
    let escritos = historia::total_escritos();
    let atual = atual_da_historia(log).unwrap_or(0);
    let cronica = atual.saturating_sub(escritos) / historia::PASSOS_POR_CRONICA;
    let fim = escritos + (cronica + CRONICAS_NO_MENU) * historia::PASSOS_POR_CRONICA;
    let mut v: Vec<&'static QuestDef> = (0..fim)
        .filter(|i| *i < escritos || *i >= escritos + cronica * historia::PASSOS_POR_CRONICA)
        .filter_map(|i| historia::id_do_passo(i).and_then(historia::def_da_historia))
        .collect();
    v.extend(todas());
    v
}

/// O grupo (cabecalho) de uma linha do menu.
fn grupo(d: &QuestDef) -> String {
    match historia::indice(d.id) {
        Some(i) => format!("História · {}", historia::nome_do_capitulo(i)),
        None => format!("{:?}", zona_da_missao(d.id)),
    }
}

#[derive(Default)]
pub struct MenuMissoes {
    pub aberto: bool,
    rolagem: crate::rolagem::Rolagem,
}

impl MenuMissoes {
    /// Abre pelo rodape do rastreador ou pelo Menu — nunca por tecla.
    pub fn abrir(&mut self) {
        self.aberto = true;
        self.rolagem.zera();
    }

    pub fn alterna(&mut self) {
        self.aberto = !self.aberto;
        self.rolagem.zera();
    }

    fn painel() -> Rect {
        let h = (screen_height() - 140.0).clamp(240.0, 720.0);
        Rect::new(
            (screen_width() - LARGURA) * 0.5,
            (screen_height() - h) * 0.5,
            LARGURA,
            h,
        )
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto && Self::painel().contains(Vec2::from(mouse_position()))
    }

    /// Desenha o menu e devolve o clique do quadro.
    pub fn desenha(&mut self, c: &Contexto) -> Option<Clique> {
        if !self.aberto {
            return None;
        }
        let p = Self::painel();
        estilo::painel(p);
        estilo::texto(p.x + 18.0, p.y + 32.0, "Todas as missões", 22, estilo::OURO);
        estilo::texto(
            p.x + 18.0,
            p.y + 52.0,
            "Esc fecha · clique: ir (se liberada e nesta ilha)",
            13,
            estilo::SUAVE,
        );
        if crate::ui::botao(
            Rect::new(p.x + p.w - 44.0, p.y + 10.0, 32.0, 28.0),
            "x",
            true,
        ) {
            self.aberto = false;
            return None;
        }
        let area = Rect::new(p.x + 10.0, p.y + 64.0, p.w - 20.0, p.h - 74.0);
        let mouse = Vec2::from(mouse_position());
        let lista = lista_do_menu(c.log);
        // Altura total pra limitar a rolagem.
        // Grupo = capitulo da historia, ou ilha.
        let mut grupos = 0;
        let mut ultima: Option<String> = None;
        for d in &lista {
            let chave = grupo(d);
            if Some(&chave) != ultima.as_ref() {
                grupos += 1;
                ultima = Some(chave);
            }
        }
        let total = grupos as f32 * TITULO_GRUPO + lista.len() as f32 * LINHA;
        // Toque na linha (ou no "Ir") vale no SOLTAR: a mesma lista rola
        // arrastando, e o aperto nao sabe ainda qual dos dois vai ser.
        let clique = self.rolagem.quadro(area, total, LINHA);
        let arrastando = self.rolagem.arrastando();
        let mut saida = None;
        let mut dica: Option<Vec<String>> = None;
        let mut y = area.y - self.rolagem.pos;
        crate::rolagem::recortar(Some(area));
        let mut ultima: Option<String> = None;
        if lista.is_empty() {
            estilo::texto(
                area.x + 8.0,
                area.y + 20.0,
                "Nenhuma missão nesta versão.",
                15,
                estilo::SUAVE,
            );
        }
        for d in &lista {
            let z = zona_da_missao(d.id);
            let chave = grupo(d);
            if Some(&chave) != ultima.as_ref() {
                ultima = Some(chave.clone());
                if y + TITULO_GRUPO > area.y && y < area.y + area.h {
                    let (t, destaque) = if historia::e_da_historia(d.id) {
                        (chave, true)
                    } else {
                        let aqui = z == c.zona;
                        let ilha = z.map_or_else(|| "?".into(), nome_da_zona);
                        let t = format!("{ilha}{}", if aqui { " · você está aqui" } else { "" });
                        (t, aqui)
                    };
                    estilo::texto(
                        area.x + 6.0,
                        y + 21.0,
                        &t,
                        15,
                        if destaque {
                            estilo::OURO
                        } else {
                            estilo::SUAVE
                        },
                    );
                }
                y += TITULO_GRUPO;
            }
            let linha = Rect::new(area.x, y, area.w - 12.0, LINHA - 4.0);
            y += LINHA;
            if linha.y + linha.h < area.y || linha.y > area.y + area.h {
                continue;
            }
            let e = if historia::e_da_historia(d.id) {
                estado_da_historia(d, c)
            } else {
                estado(d, c)
            };
            let sobre = !arrastando && linha.contains(mouse) && area.contains(mouse);
            let fundo = if sobre { 0.08 } else { 0.03 };
            draw_rectangle(
                linha.x,
                linha.y,
                linha.w,
                linha.h,
                Color::new(1.0, 1.0, 1.0, fundo),
            );
            let (rotulo, cor) = match &e {
                Estado::Disponivel => ("Disponível".to_string(), Color::new(1.0, 0.84, 0.2, 1.0)),
                Estado::EmAndamento { feito, total } => {
                    (format!("Em andamento · {feito}/{total}"), estilo::TEXTO)
                }
                Estado::Pronta => ("Pronta pra entregar".to_string(), estilo::AUTO),
                Estado::Concluida => ("Concluída".to_string(), estilo::SUAVE),
                Estado::Bloqueada(m) => (
                    m.first().cloned().unwrap_or_default(),
                    Color::new(0.85, 0.45, 0.40, 1.0),
                ),
            };
            let icone = vec2(linha.x + 22.0, linha.y + linha.h * 0.5);
            match &e {
                Estado::Bloqueada(_) => cadeado(icone, 9.0, Color::new(0.85, 0.45, 0.40, 1.0)),
                Estado::Concluida => {
                    draw_line(
                        icone.x - 7.0,
                        icone.y,
                        icone.x - 2.0,
                        icone.y + 6.0,
                        3.0,
                        estilo::SUAVE,
                    );
                    draw_line(
                        icone.x - 2.0,
                        icone.y + 6.0,
                        icone.x + 8.0,
                        icone.y - 7.0,
                        3.0,
                        estilo::SUAVE,
                    );
                }
                Estado::Pronta => {
                    estilo::texto_centro(icone.x, icone.y + 10.0, "?", 26, estilo::AUTO)
                }
                Estado::Disponivel => estilo::texto_centro(icone.x, icone.y + 10.0, "!", 26, cor),
                Estado::EmAndamento { .. } => {
                    draw_circle_lines(icone.x, icone.y, 8.0, 2.0, estilo::TEXTO)
                }
            }
            let titulo_cor = if matches!(e, Estado::Bloqueada(_) | Estado::Concluida) {
                estilo::SUAVE
            } else {
                estilo::TEXTO
            };
            estilo::texto_ajustado(
                d.title,
                linha.x + 44.0,
                linha.y + 21.0,
                linha.w - 140.0,
                16,
                titulo_cor,
            );
            estilo::texto_ajustado(
                &rotulo,
                linha.x + 44.0,
                linha.y + 41.0,
                linha.w - 140.0,
                13,
                cor,
            );
            let clicavel = matches!(
                e,
                Estado::Disponivel | Estado::EmAndamento { .. } | Estado::Pronta
            );
            let b = Rect::new(linha.x + linha.w - 82.0, linha.y + 12.0, 70.0, 28.0);
            let tocou = |r: Rect| clique.is_some_and(|c| r.contains(c) && area.contains(c));
            if clicavel {
                // Desenha o botao; quem decide e' o toque no soltar.
                let _ = crate::ui::botao(b, "Ir", true);
                if tocou(b) {
                    saida = Some(clique_de(d, &e));
                }
            } else if tocou(linha) {
                saida = Some(clique_de(d, &e));
            }
            if let Estado::Bloqueada(m) = &e {
                if sobre {
                    dica = Some(m.clone());
                }
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if let Some(m) = dica {
            let w = m
                .iter()
                .map(|s| estilo::medir(s, 14))
                .fold(160.0f32, f32::max)
                + 24.0;
            let h = 30.0 + m.len() as f32 * 20.0;
            let x = (mouse.x + 16.0).min(screen_width() - w - 8.0);
            let y = (mouse.y + 12.0).min(screen_height() - h - 8.0);
            estilo::painel(Rect::new(x, y, w, h));
            cadeado(
                vec2(x + 16.0, y + 17.0),
                7.0,
                Color::new(0.85, 0.45, 0.40, 1.0),
            );
            estilo::texto(x + 30.0, y + 22.0, "Pré-requisitos", 14, estilo::OURO);
            for (i, s) in m.iter().enumerate() {
                estilo::texto(x + 12.0, y + 42.0 + i as f32 * 20.0, s, 14, estilo::TEXTO);
            }
        }
        saida
    }
}

/// Cadeado: arco em cima e corpo.
pub(crate) fn cadeado(c: Vec2, s: f32, cor: Color) {
    if crate::icones_ui::ui("cadeado", c, s * 2.6, cor) {
        return;
    }
    draw_circle_lines(c.x, c.y - s * 0.35, s * 0.55, s * 0.22, cor);
    draw_rectangle(c.x - s * 0.8, c.y - s * 0.2, s * 1.6, s * 1.2, cor);
    draw_circle(c.x, c.y + s * 0.3, s * 0.18, Color::new(0.0, 0.0, 0.0, 0.7));
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::quests::{quest_by_id, quest_status};

    fn nada(_: u16) -> u32 {
        0
    }

    fn ctx<'a>(
        log: &'a [QuestNet],
        entregues: &'a HashMap<u16, i64>,
        nivel: u32,
        zona: Option<&'a str>,
    ) -> Contexto<'a> {
        Contexto {
            log,
            entregues,
            nivel,
            faccao: faction_id::PEACEMAIN,
            zona,
            agora_unix: 1_000,
            tem: &nada,
        }
    }

    #[test]
    fn a_cadeia_do_mestre_bloqueia_com_o_motivo_certo() {
        let (d501, d502, d504) = (
            quest_by_id(501).unwrap(),
            quest_by_id(502).unwrap(),
            quest_by_id(504).unwrap(),
        );
        let vazio = HashMap::new();
        let c = ctx(&[], &vazio, 1, Some("ilha_inicial"));
        assert_eq!(estado(d501, &c), Estado::Disponivel);
        assert_eq!(
            estado(d502, &c),
            Estado::Bloqueada(vec!["Conclua: Conheça o Alquimista".into()])
        );
        // 504: nivel 3 e a 503 antes.
        match estado(d504, &c) {
            Estado::Bloqueada(m) => {
                assert!(m.contains(&"Requer nível 3".to_string()), "{m:?}");
                assert!(m.iter().any(|s| s.starts_with("Conclua: ")), "{m:?}");
            }
            outro => panic!("{outro:?}"),
        }
        // Outra ilha: bloqueia dizendo onde.
        let fora = ctx(&[], &vazio, 1, Some("ilha_gelo"));
        assert_eq!(
            estado(d501, &fora),
            Estado::Bloqueada(vec!["Na ilha Bosque".into()])
        );
    }

    #[test]
    fn log_e_entregues_viram_andamento_pronta_e_concluida() {
        let d501 = quest_by_id(501).unwrap();
        let d502 = quest_by_id(502).unwrap();
        let mut entregues = HashMap::new();
        entregues.insert(501, 0);
        let mut q = QuestNet::from_def(d502, quest_status::ACTIVE, 2);
        let log = vec![q.clone()];
        let c = ctx(&log, &entregues, 1, Some("ilha_inicial"));
        assert_eq!(estado(d501, &c), Estado::Concluida);
        assert_eq!(estado(d502, &c), Estado::EmAndamento { feito: 2, total: 6 });
        q.status = quest_status::READY;
        let log = vec![q];
        let c = ctx(&log, &entregues, 1, Some("ilha_inicial"));
        assert_eq!(estado(d502, &c), Estado::Pronta);
    }

    #[test]
    fn liberada_anda_e_bloqueada_so_avisa() {
        let d501 = quest_by_id(501).unwrap();
        assert_eq!(clique_de(d501, &Estado::Disponivel), Clique::IrAoGiver(501));
        assert_eq!(clique_de(d501, &Estado::Pronta), Clique::AutoMissao(501));
        assert_eq!(
            clique_de(d501, &Estado::EmAndamento { feito: 0, total: 1 }),
            Clique::AutoMissao(501)
        );
        match clique_de(d501, &Estado::Bloqueada(vec!["Requer nível 3".into()])) {
            Clique::Aviso(s) => assert!(s.contains("Requer nível 3"), "{s}"),
            outro => panic!("bloqueada andou: {outro:?}"),
        }
        assert!(matches!(
            clique_de(d501, &Estado::Concluida),
            Clique::Aviso(_)
        ));
    }

    #[test]
    fn o_menu_lista_a_cadeia_em_ordem_e_esconde_as_legadas() {
        let bosque: Vec<u16> = todas()
            .iter()
            .filter(|d| zona_da_missao(d.id) == Some("ilha_inicial"))
            .map(|d| d.id)
            .collect();
        // 501-505 e' a cadeia original; 506-509 e' a oficina do Mestre, que
        // apresenta madeira, darksteel, quintessencia e berloque — as fontes
        // que a receita cinza pede e que o inicio nunca mostrava; 510 leva a
        // primeira dungeon, que era o unico lugar do inicio que ninguem
        // apresentava. A lista fica literal de proposito: e' ela que pega um id
        // legado caindo por engano na faixa da ilha. 511-538 sao as seis
        // cadeias paralelas do resto da ilha (chefes, bestiario, oficina,
        // coleta, dungeons, vila).
        let esperado: Vec<u16> = (501..=538).collect();
        assert_eq!(&bosque[..], &esperado[..], "so' as cadeias, em ordem");
        assert!(
            todas().iter().all(|d| !d.daily),
            "diaria no menu de todas: tem painel proprio"
        );
        assert!(
            todas().iter().all(|d| zona_da_missao(d.id).is_some()),
            "legada no menu"
        );
    }

    /// Historia: antes do atual concluido, o atual em andamento, o futuro com o
    /// passo anterior como motivo (e sem trava de nivel no capitulo I); o menu mostra os
    /// capitulos escritos e so' as cronicas perto do atual.
    #[test]
    fn historia_no_menu_concluida_atual_e_futura() {
        let vazio = HashMap::new();
        let atual = historia::def_da_historia(705).unwrap();
        let log = vec![QuestNet::from_def(atual, quest_status::ACTIVE, 3)];
        let c = ctx(&log, &vazio, 4, Some("ilha_inicial"));
        assert_eq!(
            estado_da_historia(historia::def_da_historia(702).unwrap(), &c),
            Estado::Concluida
        );
        assert_eq!(
            estado_da_historia(atual, &c),
            Estado::EmAndamento {
                feito: 3,
                total: 10
            }
        );
        match estado_da_historia(historia::def_da_historia(712).unwrap(), &c) {
            Estado::Bloqueada(m) => {
                // Capitulo I sem trava de nivel: a historia carrega o nivel.
                assert!(!m.iter().any(|s| s.starts_with("Requer nível")), "{m:?}");
                assert!(m.iter().any(|s| s.starts_with("Conclua: ")), "{m:?}");
            }
            outro => panic!("{outro:?}"),
        }
        assert!(matches!(
            clique_de(
                atual,
                &Estado::EmAndamento {
                    feito: 3,
                    total: 10
                }
            ),
            Clique::AutoMissao(705)
        ));
        // Outra ilha no futuro: diz onde.
        match estado_da_historia(historia::def_da_historia(719).unwrap(), &c) {
            Estado::Bloqueada(m) => assert!(m.iter().any(|s| s == "Na ilha Geleira"), "{m:?}"),
            outro => panic!("{outro:?}"),
        }
        let lista = lista_do_menu(&log);
        assert_eq!(lista[0].id, historia::PRIMEIRO_ID);
        let n_hist = lista
            .iter()
            .filter(|d| historia::e_da_historia(d.id))
            .count() as u32;
        assert_eq!(
            n_hist,
            historia::total_escritos() + CRONICAS_NO_MENU * historia::PASSOS_POR_CRONICA
        );
        // Na cronica 3, o menu mostra a 3 e a 4 (nao todas desde a 1).
        let longe = historia::id_do_passo(
            historia::total_escritos() + 2 * historia::PASSOS_POR_CRONICA + 1,
        )
        .unwrap();
        let log = vec![QuestNet::from_def(
            historia::def_da_historia(longe).unwrap(),
            quest_status::ACTIVE,
            0,
        )];
        let ids: Vec<u16> = lista_do_menu(&log)
            .iter()
            .map(|d| d.id)
            .filter(|id| *id >= historia::PRIMEIRO_ID_DO_EPILOGO)
            .collect();
        assert_eq!(
            ids.first().copied(),
            historia::id_do_passo(historia::total_escritos() + 2 * historia::PASSOS_POR_CRONICA)
        );
    }

    /// Diaria de sistema que ainda nao existe: cadeado com "Em breve"; reset
    /// conta ate' a meia-noite UTC.
    #[test]
    fn diaria_em_breve_bloqueia_e_o_reset_conta_ate_a_meia_noite() {
        let vazio = HashMap::new();
        let c = ctx(&[], &vazio, 50, Some("ilha_inicial"));
        assert_eq!(
            estado(quest_by_id(607).unwrap(), &c),
            Estado::Bloqueada(vec!["Em breve".into()])
        );
        assert_eq!(estado(quest_by_id(601).unwrap(), &c), Estado::Disponivel);
        assert_eq!(reset_em(86_400 * 10 + 3_600 * 19 + 60 * 53), "4h 07min");
    }
}
