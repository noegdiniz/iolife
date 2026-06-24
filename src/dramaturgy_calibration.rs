use crate::sim_core::Simulation;
use crate::world_model::{
    EconomicTaskPhase, EventKind, PsychologicalState, ResourceStack, SimulationSnapshot, WorldEvent,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DramaturgyCalibrationConfig {
    pub expected_days: u32,
    pub max_examples: usize,
    pub acceptable_global_score: i32,
    pub critical_axis_floor: i32,
}

impl Default for DramaturgyCalibrationConfig {
    fn default() -> Self {
        Self {
            expected_days: 2,
            max_examples: 8,
            acceptable_global_score: 70,
            critical_axis_floor: 55,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DramaturgyScore {
    pub material_coherence: i32,
    pub psychological_continuity: i32,
    pub consequence_followthrough: i32,
    pub pressure_arc: i32,
    pub social_differentiation: i32,
    pub cultural_resonance: i32,
    pub institutional_legibility: i32,
    pub noise_control: i32,
}

impl DramaturgyScore {
    pub fn global_score(&self) -> i32 {
        let values = [
            self.material_coherence,
            self.psychological_continuity,
            self.consequence_followthrough,
            self.pressure_arc,
            self.social_differentiation,
            self.cultural_resonance,
            self.institutional_legibility,
            self.noise_control,
        ];
        values.iter().sum::<i32>() / values.len() as i32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DramaturgyCalibrationSnapshot {
    pub snapshot: SimulationSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DramaturgyCalibrationReport {
    pub global_score: i32,
    pub thresholds_met: bool,
    pub critical_failures: Vec<String>,
    pub scores: DramaturgyScore,
    pub event_counts: BTreeMap<String, u64>,
    pub food_system_metrics: BTreeMap<String, u64>,
    pub top_psychological_arcs: Vec<String>,
    pub top_noise_failures: Vec<String>,
    pub good_chains: Vec<String>,
    pub bad_chains: Vec<String>,
}

impl DramaturgyCalibrationReport {
    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "score_global={} thresholds={} material={} psicologia={} consequencia={} arco={} diferenciacao={} cultura={} instituicao={} ruido={}",
            self.global_score,
            if self.thresholds_met { "ok" } else { "falha" },
            self.scores.material_coherence,
            self.scores.psychological_continuity,
            self.scores.consequence_followthrough,
            self.scores.pressure_arc,
            self.scores.social_differentiation,
            self.scores.cultural_resonance,
            self.scores.institutional_legibility,
            self.scores.noise_control,
        )];
        if !self.critical_failures.is_empty() {
            lines.push(format!(
                "falhas_criticas={}",
                self.critical_failures.join("; ")
            ));
        }
        if !self.food_system_metrics.is_empty() {
            let food_metrics = self
                .food_system_metrics
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("alimentacao={food_metrics}"));
        }
        if !self.top_noise_failures.is_empty() {
            lines.push(format!("ruido={}", self.top_noise_failures.join("; ")));
        }
        if !self.good_chains.is_empty() {
            lines.push(format!("cadeias_boas={}", self.good_chains.join("; ")));
        }
        if !self.bad_chains.is_empty() {
            lines.push(format!("cadeias_fracas={}", self.bad_chains.join("; ")));
        }
        lines
    }
}

impl Simulation {
    pub fn dramaturgy_calibration_snapshot(&mut self) -> DramaturgyCalibrationSnapshot {
        DramaturgyCalibrationSnapshot {
            snapshot: self.snapshot(),
        }
    }
}

pub fn run_dramaturgy_calibration(
    sim: &mut Simulation,
    config: DramaturgyCalibrationConfig,
) -> DramaturgyCalibrationReport {
    let calibration_snapshot = sim.dramaturgy_calibration_snapshot();
    analyze_snapshot(&calibration_snapshot.snapshot, &config)
}

pub fn analyze_snapshot(
    snapshot: &SimulationSnapshot,
    config: &DramaturgyCalibrationConfig,
) -> DramaturgyCalibrationReport {
    let event_counts = event_counts(&snapshot.events);
    let food_system_metrics = food_system_metrics(snapshot);
    let top_noise_failures = top_noise_failures(&snapshot.events, config.max_examples);
    let bad_material_events = material_coherence_failures(&snapshot.events);
    let strong_events = strong_events(&snapshot.events);
    let psychological_markers = psychological_marker_count(snapshot);
    let followthrough_hits = consequence_followthrough_hits(snapshot);
    let good_chains = good_chains(
        snapshot,
        &event_counts,
        psychological_markers,
        config.max_examples,
    );
    let mut bad_chains = Vec::new();

    let scores = DramaturgyScore {
        material_coherence: score_material_coherence(snapshot, bad_material_events.len()),
        psychological_continuity: score_psychological_continuity(
            strong_events.len(),
            psychological_markers,
            snapshot,
        ),
        consequence_followthrough: score_consequence_followthrough(
            strong_events.len(),
            followthrough_hits,
        ),
        pressure_arc: score_pressure_arc(snapshot),
        social_differentiation: score_social_differentiation(snapshot),
        cultural_resonance: score_cultural_resonance(snapshot, config.expected_days),
        institutional_legibility: score_institutional_legibility(snapshot),
        noise_control: score_noise_control(&snapshot.events, &top_noise_failures, snapshot),
    };

    if !bad_material_events.is_empty() {
        bad_chains.extend(bad_material_events.into_iter().take(config.max_examples));
    }
    if strong_events.len() > 0 && psychological_markers == 0 {
        bad_chains.push("eventos fortes sem marcas psicologicas persistentes".to_string());
    }
    if followthrough_hits == 0 && strong_events.len() > 0 {
        bad_chains.push("eventos fortes sem follow-through institucional/social claro".to_string());
    }
    bad_chains.extend(top_noise_failures.iter().take(3).cloned());
    bad_chains.truncate(config.max_examples);

    let global_score = scores.global_score();
    let mut critical_failures = Vec::new();
    if scores.material_coherence < config.critical_axis_floor {
        critical_failures.push("material_coherence abaixo do piso critico".to_string());
    }
    if scores.consequence_followthrough < config.critical_axis_floor {
        critical_failures.push("consequence_followthrough abaixo do piso critico".to_string());
    }
    if scores.noise_control < config.critical_axis_floor {
        critical_failures.push("noise_control abaixo do piso critico".to_string());
    }
    if global_score < config.acceptable_global_score {
        critical_failures.push("score global abaixo do aceitavel".to_string());
    }

    DramaturgyCalibrationReport {
        global_score,
        thresholds_met: critical_failures.is_empty(),
        critical_failures,
        scores,
        event_counts,
        food_system_metrics,
        top_psychological_arcs: top_psychological_arcs(snapshot, config.max_examples),
        top_noise_failures,
        good_chains,
        bad_chains,
    }
}

fn event_counts(events: &[WorldEvent]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for event in events {
        *counts.entry(format!("{:?}", event.kind)).or_insert(0) += 1;
    }
    counts
}

fn food_system_metrics(snapshot: &SimulationSnapshot) -> BTreeMap<String, u64> {
    let mut metrics = BTreeMap::new();
    let mut crisis_days = HashSet::new();
    for event in &snapshot.events {
        let tags = &event.impact_tags;
        if tags.iter().any(|tag| tag == "crise_alimentar") {
            crisis_days.insert(event.day);
            *metrics
                .entry("eventos_crise_alimentar".to_string())
                .or_insert(0) += 1;
        }
        if tags.iter().any(|tag| tag == "sem_fornecedor_material") {
            *metrics
                .entry("sem_fornecedor_material".to_string())
                .or_insert(0) += 1;
        }
        if tags.iter().any(|tag| tag == "processador_parado") {
            *metrics
                .entry("processadores_parados".to_string())
                .or_insert(0) += 1;
        }
        if tags.iter().any(|tag| tag == "vantagem_feudal_alimentar") {
            *metrics
                .entry("injustica_alimentar_feudal".to_string())
                .or_insert(0) += 1;
        }
        if tags.iter().any(|tag| tag == "acesso_alimentar_fragil") {
            *metrics
                .entry("acesso_alimentar_fragil".to_string())
                .or_insert(0) += 1;
        }
        if tags.iter().any(|tag| tag == "alimento_transportado")
            || event.summary.contains("Transportar") && event.summary.contains("graos")
            || event.summary.contains("Comprar") && event.summary.contains("graos")
        {
            *metrics
                .entry("recuperacao_material_alimento".to_string())
                .or_insert(0) += 1;
        }
        if event.summary.contains("outra vila") || event.summary.contains("assentamento") {
            *metrics
                .entry("comercio_alimentar_entre_vilas".to_string())
                .or_insert(0) += 1;
        }
    }
    metrics.insert(
        "dias_com_crise_alimentar".to_string(),
        crisis_days.len() as u64,
    );
    let total_household_food: i32 = snapshot
        .households
        .iter()
        .map(|household| {
            total_stack_units(&household.pantry) + total_stack_units(&household.reserved_food)
        })
        .sum();
    metrics.insert(
        "estoque_alimentar_domestico_total".to_string(),
        total_household_food.max(0) as u64,
    );
    metrics
}

fn material_coherence_failures(events: &[WorldEvent]) -> Vec<String> {
    events
        .iter()
        .filter(|event| {
            event.impact_tags.iter().any(|tag| {
                matches!(
                    tag.as_str(),
                    "sem_origem_material"
                        | "estoque_criado_sem_origem"
                        | "fallback_externo"
                        | "fallback_externo_acionado"
                        | "external_market"
                )
            }) || event.summary.contains("ExternalMarket")
                || event.summary.contains("mercado externo")
                || event.summary.contains("fallback externo")
        })
        .map(|event| format!("origem material suspeita: {}", event.summary))
        .collect()
}

fn score_material_coherence(snapshot: &SimulationSnapshot, failure_count: usize) -> i32 {
    let mut score = 100 - failure_count as i32 * 25;
    if snapshot
        .households
        .iter()
        .all(|household| household.pantry.is_empty())
        && snapshot
            .establishments
            .iter()
            .all(|establishment| establishment.stock.is_empty())
    {
        score -= 20;
    }
    clamp_score(score)
}

fn strong_events(events: &[WorldEvent]) -> Vec<&WorldEvent> {
    events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::Violence
                    | EventKind::Theft
                    | EventKind::Punishment
                    | EventKind::Death
                    | EventKind::Scarcity
                    | EventKind::CrimeReported
                    | EventKind::MilitarySupply
                    | EventKind::InstitutionalDispute
                    | EventKind::NormChanged
                    | EventKind::PoliticalPressure
                    | EventKind::FactionShift
                    | EventKind::Construction
                    | EventKind::CulturalStory
            )
        })
        .collect()
}

