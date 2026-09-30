//! A ILHOTA DA DUNGEON: a zona própria onde TODA instância acontece.
//!
//! O dono: "quero que a dungeon seja instância isolada na verdade, multi
//! server ainda por cima" — e, sobre o terreno, "só criar uma ilhota pequena
//! e simples pra dungeon".
//!
//! ## Por que uma zona só, e não salas espalhadas
//!
//! A `mesa` (fila e salas) vivia dentro de cada processo de zona. Com
//! `ilha_inicial`, `ilha_gelo` e as três mágicas em processos separados, cada
//! um enxergava só as próprias salas: o dono criou uma sala com um
//! personagem e não a viu com o outro, porque estavam em zonas diferentes.
//!
//! Havia dois caminhos. Um era publicar as salas num banco e sincronizar —
//! duas listas que podem discordar, e o clássico "entrei numa sala que já não
//! existe". O outro, escolhido pelo dono, é este: **um processo só hospeda
//! todas as instâncias**. A mesa passa a viver em um lugar porque só há um
//! lugar, e o multi-server sai de graça — não por sincronia, mas por não
//! haver o que sincronizar.
//!
//! É o mesmo desenho da Ilha Mágica, que já é zona própria.
//!
//! ## Por que a ilhota é simples
//!
//! A arena nasce de sítios planos do relevo. Aqui o relevo não precisa contar
//! história nenhuma: ninguém explora a ilhota da dungeon, ninguém coleta nela,
//! ninguém a vê do mapa-mundi. Ela existe para ter CHÃO PLANO suficiente para
//! as instâncias caberem lado a lado sem se ver.
//!
//! Por isso ela é um disco baixo e liso, sem cidade, sem porto e **sem
//! recurso** — árvore e pedra viram estorvo, e estorvo numa arena é o A*
//! contornando o que deveria ser espaço de luta. A mesma razão pela qual a
//! ilhota do Colosso ficou limpa.

use crate::terreno::{Bioma, DefIlha};

/// O nome da zona. Um processo com `MMO_ZONA=dungeon` a serve.
pub const ZONA: &str = "dungeon";

/// Raio da ilhota, em BLOCOS. 200 blocos = 100 unidades.
///
/// As instâncias são separadas por `Instancia(id)` e não se veem, então o
/// tamanho não cresce com o número de salas. Quem manda no tamanho é OUTRA
/// coisa, e ela não é óbvia:
///
/// O servidor escolhe os sítios de instância do mais LONGE do porto pra
/// dentro (`dg_arena`), e num lugar sem porto isso vira "do mais longe do
/// centro" — ou seja, na borda do chão plano. O andar tem raio 55. Então o
/// piso de um andar sempre transborda o platô em ~30 u, e isso não tem
/// conserto por tamanho: a conta é a mesma em qualquer escala.
///
/// O que conserta é a ilhota continuar SECA além do platô. Por isso há três
/// raios, e não um: 62 de chão plano (onde o andar assenta), 92 de terra
/// firme (onde o andar inteiro cabe), e este, a ilha toda.
///
/// ENCOLHEU de 420 a pedido do dono: "a arena de dungeon tá desnecessariamente
/// grande". Estava dimensionada pra quatro sítios espalhados numa planície
/// larga; quatro sítios a 70 u de distância cabem num platô bem menor.
///
/// GREW AGAIN on 30/09/2026, to 640 (320 u): the five Porões are carved into
/// the islet now (`planta`), on a ring at 200 u, each in its own rock mass,
/// and the islet needs the plain to stand them on. The middle stays the Gruta
/// floors' flat ground, exactly as before.
pub const RAIO_BLOCOS: i32 = 640;

/// Semente fixa: a ilhota é a mesma toda vez, em todo realm.
///
/// Arena que muda de forma entre servidores seria injusta — o mesmo chefe,
/// dois terrenos diferentes. E uma semente sorteada faria o teste de "cabe
/// uma arena aqui" provar coisa diferente a cada execução.
pub const SEMENTE: i32 = 77_321;

/// A altura do PLATÔ, em unidades de mundo.
///
/// Dezesseis, igual à colônia: alto o bastante pra a ilhota ter silhueta e
/// não ler como jangada, baixo o bastante pra a rampa até a orla não virar
/// paredão.
pub const ALTURA: f32 = 16.0;

