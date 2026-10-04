//! PORÃO FLOOR PLANS: hand-designed cellars with rooms, corridors and gates.
//!
//! The owner, 29/09/2026: "a cool designed island whit paths to go through and
//! you need to kill one mob to unlock a new area". Chosen afterwards: layouts
//! hand-designed per dungeon.
//!
//! ## Shape
//!
//! A plan is ROOMS (discs) joined by CORRIDORS (straight, axis-aligned strips
//! between two room centres). Walkable = inside a room, or inside an OPEN
//! corridor. Everything else is wall. Axis-aligned because the client draws the
//! walls as boxes, and a box can't be rotated.
//!
//! ## Progress
//!
//! Each fighting room has an `etapa` (the dungeon's `andar`): 0, 1, ... and the
//! boss room is `andares`. A corridor with `abre = k` is shut by an iron gate
//! until the run reaches `andar >= k` — i.e. until the Warden of the room of
//! step `k - 1` falls. `abre = 0` is always open (entrance, side alcoves).
//!
//! ## Where it lives
//!
//! Every Porão is CARVED into the Arena islet (`arena`): a mass of rock (or a
//! castle, a brick cellar, an ice hull, a sandstone tomb — its `Tema`) standing
//! on the islet's plain, with the rooms and corridors dug into it. The walls
//! are real terrain blocks, and the floor is real ground: what the player walks
//! on is what effects are drawn on.
//!
//! The owner, 30/09/2026, after the first version (a path of stone slabs laid
//! over the islet's grass): "the floor looks like just paint instead of real
//! blocks ... this island is verry poor ... i though in be something more
//! organic and visualy beauty ... like a real dungeun or castle".
//!
//! Each plan has its own spot on a ring around the islet's middle (`ancora`),
//! so the five never overlap; the Gruta floors keep the middle. Instances
//! don't see each other, so two runs of the same Porão share its halls.
//!
//! The relief (`bloco`), the ground paint and the wall stone all come from
//! here, through the `Gerador` both sides share; the plan's own collision
//! (`livre`) still holds the gates, which aren't terrain.

use glam::Vec2;

/// How far a cave's room edge wobbles in or out, in units.
pub const RUIDO_DA_SALA: f32 = 1.5;
/// The most a cave's rock bumps reach into a hall (`saliencia`), in units.
pub const SALIENCIA_MAX: f32 = 1.0;
/// How far the carved FLOOR runs past where a body may stand, in units. The
/// client sets a body on the highest block within its radius; with the
/// floor ending exactly at the collision edge, a wall block sat under the
/// body's side and the character was drawn on top of the wall.
const FOLGA_DO_CHAO: f32 = 0.6;

/// How far from the islet's middle the plans stand: past the Gruta floors
/// (sites near the middle, a 55 u floor around each) with room to spare.
pub const ANEL: f32 = 222.0;

/// How the cellar looks. Its walls, floor and the shape of its rock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tema {
    /// A wet sea cave: dark rock, uneven walls.
    Caverna,
    /// A smuggler's cellar: brick walls in courses, flagstone floor.
    Tijolo,
    /// The frozen hull: ice walls, snow on the floor.
    Gelo,
    /// A tomb in the sand: sandstone.
    Arenito,
    /// A castle vault: dressed stone, battlements on the wall tops.
    Castelo,
}

/// Corridor width, in world units. Five: room to fight in, and to be passed by
/// a mob without getting stuck on it.
pub const LARGURA: f32 = 5.0;

/// A room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sala {
    pub centro: Vec2,
    pub raio: f32,
    pub papel: Papel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Papel {
    /// Where the run starts and where the fallen come back. No mobs.
    Entrada,
    /// A fight: the mobs of step `n`, one of them the Warden.
    Luta(u8),
    /// The boss.
    Chefe,
    /// A dead end with nothing in it: the cellar isn't just a straight line.
    Recanto,
}

/// A corridor between rooms `a` and `b` (indices into `salas`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corredor {
    pub a: usize,
    pub b: usize,
    /// Open once the run reaches this `andar`. 0 = always open.
    pub abre: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct Planta {
    /// Content id of the Porão this is the plan of.
    pub conteudo: u16,
    /// Where on the Arena islet the plan's `(0, 0)` stands.
    pub ancora: Vec2,
    pub tema: Tema,
    pub salas: &'static [Sala],
    pub corredores: &'static [Corredor],
}

/// Every plan below is drawn at 2/3 of its size and scaled up by this. The
/// owner, 30/09/2026: "make it bigger and more mobs larger hordes" — a
/// bigger horde needs a bigger room to fight it in.
pub const ESCALA: f32 = 1.5;

const fn sala(x: f32, y: f32, raio: f32, papel: Papel) -> Sala {
    Sala {
        centro: Vec2::new(x * ESCALA, y * ESCALA),
        raio: raio * ESCALA,
        papel,
    }
}

const fn cor(a: usize, b: usize, abre: u8) -> Corredor {
    Corredor { a, b, abre }
}

use Papel::*;

/// Bump on any change to the plans' layout: it is in the server's height
/// cache key (`Ilha::carregar_ou_gerar_da_ilha`). 2: re-anchored 60° apart
/// for the Seraph Reliquary (02/10/2026).
pub const REVISAO: u32 = 4;

