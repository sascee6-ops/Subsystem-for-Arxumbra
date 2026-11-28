#![allow(irrefutable_let_patterns)]

mod drm;
mod handlers;

mod grabs;
mod input;
mod state;
mod winit;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::Smallvil;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<'static, Smallvil> = EventLoop::try_new()?;

    let display: Display<Smallvil> = Display::new()?;

    let mut state = Smallvil::new(&mut event_loop, display);

    match std::env::args().any(|arg| arg == "--drm")
        || (!std::env::args().any(|arg| arg == "--winit")
            && std::env::var_os("WAYLAND_DISPLAY").is_none()
            && std::env::var_os("DISPLAY").is_none())
    {
        true => crate::drm::init(&mut event_loop, &mut state)?,
        false => crate::winit::init_winit(&mut event_loop, &mut state)?,
    }

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    spawn_client();

    event_loop.run(None, &mut state, move |_| {
    })?;

    Ok(())
}

fn spawn_client() {
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "-c" || flag == "--command" {
            args.next().into_iter().for_each(|command| {
                std::process::Command::new(command).spawn().ok();
            });
        }
    }
}
