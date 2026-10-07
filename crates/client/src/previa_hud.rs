#![cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
//! MMO_PREVIA_HUD: Offline visual review of the actual HUD widgets; never connects to a server.
use crate::hud;
use macroquad::prelude::*;

pub async fn abrir(vox: &mut crate::vox::VoxCache) {
    let saida = std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-hud".into());
    std::fs::create_dir_all(&saida).unwrap();
    let fundo = match std::env::var("MMO_PREVIA_HUD_FUNDO") {
        Ok(path) => load_texture(&path).await.ok(),
        Err(_) => None,
    };
    let solido = crate::render3d::material_solido();
    let mut bolsa = crate::bolsa::Bolsa::default();
    bolsa.aberta = true;
    bolsa.nivel = 70;
    bolsa.ouro = 148_250;
    bolsa.energia = 12_840;
    bolsa.stats = Some(shared::base_player_stats());
    // A non-default look and gear with auras: the portrait has to wear both,
    // like the world draws the player (it used to be the bare default body).
    bolsa.aparencia = shared::aparencia::Aparencia {
        rosto: 2,
        cabelo: 1,
        cor_cabelo: 2,
        pele: 2,
        roupa: shared::aparencia::ROUPA_BASE + 3,
        ..Default::default()
    }
    .empacota();
    // The katana in the Stormcaller skin (index 4, stored + 1).
    bolsa.skins = shared::aparencia::Aparencia { armas: [0, 5, 0, 0], ..Default::default() }.empacota_skins();
    let peca = |grau: u8, tier: u8| {
        let mut i = shared::ItemInstance::vazia_de_grau(grau);
        i.tier = tier;
        Some(i)
    };
    bolsa.equip.weapon = Some(shared::item_id::KATANA);
    bolsa.equip.weapon_inst = peca(3, 3);
    bolsa.equip.armor = Some(221);
    bolsa.equip.armor_inst = peca(4, 2);
    for (i, id) in [
        shared::item_id::KATANA,
        shared::item_id::PISTOLAS,
        shared::item_id::COPPER,
        // A skin item: its icon is the character wearing it (`icones::vitrine_skin`).
        shared::aparencia::item_da_skin(shared::aparencia::ROUPA_BASE + 3),
        shared::aparencia::ARMA_SKIN_BASE + 4,
        shared::aparencia::MONTARIA_SKIN_BASE + 1,
    ]
    .into_iter()
    .enumerate()
    {
        bolsa.slots.push(shared::InventorySlot {
            item_id: id,
            qty: if i == 2 { 148 } else { 1 },
            instance: shared::ItemInstance::roll_for(id, 70, || 0.6),
        });
    }
    let mut skills = crate::habilidades::Habilidades::default();
    skills.catalogo = shared::skills::playtest();
    let mut missoes = crate::missoes::Missoes::default();
    missoes.define_log(
        shared::quests::QUESTS
            .iter()
            .take(2)
            .map(|d| shared::quests::QuestNet::from_def(d, shared::quests::quest_status::ACTIVE, 3))
            .collect(),
    );
    for (w, h) in [(1920, 1080), (1440, 900), (1280, 720), (960, 540)] {
        request_new_screen_size(w as f32, h as f32);
        for _ in 0..3 {
            next_frame().await;
        }
        let rt = render_target_ex(
            w,
            h,
            RenderTargetParams {
                depth: true,
                sample_count: 1,
            },
        );
        crate::render3d::define_alvo(Some(rt.clone()));
        crate::hud_layout::define_escala_ui(1.6);
        for cena in ["exploracao", "combate", "bolsa", "menu", "interface", "interface-fim", "graficos", "graficos-fim", "dialogo", "oferta", "livro"] {
            skills.estado(
                if cena == "combate" {
                    vec![(4, 8.0)]
                } else {
                    vec![]
                },
                0.0,
            );
            // The bag waits longer: skin models (outfit, weapon, coat) load
            // one per frame.
            for _ in 0..if cena == "bolsa" { 10 } else { 3 } {
                crate::render3d::camera_padrao();
                clear_background(Color::new(0.24, 0.31, 0.28, 1.0));
                if let Some(t) = &fundo {
                    draw_texture_ex(
                        t,
                        0.,
                        0.,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(vec2(w as f32, h as f32)),
                            ..Default::default()
                        },
                    );
                }
                let z = crate::hud_layout::atual();
                let mut ficha = hud::Ficha::default();
                ficha.mp = Some(740);
                ficha.vigor = Some(180);
                ficha.xp = shared::xp_for_level_with_mult(70, 1);
                ficha.mult_xp = 1;
                hud::draw_ficha(
                    &z,
                    &mut ficha,
                    0.016,
                    "Navegante",
                    70,
                    1840,
                    2400,
                    1000,
                    240,
                    Some(24850),
                );
                hud::draw_topo(&z, true, false, true, false);
                hud::draw_hud(
                    &z,
                    &hud::Info {
                        realm: "Tempest".into(),
                        canal: "Canal 1".into(),
                        zona: "ilha_inicial".into(),
                        jogadores: 42,
                        capacidade: 200,
                    },
                    &hud::Rede {
                        ms: 32.,
                        ..Default::default()
                    },
                    None,
                    0,
                    0,
                    0,
                    0,
                    Vec2::ZERO,
                    &[
                        "Grupo: vamos para a próxima ilha?",
                        "Você recebeu 148 de ouro.",
                    ],
                );
                missoes.desenha_rastreador(&|_| 0, None, 70, 0.4);
                if cena == "combate" {
                    hud::draw_alvo(&z, "Guardião da ilha", 72, 850, 1200, true);
                }
                hud::draw_atacar(&z, cena == "combate");
                hud::draw_dash(&z, if cena == "combate" { 2.5 } else { 0. }, 5.);
                hud::draw_pulo(&z, false);
                hud::draw_sprint(&z, false);
                hud::draw_botao_montaria(&z, false, None, true);
                hud::draw_botao_economia(&z);
                crate::auto_combate::AutoCombate::default().desenha();
                skills.barra(
                    shared::skills::Conjunto::Katana,
                    70,
                    740,
                    &Default::default(),
                );
                hud::draw_rapidos(
                    &z,
                    [0; 4],
                    [0; 4],
                    [false; 4],
                    [None; 4],
                    [false; 4],
                    None,
                    &|_| String::new(),
                );
                hud::draw_exp(&z, &ficha, 70);
                if cena == "bolsa" {
                    bolsa.desenha(vox, &solido, &[]);
                }
                if cena == "menu" {
                    let mut menu = crate::menu::Menu::default();
                    menu.abrir();
                    menu.desenha(&crate::menu::Contexto { nome: "Navegante", nivel:70,
                        poder:Some(24850), arma:"Katana", saldos:&[("Gold",148250)], selos:&[] });
                }
                if cena == "dialogo" {
                    let mut d = crate::dialogo::Dialogo::default();
                    d.abrir(
                        "Mestre de Missões",
                        "Lobos na estrada",
                        &["Os lobos desceram das colinas e atacam quem passa pela estrada do porto. Traga paz de volta a essa trilha."],
                        crate::dialogo::Fim::Oferta { quest_id: 501 },
                        "120 XP · 300 copper".into(),
                    );
                    d.desenha();
                }
                if cena == "oferta" || cena == "livro" {
                    use shared::quests::{quest_by_id, quest_status, QuestNet};
                    let q = |id: u16, st: u8, pr: u32| QuestNet::from_def(quest_by_id(id).unwrap(), st, pr);
                    let nomes = std::collections::HashMap::new();
                    let tem = |_: u16| 0u32;
                    if cena == "oferta" {
                        let mut m = crate::missoes::Missoes::default();
                        m.abre_oferta(None, "Mestre de Missões".into(), vec![q(501, quest_status::ACTIVE, 0), q(502, quest_status::ACTIVE, 0)]);
                        let _ = m.desenha(&nomes, &tem);
                    } else {
                        let log = vec![q(501, quest_status::ACTIVE, 3), q(502, quest_status::READY, 1)];
                        let entregues = std::collections::HashMap::new();
                        let mut livro = crate::menu_missoes::MenuMissoes::default();
                        livro.abrir();
                        let _ = livro.desenha(&crate::menu_missoes::Contexto {
                            log: &log,
                            entregues: &entregues,
                            nivel: 12,
                            faccao: 0,
                            zona: Some("ilha_inicial"),
                            agora_unix: 1_790_000_000,
                            tem: &tem,
                            nomes: &nomes,
                        });
                    }
                }
                if cena == "interface" || cena == "interface-fim" {
                    let mut ui = crate::config_interface::ConfigInterface::default();
                    ui.abrir();
                    if cena == "interface-fim" {
                        ui.rolar_ao_fim();
                    }
                    ui.desenha(1.6, 5);
                }
                if cena == "graficos" || cena == "graficos-fim" {
                    let mut ui = crate::config_graficos::ConfigGraficos::default();
                    ui.abrir();
                    if cena == "graficos-fim" {
                        ui.rolar_ao_fim();
                    }
                    ui.desenha();
                }
                unsafe { get_internal_gl().flush() };
                rt.texture
                    .get_texture_data()
                    .export_png(&format!("{saida}/{cena}-{w}.png"));
                next_frame().await;
                // Lazy outfits (a skin item's icon) load one per frame, like the game.
                vox.atende_um_pendente(crate::render3d::VOXEL).await;
            }
        }
    }
    crate::render3d::define_alvo(None);
}
