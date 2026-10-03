//! CONSTRUCOES em voxel: casa, armazem, doca, cabana e a mobilia de rua.
//!
//! Port do zone14 (`Construcao.cs` e `Voxels.cs`). As receitas sao as de
//! la', sorteio por sorteio na mesma ordem — quem ja' conhece uma vila do
//! zone14 reconhece a daqui. O que foi ACRESCENTADO (soleira, floreira,
//! diagonal do enxaimel, lampiao na porta da loja, piso do sobrado) sai de um
//! sorteio separado, depois do original, pra nao mexer no que ja' existia.
//!
//! Contrato de tudo aqui: semente -> volume deterministico. Nada viaja pela
//! rede — o servidor tira a colisao do volume, o cliente tira a malha, e os
//! dois chegam no mesmo desenho porque rodam a mesma funcao.
//!
//! ── EIXOS DO VOLUME ──
//! A celula `(ix, iy, iz)` ocupa `[ix*escala, (ix+1)*escala]` em cada eixo.
//! `y` sobe. A FACHADA da porta e' `z = 0` e olha pra `-z`. `y = 0` e' o
//! alicerce; o piso interno fica no topo dele.

use glam::{Vec2, Vec3};

/// Voxel de predio: o bloco do terreno.
pub const B_CASA: f32 = 0.5;
/// Voxel da mobilia de rua: um quarto do bloco.
pub const B_PROP: f32 = 0.125;
/// Fileira do tabuado da doca. A superficie pisada fica `(DECK_Y + 1) * B`
/// acima da origem do volume.
pub const DECK_Y: i32 = 3;

// ─────────────────────────────── sorteio ──────────────────────────────

/// Xorshift de 32 bits com o mesmo aquecimento do zone14.
#[derive(Debug, Clone, Copy)]
pub struct Rng {
    s: u32,
}

impl Rng {
    pub fn novo(seed: i32) -> Self {
        let mut r = Self {
            s: (seed as u32).wrapping_mul(2_654_435_761) | 1,
        };
        // Aquecimento: o primeiro valor herdaria a cara da semente.
        for _ in 0..4 {
            r.next();
        }
        r
    }

    pub fn next(&mut self) -> u32 {
        let mut s = self.s;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.s = s;
        s
    }

    /// 0..1.
    pub fn float(&mut self) -> f32 {
        (self.next() & 0xFF_FFFF) as f32 / 16_777_216.0
    }

    /// Inteiro em `[lo, hi)`.
    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo) as u32) as i32
    }
}

/// `seed * a + b` com o estouro do C#.
fn semente(seed: i32, a: i32, b: i32) -> i32 {
    seed.wrapping_mul(a).wrapping_add(b)
}

// ───────────────────────────── materiais ─────────────────────────────

/// Materiais de construcao. A ordem e' a do zone14 (o byte importa); os
/// novos entram no fim.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlocoCasa {
    Ar = 0,
    PedraBase,
    Reboco,
    Tabua,
    Viga,
    Telha,
    Ardosia,
    Assoalho,
    Janela,
    Lume,
    Metal,
    Pano,
    PanoRubro,
    Papel,
    Toldo,
    ToldoAzul,
    ToldoVerde,
    ToldoAmbar,
    ToldoRoxo,
    PedraNegra,
    NegroBorda,
    LumeVerde,
    ToldoPergaminho,
    // ── do Tempest ──
    Terra,
    Flor,
    FlorAmarela,
    Folha,
    // ── cor da cidade ──
    RebocoOcre,
    RebocoRosa,
    RebocoAzul,
    RebocoVerde,
    TelhaEscura,
    Palha,
    PinturaVermelha,
    PinturaAzul,
    PinturaVerde,
    PinturaAmarela,
    FlorRoxa,
    FlorBranca,
    FlorLaranja,
    FolhaClara,
    Fruta,
    Peixe,
    Corda,
    Lenha,
    // ── Skyreach (`celeste`), at the END: the discriminant is stored ──
    MarmoreCasa,
    MarmoreBase,
    OuroCasa,
    TelhaCeleste,
}

const TODOS_OS_BLOCOS: [BlocoCasa; 49] = [
    BlocoCasa::Ar,
    BlocoCasa::PedraBase,
    BlocoCasa::Reboco,
    BlocoCasa::Tabua,
    BlocoCasa::Viga,
    BlocoCasa::Telha,
    BlocoCasa::Ardosia,
    BlocoCasa::Assoalho,
    BlocoCasa::Janela,
    BlocoCasa::Lume,
    BlocoCasa::Metal,
    BlocoCasa::Pano,
    BlocoCasa::PanoRubro,
    BlocoCasa::Papel,
    BlocoCasa::Toldo,
    BlocoCasa::ToldoAzul,
    BlocoCasa::ToldoVerde,
    BlocoCasa::ToldoAmbar,
    BlocoCasa::ToldoRoxo,
    BlocoCasa::PedraNegra,
    BlocoCasa::NegroBorda,
    BlocoCasa::LumeVerde,
    BlocoCasa::ToldoPergaminho,
    BlocoCasa::Terra,
    BlocoCasa::Flor,
    BlocoCasa::FlorAmarela,
    BlocoCasa::Folha,
    BlocoCasa::RebocoOcre,
    BlocoCasa::RebocoRosa,
    BlocoCasa::RebocoAzul,
    BlocoCasa::RebocoVerde,
    BlocoCasa::TelhaEscura,
    BlocoCasa::Palha,
    BlocoCasa::PinturaVermelha,
    BlocoCasa::PinturaAzul,
    BlocoCasa::PinturaVerde,
    BlocoCasa::PinturaAmarela,
    BlocoCasa::FlorRoxa,
    BlocoCasa::FlorBranca,
    BlocoCasa::FlorLaranja,
    BlocoCasa::FolhaClara,
    BlocoCasa::Fruta,
    BlocoCasa::Peixe,
    BlocoCasa::Corda,
    BlocoCasa::Lenha,
    BlocoCasa::MarmoreCasa,
    BlocoCasa::MarmoreBase,
    BlocoCasa::OuroCasa,
    BlocoCasa::TelhaCeleste,
];

impl BlocoCasa {
    pub fn de_u8(b: u8) -> Option<Self> {
        TODOS_OS_BLOCOS.get(b as usize).copied()
    }

    /// Cor do material (paleta do zone14, `ClientGame.CorDaCasa`).
    pub fn rgb(self) -> [u8; 3] {
        use BlocoCasa::*;
        match self {
            Ar => [0, 0, 0],
            PedraBase => [126, 122, 114],
            Reboco => [233, 223, 202],
            Tabua => [152, 109, 65],
            Viga => [76, 55, 36],
            Telha => [178, 84, 58],
            Ardosia => [88, 94, 106],
            Assoalho => [170, 134, 89],
            Janela => [170, 216, 232],
            Lume => [255, 218, 130],
            Metal => [96, 100, 112],
            Pano => [214, 205, 184],
            PanoRubro => [168, 54, 48],
            Papel => [238, 232, 214],
            Toldo => [188, 60, 48],
            ToldoAzul => [58, 110, 180],
            ToldoVerde => [72, 148, 70],
            ToldoAmbar => [224, 168, 58],
            ToldoRoxo => [142, 92, 190],
            PedraNegra => [62, 58, 72],
            NegroBorda => [104, 97, 118],
            LumeVerde => [122, 244, 158],
            ToldoPergaminho => [232, 214, 168],
            Terra => [96, 70, 46],
            Flor => [214, 72, 96],
            FlorAmarela => [236, 196, 72],
            Folha => [74, 130, 58],
            RebocoOcre => [226, 196, 138],
            RebocoRosa => [232, 188, 176],
            RebocoAzul => [188, 208, 228],
            RebocoVerde => [196, 222, 184],
            TelhaEscura => [124, 62, 48],
            Palha => [212, 182, 106],
            PinturaVermelha => [178, 52, 46],
            PinturaAzul => [52, 98, 164],
            PinturaVerde => [58, 130, 78],
            PinturaAmarela => [228, 178, 52],
            FlorRoxa => [152, 96, 212],
            FlorBranca => [244, 244, 250],
            FlorLaranja => [242, 138, 56],
            FolhaClara => [112, 172, 76],
            Fruta => [232, 90, 46],
            Peixe => [176, 192, 204],
            Corda => [198, 172, 122],
            Lenha => [140, 98, 58],
            MarmoreCasa => [244, 240, 230],
            MarmoreBase => [214, 208, 196],
            OuroCasa => [232, 190, 82],
            TelhaCeleste => [118, 166, 226],
        }
    }

    /// The same block in Skyreach's angelic dress: white marble walls, gold
    /// where the timber was, azure roofs, gold and blue paint. Only the
    /// colour changes — solidity, doors and collision stay the same.
    pub fn celeste(self) -> Self {
        use BlocoCasa::*;
        match self {
            Reboco | RebocoOcre | RebocoRosa | RebocoAzul | RebocoVerde | Papel | Pano => MarmoreCasa,
            PedraBase | PedraNegra | NegroBorda => MarmoreBase,
            // Gold only on the frame beams; boards become marble, or whole
            // walls turned gold.
            Viga | Corda => OuroCasa,
            Tabua | Lenha => MarmoreBase,
            Telha | TelhaEscura | Palha | Ardosia => TelhaCeleste,
            PinturaVermelha | PinturaVerde | PinturaAmarela | PanoRubro => OuroCasa,
            PinturaAzul | Toldo | ToldoVerde | ToldoRoxo => ToldoAzul,
            ToldoAmbar | ToldoPergaminho => OuroCasa,
            Flor | FlorRoxa | FlorLaranja => FlorBranca,
            outro => outro,
        }
    }

