//! A COLONIA no servidor (docs/COLONIA.md).
//!
//! # Cada um na sua, e por isso e' barato
//!
//! A colonia vive em INSTANCIA, como a dungeon: dois jogadores nunca se veem,
//! entao podem ocupar as MESMAS coordenadas. Sem isso seria preciso um espaco
//! de mundo compartilhado, com as ilhas espalhadas e uma silhueta pra cada —
//! foi exatamente o que encareceu o Mar Aberto, e foi o que o dono mandou
//! desfazer.
//!
//! O terreno tambem e' por instancia: `colonias[instancia]` guarda a `Ilha`
//! daquele jogador, gerada de `hash(nome)`. A 120 blocos de raio o campo de
//! altura sao uns 115 KB, entao sessenta colonias vivas cabem em 7 MB.
//!
//! O que persiste sao QUATRO numeros por personagem: o nivel de cada eixo e
//! quando foi a ultima colheita. A ilha em si nunca e' salva — ela e' funcao
//! pura da semente, e regerar sai mais barato que gravar.

use super::*;

impl GameWorld {
    /// A `Ilha` da colonia desta instancia, gerando se for a primeira visita.
    pub(crate) fn colonia_de(
        &mut self,
        instancia: u32,
        nome: &str,
        nivel: u8,
    ) -> &shared::terreno::Ilha {
        self.colonias.entry(instancia).or_insert_with(|| {
            let t0 = std::time::Instant::now();
            let i = shared::terreno::Ilha::gerar(
                shared::colonia::semente(nome),
                shared::colonia::raio_blocos(nivel),
                shared::terreno::Bioma::Floresta,
                shared::terreno::ESCALA_ALTURA,
            );
            tracing::info!("colonia de '{nome}' (inst {instancia}) pronta em {:?}", t0.elapsed());
            i
        })
    }

    /// Joga fora a colonia de uma instancia que esvaziou.
    ///
    /// Regerar custa milissegundos e guardar custa memoria por jogador que
    /// deslogou: a conta so' fecha de um lado.
    pub(crate) fn esquece_colonia(&mut self, instancia: u32) {
        if self.colonias.remove(&instancia).is_some() {
            tracing::info!("colonia da instancia {instancia} liberada");
        }
    }

    /// Horas desde a ultima colheita.
    fn horas_paradas(&self, sid: SessionId) -> f32 {
        let agora = (now_ms() / 1000) as i64;
        self.sessions.get(&sid).map_or(0.0, |s| {
            if s.colonia.colhida_em == 0 {
                // Colonia nova: comeca a contar agora, e nao desde 1970.
                return 0.0;
            }
            (agora - s.colonia.colhida_em).max(0) as f32 / 3600.0
        })
    }

    /// COLHER o que a ilha rendeu.
    pub(super) fn colher_colonia(&mut self, sid: SessionId) {
        let horas = self.horas_paradas(sid);
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let nivel = s.colonia.niveis[shared::colonia::eixo::RECURSOS];
        let ganho = shared::colonia::colheita(nivel, horas);
        if ganho.is_empty() {
            self.avisa_colonia(sid, "A ilha ainda não rendeu nada. Volte mais tarde.");
            return;
        }
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        let mut texto = Vec::new();
        for (id, q) in &ganho {
            if *id == shared::item_id::COPPER {
                Self::pagar_em_cobre(s, *q, "colonia");
            } else {
                add_to_inventory(&mut s.inventory, *id, *q, None);
            }
            texto.push(format!("{q}× {id}"));
        }
        s.inventory_dirty = true;
        s.colonia.colhida_em = (now_ms() / 1000) as i64;
        self.avisa_colonia(
            sid,
            &format!("Colheita de {horas:.0}h: {}.", texto.join(", ")),
        );
        self.save_pending = true;
    }

    /// MELHORAR um eixo.
    pub(super) fn melhorar_colonia(&mut self, sid: SessionId, eixo: u8) {
        let e = eixo as usize;
        if e >= shared::colonia::EIXOS {
            return;
        }
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let atual = s.colonia.niveis[e];
        if atual >= shared::colonia::NIVEL_MAX {
            self.avisa_colonia(sid, "Este eixo já está no máximo.");
            return;
        }
        let custo = shared::colonia::custo(e, atual + 1);
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        for (id, q) in custo {
            if crate::craft::tem(&s.inventory, id) < q {
                self.avisa_colonia(sid, "Falta material para esta melhoria.");
                return;
            }
        }
        for (id, q) in custo {
            crate::craft::consumir(&mut s.inventory, id, q);
        }
        s.colonia.niveis[e] += 1;
        s.inventory_dirty = true;
        let instancia = s.instancia;
        let novo = s.colonia.niveis[e];
        let quem = s.name.clone();
        let entidade = s.entity;
        let nome = shared::colonia::eixo::NOMES[e];
        // TAMANHO muda o RELEVO. Regerar na hora, e nao so' descartar: sem
        // ilha nenhuma a fisica cai no mapa de tiles velho, e o jogador
        // atravessa o chao da propria colonia ate' o proximo login.
        if e == shared::colonia::eixo::TAMANHO {
            self.esquece_colonia(instancia);
            self.colonia_de(instancia, &quem, novo);
            // A costa e' outra: quem estava na beirada pode ter virado agua.
            // Descer de novo e' mais barato que descobrir isso afogado.
            if let Some(ent) = entidade {
                let onde = self.ecs.get::<&Position>(ent).ok().map(|p| p.0);
                let destino = onde.and_then(|p| {
                    self.colonias
                        .get(&instancia)
                        .map(|i| i.terra_mais_proxima(p.x, p.y, 400.0))
                });
                if let (Some(d), Ok(mut p)) = (destino, self.ecs.get::<&mut Position>(ent)) {
                    p.0 = d;
                }
            }
            let _ = self.mandar_terreno_da_colonia(sid);
        }
        self.avisa_colonia(sid, &format!("{nome} melhorado."));
        self.save_pending = true;
    }

