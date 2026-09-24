//! Dungeons no cliente (docs/DUNGEONS_E_RAIDS.md, "Telas no cliente"):
//! a janela (Menu → Aventura → Dungeons), a faixa da fila, o pronto-check, o
//! HUD da instancia com a porta de sair, o "Reviver em N s" e o resultado com
//! o bau. Quem decide tudo e' o servidor; aqui so' se mostra e se pede.
//! Nada abre por tecla: so' toque/clique.

use std::collections::HashMap;

use macroquad::prelude::*;
use shared::dungeon::{self as dg, Aviso, Pedido, Tipo};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

pub struct Contexto<'a> {
    pub nomes: &'a HashMap<u16, String>,
    pub ouro: u64,
    /// Nome do personagem (pra saber se e' o lider da sala).
    pub eu: &'a str,
}

fn pedir(p: Pedido) -> ClientMessage {
    ClientMessage::Dungeon { pedido: p }
}

struct Estado {
    conteudos: Vec<dg::ConteudoEstado>,
    entradas: dg::EntradasNet,
    fila: Option<(dg::FilaNet, f64)>,
    sala: Option<dg::SalaNet>,
}

struct ProntoAberto {
    partida: u32,
    conteudo: u16,
    estagio: u8,
    membros: Vec<dg::MembroNet>,
    expira_em: f64,
    respondeu: bool,
}

struct Inst {
    conteudo: u16,
    estagio: u8,
    andar: u8,
    andares: u8,
    restante_s: u32,
    recebido: f64,
    inimigos: u16,
    reviver_em_s: Option<u16>,
    membros: Vec<dg::MembroDaInstancia>,
    concluida: bool,
}

struct Resultado {
    conteudo: u16,
    estagio: u8,
    vitoria: bool,
    tempo_s: u32,
    bonus: bool,
    primeira: bool,
    bau: Option<(Vec<(u16, u32)>, u32, u8)>,
    fechado: bool,
    /// `get_time()` no 1º desenho: a contagem ate' a instancia fechar.
    chegou: f64,
}

#[derive(Default)]
pub struct DungeonUi {
    pub aberto: bool,
    estado: Option<Estado>,
    sel: u16,
    estagio: u8,
    salas: Option<Vec<dg::SalaNet>>,
    completar_pela_fila: bool,
    pronto: Option<ProntoAberto>,
    inst: Option<Inst>,
    resultado: Option<Resultado>,
    aviso: Option<(String, bool, f64)>,
    /// AUTO DUNGEON ligado (a linha de missao abaixo da faixa).
    pub auto: bool,
    /// O jogador esta' na ARENA, a zona onde a fila e as salas existem?
    ///
    /// `None` = ainda nao se sabe (antes do primeiro `Estado`). Nesse caso a
    /// janela NAO trava nada: um servidor velho, ou um quadro antes da
    /// resposta, nao pode esconder os botoes de quem ja' esta' no lugar
    /// certo. Quem decide de verdade e' o servidor, que recusa.
    pub na_arena: Option<bool>,
}

/// Altura da linha de missao da instancia (antes do fator de texto).
const MISSAO_H: f32 = 40.0;

fn mmss(s: u32) -> String {
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn nome_do_conteudo(id: u16) -> &'static str {
    dg::conteudo(id).map_or("Dungeon", |c| c.nome)
}

fn botao(r: Rect, rotulo: &str, ativo: bool, primario: bool) -> bool {
    let m = Vec2::from(mouse_position());
    let sobre = ativo && r.contains(m);
    estilo::botao(
        r,
        rotulo,
        estilo::estado(
            sobre,
            sobre && is_mouse_button_down(MouseButton::Left),
            !ativo,
            false,
        ),
        primario,
    );
    sobre && crate::foco::clique()
}

