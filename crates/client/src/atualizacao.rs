//! Checks the published release per platform before login, without stalling the network.
use macroquad::prelude::*;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

pub const BUILD: u64 = 2026100116;
const SITE: &str = "https://mmo.brunji.com.br/#downloads";

pub fn protocolo_incompativel(mensagem: &str) -> bool {
    mensagem.to_ascii_lowercase().contains("protocol mismatch")
}

pub struct Atualizacao {
    busca: Option<Receiver<Result<String, String>>>,
    inicio: Instant,
    aviso: Option<String>,
    dispensado: bool,
    erro: bool,
}

impl Default for Atualizacao {
    fn default() -> Self {
        Self {
            busca: Some(crate::api::buscar_release()),
            inicio: Instant::now(),
            aviso: None,
            dispensado: false,
            erro: false,
        }
    }
}

fn plataforma() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

fn destino(corpo: &str, plataforma: &str, build: u64) -> Option<String> {
    let manifesto: serde_json::Value = serde_json::from_str(corpo).ok()?;
    let release = manifesto.get("platforms")?.get(plataforma)?;
    if release.get("build")?.as_u64()? <= build {
        return None;
    }
    if !matches!(plataforma, "android" | "ios") {
        return Some(SITE.into());
    }
    let url = release.get("update_url")?.as_str()?;
    if plataforma == "ios" && url == "itms-beta://" {
        return Some(url.into());
    }
    // Do not forward arbitrary commands/schemes received over the network.
    [
        "https://play.google.com/",
        "https://apps.apple.com/",
        "https://testflight.apple.com/",
        "https://mmo.brunji.com.br/",
    ]
    .iter()
    .any(|prefixo| url.starts_with(prefixo))
    .then(|| url.to_string())
}

impl Atualizacao {
    /// Mandatory connection error: works even with no manifest or after
    /// dismissing the optional notice. In this flow the destination is always the site.
    pub fn desenha_incompativel(&mut self) -> bool {
        let r = crate::ui::painel((screen_width() - 24.0).min(560.0), 340.0, "Update required");
        let cx = r.center().x;
        crate::ui::texto_centro(cx, r.y + 20.0,
            "Your version is incompatible with the server.", 16, crate::ui::OURO);
        crate::ui::texto_centro(cx, r.y + 44.0,
            "Update the game from the site to continue.", 16, crate::ui::OURO);
        if crate::ui::botao(Rect::new(r.x, r.y + 68.0, r.w, 44.0),
            "Open the site to update", true) {
            self.erro = !crate::nativo::abrir_url(SITE);
        }
        crate::ui::texto_centro(cx, r.y + 138.0, "mmo.brunji.com.br", 16, crate::ui::OURO);
        if self.erro {
            crate::ui::erro(cx, r.y + 164.0, "Could not open the browser.");
        }
        crate::ui::botao(Rect::new(r.x, r.y + 186.0, r.w, 44.0), "Back", true)
    }

    /// `true`: the update screen took login's place this frame.
    pub fn desenha(&mut self) -> bool {
        if let Some(rx) = &self.busca {
            match rx.try_recv() {
                Ok(Ok(corpo)) => {
                    self.aviso = destino(&corpo, plataforma(), BUILD);
                    self.busca = None;
                }
                Ok(Err(_)) | Err(TryRecvError::Disconnected) => self.busca = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        // Even someone on auto-login sees the notice. A failure or timeout on the
        // query does not prevent playing; the network thread may finish later.
        if self.busca.is_some() && self.inicio.elapsed() < Duration::from_secs(3) {
            let r = crate::ui::painel(460.0, 140.0, "Tempest");
            crate::ui::texto_centro(
                r.center().x,
                r.y + 45.0,
                "Checking for updates...",
                18,
                crate::ui::OURO,
            );
            return true;
        }
        let Some(url) = self.aviso.as_ref().filter(|_| !self.dispensado) else {
            return false;
        };
        let r = crate::ui::painel(460.0, 300.0, "Client out of date");
        crate::ui::texto_centro(
            r.center().x,
            r.y + 35.0,
            "A new version of Tempest is available.",
            16,
            crate::ui::OURO,
        );
        let botao = if url.starts_with("https://play.google.com/")
            || url.starts_with("https://apps.apple.com/")
        {
            "Update in the store"
        } else if url == "itms-beta://" || url.starts_with("https://testflight.apple.com/") {
            "Open TestFlight"
        } else if plataforma() == "android" {
            "Download update"
        } else {
            "Open the site to update"
        };
        if crate::ui::botao(
            Rect::new(r.x + 20.0, r.y + 70.0, r.w - 40.0, 44.0),
            botao,
            true,
        ) {
            self.erro = !crate::nativo::abrir_url(url);
        }
        if self.erro {
            crate::ui::erro(
                r.center().x,
                r.y + 140.0,
                "Open mmo.brunji.com.br in your browser.",
            );
        }
        if crate::ui::botao(
            Rect::new(r.x + 20.0, r.y + 165.0, r.w - 40.0, 40.0),
            "Not now",
            true,
        ) {
            self.dispensado = true;
        }
        true
    }
}

#[cfg(debug_assertions)]
pub async fn previa_incompativel() {
    let mut aviso = Atualizacao {
        busca: None, inicio: Instant::now(), aviso: None, dispensado: true, erro: false,
    };
    for (w, h) in [(960, 540), (640, 360)] {
        request_new_screen_size(w as f32, h as f32);
        for _ in 0..3 { next_frame().await; }
        let rt = render_target(screen_width() as u32, screen_height() as u32);
        crate::render3d::define_alvo(Some(rt.clone()));
        for erro in [false, true] {
            aviso.erro = erro;
            for _ in 0..3 {
                crate::render3d::camera_padrao();
                crate::ui::fundo();
                aviso.desenha_incompativel();
                unsafe { get_internal_gl().flush() };
                rt.texture.get_texture_data().export_png(&format!("/tmp/tempest-protocolo-{w}-{erro}.png"));
                next_frame().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detecta_build_nova_sem_mudar_protocolo_e_respeita_plataforma() {
        let m = r#"{"protocol":147,"platforms":{"windows":{"build":12},"android":{"build":10,"update_url":"https://play.google.com/store/apps/details?id=com.brunji.tempest"},"ios":{"build":12,"update_url":"https://testflight.apple.com/join/teste"}}}"#;
        assert_eq!(destino(m, "windows", 11).as_deref(), Some(SITE));
        assert!(destino(m, "windows", 12).is_none());
        assert!(destino(m, "windows", 13).is_none());
        assert!(destino(m, "android", 11).is_none());
        assert!(destino(m, "linux", 11).is_none());
        assert!(destino(m, "android", 9)
            .unwrap()
            .starts_with("https://play.google.com/"));
        assert!(destino(m, "ios", 11)
            .unwrap()
            .starts_with("https://testflight.apple.com/"));
    }
    #[test]
    fn manifesto_ausente_ou_link_invalido_nao_bloqueia_login() {
        for m in [
            "",
            "<html>erro</html>",
            "{}",
            r#"{"platforms":{"android":{"build":99,"update_url":"javascript:alert(1)"}}}"#,
        ] {
            assert!(destino(m, "android", 1).is_none());
        }
    }
}