    /// Chama: desenha sem sombreamento.
    pub fn brilha(self) -> bool {
        matches!(self, BlocoCasa::Lume | BlocoCasa::LumeVerde)
    }
}

// ─────────────────────────────── tipos ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoCasa {
    Casebre,
    Armazem,
    Doca,
    Cabana,
}

/// O OFICIO de um predio. Ordem do zone14; `Alquimista` e' do Tempest.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Papel {
    Casa,
    Ferreiro,
    Armas,
    Armaduras,
    Itens,
    Identificador,
    Treinador,
    Cartografo,
    Taberna,
    Deposito,
    Alfaiate,
    Estaleiro,
    Mercador,
    Naufrago,
    Alquimista,
    /// O Mestre de Missoes: fica na praca, e nao numa porta.
    Missoes,
    /// Kōgen-tō's flying-bus driver (the professor): in Skyreach's square
    /// he is the only way to Kōgen-tō; in Kōgen-tō's he flies anywhere.
    /// Last, so every other role keeps its number on the wire.
    Motorista,
}

impl Papel {
    /// A lona da cor do oficio. `Ar` = sem toldo.
    pub fn toldo(self) -> BlocoCasa {
        use Papel::*;
        match self {
            Ferreiro | Armas => BlocoCasa::Toldo,
            Armaduras => BlocoCasa::ToldoAzul,
            Itens | Alquimista => BlocoCasa::ToldoVerde,
            Identificador | Taberna | Alfaiate => BlocoCasa::ToldoRoxo,
            Treinador => BlocoCasa::ToldoAmbar,
            Cartografo => BlocoCasa::ToldoPergaminho,
            _ => BlocoCasa::Ar,
        }
    }

    /// O emblema pintado na placa. `Ar` = sem placa.
    pub fn emblema(self) -> BlocoCasa {
        use Papel::*;
        match self {
            Ferreiro | Armas | Armaduras => BlocoCasa::Metal,
            Itens => BlocoCasa::ToldoVerde,
            Identificador => BlocoCasa::Janela,
            Treinador => BlocoCasa::ToldoAmbar,
            Cartografo => BlocoCasa::Papel,
            Taberna => BlocoCasa::PanoRubro,
            Alfaiate => BlocoCasa::Pano,
            // O frasco que brilha: de perto, e' a loja de pocao.
            Alquimista => BlocoCasa::LumeVerde,
            _ => BlocoCasa::Ar,
        }
    }

    /// Quem atende no balcao: casa de LOJA (salao), NPC na porta.
    pub fn loja(self) -> bool {
        use Papel::*;
        matches!(
            self,
            Ferreiro
                | Armas
                | Armaduras
                | Itens
                | Identificador
                | Treinador
                | Taberna
                | Alfaiate
                | Mercador
                | Alquimista
        )
    }

    /// O nome do NPC do oficio. ASCII: a fonte do cliente nao tem acento.
    pub fn nome(self) -> &'static str {
        use Papel::*;
        match self {
            Casa => "Resident",
            Ferreiro => "Blacksmith",
            Armas | Armaduras => "Armourer",
            Itens => "Mercador",
            Identificador => "Appraiser",
            Treinador => "Trainer",
            Cartografo => "Cartographer",
            Taberna => "Innkeeper",
            Deposito => "Banker",
            Alfaiate => "Tailor",
            Estaleiro => "Harbour Captain",
            Mercador => "Mercador",
            Naufrago => "Castaway",
            Alquimista => "Alchemist",
            Missoes => "Quest Master",
            Motorista => "Sky Bus Professor",
        }
    }
}

/// Mobilia de rua, no voxel fino. So' o POCO barra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoProp {
    Lampiao,
    Poco,
    Banco,
    Caixas,
    Barril,
    // ── enfeites da cidade ──
    Canteiro,
    Arbusto,
    ArvoreOrnamental,
    Vaso,
    Cerca,
    Barraca,
    Carroca,
    Varal,
    Bandeirolas,
    Lenha,
    Portal,
    QuadroDeAvisos,
    Rede,
    Boia,
    Barquinho,
    FarolTormenta,
    RuinaTormenta,
    CristalTormenta,
    /// Kōgen-tō's FLYING BUS, parked by its driver in Skyreach and in
    /// Kōgen-tō: a blue bus hanging under a striped hot-air balloon.
    OnibusVoador,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tipo {
    Casa(TipoCasa),
    Prop(TipoProp),
}

// ─────────────────────────────── volume ──────────────────────────────

/// Volume de blocos generico. `set` fora dos limites e' ignorado — as
/// receitas contam com isso (o beiral escreve em `x = -1`).
#[derive(Debug, Clone, PartialEq)]
pub struct Voxels {
    pub x0: i32,
    pub y0: i32,
    pub z0: i32,
    pub nx: i32,
    pub ny: i32,
    pub nz: i32,
    dados: Vec<u8>,
}

impl Voxels {
    /// Every block repainted with `f` (`BlocoCasa::celeste`).
    pub fn repintar(&mut self, f: impl Fn(BlocoCasa) -> BlocoCasa) {
        for b in self.dados.iter_mut() {
            if let Some(m) = BlocoCasa::de_u8(*b) {
                *b = f(m) as u8;
            }
        }
    }
}


impl Voxels {
    pub fn novo(x0: i32, y0: i32, z0: i32, nx: i32, ny: i32, nz: i32) -> Self {
        Self {
            x0,
            y0,
            z0,
            nx,
            ny,
            nz,
            dados: vec![0; (nx * ny * nz).max(0) as usize],
        }
    }

    pub fn dentro(&self, x: i32, y: i32, z: i32) -> bool {
        x >= self.x0
            && y >= self.y0
            && z >= self.z0
            && x < self.x0 + self.nx
            && y < self.y0 + self.ny
            && z < self.z0 + self.nz
    }

    fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        (((z - self.z0) * self.nx + (x - self.x0)) * self.ny + (y - self.y0)) as usize
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if self.dentro(x, y, z) {
            self.dados[self.idx(x, y, z)]
        } else {
            0
        }
    }

    pub fn bloco(&self, x: i32, y: i32, z: i32) -> BlocoCasa {
        BlocoCasa::de_u8(self.get(x, y, z)).unwrap_or(BlocoCasa::Ar)
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, m: BlocoCasa) {
        if self.dentro(x, y, z) {
            let i = self.idx(x, y, z);
            self.dados[i] = m as u8;
        }
    }

    pub fn preenchidos(&self) -> usize {
        self.dados.iter().filter(|b| **b != 0).count()
    }

    /// Funde os blocos em caixas, guloso em X, depois Z, depois Y — a mesma
    /// ordem do zone14. Cada caixa e' `[ix0, iy0, iz0, ix1, iy1, iz1]` em
    /// CELULAS, com o fim exclusivo.
    pub fn caixas(&self) -> Vec<[i32; 6]> {
        let mut usado = vec![false; self.dados.len()];
        let mut saida = Vec::new();
        let entra = |usado: &[bool], x: i32, y: i32, z: i32| {
            self.get(x, y, z) != 0 && !usado[self.idx(x, y, z)]
        };
        for iy in self.y0..self.y0 + self.ny {
            for iz in self.z0..self.z0 + self.nz {
                for ix in self.x0..self.x0 + self.nx {
                    if !entra(&usado, ix, iy, iz) {
                        continue;
                    }
                    let mut ex = 1;
                    while ix + ex < self.x0 + self.nx && entra(&usado, ix + ex, iy, iz) {
                        ex += 1;
                    }
                    let livre_x = |usado: &[bool], y: i32, z: i32| {
                        (0..ex).all(|d| entra(usado, ix + d, y, z))
                    };
                    let mut ez = 1;
                    while iz + ez < self.z0 + self.nz && livre_x(&usado, iy, iz + ez) {
                        ez += 1;
                    }
                    let mut ey = 1;
                    while iy + ey < self.y0 + self.ny
                        && (0..ez).all(|d| livre_x(&usado, iy + ey, iz + d))
                    {
                        ey += 1;
                    }
                    for dy in 0..ey {
                        for dz in 0..ez {
                            for dx in 0..ex {
                                let i = self.idx(ix + dx, iy + dy, iz + dz);
                                usado[i] = true;
                            }
                        }
                    }
                    saida.push([ix, iy, iz, ix + ex, iy + ey, iz + ez]);
                }
            }
        }
        saida
    }
}

// ─────────────────────────── a construcao ────────────────────────────

/// Um predio ou prop gerado.
#[derive(Debug, Clone, PartialEq)]
pub struct Construcao {
    pub tipo: Tipo,
    pub v: Voxels,
    /// Lado do voxel em unidades de mundo (`B_CASA` ou `B_PROP`).
    pub escala: f32,
    /// Altura aproximada ate' a cumeeira, em unidades.
    pub altura: f32,
}

/// Gera um predio. O `papel` so' pinta toldo e placa (e escolhe o salao de
/// loja no casebre).
pub fn gerar(tipo: TipoCasa, papel: Papel, seed: i32) -> Construcao {
    let mut c = match tipo {
        TipoCasa::Armazem => armazem(seed),
        TipoCasa::Doca => doca(seed),
        TipoCasa::Cabana if papel == Papel::Cartografo => cabana_do_cartografo(seed),
        TipoCasa::Cabana => cabana(seed),
        TipoCasa::Casebre => casebre(seed, papel),
    };
    poe_toldo(&mut c, papel);
    poe_placa(&mut c, papel);
    c
}

