use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::config::{AcmeConfig, Config, StealthConfig, UserConfig};

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Listen,
    Domain,
    AcmeEmail,
    AcmeStaging,
    AcmeCacheDir,
    StealthServerName,
    Users,
}

const FIELDS: &[Field] = &[
    Field::Listen,
    Field::Domain,
    Field::AcmeEmail,
    Field::AcmeStaging,
    Field::AcmeCacheDir,
    Field::StealthServerName,
    Field::Users,
];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Navigate,
    Edit,
    UserAdd,
}

struct UserAddState {
    field: UserAddField,
    username: String,
    password: String,
    password_confirm: String,
}

#[derive(Clone, Copy, PartialEq)]
enum UserAddField {
    Username,
    Password,
    PasswordConfirm,
}

pub struct SetupApp {
    selected: usize,
    mode: Mode,

    listen: String,
    domain: String,
    acme_email: String,
    acme_staging: bool,
    acme_cache_dir: String,
    stealth_server_name: String,
    users: Vec<UserConfig>,

    user_add: Option<UserAddState>,
    status: Option<String>,
    should_quit: bool,
    output_path: String,
}

impl SetupApp {
    pub fn new(output_path: String) -> Self {
        Self {
            selected: 0,
            mode: Mode::Navigate,
            listen: "0.0.0.0:443".into(),
            domain: String::new(),
            acme_email: String::new(),
            acme_staging: false,
            acme_cache_dir: "/var/lib/https-proxy/acme".into(),
            stealth_server_name: "nginx/1.24.0".into(),
            users: Vec::new(),
            user_add: None,
            status: None,
            should_quit: false,
            output_path,
        }
    }

    fn current_field(&self) -> Field {
        FIELDS[self.selected]
    }

    fn current_value_mut(&mut self) -> Option<&mut String> {
        match self.current_field() {
            Field::Listen => Some(&mut self.listen),
            Field::Domain => Some(&mut self.domain),
            Field::AcmeEmail => Some(&mut self.acme_email),
            Field::AcmeCacheDir => Some(&mut self.acme_cache_dir),
            Field::StealthServerName => Some(&mut self.stealth_server_name),
            Field::AcmeStaging | Field::Users => None,
        }
    }

    fn build_config(&self) -> Config {
        Config {
            listen: self.listen.clone(),
            domain: self.domain.clone(),
            acme: AcmeConfig {
                email: self.acme_email.clone(),
                staging: self.acme_staging,
                cache_dir: PathBuf::from(&self.acme_cache_dir),
            },
            users: self.users.clone(),
            stealth: StealthConfig {
                server_name: self.stealth_server_name.clone(),
            },
        }
    }

    fn save_config(&mut self) {
        let config = self.build_config();

        if config.domain.is_empty() {
            self.status = Some("Error: domain is required".into());
            return;
        }
        if config.acme.email.is_empty() {
            self.status = Some("Error: ACME email is required".into());
            return;
        }
        if config.users.is_empty() {
            self.status = Some("Error: add at least one user".into());
            return;
        }

        match config.save(&self.output_path) {
            Ok(()) => {
                self.status = Some(format!("Saved to {}", self.output_path));
                self.should_quit = true;
            }
            Err(e) => {
                self.status = Some(format!("Error saving: {e}"));
            }
        }
    }

