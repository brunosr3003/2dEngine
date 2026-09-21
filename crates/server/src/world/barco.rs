//! O CASCO no Mar Aberto (docs/MAR_ABERTO.md).
//!
//! Desde 21/09/2026 trocar de ilha exige navegar. Zarpar poe o jogador na
//! zona `mar_aberto` com um casco sob os pes; atracar o devolve pro cais da
//! ilha de destino.
//!
//! # NAO HA' ENTIDADE BARCO
//!
//! A primeira tentativa tinha uma: casco com `Health`, leme, acelerador e o
//! passageiro com a posicao derivada dele. O dono testou e foi direto ao
//! ponto: *"a navegacao ta perdendo o referencial de frente tras que tinha
//! antes"*. Perdia mesmo — leme e acelerador sao um esquema de controle NOVO,
//! e o jogador ja' sabia um.
//!
//! E, decidido que combate no mar e' **PvP entre barcos** e nao luta de
//! conves, o barco deixou de precisar ser um lugar onde se anda. Entao:
//!
//! > **Navegar e' andar, so' que na agua.** O mesmo direcional, o mesmo "pra
//! > frente e' longe da camera". O que muda e' com o que o corpo colide
//! > (terra em vez de agua), a velocidade, e o que o cliente DESENHA no lugar
//! > do boneco.
//!
//! O que sumiu junto: componente de casco, sincronizacao, posicao derivada,
//! altura de conves, `PedidoBarco::Comando` e o tick proprio. Sobrou o
//! movimento que ja' existia.
//!
//! Quando o PvP naval chegar (corte 5), o casco ganha vida propria — mas ai'
//! ela e' do BARCO como item (`BarcoData`, corte 3), e nao de uma entidade
//! separada carregando o jogador.

use super::*;

/// Velocidade do casco, em unidades por segundo.
///
/// 11 e' 2,2x o andar (`PLAYER_SPEED` 5,0) e 1,5x a montaria. O teto duro e'
/// o fio: `EntityState::vel` e' `i8` a `POS_SCALE`, o que da' 15,9 u/s — e
/// acima de ~16 a AOI do mar daria menos de 2,5 s de aviso, que nao da' num
/// celular de 150 ms de atraso.
pub const VEL_MAX: f32 = 11.0;

/// Raio do casco pro teste de agua. Maior que o do corpo a pe': um barco
/// raspando a costa tem que parar antes do boneco pareceer dentro da pedra.
pub const RAIO_CASCO: f32 = 1.4;

impl GameWorld {
    /// O barco equipado e o estado dele.
    pub(super) fn barco_equipado(
        &self,
        sid: SessionId,
    ) -> Option<(u16, shared::items::BarcoData)> {
        let s = self.sessions.get(&sid)?;
        let id = s.equipment.barco?;
        Some((
            id,
            shared::barcos::dados(s.equipment.barco_inst.as_ref(), id),
        ))
    }

