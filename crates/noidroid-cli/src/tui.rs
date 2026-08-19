//! `noidroid tui` — the viewer, and the one place the Stand gets to move.
//!
//! The manifesto's V0.1 is a timeline you can stand inside, and its claim is that the
//! fundamental interaction is not *rewind* but **explore from here**. So this is not a
//! dashboard: it is three panes and one verb. Pressing `e` on a recorded decision
//! reconstructs the prefix, diverges, and comes back with a new trajectory.
//!
//! The flourishes — menacing glyphs, the colour cycling on the Stand's name, the
//! impact frame when an ability fires — are on by default and off with `--plain`.
//! They never carry information that is not also written down in words, because a
//! terminal that cannot draw them still has to be usable.

use std::collections::BTreeMap;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use noidroid_core::engine::{self, Mode, RunSpec};
use noidroid_core::model::{Action, Intervention, Step, StepNote, Trajectory};
use noidroid_core::{Digest, Error, Repo, Result};

use crate::palette::{self, Ink};

const TICK: Duration = Duration::from_millis(90);
/// Long enough to read as an impact, short enough not to be in the way.
const FLASH_FRAMES: u8 = 6;

fn colour(ink: Ink) -> Color {
    let (r, g, b) = ink.rgb();
    Color::Rgb(r, g, b)
}

fn provenance_colour(label: &str) -> Color {
    match label {
        "real" => colour(palette::PHOSPHOR),
        "live" => colour(palette::CHROME),
        "simulated" => colour(palette::VIOLET),
        _ => colour(palette::AMBER),
    }
}

