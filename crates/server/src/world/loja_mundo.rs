//! Loja de cash e montarias no loop do mundo (docs/LOJA.md,
//! docs/MONTARIAS.md). Compra fala com o banco central fora do tick
//! (`crate::loja`); montar e desmontar sao do mundo, autoritativos.
//!
//! Montado:
//! - so' fora de combate (`SEM_COMBATE_PRA_MONTAR_S`), fora de dungeon, sem
//!   coletar, vivo e sem carregar nada; leva `MONTAR_S`;
//! - desmonta sozinho ao golpear, conjurar, apanhar, coletar, cair, entrar
//!   em dungeon ou carregar algo;
//! - anda `VEL_MONTADO` mais rapido. Nada de combate muda.
use super::*;
use crate::loja::{self as banco, Evento, Resposta};
use shared::loja::{self as cat, AvisoLoja, EstadoLoja, PedidoLoja, Produto};

/// Pedido de compra mais perto que isto do anterior e' ignorado.
const INTERVALO_MIN_S: f32 = 0.3;

fn avisa(to: &mpsc::UnboundedSender<ServerMessage>, aviso: AvisoLoja) {
    let _ = to.send(ServerMessage::Loja { aviso });
}

fn resultado(to: &mpsc::UnboundedSender<ServerMessage>, ok: bool, texto: impl Into<String>) {
    avisa(
        to,
        AvisoLoja::Resultado {
            ok,
            texto: texto.into(),
        },
    );
}

/// Manda o estado da loja e devolve as posses ao loop.
async fn enviar_estado(
    central: &sqlx::PgPool,
    conta: &str,
    to: &mpsc::UnboundedSender<ServerMessage>,
    tx_mundo: &mpsc::UnboundedSender<IncomingMessage>,
    sid: SessionId,
    personagem: &str,
) {
    match banco::estado(central, conta).await {
        Ok(estado) => {
            let _ = tx_mundo.send(IncomingMessage::Loja(Evento::Posses {
                sid,
                personagem: personagem.to_string(),
                posses: estado.posses.clone(),
            }));
            avisa(to, AvisoLoja::Estado(estado));
        }
        Err(e) => {
            tracing::warn!("loja: estado falhou: {e:#}");
            resultado(to, false, "Loja indisponível no momento.");
        }
    }
}

