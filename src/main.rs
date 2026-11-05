//! Opens a window and runs the pour in real time:
//! `diskpour [DISKS] [STEPS_PER_SECOND]`.
//!
//! Space pauses, R restarts, Escape quits. The title bar reports the frame rate,
//! the physics cost per frame, and the speed relative to real time.

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
/// Fifteen steps per 60 Hz frame.
const DEFAULT_STEPS_PER_SECOND: u32 = 900;

/// The most time a frame may spend on physics, leaving the rest of a 60 Hz frame
/// for drawing. Steps that do not fit are dropped, so a machine too slow to keep up
/// runs in slow motion at a smooth frame rate instead of falling ever further behind.
const PHYSICS_BUDGET: Duration = Duration::from_millis(12);

/// How long to wait before trying again when the window cannot be drawn to, as
/// when it is hidden. The surface then answers at once, so nothing else paces the
/// frame loop.
const UNDRAWABLE_RETRY: Duration = Duration::from_millis(16);

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
            Current::Timeout | Current::Occluded | Current::Validation => {
                std::thread::sleep(UNDRAWABLE_RETRY);
                return;
            }
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
    steps: u32,
    physics: Duration,
}

impl Report {
    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            steps: 0,
            physics: Duration::ZERO,
        }
    }
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
            report: Report::new(),
        }
    }

    /// Steps the world by the real time that passed since the last frame, as far
    /// as the budget allows. Driving the steps from a worker thread keeps each
    /// parallel region from paying to wake this thread when it ends.
    fn catch_up(&mut self) {
        let started = Instant::now();
        let elapsed = started - self.last_frame;
        self.last_frame = started;
        if self.paused {
            return;
        }
        self.owed_steps += elapsed.as_secs_f64() * f64::from(self.pour.steps_per_second);
        let due = self.owed_steps as u32;
        self.owed_steps -= f64::from(due);
        let (pour, world) = (&self.pour, &mut self.world);
        self.report.steps += rayon::scope(|_| {
            let mut taken = 0;
            while taken < due && started.elapsed() < PHYSICS_BUDGET {
                pour.advance(world);
                taken += 1;
            }
            taken
        });
        self.report.physics += started.elapsed();
    }

    fn update_title(&mut self) {
        self.report.frames += 1;
        let elapsed = self.report.since.elapsed().as_secs_f64();
        if elapsed < REPORT_PERIOD.as_secs_f64() {
            return;
        }
        let Report {
            frames,
            steps,
            physics,
            ..
        } = self.report;
        let title = format!(
            "diskpour · {} disks · {:.0} fps · physics {:.1} ms · {:.0}% speed",
            self.world.disks().len(),
            f64::from(frames) / elapsed,
            physics.as_secs_f64() * 1e3 / f64::from(frames),
            100.0 * f64::from(steps) / (elapsed * f64::from(self.pour.steps_per_second)),
        );
        if let Some(display) = &self.display {
            display.window.set_title(&title);
        }
        self.report = Report::new();
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

/// Asks the scheduler to treat this thread as drawing the user's frames. On Apple
/// silicon this is what keeps the physics on the performance cores.
fn request_interactive_priority() {
    #[cfg(target_os = "macos")]
    // SAFETY: the call takes plain values and affects only the calling thread.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

fn main() {
    rayon::ThreadPoolBuilder::new()
        .start_handler(|_| request_interactive_priority())
        .build_global()
        .expect("the physics threads should start");
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
