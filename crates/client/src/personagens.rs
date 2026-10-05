//! Selecao e criacao usam o mesmo contrato de personagens do servidor.
use crate::{hud_estilo as ui, render3d, vox::VoxCache};
use macroquad::material::{gl_use_default_material, gl_use_material, Material};
use macroquad::prelude::*;
use shared::{
    protocol::{CharacterListEntry, ClientMessage},
    skills::Conjunto,
    Faction,
};

pub enum Acao {
    Enviar(ClientMessage),
    Voltar,
}

/// A ilha de verdade atras da tela de personagens.
struct Fundo {
    terreno: crate::terreno::Terreno,
    casas: crate::construcoes::Construcoes,
    /// Centro da cidade: com casas o fundo le' como um LUGAR, nao so' morro.
    centro: Vec2,
}

impl Fundo {
    fn novo() -> Option<Self> {
        let def = shared::terreno::def_da_zona("ilha_inicial")?;
        let ger = shared::terreno::Gerador::da_ilha(def);
        // O Vec2 do shared e' de outra versao do glam.
        let centro = ger
            .cidade()
            .map(|c| {
                let p = c.centro();
                vec2(p.x, p.y)
            })
            .unwrap_or(Vec2::ZERO);
        Some(Self {
            terreno: crate::terreno::Terreno::novo(def),
            casas: crate::construcoes::Construcoes::para(Some(def)),
            centro,
        })
    }
}

/// Altura de um cartao da lista, com o vao.
const CARTAO: f32 = 87.0;

#[derive(Default)]
pub struct Personagens {
    pub criando: bool,
    pub nome: String,
    pub arma: Option<u16>,
    pub faccao: Faction,
    /// A aparência escolhida (docs/PERSONAGEM.md). O retrato 3D ao lado é
    /// desenhado com ela — escolher e ver são a mesma coisa.
    pub aparencia: shared::aparencia::Aparencia,
    /// Que metade do painel da direita está aberta: 0 = arma e facção,
    /// 1 = aparência. São duas porque as duas juntas não cabem numa tela de
    /// 390 px de altura (o iPhone deitado) — faltavam 119 px, medidos.
    aba: u8,
    pub mensagem: Option<String>,
    aguardando: Option<(String, f64)>,
    foco_nome: bool,
    rolagem: crate::rolagem::Rolagem,
    giro: f32,
    mouse_anterior: Option<Vec2>,
    saida_previa: Option<RenderTarget>,
    fundo: Option<Fundo>,
    fundo_tentado: bool,
}

pub fn valida_nome(nome: &str) -> Result<&str, &'static str> {
    let nome = nome.trim();
    if !(2..=24).contains(&nome.len()) {
        return Err("Use 2 to 24 characters.");
    }
    if !nome.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Use unaccented letters, numbers or _.");
    }
    Ok(nome)
}

fn conjunto_disponivel(id: u16, armas: &[u16]) -> bool {
    armas.contains(&id) && Conjunto::TODOS.iter().any(|c| c.arma() == id)
}

impl Personagens {
    pub fn recebeu_lista(
        &mut self,
        chars: &[CharacterListEntry],
        armas: &[u16],
        selecionado: &mut usize,
    ) {
        if let Some((nome, _)) = self.aguardando.take() {
            if let Some(i) = chars.iter().position(|c| c.name == nome) {
                *selecionado = i;
                self.criando = false;
                self.mensagem = Some("Personagem criado. Sua jornada pode começar.".into());
            }
        } else if chars.is_empty() {
            self.criando = true;
            self.foco_nome = true;
        }
        *selecionado = (*selecionado).min(chars.len().saturating_sub(1));
        if !self.arma.is_some_and(|id| conjunto_disponivel(id, armas)) {
            self.arma = Conjunto::TODOS
                .iter()
                .map(|c| c.arma())
                .find(|id| armas.contains(id));
        }
        self.rolagem.pos = selecionado.saturating_sub(3) as f32 * CARTAO;
    }

    /// O campo de nome esta' com foco (o teclado da tela tem que estar aberto).
    pub fn foco_no_nome(&self) -> bool {
        self.criando && self.foco_nome && self.aguardando.is_none()
    }

    pub fn falhou(&mut self, motivo: String) {
        self.aguardando = None;
        self.criando = true;
        self.mensagem = Some(match motivo.as_str() {
            "name already taken" => "That name is already taken. Choose another.".into(),
            "name 2-24 chars" => "Use a name of 2 to 24 characters.".into(),
            "name: letters/numbers/_ only" => "Use unaccented letters, numbers or _.".into(),
            _ => motivo,
        });
    }