fn psychological_marker_count(snapshot: &SimulationSnapshot) -> usize {
    snapshot
        .agents
        .iter()
        .filter(|agent| psychological_weight(&agent.psychological_state) > 0)
        .count()
        + snapshot
            .agents
            .iter()
            .filter(|agent| !agent.memories.is_empty())
            .count()
        + snapshot
            .agents
            .iter()
            .filter(|agent| {
                agent.institutional_perception.leader_legitimacy != 0
                    || agent.institutional_perception.justice_legitimacy != 0
                    || agent.institutional_perception.tax_legitimacy != 0
                    || agent.institutional_perception.guard_trust != 0
            })
            .count()
}

fn psychological_weight(state: &PsychologicalState) -> i32 {
    state.grief
        + state.humiliation
        + state.fear
        + state.pride
        + state.trauma
        + state.anger
        + state.hope
        + state.guilt
        + state.status_anxiety
        + state.revenge_drive
        + state.submission_drive
        + state.dominance_drive
        + state
            .personal_symbols
            .iter()
            .map(|symbol| symbol.intensity)
            .sum::<i32>()
            / 2
        + state
            .coping_patterns
            .iter()
            .map(|pattern| pattern.strength)
            .sum::<i32>()
            / 3
        + state
            .inner_contradictions
            .iter()
            .map(|contradiction| contradiction.pressure)
            .sum::<i32>()
            / 3
        + if state.long_term_plan.trim().is_empty() {
            0
        } else {
            8
        }
        + if state.melancholic_fixation.is_some() {
            8
        } else {
            0
        }
}

