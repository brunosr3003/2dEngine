//! A PORTA DO PORÃO no cenário: o aviso quando se chega perto e o botão que
//! abre.
//!
//! O Porão deixou o painel em 29/09/2026 e virou dungeon física. Quem entra
//! não abre menu nenhum: anda até a porta, na própria ilha, e abre com a chave
//! que fabricou (`shared::porao`).
//!
//! ## Por que um botão na tela, e não a tecla de interagir
//!
//! F e Espaço já são ATAQUE (`desktop::ataque_pressionado`), e NPC aqui é
//! clique na entidade. Sobra o botão — e ele é o que também funciona no
//! celular, que é metade das plataformas do jogo. A tarja da Ilha Mágica já
//! resolve isso do mesmo jeito.
//!
//! ## Quem decide é o servidor
//!
//! Tudo aqui é aviso. Distância, chave e cadeado são conferidos de novo em
//! `world::dungeon::dg_abrir_porao`, com a posição saindo da entidade. Se esta
//! janela mentir, o servidor recusa — e é assim que tem que ser.

use macroquad::prelude::*;
use shared::dungeon::{self as dg, Pedido};
use shared::protocol::ClientMessage;

use crate::hud_estilo as estilo;

/// A que distância a tarja aparece.
///
/// Bem maior que `shared::porao::ALCANCE_DA_PORTA` (4 u, o de abrir): a porta
/// precisa se anunciar de longe o bastante pra o jogador saber que ela existe,
/// senão uma dungeon inteira fica invisível no meio da ilha. Chegar perto é o
/// que libera o botão.
pub const ALCANCE_DO_AVISO: f32 = 26.0;

/// O que mostrar agora, se houver porta por perto.
#[derive(Debug, Clone, PartialEq)]
pub struct Aviso {
    pub conteudo: u16,
    pub nome: &'static str,
    /// Distância do jogador até a porta.
    pub distancia: f32,
    /// Perto o bastante pra abrir.
    pub na_porta: bool,
    pub tem_chave: bool,
    pub chave: u16,
}

impl Aviso {
    /// Dá pra abrir agora?
    pub fn pode_abrir(&self) -> bool {
        self.na_porta && self.tem_chave
    }

    /// A linha de baixo: o que falta, ou o convite.
    pub fn recado(&self) -> String {
        if !self.tem_chave {
            format!("Needs {}", nome_da_chave(self.chave))
        } else if !self.na_porta {
            "Get closer".into()
        } else {
            "Open".into()
        }
    }
}

/// O `glam` do `shared` e o do macroquad são versões diferentes do mesmo
/// crate, então os dois `Vec2` não se convertem sozinhos. Estas duas pontes
/// existem só pra isso.
fn pra_shared(v: Vec2) -> ::glam::Vec2 {
    ::glam::Vec2::new(v.x, v.y)
}

