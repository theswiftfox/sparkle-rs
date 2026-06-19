use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::Instant;

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};
use winit::{
    dpi::PhysicalSize,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
};

use crate::editor::{EditCommands, Editor, EditorMode, SceneSnapshot};
use crate::engine::geometry::Light;
use crate::engine::scene_info::NodeInfo;
use crate::engine::settings::Settings;
use crate::input::first_person::FPSController;
use crate::input::input_handler::{
    Action, ApplicationRequest, Button, InputHandler, Key, ScrollAxis, translate_key,
};
use crate::input::orbit::OrbitCamera;
use crate::input::{Camera, CameraSnapshot};

// Shared types for cross-thread communication

/// Sent from main thread to render thread every event-loop iteration.
/// Contains FullOutput from egui (tessellated on render thread).
pub struct FrameData {
    /// Full egui output (shapes, textures delta, etc.)
    pub full_output: egui::FullOutput,
    /// Edit commands to apply to scene on render thread
    pub edit_commands: EditCommands,
    /// Camera snapshot for rendering
    pub camera: CameraSnapshot,
    /// Window size in pixels
    pub window_size: (u32, u32),
    /// UI scale factor
    pub scale_factor: f64,
    /// Current editor mode
    pub mode: EditorMode,
    /// Delta time in seconds (time since last frame)
    pub delta_t: f32,
    /// Render frame time in milliseconds (from render thread timing feedback)
    pub render_frame_time_ms: f32,
    /// Pending scene load request
    pub pending_scene_load: Option<String>,
    /// Quit requested from UI
    pub pending_quit: bool,
}

/// Sent from render thread back to main thread (rare events).
pub enum CameraCommand {
    /// Orientation gizmo axis-click: snap camera to (azimuth, elevation).
    OrientationSnap(f32, f32),
    /// Quit requested from egui menu.
    RequestQuit,
    /// Mode toggle from egui menu.
    ToggleMode,
}

/// Timing information sent from render thread to main thread every frame.
///
/// This provides the actual render pass timing (wall-clock) for accurate FPS display.
///
/// TODO: Enrich with GPU timestamps via Vulkan timestamp queries for more precise
/// GPU-side timing breakdown (vertex shader, fragment shader, present wait, etc.)
#[derive(Debug, Clone)]
pub struct RenderFrameInfo {
    /// Wall-clock time for the entire render frame (from frame start to present complete) in milliseconds
    pub frame_time_ms: f32,
    /// Optional: GPU time via timestamp queries (when implemented)
    pub gpu_time_ms: Option<f32>,
    /// Scenegraph tree snapshot (None if no scene loaded)
    pub scene_tree: Option<NodeInfo>,
    /// Current lights in the scene
    pub scene_lights: Vec<Light>,
}

// Mouse state tracking

struct MouseState {
    right_down: bool,
    middle_down: bool,
}

pub struct App {
    // Window creation params
    width: u32,
    height: u32,
    title: String,
    initialized: bool,

    // Shared atomic state (readable from render thread)
    quit_requested: Arc<AtomicBool>,
    window_size: Arc<AtomicU64>,

    // Window + egui integration (main thread only)
    window: Option<Arc<Window>>,
    egui_winit: Option<egui_winit::State>,
    // Editor owns egui context and runs UI on main thread
    editor: Option<Editor>,

    // Camera controllers (main thread only)
    orbit_camera: Option<OrbitCamera>,
    fps: Option<FPSController>,
    mode: EditorMode,
    mouse_state: MouseState,
    last_cursor_pos: Option<(f64, f64)>,
    /// Whether egui consumed pointer input (used to gate orbit camera)
    egui_wants_pointer: bool,

    // Channels
    window_notify: mpsc::Sender<(Arc<Window>, egui::Context)>,
    frame_sender: mpsc::SyncSender<FrameData>,
    cmd_receiver: mpsc::Receiver<CameraCommand>,
    /// Receiver for render timing info from render thread
    render_info_receiver: mpsc::Receiver<RenderFrameInfo>,