/// Gera um prop de rua.
pub fn gerar_prop(tipo: TipoProp, seed: i32) -> Construcao {
    match tipo {
        TipoProp::Lampiao => lampiao(seed),
        TipoProp::Poco => poco(seed),
        TipoProp::Banco => banco(),
        TipoProp::Caixas => caixas(seed),
        TipoProp::Barril => barril(seed),
        TipoProp::Canteiro => canteiro(seed),
        TipoProp::Arbusto => arbusto(seed),
        TipoProp::ArvoreOrnamental => arvore_ornamental(seed),
        TipoProp::Vaso => vaso(seed),
        TipoProp::Cerca => cerca(),
        TipoProp::Barraca => barraca(seed),
        TipoProp::Carroca => carroca(seed),
        TipoProp::Varal => varal(seed),
        TipoProp::Bandeirolas => bandeirolas(),
        TipoProp::Lenha => lenha(),
        TipoProp::Portal => portal(seed),
        TipoProp::QuadroDeAvisos => quadro_de_avisos(seed),
        TipoProp::Rede => rede(),
        TipoProp::Boia => boia(),
        TipoProp::Barquinho => barquinho(seed),
        TipoProp::FarolTormenta | TipoProp::RuinaTormenta | TipoProp::CristalTormenta => marco_tormenta(tipo),
        TipoProp::OnibusVoador => onibus_voador(),
    }
}

impl Construcao {
    /// Pivo de rotacao, no plano XZ local, em unidades: o centro do volume.
    pub fn pivo(&self) -> Vec2 {
        Vec2::new(
            (self.v.x0 as f32 + self.v.nx as f32 * 0.5) * self.escala,
            (self.v.z0 as f32 + self.v.nz as f32 * 0.5) * self.escala,
        )
    }

    /// Meias-dimensoes do volume no chao, ja' giradas pelo quarto de volta.
    pub fn meia(&self, yaw_q: u8) -> Vec2 {
        let m = Vec2::new(self.v.nx as f32, self.v.nz as f32) * self.escala * 0.5;
        if yaw_q % 2 == 1 {
            Vec2::new(m.y, m.x)
        } else {
            m
        }
    }

    /// Ponto LOCAL (unidades, eixos do volume) pro mundo, dado onde o pivo
    /// esta' (`pos.xz`), a altura da origem do volume (`pos.y`) e o giro.
    pub fn local_para_mundo(&self, pos: Vec3, yaw_q: u8, l: Vec3) -> Vec3 {
        let p = self.pivo();
        let (rx, rz) = rot_q(l.x - p.x, l.z - p.y, yaw_q);
        Vec3::new(rx + pos.x, l.y + pos.y, rz + pos.z)
    }

    /// Caixas solidas em MUNDO: `(min, max)`.
    pub fn caixas_mundo(&self, pos: Vec3, yaw_q: u8) -> Vec<(Vec3, Vec3)> {
        let e = self.escala;
        self.v
            .caixas()
            .into_iter()
            .map(|[x0, y0, z0, x1, y1, z1]| {
                let a = self.local_para_mundo(
                    pos,
                    yaw_q,
                    Vec3::new(x0 as f32 * e, y0 as f32 * e, z0 as f32 * e),
                );
                let b = self.local_para_mundo(
                    pos,
                    yaw_q,
                    Vec3::new(x1 as f32 * e, y1 as f32 * e, z1 as f32 * e),
                );
                (a.min(b), a.max(b))
            })
            .collect()
    }

    /// Centro X local (unidades) do vao da PORTA na fachada `z = 0`: o vao
    /// que chega a `y = 4` (janela para em 3). Cabana baixa: o de `y = 3`.
    pub fn porta_local(&self) -> Option<f32> {
        for y in [4, 3] {
            let mut x0 = i32::MAX;
            let mut x1 = i32::MIN;
            for x in self.v.x0..self.v.x0 + self.v.nx {
                if self.v.get(x, y, 0) == 0 && self.v.get(x, 1, 0) == 0 && self.v.get(x, 0, 0) != 0
                {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                }
            }
            if x1 >= x0 {
                return Some((x0 + x1 + 1) as f32 * 0.5 * self.escala);
            }
        }
        None
    }
}

/// Quarto de volta no plano XZ: q1 `(z, -x)`, q2 `(-x, -z)`, q3 `(-z, x)`.
pub fn rot_q(x: f32, z: f32, q: u8) -> (f32, f32) {
    match q % 4 {
        0 => (x, z),
        1 => (z, -x),
        2 => (-x, -z),
        _ => (-z, x),
    }
}

/// O giro que faz a FRENTE (`-z` local, a porta) apontar pra `dir`.
pub fn quarto_para(dir: Vec2) -> u8 {
    let mut melhor = (f32::MIN, 0u8);
    for q in 0..4u8 {
        let (fx, fz) = rot_q(0.0, -1.0, q);
        let d = fx * dir.x + fz * dir.y;
        if d > melhor.0 {
            melhor = (d, q);
        }
    }
    melhor.1
}

/// Direcao de mundo pra onde a frente de um giro olha.
pub fn frente_de(yaw_q: u8) -> Vec2 {
    let (x, z) = rot_q(0.0, -1.0, yaw_q);
    Vec2::new(x, z)
}

fn borda(x: i32, z: i32, nx: i32, nz: i32) -> bool {
    x == 0 || z == 0 || x == nx - 1 || z == nz - 1
}

fn casa(tipo: TipoCasa, v: Voxels) -> Construcao {
    Construcao {
        tipo: Tipo::Casa(tipo),
        v,
        escala: B_CASA,
        altura: 0.0,
    }
}

fn prop(tipo: TipoProp, v: Voxels) -> Construcao {
    Construcao {
        tipo: Tipo::Prop(tipo),
        v,
        escala: B_PROP,
        altura: 0.0,
    }
}

// ───────────────────────────── receitas ──────────────────────────────

fn poe_toldo(c: &mut Construcao, papel: Papel) {
    let lona = papel.toldo();
    if lona == BlocoCasa::Ar || !matches!(c.tipo, Tipo::Casa(t) if t != TipoCasa::Doca) {
        return;
    }
    let v = &mut c.v;
    let (mut x0, mut x1) = (i32::MAX, i32::MIN);
    for x in v.x0..v.x0 + v.nx {
        if v.get(x, 2, 0) == 0 && v.get(x, 0, 0) != 0 {
            x0 = x0.min(x);
            x1 = x1.max(x);
        }
    }
    if x1 < x0 {
        return;
    }
    for x in x0 - 1..=x1 + 1 {
        v.set(x, 5, -1, lona);
        v.set(x, 5, 0, lona);
    }
}

fn poe_placa(c: &mut Construcao, papel: Papel) {
    let emblema = papel.emblema();
    if emblema == BlocoCasa::Ar || c.tipo != Tipo::Casa(TipoCasa::Casebre) {
        return;
    }
    let v = &mut c.v;
    let (mut x0, mut x1) = (i32::MAX, i32::MIN);
    for x in v.x0..v.x0 + v.nx {
        if v.get(x, 4, 0) == 0 && v.get(x, 0, 0) != 0 {
            x0 = x0.min(x);
            x1 = x1.max(x);
        }
    }
    if x1 < x0 || x0 - 2 < v.x0 || x1 + 2 >= v.x0 + v.nx {
        return;
    }
    for x in x0 - 2..=x1 + 2 {
        let b = if x == x0 - 2 || x == x1 + 2 {
            BlocoCasa::Viga
        } else if x >= x0 && x <= x1 {
            emblema
        } else {
            BlocoCasa::Tabua
        };
        v.set(x, 5, -1, b);
        v.set(x, 6, -1, b);
    }
}

/// Parede com enxaimel: cantos, prumos a cada 4 e o frechal.
fn enxaimel(v: &mut Voxels, nx: i32, nz: i32, pe: i32, parede: BlocoCasa, com_canto: bool) {
    for y in 1..=pe {
        for x in 0..nx {
            for z in 0..nz {
                if !borda(x, z, nx, nz) {
                    continue;
                }
                let canto = com_canto && (x == 0 || x == nx - 1) && (z == 0 || z == nz - 1);
                let prumo = ((z == 0 || z == nz - 1) && x % 4 == 0)
                    || ((x == 0 || x == nx - 1) && z % 4 == 0);
                v.set(
                    x,
                    y,
                    z,
                    if canto || prumo || y == pe {
                        BlocoCasa::Viga
                    } else {
                        parede
                    },
                );
            }
        }
    }
}

fn alicerce(v: &mut Voxels, nx: i32, nz: i32) {
    for x in 0..nx {
        for z in 0..nz {
            v.set(
                x,
                0,
                z,
                if borda(x, z, nx, nz) {
                    BlocoCasa::PedraBase
                } else {
                    BlocoCasa::Assoalho
                },
            );
        }
    }
}

/// Telhado de duas aguas em casca, com beiral e oitoes.
fn telhado(v: &mut Voxels, nx: i32, nz: i32, pe: i32, telha: BlocoCasa, parede: BlocoCasa) {
    let mut k = 0;
    loop {
        let (z0, z1) = (k - 1, nz - k);
        let y = pe + 1 + k;
        if z1 - z0 <= 1 {
            for x in -1..=nx {
                for z in z0.max(0)..=z1.min(nz - 1) {
                    v.set(x, y, z, BlocoCasa::Viga);
                }
            }
            break;
        }
        for x in -1..=nx {
            v.set(x, y, z0, telha);
            v.set(x, y, z1, telha);
        }
        for z in z0 + 1..=z1 - 1 {
            if z < 0 || z > nz - 1 {
                continue;
            }
            v.set(0, y, z, parede);
            v.set(nx - 1, y, z, parede);
        }
        k += 1;
    }
}

