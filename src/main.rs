//! Opens a window and runs the pour in real time:
//! `diskpour [DISKS] [STEPS_PER_SECOND]`.
//!
//! Space pauses, R restarts, Escape quits.

use diskpour::pour::Pour;
use diskpour::render::Renderer;
use diskpour::world::World;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const DEFAULT_DISKS: u32 = 100_000;
/// Twenty steps per 60 Hz frame.
const DEFAULT_STEPS_PER_SECOND: u32 = 1200;

/// The most real time a single frame may catch up on. A machine too slow to keep
/// up runs in slow motion instead of falling ever further behind.
const MOST_SECONDS_PER_FRAME: f64 = 0.05;

/// How often the title bar refreshes its numbers.
const REPORT_PERIOD: Duration = Duration::from_millis(500);

/// The window and everything bound to it.
struct Display {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

impl Display {
    fn open(event_loop: &ActiveEventLoop, pour: &Pour) -> Self {
        // A square window that fits the screen, title bar included.
        let side = event_loop.primary_monitor().map_or(800.0, |monitor| {
            let logical = monitor.size().to_logical::<f64>(monitor.scale_factor());
            0.85 * logical.width.min(logical.height)
        });
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("diskpour")
                    .with_inner_size(LogicalSize::new(side, side)),
            )
            .expect("the window system should create a window");
        let window = Arc::new(window);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .expect("the window should accept a GPU surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("a GPU should be able to draw to this window");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("the GPU should provide a device");

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("the GPU should support this window's surface");
        config.format = config.format.remove_srgb_suffix();
        surface.configure(&device, &config);
        let renderer = Renderer::new(&device, &queue, config.format, pour);
        Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn present(&mut self, world: &World) {
        use wgpu::CurrentSurfaceTexture as Current;
        let frame = match self.surface.get_current_texture() {
            Current::Success(frame) | Current::Suboptimal(frame) => frame,
            Current::Outdated | Current::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Current::Timeout | Current::Occluded | Current::Validation => return,
        };
        let target = frame.texture.create_view(&Default::default());
        let size = [self.config.width, self.config.height];
        self.renderer.draw(&target, size, world);
        self.window.pre_present_notify();
        self.queue.present(frame);
    }
}

/// Averages what happened since the last title update.
struct Report {
    since: Instant,
    frames: u32,
    physics: Duration,
}

struct App {
    pour: Pour,
    world: World,
    display: Option<Display>,
    paused: bool,
    last_frame: Instant,
    owed_steps: f64,
    report: Report,
}

impl App {
    fn new(pour: Pour) -> Self {
        let now = Instant::now();
        Self {
            pour,
            world: World::new(pour.arena),
            display: None,
            paused: false,
            last_frame: now,
            owed_steps: 0.0,
            report: Report {
                since: now,
                frames: 0,
                physics: Duration::ZERO,
            },
        }
    }

    /// Steps the world by the real time that passed since the last frame.
    fn catch_up(&mut self) {
        let now = Instant::now();
        let elapsed = now - self.last_frame;
        self.last_frame = now;
        if self.paused {
            return;
        }
        let rate = f64::from(self.pour.steps_per_second);
        self.owed_steps += elapsed.as_secs_f64().min(MOST_SECONDS_PER_FRAME) * rate;
        let due = self.owed_steps as u32;
        self.owed_steps -= f64::from(due);
        for _ in 0..due {
            self.pour.advance(&mut self.world);
        }
        self.report.physics += now.elapsed();
    }

    fn update_title(&mut self) {
        self.report.frames += 1;
        let elapsed = self.report.since.elapsed();
        if elapsed < REPORT_PERIOD {
            return;
        }
        let frames = self.report.frames;
        let title = format!(
            "diskpour · {} disks · {:.0} fps · physics {:.1} ms",
            self.world.disks().len(),
            f64::from(frames) / elapsed.as_secs_f64(),
            self.report.physics.as_secs_f64() * 1e3 / f64::from(frames),
        );
        if let Some(display) = &self.display {
            display.window.set_title(&title);
        }
        self.report = Report {
            since: Instant::now(),
            frames: 0,
            physics: Duration::ZERO,
        };
    }

    fn press(&mut self, key: &Key, event_loop: &ActiveEventLoop) {
        match key.as_ref() {
            Key::Named(NamedKey::Escape) => event_loop.exit(),
            Key::Named(NamedKey::Space) => self.paused = !self.paused,
            Key::Character("r" | "R") => {
                self.world = World::new(self.pour.arena);
                self.owed_steps = 0.0;
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.display.is_none() {
            let display = Display::open(event_loop, &self.pour);
            display.window.request_redraw();
            self.display = Some(display);
            self.last_frame = Instant::now();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(display) = &mut self.display {
                    display.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed() && !event.repeat =>
            {
                self.press(&event.logical_key, event_loop);
            }
            WindowEvent::RedrawRequested => {
                self.catch_up();
                self.update_title();
                if let Some(display) = &mut self.display {
                    display.present(&self.world);
                    display.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let mut arguments = std::env::args().skip(1).map(|text| {
        text.parse()
            .expect("usage: diskpour [DISKS] [STEPS_PER_SECOND]")
    });
    let disks = arguments.next().unwrap_or(DEFAULT_DISKS);
    let steps_per_second = arguments.next().unwrap_or(DEFAULT_STEPS_PER_SECOND);
    let event_loop = EventLoop::new().expect("the window system should start");
    event_loop
        .run_app(&mut App::new(Pour::new(disks, steps_per_second)))
        .expect("the event loop should run");
}
