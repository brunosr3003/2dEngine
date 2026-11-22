//! CLAN FLAG REVIEW (docs/TERRITORIOS.md): the images clan leaders upload
//! on the site wait here (`cla_bandeira_imagem`, estado 1); approved (2) they
//! fly on the clan's ships, islet and the map within 30 s; refused (3) the
//! clan keeps its preset flag.

use super::Estado;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use shared::bandeira::{ALT, IMAGEM_APROVADA, IMAGEM_RECUSADA, LARG};

/// RGBA → PNG → data URL, for the review list.
fn png_data_url(rgba: &[u8]) -> String {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, LARG as u32, ALT as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = enc.write_header() {
            let _ = w.write_image_data(rgba);
        }
    }
    format!("data:image/png;base64,{}", base64(&png))
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub async fn lista(State(st): State<Estado>) -> impl IntoResponse {
    // The most reported first: images go up at once (auto-approve), the
    // players' reports bring the bad ones to the top.
    let linhas: Vec<(i64, String, String, i16, String, String, Option<String>, Vec<u8>, i64)> = sqlx::query_as(
        "SELECT b.cla, c.nome, c.tag, b.estado, b.enviada_por, \
           to_char(to_timestamp(b.enviada) AT TIME ZONE 'America/Sao_Paulo', 'DD/MM HH24:MI'), \
           b.revisada_por, b.pixels, \
           (SELECT count(*) FROM cla_bandeira_denuncias d WHERE d.cla = b.cla) AS n \
         FROM cla_bandeira_imagem b JOIN social_clas c ON c.id = b.cla \
         ORDER BY n DESC, b.enviada DESC LIMIT 100",
    )
    .fetch_all(&*st.pool)
    .await
    .unwrap_or_default();
    let v: Vec<serde_json::Value> = linhas
        .into_iter()
        .map(|(cla, nome, tag, estado, por, quando, revisor, px, n)| {
            serde_json::json!({
                "cla": cla, "nome": nome, "tag": tag, "estado": estado,
                "por": por, "quando": quando, "revisor": revisor,
                "imagem": png_data_url(&px), "denuncias": n,
            })
        })
        .collect();
    Json(serde_json::json!({ "bandeiras": v }))
}

#[derive(Deserialize)]
pub struct Acao {
    cla: i64,
    aprovar: bool,
}

pub async fn acao(State(st): State<Estado>, Json(a): Json<Acao>) -> impl IntoResponse {
    let estado = if a.aprovar { IMAGEM_APROVADA } else { IMAGEM_RECUSADA };
    let r = sqlx::query("UPDATE cla_bandeira_imagem SET estado=$1, revisada_por='panoptico' WHERE cla=$2")
        .bind(estado as i16)
        .bind(a.cla)
        .execute(&*st.pool)
        .await;
    match r {
        Ok(x) if x.rows_affected() == 1 => Json(serde_json::json!({ "ok": true })),
        _ => Json(serde_json::json!({ "ok": false })),
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn base64_confere() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }
}
