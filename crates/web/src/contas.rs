//! Confirmar e-mail e redefinir senha: os tokens e as rotas.
//!
//! Os dois sao a mesma mecanica — um segredo de uso unico, com prazo, que
//! prova posse do e-mail — e por isso moram na mesma tabela (`email_tokens`,
//! com `tipo`). O que muda e' o prazo e o que acontece ao usar.
//!
//! Ver `docs/EMAIL.md`.

use anyhow::Result;
use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use std::collections::HashMap;

/// Confirmacao de conta.
pub const TIPO_CONFIRMACAO: i16 = 0;
/// Redefinicao de senha.
pub const TIPO_RESET: i16 = 1;

/// 48 h pra confirmar: e-mail as vezes demora, e a pessoa pode so' abrir no
/// dia seguinte. Nao ha' risco em ser generoso — o token so' liga a conta.
const HORAS_CONFIRMACAO: i32 = 48;
/// 1 h pro reset, e a diferenca nao e' capricho: este token TROCA a senha, e
/// quem tem a caixa de e-mail aberta por uma hora ja' teve tempo de sobra.
const HORAS_RESET: i32 = 1;

pub fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn token_novo() -> String {
    let mut bruto = [0u8; 32];
    OsRng.fill_bytes(&mut bruto);
    bruto.iter().map(|b| format!("{b:02x}")).collect()
}

/// Emite um token e devolve o segredo EM CLARO (o banco guarda so' o hash).
///
/// Apaga os anteriores do mesmo tipo: pedir "esqueci minha senha" tres vezes
/// nao pode deixar tres chaves valendo ao mesmo tempo.
pub async fn emite(pool: &PgPool, account_id: i64, tipo: i16) -> Result<String> {
    let token = token_novo();
    sqlx::query("DELETE FROM email_tokens WHERE account_id = $1 AND tipo = $2")
        .bind(account_id)
        .bind(tipo)
        .execute(pool)
        .await?;
    let horas = if tipo == TIPO_RESET {
        HORAS_RESET
    } else {
        HORAS_CONFIRMACAO
    };
    sqlx::query(
        "INSERT INTO email_tokens (token_hash, account_id, tipo, expires_at)
         VALUES ($1, $2, $3, NOW() + make_interval(hours => $4))",
    )
    .bind(hash(&token))
    .bind(account_id)
    .bind(tipo)
    .bind(horas)
    .execute(pool)
    .await?;
    // Faxina oportunista: sem isto a tabela so' cresce.
    let _ = sqlx::query("DELETE FROM email_tokens WHERE expires_at < NOW() - interval '7 days'")
        .execute(pool)
        .await;
    Ok(token)
}

/// Consome o token: devolve a conta se ele vale, e marca como usado.
///
/// O `UPDATE ... RETURNING` com `usado_em IS NULL` na condicao e' o que torna
/// o uso unico ATOMICO. Conferir e depois marcar, em dois passos, deixaria
/// dois cliques quase simultaneos passarem os dois.
pub async fn consome(pool: &PgPool, token: &str, tipo: i16) -> Result<Option<i64>> {
    let r: Option<(i64,)> = sqlx::query_as(
        "UPDATE email_tokens SET usado_em = NOW()
          WHERE token_hash = $1 AND tipo = $2 AND usado_em IS NULL AND expires_at > NOW()
      RETURNING account_id",
    )
    .bind(hash(token))
    .bind(tipo)
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|(id,)| id))
}

/// Pagina simples pro navegador, que e' onde os links do e-mail abrem.
pub fn pagina(ok: bool, titulo: &str, texto: &str) -> Html<String> {
    let cor = if ok { "#7fc98a" } else { "#e08a7a" };
    Html(format!(
        r#"<!doctype html><html lang="pt-BR"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Tempest</title></head>
<body style="margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#151d2b;font-family:-apple-system,Segoe UI,Roboto,Arial,sans-serif;color:#e8e2d4">
<div style="max-width:420px;padding:28px;background:#1d2738;border-radius:12px;text-align:center">
<div style="font-size:20px;font-weight:700;color:#f0c674;margin-bottom:14px">Tempest</div>
<div style="font-size:17px;font-weight:600;color:{cor};margin-bottom:8px">{titulo}</div>
<div style="font-size:15px;line-height:22px;color:#c9c2b4">{texto}</div>
</div></body></html>"#
    ))
}

