//! O GUARDA-ROUPA: trocar a aparência em jogo (docs/PERSONAGEM.md).
//!
//! A mesma escolha da criação de personagem, agora com o boneco de verdade na
//! frente. O handler do servidor (`ClientMessage::UpdateVisual`) já existia
//! desde o cliente antigo e **nunca teve quem o chamasse** — esta janela é
//! quem o chama.
//!
//! # Por que ele manda só no fim
//!
//! Cada toque num seletor muda a prévia na hora, mas não vai pra rede: o
//! servidor reenvia a meta pra TODOS que veem o jogador a cada troca
//! (`last_sent.remove`), e mandar a cada clique seria uma tempestade de metas
//! por um jogador mexendo numa seta. Vai no "Apply".

use macroquad::prelude::*;
use shared::aparencia::{Aparencia, GuardaRoupa};

use crate::hud_estilo as estilo;

#[derive(Debug, Default)]
pub struct GuardaRoupaUi {
    pub aberto: bool,
    /// O que está sendo experimentado. `None` = ainda não abriu.
    provando: Option<Aparencia>,
    /// O que está valendo de verdade, pra saber se há o que aplicar.
    vigente: Aparencia,
    /// As skins destravadas, pro seletor de roupa.
    destravadas: Vec<u16>,
    giro: f32,
    mouse_anterior: Option<Vec2>,
    rolagem: crate::rolagem::Rolagem,
}

impl GuardaRoupaUi {
    /// Abre com o que está valendo.
    pub fn abrir(&mut self, g: &GuardaRoupa) {
        self.aberto = true;
        self.vigente = g.aparencia;
        self.provando = Some(g.aparencia);
        self.destravadas = g.desbloqueadas.clone();
        self.giro = 0.18;
        self.mouse_anterior = None;
        self.rolagem.zera();
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.provando = None;
    }

    /// O servidor confirmou (ou o login trouxe): sincroniza.
    pub fn define(&mut self, g: &GuardaRoupa) {
        self.vigente = g.aparencia;
        self.destravadas = g.desbloqueadas.clone();
        if self.aberto {
            self.provando = Some(g.aparencia);
        } else {
            self.provando = None;
        }
    }

    /// O que desenhar na prévia e no próprio jogador enquanto a janela está
    /// aberta — experimentar tem que ser visível antes de aplicar.
    pub fn provando(&self) -> Option<Aparencia> {
        self.provando.filter(|_| self.aberto)
    }

    /// Desenha. Devolve a mensagem quando o jogador aplica.
    pub fn desenha(&mut self, vox: &crate::vox::VoxCache, solido: &Material, arma: u16) -> Option<shared::protocol::ClientMessage> {
        if !self.aberto {
            return None;
        }
        let s = crate::hud_layout::tela_segura();
        let f = estilo::escala_do_painel(840.0, 500.0)
            .min(((s.w - 16.0) / 840.0).max(0.85))
            .min(((s.h - 16.0) / 500.0).max(0.85));
        estilo::no_painel(f, || self.na_escala(vox, solido, arma))
    }

    fn na_escala(&mut self, vox: &crate::vox::VoxCache, solido: &Material, arma: u16) -> Option<shared::protocol::ClientMessage> {
        use shared::aparencia as ap;
        let mut a = self.provando?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let linha = (52.0 * f).max(52.0);
        let w = (840.0 * f).min(seguro.w - 16.0);
        let h = (500.0 * f).min(seguro.h - 16.0);
        let p = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.center().y - h * 0.5,
            w,
            h,
        );
        crate::hud_layout::escurece(0.45);
        estilo::painel_destaque(p, estilo::ACENTO);
        let m = Vec2::from(mouse_position());
        let clicou = crate::foco::clique();
        let x0 = p.x + 20.0 * f;
        estilo::texto_forte(x0, p.y + 36.0 * f, "Appearance and skins", 20, estilo::OURO);
        estilo::texto(
            x0,
            p.y + 60.0 * f,
            "Veja as mudanças antes de aplicar.",
            13,
            estilo::SUAVE,
        );
        let fechar = Rect::new(p.x + p.w - 48.0 * f, p.y + 8.0 * f, 40.0 * f, 40.0 * f);
        estilo::texto_centro(
            fechar.center().x,
            fechar.center().y + 7.0 * f,
            "X",
            18,
            estilo::TEXTO,
        );

