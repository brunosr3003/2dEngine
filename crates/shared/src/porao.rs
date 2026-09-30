//! O PORÃO como dungeon FÍSICA: uma porta no cenário, aberta com uma chave
//! que se fabrica.
//!
//! Até 29/09/2026 o Porão e a Gruta entravam pelo mesmo lugar — viajar até a
//! Arena e clicar no painel — e se separavam só na conta de entradas. O dono
//! quis os dois completamente apartados: "i want the 'porao' to be a fisical
//! dungeon, in some places of the scenario, and need a key to enter, the key
//! wyll be made whit craft - wood and some material of the lvl of the dungeon,
//! increasing the amount of wood necessary to do it and the material tier".
//!
//! Então o Porão perde o painel e ganha uma porta na própria ilha. Quem chega
//! perto e tem a chave entra; quem não tem, não. E entra QUANTAS VEZES quiser,
//! com recompensa cheia todas elas — a antiga cota de 3 por dia
//! (`PORAO_RECOMPENSAS_POR_DIA`) deixa de existir.
//!
//! ## A chave é o balanceamento inteiro
//!
//! Isto é o que mais importa neste arquivo. Enquanto havia cota diária, o teto
//! do que o Porão despeja na economia era um número fixo: 3 baús. Sem ela, o
//! teto passa a ser a velocidade com que o jogador fabrica chave — ou seja, a
//! velocidade com que junta madeira e material. A receita não é sabor, é a
//! torneira: cada chave a menos de custo é um baú a mais por hora no jogo
//! inteiro (`dungeon::ouro_do_bau`, `dungeon::marcas`, cobre e material).
//!
//! Por isso a quantidade de madeira e o tier do material sobem com o nível do
//! conteúdo, e por isso existe teste medindo que sobem mesmo.

use glam::Vec2;

use crate::constants::item_id;
use crate::dungeon::{Conteudo, Tipo};

/// A primeira chave de Porão. Os ids seguem a ordem de `dungeon::CONTEUDOS`.
///
/// 480 porque o catálogo de itens ia até 475 (`MOEDA_MAGICA`) — é a primeira
/// faixa livre, e ficar colado no fim evita buraco no meio.
pub const CHAVE_BASE: u16 = 480;

/// A chave deste Porão. `None` para Gruta e Caçada, que não têm porta.
///
/// O id sai do ID DO CONTEÚDO, e não da posição dele no catálogo. A primeira
/// versão contava a ordem (`position`), e isso é uma armadilha: acrescentar um
/// Porão no meio da lista renumerava em silêncio a chave de todos os que vêm
/// depois — e chave é item de bolsa, que jogador guarda. O conteúdo já tem id
/// estável, salvo em `DadosDungeon`; a chave anda colada nele.
pub fn chave_de(c: &Conteudo) -> Option<u16> {
    (c.tipo == Tipo::Porao).then(|| CHAVE_BASE + c.id)
}

/// O conteúdo que esta chave abre.
///
/// UMA chave por Porão, e não uma por faixa: o dono escolheu assim em
/// 29/09/2026. Custa um item novo por dungeon, e em troca nenhuma chave abre
/// uma porta para a qual não foi feita — que é o que dá sentido a fabricar a
/// chave do lugar aonde se quer ir.
pub fn porao_da_chave(item: u16) -> Option<&'static Conteudo> {
    let id = item.checked_sub(CHAVE_BASE)?;
    crate::dungeon::CONTEUDOS
        .iter()
        .find(|c| c.tipo == Tipo::Porao && c.id == id)
}

/// O nome da chave, na língua de origem (o inglês).
pub fn nome_da_chave(item: u16) -> Option<String> {
    porao_da_chave(item).map(|c| format!("{} Key", c.nome))
}

// ─────────────────────────────── a receita ───────────────────────────────

/// O que uma chave custa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Receita {
    /// `WOOD_T1..WOOD_T4`, pelo nível do conteúdo.
    pub madeira: u16,
    pub madeira_qtd: u32,
    /// Cobre ou darksteel, na cor da faixa do conteúdo.
    pub material: u16,
    pub material_qtd: u32,
}

/// O tier da madeira pelo nível, na MESMA regra do loot
/// (`constants.rs`: 1-15→t1, 16-30→t2, 31-50→t3, 51+→t4).
///
/// Repetir a regra aqui com outros cortes seria ter duas verdades sobre o que
/// é "madeira do nível 22".
pub fn madeira_do_nivel(nivel: u32) -> u16 {
    item_id::WOOD_T1 + (tier_da_madeira(nivel) - 1) as u16
}

