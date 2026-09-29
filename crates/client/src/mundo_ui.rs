//! O MAPA-MUNDI: o arquipelago inteiro numa tela, com os chefes de cada ilha
//! e quanto falta pra cada um voltar.
//!
//! # A geografia nao vem da rede
//!
//! As quatro ilhas — onde ficam, que tamanho tem, que faixa de nivel sao —
//! estao em `shared::terreno::ARQUIPELAGO`, que o cliente ja' carrega. Entao
//! o mapa-mundi nao pede geografia ao servidor: ele pede so' o que muda, que
//! e' o estado dos chefes. Um `ServerMessage::Mundo` sao umas poucas dezenas
//! de bytes, e nao um mapa.
//!
//! # A contagem anda sozinha
//!
//! O servidor manda a HORA em que cada chefe volta, em unix. O cliente
//! desconta do proprio relogio a cada quadro. Entre duas atualizacoes (5s) a
//! contagem continua andando na tela, e nao ha' como ela discordar do
//! servidor: os dois olham pro mesmo instante.

use macroquad::prelude::*;
use shared::bosses::{conta_regressiva, falta_pra_voltar, IlhaNoMundo};
use shared::terreno::{DefIlha, ARQUIPELAGO};

use crate::hud_estilo as estilo;

/// Estado local: a ultima noticia do servidor.
#[derive(Debug, Default)]
pub struct Mundo {
    ilhas: Vec<IlhaNoMundo>,
    /// `get_time()` do ultimo pedido, pra nao pedir a cada quadro.
    pedido_em: f64,
}

/// De quanto em quanto tempo se repede, com o mapa-mundi aberto.
///
/// Casado com o heartbeat (5s): pedir mais rapido nao traz noticia mais nova,
/// so' gasta ida e volta — o dado de outra ilha nasce naquele ritmo.
const INTERVALO_S: f64 = 5.0;

impl Mundo {
    pub fn define(&mut self, ilhas: Vec<IlhaNoMundo>) {
        self.ilhas = ilhas;
    }

    pub fn vazio(&self) -> bool {
        self.ilhas.is_empty()
    }

    /// `true` = esta' na hora de pedir de novo.
    pub fn precisa_pedir(&mut self, agora: f64) -> bool {
        if agora - self.pedido_em < INTERVALO_S {
            return false;
        }
        self.pedido_em = agora;
        true
    }

    /// Esquece o pedido: a proxima abertura pede na hora.
    pub fn fechou(&mut self) {
        self.pedido_em = 0.0;
    }

    fn da_zona(&self, z: &str) -> Option<&IlhaNoMundo> {
        self.ilhas.iter().find(|i| i.zona == z)
    }
}

/// A caixa que cabe todas as ilhas, em unidades de mundo, com uma folga.
///
/// Calculada da tabela e nao escrita a mao: mover uma ilha no `ARQUIPELAGO`
/// nao pode exigir lembrar de ajustar o enquadramento do mapa aqui.
pub fn caixa_do_arquipelago() -> (Vec2, f32) {
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for d in ARQUIPELAGO {
        let c = vec2(d.centro[0], d.centro[1]);
        let r = d.raio_m();
        lo = lo.min(c - r);
        hi = hi.max(c + r);
    }
    let centro = (lo + hi) * 0.5;
    // Meia-extensao do lado maior: o mapa e' quadrado, entao as duas direcoes
    // usam a mesma escala e as ilhas nao ficam ovais.
    let meia = ((hi - lo) * 0.5).max_element() * 1.12;
    (centro, meia.max(1.0))
}

