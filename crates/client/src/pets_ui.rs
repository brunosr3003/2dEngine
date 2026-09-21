//! A aba Pets: o bichinho equipado em 3D, o nível, a fome e os slots de
//! skill (docs/PETS.md).
//!
//! Tudo aqui é leitura do que o servidor mandou — o painel não calcula nem
//! gasta nada. Alimentar e instalar skill são `UseItem` na bolsa, que é o
//! mesmo caminho da poção; o botão daqui só manda o pedido.

use macroquad::prelude::*;
use shared::items::PetData;
use shared::protocol::ClientMessage;
use shared::InventorySlot;

use crate::hud_estilo as estilo;
use crate::vox::VoxCache;

#[derive(Default)]
pub struct PetsUi {
    pub aberta: bool,
    /// Gira o modelo no palco.
    giro: f32,
    /// A faixa "meus pets": escolher, equipar, combinar.
    colecao: crate::colecao::Colecao,
}

const SIGLAS: [&str; shared::STAT_COUNT] = ["FOR", "DES", "INT", "VIT", "SPD", "RES"];

impl PetsUi {
    pub fn abrir(&mut self) {
        self.aberta = true;
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
    }

    /// `agora_unix` decide se o pet está com fome. Devolve o pedido de uso da
    /// Ração quando o jogador toca em ALIMENTAR.
    pub fn desenha(
        &mut self,
        vox: &VoxCache,
        solido: &Material,
        equip: &shared::Equipment,
        bolsa: &[InventorySlot],
        agora_unix: i64,
    ) -> Option<ClientMessage> {
        if !self.aberta {
            return None;
        }
        let escala = estilo::escala_do_painel(860.0, 604.0);
        estilo::no_painel(escala, || {
            self.na_escala(vox, solido, equip, bolsa, agora_unix)
        })
    }

