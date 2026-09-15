//! A `mesa`: fila automatica, lista de salas e pronto-check das dungeons
//! (docs/DUNGEONS_E_RAIDS.md, secao 6, decisao 3).
//!
//! Regra pura, sem mundo nem rede: quem joga e' uma CHAVE (`u64`), e a mesa
//! so' devolve eventos. Hoje mora no canal e so' junta quem esta' no mesmo
//! processo; a interface e' a mesma que um servico entre canais/realms vai
//! implementar (a F5 do plano), por isso nada aqui conhece sessao.
//!
//! - Uma pessoa esta' em UMA coisa so': fila, sala ou pronto-check.
//! - Fila fecha grupo quando lota, ou depois de `ESPERA_PRA_MENOS_S` com o
//!   minimo do estagio.
//! - Sala: o lider inicia quando quiser; sala cheia inicia sozinha; com
//!   "completar pela fila" puxa da fila depois de `SALA_PUXA_FILA_S`.
//! - Pronto-check de `PRONTO_S`: todos aceitam, comeca. Alguem recusa ou deixa
//!   expirar: quem aceitou volta pro topo da fila (ou fica na sala), quem
//!   recusou sai.

pub type Chave = u64;

pub const PRONTO_S: f64 = 20.0;
pub const ESPERA_PRA_MENOS_S: f64 = 300.0;
pub const SALA_PUXA_FILA_S: f64 = 60.0;
pub const SALA_PARADA_FECHA_S: f64 = 600.0;

