//! A HISTORIA: a linha de missoes principal do arquipelago.
//!
//! Nao se aceita nem se abandona. Todo personagem recebe o primeiro passo ao
//! entrar no mundo, e o servidor passa pro seguinte assim que um termina — de
//! ponto-chave em ponto-chave: falar com alguem da vila ou do porto, subir num
//! mirante, cacar o bicho da vez, quebrar pedra, criar e refinar a primeira
//! peca, atravessar pra proxima ilha. So' TRAVA por nivel ("Alcance o nivel
//! 20 pra continuar") e, na troca de ilha, pela rota estar no ar.
//!
//! Sao quatro capitulos escritos, um por ilha, e depois as CRONICAS DA
//! TEMPESTADE: capitulos gerados pelo indice, sem fim, com uma trava a cada
//! cinco niveis ate' o teto.
//!
//! O estado do jogador e' so' o INDICE do passo atual (uma linha marcadora em
//! `character_quests`) mais a linha do passo em andamento — os concluidos nao
//! viram linha, entao a historia infinita nao engorda o banco.
//!
//! Tudo aqui e' funcao pura: cliente e servidor chegam no mesmo passo pelo
//! mesmo indice, e o cliente desenha o rastreador e o menu sem mensagem nova.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::constants::item_id;
use crate::construcao::Papel;
use crate::quests::{
    alvo_de_coleta, alvo_de_mob, mob_kind, momento, objective_kind, quest_source, tutorial as tut,
    QuestDef,
};

/// Giver das missoes da historia. Nao e' NPC nenhum: fora de toda faixa de
/// giver real, pra ninguem oferecer.
pub const GIVER_HISTORIA: u16 = 131;
/// Linha marcadora em `character_quests`: `progress` = indice do passo atual.
pub const ID_MARCO: u16 = 699;
/// Status da linha marcadora (nao e' ativa, pronta nem entregue).
pub const STATUS_MARCO: u8 = 3;
/// Primeiro id dos passos escritos (seguidos, um por passo).
pub const PRIMEIRO_ID: u16 = 700;
/// O passo que manda criar a primeira peca. O ANTERIOR e' quem entrega a chave
/// que torna isso possivel — toda receita de equipamento comeca com uma chave,
/// e chave so' cai de chefe. Os dois andam juntos: ver o teste
/// `a_chave_vem_no_passo_antes_do_craft`.
pub const PASSO_DO_CRAFT: u16 = 707;
/// Faixa dos passos TUTORIAIS: entram no meio do capitulo I sem renumerar os
/// outros (ver `id_do_passo`, que anda pela POSICAO na lista).
pub const PRIMEIRO_ID_TUTORIAL: u16 = 770;
pub const ULTIMO_ID_TUTORIAL: u16 = 789;
/// Primeiro id das cronicas. Ate' `u16::MAX` sao 55.536 passos de epilogo —
/// mais de nove mil capitulos.
pub const PRIMEIRO_ID_DO_EPILOGO: u16 = 10_000;
/// Passos por capitulo das cronicas.
pub const PASSOS_POR_CRONICA: u32 = 6;

/// Pontos-chave de uma ilha (`obj_target` de `objective_kind::LUGAR`).
pub mod ponto {
    /// Praca da cidade.
    pub const CIDADE: u16 = 1;
    /// Saida da cidade, na estrada do porto.
    pub const SAIDA: u16 = 2;
    /// Patio do porto.
    pub const PORTO: u16 = 3;
    /// Ponta do cais.
    pub const CAIS: u16 = 4;
    /// O ponto mais alto da ilha (longe da cidade).
    pub const MIRANTE: u16 = 5;
    /// A terra firme mais distante da cidade.
    pub const COSTA: u16 = 6;

    /// Quao perto do ponto conta como chegar.
    pub fn raio(p: u16) -> f32 {
        match p {
            CIDADE => 12.0,
            SAIDA => 10.0,
            PORTO => 16.0,
            CAIS => 6.0,
            MIRANTE => 26.0,
            COSTA => 22.0,
            _ => 10.0,
        }
    }

    pub fn nome(p: u16) -> &'static str {
        match p {
            CIDADE => "a praça",
            SAIDA => "a saída da cidade",
            PORTO => "o porto",
            CAIS => "a ponta do cais",
            MIRANTE => "o mirante",
            COSTA => "a costa distante",
            _ => "o lugar",
        }
    }
}

/// Um capitulo escrito: uma ilha, uma faixa de ids.
#[derive(Debug, Clone, Copy)]
pub struct Capitulo {
    pub nome: &'static str,
    /// Indice em `terreno::ARQUIPELAGO`.
    pub ilha: usize,
    pub primeiro: u16,
    pub ultimo: u16,
}

pub const CAPITULOS: &[Capitulo] = &[
    Capitulo {
        nome: "I · O Farol do Bosque",
        ilha: 0,
        primeiro: 700,
        ultimo: 718,
    },
    Capitulo {
        nome: "II · O Farol Congelado",
        ilha: 1,
        primeiro: 719,
        ultimo: 736,
    },
    Capitulo {
        nome: "III · Areias que Gritam",
        ilha: 2,
        primeiro: 737,
        ultimo: 751,
    },
    Capitulo {
        nome: "IV · O Coração da Tempestade",
        ilha: 3,
        primeiro: 752,
        ultimo: 768,
    },
];

// ─────────────────────────── construtores ───────────────────────────

const fn base(id: u16, title: &'static str, desc: &'static str, gold: u32, xp: u64) -> QuestDef {
    QuestDef {
        id,
        title,
        desc,
        reward_cobre: gold,
        reward_xp: xp,
        source: quest_source::HISTORIA,
        giver: GIVER_HISTORIA,
        ..crate::quests::quest_vazia()
    }
}

#[allow(clippy::too_many_arguments)]
const fn falar(
    id: u16,
    title: &'static str,
    desc: &'static str,
    papel: Papel,
    gold: u32,
    xp: u64,
    item: u16,
    qtd: u16,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::TALK,
        obj_target: papel as u16,
        obj_count: 1,
        reward_item: item,
        reward_item_qty: qtd,
        ..base(id, title, desc, gold, xp)
    }
}

/// Conversa que entrega DOIS itens. Existe por causa da chave: a faixa cinza
/// pede 1 chave + 30 + 10 + 10 + 200 darksteel + 300 cobre, e a chave e' o
/// UNICO desses que nao se farma na ilha (so' cai de chefe). Sem entregar a
/// chave em algum passo, o "crie seu primeiro equipamento" era um pedido
/// impossivel — ver docs/HISTORIA.md.
#[allow(clippy::too_many_arguments)]
const fn falar_com_dois(
    id: u16,
    title: &'static str,
    desc: &'static str,
    papel: Papel,
    gold: u32,
    xp: u64,
    item: u16,
    qtd: u16,
    item2: u16,
    qtd2: u16,
) -> QuestDef {
    QuestDef {
        reward_item2: item2,
        reward_item2_qty: qtd2,
        ..falar(id, title, desc, papel, gold, xp, item, qtd)
    }
}

const fn ir(
    id: u16,
    title: &'static str,
    desc: &'static str,
    p: u16,
    gold: u32,
    xp: u64,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::LUGAR,
        obj_target: p,
        obj_count: 1,
        ..base(id, title, desc, gold, xp)
    }
}

/// Caca: missao de area, paga Pocao de Experiencia.
#[allow(clippy::too_many_arguments)]
const fn cacar(
    id: u16,
    title: &'static str,
    desc: &'static str,
    alvo: u16,
    n: u32,
    gold: u32,
    xp: u64,
    pocao: u16,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::KILL,
        obj_target: alvo,
        obj_count: n,
        reward_item: pocao,
        reward_item_qty: 3,
        reward_item2: item_id::XP_POTION,
        reward_item2_qty: 1,
        ..base(id, title, desc, gold, xp)
    }
}

