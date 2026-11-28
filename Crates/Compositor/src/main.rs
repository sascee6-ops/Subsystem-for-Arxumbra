#![allow(irrefutable_let_patterns)]

mod handlers;

mod grabs;
mod input;
mod state;
mod winit;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
pub use state::Smallvil;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<Smallvil> = EventLoop::try_new()?;

    let display: Display<Smallvil> = Display::new()?;

    let mut state = Smallvil::new(&mut event_loop, display);

    crate::winit::init_winit(&mut event_loop, &mut state)?;

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    spawn_client();

    event_loop.run(None, &mut state, move |_| {
    })?;

    Ok(())
}

fn spawn_client() {
    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match (flag.as_deref(), arg) {
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            std::process::Command::new("weston-terminal").spawn().ok();
        }
    }
}
