//! CLAN FLAGS (docs/TERRITORIOS.md): every clan flies one — on its ships'
//! mainmasts, on the islets it holds and on the world map.
//!
//! A flag is either a PRESET (`Bandeira`: field colour, pattern, second
//! colour, emblem, emblem colour — five small numbers packed in a `u32`,
//! drawn here pixel by pixel the same on server and client) or an IMAGE the
//! leader uploaded, shown only once staff approved it (`BandeiraDoCla::
//! imagem`, already reduced to `LARG` × `ALT` RGBA by the server).

use serde::{Deserialize, Serialize};

/// Flag size in pixels (3:2).
pub const LARG: usize = 48;
pub const ALT: usize = 32;

/// The palette, `Bandeira::fundo`, `cor2` and `cor_emblema` index it.
pub const CORES: [[u8; 3]; 12] = [
    [24, 22, 26],    // black
    [236, 232, 220], // white
    [196, 40, 36],   // red
    [112, 22, 28],   // crimson
    [222, 176, 52],  // gold
    [222, 116, 34],  // orange
    [60, 150, 70],   // green
    [26, 80, 46],    // forest
    [52, 112, 200],  // blue
    [24, 38, 92],    // navy
    [118, 54, 156],  // purple
    [128, 128, 132], // grey
];

pub const NOMES_DAS_CORES: [&str; 12] = [
    "Black", "White", "Red", "Crimson", "Gold", "Orange", "Green", "Forest", "Blue", "Navy",
    "Purple", "Grey",
];

pub const NOMES_DOS_PADROES: [&str; 9] = [
    "Plain",
    "Halves",
    "Stripes",
    "Split",
    "Cross",
    "Diagonal",
    "Checks",
    "Border",
    "Canton",
];

pub const NOMES_DOS_EMBLEMAS: [&str; 9] = [
    "None", "Skull", "Anchor", "Swords", "Crown", "Star", "Moon", "Trident", "Wave",
];

/// The emblems, 12 × 12, drawn doubled in the middle of the flag.
const EMBLEMAS: [[&str; 12]; 8] = [
    // Skull
    [
        "....####....",
        "..########..",
        ".##########.",
        ".##..##..##.",
        ".##..##..##.",
        ".##########.",
        "..###..###..",
        "...######...",
        "...#.##.#...",
        "#..........#",
        ".###....###.",
        "....####....",
    ],
    // Anchor
    [
        ".....##.....",
        "....#..#....",
        ".....##.....",
        "..########..",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        "#....##....#",
        "##...##...##",
        ".##..##..##.",
        "..########..",
        "....####....",
    ],
    // Swords
    [
        "##........##",
        "###......###",
        ".###....###.",
        "..###..###..",
        "...######...",
        "....####....",
        "....####....",
        "...######...",
        "..##.##.##..",
        ".##......##.",
        "##........##",
        "#..........#",
    ],
    // Crown
    [
        "............",
        "#....##....#",
        "##..####..##",
        "###.####.###",
        "############",
        "############",
        "#.#.#..#.#.#",
        "############",
        "############",
        "............",
        "............",
        "............",
    ],
    // Star
    [
        ".....##.....",
        ".....##.....",
        "....####....",
        "############",
        ".##########.",
        "..########..",
        "...######...",
        "..########..",
        "..###..###..",
        ".###....###.",
        ".##......##.",
        "............",
    ],
    // Moon
    [
        "....####....",
        "..####......",
        ".####.......",
        ".###........",
        "####........",
        "####........",
        "####........",
        "####........",
        ".###........",
        ".####.......",
        "..####......",
        "....####....",
    ],
    // Trident
    [
        "#....##....#",
        "#....##....#",
        "#....##....#",
        "##...##...##",
        ".##########.",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        ".....##.....",
        ".....##.....",
    ],
    // Wave
    [
        "............",
        "...####.....",
        "..##..##....",
        ".##....##...",
        "##..##..##.#",
        "...#..#...##",
        "............",
        "...####.....",
        "..##..##....",
        ".##....##...",
        "##......####",
        "............",
    ],
];

/// A preset flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bandeira {
    pub fundo: u8,
    pub padrao: u8,
    pub cor2: u8,
    pub emblema: u8,
    pub cor_emblema: u8,
}

impl Bandeira {
    /// Packed in 20 bits (4 per field).
    pub fn empacota(self) -> u32 {
        (self.fundo as u32 & 15)
            | (self.padrao as u32 & 15) << 4
            | (self.cor2 as u32 & 15) << 8
            | (self.emblema as u32 & 15) << 12
            | (self.cor_emblema as u32 & 15) << 16
    }