        // Os cinco seletores. O de roupa lista só o que foi destravado.
        let corpo = Rect::new(x0, p.y + 80.0 * f, p.w - 40.0 * f, p.h - 146.0 * f);
        let palco = Rect::new(corpo.x, corpo.y, corpo.w * 0.34, corpo.h);
        let opcoes = Rect::new(palco.x + palco.w + 16.0 * f, corpo.y,
            corpo.w - palco.w - 16.0 * f, corpo.h);
        let clique_opcao = self.rolagem.quadro(opcoes, linha * 5.0, linha);
        let mut y = opcoes.y - self.rolagem.pos;
        // Cabelos + "no hair" + os chapéus DESTRAVADOS. Os três dividem a
        // junta da cabeça, então dividem o seletor: um chapéu por cima de um
        // cabelo seria duas peças na mesma junta.
        let chapeus: Vec<u16> = self
            .destravadas
            .iter()
            .copied()
            .filter(|id| *id >= ap::CHAPEU_BASE)
            .collect();
        let cabelos = ap::CABELOS + 1 + chapeus.len() as u8;
        let vestes: Vec<u16> = self
            .destravadas
            .iter()
            .copied()
            .filter(|id| *id < ap::CHAPEU_BASE)
            .collect();
        let roupas = vestes.len() as u8 + 1;
        let atual_roupa = vestes
            .iter()
            .position(|r| *r == a.roupa)
            .map_or(0, |i| i as u8 + 1);
        let atual_cabelo = indice_do_cabelo(a.cabelo, &chapeus);
        let linhas: [(&str, u8, u8); 5] = [
            ("Face", ap::ROSTOS, a.rosto),
            ("Hair", cabelos, atual_cabelo),
            ("Cor", ap::CORES_DE_CABELO.len() as u8, a.cor_cabelo),
            ("Skin tone", ap::TONS_DE_PELE.len() as u8, a.pele),
            ("Skin", roupas, atual_roupa),
        ];
        let mut novos = [0u8; 5];
        crate::rolagem::recortar(Some(opcoes));
        for (i, (rotulo, n, atual)) in linhas.iter().enumerate() {
            novos[i] = *atual;
            let r = Rect::new(opcoes.x, y, opcoes.w - 14.0 * f, linha - 6.0);
            y += linha;
            if r.y + r.h <= opcoes.y || r.y >= opcoes.y + opcoes.h { continue; }
            estilo::cartao(r, false, false);
            estilo::texto(
                r.x + 12.0 * f,
                r.y + r.h * 0.5 + 5.0,
                rotulo,
                14,
                estilo::SUAVE,
            );
            let lado = (38.0 * f).max(40.0);
            let mut v = *atual;
            let esq = Rect::new(
                r.x + r.w * 0.27,
                r.y + 2.0,
                lado,
                r.h - 4.0,
            );
            let dir = Rect::new(r.x + r.w - lado - 6.0 * f, r.y + 2.0, lado, r.h - 4.0);
            estilo::botao(esq, "‹", estilo::estado_de(esq, *n <= 1, false), false);
            estilo::botao(dir, "›", estilo::estado_de(dir, *n <= 1, false), false);
            if let Some(m) = clique_opcao.filter(|m| opcoes.contains(*m) && *n > 1) {
                if esq.contains(m) {
                    v = (v + n - 1) % n;
                }
                if dir.contains(m) {
                    v = (v + 1) % n;
                }
            }
            let nome = nome_da_opcao(i, *atual, &vestes, &chapeus);
            let largura_nome = (dir.x - esq.x - esq.w - 8.0 * f).max(1.0);
            let nome_x = esq.x + esq.w + 4.0 * f
                + (largura_nome - estilo::medir(&nome, 14).min(largura_nome)) * 0.5;
            estilo::texto_ajustado(
                &nome,
                nome_x,
                r.y + r.h * 0.5 + 5.0,
                largura_nome,
                14,
                estilo::TEXTO,
            );
            novos[i] = v;
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(opcoes, linha * 5.0);
        a.rosto = novos[0];
        a.cabelo = match novos[1] {
            // Cabelo de verdade ou "no hair": o índice vale direto.
            k if k <= ap::CABELOS => k,
            // Chapéu: o índice do SELETOR (que só lista os destravados) vira
            // o índice GLOBAL da tabela de chapéus — são coisas diferentes, e
            // confundi-las poria o chapéu errado na cabeça.
            k => chapeus
                .get((k - ap::CABELOS - 1) as usize)
                .and_then(|id| ap::cabelo_do_chapeu(*id))
                .unwrap_or(0),
        };
        a.cor_cabelo = novos[2];
        a.pele = novos[3];
        a.roupa = match novos[4] {
            0 => 0,
            k => vestes.get(k as usize - 1).copied().unwrap_or(0),
        };
        self.provando = Some(a);

        estilo::cartao(palco, false, false);
        let area = Rect::new(palco.x + 4.0, palco.y + 4.0,
            palco.w - 8.0, palco.h - 32.0 * f);
        if is_mouse_button_pressed(MouseButton::Left) && area.contains(m) {
            self.mouse_anterior = Some(m);
        }
        if is_mouse_button_down(MouseButton::Left) {
            if let Some(antes) = self.mouse_anterior {
                self.giro += (m.x - antes.x) * 0.012;
                self.mouse_anterior = Some(m);
            }
        } else { self.mouse_anterior = None; }
        if !crate::render3d::vitrine_aparencia(vox, a, arma, area, self.giro, solido) {
            estilo::texto_centro(area.center().x, area.center().y,
                "Loading character…", 12, estilo::SUAVE);
        }
        estilo::texto_centro(palco.center().x, palco.y + palco.h - 10.0 * f,
            "Drag to rotate", 12, estilo::SUAVE);

        // Aplicar só aparece quando há o que aplicar.
        let mudou = a != self.vigente;
        let b = Rect::new(opcoes.x, p.y + p.h - 56.0 * f, opcoes.w, (42.0 * f).max(40.0));
        estilo::botao(b, "Apply", estilo::estado_de(b, !mudou, false), mudou);
        let mut saida = None;
        if clicou && mudou && b.contains(m) {
            saida = Some(shared::protocol::ClientMessage::UpdateVisual { aparencia: a });
            self.vigente = a;
        }
        if clicou && (fechar.contains(m) || !p.contains(m)) {
            // Fechar DESISTE: o que foi experimentado e não aplicado volta.
            self.provando = Some(self.vigente);
            self.aberto = false;
        }
        saida
    }
}