fn status_colour(status: &str) -> Color {
    match status {
        "success" => colour(palette::PHOSPHOR),
        "failure" => colour(palette::CRIMSON),
        _ => colour(palette::AMBER),
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Focus {
    Trajectories,
    Timeline,
}

enum Note {
    Good(String),
    Bad(String),
}

/// The option picker shown when exploring from a recorded decision.
struct Picker {
    decision: String,
    options: Vec<String>,
    selected: usize,
}

struct App<'a> {
    repo: &'a Repo,
    cwd: PathBuf,
    plain: bool,
    trajectories: Vec<Trajectory>,
    trajectory: usize,
    chain: Vec<(Digest, Step)>,
    notes: BTreeMap<u64, StepNote>,
    step: usize,
    focus: Focus,
    frame: u64,
    flash: u8,
    note: Option<Note>,
    picker: Option<Picker>,
    busy: Option<String>,
}

impl<'a> App<'a> {
    fn new(repo: &'a Repo, cwd: &Path, start: Option<String>, plain: bool) -> Result<App<'a>> {
        let trajectories = repo.list_trajectories()?;
        let trajectory = start
            .and_then(|name| trajectories.iter().position(|t| t.name == name))
            .unwrap_or(0);
        let mut app = App {
            repo,
            cwd: cwd.to_path_buf(),
            plain,
            trajectories,
            trajectory,
            chain: Vec::new(),
            notes: BTreeMap::new(),
            step: 0,
            focus: Focus::Timeline,
            frame: 0,
            flash: 0,
            note: None,
            picker: None,
            busy: None,
        };
        app.load()?;
        Ok(app)
    }

    fn load(&mut self) -> Result<()> {
        self.chain.clear();
        self.notes.clear();
        if let Some(t) = self.trajectories.get(self.trajectory) {
            self.chain = self.repo.chain(t)?;
            self.notes = self
                .repo
                .load_notes(&t.name)?
                .into_iter()
                .map(|n| (n.index, n))
                .collect();
        }
        self.step = self.step.min(self.chain.len().saturating_sub(1));
        Ok(())
    }

    fn reload(&mut self, select: Option<&str>) -> Result<()> {
        self.trajectories = self.repo.list_trajectories()?;
        if let Some(name) = select {
            if let Some(i) = self.trajectories.iter().position(|t| t.name == name) {
                self.trajectory = i;
                self.step = 0;
            }
        }
        self.trajectory = self
            .trajectory
            .min(self.trajectories.len().saturating_sub(1));
        self.load()
    }

    fn selected_step(&self) -> Option<&Step> {
        self.chain.get(self.step).map(|(_, s)| s)
    }

    /// Open the picker if this checkpoint is a declared decision; otherwise say why
    /// there is nothing to explore from here.
    fn begin_explore(&mut self) {
        let Some(step) = self.selected_step().cloned() else {
            return;
        };
        match &step.action {
            Action::Decide {
                name,
                options,
                choice,
            } => {
                let taken = choice.as_str().map(str::to_string);
                let options: Vec<String> = options
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .filter(|candidate| Some(candidate) != taken.as_ref())
                            .collect()
                    })
                    .unwrap_or_default();
                if options.is_empty() {
                    self.note = Some(Note::Bad(
                        "this decision recorded no alternative to take instead".into(),
                    ));
                    return;
                }
                self.picker = Some(Picker {
                    decision: name.clone(),
                    options,
                    selected: 0,
                });
            }
            Action::Call { target, .. } => {
                self.note = Some(Note::Bad(format!(
                    "step {} is a call to {target}; explore it from the shell with \
                     `noidroid branch …@{} --result '<json>'` or `--fail '<message>'`",
                    step.index, step.index
                )));
            }
            _ => {
                self.note = Some(Note::Bad(
                    "genesis and finish are not interactions; pick a step between them".into(),
                ));
            }
        }
    }

    /// Reconstruct the prefix, diverge, and come back with a new trajectory.
    fn explore(&mut self, decision: String, value: String) {
        let Some(parent) = self.trajectories.get(self.trajectory).cloned() else {
            return;
        };
        let at = match self.selected_step() {
            Some(step) => step.index,
            None => return,
        };
        let label = self.repo.next_name("alt");
        let spec = RunSpec {
            command: parent.command.clone(),
            launch_dir: self.cwd.clone(),
            name: Some(label.clone()),
            env: Vec::new(),
            auto: false,
            watch: None,
        };
        let outcome = engine::run(
            self.repo,
            &spec,
            Mode::Branch {
                at,
                intervention: Intervention::ReplaceDecision {
                    name: decision,
                    value: serde_json::Value::String(value.clone()),
                },
                simulate: BTreeMap::new(),
            },
            Some(&parent),
        );
        match outcome {
            Ok(report) => {
                let prefix_broke = report.divergences.iter().any(|d| d.index < at);
                match report.trajectory {
                    Some(branch) if !prefix_broke => {
                        let status = branch.outcome.status.clone();
                        let name = branch.name.clone();
                        let _ = self.reload(Some(&name));
                        self.flash = FLASH_FRAMES;
                        self.note = Some(Note::Good(format!(
                            "{name}: {value} → {status}  ({} step(s) shared with {})",
                            at, parent.name
                        )));
                    }
                    _ => {
                        self.note = Some(Note::Bad(format!(
                            "could not branch from {}@{at}: the prefix did not reconstruct",
                            parent.name
                        )));
                    }
                }
            }
            Err(e) => self.note = Some(Note::Bad(format!("{e}"))),
        }
    }

    fn replay(&mut self) {
        let Some(t) = self.trajectories.get(self.trajectory).cloned() else {
            return;
        };
        let spec = RunSpec {
            command: t.command.clone(),
            launch_dir: self.cwd.clone(),
            name: None,
            env: Vec::new(),
            auto: false,
            watch: None,
        };
        match engine::run(self.repo, &spec, Mode::Replay, Some(&t)) {
            Ok(report) if report.faithful() => {
                self.flash = FLASH_FRAMES;
                self.note = Some(Note::Good(format!(
                    "{}: {}/{} steps re-derived to the same objects",
                    t.name, report.reproduced, report.expected
                )));
            }
            Ok(report) => {
                let first = report
                    .divergences
                    .first()
                    .map(|d| format!("@{} {}", d.index, d.kind.label()))
                    .unwrap_or_else(|| "diverged".into());
                self.note = Some(Note::Bad(format!("{}: {first}", t.name)));
            }
            Err(e) => self.note = Some(Note::Bad(format!("{e}"))),
        }
    }
}

