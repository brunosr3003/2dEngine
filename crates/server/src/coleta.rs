//! Gathering by NODE: the pure sums. The player picks a stone (or a log),
//! gets in range and gathers THAT one, cycle by cycle; the per-session state
//! and the messages live in `world`. See docs/COLETA.md.
use glam::Vec2;

/// How far the body may slip (a shove, rounding) without it counting as
/// "moved" and stopping the gathering.
pub const TOLERANCIA_MOVER: f32 = 0.35;

/// Slack on top of the range: the client stops NEAR the point, not on it.
const FOLGA_ALCANCE: f32 = 0.25;

/// Da borda do corpo do jogador a' borda do no' cabe no alcance?
pub fn ao_alcance(pos: Vec2, centro: Vec2, raio_no: f32) -> bool {
    pos.distance(centro) - raio_no - shared::ENTITY_RADIUS
        <= shared::COLETA_ALCANCE_UN + FOLGA_ALCANCE
}

/// The client may stop short of the destination. Even the furthest point of
/// that tolerance has to stay within gathering range.
pub fn aproximacao_ao_alcance(pos: Vec2, centro: Vec2, raio_no: f32) -> bool {
    pos.distance(centro) + shared::COLETA_TOLERANCIA_CHEGADA
        - raio_no - shared::ENTITY_RADIUS <= shared::COLETA_ALCANCE_UN + FOLGA_ALCANCE
}

/// Where to stand to gather: on the side of whoever arrives, in the middle of the range.
pub fn ponto_de_coleta(centro: Vec2, raio_no: f32, eu: Vec2) -> Vec2 {
    let dir = (eu - centro).try_normalize().unwrap_or(Vec2::X);
    centro + dir * (raio_no + shared::ENTITY_RADIUS + shared::COLETA_ALCANCE_UN * 0.5)
}

/// Pontos ao redor do nó, começando pelo lado de quem chega. O A* pode
/// alcançar o outro lado mesmo quando um corpo ou degrau fecha a frente.
pub fn pontos_de_coleta(centro: Vec2, raio_no: f32, eu: Vec2) -> [Vec2; 8] {
    let frente = ponto_de_coleta(centro, raio_no, eu) - centro;
    [0.0_f32, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0].map(|passo| {
        let (s, c) = (passo * std::f32::consts::FRAC_PI_4).sin_cos();
        centro + Vec2::new(frente.x * c - frente.y * s, frente.x * s + frente.y * c)
    })
}

/// O tipo (0 madeira, 1..4 pedra) esta' marcado?
pub fn aceita(tipos: &[bool; 5], tier: u8) -> bool {
    tipos.get(tier as usize).copied().unwrap_or(false)
}

/// Cabe TUDO o que o ciclo sorteou? Simula em copia, na mesma regra da bolsa
/// (empilha no que ja' tem, depois ocupa espaco vazio). Se nao couber um
/// item sequer, o ciclo nao conclui: coleta nunca joga material fora.
pub fn cabe_tudo(
    inv: &[shared::InventorySlot],
    drops: &[(u16, u32)],
    pilha: &dyn Fn(u16) -> u32,
) -> bool {
    let mut sim: Vec<(u16, u32, bool)> = inv
        .iter()
        .map(|s| (s.item_id, s.qty, s.instance.is_some()))
        .collect();
    for &(item, mut qtd) in drops {
        if qtd == 0 {
            continue;
        }
        let max = pilha(item).max(1);
        for s in sim.iter_mut() {
            if s.1 > 0 && s.0 == item && !s.2 && s.1 < max {
                let entra = qtd.min(max - s.1);
                s.1 += entra;
                qtd -= entra;
                if qtd == 0 {
                    break;
                }
            }
        }
        while qtd > 0 {
            let Some(vazio) = sim.iter_mut().find(|s| s.1 == 0) else {
                return false;
            };
            let entra = qtd.min(max);
            *vazio = (item, entra, false);
            qtd -= entra;
        }
    }
    true
}

