//! Map Editor — ferramenta dev (NAO faz parte do cliente do jogo).
//!
//! Cria/edita `MapFile`s que o servidor carrega: pinta tiles, posiciona
//! entidades (inimigos, portais, NPCs, boss, vault), define spawn.
//!
//! Run: `cargo run -p editor`

use eframe::egui;
use shared::constants::{tile_id, ENEMY_KINDS};
use shared::mapfile::{MapEntity, MapEntityPlacement, MapFile};

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("editor=info".parse().unwrap()))
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("2dEngine Map Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "2dEngine Map Editor",
        options,
        Box::new(|_cc| Ok(Box::new(EditorApp::new()))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Tile(u16),
    Erase,
    PlaceEntity,
    DeleteEntity,
    SetSpawn,
}

#[derive(Clone, PartialEq)]
enum EntityKindSel {
    Enemy(u16),
    Boss(u16),
    Portal,
    Npc,
    Vault,
}

struct EditorApp {
    map: MapFile,
    tool: Tool,
    entity_sel: EntityKindSel,
    current_path: Option<std::path::PathBuf>,
    zoom: f32,      // pixels por tile
    camera: egui::Vec2,
    // Campos para Portal (edit dialog)
    portal_target_map: String,
    portal_target_x:   f32,
    portal_target_y:   f32,
    npc_name: String,
    selected_entity_idx: Option<usize>,
    dirty: bool,
    status: String,
}

impl EditorApp {
    fn new() -> Self {
        Self {
            map: MapFile::new("novo_mapa", 64, 64),
            tool: Tool::Tile(tile_id::FLOOR),
            entity_sel: EntityKindSel::Enemy(0),
            current_path: None,
            zoom: 24.0,
            camera: egui::Vec2::ZERO,
            portal_target_map: String::from("nexus"),
            portal_target_x: 10.0,
            portal_target_y: 10.0,
            npc_name: String::from("Vendor"),
            selected_entity_idx: None,
            dirty: false,
            status: String::from("novo mapa 64x64"),
        }
    }

    fn new_map(&mut self, w: u32, h: u32) {
        self.map = MapFile::new("novo_mapa", w, h);
        self.current_path = None;
        self.dirty = false;
        self.selected_entity_idx = None;
        self.status = format!("novo mapa {}x{}", w, h);
    }

    /// Gera um Nexus template funcional: sala retangular com paredes,
    /// NPC vendor no meio, vault a esquerda, 4 portais nos cantos.
    fn new_nexus_template(&mut self) {
        let w = 40u32;
        let h = 40u32;
        let mut mf = MapFile::new("nexus", w, h);
        mf.safe_zone = true;
        // Paredes em todo o bordo + interior de floor
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let is_edge = x == 0 || y == 0 || x == w as i32 - 1 || y == h as i32 - 1;
                mf.set(x, y, if is_edge { tile_id::WALL } else { tile_id::FLOOR });
            }
        }
        // Centro como spawn
        let cx = w as f32 * 0.5;
        let cy = h as f32 * 0.5;
        mf.spawn = [cx, cy];
        // NPC Vendor no centro
        mf.entities.push(MapEntityPlacement {
            pos: [cx, cy + 3.0],
            entity: MapEntity::Npc { name: "Vendor".into() },
        });
        // Vault a esquerda
        mf.entities.push(MapEntityPlacement {
            pos: [cx - 6.0, cy],
            entity: MapEntity::Vault,
        });
        // 4 portais nos cantos apontando para o 'overworld' (posicoes placeholder)
        let inset = 3.0;
        let portals = [
            ("overworld_ne", [w as f32 - inset, h as f32 - inset], [120.0, 80.0]),
            ("overworld_nw", [inset,           h as f32 - inset], [40.0,  80.0]),
            ("overworld_se", [w as f32 - inset, inset          ], [120.0, 40.0]),
            ("overworld_sw", [inset,           inset           ], [40.0,  40.0]),
        ];
        for (name, pos, target) in portals {
            mf.entities.push(MapEntityPlacement {
                pos,
                entity: MapEntity::Portal {
                    target_map: name.into(),
                    target_spawn: target,
                },
            });
        }
        // Decoracao: alguns tiles de wood perto do vendor
        let wood_positions = [(cx as i32 + 1, cy as i32 + 2), (cx as i32 - 1, cy as i32 + 2)];
        for (x, y) in wood_positions {
            mf.set(x, y, tile_id::WOOD);
        }
        self.map = mf;
        self.current_path = None;
        self.dirty = true;
        self.selected_entity_idx = None;
        self.status = "Nexus template carregado — ajuste e salve como map.bin".into();
    }

    fn save_as(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("MapFile", &["map"])
            .set_file_name(format!("{}.map", self.map.name));
        if let Some(path) = dialog.save_file() {
            match self.map.save(&path) {
                Ok(_) => {
                    self.status = format!("salvo em {}", path.display());
                    self.current_path = Some(path);
                    self.dirty = false;
                }
                Err(e) => self.status = format!("erro: {e}"),
            }
        }
    }

    fn save(&mut self) {
        if let Some(path) = self.current_path.clone() {
            match self.map.save(&path) {
                Ok(_) => {
                    self.status = format!("salvo em {}", path.display());
                    self.dirty = false;
                }
                Err(e) => self.status = format!("erro: {e}"),
            }
        } else {
            self.save_as();
        }
    }

    fn open(&mut self) {
        let dialog = rfd::FileDialog::new().add_filter("MapFile", &["map"]);
        if let Some(path) = dialog.pick_file() {
            match MapFile::load(&path) {
                Ok(m) => {
                    self.map = m;
                    self.status = format!("aberto {}", path.display());
                    self.current_path = Some(path);
                    self.dirty = false;
                    self.selected_entity_idx = None;
                }
                Err(e) => self.status = format!("erro: {e}"),
            }
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Top bar ---
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Novo").clicked() { self.new_map(64, 64); }
                if ui.button("Novo Nexus").clicked() { self.new_nexus_template(); }
                if ui.button("Abrir").clicked() { self.open(); }
                if ui.button("Salvar").clicked() { self.save(); }
                if ui.button("Salvar como").clicked() { self.save_as(); }
                ui.separator();
                ui.label("Nome:");
                if ui.text_edit_singleline(&mut self.map.name).changed() {
                    self.dirty = true;
                }
                ui.separator();
                ui.checkbox(&mut self.map.safe_zone, "Safe zone (sem combate)");
                ui.separator();
                ui.label(&self.status);
                if self.dirty {
                    ui.colored_label(egui::Color32::YELLOW, "●");
                }
            });
        });

        // --- Left panel: tools ---
        egui::SidePanel::left("tools").resizable(true).default_width(220.0).show(ctx, |ui| {
            ui.heading("Ferramentas");
            ui.separator();

            ui.label("Tiles:");
            let tile_btn = |ui: &mut egui::Ui, app: &mut EditorApp, id: u16, label: &str, color: egui::Color32| {
                let selected = matches!(app.tool, Tool::Tile(x) if x == id);
                let btn = egui::Button::new(egui::RichText::new(label).color(if selected { egui::Color32::BLACK } else { egui::Color32::WHITE }))
                    .fill(if selected { color } else { egui::Color32::from_rgb(35,35,40) });
                if ui.add_sized([180.0, 22.0], btn).clicked() {
                    app.tool = Tool::Tile(id);
                }
            };
            tile_btn(ui, self, tile_id::FLOOR, "Floor",  egui::Color32::from_rgb(130,120,95));
            tile_btn(ui, self, tile_id::WALL,  "Wall",   egui::Color32::from_rgb(60,60,70));
            tile_btn(ui, self, tile_id::DIRT,  "Dirt",   egui::Color32::from_rgb(100,75,50));
            tile_btn(ui, self, tile_id::WATER, "Water",  egui::Color32::from_rgb(50,90,170));
            tile_btn(ui, self, tile_id::WOOD,  "Wood",   egui::Color32::from_rgb(120,80,40));

            ui.separator();
            if ui.selectable_label(matches!(self.tool, Tool::Erase), "Apagar (0)").clicked() {
                self.tool = Tool::Erase;
            }

            ui.separator();
            ui.label("Entidades:");
            if ui.selectable_label(matches!(self.tool, Tool::PlaceEntity), "Colocar entidade").clicked() {
                self.tool = Tool::PlaceEntity;
            }
            if ui.selectable_label(matches!(self.tool, Tool::DeleteEntity), "Remover entidade").clicked() {
                self.tool = Tool::DeleteEntity;
            }
            if ui.selectable_label(matches!(self.tool, Tool::SetSpawn), "Definir spawn").clicked() {
                self.tool = Tool::SetSpawn;
            }

            ui.separator();
            ui.label("Tipo de entidade:");
            egui::ComboBox::from_id_salt("entity_kind")
                .selected_text(match &self.entity_sel {
                    EntityKindSel::Enemy(k) => format!("Enemy {}", k),
                    EntityKindSel::Boss(k)  => format!("Boss {}",  k),
                    EntityKindSel::Portal   => "Portal".into(),
                    EntityKindSel::Npc      => "NPC".into(),
                    EntityKindSel::Vault    => "Vault".into(),
                })
                .show_ui(ui, |ui| {
                    for (i, _) in ENEMY_KINDS.iter().enumerate() {
                        let label = format!("Enemy kind {} ({})", i, enemy_label(i as u16));
                        ui.selectable_value(&mut self.entity_sel, EntityKindSel::Enemy(i as u16), label);
                    }
                    for (i, _) in ENEMY_KINDS.iter().enumerate() {
                        let label = format!("Boss kind {} ({})", i, enemy_label(i as u16));
                        ui.selectable_value(&mut self.entity_sel, EntityKindSel::Boss(i as u16), label);
                    }
                    ui.selectable_value(&mut self.entity_sel, EntityKindSel::Portal, "Portal");
                    ui.selectable_value(&mut self.entity_sel, EntityKindSel::Npc, "NPC");
                    ui.selectable_value(&mut self.entity_sel, EntityKindSel::Vault, "Vault");
                });

            match &self.entity_sel {
                EntityKindSel::Portal => {
                    ui.label("Portal target:");
                    ui.text_edit_singleline(&mut self.portal_target_map);
                    ui.horizontal(|ui| {
                        ui.label("x:");
                        ui.add(egui::DragValue::new(&mut self.portal_target_x).speed(0.5));
                        ui.label("y:");
                        ui.add(egui::DragValue::new(&mut self.portal_target_y).speed(0.5));
                    });
                }
                EntityKindSel::Npc => {
                    ui.label("NPC name:");
                    ui.text_edit_singleline(&mut self.npc_name);
                }
                _ => {}
            }

            ui.separator();
            ui.label(format!("Tamanho: {}x{}", self.map.width, self.map.height));
            ui.label(format!("Entidades: {}", self.map.entities.len()));
            ui.label(format!("Spawn: ({:.1}, {:.1})", self.map.spawn[0], self.map.spawn[1]));
        });

        // --- Right panel: entity list ---
        egui::SidePanel::right("entities").resizable(true).default_width(240.0).show(ctx, |ui| {
            ui.heading("Entidades");
            ui.separator();
            let mut to_delete: Option<usize> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, ent) in self.map.entities.iter().enumerate() {
                    let label = format!("{}: {} @ ({:.1},{:.1})", i, describe_entity(&ent.entity), ent.pos[0], ent.pos[1]);
                    ui.horizontal(|ui| {
                        let selected = self.selected_entity_idx == Some(i);
                        if ui.selectable_label(selected, label).clicked() {
                            self.selected_entity_idx = Some(i);
                        }
                        if ui.small_button("x").clicked() { to_delete = Some(i); }
                    });
                }
            });
            if let Some(i) = to_delete {
                self.map.entities.remove(i);
                self.dirty = true;
                if self.selected_entity_idx == Some(i) { self.selected_entity_idx = None; }
            }
        });

        // --- Central grid ---
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);

            // Background
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(15, 15, 20));

            // Pan com drag do botao do meio (ou RMB)
            if response.dragged_by(egui::PointerButton::Middle) || response.dragged_by(egui::PointerButton::Secondary) {
                self.camera -= response.drag_delta() / self.zoom;
            }
            // Zoom com scroll
            let hover_pos = response.hover_pos();
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 {
                let old_zoom = self.zoom;
                self.zoom = (self.zoom * (1.0 + scroll * 0.002)).clamp(4.0, 128.0);
                if let Some(hp) = hover_pos {
                    // Mantem o ponto sob o cursor fixo ao zoom
                    let center = rect.center().to_vec2();
                    let world_before = (hp.to_vec2() - center) / old_zoom + self.camera;
                    let world_after  = (hp.to_vec2() - center) / self.zoom + self.camera;
                    self.camera += world_before - world_after;
                }
            }

            // Desenha tiles
            let center = rect.center().to_vec2();
            let to_screen = |tx: f32, ty: f32| -> egui::Pos2 {
                let s = egui::Vec2::new(tx - self.camera.x, ty - self.camera.y) * self.zoom + center;
                egui::pos2(s.x, s.y)
            };

            // Apenas tiles visiveis
            let inv = 1.0 / self.zoom;
            let min_tx = ((rect.min.x - center.x) * inv + self.camera.x).floor() as i32 - 1;
            let max_tx = ((rect.max.x - center.x) * inv + self.camera.x).ceil()  as i32 + 1;
            let min_ty = ((rect.min.y - center.y) * inv + self.camera.y).floor() as i32 - 1;
            let max_ty = ((rect.max.y - center.y) * inv + self.camera.y).ceil()  as i32 + 1;

            for ty in min_ty..max_ty {
                for tx in min_tx..max_tx {
                    if tx < 0 || ty < 0 || tx >= self.map.width as i32 || ty >= self.map.height as i32 {
                        continue;
                    }
                    let id = self.map.get(tx, ty);
                    let col = tile_color(id);
                    let a = to_screen(tx as f32, ty as f32);
                    let b = to_screen(tx as f32 + 1.0, ty as f32 + 1.0);
                    painter.rect_filled(egui::Rect::from_two_pos(a, b), 0.0, col);
                }
            }

            // Grid (leve)
            if self.zoom >= 12.0 {
                let grid_col = egui::Color32::from_rgba_premultiplied(60, 60, 70, 40);
                for tx in min_tx..=max_tx {
                    let a = to_screen(tx as f32, min_ty as f32);
                    let b = to_screen(tx as f32, max_ty as f32);
                    painter.line_segment([a, b], egui::Stroke::new(1.0, grid_col));
                }
                for ty in min_ty..=max_ty {
                    let a = to_screen(min_tx as f32, ty as f32);
                    let b = to_screen(max_tx as f32, ty as f32);
                    painter.line_segment([a, b], egui::Stroke::new(1.0, grid_col));
                }
            }

            // Borda do mapa
            let mb = egui::Rect::from_two_pos(
                to_screen(0.0, 0.0),
                to_screen(self.map.width as f32, self.map.height as f32),
            );
            painter.rect_stroke(mb, 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(200, 180, 80)));

            // Desenha entidades
            for (i, placement) in self.map.entities.iter().enumerate() {
                let p = to_screen(placement.pos[0], placement.pos[1]);
                let (col, lbl) = entity_visual(&placement.entity);
                let r = self.zoom * 0.35;
                painter.circle_filled(p, r, col);
                painter.circle_stroke(p, r, egui::Stroke::new(2.0, egui::Color32::BLACK));
                if self.selected_entity_idx == Some(i) {
                    painter.circle_stroke(p, r + 4.0, egui::Stroke::new(2.0, egui::Color32::YELLOW));
                }
                if self.zoom >= 14.0 {
                    painter.text(p + egui::vec2(0.0, -r - 4.0),
                                 egui::Align2::CENTER_BOTTOM,
                                 lbl,
                                 egui::FontId::proportional(11.0),
                                 egui::Color32::WHITE);
                }
            }

            // Desenha spawn
            let sp = to_screen(self.map.spawn[0], self.map.spawn[1]);
            painter.circle_stroke(sp, self.zoom * 0.5, egui::Stroke::new(3.0, egui::Color32::from_rgb(80, 220, 80)));
            painter.text(sp, egui::Align2::CENTER_CENTER, "S", egui::FontId::monospace(14.0), egui::Color32::from_rgb(80, 220, 80));

            // Interacao mouse
            if let Some(hp) = response.hover_pos() {
                let world = (hp.to_vec2() - center) / self.zoom + self.camera;
                let tx = world.x.floor() as i32;
                let ty = world.y.floor() as i32;

                // Hover status bar
                let txt = format!("tile ({}, {})  world ({:.1}, {:.1})", tx, ty, world.x, world.y);
                painter.text(rect.min + egui::vec2(6.0, 4.0),
                             egui::Align2::LEFT_TOP, txt,
                             egui::FontId::monospace(11.0),
                             egui::Color32::from_rgb(200, 200, 210));

                let primary = response.dragged_by(egui::PointerButton::Primary)
                           || response.clicked_by(egui::PointerButton::Primary);

                if primary {
                    match self.tool {
                        Tool::Tile(id) => {
                            if self.map.get(tx, ty) != id {
                                self.map.set(tx, ty, id);
                                self.dirty = true;
                            }
                        }
                        Tool::Erase => {
                            if self.map.get(tx, ty) != tile_id::WALL {
                                self.map.set(tx, ty, tile_id::WALL);
                                self.dirty = true;
                            }
                        }
                        Tool::PlaceEntity => {
                            if response.clicked_by(egui::PointerButton::Primary) {
                                let entity = match &self.entity_sel {
                                    EntityKindSel::Enemy(k) => MapEntity::Enemy { kind: *k },
                                    EntityKindSel::Boss(k)  => MapEntity::Boss  { kind: *k },
                                    EntityKindSel::Portal   => MapEntity::Portal {
                                        target_map: self.portal_target_map.clone(),
                                        target_spawn: [self.portal_target_x, self.portal_target_y],
                                    },
                                    EntityKindSel::Npc      => MapEntity::Npc { name: self.npc_name.clone() },
                                    EntityKindSel::Vault    => MapEntity::Vault,
                                };
                                self.map.entities.push(MapEntityPlacement {
                                    pos: [world.x, world.y],
                                    entity,
                                });
                                self.dirty = true;
                            }
                        }
                        Tool::DeleteEntity => {
                            if response.clicked_by(egui::PointerButton::Primary) {
                                // Encontra entidade mais proxima do clique
                                let mut best: Option<(usize, f32)> = None;
                                for (i, e) in self.map.entities.iter().enumerate() {
                                    let dx = e.pos[0] - world.x;
                                    let dy = e.pos[1] - world.y;
                                    let d2 = dx * dx + dy * dy;
                                    if d2 < 0.7 * 0.7 {
                                        if best.map(|(_, bd)| d2 < bd).unwrap_or(true) {
                                            best = Some((i, d2));
                                        }
                                    }
                                }
                                if let Some((i, _)) = best {
                                    self.map.entities.remove(i);
                                    self.dirty = true;
                                }
                            }
                        }
                        Tool::SetSpawn => {
                            if response.clicked_by(egui::PointerButton::Primary) {
                                self.map.spawn = [world.x, world.y];
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
        });

        // --- Bottom bar: size/hotkeys ---
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Tam:");
                let mut w = self.map.width as i32;
                let mut h = self.map.height as i32;
                if ui.add(egui::DragValue::new(&mut w).range(8..=512).speed(1.0)).changed() ||
                   ui.add(egui::DragValue::new(&mut h).range(8..=512).speed(1.0)).changed() {
                    self.resize(w.max(8) as u32, h.max(8) as u32);
                }
                ui.separator();
                ui.label("Zoom:");
                ui.add(egui::Slider::new(&mut self.zoom, 4.0..=96.0).text("px/tile"));
                ui.separator();
                ui.small("LMB pinta/coloca • RMB/Wheel pan • Scroll zoom");
            });
        });

        // Atalhos
        ctx.input(|i| {
            if i.modifiers.command_only() && i.key_pressed(egui::Key::S) {
                // handled via button; mas atalho ctrl+s
            }
        });
        if ctx.input(|i| i.modifiers.command_only() && i.key_pressed(egui::Key::S)) {
            self.save();
        }
    }
}