fn score_psychological_continuity(
    strong_event_count: usize,
    psychological_markers: usize,
    snapshot: &SimulationSnapshot,
) -> i32 {
    if snapshot.agents.is_empty() {
        return 0;
    }
    let marker_ratio = psychological_markers as f32 / snapshot.agents.len() as f32;
    let mut score = (marker_ratio * 100.0).round() as i32;
    if strong_event_count > 0 && psychological_markers == 0 {
        score = 20;
    } else if strong_event_count == 0 {
        score = score.max(65);
    }
    clamp_score(score)
}

fn consequence_followthrough_hits(snapshot: &SimulationSnapshot) -> usize {
    let mut hits = 0;
    if !snapshot.crime_cases.is_empty()
        || snapshot.events.iter().any(|event| {
            matches!(
                event.kind,
                EventKind::Investigation | EventKind::Arrest | EventKind::Punishment
            )
        })
    {
        hits += 1;
    }
    if !snapshot.political_pressures.is_empty()
        || !snapshot.political_factions.is_empty()
        || !snapshot.policy_acts.is_empty()
    {
        hits += 1;
    }
    if !snapshot.promises.is_empty()
        || snapshot
            .secrets
            .iter()
            .any(|secret| format!("{:?}", secret.kind).contains("BrokenPromise"))
    {
        hits += 1;
    }
    if !snapshot.rumors.is_empty() || !snapshot.cultural_stories.is_empty() {
        hits += 1;
    }
    if !snapshot.economic_tasks.is_empty() || !snapshot.military_demands.is_empty() {
        hits += 1;
    }
    hits
}