fn cabana(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 17, 907));
    let (nx, nz, pe) = (r.int(8, 11), r.int(7, 9), 4);
    let mut v = Voxels::novo(-1, 0, -1, nx + 2, pe + 8, nz + 2);
    for x in 0..nx {
        for z in 0..nz {
            if borda(x, z, nx, nz) {
                v.set(x, 0, z, BlocoCasa::PedraBase);
            }
        }
    }
    for y in 1..=pe {
        for x in 0..nx {
            for z in 0..nz {
                if !borda(x, z, nx, nz) {
                    continue;
                }
                let canto = (x == 0 || x == nx - 1) && (z == 0 || z == nz - 1);
                if !canto && r.float() < 0.10 {
                    continue;
                }
                v.set(
                    x,
                    y,
                    z,
                    if canto {
                        BlocoCasa::Viga
                    } else {
                        BlocoCasa::Tabua
                    },
                );
            }
        }
    }
    let px = nx / 2;
    // O vao vai ate' o beiral: verga em y = 4 fica abaixo do teto da colisao
    // e a porta viraria parede pra quem anda.
    for y in 1..=pe {
        v.set(px, y, 0, BlocoCasa::Ar);
        v.set(px + 1, y, 0, BlocoCasa::Ar);
    }
    v.set(px, 0, 0, BlocoCasa::Assoalho);
    v.set(px + 1, 0, 0, BlocoCasa::Assoalho);
    let lona = if r.float() < 0.55 {
        BlocoCasa::Pano
    } else {
        BlocoCasa::Ardosia
    };
    for x in -1..=nx {
        for z in -1..=nz {
            v.set(x, pe + 1 + (nz - 1 - z) / 4, z, lona);
        }
    }
    let mut c = casa(TipoCasa::Cabana, v);
    c.altura = (pe + 3) as f32 * B_CASA;
    c
}

fn cabana_do_cartografo(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 23, 4111));
    let (mut nx, nz, pe) = (r.int(11, 13), r.int(8, 10), 5);
    if nx & 1 == 1 {
        nx += 1;
    }
    let alt_telhado = (nz + 1) / 2 + 1;
    let mut v = Voxels::novo(-1, 0, -1, nx + 2, pe + alt_telhado + 5, nz + 2);
    alicerce(&mut v, nx, nz);
    enxaimel(&mut v, nx, nz, pe, BlocoCasa::Tabua, true);
    let px = nx / 2;
    for y in 1..=4 {
        v.set(px, y, 0, BlocoCasa::Ar);
        v.set(px + 1, y, 0, BlocoCasa::Ar);
    }
    for y in 1..=5 {
        v.set(px - 1, y, 0, BlocoCasa::Viga);
        v.set(px + 2, y, 0, BlocoCasa::Viga);
    }
    v.set(px, 0, 0, BlocoCasa::Assoalho);
    v.set(px + 1, 0, 0, BlocoCasa::Assoalho);
    v.set(px - 2, 4, -1, BlocoCasa::Metal);
    v.set(px - 2, 3, -1, BlocoCasa::Lume);
    v.set(px + 3, 4, -1, BlocoCasa::Metal);
    v.set(px + 3, 3, -1, BlocoCasa::Lume);
    let jan = |v: &mut Voxels, x0: i32| {
        for y in 2..=3 {
            v.set(x0, y, 0, BlocoCasa::Ar);
            v.set(x0 + 1, y, 0, BlocoCasa::Ar);
        }
        v.set(x0, 1, 0, BlocoCasa::Viga);
        v.set(x0 + 1, 1, 0, BlocoCasa::Viga);
    };
    jan(&mut v, 1);
    jan(&mut v, nx - 3);
    for y in 2..=3 {
        v.set(0, y, nz / 2, BlocoCasa::Ar);
        v.set(0, y, nz / 2 + 1, BlocoCasa::Ar);
        v.set(nx - 1, y, nz / 2, BlocoCasa::Ar);
        v.set(nx - 1, y, nz / 2 + 1, BlocoCasa::Ar);
    }
    telhado(&mut v, nx, nz, pe, BlocoCasa::Ardosia, BlocoCasa::Tabua);
    for x in px - 3..=px + 4 {
        v.set(x, pe + 1, -1, BlocoCasa::Ardosia);
        v.set(x, pe, -2, BlocoCasa::Ardosia);
    }
    for y in 1..pe {
        v.set(px - 3, y, -2, BlocoCasa::Viga);
        v.set(px + 4, y, -2, BlocoCasa::Viga);
    }
    let mut c = casa(TipoCasa::Cabana, v);
    c.altura = (pe + alt_telhado + 2) as f32 * B_CASA;
    c
}