    fn handle_navigate(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected < FIELDS.len() - 1 {
                    self.selected += 1;
                }
            }
            KeyCode::Enter => match self.current_field() {
                Field::AcmeStaging => {
                    self.acme_staging = !self.acme_staging;
                }
                Field::Users => {
                    self.mode = Mode::UserAdd;
                    self.user_add = Some(UserAddState {
                        field: UserAddField::Username,
                        username: String::new(),
                        password: String::new(),
                        password_confirm: String::new(),
                    });
                }
                _ => {
                    self.mode = Mode::Edit;
                }
            },
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.current_field() == Field::Users && !self.users.is_empty() {
                    self.users.pop();
                }
            }
            KeyCode::Char('s') => {
                self.save_config();
            }
            _ => {}
        }
    }

    fn handle_edit(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.mode = Mode::Navigate;
            }
            KeyCode::Enter => {
                self.mode = Mode::Navigate;
            }
            KeyCode::Backspace => {
                if let Some(val) = self.current_value_mut() {
                    val.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(val) = self.current_value_mut() {
                    val.push(c);
                }
            }
            _ => {}
        }
    }

    fn handle_user_add(&mut self, code: KeyCode) {
        let state = match self.user_add.as_mut() {
            Some(s) => s,
            None => return,
        };

        match code {
            KeyCode::Esc => {
                self.user_add = None;
                self.mode = Mode::Navigate;
            }
            KeyCode::Tab => {
                state.field = match state.field {
                    UserAddField::Username => UserAddField::Password,
                    UserAddField::Password => UserAddField::PasswordConfirm,
                    UserAddField::PasswordConfirm => UserAddField::Username,
                };
            }
            KeyCode::BackTab => {
                state.field = match state.field {
                    UserAddField::Username => UserAddField::PasswordConfirm,
                    UserAddField::Password => UserAddField::Username,
                    UserAddField::PasswordConfirm => UserAddField::Password,
                };
            }
            KeyCode::Backspace => {
                let field = match state.field {
                    UserAddField::Username => &mut state.username,
                    UserAddField::Password => &mut state.password,
                    UserAddField::PasswordConfirm => &mut state.password_confirm,
                };
                field.pop();
            }
            KeyCode::Char(c) => {
                let field = match state.field {
                    UserAddField::Username => &mut state.username,
                    UserAddField::Password => &mut state.password,
                    UserAddField::PasswordConfirm => &mut state.password_confirm,
                };
                field.push(c);
            }
            KeyCode::Enter => {
                if state.field != UserAddField::PasswordConfirm {
                    state.field = match state.field {
                        UserAddField::Username => UserAddField::Password,
                        UserAddField::Password => UserAddField::PasswordConfirm,
                        UserAddField::PasswordConfirm => UserAddField::PasswordConfirm,
                    };
                    return;
                }

                if state.username.is_empty() {
                    self.status = Some("Username cannot be empty".into());
                    return;
                }
                if state.password.is_empty() {
                    self.status = Some("Password cannot be empty".into());
                    return;
                }
                if state.password != state.password_confirm {
                    self.status = Some("Passwords do not match".into());
                    return;
                }

                self.users.push(UserConfig {
                    username: state.username.clone(),
                    password: state.password.clone(),
                });
                self.status = Some(format!("Added user '{}'", state.username));
                self.user_add = None;
                self.mode = Mode::Navigate;
            }
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        let outer = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

        // Title
        let title = Paragraph::new(Line::from(vec![
            Span::raw("  HTTPS Proxy Setup").bold(),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(title, outer[0]);

        // Main form
        let form_block = Block::default()
            .title(" Configuration ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Gray));
        let form_inner = form_block.inner(outer[1]);
        frame.render_widget(form_block, outer[1]);

        let rows = Layout::vertical(
            std::iter::repeat(Constraint::Length(2))
                .take(FIELDS.len())
                .chain(std::iter::once(Constraint::Min(0)))
                .collect::<Vec<_>>(),
        )
        .split(form_inner);

        for (i, field) in FIELDS.iter().enumerate() {
            let selected = i == self.selected;
            let editing = selected && self.mode == Mode::Edit;
            let label = match field {
                Field::Listen => "Listen Address",
                Field::Domain => "Domain",
                Field::AcmeEmail => "ACME Email",
                Field::AcmeStaging => "ACME Staging",
                Field::AcmeCacheDir => "Cert Cache Dir",
                Field::StealthServerName => "Server Name",
                Field::Users => "Users",
            };

            let value_str = match field {
                Field::Users => {
                    if self.users.is_empty() {
                        "(none — press Enter to add)".to_string()
                    } else {
                        self.users
                            .iter()
                            .map(|u| u.username.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                }
                Field::AcmeStaging => {
                    if self.acme_staging {
                        "[x] staging".to_string()
                    } else {
                        "[ ] production".to_string()
                    }
                }
                _ => self.current_value_for(*field),
            };

            let cursor = if editing { "_" } else { "" };

            let label_style = if selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let value_style = if editing {
                Style::default().fg(Color::White)
            } else if selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            let indicator = if selected { "> " } else { "  " };

            let line = Line::from(vec![
                Span::styled(indicator, label_style),
                Span::styled(format!("{label:<18}"), label_style),
                Span::styled(format!("{value_str}{cursor}"), value_style),
            ]);

            frame.render_widget(Paragraph::new(line), rows[i]);
        }

        // Status bar
        let status_text = match &self.status {
            Some(msg) => msg.clone(),
            None => match self.mode {
                Mode::Navigate => {
                    "↑↓ navigate  Enter edit  d delete user  s save  q quit".into()
                }
                Mode::Edit => "Type to edit  Enter confirm  Esc cancel".into(),
                Mode::UserAdd => "Tab next field  Enter confirm  Esc cancel".into(),
            },
        };
        let status_style = if self.status.as_ref().is_some_and(|s| s.starts_with("Error")) {
            Style::default().fg(Color::Red)
        } else if self.status.is_some() {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let status = Paragraph::new(Span::styled(format!("  {status_text}"), status_style))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Gray)));
        frame.render_widget(status, outer[2]);

        // User add popup
        if let Some(ref state) = self.user_add {
            self.draw_user_popup(frame, area, state);
        }
    }

    fn current_value_for(&self, field: Field) -> String {
        match field {
            Field::Listen => self.listen.clone(),
            Field::Domain => self.domain.clone(),
            Field::AcmeEmail => self.acme_email.clone(),
            Field::AcmeStaging => unreachable!(),
            Field::AcmeCacheDir => self.acme_cache_dir.clone(),
            Field::StealthServerName => self.stealth_server_name.clone(),
            Field::Users => unreachable!(),
        }
    }

    fn draw_user_popup(&self, frame: &mut Frame, area: Rect, state: &UserAddState) {
        let popup_width = 50u16.min(area.width.saturating_sub(4));
        let popup_height = 10u16.min(area.height.saturating_sub(4));
        let popup = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        frame.render_widget(Clear, popup);

        let block = Block::default()
            .title(" Add User ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner);

        let fields = [
            ("Username", &state.username, false, UserAddField::Username),
            ("Password", &state.password, true, UserAddField::Password),
            (
                "Confirm",
                &state.password_confirm,
                true,
                UserAddField::PasswordConfirm,
            ),
        ];

        for (i, (label, value, masked, field_id)) in fields.iter().enumerate() {
            let active = state.field == *field_id;
            let display_val = if *masked {
                "*".repeat(value.len())
            } else {
                (*value).clone()
            };
            let cursor = if active { "_" } else { "" };
            let label_style = if active {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let val_style = if active {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::Gray)
            };
            let indicator = if active { "> " } else { "  " };

            let line = Line::from(vec![
                Span::styled(indicator, label_style),
                Span::styled(format!("{label:<12}"), label_style),
                Span::styled(format!("{display_val}{cursor}"), val_style),
            ]);
            frame.render_widget(Paragraph::new(line), rows[i]);
        }
    }
}

pub fn run_setup(output_path: String) -> anyhow::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut app = SetupApp::new(output_path);

    loop {
        terminal.draw(|f| app.draw(f))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Clear status on any keypress
            app.status = None;

            match app.mode {
                Mode::Navigate => app.handle_navigate(key.code),
                Mode::Edit => app.handle_edit(key.code),
                Mode::UserAdd => app.handle_user_add(key.code),
            }
        }

        if app.should_quit {
            break;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    Ok(())
}
