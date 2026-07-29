use std::{
    collections::HashMap,
    io::{self, Stdout},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind,
    },
    execute,
    style::force_color_output,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::{
    collector::{ApplicationSample, ProcessSnapshot},
    config::{ColorMode, Config},
    ipc::{self, ServerMessage},
    paths,
};

const TRUECOLOR_PALETTE: [Color; 8] = [
    Color::Rgb(34, 211, 238),
    Color::Rgb(251, 146, 60),
    Color::Rgb(74, 222, 128),
    Color::Rgb(248, 113, 113),
    Color::Rgb(250, 204, 21),
    Color::Rgb(96, 165, 250),
    Color::Rgb(244, 114, 182),
    Color::Rgb(163, 163, 163),
];

const ANSI256_PALETTE: [Color; 8] = [
    Color::Indexed(45),
    Color::Indexed(208),
    Color::Indexed(84),
    Color::Indexed(203),
    Color::Indexed(220),
    Color::Indexed(75),
    Color::Indexed(206),
    Color::Indexed(244),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricCategory {
    Cpu,
    Memory,
    Disk,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferDirection {
    Incoming,
    Outgoing,
}

impl MetricCategory {
    fn next(self) -> Self {
        match self {
            Self::Cpu => Self::Memory,
            Self::Memory => Self::Disk,
            Self::Disk => Self::Network,
            Self::Network => Self::Cpu,
        }
    }

    fn title(self, direction: TransferDirection) -> &'static str {
        match (self, direction) {
            (Self::Cpu, _) => "CPU",
            (Self::Memory, _) => "内存",
            (Self::Disk, TransferDirection::Incoming) => "磁盘读取",
            (Self::Disk, TransferDirection::Outgoing) => "磁盘写入",
            (Self::Network, TransferDirection::Incoming) => "网络下载",
            (Self::Network, TransferDirection::Outgoing) => "网络上传",
        }
    }

    fn value(self, application: &ApplicationSample, direction: TransferDirection) -> f64 {
        match (self, direction) {
            (Self::Cpu, _) => application.cpu_percent,
            (Self::Memory, _) => application.memory_bytes as f64,
            (Self::Disk, TransferDirection::Incoming) => application.disk_read_bytes_per_second,
            (Self::Disk, TransferDirection::Outgoing) => application.disk_write_bytes_per_second,
            (Self::Network, TransferDirection::Incoming) => {
                application.network_download_bytes_per_second
            }
            (Self::Network, TransferDirection::Outgoing) => {
                application.network_upload_bytes_per_second
            }
        }
    }

    fn format(self, value: f64) -> String {
        match self {
            Self::Cpu => format!("{value:.1}%"),
            Self::Memory => format_bytes(value),
            Self::Disk | Self::Network => format_rate(value),
        }
    }
}

struct App {
    config: Config,
    snapshots: Vec<ProcessSnapshot>,
    latest: Option<ProcessSnapshot>,
    metric: MetricCategory,
    direction: TransferDirection,
    hovered_index: Option<usize>,
    selected_application: usize,
    chart_area: Rect,
    applications_area: Rect,
    connected: bool,
    error: Option<String>,
}

impl App {
    fn new(config: Config) -> Self {
        Self {
            config,
            snapshots: Vec::new(),
            latest: None,
            metric: MetricCategory::Cpu,
            direction: TransferDirection::Incoming,
            hovered_index: None,
            selected_application: 0,
            chart_area: Rect::default(),
            applications_area: Rect::default(),
            connected: true,
            error: None,
        }
    }

    fn apply(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::State { history, latest } => {
                self.snapshots = history;
                self.latest = latest;
                self.clamp_selection();
                self.connected = true;
            }
            ServerMessage::Snapshot { snapshot } => {
                let mut compact = snapshot.clone();
                for application in &mut compact.applications {
                    application.processes.clear();
                }
                self.snapshots.push(compact);
                self.latest = Some(snapshot);
                self.trim_history();
                self.connected = true;
            }
            ServerMessage::Pong => self.connected = true,
            ServerMessage::Error { message } => self.error = Some(message),
        }
    }

    fn trim_history(&mut self) {
        let maximum = self.config.sampling.maximum_samples.max(2);
        if self.snapshots.len() > maximum {
            self.drain_oldest(self.snapshots.len() - maximum);
        }
        let Some(latest) = self.snapshots.last() else {
            return;
        };
        let retention_ms =
            i64::try_from(self.config.sampling.retention.as_millis()).unwrap_or(i64::MAX);
        let cutoff = latest.timestamp_unix_ms.saturating_sub(retention_ms);
        let keep_from = self
            .snapshots
            .iter()
            .position(|snapshot| snapshot.timestamp_unix_ms >= cutoff)
            .unwrap_or(self.snapshots.len());
        if keep_from > 0 {
            self.drain_oldest(keep_from);
        }
    }

    fn drain_oldest(&mut self, count: usize) {
        self.snapshots.drain(..count);
        if let Some(index) = self.hovered_index {
            self.hovered_index = Some(index.saturating_sub(count));
        }
        self.clamp_selection();
    }

    fn selected_snapshot_index(&self) -> Option<usize> {
        (!self.snapshots.is_empty()).then(|| {
            self.hovered_index
                .unwrap_or_else(|| self.snapshots.len().saturating_sub(1))
                .min(self.snapshots.len().saturating_sub(1))
        })
    }

    fn selected_snapshot(&self) -> Option<&ProcessSnapshot> {
        let selected = self
            .selected_snapshot_index()
            .and_then(|index| self.snapshots.get(index));
        match (selected, self.latest.as_ref()) {
            (Some(snapshot), Some(latest))
                if snapshot.timestamp_unix_ms == latest.timestamp_unix_ms =>
            {
                Some(latest)
            }
            (Some(snapshot), _) => Some(snapshot),
            (None, latest) => latest,
        }
    }

    fn selection_is_historical(&self) -> bool {
        let Some(selected) = self.selected_snapshot() else {
            return false;
        };
        self.latest
            .as_ref()
            .is_some_and(|latest| selected.timestamp_unix_ms != latest.timestamp_unix_ms)
    }

    fn select_snapshot(&mut self, index: usize) {
        if self.snapshots.is_empty() {
            return;
        }
        self.hovered_index = Some(index.min(self.snapshots.len().saturating_sub(1)));
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let maximum = self.sorted_applications().len().saturating_sub(1);
        self.selected_application = self.selected_application.min(maximum);
    }

    fn sorted_applications(&self) -> Vec<&ApplicationSample> {
        let mut applications: Vec<_> = self
            .selected_snapshot()
            .map_or_else(Vec::new, |snapshot| snapshot.applications.iter().collect());
        applications.sort_by(|left, right| {
            self.metric
                .value(right, self.direction)
                .total_cmp(&self.metric.value(left, self.direction))
        });
        applications
    }
}