pub fn run(repo: &Repo, cwd: &Path, start: Option<String>, plain: bool) -> Result<ExitCode> {
    let mut app = App::new(repo, cwd, start, plain)?;
    if app.trajectories.is_empty() {
        return Err(Error::NotFound(
            "no trajectories yet — record one with `noidroid run -- <command>`".into(),
        ));
    }

    let mut terminal = enter()?;
    let result = drive(&mut terminal, &mut app);
    leave(&mut terminal)?;
    result.map(|()| ExitCode::SUCCESS)
}

fn enter() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(out))?)
}

fn leave(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn drive(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        // Work that blocks gets announced first, so the screen is never a lie about
        // what the tool is doing.
        if let Some(job) = app.busy.take() {
            match job.as_str() {
                "replay" => app.replay(),
                _ => {
                    if let Some((decision, value)) = job.split_once('=') {
                        app.explore(decision.to_string(), value.to_string());
                    }
                }
            }
            continue;
        }

        let deadline = Instant::now() + TICK;
        while Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            if !event::poll(left)? {
                break;
            }
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if handle(app, key.code) {
                    return Ok(());
                }
            }
        }
        app.frame = app.frame.wrapping_add(1);
        app.flash = app.flash.saturating_sub(1);
    }
}

/// Returns true when it is time to leave.
fn handle(app: &mut App, code: KeyCode) -> bool {
    if let Some(picker) = app.picker.as_mut() {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => app.picker = None,
            KeyCode::Up | KeyCode::Char('k') => {
                picker.selected = picker.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                picker.selected = (picker.selected + 1).min(picker.options.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                let decision = picker.decision.clone();
                let value = picker.options[picker.selected].clone();
                app.picker = None;
                app.busy = Some(format!("{decision}={value}"));
            }
            _ => {}
        }
        return false;
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Tab => {
            app.focus = match app.focus {
                Focus::Trajectories => Focus::Timeline,
                Focus::Timeline => Focus::Trajectories,
            }
        }
        KeyCode::Up | KeyCode::Char('k') => match app.focus {
            Focus::Timeline => app.step = app.step.saturating_sub(1),
            Focus::Trajectories => {
                app.trajectory = app.trajectory.saturating_sub(1);
                app.step = 0;
                let _ = app.load();
            }
        },
        KeyCode::Down | KeyCode::Char('j') => match app.focus {
            Focus::Timeline => {
                app.step = (app.step + 1).min(app.chain.len().saturating_sub(1));
            }
            Focus::Trajectories => {
                // Saturating: an empty list would underflow, and the list is only
                // guaranteed non-empty at startup, not after a reload.
                app.trajectory = (app.trajectory + 1).min(app.trajectories.len().saturating_sub(1));
                app.step = 0;
                let _ = app.load();
            }
        },
        KeyCode::Char('e') => app.begin_explore(),
        KeyCode::Char('r') => app.busy = Some("replay".into()),
        KeyCode::Char('g') => app.step = 0,
        KeyCode::Char('G') => app.step = app.chain.len().saturating_sub(1),
        _ => {}
    }
    false
}

// ---------------------------------------------------------------------- drawing

fn draw(f: &mut Frame, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(4),
        ])
        .split(f.area());

    draw_header(f, rows[0], app);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(26),
            Constraint::Min(30),
            Constraint::Length(40),
        ])
        .split(rows[1]);

    draw_trajectories(f, columns[0], app);
    draw_timeline(f, columns[1], app);
    draw_checkpoint(f, columns[2], app);
    draw_footer(f, rows[2], app);

    if let Some(picker) = &app.picker {
        draw_picker(f, f.area(), picker);
    }
    if let Some(job) = &app.busy {
        draw_busy(f, f.area(), job);
    }
}

