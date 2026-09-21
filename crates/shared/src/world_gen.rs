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
    /// Se Some, substitui o spawn_tile() default (util em mapas craftados).
    pub override_spawn: Option<(i32, i32)>,
    /// Spawn por facção (tile coords no espaço do mapa). Setados pelo loader
    /// quando o map file marca as ilhas-sede. None = usa override_spawn/spawn_tile.
    pub morganeer_spawn: Option<(i32, i32)>,
    pub peacemain_spawn: Option<(i32, i32)>,
}

impl WorldMap {
    /// Tile de spawn para uma facção. Usa o override por facção se setado e
    /// walkable; senão cai no spawn_tile() padrão.
    pub fn faction_spawn_tile(&self, faction: crate::Faction) -> (i32, i32) {
        let pick = match faction {
            crate::Faction::Morganeers => self.morganeer_spawn,
            crate::Faction::Peacemain => self.peacemain_spawn,
        };
        if let Some((x, y)) = pick {
            if self.is_walkable(x, y) {
                return (x, y);
            }
        }
        self.spawn_tile()
    }

    pub fn get(&self, x: i32, y: i32) -> u16 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return tile_id::WALL;
        }
        self.tiles[(y as u32 * self.width + x as u32) as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, id: u16) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        self.tiles[(y as u32 * self.width + x as u32) as usize] = id;
    }

    /// Posicao de spawn padrao. Mapas craftados podem setar override_spawn.
    pub fn spawn_tile(&self) -> (i32, i32) {
        if let Some(s) = self.override_spawn {
            return s;
        }
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

    // `build_colliders` foi removido junto com o rapier: parede agora e' o
    // proprio tile, e quem resolve e' `move_and_slide`.

    pub fn move_and_slide(
        &self,
        pos: glam::Vec2,
        vel: glam::Vec2,
        dt: f32,
        radius: f32,
    ) -> glam::Vec2 {
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
            if self.check_collision(p, ext) {
                p.x = pos.x;
            } // Fallback de seguranca
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
            if self.check_collision(p, ext) {
                p.y = pos.y;
            } // Fallback de seguranca
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
                let t = self.get(tx, ty);
                if t == crate::constants::tile_id::WALL || t == crate::constants::tile_id::WATER {
                    return true;
                }
            }
        }
        false
    }

    // `is_water`/`is_navigable` sairam em 21/09/2026. Eram do mapa de TILES
    // velho, que nao sabe onde fica a agua de uma ilha — e agora que existe
    // mar navegavel elas seriam a resposta errada, em silencio, pra quem
    // perguntasse "da' pra navegar aqui?".
    //
    // "Isto e' agua?" tem UMA autoridade: `Ilha::agua` na ilha, `Mar::agua`
    // no mar aberto. Ambas leem o campo de altura, que e' o terreno de
    // verdade.

    /// True se o tile e' walkable a pe (FLOOR/DUNGEON_FLOOR/DIRT — qualquer
    /// nao-WALL e nao-WATER conta como walkable).
    pub fn is_walkable(&self, x: i32, y: i32) -> bool {
        let t = self.get(x, y);
        t != crate::constants::tile_id::WALL && t != crate::constants::tile_id::WATER
    }

    /// True se a linha de A→B nao cruza nenhum tile WALL.
    /// DDA por sub-tile (resolucao = 0.4 tile = ~2 amostras por tile).
    /// Usado por IA pra "ver" o player e por checagens de cobertura.
    pub fn has_line_of_sight(&self, a: glam::Vec2, b: glam::Vec2) -> bool {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.001 {
            return true;
        }
        // Adjacencia: < 2 tiles. Sem LOS — DDA falha em diagonais que
        // raspam canto de parede (t=0.5 cai exatamente em (n, n) com floor
        // batendo no tile wall, falsa-positivo). Em melee range o player
        // sempre alcanca alvo encostado.
        if dist < 2.0 {
            return true;
        }
        // 2.5 amostras por tile pra evitar pular esquinas finas.
        let steps = (dist * 2.5).ceil().max(1.0) as i32;
        let inv = 1.0 / steps as f32;
        // Pula start (i=0) e end (i=steps) — assume que emissor e alvo
        // estao em tiles validos. Sample em corner exato (px,py == int)
        // gerava falso-positivo bloqueando ataques adjacentes.
        for i in 1..steps {
            let t = i as f32 * inv;
            let px = a.x + dx * t;
            let py = a.y + dy * t;
            let tx = px.floor() as i32;
            let ty = py.floor() as i32;
            if self.get(tx, ty) == crate::constants::tile_id::WALL {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// LCG deterministico
// ---------------------------------------------------------------------------

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0xDEADBEEFCAFEBABE)
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % (hi - lo) as u64) as i32
    }
}