fn indice_do_cabelo(cabelo: u8, chapeus: &[u16]) -> u8 {
    use shared::aparencia as ap;
    ap::chapeu_do_cabelo(cabelo).map_or(cabelo, |id| {
        chapeus
            .iter()
            .position(|c| *c == id)
            .map_or(ap::CABELOS, |i| ap::CABELOS + 1 + i as u8)
    })
}

fn nome_da_opcao(i: usize, v: u8, vestes: &[u16], chapeus: &[u16]) -> String {
    use shared::aparencia as ap;
    match i {
        1 if v == ap::CABELOS => "no hair".into(),
        1 if v > ap::CABELOS => chapeus
            .get((v - ap::CABELOS - 1) as usize)
            .and_then(|id| ap::nome_da_skin(*id))
            .unwrap_or("no hair")
            .into(),
        2 => ap::CORES_DE_CABELO[(v as usize).min(5)].into(),
        3 => ap::TONS_DE_PELE[(v as usize).min(3)].into(),
        4 if v == 0 => "default".into(),
        4 => vestes
            .get(v as usize - 1)
            .and_then(|id| ap::nome_da_skin(*id))
            .unwrap_or("default")
            .into(),
        _ => format!("{}", v + 1),
    }
}

/// Captura de desktop (MMO_PREVIA_APARENCIA); o jogo usa viewport na tela.
#[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
pub async fn previa(vox: &mut crate::vox::VoxCache) {
    use shared::aparencia as ap;
    let solido = crate::render3d::material_solido();
    for (w, h) in [(1280, 720), (844, 390), (640, 360)] {
        let rt = render_target_ex(w, h, RenderTargetParams { depth: true, ..Default::default() });
        crate::render3d::define_alvo(Some(rt.clone()));
        crate::hud_layout::define_escala_ui(1.6);
        let mut u = GuardaRoupaUi::default();
        u.abrir(&GuardaRoupa {
            aparencia: Aparencia::default(),
            desbloqueadas: (0..ap::ROUPAS.len()).map(|i| ap::ROUPA_BASE + i as u16)
                .chain((0..ap::CHAPEUS.len()).map(|i| ap::CHAPEU_BASE + i as u16)).collect(),
        });
        for (nome, a, giro) in [
            ("padrao", Aparencia::default(), 0.18),
            ("skin", Aparencia { rosto: 2, cabelo: ap::cabelo_do_chapeu(ap::CHAPEU_BASE + 2).unwrap(),
                cor_cabelo: 2, pele: 2, roupa: ap::ROUPA_BASE + 3 }, 0.18),
            ("girado", Aparencia { rosto: 1, cabelo: 1, cor_cabelo: 3, pele: 1,
                roupa: ap::ROUPA_BASE + 1 }, 2.4),
        ] {
            u.provando = Some(a);
            u.giro = giro;
            u.rolagem.pos = if nome == "padrao" { 0.0 } else { 1000.0 };
            for _ in 0..4 {
                crate::render3d::camera_padrao();
                clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
                u.desenha(vox, &solido, shared::item_id::KATANA);
                unsafe { get_internal_gl().flush() };
                rt.texture.get_texture_data().export_png(&format!("/tmp/tempest-aparencia-{w}-{nome}.png"));
                next_frame().await;
                vox.atende_um_pendente(crate::render3d::VOXEL).await;
            }
        }
        crate::loja_tp::captura_aparencia(vox, &solido, w).await;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn g(ap: Aparencia, skins: &[u16]) -> GuardaRoupa {
        GuardaRoupa {
            aparencia: ap,
            desbloqueadas: skins.to_vec(),
        }
    }

    #[test]
    fn confirmacao_do_servidor_corrige_a_previa_aberta() {
        let mut u = GuardaRoupaUi::default();
        let salva = g(Aparencia::default(), &[]);
        u.abrir(&salva);
        u.provando.as_mut().unwrap().roupa = shared::aparencia::ROUPA_BASE;
        u.define(&salva);
        assert_eq!(u.provando(), Some(salva.aparencia));
    }

    /// Fechar sem aplicar DESISTE. Experimentar tem que ser seguro: se
    /// fechar valesse, o jogador que só quis olhar sairia de cara nova.
    #[test]
    fn fechar_sem_aplicar_volta_ao_que_estava() {
        let vigente = Aparencia {
            rosto: 1,
            ..Default::default()
        };
        let mut u = GuardaRoupaUi::default();
        u.abrir(&g(vigente, &[]));
        u.provando = Some(Aparencia {
            rosto: 2,
            ..vigente
        });
        // O que `na_escala` faz ao fechar:
        u.provando = Some(u.vigente);
        u.aberto = false;
        assert_eq!(u.provando, Some(vigente));
        assert!(u.provando().is_none(), "fechado não prova nada");
    }

    /// Provar só vale com a janela ABERTA — senão o jogador ficaria com a
    /// aparência experimentada depois de sair.
    #[test]
    fn provar_so_vale_com_a_janela_aberta() {
        let mut u = GuardaRoupaUi::default();
        u.abrir(&g(Aparencia::default(), &[]));
        assert!(u.provando().is_some());
        u.fechar();
        assert!(u.provando().is_none());
    }

    /// A lista de roupa mostra só o que foi destravado, mais o padrão.
    #[test]
    fn a_roupa_lista_o_padrao_mais_o_destravado() {
        use shared::aparencia as ap;
        let v = [ap::ROUPA_BASE, ap::ROUPA_BASE + 1];
        assert_eq!(nome_da_opcao(4, 0, &v, &[]), "default");
        assert_eq!(nome_da_opcao(4, 1, &v, &[]), ap::ROUPAS[0].1);
        assert_eq!(nome_da_opcao(4, 2, &v, &[]), ap::ROUPAS[1].1);
        // Índice além do destravado não inventa skin.
        assert_eq!(nome_da_opcao(4, 9, &v, &[]), "default");
    }

    /// O seletor de cabelo lista cabelo, "no hair" e os CHAPÉUS — e o
    /// índice do seletor (que só tem os destravados) não é o índice global
    /// da tabela de chapéus. Confundir os dois põe o chapéu errado.
    #[test]
    fn o_seletor_de_cabelo_nao_confunde_indice_local_com_global() {
        use shared::aparencia as ap;
        // Só o terceiro chapéu destravado.
        let chapeus = [ap::CHAPEU_BASE + 2];
        assert_eq!(nome_da_opcao(1, 0, &[], &chapeus), "1");
        assert_eq!(nome_da_opcao(1, ap::CABELOS, &[], &chapeus), "no hair");
        // O primeiro do SELETOR é o terceiro da TABELA.
        assert_eq!(
            nome_da_opcao(1, ap::CABELOS + 1, &[], &chapeus),
            ap::CHAPEUS[2].1
        );
        // E o índice global correspondente é o do chapéu 2, não o do 0.
        let global = ap::cabelo_do_chapeu(ap::CHAPEU_BASE + 2).unwrap();
        assert_eq!(indice_do_cabelo(global, &chapeus), ap::CABELOS + 1);
        assert_ne!(global, ap::CABELOS + 1, "o índice local vazou pro global");
        assert_eq!(ap::chapeu_do_cabelo(global), Some(ap::CHAPEU_BASE + 2));
    }
}
