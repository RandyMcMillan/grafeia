
use pathfinder_geometry::vector::{Vector2F};
use pathfinder_geometry::rect::{RectF};
use pathfinder_geometry::transform2d::Transform2F;
use pathfinder_content::color::ColorF;
use pathfinder_gl::{GLDevice, GLVersion};
use pathfinder_gpu::resources::{EmbeddedResourceLoader};
use pathfinder_renderer::scene::Scene;
use pathfinder_renderer::concurrent::rayon::RayonExecutor;
use pathfinder_renderer::concurrent::scene_proxy::SceneProxy;
use pathfinder_renderer::gpu::options::{DestFramebuffer, RendererOptions};
use pathfinder_renderer::gpu::renderer::Renderer;
use pathfinder_renderer::options::{BuildOptions, RenderTransform};
use std::env;
use glutin::{
    event::{Event, WindowEvent, DeviceEvent, KeyboardInput, ElementState, VirtualKeyCode, MouseButton, MouseScrollDelta, ModifiersState, StartCause },
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    dpi::{LogicalSize, LogicalPosition, PhysicalSize, PhysicalPosition},
    GlRequest, Api
};
use gl;
use std::time::Instant;
use std::error::Error;

pub trait Interactive: 'static {
    fn scene(&mut self) -> Scene;
    fn keyboard_input(&mut self, input: KeyboardInput) {}
}

pub fn show(mut item: impl Interactive) -> Result<(), Box<Error>> {
    let event_loop = EventLoop::new();
    let mut scale = 1.0;

    let scene = item.scene();
    let view_box = scene.view_box();
    let mut view_center = view_box.origin() + view_box.size() * Vector2F::splat(0.5);
    
    let mut window_size = view_box.size() * Vector2F::splat(scale);
    let window_builder = WindowBuilder::new()
        .with_title("A fantastic window!")
        .with_inner_size(LogicalSize::new(window_size.x() as f64, window_size.y() as f64));

    let windowed_context = glutin::ContextBuilder::new()
        .with_gl(GlRequest::Specific(Api::OpenGl, (3, 0)))
        .build_windowed(window_builder, &event_loop)
        .unwrap();
    
    let windowed_context = unsafe {
        windowed_context.make_current().unwrap()
    };

    gl::load_with(|ptr| windowed_context.get_proc_address(ptr));
    
    let window = windowed_context.window();
    let mut dpi = window.scale_factor() as f32;

    let proxy = SceneProxy::from_scene(scene, RayonExecutor);
    let mut framebuffer_size = (window_size * Vector2F::splat(dpi)).to_i32();
    // Create a Pathfinder renderer.
    let mut renderer = Renderer::new(GLDevice::new(GLVersion::GL3, 0),
        &EmbeddedResourceLoader,
        DestFramebuffer::full_window(framebuffer_size),
        RendererOptions { background_color: Some(ColorF::new(0.9, 0.85, 0.8, 1.0)) }
    );

    let mut needs_update = true;
    let mut needs_redraw = true;
    let mut cursor_pos = Vector2F::default();
    let mut dragging = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        dbg!(&event);
        match event {
            Event::NewEvents(StartCause::Init) => {
                windowed_context.window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                let scene = item.scene();
                proxy.replace_scene(scene);

                let t0 = Instant::now();

                let physical_size = window_size * Vector2F::splat(dpi);
                let new_framebuffer_size = physical_size.to_i32();
                if new_framebuffer_size != framebuffer_size {
                    framebuffer_size = new_framebuffer_size;
                    windowed_context.resize(PhysicalSize::new(framebuffer_size.x() as u32, framebuffer_size.y() as u32));
                    renderer.replace_dest_framebuffer(DestFramebuffer::full_window(framebuffer_size));
                }
                proxy.set_view_box(RectF::new(Vector2F::default(), physical_size));
                let options = BuildOptions {
                    transform: RenderTransform::Transform2D(
                        Transform2F::from_translation(physical_size.scale(0.5)) *
                        Transform2F::from_scale(Vector2F::splat(dpi * scale)) *
                        Transform2F::from_translation(-view_center)
                    ),
                    dilation: Vector2F::default(),
                    subpixel_aa_enabled: false
                };
                proxy.build_and_render(&mut renderer, options);
                windowed_context.swap_buffers().unwrap();

                println!("render: {}ms", t0.elapsed().as_millis());
            },
            Event::WindowEvent { event, .. } =>  {
                match event {
                    WindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => {
                        dpi = scale_factor as f32;
                        needs_redraw = true;
                    }
                    WindowEvent::Resized(PhysicalSize {width, height}) => {
                        window_size = Vector2F::new(width as f32, height as f32);
                        needs_redraw = true;
                    }
                    WindowEvent::KeyboardInput { input, ..  } => item.keyboard_input(input),
                    WindowEvent::CursorMoved { position: PhysicalPosition { x, y }, .. } => {
                        let new_pos = Vector2F::new(x as f32, y as f32);
                        let cursor_delta = new_pos - cursor_pos;
                        cursor_pos = new_pos;

                        if dragging {
                            view_center = view_center - cursor_delta.scale(1.0 / scale);
                            needs_redraw = true;
                        }
                    },
                    WindowEvent::MouseInput { button: MouseButton::Left, state, .. } => {
                        dragging = match state {
                            ElementState::Pressed => true,
                            ElementState::Released => false
                        };
                    },
                    WindowEvent::MouseWheel { delta, modifiers, .. } => {
                        let delta = match delta {
                            MouseScrollDelta::PixelDelta(LogicalPosition { x: dx, y: dy }) => Vector2F::new(dx as f32, dy as f32),
                            MouseScrollDelta::LineDelta(dx, dy) => Vector2F::new(dx as f32, -dy as f32).scale(10.)
                        };
                        if modifiers.ctrl() {
                            scale *= (-0.02 * delta.y()).exp();
                            needs_redraw = true;
                        } else {
                            view_center = view_center - delta.scale(1.0 / scale);
                            needs_redraw = true;
                        }
                    }
                    WindowEvent::CloseRequested => {
                        println!("The close button was pressed; stopping");
                        *control_flow = ControlFlow::Exit
                    },
                    _ => {}
                }
                let window = windowed_context.window();
                window.request_redraw();
            }
            _ => {}
        }
    });
}

impl Interactive for Scene {
    fn scene(&mut self) -> Scene {
        self.clone()
    }
}
