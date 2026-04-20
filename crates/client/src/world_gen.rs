//! Gerador procedural de mundo — salas conectadas por corredores.
//!
//! Usa LCG deterministico (sem dependencia de rand). Com o mesmo seed
//! cliente e servidor geram o mesmo mapa — base para colisao server-side.
//!
//! IDs de tile definidos em `atlas_gen::tile_id` para manter sincronismo.

use crate::atlas_gen::tile_id;

pub struct WorldMap {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<u16>,
}

impl WorldMap {
    pub fn get(&self, x: i32, y: i32) -> u16 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return tile_id::WALL;
        }
        self.tiles[(y as u32 * self.width + x as u32) as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, id: u16) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 { return; }
        self.tiles[(y as u32 * self.width + x as u32) as usize] = id;
    }

    /// Posicao de spawn padrao (centro da primeira sala carregada externamente,
    /// ou 0,0 como fallback). O gerador preenche `spawn_tile` apos gerar.
    pub fn spawn_tile(&self) -> (i32, i32) {
        // procura primeiro tile de chao
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                if self.get(x, y) == tile_id::FLOOR {
                    return (x, y);
                }
            }
        }
        (self.width as i32 / 2, self.height as i32 / 2)
    }
}

// ---------------------------------------------------------------------------
// LCG deterministico
// ---------------------------------------------------------------------------

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self { Self(seed ^ 0xDEADBEEFCAFEBABE) }

    fn next(&mut self) -> u64 {
        self.0 = self.0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo { return lo; }
        lo + (self.next() % (hi - lo) as u64) as i32
    }
}

// ---------------------------------------------------------------------------
// Gerador principal
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Room { x: i32, y: i32, w: i32, h: i32 }

impl Room {
    fn cx(&self) -> i32 { self.x + self.w / 2 }
    fn cy(&self) -> i32 { self.y + self.h / 2 }

    fn overlaps(&self, other: &Room) -> bool {
        self.x < other.x + other.w + 2
            && self.x + self.w + 2 > other.x
            && self.y < other.y + other.h + 2
            && self.y + self.h + 2 > other.y
    }
}

pub fn generate(seed: u64, width: u32, height: u32) -> WorldMap {
    let mut rng = Lcg::new(seed);
    let mut tiles = vec![tile_id::WALL; (width * height) as usize];
    let mut map = WorldMap { width, height, tiles };

    let mut rooms: Vec<Room> = Vec::new();

    // Tenta colocar salas sem sobreposicao
    for _ in 0..200 {
        let rw = rng.range(6, 16);
        let rh = rng.range(5, 12);
        let rx = rng.range(2, width as i32 - rw - 2);
        let ry = rng.range(2, height as i32 - rh - 2);
        let room = Room { x: rx, y: ry, w: rw, h: rh };

        if rooms.iter().any(|r| room.overlaps(r)) { continue; }

        carve_room(&mut map, &room, &mut rng);
        rooms.push(room);
        if rooms.len() >= 20 { break; }
    }

    // Conecta salas consecutivas com corredor em L
    for i in 1..rooms.len() {
        let a = rooms[i - 1];
        let b = rooms[i];
        if rng.range(0, 2) == 0 {
            carve_h_corridor(&mut map, a.cx(), b.cx(), a.cy());
            carve_v_corridor(&mut map, a.cy(), b.cy(), b.cx());
        } else {
            carve_v_corridor(&mut map, a.cy(), b.cy(), a.cx());
            carve_h_corridor(&mut map, a.cx(), b.cx(), b.cy());
        }
    }

    // Adiciona algumas celulas de agua decorativas
    for _ in 0..8 {
        let rx = rng.range(2, width as i32 - 4);
        let ry = rng.range(2, height as i32 - 4);
        let rw = rng.range(2, 5);
        let rh = rng.range(2, 4);
        for dy in 0..rh {
            for dx in 0..rw {
                if map.get(rx + dx, ry + dy) == tile_id::FLOOR {
                    map.set(rx + dx, ry + dy, tile_id::WATER);
                }
            }
        }
    }

    map
}

fn carve_room(map: &mut WorldMap, r: &Room, rng: &mut Lcg) {
    for dy in 0..r.h {
        for dx in 0..r.w {
            let x = r.x + dx;
            let y = r.y + dy;
            let is_edge = dx == 0 || dx == r.w - 1 || dy == 0 || dy == r.h - 1;
            if is_edge { continue; } // deixa a borda como parede
            let t = if rng.range(0, 8) == 0 { tile_id::DIRT } else { tile_id::FLOOR };
            map.set(x, y, t);
        }
    }
}

fn carve_h_corridor(map: &mut WorldMap, x0: i32, x1: i32, y: i32) {
    let (lo, hi) = (x0.min(x1), x0.max(x1));
    for x in lo..=hi {
        if map.get(x, y) == tile_id::WALL     { map.set(x, y, tile_id::FLOOR); }
        if map.get(x, y - 1) == tile_id::WALL { map.set(x, y - 1, tile_id::FLOOR); }
    }
}

fn carve_v_corridor(map: &mut WorldMap, y0: i32, y1: i32, x: i32) {
    let (lo, hi) = (y0.min(y1), y0.max(y1));
    for y in lo..=hi {
        if map.get(x, y) == tile_id::WALL     { map.set(x, y, tile_id::FLOOR); }
        if map.get(x + 1, y) == tile_id::WALL { map.set(x + 1, y, tile_id::FLOOR); }
    }
}
