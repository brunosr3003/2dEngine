//! A PROGRESSÃO: o que o bot faz quando a história não manda nada.
//!
//! O pedido do dono (27/09/2026): "muda os bots pra eles fazerem todo o
//! trajeto do jogo, andarem pelo mapa em zonas livres pra upar, fazer itens,
//! craft, coleta de darksteel, energia e tudo mais".
//!
//! Até aqui o bot só sabia seguir missão. Quando a ilha inicial acabava — ou a
//! missão emperrava num NPC —, ele vagava em rumo sorteado e batia no que
//! aparecesse, sempre na mesma ilha, com o equipamento com que nasceu, e
//! gastando todo ponto em FOR. Foi o que a trilha mostrou em 27/09: doze bots
//! entre os níveis 2 e 4, quatro deles presos no mesmo canto.
//!
//! Esta camada é o que um jogador faz entre uma missão e outra, em ordem:
//!
//! 1. **veste** a peça melhor que caiu na bolsa (mesma classe, mesmo peso);
//! 2. **coleta Energia** quando há ponto de atributo parado por falta dela;
//! 3. **refina** o equipamento vestido até o +5, onde a forja ainda não quebra
//!    a peça, com o darksteel e o cobre que juntou;
//! 4. **fabrica** a peça da faixa quando tem material, e vai colher o que falta;
//! 5. **entra na Ilha Mágica** com as entradas grátis do dia;
//! 6. **caça na zona da faixa** que o mapa da ilha mostra — e, quando a ilha
//!    fica pequena pro nível, **embarca** no Capitão do Porto pra próxima.
//!
//! Tudo pelo mesmo caminho de um cliente: `MapaDaIlha`, `Viagem`, `Magica`,
//! `Refinar`, `Craft`, `InventorySwap`. E tudo com a mesma rede do resto do
//! arquivo: todo ramo que pode não dar tem teto, e todo teto vai pra trilha.

use std::time::{Duration, Instant};

use shared::constants::EquipSlot;
use shared::protocol::{ClientMessage, InvSpot, ZonaNoMapa};

use super::{envia, ev, Eu, Ws};
use crate::trilha::Trilha;
use futures_util::SinkExt;

/// Até onde se refina sozinho. Do +6 em diante a falha DESTRÓI a peça
/// (`forja::chance_de_refino`), e isso é decisão de jogador, não de bot.
pub const REFINO_SEGURO: u8 = 5;
/// Intervalo entre tentativas de refino e de craft de equipamento.
const FORJA_A_CADA: Duration = Duration::from_secs(40);
/// Depois de uma recusa por falta de material, espera isto antes de tentar de
/// novo — o darksteel não aparece em quarenta segundos.
const FORJA_SECA: Duration = Duration::from_secs(300);
/// De quanto em quanto tempo pergunta o estado da Ilha Mágica.
const MAGICA_A_CADA: Duration = Duration::from_secs(600);
/// Quanto tempo se dá a uma ida (até a zona, até o capitão) antes de largar.
const PRAZO_DA_IDA: Duration = Duration::from_secs(90);
/// Quanto tempo se fica colhendo Energia de uma vez.
const PRAZO_DA_ENERGIA: Duration = Duration::from_secs(180);

/// O que o bot está perseguindo fora da história.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Plano {
    /// Caçar numa zona do mapa: centro, raio, faixa.
    Cacar {
        centro: glam::Vec2,
        raio: f32,
        lv: (u16, u16),
        desde: Instant,
    },
    /// Colher Energia pra pagar os pontos de atributo.
    Energia { desde: Instant },
    /// Ir embarcar: primeiro à cidade, depois ao Capitão, depois o menu.
    Embarcar { etapa: Etapa, desde: Instant },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Etapa {
    /// Andando até a praça, onde o cais está perto.
    ACidade,
    /// Viu o Capitão: indo até ele.
    AoCapitao,
    /// Falou com ele e espera o menu (`Viagem`).
    Menu,
}