/// Até onde o chão é PERFEITAMENTE plano, em unidades.
///
/// É esta a razão de ser da ilhota. O servidor escolhe quatro sítios de
/// instância separados por 70 u (`dg_arena`), e um sítio só vale se o terreno
/// em volta dele for plano num raio de 9 u. Num relevo sorteado — que foi a
/// primeira tentativa — a ilhota inteira comportava DOIS sítios, e nem
/// aumentar o raio nem trocar a semente mudava isso: o ruído não faz planície
/// por encomenda.
///
/// Desenhar resolve por construção. 90 u de raio plano bastam pros quatro
/// sítios a 70 u — `cabem_os_quatro_sitios_do_rodizio` confere.
pub const RAIO_PLANO: f32 = 285.0;

/// Onde o chão acaba, em unidades. Entre ele e `RAIO_PLANO` desce a rampa.
///
/// Cinquenta e cinco unidades de rampa, e isso NÃO é enfeite: o sítio de
/// instância mais afastado fica perto da borda do platô e o andar tem raio 55,
/// então o piso dele transborda. Terra firme bem além do platô é o que impede
/// a luta de acontecer em cima do mar.
/// `o_andar_inteiro_cai_em_terra_firme` é quem confere.
pub const RAIO_TERRA: f32 = 315.0;

/// O que há FORA da ilhota, em índice de bloco. Fundo, pra ler como mar.
pub const NIVEL_FUNDO: i32 = -64;

/// O bloco de topo da coluna. A ÚNICA fonte do relevo da Arena.
///
/// Sem ruído nenhum, e de propósito: o dono pediu "uma ilhota pequena e
/// simples pra dungeon", e aqui simples é REQUISITO, não gosto — ondulação
/// de meio bloco já reprova um sítio de instância.
///
/// Anel de nível não é problema como seria numa ilha que se explora: o platô
/// é de altura única (nenhuma fronteira de bloco pra cruzar) e a rampa é
/// curta. Ninguém vem à Arena olhar a paisagem.
pub fn bloco_da_coluna(bx: i32, bz: i32) -> i32 {
    let (x, z) = (
        bx as f32 * crate::terreno::BLOCO,
        bz as f32 * crate::terreno::BLOCO,
    );
    let d = (x * x + z * z).sqrt();
    if d >= RAIO_TERRA {
        return NIVEL_FUNDO;
    }
    // THE PORÕES are carved into the plain (`planta`): rock, castle and
    // tomb standing on it, the halls dug into them.
    if d <= RAIO_PLANO {
        let chao = (ALTURA / crate::terreno::BLOCO).round() as i32 - 1;
        let p = glam::Vec2::new(x, z);
        if let Some(b) = crate::planta::complexo_em(p).and_then(|pl| pl.bloco(p, chao)) {
            return b;
        }
    }
    let h = if d <= RAIO_PLANO {
        ALTURA
    } else {
        // Rampa reta até logo acima do mar: 16 u de queda em 60 dá 0,27 u por
        // unidade, ou 0,13 de degrau por bloco — muito abaixo do bloco
        // inteiro que o passo vence. A orla é andável, e não paredão.
        let t = (d - RAIO_PLANO) / (RAIO_TERRA - RAIO_PLANO);
        ALTURA + (0.8 - ALTURA) * t
    };
    (h / crate::terreno::BLOCO).round() as i32 - 1
}

/// Onde se chega. O relevo é um disco liso e o meio é o ponto mais fundo
/// dentro da terra, longe da borda de água por construção.
///
/// Constante, e não "a terra seca mais próxima calculada na hora": montar o
/// relevo custa caro e o handoff acontece no meio do tick. O teste abaixo é
/// quem garante que o ponto é chão.
pub const CHEGADA: glam::Vec2 = glam::Vec2::ZERO;

pub fn e_arena(zona: &str) -> bool {
    zona == ZONA
}