    /// Escreve o estado do barco equipado de volta na instancia.
    pub(super) fn guarda_barco(&mut self, sid: SessionId, d: shared::items::BarcoData) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let Some(id) = s.equipment.barco else {
            return;
        };
        let mut inst = s
            .equipment
            .barco_inst
            .unwrap_or_else(|| shared::items::ItemInstance::vazia_de_grau(1));
        inst.barco = Some(d);
        s.equipment.set(shared::EquipSlot::Barco, Some(id), Some(inst));
        s.inventory_dirty = true;
        self.save_pending = true;
    }

    /// Perto do Capitao do Porto? E' quem libera zarpar.
    fn no_cais(&self, sid: SessionId) -> bool {
        let Some(eu) = self.pos_do_jogador(sid) else {
            return false;
        };
        let capitao = shared::construcao::Papel::Estaleiro as u8;
        self.ecs
            .query::<(&Position, &NpcDaVilaTag)>()
            .iter()
            .any(|(_, (p, t))| {
                shared::npc_papel_de_kind(t.rumo) == capitao
                    && p.0.distance(eu) <= shared::viagem::PERTO_DO_CAPITAO
            })
    }

    /// ZARPAR: do porto pro Mar Aberto.
    ///
    /// A posicao de chegada e' a ponta do CAIS desta ilha, na coordenada do
    /// mar. E' a mesma conta que o atracar faz ao contrario, e e' por isso
    /// que as duas moram em `shared::mar`: um erro de sinal aqui poe o
    /// jogador do outro lado do arquipelago.
    fn handle_zarpar(&mut self, sid: SessionId) {
        if !self.no_cais(sid) {
            self.recusa_barco(sid, "Fale com o Capitão do Porto, no cais.");
            return;
        }
        // SEM BARCO NAO SE ZARPA. E' o que faz a quest da historia ser a
        // porta de saida da primeira ilha, e nao um enfeite.
        let Some((id, dados)) = self.barco_equipado(sid) else {
            self.recusa_barco(
                sid,
                "Você precisa de um barco equipado. Fale com o Carpinteiro Naval.",
            );
            return;
        };
        if dados.casco == 0 {
            self.recusa_barco(sid, "Casco destruído. O Carpinteiro Naval conserta.");
            return;
        }
        let _ = id;
        let Some(i) = shared::terreno::ARQUIPELAGO
            .iter()
            .position(|d| d.zona == self.zona)
        else {
            self.recusa_barco(sid, "Daqui não se zarpa.");
            return;
        };
        let mar = shared::mar::Mar::novo();
        let Some(cais) = mar.cais_de(i) else {
            self.recusa_barco(sid, "Esta ilha não tem cais.");
            return;
        };
        // A volta: se o processo do mar cair, e' por aqui que o personagem
        // acha o caminho de casa no proximo login. Gravado ANTES do handoff.
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.zona_volta = Some(self.zona.clone());
        }
        self.mandar_para_zona(
            sid,
            shared::mar::ZONA,
            cais,
            Some("Você solta as amarras e ganha o mar."),
            Some("o Mar Aberto"),
        );
    }

    /// ATRACAR: do mar pra ilha `ilha`.
    ///
    /// Tres recusas, nesta ordem, e cada uma existe por um motivo:
    /// estar perto do cais, a HISTORIA ter liberado a ilha, e o processo
    /// dela estar no ar. A ultima e' a que impede a viagem so' de ida.
    fn handle_atracar(&mut self, sid: SessionId, ilha: u8) {
        let Some(dest) = shared::terreno::ARQUIPELAGO.get(ilha as usize) else {
            return;
        };
        let mar = shared::mar::Mar::novo();
        let Some(cais) = mar.cais_de(ilha as usize) else {
            self.recusa_barco(sid, "Aquela ilha não tem cais.");
            return;
        };
        let Some(eu) = self.pos_do_jogador(sid) else {
            return;
        };
        if eu.distance(cais) > shared::mar::PERTO_DO_CAIS {
            self.recusa_barco(sid, "Chegue mais perto do cais.");
            return;
        }
        // A trava de progressao continua sendo a HISTORIA, nao o perigo do
        // mar: `viagem::liberada` e' a mesma funcao do menu do Capitao, so'
        // que agora perguntada aqui.
        let passo = self
            .sessions
            .get(&sid)
            .and_then(|s| crate::quests::indice_da_historia(&s.quests));
        if !shared::viagem::liberada(ilha as usize, passo) {
            let falta = shared::viagem::passo_que_libera(ilha as usize)
                .map(|(_, t)| format!(" Liberado em \"{t}\"."))
                .unwrap_or_default();
            self.recusa_barco(sid, &format!("A história ainda não te levou a {}.{falta}", dest.nome));
            return;
        }
        // Chega no patio do porto de LA', em coordenada local daquela ilha.
        let ger = shared::terreno::Gerador::da_ilha(dest);
        let chegada = ger
            .porto()
            .map(|p| p.centro)
            .or_else(|| ger.cidade().map(|c| c.centro()))
            .unwrap_or(Vec2::ZERO);
        let aviso = format!("Você atraca em {}.", dest.nome);
        if self.mandar_para_zona(sid, dest.zona, chegada, Some(&aviso), Some(dest.nome)) {
            // O hodometro. Nao faz nada mecanicamente — e' o que o mercado
            // le' quando alguem anuncia o barco.
            if let Some((_, mut d)) = self.barco_equipado(sid) {
                d.travessias = d.travessias.saturating_add(1);
                self.guarda_barco(sid, d);
            }
        }
    }

    /// NAUFRAGIO: o casco chegou a zero.
    ///
    /// A correnteza leva pra ilha MAIS PERTO que esteja no ar e que a
    /// historia ja' tenha liberado. Legivel, calculavel de quatro centros, e
    /// o filtro da historia garante que ninguem acorde onde a historia nao
    /// chegou. Se nada estiver no ar, nao teleporta: fica boiando e e'
    /// avisado — `TrocarZona` pra um host que nao existe e' o unico
    /// desfecho que nao pode acontecer.
    pub(super) fn naufragio(&mut self, sid: SessionId) {
        let Some(eu) = self.pos_do_jogador(sid) else {
            return;
        };
        // O BARCO AFUNDA E O BAU BOIA.
        //
        // Sao DUAS mortes, e elas nunca podem ser confundidas:
        //
        // - casco a zero (bicho, mar): o barco avaria e o bau FICA BOIANDO no
        //   lugar do naufragio, de quem chegar;
        // - morte em PvP: o barco fica INTACTO e o bau troca de dono na hora.
        //
        // O bau nunca e' destruido. Afundar o tesouro faria a estrategia
        // vencedora ser a NEGACAO — bastava furar o casco de quem carrega pra
        // ninguem levar nada, que e' soma negativa e um unico mau ator
        // tornaria a travessia inutil. Boiando, o naufragio vira chamado:
        // quem passar por ali leva.
        let mut boiando: Option<(Vec2, u16)> = None;
        if let Some((_, mut d)) = self.barco_equipado(sid) {
            let perdeu = d.carga;
            d.casco = 0;
            d.afundou = d.afundou.saturating_add(1);
            d.carga = 0;
            d.carga_de = 0;
            self.guarda_barco(sid, d);
            if perdeu != 0 {
                boiando = Some((eu, perdeu));
                self.recusa_barco(sid, "O baú boia nos destroços. Quem chegar primeiro leva.");
            }
        }
        if let Some((onde, bau)) = boiando {
            let eid = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(eid),
                Position(onde),
                Velocity(Vec2::ZERO),
                EntityKind::Loot(bau),
                LootTag {
                    item_id: bau,
                    qty: 1,
                    instance: None,
                    spawn_at: self.sim_time_s,
                    // De NINGUEM desde o inicio: nao ha' janela de prioridade
                    // pra quem acabou de afundar. Ele perdeu o barco; o bau
                    // agora e' do mar.
                    dono: None,
                },
            ));
        }
        let mar = shared::mar::Mar::novo();
        let passo = self
            .sessions
            .get(&sid)
            .and_then(|s| crate::quests::indice_da_historia(&s.quests));
        let mut ordem: Vec<usize> = (0..shared::terreno::ARQUIPELAGO.len()).collect();
        ordem.sort_by(|a, b| {
            let da = mar.cais_de(*a).map_or(f32::MAX, |c| c.distance(eu));
            let db = mar.cais_de(*b).map_or(f32::MAX, |c| c.distance(eu));
            da.total_cmp(&db)
        });
        for i in ordem {
            if !shared::viagem::liberada(i, passo) {
                continue;
            }
            let dest = &shared::terreno::ARQUIPELAGO[i];
            let ger = shared::terreno::Gerador::da_ilha(dest);
            let chegada = ger
                .porto()
                .map(|p| p.centro)
                .or_else(|| ger.cidade().map(|c| c.centro()))
                .unwrap_or(Vec2::ZERO);
            if self.mandar_para_zona(
                sid,
                dest.zona,
                chegada,
                Some("O casco cedeu. A correnteza te levou até a costa mais perto."),
                Some(dest.nome),
            ) {
                if let Some(s) = self.sessions.get(&sid) {
                    let _ = s.handle.to_client.send(ServerMessage::Barco {
                        aviso: shared::mar::AvisoBarco::Naufragio {
                            porto: dest.nome.to_string(),
                            perdeu: 0,
                        },
                    });
                }
                return;
            }
        }
        self.recusa_barco(sid, "O casco cedeu, mas não há porto no ar. Você fica à deriva.");
    }

    pub(super) fn recusa_barco(&self, sid: SessionId, motivo: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Barco {
                aviso: shared::mar::AvisoBarco::Recusa(motivo.to_string()),
            });
        }
    }

    /// A porta de entrada de tudo o que o barco pede.
    pub(super) fn handle_barco(&mut self, sid: SessionId, pedido: shared::mar::PedidoBarco) {
        use shared::mar::PedidoBarco as P;
        match pedido {
            P::Zarpar => self.handle_zarpar(sid),
            P::Atracar { ilha } => self.handle_atracar(sid, ilha),
            P::Desembarcar => {}
            P::Reparar { pagando } => self.handle_reparar(sid, pagando),
            P::Melhorar { eixo } => self.handle_melhorar(sid, eixo),
            P::EntregarBau => self.entregar_bau(sid),
            P::Canhao => self.handle_canhao(sid),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O corpo anda no mar e PARA na costa: e' o `mover_casco` que o passo
    /// do jogador usa quando a zona e' o mar.
    #[test]
    fn o_casco_anda_e_encalha() {
        let mar = shared::mar::Mar::com(&[shared::terreno::DefIlha {
            zona: "t",
            nome: "T",
            semente: 5,
            raio_blocos: 64,
            bioma: shared::terreno::Bioma::Floresta,
            centro: [0.0, 0.0],
            nivel: (1, 10),
        }]);
        // Mar aberto, longe da ilha de teste.
        let fora = Vec2::new(60.0, 0.0);
        assert!(mar.agua(fora.x, fora.y));
        let andou = mar.mover_no_mar(fora, Vec2::new(0.0, VEL_MAX), 1.0, RAIO_CASCO);
        assert!(andou.distance(fora) > 5.0, "o casco nao andou: {andou:?}");

        // Rumo ao centro da ilha: para na agua, nao sobe a praia.
        let bateu = mar.mover_no_mar(fora, Vec2::new(-VEL_MAX, 0.0), 6.0, RAIO_CASCO);
        assert!(
            mar.agua(bateu.x, bateu.y),
            "o casco subiu a praia, em {bateu:?}"
        );
    }

    /// A ilha e' PAREDE, e o cais e' a porta.
    ///
    /// Regra do dono: *"nao pode entrar dentro da ilha com o barco, apenas na
    /// borda e aportar no porto"*. Ela sai de graca do desenho — a ilha vista
    /// do mar e' um domo de terra, e o casco so' anda na agua — mas "de
    /// graca" e' exatamente o tipo de coisa que alguem quebra sem perceber.
    #[test]
    fn a_ilha_e_parede_e_o_cais_e_a_porta() {
        let mar = shared::mar::Mar::novo();
        for i in 0..mar.ilhas() {
            let vista = &mar.vistas()[i];
            // Dentro da ilha nao ha' agua: nao se navega pra dentro.
            assert!(
                !mar.agua(vista.centro.x, vista.centro.y),
                "da' pra navegar ate' o centro da ilha {i}"
            );
            let Some(ancoradouro) = mar.cais_de(i) else {
                continue;
            };
            // O ANCORADOURO e' agua: e' onde o casco aparece ao zarpar, e
            // nascer dentro do tabuado deixaria o jogador entalado.
            assert!(
                mar.agua(ancoradouro.x, ancoradouro.y),
                "o ancoradouro {i} caiu em terra"
            );
            // O CAIS e' solido: e' nele que o casco encosta e para. E' a
            // porta da ilha, nao um caminho pra dentro dela.
            let (raiz, ponta) = vista.cais.expect("tem cais");
            let meio = (raiz + ponta) * 0.5;
            assert!(!mar.agua(meio.x, meio.y), "o cais {i} nao e' solido");
            assert_eq!(mar.ilha_mais_perto(ancoradouro), i);
            // Ida e volta pro referencial da ilha.
            let local = mar.para_local(i, ancoradouro);
            assert!((mar.para_mar(i, local) - ancoradouro).length() < 1e-3);
        }
    }
}

impl GameWorld {
    /// Povoa o MAR ABERTO: bicho ao longo das rotas e naufragio pra saquear.
    ///
    /// E' o irmao do `povoar_ilha`, e a diferenca que importa e' de ONDE sai
    /// o lugar. Na ilha os sitios saem do relevo — clareira plana, longe da
    /// cidade. No mar nao ha' relevo pra consultar: o que existe sao as
    /// ROTAS, e elas sao o mapa. Bicho no meio do nada do oceano nunca seria
    /// encontrado; bicho na rota e' a travessia ficando perigosa, que e' o
    /// ponto.
    pub fn povoar_mar(&mut self) {
        let Some(mar) = self.mar.as_ref() else {
            return;
        };
        const POR_ROTA: usize = 7;
        const RAIO_DA_ZONA: f32 = 30.0;
        const POR_ZONA: usize = 4;
        const ESPACO: f32 = 9.0;

        let mut zonas: Vec<ServerSpawnZone> = Vec::new();
        let mut naufragios: Vec<Vec2> = Vec::new();
        for (ir, rota) in shared::mar::ROTAS.iter().enumerate() {
            for (k, centro) in mar.pontos_da_rota(rota, POR_ROTA).into_iter().enumerate() {
                // Um ponto em cada tres vira NAUFRAGIO em vez de cardume: sem
                // isso a rota e' so' uma fila de briga, e nao ha' motivo pra
                // parar no meio do mar.
                if k % 3 == 1 {
                    naufragios.push(centro);
                    continue;
                }
                // Os slots saem de um anel em volta do centro — e todos tem
                // que estar na AGUA, senao um bicho nasce dentro do domo de
                // uma ilha e fica preso pra sempre.
                let mut slots: Vec<SpawnSlot> = Vec::new();
                for j in 0..POR_ZONA * 3 {
                    if slots.len() >= POR_ZONA {
                        break;
                    }
                    let a = j as f32 / (POR_ZONA * 3) as f32 * std::f32::consts::TAU;
                    let p = centro + Vec2::new(a.cos(), a.sin()) * (RAIO_DA_ZONA * 0.6);
                    if !mar.agua(p.x, p.y) || slots.iter().any(|s: &SpawnSlot| s.pos.distance(p) < ESPACO) {
                        continue;
                    }
                    slots.push(SpawnSlot {
                        pos: p,
                        occupant: None,
                        respawn_at: 0.0,
                    });
                }
                if slots.len() < 2 {
                    continue;
                }
                let n = slots.len() as u32;
                zonas.push(ServerSpawnZone {
                    id: 20_000 + (ir * 100 + k) as u32,
                    origin: centro - Vec2::splat(RAIO_DA_ZONA),
                    size: Vec2::splat(RAIO_DA_ZONA * 2.0),
                    respawn_delay_s: 25.0,
                    quotas: Vec::new(),
                    live: Vec::new(),
                    respawn_queue: Vec::new(),
                    polygon: None,
                    level_range: Some((rota.nivel.0, rota.nivel.1, n)),
                    level_range_live: 0,
                    level_range_queue: (0..n).map(|_| 0.0_f32).collect(),
                    slots,
                    forte: false,
                    active: false,
                    last_player_near_at: 0.0,
                });
            }
        }
        // O LEVIATA no covil dele: uma entidade so', no mundo inteiro.
        if let Some(covil) = shared::mar::Leviata::covil(mar) {
            let eid = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(eid),
                Position(covil),
                Velocity(Vec2::ZERO),
                EntityKind::Enemy(24),
                Health {
                    current: shared::mar::Leviata::VIDA,
                    max: shared::mar::Leviata::VIDA,
                },
                LeviataTag { proxima: 0.0 },
            ));
            tracing::info!("leviata em ({:.0},{:.0})", covil.x, covil.y);
        }
        tracing::info!(
            "mar aberto: {} zonas de bicho, {} naufragios",
            zonas.len(),
            naufragios.len()
        );
        self.spawn_zones = zonas;
        self.boss_areas.clear();
        for p in naufragios {
            let eid = self.alloc_entity_id();
            self.ecs.spawn((
                NetId(eid),
                Position(p),
                Velocity(Vec2::ZERO),
                EntityKind::Npc(7), // 7 = bau de tesouro
                NaufragioTag,
            ));
        }
    }
}