impl EditorApp {
    fn resize(&mut self, new_w: u32, new_h: u32) {
        if new_w == self.map.width && new_h == self.map.height { return; }
        let mut new_tiles = vec![tile_id::WALL; (new_w * new_h) as usize];
        let copy_w = self.map.width.min(new_w);
        let copy_h = self.map.height.min(new_h);
        for y in 0..copy_h {
            for x in 0..copy_w {
                let src = (y * self.map.width + x) as usize;
                let dst = (y * new_w + x) as usize;
                new_tiles[dst] = self.map.tiles[src];
            }
        }
        self.map.width = new_w;
        self.map.height = new_h;
        self.map.tiles = new_tiles;
        self.dirty = true;
    }
}

fn tile_color(id: u16) -> egui::Color32 {
    match id {
        x if x == tile_id::FLOOR => egui::Color32::from_rgb(130, 120, 95),
        x if x == tile_id::WALL  => egui::Color32::from_rgb(50, 50, 60),
        x if x == tile_id::DIRT  => egui::Color32::from_rgb(100, 75, 50),
        x if x == tile_id::WATER => egui::Color32::from_rgb(50, 90, 170),
        x if x == tile_id::WOOD  => egui::Color32::from_rgb(120, 80, 40),
        _ => egui::Color32::from_rgb(200, 50, 200), // tile desconhecido
    }
}

