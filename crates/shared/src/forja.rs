//! Grau, tier, refino e combinacao. Ver `docs/ITENS.md`.
//!
//! Mesmo padrao do MIR4, nomes nossos: cinco cores, quatro tiers dentro de
//! cada uma, dois iguais viram o proximo, dois tier IV sobem de cor, refino de
//! +0 a +15 que ZERA ao subir de tier.
//!
//! ## A conta que o desenho implica
//!
//! "Dois viram um" aplicado cinco vezes por cor da uma ladder exponencial:
//!
//! ```text
//! Comum I → Comum IV          8 pecas
//! uma cor inteira             ×16
//! Comum I → Lendario IV  524.288 pecas
//! ```
//!
//! Meio milhao de pecas comuns por um lendario IV. Nao e' acidente do
//! desenho — e' o desenho, e e' de onde vem o grind do genero.

use serde::{Deserialize, Serialize};

/// A cor que o jogador ve' de longe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Grau {
    Comum = 1,
    Fino = 2,
    Raro = 3,
    Epico = 4,
    Lendario = 5,
}

impl Grau {
    pub const TODOS: [Grau; 5] = [
        Grau::Comum,
        Grau::Fino,
        Grau::Raro,
        Grau::Epico,
        Grau::Lendario,
    ];

    pub fn de_u8(v: u8) -> Option<Grau> {
        Self::TODOS.get(v.checked_sub(1)? as usize).copied()
    }

    pub fn acima(self) -> Option<Grau> {
        Grau::de_u8(self as u8 + 1)
    }

    pub fn nome(self) -> &'static str {
        match self {
            Grau::Comum => "Common",
            Grau::Fino => "Fine",
            Grau::Raro => "Rare",
            Grau::Epico => "Epic",
            Grau::Lendario => "Legendary",
        }
    }

    /// Quantos slots de encanto a peca abre. E' o que da' identidade a uma
    /// peca e a razao de guardar uma em vez de fundir tudo.
    pub fn slots_de_encanto(self) -> u8 {
        match self {
            Grau::Comum => 1,
            Grau::Fino => 1,
            Grau::Raro => 2,
            Grau::Epico => 3,
            Grau::Lendario => 4,
        }
    }
}

pub const TIER_MAX: u8 = 4;

/// Onde a peca esta' na ladder. Tier vai de 1 a 4 (I..IV).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Degrau {
    pub grau: Grau,
    pub tier: u8,
}

impl Degrau {
    pub fn novo(grau: Grau, tier: u8) -> Self {
        Self {
            grau,
            tier: tier.clamp(1, TIER_MAX),
        }
    }

    /// O degrau imediatamente acima. `None` no topo (Lendario IV).
    pub fn acima(self) -> Option<Degrau> {
        if self.tier < TIER_MAX {
            Some(Degrau::novo(self.grau, self.tier + 1))
        } else {
            self.grau.acima().map(|g| Degrau::novo(g, 1))
        }
    }

    /// Quantas pecas de `base` custam UMA deste degrau.
    ///
    /// Serve pra dizer o tamanho do grind antes de assinar embaixo dele: e' um
    /// numero que so' aparece quando alguem faz a conta, e ai' e' tarde.
    pub fn custo_em(self, base: Degrau) -> u64 {
        let passos = self.indice().saturating_sub(base.indice());
        1u64 << passos.min(63)
    }

    /// Posicao absoluta na ladder, 0 = Comum I.
    pub fn indice(self) -> u32 {
        (self.grau as u32 - 1) * TIER_MAX as u32 + (self.tier as u32 - 1)
    }

    pub fn romano(self) -> &'static str {
        match self.tier {
            1 => "I",
            2 => "II",
            3 => "III",
            _ => "IV",
        }
    }
}

impl std::fmt::Display for Degrau {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.grau.nome(), self.romano())
    }
}

/// O que a combinacao devolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinacao {
    /// Duas pecas viraram uma do degrau acima. O refino das duas foi perdido.
    Subiu { degrau: Degrau, refino_perdido: u8 },
    /// Ja' esta' no topo.
    NoTopo,
    /// As duas pecas nao sao do mesmo degrau.
    Diferentes,
}