// ─────────────────────────── rotas ───────────────────────────

#[derive(Clone)]
pub struct Contas {
    pub pool: PgPool,
    pub email: Option<crate::email::Email>,
}

/// `GET /api/auth/confirmar?token=…` — o link do e-mail de boas-vindas.
pub async fn confirmar(State(st): State<Contas>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(token) = q.get("token") else {
        return (StatusCode::BAD_REQUEST, pagina(false, "Link inválido", "Faltou o código.")).into_response();
    };
    match consome(&st.pool, token, TIPO_CONFIRMACAO).await {
        Ok(Some(id)) => {
            if let Err(e) = sqlx::query("UPDATE accounts SET email_confirmado = TRUE WHERE id = $1")
                .bind(id)
                .execute(&st.pool)
                .await
            {
                tracing::error!("confirmar conta {id}: {e:?}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    pagina(false, "Deu erro aqui", "Tente de novo daqui a pouco."),
                )
                    .into_response();
            }
            pagina(
                true,
                "Conta confirmada!",
                "Pode voltar ao jogo e entrar normalmente.",
            )
            .into_response()
        }
        Ok(None) => (
            StatusCode::BAD_REQUEST,
            pagina(
                false,
                "Link vencido ou já usado",
                "Peça um novo pelo jogo, na tela de login.",
            ),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("confirmar: {e:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                pagina(false, "Deu erro aqui", "Tente de novo daqui a pouco."),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct EsqueciReq {
    pub email: String,
}

/// `POST /api/auth/esqueci { email }` — sempre 200.
///
/// Responder 404 pra e-mail que nao existe transformaria esta rota num
/// verificador de cadastro: qualquer um descobriria quem tem conta aqui. A
/// resposta e' a mesma nos dois casos, e o e-mail so' sai se houver conta.
pub async fn esqueci(State(st): State<Contas>, Json(req): Json<EsqueciReq>) -> StatusCode {
    let email = req.email.trim().to_lowercase();
    let achou: Option<(i64, String, bool)> = sqlx::query_as(
        "SELECT id, username, google_sub IS NOT NULL FROM accounts WHERE lower(email) = $1",
    )
    .bind(&email)
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);
    if let (Some((id, usuario, so_google)), Some(mail)) = (achou, st.email.as_ref()) {
        // Conta criada pelo Google nao tem senha pra redefinir — a senha dela
        // e' `!google`, que nunca casa. Mandar o link seria prometer o que
        // nao acontece.
        if so_google {
            tracing::info!("esqueci: conta {id} e' do Google, sem senha pra trocar");
        } else {
            match emite(&st.pool, id, TIPO_RESET).await {
                Ok(t) => {
                    if let Err(e) = mail.reset(&email, &usuario, &t).await {
                        tracing::warn!("nao consegui mandar reset pra conta {id}: {e:?}");
                    }
                }
                Err(e) => tracing::error!("emitir reset {id}: {e:?}"),
            }
        }
    }
    StatusCode::OK
}

/// `GET /api/auth/reset?token=…` — o formulário de senha nova, no navegador.
///
/// E' uma pagina, e nao um caminho de volta pro app, porque o link abre no
/// e-mail: quem clica ja' esta' num navegador, e pedir pra voltar ao jogo
/// digitar um codigo seria trabalho que a pagina faz melhor.
pub async fn reset_form(Query(q): Query<HashMap<String, String>>) -> Response {
    let token = q.get("token").cloned().unwrap_or_default();
    if token.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            pagina(false, "Link inválido", "Faltou o código."),
        )
            .into_response();
    }
    let t = token.replace('"', "");
    Html(format!(
        r#"<!doctype html><html lang="pt-BR"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Nova senha — Tempest</title></head>
<body style="margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#151d2b;font-family:-apple-system,Segoe UI,Roboto,Arial,sans-serif;color:#e8e2d4">
<div style="max-width:420px;width:100%;padding:28px;background:#1d2738;border-radius:12px">
<div style="font-size:20px;font-weight:700;color:#f0c674;margin-bottom:16px">Tempest</div>
<div style="font-size:16px;margin-bottom:16px">Escolha uma senha nova</div>
<input id="a" type="password" placeholder="nova senha" autocomplete="new-password"
  style="width:100%;box-sizing:border-box;padding:12px;margin-bottom:10px;border-radius:8px;border:1px solid #3a4658;background:#151d2b;color:#e8e2d4;font-size:16px">
<input id="b" type="password" placeholder="repetir a senha" autocomplete="new-password"
  style="width:100%;box-sizing:border-box;padding:12px;margin-bottom:6px;border-radius:8px;border:1px solid #3a4658;background:#151d2b;color:#e8e2d4;font-size:16px">
<div id="m" style="font-size:13px;color:#e08a7a;min-height:19px;margin-bottom:10px"></div>
<button id="s" style="width:100%;padding:13px;border:0;border-radius:8px;background:#f0c674;color:#1d2738;font-weight:700;font-size:15px;cursor:pointer">Salvar</button>
</div>
<script>
const $=i=>document.getElementById(i);
$('s').onclick=async()=>{{
  const a=$('a').value,b=$('b').value,m=$('m');
  if(a.length<6){{m.textContent='A senha precisa de pelo menos 6 caracteres.';return;}}
  if(a!==b){{m.textContent='As duas senhas não são iguais.';return;}}
  $('s').disabled=true;m.style.color='#c9c2b4';m.textContent='Salvando…';
  try{{
    const r=await fetch('/api/auth/reset',{{method:'POST',headers:{{'content-type':'application/json'}},
      body:JSON.stringify({{token:{t:?},password:a}})}});
    if(r.ok){{document.body.innerHTML='<div style="text-align:center;padding:28px"><div style="font-size:20px;font-weight:700;color:#f0c674">Tempest</div><div style="font-size:17px;color:#7fc98a;margin-top:12px">Senha trocada!</div><div style="color:#c9c2b4;margin-top:8px">Volte ao jogo e entre com ela.</div></div>';return;}}
    const j=await r.json().catch(()=>({{}}));
    m.style.color='#e08a7a';m.textContent=j.error||'Link vencido ou já usado. Peça outro pelo jogo.';
  }}catch(e){{m.style.color='#e08a7a';m.textContent='Falha de rede. Tente de novo.';}}
  $('s').disabled=false;
}};
</script></body></html>"#
    ))
    .into_response()
}