/// The Stand's name, and the menace behind it.
fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let cycle = [
        palette::CHROME,
        palette::CYAN,
        palette::VIOLET,
        palette::PHOSPHOR,
    ];
    let name_colour = if app.plain {
        colour(palette::CHROME)
    } else {
        colour(cycle[(app.frame / 8) as usize % cycle.len()])
    };
    let menace = if app.plain {
        String::new()
    } else {
        // Drifts sideways, the way it does across a panel.
        let pad = " ".repeat((app.frame / 3) as usize % 6);
        format!("{pad}{}", crate::stand::MENACE)
    };
    let line = Line::from(vec![
        Span::styled(
            "「PARANOID ANDROID」",
            Style::default()
                .fg(name_colour)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(menace, Style::default().fg(colour(palette::ASH))),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colour(palette::INDIGO)))
        .title(Span::styled(
            " explore from here ",
            Style::default().fg(colour(palette::ASH)),
        ));
    f.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_trajectories(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .trajectories
        .iter()
        .map(|t| {
            let forked = t.forked_from.is_some();
            ListItem::new(Line::from(vec![
                Span::styled(
                    if forked { "└ " } else { "● " },
                    Style::default().fg(colour(palette::INDIGO)),
                ),
                Span::styled(
                    format!("{:<12}", t.name),
                    Style::default().fg(colour(palette::CHROME)),
                ),
                Span::styled(
                    t.outcome.status.clone(),
                    Style::default().fg(status_colour(&t.outcome.status)),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.trajectory));
    let list = List::new(items)
        .block(pane("TRAJECTORIES", app.focus == Focus::Trajectories))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_timeline(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .chain
        .iter()
        .map(|(_, step)| {
            let marker = match (&step.action, step.intervention.is_some()) {
                (_, true) => "◆",
                (Action::Finish { status, .. }, _) if status == "success" => "✔",
                (Action::Finish { .. }, _) => "✘",
                _ => "●",
            };
            let delivery = app
                .notes
                .get(&step.index)
                .map(|n| n.delivery.label())
                .unwrap_or("-");
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>3} ", step.index),
                    Style::default().fg(colour(palette::ASH)),
                ),
                Span::styled(
                    format!("{marker} "),
                    Style::default().fg(provenance_colour(step.provenance.label())),
                ),
                Span::styled(
                    truncate(
                        &step.action.summary(),
                        area.width.saturating_sub(22) as usize,
                    ),
                    Style::default().fg(colour(palette::CHROME)),
                ),
                Span::raw(" "),
                Span::styled(
                    delivery.to_string(),
                    Style::default().fg(colour(palette::ASH)),
                ),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.step));
    // The impact frame: when an ability has just fired, the selected step is struck.
    let highlight = if app.flash > 0 && !app.plain {
        Style::default()
            .fg(colour(palette::VIOLET))
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    };
    let title = if app.flash > 0 && !app.plain {
        "TIMELINE ≫≫≫"
    } else {
        "TIMELINE"
    };
    let list = List::new(items)
        .block(pane(title, app.focus == Focus::Timeline))
        .highlight_style(highlight);
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_checkpoint(f: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some((digest, step)) = app.chain.get(app.step) {
        let summary = step.action.summary();
        lines.push(field("step", digest.short()));
        lines.push(field("action", &summary));
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<12}", "provenance"),
                Style::default().fg(colour(palette::ASH)),
            ),
            Span::styled(
                step.provenance.label().to_string(),
                Style::default().fg(provenance_colour(step.provenance.label())),
            ),
        ]));
        if let Some(intervention) = &step.intervention {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<12}", "intervention"),
                    Style::default().fg(colour(palette::ASH)),
                ),
                Span::styled(
                    intervention.summary(),
                    Style::default().fg(colour(palette::VIOLET)),
                ),
            ]));
        }
        for effect in &step.effects {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<12}", "effect"),
                    Style::default().fg(colour(palette::ASH)),
                ),
                Span::styled(
                    format!("{} · ", effect.effect.label()),
                    Style::default().fg(colour(palette::INDIGO)),
                ),
                Span::styled(
                    effect.provenance.label().to_string(),
                    Style::default().fg(provenance_colour(effect.provenance.label())),
                ),
            ]));
        }
        // The address, and what holding it is worth. A viewer that shows a state root
        // without its grip invites the reader to assume `captured`, which for a page
        // or a reactor is the one thing it is not.
        lines.push(field(
            "state",
            &format!("{} · {}", step.state_root.short(), step.grip.label()),
        ));
        if let Action::Decide { options, .. } = &step.action {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                "  it could have chosen",
                Style::default().fg(colour(palette::ASH)),
            )));
            if let Some(items) = options.as_array() {
                for option in items {
                    let text = match option {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    lines.push(Line::from(Span::styled(
                        format!("    {text}"),
                        Style::default().fg(colour(palette::CYAN)),
                    )));
                }
            }
        }
    }
    let paragraph = Paragraph::new(lines)
        .block(pane("CHECKPOINT", false))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    match &app.note {
        Some(Note::Good(text)) => lines.push(Line::from(Span::styled(
            format!(" {text}"),
            Style::default().fg(colour(palette::PHOSPHOR)),
        ))),
        Some(Note::Bad(text)) => lines.push(Line::from(Span::styled(
            format!(" {text}"),
            Style::default().fg(colour(palette::AMBER)),
        ))),
        None => lines.push(Line::raw("")),
    }
    lines.push(Line::from(vec![
        key("e"),
        Span::styled(
            " explore from here   ",
            Style::default().fg(colour(palette::CHROME)),
        ),
        key("r"),
        Span::styled(" replay   ", Style::default().fg(colour(palette::ASH))),
        key("tab"),
        Span::styled(" pane   ", Style::default().fg(colour(palette::ASH))),
        key("↑↓"),
        Span::styled(" move   ", Style::default().fg(colour(palette::ASH))),
        key("q"),
        Span::styled(" quit", Style::default().fg(colour(palette::ASH))),
    ]));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colour(palette::INDIGO)));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_picker(f: &mut Frame, area: Rect, picker: &Picker) {
    let height = (picker.options.len() as u16 + 4).min(area.height);
    let popup = centred(56, height, area);
    f.render_widget(Clear, popup);
    let items: Vec<ListItem> = picker
        .options
        .iter()
        .map(|option| {
            ListItem::new(Line::from(Span::styled(
                format!("  {option}"),
                Style::default().fg(colour(palette::CYAN)),
            )))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(picker.selected));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colour(palette::VIOLET)))
                .title(Span::styled(
                    format!(" what if it had chosen — {} ", picker.decision),
                    Style::default()
                        .fg(colour(palette::VIOLET))
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .fg(colour(palette::CHROME))
                .add_modifier(Modifier::REVERSED),
        );
    f.render_stateful_widget(list, popup, &mut state);
}

