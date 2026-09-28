//! Modo economia de energia, no molde do MIR4 (docs/MODO_ECONOMIA.md).
//!
//! A tela continua ACESA (o jogo segue no automatico), mas o mundo 3D para de
//! ser desenhado: fundo preto, um resumo pequeno do que rendeu desde que o modo
//! ligou e um trilho pra deslizar e voltar. O quadro cai pra `FPS` — a rede, o
//! auto combate, as SKILLS automaticas, a coleta e as pocoes seguem rodando
//! nesse ritmo.
//!
//! As skills nao seguiam: `bloqueia_entrada` entra no `teclado_bloqueado()` do
//! jogo, e `usar_habilidade` saia inteiro nele — a rotacao AUTO ia embora com
//! o gesto e a tecla. Com a tela preta o personagem nao lancava NADA, nem a
//! cura, e morria em lugar onde aguentava jogando. Entrada bloqueada e' pra
//! valer pro toque, nao pro automatico (`habilidades::pedido_automatico`).
//!
//! Toque no preto nao faz nada: nem anda, nem mira, nem desliga o auto. So' o
//! deslize completo volta.
use macroquad::prelude::*;

use crate::hud_estilo as estilo;

/// Quadros por segundo com o modo ligado. Dez bastam pro auto (o input vai a
/// 20 Hz mas so' muda quando o alvo muda) e o resumo nao precisa de mais.
pub const FPS: f64 = 10.0;
/// Escolhas do "entrar sozinho parado", em minutos (0 = nunca).
pub const OPCOES_AUTO_MIN: [u16; 4] = [0, 3, 5, 10];
/// Teto aceito das preferencias.
pub const AUTO_MIN_MAX: u16 = 60;
/// Depois de sair, o dedo que deslizou ainda esta' na tela: por este tempo
/// nenhum toque vira clique no mundo.
const RESPIRO_S: f64 = 0.4;
/// Quanto do trilho o deslize precisa andar pra valer.
const DESLIZE_VALE: f32 = 0.85;

/// No celular o padrao e' entrar sozinho depois de 5 min parado; no PC, nunca
/// (a janela preta no meio do teste so' atrapalha).
pub fn auto_min_padrao() -> u16 {
    if crate::nativo::TECLADO_NA_TELA {
        5
    } else {
        0
    }
}

/// O que o resumo mostra. Montado pelo `Jogo` a cada quadro.
pub struct Resumo<'a> {
    pub nome: &'a str,
    pub nivel: u32,
    pub hp: i32,
    pub hp_max: i32,
    /// Fracao da EXP dentro do nivel, 0..1.
    pub exp: f32,
    pub xp: u64,
    pub ouro: u64,
    /// Ex.: "AUTO COMBATE". A cor diz se esta' rendendo.
    pub estado: (&'a str, Color),
    pub ping_ms: f32,
    pub nome_item: &'a dyn Fn(u16) -> String,
    pub auto_resumo: Option<&'a crate::auto_resumo::AutoResumo>,
    pub missao_atual: Option<(u16, &'a str, u32, u32)>,
    pub proximas_missoes: &'a [u16],
    pub nomes_itens: &'a std::collections::HashMap<u16, String>,
}

/// O que rendeu enquanto o jogador estava fora: mostrado numa janela
/// flutuante quando ele desliza pra voltar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResumoDaAusencia {
    pub duracao_s: f64,
    pub xp: u64,
    pub ouro: u64,
    pub niveis: u32,
    pub mortes: u32,
    /// Maior quantidade primeiro.
    pub itens: Vec<(u16, u32)>,
}

#[derive(Default)]
pub struct Economia {
    pub ativa: bool,
    /// Janela "Enquanto você estava fora", aberta ate' o jogador fechar.
    pub resumo: Option<ResumoDaAusencia>,
    /// `None` = nunca escolheu: vale `auto_min_padrao`.
    pub auto_min: Option<u16>,
    desde: f64,
    saiu_em: f64,
    xp_inicio: u64,
    nivel_inicio: u32,
    ouro_inicio: u64,
    /// O que entrou na bolsa desde que ligou, por item.
    itens: Vec<(u16, u32)>,
    mortes: u32,
    ultima_atividade: f64,
    ultimo_quadro: f64,
    /// x do dedo quando comecou a deslizar.
    arrasto: Option<f32>,
}