    // Timing
    last_frame_time: Instant,
    /// Latest render frame timing info (updated each frame from render thread)
    latest_render_info: Option<RenderFrameInfo>,

    /// Cached scene snapshot from render thread (updated each frame)
    scene_snapshot: SceneSnapshot,

    // Camera creation params (used when window is created)
    camera_aspect: f32,
    camera_fov: f32,
    camera_near: f32,
    camera_far: f32,

    // Engine settings (shared with FPS controller)
    settings: Settings,
}

/// Render-thread channel endpoints returned by App::new().
pub struct RenderChannels {
    pub frame_receiver: mpsc::Receiver<FrameData>,
    pub cmd_sender: mpsc::Sender<CameraCommand>,
    pub window_receiver: mpsc::Receiver<(Arc<Window>, egui::Context)>,
    /// Channel for render timing info from render thread (sync_channel with capacity 1 for "latest only" semantics)
    pub render_info_sender: mpsc::SyncSender<RenderFrameInfo>,
    pub quit_flag: Arc<AtomicBool>,
    pub window_size: Arc<AtomicU64>,
}

impl App {
    pub fn new(
        width: u32,
        height: u32,
        title: &str,
        camera_fov: f32,
        camera_near: f32,
        camera_far: f32,
        settings: Settings,
    ) -> Result<(Self, EventLoop<()>, RenderChannels), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        let (window_snd, window_rcv) = mpsc::channel();
        let (frame_snd, frame_rcv) = mpsc::sync_channel(2);
        let (cmd_snd, cmd_rcv) = mpsc::channel();
        // Sync channel with capacity 1 for "latest only" semantics - render thread overwrites if main thread hasn't consumed yet
        let (render_info_snd, render_info_rcv) = mpsc::sync_channel(1);

        let quit_flag = Arc::new(AtomicBool::new(false));
        let window_size = Arc::new(AtomicU64::new(Self::pack_size(width, height)));

        let aspect = width as f32 / height as f32;

        let app = Self {
            width,
            height,
            title: title.to_owned(),
            initialized: false,
            quit_requested: Arc::clone(&quit_flag),
            window_size: Arc::clone(&window_size),
            window: None,
            egui_winit: None,
            editor: None,
            orbit_camera: None,
            fps: None,
            mode: EditorMode::Editor,
            mouse_state: MouseState {
                right_down: false,
                middle_down: false,
            },
            last_cursor_pos: None,
            egui_wants_pointer: false,
            window_notify: window_snd,
            frame_sender: frame_snd,
            cmd_receiver: cmd_rcv,
            render_info_receiver: render_info_rcv,
            last_frame_time: Instant::now(),
            latest_render_info: None,
            scene_snapshot: SceneSnapshot::empty(),
            camera_aspect: aspect,
            camera_fov,
            camera_near,
            camera_far,
            settings,
        };

        let channels = RenderChannels {
            frame_receiver: frame_rcv,
            cmd_sender: cmd_snd,
            window_receiver: window_rcv,
            render_info_sender: render_info_snd,
            quit_flag,
            window_size,
        };