pub const DEF: DefIlha = DefIlha {
    zona: ZONA,
    nome: "Arena",
    semente: SEMENTE,
    raio_blocos: RAIO_BLOCOS,
    bioma: Bioma::Floresta,
    // Longe do arquipélago e da Ilha Mágica no mapa-mundi. Ninguém navega até
    // aqui — só se chega por handoff —, mas duas zonas no mesmo ponto
    // confundiriam qualquer tela que desenhe o mundo.
    centro: [-1800.0, 2600.0],
    // A faixa não decide nada aqui: quem manda no nível do bicho da instância
    // é o conteúdo da dungeon (`shared::dungeon`), não o relevo. Fica estreita
    // e baixa para o caso de alguma zona comum nascer por engano.
    nivel: (1, 3),
};

#[cfg(test)]
mod testes {
    use super::*;

    /// O PLATÔ CABE UM ANDAR INTEIRO, e a terra firme cabe com folga.
    ///
    /// É a única coisa que dimensiona esta ilhota. Ela já foi de 210 u de raio
    /// porque o servidor exigia quatro sítios de instância a 70 u de
    /// distância — regra do mundo aberto, onde duas dungeons no mesmo lugar
    /// seriam duas hordas empilhadas. Aqui elas não se veem, então podem
    /// dividir o mesmo chão, e o que sobra é caber UM andar.
    #[test]
    fn o_plato_cabe_um_andar() {
        assert!(
            RAIO_PLANO >= RAIO_DO_ANDAR,
            "o platô ({RAIO_PLANO}) não cabe o andar ({RAIO_DO_ANDAR})"
        );
        // A terra tem que ir ALÉM do andar posto na pior posição que o
        // servidor escolheria — um sítio na borda do plano.
        let pior = (RAIO_PLANO - 9.0) + RAIO_DO_ANDAR;
        assert!(
            RAIO_TERRA >= pior * 0.8,
            "terra firme até {RAIO_TERRA} não cobre um andar na borda do platô ({pior:.0})"
        );
        let raio_un = RAIO_BLOCOS as f32 * crate::terreno::BLOCO;
        assert!(raio_un > RAIO_TERRA, "a ilha é menor que a terra dela");
    }

    /// Raio do andar de dungeon (`RAIO_DO_ANDAR` do servidor).
    ///
    /// Repetido aqui, e não importado, porque `shared` não vê o servidor —
    /// e um número repetido num teste é melhor que a regra não ser testada.
    /// Se ele crescer lá, este teste é quem avisa.
    const RAIO_DO_ANDAR: f32 = 55.0;

    /// Refaz a escolha de sítios do servidor (`dg_arena`) sobre a ilhota.
    ///
    /// Os mesmos números e a mesma ordem: do mais longe do "porto" pra
    /// dentro, com 70 u entre eles. Num lugar sem porto o servidor usa o
    /// centro, e é o que se faz aqui.
    fn sitios_do_rodizio() -> Vec<glam::Vec2> {
        use crate::terreno::BLOCO;
        let def = &DEF;
        let ilha = crate::terreno::Ilha::da_ilha(def);
        let raio_sitio = (9.0 / BLOCO) as i32;
        let mut cand: Vec<glam::Vec2> = Vec::new();
        let mut b = -def.raio_blocos;
        while b < def.raio_blocos {
            let mut a = -def.raio_blocos;
            while a < def.raio_blocos {
                if ilha.sitio_plano(a + def.raio_blocos, b + def.raio_blocos, raio_sitio) {
                    let p = glam::Vec2::new(a as f32 * BLOCO, b as f32 * BLOCO);
                    if !ilha.agua(p.x, p.y) && ilha.sem_estorvo(p, 0.6) {
                        cand.push(p);
                    }
                }
                a += 48;
            }
            b += 48;
        }
        // DO MEIO PRA FORA, e sem exigir distância entre eles: é o que o
        // servidor faz na Arena (`dg_arena`). As instâncias não se veem — cada
        // uma vive no seu `Instancia(id)` —, então dividir chão não é
        // problema, e exigir 70 u entre sítios era o que obrigava a ilhota a
        // ser grande.
        cand.sort_by(|x, y| {
            x.length_squared()
                .total_cmp(&y.length_squared())
                .then(x.x.total_cmp(&y.x))
        });
        cand.truncate(4);
        cand
    }