pub async fn run(config: Config) -> Result<()> {
    force_color_output(config.display.color != ColorMode::None);
    let socket = paths::socket_path()?;
    let mut messages = ipc::subscribe(&socket).await?;
    let mut terminal = setup_terminal(config.display.mouse)?;
    let mut app = App::new(config);

    let result = async {
        loop {
            while let Ok(message) = messages.try_recv() {
                app.apply(message);
            }
            if messages.is_closed() {
                app.connected = false;
            }

            terminal.draw(|frame| render(frame, &mut app))?;

            if event::poll(Duration::from_millis(100)).context("无法读取终端事件")? {
                let terminal_event = event::read().context("无法读取终端事件")?;
                if handle_event(&mut app, terminal_event) {
                    break;
                }
            }
        }
        Ok(())
    }
    .await;

    restore_terminal(&mut terminal, app.config.display.mouse)?;
    result
}

fn setup_terminal(mouse: bool) -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().context("无法启用终端 raw mode")?;
    let mut stdout = io::stdout();
    if mouse {
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    } else {
        execute!(stdout, EnterAlternateScreen)?;
    }
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("无法初始化终端")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>, mouse: bool) -> Result<()> {
    disable_raw_mode().context("无法恢复终端模式")?;
    if mouse {
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
    } else {
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    }
    terminal.show_cursor().context("无法恢复终端光标")
}