/// Um naufragio: o bau que boia na rota, pra quem parar e saquear.
pub struct NaufragioTag;

impl GameWorld {
    /// Saqueia um naufragio: cobre, darksteel e material da faixa da rota.
    ///
    /// **Sem peca de equipamento, de proposito.** O mar da' MATERIAL; a forja
    /// da' peca. Um bau de mar que droppasse equipamento passaria a competir
    /// com a dungeon pelo unico slot de recompensa que a dungeon existe pra
    /// ocupar — e o mar e' solavel e repetivel, a dungeon nao.
    ///
    /// O bau some e volta depois. Quem chegou primeiro levou: nao ha' "ja'
    /// abri este" por jogador aqui, porque nao ha' quest — ha' um objeto no
    /// mundo, e objeto no mundo e' de quem alcanca.
    pub(crate) fn saquear_naufragio(&mut self, sid: SessionId, entidade: Entity) {
        let Some(pos) = self.ecs.get::<&Position>(entidade).ok().map(|p| p.0) else {
            return;
        };
        // O nivel sai da ROTA mais perto: um bau no Olho da Tempestade nao
        // pode pagar como um da Rota do Bosque.
        let nivel = self
            .mar
            .as_ref()
            .map(|m| {
                shared::mar::ROTAS
                    .iter()
                    .filter_map(|r| {
                        let a = m.cais_de(r.de as usize)?;
                        let b = m.cais_de(r.para as usize)?;
                        let meio = (a + b) * 0.5;
                        Some((meio.distance(pos) as i32, r.nivel.0))
                    })
                    .min()
                    .map_or(12, |(_, n)| n)
            })
            .unwrap_or(12);

        let cobre = 150 + 20 * nivel;
        let darksteel = 40 + 6 * nivel;
        let cor = shared::chaves::faixa(nivel).cor;
        let material = shared::item_id::na_cor(shared::item_id::STEEL, cor);

        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        Self::pagar_em_cobre(s, cobre, "naufragio");
        add_to_inventory(&mut s.inventory, shared::item_id::DARKSTEEL, darksteel, None);
        add_to_inventory(&mut s.inventory, material, 3 + nivel / 10, None);
        s.inventory_dirty = true;
        let _ = s.handle.to_client.send(ServerMessage::Chat {
            from: "SYS".into(),
            text: format!(
                "Naufrágio saqueado: {cobre} cobre, {darksteel} darksteel e material."
            ),
        });

        let eid = self.ecs.get::<&NetId>(entidade).map(|n| n.0).ok();
        let _ = self.ecs.despawn(entidade);
        if let Some(eid) = eid {
            self.removed_this_tick.push(eid);
        }
        self.save_pending = true;
    }
}