/// The plans. Every Porão has three fights and a boss (`andares = 3`).
pub const PLANTAS: [Planta; 8] = [
    // SHIPWRECK CELLAR — a serpentine: up, west, north, then east to the boss.
    // The alcove east of the first room is the hold of the wreck: empty, but
    // the cellar isn't a single line.
    Planta {
        conteudo: 1,
        ancora: Vec2::new(0.0, 222.0),
        tema: Tema::Caverna,
        salas: &[
            sala(0.0, -40.0, 6.0, Entrada),  // 0
            sala(0.0, -20.0, 9.0, Luta(0)),  // 1
            sala(-26.0, -20.0, 8.0, Luta(1)), // 2
            sala(-26.0, 6.0, 9.0, Luta(2)),  // 3
            sala(-26.0, 28.0, 4.0, Recanto), // 4: the bend before the boss
            sala(4.0, 28.0, 11.0, Chefe),    // 5
            sala(24.0, -20.0, 6.0, Recanto), // 6: the hold
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 1),
            cor(2, 3, 2),
            cor(3, 4, 3),
            cor(4, 5, 0),
            cor(1, 6, 0),
        ],
    },
    // SMUGGLER'S CELLAR — a hub. West first, then east, then north to the
    // boss: you keep coming back through the same hall. Each wing has a
    // stash room behind it.
    Planta {
        conteudo: 2,
        ancora: Vec2::new(-207.8, 120.0),
        tema: Tema::Tijolo,
        salas: &[
            sala(0.0, -38.0, 6.0, Entrada),   // 0
            sala(0.0, -14.0, 10.0, Luta(0)),  // 1: the hall
            sala(-28.0, -14.0, 8.0, Luta(1)), // 2: west wing
            sala(28.0, -14.0, 8.0, Luta(2)),  // 3: east wing
            sala(0.0, 22.0, 12.0, Chefe),     // 4
            sala(-28.0, 12.0, 6.0, Recanto),  // 5: west stash
            sala(28.0, 12.0, 6.0, Recanto),   // 6: east stash
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 1),
            cor(1, 3, 2),
            cor(1, 4, 3),
            cor(2, 5, 0),
            cor(3, 6, 0),
        ],
    },
    // FROZEN HULL — the length of a ship: bow to stern in a straight line,
    // cabins off the middle deck, and the boss below the stern.
    Planta {
        conteudo: 3,
        ancora: Vec2::new(-207.8, -120.0),
        tema: Tema::Gelo,
        salas: &[
            sala(-42.0, 0.0, 6.0, Entrada), // 0: the bow
            sala(-22.0, 0.0, 9.0, Luta(0)), // 1
            sala(0.0, 0.0, 9.0, Luta(1)),   // 2: middle deck
            sala(22.0, 0.0, 9.0, Luta(2)),  // 3
            sala(22.0, 26.0, 11.0, Chefe),  // 4: below the stern
            sala(0.0, 20.0, 6.0, Recanto),  // 5: port cabin
            sala(0.0, -20.0, 6.0, Recanto), // 6: starboard cabin
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 1),
            cor(2, 3, 2),
            cor(3, 4, 3),
            cor(2, 5, 0),
            cor(2, 6, 0),
        ],
    },
    // SUNKEN CARAVAN — tunnels that turn at every room, like a caravan route
    // buried in the sand, with an L-shaped dead end off the second room.
    Planta {
        conteudo: 4,
        ancora: Vec2::new(207.8, -120.0),
        tema: Tema::Arenito,
        salas: &[
            sala(-30.0, -34.0, 6.0, Entrada), // 0
            sala(-30.0, -8.0, 9.0, Luta(0)),  // 1
            sala(-4.0, -8.0, 9.0, Luta(1)),   // 2
            sala(-4.0, 20.0, 9.0, Luta(2)),   // 3
            sala(26.0, 20.0, 11.0, Chefe),    // 4
            sala(20.0, -8.0, 5.0, Recanto),   // 5: the bend of the dead end
            sala(20.0, -30.0, 6.0, Recanto),  // 6: its end
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 1),
            cor(2, 3, 2),
            cor(3, 4, 3),
            cor(2, 5, 0),
            cor(5, 6, 0),
        ],
    },
    // THUNDER VAULT — a cross. The antechamber opens west, then east, and only
    // then the long corridor north to the vault itself.
    Planta {
        conteudo: 5,
        ancora: Vec2::new(207.8, 120.0),
        tema: Tema::Castelo,
        salas: &[
            sala(0.0, -44.0, 6.0, Entrada),   // 0
            sala(0.0, -24.0, 9.0, Luta(0)),   // 1: antechamber
            sala(-26.0, -24.0, 8.0, Luta(1)), // 2
            sala(26.0, -24.0, 8.0, Luta(2)),  // 3
            sala(0.0, 16.0, 14.0, Chefe),     // 4: the vault
            sala(-26.0, 0.0, 6.0, Recanto),   // 5
            sala(26.0, 0.0, 6.0, Recanto),    // 6
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 1),
            cor(1, 3, 2),
            cor(1, 4, 3),
            cor(2, 5, 0),
            cor(3, 6, 0),
        ],
    },
    // SERAPH RELIQUARY (Skyreach) — a temple: the nave runs north from the
    // door, the aisles hold the first two fights, the crossing the third, and
    // the relics wait in the apse. The plans now sit 60° apart (they were five
    // at 72°): the Shipwreck Cellar keeps its old anchor at 222 — at 240 the
    // player stood a little past its entrance room's edge — the rest are on a
    // ring of 240.
    Planta {
        conteudo: 6,
        ancora: Vec2::new(-0.0, -240.0),
        tema: Tema::Castelo,
        salas: &[
            sala(0.0, -42.0, 6.0, Entrada),   // 0: the porch
            sala(0.0, -20.0, 8.0, Recanto),   // 1: the nave
            sala(-24.0, -20.0, 8.0, Luta(0)), // 2: west aisle
            sala(24.0, -20.0, 8.0, Luta(1)),  // 3: east aisle
            sala(0.0, 4.0, 10.0, Luta(2)),    // 4: the crossing
            sala(0.0, 30.0, 12.0, Chefe),     // 5: the apse
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 0),
            cor(1, 3, 1),
            cor(1, 4, 2),
            cor(4, 5, 3),
        ],
    },
    // ROBOT FOUNDRY (Kōgen-tō) — a factory floor: the loading dock, then the
    // assembly line running east in three bays, and the mainframe's hall at
    // the line's end. On an outer ring, 30° between the Shipwreck Cellar and
    // the Smugglers' plan (the islet grew for it: `arena::RAIO_BLOCOS`).
    Planta {
        conteudo: 7,
        ancora: Vec2::new(190.0, 329.1),
        tema: Tema::Tijolo,
        salas: &[
            sala(-36.0, 0.0, 6.0, Entrada),  // 0: the loading dock
            sala(-16.0, 0.0, 8.0, Luta(0)),  // 1: the first bay
            sala(-16.0, -22.0, 7.0, Recanto), // 2: the parts store
            sala(6.0, 0.0, 8.0, Luta(1)),    // 3: the second bay
            sala(28.0, 0.0, 9.0, Luta(2)),   // 4: the third bay
            sala(28.0, 26.0, 12.0, Chefe),   // 5: the mainframe's hall
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 0),
            cor(1, 3, 1),
            cor(3, 4, 2),
            cor(4, 5, 3),
        ],
    },
    // SUNKEN GALLEON (Abyssia) — the ship's length: the gun deck, the
    // captain's cabin aside, the hold in two flooded halves, and the
    // admiral's quarters at the stern. On the outer ring at 120°, mirroring
    // the Robot Foundry.
    Planta {
        conteudo: 19,
        ancora: Vec2::new(-190.0, 329.1),
        tema: Tema::Caverna,
        salas: &[
            sala(36.0, 0.0, 6.0, Entrada),   // 0: the breach in the hull
            sala(16.0, 0.0, 8.0, Luta(0)),   // 1: the gun deck
            sala(16.0, -22.0, 7.0, Recanto), // 2: the captain's cabin
            sala(-6.0, 0.0, 8.0, Luta(1)),   // 3: the forward hold
            sala(-28.0, 0.0, 9.0, Luta(2)),  // 4: the aft hold
            sala(-28.0, 26.0, 12.0, Chefe),  // 5: the admiral's quarters
        ],
        corredores: &[
            cor(0, 1, 0),
            cor(1, 2, 0),
            cor(1, 3, 1),
            cor(3, 4, 2),
            cor(4, 5, 3),
        ],
    },
];