fn handle_event(app: &mut App, terminal_event: Event) -> bool {
    match terminal_event {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Tab => {
                app.metric = app.metric.next();
                app.selected_application = 0;
            }
            KeyCode::Char('d') => {
                if matches!(app.metric, MetricCategory::Disk | MetricCategory::Network) {
                    app.direction = match app.direction {
                        TransferDirection::Incoming => TransferDirection::Outgoing,
                        TransferDirection::Outgoing => TransferDirection::Incoming,
                    };
                }
            }
            KeyCode::Left => move_hover(app, -1),
            KeyCode::Right => move_hover(app, 1),
            KeyCode::Up => {
                app.selected_application = app.selected_application.saturating_sub(1);
            }
            KeyCode::Down => {
                let maximum = app.sorted_applications().len().saturating_sub(1);
                app.selected_application = (app.selected_application + 1).min(maximum);
            }
            _ => {}
        },
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Down(_) | MouseEventKind::Drag(_) => {
                if app.chart_area.contains((mouse.column, mouse.row).into()) {
                    let width = usize::from(app.chart_area.width.max(1));
                    let relative = usize::from(mouse.column.saturating_sub(app.chart_area.x));
                    let count = app.snapshots.len();
                    if count > 0 {
                        app.select_snapshot(relative.saturating_mul(count) / width);
                    }
                } else if app
                    .applications_area
                    .contains((mouse.column, mouse.row).into())
                    && matches!(mouse.kind, MouseEventKind::Down(_))
                {
                    let row = mouse
                        .row
                        .saturating_sub(app.applications_area.y.saturating_add(1));
                    app.selected_application = usize::from(row);
                }
            }
            MouseEventKind::ScrollUp => {
                app.selected_application = app.selected_application.saturating_sub(1);
            }
            MouseEventKind::ScrollDown => {
                let maximum = app.sorted_applications().len().saturating_sub(1);
                app.selected_application = (app.selected_application + 1).min(maximum);
            }
            _ => {}
        },
        _ => {}
    }
    false
}

fn move_hover(app: &mut App, delta: isize) {
    if app.snapshots.is_empty() {
        return;
    }
    let current = app
        .hovered_index
        .unwrap_or_else(|| app.snapshots.len().saturating_sub(1));
    app.select_snapshot(
        current
            .saturating_add_signed(delta)
            .min(app.snapshots.len().saturating_sub(1)),
    );
}

