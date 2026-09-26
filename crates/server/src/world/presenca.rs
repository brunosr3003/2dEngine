//! Calendario de presenca no loop do mundo (docs/CALENDARIO.md). Quem decide
//! o resgate e' o banco (`crate::presenca`): aqui so' se pede, entrega o
//! premio que o banco concedeu (ouro no `gold`, item na bolsa, o que nao cabe
//! no correio das Entregas) uma vez por id, e mostra o estado.

use super::*;
use crate::presenca::{self as db, Evento};
use shared::presenca::{self as pr, AvisoPresenca, DadosPresenca, PedidoPresenca, Premio};

/// Ids de resgate lembrados por sessao (o save marca todos).
const APLICADOS_LEMBRADOS: usize = 64;

/// Onde cada premio foi parar.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Entrega {
    pub ouro: u64,
    pub energia: u64,
    pub na_bolsa: Vec<(u16, u32)>,
    pub no_correio: Vec<(u16, u32)>,
}

/// Entrega `premios`: ouro soma no `gold`; item vai pra bolsa; se nao couber,
/// vira carta no correio (Entregas do Mercado). Nada se perde.
/// `na_bolsa(item, qtd)` tenta por na bolsa (`add_to_inventory`).
pub(super) fn entregar(
    premios: &[Premio],
    gold: &mut u64,
    energia: &mut u64,
    mut na_bolsa: impl FnMut(u16, u32) -> bool,
    dungeon: &mut shared::dungeon::DadosDungeon,
    quando: i64,
) -> Entrega {
    let mut e = Entrega::default();
    for p in premios.iter().filter(|p| p.qtd > 0) {
        if p.item_id == pr::OURO {
            *gold = gold.saturating_add(p.qtd as u64);
            e.ouro += p.qtd as u64;
        } else if p.item_id == pr::ENERGIA {
            *energia = energia.saturating_add(p.qtd as u64);
            e.energia += p.qtd as u64;
        } else if na_bolsa(p.item_id, p.qtd) {
            e.na_bolsa.push((p.item_id, p.qtd));
        } else {
            dungeon.postar(p.item_id, p.qtd, None, pr::MOTIVO_CORREIO, quando);
            e.no_correio.push((p.item_id, p.qtd));
        }
    }
    e
}

/// `true` = o id ainda nao tinha sido entregue nesta sessao (e agora esta').
pub(super) fn marcar_entregue(aplicados: &mut Vec<String>, id: &str) -> bool {
    if aplicados.iter().any(|a| a == id) {
        return false;
    }
    aplicados.push(id.to_string());
    let sobra = aplicados.len().saturating_sub(APLICADOS_LEMBRADOS);
    aplicados.drain(..sobra);
    true
}