    fn na_escala(
        &mut self,
        vox: &VoxCache,
        solido: &Material,
        equip: &shared::Equipment,
        bolsa: &[InventorySlot],
        agora_unix: i64,
    ) -> Option<ClientMessage> {
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = (860.0 * f).min(seguro.w - 16.0);
        let h = (604.0 * f).min(seguro.h - 16.0);
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

        estilo::texto_forte(p.x + 20.0 * f, p.y + 36.0 * f, "PET", 23, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.contains(mouse) {
            self.fechar();
            return None;
        }

        let mut pedido: Option<ClientMessage> = None;
        let Some(pet) = equip.pet.filter(|id| shared::pets::de_item(*id).is_some()) else {
            estilo::texto_ajustado(
                "Nenhum pet equipado. Equipe um no slot Pet da bolsa — ele nasce no mundo e busca o saque do chão pra você.",
                p.x + 24.0 * f,
                p.y + 120.0 * f,
                p.w - 48.0 * f,
                17,
                estilo::SUAVE,
            );
            return None;
        };
        let (especie, grau) = shared::pets::de_item(pet).expect("filtrado acima");
        let d = shared::pets::dados(equip.pet_inst.as_ref());
        let nivel = shared::pets::nivel_de_xp(d.xp);
        let cor = cor_do_grau(grau);

        // ── palco 3D, à esquerda ──
        let palco = Rect::new(p.x + 18.0 * f, p.y + 60.0 * f, 250.0 * f, 250.0 * f);
        estilo::cartao(palco, false, false);
        self.giro += get_frame_time().min(0.1) * 0.6;
        crate::render3d::vitrine_pet(vox, pet, palco, self.giro, solido);
        estilo::texto_centro_forte(
            palco.center().x,
            palco.y + palco.h - 14.0 * f,
            especie.nome,
            19,
            cor,
        );

        // ── nível e experiência ──
        let dir = Rect::new(p.x + 284.0 * f, p.y + 60.0 * f, p.w - 302.0 * f, 250.0 * f);
        estilo::cartao(dir, false, false);
        let mut y = dir.y + 30.0 * f;
        estilo::texto_forte(
            dir.x + 14.0 * f,
            y,
            &format!("Nível {nivel} / {}", shared::pets::NIVEL_MAX),
            19,
            estilo::OURO,
        );
        let no_topo = nivel >= shared::pets::NIVEL_MAX;
        let base = shared::pets::xp_para_nivel(nivel);
        let prox = shared::pets::xp_para_nivel(nivel + 1);
        let frac = if no_topo || prox <= base {
            1.0
        } else {
            ((d.xp - base) as f32 / (prox - base) as f32).clamp(0.0, 1.0)
        };
        y += 16.0 * f;
        estilo::barra(
            Rect::new(dir.x + 14.0 * f, y, dir.w - 28.0 * f, 14.0 * f),
            frac,
            frac,
            estilo::ACENTO,
            None,
        );
        y += 30.0 * f;
        estilo::texto(
            dir.x + 14.0 * f,
            y,
            &if no_topo {
                "Nível máximo.".to_string()
            } else {
                format!(
                    "Faltam {} de experiência para o nível {}",
                    crate::bolsa::milhar(prox.saturating_sub(d.xp)),
                    nivel + 1
                )
            },
            14,
            estilo::SUAVE,
        );

        // ── fome ──
        y += 30.0 * f;
        let cheio = shared::pets::alimentado(&d, agora_unix);
        let (txt, c) = if cheio {
            let falta = (d.alimentado_ate - agora_unix).max(0);
            (
                format!(
                    "Alimentado por mais {}h{:02}",
                    falta / 3600,
                    (falta % 3600) / 60
                ),
                estilo::VERDE,
            )
        } else {
            (
                "Com fome: não ganha experiência nenhuma".to_string(),
                estilo::VERMELHO,
            )
        };
        estilo::texto_forte(dir.x + 14.0 * f, y, &txt, 16, c);
        y += 14.0 * f;
        let racoes = bolsa
            .iter()
            .filter(|s| s.item_id == shared::item_id::RACAO_DE_PET)
            .map(|s| s.qty)
            .sum::<u32>();
        let alimentar = Rect::new(dir.x + 14.0 * f, y, 180.0 * f, 34.0 * f);
        estilo::botao(
            alimentar,
            &format!("Alimentar ({racoes})"),
            estilo::estado_de(alimentar, racoes == 0, false),
            racoes > 0,
        );
        if clique && racoes > 0 && alimentar.contains(mouse) {
            if let Some(i) = bolsa
                .iter()
                .position(|s| s.item_id == shared::item_id::RACAO_DE_PET && s.qty > 0)
            {
                pedido = Some(ClientMessage::UseItem { slot: i as u16 });
            }
        }
        estilo::texto(
            dir.x + 204.0 * f,
            y + 22.0 * f,
            &format!(
                "A Ração dura {}h por uso",
                shared::pets::duracao_da_racao(&d) / 3600
            ),
            13,
            estilo::SUAVE,
        );

        // ── atributos e poder ──
        y += 62.0 * f;
        let pontos = shared::pets::pontos_por_stat(pet, &d);
        let mut x = dir.x + 14.0 * f;
        for (i, pts) in pontos.iter().enumerate() {
            if *pts == 0 {
                continue;
            }
            estilo::texto(x, y, SIGLAS[i], 13, estilo::SUAVE);
            estilo::texto_forte(x, y + 20.0 * f, &format!("+{pts}"), 18, estilo::VERDE);
            x += 62.0 * f;
        }
        let poder = format!(
            "PODER  {}",
            crate::bolsa::milhar(crate::bolsa::poder_do_pet(pet, &d).max(0) as u64)
        );
        estilo::texto_forte(
            dir.x + dir.w - 14.0 * f - estilo::medir_forte(&poder, 18),
            y + 14.0 * f,
            &poder,
            18,
            estilo::OURO,
        );

        // ── meus pets: tudo o que esta' na bolsa ──
        let faixa = Rect::new(p.x + 18.0 * f, p.y + 318.0 * f, p.w - 36.0 * f, 152.0 * f);
        if let Some(msg) = self.colecao.desenha(
            faixa,
            f,
            "MEUS PETS",
            bolsa,
            equip.pet,
            &|id| shared::pets::de_item(id).is_some(),
            &|id| shared::pets::nome_do_item(id).unwrap_or_default(),
            Some((vox, solido)),
        ) {
            pedido = Some(msg);
        }

        // ── slots de skill ──
        let baixo = Rect::new(p.x + 18.0 * f, p.y + 478.0 * f, p.w - 36.0 * f, 112.0 * f);
        estilo::cartao(baixo, false, false);
        estilo::texto_forte(
            baixo.x + 14.0 * f,
            baixo.y + 26.0 * f,
            "SKILLS",
            17,
            estilo::OURO,
        );
        estilo::texto(
            baixo.x + 90.0 * f,
            baixo.y + 25.0 * f,
            "Compradas na Loja e instaladas pela bolsa. O Removedor devolve os slots.",
            13,
            estilo::SUAVE,
        );
        let abertos = shared::pets::slots_de_skill(nivel);
        let sw = (baixo.w - 28.0 * f - 20.0 * f) / 3.0;
        for i in 0..3 {
            let r = Rect::new(
                baixo.x + 14.0 * f + i as f32 * (sw + 10.0 * f),
                baixo.y + 36.0 * f,
                sw,
                66.0 * f,
            );
            let aberto = i < abertos;
            estilo::cartao(r, false, false);
            if !aberto {
                estilo::texto_centro(
                    r.center().x,
                    r.center().y + 6.0 * f,
                    &format!("Abre no nível {}", shared::pets::nivel_do_slot(i)),
                    14,
                    estilo::SUAVE,
                );
                continue;
            }
            match d.skills.get(i).copied().and_then(shared::pets::skill) {
                Some(sk) => {
                    estilo::texto_ajustado(
                        sk.nome,
                        r.x + 12.0 * f,
                        r.y + 30.0 * f,
                        r.w - 24.0 * f,
                        17,
                        estilo::TEXTO,
                    );
                    estilo::texto_ajustado(
                        sk.descricao,
                        r.x + 12.0 * f,
                        r.y + 56.0 * f,
                        r.w - 24.0 * f,
                        13,
                        estilo::VERDE,
                    );
                }
                None => {
                    estilo::texto_centro(
                        r.center().x,
                        r.center().y + 6.0 * f,
                        "Slot livre",
                        15,
                        estilo::SUAVE,
                    );
                }
            }
        }
        pedido
    }
}

fn cor_do_grau(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

/// `MMO_PREVIA_PETS=1`: captura local sem rede, pra conferir layout e
/// legibilidade no celular. So' roda no desktop, pra gerar PNG — nunca entra
/// no fluxo do app, que e' por isso que ela pode usar render target com
/// profundidade (ver o teste em `personagens.rs`).
#[cfg(debug_assertions)]
pub async fn previa(vox: &VoxCache, solido: &Material) {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-pets-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = macroquad::texture::render_target_ex(
        screen_width() as u32,
        screen_height() as u32,
        macroquad::texture::RenderTargetParams {
            depth: true,
            sample_count: 1,
        },
    );
    rt.texture.set_filter(FilterMode::Linear);
    crate::render3d::define_alvo(Some(rt.clone()));
    let mut ui = PetsUi::default();
    ui.abrir();
    let mut equip = shared::Equipment::default();
    let pet = shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 4);
    let agora = 1_700_000_000i64;
    let mut inst = shared::items::ItemInstance::vazia_de_grau(4);
    inst.pet = Some(PetData {
        xp: shared::pets::xp_para_nivel(22) + 400,
        alimentado_ate: agora + 4_200,
        skills: [
            shared::item_id::SKILL_PET_FARO,
            shared::item_id::SKILL_PET_APRENDIZ,
            0,
        ],
    });
    equip.set(shared::EquipSlot::Pet, Some(pet), Some(inst));
    let item = |id, qty| InventorySlot {
        item_id: id,
        qty,
        instance: None,
    };
    let bolsa = vec![
        item(shared::item_id::RACAO_DE_PET, 7),
        item(shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 2), 3),
        item(shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 1), 1),
        item(
            shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 3),
            1,
        ),
    ];
    for _ in 0..3 {
        crate::render3d::camera_padrao();
        clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
        ui.desenha(vox, solido, &equip, &bolsa, agora);
        unsafe { macroquad::window::get_internal_gl().flush() };
        rt.texture
            .get_texture_data()
            .export_png(&format!("{saida}/pets.png"));
        next_frame().await;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O painel e' leitura: sem pet equipado ele nao pede nada, e a barra de
    /// XP nunca estoura nem divide por zero no topo.
    #[test]
    fn sem_pet_nao_ha_pedido_e_o_topo_nao_quebra() {
        let ui = PetsUi::default();
        assert!(!ui.aberta);
        let d = PetData {
            xp: shared::pets::xp_para_nivel(shared::pets::NIVEL_MAX),
            alimentado_ate: 0,
            skills: [0; 3],
        };
        let n = shared::pets::nivel_de_xp(d.xp);
        assert_eq!(n, shared::pets::NIVEL_MAX);
        // No topo, `xp_para_nivel(n + 1)` satura no proprio teto: a conta da
        // barra tem que cair no ramo do "nivel maximo", nao dividir por zero.
        assert_eq!(
            shared::pets::xp_para_nivel(n + 1),
            shared::pets::xp_para_nivel(n)
        );
        assert_eq!(shared::pets::slots_de_skill(n), 3);
    }
}