    /// O ANDAR INTEIRO CAI EM TERRA FIRME.
    ///
    /// Este é o teste que decidiu o tamanho da ilhota, e o raciocínio que o
    /// produziu é o que vale registrar: o servidor escolhe os sítios do mais
    /// LONGE do porto pra dentro, e sem porto isso é "da borda pra dentro".
    /// Então o piso do andar (raio 55) sempre transborda o chão PLANO — a
    /// conta dá ~30 u de sobra em qualquer escala, porque crescer a ilhota
    /// empurra os sítios junto.
    ///
    /// Quem conserta é a terra firme continuar além do platô. Se isso quebrar,
    /// o sintoma em jogo é mob nascendo dentro d'água e jogador perseguindo
    /// até cair no mar — e ninguém liga aquilo a esta constante.
    #[test]
    fn o_andar_inteiro_cai_em_terra_firme() {
        let ilha = crate::terreno::Ilha::da_ilha(&DEF);
        let sitios = sitios_do_rodizio();
        assert!(!sitios.is_empty(), "nenhum sítio");
        for s in sitios.iter().take(4) {
            let mut molhado = 0;
            for i in 0..36 {
                let ang = i as f32 * std::f32::consts::TAU / 36.0;
                for k in 1..=5 {
                    let r = RAIO_DO_ANDAR * k as f32 / 5.0;
                    let p = *s + glam::Vec2::new(ang.cos() * r, ang.sin() * r);
                    if ilha.agua(p.x, p.y) {
                        molhado += 1;
                    }
                }
            }
            assert_eq!(
                molhado, 0,
                "o andar em {s:?} tem {molhado} pontos n'água (raio {RAIO_DO_ANDAR})"
            );
        }
    }

    /// O RODÍZIO TEM QUATRO SÍTIOS, e todos em chão plano.
    ///
    /// Eles não precisam mais estar espalhados (as instâncias não se veem),
    /// mas precisam EXISTIR: o servidor pede quatro e completa repetindo o
    /// último. Se a ilhota só oferecesse um, as quatro seriam o mesmo ponto —
    /// o que funciona, mas esconde um relevo que encolheu demais.
    #[test]
    fn o_rodizio_tem_quatro_sitios() {
        let sitios = sitios_do_rodizio();
        assert!(
            sitios.len() >= 4,
            "a ilhota só oferece {} sítio(s): {sitios:?}",
            sitios.len()
        );
    }

    /// A ZONA NÃO COLIDE COM NENHUMA OUTRA.
    ///
    /// `def_da_zona` procura por nome; um nome repetido faria a dungeon
    /// devolver o relevo de outra ilha, e ninguém olharia aqui pra descobrir.
    #[test]
    fn o_nome_da_zona_e_unico() {
        assert!(
            crate::terreno::ARQUIPELAGO.iter().all(|d| d.zona != ZONA),
            "o arquipélago já tem uma zona chamada '{ZONA}'"
        );
        assert!(!crate::magica::e_magica(ZONA));
        assert!(e_arena(ZONA));
        assert!(!e_arena("ilha_inicial"));
    }