/// The plan of this Porão, if it has one.
pub fn da(conteudo: u16) -> Option<&'static Planta> {
    PLANTAS.iter().find(|p| p.conteudo == conteudo)
}

/// Closest point of segment `a`-`b` to `p`.
fn no_segmento(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    a + ab * t
}

impl Planta {
    /// The room of step `andar` (the boss room once `andar` reaches the end).
    pub fn sala_da_etapa(&self, andar: u8) -> Option<usize> {
        self.salas
            .iter()
            .position(|s| s.papel == Luta(andar))
            .or_else(|| {
                (andar >= self.lutas())
                    .then(|| self.salas.iter().position(|s| s.papel == Chefe))
                    .flatten()
            })
    }

    /// How many fighting rooms (the content's `andares`).
    pub fn lutas(&self) -> u8 {
        self.salas
            .iter()
            .filter(|s| matches!(s.papel, Luta(_)))
            .count() as u8
    }

    pub fn entrada(&self) -> usize {
        self.salas
            .iter()
            .position(|s| s.papel == Entrada)
            .unwrap_or(0)
    }

    /// World position of a room's centre.
    pub fn centro(&self, sala: usize) -> Vec2 {
        self.ancora + self.salas[sala].centro
    }

    /// Where the run starts, and where the fallen get up: the entrance room
    /// for the first fight, the room of the step BEFORE for the others — the
    /// last room already cleared, one gate away from the fight.
    pub fn ponto_de_volta(&self, andar: u8) -> Vec2 {
        if andar == 0 {
            return self.centro(self.entrada());
        }
        self.sala_da_etapa(andar - 1)
            .map_or(self.centro(self.entrada()), |s| self.centro(s))
    }

    fn corredor_aberto(&self, c: &Corredor, andar: u8) -> bool {
        andar >= c.abre
    }

    /// The two ends of a corridor, in world coordinates.
    pub fn pontas(&self, c: &Corredor) -> (Vec2, Vec2) {
        (self.centro(c.a), self.centro(c.b))
    }

    /// The room's radius in the direction of `p`: an irregular edge, not a
    /// compass circle — the owner, 30/09/2026: "make the dungeun whit more
    /// noise instead of just a wall". Built cellars (castle, brick) keep
    /// round rooms; caves, ice and sand wobble by up to `RUIDO_DA_SALA`.
    ///
    /// Collision, routing and the carved relief all read THIS radius, so the
    /// edge the player bumps is the edge the terrain shows. A room stays
    /// star-shaped around its centre — every point of it sees the centre in a
    /// straight line — which is what keeps the routes (centre to centre, then
    /// straight to the destination) off the walls.
    pub fn raio_em(&self, sala: usize, p: Vec2) -> f32 {
        let s = &self.salas[sala];
        if self.construido() {
            return s.raio;
        }
        let v = p - (self.ancora + s.centro);
        let a = v.y.atan2(v.x);
        let fase = sala as f32 * 1.91 + self.conteudo as f32 * 0.73;
        let r = (3.0 * a + fase).sin() * 0.6 + (5.0 * a + fase * 2.3).sin() * 0.4;
        s.raio + RUIDO_DA_SALA * r
    }

    /// Can a body of radius `raio` stand at `p` (world) at this step?
    ///
    /// The cave's rock bumps (`saliencia`) count: a body stops at the rock's
    /// face, not at the hall's ideal edge behind it. Before 30/09/2026 they
    /// didn't, and a body could stand with the rock under its side — the
    /// client, which sets the body on the highest block under it, drew the
    /// character climbing the wall.
    pub fn livre(&self, p: Vec2, raio: f32, andar: u8) -> bool {
        let raio = raio + self.saliencia(p);
        self.salas
            .iter()
            .enumerate()
            .any(|(i, s)| (self.ancora + s.centro).distance(p) <= self.raio_em(i, p) - raio)
            || self.corredores.iter().any(|c| {
                if !self.corredor_aberto(c, andar) {
                    return false;
                }
                let (a, b) = self.pontas(c);
                no_segmento(p, a, b).distance(p) <= LARGURA * 0.5 - raio
            })
    }

    /// The walkable point closest to `p`.
    pub fn mais_perto(&self, p: Vec2, raio: f32, andar: u8) -> Vec2 {
        let mut melhor = self.centro(self.entrada());
        let mut dist = f32::MAX;
        let mut pesa = |q: Vec2| {
            let d = q.distance_squared(p);
            if d < dist {
                dist = d;
                melhor = q;
            }
        };
        for (i, s) in self.salas.iter().enumerate() {
            let c = self.ancora + s.centro;
            // A hair inside: exactly on the edge, float error calls it outside.
            // The noisy radius is taken in the direction of `p`, and the room
            // is star-shaped, so the clamped point is inside.
            let folga = (self.raio_em(i, p) - raio - SALIENCIA_MAX - 1e-3).max(0.0);
            pesa(c + (p - c).clamp_length_max(folga));
        }
        for c in self.corredores {
            if !self.corredor_aberto(c, andar) {
                continue;
            }
            let (a, b) = self.pontas(c);
            let eixo = no_segmento(p, a, b);
            let folga = (LARGURA * 0.5 - raio - SALIENCIA_MAX - 1e-3).max(0.0);
            pesa(eixo + (p - eixo).clamp_length_max(folga));
        }
        melhor
    }

