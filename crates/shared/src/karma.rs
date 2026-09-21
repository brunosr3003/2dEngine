//! A MARCA e o KARMA (docs/MAR_ABERTO.md).
//!
//! O mar **nao** e' open PvP. Decisao do dono, e ela e' o que separa este
//! desenho de um free-for-all:
//!
//! > *"Nao sera open pvp no mar aberto. Terao zonas de open pvp, mas no geral
//! > so' vai ficar taggeado como PK aberto quem tiver com esses baus — em
//! > safe zones ai' todo mundo pode atacar sem penalidade. Se nao, tera
//! > penalidade de PK se atacar."*
//!
//! Disso saem duas regras e uma simetria.
//!
//! # A marca e' DERIVADA
//!
//! Voce esta' marcado enquanto houver um Bau do Colosso no conves do seu
//! barco. Nao ha' campo "esta_marcado" pra dessincronizar, esquecer de
//! limpar, ou deixar alguem atacavel sem baú nenhum. O unico pedaco guardado
//! e' o RASTRO de um minuto depois que o bau sai, e ele existe por um motivo
//! especifico: sem ele, entregar um segundo antes do golpe e' um drible.
//!
//! # O karma e' o preco de atacar quem NAO esta' marcado
//!
//! So' a MORTE cobra, nao o toque. Area necessariamente respinga em quem
//! passa, e punir um corte perdido seria injusto e ilegivel.
//!
//! # A simetria
//!
//! No topo da escala, o assassino **vira exatamente o que o carregador e'**:
//! PK aberto. O sistema tem um estado so', com duas portas de entrada —
//! carregar tesouro, ou matar inocente. Um ruleset, uma placa vermelha, uma
//! replicacao.

/// Karma cobrado por matar quem nao estava marcado.
pub const POR_MORTE: i32 = 100;
/// De novo na MESMA vitima, dentro da janela: cobra o triplo.
pub const REPETIDA: i32 = 300;
/// Janela da reincidencia, em segundos.
pub const JANELA_REPETIDA_S: i64 = 600;
/// Karma perdoado a cada `DECAIMENTO_S` de jogo.
///
/// Escolhidos pra a frase fechar redondo: **uma morte custa uma hora de
/// jogo**. Um numero que se enuncia numa linha e' um numero que o jogador
/// consegue guardar — e karma so' funciona se ele souber o preco antes.
pub const DECAIMENTO: i32 = 10;
pub const DECAIMENTO_S: f32 = 360.0;

/// Em que degrau da escala este karma cai.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grau {
    Limpo,
    Suspeito,
    Criminoso,
    Foragido,
}

pub fn grau(karma: i32) -> Grau {
    match karma {
        0 => Grau::Limpo,
        1..=199 => Grau::Suspeito,
        200..=499 => Grau::Criminoso,
        _ => Grau::Foragido,
    }
}

impl Grau {
    pub fn nome(self) -> &'static str {
        match self {
            Grau::Limpo => "Limpo",
            Grau::Suspeito => "Suspeito",
            Grau::Criminoso => "Criminoso",
            Grau::Foragido => "Foragido",
        }
    }

    /// A partir de Criminoso o assassino vira o que a vitima dele era: PK
    /// aberto. E' a simetria que faz o sistema ter UM estado so'.
    pub fn marcado(self) -> bool {
        matches!(self, Grau::Criminoso | Grau::Foragido)
    }

    /// NPC atende? Criminoso perde loja, reparo, banco e craft.
    ///
    /// E' a punicao com mais dentes e a mais barata de escrever: nao ha'
    /// guarda com IA, nao ha' perseguicao. O Foragido simplesmente nao
    /// consegue CONSERTAR O CASCO, e um barco que nao repara nao navega —
    /// o mar cospe ele pra fora sozinho.
    pub fn npc_atende(self) -> bool {
        !matches!(self, Grau::Criminoso | Grau::Foragido)
    }
}

/// O karma depois de uma morte de inocente.
pub fn apos_matar(karma: i32, repetida: bool) -> i32 {
    (karma + if repetida { REPETIDA } else { POR_MORTE }).min(9_999)
}

/// O karma depois de `segundos` jogando.
pub fn apos_jogar(karma: i32, segundos: f32) -> i32 {
    let perdoa = (segundos / DECAIMENTO_S * DECAIMENTO as f32) as i32;
    (karma - perdoa).max(0)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Uma morte custa cerca de uma hora de jogo. Reincidir custa tres.
    #[test]
    fn matar_custa_tempo_e_reincidir_custa_mais() {
        let k = apos_matar(0, false);
        assert_eq!(k, POR_MORTE);
        assert_eq!(grau(k), Grau::Suspeito);
        // Uma hora online limpa uma morte.
        assert_eq!(apos_jogar(k, 3_600.0), 0);
        // Reincidir na mesma vitima pesa o triplo e ja' marca o assassino.
        let r = apos_matar(k, true);
        assert!(grau(r).marcado(), "reincidente tinha que virar alvo");
        assert!(!grau(r).npc_atende(), "reincidente perde o porto");
    }

    /// A escala e' monotona: mais karma nunca melhora a situacao.
    #[test]
    fn a_escala_nunca_melhora_com_mais_karma() {
        let mut pior = false;
        for k in 0..1_200 {
            let g = grau(k);
            if g.marcado() {
                pior = true;
            }
            assert!(!(pior && !g.marcado()), "karma {k} desmarcou");
        }
        assert_eq!(grau(0), Grau::Limpo);
        assert!(grau(9_999).marcado());
    }
}