fn casebre(seed: i32, papel: Papel) -> Construcao {
    let mut r = Rng::novo(semente(seed, 31, 1601));
    let porte = r.float();
    let sobrado = porte >= 0.75;
    let loja = papel.loja();
    let (nx, nz, pe);
    if loja {
        nx = r.int(19, 25);
        nz = r.int(14, (nx - 4).min(19));
        pe = r.int(6, 8);
    } else if porte < 0.30 {
        nx = r.int(11, 14);
        nz = r.int(8, nx - 2);
        pe = 5;
    } else if !sobrado {
        nx = r.int(13, 18);
        nz = r.int(9, (nx - 3).min(13));
        pe = r.int(5, 7);
    } else {
        nx = r.int(16, 21);
        nz = r.int(11, (nx - 4).min(15));
        pe = r.int(9, 11);
    }
    let parede = if r.float() < 0.40 {
        BlocoCasa::Tabua
    } else {
        BlocoCasa::Reboco
    };
    let telha = if r.float() < 0.35 {
        BlocoCasa::Ardosia
    } else {
        BlocoCasa::Telha
    };

    let alt_telhado = (nz + 1) / 2 + 1;
    let mut v = Voxels::novo(-1, 0, -1, nx + 2, pe + alt_telhado + 5, nz + 2);
    alicerce(&mut v, nx, nz);
    enxaimel(&mut v, nx, nz, pe, parede, true);

    // PORTA com moldura: vao de 1 x 2 m, ombreiras e verga de viga.
    let px = 4 + r.int(0, (nx - 10).max(1));
    for y in 1..=4 {
        v.set(px, y, 0, BlocoCasa::Ar);
        v.set(px + 1, y, 0, BlocoCasa::Ar);
    }
    for y in 1..=5 {
        v.set(px - 1, y, 0, BlocoCasa::Viga);
        v.set(px + 2, y, 0, BlocoCasa::Viga);
    }
    v.set(px, 5, 0, BlocoCasa::Viga);
    v.set(px + 1, 5, 0, BlocoCasa::Viga);

    let jan_frente = |v: &mut Voxels, x0: i32| {
        for y in 2..=3 {
            v.set(x0, y, 0, BlocoCasa::Ar);
            v.set(x0 + 1, y, 0, BlocoCasa::Ar);
        }
        v.set(x0, 1, 0, BlocoCasa::Viga);
        v.set(x0 + 1, 1, 0, BlocoCasa::Viga);
    };
    let mut janelas_frente = Vec::new();
    if px >= 5 {
        jan_frente(&mut v, 2);
        janelas_frente.push(2);
    }
    if px + 2 <= nx - 5 {
        jan_frente(&mut v, nx - 4);
        janelas_frente.push(nx - 4);
    }

    if sobrado {
        for x in 0..nx {
            for z in 0..nz {
                if borda(x, z, nx, nz) && v.get(x, 5, z) != 0 {
                    v.set(x, 5, z, BlocoCasa::Viga);
                }
            }
        }
        let jan_cima = |v: &mut Voxels, x0: i32| {
            for y in 7..=8 {
                v.set(x0, y, 0, BlocoCasa::Ar);
                v.set(x0 + 1, y, 0, BlocoCasa::Ar);
            }
            v.set(x0, 6, 0, BlocoCasa::Viga);
            v.set(x0 + 1, 6, 0, BlocoCasa::Viga);
        };
        if px >= 5 {
            jan_cima(&mut v, 2);
        }
        jan_cima(&mut v, px);
        if px + 2 <= nx - 5 {
            jan_cima(&mut v, nx - 4);
        }
        for y in 7..=8 {
            v.set(0, y, nz / 2, BlocoCasa::Ar);
            v.set(0, y, nz / 2 + 1, BlocoCasa::Ar);
            v.set(nx - 1, y, nz / 2, BlocoCasa::Ar);
            v.set(nx - 1, y, nz / 2 + 1, BlocoCasa::Ar);
        }
    }
    for y in 2..=3 {
        v.set(0, y, nz / 2, BlocoCasa::Ar);
        v.set(0, y, nz / 2 + 1, BlocoCasa::Ar);
        v.set(nx - 1, y, nz / 2, BlocoCasa::Ar);
        v.set(nx - 1, y, nz / 2 + 1, BlocoCasa::Ar);
    }

    telhado(&mut v, nx, nz, pe, telha, parede);

    if r.float() < 0.45 {
        let chx = if r.float() < 0.5 { 2 } else { nx - 3 };
        let chz = 2;
        let base = pe + 1 + (chz + 1);
        for y in base - 1..=base + 2 {
            v.set(chx, y, chz, BlocoCasa::PedraBase);
            v.set(chx, y, chz + 1, BlocoCasa::PedraBase);
        }
        v.set(chx, base + 3, chz, BlocoCasa::Viga);
        v.set(chx, base + 3, chz + 1, BlocoCasa::Viga);
    }

    // ══ DETALHES DO TEMPEST — sorteio proprio, depois do original ══
    let mut d = Rng::novo(semente(seed, 37, 2711));

    // DIAGONAIS do enxaimel: a mao-francesa nos paineis de baixo. Cruza so'
    // parede cheia — nunca vao de porta ou janela.
    if pe >= 4 {
        let mut painel = 0;
        let mut diagonal = |v: &mut Voxels, fora: &dyn Fn(i32, i32) -> (i32, i32), n: i32| {
            let mut a = 0;
            while a + 4 < n {
                let sobe = painel % 2 == 0;
                painel += 1;
                for t in 1..=3 {
                    let (x, z) = fora(a + if sobe { t } else { 4 - t }, t);
                    let _ = (x, z);
                    let (cx, cz) = fora(a + if sobe { t } else { 4 - t }, 0);
                    if v.get(cx, t, cz) == parede as u8 {
                        v.set(cx, t, cz, BlocoCasa::Viga);
                    }
                }
                a += 4;
            }
        };
        diagonal(&mut v, &|i, _| (i, 0), nx);
        diagonal(&mut v, &|i, _| (i, nz - 1), nx);
        diagonal(&mut v, &|i, _| (0, i), nz);
        diagonal(&mut v, &|i, _| (nx - 1, i), nz);
    }

    // FLOREIRA sob a janela da frente: caixa de tabua com flor.
    for &x0 in &janelas_frente {
        let flor = if d.float() < 0.5 {
            BlocoCasa::Flor
        } else {
            BlocoCasa::FlorAmarela
        };
        for x in x0..=x0 + 1 {
            v.set(x, 1, -1, BlocoCasa::Tabua);
            v.set(
                x,
                2,
                -1,
                if d.float() < 0.3 {
                    BlocoCasa::Folha
                } else {
                    flor
                },
            );
        }
    }

    // SOLEIRA: a laje de pedra na frente da porta.
    v.set(px, 0, -1, BlocoCasa::PedraBase);
    v.set(px + 1, 0, -1, BlocoCasa::PedraBase);

    // LAMPIAO nas ombreiras da loja: quem chega de noite acha o balcao.
    if loja {
        for lx in [px - 2, px + 3] {
            if lx >= 0 && lx <= nx - 1 {
                v.set(lx, 4, -1, BlocoCasa::Metal);
                v.set(lx, 3, -1, BlocoCasa::Lume);
            }
        }
    }

    // PISO do sobrado, com o vao da ladder no fundo.
    if sobrado {
        for x in 1..nx - 1 {
            for z in 1..nz - 1 {
                if x <= 2 && z >= nz - 4 {
                    continue;
                }
                v.set(x, 5, z, BlocoCasa::Assoalho);
            }
        }
    }

    // ══ COR — sorteio proprio, depois de tudo ══
    // Troca de MATERIAL em celula ja' cheia nao muda nem o vao da porta nem a
    // colisao. So' veneziana e hera acrescentam celula, e as duas ficam fora
    // do vao (ao lado da janela e na parede lateral).
    let mut k = Rng::novo(semente(seed, 41, 3301));
    let tons = [
        BlocoCasa::Reboco,
        BlocoCasa::RebocoOcre,
        BlocoCasa::RebocoRosa,
        BlocoCasa::RebocoAzul,
        BlocoCasa::RebocoVerde,
    ];
    let reboco = tons[k.int(0, 5) as usize];
    let telha_nova = if papel == Papel::Casa && k.float() < 0.45 {
        BlocoCasa::Palha
    } else if telha == BlocoCasa::Telha && k.float() < 0.4 {
        BlocoCasa::TelhaEscura
    } else {
        telha
    };
    let tintas = [
        BlocoCasa::PinturaVermelha,
        BlocoCasa::PinturaAzul,
        BlocoCasa::PinturaVerde,
        BlocoCasa::PinturaAmarela,
    ];
    let tinta = tintas[k.int(0, 4) as usize];
    // RODAPE: a primeira fiada da parede num tom mais escuro.
    let rodape = if parede == BlocoCasa::Reboco {
        BlocoCasa::PedraBase
    } else {
        BlocoCasa::Viga
    };
    for x in 0..nx {
        for z in 0..nz {
            if borda(x, z, nx, nz) && v.get(x, 1, z) == parede as u8 {
                v.set(x, 1, z, rodape);
            }
        }
    }
    for x in v.x0..v.x0 + v.nx {
        for y in v.y0..v.y0 + v.ny {
            for z in v.z0..v.z0 + v.nz {
                let b = v.get(x, y, z);
                if b == BlocoCasa::Reboco as u8 && reboco != BlocoCasa::Reboco {
                    v.set(x, y, z, reboco);
                } else if b == telha as u8 && telha_nova != telha {
                    v.set(x, y, z, telha_nova);
                }
            }
        }
    }
    // PORTA pintada: ombreiras e verga na cor da casa.
    for y in 1..=5 {
        v.set(px - 1, y, 0, tinta);
        v.set(px + 2, y, 0, tinta);
    }
    v.set(px, 5, 0, tinta);
    v.set(px + 1, 5, 0, tinta);
    // VENEZIANAS abertas dos dois lados das janelas da frente.
    for &x0 in &janelas_frente {
        for y in 2..=3 {
            for sx in [x0 - 1, x0 + 2] {
                if sx >= 0 && sx < nx && v.get(sx, y, -1) == 0 {
                    v.set(sx, y, -1, tinta);
                }
            }
        }
    }
    // HERA subindo numa parede lateral, contornando a janela.
    if k.float() < 0.35 {
        let (fora, dentro) = if k.float() < 0.5 {
            (-1, 0)
        } else {
            (nx, nx - 1)
        };
        let z_ini = k.int(1, (nz - 5).max(2));
        let fim = (z_ini + k.int(3, 6)).min(nz - 1);
        for z in z_ini..fim {
            let alt = k.int(2, pe.max(3));
            for y in 1..=alt {
                if v.get(dentro, y, z) == 0 || k.float() >= 0.8 {
                    continue;
                }
                let b = if k.float() < 0.12 {
                    BlocoCasa::Flor
                } else if k.float() < 0.4 {
                    BlocoCasa::FolhaClara
                } else {
                    BlocoCasa::Folha
                };
                v.set(fora, y, z, b);
            }
        }
    }

    let mut c = casa(TipoCasa::Casebre, v);
    c.altura = (pe + alt_telhado + 2) as f32 * B_CASA;
    c
}

fn armazem(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 31, 1733));
    let (nx, nz, pe) = (r.int(15, 21), r.int(12, 17), r.int(7, 10));
    let telha = if r.float() < 0.6 {
        BlocoCasa::Ardosia
    } else {
        BlocoCasa::Telha
    };
    let mut v = Voxels::novo(-1, 0, -1, nx + 2, pe + (nz + 3) / 2 + 4, nz + 2);
    alicerce(&mut v, nx, nz);
    enxaimel(&mut v, nx, nz, pe, BlocoCasa::Tabua, false);
    let px = nx / 2 - 2;
    for y in 1..=5 {
        for dx in 0..4 {
            v.set(px + dx, y, 0, BlocoCasa::Ar);
        }
    }
    let mut x = 3;
    while x < nx - 3 {
        v.set(x, pe - 1, 0, BlocoCasa::Ar);
        v.set(x, pe - 1, nz - 1, BlocoCasa::Ar);
        x += 4;
    }
    telhado(&mut v, nx, nz, pe, telha, BlocoCasa::Tabua);
    let mut c = casa(TipoCasa::Armazem, v);
    c.altura = (pe + (nz + 1) / 2 + 2) as f32 * B_CASA;
    c
}

fn doca(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 31, 1889));
    let comp = r.int(18, 28);
    let larg = r.int(5, 8);
    let mut v = Voxels::novo(0, 0, 0, larg, DECK_Y + 4, comp);
    let mut z = 1;
    while z < comp {
        for y in 0..DECK_Y {
            v.set(0, y, z, BlocoCasa::Viga);
            v.set(larg - 1, y, z, BlocoCasa::Viga);
        }
        z += 4;
    }
    for z in 0..comp {
        for x in 0..larg {
            v.set(
                x,
                DECK_Y,
                z,
                if z & 1 == 0 {
                    BlocoCasa::Assoalho
                } else {
                    BlocoCasa::Tabua
                },
            );
        }
    }
    for x in 0..larg {
        v.set(x, DECK_Y - 1, 0, BlocoCasa::PedraBase);
        v.set(x, DECK_Y - 2, 0, BlocoCasa::PedraBase);
    }
    for z in [comp - 2, comp / 2] {
        v.set(0, DECK_Y + 1, z, BlocoCasa::Viga);
        v.set(larg - 1, DECK_Y + 1, z, BlocoCasa::Viga);
    }
    let mut c = casa(TipoCasa::Doca, v);
    c.altura = (DECK_Y + 2) as f32 * B_CASA;
    c
}

fn lampiao(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 31, 2203));
    let h = r.int(12, 16);
    let mut v = Voxels::novo(-3, 0, -3, 7, h + 6, 7);
    for y in 0..h {
        v.set(0, y, 0, BlocoCasa::Viga);
        v.set(-1, y, 0, BlocoCasa::Viga);
    }
    for (x, z) in [(-1, -1), (0, -1), (-1, 1), (0, 1)] {
        v.set(x, 0, z, BlocoCasa::PedraBase);
    }
    for dx in -2..=1 {
        for dz in -1..=1 {
            v.set(dx, h, dz, BlocoCasa::Viga);
            v.set(dx, h + 3, dz, BlocoCasa::Viga);
        }
    }
    for y in h + 1..=h + 2 {
        for dx in -1..=0 {
            v.set(dx, y, 0, BlocoCasa::Lume);
        }
    }
    prop(TipoProp::Lampiao, v)
}

