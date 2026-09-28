//! O Menu Principal (≡), no molde do MIR4 (docs/HUD.md, 3).
//!
//! So' abre pelo botao MENU do HUD — nenhuma tecla abre (decisao do usuario).
//! O X, clicar de novo no ≡ ou Esc fecham. A coluna da esquerda tem o retrato
//! em texto (nome, nivel, Poder, arma) e os SALDOS, que no MIR4 moram aqui e
//! nao no HUD do mundo. A direita, os sistemas por grupo; os que ainda nao
//! existem mostram cadeado e o motivo no hover.
use macroquad::prelude::*;

use crate::hud::{pictograma, selo};
use crate::hud_estilo as estilo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    Bolsa,
    Ficha,
    Habilidades,
    /// O pet coletor (docs/PETS.md).
    Pets,
    Montaria,
    RecuperarXp,
    Missoes,
    TodasMissoes,
    Diarias,
    Conquistas,
    Craft,
    /// A oficina de COMBINAR (`craft_ui`, aba propria).
    ///
    /// Tinha entrada so' dentro do Craft, como a sexta abinha de uma tela de
    /// receitas. O dono: "combinar tem q ser uma aba separada no menu, n
    /// junto com o craft" — nao e' receita, e' arrastar cinco iguais.
    Combinar,
    Forja,
    Encantar,
    Mapa,
    Mobs,
    Aventuras,
    /// Calendario de presenca.
    Presenca,
    /// A colonia (docs/COLONIA.md): colher e melhorar.
    /// O guarda-roupa (docs/PERSONAGEM.md): trocar a aparencia.
    GuardaRoupa,
    /// SÓ aparece DENTRO da colonia (`Menu::desenha` filtra).
    ///
    /// Ele existia e saiu quando o painel virou mural — "pra ficar mais
    /// imersivo". Mas o mural exige ANDAR ate' ele, e a saida da ilha
    /// (`PedidoColonia::Voltar`) mora dentro desse painel: quem chegou e nao
    /// conseguiu andar ficou PRESO na propria ilha, sem porto e sem porta.
    /// O dono ficou. A porta imersiva pode exigir caminhada; a porta de SAIR
    /// nao pode exigir nada.
    MinhaIlha,
    /// A ILHA MÁGICA (`shared::magica`): o evento com passe.
    IlhaMagica,
    Grupo,
    Amigos,
    Correio,
    Clan,
    Lojas,
    /// O banco (leva ao Banqueiro).
    Banco,
    Mercado,
    LojaTp,
    BarraItens,
    /// O que coletar e o raio do AUTO COLETA (toque: sem botao direito).
    Coleta,
    Configuracoes,
    TrocarPersonagem,
    Sair,
}

/// (item, rotulo, motivo do cadeado).
type Linha = (Item, &'static str, Option<&'static str>);

/// Quantos quadradinhos cabem numa linha de grupo. A grade NAO quebra linha:
/// um item a mais entra na coluna do lado.
///
/// Em 21/09/2026 o guarda-roupa precisou de lugar em PERSONAGEM, que ja'
/// estava nos cinco. Em vez de apertar a grade (o quadrado tem piso de 44 px
/// e encolher so' faria a linha vazar), duas coisas mudaram de grupo pra onde
/// elas ja' faziam mais sentido: a MONTARIA foi pra AVENTURA, que e' como se
/// viaja, e RECUPERAR XP foi pra PROGRESSO, que e' do que ele trata.
const POR_LINHA: f32 = 5.0;