impl GameWorld {
    /// De quem e' esta entidade de jogador?
    pub(super) fn sid_da_entidade(&self, e: Entity) -> Option<SessionId> {
        self.sessions
            .values()
            .find(|s| s.entity == Some(e))
            .map(|s| s.handle.id)
    }

    /// Dano no CASCO. Zerou, naufragou.
    pub(super) fn dano_no_casco(&mut self, sid: SessionId, dmg: i32) {
        let Some((id, mut d)) = self.barco_equipado(sid) else {
            // Sem barco no mar (corte 1, barco gratis): nao ha' o que roer.
            return;
        };
        d.casco = d.casco.saturating_sub(dmg.max(0) as u16);
        let max = shared::barcos::casco_max(id, d.melhorias[shared::barcos::eixo::CASCO]);
        // Um aviso so', na primeira vez que cruza 30%: o jogador tem que ter
        // chance de voltar antes de afundar, e uma barra que ele talvez nao
        // esteja olhando nao e' chance.
        let antes = d.casco as f32 + dmg.max(0) as f32;
        let limiar = max as f32 * 0.3;
        if antes > limiar && (d.casco as f32) <= limiar && d.casco > 0 {
            self.recusa_barco(sid, "Casco crítico — volte ao porto.");
        }
        let afundou = d.casco == 0;
        self.guarda_barco(sid, d);
        if afundou {
            self.naufragio(sid);
        }
    }
}

