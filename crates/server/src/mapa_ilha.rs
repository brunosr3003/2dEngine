//! O que o mapa do cliente mostra da ilha: as zonas de mob com os bichos que
//! nascem em cada uma e as regioes de recurso. Funcoes puras — o `World` so'
//! junta os dados e manda `MapaDaIlha`.
//!
//! A chance de cada bicho e' a MESMA conta que a auto missao usa pra escolher a
//! zona (`quests::chance_do_kind`): o mapa nao pode dizer "Lobo aqui" numa zona
//! onde a auto missao nunca mandaria caçar lobo.
use glam::Vec2;
use shared::protocol::{RegiaoNoMapa, ZonaNoMapa};

/// Lado da celula que agrupa corpos de recurso, em unidades. Da ordem do raio
/// de uma zona de mob: uma regiao e' um lugar pra onde se vai, nao um corpo.
pub const CELULA_DE_RECURSO: f32 = 48.0;
/// Menos corpos que isto numa celula nao vira regiao: pedra solta nao e' veio.
pub const MINIMO_POR_REGIAO: usize = 3;
/// Teto de regioes mandadas — as mais cheias ficam. Com 600 o pior caso da
/// mensagem fica em poucas dezenas de KB.
pub const MAX_REGIOES: usize = 600;

/// (kind, chance em %) dos bichos de uma zona de nivel `lv_min..=lv_max`, da
/// maior chance pra menor. `kinds` na ordem do sorteio (`kinds_comuns`).
///
/// Empate vai pro kind MAIOR: numa zona alta todos saem igual, e o primeiro da
/// lista e' o rotulo da zona no mapa — "Lobo · Nv 40–42" diria que la' e' lugar
/// de lobo, quando o que ela tem de proprio e' o bicho mais forte.
pub fn bichos_da_zona(kinds: &[u16], lv_min: u32, lv_max: u32) -> Vec<(u16, u8)> {
    let mut v: Vec<(u16, u8)> = kinds
        .iter()
        .map(|k| {
            (
                *k,
                (crate::quests::chance_do_kind(kinds, *k, lv_min, lv_max) * 100.0).round() as u8,
            )
        })
        .filter(|(_, c)| *c > 0)
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    v
}

pub fn zona_no_mapa(
    centro: Vec2,
    raio: f32,
    lv_min: u32,
    lv_max: u32,
    kinds: &[u16],
) -> ZonaNoMapa {
    ZonaNoMapa {
        centro: [quantiza(centro.x), quantiza(centro.y)],
        raio: raio.round(),
        lv_min: lv_min.min(u16::MAX as u32) as u16,
        lv_max: lv_max.min(u16::MAX as u32) as u16,
        bichos: bichos_da_zona(kinds, lv_min, lv_max),
    }
}

/// Agrupa corpos coletaveis `(centro, tipo)` em regioes por celula e tipo.
/// Tipo 0 = madeira, 1..4 = pedra pela cor. Ordem estavel: da mais cheia pra
/// menos, desempatando pela posicao — duas subidas do servidor mandam igual.
pub fn regioes_de_recurso(corpos: &[(Vec2, u8)]) -> Vec<RegiaoNoMapa> {
    use std::collections::HashMap;
    let mut grupos: HashMap<(i32, i32, u8), Vec<Vec2>> = HashMap::new();
    for (p, tipo) in corpos {
        let chave = (
            (p.x / CELULA_DE_RECURSO).floor() as i32,
            (p.y / CELULA_DE_RECURSO).floor() as i32,
            *tipo,
        );
        grupos.entry(chave).or_default().push(*p);
    }
    let mut v: Vec<RegiaoNoMapa> = grupos
        .into_iter()
        .filter(|(_, ps)| ps.len() >= MINIMO_POR_REGIAO)
        .map(|((_, _, tipo), ps)| {
            let c = ps.iter().fold(Vec2::ZERO, |a, b| a + *b) / ps.len() as f32;
            let r = ps.iter().map(|p| p.distance(c)).fold(0.0f32, f32::max);
            RegiaoNoMapa {
                centro: [quantiza(c.x), quantiza(c.y)],
                // Arredonda pra CIMA: a regiao tem que cobrir os corpos dela
                // mesmo depois de o centro ser quantizado.
                raio: (r + 0.5).ceil().clamp(4.0, CELULA_DE_RECURSO * 1.5),
                tipo,
                contagem: ps.len().min(u16::MAX as usize) as u16,
            }
        })
        .collect();
    v.sort_by(|a, b| {
        b.contagem
            .cmp(&a.contagem)
            .then(a.tipo.cmp(&b.tipo))
            .then(a.centro[0].total_cmp(&b.centro[0]))
            .then(a.centro[1].total_cmp(&b.centro[1]))
    });
    v.truncate(MAX_REGIOES);
    v
}

