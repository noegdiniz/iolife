use crate::llm_adapter::LlmAdapter;
use crate::persistence::Persistence;
use crate::sim_core::{AgentView, DEFAULT_TICKS_PER_SECOND, Simulation, tick_interval_ms};
use anyhow::Result;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessConfig {
    pub max_ticks: Option<u64>,
    pub max_days: Option<u32>,
    pub save_every_ticks: Option<u64>,
    pub summary_every_ticks: u64,
    pub event_tail: usize,
    pub render_map: bool,
    pub ticks_per_second: u32,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            max_ticks: None,
            max_days: None,
            save_every_ticks: Some(24),
            summary_every_ticks: 24,
            event_tail: 8,
            render_map: false,
            ticks_per_second: DEFAULT_TICKS_PER_SECOND,
        }
    }
}

pub fn run_headless(
    mut sim: Simulation,
    llm: Box<dyn LlmAdapter>,
    mut persistence: Persistence,
    config: HeadlessConfig,
) -> Result<()> {
    let provider_name = llm.provider_name().to_string();
    let started_day = sim.current_day();
    let started_total_ticks = sim.total_ticks();
    let stop_total_ticks = config
        .max_ticks
        .map(|ticks| started_total_ticks.saturating_add(ticks));
    let stop_day = config
        .max_days
        .map(|days| started_day.saturating_add(days.max(1)));
    let mut last_saved_day = sim.current_day();
    let mut ran_ticks = 0_u64;

    println!(
        "[headless] iniciado | provider={} | resumo={} | limite_ticks={} | limite_dias={} | ticks_por_segundo={}",
        provider_name,
        sim.summary(),
        config
            .max_ticks
            .map(|value| value.to_string())
            .unwrap_or_else(|| "nenhum".to_string()),
        config
            .max_days
            .map(|value| value.to_string())
            .unwrap_or_else(|| "nenhum".to_string()),
        config.ticks_per_second
    );
    print_report(&mut sim, &config, "estado_inicial");

    while !should_stop(&sim, ran_ticks, stop_total_ticks, stop_day) {
        let tick_started = Instant::now();
        sim.tick(llm.as_ref())?;
        ran_ticks += 1;

        if sim.current_day() != last_saved_day {
            persistence.save(&mut sim, "daily")?;
            last_saved_day = sim.current_day();
        }

        if let Some(interval) = config.save_every_ticks {
            if interval > 0 && ran_ticks % interval == 0 {
                persistence.save(&mut sim, "interval")?;
            }
        }

        if config.summary_every_ticks > 0 && ran_ticks % config.summary_every_ticks == 0 {
            print_report(&mut sim, &config, "progresso");
        }

        let target_tick_duration = Duration::from_millis(tick_interval_ms(config.ticks_per_second));
        let elapsed = tick_started.elapsed();
        if elapsed < target_tick_duration {
            std::thread::sleep(target_tick_duration - elapsed);
        }
    }

    persistence.save(&mut sim, "shutdown")?;
    print_report(&mut sim, &config, "encerrado");
    println!(
        "[headless] finalizado | ticks_executados={} | resumo={}",
        ran_ticks,
        sim.summary()
    );
    Ok(())
}

fn should_stop(
    sim: &Simulation,
    ran_ticks: u64,
    stop_total_ticks: Option<u64>,
    stop_day: Option<u32>,
) -> bool {
    if ran_ticks == 0 {
        return false;
    }
    if stop_total_ticks.is_some_and(|limit| sim.total_ticks() >= limit) {
        return true;
    }
    if stop_day.is_some_and(|limit| sim.current_day() >= limit) {
        return true;
    }
    false
}