fn entity_visual(e: &MapEntity) -> (egui::Color32, &'static str) {
    match e {
        MapEntity::Enemy { kind } => (enemy_color(*kind), enemy_label(*kind)),
        MapEntity::Boss  { .. }   => (egui::Color32::from_rgb(255, 215, 40), "BOSS"),
        MapEntity::Portal { .. }  => (egui::Color32::from_rgb(180, 100, 255), "Portal"),
        MapEntity::Npc    { .. }  => (egui::Color32::from_rgb(80, 200, 255),  "NPC"),
        MapEntity::Vault          => (egui::Color32::from_rgb(230, 180, 90),  "Vault"),
    }
}

fn enemy_color(kind: u16) -> egui::Color32 {
    match kind {
        0 => egui::Color32::from_rgb(220, 220, 220),
        1 => egui::Color32::from_rgb(220, 80, 60),
        2 => egui::Color32::from_rgb(80, 220, 255),
        3 => egui::Color32::from_rgb(180, 60, 255),
        4 => egui::Color32::from_rgb(100, 100, 255),
        5 => egui::Color32::from_rgb(255, 130, 30),
        6 => egui::Color32::from_rgb(60, 220, 80),
        7 => egui::Color32::from_rgb(255, 215, 40),
        _ => egui::Color32::WHITE,
    }
}

fn enemy_label(kind: u16) -> &'static str {
    match kind {
        0 => "Grunt",
        1 => "Tank",
        2 => "Ranger",
        3 => "Ninja",
        4 => "Mago",
        5 => "Berserker",
        6 => "Arqueiro",
        7 => "Boss",
        _ => "?",
    }
}

fn describe_entity(e: &MapEntity) -> String {
    match e {
        MapEntity::Enemy { kind }           => format!("Enemy {} ({})", kind, enemy_label(*kind)),
        MapEntity::Boss  { kind }           => format!("Boss  {} ({})", kind, enemy_label(*kind)),
        MapEntity::Portal { target_map, .. } => format!("Portal -> {}", target_map),
        MapEntity::Npc   { name }           => format!("NPC '{}'", name),
        MapEntity::Vault                    => "Vault".into(),
    }
}
