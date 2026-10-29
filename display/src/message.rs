use std::{
    convert::Infallible,
    sync::{Arc, mpsc::Sender},
};

use anyhow::Result;
use superconsole::{Component, Dimensions, DrawMode, Line, Lines, Span, SpanError, SuperConsole};

pub enum DisplayMessage {
    Progress(Arc<ProgressMessage>, String),
    /// The command's summary, on stdout so it can be captured apart from logs and progress.
    Report(Vec<Line>, String),
    /// Anything else to show the user, such as warnings or why a run failed, on stderr.
    Notice(Vec<Line>, String),
}

impl DisplayMessage {
    pub fn render(&self, superconsole: &mut SuperConsole) {
        let render_result = match self {
            DisplayMessage::Progress(progress_message, _) => superconsole
                .render(&**progress_message)
                .map_err(anyhow::Error::new),
            DisplayMessage::Report(lines, _) => {
                emit_lines(superconsole, lines, SuperConsole::emit_aux)
            }
            DisplayMessage::Notice(lines, _) => emit_lines(superconsole, lines, SuperConsole::emit),
        };
        if let Err(e) = render_result {
            tracing::error!("Failed to render {}: {:?}", self.description(), e);
        }
    }

    fn description(&self) -> String {
        match self {
            DisplayMessage::Progress(_, description) => description.to_string(),
            DisplayMessage::Report(_, description) => description.to_string(),
            DisplayMessage::Notice(_, description) => description.to_string(),
        }
    }
}

struct EmptyComponent;

impl Component for EmptyComponent {
    type Error = Infallible;
    fn draw_unchecked(
        &self,
        _dimensions: Dimensions,
        _mode: DrawMode,
    ) -> Result<Lines, Infallible> {
        Ok(Lines(vec![]))
    }
}

// Clears the progress line, and writes the lines as normal terminal output that scrolls and wraps.
fn emit_lines(
    superconsole: &mut SuperConsole,
    lines: &[Line],
    emit: fn(&mut SuperConsole, Lines),
) -> Result<()> {
    // Superconsole truncates line emission if you emit more lines than the console has rows in a single render loop.
    // We could batch these based on tty rows, but the performance is fine without doing so, and we're not exactly running
    // a reactive ui here.
    for line in lines {
        emit(superconsole, Lines(vec![line.clone()]));
        superconsole.render(&EmptyComponent)?;
    }
    Ok(())
}

pub fn send_message(message: DisplayMessage, sender: &Sender<DisplayMessage>) {
    let send_result = sender.send(message);
    if let Err(e) = send_result {
        tracing::error!("Failed to send message: {:?}", e);
    }
}

pub struct ProgressMessage {
    pub message: String,
}
impl Component for ProgressMessage {
    type Error = SpanError;

    fn draw_unchecked(&self, _dimensions: Dimensions, _mode: DrawMode) -> Result<Lines, SpanError> {
        let output = vec![Line::from_iter([
            Span::new_unstyled("⏳ ")?,
            Span::new_unstyled_lossy(self.message.clone()),
        ])];
        Ok(Lines(output))
    }
}