/// Quanta madeira, pelo nível do conteúdo.
///
/// Sobe com o nível porque foi o pedido — "increasing the amount of wood
/// necessary to do it" — e porque o Porão de nível alto paga mais por baú
/// (`dungeon::ouro_do_bau` é 150 + 25·nível): se o custo não subisse junto, a
/// chave mais barata do jogo abriria a porta mais lucrativa.
pub fn madeira_qtd(nivel: u32) -> u32 {
    let t = segundos_alvo(nivel) * REPARTE_MADEIRA;
    (t / crate::constants::COLETA_CICLO_ARVORE_S * rende_por_ciclo(tier_da_madeira(nivel))).round()
        as u32
}

/// QUANTOS SEGUNDOS DE COLETA A CHAVE DEVE CUSTAR — e o resto sai daqui.
///
/// A quantidade de madeira e de aço deixou de ser escrita à mão em 29/09/2026
/// e passou a ser DERIVADA deste alvo. O motivo é que a quantidade sozinha não
/// diz nada: 22 de madeira parece um custo até se descobrir que uma árvore
/// derruba 3 a 5 por ciclo, e aí viram cinco segundos. Foi exatamente esse o
/// erro — o modelo supunha um item por ciclo e errava por quatro.
///
/// Escrevendo o TEMPO, o custo continua verdadeiro quando o rendimento do nó
/// mudar: a receita se ajusta sozinha.
///
/// A escada (3 min no nível 6, 11 min no 50) é a que o dono escolheu em
/// 29/09/2026, entre quatro formas de segurar a torneira: "keys cost ~4x,
/// chest gold UNCHANGED". O Porão passa a ser um laço de quem coleta — junta
/// um tanto, entra uma vez.
pub fn segundos_alvo(nivel: u32) -> f32 {
    180.0 + 10.9 * (nivel.max(6) - 6) as f32
}

/// Quanto do tempo da chave é madeira; o resto é aço.
const REPARTE_MADEIRA: f32 = 0.6;

/// O material caro da chave: AÇO, na cor da faixa do conteúdo.
///
/// Aço e não cobre/darksteel, e isto tem motivo escrito em `constants.rs`:
/// "Darksteel e Copper variam com o NIVEL do item, nao com a cor". Os dois são
/// itens avulsos e vizinhos — `COPPER` é 344 e `DARKSTEEL` é 345 —, então
/// `na_cor(COPPER, 3)` não dá "cobre tier 3", dá 346, que é GLITTERING_POWDER.
/// Foi exatamente o erro da primeira versão desta função: a receita pedia Pó
/// Cintilante achando que pedia cobre graúdo.
///
/// `STEEL` é uma das bases que TÊM as quatro cores (300-303), que é o que o
/// pedido precisa: "increasing ... the material tier".
pub fn material_do_nivel(nivel: u32) -> u16 {
    item_id::na_cor(item_id::STEEL, cor_do_nivel(nivel))
}

/// A cor do aço pedido, 1 a 4, pelo nível do conteúdo.
///
/// Faixa própria e não `dungeon::teto_de_grau`: aquele é o teto de RAREZA do
/// que a dungeon paga, e ele devolve a mesma cor para os níveis 6, 14 e 22 —
/// os três Porões que existem. Usá-lo deixava a "material tier" parada no 302
/// nos três, que é o contrário do pedido.
pub fn cor_do_nivel(nivel: u32) -> u8 {
    match nivel {
        0..=9 => 1,
        10..=19 => 2,
        20..=34 => 3,
        _ => 4,
    }
}

/// O TERMO QUADRÁTICO NÃO É ENFEITE.
///
/// A corrida custa o mesmo tempo em todo Porão (`limite_s` é 600 nos cinco),
/// então um custo de chave que crescesse só em linha com o ouro do baú
/// deixaria o ouro POR HORA subir com o nível — o de cima pagaria muito mais
/// por hora que o de baixo, e ninguém olharia pros outros. O `n²` é o que
/// segura isso; ver `a_torneira_do_porao_fica_dentro_do_combinado`.
pub fn material_qtd(nivel: u32) -> u32 {
    let tier = cor_do_nivel(nivel).clamp(1, 4);
    let t = segundos_alvo(nivel) * (1.0 - REPARTE_MADEIRA);
    (t / crate::constants::COLETA_CICLO_PEDRA_S[tier as usize] * rende_por_ciclo(tier)).round()
        as u32
}

/// A receita da chave deste Porão.
pub fn receita_de(c: &Conteudo) -> Option<Receita> {
    if c.tipo != Tipo::Porao {
        return None;
    }
    let n = c.nivel_min;
    Some(Receita {
        madeira: madeira_do_nivel(n),
        madeira_qtd: madeira_qtd(n),
        material: material_do_nivel(n),
        material_qtd: material_qtd(n),
    })
}

// ─────────────────────────────── a porta ───────────────────────────────

/// A que distância da porta o jogador pode abri-la.
///
/// Quem confere é o SERVIDOR (`world::dungeon`), nunca o cliente: a porta é
/// uma entrada de conteúdo, e posição vinda do cliente é posição que se mente.
pub const ALCANCE_DA_PORTA: f32 = 4.0;

