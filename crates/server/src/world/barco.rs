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
