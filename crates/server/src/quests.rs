//! Quests — persistência por personagem + helpers de elegibilidade.
//!
//! As DEFINIÇÕES de quest vivem em `shared::quests::QUESTS` (estáticas, igual
//! ao começo do sistema de recipes). Aqui só persistimos o ESTADO por
//! personagem (`character_quests`) + os pontos de facção (`characters.faction_points`).
//! Hot-reload de defs via DB pode vir depois (padrão recipes/economy).

use shared::historia;
use shared::quests::{self, QuestDef};
use sqlx::PgPool;

/// Estado de uma quest para um personagem (linha em `character_quests`).
#[derive(Debug, Clone)]
pub struct CharQuest {
    pub quest_id: u16,
    pub status: u8, // shared::quests::quest_status
    pub progress: u32,
    pub cooldown_until: i64, // unix secs; 0 = sem cooldown
}

/// Cria as tabelas/colunas de quest se não existirem.
pub async fn init(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS character_quests (
            char_name      TEXT NOT NULL,
            quest_id       INT  NOT NULL,
            status         SMALLINT NOT NULL DEFAULT 0,
            progress       INT  NOT NULL DEFAULT 0,
            cooldown_until BIGINT NOT NULL DEFAULT 0,
            PRIMARY KEY (char_name, quest_id)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cquests_char ON character_quests(char_name)")
        .execute(pool)
        .await?;
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS faction_points INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await?;
    tracing::info!(
        "[quests] schema ready ({} static defs)",
        quests::QUESTS.len()
    );
    Ok(())
}

/// Carrega o estado de quests + pontos de facção de um personagem.
pub async fn load_char(pool: &PgPool, char_name: &str) -> anyhow::Result<(Vec<CharQuest>, u32)> {
    let rows: Vec<(i32, i16, i32, i64)> = sqlx::query_as(
        "SELECT quest_id, status, progress, cooldown_until FROM character_quests WHERE char_name = $1"
    ).bind(char_name).fetch_all(pool).await?;
    let quests = rows
        .into_iter()
        .map(|(qid, st, pr, cd)| CharQuest {
            quest_id: qid as u16,
            status: st as u8,
            progress: pr.max(0) as u32,
            cooldown_until: cd,
        })
        .collect();
    let fp: i32 = sqlx::query_scalar("SELECT faction_points FROM characters WHERE name = $1")
        .bind(char_name)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);
    Ok((quests, fp.max(0) as u32))
}

