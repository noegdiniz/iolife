use super::Simulation;
use super::*;
use crate::llm_adapter::LlmAdapter;
use crate::world_model::CropStage;
use anyhow::Result;

pub fn tick_interval_ms(ticks_per_second: u32) -> u64 {
    1_000 / u64::from(ticks_per_second.max(1))
}

impl Simulation {
    pub fn tick(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        self.total_ticks += 1;
        self.tick_of_day += 1;

        let crop_bonus_percent = self
            .active_policy_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                PolicyEffect::LaborDraft {
                    output_resource_id,
                    production_bonus_percent,
                } if output_resource_id == "graos" => Some(*production_bonus_percent),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let crop_inc = (1 + (crop_bonus_percent / 100).max(0)) as u32;

        for crop in self.crops.values_mut() {
            crop.ticks_since_planted += crop_inc;
            if crop.ticks_since_planted >= 30 {
                crop.stage = CropStage::Ready;
            } else if crop.ticks_since_planted >= 10 {
                crop.stage = CropStage::Growing;
            }
        }

        let crossed_day = self.tick_of_day >= self.ticks_per_day;
        if self.tick_of_day >= self.ticks_per_day {
            self.tick_of_day = 0;
            self.day += 1;
        }

        if crossed_day {
            self.close_daily_economy()?;
            self.apply_daily_aging()?;
            self.apply_daily_births()?;
            self.apply_daily_marriages()?;
            self.update_mourning_states()?;
            self.decay_psychological_states_daily()?;
            self.generate_daily_caravans()?;
            self.decay_rumors_daily()?;
            self.update_cultural_stories_daily()?;
            self.update_abstract_wars()?;
        }

        self.apply_needs_decay();
        self.refresh_economy_state()?;
        self.refresh_political_state()?;
        self.apply_faction_action_overrides()?;
        self.apply_child_behaviors()?;
        self.apply_caravan_behaviors()?;
        let agent_ids = self.agent_ids();

        for agent_id in &agent_ids {
            if self.can_agent_act(*agent_id)? {
                self.advance_agent_movement(*agent_id)?;
            }
        }

        for agent_id in &agent_ids {
            if self.can_agent_act(*agent_id)? {
                self.ensure_navigation_for_current_intent(*agent_id)?;
            }
        }

        for agent_id in &agent_ids {
            if self.can_agent_act(*agent_id)? {
                self.try_execute_current_intent(*agent_id, llm)?;
            }
        }

        self.process_scheduled_meetings()?;
        self.process_active_conversations(llm)?;
        self.process_general_decisions(llm)?;
        self.update_trauma_trackers()?;
        self.check_active_promises()?;
        self.tick_fauna_behavior()?;

        Ok(())
    }
}

impl Simulation {
    pub(super) fn can_agent_act(&mut self, agent_id: u64) -> Result<bool> {
        let entity = self.find_agent_entity(agent_id)?;
        let life_status = self
            .world
            .entity(entity)
            .get::<LifeStatusComponent>()
            .ok_or_else(|| anyhow!("missing life status component"))?
            .0;
        if life_status != AgentLifeStatus::Vivo {
            return Ok(false);
        }
        let in_conversation = self
            .world
            .entity(entity)
            .get::<ConversationComponent>()
            .map(|conversation| conversation.active_conversation_id.is_some())
            .unwrap_or(false);
        Ok(!in_conversation)
    }

