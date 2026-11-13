//! Grupo, amigos, cartas e cla: o cliente pede, o servidor valida.
#[allow(unused_imports)]
use crate::ui::Toque as _;
use crate::hud_estilo as e;
use macroquad::prelude::*;
use shared::{
    protocol::ClientMessage,
    social::{self, Aviso, Estado, Pedido},
};

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Aba {
    #[default]
    Grupo,
    Amigos,
    Correio,
    Cla,
}

#[derive(Default)]
pub struct Social {
    pub aberto: bool,
    pub estado: Estado,
    pub grupo: Vec<String>,
    /// Clan tag of each `grupo` member, same order.
    pub grupo_tags: Vec<String>,
    pub convite: Option<(String, f64)>,
    /// Recompensas da dungeon, guardadas no correio do personagem.
    pub correio_dungeon: Vec<shared::dungeon::CartaNet>,
    recompensas: bool,
    pub nomes: std::collections::HashMap<u16, String>,
    oficial: bool,
    admin: bool,
    todos: bool,
    anexos: Vec<social::Anexo>,
    picker: bool,
    busca_item: String,
    quantidade: String,
    pagina_item: usize,
    envio_id: String,
    aba: Aba,
    nome: String,
    assunto: String,
    texto: String,
    nome_cla: String,
    aviso_cla: String,
    /// The clan tag being typed (create / change).
    tag_cla: String,
    /// Clan tab: the member list or the management page.
    cla_gerir: bool,
    /// The member whose actions are showing.
    cla_sel: Option<String>,
    foco: Option<u8>,
    pagina: usize,
    carta: Option<i64>,
    carta_pagina: usize,
    escrevendo: bool,
    /// (text, requests). It is a LIST because "Take all" confirms several at
    /// once — a single request is a list of one.
    confirmacao: Option<(String, Vec<Pedido>)>,
    aviso: String,
    ok: bool,
    carregado: bool,
    atualizar_em: f64,
    correio_atualizar_em: f64,
    ocupado_ate: f64,
    /// Pedidos em fila do "Pegar tudo e apagar".
    ///
    /// The server claims ONE letter at a time (`correio_em_voo`), so the button
    /// cannot fire everything at once: the queue drains in `passo`, one request
    /// per answer, taking advantage of `ocupado_ate` already zeroing when the
    /// server replies. Claiming comes before deleting — deleting a letter with an
    /// unclaimed attachment is throwing the item away.
    fila: Vec<Pedido>,
}
fn msg(pedido: Pedido) -> ClientMessage {
    ClientMessage::Social { pedido }
}

impl Social {
    /// Keeps the Mail dot up to date even before the window is opened.
    pub fn verificar_correio_dungeon(&mut self, agora: f64) -> Option<ClientMessage> {
        if agora < self.correio_atualizar_em {
            return None;
        }
        self.correio_atualizar_em = agora + 20.0;
        Some(ClientMessage::Dungeon {
            pedido: shared::dungeon::Pedido::Correio,
        })
    }

    /// A letter not yet read, or a reward awaiting claim.
    pub fn correio_pendente(&self) -> bool {
        !self.correio_dungeon.is_empty()
            || self
                .estado
                .cartas
                .iter()
                .chain(&self.estado.oficiais)
                .any(|c| !c.lida || (!c.resgatada && !c.anexos.is_empty()))
    }

    pub fn abrir(&mut self, aba: Aba) -> Vec<ClientMessage> {
        self.aberto = true;
        self.aba = aba;
        if aba == Aba::Correio && self.estado.oficiais.iter().any(|c| !c.lida) {
            self.oficial = true;
        }
        self.pagina = 0;
        self.foco = None;
        self.confirmacao = None;
        self.atualizar_em = get_time() + 5.0;
        let mut pedidos = vec![msg(Pedido::Estado)];
        if aba == Aba::Correio {
            self.correio_atualizar_em = get_time() + 20.0;
            pedidos.push(ClientMessage::Dungeon {
                pedido: shared::dungeon::Pedido::Correio,
            });
        }
        pedidos
    }
    pub fn fechar(&mut self) {
        self.aberto = false;
        self.foco = None;
        self.confirmacao = None;
    }
    pub fn foco(&self) -> bool {
        self.aberto && self.foco.is_some() && self.confirmacao.is_none()
    }
    pub fn receber(&mut self, aviso: Aviso) {
        match aviso {
            Aviso::Estado(estado) => {
                if self.foco != Some(4) {
                    self.aviso_cla = estado
                        .cla
                        .as_ref()
                        .map(|c| c.aviso.clone())
                        .unwrap_or_default();
                }
                self.estado = estado;
                self.carregado = true;
            }
            Aviso::Resultado { ok, texto } => {
                if ok && (texto == "Letter sent." || texto.starts_with("Official mail sent"))
                {
                    self.texto.clear();
                    self.assunto.clear();
                    self.escrevendo = false;
                    self.anexos.clear();
                    self.envio_id.clear();
                    self.admin = false;
                }
                self.aviso = texto;
                self.ok = ok;
                self.ocupado_ate = 0.0;
            }
        }
    }
    pub fn passo(&mut self, agora: f64) -> Option<ClientMessage> {
        if self.convite.as_ref().is_some_and(|(_, ate)| agora >= *ate) {
            self.convite = None;
        }
        // The "Take all" queue drains here: one request per answer.
        if agora >= self.ocupado_ate && !self.fila.is_empty() {
            let p = self.fila.remove(0);
            self.ocupado_ate = agora + 8.0;
            self.atualizar_em = agora + 5.0;
            self.aviso = format!("Emptying the inbox… {} left", self.fila.len() + 1);
            self.ok = true;
            return Some(msg(p));
        }
        // Updates notifications even with the window closed, without interrupting typing.
        if agora >= self.atualizar_em && agora >= self.ocupado_ate {
            self.atualizar_em = agora + if self.aberto { 5.0 } else { 20.0 };
            return Some(msg(Pedido::Estado));
        }
        None
    }
    pub fn desenha(
        &mut self,
        eu: &str,
        digitado: &[char],
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) -> Vec<ClientMessage> {
        let mut saida = Vec::new();
        if !self.aberto {
            return saida;
        }
        crate::hud_layout::escurece(0.65);
        let t = crate::hud_layout::tela_segura();
        let w = (t.w - 32.0).min(1220.0);
        let h = (t.h - 32.0).min(770.0);
        let p = Rect::new(t.center().x - w / 2.0, t.center().y - h / 2.0, w, h);
        e::painel_destaque(p, e::OURO);
        e::texto_forte(p.x + 20.0, p.y + 34.0, "SOCIAL", 24, e::OURO);
        if crate::ui::botao(
            Rect::new(p.x + p.w - 52.0, p.y + 12.0, 36.0, 32.0),
            "x",
            true,
        ) {
            self.fechar();
            return saida;
        }
        // Editor above the on-screen keyboard, including for long letters.
        // Returns to the form on finishing, without sending anything implicitly.
        if crate::nativo::TECLADO_NA_TELA && self.foco.is_some() {
            self.editor_mobile(t, digitado);
            return saida;
        }
        if self.confirmacao.is_some() {
            self.desenha_confirmacao(p, &mut saida);
            return saida;
        }
        let abas = [
            (Aba::Grupo, "Party"),
            (Aba::Amigos, "Friends"),
            (Aba::Correio, "Mail"),
            (Aba::Cla, "Clan"),
        ];
        let tw = (p.w - 40.0) / 4.0;
        for (i, (aba, nome)) in abas.iter().enumerate() {
            let r = Rect::new(p.x + 20.0 + i as f32 * tw, p.y + 50.0, tw - 5.0, 40.0);
            e::aba(
                r,
                nome,
                self.aba == *aba,
                r.toca(Vec2::from(mouse_position())),
            );
            let alerta = match aba {
                Aba::Grupo => self.convite.is_some(),
                Aba::Amigos => !self.estado.recebidos.is_empty(),
                Aba::Correio => self.correio_pendente(),
                Aba::Cla => !self.estado.convites_cla.is_empty(),
            };
            if alerta {
                e::badge(r);
            }
            if r.toca(Vec2::from(mouse_position())) && crate::foco::clique() {
                self.aba = *aba;
                self.pagina = 0;
                self.foco = None;
            }
        }
        let a = Rect::new(p.x + 20.0, p.y + 112.0, p.w - 40.0, p.h - 180.0);
        match self.aba {
            Aba::Grupo => self.grupo_ui(a, digitado, &mut saida),
            Aba::Amigos => self.amigos_ui(a, digitado, &mut saida),
            Aba::Correio => self.correio_ui(a, digitado, &mut saida, vox, solido),
            Aba::Cla => self.cla_ui(a, eu, digitado, &mut saida),
        }
        let aviso = if !self.carregado && self.aviso.is_empty() {
            "Loading…"
        } else {
            &self.aviso
        };
        e::texto_ajustado(
            aviso,
            p.x + 20.0,
            p.y + p.h - 22.0,
            p.w - 40.0,
            14,
            if self.ok { e::VERDE } else { e::OURO },
        );
        saida
    }
    fn editor_mobile(&mut self, tela: Rect, digitado: &[char]) {
        let id = self.foco.unwrap();
        let x = tela.x + 24.0;
        let y = tela.y + 24.0;
        let largura = tela.w - 48.0;
        let limite = tela.y + tela.h * 0.46;
        let painel = Rect::new(x, y, largura, (limite - y).max(110.0));
        e::painel_destaque(painel, e::OURO);
        if crate::ui::botao(
            Rect::new(x + largura - 140.0, y + 10.0, 125.0, 38.0),
            "Complete",
            true,
        ) {
            self.foco = None;
            return;
        }
        let (label, valor, max) = match id {
            1 => ("Subject", &mut self.assunto, 60),
            2 => ("Message", &mut self.texto, 1000),
            3 => ("Clan name", &mut self.nome_cla, 24),
            4 => ("Clan notice", &mut self.aviso_cla, 200),
            7 => ("Clan tag", &mut self.tag_cla, shared::social::TAG_MAX),
            5 => ("Search item", &mut self.busca_item, 60),
            6 => ("Quantity", &mut self.quantidade, 6),
            _ => ("Character name", &mut self.nome, 32),
        };
        campo(
            Rect::new(
                x + 14.0,
                y + 72.0,
                largura - 28.0,
                (painel.h - 85.0).max(30.0),
            ),
            label,
            valor,
            id,
            &mut self.foco,
            digitado,
            max,
        );
    }