fn poco(_seed: i32) -> Construcao {
    const R: i32 = 5;
    let mut v = Voxels::novo(-R - 1, 0, -R - 1, R * 2 + 3, 16, R * 2 + 3);
    for x in -R..=R {
        for z in -R..=R {
            let d2 = x * x + z * z;
            if d2 > R * R || d2 < (R - 2) * (R - 2) {
                continue;
            }
            for y in 0..=3 {
                v.set(x, y, z, BlocoCasa::PedraBase);
            }
        }
    }
    for y in 0..=9 {
        v.set(-R, y, 0, BlocoCasa::Viga);
        v.set(R, y, 0, BlocoCasa::Viga);
    }
    for x in -R..=R {
        v.set(x, 10, 0, BlocoCasa::Viga);
    }
    for x in -R - 1..=R + 1 {
        v.set(x, 11, -2, BlocoCasa::Telha);
        v.set(x, 11, 2, BlocoCasa::Telha);
        v.set(x, 12, -1, BlocoCasa::Telha);
        v.set(x, 12, 1, BlocoCasa::Telha);
        v.set(x, 13, 0, BlocoCasa::Telha);
    }
    prop(TipoProp::Poco, v)
}

fn banco() -> Construcao {
    let mut v = Voxels::novo(-6, 0, -2, 13, 6, 5);
    for x in [-5, 3] {
        for y in 0..=2 {
            v.set(x, y, -1, BlocoCasa::Viga);
            v.set(x, y, 1, BlocoCasa::Viga);
        }
    }
    for x in -6..=6 {
        for z in -1..=1 {
            v.set(x, 3, z, BlocoCasa::Assoalho);
        }
    }
    prop(TipoProp::Banco, v)
}

fn caixas(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 31, 2411));
    let quantas = r.int(2, 5);
    let mut v = Voxels::novo(-8, 0, -8, 17, 14, 17);
    for c in 0..quantas {
        let lado = r.int(4, 7);
        let (cx, cz) = (r.int(-4, 5), r.int(-4, 5));
        let cy = if c > 1 && r.float() < 0.5 { lado } else { 0 };
        for x in 0..lado {
            for z in 0..lado {
                for y in 0..lado {
                    let arestas = [
                        x == 0 || x == lado - 1,
                        y == 0 || y == lado - 1,
                        z == 0 || z == lado - 1,
                    ]
                    .iter()
                    .filter(|b| **b)
                    .count();
                    v.set(
                        cx + x,
                        cy + y,
                        cz + z,
                        if arestas >= 2 {
                            BlocoCasa::Viga
                        } else {
                            BlocoCasa::Tabua
                        },
                    );
                }
            }
        }
    }
    prop(TipoProp::Caixas, v)
}

fn barril(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 31, 2531));
    let (raio, alt) = (3, r.int(6, 9));
    let mut v = Voxels::novo(-raio - 1, 0, -raio - 1, raio * 2 + 3, alt + 2, raio * 2 + 3);
    for x in -raio..=raio {
        for z in -raio..=raio {
            if x * x + z * z > raio * raio + 1 {
                continue;
            }
            for y in 0..alt {
                v.set(
                    x,
                    y,
                    z,
                    if y == 1 || y == alt - 2 {
                        BlocoCasa::Viga
                    } else {
                        BlocoCasa::Tabua
                    },
                );
            }
        }
    }
    prop(TipoProp::Barril, v)
}

// ─────────────────────────── enfeites da cidade ───────────────────────
// Todos no voxel fino (`B_PROP`), frente em -z, base em y = 0.

const FLORES: [BlocoCasa; 5] = [
    BlocoCasa::Flor,
    BlocoCasa::FlorAmarela,
    BlocoCasa::FlorRoxa,
    BlocoCasa::FlorBranca,
    BlocoCasa::FlorLaranja,
];

const LONAS: [BlocoCasa; 5] = [
    BlocoCasa::Toldo,
    BlocoCasa::ToldoAmbar,
    BlocoCasa::ToldoAzul,
    BlocoCasa::ToldoVerde,
    BlocoCasa::ToldoRoxo,
];

/// Canteiro de tabua com terra e flor de duas cores, comprido em X.
fn canteiro(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 43, 3407));
    let (l, w) = (r.int(10, 15), 5);
    let (x0, z0) = (-l / 2, -w / 2);
    let mut v = Voxels::novo(x0 - 1, 0, z0 - 1, l + 2, 7, w + 2);
    let a = FLORES[r.int(0, 5) as usize];
    let b = FLORES[r.int(0, 5) as usize];
    for x in x0..x0 + l {
        for z in z0..z0 + w {
            if x == x0 || x == x0 + l - 1 || z == z0 || z == z0 + w - 1 {
                v.set(x, 0, z, BlocoCasa::Tabua);
                v.set(x, 1, z, BlocoCasa::Tabua);
                continue;
            }
            v.set(x, 0, z, BlocoCasa::Terra);
            v.set(x, 1, z, BlocoCasa::Terra);
            let s = r.float();
            if s < 0.78 {
                let alt = 2 + r.int(1, 3);
                for y in 2..alt {
                    v.set(x, y, z, BlocoCasa::Folha);
                }
                v.set(
                    x,
                    alt,
                    z,
                    if s < 0.42 {
                        a
                    } else if s < 0.68 {
                        b
                    } else {
                        BlocoCasa::FolhaClara
                    },
                );
            }
        }
    }
    prop(TipoProp::Canteiro, v)
}

/// Moita redonda; a maioria floresce.
fn arbusto(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 47, 3511));
    let (rx, ry) = (r.int(3, 5), r.int(3, 5));
    let flor = FLORES[r.int(0, 5) as usize];
    let florido = r.float() < 0.7;
    let mut v = Voxels::novo(-rx - 1, 0, -rx - 1, rx * 2 + 3, ry * 2 + 3, rx * 2 + 3);
    for x in -rx..=rx {
        for z in -rx..=rx {
            for y in 0..=ry * 2 {
                let dy = y - ry;
                let d =
                    (x * x + z * z) as f32 / (rx * rx) as f32 + (dy * dy) as f32 / (ry * ry) as f32;
                if d > 1.0 {
                    continue;
                }
                let b = if d > 0.55 && florido && r.float() < 0.22 {
                    flor
                } else if r.float() < 0.35 {
                    BlocoCasa::FolhaClara
                } else {
                    BlocoCasa::Folha
                };
                v.set(x, y, z, b);
            }
        }
    }
    prop(TipoProp::Arbusto, v)
}

/// Arvore pequena de praca: florida ou frutifera. A copa comeca ACIMA da
/// altura em que a colisao olha (1,7), entao so' o tronco barra.
fn arvore_ornamental(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 53, 3613));
    let h = r.int(15, 19);
    let rc = r.int(5, 7);
    let tipo = r.int(0, 3);
    let enfeite = [BlocoCasa::Flor, BlocoCasa::FlorBranca, BlocoCasa::Fruta][tipo as usize];
    let chance = if tipo == 2 { 0.12 } else { 0.3 };
    let cy = h + rc - 1;
    let mut v = Voxels::novo(-rc - 1, 0, -rc - 1, rc * 2 + 3, cy + rc + 2, rc * 2 + 3);
    for y in 0..h + 2 {
        for (x, z) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
            v.set(x, y, z, BlocoCasa::Viga);
        }
    }
    for x in -rc - 1..=rc {
        for z in -rc - 1..=rc {
            for y in h..=cy + rc {
                let (fx, fy, fz) = (x as f32 + 0.5, (y - cy) as f32, z as f32 + 0.5);
                let d = (fx * fx + fy * fy + fz * fz).sqrt();
                if d > rc as f32 + 0.3 {
                    continue;
                }
                let b = if d > rc as f32 - 1.3 && r.float() < chance {
                    enfeite
                } else if r.float() < 0.3 {
                    BlocoCasa::FolhaClara
                } else {
                    BlocoCasa::Folha
                };
                v.set(x, y, z, b);
            }
        }
    }
    prop(TipoProp::ArvoreOrnamental, v)
}

/// Vaso de porta com flor.
fn vaso(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 59, 3719));
    let cor = [
        BlocoCasa::Telha,
        BlocoCasa::PinturaAzul,
        BlocoCasa::PedraBase,
    ][r.int(0, 3) as usize];
    let flor = FLORES[r.int(0, 5) as usize];
    let mut v = Voxels::novo(-3, 0, -3, 7, 9, 7);
    for x in -2..=2 {
        for z in -2..=2 {
            let d = x * x + z * z;
            if d > 5 {
                continue;
            }
            for y in 0..=3 {
                v.set(
                    x,
                    y,
                    z,
                    if d >= 4 || y == 0 {
                        cor
                    } else {
                        BlocoCasa::Terra
                    },
                );
            }
        }
    }
    for (x, z) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        v.set(x, 4, z, BlocoCasa::Folha);
        v.set(
            x,
            5,
            z,
            if (x + z) % 2 == 0 {
                flor
            } else {
                BlocoCasa::FolhaClara
            },
        );
    }
    v.set(0, 6, 0, flor);
    prop(TipoProp::Vaso, v)
}

/// Um lance de cerca baixa de 2 u, comprido em X.
fn cerca() -> Construcao {
    let mut v = Voxels::novo(-8, 0, -1, 17, 7, 3);
    for x in -8..=8 {
        v.set(x, 2, 0, BlocoCasa::Tabua);
        v.set(x, 4, 0, BlocoCasa::Tabua);
    }
    for x in [-8, -3, 3, 8] {
        for y in 0..=5 {
            v.set(x, y, 0, BlocoCasa::Viga);
        }
    }
    prop(TipoProp::Cerca, v)
}