    fn pedido_criacao(&self, armas: &[u16]) -> Result<ClientMessage, &'static str> {
        let nome = valida_nome(&self.nome)?;
        let arma = self
            .arma
            .filter(|&id| conjunto_disponivel(id, armas))
            .ok_or("Choose an available weapon.")?;
        Ok(ClientMessage::CreateCharacter {
            name: nome.into(),
            aparencia: self.aparencia,
            starting_weapon: arma,
            faction: self.faccao,
        })
    }

    pub fn desenha(
        &mut self,
        chars: &[CharacterListEntry],
        armas: &[u16],
        selecionado: &mut usize,
        digitado: &[char],
        vox: &VoxCache,
        solido: &Material,
    ) -> Option<Acao> {
        let w = largura_tela();
        let h = altura_tela();
        self.camera_ui();
        if self.saida_previa.is_none() {
            // Mesmo ceu do jogo: o que sobrar de vao no horizonte le' como ceu,
            // e nao como buraco escuro na malha.
            render3d::define_noite(false);
            render3d::clear();
            self.desenha_mundo(solido);
            self.camera_ui();
            // So' uma faixa atras do titulo: veu na tela inteira apagava a ilha
            // (so' aparecia desde que o mundo passou a sair antes do 2D).
            ui::ret_gradiente(
                Rect::new(0.0, 0.0, w, (h * 0.22).max(120.0)),
                1.0,
                Color::new(0.012, 0.018, 0.030, 0.70),
                Color::new(0.012, 0.018, 0.030, 0.0),
            );
        } else {
            clear_background(Color::new(0.026, 0.041, 0.063, 1.0));
        }
        for i in 0..20 {
            let x = (i as f32 * 157.3 + get_time() as f32 * (3.0 + (i % 3) as f32)) % w;
            let y = (i as f32 * 89.7) % h;
            draw_circle(x, y, 1.0, Color::new(0.85, 0.73, 0.47, 0.18));
        }
        let titulo = if self.criando {
            "Write your own story"
        } else {
            "Choose your character"
        };
        let seguro = crate::nativo::area_segura();
        let compacto = self.criando && h < ALTURA_COMPACTA;
        if compacto {
            ui::texto(16.0 + seguro[1], seguro[0] + 31.0, titulo, 22, ui::TEXTO);
        } else {
            ui::texto(32.0, 35.0, "T E M P E S T", 14, ui::OURO);
            ui::texto(32.0, 77.0, titulo, 32, ui::TEXTO);
            ui::texto(
                32.0,
                103.0,
                if self.criando {
                    "One weapon. One fresh start."
                } else {
                    "Your next chapter is waiting."
                },
                15,
                ui::SUAVE,
            );
        }
        let ocupado = self.aguardando.is_some();
        let voltar = if compacto {
            Rect::new(w - 16.0 - seguro[3] - 110.0, seguro[0] + 8.0, 110.0, 32.0)
        } else {
            Rect::new(w - 150.0, 30.0, 118.0, 34.0)
        };
        if botao(
            voltar,
            if self.criando { "Cancel" } else { "Back" },
            !ocupado,
            false,
        ) || (!ocupado && is_key_pressed(KeyCode::Escape))
        {
            self.mensagem = None;
            self.foco_nome = false;
            if self.criando {
                self.criando = false;
            } else {
                return Some(Acao::Voltar);
            }
        }
        if self
            .aguardando
            .as_ref()
            .is_some_and(|(_, t)| get_time() - t > 15.0)
        {
            self.mensagem = Some("Waiting for confirmation from the server…".into());
        }
        if self.criando {
            self.desenha_criacao(armas, digitado, vox, solido)
        } else {
            self.desenha_selecao(chars, selecionado, vox, solido)
        }
    }

    fn desenha_criacao(
        &mut self,
        armas: &[u16],
        digitado: &[char],
        vox: &VoxCache,
        solido: &Material,
    ) -> Option<Acao> {
        let w = largura_tela();
        let h = altura_tela();
        let l = LayoutCriacao::novo(w, h, crate::nativo::area_segura());
        let ocupado = self.aguardando.is_some();
        let conjunto = Conjunto::da_arma(self.arma.unwrap_or(0));

        // ── esquerda: o personagem, e embaixo dele o nome e o botao ──────────
        let visual = (self.aparencia.empacota(), 0, 0);
        self.desenha_retrato(l.retrato, l.rodape_retrato, conjunto, visual, vox, solido);
        let cx = l.retrato.x + l.retrato.w * 0.5;
        ui::texto_centro(
            cx,
            l.retrato.y + l.retrato.h - (l.rodape_retrato - 20.0) * 0.5 - 4.0,
            conjunto.nome(),
            if l.compacto { 18 } else { 24 },
            ui::TEXTO,
        );
        if !l.compacto {
            ui::texto_centro(
                cx,
                l.retrato.y + l.retrato.h - 6.0,
                "Drag the character to rotate",
                13,
                ui::SUAVE,
            );
        }

        // The name and the Create button live on the Appearance tab: the
        // weapon tab only leads there, so nobody creates a character without
        // having seen the look.
        if self.aba != 1 {
            self.foco_nome = false;
            ui::texto_centro(
                cx,
                l.nome.y + l.nome.h * 0.5 + 6.0,
                "Look and name come next",
                14,
                ui::SUAVE,
            );
            if botao(l.botao, "Next: Appearance", !ocupado && self.arma.is_some(), true) {
                self.aba = 1;
            }
        } else {
            // Teclado da tela aberto: ele cobre a metade de baixo, onde o campo mora.
            // O campo sobe pra logo acima dele, sobre o personagem.
            let teclado = crate::nativo::TECLADO_NA_TELA && self.foco_no_nome();
            let dy = l.subida_do_nome(teclado, h);
            let campo = Rect::new(l.nome.x, l.nome.y - dy, l.nome.w, l.nome.h);
            if dy > 0.0 {
                draw_rectangle(
                    l.retrato.x - 8.0,
                    campo.y - 28.0,
                    l.retrato.w + 16.0,
                    campo.h + 40.0,
                    Color::new(0.012, 0.018, 0.030, 0.94),
                );
                ui::texto(campo.x, campo.y - 10.0, "CHARACTER NAME", 12, ui::OURO);
            } else if !l.compacto {
                ui::texto(campo.x, campo.y - 8.0, "NAME", 12, ui::OURO);
            }
            if !ocupado {
                if let Some(p) = apertou_em() {
                    self.foco_nome = campo.contains(p);
                }
            }
            if !ocupado && self.foco_nome {
                for &c in digitado {
                    if c as u32 == 8 {
                        self.nome.pop();
                    } else if !c.is_control() && self.nome.chars().count() < 24 {
                        self.nome.push(c);
                    }
                }
                if is_key_pressed(KeyCode::Backspace) && !digitado.contains(&'\u{8}') {
                    self.nome.pop();
                }
            }
            draw_rectangle(campo.x, campo.y, campo.w, campo.h, ui::FUNDO);
            draw_rectangle_lines(
                campo.x,
                campo.y,
                campo.w,
                campo.h,
                if self.foco_nome { 2.0 } else { 1.0 },
                if self.foco_nome { ui::OURO } else { ui::BORDA },
            );
            let cursor = if self.foco_nome && (get_time() * 2.0) as u32 % 2 == 0 {
                "|"
            } else {
                ""
            };
            let meio = campo.y + campo.h * 0.5 + 6.0;
            if self.nome.is_empty() && !self.foco_nome {
                ui::texto_centro(
                    campo.x + campo.w * 0.5,
                    meio,
                    "Tap to choose the name",
                    16,
                    ui::SUAVE,
                );
            } else {
                ui::texto_centro(
                    campo.x + campo.w * 0.5,
                    meio,
                    &format!("{}{cursor}", self.nome),
                    18,
                    ui::TEXTO,
                );
            }

            let erro = if self.nome.is_empty() {
                None
            } else {
                valida_nome(&self.nome).err()
            };
            let msg = self
                .mensagem
                .as_deref()
                .or(erro)
                .or(if self.arma.is_none() {
                    Some("Nenhuma arma inicial disponível neste servidor.")
                } else {
                    None
                })
                .or(if self.foco_nome {
                    Some("2 to 24 characters · letters, numbers and _")
                } else {
                    None
                });
            if let Some(msg) = msg {
                let cor = if erro.is_some() || self.mensagem.is_some() {
                    ui::OURO
                } else {
                    ui::SUAVE
                };
                ui::texto_centro(
                    cx,
                    campo.y
                        - if dy > 0.0 {
                            30.0
                        } else if l.compacto {
                            7.0
                        } else {
                            26.0
                        },
                    msg,
                    13,
                    cor,
                );
            }

            let pode = !ocupado && self.pedido_criacao(armas).is_ok();
            let rotulo = if ocupado {
                "Creating the character…"
            } else if self.nome.trim().is_empty() {
                "Choose a name"
            } else {
                "Create character"
            };
            if dy == 0.0 {
                if botao(l.botao, rotulo, pode, true) {
                    if let Some(a) = self.envia_criacao(armas) {
                        return Some(a);
                    }
                } else if !ocupado && !pode && clicou(l.botao) {
                    // Botao apagado por falta de nome: leva direto pro campo.
                    self.foco_nome = true;
                }
            }
            if pode && self.foco_nome && is_key_pressed(KeyCode::Enter) {
                if let Some(a) = self.envia_criacao(armas) {
                    return Some(a);
                }
            }
        }

        // ── direita: arma e faccao ───────────────────────────────────────────
        let painel = l.painel;
        ui::painel(painel);
        let x = painel.x + 14.0;
        let lw = painel.w - 28.0;
        let card_h = l.card_h;
        // ── as duas abas ──
        for (i, nome) in ["Weapon & Faction", "Appearance"].iter().enumerate() {
            let r = Rect::new(
                x + i as f32 * (lw + 8.0) * 0.5,
                painel.y + 8.0,
                (lw - 8.0) * 0.5,
                l.aba_h,
            );
            if botao(r, nome, !ocupado, self.aba == i as u8) {
                self.aba = i as u8;
            }
        }
        if self.aba == 1 {
            self.desenha_aparencia(x, painel.y + l.aba_h + 22.0, lw, l.faccao_h, ocupado);
            return None;
        }
        ui::texto(x, painel.y + l.aba_h + 30.0, "STARTING WEAPON", 13, ui::OURO);
        for (i, c) in Conjunto::TODOS.iter().enumerate() {
            let r = Rect::new(
                x + (i % 2) as f32 * (lw + 8.0) * 0.5,
                painel.y + l.aba_h + 40.0 + (i / 2) as f32 * (card_h + 6.0),
                (lw - 8.0) * 0.5,
                card_h,
            );
            let disponivel = armas.contains(&c.arma());
            let ativo = self.arma == Some(c.arma());
            let cor = ui::cor_skill(*c as u32 * 3 + 1);
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                if ativo {
                    Color::new(cor.r * 0.11, cor.g * 0.11, cor.b * 0.11, 1.0)
                } else {
                    ui::FUNDO
                },
            );
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                if ativo { 2.0 } else { 1.0 },
                if ativo { cor } else { ui::BORDA },
            );
            let icone = if l.compacto { 28.0 } else { 34.0 };
            let ix = r.x + 8.0 + icone * 0.5;
            if !crate::icones_ui::skill(
                *c as u32 * 3 + 1,
                vec2(ix, r.y + r.h * 0.5),
                icone,
                if disponivel { 1.0 } else { 0.35 },
            ) {
                ui::icone(
                    *c as u32 * 3 + 1,
                    vec2(ix, r.y + r.h * 0.5),
                    icone * 0.5,
                    if disponivel { cor } else { ui::SUAVE },
                );
            }
            let tx = r.x + 16.0 + icone;
            ui::texto_ajustado(
                c.nome(),
                tx,
                r.y + r.h * 0.5 - 2.0,
                r.w - tx + r.x - 6.0,
                if l.compacto { 14 } else { 15 },
                if disponivel { ui::TEXTO } else { ui::SUAVE },
            );
            if disponivel {
                let fim_estrelas = estrelas(tx, r.y + r.h * 0.5 + 10.0, 5.5, dificuldade(*c));
                ui::texto_ajustado(
                    estilo(*c).0,
                    fim_estrelas + 6.0,
                    r.y + r.h * 0.5 + 14.0,
                    r.w - fim_estrelas + r.x - 12.0,
                    11,
                    ui::SUAVE,
                );
            } else {
                ui::texto_ajustado(
                    "Unavailable",
                    tx,
                    r.y + r.h * 0.5 + 14.0,
                    r.w - tx + r.x - 6.0,
                    11,
                    ui::SUAVE,
                );
            }
            if !ocupado && disponivel && clicou(r) {
                self.arma = Some(c.arma());
                self.mensagem = None;
                self.giro = 0.0;
            }
        }
        let fy = painel.y + l.aba_h + 40.0 + 2.0 * (card_h + 6.0) + 14.0;
        ui::texto(x, fy, "FACTION", 13, ui::OURO);
        for (i, (f, n)) in [
            (Faction::Peacemain, "Peacemain"),
            (Faction::Morganeers, "Morganeers"),
        ]
        .iter()
        .enumerate()
        {
            let r = Rect::new(
                x + i as f32 * (lw + 8.0) * 0.5,
                fy + 8.0,
                (lw - 8.0) * 0.5,
                l.faccao_h,
            );
            if botao(r, n, !ocupado, self.faccao == *f) {
                self.faccao = *f;
            }
        }
        let mut y = fy + 8.0 + l.faccao_h + 18.0;
        ui::texto(
            x,
            y,
            if self.faccao == Faction::Peacemain {
                "Adventurers and explorers."
            } else {
                "Pirates after plunder."
            },
            13,
            ui::SUAVE,
        );
        y += 22.0;
        let n = dificuldade(conjunto);
        ui::texto(x, y, "DIFFICULTY", 12, ui::OURO);
        let ex = estrelas(x + ui::medir("DIFFICULTY", 12) + 10.0, y - 4.0, 6.5, n);
        ui::texto_ajustado(["", "Easy", "Medium", "Hard"][n as usize], ex + 8.0, y, x + lw - ex - 8.0, 13, ui::SUAVE);
        y += 22.0;
        let fim = painel.y + painel.h - 10.0;
        if y + 14.0 < fim {
            y = texto_linhas_ate(estilo(conjunto).1, x, y, lw, 13, ui::TEXTO, fim) + 24.0;
        }
        // Skills iniciais so' quando sobra altura (desktop).
        if y + 44.0 < fim {
            let skills = shared::skills::playtest();
            for (i, s) in skills.iter().filter(|s| s.conjunto == conjunto).enumerate() {
                let sx = x + i as f32 * lw / 3.0;
                if !crate::icones_ui::skill(s.id, vec2(sx + 14.0, y), 24.0, 1.0) {
                    ui::icone(s.id, vec2(sx + 14.0, y), 11.0, ui::cor_skill(s.id));
                }
                ui::texto(
                    sx + 31.0,
                    y + 4.0,
                    &format!("Lv {}", s.nivel_necessario()),
                    12,
                    ui::OURO,
                );
                ui::texto_ajustado(&s.nome, sx, y + 24.0, lw / 3.0 - 8.0, 12, ui::TEXTO);
            }
        }
        None
    }

    fn envia_criacao(&mut self, armas: &[u16]) -> Option<Acao> {
        let pedido = self.pedido_criacao(armas).ok()?;
        self.aguardando = Some((self.nome.trim().into(), get_time()));
        self.mensagem = None;
        self.foco_nome = false;
        Some(Acao::Enviar(pedido))
    }

    fn desenha_selecao(
        &mut self,
        chars: &[CharacterListEntry],
        selecionado: &mut usize,
        vox: &VoxCache,
        solido: &Material,
    ) -> Option<Acao> {
        let w = largura_tela();
        let h = altura_tela();
        let r = Rect::new(28.0, 136.0, (w * 0.34).clamp(280.0, 370.0), h - 220.0);
        ui::painel(r);
        ui::texto(
            r.x + 18.0,
            r.y + 28.0,
            &format!("YOUR CHARACTERS  ·  {}", chars.len()),
            13,
            ui::OURO,
        );
        // Os cartoes rolam arrastando (dedo), pela roda ou pela barra.
        let area = Rect::new(r.x + 12.0, r.y + 45.0, r.w - 24.0, r.h - 115.0);
        let total = chars.len() as f32 * CARTAO;
        let clique = self.rolagem.quadro(area, total, CARTAO);
        if !chars.is_empty() {
            if is_key_pressed(KeyCode::Down) {
                *selecionado = (*selecionado + 1).min(chars.len() - 1);
            }
            if is_key_pressed(KeyCode::Up) {
                *selecionado = selecionado.saturating_sub(1);
            }
            if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Up) {
                // O escolhido pelas setas fica a' vista.
                let y = *selecionado as f32 * CARTAO;
                if y < self.rolagem.pos {
                    self.rolagem.pos = y;
                } else if y + CARTAO > self.rolagem.pos + area.h {
                    self.rolagem.pos = y + CARTAO - area.h;
                }
            }
        }
        self.rolagem.pos = self
            .rolagem
            .pos
            .clamp(0.0, crate::rolagem::maximo(area, total));
        crate::rolagem::recortar(Some(area));
        for (i, c) in chars.iter().enumerate() {
            let card = Rect::new(
                area.x,
                area.y + i as f32 * CARTAO - self.rolagem.pos,
                area.w - 12.0,
                CARTAO - 10.0,
            );
            if card.y + card.h < area.y || card.y > area.y + area.h {
                continue;
            }
            let conjunto = Conjunto::da_arma(c.weapon_id.unwrap_or(0));
            let cor = ui::cor_skill(conjunto as u32 * 3 + 1);
            draw_rectangle(
                card.x,
                card.y,
                card.w,
                card.h,
                if i == *selecionado {
                    Color::new(0.10, 0.13, 0.17, 1.0)
                } else {
                    ui::FUNDO
                },
            );
            draw_rectangle_lines(
                card.x,
                card.y,
                card.w,
                card.h,
                1.0,
                if i == *selecionado {
                    ui::OURO
                } else {
                    ui::BORDA
                },
            );
            if !crate::icones_ui::skill(
                conjunto as u32 * 3 + 1,
                vec2(card.x + 27.0, card.y + 37.0),
                34.0,
                1.0,
            ) {
                ui::icone(
                    conjunto as u32 * 3 + 1,
                    vec2(card.x + 27.0, card.y + 37.0),
                    18.0,
                    cor,
                );
            }
            ui::texto_ajustado(
                &c.name,
                card.x + 54.0,
                card.y + 29.0,
                card.w - 62.0,
                19,
                ui::TEXTO,
            );
            ui::texto(
                card.x + 54.0,
                card.y + 53.0,
                &format!("Lv {}  ·  {}", c.level, conjunto.nome()),
                12,
                ui::SUAVE,
            );
            if clique.is_some_and(|p| card.contains(p) && area.contains(p)) {
                *selecionado = i;
                self.giro = 0.0;
                self.mensagem = None;
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(area, total);
        if chars.is_empty() {
            texto_linhas(
                "Sua história começa aqui. Crie seu primeiro personagem.",
                r.x + 18.0,
                r.y + 85.0,
                r.w - 36.0,
                18,
                ui::SUAVE,
            );
        }
        if total > area.h {
            ui::texto(
                r.x + 18.0,
                r.y + r.h - 62.0,
                "Drag to see more characters",
                12,
                ui::SUAVE,
            );
        }
        if botao(
            Rect::new(r.x + 12.0, r.y + r.h - 50.0, r.w - 24.0, 38.0),
            "+  New character",
            true,
            false,
        ) {
            self.criando = true;
            self.nome.clear();
            self.mensagem = None;
            self.foco_nome = true;
            self.giro = 0.0;
        }
        if let Some(c) = chars.get(*selecionado) {
            let conjunto = Conjunto::da_arma(c.weapon_id.unwrap_or(0));
            let hero = Rect::new(r.x + r.w + 20.0, 128.0, w - r.x - r.w - 48.0, h - 218.0);
            // THIS character's look, skins and auras — it drew the creation
            // screen's appearance for every character on the list.
            self.desenha_retrato(hero, 84.0, conjunto, (c.aparencia, c.skins, c.auras), vox, solido);
            let cx = hero.x + hero.w * 0.5;
            ui::texto_centro(cx, hero.y + hero.h - 57.0, &c.name, 30, ui::TEXTO);
            ui::texto_centro(
                cx,
                hero.y + hero.h - 29.0,
                &format!("Level {}  ·  {}", c.level, conjunto.nome()),
                16,
                ui::OURO,
            );
            ui::texto_centro(
                cx,
                hero.y + hero.h - 7.0,
                nome_faccao(c.faction),
                13,
                ui::SUAVE,
            );
            if botao(
                Rect::new(hero.x, h - 66.0, hero.w, 42.0),
                "Enter the world",
                true,
                true,
            ) || is_key_pressed(KeyCode::Enter)
            {
                return Some(Acao::Enviar(ClientMessage::SelectCharacter {
                    name: c.name.clone(),
                }));
            }
        }
        if let Some(msg) = &self.mensagem {
            texto_linhas(msg, 32.0, h - 58.0, r.w, 14, ui::OURO);
        }
        None
    }

    /// A ilha de verdade atras da interface, girando devagar em volta da cidade.
    ///
    /// Na TELA, nao num render target: no iPhone area 3D em render target
    /// saia vazia (sem profundidade). O retrato do boneco vem depois e limpa
    /// so' a profundidade, entao fica por cima sem brigar com o chao. So' e'
    /// chamado fora do modo de exportacao, que e' render target.
    fn desenha_mundo(&mut self, solido: &Material) {
        if !self.fundo_tentado {
            self.fundo_tentado = true;
            self.fundo = Fundo::novo();
        }
        let Some(f) = self.fundo.as_mut() else { return };
        let t = get_time() as f32;
        // A vila e' assada numa thread; sem recolher aqui as malhas nunca ficam
        // prontas e nenhuma casa aparece (o jogo faz isso todo quadro, em main).
        f.casas.acompanhar();
        // O chao chega aos poucos, como no jogo: orcamento por quadro. Raio 11
        // (176 unidades) cobre o que esta camera enxerga.
        f.terreno.atualiza(f.centro, 11, 4);
        let giro = t * 0.04;
        let chao = f.terreno.altura(f.centro.x, f.centro.y);
        // ~33 graus e LONGE da vila. Perto demais, a casa no caminho da orbita
        // entrava no quadro como um telhado gigante cortado atras do boneco; de
        // mais longe as casas ficam do tamanho de cenario.
        let cam = Camera3D {
            position: vec3(
                f.centro.x + giro.sin() * 70.0,
                chao + 46.0,
                f.centro.y + giro.cos() * 70.0,
            ),
            target: vec3(f.centro.x, chao + 3.0, f.centro.y),
            up: Vec3::Y,
            fovy: 45f32.to_radians(),
            ..Default::default()
        };
        set_camera(&cam);
        gl_use_material(solido);
        solido.set_uniform("Crop", Vec3::ZERO);
        solido.set_uniform("RecorteZ", 0.0f32);
        f.terreno.desenha(&cam, Vec3::ZERO, 0.0);
        f.casas.desenha(&cam, None, Vec3::ZERO, 0.0);
        crate::agua::desenha(&f.terreno, &cam, t);
        gl_use_default_material();
    }

    fn camera_ui(&self) {
        if let Some(rt) = &self.saida_previa {
            let mut c =
                Camera2D::from_display_rect(Rect::new(0.0, 0.0, largura_tela(), altura_tela()));
            c.render_target = Some(rt.clone());
            set_camera(&c);
        } else {
            set_default_camera();
        }
    }

    /// A previa 3D desenhada DIRETO na tela, num viewport do tamanho do
    /// retrato — sem render target.
    ///
    /// Antes ela ia pra uma textura com profundidade (`render_target_ex(..,
    /// depth: true)`). A miniquad 0.4.11 cria essa profundidade com o formato
    /// sem tamanho `GL_DEPTH_COMPONENT` (graphics/gl.rs), que o GL de desktop
    /// aceita e o OpenGL ES do iPhone recusa: o framebuffer fica incompleto e
    /// a area sai vazia nas duas telas (lista e criacao). O mundo aparecia
    /// porque desenha na tela, cuja view tem profundidade de 24 bits. Mesmo
    /// caminho aqui, igual em todas as plataformas.
    fn desenha_retrato(
        &mut self,
        r: Rect,
        rodape: f32,
        conjunto: Conjunto,
        // (packed appearance, skins, auras)
        visual: (u32, u64, u64),
        vox: &VoxCache,
        solido: &Material,
    ) {
        let area = Rect::new(r.x, r.y, r.w.max(1.0), (r.h - rodape).max(1.0));
        // Sem caixa atras do boneco: ele fica de pe' direto na frente da ilha.
        // (Um retangulo escuro translucido aqui escurecia o fundo e ficava feio.)
        let mouse = Vec2::from(mouse_position());
        if is_mouse_button_down(MouseButton::Left) && r.contains(mouse) {
            if let Some(antes) = self.mouse_anterior {
                self.giro += (mouse.x - antes.x) * 0.012;
            }
            self.mouse_anterior = Some(mouse);
        } else {
            self.mouse_anterior = None;
        }
        let Some(corpo) = vox.rig(render3d::RIG_CORPO) else {
            // Sem o corpo nao ha' o que girar — mas sumir com a area inteira
            // escondia a causa (no iPhone nenhum .vox carregava). Fundo, aviso
            // na tela e uma linha de log.
            static AVISOU: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !AVISOU.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "[preview] rig {} did not load — see the [vox] missing lines",
                    render3d::RIG_CORPO
                );
            }
            ui::texto_centro(
                area.x + area.w * 0.5,
                area.y + area.h * 0.5,
                "Character model unavailable",
                14,
                ui::SUAVE,
            );
            ui::texto_centro(
                area.x + area.w * 0.5,
                area.y + area.h * 0.5 + 18.0,
                "vox faltando: personagem/corpo.vox",
                12,
                ui::SUAVE,
            );
            return;
        };
        // Tela exportada (MMO_PREVIA_EXPORTAR) desenha num alvo do tamanho da
        // tela em pontos; o resto, na tela em pixels.
        let (escala, alto_px) = match &self.saida_previa {
            Some(t) => (1.0, t.texture.height()),
            None => {
                let s = macroquad::miniquad::window::dpi_scale();
                (s, altura_tela() * s)
            }
        };
        let Some(vp) = render3d::viewport_em_pixels(area, escala, alto_px) else {
            static AVISOU_VP: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !AVISOU_VP.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "[preview] empty viewport: area {area:?} scale {escala} height {alto_px}"
                );
            }
            ui::texto_centro(
                area.x + area.w * 0.5,
                area.y + area.h * 0.5,
                "No room for the preview",
                13,
                ui::SUAVE,
            );
            return;
        };
        let aspecto = vp.2 as f32 / vp.3 as f32;
        let distancia = 4.7 * (0.62 / aspecto).max(1.0);
        let cam = Camera3D {
            position: vec3(0.0, 1.35, distancia),
            target: vec3(0.0, 0.88, 0.0),
            up: Vec3::Y,
            fovy: 34f32.to_radians(),
            aspect: Some(aspecto),
            viewport: Some(vp),
            render_target: self.saida_previa.clone(),
            ..Default::default()
        };
        set_camera(&cam);
        // O 2D do painel ja' foi pro framebuffer no `set_camera`; limpa so' a
        // PROFUNDIDADE (a cor fica) pro personagem nao brigar com o que houver.
        render3d::limpa_so_profundidade();
        draw_cylinder(
            vec3(0.0, -0.07, 0.0),
            0.90,
            0.93,
            0.06,
            None,
            Color::new(0.09, 0.12, 0.16, 1.0),
        );
        for i in 0..64 {
            let a = i as f32 * std::f32::consts::TAU / 64.0;
            let b = (i + 1) as f32 * std::f32::consts::TAU / 64.0;
            draw_line_3d(
                vec3(a.cos() * 0.88, -0.008, a.sin() * 0.88),
                vec3(b.cos() * 0.88, -0.008, b.sin() * 0.88),
                ui::OURO,
            );
        }
        gl_use_material(solido);
        solido.set_uniform("Crop", Vec3::ZERO);
        let entrada = crate::rig::Entrada {
            fase: 0.0,
            andar: 0.0,
            correr: 0.0,
            tempo: get_time() as f32,
            ar: 0.0,
            degrau: [0.0, 0.0],
            combate: crate::rig::Combate {
                conjunto: conjunto as u8,
                sacada: 1.0,
                ..Default::default()
            },
        };
        let pose = crate::rig::pose(&entrada);
        let base =
            Mat4::from_rotation_y(self.giro + 0.18 + (get_time() as f32 * 0.35).sin() * 0.10);
        // O retrato mostra a APARÊNCIA escolhida: é o que faz os seletores
        // ao lado significarem alguma coisa.
        let mut veste = render3d::vestimenta_de(vox, visual.0)
            .unwrap_or_else(|| render3d::Vestimenta::nua(corpo));
        veste.skins = visual.1;
        render3d::desenha_rig_com_auras(base, &pose, &veste, vox, visual.2);
        gl_use_default_material();
        self.camera_ui();
    }
}

