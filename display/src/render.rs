use std::{
    io::{self, Write},
    sync::mpsc::{Receiver, Sender, channel},
    thread::{self, JoinHandle},
};

use superconsole::{Builder, Dimensions, SuperConsole};
use terminal_size::{Height, Width, terminal_size};

use crate::message::DisplayMessage;

/// Cursor hide/show escapes superconsole wraps around every frame. Log viewers such as GitHub
/// Actions don't recognise them and print `25l`/`25h` at the start of each line.
const CURSOR_VISIBILITY: [&[u8]; 2] = [b"\x1b[?25l", b"\x1b[?25h"];

/// Stderr minus [`CURSOR_VISIBILITY`], for when there is no interactive terminal.
struct NonTtyStderr(io::Stderr);

impl Write for NonTtyStderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write_all(&strip_cursor_visibility(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn strip_cursor_visibility(buf: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len());
    let mut rest = buf;
    while !rest.is_empty() {
        match CURSOR_VISIBILITY.iter().find(|seq| rest.starts_with(seq)) {
            Some(seq) => rest = &rest[seq.len()..],
            None => {
                out.push(rest[0]);
                rest = &rest[1..];
            }
        }
    }
    out
}

fn forced_console(fallback_size: Dimensions) -> SuperConsole {
    let mut builder = Builder::new();
    builder.write_to(Box::new(NonTtyStderr(io::stderr())));
    builder
        .build_forced(fallback_size)
        .unwrap_or_else(|_| SuperConsole::forced_new(fallback_size))
}

fn output_render_loop(receiver: Receiver<DisplayMessage>) {
    let mut superconsole = match SuperConsole::new() {
        Some(console) => console,
        None => {
            tracing::warn!("Failed to create superconsole because of incompatible TTY");
            let size = terminal_size();
            if let Some((Width(width), Height(height))) = size {
                forced_console(Dimensions {
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
                forced_console(Dimensions {
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

#[cfg(test)]
mod tests {
    use super::strip_cursor_visibility;

    #[test]
    fn strips_only_cursor_visibility_escapes() {
        let frame = b"\x1b[?25l\x1b[1G\x1b[1mUpload:\x1b[0m\x1b[K\n\x1b[?25h\x1b[J";
        assert_eq!(
            strip_cursor_visibility(frame),
            b"\x1b[1G\x1b[1mUpload:\x1b[0m\x1b[K\n\x1b[J"
        );
    }
}
