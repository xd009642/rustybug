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
use rustybug::{commands::Command, Args, DebuggerStateMachine, State};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::str::FromStr;
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
        history_len: 10,
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
    history_len: usize,
    debugger: Option<DebuggerStateMachine>,
    command_history: VecDeque<String>,
    history_index: Option<usize>,
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
        info!("Exiting");
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

    fn run_command(&mut self, command: &Command) -> Result<()> {
        match command {
            Command::Quit => self.exit(),
            Command::ToggleLogs => self.toggle_logs(),
            Command::Help => {
                self.show_help = true;
            }
            Command::Restart => {
                self.debugger = Some(DebuggerStateMachine::start(self.args.clone())?);
            }
            Command::Load(path) => {
                self.args.set_input(path.clone());
                self.debugger = Some(DebuggerStateMachine::start(self.args.clone())?);
            }
            Command::Attach(pid) => {
                self.args.set_pid(*pid);
                self.debugger = Some(DebuggerStateMachine::start(self.args.clone())?);
            }
            Command::Null => {}
            c => {
                error!("{:?} is not yet implemented", c);
            }
        }
        Ok(())
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
            KeyCode::Down => match self.history_index.as_mut() {
                Some(index) if *index + 1 >= self.command_history.len() => {
                    self.current_command.clear();
                    self.history_index = None;
                }
                Some(index) => {
                    *index += 1;
                    if let Some(command) = self.command_history.get(*index) {
                        self.current_command = command.clone();
                    }
                }
                None => {}
            },
            KeyCode::Up => {
                if let Some(index) = self.history_index.as_mut() {
                    *index = index.saturating_sub(1);
                    if let Some(command) = self.command_history.get(*index) {
                        self.current_command = command.clone();
                    }
                } else {
                    if let Some(history) = self.command_history.back() {
                        self.current_command = history.clone();
                        self.history_index = Some(self.command_history.len() - 1);
                    }
                }
            }
            KeyCode::Enter => {
                let mut command_str = String::new();
                std::mem::swap(&mut command_str, &mut self.current_command);
                let command = match Command::from_str(&command_str) {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Invalid command: {}", e);
                        // We don't need to bubble these errors up.
                        return Ok(());
                    }
                };
                self.run_command(&command);
                if command.store_in_history() && self.history_len > 0 {
                    if self.command_history.back() != Some(&command_str) {
                        while self.command_history.len() >= self.history_len.saturating_sub(1) {
                            self.command_history.pop_front();
                        }
                        // So this will put nonsense onto the history we should actually parse into proper
                        // commands
                        self.command_history.push_back(command_str);
                    }
                }
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