// ---------------------------------------------------------------------------
// Gerador principal
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Room {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Room {
    fn cx(&self) -> i32 {
        self.x + self.w / 2
    }
    fn cy(&self) -> i32 {
        self.y + self.h / 2
    }

    fn overlaps(&self, other: &Room) -> bool {
        self.x < other.x + other.w + 2
            && self.x + self.w + 2 > other.x
            && self.y < other.y + other.h + 2
            && self.y + self.h + 2 > other.y
    }
}

pub fn generate(seed: u64, width: u32, height: u32) -> WorldMap {
    let mut rng = Lcg::new(seed);
    let tiles = vec![tile_id::WALL; (width * height) as usize];
    let mut map = WorldMap {
        width,
        height,
        tiles,
        override_spawn: None,
        morganeer_spawn: None,
        peacemain_spawn: None,
    };

    let mut rooms: Vec<Room> = Vec::new();

    // Tenta colocar salas sem sobreposicao
    for _ in 0..500 {
        let rw = rng.range(10, 24);
        let rh = rng.range(8, 18);
        let rx = rng.range(2, width as i32 - rw - 2);
        let ry = rng.range(2, height as i32 - rh - 2);
        let room = Room {
            x: rx,
            y: ry,
            w: rw,
            h: rh,
        };

        if rooms.iter().any(|r| room.overlaps(r)) {
            continue;
        }

        carve_room(&mut map, &room, &mut rng);
        rooms.push(room);
        if rooms.len() >= 32 {
            break;
        }
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
        if i == j {
            continue;
        }
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
            let tile = if is_edge {
                tile_id::WALL
            } else {
                tile_id::DUNGEON_FLOOR
            };
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
            if is_edge {
                continue;
            } // deixa a borda como parede
            let t = if rng.range(0, 8) == 0 {
                tile_id::DIRT
            } else {
                tile_id::FLOOR
            };
            map.set(x, y, t);
        }
    }
}

fn carve_h_corridor(map: &mut WorldMap, x0: i32, x1: i32, y: i32) {
    let (lo, hi) = (x0.min(x1), x0.max(x1));
    for x in lo..=hi {
        for dy in -1..=1 {
            if map.get(x, y + dy) == tile_id::WALL {
                map.set(x, y + dy, tile_id::FLOOR);
            }
        }
    }
}