/// Onde fica a porta deste Porão, em coordenadas de mundo.
///
/// Sai da CIDADE da ilha e não de um par de números escrito à mão: o relevo é
/// gerado por semente, e uma coordenada fixa que hoje cai em chão bom pode
/// cair na água quando a ilha for regerada.
///
/// ## A âncora sozinha não bastava
///
/// A primeira versão escolhia um ângulo fixo por Porão e parava aí. Ancorar na
/// cidade garante que a porta ande junto com ela, mas não garante NADA sobre o
/// que tem naquele ângulo — e no Ermo o ângulo apontava para o mar. A porta da
/// Caravana Afundada nasceu na água, a 2 unidades abaixo do nível do mar, onde
/// jogador nenhum ia chegar. Ela foi publicada assim.
///
/// Agora a porta PROCURA chão: varre ângulos a partir do preferido e distâncias
/// a partir de 56 u, e fica no primeiro ponto que está acima do nível do mar em
/// si e na volta toda. A varredura é determinística — mesma ilha, mesma porta —,
/// e é a mesma nos dois lados porque os dois passam o mesmo `Gerador`.
pub fn porta_de(c: &Conteudo, ger: &crate::terreno::Gerador) -> Option<Vec2> {
    if c.tipo != Tipo::Porao {
        return None;
    }
    let cidade = ger.cidade()?.centro();
    let i = crate::dungeon::CONTEUDOS
        .iter()
        .filter(|o| o.tipo == Tipo::Porao && o.zona == c.zona)
        .position(|o| o.id == c.id)? as f32;
    let preferido = 0.7 + i * 2.1;
    // Nem em cima do platô da cidade (RAIO 42) nem longe demais pra se achar.
    for passo_d in 0..8 {
        let d = 56.0 + passo_d as f32 * 9.0;
        for passo_a in 0..24 {
            // Abre em leque a partir do ângulo preferido: +0, +15°, -15°, …
            let giro = (passo_a as f32 / 2.0).ceil() * 0.262
                * if passo_a % 2 == 0 { 1.0 } else { -1.0 };
            let a = preferido + giro;
            let p = cidade + Vec2::new(a.cos(), a.sin()) * d;
            if firme(ger, p) {
                return Some(p);
            }
        }
    }
    None
}

/// Este ponto aguenta uma porta? Chão seco nele e nos quatro lados, pra ela
/// não nascer na beirada com metade no ar.
fn firme(ger: &crate::terreno::Gerador, p: Vec2) -> bool {
    let seco = |q: Vec2| ger.altura(q.x, q.y) > crate::terreno::NIVEL_DO_MAR;
    seco(p)
        && [
            Vec2::new(2.5, 0.0),
            Vec2::new(-2.5, 0.0),
            Vec2::new(0.0, 2.5),
            Vec2::new(0.0, -2.5),
        ]
        .iter()
        .all(|d| seco(p + *d))
}

/// Todos os Porões desta zona, com a chave e a receita de cada um.
/// As portas desta zona, já procuradas no relevo: `(id do conteúdo, posição)`.
///
/// CALCULE UMA VEZ E GUARDE — E O `Gerador` TAMBÉM. Medido em release em
/// 29/09/2026: a busca das portas custa 5 µs, mas `Gerador::da_ilha` custa
/// **24,6 ms**, porque `novo` procura a cidade e o porto no relevo inteiro. Um
/// quadro a 60 fps tem 16,7 ms. O cliente 2026092904 construía um `Gerador`
/// DUAS VEZES POR QUADRO (prop da porta e tarja) e derrubou o jogo pra 15-20
/// fps em toda plataforma — foi o "laggy after 16:00" que os jogadores
/// relataram. Quem chamar isto guarda o resultado por zona.
pub fn portas_da_zona(zona: &str, ger: &crate::terreno::Gerador) -> Vec<(u16, Vec2)> {
    poroes_da_zona(zona)
        .into_iter()
        .filter_map(|c| porta_de(c, ger).map(|p| (c.id, p)))
        .collect()
}

pub fn poroes_da_zona(zona: &str) -> Vec<&'static Conteudo> {
    crate::dungeon::CONTEUDOS
        .iter()
        .filter(|c| c.tipo == Tipo::Porao && c.zona == zona && c.disponivel)
        .collect()
}

// ───────────────────────── a torneira, medida ─────────────────────────