fn score_consequence_followthrough(strong_event_count: usize, hits: usize) -> i32 {
    if strong_event_count == 0 {
        return 70;
    }
    clamp_score(35 + hits as i32 * 15)
}

fn score_pressure_arc(snapshot: &SimulationSnapshot) -> i32 {
    if snapshot.agents.is_empty() {
        return 0;
    }
    let hunger_values = snapshot
        .agents
        .iter()
        .map(|agent| agent.state.hunger)
        .collect::<Vec<_>>();
    let stress_values = snapshot
        .agents
        .iter()
        .map(|agent| agent.state.stress)
        .collect::<Vec<_>>();
    let psych_values = snapshot
        .agents
        .iter()
        .map(|agent| psychological_weight(&agent.psychological_state).min(100))
        .collect::<Vec<_>>();
    let avg_hunger = average(&hunger_values);
    let avg_stress = average(&stress_values);
    let variance = variance_score(&hunger_values)
        + variance_score(&stress_values)
        + variance_score(&psych_values);
    let saturation_penalty = if avg_hunger >= 90.0 || avg_stress >= 90.0 {
        30
    } else if avg_hunger <= 2.0 && avg_stress <= 2.0 && variance < 10 {
        20
    } else {
        0
    };
    clamp_score(65 + variance.min(30) - saturation_penalty)
}

fn score_social_differentiation(snapshot: &SimulationSnapshot) -> i32 {
    if snapshot.agents.len() < 2 {
        return 50;
    }
    let psych_values = snapshot
        .agents
        .iter()
        .map(|agent| psychological_weight(&agent.psychological_state).min(100))
        .collect::<Vec<_>>();
    let relation_count = snapshot
        .agents
        .iter()
        .map(|agent| agent.relations.len())
        .sum::<usize>();
    let roles = snapshot
        .agents
        .iter()
        .map(|agent| agent.role_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    clamp_score(
        35 + variance_score(&psych_values) + relation_count.min(40) as i32 + roles as i32 * 4,
    )
}

fn score_cultural_resonance(snapshot: &SimulationSnapshot, expected_days: u32) -> i32 {
    let story_count = snapshot.cultural_stories.len() as i32;
    let rumor_count = snapshot.rumors.len() as i32;
    let tradition_count = snapshot.cultural_traditions.len() as i32;
    let belief_count = snapshot
        .agents
        .iter()
        .map(|agent| agent.story_beliefs.len() as i32 + agent.rumor_beliefs.len() as i32)
        .sum::<i32>();
    let target = if expected_days >= 5 { 2 } else { 1 };
    let mut score = 45
        + (story_count.min(target + 3) * 14)
        + (tradition_count * 8)
        + rumor_count.min(4) * 4
        + belief_count.min(20);
    if story_count > 12 {
        score -= (story_count - 12) * 4;
    }
    clamp_score(score)
}

fn score_institutional_legibility(snapshot: &SimulationSnapshot) -> i32 {
    let institutional_events = snapshot
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::PoliticalPressure
                    | EventKind::PolicyProposal
                    | EventKind::PoliticalSupport
                    | EventKind::NormChanged
                    | EventKind::InstitutionalDispute
                    | EventKind::Investigation
                    | EventKind::Arrest
                    | EventKind::Punishment
                    | EventKind::TributeDemanded
                    | EventKind::TributePaid
                    | EventKind::TributeRefused
                    | EventKind::LevyCalled
                    | EventKind::LevyRefused
                    | EventKind::FeudalSanction
            )
        })
        .count() as i32;
    let structural_markers = snapshot.policy_acts.len() as i32
        + snapshot.political_pressures.len() as i32
        + snapshot.political_factions.len() as i32
        + snapshot.feudal_contracts.len() as i32
        + snapshot.estate_holdings.len() as i32
        + snapshot.wars.len() as i32
        + snapshot.insurrections.len() as i32
        + snapshot.crime_cases.len() as i32;
    clamp_score(45 + institutional_events.min(20) * 2 + structural_markers.min(25) * 2)
}