    /// One step of movement against the walls: `de` wants to go to `para`.
    ///
    /// Free: goes. Blocked: slides — first onto the closest walkable point,
    /// then, if that isn't progress, along the wall at growing angles (the
    /// terrain's own `mover_com_degrau` does the same against a trunk).
    /// Already outside (a teleport, a spawn): comes back in.
    ///
    /// The angled tries are what gets a body round the INSIDE corner where a
    /// corridor meets a round room. Projecting alone stopped it dead there:
    /// the closest walkable point to a step across the corner is the body's
    /// own spot. With the dungeon auto that was a player frozen at the
    /// corridor mouth — where the gate stands — found by
    /// `uma_corrida_inteira_em_cada_porao`.
    pub fn mover(&self, de: Vec2, para: Vec2, raio: f32, andar: u8) -> Vec2 {
        if self.livre(para, raio, andar) {
            return para;
        }
        if !self.livre(de, raio, andar) {
            return self.mais_perto(de, raio, andar);
        }
        let v = para - de;
        let passo = v.length();
        if passo < 1e-6 {
            return de;
        }
        let deslizou = self.mais_perto(para, raio, andar);
        if deslizou.distance(de) <= passo + 0.05
            && (deslizou - de).dot(v) > passo * passo * 0.2
            && self.livre(deslizou, raio, andar)
        {
            return deslizou;
        }
        for graus in [20.0f32, 40.0, 60.0, 80.0] {
            for sinal in [1.0f32, -1.0] {
                let a = (graus * sinal).to_radians();
                let (s, c) = a.sin_cos();
                let dir = Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c);
                let q = de + dir * c;
                if self.livre(q, raio, andar) {
                    return q;
                }
            }
        }
        de
    }

    /// The rooms a point is in (a point in a corridor mouth is in two shapes).
    fn salas_de(&self, p: Vec2, andar: u8) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .salas
            .iter()
            .enumerate()
            .filter(|(i, s)| (self.ancora + s.centro).distance(p) <= self.raio_em(*i, p))
            .map(|(i, _)| i)
            .collect();
        if v.is_empty() {
            // In a corridor: both ends are reachable along it.
            for c in self.corredores {
                if !self.corredor_aberto(c, andar) {
                    continue;
                }
                let (a, b) = self.pontas(c);
                if no_segmento(p, a, b).distance(p) <= LARGURA * 0.5 {
                    v.push(c.a);
                    v.push(c.b);
                }
            }
        }
        v
    }

    /// A walking route from `de` to `para` through open corridors: the room
    /// centres in between, then the destination.
    ///
    /// A destination that can't be reached — behind a shut gate, inside the
    /// rock, off the plan — does NOT refuse the walk: the route goes to the
    /// reachable point closest to it. The owner, 30/09/2026, with the dungeon
    /// auto on: "the message is a gate is shut that way, but there is no
    /// longer a gate". The first version projected the destination onto the
    /// nearest room of the WHOLE plan, open or not, and when that room was
    /// behind a gate the walk was refused — with the way the player wanted
    /// wide open. `None` only when `de` itself is off the plan.
    ///
    /// Every leg is straight inside one convex-enough shape (a star-shaped
    /// room around its centre, or a strip whose axis runs between two room
    /// centres), so the route never touches a wall.
    pub fn caminho(&self, de: Vec2, para: Vec2, andar: u8) -> Option<Vec<Vec2>> {
        let r = crate::constants::ENTITY_RADIUS;
        let origens = self.salas_de(de, andar);
        if origens.is_empty() {
            return None;
        }
        // Every room reachable from here, and how (breadth-first: the plans
        // are small and the corridors all cost about the same).
        let n = self.salas.len();
        let mut veio: Vec<Option<usize>> = vec![None; n];
        let mut visto = vec![false; n];
        let mut fila = std::collections::VecDeque::new();
        for &o in &origens {
            visto[o] = true;
            fila.push_back(o);
        }
        while let Some(s) = fila.pop_front() {
            for c in self.corredores {
                if !self.corredor_aberto(c, andar) {
                    continue;
                }
                let outra = if c.a == s {
                    c.b
                } else if c.b == s {
                    c.a
                } else {
                    continue;
                };
                if !visto[outra] {
                    visto[outra] = true;
                    veio[outra] = Some(s);
                    fila.push_back(outra);
                }
            }
        }
        // The destination, brought into what is reachable.
        let alcancavel = |q: Vec2| self.salas_de(q, andar).iter().any(|&s| visto[s]);
        let (para, alvo) = if self.livre(para, r, andar) && alcancavel(para) {
            let alvo = *self.salas_de(para, andar).iter().find(|&&s| visto[s])?;
            (para, alvo)
        } else {
            // The reachable room whose floor comes closest to it.
            let (alvo, q) = (0..n)
                .filter(|&s| visto[s])
                .map(|s| {
                    let c = self.centro(s);
                    let folga = (self.raio_em(s, para) - r - SALIENCIA_MAX - 1e-3).max(0.0);
                    (s, c + (para - c).clamp_length_max(folga))
                })
                .min_by(|a, b| a.1.distance_squared(para).total_cmp(&b.1.distance_squared(para)))?;
            (q, alvo)
        };
        // Straight there when it's the same room and nothing is in the way;
        // otherwise room centre to room centre. A straight line from a
        // corridor into a room, or across a cave room's dents, can clip the
        // wall; a leg to a room's centre never does.
        let reto = |a: Vec2, b: Vec2| {
            (0..=24).all(|k| self.livre(a.lerp(b, k as f32 / 24.0), 0.0, andar))
        };
        let dentro: Vec<usize> = (0..n)
            .filter(|&i| self.centro(i).distance(de) <= self.raio_em(i, de))
            .collect();
        if dentro.contains(&alvo) && reto(de, para) {
            return Some(vec![para]);
        }
        let mut s = alvo;
        let mut salas = vec![s];
        while let Some(antes) = veio[s] {
            salas.push(antes);
            s = antes;
        }
        salas.reverse();
        let mut pontos: Vec<Vec2> = salas.iter().map(|&s| self.centro(s)).collect();
        pontos.push(para);
        // STRING-PULLING: skip every waypoint a straight, clear line reaches
        // past. Without it the route began at the centre of the room the body
        // is ALREADY in — behind it, often — and the dungeon auto, which asks
        // for a new route every second, walked back to that centre every
        // second and never got through the corridor: it looked like pushing
        // at the gate forever. Found by `uma_corrida_inteira_em_cada_porao`.
        // Sampled every 0.1 u, with a hair of clearance: the inside corner
        // where a corridor meets a room is a sliver a coarser check misses.
        let limpo = |a: Vec2, b: Vec2| {
            let passos = (a.distance(b) / 0.1).ceil().max(1.0) as usize;
            (0..=passos).all(|k| self.livre(a.lerp(b, k as f32 / passos as f32), r + 0.05, andar))
        };
        let mut rota = Vec::new();
        let mut atual = de;
        let mut i = 0;
        while i < pontos.len() {
            let mut j = pontos.len() - 1;
            while j > i && !limpo(atual, pontos[j]) {
                j -= 1;
            }
            rota.push(pontos[j]);
            atual = pontos[j];
            i = j + 1;
        }
        Some(rota)
    }

    /// Does the route from `de` actually END at `para` (and not at the
    /// reachable point closest to it)?
    pub fn alcanca(&self, de: Vec2, para: Vec2, andar: u8) -> bool {
        self.caminho(de, para, andar)
            .and_then(|r| r.last().copied())
            .is_some_and(|f| f.distance(para) < 0.01)
    }

    /// The gates still shut at this step: (where, facing along the corridor).
    /// Each sits at the mouth of the corridor, on the side of the room the run
    /// is coming from.
    pub fn portoes_fechados(&self, andar: u8) -> Vec<(Vec2, Vec2)> {
        self.corredores
            .iter()
            .filter(|c| !self.corredor_aberto(c, andar))
            .map(|c| {
                // The gate faces the side you arrive from: the room of the
                // earlier step (the entrance counts as the earliest).
                let ordem = |i: usize| match self.salas[i].papel {
                    Entrada => -1i32,
                    Luta(n) => n as i32,
                    Chefe => 99,
                    Recanto => 50,
                };
                let (de, para) = if ordem(c.a) <= ordem(c.b) {
                    (c.a, c.b)
                } else {
                    (c.b, c.a)
                };
                let (a, b) = (self.centro(de), self.centro(para));
                let dir = (b - a).normalize_or_zero();
                (a + dir * (self.salas[de].raio + 0.4), dir)
            })
            .collect()
    }

    /// The farthest walkable point from the anchor: what must fit on the
    /// islet's flat top.
    pub fn alcance(&self) -> f32 {
        let salas = self
            .salas
            .iter()
            .map(|s| s.centro.length() + s.raio + RUIDO_DA_SALA)
            .fold(0.0f32, f32::max);
        let corredores = self
            .corredores
            .iter()
            .map(|c| {
                self.salas[c.a]
                    .centro
                    .length()
                    .max(self.salas[c.b].centro.length())
                    + LARGURA * 0.5
            })
            .fold(0.0f32, f32::max);
        salas.max(corredores)
    }
}