impl DungeonUi {
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        self.salas = None;
        vec![pedir(Pedido::Estado)]
    }

    /// Abre ja' com a dungeon `conteudo` escolhida (0 = a de sempre).
    pub fn abrir_em(&mut self, conteudo: u16) -> Vec<ClientMessage> {
        if conteudo != 0 {
            self.sel = conteudo;
        }
        self.abrir()
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.salas = None;
    }

    pub fn na_instancia(&self) -> bool {
        self.inst.is_some()
    }

    /// O que o AUTO DUNGEON precisa saber da instancia (sem o bau, que vem do
    /// mundo).
    pub fn estado_auto(&self, agora: f64) -> Option<crate::auto_dungeon::Estado> {
        let i = self.inst.as_ref()?;
        let caido = i.reviver_em_s.is_some();
        let reviver_pronto = i
            .reviver_em_s
            .is_some_and(|s| agora - i.recebido >= s as f64);
        let r = self.resultado.as_ref();
        Some(crate::auto_dungeon::Estado {
            caido,
            reviver_pronto,
            vitoria: i.concluida || r.is_some_and(|r| r.vitoria),
            bau_aberto: r.is_some_and(|r| r.bau.is_some()),
            bau: None,
        })
    }

    /// Janela, pronto-check ou resultado na frente: o clique nao e' do mundo.
    pub fn pega_mouse(&self) -> bool {
        if self.aberto
            || self.pronto.is_some()
            || self.resultado.as_ref().is_some_and(|r| !r.fechado)
        {
            return true;
        }
        let m = Vec2::from(mouse_position());
        let caido = self.inst.as_ref().is_some_and(|i| i.reviver_em_s.is_some());
        let (faixa, porta, reviver) = Self::rects_da_instancia();
        self.inst.is_some()
            && (faixa.contains(m)
                || porta.contains(m)
                || Self::missao_rect().contains(m)
                || (caido && reviver.contains(m)))
    }

    /// O que o servidor mandou. Devolve pedidos de volta (raro).
    pub fn aviso(&mut self, a: Aviso, agora: f64) -> Vec<ClientMessage> {
        match a {
            Aviso::Estado {
                conteudos,
                entradas,
                fila,
                sala,
            } => {
                if self.sel == 0 || !conteudos.iter().any(|c| c.id == self.sel) {
                    self.sel = conteudos
                        .iter()
                        .find(|c| {
                            dg::conteudo(c.id).is_some_and(|d| d.disponivel)
                                && c.cadeados.first().is_some_and(|x| x.is_none())
                        })
                        .or(conteudos.first())
                        .map_or(0, |c| c.id);
                    self.estagio = 1;
                }
                self.estado = Some(Estado {
                    conteudos,
                    entradas,
                    fila: fila.map(|f| (f, agora)),
                    sala,
                });
            }
            Aviso::Salas { lista } => self.salas = Some(lista),
            Aviso::Pronto {
                partida,
                conteudo,
                estagio,
                membros,
                expira_s,
            } => {
                self.pronto = Some(ProntoAberto {
                    partida,
                    conteudo,
                    estagio,
                    membros,
                    expira_em: agora + expira_s as f64,
                    respondeu: false,
                });
            }
            Aviso::ProntoFechou { partida, texto } => {
                if self.pronto.as_ref().is_some_and(|p| p.partida == partida) {
                    self.pronto = None;
                }
                self.aviso = Some((texto, false, agora));
            }
            Aviso::Instancia {
                conteudo,
                estagio,
                andar,
                andares,
                restante_s,
                inimigos,
                reviver_em_s,
                membros,
                concluida,
            } => {
                if self.inst.is_none() {
                    // Entrou: a janela e o pronto-check saem da frente.
                    self.aberto = false;
                    self.pronto = None;
                    self.resultado = None;
                }
                self.inst = Some(Inst {
                    conteudo,
                    estagio,
                    andar,
                    andares,
                    restante_s,
                    recebido: agora,
                    inimigos,
                    reviver_em_s,
                    membros,
                    concluida,
                });
            }
            Aviso::Resultado {
                conteudo,
                estagio,
                vitoria,
                tempo_s,
                bonus_tempo,
                primeira_vitoria,
            } => {
                self.resultado = Some(Resultado {
                    conteudo,
                    estagio,
                    vitoria,
                    tempo_s,
                    bonus: bonus_tempo,
                    primeira: primeira_vitoria,
                    bau: None,
                    fechado: false,
                    chegou: 0.0,
                });
            }
            Aviso::Bau {
                itens,
                marcas,
                no_correio,
            } => {
                if let Some(r) = &mut self.resultado {
                    r.bau = Some((itens, marcas, no_correio));
                    r.fechado = false;
                }
            }
            Aviso::Saiu => {
                self.auto = false;
                self.inst = None;
                self.resultado = None;
                return vec![pedir(Pedido::Estado)];
            }
            Aviso::Correio { .. } => {}
            Aviso::NaArena { dentro } => self.na_arena = Some(dentro),
            // O servidor recusou porque a mesa nao mora nesta zona. Corrige o
            // que a janela achava e deixa ela oferecer a viagem — a recusa
            // vira convite, que e' o ponto.
            Aviso::PrecisaDaArena => self.na_arena = Some(false),
            Aviso::Texto { ok, texto } => self.aviso = Some((texto, ok, agora)),
        }
        Vec::new()
    }

    /// Texto curto pro chat (o que acabou de acontecer), se houver.
    pub fn texto_pro_chat(a: &Aviso) -> Option<String> {
        match a {
            Aviso::Texto { texto, .. } | Aviso::ProntoFechou { texto, .. } => {
                Some(format!("Dungeon: {texto}"))
            }
            Aviso::Resultado {
                vitoria: true,
                conteudo,
                ..
            } => Some(format!(
                "Vitória em {}! Toque no baú.",
                nome_do_conteudo(*conteudo)
            )),
            Aviso::Resultado { vitoria: false, .. } => {
                Some("Tempo esgotado: o estágio falhou.".into())
            }
            _ => None,
        }
    }

    pub fn desenha(&mut self, c: &Contexto, agora: f64) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        if self.inst.is_some() {
            self.desenha_instancia(agora, &mut saida);
        } else if !self.aberto {
            self.desenha_faixa_da_fila(agora, &mut saida);
        }
        estilo::no_painel(estilo::escala_do_painel(980.0, 660.0), || {
            if self.aberto {
                self.desenha_janela(c, agora, &mut saida);
            }
            self.desenha_resultado(c, &mut saida);
            self.desenha_pronto(agora, &mut saida);
        });
        saida
    }

    // ─────────────────────────────── janela ───────────────────────────────

    fn desenha_janela(&mut self, c: &Contexto, agora: f64, saida: &mut Vec<ClientMessage>) {
        crate::hud_layout::escurece(0.55);
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = (
            (980.0 * f).min(seguro.w - 16.0),
            (660.0 * f).min(seguro.h - 16.0),
        );
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        estilo::painel(p);
        estilo::texto(p.x + 18.0 * f, p.y + 34.0 * f, "Dungeons", 22, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 10.0 * f, 38.0 * f, 34.0 * f);
        if crate::ui::botao(fechar, "x", true) {
            self.fechar();
            return;
        }
        // A VOLTA fica no cabeçalho, e nao no meio dos botoes de entrar: quem
        // esta' na Arena chegou por handoff e nao tem barco nem mapa daqui —
        // sem este botao, a unica saida seria deslogar.
        if self.na_arena == Some(true) {
            let voltar = Rect::new(p.x + p.w - 190.0 * f, p.y + 10.0 * f, 134.0 * f, 34.0 * f);
            if botao(voltar, "Sair da Arena", true, false) {
                saida.push(pedir(Pedido::SairDaArena));
                self.fechar();
                return;
            }
        }
        let Some(estado) = &self.estado else {
            estilo::texto(
                p.x + 18.0 * f,
                p.y + 80.0 * f,
                "Carregando…",
                16,
                estilo::SUAVE,
            );
            return;
        };
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();

        // ── esquerda: os conteudos ──
        let esq = Rect::new(
            p.x + 18.0 * f,
            p.y + 56.0 * f,
            (p.w * 0.38).floor(),
            p.h - 56.0 * f - 44.0 * f,
        );
        let alt = ((esq.h / estado.conteudos.len().max(1) as f32).min(62.0 * f)).max(36.0 * f);
        let mut novo_sel = None;
        for (i, ce) in estado.conteudos.iter().enumerate() {
            let Some(def) = dg::conteudo(ce.id) else {
                continue;
            };
            let r = Rect::new(esq.x, esq.y + i as f32 * alt, esq.w, alt - 4.0 * f);
            let sobre = r.contains(m);
            estilo::cartao(r, sobre, self.sel == ce.id);
            let travado = !def.disponivel || ce.cadeados.first().is_some_and(|x| x.is_some());
            let cor = if travado {
                estilo::SUAVE
            } else {
                estilo::TEXTO
            };
            estilo::texto_ajustado(
                def.nome,
                r.x + 12.0 * f,
                r.y + r.h * 0.45,
                r.w - 24.0 * f,
                15,
                cor,
            );
            let ilha = shared::terreno::def_da_zona(def.zona).map_or("Ilha nova", |d| d.nome);
            let sub = if def.disponivel {
                format!("{} · nível {} · {}", def.tipo.nome(), def.nivel_min, ilha)
            } else {
                format!("{} · Em breve", def.tipo.nome())
            };
            estilo::texto_ajustado(
                &sub,
                r.x + 12.0 * f,
                r.y + r.h * 0.82,
                r.w - 24.0 * f,
                11,
                estilo::SUAVE,
            );
            if sobre && clicou {
                novo_sel = Some(ce.id);
            }
        }
        if let Some(id) = novo_sel {
            self.sel = id;
            self.estagio = 1;
            self.salas = None;
        }

        // ── direita: o selecionado ──
        let dir = Rect::new(
            esq.x + esq.w + 18.0 * f,
            esq.y,
            p.x + p.w - 18.0 * f - (esq.x + esq.w + 18.0 * f),
            esq.h,
        );
        let (Some(def), Some(ce)) = (
            dg::conteudo(self.sel),
            estado.conteudos.iter().find(|x| x.id == self.sel),
        ) else {
            return;
        };
        let mut y = dir.y + 22.0 * f;
        estilo::texto_forte(dir.x, y, def.nome, 20, estilo::OURO);
        y += 22.0 * f;
        let grupo = if def.grupo_max <= 1 {
            "Solo".to_string()
        } else {
            format!("Grupo de até {}", def.grupo_max)
        };
        estilo::texto(
            dir.x,
            y,
            &format!(
                "{} · {} · limite {} · vitórias {}",
                def.tipo.nome(),
                grupo,
                mmss(def.limite_s),
                ce.vitorias
            ),
            13,
            estilo::SUAVE,
        );
        y += 26.0 * f;

        // Estagios 1–5 com cadeado; o motivo do selecionado embaixo.
        let n = dg::estagios(def);
        if n > 1 {
            estilo::texto(dir.x, y, "Estágio", 13, estilo::SUAVE);
            y += 8.0 * f;
            let bw = ((dir.w - 8.0 * f * 4.0) / 5.0).min(84.0 * f);
            for e in 1..=n {
                let r = Rect::new(dir.x + (e - 1) as f32 * (bw + 8.0 * f), y, bw, 44.0 * f);
                let travado = ce
                    .cadeados
                    .get((e - 1) as usize)
                    .is_some_and(|x| x.is_some());
                let sobre = r.contains(m);
                estilo::cartao(r, sobre, self.estagio == e);
                // Cadeado desenhado: a fonte da HUD nao tem o emoji.
                let rot = e.to_string();
                if travado {
                    crate::menu_missoes::cadeado(
                        vec2(r.center().x + 16.0 * f, r.center().y),
                        7.0 * f,
                        estilo::SUAVE,
                    );
                }
                estilo::texto_centro_forte(
                    r.center().x - if travado { 8.0 * f } else { 0.0 },
                    r.center().y + 6.0 * f,
                    &rot,
                    16,
                    if travado {
                        estilo::SUAVE
                    } else {
                        estilo::TEXTO
                    },
                );
                if sobre && clicou {
                    self.estagio = e;
                    self.salas = None;
                }
            }
            y += 60.0 * f;
        }
        let estagio = self.estagio.clamp(1, n);
        let nivel = dg::nivel_do_estagio(def, estagio);
        let cadeado = ce.cadeados.get((estagio - 1) as usize).cloned().flatten();
        let teto = dg::teto_de_grau(nivel);
        estilo::texto(
            dir.x,
            y,
            &format!(
                "Inimigos nível {nivel} · poder mínimo {} · peça até {}",
                dg::poder_minimo(def, estagio),
                teto.nome()
            ),
            13,
            estilo::TEXTO,
        );
        y += 20.0 * f;
        let chave = shared::chaves::faixa(nivel);
        estilo::texto(
            dir.x,
            y,
            &format!(
                "Chave de craft {} {:.0}% · Marcas da Tempestade",
                shared::chaves::nome_da_cor(chave.cor),
                chave.chance * 100.0
            ),
            13,
            estilo::SUAVE,
        );
        y += 24.0 * f;
        if let Some(cad) = &cadeado {
            estilo::texto(
                dir.x,
                y,
                &format!("Fechado: {}", cad.texto()),
                15,
                estilo::VERMELHO,
            );
            y += 24.0 * f;
        }

        // Entradas.
        let bw = (dir.w - 12.0 * f) * 0.5;
        match def.tipo {
            Tipo::Porao => {
                estilo::texto(
                    dir.x,
                    y,
                    &format!(
                        "Recompensas hoje: {}/{}",
                        estado.entradas.porao,
                        dg::PORAO_RECOMPENSAS_POR_DIA
                    ),
                    14,
                    estilo::TEXTO,
                );
                y += 18.0 * f;
                estilo::texto(
                    dir.x,
                    y,
                    "Entrada livre; sem recompensa, só cobre e Marcas.",
                    12,
                    estilo::SUAVE,
                );
            }
            _ => {
                estilo::texto(
                    dir.x,
                    y,
                    &format!(
                        "Entradas: {}/{} (2 por dia)",
                        estado.entradas.gruta,
                        dg::GRUTA_ACUMULA
                    ),
                    14,
                    estilo::TEXTO,
                );
                y += 18.0 * f;
                estilo::texto(
                    dir.x,
                    y,
                    "Sem entrada você vai como Ajudante: sem peça nem chave.",
                    12,
                    estilo::SUAVE,
                );
                if let Some(preco) = estado.entradas.gruta_preco {
                    let r = Rect::new(dir.x + dir.w - bw, y - 30.0 * f, bw, 36.0 * f);
                    if botao(
                        r,
                        &format!("Comprar entrada · {} ouro", crate::economia::milhar(preco)),
                        c.ouro >= preco,
                        false,
                    ) {
                        saida.push(pedir(Pedido::ComprarEntrada));
                    }
                    // O preço TRIPLICA a cada compra do dia, e isso tem que
                    // estar escrito: sem isso o jogador compra a segunda sem
                    // saber que ela custa o triplo da primeira.
                    y += 18.0 * f;
                    estilo::texto(
                        dir.x,
                        y,
                        "Cada compra do dia custa o triplo da anterior.",
                        12,
                        estilo::SUAVE,
                    );
                }
            }
        }
        y += 22.0 * f;
        estilo::separador(dir.x, y, dir.w);
        y += 14.0 * f;

        // ── FORA DA ARENA: um convite, e nao seis botoes mortos ──
        //
        // A fila e as salas vivem num processo so' (`shared::arena`), e e' isso
        // que faz elas serem as MESMAS pra todo mundo: o dono criou uma sala
        // com um personagem e nao a viu com o outro justamente porque cada
        // zona tinha a propria mesa.
        //
        // Entao daqui de fora nao ha' o que clicar. Em vez de deixar os botoes
        // la' pra falharem um a um, a janela troca todos por um: a viagem.
        if self.na_arena == Some(false) {
            estilo::texto(
                dir.x,
                y + 18.0 * f,
                "A fila e as salas ficam na Arena.",
                16,
                estilo::TEXTO,
            );
            estilo::texto(
                dir.x,
                y + 40.0 * f,
                "Você volta para onde está ao sair de lá.",
                13,
                estilo::SUAVE,
            );
            if botao(
                Rect::new(dir.x, y + 54.0 * f, bw * 1.4, 46.0 * f),
                "Ir para a Arena",
                true,
                true,
            ) {
                saida.push(pedir(Pedido::IrParaArena));
            }
            return;
        }

        let aberto = cadeado.is_none() && def.disponivel;
        match def.tipo {
            Tipo::Porao => {
                if botao(Rect::new(dir.x, y, bw, 46.0 * f), "Entrar", aberto, true) {
                    saida.push(pedir(Pedido::EntrarSolo { conteudo: def.id }));
                }
            }
            Tipo::Cacada => {
                estilo::texto(
                    dir.x,
                    y + 26.0 * f,
                    "A Caçada (raid) chega em breve.",
                    15,
                    estilo::SUAVE,
                );
            }
            Tipo::Gruta => {
                let fila_aqui = estado
                    .fila
                    .as_ref()
                    .filter(|(fl, _)| fl.conteudo == def.id && fl.estagio == estagio);
                if let Some((fl, desde)) = fila_aqui {
                    let espera = fl.esperando_s + (agora - desde).max(0.0) as u32;
                    estilo::texto(
                        dir.x,
                        y + 18.0 * f,
                        &format!(
                            "Procurando grupo · {} · {} na fila",
                            mmss(espera),
                            fl.na_fila
                        ),
                        15,
                        estilo::AUTO,
                    );
                    if botao(
                        Rect::new(dir.x + dir.w - bw, y, bw, 40.0 * f),
                        "Sair da fila",
                        true,
                        false,
                    ) {
                        saida.push(pedir(Pedido::FilaSair));
                    }
                } else {
                    if botao(
                        Rect::new(dir.x, y, bw, 40.0 * f),
                        "Entrar na fila",
                        aberto && estado.sala.is_none(),
                        true,
                    ) {
                        saida.push(pedir(Pedido::FilaEntrar {
                            conteudo: def.id,
                            estagio,
                        }));
                    }
                    // Sem grupo: vai sozinho (os inimigos tem menos vida).
                    if botao(
                        Rect::new(dir.x + dir.w - bw, y, bw, 40.0 * f),
                        "Entrar sozinho",
                        aberto && estado.sala.is_none(),
                        false,
                    ) {
                        saida.push(pedir(Pedido::GrutaSolo {
                            conteudo: def.id,
                            estagio,
                        }));
                    }
                }
                y += 50.0 * f;
                if let Some(sala) = &estado.sala {
                    self.desenha_sala(sala, c, dir, y, f, saida);
                } else {
                    if botao(
                        Rect::new(dir.x, y, bw, 40.0 * f),
                        "Criar sala",
                        aberto,
                        false,
                    ) {
                        saida.push(pedir(Pedido::SalaCriar {
                            conteudo: def.id,
                            estagio,
                            completar_pela_fila: self.completar_pela_fila,
                        }));
                    }
                    if botao(
                        Rect::new(dir.x + dir.w - bw, y, bw, 40.0 * f),
                        "Procurar salas",
                        def.disponivel,
                        false,
                    ) {
                        saida.push(pedir(Pedido::SalasBuscar {
                            conteudo: def.id,
                            estagio,
                        }));
                    }
                    y += 48.0 * f;
                    let toggle = Rect::new(dir.x, y, dir.w, 30.0 * f);
                    // Caixinha desenhada: a fonte nao tem ☐/☑.
                    let cx = Rect::new(dir.x, y + 6.0 * f, 18.0 * f, 18.0 * f);
                    estilo::borda_arredondada(cx, 3.0 * f, 1.5, estilo::TEXTO);
                    if self.completar_pela_fila {
                        estilo::ret_arredondado(
                            Rect::new(cx.x + 4.0 * f, cx.y + 4.0 * f, cx.w - 8.0 * f, cx.h - 8.0 * f),
                            2.0 * f,
                            estilo::AUTO,
                        );
                    }
                    estilo::texto(
                        dir.x + 26.0 * f,
                        y + 20.0 * f,
                        "Completar as vagas pela fila depois de 1 min",
                        13,
                        estilo::TEXTO,
                    );
                    if toggle.contains(m) && clicou {
                        self.completar_pela_fila = !self.completar_pela_fila;
                    }
                    y += 38.0 * f;
                    if let Some(salas) = &self.salas {
                        if salas.is_empty() {
                            estilo::texto(
                                dir.x,
                                y + 18.0 * f,
                                "Nenhuma sala aberta neste estágio.",
                                14,
                                estilo::SUAVE,
                            );
                        }
                        let alt = 46.0 * f;
                        let cabem = (((dir.y + dir.h) - y) / alt).floor().max(0.0) as usize;
                        for (i, s) in salas.iter().take(cabem).enumerate() {
                            let r = Rect::new(dir.x, y + i as f32 * alt, dir.w, alt - 4.0 * f);
                            estilo::cartao(r, false, false);
                            let lider = s
                                .membros
                                .iter()
                                .find(|x| x.lider)
                                .map_or("?", |x| x.nome.as_str());
                            estilo::texto_ajustado(
                                &format!(
                                    "{lider} · {} jogador(es) · {} vaga(s)",
                                    s.membros.len(),
                                    s.vagas
                                ),
                                r.x + 10.0 * f,
                                r.y + r.h * 0.62,
                                r.w - 130.0 * f,
                                14,
                                estilo::TEXTO,
                            );
                            if botao(
                                Rect::new(
                                    r.x + r.w - 110.0 * f,
                                    r.y + 4.0 * f,
                                    104.0 * f,
                                    r.h - 8.0 * f,
                                ),
                                "Entrar",
                                s.vagas > 0 && aberto,
                                true,
                            ) {
                                saida.push(pedir(Pedido::SalaEntrar { sala: s.id }));
                            }
                        }
                    }
                }
            }
        }

        if let Some((texto, ok, quando)) = &self.aviso {
            if agora - quando < 6.0 {
                let cor = if *ok { estilo::VERDE } else { estilo::VERMELHO };
                estilo::texto_ajustado(
                    texto,
                    p.x + 18.0 * f,
                    p.y + p.h - 16.0 * f,
                    p.w - 36.0 * f,
                    14,
                    cor,
                );
            }
        }
    }

    fn desenha_sala(
        &self,
        sala: &dg::SalaNet,
        c: &Contexto,
        dir: Rect,
        mut y: f32,
        f: f32,
        saida: &mut Vec<ClientMessage>,
    ) {
        let eu_lider = sala
            .membros
            .iter()
            .any(|m| m.lider && m.nome.split('@').next() == Some(c.eu));
        estilo::texto_forte(
            dir.x,
            y + 14.0 * f,
            &format!(
                "Sua sala · {} · estágio {}",
                nome_do_conteudo(sala.conteudo),
                sala.estagio
            ),
            15,
            estilo::OURO,
        );
        y += 26.0 * f;
        for m in &sala.membros {
            let coroa = if m.lider { " (líder)" } else { "" };
            estilo::texto(
                dir.x + 8.0 * f,
                y + 14.0 * f,
                &format!("• {}{coroa}", m.nome),
                14,
                estilo::TEXTO,
            );
            y += 20.0 * f;
        }
        if sala.completar_pela_fila {
            estilo::texto(
                dir.x + 8.0 * f,
                y + 14.0 * f,
                "Completa as vagas pela fila.",
                12,
                estilo::SUAVE,
            );
            y += 20.0 * f;
        }
        y += 10.0 * f;
        let bw = (dir.w - 12.0 * f) * 0.5;
        if botao(
            Rect::new(dir.x, y, bw, 40.0 * f),
            if eu_lider {
                "Começar"
            } else {
                "Só o líder começa"
            },
            eu_lider,
            true,
        ) {
            saida.push(pedir(Pedido::SalaIniciar));
        }
        if botao(
            Rect::new(dir.x + dir.w - bw, y, bw, 40.0 * f),
            "Sair da sala",
            true,
            false,
        ) {
            saida.push(pedir(Pedido::SalaSair));
        }
    }

    // ─────────────────────────────── fila ───────────────────────────────

    fn desenha_faixa_da_fila(&mut self, agora: f64, saida: &mut Vec<ClientMessage>) {
        let Some((fl, desde)) = self.estado.as_ref().and_then(|e| e.fila.as_ref()) else {
            return;
        };
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (520.0 * f).min(seguro.w - 32.0);
        let r = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.y + 104.0 * f,
            w,
            40.0 * f,
        );
        estilo::painel(r);
        let espera = fl.esperando_s + (agora - desde).max(0.0) as u32;
        let t = format!(
            "PROCURANDO GRUPO · {} · est. {} · {} · {} na fila",
            nome_do_conteudo(fl.conteudo),
            fl.estagio,
            mmss(espera),
            fl.na_fila
        );
        estilo::texto_ajustado(
            &t,
            r.x + 12.0 * f,
            r.y + r.h * 0.64,
            r.w - 110.0 * f,
            13,
            estilo::AUTO,
        );
        if botao(
            Rect::new(r.x + r.w - 92.0 * f, r.y + 4.0 * f, 86.0 * f, r.h - 8.0 * f),
            "Sair",
            true,
            false,
        ) {
            saida.push(pedir(Pedido::FilaSair));
        }
    }

    // ─────────────────────────────── pronto-check ───────────────────────────────

    fn desenha_pronto(&mut self, agora: f64, saida: &mut Vec<ClientMessage>) {
        let Some(p) = &mut self.pronto else { return };
        let falta = (p.expira_em - agora).max(0.0);
        if falta <= 0.0 {
            self.pronto = None;
            return;
        }
        crate::hud_layout::escurece(0.45);
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let (w, h) = (
            (460.0 * f).min(seguro.w - 16.0),
            (170.0 * f + 22.0 * f * p.membros.len() as f32).min(seguro.h - 16.0),
        );
        let r = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        estilo::painel_destaque(r, estilo::OURO);
        estilo::texto_centro_forte(
            r.center().x,
            r.y + 32.0 * f,
            "Grupo encontrado",
            20,
            estilo::OURO,
        );
        estilo::texto_centro(
            r.center().x,
            r.y + 56.0 * f,
            &format!(
                "{} · estágio {} · {:.0} s",
                nome_do_conteudo(p.conteudo),
                p.estagio,
                falta.ceil()
            ),
            14,
            estilo::TEXTO,
        );
        for (i, m) in p.membros.iter().enumerate() {
            estilo::texto(
                r.x + 24.0 * f,
                r.y + 84.0 * f + i as f32 * 22.0 * f,
                &format!("• {}", m.nome),
                14,
                estilo::TEXTO,
            );
        }
        let bw = (r.w - 60.0 * f) * 0.5;
        let by = r.y + r.h - 56.0 * f;
        if p.respondeu {
            estilo::texto_centro(
                r.center().x,
                by + 26.0 * f,
                "Esperando os outros…",
                15,
                estilo::SUAVE,
            );
            return;
        }
        if botao(
            Rect::new(r.x + 20.0 * f, by, bw, 42.0 * f),
            "Aceitar",
            true,
            true,
        ) {
            p.respondeu = true;
            saida.push(pedir(Pedido::Pronto {
                partida: p.partida,
                aceito: true,
            }));
        }
        if botao(
            Rect::new(r.x + r.w - 20.0 * f - bw, by, bw, 42.0 * f),
            "Recusar",
            true,
            false,
        ) {
            saida.push(pedir(Pedido::Pronto {
                partida: p.partida,
                aceito: false,
            }));
            self.pronto = None;
        }
    }

    // ─────────────────────────────── instancia ───────────────────────────────

    /// A linha de missao "Completar ..." entre a faixa e a porta. Tocar liga
    /// e desliga o AUTO DUNGEON.
    fn missao_rect() -> Rect {
        Self::missao_rect_em(&crate::hud_layout::atual())
    }

    /// Fora do desenho pra poder ser medida junto com o relatório — ver
    /// `o_relatorio_da_dungeon_fica_na_coluna_da_esquerda`.
    fn missao_rect_em(z: &crate::hud_layout::Zonas) -> Rect {
        // No lugar do rastreador de missoes: dentro da dungeon a missao E' a
        // dungeon, e as missoes normais nem aparecem (pedido do dono).
        Rect::new(
            z.rastreador.x,
            z.rastreador.y,
            z.rastreador.w.min(440.0 * z.s),
            MISSAO_H * z.s,
        )
    }

    /// (relatorio da dungeon, porta de sair, botao de reviver).
    ///
    /// NA COLUNA DA ESQUERDA, logo abaixo da linha de missão — e não mais
    /// centrado no alto da tela.
    ///
    /// O dono: "na dungeon os status de andar etc, o relatório dela no geral,
    /// tem que ficar na lateral, no lugar onde ficam as infos da Ilha Mágica;
    /// lá em cima é bem ruim". É a mesma faixa da tarja da ilha, e pelo mesmo
    /// motivo: andar, tempo e grupo se leem junto do que se está fazendo, e
    /// no alto do meio eles disputavam o lugar onde o olho procura o alvo.
    ///
    /// Cabe: dentro da dungeon o rastreador vira a linha de missão (40 de
    /// altura contra os ~300 do rastreador), então a coluna fica com o vão
    /// inteiro livre até os avisos.
    fn rects_da_instancia() -> (Rect, Rect, Rect) {
        Self::rects_da_instancia_em(
            &crate::hud_layout::atual(),
            crate::hud_layout::tela_segura(),
            estilo::fator_texto(),
        )
    }

    fn rects_da_instancia_em(
        z: &crate::hud_layout::Zonas,
        seguro: Rect,
        f: f32,
    ) -> (Rect, Rect, Rect) {
        let missao = Self::missao_rect_em(z);
        let faixa = Rect::new(
            missao.x,
            missao.y + missao.h + 6.0 * z.s,
            missao.w,
            58.0 * z.s,
        );
        // O SAIR ancora na direita da faixa, e não no meio: à esquerda fica o
        // texto, que muda de tamanho a cada andar — botão que anda é botão
        // que se erra.
        // PISO EM PIXELS CRUS. `area_de_toque` cresce o alvo até o dedo, mas
        // com teto (+14): numa tela de escala 0,7 os 34 viravam 24, e nem com
        // o crescimento chegavam aos 44 pt da Apple. Foi o teste que pegou.
        let alto = (34.0 * z.s).max(34.0);
        let larg = (128.0 * z.s).max(112.0).min(faixa.w);
        let porta = Rect::new(
            faixa.x + faixa.w - larg,
            faixa.y + faixa.h + 6.0 * z.s,
            larg,
            alto,
        );
        let reviver = Rect::new(
            seguro.center().x - 130.0 * f,
            seguro.center().y + 20.0 * f,
            260.0 * f,
            48.0 * f,
        );
        (faixa, porta, reviver)
    }

    fn desenha_instancia(&mut self, agora: f64, saida: &mut Vec<ClientMessage>) {
        let Some(i) = &self.inst else { return };
        let f = estilo::fator_texto();
        let (faixa, porta, reviver) = Self::rects_da_instancia();
        estilo::painel(faixa);
        // O texto da faixa mede em ESCALA DE LAYOUT, como o retângulo: a
        // faixa passou a sair de `missao_rect` (que é layout), e misturar o
        // fator de texto aqui faria a segunda linha cair fora dela nas telas
        // em que os dois não batem.
        let fl = crate::hud_layout::atual().s.max(0.5);
        let andar = if i.andar >= i.andares {
            "Chefe".to_string()
        } else {
            format!("Andar {}/{}", i.andar + 1, i.andares)
        };
        let titulo = format!(
            "{} · estágio {} · {andar}",
            nome_do_conteudo(i.conteudo),
            i.estagio
        );
        estilo::texto_ajustado(
            &titulo,
            faixa.x + 12.0 * fl,
            faixa.y + 23.0 * fl,
            faixa.w - 24.0 * fl,
            15,
            estilo::OURO,
        );
        let restante = i
            .restante_s
            .saturating_sub((agora - i.recebido).max(0.0) as u32);
        let linha = if i.concluida {
            "Vitória! Toque no baú para abrir.".to_string()
        } else {
            format!(
                "{} · inimigos {} · grupo {}/{}",
                mmss(restante),
                i.inimigos,
                i.membros.iter().filter(|m| m.vivo).count(),
                i.membros.len()
            )
        };
        let cor = if !i.concluida && restante < 60 {
            estilo::VERMELHO
        } else {
            estilo::TEXTO
        };
        estilo::texto_ajustado(
            &linha,
            faixa.x + 12.0 * fl,
            faixa.y + 45.0 * fl,
            faixa.w - 24.0 * fl,
            13,
            cor,
        );
        // ── a missao: completar a dungeon no automatico ──
        let missao = Self::missao_rect();
        let fm = crate::hud_layout::atual().s.max(0.5);
        let sobre = missao.contains(Vec2::from(mouse_position()));
        let cor = if self.auto {
            estilo::AUTO
        } else {
            estilo::OURO
        };
        estilo::painel(missao);
        draw_rectangle(
            missao.x,
            missao.y,
            missao.w,
            missao.h,
            Color::new(cor.r, cor.g, cor.b, if sobre { 0.16 } else { 0.08 }),
        );
        let c = vec2(missao.x + 16.0 * fm, missao.center().y);
        draw_poly(c.x, c.y, 4, 6.0 * fm, 0.0, cor);
        let feito = if i.andares == 0 {
            0.0
        } else {
            (i.andar.min(i.andares) as f32 + if i.concluida { 1.0 } else { 0.0 })
                / (i.andares + 1) as f32
        };
        let rotulo = if self.auto { "› AUTO" } else { "Toque: AUTO" };
        let rw = estilo::medir(rotulo, 13) + 16.0 * fm;
        estilo::texto_ajustado(
            &format!("Completar {}", nome_do_conteudo(i.conteudo)),
            missao.x + 30.0 * fm,
            missao.y + missao.h * 0.5 + 5.0 * fm,
            missao.w - rw - 44.0 * fm,
            14,
            estilo::TEXTO,
        );
        estilo::texto(
            missao.x + missao.w - rw,
            missao.y + missao.h * 0.5 + 5.0 * fm,
            rotulo,
            13,
            cor,
        );
        draw_rectangle(
            missao.x + 2.0,
            missao.y + missao.h - 3.0 * fm,
            (missao.w - 4.0) * feito,
            2.0 * fm,
            cor,
        );
        if sobre && crate::foco::clique() {
            self.auto = !self.auto;
        }
        if botao(porta, "Sair", true, false) {
            saida.push(pedir(Pedido::Sair));
        }
        if let Some(espera) = i.reviver_em_s {
            let falta = (espera as f64 - (agora - i.recebido)).max(0.0);
            let caixa = Rect::new(
                reviver.x - 40.0 * f,
                reviver.y - 90.0 * f,
                reviver.w + 80.0 * f,
                160.0 * f,
            );
            estilo::painel_destaque(caixa, estilo::VERMELHO);
            estilo::texto_centro_forte(
                caixa.center().x,
                caixa.y + 34.0 * f,
                "VOCÊ CAIU",
                22,
                estilo::VERMELHO,
            );
            estilo::texto_centro(
                caixa.center().x,
                caixa.y + 60.0 * f,
                "Sem perda de experiência dentro da dungeon.",
                13,
                estilo::SUAVE,
            );
            let pronto = falta <= 0.0;
            let rot = if pronto {
                "Reviver".to_string()
            } else {
                format!("Reviver em {:.0} s", falta.ceil())
            };
            if botao(reviver, &rot, pronto, true) {
                saida.push(pedir(Pedido::Reviver));
            }
        }
    }

    fn desenha_resultado(&mut self, c: &Contexto, saida: &mut Vec<ClientMessage>) {
        let Some(r) = &mut self.resultado else { return };
        if r.fechado {
            return;
        }
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let linhas = r.bau.as_ref().map_or(0, |b| b.0.len() + 1);
        let (w, h) = (
            (460.0 * f).min(seguro.w - 16.0),
            (196.0 * f + 20.0 * f * linhas as f32).min(seguro.h - 16.0),
        );
        let caixa = Rect::new(
            seguro.x + seguro.w - w - 16.0 * f,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        estilo::painel_destaque(
            caixa,
            if r.vitoria {
                estilo::OURO
            } else {
                estilo::VERMELHO
            },
        );
        let titulo = if r.vitoria {
            "VITÓRIA"
        } else {
            "TEMPO ESGOTADO"
        };
        estilo::texto_centro_forte(
            caixa.center().x,
            caixa.y + 32.0 * f,
            titulo,
            22,
            if r.vitoria {
                estilo::OURO
            } else {
                estilo::VERMELHO
            },
        );
        let mut y = caixa.y + 56.0 * f;
        estilo::texto_centro(
            caixa.center().x,
            y,
            &format!(
                "{} · estágio {} · {}",
                nome_do_conteudo(r.conteudo),
                r.estagio,
                mmss(r.tempo_s)
            ),
            14,
            estilo::TEXTO,
        );
        y += 20.0 * f;
        if r.bonus {
            estilo::texto_centro(
                caixa.center().x,
                y,
                "Bônus de tempo: +50% de Marcas",
                13,
                estilo::AUTO,
            );
            y += 18.0 * f;
        }
        if r.primeira {
            estilo::texto_centro(
                caixa.center().x,
                y,
                "Primeira vitória! Recompensa nas Entregas.",
                13,
                estilo::AUTO,
            );
            y += 18.0 * f;
        }
        match &r.bau {
            Some((itens, marcas, no_correio)) => {
                for (id, q) in itens {
                    let nome = c
                        .nomes
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| format!("Item {id}"));
                    estilo::texto(
                        caixa.x + 24.0 * f,
                        y + 4.0 * f,
                        &format!("• {nome} ×{q}"),
                        13,
                        estilo::TEXTO,
                    );
                    y += 20.0 * f;
                }
                let extra = if *no_correio > 0 {
                    format!(" · {no_correio} nas Entregas (bolsa cheia)")
                } else {
                    String::new()
                };
                estilo::texto(
                    caixa.x + 24.0 * f,
                    y + 4.0 * f,
                    &format!("+{marcas} Marcas da Tempestade{extra}"),
                    13,
                    estilo::OURO,
                );
            }
            None if r.vitoria => estilo::texto_centro(
                caixa.center().x,
                y + 6.0 * f,
                "Toque no baú para abrir (abre sozinho em 1 min).",
                13,
                estilo::SUAVE,
            ),
            None => estilo::texto_centro(
                caixa.center().x,
                y + 6.0 * f,
                "Sem baú de conclusão. A entrada ficou gasta.",
                13,
                estilo::SUAVE,
            ),
        }
        // Vitoria: tempo pra juntar o saque do chao antes de a instancia
        // fechar; sair antes traz o que sobrou (sozinho) — nada se perde.
        if r.chegou == 0.0 {
            r.chegou = get_time();
        }
        if r.vitoria {
            let resta = (shared::dungeon::FECHA_DEPOIS_DE_VENCER_S as f64
                - (get_time() - r.chegou))
                .max(0.0) as u32;
            estilo::texto_centro(
                caixa.center().x,
                caixa.y + caixa.h - 64.0 * f,
                &format!("Junte o saque · a dungeon fecha em {}", mmss(resta)),
                13,
                estilo::AUTO,
            );
        }
        let bw = (caixa.w - 60.0 * f) * 0.5;
        let by = caixa.y + caixa.h - 54.0 * f;
        // "Juntar saque" (fecha o painel) vem primeiro: e' o que se quer.
        if botao(
            Rect::new(caixa.x + 20.0 * f, by, bw, 40.0 * f),
            if r.vitoria { "Juntar saque" } else { "Fechar" },
            true,
            false,
        ) {
            r.fechado = true;
        }
        if botao(
            Rect::new(caixa.x + caixa.w - 20.0 * f - bw, by, bw, 40.0 * f),
            "Sair",
            true,
            false,
        ) {
            saida.push(pedir(Pedido::Sair));
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn abrir_pede_o_estado() {
        let mut d = DungeonUi::default();
        assert!(matches!(
            d.abrir()[..],
            [ClientMessage::Dungeon {
                pedido: Pedido::Estado
            }]
        ));
        assert!(d.aberto);
    }

    #[test]
    fn entrar_na_instancia_fecha_janela_e_pronto_e_sair_pede_estado() {
        let mut d = DungeonUi::default();
        d.abrir();
        d.aviso(
            Aviso::Pronto {
                partida: 1,
                conteudo: 10,
                estagio: 1,
                membros: vec![],
                expira_s: 20,
            },
            0.0,
        );
        assert!(d.pronto.is_some());
        d.aviso(
            Aviso::Instancia {
                conteudo: 10,
                estagio: 1,
                andar: 0,
                andares: 3,
                restante_s: 1500,
                inimigos: 7,
                reviver_em_s: None,
                membros: vec![],
                concluida: false,
            },
            1.0,
        );
        assert!(d.na_instancia() && !d.aberto && d.pronto.is_none());
        let volta = d.aviso(Aviso::Saiu, 2.0);
        assert!(!d.na_instancia());
        assert!(matches!(
            volta[..],
            [ClientMessage::Dungeon {
                pedido: Pedido::Estado
            }]
        ));
    }

    #[test]
    fn estado_escolhe_um_conteudo_aberto() {
        let mut d = DungeonUi::default();
        let conteudos = dg::CONTEUDOS
            .iter()
            .map(|c| dg::ConteudoEstado {
                id: c.id,
                liberado: 0,
                cadeados: (1..=dg::estagios(c))
                    .map(|_| {
                        if c.id == 1 {
                            Some(dg::Cadeado::Nivel(6))
                        } else {
                            None
                        }
                    })
                    .collect(),
                vitorias: 0,
            })
            .collect();
        d.aviso(
            Aviso::Estado {
                conteudos,
                entradas: Default::default(),
                fila: None,
                sala: None,
            },
            0.0,
        );
        assert_eq!(d.sel, 2, "o primeiro sem cadeado");
    }

    #[test]
    fn bau_entra_no_resultado() {
        let mut d = DungeonUi::default();
        d.aviso(
            Aviso::Resultado {
                conteudo: 10,
                estagio: 1,
                vitoria: true,
                tempo_s: 300,
                bonus_tempo: true,
                primeira_vitoria: false,
            },
            0.0,
        );
        d.aviso(
            Aviso::Bau {
                itens: vec![(344, 500)],
                marcas: 15,
                no_correio: 0,
            },
            1.0,
        );
        assert!(d
            .resultado
            .as_ref()
            .is_some_and(|r| r.bau.as_ref().is_some_and(|b| b.1 == 15)));
    }

    #[test]
    fn mmss_formata() {
        assert_eq!(mmss(125), "02:05");
    }
    /// O RELATÓRIO DA DUNGEON FICA NA COLUNA DA ESQUERDA.
    ///
    /// O dono: "na dungeon os status de andar etc, o relatório dela no geral,
    /// tem que ficar na lateral, no lugar onde ficam as infos da Ilha Mágica;
    /// lá em cima é bem ruim".
    ///
    /// O teste mede as três coisas que o lugar novo tem que respeitar: ficar
    /// colado na linha de missão, não descer em cima dos avisos, e não
    /// encostar em nada do HUD. A versão antiga ficava centrada no alto —
    /// mover sem medir trocaria um estorvo por outro.
    #[test]
    fn o_relatorio_da_dungeon_fica_na_coluna_da_esquerda() {
        for (sw, sh) in [(1920.0f32, 1080.0f32), (2400.0, 1080.0), (1280.0, 800.0)] {
            let z = crate::hud_layout::zonas(sw, sh);
            let seguro = Rect::new(0.0, 0.0, sw, sh);
            let missao = DungeonUi::missao_rect_em(&z);
            let (faixa, porta, _) = DungeonUi::rects_da_instancia_em(&z, seguro, 1.0);

            assert!(
                (faixa.x - missao.x).abs() < 0.01,
                "{sw}×{sh}: a faixa não está alinhada com a linha de missão"
            );
            assert!(
                faixa.y >= missao.y + missao.h - 0.01,
                "{sw}×{sh}: a faixa subiu em cima da linha de missão"
            );
            assert!(
                faixa.x + faixa.w <= z.rastreador.x + z.rastreador.w + 0.01,
                "{sw}×{sh}: a faixa passa da coluna da esquerda"
            );
            // Nem ela nem o Sair podem cair em cima dos avisos.
            assert!(
                porta.y + porta.h <= z.avisos.y + 0.01,
                "{sw}×{sh}: o Sair {porta:?} desce em cima dos avisos {:?}",
                z.avisos
            );
            assert!(
                porta.x >= faixa.x - 0.01 && porta.x + porta.w <= faixa.x + faixa.w + 0.01,
                "{sw}×{sh}: o Sair saiu da largura da faixa"
            );
            assert!(
                crate::ui::area_de_toque(porta).h >= 44.0,
                "{sw}×{sh}: o Sair é menor que um dedo"
            );
        }
    }

}