/// Funde duas pecas. Uma regra so', aplicada cinco vezes por cor.
///
/// **O refino zera.** E' isso que impede refinar cedo: refino e' pra peca em
/// que o jogador vai ficar, nao pra que vai virar material. Devolver o refino
/// junto tornaria a decisao "refina sempre", que nao e' decisao.
pub fn combinar(a: (Degrau, u8), b: (Degrau, u8)) -> Combinacao {
    if a.0 != b.0 {
        return Combinacao::Diferentes;
    }
    match a.0.acima() {
        Some(degrau) => Combinacao::Subiu {
            degrau,
            refino_perdido: a.1.max(b.1),
        },
        None => Combinacao::NoTopo,
    }
}

// ─────────────────────────────── refino ──────────────────────────────
//
// Numeros REAIS do MIR4, da MIR4 Wiki (revisao 4392, 5/fev/2022 — a era do
// jogo base). A wiki esta' fora do ar desde entao; isto veio do arquivo.
// Ver `docs/ITENS.md` pra procedencia e pro que ficou faltando.

/// Ultimo nivel de refino.
pub const REFINO_MAX: u8 = 12;

/// Ate' aqui a falha so' come material. Do +6 em diante, ela DESTROI a peca —
/// e isso independe do grau.
///
/// E' a diferenca entre "tempo" e "aposta", e muda a economia inteira: acima
/// do +5 cada tentativa arrisca o item, entao refinar deixa de ser rotina e
/// vira decisao.
pub const REFINO_SEGURO: u8 = 5;

/// Chance de CHEGAR ao nivel `alvo`, em porcentagem.
///
/// ```text
/// alvo   chance
///  1-3    100%
///    4     80%
///    5     50%
///    6     30%   ← daqui pra cima, falhar destroi
///    7     20%
///    8     15%
///    9     10%
/// 10-12     ?     (a wiki nunca preencheu)
/// ```
pub fn chance_de_refino(alvo: u8) -> u8 {
    match alvo {
        0 => 100,
        1..=3 => 100,
        4 => 80,
        5 => 50,
        6 => 30,
        7 => 20,
        8 => 15,
        9 => 10,
        // EXTRAPOLADO, nao e' dado: a wiki tem "?" de 10 a 12. A curva que
        // vinha (80, 50, 30, 20, 15, 10) achata, entao continuei achatando.
        // Marcado pra ninguem confundir isto com os numeros de cima.
        10 => 8,
        11 => 6,
        12 => 5,
        _ => 0,
    }
}

/// Custo de UMA tentativa, por grau: (darksteel, cobre). Mais uma Pedra de
/// Melhoria do mesmo grau, sempre.
///
/// Lendario a wiki nunca preencheu; extrapolei mantendo o salto de ~10x que
/// vinha de Raro pra Epico.
pub fn custo_de_refino(grau: Grau) -> (u32, u32) {
    match grau {
        Grau::Comum | Grau::Fino => (3_000, 1_000),
        Grau::Raro => (12_000, 8_000),
        Grau::Epico => (120_000, 50_000),
        Grau::Lendario => (1_200_000, 500_000), // EXTRAPOLADO
    }
}

/// Divisor do custo no JOGO. Os numeros de `custo_de_refino` sao os do MIR4
/// (Comum +1 = 3.000 darksteel); com o rendimento da pedra daqui isso seriam
/// centenas de coletas por tentativa. A estrutura fica, o valor cabe numa
/// sessao (docs/ITENS.md). Mudou aqui, muda pra todos.
pub const DIVISOR_DO_CUSTO_EM_JOGO: u32 = 10;

/// (darksteel, cobre) de UMA tentativa no jogo.
pub fn custo_em_jogo(grau: Grau) -> (u32, u32) {
    let (d, c) = custo_de_refino(grau);
    (d / DIVISOR_DO_CUSTO_EM_JOGO, c / DIVISOR_DO_CUSTO_EM_JOGO)
}