/// Os grupos, na ordem da tela.
pub const GRUPOS: [(&str, &[Linha]); 8] = [
    (
        "PERSONAGEM",
        &[
            (Item::Bolsa, "Bolsa", None),
            (Item::Ficha, "Ficha", None),
            (Item::Habilidades, "Habilidades", None),
            (Item::Pets, "Pets", None),
            (Item::GuardaRoupa, "Aparência", None),
        ],
    ),
    (
        "PROGRESSO",
        &[
            (Item::Missoes, "Missões", None),
            (Item::TodasMissoes, "Todas", None),
            (Item::Diarias, "Diárias", None),
            (Item::Conquistas, "Conquistas", Some("Em breve")),
            (Item::RecuperarXp, "Recuperar XP", None),
        ],
    ),
    (
        "OFICINA",
        &[
            (Item::Craft, "Craft", None),
            (Item::Combinar, "Combinar", None),
            (Item::Forja, "Forja", None),
            (Item::Encantar, "Encantar", Some("Em breve")),
            (Item::Coleta, "Coleta", None),
        ],
    ),
    (
        "AVENTURA",
        &[
            (Item::Mapa, "Mapa", None),
            (Item::Mobs, "Mobs", None),
            (Item::Aventuras, "Dungeons", None),
            (Item::MinhaIlha, "Minha Ilha", None),
            (Item::Montaria, "Montaria", None),
        ],
    ),
    (
        "SOCIAL",
        &[
            (Item::Grupo, "Grupo", None),
            (Item::Amigos, "Amigos", None),
            (Item::Correio, "Correio", None),
            (Item::Clan, "Clã", None),
        ],
    ),
    // "Loja" do Menu e' a loja de CASH (Tempest Points), que ainda nao existe.
    // Vendedor NPC nunca vende de longe: "Vendedores" so' leva ate' ele.
    (
        "COMÉRCIO",
        &[
            (Item::LojaTp, "Loja", None),
            (Item::Lojas, "Vendedores", None),
            (Item::Mercado, "Mercado", None),
            (Item::Banco, "Banco", None),
        ],
    ),
    // EVENTO: o que tem HORA, e nao o que esta' sempre la'.
    //
    // Nasceu porque a Ilha Magica nao cabia — AVENTURA ja' estava nos cinco
    // de `POR_LINHA`, e `nenhum_grupo_passa_da_linha` reprovou o sexto. Em
    // vez de apertar a grade, a PRESENCA veio junto: calendario e ilha de
    // evento sao a mesma categoria de coisa, e ela estava em AVENTURA por
    // falta de lugar melhor.
    (
        "EVENTO",
        &[
            (Item::Presenca, "Presença", None),
            (Item::IlhaMagica, "Ilha Mágica", None),
        ],
    ),
    (
        "SISTEMA",
        &[
            (Item::BarraItens, "Barra", None),
            (Item::Configuracoes, "Interface", None),
            (Item::TrocarPersonagem, "Trocar", Some("Em breve")),
            (Item::Sair, "Sair", None),
        ],
    ),
];

/// O que clicar num item faz.
#[derive(Debug, Clone, PartialEq)]
pub enum Clique {
    Abrir(Item),
    /// Bloqueado: so' avisa.
    Aviso(String),
}

pub fn clique_de(l: &Linha) -> Clique {
    match l.2 {
        Some(motivo) => Clique::Aviso(format!("{}: {motivo}.", l.1)),
        None => Clique::Abrir(l.0),
    }
}

/// O que o Menu mostra do personagem.
pub struct Contexto<'a> {
    pub nome: &'a str,
    pub nivel: u32,
    pub poder: Option<i32>,
    pub arma: &'a str,
    /// (rotulo, valor).
    pub saldos: &'a [(&'a str, u64)],
    /// Itens com ponto vermelho.
    pub selos: &'a [Item],
}

#[derive(Default)]
pub struct Menu {
    pub aberto: bool,
    /// Abriu com o botao/dedo AINDA apertado: o clique que abriu nao clica
    /// dentro do menu. Mesma trava do `escolha_npc`, e pelo mesmo motivo.
    espera_soltar: bool,
    rolagem: crate::rolagem::Rolagem,
}

/// Duas colunas só quando ambas comportam cinco alvos de toque.
fn grade(largura: f32) -> (usize, usize, f32) {
    let colunas = if largura >= 2.0 * (44.0 * POR_LINHA + 40.0) + 14.0 {
        2
    } else {
        1
    };
    let coluna = (largura - 14.0 * (colunas - 1) as f32) / colunas as f32;
    let por_linha = (((coluna + 10.0) / 54.0).floor() as usize).clamp(1, POR_LINHA as usize);
    let tamanho = ((coluna - 10.0 * (por_linha - 1) as f32) / por_linha as f32).min(110.0);
    (colunas, por_linha, tamanho)
}

impl Menu {
    pub fn alterna(&mut self) {
        if self.aberto {
            self.fechar();
        } else {
            self.abrir();
        }
    }

