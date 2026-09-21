//! O CASCO no Mar Aberto (docs/MAR_ABERTO.md).
//!
//! Desde 21/09/2026 trocar de ilha exige navegar. Zarpar poe o jogador na
//! zona `mar_aberto` com um casco sob os pes; atracar o devolve pro cais da
//! ilha de destino.
//!
//! Tres escolhas que valem estar escritas, porque as tres economizaram um
//! sistema inteiro:
//!
//! - **O casco usa o `Health` que ja' existe.** Nao ha' campo de vida novo:
//!   o encanamento de dano, o `EntityState::hp`, a barra do cliente e o
//!   aggro de mob passam a valer no barco de graca. E' o que "fase 1: voce
//!   luta do conves com as skills normais, e o barco e' uma plataforma com
//!   HP" pede, sem escrever nada.
//! - **A posicao do passageiro e' DERIVADA, nao aparentada.** Nao ha'
//!   componente de parentesco, nao ha' campo novo no fio: depois que o casco
//!   anda, a posicao de quem esta' a bordo e' recalculada a partir dele. O
//!   jogador mantem `NetId`, `Health`, AOI e alvo, e nada do combate muda.
//! - **O barco NAO e' persistido.** O que persiste e' o item na bolsa (corte
//!   3) e o porao (corte 4). Um casco no mundo e' estado de sessao.

use super::*;

/// O casco de um jogador, no mar.
pub struct BarcoTag {
    pub dono: SessionId,
    /// Classe do casco. No corte 1 e' sempre 1 — o barco e' gratis e
    /// automatico, pra a travessia poder ser testada antes de existir item.
    pub classe: u16,
    /// Pra onde a proa aponta, em radianos.
    pub yaw: f32,
    /// Velocidade atual, em unidades por segundo.
    pub vel: f32,
    /// -1..1, do HUD. O piloto automatico escreve no MESMO campo, pra haver
    /// um caminho de pilotagem so'.
    pub leme: f32,
    /// De que cais o casco estava perto no tick passado. E' o que faz o
    /// painel de atracar abrir sozinho ao chegar, e so' uma vez.
    pub cais_perto: Option<u8>,
    /// 0..1, do HUD.
    pub acelerador: f32,
}

/// Velocidade maxima do casco, em unidades por segundo.
///
/// 11 e' 2,2x o andar (`PLAYER_SPEED` 5,0) e 1,5x a montaria. O teto duro e'
/// o fio: `EntityState::vel` e' `i8` a `POS_SCALE`, o que da' 15,9 u/s — e
/// acima de ~16 a AOI do mar (40 u) daria menos de 2,5 s de aviso, que nao
/// da' num celular de 150 ms de atraso.
pub const VEL_MAX: f32 = 11.0;
/// Quanto o casco ganha e perde de velocidade por segundo.
const ACEL: f32 = 3.5;
const FREIO: f32 = 5.0;
/// Giro maximo do leme, em radianos por segundo.
const VIRADA: f32 = 1.1;
/// Raio do casco pro teste de agua.
pub const RAIO_CASCO: f32 = 1.4;
/// Dano de encalhe, por segundo raspando a costa.
const DANO_ENCALHE: i32 = 12;
/// Vida do casco da classe 1.
pub const CASCO_MAX: i32 = 400;