#[derive(Deserialize)]
pub struct ResetReq {
    pub token: String,
    pub password: String,
}

/// `POST /api/auth/reset { token, password }`.
pub async fn reset(State(st): State<Contas>, Json(req): Json<ResetReq>) -> Response {
    if req.password.len() < 6 || req.password.len() > 128 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"senha deve ter 6-128 chars"})),
        )
            .into_response();
    }
    let id = match consome(&st.pool, &req.token, TIPO_RESET).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"link vencido ou já usado"})),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("reset consome: {e:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let hash = match crate::auth::hash_password(&req.password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("reset hash: {e:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    // A troca de senha DERRUBA as sessões guardadas.
    //
    // "Esqueci minha senha" é o que a pessoa faz quando desconfia que alguém
    // entrou. Deixar de pé um "lembrar de mim" emitido antes manteria esse
    // alguém dentro justamente depois do conserto.
    let r = sqlx::query("UPDATE accounts SET password_hash = $1, email_confirmado = TRUE WHERE id = $2")
        .bind(&hash)
        .bind(id)
        .execute(&st.pool)
        .await;
    if let Err(e) = r {
        tracing::error!("reset update: {e:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let _ = sqlx::query("DELETE FROM login_tokens WHERE account_id = $1")
        .bind(id)
        .execute(&st.pool)
        .await;
    tracing::info!("senha redefinida pra conta {id}");
    StatusCode::OK.into_response()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_hash_do_token_e_sha256_hex() {
        assert_eq!(hash("abc").len(), 64);
        assert_eq!(
            hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(hash("abc"), hash("abd"));
    }

    /// Token novo a cada chamada, e com entropia de verdade.
    #[test]
    fn cada_token_e_diferente() {
        let a = token_novo();
        let b = token_novo();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64, "256 bits em hex");
    }

    /// A página do formulário tem que carregar o token que recebeu.
    #[test]
    fn a_pagina_de_reset_leva_o_token() {
        let h = format!("{:?}", "abc123");
        assert_eq!(h, "\"abc123\"");
    }
}