/// Quebrar pedra: missao de area, paga material da cor da faixa e Pocao de
/// Experiencia.
#[allow(clippy::too_many_arguments)]
const fn coletar(
    id: u16,
    title: &'static str,
    desc: &'static str,
    n: u32,
    gold: u32,
    xp: u64,
    material: u16,
    qtd: u16,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::GATHER,
        obj_target: alvo_de_coleta::PEDRA,
        obj_count: n,
        reward_item: material,
        reward_item_qty: qtd,
        reward_item2: item_id::XP_POTION,
        reward_item2_qty: 1,
        ..base(id, title, desc, gold, xp)
    }
}

/// Cacar com recompensa escolhida (em vez da pocao + Pocao de Experiencia).
#[allow(clippy::too_many_arguments)]
const fn cacar_com(
    id: u16,
    title: &'static str,
    desc: &'static str,
    alvo: u16,
    n: u32,
    gold: u32,
    xp: u64,
    item: u16,
    qtd: u16,
    item2: u16,
    qtd2: u16,
) -> QuestDef {
    QuestDef {
        reward_item: item,
        reward_item_qty: qtd,
        reward_item2: item2,
        reward_item2_qty: qtd2,
        ..cacar(id, title, desc, alvo, n, gold, xp, 0)
    }
}

/// Coletar (pedra OU arvore, `alvo_de_coleta`) com recompensa escolhida.
#[allow(clippy::too_many_arguments)]
const fn coletar_com(
    id: u16,
    title: &'static str,
    desc: &'static str,
    alvo: u16,
    n: u32,
    gold: u32,
    xp: u64,
    item: u16,
    qtd: u16,
    item2: u16,
    qtd2: u16,
) -> QuestDef {
    QuestDef {
        obj_target: alvo,
        reward_item2: item2,
        reward_item2_qty: qtd2,
        ..coletar(id, title, desc, n, gold, xp, item, qtd)
    }
}

/// Vencer a dungeon `conteudo` (`shared::dungeon::CONTEUDOS`). O servidor so'
/// conta a vitoria DAQUELA dungeon; o toque no rastreador abre o painel.
#[allow(clippy::too_many_arguments)]
const fn dungeon(
    id: u16,
    title: &'static str,
    desc: &'static str,
    conteudo: u16,
    gold: u32,
    xp: u64,
    item: u16,
    qtd: u16,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::DUNGEON,
        obj_target: conteudo,
        obj_count: 1,
        reward_item: item,
        reward_item_qty: qtd,
        ..base(id, title, desc, gold, xp)
    }
}

const fn criar(id: u16, title: &'static str, desc: &'static str, gold: u32, xp: u64) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::CRAFT,
        obj_target: 0,
        obj_count: 1,
        ..base(id, title, desc, gold, xp)
    }
}

const fn refinar(
    id: u16,
    title: &'static str,
    desc: &'static str,
    vezes: u32,
    gold: u32,
    xp: u64,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::REFINE,
        obj_target: 0,
        obj_count: vezes,
        ..base(id, title, desc, gold, xp)
    }
}

const fn nivel(id: u16, title: &'static str, n: u32) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::NIVEL,
        obj_target: 0,
        obj_count: n,
        ..base(id, title, "A história continua quando você chegar a esse nível. Cace, colete e faça as diárias do Mestre de Missões.", 0, 0)
    }
}

/// Passo tutorial: fazer uma acao da interface (`quests::tutorial::*`).
#[allow(clippy::too_many_arguments)]
const fn tutorial(
    id: u16,
    title: &'static str,
    desc: &'static str,
    acao: u16,
    gold: u32,
    xp: u64,
    item: u16,
    qtd: u16,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::TUTORIAL,
        obj_target: acao,
        obj_count: 1,
        reward_item: item,
        reward_item_qty: qtd,
        ..base(id, title, desc, gold, xp)
    }
}

/// Tutorial que pede uma QUANTIDADE (hoje so' a Energia). O passo continua
/// sendo TUTORIAL: quem conta e' o servidor, somando o que entrou.
#[allow(clippy::too_many_arguments)]
const fn tutorial_de(
    id: u16,
    title: &'static str,
    desc: &'static str,
    acao: u16,
    conta: u32,
    gold: u32,
    xp: u64,
    item: u16,
    qtd: u16,
) -> QuestDef {
    QuestDef {
        obj_count: conta,
        ..tutorial(id, title, desc, acao, gold, xp, item, qtd)
    }
}

const fn viajar(
    id: u16,
    title: &'static str,
    desc: &'static str,
    ilha: u16,
    gold: u32,
    xp: u64,
) -> QuestDef {
    QuestDef {
        obj_kind: objective_kind::VIAGEM,
        obj_target: ilha,
        obj_count: 1,
        ..base(id, title, desc, gold, xp)
    }
}

const VERDE: u16 = item_id::na_cor(item_id::STEEL, 2);
const AZUL: u16 = item_id::na_cor(item_id::STEEL, 3);