fn render(frame: &mut Frame<'_>, app: &mut App) {
    let root = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(root);

    frame.render_widget(summary_widget(app), rows[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(rows[1]);

    app.chart_area = Rect {
        x: columns[0].x.saturating_add(1),
        y: columns[0].y.saturating_add(1),
        width: columns[0].width.saturating_sub(2),
        height: columns[0].height.saturating_sub(2),
    };
    app.applications_area = columns[1];

    frame.render_widget(
        StackedAreaChart {
            snapshots: &app.snapshots,
            metric: app.metric,
            direction: app.direction,
            top_count: app.config.display.top_applications,
            selected_index: app.selected_snapshot_index(),
            palette: palette(app.config.display.color),
        },
        columns[0],
    );
    render_application_panel(frame, app, columns[1]);
    frame.render_widget(footer_widget(app), rows[2]);
}

fn summary_widget(app: &App) -> Paragraph<'static> {
    let palette = palette(app.config.display.color);
    let (cpu, memory, disk, network) = app.latest.as_ref().map_or((0.0, 0, 0.0, 0.0), |snapshot| {
        let process_cpu: f64 = snapshot
            .applications
            .iter()
            .map(|item| item.cpu_percent)
            .sum();
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        let cpu = (process_cpu / cores as f64).clamp(0.0, 100.0);
        let memory = snapshot
            .applications
            .iter()
            .map(|item| item.memory_bytes)
            .sum();
        let disk = snapshot
            .applications
            .iter()
            .map(|item| item.disk_read_bytes_per_second + item.disk_write_bytes_per_second)
            .sum();
        let network = snapshot
            .applications
            .iter()
            .map(|item| {
                item.network_download_bytes_per_second + item.network_upload_bytes_per_second
            })
            .sum();
        (cpu, memory, disk, network)
    });

    Paragraph::new(Line::from(vec![
        Span::styled(" CPU ", Style::default().fg(palette[0]).bold()),
        Span::raw(format!("{cpu:5.1}%   ")),
        Span::styled("内存 ", Style::default().fg(palette[1]).bold()),
        Span::raw(format!("{:>8}   ", format_bytes(memory as f64))),
        Span::styled("磁盘 ", Style::default().fg(palette[2]).bold()),
        Span::raw(format!("{:>10}   ", format_rate(disk))),
        Span::styled("网络 ", Style::default().fg(palette[5]).bold()),
        Span::raw(format!("{:>10}", format_rate(network))),
    ]))
    .block(Block::default().borders(Borders::ALL).title("刻度"))
}

fn render_application_panel(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let palette = palette(app.config.display.color);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);
    let applications = app.sorted_applications();
    let visible_height = usize::from(sections[0].height.saturating_sub(2));
    let offset = app
        .selected_application
        .saturating_sub(visible_height.saturating_sub(1));
    let lines = applications
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_height)
        .map(|(index, application)| {
            let marker = if index == app.selected_application {
                "›"
            } else {
                " "
            };
            let style = if index == app.selected_application {
                Style::default().fg(Color::Black).bg(palette[0]).bold()
            } else {
                Style::default()
            };
            Line::styled(
                format!(
                    "{marker} {:<18} {:>9}",
                    truncate(&application.identity.name, 18),
                    app.metric
                        .format(app.metric.value(application, app.direction))
                ),
                style,
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(application_panel_title(app)),
            )
            .wrap(Wrap { trim: false }),
        sections[0],
    );

    let process_lines = if app.selection_is_historical() {
        Vec::new()
    } else {
        applications
            .get(app.selected_application)
            .map_or_else(Vec::new, |application| {
                let mut processes = application.processes.iter().collect::<Vec<_>>();
                processes.sort_by(|left, right| right.cpu_percent.total_cmp(&left.cpu_percent));
                processes
                    .into_iter()
                    .take(usize::from(sections[1].height.saturating_sub(2)))
                    .map(|process| {
                        Line::from(format!(
                            "{:>6} {:<13} {:>6.1}%",
                            process.pid,
                            truncate(&process.name, 13),
                            process.cpu_percent
                        ))
                    })
                    .collect()
            })
    };
    let process_title = if app.selection_is_historical() {
        "PID · 历史不保存"
    } else {
        "当前 PID"
    };
    frame.render_widget(
        Paragraph::new(process_lines)
            .block(Block::default().borders(Borders::ALL).title(process_title)),
        sections[1],
    );
}

fn application_panel_title(app: &App) -> String {
    let state = if app.selection_is_historical() {
        "历史"
    } else {
        "当前"
    };
    let timestamp = app
        .selected_snapshot()
        .map(|snapshot| format_timestamp(snapshot.timestamp_unix_ms))
        .unwrap_or_else(|| "--:--:--".to_owned());
    format!("应用 · {state} {timestamp}")
}

