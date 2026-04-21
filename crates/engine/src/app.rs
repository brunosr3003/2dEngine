//! Event loop + bootstrap do renderer. Define a trait `Game` que clientes
//! implementam. Loop fixed-timestep: `update()` roda em tick fixo,
//! `render()` roda a cada frame de display.

use crate::input::Input;
use crate::render::{Camera2D, Renderer, SpriteBatch};
use crate::time::FixedTimestep;
use anyhow::Result;
use glam::Vec2;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Trait do jogo. O runner chama os metodos na ordem certa:
/// `init` uma vez apos o renderer estar pronto, `update` em tick fixo,
/// `render` todo frame.
pub trait Game: 'static {
    /// Chamado uma unica vez, apos o renderer estar pronto.
    /// Use para carregar assets e fazer upload de atlas.
    fn init(&mut self, ctx: &mut AppContext, renderer: &mut Renderer);

    /// Chamado N vezes por frame (onde N = ticks acumulados).
    /// `dt` = `TICK_DT` constante.
    fn update(&mut self, ctx: &mut AppContext, dt: f32);

    /// Chamado uma vez por frame. Preencha `batch` com sprites.
    /// `alpha` = fracao do tick para interpolacao de render.
    fn render(&mut self, ctx: &mut AppContext, batch: &mut SpriteBatch, alpha: f32);

    fn shutdown(&mut self, _ctx: &mut AppContext) {}
}

/// Dados de contexto disponiveis em todos os callbacks.
pub struct AppContext {
    pub input: Input,
    pub camera: Camera2D,
    pub viewport: Vec2,
    pub should_exit: bool,
}

impl AppContext {
    fn new(viewport: Vec2) -> Self {
        Self {
            input: Input::new(),
            camera: Camera2D::new(viewport),
            viewport,
            should_exit: false,
        }
    }
}

pub struct AppConfig {
    pub title: &'static str,
    pub tick_hz: u32,
    pub initial_size: (u32, u32),
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { title: "2dEngine", tick_hz: 30, initial_size: (1280, 720) }
    }
}

struct Runner<G: Game> {
    config: AppConfig,
    game: G,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    batch: SpriteBatch,
    ctx: AppContext,
    sim: FixedTimestep,
    initialized: bool,
}

impl<G: Game> ApplicationHandler for Runner<G> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let (w, h) = self.config.initial_size;
        let attrs = Window::default_attributes()
            .with_title(self.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(w as f64, h as f64));
        let window = Arc::new(el.create_window(attrs).expect("create_window"));
        let size = window.inner_size();
        self.ctx.viewport = Vec2::new(size.width as f32, size.height as f32);
        self.ctx.camera.viewport = self.ctx.viewport;
        let renderer = pollster::block_on(Renderer::new(window.clone()))
            .expect("renderer init");
        self.window = Some(window.clone());
        self.renderer = Some(renderer);

        if !self.initialized {
            // Passa renderer mutavel para o jogo fazer upload de atlas, etc.
            if let Some(r) = &mut self.renderer {
                self.game.init(&mut self.ctx, r);
            }
            self.initialized = true;
        }
        window.request_redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.ctx.input.on_window_event(&event);
        match event {
            WindowEvent::CloseRequested => { self.ctx.should_exit = true; }
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
                self.ctx.viewport = Vec2::new(size.width as f32, size.height as f32);
                self.ctx.camera.viewport = self.ctx.viewport;
            }
            WindowEvent::RedrawRequested => {
                let ticks = self.sim.advance();
                let dt = self.sim.step().as_secs_f32();
                for _ in 0..ticks {
                    self.game.update(&mut self.ctx, dt);
                }
                self.batch.clear();
                self.game.render(&mut self.ctx, &mut self.batch, self.sim.alpha());
                if let Some(r) = &mut self.renderer {
                    if let Err(e) = r.render(
                        self.ctx.camera.view_proj(),
                        &mut self.batch,
                        [0.05, 0.05, 0.08, 1.0],
                    ) {
                        tracing::warn!("render error: {e:?}");
                    }
                }
                // So limpa input apos pelo menos um update ter processado.
                // Se ticks==0 o input acumula ate o proximo tick — nenhum
                // caractere e perdido entre frames de display intermediarios.
                if ticks > 0 {
                    self.ctx.input.end_frame();
                }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            _ => {}
        }
        if self.ctx.should_exit {
            self.game.shutdown(&mut self.ctx);
            el.exit();
        }
    }
}

pub fn run<G: Game>(config: AppConfig, game: G) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut runner = Runner {
        sim: FixedTimestep::new(config.tick_hz),
        config,
        game,
        window: None,
        renderer: None,
        batch: SpriteBatch::new(),
        ctx: AppContext::new(Vec2::new(1280.0, 720.0)),
        initialized: false,
    };
    event_loop.run_app(&mut runner)?;
    Ok(())
}
