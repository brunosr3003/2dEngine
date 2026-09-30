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
    portas: &[(u16, Vec2)],
    eu: Option<Vec2>,
    tem_chave: &dyn Fn(u16) -> bool,
) -> Option<Aviso> {
    let eu = eu?;
    portas
        .iter()
        .filter_map(|(id, porta)| {
            let c = shared::dungeon::conteudo(*id)?;
            let chave = shared::porao::chave_de(c)?;
            let d = eu.distance(*porta);
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
    /// As portas desta zona, achadas uma vez. A busca sonda até 192 pontos no
    /// relevo — barato ao trocar de ilha, caro a 60 quadros por segundo.
    cache: Option<(String, Vec<(u16, Vec2)>)>,
}

/// As portas desta zona, procurando o chão de verdade. Uma vez por zona.
pub fn portas_da_zona(zona: &str) -> Vec<(u16, Vec2)> {
    let Some(def) = shared::terreno::def_da_zona(zona) else {
        return Vec::new();
    };
    let ger = shared::terreno::Gerador::da_ilha(def);
    shared::porao::portas_da_zona(zona, &ger)
        .into_iter()
        .map(|(id, p)| (id, pra_mq(p)))
        .collect()
}

impl PoraoUi {
    /// Recalcula e desenha. Devolve o pedido quando o jogador aperta o botão.
    /// As portas desta zona, do cache. Recalcula só quando a ilha muda.
    pub fn portas(&mut self, zona: &str) -> &[(u16, Vec2)] {
        if self.cache.as_ref().is_none_or(|(z, _)| z != zona) {
            self.cache = Some((zona.to_string(), portas_da_zona(zona)));
        }
        &self.cache.as_ref().unwrap().1
    }

    pub fn desenha(
        &mut self,
        zona: &str,
        eu: Option<Vec2>,
        tem_chave: &dyn Fn(u16) -> bool,
    ) -> Option<ClientMessage> {
        let portas = self.portas(zona).to_vec();
        self.aviso = aviso_de(&portas, eu, tem_chave);
        let a = self.aviso.clone()?;
        let f = estilo::fator_texto();
        let seguro = crate::hud_layout::tela_segura();
        // Os DESAFIOS desta hora, na porta, ANTES de entrar — o mesmo sorteio
        // que o servidor faz na entrada (`shared::desafio::da_hora`).
        let agora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let desafios: Vec<String> = shared::dungeon::conteudo(a.conteudo)
            .filter(|c| shared::desafio::tem_desafio(c))
            .map(|c| {
                shared::desafio::da_hora(c, agora)
                    .iter()
                    .map(|d| d.texto(c))
                    .collect()
            })
            .unwrap_or_default();
        let w = 340.0 * f;
        let h = (74.0 + 20.0 * desafios.len() as f32 + if desafios.is_empty() { 0.0 } else { 8.0 }) * f;
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
        for (i, t) in desafios.iter().enumerate() {
            estilo::texto(
                r.x + 16.0 * f,
                r.y + 50.0 * f + i as f32 * 20.0 * f,
                &format!("› {t}  +{:.0}%", shared::desafio::BONUS_POR_DESAFIO * 100.0),
                13,
                estilo::SUAVE,
            );
        }
        let m = Vec2::from(mouse_position());
        let topo_botao = r.y + r.h - 38.0 * f;
        let botao = Rect::new(r.x + 12.0 * f, topo_botao, r.w - 24.0 * f, 28.0 * f);
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

}

/// Prévia da tarja da porta (`MMO_PREVIA_PORAO=1`; PNGs em `MMO_PREVIA_SAIDA`):
/// na porta com e sem a chave, com os desafios da hora.
#[cfg(debug_assertions)]
pub async fn previa() {
    let saida =
        std::env::var("MMO_PREVIA_SAIDA").unwrap_or_else(|_| "/tmp/tempest-porao-preview".into());
    std::fs::create_dir_all(&saida).unwrap();
    next_frame().await;
    let rt = render_target(screen_width() as u32, screen_height() as u32);
    crate::render3d::define_alvo(Some(rt.clone()));
    let zona = "ilha_inicial";
    let portas = portas_da_zona(zona);
    let (_, porta) = portas[0];
    for (nome, tem) in [("porta-com-chave", true), ("porta-sem-chave", false)] {
        let mut ui = PoraoUi::default();
        for _ in 0..3 {
            crate::render3d::camera_padrao();
            clear_background(Color::new(0.08, 0.12, 0.16, 1.0));
            ui.desenha(zona, Some(porta), &move |_| tem);
            unsafe { get_internal_gl().flush() };
            rt.texture.get_texture_data().export_png(&format!("{saida}/{nome}.png"));
            next_frame().await;
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn primeiro_porao() -> &'static dg::Conteudo {
        dg::CONTEUDOS
            .iter()
            .find(|c| c.tipo == dg::Tipo::Porao)
            .unwrap()
    }

    /// LONGE NÃO MOSTRA, PERTO MOSTRA, E SÓ NA PORTA ABRE.
    ///
    /// Ver a porta de longe é o que faz a dungeon física existir pro jogador;
    /// abrir de longe é o que o servidor recusa.
    #[test]
    fn a_tarja_aparece_de_longe_e_o_botao_so_na_porta() {
        let c = primeiro_porao();
        let portas = portas_da_zona(c.zona);
        let (_, porta) = *portas.iter().find(|(id, _)| *id == c.id).unwrap();
        let tem = |_: u16| true;

        assert!(
            aviso_de(&portas, Some(porta + Vec2::new(500.0, 0.0)), &tem).is_none(),
            "a tarja apareceu do outro lado da ilha"
        );

        let perto = aviso_de(
            &portas,
            Some(porta + Vec2::new(ALCANCE_DO_AVISO - 2.0, 0.0)),
            &tem,
        )
        .expect("a tarja tinha que aparecer de perto");
        assert!(!perto.na_porta, "abriu antes de chegar");
        assert_eq!(perto.recado(), "Get closer");

        let na_porta = aviso_de(&portas, Some(porta), &tem).unwrap();
        assert!(na_porta.pode_abrir());
        assert_eq!(na_porta.recado(), "Open");
    }

    /// SEM CHAVE, A TARJA DIZ QUAL CHAVE — e o botão não abre.
    #[test]
    fn sem_chave_o_botao_diz_qual_falta() {
        let c = primeiro_porao();
        let portas = portas_da_zona(c.zona);
        let (_, porta) = *portas.iter().find(|(id, _)| *id == c.id).unwrap();
        let a = aviso_de(&portas, Some(porta), &|_| false).unwrap();
        assert!(!a.pode_abrir(), "abriu sem chave");
        let recado = a.recado();
        assert!(recado.starts_with("Needs "), "{recado}");
        assert!(recado.contains(c.nome), "{recado} não diz de que porta é");
    }

    /// NUMA ILHA SEM PORÃO NÃO HÁ TARJA NENHUMA.
    #[test]
    fn zona_sem_porao_nao_mostra_nada() {
        for zona in ["ilha_magica", "dungeon"] {
            let portas = portas_da_zona(zona);
            assert!(portas.is_empty(), "{zona} tem porta e não devia");
            assert!(aviso_de(&portas, Some(Vec2::ZERO), &|_| true).is_none());
        }
    }

    /// TODA PORTA DO MAPA ESTÁ EM TERRA — a mesma garantia de `shared::porao`,
    /// conferida no caminho que o CLIENTE usa pra desenhar e pra marcar o mapa.
    #[test]
    fn as_portas_que_o_cliente_desenha_estao_em_terra() {
        for def in shared::terreno::ARQUIPELAGO {
            let ger = shared::terreno::Gerador::da_ilha(&def);
            let portas = portas_da_zona(def.zona);
            assert_eq!(
                portas.len(),
                shared::porao::poroes_da_zona(def.zona).len(),
                "{}: sumiu porta no caminho do cliente",
                def.zona
            );
            for (_, p) in portas {
                assert!(
                    ger.altura(p.x, p.y) > shared::terreno::NIVEL_DO_MAR,
                    "{}: porta na água",
                    def.zona
                );
            }
        }
    }
}
