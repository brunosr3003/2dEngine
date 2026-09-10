//! Grau, tier, refino e combinacao. Ver `docs/ITENS.md`.
//!
//! Mesmo padrao do MIR4, nomes nossos: cinco cores, quatro tiers dentro de
//! cada uma, dois iguais viram o proximo, dois tier IV sobem de cor, refino de
//! +0 a +15 que ZERA ao subir de tier.
//!
//! ## A conta que o desenho implica
//!
//! "Dois viram um" aplicado cinco vezes por cor da uma escada exponencial:
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
    pub const TODOS: [Grau; 5] =
        [Grau::Comum, Grau::Fino, Grau::Raro, Grau::Epico, Grau::Lendario];

    pub fn de_u8(v: u8) -> Option<Grau> {
        Self::TODOS.get(v.checked_sub(1)? as usize).copied()
    }

    pub fn acima(self) -> Option<Grau> {
        Grau::de_u8(self as u8 + 1)
    }

    pub fn nome(self) -> &'static str {
        match self {
            Grau::Comum => "Comum",
            Grau::Fino => "Fino",
            Grau::Raro => "Raro",
            Grau::Epico => "Épico",
            Grau::Lendario => "Lendário",
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

/// Onde a peca esta' na escada. Tier vai de 1 a 4 (I..IV).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Degrau {
    pub grau: Grau,
    pub tier: u8,
}

impl Degrau {
    pub fn novo(grau: Grau, tier: u8) -> Self {
        Self { grau, tier: tier.clamp(1, TIER_MAX) }
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

    /// Posicao absoluta na escada, 0 = Comum I.
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
        Some(degrau) => Combinacao::Subiu { degrau, refino_perdido: a.1.max(b.1) },
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
    if p <= 0.0 { f64::INFINITY } else { 1.0 / p }
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

#[cfg(test)]
mod testes {
    use super::*;

    /// A escada inteira, e o numero que ela implica. Se alguem mexer na regra
    /// de combinacao sem perceber, o custo do jogo muda por ordens de
    /// grandeza — e isso tem que quebrar um teste, nao aparecer no forum.
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
            Combinacao::Subiu { degrau: d(Grau::Raro, 3), refino_perdido: 0 }
        );
        assert_eq!(
            combinar((d(Grau::Raro, 4), 5), (d(Grau::Raro, 4), 9)),
            Combinacao::Subiu { degrau: d(Grau::Epico, 1), refino_perdido: 9 }
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