fn score_noise_control(
    events: &[WorldEvent],
    top_noise_failures: &[String],
    snapshot: &SimulationSnapshot,
) -> i32 {
    let repeated_penalty = top_noise_failures.len() as i32 * 12;
    let stale_task_penalty = snapshot
        .economic_tasks
        .iter()
        .filter(|task| {
            !matches!(
                task.phase,
                EconomicTaskPhase::Completed | EconomicTaskPhase::Failed
            )
        })
        .filter(|task| task.assigned_agent_id.is_none() && task.priority <= 3)
        .count() as i32
        * 3;
    let promise_penalty = snapshot.promises.len().saturating_sub(8) as i32 * 4;
    let empty_conversation_penalty = events
        .iter()
        .filter(|event| {
            matches!(event.kind, EventKind::ConversationTurn) && event.impact_tags.is_empty()
        })
        .count() as i32;
    clamp_score(
        100 - repeated_penalty - stale_task_penalty - promise_penalty - empty_conversation_penalty,
    )
}

fn top_noise_failures(events: &[WorldEvent], limit: usize) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for event in events {
        let key = format!(
            "{:?}|{}|{:?}|{}",
            event.kind, event.actor, event.target, event.summary
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut repeated = counts
        .into_iter()
        .filter(|(_, count)| *count >= 3)
        .map(|(key, count)| format!("evento repetido {}x: {}", count, key))
        .collect::<Vec<_>>();
    repeated.sort();
    repeated.truncate(limit);
    repeated
}

fn top_psychological_arcs(snapshot: &SimulationSnapshot, limit: usize) -> Vec<String> {
    let mut arcs = snapshot
        .agents
        .iter()
        .map(|agent| {
            let weight = psychological_weight(&agent.psychological_state);
            (
                weight,
                format!(
                    "{} peso={} plano={} memoria={} simbolos={} vinganca={}",
                    agent.name,
                    weight,
                    if agent.psychological_state.long_term_plan.is_empty() {
                        "-"
                    } else {
                        agent.psychological_state.long_term_plan.as_str()
                    },
                    agent.memories.len(),
                    agent.psychological_state.personal_symbols.len(),
                    agent
                        .psychological_state
                        .active_revenge_target
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string())
                ),
            )
        })
        .collect::<Vec<_>>();
    arcs.sort_by(|a, b| b.0.cmp(&a.0));
    arcs.into_iter().take(limit).map(|(_, line)| line).collect()
}

fn good_chains(
    snapshot: &SimulationSnapshot,
    event_counts: &BTreeMap<String, u64>,
    psychological_markers: usize,
    limit: usize,
) -> Vec<String> {
    let mut chains = Vec::new();
    if event_counts.get("Scarcity").copied().unwrap_or(0) > 0
        && !snapshot.political_pressures.is_empty()
    {
        chains.push("escassez gerou pressao politica".to_string());
    }
    if event_counts.get("Violence").copied().unwrap_or(0) > 0 && !snapshot.crime_cases.is_empty() {
        chains.push("violencia gerou caso de justica".to_string());
    }
    if !snapshot.cultural_stories.is_empty() {
        chains.push("eventos ou conversas alimentaram historia cultural".to_string());
    }
    if psychological_markers > 0 {
        chains.push("estado psicologico persistente registra consequencias".to_string());
    }
    if !snapshot.policy_acts.is_empty() && !snapshot.political_pressures.is_empty() {
        chains.push("pressao institucional convive com decretos ativos".to_string());
    }
    chains.truncate(limit);
    chains
}

