use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use rustybug::{Args, DebuggerStateMachine, State};
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::{self, layer::SubscriberExt, util::SubscriberInitExt};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget};

const HELP_TEXT: &str = "Rustybug
This is a simple debugger mainly for playing with ptrace. But being a debugger there are
some commands to learn:

attach <PID>   Attach to the given PID for debugging
load <PATH>    Loads the given program and starts debugging it. TODO args
restart        Restart the program/attached pid you launched rustybug with
l logs         Show the debug logs
q quit         Quit rustybuy
? help         Show this message

Press any key to dismiss this message.
";

fn main() -> anyhow::Result<()> {
    init_logging()?;
    let args = Args::parse();

    let mut terminal = ratatui::init();
    terminal.hide_cursor();

    let mut app = App {
        args,
        show_logs: true,
        ..Default::default()
    };
    if let Err(e) = app.run(&mut terminal) {
        ratatui::restore();
        eprintln!("{}", e);
    } else {
        ratatui::restore()
    }

    Ok(())
}

fn init_logging() -> Result<()> {
    tracing_subscriber::registry()
        .with(tui_logger::tracing_subscriber_layer())
        .init();
    tui_logger::init_logger(tui_logger::LevelFilter::Trace)?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct App {
    args: Args,
    exit: bool,
    show_help: bool,
    show_logs: bool,
    current_command: String,
    debugger: Option<DebuggerStateMachine>,
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.debugger = Some(DebuggerStateMachine::start(self.args.clone())?);
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;

            if let Some(sm) = self.debugger.as_mut() {
                let state = sm.wait()?;

                if state == State::Finished {
                    self.debugger = None;
                    info!("Done");
                    //self.exit();
                }
            }
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
        if self.show_help {
            let area = frame.area();
            let rect = popup_area(area, 80, 60);
            frame.render_widget(Clear, area);

            let block = Block::bordered().title("Help");

            let paragraph = Paragraph::new(HELP_TEXT).block(block);

            frame.render_widget(paragraph, area);
        }
    }

    fn handle_events(&mut self) -> Result<()> {
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => Ok(()),
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        if self.show_help {
            self.show_help = false;
            return Ok(());
        }
        match key_event.code {
            KeyCode::Char(c) => {
                self.current_command.push(c);
            }
            KeyCode::Enter => {
                match self.current_command.as_str() {
                    "q" | "quit" => self.exit(),
                    "l" | "logs" => self.toggle_logs(),
                    "?" | "help" => {
                        self.show_help = true;
                    }
                    "restart" => {
                        self.debugger = Some(DebuggerStateMachine::start(self.args.clone())?);
                    }
                    x if x.starts_with("load ") => {
                        let path = x.trim_start_matches("load ");
                        let path = PathBuf::from(path);
                        self.args.set_input(path);
                        self.debugger = Some(DebuggerStateMachine::start(self.args.clone())?);
                    }
                    x if x.starts_with("attach ") => {
                        let pid_str = x.trim_start_matches("attach ");
                        let pid = pid_str.parse::<i32>();
                        match pid {
                            Ok(pid) => {
                                self.args.set_pid(pid);
                                self.debugger =
                                    Some(DebuggerStateMachine::start(self.args.clone())?);
                            }
                            Err(e) => {
                                error!(
                                    "attach expects a pid. '{}' is not a valid pid: {}",
                                    pid_str, e
                                );
                            }
                        }
                    }
                    x if !x.trim().is_empty() => {
                        error!("Unknown command: {}", self.current_command)
                    }
                    _ => {}
                }
                self.current_command.clear();
            }
            KeyCode::Esc => self.current_command.clear(),
            KeyCode::Backspace => {
                self.current_command.pop();
            }
            _ => {}
        }
        Ok(())
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn toggle_logs(&mut self) {
        self.show_logs = !self.show_logs;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(" RustyBug ".bold());
        let instructions = Line::from(vec![
            " Quit ".into(),
            "<Q> ".blue().bold(),
            " Help ".into(),
            "<?> ".blue().bold(),
        ]);

        let [view, prompt] = if self.show_logs {
            let [view, logs, prompt] =
                Layout::vertical([Constraint::Fill(5), Constraint::Fill(3), Constraint::Max(1)])
                    .areas(area);

            let block = Block::bordered().border_set(border::THICK);

            TuiLoggerWidget::default()
                .output_separator(':')
                .output_timestamp(Some("%H:%M:%S".to_string()))
                .output_level(Some(TuiLoggerLevelOutput::Abbreviated))
                .output_target(true)
                .output_file(true)
                .output_line(true)
                .block(block)
                .render(logs, buf);
            [view, prompt]
        } else {
            Layout::vertical([Constraint::Fill(8), Constraint::Max(1)]).areas(area)
        };

        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        let view_window = Text::from(vec![Line::from(self.args.name())]);

        Paragraph::new(view_window)
            .centered()
            .block(block)
            .render(view, buf);

        Line::from(vec![
            Span::styled("rb> ", Style::new().blue()),
            Span::raw(&self.current_command),
        ])
        .left_aligned()
        .render(prompt, buf);
    }
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}