    pub(super) fn try_execute_current_intent(
        &mut self,
        agent_id: u64,
        llm: &dyn LlmAdapter,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        if self
            .world
            .entity(entity)
            .get::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .active_conversation_id
            .is_some()
        {
            return Ok(());
        }
        if self.apply_emergency_food_rule(agent_id)? {
            return Ok(());
        }
        let synced_intent = self.sync_intent_with_locked_task(agent_id)?;
        let mut intent = self
            .world
            .entity(entity)
            .get::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0
            .clone();
        if synced_intent.is_some() {
            intent = synced_intent;
        }

        if intent.is_none() {
            if let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() {
                let restored = Self::intent_for_economic_task(&task);
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<IntentComponent>()
                    .ok_or_else(|| anyhow!("missing intent component"))?
                    .0 = Some(restored.clone());
                entity_mut
                    .get_mut::<ThoughtComponent>()
                    .ok_or_else(|| anyhow!("missing thought component"))?
                    .0 = format!("Persistindo tarefa economica ativa: {}", task.description);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = None;
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = restored.target_semantic.clone();
                entity_mut
                    .get_mut::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .clear();
                drop(entity_mut);
                self.ensure_navigation_for_current_intent(agent_id)?;
                intent = Some(restored);
            }
        }

        if intent.is_none() {
            let task_opt = {
                let mut entity_mut = self.world.entity_mut(entity);
                let mut queue = entity_mut
                    .get_mut::<TaskQueueComponent>()
                    .ok_or_else(|| anyhow!("missing task queue component"))?;
                queue.0.pop_front()
            };

            if let Some(task) = task_opt {
                let current_pos = self
                    .world
                    .entity(entity)
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0;
                let mut nearby_ids = Vec::new();
                let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
                for (core, position) in query.iter(&self.world) {
                    if core.id != agent_id && current_pos.manhattan(position.0) <= 6 {
                        nearby_ids.push(core.id);
                    }
                }

                let new_intent = AgentIntent {
                    kind: task.kind,
                    target_agent: task.target_agent,
                    target_semantic: task.target_semantic.clone(),
                    justification: format!("Executando tarefa da fila: {:?}", task.kind),
                    dominant_emotion: "determinado".to_string(),
                    perceived_risk: 0,
                    belief_updates: Vec::new(),
                    priority: 1,
                    social_move: task.social_move,
                };
                let validated = validate_intent(new_intent, &nearby_ids);

                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<IntentComponent>()
                    .ok_or_else(|| anyhow!("missing intent component"))?
                    .0 = Some(validated.clone());
                entity_mut
                    .get_mut::<ThoughtComponent>()
                    .ok_or_else(|| anyhow!("missing thought component"))?
                    .0 = format!("Sequencia: {:?}", validated.kind);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = None;
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = validated.target_semantic.clone();
                entity_mut
                    .get_mut::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .clear();

                drop(entity_mut);

                if matches!(
                    validated.kind,
                    IntentKind::Comprar
                        | IntentKind::Transportar
                        | IntentKind::Vender
                        | IntentKind::ReceberPagamento
                        | IntentKind::Construir
                        | IntentKind::Trabalhar
                ) {
                    self.bind_or_create_economic_task(agent_id, &validated)?;
                } else {
                    self.clear_active_economic_task(agent_id)?;
                }

                self.ensure_navigation_for_current_intent(agent_id)?;

                intent = Some(validated);
            }
        }

        // â”€â”€ Motor EconÃ´mico AutÃ´nomo â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        // Se o agente ainda nÃ£o tem intent (aguardando LLM ou recÃ©m-criado),
        // aplica regras determinÃ­sticas de sobrevivÃªncia e produÃ§Ã£o.
        if intent.is_none() {
            intent = self.apply_survival_economy(agent_id)?;
            if intent.is_some() {
                self.ensure_navigation_for_current_intent(agent_id)?;
            }
        }

        let Some(intent) = intent else {
            return Ok(());
        };
        if intent.kind == IntentKind::Comer && !self.household_has_food_available(agent_id)? {
            if self.reroute_eat_intent_to_food_purchase(agent_id)? {
                return self.try_execute_current_intent(agent_id, llm);
            }
        }
        if !self.ready_to_execute(agent_id, &intent)? {
            return Ok(());
        }
        match intent.kind {
            IntentKind::Trabalhar => self.apply_work(agent_id)?,
            IntentKind::Descansar => self.apply_rest(agent_id)?,
            IntentKind::Comer => self.apply_eat(agent_id)?,
            IntentKind::Refletir => self.apply_reflect(agent_id)?,
            IntentKind::Andar => self.apply_wander(agent_id)?,
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento
            | IntentKind::Construir => self.apply_economic_intent(agent_id)?,
            IntentKind::Agredir => self.apply_assault_intent(agent_id, intent.target_agent)?,
            IntentKind::Combater => self.apply_combat_intent(agent_id, intent.target_agent)?,
            IntentKind::Roubar => self.apply_robbery_intent(agent_id, intent.target_agent)?,
            IntentKind::Furtar => self.apply_theft_intent(agent_id, intent.target_agent)?,
            IntentKind::Fugir => self.apply_flee_intent(agent_id)?,
            IntentKind::Acusar => self.apply_accuse_intent(agent_id, intent.target_agent)?,
            IntentKind::Investigar => self.apply_investigate_intent(agent_id)?,
            IntentKind::Prender => self.apply_arrest_intent(agent_id, intent.target_agent)?,
            IntentKind::Punir => self.apply_punish_intent(agent_id, intent.target_agent)?,
            IntentKind::Apoiar => self.apply_political_support_intent(agent_id, true)?,
            IntentKind::Opor => self.apply_political_support_intent(agent_id, false)?,
            IntentKind::Pressionar => {
                self.apply_political_pressure_intent(agent_id, intent.target_agent)?
            }
            IntentKind::PedirApoio => {
                self.apply_political_request_support_intent(agent_id, intent.target_agent)?
            }
            IntentKind::Mediar => {
                self.apply_political_mediate_intent(agent_id, intent.target_agent)?
            }
            IntentKind::JurarLealdade => {
                self.apply_feudal_oath_intent(agent_id, intent.target_agent)?
            }
            IntentKind::RomperLealdade => {
                self.apply_break_fealty_intent(agent_id, intent.target_agent)?
            }
            IntentKind::ConcederTitulo => {
                self.apply_grant_title_intent(agent_id, intent.target_agent, &intent)?
            }
            IntentKind::RevogarTitulo => {
                self.apply_revoke_title_intent(agent_id, intent.target_agent, &intent)?
            }
            IntentKind::NomearOficial => {
                self.apply_appoint_office_intent(agent_id, intent.target_agent, &intent)?
            }
            IntentKind::ExigirTributo => {
                self.apply_demand_tribute_intent(agent_id, intent.target_agent)?
            }
            IntentKind::CobrarCorveia => self.apply_corvee_intent(agent_id, intent.target_agent)?,
            IntentKind::ConvocarLevy => {
                self.apply_levy_call_intent(agent_id, intent.target_agent)?
            }
            IntentKind::ReconhecerHerdeiro => {
                self.apply_recognize_heir_intent(agent_id, intent.target_agent)?
            }
            IntentKind::ApoiarPretendente => {
                self.apply_support_claimant_intent(agent_id, intent.target_agent)?
            }
            IntentKind::Usurpar => self.apply_usurp_intent(agent_id, intent.target_agent)?,
            IntentKind::ReivindicarTerritorio => {
                self.apply_claim_territory_intent(agent_id, &intent)?
            }
            IntentKind::NegociarSuserania => {
                self.apply_negotiate_suzerainty_intent(agent_id, intent.target_agent)?
            }
            IntentKind::Socializar => {
                if let Some(target_id) = intent.target_agent {
                    if self.agents_adjacent(agent_id, target_id)?
                        && self.open_conversation(
                            agent_id,
                            target_id,
                            intent.social_move.unwrap_or(SocialMove::Chat),
                            &intent.justification,
                        )?
                    {
                        let _ = llm.provider_name();
                    }
                }
            }
            IntentKind::Decretar => self.apply_decretar_intent(agent_id, &intent)?,
            IntentKind::Esconder => self.apply_esconder_intent(agent_id, &intent)?,
        }
        self.reset_blocked_ticks(agent_id)?;
        match intent.kind {
            IntentKind::Socializar => self.clear_intent_navigation(agent_id)?,
            IntentKind::Andar => self.clear_navigation_keep_intent(agent_id)?,
            IntentKind::Comer => {
                if self.agent_state(agent_id)?.hunger <= 25 {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
            IntentKind::Descansar => {
                if self.agent_state(agent_id)?.energy >= 80 {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
            IntentKind::Refletir => {
                if self.agent_state(agent_id)?.stress <= 25 {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
            IntentKind::Trabalhar
            | IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento
            | IntentKind::Construir => {
                let task_opt = self.active_economic_task_for_agent(agent_id).cloned();
                if let Some(task) = task_opt {
                    if task.phase == EconomicTaskPhase::Completed {
                        self.clear_intent_navigation(agent_id)?;
                        self.clear_active_economic_task(agent_id)?;
                    } else if task.phase == EconomicTaskPhase::Failed {
                        let entity = self.find_agent_entity(agent_id)?;
                        self.world
                            .entity_mut(entity)
                            .get_mut::<TaskQueueComponent>()
                            .ok_or_else(|| anyhow!("missing task queue component"))?
                            .0
                            .clear();
                        self.clear_intent_navigation(agent_id)?;
                        self.clear_active_economic_task(agent_id)?;
                        self.add_memory(
                            agent_id,
                            MemoryKind::Failure,
                            format!("Tarefa falhou: {}", task.description),
                            vec!["falha".to_string(), "economia".to_string()],
                            10,
                            Vec::new(),
                        )?;
                    }
                } else {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
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
            | IntentKind::Decretar
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
            | IntentKind::NegociarSuserania
            | IntentKind::Esconder => self.clear_intent_navigation(agent_id)?,
        }
        Ok(())
    }
}