/// Os passos escritos, em ordem. Ids seguidos a partir de `PRIMEIRO_ID`.
pub const PASSOS: &[QuestDef] = &[
    // ═════════════ I · O Farol do Bosque (Bosque, 1–15) ═════════════
    // XP: a historia CARREGA o nivel do capitulo I (lobo da' 32 de XP e o
    // nivel 5 pede 15.000 — trava de nivel aqui era 450 lobos). Chega ao 6
    // antes do Porao, ao 14 antes da Adega e ao 15 no barco. E os passos ate'
    // o craft entregam a receita INTEIRA da primeira armadura cinza. Os dois
    // contratos sao testes: `capitulo_um_carrega_o_nivel_das_dungeons` e
    // `o_inicio_entrega_a_primeira_armadura`.
    falar(700, "Desperte na praça", "Você acordou na praia na noite em que o farol do Bosque apagou. Procure o Mestre de Missões, perto do poço da praça.", Papel::Missoes, 20, 500, item_id::HEALTH_POTION, 5),
    falar(701, "Um gole de coragem", "O Mestre quer você de pé. Fale com o Alquimista, na loja de toldo verde: sem poção, a mata engole qualquer um.", Papel::Alquimista, 20, 1_000, item_id::HEALTH_POTION, 5),
    // Tutoriais (770+): cada um ensina UMA coisa da interface, na hora em que
    // ela passa a fazer falta. Ids fora da sequencia de proposito.
    tutorial(770, "Poção na hora certa", "O Alquimista insiste: poção boa é a que se bebe sozinha. Abra Menu › Sistema › Barra, escolha a Poção de Vida e ajuste com − e + a % de vida em que ela é bebida.", tut::POCAO_LIMIAR, 20, 300, item_id::HEALTH_POTION, 5),
    tutorial(771, "Luta sem as mãos", "Antes dos lobos, aprenda a lutar sem pensar: toque em COMBATE, no canto de baixo, e o personagem enfrenta sozinho o que estiver perto. Toque de novo para parar.", tut::AUTO_COMBATE, 20, 300, item_id::HEALTH_POTION, 3),
    cacar_com(702, "A trilha dos lobos", "Os lobos enlouqueceram desde que o farol apagou: descem à trilha de dia, coisa que nunca fizeram. Derrote 5 fora da cidade. Os caçadores juntaram berloques que os bichos arrancaram das carroças — são seus.", alvo_de_mob(mob_kind::LOBO), 5, 60, 1_000, item_id::EXORCISM_BAUBLE, 10, item_id::HEALTH_POTION, 5),
    falar(703, "Mãos firmes", "Você sobreviveu aos lobos. O Treinador da praça quer ver do que é capaz — e guarda quintessência para quem aguenta o tranco.", Papel::Treinador, 40, 2_500, item_id::QUINTESSENCE, 10),
    tutorial(772, "Golpe no automático", "O Treinador mostra: arraste uma skill PARA CIMA e ela passa a ser usada sozinha no combate. Para baixo, volta pro manual.", tut::SKILL_AUTO, 40, 800, item_id::HEALTH_POTION, 3),
    tutorial(773, "Coleta sem esforço", "A forja come madeira e pedra. Toque em COLETA: o personagem corta e quebra sozinho o que estiver por perto — a engrenagem do botão escolhe o quê.", tut::AUTO_COLETA, 40, 800, item_id::XP_POTION, 1),
    tutorial_de(776, "A luz nas pedras", "O trovão deixou cristais azuis no relevo: é Energia, e ela não ocupa espaço na bolsa e é o que paga os seus pontos de atributo. Toque no passo que o caminho até um veio abre sozinho — junte o bastante pra gastar tudo o que os primeiros níveis te deram.", tut::COLETA_ENERGIA, crate::constants::ENERGIA_DO_TUTORIAL, 40, 900, item_id::HEALTH_POTION, 3),
    tutorial(775, "O que o nível te deu", "Cada ponto de atributo custa Energia — a mesma dos cristais azuis do relevo. Abra Menu › Personagem › Ficha e coloque um ponto no atributo que combina com a sua arma: FOR bate mais forte, DES acerta mais, INT move a magia. Se faltar Energia, quebre um cristal e volte.", tut::PONTO_ATRIBUTO, 20, 400, item_id::HEALTH_POTION, 3),
    coletar_com(704, "Lenha para a forja", "A forja da vila come madeira dia e noite. Derrube 8 árvores — é da árvore que sai toda a madeira da ilha. O Ferreiro paga em Darksteel, o metal escuro que toda peça pede.", alvo_de_coleta::ARVORE, 8, 60, 2_000, item_id::DARKSTEEL, 200, item_id::XP_POTION, 1),
    coletar_com(705, "Pedra que canta", "As pedras da ilha zumbem com o trovão. Quebre 10 pedras em qualquer veio — é da pedra que saem o Aço e o Darksteel de toda peça. A mineradora completa o seu Aço.", alvo_de_coleta::PEDRA, 10, 80, 4_000, item_id::STEEL, 30, item_id::XP_POTION, 1),
    falar_com_dois(706, "O metal da tempestade", "Leve o que ouviu nas pedras ao Ferreiro. Ele sabe o que o metal carrega — e guarda o couro e o cobre que faltam para quem vai forjar.", Papel::Ferreiro, 60, 4_000, item_id::COPPER, 300, item_id::HIDE, 1),
    criar(707, "Sua primeira peça", "Você tem tudo o que a Armadura pede: o couro do Ferreiro, Aço, Quintessência, Berloque, Darksteel e cobre. Abra o Craft e crie sua primeira armadura. Depois disso, tudo isso se farma: pedra, árvore, bicho e chefe.", 100, 6_000),
    cacar(708, "Ursos na encosta", "Os ursos desceram das encostas atrás do cheiro de trovão. Derrote 4 ursos.", alvo_de_mob(mob_kind::URSO), 4, 120, 6_500, item_id::HEALTH_POTION),
    tutorial(774, "O mapa mostra o caminho", "Toque no minimapa para abrir o mapa da ilha e toque num lugar: o personagem vai sozinho até lá.", tut::MAPA_IR, 60, 1_500, item_id::HEALTH_POTION, 3),
    ir(709, "O mirante do Bosque", "Suba ao ponto mais alto da ilha. De lá se vê o olho da tempestade — e, lá embaixo, o casco do naufrágio encalhado.", ponto::MIRANTE, 150, 8_500),
    dungeon(710, "O porão do naufrágio", "Do mirante você viu o casco. Os Morganeers fizeram do porão um esconderijo, e quem manda lá dentro carrega chave no bolso — chefe de dungeon larga chave bem mais que chefe de campo. Toque no passo (ou em Dungeons, no Menu) e limpe o Porão do Naufrágio. Sozinho dá.", 1, 300, 19_000, item_id::GREATER_HEAL, 5),
    refinar(711, "Fogo na forja", "Peça fraca não aguenta a tempestade. Tente refinar uma peça na Forja.", 1, 150, 20_000),
    cacar(712, "Os pistoleiros de Morgan", "Pistoleiros dos Morganeers rondam a mata atrás das pedras do farol. Derrote 6 deles.", alvo_de_mob(mob_kind::PISTOLEIRO), 6, 200, 30_000, item_id::GREATER_HEAL),
    ir(713, "A estrada do porto", "Os Peacemain guardam o porto. Siga pela estrada até o pátio do cais.", ponto::PORTO, 150, 37_500),
    falar(714, "O Capitão do Porto", "O Capitão sabe por que os faróis apagam. Fale com ele no porto.", Papel::Estaleiro, 200, 57_500, item_id::GREATER_HEAL, 3),
    cacar(715, "Garras no caminho do cais", "Tigres cercam a estrada dos carregadores e ninguém passa com carga. Derrote 5 — deles se tira a Quintessência, que toda armadura pede e a pedra dá a conta-gotas.", alvo_de_mob(mob_kind::TIGRE), 5, 250, 90_000, item_id::GREATER_HEAL),
    ir(716, "O cais ao amanhecer", "Vá até a ponta do cais: o Capitão prometeu mostrar a rota das ilhas.", ponto::CAIS, 200, 120_000),
    dungeon(717, "A adega do contrabandista", "Antes de zarpar, o Capitão quer o porto limpo: os contrabandistas de Morgan guardam pedra do farol numa adega sob o cais. Toque no passo (ou em Dungeons, no Menu) e limpe a Adega do Contrabandista.", 2, 500, 97_500, item_id::GREATER_HEAL, 5),
    viajar(718, "Rumo à Geleira", "O farol da Geleira ainda brilha, mas por pouco. Peça ao Capitão do Porto um lugar no barco.", 1, 400, 1_500),
    // ═════════════ II · O Farol Congelado (Geleira, 15–30) ═════════════
    falar(719, "Frio de rachar os ossos", "Você desembarcou na Geleira. Apresente-se ao Mestre de Missões da praça.", Papel::Missoes, 200, 1_200, item_id::GREATER_HEAL, 2),
    tutorial(777, "O primeiro despertar", "A Energia que você juntou no Bosque não é só brilho: ela desperta o que você já sabe. Abra Menu › Personagem › Habilidades, escolha uma das suas e suba um tier.", tut::EVOLUIR_SKILL, 200, 3_000, item_id::XP_POTION, 1),
    falar(720, "Histórias de taberna", "Quem sabe dos Morganeers na neve é o Taberneiro. Pague um ouvido a ele.", Papel::Taberna, 200, 1_300, item_id::GREATER_MANA, 2),
    cacar(721, "Corujursos na neve", "Owlbears famintos atacam as trilhas de gelo. Derrote 6.", alvo_de_mob(mob_kind::OWLBEAR), 6, 300, 1_800, item_id::GREATER_HEAL),
    coletar(722, "Gelo que guarda trovão", "A pedra da Geleira prende relâmpago. Quebre 20 pedras.", 20, 300, 2_000, VERDE, 4),
    falar(723, "O fragmento de gelo", "Mostre o que achou nas pedras ao Identificador.", Papel::Identificador, 250, 2_000, item_id::GREATER_MANA, 2),
    ir(724, "O farol congelado", "O farol da Geleira fica no alto. Suba até o mirante da ilha.", ponto::MIRANTE, 350, 2_600),
    nivel(725, "Alcance o nível 20", 20),
    cacar(726, "Arqueiros da nevasca", "Arqueiros dos Morganeers vigiam o farol. Derrote 8.", alvo_de_mob(mob_kind::ARQUEIRO), 8, 400, 3_200, item_id::GREATER_HEAL),
    criar(727, "Couraça contra o frio", "Crie uma peça nova no Craft para o frio que vem.", 350, 3_000),
    refinar(728, "Aço que não quebra", "Tente refinar duas vezes na Forja.", 2, 400, 3_400),
    falar(729, "Capa de lã-de-tempestade", "O Alfaiate costura com fios que seguram o vento. Fale com ele.", Papel::Alfaiate, 350, 3_200, item_id::GREATER_HEAL, 3),
    nivel(730, "Alcance o nível 25", 25),
    cacar(731, "A patrulha de Morgan", "Os Morganeers cercam a ilha. Derrote 30 inimigos de qualquer tipo.", 0, 30, 600, 4_500, item_id::GREATER_HEAL),
    ir(732, "A costa distante", "Um barco dos Morganeers encalhou na costa mais distante. Vá até lá.", ponto::COSTA, 500, 4_200),
    falar(733, "O mapa do Ermo", "O Cartógrafo do porto desenhou a rota para o Ermo. Busque o mapa com ele.", Papel::Cartografo, 450, 4_000, item_id::GREATER_MANA, 3),
    nivel(734, "Alcance o nível 30", 30),
    ir(735, "De volta ao cais", "Com o mapa na mão, volte ao pátio do porto.", ponto::PORTO, 400, 4_000),
    viajar(736, "Rumo ao Ermo", "O Capitão do Porto leva você ao Ermo, onde o terceiro farol foi soterrado.", 2, 800, 6_000),
    // ═════════════ III · Areias que Gritam (Ermo, 28–42) ═════════════
    falar(737, "O calor do Ermo", "Areia até onde a vista alcança. Procure o Mestre de Missões na praça.", Papel::Missoes, 500, 5_500, item_id::GREATER_HEAL, 3),
    falar(738, "Água e remédio", "Sem água ninguém atravessa as dunas. Fale com o Alquimista.", Papel::Alquimista, 500, 5_500, item_id::GREATER_MANA, 3),
    cacar(739, "Magos da areia", "Magos dos Morganeers usam o trovão preso na areia. Derrote 10.", alvo_de_mob(mob_kind::MAGO), 10, 700, 7_500, item_id::GREATER_HEAL),
    coletar(740, "Vidro de relâmpago", "Onde o raio cai, a areia vira pedra. Quebre 25 pedras.", 25, 700, 8_000, VERDE, 6),
    falar(741, "Lâminas de vidro", "Leve o vidro ao Ferreiro. Dizem que corta tempestade.", Papel::Ferreiro, 600, 7_500, item_id::COPPER, 800),
    criar(742, "Arma do deserto", "Crie uma peça nova no Craft com o que o Ermo deu.", 700, 8_500),
    nivel(743, "Alcance o nível 35", 35),
    ir(744, "O farol soterrado", "O farol do Ermo está no topo das dunas. Suba ao mirante.", ponto::MIRANTE, 900, 10_000),
    cacar(745, "O cerco das dunas", "Os Morganeers querem o farol. Derrote 40 inimigos.", 0, 40, 1_100, 12_000, item_id::GREATER_HEAL),
    refinar(746, "Têmpera no calor", "Tente refinar duas vezes na Forja.", 2, 900, 11_000),
    falar(747, "A dança da tempestade", "O Treinador conhece o passo de quem luta dentro do vento. Aprenda com ele.", Papel::Treinador, 900, 11_000, item_id::GREATER_HEAL, 3),
    nivel(748, "Alcance o nível 40", 40),
    ir(749, "O naufrágio de Morgan", "A nau capitânia de Morgan jaz na costa mais distante. Vá até lá.", ponto::COSTA, 1_100, 13_000),
    falar(750, "O Capitão do Ermo", "No porto, o Capitão diz que o último farol fica no Planalto.", Papel::Estaleiro, 1_000, 12_000, item_id::GREATER_MANA, 3),
    viajar(751, "Rumo ao Planalto", "Embarque com o Capitão rumo ao Planalto, perto do olho da tempestade.", 3, 1_500, 18_000),
    // ═════════════ IV · O Coração da Tempestade (Planalto, 40–60) ═════════════
    falar(752, "Ar rarefeito", "No Planalto o vento corta. Apresente-se ao Mestre de Missões.", Papel::Missoes, 1_000, 14_000, item_id::GREATER_HEAL, 4),
    cacar(753, "Sentinelas do Planalto", "Arqueiros vigiam os penhascos. Derrote 12.", alvo_de_mob(mob_kind::ARQUEIRO), 12, 1_400, 18_000, item_id::GREATER_HEAL),
    coletar(754, "Pedra do céu", "As pedras do Planalto guardam o trovão mais forte. Quebre 30.", 30, 1_400, 19_000, AZUL, 4),
    falar(755, "A última runa", "O Identificador lê a runa gravada na pedra do céu.", Papel::Identificador, 1_200, 18_000, item_id::GREATER_MANA, 4),
    nivel(756, "Alcance o nível 45", 45),
    ir(757, "O farol do céu", "O último farol fica no ponto mais alto do arquipélago. Suba ao mirante.", ponto::MIRANTE, 1_800, 24_000),
    cacar(758, "A frota de Morgan", "A frota inteira dos Morganeers desembarcou. Derrote 50 inimigos.", 0, 50, 2_200, 30_000, item_id::GREATER_HEAL),
    criar(759, "Armadura para o olho", "Crie uma peça nova no Craft para enfrentar o olho da tempestade.", 1_800, 26_000),
    refinar(760, "Aço de trovão", "Tente refinar três vezes na Forja.", 3, 2_000, 28_000),
    nivel(761, "Alcance o nível 50", 50),
    falar(762, "Brinde aos faróis", "Três faróis voltaram a brilhar. O Taberneiro serve a rodada.", Papel::Taberna, 1_800, 26_000, item_id::GREATER_HEAL, 4),
    ir(763, "O olho no horizonte", "Da ponta do cais se vê o olho da tempestade girando. Vá até lá.", ponto::CAIS, 2_000, 30_000),
    nivel(764, "Alcance o nível 55", 55),
    cacar(765, "As feras do vento", "As feras do olho da tempestade descem ao Planalto. Derrote 15 owlbears.", alvo_de_mob(mob_kind::OWLBEAR), 15, 2_600, 36_000, item_id::GREATER_HEAL),
    falar(766, "O que há além", "O Cartógrafo quer desenhar o que existe além da tempestade.", Papel::Cartografo, 2_400, 34_000, item_id::GREATER_MANA, 4),
    nivel(767, "Alcance o nível 60", 60),
    falar(768, "O juramento do Guardião", "Os quatro faróis brilham. O Mestre de Missões tem um juramento para você.", Papel::Missoes, 4_000, 50_000, item_id::GREATER_HEAL, 5),
];