fn average(values: &[i32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<i32>() as f32 / values.len() as f32
}

fn variance_score(values: &[i32]) -> i32 {
    if values.len() < 2 {
        return 0;
    }
    let avg = average(values);
    let variance = values
        .iter()
        .map(|value| {
            let diff = *value as f32 - avg;
            diff * diff
        })
        .sum::<f32>()
        / values.len() as f32;
    (variance.sqrt() / 2.0).round().clamp(0.0, 35.0) as i32
}

fn clamp_score(value: i32) -> i32 {
    value.clamp(0, 100)
}

fn total_stack_units(stacks: &[ResourceStack]) -> i32 {
    stacks.iter().map(|stack| stack.amount).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim_core::SimulationConfig;
    use crate::world_gen::generate_world;

    fn generated_snapshot() -> SimulationSnapshot {
        let config = SimulationConfig {
            max_agents: 8,
            history_years: 8,
            history_seed: Some(1234),
            ..Default::default()
        };
        generate_world(config).expect("world generation should succeed")
    }

    #[test]
    fn material_origin_failure_penalizes_material_coherence() {
        let mut snapshot = generated_snapshot();
        snapshot.events.push(WorldEvent {
            day: 1,
            tick: 0,
            actor: 0,
            target: None,
            kind: EventKind::Scarcity,
            summary: "graos surgiram por fallback externo".to_string(),
            impact_tags: vec!["sem_origem_material".to_string()],
        });

        let report = analyze_snapshot(&snapshot, &DramaturgyCalibrationConfig::default());

        assert!(report.scores.material_coherence < 100);
        assert!(
            report
                .bad_chains
                .iter()
                .any(|chain| chain.contains("origem material"))
        );
    }

    #[test]
    fn repeated_events_reduce_noise_control() {
        let mut snapshot = generated_snapshot();
        let repeated = WorldEvent {
            day: 1,
            tick: 0,
            actor: 1,
            target: None,
            kind: EventKind::PoliticalSupport,
            summary: "apoio repetido sem mudanca".to_string(),
            impact_tags: vec!["politica".to_string()],
        };
        snapshot.events.push(repeated.clone());
        snapshot.events.push(repeated.clone());
        snapshot.events.push(repeated);

        let report = analyze_snapshot(&snapshot, &DramaturgyCalibrationConfig::default());

        assert!(report.scores.noise_control < 100);
        assert!(!report.top_noise_failures.is_empty());
    }

    #[test]
    fn food_crisis_tags_feed_calibration_metrics() {
        let mut snapshot = generated_snapshot();
        snapshot.events.push(WorldEvent {
            day: 1,
            tick: 10,
            actor: 1,
            target: None,
            kind: EventKind::Scarcity,
            summary: "Lar entra em crise alimentar sem fornecedor material".to_string(),
            impact_tags: vec![
                "crise_alimentar".to_string(),
                "sem_fornecedor_material".to_string(),
                "processador_parado".to_string(),
                "acesso_alimentar_fragil".to_string(),
            ],
        });
        snapshot.events.push(WorldEvent {
            day: 1,
            tick: 12,
            actor: 2,
            target: None,
            kind: EventKind::Scarcity,
            summary: "Senhor preserva acesso alimentar privilegiado".to_string(),
            impact_tags: vec![
                "crise_alimentar".to_string(),
                "vantagem_feudal_alimentar".to_string(),
            ],
        });

        let report = analyze_snapshot(&snapshot, &DramaturgyCalibrationConfig::default());

        assert_eq!(
            report.food_system_metrics.get("dias_com_crise_alimentar"),
            Some(&1)
        );
        assert_eq!(
            report.food_system_metrics.get("sem_fornecedor_material"),
            Some(&1)
        );
        assert_eq!(
            report.food_system_metrics.get("injustica_alimentar_feudal"),
            Some(&1)
        );
    }

    #[test]
    fn report_is_json_serializable() {
        let snapshot = generated_snapshot();
        let report = analyze_snapshot(&snapshot, &DramaturgyCalibrationConfig::default());
        let json = serde_json::to_string(&report).expect("report should serialize");

        assert!(json.contains("global_score"));
        assert!(json.contains("food_system_metrics"));
        assert!(report.global_score >= 0 && report.global_score <= 100);
    }
}