impl Economia {
    pub fn auto_min(&self) -> u16 {
        self.auto_min.unwrap_or_else(auto_min_padrao)
    }

    pub fn entrar(&mut self, agora: f64, xp: u64, nivel: u32, ouro: u64) {
        if self.ativa {
            return;
        }
        *self = Economia {
            ativa: true,
            auto_min: self.auto_min,
            desde: agora,
            xp_inicio: xp,
            nivel_inicio: nivel,
            ouro_inicio: ouro,
            ..Default::default()
        };
    }

    pub fn sair(&mut self, agora: f64) {
        self.ativa = false;
        self.arrasto = None;
        self.saiu_em = agora;
        self.ultima_atividade = agora;
    }

    /// Saida pelo deslize: sai e abre o resumo do que rendeu.
    pub fn sair_com_resumo(&mut self, agora: f64, xp: u64, nivel: u32, ouro: u64) {
        if !self.ativa {
            return;
        }
        let mut itens = self.itens.clone();
        itens.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        self.resumo = Some(ResumoDaAusencia {
            duracao_s: (agora - self.desde).max(0.0),
            xp: xp.saturating_sub(self.xp_inicio),
            ouro: ouro.saturating_sub(self.ouro_inicio),
            niveis: nivel.saturating_sub(self.nivel_inicio),
            mortes: self.mortes,
            itens,
        });
        self.sair(agora);
    }

    /// A janela do resumo, por cima do jogo. Fecha no botao.
    pub fn desenha_resumo(
        &mut self,
        nome_item: &dyn Fn(u16) -> String,
        vox: &crate::vox::VoxCache,
        solido: &macroquad::material::Material,
    ) {
        let Some(r) = &self.resumo else { return };
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let colunas = 4usize;
        let lado = 64.0 * f;
        let vao = 10.0 * f;
        let w = (colunas as f32 * (lado + vao) - vao + 48.0 * f)
            .max(420.0 * f)
            .min(seguro.w - 24.0);
        let linhas_itens = r.itens.len().div_ceil(colunas).clamp(1, 3);
        let h = (230.0 * f + linhas_itens as f32 * (lado + 30.0 * f)).min(seguro.h - 24.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.45);
        estilo::painel(p);
        let x = p.x + 24.0 * f;
        let mut y = p.y + 38.0 * f;
        estilo::texto_forte(x, y, "Enquanto você estava fora", 20, estilo::OURO);
        let t = formata_duracao(r.duracao_s);
        estilo::texto(
            p.x + p.w - 24.0 * f - estilo::medir(&t, 14),
            y,
            &t,
            14,
            estilo::SUAVE,
        );
        y += 36.0 * f;
        let mut numeros = vec![
            ("XP", format!("+{}", milhar(r.xp))),
            ("Ouro", format!("+{}", milhar(r.ouro))),
        ];
        if r.niveis > 0 {
            numeros.push(("Níveis", format!("+{}", r.niveis)));
        }
        if r.mortes > 0 {
            numeros.push(("Mortes", r.mortes.to_string()));
        }
        let cw = (p.w - 48.0 * f) / numeros.len() as f32;
        for (i, (rot, val)) in numeros.iter().enumerate() {
            let cx = x + cw * (i as f32 + 0.5);
            estilo::texto_centro(cx, y, rot, 13, estilo::SUAVE);
            estilo::texto_centro_forte(cx, y + 26.0 * f, val, 20, estilo::TEXTO);
        }
        y += 52.0 * f;
        estilo::texto_forte(x, y, "ITENS", 12, estilo::SUAVE);
        y += 12.0 * f;
        let m = Vec2::from(mouse_position());
        if r.itens.is_empty() {
            estilo::texto(x, y + 24.0 * f, "Nenhum item coletado", 14, estilo::SUAVE);
        }
        let cabem = colunas * linhas_itens;
        let mut dica = None;
        for (i, (id, q)) in r.itens.iter().take(cabem).enumerate() {
            let c = Rect::new(
                x + (i % colunas) as f32 * (lado + vao),
                y + (i / colunas) as f32 * (lado + 30.0 * f),
                lado,
                lado,
            );
            estilo::cartao(c, c.contains(m), false);
            crate::icones::icone_com_3d(*id, c, None, Some(*q), Some((vox, solido)));
            if *q == 1 {
                estilo::texto_centro_forte(
                    c.x + c.w - 10.0 * f,
                    c.y + c.h - 4.0 * f,
                    "1",
                    12,
                    estilo::TEXTO,
                );
            }
            let nome = nome_item(*id);
            estilo::texto_ajustado(
                &nome,
                c.x,
                c.y + c.h + 16.0 * f,
                lado + vao - 2.0,
                11,
                estilo::SUAVE,
            );
            if c.contains(m) {
                dica = Some((c, format!("{nome} ×{}", milhar(*q as u64))));
            }
        }
        if r.itens.len() > cabem {
            let t = format!("+ {} outros itens", r.itens.len() - cabem);
            estilo::texto(x, p.y + p.h - 70.0 * f, &t, 13, estilo::SUAVE);
        }
        let ok = Rect::new(
            p.center().x - 90.0 * f,
            p.y + p.h - 58.0 * f,
            180.0 * f,
            44.0 * f,
        );
        estilo::cartao(ok, ok.contains(m), true);
        estilo::texto_centro_forte(
            ok.center().x,
            ok.center().y + 6.0 * f,
            "OK",
            17,
            estilo::OURO,
        );
        if let Some((c, t)) = dica {
            estilo::tooltip(c, &t, false);
        }
        if crate::foco::clique() && ok.contains(m) {
            self.resumo = None;
        }
    }