// ─────────────────────────── consultas ───────────────────────────

/// Quantos passos escritos existem.
pub fn total_escritos() -> u32 {
    PASSOS.len() as u32
}

/// `id` e' um passo da historia (escrito ou cronica)?
pub fn e_da_historia(id: u16) -> bool {
    id >= PRIMEIRO_ID_DO_EPILOGO || PASSOS.iter().any(|d| d.id == id)
}

/// O id do passo de indice `i`. `None` so' depois do ultimo id de cronica.
///
/// Os escritos andam pela POSICAO na lista, nao por `PRIMEIRO_ID + i`: e' o
/// que deixa os tutoriais (770+) entrarem no meio do capitulo I sem mudar o
/// id de passo nenhum. O jogador guarda o INDICE (a linha marcadora), entao
/// inserir passo desloca quem ja' passou dali — pro personagem novo, nada.
pub fn id_do_passo(i: u32) -> Option<u16> {
    let n = total_escritos();
    if i < n {
        return Some(PASSOS[i as usize].id);
    }
    let id = PRIMEIRO_ID_DO_EPILOGO as u32 + (i - n);
    (id <= u16::MAX as u32).then_some(id as u16)
}

/// O indice do passo `id`.
pub fn indice(id: u16) -> Option<u32> {
    if !e_da_historia(id) {
        return None;
    }
    if id >= PRIMEIRO_ID_DO_EPILOGO {
        return Some(total_escritos() + (id - PRIMEIRO_ID_DO_EPILOGO) as u32);
    }
    PASSOS.iter().position(|d| d.id == id).map(|i| i as u32)
}