        Ok((app, event_loop, channels))
    }

    pub fn wants_quit(&self) -> bool {
        self.quit_requested.load(Ordering::SeqCst)
    }

    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }

    fn pack_size(w: u32, h: u32) -> u64 {
        ((w as u64) << 32) | (h as u64)
    }

    fn unpack_size(packed: u64) -> (u32, u32) {
        ((packed >> 32) as u32, packed as u32)
    }

    fn update_size(&self, w: u32, h: u32) {
        self.window_size
            .store(Self::pack_size(w, h), Ordering::Relaxed);
    }

    fn physical_size(&self) -> (u32, u32) {
        Self::unpack_size(self.window_size.load(Ordering::Relaxed))
    }

    /// Build a CameraSnapshot from the currently active camera.
    fn camera_snapshot(&mut self) -> CameraSnapshot {
        match self.mode {
            EditorMode::Editor => {
                let cam = self.orbit_camera.as_ref().unwrap();
                CameraSnapshot {
                    view_matrix: cam.view_mat(),
                    projection_matrix: cam.projection_mat(),
                    pos: cam.position(),
                    near: cam.near_far().0,
                    far: cam.near_far().1,
                }
            }
            EditorMode::Play => {
                let cam = self.fps.as_ref().unwrap();
                CameraSnapshot {
                    view_matrix: cam.view_mat(),
                    projection_matrix: cam.projection_mat(),
                    pos: cam.position(),
                    near: cam.near_far().0,
                    far: cam.near_far().1,
                }
            }
        }
    }

    /// Handle input that was NOT consumed by egui.
    fn handle_game_input(&mut self, event: &WindowEvent) {
        use WindowEvent::*;

        match event {
            CursorMoved { position, .. } => {
                if let Some((lx, ly)) = self.last_cursor_pos {
                    let dx = (position.x - lx) as f32;
                    let dy = (position.y - ly) as f32;

                    match self.mode {
                        EditorMode::Editor => {
                            if !self.egui_wants_pointer {
                                if self.mouse_state.right_down {
                                    if let Some(cam) = &mut self.orbit_camera {
                                        cam.orbit(dx, dy);
                                    }
                                } else if self.mouse_state.middle_down {
                                    if let Some(cam) = &mut self.orbit_camera {
                                        cam.pan(dx, dy);
                                    }
                                }
                            }
                        }
                        EditorMode::Play => {
                            if let Some(fps) = &mut self.fps {
                                fps.handle_mouse_move(dx as i32, dy as i32);
                            }
                        }
                    }
                }

                // Cursor snapping in FPS mode
                let centre_on_move = self.mode == EditorMode::Play
                    && self.fps.as_ref().map_or(false, |f| f.is_aiming());

                if centre_on_move {
                    if let Some(w) = &self.window {
                        let size = w.window_ref.inner_size();
                        let cx = size.width as f64 / 2.0;
                        let cy = size.height as f64 / 2.0;
                        let _ = w
                            .window_ref
                            .set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                        self.last_cursor_pos = Some((cx, cy));
                    }
                } else {
                    self.last_cursor_pos = Some((position.x, position.y));
                }
            }

            MouseInput { state, button, .. } => {
                let pressed = *state == ElementState::Pressed;
                match button {
                    MouseButton::Right => self.mouse_state.right_down = pressed,
                    MouseButton::Middle => self.mouse_state.middle_down = pressed,
                    _ => {}
                }

                // In editor mode, orbit camera handles button state (already done above).
                // In play mode, FPS controller handles mouse buttons.
                if self.mode == EditorMode::Play {
                    if let Some(fps) = &mut self.fps {
                        let action = match state {
                            ElementState::Pressed => Action::Down,
                            ElementState::Released => Action::Up,
                        };
                        let btn = match button {
                            MouseButton::Left => Button::Left,
                            MouseButton::Right => Button::Right,
                            MouseButton::Middle => Button::Middle,
                            _ => return,
                        };
                        match fps.handle_mouse(btn, action) {
                            ApplicationRequest::SnapMouse => {
                                if let Some(w) = &self.window {
                                    w.window_ref.set_cursor_visible(false);
                                    let size = w.window_ref.inner_size();
                                    let cx = size.width as f64 / 2.0;
                                    let cy = size.height as f64 / 2.0;
                                    let _ = w.window_ref.set_cursor_position(
                                        winit::dpi::PhysicalPosition::new(cx, cy),
                                    );
                                    self.last_cursor_pos = Some((cx, cy));
                                }
                            }
                            ApplicationRequest::UnsnapMouse => {
                                if let Some(w) = &self.window {
                                    w.window_ref.set_cursor_visible(true);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            MouseWheel { delta, .. } => match self.mode {
                EditorMode::Editor => {
                    if !self.egui_wants_pointer {
                        let scroll = match delta {
                            MouseScrollDelta::LineDelta(_, y) => *y,
                            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 24.0,
                        };
                        if scroll.abs() > 0.0 {
                            if let Some(cam) = &mut self.orbit_camera {
                                cam.zoom(scroll);
                            }
                        }
                    }
                }
                EditorMode::Play => {
                    if let Some(fps) = &mut self.fps {
                        match delta {
                            MouseScrollDelta::LineDelta(x, y) => {
                                if y.abs() > 0.0 {
                                    fps.handle_wheel(ScrollAxis::Vertical, y * 24.0);
                                }
                                if x.abs() > 0.0 {
                                    fps.handle_wheel(ScrollAxis::Horizontal, x * 24.0);
                                }
                            }
                            MouseScrollDelta::PixelDelta(pos) => {
                                if pos.y.abs() > 0.0 {
                                    fps.handle_wheel(ScrollAxis::Vertical, pos.y as f32);
                                }
                                if pos.x.abs() > 0.0 {
                                    fps.handle_wheel(ScrollAxis::Horizontal, pos.x as f32);
                                }
                            }
                        }
                    }
                }
            },

            KeyboardInput {
                event: key_event, ..
            } => {
                // F1 toggles mode regardless
                if key_event.state == ElementState::Pressed && !key_event.repeat {
                    if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F1) =
                        key_event.physical_key
                    {
                        self.mode = match self.mode {
                            EditorMode::Editor => EditorMode::Play,
                            EditorMode::Play => EditorMode::Editor,
                        };
                        return;
                    }
                }

                // FPS controller key handling (play mode only)
                if self.mode == EditorMode::Play {
                    if let Some(fps) = &mut self.fps {
                        let action = match key_event.state {
                            ElementState::Pressed => Action::Down,
                            ElementState::Released => Action::Up,
                        };
                        let key = match key_event.physical_key {
                            winit::keyboard::PhysicalKey::Code(code) => translate_key(code),
                            _ => Key::None,
                        };
                        match fps.handle_key(key, action) {
                            ApplicationRequest::Quit => {
                                self.quit_requested.store(true, Ordering::SeqCst);
                            }
                            _ => {}
                        }
                    }
                }
            }

            _ => {}
        }
    }

    /// Drain render commands from the render thread.
    fn process_render_commands(&mut self) {
        while let Ok(cmd) = self.cmd_receiver.try_recv() {
            match cmd {
                CameraCommand::OrientationSnap(az, el) => {
                    if let Some(cam) = &mut self.orbit_camera {
                        cam.set_orientation(az, el);
                    }
                }
                CameraCommand::RequestQuit => {
                    self.quit_requested.store(true, Ordering::SeqCst);
                }
                CameraCommand::ToggleMode => {
                    self.mode = match self.mode {
                        EditorMode::Editor => EditorMode::Play,
                        EditorMode::Play => EditorMode::Editor,
                    };
                }
            }
        }
    }
}

// Window wrapper (sent to render thread for VK surface creation)

pub struct Window {
    pub(crate) window_ref: Arc<winit::window::Window>,
    h_wnd: RawWindowHandle,
    h_dsp: RawDisplayHandle,
    /// Pre-created CAMetalLayer pointer (macOS only). Created on main thread.
    metal_layer_ptr: *const std::ffi::c_void,
}

impl Window {
    #[must_use]
    pub fn h_wnd(&self) -> RawWindowHandle {
        self.h_wnd
    }
    #[must_use]
    pub fn h_dsp(&self) -> RawDisplayHandle {
        self.h_dsp
    }
    #[must_use]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.window_ref.inner_size()
    }

    #[must_use]
    pub fn winit_window(&self) -> &winit::window::Window {
        &self.window_ref
    }

    #[must_use]
    pub fn winit_window_arc(&self) -> Arc<winit::window::Window> {
        Arc::clone(&self.window_ref)
    }

    /// Returns the CAMetalLayer pointer (macOS). Null on other platforms.
    #[must_use]
    pub fn metal_layer(&self) -> *const std::ffi::c_void {
        self.metal_layer_ptr
    }
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.initialized {
            return;
        }

        match event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_title(&self.title)
                    .with_inner_size(LogicalSize::new(self.width, self.height)),
            )
            .map_err(|e| format!("{e}"))
            .and_then(|w| {
                let hwnd = w
                    .window_handle()
                    .map(|rwh| rwh.as_raw())
                    .map_err(|e| format!("{e}"))?;
                let hdsp = w
                    .display_handle()
                    .map(|rdh| rdh.as_raw())
                    .map_err(|e| format!("{e}"))?;
                Ok((w, hwnd, hdsp))
            }) {
            Ok((w, hwnd, hdsp)) => {
                let winit_window = Arc::new(w);

                let LogicalSize { width, height } = winit_window
                    .inner_size()
                    .to_logical(winit_window.scale_factor());
                self.width = width;
                self.height = height;

                // Create shared egui context (used by both egui_winit and Editor)
                // This ensures texture IDs (font atlas, images) are consistent
                let shared_ctx = egui::Context::default();

                let egui_winit_state = egui_winit::State::new(
                    shared_ctx.clone(),
                    egui::ViewportId::ROOT,
                    &*winit_window,
                    Some(winit_window.scale_factor() as f32),
                    None,
                    None,
                );
                self.egui_winit = Some(egui_winit_state);

                // Editor uses the SAME shared context
                self.editor = Some(Editor::new(shared_ctx.clone()));

                // Create camera controllers
                self.orbit_camera = Some(OrbitCamera::new(
                    self.camera_aspect,
                    self.camera_fov,
                    self.camera_near,
                    self.camera_far,
                ));
                self.fps = Some(FPSController::create(
                    self.camera_aspect,
                    self.camera_fov,
                    self.camera_near,
                    self.camera_far,
                ));

                // Update size from actual window
                let size = winit_window.inner_size();
                self.update_size(size.width, size.height);

                // Create CAMetalLayer on main thread (macOS only)
                #[cfg(target_os = "macos")]
                let metal_layer_ptr = {
                    use winit::raw_window_handle::HasWindowHandle;
                    let rwh = winit_window.window_handle().unwrap().as_raw();
                    if let RawWindowHandle::AppKit(appkit) = rwh {
                        let nsview_nn = std::ptr::NonNull::new(appkit.ns_view.as_ptr()).unwrap();
                        let layer = unsafe { raw_window_metal::Layer::from_ns_view(nsview_nn) };
                        layer.as_ptr().as_ptr() as *const std::ffi::c_void
                    } else {
                        std::ptr::null()
                    }
                };
                #[cfg(not(target_os = "macos"))]
                let metal_layer_ptr: *const std::ffi::c_void = std::ptr::null();

                let window = Arc::new(Window {
                    window_ref: Arc::clone(&winit_window),
                    h_wnd: hwnd,
                    h_dsp: hdsp,
                    metal_layer_ptr,
                });

                self.window = Some(Arc::clone(&window));

                // Send window + shared context to render thread
                // Render thread needs the context for tessellation (shares texture manager)
                if let Err(e) = self.window_notify.send((window, shared_ctx)) {
                    eprintln!("Failed to notify about window creation: {e}");
                }

                self.initialized = true;
                self.last_frame_time = Instant::now();
            }
            Err(e) => eprintln!("Failed to create window: {e}"),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match &event {
            WindowEvent::CloseRequested => {
                self.quit_requested.store(true, Ordering::SeqCst);
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                self.update_size(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                return;
            }
            _ => {}
        }

        // Feed event to egui_winit
        if let Some(egui_winit) = &mut self.egui_winit {
            if let Some(w) = &self.window {
                let response = egui_winit.on_window_event(&*w.window_ref, &event);
                // Update egui wants flags
                self.egui_wants_pointer = egui_winit.egui_ctx().egui_wants_pointer_input();

                // In editor mode, egui consumes events; orbit camera gets non-consumed
                let consumed = self.mode == EditorMode::Editor && response.consumed;

                if !consumed {
                    self.handle_game_input(&event);
                }
                return;
            }
        }

        // Fallback: no egui_winit yet, handle game input directly
        self.handle_game_input(&event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.wants_quit() {
            event_loop.exit();
            return;
        }

        // Process commands from render thread
        self.process_render_commands();

        // Only send frame state if fully initialized
        if self.egui_winit.is_none() || self.window.is_none() || self.editor.is_none() {
            return;
        }

        // Compute delta_t
        let now = Instant::now();
        let delta_t = now.duration_since(self.last_frame_time).as_secs_f32();

        // Receive render timing info from render thread (non-blocking, keep latest)
        // The channel has capacity 1, so the render thread overwrites if we haven't consumed yet
        let mut got_render_info = false;
        while let Ok(info) = self.render_info_receiver.try_recv() {
            self.scene_snapshot = SceneSnapshot {
                tree: info.scene_tree.clone(),
                lights: info.scene_lights.clone(),
            };
            self.latest_render_info = Some(info);
            got_render_info = true;
        }

        // If we didn't get new render timing info, it means the render thread is still busy with the previous frame.
        // if we never got render info, this is the first frame, so we should proceed with UI update (using default render timing).
        if !got_render_info && self.latest_render_info.is_some() {
            return; // Skip UI update if we didn't get render timing info 
            // this prevents us from running the UI at full speed if the render thread
            // is stalled for some reason (e.g. waiting on GPU).
        }

        self.last_frame_time = now;

        let render_frame_time_ms = self
            .latest_render_info
            .as_ref()
            .map(|i| i.frame_time_ms)
            .unwrap_or(0.0);

        // Update FPS controller movement (uses delta_t)
        if self.mode == EditorMode::Play {
            if let Some(fps) = &mut self.fps {
                Camera::update(fps, delta_t);
                InputHandler::update(fps, delta_t, &mut self.settings);
            }
        }

        // Build camera snapshot (before borrowing egui_winit/window)
        let camera = self.camera_snapshot();
        let (ww, wh) = self.physical_size();

        // Gather egui input (needs &mut egui_winit + &window)
        let egui_winit = self.egui_winit.as_mut().unwrap();
        let w = self.window.as_ref().unwrap();
        let raw_input = egui_winit.take_egui_input(&*w.window_ref);
        let scale_factor = w.window_ref.scale_factor();

        // Run UI on main thread: produces FullOutput + EditCommands
        let editor = self.editor.as_mut().unwrap();
        let (full_output, edit_commands) = editor.run_ui(
            raw_input,
            &camera,
            self.mode,
            (ww, wh),
            delta_t,
            render_frame_time_ms,
            &self.scene_snapshot,
        );

        // Check for quit from editor
        if editor.pending_quit {
            self.quit_requested.store(true, Ordering::SeqCst);
            event_loop.exit();
            return;
        }

        // Handle mode toggle from editor
        if editor.pending_mode_toggle {
            self.mode = match self.mode {
                EditorMode::Editor => EditorMode::Play,
                EditorMode::Play => EditorMode::Editor,
            };
        }

        // Handle camera commands from editor (orientation snap, etc.)
        for cmd in &editor.pending_camera_commands {
            match cmd {
                CameraCommand::OrientationSnap(az, el) => {
                    if let Some(cam) = &mut self.orbit_camera {
                        cam.set_orientation(*az, *el);
                    }
                }
                CameraCommand::ToggleMode => {
                    self.mode = match self.mode {
                        EditorMode::Editor => EditorMode::Play,
                        EditorMode::Play => EditorMode::Editor,
                    };
                }
                _ => {}
            }
        }
        editor.pending_camera_commands.clear();

        // Check for scene load request
        let pending_scene_load = editor.pending_scene_load.take();

        let frame_data = FrameData {
            full_output,
            edit_commands,
            camera,
            window_size: (ww, wh),
            scale_factor,
            mode: self.mode,
            delta_t,
            render_frame_time_ms,
            pending_scene_load,
            pending_quit: false, // We already handled quit above
        };

        // Send to render thread (non-blocking with bounded channel)
        let _ = self.frame_sender.try_send(frame_data);
    }
}
