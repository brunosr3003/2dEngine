pub mod manager;

pub use manager::{AssetManager, ImageHandle};

use anyhow::Result;
use image::GenericImageView;
use std::collections::HashMap;

pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ImageData {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let img = image::load_from_memory(bytes)?;
        let (w, h) = img.dimensions();
        Ok(Self { width: w, height: h, rgba: img.into_rgba8().into_raw() })
    }
}

/// Sub-regiao de um atlas, em UVs normalizadas [0,1].
#[derive(Debug, Clone, Copy)]
pub struct AtlasEntry {
    pub uv_min: glam::Vec2,
    pub uv_max: glam::Vec2,
}

#[derive(Default)]
pub struct Atlas {
    pub image: Option<ImageData>,
    pub entries: HashMap<String, AtlasEntry>,
}

impl Atlas {
    pub fn new() -> Self { Self::default() }

    pub fn insert_pixels(&mut self, name: impl Into<String>, x: u32, y: u32, w: u32, h: u32) {
        let Some(img) = &self.image else { return };
        let (iw, ih) = (img.width as f32, img.height as f32);
        self.entries.insert(name.into(), AtlasEntry {
            uv_min: glam::Vec2::new(x as f32 / iw, y as f32 / ih),
            uv_max: glam::Vec2::new((x + w) as f32 / iw, (y + h) as f32 / ih),
        });
    }

    pub fn get(&self, name: &str) -> Option<&AtlasEntry> { self.entries.get(name) }
}
