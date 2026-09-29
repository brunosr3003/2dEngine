//! Client-side tile map.
//!
//! The server sends `MapChange` with `tiles` deliberately EMPTY: the real
//! archipelago has ~24M tiles, which would be ~72MB of JSON per login. The
//! Unity client worked around this by drawing the world from its own scene —
//! which is exactly the 100MB scene that stayed out of git.
//!
//! Here the map is DATA: the same `MapFile` the server loads, versionable and
//! with no giant binary scene. In time this becomes per-chunk streaming around
//! the player; for now the file is enough.

use shared::constants::tile_id;

pub struct Map {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<u16>,
}

impl Map {
    pub fn new(name: String, width: u32, height: u32, tiles: Vec<u16>) -> Self {
        Self {
            name,
            width,
            height,
            tiles,
        }
    }

    pub fn tile(&self, x: i32, y: i32) -> u16 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return tile_id::WALL;
        }
        // Checked against the vector's REAL length, and not only against
        // width/height: the server may send the dimensions with an empty array,
        // and network data never indexes blind.
        let i = (y as u32 * self.width + x as u32) as usize;
        self.tiles.get(i).copied().unwrap_or(tile_id::WALL)
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Fills the tiles from the `MapFile` on disk. Used when the server sends
    /// an empty array.
    pub fn fill_from_disk(&mut self) -> bool {
        // The file comes from the NAME of the zone the server sent. Each zone runs
        // in its own process with its own map; always loading the same file showed
        // the field while the player was in the city.
        let path =
            std::env::var("MMO_MAP").unwrap_or_else(|_| format!("data/maps/{}.json", self.name));
        match shared::mapfile::MapFile::load(&path) {
            Ok(mf) => {
                println!("[map] loaded from {path}: {}x{}", mf.width, mf.height);
                self.width = mf.width;
                self.height = mf.height;
                self.tiles = mf.tiles;
                true
            }
            Err(e) => {
                eprintln!("[map] {path}: {e}");
                false
            }
        }
    }
}
