//! Gerador procedural de mundo — salas conectadas por corredores.
//!
//! Usa LCG deterministico (sem dependencia de rand). Com o mesmo seed
//! cliente e servidor geram o mesmo mapa — base para colisao server-side.
//!
//! IDs de tile definidos em `atlas_gen::tile_id` para manter sincronismo.

use crate::constants::tile_id;

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

    pub fn build_colliders(&self, physics: &mut crate::physics::PhysicsWorld) {
        use rapier2d::prelude::*;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x as i32, y as i32) == crate::constants::tile_id::WALL {
                    let collider = ColliderBuilder::cuboid(0.5, 0.5)
                        .translation([x as f32 + 0.5, y as f32 + 0.5].into())
                        .collision_groups(InteractionGroups::new(Group::GROUP_1, Group::GROUP_2, Default::default()))
                        .build();
                    physics.collider_set.insert(collider);
                }
            }
        }
    }

    pub fn move_and_slide(&self, pos: glam::Vec2, vel: glam::Vec2, dt: f32, radius: f32) -> glam::Vec2 {
        let mut p = pos;
        let v = vel * dt;
        let ext = radius * 0.95; // Leve shrink para não agarrar em quinas de tile

        // Eixo X
        p.x += v.x;
        if self.check_collision(p, ext) {
            p.x -= v.x; // rollback
            if v.x > 0.0 {
                p.x = (p.x + ext + v.x).floor() - ext - 0.001;
            } else if v.x < 0.0 {
                p.x = (p.x - ext + v.x).floor() + 1.0 + ext + 0.001;
            }
            if self.check_collision(p, ext) { p.x = pos.x; } // Fallback de seguranca
        }

        // Eixo Y
        p.y += v.y;
        if self.check_collision(p, ext) {
            p.y -= v.y; // rollback
            if v.y > 0.0 {
                p.y = (p.y + ext + v.y).floor() - ext - 0.001;
            } else if v.y < 0.0 {
                p.y = (p.y - ext + v.y).floor() + 1.0 + ext + 0.001;
            }
            if self.check_collision(p, ext) { p.y = pos.y; } // Fallback de seguranca
        }

        p
    }

    fn check_collision(&self, p: glam::Vec2, ext: f32) -> bool {
        let min_x = (p.x - ext).floor() as i32;
        let max_x = (p.x + ext).floor() as i32;
        let min_y = (p.y - ext).floor() as i32;
        let max_y = (p.y + ext).floor() as i32;

        for ty in min_y..=max_y {
            for tx in min_x..=max_x {
                if self.get(tx, ty) == crate::constants::tile_id::WALL {
                    return true;
                }
            }
        }
        false
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
    for _ in 0..500 {
        let rw = rng.range(10, 24);
        let rh = rng.range(8, 18);
        let rx = rng.range(2, width as i32 - rw - 2);
        let ry = rng.range(2, height as i32 - rh - 2);
        let room = Room { x: rx, y: ry, w: rw, h: rh };

        if rooms.iter().any(|r| room.overlaps(r)) { continue; }

        carve_room(&mut map, &room, &mut rng);
        rooms.push(room);
        if rooms.len() >= 32 { break; }
    }

    // Conecta salas consecutivas com corredor em L (mais largo — mundo mais aberto)
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

    // Conexoes extras: liga cada sala a outra aleatoria — menos becos sem saida
    let n = rooms.len();
    for i in 0..n {
        let j = rng.range(0, n as i32) as usize;
        if i == j { continue; }
        let a = rooms[i];
        let b = rooms[j];
        carve_h_corridor(&mut map, a.cx(), b.cx(), a.cy());
        carve_v_corridor(&mut map, a.cy(), b.cy(), b.cx());
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

    // Dungeon isolada no canto — sala retangular com tile distinto, sem corredor
    // conectando. So se chega via portal.
    carve_dungeon_room(&mut map, width, height);

    map
}

/// Posicao e tamanho da sala de dungeon. Determinista — outros modulos usam
/// essa funcao pra saber onde colocar portais e inimigos.
pub fn dungeon_room_rect(width: u32, height: u32) -> (i32, i32, i32, i32) {
    let w = 24i32;
    let h = 20i32;
    let x = (width as i32 - w - 4).max(2);
    let y = (height as i32 - h - 4).max(2);
    (x, y, w, h)
}

/// Centro da dungeon (ponto de spawn do portal de retorno).
pub fn dungeon_center(width: u32, height: u32) -> (i32, i32) {
    let (x, y, w, h) = dungeon_room_rect(width, height);
    (x + w / 2, y + h / 2)
}

fn carve_dungeon_room(map: &mut WorldMap, width: u32, height: u32) {
    let (x, y, w, h) = dungeon_room_rect(width, height);
    for dy in 0..h {
        for dx in 0..w {
            let is_edge = dx == 0 || dx == w - 1 || dy == 0 || dy == h - 1;
            let tile = if is_edge { tile_id::WALL } else { tile_id::DUNGEON_FLOOR };
            map.set(x + dx, y + dy, tile);
        }
    }
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
        for dy in -1..=1 {
            if map.get(x, y + dy) == tile_id::WALL { map.set(x, y + dy, tile_id::FLOOR); }
        }
    }
}

fn carve_v_corridor(map: &mut WorldMap, y0: i32, y1: i32, x: i32) {
    let (lo, hi) = (y0.min(y1), y0.max(y1));
    for y in lo..=hi {
        for dx in -1..=1 {
            if map.get(x + dx, y) == tile_id::WALL { map.set(x + dx, y, tile_id::FLOOR); }
        }
    }
}