fn footer_widget(app: &App) -> Paragraph<'static> {
    let status = if let Some(error) = &app.error {
        format!("错误：{error}")
    } else if app.connected {
        "已连接".to_owned()
    } else {
        "服务连接已断开".to_owned()
    };
    let selection = app
        .selected_snapshot()
        .map_or_else(String::new, |snapshot| {
            let state = if app.selection_is_historical() {
                "历史"
            } else {
                "当前"
            };
            format!("  {state} {}", format_timestamp(snapshot.timestamp_unix_ms))
        });
    Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {status}{selection}  "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("Tab 指标  d 方向  ←→ 历史  ↑↓ 应用  q 退出"),
    ]))
}

struct StackedAreaChart<'a> {
    snapshots: &'a [ProcessSnapshot],
    metric: MetricCategory,
    direction: TransferDirection,
    top_count: usize,
    selected_index: Option<usize>,
    palette: [Color; 8],
}

impl Widget for StackedAreaChart<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let title = format!("{}历史", self.metric.title(self.direction));
        let block = Block::default().borders(Borders::ALL).title(title);
        let plot = block.inner(area);
        block.render(area, buffer);
        if plot.width == 0 || plot.height == 0 || self.snapshots.is_empty() {
            return;
        }

        let sampled = downsample(self.snapshots, usize::from(plot.width));
        let series = build_series(&sampled, self.metric, self.direction, self.top_count);
        let maximum = (0..sampled.len())
            .map(|index| series.iter().map(|item| item.values[index]).sum::<f64>())
            .fold(0.0, f64::max)
            .max(1.0)
            * 1.06;
        let sub_height = usize::from(plot.height) * 2;

        for (column, _) in sampled.iter().enumerate() {
            let mut pixels = vec![None; sub_height];
            let mut cumulative = 0.0;
            for (series_index, item) in series.iter().enumerate() {
                let lower = cumulative;
                cumulative += item.values[column];
                let start = ((lower / maximum) * sub_height as f64).floor() as usize;
                let end = ((cumulative / maximum) * sub_height as f64).ceil() as usize;
                for pixel in pixels.iter_mut().take(end.min(sub_height)).skip(start) {
                    *pixel = Some(self.palette[series_index.min(self.palette.len() - 1)]);
                }
            }

            let x = plot
                .x
                .saturating_add(u16::try_from(column).unwrap_or(u16::MAX));
            for row in 0..usize::from(plot.height) {
                let upper_index = sub_height.saturating_sub(1 + row * 2);
                let lower_index = upper_index.saturating_sub(1);
                let upper = pixels.get(upper_index).copied().flatten();
                let lower = pixels.get(lower_index).copied().flatten();
                let y = plot
                    .y
                    .saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
                let cell = &mut buffer[(x, y)];
                match (upper, lower) {
                    (None, None) => {
                        cell.set_symbol(" ");
                    }
                    (Some(color), None) => {
                        cell.set_symbol("▀").set_fg(color).set_bg(Color::Reset);
                    }
                    (None, Some(color)) => {
                        cell.set_symbol("▄").set_fg(color).set_bg(Color::Reset);
                    }
                    (Some(upper), Some(lower)) if upper == lower => {
                        cell.set_symbol("█").set_fg(upper).set_bg(Color::Reset);
                    }
                    (Some(upper), Some(lower)) => {
                        cell.set_symbol("▀").set_fg(upper).set_bg(lower);
                    }
                }
            }
        }

        if let Some(index) = self.selected_index {
            let x_offset = selected_column(index, self.snapshots.len(), usize::from(plot.width));
            let x = plot
                .x
                .saturating_add(u16::try_from(x_offset).unwrap_or(u16::MAX))
                .min(plot.right().saturating_sub(1));
            let cursor_style = Style::default()
                .fg(self.palette[4])
                .add_modifier(Modifier::BOLD);
            for y in plot.y..plot.bottom() {
                let cell = &mut buffer[(x, y)];
                if cell.symbol() == " " {
                    cell.set_symbol("┊").set_style(cursor_style);
                } else {
                    cell.set_style(Style::default().add_modifier(Modifier::BOLD));
                }
            }
            buffer[(x, area.y)].set_symbol("▼").set_style(cursor_style);
            buffer[(x, area.bottom().saturating_sub(1))]
                .set_symbol("▲")
                .set_style(cursor_style);
        }
    }
}