impl GameWorld {
    /// Anda os cascos. Roda depois do movimento dos jogadores, pra quem esta'
    /// a bordo seguir a posicao DESTE tick.
    pub(super) fn tick_barcos(&mut self, dt: f32) {
        let Some(mar) = self.mar.as_ref() else {
            return;
        };
        // (entidade, nova pos, novo yaw, nova vel, encalhou)
        let mut passos: Vec<(Entity, Vec2, f32, f32, bool)> = Vec::new();
        for (e, (pos, tag)) in self.ecs.query::<(&Position, &BarcoTag)>().iter() {
            // Um casco parado quase nao vira. Uma multiplicacao, e e' o que
            // faz ele ler como barco em vez de carro.
            let peso = (tag.vel / VEL_MAX).clamp(0.2, 1.0);
            let yaw = tag.yaw + tag.leme * VIRADA * dt * peso;
            let alvo = tag.acelerador.clamp(0.0, 1.0) * VEL_MAX;
            let vel = if alvo > tag.vel {
                (tag.vel + ACEL * dt).min(alvo)
            } else {
                (tag.vel - FREIO * dt).max(alvo)
            };
            // Convencao do FIO: `rumo_de_dir` usa `x.atan2(z)`, entao a
            // frente e' (sin, cos). Guardar o yaw ja' nessa convencao evita
            // uma conversao a cada snapshot — e evita o bug de ela existir
            // num lugar e faltar noutro.
            let dir = Vec2::new(yaw.sin(), yaw.cos());
            let pedido = dir * vel;
            let novo = mar.mover_no_mar(pos.0, pedido, dt, RAIO_CASCO);
            // Andou menos de 30% do que pediu: bateu na costa. Encalhe nao e'
            // dano escondido — o recife ensina sozinho.
            let encalhou = vel > 0.5 && novo.distance(pos.0) < (pedido * dt).length() * 0.3;
            let vel = if encalhou { vel * 0.3 } else { vel };
            passos.push((e, novo, yaw, vel, encalhou));
        }

        let mut afundaram: Vec<SessionId> = Vec::new();
        for (e, novo, yaw, vel, encalhou) in passos {
            if let Ok(mut pos) = self.ecs.get::<&mut Position>(e) {
                pos.0 = novo;
            }
            if let Ok(mut v) = self.ecs.get::<&mut Velocity>(e) {
                v.0 = Vec2::new(yaw.sin(), yaw.cos()) * vel;
            }
            let dono = {
                let Ok(mut t) = self.ecs.get::<&mut BarcoTag>(e) else {
                    continue;
                };
                t.yaw = yaw;
                t.vel = vel;
                t.dono
            };
            if encalhou {
                if let Ok(mut h) = self.ecs.get::<&mut Health>(e) {
                    h.current -= (DANO_ENCALHE as f32 * dt).ceil() as i32;
                    if h.current <= 0 {
                        afundaram.push(dono);
                    }
                }
            }
            // O passageiro vai junto: posicao DERIVADA do casco.
            if let Some(s) = self.sessions.get(&dono) {
                if let Some(pe) = s.entity {
                    if let Ok(mut p) = self.ecs.get::<&mut Position>(pe) {
                        p.0 = novo;
                    }
                }
            }
        }
        for sid in afundaram {
            self.naufragio(sid);
        }
        self.avisa_cais();
    }

    /// O CAIS AVISA SOZINHO.
    ///
    /// No mar nao ha' Capitao pra clicar, e um botao permanente de "atracar"
    /// seria um botao que nao serve 99% da travessia. Chegar perto de um cais
    /// abre a lista de portos — a MESMA do porto, com os mesmos estados, pra
    /// quem chegou saber na hora se aquele porto caiu ou se a historia ainda
    /// nao liberou.
    ///
    /// So' na BORDA: abre ao entrar no alcance, e nao de novo ate' sair.
    fn avisa_cais(&mut self) {
        let Some(mar) = self.mar.as_ref() else {
            return;
        };
        let cais: Vec<(u8, Vec2)> = (0..shared::terreno::ARQUIPELAGO.len())
            .filter_map(|i| mar.cais_de(i).map(|c| (i as u8, c)))
            .collect();
        let mut abrir: Vec<SessionId> = Vec::new();
        let mut mudancas: Vec<(Entity, Option<u8>)> = Vec::new();
        for (e, (pos, tag)) in self.ecs.query::<(&Position, &BarcoTag)>().iter() {
            let perto = cais
                .iter()
                .find(|(_, c)| c.distance(pos.0) <= shared::mar::PERTO_DO_CAIS)
                .map(|(i, _)| *i);
            if perto != tag.cais_perto {
                mudancas.push((e, perto));
                if perto.is_some() {
                    abrir.push(tag.dono);
                }
            }
        }
        for (e, perto) in mudancas {
            if let Ok(mut t) = self.ecs.get::<&mut BarcoTag>(e) {
                t.cais_perto = perto;
            }
        }
        for sid in abrir {
            self.abrir_menu_viagem(sid);
        }
    }

    /// Poe um casco no mundo sob o jogador, se ele ainda nao tem um.
    pub(super) fn nasce_barco(&mut self, sid: SessionId, pos: Vec2, yaw: f32) {
        if self.barco_de(sid).is_some() {
            return;
        }
        let eid = self.alloc_entity_id();
        self.ecs.spawn((
            NetId(eid),
            Position(pos),
            Velocity(Vec2::ZERO),
            EntityKind::Barco(1),
            Health {
                current: CASCO_MAX,
                max: CASCO_MAX,
            },
            BarcoTag {
                dono: sid,
                classe: 1,
                yaw,
                vel: 0.0,
                leme: 0.0,
                acelerador: 0.0,
                cais_perto: None,
            },
        ));
    }

