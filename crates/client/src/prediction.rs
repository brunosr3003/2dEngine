//! Predicao client-side de movimento + reconciliacao com servidor.
//!
//! Fluxo:
//! 1. Jogador pressiona WASD -> gera InputFrame{seq, move_dir}.
//! 2. `push_input()`: aplica localmente (move jogador ja), guarda no buffer.
//! 3. Servidor recebe o input, simula, manda Snapshot com last_input_seq=N.
//! 4. `reconcile()`: pega posicao autoritativa, descarta inputs acked,
//!    re-simula inputs pendentes a partir dai.
//!
//! Se a diferenca auth vs predicted for > SNAP_THRESHOLD, snappa direto.
//! Caso contrario, lerp suave em CORRECTION_FRAMES frames.

use glam::Vec2;
use shared::{protocol::InputFrame, PLAYER_SPEED, TICK_DT};
use std::collections::VecDeque;

/// Diferenca maxima (tiles) antes de snap direto (sem lerp).
const SNAP_THRESHOLD: f32 = 3.0;
/// Frames para suavizar a correcao (lerp incremental).
const CORRECTION_FRAMES: u32 = 8;

pub struct PredictionBuffer {
    /// Inputs enviados mas ainda nao confirmados pelo servidor.
    pending: VecDeque<InputFrame>,
    /// Posicao que o cliente acredita estar (predicao).
    pub predicted_pos: Vec2,
    /// Correcao pendente: posicao alvo e frames restantes.
    correction: Option<(Vec2, u32)>,
}

impl PredictionBuffer {
    pub fn new(spawn: Vec2) -> Self {
        Self {
            pending: VecDeque::new(),
            predicted_pos: spawn,
            correction: None,
        }
    }

    /// Aplica o input localmente e guarda para reconciliacao posterior.
    /// Chamar uma vez por tick, antes de enviar o InputFrame ao servidor.
    pub fn push_input(&mut self, frame: InputFrame) {
        let dir = if frame.move_dir.length_squared() > 1.0 {
            frame.move_dir.normalize()
        } else {
            frame.move_dir
        };
        self.predicted_pos += dir * PLAYER_SPEED * TICK_DT;
        self.pending.push_back(frame);

        // Limita o buffer (se nao ha resposta do servidor, algo errado).
        if self.pending.len() > 120 {
            self.pending.pop_front();
        }
    }

    /// Processa snapshot do servidor: reconcilia posicao.
    /// `auth_pos` = posicao autoritativa do proprio jogador no snapshot.
    /// `last_acked_seq` = `WorldSnapshot::last_input_seq`.
    pub fn reconcile(&mut self, auth_pos: Vec2, last_acked_seq: u32) {
        // Descarta inputs ja processados pelo servidor.
        while let Some(front) = self.pending.front() {
            if front.seq <= last_acked_seq {
                self.pending.pop_front();
            } else {
                break;
            }
        }

        // Re-simula a partir da posicao autoritativa.
        let mut pos = auth_pos;
        for frame in &self.pending {
            let dir = if frame.move_dir.length_squared() > 1.0 {
                frame.move_dir.normalize()
            } else {
                frame.move_dir
            };
            pos += dir * PLAYER_SPEED * TICK_DT;
        }

        let diff = (pos - self.predicted_pos).length();
        if diff > SNAP_THRESHOLD {
            // Divergencia grande: snap imediato.
            self.predicted_pos = pos;
            self.correction = None;
        } else if diff > 0.001 {
            // Divergencia pequena: correcao suave.
            self.correction = Some((pos, CORRECTION_FRAMES));
        }
    }

    /// Avanca a correcao gradual. Chamar uma vez por frame de render.
    /// Retorna a posicao suavizada para usar no render.
    pub fn smooth_position(&mut self) -> Vec2 {
        if let Some((target, frames_left)) = &mut self.correction {
            let t = 1.0 / (*frames_left as f32).max(1.0);
            self.predicted_pos = self.predicted_pos.lerp(*target, t);
            *frames_left -= 1;
            if *frames_left == 0 {
                self.predicted_pos = *target;
                self.correction = None;
            }
        }
        self.predicted_pos
    }
}
