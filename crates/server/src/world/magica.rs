//! A ILHA MÁGICA no servidor (`shared::magica`).
//!
//! # Ela é uma ZONA, e não uma instância
//!
//! A colônia é instância porque é de um jogador só. A Ilha Mágica é o
//! contrário: PvP aberto num punhado de ilhotas, onde a ponte é gargalo e o
//! gargalo é onde duas pessoas que querem a mesma ilhota se encontram. Numa
//! instância por jogador nada disso existe — sobra um bônus de graça.
//!
//! Então ela é um processo com `MMO_ZONA=ilha_magica`, como as ilhas do
//! arquipélago, e entrar nela é o mesmo handoff de sempre
//! (`mandar_para_zona`).
//!
//! # O relógio é um INSTANTE, não um saldo
//!
//! `magica_ate` guarda QUANDO a sessão acaba, em unix secs. É o mesmo
//! desenho das poções, e pelo mesmo motivo: o tempo corre mesmo deslogado.
//! Guardar "quanto falta" deixaria o jogador deslogar na ilhota boa e voltar
//! no dia seguinte com a meia hora inteira — e a ilha deixaria de ter hora,
//! que é a única coisa que dá peso à escolha de onde ficar.
//!
//! # O passe é um ITEM
//!
//! `item_id::PASSE_MAGICO` na bolsa. Assim ele cai de chefe, se compra com TP
//! e se vende no mercado sem nenhuma regra nova, e o "quantos eu tenho" é a
//! quantidade na bolsa, que o jogador já sabe ler.

use super::*;

impl GameWorld {
    /// Este processo É a Ilha Mágica?
    pub(crate) fn na_magica(&self) -> bool {
        shared::magica::e_magica(&self.zona)
    }

    pub(super) fn handle_magica(&mut self, sid: SessionId, pedido: shared::magica::PedidoMagica) {
        use shared::magica::PedidoMagica as P;
        match pedido {
            P::Painel => self.abrir_magica(sid),
            P::Entrar { entradas, grau } => self.entrar_na_magica(sid, entradas, grau),
            P::Trocar { grau } => self.trocar_degrau_magico(sid, grau),
            P::Sair => self.sair_da_magica(sid, "Você deixa a Ilha Mágica."),
        }
    }

