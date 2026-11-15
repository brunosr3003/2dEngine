//! BONUS PACKS in the world loop (docs/OFERTAS_RELAMPAGO.md, "Pacotes
//! bônus"; rules in `shared::pacotes_bonus`, database in `crate::ofertas`).
//!
//! The world only notices the moment (went down, left or failed the first
//! Porão), builds the blue gear the pack would give from the character's
//! equipment and computes the power with it on — the sheet's maths,
//! `effective_stats` -> `dungeon::poder_de_stats`. The central database picks
//! the pack and opens it, off the tick; the answer pops on the client.
//!
//! Per ACCOUNT, like the level offers: each pack opens once per account, an
//! alt never reopens it. Never for staff or test bots, and only for a session
//! that can pay (its client asked for offers: Android with Google Play, or
//! any client on a server with simulated payments).
use super::*;
use shared::pacotes_bonus::{self as pb, Gatilho, Pecas};
use std::collections::HashMap;
use std::sync::Mutex;

/// Sessions whose client asked for offers (it can pay), by character name.
static PAGANTES: Mutex<Option<HashMap<SessionId, String>>> = Mutex::new(None);

/// The client of `sid` asked for offers: it can pay.
pub(super) fn marcar_pagante(sid: SessionId, nome: &str) {
    if let Ok(mut g) = PAGANTES.lock() {
        let m = g.get_or_insert_with(HashMap::new);
        if m.len() > 20_000 {
            m.clear();
        }
        m.insert(sid, nome.to_string());
    }
}

fn e_pagante(sid: SessionId, nome: &str) -> bool {
    PAGANTES
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|m| m.get(&sid).is_some_and(|n| n == nome)))
        .unwrap_or(false)
}

/// May this character get a NEW offer at all? Not staff, not a test bot.
pub(super) fn recebe_ofertas(s: &Session) -> bool {
    s.logged_in
        && s.account_id.is_some_and(|c| !crate::chat_global::staff(c))
        && !pb::e_bot_de_teste(&s.name)
}

impl GameWorld {
    /// HOOK: a player just went down (`entity_id` of the player).
    pub(super) fn oferta_ao_cair(&mut self, eid: EntityId) {
        let Some((sid, inst)) = self
            .sessions
            .iter()
            .find(|(_, s)| s.entity_id == eid)
            .map(|(sid, s)| (*sid, s.instancia))
        else {
            return;
        };
        let no_inicial = inst != 0
            && self
                .instancias
                .iter()
                .any(|i| i.id == inst && i.conteudo == pb::PORAO_INICIAL);
        if no_inicial {
            self.disparar_oferta(sid, &[Gatilho::PoraoInicial, Gatilho::Morte]);
        } else {
            self.disparar_oferta(sid, &[Gatilho::Morte]);
        }
    }

    /// HOOK: left (or ran out of time in) dungeon `conteudo` without winning.
    pub(super) fn oferta_ao_falhar_porao(&mut self, sid: SessionId, conteudo: u16) {
        if conteudo == pb::PORAO_INICIAL {
            self.disparar_oferta(sid, &[Gatilho::PoraoInicial]);
        }
    }

    fn disparar_oferta(&mut self, sid: SessionId, gatilhos: &[Gatilho]) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if !recebe_ofertas(s) || !e_pagante(sid, &s.name) {
            return;
        }
        let Some(central) = crate::mercado::central() else {
            return;
        };
        let conta = crate::mercado::conta_global(&crate::canais::realm(), s.account_id, &s.name);
        let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
        let montagens = montagens_de(s, nivel);
        let (personagem, to, gatilhos) = (s.name.clone(), s.handle.to_client.clone(), gatilhos.to_vec());
        tokio::spawn(async move {
            match crate::ofertas::disparar(&central, &conta, &personagem, &gatilhos, nivel, &montagens).await {
                Ok(Some(id)) => {
                    crate::telemetria::conta_de(&personagem, "oferta_aberta", &id, 1);
                    let g = gatilhos.first().map(|g| g.chave()).unwrap_or("");
                    crate::telemetria::conta_de(&personagem, "oferta_gatilho", format!("{g}:{id}"), 1);
                    if let Ok(v) = crate::ofertas::listar(&central, &conta, Some(&id)).await {
                        let _ = to.send(ServerMessage::Loja {
                            aviso: shared::loja::AvisoLoja::OfertasRelampago(v),
                        });
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::warn!("ofertas: gatilho falhou: {e:#}"),
            }
        });
    }
}

/// The gear options for this character: his weapon in blue, and the whole
/// blue set, each with the power now and with it on.
fn montagens_de(s: &Session, nivel: u32) -> Vec<crate::ofertas::Montagem> {
    let antes = shared::dungeon::poder_de_stats(&effective_stats(
        &s.equipment,
        &s.allocated_points,
        &s.proficiencies,
        s.xp,
    ));
    [Pecas::Arma, Pecas::Conjunto]
        .into_iter()
        .map(|tipo| {
            let mut equip = s.equipment;
            let mut itens = Vec::new();
            for (slot, item, ilvl) in pb::pecas_azuis(tipo, &s.equipment, nivel) {
                let inst = crate::ofertas::instancia_da_peca(item, ilvl);
                equip.set(slot, Some(item), inst);
                itens.push((item, ilvl));
            }
            let depois = shared::dungeon::poder_de_stats(&effective_stats(
                &equip,
                &s.allocated_points,
                &s.proficiencies,
                s.xp,
            ));
            crate::ofertas::Montagem { pecas: tipo, itens, poder_antes: antes, poder_depois: depois }
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pagante_e_por_sessao_e_nome() {
        let sid = SessionId("127.0.0.1:41999".parse().unwrap());
        assert!(!e_pagante(sid, "Fulano"));
        marcar_pagante(sid, "Fulano");
        assert!(e_pagante(sid, "Fulano"));
        assert!(!e_pagante(sid, "Outro"), "another character on the same socket");
    }
}
