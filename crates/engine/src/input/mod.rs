//! Estado de input do frame. Alimentado pelo `app` a partir de eventos winit.

use glam::Vec2;
use std::collections::HashSet;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Default)]
pub struct Input {
    keys_down: HashSet<KeyCode>,
    keys_pressed: HashSet<KeyCode>,
    keys_released: HashSet<KeyCode>,
    mouse_down: HashSet<MouseButton>,
    mouse_pressed: HashSet<MouseButton>,
    mouse_released: HashSet<MouseButton>,
    mouse_pos: Vec2,
    scroll: f32,
    /// Texto acumulado neste frame (de KeyEvent.text do winit). O jogo
    /// consome via `take_text_input()` quando estiver em modo "digitando".
    text_input: String,
}

impl Input {
    pub fn new() -> Self { Self::default() }

    /// Limpa estados edge-triggered. Chamar no fim do frame, depois de
    /// `game.update()` e `game.render()`.
    pub fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.mouse_pressed.clear();
        self.mouse_released.clear();
        self.scroll = 0.0;
        self.text_input.clear();
    }

    /// Consome e retorna o texto digitado no frame atual. Retorna string
    /// vazia se nada foi digitado.
    pub fn take_text_input(&mut self) -> String {
        std::mem::take(&mut self.text_input)
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event: k, .. } => {
                if let PhysicalKey::Code(code) = k.physical_key {
                    match k.state {
                        ElementState::Pressed if !k.repeat => {
                            if self.keys_down.insert(code) {
                                self.keys_pressed.insert(code);
                            }
                        }
                        ElementState::Released => {
                            if self.keys_down.remove(&code) {
                                self.keys_released.insert(code);
                            }
                        }
                        _ => {}
                    }
                }
                // Acumula texto digitado no frame (respeita layout do SO,
                // incluindo modificadores e caracteres fora ASCII).
                if k.state == ElementState::Pressed {
                    if let Some(txt) = &k.text {
                        self.text_input.push_str(txt);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => {
                    if self.mouse_down.insert(*button) {
                        self.mouse_pressed.insert(*button);
                    }
                }
                ElementState::Released => {
                    if self.mouse_down.remove(button) {
                        self.mouse_released.insert(*button);
                    }
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = Vec2::new(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                use winit::event::MouseScrollDelta::*;
                self.scroll += match delta {
                    LineDelta(_, y) => *y,
                    PixelDelta(p) => (p.y as f32) / 120.0,
                };
            }
            _ => {}
        }
    }

    pub fn key_down(&self, code: KeyCode) -> bool { self.keys_down.contains(&code) }
    pub fn key_pressed(&self, code: KeyCode) -> bool { self.keys_pressed.contains(&code) }
    pub fn key_released(&self, code: KeyCode) -> bool { self.keys_released.contains(&code) }
    pub fn mouse_down(&self, b: MouseButton) -> bool { self.mouse_down.contains(&b) }
    pub fn mouse_pressed(&self, b: MouseButton) -> bool { self.mouse_pressed.contains(&b) }
    pub fn mouse_released(&self, b: MouseButton) -> bool { self.mouse_released.contains(&b) }
    pub fn mouse_pos(&self) -> Vec2 { self.mouse_pos }
    pub fn scroll(&self) -> f32 { self.scroll }

    /// Vetor WASD / setas normalizado.
    pub fn move_vector(&self) -> Vec2 {
        let mut v = Vec2::ZERO;
        if self.key_down(KeyCode::KeyW) || self.key_down(KeyCode::ArrowUp)    { v.y += 1.0; }
        if self.key_down(KeyCode::KeyS) || self.key_down(KeyCode::ArrowDown)  { v.y -= 1.0; }
        if self.key_down(KeyCode::KeyA) || self.key_down(KeyCode::ArrowLeft)  { v.x -= 1.0; }
        if self.key_down(KeyCode::KeyD) || self.key_down(KeyCode::ArrowRight) { v.x += 1.0; }
        if v.length_squared() > 0.0 { v.normalize() } else { v }
    }
}