/// Desenha o mapa-mundi dentro de `r`. `zona_atual` ganha destaque.
///
/// Devolve a zona clicada, se houver — o painel de cima decide o que fazer
/// com ela (hoje, nada: viajar continua sendo com o Capitao).
pub fn desenha(
    mundo: &Mundo,
    r: Rect,
    zona_atual: Option<&str>,
    agora_unix: i64,
    u: impl Fn(f32) -> f32,
) -> Option<&'static DefIlha> {
    let (centro, meia) = caixa_do_arquipelago();
    let escala = r.w / (2.0 * meia);
    let ponto = |p: Vec2| {
        vec2(
            r.x + r.w * 0.5 + (p.x - centro.x) * escala,
            r.y + r.h * 0.5 + (p.y - centro.y) * escala,
        )
    };
    let m = Vec2::from(mouse_position());
    let clicou = crate::foco::clique();
    draw_rectangle(r.x, r.y, r.w, r.h, COR_OCEANO);
    malha_do_oceano(r, u(1.0));

    let mut escolhida = None;
    for d in ARQUIPELAGO.iter() {
        let c = ponto(vec2(d.centro[0], d.centro[1]));
        let raio = (d.raio_m() * escala).max(u(10.0));
        let aqui = zona_atual == Some(d.zona);
        let info = mundo.da_zona(d.zona);
        let no_ar = info.is_none_or(|i| i.no_ar);
        // Ilha fora do ar fica APAGADA. Nao e' decoracao: e' a diferenca
        // entre "o chefe de la' esta' vivo" e "ninguem sabe o que ha' la'".
        let a = if no_ar { 1.0 } else { 0.45 };
        let terra = Color::new(0.30, 0.42, 0.26, a);
        draw_circle(c.x, c.y, raio, terra);
        draw_circle_lines(
            c.x,
            c.y,
            raio,
            u(if aqui { 3.0 } else { 1.5 }),
            if aqui {
                estilo::OURO
            } else {
                Color::new(0.62, 0.72, 0.55, a)
            },
        );
        if aqui {
            draw_circle_lines(
                c.x,
                c.y,
                raio + u(5.0),
                u(1.0),
                Color::new(0.97, 0.79, 0.44, 0.5),
            );
        }
        // Nome e faixa de nivel.
        let titulo = if aqui {
            format!("{} · you are here", d.nome)
        } else if no_ar {
            format!("{} · Nv {}–{}", d.nome, d.nivel.0, d.nivel.1)
        } else {
            format!("{} · offline", d.nome)
        };
        estilo::texto_centro(
            c.x,
            c.y - raio - u(8.0),
            &titulo,
            13,
            if aqui { estilo::OURO } else { estilo::TEXTO },
        );
        // Os chefes, em volta do centro da ilha.
        let chefes = info.map(|i| i.chefes.as_slice()).unwrap_or(&[]);
        for (k, ch) in chefes.iter().enumerate() {
            let ang = -std::f32::consts::FRAC_PI_2
                + k as f32 * std::f32::consts::TAU / chefes.len().max(1) as f32;
            let q = c + vec2(ang.cos(), ang.sin()) * raio * 0.52;
            let falta = falta_pra_voltar(ch, agora_unix);
            let cor = match falta {
                None => Color::new(1.0, 0.72, 0.25, a),
                Some(_) => Color::new(0.55, 0.58, 0.62, a),
            };
            if !crate::icones_ui::mapa("chefe", q, u(20.0), cor, 0.0) {
                crate::telegrafico::desenha_coroa(q, u(6.0));
            }
            let rotulo = match falta {
                None => "vivo".to_string(),
                Some(s) => conta_regressiva(s),
            };
            estilo::texto_centro(
                q.x + 1.0,
                q.y + u(19.0),
                &rotulo,
                11,
                Color::new(0.0, 0.0, 0.0, 0.85),
            );
            estilo::texto_centro(q.x, q.y + u(18.0), &rotulo, 11, cor);
        }
        if clicou && c.distance(m) <= raio {
            escolhida = Some(d);
        }
    }
    escolhida
}

const COR_OCEANO: Color = Color::new(0.07, 0.13, 0.22, 1.0);

/// Linhas de longitude/latitude bem fracas: dao escala ao oceano vazio e
/// custam oito `draw_line`.
fn malha_do_oceano(r: Rect, esp: f32) {
    let c = Color::new(0.35, 0.55, 0.75, 0.10);
    for i in 1..4 {
        let t = i as f32 / 4.0;
        draw_line(r.x + r.w * t, r.y, r.x + r.w * t, r.y + r.h, esp, c);
        draw_line(r.x, r.y + r.h * t, r.x + r.w, r.y + r.h * t, esp, c);
    }
}

