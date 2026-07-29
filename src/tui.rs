use std::{
    collections::{HashMap, HashSet},
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

const AXIS_SHRINK_RATIO: f64 = 0.65;
const AXIS_SHRINK_SAMPLES: u8 = 6;
const NICE_AXIS_STEPS: [f64; 10] = [1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0];

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
    chart_source_indices: Vec<Option<usize>>,
    chart_view_end: Option<usize>,
    chart_series_slots: Vec<Option<String>>,
    chart_axis_maximum: Option<f64>,
    chart_axis_shrink_samples: u8,
    chart_axis_last_timestamp: Option<i64>,
    chart_analysis_dirty: bool,
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
            chart_source_indices: Vec::new(),
            chart_view_end: None,
            chart_series_slots: Vec::new(),
            chart_axis_maximum: None,
            chart_axis_shrink_samples: 0,
            chart_axis_last_timestamp: None,
            chart_analysis_dirty: true,
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
                self.hovered_index = None;
                self.chart_view_end = None;
                self.clamp_selection();
                self.chart_analysis_dirty = true;
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
                self.chart_analysis_dirty = true;
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
        if let Some(end) = self.chart_view_end {
            self.chart_view_end = if self.snapshots.is_empty() {
                None
            } else {
                Some(end.saturating_sub(count).clamp(1, self.snapshots.len()))
            };
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

    fn select_chart_column(&mut self, column: usize) {
        if let Some(index) = self.chart_source_indices.get(column).copied().flatten() {
            if index == self.snapshots.len().saturating_sub(1) {
                self.hovered_index = None;
                self.chart_view_end = None;
                self.clamp_selection();
            } else {
                self.chart_view_end.get_or_insert(self.snapshots.len());
                self.select_snapshot(index);
            }
        }
    }

    fn ensure_selected_snapshot_is_visible(&mut self, width: usize) {
        let Some(selected) = self.hovered_index else {
            return;
        };
        if self.snapshots.is_empty() {
            return;
        }
        let width = width.max(1);
        let end = self
            .chart_view_end
            .unwrap_or(self.snapshots.len())
            .clamp(1, self.snapshots.len());
        let start = end.saturating_sub(width);
        if selected < start {
            self.chart_view_end = Some(selected.saturating_add(width).min(self.snapshots.len()));
        } else if selected >= end {
            self.chart_view_end = Some(selected.saturating_add(1).min(self.snapshots.len()));
        }
    }

    fn reset_chart_state(&mut self) {
        self.chart_series_slots.clear();
        self.chart_axis_maximum = None;
        self.chart_axis_shrink_samples = 0;
        self.chart_axis_last_timestamp = None;
        self.chart_analysis_dirty = true;
    }

    fn update_series_slots(&mut self, top_ids: &[String]) {
        let slot_count = self.config.display.top_applications;
        self.chart_series_slots.resize(slot_count, None);
        self.chart_series_slots.truncate(slot_count);

        let desired = top_ids.iter().map(String::as_str).collect::<HashSet<_>>();
        for slot in &mut self.chart_series_slots {
            if slot.as_deref().is_some_and(|id| !desired.contains(id)) {
                *slot = None;
            }
        }

        for id in top_ids {
            if self
                .chart_series_slots
                .iter()
                .any(|slot| slot.as_deref() == Some(id.as_str()))
            {
                continue;
            }
            if let Some(slot) = self
                .chart_series_slots
                .iter_mut()
                .find(|slot| slot.is_none())
            {
                *slot = Some(id.clone());
            }
        }
    }

    fn update_axis_maximum(&mut self, peak: f64) -> f64 {
        let candidate = nice_upper_bound((peak * 1.05).max(1.0));
        let latest_timestamp = self
            .snapshots
            .last()
            .map(|snapshot| snapshot.timestamp_unix_ms);
        let is_new_sample = latest_timestamp != self.chart_axis_last_timestamp;
        self.chart_axis_last_timestamp = latest_timestamp;

        let Some(current) = self.chart_axis_maximum else {
            self.chart_axis_maximum = Some(candidate);
            return candidate;
        };

        if candidate > current {
            self.chart_axis_maximum = Some(candidate);
            self.chart_axis_shrink_samples = 0;
        } else if candidate < current && peak <= current * AXIS_SHRINK_RATIO {
            if is_new_sample {
                self.chart_axis_shrink_samples = self.chart_axis_shrink_samples.saturating_add(1);
            }
            if self.chart_axis_shrink_samples >= AXIS_SHRINK_SAMPLES {
                self.chart_axis_maximum = Some(previous_nice_step(current).max(candidate));
                self.chart_axis_shrink_samples = 0;
            }
        } else {
            self.chart_axis_shrink_samples = 0;
        }

        self.chart_axis_maximum.unwrap_or(candidate)
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
                app.reset_chart_state();
            }
            KeyCode::Char('d') => {
                if matches!(app.metric, MetricCategory::Disk | MetricCategory::Network) {
                    app.direction = match app.direction {
                        TransferDirection::Incoming => TransferDirection::Outgoing,
                        TransferDirection::Outgoing => TransferDirection::Incoming,
                    };
                    app.reset_chart_state();
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
                    let relative = usize::from(mouse.column.saturating_sub(app.chart_area.x));
                    app.select_chart_column(relative);
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
    let latest = app.snapshots.len().saturating_sub(1);
    let current = app.hovered_index.unwrap_or(latest);
    let target = current.saturating_add_signed(delta).min(latest);
    if target == current {
        return;
    }
    if target == latest && delta.is_positive() {
        app.hovered_index = None;
        app.chart_view_end = None;
        app.clamp_selection();
        return;
    }

    app.chart_view_end.get_or_insert(app.snapshots.len());
    let visible_width = app.chart_source_indices.len().max(1);
    let visible_first = app.chart_source_indices.iter().flatten().copied().min();
    let visible_last = app.chart_source_indices.iter().flatten().copied().max();
    if visible_first.is_some_and(|first| target < first) {
        app.chart_view_end = Some(
            target
                .saturating_add(visible_width)
                .min(app.snapshots.len()),
        );
    } else if visible_last.is_some_and(|last| target > last) {
        app.chart_view_end = Some(target.saturating_add(1).min(app.snapshots.len()));
    }
    app.select_snapshot(target);
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
    app.ensure_selected_snapshot_is_visible(usize::from(app.chart_area.width));

    if app.chart_analysis_dirty {
        let top_ids = top_application_ids(
            &app.snapshots,
            app.metric,
            app.direction,
            app.config.display.top_applications,
        );
        app.update_series_slots(&top_ids);
        let peak = history_peak(&app.snapshots, app.metric, app.direction);
        app.update_axis_maximum(peak);
        app.chart_analysis_dirty = false;
    }
    let maximum = app.chart_axis_maximum.unwrap_or(1.0);
    let selected_index = app.selected_snapshot_index();
    let window = ChartWindow::new(
        &app.snapshots,
        usize::from(app.chart_area.width),
        app.chart_view_end,
    );
    app.chart_source_indices = window
        .columns
        .iter()
        .map(|column| column.source_index)
        .collect();

    frame.render_widget(
        StackedAreaChart {
            window,
            metric: app.metric,
            direction: app.direction,
            series_slots: app.chart_series_slots.clone(),
            selected_index,
            maximum,
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
    window: ChartWindow<'a>,
    metric: MetricCategory,
    direction: TransferDirection,
    series_slots: Vec<Option<String>>,
    selected_index: Option<usize>,
    maximum: f64,
    palette: [Color; 8],
}

impl Widget for StackedAreaChart<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let title = format!("{}历史", self.metric.title(self.direction));
        let block = Block::default().borders(Borders::ALL).title(title);
        let plot = block.inner(area);
        block.render(area, buffer);
        if plot.width == 0 || plot.height == 0 || self.window.columns.is_empty() {
            return;
        }

        let series = build_series(
            &self.window.columns,
            self.metric,
            self.direction,
            &self.series_slots,
            self.palette.len(),
        );
        let sub_height = usize::from(plot.height) * 2;

        for column in 0..self.window.columns.len() {
            let mut pixels = vec![None; sub_height];
            let mut cumulative = 0.0;
            for item in &series {
                let lower = cumulative;
                cumulative += item.values[column];
                let start = ((lower / self.maximum) * sub_height as f64).floor() as usize;
                let end = ((cumulative / self.maximum) * sub_height as f64).ceil() as usize;
                for pixel in pixels.iter_mut().take(end.min(sub_height)).skip(start) {
                    *pixel = Some(self.palette[item.palette_index]);
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

        if let Some(x_offset) = self
            .selected_index
            .and_then(|index| self.window.column_for_source_index(index))
        {
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

#[derive(Clone, Copy, Default)]
struct ChartColumn<'a> {
    snapshot: Option<&'a ProcessSnapshot>,
    source_index: Option<usize>,
}

struct ChartWindow<'a> {
    columns: Vec<ChartColumn<'a>>,
}

impl<'a> ChartWindow<'a> {
    fn new(snapshots: &'a [ProcessSnapshot], width: usize, view_end: Option<usize>) -> Self {
        if width == 0 || snapshots.is_empty() {
            return Self {
                columns: Vec::new(),
            };
        }

        let end = view_end
            .unwrap_or(snapshots.len())
            .clamp(1, snapshots.len());
        let start = end.saturating_sub(width);
        let visible_count = end.saturating_sub(start);
        let leading_empty = width.saturating_sub(visible_count);
        let mut columns = vec![ChartColumn::default(); width];
        for (offset, source_index) in (start..end).enumerate() {
            columns[leading_empty + offset] = ChartColumn {
                snapshot: snapshots.get(source_index),
                source_index: Some(source_index),
            };
        }

        Self { columns }
    }

    fn column_for_source_index(&self, source_index: usize) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| column.source_index == Some(source_index))
    }
}

struct Series {
    values: Vec<f64>,
    palette_index: usize,
}

fn build_series(
    columns: &[ChartColumn<'_>],
    metric: MetricCategory,
    direction: TransferDirection,
    slots: &[Option<String>],
    palette_len: usize,
) -> Vec<Series> {
    let index_by_id = slots
        .iter()
        .enumerate()
        .filter_map(|(index, id)| id.as_deref().map(|id| (id, index)))
        .collect::<HashMap<_, _>>();
    let mut values = vec![vec![0.0; columns.len()]; slots.len()];
    let mut other = vec![0.0; columns.len()];
    for (column_index, column) in columns.iter().enumerate() {
        let Some(snapshot) = column.snapshot else {
            continue;
        };
        for application in &snapshot.applications {
            let value = metric.value(application, direction);
            if let Some(series_index) = index_by_id.get(application.identity.id.as_str()) {
                values[*series_index][column_index] += value;
            } else {
                other[column_index] += value;
            }
        }
    }
    let mut output = values
        .into_iter()
        .enumerate()
        .map(|(palette_index, values)| Series {
            values,
            palette_index: palette_index.min(palette_len.saturating_sub(1)),
        })
        .collect::<Vec<_>>();
    if other.iter().any(|value| *value > 0.0) {
        output.push(Series {
            values: other,
            palette_index: palette_len.saturating_sub(1),
        });
    }
    output
}

fn top_application_ids(
    snapshots: &[ProcessSnapshot],
    metric: MetricCategory,
    direction: TransferDirection,
    top_count: usize,
) -> Vec<String> {
    let mut totals = HashMap::<String, f64>::new();
    for snapshot in snapshots {
        for application in &snapshot.applications {
            *totals.entry(application.identity.id.clone()).or_default() +=
                metric.value(application, direction);
        }
    }
    let mut identities = totals.into_iter().collect::<Vec<_>>();
    identities.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    identities
        .into_iter()
        .take(top_count)
        .map(|(id, _)| id)
        .collect()
}

fn history_peak(
    snapshots: &[ProcessSnapshot],
    metric: MetricCategory,
    direction: TransferDirection,
) -> f64 {
    snapshots
        .iter()
        .map(|snapshot| {
            snapshot
                .applications
                .iter()
                .map(|application| metric.value(application, direction))
                .sum::<f64>()
        })
        .fold(0.0, f64::max)
}

fn nice_upper_bound(value: f64) -> f64 {
    if !value.is_finite() || value <= 1.0 {
        return 1.0;
    }
    let magnitude = 10_f64.powf(value.log10().floor());
    let normalized = value / magnitude;
    let step = NICE_AXIS_STEPS
        .iter()
        .copied()
        .find(|step| normalized <= *step)
        .unwrap_or(10.0);
    step * magnitude
}

fn previous_nice_step(value: f64) -> f64 {
    if !value.is_finite() || value <= 1.0 {
        return 1.0;
    }
    let magnitude = 10_f64.powf(value.log10().floor());
    let normalized = value / magnitude;
    let last_step = NICE_AXIS_STEPS[NICE_AXIS_STEPS.len() - 1];
    if normalized > last_step + f64::EPSILON {
        return last_step * magnitude;
    }
    let current_index = NICE_AXIS_STEPS
        .iter()
        .position(|step| normalized <= *step + f64::EPSILON)
        .unwrap_or(0);
    if current_index == 0 {
        last_step * magnitude / 10.0
    } else {
        NICE_AXIS_STEPS[current_index - 1] * magnitude
    }
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
        sample_with_applications(timestamp, &[("app:test", "Test", cpu)])
    }

    fn sample_with_applications(
        timestamp: i64,
        applications: &[(&str, &str, f64)],
    ) -> ProcessSnapshot {
        ProcessSnapshot {
            timestamp_unix_ms: timestamp,
            applications: applications
                .iter()
                .map(|(id, name, cpu)| ApplicationSample {
                    identity: ApplicationIdentity {
                        id: (*id).into(),
                        name: (*name).into(),
                        bundle_path: None,
                    },
                    processes: vec![],
                    cpu_percent: *cpu,
                    memory_bytes: 0,
                    disk_read_bytes_per_second: 0.0,
                    disk_write_bytes_per_second: 0.0,
                    network_download_bytes_per_second: 0.0,
                    network_upload_bytes_per_second: 0.0,
                })
                .collect(),
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
                        window: ChartWindow::new(&snapshots, 78, None),
                        metric: MetricCategory::Cpu,
                        direction: TransferDirection::Incoming,
                        series_slots: vec![Some("app:test".into())],
                        selected_index: Some(359),
                        maximum: 100.0,
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
    fn rolling_window_right_aligns_short_history() {
        let snapshots = (0..3).map(|index| sample(index, 1.0)).collect::<Vec<_>>();
        let window = ChartWindow::new(&snapshots, 5, None);

        assert_eq!(
            window
                .columns
                .iter()
                .map(|column| column.source_index)
                .collect::<Vec<_>>(),
            vec![None, None, Some(0), Some(1), Some(2)]
        );
    }

    #[test]
    fn live_append_shifts_the_window_left_one_column() {
        let mut snapshots = (0..10)
            .map(|index| sample(index, index as f64))
            .collect::<Vec<_>>();
        let before = ChartWindow::new(&snapshots, 4, None)
            .columns
            .iter()
            .map(|column| column.source_index)
            .collect::<Vec<_>>();

        snapshots.push(sample(10, 10.0));
        let after = ChartWindow::new(&snapshots, 4, None)
            .columns
            .iter()
            .map(|column| column.source_index)
            .collect::<Vec<_>>();

        assert_eq!(before, vec![Some(6), Some(7), Some(8), Some(9)]);
        assert_eq!(after, vec![Some(7), Some(8), Some(9), Some(10)]);
    }

    #[test]
    fn historical_view_does_not_move_when_live_data_arrives() {
        let mut snapshots = (0..10)
            .map(|index| sample(index, index as f64))
            .collect::<Vec<_>>();
        let before = ChartWindow::new(&snapshots, 4, Some(8))
            .columns
            .iter()
            .map(|column| column.source_index)
            .collect::<Vec<_>>();

        snapshots.push(sample(10, 10.0));
        let after = ChartWindow::new(&snapshots, 4, Some(8))
            .columns
            .iter()
            .map(|column| column.source_index)
            .collect::<Vec<_>>();

        assert_eq!(before, vec![Some(4), Some(5), Some(6), Some(7)]);
        assert_eq!(after, before);
    }

    #[test]
    fn live_refresh_scrolls_the_rendered_pixels_left() {
        let mut snapshots = (0..20)
            .map(|index| sample(index * 5_000, (index % 100) as f64))
            .collect::<Vec<_>>();
        let render = |snapshots: &[ProcessSnapshot]| {
            let backend = TestBackend::new(22, 10);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        StackedAreaChart {
                            window: ChartWindow::new(snapshots, 20, None),
                            metric: MetricCategory::Cpu,
                            direction: TransferDirection::Incoming,
                            series_slots: vec![Some("app:test".into())],
                            selected_index: None,
                            maximum: 100.0,
                            palette: TRUECOLOR_PALETTE,
                        },
                        frame.area(),
                    );
                })
                .unwrap();
            terminal.backend().buffer().clone()
        };

        let before = render(&snapshots);
        snapshots.push(sample(100_000, 99.0));
        let after = render(&snapshots);
        for y in 1..9 {
            for x in 1..20 {
                assert_eq!(after[(x, y)], before[(x + 1, y)]);
            }
        }
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
    fn selected_source_index_maps_to_its_visible_column() {
        let snapshots = (0..10).map(|index| sample(index, 1.0)).collect::<Vec<_>>();
        let window = ChartWindow::new(&snapshots, 5, None);

        assert_eq!(window.column_for_source_index(5), Some(0));
        assert_eq!(window.column_for_source_index(9), Some(4));
        assert_eq!(window.column_for_source_index(4), None);
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
                        window: ChartWindow::new(&snapshots, 18, None),
                        metric: MetricCategory::Cpu,
                        direction: TransferDirection::Incoming,
                        series_slots: vec![Some("app:test".into())],
                        selected_index: Some(5),
                        maximum: 1.0,
                        palette: TRUECOLOR_PALETTE,
                    },
                    frame.area(),
                );
            })
            .unwrap();

        let window = ChartWindow::new(&snapshots, 18, None);
        let cursor_x = 1 + window.column_for_source_index(5).unwrap();
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
        app.chart_source_indices = vec![None, None, Some(0), Some(1), Some(2), Some(3)];

        handle_event(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                column: 5,
                row: 6,
                modifiers: KeyModifiers::NONE,
            }),
        );
        assert_eq!(app.hovered_index, None);

        handle_event(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                column: 10,
                row: 6,
                modifiers: KeyModifiers::NONE,
            }),
        );

        assert_eq!(app.hovered_index, Some(3));
        assert_eq!(app.chart_view_end, Some(10));
    }

    #[test]
    fn keyboard_history_pans_at_edges_and_returns_to_live_mode() {
        let mut app = App::new(Config::default());
        app.snapshots = (0..15).map(|index| sample(index, 1.0)).collect();
        app.chart_source_indices = (5..15).map(Some).collect();

        move_hover(&mut app, -1);
        assert_eq!(app.hovered_index, Some(13));
        assert_eq!(app.chart_view_end, Some(15));

        app.hovered_index = Some(5);
        move_hover(&mut app, -1);
        assert_eq!(app.hovered_index, Some(4));
        assert_eq!(app.chart_view_end, Some(14));

        app.hovered_index = Some(13);
        app.chart_view_end = Some(14);
        move_hover(&mut app, 1);
        assert_eq!(app.hovered_index, None);
        assert_eq!(app.chart_view_end, None);
    }

    #[test]
    fn resize_keeps_the_selected_history_visible() {
        let mut app = App::new(Config::default());
        app.snapshots = (0..30).map(|index| sample(index, 1.0)).collect();
        app.hovered_index = Some(20);
        app.chart_view_end = Some(30);

        app.ensure_selected_snapshot_is_visible(5);
        let window = ChartWindow::new(&app.snapshots, 5, app.chart_view_end);

        assert_eq!(app.chart_view_end, Some(25));
        assert_eq!(window.column_for_source_index(20), Some(0));
    }

    #[test]
    fn trimming_history_preserves_the_selected_timestamp() {
        let mut app = App::new(Config::default());
        app.snapshots = (0..30).map(|index| sample(index, 1.0)).collect();
        app.hovered_index = Some(10);
        app.chart_view_end = Some(20);

        app.drain_oldest(5);

        assert_eq!(app.hovered_index, Some(5));
        assert_eq!(app.chart_view_end, Some(15));
        assert_eq!(app.selected_snapshot().unwrap().timestamp_unix_ms, 10);
    }

    #[test]
    fn series_slots_survive_ranking_changes() {
        let mut app = App::new(Config::default());
        app.config.display.top_applications = 2;
        app.update_series_slots(&["app:a".into(), "app:b".into()]);
        let original = app.chart_series_slots.clone();

        app.update_series_slots(&["app:b".into(), "app:a".into()]);
        assert_eq!(app.chart_series_slots, original);

        let b_slot = app
            .chart_series_slots
            .iter()
            .position(|slot| slot.as_deref() == Some("app:b"))
            .unwrap();
        app.update_series_slots(&["app:b".into(), "app:c".into()]);
        assert_eq!(app.chart_series_slots[b_slot].as_deref(), Some("app:b"));
    }

    #[test]
    fn equal_application_totals_use_identity_as_tiebreaker() {
        let snapshots = vec![sample_with_applications(
            1_000,
            &[("app:b", "B", 1.0), ("app:a", "A", 1.0)],
        )];

        assert_eq!(
            top_application_ids(
                &snapshots,
                MetricCategory::Cpu,
                TransferDirection::Incoming,
                2,
            ),
            vec!["app:a", "app:b"]
        );
    }

    #[test]
    fn series_preserve_the_total_value_of_each_column() {
        let snapshots = vec![sample_with_applications(
            1_000,
            &[("app:a", "A", 2.0), ("app:b", "B", 3.0)],
        )];
        let window = ChartWindow::new(&snapshots, 1, None);
        let series = build_series(
            &window.columns,
            MetricCategory::Cpu,
            TransferDirection::Incoming,
            &[Some("app:a".into())],
            TRUECOLOR_PALETTE.len(),
        );

        assert_eq!(series.iter().map(|item| item.values[0]).sum::<f64>(), 5.0);
    }

    #[test]
    fn axis_grows_immediately_and_shrinks_after_stable_samples() {
        let mut app = App::new(Config::default());
        app.snapshots = vec![sample(1_000, 9.0)];
        assert_eq!(app.update_axis_maximum(9.0), 10.0);

        app.snapshots.push(sample(2_000, 11.0));
        assert_eq!(app.update_axis_maximum(11.0), 12.5);

        for timestamp in 3..(3 + AXIS_SHRINK_SAMPLES) {
            app.snapshots
                .push(sample(i64::from(timestamp) * 1_000, 4.0));
            let maximum = app.update_axis_maximum(4.0);
            if timestamp < 2 + AXIS_SHRINK_SAMPLES {
                assert_eq!(maximum, 12.5);
            }
        }
        assert_eq!(app.chart_axis_maximum, Some(10.0));
    }

    #[test]
    fn axis_threshold_growth_does_not_double_the_chart_scale() {
        let mut app = App::new(Config::default());
        app.snapshots = vec![sample(1_000, 95.0)];
        assert_eq!(app.update_axis_maximum(95.0), 100.0);

        app.snapshots.push(sample(2_000, 96.0));
        assert_eq!(app.update_axis_maximum(96.0), 125.0);
    }

    #[test]
    fn nice_axis_uses_fine_grained_steps() {
        assert_eq!(nice_upper_bound(1.1), 1.25);
        assert_eq!(nice_upper_bound(2.1), 2.5);
        assert_eq!(nice_upper_bound(5.1), 6.0);
        assert_eq!(nice_upper_bound(51.0), 60.0);
        assert_eq!(previous_nice_step(100.0), 80.0);
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