// ───────────────────────────── the carved relief ─────────────────────────────

/// Wall height above the floor, in blocks: 2 u. The owner, 30/09/2026: "the
/// wall is too big it should be 3 or 4 steps". The walls don't have to hold
/// anyone in — the plan's collision (`livre`) does that — they have to read
/// as walls without hiding the fight.
pub const PAREDE_BLOCOS: i32 = 4;
/// The farthest the rock reaches out from a hall, in units (a cave's hill at
/// its thickest). Bounds `complexo_em`.
const MASSA: f32 = 14.0;
/// The widest the shore gets past the rock (`praia`).
const PRAIA_MAX: f32 = 7.0;

/// The plan whose rock covers `p`, if any. Cheap: one distance per plan.
pub fn complexo_em(p: Vec2) -> Option<&'static Planta> {
    PLANTAS
        .iter()
        .find(|pl| p.distance(pl.ancora) <= pl.alcance() + MASSA + PRAIA_MAX + 1.0)
}

/// Smooth pseudo-noise in 0..1 from two sines. Enough for a wall that isn't
/// a compass line; cheap enough to run per column on both sides.
fn onda(x: f32, z: f32, k: f32) -> f32 {
    let a = (x * 0.23 * k + 1.7 * (z * 0.11 * k).sin()).sin();
    let b = (z * 0.19 * k + 1.3 * (x * 0.13 * k).sin()).sin();
    0.5 + 0.25 * (a + b)
}

fn coluna(p: Vec2) -> (i32, i32) {
    (
        (p.x / crate::terreno::BLOCO).round() as i32,
        (p.y / crate::terreno::BLOCO).round() as i32,
    )
}

impl Planta {
    /// How deep inside the walkable area `p` is (negative = outside), all
    /// gates open: the SHAPE of the cellar, in units.
    pub fn profundidade(&self, p: Vec2) -> f32 {
        let salas = self
            .salas
            .iter()
            .enumerate()
            .map(|(i, s)| self.raio_em(i, p) - (self.ancora + s.centro).distance(p));
        let corredores = self.corredores.iter().map(|c| {
            let (a, b) = self.pontas(c);
            LARGURA * 0.5 - no_segmento(p, a, b).distance(p)
        });
        salas.chain(corredores).fold(f32::MIN, f32::max)
    }

    /// Rock pushed into the hall at this column, in units: the uneven face of
    /// a cave. Built walls are straight.
    fn saliencia(&self, p: Vec2) -> f32 {
        if self.construido() {
            0.0
        } else {
            SALIENCIA_MAX * onda(p.x, p.y, 1.3)
        }
    }

    /// A castle or a brick cellar is BUILT: thin walls hugging the halls. The
    /// others are dug into a hill of rock, ice or sand.
    fn construido(&self) -> bool {
        matches!(self.tema, Tema::Castelo | Tema::Tijolo)
    }

    /// How thick the rock (or the wall) is at `p`, in units, measured out
    /// from the hall. The outline follows the halls, so from outside the
    /// cellar reads as its own shape — not a disc.
    fn espessura(&self, p: Vec2) -> f32 {
        match self.tema {
            Tema::Castelo => 2.5,
            Tema::Tijolo => 3.0,
            _ => 6.0 + 7.5 * onda(p.x * 0.8 + 11.0, p.y * 0.8, 0.45),
        }
    }

