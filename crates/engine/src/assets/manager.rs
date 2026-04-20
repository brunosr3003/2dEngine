//! Asset manager: cache central de recursos CPU-side.
//!
//! Separa carregamento (CPU) de upload GPU (Renderer::set_atlas).
//! Assets sao identificados por chave string e armazenados em Arc para
//! clone barato entre sistemas.
//!
//! Fluxo:
//! ```ignore
//! assets.load_image("atlas", include_bytes!("../assets/atlas.png"))?;
//! let img = assets.image("atlas").unwrap();
//! renderer.set_atlas(&img.rgba, img.width, img.height)?;
//! ```

use crate::assets::ImageData;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Handle leve para um asset (clone = incrementa refcount, sem copia dos dados).
pub type ImageHandle = Arc<ImageData>;

#[derive(Default)]
pub struct AssetManager {
    images: HashMap<String, ImageHandle>,
    /// Bytes brutos (audio, fontes, etc.) indexados por chave.
    raw: HashMap<String, Arc<Vec<u8>>>,
}

impl AssetManager {
    pub fn new() -> Self { Self::default() }

    // --- Imagens ---

    /// Carrega PNG dos `bytes` e armazena com `key`. Sobrescreve se ja existe.
    pub fn load_image(&mut self, key: impl Into<String>, bytes: &[u8]) -> Result<ImageHandle> {
        let img = ImageData::from_bytes(bytes)?;
        let handle = Arc::new(img);
        self.images.insert(key.into(), handle.clone());
        Ok(handle)
    }

    /// Armazena `ImageData` ja construido (util para texturas geradas em codigo).
    pub fn insert_image(&mut self, key: impl Into<String>, img: ImageData) -> ImageHandle {
        let handle = Arc::new(img);
        self.images.insert(key.into(), handle.clone());
        handle
    }

    pub fn image(&self, key: &str) -> Option<ImageHandle> {
        self.images.get(key).cloned()
    }

    pub fn has_image(&self, key: &str) -> bool { self.images.contains_key(key) }

    // --- Bytes brutos (audio, shaders, etc.) ---

    pub fn load_raw(&mut self, key: impl Into<String>, bytes: Vec<u8>) -> Arc<Vec<u8>> {
        let arc = Arc::new(bytes);
        self.raw.insert(key.into(), arc.clone());
        arc
    }

    pub fn raw(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.raw.get(key).cloned()
    }

    // --- Utilitarios ---

    pub fn unload_image(&mut self, key: &str) { self.images.remove(key); }
    pub fn unload_raw(&mut self, key: &str) { self.raw.remove(key); }

    /// Total de imagens em cache.
    pub fn image_count(&self) -> usize { self.images.len() }
}
