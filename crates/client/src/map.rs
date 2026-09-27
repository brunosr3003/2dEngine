//! Mapa de tiles do lado do cliente.
//!
//! O servidor manda `MapChange` com `tiles` VAZIO de proposito: o arquipelago
//! real tem ~24M tiles, o que daria ~72MB de JSON por login. O cliente Unity
//! contornava isso desenhando o mundo a partir da propria cena — que e'
//! justamente a cena de 100MB que ficou fora do git.
//!
//! Aqui o mapa e' DADO: o mesmo `MapFile` que o servidor carrega, versionavel
//! e sem cena binaria gigante. A prazo isto vira streaming por chunk em volta
//! do player; por ora o arquivo basta.

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
        // Checagem contra o tamanho REAL do vetor, e nao so' contra
        // width/height: o servidor pode mandar as dimensoes com o array vazio,
        // e dado de rede nunca indexa as cegas.
        let i = (y as u32 * self.width + x as u32) as usize;
        self.tiles.get(i).copied().unwrap_or(tile_id::WALL)
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Preenche os tiles a partir do `MapFile` em disco. Usado quando o
    /// servidor manda o array vazio.
    pub fn fill_from_disk(&mut self) -> bool {
        // O arquivo vem do NOME da zona que o servidor mandou. Cada zona roda
        // em processo proprio com mapa proprio; carregar sempre o mesmo
        // arquivo mostrava o campo enquanto o jogador estava na cidade.
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
