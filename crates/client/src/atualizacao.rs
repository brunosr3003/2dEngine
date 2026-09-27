//! Verifica a release publicada por plataforma antes do login, sem travar a rede.
use macroquad::prelude::*;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

pub const BUILD: u64 = 2026092701;
const SITE: &str = "https://mmo.brunji.com.br/#downloads";

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
    // Não encaminhar comandos/esquemas arbitrários recebidos pela rede.
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
    /// `true`: a tela de atualização tomou o lugar do login neste quadro.
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
        // Inclusive quem usa login automático vê o aviso. Falha/timeout na
        // consulta não impede jogar; a thread da rede pode terminar depois.
        if self.busca.is_some() && self.inicio.elapsed() < Duration::from_secs(3) {
            let r = crate::ui::painel(460.0, 140.0, "Tempest");
            crate::ui::texto_centro(
                r.center().x,
                r.y + 45.0,
                "Verificando atualizações...",
                18,
                crate::ui::OURO,
            );
            return true;
        }
        let Some(url) = self.aviso.as_ref().filter(|_| !self.dispensado) else {
            return false;
        };
        let r = crate::ui::painel(460.0, 300.0, "Cliente desatualizado");
        crate::ui::texto_centro(
            r.center().x,
            r.y + 35.0,
            "Uma nova versão do Tempest está disponível.",
            16,
            crate::ui::OURO,
        );
        let botao = if url.starts_with("https://play.google.com/")
            || url.starts_with("https://apps.apple.com/")
        {
            "Atualizar na loja"
        } else if url == "itms-beta://" || url.starts_with("https://testflight.apple.com/") {
            "Abrir TestFlight"
        } else if plataforma() == "android" {
            "Baixar atualização"
        } else {
            "Abrir site para atualizar"
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
                "Abra mmo.brunji.com.br no navegador.",
            );
        }
        if crate::ui::botao(
            Rect::new(r.x + 20.0, r.y + 165.0, r.w - 40.0, 40.0),
            "Agora não",
            true,
        ) {
            self.dispensado = true;
        }
        true
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