/// Uma impressao da bolsa: a coleta pausada so' tenta de novo quando ela
/// MUDA (vendeu, jogou fora, usou). Tentar a cada ciclo com a bolsa igual so'
/// piscaria a barra entre pausado e coletando.
pub fn impressao_da_bolsa(inv: &[shared::InventorySlot]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in inv {
        for v in [s.item_id as u64, s.qty as u64, s.instance.is_some() as u64] {
            h ^= v;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// A coleta continua pausada? So' enquanto a bolsa for a mesma da pausa.
pub fn continua_pausada(pausa: Option<u64>, bolsa_agora: u64) -> bool {
    pausa == Some(bolsa_agora)
}

#[cfg(test)]
mod testes {
    use super::*;
    use shared::terreno::{Bioma, Ilha, TipoDeEstorvo, ARQUIPELAGO, ESCALA_ALTURA};

    #[test]
    fn o_ponto_de_coleta_esta_ao_alcance_e_longe_nao() {
        let centro = Vec2::new(10.0, -4.0);
        let p = ponto_de_coleta(centro, 0.6, Vec2::new(0.0, 0.0));
        assert!(ao_alcance(p, centro, 0.6));
        assert!(
            p.distance(centro) > 0.6 + shared::ENTITY_RADIUS,
            "nao fica dentro do no'"
        );
        assert!(!ao_alcance(centro + Vec2::new(5.0, 0.0), centro, 0.6));
    }

    #[test]
    fn aproximacoes_suportam_parada_antecipada_do_cliente() {
        let centro = Vec2::new(10.0, -4.0);
        for raio in [0.3, 0.6, 1.5] {
            for p in pontos_de_coleta(centro, raio, Vec2::ZERO) {
                assert!(aproximacao_ao_alcance(p, centro, raio));
                let parada = p + (p - centro).normalize() * shared::COLETA_TOLERANCIA_CHEGADA;
                assert!(ao_alcance(parada, centro, raio));
                assert!(!aproximacao_ao_alcance(p + (p - centro).normalize(), centro, raio));
            }
        }
    }

    #[test]
    fn bolsa_cheia_nao_cabe_e_a_pausa_so_sai_quando_a_bolsa_muda() {
        use shared::InventorySlot;
        let pilha = |id: u16| if id == 7 { 100 } else { 1 };
        let slot = |id: u16, qty: u32| InventorySlot {
            item_id: id,
            qty,
            instance: None,
        };
        // 40 espacos ocupados por itens que nao empilham.
        let mut cheia: Vec<InventorySlot> = (0..shared::INVENTORY_SLOTS)
            .map(|i| slot(1000 + i as u16, 1))
            .collect();
        assert!(
            !cabe_tudo(&cheia, &[(7, 40)], &pilha),
            "sem espaco nao cabe"
        );
        // Uma pilha do mesmo material com folga: cabe o que a folga comporta.
        cheia[3] = slot(7, 70);
        assert!(cabe_tudo(&cheia, &[(7, 30)], &pilha));
        assert!(
            !cabe_tudo(&cheia, &[(7, 31)], &pilha),
            "31 nao cabe em 30 de folga"
        );
        // Dois itens: um cabe, o outro nao -> o ciclo inteiro nao cabe.
        assert!(
            !cabe_tudo(&cheia, &[(7, 10), (8, 1)], &pilha),
            "nao conclui pela metade"
        );
        // Pausa: igual continua, mudou sai.
        let antes = impressao_da_bolsa(&cheia);
        assert!(continua_pausada(Some(antes), impressao_da_bolsa(&cheia)));
        cheia[10] = InventorySlot::default();
        let depois = impressao_da_bolsa(&cheia);
        assert_ne!(antes, depois);
        assert!(
            !continua_pausada(Some(antes), depois),
            "abriu espaco: tenta de novo"
        );
        assert!(!continua_pausada(None, depois));
        assert!(
            cabe_tudo(&cheia, &[(8, 1)], &pilha),
            "com o espaco aberto, cabe"
        );
    }

    #[test]
    fn ciclo_por_tier_e_filtro_de_tipo() {
        assert_eq!(shared::ciclo_de_coleta_s(0), shared::COLETA_CICLO_ARVORE_S);
        for t in 1..=4u8 {
            assert_eq!(
                shared::ciclo_de_coleta_s(t),
                shared::COLETA_CICLO_PEDRA_S[t as usize]
            );
        }
        let tipos = [true, false, false, true, false];
        assert!(aceita(&tipos, 0) && aceita(&tipos, 3));
        assert!(!aceita(&tipos, 1) && !aceita(&tipos, 9));
    }

    /// No' esgotado some da colisao e da lista de coletaveis; ao voltar,
    /// volta nos dois.
    #[test]
    fn esgotado_nao_barra_e_volta_ao_respawn() {
        let d = &ARQUIPELAGO[0];
        let mut ilha = Ilha::gerar(d.semente, 220, Bioma::Floresta, ESCALA_ALTURA);
        let e = *ilha
            .todos_os_estorvos()
            .iter()
            .find(|e| {
                matches!(
                    e.tipo,
                    TipoDeEstorvo::Tronco | TipoDeEstorvo::Minerio(_) | TipoDeEstorvo::Energia
                )
            })
            .expect("ilha de teste sem no'");
        assert!(!ilha.sem_estorvo(e.centro, 0.05), "vivo barra");
        let mut achados = Vec::new();
        ilha.coletaveis_em(e.centro, 1.0, &mut achados);
        assert!(achados.iter().any(|c| c.coluna == e.coluna));
        let tirados = ilha.esconder_coluna(e.coluna);
        assert!(!tirados.is_empty());
        assert!(
            ilha.sem_estorvo(e.centro, 0.05)
                || ilha
                    .estorvo_em(e.centro, 0.05)
                    .is_some_and(|o| o.coluna != e.coluna),
            "esgotado nao barra mais"
        );
        ilha.coletaveis_em(e.centro, 1.0, &mut achados);
        assert!(
            !achados.iter().any(|c| c.coluna == e.coluna),
            "esgotado nao e' coletavel"
        );
        ilha.mostrar_estorvos(&tirados);
        assert!(!ilha.sem_estorvo(e.centro, 0.05), "voltou e barra de novo");
        ilha.coletaveis_em(e.centro, 1.0, &mut achados);
        assert!(achados.iter().any(|c| c.coluna == e.coluna));
    }
}