    fn enviar(&mut self, p: Pedido, saida: &mut Vec<ClientMessage>) {
        if get_time() < self.ocupado_ate {
            return;
        }
        self.ocupado_ate = get_time() + 8.0;
        self.atualizar_em = get_time() + 5.0;
        self.aviso = "Waiting for the server…".into();
        self.ok = true;
        self.foco = None;
        saida.push(msg(p));
    }
    fn confirmar(&mut self, texto: String, p: Pedido) {
        self.confirmacao = Some((texto, vec![p]));
        self.foco = None;
    }
    fn desenha_confirmacao(&mut self, p: Rect, saida: &mut Vec<ClientMessage>) {
        let (texto, pedidos) = self.confirmacao.clone().unwrap();
        texto_em_linhas(
            &texto,
            Rect::new(
                p.x + 30.0,
                p.y + 100.0,
                p.w - 60.0,
                (p.h - 220.0).max(100.0),
            ),
            20,
            e::TEXTO,
            0,
        );
        if crate::ui::botao(
            Rect::new(p.center().x - 170.0, p.y + p.h - 85.0, 160.0, 42.0),
            "Cancel",
            true,
        ) {
            self.confirmacao = None;
        }
        if crate::ui::botao(
            Rect::new(p.center().x + 10.0, p.y + p.h - 85.0, 160.0, 42.0),
            "Confirm",
            get_time() >= self.ocupado_ate,
        ) {
            self.confirmacao = None;
            // One request goes straight through; several enter the queue, which drains
            // in `passo` as the server answers.
            let mut it = pedidos.into_iter();
            if let Some(primeiro) = it.next() {
                self.enviar(primeiro, saida);
            }
            self.fila.extend(it);
        }
    }
    fn grupo_ui(&mut self, a: Rect, d: &[char], s: &mut Vec<ClientMessage>) {
        e::texto(a.x, a.y + 14.0, "Up to 5 players on the same channel. Invites last 60 seconds.", 15, e::SUAVE);
        e::texto(a.x, a.y + 38.0, "XP is split evenly between members in the same area, within 25 m of the kill.", 14, e::TEXTO);
        e::texto(a.x, a.y + 61.0, "2 nearby: +10% (55% each) · 3: +20% (40% each)", 14, e::OURO);
        e::texto(a.x, a.y + 84.0, "4 nearby: +30% (32.5% each) · 5: +30% (26% each)", 14, e::OURO);
        e::texto(a.x, a.y + 107.0, "The bonus grows up to 4 players. Solo: 100%. XP fractions are dropped.", 13, e::SUAVE);
        campo(
            Rect::new(a.x, a.y + 148.0, a.w - 180.0, 40.0),
            "Character name",
            &mut self.nome,
            0,
            &mut self.foco,
            d,
            32,
        );
        if crate::ui::botao(
            Rect::new(a.x + a.w - 168.0, a.y + 148.0, 168.0, 40.0),
            "Invite",
            !self.nome.trim().is_empty(),
        ) {
            s.push(ClientMessage::PartyInvite {
                target_name: self.nome.trim().into(),
            });
            self.foco = None;
        }
        let mut y = a.y + 216.0;
        if let Some((nome, _)) = self.convite.clone() {
            e::texto_ajustado(
                &format!("{nome} invited you"),
                a.x,
                y,
                a.w - 235.0,
                18,
                e::OURO,
            );
            if crate::ui::botao(
                Rect::new(a.x + a.w - 226.0, y - 24.0, 108.0, 36.0),
                "Accept",
                true,
            ) {
                s.push(ClientMessage::PartyAccept);
                self.convite = None;
            }
            if crate::ui::botao(
                Rect::new(a.x + a.w - 108.0, y - 24.0, 108.0, 36.0),
                "Decline",
                true,
            ) {
                s.push(ClientMessage::PartyDecline);
                self.convite = None;
            }
            y += 56.0;
        }
        if self.grupo.is_empty() {
            e::texto(a.x, y, "You are playing solo.", 18, e::TEXTO);
        }
        for (i, nome) in self.grupo.iter().enumerate() {
            let tag = self.grupo_tags.get(i).map_or("", String::as_str);
            e::texto(a.x + 12.0, y, &social::com_tag(tag, nome), 18, e::TEXTO);
            y += 30.0;
        }
        if !self.grupo.is_empty()
            && crate::ui::botao(
                Rect::new(a.x, a.y + a.h - 36.0, 160.0, 36.0),
                "Leave party",
                true,
            )
        {
            s.push(ClientMessage::PartyLeave);
        }
    }
    fn amigos_ui(&mut self, a: Rect, d: &[char], s: &mut Vec<ClientMessage>) {
        campo(
            Rect::new(a.x, a.y + 20.0, a.w - 190.0, 40.0),
            "Character name",
            &mut self.nome,
            0,
            &mut self.foco,
            d,
            32,
        );
        if crate::ui::botao(
            Rect::new(a.x + a.w - 178.0, a.y + 20.0, 178.0, 40.0),
            "Add friend",
            !self.nome.trim().is_empty(),
        ) {
            self.enviar(
                Pedido::Amizade {
                    nome: self.nome.trim().into(),
                },
                s,
            );
        }
        e::texto(
            a.x,
            a.y + 91.0,
            &format!(
                "{} / {} friends · {} online",
                self.estado.amigos.len(),
                social::MAX_AMIGOS,
                self.estado.amigos.iter().filter(|n| self.estado.neste_canal.contains(n)).count()
            ),
            14,
            e::SUAVE,
        );
        let rows: Vec<(String, u8)> = self
            .estado
            .recebidos
            .iter()
            .map(|n| (n.clone(), 0))
            .chain(self.estado.amigos.iter().map(|n| (n.clone(), 1)))
            .chain(self.estado.enviados.iter().map(|n| (n.clone(), 2)))
            .collect();
        let area = Rect::new(a.x, a.y + 108.0, a.w, a.h - 108.0);
        let range = paginacao(area, rows.len(), &mut self.pagina);
        if rows.is_empty() {
            e::texto(
                a.x,
                area.y + 25.0,
                "Add friends to find them here.",
                18,
                e::SUAVE,
            );
        }
        let agora = agora_unix();
        for (i, (nome, tipo)) in rows[range].iter().enumerate() {
            let y = area.y + i as f32 * 50.0;
            let contato = self.estado.contatos.iter().find(|c| &c.nome == nome);
            let status = match tipo {
                0 => "wants to be your friend".to_string(),
                2 => "request sent".to_string(),
                _ => match contato {
                    Some(c) => onde_esta(c, agora),
                    None if self.estado.neste_canal.contains(nome) => "Online".to_string(),
                    None => "Offline".to_string(),
                },
            };
            let rotulo = contato.map_or_else(|| nome.clone(), |c| social::com_tag(&c.tag, nome));
            if *tipo == 1 {
                let on = contato.is_some_and(|c| c.online) || self.estado.neste_canal.contains(nome);
                draw_circle(a.x + 6.0, y + 18.0, 5.0, if on { e::VERDE } else { e::alfa(e::SUAVE, 0.5) });
            }
            e::texto_ajustado(
                &format!("{rotulo} · {status}"),
                a.x + if *tipo == 1 { 18.0 } else { 0.0 },
                y + 24.0,
                a.w - 345.0,
                16,
                e::TEXTO,
            );
            if *tipo == 0 {
                if botao_fim(a, y, 2, "Accept") {
                    self.enviar(
                        Pedido::ResponderAmizade {
                            nome: nome.clone(),
                            aceitar: true,
                        },
                        s,
                    );
                }
                if botao_fim(a, y, 1, "Decline") {
                    self.enviar(
                        Pedido::ResponderAmizade {
                            nome: nome.clone(),
                            aceitar: false,
                        },
                        s,
                    );
                }
            } else {
                if *tipo == 1 && botao_fim(a, y, 3, "Party") {
                    s.push(ClientMessage::PartyInvite {
                        target_name: nome.clone(),
                    });
                }
                if *tipo == 1 && botao_fim(a, y, 2, "Letter") {
                    self.nome = nome.clone();
                    self.aba = Aba::Correio;
                    self.escrevendo = true;
                    self.foco = None;
                }
                if botao_fim(a, y, 1, if *tipo == 2 { "Cancel" } else { "Remove" }) {
                    self.confirmar(
                        format!("Remove friendship or request with {nome}?"),
                        Pedido::RemoverAmigo { nome: nome.clone() },
                    );
                }
            }
        }
    }
    fn correio_ui(
        &mut self,
        a: Rect,
        d: &[char],
        s: &mut Vec<ClientMessage>,
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) {
        if crate::ui::botao(Rect::new(a.x, a.y, 150.0, 38.0), "Write a letter", true) {
            self.escrevendo = true;
            self.carta = None;
            self.admin = false;
            self.foco = None;
        }
        if crate::ui::botao(
            Rect::new(a.x + 160.0, a.y, 210.0, 38.0),
            &format!("Rewards ({})", self.correio_dungeon.len()),
            true,
        ) {
            self.recompensas = !self.recompensas;
            self.pagina = 0;
        }
        if self.recompensas {
            self.recompensas_da_dungeon(a, s, vox, solido);
            return;
        }
        if self.estado.cargo.is_some()
            && crate::ui::botao(
                Rect::new(a.x + 382.0, a.y, 180.0, 38.0),
                "Admin/mod send",
                true,
            )
        {
            self.admin = true;
            self.escrevendo = true;
            self.picker = false;
            self.todos = false;
            self.anexos.clear();
            self.assunto.clear();
            self.texto.clear();
            self.foco = None;
            self.envio_id = format!(
                "carta-{}-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |t| t.as_nanos()),
                rand::gen_range(1u32, u32::MAX)
            );
        }
        if self.picker && self.admin {
            self.picker_ui(a, d);
            return;
        }
        if self.escrevendo {
            if self.admin && self.estado.cargo.is_none() {
                self.admin = false;
                self.escrevendo = false;
                return;
            }
            if self.admin {
                if crate::ui::botao(
                    Rect::new(a.x + a.w - 360.0, a.y, 240.0, 38.0),
                    if self.todos {
                        "All characters"
                    } else {
                        "One character"
                    },
                    true,
                ) {
                    self.todos = !self.todos;
                    self.foco = None;
                }
            }
            if crate::ui::botao(
                Rect::new(a.x + a.w - 110.0, a.y, 110.0, 38.0),
                "Inbox",
                true,
            ) {
                self.escrevendo = false;
                self.foco = None;
            }
            if self.admin && self.todos {
                e::texto_ajustado(
                    &format!("All: {} characters", self.estado.destinatarios),
                    a.x,
                    a.y + 102.0,
                    a.w * 0.35,
                    18,
                    e::OURO,
                );
            } else {
                campo(
                    Rect::new(a.x, a.y + 76.0, a.w * 0.35, 40.0),
                    "To",
                    &mut self.nome,
                    0,
                    &mut self.foco,
                    d,
                    32,
                );
            }
            campo(
                Rect::new(a.x + a.w * 0.37, a.y + 76.0, a.w * 0.63, 40.0),
                "Subject",
                &mut self.assunto,
                1,
                &mut self.foco,
                d,
                60,
            );
            campo(
                Rect::new(a.x, a.y + 155.0, a.w, a.h - 208.0),
                "Message (up to 1000 characters)",
                &mut self.texto,
                2,
                &mut self.foco,
                d,
                1000,
            );
            if crate::ui::botao(
                Rect::new(a.x + a.w - 140.0, a.y + a.h - 40.0, 140.0, 40.0),
                "Send",
                (self.todos && self.admin || !self.nome.trim().is_empty())
                    && !self.assunto.trim().is_empty()
                    && !self.texto.trim().is_empty()
                    && get_time() >= self.ocupado_ate,
            ) {
                if self.admin {
                    let destino = if self.todos {
                        format!(
                            "TODOS os personagens existentes (aprox. {})",
                            self.estado.destinatarios
                        )
                    } else {
                        self.nome.trim().to_string()
                    };
                    let resumo = self
                        .anexos
                        .iter()
                        .map(|a| {
                            format!(
                                "{} × {}",
                                a.qtd,
                                self.nomes
                                    .get(&a.item_id)
                                    .cloned()
                                    .unwrap_or_else(|| a.item_id.to_string())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.confirmar(format!("Enviar correio oficial para {destino}? Assunto: {}. Anexos por destinatário: {}. O envio será registrado e não pode ser desfeito.",self.assunto,if resumo.is_empty(){"nenhum"}else{&resumo}),Pedido::EnviarOficial{envio:self.envio_id.clone(),para:if self.todos{None}else{Some(self.nome.trim().into())},assunto:self.assunto.clone(),texto:self.texto.clone(),anexos:self.anexos.clone()});
                } else {
                    self.enviar(
                        Pedido::EnviarCarta {
                            para: self.nome.trim().into(),
                            assunto: self.assunto.clone(),
                            texto: self.texto.clone(),
                        },
                        s,
                    );
                }
            }
            if self.admin
                && crate::ui::botao(
                    Rect::new(a.x, a.y + a.h - 40.0, 240.0, 40.0),
                    &format!("Attachments: {} / 8", self.anexos.len()),
                    true,
                )
            {
                self.picker = true;
                self.foco = None;
                self.quantidade = "1".into();
            }
            return;
        }
        if crate::ui::botao(
            Rect::new(a.x + a.w - 245.0, a.y, 245.0, 38.0),
            if self.oficial {
                "Official · switch inbox"
            } else {
                "Personal · switch inbox"
            },
            true,
        ) {
            self.oficial = !self.oficial;
            self.pagina = 0;
            self.carta = None;
        }
        let left = Rect::new(a.x, a.y + 58.0, a.w * 0.42, a.h - 58.0);
        let cartas = if self.oficial {
            self.estado.oficiais.clone()
        } else {
            self.estado.cartas.clone()
        };
        // ── pegar tudo e apagar ──
        //
        // Claiming letter by letter and deleting one by one was the only path, and
        // with half a dozen letters it becomes work. Claim BEFORE delete, in the
        // queue: the order is what guarantees no attachment goes in the bin.
        let a_resgatar: Vec<i64> = cartas
            .iter()
            .filter(|c| c.oficial && !c.anexos.is_empty() && !c.resgatada)
            .map(|c| c.id)
            .collect();
        let quantas = cartas.len();
        if crate::ui::botao(
            Rect::new(a.x + a.w - 500.0, a.y, 245.0, 38.0),
            &if a_resgatar.is_empty() {
                format!("Delete everything ({quantas})")
            } else {
                format!("Take everything and delete ({quantas})")
            },
            quantas > 0 && self.fila.is_empty(),
        ) {
            let mut fila: Vec<Pedido> = a_resgatar
                .iter()
                .map(|id| Pedido::ReceberAnexos { id: *id })
                .collect();
            fila.extend(cartas.iter().map(|c| Pedido::ApagarCarta { id: c.id }));
            self.confirmacao = Some((
                format!(
                    "Receber os itens de {} carta(s) e apagar as {quantas}? Não dá pra desfazer.",
                    a_resgatar.len()
                ),
                fila,
            ));
            self.foco = None;
        }
        let range = paginacao(left, cartas.len(), &mut self.pagina);
        if cartas.is_empty() {
            e::texto(left.x, left.y + 25.0, "Your inbox is empty.", 18, e::SUAVE);
        }
        for (i, c) in cartas[range].iter().enumerate() {
            let r = Rect::new(left.x, left.y + i as f32 * 50.0, left.w - 12.0, 44.0);
            e::cartao(r, false, self.carta == Some(c.id));
            e::texto_ajustado(
                &format!(
                    "{}{} · {}",
                    if c.lida { "" } else { "New: " },
                    c.de,
                    c.assunto
                ),
                r.x + 8.0,
                r.y + 27.0,
                r.w - 16.0,
                16,
                if c.lida { e::TEXTO } else { e::OURO },
            );
            if r.toca(Vec2::from(mouse_position())) && crate::foco::clique() {
                self.carta = Some(c.id);
                self.carta_pagina = 0;
                if !c.lida {
                    self.enviar(Pedido::LerCarta { id: c.id }, s);
                }
            }
        }
        let right = Rect::new(a.x + a.w * 0.45, a.y + 58.0, a.w * 0.55, a.h - 58.0);
        if let Some(c) = cartas.iter().find(|c| Some(c.id) == self.carta) {
            e::texto_ajustado(&c.assunto, right.x, right.y + 24.0, right.w, 20, e::OURO);
            e::texto(
                right.x,
                right.y + 52.0,
                &format!("From: {}", c.de),
                15,
                e::SUAVE,
            );
            let corpo = Rect::new(
                right.x,
                right.y + 70.0,
                right.w,
                (right.h - 162.0).max(22.0),
            );
            let por_pagina = (corpo.h / (16.0 * e::fator_texto() + 6.0)).floor().max(1.0) as usize;
            let anexos = c
                .anexos
                .iter()
                .map(|a| {
                    format!(
                        "{} × {}",
                        a.qtd,
                        self.nomes
                            .get(&a.item_id)
                            .cloned()
                            .unwrap_or_else(|| format!("Item {}", a.item_id))
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let conteudo = if anexos.is_empty() {
                c.texto.clone()
            } else {
                format!(
                    "{}  |  Attachments: {}. {}",
                    c.texto,
                    anexos,
                    if c.resgatada {
                        "Received."
                    } else {
                        "Waiting to be claimed."
                    }
                )
            };
            let total = texto_em_linhas(
                &conteudo,
                corpo,
                16,
                e::TEXTO,
                self.carta_pagina * por_pagina,
            );
            let paginas = total.div_ceil(por_pagina).max(1);
            self.carta_pagina = self.carta_pagina.min(paginas - 1);
            if paginas > 1 {
                let y = right.y + right.h - 80.0;
                if crate::ui::botao(
                    Rect::new(right.x, y, 60.0, 30.0),
                    "<",
                    self.carta_pagina > 0,
                ) {
                    self.carta_pagina -= 1;
                }
                e::texto(
                    right.x + 75.0,
                    y + 22.0,
                    &format!("Text {} / {}", self.carta_pagina + 1, paginas),
                    13,
                    e::SUAVE,
                );
                if crate::ui::botao(
                    Rect::new(right.x + 200.0, y, 60.0, 30.0),
                    ">",
                    self.carta_pagina + 1 < paginas,
                ) {
                    self.carta_pagina += 1;
                }
            }
            if crate::ui::botao(
                Rect::new(right.x, right.y + right.h - 40.0, 130.0, 40.0),
                if c.oficial {
                    "Claim items"
                } else {
                    "Reply"
                },
                !c.oficial
                    || (!c.anexos.is_empty() && !c.resgatada && get_time() >= self.ocupado_ate),
            ) {
                if c.oficial {
                    self.enviar(Pedido::ReceberAnexos { id: c.id }, s);
                } else {
                    self.nome = c.de.clone();
                    self.assunto = format!("Re: {}", c.assunto).chars().take(60).collect();
                    self.texto.clear();
                    self.escrevendo = true;
                    self.admin = false;
                }
            }
            if crate::ui::botao(
                Rect::new(right.x + 140.0, right.y + right.h - 40.0, 130.0, 40.0),
                "Delete",
                !c.oficial || c.anexos.is_empty() || c.resgatada,
            ) {
                self.confirmar(
                    "Delete this letter?".into(),
                    Pedido::ApagarCarta { id: c.id },
                );
            }
        } else {
            e::texto(
                right.x,
                right.y + 24.0,
                "Select a letter to read.",
                16,
                e::SUAVE,
            );
        }
    }

    fn recompensas_da_dungeon(
        &mut self,
        a: Rect,
        saida: &mut Vec<ClientMessage>,
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) {
        let lista = Rect::new(a.x, a.y + 58.0, a.w, a.h - 58.0);
        if self.correio_dungeon.is_empty() {
            e::texto(
                lista.x,
                lista.y + 28.0,
                "Nenhuma recompensa aguardando no Correio.",
                16,
                e::SUAVE,
            );
        }
        let range = paginacao(lista, self.correio_dungeon.len(), &mut self.pagina);
        for (n, carta) in self.correio_dungeon[range].iter().enumerate() {
            let r = Rect::new(lista.x, lista.y + n as f32 * 50.0, lista.w, 44.0);
            e::cartao(r, false, true);
            let icone = Rect::new(r.x + 4.0, r.y + 2.0, 40.0, 40.0);
            crate::icones::icone_com_3d(carta.item_id, icone, None, None, Some((vox, solido)));
            let nome = self
                .nomes
                .get(&carta.item_id)
                .map(String::as_str)
                .unwrap_or("Item");
            let origem = match carta.motivo {
                1 => "First win of the week",
                2 => "First win",
                shared::presenca::MOTIVO_CORREIO => "Attendance calendar",
                shared::loja::MOTIVO_CORREIO_LOJA => "Shop purchase",
                _ => "Bag full",
            };
            e::texto_ajustado(
                &format!("{nome} ×{} · {origem}", carta.qtd),
                r.x + 52.0,
                r.y + 28.0,
                r.w - 200.0,
                15,
                e::TEXTO,
            );
            if crate::ui::botao(
                Rect::new(r.x + r.w - 136.0, r.y + 3.0, 128.0, 38.0),
                "Receive",
                true,
            ) {
                saida.push(ClientMessage::Dungeon {
                    pedido: shared::dungeon::Pedido::CorreioRetirar { id: carta.id },
                });
            }
        }
    }
    fn picker_ui(&mut self, a: Rect, d: &[char]) {
        if crate::ui::botao(
            Rect::new(a.x + a.w - 160.0, a.y, 160.0, 38.0),
            "Back to the letter",
            true,
        ) {
            self.picker = false;
            self.foco = None;
            return;
        }
        campo(
            Rect::new(a.x, a.y + 80.0, a.w * 0.55, 40.0),
            "Search for an item by name or ID",
            &mut self.busca_item,
            5,
            &mut self.foco,
            d,
            60,
        );
        campo(
            Rect::new(a.x + a.w * 0.60, a.y + 80.0, 150.0, 40.0),
            "Quantity",
            &mut self.quantidade,
            6,
            &mut self.foco,
            d,
            6,
        );
        let qtd = self
            .quantidade
            .parse::<u32>()
            .ok()
            .filter(|q| *q > 0 && *q <= social::MAX_QTD_ANEXO);
        let busca = self.busca_item.to_lowercase();
        let mut itens: Vec<_> = self
            .nomes
            .iter()
            .filter(|(id, n)| n.to_lowercase().contains(&busca) || id.to_string() == busca)
            .map(|(id, n)| (*id, n.clone()))
            .collect();
        itens.sort_by(|a, b| a.1.cmp(&b.1));
        let area = Rect::new(a.x, a.y + 142.0, a.w * 0.56, a.h - 142.0);
        let range = paginacao(area, itens.len(), &mut self.pagina_item);
        for (i, (id, nome)) in itens[range].iter().enumerate() {
            let y = area.y + i as f32 * 50.0;
            e::texto_ajustado(
                &format!("#{id} · {nome}"),
                area.x,
                y + 25.0,
                area.w - 120.0,
                16,
                e::TEXTO,
            );
            if crate::ui::botao(
                Rect::new(area.x + area.w - 110.0, y, 104.0, 38.0),
                "Add",
                qtd.is_some()
                    && self.anexos.len() < social::MAX_ANEXOS
                    && !self.anexos.iter().any(|a| a.item_id == *id),
            ) {
                self.anexos.push(social::Anexo {
                    item_id: *id,
                    qtd: qtd.unwrap(),
                    instance: None,
                });
            }
        }
        let x = a.x + a.w * 0.60;
        e::texto(
            x,
            a.y + 150.0,
            &format!("Attachments: {} / 8", self.anexos.len()),
            16,
            e::OURO,
        );
        let anexos = self.anexos.clone();
        for (i, item) in anexos.iter().enumerate() {
            let y = a.y + 170.0 + i as f32 * 38.0;
            e::texto_ajustado(
                &format!(
                    "{} × {}",
                    item.qtd,
                    self.nomes.get(&item.item_id).cloned().unwrap_or_default()
                ),
                x,
                y + 23.0,
                a.w * 0.40 - 48.0,
                14,
                e::TEXTO,
            );
            if crate::ui::botao(Rect::new(a.x + a.w - 40.0, y, 36.0, 32.0), "x", true) {
                self.anexos.retain(|a| a.item_id != item.item_id);
            }
        }
    }

    /// The clan tab: a header with the emblem, name, [TAG] and head count; then
    /// either the member list (online first, rank, level, class, where they
    /// are or when they were last seen, actions on the selected row) or the
    /// management page (notice, invite, tag, leave / disband). Every action is
    /// shown only to the ranks that may do it (`social::pode_no_cla`); the
    /// server checks again.
    fn cla_ui(&mut self, a: Rect, eu: &str, d: &[char], s: &mut Vec<ClientMessage>) {
        let Some(c) = self.estado.cla.clone() else {
            self.sem_cla_ui(a, d, s);
            return;
        };
        // Rows grow with the interface scale, never below a finger.
        let k = e::fator_texto().clamp(1.0, 1.4);
        let meu = cargo_de(&c, eu);
        let agora = agora_unix();
        let mut membros = membros_do_cla(&c, &self.estado.contatos, &self.estado.neste_canal);
        social::ordenar_membros(&mut membros);
        let online = membros.iter().filter(|m| m.online).count();

        // ── header ──
        let cab = Rect::new(a.x, a.y, a.w, 62.0 * k);
        e::cartao(cab, false, false);
        emblema(vec2(cab.x + 31.0 * k, cab.center().y), 22.0 * k, &c.tag, &c.nome);
        let tx = cab.x + 62.0 * k;
        let seg_w = 230.0 * k;
        e::texto_ajustado(&c.nome, tx, cab.y + 26.0 * k, cab.w - seg_w - 80.0 * k, 20, e::OURO);
        let tag = if c.tag.is_empty() { "No tag".to_string() } else { format!("[{}]", c.tag) };
        e::texto_ajustado(
            &format!(
                "{tag} · {} / {} members · {online} online · You: {}",
                c.membros.len(),
                social::MAX_CLA,
                social::cargo::nome(meu)
            ),
            tx,
            cab.y + 49.0 * k,
            cab.w - seg_w - 80.0 * k,
            14,
            e::SUAVE,
        );
        // Members | Manage.
        let m = Vec2::from(mouse_position());
        for (i, (rotulo, gerir)) in [("Members", false), ("Manage", true)].into_iter().enumerate() {
            let r = Rect::new(
                cab.x + cab.w - seg_w - 10.0 * k + i as f32 * seg_w * 0.5,
                cab.y + 11.0 * k,
                seg_w * 0.5 - 4.0,
                40.0 * k,
            );
            e::aba(r, rotulo, self.cla_gerir == gerir, r.toca(m));
            if gerir && meu == social::cargo::LIDER && c.tag.is_empty() {
                e::badge(r);
            }
            if r.toca(m) && crate::foco::clique() {
                self.cla_gerir = gerir;
                self.foco = None;
            }
        }
        let corpo = Rect::new(a.x, cab.y + cab.h + 10.0 * k, a.w, a.h - cab.h - 10.0 * k);
        if self.cla_gerir {
            self.cla_gerir_ui(corpo, &c, meu, k, d, s);
        } else {
            self.cla_membros_ui(corpo, &c, eu, meu, &membros, agora, k, s);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cla_membros_ui(
        &mut self,
        a: Rect,
        c: &social::Cla,
        eu: &str,
        meu: u8,
        membros: &[social::Contato],
        agora: i64,
        k: f32,
        s: &mut Vec<ClientMessage>,
    ) {
        let mut topo = a.y;
        // The notice, one line, on top of the list (the whole text is in Manage).
        if !c.aviso.trim().is_empty() {
            let r = Rect::new(a.x, topo, a.w, 30.0 * k);
            e::ret_arredondado(r, e::RAIO_PEQUENO, e::alfa(e::OURO, 0.10));
            e::texto_ajustado(
                &format!("Notice: {}", c.aviso.replace('\n', " ")),
                r.x + 10.0,
                r.y + 21.0 * k,
                r.w - 20.0,
                14,
                e::ACENTO,
            );
            topo += r.h + 6.0 * k;
        }
        let rh = (48.0 * k).max(e::ALVO_DO_DEDO + 4.0);
        let lista = Rect::new(a.x, topo, a.w, a.y + a.h - topo);
        let range = paginacao_em(lista, membros.len(), &mut self.pagina, rh, k);
        let m = Vec2::from(mouse_position());
        let bw = 104.0 * k;
        for (i, mb) in membros[range].iter().enumerate() {
            let r = Rect::new(lista.x, lista.y + i as f32 * rh, lista.w, rh - 4.0);
            let sel = self.cla_sel.as_deref() == Some(mb.nome.as_str());
            e::cartao(r, r.toca(m), sel);
            let cy = r.center().y;
            draw_circle(r.x + 14.0 * k, cy, 5.0 * k, if mb.online { e::VERDE } else { e::alfa(e::SUAVE, 0.45) });
            // Name + rank.
            let x = r.x + 28.0 * k;
            let cor = if mb.nome == eu { e::VERDE } else if mb.online { e::TEXTO } else { e::SUAVE };
            let nome_w = (r.w * 0.30).max(120.0 * k);
            e::texto_ajustado(&mb.nome, x, cy - 3.0 * k, nome_w, 16, cor);
            if mb.cargo != social::cargo::MEMBRO {
                let cor_cargo = if mb.cargo == social::cargo::LIDER { e::OURO } else { e::AZUL };
                e::texto(x, cy + 15.0 * k, social::cargo::nome(mb.cargo), 12, cor_cargo);
            }
            // Level and class.
            let x2 = x + nome_w + 10.0 * k;
            let classe = shared::skills::Conjunto::de_u8(mb.conjunto).map_or("", |c| c.nome());
            let nivel = if mb.nivel > 0 { format!("Lv {}", mb.nivel) } else { "Lv ?".to_string() };
            e::texto(x2, cy - 3.0 * k, &nivel, 15, e::TEXTO);
            e::texto_ajustado(classe, x2, cy + 15.0 * k, 130.0 * k, 12, e::SUAVE);
            // Where / last seen.
            let x3 = x2 + 140.0 * k;
            let acoes = if sel { acoes_do_membro(meu, mb, eu) } else { Vec::new() };
            let fim = r.x + r.w - acoes.len() as f32 * (bw + 6.0) - 8.0;
            e::texto_ajustado(&onde_esta(mb, agora), x3, cy + 5.0 * k, (fim - x3 - 8.0).max(0.0), 14,
                if mb.online { e::TEXTO } else { e::SUAVE });
            // Actions on the selected row only: the list stays readable.
            let mut usou = false;
            for (j, acao) in acoes.iter().enumerate() {
                let b = Rect::new(fim + j as f32 * (bw + 6.0), r.y + (r.h - 38.0 * k.min(1.15)) * 0.5, bw, 38.0 * k.min(1.15));
                if crate::ui::botao(b, acao.rotulo(), true) {
                    usou = true;
                    self.acao_no_membro(*acao, &mb.nome, s);
                }
                if b.toca(m) {
                    usou = true;
                }
            }
            if !usou && r.toca(m) && crate::foco::clique() {
                self.cla_sel = if sel { None } else { Some(mb.nome.clone()) };
            }
        }
    }

    fn acao_no_membro(&mut self, acao: AcaoMembro, n: &str, s: &mut Vec<ClientMessage>) {
        let n = n.to_string();
        match acao {
            AcaoMembro::Grupo => s.push(ClientMessage::PartyInvite { target_name: n }),
            AcaoMembro::Promover => self.enviar(Pedido::CargoCla { nome: n, oficial: true }, s),
            AcaoMembro::Rebaixar => self.enviar(Pedido::CargoCla { nome: n, oficial: false }, s),
            AcaoMembro::Lider => self.confirmar(
                format!("Transfer leadership to {n}? You will become an officer and lose control of the clan."),
                Pedido::LiderCla { nome: n },
            ),
            AcaoMembro::Expulsar => {
                self.confirmar(format!("Kick {n} from the clan?"), Pedido::ExpulsarCla { nome: n })
            }
        }
    }

    fn cla_gerir_ui(&mut self, a: Rect, c: &social::Cla, meu: u8, k: f32, d: &[char], s: &mut Vec<ClientMessage>) {
        use social::AcaoCla as A;
        let gap = 14.0 * k;
        let col_w = (a.w - gap) * 0.5;
        let esq = Rect::new(a.x, a.y, col_w, a.h);
        let dir = Rect::new(a.x + col_w + gap, a.y, col_w, a.h);
        let fh = 40.0 * k.min(1.2);
        // ── left: the notice ──
        e::texto(esq.x, esq.y + 16.0 * k, "Clan notice", 16, e::OURO);
        let caixa = Rect::new(esq.x, esq.y + 30.0 * k, esq.w, (esq.h - 30.0 * k - fh - 12.0).max(fh));
        if social::pode_no_cla(meu, A::Aviso, 0) {
            campo(caixa, "", &mut self.aviso_cla, 4, &mut self.foco, d, 200);
            let mudou = self.aviso_cla.trim() != c.aviso.trim();
            if crate::ui::botao(
                Rect::new(esq.x + esq.w - 170.0 * k, caixa.y + caixa.h + 8.0, 170.0 * k, fh),
                "Save notice",
                mudou,
            ) {
                self.enviar(Pedido::AvisoCla { texto: self.aviso_cla.clone() }, s);
            }
            e::texto(
                esq.x,
                caixa.y + caixa.h + 8.0 + fh * 0.65,
                &format!("{} / 200", self.aviso_cla.chars().count()),
                13,
                e::SUAVE,
            );
        } else {
            e::cartao(caixa, false, false);
            let texto = if c.aviso.trim().is_empty() { "No notice yet." } else { c.aviso.as_str() };
            texto_em_linhas(
                texto,
                Rect::new(caixa.x + 10.0, caixa.y + 6.0, caixa.w - 20.0, caixa.h - 10.0),
                16,
                e::TEXTO,
                0,
            );
        }
        // ── right: invite, tag, leave / disband ──
        let mut y = dir.y + 30.0 * k;
        let bw = 140.0 * k;
        if social::pode_no_cla(meu, A::Convidar, 0) {
            campo(
                Rect::new(dir.x, y, dir.w - bw - 8.0, fh),
                "Invite a character",
                &mut self.nome,
                0,
                &mut self.foco,
                d,
                32,
            );
            if crate::ui::botao(Rect::new(dir.x + dir.w - bw, y, bw, fh), "Invite", !self.nome.trim().is_empty()) {
                self.enviar(Pedido::ConvidarCla { nome: self.nome.trim().into() }, s);
            }
            y += fh + 30.0 * k;
        }
        if social::pode_no_cla(meu, A::Tag, 0) {
            let rotulo = if c.tag.is_empty() {
                "Clan tag (2-5 letters or digits): none yet".to_string()
            } else {
                format!("Clan tag (2-5 letters or digits): now [{}]", c.tag)
            };
            campo(Rect::new(dir.x, y, dir.w - bw - 8.0, fh), &rotulo, &mut self.tag_cla, 7, &mut self.foco, d, social::TAG_MAX);
            let valida = social::tag_valida(&self.tag_cla);
            if crate::ui::botao(
                Rect::new(dir.x + dir.w - bw, y, bw, fh),
                "Save tag",
                valida.as_ref().is_ok_and(|t| *t != c.tag),
            ) {
                self.enviar(Pedido::TagCla { tag: self.tag_cla.trim().into() }, s);
            }
            if let (Err(motivo), false) = (&valida, self.tag_cla.trim().is_empty()) {
                e::texto_ajustado(motivo, dir.x, y + fh + 16.0 * k, dir.w, 12, e::VERMELHO);
            } else if let Ok(t) = &valida {
                e::texto_ajustado(&format!("Shown as: [{t}] Name"), dir.x, y + fh + 16.0 * k, dir.w, 12, e::SUAVE);
            }
            y += fh + 30.0 * k;
        } else if !c.tag.is_empty() {
            e::texto(dir.x, y + 4.0, &format!("Clan tag: [{}]", c.tag), 16, e::TEXTO);
            y += 30.0 * k;
        }
        // Ranks, for whoever wonders what an officer can do.
        texto_em_linhas(
            "Leader: everything. Officers: invite, kick members, edit the notice. Select a member in Members to see their actions.",
            Rect::new(dir.x, y, dir.w, (dir.y + dir.h - fh - 12.0 - y).max(0.0)),
            13,
            e::SUAVE,
            0,
        );
        let lider = meu == social::cargo::LIDER;
        if crate::ui::botao(
            Rect::new(dir.x + dir.w - 190.0 * k, dir.y + dir.h - fh, 190.0 * k, fh),
            if lider { "Disband clan" } else { "Leave clan" },
            true,
        ) {
            if lider {
                self.confirmar(
                    format!("Disband {} and remove all {} members? This cannot be undone.", c.nome, c.membros.len()),
                    Pedido::DissolverCla,
                );
            } else {
                self.confirmar(format!("Leave {}?", c.nome), Pedido::SairCla);
            }
        }
        if lider {
            e::texto_ajustado(
                "To leave, transfer the leadership first.",
                dir.x,
                dir.y + dir.h - fh * 0.35,
                dir.w - 200.0 * k,
                12,
                e::SUAVE,
            );
        }
    }

    fn sem_cla_ui(&mut self, a: Rect, d: &[char], s: &mut Vec<ClientMessage>) {
        let k = e::fator_texto().clamp(1.0, 1.4);
        let fh = 40.0 * k.min(1.2);
        let card = Rect::new(a.x, a.y, a.w, 30.0 * k + fh + 46.0 * k);
        e::cartao(card, false, false);
        e::texto(card.x + 14.0, card.y + 24.0 * k, "Create a clan", 18, e::OURO);
        let y = card.y + 52.0 * k;
        let bw = 150.0 * k;
        let tag_w = 150.0 * k;
        let nome_w = card.w - tag_w - bw - 52.0;
        campo(
            Rect::new(card.x + 14.0, y, nome_w, fh),
            "Clan name (3 to 24 characters)",
            &mut self.nome_cla,
            3,
            &mut self.foco,
            d,
            24,
        );
        campo(
            Rect::new(card.x + 26.0 + nome_w, y, tag_w, fh),
            "Tag (2-5, A-Z 0-9)",
            &mut self.tag_cla,
            7,
            &mut self.foco,
            d,
            social::TAG_MAX,
        );
        let valida = social::tag_valida(&self.tag_cla);
        if crate::ui::botao(
            Rect::new(card.x + card.w - bw - 14.0, y, bw, fh),
            "Create clan",
            self.nome_cla.trim().chars().count() >= 3 && valida.is_ok(),
        ) {
            self.enviar(
                Pedido::CriarCla { nome: self.nome_cla.trim().into(), tag: self.tag_cla.trim().into() },
                s,
            );
        }
        let dica = match (&valida, self.tag_cla.trim().is_empty()) {
            (_, true) => "The tag appears before every member's name: [TAG] Name.".to_string(),
            (Err(m), _) => m.to_string(),
            (Ok(t), _) => format!("Members will show as [{t}] Name."),
        };
        e::texto_ajustado(
            &dica,
            card.x + 14.0,
            y + fh + 18.0 * k,
            card.w - 28.0,
            13,
            if valida.is_err() && !self.tag_cla.trim().is_empty() { e::VERMELHO } else { e::SUAVE },
        );
        let ty = card.y + card.h + 28.0 * k;
        e::texto(a.x, ty, "Invites received", 18, e::OURO);
        let area = Rect::new(a.x, ty + 12.0 * k, a.w, a.y + a.h - ty - 12.0 * k);
        let convites = self.estado.convites_cla.clone();
        let rh = (48.0 * k).max(e::ALVO_DO_DEDO + 4.0);
        let range = paginacao_em(area, convites.len(), &mut self.pagina, rh, k);
        if convites.is_empty() {
            e::texto(a.x, area.y + 25.0 * k, "You have no clan invites yet.", 16, e::SUAVE);
        }
        let bw = 110.0 * k;
        for (i, cv) in convites[range].iter().enumerate() {
            let r = Rect::new(area.x, area.y + i as f32 * rh, area.w, rh - 4.0);
            e::cartao(r, false, false);
            e::texto_ajustado(
                &format!("{} · invited by {}", cv.nome, cv.de),
                r.x + 12.0,
                r.center().y + 6.0,
                r.w - 2.0 * bw - 40.0,
                16,
                e::TEXTO,
            );
            let bh = 38.0 * k.min(1.15);
            let by = r.y + (r.h - bh) * 0.5;
            if crate::ui::botao(Rect::new(r.x + r.w - 2.0 * bw - 16.0, by, bw, bh), "Accept", true) {
                self.enviar(Pedido::ResponderCla { id: cv.id, aceitar: true }, s);
            }
            if crate::ui::botao(Rect::new(r.x + r.w - bw - 8.0, by, bw, bh), "Decline", true) {
                self.enviar(Pedido::ResponderCla { id: cv.id, aceitar: false }, s);
            }
        }
    }
}

/// What can be done to a member from the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcaoMembro {
    Grupo,
    Promover,
    Rebaixar,
    Lider,
    Expulsar,
}

impl AcaoMembro {
    fn rotulo(self) -> &'static str {
        match self {
            Self::Grupo => "Party",
            Self::Promover => "Promote",
            Self::Rebaixar => "Demote",
            Self::Lider => "Leader",
            Self::Expulsar => "Kick",
        }
    }
}

/// The buttons a member row shows to me (`meu` = my rank).
fn acoes_do_membro(meu: u8, mb: &social::Contato, eu: &str) -> Vec<AcaoMembro> {
    use social::{cargo, pode_no_cla, AcaoCla as A};
    if mb.nome == eu {
        return Vec::new();
    }
    let mut v = Vec::new();
    if mb.online {
        v.push(AcaoMembro::Grupo);
    }
    if pode_no_cla(meu, A::Cargo, mb.cargo) {
        v.push(if mb.cargo == cargo::OFICIAL { AcaoMembro::Rebaixar } else { AcaoMembro::Promover });
    }
    if pode_no_cla(meu, A::TransferirLideranca, mb.cargo) {
        v.push(AcaoMembro::Lider);
    }
    if pode_no_cla(meu, A::Expulsar, mb.cargo) {
        v.push(AcaoMembro::Expulsar);
    }
    v
}

/// My rank in `c`.
fn cargo_de(c: &social::Cla, nome: &str) -> u8 {
    if c.lider == nome {
        social::cargo::LIDER
    } else if c.oficiais.iter().any(|o| o == nome) {
        social::cargo::OFICIAL
    } else {
        social::cargo::MEMBRO
    }
}

/// Every member with what the server knows of them; a member missing from
/// `contatos` (an older server answer) still shows, with only the name.
fn membros_do_cla(c: &social::Cla, contatos: &[social::Contato], neste_canal: &[String]) -> Vec<social::Contato> {
    c.membros
        .iter()
        .map(|n| {
            let mut m = contatos
                .iter()
                .find(|x| &x.nome == n)
                .cloned()
                .unwrap_or_else(|| social::Contato {
                    nome: n.clone(),
                    tag: c.tag.clone(),
                    conjunto: social::SEM_CONJUNTO,
                    online: neste_canal.contains(n),
                    ..Default::default()
                });
            m.cargo = cargo_de(c, n);
            m
        })
        .collect()
}

/// "Frozen Isle · Ch 2" when online, "Last seen 3h ago" when not.
fn onde_esta(c: &social::Contato, agora: i64) -> String {
    if c.online {
        if c.zona.is_empty() {
            "Online".to_string()
        } else if c.canal.is_empty() {
            crate::hud::nome_da_zona(&c.zona)
        } else {
            format!("{} · Ch {}", crate::hud::nome_da_zona(&c.zona), c.canal)
        }
    } else if c.visto > 0 {
        format!("Last seen {}", social::visto_ha(c.visto, agora).to_lowercase())
    } else {
        "Offline".to_string()
    }
}

fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The clan's emblem: a coin with the tag (or the name's initial), in a colour
/// derived from it so each clan keeps its own.
fn emblema(c: Vec2, raio: f32, tag: &str, nome: &str) {
    let base = if tag.is_empty() { nome } else { tag };
    let h = base.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let matiz = (h % 360) as f32 / 360.0;
    let cor = macroquad::color::hsl_to_rgb(matiz, 0.45, 0.38);
    draw_circle(c.x, c.y, raio + 2.0, e::BORDA_FORTE);
    draw_circle(c.x, c.y, raio, cor);
    draw_circle_lines(c.x, c.y, raio - 3.0, 1.0, e::alfa(e::OURO, 0.5));
    let texto: String = if tag.is_empty() {
        nome.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
    } else {
        tag.to_string()
    };
    let tam = match texto.chars().count() {
        0..=2 => 17,
        3 => 14,
        4 => 12,
        _ => 10,
    };
    let w = e::medir_forte(&texto, tam);
    e::texto_forte(c.x - w * 0.5, c.y + tam as f32 * e::fator_texto() * 0.36, &texto, tam, e::TEXTO);
}
fn botao_fim(a: Rect, y: f32, col: usize, t: &str) -> bool {
    crate::ui::botao(
        Rect::new(a.x + a.w - col as f32 * 112.0, y, 104.0, 38.0),
        t,
        true,
    )
}
/// Like `paginacao`, with the row height and button size of a scaled list.
fn paginacao_em(a: Rect, total: usize, pagina: &mut usize, rh: f32, k: f32) -> std::ops::Range<usize> {
    let bh = 34.0 * k.min(1.2);
    let linhas = ((a.h - bh - 8.0) / rh).floor().max(1.0) as usize;
    let paginas = total.div_ceil(linhas).max(1);
    *pagina = (*pagina).min(paginas - 1);
    if paginas > 1 {
        let y = a.y + a.h - bh;
        let bw = 80.0 * k.min(1.2);
        if crate::ui::botao(Rect::new(a.x, y, bw, bh), "<", *pagina > 0) {
            *pagina -= 1;
        }
        e::texto(a.x + bw + 14.0, y + bh * 0.68, &format!("{} / {}", *pagina + 1, paginas), 14, e::SUAVE);
        if crate::ui::botao(Rect::new(a.x + bw + 84.0 * k.min(1.2), y, bw, bh), ">", *pagina + 1 < paginas) {
            *pagina += 1;
        }
    }
    (*pagina * linhas).min(total)..(((*pagina + 1) * linhas).min(total))
}
fn paginacao(a: Rect, total: usize, pagina: &mut usize) -> std::ops::Range<usize> {
    let linhas = ((a.h - 44.0) / 50.0).floor().max(1.0) as usize;
    let paginas = total.div_ceil(linhas).max(1);
    *pagina = (*pagina).min(paginas - 1);
    let y = a.y + a.h - 36.0;
    if crate::ui::botao(Rect::new(a.x, y, 80.0, 32.0), "<", *pagina > 0) {
        *pagina -= 1;
    }
    e::texto(
        a.x + 94.0,
        y + 22.0,
        &format!("{} / {}", *pagina + 1, paginas),
        14,
        e::SUAVE,
    );
    if crate::ui::botao(
        Rect::new(a.x + 164.0, y, 80.0, 32.0),
        ">",
        *pagina + 1 < paginas,
    ) {
        *pagina += 1;
    }
    (*pagina * linhas)..(((*pagina + 1) * linhas).min(total))
}
fn campo(
    r: Rect,
    label: &str,
    valor: &mut String,
    id: u8,
    foco: &mut Option<u8>,
    digitado: &[char],
    max: usize,
) {
    let clicou = crate::foco::clique();
    if clicou && r.toca(Vec2::from(mouse_position())) {
        *foco = Some(id);
    } else if clicou && *foco == Some(id) {
        *foco = None;
    }
    let ativo = *foco == Some(id);
    if ativo {
        for &c in digitado {
            if c == '\u{8}' {
                valor.pop();
            } else if !c.is_control() && valor.chars().count() < max {
                valor.push(c);
            }
        }
        if is_key_pressed(KeyCode::Backspace) && !digitado.contains(&'\u{8}') {
            valor.pop();
        }
    }
    e::texto(r.x, r.y - 8.0, label, 13, e::SUAVE);
    e::cartao(r, false, ativo);
    let texto = format!(
        "{}{}",
        valor,
        if ativo && ((get_time() * 2.0) as i32) % 2 == 0 {
            "|"
        } else {
            ""
        }
    );
    texto_em_linhas(
        &texto,
        Rect::new(r.x + 10.0, r.y + 6.0, r.w - 20.0, r.h - 10.0),
        16,
        e::TEXTO,
        if ativo { usize::MAX } else { 0 },
    );
}
fn texto_em_linhas(texto: &str, r: Rect, tam: u16, cor: Color, desloc: usize) -> usize {
    let mut linhas = vec![String::new()];
    for ch in texto.chars() {
        // A letter's paragraphs (official mail can have line breaks).
        if ch == '\n' {
            linhas.push(String::new());
            continue;
        }
        let ultima = linhas.last_mut().unwrap();
        if !ultima.is_empty() && e::medir(&format!("{ultima}{ch}"), tam) > r.w {
            linhas.push(ch.to_string());
        } else {
            ultima.push(ch);
        }
    }
    let altura = tam as f32 * e::fator_texto() + 6.0;
    let n = (r.h / altura).floor().max(1.0) as usize;
    let inicio = if desloc == usize::MAX {
        linhas.len().saturating_sub(n)
    } else {
        desloc.min(linhas.len())
    };
    for (i, l) in linhas.iter().skip(inicio).take(n).enumerate() {
        e::texto(r.x, r.y + tam as f32 + i as f32 * altura, l, tam, cor);
    }
    linhas.len()
}

#[cfg(debug_assertions)]
fn previa_contatos() -> Vec<social::Contato> {
    let agora = agora_unix();
    let c = |nome: &str, nivel: u16, conjunto: u8, zona: &str, canal: &str, online: bool, visto: i64| social::Contato {
        nome: nome.into(),
        tag: "TIDE".into(),
        cargo: 0,
        nivel,
        conjunto,
        zona: zona.into(),
        canal: canal.into(),
        online,
        visto,
    };
    vec![
        c("brunji", 42, 2, "ilha_inicial", "1", true, agora),
        c("MaréAlta", 38, 0, "ilha_gelo", "2", true, agora),
        c("Navegante", 35, 3, "ilha_inicial", "3", true, agora),
        c("Captain", 29, 1, "", "", false, agora - 3 * 3600),
    ]
}

/// Local captures with fictional data, with no network and no character changes.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-social-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    let rt = render_target(1280, 720);
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::hud_layout::define_escala_ui(1.3);
    let mut ui = Social::default();
    let vox = crate::vox::VoxCache::default();
    let solido = crate::render3d::material_solido();
    ui.receber(Aviso::Estado(Estado {
        amigos:vec!["MaréAlta".into(),"Navegante".into()],recebidos:vec!["Corsair".into()],enviados:vec!["Captain".into()],
        neste_canal:vec!["brunji".into(),"MaréAlta".into()],
        cartas:vec![social::Carta{id:1,de:"MaréAlta".into(),assunto:"Ready for the next adventure?".into(),texto:"Vamos reunir o grupo na vila e explorar as dungeons. Se precisar de ajuda, mande uma carta!".into(),quando:0,lida:false,oficial:false,anexos:vec![],resgatada:false}],
        cla:Some(social::Cla{id:1,nome:"Tide Corsairs".into(),lider:"brunji".into(),aviso:"Meet at the village for the next expedition.".into(),membros:vec!["brunji".into(),"MaréAlta".into(),"Navegante".into(),"Captain".into()],tag:"TIDE".into(),oficiais:vec!["MaréAlta".into()]}),
        convites_cla:vec![], cargo:Some("admin".into()),destinatarios:42,oficiais:vec![],
        contatos: previa_contatos(),
    }));
    ui.grupo = vec!["brunji".into(), "MaréAlta".into()];
    ui.grupo_tags = vec!["TIDE".into(), "TIDE".into()];
    ui.carta = Some(1);
    for (aba, nome) in [
        (Aba::Grupo, "grupo"),
        (Aba::Amigos, "amigos"),
        (Aba::Correio, "correio"),
        (Aba::Cla, "cla"),
    ] {
        ui.abrir(aba);
        if aba == Aba::Cla {
            ui.cla_sel = Some("Navegante".into());
        }
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha("brunji", &[], &vox, &solido);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    // The clan's Manage page, and the no-clan page with an invite.
    ui.abrir(Aba::Cla);
    ui.cla_gerir = true;
    ui.tag_cla = "tide".into();
    for (nome, sem_cla) in [("cla-gerir", false), ("cla-criar", true)] {
        if sem_cla {
            ui.estado.cla = None;
            ui.estado.convites_cla = vec![social::ConviteCla { id: 2, nome: "Storm Wardens".into(), de: "Captain".into() }];
            ui.nome_cla = "Storm Wardens".into();
            ui.tag_cla = "ST0RM".into();
        }
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha("brunji", &[], &vox, &solido);
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    ui.abrir(Aba::Correio);
    ui.escrevendo = true;
    ui.nome = "MaréAlta".into();
    ui.assunto = "Meeting in the village".into();
    ui.texto = "Vamos explorar juntos depois de reunir o grupo?".into();
    for editor in [false, true] {
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha("brunji", &[], &vox, &solido);
            if editor {
                ui.foco = Some(2);
                ui.editor_mobile(crate::hud_layout::tela_segura(), &[]);
            }
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!(
                "{saida}/{}.png",
                if editor { "teclado" } else { "escrever" }
            ));
            next_frame().await;
        }
    }
    ui.foco = None;
    ui.admin = true;
    ui.todos = true;
    ui.picker = false;
    ui.assunto = "Event reward".into();
    ui.texto = "Obrigado por participar! Receba seus itens abaixo.".into();
    ui.nomes.insert(1, "Health Potion".into());
    ui.nomes.insert(2, "Forge Stone".into());
    ui.anexos = vec![
        social::Anexo {
            item_id: 1,
            qtd: 10,
            instance: None,
        },
        social::Anexo {
            item_id: 2,
            qtd: 5,
            instance: None,
        },
    ];
    for etapa in ["admin-enviar", "admin-anexos", "oficial-receber"] {
        ui.picker = etapa == "admin-anexos";
        if etapa == "oficial-receber" {
            ui.escrevendo = false;
            ui.oficial = true;
            ui.carta = Some(-1);
            ui.estado.oficiais = vec![social::Carta {
                id: -1,
                de: "Equipe · brunji (admin)".into(),
                assunto: ui.assunto.clone(),
                texto: ui.texto.clone(),
                quando: 0,
                lida: false,
                oficial: true,
                anexos: ui.anexos.clone(),
                resgatada: false,
            }];
        }
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha("brunji", &[], &vox, &solido);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{etapa}.png"));
            next_frame().await;
        }
    }
    crate::render3d::define_alvo(None);
}

#[cfg(test)]
mod testes {
    use super::*;

    fn cla() -> social::Cla {
        social::Cla {
            id: 1,
            nome: "Tide".into(),
            lider: "Ana".into(),
            aviso: String::new(),
            membros: vec!["Ana".into(), "Bia".into(), "Caio".into()],
            tag: "TIDE".into(),
            oficiais: vec!["Bia".into()],
        }
    }

    #[test]
    fn membros_do_cla_tem_cargo_e_presenca() {
        let c = cla();
        let contatos = vec![social::Contato {
            nome: "Caio".into(),
            online: true,
            zona: "ilha_gelo".into(),
            canal: "2".into(),
            nivel: 12,
            ..Default::default()
        }];
        // Bia is missing from `contatos` but is on this channel.
        let mut v = membros_do_cla(&c, &contatos, &["Bia".to_string()]);
        social::ordenar_membros(&mut v);
        let r: Vec<_> = v.iter().map(|m| (m.nome.as_str(), m.cargo, m.online)).collect();
        assert_eq!(r, [("Bia", 1, true), ("Caio", 0, true), ("Ana", 2, false)]);
        assert!(onde_esta(&v[1], 0).ends_with("· Ch 2"));
        assert_eq!(onde_esta(&v[2], 0), "Offline");
        let visto = social::Contato { visto: 100, ..Default::default() };
        assert_eq!(onde_esta(&visto, 100 + 7200), "Last seen 2h ago");
    }

    #[test]
    fn acoes_por_cargo() {
        let c = cla();
        let m = |n: &str, online: bool| social::Contato { nome: n.into(), cargo: cargo_de(&c, n), online, ..Default::default() };
        use AcaoMembro::*;
        // The leader sees everything on an officer; an officer on a member: party + kick.
        assert_eq!(acoes_do_membro(2, &m("Bia", true), "Ana"), [Grupo, Rebaixar, Lider, Expulsar]);
        assert_eq!(acoes_do_membro(1, &m("Caio", false), "Bia"), [Expulsar]);
        assert!(acoes_do_membro(1, &m("Ana", true), "Bia") == [Grupo], "nobody kicks the leader");
        assert!(acoes_do_membro(0, &m("Bia", false), "Caio").is_empty());
        assert!(acoes_do_membro(2, &m("Ana", true), "Ana").is_empty(), "no buttons on myself");
    }

    /// "Take all" claims BEFORE deleting. Inverted, a letter with an attachment
    /// vanished with the item inside — the most expensive possible way to empty
    /// the inbox.
    #[test]
    fn a_fila_resgata_antes_de_apagar() {
        let carta = |id: i64, anexos: bool, resgatada: bool| shared::social::Carta {
            id,
            de: "System".into(),
            assunto: "x".into(),
            texto: "y".into(),
            quando: 0,
            lida: false,
            oficial: true,
            anexos: if anexos {
                vec![shared::social::Anexo {
                    item_id: 424,
                    qtd: 1,
                    instance: None,
                }]
            } else {
                Vec::new()
            },
            resgatada,
        };
        // Two with an unclaimed attachment, one already claimed, one with no attachment.
        let cartas = vec![
            carta(1, true, false),
            carta(2, true, true),
            carta(3, false, false),
            carta(4, true, false),
        ];
        let a_resgatar: Vec<i64> = cartas
            .iter()
            .filter(|c| c.oficial && !c.anexos.is_empty() && !c.resgatada)
            .map(|c| c.id)
            .collect();
        assert_eq!(a_resgatar, vec![1, 4], "so' as que tem anexo POR PEGAR");

        let mut fila: Vec<Pedido> = a_resgatar
            .iter()
            .map(|id| Pedido::ReceberAnexos { id: *id })
            .collect();
        fila.extend(cartas.iter().map(|c| Pedido::ApagarCarta { id: c.id }));

        // Every claim comes before every delete.
        let primeiro_apagar = fila
            .iter()
            .position(|p| matches!(p, Pedido::ApagarCarta { .. }))
            .expect("tem apagar");
        assert!(
            fila[..primeiro_apagar]
                .iter()
                .all(|p| matches!(p, Pedido::ReceberAnexos { .. })),
            "apagar entrou no meio dos resgates"
        );
        // And every letter is deleted, including the ones that had no attachment.
        assert_eq!(fila.len(), a_resgatar.len() + cartas.len());
        for c in &cartas {
            assert!(
                fila.iter()
                    .any(|p| matches!(p, Pedido::ApagarCarta { id } if *id == c.id)),
                "carta {} ficou pra tras",
                c.id
            );
        }
    }
}