impl GameWorld {
    /// Perto do Carpinteiro Naval? Por ora ele divide o cais com o Capitao.
    fn no_estaleiro(&self, sid: SessionId) -> bool {
        self.no_cais(sid)
    }

    /// Abre o painel do estaleiro.
    pub(super) fn abrir_estaleiro(&self, sid: SessionId) {
        let Some((item, d)) = self.barco_equipado(sid) else {
            return;
        };
        let custos: Vec<Vec<(u16, u32)>> = shared::barcos::eixo::EIXOS
            .iter()
            .map(|&e| {
                let n = d.melhorias[e];
                if n >= shared::barcos::MELHORIA_MAX {
                    Vec::new()
                } else {
                    shared::barcos::custo_da_melhoria(item, n + 1).to_vec()
                }
            })
            .collect();
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Barco {
                aviso: shared::mar::AvisoBarco::Estaleiro {
                    item,
                    casco: d.casco,
                    casco_max: shared::barcos::casco_max(
                        item,
                        d.melhorias[shared::barcos::eixo::CASCO],
                    ),
                    melhorias: d.melhorias,
                    travessias: d.travessias,
                    afundou: d.afundou,
                    reparo: shared::barcos::custo_do_reparo(item, &d),
                    custos,
                },
            });
        }
    }

    /// REPARAR. Pagando, enche; de graca, o piso.
    fn handle_reparar(&mut self, sid: SessionId, pagando: bool) {
        if !self.no_estaleiro(sid) {
            self.recusa_barco(sid, "O Carpinteiro Naval fica no cais.");
            return;
        }
        if !self.npc_atende(sid) {
            self.recusa_barco(sid, "Nenhum NPC atende quem tem sangue nas mãos.");
            return;
        }
        let Some((item, mut d)) = self.barco_equipado(sid) else {
            self.recusa_barco(sid, "Você não tem barco equipado.");
            return;
        };
        let max = shared::barcos::casco_max(item, d.melhorias[shared::barcos::eixo::CASCO]);
        if d.casco >= max {
            self.recusa_barco(sid, "O casco já está inteiro.");
            return;
        }
        if !pagando {
            // O PISO DE GRACA. Incondicional: sem cooldown, sem teste de
            // riqueza, sem contar quantas vezes. E' o que impede a travessia
            // obrigatoria de travar uma conta — e e' por ser incondicional
            // que nao ha' nada pra burlar.
            let piso = shared::barcos::reparo_de_graca(item, &d);
            if d.casco >= piso {
                self.recusa_barco(
                    sid,
                    "O conserto de cortesia só cobre até um quarto do casco.",
                );
                return;
            }
            d.casco = piso;
            self.guarda_barco(sid, d);
            self.recusa_barco(sid, "O Carpinteiro calafeta o pior. Navegue com cuidado.");
            self.abrir_estaleiro(sid);
            return;
        }
        let (cobre, madeira) = shared::barcos::custo_do_reparo(item, &d);
        let mad = shared::barcos::madeira_da_classe(
            shared::item_id::casco_de_id(item).unwrap_or(1),
        );
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if crate::craft::tem(&s.inventory, shared::item_id::COPPER) < cobre
            || crate::craft::tem(&s.inventory, mad) < madeira
        {
            self.recusa_barco(sid, &format!("Faltam {cobre} cobre e {madeira} de madeira."));
            return;
        }
        crate::craft::consumir(&mut s.inventory, shared::item_id::COPPER, cobre);
        crate::craft::consumir(&mut s.inventory, mad, madeira);
        s.inventory_dirty = true;
        d.casco = max;
        self.guarda_barco(sid, d);
        self.recusa_barco(sid, "Casco inteiro. Bom mar.");
        self.abrir_estaleiro(sid);
    }

    /// MELHORAR um eixo. Nunca falha — ver `shared::barcos`.
    fn handle_melhorar(&mut self, sid: SessionId, eixo: u8) {
        if !self.no_estaleiro(sid) {
            self.recusa_barco(sid, "O Carpinteiro Naval fica no cais.");
            return;
        }
        if !self.npc_atende(sid) {
            self.recusa_barco(sid, "Nenhum NPC atende quem tem sangue nas mãos.");
            return;
        }
        let e = eixo as usize;
        if !shared::barcos::eixo::EIXOS.contains(&e) {
            return;
        }
        let Some((item, mut d)) = self.barco_equipado(sid) else {
            self.recusa_barco(sid, "Você não tem barco equipado.");
            return;
        };
        if d.melhorias[e] >= shared::barcos::MELHORIA_MAX {
            self.recusa_barco(sid, "Este eixo já está no máximo.");
            return;
        }
        let custo = shared::barcos::custo_da_melhoria(item, d.melhorias[e] + 1);
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        for (id, q) in custo {
            if crate::craft::tem(&s.inventory, id) < q {
                self.recusa_barco(sid, "Falta material para esta melhoria.");
                return;
            }
        }
        for (id, q) in custo {
            crate::craft::consumir(&mut s.inventory, id, q);
        }
        s.inventory_dirty = true;
        // Melhorar o CASCO enche junto o que ele ganhou: subir o teto e
        // deixar o barco pela metade seria cobrar duas vezes pelo mesmo
        // conserto.
        let antes = shared::barcos::casco_max(item, d.melhorias[shared::barcos::eixo::CASCO]);
        d.melhorias[e] += 1;
        let depois = shared::barcos::casco_max(item, d.melhorias[shared::barcos::eixo::CASCO]);
        d.casco = d.casco.saturating_add(depois.saturating_sub(antes));
        self.guarda_barco(sid, d);
        self.recusa_barco(sid, "O Carpinteiro trabalha. O barco melhorou.");
        self.abrir_estaleiro(sid);
    }
}