fn draw_busy(f: &mut Frame, area: Rect, job: &str) {
    let popup = centred(54, 5, area);
    f.render_widget(Clear, popup);
    let what = if job == "replay" {
        "re-deriving the trajectory…"
    } else {
        "reconstructing the prefix, then diverging…"
    };
    let text = vec![
        Line::from(Span::styled(
            format!("  {}", crate::stand::MENACE),
            Style::default().fg(colour(palette::ASH)),
        )),
        Line::from(Span::styled(
            format!("  {what}"),
            Style::default()
                .fg(colour(palette::CHROME))
                .add_modifier(Modifier::BOLD),
        )),
    ];
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colour(palette::VIOLET))),
        ),
        popup,
    );
}

fn pane(title: &str, focused: bool) -> Block<'_> {
    let border = if focused {
        colour(palette::CHROME)
    } else {
        colour(palette::INDIGO)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(border),
        ))
}

/// Both spans own their text, so the line outlives the values it was built from.
fn field(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<12}"),
            Style::default().fg(colour(palette::ASH)),
        ),
        Span::styled(
            value.to_string(),
            Style::default().fg(colour(palette::CHROME)),
        ),
    ])
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(colour(palette::INDIGO))
            .add_modifier(Modifier::REVERSED | Modifier::BOLD),
    )
}

fn centred(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_keeps_the_line_inside_the_pane() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn centring_never_leaves_the_screen() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 6,
        };
        let popup = centred(80, 40, area);
        assert!(popup.width <= area.width && popup.height <= area.height);
    }

    #[test]
    fn every_provenance_reads_as_a_distinct_colour() {
        let colours: Vec<Color> = ["real", "live", "simulated", "unknown"]
            .iter()
            .map(|l| provenance_colour(l))
            .collect();
        for (i, a) in colours.iter().enumerate() {
            for b in colours.iter().skip(i + 1) {
                assert_ne!(a, b, "two provenances share a colour; the palette lies");
            }
        }
    }
}
