//! TERRITORIES, phase 3 (docs/TERRITORIOS.md): who holds which territory
//! islet, the claim, and the list every client at sea keeps.
//!
//! The database (`territorios`, `social::reivindicar`) is the truth; the sea
//! process keeps a copy (`GameWorld::territorios`), reloaded every 30 s and
//! after each claim, so a clan dissolved elsewhere frees its islet here too.

use super::*;
use shared::territorio::{Dono, RAIO_DO_NUCLEO};

/// Ticks between reloads of the held islets.
const RECARREGA_A_CADA_S: u32 = 30;

impl GameWorld {
    /// Each tick at sea: reload now and then; send the list to whoever does
    /// not have it yet.
    pub(crate) fn tick_territorios(&mut self) {
        if self.tick % (shared::TICK_RATE_HZ as u32 * RECARREGA_A_CADA_S) == 1 {
            self.carregar_territorios();
        }
        let lista = self.lista_de_territorios();
        for s in self.sessions.values_mut() {
            if s.logged_in && !s.territorios_enviados {
                s.territorios_enviados = true;
                let _ = s.handle.to_client.send(ServerMessage::Territorios {
                    lista: lista.clone(),
                });
            }
        }
    }

    fn lista_de_territorios(&self) -> Vec<Dono> {
        let mut v: Vec<Dono> = self.territorios.values().cloned().collect();
        v.sort_by_key(|d| d.ilhota);
        v
    }

    /// Reads the held islets from the database (no database in tests: the
    /// in-memory copy stays).
    pub(crate) fn carregar_territorios(&mut self) {
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        tokio::spawn(async move {
            match crate::social::territorios(&ctx.pool).await {
                Ok(v) => {
                    let _ = ctx.tx.send(IncomingMessage::Territorios(v));
                }
                Err(e) => tracing::warn!("territorios: falha ao carregar: {e:#}"),
            }
        });
    }

    /// The database's list arrived: replace the copy and, when it changed,
    /// tell everyone at sea.
    pub fn on_territorios(&mut self, v: Vec<Dono>) {
        let novo: HashMap<u8, Dono> = v.into_iter().map(|d| (d.ilhota, d)).collect();
        if novo != self.territorios {
            self.territorios = novo;
            self.avisar_territorios();
        }
    }

    fn avisar_territorios(&mut self) {
        let lista = self.lista_de_territorios();
        for s in self.sessions.values_mut().filter(|s| s.logged_in) {
            s.territorios_enviados = true;
            let _ = s.handle.to_client.send(ServerMessage::Territorios {
                lista: lista.clone(),
            });
        }
    }

    /// Why `sid` cannot claim the islet they stand on, or the islet index.
    /// The rules the sea knows; the clan rank is the database's.
    pub(crate) fn pode_reivindicar(&self, sid: SessionId) -> Result<u8, &'static str> {
        let s = self.sessions.get(&sid).ok_or("")?;
        let t = s.em_terra.ok_or("Go ashore on a territory islet first.")?;
        let ilha = shared::mar::ilhotas()
            .get(t.ilhota as usize)
            .ok_or("Go ashore on a territory islet first.")?;
        let corpo = s
            .entity
            .and_then(|e| self.ecs.get::<&Position>(e).ok().map(|p| p.0))
            .ok_or("")?;
        if corpo.distance(ilha.centro) > RAIO_DO_NUCLEO {
            return Err("Walk to the heart of the islet to claim it.");
        }
        if s.cla_id == 0 {
            return Err("Only a clan can claim a territory islet.");
        }
        if self.territorios.contains_key(&t.ilhota) {
            return Err("This islet already belongs to a clan.");
        }
        if self.territorios.values().any(|d| d.cla_id == s.cla_id) {
            return Err("Your clan already holds a territory islet.");
        }
        Ok(t.ilhota)
    }

    /// CLAIM the free islet I stand on, at its heart (`PedidoNavio::
    /// ReivindicarIlhota`). The sea's checks first, then the database's
    /// (rank, one per clan, still free) in a task.
    pub(crate) fn reivindicar_ilhota(&mut self, sid: SessionId) {
        let ilhota = match self.pode_reivindicar(sid) {
            Ok(i) => i,
            Err(motivo) => {
                if !motivo.is_empty() {
                    self.avisa_missao(sid, motivo.into());
                }
                return;
            }
        };
        let nome = self.nome_de(sid);
        let Some(ctx) = self.auth_ctx.clone() else {
            return;
        };
        tokio::spawn(async move {
            let resultado = crate::social::reivindicar(&ctx.pool, &nome, ilhota)
                .await
                .map_err(|e| e.to_string());
            let _ = ctx.tx.send(IncomingMessage::Reivindicacao { sid, resultado });
        });
    }

    /// A claim's outcome came back from the database.
    pub fn on_reivindicacao(&mut self, sid: SessionId, resultado: Result<Dono, String>) {
        match resultado {
            Ok(d) => {
                let ilha = shared::mar::ilhotas()
                    .get(d.ilhota as usize)
                    .map_or("the islet", |i| i.nome);
                tracing::info!("territorio: {} reivindicou {ilha} para {}", self.nome_de(sid), d.cla);
                self.send_chat_to(sid, &format!("{ilha} now belongs to {}.", d.cla));
                self.territorios.insert(d.ilhota, d);
                self.avisar_territorios();
            }
            Err(e) => self.avisa_missao(sid, e),
        }
    }
}