// ────────────────────────── a marca e o karma ──────────────────────────

/// Raio da bolha da Capitania, em unidades. Dentro dela ninguem apanha.
pub const CORREDOR_DA_CAPITANIA: f32 = 12.0;

impl GameWorld {
    /// Esta sessao esta' MARCADA (PK aberto)?
    ///
    /// Derivada, e nao guardada: ou ha' um bau no conves, ou o rastro dele
    /// ainda esta' quente, ou o karma passou do degrau. Nao ha' campo pra
    /// dessincronizar nem pra esquecer de limpar — e ninguem fica atacavel
    /// sem uma razao que ele proprio consegue ver na tela.
    pub(super) fn marcado(&self, s: &Session, agora: i64) -> bool {
        let com_bau = s
            .equipment
            .barco
            .map(|id| shared::barcos::dados(s.equipment.barco_inst.as_ref(), id))
            .is_some_and(|d| d.carga != 0);
        com_bau
            || agora < s.marcado_ate
            || shared::karma::grau(s.karma).marcado()
    }

    /// Dentro da bolha da Capitania?
    pub(super) fn no_corredor_da_capitania(&self, p: Vec2) -> bool {
        let capitao = shared::construcao::Papel::Estaleiro as u8;
        self.ecs
            .query::<(&Position, &NpcDaVilaTag)>()
            .iter()
            .any(|(_, (pos, t))| {
                shared::npc_papel_de_kind(t.rumo) == capitao
                    && pos.0.distance(p) <= CORREDOR_DA_CAPITANIA
            })
    }

    /// O ASSASSINO LEVA O BAU.
    ///
    /// Tudo o que a vitima perde e' o bau: equipamento, bolsa, ouro, barco e
    /// casco ficam. Uma morte em PvP custa o tesouro e a volta — grande, e
    /// sobrevivivel.
    ///
    /// E o matador fica MARCADO na hora, com o bau no conves dele: a cacada
    /// continua, agora nele. Tres barcos convergindo e o bau trocando de mao
    /// duas vezes e' exatamente o evento que isto existe pra criar.
    pub(super) fn bau_troca_de_dono(&mut self, morto: SessionId, matador: SessionId) {
        let Some((bau, de)) = self.tirar_bau(morto) else {
            return;
        };
        if let Some(s) = self.sessions.get(&morto) {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "SYS".into(),
                text: "Levaram o baú do seu convés.".into(),
            });
        }
        if !self.carregar_bau(matador, bau, de) {
            // O matador nao tinha onde por: o bau cai no mar. Nao some do
            // jogo por acidente de inventario — mas tambem nao premia quem
            // matou sem ter barco.
            if let Some(s) = self.sessions.get(&matador) {
                let _ = s.handle.to_client.send(ServerMessage::Chat {
                    from: "SYS".into(),
                    text: "Sem convés livre, o baú afundou.".into(),
                });
            }
        }
    }

    /// Cobra o karma de matar quem nao estava marcado.
    ///
    /// So' a MORTE cobra, e nao o toque: area necessariamente respinga em
    /// quem passa, e punir um corte perdido seria injusto e ilegivel.
    pub(super) fn karma_por_matar(&mut self, matador: SessionId, vitima: SessionId) {
        let agora = (now_ms() / 1000) as i64;
        let Some(v) = self.sessions.get(&vitima) else {
            return;
        };
        // Matar quem esta' MARCADO nao e' crime nem virtude: e' o jogo.
        if self.marcado(v, agora) {
            return;
        }
        // Nem matar em ZONA SEM LEI: la' nao ha' regra pra quebrar.
        if let (Some(mar), Some(p)) = (
            self.mar.as_ref(),
            v.entity.and_then(|e| self.ecs.get::<&Position>(e).ok().map(|x| x.0)),
        ) {
            if mar.sem_lei(p) {
                return;
            }
        }
        let nome_da_vitima = v.name.clone();
        let mesma_faccao = self
            .sessions
            .get(&matador)
            .is_some_and(|m| m.faction == v.faction);
        // Faccao e' guerra consentida: as quests de faccao tem que continuar
        // funcionando, e morrer pro lado contrario nao e' assassinato.
        if !mesma_faccao {
            return;
        }
        let Some(m) = self.sessions.get_mut(&matador) else {
            return;
        };
        let repetida = m
            .ultima_vitima
            .as_ref()
            .is_some_and(|(n, t)| *n == nome_da_vitima && agora - t < shared::karma::JANELA_REPETIDA_S);
        m.karma = shared::karma::apos_matar(m.karma, repetida);
        m.ultima_vitima = Some((nome_da_vitima, agora));
        let g = shared::karma::grau(m.karma);
        let _ = m.handle.to_client.send(ServerMessage::Chat {
            from: "SYS".into(),
            text: format!(
                "Você matou alguém que não carregava nada. Karma {} — {}.",
                m.karma,
                g.nome()
            ),
        });
        if g.marcado() {
            let _ = m.handle.to_client.send(ServerMessage::Chat {
                from: "SYS".into(),
                text: "Agora você é o caçado: qualquer um pode te atacar, e nenhum NPC te atende."
                    .into(),
            });
        }
        self.save_pending = true;
    }
}