    pub fn abrir(&mut self) {
        self.aberto = true;
        self.espera_soltar = is_mouse_button_down(MouseButton::Left);
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
        self.espera_soltar = false;
    }

    /// O clique DESTE quadro vale dentro do menu?
    ///
    /// O botao MENU do HUD (`hud::draw_topo`) e o "x" daqui leem o mesmo
    /// `is_mouse_button_pressed` — e o `draw_topo` roda ANTES do menu no
    /// mesmo quadro. Sem esta trava, um clique que pegasse os dois
    /// retangulos abria e fechava o menu no MESMO quadro: o painel piscava.
    ///
    /// E eles se pegam: o "x" e' de 34x30, mas o `ui::area_de_toque` cresce o
    /// alvo pra 42x44 e o sobe ate' `painel().y + 5`, que e' o canto de cima
    /// do painel — a mesma altura do ≡. Medido com `zonas_com(.., 1.6, ..)`:
    /// na janela padrao de 940x980 o "x" fica em (850,45)+42x44 e o ≡ em
    /// (881,14)+45x34, um sliver de 11x3 px no canto de baixo do ≡; em
    /// 1600x900 a mordida e' de 42x29; em 1920x1080 e 1920x1200 nao ha'
    /// encontro nenhum. Por isso o defeito era "as vezes": dependia da
    /// janela e de onde, dentro do ≡, o clique caiu.
    fn aceita_clique(&mut self, apertado: bool) -> bool {
        if self.espera_soltar && !apertado {
            self.espera_soltar = false;
        }
        !self.espera_soltar
    }

    fn painel() -> Rect {
        // Dentro da area segura: no iPhone o notch e a barra do home cortavam.
        let t = crate::hud_layout::tela_segura();
        let w = (t.w - 80.0).clamp(320.0, 1600.0);
        let h = (t.h - 80.0).clamp(320.0, 900.0);
        Rect::new(t.x + (t.w - w) * 0.5, t.y + (t.h - h) * 0.5, w, h)
    }

    /// Aberto, o Menu pega a tela toda (o mundo nao recebe clique nem roda).
    pub fn pega_mouse(&self) -> bool {
        self.aberto
    }