/// Barraca de feira: balcao na frente, toldo listrado, mercadoria colorida.
fn barraca(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 61, 3821));
    let lona_a = LONAS[r.int(0, 5) as usize];
    let lona_b = BlocoCasa::FlorBranca;
    let (hx, hz) = (12, 6);
    let mut v = Voxels::novo(-hx - 2, 0, -hz - 3, hx * 2 + 5, 26, hz * 2 + 6);
    for x in [-hx, hx] {
        for z in [-hz, hz] {
            for y in 0..=19 {
                v.set(x, y, z, BlocoCasa::Viga);
            }
        }
    }
    for x in -hx..=hx {
        for z in -hz..=-hz + 2 {
            for y in 0..=6 {
                v.set(
                    x,
                    y,
                    z,
                    if y == 6 {
                        BlocoCasa::Assoalho
                    } else {
                        BlocoCasa::Tabua
                    },
                );
            }
        }
        for z in hz - 2..=hz {
            for y in 0..=4 {
                v.set(x, y, z, BlocoCasa::Tabua);
            }
        }
    }
    for x in -hx - 1..=hx + 1 {
        for z in -hz - 3..=hz + 1 {
            let y = 20 + (z + hz + 3) / 4;
            v.set(
                x,
                y,
                z,
                if (x + 64) / 3 % 2 == 0 {
                    lona_a
                } else {
                    lona_b
                },
            );
        }
    }
    let mercadorias = [
        BlocoCasa::Fruta,
        BlocoCasa::FlorAmarela,
        BlocoCasa::FlorLaranja,
        BlocoCasa::Peixe,
        BlocoCasa::PanoRubro,
        BlocoCasa::ToldoAzul,
        BlocoCasa::Pano,
        BlocoCasa::FolhaClara,
    ];
    let mut x = -hx + 1;
    while x < hx - 1 {
        let m = mercadorias[r.int(0, mercadorias.len() as i32) as usize];
        let alto = r.float() < 0.5;
        for dx in 0..2 {
            for z in -hz..=-hz + 2 {
                v.set(x + dx, 7, z, m);
                if alto && z == -hz + 1 {
                    v.set(x + dx, 8, z, m);
                }
            }
        }
        x += 3;
    }
    prop(TipoProp::Barraca, v)
}

/// Carroca de duas rodas com carga.
fn carroca(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 67, 3923));
    let mut v = Voxels::novo(-15, 0, -8, 31, 12, 17);
    for x in -9..=9 {
        for z in -5..=5 {
            v.set(x, 4, z, BlocoCasa::Tabua);
            if x.abs() == 9 || z.abs() == 5 {
                for y in 5..=7 {
                    let canto = x.abs() == 9 && z.abs() == 5;
                    v.set(
                        x,
                        y,
                        z,
                        if canto {
                            BlocoCasa::Viga
                        } else {
                            BlocoCasa::Tabua
                        },
                    );
                }
            }
        }
    }
    for cx in [-5, 5] {
        for lado in [-6, 6] {
            for dx in -3i32..=3 {
                for dy in -3i32..=3 {
                    let d = dx * dx + dy * dy;
                    if d <= 10 {
                        v.set(
                            cx + dx,
                            3 + dy,
                            lado,
                            if d >= 5 {
                                BlocoCasa::Viga
                            } else {
                                BlocoCasa::Tabua
                            },
                        );
                    }
                }
            }
        }
    }
    for x in 10..=15 {
        v.set(x, 4, -2, BlocoCasa::Viga);
        v.set(x, 4, 2, BlocoCasa::Viga);
    }
    let carga = [
        BlocoCasa::Lenha,
        BlocoCasa::Fruta,
        BlocoCasa::Palha,
        BlocoCasa::FlorAmarela,
    ][r.int(0, 4) as usize];
    for x in -8..=8 {
        for z in -4..=4 {
            if r.float() < 0.8 {
                v.set(x, 5, z, carga);
                if r.float() < 0.45 {
                    v.set(x, 6, z, carga);
                }
            }
        }
    }
    prop(TipoProp::Carroca, v)
}

/// Varal: dois postes, corda e roupa colorida pendurada.
fn varal(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 71, 4027));
    let mut v = Voxels::novo(-13, 0, -1, 27, 16, 3);
    for x in [-12, 12] {
        for y in 0..=14 {
            v.set(x, y, 0, BlocoCasa::Viga);
        }
    }
    for x in -12..=12 {
        v.set(x, 14, 0, BlocoCasa::Corda);
    }
    let roupas = [
        BlocoCasa::Pano,
        BlocoCasa::ToldoAzul,
        BlocoCasa::PanoRubro,
        BlocoCasa::FlorAmarela,
        BlocoCasa::ToldoVerde,
        BlocoCasa::FlorBranca,
    ];
    let mut x = -10;
    while x < 9 {
        let (w, h) = (r.int(3, 5), r.int(3, 7));
        let cor = roupas[r.int(0, roupas.len() as i32) as usize];
        for dx in 0..w {
            for dy in 1..=h {
                v.set(x + dx, 14 - dy, 0, cor);
            }
        }
        x += w + 1 + r.int(0, 2);
    }
    prop(TipoProp::Varal, v)
}

/// Bandeirolas cruzando a praca: 14 u de corda com barriga e bandeirinhas.
fn bandeirolas() -> Construcao {
    const L: i32 = 56;
    let mut v = Voxels::novo(-L - 1, 0, -2, L * 2 + 3, 40, 5);
    for x in [-L, L] {
        for y in 0..=36 {
            v.set(x, y, 0, BlocoCasa::Viga);
        }
    }
    let altura = |x: i32| {
        let t = x as f32 / L as f32;
        36 - ((1.0 - t * t) * 5.0).round() as i32
    };
    for x in -L..=L {
        v.set(x, altura(x), 0, BlocoCasa::Corda);
    }
    let mut x = -L + 3;
    let mut k = 0;
    while x <= L - 3 {
        let cor = LONAS[k % LONAS.len()];
        let y0 = altura(x);
        for fila in 1..=3 {
            let meia = 3 - fila;
            for dx in -meia..=meia {
                v.set(x + dx, y0 - fila, 0, cor);
            }
        }
        x += 5;
        k += 1;
    }
    prop(TipoProp::Bandeirolas, v)
}

/// Pilha de lenha encostada na parede.
fn lenha() -> Construcao {
    let mut v = Voxels::novo(-7, 0, -4, 15, 8, 8);
    for camada in 0..3 {
        for i in 0..(3 - camada) {
            let z0 = -3 + camada + i * 2;
            for x in -6..=5 {
                for dy in 0..2 {
                    for dz in 0..2 {
                        let ponta = x == -6 || x == 5;
                        v.set(
                            x,
                            camada * 2 + dy,
                            z0 + dz,
                            if ponta {
                                BlocoCasa::Assoalho
                            } else {
                                BlocoCasa::Lenha
                            },
                        );
                    }
                }
            }
        }
    }
    prop(TipoProp::Lenha, v)
}

/// Portal de entrada da cidade: dois postes, viga, placa e lampioes.
fn portal(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 73, 4129));
    let mut v = Voxels::novo(-18, 0, -3, 37, 31, 7);
    for px in [-15, 14] {
        for dx in 0..2 {
            for z in -1..=1 {
                for y in 0..=27 {
                    v.set(px + dx, y, z, BlocoCasa::Viga);
                }
            }
        }
    }
    for x in -17..=17 {
        for z in -1..=1 {
            v.set(x, 26, z, BlocoCasa::Tabua);
            v.set(x, 27, z, BlocoCasa::Tabua);
        }
        v.set(x, 28, 0, BlocoCasa::Viga);
    }
    for x in -8i32..=8 {
        for y in 19..=24 {
            let borda = x.abs() == 8 || y == 19 || y == 24;
            v.set(
                x,
                y,
                -1,
                if borda {
                    BlocoCasa::Viga
                } else {
                    BlocoCasa::Papel
                },
            );
        }
    }
    let tinta = [
        BlocoCasa::PinturaVermelha,
        BlocoCasa::PinturaAzul,
        BlocoCasa::PinturaVerde,
    ][r.int(0, 3) as usize];
    let mut x = -6;
    while x <= 6 {
        v.set(x, 21, -2, tinta);
        v.set(x, 22, -2, tinta);
        x += 2;
    }
    for lx in [-11, 11] {
        v.set(lx, 25, -1, BlocoCasa::Metal);
        v.set(lx, 24, -1, BlocoCasa::Lume);
    }
    for px in [-16, 16] {
        for y in 0..24 {
            if r.float() < 0.45 {
                let b = if r.float() < 0.15 {
                    BlocoCasa::Flor
                } else {
                    BlocoCasa::Folha
                };
                v.set(px, y, -2, b);
            }
        }
    }
    prop(TipoProp::Portal, v)
}

/// Quadro de avisos com papeis pregados e um telhadinho.
fn quadro_de_avisos(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 79, 4231));
    let mut v = Voxels::novo(-9, 0, -2, 19, 19, 5);
    for x in [-7, 7] {
        for y in 0..=15 {
            v.set(x, y, 0, BlocoCasa::Viga);
        }
    }
    for x in -7i32..=7 {
        for y in 6..=13 {
            let borda = x.abs() == 7 || y == 6 || y == 13;
            v.set(
                x,
                y,
                0,
                if borda {
                    BlocoCasa::Viga
                } else {
                    BlocoCasa::Tabua
                },
            );
        }
    }
    for _ in 0..5 {
        let (px, py, w, h) = (r.int(-6, 4), r.int(7, 11), r.int(2, 4), r.int(2, 3));
        let papel = if r.float() < 0.5 {
            BlocoCasa::Papel
        } else {
            BlocoCasa::ToldoPergaminho
        };
        for dx in 0..w {
            for dy in 0..h {
                v.set(px + dx, py + dy, -1, papel);
            }
        }
        v.set(px, py + h - 1, -1, BlocoCasa::PinturaVermelha);
    }
    for x in -8..=8 {
        v.set(x, 16, -1, BlocoCasa::Telha);
        v.set(x, 16, 1, BlocoCasa::Telha);
        v.set(x, 17, 0, BlocoCasa::Telha);
    }
    prop(TipoProp::QuadroDeAvisos, v)
}