    /// The top block of the column at `p` (world units), given the floor's
    /// block `chao`. `None` = open sea.
    ///
    /// Each Porão is its OWN ISLAND: the rock (or the walls) around the halls,
    /// then a rocky shore that drops into the sea. The owner, 30/09/2026:
    /// "this enormous island that exist now dont need to be" — the first
    /// carved version stood the cellars on a 300 u plain of grass and flowers.
    pub fn bloco(&self, p: Vec2, chao: i32) -> Option<i32> {
        let fundo = self.profundidade(p);
        if fundo >= self.saliencia(p) - FOLGA_DO_CHAO {
            return Some(chao);
        }
        let fora = -fundo; // how far into the rock, from the hall
        let espessura = self.espessura(p);
        let praia = self.praia(p);
        if fora > espessura + praia {
            return None;
        }
        if fora > espessura {
            // The shore: from the foot of the wall down into the water, over
            // broken rock.
            let t = crate::terreno::suave((fora - espessura) / praia);
            let topo = if self.construido() { chao - 1 } else { chao + 1 };
            let fundo_do_mar = -4;
            let degrau = ((onda(p.x * 1.7, p.y * 1.7, 1.0) - 0.5) * 3.0) as i32;
            return Some((topo as f32 + (fundo_do_mar - topo) as f32 * t).round() as i32 + degrau);
        }
        let (bx, bz) = coluna(p);
        let fino = onda(p.x * 2.3 + 7.0, p.y * 2.3 - 3.0, 1.0);
        let mut alto = chao + PAREDE_BLOCOS;
        match self.tema {
            // Jagged rock: a rolling top with a finer, sharper one on it.
            Tema::Caverna => {
                alto += (onda(p.x + 40.0, p.y - 17.0, 0.6) * 1.5 + fino * 1.5) as i32;
            }
            // Ice: mostly smooth, with spikes breaking through.
            Tema::Gelo => {
                alto += (onda(p.x + 40.0, p.y - 17.0, 0.6) * 1.5) as i32;
                if fino > 0.74 {
                    alto += ((fino - 0.74) * 12.0) as i32;
                }
            }
            // Sandstone: strata, two blocks at a time.
            Tema::Arenito => {
                alto += ((onda(p.x + 40.0, p.y - 17.0, 0.5) * 3.9) as i32 / 2) * 2;
            }
            // A castle that has seen sieges: collapsed stretches, and
            // battlements only where the wall still stands whole.
            Tema::Castelo => {
                let ruina = onda(p.x * 0.6 - 21.0, p.y * 0.6 + 5.0, 0.8);
                if ruina < 0.34 {
                    alto -= 1;
                } else if fora > espessura - 0.9 && (bx.div_euclid(2) + bz.div_euclid(2)) % 2 == 0 {
                    alto += 1;
                }
            }
            // Brick: uneven courses along the top, and a buttress every so
            // often standing a little taller.
            Tema::Tijolo => {
                alto += (fino * 1.5) as i32;
                if (bx.div_euclid(3) + bz.div_euclid(3)).rem_euclid(5) == 0 && fora > espessura - 1.0 {
                    alto += 1;
                }
            }
        }
        // A hill's outer side slopes down to the shore instead of being cut
        // with a knife.
        if !self.construido() {
            let encosta = espessura - fora;
            let pe = chao + 1;
            if encosta < 3.5 {
                alto = pe + ((alto - pe) as f32 * (encosta / 3.5)).round() as i32;
            }
        }
        Some(alto.max(chao))
    }

    /// How wide the shore is here, in units: rock falling into the sea.
    fn praia(&self, p: Vec2) -> f32 {
        3.0 + 4.0 * onda(p.x * 0.9 - 30.0, p.y * 0.9 + 12.0, 0.5)
    }

    /// Paint for the top of the column: the hall's floor, the top of the
    /// rock (bare stone on a cave's hill, snow on ice, sand on a tomb, stone
    /// on built walls), and wet rock and sand on the shore.
    pub fn pintura(&self, p: Vec2) -> Option<crate::terreno::Material> {
        use crate::terreno::Material as M;
        let (bx, bz) = coluna(p);
        let xadrez = (bx.div_euclid(2) + bz.div_euclid(2)) % 2 == 0;
        let n = onda(p.x, p.y, 2.0);
        let fundo = self.profundidade(p);
        // Construction materials continue across decks and surrounding walls.
        match self.conteudo {
            1 | 19 => return Some(if xadrez { M::Tronco } else { M::Terra }),
            2 => return Some(if xadrez { M::CalcadaEscura } else { M::Tronco }),
            6 => return Some(if xadrez { M::Marmore } else { M::MarmoreSombra }),
            7 => return Some(if xadrez { M::ConcretoEscuro } else { M::Concreto }),
            _ => {}
        }
        if fundo >= self.saliencia(p) - FOLGA_DO_CHAO {
            return Some(match self.tema {
                // Dark wet stone, lighter where the rock shows through.
                Tema::Caverna => if n > 0.64 { M::Rocha } else { M::RochaEscura },
                Tema::Tijolo => if xadrez { M::Calcada } else { M::CalcadaEscura },
                Tema::Gelo => if n > 0.6 { M::Gelo } else { M::Neve },
                Tema::Arenito => if xadrez { M::Arenito } else { M::Areia },
                Tema::Castelo => if xadrez { M::CalcadaEscura } else { M::Calcada },
            });
        }
        if -fundo > self.espessura(p) {
            // The shore.
            return Some(match self.tema {
                Tema::Gelo => if n > 0.55 { M::Gelo } else { M::Neve },
                Tema::Arenito => if n > 0.6 { M::Arenito } else { M::Areia },
                _ => if n > 0.58 { M::RochaEscura } else if n > 0.4 { M::Rocha } else { M::AreiaMolhada },
            });
        }
        Some(match self.tema {
            // Moss on the rock's top: from above, the halls (grey stone) have
            // to read apart from the walls around them.
            Tema::Caverna => if n > 0.66 { M::Rocha } else { M::GramaEscura },
            Tema::Tijolo => if n > 0.7 { M::GramaEscura } else { M::CalcadaEscura },
            Tema::Gelo => if n > 0.7 { M::Gelo } else { M::Neve },
            Tema::Arenito => if n > 0.62 { M::Arenito } else { M::Areia },
            Tema::Castelo => if n > 0.75 { M::GramaEscura } else { M::Calcada },
        })
    }

    /// Is `p` on top of the rock (not a hall, not the plain)? Plants grow
    /// there on a cave's hill.
    pub fn no_topo(&self, p: Vec2) -> bool {
        let fundo = self.profundidade(p);
        fundo < self.saliencia(p) - 0.8 && -fundo <= self.espessura(p) - 1.0
    }