/// Quantos segundos de coleta uma chave custa.
///
/// ⚠️ ISTO É UM MODELO, E NÃO UMA MEDIÇÃO. Uma coisa aqui é suposição e está
/// dita de propósito: **um item por ciclo de coleta**. O rendimento de verdade
/// mora na tabela de loot do banco (`economy::farm_node_loot`), que nenhum
/// teste alcança, então o número abaixo é um PISO — se um tronco der duas
/// madeiras, a chave sai na metade do tempo daqui.
///
/// O que o modelo serve pra pegar é a MUDANÇA: no dia em que alguém cortar o
/// custo da chave pela metade, a conta muda junto e o guarda fala.
///
/// Os ciclos saem de `constants`: `COLETA_CICLO_ARVORE_S` e
/// `COLETA_CICLO_PEDRA_S[tier]`.
pub fn segundos_de_coleta(nivel: u32) -> f32 {
    let t_madeira = tier_da_madeira(nivel);
    let ciclos_madeira = madeira_qtd(nivel) as f32 / rende_por_ciclo(t_madeira);
    let madeira = ciclos_madeira * crate::constants::COLETA_CICLO_ARVORE_S;
    // O aço vem de PEDRA, e a pedra do tier da faixa.
    let t_pedra = cor_do_nivel(nivel).clamp(1, 4);
    let ciclos_pedra = material_qtd(nivel) as f32 / rende_por_ciclo(t_pedra);
    let pedra = ciclos_pedra * crate::constants::COLETA_CICLO_PEDRA_S[t_pedra as usize];
    madeira + pedra
}

/// O TIER DA MADEIRA QUE UMA ÁRVORE DÁ, pelo nível do lugar onde ela cresce.
///
/// Até 29/09/2026 TODA árvore do jogo dava Madeira T1 (`coletar_plantado`
/// fixava `tier_material = 1` pro tronco), em qualquer ilha. A tabela de loot
/// já tinha as linhas de T2 a T4 pra árvore, e nada as sorteava — então as
/// chaves de Porão que pedem Madeira T2/T3 (Frozen Hull, Sunken Caravan,
/// Thunder Vault) não se fabricavam com madeira de árvore. O dono: "now you
/// need to create the next tiers of trees".
///
/// A regra é a MESMA de `tier_da_madeira`, aplicada ao nível do LUGAR: a faixa
/// da ilha (`DefIlha::nivel`), ou, no Planalto, a da região da trilha em que a
/// árvore está — que é o único lugar do jogo com nível 51+, e por isso o único
/// com Madeira T4. Bosque dá T1, Geleira T2, Ermo T3, e o Planalto T3 no começo
/// da trilha e T4 no fundo dela.
pub fn tier_da_arvore(
    def: &crate::terreno::DefIlha,
    planalto: Option<&crate::planalto::Plano>,
    p: Vec2,
) -> u8 {
    let (a, b) = planalto.map_or(def.nivel, |pl| pl.faixa(p));
    tier_da_madeira((a + b) / 2)
}

/// Quanto um nó de coleta daquele tier rende POR CICLO, em média.
///
/// ⚠️ ESTE NÚMERO CORRIGE UM ERRO. A primeira versão do modelo supunha UM item
/// por ciclo, e a suposição estava escrita como tal — mas estava errada por um
/// fator de quatro, e o guarda inteiro media uma torneira que não existe.
///
/// O número de verdade está no seed de `farm_node_drops`
/// (`persistence.rs`), com a regra escrita ao lado: `qty_base = 2 + tier` e
/// um extra de 0 a 2. A média é, portanto, `3 + tier`.
///
/// Fica aqui como espelho por um motivo chato: a tabela real mora numa função
/// async do crate `server`, que este crate não enxerga. Espelho mente com o
/// tempo, então quem mexer num dos dois tem que mexer no outro — é o mesmo
/// arranjo do simulador da escada, e vale o mesmo cuidado.
pub fn rende_por_ciclo(tier: u8) -> f32 {
    3.0 + tier.clamp(1, 4) as f32
}

/// O tier (1-4) da madeira daquele nível, na regra do loot.
pub fn tier_da_madeira(nivel: u32) -> u8 {
    match nivel {
        0..=15 => 1,
        16..=30 => 2,
        31..=50 => 3,
        _ => 4,
    }
}

/// Segundos de uma corrida do Porão, no ritmo que o próprio jogo chama de bom.
///
/// 60% do limite é o corte de `dungeon::bonus_tempo` — matar o chefe antes
/// disso paga bônus. Usar o limite cheio fingiria que todo mundo joga no
/// talo do cronômetro; usar 60% é o número que o jogo já escolheu pra dizer
/// "correu bem".
pub fn segundos_de_corrida(c: &Conteudo) -> f32 {
    c.limite_s as f32 * 0.6
}

/// Ouro por hora que este Porão despeja, contando a chave E a corrida.
///
/// É o número que a cota diária escondia: enquanto havia 3 baús por dia, o
/// teto era 3 × `ouro_do_bau`. Agora é isto, e não tem teto nenhum além do
/// tempo do jogador.
pub fn ouro_por_hora(c: &Conteudo) -> f32 {
    let ciclo = segundos_de_coleta(c.nivel_min) + segundos_de_corrida(c);
    let ouro = crate::dungeon::ouro_do_bau(c.tipo, c.nivel_min, false) as f32;
    ouro * 3600.0 / ciclo
}