/// As linhas do painel lateral: cada chefe do mundo, ordenado por quem volta
/// primeiro. E' esta lista que responde "o que da' pra caçar AGORA".
pub fn linhas_de_chefe(mundo: &Mundo, agora_unix: i64) -> Vec<(String, String, bool)> {
    let mut v: Vec<(String, String, bool, i64)> = Vec::new();
    for d in ARQUIPELAGO {
        let Some(i) = mundo.da_zona(d.zona) else {
            continue;
        };
        for ch in &i.chefes {
            let falta = falta_pra_voltar(ch, agora_unix);
            let (texto, vivo, ordem) = match falta {
                None if i.no_ar => ("vivo".to_string(), true, -1),
                None => ("?".to_string(), false, i64::MAX),
                Some(s) => (conta_regressiva(s), false, s),
            };
            v.push((
                format!("{} · Nv {}", ch.nome, ch.nivel),
                format!("{} · {texto}", d.nome),
                vivo,
                ordem,
            ));
        }
    }
    // Vivo primeiro, e depois quem volta antes: e' a ordem em que o jogador
    // decide pra onde ir.
    v.sort_by_key(|(_, _, _, o)| *o);
    v.into_iter().map(|(a, b, c, _)| (a, b, c)).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn ch(nome: &str, vivo: bool, volta: i64) -> shared::bosses::ChefeNoMapa {
        shared::bosses::ChefeNoMapa {
            kind: 0,
            nome: nome.into(),
            nivel: 10,
            centro: [0.0, 0.0],
            vivo,
            volta_em_unix: volta,
        }
    }

    fn mundo(ilhas: Vec<IlhaNoMundo>) -> Mundo {
        Mundo {
            ilhas,
            pedido_em: 0.0,
        }
    }

    /// A caixa tem que CABER todas as ilhas. Se ela fosse escrita a mao,
    /// mover uma ilha no `ARQUIPELAGO` a deixaria pela metade fora da tela —
    /// e nada avisaria.
    #[test]
    fn a_caixa_cabe_o_arquipelago_inteiro() {
        let (centro, meia) = caixa_do_arquipelago();
        for d in ARQUIPELAGO {
            let c = vec2(d.centro[0], d.centro[1]);
            let r = d.raio_m();
            assert!(
                (c.x - centro.x).abs() + r <= meia && (c.y - centro.y).abs() + r <= meia,
                "{} ({c:?}, raio {r:.0}) nao cabe na caixa (centro {centro:?}, meia {meia:.0})",
                d.nome
            );
        }
    }

    /// A lista ordena por quem da' pra caçar AGORA e depois por quem volta
    /// antes. Era isso que o dono pediu do mapa: nao "onde ficam os chefes",
    /// mas "qual deles vale a viagem neste minuto".
    #[test]
    fn a_lista_poe_o_vivo_na_frente_e_ordena_pela_volta() {
        let z = |zona: &str, chefes: Vec<shared::bosses::ChefeNoMapa>| IlhaNoMundo {
            zona: zona.into(),
            chefes,
            no_ar: true,
        };
        let m = mundo(vec![
            z(
                ARQUIPELAGO[0].zona,
                vec![ch("Tarde", false, 1000), ch("Vivo", true, 0)],
            ),
            z(ARQUIPELAGO[1].zona, vec![ch("Cedo", false, 200)]),
        ]);
        let l = linhas_de_chefe(&m, 100);
        let nomes: Vec<&str> = l
            .iter()
            .map(|(n, _, _)| n.split(' ').next().unwrap())
            .collect();
        assert_eq!(nomes, ["Vivo", "Cedo", "Tarde"], "{l:?}");
        assert!(l[0].2, "o primeiro tem que estar vivo");
        assert!(l[1].1.contains("1m40s"), "{:?}", l[1]);
    }

    /// Ilha fora do ar nao pode dizer "vivo": ninguem sabe. Dizer que esta'
    /// vivo manda o jogador atravessar o mar atras de um chefe que talvez
    /// nem exista.
    #[test]
    fn ilha_fora_do_ar_nao_afirma_nada() {
        let m = mundo(vec![IlhaNoMundo {
            zona: ARQUIPELAGO[0].zona.into(),
            chefes: vec![ch("X", true, 0)],
            no_ar: false,
        }]);
        let l = linhas_de_chefe(&m, 100);
        assert_eq!(l.len(), 1);
        assert!(!l[0].2, "fora do ar nao e' 'vivo'");
        assert!(l[0].1.ends_with("?"), "{:?}", l[0]);
    }

    /// Nao pede a cada quadro: o dado nasce a cada 5s no heartbeat, entao
    /// pedir a 60 Hz seriam 300 idas pra 1 noticia.
    #[test]
    fn repede_no_ritmo_do_heartbeat() {
        let mut m = mundo(Vec::new());
        assert!(m.precisa_pedir(100.0), "a primeira vez pede");
        assert!(!m.precisa_pedir(102.0), "2s depois nao");
        assert!(m.precisa_pedir(105.5), "passados os 5s, pede");
        m.fechou();
        assert!(m.precisa_pedir(105.6), "reabrir pede na hora");
    }
}
