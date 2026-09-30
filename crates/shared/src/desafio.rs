//! DESAFIOS DO PORÃO: metas opcionais que aumentam o baú.
//!
//! O dono, em 29/09/2026: "maybe some random challengs that increas the final
//! bonus like 'complete the dungeun in lass than: x minuts' or dont let you hp
//! drop more than x %, dont drink any potions, etc". E, escolhido depois:
//! sorteados, mostrados na porta antes de entrar, e pagos em baú maior.
//!
//! ## Por que rodam por HORA e não por corrida
//!
//! "Mostrado na porta" exige que o cliente saiba o sorteio antes de entrar, e
//! que o servidor chegue ao MESMO sorteio na hora da entrada. Sortear por
//! corrida pediria uma mensagem nova de rede só pra isso. Por hora, os dois
//! lados calculam sozinhos a partir do relógio: mesmo Porão, mesma hora,
//! mesmos desafios. Quem entra na virada da hora pega os da hora da ENTRADA —
//! é o servidor que decide, e a tarja mostra a hora corrente.
//!
//! Falhar um desafio NÃO encerra a corrida: só perde o bônus daquele.

use crate::dungeon::{Conteudo, Tipo};

/// Uma meta opcional de corrida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Desafio {
    /// Vencer o chefe em menos da metade do limite.
    Rapido,
    /// A vida nunca cai abaixo de `VIDA_MINIMA_PCT`.
    VidaAlta,
    /// Nenhuma poção bebida dentro da instância.
    SemPocao,
    /// Ninguém cai.
    SemMorte,
    /// Nenhum dash.
    SemDash,
}

pub const TODOS: [Desafio; 5] = [
    Desafio::Rapido,
    Desafio::VidaAlta,
    Desafio::SemPocao,
    Desafio::SemMorte,
    Desafio::SemDash,
];

/// Quantos desafios cada corrida sorteia.
pub const POR_CORRIDA: usize = 2;
/// A vida mínima do `VidaAlta`, em %.
pub const VIDA_MINIMA_PCT: u8 = 50;
/// O `Rapido` pede vencer em até esta fração do limite.
pub const FRACAO_DO_RAPIDO: f32 = 0.5;
/// Quanto CADA desafio cumprido soma ao baú (ouro, cobre, marcas e material).
///
/// Dois cumpridos = +40%. O guarda da torneira
/// (`porao::a_torneira_do_porao_fica_dentro_do_combinado`) mede o pior caso
/// com os dois.
pub const BONUS_POR_DESAFIO: f32 = 0.20;

/// O que se mediu na corrida, pra conferir os desafios.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Placar {
    pub segundos: f32,
    /// A menor fração de vida (0..1) que alguém do grupo chegou a ter.
    pub vida_min: f32,
    pub pocoes: u32,
    pub mortes: u32,
    pub dashes: u32,
}

impl Desafio {
    /// O texto da tarja e do resultado, na língua de origem.
    pub fn texto(self, c: &Conteudo) -> String {
        match self {
            Desafio::Rapido => {
                let s = (c.limite_s as f32 * FRACAO_DO_RAPIDO) as u32;
                format!("Finish in under {}:{:02}", s / 60, s % 60)
            }
            Desafio::VidaAlta => format!("Never drop below {VIDA_MINIMA_PCT}% HP"),
            Desafio::SemPocao => "Drink no potions".into(),
            Desafio::SemMorte => "Nobody falls".into(),
            Desafio::SemDash => "No dashing".into(),
        }
    }

    /// Cumpriu? Só vale pra quem VENCEU — o chamador confere a vitória.
    pub fn cumpriu(self, c: &Conteudo, p: &Placar) -> bool {
        match self {
            Desafio::Rapido => p.segundos <= c.limite_s as f32 * FRACAO_DO_RAPIDO,
            Desafio::VidaAlta => p.vida_min * 100.0 >= VIDA_MINIMA_PCT as f32,
            Desafio::SemPocao => p.pocoes == 0,
            Desafio::SemMorte => p.mortes == 0,
            Desafio::SemDash => p.dashes == 0,
        }
    }
}