    /// Entrada do jogador nao vale pro mundo: modo ligado ou acabou de sair.
    pub fn bloqueia_entrada(&self, agora: f64) -> bool {
        self.ativa || (self.saiu_em > 0.0 && agora - self.saiu_em < RESPIRO_S)
    }

    /// O que entrou na bolsa (`Ganhos::bolsa_nova`). Fora do modo, nada.
    pub fn itens_novos(&mut self, entrou: &[(u16, u32)]) {
        if !self.ativa {
            return;
        }
        for &(id, q) in entrou {
            match self.itens.iter_mut().find(|(i, _)| *i == id) {
                Some(l) => l.1 = l.1.saturating_add(q),
                None => self.itens.push((id, q)),
            }
        }
    }

    pub fn morreu(&mut self) {
        if self.ativa {
            self.mortes += 1;
        }
    }

    /// Um quadro fora do modo. `true` = ficou parado o bastante: entrar.
    pub fn parado_demais(&mut self, agora: f64, mexeu: bool) -> bool {
        if mexeu || self.ativa || self.ultima_atividade == 0.0 {
            self.ultima_atividade = agora;
            return false;
        }
        let min = self.auto_min();
        min > 0 && agora - self.ultima_atividade >= min as f64 * 60.0
    }

    /// Segura o quadro pra ficar em `FPS`. Chamado depois de desenhar.
    pub fn segurar_quadro(&mut self) {
        if self.ativa {
            let falta = 1.0 / FPS - (get_time() - self.ultimo_quadro);
            if falta > 0.0 {
                std::thread::sleep(std::time::Duration::from_secs_f64(falta.min(0.2)));
            }
        }
        self.ultimo_quadro = get_time();
    }