fn carve_v_corridor(map: &mut WorldMap, y0: i32, y1: i32, x: i32) {
    let (lo, hi) = (y0.min(y1), y0.max(y1));
    for y in lo..=hi {
        for dx in -1..=1 {
            if map.get(x + dx, y) == tile_id::WALL {
                map.set(x + dx, y, tile_id::FLOOR);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mapa pequeno "craftado" - hand-designed para demonstrar o tileset Gentle
// Forest. Segue os padroes dos exemplos: areas enclausuradas por cliff, pond
// com paredes, caminhos de dirt, sem inimigos.
// ---------------------------------------------------------------------------

/// Decoracao nao-tile posicionada em tile coords. O cliente renderiza como
/// sprite sobre o chao (nao bloqueia colisao aqui; server pode adicionar
/// collider a parte se desejar).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Decoration {
    /// Arvore grande (80x96 pixels = 5x6 tiles ocupando espaco visual).
    BigTree,
    /// Pedra decorativa (1 tile).
    Stone,
    /// Toco de arvore cortada (1 tile).
    Stump,
    /// Tronco caido horizontal (1 tile).
    Log,
    /// Cluster de flores (1 tile).
    Flowers,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DecoPlacement {
    pub kind: Decoration,
    /// Posicao em tile coords (pivot bottom-center do sprite).
    pub pos: [i32; 2],
}

pub struct CraftedMap {
    pub map: WorldMap,
    pub decorations: Vec<DecoPlacement>,
    /// Entidades importantes (vendor, vault) posicionadas.
    pub vendor_pos: (f32, f32),
    pub vault_pos: (f32, f32),
}

/// Constroi mapa crafted 32x32 baseado nos patterns do Gentle Forest.
///
/// Layout:
/// - Grama em todo o interior
/// - Cliff wall border (row 0 e 31; col 0 e 31)
/// - Plaza central com DIRT circular (spawn zone)
/// - Pond enclosure no canto superior-direito (water enclosed by cliff)
/// - Dungeon cave no canto inferior-esquerdo (dungeon_floor enclosed)
/// - Dirt paths conectando spawn → pond e spawn → dungeon
/// - Arvores + decoracoes espalhadas pelo mapa
pub fn build_crafted_map() -> CraftedMap {
    let w = 40u32;
    let h = 30u32;
    let mut map = WorldMap {
        width: w,
        height: h,
        tiles: vec![tile_id::FLOOR; (w * h) as usize],
        override_spawn: Some((20, 15)),
        morganeer_spawn: None,
        peacemain_spawn: None,
    };

    // Border de cliff walls
    let w_i = w as i32;
    let h_i = h as i32;
    for x in 0..w_i {
        map.set(x, 0, tile_id::WALL);
        map.set(x, h_i - 1, tile_id::WALL);
    }
    for y in 0..h_i {
        map.set(0, y, tile_id::WALL);
        map.set(w_i - 1, y, tile_id::WALL);
    }

    // Plaza central: circulo de dirt aprox 5x3 centrado em (20, 15)
    for dy in -1..=1 {
        for dx in -2..=2 {
            map.set(20 + dx, 15 + dy, tile_id::DIRT);
        }
    }
    // "orelhas" do circulo pra parecer organico
    map.set(18, 16, tile_id::DIRT);
    map.set(22, 16, tile_id::DIRT);
    map.set(18, 14, tile_id::DIRT);
    map.set(22, 14, tile_id::DIRT);

    // Pond encloure (cliff walls + water) - canto superior-direito
    // Rect (28..=34, 4..=9), borda = WALL, interior = WATER
    let (px0, py0, px1, py1) = (28, 4, 34, 9);
    for x in px0..=px1 {
        map.set(x, py0, tile_id::WALL);
        map.set(x, py1, tile_id::WALL);
    }
    for y in py0..=py1 {
        map.set(px0, y, tile_id::WALL);
        map.set(px1, y, tile_id::WALL);
    }
    for x in (px0 + 1)..px1 {
        for y in (py0 + 1)..py1 {
            map.set(x, y, tile_id::WATER);
        }
    }

    // Dungeon cave - canto inferior-esquerdo
    // Rect (5..=11, 20..=25), borda = WALL, interior = DUNGEON_FLOOR
    let (dx0, dy0, dx1, dy1) = (5, 20, 11, 25);
    for x in dx0..=dx1 {
        map.set(x, dy0, tile_id::WALL);
        map.set(x, dy1, tile_id::WALL);
    }
    for y in dy0..=dy1 {
        map.set(dx0, y, tile_id::WALL);
        map.set(dx1, y, tile_id::WALL);
    }
    for x in (dx0 + 1)..dx1 {
        for y in (dy0 + 1)..dy1 {
            map.set(x, y, tile_id::DUNGEON_FLOOR);
        }
    }
    // Entrada da dungeon: abre o wall em (8, 20) e coloca DIRT pra conectar
    map.set(8, 20, tile_id::DUNGEON_FLOOR);

    // Caminho de dirt: plaza (20, 15) → dungeon (8, 20)
    // Horizontal de x=9 a x=19 em y=17
    for x in 9..=19 {
        map.set(x, 17, tile_id::DIRT);
    }
    // Vertical de y=17 a y=19 em x=8
    for y in 17..=19 {
        map.set(8, y, tile_id::DIRT);
    }
    // Conecta plaza → path horizontal
    map.set(19, 16, tile_id::DIRT);

    // Caminho de dirt: plaza (20, 15) → pond (31, 6)
    // Vertical de y=7 a y=14 em x=25
    for y in 7..=14 {
        map.set(25, y, tile_id::DIRT);
    }
    // Horizontal de x=21 a x=25 em y=14
    for x in 21..=25 {
        map.set(x, 14, tile_id::DIRT);
    }
    // Conecta pond → path
    map.set(31, 10, tile_id::DIRT);
    map.set(30, 10, tile_id::DIRT);
    map.set(29, 10, tile_id::DIRT);
    map.set(28, 10, tile_id::DIRT);
    map.set(27, 10, tile_id::DIRT);
    map.set(26, 10, tile_id::DIRT);
    map.set(25, 10, tile_id::DIRT);

    // Decoracoes: arvores grandes em clusters + objetos espalhados.
    let decos = vec![
        // Arvores grandes
        DecoPlacement {
            kind: Decoration::BigTree,
            pos: [4, 6],
        },
        DecoPlacement {
            kind: Decoration::BigTree,
            pos: [10, 4],
        },
        DecoPlacement {
            kind: Decoration::BigTree,
            pos: [16, 5],
        },
        DecoPlacement {
            kind: Decoration::BigTree,
            pos: [22, 25],
        },
        DecoPlacement {
            kind: Decoration::BigTree,
            pos: [30, 22],
        },
        DecoPlacement {
            kind: Decoration::BigTree,
            pos: [35, 14],
        },
        DecoPlacement {
            kind: Decoration::BigTree,
            pos: [14, 24],
        },
        // Pedras
        DecoPlacement {
            kind: Decoration::Stone,
            pos: [6, 13],
        },
        DecoPlacement {
            kind: Decoration::Stone,
            pos: [14, 11],
        },
        DecoPlacement {
            kind: Decoration::Stone,
            pos: [28, 18],
        },
        DecoPlacement {
            kind: Decoration::Stone,
            pos: [33, 25],
        },
        // Tocos
        DecoPlacement {
            kind: Decoration::Stump,
            pos: [7, 8],
        },
        DecoPlacement {
            kind: Decoration::Stump,
            pos: [26, 23],
        },
        // Troncos
        DecoPlacement {
            kind: Decoration::Log,
            pos: [13, 8],
        },
        DecoPlacement {
            kind: Decoration::Log,
            pos: [31, 17],
        },
        // Flores
        DecoPlacement {
            kind: Decoration::Flowers,
            pos: [11, 13],
        },
        DecoPlacement {
            kind: Decoration::Flowers,
            pos: [17, 20],
        },
        DecoPlacement {
            kind: Decoration::Flowers,
            pos: [27, 12],
        },
        DecoPlacement {
            kind: Decoration::Flowers,
            pos: [34, 8],
        },
        DecoPlacement {
            kind: Decoration::Flowers,
            pos: [19, 24],
        },
    ];

    CraftedMap {
        map,
        decorations: decos,
        vendor_pos: (17.5, 11.5),
        vault_pos: (22.5, 11.5),
    }
}
