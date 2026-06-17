use super::*;
use crate::agent_mind::{ConversationTurnInput, ConversationTurnOutput, DecisionInput};
use crate::llm_adapter::LlmError;
use crate::world_model::{
    AgentIntent, AgentLifeStatus, AgentMemory, AgentProfile, AgentRelation, AgentState, BuildingId,
    ConversationId, PsychologicalState, RoomId, SimplifiedTask, TileCoord, TraumaTracker,
};
use std::collections::{HashMap, VecDeque};

#[allow(dead_code)]
#[derive(Clone)]
pub(super) struct AgentContext {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) role_id: String,
    pub(super) position: TileCoord,
    pub(super) state: AgentState,
    pub(super) life_status: AgentLifeStatus,
    pub(super) profile: AgentProfile,
    pub(super) relations: HashMap<u64, AgentRelation>,
    pub(super) memories: Vec<AgentMemory>,
    pub(super) destination_label: Option<String>,
    pub(super) current_building_id: Option<BuildingId>,
    pub(super) current_room_id: Option<RoomId>,
    pub(super) last_intent: Option<AgentIntent>,
    pub(super) llm_calls: u64,
    pub(super) blocked_ticks: u32,
    pub(super) active_conversation_id: Option<ConversationId>,
    pub(super) social_cooldown_until: u64,
    pub(super) household_id: Option<BuildingId>,
    pub(super) task_queue: VecDeque<SimplifiedTask>,
    pub(super) trauma_tracker: TraumaTracker,
    pub(super) psychological_state: PsychologicalState,
}

impl AgentContext {
    pub(super) fn profile_summary(&self) -> Vec<String> {
        let mut summary = self.profile.values.clone();
        summary.extend(self.profile.long_term_desires.clone());
        summary.extend(self.profile.fears.clone());
        summary
    }
}

pub(super) struct PreparedDecisionRequest {
    pub(super) agent_id: u64,
    pub(super) nearby_ids: Vec<u64>,
    pub(super) cognition_trigger: String,
    pub(super) social_opportunity_signature: Option<String>,
    pub(super) input: DecisionInput,
}

pub(super) struct PreparedConversationTurn {
    pub(super) conversation_id: ConversationId,
    pub(super) speaker_id: u64,
    pub(super) listener_id: u64,
    pub(super) input: ConversationTurnInput,
}

pub(super) struct CompletedConversationTurn {
    pub(super) conversation_id: ConversationId,
    pub(super) speaker_id: u64,
    pub(super) listener_id: u64,
    pub(super) output: ConversationTurnOutput,
}

pub(super) struct InterruptedConversationTurn {
    pub(super) conversation_id: ConversationId,
    pub(super) speaker_id: u64,
    pub(super) listener_id: u64,
    pub(super) error: LlmError,
}

pub(super) enum ConversationBatchItem {
    Completed(CompletedConversationTurn),
    Interrupted(InterruptedConversationTurn),
}

impl ConversationBatchItem {
    pub(super) fn conversation_id(&self) -> ConversationId {
        match self {
            Self::Completed(result) => result.conversation_id,
            Self::Interrupted(result) => result.conversation_id,
        }
    }
}

impl Simulation {
    pub(super) fn agent_ids(&mut self) -> Vec<u64> {
        let mut query = self.world.query::<&AgentCore>();
        query.iter(&self.world).map(|core| core.id).collect()
    }

    pub(super) fn collect_contexts(&mut self) -> Vec<AgentContext> {
        let mut query = self.world.query::<(
            &AgentCore,
            &ProfileComponent,
            &StateComponent,
            &LifeStatusComponent,
            &RelationComponent,
            &MemoryComponent,
            &PositionComponent,
            &DestinationLabelComponent,
            &IntentComponent,
            &DecisionBudgetComponent,
            &CognitionComponent,
            &ConversationComponent,
            &TaskQueueComponent,
            Option<&TraumaTrackerComponent>,
            Option<&PsychologicalStateComponent>,
        )>();
        query
            .iter(&self.world)
            .map(
                |(
                    core,
                    profile,
                    state,
                    life_status,
                    relations,
                    memories,
                    position,
                    destination_label,
                    intent,
                    budget,
                    cognition,
                    conversation,
                    task_queue,
                    trauma_tracker,
                    psychological_state,
                )| {
                    let tile = self.tile_at(position.0);
                    AgentContext {
                        id: core.id,
                        name: core.name.clone(),
                        role_id: core.role_id.clone(),
                        position: position.0,
                        state: state.0.clone(),
                        life_status: life_status.0,
                        profile: profile.0.clone(),
                        relations: relations.0.clone(),
                        memories: memories.0.clone(),
                        destination_label: destination_label.0.clone(),
                        current_building_id: tile.and_then(|entry| entry.building_id),
                        current_room_id: tile.and_then(|entry| entry.room_id),
                        last_intent: intent.0.clone(),
                        llm_calls: budget.llm_calls,
                        blocked_ticks: cognition.blocked_ticks,
                        active_conversation_id: conversation.active_conversation_id,
                        social_cooldown_until: conversation.social_cooldown_until,
                        household_id: core.home_building_id,
                        task_queue: task_queue.0.clone(),
                        trauma_tracker: trauma_tracker.map(|t| t.0.clone()).unwrap_or_default(),
                        psychological_state: psychological_state
                            .map(|p| p.0.clone())
                            .unwrap_or_default(),
                    }
                },
            )
            .collect()
    }

    pub(super) fn assign_intent(
        &mut self,
        agent_id: u64,
        intent: AgentIntent,
        reflection: String,
    ) -> Result<()> {
        let normalized_horizon = self.normalized_reconsideration_horizon(intent.kind);
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let current_state = entity_mut
            .get::<StateComponent>()
            .ok_or_else(|| anyhow!("missing state component"))?
            .0
            .clone();
        entity_mut
            .get_mut::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0 = Some(intent.clone());
        entity_mut
            .get_mut::<ThoughtComponent>()
            .ok_or_else(|| anyhow!("missing thought component"))?
            .0 = reflection.clone();
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = intent.target_semantic.clone();
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        {
            let mut budget = entity_mut
                .get_mut::<DecisionBudgetComponent>()
                .ok_or_else(|| anyhow!("missing budget component"))?;
            budget.cooldown_until = self.total_ticks + normalized_horizon;
            budget.llm_calls += 1;
        }
        {
            let mut cognition = entity_mut
                .get_mut::<CognitionComponent>()
                .ok_or_else(|| anyhow!("missing cognition component"))?;
            cognition.next_reconsideration_tick = self.total_ticks + normalized_horizon;
            cognition.blocked_ticks = 0;
            cognition.last_cognition_trigger = Some("novo_plano".to_string());
            cognition.last_deliberation_hunger = current_state.hunger;
            cognition.last_deliberation_energy = current_state.energy;
            cognition.last_deliberation_health = current_state.health;
            cognition.last_deliberation_stress = current_state.stress;
        }
        {
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.current_focus = intent.kind.as_str().to_string();
            for belief in &intent.belief_updates {
                if !state.0.active_goals.iter().any(|goal| goal == belief) {
                    state.0.active_goals.push(belief.clone());
                }
            }
            if state.0.active_goals.len() > 4 {
                state.0.active_goals.truncate(4);
            }
        }
        drop(entity_mut);
        if matches!(
            intent.kind,
            IntentKind::Comprar
                | IntentKind::Transportar
                | IntentKind::Vender
                | IntentKind::ReceberPagamento
                | IntentKind::Construir
                | IntentKind::Trabalhar
        ) {
            self.bind_or_create_economic_task(agent_id, &intent)?;
        } else {
            self.clear_active_economic_task(agent_id)?;
        }
        self.add_memory(
            agent_id,
            MemoryKind::Reflection,
            format!("Reflexao: {}", reflection),
            intent.belief_updates.clone(),
            12,
            intent.target_agent.into_iter().collect(),
        )?;
        Ok(())
    }

