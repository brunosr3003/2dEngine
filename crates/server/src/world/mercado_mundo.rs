//! The global market in the world loop (docs/MERCADO.md): validates the
//! request, takes what goes into escrow out of the bag (or out of gold) and
//! applies the letters that come back. Talking to the central database is
//! `crate::mercado`'s job, always outside the tick.
use super::*;
use crate::mercado::{self, Evento, OpCentral, Registro};
use shared::mercado::{self as regras, CartaNet, Recusa};

/// Market requests closer together than this are ignored (sim seconds).
const INTERVALO_MIN_S: f32 = 0.25;
/// A listing id that fits in the database.
const ID_MAX: usize = 64;

/// The character, the way the market needs it.
struct Quem {
    to_client: mpsc::UnboundedSender<ServerMessage>,
    nome: String,
    conta: String,
    realm: String,
    nivel: u32,
}

fn responde(to: &mpsc::UnboundedSender<ServerMessage>, ok: bool, texto: impl Into<String>) {
    let _ = to.send(ServerMessage::MercadoResultado {
        ok,
        texto: texto.into(),
    });
}

impl GameWorld {
    fn mercado_quem(&self, sid: SessionId) -> Option<Quem> {
        let s = self.sessions.get(&sid).filter(|s| s.logged_in)?;
        let realm = crate::canais::realm();
        Some(Quem {
            to_client: s.handle.to_client.clone(),
            conta: mercado::conta_global(&realm, s.account_id, &s.name),
            nome: s.name.clone(),
            nivel: shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier()),
            realm,
        })
    }

    /// Every request from the Market panel goes through here.
    pub(super) fn handle_mercado(&mut self, sid: SessionId, msg: ClientMessage) {
        let Some(quem) = self.mercado_quem(sid) else {
            return;
        };
        let Some(central) = mercado::central() else {
            responde(
                &quem.to_client,
                false,
                "O mercado global está desligado neste servidor.",
            );
            return;
        };
        let agora = self.sim_time_s;
        let recebe = matches!(msg, ClientMessage::MercadoReceber);
        match self.mercado_pedido_em.get(&sid) {
            // The "Receive" the server chains itself does not count as spam.
            Some(t) if agora - t < INTERVALO_MIN_S && !recebe => return,
            _ => {}
        }
        self.mercado_pedido_em.insert(sid, agora);
        let Some((realm_pool, tx_mundo)) = self
            .auth_ctx
            .as_ref()
            .map(|c| (c.pool.clone(), c.tx.clone()))
        else {
            return;
        };
        match msg {
            ClientMessage::MercadoBuscar { filtro } => {
                tokio::spawn(async move {
                    match mercado::buscar(&central, &filtro, &quem.realm, &quem.nome).await {
                        Ok((anuncios, tem_mais)) => {
                            let _ = quem.to_client.send(ServerMessage::MercadoLista {
                                anuncios,
                                pagina: filtro.pagina,
                                tem_mais,
                            });
                        }
                        Err(e) => {
                            tracing::warn!("mercado: busca falhou: {e:#}");
                            responde(&quem.to_client, false, Recusa::Indisponivel.texto());
                        }
                    }
                });
            }
            ClientMessage::MercadoMeus => {
                tokio::spawn(async move { enviar_meus(&central, &quem).await });
            }
            ClientMessage::MercadoEntregas => {
                tokio::spawn(async move { enviar_entregas(&central, &realm_pool, &quem).await });
            }
            ClientMessage::MercadoReceber => {
                tokio::spawn(async move {
                    match mercado::cartas_pendentes(&central, &realm_pool, &quem.realm, &quem.nome)
                        .await
                    {
                        Ok(cartas) if cartas.is_empty() => {
                            responde(&quem.to_client, true, "No delivery waiting.")
                        }
                        Ok(cartas) => {
                            let _ = tx_mundo.send(IncomingMessage::Mercado(Evento::Cartas {
                                sid,
                                personagem: quem.nome.clone(),
                                cartas,
                            }));
                        }
                        Err(e) => {
                            tracing::warn!("mercado: entregas falharam: {e:#}");
                            responde(&quem.to_client, false, Recusa::Indisponivel.texto());
                        }
                    }
                });
            }
            ClientMessage::MercadoCancelar { anuncio } => {
                if anuncio.len() > ID_MAX {
                    return;
                }
                tokio::spawn(async move {
                    match mercado::cancelar(&central, &quem.realm, &quem.nome, &anuncio).await {
                        Ok((ok, texto)) => responde(&quem.to_client, ok, texto),
                        Err(e) => {
                            tracing::warn!("mercado: cancelar falhou: {e:#}");
                            responde(&quem.to_client, false, Recusa::Indisponivel.texto());
                        }
                    }
                    enviar_meus(&central, &quem).await;
                });
            }
            ClientMessage::MercadoAnunciarTp { qtd, preco_unit } => {
                if let Err(r) = regras::pode_anunciar_tp(quem.nivel, qtd, preco_unit) {
                    responde(&quem.to_client, false, r.texto());
                    return;
                }
                tokio::spawn(async move {
                    match mercado::anunciar_tp(
                        &central,
                        &quem.realm,
                        &quem.conta,
                        &quem.nome,
                        qtd,
                        preco_unit,
                    )
                    .await
                    {
                        Ok((ok, texto)) => responde(&quem.to_client, ok, texto),
                        Err(e) => {
                            tracing::warn!("mercado: anuncio de TP falhou: {e:#}");
                            responde(&quem.to_client, false, Recusa::Indisponivel.texto());
                        }
                    }
                    enviar_meus(&central, &quem).await;
                });
            }
            ClientMessage::MercadoAnunciar {
                inv_slot,
                qtd,
                preco_unit,
            } => {
                self.mercado_anunciar(sid, quem, inv_slot as usize, qtd, preco_unit);
            }
            ClientMessage::MercadoComprar {
                anuncio,
                qtd,
                preco_unit,
            } => {
                if anuncio.is_empty() || anuncio.len() > ID_MAX {
                    return;
                }
                self.mercado_comprar(sid, quem, anuncio, qtd, preco_unit);
            }
            _ => {}
        }
    }

    /// The item leaves the bag NOW and the operation joins the save queue. Both
    /// go to the database in the same transaction; the relay carries it to the
    /// central one later.
    fn mercado_anunciar(
        &mut self,
        sid: SessionId,
        quem: Quem,
        inv_slot: usize,
        qtd: u32,
        preco_unit: u64,
    ) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let Some(slot) = s.inventory.get(inv_slot).copied().filter(|sl| sl.qty > 0) else {
            responde(&quem.to_client, false, Recusa::Quantidade.texto());
            return;
        };
        // Peca de bau de dungeon vem vinculada na propria instancia.
        let vinculado = crate::economy::item_vinculado(slot.item_id)
            || slot.instance.is_some_and(|i| i.vinculado);
        if let Err(r) = regras::pode_anunciar(quem.nivel, vinculado, slot.qty, qtd, preco_unit) {
            responde(&quem.to_client, false, r.texto());
            return;
        }
        s.inventory[inv_slot].qty -= qtd;
        if s.inventory[inv_slot].qty == 0 {
            s.inventory[inv_slot] = shared::InventorySlot::default();
        }
        s.inventory_dirty = true;
        let mut nome = crate::economy::item_nome(slot.item_id);
        if nome.is_empty() {
            nome = format!("Item {}", slot.item_id);
        }
        let categoria =
            regras::categoria_do_item(slot.item_id, crate::economy::item_equipavel(slot.item_id));
        crate::telemetria::conta("mercado", "anunciar", 1);
        responde(
            &quem.to_client,
            true,
            format!("Anúncio enviado: {nome} ×{qtd}. Aparece no mercado em instantes."),
        );
        self.mercado_registros
            .push(Registro::Saida(OpCentral::Anunciar {
                id: mercado::novo_id(),
                realm: quem.realm,
                conta: quem.conta,
                personagem: quem.nome,
                item_id: slot.item_id,
                nome,
                categoria: categoria as u8,
                instancia: slot.instance,
                qtd: qtd as u64,
                preco_unit,
            }));
        self.save_pending = true;
    }

    /// The gold leaves NOW; the central one closes the purchase or returns it by letter.
    fn mercado_comprar(
        &mut self,
        sid: SessionId,
        quem: Quem,
        anuncio: String,
        qtd: u64,
        preco_unit: u64,
    ) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let total = match regras::pode_comprar(s.gold, qtd, preco_unit) {
            Ok(t) => t,
            Err(r) => {
                responde(&quem.to_client, false, r.texto());
                return;
            }
        };
        s.gold -= total;
        crate::telemetria::conta("mercado", "comprar", 1);
        crate::telemetria::conta("ouro_ralo", "mercado_compra", total as i64);
        responde(
            &quem.to_client,
            true,
            format!("Compra enviada: {total} gold reservados. Chega em Entregas."),
        );
        self.mercado_registros
            .push(Registro::Saida(OpCentral::Comprar {
                id: mercado::novo_id(),
                realm: quem.realm,
                conta: quem.conta,
                personagem: quem.nome,
                anuncio,
                qtd,
                preco_unit,
                pago: total,
            }));
        self.save_pending = true;
    }

    /// Records that go in these characters' save. The rest waits for the owner's
    /// save: an operation recorded without the bag it changed would duplicate an item.
    pub fn tomar_registros_mercado(
        &mut self,
        rows: &[crate::persistence::CharacterRow],
    ) -> Vec<Registro> {
        if self.mercado_registros.is_empty() {
            return Vec::new();
        }
        let (vao, ficam): (Vec<Registro>, Vec<Registro>) =
            std::mem::take(&mut self.mercado_registros)
                .into_iter()
                .partition(|r| rows.iter().any(|row| row.name == r.personagem()));
        self.mercado_registros = ficam;
        vao
    }

    pub fn on_mercado(&mut self, ev: Evento) {
        match ev {
            Evento::Aviso {
                personagem,
                ok,
                texto,
            } => {
                if let Some(s) = self
                    .sessions
                    .values()
                    .find(|s| s.logged_in && s.name == personagem)
                {
                    responde(&s.handle.to_client, ok, texto);
                }
            }
            Evento::Cartas {
                sid,
                personagem,
                cartas,
            } => self.mercado_aplicar_cartas(sid, &personagem, cartas),
        }
    }

    /// Aplica o que couber na bolsa. Cada carta aplicada vira registro do
    /// save; a que ja' foi aplicada neste processo e' pulada.
    fn mercado_aplicar_cartas(&mut self, sid: SessionId, personagem: &str, cartas: Vec<CartaNet>) {
        let a_aplicar: Vec<CartaNet> =
            regras::cartas_a_aplicar(&cartas, &self.mercado_cartas_vistas)
                .into_iter()
                .cloned()
                .collect();
        let Some(s) = self
            .sessions
            .get_mut(&sid)
            .filter(|s| s.logged_in && s.name == personagem)
        else {
            return;
        };
        let (mut recebidas, mut gold, mut cheia) = (0usize, 0u64, false);
        let mut aplicadas = Vec::new();
        for c in a_aplicar {
            if c.item_id != 0 && c.qtd > 0 {
                let qtd = u32::try_from(c.qtd).unwrap_or(u32::MAX);
                let mut prova = s.inventory.clone();
                if !add_to_inventory(&mut prova, c.item_id, qtd, c.instancia) {
                    cheia = true;
                    break;
                }
                s.inventory = prova;
                s.inventory_dirty = true;
            }
            s.gold = s.gold.saturating_add(c.gold);
            gold += c.gold;
            crate::telemetria::conta("mercado", "carta_aplicada", 1);
            crate::telemetria::conta("ouro_fonte", "mercado_carta", c.gold as i64);
            recebidas += 1;
            aplicadas.push(c.id);
        }
        let to_client = s.handle.to_client.clone();
        for id in aplicadas {
            self.mercado_cartas_vistas.insert(id.clone());
            self.mercado_registros.push(Registro::CartaAplicada {
                id,
                personagem: personagem.to_string(),
            });
        }
        if recebidas > 0 {
            self.save_pending = true;
        }
        let mut texto = match recebidas {
            0 => String::new(),
            1 => "1 delivery claimed".to_string(),
            n => format!("{n} deliveries claimed"),
        };
        if gold > 0 {
            texto.push_str(&format!(" (+{gold} gold)"));
        }
        if cheia {
            if !texto.is_empty() {
                texto.push_str(". ");
            }
            texto.push_str("Bolsa cheia: libere espaço para receber o resto.");
        }
        responde(
            &to_client,
            recebidas > 0,
            if texto.is_empty() {
                "Nothing to claim.".to_string()
            } else {
                texto
            },
        );
        self.handle_mercado(sid, ClientMessage::MercadoEntregas);
    }
}

async fn enviar_meus(central: &sqlx::PgPool, quem: &Quem) {
    match mercado::meus(central, &quem.realm, &quem.nome, &quem.conta).await {
        Ok((anuncios, historico, tp)) => {
            let _ = quem.to_client.send(ServerMessage::MercadoMeus {
                anuncios,
                historico,
                tp,
            });
        }
        Err(e) => {
            tracing::warn!("mercado: meus anuncios falhou: {e:#}");
            responde(&quem.to_client, false, Recusa::Indisponivel.texto());
        }
    }
}

async fn enviar_entregas(central: &sqlx::PgPool, realm_pool: &sqlx::PgPool, quem: &Quem) {
    let cartas = mercado::cartas_pendentes(central, realm_pool, &quem.realm, &quem.nome).await;
    let tp = mercado::saldo_tp(central, &quem.conta).await;
    match (cartas, tp) {
        (Ok(cartas), Ok(tp)) => {
            let _ = quem
                .to_client
                .send(ServerMessage::MercadoEntregas { cartas, tp });
        }
        (Err(e), _) | (_, Err(e)) => {
            tracing::warn!("mercado: entregas falharam: {e:#}");
            responde(&quem.to_client, false, Recusa::Indisponivel.texto());
        }
    }
}