/// A definicao do passo `id`, com vida estatica. As cronicas sao geradas uma
/// vez por id e guardadas: a mesma chamada devolve sempre a mesma referencia.
pub fn def_da_historia(id: u16) -> Option<&'static QuestDef> {
    let i = indice(id)?;
    if i < total_escritos() {
        return PASSOS.get(i as usize);
    }
    static CACHE: OnceLock<Mutex<HashMap<u16, &'static QuestDef>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?;
    if let Some(d) = cache.get(&id) {
        return Some(d);
    }
    let d: &'static QuestDef = Box::leak(Box::new(cronica(i - total_escritos())?));
    cache.insert(id, d);
    Some(d)
}

/// O capitulo escrito do passo `id`, se for um — pela POSICAO: o capitulo
/// vai do indice do `primeiro` ao do `ultimo` (os tutoriais moram no meio).
pub fn capitulo_escrito(id: u16) -> Option<&'static Capitulo> {
    let i = indice(id).filter(|i| *i < total_escritos())?;
    CAPITULOS.iter().find(|c| {
        matches!((indice(c.primeiro), indice(c.ultimo)), (Some(a), Some(b)) if (a..=b).contains(&i))
    })
}

/// Os passos escritos de um capitulo, na ordem.
pub fn passos_do_capitulo(c: &Capitulo) -> &'static [QuestDef] {
    match (indice(c.primeiro), indice(c.ultimo)) {
        (Some(a), Some(b)) => &PASSOS[a as usize..=b as usize],
        _ => &[],
    }
}

/// Nome do capitulo do passo de indice `i` ("I · O Farol do Bosque",
/// "Crônica 3").
pub fn nome_do_capitulo(i: u32) -> String {
    match id_do_passo(i).and_then(capitulo_escrito) {
        Some(c) => c.nome.to_string(),
        None => format!("Crônicas da Tempestade · {}", numero_da_cronica(i)),
    }
}

/// Numero (1..) da cronica do indice `i` (so' faz sentido no epilogo).
pub fn numero_da_cronica(i: u32) -> u32 {
    i.saturating_sub(total_escritos()) / PASSOS_POR_CRONICA + 1
}

/// Em que ilha o passo `id` acontece. Cronica: em qualquer uma (`None`).
pub fn zona_do_passo(id: u16) -> Option<&'static str> {
    capitulo_escrito(id)
        .and_then(|c| crate::terreno::ARQUIPELAGO.get(c.ilha))
        .map(|d| d.zona)
}