impl GameWorld {
    pub(super) fn handle_loja(&mut self, sid: SessionId, pedido: PedidoLoja) {
        match pedido {
            PedidoLoja::Montar => return self.pedir_montar(sid),
            PedidoLoja::Desmontar => {
                self.desmontar(sid);
                return;
            }
            _ => {}
        }
        let Some(s) = self.sessions.get(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let to = s.handle.to_client.clone();
        let personagem = s.name.clone();
        let conta = crate::mercado::conta_global(&crate::canais::realm(), s.account_id, &s.name);
        let Some(central) = crate::mercado::central() else {
            avisa(
                &to,
                AvisoLoja::Estado(EstadoLoja {
                    ligada: false,
                    simulado: banco::simulado(),
                    ..Default::default()
                }),
            );
            resultado(&to, false, "A loja está desligada neste servidor.");
            return;
        };
        let compra = matches!(
            pedido,
            PedidoLoja::ComprarTp { .. } | PedidoLoja::ComprarItem { .. }
        );
        if compra {
            let agora = self.sim_time_s;
            if self
                .loja_pedido_em
                .get(&sid)
                .is_some_and(|t| agora - t < INTERVALO_MIN_S)
            {
                return;
            }
            self.loja_pedido_em.insert(sid, agora);
        }
        let Some(tx_mundo) = self.auth_ctx.as_ref().map(|c| c.tx.clone()) else {
            return;
        };
        tokio::spawn(async move {
            match pedido {
                PedidoLoja::Estado => {}
                PedidoLoja::ComprarTp { pacote, pedido } => {
                    match banco::comprar_tp(
                        &central,
                        banco::Provedor::do_ambiente(),
                        &conta,
                        pacote,
                        &pedido,
                    )
                    .await
                    {
                        Ok(r) => {
                            if let (Resposta::Feito { .. }, Some(p)) = (&r, cat::pacote(pacote)) {
                                let codigo = Produto::Tp(pacote).codigo();
                                crate::telemetria::conta("loja_pedido", &codigo, 1);
                                crate::telemetria::conta(
                                    "loja_receita_centavos",
                                    "BRL",
                                    p.centavos as i64,
                                );
                                crate::telemetria::conta(
                                    "loja_tp_vendida",
                                    codigo,
                                    p.total() as i64,
                                );
                            }
                            resultado(&to, r.ok(), r.texto());
                        }
                        Err(e) => {
                            tracing::warn!("loja: compra de TP falhou: {e:#}");
                            resultado(&to, false, "Loja indisponível no momento.");
                        }
                    }
                }
                PedidoLoja::ComprarItem { produto, pedido } => {
                    match banco::comprar_item(&central, &conta, produto, &pedido).await {
                        Ok(r) => {
                            if let Resposta::Feito { .. } = &r {
                                let codigo = produto.codigo();
                                crate::telemetria::conta("loja_item", &codigo, 1);
                                crate::telemetria::conta(
                                    "loja_tp_gasta",
                                    codigo,
                                    produto.preco_tp().unwrap_or(0) as i64,
                                );
                                if let Some(m) = match produto {
                                    Produto::Moeda(id) => cat::moeda(id),
                                    _ => None,
                                } {
                                    let _ = tx_mundo.send(IncomingMessage::Loja(Evento::Moeda {
                                        sid,
                                        personagem: personagem.clone(),
                                        item_id: m.item_id,
                                        qtd: m.qtd,
                                    }));
                                }
                                let pergaminho = match produto {
                                    Produto::BauCraft(_) => {
                                        Some(shared::item_id::PERGAMINHO_INVOCA_CHAVE)
                                    }
                                    Produto::PergaminhoMontaria(_) => {
                                        Some(shared::item_id::PERGAMINHO_INVOCA_MONTARIA)
                                    }
                                    Produto::PergaminhoTomo(_) => {
                                        Some(shared::item_id::PERGAMINHO_INVOCA_TOMO)
                                    }
                                    _ => None,
                                };
                                if let Some(item_id) = pergaminho {
                                    let _ =
                                        tx_mundo.send(IncomingMessage::Loja(Evento::Consumivel {
                                            sid,
                                            personagem: personagem.clone(),
                                            item_id,
                                        }));
                                }
                            }
                            resultado(&to, r.ok(), r.texto());
                        }
                        Err(e) => {
                            tracing::warn!("loja: compra de item falhou: {e:#}");
                            resultado(&to, false, "Loja indisponível no momento.");
                        }
                    }
                }
                PedidoLoja::Montar | PedidoLoja::Desmontar => return,
            }
            enviar_estado(&central, &conta, &to, &tx_mundo, sid, &personagem).await;
        });
    }

    /// Login: as posses da conta vem do central (montar sem abrir a loja).
    pub(super) fn loja_ao_logar(&self, sid: SessionId) {
        let (Some(s), Some(central), Some(ctx)) = (
            self.sessions.get(&sid),
            crate::mercado::central(),
            self.auth_ctx.as_ref(),
        ) else {
            return;
        };
        let conta = crate::mercado::conta_global(&crate::canais::realm(), s.account_id, &s.name);
        let (personagem, tx, to) = (s.name.clone(), ctx.tx.clone(), s.handle.to_client.clone());
        // O estado inteiro (saldo e posses): o cliente sabe se tem montaria
        // pro botao do HUD, sem abrir a loja.
        tokio::spawn(
            async move { enviar_estado(&central, &conta, &to, &tx, sid, &personagem).await },
        );
    }

    pub fn on_loja(&mut self, ev: Evento) {
        match ev {
            Evento::Posses {
                sid,
                personagem,
                posses,
            } => {
                let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.name == personagem) else {
                    return;
                };
                s.loja_posses = posses;
                s.loja_carregada = true;
                self.atualizar_skin_vista(sid);
            }
            Evento::Consumivel {
                sid,
                personagem,
                item_id,
            } => {
                let Some(s) = self
                    .sessions
                    .get_mut(&sid)
                    .filter(|s| s.logged_in && s.name == personagem)
                else {
                    return;
                };
                let foi_correio = !add_to_inventory(&mut s.inventory, item_id, 1, None);
                if foi_correio {
                    let quando = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    s.dungeon.postar(item_id, 1, None, 0, quando);
                } else {
                    s.inventory_dirty = true;
                }
                self.save_pending = true;
                let nome = crate::economy::nome_do_item(item_id);
                resultado(
                    &s.handle.to_client,
                    true,
                    if foi_correio {
                        format!("{nome} enviado ao correio: sua bolsa está cheia.")
                    } else {
                        format!("{nome} entregue na bolsa. Abra para revelar o prêmio!")
                    },
                );
            }
            Evento::MontariaInvocada {
                sid,
                personagem,
                montaria,
                quantidade,
            } => {
                let to = {
                    let Some(s) = self
                        .sessions
                        .get_mut(&sid)
                        .filter(|s| s.logged_in && s.name == personagem)
                    else {
                        return;
                    };
                    s.loja_posses.registrar_montaria(montaria, quantidade);
                    s.handle.to_client.clone()
                };
                self.atualizar_skin_vista(sid);
                avisa(
                    &to,
                    AvisoLoja::Invocacao {
                        premio: cat::PremioInvocacao::Montaria {
                            id: montaria,
                            quantidade,
                        },
                    },
                );
            }
            Evento::MontariasInvocadas {
                sid,
                personagem,
                premios,
            } => {
                let (to, varios) = {
                    let Some(s) = self
                        .sessions
                        .get_mut(&sid)
                        .filter(|s| s.logged_in && s.name == personagem)
                    else {
                        return;
                    };
                    for &(id, quantidade) in &premios {
                        s.loja_posses.registrar_montaria(id, quantidade);
                    }
                    (s.handle.to_client.clone(), premios.len() > 1)
                };
                self.atualizar_skin_vista(sid);
                let mut premios: Vec<_> = premios
                    .into_iter()
                    .map(|(id, quantidade)| cat::PremioInvocacao::Montaria { id, quantidade })
                    .collect();
                let aviso = if varios {
                    AvisoLoja::Invocacoes { premios }
                } else {
                    AvisoLoja::Invocacao {
                        premio: premios.remove(0),
                    }
                };
                avisa(&to, aviso);
            }
            Evento::FalhaInvocacao {
                sid,
                personagem,
                item_id,
                quantidade,
            } => {
                let Some(s) = self
                    .sessions
                    .get_mut(&sid)
                    .filter(|s| s.logged_in && s.name == personagem)
                else {
                    return;
                };
                let foi_correio = !crate::craft::por_empilhavel(
                    &mut s.inventory,
                    item_id,
                    quantidade,
                    crate::economy::item_stack_max(item_id),
                );
                if foi_correio {
                    let quando = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    s.dungeon.postar(item_id, quantidade, None, 0, quando);
                } else {
                    s.inventory_dirty = true;
                }
                self.save_pending = true;
                resultado(
                    &s.handle.to_client,
                    false,
                    if foi_correio {
                        "Invocação indisponível. Os pergaminhos voltaram pelas Entregas."
                    } else {
                        "Invocação indisponível. Os pergaminhos voltaram para sua bolsa."
                    },
                );
            }
            Evento::Moeda {
                sid,
                personagem,
                item_id,
                qtd,
            } => {
                let Some(s) = self
                    .sessions
                    .get_mut(&sid)
                    .filter(|s| s.logged_in && s.name == personagem)
                else {
                    return;
                };
                // Ouro vai pro saldo; cobre e darksteel pra carteira (nunca
                // falta espaco pra eles).
                if item_id == shared::item_id::GOLD {
                    s.gold = s.gold.saturating_add(qtd as u64);
                } else {
                    add_to_inventory(&mut s.inventory, item_id, qtd, None);
                    s.inventory_dirty = true;
                }
                self.save_pending = true;
                crate::telemetria::conta("loja_moeda", item_id.to_string(), qtd as i64);
                let nome = crate::economy::nome_do_item(item_id);
                resultado(
                    &s.handle.to_client,
                    true,
                    format!("Você recebeu {} de {nome}!", milhar(qtd as u64)),
                );
            }
        }
    }