    pub fn desenha(&mut self, c: &Contexto) -> Option<Clique> {
        if !self.aberto {
            return None;
        }
        let vale = self.aceita_clique(is_mouse_button_down(MouseButton::Left));
        crate::hud_layout::escurece(0.6);
        let p = Self::painel();
        estilo::painel_destaque(p, estilo::OURO);
        let m = Vec2::from(mouse_position());
        estilo::texto_forte(p.x + 20.0, p.y + 34.0, "MENU", 24, estilo::OURO);
        // O botao desenha sempre; so' o clique e' que espera o dedo soltar.
        let fechar = crate::ui::botao(
            Rect::new(p.x + p.w - 46.0, p.y + 12.0, 34.0, 30.0),
            "x",
            true,
        );
        if fechar && vale {
            self.fechar();
            return None;
        }
        estilo::separador(p.x + 14.0, p.y + 50.0, p.w - 28.0);

        // ── coluna da esquerda: personagem e saldos ──
        let esq = Rect::new(
            p.x + 14.0,
            p.y + 60.0,
            (p.w * 0.26).clamp(170.0, 300.0),
            p.h - 74.0,
        );
        estilo::cartao(esq, false, false);
        let cx = esq.center().x;
        estilo::botao_redondo(
            vec2(cx, esq.y + 56.0),
            38.0,
            estilo::OURO,
            estilo::Estado::Normal,
            false,
        );
        estilo::texto_centro_forte(cx, esq.y + 52.0, "LV", 11, estilo::SUAVE);
        estilo::texto_centro_forte(cx, esq.y + 76.0, &c.nivel.to_string(), 26, estilo::TEXTO);
        let mut y = esq.y + 122.0;
        estilo::texto_ajustado(c.nome, esq.x + 14.0, y, esq.w - 28.0, 19, estilo::TEXTO);
        y += 24.0;
        estilo::texto_ajustado(c.arma, esq.x + 14.0, y, esq.w - 28.0, 14, estilo::SUAVE);
        y += 30.0;
        estilo::texto(esq.x + 14.0, y, "PODER", 11, estilo::SUAVE);
        let poder = c
            .poder
            .map(|v| crate::bolsa::milhar(v.max(0) as u64))
            .unwrap_or_else(|| "—".into());
        estilo::texto(
            esq.x + esq.w - 14.0 - estilo::medir(&poder, 18),
            y + 2.0,
            &poder,
            18,
            estilo::OURO,
        );
        y += 16.0;
        estilo::separador(esq.x + 10.0, y, esq.w - 20.0);
        y += 24.0;
        estilo::texto(esq.x + 14.0, y, "SALDOS", 11, estilo::SUAVE);
        for (rotulo, valor) in c.saldos {
            y += 22.0;
            if y > esq.y + esq.h - 8.0 {
                break;
            }
            estilo::texto(esq.x + 14.0, y, rotulo, 14, estilo::TEXTO);
            let v = crate::bolsa::milhar(*valor);
            estilo::texto(
                esq.x + esq.w - 14.0 - estilo::medir(&v, 14),
                y,
                &v,
                14,
                estilo::OURO,
            );
        }

        // ── direita: grupos em duas colunas ──
        let dir = Rect::new(
            esq.x + esq.w + 14.0,
            esq.y,
            p.x + p.w - 14.0 - (esq.x + esq.w + 14.0),
            esq.h,
        );
        let (ncolunas, por_linha, tamanho) = grade(dir.w - 14.0);
        let colunas: Vec<_> = if ncolunas == 2 {
            vec![&GRUPOS[..4], &GRUPOS[4..]]
        } else {
            vec![&GRUPOS[..]]
        };
        let col_w = (dir.w - 14.0 - 14.0 * (ncolunas - 1) as f32) / ncolunas as f32;
        let t = tamanho.min((dir.h / 4.0 - 46.0).max(44.0));
        let altura_grupo =
            |itens: &[Linha]| 46.0 + itens.len().div_ceil(por_linha) as f32 * (t + 10.0) - 10.0;
        let total = colunas
            .iter()
            .map(|grupos| {
                grupos
                    .iter()
                    .map(|(_, itens)| altura_grupo(itens))
                    .sum::<f32>()
            })
            .fold(0.0, f32::max);
        let toque = self.rolagem.quadro(dir, total, t + 46.0).filter(|_| vale);
        let mut saida = None;
        let mut dica: Option<(Rect, String)> = None;
        crate::rolagem::recortar(Some(dir));
        for (k, grupos) in colunas.iter().enumerate() {
            let x0 = dir.x + k as f32 * (col_w + 14.0);
            let mut gy = dir.y - self.rolagem.pos;
            for (nome, itens) in grupos.iter() {
                estilo::texto_forte(x0, gy + 16.0, nome, 12, estilo::SUAVE);
                gy += 24.0;
                for (i, l) in itens.iter().enumerate() {
                    let r = Rect::new(
                        x0 + (i % por_linha) as f32 * (t + 10.0),
                        gy + (i / por_linha) as f32 * (t + 10.0),
                        t,
                        t,
                    );
                    let sobre = r.contains(m) && dir.contains(m) && !self.rolagem.arrastando();
                    let travado = l.2.is_some();
                    // O foco do tutorial anda ate' aqui: "Menu > Ficha".
                    match l.0 {
                        Item::Ficha => crate::foco::marca(crate::foco::chave::MENU_FICHA, r),
                        Item::Habilidades => crate::foco::marca(crate::foco::chave::MENU_SKILLS, r),
                        Item::MinhaIlha => {
                            crate::foco::marca(crate::foco::chave::MENU_MINHA_ILHA, r)
                        }
                        Item::IlhaMagica => {
                            crate::foco::marca(crate::foco::chave::MENU_ILHA_MAGICA, r)
                        }
                        _ => {}
                    }
                    estilo::cartao(r, sobre && !travado, false);
                    let cor = if travado {
                        Color::new(0.45, 0.47, 0.50, 1.0)
                    } else if sobre {
                        estilo::OURO
                    } else {
                        estilo::TEXTO
                    };
                    icone_do_item(l.0, vec2(r.center().x, r.y + r.h * 0.42), r.w * 0.22, cor);
                    estilo::texto_ajustado(l.1, r.x + 4.0, r.y + r.h - 7.0, r.w - 8.0, 12, cor);
                    if travado {
                        cadeado(vec2(r.x + r.w - 11.0, r.y + 12.0), 6.0);
                        if sobre {
                            dica = Some((r, format!("{} · {}", l.1, l.2.unwrap_or(""))));
                        }
                    } else if c.selos.contains(&l.0) {
                        selo(r);
                    }
                    if toque.is_some_and(|p| dir.contains(p) && r.contains(p)) {
                        saida = Some(clique_de(l));
                    }
                }
                gy += altura_grupo(itens) - 24.0;
            }
        }
        crate::rolagem::recortar(None);
        self.rolagem.desenha(dir, total);
        if let Some((r, texto)) = dica {
            estilo::tooltip(r, &texto, false);
        }
        saida
    }
}