    pub(super) fn apply_think_maker_output(
        &mut self,
        agent_id: u64,
        output: ThinkMakerOutput,
    ) -> Result<()> {
        // 1. Update ThoughtComponent (reflection)
        self.set_thought(agent_id, output.reflection.clone())?;

        // 2. Find agent entity and update active goals (belief_updates) in StateComponent
        // Also update the active intent's dominant_emotion and justification
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
            for belief in &output.belief_updates {
                if !state.0.active_goals.iter().any(|goal| goal == belief) {
                    state.0.active_goals.push(belief.clone());
                }
            }
            if state.0.active_goals.len() > 4 {
                state.0.active_goals.truncate(4);
            }
        }

        if let Some(mut intent_comp) = entity_mut.get_mut::<IntentComponent>() {
            if let Some(ref mut intent) = intent_comp.0 {
                intent.dominant_emotion = output.dominant_emotion.clone();
                intent.justification = output.reflection.clone();
                intent.belief_updates = output.belief_updates.clone();
            }
        }
        drop(entity_mut);

        // 3. Add reflection memory
        self.add_memory(
            agent_id,
            MemoryKind::Reflection,
            format!("Reflexao: {}", output.reflection),
            output.belief_updates,
            12,
            Vec::new(),
        )?;

        Ok(())
    }

    pub(super) fn process_general_decisions(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        // 1. Process completed background thoughts
        let mut completed_results = Vec::new();
        let mut skipped_results = Vec::new();

        let mut active_thoughts = Vec::new();
        for pending in self.pending_thoughts.drain(..) {
            if pending.handle.is_finished() {
                match pending.handle.join() {
                    Ok(ThinkMakerResult::Completed(res)) => {
                        completed_results.push(res);
                    }
                    Ok(ThinkMakerResult::Skipped(res)) => {
                        skipped_results.push(res);
                    }
                    Err(_) => {
                        return Err(anyhow!(
                            "Background Think Maker thread panicked for agent {}",
                            pending.agent_id
                        ));
                    }
                }
            } else {
                active_thoughts.push(pending);
            }
        }
        self.pending_thoughts = active_thoughts;

        for result in completed_results {
            self.apply_think_maker_output(result.agent_id, result.output)?;
        }

        for result in skipped_results {
            if !result.error.is_transient() {
                if let Some(entity) = self.find_agent_entity(result.agent_id).ok() {
                    if let Some(mut budget) = self
                        .world
                        .entity_mut(entity)
                        .get_mut::<DecisionBudgetComponent>()
                    {
                        budget.cooldown_until = self.total_ticks + 60;
                    }
                }
                eprintln!(
                    "Persistent Think Maker failure for agent {}: {}. Put on 60-tick cooldown.",
                    result.agent_id, result.error
                );
            }
        }

        // 2. Synchronous Action Planning
        let requests = self.prepare_decision_requests()?;

        use rayon::prelude::*;
        let planner_results = requests
            .into_par_iter()
            .map(|request| {
                let res = llm.plan_actions(&request.input);
                (request, res)
            })
            .collect::<Vec<_>>();

        for (request, plan_res) in planner_results {
            let agent_id = request.agent_id;
            let input = request.input;

            self.pending_thoughts
                .retain(|pending| pending.agent_id != agent_id);

            let raw_plan = match plan_res {
                Ok(plan) => plan,
                Err(error) => {
                    if !error.is_transient() {
                        if let Some(entity) = self.find_agent_entity(agent_id).ok() {
                            if let Some(mut budget) = self
                                .world
                                .entity_mut(entity)
                                .get_mut::<DecisionBudgetComponent>()
                            {
                                budget.cooldown_until = self.total_ticks + 60;
                            }
                        }
                        eprintln!(
                            "Persistent Action Planner failure for agent {}: {}. Put on 60-tick cooldown.",
                            agent_id, error
                        );
                    }
                    self.handle_transient_decision_failure(
                        agent_id,
                        &request.cognition_trigger,
                        request.social_opportunity_signature,
                        &error,
                    )?;
                    continue;
                }
            };

            let tasks = parse_action_planner_output(&raw_plan);
            if let Some(invalid_task) = tasks
                .iter()
                .find(|task| self.llm_task_has_invalid_physical_place(task))
            {
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: invalid_task.target_agent,
                    kind: EventKind::CognitionFailure,
                    summary: format!(
                        "Plano LLM rejeitado: {:?} usou lugar livre invalido {:?}; use place_id de world_places.",
                        invalid_task.kind, invalid_task.target_semantic
                    ),
                    impact_tags: vec![
                        "contrato_llm".to_string(),
                        "place_id_obrigatorio".to_string(),
                    ],
                });
                continue;
            }

            let first_task = tasks.first().cloned();
            if let Some(task) = first_task {
                let intent = AgentIntent {
                    kind: task.kind,
                    target_agent: task.target_agent,
                    target_semantic: task.target_semantic.clone(),
                    justification: "Planejamento instintivo".to_string(),
                    dominant_emotion: "contido".to_string(),
                    perceived_risk: 0,
                    belief_updates: Vec::new(),
                    priority: 1,
                    social_move: task.social_move,
                };
                let validated = validate_intent(intent, &request.nearby_ids);

                let entity = self.find_agent_entity(agent_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                let mut queue = entity_mut
                    .get_mut::<TaskQueueComponent>()
                    .ok_or_else(|| anyhow!("missing task queue component"))?;
                queue.0.clear();
                for t in tasks.iter().skip(1) {
                    queue.0.push_back(t.clone());
                }
                drop(queue);
                drop(entity_mut);

                self.assign_intent(agent_id, validated, "Pensando...".to_string())?;
            } else {
                let entity = self.find_agent_entity(agent_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<TaskQueueComponent>()
                    .ok_or_else(|| anyhow!("missing task queue component"))?
                    .0
                    .clear();
            }

            self.record_cognition_trigger(agent_id, &request.cognition_trigger)?;
            self.record_social_opportunity_signature(
                agent_id,
                request.social_opportunity_signature.clone(),
            )?;

            // 3. Spawn Think Maker in background
            let think_input = ThinkMakerInput {
                decision_input: input,
                planned_tasks: tasks,
            };
            let worker_llm = llm.clone_box();
            let handle =
                std::thread::spawn(move || match worker_llm.generate_thoughts(&think_input) {
                    Ok(output) => {
                        ThinkMakerResult::Completed(CompletedThoughts { agent_id, output })
                    }
                    Err(error) => ThinkMakerResult::Skipped(SkippedThoughts { agent_id, error }),
                });

            self.pending_thoughts
                .push(PendingThoughts { agent_id, handle });
        }

        Ok(())
    }

    fn llm_task_has_invalid_physical_place(&self, task: &SimplifiedTask) -> bool {
        let requires_place = matches!(
            task.kind,
            IntentKind::Andar
                | IntentKind::Trabalhar
                | IntentKind::Comer
                | IntentKind::Descansar
                | IntentKind::Refletir
        );
        if !requires_place {
            return false;
        }
        let Some(target) = task.target_semantic.as_deref() else {
            return false;
        };
        !Self::looks_like_place_id(target) || self.place_by_id(target).is_none()
    }

    pub(super) fn prepare_decision_requests(&mut self) -> Result<Vec<PreparedDecisionRequest>> {
        let contexts = self.collect_contexts();
        let mut requests = Vec::new();

        for context in contexts {
            if context.life_status != AgentLifeStatus::Vivo {
                continue;
            }
            if context.role_id == "caravana" || context.role_id == "guarda_caravana" {
                continue;
            }
            let in_active_faction = self
                .political_factions
                .iter()
                .any(|f| f.is_action_active && f.member_ids.contains(&context.id));
            if in_active_faction {
                continue;
            }
            if context.active_conversation_id.is_some() {
                continue;
            }
            if self.should_hold_locked_economic_task(&context) {
                continue;
            }
            if context.social_cooldown_until > self.total_ticks
                && matches!(
                    context.last_intent.as_ref().map(|intent| intent.kind),
                    Some(IntentKind::Socializar)
                )
            {
                continue;
            }

            let recent_events = self.recent_events_for(
                context.id,
                context.position,
                self.recent_event_limit.max(8),
            );
            let social_opportunity_signature = self.social_opportunity_signature(&context);
            let Some(cognition_trigger) =
                self.decision_trigger_for_context(&context, &recent_events)?
            else {
                continue;
            };

            let trigger_overrides_cooldown = matches!(
                cognition_trigger.as_str(),
                "necessidade_critica"
                    | "bloqueio_repetido"
                    | "evento_social_direto"
                    | "falha_tarefa_economica"
                    | "sem_intencao"
            );

            if let Some(entity) = self.find_agent_entity(context.id).ok()
                && let Some(budget) = self.world.entity(entity).get::<DecisionBudgetComponent>()
                && budget.cooldown_until > self.total_ticks
                && !trigger_overrides_cooldown
            {
                continue;
            }

            let context_depth = self
                .context_depth_for_trigger(&cognition_trigger)
                .to_string();
            let (memory_limit, fixture_limit, agent_limit, event_limit) =
                self.context_limits_for_trigger(&cognition_trigger);
            let relevant_memories = retrieve_relevant_memories(
                &context.memories,
                &context.state,
                &recent_events,
                memory_limit,
            );
            let mut nearby_agents = self.nearby_agent_inputs(
                context.id,
                context.position,
                context.current_room_id,
                &context.relations,
            );
            nearby_agents.truncate(agent_limit);
            let nearby_ids = nearby_agents.iter().map(|item| item.id).collect::<Vec<_>>();
            let mut nearby_fixtures = self.nearby_fixture_inputs(context.position, 6);
            nearby_fixtures.truncate(fixture_limit);
            let psychological_context = self.build_psychological_context(
                &context,
                &recent_events,
                &relevant_memories,
                &cognition_trigger,
            );
            let economic_context = self.build_economic_context(&context);
            let legal_context = self.build_legal_context(&context);
            let political_context = self.build_political_context(&context);
            let institutional_context = self.build_institutional_context(context.id);
            let feudal_context = self.build_feudal_context(context.id);
            let information_context = self.build_information_context(context.id, None);
            let cultural_context = self.build_cultural_context(context.id, None);
            let time_context = self.time_context();
            let world_places = self.world_place_inputs();
            let input = DecisionInput {
                actor_id: context.id,
                actor_name: context.name.clone(),
                role: self.role_display_name(&context.role_id),
                day: self.day,
                tick: self.tick_of_day,
                time_context,
                world_places,
                current_area: self.area_name(context.position),
                current_building: context
                    .current_building_id
                    .and_then(|id| self.building_name(id)),
                current_building_kind: context
                    .current_building_id
                    .and_then(|id| self.building_kind(id).map(|kind| kind.as_str().to_string())),
                current_room: context.current_room_id.and_then(|id| self.room_name(id)),
                accessible_exits: self.accessible_exits(context.position),
                nearby_fixtures,
                nearby_agents,
                relevant_memories,
                recent_events: recent_events
                    .into_iter()
                    .take(event_limit)
                    .map(|event| RecentEventInput {
                        day: event.day,
                        tick: event.tick,
                        kind: event.kind,
                        summary: event.summary,
                    })
                    .collect(),
                current_goals: context.state.active_goals.clone(),
                known_destination: context.destination_label.clone(),
                blockers: self.local_blockers(context.position),
                state: context.state.clone(),
                cognition_trigger: cognition_trigger.clone(),
                context_depth,
                psychological_context,
                economic_context,
                legal_context,
                political_context,
                institutional_context,
                feudal_context,
                information_context,
                cultural_context,
                profile_summary: context.profile_summary(),
                llm_budget_remaining: 24u32.saturating_sub(context.llm_calls as u32),
                chaos_pressure: self.agent_chaos_pressure(context.id).unwrap_or(0),
                personality_traits: context.profile.traits.clone(),
                trauma_traits: context.profile.trauma_traits.clone(),
            };
            requests.push(PreparedDecisionRequest {
                agent_id: context.id,
                nearby_ids,
                cognition_trigger,
                social_opportunity_signature,
                input,
            });
        }

        requests.sort_by_key(|request| request.agent_id);
        Ok(requests)
    }

    pub(super) fn run_parallel_conversation_turns(
        &self,
        llm: &dyn LlmAdapter,
        turns: Vec<PreparedConversationTurn>,
    ) -> Result<Vec<ConversationBatchItem>> {
        let mut results = Vec::with_capacity(turns.len());
        for turn in turns {
            let conversation_id = turn.conversation_id;
            match llm.generate_conversation_turn(&turn.input) {
                Ok(output) => {
                    results.push(ConversationBatchItem::Completed(
                        CompletedConversationTurn {
                            conversation_id,
                            speaker_id: turn.speaker_id,
                            listener_id: turn.listener_id,
                            output,
                        },
                    ));
                }
                Err(error) => {
                    if error.is_transient() {
                        results.push(ConversationBatchItem::Interrupted(
                            InterruptedConversationTurn {
                                conversation_id,
                                speaker_id: turn.speaker_id,
                                listener_id: turn.listener_id,
                                error,
                            },
                        ));
                    } else {
                        return Err(anyhow!(
                            "conversation {} failed: {}",
                            conversation_id,
                            error
                        ));
                    }
                }
            }
        }
        results.sort_by_key(ConversationBatchItem::conversation_id);
        Ok(results)
    }

    pub(super) fn handle_transient_decision_failure(
        &mut self,
        agent_id: u64,
        cognition_trigger: &str,
        social_opportunity_signature: Option<String>,
        error: &LlmError,
    ) -> Result<()> {
        let agent_name = self.agent_name(agent_id)?;
        self.record_cognition_trigger(agent_id, cognition_trigger)?;
        self.record_social_opportunity_signature(agent_id, social_opportunity_signature)?;
        self.set_thought(
            agent_id,
            "Uma falha transitÃ³ria atrapalhou meu raciocÃ­nio neste momento.".to_string(),
        )?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::CognitionFailure,
            summary: format!(
                "{} perde a deliberacao deste tick por falha transitÃ³ria do provider: {}.",
                agent_name, error
            ),
            impact_tags: vec![
                "llm".to_string(),
                "timeout".to_string(),
                "cognicao".to_string(),
            ],
        });
        Ok(())
    }

    pub(super) fn handle_transient_conversation_failure(
        &mut self,
        conversation_id: ConversationId,
        speaker_id: u64,
        listener_id: u64,
        error: &LlmError,
    ) -> Result<()> {
        let speaker_name = self.agent_name(speaker_id)?;
        let listener_name = self.agent_name(listener_id)?;
        self.end_conversation(
            conversation_id,
            ConversationStatus::Interrupted,
            ConversationOutcome::ProviderTimeout,
            format!(
                "timeout_llm: {} nao conseguiu responder a {} por falha transitÃ³ria do provider ({})",
                speaker_name, listener_name, error
            ),
        )
    }

    pub(super) fn decision_trigger_for_context(
        &mut self,
        context: &AgentContext,
        recent_events: &[WorldEvent],
    ) -> Result<Option<String>> {
        if context.last_intent.is_none() && context.task_queue.is_empty() {
            return Ok(Some("sem_intencao".to_string()));
        }

        if context.blocked_ticks >= BLOCKED_RECONSIDERATION_TICKS {
            return Ok(Some("bloqueio_repetido".to_string()));
        }

        if self.has_critical_need(context) {
            return Ok(Some("necessidade_critica".to_string()));
        }

        if self.has_direct_social_event(context.id, recent_events) {
            return Ok(Some("evento_social_direto".to_string()));
        }

        // Check if an active economic task has failed
        if let Some(task) = self.active_economic_task_for_agent(context.id) {
            if task.phase == EconomicTaskPhase::Failed {
                return Ok(Some("falha_tarefa_economica".to_string()));
            }
        }

        Ok(None)
    }

    pub(super) fn context_depth_for_trigger(&self, trigger: &str) -> &'static str {
        match trigger {
            "evento_social_direto"
            | "bloqueio_repetido"
            | "necessidade_critica"
            | "falha_tarefa_economica" => "expanded",
            _ => "normal",
        }
    }

    pub(super) fn context_limits_for_trigger(&self, trigger: &str) -> (usize, usize, usize, usize) {
        match self.context_depth_for_trigger(trigger) {
            "expanded" => (self.relevant_memory_limit, 4, 4, 5),
            _ => (3, 3, 3, 3),
        }
    }

    pub(super) fn has_critical_need(&self, context: &AgentContext) -> bool {
        context.state.hunger >= 70 || context.state.energy <= 20 || context.state.stress >= 72
    }

    pub(super) fn has_direct_social_event(
        &self,
        agent_id: u64,
        recent_events: &[WorldEvent],
    ) -> bool {
        recent_events.iter().rev().take(4).any(|event| {
            (event.actor == agent_id || event.target == Some(agent_id))
                && matches!(
                    event.kind,
                    EventKind::Conflict
                        | EventKind::SocialBond
                        | EventKind::ConversationStarted
                        | EventKind::ConversationEnded
                )
        })
    }

    pub(super) fn social_opportunity_signature(
        &mut self,
        context: &AgentContext,
    ) -> Option<String> {
        let mut nearby = context
            .relations
            .iter()
            .filter_map(|(other_id, relation)| {
                let distance = self.agent_distance_from(context.position, *other_id)?;
                if distance != 1 {
                    return None;
                }
                if relation.friendship >= 25 || relation.trust >= 25 {
                    return Some(format!("amigo:{other_id}"));
                }
                if relation.resentment >= 20 {
                    return Some(format!("rival:{other_id}"));
                }
                None
            })
            .collect::<Vec<_>>();
        nearby.sort();
        nearby.into_iter().next()
    }

    pub(super) fn build_psychological_context(
        &self,
        context: &AgentContext,
        recent_events: &[WorldEvent],
        relevant_memories: &[crate::agent_mind::RelevantMemoryInput],
        trigger: &str,
    ) -> PsychologicalContextInput {
        self.build_psychological_context_for_values(
            context.id,
            &context.profile,
            &context.state,
            &context.memories,
            recent_events,
            relevant_memories,
            trigger,
            &context.psychological_state,
        )
    }

    pub(super) fn build_psychological_context_for_values(
        &self,
        _agent_id: u64,
        profile: &AgentProfile,
        state: &AgentState,
        memories: &[AgentMemory],
        recent_events: &[WorldEvent],
        relevant_memories: &[crate::agent_mind::RelevantMemoryInput],
        trigger: &str,
        psychological_state: &PsychologicalState,
    ) -> PsychologicalContextInput {
        PsychologicalContextInput {
            core_values: profile.values.iter().take(3).cloned().collect(),
            long_term_desires: profile.long_term_desires.iter().take(3).cloned().collect(),
            fears: profile.fears.iter().take(3).cloned().collect(),
            social_style: profile.social_style.clone(),
            moral_tolerances: profile.moral_tolerances.iter().take(3).cloned().collect(),
            inner_conflicts: self.derive_inner_conflicts(
                profile,
                state,
                relevant_memories,
                psychological_state,
            ),
            current_identity_tension: self.current_identity_tension(
                profile,
                state,
                trigger,
                psychological_state,
            ),
            dominant_preoccupations: self.dominant_preoccupations(
                state,
                recent_events,
                psychological_state,
            ),
            recent_self_narrative: self.recent_self_narrative(
                memories,
                recent_events,
                psychological_state,
            ),
        }
    }

    pub(super) fn build_relational_history(
        &self,
        speaker_id: u64,
        listener_id: u64,
        relation: &AgentRelation,
        speaker_memories: &[AgentMemory],
    ) -> RelationalHistoryInput {
        let mut shared_memories = speaker_memories
            .iter()
            .filter(|memory| memory.about.contains(&listener_id))
            .collect::<Vec<_>>();
        shared_memories.sort_by_key(|memory| {
            -((memory.emotional_weight * 1000) + memory.day as i32 * 10 + memory.tick as i32)
        });

        let shared_history = shared_memories
            .iter()
            .take(5)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        let mut open_promises = shared_memories
            .iter()
            .filter(|memory| {
                memory.kind == MemoryKind::Promise
                    || memory.tags.iter().any(|tag| tag.contains("prom"))
            })
            .take(3)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        let live_promises = self
            .promises
            .iter()
            .filter(|promise| {
                promise.promiser_id == speaker_id && promise.promisee_id == listener_id
            })
            .map(|promise| match &promise.condition {
                PromiseCondition::DeliverResource {
                    resource_id,
                    amount,
                } => format!(
                    "Promessa ativa: entregar {} x{} ate tick {}.",
                    self.resource_display_name(resource_id),
                    amount,
                    promise.deadline_tick
                ),
                PromiseCondition::VoteForPolicy { domain, value } => format!(
                    "Promessa ativa: apoiar pauta {domain} -> {value} ate tick {}.",
                    promise.deadline_tick
                ),
                PromiseCondition::KeepSecret { secret_id } => format!(
                    "Promessa ativa: guardar segredo #{} ate tick {}.",
                    secret_id, promise.deadline_tick
                ),
            })
            .collect::<Vec<_>>();
        open_promises.extend(live_promises);
        if self.has_recent_event(240, |event| {
            event.kind == EventKind::SocialBond
                && event.actor == speaker_id
                && event.target == Some(listener_id)
                && event.summary.contains("quebrou a promessa")
        }) {
            open_promises
                .push("Promessa quebrada recente ainda contamina a confianca.".to_string());
        }
        open_promises.truncate(4);
        let unresolved_offenses = shared_memories
            .iter()
            .filter(|memory| {
                memory.kind == MemoryKind::Offense
                    || memory.tags.iter().any(|tag| tag.contains("ofens"))
            })
            .take(3)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        let recent_favors = shared_memories
            .iter()
            .filter(|memory| {
                memory.tags.iter().any(|tag| {
                    tag.contains("favor") || tag.contains("ajuda") || tag.contains("divida")
                })
            })
            .take(3)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();

        RelationalHistoryInput {
            relationship_summary: format!(
                "Entre {} e {}: confianca {}, amizade {}, ressentimento {}, divida moral {}.",
                speaker_id,
                listener_id,
                relation.trust,
                relation.friendship,
                relation.resentment,
                relation.moral_debt
            ),
            shared_history,
            open_promises,
            unresolved_offenses,
            recent_favors,
            trust_trajectory: if relation.trust >= 25 {
                "confianca em alta".to_string()
            } else if relation.trust <= -10 {
                "confianca abalada".to_string()
            } else {
                "confianca oscilante".to_string()
            },
            resentment_trajectory: if relation.resentment >= 25 {
                "ressentimento acumulado".to_string()
            } else if relation.resentment <= 5 {
                "ressentimento baixo".to_string()
            } else {
                "ressentimento latente".to_string()
            },
            social_imbalance: if relation.moral_debt > 10 {
                "o falante sente credito moral".to_string()
            } else if relation.moral_debt < -10 {
                "o falante sente dever ao outro".to_string()
            } else {
                "a relacao parece relativamente equilibrada".to_string()
            },
        }
    }

    pub(super) fn derive_inner_conflicts(
        &self,
        profile: &AgentProfile,
        state: &AgentState,
        relevant_memories: &[crate::agent_mind::RelevantMemoryInput],
        psychological_state: &PsychologicalState,
    ) -> Vec<String> {
        let mut conflicts = Vec::new();
        if state.hunger >= 60 && !profile.values.is_empty() {
            conflicts.push(format!(
                "{} disputa espaco com a fome imediata.",
                profile.values[0]
            ));
        }
        if state.stress >= 55 && !profile.fears.is_empty() {
            conflicts.push(format!(
                "O medo de {} pressiona a necessidade de agir.",
                profile.fears[0]
            ));
        }
        if state.energy <= 25 && !state.active_goals.is_empty() {
            conflicts.push(format!(
                "O corpo pede pausa, mas {} continua urgente.",
                state.active_goals[0]
            ));
        }
        if psychological_state.grief >= 35 {
            conflicts.push("O luto disputa espaco com qualquer rotina normal.".to_string());
        }
        if psychological_state.humiliation >= 35 && !profile.values.is_empty() {
            conflicts.push(format!(
                "A humilhacao fere {} e pede reparacao.",
                profile.values[0]
            ));
        }
        if psychological_state.fear >= 45 && psychological_state.anger >= 25 {
            conflicts
                .push("Medo e raiva puxam em direcoes opostas: fugir ou retaliar.".to_string());
        }
        if psychological_state.guilt >= 30 {
            conflicts.push("A culpa pede reparacao, segredo ou justificativa moral.".to_string());
        }
        if conflicts.is_empty() && !relevant_memories.is_empty() {
            conflicts.push(format!(
                "As lembrancas de {} continuam pesando.",
                relevant_memories[0].summary
            ));
        }
        conflicts.truncate(3);
        conflicts
    }

    pub(super) fn current_identity_tension(
        &self,
        profile: &AgentProfile,
        state: &AgentState,
        trigger: &str,
        psychological_state: &PsychologicalState,
    ) -> String {
        if psychological_state.trauma >= 55 {
            return format!(
                "{} tenta parecer funcional, mas o trauma ainda organiza suas escolhas.",
                state.current_focus
            );
        }
        if psychological_state.pride >= 45 && psychological_state.humiliation >= 30 {
            return "Orgulho e humilhacao competem por uma resposta publica.".to_string();
        }
        if trigger.contains("social") && !profile.values.is_empty() && !profile.fears.is_empty() {
            return format!(
                "{} tenta proteger {} sem ativar {}.",
                state.current_focus, profile.values[0], profile.fears[0]
            );
        }
        if state.stress >= 60 {
            return format!(
                "{} luta para manter o autocontrole sob stress.",
                state.current_focus
            );
        }
        format!(
            "{} tenta alinhar rotina, reputacao e desejo de longo prazo.",
            state.current_focus
        )
    }

    pub(super) fn dominant_preoccupations(
        &self,
        state: &AgentState,
        recent_events: &[WorldEvent],
        psychological_state: &PsychologicalState,
    ) -> Vec<String> {
        let mut concerns = Vec::new();
        concerns.extend(state.active_goals.iter().take(2).cloned());
        if state.hunger >= 60 {
            concerns.push("fome crescente".to_string());
        }
        if state.energy <= 30 {
            concerns.push("fadiga".to_string());
        }
        if state.stress >= 55 {
            concerns.push("stress alto".to_string());
        }
        if psychological_state.grief >= 25 {
            concerns.push(format!("luto persistente ({})", psychological_state.grief));
        }
        if psychological_state.humiliation >= 25 {
            concerns.push(format!(
                "humilhacao acumulada ({})",
                psychological_state.humiliation
            ));
        }
        if psychological_state.fear >= 30 {
            concerns.push(format!("medo persistente ({})", psychological_state.fear));
        }
        if psychological_state.pride >= 30 {
            concerns.push(format!(
                "orgulho ferido ou elevado ({})",
                psychological_state.pride
            ));
        }
        if psychological_state.trauma >= 30 {
            concerns.push(format!("trauma ativo ({})", psychological_state.trauma));
        }
        concerns.extend(
            recent_events
                .iter()
                .take(2)
                .map(|event| event.summary.clone()),
        );
        concerns.truncate(4);
        concerns
    }

    pub(super) fn recent_self_narrative(
        &self,
        memories: &[AgentMemory],
        recent_events: &[WorldEvent],
        psychological_state: &PsychologicalState,
    ) -> String {
        let mut pieces = memories
            .iter()
            .rev()
            .take(2)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        pieces.extend(
            recent_events
                .iter()
                .take(2)
                .map(|event| event.summary.clone()),
        );
        if psychological_state.grief
            + psychological_state.humiliation
            + psychological_state.fear
            + psychological_state.pride
            + psychological_state.trauma
            > 0
        {
            pieces.push(format!(
                "Estado interno atual: {}",
                psychological_state.summary()
            ));
        }
        if pieces.is_empty() {
            "Nada recente reorganizou a mente do agente.".to_string()
        } else {
            pieces.join(" | ")
        }
    }

    pub(super) fn build_economic_context(&self, context: &AgentContext) -> EconomicContextInput {
        let household = context
            .household_id
            .and_then(|household_id| self.household_by_id(household_id));
        let pantry = household
            .map(|household| {
                household
                    .pantry
                    .iter()
                    .map(|stack| {
                        format!(
                            "{} x{}",
                            self.resource_display_name(&stack.resource_id),
                            stack.amount
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let reserved_food = household
            .map(|household| {
                household
                    .reserved_food
                    .iter()
                    .map(|stack| {
                        format!(
                            "{} x{}",
                            self.resource_display_name(&stack.resource_id),
                            stack.amount
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let pending_salary = household
            .map(|household| {
                household
                    .pending_payments
                    .iter()
                    .map(|claim| claim.amount)
                    .sum()
            })
            .unwrap_or(0);
        let tax_pressure = household
            .map(|household| self.village_economy.daily_household_tax + household.tax_arrears)
            .unwrap_or(0);
        let mut work_obligations = self.work_obligations_for_context(context);
        let war_supply_status = self
            .military_demands
            .iter()
            .filter(|demand| {
                matches!(
                    demand.status,
                    MilitaryDemandStatus::Open | MilitaryDemandStatus::PartiallySupplied
                )
            })
            .take(5)
            .map(|demand| {
                let missing = Self::missing_military_resources_for_demand(demand)
                    .into_iter()
                    .map(|stack| {
                        format!(
                            "{} x{}",
                            self.resource_display_name(&stack.resource_id),
                            stack.amount
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let cash_missing = (demand.cash_required - demand.cash_delivered).max(0);
                format!(
                    "guerra #{} {:?}: faltam [{}], caixa={} moedas, prazo dia {}",
                    demand.war_id, demand.stage, missing, cash_missing, demand.deadline_day
                )
            })
            .collect::<Vec<_>>();
        for status in &war_supply_status {
            work_obligations.push(format!("SUPRIMENTO MILITAR: {status}"));
        }

        // Se o agente conspirou sobre o mercado negro, injetamos as affordances rebeldes como opções
        let has_mercado_negro = context
            .memories
            .iter()
            .any(|m| m.tags.contains(&"mercado_negro".to_string()));
        if has_mercado_negro {
            for act in self
                .policy_acts
                .iter()
                .filter(|act| self.policy_act_is_active(act))
            {
                if matches!(act.authority, PolicyAuthority::LocalLeader) {
                    let edital = &act.agenda_tag;
                    let agente_sabe = context.role_id == "lider_local"
                        || context.role_id == "guarda"
                        || context.memories.iter().any(|m| m.tags.contains(edital));

                    if agente_sabe {
                        match edital.as_str() {
                            "trabalho_forcado_campos" => {
                                work_obligations.push("SUBVERTER LEI: Construir um Armazem Oculto para desviar graos (Construir('armazem_oculto'))".to_string());
                            }
                            "racionamento_estrito" => {
                                work_obligations.push("SUBVERTER LEI: Furtar graos do Celeiro ou contrabandear graos (Furtar('graos') ou Roubar('celeiro'))".to_string());
                            }
                            "imposto_guerra" => {
                                work_obligations.push("SUBVERTER LEI: Esconder moedas para sonegar imposto (Esconder('moedas'))".to_string());
                            }
                            "proibicao_tavernas" => {
                                work_obligations.push("SUBVERTER LEI: Construir uma Taverna Secreta (Speakeasy) para beber e conspirar (Construir('taverna_secreta'))".to_string());
                            }
                            "confisco_metais" => {
                                work_obligations.push("SUBVERTER LEI: Esconder ferro bruto para evitar confisco (Esconder('metal_bruto'))".to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        let local_prices = self
            .local_prices_for_agent(context.position)
            .into_iter()
            .map(|price| {
                format!(
                    "{}={} moedas",
                    self.resource_display_name(&price.resource_id),
                    price.unit_price
                )
            })
            .collect::<Vec<_>>();
        let base_resource_availability = self
            .catalog
            .resources
            .iter()
            .filter(|resource| {
                resource
                    .tags
                    .iter()
                    .any(|tag| tag == "raw_material" || tag == "capital")
            })
            .map(|resource| {
                let total: i32 = self
                    .establishments
                    .iter()
                    .map(|establishment| {
                        Self::total_resource_amount(&establishment.stock, &resource.id)
                    })
                    .sum();
                format!("{} disponivel localmente: {}", resource.display_name, total)
            })
            .collect::<Vec<_>>();
        let scarcity_signals = self
            .village_economy
            .scarcity_metrics
            .iter()
            .filter(|metric| metric.pressure > 0)
            .take(4)
            .map(|metric| {
                format!(
                    "escassez de {} ({})",
                    self.resource_display_name(&metric.resource_id),
                    metric.pressure
                )
            })
            .collect::<Vec<_>>();
        let public_treasury_status = if self.village_economy.public_treasury < 12 {
            format!(
                "caixa publico baixo ({}) e risco de atraso civico",
                self.village_economy.public_treasury
            )
        } else {
            format!(
                "caixa publico estavel ({})",
                self.village_economy.public_treasury
            )
        };
        let open_tasks = context
            .household_id
            .map(|household_id| self.open_tasks_for_household(household_id))
            .unwrap_or_default();
        let has_food_purchase_in_transit = context
            .household_id
            .map(|household_id| {
                self.economic_tasks.iter().any(|task| {
                    task.actor_household_id == household_id
                        && task.creates_household_reserve
                        && task.phase == EconomicTaskPhase::InTransit
                        && task.phase != EconomicTaskPhase::Completed
                        && task.phase != EconomicTaskPhase::Failed
                })
            })
            .unwrap_or(false);
        let open_food_tasks = context
            .household_id
            .map(|household_id| {
                self.economic_tasks
                    .iter()
                    .filter(|task| {
                        task.actor_household_id == household_id
                            && matches!(
                                task.class,
                                EconomicTaskClass::HouseholdFoodPurchase
                                    | EconomicTaskClass::FoodSupplyTransport
                                    | EconomicTaskClass::FoodProduction
                            )
                            && task.phase != EconomicTaskPhase::Completed
                            && task.phase != EconomicTaskPhase::Failed
                    })
                    .count()
            })
            .unwrap_or(0);
        let grain_availability_total: i32 = self
            .establishments
            .iter()
            .map(|establishment| {
                Self::total_resource_amount(&establishment.stock, ResourceKind::Graos.id())
            })
            .sum();
        let external_grain_offer = self
            .market_quote(ResourceKind::Graos.id())
            .map(|quote| format!("graos externos por {} moedas", quote.buy_price));

        EconomicContextInput {
            household_name: household
                .map(|household| household.name.clone())
                .unwrap_or_else(|| "Sem lar".to_string()),
            household_treasury: household.map(|household| household.treasury).unwrap_or(0),
            pantry,
            reserved_food,
            food_crisis_level: household
                .map(|household| household.food_crisis_level)
                .unwrap_or(0),
            reserved_food_workers: household
                .map(|household| household.reserved_food_workers)
                .unwrap_or(0),
            open_food_tasks,
            has_food_purchase_in_transit,
            can_eat_from_reserve: household
                .map(|household| {
                    self.food_resource_ids_sorted()
                        .into_iter()
                        .any(|resource_id| {
                            Self::total_resource_amount(&household.reserved_food, &resource_id) > 0
                        })
                })
                .unwrap_or(false),
            pending_salary,
            tax_pressure,
            work_obligations,
            local_prices,
            base_resource_availability,
            scarcity_signals,
            grain_availability: format!("graos disponiveis localmente: {grain_availability_total}"),
            external_grain_offer,
            public_treasury_status,
            war_supply_status,
            open_tasks,
        }
    }

    pub(super) fn build_legal_context(&mut self, context: &AgentContext) -> LegalContextInput {
        let active_combat = self
            .combats
            .iter()
            .find(|combat| {
                combat.status == CombatStatus::Active && combat.participants.contains(&context.id)
            })
            .map(|combat| {
                format!(
                    "combate {} contra {}",
                    combat.id,
                    other_participant(&combat.participants, context.id)
                )
            });
        let nearby_threats = context
            .relations
            .iter()
            .filter_map(|(other_id, relation)| {
                let distance = self.agent_distance_from_immutable(context.position, *other_id)?;
                (distance <= 2 && relation.resentment >= 30).then(|| {
                    format!(
                        "agente {} proximo com ressentimento {}",
                        other_id, relation.resentment
                    )
                })
            })
            .take(4)
            .collect::<Vec<_>>();
        let open_cases = self
            .crime_cases
            .iter()
            .filter(|case| {
                matches!(
                    case.status,
                    CrimeCaseStatus::Open
                        | CrimeCaseStatus::Investigating
                        | CrimeCaseStatus::Proven
                        | CrimeCaseStatus::Arrested
                )
            })
            .take(5)
            .map(|case| {
                format!(
                    "caso {} {:?}: suspeito={:?} vitima={:?} severidade={} confianca={}",
                    case.id,
                    case.crime_type,
                    case.suspect_id,
                    case.victim_id,
                    case.severity,
                    case.confidence
                )
            })
            .collect::<Vec<_>>();
        let cases_against_actor = self
            .crime_cases
            .iter()
            .filter(|case| case.suspect_id == Some(context.id))
            .take(4)
            .map(|case| {
                format!(
                    "caso {} {:?} status {:?}",
                    case.id, case.crime_type, case.status
                )
            })
            .collect::<Vec<_>>();
        let cases_involving_actor = self
            .crime_cases
            .iter()
            .filter(|case| {
                case.victim_id == Some(context.id) || case.witnesses.contains(&context.id)
            })
            .take(4)
            .map(|case| {
                format!(
                    "caso {} {:?} status {:?}",
                    case.id, case.crime_type, case.status
                )
            })
            .collect::<Vec<_>>();
        let witness_count = self.witnesses_near(context.id, context.position, 4).len();
        LegalContextInput {
            life_status: format!("{:?}", context.life_status),
            injury_summary: self.injury_summary_for_agent(context.id),
            active_combat,
            nearby_threats,
            open_cases,
            cases_against_actor,
            cases_involving_actor,
            witness_risk: if witness_count > 0 {
                format!("{witness_count} testemunha(s) possiveis por perto")
            } else {
                "sem testemunhas proximas visiveis".to_string()
            },
        }
    }

    pub(super) fn work_obligations_for_context(&self, context: &AgentContext) -> Vec<String> {
        let mut obligations = Vec::new();
        if let Some(role_def) = self.role_def(&context.role_id) {
            for establishment in self.establishments.iter().filter(|establishment| {
                role_def
                    .allowed_establishment_type_ids
                    .contains(&establishment.establishment_type_id)
            }) {
                if context.role_id != Role::Farmer.id()
                    && establishment.building_id != self.work_building_id_for_role(&context.role_id)
                {
                    continue;
                }
                for target in &establishment.stock_targets {
                    let current =
                        Self::total_resource_amount(&establishment.stock, &target.resource_id);
                    if current < target.amount {
                        obligations.push(format!(
                            "{} abaixo do alvo em {}",
                            self.resource_display_name(&target.resource_id),
                            establishment.name
                        ));
                    }
                }
            }
        } else if let Some(building_id) = self.work_building_id_for_role(&context.role_id)
            && let Some(establishment) = self.establishment_by_building(building_id)
        {
            for target in &establishment.stock_targets {
                let current =
                    Self::total_resource_amount(&establishment.stock, &target.resource_id);
                if current < target.amount {
                    obligations.push(format!(
                        "{} abaixo do alvo em {}",
                        self.resource_display_name(&target.resource_id),
                        establishment.name
                    ));
                }
            }
        }
        obligations.truncate(4);
        obligations
    }

    pub(super) fn work_building_id_for_role(&self, role_id: &str) -> Option<BuildingId> {
        let role_def = self.role_def(role_id)?;
        self.establishments
            .iter()
            .find(|establishment| {
                role_def
                    .allowed_establishment_type_ids
                    .contains(&establishment.establishment_type_id)
            })
            .and_then(|establishment| establishment.building_id)
    }

    pub(super) fn open_tasks_for_household(
        &self,
        household_id: BuildingId,
    ) -> Vec<EconomicOpportunityInput> {
        let mut tasks = self
            .economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
            })
            .map(|task| EconomicOpportunityInput {
                kind: task.kind,
                class: task.class,
                priority: task.priority,
                summary: task.description.clone(),
                resource_id: task.resource_id.clone(),
                amount: task.amount,
                unit_price: (task.unit_price > 0).then_some(task.unit_price),
            })
            .collect::<Vec<_>>();
        tasks.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.summary.cmp(&b.summary))
        });
        tasks.truncate(6);
        tasks
    }

    pub(super) fn normalized_reconsideration_horizon(&self, kind: IntentKind) -> u64 {
        match kind {
            IntentKind::Socializar => 1,
            IntentKind::Comer => 2,
            IntentKind::Descansar => 3,
            IntentKind::Refletir => 3,
            IntentKind::Andar => 2,
            IntentKind::Comprar => 4,
            IntentKind::Transportar => 4,
            IntentKind::Vender => 4,
            IntentKind::ReceberPagamento => 3,
            IntentKind::Construir => 6,
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Furtar
            | IntentKind::Fugir
            | IntentKind::Acusar
            | IntentKind::Investigar
            | IntentKind::Prender
            | IntentKind::Punir
            | IntentKind::Apoiar
            | IntentKind::Opor
            | IntentKind::Pressionar
            | IntentKind::PedirApoio
            | IntentKind::Mediar
            | IntentKind::JurarLealdade
            | IntentKind::RomperLealdade
            | IntentKind::ConcederTitulo
            | IntentKind::RevogarTitulo
            | IntentKind::NomearOficial
            | IntentKind::ExigirTributo
            | IntentKind::CobrarCorveia
            | IntentKind::ConvocarLevy
            | IntentKind::ReconhecerHerdeiro
            | IntentKind::ApoiarPretendente
            | IntentKind::Usurpar
            | IntentKind::ReivindicarTerritorio
            | IntentKind::NegociarSuserania => 1,
            IntentKind::Trabalhar => ROUTINE_RECONSIDERATION_MAX as u64,
            IntentKind::Decretar | IntentKind::Esconder => 4,
        }
    }

    pub(super) fn blocked_ticks(&mut self, agent_id: u64) -> Result<u32> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .blocked_ticks)
    }

    pub(super) fn increment_blocked_ticks(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .blocked_ticks += 1;
        Ok(())
    }

    pub(super) fn reset_blocked_ticks(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .blocked_ticks = 0;
        Ok(())
    }

    pub(super) fn record_cognition_trigger(&mut self, agent_id: u64, trigger: &str) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .last_cognition_trigger = Some(trigger.to_string());
        Ok(())
    }

    pub(super) fn record_social_opportunity_signature(
        &mut self,
        agent_id: u64,
        signature: Option<String>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .last_social_opportunity_signature = signature;
        Ok(())
    }

    pub(super) fn set_thought(&mut self, agent_id: u64, thought: String) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<ThoughtComponent>()
            .ok_or_else(|| anyhow!("missing thought component"))?
            .0 = thought;
        Ok(())
    }

    pub(super) fn agent_name_map(&mut self) -> HashMap<u64, String> {
        let mut query = self.world.query::<&AgentCore>();
        query
            .iter(&self.world)
            .map(|core| (core.id, core.name.clone()))
            .collect()
    }

    pub(super) fn agent_role_pairs(&mut self) -> Vec<(u64, String)> {
        let mut query = self.world.query::<&AgentCore>();
        query
            .iter(&self.world)
            .map(|core| (core.id, core.role_id.clone()))
            .collect()
    }

    pub fn apply_edict_psychological_resistance(
        &mut self,
        agent_id: u64,
        edital: &str,
    ) -> Result<()> {
        let name = self.agent_name(agent_id)?;
        let role_id = self.agent_role_id(agent_id)?;
        let household_id = self.agent_home_building_id(agent_id)?;

        let state = self.agent_state(agent_id)?;
        let hunger = state.hunger;
        let stress = state.stress;

        // Achar o ID do líder
        let mut leader_id = 6; // default fallback
        let mut query = self.world.query::<(&AgentCore, &LifeStatusComponent)>();
        for (c, life_status) in query.iter(&self.world) {
            if c.role_id == "lider_local" && life_status.0 == AgentLifeStatus::Vivo {
                leader_id = c.id;
                break;
            }
        }

        // Achar a relação atual com o líder
        let relation = self.relation_between(agent_id, leader_id);
        let trust = relation.trust;

        let mut stress_inc = 0;
        let mut resentment_inc = 0;
        let mut trust_dec = 0;
        let mut resistiu = false;

        match edital {
            "trabalho_forcado_campos" => {
                if hunger >= 60 && trust < 0 {
                    stress_inc = 20;
                    resentment_inc = 15;
                    trust_dec = 15;
                    resistiu = true;
                }
            }
            "racionamento_estrito" => {
                if hunger >= 40 {
                    stress_inc = 15;
                    resentment_inc = 10;
                    trust_dec = 10;
                    resistiu = true;
                }
            }
            "imposto_guerra" => {
                let treasury = household_id
                    .and_then(|hid| self.household_by_id(hid))
                    .map(|h| h.treasury)
                    .unwrap_or(0);
                if treasury < 15 {
                    stress_inc = 25;
                    resentment_inc = 20;
                    trust_dec = 20;
                    resistiu = true;
                }
            }
            "proibicao_tavernas" => {
                if stress >= 50 {
                    stress_inc = 15;
                    resentment_inc = 10;
                    trust_dec = 10;
                    resistiu = true;
                }
            }
            "confisco_metais" => {
                if role_id == "ferreiro" && trust < 0 {
                    stress_inc = 30;
                    resentment_inc = 25;
                    trust_dec = 25;
                    resistiu = true;
                }
            }
            _ => {}
        }

        if resistiu {
            let mut institutional_delta = InstitutionalPerception::zero_delta();
            institutional_delta.leader_legitimacy = -trust_dec / 2;
            institutional_delta.fear_of_authority = (stress_inc / 4).max(1);
            institutional_delta.perceived_corruption = (resentment_inc / 5).max(1);
            institutional_delta.perceived_fairness = -(resentment_inc / 3).max(1);
            match edital {
                "racionamento_estrito" => {
                    institutional_delta.rationing_legitimacy = -12;
                }
                "imposto_guerra" => {
                    institutional_delta.tax_legitimacy = -16;
                    institutional_delta.war_support = -8;
                }
                "confisco_metais" => {
                    institutional_delta.tax_legitimacy = -10;
                    institutional_delta.perceived_corruption += 8;
                    institutional_delta.perceived_fairness -= 8;
                }
                "trabalho_forcado_campos" => {
                    institutional_delta.leader_legitimacy -= 8;
                    institutional_delta.perceived_fairness -= 6;
                }
                "proibicao_tavernas" => {
                    institutional_delta.leader_legitimacy -= 5;
                }
                _ => {}
            }
            self.adjust_institutional_perception(
                agent_id,
                institutional_delta,
                format!("resistencia ao edital {}", edital),
            )?;
            // Aplicar modificações
            let entity = self.find_agent_entity(agent_id)?;
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state_comp = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state_comp.0.stress = (state_comp.0.stress + stress_inc).clamp(0, 100);
            drop(state_comp);
            drop(entity_mut);

            let delta = RelationDelta {
                trust: -trust_dec,
                resentment: resentment_inc,
                ..Default::default()
            };
            self.apply_relation_delta(agent_id, leader_id, &delta)?;

            // O agente cria uma memória sobre a resistência
            self.add_memory(
                agent_id,
                MemoryKind::Reflection,
                format!(
                    "Eu odeio o edital '{}' do lider! Ele me causa muito estresse.",
                    edital
                ),
                vec![
                    "edital_rei".to_string(),
                    "resistencia".to_string(),
                    edital.to_string(),
                ],
                10,
                vec![leader_id],
            )?;

            // Evento
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: Some(leader_id),
                kind: EventKind::Reflection,
                summary: format!(
                    "{} esta sofrendo resistencia psicologica contra o edital '{}'",
                    name, edital
                ),
                impact_tags: vec!["resistencia".to_string(), edital.to_string()],
            });
        }

        Ok(())
    }
}