    /// Manda o cliente REDESENHAR a colonia. O relevo do cliente sai da mesma
    /// semente e do mesmo raio; o que ele nao tem como adivinhar e' que o raio
    /// mudou agora.
    fn mandar_terreno_da_colonia(&self, sid: SessionId) -> bool {
        let Some(s) = self.sessions.get(&sid) else {
            return false;
        };
        let _ = s.handle.to_client.send(ServerMessage::Colonia {
            aviso: shared::colonia::AvisoColonia::Terreno {
                semente: shared::colonia::semente(&s.name),
                raio: shared::colonia::raio_blocos(
                    s.colonia.niveis[shared::colonia::eixo::TAMANHO],
                ),
            },
        });
        true
    }

    fn avisa_colonia(&self, sid: SessionId, texto: &str) {
        if let Some(s) = self.sessions.get(&sid) {
            let _ = s.handle.to_client.send(ServerMessage::Chat {
                from: "SYS".into(),
                text: texto.to_string(),
            });
        }
    }
}

impl GameWorld {
    /// O painel da colonia.
    pub(super) fn abrir_colonia(&self, sid: SessionId) {
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let horas = self.horas_paradas(sid);
        let custos: Vec<Vec<(u16, u32)>> = (0..shared::colonia::EIXOS)
            .map(|e| {
                let n = s.colonia.niveis[e];
                if n >= shared::colonia::NIVEL_MAX {
                    Vec::new()
                } else {
                    shared::colonia::custo(e, n + 1).to_vec()
                }
            })
            .collect();
        let _ = s.handle.to_client.send(ServerMessage::Colonia {
            aviso: shared::colonia::AvisoColonia::Estado {
                niveis: s.colonia.niveis,
                horas,
                colheita: shared::colonia::colheita(
                    s.colonia.niveis[shared::colonia::eixo::RECURSOS],
                    horas,
                ),
                custos,
                banco: shared::colonia::espacos_do_banco(
                    s.colonia.niveis[shared::colonia::eixo::BANCO],
                ),
            },
        });
    }

    /// VISITAR: do porto pra ilha.
    ///
    /// A instancia e' o que separa uma colonia da outra, e ela sai do PROPRIO
    /// personagem: assim dois jogadores nunca dividem ilha, e reconectar cai
    /// sempre na mesma.
    fn visitar_colonia(&mut self, sid: SessionId) {
        if !self.perto_do_capitao(sid) {
            self.avisa_colonia(sid, "O barco pra sua ilha sai do porto.");
            return;
        }
        let zona = self.zona.clone();
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        // A volta e' gravada ANTES de viajar, e vai pro banco no `save_pending`
        // que o `mandar_para_zona` liga: e' a sessao de LA' que vai le-la.
        s.colonia.volta = zona;
        self.mandar_para_zona(
            sid,
            shared::colonia::ZONA,
            shared::colonia::CHEGADA,
            Some("Você embarca para a sua ilha."),
            Some("sua ilha"),
        );
    }

    /// VOLTAR: da ilha pro porto de onde veio.
    fn voltar_da_colonia(&mut self, sid: SessionId) {
        let volta = self
            .sessions
            .get(&sid)
            .map(|s| s.colonia.volta.clone())
            .filter(|z| !z.is_empty())
            .unwrap_or_else(|| shared::terreno::ARQUIPELAGO[0].zona.to_string());
        // Volta pro CAIS de onde saiu, nao pra praca: quem foi ver a ilha
        // pelo porto volta olhando pro Capitao.
        let chegada = shared::terreno::def_da_zona(&volta)
            .map(shared::terreno::Gerador::da_ilha)
            .and_then(|g| g.porto().map(|p| p.centro))
            .unwrap_or(Vec2::ZERO);
        self.mandar_para_zona(sid, &volta, chegada, Some("De volta ao porto."), None);
    }

    /// A porta de entrada de tudo o que a colonia pede.
    pub(super) fn handle_colonia(&mut self, sid: SessionId, pedido: shared::colonia::PedidoColonia) {
        use shared::colonia::PedidoColonia as P;
        // Sem a escritura nao ha' o que abrir, colher ou melhorar. A trava
        // fica AQUI, e nao em cada braco: um braco novo nasceria sem ela.
        if !self.sessions.get(&sid).is_some_and(|s| s.colonia.tem) {
            self.avisa_colonia(sid, "Você ainda não tem uma ilha.");
            return;
        }
        match pedido {
            P::Painel => self.abrir_colonia(sid),
            P::Visitar => self.visitar_colonia(sid),
            P::Voltar => self.voltar_da_colonia(sid),
            P::Colher => {
                self.colher_colonia(sid);
                self.abrir_colonia(sid);
            }
            P::Melhorar { eixo } => {
                self.melhorar_colonia(sid, eixo);
                self.abrir_colonia(sid);
            }
        }
    }
}

/// A instancia de um personagem. Estavel: reconectar cai na mesma ilha.
///
/// Nao pode ser 0, que e' "fora de instancia" — e' assim que o resto do
/// mundo distingue quem esta' na colonia de quem esta' na ilha comum.
pub(crate) fn instancia_do_nome(nome: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in nome.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16777619);
    }
    h.max(1)
}