/// Rede de pesca estendida entre dois postes, com boias.
fn rede() -> Construcao {
    let mut v = Voxels::novo(-11, 0, -1, 23, 14, 3);
    for x in [-10, 10] {
        for y in 0..=12 {
            v.set(x, y, 0, BlocoCasa::Viga);
        }
    }
    for x in -9..=9 {
        for y in 3..=11 {
            if (x + y) % 2 == 0 || y == 11 {
                v.set(x, y, 0, BlocoCasa::Corda);
            }
        }
        if x % 4 == 0 {
            v.set(x, 10, -1, BlocoCasa::PinturaVermelha);
        }
    }
    prop(TipoProp::Rede, v)
}

/// Boia listrada.
fn boia() -> Construcao {
    let mut v = Voxels::novo(-3, 0, -3, 7, 6, 7);
    for x in -2i32..=2 {
        for z in -2i32..=2 {
            for y in 0..=4 {
                let dy = y - 2;
                if x * x + z * z + dy * dy <= 5 {
                    v.set(
                        x,
                        y,
                        z,
                        if y == 2 {
                            BlocoCasa::FlorBranca
                        } else {
                            BlocoCasa::PinturaVermelha
                        },
                    );
                }
            }
        }
    }
    prop(TipoProp::Boia, v)
}

/// Barco a remo amarrado, comprido em X.
fn barquinho(seed: i32) -> Construcao {
    let mut r = Rng::novo(semente(seed, 83, 4337));
    let cor = [
        BlocoCasa::PinturaAzul,
        BlocoCasa::PinturaVermelha,
        BlocoCasa::PinturaVerde,
        BlocoCasa::Tabua,
    ][r.int(0, 4) as usize];
    let (l, w) = (12, 4);
    let mut v = Voxels::novo(-l - 2, 0, -w - 3, l * 2 + 5, 7, w * 2 + 7);
    for x in -l..=l {
        let meia = (w - (x.abs() - (l - 4)).max(0)).max(1);
        for z in -meia..=meia {
            if z.abs() < meia && x.abs() < l {
                v.set(x, 0, z, BlocoCasa::Tabua);
            } else {
                for y in 0..=3 {
                    v.set(x, y, z, if y == 3 { BlocoCasa::Viga } else { cor });
                }
            }
        }
    }
    for bx in [-3, 5] {
        for z in -w + 1..=w - 1 {
            v.set(bx, 2, z, BlocoCasa::Tabua);
        }
    }
    for x in -5..=5 {
        v.set(x, 4, -w - 1, BlocoCasa::Viga);
        v.set(x, 4, w + 1, BlocoCasa::Viga);
    }
    prop(TipoProp::Barquinho, v)
}

/// The flying bus: a blue bus (white roof, a row of windows, yellow lamps)
/// hovering on its wheels, roped to a red, white and blue hot-air balloon.
fn onibus_voador() -> Construcao {
    let mut v = Voxels::novo(-14, 0, -13, 29, 53, 27);
    for x in -12..=12i32 {
        for y in 4..=13i32 {
            for z in -5..=5i32 {
                let casca = x.abs() == 12 || y == 4 || y == 13 || z.abs() == 5;
                if !casca {
                    continue;
                }
                let bloco = if y == 13 {
                    BlocoCasa::Pano
                } else if y <= 5 {
                    BlocoCasa::Metal
                } else if x == 12 && (6..=7).contains(&y) && z.abs() == 4 {
                    BlocoCasa::PinturaAmarela
                } else if (9..=11).contains(&y) && (x == 12 || (z.abs() == 5 && x.rem_euclid(3) != 0 && x.abs() < 11)) {
                    BlocoCasa::Janela
                } else {
                    BlocoCasa::PinturaAzul
                };
                v.set(x, y, z, bloco);
            }
        }
    }
    for (wx, wz) in [(-8, -5), (-8, 5), (8, -5), (8, 5)] {
        for x in wx - 1..=wx + 1 {
            for y in 2..=4 {
                v.set(x, y, wz, BlocoCasa::NegroBorda);
            }
        }
    }
    // Ropes from the roof's corners up to the balloon's mouth.
    for (cx, cz) in [(-10, -4), (-10, 4), (10, -4), (10, 4)] {
        for y in 14..=27 {
            let t = (y - 14) as f32 / 13.0;
            let x = (cx as f32 * (1.0 - t * 0.55)).round() as i32;
            let z = (cz as f32 * (1.0 - t * 0.2)).round() as i32;
            v.set(x, y, z, BlocoCasa::Corda);
        }
    }
    // The balloon, striped by angle.
    for y in 25..=52 {
        for x in -12..=12i32 {
            for z in -12..=12i32 {
                let e = (x * x + z * z) as f32 / 144.0 + ((y - 39) as f32 / 13.5).powi(2);
                if !(0.80..=1.0).contains(&e) {
                    continue;
                }
                let faixa = (((z as f32).atan2(x as f32) + std::f32::consts::PI) / std::f32::consts::TAU * 9.0) as i32 % 3;
                let bloco = [BlocoCasa::PinturaVermelha, BlocoCasa::Pano, BlocoCasa::PinturaAzul][faixa as usize];
                v.set(x, y, z, bloco);
            }
        }
    }
    let mut c = prop(TipoProp::OnibusVoador, v);
    c.escala = 0.25;
    c
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn rng_bate_com_o_do_zone14() {
        // Mesma conta do C#: (uint)(seed * 2654435761u) | 1, quatro voltas.
        let mut r = Rng::novo(1601);
        let a = r.next();
        let mut s: u32 = 1601u32.wrapping_mul(2_654_435_761) | 1;
        for _ in 0..5 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
        }
        assert_eq!(a, s);
        let mut r = Rng::novo(7);
        for _ in 0..1000 {
            let v = r.int(3, 9);
            assert!((3..9).contains(&v));
        }
    }

    #[test]
    fn caixas_cobrem_exatamente_os_voxels() {
        for seed in 0..40 {
            for papel in [Papel::Casa, Papel::Alquimista, Papel::Ferreiro] {
                let c = gerar(TipoCasa::Casebre, papel, seed);
                let total: i32 =
                    c.v.caixas()
                        .iter()
                        .map(|[a, b, cc, d, e, f]| (d - a) * (e - b) * (f - cc))
                        .sum();
                assert_eq!(total as usize, c.v.preenchidos(), "seed {seed}");
            }
        }
    }

    /// O vao da porta fica livre: 1 m de largura, 2 m de altura acima do piso.
    #[test]
    fn a_porta_e_um_vao_livre() {
        for seed in 0..200 {
            for (tipo, papel) in [
                (TipoCasa::Casebre, Papel::Casa),
                (TipoCasa::Casebre, Papel::Alquimista),
                (TipoCasa::Casebre, Papel::Taberna),
                (TipoCasa::Armazem, Papel::Deposito),
                (TipoCasa::Cabana, Papel::Cartografo),
            ] {
                let c = gerar(tipo, papel, seed);
                let lx = c
                    .porta_local()
                    .unwrap_or_else(|| panic!("{tipo:?} {seed} sem porta"));
                let ix = (lx / c.escala - 1.0).round() as i32;
                for x in ix..=ix + 1 {
                    for y in 1..=4 {
                        for z in -1..=0 {
                            assert_eq!(
                                c.v.get(x, y, z),
                                0,
                                "{tipo:?}/{papel:?} seed {seed}: vao tapado em {x},{y},{z}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn giro_e_frente() {
        assert_eq!(quarto_para(Vec2::new(0.0, -1.0)), 0);
        assert_eq!(quarto_para(Vec2::new(-1.0, 0.0)), 1);
        assert_eq!(quarto_para(Vec2::new(0.0, 1.0)), 2);
        assert_eq!(quarto_para(Vec2::new(1.0, 0.0)), 3);
        for q in 0..4 {
            assert_eq!(quarto_para(frente_de(q)), q);
        }
    }
}

/// Silhuetas do Planalto, no mesmo pipeline voxel e colisão das construções.
fn marco_tormenta(tipo: TipoProp) -> Construcao {
    let (r, h) = match tipo { TipoProp::FarolTormenta => (7, 80), TipoProp::RuinaTormenta => (10, 24), _ => (4, 20) };
    let mut v = Voxels::novo(-r,0,-r,2*r+1,h+1,2*r+1);
    for y in 0..=h { for x in -r..=r { for z in -r..=r {
        let borda = x.abs().max(z.abs());
        let bloco = match tipo {
            TipoProp::FarolTormenta if y>=h-13 && y<h-3 && borda<=r-2 && x.abs()+z.abs()<=r+1 => BlocoCasa::Lume,
            TipoProp::FarolTormenta if (y==h-14 || y==h-3 || y==0 || y==1) && x.abs()+z.abs()<=r+3 => BlocoCasa::PedraBase,
            TipoProp::FarolTormenta if y>=h-2 && borda<=(h-y+1).max(1) => BlocoCasa::PinturaAzul,
            TipoProp::FarolTormenta if y<h-14 && x.abs()+z.abs()<=r+1 && borda<=r-1
                && (borda>=r-3 || y<3)
                && !(z<0 && x.abs()<=1 && ((y>8 && y<15)||(y>29 && y<36)||(y>50 && y<57))) =>
                    if y%12<=1 { BlocoCasa::PedraBase } else { BlocoCasa::PedraNegra },
            TipoProp::RuinaTormenta if borda >= r-1 && y < h-(x+z).abs()%8 && !(z == -r && x.abs()<4 && y<15) => BlocoCasa::PedraNegra,
            TipoProp::CristalTormenta if x.abs()+z.abs() <= ((h-y)/4).min(r) => BlocoCasa::PinturaAzul,
            _ => BlocoCasa::Ar,
        };
        v.set(x,y,z,bloco);
    } } }
    let mut c = prop(tipo,v);
    c.escala = 0.5;
    c
}