/// `ServerMessage::RefinoResultado::resultado`.
pub mod resultado {
    pub const SUBIU: u8 = 0;
    /// Falhou na faixa segura: so' o material foi.
    pub const FALHOU: u8 = 1;
    /// Falhou do +6 em diante: a peca se foi.
    pub const DESTRUIU: u8 = 2;
    pub const NO_TOPO: u8 = 3;
    pub const SEM_MATERIAL: u8 = 4;
    pub const INVALIDO: u8 = 5;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refino {
    Subiu(u8),
    /// Falhou dentro da faixa segura: so' o material foi embora.
    Falhou(u8),
    /// Falhou acima do +5: a peca foi destruida.
    Destruiu,
    NoTopo,
}

/// Tenta refinar. `sorte` e' 0..=99 — quem chama sorteia, pra a funcao ficar
/// pura e testavel.
pub fn refinar(nivel: u8, sorte: u8) -> Refino {
    if nivel >= REFINO_MAX {
        return Refino::NoTopo;
    }
    let alvo = nivel + 1;
    if sorte < chance_de_refino(alvo) {
        return Refino::Subiu(alvo);
    }
    if alvo > REFINO_SEGURO {
        Refino::Destruiu
    } else {
        Refino::Falhou(nivel)
    }
}

/// Quantas PECAS se queimam, em media, pra levar UMA ate' `alvo`.
///
/// Com destruicao a pergunta deixa de ser "quantas tentativas" e vira
/// "quantas pecas" — o refino passa a consumir a economia de DROP, nao so' a
/// de moeda. E' o numero que decide o jogo.
///
/// Conta fechada e nao simulacao: ate' o +5 a falha e' segura, entao a peca
/// sempre chega la' (so' custa material); do +6 pra cima cada nivel e' uma
/// moeda unica, e a peca so' sobrevive se todas derem cara. Logo
/// `pecas = 1 / ∏ chance`. Simular seria pior que inutil — o +12 sai a
/// milhoes de pecas e o laco nao termina em tempo de teste.
pub fn pecas_por(alvo: u8) -> f64 {
    let mut p = 1.0f64;
    for k in (REFINO_SEGURO + 1)..=alvo.min(REFINO_MAX) {
        p *= chance_de_refino(k) as f64 / 100.0;
    }
    if p <= 0.0 {
        f64::INFINITY
    } else {
        1.0 / p
    }
}

/// Tentativas gastas em media por peca ate' ela chegar ao alvo ou morrer
/// tentando. Serve pro custo em moeda, que e' por TENTATIVA.
pub fn tentativas_por_peca(alvo: u8, amostras: u32) -> f32 {
    let mut total: u64 = 0;
    let mut est: u32 = 0x1234_5678;
    for _ in 0..amostras {
        let mut n = 0u8;
        while n < alvo {
            est = est.wrapping_mul(1664525).wrapping_add(1013904223);
            let sorte = ((est >> 16) % 100) as u8;
            total += 1;
            match refinar(n, sorte) {
                Refino::Subiu(v) => n = v,
                Refino::Falhou(_) => {}
                Refino::Destruiu | Refino::NoTopo => break,
            }
        }
    }
    total as f32 / amostras as f32
}

// ────────────────────────────── aprimorar ─────────────────────────────
//
// A aba "Upgrade" do Craft, a ladder de `combinar` aplicada ao item de
// verdade. A instancia guarda a COR em `rarity` e o TIER em `tier`:
//
//   2 x (cor G, Tier I..III)        ->  1 x (cor G, tier seguinte)
//   2 x (cor G, Tier IV, ambas +8)  ->  1 x (cor G+1, Tier I)
//
// "Iguais" = mesmo `item_id`, mesma cor, mesmo tier. A peça nova recebe os
// valores fixos do degrau novo; o refino das duas se perde — é por isso
// que a subida de cor pede +8: o refino deixa de ser so' poder e vira
// materia-prima da progressao (docs/ITENS.md).

/// Refino que as duas Tier IV precisam pra subir de cor.
pub const REFINO_PARA_COR: u8 = 8;

/// Nivel do personagem pra ter uma peca desta cor pelo Aprimorar: o mesmo
/// minimo do craft.
///
/// LIDO de `receitas`, nao copiado. Era copiado, e dizia 40 no azul, 60 no
/// roxo e 80 no lendario — os valores de antes de 28/09/2026. Quem criava a
/// peca Rara no 30 nao conseguia aprimorar pra Rara antes do 40: a mesma cor
/// com duas portas de altura diferente.
pub fn nivel_da_cor(grau: u8) -> u32 {
    crate::receitas::nivel_da_cor(grau.max(1)) as u32
}

/// Nivel de item da peca que sobe pra esta cor: o MESMO do craft daquela cor,
/// pra aprimorar e criar entregarem a mesma peca. Nunca desce: fica o maior
/// entre este e o das duas.
pub fn nivel_de_item_da_cor(grau: u8) -> u16 {
    let cor = grau.clamp(1, 5);
    match crate::receitas::FAIXAS.iter().find(|f| f.cor == cor) {
        Some(f) => f.item_level,
        None => 5,
    }
}

/// Cobre de uma fusao, pela cor e tier de ORIGEM. Tier sobe 4x por degrau e
/// cada cor multiplica por 4; subir de cor custa o dobro do ultimo tier.
pub fn custo_de_aprimorar(grau: u8, tier: u8) -> u32 {
    let por_cor = 4u32.pow(grau.clamp(1, 5) as u32 - 1);
    let passo = match tier {
        1 => 500,
        2 => 2_000,
        3 => 8_000,
        _ => 16_000,
    };
    passo * por_cor
}

/// Uma peca como o Aprimorar ve': (item_id, cor, tier, refino, tem gema).
pub type PecaDoAprimorar = (u16, u8, u8, u8, bool);

/// Por que duas pecas nao se fundem (a frase que o jogador le), ou o
/// (cor, tier) que sai.
pub fn conferir_aprimorar(a: PecaDoAprimorar, b: PecaDoAprimorar) -> Result<(u8, u8), String> {
    if a.0 != b.0 {
        return Err("as duas peças precisam ser o mesmo item".into());
    }
    if a.1 != b.1 {
        return Err("as duas peças precisam ser da mesma cor".into());
    }
    if a.2 != b.2 {
        return Err("as duas peças precisam ter o mesmo tier".into());
    }
    if a.4 || b.4 {
        return Err("take the gems out of the pieces first".into());
    }
    let (grau, tier) = (a.1.clamp(1, 5), a.2.clamp(1, TIER_MAX));
    if tier < TIER_MAX {
        return Ok((grau, tier + 1));
    }
    if grau >= Grau::Lendario as u8 {
        return Err("Legendary IV is the top".into());
    }
    if a.3 < REFINO_PARA_COR || b.3 < REFINO_PARA_COR {
        return Err(format!(
            "para subir de cor, as duas Tier IV precisam estar +{REFINO_PARA_COR}"
        ));
    }
    Ok((grau + 1, 1))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A ladder inteira, e o numero que ela implica. Se alguem mexer na regra
    /// de combinacao sem perceber, o custo do jogo muda por ordens de
    /// grandeza — e isso tem que quebrar um teste, nao aparecer no forum.
    #[test]
    fn aprimorar_sobe_o_tier_e_duas_iv_mais_8_sobem_de_cor() {
        let p = |cor, tier, refino| (10u16, cor, tier, refino, false);
        assert_eq!(conferir_aprimorar(p(1, 1, 0), p(1, 1, 3)), Ok((1, 2)));
        assert_eq!(conferir_aprimorar(p(2, 3, 0), p(2, 3, 0)), Ok((2, 4)));
        // Tier IV: so' com as duas +8, e ai' vira a cor de cima no Tier I.
        assert!(conferir_aprimorar(p(1, 4, 8), p(1, 4, 7)).is_err());
        assert_eq!(conferir_aprimorar(p(1, 4, 8), p(1, 4, 12)), Ok((2, 1)));
        assert!(conferir_aprimorar(p(5, 4, 12), p(5, 4, 12)).is_err(), "topo");
        // Diferentes nao fundem.
        assert!(conferir_aprimorar(p(1, 1, 0), p(2, 1, 0)).is_err());
        assert!(conferir_aprimorar(p(1, 1, 0), p(1, 2, 0)).is_err());
        assert!(conferir_aprimorar((10, 1, 1, 0, false), (11, 1, 1, 0, false)).is_err());
        assert!(conferir_aprimorar((10, 1, 1, 0, true), (10, 1, 1, 0, false)).is_err());
        // Custo cresce no tier e na cor.
        assert!(custo_de_aprimorar(1, 1) < custo_de_aprimorar(1, 2));
        assert!(custo_de_aprimorar(1, 4) < custo_de_aprimorar(2, 4));
        assert_eq!(custo_de_aprimorar(1, 1), 500);
    }

    #[test]
    fn a_escada_custa_meio_milhao() {
        let base = Degrau::novo(Grau::Comum, 1);
        assert_eq!(Degrau::novo(Grau::Comum, 4).custo_em(base), 8);
        assert_eq!(Degrau::novo(Grau::Fino, 1).custo_em(base), 16);
        assert_eq!(Degrau::novo(Grau::Lendario, 1).custo_em(base), 65_536);
        assert_eq!(Degrau::novo(Grau::Lendario, 4).custo_em(base), 524_288);
    }

    #[test]
    fn combina_dois_iguais_e_sobe_de_cor_no_iv() {
        let d = |g, t| Degrau::novo(g, t);
        assert_eq!(
            combinar((d(Grau::Raro, 2), 0), (d(Grau::Raro, 2), 0)),
            Combinacao::Subiu {
                degrau: d(Grau::Raro, 3),
                refino_perdido: 0
            }
        );
        assert_eq!(
            combinar((d(Grau::Raro, 4), 5), (d(Grau::Raro, 4), 9)),
            Combinacao::Subiu {
                degrau: d(Grau::Epico, 1),
                refino_perdido: 9
            }
        );
        assert_eq!(
            combinar((d(Grau::Lendario, 4), 0), (d(Grau::Lendario, 4), 0)),
            Combinacao::NoTopo
        );
        assert_eq!(
            combinar((d(Grau::Raro, 1), 0), (d(Grau::Raro, 2), 0)),
            Combinacao::Diferentes
        );
    }

    /// A faixa segura vai ate' o +5; do +6 em diante falhar DESTROI.
    #[test]
    fn destroi_do_seis_em_diante() {
        assert_eq!(refinar(0, 99), Refino::Subiu(1)); // 100% nao falha
        assert_eq!(refinar(3, 99), Refino::Falhou(3)); // alvo 4, 80%, seguro
        assert_eq!(refinar(4, 99), Refino::Falhou(4)); // alvo 5, 50%, seguro
        assert_eq!(refinar(5, 99), Refino::Destruiu); // alvo 6 — perigo
        assert_eq!(refinar(REFINO_MAX, 0), Refino::NoTopo);
    }

    /// O custo em PECAS, medido. Com destruicao o refino consome a economia
    /// de drop, nao so' a de moeda — e' este numero que decide o jogo, e ele
    /// nao aparece em lugar nenhum ate' alguem simular.
    #[test]
    fn o_custo_em_pecas() {
        for alvo in [5u8, 6, 7, 8, 9, 10, 11, 12] {
            println!(
                "  +{alvo:<2} {:>12.0} pecas   {:>4.1} tentativas por peca",
                pecas_por(alvo),
                tentativas_por_peca(alvo, 50_000)
            );
        }
        assert_eq!(pecas_por(5), 1.0, "ate' o +5 nao se perde peca");
        assert!(pecas_por(9) > 1000.0, "o +9 tem que ser caro em pecas");
        assert!(pecas_por(12) > 1e6, "o +12 tem que ser absurdo");
    }
}

// ─────────────────────── o custo da ladder, fechado ───────────────────────

/// Darksteel por hora de mineracao ATIVA.
///
/// Sai da coleta offline, que e' 25% disto e rende 85.200 por dia (ver
/// `docs/ECONOMIA.md`): 85.200 / 0,25 / 24 = 14.200. Esta' aqui, e nao so' no
/// documento, porque o painel de economia calcula TEMPO a partir dele — e
/// numero de desenho que mora em dois lugares vira dois desenhos diferentes.
pub const DARKSTEEL_POR_HORA: f64 = 14_200.0;

/// O que custa levar UMA peca de zero ate' `alvo`.
///
/// Fecha as tres contas que a economia faz o tempo todo, e fecha juntas
/// porque separadas elas mentem: contar so' tentativas ignora que do +6 pra
/// cima a peca morre, e contar so' pecas ignora que cada tentativa cobra
/// moeda de novo.
#[derive(Debug, Clone, Copy)]
pub struct Escada {
    pub alvo: u8,
    /// Quantas pecas se gastam, em media, ate' uma chegar.
    pub pecas: f64,
    /// Tentativas somadas, contando as das pecas que morreram no caminho.
    pub tentativas: f64,
    pub darksteel: f64,
    pub cobre: f64,
    /// Horas de mineracao ativa pra pagar o darksteel.
    pub horas: f64,
}

/// A ladder inteira de um grau, do +1 ao `REFINO_MAX`.
///
/// A conta separa os dois regimes, e tem que separar: eles cobram diferente.
///
///   * **ate' `REFINO_SEGURO`** a falha so' come material, entao a peca
///     insiste ate' passar — `1 / chance` tentativas por nivel, sempre a
///     mesma peca;
///   * **do +6 em diante** cada tentativa e' moeda unica: ou sobe, ou destroi.
///     Uma tentativa por peca viva, e as que morrem param de gastar ali.
///
/// A primeira versao multiplicava a ladder inteira pelo numero de pecas, como
/// se toda peca perdida tivesse pago ate' o topo. Dava o DOBRO: 205 horas
/// pra um Raro +7 que o desenho da economia fixou em 106. Peca que morre no
/// +6 nunca pagou a tentativa do +7.
pub fn ladder(grau: Grau) -> Vec<Escada> {
    let (ds, cu) = custo_de_refino(grau);
    let chance = |k: u8| chance_de_refino(k) as f64 / 100.0;
    let mut fora = Vec::new();
    for alvo in 1..=REFINO_MAX {
        let pecas = pecas_por(alvo);
        // Faixa segura: cada peca sobe sozinha, custando 1/chance por nivel.
        let por_peca: f64 = (1..=alvo.min(REFINO_SEGURO)).map(|k| 1.0 / chance(k)).sum();
        let mut tentativas = pecas * por_peca;
        // Faixa de aposta: uma tentativa por peca VIVA, e a cada nivel sobram
        // menos.
        let mut vivas = pecas;
        for k in (REFINO_SEGURO + 1)..=alvo {
            tentativas += vivas;
            vivas *= chance(k);
        }
        let darksteel = tentativas * ds as f64;
        fora.push(Escada {
            alvo,
            pecas,
            tentativas,
            darksteel,
            cobre: tentativas * cu as f64,
            horas: darksteel / DARKSTEEL_POR_HORA,
        });
    }
    fora
}

#[cfg(test)]
mod testes_escada {
    use super::*;