/// Persiste o estado completo de quests + pontos de facção de um personagem.
pub async fn save_char(
    pool: &PgPool,
    char_name: &str,
    quests: &[CharQuest],
    faction_points: u32,
) -> anyhow::Result<()> {
    // Reconcilia: apaga tudo do char e reinsere o estado atual. Assim quests
    // abandonadas (removidas da memória) somem do DB sem lista de deleção.
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM character_quests WHERE char_name = $1")
        .bind(char_name)
        .execute(&mut *tx)
        .await?;
    for q in quests {
        sqlx::query(
            "INSERT INTO character_quests (char_name, quest_id, status, progress, cooldown_until)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(char_name)
        .bind(q.quest_id as i32)
        .bind(q.status as i16)
        .bind(q.progress as i32)
        .bind(q.cooldown_until)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE characters SET faction_points = $2 WHERE name = $1")
        .bind(char_name)
        .bind(faction_points as i32)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Givers (ids) que têm ao menos uma quest aceitável agora pra este player.
/// Mesmos critérios do `offerable` (level, facção, cadeia, cooldown/estado).
pub fn available_givers(
    level: u32,
    faction: u8,
    active: &[CharQuest],
    now: i64,
    zona: &str,
) -> Vec<u16> {
    let mut set: Vec<u16> = Vec::new();
    for d in quests::QUESTS.iter() {
        if na_zona(d, zona)
            && pode_aceitar(d, level, faction, active, now)
            && aparece_no_npc(d)
            && !set.contains(&d.giver)
        {
            set.push(d.giver);
        }
    }
    set
}

/// Missoes comuns comecam pelo menu. O NPC oferece apenas a proxima de uma
/// cadeia ja' iniciada, depois que o requisito foi entregue.
pub fn aparece_no_npc(d: &QuestDef) -> bool {
    d.source != quests::quest_source::NPC || d.requires != 0
}

/// Quests que `giver` (source+id) oferece e o player PODE aceitar agora.
pub fn offerable<'a>(
    giver_source: u8,
    giver_id: u16,
    level: u32,
    faction: u8,
    active: &[CharQuest],
    now: i64,
    zona: &str,
) -> Vec<&'a QuestDef> {
    quests::QUESTS
        .iter()
        .filter(|d| d.source == giver_source && d.giver == giver_id && na_zona(d, zona))
        .filter(|d| pode_aceitar(d, level, faction, active, now))
        .collect()
}

/// A missão pré-requisito (se houver) já foi ENTREGUE?
pub fn requisito_ok(d: &QuestDef, active: &[CharQuest]) -> bool {
    d.requires == 0
        || active
            .iter()
            .any(|c| c.quest_id == d.requires && c.status == quests::quest_status::TURNED_IN)
}

/// Pode aceitar `d` agora? A MESMA regra da oferta: o servidor não aceita o
/// que não ofereceria — nível, facção, cadeia, estado atual e cooldown.
pub fn pode_aceitar(d: &QuestDef, level: u32, faction: u8, active: &[CharQuest], now: i64) -> bool {
    // Sistema que ainda nao existe: no catalogo, com cadeado, e so'.
    if d.em_breve {
        return false;
    }
    // A historia ninguem oferece: o servidor da' o passo sozinho.
    if d.source == quests::quest_source::HISTORIA {
        return false;
    }
    if level < d.min_level {
        return false;
    }
    if d.faction != quests::faction_id::NONE && d.faction != faction {
        return false;
    }
    if !requisito_ok(d, active) {
        return false;
    }
    match active.iter().find(|c| c.quest_id == d.id) {
        None => true, // nunca pegou
        // Ativa/pronta → não reoferece. Concluída → só se repetível e fora do cooldown.
        Some(c) => {
            c.status == quests::quest_status::TURNED_IN && d.repeatable && now >= c.cooldown_until
        }
    }
}

/// A missao pertence a esta ilha? As da cadeia e as diarias moram numa zona
/// (`quests::zona_da_missao`); o Mestre de uma ilha nao oferece as da outra.
/// Legado sem zona continua valendo onde o giver dele existir.
pub fn na_zona(d: &QuestDef, zona: &str) -> bool {
    quests::zona_da_missao(d.id).is_none_or(|z| z == zona)
}

/// Does an act HERE count for `d`? Side quests can be taken from any island
/// (the menu, "like MIR4"), but their objective in the world — hunting,
/// gathering, exploring — is that island's: a Glacier archer quest does not
/// fill up with Bosque archers. TALKING counts on any island: every village
/// has the same people, and the auto quest walks to the local one — counting
/// only the quest's island looped it on the local Alchemist forever (owner,
/// 04/10/2026). Items (COLLECT/DELIVER), craft, forge and dungeon count
/// anywhere, and the story keeps its own flow.
pub fn conta_nesta_ilha(d: &QuestDef, zona: &str) -> bool {
    use quests::objective_kind as o;
    d.source == quests::quest_source::HISTORIA
        || !matches!(d.obj_kind, o::KILL | o::GATHER | o::EXPLORE)
        || na_zona(d, zona)
}

/// Mudanças de uma missão: (quest_id, progresso, status).
pub type Mudancas = Vec<(u16, u32, u8)>;

// ─────────────────────────── historia ───────────────────────────

/// Indice do passo atual da historia (a linha marcadora), se ja' comecou.
pub fn indice_da_historia(active: &[CharQuest]) -> Option<u32> {
    active
        .iter()
        .find(|c| c.quest_id == historia::ID_MARCO)
        .map(|c| c.progress)
}

/// A linha do passo da historia em andamento.
pub fn passo_atual(active: &[CharQuest]) -> Option<&CharQuest> {
    let id = historia::id_do_passo(indice_da_historia(active)?)?;
    active.iter().find(|c| c.quest_id == id)
}

/// Sem historia (personagem novo ou de antes dela): marca o indice 0 e da' o
/// primeiro passo. Repoe a linha do passo atual se ela sumiu. Devolve o id
/// dado, se deu.
pub fn garantir_historia(active: &mut Vec<CharQuest>) -> Option<u16> {
    // PASSO FANTASMA: um id que nao e' mais passo da historia trava o jogador
    // pra sempre — `quest_by_id` devolve `None`, nada avanca, e nada reclama.
    //
    // Aconteceu de verdade em 21/09/2026: a historia foi renumerada e dois
    // personagens ficaram apontando pros ids 775 e 776, que deixaram de
    // existir. A migracao `historia_renumerada_v1` conserta os que ja' estao
    // no banco; isto aqui e' pra proxima vez, porque vai haver proxima vez —
    // a tabela de passos e' codigo e o progresso e' dado, e eles se separam.
    //
    // A saida e' o MARCADOR, que guarda o INDICE e nao o id.
    active.retain(|c| {
        let fantasma = c.quest_id != historia::ID_MARCO
            && c.quest_id >= historia::PRIMEIRO_ID
            && historia::indice(c.quest_id).is_none();
        if fantasma {
            tracing::warn!(
                "step {} no longer exists in the story — discarded; the marker takes over",
                c.quest_id
            );
        }
        !fantasma
    });
    // O marcador guarda o INDICE; o passo em andamento guarda o ID. Se a
    // historia ganhou passo no meio (os tutoriais 770+, em 19/09/2026), o
    // indice salvo aponta pra outro passo — o id em andamento e' quem manda,
    // e o marcador se realinha a ele.
    let em_andamento = active
        .iter()
        .find(|c| {
            c.quest_id != historia::ID_MARCO
                && historia::e_da_historia(c.quest_id)
                && c.status != quests::quest_status::TURNED_IN
        })
        .and_then(|c| historia::indice(c.quest_id));
    if let (Some(certo), Some(m)) = (
        em_andamento,
        active.iter_mut().find(|c| c.quest_id == historia::ID_MARCO),
    ) {
        m.progress = certo;
    }
    let i = match indice_da_historia(active) {
        Some(i) => i,
        None => {
            active.push(CharQuest {
                quest_id: historia::ID_MARCO,
                status: historia::STATUS_MARCO,
                progress: 0,
                cooldown_until: 0,
            });
            0
        }
    };
    let id = historia::id_do_passo(i)?;
    if active.iter().any(|c| c.quest_id == id) {
        return None;
    }
    active.push(CharQuest {
        quest_id: id,
        status: quests::quest_status::ACTIVE,
        progress: 0,
        cooldown_until: 0,
    });
    Some(id)
}

/// O passo atual esta' PRONTO: sai do log (concluido nao vira linha), o indice
/// anda e o proximo entra ativo. Devolve (o que concluiu, o proximo).
pub fn avancar_historia(active: &mut Vec<CharQuest>) -> Option<(&'static QuestDef, Option<u16>)> {
    let i = indice_da_historia(active)?;
    let id = historia::id_do_passo(i)?;
    let pos = active
        .iter()
        .position(|c| c.quest_id == id && c.status == quests::quest_status::READY)?;
    active.remove(pos);
    let def = quests::quest_by_id(id)?;
    if let Some(m) = active.iter_mut().find(|c| c.quest_id == historia::ID_MARCO) {
        m.progress = i + 1;
    }
    let prox = historia::id_do_passo(i + 1);
    if let Some(p) = prox {
        active.push(CharQuest {
            quest_id: p,
            status: quests::quest_status::ACTIVE,
            progress: 0,
            cooldown_until: 0,
        });
    }
    Some((def, prox))
}

/// Trava de nivel: o progresso acompanha o nivel; chegou, fica pronta.
pub fn checar_trava(active: &mut [CharQuest], nivel: u32) -> Mudancas {
    let mut mudou = Vec::new();
    for c in active.iter_mut() {
        if c.status != quests::quest_status::ACTIVE {
            continue;
        }
        let Some(def) = quests::quest_by_id(c.quest_id) else {
            continue;
        };
        if def.obj_kind != quests::objective_kind::NIVEL {
            continue;
        }
        let p = nivel.min(def.obj_count);
        if p == c.progress && p < def.obj_count {
            continue;
        }
        c.progress = p;
        if p >= def.obj_count {
            c.status = quests::quest_status::READY;
        }
        mudou.push((c.quest_id, c.progress, c.status));
    }
    mudou
}

/// Passo ativo de LUGAR/VIAGEM ficou cumprido: pronto.
pub fn cumprir_passo(active: &mut [CharQuest], id: u16) -> Mudancas {
    let mut mudou = Vec::new();
    if let Some(c) = active
        .iter_mut()
        .find(|c| c.quest_id == id && c.status == quests::quest_status::ACTIVE)
    {
        c.progress = quests::quest_by_id(id).map_or(1, |d| d.obj_count.max(1));
        c.status = quests::quest_status::READY;
        mudou.push((c.quest_id, c.progress, c.status));
    }
    mudou
}

/// Um evento que as missoes "live-track" contam (coletar, criar, refinar...):
/// avanca `qtd` em toda ATIVA do tipo `kind` cujo alvo `conta` aceitar.
pub fn avancar_evento(
    active: &mut [CharQuest],
    kind: u8,
    conta: &dyn Fn(&QuestDef) -> bool,
    qtd: u32,
) -> Mudancas {
    let mut mudou = Vec::new();
    for c in active.iter_mut() {
        if c.status != quests::quest_status::ACTIVE {
            continue;
        }
        let Some(def) = quests::quest_by_id(c.quest_id) else {
            continue;
        };
        if def.obj_kind != kind || !conta(def) {
            continue;
        }
        c.progress = (c.progress + qtd).min(def.obj_count);
        if c.progress >= def.obj_count {
            c.status = quests::quest_status::READY;
        }
        mudou.push((c.quest_id, c.progress, c.status));
    }
    mudou
}

/// Diarias aceitas e nao entregues ate' a meia-noite UTC em que foram aceitas
/// expiram: saem do log. `cooldown_until` de uma diaria ATIVA guarda o fim do
/// dia dela (posto no aceite). Devolve os ids que sairam.
pub fn expirar_diarias(active: &mut Vec<CharQuest>, now: i64) -> Vec<u16> {
    let mut sairam = Vec::new();
    active.retain(|c| {
        let diaria = quests::quest_by_id(c.quest_id).is_some_and(|d| d.daily);
        let vencida = diaria
            && c.status != quests::quest_status::TURNED_IN
            && c.cooldown_until > 0
            && now >= c.cooldown_until;
        if vencida {
            sairam.push(c.quest_id);
        }
        !vencida
    });
    sairam
}

/// Morreu um mob de `mob_kind` (ou um jogador da facção `pvp_victim_faction`):
/// avança KILL/PVP_KILL. KILL com `obj_target` 0 aceita qualquer mob; senão
/// só o kind de `quests::alvo_de_mob`.
pub fn avancar_kill(
    active: &mut [CharQuest],
    mob_kind: Option<u16>,
    pvp_victim_faction: Option<u8>,
) -> Mudancas {
    avancar_kill_com_chefe(active, mob_kind, pvp_victim_faction, false)
}

/// O `kind` de mob comum pode coincidir com o de um chefe. O servidor passa
/// o marcador da entidade para a missao de chefe contar so' chefe de verdade.
pub fn avancar_kill_com_chefe(
    active: &mut [CharQuest],
    mob_kind: Option<u16>,
    pvp_victim_faction: Option<u8>,
    e_chefe: bool,
) -> Mudancas {
    avancar_kill_na_ilha(active, mob_kind, pvp_victim_faction, e_chefe, None)
}

/// `avancar_kill_com_chefe` on island `zona` (`conta_nesta_ilha`).
pub fn avancar_kill_na_ilha(
    active: &mut [CharQuest],
    mob_kind: Option<u16>,
    pvp_victim_faction: Option<u8>,
    e_chefe: bool,
    zona: Option<&str>,
) -> Mudancas {
    let mut mudou = Vec::new();
    for c in active.iter_mut() {
        if c.status != quests::quest_status::ACTIVE {
            continue;
        }
        let Some(def) = quests::quest_by_id(c.quest_id) else {
            continue;
        };
        if zona.is_some_and(|z| !conta_nesta_ilha(def, z)) {
            continue;
        }
        let hit = if def.obj_kind == quests::objective_kind::KILL {
            pvp_victim_faction.is_none()
                && (def.obj_target == 0
                    || (def.obj_target == quests::ALVO_QUALQUER_CHEFE && e_chefe)
                    || (def.obj_target != quests::ALVO_QUALQUER_CHEFE
                        && mob_kind.is_some_and(|k| {
                            // An island VARIANT counts for its species: the
                            // Glacier's Frostcoat Archer is an archer for
                            // "Defeat 8 archers" (`bestiary::species_of`).
                            // Comparing the raw kind made every variant kill
                            // count for nothing on the Glacier, Waste and
                            // Plateau since the variants (01/10/2026).
                            quests::alvo_de_mob(k) == def.obj_target
                                || quests::alvo_de_mob(shared::bestiary::species_of(k))
                                    == def.obj_target
                        })))
        } else if def.obj_kind == quests::objective_kind::PVP_KILL {
            pvp_victim_faction
                .map(|vf| vf != def.faction)
                .unwrap_or(false)
        } else {
            false
        };
        if !hit {
            continue;
        }
        c.progress = (c.progress + 1).min(def.obj_count);
        if c.progress >= def.obj_count {
            c.status = quests::quest_status::READY;
        }
        mudou.push((c.quest_id, c.progress, c.status));
    }
    mudou
}

/// Conversou com o NPC da vila de `papel`: as TALK dele ficam prontas.
pub fn avancar_conversa(active: &mut [CharQuest], papel: u16) -> Mudancas {
    avancar_conversa_na_ilha(active, papel, None)
}

/// `avancar_conversa` on island `zona` (`conta_nesta_ilha`).
pub fn avancar_conversa_na_ilha(active: &mut [CharQuest], papel: u16, zona: Option<&str>) -> Mudancas {
    let mut mudou = Vec::new();
    for c in active.iter_mut() {
        if c.status != quests::quest_status::ACTIVE {
            continue;
        }
        let Some(def) = quests::quest_by_id(c.quest_id) else {
            continue;
        };
        if zona.is_some_and(|z| !conta_nesta_ilha(def, z)) {
            continue;
        }
        if def.obj_kind != quests::objective_kind::TALK || def.obj_target != papel {
            continue;
        }
        c.progress = def.obj_count;
        c.status = quests::quest_status::READY;
        mudou.push((c.quest_id, c.progress, c.status));
    }
    mudou
}

/// Dá pra entregar? `tem` = quantos do item-alvo o jogador carrega. `Ok(n)` =
/// entrega consumindo `n` itens (0 pra objetivo que não é item); `Err` = o
/// motivo, pra dizer ao jogador em vez de ignorar o clique.
pub fn checar_entrega(d: &QuestDef, c: &CharQuest, tem: u32) -> Result<u32, String> {
    if c.status == quests::quest_status::TURNED_IN {
        return Err("quest already turned in".into());
    }
    if d.obj_kind == quests::objective_kind::COLLECT
        || d.obj_kind == quests::objective_kind::DELIVER
    {
        if tem < d.obj_count {
            return Err(format!("{} of {} short", d.obj_count - tem, d.obj_count));
        }
        return Ok(d.obj_count);
    }
    if c.status != quests::quest_status::READY {
        return Err("the objective has not been met yet".into());
    }
    Ok(0)
}

// ─────────────────────── destino da auto missao ───────────────────────

/// Ate' onde, do NPC, terminar o dialogo conta como conversa. Um pouco alem do
/// alcance do `Interact`: o jogador pode ter dado um passo lendo as falas.
pub const ALCANCE_DA_CONVERSA: f32 = shared::INTERACT_RADIUS + 1.5;

pub fn pode_concluir_conversa(distancia: f32) -> bool {
    distancia <= ALCANCE_DA_CONVERSA
}

/// Chance de `kind` sair numa zona de nivel `lv_min..=lv_max`, pela regra de
/// `economy::kind_para_nivel`: no nivel `n` sorteia uniforme entre os
/// `n/3 + 1` primeiros kinds (`kinds` ja' na ordem do sorteio).
pub fn chance_do_kind(kinds: &[u16], kind: u16, lv_min: u32, lv_max: u32) -> f32 {
    let Some(i) = kinds.iter().position(|k| *k == kind) else {
        return 0.0;
    };
    if kinds.is_empty() || lv_max < lv_min {
        return 0.0;
    }
    let mut soma = 0.0;
    for n in lv_min..=lv_max {
        // The same draw as the spawn (`economy::sorteio_do_nivel`).
        let (teto, dobra) = crate::economy::sorteio_do_nivel(kinds.len(), n);
        if i < teto {
            let fatias = (teto + dobra as usize) as f32;
            let peso = if dobra && i == teto - 1 { 2.0 } else { 1.0 };
            soma += peso / fatias;
        }
    }
    soma / (lv_max - lv_min + 1) as f32
}

/// Chance minima pra uma zona contar como "onde o bicho nasce".
const CHANCE_MINIMA: f32 = 0.15;

/// Centro da zona de mob pra ir caçar um de `alvos` (vazio = qualquer bicho).
/// `zonas` = (centro, nivel minimo, nivel maximo). Prefere a mais perto em que
/// o bicho sai com chance razoavel e cujo nivel o jogador aguenta; sem nenhuma
/// assim, a de maior chance.
pub fn zona_do_bicho(
    zonas: &[(glam::Vec2, u32, u32)],
    kinds: &[u16],
    alvos: &[u16],
    eu: glam::Vec2,
    nivel: u32,
) -> Option<glam::Vec2> {
    let com_forte: Vec<_> = zonas.iter().map(|z| (z.0, z.1, z.2, false)).collect();
    zona_do_bicho_com_forte(&com_forte, kinds, alvos, eu, nivel)
}

/// How much closer a FORT counts when picking where to hunt: a fort 1/0.6 =
/// 1.67x as far as an ordinary zone still wins. The owner asked on 20/09/2026
/// that kill quests lead "to the spot with more mobs to kill" — twice the
/// density, half the time. Until 01/10/2026 the comment promised it and the
/// code sorted by distance only: a fort won when it happened to be the
/// nearest, and stopped winning when the newest-species weight
/// (`economy::sorteio_do_nivel`) let closer zones qualify. Not unconditional:
/// a fort across the island still loses to the zone next door.
pub const FORTE_ENCURTA: f32 = 0.6;

/// `zona_do_bicho` with each zone tagged as fort or not (`.3`).
pub fn zona_do_bicho_com_forte(
    zonas: &[(glam::Vec2, u32, u32, bool)],
    kinds: &[u16],
    alvos: &[u16],
    eu: glam::Vec2,
    nivel: u32,
) -> Option<glam::Vec2> {
    let chance = |z: &(glam::Vec2, u32, u32, bool)| {
        if alvos.is_empty() {
            1.0
        } else {
            alvos
                .iter()
                .map(|k| chance_do_kind(kinds, *k, z.1, z.2))
                .fold(0.0, f32::max)
        }
    };
    let longe = |z: &(glam::Vec2, u32, u32, bool)| {
        z.0.distance(eu) * if z.3 { FORTE_ENCURTA } else { 1.0 }
    };
    let perto = |v: &mut Vec<&(glam::Vec2, u32, u32, bool)>| {
        v.sort_by(|a, b| longe(a).total_cmp(&longe(b)));
        v.first().map(|z| z.0)
    };
    let mut boas: Vec<_> = zonas
        .iter()
        .filter(|z| chance(z) >= CHANCE_MINIMA && z.1 <= nivel + 3)
        .collect();
    if let Some(p) = perto(&mut boas) {
        return Some(p);
    }
    let mut com_bicho: Vec<_> = zonas
        .iter()
        .filter(|z| chance(z) >= CHANCE_MINIMA)
        .collect();
    if let Some(p) = perto(&mut com_bicho) {
        return Some(p);
    }
    zonas
        .iter()
        .filter(|z| chance(z) > 0.0)
        .max_by(|a, b| {
            chance(a).total_cmp(&chance(b)).then(
                b.0.distance_squared(eu)
                    .total_cmp(&a.0.distance_squared(eu)),
            )
        })
        .map(|z| z.0)
}

/// O melhor spot de coleta entre `corpos` (centro, tier; so' os vivos): o
/// ponto com mais corpos no raio do spot, desempatando pelo mais perto de
/// `eu`. Devolve o CENTRO dos corpos daquele spot e quantos sao.
pub fn melhor_spot(
    corpos: &[(glam::Vec2, u8)],
    eu: glam::Vec2,
    raio: f32,
) -> Option<(glam::Vec2, usize)> {
    spots_ordenados(corpos, eu, raio, 1).into_iter().next()
}

/// Os spots do mais cheio pro menos, sem dois no mesmo veio. Quem escolhe
/// testa o ALCANCE de cada um em ordem: o mais cheio pode estar num cume sem
/// rampa.
pub fn spots_ordenados(
    corpos: &[(glam::Vec2, u8)],
    eu: glam::Vec2,
    raio: f32,
    max: usize,
) -> Vec<(glam::Vec2, usize)> {
    let raio_sq = (raio * 0.9) * (raio * 0.9);
    let mut notas: Vec<(f32, glam::Vec2, usize)> = corpos
        .iter()
        .map(|(c, _)| {
            let perto: Vec<glam::Vec2> = corpos
                .iter()
                .map(|(o, _)| *o)
                .filter(|o| o.distance_squared(*c) <= raio_sq)
                .collect();
            let centro =
                perto.iter().copied().fold(glam::Vec2::ZERO, |a, b| a + b) / perto.len() as f32;
            (
                perto.len() as f32 - c.distance(eu) * 0.01,
                centro,
                perto.len(),
            )
        })
        .collect();
    // Estavel: entre notas iguais fica o primeiro, como antes.
    notas.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut saida: Vec<(glam::Vec2, usize)> = Vec::new();
    for (_, c, n) in notas {
        if saida.iter().any(|(o, _)| o.distance(c) < raio) {
            continue;
        }
        saida.push((c, n));
        if saida.len() >= max {
            break;
        }
    }
    saida
}

/// O corpo mais perto de `eu`. E' por onde a missao de coleta comeca quando
/// nao ha' nada em volta: pedra nasce no alto, longe da cidade.
pub fn mais_perto(corpos: &[(glam::Vec2, u8)], eu: glam::Vec2) -> Option<glam::Vec2> {
    corpos
        .iter()
        .map(|(c, _)| *c)
        .min_by(|a, b| a.distance_squared(eu).total_cmp(&b.distance_squared(eu)))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// "Meet the Alchemist" is a Bosque quest; on the Glacier the auto quest
    /// walks to the Glacier's Alchemist. That talk has to count, or it loops
    /// on the same dialog forever (owner, 04/10/2026).
    #[test]
    fn conversa_conta_em_qualquer_ilha() {
        let d = quests::quest_by_id(501).unwrap();
        assert_eq!(quests::zona_da_missao(501), Some("ilha_inicial"));
        assert!(conta_nesta_ilha(d, "ilha_gelo"));
        let mut ativas = vec![CharQuest { quest_id: 501, status: quests::quest_status::ACTIVE, progress: 0, cooldown_until: 0 }];
        let mudou = avancar_conversa_na_ilha(&mut ativas, shared::construcao::Papel::Alquimista as u16, Some("ilha_abissal"));
        assert_eq!(mudou.len(), 1);
        assert_eq!(ativas[0].status, quests::quest_status::READY);
    }
    use quests::{quest_status::*, GIVER_MESTRE_DA_ILHA};

    #[test]
    fn mais_perto_acha_o_corpo_longe_quando_nao_ha_nada_em_volta() {
        let v = glam::Vec2::new;
        assert_eq!(mais_perto(&[], v(0.0, 0.0)), None);
        let corpos = [(v(900.0, 0.0), 2), (v(-450.0, 30.0), 1), (v(0.0, 700.0), 3)];
        assert_eq!(mais_perto(&corpos, v(0.0, 0.0)), Some(v(-450.0, 30.0)));
    }
    use shared::construcao::Papel;

    fn cq(id: u16, status: u8, progress: u32) -> CharQuest {
        CharQuest {
            quest_id: id,
            status,
            progress,
            cooldown_until: 0,
        }
    }
    fn def(id: u16) -> &'static QuestDef {
        quests::quest_by_id(id).unwrap()
    }

    #[test]
    fn a_cadeia_do_mestre_anda_em_ordem() {
        let src = quests::quest_source::NPC;
        // So' a cadeia (5xx): as diarias do mesmo Mestre tem teste proprio.
        let ids = |a: &[CharQuest], lv| {
            offerable(src, GIVER_MESTRE_DA_ILHA, lv, 0, a, 0, "ilha_inicial")
                .iter()
                .map(|d| d.id)
                .filter(|id| (501..=510).contains(id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&[], 1), vec![501], "personagem novo so' ve' a primeira");
        assert!(!pode_aceitar(def(502), 1, 0, &[], 0), "502 antes da 501");
        let feita = [cq(501, TURNED_IN, 1)];
        assert_eq!(ids(&feita, 1), vec![502]);
        // 504 pede nivel 3.
        let ate_503 = [
            cq(501, TURNED_IN, 1),
            cq(502, TURNED_IN, 6),
            cq(503, TURNED_IN, 30),
        ];
        assert!(ids(&ate_503, 2).is_empty());
        assert_eq!(ids(&ate_503, 3), vec![504]);
        assert!(available_givers(1, 0, &[], 0, "ilha_inicial").contains(&GIVER_MESTRE_DA_ILHA));
    }

    /// O caso que motivou 511-538: nivel 16, capitulo I acabado, cadeia do
    /// Mestre ate' a 507. As seis cadeias novas abrem juntas, mas CADA UMA com
    /// o seu NPC da vila — o Mestre nao oferece mais tudo de uma vez.
    #[test]
    fn no_nivel_16_cada_cadeia_abre_no_seu_npc() {
        use shared::construcao::Papel;
        use shared::quests::giver_do_papel;
        let src = quests::quest_source::NPC;
        let feitas: Vec<CharQuest> = (501..=507).map(|id| cq(id, TURNED_IN, 1)).collect();
        let de = |g: u16| -> Vec<u16> {
            offerable(src, g, 16, 0, &feitas, 0, "ilha_inicial")
                .iter()
                .map(|d| d.id)
                .filter(|id| (500..600).contains(id))
                .collect()
        };
        assert_eq!(
            de(GIVER_MESTRE_DA_ILHA),
            vec![508, 534],
            "o Mestre so' tem as dele"
        );
        // O Treinador ganhou a CACADA (22/09/2026): 539 abre a cadeia de
        // volume e 543 e' a tematica dos lobos, que nao depende de nada.
        assert_eq!(de(giver_do_papel(Papel::Treinador)), vec![539, 543]);
        // As cadeias de exploracao sairam da cidade pras cabanas (25/09/2026):
        // o Guia do Mirante, o Vigia da Clareira e o Lenhador da Trilha.
        assert_eq!(de(203), vec![511]);
        assert_eq!(de(202), vec![516]);
        assert_eq!(de(201), vec![528]);
        // 544 abre a cadeia das chaves de craft (escama, garra, chifre, couro).
        assert_eq!(de(giver_do_papel(Papel::Ferreiro)), vec![523, 544]);
        assert_eq!(de(giver_do_papel(Papel::Estaleiro)), vec![531]);
        // Cadeia de cabana fica na cabana: 511 entregue, a 512 e' do mesmo
        // Guia do Mirante.
        let mut depois = feitas.clone();
        depois.push(cq(511, TURNED_IN, 1));
        let guia = offerable(src, 203, 16, 0, &depois, 0, "ilha_inicial");
        assert!(guia.iter().any(|d| d.id == 512));
        // Nivel 1: so' a primeira da cadeia antiga e a conversa na taberna.
        let novo: Vec<u16> = offerable(src, GIVER_MESTRE_DA_ILHA, 1, 0, &[], 0, "ilha_inicial")
            .iter()
            .map(|d| d.id)
            .filter(|id| (500..600).contains(id))
            .collect();
        assert_eq!(novo, vec![501, 534]);
    }

    #[test]
    fn chefe_conta_na_missao_de_chefe_e_bicho_nao() {
        let um = |alvo: u16| {
            let mut a = vec![cq(alvo, ACTIVE, 0)];
            (
                !avancar_kill(&mut a, Some(quests::mob_kind::LOBO), None).is_empty(),
                !avancar_kill(&mut a, Some(10), None).is_empty(),
                !avancar_kill(&mut a, Some(12), None).is_empty(),
            )
        };
        // 511: o Lobo Alfa (kind 10), e so' ele.
        assert_eq!(um(511), (false, true, false));
        // 514: qualquer chefe — e so' quem a entidade diz que e' chefe. O kind
        // sozinho nao basta: escaravelho do Ermo divide numero com chefe.
        assert_eq!(um(514), (false, false, false));
        let chefe = |alvo: u16, kind: u16, e_chefe: bool| {
            let mut a = vec![cq(alvo, ACTIVE, 0)];
            !avancar_kill_com_chefe(&mut a, Some(kind), None, e_chefe).is_empty()
        };
        assert!(chefe(514, 10, true));
        assert!(!chefe(514, 10, false), "bicho comum com kind de chefe");
        // 522: qualquer bicho — chefe tambem e' bicho.
        assert_eq!(um(522), (true, true, true));
    }

    /// Personagem salvo antes dos tutoriais: marcador no indice velho (11) e
    /// o passo 711 em andamento. O marcador se realinha ao 711 e nada de
    /// passo fantasma entra no diario.
    /// A side quest taken from another island: its hunt only counts on its
    /// island; items, craft and dungeons count anywhere.
    #[test]
    fn caca_de_outra_ilha_so_conta_na_ilha_dela() {
        // 810+: the Glacier's chain; 102 (COLLECT steel) is an item quest.
        let caca = quests::QUESTS
            .iter()
            .find(|d| quests::zona_da_missao(d.id) == Some("ilha_gelo") && d.obj_kind == quests::objective_kind::KILL && d.obj_target != 0 && d.obj_target != quests::ALVO_QUALQUER_CHEFE)
            .expect("a Glacier hunt");
        assert!(conta_nesta_ilha(caca, "ilha_gelo"));
        assert!(!conta_nesta_ilha(caca, "ilha_inicial"));
        let mut a = vec![cq(caca.id, ACTIVE, 0)];
        let kind = caca.obj_target - 1;
        assert!(avancar_kill_na_ilha(&mut a, Some(kind), None, false, Some("ilha_inicial")).is_empty());
        assert_eq!(avancar_kill_na_ilha(&mut a, Some(kind), None, false, Some("ilha_gelo")).len(), 1);
        assert!(conta_nesta_ilha(def(102), "ilha_gelo"), "items count anywhere");
    }

    #[test]
    fn marcador_velho_se_realinha_ao_passo_em_andamento() {
        let mut a = vec![
            CharQuest {
                quest_id: historia::ID_MARCO,
                status: historia::STATUS_MARCO,
                progress: 11,
                cooldown_until: 0,
            },
            cq(711, ACTIVE, 0),
        ];
        assert_eq!(garantir_historia(&mut a), None, "nao da' passo novo");
        assert_eq!(indice_da_historia(&a), historia::indice(711));
        assert_eq!(passo_atual(&a).map(|c| c.quest_id), Some(711));
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn nao_aceita_duas_vezes() {
        let ativa = [cq(501, ACTIVE, 0)];
        assert!(!pode_aceitar(def(501), 1, 0, &ativa, 0));
        let pronta = [cq(501, READY, 1)];
        assert!(!pode_aceitar(def(501), 1, 0, &pronta, 0));
        let entregue = [cq(501, TURNED_IN, 1)];
        assert!(!pode_aceitar(def(501), 1, 0, &entregue, 0), "nao repetivel");
    }

    /// A variant kill counts for its species' quest, and not for another's.
    #[test]
    fn variante_conta_pra_especie() {
        let arqueiro = quests::mob_kind::ARQUEIRO;
        let gelo = shared::bestiary::kinds_of_species(arqueiro)
            .into_iter()
            .find(|k| *k != arqueiro)
            .expect("the archer has an island variant");
        let alvo = quests::alvo_de_mob(arqueiro);
        let q = quests::QUESTS
            .iter()
            .chain(shared::historia::PASSOS.iter())
            .find(|d| d.obj_kind == quests::objective_kind::KILL && d.obj_target == alvo)
            .expect("an archer kill quest");
        let mut a = vec![cq(q.id, ACTIVE, 0)];
        assert_eq!(avancar_kill(&mut a, Some(gelo), None).len(), 1, "the variant did not count");
        let mago = shared::bestiary::kinds_of_species(quests::mob_kind::MAGO)
            .into_iter()
            .find(|k| *k != quests::mob_kind::MAGO)
            .unwrap();
        assert!(avancar_kill(&mut a, Some(mago), None).is_empty(), "a mage variant counted for archers");
    }

    #[test]
    fn kill_conta_so_o_bicho_certo() {
        let mut a = vec![cq(502, ACTIVE, 0)];
        assert!(
            avancar_kill(&mut a, Some(quests::mob_kind::URSO), None).is_empty(),
            "urso nao e' lobo"
        );
        assert!(
            avancar_kill(&mut a, Some(quests::mob_kind::LOBO), Some(1)).is_empty(),
            "pvp nao conta"
        );
        for k in 1..=6 {
            let m = avancar_kill(&mut a, Some(quests::mob_kind::LOBO), None);
            assert_eq!(m, vec![(502, k, if k == 6 { READY } else { ACTIVE })]);
        }
        assert!(
            avancar_kill(&mut a, Some(quests::mob_kind::LOBO), None).is_empty(),
            "pronta nao passa do total"
        );
        // KILL de qualquer mob (alvo 0) continua contando tudo.
        let mut b = vec![cq(104, ACTIVE, 0)];
        assert_eq!(
            avancar_kill(&mut b, Some(quests::mob_kind::URSO), None).len(),
            1
        );
    }

    #[test]
    fn conversa_so_com_o_npc_certo() {
        let mut a = vec![cq(501, ACTIVE, 0), cq(505, ACTIVE, 0)];
        assert!(avancar_conversa(&mut a, Papel::Ferreiro as u16).is_empty());
        assert_eq!(
            avancar_conversa(&mut a, Papel::Alquimista as u16),
            vec![(501, 1, READY)]
        );
        assert_eq!(
            a[1].status, ACTIVE,
            "falar com o Alquimista nao conclui a do Capitao"
        );
        assert_eq!(
            avancar_conversa(&mut a, Papel::Estaleiro as u16),
            vec![(505, 1, READY)]
        );
        assert_eq!(
            quests::papel_de_conversa(Papel::Estaleiro.nome()),
            Some(Papel::Estaleiro as u16)
        );
        assert_eq!(quests::papel_de_conversa("Resident"), None);
    }

    #[test]
    fn nao_entrega_incompleta() {
        // 102 ("Miner", 15 Steel): no island menu offers a COLLECT any more
        // (04/10/2026, they became hunts and quarries), but the rule stays.
        let coleta = cq(102, ACTIVE, 0);
        assert!(checar_entrega(def(102), &coleta, 10)
            .unwrap_err()
            .contains("5 of"));
        assert_eq!(checar_entrega(def(102), &coleta, 15), Ok(15));
        assert!(
            checar_entrega(def(502), &cq(502, ACTIVE, 5), 0).is_err(),
            "kill pela metade"
        );
        assert_eq!(checar_entrega(def(502), &cq(502, READY, 6), 0), Ok(0));
        assert!(checar_entrega(def(501), &cq(501, TURNED_IN, 1), 0).is_err());
    }

    /// A cadeia e' coerente: todo 5xx e' de um NPC que EXISTE na vila (o
    /// Mestre ou um oficio), o pre-requisito existe e vem antes, e toda TALK
    /// aponta pra um NPC que existe na vila.
    #[test]
    fn a_cadeia_e_coerente() {
        let cadeia: Vec<_> = quests::QUESTS
            .iter()
            .filter(|d| (500..600).contains(&d.id))
            .collect();
        assert!(cadeia.len() >= 4);
        let vila_papeis: Vec<u16> = {
            let d = &shared::terreno::ARQUIPELAGO[0];
            let ger = shared::terreno::Gerador::da_ilha(d);
            ger.vila().npcs.iter().map(|n| n.papel as u16).collect()
        };
        let postos: Vec<u16> = {
            let d = &shared::terreno::ARQUIPELAGO[0];
            let ger = shared::terreno::Gerador::da_ilha(d);
            ger.vila().npcs.iter().filter_map(|n| n.giver).collect()
        };
        let givers_da_vila: Vec<u16> = vila_papeis
            .iter()
            .filter_map(|p| shared::quests::giver_do_npc(*p))
            .chain(postos)
            .collect();
        for d in cadeia {
            assert!(
                d.giver == GIVER_MESTRE_DA_ILHA || givers_da_vila.contains(&d.giver),
                "{}: giver {} nao tem NPC na vila",
                d.id,
                d.giver
            );
            if d.requires != 0 {
                assert!(
                    d.requires < d.id && quests::quest_by_id(d.requires).is_some(),
                    "{}",
                    d.id
                );
            }
            if d.obj_kind == quests::objective_kind::TALK {
                assert!(
                    vila_papeis.contains(&d.obj_target),
                    "{}: TALK pra NPC que nao existe na vila",
                    d.id
                );
            }
        }
    }

    /// Lobo (kind 0) sai em qualquer nivel; urso (kind 1) so' a partir do 3.
    #[test]
    fn chance_do_bicho_segue_o_sorteio_por_nivel() {
        let kinds = [0u16, 1, 2, 3];
        assert_eq!(chance_do_kind(&kinds, 0, 1, 2), 1.0, "nivel 1-2 so' lobo");
        assert_eq!(chance_do_kind(&kinds, 1, 1, 2), 0.0);
        // 3-5: lobo ou urso, e o urso — recem-chegado — vale dobrado.
        assert!(
            (chance_do_kind(&kinds, 1, 3, 5) - 2.0 / 3.0).abs() < 1e-6,
            "3-5: lobo ou urso, urso dobrado"
        );
        assert_eq!(chance_do_kind(&kinds, 9, 1, 15), 0.0, "kind que nao existe");
    }

    /// A zona do urso e' a mais perto ONDE URSO NASCE, nao a mais perto.
    #[test]
    fn zona_do_bicho_escolhe_onde_ele_nasce() {
        use glam::Vec2;
        let kinds = [0u16, 1, 2];
        let zonas = [
            (Vec2::new(10.0, 0.0), 1, 2),
            (Vec2::new(50.0, 0.0), 3, 5),
            (Vec2::new(90.0, 0.0), 6, 8),
        ];
        let eu = Vec2::ZERO;
        assert_eq!(
            zona_do_bicho(&zonas, &kinds, &[0], eu, 1),
            Some(Vec2::new(10.0, 0.0))
        );
        assert_eq!(
            zona_do_bicho(&zonas, &kinds, &[1], eu, 3),
            Some(Vec2::new(50.0, 0.0))
        );
        // Nivel baixo demais pras zonas de urso: ainda aponta onde ele nasce.
        assert_eq!(
            zona_do_bicho(&zonas, &kinds, &[1], eu, 0),
            Some(Vec2::new(50.0, 0.0))
        );
        // Qualquer bicho: a mais perto.
        assert_eq!(
            zona_do_bicho(&zonas, &kinds, &[], Vec2::new(85.0, 0.0), 10),
            Some(Vec2::new(90.0, 0.0))
        );
        assert_eq!(zona_do_bicho(&[], &kinds, &[0], eu, 1), None);
    }

    /// O spot com mais corpos vence o corpo sozinho mais perto.
    #[test]
    fn melhor_spot_prefere_o_veio_cheio() {
        use glam::Vec2;
        let corpos = [
            (Vec2::new(2.0, 0.0), 1),
            (Vec2::new(40.0, 0.0), 1),
            (Vec2::new(41.0, 1.0), 1),
            (Vec2::new(42.0, -1.0), 2),
        ];
        let (c, n) = melhor_spot(&corpos, Vec2::ZERO, 6.0).unwrap();
        assert_eq!(n, 3);
        assert!(
            c.distance(Vec2::new(41.0, 0.0)) < 0.5,
            "centro do veio: {c:?}"
        );
        assert!(melhor_spot(&[], Vec2::ZERO, 6.0).is_none());
    }

    /// Os spots saem do mais cheio pro menos e nunca dois no mesmo veio: e'
    /// essa lista que o servidor percorre testando o ALCANCE de cada um.
    #[test]
    fn spots_ordenados_sem_repetir_veio() {
        use glam::Vec2;
        let corpos = [
            (Vec2::new(40.0, 0.0), 1),
            (Vec2::new(41.0, 1.0), 1),
            (Vec2::new(42.0, -1.0), 1),
            (Vec2::new(-20.0, 0.0), 1),
            (Vec2::new(-21.0, 0.5), 1),
            (Vec2::new(0.0, 30.0), 1),
        ];
        let s = spots_ordenados(&corpos, Vec2::ZERO, 6.0, 6);
        let ns: Vec<usize> = s.iter().map(|(_, n)| *n).collect();
        assert_eq!(ns, vec![3, 2, 1], "{s:?}");
        for (i, (a, _)) in s.iter().enumerate() {
            for (b, _) in &s[i + 1..] {
                assert!(
                    a.distance(*b) >= 6.0,
                    "dois spots no mesmo veio: {a:?} {b:?}"
                );
            }
        }
        assert_eq!(spots_ordenados(&corpos, Vec2::ZERO, 6.0, 1).len(), 1);
    }

    #[test]
    fn conversa_so_conta_perto() {
        assert!(pode_concluir_conversa(shared::INTERACT_RADIUS));
        assert!(!pode_concluir_conversa(ALCANCE_DA_CONVERSA + 0.1));
    }

    /// O estado que o save grava e o load devolve e' o mesmo — sem banco no
    /// teste, confere a conversao de tipos que as duas pontas fazem.
    #[test]
    fn estado_ida_e_volta_pelos_tipos_do_banco() {
        let a = vec![
            cq(502, READY, 6),
            CharQuest {
                quest_id: 503,
                status: TURNED_IN,
                progress: 30,
                cooldown_until: i64::MAX,
            },
        ];
        let linhas: Vec<(i32, i16, i32, i64)> = a
            .iter()
            .map(|q| {
                (
                    q.quest_id as i32,
                    q.status as i16,
                    q.progress as i32,
                    q.cooldown_until,
                )
            })
            .collect();
        let volta: Vec<CharQuest> = linhas
            .into_iter()
            .map(|(qid, st, pr, cd)| CharQuest {
                quest_id: qid as u16,
                status: st as u8,
                progress: pr.max(0) as u32,
                cooldown_until: cd,
            })
            .collect();
        for (x, y) in a.iter().zip(&volta) {
            assert_eq!(
                (x.quest_id, x.status, x.progress, x.cooldown_until),
                (y.quest_id, y.status, y.progress, y.cooldown_until)
            );
        }
    }
}

#[cfg(test)]
mod testes_historia {
    use super::*;
    use shared::quests::{objective_kind, quest_by_id, quest_status};

    /// Personagem novo recebe o primeiro passo; garantir de novo nao duplica.
    #[test]
    fn novo_recebe_o_primeiro_passo() {
        let mut q = Vec::new();
        assert_eq!(garantir_historia(&mut q), Some(historia::PRIMEIRO_ID));
        assert_eq!(indice_da_historia(&q), Some(0));
        assert_eq!(passo_atual(&q).unwrap().status, quest_status::ACTIVE);
        assert_eq!(garantir_historia(&mut q), None, "duplicou o passo");
        assert_eq!(q.len(), 2);
        // Ninguem aceita a historia na mao.
        assert!(!pode_aceitar(
            quest_by_id(historia::PRIMEIRO_ID).unwrap(),
            99,
            0,
            &[],
            0
        ));
    }

    /// Pronto → sai, o indice anda e o proximo entra; ativo nao anda.
    #[test]
    fn conclusao_avanca() {
        let mut q = Vec::new();
        garantir_historia(&mut q);
        assert!(avancar_historia(&mut q).is_none(), "ativo andou");
        let m = avancar_conversa(&mut q, shared::construcao::Papel::Missoes as u16);
        assert_eq!(m, vec![(700, 1, quest_status::READY)]);
        let (feito, prox) = avancar_historia(&mut q).unwrap();
        assert_eq!((feito.id, prox), (700, Some(701)));
        assert_eq!(indice_da_historia(&q), Some(1));
        assert!(q.iter().all(|c| c.quest_id != 700), "concluido virou linha");
        assert_eq!(passo_atual(&q).unwrap().quest_id, 701);
    }

    /// A trava acompanha o nivel e fica pronta ao alcancar.
    #[test]
    fn trava_completa_no_level_up() {
        let id = (0..historia::total_escritos())
            .map(|i| historia::id_do_passo(i).unwrap())
            .find(|id| quest_by_id(*id).unwrap().obj_kind == objective_kind::NIVEL)
            .unwrap();
        let alvo = quest_by_id(id).unwrap().obj_count;
        let mut q = vec![CharQuest {
            quest_id: id,
            status: quest_status::ACTIVE,
            progress: 0,
            cooldown_until: 0,
        }];
        assert_eq!(
            checar_trava(&mut q, alvo - 1),
            vec![(id, alvo - 1, quest_status::ACTIVE)]
        );
        assert!(
            checar_trava(&mut q, alvo - 1).is_empty(),
            "reenviou sem mudar"
        );
        assert_eq!(
            checar_trava(&mut q, alvo + 3),
            vec![(id, alvo, quest_status::READY)]
        );
    }

    /// Indices do epilogo seguem sem fim e com id estavel; o marcador e o
    /// passo sobrevivem ao banco pelos tipos que o save usa.
    #[test]
    fn epilogo_estavel_e_persistencia() {
        let n = historia::total_escritos();
        let mut q = vec![CharQuest {
            quest_id: historia::ID_MARCO,
            status: historia::STATUS_MARCO,
            progress: n + 11,
            cooldown_until: 0,
        }];
        garantir_historia(&mut q);
        let atual = passo_atual(&q).unwrap().quest_id;
        assert_eq!(atual, historia::id_do_passo(n + 11).unwrap());
        assert_eq!(historia::indice(atual), Some(n + 11));
        let linhas: Vec<(i32, i16, i32, i64)> = q
            .iter()
            .map(|c| {
                (
                    c.quest_id as i32,
                    c.status as i16,
                    c.progress as i32,
                    c.cooldown_until,
                )
            })
            .collect();
        let volta: Vec<CharQuest> = linhas
            .into_iter()
            .map(|(qid, st, pr, cd)| CharQuest {
                quest_id: qid as u16,
                status: st as u8,
                progress: pr.max(0) as u32,
                cooldown_until: cd,
            })
            .collect();
        assert_eq!(indice_da_historia(&volta), Some(n + 11));
        assert_eq!(passo_atual(&volta).unwrap().quest_id, atual);
        // O marcador nao expira como diaria nem aparece como missao.
        let mut v = volta.clone();
        assert!(expirar_diarias(&mut v, i64::MAX).is_empty());
        assert!(quest_by_id(historia::ID_MARCO).is_none());
    }
}

#[cfg(test)]
mod testes_diarias {
    use super::*;
    use shared::quests::{alvo_de_coleta, objective_kind, quest_by_id, quest_status};

    fn ativa(id: u16) -> CharQuest {
        CharQuest {
            quest_id: id,
            status: quest_status::ACTIVE,
            progress: 0,
            cooldown_until: 0,
        }
    }

    /// Coletar conta pelo EVENTO e pelo alvo (pedra nao conta arvore); chega no
    /// total e fica pronta.
    #[test]
    fn diaria_de_coleta_conta_por_evento_e_fica_pronta() {
        let d = quest_by_id(601).unwrap();
        assert_eq!(d.obj_kind, objective_kind::GATHER);
        let mut q = vec![ativa(601), ativa(603)];
        let arvore = avancar_evento(
            &mut q,
            objective_kind::GATHER,
            &|d| alvo_de_coleta::conta(d.obj_target, 0),
            1,
        );
        assert!(arvore.is_empty(), "arvore contou numa diaria de pedra");
        for _ in 0..d.obj_count {
            avancar_evento(
                &mut q,
                objective_kind::GATHER,
                &|d| alvo_de_coleta::conta(d.obj_target, 2),
                1,
            );
        }
        assert_eq!(q[0].status, quest_status::READY);
        assert_eq!(q[0].progress, d.obj_count);
        assert_eq!(
            q[1].progress, 0,
            "evento de coleta contou na diaria de craft"
        );
        let feito = avancar_evento(&mut q, objective_kind::CRAFT, &|_| true, 1);
        assert_eq!(feito, vec![(603, 1, quest_status::READY)]);
    }

    /// Aceita num dia e nao entregou ate' a meia-noite UTC: sai do log. A
    /// entregue (em cooldown) fica — e volta a poder aceitar no dia seguinte.
    #[test]
    fn diaria_expira_na_virada_e_reseta_no_dia_seguinte() {
        let dia = 20_000 * 86_400;
        let meio_dia = dia + 43_200;
        let fim = shared::quests::proxima_meia_noite(meio_dia);
        assert_eq!(fim, dia + 86_400);
        let mut q = vec![
            CharQuest {
                quest_id: 602,
                status: quest_status::ACTIVE,
                progress: 3,
                cooldown_until: fim,
            },
            CharQuest {
                quest_id: 601,
                status: quest_status::TURNED_IN,
                progress: 20,
                cooldown_until: fim,
            },
            CharQuest {
                quest_id: 501,
                status: quest_status::ACTIVE,
                progress: 0,
                cooldown_until: 0,
            },
        ];
        assert!(expirar_diarias(&mut q, fim - 1).is_empty());
        assert_eq!(expirar_diarias(&mut q, fim), vec![602]);
        assert_eq!(q.len(), 2);
        let d601 = quest_by_id(601).unwrap();
        assert!(!pode_aceitar(d601, 1, 0, &q, fim - 1));
        assert!(pode_aceitar(d601, 1, 0, &q, fim));
    }

    /// Vencer dungeon avanca e FECHA as missoes de dungeon — e so' elas.
    ///
    /// O gancho mora em `world/dungeon.rs` (`dg_terminar` chama
    /// `quest_on_evento` com DUNGEON, por membro, so' na vitoria). Nada
    /// guardava isso: apagar aquela linha deixava a diaria 606 e a 510 do
    /// Mestre impossiveis de concluir, em silencio. Este teste cobre o lado
    /// desta camada — que o tipo casa e fecha.
    #[test]
    fn vitoria_de_dungeon_fecha_as_missoes_de_dungeon() {
        use shared::quests::objective_kind as ok;
        let ativa = |id: u16| CharQuest {
            quest_id: id,
            status: quest_status::ACTIVE,
            progress: 0,
            cooldown_until: 0,
        };
        // 606 e' a diaria "Porao do dia"; 510, a do Mestre que apresenta a dungeon.
        for id in [606u16, 510] {
            let d = quest_by_id(id).unwrap();
            assert_eq!(d.obj_kind, ok::DUNGEON, "{id} deixou de ser de dungeon");
            let mut q = vec![ativa(id)];
            let mudou = avancar_evento(&mut q, ok::DUNGEON, &|_| true, 1);
            assert_eq!(mudou.len(), 1, "{id}: vitoria nao avancou");
            assert_eq!(
                q[0].status,
                quest_status::READY,
                "{id}: avancou mas nao fechou"
            );
        }
        // Tipo errado nao mexe: raid e craft nao fecham missao de dungeon.
        let mut q = vec![ativa(606)];
        assert!(
            avancar_evento(&mut q, ok::RAID, &|_| true, 1).is_empty(),
            "raid fechou dungeon"
        );
        assert!(
            avancar_evento(&mut q, ok::CRAFT, &|_| true, 1).is_empty(),
            "craft fechou dungeon"
        );
        assert_eq!(q[0].status, quest_status::ACTIVE);
    }

    /// Em breve nao se aceita; a diaria de outra ilha nao e' oferecida aqui.
    #[test]
    fn em_breve_nao_aceita_e_cada_ilha_oferece_as_suas() {
        // A Cacada (607) e' RAID e nao existe; a dungeon (606) conta de
        // verdade — coberto em `vitoria_de_dungeon_fecha_as_missoes_de_dungeon`.
        assert!(!pode_aceitar(quest_by_id(607).unwrap(), 99, 0, &[], 0));
        let src = shared::quests::quest_source::NPC;
        let aqui: Vec<u16> = offerable(
            src,
            shared::quests::GIVER_MESTRE_DA_ILHA,
            60,
            0,
            &[],
            0,
            "ilha_gelo",
        )
        .iter()
        .map(|d| d.id)
        .collect();
        assert!(aqui.contains(&611) && aqui.contains(&612));
        assert!(
            aqui.iter()
                .all(|id| shared::quests::zona_da_missao(*id) == Some("ilha_gelo")),
            "{aqui:?}"
        );
        assert!(!aqui.contains(&615), "em breve oferecida");
    }

    /// Missao de area paga a Pocao de Experiencia; a diaria de criar paga a de
    /// Fortuna, a de refinar a de Sorte; a de conversa nao paga pocao.
    #[test]
    fn missao_de_area_paga_pocao_de_xp() {
        for id in [502u16, 503, 504, 601, 602, 611, 632] {
            let d = quest_by_id(id).unwrap();
            assert!(shared::quests::e_de_area(d));
            assert_eq!(
                (d.reward_item2, d.reward_item2_qty),
                (shared::item_id::XP_POTION, 1),
                "{id}"
            );
        }
        assert_eq!(quest_by_id(501).unwrap().reward_item2, 0);
        let criar = quest_by_id(603).unwrap();
        assert_eq!(
            (criar.reward_item2, criar.reward_item2_qty),
            (shared::item_id::FORTUNA_POTION, 1)
        );
        let refinar = quest_by_id(604).unwrap();
        assert_eq!(
            (refinar.reward_item2, refinar.reward_item2_qty),
            (shared::item_id::SORTE_POTION, 1)
        );
    }

    /// O bonus multiplica, renova sem acumular e expira.
    #[test]
    fn bonus_de_xp_multiplica_renova_e_expira() {
        let agora = 1_000_000;
        let ate = shared::renovar_bonus_xp(agora);
        assert_eq!(shared::xp_com_bonus(100, agora, ate), 130);
        // Beber de novo meia hora depois: volta a 1 h cheia, nao soma nem vira 60%.
        let de_novo = shared::renovar_bonus_xp(agora + 1_800);
        assert_eq!(de_novo, agora + 1_800 + 3_600);
        assert_eq!(shared::xp_com_bonus(100, agora + 1_800, de_novo), 130);
        assert_eq!(
            shared::xp_com_bonus(100, de_novo, de_novo),
            100,
            "expirado ainda multiplica"
        );
        assert_eq!(shared::xp_com_bonus(100, agora, 0), 100);
    }
}

#[cfg(test)]
mod testes_do_passo_fantasma {
    use super::*;

    fn cq(id: u16, status: u8) -> CharQuest {
        CharQuest {
            quest_id: id,
            status,
            progress: 0,
            cooldown_until: 0,
        }
    }

    /// Um passo que não existe mais NÃO pode travar o jogador.
    ///
    /// A tabela de passos é código e o progresso é dado: eles se separam
    /// quando a história é renumerada. Em 21/09/2026 dois personagens
    /// ficaram apontando para 775 e 776, ids que deixaram de existir —
    /// `quest_by_id` devolvia `None`, nada avançava, e **nada reclamava**.
    ///
    /// O marcador guarda o ÍNDICE, que sobrevive à renumeração. Descartar o
    /// fantasma devolve o comando a ele.
    #[test]
    fn passo_que_nao_existe_mais_e_descartado_e_o_marcador_reassume() {
        let inexistente = 899u16; // fora de qualquer faixa de passo
        assert!(
            historia::indice(inexistente).is_none(),
            "o teste precisa de um id que realmente não exista"
        );
        let mut q = vec![
            cq(historia::ID_MARCO, historia::STATUS_MARCO),
            cq(inexistente, shared::quests::quest_status::ACTIVE),
        ];
        let novo = garantir_historia(&mut q);
        assert!(
            !q.iter().any(|c| c.quest_id == inexistente),
            "o fantasma continua no log e vai travar o jogador"
        );
        // E um passo de verdade entrou no lugar.
        let id = novo.expect("o marcador tem que reabrir um passo válido");
        assert!(historia::indice(id).is_some(), "reabriu outro fantasma");
    }

    /// E um passo VÁLIDO não é descartado — senão o conserto viraria um
    /// reset de progresso a cada login.
    #[test]
    fn passo_valido_continua_no_log() {
        let valido = historia::PRIMEIRO_ID + 3;
        assert!(historia::indice(valido).is_some());
        let mut q = vec![
            cq(historia::ID_MARCO, historia::STATUS_MARCO),
            cq(valido, shared::quests::quest_status::ACTIVE),
        ];
        garantir_historia(&mut q);
        assert!(
            q.iter().any(|c| c.quest_id == valido),
            "o passo em andamento foi descartado"
        );
    }

    /// PASSO NOVO NO MEIO nao rebobina quem ja' passou dali.
    ///
    /// Esta e' a pergunta do dono — "personagem novo ja' vem com as quests
    /// novas, né?" —, e a resposta interessante e' a OUTRA metade: o que
    /// acontece com quem ja' existe. O marcador guarda o INDICE, e inserir
    /// cinco passos no meio (o tutorial da ilha, 798-802, em 21/09/2026)
    /// desloca o indice de TUDO o que vem depois. Sem defesa, quem estava em
    /// "Rumo a' Geleira" acordaria cinco passos atras, dentro de um tutorial
    /// que so' se cumpre na propria ilha — longe de onde ele esta'.
    ///
    /// A defesa e' o ID: o passo em andamento guarda o id, que NAO se desloca,
    /// e o marcador se realinha a ele. Isto trava esse contrato, que hoje
    /// existe so' como um comentario dentro de `garantir_historia`.
    #[test]
    fn passo_novo_no_meio_nao_rebobina_quem_passou() {
        for id in [710u16, 719, 730, 752, 769] {
            let certo = historia::indice(id).expect("passo de verdade");
            // Marcador ERRADO de proposito, como se a lista tivesse crescido
            // depois do save: cinco passos atras do id em andamento.
            let mut q = vec![
                CharQuest {
                    quest_id: historia::ID_MARCO,
                    status: historia::STATUS_MARCO,
                    progress: certo.saturating_sub(5),
                    cooldown_until: 0,
                },
                cq(id, shared::quests::quest_status::ACTIVE),
            ];
            garantir_historia(&mut q);
            let marcador = q
                .iter()
                .find(|c| c.quest_id == historia::ID_MARCO)
                .expect("o marcador tem que continuar la'");
            assert_eq!(
                marcador.progress, certo,
                "o marcador nao se realinhou ao passo {id}: o jogador rebobinou"
            );
            assert_eq!(
                passo_atual(&q).map(|c| c.quest_id),
                Some(id),
                "o passo atual deixou de ser o {id}"
            );
        }
    }
}

#[cfg(test)]
mod testes_do_alvo_existe {
    use shared::quests::{self, objective_kind, ALVO_QUALQUER_CHEFE};

    /// EVERY KILL QUEST'S TARGET SPAWNS ON ITS ISLAND — as itself or as one
    /// of its island variants (`bestiary`). The island variants (01/10/2026)
    /// changed what each island spawns, and the owner suspected quests were
    /// "sending me to kill mobs that don't exist anymore". This walks the
    /// real catalogues (side quests and the written story), not a list here.
    #[test]
    fn todo_alvo_de_caca_nasce_na_ilha_da_missao() {
        let todas = quests::QUESTS.iter().chain(shared::historia::PASSOS.iter());
        let mut faltam = Vec::new();
        for d in todas {
            if d.obj_kind != objective_kind::KILL || d.obj_target == 0 || d.obj_target == ALVO_QUALQUER_CHEFE {
                continue;
            }
            let Some(zona) = quests::zona_da_missao(d.id) else { continue };
            let Some(def) = shared::terreno::def_da_zona(zona) else { continue };
            let especie = d.obj_target - 1;
            let nasce = crate::economy::kinds_do_bioma(def.bioma)
                .iter()
                .chain(crate::economy::kinds_de_praia_do_bioma(def.bioma))
                .any(|k| shared::bestiary::species_of(*k) == especie)
                // A field boss of that island (511-513 hunt the Bosque's).
                || shared::bosses::chefe(especie).is_some_and(|c| c.zona == zona);
            if !nasce {
                faltam.push(format!("{} \"{}\" on {zona}: species {especie} never spawns there", d.id, d.title));
            }
        }
        assert!(faltam.is_empty(), "{} kill quest(s) without their mob:\n{}", faltam.len(), faltam.join("\n"));
    }
}