    /// A skin que os outros veem (`EntityMeta::kind` do jogador). Mudou: a
    /// meta vai de novo pra todo mundo que ja' conhecia a entidade.
    pub(super) fn atualizar_skin_vista(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let skin = s.loja_posses.skin_para_montar(s.preferencias.montaria_skin);
        if skin == s.montaria_skin_vista {
            return;
        }
        s.montaria_skin_vista = skin;
        let eid = s.entity_id;
        if skin.is_none() && (s.montado || s.montando_ate > 0.0) {
            s.montado = false;
            s.montando_ate = 0.0;
        }
        for outra in self.sessions.values_mut() {
            outra.last_sent.remove(&eid);
        }
    }

    fn pedir_montar(&mut self, sid: SessionId) {
        let agora = self.sim_time_s;
        let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.logged_in) else {
            return;
        };
        let to = s.handle.to_client.clone();
        if s.montado || s.montando_ate > 0.0 {
            return;
        }
        if !s.loja_carregada {
            resultado(&to, false, "Carregando suas montarias…");
            self.loja_ao_logar(sid);
            return;
        }
        if s.loja_posses
            .skin_para_montar(s.preferencias.montaria_skin)
            .is_none()
        {
            resultado(&to, false, "Você ainda não tem montaria. Veja na Loja.");
            return;
        }
        if let Err(t) = pode_montar(s, agora) {
            resultado(&to, false, t);
            return;
        }
        s.montando_ate = agora + cat::MONTAR_S;
        avisa(
            &to,
            AvisoLoja::Montando {
                segundos: cat::MONTAR_S,
            },
        );
    }

    /// Desce (ou cancela a montada). `true` = estava montado/montando.
    pub(super) fn desmontar(&mut self, sid: SessionId) -> bool {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return false;
        };
        let tinha = s.montado || s.montando_ate > 0.0;
        if s.montando_ate > 0.0 {
            avisa(&s.handle.to_client, AvisoLoja::Montando { segundos: 0.0 });
        }
        s.montado = false;
        s.montando_ate = 0.0;
        tinha
    }

    /// Uma vez por tick: termina a montada e desmonta quem nao pode mais.
    pub(super) fn avancar_montarias(&mut self) {
        let agora = self.sim_time_s;
        for s in self.sessions.values_mut().filter(|s| s.logged_in) {
            if s.montando_ate <= 0.0 && !s.montado {
                continue;
            }
            let desde = if s.montado {
                s.montado_em
            } else {
                s.montando_ate - cat::MONTAR_S
            };
            let lutou = cat::luta_desmonta(ultima_luta(s), desde);
            let pode = pode_ficar_montado(s) && !lutou;
            if s.montando_ate > 0.0 {
                if !pode {
                    s.montando_ate = 0.0;
                    avisa(&s.handle.to_client, AvisoLoja::Montando { segundos: 0.0 });
                } else if agora >= s.montando_ate {
                    s.montando_ate = 0.0;
                    s.montado = true;
                    s.montado_em = agora;
                    crate::telemetria::conta("montaria", s.montaria_skin_vista.unwrap_or(0), 1);
                }
            } else if !pode {
                s.montado = false;
            }
        }
    }

    /// Jogadores montados agora (panoptico).
    pub fn montados(&self) -> usize {
        self.sessions
            .values()
            .filter(|s| s.logged_in && s.montado)
            .count()
    }
}

/// O instante mais recente de golpe, skill ou pancada recebida.
fn ultima_luta(s: &Session) -> f32 {
    s.last_combat_at_s
        .max(s.combo_last_attack)
        .max(s.gesto_skill_em)
}

fn pode_ficar_montado(s: &Session) -> bool {
    !s.downed
        && s.carrying.is_none()
        && s.instancia == 0
        && s.coleta_no.is_none()
        && s.entity.is_some()
}

/// Pode comecar a montar agora? O texto e' o motivo pro jogador.
fn pode_montar(s: &Session, agora: f32) -> Result<(), &'static str> {
    if s.instancia != 0 {
        return Err("Não dá para montar dentro da dungeon.");
    }
    if s.coleta_no.is_some() {
        return Err("Pare a coleta para montar.");
    }
    if !pode_ficar_montado(s) {
        return Err("Agora não dá para montar.");
    }
    if !cat::pode_montar_apos_luta(ultima_luta(s), agora) {
        return Err("Saia do combate para montar.");
    }
    Ok(())
}

/// 20000 -> "20.000".
fn milhar(v: u64) -> String {
    let t = v.to_string();
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && (t.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}
