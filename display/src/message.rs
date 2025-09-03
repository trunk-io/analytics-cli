use std::sync::{mpsc::Sender, Arc};

use anyhow::Result;
use superconsole::{Component, Dimensions, DrawMode, Line, Lines, Span, SuperConsole};

use crate::end_output::{display_end, EndOutput};

pub enum DisplayMessage {
    Progress(Arc<dyn Component + Send + Sync>, String),
    Final(Arc<dyn EndOutput + Send + Sync>, String),
}

impl DisplayMessage {
    pub fn render(&self, superconsole: &mut SuperConsole) {
        let render_result = match self {
            DisplayMessage::Progress(progress_message, _) => {
                superconsole.render(&**progress_message)
            }
            DisplayMessage::Final(final_message, _) => display_end(superconsole, &**final_message),
        };
        if let Err(e) = render_result {
            tracing::error!("Failed to render {}: {:?}", self.description(), e);
        }
    }

    fn description(&self) -> String {
        match self {
            DisplayMessage::Progress(_, description) => description.to_string(),
            DisplayMessage::Final(_, description) => description.to_string(),
        }
    }
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
    fn draw_unchecked(&self, _dimensions: Dimensions, _mode: DrawMode) -> Result<Lines> {
        let output = vec![Line::from_iter([
            Span::new_unstyled("⏳ ")?,
            Span::new_unstyled_lossy(self.message.clone()),
        ])];
        Ok(Lines(output))
    }
}