impl GameWorld {
    /// Poe um Bau do Colosso no CONVES.
    ///
    /// Um por vez, e a recusa e' explicita: nao ha' fila, nao ha' "pega o
    /// segundo e larga o primeiro". Um so' mantem a marca BINARIA — ou voce
    /// carrega, ou nao — e impede uma guilda de juntar seis baus num galeao
    /// defendido, que transformaria a caçada numa unica batalha por ano.
    pub(super) fn carregar_bau(&mut self, sid: SessionId, bau: u16, de_ilha: u8) -> bool {
        let Some((_, mut d)) = self.barco_equipado(sid) else {
            self.recusa_barco(sid, "Sem barco não há convés onde pôr o baú.");
            return false;
        };
        if d.carga != 0 {
            self.recusa_barco(sid, "Já há um baú no convés. Entregue-o primeiro.");
            return false;
        }
        d.carga = bau;
        d.carga_de = de_ilha + 1;
        self.guarda_barco(sid, d);
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "SYS".into(),
                text: "O baú está no convés. A partir de agora QUALQUER UM pode te atacar, \
                       em qualquer lugar. Entregue numa Capitania de outra ilha."
                    .into(),
            });
        }
        true
    }

    /// Tira o bau do conves e devolve (item, ilha de origem).
    fn tirar_bau(&mut self, sid: SessionId) -> Option<(u16, u8)> {
        let (_, mut d) = self.barco_equipado(sid)?;
        if d.carga == 0 {
            return None;
        }
        let saiu = (d.carga, d.carga_de.saturating_sub(1));
        d.carga = 0;
        d.carga_de = 0;
        self.guarda_barco(sid, d);
        // O RASTRO: a marca sobrevive um minuto a entrega. Sem ele, entregar
        // um segundo antes do golpe e' um drible — quem correu a travessia
        // inteira atras de alguem perde no ultimo quadro.
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.marcado_ate = (now_ms() / 1000) as i64 + shared::mar::RASTRO_DA_MARCA_S;
        }
        Some(saiu)
    }

    /// ENTREGAR o bau na Capitania desta ilha.
    ///
    /// Quem abre onde o chefe caiu leva o premio de chefe e pronto — nao e'
    /// castigo, e' o piso. Cada travessia multiplica, e cada travessia e' uma
    /// chance de perder tudo: o jogador escolhe quanto quer apostar.
    pub(super) fn entregar_bau(&mut self, sid: SessionId) {
        if !self.no_cais(sid) {
            self.recusa_barco(sid, "A entrega é na Capitania, no cais.");
            return;
        }
        let aqui = shared::terreno::ARQUIPELAGO
            .iter()
            .position(|d| d.zona == self.zona)
            .unwrap_or(0) as u8;
        let Some((bau, de)) = self.tirar_bau(sid) else {
            return;
        };
        let saltos = shared::mar::saltos_entre(de, aqui);
        let mult = shared::mar::multiplicador_da_carga(saltos);
        let cor = (bau - shared::item_id::BAU_COLOSSO_BASE) as u8 + 1;
        let nivel = match cor {
            1 => 14u32,
            2 => 30,
            3 => 42,
            4 => 55,
            _ => 60,
        };
        let ouro = ((200 + 30 * nivel) as f32 * mult) as u64;
        let cobre = ((300 + 40 * nivel) as f32 * mult) as u32;
        let darksteel = ((60 + 10 * nivel) as f32 * mult) as u32;
        let material = shared::item_id::na_cor(shared::item_id::STEEL, cor.min(4));
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        s.gold = s.gold.saturating_add(ouro);
        Self::pagar_em_cobre(s, cobre, "bau_do_colosso");
        add_to_inventory(&mut s.inventory, shared::item_id::DARKSTEEL, darksteel, None);
        add_to_inventory(&mut s.inventory, material, 2 + nivel / 12, None);
        // A CHAVE garantida, mas so' a partir de uma travessia. E' a cenoura
        // da travessia inteira: chefe do mundo da' chave a 5/3/2%, e uma
        // garantida que custou um mar e uma marca nas costas e' coisa nova.
        if saltos > 0 {
            if let Some(chave) = shared::item_id::CHAVES.first() {
                add_to_inventory(
                    &mut s.inventory,
                    shared::item_id::chave_na_cor(*chave, cor.min(5)),
                    1,
                    None,
                );
            }
        }
        s.inventory_dirty = true;
        let _ = s.handle.to_client.send(ServerMessage::Chat {
            from: "SYS".into(),
            text: format!(
                "Baú entregue após {saltos} travessia(s) — x{mult:.1}: {ouro} ouro, \
                 {cobre} cobre e {darksteel} darksteel."
            ),
        });
        self.save_pending = true;
    }
}

