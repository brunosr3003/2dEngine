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
            P::Entrar { entradas } => self.entrar_na_magica(sid, entradas),
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
        let _ = s.handle.to_client.send(ServerMessage::Magica {
            aviso: shared::magica::AvisoMagica::Estado {
                passes: self.passes_de(sid),
                fim_unix: s.magica_ate,
                dentro,
                bonus,
            },
        });
    }

    /// Gasta `entradas` passes e vai. Qualquer recusa não gasta nada.
    fn entrar_na_magica(&mut self, sid: SessionId, entradas: u8) {
        if self.tutorial_mode || self.dungeon_mode {
            self.avisa_magica(sid, "A Ilha Mágica não abre daqui.");
            return;
        }
        if self.na_magica() {
            self.avisa_magica(sid, "Você já está na Ilha Mágica.");
            return;
        }
        let agora = (now_ms() / 1000) as i64;
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if !s.logged_in || s.instancia != 0 || s.downed {
            return;
        }
        let passes = self.passes_de(sid);
        if let Err(motivo) = shared::magica::pode_entrar(s.magica_ate, agora, passes, entradas) {
            self.avisa_magica(sid, &motivo);
            return;
        }
        // A ZONA TEM QUE ESTAR NO AR antes de gastar o passe. `mandar_para_zona`
        // também confere e recusa, mas ali o passe já teria sumido — e o
        // jogador ficaria sem passe e sem ilha.
        if self
            .diretorio
            .as_ref()
            .and_then(|d| d.melhor(shared::magica::ZONA))
            .is_none()
        {
            self.avisa_magica(sid, "A Ilha Mágica está fechada no momento.");
            return;
        }
        let volta = self.zona.clone();
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        dungeon::tirar_item(
            &mut s.inventory,
            shared::item_id::PASSE_MAGICO,
            entradas as u32,
        );
        s.inventory_dirty = true;
        s.magica_ate = shared::magica::fim_apos_entrar(s.magica_ate, agora, entradas);
        s.magica_volta = volta;
        // Os avisos do relogio valem de novo: quem gastou mais um passe
        // merece ouvir "5 minutos" outra vez.
        s.magica_avisado = i64::MAX;
        let minutos = shared::magica::resta(s.magica_ate, agora) / 60;
        let nome = s.name.clone();
        self.save_pending = true;
        let aviso = format!("Você entra na Ilha Mágica — {minutos} minutos.");
        crate::telemetria::conta("magica_entrada", self.zona.clone(), entradas as i64);
        tracing::info!("{nome}: entra na Ilha Mágica por {minutos} min ({entradas} passe(s))");
        self.mandar_para_zona(
            sid,
            shared::magica::ZONA,
            shared::magica::CHEGADA,
            Some(&aviso),
            Some("Ilha Mágica"),
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
        crate::economy::init_vazia_para_testes();
        let mut w = GameWorld::new(HashMap::new());
        w.zona = zona.to_string();
        let sid = SessionId(([127, 0, 0, 1], 19_960).into());
        let (tx, _rx) = mpsc::unbounded_channel();
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
        (w, sid)
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

    /// RECUSA NÃO GASTA. É a propriedade que mais importa: um passe é meia
    /// hora, e sumir com ele sem entregar a ilha é o pior defeito possível
    /// aqui — o jogador não tem como saber que perdeu.
    #[test]
    fn recusar_a_entrada_nao_queima_o_passe() {
        let (mut w, sid) = mundo("ilha_inicial");
        dar_passes(&mut w, sid, 2);
        // Sem canal da Ilha Mágica no ar (não há diretório neste mundo de
        // teste), a entrada tem que ser recusada ANTES de gastar.
        w.handle_magica(sid, shared::magica::PedidoMagica::Entrar { entradas: 1 });
        assert_eq!(w.passes_de(sid), 2, "o passe sumiu sem entregar a ilha");
        assert_eq!(w.sessions[&sid].magica_ate, 0, "o relógio começou sem ida");

        // Pedir mais passes do que tem também não gasta nada.
        w.handle_magica(sid, shared::magica::PedidoMagica::Entrar { entradas: 3 });
        assert_eq!(w.passes_de(sid), 2);
        // Nem um número que não existe.
        w.handle_magica(sid, shared::magica::PedidoMagica::Entrar { entradas: 9 });
        assert_eq!(w.passes_de(sid), 2);
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

    /// Na Ilha Mágica o PvP é aberto: nem facção nem PK ON contam.
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
        w.sessions.get_mut(&sid).unwrap().pk_mode_on = false;
        assert!(
            w.can_damage_player(EntityId(901), EntityId(902)),
            "mesma facção e PK OFF: na Ilha Mágica tem que bater mesmo assim"
        );
        // A mesma dupla, FORA dela, continua protegida.
        w.zona = "ilha_inicial".into();
        assert!(!w.can_damage_player(EntityId(901), EntityId(902)));
    }
}
