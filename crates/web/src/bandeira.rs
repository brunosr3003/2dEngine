//! CLAN FLAG UPLOAD (docs/TERRITORIOS.md): the game hands the clan leader a
//! one-time code (`social::Pedido::CodigoBandeira`) and opens this page; the
//! page sends a PNG or JPEG with the code; here it is cut to 3:2, reduced to
//! `bandeira::LARG` × `ALT` RGBA and flies at once (owner, 21/11/2026:
//! auto-approve); players report bad ones in the game's Reports tab and
//! staff refuse them in the Panopticon.

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde::Deserialize;
use shared::bandeira::{ALT, IMAGEM_APROVADA, LARG};
use sqlx::postgres::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct Estado {
    pub pool: Arc<PgPool>,
}

pub fn router(st: Estado) -> Router {
    Router::new()
        // Under /api: the proxies in front of `web` (production's nginx, the
        // test server's Caddy) only pass /api/* through.
        .route("/api/bandeira", get(pagina).post(enviar))
        // A 3 MB file in base64 is past axum's 2 MB default.
        .layer(axum::extract::DefaultBodyLimit::max(5 * 1024 * 1024))
        .with_state(st)
}

async fn pagina() -> Html<&'static str> {
    Html(include_str!("../estatico/bandeira.html"))
}

#[derive(Deserialize)]
struct Envio {
    codigo: String,
    /// The image file, base64 (PNG or JPEG).
    imagem: String,
}

/// The largest file accepted, bytes (before base64).
const MAX_BYTES: usize = 3 * 1024 * 1024;

/// Cut the middle to 3:2 and reduce to the flag's size, RGBA.
pub fn reduzir(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|_| "Not a PNG or JPEG image.".to_string())?;
    let (w, h) = (img.width(), img.height());
    if w < 12 || h < 8 {
        return Err("The image is too small.".into());
    }
    // The biggest 3:2 box in the middle.
    let (cw, ch) = if w * 2 > h * 3 { (h * 3 / 2, h) } else { (w, w * 2 / 3) };
    let corte = img.crop_imm((w - cw) / 2, (h - ch) / 2, cw, ch);
    let pequena = corte.resize_exact(LARG as u32, ALT as u32, image::imageops::FilterType::Triangle);
    let mut rgba = pequena.to_rgba8().into_raw();
    // No see-through flags: transparency becomes opaque over black.
    for p in rgba.chunks_mut(4) {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            *c = (*c as u32 * a / 255) as u8;
        }
        p[3] = 255;
    }
    Ok(rgba)
}

async fn enviar(State(st): State<Estado>, Json(e): Json<Envio>) -> impl IntoResponse {
    let erro = |c: StatusCode, m: &str| (c, Json(serde_json::json!({ "ok": false, "erro": m })));
    let bytes = match base64::engine::general_purpose::STANDARD.decode(e.imagem.trim()) {
        Ok(b) if b.len() <= MAX_BYTES => b,
        Ok(_) => return erro(StatusCode::PAYLOAD_TOO_LARGE, "The file is too big (3 MB at most)."),
        Err(_) => return erro(StatusCode::BAD_REQUEST, "Could not read the file."),
    };
    let pixels = match reduzir(&bytes) {
        Ok(p) => p,
        Err(m) => return erro(StatusCode::BAD_REQUEST, &m),
    };
    let codigo = e.codigo.trim().to_uppercase();
    let dono: Option<(i64, String)> = match sqlx::query_as(
        "DELETE FROM cla_bandeira_codigos WHERE codigo=$1 AND expira > extract(epoch from now())::bigint RETURNING cla, nome",
    )
    .bind(&codigo)
    .fetch_optional(&*st.pool)
    .await
    {
        Ok(d) => d,
        Err(err) => {
            tracing::error!("bandeira: db {err:?}");
            return erro(StatusCode::INTERNAL_SERVER_ERROR, "Server error, try again.");
        }
    };
    let Some((cla, nome)) = dono else {
        return erro(
            StatusCode::FORBIDDEN,
            "This code expired or was already used: ask for a new one in the clan panel.",
        );
    };
    let gravou = sqlx::query(
        "INSERT INTO cla_bandeira_imagem(cla,pixels,estado,enviada,enviada_por,revisada_por) \
         VALUES($1,$2,$3,extract(epoch from now())::bigint,$4,NULL) \
         ON CONFLICT (cla) DO UPDATE SET pixels=EXCLUDED.pixels, estado=EXCLUDED.estado, \
         enviada=EXCLUDED.enviada, enviada_por=EXCLUDED.enviada_por, revisada_por=NULL",
    )
    .bind(cla)
    .bind(&pixels)
    .bind(IMAGEM_APROVADA as i16)
    .bind(&nome)
    .execute(&*st.pool)
    .await;
    if let Err(err) = gravou {
        tracing::error!("bandeira: db {err:?}");
        return erro(StatusCode::INTERNAL_SERVER_ERROR, "Server error, try again.");
    }
    // A new image starts with no reports (the old ones were about the old).
    let _ = sqlx::query("DELETE FROM cla_bandeira_denuncias WHERE cla=$1")
        .bind(cla)
        .execute(&*st.pool)
        .await;
    tracing::info!("bandeira: {nome} enviou imagem para o cla {cla} (no ar; denúncias vão ao Panóptico)");
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_imagem_vira_48x32_opaca() {
        // A 300 x 100 PNG, half transparent.
        let mut img = image::RgbaImage::new(300, 100);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([200, 40, (x % 255) as u8, if x < 150 { 255 } else { 0 }]);
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let r = reduzir(&png).unwrap();
        assert_eq!(r.len(), LARG * ALT * 4);
        assert!(r.chunks(4).all(|p| p[3] == 255));
        assert!(reduzir(b"not an image").is_err());
    }
}