    /// E ELA NÃO SE SOBREPÕE A OUTRA ILHA NO MAPA-MUNDI.
    #[test]
    fn a_ilhota_fica_longe_das_outras() {
        let meu = glam::Vec2::new(DEF.centro[0], DEF.centro[1]);
        let raio = |d: &DefIlha| d.raio_blocos as f32 * crate::terreno::BLOCO;
        for d in crate::terreno::ARQUIPELAGO.iter().chain([&crate::magica::DEF]) {
            let outro = glam::Vec2::new(d.centro[0], d.centro[1]);
            let minimo = raio(&DEF) + raio(d);
            assert!(
                meu.distance(outro) > minimo,
                "a Arena encosta em '{}' no mapa-mundi",
                d.zona
            );
        }
    }
    /// A ILHOTA NASCE, TEM CHÃO, TEM FLOR E QUASE NADA DE ENTULHO.
    ///
    /// O dono: "pode ser uma ilhota bem pequena com bem pouco recurso, só pra
    /// ser visualmente agradável, e também com mais flores etc que pedra e
    /// árvore".
    ///
    /// São três medidas, e cada uma pega um erro diferente:
    ///
    /// * **chão seco** — uma ilhota submersa passaria em todo teste de número;
    /// * **quase nenhum estorvo** — não ZERO, que foi a primeira versão e
    ///   estava errada: eu zerava os estorvos do servidor e o cliente
    ///   continuava desenhando as árvores, então a ilhota parecia cheia e se
    ///   atravessava andando. Agora a poda é na fonte, e o pouco que sobra
    ///   barra de verdade;
    /// * **mais forração que árvore** — que é o pedido, em número.
    #[test]
    fn a_ilhota_tem_chao_flor_e_quase_nenhum_entulho() {
        let def = crate::terreno::def_da_zona(ZONA).expect("a zona existe");
        assert_eq!(def.zona, ZONA);
        let ilha = crate::terreno::Ilha::da_ilha(def);
        assert!(ilha.cidade().is_none(), "a arena não devia ter cidade");
        assert!(ilha.porto().is_none(), "a arena não devia ter porto");

        use crate::terreno::BLOCO;
        let ger = crate::terreno::Gerador::da_arena();
        let raio = RAIO_BLOCOS / 2;
        let (mut seco, mut estorvos, mut arvores, mut plantas) = (0, 0, 0, 0);
        let passo = 4;
        let mut bz = -raio;
        while bz < raio {
            let mut bx = -raio;
            while bx < raio {
                let p = glam::Vec2::new(bx as f32 * BLOCO, bz as f32 * BLOCO);
                if !ilha.agua(p.x, p.y) {
                    seco += 1;
                    if ilha.estorvo_em(p, 0.1).is_some() {
                        estorvos += 1;
                    }
                    let topo = ger.bloco_em(bx, bz);
                    if crate::terreno::arvore_da_coluna(
                        def.bioma, bx, bz, topo, 0, &ger, false,
                    )
                    .is_some()
                    {
                        arvores += 1;
                    }
                    if crate::terreno::planta_da_coluna(
                        def.bioma, bx, bz, topo, 0, &ger, false,
                    )
                    .is_some()
                    {
                        plantas += 1;
                    }
                }
                bx += passo;
            }
            bz += passo;
        }
        assert!(seco > 100, "a ilhota quase não tem terra seca ({seco} amostras)");
        // E A CHEGADA É CHÃO. Sem isto o handoff largaria o jogador na água —
        // e o sintoma apareceria como "entrei na dungeon e afundei", que
        // ninguém liga a este arquivo.
        assert!(
            !ilha.agua(CHEGADA.x, CHEGADA.y),
            "a chegada da Arena caiu na água"
        );
        // QUASE nenhum, e medido em FRAÇÃO E EM NÚMERO.
        //
        // Só a fração não bastou: a primeira subida saiu com 1.529 troncos na
        // ilhota — uma árvore a cada seis unidades — e passou, porque 1.529
        // era 0,58% das amostras. Fração pequena numa ilhota inteira ainda é
        // floresta. Quem pega isso é a densidade lá embaixo, em u² por
        // árvore, que é a unidade em que o olho mede.
        let teto = (seco as f32 * 0.02).ceil() as i32;
        assert!(
            estorvos <= teto,
            "a arena tem {estorvos} estorvos em {seco} amostras (teto {teto}): virou mato"
        );
        // E A DENSIDADE, EM u² POR ESTORVO — a unidade em que o olho mede.
        //
        // Medir só a árvore não bastava, e o log provou: depois de raleá-las a
        // um cinquentavo, a ilhota ainda nascia com 1.461 "troncos e
        // matacões". Os estorvos nunca tinham sido árvore — eram MATACÃO, que
        // é forração, e eu tinha acabado de dobrar a forração pra ter flores.
        //
        // Por isso a conta é sobre tudo o que barra, e não sobre uma espécie.
        let area = seco as f32 * (passo as f32 * BLOCO).powi(2);
        let por_estorvo = area / estorvos.max(1) as f32;
        assert!(
            por_estorvo > 250.0,
            "um estorvo a cada {por_estorvo:.0} u²: ainda é floresta ({estorvos} em {area:.0} u²)"
        );
        let por_arvore = area / arvores.max(1) as f32;
        assert!(por_arvore > 250.0, "uma árvore a cada {por_arvore:.0} u²");
        // FLOR MANDA. É o pedido, e é o que faz a ilhota parecer um lugar em
        // vez de um tabuleiro.
        assert!(
            plantas > arvores * 3,
            "a arena tem {arvores} árvore(s) pra {plantas} forração(ões) — devia ser o contrário"
        );
    }
}