    pub fn desempacota(v: u32) -> Bandeira {
        Bandeira {
            fundo: (v & 15) as u8,
            padrao: (v >> 4 & 15) as u8,
            cor2: (v >> 8 & 15) as u8,
            emblema: (v >> 12 & 15) as u8,
            cor_emblema: (v >> 16 & 15) as u8,
        }
        .valida()
    }

    /// Every field inside its list.
    pub fn valida(self) -> Bandeira {
        let c = CORES.len() as u8;
        Bandeira {
            fundo: self.fundo % c,
            padrao: self.padrao % NOMES_DOS_PADROES.len() as u8,
            cor2: self.cor2 % c,
            emblema: self.emblema % NOMES_DOS_EMBLEMAS.len() as u8,
            cor_emblema: self.cor_emblema % c,
        }
    }

    /// The flag a clan flies before it picks one: fixed by its id, black
    /// with a skull for the first, varied after.
    pub fn do_cla(cla_id: i64) -> Bandeira {
        let h = (cla_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 24;
        let c = CORES.len() as u64;
        let fundo = (h % c) as u8;
        let mut cor_emblema = ((h >> 8) % c) as u8;
        if cor_emblema == fundo {
            cor_emblema = (fundo + 1) % c as u8;
        }
        Bandeira {
            fundo,
            padrao: ((h >> 16) % NOMES_DOS_PADROES.len() as u64) as u8,
            cor2: ((h >> 20) % c) as u8,
            emblema: 1 + ((h >> 28) % (NOMES_DOS_EMBLEMAS.len() as u64 - 1)) as u8,
            cor_emblema,
        }
    }

    /// The flag in pixels, `LARG` × `ALT` RGBA, rows top to bottom.
    pub fn pixels(self) -> Vec<u8> {
        let b = self.valida();
        let cor = |i: u8| CORES[i as usize];
        let (f, c2, ce) = (cor(b.fundo), cor(b.cor2), cor(b.cor_emblema));
        let mut v = Vec::with_capacity(LARG * ALT * 4);
        for y in 0..ALT {
            for x in 0..LARG {
                let segunda = match b.padrao {
                    1 => y >= ALT / 2,
                    2 => (y * 3 / ALT) == 1,
                    3 => x >= LARG / 2,
                    4 => (x as i32 - 17).abs() <= 3 || (y as i32 - 16).abs() <= 3,
                    5 => x * ALT > y * LARG,
                    6 => (x / 8 + y / 8) % 2 == 1,
                    7 => x < 3 || y < 3 || x >= LARG - 3 || y >= ALT - 3,
                    8 => x < LARG / 2 && y < ALT / 2,
                    _ => false,
                };
                let mut p = if segunda { c2 } else { f };
                if b.emblema > 0 {
                    // 12 × 12 drawn at 2× in the middle (24 × 24).
                    let (ex, ey) = (x as i32 - 12, y as i32 - 4);
                    if (0..24).contains(&ex) && (0..24).contains(&ey) {
                        let linha = EMBLEMAS[(b.emblema - 1) as usize][(ey / 2) as usize];
                        if linha.as_bytes()[(ex / 2) as usize] == b'#' {
                            p = ce;
                        }
                    }
                }
                v.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        v
    }
}

/// A clan's flag as the sea sends it: its preset, and the approved image
/// when there is one (it wins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BandeiraDoCla {
    pub cla_id: i64,
    pub tag: String,
    pub preset: u32,
    /// `LARG` × `ALT` RGBA, approved by staff.
    pub imagem: Option<Vec<u8>>,
}

impl BandeiraDoCla {
    pub fn pixels(&self) -> Vec<u8> {
        match &self.imagem {
            Some(i) if i.len() == LARG * ALT * 4 => i.clone(),
            _ => Bandeira::desempacota(self.preset).pixels(),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_bandeira_empacota_desenha_e_tem_tamanho_fixo() {
        for id in 0..200 {
            let b = Bandeira::do_cla(id);
            assert_eq!(Bandeira::desempacota(b.empacota()), b);
            assert_eq!(b.pixels().len(), LARG * ALT * 4);
            assert_ne!(b.fundo, b.cor_emblema);
            assert!(b.emblema > 0);
        }
        // Out-of-range fields are folded back in, never a panic.
        let b = Bandeira::desempacota(0xF_FFFF);
        assert_eq!(b.pixels().len(), LARG * ALT * 4);
        for e in &EMBLEMAS {
            assert!(e.iter().all(|l| l.len() == 12));
        }
    }
}