fn selected_column(index: usize, count: usize, width: usize) -> usize {
    if count == 0 || width == 0 {
        return 0;
    }
    index.min(count.saturating_sub(1)).saturating_mul(width) / count
}

struct Series {
    values: Vec<f64>,
}

fn build_series(
    snapshots: &[&ProcessSnapshot],
    metric: MetricCategory,
    direction: TransferDirection,
    top_count: usize,
) -> Vec<Series> {
    let mut totals = HashMap::<String, (String, f64)>::new();
    for snapshot in snapshots {
        for application in &snapshot.applications {
            let entry = totals
                .entry(application.identity.id.clone())
                .or_insert_with(|| (application.identity.name.clone(), 0.0));
            entry.1 += metric.value(application, direction);
        }
    }
    let mut identities = totals.into_iter().collect::<Vec<_>>();
    identities.sort_by(|left, right| right.1.1.total_cmp(&left.1.1));
    let top_ids = identities
        .into_iter()
        .take(top_count)
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let index_by_id = top_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut values = vec![vec![0.0; snapshots.len()]; top_ids.len()];
    let mut other = vec![0.0; snapshots.len()];
    for (sample_index, snapshot) in snapshots.iter().enumerate() {
        for application in &snapshot.applications {
            let value = metric.value(application, direction);
            if let Some(series_index) = index_by_id.get(application.identity.id.as_str()) {
                values[*series_index][sample_index] += value;
            } else {
                other[sample_index] += value;
            }
        }
    }
    let mut output = values
        .into_iter()
        .map(|values| Series { values })
        .collect::<Vec<_>>();
    if other.iter().any(|value| *value > 0.0) {
        output.push(Series { values: other });
    }
    output
}

fn downsample(snapshots: &[ProcessSnapshot], maximum: usize) -> Vec<&ProcessSnapshot> {
    if snapshots.len() <= maximum || maximum < 2 {
        return snapshots.iter().collect();
    }
    (0..maximum)
        .map(|index| {
            let source = ((index + 1) * snapshots.len() / maximum).saturating_sub(1);
            &snapshots[source]
        })
        .collect()
}

fn format_bytes(value: f64) -> String {
    const GIB: f64 = 1_073_741_824.0;
    const MIB: f64 = 1_048_576.0;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else {
        format!("{:.1} MiB", value / MIB)
    }
}

fn format_rate(value: f64) -> String {
    const MIB: f64 = 1_048_576.0;
    const KIB: f64 = 1024.0;
    if value >= MIB {
        format!("{:.1} MiB/s", value / MIB)
    } else {
        format!("{:.1} KiB/s", value / KIB)
    }
}

