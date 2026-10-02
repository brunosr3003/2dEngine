//! Loja de cash e montarias no loop do mundo (docs/LOJA.md,
//! docs/MONTARIAS.md). Compra fala com o banco central fora do tick
//! (`crate::loja`); montar e desmontar sao do mundo, autoritativos.
//!
//! Montado: pode subir em combate e dungeon, mas nao durante coleta, caido
//! ou carregando outro jogador. Leva `MONTAR_S` e conserva a velocidade.
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

/// Manda o estado da loja (saldo e historico). Nao ha' mais posse: montaria
/// e skin sairam da loja (docs/MONTARIAS.md).
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
            let _ = tx_mundo.send(IncomingMessage::Loja(Evento::LojaCarregada {
                sid,
                personagem: personagem.to_string(),
            }));
            avisa(to, AvisoLoja::Estado(estado));
        }
        Err(e) => {
            tracing::warn!("loja: estado falhou: {e:#}");
            resultado(to, false, "The shop is unavailable right now.");
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
            resultado(&to, false, "The shop is switched off on this server.");
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
                            resultado(&to, false, "The shop is unavailable right now.");
                        }
                    }
                }
                PedidoLoja::ComprarItem {
                    produto,
                    vezes,
                    pedido,
                } => {
                    let vezes = cat::lote(vezes);
                    match banco::comprar_item(&central, &conta, produto, vezes, &pedido).await {
                        Ok(r) => {
                            if let Resposta::Feito { .. } = &r {
                                let codigo = produto.codigo();
                                crate::telemetria::conta("loja_item", &codigo, vezes as i64);
                                // O gasto medido e' o COBRADO, com desconto —
                                // senao a telemetria de TP nao fecha com o razao.
                                crate::telemetria::conta(
                                    "loja_tp_gasta",
                                    codigo,
                                    cat::preco_do_lote(produto.preco_tp().unwrap_or(0), vezes)
                                        as i64,
                                );
                                if let Some(m) = match produto {
                                    Produto::Moeda(id) => cat::moeda(id),
                                    _ => None,
                                } {
                                    // Lote: a moeda entra de uma vez, multiplicada.
                                    let _ = tx_mundo.send(IncomingMessage::Loja(Evento::Moeda {
                                        sid,
                                        personagem: personagem.clone(),
                                        item_id: m.item_id,
                                        qtd: m.qtd.saturating_mul(vezes as u32),
                                    }));
                                }
                                if let Produto::Energia(id) = produto {
                                    if let Some(e) = cat::energia(id) {
                                        let _ =
                                            tx_mundo.send(IncomingMessage::Loja(Evento::Energia {
                                                sid,
                                                personagem: personagem.clone(),
                                                qtd: e.qtd.saturating_mul(vezes as u64),
                                            }));
                                    }
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
                                    Produto::PergaminhoPet(_) => {
                                        Some(shared::item_id::PERGAMINHO_INVOCA_PET)
                                    }
                                    Produto::ItemDePet(id) => {
                                        cat::item_de_pet(id).map(|x| x.item_id)
                                    }
                                    // A SKIN chega como item; usar e' que
                                    // destrava (`world::usar_skin`). The bag
                                    // item has its own id for outfits
                                    // (`aparencia::item_da_skin`): 483 is a
                                    // Porão key now.
                                    Produto::Skin(id) => Some(shared::aparencia::item_da_skin(id)),
                                    Produto::Item(id) => cat::item_da_loja(id).map(|x| x.item_id),
                                    _ => None,
                                };
                                // O PASSE VAI EM LOTE: o pacote entrega N
                                // passes, e o lote da loja multiplica por
                                // `vezes`. Os dois se multiplicam, e o
                                // `Consumivel` entrega um item por evento.
                                if let Produto::PasseMagico(id) = produto {
                                    if let Some(pk) = cat::passe(id) {
                                        let total = pk.qtd.saturating_mul(vezes as u32);
                                        for _ in 0..total {
                                            let _ = tx_mundo.send(IncomingMessage::Loja(
                                                Evento::Consumivel {
                                                    sid,
                                                    personagem: personagem.clone(),
                                                    item_id: shared::item_id::PASSE_MAGICO,
                                                },
                                            ));
                                        }
                                    }
                                }
                                if let Some(item_id) = pergaminho {
                                    // Um evento por unidade: o `Consumivel` entrega
                                    // UM item, e empilhar na bolsa e' trabalho do
                                    // `add_to_inventory`.
                                    for _ in 0..vezes {
                                        let _ = tx_mundo.send(IncomingMessage::Loja(
                                            Evento::Consumivel {
                                                sid,
                                                personagem: personagem.clone(),
                                                item_id,
                                            },
                                        ));
                                    }
                                }
                            }
                            resultado(&to, r.ok(), r.texto());
                        }
                        Err(e) => {
                            tracing::warn!("loja: compra de item falhou: {e:#}");
                            resultado(&to, false, "The shop is unavailable right now.");
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
            Evento::LojaCarregada { sid, personagem } => {
                let Some(s) = self.sessions.get_mut(&sid).filter(|s| s.name == personagem) else {
                    return;
                };
                s.loja_carregada = true;
                self.atualizar_montaria_vista(sid);
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
                    format!("You received {} {nome}!", milhar(qtd as u64)),
                );
            }
            Evento::Energia {
                sid,
                personagem,
                qtd,
            } => {
                let Some(s) = self
                    .sessions
                    .get_mut(&sid)
                    .filter(|s| s.logged_in && s.name == personagem)
                else {
                    return;
                };
                // Mesmo saldo que a coleta enche e que tier e atributo gastam.
                s.skill_progress.energia = s.skill_progress.energia.saturating_add(qtd);
                s.skills_dirty = true;
                let _ = s.handle.to_client.send(ServerMessage::ProgressoDeSkills {
                    progresso: s.skill_progress.clone(),
                });
                self.save_pending = true;
                crate::telemetria::conta("loja_energia", "TP", qtd as i64);
                resultado(
                    &s.handle.to_client,
                    true,
                    format!("You received {} Energy!", milhar(qtd)),
                );
            }
        }
    }

    /// A skin que os outros veem (`EntityMeta::kind` do jogador). Mudou: a
    /// meta vai de novo pra todo mundo que ja' conhecia a entidade.
    /// Passa em todo mundo logado e acerta a montaria vista. Roda no tick,
    /// pelo mesmo motivo do `sincroniza_pets`: equipar tem caminho demais pra
    /// pendurar gancho em cada um.
    pub(super) fn sincroniza_montarias(&mut self) {
        let sids: Vec<SessionId> = self
            .sessions
            .values()
            .filter(|s| s.logged_in)
            .map(|s| s.handle.id)
            .collect();
        for sid in sids {
            self.atualizar_montaria_vista(sid);
        }
    }

    /// A montaria que os outros veem e' a EQUIPADA (docs/MONTARIAS.md). Sem
    /// montaria equipada ninguem fica montado no ar.
    pub(super) fn atualizar_montaria_vista(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let montaria = s
            .equipment
            .montaria
            .filter(|id| shared::montarias::de_item(*id).is_some());
        if montaria == s.montaria_vista {
            return;
        }
        s.montaria_vista = montaria;
        let eid = s.entity_id;
        if montaria.is_none() && (s.montado || s.montando_ate > 0.0) {
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
        if s.equipment
            .montaria
            .filter(|id| shared::montarias::de_item(*id).is_some())
            .is_none()
        {
            resultado(&to, false, "Equipe uma montaria na bolsa antes de montar.");
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
            // Combate pode continuar durante a subida e depois de montar.
            let pode = pode_ficar_montado(s);
            if s.montando_ate > 0.0 {
                if !pode {
                    s.montando_ate = 0.0;
                    avisa(&s.handle.to_client, AvisoLoja::Montando { segundos: 0.0 });
                } else if agora >= s.montando_ate {
                    s.montando_ate = 0.0;
                    s.montado = true;
                    s.montaria_firmeza = cat::FIRMEZA_MAX;
                    avisa(&s.handle.to_client, AvisoLoja::MontariaCombate {
                        firmeza: 100, bloqueio_segundos: 0.0,
                    });
                    s.montado_em = agora;
                    crate::telemetria::conta("montaria", s.montaria_vista.unwrap_or(0), 1);
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

fn pode_ficar_montado(s: &Session) -> bool {
    !s.downed
        && s.carrying.is_none()
        && s.coleta_no.is_none()
        && s.entity.is_some()
}

/// Pode comecar a montar agora? O texto e' o motivo pro jogador.
fn pode_montar(s: &Session, _agora: f32) -> Result<(), &'static str> {
    if s.coleta_no.is_some() {
        return Err("Stop gathering to mount.");
    }
    if !pode_ficar_montado(s) {
        return Err("You cannot mount right now.");
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

#[cfg(test)]
mod testes_combate_montado {
    use super::*;

    fn mundo() -> (GameWorld, SessionId) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        let sid = SessionId(([127, 0, 0, 1], 19841).into());
        let (tx, _) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle { id: sid, to_client: tx });
        let e = w.ecs.spawn((NetId(EntityId(904)), Position(Vec2::ZERO),
            Velocity(Vec2::ZERO), EntityKind::Player, Health { current: 100, max: 100 }));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.entity = Some(e);
        s.entity_id = EntityId(904);
        s.equipment.montaria = Some(shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 1));
        w.sim_time_s = 20.0;
        (w, sid)
    }

    #[test]
    fn combate_montado_mantem_velocidade_e_sprint() {
        for grau in [1, 5] {
            for sprint in [false, true] {
                let velocidade = |em_luta: bool| {
                    let (mut w, sid) = mundo();
                    let s = w.sessions.get_mut(&sid).unwrap();
                    s.montado = true;
                    s.montado_em = 1.0;
                    s.equipment.montaria = Some(shared::item_id::montaria_no_grau(
                        shared::item_id::MONTARIA_BASE, grau));
                    s.stamina_current = 100.0;
                    if em_luta {
                        s.last_combat_at_s = 19.9;
                        s.combo_last_attack = 19.9;
                    }
                    s.pending_input = Some(shared::protocol::InputFrame {
                        seq: 1, tick: 1, move_dir: Vec2::X, aim: Vec2::X,
                        buttons: if sprint { shared::protocol::buttons::SPRINT } else { 0 },
                    });
                    let e = s.entity.unwrap();
                    w.step(1.0 / 30.0);
                    assert!(w.sessions[&sid].montado);
                    let v = w.ecs.get::<&Velocity>(e).unwrap().0.length();
                    v
                };
                let normal = velocidade(false);
                assert!(normal > 0.0);
                assert!((velocidade(true) - normal).abs() < 0.001,
                    "montaria grau {grau}, sprint {sprint}: combate reduziu velocidade");
            }
        }
    }

    #[test]
    fn ataque_e_dano_nao_desmontam_nem_a_instancia() {
        let (mut w, sid) = mundo();
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.montado = true;
            s.montado_em = 10.0;
            s.combo_last_attack = 19.0;
            s.last_combat_at_s = 19.5;
        }
        w.avancar_montarias();
        assert!(w.sessions[&sid].montado);
        w.sessions.get_mut(&sid).unwrap().instancia = 1;
        w.avancar_montarias();
        assert!(w.sessions[&sid].montado);
    }

    #[test]
    fn monta_dentro_da_dungeon_e_mantem_apos_a_subida() {
        let (mut w, sid) = mundo();
        w.sessions.get_mut(&sid).unwrap().instancia = 1;
        w.pedir_montar(sid);
        assert!(w.sessions[&sid].montando_ate > 0.0);
        w.sim_time_s += cat::MONTAR_S;
        w.avancar_montarias();
        assert!(w.sessions[&sid].montado);
    }

    #[test]
    fn golpe_durante_subida_permite_montar() {
        let (mut w, sid) = mundo();
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.target = Some(EntityId(905));
            s.last_combat_at_s = w.sim_time_s;
        }
        w.pedir_montar(sid);
        assert!(w.sessions[&sid].montando_ate > 0.0);
        w.sim_time_s += 0.5;
        w.sessions.get_mut(&sid).unwrap().last_combat_at_s = w.sim_time_s;
        w.avancar_montarias();
        assert!(w.sessions[&sid].montando_ate > 0.0);
        w.sim_time_s += cat::MONTAR_S;
        w.avancar_montarias();
        assert!(w.sessions[&sid].montado);
        assert!(w.sessions[&sid].montado);
    }

    #[test]
    fn ataque_basico_causa_o_mesmo_dano_montado_e_a_pe() {
        let lutar = |montado| {
            let (mut w, sid) = mundo();
            w.safe_zone = false;
            let alvo = w.ecs.spawn((NetId(EntityId(905)), Position(Vec2::new(1.0, 0.0)),
                EntityKind::Enemy(1), Health { current: 10_000, max: 10_000 }));
            {
                let s = w.sessions.get_mut(&sid).unwrap();
                s.montado = montado;
                s.montado_em = 1.0;
                s.equipment.weapon = Some(shared::item_id::KATANA);
                s.target = Some(EntityId(905));
                s.stats.attack_damage = 100;
                s.stats.crit_chance = 0.0;
                s.stamina_current = 1000.0;
            }
            for n in 0..60 {
                w.sessions.get_mut(&sid).unwrap().pending_input = Some(shared::protocol::InputFrame {
                    seq: n, tick: n, move_dir: Vec2::ZERO, aim: Vec2::X, buttons: 0,
                });
                w.step(1.0 / 30.0);
            }
            assert_eq!(w.sessions[&sid].montado, montado);
            let hp = w.ecs.get::<&Health>(alvo).unwrap().current;
            10_000 - hp
        };
        let a_pe = lutar(false);
        assert!(a_pe > 0);
        assert_eq!(lutar(true), a_pe);
    }

    #[test]
    fn ataque_montado_atinge_chefe_sem_desmontar() {
        let (mut w, sid) = mundo();
        w.safe_zone = false;
        w.imortal = false;
        let (mut tag, _) = w.build_enemy_tag(7, Vec2::ZERO, 0.0, Vec2::X);
        tag.is_boss = true;
        tag.stats.defense = 0;
        tag.stats.damage_reduction_pct = 0.0;
        let alvo = w.ecs.spawn((NetId(EntityId(905)), Position(Vec2::X),
            EntityKind::Enemy(7), Health { current: 10000, max: 10000 }, tag));
        w.pedir_montar(sid);
        assert!(w.sessions[&sid].montando_ate > 0.0);
        w.sim_time_s += cat::MONTAR_S;
        w.avancar_montarias();
        w.sessions.get_mut(&sid).unwrap().target = Some(EntityId(905));
        w.pending_skill_hits.push(PendingSkillHit {
            target_net: EntityId(905), attacker_net: EntityId(904),
            damage: 100, hurt_dir: Vec2::X, is_crit: false,
            from_player: true, knockback: 0.0,
        });
        w.step(1.0 / 30.0);
        assert!(w.sessions[&sid].montado);
        assert!(w.ecs.get::<&Health>(alvo).unwrap().current < 10000);
    }

    #[test]
    fn dano_no_mundo_reduz_hp_sem_desmontar() {
        let (mut w, sid) = mundo();
        w.safe_zone = false;
        w.imortal = false;
        {
            let s = w.sessions.get_mut(&sid).unwrap();
            s.montado = true;
            s.montado_em = 1.0;
            s.stats.hp_max = 100;
            s.stats.defense = 0;
            s.stats.damage_reduction_pct = 0.0;
            s.stats.hp_regen = 0.0;
            s.poise_current = 0.0;
            s.last_press_primary_at = f32::NEG_INFINITY;
            s.last_press_secondary_at = f32::NEG_INFINITY;
        }
        for _ in 0..7 {
            w.pending_skill_hits.push(PendingSkillHit {
                target_net: EntityId(904), attacker_net: EntityId(999),
                damage: 1, hurt_dir: Vec2::X, is_crit: false,
                from_player: false, knockback: 0.0,
            });
            w.step(1.0 / 30.0);
        }
        let s = &w.sessions[&sid];
        assert!(s.montado);
        assert_eq!(s.montaria_firmeza, cat::FIRMEZA_MAX);
        assert_eq!(s.dungeon.montaria_bloqueada_ate_ms, 0);
        let hp = w.ecs.get::<&Health>(s.entity.unwrap()).unwrap();
        assert_eq!(hp.current, 93, "montaria nao absorve o dano");
    }

    #[test]
    fn entrega_antiga_de_teleporte_converte_em_cobre() {
        let mut bolsa = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS + 2];
        assert!(add_to_inventory(&mut bolsa, shared::item_id::PERGAMINHO_TELEPORTE, 3, None));
        assert!(!bolsa.iter().any(|i| i.item_id == shared::item_id::PERGAMINHO_TELEPORTE));
        assert_eq!(bolsa.iter().filter(|i| i.item_id == shared::item_id::COPPER).map(|i| i.qty).sum::<u32>(), 300);
    }
}