#[cfg(test)]
mod testes {
    use super::*;

    /// TODO PORÃO TEM PORTA, CHAVE E RECEITA — e nenhum outro tipo tem.
    ///
    /// É o que segura o "separar completamente": no dia em que alguém
    /// acrescentar um Porão ao catálogo sem chave, ele vira uma dungeon sem
    /// entrada nenhuma, porque o painel não o oferece mais.
    #[test]
    fn todo_porao_tem_chave_e_receita_e_so_o_porao_tem() {
        for c in crate::dungeon::CONTEUDOS {
            let chave = chave_de(c);
            let receita = receita_de(c);
            if c.tipo == Tipo::Porao {
                let k = chave.unwrap_or_else(|| panic!("{} ficou sem chave", c.nome));
                assert_eq!(
                    porao_da_chave(k).map(|o| o.id),
                    Some(c.id),
                    "{}: a chave {k} não volta pro porão dela",
                    c.nome
                );
                let r = receita.unwrap_or_else(|| panic!("{} ficou sem receita", c.nome));
                assert!(r.madeira_qtd > 0 && r.material_qtd > 0, "{}", c.nome);
            } else {
                assert!(chave.is_none(), "{} não é Porão e ganhou chave", c.nome);
                assert!(receita.is_none(), "{} não é Porão e ganhou receita", c.nome);
            }
        }
    }

    /// A CHAVE FICA MAIS CARA CONFORME A PORTA PAGA MAIS.
    ///
    /// Sem cota diária, a receita é o único freio do que o Porão despeja na
    /// economia. Se o custo não subisse com o nível, a porta que paga
    /// `150 + 25·nível` de ouro sairia pelo preço da que paga menos, e o jogo
    /// inteiro farmaria só a última.
    #[test]
    fn a_chave_encarece_com_o_nivel_do_porao() {
        let poroes: Vec<_> = crate::dungeon::CONTEUDOS
            .iter()
            .filter(|c| c.tipo == Tipo::Porao)
            .collect();
        assert!(poroes.len() >= 2, "o teste precisa de dois porões");
        for par in poroes.windows(2) {
            let (a, b) = (par[0], par[1]);
            assert!(
                b.nivel_min > a.nivel_min,
                "o catálogo deixou de estar em ordem de nível"
            );
            let (ra, rb) = (receita_de(a).unwrap(), receita_de(b).unwrap());
            assert!(
                rb.madeira_qtd > ra.madeira_qtd,
                "{} pede {} de madeira e {} pede {}: não subiu",
                a.nome,
                ra.madeira_qtd,
                b.nome,
                rb.madeira_qtd
            );
            assert!(
                rb.material_qtd > ra.material_qtd,
                "{} → {}: o material não subiu",
                a.nome,
                b.nome
            );
            assert!(
                rb.madeira >= ra.madeira,
                "{} → {}: o tier da madeira desceu",
                a.nome,
                b.nome
            );
            assert!(
                rb.material >= ra.material,
                "{} → {}: o tier do material desceu",
                a.nome,
                b.nome
            );
        }
    }

    /// AS PORTAS DA MESMA ILHA NÃO NASCEM UMA EM CIMA DA OUTRA.
    #[test]
    fn portas_da_mesma_ilha_ficam_separadas() {
        for zona in ["ilha_inicial", "ilha_gelo"] {
            let Some(def) = crate::terreno::def_da_zona(zona) else { continue };
            let ger = crate::terreno::Gerador::da_ilha(def);
            let cidade = ger.cidade().map(|c| c.centro()).unwrap_or_default();
            let portas: Vec<Vec2> = portas_da_zona(zona, &ger).into_iter().map(|(_, p)| p).collect();
            for (i, a) in portas.iter().enumerate() {
                assert!(
                    a.distance(cidade) > crate::terreno::Cidade::RAIO,
                    "{zona}: uma porta caiu dentro do platô da cidade"
                );
                for b in portas.iter().skip(i + 1) {
                    assert!(
                        a.distance(*b) > ALCANCE_DA_PORTA * 4.0,
                        "{zona}: duas portas a {:.1} uma da outra",
                        a.distance(*b)
                    );
                }
            }
        }
    }