fn format_timestamp(timestamp_unix_ms: i64) -> String {
    DateTime::from_timestamp_millis(timestamp_unix_ms)
        .map(|time| time.with_timezone(&Local).format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".to_owned())
}

fn truncate(value: &str, maximum: usize) -> String {
    let mut output = value.chars().take(maximum).collect::<String>();
    if value.chars().count() > maximum && maximum > 0 {
        output.pop();
        output.push('…');
    }
    output
}

fn palette(mode: ColorMode) -> [Color; 8] {
    match mode {
        ColorMode::Ansi256 => ANSI256_PALETTE,
        ColorMode::None => [Color::Reset; 8],
        ColorMode::Auto | ColorMode::Truecolor => TRUECOLOR_PALETTE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{ApplicationIdentity, ApplicationSample};
    use crossterm::event::{KeyModifiers, MouseEvent};
    use ratatui::backend::TestBackend;

    fn sample(timestamp: i64, cpu: f64) -> ProcessSnapshot {
        ProcessSnapshot {
            timestamp_unix_ms: timestamp,
            applications: vec![ApplicationSample {
                identity: ApplicationIdentity {
                    id: "app:test".into(),
                    name: "Test".into(),
                    bundle_path: None,
                },
                processes: vec![],
                cpu_percent: cpu,
                memory_bytes: 0,
                disk_read_bytes_per_second: 0.0,
                disk_write_bytes_per_second: 0.0,
                network_download_bytes_per_second: 0.0,
                network_upload_bytes_per_second: 0.0,
            }],
        }
    }

    #[test]
    fn dense_history_renders_with_test_backend() {
        let snapshots = (0..360)
            .map(|index| sample(index * 5_000, (index % 100) as f64))
            .collect::<Vec<_>>();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    StackedAreaChart {
                        snapshots: &snapshots,
                        metric: MetricCategory::Cpu,
                        direction: TransferDirection::Incoming,
                        top_count: 7,
                        selected_index: Some(200),
                        palette: TRUECOLOR_PALETTE,
                    },
                    frame.area(),
                );
            })
            .unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| { matches!(cell.symbol(), "█" | "▀" | "▄") })
        );
    }

    #[test]
    fn downsampling_keeps_requested_width() {
        let snapshots = (0..100).map(|index| sample(index, 1.0)).collect::<Vec<_>>();
        assert_eq!(downsample(&snapshots, 20).len(), 20);
    }

    #[test]
    fn selected_history_drives_application_panel() {
        let mut historical = sample(1_000, 42.0);
        historical.applications[0].identity.name = "Historical".into();
        let mut latest = sample(2_000, 99.0);
        latest.applications[0].identity.name = "Latest".into();

        let mut app = App::new(Config::default());
        app.snapshots = vec![historical, latest.clone()];
        app.latest = Some(latest);
        app.select_snapshot(0);

        assert_eq!(app.sorted_applications()[0].identity.name, "Historical");
        assert!(app.selection_is_historical());
        assert!(application_panel_title(&app).starts_with("应用 · 历史 "));
    }

    #[test]
    fn selected_column_reaches_both_chart_edges() {
        assert_eq!(selected_column(0, 100, 20), 0);
        assert_eq!(selected_column(99, 100, 20), 19);
    }

    #[test]
    fn selected_history_renders_a_visible_chart_cursor() {
        let snapshots = (0..10).map(|index| sample(index, 1.0)).collect::<Vec<_>>();
        let backend = TestBackend::new(20, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    StackedAreaChart {
                        snapshots: &snapshots,
                        metric: MetricCategory::Cpu,
                        direction: TransferDirection::Incoming,
                        top_count: 7,
                        selected_index: Some(5),
                        palette: TRUECOLOR_PALETTE,
                    },
                    frame.area(),
                );
            })
            .unwrap();

        let plot_width = 18;
        let cursor_x = 1 + selected_column(5, snapshots.len(), plot_width);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(cursor_x as u16, 0)].symbol(), "▼");
        assert!(matches!(
            buffer[(cursor_x as u16, 2)].symbol(),
            "█" | "▀" | "▄"
        ));
        assert_eq!(buffer[(cursor_x as u16, 7)].symbol(), "▲");
    }

    #[test]
    fn mouse_motion_selects_a_history_sample() {
        let mut app = App::new(Config::default());
        app.snapshots = (0..10).map(|index| sample(index, 1.0)).collect();
        app.chart_area = Rect::new(5, 4, 10, 8);

        handle_event(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                column: 10,
                row: 6,
                modifiers: KeyModifiers::NONE,
            }),
        );

        assert_eq!(app.hovered_index, Some(5));
    }

    #[test]
    fn ansi256_mode_uses_indexed_colors() {
        assert!(
            palette(ColorMode::Ansi256)
                .iter()
                .all(|color| matches!(color, Color::Indexed(_)))
        );
    }
}