    /// A ladder tem que bater com o numero que decidiu o desenho da economia:
    /// **Raro +7 sai por ~106 horas de mineracao ativa**. E' dele que veio a
    /// colonia offline, e se ele mudar sem ninguem ver, a colonia passa a
    /// resolver um problema que nao existe mais.
    #[test]
    fn raro_mais_sete_custa_cem_e_poucas_horas() {
        let e = ladder(Grau::Raro);
        let sete = e.iter().find(|x| x.alvo == 7).unwrap();
        assert!(
            (sete.pecas - 17.0).abs() < 1.0,
            "Raro +7 pede {:.1} pecas, esperado ~17",
            sete.pecas
        );
        assert!(
            (90.0..130.0).contains(&sete.horas),
            "Raro +7 sai por {:.0} h de mineracao, esperado ~106",
            sete.horas
        );
    }

    /// Ate' o nivel seguro nao se perde peca: e' o que separa "tempo" de
    /// "aposta", e a diferenca e' o jogo inteiro.
    #[test]
    fn ate_o_seguro_a_peca_sempre_chega() {
        for grau in Grau::TODOS {
            for e in ladder(grau).iter().filter(|e| e.alvo <= REFINO_SEGURO) {
                assert!(
                    (e.pecas - 1.0).abs() < 1e-9,
                    "{grau:?} +{}: {:.2} pecas dentro da faixa segura",
                    e.alvo,
                    e.pecas
                );
            }
        }
    }
}

/// O custo FECHADO de uma peca: subir os degraus e refinar em cima do topo.
///
/// As duas escadas se multiplicam, e e' isso que o painel precisava dizer.
/// `pecas_por` conta pecas DAQUELE degrau — e cada uma delas ja' custou
/// `custo_em(base)` pecas de base pra existir. Uma Rara IV destruida no +6
/// nao foi "uma peca": foram 2.048 Comuns I.
///
/// Mostrar as duas separadas, como o painel fazia, esconde justamente a conta
/// que decide o jogo. Raro IV +7 nao custa 17 pecas: custa 34.816 de base.
#[derive(Debug, Clone, Copy)]
pub struct CustoTotal {
    pub degrau: Degrau,
    pub alvo: u8,
    /// Pecas do degrau `base` que a coisa inteira consome.
    pub pecas_base: f64,
    /// Pecas DO PROPRIO degrau gastas no refino (as destruidas incluidas).
    pub pecas_do_degrau: f64,
    pub darksteel: f64,
    pub cobre: f64,
    pub horas: f64,
}

pub fn custo_total(degrau: Degrau, alvo: u8, base: Degrau) -> CustoTotal {
    let e = ladder(degrau.grau)
        .into_iter()
        .find(|e| e.alvo == alvo)
        .unwrap_or(Escada {
            alvo: 0,
            pecas: 1.0,
            tentativas: 0.0,
            darksteel: 0.0,
            cobre: 0.0,
            horas: 0.0,
        });
    let por_peca = degrau.custo_em(base) as f64;
    CustoTotal {
        degrau,
        alvo,
        pecas_base: e.pecas * por_peca,
        pecas_do_degrau: e.pecas,
        darksteel: e.darksteel,
        cobre: e.cobre,
        horas: e.horas,
    }
}

#[cfg(test)]
mod testes_custo_total {
    use super::*;

