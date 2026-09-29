//! Preview of the rewards and progress per stage, inside the dungeon menu.
use crate::{dungeon_ui::Contexto, hud_estilo as e, rolagem::Rolagem};
use macroquad::prelude::*;
use shared::{dungeon as dg, item_id};

enum Linha {
    Titulo(String),
    Texto(String, bool),
    Itens(Vec<u16>),
}
fn recebido(flags: &[bool], estagio: u8) -> Option<bool> {
    flags.get(estagio.checked_sub(1)? as usize).copied()
}
fn status(flags: &[bool], estagio: u8, concluida: &str) -> String {
    match recebido(flags, estagio) {
        Some(true) => concluida.into(),
        Some(false) => "Disponível".into(),
        None => "Consultando progresso…".into(),
    }
}
fn linhas(def: &dg::Conteudo, ce: &dg::ConteudoEstado, estagio: u8) -> Vec<Linha> {
    let nivel = dg::nivel_do_estagio(def, estagio);
    let cor = shared::chaves::faixa(nivel).cor;
    let graus = dg::graus_da_primeira(def, estagio)
        .iter()
        .map(|g| g.nome())
        .collect::<Vec<_>>()
        .join(" / ");
    let mut linhas = vec![
        Linha::Titulo(format!("Primeira conclusão · estágio {estagio}")),
        Linha::Texto(
            status(
                &ce.primeiras_concluidas,
                estagio,
                "Já concluída · bônus único encerrado",
            ),
            true,
        ),
        Linha::Texto(
            "1 equipamento aleatório + 1 chave de craft aleatória.".into(),
            false,
        ),
        Linha::Texto(format!("Equipamento garantido: {graus}."), false),
        Linha::Texto(
            "Entrega no Correio ao vencer com entrada/recompensa.".into(),
            false,
        ),
        Linha::Titulo("Primeira vitória da semana · por conta".into()),
        Linha::Texto(
            status(&ce.semanais_recebidas, estagio, "Já recebeu nesta semana"),
            true,
        ),
        Linha::Texto(
            "+1 equipamento aleatório garantido, enviado ao Correio.".into(),
            false,
        ),
        Linha::Texto(
            "Na estreia, os dois bônus podem ser recebidos juntos.".into(),
            false,
        ),
        Linha::Titulo("Drops possíveis do boss".into()),
        Linha::Texto(
            shared::bosses::chefe(def.chefe)
                .map_or("Chefe", |b| b.nome)
                .into(),
            true,
        ),
        Linha::Texto(
            "Saque no chão; itens e quantidades variam a cada abate.".into(),
            false,
        ),
    ];
    if ce.drops_chefe.is_empty() {
        linhas.push(Linha::Texto(
            "Nenhum drop cadastrado para este chefe.".into(),
            false,
        ));
    } else {
        linhas.push(Linha::Itens(ce.drops_chefe.clone()));
    }
    let (chance, _) = dg::tabela_de_peca(def.tipo, nivel, estagio);
    linhas.extend([
        Linha::Titulo("Baú de conclusão · separado do drop do boss".into()),
        Linha::Texto(
            format!(
                "{} ouro + {} Marcas da Tempestade (sem bônus).",
                dg::ouro_do_bau(def.tipo, nivel, false),
                dg::marcas(def.tipo, false, false)
            ),
            false,
        ),
        Linha::Texto(
            "Também contém cobre, darksteel e material de craft.".into(),
            false,
        ),
        Linha::Texto(
            format!(
                "Equipamento: {:.0}% · até {}.",
                chance * 100.0,
                dg::teto_de_grau(nivel).nome()
            ),
            false,
        ),
        Linha::Texto(
            format!(
                "Chave {}: {:.0}% no total.",
                shared::chaves::nome_da_cor(cor),
                shared::chaves::faixa(nivel).chance * 100.0
            ),
            false,
        ),
        Linha::Texto(
            "Bônus de tempo: +50% de Marcas e material extra.".into(),
            false,
        ),
        Linha::Texto(
            "Ajudante: sem equipamento, chave ou bônus de estreia.".into(),
            false,
        ),
        Linha::Titulo("Equipamento aleatório · uma destas peças".into()),
        Linha::Itens((item_id::ESPADA_E_ESCUDO..=item_id::CINTO).collect()),
        Linha::Titulo(format!(
            "Chave aleatória · cor {}",
            shared::chaves::nome_da_cor(cor)
        )),
        Linha::Itens(
            item_id::CHAVES
                .iter()
                .map(|&b| item_id::chave_na_cor(b, cor))
                .collect(),
        ),
    ]);
    linhas
}
fn altura(l: &Linha) -> f32 {
    match l {
        Linha::Titulo(_) => 36.,
        Linha::Texto(..) => 25.,
        Linha::Itens(ids) => ids.len().div_ceil(2) as f32 * 50.,
    }
}
pub fn desenha(
    c: &Contexto,
    def: &dg::Conteudo,
    ce: &dg::ConteudoEstado,
    estagio: u8,
    area: Rect,
    rolagem: &mut Rolagem,
) {
    let f = e::fator_texto();
    let linhas = linhas(def, ce, estagio);
    let total = linhas.iter().map(altura).sum::<f32>() * f;
    rolagem.quadro(area, total, 50. * f);
    crate::rolagem::recortar(Some(area));
    let mut y = area.y - rolagem.pos;
    for linha in linhas {
        let h = altura(&linha) * f;
        if y + h >= area.y && y < area.y + area.h {
            match linha {
                Linha::Titulo(t) => {
                    e::texto_ajustado(&t, area.x, y + 25. * f, area.w - 22. * f, 15, e::OURO)
                }
                Linha::Texto(t, destaque) => e::texto_ajustado(
                    &t,
                    area.x,
                    y + 18. * f,
                    area.w - 22. * f,
                    13,
                    if destaque { e::AUTO } else { e::TEXTO },
                ),
                Linha::Itens(ids) => {
                    let w = (area.w - 24. * f) * 0.5;
                    for (i, id) in ids.iter().enumerate() {
                        let x = area.x + (i % 2) as f32 * w;
                        let iy = y + (i / 2) as f32 * 50. * f;
                        if iy + 50. * f < area.y || iy > area.y + area.h {
                            continue;
                        }
                        crate::icones::icone(
                            *id,
                            Rect::new(x, iy + 5. * f, 38. * f, 38. * f),
                            None,
                            None,
                        );
                        let nome = c
                            .nomes
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| format!("Item {id}"));
                        e::texto_ajustado(
                            &nome,
                            x + 45. * f,
                            iy + 27. * f,
                            w - 51. * f,
                            12,
                            e::TEXTO,
                        );
                    }
                }
            }
        }
        y += h;
    }
    crate::rolagem::recortar(None);
    rolagem.desenha(area, total);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_e_do_estagio_selecionado_e_ausencia_nao_vira_recebido() {
        assert_eq!(status(&[true, false], 1, "Recebida"), "Recebida");
        assert_eq!(status(&[true, false], 2, "Recebida"), "Disponível");
        assert_eq!(status(&[true], 2, "Recebida"), "Consultando progresso…");
        assert_eq!(recebido(&[true], 0), None);
    }
}