/// A hora (unix / 3600) em que um instante cai. É a chave do sorteio.
pub fn hora(unix: i64) -> i64 {
    unix.div_euclid(3600)
}

/// Os desafios deste Porão nesta hora. Determinístico: cliente e servidor
/// chegam ao mesmo par. Nunca repete o mesmo desafio.
pub fn da_hora(c: &Conteudo, unix: i64) -> [Desafio; POR_CORRIDA] {
    // Mistura simples e estável (splitmix64) de (conteúdo, hora).
    let mut x = (c.id as u64) << 32 ^ hora(unix) as u64 ^ 0x9E37_79B9_7F4A_7C15;
    let mut prox = || {
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let a = (prox() % TODOS.len() as u64) as usize;
    let mut b = (prox() % (TODOS.len() as u64 - 1)) as usize;
    if b >= a {
        b += 1;
    }
    [TODOS[a], TODOS[b]]
}

/// Só o Porão tem desafio.
pub fn tem_desafio(c: &Conteudo) -> bool {
    c.tipo == Tipo::Porao
}

/// O multiplicador do baú por `n` desafios cumpridos.
pub fn multiplicador(n: usize) -> f32 {
    1.0 + BONUS_POR_DESAFIO * n.min(POR_CORRIDA) as f32
}

#[cfg(test)]
mod testes {
    use super::*;

    fn porao() -> &'static Conteudo {
        crate::dungeon::CONTEUDOS
            .iter()
            .find(|c| c.tipo == Tipo::Porao)
            .unwrap()
    }

    /// O MESMO PAR PRA MESMA HORA, E NUNCA DOIS IGUAIS.
    ///
    /// É o que deixa a tarja da porta e o servidor concordarem sem conversar.
    #[test]
    fn o_sorteio_e_estavel_na_hora_e_nunca_repete() {
        let c = porao();
        let t = 1_789_000_000;
        assert_eq!(da_hora(c, t), da_hora(c, t + 1200), "mudou dentro da hora");
        for h in 0..500 {
            let [a, b] = da_hora(c, t + h * 3600);
            assert_ne!(a, b, "hora {h} sorteou o mesmo desafio duas vezes");
        }
    }

    /// TODO DESAFIO APARECE — o sorteio não esquece nenhum.
    #[test]
    fn em_um_dia_de_horas_todo_desafio_sai() {
        let c = porao();
        let mut vistos = std::collections::HashSet::new();
        for h in 0..48 {
            for d in da_hora(c, 1_789_000_000 + h * 3600) {
                vistos.insert(d);
            }
        }
        assert_eq!(vistos.len(), TODOS.len(), "{vistos:?}");
    }

    /// CADA DESAFIO PASSA E FALHA ONDE DEVE.
    #[test]
    fn cada_desafio_confere_a_propria_medida() {
        let c = porao();
        let limpo = Placar {
            segundos: c.limite_s as f32 * 0.4,
            vida_min: 0.9,
            ..Default::default()
        };
        for d in TODOS {
            assert!(d.cumpriu(c, &limpo), "{d:?} reprovou uma corrida limpa");
        }
        let lento = Placar { segundos: c.limite_s as f32 * 0.7, ..limpo };
        assert!(!Desafio::Rapido.cumpriu(c, &lento));
        let ferido = Placar { vida_min: 0.3, ..limpo };
        assert!(!Desafio::VidaAlta.cumpriu(c, &ferido));
        assert!(!Desafio::SemPocao.cumpriu(c, &Placar { pocoes: 1, ..limpo }));
        assert!(!Desafio::SemMorte.cumpriu(c, &Placar { mortes: 1, ..limpo }));
        assert!(!Desafio::SemDash.cumpriu(c, &Placar { dashes: 1, ..limpo }));
    }

    #[test]
    fn o_bonus_soma_por_desafio_e_para_no_teto() {
        assert_eq!(multiplicador(0), 1.0);
        assert!((multiplicador(2) - 1.4).abs() < 1e-6);
        assert_eq!(multiplicador(5), multiplicador(POR_CORRIDA));
    }
}