impl GameWorld {
    /// O karma DECAI com tempo de jogo.
    ///
    /// Com tempo ONLINE e nao com tempo de relogio, de proposito: deslogar
    /// nao pode ser a forma barata de limpar a ficha. Uma morte custa uma
    /// hora jogando — um preco que o jogador consegue guardar, e karma so'
    /// funciona se ele souber o preco antes de matar.
    pub(super) fn tick_karma(&mut self, dt: f32) {
        for s in self.sessions.values_mut() {
            if !s.logged_in || s.karma <= 0 {
                continue;
            }
            let antes = shared::karma::grau(s.karma);
            s.karma = shared::karma::apos_jogar(s.karma, dt);
            let depois = shared::karma::grau(s.karma);
            if antes != depois {
                let _ = s.handle.to_client.send(ServerMessage::Chat {
                    from: "SYS".into(),
                    text: format!("Sua ficha melhorou: {}.", depois.nome()),
                });
            }
        }
    }

    /// O NPC atende este jogador?
    ///
    /// A punicao com mais dentes e a mais barata de escrever: nao ha' guarda
    /// com IA, nao ha' perseguicao. O Criminoso simplesmente nao consegue
    /// CONSERTAR O CASCO — e um barco que nao repara nao navega. O mar cospe
    /// ele pra fora sozinho.
    pub(super) fn npc_atende(&self, sid: SessionId) -> bool {
        self.sessions
            .get(&sid)
            .is_none_or(|s| shared::karma::grau(s.karma).npc_atende())
    }
}

impl GameWorld {
    /// DISPARA o canhao no casco inimigo mais perto.
    ///
    /// Sem mira e sem projetil: o tiro resolve na hora. Um projetil viajando
    /// daria a chance de desviar, que e' o que faz sentido em terra — mas
    /// aqui os dois alvos sao cascos de dez metros a onze unidades por
    /// segundo, e "desviar" nao e' um verbo que exista. O que decide e'
    /// entrar no alcance e sair dele.
    fn handle_canhao(&mut self, sid: SessionId) {
        let agora = self.sim_time_s;
        let Some((item, d)) = self.barco_equipado(sid) else {
            return;
        };
        let dano = shared::barcos::dano_do_canhao(item, d.melhorias[shared::barcos::eixo::CANHAO]);
        if dano == 0 {
            self.recusa_barco(sid, "Seu barco não tem canhão. O Carpinteiro instala.");
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        if agora < s.canhao_pronto_em {
            return;
        }
        let (eu_eid, eu_pos) = (s.entity_id, self.pos_do_jogador(sid));
        let Some(eu) = eu_pos else { return };
        // O alvo: o casco mais perto em que se PODE atirar. A mesma
        // `can_damage_player` do resto do jogo — o canhao nao inventa uma
        // segunda regra de quem pode bater em quem.
        let alvo = self
            .sessions
            .values()
            .filter(|o| o.logged_in && o.handle.id != sid)
            .filter_map(|o| {
                let p = o
                    .entity
                    .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|x| x.0))?;
                let d = p.distance(eu);
                (d <= shared::barcos::ALCANCE_DO_CANHAO
                    && self.can_damage_player(eu_eid, o.entity_id))
                .then_some((o.handle.id, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((alvo, _)) = alvo else {
            self.recusa_barco(sid, "Nenhum casco no alcance.");
            return;
        };
        if let Some(s) = self.sessions.get_mut(&sid) {
            s.canhao_pronto_em = agora + shared::barcos::RECARGA_DO_CANHAO;
        }
        self.dano_no_casco(alvo, dano as i32);
        // Quem levou o tiro precisa saber de ONDE veio: sem isso o casco cai
        // e o jogador nao tem como reagir a coisa nenhuma.
        let nome = self
            .sessions
            .get(&sid)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        if let Some(v) = self.sessions.get(&alvo) {
            let _ = v.handle.to_client.send(ServerMessage::Chat {
                from: "SYS".into(),
                text: format!("Canhão de {nome} acerta seu casco!"),
            });
        }
    }
}

/// O Leviata no mundo. Ver `shared::mar::Leviata`.
pub struct LeviataTag {
    /// Quando ele morde de novo (`sim_time_s`).
    pub proxima: f32,
}

impl GameWorld {
    /// O Leviata morde o casco de quem chegar perto.
    ///
    /// Nao ha' telegrafico, nao ha' desvio, nao ha' perseguicao: ele fica no
    /// covil e cobra pedagio. A luta e' uma corrida entre o dano dele e o
    /// teu, e quem decide nao e' o reflexo — e' com que barco voce veio.
    pub(super) fn tick_leviata(&mut self, _dt: f32) {
        if self.mar.is_none() {
            return;
        }
        let agora = self.sim_time_s;
        let mordidas: Vec<(Entity, Vec<SessionId>)> = self
            .ecs
            .query::<(&Position, &LeviataTag)>()
            .iter()
            .filter(|(_, (_, t))| agora >= t.proxima)
            .map(|(e, (pos, _))| {
                let perto = self
                    .sessions
                    .values()
                    .filter(|s| s.logged_in)
                    .filter(|s| {
                        s.entity
                            .and_then(|pe| self.ecs.get::<&Position>(pe).ok().map(|p| p.0))
                            .is_some_and(|p| p.distance(pos.0) <= shared::mar::Leviata::ALCANCE)
                    })
                    .map(|s| s.handle.id)
                    .collect::<Vec<_>>();
                (e, perto)
            })
            .collect();
        for (e, alvos) in mordidas {
            if alvos.is_empty() {
                continue;
            }
            if let Ok(mut t) = self.ecs.get::<&mut LeviataTag>(e) {
                t.proxima = agora + shared::mar::Leviata::RITMO_S;
            }
            for sid in alvos {
                self.dano_no_casco(sid, shared::mar::Leviata::BOCADA);
            }
        }
    }
}