impl GameWorld {
    pub(super) fn handle_presenca(&mut self, sid: SessionId, pedido: PedidoPresenca) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if !s.logged_in {
            return;
        }
        let (Some(conta), Some(ctx)) = (s.account_id, self.auth_ctx.as_ref()) else {
            let _ = s.handle.to_client.send(ServerMessage::Presenca {
                aviso: AvisoPresenca::Recusado {
                    texto: "Calendário indisponível.".into(),
                },
            });
            return;
        };
        match pedido {
            PedidoPresenca::Estado => {
                db::spawn_estado(ctx.pool.clone(), ctx.tx.clone(), sid, conta)
            }
            PedidoPresenca::Resgatar { calendario } => {
                // Duplo toque: o segundo espera a resposta do primeiro. O banco
                // recusaria de qualquer jeito; isto so' poupa a ida.
                if s.presenca_em_voo {
                    return;
                }
                s.presenca_em_voo = true;
                db::spawn_resgate(
                    ctx.pool.clone(),
                    ctx.tx.clone(),
                    sid,
                    conta,
                    s.name.clone(),
                    calendario,
                );
            }
        }
    }

    pub fn on_presenca(&mut self, ev: Evento) {
        let unix = db::agora();
        match ev {
            Evento::Estado { sid, feitos } => {
                let Some(s) = self.sessions.get_mut(&sid) else {
                    return;
                };
                s.presenca = DadosPresenca::de_resgates(&feitos, unix);
                let _ = s.handle.to_client.send(ServerMessage::Presenca {
                    aviso: AvisoPresenca::Estado(s.presenca.estado(unix, pr::EVENTOS)),
                });
            }
            Evento::Recusado { sid, texto, feitos } => {
                let Some(s) = self.sessions.get_mut(&sid) else {
                    return;
                };
                s.presenca_em_voo = false;
                let _ = s.handle.to_client.send(ServerMessage::Presenca {
                    aviso: AvisoPresenca::Recusado { texto },
                });
                if let Some(feitos) = feitos {
                    s.presenca = DadosPresenca::de_resgates(&feitos, unix);
                    let _ = s.handle.to_client.send(ServerMessage::Presenca {
                        aviso: AvisoPresenca::Estado(s.presenca.estado(unix, pr::EVENTOS)),
                    });
                }
            }
            Evento::Resgatou {
                sid,
                personagem,
                id,
                plano,
                feitos,
            } => {
                let entregue = self.presenca_entregar(sid, &personagem, &id, &plano.premios, unix);
                let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.name == personagem) else {
                    // Saiu antes da resposta: a linha fica pendente e o proximo
                    // login da conta recebe.
                    tracing::warn!(
                        "[presenca] {personagem} saiu antes de receber {id}: fica pendente"
                    );
                    return;
                };
                s.presenca_em_voo = false;
                s.presenca = DadosPresenca::de_resgates(&feitos, unix);
                let dados = s.presenca.clone();
                let conta = s.account_id;
                if let Some(e) = &entregue {
                    let _ = s.handle.to_client.send(ServerMessage::Presenca {
                        aviso: AvisoPresenca::Resgatou {
                            calendario: plano.calendario,
                            dia: plano.dia_grade,
                            premios: plano.premios.clone(),
                            no_correio: e.no_correio.len() as u8,
                        },
                    });
                }
                let _ = s.handle.to_client.send(ServerMessage::Presenca {
                    aviso: AvisoPresenca::Estado(dados.estado(unix, pr::EVENTOS)),
                });
                // Os outros personagens da conta neste canal veem o dia resgatado.
                if let Some(conta) = conta {
                    for (outro, o) in self.sessions.iter_mut() {
                        if *outro != sid && o.logged_in && o.account_id == Some(conta) {
                            o.presenca = dados.clone();
                            let _ = o.handle.to_client.send(ServerMessage::Presenca {
                                aviso: AvisoPresenca::Estado(dados.estado(unix, pr::EVENTOS)),
                            });
                        }
                    }
                }
                if entregue.is_some_and(|e| !e.no_correio.is_empty()) {
                    self.dg_enviar_correio(sid);
                }
            }
            Evento::Pendentes {
                sid,
                personagem,
                pendentes,
                feitos,
            } => {
                let mut correio = false;
                for (id, premios) in &pendentes {
                    let Some(e) = self.presenca_entregar(sid, &personagem, id, premios, unix)
                    else {
                        continue;
                    };
                    correio |= !e.no_correio.is_empty();
                    if let Some(s) = self.sessions.get(&sid) {
                        let _ = s.handle.to_client.send(ServerMessage::Presenca {
                            aviso: AvisoPresenca::Resgatou {
                                calendario: pr::MENSAL,
                                dia: 0,
                                premios: premios.clone(),
                                no_correio: e.no_correio.len() as u8,
                            },
                        });
                    }
                }
                let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.name == personagem) else {
                    return;
                };
                s.presenca = DadosPresenca::de_resgates(&feitos, unix);
                let _ = s.handle.to_client.send(ServerMessage::Presenca {
                    aviso: AvisoPresenca::Estado(s.presenca.estado(unix, pr::EVENTOS)),
                });
                if correio {
                    self.dg_enviar_correio(sid);
                }
            }
        }
    }

    /// Entrega o premio do resgate `id` ao personagem logado, uma vez por id.
    /// `None` = ja' entregue ou o personagem nao esta' mais nesta sessao.
    fn presenca_entregar(
        &mut self,
        sid: SessionId,
        personagem: &str,
        id: &str,
        premios: &[Premio],
        unix: i64,
    ) -> Option<Entrega> {
        let s = self
            .sessions
            .get_mut(&sid)
            .filter(|s| s.logged_in && s.name == personagem)?;
        if !marcar_entregue(&mut s.presenca_aplicados, id) {
            return None;
        }
        let inventario = &mut s.inventory;
        let e = entregar(
            premios,
            &mut s.gold,
            &mut s.skill_progress.energia,
            |item, q| add_to_inventory(inventario, item, q, None),
            &mut s.dungeon,
            unix,
        );
        if e.energia > 0 {
            s.skills_dirty = true;
            let _ = s.handle.to_client.send(ServerMessage::ProgressoDeSkills { progresso: s.skill_progress.clone() });
        }
        s.inventory_dirty = true;
        tracing::info!("[presenca] {personagem} recebeu {id}: {e:?}");
        self.save_pending = true;
        Some(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::item_id;

    #[test]
    fn energia_vai_ao_saldo_sem_depender_da_bolsa() {
        let mut gold = 0;
        let mut energia = 100;
        let mut dg = shared::dungeon::DadosDungeon::default();
        let e = entregar(&[Premio { item_id: pr::ENERGIA, qtd: 5_000 }],
            &mut gold, &mut energia, |_, _| panic!("Energia nao e item"), &mut dg, 1);
        assert_eq!(energia, 5_100);
        assert_eq!(e.energia, 5_000);
        assert!(dg.correio.is_empty());
    }

    #[test]
    fn ouro_vai_pro_gold_item_pra_bolsa() {
        let mut gold = 10;
        let mut bolsa: Vec<(u16, u32)> = Vec::new();
        let mut dg = shared::dungeon::DadosDungeon::default();
        let premios = [
            Premio {
                item_id: pr::OURO,
                qtd: 500,
            },
            Premio {
                item_id: item_id::XP_POTION,
                qtd: 2,
            },
        ];
        let e = entregar(
            &premios,
            &mut gold,
            &mut 0,
            |id, q| {
                bolsa.push((id, q));
                true
            },
            &mut dg,
            1,
        );
        assert_eq!(gold, 510);
        assert_eq!(e.na_bolsa, vec![(item_id::XP_POTION, 2)]);
        assert_eq!(
            bolsa,
            vec![(item_id::XP_POTION, 2)],
            "ouro nao entra na bolsa"
        );
        assert!(dg.correio.is_empty());
    }

    #[test]
    fn bolsa_cheia_vai_pro_correio() {
        let mut gold = 0;
        let mut dg = shared::dungeon::DadosDungeon::default();
        let premios = [Premio {
            item_id: item_id::MARCAS_TEMPESTADE,
            qtd: 40,
        }];
        let e = entregar(&premios, &mut gold, &mut 0, |_, _| false, &mut dg, 99);
        assert_eq!(e.no_correio, vec![(item_id::MARCAS_TEMPESTADE, 40)]);
        assert_eq!(dg.correio.len(), 1);
        assert_eq!(
            (
                dg.correio[0].item_id,
                dg.correio[0].qtd,
                dg.correio[0].motivo
            ),
            (item_id::MARCAS_TEMPESTADE, 40, pr::MOTIVO_CORREIO)
        );
    }

    /// A mesma resposta do banco chegando duas vezes (ou o pendente de um
    /// resgate ja' entregue) nao entrega de novo.
    #[test]
    fn mesmo_id_entrega_uma_vez() {
        let mut aplicados = Vec::new();
        assert!(marcar_entregue(&mut aplicados, "7:0:24320:1"));
        assert!(!marcar_entregue(&mut aplicados, "7:0:24320:1"));
        assert!(marcar_entregue(&mut aplicados, "7:0:24320:2"));
        for i in 0..200 {
            marcar_entregue(&mut aplicados, &format!("x{i}"));
        }
        assert_eq!(aplicados.len(), APLICADOS_LEMBRADOS);
    }
}