    /// Desenha a tela preta. `true` no quadro em que o deslize destravou.
    pub fn desenha(&mut self, r: &Resumo, agora: f64) -> bool {
        clear_background(BLACK);
        set_default_camera();
        let (sw, sh) = (screen_width(), screen_height());
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        // Tudo escorrega devagar uns pixels: o mesmo desenho parado por uma
        // hora marcaria a tela OLED.
        let t = (agora - self.desde) as f32;
        let deriva = vec2((t / 67.0).sin() * 14.0, (t / 53.0).cos() * 10.0) * f;
        let w = (560.0 * f).min(seguro.w - 32.0);
        let x = seguro.center().x - w * 0.5 + deriva.x;
        let mut y = seguro.y + (seguro.h * 0.10).max(24.0 * f) + deriva.y;
        let suave = Color::new(0.55, 0.56, 0.60, 1.0);
        let texto = Color::new(0.80, 0.80, 0.83, 1.0);

        bateria(
            vec2(x + 14.0 * f, y - 5.0 * f),
            11.0 * f,
            Color::new(0.35, 0.78, 0.45, 1.0),
        );
        estilo::texto_forte(x + 34.0 * f, y, "MODO ECONOMIA DE ENERGIA", 13, suave);
        let tempo = formata_duracao(agora - self.desde);
        estilo::texto(x + w - estilo::medir(&tempo, 13), y, &tempo, 13, suave);
        y += 44.0 * f;

        estilo::texto_forte(x, y, &format!("Nv {}  {}", r.nivel, r.nome), 22, texto);
        let (estado, cor) = r.estado;
        estilo::texto_forte(
            x + w - estilo::medir_forte(estado, 15),
            y,
            estado,
            15,
            estilo::alfa(cor, 0.9),
        );
        y += 16.0 * f;
        let hp = if r.hp_max > 0 {
            r.hp as f32 / r.hp_max as f32
        } else {
            0.0
        };
        barra(
            Rect::new(x, y, w, 8.0 * f),
            hp,
            Color::new(0.72, 0.22, 0.22, 1.0),
        );
        y += 22.0 * f;
        estilo::texto(x, y, &format!("HP {}/{}", r.hp.max(0), r.hp_max), 12, suave);
        let exp = format!("EXP {:.2}%", (r.exp * 100.0).clamp(0.0, 100.0));
        estilo::texto(x + w - estilo::medir(&exp, 12), y, &exp, 12, suave);
        y += 8.0 * f;
        barra(
            Rect::new(x, y, w, 5.0 * f),
            r.exp,
            Color::new(0.78, 0.64, 0.30, 1.0),
        );
        y += 40.0 * f;

        if let Some(auto) = r.auto_resumo {
            let alto = (seguro.y + seguro.h - 108.0 * f - y).max(100.0 * f);
            auto.desenhar(Rect::new(x, y - 12.0 * f, w, alto),
                r.missao_atual, r.proximas_missoes, r.nomes_itens);
        } else {
        estilo::texto_forte(x, y, "DESDE QUE LIGOU", 12, suave);
        y += 26.0 * f;
        let niveis = r.nivel.saturating_sub(self.nivel_inicio);
        let mut linhas = vec![
            (
                "XP",
                format!("+{}", milhar(r.xp.saturating_sub(self.xp_inicio))),
            ),
            (
                "Ouro",
                format!("+{}", milhar(r.ouro.saturating_sub(self.ouro_inicio))),
            ),
        ];
        if niveis > 0 {
            linhas.push(("Níveis", format!("+{niveis}")));
        }
        if self.mortes > 0 {
            linhas.push(("Mortes", self.mortes.to_string()));
        }
        let mut itens = self.itens.clone();
        itens.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        // Duas colunas: numeros a' esquerda, itens a' direita.
        let (xe, xd, cw) = (x, x + w * 0.42, w * 0.58);
        let y0 = y;
        for (rot, val) in &linhas {
            estilo::texto(xe, y, rot, 15, suave);
            estilo::texto_forte(
                xe + w * 0.36 - estilo::medir_forte(val, 15),
                y,
                val,
                15,
                texto,
            );
            y += 24.0 * f;
        }
        let cabem = (((seguro.y + seguro.h - 150.0 * f) - y0) / (22.0 * f)).max(1.0) as usize;
        let mut yd = y0;
        if itens.is_empty() {
            estilo::texto(xd, yd, "Nenhum item ainda", 14, suave);
        }
        for (i, (id, q)) in itens.iter().enumerate() {
            if i + 1 == cabem && itens.len() > cabem {
                estilo::texto(xd, yd, &format!("+ {} outros", itens.len() - i), 13, suave);
                break;
            }
            let qtd = format!("×{}", milhar(*q as u64));
            estilo::texto_ajustado(
                &(r.nome_item)(*id),
                xd,
                yd,
                cw - estilo::medir_forte(&qtd, 14) - 12.0 * f,
                14,
                texto,
            );
            estilo::texto_forte(xd + cw - estilo::medir_forte(&qtd, 14), yd, &qtd, 14, texto);
            yd += 22.0 * f;
        }
        }
        if r.ping_ms > 0.0 {
            let p = format!("{:.0} ms", r.ping_ms);
            estilo::texto(
                seguro.x + seguro.w - estilo::medir(&p, 11) - 8.0 * f,
                seguro.y + seguro.h - 8.0 * f,
                &p,
                11,
                estilo::alfa(suave, 0.6),
            );
        }

        self.deslize(Rect::new(
            sw * 0.5 - (210.0 * f).min(sw * 0.5 - 24.0),
            (seguro.y + seguro.h - 90.0 * f).min(sh - 70.0 * f),
            (420.0 * f).min(sw - 48.0),
            58.0 * f,
        ))
    }