// ─────────────────────────── epilogo ───────────────────────────

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// O passo `k` (0..) das Cronicas da Tempestade. Deterministico: so' depende
/// de `k`. Seis passos por cronica — cacar, quebrar pedra, criar, refinar, ir
/// a um ponto-chave e a trava de nivel (depois do teto, mais uma caca).
fn cronica(k: u32) -> Option<QuestDef> {
    let id = PRIMEIRO_ID_DO_EPILOGO as u32 + k;
    if id > u16::MAX as u32 {
        return None;
    }
    let id = id as u16;
    let c = k / PASSOS_POR_CRONICA;
    let n = c + 1;
    let j = k % PASSOS_POR_CRONICA;
    let gold = 3_000u32.saturating_add(600u32.saturating_mul(c));
    let xp = 40_000u64.saturating_add(8_000u64.saturating_mul(c as u64));
    let trava = 60u64 + 5 * n as u64;
    let escolhe = |v: &[&'static str]| v[(c as usize) % v.len()];
    let def = match j {
        0 => {
            let t = escolhe(&[
                "A caçada sem fim",
                "Feras da maré alta",
                "O rastro do trovão",
                "Sombras na mata",
            ]);
            let q = 40 + 5 * (c % 12);
            QuestDef {
                title: leak(format!("Crônica {n} · {t}")),
                desc: leak(format!("A tempestade não dorme e os bichos não param. Derrote {q} inimigos de qualquer tipo.")),
                ..cacar(id, "", "", 0, q, gold, xp, item_id::GREATER_HEAL)
            }
        }
        1 => {
            let t = escolhe(&[
                "Pedras que cantam",
                "O veio da tormenta",
                "Minério do relâmpago",
            ]);
            let q = 30 + 2 * (c % 15);
            QuestDef {
                title: leak(format!("Crônica {n} · {t}")),
                desc: leak(format!(
                    "Os faróis bebem pedra de trovão. Quebre {q} pedras em qualquer veio."
                )),
                ..coletar(id, "", "", q, gold, xp, AZUL, 4 + (c % 4) as u16)
            }
        }
        2 => {
            let t = escolhe(&["Uma peça para a jornada", "Ferro forjado no vento"]);
            QuestDef {
                title: leak(format!("Crônica {n} · {t}")),
                desc: "Os guardiões precisam de equipamento novo. Crie uma peça no Craft.",
                ..criar(id, "", "", gold, xp)
            }
        }
        3 => {
            let t = escolhe(&["Fogo mais quente", "O brilho do aço"]);
            let vezes = 1 + (c % 3);
            QuestDef {
                title: leak(format!("Crônica {n} · {t}")),
                desc: leak(format!("Aço que enfrenta a tempestade é aço refinado. Tente refinar {vezes} vez(es) na Forja.")),
                ..refinar(id, "", "", vezes, gold, xp)
            }
        }
        4 => {
            let (p, t, d) = match c % 3 {
                0 => (
                    ponto::MIRANTE,
                    "Vigia do mirante",
                    "Suba ao mirante da ilha em que estiver e confira se o farol ainda brilha.",
                ),
                1 => (
                    ponto::COSTA,
                    "A costa distante",
                    "Barcos estranhos rondam a costa mais distante da ilha. Vá ver.",
                ),
                _ => (
                    ponto::CAIS,
                    "Sinais no cais",
                    "O Capitão acendeu um sinal na ponta do cais. Vá até lá.",
                ),
            };
            QuestDef {
                title: leak(format!("Crônica {n} · {t}")),
                ..ir(id, "", d, p, gold, xp)
            }
        }
        _ => {
            if trava <= crate::constants::CHAR_LEVEL_CAP as u64 {
                QuestDef {
                    title: leak(format!("Crônica {n} · Alcance o nível {trava}")),
                    ..nivel(id, "", trava as u32)
                }
            } else {
                let q = 60;
                QuestDef {
                    title: leak(format!("Crônica {n} · Guardião da Tempestade")),
                    desc: leak(format!(
                        "No teto do poder, a guarda nunca acaba. Derrote {q} inimigos."
                    )),
                    ..cacar(id, "", "", 0, q, gold, xp, item_id::GREATER_HEAL)
                }
            }
        }
    };
    Some(def)
}

// ─────────────────────────── falas ───────────────────────────

/// As falas do dialogo de um passo escrito com NPC (conversa e viagem).
pub fn falas(id: u16, m: u8) -> Option<Vec<&'static str>> {
    if !e_da_historia(id) {
        return None;
    }
    if m != momento::CONVERSA {
        return Some(vec![def_da_historia(id).map_or("…", |d| d.desc)]);
    }
    let f: &[&str] = match id {
        700 => &[
            "Você está vivo! O mar trouxe você na noite em que o farol apagou.",
            "Há gerações quatro faróis de pedra-trovão seguram a Grande Tempestade longe das ilhas.",
            "O do Bosque se apagou. Os bichos enlouquecem e os Morganeers já sentiram o cheiro.",
            "Primeiro, cuide de você: o Alquimista tem o que você precisa.",
        ],
        701 => &[
            "O Mestre mandou você? Então é o náufrago do farol.",
            "Sem poção, a mata engole qualquer um. Tome estas, e beba antes de cair — não depois.",
            "Os lobos estão estranhos. Vá ver a trilha, mas com cuidado.",
        ],
        703 => &[
            "Ouvi falar dos lobos. Nada mal para quem chegou boiando.",
            "Guarde isto: quintessência. Armadura nenhuma fecha sem ela.",
            "A forja precisa de lenha e o Ferreiro paga bem por ela. Vá derrubar umas árvores.",
        ],
        706 => &[
            "Pedra que canta? Deixe eu ouvir… É trovão preso no metal.",
            "Esse metal é o que endurece os bichos. E também faz boa armadura.",
            "Toda peça pede um couro curtido para segurar o metal. Tome o meu — é o último, e some depois de uma forja.",
            "Outro só arrancando de chefe: o do Porão do Naufrágio carrega chave no bolso.",
            "Tome o cobre também. Com o que você juntou, já dá uma armadura. Vá ao Craft.",
        ],
        714 => &[
            "Então você viu o farol apagado do mirante. Eu vi também.",
            "Os Peacemain guardam os portos e os faróis. Os Morganeers querem a tempestade para eles.",
            "Se os faróis caírem, a tempestade engole as ilhas uma a uma.",
            "Vá até a ponta do cais comigo ao amanhecer. Vou mostrar a rota.",
        ],
        718 | 736 | 751 => &[
            "O barco está pronto e a maré ajuda.",
            "Suba a bordo. Do outro lado, procure o Mestre de Missões da praça.",
            "Que os faróis guiem você.",
        ],
        719 => &[
            "Mais um que o Capitão mandou. Bem-vindo à Geleira.",
            "Nosso farol ainda brilha, mas o gelo está rachando a pedra.",
            "O Taberneiro sabe onde os Morganeers se escondem na neve.",
        ],
        720 => &[
            "Senta, esquenta as mãos. Morganeers? Chegaram com a nevasca.",
            "Os corujursos descem das trilhas atrás deles, famintos.",
            "E a pedra daqui prende relâmpago. Eles querem essa pedra.",
        ],
        723 => &[
            "Deixe eu ver… Este fragmento é um pedaço do farol.",
            "Alguém está arrancando pedras do farol congelado.",
            "Suba lá antes que ele caia.",
        ],
        729 => &[
            "Lã-de-tempestade: fio que segura o vento. Não é para qualquer um.",
            "Você salvou nosso farol. Esta capa é por minha conta.",
            "O Cartógrafo do porto tem algo para você.",
        ],
        733 => &[
            "O Ermo fica ao sul, depois do mar de areia molhada.",
            "O farol de lá foi soterrado. Morgan esteve lá primeiro.",
            "Leve o mapa. Volte ao porto quando estiver pronto.",
        ],
        737 => &[
            "Você chegou vivo ao Ermo. Poucos chegam.",
            "O farol daqui está debaixo das dunas, e os magos de Morgan cavam dia e noite.",
            "Fale com o Alquimista antes de pisar na areia.",
        ],
        738 => &[
            "Água, sal e remédio: é o que mantém alguém vivo aqui.",
            "Os magos usam o trovão da areia. Cuidado com os raios.",
            "Tome isto e vá.",
        ],
        741 => &[
            "Vidro de relâmpago… Afiado como nada que eu já forjei.",
            "Com isso dá para cortar o vento da tempestade.",
            "Leve cobre e crie sua arma.",
        ],
        747 => &[
            "Quem luta dentro do vento não pode ficar parado.",
            "Mova-se com a tempestade, não contra ela.",
            "Você está pronto para o que vem.",
        ],
        750 => &[
            "O farol do Ermo voltou a brilhar. Eu vi do mar.",
            "Mas o olho da tempestade se aproximou do Planalto.",
            "O último farol fica lá. Eu levo você.",
        ],
        752 => &[
            "O ar é fino aqui em cima. Respire devagar.",
            "Daqui dá para ver o olho da tempestade girando.",
            "O último farol é o mais alto de todos. Vai precisar de força.",
        ],
        755 => &[
            "Esta runa diz: “quem acende o farol do céu vira guardião”.",
            "Morgan leu a mesma runa. Ele quer ser o guardião da tempestade.",
            "Suba antes dele.",
        ],
        762 => &[
            "Três faróis acesos! A casa paga a rodada.",
            "Dizem que Morgan desembarcou com a frota inteira.",
            "Beba. Amanhã o vento vai soprar forte.",
        ],
        766 => &[
            "Ninguém nunca desenhou o que existe além da tempestade.",
            "Com os quatro faróis acesos, talvez o mar se abra.",
            "Quando estiver pronto, o Mestre espera você.",
        ],
        768 => &[
            "Os quatro faróis brilham. Você fez o que nenhum náufrago fez.",
            "Mas a tempestade não morre: ela dorme e acorda.",
            "Faça o juramento dos Guardiões: vigiar os faróis enquanto houver vento.",
            "Suas crônicas começam agora.",
        ],
        _ => return Some(vec![def_da_historia(id).map_or("…", |d| d.desc)]),
    };
    Some(f.to_vec())
}

// ─────────────────────────── pontos-chave ───────────────────────────

