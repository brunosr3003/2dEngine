//! O BANCO, com o Banqueiro da vila: a bolsa a' esquerda, o banco a' direita.
//! Tocar num item o passa pro outro lado (`VaultDeposit` / `VaultWithdraw`).
//! Embaixo de cada lado, o "+10 espaços" em ouro (`shared::armazem`).
//!
//! Abre quando o servidor manda `VaultOpen` (toque no Banqueiro). Nada aqui
//! decide: o servidor confere a distancia, o espaco e o ouro.

use macroquad::prelude::*;
use shared::armazem;
use shared::protocol::ClientMessage;
use shared::InventorySlot;

use crate::hud_estilo::{self as estilo, u};
use crate::rolagem::Rolagem;

const LARGURA: f32 = 920.0;
const ALTURA: f32 = 600.0;
const COLUNAS: usize = 6;
const VAO: f32 = 6.0;

#[derive(Default)]
pub struct Banco {
    aberto: bool,
    pub cofre: Vec<InventorySlot>,
    pub bolsa_extra: u8,
    pub banco_extra: u8,
    rol_bolsa: Rolagem,
    rol_banco: Rolagem,
}

/// "+10 espaços · 2.000 ouro", ou o teto.
pub fn rotulo_de_expandir(banco: bool, extra: u8) -> String {
    match armazem::custo(banco, extra) {
        Some(c) => format!("+{} espaços · {} ouro", armazem::PASSO, crate::bolsa::milhar(c)),
        None => "Tamanho máximo".to_string(),
    }
}

impl Banco {
    pub fn abrir(&mut self, cofre: Vec<InventorySlot>) {
        self.aberto = true;
        self.cofre = cofre;
        self.rol_bolsa.zera();
        self.rol_banco.zera();
    }

    pub fn fechar(&mut self) {
        self.aberto = false;
    }

    pub fn aberto(&self) -> bool {
        self.aberto
    }

    fn escala() -> f32 {
        estilo::escala_do_painel(LARGURA, ALTURA)
    }

    fn painel() -> Rect {
        let k = Self::escala();
        let s = crate::hud_layout::tela_segura();
        let (w, h) = ((LARGURA * k).min(s.w - 16.0), (ALTURA * k).min(s.h - 16.0));
        Rect::new(s.center().x - w * 0.5, s.center().y - h * 0.5, w, h)
    }

    pub fn pega_mouse(&self) -> bool {
        self.aberto && Self::painel().contains(Vec2::from(mouse_position()))
    }

    /// Desenha; devolve o pedido do quadro.
    pub fn desenha(&mut self, bolsa: &[InventorySlot], ouro: u64) -> Option<ClientMessage> {
        if !self.aberto {
            return None;
        }
        estilo::no_painel(Self::escala(), || self.desenha_na_escala(bolsa, ouro))
    }

    fn desenha_na_escala(&mut self, bolsa: &[InventorySlot], ouro: u64) -> Option<ClientMessage> {
        crate::hud_layout::escurece(0.5);
        let p = Self::painel();
        estilo::painel_destaque(p, estilo::OURO);
        estilo::texto_forte(p.x + u(20.0), p.y + u(36.0), "Banco", 22, estilo::OURO);
        estilo::texto(
            p.x + u(110.0),
            p.y + u(35.0),
            "toque num item para guardar ou retirar",
            14,
            estilo::SUAVE,
        );
        let ouro_txt = format!("Ouro {}", crate::bolsa::milhar(ouro));
        estilo::texto(
            p.x + p.w - u(70.0) - estilo::medir(&ouro_txt, 15),
            p.y + u(35.0),
            &ouro_txt,
            15,
            estilo::OURO,
        );
        let fechar = Rect::new(p.x + p.w - u(52.0), p.y + u(10.0), u(42.0), u(38.0));
        if crate::ui::botao(fechar, "x", true) {
            self.fechar();
            return None;
        }
        let meio = u(16.0);
        let col_w = (p.w - u(40.0) - meio) * 0.5;
        let topo = p.y + u(60.0);
        let alto = p.h - u(70.0);
        let esq = Rect::new(p.x + u(20.0), topo, col_w, alto);
        let dir = Rect::new(esq.x + col_w + meio, topo, col_w, alto);
        let mut pedido = None;
        let (bolsa_extra, banco_extra) = (self.bolsa_extra, self.banco_extra);
        // So' a grade: a carteira (cobre, darksteel) nao vai pro banco.
        let n = crate::bolsa::grade(bolsa, bolsa_extra).min(bolsa.len());
        let bolsa = &bolsa[..n];
        if let Some(r) = lado(esq, "Bolsa", bolsa, false, bolsa_extra, ouro, &mut self.rol_bolsa) {
            pedido = Some(match r {
                Toque::Item(i) => ClientMessage::VaultDeposit { inv_slot: i as u16 },
                Toque::Expandir => ClientMessage::ExpandirArmazem { banco: false },
            });
        }
        let cofre = self.cofre.clone();
        if let Some(r) = lado(dir, "Banco", &cofre, true, banco_extra, ouro, &mut self.rol_banco) {
            pedido = Some(match r {
                Toque::Item(i) => ClientMessage::VaultWithdraw { vault_slot: i as u16 },
                Toque::Expandir => ClientMessage::ExpandirArmazem { banco: true },
            });
        }
        pedido
    }
}

