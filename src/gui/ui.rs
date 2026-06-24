use super::GameState;
use super::runtime::GuiRuntimeState;
use bevy::prelude::*;

pub struct GuiUiPlugin;

impl Plugin for GuiUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_ui)
            .add_systems(Update, refresh_ui);
    }
}

#[derive(Component)]
struct TopBarText;

#[derive(Component)]
struct SidePanelText;

#[derive(Component)]
struct TimelineText;

fn setup_ui(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(10),
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.78)),
        Text::new("iniciando GUI..."),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.88, 0.78)),
        TopBarText,
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(12),
            top: px(56),
            width: px(410),
            padding: UiRect::all(px(10)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.025, 0.02, 0.82)),
        Text::new("sem agente selecionado"),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.84, 0.74)),
        SidePanelText,
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(12),
            width: px(760),
            padding: UiRect::all(px(10)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.025, 0.02, 0.82)),
        Text::new("eventos recentes"),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.86, 0.82, 0.70)),
        TimelineText,
    ));
}

fn refresh_ui(
    mut game: NonSendMut<GameState>,
    runtime: Res<GuiRuntimeState>,
    mut top_q: Query<
        &mut Text,
        (
            With<TopBarText>,
            Without<SidePanelText>,
            Without<TimelineText>,
        ),
    >,
    mut side_q: Query<
        &mut Text,
        (
            With<SidePanelText>,
            Without<TopBarText>,
            Without<TimelineText>,
        ),
    >,
    mut timeline_q: Query<
        &mut Text,
        (
            With<TimelineText>,
            Without<TopBarText>,
            Without<SidePanelText>,
        ),
    >,
) {
    if let Ok(mut text) = top_q.single_mut() {
        **text = top_bar_text(&*game, &runtime);
    }
    if let Ok(mut text) = side_q.single_mut() {
        **text = side_panel_text(&mut *game);
    }
    if let Ok(mut text) = timeline_q.single_mut() {
        **text = timeline_text(&mut *game);
    }
}

fn top_bar_text(game: &GameState, runtime: &GuiRuntimeState) -> String {
    let mode = if runtime.paused { "PAUSADO" } else { "RODANDO" };
    let error = runtime
        .last_error
        .as_ref()
        .map(|error| format!(" | ERRO={error}"))
        .unwrap_or_default();
    format!(
        "{} | {} | {} | TPS={} | Space pause/play | . tick | S salvar{}",
        game.sim.summary(),
        mode,
        game.sim.time_context().day_phase,
        runtime.ticks_per_second,
        error
    )
}

fn side_panel_text(game: &mut GameState) -> String {
    let Some(selected_agent_id) = game.selected_agent_id else {
        return "sem agente selecionado\nClique em um agente no mapa.".to_string();
    };
    let views = game.sim.agent_views();
    let Some(view) = views.iter().find(|view| view.id == selected_agent_id) else {
        return format!("agente {} nao encontrado", selected_agent_id);
    };

    let intent = view
        .last_intent
        .as_ref()
        .map(|intent| format!("{:?}: {}", intent.kind, intent.justification))
        .unwrap_or_else(|| "-".to_string());
    let conversation = view
        .active_conversation_id
        .map(|id| {
            format!(
                "#{} turnos={:?} participantes=[{}] fala_agora={}",
                id,
                view.conversation_turn_count,
                view.conversation_participant_names.join(", "),
                view.speaking_now
            )
        })
        .unwrap_or_else(|| "-".to_string());
    let pantry = view
        .household_pantry
        .iter()
        .map(|stack| format!("{}x{}", stack.resource_id, stack.amount))
        .collect::<Vec<_>>()
        .join(", ");

    let mut lines = vec![
        format!("{} #{} | {}", view.name, view.id, view.role_name),
        format!(
            "vida={:?} ferimentos=L{} G{} dor={} sangramento={}",
            view.life_status,
            view.injury.light_wounds,
            view.injury.severe_wounds,
            view.injury.pain,
            view.injury.bleeding
        ),
        format!(
            "pos=({}, {}) area={} destino={}",
            view.position.x,
            view.position.y,
            view.area,
            view.destination_label
                .clone()
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "controle={} planner={} utility={}",
            view.control_mode,
            view.planner_status,
            view.active_utility_directive
                .clone()
                .unwrap_or_else(|| "-".to_string())
        ),
        format!("stance={} | {}", view.reactive_stance, view.reactive_reason),
        format!("intencao={intent}"),
        format!("conversa={conversation}"),
        format!(
            "tarefa={} salario={} caixa_lar={} pantry=[{}]",
            view.active_task_summary
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            view.pending_salary,
            view.household_treasury,
            pantry
        ),
        format!(
            "politica={} queixas={}",
            view.political_position,
            if view.political_grievances.is_empty() {
                "-".to_string()
            } else {
                view.political_grievances.join("; ")
            }
        ),
        format!(
            "instituicoes lider={} justica={} imposto={} guardas={} medo={}",
            view.institutional_perception.leader_legitimacy,
            view.institutional_perception.justice_legitimacy,
            view.institutional_perception.tax_legitimacy,
            view.institutional_perception.guard_trust,
            view.institutional_perception.fear_of_authority
        ),
        format!("plano_longo={}", view.psychological_state.long_term_plan),
        format!("pensamento={}", view.last_thought),
    ];

    if !view.scheduled_meetings.is_empty() {
        lines.push(format!("encontros={}", view.scheduled_meetings.join(" | ")));
    }
    lines.join("\n")
}

fn timeline_text(game: &mut GameState) -> String {
    let mut lines = vec!["eventos recentes".to_string()];
    for event in game.sim.recent_events(12) {
        lines.push(format!(
            "D{} T{} | {:?} | {}",
            event.day, event.tick, event.kind, event.summary
        ));
    }
    for line in game.sim.economy_overview().into_iter().take(3) {
        lines.push(format!("eco | {line}"));
    }
    for line in game.sim.politics_overview().into_iter().take(2) {
        lines.push(format!("pol | {line}"));
    }
    for line in game.sim.meetings_overview().into_iter().take(2) {
        lines.push(format!("enc | {line}"));
    }
    lines.join("\n")
}