    /// O casco deste jogador, se ha' um.
    pub(super) fn barco_de(&self, sid: SessionId) -> Option<Entity> {
        self.ecs
            .query::<&BarcoTag>()
            .iter()
            .find(|(_, t)| t.dono == sid)
            .map(|(e, _)| e)
    }

    /// Tira o casco do mundo.
    pub(super) fn some_barco(&mut self, sid: SessionId) {
        let Some(e) = self.barco_de(sid) else {
            return;
        };
        let eid = self.ecs.get::<&NetId>(e).map(|n| n.0).ok();
        let _ = self.ecs.despawn(e);
        if let Some(eid) = eid {
            self.removed_this_tick.push(eid);
        }
    }
}

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
        if self.mandar_para_zona(sid, dest.zona, chegada, Some(&aviso), Some(dest.nome)) {
            self.some_barco(sid);
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
        self.some_barco(sid);
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

    /// Leme e acelerador. O piloto automatico (corte futuro) escreve nos
    /// MESMOS campos — um caminho de pilotagem so'.
    fn handle_comando_barco(&mut self, sid: SessionId, leme: i8, forca: i8) {
        let Some(e) = self.barco_de(sid) else {
            return;
        };
        if let Ok(mut t) = self.ecs.get::<&mut BarcoTag>(e) {
            t.leme = (leme as f32 / 127.0).clamp(-1.0, 1.0);
            t.acelerador = (forca as f32 / 127.0).clamp(0.0, 1.0);
        }
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
            P::Comando { leme, forca } => self.handle_comando_barco(sid, leme, forca),
            P::Desembarcar => {}
        }
    }
}

impl GameWorld {
    /// Acerta quem esta' no mar com quem tem casco.
    ///
    /// Mesmo desenho do `sincroniza_pets`, e pelo mesmo motivo: rodando todo
    /// tick, entrar na zona, sair dela, morrer e deslogar passam por aqui sem
    /// precisar de um gancho em cada um desses caminhos.
    ///
    /// No corte 1 o barco e' GRATIS e automatico — quem esta' no mar tem um.
    /// E' de proposito: da' pra testar a travessia inteira antes de existir
    /// item, craft e quest, que e' o corte 3.
    pub(super) fn sincroniza_barcos(&mut self) {
        if self.mar.is_none() {
            return;
        }
        let querem: Vec<(SessionId, Vec2)> = self
            .sessions
            .values()
            .filter(|s| s.logged_in && s.entity.is_some())
            .filter_map(|s| {
                let e = s.entity?;
                let pos = self.ecs.get::<&Position>(e).ok()?.0;
                Some((s.handle.id, pos))
            })
            .collect();
        for (sid, pos) in querem {
            if self.barco_de(sid).is_none() {
                self.nasce_barco(sid, pos, 0.0);
            }
        }
        // Casco orfao: o dono saiu sem passar pelo laco acima.
        let vivos: Vec<SessionId> = self.sessions.keys().copied().collect();
        let orfaos: Vec<(Entity, EntityId)> = self
            .ecs
            .query::<(&NetId, &BarcoTag)>()
            .iter()
            .filter(|(_, (_, t))| !vivos.contains(&t.dono))
            .map(|(e, (n, _))| (e, n.0))
            .collect();
        for (e, eid) in orfaos {
            let _ = self.ecs.despawn(e);
            self.removed_this_tick.push(eid);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O casco anda no mar e PARA na costa. E' o `mover_casco` visto de
    /// dentro do tick: acelera, vira e encalha.
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

    /// Zarpar e atracar sao a MESMA conta, em sentidos opostos. Um erro de
    /// sinal aqui poe o jogador do outro lado do arquipelago, e o teste
    /// custa menos que descobrir isso jogando.
    #[test]
    fn zarpar_e_atracar_fecham_a_conta() {
        let mar = shared::mar::Mar::novo();
        for i in 0..mar.ilhas() {
            let Some(cais) = mar.cais_de(i) else { continue };
            // A ponta do cais e' agua: da' pra chegar nela de casco.
            assert!(mar.agua(cais.x, cais.y), "cais {i} fora da agua");
            // E fica DENTRO do alcance de atracar, olhando da propria ilha.
            assert!(mar.ilha_mais_perto(cais) == i, "cais {i} mais perto de outra ilha");
            // Ida e volta pro referencial da ilha.
            let local = mar.para_local(i, cais);
            assert!((mar.para_mar(i, local) - cais).length() < 1e-3);
        }
    }
}
