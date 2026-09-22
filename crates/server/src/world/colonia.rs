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
    /// A `Ilha` da colonia deste TAMANHO, gerando na primeira vez que alguem
    /// daquele tamanho entra.
    ///
    /// A chave e' o RAIO, e nao a instancia: a semente e' uma so'
    /// (`shared::colonia::SEMENTE`), entao duas colonias do mesmo tamanho sao
    /// o mesmo relevo — guardar uma copia por jogador era pagar memoria por
    /// uma diferenca que nao existe. Sao SEIS ilhas no maximo, pra qualquer
    /// numero de jogadores.
    pub(crate) fn colonia_de(&mut self, nivel: u8) -> &shared::terreno::Ilha {
        // A chave e' o PLATO, em decimos de unidade: a ilha e' sempre a mesma
        // e do mesmo tamanho, e a unica coisa que o nivel muda e' quanto chao
        // sai aplainado em volta da praca. Cinco niveis = cinco relevos no
        // maximo, pra qualquer numero de jogadores.
        let plato = shared::colonia::plato_do_assentamento(nivel);
        let chave = (plato * 10.0) as i32;
        self.colonias.entry(chave).or_insert_with(|| {
            let t0 = std::time::Instant::now();
            let i = shared::terreno::Ilha::da_colonia(plato);
            tracing::info!("colonia com plato {plato:.0} pronta em {:?}", t0.elapsed());
            i
        })
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
        self.colher_colonia_com_aviso(sid, true)
    }

    /// Colher sem falar nada quando nao ha' o que colher.
    ///
    /// `contratar` e `demitir` colhem ANTES de mexer nas vagas, pra fechar o
    /// periodo com quem de fato trabalhou. Numa ilha vazia isso despejava
    /// "Ninguem mora aqui ainda" no meio do ato de CONTRATAR alguem — o jogo
    /// reclamando de um problema que o jogador estava resolvendo naquele
    /// segundo.
    pub(super) fn colher_colonia_quieto(&mut self, sid: SessionId) {
        self.colher_colonia_com_aviso(sid, false)
    }

    fn colher_colonia_com_aviso(&mut self, sid: SessionId, falar: bool) {
        let horas = self.horas_paradas(sid);
        let Some(s) = self.sessions.get(&sid) else {
            return;
        };
        let nivel = s.colonia.niveis[shared::colonia::eixo::RECURSOS];
        let ganho = shared::colonia::colheita(&s.colonia.trabalhadores, nivel, horas);
        if ganho.is_empty() {
            // Sem morador a ilha nao rende NADA, e dizer "volte mais tarde"
            // mandaria o jogador esperar por uma coisa que nao vai acontecer.
            let texto = if s.colonia.trabalhadores.is_empty() {
                "Ninguém mora aqui ainda. Suba o Assentamento e contrate alguém."
            } else {
                "A ilha ainda não rendeu nada. Volte mais tarde."
            };
            if falar {
                self.avisa_colonia(sid, texto);
            }
            return;
        }
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        // A colheita vai pro BAU DA ILHA, e nao pra bolsa.
        //
        // Ia pra bolsa, e isso apagava a ilha como lugar: dava pra administrar
        // a colonia inteira de outra ilha, sem nunca pisar nela. Agora colher
        // enche o bau, e buscar e' uma viagem — que e' o que o dono pediu.
        let espacos = shared::colonia::espacos_do_bau(
            s.colonia.niveis[shared::colonia::eixo::BANCO],
        );
        let mut texto = Vec::new();
        let mut sobrou = false;
        for (id, q) in &ganho {
            let fora = shared::colonia::guardar_no_bau(&mut s.colonia.bau, espacos, *id, *q);
            if fora > 0 {
                sobrou = true;
            } else {
                texto.push(format!("{q}× {}", crate::economy::nome_do_item(*id)));
            }
        }
        if texto.is_empty() {
            // Nada entrou: o relogio NAO e' zerado. Zerar apagaria as horas
            // de trabalho de quem chegou com o bau cheio, e o jogador nao
            // teria como saber o que perdeu.
            if falar {
                self.avisa_colonia(sid, "O baú da ilha está cheio. Retire o que há nele primeiro.");
            }
            return;
        }
        s.colonia.colhida_em = (now_ms() / 1000) as i64;
        let aviso = if sobrou {
            format!(
                "Colheita de {horas:.0}h no baú: {}. O resto não caibe — o baú está cheio.",
                texto.join(", ")
            )
        } else {
            format!("Colheita de {horas:.0}h, no baú da ilha: {}.", texto.join(", "))
        };
        self.avisa_colonia(sid, &aviso);
        self.passo_de_tutorial(sid, shared::quests::tutorial::COLONIA_COLHER);
        self.save_pending = true;
    }

    /// RETIRAR: tira do bau o que couber na bolsa.
    ///
    /// O que nao couber FICA no bau. Devolver pro chao ou sumir com o item
    /// seriam as duas formas de o jogador perder o que ele ja' tinha colhido.
    pub(super) fn retirar_do_bau(&mut self, sid: SessionId) {
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if s.colonia.bau.is_empty() {
            self.avisa_colonia(sid, "O baú está vazio.");
            return;
        }
        let mut levou = Vec::new();
        let mut ficou = 0;
        let mut resto: Vec<shared::InventorySlot> = Vec::new();
        for slot in std::mem::take(&mut s.colonia.bau) {
            if slot.qty == 0 {
                continue;
            }
            if slot.item_id == shared::item_id::COPPER {
                Self::pagar_em_cobre(s, slot.qty, "colonia");
                levou.push(format!(
                    "{}× {}",
                    slot.qty,
                    crate::economy::nome_do_item(slot.item_id)
                ));
                continue;
            }
            if add_to_inventory(&mut s.inventory, slot.item_id, slot.qty, None) {
                levou.push(format!(
                    "{}× {}",
                    slot.qty,
                    crate::economy::nome_do_item(slot.item_id)
                ));
            } else {
                ficou += 1;
                resto.push(slot);
            }
        }
        s.colonia.bau = resto;
        s.inventory_dirty = true;
        self.save_pending = true;
        let aviso = match (levou.is_empty(), ficou) {
            (true, _) => "A bolsa está cheia: nada saiu do baú.".to_string(),
            (false, 0) => format!("Do baú: {}.", levou.join(", ")),
            (false, n) => format!(
                "Do baú: {}. Ficaram {n} pilha(s) — a bolsa encheu.",
                levou.join(", ")
            ),
        };
        self.avisa_colonia(sid, &aviso);
        if !levou.is_empty() {
            self.passo_de_tutorial(sid, shared::quests::tutorial::COLONIA_RETIRAR);
        }
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
        let novo = s.colonia.niveis[e];
        let entidade = s.entity;
        let nome = shared::colonia::eixo::NOMES[e];
        // TAMANHO muda o RELEVO. Regerar na hora, e nao so' descartar: sem
        // ilha nenhuma a fisica cai no mapa de tiles velho, e o jogador
        // atravessa o chao da propria colonia ate' o proximo login.
        if e == shared::colonia::eixo::TAMANHO {
            // Nao se DESCARTA mais a ilha antiga: ela e' de um TAMANHO, nao
            // deste jogador, e pode haver outra gente do tamanho de antes.
            self.colonia_de(novo);
            let chave = (shared::colonia::plato_do_assentamento(novo) * 10.0) as i32;
            // A costa e' outra: quem estava na beirada pode ter virado agua.
            // Descer de novo e' mais barato que descobrir isso afogado.
            if let Some(ent) = entidade {
                let onde = self.ecs.get::<&Position>(ent).ok().map(|p| p.0);
                let destino = onde.and_then(|p| {
                    self.colonias
                        .get(&chave)
                        .map(|i| i.terra_mais_proxima(p.x, p.y, 400.0))
                });
                if let (Some(d), Ok(mut p)) = (destino, self.ecs.get::<&mut Position>(ent)) {
                    p.0 = d;
                }
            }
            let _ = self.mandar_terreno_da_colonia(sid);
        }
        self.avisa_colonia(sid, &format!("{nome} melhorado."));
        if e == shared::colonia::eixo::ASSENTAMENTO {
            self.passo_de_tutorial(sid, shared::quests::tutorial::COLONIA_ASSENTAMENTO);
        }
        self.save_pending = true;
    }

    /// Manda o cliente REDESENHAR a colonia. O relevo do cliente sai da mesma
    /// semente e do mesmo raio; o que ele nao tem como adivinhar e' que o raio
    /// mudou agora.
    fn mandar_terreno_da_colonia(&self, sid: SessionId) -> bool {
        let Some(s) = self.sessions.get(&sid) else {
            return false;
        };
        let nivel = s.colonia.niveis[shared::colonia::eixo::ASSENTAMENTO];
        let _ = s.handle.to_client.send(ServerMessage::Colonia {
            aviso: shared::colonia::AvisoColonia::Terreno {
                plato: shared::colonia::plato_do_assentamento(nivel),
                assentamento: nivel,
                trabalhadores: s.colonia.trabalhadores.clone(),
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
                    &s.colonia.trabalhadores,
                    s.colonia.niveis[shared::colonia::eixo::RECURSOS],
                    horas,
                ),
                custos,
                banco: shared::colonia::espacos_do_banco(
                    s.colonia.niveis[shared::colonia::eixo::BANCO],
                ),
                bau: s.colonia.bau.clone(),
                trabalhadores: s.colonia.trabalhadores.clone(),
                vagas: shared::colonia::vagas_de_trabalho(
                    s.colonia.niveis[shared::colonia::eixo::ASSENTAMENTO],
                ) as u8,
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

    /// CONTRATAR: poe (ou troca) o morador de uma vaga.
    ///
    /// A colheita e' contada ANTES, e nao depois: quem trocasse de oficio na
    /// hora de colher levaria as 12 h do lenhador como se fossem do
    /// minerador. Colher primeiro fecha o periodo com quem de fato trabalhou.
    fn contratar_na_colonia(&mut self, sid: SessionId, vaga: u8, oficio: u8) {
        let Some(p) = shared::colonia::Profissao::do_indice(oficio) else {
            self.avisa_colonia(sid, "Esse ofício não existe.");
            return;
        };
        let vagas = self
            .sessions
            .get(&sid)
            .map(|s| {
                shared::colonia::vagas_de_trabalho(
                    s.colonia.niveis[shared::colonia::eixo::ASSENTAMENTO],
                )
            })
            .unwrap_or(0);
        if (vaga as usize) >= vagas {
            self.avisa_colonia(
                sid,
                "Não há casa para mais ninguém. Suba o Assentamento primeiro.",
            );
            return;
        }
        self.colher_colonia_quieto(sid);
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        // O PRIMEIRO morador chega com a primeira carga.
        //
        // Sem isso, contratar e colher em seguida nao rende NADA: um lenhador
        // faz 8 de madeira por hora, entao a primeira unidade sai em 7,5
        // minutos. O passo "Colher" do tutorial abria e ficava esperando — o
        // jogador parado na ilha sem saber o que fazer, com o barco pra
        // Geleira atras disso.
        //
        // So' na primeira contratacao (colonia sem ninguem): depois o relogio
        // e' o relogio, e esperar faz parte.
        if s.colonia.trabalhadores.is_empty() {
            let uma_hora = 3_600;
            s.colonia.colhida_em = (now_ms() / 1000) as i64 - uma_hora;
        }
        let t = &mut s.colonia.trabalhadores;
        while t.len() <= vaga as usize {
            t.push(p);
        }
        t[vaga as usize] = p;
        t.truncate(vagas);
        self.save_pending = true;
        self.avisa_colonia(sid, &format!("{} mudou-se para a sua ilha.", p.nome()));
        self.passo_de_tutorial(sid, shared::quests::tutorial::COLONIA_CONTRATAR);
        let _ = self.mandar_terreno_da_colonia(sid);
    }

    /// DEMITIR: esvazia a vaga. A casa some e o rendimento dela para.
    fn demitir_na_colonia(&mut self, sid: SessionId, vaga: u8) {
        self.colher_colonia_quieto(sid);
        let Some(s) = self.sessions.get_mut(&sid) else {
            return;
        };
        if (vaga as usize) >= s.colonia.trabalhadores.len() {
            return;
        }
        let quem = s.colonia.trabalhadores.remove(vaga as usize);
        self.save_pending = true;
        self.avisa_colonia(sid, &format!("{} foi embora.", quem.nome()));
        let _ = self.mandar_terreno_da_colonia(sid);
    }

    /// A porta de entrada de tudo o que a colonia pede.
    pub(super) fn handle_colonia(&mut self, sid: SessionId, pedido: shared::colonia::PedidoColonia) {
        use shared::colonia::PedidoColonia as P;
        // SAIR nunca depende da escritura.
        //
        // A trava do `tem` vinha antes de tudo, inclusive do `Voltar` — e quem
        // estivesse DENTRO da colonia sem a escritura (save antigo, escritura
        // perdida, entrada por outro caminho) nao podia fazer nada ali, nem
        // ir embora. Nem pelo barqueiro, que passa por aqui. Ilha sem saida e'
        // armadilha, e esta fechava com o jogador dentro.
        let dentro_agora = shared::colonia::e_colonia(&self.zona);
        if matches!(pedido, P::Voltar) && dentro_agora {
            self.voltar_da_colonia(sid);
            return;
        }
        // Pro resto, sem a escritura nao ha' o que abrir, colher ou melhorar.
        // A trava fica AQUI, e nao em cada braco: braco novo nasce travado.
        if !self.sessions.get(&sid).is_some_and(|s| s.colonia.tem) {
            self.avisa_colonia(sid, "Você ainda não tem uma ilha.");
            return;
        }
        // O que MEXE na ilha so' se faz DENTRO dela. Antes dava pra subir o
        // assentamento, contratar e colher de qualquer canto do arquipelago —
        // e uma ilha que se administra de longe nao e' um lugar, e' uma aba
        // de menu. O dono pediu o contrario.
        //
        // A trava fica AQUI, na porta, e nao em cada braco: braco novo nasce
        // travado. `Painel` fica de fora (olhar de longe nao mexe em nada) e
        // `Visitar` tambem, que e' o que se faz PRA chegar.
        let dentro = dentro_agora;
        let mexe = !matches!(pedido, P::Painel | P::Visitar);
        if mexe && !dentro {
            self.avisa_colonia(sid, "Isso se faz na ilha. Fale com o Capitão do Porto.");
            return;
        }
        match pedido {
            P::Painel => {
                // O tutorial do mural fecha aqui: abrir o painel DENTRO da
                // ilha e' o gesto. De fora nao conta — de fora nao e' mural.
                if dentro {
                    self.passo_de_tutorial(sid, shared::quests::tutorial::COLONIA_MURAL);
                }
                self.abrir_colonia(sid)
            }
            P::Visitar => self.visitar_colonia(sid),
            P::Voltar => self.voltar_da_colonia(sid),
            P::Retirar => {
                self.retirar_do_bau(sid);
                self.abrir_colonia(sid);
            }
            P::Colher => {
                self.colher_colonia(sid);
                self.abrir_colonia(sid);
            }
            P::Melhorar { eixo } => {
                self.melhorar_colonia(sid, eixo);
                self.abrir_colonia(sid);
            }
            P::Contratar { vaga, oficio } => {
                self.contratar_na_colonia(sid, vaga, oficio);
                self.abrir_colonia(sid);
            }
            P::Demitir { vaga } => {
                self.demitir_na_colonia(sid, vaga);
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

#[cfg(test)]
mod testes {
    /// A instancia da SESSAO e a da ENTIDADE andam juntas — sempre.
    ///
    /// O filtro de AOI (`world.rs`, "candidatos por distancia") compara a
    /// instancia da SESSAO com a da ENTIDADE, e entidade sem o componente
    /// `Instancia` conta como 0. Entao pôr `s.instancia` sem pôr
    /// `Instancia(i)` no corpo nao esconde o jogador dos outros: esconde o
    /// MUNDO INTEIRO dele, inclusive ele mesmo. O snapshot chega com zero
    /// entidades e a tela fica sem nada — foi o que aconteceu na colonia.
    ///
    /// A dungeon sempre fez os dois na mesma linha; a colonia copiou metade.
    /// Este teste le' o FONTE porque o defeito nao e' de valor, e' de par:
    /// um teste de runtime precisaria de um mundo inteiro pra dizer o que
    /// duas linhas de codigo ja' dizem.
    #[test]
    fn instancia_da_sessao_e_da_entidade_andam_juntas() {
        let fonte = include_str!("../world.rs");
        let atribui = fonte
            .lines()
            .filter(|l| {
                let l = l.trim();
                l.starts_with("s.instancia =") || l.starts_with("session.instancia =")
            })
            .count();
        assert!(atribui > 0, "ninguem mais define a instancia da sessao?");
        // Toda atribuicao em `world.rs` tem que ser acompanhada, no mesmo
        // arquivo, de um `Instancia(...)` indo pra entidade.
        assert!(
            fonte.contains("Instancia(instancia_pendente)"),
            "a instancia da sessao foi definida sem por o componente na entidade: \
             o jogador nao vai ver nem a si mesmo"
        );
    }

    /// A ilha e' a MESMA a cada visita: a instancia sai do nome, e nada mais.
    #[test]
    fn a_instancia_e_estavel_e_nunca_zero() {
        for nome in ["sadasdas", "brunji", "A", "ÁÊÎÕÜ"] {
            let a = super::instancia_do_nome(nome);
            assert_eq!(a, super::instancia_do_nome(nome), "{nome}: instavel");
            // Zero e' "fora de instancia" — o mundo comum. Uma colonia que
            // caisse em 0 seria visivel de dentro do arquipelago.
            assert_ne!(a, 0, "{nome}: caiu em zero");
        }
        assert_ne!(
            super::instancia_do_nome("brunji"),
            super::instancia_do_nome("brunja")
        );
    }
}

#[cfg(test)]
mod testes_do_relevo {
    /// O relevo da colonia e' guardado por TAMANHO, e nunca por jogador.
    ///
    /// O dono recusou a ilha por personagem — "não quero que seja única, vai
    /// pesar e ter margem pra erro" — e o peso e' literal: o campo de altura
    /// de raio 160 sao 102.400 colunas. Uma por jogador no processo cresce com
    /// quem entra e nada a limita; uma por TAMANHO sao SEIS, com mil jogadores
    /// ou com um.
    ///
    /// O que sobra por personagem e' a INSTANCIA (ninguem entra na sua ilha) e
    /// os niveis dos eixos. O relevo, nao.
    #[test]
    fn o_relevo_e_por_tamanho_e_nao_por_jogador() {
        let fonte = include_str!("colonia.rs");
        assert!(
            fonte.contains("self.colonias.entry(raio)"),
            "o cache de relevo voltou a ser por jogador"
        );
        // Seis tamanhos, e so'.
        let raios: std::collections::HashSet<i32> = (0..=shared::colonia::NIVEL_MAX)
            .map(shared::colonia::raio_blocos)
            .collect();
        assert!(
            raios.len() <= 6,
            "{} tamanhos distintos: o cache deixou de ter teto",
            raios.len()
        );
    }

    /// Dois personagens diferentes pisam no MESMO chao.
    #[test]
    fn dois_jogadores_veem_a_mesma_ilha() {
        let a = shared::colonia::semente("brunji");
        let b = shared::colonia::semente("outro_qualquer");
        assert_eq!(a, b, "a semente voltou a sair do nome");
        let ilha = |s: i32| {
            shared::terreno::Ilha::gerar(
                s,
                shared::colonia::raio_blocos(1),
                shared::terreno::Bioma::Floresta,
                shared::terreno::ESCALA_ALTURA,
            )
        };
        let (ia, ib) = (ilha(a), ilha(b));
        for (x, z) in [(0.0, 0.0), (12.5, -8.0), (-30.0, 22.5), (60.0, 60.0)] {
            assert_eq!(
                ia.altura(x, z),
                ib.altura(x, z),
                "({x}, {z}) difere entre dois personagens"
            );
        }
    }
}