    /// O trilho "deslize para voltar". Dedo ou mouse.
    fn deslize(&mut self, trilho: Rect) -> bool {
        let m = Vec2::from(mouse_position());
        let bola = trilho.h - 8.0;
        let curso = (trilho.w - bola - 8.0).max(1.0);
        if crate::foco::clique() && trilho.contains(m) {
            self.arrasto = Some(m.x);
        }
        let progresso = match self.arrasto {
            Some(x0) if is_mouse_button_down(MouseButton::Left) => {
                ((m.x - x0) / curso).clamp(0.0, 1.0)
            }
            _ => 0.0,
        };
        let soltou = self.arrasto.is_some() && !is_mouse_button_down(MouseButton::Left);
        if soltou {
            // O ultimo quadro com o dedo ja' mediu: vale o que andou ate' ali.
            let x0 = self.arrasto.take().unwrap_or(m.x);
            if deslize_vale((m.x - x0) / curso) {
                return true;
            }
        }
        let raio = trilho.h * 0.5;
        estilo::ret_arredondado(trilho, raio, Color::new(0.07, 0.07, 0.08, 1.0));
        estilo::borda_arredondada(trilho, raio, 1.0, Color::new(0.22, 0.22, 0.25, 1.0));
        if progresso > 0.0 {
            estilo::ret_arredondado(
                Rect::new(trilho.x, trilho.y, bola + 8.0 + curso * progresso, trilho.h),
                raio,
                Color::new(0.14, 0.26, 0.17, 1.0),
            );
        }
        let c = vec2(
            trilho.x + 4.0 + bola * 0.5 + curso * progresso,
            trilho.center().y,
        );
        draw_circle(c.x, c.y, bola * 0.5, Color::new(0.30, 0.62, 0.38, 1.0));
        for k in 0..3 {
            let dx = (k as f32 - 1.0) * bola * 0.14;
            estilo::traco(
                vec2(c.x + dx - bola * 0.06, c.y - bola * 0.14),
                vec2(c.x + dx + bola * 0.06, c.y),
                2.0,
                BLACK,
            );
            estilo::traco(
                vec2(c.x + dx + bola * 0.06, c.y),
                vec2(c.x + dx - bola * 0.06, c.y + bola * 0.14),
                2.0,
                BLACK,
            );
        }
        let rotulo = "Deslize para voltar";
        estilo::texto_centro(
            trilho.center().x + bola * 0.4,
            trilho.center().y + 6.0,
            rotulo,
            15,
            Color::new(0.55, 0.56, 0.60, 1.0 - progresso),
        );
        false
    }
}

/// O jogador fez alguma coisa neste quadro (dedo, mouse, tecla)?
pub fn houve_entrada() -> bool {
    !touches().is_empty()
        || is_mouse_button_down(MouseButton::Left)
        || is_mouse_button_down(MouseButton::Right)
        || !get_keys_down().is_empty()
        || mouse_delta_position() != Vec2::ZERO
        || mouse_wheel().1 != 0.0
}

fn deslize_vale(fracao: f32) -> bool {
    fracao >= DESLIZE_VALE
}

fn barra(r: Rect, fracao: f32, cor: Color) {
    let raio = r.h * 0.5;
    estilo::ret_arredondado(r, raio, Color::new(0.10, 0.10, 0.11, 1.0));
    let w = r.w * fracao.clamp(0.0, 1.0);
    if w > 1.0 {
        estilo::ret_arredondado(Rect::new(r.x, r.y, w.max(r.h), r.h), raio, cor);
    }
}

/// Pilha de bateria com carga, o icone do modo (HUD e tela preta).
pub fn bateria(c: Vec2, s: f32, cor: Color) {
    let corpo = Rect::new(c.x - s, c.y - s * 0.55, s * 1.8, s * 1.1);
    let esp = (s * 0.15).max(1.2);
    estilo::borda_arredondada(corpo, s * 0.2, esp, cor);
    draw_rectangle(
        corpo.x + corpo.w + esp * 0.3,
        c.y - s * 0.25,
        s * 0.22,
        s * 0.5,
        cor,
    );
    let dentro = s * 0.22;
    draw_rectangle(
        corpo.x + dentro,
        corpo.y + dentro,
        (corpo.w - 2.0 * dentro) * 0.6,
        corpo.h - 2.0 * dentro,
        cor,
    );
}