    fn avisa_magica(&self, sid: SessionId, texto: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Magica {
                aviso: shared::magica::AvisoMagica::Recusa(texto.to_string()),
            });
        }
    }

    /// Quantos passes o personagem tem na bolsa.
    fn passes_de(&self, sid: SessionId) -> u32 {
        self.sessions.get(&sid).map_or(0, |s| {
            s.inventory
                .iter()
                .filter(|i| i.item_id == shared::item_id::PASSE_MAGICO)
                .map(|i| i.qty)
                .sum()
        })
    }

    /// O bônus do chão onde o jogador está — `None` fora da Ilha Mágica, na
    /// ponte ou na água.
    pub(crate) fn bonus_magico_de(&self, sid: SessionId) -> Option<shared::magica::Bonus> {
        if !self.na_magica() {
            return None;
        }
        // O relógio manda: vencido, o bônus acaba na hora, mesmo antes de o
        // tick expulsar. Sem isso haveria uma janela de um tick por segundo
        // em que a ilha ainda paga o dobro.
        let s = self.sessions.get(&sid)?;
        if shared::magica::resta(s.magica_ate, (now_ms() / 1000) as i64) == 0 {
            return None;
        }
        let p = self.pos_do_jogador(sid)?;
        shared::magica::bonus_em(p)
    }

    /// O multiplicador que vale pra este jogador AGORA, se o bônus dele for
    /// `qual`. 1.0 = nada muda.
    ///
    /// Recebe o bônus PEDIDO em vez de devolver o que ele tem porque quem
    /// chama sabe o que está pagando: quem dá XP pergunta por `Xp`, quem
    /// solta ouro pergunta por `Ouro`. Uma função que devolvesse "o bônus
    /// dele" obrigaria cada chamador a comparar, e um `match` esquecido
    /// pagaria ouro com o multiplicador de XP.
    pub(crate) fn mult_magico(&self, sid: SessionId, qual: shared::magica::Bonus) -> f32 {
        match self.bonus_magico_de(sid) {
            Some(b) if b == qual => b.multiplicador(),
            _ => 1.0,
        }
    }

    /// O painel: passes, relógio e o chão de agora.
    pub(crate) fn abrir_magica(&self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let dentro = self.na_magica();
        let bonus = self
            .bonus_magico_de(sid)
            .map_or(255, |b| shared::magica::Bonus::indice(b));
        let agora = (now_ms() / 1000) as i64;
        // O DEGRAU SAI DAQUI, e não do cliente. A trava é do servidor; uma
        // tela que decidisse sozinha mostraria botão que o servidor recusa.
        let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
        let grau_maximo = shared::magica::maior_liberado(nivel).map_or(0, |n| n.grau);
        let grau_atual = shared::magica::nivel_da_zona(&self.zona).map_or(0, |n| n.grau);
        let _ = s.handle.to_client.send(ServerMessage::Magica {
            aviso: shared::magica::AvisoMagica::Estado {
                grau_maximo,
                grau_atual,
                meu_nivel: nivel,
                passes: self.passes_de(sid),
                gratis: shared::magica::gratis_restantes(s.magica_gratis, agora),
                fim_unix: s.magica_ate,
                dentro,
                bonus,
            },
        });
    }

    /// Gasta `entradas` passes e vai. Qualquer recusa não gasta nada.
    fn entrar_na_magica(&mut self, sid: SessionId, entradas: u8, grau: u8) {
        if self.tutorial_mode || self.dungeon_mode {
            self.avisa_magica(sid, "A Ilha Mágica não abre daqui.");
            return;
        }
        let dentro = self.na_magica();
        let agora = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if !s.logged_in || s.instancia != 0 || s.downed {
            return;
        }
        // O DEGRAU, e o PORTÃO DE NÍVEL dele.
        //
        // Conferido aqui, antes de gastar qualquer passe: recusar depois
        // deixaria o jogador sem passe e sem ilha. `grau == 0` significa "o
        // maior que eu posso" — é o que o "+" da tarja manda, onde não há
        // onde escolher.
        let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
        let Some(alvo) = (if grau == 0 {
            shared::magica::maior_liberado(nivel)
        } else {
            shared::magica::NIVEIS.iter().find(|n| n.grau == grau)
        }) else {
            self.avisa_magica(
                sid,
                &format!(
                    "A Ilha Mágica abre no nível {}.",
                    shared::magica::NIVEIS[0].exige_nivel
                ),
            );
            return;
        };
        if nivel < alvo.exige_nivel {
            let msg = format!(
                "{} abre no nível {} (você: {nivel}).",
                alvo.nome, alvo.exige_nivel
            );
            self.avisa_magica(sid, &msg);
            return;
        }
        if entradas == 0 {
            if dentro || shared::magica::resta(s.magica_ate, agora) == 0 {
                self.avisa_magica(sid, "Não há tempo ativo para retomar.");
                return;
            }
            if self
                .diretorio
                .as_ref()
                .and_then(|d| d.melhor(alvo.zona))
                .is_none()
            {
                self.avisa_magica(sid, &format!("{} está fechada no momento.", alvo.nome));
                return;
            }
            if let Some(s) = self.sessions.get_mut(&sid) {
                s.magica_volta = self.zona.clone();
            }
            self.save_pending = true;
            self.mandar_para_zona(
                sid,
                alvo.zona,
                shared::magica::CHEGADA,
                Some("Você retoma seu tempo na Ilha Mágica."),
                Some(alvo.nome),
            );
            return;
        }
        // A COTA DIÁRIA ENTRA NA CONTA, e é gasta PRIMEIRO.
        //
        // O dono: "a Ilha Mágica terá 3 passes por dia de 30 min grátis".
        // Gastar o item antes do que é de graça seria cobrar de quem tinha
        // crédito — e o jogador só descobriria olhando a bolsa depois.
        let passes = self.passes_de(sid);
        let gratis = shared::magica::gratis_restantes(s.magica_gratis, agora);
        let disponivel = passes.saturating_add(gratis as u32);
        if let Err(motivo) = shared::magica::pode_entrar(s.magica_ate, agora, disponivel, entradas)
        {
            self.avisa_magica(sid, &motivo);
            return;
        }
        let de_graca = gratis.min(entradas);
        let do_item = entradas - de_graca;
        // A ZONA TEM QUE ESTAR NO AR antes de gastar o passe. `mandar_para_zona`
        // também confere e recusa, mas ali o passe já teria sumido — e o
        // jogador ficaria sem passe e sem ilha.
        if !dentro
            && self
                .diretorio
                .as_ref()
                .and_then(|d| d.melhor(alvo.zona))
                .is_none()
        {
            let msg = format!("{} está fechada no momento.", alvo.nome);
            self.avisa_magica(sid, &msg);
            return;
        }
        let volta = self.zona.clone();
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if de_graca > 0 {
            s.magica_gratis = shared::magica::apos_gastar_gratis(s.magica_gratis, agora, de_graca);
        }
        if do_item > 0 {
            dungeon::tirar_item(
                &mut s.inventory,
                shared::item_id::PASSE_MAGICO,
                do_item as u32,
            );
            s.inventory_dirty = true;
        }
        s.magica_ate = shared::magica::fim_apos_entrar(s.magica_ate, agora, entradas);
        if !dentro {
            s.magica_volta = volta;
        }
        // Os avisos do relogio valem de novo: quem gastou mais um passe
        // merece ouvir "5 minutos" outra vez.
        s.magica_avisado = i64::MAX;
        let minutos = shared::magica::resta(s.magica_ate, agora) / 60;
        let nome = s.name.clone();
        self.save_pending = true;
        if dentro {
            self.abrir_magica(sid);
            let _ = self
                .sessions
                .get(&sid)
                .unwrap()
                .handle
                .to_client
                .send(ServerMessage::Chat {
                    from: "System".into(),
                    text: format!("Tempo da Ilha Mágica estendido: {minutos} minutos restantes."),
                });
            return;
        }
        let aviso = format!("Você entra na {} — {minutos} minutos.", alvo.nome);
        crate::telemetria::conta("magica_entrada", self.zona.clone(), entradas as i64);
        tracing::info!(
            "{nome}: entra na Ilha Mágica por {minutos} min ({de_graca} de graça, {do_item} passe(s))"
        );
        self.mandar_para_zona(
            sid,
            alvo.zona,
            shared::magica::CHEGADA,
            Some(&aviso),
            Some(alvo.nome),
        );
    }

    fn trocar_degrau_magico(&mut self, sid: SessionId, grau: u8) {
        if !self.na_magica() {
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if !s.logged_in || s.downed || s.instancia != 0 {
            return;
        }
        let agora = (now_ms() / 1000) as i64;
        if shared::magica::resta(s.magica_ate, agora) == 0 {
            self.avisa_magica(sid, "Seu tempo na Ilha Mágica acabou.");
            return;
        }
        let nivel = shared::level_of_xp_with_mult(s.xp, crate::economy::xp_multiplier());
        let Some(alvo) = shared::magica::NIVEIS.iter().find(|n| n.grau == grau) else {
            self.avisa_magica(sid, "Degrau desconhecido.");
            return;
        };
        if nivel < alvo.exige_nivel {
            self.avisa_magica(
                sid,
                &format!("{} abre no nível {}.", alvo.nome, alvo.exige_nivel),
            );
            return;
        }
        if alvo.zona == self.zona {
            self.avisa_magica(sid, "Você já está neste degrau.");
            return;
        }
        if self
            .diretorio
            .as_ref()
            .and_then(|d| d.melhor(alvo.zona))
            .is_none()
        {
            self.avisa_magica(sid, &format!("{} está fechada no momento.", alvo.nome));
            return;
        }
        self.mandar_para_zona(
            sid,
            alvo.zona,
            shared::magica::CHEGADA,
            Some(&format!(
                "Você viaja para a {}. O relógio continua.",
                alvo.nome
            )),
            Some(alvo.nome),
        );
    }

    /// Volta pra zona de onde entrou. O relógio NÃO para.
    fn sair_da_magica(&mut self, sid: SessionId, aviso: &str) {
        if !self.na_magica() {
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        // Vazio (entrou antes desta coluna existir, ou o processo caiu) volta
        // pro Bosque: ficar preso numa ilha de evento é bem pior que chegar
        // na ilha errada.
        let volta = if s.magica_volta.is_empty() {
            shared::terreno::ARQUIPELAGO[0].zona.to_string()
        } else {
            s.magica_volta.clone()
        };
        let Some(def) = shared::terreno::def_da_zona(&volta) else {
            return;
        };
        let chegada = shared::terreno::Gerador::da_ilha(def)
            .cidade()
            .map(|c| c.centro())
            .unwrap_or(Vec2::ZERO);
        self.mandar_para_zona(sid, def.zona, chegada, Some(aviso), Some(def.nome));
    }

    /// O relógio, uma vez por segundo: quem estourou sai.
    ///
    /// Avisa nos últimos minutos, porque ser teleportado no meio de uma luta
    /// sem aviso nenhum é a diferença entre "acabou meu tempo" e "o jogo me
    /// chutou".
    pub(crate) fn tick_magica(&mut self) {
        if !self.na_magica() {
            return;
        }
        let agora = (now_ms() / 1000) as i64;
        let sids: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.logged_in)
            .map(|(id, _)| *id)
            .collect();
        for sid in sids {
            let Some(s) = self.sessions.get_mut(&sid) else {
                continue;
            };
            let resta = shared::magica::resta(s.magica_ate, agora);
            if resta == 0 {
                s.magica_ate = 0;
                self.sair_da_magica(sid, "Seu tempo na Ilha Mágica acabou.");
                continue;
            }
            // O ESTADO VAI QUANDO MUDA, e é isto que acende a tarja.
            //
            // Antes ele só saía quando o jogador ABRIA o painel. Depois do
            // handoff pra dentro da ilha o cliente nunca recebia
            // `dentro: true`, e a tarja do HUD ficava invisível a sessão
            // inteira — sem ilhota, sem sair, sem estender. O dono: "não tô
            // vendo a HUD da ilha mágica dentro dela, nem o botão de sair nem
            // estender".
            //
            // Só quando MUDA (entrou, ou trocou de ilhota). Este tick é 1x
            // por segundo, então mandar sempre seria uma mensagem por
            // jogador por segundo, a sessão inteira, repetindo o que o
            // cliente já sabe. O preço é a tarja demorar até 1s pra trocar
            // ao cruzar de ilhota, que ninguém percebe andando.
            let bonus = self
                .bonus_magico_de(sid)
                .map_or(255, shared::magica::Bonus::indice);
            let mudou = self
                .sessions
                .get(&sid)
                .is_some_and(|s| s.magica_bonus_visto != bonus);
            if mudou {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.magica_bonus_visto = bonus;
                }
                self.abrir_magica(sid);
            }
            let Some(s) = self.sessions.get_mut(&sid) else {
                continue;
            };
            // Um aviso por marco, e só uma vez cada: `avisado_em` guarda o
            // último marco falado, senão o chat viraria uma contagem
            // regressiva de um aviso por segundo.
            for marco in [300i64, 60, 10] {
                if resta <= marco && s.magica_avisado > marco {
                    s.magica_avisado = marco;
                    let texto = if marco >= 60 {
                        format!("Ilha Mágica: {} minuto(s) restantes.", marco / 60)
                    } else {
                        format!("Ilha Mágica: {marco} segundos restantes.")
                    };
                    let _ = s.handle.to_client.send(ServerMessage::Chat {
                        from: "Sistema".into(),
                        text: texto,
                    });
                    break;
                }
            }
        }
    }

    /// A sessão de uma entidade de jogador.
    pub(crate) fn sid_do_eid(&self, eid: EntityId) -> Option<SessionId> {
        self.sessions
            .iter()
            .find(|(_, s)| s.entity_id == eid && s.logged_in)
            .map(|(id, _)| *id)
    }

    /// Os três multiplicadores que um ABATE paga a quem está na Ilha Mágica:
    /// (XP, ouro, chance de drop). Fora dela, `(1.0, 1.0, 1.0)`.
    ///
    /// Devolvidos juntos porque quem mata precisa dos três no mesmo ponto, e
    /// buscar a sessão três vezes no meio do laço de abate seria três
    /// varreduras por mob morto.
    pub(crate) fn mults_de_abate(&self, eid: EntityId, chefe: bool) -> (f32, f32, f32) {
        use shared::magica::Bonus;
        let Some(sid) = self.sid_do_eid(eid) else {
            return (1.0, 1.0, 1.0);
        };
        let drop = if chefe {
            Bonus::DropDeChefe
        } else {
            Bonus::DropDeMob
        };
        (
            self.mult_magico(sid, Bonus::Xp),
            self.mult_magico(sid, Bonus::Ouro),
            self.mult_magico(sid, drop),
        )
    }

    /// Onde quem morre na Ilha Mágica renasce: a CHEGADA.
    ///
    /// O dono pediu "morrer volta pra entrada da própria ilha". Renascer na
    /// ilhota em que caiu devolveria o jogador ao pé de quem o matou, e o
    /// PvP aberto viraria uma fila de execução; renascer na chegada cobra a
    /// travessia de volta, que é o preço de ter morrido.
    pub(crate) fn renascimento_magico(&self) -> Option<Vec2> {
        self.na_magica().then_some(shared::magica::CHEGADA)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    /// Um mundo com um jogador logado, na zona pedida.
    fn mundo(zona: &str) -> (GameWorld, SessionId) {
        let (w, sid, _rx) = mundo_com_rx(zona);
        (w, sid)
    }

    /// Igual, mas SEGURANDO a ponta do cliente. `mundo` deixa o `rx` cair, e
    /// canal fechado faz todo `send` falhar calado — ou seja, quem quer ler o
    /// que o servidor mandou precisa desta versão.
    fn mundo_com_rx(
        zona: &str,
    ) -> (
        GameWorld,
        SessionId,
        mpsc::UnboundedReceiver<shared::protocol::ServerMessage>,
    ) {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = zona.to_string();
        let sid = SessionId(([127, 0, 0, 1], 19_960).into());
        let (tx, _rx) = mpsc::unbounded_channel::<shared::protocol::ServerMessage>();
        w.on_connect(SessionHandle {
            id: sid,
            to_client: tx,
        });
        let e = w.ecs.spawn((
            NetId(EntityId(901)),
            Position(shared::magica::CHEGADA),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
        ));
        let s = w.sessions.get_mut(&sid).unwrap();
        s.logged_in = true;
        s.name = "passageiro".into();
        s.entity = Some(e);
        s.entity_id = EntityId(901);
        s.inventory = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
        (w, sid, _rx)
    }

    fn dar_passes(w: &mut GameWorld, sid: SessionId, n: u32) {
        let s = w.sessions.get_mut(&sid).unwrap();
        s.inventory[0] = shared::InventorySlot {
            item_id: shared::item_id::PASSE_MAGICO,
            qty: n,
            ..Default::default()
        };
    }

    fn poe_em(w: &mut GameWorld, sid: SessionId, p: Vec2) {
        let e = w.sessions[&sid].entity.unwrap();
        w.ecs.get::<&mut Position>(e).unwrap().0 = p;
    }

    #[test]
    fn guia_abre_painel_para_trocar_degrau() {
        let (mut w, sid, mut rx) = mundo_com_rx(shared::magica::ZONA);
        let p = shared::magica::posto_dos_degraus();
        poe_em(&mut w, sid, p);
        let eid = EntityId(902);
        w.ecs.spawn((
            NetId(eid),
            Position(p),
            EntityKind::Npc(9),
            NpcDaVilaTag {
                nome: shared::magica::GUIA_DOS_DEGRAUS.into(),
                rumo: 0,
                giver: None,
            },
        ));
        w.interagir(sid, Some(eid.0 as u64), false);
        assert!(matches!(
            rx.try_recv(),
            Ok(ServerMessage::Magica {
                aviso: shared::magica::AvisoMagica::AbrirPainel
            })
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ServerMessage::Magica {
                aviso: shared::magica::AvisoMagica::Estado { dentro: true, .. }
            })
        ));
    }

    #[test]
    fn tempo_ativo_permite_voltar_sem_nova_entrada() {
        let (mut w, sid, mut rx) = mundo_com_rx("ilha_deserto");
        w.diretorio = Some(crate::canais::Diretorio::para_teste(&[
            shared::magica::ZONA,
        ]));
        let agora = (now_ms() / 1000) as i64;
        let s = w.sessions.get_mut(&sid).unwrap();
        s.xp = shared::xp_for_level_with_mult(30, crate::economy::xp_multiplier());
        s.magica_ate = agora + 600;
        let gratis = s.magica_gratis;
        w.handle_magica(
            sid,
            shared::magica::PedidoMagica::Entrar {
                entradas: 0,
                grau: 1,
            },
        );
        assert_eq!(w.passes_de(sid), 0);
        assert_eq!(w.sessions[&sid].magica_gratis, gratis);
        assert_eq!(w.sessions[&sid].magica_ate, agora + 600);
        assert_eq!(w.sessions[&sid].magica_volta, "ilha_deserto");
        assert!(std::iter::from_fn(|| rx.try_recv().ok()).any(
            |m| matches!(m, ServerMessage::TrocarZona { zona, .. } if zona == shared::magica::ZONA)
        ));
    }

    #[test]
    fn troca_degrau_preserva_relogio_e_volta() {
        let (mut w, sid, mut rx) = mundo_com_rx(shared::magica::ZONA);
        w.diretorio = Some(crate::canais::Diretorio::para_teste(&["ilha_magica_2"]));
        let agora = (now_ms() / 1000) as i64;
        let s = w.sessions.get_mut(&sid).unwrap();
        s.xp = shared::xp_for_level_with_mult(30, crate::economy::xp_multiplier());
        s.magica_ate = agora + 600;
        s.magica_volta = "ilha_deserto".into();
        w.handle_magica(sid, shared::magica::PedidoMagica::Trocar { grau: 2 });
        assert_eq!(w.sessions[&sid].magica_ate, agora + 600);
        assert_eq!(w.sessions[&sid].magica_volta, "ilha_deserto");
        assert!(std::iter::from_fn(|| rx.try_recv().ok()).any(
            |m| matches!(m, ServerMessage::TrocarZona { zona, .. } if zona == "ilha_magica_2")
        ));
    }

    #[test]
    fn estender_dentro_nao_troca_a_zona_de_volta() {
        let (mut w, sid) = mundo(shared::magica::ZONA);
        let agora = (now_ms() / 1000) as i64;
        let s = w.sessions.get_mut(&sid).unwrap();
        s.xp = shared::xp_for_level_with_mult(30, crate::economy::xp_multiplier());
        s.magica_ate = agora + 600;
        s.magica_volta = "ilha_deserto".into();
        w.handle_magica(
            sid,
            shared::magica::PedidoMagica::Entrar {
                entradas: 1,
                grau: 1,
            },
        );
        assert_eq!(
            w.sessions[&sid].magica_ate,
            agora + 600 + shared::magica::DURACAO_S
        );
        assert_eq!(w.sessions[&sid].magica_volta, "ilha_deserto");
    }

    /// RECUSA NÃO GASTA. É a propriedade que mais importa: um passe é meia
    /// hora, e sumir com ele sem entregar a ilha é o pior defeito possível
    /// aqui — o jogador não tem como saber que perdeu.
    #[test]
    fn recusar_a_entrada_nao_queima_o_passe() {
        let (mut w, sid) = mundo("ilha_inicial");
        dar_passes(&mut w, sid, 2);
        // Sem canal da Ilha Mágica no ar (não há diretório neste mundo de
        // teste), a entrada tem que ser recusada ANTES de gastar.
        w.handle_magica(
            sid,
            shared::magica::PedidoMagica::Entrar {
                entradas: 1,
                grau: 1,
            },
        );
        assert_eq!(w.passes_de(sid), 2, "o passe sumiu sem entregar a ilha");
        assert_eq!(w.sessions[&sid].magica_ate, 0, "o relógio começou sem ida");

        // Pedir mais passes do que tem também não gasta nada.
        w.handle_magica(
            sid,
            shared::magica::PedidoMagica::Entrar {
                entradas: 3,
                grau: 1,
            },
        );
        assert_eq!(w.passes_de(sid), 2);
        // Nem um número que não existe.
        w.handle_magica(
            sid,
            shared::magica::PedidoMagica::Entrar {
                entradas: 9,
                grau: 1,
            },
        );
        assert_eq!(w.passes_de(sid), 2);
    }

    /// A COTA DE GRAÇA É GASTA ANTES DO ITEM.
    ///
    /// Cobrar o passe de quem tinha crédito é o tipo de erro que o jogador só
    /// descobre olhando a bolsa depois, e aí já era. Como a entrada de
    /// verdade exige a zona no ar (que não há no mundo de teste), o teste
    /// mede as DUAS coisas que `entrar_na_magica` decide antes de viajar: a
    /// conta do que está disponível e a repartição entre grátis e item.
    #[test]
    fn a_entrada_de_graca_sai_antes_do_passe() {
        use shared::magica as m;
        let agora = (now_ms() / 1000) as i64;

        // Dia virgem: três de graça, e elas bastam para as três entradas.
        let g0 = 0i64;
        assert_eq!(m::gratis_restantes(g0, agora), m::GRATIS_POR_DIA);
        assert!(m::pode_entrar(0, agora, m::GRATIS_POR_DIA as u32, 3).is_ok());

        // Uma entrada de graça: sobram duas, e o item não foi tocado.
        let g1 = m::apos_gastar_gratis(g0, agora, 1);
        assert_eq!(m::gratis_restantes(g1, agora), 2);

        // Gastas as três, quem entra de novo paga com ITEM — e sem item a
        // entrada é recusada, mesmo com a cota zerada (não vira dívida).
        let g3 = m::apos_gastar_gratis(g0, agora, 3);
        assert_eq!(m::gratis_restantes(g3, agora), 0);
        assert!(
            m::pode_entrar(0, agora, 0, 1).is_err(),
            "sem cota e sem passe, não entra"
        );
        assert!(m::pode_entrar(0, agora, 1, 1).is_ok(), "com passe, entra");

        // E a repartição: pedindo 3 com 1 de graça restando, 1 sai da cota e
        // 2 do item — que é o que `entrar_na_magica` calcula.
        let gratis: u8 = 1;
        let entradas: u8 = 3;
        let de_graca = gratis.min(entradas);
        assert_eq!((de_graca, entradas - de_graca), (1, 2));
    }

    /// O PAINEL conta a cota junto com a bolsa.
    #[test]
    fn o_painel_mostra_a_cota_do_dia() {
        let (mut w, sid) = mundo("ilha_inicial");
        dar_passes(&mut w, sid, 2);
        assert_eq!(w.passes_de(sid), 2);
        let agora = (now_ms() / 1000) as i64;
        let s = w.sessions.get_mut(&sid).unwrap();
        // Personagem novo (coluna zerada) começa com a cota cheia.
        assert_eq!(
            shared::magica::gratis_restantes(s.magica_gratis, agora),
            shared::magica::GRATIS_POR_DIA
        );
        // Gastou tudo hoje: a cota zera e só a bolsa conta.
        s.magica_gratis = shared::magica::apos_gastar_gratis(0, agora, 3);
        let g = shared::magica::gratis_restantes(w.sessions[&sid].magica_gratis, agora);
        assert_eq!(g, 0);
        assert_eq!(w.passes_de(sid) + g as u32, 2);
    }

    /// Dentro da ilha, a ilhota em que se está é que paga.
    ///
    /// Anda pelas sete e confere que cada uma paga o SEU bônus e mais
    /// nenhum: é o acoplamento que uma tabela de multiplicadores erra calada
    /// — ouro pago com o multiplicador de XP não dá erro nenhum, só um número
    /// errado.
    #[test]
    fn cada_ilhota_paga_so_o_bonus_dela() {
        use shared::magica::Bonus;
        let (mut w, sid) = mundo(shared::magica::ZONA);
        let agora = (now_ms() / 1000) as i64;
        w.sessions.get_mut(&sid).unwrap().magica_ate = agora + 600;

        let todos = [
            Bonus::Xp,
            Bonus::DropDeMob,
            Bonus::Ouro,
            Bonus::DropDeChefe,
            Bonus::Coleta(0),
            Bonus::Coleta(1),
            Bonus::Coleta(5),
        ];
        for i in shared::magica::ilhotas() {
            poe_em(&mut w, sid, i.centro);
            assert_eq!(w.bonus_magico_de(sid), Some(i.bonus), "{}", i.bonus.nome());
            for b in todos {
                let m = w.mult_magico(sid, b);
                if b == i.bonus {
                    assert!(m > 1.0, "{}: não paga o próprio bônus", i.bonus.nome());
                } else {
                    assert_eq!(m, 1.0, "{} pagou o bônus de {}", i.bonus.nome(), b.nome());
                }
            }
        }

        // NA PONTE não paga nada: quem atravessa está entre dois lugares.
        let v = shared::magica::ilhotas();
        poe_em(&mut w, sid, v[0].centro.lerp(v[1].centro, 0.5));
        assert_eq!(w.bonus_magico_de(sid), None);

        // RELÓGIO VENCIDO corta o bônus na hora, antes de o tick expulsar.
        poe_em(&mut w, sid, v[1].centro);
        assert!(w.bonus_magico_de(sid).is_some());
        w.sessions.get_mut(&sid).unwrap().magica_ate = agora - 1;
        assert_eq!(
            w.bonus_magico_de(sid),
            None,
            "tempo vencido ainda pagava bônus"
        );
    }

    /// A ilha nasce POVOADA: bicho pra matar e o Colosso pra caçar.
    ///
    /// Três das sete ilhotas pagam bônus de abate (XP, espólio, ouro) e uma
    /// paga drop de CHEFE. Numa ilha vazia as quatro são promessa sem nada
    /// atrás — e o jogador só descobre depois de gastar meia hora de passe
    /// procurando.
    ///
    /// Este é também o teste que prova que `povoar_ilha` funciona numa zona
    /// que não está no `ARQUIPELAGO`: ela chega pelo `def_da_zona`, e era
    /// exatamente ali que uma zona de fora sairia calada pelo `else { return }`.
    #[test]
    fn a_ilha_magica_nasce_com_bicho_e_com_chefe() {
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = shared::magica::ZONA.to_string();
        w.ilha = Some(shared::terreno::Ilha::da_ilha_magica());
        w.povoar_ilha(shared::magica::CHEGADA);

        // As ZONAS DE SPAWN, e não as entidades: a economia do teste é vazia
        // (`init_vazia_para_testes` não registra `enemy_kinds`), então nenhum
        // bicho comum chega a nascer aqui. A zona é o que `povoar_ilha`
        // produz, e é ela que o `tick_spawn_zones` enche em produção.
        let zonas = w.spawn_zones.len();
        let chefes = w
            .ecs
            .query::<&EntityKind>()
            .iter()
            .filter(|(_, k)| matches!(k, EntityKind::Enemy(k) if shared::bosses::e_chefe(*k)))
            .count();
        println!("Ilha Mágica: {zonas} zonas de spawn, {chefes} chefe(s)");
        assert!(
            zonas > 0,
            "ilha sem zona de spawn: XP, espólio e ouro não rendem — \
             `povoar_ilha` saiu calada pelo `else {{ return }}`"
        );
        assert!(
            chefes > 0,
            "ilha sem chefe: a Ilhota do Colosso paga drop de chefe"
        );
        // TODA zona e TODO chefe em chão de ilhota ou ponte: bicho no fundo
        // do mar conta pro total e não se mata.
        for z in &w.spawn_zones {
            let c = z.origin + z.size * 0.5;
            assert!(
                shared::magica::e_chao(c),
                "zona de spawn no mar, em ({:.0},{:.0})",
                c.x,
                c.y
            );
        }
        for (_, (kind, pos)) in w.ecs.query::<(&EntityKind, &Position)>().iter() {
            if matches!(kind, EntityKind::Enemy(k) if shared::bosses::e_chefe(*k)) {
                assert!(
                    shared::magica::e_chao(pos.0),
                    "o chefe nasceu fora do chão, em ({:.0},{:.0})",
                    pos.0.x,
                    pos.0.y
                );
                // E NA ILHOTA DO COLOSSO, que é a que paga drop de chefe.
                //
                // No primeiro boot em produção ele nasceu em (80,-120) — a
                // Ilhota do Espólio. `sitios_de_chefe` procura chão plano
                // longe do porto e das zonas seguras, que é boa regra numa
                // ilha de verdade e a regra errada aqui: matar o chefe noutra
                // ilhota deixa o bônus de drop DELE inalcançável, e a ilhota
                // mais cobiçada do desenho vira a única impossível de usar.
                //
                // Este teste existia e não pegou: ele perguntava "nasceu
                // chefe?", e nascia. Achado lendo o log do boot.
                assert_eq!(
                    shared::magica::bonus_em(pos.0),
                    Some(shared::magica::Bonus::DropDeChefe),
                    "o chefe nasceu em ({:.0},{:.0}), fora da Ilhota do Colosso",
                    pos.0.x,
                    pos.0.y
                );
            }
        }
    }

    /// FORA da Ilha Mágica nada disso vale — nem com o relógio cheio.
    #[test]
    fn fora_da_ilha_o_relogio_nao_paga_nada() {
        let (mut w, sid) = mundo("ilha_inicial");
        w.sessions.get_mut(&sid).unwrap().magica_ate = (now_ms() / 1000) as i64 + 3_600;
        poe_em(&mut w, sid, shared::magica::CHEGADA);
        assert_eq!(w.bonus_magico_de(sid), None);
        assert_eq!(w.mult_magico(sid, shared::magica::Bonus::Xp), 1.0);
        assert!(w.renascimento_magico().is_none());
    }

    /// Na Ilha Mágica atacar também exige Hostil; o alvo pode estar Pacífico.
    #[test]
    fn na_ilha_magica_todo_mundo_pode_bater_em_todo_mundo() {
        let (mut w, sid) = mundo(shared::magica::ZONA);
        let outro = SessionId(([127, 0, 0, 1], 19_961).into());
        let (tx, _rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: outro,
            to_client: tx,
        });
        let e = w.ecs.spawn((
            NetId(EntityId(902)),
            Position(shared::magica::CHEGADA),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
        ));
        // FORA DA CHEGADA: ela virou porto seguro (ver
        // `na_chegada_ninguem_bate_em_ninguem`), e o que este teste mede é o
        // PvP aberto das ilhotas de combate.
        let combate = *shared::magica::centros_de_combate()
            .first()
            .expect("a ilha tem ilhota de combate");
        poe_em(&mut w, sid, combate);
        w.ecs.get::<&mut Position>(e).unwrap().0 = combate;
        let faccao = w.sessions[&sid].faction;
        {
            let s = w.sessions.get_mut(&outro).unwrap();
            s.logged_in = true;
            s.name = "rival".into();
            s.entity = Some(e);
            s.entity_id = EntityId(902);
            s.faction = faccao;
            s.pk_mode_on = false;
        }
        w.sessions.get_mut(&sid).unwrap().pk_mode_on = true;
        assert!(
            w.can_damage_player(EntityId(901), EntityId(902)),
            "Hostil pode atacar um alvo Pacífico na Ilha Mágica"
        );
        // Desligar Hostil impede o ataque também fora da ilha.
        w.sessions.get_mut(&sid).unwrap().pk_mode_on = false;
        w.zona = "ilha_inicial".into();
        assert!(!w.can_damage_player(EntityId(901), EntityId(902)));
    }
    /// A TARJA TEM QUE ACENDER SOZINHA.
    ///
    /// O estado da ilha só saía quando o jogador ABRIA o painel. Mas quem
    /// está dentro chegou por handoff, numa sessão nova que nunca pediu
    /// painel nenhum — então o cliente nunca recebia `dentro: true` e a tarja
    /// do HUD (ilhota, relógio, "+" e "Sair") ficava invisível a sessão
    /// inteira. O dono: "não tô vendo a HUD da ilha mágica dentro dela, nem o
    /// botão de sair nem estender".
    ///
    /// O teste é do TICK, e não do painel, justamente porque o painel já
    /// funcionava: o defeito era não existir nenhum outro caminho.
    #[test]
    fn o_tick_acende_a_tarja_de_quem_esta_dentro() {
        use shared::magica::AvisoMagica;
        use shared::protocol::ServerMessage;

        let (mut w, sid, mut rx) = mundo_com_rx(shared::magica::ZONA);
        let agora = (now_ms() / 1000) as i64;
        w.sessions.get_mut(&sid).unwrap().magica_ate = agora + 600;

        w.tick_magica();

        let estado = std::iter::from_fn(|| rx.try_recv().ok())
            .find_map(|m| match m {
                ServerMessage::Magica {
                    aviso: AvisoMagica::Estado { dentro, bonus, .. },
                } => Some((dentro, bonus)),
                _ => None,
            })
            .expect("ninguém mandou o Estado: a tarja fica invisível dentro da ilha");
        assert!(estado.0, "mandou `dentro: false` de dentro da ilha");
        assert_ne!(estado.1, 255, "sem bônus não há nome de ilhota na tarja");

        // E MANDA UMA VEZ SÓ. O tick é 1x por segundo: repetir o mesmo
        // estado seria uma mensagem por segundo, por jogador, a sessão
        // toda, pra dizer o que o cliente já sabe.
        for _ in 0..5 {
            w.tick_magica();
        }
        let repetidos = std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|m| {
                matches!(
                    m,
                    ServerMessage::Magica {
                        aviso: AvisoMagica::Estado { .. }
                    }
                )
            })
            .count();
        assert_eq!(
            repetidos, 0,
            "{repetidos} estados repetidos em 5 ticks parados"
        );
    }

    /// TROCOU DE ILHOTA, A TARJA TROCA JUNTO.
    ///
    /// É o outro motivo de a tarja existir: dizer em qual ilhota o jogador
    /// está e o que ela dá. Se só a entrada mandasse o estado, a tarja diria
    /// "Madeira" para sempre, inclusive dentro da ilhota de XP.
    #[test]
    fn andar_pra_outra_ilhota_atualiza_a_tarja() {
        use shared::magica::AvisoMagica;
        use shared::protocol::ServerMessage;

        let (mut w, sid, mut rx) = mundo_com_rx(shared::magica::ZONA);
        let agora = (now_ms() / 1000) as i64;
        w.sessions.get_mut(&sid).unwrap().magica_ate = agora + 600;
        w.tick_magica();
        let primeiro = ultimo_bonus(&mut rx).expect("nem o primeiro estado saiu");

        // Um centro de combate é, por construção, outra ilhota que a chegada.
        let outra = *shared::magica::centros_de_combate()
            .first()
            .expect("a ilha mágica não tem ilhota de combate");
        poe_em(&mut w, sid, outra);
        w.tick_magica();

        let depois = ultimo_bonus(&mut rx).expect("mudou de ilhota e a tarja não soube");
        assert_ne!(
            primeiro, depois,
            "a tarja continuou anunciando o bônus da ilhota anterior"
        );
    }

    fn ultimo_bonus(
        rx: &mut mpsc::UnboundedReceiver<shared::protocol::ServerMessage>,
    ) -> Option<u8> {
        use shared::magica::AvisoMagica;
        use shared::protocol::ServerMessage;
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter_map(|m| match m {
                ServerMessage::Magica {
                    aviso: AvisoMagica::Estado { bonus, .. },
                } => Some(bonus),
                _ => None,
            })
            .last()
    }

    /// A ILHOTA DA CHEGADA É PORTO SEGURO — e só ela.
    ///
    /// O dono: "lá tem que ser PvP desativado". É onde se chega e onde se
    /// volta ao morrer: bater em quem acabou de renascer, sem poção e sem
    /// chance, não é disputa de ilhota, é camping de respawn.
    ///
    /// As outras seis continuam abertas: é delas que o PvP da ilha é feito.
    #[test]
    fn na_chegada_ninguem_bate_em_ninguem() {
        let (mut w, sid) = mundo(shared::magica::ZONA);
        let outro = SessionId(([127, 0, 0, 1], 19_962).into());
        let (tx, _rx) = mpsc::unbounded_channel();
        w.on_connect(SessionHandle {
            id: outro,
            to_client: tx,
        });
        let e = w.ecs.spawn((
            NetId(EntityId(903)),
            Position(shared::magica::CHEGADA),
            Velocity(Vec2::ZERO),
            EntityKind::Player,
            Health {
                current: 100,
                max: 100,
            },
        ));
        {
            let s = w.sessions.get_mut(&outro).unwrap();
            s.logged_in = true;
            s.name = "rival".into();
            s.entity = Some(e);
            s.entity_id = EntityId(903);
        }
        // Os dois na chegada: ninguém bate.
        assert!(
            !w.can_damage_player(EntityId(901), EntityId(903)),
            "bateram dentro do porto seguro da chegada"
        );

        // UM SÓ dentro já basta pra proteger: senão daria pra ficar na borda
        // batendo em quem está dentro, que é o mesmo camping por outro nome.
        let combate = *shared::magica::centros_de_combate()
            .first()
            .expect("a ilha tem ilhota de combate");
        poe_em(&mut w, sid, combate);
        assert!(
            !w.can_damage_player(EntityId(901), EntityId(903)),
            "de fora deu pra bater em quem está no porto seguro"
        );

        // Os dois fora e atacante Hostil: PvP aberto.
        w.sessions.get_mut(&sid).unwrap().pk_mode_on = true;
        w.ecs.get::<&mut Position>(e).unwrap().0 = combate;
        assert!(
            w.can_damage_player(EntityId(901), EntityId(903)),
            "nas ilhotas de combate o PvP tem que continuar aberto"
        );
    }

    /// TEM ONDE COMPRAR POÇÃO NA CHEGADA.
    ///
    /// O dono: "na primeira ilha, na central, tem que ter um NPC de venda de
    /// poções". Sem ele a ilha é até uma hora e meia sem reposição: quem
    /// gastou as poções ou sai (e perde o tempo que pagou) ou passa o resto
    /// do relógio sem poder brigar.
    #[test]
    fn a_chegada_tem_quem_venda_pocao() {
        let (mut w, _sid) = mundo(shared::magica::ZONA);
        w.montar_cidade(None);
        let achou = w
            .ecs
            .query::<(&Position, &VendorTag)>()
            .iter()
            .any(|(_, (p, v))| {
                v.shop_id == shared::vila::LOJA_DE_POCOES && shared::magica::e_porto_seguro(p.0)
            });
        assert!(achou, "a ilhota da chegada ficou sem vendedor de poções");
    }

    /// E O VENDEDOR NÃO NASCE EM CIMA DE QUEM RENASCE.
    ///
    /// A chegada é o ponto de respawn: um NPC plantado no meio receberia
    /// todo mundo em cima dele a cada morte, e o toque nele roubaria o
    /// clique de quem só queria sair correndo.
    #[test]
    fn o_vendedor_nao_fica_em_cima_do_respawn() {
        let p = shared::magica::posto_de_pocoes();
        let d = p.distance(shared::magica::CHEGADA);
        assert!(
            d >= 12.0,
            "o vendedor está a {d:.1} do ponto de renascimento"
        );
        assert!(
            shared::magica::e_porto_seguro(p),
            "o vendedor caiu fora da ilhota da chegada"
        );
    }
}
