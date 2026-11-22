//! CLAN FLAGS on the client (docs/TERRITORIOS.md): the flags the sea sent
//! (`ServerMessage::Bandeiras`) and their textures, made once per flag and
//! kept — for the islets' poles, the ships' mainmasts, the world map and the
//! clan panel's editor.

use macroquad::prelude::*;
use shared::bandeira::{Bandeira, BandeiraDoCla, ALT, LARG};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static LISTA: RefCell<Vec<BandeiraDoCla>> = const { RefCell::new(Vec::new()) };
    /// (clan id, preset, has image) -> texture: a changed flag makes a new key.
    static TEXTURAS: RefCell<HashMap<(i64, u32, bool), Texture2D>> = RefCell::new(HashMap::new());
    static PRESETS: RefCell<HashMap<u32, Texture2D>> = RefCell::new(HashMap::new());
}

pub fn define(lista: Vec<BandeiraDoCla>) {
    LISTA.with(|l| *l.borrow_mut() = lista);
}

fn textura_de(pixels: &[u8]) -> Texture2D {
    let t = Texture2D::from_rgba8(LARG as u16, ALT as u16, pixels);
    t.set_filter(FilterMode::Nearest);
    t
}

fn textura_da(b: &BandeiraDoCla) -> Texture2D {
    let chave = (b.cla_id, b.preset, b.imagem.is_some());
    TEXTURAS.with(|t| {
        t.borrow_mut()
            .entry(chave)
            .or_insert_with(|| textura_de(&b.pixels()))
            .clone()
    })
}

/// The flag of clan `cla_id`.
pub fn do_cla(cla_id: i64) -> Option<Texture2D> {
    let b = LISTA.with(|l| l.borrow().iter().find(|b| b.cla_id == cla_id).cloned())?;
    Some(textura_da(&b))
}

/// The flag of the clan with tag `tag` ("" = none).
pub fn da_tag(tag: &str) -> Option<Texture2D> {
    if tag.is_empty() {
        return None;
    }
    let b = LISTA.with(|l| l.borrow().iter().find(|b| b.tag == tag).cloned())?;
    Some(textura_da(&b))
}

/// An approved image's texture (the clan panel), cached per clan and image.
pub fn da_imagem(cla_id: i64, pixels: &[u8]) -> Texture2D {
    thread_local! {
        static IMAGENS: RefCell<HashMap<(i64, u64), Texture2D>> = RefCell::new(HashMap::new());
    }
    // A cheap fingerprint: a new upload makes a new texture.
    let marca = pixels
        .iter()
        .step_by(7)
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x1000_0000_01b3));
    IMAGENS.with(|t| {
        t.borrow_mut()
            .entry((cla_id, marca))
            .or_insert_with(|| {
                if pixels.len() == LARG * ALT * 4 {
                    textura_de(pixels)
                } else {
                    textura_de(&Bandeira::do_cla(cla_id).pixels())
                }
            })
            .clone()
    })
}

/// A preset's texture (the editor's preview).
pub fn do_preset(v: u32) -> Texture2D {
    PRESETS.with(|p| {
        p.borrow_mut()
            .entry(v)
            .or_insert_with(|| textura_de(&Bandeira::desempacota(v).pixels()))
            .clone()
    })
}