fn cadeado(c: Vec2, s: f32) {
    let cor = Color::new(0.85, 0.74, 0.50, 1.0);
    if crate::icones_ui::ui("cadeado", c, s * 2.6, cor) {
        return;
    }
    estilo::ret_arredondado(
        Rect::new(c.x - s, c.y - s * 0.2, s * 2.0, s * 1.5),
        s * 0.35,
        cor,
    );
    estilo::arco(
        c - vec2(0.0, s * 0.2),
        s * 0.7,
        std::f32::consts::PI,
        0.5,
        1.8,
        cor,
    );
}

fn icone_do_item(item: Item, c: Vec2, s: f32, cor: Color) {
    let nome = match item {
        Item::Bolsa => "bolsa",
        Item::Ficha => "ficha",
        Item::Pets => "montaria",
        Item::Habilidades => "habilidades",
        Item::Montaria => "montaria",
        Item::RecuperarXp => "recuperar_xp",
        Item::Missoes => "missoes",
        Item::TodasMissoes => "todas_missoes",
        Item::Diarias => "diarias",
        Item::Conquistas => "conquistas",
        Item::Craft => "craft",
        Item::Combinar => "combinar",
        Item::IlhaMagica => "ilha_magica",
        Item::Forja => "forja",
        Item::Encantar => "encantar",
        Item::Mapa => "mapa",
        Item::Mobs => "mobs",
        Item::Aventuras => "aventuras",
        Item::Presenca => "presenca",
        Item::GuardaRoupa => "ficha",
        Item::MinhaIlha => "mapa",
        Item::Grupo => "grupo",
        Item::Amigos => "amigos",
        Item::Correio => "correio",
        Item::Clan => "clan",
        Item::Lojas => "lojas",
        Item::Banco => "banco",
        Item::Mercado => "mercado",
        Item::LojaTp => "loja_tp",
        Item::BarraItens => "barra_itens",
        Item::Coleta => "coleta",
        Item::Configuracoes => "configuracoes",
        Item::TrocarPersonagem => "trocar_personagem",
        Item::Sair => "sair",
    };
    if crate::icones_ui::ui(nome, c, s * 2.6, cor) {
        return;
    }
    match item {
        Item::Bolsa => pictograma(0, c, s, cor),
        Item::Missoes | Item::TodasMissoes | Item::Conquistas => pictograma(1, c, s, cor),
        Item::Diarias => pictograma(5, c, s, cor),
        Item::Grupo | Item::Amigos | Item::Clan => pictograma(2, c, s, cor),
        Item::Correio => pictograma(3, c, s, cor),
        Item::Craft | Item::Combinar => estilo::icone(3, c, s * 1.1, cor),
        Item::Forja | Item::Encantar => estilo::icone(1, c, s * 1.1, cor),
        Item::Habilidades => estilo::icone(6, c, s * 1.1, cor),
        Item::Aventuras => estilo::icone(7, c, s * 1.1, cor),
        _ => {
            let letra: String = format!("{item:?}")
                .chars()
                .next()
                .unwrap_or('?')
                .to_string();
            draw_circle_lines(c.x, c.y, s * 1.1, 1.5, cor);
            estilo::texto_centro(
                c.x,
                c.y + s * 0.5,
                &letra,
                (s * 1.3).clamp(12.0, 40.0) as u16,
                cor,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_cabe_sem_sobrepor_aparencia_a_coluna_seguinte() {
        for largura in [180.0, 260.0, 400.0, 540.0, 900.0, 1200.0] {
            let (colunas, n, t) = grade(largura);
            let coluna = (largura - (colunas - 1) as f32 * 14.0) / colunas as f32;
            assert!(n as f32 * t + (n - 1) as f32 * 10.0 <= coluna + 0.01);
            assert!(t >= 44.0);
        }
        assert_eq!(grade(400.0).0, 1);
        assert_eq!(grade(900.0).0, 2);
    }

    #[test]
    fn todo_sistema_que_existe_abre_e_o_resto_so_avisa() {
        let abre = [
            Item::Grupo,
            Item::Amigos,
            Item::Correio,
            Item::Clan,
            Item::Bolsa,
            Item::Ficha,
            Item::Pets,
            Item::Missoes,
            Item::TodasMissoes,
            Item::Diarias,
            Item::Craft,
            Item::Combinar,
            Item::Forja,
            Item::Habilidades,
            Item::Mapa,
            Item::Mobs,
            Item::Lojas,
            Item::Banco,
            Item::Mercado,
            Item::Aventuras,
            Item::Presenca,
            Item::MinhaIlha,
            Item::IlhaMagica,
            Item::GuardaRoupa,
            Item::LojaTp,
            Item::Montaria,
            Item::RecuperarXp,
            Item::BarraItens,
            Item::Coleta,
            Item::Configuracoes,
            Item::Sair,
        ];
        for (_, itens) in GRUPOS.iter() {
            for l in itens.iter() {
                match clique_de(l) {
                    Clique::Abrir(i) => assert!(abre.contains(&i), "{:?} abre mas nao existe", i),
                    Clique::Aviso(t) => {
                        assert!(!abre.contains(&l.0), "{:?} existe mas esta' travado", l.0);
                        assert!(t.contains("Em breve"), "aviso sem motivo: {t}");
                    }
                }
            }
        }
        // "Loja" e' a de cash (TP); vendedor NPC so' com "Ir".
        let loja = GRUPOS
            .iter()
            .flat_map(|(_, it)| it.iter())
            .find(|l| l.1 == "Loja")
            .expect("sem Loja");
        assert_eq!(loja.0, Item::LojaTp);
        assert_eq!(clique_de(loja), Clique::Abrir(Item::LojaTp));
        for i in abre {
            assert!(
                GRUPOS.iter().any(|(_, it)| it.iter().any(|l| l.0 == i)),
                "{i:?} fora do menu"
            );
        }
    }

    /// O clique que ABRE o menu nao pode clicar dentro dele.
    ///
    /// O quadro em que o ≡ do HUD foi apertado e' o MESMO em que o menu
    /// desenha, e `is_mouse_button_pressed` ainda esta' de pe': o "x" via
    /// esse clique e fechava na hora — o painel aparecia por um quadro so'.
    #[test]
    fn o_clique_que_abre_nao_fecha() {
        let mut m = Menu::default();
        // Abriu com o botao apertado (o proprio clique no ≡).
        m.aberto = true;
        m.espera_soltar = true;
        assert!(!m.aceita_clique(true), "o clique que abriu nao pode valer");
        // Ainda segurando: continua sem valer.
        assert!(!m.aceita_clique(true));
        // Soltou: o proximo clique ja' e' outro clique.
        assert!(m.aceita_clique(false), "soltou, a trava sai");
        assert!(m.aceita_clique(true), "clique novo vale");
    }

    /// Aberto pelo teclado (Esc volta ao menu) nao ha' o que esperar.
    #[test]
    fn sem_dedo_na_tela_o_primeiro_clique_ja_vale() {
        let mut m = Menu::default();
        m.aberto = true;
        m.espera_soltar = false;
        assert!(m.aceita_clique(true));
    }

    /// A grade dimensiona o quadradinho por `POR_LINHA`; um item a mais que
    /// isso sairia da coluna e entraria na do lado.
    #[test]
    fn nenhum_grupo_passa_da_linha() {
        assert!(GRUPOS
            .iter()
            .all(|(nome, it)| it.len() <= POR_LINHA as usize
                || panic!("{nome} tem {} itens", it.len())));
    }
}
