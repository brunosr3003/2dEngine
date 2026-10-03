//! A VILA da ilha: onde cada casa, prop e NPC fica — na cidade e no porto.
//!
//! Funcao pura do `Gerador`, como todo o resto do terreno: o servidor tira
//! daqui a colisao e os NPCs, o cliente tira as malhas, e nada viaja pela
//! rede. A disposicao LE o relevo pronto (`bloco_em`); o relevo nao le a
//! disposicao — o plato e o pier sao do gerador, as casas assentam onde o
//! chao ja' esta' no nivel.
//!
//! Base do algoritmo: `Porto.Gerar` do zone14 (anel de oficios virado pra
//! praca, voltas com raio crescente, cais com armazem e cartografo).

use glam::{Vec2, Vec3};

use crate::construcao::{
    self, frente_de, quarto_para, Construcao, Papel, Rng, TipoCasa, TipoProp, B_CASA, DECK_Y,
};
use crate::terreno::{Cidade, Gerador, Material, SitioPorto, BLOCO, NIVEL_DO_MAR};

/// Loja do banco que o Alquimista abre: so' pocoes.
pub const LOJA_DE_POCOES: u32 = 3;

/// Caixa acima disto (a partir do chao do predio) nao barra: beiral, toldo,
/// placa, verga e telhado passam por cima da cabeca.
const TETO_DA_COLISAO: f32 = 1.7;
/// Caixa que nao passa disto acima do chao nao barra: alicerce, soleira e
/// tabuado ficam no plano do chao que o relevo ja' da'.
const PISO_DA_COLISAO: f32 = 0.31;
/// O alicerce sobe 2 cm acima do chao: a face de cima dele e a do terreno
/// no mesmo plano brigariam no depth buffer.
const RESPIRO: f32 = 0.02;
/// Mesmo respiro pro tabuado do cais, um pouco maior (ele e' fino).
const RESPIRO_DO_CAIS: f32 = 0.03;
/// NPC de porta: quanto a frente da fachada ele fica.
const NPC_NA_PORTA: f32 = 1.2;

#[derive(Debug, Clone, PartialEq)]
pub struct Predio {
    pub tipo: TipoCasa,
    pub papel: Papel,
    pub seed: i32,
    /// `xz` = onde fica o PIVO (centro do volume); `y` = origem do volume.
    pub pos: Vec3,
    /// Quarto de volta (0..3). A frente (porta) olha pra `frente_de(yaw_q)`.
    pub yaw_q: u8,
    /// Altura do chao em que se pisa dentro (ou em cima, na doca).
    pub chao: f32,
}