fn nome_faccao(f: Faction) -> &'static str {
    match f {
        Faction::Peacemain => "Peacemain",
        Faction::Morganeers => "Morganeers",
    }
}
/// How hard the class is to play, in stars (1-3): the owner's call. The
/// tanky melee and the healer forgive mistakes; the katana wants timing; the
/// pistols want spacing on top of it.
fn dificuldade(c: Conjunto) -> u8 {
    match c {
        Conjunto::EspadaEscudo | Conjunto::AnelMagico => 1,
        Conjunto::Katana => 2,
        Conjunto::Pistolas => 3,
    }
}

/// Three stars from `x`, centred on `y`, the first `cheias` lit. Returns
/// where they end.
fn estrelas(x: f32, y: f32, raio: f32, cheias: u8) -> f32 {
    for i in 0..3u8 {
        let c = vec2(x + raio + i as f32 * raio * 2.3, y);
        let cor = if i < cheias { ui::OURO } else { Color::new(0.35, 0.33, 0.30, 1.0) };
        let p: Vec<Vec2> = (0..10)
            .map(|k| {
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
                let r = if k % 2 == 0 { raio } else { raio * 0.45 };
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        for k in 0..10 {
            draw_triangle(c, p[k], p[(k + 1) % 10], cor);
        }
    }
    x + raio * 2.0 + 2.0 * raio * 2.3
}

fn estilo(c: Conjunto) -> (&'static str, &'static str) {
    match c {
        Conjunto::EspadaEscudo => (
            "Defence and pressure",
            "Lute na linha de frente com investidas, cortes amplos e uma barreira protetora.",
        ),
        Conjunto::Katana => (
            "Opens and closes the fight",
            "Empunhe com as duas mãos: o saque contra quem ainda não te viu dá 2,5x de dano, e o golpe em alvo abaixo de 30% de vida dá 1,6x. Em troca, o dano contínuo é menor que o das pistolas, e a vida só volta na janela da Dança.",
        ),
        Conjunto::Pistolas => (
            "Ranged combat",
            "Mantenha distância com tiros precisos, rajadas e um barril explosivo.",
        ),
        Conjunto::AnelMagico => (
            "Magic and support",
            "Restaure sua vida, cure aliados próximos e invoque um impacto mágico sobre o alvo.",
        ),
    }
}
/// Onde o dedo (ou o mouse) apertou NESTE quadro.
///
/// No iPhone o "mouse" e' simulado a partir do toque, e no quadro em que o dedo
/// encosta ele ainda aponta pro toque ANTERIOR. Testar o clique contra ele
/// errava o alvo: o toque no campo de nome nao dava foco, o nome ficava vazio,
/// e com nome vazio o "Create character" ficava desabilitado pra sempre — o
/// botao parecia quebrado. E' o mesmo defeito que ja' tinha sido corrigido no
/// HUD (ver `ui_pega_em` em main.rs), so' que esta tela desenha antes dele e
/// ficou de fora. Por isso o toque real vem primeiro; sem dedo, o mouse de
/// sempre (PC igual).
fn apertou_em() -> Option<Vec2> {
    if let Some(t) = touches()
        .into_iter()
        .find(|t| t.phase == TouchPhase::Started)
    {
        return Some(t.position);
    }
    crate::foco::clique().then(|| Vec2::from(mouse_position()))
}
fn clicou(r: Rect) -> bool {
    apertou_em().is_some_and(|p| r.contains(p))
}
impl Personagens {
    /// Os quatro seletores de aparência: rosto, cabelo, cor e pele.
    ///
    /// Cada um é `‹ nome ›` — a forma mais barata que existe de escolher
    /// numa lista curta com o dedo, e que não ocupa a tela com uma grade.
    /// Devolve o y em que terminou.
    fn desenha_aparencia(&mut self, x: f32, y: f32, lw: f32, alt: f32, ocupado: bool) -> f32 {
        use shared::aparencia as ap;
        ui::texto(x, y, "APPEARANCE", 13, ui::OURO);
        let mut y = y + 8.0;
        // (rótulo, quantas opções, índice atual)
        // Hair + "no hair" + the FREE hats; the paid ones are for the shop.
        // The free hats are not contiguous, so the row walks a list.
        let chapeus = ap::chapeus_gratis();
        let cabelos = ap::CABELOS + 1 + chapeus.len() as u8;
        let atual_cabelo = match ap::chapeu_do_cabelo(self.aparencia.cabelo) {
            Some(c) => chapeus
                .iter()
                .position(|x| *x == c)
                .map_or(0, |i| ap::CABELOS + 1 + i as u8),
            None => self.aparencia.cabelo,
        };
        // Outfit: the default plus the free ones.
        let gratis = ap::roupas_gratis();
        let roupas = gratis.len() as u8 + 1;
        let atual_roupa = gratis
            .iter()
            .position(|r| *r == self.aparencia.roupa)
            .map_or(0, |i| i as u8 + 1);
        let linhas: [(&str, u8, u8); 5] = [
            ("Face", ap::ROSTOS, self.aparencia.rosto),
            ("Hair", cabelos, atual_cabelo),
            (
                "Cor",
                ap::CORES_DE_CABELO.len() as u8,
                self.aparencia.cor_cabelo,
            ),
            ("Skin tone", ap::TONS_DE_PELE.len() as u8, self.aparencia.pele),
            ("Outfit", roupas, atual_roupa),
        ];
        // A mesma constante que dimensiona o painel (`altura_da_aparencia`).
        debug_assert_eq!(linhas.len() as f32, LINHAS_DE_APARENCIA);
        let mut escolhas = [0u8; 5];
        for (i, (rotulo, n, atual)) in linhas.iter().enumerate() {
            let r = Rect::new(x, y, lw, alt);
            let lado = alt.min(40.0);
            let mut v = *atual;
            if botao(Rect::new(r.x, r.y, lado, r.h), "‹", !ocupado, false) {
                v = (v + n - 1) % n;
            }
            if botao(
                Rect::new(r.x + r.w - lado, r.y, lado, r.h),
                "›",
                !ocupado,
                false,
            ) {
                v = (v + 1) % n;
            }
            let nome = match i {
                1 if *atual == ap::CABELOS => "no hair".to_string(),
                1 if *atual > ap::CABELOS => chapeus
                    .get((*atual - ap::CABELOS - 1) as usize)
                    .and_then(|c| ap::nome_da_skin(*c))
                    .unwrap_or("no hair")
                    .to_string(),
                2 => ap::CORES_DE_CABELO[(*atual as usize).min(5)].to_string(),
                3 => ap::TONS_DE_PELE[(*atual as usize).min(3)].to_string(),
                4 if *atual == 0 => "default".to_string(),
                4 => gratis
                    .get(*atual as usize - 1)
                    .and_then(|r| ap::nome_da_skin(*r))
                    .unwrap_or("default")
                    .to_string(),
                _ => format!("{} {}", rotulo, atual + 1),
            };
            ui::texto_centro(r.x + r.w * 0.5, r.y + r.h * 0.5 + 5.0, &nome, 14, ui::TEXTO);
            escolhas[i] = v;
            y += alt + 6.0;
        }
        self.aparencia.rosto = escolhas[0];
        self.aparencia.cabelo = match escolhas[1] {
            k if k > ap::CABELOS => chapeus
                .get((k - ap::CABELOS - 1) as usize)
                .and_then(|c| ap::cabelo_do_chapeu(*c))
                .unwrap_or(0),
            k => k,
        };
        self.aparencia.cor_cabelo = escolhas[2];
        self.aparencia.pele = escolhas[3];
        self.aparencia.roupa = match escolhas[4] {
            0 => 0,
            k => gratis[k as usize - 1],
        };
        y
    }
}

fn botao(r: Rect, t: &str, ativo: bool, destaque: bool) -> bool {
    let sobre = ativo && r.contains(Vec2::from(mouse_position()));
    draw_rectangle(
        r.x,
        r.y,
        r.w,
        r.h,
        if destaque && ativo {
            Color::new(0.24, 0.21, 0.14, 1.0)
        } else {
            ui::FUNDO
        },
    );
    draw_rectangle_lines(
        r.x,
        r.y,
        r.w,
        r.h,
        1.0,
        if ativo && (destaque || sobre) {
            ui::OURO
        } else {
            ui::BORDA
        },
    );
    ui::texto_centro(
        r.x + r.w * 0.5,
        r.y + r.h * 0.5 + 6.0,
        t,
        16,
        if ativo { ui::TEXTO } else { ui::SUAVE },
    );
    ativo && clicou(r)
}
/// Tamanho da tela desta tela. Na previa exportada (`MMO_PREVIA_TAM=844x390`)
/// e' o tamanho pedido, pra conferir o layout do celular num PC cuja janela o
/// Hyprland ladrilha; no jogo, sempre a tela de verdade.
fn tamanho_forcado() -> Option<(f32, f32)> {
    static T: std::sync::OnceLock<Option<(f32, f32)>> = std::sync::OnceLock::new();
    *T.get_or_init(|| {
        std::env::var("MMO_PREVIA_EXPORTAR").ok()?;
        let v = std::env::var("MMO_PREVIA_TAM").ok()?;
        let (a, b) = v.split_once('x')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    })
}
fn largura_tela() -> f32 {
    tamanho_forcado().map_or_else(screen_width, |t| t.0)
}
fn altura_tela() -> f32 {
    tamanho_forcado().map_or_else(screen_height, |t| t.1)
}

/// Abaixo desta altura (celular deitado: 390-430 pontos) a criacao usa o
/// layout compacto: sem o cabecalho grande e com cartoes baixos.
const ALTURA_COMPACTA: f32 = 560.0;

/// Onde cada parte da criacao fica. Separado do desenho pra ser testado nos
/// tamanhos de tela de verdade: no iPhone deitado o campo de nome caia em
/// y=450 numa tela de 390 — fora dela — e sem nome o "Create character" nunca
/// acendia.
/// Quantos seletores o bloco de APARENCIA tem (rosto, cabelo, cor, pele,
/// roupa). Mora aqui porque DOIS lugares precisam concordar: quem desenha e
/// quem calcula a altura do painel. Quando eram dois numeros a mao, o painel
/// parou de cobrir as duas ultimas linhas.
const LINHAS_DE_APARENCIA: f32 = 5.0;

#[derive(Debug, Clone, Copy)]
struct LayoutCriacao {
    compacto: bool,
    /// Personagem 3D (a area inclui `rodape_retrato` embaixo, pro nome da arma).
    retrato: Rect,
    rodape_retrato: f32,
    /// Campo de nome e botao, centrados embaixo do personagem.
    nome: Rect,
    botao: Rect,
    /// Painel da direita: arma e faccao.
    painel: Rect,
    card_h: f32,
    faccao_h: f32,
    /// Altura dos botoes de aba, no topo do painel da direita.
    aba_h: f32,
}

impl LayoutCriacao {
    /// `seguro` = area segura (topo, esquerda, baixo, direita).
    fn novo(w: f32, h: f32, seguro: [f32; 4]) -> Self {
        let compacto = h < ALTURA_COMPACTA;
        let esq = (if compacto { 16.0 } else { 28.0 }) + seguro[1];
        let dir = w - (if compacto { 16.0 } else { 28.0 }) - seguro[3];
        let topo = seguro[0] + if compacto { 48.0 } else { 128.0 };
        let base = h - seguro[2] - if compacto { 10.0 } else { 24.0 };
        let largura = dir - esq;
        let col = (largura * if compacto { 0.44 } else { 0.48 }).min(620.0);
        let botao_h = if compacto { 40.0 } else { 46.0 };
        let nome_h = if compacto { 38.0 } else { 44.0 };
        let botao = Rect::new(esq, base - botao_h, col, botao_h);
        // Com mensagem (erro/dica) entre o personagem e o campo.
        let nome = Rect::new(esq, botao.y - 8.0 - nome_h, col, nome_h);
        let rodape = if compacto { 26.0 } else { 60.0 };
        let retrato = Rect::new(
            esq,
            topo,
            col,
            (nome.y - if compacto { 20.0 } else { 40.0 } - topo).max(1.0),
        );
        let px = esq + col + if compacto { 14.0 } else { 24.0 };
        let mut l = Self {
            compacto,
            retrato,
            rodape_retrato: rodape,
            nome,
            botao,
            painel: Rect::new(px, topo, dir - px, base - topo),
            card_h: if compacto { 46.0 } else { 74.0 },
            faccao_h: if compacto { 30.0 } else { 36.0 },
            aba_h: if compacto { 28.0 } else { 34.0 },
        };
        // Tela grande: o painel acaba onde acaba o conteudo, sem um vao vazio embaixo.
        if !compacto {
            l.painel.h = l.painel.h.min(l.fim_do_essencial() + 130.0 - topo);
        }
        l
    }

    /// Quanto o campo de nome sobe com o teclado da tela aberto: o bastante pra
    /// ficar acima dele, deixando 36 px em cima pro rotulo e a dica.
    fn subida_do_nome(&self, teclado: bool, h: f32) -> f32 {
        crate::teclado_virtual::deslocamento(
            teclado,
            h,
            self.nome.y - 28.0,
            self.nome.y + self.nome.h,
        )
    }

    /// Onde termina o que TEM que caber no painel: cartoes de arma, faccao,
    /// os seletores de APARENCIA e a legenda.
    ///
    /// A aparencia entrou aqui em 21/09/2026 e nao estava: o painel e'
    /// recortado por esta conta, entao as duas ultimas linhas (Pele e Roupa)
    /// ficavam DESENHADAS FORA do fundo. O numero de linhas vem de uma
    /// constante compartilhada com quem desenha — duas contagens a mao
    /// divergem no primeiro seletor novo.
    fn fim_do_essencial(&self) -> f32 {
        self.fim_da_faccao().max(self.fim_da_aparencia())
    }

    /// Onde a aba ARMA & FACCAO termina (cartoes + faccao + legenda).
    fn fim_da_faccao(&self) -> f32 {
        self.painel.y
            + self.aba_h
            + 40.0
            + 2.0 * (self.card_h + 6.0)
            + 14.0
            + 8.0
            + self.faccao_h
            + 18.0
            + 22.0
    }

    /// Onde a aba APARENCIA termina.
    fn fim_da_aparencia(&self) -> f32 {
        self.painel.y + self.aba_h + 22.0 + self.altura_da_aparencia()
    }

    /// Quanto o bloco de aparencia ocupa: o rotulo mais as linhas.
    fn altura_da_aparencia(&self) -> f32 {
        8.0 + LINHAS_DE_APARENCIA * (self.faccao_h + 6.0)
    }
}

/// Como `texto_linhas`, mas para antes de `limite` (y). Devolve o y da ultima linha.
fn texto_linhas_ate(t: &str, x: f32, y: f32, w: f32, tam: u16, cor: Color, limite: f32) -> f32 {
    // Traduz antes de quebrar: a quebra desenha PEDACOS, e pedaco nao casa com
    // verbete (ver `hud_estilo::desenha_texto`).
    let t = &shared::idioma::tr(t);
    let mut linha = String::new();
    let mut y = y;
    for palavra in t.split_whitespace() {
        let proxima = if linha.is_empty() {
            palavra.into()
        } else {
            format!("{linha} {palavra}")
        };
        if ui::medir(&proxima, tam) > w && !linha.is_empty() {
            if y + tam as f32 + 5.0 > limite {
                break;
            }
            ui::texto(x, y, &linha, tam, cor);
            y += tam as f32 + 5.0;
            linha = palavra.into();
        } else {
            linha = proxima;
        }
    }
    ui::texto(x, y, &linha, tam, cor);
    y
}
fn texto_linhas(t: &str, x: f32, y: f32, w: f32, tam: u16, cor: Color) {
    // Traduz antes de quebrar: a quebra desenha PEDACOS, e pedaco nao casa com
    // verbete (ver `hud_estilo::desenha_texto`).
    let t = &shared::idioma::tr(t);
    let mut linha = String::new();
    let mut y = y;
    for palavra in t.split_whitespace() {
        let proxima = if linha.is_empty() {
            palavra.into()
        } else {
            format!("{linha} {palavra}")
        };
        if ui::medir(&proxima, tam) > w && !linha.is_empty() {
            ui::texto(x, y, &linha, tam, cor);
            y += tam as f32 + 5.0;
            linha = palavra.into();
        } else {
            linha = proxima;
        }
    }
    ui::texto(x, y, &linha, tam, cor);
}

/// Previa sem login nem escritas no servidor. Exporta selecao e as quatro armas.
pub async fn previa(vox: &VoxCache) {
    let mut tela = Personagens::default();
    let chars = [
        ("brunji", 20, Conjunto::Katana),
        ("Mare", 5, Conjunto::Pistolas),
    ]
    .map(|(n, level, c)| CharacterListEntry {
        name: n.into(),
        level,
        aparencia: 0,
        weapon_id: Some(c.arma()),
        faction: Default::default(),
        skins: 0,
        auras: 0,
    });
    // The first one dressed: an outfit, a hat and legendary +12 gear, so the
    // select screen's portrait shows its OWN look and glow.
    let mut chars = chars;
    {
        use shared::aparencia as ap;
        let mut a = ap::Aparencia::default();
        a.roupa = ap::ROUPA_BASE + 4;
        a.cabelo = ap::cabelo_do_chapeu(ap::CHAPEU_BASE + 8).unwrap_or(0);
        chars[0].aparencia = a.empacota();
        let mut eq = shared::Equipment::default();
        let mut i = shared::items::ItemInstance::vazia_de_grau(5);
        i.tier = 4;
        i.refinement = 12;
        eq.weapon = Some(Conjunto::Katana.arma());
        eq.weapon_inst = Some(i);
        eq.armor = Some(406);
        eq.armor_inst = Some(i);
        chars[0].auras = shared::auras::equipamento(&eq);
    }
    let armas = Conjunto::TODOS.map(|c| c.arma());
    let mut sel = 0;
    tela.recebeu_lista(&chars, &armas, &mut sel);
    let solido = render3d::material_solido();
    let inicio = get_time();
    let mut quadro = 0;
    let mut cena = 0;
    let exportar = std::env::var("MMO_PREVIA_EXPORTAR").is_ok();
    let mut teclado = crate::entrada::Teclado::novo();
    loop {
        teclado.coleta(get_time());
        if is_key_pressed(KeyCode::F10) {
            break;
        }
        if exportar {
            if tela.saida_previa.as_ref().is_none_or(|t| {
                t.texture.width() != largura_tela() || t.texture.height() != altura_tela()
            }) {
                tela.saida_previa =
                    Some(render_target(largura_tela() as u32, altura_tela() as u32));
            }
            tela.criando = cena > 0;
            if cena > 0 {
                tela.arma = Some(armas[cena - 1]);
                tela.nome = "NovoHeroi".into();
                // A free outfit and a free hat that come AFTER paid ones in
                // the tables: the rows list them by price, not position.
                use shared::aparencia as ap;
                tela.aparencia.roupa = ap::ROUPA_BASE + 4;
                tela.aparencia.cabelo = ap::cabelo_do_chapeu(ap::CHAPEU_BASE + 8).unwrap_or(0);
                // The last scene shows the Appearance tab.
                tela.aba = (cena == 4) as u8;
            }
        }
        let _ = tela.desenha(&chars, &armas, &mut sel, teclado.digitado(), vox, &solido);
        if exportar && get_time() - inicio > 2.0 {
            quadro += 1;
            if quadro % 6 == 0 {
                unsafe {
                    get_internal_gl().flush();
                }
                tela.saida_previa
                    .as_ref()
                    .unwrap()
                    .texture
                    .get_texture_data()
                    .export_png(&format!("/tmp/2dengine-personagem-{cena}.png"));
                cena += 1;
                if cena == 5 {
                    break;
                }
            }
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_da_previa_em_pixels_cai_dentro_do_retrato() {
        // Tela 844x390 pontos (iPhone paisagem), retrato na esquerda.
        let area = Rect::new(26.0, 128.0, 300.0, 172.0);
        for s in [1.0f32, 2.0, 3.0] {
            let alto = 390.0 * s;
            let (x, y, w, h) = render3d::viewport_em_pixels(area, s, alto).unwrap();
            assert_eq!(
                (x, w),
                ((26.0 * s) as i32, (300.0 * s) as i32),
                "escala {s}"
            );
            assert_eq!(h, (172.0 * s) as i32, "escala {s}");
            // Origem embaixo: a base do retrato (300 pt) fica a 90 pt do chao.
            assert_eq!(y, (90.0 * s) as i32, "escala {s}");
            assert!(y + h <= alto as i32);
        }
        assert!(
            render3d::viewport_em_pixels(Rect::new(0.0, 500.0, 100.0, 50.0), 2.0, 780.0).is_none(),
            "fora da tela"
        );
        assert!(
            render3d::viewport_em_pixels(Rect::new(0.0, 0.0, 0.2, 10.0), 1.0, 100.0).is_none(),
            "sem largura"
        );
    }
    /// Nenhum render target com profundidade em lugar nenhum do cliente: no
    /// iPhone ele sai vazio (ver `render3d::viewport_em_pixels`).
    #[test]
    fn nenhum_render_target_com_profundidade_no_cliente() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let proibido = ["render_target", "_ex("].concat();
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "rs") {
                let fonte = std::fs::read_to_string(&p).unwrap();
                // Excecoes: previas de captura que so' rodam no desktop pra
                // gerar PNG e nunca entram no fluxo do app. A loja e a
                // revelacao de montaria precisam de profundidade na cena 3D.
                let previa_hud = p.file_name().is_some_and(|n| n == "previa_hud.rs")
                    && fonte.contains("#![cfg(all(debug_assertions, not(any(target_os = \"ios\", target_os = \"android\"))))]");
                let previa = previa_hud || (p.file_name().is_some_and(|n| n == "loja_tp.rs")
                    && fonte.contains("MMO_PREVIA_LOJA"))
                    || (p.file_name().is_some_and(|n| n == "invocacao_ui.rs")
                        && fonte.contains("MMO_PREVIA_INVOCACAO"))
                    || (p.file_name().is_some_and(|n| n == "pets_ui.rs")
                        && fonte.contains("MMO_PREVIA_PETS"))
                    || (p.file_name().is_some_and(|n| n == "energia_vfx.rs")
                        && fonte.contains("MMO_PREVIA_ENERGIA"))
                    || (p.file_name().is_some_and(|n| n == "guarda_roupa_ui.rs")
                        && fonte.contains("MMO_PREVIA_APARENCIA")
                        && fonte.contains("#[cfg(all(debug_assertions, not(any(target_os = \"ios\", target_os = \"android\"))))]"))
                    || (p.file_name().is_some_and(|n| n == "terreno.rs")
                        && fonte.contains("MMO_PREVIA_COLONIA"))
                    || (p.file_name().is_some_and(|n| n == "previa_porta.rs")
                        && fonte.contains("MMO_PREVIA_PORTA"))
                    || (p.file_name().is_some_and(|n| n == "previa_bestiary.rs")
                        && fonte.contains("MMO_PREVIA_BESTIARY"));
                for (n, l) in fonte.lines().enumerate() {
                    let codigo = l.split("//").next().unwrap_or("");
                    assert!(
                        !codigo.contains(&proibido) || previa,
                        "{}:{}: render target com profundidade volta a sumir no iOS",
                        p.display(),
                        n + 1
                    );
                }
            }
        }
        assert!(include_str!("personagens.rs").contains("viewport:Some(vp)"));
        assert!(include_str!("bolsa.rs").contains("viewport: Some(vp)"));
    }
    /// Tudo da criacao dentro da tela e sem se sobrepor, nos tamanhos de verdade.
    #[test]
    fn criacao_cabe_na_tela_do_celular_e_do_pc() {
        let dentro = |r: Rect, w: f32, h: f32| {
            r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= w && r.y + r.h <= h && r.w > 0.0 && r.h > 0.0
        };
        // iPhone deitado (com notch e barra do home), Android do emulador, PC.
        for (w, h, seguro) in [
            (844.0, 390.0, [0.0, 47.0, 21.0, 47.0]),
            (914.0, 411.0, [0.0; 4]),
            (667.0, 375.0, [0.0; 4]),
            (1280.0, 720.0, [0.0; 4]),
            (1920.0, 1080.0, [0.0; 4]),
        ] {
            let l = LayoutCriacao::novo(w, h, seguro);
            for (nome, r) in [
                ("retrato", l.retrato),
                ("nome", l.nome),
                ("botao", l.botao),
                ("painel", l.painel),
            ] {
                assert!(dentro(r, w, h), "{w}x{h}: {nome} fora da tela: {r:?}");
            }
            assert!(!l.nome.overlaps(&l.botao), "{w}x{h}: nome em cima do botao");
            assert!(
                l.retrato.y + l.retrato.h <= l.nome.y,
                "{w}x{h}: personagem em cima do nome"
            );
            assert!(
                l.retrato.x + l.retrato.w <= l.painel.x,
                "{w}x{h}: personagem em cima do painel"
            );
            // Nome e botao centrados embaixo do personagem.
            let cx = l.retrato.x + l.retrato.w * 0.5;
            for r in [l.nome, l.botao] {
                assert!(
                    (r.x + r.w * 0.5 - cx).abs() < 1.0,
                    "{w}x{h}: fora do centro do personagem"
                );
            }
            assert!(
                l.nome.y > l.retrato.y && l.botao.y > l.nome.y,
                "{w}x{h}: ordem personagem > nome > botao"
            );
            assert!(
                l.retrato.h - l.rodape_retrato >= 150.0,
                "{w}x{h}: personagem pequeno demais: {}",
                l.retrato.h
            );
            assert!(
                l.fim_do_essencial() <= l.painel.y + l.painel.h,
                "{w}x{h}: o painel nao cobre o conteudo — texto desenhado fora do fundo"
            );
            assert!(
                l.painel.w >= 300.0,
                "{w}x{h}: painel estreito demais: {}",
                l.painel.w
            );
        }
    }
    /// Com o teclado da tela aberto, o campo sobe e continua dentro da tela.
    #[test]
    fn campo_de_nome_sobe_acima_do_teclado() {
        let (w, h) = (844.0, 390.0);
        let l = LayoutCriacao::novo(w, h, [0.0, 47.0, 21.0, 47.0]);
        let dy = l.subida_do_nome(true, h);
        let y = l.nome.y - dy;
        assert!(
            y + l.nome.h <= h * 0.48,
            "campo ainda atras do teclado: {y}"
        );
        assert!(y - 28.0 >= 0.0, "campo saiu por cima: {y}");
    }
    #[test]
    fn nome_segue_validacao_do_servidor() {
        assert_eq!(valida_nome("  Ana_23 "), Ok("Ana_23"));
        for n in [
            "a",
            "",
            "nome com espaco",
            "João",
            "aaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(valida_nome(n).is_err());
        }
    }
    #[test]
    fn criacao_envia_arma_e_faccao_e_recusa_arma_indisponivel() {
        let mut p = Personagens::default();
        p.nome = "Teste".into();
        p.arma = Some(Conjunto::Katana.arma());
        p.faccao = Faction::Morganeers;
        let armas = Conjunto::TODOS.map(|c| c.arma());
        match p.pedido_criacao(&armas).unwrap() {
            ClientMessage::CreateCharacter {
                name,
                starting_weapon,
                faction,
                ..
            } => {
                assert_eq!(name, "Teste");
                assert_eq!(starting_weapon, Conjunto::Katana.arma());
                assert_eq!(faction, Faction::Morganeers);
            }
            _ => panic!("mensagem incorreta"),
        }
        assert!(p.pedido_criacao(&[Conjunto::Pistolas.arma()]).is_err());
    }
    #[test]
    fn confirmacao_seleciona_novo_personagem_e_erro_preserva_escolhas() {
        let mut p = Personagens::default();
        let mut sel = 0;
        let armas = [Conjunto::Katana.arma()];
        p.recebeu_lista(&[], &armas, &mut sel);
        assert!(p.criando);
        assert_eq!(p.arma, Some(armas[0]));
        p.nome = "Novo".into();
        p.aguardando = Some(("Novo".into(), 0.0));
        let chars = ["Antigo", "Novo"].map(|n| CharacterListEntry {
            name: n.into(),
            level: 1,
            weapon_id: Some(armas[0]),
            aparencia: 0,
            faction: Default::default(),
            skins: 0,
            auras: 0,
        });
        p.recebeu_lista(&chars, &armas, &mut sel);
        assert_eq!(sel, 1);
        assert!(!p.criando);
        assert!(p.aguardando.is_none());
        p.falhou("name already taken".into());
        assert_eq!(p.nome, "Novo");
        assert_eq!(p.arma, Some(armas[0]));
        assert!(p.criando);
    }
}