fn print_report(sim: &mut Simulation, config: &HeadlessConfig, label: &str) {
    let views = sim.agent_views();
    let active_conversations = count_active_conversations(&views);
    let events = sim.recent_events(config.event_tail);
    let avg_hunger = average_metric(&views, |view| view.state.hunger);
    let avg_energy = average_metric(&views, |view| view.state.energy);
    let avg_stress = average_metric(&views, |view| view.state.stress);

    println!(
        "[headless] {} | {} | agentes={} | conversas_ativas={} | fome_media={:.1} | energia_media={:.1} | stress_medio={:.1}",
        label,
        sim.summary(),
        views.len(),
        active_conversations,
        avg_hunger,
        avg_energy,
        avg_stress
    );

    for line in summarize_roles(&views) {
        println!("[headless] papeis | {}", line);
    }
    for line in sim.economy_overview() {
        println!("[headless] economia | {}", line);
    }
    for line in sim.history_overview() {
        println!("[headless] historia | {}", line);
    }
    for line in sim.legal_overview() {
        println!("[headless] justica | {}", line);
    }
    for line in sim.politics_overview() {
        println!("[headless] politica | {}", line);
    }
    for line in sim.culture_overview() {
        println!("[headless] cultura | {}", line);
    }
    for line in sim.meetings_overview() {
        println!("[headless] encontros | {}", line);
    }

    for view in views.iter().take(6) {
        let pantry = if view.household_pantry.is_empty() {
            "-".to_string()
        } else {
            view.household_pantry
                .iter()
                .map(|stack| format!("{}x{}", stack.resource_id, stack.amount))
                .collect::<Vec<_>>()
                .join(",")
        };
        let rumors = if view.known_rumors.is_empty() {
            "-".to_string()
        } else {
            view.known_rumors.join("; ")
        };
        let stories = if view.known_stories.is_empty() {
            "-".to_string()
        } else {
            view.known_stories.join("; ")
        };
        let equipped = if view.equipped_items.is_empty() {
            "-".to_string()
        } else {
            view.equipped_items.join("; ")
        };
        let inventory_items = if view.inventory_items.is_empty() {
            "-".to_string()
        } else {
            view.inventory_items.join("; ")
        };
        let work_items = if view.work_establishment_items.is_empty() {
            "-".to_string()
        } else {
            view.work_establishment_items.join("; ")
        };
        println!(
            "[headless] agente | {} | papel={} | vida={:?} | ferimentos=leves:{} graves:{} dor:{} sangramento:{} | pos=({}, {}) | area={} | destino={} | intencao={} | controle={} | planner={} | utility={} | stance={} | reativo={} | vinganca_alvo={} | pressao_status={} | desafio={} | politica={} | queixas={} | instituicoes=lider:{} justica:{} imposto:{} rac:{} guardas:{} guerra:{} medo:{} corrupcao:{} equidade:{} | feudal=titulo:{} senhor:{} poder:{} obrigacoes:{} sucessao:{} | psicologia={} | plano_longo={} | prestigio={}({}) | equipamento={} | inventario={} | oficio=fer:{} alf:{} our:{} couro:{} | rumores={} | historias={} | caixa_lar={} | imposto_devendo={} | caixa_publico={} | pantry={} | salario_pendente={} | tarefa={} | estoque_inst_trabalho={} | pensamento={} ",
            view.name,
            view.role_name,
            view.life_status,
            view.injury.light_wounds,
            view.injury.severe_wounds,
            view.injury.pain,
            view.injury.bleeding,
            view.position.x,
            view.position.y,
            view.area,
            view.destination_label
                .clone()
                .or_else(|| view
                    .destination
                    .map(|coord| format!("({}, {})", coord.x, coord.y)))
                .unwrap_or_else(|| "-".to_string()),
            view.last_intent
                .as_ref()
                .map(|intent| intent.kind.as_str().to_string())
                .unwrap_or_else(|| "-".to_string()),
            view.control_mode,
            view.planner_status,
            view.active_utility_directive
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            view.reactive_stance.clone(),
            view.reactive_reason.clone(),
            view.reactive_revenge_target
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            view.reactive_status_pressure.clone(),
            view.reactive_defiance_posture.clone(),
            view.political_position,
            if view.political_grievances.is_empty() {
                "-".to_string()
            } else {
                view.political_grievances.join("; ")
            },
            view.institutional_perception.leader_legitimacy,
            view.institutional_perception.justice_legitimacy,
            view.institutional_perception.tax_legitimacy,
            view.institutional_perception.rationing_legitimacy,
            view.institutional_perception.guard_trust,
            view.institutional_perception.war_support,
            view.institutional_perception.fear_of_authority,
            view.institutional_perception.perceived_corruption,
            view.institutional_perception.perceived_fairness,
            view.feudal_title.clone().unwrap_or_else(|| "-".to_string()),
            view.direct_lord_name
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            view.feudal_power_summary
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            if view.feudal_obligations.is_empty() {
                "-".to_string()
            } else {
                view.feudal_obligations.join("; ")
            },
            if view.succession_status.is_empty() {
                "-".to_string()
            } else {
                view.succession_status.join("; ")
            },
            view.psychological_state.summary(),
            if view.psychological_state.long_term_plan.is_empty() {
                "-".to_string()
            } else {
                view.psychological_state.long_term_plan.clone()
            },
            view.visible_prestige_summary,
            view.perceived_status_score,
            equipped,
            inventory_items,
            view.craft_proficiencies.smithing,
            view.craft_proficiencies.tailoring,
            view.craft_proficiencies.jewelry,
            view.craft_proficiencies.leatherwork,
            rumors,
            stories,
            view.household_treasury,
            view.household_tax_arrears,
            view.public_treasury,
            pantry,
            view.pending_salary,
            view.active_task_summary
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            work_items,
            truncate_for_log(&view.last_thought, 300)
        );
    }

    if !events.is_empty() {
        println!("[headless] eventos_recentes");
        for event in events.into_iter().rev() {
            println!(
                "[headless]   D{} T{} | {:?} | {}",
                event.day, event.tick, event.kind, event.summary
            );
        }
    }

    if config.render_map {
        let selected_id = views.first().map(|view| view.id);
        let width = sim.spatial().grid.width.max(1) as usize;
        let height = sim.spatial().grid.height.max(1) as usize;
        let map = sim.render_ascii_map(selected_id, width, height);
        println!("[headless] mapa");
        for row in map.rows {
            println!("{}", row);
        }
    }
}