impl Predio {
    pub fn construcao(&self) -> Construcao {
        construcao::gerar(self.tipo, self.papel, self.seed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropPosto {
    pub tipo: TipoProp,
    pub seed: i32,
    /// `xz` = pivo, `y` = base (o chao).
    pub pos: Vec3,
    pub yaw_q: u8,
}

impl PropPosto {
    pub fn construcao(&self) -> Construcao {
        construcao::gerar_prop(self.tipo, self.seed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NpcDaVila {
    pub papel: Papel,
    pub nome: &'static str,
    pub pos: Vec2,
    /// Rumo pra onde olha, na convencao do cliente: `atan2(dir.x, dir.z)`.
    pub yaw: f32,
    pub loja: Option<u32>,
    /// Postos usam um giver proprio em vez do papel visual do NPC.
    pub giver: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortoDaVila {
    pub centro: Vec2,
    pub raio: f32,
    /// Onde o cais toca a costa, e a ponta dele no mar.
    pub raiz: Vec2,
    pub ponta: Vec2,
    /// Direcao do mar.
    pub mar: Vec2,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vila {
    pub predios: Vec<Predio>,
    pub props: Vec<PropPosto>,
    pub npcs: Vec<NpcDaVila>,
    pub porto: Option<PortoDaVila>,
    /// Caminhos calcados (segmentos), pracas calcadas e gramados cuidados:
    /// so' tinta do chao, ver `pintura_em`.
    pub caminhos: Vec<(Vec2, Vec2)>,
    pub pracas: Vec<(Vec2, f32)>,
    pub gramados: Vec<(Vec2, f32)>,
}

/// Caixa solida projetada no chao, em unidades de mundo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caixa2 {
    pub min: Vec2,
    pub max: Vec2,
}

/// Rumo (convencao do cliente) de uma direcao no plano `(x, z)`.
pub fn yaw_de(dir: Vec2) -> f32 {
    dir.x.atan2(dir.y)
}

pub fn montar(ger: &Gerador) -> Vila {
    let mut vila = Vila::default();
    if let Some(c) = ger.cidade() {
        montar_cidade(ger, &c, &mut vila);
    }
    if let Some(p) = ger.porto() {
        montar_porto(ger, &p, &mut vila);
    } else if let Some(c) = ger.cidade().filter(|_| ger.semente == crate::celeste::SEMENTE) {
        // SKYREACH HAS NO PORT, and the Harbour Captain only stood on one: no
        // Captain, no travel, and whoever sailed up could not leave (the
        // owner: "how do I leave the sky islands?"). He keeps the town square,
        // in the gap between the shop doors, facing the middle.
        let pos = c.centro() + Vec2::new(-4.8, -3.6);
        vila.npcs.push(NpcDaVila {
            papel: Papel::Estaleiro,
            nome: Papel::Estaleiro.nome(),
            pos,
            yaw: yaw_de(c.centro() - pos),
            loja: None,
            giver: None,
        });
    }
    // THE FLYING BUS (Kōgen-tō): its driver, the professor, with the bus
    // parked beside him. In Skyreach he waits on the THRONE OF THE SKY, the
    // highest island, where chapter V ends — the owner wanted him apart from
    // the Captain and the town, in his own corner of the map — opposite the
    // Archon's spot. In Kōgen-tō he waits on the Docks lawn east of the
    // shops (the way out).
    let lugar_do_onibus = if ger.semente == crate::celeste::SEMENTE {
        let trono = crate::celeste::PLATOS[11].centro;
        Some((trono, Vec2::new(22.0, -22.0), Vec2::new(30.0, -30.0)))
    } else if ger.semente == crate::kogen::SEMENTE {
        ger.cidade().map(|c| (c.centro(), Vec2::new(27.0, -4.0), Vec2::new(33.0, 0.0)))
    } else {
        None
    };
    if let Some((centro, a, b)) = lugar_do_onibus {
        let c = centro;
        let pos = c + a;
        let onibus = c + b;
        vila.props.push(PropPosto {
            tipo: TipoProp::OnibusVoador,
            seed: ger.semente,
            pos: Vec3::new(onibus.x, ger.altura(onibus.x, onibus.y), onibus.y),
            yaw_q: 0,
        });
        vila.npcs.push(NpcDaVila {
            papel: Papel::Motorista,
            nome: Papel::Motorista.nome(),
            pos,
            yaw: yaw_de(c - pos),
            loja: None,
            giver: None,
        });
    }
    // KŌGEN-TŌ's boulevard: glowing sakura and lanterns down its median,
    // torii gates spanning it through the Shrine Forest. Decoration only:
    // none of them blocks the way (`prop_barra`).
    if ger.semente == crate::kogen::SEMENTE {
        use crate::kogen as k;
        let livre = |p: Vec2| {
            k::chao_em(p) == k::Chao::Parque
                && k::deck_em(p).is_none()
                && k::arenas().iter().all(|a| a.distance(p) > k::RAIO_ARENA + 6.0)
        };
        let mut z = k::AVENIDA_DO_CAIS - 10.0;
        let mut i = 0;
        while z > -k::COSTA_B {
            let p = Vec2::new(k::boulevard_x(z), z);
            if livre(p) {
                let tipo = if i % 3 == 2 { TipoProp::Lanterna } else { TipoProp::Sakura };
                vila.props.push(PropPosto { tipo, seed: i, pos: Vec3::new(p.x, ger.altura(p.x, p.y), p.y), yaw_q: 0 });
            }
            z -= 14.0;
            i += 1;
        }
        for z in [45.0f32, 90.0, 135.0, 180.0, 225.0] {
            let p = Vec2::new(k::boulevard_x(z), z);
            vila.props.push(PropPosto { tipo: TipoProp::Torii, seed: 0, pos: Vec3::new(p.x, ger.altura(p.x, p.y), p.y), yaw_q: 0 });
        }
    }
    montar_postos(ger, &mut vila);
    if let Some(pl) = ger.planalto() {
        for (i,r) in pl.regioes.iter().enumerate() {
            // The lighthouse and the crystal stay. The three storm RUINS were
            // square walls of black stone that read as little castles on the
            // map, and the owner asked for them out (30/09/2026) — the one
            // castle on this island is Last Refuge's (`planalto::muralha`).
            let tipo = match i { 4 => Some(TipoProp::FarolTormenta), 2 => Some(TipoProp::CristalTormenta), _ => None };
            let p = r.centro + Vec2::new(22.0,-22.0);
            if let Some(tipo) = tipo {
                vila.props.push(PropPosto { tipo, seed: ger.semente, pos: Vec3::new(p.x,ger.altura(p.x,p.y),p.y), yaw_q: 0 });
            }
            if i == 1 || i == 3 {
                let p = r.centro + Vec2::new(-10.0,-12.0);
                vila.props.push(PropPosto { tipo: TipoProp::Portal, seed: ger.semente, pos: Vec3::new(p.x,ger.altura(p.x,p.y),p.y), yaw_q: 0 });
            }
        }
    }
    vila
}

/// Cabanas de missao fora da cidade. A busca e deterministica e usa o relevo
/// pronto: cliente, servidor, mapa e colisao recebem exatamente o mesmo posto.
fn montar_postos(ger: &Gerador, vila: &mut Vila) {
    let indice = crate::terreno::ARQUIPELAGO
        .iter()
        .take(3)
        .position(|def| def.semente == ger.semente);
    let Some(indice) = indice else {
        return;
    };
    let grupos: &[u16] = match indice {
        0 => &[201, 202, 203],
        1 => &[204, 205, 206, 207],
        _ => &[208, 209, 210],
    };
    let centro = ger.cidade().map_or(Vec2::ZERO, |c| c.centro());
    let porto = ger.porto().map(|p| p.centro);
    let raio = ger.raio_blocos as f32 * BLOCO;
    let modelo = construcao::gerar(TipoCasa::Cabana, Papel::Missoes, ger.semente ^ 0xCA8A);
    for (n, &giver) in grupos.iter().enumerate() {
        let angulo = (n as f32 + 0.45) / grupos.len() as f32 * std::f32::consts::TAU;
        let mut escolhido = None;
        for passo in 0..20 {
            let distancia = raio * (0.36 + passo as f32 * 0.025);
            for desvio in [
                0.0f32, 0.12, -0.12, 0.24, -0.24, 0.36, -0.36, 0.48, -0.48, 0.72, -0.72, 0.96,
                -0.96,
            ] {
                let a = angulo + desvio;
                let p = Vec2::new(a.cos(), a.sin()) * distancia;
                if p.distance(centro) < 100.0
                    || porto.is_some_and(|porto| p.distance(porto) < 85.0)
                    || vila
                        .predios
                        .iter()
                        .any(|c| p.distance(Vec2::new(c.pos.x, c.pos.z)) < 75.0)
                {
                    continue;
                }
                let q = quarto_para(centro - p);
                let meia = modelo.meia(q) + Vec2::splat(2.0);
                let nivel =
                    ger.bloco_em((p.x / BLOCO).round() as i32, (p.y / BLOCO).round() as i32);
                if (nivel + 1) as f32 * BLOCO <= NIVEL_DO_MAR + BLOCO {
                    continue;
                }
                // Plano bloco a bloco, com folga em volta: cabana em degrau
                // flutua ou deixa a soleira alta demais pra entrar andando.
                let plano =
                    (-(meia.x / BLOCO).ceil() as i32..=(meia.x / BLOCO).ceil() as i32).all(|dx| {
                        (-(meia.y / BLOCO).ceil() as i32..=(meia.y / BLOCO).ceil() as i32).all(
                            |dz| {
                                let s = p + Vec2::new(dx as f32, dz as f32) * BLOCO;
                                ger.bloco_em(
                                    (s.x / BLOCO).round() as i32,
                                    (s.y / BLOCO).round() as i32,
                                ) == nivel
                            },
                        )
                    });
                if plano {
                    escolhido = Some((p, q, (nivel + 1) as f32 * BLOCO));
                    break;
                }
            }
            if escolhido.is_some() {
                break;
            }
        }
        let Some((p, q, chao)) = escolhido else {
            continue;
        };
        let predio = Predio {
            tipo: TipoCasa::Cabana,
            papel: Papel::Missoes,
            seed: ger.semente.wrapping_add(giver as i32 * 113),
            pos: Vec3::new(p.x, chao - B_CASA + RESPIRO, p.y),
            yaw_q: q,
            chao,
        };
        let Some(mut npc) = npc_da_porta(&predio) else {
            continue;
        };
        npc.nome = crate::quests::nome_do_posto(giver).unwrap_or("Watchman");
        npc.giver = Some(giver);
        vila.predios.push(predio);
        vila.npcs.push(npc);
    }
}

/// O que barra, de todos os predios e do poco.
pub fn caixas_solidas(vila: &Vila) -> Vec<Caixa2> {
    let mut saida = Vec::new();
    let mut junta = |caixas: Vec<(Vec3, Vec3)>, chao: f32| {
        for (mn, mx) in caixas {
            if mn.y - chao < TETO_DA_COLISAO && mx.y - chao > PISO_DA_COLISAO {
                saida.push(Caixa2 {
                    min: Vec2::new(mn.x, mn.z),
                    max: Vec2::new(mx.x, mx.z),
                });
            }
        }
    };
    for p in &vila.predios {
        junta(p.construcao().caixas_mundo(p.pos, p.yaw_q), p.chao);
    }
    for pr in &vila.props {
        if prop_barra(pr.tipo) {
            junta(pr.construcao().caixas_mundo(pr.pos, pr.yaw_q), pr.pos.y);
        }
    }
    saida
}

// ─────────────────────────────── lotes ───────────────────────────────

/// Mapa de "esta coluna esta' no nivel do plato" em volta de um sitio.
/// Montado uma vez: a busca de lote testa centenas de retangulos.
struct Chao {
    x0: i32,
    z0: i32,
    lado: i32,
    plano: Vec<bool>,
}

impl Chao {
    fn novo(ger: &Gerador, centro: Vec2, raio: f32, nivel: i32) -> Self {
        let r = (raio / BLOCO).ceil() as i32;
        let (cx, cz) = (
            (centro.x / BLOCO).round() as i32,
            (centro.y / BLOCO).round() as i32,
        );
        let lado = r * 2 + 1;
        let mut plano = vec![false; (lado * lado) as usize];
        for dz in 0..lado {
            for dx in 0..lado {
                plano[(dz * lado + dx) as usize] = ger.bloco_em(cx - r + dx, cz - r + dz) == nivel;
            }
        }
        Self {
            x0: cx - r,
            z0: cz - r,
            lado,
            plano,
        }
    }

    fn plano_em(&self, x: f32, z: f32) -> bool {
        let (ix, iz) = (
            (x / BLOCO).round() as i32 - self.x0,
            (z / BLOCO).round() as i32 - self.z0,
        );
        ix >= 0
            && iz >= 0
            && ix < self.lado
            && iz < self.lado
            && self.plano[(iz * self.lado + ix) as usize]
    }

    /// O retangulo (com folga) inteiro no nivel, e sem pisar em outro lote.
    fn cabe(&self, lotes: &[(Vec2, Vec2)], centro: Vec2, meia: Vec2, folga: f32) -> bool {
        for (c, m) in lotes {
            if (centro.x - c.x).abs() < meia.x + m.x + folga
                && (centro.y - c.y).abs() < meia.y + m.y + folga
            {
                return false;
            }
        }
        let ext = meia + Vec2::splat(folga);
        let (nx, nz) = (
            (ext.x * 2.0 / BLOCO).ceil() as i32,
            (ext.y * 2.0 / BLOCO).ceil() as i32,
        );
        for i in 0..=nx {
            for k in 0..=nz {
                let x = (centro.x - ext.x + i as f32 * BLOCO).min(centro.x + ext.x);
                let z = (centro.y - ext.y + k as f32 * BLOCO).min(centro.y + ext.y);
                if !self.plano_em(x, z) {
                    return false;
                }
            }
        }
        true
    }
}

/// Desvios de angulo tentados em volta do rumo ideal de cada casa.
const DESVIOS: [f32; 17] = [
    0.0, 0.18, -0.18, 0.36, -0.36, 0.55, -0.55, 0.8, -0.8, 1.1, -1.1, 1.5, -1.5, 2.0, -2.0, 2.6,
    -2.6,
];

/// Uma volta da busca: (raio minimo, raio maximo, folga do lote).
type Volta = (f32, f32, f32);
const VOLTAS: [Volta; 3] = [(11.5, 25.0, 1.0), (11.5, 40.0, 1.0), (9.0, 56.0, 0.2)];

fn corredor_livre(de: Vec2, ate: Vec2, lote: (Vec2, Vec2)) -> bool {
    let (centro, meia) = lote;
    let comprimento = de.distance(ate);
    let passos = (comprimento / 0.5).ceil().max(1.0) as usize;
    for i in 0..=passos {
        let p = de.lerp(ate, i as f32 / passos as f32);
        if (p.x - centro.x).abs() < meia.x + 0.8 && (p.y - centro.y).abs() < meia.y + 0.8 {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn assentar(
    chao: &Chao,
    lotes: &mut Vec<(Vec2, Vec2)>,
    acessos: &mut Vec<Vec2>,
    centro: Vec2,
    chao_y: f32,
    tipo: TipoCasa,
    papel: Papel,
    base_seed: i32,
    ang: f32,
) -> Option<Predio> {
    for (volta, &(rmin, rmax, folga)) in VOLTAS.iter().enumerate() {
        // Na primeira volta so' o leque perto do rumo; depois, a volta toda.
        let desvios = if volta == 0 {
            &DESVIOS[..9]
        } else {
            &DESVIOS[..]
        };
        for k in 0..8 {
            let seed = base_seed.wrapping_add(k * 7919);
            let c = construcao::gerar(tipo, papel, seed);
            for &dang in desvios {
                let a = ang + dang;
                let dir = Vec2::new(a.cos(), a.sin());
                let mut r = rmin;
                while r <= rmax {
                    let pos = centro + dir * r;
                    let q = quarto_para(centro - pos);
                    let meia = c.meia(q);
                    if chao.cabe(lotes, pos, meia, folga) {
                        let predio = Predio {
                            tipo,
                            papel,
                            seed,
                            pos: Vec3::new(pos.x, chao_y - B_CASA + RESPIRO, pos.y),
                            yaw_q: q,
                            chao: chao_y,
                        };
                        let entrada = npc_da_porta(&predio).map(|npc| npc.pos);
                        let inicio = |fim: Vec2| centro + (fim - centro).normalize_or_zero() * 7.0;
                        if entrada.is_some_and(|fim| {
                            lotes[1..]
                                .iter()
                                .any(|&lote| !corredor_livre(inicio(fim), fim, lote))
                        }) || acessos
                            .iter()
                            .any(|&fim| !corredor_livre(inicio(fim), fim, (pos, meia)))
                        {
                            r += 1.6;
                            continue;
                        }
                        lotes.push((pos, meia));
                        if let Some(entrada) = entrada {
                            acessos.push(entrada);
                        }
                        return Some(predio);
                    }
                    r += 1.6;
                }
            }
        }
    }
    None
}

/// O NPC de porta de um predio de oficio.
fn npc_da_porta(p: &Predio) -> Option<NpcDaVila> {
    if p.papel == Papel::Casa {
        return None;
    }
    let c = p.construcao();
    let lx = c.porta_local()?;
    let w = c.local_para_mundo(p.pos, p.yaw_q, Vec3::new(lx, 0.0, -NPC_NA_PORTA));
    Some(NpcDaVila {
        papel: p.papel,
        nome: p.papel.nome(),
        pos: Vec2::new(w.x, w.z),
        yaw: yaw_de(frente_de(p.yaw_q)),
        loja: (p.papel == Papel::Alquimista).then_some(LOJA_DE_POCOES),
        giver: None,
    })
}

// ─────────────────────────────── cidade ──────────────────────────────

fn montar_cidade(ger: &Gerador, c: &Cidade, vila: &mut Vila) {
    use std::f32::consts::{PI, TAU};
    let centro = c.centro();
    let chao_y = c.altura();
    let chao = Chao::novo(ger, centro, c.raio + 14.0, c.nivel);
    let seed_vila = ger.semente ^ 0x9047;
    let mut r = Rng::novo(seed_vila);
    let ang0 = r.float() * TAU;

    // A PRACA fica livre: poco, bancos e lampioes.
    let mut lotes: Vec<(Vec2, Vec2)> = vec![(centro, Vec2::splat(6.8))];
    let mut acessos = Vec::new();

    // Anel de oficios virado pro poco, e as casas de morador do outro lado.
    let oficios = [
        Papel::Alquimista,
        Papel::Ferreiro,
        Papel::Armaduras,
        Papel::Taberna,
        Papel::Alfaiate,
        Papel::Treinador,
        Papel::Identificador,
    ];
    let n = oficios.len() as f32;
    for (i, &papel) in oficios.iter().enumerate() {
        let ang = ang0 + (i as f32 - n / 2.0 + 0.5) / n * TAU * 0.82;
        let base = r.next() as i32;
        if let Some(p) = assentar(
            &chao,
            &mut lotes,
            &mut acessos,
            centro,
            chao_y,
            TipoCasa::Casebre,
            papel,
            base,
            ang,
        ) {
            vila.predios.push(p);
        }
    }
    let casas = 2 + r.int(0, 2);
    for j in 0..casas {
        let ang = ang0 + PI + (j as f32 - (casas - 1) as f32 * 0.5) * 1.1;
        let base = r.next() as i32;
        if let Some(p) = assentar(
            &chao,
            &mut lotes,
            &mut acessos,
            centro,
            chao_y,
            TipoCasa::Casebre,
            Papel::Casa,
            base,
            ang,
        ) {
            vila.predios.push(p);
        }
    }

    // Mobilia da praca.
    vila.props.push(PropPosto {
        tipo: TipoProp::Poco,
        seed: seed_vila,
        pos: Vec3::new(centro.x, chao_y, centro.y),
        yaw_q: 0,
    });
    for k in 0..3 {
        let a = ang0 + TAU / 6.0 + k as f32 * TAU / 3.0;
        let p = centro + Vec2::new(a.cos(), a.sin()) * 3.2;
        vila.props.push(PropPosto {
            tipo: TipoProp::Banco,
            seed: seed_vila.wrapping_add(k),
            pos: Vec3::new(p.x, chao_y, p.y),
            yaw_q: quarto_para(centro - p),
        });
    }
    for k in 0..4 {
        let a = ang0 + TAU / 8.0 + k as f32 * TAU / 4.0;
        let p = centro + Vec2::new(a.cos(), a.sin()) * 5.6;
        vila.props.push(PropPosto {
            tipo: TipoProp::Lampiao,
            seed: seed_vila.wrapping_add(10 + k),
            pos: Vec3::new(p.x, chao_y, p.y),
            yaw_q: 0,
        });
    }

    let npcs: Vec<NpcDaVila> = vila
        .predios
        .iter()
        .filter_map(npc_da_porta)
        .map(|mut npc| {
            // O anel da praca e' caminhavel. A porta pode ficar isolada entre
            // casas mesmo quando a soleira em si esta' livre.
            npc.pos = centro + (npc.pos - centro).normalize_or_zero() * 7.5;
            npc
        })
        .collect();
    vila.npcs.extend(npcs);

    // O MESTRE DE MISSOES fica na praca, e nao numa porta: e' o primeiro
    // rosto que quem chega procura. No rumo `ang0`, que cai entre os bancos
    // (60/180/300 graus) e os lampioes (45/135/225/315), a 4,4 do poco — a
    // ~4 de cada um deles —, olhando pro poco. Nao consome sorteio: a
    // disposicao das casas continua a mesma.
    let pos = centro + Vec2::new(ang0.cos(), ang0.sin()) * MESTRE_DO_POCO;
    vila.npcs.push(NpcDaVila {
        papel: Papel::Missoes,
        nome: Papel::Missoes.nome(),
        pos,
        yaw: yaw_de(centro - pos),
        loja: None,
        giver: None,
    });

    // O banco do porto continua; esta atendente deixa o mesmo banco
    // acessivel na cidade, perto do poco e do Mestre de Missoes.
    let ang_banco = ang0 + PI * 0.5;
    let pos_banco = centro + Vec2::new(ang_banco.cos(), ang_banco.sin()) * MESTRE_DO_POCO;
    vila.npcs.push(NpcDaVila {
        papel: Papel::Deposito,
        nome: "Banker",
        pos: pos_banco,
        yaw: yaw_de(centro - pos_banco),
        loja: None,
        giver: None,
    });

    decorar_cidade(ger, c, &chao, &lotes[1..], ang0, seed_vila, vila);
}

/// Distancia do Mestre de Missoes ao centro da praca.
const MESTRE_DO_POCO: f32 = 4.4;

// ─────────────────────────────── porto ───────────────────────────────

fn montar_porto(ger: &Gerador, p: &SitioPorto, vila: &mut Vila) {
    let centro = p.centro;
    let chao_y = p.altura();
    let mar = p.mar();
    let lado = Vec2::new(-mar.y, mar.x);
    let raiz = p.raiz;
    let seed = ger.semente ^ 0x0B0A_7E;
    // So' os predios DESTE porto ganham NPC aqui: os da cidade ja' ganharam.
    let primeiro = vila.predios.len();

    // O CAIS: a raiz (z = 0 local) na costa, a frente virada pra terra.
    let doca = construcao::gerar(TipoCasa::Doca, Papel::Estaleiro, p.doca_seed);
    let comp = doca.v.nz as f32 * B_CASA;
    let pos_doca = raiz + mar * (comp * 0.5);
    vila.predios.push(Predio {
        tipo: TipoCasa::Doca,
        papel: Papel::Estaleiro,
        seed: p.doca_seed,
        pos: Vec3::new(
            pos_doca.x,
            chao_y - (DECK_Y + 1) as f32 * B_CASA + RESPIRO_DO_CAIS,
            pos_doca.y,
        ),
        yaw_q: (p.mar_q + 2) % 4,
        chao: chao_y,
    });

    let chao = Chao::novo(ger, centro, SitioPorto::RAIO + 6.0, p.nivel);
    // O caminho do patio ate' o cais fica livre de predio.
    let meio = raiz + mar * ((comp - p.recuo) * 0.5);
    let comprido = (comp + p.recuo) * 0.5;
    let largo = p.larg * 0.5 + 2.0;
    let meia_cais = if mar.x.abs() > 0.5 {
        Vec2::new(comprido, largo)
    } else {
        Vec2::new(largo, comprido)
    };
    let mut lotes: Vec<(Vec2, Vec2)> = vec![(meio, meia_cais)];

    // Armazem e cabana do cartografo no patio, com a porta pro cais.
    const VAGAS: [(f32, f32); 10] = [
        (8.0, 9.0),
        (8.0, -9.0),
        (10.0, 9.0),
        (10.0, -9.0),
        (6.0, 10.0),
        (6.0, -10.0),
        (12.0, 10.0),
        (12.0, -10.0),
        (14.0, 12.0),
        (14.0, -12.0),
    ];
    for (i, (tipo, papel)) in [
        (TipoCasa::Armazem, Papel::Deposito),
        (TipoCasa::Cabana, Papel::Cartografo),
    ]
    .into_iter()
    .enumerate()
    {
        // O NPC DO PORTO NAO E' OPCIONAL.
        //
        // Antes isto era um laco que, nao achando vaga, simplesmente nao
        // punha o predio — e com ele sumia o NPC. Um passo da historia que
        // aponta pro Cartografo vira um passo impossivel, sem erro em lugar
        // nenhum ate' alguem jogar ate' la'. Foi o que aconteceu quando o
        // cais virou pra rota e o porto do Planalto mudou de costa.
        //
        // Agora a folga cede antes do NPC: tenta com respiro, depois
        // espremido, e por ultimo enfia na primeira vaga. Predio encostado e'
        // feio; ilha sem Cartografo e' quest quebrada.
        let mut posto = false;
        'achou: for folga in [1.0f32, 0.5, 0.0] {
            for k in 0..6 {
                let s = seed.wrapping_add((i as i32 + 1) * 1000 + k * 7919);
                let c = construcao::gerar(tipo, papel, s);
                for (recuo, afast) in VAGAS {
                    let pos = raiz - mar * recuo + lado * afast;
                    let q = quarto_para(raiz - mar * recuo - pos);
                    let meia = c.meia(q);
                    if chao.cabe(&lotes, pos, meia, folga) {
                        lotes.push((pos, meia));
                        vila.predios.push(Predio {
                            tipo,
                            papel,
                            seed: s,
                            pos: Vec3::new(pos.x, chao_y - B_CASA + RESPIRO, pos.y),
                            yaw_q: q,
                            chao: chao_y,
                        });
                        posto = true;
                        break 'achou;
                    }
                }
            }
        }
        if !posto {
            let s = seed.wrapping_add((i as i32 + 1) * 1000);
            let c = construcao::gerar(tipo, papel, s);
            let (recuo, afast) = VAGAS[0];
            let pos = raiz - mar * recuo + lado * afast;
            let q = quarto_para(raiz - mar * recuo - pos);
            lotes.push((pos, c.meia(q)));
            vila.predios.push(Predio {
                tipo,
                papel,
                seed: s,
                pos: Vec3::new(pos.x, chao_y - B_CASA + RESPIRO, pos.y),
                yaw_q: q,
                chao: chao_y,
            });
        }
    }

    // Carga do cais e a luz dele.
    let props = [
        (TipoProp::Caixas, raiz - mar * 4.0 - lado * 3.0),
        (TipoProp::Barril, raiz - mar * 5.5 - lado * 1.5),
        (TipoProp::Caixas, raiz - mar * 3.0 + lado * 3.5),
        (TipoProp::Barril, raiz - mar * 2.0 + lado * 2.6),
        (
            TipoProp::Lampiao,
            raiz - mar * 1.5 + lado * (p.larg * 0.5 + 0.8),
        ),
        (
            TipoProp::Lampiao,
            raiz - mar * 1.5 - lado * (p.larg * 0.5 + 0.8),
        ),
        (TipoProp::Lampiao, centro),
    ];
    for (k, (tipo, pos)) in props.into_iter().enumerate() {
        vila.props.push(PropPosto {
            tipo,
            seed: seed.wrapping_add(k as i32 * 31),
            pos: Vec3::new(pos.x, chao_y, pos.y),
            yaw_q: p.mar_q,
        });
    }

    let npcs: Vec<NpcDaVila> = vila.predios[primeiro..]
        .iter()
        .filter(|pr| pr.tipo != TipoCasa::Doca)
        .filter_map(npc_da_porta)
        .collect();
    vila.npcs.extend(npcs);
    // Capitão na entrada do cais, do lado de terra, voltado para a ilha.
    vila.npcs.push(NpcDaVila {
        papel: Papel::Estaleiro,
        nome: Papel::Estaleiro.nome(),
        pos: raiz - mar * 2.0 + lado * 0.9,
        yaw: yaw_de(-mar),
        loja: None,
        giver: None,
    });

    decorar_porto(ger, p, &chao, &lotes, comp, seed, primeiro, vila);

    vila.porto = Some(PortoDaVila {
        centro,
        raio: SitioPorto::RAIO,
        raiz,
        ponta: raiz + mar * comp,
        mar,
    });
}

// ─────────────────────────────── enfeites ────────────────────────────

/// Meia largura do caminho calcado.
pub const CAMINHO_MEIA: f32 = 0.9;
/// Raio da praca calcada.
pub const PRACA_RAIO: f32 = 6.8;

/// So' estes props barram passagem. Flor, cerca, varal, vaso e bandeirola
/// sao enfeite: travar o jogador num canteiro seria pior que o canteiro nao
/// existir.
pub fn prop_barra(t: TipoProp) -> bool {
    matches!(
        t,
        TipoProp::Poco
            | TipoProp::ArvoreOrnamental
            | TipoProp::Barraca
            | TipoProp::Carroca
            | TipoProp::Portal
            | TipoProp::OnibusVoador
            | TipoProp::FarolTormenta
            | TipoProp::RuinaTormenta
            | TipoProp::CristalTormenta
    )
}

fn dist_seg(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 < 1e-6 {
        0.0
    } else {
        ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
    };
    p.distance(a + ab * t)
}

/// A tinta do chao em `p` (coluna `bx, bz`), se `p` estiver numa praca,
/// num caminho ou num gramado da vila.
pub fn pintura_em(vila: &Vila, p: Vec2, bx: i32, bz: i32) -> Option<Material> {
    // Hash por LAJE de 1 u (2 colunas): pedra do tamanho de pedra, e o merge
    // do chao ainda junta as colunas de uma mesma laje.
    let (tx, tz) = (bx.div_euclid(2), bz.div_euclid(2));
    let h = ((tx.wrapping_mul(73_856_093) ^ tz.wrapping_mul(19_349_663)) as u32)
        .wrapping_mul(2_654_435_761)
        >> 28;
    for (c, raio) in &vila.pracas {
        let d = p.distance(*c);
        if d < *raio {
            if d > raio - 0.6 {
                return Some(Material::CalcadaEscura);
            }
            return Some(if (tx + tz) & 1 == 0 || h < 3 {
                Material::Calcada
            } else {
                Material::CalcadaEscura
            });
        }
    }
    for (a, b) in &vila.caminhos {
        let d = dist_seg(p, *a, *b);
        if d < CAMINHO_MEIA {
            if d > CAMINHO_MEIA - 0.35 {
                return Some(Material::CalcadaEscura);
            }
            return Some(if h < 5 {
                Material::Calcada
            } else {
                Material::Caminho
            });
        }
    }
    vila.gramados
        .iter()
        .any(|(c, raio)| p.distance(*c) < *raio)
        .then_some(Material::GramaCuidada)
}

/// Onde ja' tem coisa: lotes das casas, caminhos e pontos ocupados.
struct Ocupacao<'a> {
    chao: &'a Chao,
    lotes: &'a [(Vec2, Vec2)],
    caminhos: Vec<(Vec2, Vec2)>,
    pontos: Vec<(Vec2, f32)>,
}

impl Ocupacao<'_> {
    /// Cabe um enfeite de raio `r` em `p`? `lote` = o lote em que ele pode
    /// encostar (o vaso na porta da propria casa); `caminho` = respeitar os
    /// caminhos.
    fn livre(&self, p: Vec2, r: f32, lote: Option<usize>, caminho: bool) -> bool {
        if !self.chao.plano_em(p.x, p.y) {
            return false;
        }
        for (i, (c, m)) in self.lotes.iter().enumerate() {
            if Some(i) != lote && (p.x - c.x).abs() < m.x + r && (p.y - c.y).abs() < m.y + r {
                return false;
            }
        }
        if caminho
            && self
                .caminhos
                .iter()
                .any(|(a, b)| dist_seg(p, *a, *b) < CAMINHO_MEIA + r)
        {
            return false;
        }
        self.pontos.iter().all(|(q, qr)| p.distance(*q) >= qr + r)
    }

    fn ocupa(&mut self, p: Vec2, r: f32) {
        self.pontos.push((p, r));
    }
}

fn poe(vila: &mut Vila, tipo: TipoProp, seed: i32, p: Vec2, y: f32, yaw_q: u8) {
    vila.props.push(PropPosto {
        tipo,
        seed,
        pos: Vec3::new(p.x, y, p.y),
        yaw_q,
    });
}

/// O giro cujo eixo X LOCAL aponta pra `dir` (props compridos).
fn eixo_x_para(dir: Vec2) -> u8 {
    let mut melhor = (f32::MIN, 0u8);
    for q in 0..4u8 {
        let (x, z) = construcao::rot_q(1.0, 0.0, q);
        let d = x * dir.x + z * dir.y;
        if d > melhor.0 {
            melhor = (d, q);
        }
    }
    melhor.1
}

/// Vaso, lenha e barril na porta de uma casa (`lote` = o dela).
#[allow(clippy::too_many_arguments)]
/// De dentro de qual prédio este ponto está — e por onde se sai dele.
///
/// Devolve o ponto da rua em frente à porta.
///
/// Existe porque a grade do A* tem 4 unidades de lado (`BLOCO` × `PASSO_CAMINHO`)
/// e uma porta é mais estreita que isso: a grade **não consegue representar
/// uma porta**. Casa é obstáculo sólido, então quem está dentro não tem
/// vizinho livre e nenhuma rota existe — o personagem fica preso.
///
/// Isso não é hipótese: alguns teleportes largam o jogador dentro da casa do
/// NPC. O dono: "o A* não sabe sair disso nem contornar as casas".
///
/// A saída não é refinar a grade (multiplicaria por 64 o custo de toda rota
/// da ilha, por causa de uma dúzia de portas): é dar ao A* o ponto de saída
/// já pronto, e deixá-lo começar dali.
pub fn saida_do_predio(vila: &Vila, p: Vec2) -> Option<Vec2> {
    for pd in &vila.predios {
        let dentro = pd
            .construcao()
            .caixas_mundo(pd.pos, pd.yaw_q)
            .iter()
            .any(|(mn, mx)| p.x >= mn.x && p.x <= mx.x && p.y >= mn.z && p.y <= mx.z);
        if dentro {
            return frente_da_porta(pd);
        }
    }
    None
}

/// O ponto logo À FRENTE da porta de um prédio — por onde se entra.
///
/// `None` para prédio sem porta.
pub fn frente_da_porta(p: &Predio) -> Option<Vec2> {
    let pc = p.construcao();
    let lx = pc.porta_local()?;
    // Dois metros e meio pra fora da soleira: é a faixa por onde o caminho
    // passa, não a soleira em si.
    let w = pc.local_para_mundo(p.pos, p.yaw_q, Vec3::new(lx, 0.0, -2.5));
    Some(Vec2::new(w.x, w.z))
}

/// Este ponto atrapalha a entrada de alguma casa?
///
/// O `Ocupacao` sabe se um lugar está VAZIO, e não se ele está no CAMINHO —
/// são coisas diferentes, e a diferença apareceu num bot: a carroça caiu na
/// frente da loja de poções, o A* não contornava, e ninguém chegava no
/// alquimista. O dono: "o carrinho tá bonito, então quero manter ele, só
/// troca de lugar".
///
/// O raio é generoso de propósito. Prop encostado na porta é enfeite; prop a
/// dois metros dela é obstáculo, e quem paga a conta é o pathfinding.
pub fn atrapalha_porta(vila: &Vila, ponto: Vec2, raio: f32) -> bool {
    vila.predios
        .iter()
        .any(|p| frente_da_porta(p).is_some_and(|f| f.distance(ponto) < raio))
}

fn enfeitar_porta(
    oc: &mut Ocupacao,
    vila: &mut Vila,
    r: &mut Rng,
    prox: &mut dyn FnMut() -> i32,
    p: &Predio,
    lote: usize,
    y: f32,
) {
    let pc = p.construcao();
    let Some(lx) = pc.porta_local() else { return };
    let mundo = |x: f32, z: f32| {
        let w = pc.local_para_mundo(p.pos, p.yaw_q, Vec3::new(x, 0.0, z));
        Vec2::new(w.x, w.z)
    };
    let e = pc.escala;
    let (x_esq, z_fundo, piv) = (
        pc.v.x0 as f32 * e,
        (pc.v.z0 + pc.v.nz) as f32 * e,
        pc.pivo(),
    );
    for sx in [-1.8f32, 1.8] {
        let pv = mundo(lx + sx, -0.8);
        if oc.livre(pv, 0.3, Some(lote), true) {
            poe(vila, TipoProp::Vaso, prox(), pv, y, p.yaw_q);
            oc.ocupa(pv, 0.3);
        }
    }
    if (p.papel.loja() || p.tipo != TipoCasa::Casebre) && r.float() < 0.7 {
        let sx = if r.float() < 0.5 { 3.3 } else { -3.3 };
        let pb = mundo(lx + sx, -1.1);
        let tipo = if r.float() < 0.5 {
            TipoProp::Barril
        } else {
            TipoProp::Caixas
        };
        if oc.livre(pb, 0.6, Some(lote), true) {
            poe(vila, tipo, prox(), pb, y, p.yaw_q);
            oc.ocupa(pb, 0.6);
        }
    }
    if matches!(p.papel, Papel::Ferreiro | Papel::Casa | Papel::Deposito) {
        let pl = mundo(x_esq - 0.9, piv.y);
        if oc.livre(pl, 0.7, Some(lote), true) {
            poe(vila, TipoProp::Lenha, prox(), pl, y, (p.yaw_q + 1) % 4);
            oc.ocupa(pl, 0.7);
        }
    }
    if p.papel == Papel::Casa {
        // QUINTAL atras da casa: cerca, varal e canteiro.
        let x_dir = (pc.v.x0 + pc.v.nx) as f32 * e;
        let zc = z_fundo + 2.3;
        let mut x = x_esq;
        while x + 2.0 <= x_dir + 0.01 {
            let pp = mundo(x + 1.0, zc);
            if oc.livre(pp, 0.25, Some(lote), true) {
                poe(vila, TipoProp::Cerca, prox(), pp, y, p.yaw_q);
            }
            x += 2.0;
        }
        for xl in [x_esq - 0.1, x_dir + 0.1] {
            let pp = mundo(xl, z_fundo + 1.3);
            if oc.livre(pp, 0.25, Some(lote), true) {
                poe(vila, TipoProp::Cerca, prox(), pp, y, (p.yaw_q + 1) % 4);
            }
        }
        let pv = mundo(piv.x, z_fundo + 1.2);
        if oc.livre(pv, 0.4, Some(lote), true) {
            poe(vila, TipoProp::Varal, prox(), pv, y, p.yaw_q);
            oc.ocupa(pv, 0.4);
        }
        let pcan = mundo(x_esq + 1.3, z_fundo + 1.2);
        if oc.livre(pcan, 0.6, Some(lote), true) {
            poe(vila, TipoProp::Canteiro, prox(), pcan, y, p.yaw_q);
            oc.ocupa(pcan, 0.6);
        }
    }
}

/// Canteiro, moita e lampiao dos dois lados de cada caminho.
fn enfeitar_caminhos(
    oc: &mut Ocupacao,
    vila: &mut Vila,
    r: &mut Rng,
    prox: &mut dyn FnMut() -> i32,
    y: f32,
) {
    let caminhos = oc.caminhos.clone();
    for (i, (a, b)) in caminhos.iter().enumerate() {
        let len = a.distance(*b);
        if len < 3.0 {
            continue;
        }
        let dir = (*b - *a) / len;
        let lado = Vec2::new(-dir.y, dir.x);
        let (mut t, mut k) = (1.5f32, i);
        while t < len - 1.2 {
            for sinal in [1.0f32, -1.0] {
                let p = *a + dir * t + lado * sinal * 1.9;
                let (tipo, rr) = if k % 3 == 0 && sinal > 0.0 {
                    (TipoProp::Lampiao, 0.35)
                } else if r.float() < 0.7 {
                    (TipoProp::Canteiro, 0.8)
                } else {
                    (TipoProp::Arbusto, 0.75)
                };
                if oc.livre(p, rr, None, true) {
                    poe(vila, tipo, prox(), p, y, eixo_x_para(dir));
                    oc.ocupa(p, rr);
                }
            }
            t += 3.4;
            k += 1;
        }
    }
}

/// Arvores e moitas num anel, espacadas.
#[allow(clippy::too_many_arguments)]
fn plantar_anel(
    oc: &mut Ocupacao,
    vila: &mut Vila,
    prox: &mut dyn FnMut() -> i32,
    centro: Vec2,
    y: f32,
    ang0: f32,
    raios: &[f32],
    tipo: TipoProp,
    raio: f32,
    espaco: f32,
    max: usize,
) -> usize {
    use std::f32::consts::TAU;
    let mut postas: Vec<Vec2> = Vec::new();
    'fora: for &rr in raios {
        for k in 0..24 {
            if postas.len() >= max {
                break 'fora;
            }
            let a = ang0 + (k as f32 + rr * 0.37) * TAU / 24.0;
            let p = centro + Vec2::new(a.cos(), a.sin()) * rr;
            if postas.iter().all(|q| q.distance(p) >= espaco) && oc.livre(p, raio, None, true) {
                poe(vila, tipo, prox(), p, y, 0);
                oc.ocupa(p, raio);
                postas.push(p);
            }
        }
    }
    postas.len()
}

fn decorar_cidade(
    ger: &Gerador,
    c: &Cidade,
    chao: &Chao,
    lotes: &[(Vec2, Vec2)],
    ang0: f32,
    seed: i32,
    vila: &mut Vila,
) {
    use std::f32::consts::{PI, TAU};
    let centro = c.centro();
    let y = c.altura();
    let mut r = Rng::novo(seed ^ 0x0F10_2E5);
    let mut s = seed.wrapping_mul(31).wrapping_add(0x5EED);
    let mut prox = move || {
        s = s.wrapping_add(7919);
        s
    };
    vila.pracas.push((centro, PRACA_RAIO));
    vila.gramados.push((centro, c.raio_plato));

    // CAMINHOS da praca a cada porta, e a saida pro lado do porto.
    let mut caminhos = Vec::new();
    for p in vila.predios.iter().take(lotes.len()) {
        let pc = p.construcao();
        let Some(lx) = pc.porta_local() else { continue };
        let w = pc.local_para_mundo(p.pos, p.yaw_q, Vec3::new(lx, 0.0, -1.9));
        let porta = Vec2::new(w.x, w.z);
        let dir = (porta - centro).normalize_or_zero();
        caminhos.push((centro + dir * (PRACA_RAIO - 0.5), porta));
    }
    let mut saida = ger
        .porto()
        .map(|p| p.centro - centro)
        .unwrap_or(-centro)
        .normalize_or_zero();
    if saida == Vec2::ZERO {
        saida = Vec2::X;
    }
    caminhos.push((
        centro + saida * (PRACA_RAIO - 0.5),
        centro + saida * (c.raio_plato + 6.0),
    ));
    vila.caminhos.extend(caminhos.iter().copied());

    let mut oc = Ocupacao {
        chao,
        lotes,
        caminhos,
        pontos: Vec::new(),
    };
    for n in &vila.npcs {
        oc.ocupa(n.pos, 1.0);
    }
    for pr in &vila.props {
        oc.ocupa(Vec2::new(pr.pos.x, pr.pos.z), 0.8);
    }

    // BANDEIROLAS cruzando a praca.
    for q in 0..2u8 {
        let (ex, ez) = construcao::rot_q(1.0, 0.0, q);
        let eixo = Vec2::new(ex, ez);
        if [centro + eixo * 7.0, centro - eixo * 7.0]
            .iter()
            .all(|pp| oc.livre(*pp, 0.2, None, false))
        {
            poe(vila, TipoProp::Bandeirolas, prox(), centro, y, q);
        }
    }

    // Borda da praca: canteiros e dois bancos.
    for k in 0..8 {
        let a = ang0 + TAU / 16.0 + k as f32 * TAU / 8.0;
        let p = centro + Vec2::new(a.cos(), a.sin()) * 7.9;
        let tipo = if k % 4 == 1 {
            TipoProp::Banco
        } else {
            TipoProp::Canteiro
        };
        if oc.livre(p, 0.8, None, true) {
            let pc = centro - p;
            let q = if tipo == TipoProp::Banco {
                quarto_para(pc)
            } else {
                eixo_x_para(Vec2::new(-pc.y, pc.x))
            };
            poe(vila, tipo, prox(), p, y, q);
            oc.ocupa(p, 0.8);
        }
    }

    // Duas BARRACAS de feira, viradas pra praca.
    for da in [0.8f32, -0.8] {
        for rr in [10.5f32, 12.0, 13.5] {
            let a = ang0 + PI + da;
            let p = centro + Vec2::new(a.cos(), a.sin()) * rr;
            if oc.livre(p, 2.0, None, true) {
                poe(
                    vila,
                    TipoProp::Barraca,
                    prox(),
                    p,
                    y,
                    quarto_para(centro - p),
                );
                oc.ocupa(p, 2.0);
                break;
            }
        }
    }

    // QUADRO DE AVISOS ao lado do Mestre de Missoes.
    if let Some(m) = vila
        .npcs
        .iter()
        .find(|n| n.papel == Papel::Missoes)
        .map(|n| n.pos)
    {
        let radial = (m - centro).normalize_or_zero();
        let lado = Vec2::new(-radial.y, radial.x);
        for sinal in [1.0f32, -1.0] {
            let p = m + radial * 0.8 + lado * sinal * 2.0;
            if oc.livre(p, 0.4, None, true) {
                poe(
                    vila,
                    TipoProp::QuadroDeAvisos,
                    prox(),
                    p,
                    y,
                    quarto_para(centro - p),
                );
                oc.ocupa(p, 0.4);
                break;
            }
        }
    }

    // PORTAL na saida da cidade.
    let pp = centro + saida * (c.raio_plato - 1.5);
    let lado = Vec2::new(-saida.y, saida.x);
    let postes = [pp + lado * 1.8, pp - lado * 1.8];
    if postes.iter().all(|q| oc.livre(*q, 0.3, None, false)) {
        poe(vila, TipoProp::Portal, prox(), pp, y, eixo_x_para(lado));
        for q in postes {
            oc.ocupa(q, 0.4);
        }
    }

    enfeitar_caminhos(&mut oc, vila, &mut r, &mut prox, y);
    let casas: Vec<Predio> = vila.predios.iter().take(lotes.len()).cloned().collect();
    for (i, p) in casas.iter().enumerate() {
        enfeitar_porta(&mut oc, vila, &mut r, &mut prox, p, i, y);
    }

    // CARROCA num canto da praca, LONGE DAS PORTAS.
    //
    // Ela testava só se o ponto estava livre — e livre não é o mesmo que fora
    // do caminho. Caiu na frente da loja de poções, o A* não contornou, e o
    // primeiro NPC que vende poção ficou inalcançável. Achado por bot, que é
    // o tipo de coisa que teste de unidade não pega: cada peça estava certa.
    const LONGE_DA_PORTA: f32 = 5.0;
    'carroca: for rr in [9.5f32, 11.5, 14.0] {
        for k in 0..16 {
            let a = ang0 + PI * 0.5 + k as f32 * TAU / 16.0;
            let p = centro + Vec2::new(a.cos(), a.sin()) * rr;
            if atrapalha_porta(vila, p, LONGE_DA_PORTA) {
                continue;
            }
            if oc.livre(p, 2.1, None, true) {
                let t = p - centro;
                poe(
                    vila,
                    TipoProp::Carroca,
                    prox(),
                    p,
                    y,
                    eixo_x_para(Vec2::new(-t.y, t.x)),
                );
                oc.ocupa(p, 2.1);
                break 'carroca;
            }
        }
    }

    plantar_anel(
        &mut oc,
        vila,
        &mut prox,
        centro,
        y,
        ang0,
        &[13.5, 18.0, 22.5, 26.0],
        TipoProp::ArvoreOrnamental,
        1.6,
        6.5,
        10,
    );
    plantar_anel(
        &mut oc,
        vila,
        &mut prox,
        centro,
        y,
        ang0 + 0.4,
        &[10.5, 15.5, 20.0, 24.5],
        TipoProp::Arbusto,
        0.9,
        3.0,
        14,
    );
}

#[allow(clippy::too_many_arguments)]
fn decorar_porto(
    ger: &Gerador,
    p: &SitioPorto,
    chao: &Chao,
    lotes: &[(Vec2, Vec2)],
    comp: f32,
    seed: i32,
    primeiro: usize,
    vila: &mut Vila,
) {
    let (centro, y, mar, raiz) = (p.centro, p.altura(), p.mar(), p.raiz);
    let lado = Vec2::new(-mar.y, mar.x);
    let mut r = Rng::novo(seed ^ 0x0B01_A5);
    let mut s = seed.wrapping_mul(17).wrapping_add(0xB0A);
    let mut prox = move || {
        s = s.wrapping_add(7919);
        s
    };
    vila.caminhos.push((centro, raiz));
    vila.gramados.push((centro, SitioPorto::RAIO_PLATO));
    let mut oc = Ocupacao {
        chao,
        lotes,
        caminhos: vec![(centro, raiz)],
        pontos: Vec::new(),
    };
    for n in &vila.npcs {
        oc.ocupa(n.pos, 1.0);
    }
    for pr in &vila.props {
        oc.ocupa(Vec2::new(pr.pos.x, pr.pos.z), 0.8);
    }

    // REDES de pesca no patio, dos dois lados do cais.
    for sinal in [1.0f32, -1.0] {
        for recuo in [6.5f32, 9.0, 11.5] {
            let pr = raiz - mar * recuo + lado * sinal * (p.larg * 0.5 + 4.5);
            if oc.livre(pr, 1.4, None, true) {
                poe(vila, TipoProp::Rede, prox(), pr, y, eixo_x_para(mar));
                oc.ocupa(pr, 1.4);
                break;
            }
        }
    }
    // BOIAS no tabuado.
    for k in 0..3 {
        let t = (3.0 + k as f32 * 3.5).min(comp - 1.0);
        let sinal = if k % 2 == 0 { 1.0 } else { -1.0 };
        let pb = raiz + mar * t + lado * sinal * (p.larg * 0.5 - 0.4);
        poe(vila, TipoProp::Boia, prox(), pb, y + RESPIRO_DO_CAIS, 0);
    }
    // BARQUINHOS amarrados na agua ao lado do cais.
    for k in 0..2 {
        let sinal = if k == 0 { 1.0 } else { -1.0 };
        let pb = raiz + mar * (comp * 0.55 + k as f32 * 1.5) + lado * sinal * (p.larg * 0.5 + 1.8);
        let (bx, bz) = ((pb.x / BLOCO).round() as i32, (pb.y / BLOCO).round() as i32);
        if ((ger.bloco_em(bx, bz) + 1) as f32 * BLOCO) < NIVEL_DO_MAR - 0.3 {
            poe(
                vila,
                TipoProp::Barquinho,
                prox(),
                pb,
                NIVEL_DO_MAR - 0.12,
                eixo_x_para(mar),
            );
        }
    }

    let casas: Vec<Predio> = vila.predios[primeiro..]
        .iter()
        .filter(|pr| pr.tipo != TipoCasa::Doca)
        .cloned()
        .collect();
    for (j, pr) in casas.iter().enumerate() {
        enfeitar_porta(&mut oc, vila, &mut r, &mut prox, pr, j + 1, y);
    }
    enfeitar_caminhos(&mut oc, vila, &mut r, &mut prox, y);
    plantar_anel(
        &mut oc,
        vila,
        &mut prox,
        centro,
        y,
        0.3,
        &[12.0, 16.0, 20.0, 24.0],
        TipoProp::Arbusto,
        0.9,
        3.0,
        8,
    );
    plantar_anel(
        &mut oc,
        vila,
        &mut prox,
        centro,
        y,
        1.1,
        &[15.0, 21.0],
        TipoProp::ArvoreOrnamental,
        1.6,
        7.0,
        3,
    );
}

#[cfg(test)]
mod testes {
    use super::*;

    /// TODA ilha do arquipelago tem os NPCs do porto.
    ///
    /// O Cartografo e o Deposito moram no patio do porto, e antes eles eram
    /// postos por um laco que desistia calado quando nao achava vaga. O passo
    /// 766 da historia aponta pro Cartografo do Planalto — e no dia em que o
    /// cais virou pra rota, o porto mudou de costa, o predio nao coube e a
    /// quest virou impossivel. Sem erro, sem log: so' um NPC que nao existe.
    ///
    /// Este teste e' o que impede isso de voltar em silencio.
    /// Nenhum prop grande nasce na frente de uma porta.
    ///
    /// A carroça caiu na entrada da loja de poções e o A* não contornava: o
    /// alquimista, que é o primeiro NPC que vende poção, ficou inalcançável.
    /// Achado por bot jogando, não por teste — cada peça estava certa
    /// sozinha, e o defeito só existia na soma.
    ///
    /// O teste varre as vilas de várias sementes porque a posição é sorteada:
    /// conferir uma só provaria pouco.
    #[test]
    fn prop_grande_nao_nasce_na_frente_da_porta() {
        // AS ILHAS DE VERDADE, e não sementes inventadas.
        //
        // A primeira versão deste teste varria sementes aleatórias e passava
        // ATÉ SEM O CONSERTO — conferido tirando a guarda e rodando. Não
        // provava nada: o defeito é das ilhas que o jogo realmente usa, e uma
        // semente qualquer quase nunca põe a carroça na porta.
        for def in crate::terreno::ARQUIPELAGO.iter() {
            let ger = crate::terreno::Gerador::da_ilha(def);
            let vila = ger.vila();
            for pr in &vila.props {
                if !matches!(pr.tipo, TipoProp::Carroca) {
                    continue;
                }
                let ponto = Vec2::new(pr.pos.x, pr.pos.z);
                for pd in &vila.predios {
                    if let Some(f) = frente_da_porta(pd) {
                        let d = f.distance(ponto);
                        assert!(
                            d >= 5.0,
                            "{}: carroça a {d:.1} da porta de {:?} — \
                             é aí que o caminho passa",
                            def.zona,
                            pd.papel
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn todo_porto_tem_os_npcs_dele() {
        for d in crate::terreno::ARQUIPELAGO.iter() {
            let ger = Gerador::da_ilha(d);
            if ger.porto().is_none() {
                continue; // ilha sem costa que sirva nao promete porto
            }
            let vila = ger.vila();
            let porto = vila.porto.as_ref().unwrap();
            let capitao = vila.npcs.iter().find(|n| n.papel == Papel::Estaleiro).unwrap();
            assert!((capitao.pos - porto.raiz).dot(porto.mar) < 0.0,
                "{}: capitão precisa ficar na entrada do cais, dentro da ilha", d.zona);
            let papeis: Vec<Papel> = vila.npcs.iter().map(|n| n.papel).collect();
            for papel in [Papel::Estaleiro, Papel::Deposito, Papel::Cartografo] {
                assert!(
                    papeis.contains(&papel),
                    "{}: o porto ficou sem {papel:?}",
                    d.zona
                );
            }
        }
    }

    #[test]
    fn toda_cidade_tem_banqueira_perto_da_praca() {
        for d in crate::terreno::ARQUIPELAGO.iter() {
            let ger = Gerador::da_ilha(d);
            let Some(cidade) = ger.cidade() else { continue };
            assert!(ger.vila().npcs.iter().any(|n|
                n.nome == "Banker" && n.papel == Papel::Deposito
                    && n.pos.distance(cidade.centro()) < 8.0),
                "{} sem Banqueira na praca", d.zona);
        }
    }

    use crate::terreno::{
        Ilha, SeguidorDeRota, ARQUIPELAGO, DEGRAU_BLOCOS, ESCALA_ALTURA, PULO_BLOCOS,
    };
    use std::sync::OnceLock;

    const R: f32 = crate::constants::ENTITY_RADIUS;

    /// A ilha inicial no raio de teste (800 blocos): tem cidade e porto, e
    /// sai em segundos.
    fn ilha() -> &'static Ilha {
        static I: OnceLock<Ilha> = OnceLock::new();
        I.get_or_init(|| {
            let d = &ARQUIPELAGO[0];
            Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA)
        })
    }

    #[test]
    fn todos_os_npcs_da_vila_tem_acesso_pela_praca() {
        for d in &ARQUIPELAGO {
            let i = Ilha::gerar(d.semente, 800, d.bioma, ESCALA_ALTURA);
            let vila = i.vila();
            let centro = i.cidade().expect("ilha sem cidade").centro();
            let mut falhas = Vec::new();
            for npc in &vila.npcs {
                if centro.distance(npc.pos) > 70.0 {
                    continue;
                }
                let partida =
                    i.ponto_livre_perto(centro + (npc.pos - centro).normalize_or_zero() * 5.5, R);
                let destino = i.ponto_livre_perto(npc.pos, R);
                let rota = i.caminho(partida, destino, 60_000);
                let fim = simula_rota(&i, partida, destino);
                if rota.is_none() || fim.distance(npc.pos) >= 2.5 {
                    falhas.push(format!("{:?}: {:.1}", npc.papel, fim.distance(npc.pos)));
                }
            }
            assert!(
                falhas.is_empty(),
                "{}: NPCs inacessíveis: {}",
                d.zona,
                falhas.join(", ")
            );
        }
    }

    fn anda_ate(i: &Ilha, de: Vec2, para: Vec2, ticks: u32) -> Vec2 {
        let mut p = de;
        for _ in 0..ticks {
            if p.distance(para) < 0.05 {
                break;
            }
            let dir = (para - p).normalize_or_zero();
            p = i.mover_e_deslizar(p, dir * 3.0, 1.0 / 30.0, R);
        }
        p
    }

    /// A rota como o servidor anda: A*, pulo automatico e rota refeita.
    fn simula_rota(i: &Ilha, de: Vec2, para: Vec2) -> Vec2 {
        let dt = 1.0 / 30.0;
        let vel = 5.0;
        let Some(rota) = i.caminho(de, para, 60_000) else {
            return de;
        };
        let mut seg = SeguidorDeRota::nova(rota, para);
        let mut p = de;
        let (mut pulo_ate, mut pronto, mut agora, mut refeitas, mut ultima) =
            (-1.0f32, 0.0f32, 0.0f32, 0, -1.0f32);
        for _ in 0..30_000 {
            let Some(dir) = seg.direcao(p) else { break };
            if seg.travado() && agora - ultima >= 0.2 {
                ultima = agora;
                refeitas += 1;
                if refeitas > 30 {
                    break;
                }
                match i.caminho(p, para, 60_000) {
                    Some(r) => seg = SeguidorDeRota::nova(r, para),
                    None => break,
                }
                continue;
            }
            if i.precisa_pular(p, dir * vel, dt, R) && agora >= pronto {
                pulo_ate = agora + crate::constants::PULO_DURACAO;
                pronto = pulo_ate + crate::constants::PULO_ESPERA;
            }
            let degrau = if agora < pulo_ate {
                PULO_BLOCOS
            } else {
                DEGRAU_BLOCOS
            };
            p = i.mover_com_degrau(p, dir * vel, dt, R, degrau);
            agora += dt;
        }
        p
    }

    /// EVERY island has a Harbour Captain, reachable on foot from its town
    /// square: travel only starts next to him. Skyreach has no port, and
    /// without this whoever sailed up could not leave.
    #[test]
    fn toda_ilha_tem_capitao_alcancavel() {
        for d in ARQUIPELAGO.iter() {
            let ilha = crate::terreno::Ilha::da_ilha(d);
            let cap = ilha
                .vila()
                .npcs
                .iter()
                .find(|n| n.papel == Papel::Estaleiro || (crate::kogen::e_kogen(d.zona) && n.papel == Papel::Motorista))
                .unwrap_or_else(|| panic!("{}: no Harbour Captain", d.zona));
            let praca = ilha.cidade().expect("a town").centro() + Vec2::new(0.0, 4.0);
            let rota = ilha.caminho(praca, cap.pos, 40_000);
            assert!(
                rota.as_ref().and_then(|r| r.last()).is_some_and(|f| f.distance(cap.pos) < crate::viagem::PERTO_DO_CAPITAO),
                "{}: the Captain at {:?} cannot be reached from the square",
                d.zona,
                cap.pos
            );
        }
    }

    /// The Sky Bus Professor stands where a player walks to him from the
    /// square, in Skyreach and in Kōgen-tō.
    #[test]
    fn o_professor_do_onibus_se_alcanca() {
        for d in [crate::celeste::DEF, crate::kogen::DEF] {
            let ilha = crate::terreno::Ilha::da_ilha(&d);
            let prof = ilha.vila().npcs.iter().find(|n| n.papel == Papel::Motorista).cloned()
                .unwrap_or_else(|| panic!("{}: no Sky Bus Professor", d.zona));
            let praca = if crate::celeste::e_celeste(d.zona) {
                crate::celeste::PLATOS[11].centro
            } else {
                ilha.cidade().unwrap().centro() + Vec2::new(0.0, 4.0)
            };
            let fim = ilha.caminho(praca, prof.pos, 40_000).and_then(|r| r.last().copied());
            assert!(fim.is_some_and(|f| f.distance(prof.pos) < crate::viagem::PERTO_DO_CAPITAO),
                "{}: the professor at {:?} cannot be reached", d.zona, prof.pos);
        }
    }

    #[test]
    fn numeros_das_quatro_ilhas() {
        // The sea islands: Skyreach has a town but no port.
        for d in ARQUIPELAGO.iter().filter(|d| crate::terreno::tem_porto(d.zona)) {
            let ger = Gerador::da_ilha(d);
            let vila = ger.vila();
            let c = ger.cidade().expect("sem cidade");
            let na_cidade = vila
                .predios
                .iter()
                .filter(|p| c.distancia(Vec2::new(p.pos.x, p.pos.z)) < 60.0)
                .count();
            let porto = vila.porto.expect("sem porto");
            println!(
                "{}: cidade {:?} com {} predios ({} NPCs no total) | porto {:?} a {:.0} da cidade, cais {:.1} de comprimento",
                d.zona, c.centro(), na_cidade, vila.npcs.len(), porto.centro,
                porto.centro.distance(c.centro()), porto.raiz.distance(porto.ponta)
            );
            let mut contagem: std::collections::BTreeMap<String, usize> = Default::default();
            for pr in &vila.props {
                *contagem.entry(format!("{:?}", pr.tipo)).or_default() += 1;
            }
            println!(
                "  {} props {contagem:?} | {} caminhos",
                vila.props.len(),
                vila.caminhos.len()
            );
            assert!(
                na_cidade >= 7,
                "{}: so' {na_cidade} predios na cidade",
                d.zona
            );
            assert!(
                contagem.get("Canteiro").copied().unwrap_or(0) >= 6,
                "{}: poucos canteiros",
                d.zona
            );
            assert!(
                vila.npcs.iter().any(|n| n.loja == Some(LOJA_DE_POCOES)),
                "{}: sem Alquimista",
                d.zona
            );
            assert!(porto.centro.distance(c.centro()) >= 150.0);
        }
    }

    #[test]
    fn vila_e_deterministica() {
        let d = &ARQUIPELAGO[1];
        let a = Gerador::da_ilha(d);
        let b = Gerador::da_ilha(d);
        assert_eq!(a.vila(), b.vila());
    }

    #[test]
    fn casas_no_plato_e_nunca_na_agua() {
        let i = ilha();
        let vila = i.vila();
        let niveis: Vec<i32> = [i.cidade().map(|c| c.nivel), i.porto().map(|p| p.nivel)]
            .into_iter()
            .flatten()
            .collect();
        for p in vila.predios.iter().filter(|p| p.tipo != TipoCasa::Doca) {
            let c = p.construcao();
            let meia = c.meia(p.yaw_q);
            let mut x = p.pos.x - meia.x;
            while x <= p.pos.x + meia.x {
                let mut z = p.pos.z - meia.y;
                while z <= p.pos.z + meia.y {
                    assert!(!i.agua(x, z), "{:?} sobre a agua", p.papel);
                    let (ix, iz) = i.coluna(x, z);
                    // Cabana de posto fica fora da cidade, no chao plano dela.
                    let posto = p.tipo == TipoCasa::Cabana && p.papel == Papel::Missoes;
                    let proprio = (p.chao / BLOCO).round() as i32 - 1;
                    assert!(
                        niveis.contains(&i.bloco(ix, iz)) || (posto && i.bloco(ix, iz) == proprio),
                        "{:?} fora do plato",
                        p.papel
                    );
                    z += BLOCO;
                }
                x += BLOCO;
            }
        }
    }

    #[test]
    fn entra_pela_porta_de_cada_casa() {
        let i = ilha();
        let mut testadas = 0;
        for p in i.vila().predios.iter().filter(|p| p.tipo != TipoCasa::Doca) {
            let c = p.construcao();
            let lx = c.porta_local().unwrap();
            let fora = c.local_para_mundo(p.pos, p.yaw_q, Vec3::new(lx, 0.0, -1.6));
            let dentro = c.local_para_mundo(p.pos, p.yaw_q, Vec3::new(lx, 0.0, 1.6));
            let (a, b) = (Vec2::new(fora.x, fora.z), Vec2::new(dentro.x, dentro.z));
            let fim = anda_ate(i, a, b, 150);
            assert!(
                fim.distance(b) < 0.3,
                "{:?}/{:?}: parou a {:.2} de dentro",
                p.tipo,
                p.papel,
                fim.distance(b)
            );
            testadas += 1;
        }
        assert!(testadas >= 7);
    }

    #[test]
    fn nao_atravessa_parede() {
        let i = ilha();
        let mut testadas = 0;
        for p in i.vila().predios.iter().filter(|p| p.tipo != TipoCasa::Doca) {
            let c = p.construcao();
            let piv = c.pivo();
            let fundo = (c.v.z0 + c.v.nz) as f32 * c.escala;
            let atras = c.local_para_mundo(p.pos, p.yaw_q, Vec3::new(piv.x, 0.0, fundo + 1.5));
            let a = Vec2::new(atras.x, atras.z);
            if !i.sem_estorvo(a, R) || i.agua(a.x, a.y) {
                continue;
            }
            let b = Vec2::new(p.pos.x, p.pos.z);
            let fim = anda_ate(i, a, b, 200);
            let parede = (c.v.nz - 2) as f32 * c.escala * 0.5 - c.escala;
            assert!(
                fim.distance(b) > parede,
                "{:?}: atravessou a parede do fundo",
                p.papel
            );
            testadas += 1;
        }
        assert!(testadas >= 3, "so' {testadas} fundos testados");
    }

    #[test]
    fn npc_nao_fica_dentro_de_parede() {
        let i = ilha();
        for n in &i.vila().npcs {
            assert!(
                !i.caixa_toca(n.pos, 0.3),
                "{} dentro de parede em {:?}",
                n.nome,
                n.pos
            );
        }
    }

    /// Um Mestre de Missoes por ilha, na praca, em chao livre e andavel ate'
    /// ele a partir do centro.
    #[test]
    fn mestre_de_missoes_na_praca() {
        let i = ilha();
        let c = i.cidade().unwrap();
        let mestres: Vec<_> = i
            .vila()
            .npcs
            .iter()
            .filter(|n| n.papel == Papel::Missoes && n.giver.is_none())
            .collect();
        assert_eq!(mestres.len(), 1, "mestres de missoes: {}", mestres.len());
        let m = mestres[0];
        assert!(
            c.distancia(m.pos) < 6.0,
            "mestre longe da praca: {:.1}",
            c.distancia(m.pos)
        );
        assert!(
            i.sem_estorvo(m.pos, 0.3),
            "mestre dentro de estorvo em {:?}",
            m.pos
        );
        assert!(!i.agua(m.pos.x, m.pos.y));
        // Da' pra chegar nele andando pela praca — pelo LADO, tangente ao
        // poco: a reta vinda do outro lado da praca cruzaria o poco, que barra.
        let radial = (m.pos - c.centro()).normalize();
        let lado = Vec2::new(-radial.y, radial.x);
        let de = i.ponto_livre_perto(m.pos + lado * 4.0, R);
        let perto = m.pos + (de - m.pos).normalize() * 1.5;
        let fim = anda_ate(i, de, perto, 300);
        assert!(
            fim.distance(perto) < 1.0,
            "nao chega no mestre: parou a {:.2}",
            fim.distance(perto)
        );
        // Olha pro poco.
        assert!((m.yaw - yaw_de(c.centro() - m.pos)).abs() < 1e-4);
    }

    #[test]
    fn o_cais_e_andavel_ate_a_ponta() {
        let i = ilha();
        let porto = i.vila().porto.unwrap();
        let alvo = porto.ponta - porto.mar * 1.0;
        let fim = anda_ate(i, porto.centro, alvo, 900);
        assert!(
            fim.distance(alvo) < 1.0,
            "parou a {:.1} da ponta",
            fim.distance(alvo)
        );
    }

    #[test]
    fn da_pra_ir_da_praca_ao_porto() {
        let i = ilha();
        let c = i.cidade().unwrap();
        let porto = i.vila().porto.unwrap();
        let de = i.ponto_livre_perto(c.centro() + Vec2::new(9.0, 0.0), R);
        let fim = simula_rota(i, de, porto.centro);
        println!(
            "praca -> porto: {:.0} em linha reta, parou a {:.1}",
            de.distance(porto.centro),
            fim.distance(porto.centro)
        );
        assert!(
            fim.distance(porto.centro) < 6.0,
            "parou a {:.1} do porto",
            fim.distance(porto.centro)
        );
    }

    /// Com todos os enfeites no lugar, da' pra ir da praca a cada porta e ao
    /// Mestre de Missoes pela rota do servidor.
    #[test]
    fn enfeite_nao_tranca_porta_nem_o_mestre() {
        let i = ilha();
        let c = i.cidade().unwrap();
        let vila = i.vila();
        let mut n = 0;
        for p in vila
            .predios
            .iter()
            .filter(|p| p.tipo != TipoCasa::Doca && c.distancia(Vec2::new(p.pos.x, p.pos.z)) < 70.0)
        {
            let pc = p.construcao();
            let lx = pc.porta_local().unwrap();
            let w = pc.local_para_mundo(p.pos, p.yaw_q, Vec3::new(lx, 0.0, -1.6));
            let porta = Vec2::new(w.x, w.z);
            let de = i.ponto_livre_perto(c.centro() + (porta - c.centro()).normalize() * 7.0, R);
            let fim = simula_rota(i, de, porta);
            assert!(
                fim.distance(porta) < 1.0,
                "{:?}: parou a {:.2} da porta",
                p.papel,
                fim.distance(porta)
            );
            n += 1;
        }
        assert!(n >= 7, "so' {n} portas");
        let m = vila
            .npcs
            .iter()
            .find(|n| n.papel == Papel::Missoes)
            .unwrap()
            .pos;
        let de = i.ponto_livre_perto(c.centro() - (m - c.centro()).normalize() * 8.0, R);
        let fim = simula_rota(i, de, m);
        assert!(
            fim.distance(m) < 1.6,
            "nao chega no mestre: parou a {:.2}",
            fim.distance(m)
        );
    }

    /// O poco da praca: a rota de um lado ate' o outro contorna ele, e o
    /// corpo chega. (Relato de 19/09/2026: "o poco nao ta' sendo
    /// considerado pro A*".)
    #[test]
    fn a_rota_contorna_o_poco_da_praca() {
        let i = ilha();
        let vila = i.vila();
        let poco = vila
            .props
            .iter()
            .find(|pr| pr.tipo == TipoProp::Poco)
            .expect("a vila tem poco");
        let caixas: Vec<(Vec2, Vec2)> = poco
            .construcao()
            .caixas_mundo(poco.pos, poco.yaw_q)
            .into_iter()
            .filter(|(mn, mx)| {
                mn.y - poco.pos.y < TETO_DA_COLISAO && mx.y - poco.pos.y > PISO_DA_COLISAO
            })
            .map(|(mn, mx)| (Vec2::new(mn.x, mn.z), Vec2::new(mx.x, mx.z)))
            .collect();
        assert!(!caixas.is_empty(), "o poco nao tem caixa de colisao");
        let c = Vec2::new(poco.pos.x, poco.pos.z);
        let raio_poco = caixas
            .iter()
            .map(|(mn, mx)| mn.distance(c).max(mx.distance(c)))
            .fold(0.0f32, f32::max);
        let corta = |a: Vec2, b: Vec2| {
            let n = ((a.distance(b) / 0.05).ceil() as i32).max(1);
            (0..=n).any(|k| {
                let q = a.lerp(b, k as f32 / n as f32);
                caixas.iter().any(|(mn, mx)| {
                    let perto = Vec2::new(q.x.clamp(mn.x, mx.x), q.y.clamp(mn.y, mx.y));
                    perto.distance(q) < R * 0.9
                })
            })
        };
        let mut falhas = Vec::new();
        for k in 0..8 {
            let ang = k as f32 * std::f32::consts::TAU / 8.0;
            let dir = Vec2::new(ang.cos(), ang.sin());
            let de = i.ponto_livre_perto(c + dir * (raio_poco + 2.5), R);
            let para = i.ponto_livre_perto(c - dir * (raio_poco + 2.5), R);
            let Some(rota) = i.caminho(de, para, 60_000) else {
                falhas.push(format!("{k}: sem rota"));
                continue;
            };
            let mut a = de;
            for &b in &rota {
                if corta(a, b) {
                    falhas.push(format!("{k}: trecho {a:?} -> {b:?} atravessa o poco"));
                    break;
                }
                a = b;
            }
            let fim = simula_rota(i, de, para);
            if fim.distance(para) > 1.0 {
                falhas.push(format!(
                    "{k}: parou a {:.2} do destino; rota termina em {:?}, destino {:?}, ultimo trecho livre {}",
                    fim.distance(para),
                    rota.last(),
                    para,
                    rota.len() >= 2 && i.trecho_livre_publico(rota[rota.len() - 2], para)
                ));
            }
        }
        assert!(
            falhas.is_empty(),
            "poco em {c:?} (raio {raio_poco:.2}):\n{}",
            falhas.join("\n")
        );
    }

    /// Enfeite que BARRA (arvore, barraca, carroca, portal) nunca fica em
    /// cima de caminho calcado.
    #[test]
    fn nada_que_barra_fica_no_caminho() {
        let i = ilha();
        let vila = i.vila();
        let mut caixas = Vec::new();
        for pr in vila
            .props
            .iter()
            .filter(|pr| prop_barra(pr.tipo) && pr.tipo != TipoProp::Poco)
        {
            for (mn, mx) in pr.construcao().caixas_mundo(pr.pos, pr.yaw_q) {
                if mn.y - pr.pos.y < TETO_DA_COLISAO && mx.y - pr.pos.y > PISO_DA_COLISAO {
                    caixas.push((Vec2::new(mn.x, mn.z), Vec2::new(mx.x, mx.z)));
                }
            }
        }
        assert!(!caixas.is_empty(), "nenhum enfeite que barra");
        for (a, b) in &vila.caminhos {
            let n = ((a.distance(*b) / 0.25).ceil() as i32).max(1);
            for k in 0..=n {
                let q = a.lerp(*b, k as f32 / n as f32);
                for (mn, mx) in &caixas {
                    let perto = Vec2::new(q.x.clamp(mn.x, mx.x), q.y.clamp(mn.y, mx.y));
                    assert!(
                        perto.distance(q) > R,
                        "enfeite que barra em cima do caminho em {q:?}"
                    );
                }
            }
        }
    }

    /// A praca e os caminhos saem pintados; longe da vila, nada.
    #[test]
    fn chao_pintado_so_na_vila() {
        let d = &ARQUIPELAGO[0];
        let ger = Gerador::da_ilha(d);
        let c = ger.cidade().unwrap().centro();
        let col = |p: Vec2| ((p.x / BLOCO).round() as i32, (p.y / BLOCO).round() as i32);
        let (bx, bz) = col(c + Vec2::new(1.0, 1.0));
        assert!(matches!(
            ger.pintura_do_chao(bx, bz),
            Some(Material::Calcada | Material::CalcadaEscura)
        ));
        let (a, b) = ger.vila().caminhos[0];
        let (bx, bz) = col(a.lerp(b, 0.5));
        assert!(matches!(
            ger.pintura_do_chao(bx, bz),
            Some(Material::Calcada | Material::CalcadaEscura | Material::Caminho)
        ));
        let (bx, bz) = col(c + Vec2::new(200.0, 0.0));
        assert_eq!(ger.pintura_do_chao(bx, bz), None);
    }
}
