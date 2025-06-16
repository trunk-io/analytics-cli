use std::{
    sync::mpsc::{channel, Receiver, Sender},
    thread::{self, JoinHandle},
};

use superconsole::{Dimensions, SuperConsole};
use terminal_size::{terminal_size, Height, Width};

use crate::message::DisplayMessage;

fn output_render_loop(receiver: Receiver<DisplayMessage>) {
    let mut superconsole = match SuperConsole::new() {
        Some(console) => console,
        None => {
            tracing::warn!("Failed to create superconsole because of incompatible TTY");
            let size = terminal_size();
            if let Some((Width(width), Height(height))) = size {
                SuperConsole::forced_new(Dimensions {
                    width: width as usize,
                    height: height as usize,
                })
            } else {
                tracing::warn!("Falling back to default dimensions for superconsole");
                // Fallback to a reasonable default size
                // This is a fallback for when the terminal size cannot be determined
                // or when running in a non-TTY environment.
                tracing::warn!("Using default dimensions for superconsole");
                tracing::warn!("This may not render correctly in some environments");
                tracing::warn!("Please use a TTY compatible terminal for best results");
                SuperConsole::forced_new(Dimensions {
                    width: 143,
                    height: 24,
                })
            }
        }
    };

    while let Ok(message) = receiver.recv() {
        message.render(&mut superconsole);
    }
}
pub fn spin_up_renderer() -> (JoinHandle<()>, Sender<DisplayMessage>) {
    let (render_sender, render_receiver) = channel::<DisplayMessage>();
    let render_handle = thread::spawn(move || {
        output_render_loop(render_receiver);
    });
    (render_handle, render_sender)
}
