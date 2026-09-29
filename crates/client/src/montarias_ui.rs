//! Montarias (docs/MONTARIAS.md): Menu → Personagem → Montaria.
//!
//! The mount became a **bag item**, like the pet: what decides which one
//! counts is the equipment's Mount slot, not this window. Here you only see
//! the equipped one — in 3D, in its color — and mount it.
//!
//! There are no skins any more: **the color is the variation**. The skin
//! catalogue went entirely.

use macroquad::prelude::*;
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;
use crate::vox::VoxCache;

/// What the window asks of the game.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Acao {
    AbrirLoja,
    Montar,
}

#[derive(Default)]
pub struct MontariasUi {
    pub aberto: bool,
    pub firmeza: u8,
    pub bloqueada_ate: f64,
    /// Gira o modelo no palco.
    giro: f32,
    /// A faixa "minhas montarias": escolher, equipar, combinar.
    colecao: crate::colecao::Colecao,
    /// Clock (get_time) at which mounting ends; 0 = not mounting.
    montando_ate: f64,
    montando_desde: f64,
}

const SIGLAS: [&str; shared::STAT_COUNT] = ["FOR", "DES", "INT", "VIT", "SPD", "RES"];

impl MontariasUi {
    pub fn abrir(&mut self) -> Vec<ClientMessage> {
        self.aberto = true;
        Vec::new()
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    pub fn pega_o_mouse(&self) -> bool {
        self.aberto
    }

    /// A montada acabou: zera o progresso.
    pub fn montou(&mut self) {
        self.montando_ate = 0.0;
        self.montando_desde = 0.0;
    }

    /// 0..1 enquanto monta; `None` fora disso. E' o anel do botao do HUD.
    pub fn progresso(&self, agora: f64) -> Option<f32> {
        if agora >= self.montando_ate {
            return None;
        }
        let total = (self.montando_ate - self.montando_desde).max(0.001);
        Some(((agora - self.montando_desde) / total).clamp(0.0, 1.0) as f32)
    }

    /// In the middle of mounting?
    pub fn montando(&self, agora: f64) -> bool {
        agora < self.montando_ate
    }

    /// What the shop announces that matters here: the start of mounting.
    pub fn receber(&mut self, aviso: &shared::loja::AvisoLoja, agora: f64) {
        if let shared::loja::AvisoLoja::MontariaCombate { firmeza, bloqueio_segundos } = aviso {
            self.firmeza = *firmeza;
            self.bloqueada_ate = agora + *bloqueio_segundos as f64;
        }
        if let shared::loja::AvisoLoja::Montando { segundos } = aviso {
            self.montando_desde = agora;
            self.montando_ate = agora + *segundos as f64;
        }
    }

    pub fn desenha(
        &mut self,
        vox: &VoxCache,
        solido: &Material,
        equip: &shared::Equipment,
        bolsa: &[shared::InventorySlot],
    ) -> (Option<Acao>, Option<ClientMessage>) {
        if !self.aberto {
            return (None, None);
        }
        let escala = estilo::escala_do_painel(760.0, 552.0);
        estilo::no_painel(escala, || self.na_escala(vox, solido, equip, bolsa))
    }

    fn na_escala(
        &mut self,
        vox: &VoxCache,
        solido: &Material,
        equip: &shared::Equipment,
        bolsa: &[shared::InventorySlot],
    ) -> (Option<Acao>, Option<ClientMessage>) {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (760.0 * f).min(seguro.w - 16.0);
        let h = (552.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.55);
        estilo::painel(p);
        let mouse = Vec2::from(mouse_position());
        let clique = crate::foco::clique();

        estilo::texto_forte(p.x + 20.0 * f, p.y + 36.0 * f, "MONTARIA", 23, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.contains(mouse) {
            self.fechar();
            return (None, None);
        }

        let Some(id) = equip
            .montaria
            .filter(|id| shared::montarias::de_item(*id).is_some())
        else {
            estilo::texto_ajustado(
                "Nenhuma montaria equipada. Equipe uma no slot Montaria da bolsa — a cor dela manda na velocidade.",
                p.x + 24.0 * f,
                p.y + 110.0 * f,
                p.w - 48.0 * f,
                17,
                estilo::SUAVE,
            );
            let loja = Rect::new(
                p.center().x - 110.0 * f,
                p.y + 160.0 * f,
                220.0 * f,
                40.0 * f,
            );
            estilo::botao(
                loja,
                "Ver na Loja",
                estilo::estado_de(loja, false, false),
                true,
            );
            let acao = (clique && loja.contains(mouse)).then_some(Acao::AbrirLoja);
            // The band counts even with nothing equipped: it is what you equip from.
            let faixa = Rect::new(p.x + 18.0 * f, p.y + 220.0 * f, p.w - 36.0 * f, 152.0 * f);
            let msg = self.colecao.desenha(
                faixa,
                f,
                "MINHAS MONTARIAS",
                bolsa,
                None,
                &|id| shared::montarias::de_item(id).is_some(),
                &|id| shared::montarias::nome_do_item(id).unwrap_or_default(),
                Some((vox, solido)),
            );
            return (acao, msg);
        };
        let (especie, grau) = shared::montarias::de_item(id).expect("filtrado acima");
        let cor = cor_do_grau(grau);

        // ── palco 3D ──
        let palco = Rect::new(p.x + 18.0 * f, p.y + 60.0 * f, 250.0 * f, 250.0 * f);
        estilo::cartao(palco, false, false);
        self.giro += get_frame_time().min(0.1) * 0.5;
        crate::render3d::vitrine_montaria(vox, id, palco, self.giro, solido);
        estilo::texto_centro_forte(
            palco.center().x,
            palco.y + palco.h - 14.0 * f,
            especie.nome,
            19,
            cor,
        );

        // ── ficha ──
        let dir = Rect::new(p.x + 284.0 * f, p.y + 60.0 * f, p.w - 302.0 * f, 250.0 * f);
        estilo::cartao(dir, false, false);
        let mut y = dir.y + 32.0 * f;
        estilo::texto_ajustado(
            especie.descricao,
            dir.x + 14.0 * f,
            y,
            dir.w - 28.0 * f,
            15,
            estilo::SUAVE,
        );
        y += 34.0 * f;
        estilo::texto(dir.x + 14.0 * f, y, "Velocidade montado", 13, estilo::SUAVE);
        estilo::texto_forte(
            dir.x + 14.0 * f,
            y + 24.0 * f,
            &format!(
                "{:.0}%",
                shared::montarias::velocidade_da_instancia(grau, equip.montaria_inst.as_ref())
                    * 100.0
            ),
            22,
            estilo::VERDE,
        );

        y += 62.0 * f;
        estilo::texto(dir.x + 14.0 * f, y, "ATRIBUTOS", 13, estilo::SUAVE);
        y += 22.0 * f;
        let mut x = dir.x + 14.0 * f;
        let af = equip.montaria_inst.as_ref().and_then(|i| i.afinidade);
        for (i, pts) in
            shared::montarias::pontos_por_stat_da_instancia(id, equip.montaria_inst.as_ref())
                .iter()
                .enumerate()
        {
            if *pts == 0 {
                continue;
            }
            estilo::texto(x, y, SIGLAS[i], 13, estilo::SUAVE);
            estilo::texto_forte(x, y + 20.0 * f, &format!("+{pts}"), 18, estilo::VERDE);
            x += 62.0 * f;
        }
        let poder = format!(
            "PODER  {}",
            crate::bolsa::milhar(
                crate::bolsa::poder_dos_pontos(shared::montarias::pontos_por_stat_da_instancia(
                    id,
                    equip.montaria_inst.as_ref()
                ))
                .max(0) as u64
            )
        );
        estilo::texto_forte(
            dir.x + dir.w - 14.0 * f - estilo::medir_forte(&poder, 17),
            y + 20.0 * f,
            &poder,
            17,
            estilo::OURO,
        );

        // ── minhas montarias ──
        let faixa = Rect::new(p.x + 18.0 * f, p.y + 318.0 * f, p.w - 36.0 * f, 152.0 * f);
        let msg = self.colecao.desenha(
            faixa,
            f,
            "MINHAS MONTARIAS",
            bolsa,
            Some(id),
            &|x| shared::montarias::de_item(x).is_some(),
            &|x| shared::montarias::nome_do_item(x).unwrap_or_default(),
            Some((vox, solido)),
        );

        // ── montar ──
        let agora = get_time();
        let montando = agora < self.montando_ate;
        let bt = Rect::new(
            p.center().x - 130.0 * f,
            p.y + h - 62.0 * f,
            260.0 * f,
            44.0 * f,
        );
        if montando {
            let total = (self.montando_ate - self.montando_desde).max(0.001);
            let frac = ((agora - self.montando_desde) / total).clamp(0.0, 1.0) as f32;
            estilo::barra(bt, frac, frac, estilo::ACENTO, None);
            estilo::texto_centro_forte(
                bt.center().x,
                bt.center().y + 6.0 * f,
                "MONTANDO…",
                18,
                estilo::TEXTO,
            );
        } else {
            estilo::botao(bt, "MONTAR", estilo::estado_de(bt, false, false), true);
            if clique && bt.contains(mouse) {
                return (Some(Acao::Montar), msg);
            }
        }
        (None, msg)
    }
}

fn cor_do_grau(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

/// The power the mount adds, by the same formula as the rest of the sheet.
pub(crate) fn poder_da_montaria(id: u16, af: Option<[u8; 2]>) -> i32 {
    let (mut atk, mut def, mut hp, mut mp, mut dex, mut wis) = (0, 0, 0, 0, 0, 0);
    let mut crit = 0.0f32;
    for (i, pts) in shared::montarias::pontos_por_stat(id, af)
        .iter()
        .enumerate()
    {
        let Some(b) = shared::STAT_POINT_BONUS.get(i) else {
            continue;
        };
        let p = *pts as i32;
        atk += b.attack_damage * p;
        def += b.defense * p;
        hp += b.hp_max * p;
        mp += b.mp_max * p;
        dex += b.dex * p;
        wis += b.wis * p;
        crit += b.crit_chance * *pts as f32;
    }
    atk * 10 + def * 8 + hp + mp / 2 + (dex + wis) * 5 + (crit * 1000.0) as i32
}

#[cfg(test)]
mod testes {
    use super::*;

    /// With no mount equipped the window asks for nothing beyond opening the
    /// Shop, and with one equipped it can say what the color gives.
    #[test]
    fn a_janela_le_a_montaria_equipada() {
        let ui = MontariasUi::default();
        assert!(!ui.aberto);
        let mut equip = shared::Equipment::default();
        assert!(equip.montaria.is_none());
        let id = shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 4);
        equip.set(shared::EquipSlot::Montaria, Some(id), None);
        assert_eq!(equip.montaria, Some(id));
        let (e, grau) = shared::montarias::de_item(id).unwrap();
        assert_eq!(grau, 4);
        assert_eq!(e.grau, 4, "a criatura E' o grau");
        // Power follows the color: purple is worth more than grey.
        let cinza = shared::item_id::montaria_no_grau(shared::item_id::MONTARIA_BASE, 1);
        assert!(poder_da_montaria(id, None) > poder_da_montaria(cinza, None));
    }
}
