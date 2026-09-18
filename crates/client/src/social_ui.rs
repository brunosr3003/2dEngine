//! Grupo, amigos, cartas e cla: o cliente pede, o servidor valida.
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
    pub convite: Option<(String, f64)>,
    pub entregas: bool,
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
    foco: Option<u8>,
    pagina: usize,
    carta: Option<i64>,
    carta_pagina: usize,
    escrevendo: bool,
    confirmacao: Option<(String, Pedido)>,
    aviso: String,
    ok: bool,
    carregado: bool,
    atualizar_em: f64,
    ocupado_ate: f64,
}
fn msg(pedido: Pedido) -> ClientMessage {
    ClientMessage::Social { pedido }
}

impl Social {
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
        vec![msg(Pedido::Estado)]
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
                if ok && (texto == "Carta enviada." || texto.starts_with("Correio oficial enviado"))
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
        // Atualiza notificacoes mesmo com a janela fechada, sem interromper digitacao.
        if agora >= self.atualizar_em && agora >= self.ocupado_ate {
            self.atualizar_em = agora + if self.aberto { 5.0 } else { 20.0 };
            return Some(msg(Pedido::Estado));
        }
        None
    }
    pub fn desenha(&mut self, eu: &str, digitado: &[char]) -> Vec<ClientMessage> {
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
        // Editor acima do teclado virtual, inclusive para cartas longas.
        // Volta ao formulario ao concluir, sem enviar nada implicitamente.
        if crate::nativo::TECLADO_NA_TELA && self.foco.is_some() {
            self.editor_mobile(t, digitado);
            return saida;
        }
        if self.confirmacao.is_some() {
            self.desenha_confirmacao(p, &mut saida);
            return saida;
        }
        let abas = [
            (Aba::Grupo, "Grupo"),
            (Aba::Amigos, "Amigos"),
            (Aba::Correio, "Correio"),
            (Aba::Cla, "Clã"),
        ];
        let tw = (p.w - 40.0) / 4.0;
        for (i, (aba, nome)) in abas.iter().enumerate() {
            let r = Rect::new(p.x + 20.0 + i as f32 * tw, p.y + 50.0, tw - 5.0, 40.0);
            e::aba(
                r,
                nome,
                self.aba == *aba,
                r.contains(Vec2::from(mouse_position())),
            );
            let alerta = match aba {
                Aba::Grupo => self.convite.is_some(),
                Aba::Amigos => !self.estado.recebidos.is_empty(),
                Aba::Correio => self
                    .estado
                    .cartas
                    .iter()
                    .chain(&self.estado.oficiais)
                    .any(|c| !c.lida),
                Aba::Cla => !self.estado.convites_cla.is_empty(),
            };
            if alerta {
                e::badge(r);
            }
            if r.contains(Vec2::from(mouse_position()))
                && is_mouse_button_pressed(MouseButton::Left)
            {
                self.aba = *aba;
                self.pagina = 0;
                self.foco = None;
            }
        }
        let a = Rect::new(p.x + 20.0, p.y + 112.0, p.w - 40.0, p.h - 180.0);
        match self.aba {
            Aba::Grupo => self.grupo_ui(a, digitado, &mut saida),
            Aba::Amigos => self.amigos_ui(a, digitado, &mut saida),
            Aba::Correio => self.correio_ui(a, digitado, &mut saida),
            Aba::Cla => self.cla_ui(a, eu, digitado, &mut saida),
        }
        let aviso = if !self.carregado && self.aviso.is_empty() {
            "Carregando…"
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
            "Concluir",
            true,
        ) {
            self.foco = None;
            return;
        }
        let (label, valor, max) = match id {
            1 => ("Assunto", &mut self.assunto, 60),
            2 => ("Mensagem", &mut self.texto, 1000),
            3 => ("Nome do clã", &mut self.nome_cla, 24),
            4 => ("Aviso do clã", &mut self.aviso_cla, 200),
            5 => ("Buscar item", &mut self.busca_item, 60),
            6 => ("Quantidade", &mut self.quantidade, 6),
            _ => ("Nome do personagem", &mut self.nome, 32),
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
        self.aviso = "Aguardando servidor…".into();
        self.ok = true;
        self.foco = None;
        saida.push(msg(p));
    }
    fn confirmar(&mut self, texto: String, p: Pedido) {
        self.confirmacao = Some((texto, p));
        self.foco = None;
    }
    fn desenha_confirmacao(&mut self, p: Rect, saida: &mut Vec<ClientMessage>) {
        let (texto, pedido) = self.confirmacao.clone().unwrap();
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
            "Cancelar",
            true,
        ) {
            self.confirmacao = None;
        }
        if crate::ui::botao(
            Rect::new(p.center().x + 10.0, p.y + p.h - 85.0, 160.0, 42.0),
            "Confirmar",
            get_time() >= self.ocupado_ate,
        ) {
            self.confirmacao = None;
            self.enviar(pedido, saida);
        }
    }
    fn grupo_ui(&mut self, a: Rect, d: &[char], s: &mut Vec<ClientMessage>) {
        e::texto(
            a.x,
            a.y + 14.0,
            "Até 5 jogadores no mesmo canal. Convites duram 60 segundos.",
            15,
            e::SUAVE,
        );
        campo(
            Rect::new(a.x, a.y + 48.0, a.w - 180.0, 40.0),
            "Nome do personagem",
            &mut self.nome,
            0,
            &mut self.foco,
            d,
            32,
        );
        if crate::ui::botao(
            Rect::new(a.x + a.w - 168.0, a.y + 48.0, 168.0, 40.0),
            "Convidar",
            !self.nome.trim().is_empty(),
        ) {
            s.push(ClientMessage::PartyInvite {
                target_name: self.nome.trim().into(),
            });
            self.foco = None;
        }
        let mut y = a.y + 124.0;
        if let Some((nome, _)) = self.convite.clone() {
            e::texto_ajustado(
                &format!("{nome} convidou você"),
                a.x,
                y,
                a.w - 235.0,
                18,
                e::OURO,
            );
            if crate::ui::botao(
                Rect::new(a.x + a.w - 226.0, y - 24.0, 108.0, 36.0),
                "Aceitar",
                true,
            ) {
                s.push(ClientMessage::PartyAccept);
                self.convite = None;
            }
            if crate::ui::botao(
                Rect::new(a.x + a.w - 108.0, y - 24.0, 108.0, 36.0),
                "Recusar",
                true,
            ) {
                s.push(ClientMessage::PartyDecline);
                self.convite = None;
            }
            y += 56.0;
        }
        if self.grupo.is_empty() {
            e::texto(a.x, y, "Você está jogando solo.", 18, e::TEXTO);
        }
        for nome in &self.grupo {
            e::texto(a.x + 12.0, y, nome, 18, e::TEXTO);
            y += 42.0;
        }
        if !self.grupo.is_empty()
            && crate::ui::botao(
                Rect::new(a.x, a.y + a.h - 36.0, 160.0, 36.0),
                "Sair do grupo",
                true,
            )
        {
            s.push(ClientMessage::PartyLeave);
        }
    }
    fn amigos_ui(&mut self, a: Rect, d: &[char], s: &mut Vec<ClientMessage>) {
        campo(
            Rect::new(a.x, a.y + 20.0, a.w - 190.0, 40.0),
            "Nome do personagem",
            &mut self.nome,
            0,
            &mut self.foco,
            d,
            32,
        );
        if crate::ui::botao(
            Rect::new(a.x + a.w - 178.0, a.y + 20.0, 178.0, 40.0),
            "Adicionar amigo",
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
                "{} / {} amigos · Presença indica este canal",
                self.estado.amigos.len(),
                social::MAX_AMIGOS
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
                "Adicione amigos para encontrá-los aqui.",
                18,
                e::SUAVE,
            );
        }
        for (i, (nome, tipo)) in rows[range].iter().enumerate() {
            let y = area.y + i as f32 * 50.0;
            let status = match tipo {
                0 => "quer ser seu amigo",
                2 => "pedido enviado",
                _ => {
                    if self.estado.neste_canal.contains(nome) {
                        "neste canal"
                    } else {
                        "fora deste canal"
                    }
                }
            };
            e::texto_ajustado(
                &format!("{nome} · {status}"),
                a.x,
                y + 24.0,
                a.w - 345.0,
                16,
                e::TEXTO,
            );
            if *tipo == 0 {
                if botao_fim(a, y, 2, "Aceitar") {
                    self.enviar(
                        Pedido::ResponderAmizade {
                            nome: nome.clone(),
                            aceitar: true,
                        },
                        s,
                    );
                }
                if botao_fim(a, y, 1, "Recusar") {
                    self.enviar(
                        Pedido::ResponderAmizade {
                            nome: nome.clone(),
                            aceitar: false,
                        },
                        s,
                    );
                }
            } else {
                if *tipo == 1 && botao_fim(a, y, 3, "Grupo") {
                    s.push(ClientMessage::PartyInvite {
                        target_name: nome.clone(),
                    });
                }
                if *tipo == 1 && botao_fim(a, y, 2, "Carta") {
                    self.nome = nome.clone();
                    self.aba = Aba::Correio;
                    self.escrevendo = true;
                    self.foco = None;
                }
                if botao_fim(a, y, 1, if *tipo == 2 { "Cancelar" } else { "Remover" }) {
                    self.confirmar(
                        format!("Remover amizade ou pedido com {nome}?"),
                        Pedido::RemoverAmigo { nome: nome.clone() },
                    );
                }
            }
        }
    }
    fn correio_ui(&mut self, a: Rect, d: &[char], s: &mut Vec<ClientMessage>) {
        if crate::ui::botao(Rect::new(a.x, a.y, 150.0, 38.0), "Escrever carta", true) {
            self.escrevendo = true;
            self.carta = None;
            self.admin = false;
            self.foco = None;
        }
        if crate::ui::botao(
            Rect::new(a.x + 160.0, a.y, 210.0, 38.0),
            "Recompensas",
            true,
        ) {
            self.entregas = true;
            self.fechar();
            return;
        }
        if self.estado.cargo.is_some()
            && crate::ui::botao(
                Rect::new(a.x + 382.0, a.y, 180.0, 38.0),
                "Envio admin/mod",
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
                        "Todos os personagens"
                    } else {
                        "Um personagem"
                    },
                    true,
                ) {
                    self.todos = !self.todos;
                    self.foco = None;
                }
            }
            if crate::ui::botao(
                Rect::new(a.x + a.w - 110.0, a.y, 110.0, 38.0),
                "Caixa",
                true,
            ) {
                self.escrevendo = false;
                self.foco = None;
            }
            if self.admin && self.todos {
                e::texto_ajustado(
                    &format!("Todos: {} personagens", self.estado.destinatarios),
                    a.x,
                    a.y + 102.0,
                    a.w * 0.35,
                    18,
                    e::OURO,
                );
            } else {
                campo(
                    Rect::new(a.x, a.y + 76.0, a.w * 0.35, 40.0),
                    "Para",
                    &mut self.nome,
                    0,
                    &mut self.foco,
                    d,
                    32,
                );
            }
            campo(
                Rect::new(a.x + a.w * 0.37, a.y + 76.0, a.w * 0.63, 40.0),
                "Assunto",
                &mut self.assunto,
                1,
                &mut self.foco,
                d,
                60,
            );
            campo(
                Rect::new(a.x, a.y + 155.0, a.w, a.h - 208.0),
                "Mensagem (até 1000 caracteres)",
                &mut self.texto,
                2,
                &mut self.foco,
                d,
                1000,
            );
            if crate::ui::botao(
                Rect::new(a.x + a.w - 140.0, a.y + a.h - 40.0, 140.0, 40.0),
                "Enviar",
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
                    &format!("Anexos: {} / 8", self.anexos.len()),
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
                "Oficiais · trocar caixa"
            } else {
                "Pessoais · trocar caixa"
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
        let range = paginacao(left, cartas.len(), &mut self.pagina);
        if cartas.is_empty() {
            e::texto(left.x, left.y + 25.0, "Sua caixa está vazia.", 18, e::SUAVE);
        }
        for (i, c) in cartas[range].iter().enumerate() {
            let r = Rect::new(left.x, left.y + i as f32 * 50.0, left.w - 12.0, 44.0);
            e::cartao(r, false, self.carta == Some(c.id));
            e::texto_ajustado(
                &format!(
                    "{}{} · {}",
                    if c.lida { "" } else { "Nova: " },
                    c.de,
                    c.assunto
                ),
                r.x + 8.0,
                r.y + 27.0,
                r.w - 16.0,
                16,
                if c.lida { e::TEXTO } else { e::OURO },
            );
            if r.contains(Vec2::from(mouse_position()))
                && is_mouse_button_pressed(MouseButton::Left)
            {
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
                &format!("De: {}", c.de),
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
                    "{}  |  Anexos: {}. {}",
                    c.texto,
                    anexos,
                    if c.resgatada {
                        "Recebidos."
                    } else {
                        "Aguardando resgate."
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
                    &format!("Texto {} / {}", self.carta_pagina + 1, paginas),
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
                    "Receber itens"
                } else {
                    "Responder"
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
                "Apagar",
                !c.oficial || c.anexos.is_empty() || c.resgatada,
            ) {
                self.confirmar(
                    "Apagar esta carta?".into(),
                    Pedido::ApagarCarta { id: c.id },
                );
            }
        } else {
            e::texto(
                right.x,
                right.y + 24.0,
                "Selecione uma carta para ler.",
                16,
                e::SUAVE,
            );
        }
    }
    fn picker_ui(&mut self, a: Rect, d: &[char]) {
        if crate::ui::botao(
            Rect::new(a.x + a.w - 160.0, a.y, 160.0, 38.0),
            "Voltar à carta",
            true,
        ) {
            self.picker = false;
            self.foco = None;
            return;
        }
        campo(
            Rect::new(a.x, a.y + 80.0, a.w * 0.55, 40.0),
            "Buscar item por nome ou ID",
            &mut self.busca_item,
            5,
            &mut self.foco,
            d,
            60,
        );
        campo(
            Rect::new(a.x + a.w * 0.60, a.y + 80.0, 150.0, 40.0),
            "Quantidade",
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
                "Adicionar",
                qtd.is_some()
                    && self.anexos.len() < social::MAX_ANEXOS
                    && !self.anexos.iter().any(|a| a.item_id == *id),
            ) {
                self.anexos.push(social::Anexo {
                    item_id: *id,
                    qtd: qtd.unwrap(),
                });
            }
        }
        let x = a.x + a.w * 0.60;
        e::texto(
            x,
            a.y + 150.0,
            &format!("Anexos: {} / 8", self.anexos.len()),
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

    fn cla_ui(&mut self, a: Rect, eu: &str, d: &[char], s: &mut Vec<ClientMessage>) {
        if let Some(c) = self.estado.cla.clone() {
            let lider = c.lider == eu;
            e::texto_ajustado(
                &format!(
                    "{} · {} / {} membros · Líder: {}",
                    c.nome,
                    c.membros.len(),
                    social::MAX_CLA,
                    c.lider
                ),
                a.x,
                a.y + 18.0,
                a.w,
                20,
                e::OURO,
            );
            if lider {
                campo(
                    Rect::new(a.x, a.y + 54.0, a.w - 145.0, 42.0),
                    "Aviso do clã",
                    &mut self.aviso_cla,
                    4,
                    &mut self.foco,
                    d,
                    200,
                );
                if crate::ui::botao(
                    Rect::new(a.x + a.w - 135.0, a.y + 54.0, 135.0, 42.0),
                    "Salvar aviso",
                    true,
                ) {
                    self.enviar(
                        Pedido::AvisoCla {
                            texto: self.aviso_cla.clone(),
                        },
                        s,
                    );
                }
                campo(
                    Rect::new(a.x, a.y + 132.0, a.w - 145.0, 40.0),
                    "Convidar personagem",
                    &mut self.nome,
                    0,
                    &mut self.foco,
                    d,
                    32,
                );
                if crate::ui::botao(
                    Rect::new(a.x + a.w - 135.0, a.y + 132.0, 135.0, 40.0),
                    "Convidar",
                    !self.nome.trim().is_empty(),
                ) {
                    self.enviar(
                        Pedido::ConvidarCla {
                            nome: self.nome.trim().into(),
                        },
                        s,
                    );
                }
            } else {
                texto_em_linhas(
                    &c.aviso,
                    Rect::new(a.x, a.y + 44.0, a.w, 100.0),
                    17,
                    e::TEXTO,
                    0,
                );
            }
            let area = Rect::new(a.x, a.y + 192.0, a.w, a.h - 242.0);
            let range = paginacao(area, c.membros.len(), &mut self.pagina);
            for (i, n) in c.membros[range].iter().enumerate() {
                let y = area.y + i as f32 * 50.0;
                let status = if self.estado.neste_canal.contains(n) {
                    "neste canal"
                } else {
                    "fora deste canal"
                };
                e::texto_ajustado(
                    &format!(
                        "{n} · {status}{}",
                        if *n == c.lider { " · Líder" } else { "" }
                    ),
                    a.x,
                    y + 24.0,
                    a.w - 345.0,
                    16,
                    e::TEXTO,
                );
                if n != eu {
                    if botao_fim(a, y, 3, "Grupo") {
                        s.push(ClientMessage::PartyInvite {
                            target_name: n.clone(),
                        });
                    }
                    if lider {
                        if botao_fim(a, y, 2, "Líder") {
                            self.confirmar(format!("Transferir a liderança para {n}? Você perderá a administração do clã."),Pedido::LiderCla{nome:n.clone()});
                        }
                        if botao_fim(a, y, 1, "Expulsar") {
                            self.confirmar(
                                format!("Expulsar {n} do clã?"),
                                Pedido::ExpulsarCla { nome: n.clone() },
                            );
                        }
                    }
                }
            }
            if crate::ui::botao(
                Rect::new(a.x, a.y + a.h - 36.0, 180.0, 36.0),
                if lider {
                    "Dissolver clã"
                } else {
                    "Sair do clã"
                },
                true,
            ) {
                self.confirmar(
                    if lider {
                        "Dissolver o clã e remover todos os membros?"
                    } else {
                        "Sair do clã?"
                    }
                    .into(),
                    if lider {
                        Pedido::DissolverCla
                    } else {
                        Pedido::SairCla
                    },
                );
            }
        } else {
            campo(
                Rect::new(a.x, a.y + 24.0, a.w - 150.0, 40.0),
                "Nome do novo clã (3 a 24 caracteres)",
                &mut self.nome_cla,
                3,
                &mut self.foco,
                d,
                24,
            );
            if crate::ui::botao(
                Rect::new(a.x + a.w - 138.0, a.y + 24.0, 138.0, 40.0),
                "Criar clã",
                self.nome_cla.trim().chars().count() >= 3,
            ) {
                self.enviar(
                    Pedido::CriarCla {
                        nome: self.nome_cla.trim().into(),
                    },
                    s,
                );
            }
            e::texto(a.x, a.y + 110.0, "Convites recebidos", 18, e::OURO);
            let area = Rect::new(a.x, a.y + 132.0, a.w, a.h - 132.0);
            let convites = self.estado.convites_cla.clone();
            let range = paginacao(area, convites.len(), &mut self.pagina);
            if convites.is_empty() {
                e::texto(
                    a.x,
                    area.y + 25.0,
                    "Você ainda não tem convites de clã.",
                    17,
                    e::SUAVE,
                );
            }
            for (i, c) in convites[range].iter().enumerate() {
                let y = area.y + i as f32 * 50.0;
                e::texto_ajustado(
                    &format!("{} · convite de {}", c.nome, c.de),
                    a.x,
                    y + 24.0,
                    a.w - 240.0,
                    17,
                    e::TEXTO,
                );
                if botao_fim(a, y, 2, "Aceitar") {
                    self.enviar(
                        Pedido::ResponderCla {
                            id: c.id,
                            aceitar: true,
                        },
                        s,
                    );
                }
                if botao_fim(a, y, 1, "Recusar") {
                    self.enviar(
                        Pedido::ResponderCla {
                            id: c.id,
                            aceitar: false,
                        },
                        s,
                    );
                }
            }
        }
    }
}
fn botao_fim(a: Rect, y: f32, col: usize, t: &str) -> bool {
    crate::ui::botao(
        Rect::new(a.x + a.w - col as f32 * 112.0, y, 104.0, 38.0),
        t,
        true,
    )
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
    let clicou = is_mouse_button_pressed(MouseButton::Left);
    if clicou && r.contains(Vec2::from(mouse_position())) {
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

/// Capturas locais com dados ficticios, sem rede ou alteracao de personagens.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-social-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    let rt = render_target(1280, 720);
    crate::render3d::define_alvo(Some(rt.clone()));
    crate::hud_layout::define_escala_ui(1.3);
    let mut ui = Social::default();
    ui.receber(Aviso::Estado(Estado {
        amigos:vec!["MaréAlta".into(),"Navegante".into()],recebidos:vec!["Corsário".into()],enviados:vec!["Capitã".into()],
        neste_canal:vec!["brunji".into(),"MaréAlta".into()],
        cartas:vec![social::Carta{id:1,de:"MaréAlta".into(),assunto:"Prontos para a próxima aventura?".into(),texto:"Vamos reunir o grupo na vila e explorar as dungeons. Se precisar de ajuda, mande uma carta!".into(),quando:0,lida:false,oficial:false,anexos:vec![],resgatada:false}],
        cla:Some(social::Cla{id:1,nome:"Corsários da Maré".into(),lider:"brunji".into(),aviso:"Encontro na vila para a próxima expedição.".into(),membros:vec!["brunji".into(),"MaréAlta".into(),"Navegante".into()]}),
        convites_cla:vec![], cargo:Some("admin".into()),destinatarios:42,oficiais:vec![],
    }));
    ui.grupo = vec!["brunji".into(), "MaréAlta".into()];
    ui.carta = Some(1);
    for (aba, nome) in [
        (Aba::Grupo, "grupo"),
        (Aba::Amigos, "amigos"),
        (Aba::Correio, "correio"),
        (Aba::Cla, "cla"),
    ] {
        ui.abrir(aba);
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha("brunji", &[]);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
    ui.abrir(Aba::Correio);
    ui.escrevendo = true;
    ui.nome = "MaréAlta".into();
    ui.assunto = "Encontro na vila".into();
    ui.texto = "Vamos explorar juntos depois de reunir o grupo?".into();
    for editor in [false, true] {
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha("brunji", &[]);
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
    ui.assunto = "Recompensa do evento".into();
    ui.texto = "Obrigado por participar! Receba seus itens abaixo.".into();
    ui.nomes.insert(1, "Poção de Vida".into());
    ui.nomes.insert(2, "Pedra de Forja".into());
    ui.anexos = vec![
        social::Anexo {
            item_id: 1,
            qtd: 10,
        },
        social::Anexo { item_id: 2, qtd: 5 },
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
            ui.desenha("brunji", &[]);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{etapa}.png"));
            next_frame().await;
        }
    }
    crate::render3d::define_alvo(None);
}
