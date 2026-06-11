use crate::llm_adapter::LlmAdapter;
use crate::persistence::Persistence;
use crate::sim_core::{
    AgentView, DEFAULT_TICKS_PER_SECOND, MAX_TICKS_PER_SECOND, MapRender, Simulation,
    tick_interval_ms,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::io::stdout;
use std::time::{Duration, Instant};

pub fn run_tui(
    mut sim: Simulation,
    llm: Box<dyn LlmAdapter>,
    mut persistence: Persistence,
) -> Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let terminal = ratatui::init();
    let result = run_app(terminal, &mut sim, llm.as_ref(), &mut persistence);
    ratatui::restore();
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    persistence.save(&mut sim, "shutdown")?;
    result
}

fn run_app(
    mut terminal: DefaultTerminal,
    sim: &mut Simulation,
    llm: &dyn LlmAdapter,
    persistence: &mut Persistence,
) -> Result<()> {
    let mut app = AppState::new(llm.provider_name().to_string());
    let mut last_tick = Instant::now();
    let mut last_saved_day = sim.current_day();

    loop {
        let views = filtered_views(sim.agent_views(), app.role_filter.clone());
        if !views.is_empty() {
            app.selected_agent = app.selected_agent.min(views.len().saturating_sub(1));
        } else {
            app.selected_agent = 0;
        }
        let selected_id = views.get(app.selected_agent).map(|view| view.id);
        let map = sim.render_ascii_map(selected_id, 44, 22);
        let events = sim.recent_events(16);

        terminal.draw(|frame| render(frame, sim, &app, &views, &map, &events))?;

        if !app.paused && last_tick.elapsed() >= Duration::from_millis(app.tick_rate_ms) {
            sim.tick(llm)?;
            last_tick = Instant::now();
            if sim.current_day() != last_saved_day {
                persistence.save(sim, "daily")?;
                last_saved_day = sim.current_day();
            }
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char(' ') => app.paused = !app.paused,
                    KeyCode::Char('n') => {
                        sim.tick(llm)?;
                    }
                    KeyCode::Char('+') => {
                        app.ticks_per_second = (app.ticks_per_second + 1).min(MAX_TICKS_PER_SECOND);
                        app.tick_rate_ms = tick_interval_ms(app.ticks_per_second);
                    }
                    KeyCode::Char('-') => {
                        app.ticks_per_second = app.ticks_per_second.saturating_sub(1).max(1);
                        app.tick_rate_ms = tick_interval_ms(app.ticks_per_second);
                    }
                    KeyCode::Down => {
                        if !views.is_empty() {
                            app.selected_agent = (app.selected_agent + 1) % views.len();
                        }
                    }
                    KeyCode::Up => {
                        if !views.is_empty() {
                            app.selected_agent = app
                                .selected_agent
                                .checked_sub(1)
                                .unwrap_or(views.len().saturating_sub(1));
                        }
                    }
                    KeyCode::Char('f') => {
                        app.role_filter = next_filter(app.role_filter.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn render(
    frame: &mut Frame<'_>,
    sim: &Simulation,
    app: &AppState,
    views: &[AgentView],
    map: &MapRender,
    events: &[crate::world_model::WorldEvent],
) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());
    render_header(frame, root[0], sim, app);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(26),
            Constraint::Percentage(44),
            Constraint::Percentage(30),
        ])
        .split(root[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(main[2]);

    render_agent_list(frame, main[0], views, app);
    render_map(frame, main[1], map);
    render_agent_detail(frame, right[0], views.get(app.selected_agent));
    render_events(frame, right[1], events, views.get(app.selected_agent));

    let help = Paragraph::new(
        "q sair | espaco pausa | n step | +/- velocidade | setas selecionam | f filtro",
    )
    .block(Block::default().borders(Borders::ALL).title("Controles"));
    frame.render_widget(help, root[2]);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, sim: &Simulation, app: &AppState) {
    let economy = sim
        .economy_overview()
        .into_iter()
        .next()
        .unwrap_or_default();
    let politics = sim
        .politics_overview()
        .into_iter()
        .next()
        .unwrap_or_default();
    let mut planted = 0;
    let mut growing = 0;
    let mut ready = 0;
    for crop in sim.crops.values() {
        match crop.stage {
            crate::world_model::CropStage::Planted => planted += 1,
            crate::world_model::CropStage::Growing => growing += 1,
            crate::world_model::CropStage::Ready => ready += 1,
        }
    }

    let text = vec![
        Line::from(vec![
            Span::styled(
                sim.summary(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" | "),
            Span::raw(format!(
                "LLM={} | estado={} | velocidade={} tick/s | filtro={} | Plantas: {}P, {}C, {}R",
                app.provider_name,
                if app.paused { "pausado" } else { "rodando" },
                app.ticks_per_second,
                app.role_filter.as_deref().unwrap_or("todos"),
                planted,
                growing,
                ready
            )),
        ]),
        Line::from(economy),
        Line::from(politics),
        Line::from(
            "Mapa: @ agente selecionado | & em conversa | * caminho | # parede | + porta | = rua | , campo | . plantado | v crescendo | Y pronto | ^ lenhal | % pedreira",
        ),
    ];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(sim.village_name()),
        ),
        area,
    );
}

fn render_agent_list(frame: &mut Frame<'_>, area: Rect, views: &[AgentView], app: &AppState) {
    let items: Vec<ListItem<'_>> = views
        .iter()
        .enumerate()
        .map(|(idx, view)| {
            let line = format!(
                "{} | {} | ({}, {})",
                view.name, view.role_name, view.position.x, view.position.y
            );
            let style = if idx == app.selected_agent {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Aldeoes")),
        area,
    );
}

fn render_map(frame: &mut Frame<'_>, area: Rect, map: &MapRender) {
    let lines = map
        .rows
        .iter()
        .map(|row| Line::from(Span::raw(row.clone())))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Mapa da Vila"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_agent_detail(frame: &mut Frame<'_>, area: Rect, view: Option<&AgentView>) {
    let Some(view) = view else {
        frame.render_widget(
            Paragraph::new("Sem agentes para o filtro atual.")
                .block(Block::default().borders(Borders::ALL).title("Detalhe")),
            area,
        );
        return;
    };

    let strongest = view
        .relations
        .iter()
        .max_by_key(|(_, relation)| {
            relation.friendship + relation.trust + relation.resentment.abs()
        })
        .map(|(other_id, relation)| {
            format!(
                "Mais forte com #{other_id}: amizade {} confianca {} ressentimento {}",
                relation.friendship, relation.trust, relation.resentment
            )
        })
        .unwrap_or_else(|| "Sem relacoes fortes ainda.".to_string());
    let memories = view
        .recent_memories
        .iter()
        .map(|memory| format!("- [{}] {}", format!("{:?}", memory.kind), memory.summary))
        .collect::<Vec<_>>()
        .join("\n");
    let pantry = if view.household_pantry.is_empty() {
        "-".to_string()
    } else {
        view.household_pantry
            .iter()
            .map(|stack| format!("{} x{}", stack.resource_id, stack.amount))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let work_stock = if view.work_establishment_stock.is_empty() {
        "-".to_string()
    } else {
        view.work_establishment_stock
            .iter()
            .map(|stack| format!("{} x{}", stack.resource_id, stack.amount))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let local_prices = if view.local_prices.is_empty() {
        "-".to_string()
    } else {
        view.local_prices
            .iter()
            .map(|price| format!("{}={}m", price.resource_id, price.unit_price))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let carrying = if view.carrying.is_empty() {
        "-".to_string()
    } else {
        view.carrying
            .iter()
            .map(|stack| format!("{} x{}", stack.resource_id, stack.amount))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let conversation = if let Some(conversation_id) = view.active_conversation_id {
        format!(
            "Conversa ativa: #{}\nParceiro: {}\nTurnos: {}\nFalando agora: {}\nUltimo ato social: {}\nResumo: {}",
            conversation_id,
            view.conversation_partner_name
                .clone()
                .unwrap_or_else(|| "desconhecido".to_string()),
            view.conversation_turn_count.unwrap_or(0),
            if view.speaking_now { "sim" } else { "nao" },
            view.last_social_act
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            view.conversation_summary
                .clone()
                .unwrap_or_else(|| "sem resumo".to_string())
        )
    } else {
        format!(
            "Conversa ativa: nao\nUltimo ato social: {}",
            view.last_social_act
                .clone()
                .unwrap_or_else(|| "-".to_string())
        )
    };
    let detail = format!(
        "Nome: {}\nPapel: {}\nVida: {:?}\nFerimentos: leves={} graves={} dor={} sangramento={}\nLar: {}\nArea: {}\nEdificio: {}\nSala: {}\nPosicao: ({}, {})\nDestino: {}\nCaminho pendente: {} tiles\nFoco: {}\nHumor:{} Energia:{} Saude:{} Fome:{} Stress:{}\n\nIntento geral: {}\nUltimo pensamento: {}\n\nPolitica:\nPosicao: {}\nQueixas: {}\n\nEconomia:\nCaixa do lar: {}\nDivida de imposto: {}\nDespensa: {}\nSalario pendente: {}\nCaixa publico: {}\nTarefa economica: {}\nCarregando: {}\nEstabelecimento: {}\nCaixa do estabelecimento: {}\nEstoque do estabelecimento: {}\nPrecos locais: {}\n\n{}\n\n{}\n\nMemorias recentes:\n{}",
        view.name,
        view.role_name,
        view.life_status,
        view.injury.light_wounds,
        view.injury.severe_wounds,
        view.injury.pain,
        view.injury.bleeding,
        view.household_name
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        view.area,
        view.building
            .clone()
            .unwrap_or_else(|| "Exterior".to_string()),
        view.room.clone().unwrap_or_else(|| "-".to_string()),
        view.position.x,
        view.position.y,
        view.destination_label.clone().unwrap_or_else(|| view
            .destination
            .map(|coord| format!("({}, {})", coord.x, coord.y))
            .unwrap_or_else(|| "nenhum".to_string())),
        view.path_len,
        view.state.current_focus,
        view.state.mood,
        view.state.energy,
        view.state.health,
        view.state.hunger,
        view.state.stress,
        view.last_intent
            .as_ref()
            .map(|intent| format!("{} ({})", intent.kind.as_str(), intent.justification))
            .unwrap_or_else(|| "nenhuma".to_string()),
        view.last_thought,
        view.political_position,
        if view.political_grievances.is_empty() {
            "-".to_string()
        } else {
            view.political_grievances.join("; ")
        },
        view.household_treasury,
        view.household_tax_arrears,
        pantry,
        view.pending_salary,
        view.public_treasury,
        view.active_task_summary
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        carrying,
        view.work_establishment_name
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        view.work_establishment_cash
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        work_stock,
        local_prices,
        conversation,
        strongest,
        memories
    );
    frame.render_widget(
        Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title("Agente"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_events(
    frame: &mut Frame<'_>,
    area: Rect,
    events: &[crate::world_model::WorldEvent],
    selected: Option<&AgentView>,
) {
    let selected_id = selected.map(|view| view.id);
    let lines = events
        .iter()
        .map(|event| {
            let mut style = Style::default();
            if Some(event.actor) == selected_id || event.target == selected_id {
                style = style.fg(Color::LightGreen);
            }
            Line::from(Span::styled(
                format!("D{} T{} | {}", event.day, event.tick, event.summary),
                style,
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Timeline / Porque"),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn filtered_views(mut views: Vec<AgentView>, filter: Option<String>) -> Vec<AgentView> {
    if let Some(role_id) = filter {
        views.retain(|view| view.role_id == role_id);
    }
    views
}

fn next_filter(current: Option<String>) -> Option<String> {
    const ROLE_FILTERS: [&str; 6] = [
        "campones",
        "ferreiro",
        "padeiro",
        "taverneiro",
        "guarda",
        "lider_local",
    ];
    match current {
        None => Some(ROLE_FILTERS[0].to_string()),
        Some(current_role) => {
            let idx = ROLE_FILTERS
                .iter()
                .position(|role| *role == current_role)
                .unwrap_or(0);
            if idx + 1 >= ROLE_FILTERS.len() {
                None
            } else {
                Some(ROLE_FILTERS[idx + 1].to_string())
            }
        }
    }
}

struct AppState {
    selected_agent: usize,
    paused: bool,
    ticks_per_second: u32,
    tick_rate_ms: u64,
    role_filter: Option<String>,
    provider_name: String,
}

impl AppState {
    fn new(provider_name: String) -> Self {
        Self {
            selected_agent: 0,
            paused: false,
            ticks_per_second: DEFAULT_TICKS_PER_SECOND,
            tick_rate_ms: tick_interval_ms(DEFAULT_TICKS_PER_SECOND),
            role_filter: None,
            provider_name,
        }
    }
}