    /// As duas escadas se MULTIPLICAM.
    ///
    /// Raro IV custa 2.048 Comuns I (onze dobras), e o +7 come 17 pecas do
    /// proprio degrau. Sao 34 mil pecas de base, e nao 17 — a diferenca entre
    /// as duas leituras e' de tres ordens de grandeza, e e' a leitura errada
    /// que faz um numero parecer aceitavel.
    #[test]
    fn o_tier_multiplica_o_refino() {
        let base = Degrau::novo(Grau::Comum, 1);
        let raro4 = Degrau::novo(Grau::Raro, TIER_MAX);
        assert_eq!(raro4.custo_em(base), 2048);

        let c = custo_total(raro4, 7, base);
        assert!(
            (c.pecas_do_degrau - 16.67).abs() < 0.5,
            "{}",
            c.pecas_do_degrau
        );
        assert!(
            (c.pecas_base - 34_133.0).abs() < 200.0,
            "Raro IV +7 deu {:.0} pecas de base, esperado ~34.100",
            c.pecas_base
        );
        // O darksteel NAO muda com o tier: ele e' por tentativa, e a tentativa
        // cobra pelo grau. Quem paga o tier sao as pecas.
        let raro1 = custo_total(Degrau::novo(Grau::Raro, 1), 7, base);
        assert_eq!(c.darksteel.round(), raro1.darksteel.round());
        assert!(c.pecas_base > raro1.pecas_base * 7.0);
    }

    /// Sem refino, o custo e' so' a ladder de degraus: dobra a cada tier.
    #[test]
    fn cada_degrau_dobra() {
        let base = Degrau::novo(Grau::Comum, 1);
        let mut anterior = 0.0;
        for grau in Grau::TODOS {
            for tier in 1..=TIER_MAX {
                let d = Degrau::novo(grau, tier);
                let c = custo_total(d, 0, base);
                if anterior > 0.0 {
                    assert!(
                        (c.pecas_base / anterior - 2.0).abs() < 1e-9,
                        "{d}: {} nao e' o dobro de {anterior}",
                        c.pecas_base
                    );
                }
                anterior = c.pecas_base;
            }
        }
        // Lendario IV: dezenove dobras a partir do Comum I.
        let topo = custo_total(Degrau::novo(Grau::Lendario, TIER_MAX), 0, base);
        assert_eq!(topo.pecas_base as u64, 1 << 19);
    }
}