/// Onde fica o ponto-chave `p` numa ilha. Funcao pura do relevo e da vila —
/// o servidor passa a `Ilha`, o teste passa o `Gerador`.
///
/// `cidade` e' o centro da praca; `porto` = (patio, ponta do cais); `raio_ilha`
/// em unidades; `altura`/`agua` na coordenada de mundo.
pub fn ponto_da_historia(
    p: u16,
    cidade: Option<glam::Vec2>,
    porto: Option<(glam::Vec2, glam::Vec2)>,
    raio_ilha: f32,
    altura: &dyn Fn(f32, f32) -> f32,
    agua: &dyn Fn(f32, f32) -> bool,
) -> Option<glam::Vec2> {
    use glam::Vec2;
    let c = cidade?;
    match p {
        ponto::CIDADE => Some(c),
        ponto::PORTO => porto.map(|x| x.0),
        ponto::CAIS => porto.map(|x| x.1),
        ponto::SAIDA => {
            let dir = porto
                .map(|x| (x.0 - c).normalize_or_zero())
                .filter(|d| *d != Vec2::ZERO)
                .unwrap_or(Vec2::X);
            let s = c + dir * (crate::terreno::Cidade::RAIO + 12.0);
            Some(if agua(s.x, s.y) { c } else { s })
        }
        ponto::MIRANTE => {
            // Grade grossa: o cume mais alto fora da cidade, dentro da ilha.
            let passos = 48i32;
            let lado = raio_ilha * 0.85;
            let mut melhor: Option<(f32, Vec2)> = None;
            for gz in -passos..=passos {
                for gx in -passos..=passos {
                    let q = Vec2::new(gx as f32, gz as f32) * (lado / passos as f32);
                    if q.length() > lado
                        || q.distance(c) < crate::terreno::Cidade::RAIO + 30.0
                        || agua(q.x, q.y)
                    {
                        continue;
                    }
                    let h = altura(q.x, q.y);
                    if melhor.is_none_or(|(m, _)| h > m) {
                        melhor = Some((h, q));
                    }
                }
            }
            melhor.map(|(_, q)| q)
        }
        ponto::COSTA => {
            // Em 24 rumos a partir da praca: a terra firme que vai mais longe.
            let mut melhor: Option<(f32, Vec2)> = None;
            for k in 0..24 {
                let a = k as f32 / 24.0 * std::f32::consts::TAU;
                let dir = Vec2::new(a.cos(), a.sin());
                let mut ultima = c;
                let mut d = 6.0;
                while d < raio_ilha * 1.2 {
                    let q = c + dir * d;
                    if agua(q.x, q.y) {
                        break;
                    }
                    ultima = q;
                    d += 6.0;
                }
                // Um pouco pra dentro da beira: chegar sem molhar o pe'.
                let fim = c + (ultima - c) * 0.96;
                let dist = fim.distance(c);
                if melhor.is_none_or(|(m, _)| dist > m) {
                    melhor = Some((dist, fim));
                }
            }
            melhor.map(|(_, q)| q)
        }
        _ => None,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::terreno::{Gerador, ARQUIPELAGO, BLOCO, ESCALA_ALTURA};

    #[test]
    fn ids_seguidos_e_capitulos_cobrem_tudo() {
        let mut vistos = std::collections::HashSet::new();
        let mut escrito = PRIMEIRO_ID;
        for d in PASSOS.iter() {
            assert!(vistos.insert(d.id), "{} repetido", d.id);
            if (PRIMEIRO_ID_TUTORIAL..=ULTIMO_ID_TUTORIAL).contains(&d.id) {
                assert_eq!(d.obj_kind, objective_kind::TUTORIAL, "{} na faixa de tutorial", d.id);
            } else {
                // Os de sempre seguem 700, 701, 702... com tutorial no meio.
                assert_eq!(d.id, escrito, "passo fora de ordem");
                escrito += 1;
            }
            assert_eq!(indice(d.id).and_then(id_do_passo), Some(d.id));
            assert_eq!(d.source, quest_source::HISTORIA);
            assert!(
                crate::quests::QUESTS.iter().all(|q| q.id != d.id),
                "{} colide com uma missao",
                d.id
            );
            assert!(capitulo_escrito(d.id).is_some(), "{} sem capitulo", d.id);
            assert!(
                !d.title.is_empty() && !d.desc.is_empty(),
                "{} sem texto",
                d.id
            );
        }
        assert_eq!(PASSOS.last().unwrap().id, CAPITULOS.last().unwrap().ultimo);
        let cobertos: usize = CAPITULOS.iter().map(|c| passos_do_capitulo(c).len()).sum();
        assert_eq!(cobertos, PASSOS.len(), "passo fora de capitulo");
        for c in CAPITULOS {
            let n = passos_do_capitulo(c).len();
            assert!((12..=26).contains(&n), "{}: {n} passos", c.nome);
        }
        assert!(!e_da_historia(ID_MARCO));
    }

    /// Travas crescem, ficam na faixa da ilha e nunca passam do teto; toda
    /// viagem leva pra ilha seguinte e fecha o capitulo.
    #[test]
    fn travas_e_viagens_coerentes() {
        let mut ultima = 0;
        for d in PASSOS {
            if d.obj_kind == objective_kind::NIVEL {
                assert!(d.obj_count > ultima, "{}: trava fora de ordem", d.id);
                ultima = d.obj_count;
                let ilha = &ARQUIPELAGO[capitulo_escrito(d.id).unwrap().ilha];
                assert!(
                    d.obj_count <= ilha.nivel.1 + 5 && d.obj_count >= ilha.nivel.0,
                    "{}: nivel {} fora da ilha {:?}",
                    d.id,
                    d.obj_count,
                    ilha.nivel
                );
            }
            if d.obj_kind == objective_kind::VIAGEM {
                let c = capitulo_escrito(d.id).unwrap();
                assert_eq!(d.id, c.ultimo, "viagem no meio do capitulo");
                assert_eq!(d.obj_target as usize, c.ilha + 1);
            }
        }
    }

    /// Todo passo escrito tem destino valido NA ilha dele: o NPC existe na
    /// vila, o bicho nasce na faixa de nivel, o ponto-chave cai em terra.
    #[test]
    fn todo_destino_escrito_existe_na_ilha() {
        for cap in CAPITULOS {
            let def = &ARQUIPELAGO[cap.ilha];
            let ger = Gerador::da_ilha(def);
            let papeis: Vec<u16> = ger.vila().npcs.iter().map(|n| n.papel as u16).collect();
            let porto = ger.vila().porto.as_ref().map(|p| (p.centro, p.ponta));
            let cidade = ger.cidade().map(|c| c.centro());
            let raio = def.raio_blocos as f32 * BLOCO;
            let altura = |x: f32, z: f32| ger.altura(x, z);
            let agua = |x: f32, z: f32| ger.altura(x, z) <= crate::terreno::NIVEL_DO_MAR + 0.01;
            let mut pontos: HashMap<u16, glam::Vec2> = HashMap::new();
            for d in passos_do_capitulo(cap) {
                let id = d.id;
                match d.obj_kind {
                    objective_kind::TALK => {
                        assert!(
                            papeis.contains(&d.obj_target),
                            "{id}: NPC {} nao existe na vila de {}",
                            d.obj_target,
                            def.zona
                        );
                        let f = falas(id, momento::CONVERSA).unwrap();
                        assert!(
                            !f.is_empty() && f[0] != d.desc,
                            "{id}: conversa sem falas escritas"
                        );
                    }
                    objective_kind::VIAGEM => {
                        assert!(
                            papeis.contains(&(Papel::Estaleiro as u16)),
                            "{id}: sem Capitao do Porto"
                        );
                    }
                    objective_kind::KILL if d.obj_target != 0 => {
                        let kind = d.obj_target - 1;
                        assert!(
                            (kind as u32) < def.nivel.1 / 3 + 1,
                            "{id}: kind {kind} nao nasce ate' o nivel {}",
                            def.nivel.1
                        );
                    }
                    objective_kind::LUGAR => {
                        let q = *pontos.entry(d.obj_target).or_insert_with(|| {
                            ponto_da_historia(d.obj_target, cidade, porto, raio, &altura, &agua)
                                .unwrap_or_else(|| {
                                    panic!("{id}: {} sem posicao", ponto::nome(d.obj_target))
                                })
                        });
                        assert!(
                            !agua(q.x, q.y),
                            "{id}: {} na agua em {q:?}",
                            ponto::nome(d.obj_target)
                        );
                        println!("{}: {} em {q:?}", def.zona, ponto::nome(d.obj_target));
                    }
                    _ => {}
                }
            }
        }
    }

    /// O capitulo I leva o nivel sozinho: so' a XP dos passos (sem contar
    /// bicho) ja' passa o nivel minimo de cada dungeon ANTES do passo dela, e
    /// o 15 da Geleira antes do barco. Trava de nivel no capitulo I nao ha':
    /// era ela que parava o jogador no 704 pedindo ~450 lobos.
    #[test]
    fn capitulo_um_carrega_o_nivel_das_dungeons() {
        let mult = crate::constants::DEFAULT_XP_MULTIPLIER;
        let cap = CAPITULOS[0];
        let zona = ARQUIPELAGO[cap.ilha].zona;
        let mut xp = 0u64;
        let mut dungeons = 0;
        for d in passos_do_capitulo(&cap) {
            let id = d.id;
            assert_ne!(
                d.obj_kind,
                objective_kind::NIVEL,
                "{id}: trava de nivel no capitulo I"
            );
            let nivel = crate::constants::level_of_xp_with_mult(xp, mult);
            if d.obj_kind == objective_kind::DUNGEON {
                let c = crate::dungeon::conteudo(d.obj_target)
                    .unwrap_or_else(|| panic!("{id}: dungeon {} nao existe", d.obj_target));
                assert!(
                    c.disponivel && c.zona == zona,
                    "{id}: {} nao esta' aberta em {zona}",
                    c.nome
                );
                assert!(
                    nivel >= c.nivel_min,
                    "{id}: chega no nivel {nivel}, {} pede {}",
                    c.nome,
                    c.nivel_min
                );
                dungeons += 1;
            }
            if d.obj_kind == objective_kind::VIAGEM {
                let destino = &ARQUIPELAGO[d.obj_target as usize];
                assert!(
                    nivel >= destino.nivel.0,
                    "{id}: embarca no nivel {nivel}, {} comeca no {}",
                    destino.nome,
                    destino.nivel.0
                );
            }
            xp += d.reward_xp;
        }
        assert!(dungeons >= 2, "o capitulo I tem que levar a dungeon");
    }

    /// Os passos ANTES do craft entregam a receita inteira da primeira
    /// armadura cinza: sem isso o "crie sua primeira peca" pede farm que a
    /// historia nunca apresentou.
    #[test]
    fn o_inicio_entrega_a_primeira_armadura() {
        let receita = crate::receitas::receitas_de_equipamento()
            .into_iter()
            .find(|r| r.tier == 1 && r.category == crate::receitas::categoria::ARMADURA)
            .unwrap();
        let mut ganho: HashMap<u32, u32> = HashMap::new();
        let ate = indice(PASSO_DO_CRAFT).unwrap() as usize;
        for d in &PASSOS[..ate] {
            for (item, qtd) in [
                (d.reward_item, d.reward_item_qty),
                (d.reward_item2, d.reward_item2_qty),
            ] {
                if item != 0 {
                    *ganho.entry(item as u32).or_default() += qtd as u32;
                }
            }
        }
        for [item, qtd] in receita.inputs {
            let tem = ganho.get(&item).copied().unwrap_or(0);
            assert!(
                tem >= qtd,
                "{}: a historia da' {tem} de {item}, a receita pede {qtd}",
                receita.name
            );
        }
    }

    /// A chave mora no passo ANTERIOR ao craft. Se alguem tirar a chave dali,
    /// ou renumerar a historia, o passo de criar volta a pedir algo que o
    /// jogador nao tem como fazer — e isso nao aparece em lugar nenhum ate'
    /// alguem travar no meio do capitulo I. Este teste cai primeiro.
    #[test]
    fn a_chave_vem_no_passo_antes_do_craft() {
        let craft = def_da_historia(PASSO_DO_CRAFT).unwrap();
        assert_eq!(
            craft.obj_kind,
            objective_kind::CRAFT,
            "{PASSO_DO_CRAFT} deixou de ser o passo de criar"
        );
        let chave = indice(PASSO_DO_CRAFT)
            .and_then(|i| id_do_passo(i - 1))
            .and_then(def_da_historia)
            .unwrap();
        assert_eq!(
            chave.obj_kind,
            objective_kind::TALK,
            "o passo da chave deixou de ser conversa"
        );
        assert_eq!(
            chave.obj_target,
            Papel::Ferreiro as u16,
            "a chave saiu do Ferreiro"
        );
        let chaves = crate::item_id::todas_as_chaves();
        assert!(
            chaves.contains(&chave.reward_item) || chaves.contains(&chave.reward_item2),
            "o passo {} parou de entregar chave: o craft do {PASSO_DO_CRAFT} fica impossivel",
            chave.id
        );
    }

    /// Os tutoriais ensinam cada acao UMA vez, no capitulo I, e cada um tem
    /// o texto do que fazer.
    #[test]
    fn tutoriais_no_capitulo_um_uma_acao_cada() {
        use crate::quests::tutorial as t;
        let tut: Vec<&QuestDef> = PASSOS
            .iter()
            .filter(|d| d.obj_kind == objective_kind::TUTORIAL)
            .collect();
        let acoes: Vec<u16> = tut.iter().map(|d| d.obj_target).collect();
        for a in [
            t::POCAO_LIMIAR,
            t::SKILL_AUTO,
            t::AUTO_COMBATE,
            t::AUTO_COLETA,
            t::MAPA_IR,
            t::COLETA_ENERGIA,
            t::PONTO_ATRIBUTO,
            t::EVOLUIR_SKILL,
        ] {
            assert_eq!(acoes.iter().filter(|x| **x == a).count(), 1, "acao {a}");
            assert_ne!(t::instrucao(a), "Siga a dica");
        }
        // Todo tutorial mora num capitulo — o da hora em que aquilo passa a
        // fazer falta. Quase todos no I; o despertar de skill espera a
        // Energia juntar, e so' aparece no II.
        for d in &tut {
            let cap = capitulo_escrito(d.id).map(|c| c.nome);
            assert!(cap.is_some(), "{} sem capitulo", d.id);
            if d.obj_target == t::EVOLUIR_SKILL {
                assert_eq!(cap, Some(CAPITULOS[1].nome), "{} devia estar no II", d.id);
            } else {
                assert_eq!(cap, Some(CAPITULOS[0].nome), "{} fora do capitulo I", d.id);
            }
        }
        // A poção vem logo depois do Alquimista dar as pocoes, e o auto
        // combate antes dos lobos.
        let pos = |id: u16| indice(id).unwrap();
        assert_eq!(pos(770), pos(701) + 1);
        assert!(pos(771) < pos(702));
        assert!(pos(772) > pos(703) && pos(773) < pos(704) && pos(774) < pos(709));
        // O ponto de atributo vem cedo, antes do primeiro bando de lobos; a
        // Energia junto da coleta; o despertar depois da travessia.
        // Gastar ponto CUSTA Energia (`custo_energia_do_ponto`): o passo que
        // manda gastar tem que vir depois do que manda coletar, senao ele
        // pede uma coisa que o jogador ainda nao tem como fazer. Foi
        // exatamente isso que o dono encontrou jogando, em 20/09/2026.
        assert!(pos(776) < pos(775), "Energia antes do ponto de atributo");
        assert!(pos(776) < pos(704));
        assert!(pos(777) > pos(718));
    }

    /// Cronicas: estaveis (mesmo id, mesma referencia e mesmo texto),
    /// infinitas na pratica, e sem trava acima do teto.
    #[test]
    fn cronicas_estaveis_e_sem_fim() {
        let n = total_escritos();
        for i in [0, 1, n - 1, n, n + 5, n + 6, n + 1_000, n + 50_000] {
            let id = id_do_passo(i).unwrap();
            assert_eq!(indice(id), Some(i));
            let a = def_da_historia(id).unwrap();
            let b = def_da_historia(id).unwrap();
            assert!(std::ptr::eq(a, b), "{id}: gerou duas vezes");
            assert_eq!(a.id, id);
            assert!(!a.title.is_empty());
        }
        assert_eq!(cronica(7).unwrap().title, cronica(7).unwrap().title);
        assert_eq!(id_do_passo(n), Some(PRIMEIRO_ID_DO_EPILOGO));
        assert!(id_do_passo(n + 55_535).is_some() && id_do_passo(n + 55_536).is_none());
        for k in 0..600 {
            let d = cronica(k).unwrap();
            if d.obj_kind == objective_kind::NIVEL {
                assert!(
                    d.obj_count <= crate::constants::CHAR_LEVEL_CAP,
                    "trava {} acima do teto",
                    d.obj_count
                );
                assert_eq!(k % PASSOS_POR_CRONICA, PASSOS_POR_CRONICA - 1);
            }
        }
        // A primeira trava do epilogo vem depois da ultima escrita.
        let ultima_escrita = PASSOS
            .iter()
            .filter(|d| d.obj_kind == objective_kind::NIVEL)
            .map(|d| d.obj_count)
            .max()
            .unwrap();
        assert!(cronica(PASSOS_POR_CRONICA - 1).unwrap().obj_count > ultima_escrita);
        assert_eq!(nome_do_capitulo(0), CAPITULOS[0].nome);
        assert!(nome_do_capitulo(n + 7).contains("· 2"));
        // Missoes de area pagam Pocao de Experiencia.
        assert_eq!(cronica(0).unwrap().reward_item2, item_id::XP_POTION);
        assert_eq!(
            crate::quests::quest_by_id(PRIMEIRO_ID).unwrap().title,
            PASSOS[0].title
        );
    }
}