#[derive(Debug, Clone, PartialEq)]
pub struct NaFila {
    pub chave: Chave,
    pub conteudo: u16,
    pub estagio: u8,
    pub desde: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sala {
    pub id: u32,
    pub lider: Chave,
    pub conteudo: u16,
    pub estagio: u8,
    pub membros: Vec<Chave>,
    pub completar_pela_fila: bool,
    pub mexeu_em: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pronto {
    pub id: u32,
    pub conteudo: u16,
    pub estagio: u8,
    pub membros: Vec<Chave>,
    pub aceitos: Vec<Chave>,
    pub expira: f64,
    /// De uma sala (quem aceitou volta pra ela) ou da fila.
    pub sala: Option<u32>,
    /// Quando saiu da fila, por membro — pra voltar ao TOPO se cair.
    pub desde: Vec<(Chave, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Evento {
    /// Abriu um pronto-check: avisar os membros.
    Pronto(Pronto),
    /// Todos aceitaram: criar a instancia.
    Comecar { conteudo: u16, estagio: u8, membros: Vec<Chave> },
    /// Pronto-check caiu. `recusou` saiu de tudo; o resto voltou.
    Cancelado { partida: u32, membros: Vec<Chave>, recusou: Vec<Chave> },
    SalaFechou { id: u32, membros: Vec<Chave> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Onde {
    Livre,
    Fila,
    Sala(u32),
    Pronto(u32),
}

#[derive(Debug, Default)]
pub struct Mesa {
    pub fila: Vec<NaFila>,
    pub salas: Vec<Sala>,
    pub prontos: Vec<Pronto>,
    prox_sala: u32,
    prox_partida: u32,
}

/// Tamanho maximo e minimo da fila por (conteudo, estagio).
pub struct Regras<'a> {
    pub grupo_max: &'a dyn Fn(u16) -> u8,
    pub minimo: &'a dyn Fn(u16, u8) -> u8,
}

impl Mesa {
    pub fn onde(&self, k: Chave) -> Onde {
        if let Some(p) = self.prontos.iter().find(|p| p.membros.contains(&k)) {
            return Onde::Pronto(p.id);
        }
        if let Some(s) = self.salas.iter().find(|s| s.membros.contains(&k)) {
            return Onde::Sala(s.id);
        }
        if self.fila.iter().any(|f| f.chave == k) {
            return Onde::Fila;
        }
        Onde::Livre
    }

    pub fn na_fila(&self, k: Chave) -> Option<&NaFila> {
        self.fila.iter().find(|f| f.chave == k)
    }

    pub fn sala(&self, id: u32) -> Option<&Sala> {
        self.salas.iter().find(|s| s.id == id)
    }

    pub fn pronto(&self, id: u32) -> Option<&Pronto> {
        self.prontos.iter().find(|p| p.id == id)
    }

    pub fn entrar_fila(&mut self, k: Chave, conteudo: u16, estagio: u8, agora: f64) -> Result<(), &'static str> {
        if matches!(self.onde(k), Onde::Pronto(_)) {
            return Err("Responda o pronto-check antes.");
        }
        self.sair_sala(k, agora);
        self.fila.retain(|f| f.chave != k);
        self.fila.push(NaFila { chave: k, conteudo, estagio, desde: agora });
        Ok(())
    }

    pub fn sair_fila(&mut self, k: Chave) {
        self.fila.retain(|f| f.chave != k);
    }

    pub fn criar_sala(&mut self, k: Chave, conteudo: u16, estagio: u8, completar_pela_fila: bool, agora: f64) -> Result<u32, &'static str> {
        if matches!(self.onde(k), Onde::Pronto(_)) {
            return Err("Responda o pronto-check antes.");
        }
        self.sair_fila(k);
        self.sair_sala(k, agora);
        self.prox_sala += 1;
        let id = self.prox_sala;
        self.salas.push(Sala { id, lider: k, conteudo, estagio, membros: vec![k], completar_pela_fila, mexeu_em: agora });
        Ok(id)
    }

    pub fn entrar_sala(&mut self, k: Chave, id: u32, grupo_max: u8, agora: f64) -> Result<(), &'static str> {
        if matches!(self.onde(k), Onde::Pronto(_)) {
            return Err("Responda o pronto-check antes.");
        }
        if self.prontos.iter().any(|p| p.sala == Some(id)) {
            return Err("A sala está começando.");
        }
        let Some(s) = self.salas.iter().find(|s| s.id == id) else { return Err("A sala não existe mais.") };
        if s.membros.contains(&k) {
            return Ok(());
        }
        if s.membros.len() >= grupo_max as usize {
            return Err("A sala está cheia.");
        }
        self.sair_fila(k);
        self.sair_sala(k, agora);
        let s = self.salas.iter_mut().find(|s| s.id == id).expect("conferida acima");
        s.membros.push(k);
        s.mexeu_em = agora;
        Ok(())
    }

    /// Sai da sala. Lider saindo passa a lideranca; sala vazia some.
    pub fn sair_sala(&mut self, k: Chave, agora: f64) {
        for s in self.salas.iter_mut() {
            if let Some(i) = s.membros.iter().position(|m| *m == k) {
                s.membros.remove(i);
                s.mexeu_em = agora;
                if s.lider == k {
                    if let Some(&novo) = s.membros.first() {
                        s.lider = novo;
                    }
                }
            }
        }
        self.salas.retain(|s| !s.membros.is_empty());
    }

    /// O lider abre o pronto-check com quem esta' na sala.
    pub fn iniciar_sala(&mut self, k: Chave, agora: f64) -> Result<Evento, &'static str> {
        let Some(s) = self.salas.iter().find(|s| s.membros.contains(&k)) else { return Err("Você não está numa sala.") };
        if s.lider != k {
            return Err("Só o líder começa.");
        }
        if self.prontos.iter().any(|p| p.sala == Some(s.id)) {
            return Err("O pronto-check já está aberto.");
        }
        let (id, conteudo, estagio, membros) = (s.id, s.conteudo, s.estagio, s.membros.clone());
        Ok(self.abrir_pronto(conteudo, estagio, membros, Some(id), Vec::new(), agora))
    }

    fn abrir_pronto(&mut self, conteudo: u16, estagio: u8, membros: Vec<Chave>, sala: Option<u32>, desde: Vec<(Chave, f64)>, agora: f64) -> Evento {
        self.prox_partida += 1;
        let p = Pronto { id: self.prox_partida, conteudo, estagio, membros, aceitos: Vec::new(), expira: agora + PRONTO_S, sala, desde };
        self.prontos.push(p.clone());
        Evento::Pronto(p)
    }

    /// Resposta do pronto-check. Recusa fecha na hora.
    pub fn responder(&mut self, k: Chave, partida: u32, aceito: bool, agora: f64) -> Vec<Evento> {
        let Some(p) = self.prontos.iter_mut().find(|p| p.id == partida && p.membros.contains(&k)) else { return Vec::new() };
        if aceito {
            if !p.aceitos.contains(&k) {
                p.aceitos.push(k);
            }
            return self.fechar_prontos(agora, None);
        }
        self.fechar_prontos(agora, Some((partida, k)))
    }

    /// Desconectou: sai de tudo. Pronto-check aberto conta como recusa.
    pub fn remover(&mut self, k: Chave, agora: f64) -> Vec<Evento> {
        self.sair_fila(k);
        self.sair_sala(k, agora);
        let partida = self.prontos.iter().find(|p| p.membros.contains(&k)).map(|p| p.id);
        match partida {
            Some(id) => self.fechar_prontos(agora, Some((id, k))),
            None => Vec::new(),
        }
    }

    fn fechar_prontos(&mut self, agora: f64, recusa: Option<(u32, Chave)>) -> Vec<Evento> {
        let mut ev = Vec::new();
        let mut i = 0;
        while i < self.prontos.len() {
            let p = &self.prontos[i];
            let recusou: Vec<Chave> = match recusa {
                Some((id, k)) if id == p.id => vec![k],
                _ if agora >= p.expira => p.membros.iter().copied().filter(|m| !p.aceitos.contains(m)).collect(),
                _ => Vec::new(),
            };
            let todos = p.membros.iter().all(|m| p.aceitos.contains(m));
            if recusou.is_empty() && !todos {
                i += 1;
                continue;
            }
            let p = self.prontos.remove(i);
            if recusou.is_empty() {
                // Comeca: sai da sala (a sala se desfaz) e da fila.
                if let Some(id) = p.sala {
                    self.salas.retain(|s| s.id != id);
                }
                self.fila.retain(|f| !p.membros.contains(&f.chave));
                ev.push(Evento::Comecar { conteudo: p.conteudo, estagio: p.estagio, membros: p.membros });
                continue;
            }
            // Caiu: quem recusou sai; quem nao recusou volta.
            match p.sala {
                Some(id) => {
                    for k in &recusou {
                        self.sair_sala(*k, agora);
                    }
                    if let Some(s) = self.salas.iter_mut().find(|s| s.id == id) {
                        s.mexeu_em = agora;
                    }
                }
                None => {
                    // Volta pro TOPO: com o `desde` de quando entrou.
                    for &k in p.membros.iter().filter(|m| !recusou.contains(m)) {
                        let desde = p.desde.iter().find(|d| d.0 == k).map_or(agora, |d| d.1);
                        self.fila.push(NaFila { chave: k, conteudo: p.conteudo, estagio: p.estagio, desde });
                    }
                    self.fila.sort_by(|a, b| a.desde.total_cmp(&b.desde));
                }
            }
            ev.push(Evento::Cancelado { partida: p.id, membros: p.membros, recusou });
        }
        ev
    }

    /// Um passo: expira pronto-check, completa sala pela fila, fecha grupo da
    /// fila e fecha sala parada.
    pub fn tick(&mut self, agora: f64, r: &Regras) -> Vec<Evento> {
        let mut ev = self.fechar_prontos(agora, None);

        // Sala com "completar pela fila": puxa depois de um minuto.
        for si in 0..self.salas.len() {
            let (id, c, e, max, puxa, parada) = {
                let s = &self.salas[si];
                ((s.id), s.conteudo, s.estagio, (r.grupo_max)(s.conteudo) as usize, s.completar_pela_fila, agora - s.mexeu_em)
            };
            if self.prontos.iter().any(|p| p.sala == Some(id)) {
                continue;
            }
            if puxa && parada >= SALA_PUXA_FILA_S {
                while self.salas[si].membros.len() < max {
                    let Some(fi) = self.fila.iter().position(|f| f.conteudo == c && f.estagio == e) else { break };
                    let f = self.fila.remove(fi);
                    self.salas[si].membros.push(f.chave);
                }
            }
            if self.salas[si].membros.len() >= max {
                let membros = self.salas[si].membros.clone();
                ev.push(self.abrir_pronto(c, e, membros, Some(id), Vec::new(), agora));
            }
        }

        // Sala parada demais fecha.
        let paradas: Vec<Sala> = self
            .salas
            .iter()
            .filter(|s| agora - s.mexeu_em >= SALA_PARADA_FECHA_S && !self.prontos.iter().any(|p| p.sala == Some(s.id)))
            .cloned()
            .collect();
        for s in paradas {
            self.salas.retain(|x| x.id != s.id);
            ev.push(Evento::SalaFechou { id: s.id, membros: s.membros });
        }

        // Fila: por (conteudo, estagio), na ordem de chegada.
        let mut chaves: Vec<(u16, u8)> = Vec::new();
        for f in &self.fila {
            if !chaves.contains(&(f.conteudo, f.estagio)) {
                chaves.push((f.conteudo, f.estagio));
            }
        }
        for (c, e) in chaves {
            let max = (r.grupo_max)(c).max(1) as usize;
            let min = ((r.minimo)(c, e) as usize).clamp(1, max);
            loop {
                let da_chave: Vec<&NaFila> = self.fila.iter().filter(|f| f.conteudo == c && f.estagio == e).collect();
                let Some(mais_velho) = da_chave.first().map(|f| f.desde) else { break };
                let fecha = da_chave.len() >= max || (da_chave.len() >= min && agora - mais_velho >= ESPERA_PRA_MENOS_S);
                if !fecha {
                    break;
                }
                let grupo: Vec<(Chave, f64)> = da_chave.iter().take(max).map(|f| (f.chave, f.desde)).collect();
                self.fila.retain(|f| !grupo.iter().any(|g| g.0 == f.chave));
                let membros = grupo.iter().map(|g| g.0).collect();
                ev.push(self.abrir_pronto(c, e, membros, None, grupo, agora));
            }
        }
        ev
    }

    /// Salas abertas de um conteudo/estagio (pra a lista Procurar).
    pub fn salas_de(&self, conteudo: u16, estagio: u8) -> Vec<&Sala> {
        self.salas.iter().filter(|s| s.conteudo == conteudo && s.estagio == estagio).collect()
    }

    pub fn contar_fila(&self, conteudo: u16, estagio: u8) -> usize {
        self.fila.iter().filter(|f| f.conteudo == conteudo && f.estagio == estagio).count()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn regras() -> Regras<'static> {
        Regras { grupo_max: &|c| if c == 1 { 1 } else { 5 }, minimo: &|_, e| if e <= 2 { 3 } else { 4 } }
    }

    fn comecou(ev: &[Evento]) -> Option<Vec<Chave>> {
        ev.iter().find_map(|e| match e {
            Evento::Comecar { membros, .. } => Some(membros.clone()),
            _ => None,
        })
    }

    #[test]
    fn cinco_na_fila_fecham_grupo_e_comecam_quando_todos_aceitam() {
        let mut m = Mesa::default();
        for k in 1..=5 {
            m.entrar_fila(k, 10, 1, 0.0).unwrap();
        }
        let ev = m.tick(1.0, &regras());
        let Some(Evento::Pronto(p)) = ev.first() else { panic!("{ev:?}") };
        assert_eq!(p.membros.len(), 5);
        assert!(m.fila.is_empty());
        for k in 1..=4 {
            assert!(comecou(&m.responder(k, p.id, true, 2.0)).is_none());
        }
        assert_eq!(comecou(&m.responder(5, p.id, true, 2.0)), Some(vec![1, 2, 3, 4, 5]));
        assert_eq!(m.onde(1), Onde::Livre);
    }

    #[test]
    fn fila_pequena_so_fecha_depois_de_esperar_e_com_o_minimo() {
        let mut m = Mesa::default();
        for k in 1..=3 {
            m.entrar_fila(k, 10, 1, 0.0).unwrap();
        }
        assert!(m.tick(100.0, &regras()).is_empty());
        assert!(matches!(m.tick(ESPERA_PRA_MENOS_S, &regras()).first(), Some(Evento::Pronto(p)) if p.membros.len() == 3));
        let mut n = Mesa::default();
        for k in 1..=3 {
            n.entrar_fila(k, 10, 3, 0.0).unwrap();
        }
        assert!(n.tick(ESPERA_PRA_MENOS_S * 2.0, &regras()).is_empty(), "estagio 3 pede 4");
    }

    #[test]
    fn recusa_devolve_quem_aceitou_pro_topo_e_tira_quem_recusou() {
        let mut m = Mesa::default();
        for k in 1..=5 {
            m.entrar_fila(k, 10, 1, k as f64).unwrap();
        }
        m.entrar_fila(9, 10, 1, 50.0).unwrap();
        let ev = m.tick(60.0, &regras());
        let Some(Evento::Pronto(p)) = ev.first() else { panic!() };
        m.responder(1, p.id, true, 61.0);
        let ev = m.responder(3, p.id, false, 61.0);
        assert!(matches!(&ev[..], [Evento::Cancelado { recusou, .. }] if recusou == &vec![3]));
        assert_eq!(m.onde(3), Onde::Livre);
        assert_eq!(m.fila.first().map(|f| f.chave), Some(1), "quem aceitou volta ao topo");
        assert!(m.fila.iter().any(|f| f.chave == 9));
    }

    #[test]
    fn pronto_expira_e_quem_nao_respondeu_sai() {
        let mut m = Mesa::default();
        for k in 1..=5 {
            m.entrar_fila(k, 10, 1, 0.0).unwrap();
        }
        let Some(Evento::Pronto(p)) = m.tick(1.0, &regras()).into_iter().next() else { panic!() };
        m.responder(2, p.id, true, 2.0);
        let ev = m.tick(1.0 + PRONTO_S, &regras());
        let Some(Evento::Cancelado { recusou, .. }) = ev.iter().find(|e| matches!(e, Evento::Cancelado { .. })) else { panic!("{ev:?}") };
        assert_eq!(recusou.len(), 4);
        assert_eq!(m.onde(2), Onde::Fila);
    }

    #[test]
    fn uma_coisa_so_fila_ou_sala() {
        let mut m = Mesa::default();
        m.entrar_fila(1, 10, 1, 0.0).unwrap();
        let s = m.criar_sala(1, 10, 1, false, 1.0).unwrap();
        assert_eq!(m.onde(1), Onde::Sala(s), "criar sala tira da fila");
        m.entrar_fila(1, 10, 2, 2.0).unwrap();
        assert_eq!(m.onde(1), Onde::Fila, "entrar na fila tira da sala");
        assert!(m.salas.is_empty(), "sala vazia some");
    }

    #[test]
    fn sala_lider_inicia_e_sala_completa_pela_fila() {
        let mut m = Mesa::default();
        let s = m.criar_sala(1, 10, 1, true, 0.0).unwrap();
        m.entrar_sala(2, s, 5, 1.0).unwrap();
        assert_eq!(m.iniciar_sala(2, 2.0), Err("Só o líder começa."));
        for k in 10..=12 {
            m.entrar_fila(k, 10, 1, 3.0).unwrap();
        }
        assert!(m.tick(30.0, &regras()).is_empty(), "antes de um minuto nao puxa (e a fila tem 3 < 5)");
        let ev = m.tick(1.0 + SALA_PUXA_FILA_S, &regras());
        let Some(Evento::Pronto(p)) = ev.iter().find(|e| matches!(e, Evento::Pronto(_))) else { panic!("{ev:?}") };
        assert_eq!(p.membros, vec![1, 2, 10, 11, 12]);
        assert_eq!(p.sala, Some(s));
        let mut fim = Vec::new();
        for k in [1, 2, 10, 11, 12] {
            fim = m.responder(k, p.id, true, 70.0);
        }
        assert_eq!(comecou(&fim).map(|v| v.len()), Some(5));
        assert!(m.salas.is_empty());
    }

    #[test]
    fn sala_com_menos_gente_comeca_pelo_lider() {
        let mut m = Mesa::default();
        let s = m.criar_sala(7, 10, 1, false, 0.0).unwrap();
        m.entrar_sala(8, s, 5, 0.0).unwrap();
        let Evento::Pronto(p) = m.iniciar_sala(7, 1.0).unwrap() else { panic!() };
        m.responder(7, p.id, true, 1.0);
        assert_eq!(comecou(&m.responder(8, p.id, true, 1.0)), Some(vec![7, 8]));
    }

    #[test]
    fn desconectar_no_pronto_conta_como_recusa() {
        let mut m = Mesa::default();
        let s = m.criar_sala(1, 10, 1, false, 0.0).unwrap();
        m.entrar_sala(2, s, 5, 0.0).unwrap();
        let Evento::Pronto(p) = m.iniciar_sala(1, 1.0).unwrap() else { panic!() };
        let ev = m.remover(2, 2.0);
        assert!(matches!(&ev[..], [Evento::Cancelado { recusou, .. }] if recusou == &vec![2]));
        assert_eq!(m.onde(1), Onde::Sala(s), "o lider continua na sala");
        assert!(m.pronto(p.id).is_none());
    }
}