    /// A tabela que o dono vai ler: o que cada chave custa de verdade.
    #[test]
    fn mostra_o_custo_de_cada_chave() {
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let r = receita_de(c).unwrap();
            println!(
                "{:<20} lv{:<3} chave={} | madeira id={} x{:<4} material id={} x{}",
                c.nome, c.nivel_min, chave_de(c).unwrap(),
                r.madeira, r.madeira_qtd, r.material, r.material_qtd
            );
        }
    }

    /// O MATERIAL DA CHAVE É UMA COR DE VERDADE, e não o item do vizinho.
    ///
    /// `na_cor(base, cor)` é só `base + cor - 1`: passar uma base que não tem
    /// as quatro cores devolve, sem reclamar, o item seguinte do catálogo. A
    /// primeira versão desta receita pedia `na_cor(COPPER, 3)` = 346 =
    /// GLITTERING_POWDER, que o próprio `constants.rs` diz não entrar em craft
    /// nenhum. Este teste prende as duas pontas: a cor sai dentro da faixa do
    /// aço, e nunca encosta num material que não seja aço.
    #[test]
    fn o_material_da_chave_e_aco_de_verdade() {
        let faixa = item_id::STEEL..item_id::STEEL + 4;
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let m = receita_de(c).unwrap().material;
            assert!(
                faixa.contains(&m),
                "{}: material {m} caiu fora do aço ({:?})",
                c.nome,
                faixa
            );
            assert_ne!(m, item_id::GLITTERING_POWDER, "{}: pediu pó", c.nome);
            assert_ne!(m, item_id::DARKSTEEL, "{}", c.nome);
        }
        // E a madeira tem que ser madeira.
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let w = receita_de(c).unwrap().madeira;
            assert!(
                (item_id::WOOD_T1..=item_id::WOOD_T4).contains(&w),
                "{}: madeira {w} não é madeira",
                c.nome
            );
        }
    }

    /// O TIER DO MATERIAL SOBE DE PONTA A PONTA, e não só "não desce".
    ///
    /// O pedido foi "increasing the amount of wood necessary to do it and the
    /// material tier". Uma versão desta receita lia a cor de
    /// `dungeon::teto_de_grau`, que devolve a MESMA cor pros níveis 6, 14 e 22:
    /// os três Porões pediam aço 302 e o tier não subia em lugar nenhum. Um
    /// teste de "não desceu" passa feliz nisso; este não.
    #[test]
    fn o_tier_do_material_sobe_do_primeiro_porao_ao_ultimo() {
        let poroes: Vec<_> = crate::dungeon::CONTEUDOS
            .iter()
            .filter(|c| c.tipo == Tipo::Porao)
            .collect();
        let primeiro = receita_de(poroes.first().unwrap()).unwrap().material;
        let ultimo = receita_de(poroes.last().unwrap()).unwrap().material;
        assert!(
            ultimo > primeiro,
            "o aço do primeiro Porão ({primeiro}) e o do último ({ultimo}) são o mesmo tier"
        );
        // E a cor anda mesmo ao longo da faixa de nível que o jogo usa.
        assert_eq!(cor_do_nivel(6), 1);
        assert_eq!(cor_do_nivel(14), 2);
        assert_eq!(cor_do_nivel(22), 3);
        assert_eq!(cor_do_nivel(50), 4);
    }

    /// A TORNEIRA DO PORÃO, MEDIDA E PRESA.
    ///
    /// Este é o guarda que a fase 4 existia pra escrever. Enquanto houve cota
    /// diária, o que o Porão despejava na economia era 3 baús por dia por
    /// personagem, e ponto. Tirada a cota, o teto virou o tempo do jogador —
    /// e, porque a chave é vendável no mercado, o teto do SERVIDOR virou a
    /// madeira e o aço que o servidor inteiro junta. Um gatherer abastece
    /// muitos corredores.
    ///
    /// Os três limites abaixo são escolhas, e estão aqui pra serem discutidas
    /// quando alguém mexer no custo — não pra passarem despercebidas.
    #[test]
    fn a_torneira_do_porao_fica_dentro_do_combinado() {
        let poroes: Vec<_> = crate::dungeon::CONTEUDOS
            .iter()
            .filter(|c| c.tipo == Tipo::Porao)
            .collect();

        for c in &poroes {
            let coleta = segundos_de_coleta(c.nivel_min);
            // 1. A CHAVE NÃO PODE SER DE GRAÇA. Menos de um minuto de coleta e
            //    ela deixa de ser freio: vira uma formalidade entre corridas.
            assert!(
                coleta >= 60.0,
                "{}: a chave custa só {coleta:.0}s de coleta",
                c.nome
            );
            // 2. NEM PODE SER UMA PAREDE. O teto era 600s e subiu pra 720 em
            //    29/09/2026, quando o dono escolheu a forma "keys cost ~4x,
            //    chest gold UNCHANGED" entre as quatro que seguravam a
            //    torneira. Nessa forma a coleta DOMINA de propósito — 11
            //    minutos juntando pros 6 de dungeon no topo da escada —, e um
            //    teto de 10 minutos proibiria a própria decisão.
            assert!(
                coleta <= 720.0,
                "{}: a chave custa {coleta:.0}s de coleta, virou parede",
                c.nome
            );
        }

        // 3. NENHUM DEGRAU PODE VALER MUITO MAIS QUE O ANTERIOR.
        //
        //    Aqui havia um limite de 2x entre o Porão mais rico e o mais
        //    pobre do catálogo inteiro, e ele foi AFROUXADO em 29/09/2026,
        //    quando o catálogo passou de três Porões (níveis 6 a 22) para
        //    cinco (6 a 50). Vale dizer por que, porque afrouxar o próprio
        //    guarda é exatamente o que não se deve fazer sem motivo:
        //
        //    comparar as pontas de uma escada de 44 níveis é comparar dois
        //    conteúdos que personagem nenhum escolhe entre si. Ninguém de
        //    nível 50 pondera o Porão de nível 6. A pergunta de funil que o
        //    jogador realmente faz é "vale a pena pular pro próximo?", e ela
        //    é entre VIZINHOS.
        //
        //    O limite absoluto (item 4) é que passou a segurar o topo, e ele
        //    não foi afrouxado. Os dois juntos são mais apertados que o 2x
        //    sozinho era: o 2x não dizia nada sobre o valor absoluto.
        let por_hora: Vec<f32> = poroes.iter().map(|c| ouro_por_hora(c)).collect();
        for (i, par) in por_hora.windows(2).enumerate() {
            assert!(
                par[1] <= par[0] * 1.6,
                "{} paga {:.0} ouro/h e {} paga {:.0}: o degrau é grande demais",
                poroes[i].nome,
                par[0],
                poroes[i + 1].nome,
                par[1]
            );
        }
        let maior = por_hora.iter().cloned().fold(0.0f32, f32::max);

        // 4. E O JOGO INTEIRO TEM UM TETO. Este é o número que substitui a
        //    cota: com ele, uma hora de Porão vale menos que uma entrada
        //    comprada de Gruta no nível 20 (`preco_da_compra`, base 2.500).
        assert!(
            maior <= 6_000.0,
            "o Porão está despejando {maior:.0} ouro por hora"
        );
    }

    /// A tabela que o dono lê pra decidir se o custo está certo.
    #[test]
    fn mostra_a_torneira() {
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let r = receita_de(c).unwrap();
            println!(
                "{:<20} lv{:<3} chave={:>3}s corrida={:>3}s  ouro/baú={:<5} ouro/h={:>6.0}  ({}x madeira + {}x aço)",
                c.nome,
                c.nivel_min,
                segundos_de_coleta(c.nivel_min) as i32,
                segundos_de_corrida(c) as i32,
                crate::dungeon::ouro_do_bau(c.tipo, c.nivel_min, false),
                ouro_por_hora(c),
                r.madeira_qtd,
                r.material_qtd,
            );
        }
    }

    /// TODA ILHA TEM PELO MENOS UM PORÃO.
    ///
    /// Pedido do dono em 29/09/2026: "make at least 1 dungeuns per map". Até
    /// ali o Ermo e o Planalto não tinham dungeon física nenhuma — quem
    /// passasse do nível 30 ficava sem porta pra abrir, e a chave que tivesse
    /// fabricado não servia pra ilha em que estava.
    ///
    /// O teste anda com o `ARQUIPELAGO`: ilha nova entra aqui sozinha e cobra
    /// o Porão dela.
    #[test]
    fn toda_ilha_do_arquipelago_tem_porao() {
        for def in crate::terreno::ARQUIPELAGO {
            let poroes = poroes_da_zona(def.zona);
            assert!(
                !poroes.is_empty(),
                "{} ({}) não tem Porão nenhum",
                def.nome,
                def.zona
            );
            // E o nível dele tem que fazer sentido pra quem está na ilha: nem
            // abaixo do começo dela, nem acima do fim.
            for p in &poroes {
                assert!(
                    p.nivel_min + 4 >= def.nivel.0 && p.nivel_min <= def.nivel.1,
                    "{}: o Porão {} é nível {} e a ilha é {:?}",
                    def.nome,
                    p.nome,
                    p.nivel_min,
                    def.nivel
                );
            }
        }
    }

    /// A CHAVE NÃO SE RENUMERA QUANDO O CATÁLOGO CRESCE.
    ///
    /// A primeira versão de `chave_de` contava a POSIÇÃO do Porão na lista, e
    /// os dois Porões novos entraram no meio dela. Com a conta antiga, a chave
    /// do Frozen Hull teria trocado de id sozinha — e chave é item de bolsa.
    #[test]
    fn a_chave_segue_o_id_do_conteudo_e_nao_a_ordem() {
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            assert_eq!(chave_de(c), Some(CHAVE_BASE + c.id));
        }
        // Ida e volta pra todas, e nenhuma chave serve pra duas portas.
        let mut vistas = std::collections::HashSet::new();
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let k = chave_de(c).unwrap();
            assert!(vistas.insert(k), "a chave {k} abre mais de um Porão");
            assert_eq!(porao_da_chave(k).map(|o| o.id), Some(c.id));
        }
    }

    /// A CHAVE TEM QUE CABER NA BOLSA.
    ///
    /// A madeira empilha até 999 (`persistence`, `stack: 999`), e a chave do
    /// Thunder Vault pede 1187 — mais de uma pilha cheia. Isso é aceitável, e
    /// está aqui escrito pra ser uma decisão e não uma surpresa: a receita
    /// ocupa DUAS vagas de madeira, e o `tirar_item` do servidor já varre
    /// várias vagas. O que não pode é passar de um punhado de vagas, senão a
    /// bolsa inteira vira depósito de uma chave só.
    #[test]
    fn a_chave_nao_toma_a_bolsa_inteira() {
        const PILHA: u32 = 999;
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let r = receita_de(c).unwrap();
            let vagas = r.madeira_qtd.div_ceil(PILHA) + r.material_qtd.div_ceil(PILHA);
            assert!(
                vagas <= 4,
                "{}: a receita ocupa {vagas} vagas de bolsa ({} madeira + {} aço)",
                c.nome,
                r.madeira_qtd,
                r.material_qtd
            );
        }
    }

    /// NENHUMA PORTA NASCE NO MAR.
    ///
    /// Este teste existe porque aconteceu. A primeira versão de `porta_de`
    /// escolhia um ângulo fixo a 56 u da cidade e não olhava o chão: no Ermo
    /// esse ângulo apontava pro oceano, e a porta da Caravana Afundada foi
    /// publicada a duas unidades ABAIXO do nível do mar, onde jogador nenhum
    /// chegaria. O dono encontrou do jeito mais caro — procurando no jogo.
    #[test]
    fn nenhuma_porta_nasce_na_agua() {
        for def in crate::terreno::ARQUIPELAGO {
            let ger = crate::terreno::Gerador::da_ilha(&def);
            let portas = portas_da_zona(def.zona, &ger);
            assert_eq!(
                portas.len(),
                poroes_da_zona(def.zona).len(),
                "{}: alguma porta não achou chão nenhum",
                def.zona
            );
            for (id, p) in portas {
                let alt = ger.altura(p.x, p.y);
                assert!(
                    alt > crate::terreno::NIVEL_DO_MAR,
                    "{}: a porta de {} ficou em {alt:.2}, no mar",
                    def.zona,
                    crate::dungeon::conteudo(id).map_or("?", |c| c.nome)
                );
                // E firme na volta toda, pra não nascer pendurada na beirada.
                assert!(firme(&ger, p), "{}: porta na beirada", def.zona);
            }
        }
    }

    /// CADA ILHA DÁ A MADEIRA DO SEU NÍVEL, e toda chave tem de onde tirar a
    /// dela.
    #[test]
    fn cada_ilha_da_a_madeira_do_seu_nivel_e_toda_chave_acha_madeira() {
        use crate::terreno::{def_da_zona, Gerador};
        let tier = |zona: &str| {
            let def = def_da_zona(zona).unwrap();
            let ger = Gerador::da_ilha(def);
            let cidade = ger.cidade().map(|c| c.centro()).unwrap_or_default();
            tier_da_arvore(def, ger.planalto(), cidade)
        };
        assert_eq!(tier("ilha_inicial"), 1, "Bosque");
        assert_eq!(tier("ilha_gelo"), 2, "Geleira");
        assert_eq!(tier("ilha_deserto"), 3, "Ermo");
        // No Planalto o tier sobe pela trilha: o fundo tem que dar T4.
        let def = def_da_zona("ilha_planalto").unwrap();
        let ger = Gerador::da_ilha(def);
        let pl = ger.planalto().expect("o Planalto tem trilha");
        let tiers: Vec<u8> = pl.regioes.iter().map(|r| tier_da_arvore(def, Some(pl), r.centro)).collect();
        assert_eq!(tiers.first(), Some(&3), "começo da trilha: {tiers:?}");
        assert_eq!(tiers.last(), Some(&4), "fundo da trilha: {tiers:?}");

        // E toda chave pede uma madeira que alguma árvore do jogo dá.
        let mut da: std::collections::HashSet<u16> = std::collections::HashSet::new();
        for z in ["ilha_inicial", "ilha_gelo", "ilha_deserto"] {
            da.insert(item_id::WOOD_T1 + (tier(z) - 1) as u16);
        }
        for t in &tiers {
            da.insert(item_id::WOOD_T1 + (*t - 1) as u16);
        }
        for c in crate::dungeon::CONTEUDOS.iter().filter(|c| c.tipo == Tipo::Porao) {
            let m = receita_de(c).unwrap().madeira;
            assert!(da.contains(&m), "{}: pede madeira {m} que nenhuma árvore dá", c.nome);
        }
    }
}
