use anyhow::Result;
use superconsole::{Component, Dimensions, DrawMode, Line, Lines, SuperConsole};

pub trait EndOutput {
    fn output(&self) -> Result<Vec<Line>>;
}

enum EmptyComponent {
    Base,
}
impl Component for EmptyComponent {
    fn draw_unchecked(&self, _dimensions: Dimensions, _mode: DrawMode) -> Result<Lines> {
        Ok(Lines(vec![]))
    }
}

// Clears out any renders, and outputs the entire output of result, scrolling the terminal and wrapping long lines (you know, normal terminal output)
pub fn display_end(superconsole: &mut SuperConsole, result: &impl EndOutput) -> Result<()> {
    let lines = result.output()?;
    // Superconsole truncates line emission if you emit more lines than the console has rows in a single render loop.
    // We could batch these based on tty rows, but the performance is fine without doing so, and we're not exactly running
    // a reactive ui here.
    for line in lines {
        superconsole.emit_now(Lines(vec![line]), &EmptyComponent::Base)?;
    }
    Ok(())
}