/// O que cada cor de pedra rende, pro tooltip: os itens mais provaveis daquela
/// cor com quantidade e chance. `linhas` = `economy::linhas_da_pedra`.
pub fn rendimentos_da_pedra(
    linhas: &[(u8, u16, i32, i32, f32)],
    nome: impl Fn(u16) -> String,
) -> Vec<(u8, String)> {
    (1..=4u8)
        .map(|tier| {
            let mut da_cor: Vec<_> = linhas.iter().filter(|l| l.0 == tier).collect();
            da_cor.sort_by(|a, b| b.4.total_cmp(&a.4).then(a.1.cmp(&b.1)));
            let partes: Vec<String> = da_cor
                .iter()
                .take(4)
                .map(|(_, item, mn, mx, chance)| {
                    let qtd = if mn == mx {
                        format!("{mn}")
                    } else {
                        format!("{mn}–{mx}")
                    };
                    format!("{} {qtd} ({:.0}%)", nome(*item), chance * 100.0)
                })
                .collect();
            (tier, partes.join(", "))
        })
        .collect()
}

/// Meia unidade basta pro mapa, e o numero fica curto no fio.
fn quantiza(x: f32) -> f32 {
    (x * 2.0).round() / 2.0
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O bicho que o mapa diz dominar a zona e' o que a auto missao manda caçar
    /// la': mesma conta.
    #[test]
    fn bichos_da_zona_batem_com_a_auto_missao() {
        let kinds: Vec<u16> = (0..8).collect();
        let b = bichos_da_zona(&kinds, 1, 3);
        // Nivel 1..3: niveis 1 e 2 so' sorteiam o kind 0, nivel 3 sorteia 0 e 1.
        assert_eq!(b, vec![(0, 83), (1, 17)]);
        for (k, pct) in &b {
            let c = crate::quests::chance_do_kind(&kinds, *k, 1, 3);
            assert_eq!(*pct, (c * 100.0).round() as u8);
        }
        let zonas = [
            (Vec2::new(10.0, 0.0), 1, 3),
            (Vec2::new(500.0, 0.0), 20, 25),
        ];
        let dominante = b[0].0;
        assert_eq!(
            crate::quests::zona_do_bicho(&zonas, &kinds, &[dominante], Vec2::ZERO, 1),
            Some(Vec2::new(10.0, 0.0)),
            "a auto missao mandaria caçar o dominante em outra zona"
        );
        // Zona alta: bicho de nivel alto aparece com chance, e o soma ~100%.
        let alta = bichos_da_zona(&kinds, 20, 25);
        let soma: u32 = alta.iter().map(|(_, c)| *c as u32).sum();
        assert!((98..=102).contains(&soma), "chances somam {soma}%");
        assert!(alta.iter().any(|(k, _)| *k == 7));
    }

    /// Cada corpo de uma celula cheia cai numa regiao do tipo dele e dentro do
    /// raio dela; celula rala nao vira regiao.
    #[test]
    fn regioes_cobrem_os_corpos_e_separam_por_tipo() {
        let mut corpos = Vec::new();
        // Veio azul, bosque e um veio cinza na mesma celula do bosque.
        for i in 0..10 {
            corpos.push((Vec2::new(100.0 + i as f32, 100.0 + (i % 3) as f32), 3));
            corpos.push((Vec2::new(5.0 + (i % 4) as f32 * 3.0, 7.0 + i as f32), 0));
        }
        for i in 0..4 {
            corpos.push((Vec2::new(20.0 + i as f32, 20.0), 1));
        }
        // Pedra roxa solta: nao e' regiao.
        corpos.push((Vec2::new(300.0, 300.0), 4));
        corpos.push((Vec2::new(301.0, 300.0), 4));
        let r = regioes_de_recurso(&corpos);
        assert_eq!(r.len(), 3, "{r:?}");
        let soma: u32 = r.iter().map(|x| x.contagem as u32).sum();
        assert_eq!(soma, 24);
        assert!(r.iter().all(|x| x.tipo != 4));
        for (p, tipo) in corpos.iter().filter(|(_, t)| *t != 4) {
            let dona = r.iter().find(|x| x.tipo == *tipo).expect("tipo sem regiao");
            let c = Vec2::new(dona.centro[0], dona.centro[1]);
            assert!(
                p.distance(c) <= dona.raio,
                "corpo {p:?} fora da regiao {dona:?}"
            );
        }
        assert_eq!(r[0].contagem, 10, "a mais cheia vem primeiro");
        assert_eq!(regioes_de_recurso(&corpos), r, "ordem instavel");
    }

    /// Ilha cheia de pedra e tronco espalhados, 60 zonas: a mensagem inteira no
    /// fio fica em dezenas de KB, nao centenas.
    #[test]
    fn mapa_da_ilha_cabe_no_fio() {
        let mut s: u32 = 0x9E37_79B9;
        let mut sorteio = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s
        };
        let corpos: Vec<(Vec2, u8)> = (0..40_000)
            .map(|_| {
                let x = (sorteio() % 1600) as f32 - 800.0;
                let z = (sorteio() % 1600) as f32 - 800.0;
                (Vec2::new(x, z), (sorteio() % 5) as u8)
            })
            .collect();
        let recursos = regioes_de_recurso(&corpos);
        assert_eq!(recursos.len(), MAX_REGIOES, "o teto de regioes nao segurou");
        let kinds: Vec<u16> = (0..8).collect();
        let zonas: Vec<_> = (0..60)
            .map(|i| zona_no_mapa(Vec2::new(i as f32 * 20.0, 0.0), 45.0, 1 + i, 3 + i, &kinds))
            .collect();
        let nomes = kinds
            .iter()
            .map(|k| (*k, format!("Bicho numero {k}")))
            .collect();
        let rendimentos = (0..5)
            .map(|t| {
                (
                    t,
                    "Cobre 40–120 (100%), Aço 3–6 (55%), Platina 3–6 (30%)".to_string(),
                )
            })
            .collect();
        let msg = shared::protocol::ServerMessage::MapaDaIlha {
            zonas,
            recursos,
            nomes,
            rendimentos,
            chefes: Vec::new(),
        };
        let bytes = shared::protocol::encode(&msg).unwrap().len();
        println!("MapaDaIlha: {bytes} bytes");
        assert!(bytes < 48_000, "MapaDaIlha com {bytes} bytes");
    }

    #[test]
    fn rendimento_lista_o_mais_provavel_primeiro() {
        let linhas = vec![
            (1, 10, 40, 120, 1.0),
            (1, 11, 3, 6, 0.55),
            (2, 12, 1, 1, 0.01),
        ];
        let r = rendimentos_da_pedra(&linhas, |id| format!("i{id}"));
        assert_eq!(r.len(), 4);
        assert_eq!(r[0], (1, "i10 40–120 (100%), i11 3–6 (55%)".to_string()));
        assert_eq!(r[1], (2, "i12 1 (1%)".to_string()));
        assert_eq!(r[3].1, "");
    }
}
