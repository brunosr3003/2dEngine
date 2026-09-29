//! The Pets tab: the equipped critter in 3D, the level, the hunger and the
//! skill slots (docs/PETS.md).
//!
//! Everything here is a read of what the server sent — the panel neither
//! computes nor spends anything. Feeding and installing a skill are `UseItem`
//! in the bag, which is the same path as the potion; the button here only
//! sends the request.

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
    selecionado: usize,
    skill_slot_escolhido: Option<usize>,
    rolagem_skills: crate::rolagem::Rolagem,
}

const SIGLAS: [&str; shared::STAT_COUNT] = ["FOR", "DES", "INT", "VIT", "SPD", "RES"];

impl PetsUi {
    pub fn abrir(&mut self) {
        self.aberta = true;
    }

    pub fn fechar(&mut self) {
        self.aberta = false;
        self.skill_slot_escolhido = None;
    }

    /// `agora_unix` decides whether the pet is hungry. Returns the request to use
    /// the Feed when the player touches FEED.
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
        let clique = crate::foco::clique() && self.skill_slot_escolhido.is_none();

        estilo::texto_forte(p.x + 20.0 * f, p.y + 36.0 * f, "PET", 23, estilo::OURO);
        let fechar = Rect::new(p.x + p.w - 49.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::botao(fechar, "X", estilo::estado_de(fechar, false, false), false);
        if clique && fechar.contains(mouse) {
            self.fechar();
            return None;
        }

        let mut pedido: Option<ClientMessage> = None;
        let pets = equip.pets();
        let ocupados = pets.iter().filter(|(_, id, _)| id.is_some()).count();
        for i in 0..3 {
            let botao = Rect::new(
                p.x + (130.0 + i as f32 * 91.0) * f,
                p.y + 8.0 * f,
                82.0 * f,
                38.0 * f,
            );
            let ativo = pets[i].1.is_some();
            estilo::botao(
                botao,
                &format!("Pet {}", i + 1),
                estilo::estado_de(botao, !ativo, self.selecionado == i),
                ativo,
            );
            if clique && ativo && botao.contains(mouse) {
                self.selecionado = i;
                self.skill_slot_escolhido = None;
            }
        }
        if pets[self.selecionado].1.is_none() {
            self.selecionado = pets.iter().position(|(_, id, _)| id.is_some()).unwrap_or(0);
        }
        let Some((_, pet, pet_inst)) = pets[self.selecionado]
            .1
            .map(|id| (pets[self.selecionado].0, id, pets[self.selecionado].2))
        else {
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
        let d = shared::pets::dados(pet_inst.as_ref());
        let nivel = shared::pets::nivel_de_xp(d.xp);
        let cor = cor_do_grau(grau);

        // ── 3D stage, on the left ──
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

        // ── level and experience ──
        let dir = Rect::new(p.x + 284.0 * f, p.y + 60.0 * f, p.w - 302.0 * f, 250.0 * f);
        estilo::cartao(dir, false, false);
        let mut y = dir.y + 30.0 * f;
        estilo::texto_forte(
            dir.x + 14.0 * f,
            y,
            &format!(
                "Nível {nivel} / {} · {ocupados}/3 pets · T{} +{}",
                shared::pets::NIVEL_MAX,
                pet_inst.map_or(1, |i| i.tier()),
                pet_inst.map_or(0, |i| i.refinement)
            ),
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
        let af = pet_inst.as_ref().and_then(|i| i.afinidade);
        let pontos = shared::pets::pontos_por_stat_da_instancia(pet, pet_inst.as_ref());
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
            crate::bolsa::milhar(crate::bolsa::poder_dos_pontos(pontos).max(0) as u64)
        );
        estilo::texto_forte(
            dir.x + dir.w - 14.0 * f - estilo::medir_forte(&poder, 18),
            y + 14.0 * f,
            &poder,
            18,
            estilo::OURO,
        );

        // ── my pets: everything in the bag ──
        let faixa = Rect::new(p.x + 18.0 * f, p.y + 318.0 * f, p.w - 36.0 * f, 152.0 * f);
        if let Some(msg) = self.colecao.desenha(
            faixa,
            f,
            "MEUS PETS",
            bolsa,
            Some(pet),
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
            "Toque num slot livre para escolher uma skill da bolsa.",
            13,
            estilo::SUAVE,
        );
        let abertos = shared::pets::slots_de_skill(nivel);
        let sw = (baixo.w - 28.0 * f - 20.0 * f) / 3.0;
        let mut abriu_skill_agora = false;
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
                        "Slot livre · toque para escolher",
                        15,
                        estilo::SUAVE,
                    );
                    if clique && r.contains(mouse) {
                        self.skill_slot_escolhido = Some(i);
                        self.rolagem_skills.zera();
                        abriu_skill_agora = true;
                    }
                }
            }
        }
        if let Some(skill_slot) = self.skill_slot_escolhido.filter(|_| !abriu_skill_agora) {
            if let Some(msg) = self.escolher_skill(p, f, bolsa, &d, skill_slot) {
                pedido = Some(msg);
            }
        }
        pedido
    }

    fn escolher_skill(
        &mut self,
        painel: Rect,
        f: f32,
        bolsa: &[InventorySlot],
        pet: &PetData,
        skill_slot: usize,
    ) -> Option<ClientMessage> {
        crate::hud_layout::escurece(0.65);
        let largura = (510.0 * f).min(painel.w - 24.0);
        let altura = (420.0 * f).min(painel.h - 24.0);
        let r = Rect::new(
            painel.center().x - largura * 0.5,
            painel.center().y - altura * 0.5,
            largura,
            altura,
        );
        estilo::painel_destaque(r, estilo::OURO);
        estilo::texto_forte(
            r.x + 16.0 * f,
            r.y + 32.0 * f,
            &format!("Skill para o slot {}", skill_slot + 1),
            20,
            estilo::OURO,
        );
        let fechar = Rect::new(r.x + r.w - 48.0 * f, r.y + 8.0 * f, 38.0 * f, 36.0 * f);
        if crate::ui::botao(fechar, "X", true) {
            self.skill_slot_escolhido = None;
            return None;
        }
        let disponiveis: Vec<_> = bolsa
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                if item.qty == 0 || pet.skills.contains(&item.item_id) {
                    return None;
                }
                shared::pets::skill(item.item_id).map(|skill| (i, item.qty, skill))
            })
            .collect();
        let area = Rect::new(
            r.x + 14.0 * f,
            r.y + 52.0 * f,
            r.w - 28.0 * f,
            r.h - 66.0 * f,
        );
        if disponiveis.is_empty() {
            estilo::texto_ajustado(
                "Você não tem skills disponíveis na bolsa. Elas podem ser compradas na Loja.",
                area.x + 8.0 * f,
                area.y + 34.0 * f,
                area.w - 16.0 * f,
                15,
                estilo::SUAVE,
            );
            return None;
        }
        let passo = 54.0 * f;
        let total = disponiveis.len() as f32 * passo;
        let toque = self.rolagem_skills.quadro(area, total, passo);
        let mut pedido = None;
        crate::rolagem::recortar(Some(area));
        for (n, (item_slot, quantidade, skill)) in disponiveis.iter().enumerate() {
            let linha = Rect::new(
                area.x,
                area.y + n as f32 * passo - self.rolagem_skills.pos,
                area.w - 10.0 * f,
                48.0 * f,
            );
            if linha.y + linha.h < area.y || linha.y > area.y + area.h {
                continue;
            }
            estilo::cartao(linha, false, toque.is_some_and(|p| linha.contains(p)));
            estilo::texto_forte(
                linha.x + 12.0 * f,
                linha.y + 21.0 * f,
                skill.nome,
                16,
                estilo::TEXTO,
            );
            estilo::texto_ajustado(
                skill.descricao,
                linha.x + 12.0 * f,
                linha.y + 41.0 * f,
                linha.w - 100.0 * f,
                13,
                estilo::VERDE,
            );
            estilo::texto(
                linha.x + linha.w - 70.0 * f,
                linha.y + 28.0 * f,
                &format!("×{quantidade}"),
                14,
                estilo::OURO,
            );
            if toque.is_some_and(|p| linha.contains(p) && area.contains(p)) {
                pedido = Some(ClientMessage::PetSkillEquip {
                    item_slot: *item_slot as u16,
                    pet_slot: self.selecionado as u8,
                    skill_slot: skill_slot as u8,
                });
                self.skill_slot_escolhido = None;
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem_skills.desenha(area, total);
        pedido
    }
}

fn cor_do_grau(grau: u8) -> Color {
    let h = shared::items::tier_color_hex(grau).trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
    Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

/// `MMO_PREVIA_PETS=1`: local capture with no network, to check layout and
/// legibility on a phone. It only runs on desktop, to generate a PNG — it
/// never enters the app's flow, which is why it can use a render target with
/// depth (see the test in `personagens.rs`).
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
        item(
            shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 2),
            3,
        ),
        item(
            shared::item_id::pet_no_grau(shared::item_id::PET_BASE, 1),
            1,
        ),
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

    /// The panel is a read: with no pet equipped it asks for nothing, and the XP
    /// bar never overflows nor divides by zero at the top.
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
        // At the top, `xp_para_nivel(n + 1)` saturates at its own cap: the bar's
        //  maths has to fall into the "max level" branch, not divide by zero.
        assert_eq!(
            shared::pets::xp_para_nivel(n + 1),
            shared::pets::xp_para_nivel(n)
        );
        assert_eq!(shared::pets::slots_de_skill(n), 3);
    }
}