/// "1h 02m" ou "12m 05s".
pub fn formata_duracao(s: f64) -> String {
    let s = s.max(0.0) as u64;
    let (h, m, seg) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m {seg:02}s")
    }
}

/// 1234567 -> "1.234.567".
pub fn milhar(v: u64) -> String {
    let d = v.to_string();
    let mut out = String::with_capacity(d.len() + d.len() / 3);
    for (i, ch) in d.chars().enumerate() {
        if i > 0 && (d.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_entra_sozinho_depois_do_tempo_parado() {
        let mut e = Economia {
            auto_min: Some(3),
            ..Default::default()
        };
        assert!(!e.parado_demais(10.0, false), "o primeiro quadro so' marca");
        assert!(!e.parado_demais(10.0 + 179.0, false));
        assert!(e.parado_demais(10.0 + 180.0, false));
        assert!(!e.parado_demais(200.0, true), "mexeu zera");
        assert!(!e.parado_demais(200.0 + 179.0, false));
    }

    #[test]
    fn nunca_quando_desligado() {
        let mut e = Economia {
            auto_min: Some(0),
            ..Default::default()
        };
        e.parado_demais(1.0, false);
        assert!(!e.parado_demais(1.0e6, false));
    }

    #[test]
    fn resumo_junta_itens_so_com_o_modo_ligado() {
        let mut e = Economia::default();
        e.itens_novos(&[(1, 5)]);
        e.morreu();
        e.entrar(0.0, 100, 3, 50);
        e.itens_novos(&[(1, 5), (2, 1)]);
        e.itens_novos(&[(1, 7)]);
        e.morreu();
        assert_eq!(e.itens, vec![(1, 12), (2, 1)]);
        assert_eq!(e.mortes, 1);
    }

    #[test]
    fn entrar_de_novo_nao_zera_o_resumo() {
        let mut e = Economia::default();
        e.entrar(0.0, 100, 3, 50);
        e.itens_novos(&[(9, 2)]);
        e.entrar(5.0, 999, 9, 999);
        assert_eq!((e.xp_inicio, e.itens.len()), (100, 1));
    }

    #[test]
    fn saida_segura_o_toque_um_instante() {
        let mut e = Economia::default();
        e.entrar(0.0, 0, 1, 0);
        assert!(e.bloqueia_entrada(1.0));
        e.sair(10.0);
        assert!(e.bloqueia_entrada(10.2));
        assert!(!e.bloqueia_entrada(10.5));
        assert!(
            !Economia::default().bloqueia_entrada(0.1),
            "quem nunca entrou nao bloqueia"
        );
    }

    #[test]
    fn deslizar_pra_sair_abre_o_resumo_da_ausencia() {
        let mut e = Economia::default();
        e.entrar(100.0, 1_000, 5, 50);
        e.itens_novos(&[(7, 3), (9, 10)]);
        e.morreu();
        e.sair_com_resumo(100.0 + 3725.0, 4_500, 6, 180);
        assert!(!e.ativa);
        let r = e.resumo.clone().expect("resumo aberto");
        assert_eq!(
            r,
            ResumoDaAusencia {
                duracao_s: 3725.0,
                xp: 3_500,
                ouro: 130,
                niveis: 1,
                mortes: 1,
                itens: vec![(9, 10), (7, 3)]
            }
        );
        // Fora do modo nao abre nada.
        let mut fora = Economia::default();
        fora.sair_com_resumo(10.0, 1, 1, 1);
        assert!(fora.resumo.is_none());
    }

    #[test]
    fn deslize_curto_nao_vale() {
        assert!(!deslize_vale(0.5));
        assert!(deslize_vale(0.9));
    }

    #[test]
    fn textos() {
        assert_eq!(milhar(1234567), "1.234.567");
        assert_eq!(milhar(12), "12");
        assert_eq!(formata_duracao(3725.0), "1h 02m");
        assert_eq!(formata_duracao(65.0), "1m 05s");
    }
}