/// O estado da progressão, dentro do `Eu`.
#[derive(Default)]
pub struct Progresso {
    pub plano: Option<Plano>,
    /// Zonas de mob da ilha atual (`MapaDaIlha`).
    pub zonas: Vec<ZonaNoMapa>,
    /// Onde o personagem nasceu nesta zona: é a praça, e o cais fica perto.
    pub cidade: Option<glam::Vec2>,
    /// O que está vestido (`StatsUpdate`).
    pub equip: shared::Equipment,
    /// Pontos alocados por atributo (`StatPointsUpdate`).
    pub alocados: [u32; shared::STAT_COUNT],
    /// Energia disponível (`ProgressoDeSkills`).
    pub energia: u64,
    /// `kind` dos NPCs vistos (papel da vila nos bits de cima).
    pub npc_kind: std::collections::HashMap<shared::EntityId, u16>,
    /// Última vez que tentou refino ou craft de equipamento.
    pub forjou_em: Option<Instant>,
    /// Forja recusou por material: até quando não tentar de novo.
    pub forja_seca_ate: Option<Instant>,
    /// Qual slot vestido está sendo refinado agora (espera a resposta).
    pub refinando: Option<EquipSlot>,
    /// Slots que chegaram ao teto seguro ou recusaram de vez nesta vida.
    pub refino_pronto: std::collections::HashSet<u8>,
    /// Estado da Ilha Mágica que o servidor mandou.
    pub magica: Option<Magica>,
    pub perguntou_magica_em: Option<Instant>,
    pub pediu_magica_em: Option<Instant>,
    /// Destinos do menu do Capitão (`Viagem`).
    pub destinos: Vec<shared::viagem::Destino>,
    /// Embarques que não deram: não insistir na mesma ilha tão cedo.
    pub embarque_falhou_em: Option<Instant>,
    /// Ponto dentro da zona pra onde está andando atrás de bicho.
    pub rondando: Option<(glam::Vec2, Instant)>,
    /// Receitas de equipamento já recusadas nesta vida (id).
    pub receita_seca: std::collections::HashSet<u16>,
    /// Última peça vestida (evita trocar duas vezes no mesmo instante).
    pub vestiu_em: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Magica {
    pub grau_maximo: u8,
    pub gratis: u8,
    pub passes: u32,
    pub dentro: bool,
}

/// O atributo principal da classe, pela arma: é o que `balanceamento::
/// build_do_nivel` assume, e o que o jogador de cada classe sobe.
pub fn atributo_da_classe(arma: u16) -> usize {
    use shared::item_id::*;
    use shared::stat_idx;
    match arma {
        PISTOLAS => stat_idx::DES,
        ANEL_MAGICO => stat_idx::INT,
        _ => stat_idx::FOR,
    }
}

/// O próximo ponto: um no principal a cada três, o resto em VIT — a divisão
/// da build de referência (`balanceamento::build_do_nivel`).
pub fn proximo_ponto(arma: u16, alocados: &[u32; shared::STAT_COUNT]) -> u8 {
    let principal = atributo_da_classe(arma);
    let vit = shared::stat_idx::VIT;
    if alocados[principal] * 2 <= alocados[vit] {
        principal as u8
    } else {
        vit as u8
    }
}

/// Quanto a peça vale, na conta da ficha (`dungeon::poder_de_stats`), com o
/// refino. É o que decide "esta é melhor que a vestida".
fn poder_da_peca(inst: Option<&shared::items::ItemInstance>, id: u16) -> i32 {
    let b = match inst {
        Some(i) => i.effective_bonus(),
        None => shared::item_bonus(id),
    };
    b.attack_damage * 10 + b.defense * 8 + b.hp_max + b.mp_max / 2 + (b.dex + b.wis) * 5
}

/// A peça da bolsa pode entrar neste slot sem mudar a classe nem o peso?
///
/// Arma, secundária e armadura: só a MESMA peça (`item_id`) melhor. Trocar de
/// katana pra pistola é trocar de classe, e trocar de armadura média pra
/// pesada é trocar de estilo — o bot não faz escolha de build, faz upgrade.
/// Acessório é igual pra todo mundo, então qualquer um serve.
fn cabe_no_slot(eu: &Eu, id: u16, slot: EquipSlot) -> bool {
    match slot {
        EquipSlot::Weapon | EquipSlot::Offhand | EquipSlot::Armor => {
            eu.prog.equip.get(slot).is_none_or(|atual| atual == id)
        }
        EquipSlot::Earring | EquipSlot::Necklace | EquipSlot::Bracelet | EquipSlot::Belt => true,
        _ => false,
    }
}

/// A melhor troca de equipamento disponível agora: (slot da bolsa, slot vestido).
fn melhor_troca(eu: &Eu) -> Option<(u16, EquipSlot, i32, i32)> {
    let mut melhor: Option<(u16, EquipSlot, i32, i32)> = None;
    for (i, s) in &eu.slots {
        let Some(slot) = shared::constants::equip_slot_of(s.item_id) else {
            continue;
        };
        if !cabe_no_slot(eu, s.item_id, slot) {
            continue;
        }
        // Peça acima do nível o servidor recusa (`level_req`).
        if s.instance.and_then(|x| x.level_req).is_some_and(|r| r as u32 > eu.nivel) {
            continue;
        }
        let novo = poder_da_peca(s.instance.as_ref(), s.item_id);
        let atual = match eu.prog.equip.get(slot) {
            Some(id) => poder_da_peca(eu.prog.equip.get_inst(slot).as_ref(), id),
            None => 0,
        };
        // 5% de folga: trocar por empate é vaivém.
        if novo as f32 > atual as f32 * 1.05 && melhor.is_none_or(|m| novo - atual > m.2 - m.3) {
            melhor = Some((*i, slot, novo, atual));
        }
    }
    melhor
}

/// A zona da faixa: a de maior nível que o personagem ainda aguenta.
///
/// "Aguenta" é `lv_min <= nível + 1`; "vale a pena" é `lv_max + 3 >= nível`.
/// Forte só depois de passar da faixa dele — a horda soma (docs/ESCADA.md), e
/// um bot na faixa certa morre lá sem grupo.
pub fn zona_da_faixa(zonas: &[ZonaNoMapa], nivel: u32, perto: glam::Vec2) -> Option<&ZonaNoMapa> {
    let n = nivel as i32;
    zonas
        .iter()
        .filter(|z| (z.lv_min as i32) <= n + 1 && (z.lv_max as i32) + 3 >= n)
        .filter(|z| !z.forte || n > z.lv_max as i32)
        .max_by(|a, b| {
            // Maior faixa primeiro; empate, a mais perto.
            a.lv_min.cmp(&b.lv_min).then_with(|| {
                let da = glam::Vec2::from(a.centro).distance_squared(perto);
                let db = glam::Vec2::from(b.centro).distance_squared(perto);
                db.total_cmp(&da)
            })
        })
}

/// A ilha pequena demais? Nenhuma zona chega perto do nível.
pub fn ilha_pequena(zonas: &[ZonaNoMapa], nivel: u32) -> bool {
    !zonas.is_empty() && zonas.iter().all(|z| (z.lv_max as u32) + 3 < nivel)
}

/// O destino do menu do Capitão pra esse nível: a ilha liberada de maior faixa
/// que ainda aceita o personagem.
pub fn destino_pro_nivel(destinos: &[shared::viagem::Destino], nivel: u32) -> Option<u8> {
    use shared::viagem::estado;
    destinos
        .iter()
        .filter(|d| d.estado == estado::LIBERADA && d.nivel_min as u32 <= nivel)
        .max_by_key(|d| d.nivel_min)
        .map(|d| d.ilha)
}

/// A DECISÃO DA PROGRESSÃO. Devolve `true` se agiu (a decisão acaba aqui).
pub async fn progride(ws: &mut Ws, eu: &mut Eu, nome: &str, t: &Trilha) -> anyhow::Result<bool> {
    let agora = Instant::now();

    // 1. VESTE O QUE É MELHOR. Uma troca por decisão, e nunca duas no mesmo
    // segundo: o `StatsUpdate` com o equipamento novo precisa chegar antes de
    // comparar de novo, senão ele troca a mesma peça duas vezes.
    if eu.prog.vestiu_em.is_none_or(|t| agora.duration_since(t) > Duration::from_secs(3)) {
        if let Some((slot, onde, novo, atual)) = melhor_troca(eu) {
            eu.prog.vestiu_em = Some(agora);
            ws.send(envia(&ClientMessage::InventorySwap {
                a: InvSpot::Inv(slot),
                b: InvSpot::Equip(onde),
            })?)
            .await?;
            t.registra(ev(nome, eu, "vestiu", true, format!(
                "{onde:?}: poder {atual} -> {novo}"
            )));
            return Ok(true);
        }
    }

    // 2. ENERGIA pros pontos parados.
    let custo = shared::custo_energia_do_ponto(eu.prog.alocados.iter().sum());
    let falta_energia = eu.pontos_livres > 0 && eu.prog.energia < custo;
    match eu.prog.plano {
        Some(super::progresso::Plano::Energia { desde }) => {
            let bastou = !falta_energia;
            if bastou || agora.duration_since(desde) > PRAZO_DA_ENERGIA {
                eu.prog.plano = None;
                eu.no_de_coleta = None;
                t.registra(ev(nome, eu, "energia_colhida", bastou, format!(
                    "energia {} (ponto custa {custo})", eu.prog.energia
                )));
                return Ok(true);
            }
            return colhe_energia(ws, eu, nome, t).await.map(|_| true);
        }
        _ if falta_energia && eu.nivel >= 2 => {
            eu.prog.plano = Some(Plano::Energia { desde: agora });
            eu.no_de_coleta = None;
            t.registra(ev(nome, eu, "foi_colher_energia", true, format!(
                "{} ponto(s) parado(s), energia {} de {custo}", eu.pontos_livres, eu.prog.energia
            )));
            return Ok(true);
        }
        _ => {}
    }

    // 3 e 4. FORJA: refino e equipamento da faixa, de tempos em tempos.
    let forja_livre = eu.prog.forjou_em.is_none_or(|t| agora.duration_since(t) >= FORJA_A_CADA)
        && eu.prog.forja_seca_ate.is_none_or(|t| agora >= t)
        && eu.prog.refinando.is_none();
    if forja_livre {
        eu.prog.forjou_em = Some(agora);
        if refina(ws, eu, nome, t).await? {
            return Ok(true);
        }
        if fabrica(ws, eu, nome, t).await? {
            return Ok(true);
        }
    }

    // 5. ILHA MÁGICA com as entradas grátis do dia.
    if magica(ws, eu, nome, t).await? {
        return Ok(true);
    }

    // 6. CAÇAR NA FAIXA, ou embarcar pra próxima ilha.
    caca_ou_embarca(ws, eu, nome, t).await
}

/// Colhe Energia: pede o nó de energia mais perto e vai até ele.
async fn colhe_energia(ws: &mut Ws, eu: &mut Eu, nome: &str, t: &Trilha) -> anyhow::Result<()> {
    if eu.colhendo {
        return Ok(());
    }
    match eu.no_de_coleta {
        Some((coluna, onde)) => {
            let d = eu.pos.distance(onde);
            if d > shared::COLETA_ALCANCE_UN * 0.6 {
                if !super::empurra_no(eu, onde) {
                    eu.no_de_coleta = None;
                    t.registra(ev(nome, eu, "energia_inalcancavel", false, format!("a {d:.1}u do nó")));
                }
                return Ok(());
            }
            eu.no_de_coleta = None;
            ws.send(envia(&ClientMessage::ColetarNo { coluna })?).await?;
            t.registra(ev(nome, eu, "colheu_energia", true, format!("energia {}", eu.prog.energia)));
        }
        None => {
            ws.send(envia(&ClientMessage::PedirNoDeColeta {
                tipos: [false; 5],
                energia: true,
                raio: 80.0,
                centro: [eu.pos.x, eu.pos.y],
            })?)
            .await?;
            eu.pedidos_de_no += 1;
            if eu.pedidos_de_no >= 8 {
                eu.pedidos_de_no = 0;
                eu.prog.plano = None;
                t.registra(ev(nome, eu, "sem_energia_por_perto", false, "nada em 80u".into()));
            }
        }
    }
    Ok(())
}

/// Refina a peça vestida de menor refino, até o teto seguro.
async fn refina(ws: &mut Ws, eu: &mut Eu, nome: &str, t: &Trilha) -> anyhow::Result<bool> {
    // Arma primeiro (dano), depois armadura (vida e defesa), depois o resto.
    const ORDEM: [EquipSlot; 7] = [
        EquipSlot::Weapon,
        EquipSlot::Armor,
        EquipSlot::Offhand,
        EquipSlot::Bracelet,
        EquipSlot::Belt,
        EquipSlot::Earring,
        EquipSlot::Necklace,
    ];
    let alvo = ORDEM
        .iter()
        .filter(|s| !eu.prog.refino_pronto.contains(&(**s as u8)))
        .filter_map(|s| eu.prog.equip.get_inst(*s).map(|i| (*s, i.refinement)))
        .filter(|(_, r)| *r < REFINO_SEGURO)
        .min_by_key(|(_, r)| *r);
    let Some((slot, nivel)) = alvo else {
        return Ok(false);
    };
    // Sem darksteel nenhum nem adianta pedir: a recusa é certa.
    if eu.bolsa.get(&shared::item_id::DARKSTEEL).copied().unwrap_or(0) == 0 {
        return Ok(false);
    }
    eu.prog.refinando = Some(slot);
    ws.send(envia(&ClientMessage::Refinar {
        alvo: shared::protocol::AlvoDaForja::Equipado(slot),
    })?)
    .await?;
    t.registra(ev(nome, eu, "pediu_refino", true, format!("{slot:?} +{nivel} -> +{}", nivel + 1)));
    Ok(true)
}

/// O resultado do refino de equipamento vestido (chamado de `recebe`).
pub fn refino_respondido(eu: &mut Eu, t: &Trilha, nome: &str, resultado: u8, nivel: u8, motivo: &str) {
    use shared::forja::resultado as r;
    let Some(slot) = eu.prog.refinando.take() else {
        return;
    };
    match resultado {
        r::SUBIU | r::FALHOU | r::DESTRUIU => {
            if nivel >= REFINO_SEGURO {
                eu.prog.refino_pronto.insert(slot as u8);
            }
            t.registra(ev(nome, eu, "refinou_equipado", true, format!("{slot:?} -> +{nivel} ({resultado})")));
        }
        _ => {
            // Sem material é passageiro: a forja espera. Outra recusa (topo,
            // peça sem instância) é deste slot: larga ele nesta vida.
            if motivo.to_lowercase().contains("falt") || motivo.to_lowercase().contains("material") {
                eu.prog.forja_seca_ate = Some(Instant::now() + FORJA_SECA);
            } else {
                eu.prog.refino_pronto.insert(slot as u8);
            }
            t.registra(ev(nome, eu, "refino_recusado", false, format!("{slot:?}: {motivo}")));
        }
    }
}

/// Fabrica a peça da faixa que melhora o que está vestido.
async fn fabrica(ws: &mut Ws, eu: &mut Eu, nome: &str, t: &Trilha) -> anyhow::Result<bool> {
    let candidata = eu
        .receitas
        .iter()
        .filter(|r| r.roll_instance && r.nivel_min as u32 <= eu.nivel.max(1))
        .filter(|r| !eu.prog.receita_seca.contains(&r.id))
        .filter_map(|r| {
            let slot = shared::constants::equip_slot_of(r.output_item_id)?;
            if !cabe_no_slot(eu, r.output_item_id, slot) {
                return None;
            }
            let atual = eu.prog.equip.get_inst(slot).map_or(0, |i| i.item_level);
            // Só vale se a peça sai num nível de item acima do vestido.
            (r.output_item_level > atual).then_some((r, slot))
        })
        .max_by_key(|(r, _)| r.output_item_level)
        .map(|(r, slot)| (r.id, r.inputs.clone(), r.output_item_id, slot));
    let Some((id, inputs, saida, slot)) = candidata else {
        return Ok(false);
    };
    let faltando: Vec<(u16, u32)> = inputs
        .iter()
        .filter_map(|[item, q]| {
            let item = *item as u16;
            let tem = eu.bolsa.get(&item).copied().unwrap_or(0);
            (tem < *q).then_some((item, *q))
        })
        .collect();
    if faltando.is_empty() {
        ws.send(envia(&ClientMessage::Craft { recipe_id: id })?).await?;
        t.registra(ev(nome, eu, "fabricou_equipamento", true, format!(
            "receita {id} -> item {saida} ({slot:?})"
        )));
        return Ok(true);
    }
    // FALTA MATERIAL: o que sai de coleta ele vai buscar (o ramo 4.5 do
    // `decide` já sabe colher até ter); o resto, larga a receita nesta vida.
    let colhivel = faltando
        .iter()
        .find(|(item, _)| eu.fontes.get(item).is_some_and(|f| !f.is_empty()));
    match colhivel {
        Some((item, q)) if eu.buscando.is_none() => {
            eu.buscando = Some((0, *item, *q));
            t.registra(ev(nome, eu, "foi_buscar_pra_forja", true, format!(
                "receita {id}: {q}x item {item} (tem {})",
                eu.bolsa.get(item).copied().unwrap_or(0)
            )));
            Ok(true)
        }
        Some(_) => Ok(false),
        None => {
            eu.prog.receita_seca.insert(id);
            t.registra(ev(nome, eu, "receita_sem_caminho", false, format!(
                "receita {id}: falta {:?}, que não sai de coleta", faltando
            )));
            Ok(false)
        }
    }
}

/// Ilha Mágica: pergunta de tempos em tempos e entra com as grátis do dia.
async fn magica(ws: &mut Ws, eu: &mut Eu, nome: &str, t: &Trilha) -> anyhow::Result<bool> {
    let agora = Instant::now();
    if eu.nivel < 15 {
        return Ok(false);
    }
    if eu.prog.perguntou_magica_em.is_none_or(|t| agora.duration_since(t) > MAGICA_A_CADA) {
        eu.prog.perguntou_magica_em = Some(agora);
        ws.send(envia(&ClientMessage::Magica {
            pedido: shared::magica::PedidoMagica::Painel,
        })?)
        .await?;
        return Ok(true);
    }
    let Some(m) = eu.prog.magica else {
        return Ok(false);
    };
    if m.dentro || m.grau_maximo == 0 || (m.gratis == 0 && m.passes == 0) {
        return Ok(false);
    }
    if eu.prog.pediu_magica_em.is_some_and(|t| agora.duration_since(t) < MAGICA_A_CADA) {
        return Ok(false);
    }
    eu.prog.pediu_magica_em = Some(agora);
    // As grátis primeiro; passe só se não houver grátis — o passe é do
    // jogador, e o bot gasta só um de cada vez.
    let entradas = if m.gratis > 0 { m.gratis.min(shared::magica::ENTRADAS_MAX) } else { 1 };
    ws.send(envia(&ClientMessage::Magica {
        pedido: shared::magica::PedidoMagica::Entrar { entradas, grau: 0 },
    })?)
    .await?;
    t.registra(ev(nome, eu, "pediu_ilha_magica", true, format!(
        "{entradas} entrada(s), degrau até {} ({} grátis, {} passes)",
        m.grau_maximo, m.gratis, m.passes
    )));
    Ok(true)
}

/// Caça na zona da faixa; se a ilha ficou pequena, vai embarcar.
async fn caca_ou_embarca(ws: &mut Ws, eu: &mut Eu, nome: &str, t: &Trilha) -> anyhow::Result<bool> {
    let agora = Instant::now();
    // EMBARCANDO: conduz as etapas.
    if let Some(Plano::Embarcar { etapa, desde }) = eu.prog.plano {
        if agora.duration_since(desde) > PRAZO_DA_IDA * 2 {
            eu.prog.plano = None;
            eu.prog.embarque_falhou_em = Some(agora);
            t.registra(ev(nome, eu, "embarque_desistiu", false, format!("parou em {etapa:?}")));
            return Ok(false);
        }
        return embarca(ws, eu, nome, t, etapa, desde).await;
    }
    // A ILHA FICOU PEQUENA: embarca, se o menu já deu (ou dará) destino.
    let pode_tentar_embarque = eu
        .prog
        .embarque_falhou_em
        .is_none_or(|t| agora.duration_since(t) > Duration::from_secs(900));
    if ilha_pequena(&eu.prog.zonas, eu.nivel) && pode_tentar_embarque && !eu.na_arena {
        eu.prog.plano = Some(Plano::Embarcar { etapa: Etapa::ACidade, desde: agora });
        t.registra(ev(nome, eu, "vai_embarcar", true, format!(
            "nível {} e a ilha vai até {}",
            eu.nivel,
            eu.prog.zonas.iter().map(|z| z.lv_max).max().unwrap_or(0)
        )));
        return Ok(true);
    }
    // A ZONA DA FAIXA.
    let Some(z) = zona_da_faixa(&eu.prog.zonas, eu.nivel, eu.pos).cloned() else {
        return Ok(false);
    };
    let centro = glam::Vec2::from(z.centro);
    let mesma = matches!(eu.prog.plano, Some(Plano::Cacar { centro: c, .. }) if c.distance(centro) < 1.0);
    if !mesma {
        eu.prog.plano = Some(Plano::Cacar { centro, raio: z.raio, lv: (z.lv_min, z.lv_max), desde: agora });
        eu.prog.rondando = None;
        eu.indo_ao_alvo = None;
        t.registra(ev(nome, eu, "zona_da_faixa", true, format!(
            "nv {}–{} em {:.0},{:.0}{} (a {:.0}u)",
            z.lv_min, z.lv_max, centro.x, centro.y,
            if z.forte { " FORTE" } else { "" },
            eu.pos.distance(centro)
        )));
        ws.send(envia(&ClientMessage::MoverPara { x: centro.x, z: centro.y })?).await?;
        eu.prog.rondando = Some((centro, agora));
        return Ok(true);
    }
    let dentro = eu.pos.distance(centro) <= z.raio;
    // Dentro da zona com bicho perto: quem bate é o ramo 10 do `decide`.
    if dentro && super::alvo_util(eu).is_some() {
        eu.prog.rondando = None;
        return Ok(false);
    }
    // Andando pra um ponto da zona: espera chegar (ou o prazo).
    if let Some((p, desde)) = eu.prog.rondando {
        let chegou = eu.pos.distance(p) <= 6.0;
        if !chegou && agora.duration_since(desde) < Duration::from_secs(25) {
            // Fora da zona e sem sair do lugar há muito: a zona não se alcança.
            if !dentro && agora.duration_since(desde) >= Duration::from_secs(24) {
                if let Some(Plano::Cacar { desde: d0, .. }) = eu.prog.plano {
                    if agora.duration_since(d0) > PRAZO_DA_IDA {
                        eu.prog.plano = None;
                        eu.prog.zonas.retain(|x| glam::Vec2::from(x.centro).distance(centro) > 1.0);
                        t.registra(ev(nome, eu, "zona_inalcancavel", false, format!(
                            "{:.0},{:.0} — tirada da lista nesta vida", centro.x, centro.y
                        )));
                    }
                }
            }
            return Ok(super::alvo_util(eu).is_none());
        }
    }
    // RONDA: um ponto dentro da zona, girando pelo ângulo de ouro.
    eu.vagou = eu.vagou.wrapping_add(1);
    let a = eu.vagou as f32 * 2.399_963;
    let r = z.raio * 0.6 * (((eu.vagou % 5) + 1) as f32 / 5.0).sqrt();
    let p = centro + glam::Vec2::new(a.cos(), a.sin()) * r;
    eu.prog.rondando = Some((p, agora));
    ws.send(envia(&ClientMessage::MoverPara { x: p.x, z: p.y })?).await?;
    Ok(true)
}

/// As etapas do embarque.
async fn embarca(
    ws: &mut Ws,
    eu: &mut Eu,
    nome: &str,
    t: &Trilha,
    etapa: Etapa,
    desde: Instant,
) -> anyhow::Result<bool> {
    let capitao = shared::construcao::Papel::Estaleiro as u8;
    let visto = eu
        .prog
        .npc_kind
        .iter()
        .filter(|(_, k)| shared::npc_papel_de_kind(**k) == capitao)
        .filter_map(|(id, _)| eu.posicoes.get(id).map(|p| (*id, *p)))
        .min_by(|a, b| a.1.distance_squared(eu.pos).total_cmp(&b.1.distance_squared(eu.pos)));
    match etapa {
        Etapa::ACidade | Etapa::AoCapitao => {
            let Some((id, p)) = visto else {
                // Ainda não viu o Capitão: vai à praça, onde o cais fica.
                let Some(cidade) = eu.prog.cidade else {
                    eu.prog.plano = None;
                    return Ok(false);
                };
                if etapa == Etapa::ACidade && eu.andando_para.is_none_or(|(a, _, _)| a.distance(cidade) > 1.0) {
                    ws.send(envia(&ClientMessage::MoverPara { x: cidade.x, z: cidade.y })?).await?;
                    eu.andando_para = Some((cidade, eu.pos, 0));
                    t.registra(ev(nome, eu, "indo_ao_porto", true, format!(
                        "praça em {:.0},{:.0} (a {:.0}u)", cidade.x, cidade.y, eu.pos.distance(cidade)
                    )));
                }
                // Na praça e sem Capitão à vista: ronda o cais em volta.
                if eu.pos.distance(cidade) < 12.0 {
                    eu.vagou = eu.vagou.wrapping_add(1);
                    let a = eu.vagou as f32 * 2.399_963;
                    let q = cidade + glam::Vec2::new(a.cos(), a.sin()) * 25.0;
                    ws.send(envia(&ClientMessage::MoverPara { x: q.x, z: q.y })?).await?;
                    eu.andando_para = Some((q, eu.pos, 0));
                }
                return Ok(true);
            };
            let d = eu.pos.distance(p);
            if d > shared::viagem::PERTO_DO_CAPITAO - 0.5 {
                if d <= 20.0 {
                    if !super::empurra(eu, p) {
                        ws.send(envia(&ClientMessage::MoverPara { x: p.x, z: p.y })?).await?;
                    }
                } else if eu.andando_para.is_none_or(|(a, _, _)| a.distance(p) > 1.0) {
                    ws.send(envia(&ClientMessage::MoverPara { x: p.x, z: p.y })?).await?;
                    eu.andando_para = Some((p, eu.pos, 0));
                }
                eu.prog.plano = Some(Plano::Embarcar { etapa: Etapa::AoCapitao, desde });
                return Ok(true);
            }
            eu.empurrao = None;
            eu.empurrao_ate = None;
            eu.prog.destinos.clear();
            ws.send(envia(&ClientMessage::Interact { target_eid: Some(id.0 as u64) })?).await?;
            eu.prog.plano = Some(Plano::Embarcar { etapa: Etapa::Menu, desde });
            t.registra(ev(nome, eu, "falou_com_capitao", true, format!("a {d:.1}u")));
            Ok(true)
        }
        Etapa::Menu => {
            if eu.prog.destinos.is_empty() {
                // O menu ainda não chegou; o prazo geral cuida de desistir.
                return Ok(true);
            }
            match destino_pro_nivel(&eu.prog.destinos, eu.nivel) {
                Some(ilha) => {
                    ws.send(envia(&ClientMessage::Viajar { ilha })?).await?;
                    t.registra(ev(nome, eu, "embarcou", true, format!(
                        "ilha {ilha} no nível {}", eu.nivel
                    )));
                    eu.prog.plano = None;
                    // Se o embarque não trocar de zona, não tenta de novo
                    // tão cedo — a trilha diz por quê (`ultima_recusa`).
                    eu.prog.embarque_falhou_em = Some(Instant::now());
                }
                None => {
                    let porque = eu
                        .prog
                        .destinos
                        .iter()
                        .map(|d| format!("{}={}{}", d.nome, d.estado, if d.requisito.is_empty() { String::new() } else { format!(" ({})", d.requisito) }))
                        .collect::<Vec<_>>()
                        .join(", ");
                    eu.prog.plano = None;
                    eu.prog.embarque_falhou_em = Some(Instant::now());
                    t.registra(ev(nome, eu, "sem_ilha_pro_nivel", false, porque));
                }
            }
            Ok(true)
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn z(lv_min: u16, lv_max: u16, forte: bool, x: f32) -> ZonaNoMapa {
        ZonaNoMapa { centro: [x, 0.0], raio: 30.0, lv_min, lv_max, bichos: vec![], forte }
    }

    #[test]
    fn a_zona_da_faixa_e_a_mais_alta_que_ele_aguenta() {
        let zonas = [z(1, 3, false, 0.0), z(4, 6, false, 100.0), z(7, 9, false, 200.0), z(7, 9, true, 300.0)];
        assert_eq!(zona_da_faixa(&zonas, 5, glam::Vec2::ZERO).unwrap().lv_min, 4);
        assert_eq!(zona_da_faixa(&zonas, 6, glam::Vec2::ZERO).unwrap().lv_min, 7);
        // Forte só depois de passar da faixa dele.
        let z10 = zona_da_faixa(&zonas, 10, glam::Vec2::new(300.0, 0.0)).unwrap();
        assert!(z10.forte, "no 10 o forte de 7-9 já é caça");
        assert!(!zona_da_faixa(&zonas, 8, glam::Vec2::new(300.0, 0.0)).unwrap().forte);
        assert!(zona_da_faixa(&zonas, 14, glam::Vec2::ZERO).is_none());
        assert!(ilha_pequena(&zonas, 14) && !ilha_pequena(&zonas, 11));
    }

    #[test]
    fn os_pontos_seguem_a_build_de_referencia() {
        let mut a = [0u32; shared::STAT_COUNT];
        let arma = shared::item_id::PISTOLAS;
        for _ in 0..30 {
            let s = proximo_ponto(arma, &a) as usize;
            a[s] += 1;
        }
        assert_eq!(a[shared::stat_idx::DES], 10);
        assert_eq!(a[shared::stat_idx::VIT], 20);
        assert_eq!(atributo_da_classe(shared::item_id::ANEL_MAGICO), shared::stat_idx::INT);
    }

    #[test]
    fn embarca_pra_ilha_liberada_de_maior_faixa_que_aceita() {
        use shared::viagem::{estado, Destino};
        let d = |ilha, min, e| Destino { ilha, nome: format!("{ilha}"), nivel_min: min, nivel_max: min + 15, estado: e, requisito: String::new() };
        let ds = [d(0, 1, estado::AQUI), d(1, 15, estado::LIBERADA), d(2, 28, estado::LIBERADA), d(3, 40, estado::BLOQUEADA)];
        assert_eq!(destino_pro_nivel(&ds, 20), Some(1));
        assert_eq!(destino_pro_nivel(&ds, 30), Some(2));
        assert_eq!(destino_pro_nivel(&ds, 45), Some(2), "bloqueada não conta");
        assert_eq!(destino_pro_nivel(&ds, 10), None);
    }
}