enum Toque {
    Item(usize),
    Expandir,
}

/// Um lado: titulo com a ocupacao, a grade que rola e o botao de expandir.
fn lado(
    r: Rect,
    titulo: &str,
    slots: &[InventorySlot],
    banco: bool,
    extra: u8,
    ouro: u64,
    rolagem: &mut Rolagem,
) -> Option<Toque> {
    estilo::cartao(r, false, false);
    let tamanho = armazem::tamanho(banco, extra).max(slots.len());
    let ocupados = slots.iter().filter(|s| s.qty > 0).count();
    estilo::texto_forte(r.x + u(12.0), r.y + u(26.0), titulo, 18, estilo::TEXTO);
    let ocup = format!("{ocupados}/{tamanho}");
    estilo::texto(
        r.x + r.w - u(12.0) - estilo::medir(&ocup, 15),
        r.y + u(26.0),
        &ocup,
        15,
        if ocupados >= tamanho { estilo::OURO } else { estilo::SUAVE },
    );
    let pe_h = u(48.0);
    let area = Rect::new(r.x + u(8.0), r.y + u(38.0), r.w - u(16.0), r.h - u(38.0) - pe_h - u(8.0));
    let cel = ((area.w - u(14.0) - (COLUNAS as f32 - 1.0) * u(VAO)) / COLUNAS as f32).floor();
    let passo = cel + u(VAO);
    let linhas = tamanho.div_ceil(COLUNAS);
    let total = linhas as f32 * passo;
    let clique = rolagem.quadro(area, total, passo);
    let mut saida = None;
    crate::rolagem::recortar(Some(area));
    for i in 0..tamanho {
        let (col, lin) = (i % COLUNAS, i / COLUNAS);
        let c = Rect::new(
            area.x + col as f32 * passo,
            area.y + lin as f32 * passo - rolagem.pos,
            cel,
            cel,
        );
        if c.y + c.h < area.y || c.y > area.y + area.h {
            continue;
        }
        let s = slots.get(i).filter(|s| s.qty > 0);
        celula(c, s);
        if s.is_some() && clique.is_some_and(|p| c.contains(p) && area.contains(p)) {
            saida = Some(Toque::Item(i));
        }
    }
    crate::rolagem::recortar(None);
    rolagem.desenha(area, total);
    // o pe': expandir
    let b = Rect::new(r.x + u(8.0), r.y + r.h - pe_h, r.w - u(16.0), pe_h - u(8.0));
    let custo = armazem::custo(banco, extra);
    let pode = custo.is_some_and(|c| ouro >= c);
    estilo::botao(
        b,
        &rotulo_de_expandir(banco, extra),
        estilo::estado_de(b, !pode, false),
        pode,
    );
    if custo.is_some() && crate::foco::clique() && b.contains(Vec2::from(mouse_position())) {
        saida = Some(Toque::Expandir);
    }
    saida
}

/// Uma celula: fundo, icone, a borda na cor da peca e a quantidade.
fn celula(c: Rect, s: Option<&InventorySlot>) {
    estilo::ret_arredondado(c, u(6.0), Color::new(0.13, 0.12, 0.15, 1.0));
    let Some(s) = s else {
        return;
    };
    crate::bolsa::icone_do_item(
        Rect::new(c.x + c.w * 0.1, c.y + c.h * 0.1, c.w * 0.8, c.h * 0.8),
        s.item_id,
        1.0,
    );
    if let Some(i) = s.instance {
        let h = shared::items::tier_color_hex(i.grau()).trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).unwrap_or(0xbf_bf_bf);
        let cor = Color::from_rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 230);
        estilo::borda_arredondada(c, u(6.0), 2.0, cor);
    }
    if s.qty > 1 {
        let q = crate::bolsa::curta(s.qty);
        estilo::texto_forte(
            c.x + c.w - estilo::medir_forte(&q, 12) - u(4.0),
            c.y + c.h - u(4.0),
            &q,
            12,
            estilo::TEXTO,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotulo_mostra_preco_e_o_teto() {
        assert_eq!(rotulo_de_expandir(false, 0), "+10 espaços · 2.000 ouro");
        assert_eq!(rotulo_de_expandir(true, 1), "+10 espaços · 4.000 ouro");
        assert_eq!(rotulo_de_expandir(false, 6), "Tamanho máximo");
    }
}
