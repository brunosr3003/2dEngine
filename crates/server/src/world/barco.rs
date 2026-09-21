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
        self.mandar_para_zona(sid, dest.zona, chegada, Some(&aviso), Some(dest.nome));
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

    fn recusa_barco(&self, sid: SessionId, motivo: &str) {
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
