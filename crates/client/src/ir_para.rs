//! "Ir para": do menu do mapa ou do menu de missoes, o personagem vai sozinho
//! ate' um LUGAR e, ao chegar, faz o que o lugar pede — liga o auto combate na
//! zona do bicho, a auto coleta na regiao do recurso, ou fala com o NPC.
//!
//! A viagem e' a do mapa (etapas, tracejado no chao, corrida automatica): aqui
//! so' se decide o proximo passo, sem macroquad no nucleo.
use macroquad::prelude::*;

/// A viagem acabou longe do alvo: pede de novo depois disso.
const RELIGA_S: f64 = 1.0;
/// Perto disto do NPC: chegou (o "ir ate' o NPC" da loja fecha o caminho).
const PERTO_DO_NPC: f32 = 4.5;
/// Zona e regiao sao grandes; chegar e' entrar nelas, nao pisar no centro.
const CHEGOU_MAX: f32 = 12.0;
/// Pedidos seguidos sem chegar mais perto que isto: desiste.
const DESISTE_APOS: u32 = 8;
const PROGRESSO_MINIMO: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Objetivo {
    /// Zona de bicho: auto combate centrado nela.
    Combate,
    /// Regiao de recurso do tipo (0 madeira, 1..4 pedra pela cor).
    Coleta(u8),
    /// Um NPC: fala com ele ao chegar.
    Npc,
    /// So' chegar la' (chefe muito acima do nivel: nada liga sozinho).
    Lugar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alvo {
    pub objetivo: Objetivo,
    pub pos: Vec2,
    pub raio: f32,
    /// "Lobo", "Pedra azul", "Mestre de Missões": o que o HUD mostra.
    pub rotulo: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Acao {
    Viajar(Vec2),
    LigarCombate(Vec2),
    LigarColeta(u8, Vec2),
    /// Falar com o NPC que estiver perto deste ponto.
    FalarPerto(Vec2),
    Aviso(String),
}

#[derive(Debug, Default)]
pub struct IrPara {
    alvo: Option<Alvo>,
    desde: f64,
    pedidos_sem_progresso: u32,
    melhor: f32,
    /// "Nada está mudando" — ver `parado.rs`.
    parado: crate::parado::Parado,
}

impl IrPara {
    pub fn ativo(&self) -> bool {
        self.alvo.is_some()
    }

    pub fn alvo(&self) -> Option<&Alvo> {
        self.alvo.as_ref()
    }

    pub fn iniciar(&mut self, alvo: Alvo, agora: f64) {
        *self = Self {
            alvo: Some(alvo),
            desde: agora - RELIGA_S,
            pedidos_sem_progresso: 0,
            melhor: f32::MAX,
            parado: crate::parado::Parado::default(),
        };
    }

    pub fn parar(&mut self) {
        *self = Self::default();
    }

    fn alcance(a: &Alvo) -> f32 {
        match a.objetivo {
            Objetivo::Npc => PERTO_DO_NPC,
            _ => a.raio.clamp(3.0, CHEGOU_MAX),
        }
    }

    /// Um quadro. `viajando` = a viagem do mapa ainda esta' andando.
    pub fn passo(&mut self, eu: Vec2, agora: f64, viajando: bool) -> Option<Acao> {
        let a = self.alvo.as_ref()?;
        let d = eu.distance(a.pos);
        if d <= Self::alcance(a) {
            let acao = match a.objetivo {
                Objetivo::Combate => Acao::LigarCombate(a.pos),
                Objetivo::Coleta(t) => Acao::LigarColeta(t, a.pos),
                Objetivo::Npc => Acao::FalarPerto(a.pos),
                Objetivo::Lugar => Acao::Aviso(format!("Chegou: {}.", a.rotulo)),
            };
            self.parar();
            return Some(acao);
        }
        // TRAVADO CONTA MESMO "VIAJANDO".
        //
        // O `viajando` sozinho fazia este passo nunca rodar enquanto o corpo
        // empurrava uma parede: a viagem segue "ativa" e a conta de
        // `pedidos_sem_progresso` — que é quem desiste — nunca avançava.
        //
        // Terceiro arquivo com a mesma suposição (ver `parado.rs`): o
        // `auto_missao.rs` e o `auto_coleta.rs` tinham cópia dela.
        self.parado.acompanha(eu, agora);
        let travado = self.parado.travado(agora);
        if (viajando && !travado) || agora - self.desde < RELIGA_S {
            return None;
        }
        if travado {
            self.parado.zera(agora);
        }
        self.desde = agora;
        if d < self.melhor - PROGRESSO_MINIMO {
            self.melhor = d;
            self.pedidos_sem_progresso = 0;
        } else {
            self.pedidos_sem_progresso += 1;
            if self.pedidos_sem_progresso >= DESISTE_APOS {
                let rotulo = a.rotulo.clone();
                self.parar();
                return Some(Acao::Aviso(format!("Não achei caminho até {rotulo}.")));
            }
        }
        // NPC: pare do lado dele, nao em cima.
        let destino = match a.objetivo {
            Objetivo::Npc => a.pos + (eu - a.pos).normalize_or_zero() * 2.0,
            _ => a.pos,
        };
        Some(Acao::Viajar(destino))
    }

    /// O texto da faixa de estado unica do HUD.
    pub fn faixa(&self, eu: Option<Vec2>) -> Option<String> {
        let (Some(a), Some(eu)) = (&self.alvo, eu) else {
            return None;
        };
        Some(format!("INDO · {} · {:.0} m", a.rotulo, eu.distance(a.pos)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alvo(objetivo: Objetivo, pos: Vec2, raio: f32) -> Alvo {
        Alvo {
            objetivo,
            pos,
            raio,
            rotulo: "x".into(),
        }
    }

    #[test]
    fn longe_viaja_e_perto_liga_o_que_o_lugar_pede() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Combate, vec2(300.0, 0.0), 45.0), 0.0);
        assert_eq!(
            ir.passo(Vec2::ZERO, 0.0, false),
            Some(Acao::Viajar(vec2(300.0, 0.0)))
        );
        // Viajando: nao pede de novo.
        assert_eq!(ir.passo(vec2(100.0, 0.0), 0.5, true), None);
        // Dentro da zona (ate' 12 do centro): liga o combate e acaba.
        assert_eq!(
            ir.passo(vec2(290.0, 0.0), 3.0, true),
            Some(Acao::LigarCombate(vec2(300.0, 0.0)))
        );
        assert!(!ir.ativo());

        ir.iniciar(alvo(Objetivo::Coleta(3), vec2(0.0, 50.0), 8.0), 0.0);
        assert_eq!(
            ir.passo(vec2(0.0, 44.0), 0.0, false),
            Some(Acao::LigarColeta(3, vec2(0.0, 50.0)))
        );
    }

    #[test]
    fn npc_para_do_lado_e_fala_ao_chegar() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Npc, vec2(20.0, 0.0), 0.0), 0.0);
        assert_eq!(
            ir.passo(Vec2::ZERO, 0.0, false),
            Some(Acao::Viajar(vec2(18.0, 0.0)))
        );
        assert_eq!(
            ir.passo(vec2(17.0, 0.0), 2.0, false),
            Some(Acao::FalarPerto(vec2(20.0, 0.0)))
        );
    }

    #[test]
    fn sem_chegar_mais_perto_desiste() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Combate, vec2(500.0, 0.0), 45.0), 0.0);
        let mut aviso = None;
        for k in 0..20 {
            match ir.passo(vec2(100.0, 0.0), k as f64 * 1.1, false) {
                Some(Acao::Aviso(s)) => {
                    aviso = Some(s);
                    break;
                }
                Some(Acao::Viajar(_)) => {}
                outra => panic!("inesperado: {outra:?}"),
            }
        }
        assert!(aviso.is_some(), "nao desistiu");
        assert!(!ir.ativo());
    }

    #[test]
    fn lugar_so_chega_e_avisa() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Lugar, vec2(40.0, 0.0), 6.0), 0.0);
        assert_eq!(
            ir.passo(Vec2::ZERO, 0.0, false),
            Some(Acao::Viajar(vec2(40.0, 0.0)))
        );
        assert_eq!(
            ir.passo(vec2(36.0, 0.0), 2.0, false),
            Some(Acao::Aviso("Chegou: x.".into()))
        );
        assert!(!ir.ativo());
    }

    #[test]
    fn parar_cancela() {
        let mut ir = IrPara::default();
        ir.iniciar(alvo(Objetivo::Combate, vec2(500.0, 0.0), 45.0), 0.0);
        ir.parar();
        assert_eq!(ir.passo(vec2(499.0, 0.0), 5.0, false), None);
    }
}