fn summarize_roles(views: &[AgentView]) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for view in views {
        *counts.entry(view.role_name.as_str()).or_insert(0_usize) += 1;
    }
    counts
        .into_iter()
        .map(|(role, count)| format!("{}={}", role, count))
        .collect()
}

fn count_active_conversations(views: &[AgentView]) -> usize {
    let mut unique = BTreeMap::new();
    for view in views {
        if let Some(conversation_id) = view.active_conversation_id {
            unique.insert(conversation_id, ());
        }
    }
    unique.len()
}

fn average_metric<F>(views: &[AgentView], metric: F) -> f32
where
    F: Fn(&AgentView) -> i32,
{
    if views.is_empty() {
        return 0.0;
    }
    let sum: i32 = views.iter().map(metric).sum();
    sum as f32 / views.len() as f32
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    let truncated: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::{HeadlessConfig, should_stop};
    use crate::sim_core::{DEFAULT_TICKS_PER_SECOND, Simulation, SimulationConfig};

    #[test]
    fn headless_defaults_are_safe_for_batch_runs() {
        let config = HeadlessConfig::default();
        assert_eq!(config.max_ticks, None);
        assert_eq!(config.max_days, None);
        assert_eq!(config.save_every_ticks, Some(24));
        assert_eq!(config.summary_every_ticks, 24);
        assert_eq!(config.event_tail, 8);
        assert!(!config.render_map);
        assert_eq!(config.ticks_per_second, DEFAULT_TICKS_PER_SECOND);
    }

    #[test]
    fn stop_condition_uses_total_ticks_or_day_limit_after_progress() {
        let sim = Simulation::seeded(SimulationConfig::default());
        assert!(!should_stop(&sim, 0, Some(0), Some(1)));
        assert!(!should_stop(&sim, 1, Some(10), Some(5)));
    }
}