    /// The stone of the wall face at column `(bx, bz)`, `prof` blocks under
    /// its top: brick courses and dressed stone alternate by row; a cave's
    /// hill keeps a band of soil under its grass.
    pub fn pedra(&self, bx: i32, bz: i32, prof: i32) -> crate::terreno::Material {
        use crate::terreno::Material as M;
        match self.conteudo {
            1 | 2 | 19 => return if prof % 4 == 0 { M::Terra } else { M::Tronco },
            6 => return if prof % 3 == 0 { M::MarmoreSombra } else { M::Marmore },
            7 => return if prof % 4 == 0 { M::Concreto } else { M::ConcretoEscuro },
            _ => {}
        }
        // A stable per-block roll: moss, cracks and stained stones scattered
        // over the face instead of clean stripes.
        let h = (bx as u32)
            .wrapping_mul(73_856_093)
            ^ (bz as u32).wrapping_mul(19_349_663)
            ^ (prof as u32).wrapping_mul(83_492_791);
        let sorte = (h >> 7) % 100;
        match self.tema {
            Tema::Caverna => {
                if sorte < 12 {
                    M::GramaEscura // moss
                } else if (bx + bz + prof) % 5 == 0 || sorte < 30 {
                    M::Rocha
                } else {
                    M::RochaEscura
                }
            }
            Tema::Tijolo => {
                if sorte < 8 {
                    M::GramaEscura
                } else if sorte < 18 {
                    M::Rocha
                } else if prof % 2 == 0 {
                    M::CalcadaEscura
                } else {
                    M::Terra
                }
            }
            Tema::Gelo => if sorte < 25 || prof % 3 == 2 { M::Neve } else { M::Gelo },
            Tema::Arenito => if sorte < 15 { M::Rocha } else if prof % 3 == 1 { M::Areia } else { M::Arenito },
            Tema::Castelo => {
                if sorte < 10 {
                    M::GramaEscura
                } else if sorte < 22 {
                    M::Calcada
                } else if prof % 2 == 0 {
                    M::Rocha
                } else {
                    M::RochaEscura
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::constants::ENTITY_RADIUS as R;

    /// EVERY PORÃO HAS A PLAN, AND THE PLAN AGREES WITH THE CONTENT.
    #[test]
    fn todo_porao_tem_planta_e_ela_bate_com_o_conteudo() {
        for c in crate::dungeon::CONTEUDOS
            .iter()
            .filter(|c| c.tipo == crate::dungeon::Tipo::Porao)
        {
            let p = da(c.id).unwrap_or_else(|| panic!("{} has no plan", c.nome));
            assert_eq!(
                p.lutas(),
                c.andares,
                "{}: the plan has {} fights and the content {} floors",
                c.nome,
                p.lutas(),
                c.andares
            );
            for n in 0..c.andares {
                assert!(p.sala_da_etapa(n).is_some(), "{}: no room for step {n}", c.nome);
            }
            assert!(
                p.salas.iter().filter(|s| s.papel == Chefe).count() == 1,
                "{}: one boss room",
                c.nome
            );
            assert!(
                p.salas.iter().filter(|s| s.papel == Entrada).count() == 1,
                "{}: one entrance",
                c.nome
            );
        }
    }

    /// CORRIDORS ARE STRAIGHT ALONG AN AXIS — the client draws walls as boxes.
    #[test]
    fn todo_corredor_e_reto_num_eixo() {
        for p in &PLANTAS {
            for c in p.corredores {
                let (a, b) = (p.salas[c.a].centro, p.salas[c.b].centro);
                assert!(
                    a.x == b.x || a.y == b.y,
                    "plan {}: corridor {}-{} is diagonal",
                    p.conteudo,
                    c.a,
                    c.b
                );
            }
        }
    }

    /// THE CELLARS STAND APART, CLEAR OF THE GRUTA FLOORS, AND ON DRY LAND.
    #[test]
    fn os_poroes_nao_se_encostam_e_cabem_na_ilhota() {
        let massa = |p: &Planta| p.alcance() + MASSA;
        for (i, a) in PLANTAS.iter().enumerate() {

            // Clear of the Gruta islet, with open sea between.
            assert!(
                a.ancora.length() - massa(a) - PRAIA_MAX >= crate::arena::RAIO_TERRA + 15.0,
                "plan {} touches the Gruta islet",
                a.conteudo
            );
            // And inside the zone's grid.
            let zona = crate::arena::RAIO_BLOCOS as f32 * crate::terreno::BLOCO;
            assert!(
                a.ancora.length() + massa(a) + PRAIA_MAX + 5.0 <= zona,
                "plan {} runs off the zone: anchor {:?}",
                a.conteudo,
                a.ancora
            );
            for b in PLANTAS.iter().skip(i + 1) {
                assert!(
                    a.ancora.distance(b.ancora) >= massa(a) + massa(b) + 2.0 * PRAIA_MAX + 10.0,
                    "plans {} and {} touch",
                    a.conteudo,
                    b.conteudo
                );
            }
        }
    }

    /// THE CARVED RELIEF AGREES WITH THE PLAN: every point a body may stand on
    /// is FLOOR, and a wall is a wall — the terrain never lets a body out where
    /// the plan says rock.
    #[test]
    fn o_relevo_escavado_bate_com_a_planta() {
        let chao = 31;
        for p in &PLANTAS {
            let alcance = p.alcance() + 3.0;
            let mut paredes = 0;
            let mut k = 0.0f32;
            while k < 1.0 {
                let mut j = 0.0f32;
                while j < 1.0 {
                    let q = p.ancora + Vec2::new((k - 0.5) * 2.0 * alcance, (j - 0.5) * 2.0 * alcance);
                    let Some(b) = p.bloco(q, chao) else {
                        assert!(!p.livre(q, 0.0, u8::MAX), "plan {}: walkable {q:?} outside the mass", p.conteudo);
                        j += 0.01;
                        continue;
                    };
                    // A body standing here has floor under ALL of it — every
                    // point the client samples for its height.
                    if p.livre(q, R, u8::MAX) {
                        for (dx, dz) in [(R, 0.0), (-R, 0.0), (0.0, R), (0.0, -R), (0.0, 0.0)] {
                            let pe = q + Vec2::new(dx, dz);
                            assert_eq!(
                                p.bloco(pe, chao),
                                Some(chao),
                                "plan {}: a body at {q:?} has wall under {pe:?}",
                                p.conteudo
                            );
                        }
                    }
                    // Just outside the rock's face (past the floor's margin,
                    // short of the mass's sloping outer rim): wall.
                    let fundo = p.profundidade(q) - p.saliencia(q);
                    if (-2.0..-(FOLGA_DO_CHAO + 0.4)).contains(&fundo) {
                        // At least three steps, even where a castle wall has
                        // half collapsed.
                        assert!(
                            b >= chao + 3,
                            "plan {}: the wall at {q:?} is {} blocks",
                            p.conteudo,
                            b - chao
                        );
                        paredes += 1;
                    }
                    j += 0.01;
                }
                k += 0.01;
            }
            assert!(paredes > 100, "plan {}: no walls measured", p.conteudo);
        }
    }

    /// THE GATES DO THEIR JOB: at the start the boss is unreachable, and once
    /// every step is done, the route runs from the entrance to the boss.
    ///
    /// And the order is the one designed: each gate opens the next fighting
    /// room and no other — skipping a room is not a shortcut the plan offers.
    #[test]
    fn os_portoes_seguram_a_ordem_das_salas() {
        for p in &PLANTAS {
            let entrada = p.centro(p.entrada());
            let chefe = p.centro(p.sala_da_etapa(p.lutas()).unwrap());
            assert!(
                !p.alcanca(entrada, chefe, 0),
                "plan {}: the boss is reachable at the start",
                p.conteudo
            );
            for andar in 0..=p.lutas() {
                let alvo = p.centro(p.sala_da_etapa(andar).unwrap());
                assert!(
                    p.alcanca(entrada, alvo, andar),
                    "plan {}: step {andar}'s room unreachable at step {andar}",
                    p.conteudo
                );
                if let Some(depois) = p.sala_da_etapa(andar + 1) {
                    if andar < p.lutas() {
                        assert!(
                            !p.alcanca(entrada, p.centro(depois), andar),
                            "plan {}: step {}'s room reachable before its gate",
                            p.conteudo,
                            andar + 1
                        );
                    }
                }
            }
        }
    }

    /// EVERY LEG OF A ROUTE IS WALKABLE — sampled along its length. A route
    /// that clips a wall would leave the body sliding on it forever.
    #[test]
    fn a_rota_nunca_raspa_na_parede() {
        for p in &PLANTAS {
            let fim = p.lutas();
            for (i, a) in p.salas.iter().enumerate() {
                for (j, b) in p.salas.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    let de = p.ancora + a.centro + Vec2::new(a.raio * 0.5, 0.0);
                    let para = p.ancora + b.centro - Vec2::new(0.0, b.raio * 0.5);
                    let rota = p.caminho(de, para, fim).expect("all open at the end");
                    let mut antes = de;
                    for ponto in rota {
                        for k in 0..=40 {
                            let q = antes.lerp(ponto, k as f32 / 40.0);
                            assert!(
                                p.livre(q, R, fim),
                                "plan {}: route {i}->{j} clips a wall at {q:?}",
                                p.conteudo
                            );
                        }
                        antes = ponto;
                    }
                }
            }
        }
    }

    /// NO TWO SHAPES TOUCH UNLESS A CORRIDOR JOINS THEM. Otherwise a wall
    /// thinner than a body separates two rooms — or none does, and the gate
    /// is decoration.
    #[test]
    fn paredes_separam_o_que_nao_e_ligado() {
        for p in &PLANTAS {
            for (i, a) in p.salas.iter().enumerate() {
                for (j, b) in p.salas.iter().enumerate().skip(i + 1) {
                    let ligadas = p
                        .corredores
                        .iter()
                        .any(|c| (c.a == i && c.b == j) || (c.a == j && c.b == i));
                    if ligadas {
                        continue;
                    }
                    let folga = a.centro.distance(b.centro) - a.raio - b.raio;
                    assert!(
                        folga >= 3.0,
                        "plan {}: rooms {i} and {j} are {folga:.1} u apart",
                        p.conteudo
                    );
                }
                for (k, c) in p.corredores.iter().enumerate() {
                    if c.a == i || c.b == i {
                        continue;
                    }
                    let (x, y) = (p.salas[c.a].centro, p.salas[c.b].centro);
                    let folga = no_segmento(a.centro, x, y).distance(a.centro)
                        - a.raio
                        - LARGURA * 0.5;
                    assert!(
                        folga >= 3.0,
                        "plan {}: room {i} is {folga:.1} u from corridor {k}",
                        p.conteudo
                    );
                }
            }
        }
    }

    /// MOVING INTO A WALL SLIDES ALONG IT, AND NEVER THROUGH IT.
    #[test]
    fn andar_contra_a_parede_desliza_e_nao_atravessa() {
        let p = da(1).unwrap();
        let c = p.centro(1);
        let raio = p.salas[1].raio;
        // Walking north from the room centre (no corridor that way): stops at
        // the wall, never beyond, sliding a little sideways as it pushes.
        let mut q = c;
        for _ in 0..400 {
            q = p.mover(q, q + Vec2::new(0.03, 0.15), R, 0);
            assert!(p.livre(q, R, 0), "left the walkable area at {q:?}");
        }
        // The edge in the direction it stopped (rooms wobble: `raio_em`).
        let borda = p.raio_em(1, q);
        assert!(q.distance(c) <= borda, "went through the wall: {q:?}");
        // It stops at the rock's FACE, which a cave's bumps push in by up to
        // `SALIENCIA_MAX` from the room's edge.
        assert!(q.distance(c) > borda - 1.0 - SALIENCIA_MAX, "didn't reach the wall: {q:?}");
        // A shut gate: walking west from room 1 into the gate to room 2 stops.
        let mut q = c;
        for _ in 0..400 {
            q = p.mover(q, q - Vec2::new(0.15, 0.0), R, 0);
        }
        assert!(q.x > c.x - raio - RUIDO_DA_SALA, "walked through the shut gate: {q:?}");
        // The same walk at step 1 (gate open) gets through.
        let mut q = c;
        for _ in 0..400 {
            q = p.mover(q, q - Vec2::new(0.15, 0.0), R, 1);
        }
        assert!(q.x < c.x - raio - RUIDO_DA_SALA - 5.0, "the open gate didn't let through: {q:?}");
        // Thrown outside (teleport): comes back in.
        let fora = p.ancora + Vec2::new(60.0, 60.0);
        assert!(p.livre(p.mover(fora, fora, R, 0), R, 0));
    }
}
#[cfg(test)]
mod testes_da_rota {
    use super::*;
    use crate::constants::ENTITY_RADIUS as R;

    /// ANY DESTINATION GETS A ROUTE, and the route stays on the floor: behind
    /// a shut gate, inside the rock, off the plan — the walk goes to the
    /// reachable point closest to it instead of being refused. Fuzzed over
    /// every plan and step, from every reachable spot.
    #[test]
    fn qualquer_destino_tem_rota_e_ela_nao_raspa_na_parede() {
        let mut semente = 12_345u64;
        let mut sorteio = || {
            semente = semente
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((semente >> 33) as f32) / (1u64 << 31) as f32
        };
        for p in &PLANTAS {
            let a = p.alcance() + 20.0;
            for andar in 0..=p.lutas() {
                let entrada = p.centro(p.entrada());
                let mut origens = Vec::new();
                while origens.len() < 60 {
                    let q = p.ancora + Vec2::new((sorteio() - 0.5) * 2.0 * a, (sorteio() - 0.5) * 2.0 * a);
                    if p.livre(q, R, andar) && p.alcanca(entrada, q, andar) {
                        origens.push(q);
                    }
                }
                for (k, de) in origens.iter().enumerate() {
                    // Anywhere at all, walkable or not.
                    let para = p.ancora + Vec2::new((sorteio() - 0.5) * 2.0 * a, (sorteio() - 0.5) * 2.0 * a);
                    let rota = p
                        .caminho(*de, para, andar)
                        .unwrap_or_else(|| panic!("plan {} step {andar}: no route from {de:?} ({k})", p.conteudo));
                    let mut antes = *de;
                    for ponto in rota {
                        for t in 0..=20 {
                            let q = antes.lerp(ponto, t as f32 / 20.0);
                            assert!(
                                p.livre(q, 0.0, andar),
                                "plan {} step {andar}: route {de:?} -> {para:?} leaves the floor at {q:?}",
                                p.conteudo
                            );
                        }
                        antes = ponto;
                    }
                }
            }
        }
    }
}