fn pra_mq(v: ::glam::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

fn nome_da_chave(chave: u16) -> String {
    shared::porao::nome_da_chave(chave).unwrap_or_else(|| "a key".into())
}

/// A porta mais perto do jogador nesta zona, se estiver dentro do aviso.
///
/// Fora do desenho pra poder ser testada: é a regra que decide se uma dungeon
/// inteira aparece ou não.
pub fn aviso_de(
    zona: &str,
    cidade: Option<Vec2>,
    eu: Option<Vec2>,
    tem_chave: &dyn Fn(u16) -> bool,
) -> Option<Aviso> {
    let (cidade, eu) = (cidade?, eu?);
    shared::porao::poroes_da_zona(zona)
        .into_iter()
        .filter_map(|c| {
            let porta = pra_mq(shared::porao::porta_de(c, pra_shared(cidade))?);
            let chave = shared::porao::chave_de(c)?;
            let d = eu.distance(porta);
            (d <= ALCANCE_DO_AVISO).then(|| Aviso {
                conteudo: c.id,
                nome: c.nome,
                distancia: d,
                na_porta: d <= shared::porao::ALCANCE_DA_PORTA,
                tem_chave: tem_chave(chave),
                chave,
            })
        })
        // Duas portas na mesma ilha: vale a mais perto.
        .min_by(|a, b| a.distancia.total_cmp(&b.distancia))
}

#[derive(Default)]
pub struct PoraoUi {
    aviso: Option<Aviso>,
}

impl PoraoUi {
    /// Recalcula e desenha. Devolve o pedido quando o jogador aperta o botão.
    pub fn desenha(
        &mut self,
        zona: &str,
        cidade: Option<Vec2>,
        eu: Option<Vec2>,
        tem_chave: &dyn Fn(u16) -> bool,
    ) -> Option<ClientMessage> {
        self.aviso = aviso_de(zona, cidade, eu, tem_chave);
        let a = self.aviso.clone()?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        let w = 300.0 * f;
        let h = 74.0 * f;
        // Em cima do rodapé e no meio: é onde o olho já procura o que fazer
        // agora, e não disputa com a barra de vida nem com o rastreador.
        let r = Rect::new(
            seguro.center().x - w * 0.5,
            seguro.y + seguro.h - h - 108.0 * f,
            w,
            h,
        );
        estilo::painel(r);
        estilo::texto_centro(
            r.center().x,
            r.y + 26.0 * f,
            a.nome,
            17,
            if a.pode_abrir() { estilo::OURO } else { estilo::TEXTO },
        );
        let m = Vec2::from(mouse_position());
        let botao = Rect::new(r.x + 12.0 * f, r.y + 36.0 * f, r.w - 24.0 * f, 28.0 * f);
        let pode = a.pode_abrir();
        estilo::cartao(botao, pode && botao.contains(m), pode);
        estilo::texto_centro(
            botao.center().x,
            botao.center().y + 5.0 * f,
            &a.recado(),
            14,
            if pode { estilo::OURO } else { estilo::SUAVE },
        );
        if pode && crate::foco::clique() && botao.contains(m) {
            return Some(ClientMessage::Dungeon {
                pedido: Pedido::AbrirPorao {
                    conteudo: a.conteudo,
                },
            });
        }
        None
    }

    /// Onde desenhar o marco no mundo, pra a porta se achar de longe.
    pub fn porta_visivel(zona: &str, cidade: Option<Vec2>) -> Vec<(Vec2, &'static str)> {
        let Some(cidade) = cidade else {
            return Vec::new();
        };
        shared::porao::poroes_da_zona(zona)
            .into_iter()
            .filter_map(|c| {
                shared::porao::porta_de(c, pra_shared(cidade)).map(|p| (pra_mq(p), c.nome))
            })
            .collect()
    }
}

/// A cidade desta zona, que é a âncora das portas.
///
/// Mesmo `Gerador` que o servidor usa, então os dois chegam na MESMA porta —
/// uma tarja que aparecesse a três metros de onde o servidor aceita abrir
/// seria pior que tarja nenhuma.
pub fn cidade_da_zona(zona: &str) -> Option<Vec2> {
    let def = shared::terreno::def_da_zona(zona)?;
    shared::terreno::Gerador::da_ilha(def)
        .cidade()
        .map(|c| pra_mq(c.centro()))
}

/// Prévia da tarja da porta (`MMO_PREVIA_PORAO=1`; PNGs em `MMO_PREVIA_SAIDA`).
///
/// Os três estados que o jogador vê, porque são três telas diferentes: longe
/// com a chave, na porta sem a chave, e na porta com ela.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-porao-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let c = dg::CONTEUDOS
        .iter()
        .find(|c| c.tipo == dg::Tipo::Porao)
        .unwrap();
    let cidade = cidade_da_zona(c.zona).unwrap_or(Vec2::ZERO);
    let porta = pra_mq(shared::porao::porta_de(c, pra_shared(cidade)).unwrap());
    let casos: [(&str, Vec2, bool); 3] = [
        ("porao-longe-com-chave", porta + Vec2::new(18.0, 0.0), true),
        ("porao-na-porta-sem-chave", porta, false),
        ("porao-na-porta-com-chave", porta, true),
    ];
    for (nome, eu, tem) in casos {
        let mut ui = PoraoUi::default();
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha(c.zona, Some(cidade), Some(eu), &move |_| tem);
            unsafe { get_internal_gl().flush() };
            rt.texture
                .get_texture_data()
                .export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn cidade() -> Vec2 {
        Vec2::new(120.0, -60.0)
    }

    fn primeiro_porao() -> &'static dg::Conteudo {
        dg::CONTEUDOS
            .iter()
            .find(|c| c.tipo == dg::Tipo::Porao)
            .unwrap()
    }

    /// LONGE NÃO MOSTRA, PERTO MOSTRA, E SÓ NA PORTA ABRE.
    ///
    /// Os três estados são a regra inteira. O do meio é o que importa: ver a
    /// porta de longe é o que faz uma dungeon física existir pro jogador, e
    /// abrir de longe é o que o servidor recusa.
    #[test]
    fn a_tarja_aparece_de_longe_e_o_botao_so_na_porta() {
        let c = primeiro_porao();
        let porta = pra_mq(shared::porao::porta_de(c, pra_shared(cidade())).unwrap());
        let tem = |_: u16| true;

        let longe = aviso_de(c.zona, Some(cidade()), Some(porta + Vec2::new(500.0, 0.0)), &tem);
        assert!(longe.is_none(), "a tarja apareceu do outro lado da ilha");

        let perto = aviso_de(
            c.zona,
            Some(cidade()),
            Some(porta + Vec2::new(ALCANCE_DO_AVISO - 2.0, 0.0)),
            &tem,
        )
        .expect("a tarja tinha que aparecer de perto");
        assert!(!perto.na_porta, "abriu antes de chegar");
        assert_eq!(perto.recado(), "Get closer");

        let na_porta = aviso_de(c.zona, Some(cidade()), Some(porta), &tem).unwrap();
        assert!(na_porta.pode_abrir());
        assert_eq!(na_porta.recado(), "Open");
    }

    /// SEM CHAVE, A TARJA DIZ QUAL CHAVE — e o botão não abre.
    ///
    /// Um botão que só depois avisa "faltou a chave" é o botão morto que o
    /// resto deste jogo já evitou uma vez (ver `dungeon_ui`, a recusa da
    /// Arena virando convite).
    #[test]
    fn sem_chave_o_botao_diz_qual_falta() {
        let c = primeiro_porao();
        let porta = pra_mq(shared::porao::porta_de(c, pra_shared(cidade())).unwrap());
        let a = aviso_de(c.zona, Some(cidade()), Some(porta), &|_| false).unwrap();
        assert!(!a.pode_abrir(), "abriu sem chave");
        let recado = a.recado();
        assert!(recado.starts_with("Needs "), "{recado}");
        assert!(recado.contains(c.nome), "{recado} não diz de que porta é");
    }

    /// NUMA ILHA SEM PORÃO NÃO HÁ TARJA NENHUMA.
    #[test]
    fn zona_sem_porao_nao_mostra_nada() {
        assert!(aviso_de("ilha_magica", Some(cidade()), Some(cidade()), &|_| true).is_none());
        assert!(aviso_de("dungeon", Some(cidade()), Some(cidade()), &|_| true).is_none());
    }

    /// COM DUAS PORTAS NA ILHA, VALE A MAIS PERTO.
    #[test]
    fn duas_portas_na_mesma_ilha_a_mais_perto_ganha() {
        let poroes = shared::porao::poroes_da_zona("ilha_inicial");
        if poroes.len() < 2 {
            return;
        }
        let a = pra_mq(shared::porao::porta_de(poroes[0], pra_shared(cidade())).unwrap());
        let b = pra_mq(shared::porao::porta_de(poroes[1], pra_shared(cidade())).unwrap());
        let meio = (a + b) * 0.5;
        // Um passo na direção de `a` decide o empate.
        let eu = meio + (a - meio).normalize() * 1.0;
        let visto = aviso_de("ilha_inicial", Some(cidade()), Some(eu), &|_| true);
        if let Some(v) = visto {
            assert_eq!(v.conteudo, poroes[0].id, "escolheu a porta mais longe");
        }
    }
}
