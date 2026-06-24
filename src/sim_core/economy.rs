use super::*;
use crate::world_model::PartInjuryStatus;
// Economic task, production, stock, price and logistics systems.

#[derive(Clone)]
pub(super) struct FoodSourceOffer {
    source: EconomicNode,
    related_establishment_id: Option<EstablishmentId>,
    resource_id: String,
    unit_price: i32,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FoodCrisisAssessment {
    pub household_minimum_food_units: i32,
    pub household_food_supply_days_tenths: i32,
    pub household_treasury: i32,
    pub household_feudal_arrears: i32,
    pub household_tribute_due: i32,
    pub village_grain_units: i32,
    pub village_ready_food_units: i32,
    pub stalled_food_processors: usize,
    pub material_food_source_count: usize,
    pub inter_village_food_source_count: usize,
    pub food_supply_emergency: bool,
    pub bottlenecks: Vec<String>,
    pub access_summary: String,
    pub political_cost_summary: String,
}

impl Simulation {
    pub(super) fn apply_needs_decay(&mut self) {
        let mut death_candidates = Vec::new();
        let mut query = self.world.query::<(
            &AgentCore,
            &LifeStatusComponent,
            &mut InjuryComponent,
            &mut StateComponent,
        )>();
        for (core, life_status, mut injury, mut state) in query.iter_mut(&mut self.world) {
            if life_status.0 == AgentLifeStatus::Morto {
                continue;
            }

            // 1. Processar sangramento de partes do corpo feridas (coagulação natural)
            if self.total_ticks % 8 == 0 {
                for part in &mut injury.0.body_parts {
                    if part.bleeding > 0 {
                        part.bleeding = (part.bleeding - 1).max(0);
                    }
                }
            }

            // 2. Curar partes do corpo ao longo dos ticks
            let is_heal_tick = (self.total_ticks.wrapping_add(core.id)) % 25 == 0;
            if is_heal_tick {
                for part in &mut injury.0.body_parts {
                    match part.status {
                        PartInjuryStatus::Bruised | PartInjuryStatus::Lacerated => {
                            if part.health < 100 {
                                part.health = (part.health + 4).clamp(0, 100);
                                part.pain = (part.pain - 5).max(0);
                                if part.health == 100 {
                                    part.status = PartInjuryStatus::Intact;
                                }
                            }
                        }
                        PartInjuryStatus::Fractured => {
                            if part.health < 100 {
                                // Fratura cura 4x mais lento
                                if self.total_ticks % 100 == 0 {
                                    part.health = (part.health + 1).clamp(0, 100);
                                    part.pain = (part.pain - 2).max(0);
                                    if part.health == 100 {
                                        part.status = PartInjuryStatus::Intact;
                                    }
                                }
                            }
                        }
                        PartInjuryStatus::Severed | PartInjuryStatus::Destroyed => {
                            // Danos permanentes, a dor residual diminui lentamente até um limite mínimo (phantom pain)
                            if part.pain > 15 {
                                part.pain = (part.pain - 1).max(15);
                            }
                        }
                        PartInjuryStatus::Intact => {}
                    }
                }
            }

            // 3. Atualizar estatísticas de dor e sangramento globais com base nas partes
            let total_pain: i32 = injury.0.body_parts.iter().map(|p| p.pain).sum();
            let total_bleeding: i32 = injury.0.body_parts.iter().map(|p| p.bleeding).sum();
            injury.0.pain = total_pain.clamp(0, 100);
            injury.0.bleeding = total_bleeding.clamp(0, 10);

            // Se o agente está sangrando, a saúde global cai
            if injury.0.bleeding > 0 {
                state.0.health = (state.0.health - injury.0.bleeding).clamp(0, 100);
            }

            // Atualizar estresse com base na dor geral
            if injury.0.pain > 0 && self.total_ticks % 10 == 0 {
                state.0.stress = (state.0.stress + (injury.0.pain / 15).max(1)).clamp(0, 100);
            }

            // 4. Decaimento normal de necessidades
            if (self.total_ticks.wrapping_add(core.id)) % 10 == 0 {
                state.0.hunger = (state.0.hunger + 1).clamp(0, 100);
            }
            if (self.total_ticks.wrapping_add(core.id)) % 20 == 0 {
                state.0.energy = (state.0.energy - 1).clamp(0, 100);
            }
            if (self.total_ticks.wrapping_add(core.id)) % 10 == 0 {
                state.0.stress = (state.0.stress + 1).clamp(0, 100);
            }
            if state.0.hunger > 90 || state.0.energy < 10 {
                state.0.health = (state.0.health - 1).clamp(0, 100);
            }
            if state.0.health <= 0 {
                death_candidates.push(core.id);
            }
        }
        for agent_id in death_candidates {
            let _ = self.mark_agent_dead(agent_id, "colapso fisico");
        }
    }
    pub(super) fn bind_or_create_economic_task(
        &mut self,
        agent_id: u64,
        intent: &AgentIntent,
    ) -> Result<()> {
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(());
        };
        let agent_entity = self.find_agent_entity(agent_id)?;
        let agent_role = self
            .world
            .entity(agent_entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .role_id
            .clone();
        let role_def = self.role_def(&agent_role).cloned();
        let allowed_production_establishments = role_def
            .as_ref()
            .map(|def| {
                self.establishments
                    .iter()
                    .filter(|establishment| {
                        def.allowed_establishment_type_ids
                            .contains(&establishment.establishment_type_id)
                    })
                    .map(|establishment| establishment.id)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        self.clear_active_economic_task(agent_id)?;
        let desired_kind = match intent.kind {
            IntentKind::Trabalhar => Some(EconomicTaskKind::Produzir),
            IntentKind::Comprar => Some(EconomicTaskKind::Comprar),
            IntentKind::Transportar => Some(EconomicTaskKind::Transportar),
            IntentKind::Construir => Some(EconomicTaskKind::Construir),
            IntentKind::Vender => Some(EconomicTaskKind::Vender),
            IntentKind::ReceberPagamento => Some(EconomicTaskKind::ReceberPagamento),
            _ => None,
        };
        let Some(desired_kind) = desired_kind else {
            return Ok(());
        };
        let target_hint = intent
            .target_semantic
            .clone()
            .unwrap_or_default()
            .to_lowercase();
        let matches_target = |task: &EconomicTask| {
            if target_hint.is_empty() {
                return true;
            }
            let description = task.description.to_lowercase();
            let resource_match = task
                .resource_id
                .as_ref()
                .map(|resource_id| target_hint.contains(resource_id))
                .unwrap_or(false);
            let household_food_match = matches!(task.destination, EconomicNode::HouseholdPantry(_))
                && (target_hint.contains("comida")
                    || target_hint.contains("lar")
                    || target_hint.contains("despensa"));
            let production_match = desired_kind == EconomicTaskKind::Produzir
                && (description.contains("lenha") && target_hint.contains("lenha")
                    || description.contains("metal") && target_hint.contains("metal")
                    || description.contains("graos") && target_hint.contains("graos")
                    || target_hint.contains("trabalho"));
            description.contains(&target_hint)
                || resource_match
                || household_food_match
                || production_match
        };
        let role_allows_task = |task: &EconomicTask| match desired_kind {
            EconomicTaskKind::Produzir => task
                .related_establishment_id
                .map(|establishment_id| {
                    allowed_production_establishments.contains(&establishment_id)
                })
                .unwrap_or(false),
            EconomicTaskKind::ReceberPagamento => role_def
                .as_ref()
                .map(|def| def.can_collect_payments)
                .unwrap_or(false),
            EconomicTaskKind::Construir => role_def
                .as_ref()
                .map(|def| def.id == Role::Farmer.id())
                .unwrap_or(false),
            EconomicTaskKind::Comprar
            | EconomicTaskKind::Transportar
            | EconomicTaskKind::Vender => role_def
                .as_ref()
                .map(|def| def.can_take_logistics_tasks)
                .unwrap_or(true),
        };

        let mut selected_task_id = self
            .economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.kind == desired_kind
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
                    && (task.assigned_agent_id.is_none()
                        || task.assigned_agent_id == Some(agent_id))
                    && matches_target(task)
                    && role_allows_task(task)
                    && self.allow_food_support_assignment(household_id, agent_id, task)
            })
            .max_by(|a, b| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| b.description.len().cmp(&a.description.len()))
            })
            .map(|task| task.id);

        if selected_task_id.is_none() {
            self.ensure_economic_tasks();
            selected_task_id = self
                .economic_tasks
                .iter()
                .filter(|task| {
                    task.actor_household_id == household_id
                        && task.kind == desired_kind
                        && task.phase != EconomicTaskPhase::Completed
                        && task.phase != EconomicTaskPhase::Failed
                        && (task.assigned_agent_id.is_none()
                            || task.assigned_agent_id == Some(agent_id))
                        && matches_target(task)
                        && role_allows_task(task)
                        && self.allow_food_support_assignment(household_id, agent_id, task)
                })
                .max_by(|a, b| {
                    a.priority
                        .cmp(&b.priority)
                        .then_with(|| b.description.len().cmp(&a.description.len()))
                })
                .map(|task| task.id);
        }

        if selected_task_id.is_none()
            && matches!(
                desired_kind,
                EconomicTaskKind::Comprar
                    | EconomicTaskKind::Transportar
                    | EconomicTaskKind::Produzir
            )
        {
            selected_task_id = self
                .economic_tasks
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
                        && (task.assigned_agent_id.is_none()
                            || task.assigned_agent_id == Some(agent_id))
                        && role_allows_task(task)
                        && self.allow_food_support_assignment(household_id, agent_id, task)
                })
                .max_by(|a, b| a.priority.cmp(&b.priority))
                .map(|task| task.id);
        }

        if let Some(task_id) = selected_task_id {
            if let Some(task) = self
                .economic_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            {
                task.assigned_agent_id = Some(agent_id);
            }
            let entity = self.find_agent_entity(agent_id)?;
            self.world
                .entity_mut(entity)
                .get_mut::<EconomicActivityComponent>()
                .ok_or_else(|| anyhow!("missing economy component"))?
                .active_task_id = Some(task_id);
        } else if desired_kind == EconomicTaskKind::Comprar
            && let Some(resource_id) = self.resolve_resource_id_from_hint(&target_hint)
            && let Some(task_id) =
                self.create_personal_item_purchase_task(agent_id, household_id, &resource_id)
        {
            let entity = self.find_agent_entity(agent_id)?;
            self.world
                .entity_mut(entity)
                .get_mut::<EconomicActivityComponent>()
                .ok_or_else(|| anyhow!("missing economy component"))?
                .active_task_id = Some(task_id);
        } else if desired_kind == EconomicTaskKind::Vender
            && let Some(resource_id) = self.resolve_resource_id_from_hint(&target_hint)
            && let Some(task_id) =
                self.create_personal_item_sale_task(agent_id, household_id, &resource_id)
        {
            let entity = self.find_agent_entity(agent_id)?;
            self.world
                .entity_mut(entity)
                .get_mut::<EconomicActivityComponent>()
                .ok_or_else(|| anyhow!("missing economy component"))?
                .active_task_id = Some(task_id);
        }
        Ok(())
    }

    pub(super) fn intent_for_economic_task(task: &EconomicTask) -> AgentIntent {
        let kind = match task.kind {
            EconomicTaskKind::Produzir => IntentKind::Trabalhar,
            EconomicTaskKind::Comprar => IntentKind::Comprar,
            EconomicTaskKind::Transportar => IntentKind::Transportar,
            EconomicTaskKind::Construir => IntentKind::Construir,
            EconomicTaskKind::Vender => IntentKind::Vender,
            EconomicTaskKind::ReceberPagamento => IntentKind::ReceberPagamento,
        };
        AgentIntent {
            kind,
            target_agent: None,
            target_semantic: Some(task.description.clone()),
            justification: format!("Concluir tarefa economica ativa: {}", task.description),
            dominant_emotion: "determinado".to_string(),
            perceived_risk: 2,
            belief_updates: Vec::new(),
            priority: task.priority.clamp(1, 10),
            social_move: None,
        }
    }

    pub(super) fn has_extreme_incapacity(&self, context: &AgentContext) -> bool {
        context.state.energy <= 5 || context.state.health <= 10 || context.state.hunger >= 95
    }

    pub(super) fn should_hold_locked_economic_task(&self, context: &AgentContext) -> bool {
        let Some(task) = self.active_economic_task_for_agent(context.id) else {
            return false;
        };
        task.lock_until_complete
            && task.phase != EconomicTaskPhase::Completed
            && task.phase != EconomicTaskPhase::Failed
            && context.blocked_ticks < BLOCKED_RECONSIDERATION_TICKS * 3
            && !self.has_extreme_incapacity(context)
    }

    pub(super) fn fail_active_economic_task(
        &mut self,
        agent_id: u64,
        reason: &str,
        clear_intent: bool,
    ) -> Result<()> {
        let active_task = self.active_economic_task_for_agent(agent_id).cloned();
        let Some(task) = active_task else {
            return Ok(());
        };
        if let Some(task_state) = self
            .economic_tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
        {
            task_state.phase = EconomicTaskPhase::Failed;
            task_state.assigned_agent_id = None;
        }
        let entity = self.find_agent_entity(agent_id)?;
        let core_name = self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .name
            .clone();
        self.world
            .entity_mut(entity)
            .get_mut::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .active_task_id = None;
        if clear_intent {
            self.clear_intent_navigation(agent_id)?;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Blocking,
            summary: format!(
                "{core_name} abandona a tarefa {}: {reason}.",
                task.description
            ),
            impact_tags: vec![
                "economia".to_string(),
                "falha".to_string(),
                "bloqueio".to_string(),
            ],
        });
        Ok(())
    }

    pub(super) fn sync_intent_with_locked_task(
        &mut self,
        agent_id: u64,
    ) -> Result<Option<AgentIntent>> {
        let active_task = self.active_economic_task_for_agent(agent_id).cloned();
        let Some(task) = active_task else {
            return Ok(None);
        };
        if !task.lock_until_complete {
            return Ok(None);
        }
        let expected_intent = Self::intent_for_economic_task(&task);
        let entity = self.find_agent_entity(agent_id)?;
        let current_intent = self
            .world
            .entity(entity)
            .get::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0
            .clone();
        let needs_sync = current_intent
            .as_ref()
            .map(|intent| {
                intent.kind != expected_intent.kind
                    || intent.target_semantic != expected_intent.target_semantic
            })
            .unwrap_or(true);
        if needs_sync {
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<IntentComponent>()
                .ok_or_else(|| anyhow!("missing intent component"))?
                .0 = Some(expected_intent.clone());
            entity_mut
                .get_mut::<ThoughtComponent>()
                .ok_or_else(|| anyhow!("missing thought component"))?
                .0 = format!("Persistindo tarefa economica ativa: {}", task.description);
            entity_mut
                .get_mut::<DestinationLabelComponent>()
                .ok_or_else(|| anyhow!("missing destination label component"))?
                .0 = expected_intent.target_semantic.clone();
        }
        Ok(Some(expected_intent))
    }

    pub(super) fn clear_active_economic_task(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let previous_task_id = self
            .world
            .entity(entity)
            .get::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .active_task_id;
        if let Some(task_id) = previous_task_id
            && let Some(task) = self
                .economic_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            && task.phase != EconomicTaskPhase::Completed
            && task.phase != EconomicTaskPhase::Failed
        {
            task.assigned_agent_id = None;
        }
        self.world
            .entity_mut(entity)
            .get_mut::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .active_task_id = None;
        Ok(())
    }

    pub(super) fn household_has_food_available(&mut self, agent_id: u64) -> Result<bool> {
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(false);
        };
        Ok(self.household_has_ready_food_available(household_id)
            || self.household_has_reserved_food_available(household_id))
    }
    pub(super) fn try_rebind_household_food_intent(&mut self, agent_id: u64) -> Result<bool> {
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(false);
        };
        let Some(household) = self.household_by_id(household_id) else {
            return Ok(false);
        };
        if household.food_crisis_level == 0 {
            return Ok(false);
        }
        self.ensure_economic_tasks();
        let fallback_intent = AgentIntent {
            kind: IntentKind::Comprar,
            target_agent: None,
            target_semantic: Some("comida para a despensa".to_string()),
            justification: "O lar segue em crise alimentar; preciso assumir a melhor tarefa de abastecimento disponivel.".to_string(),
            dominant_emotion: "urgencia".to_string(),
            perceived_risk: 6,
            belief_updates: vec!["Abastecimento de comida tem prioridade sobre a rotina agora.".to_string()],
            priority: 9,
            social_move: None,
        };
        self.bind_or_create_economic_task(agent_id, &fallback_intent)?;
        let task_found = self
            .active_economic_task_for_agent(agent_id)
            .map(|task| task.kind == EconomicTaskKind::Comprar)
            .unwrap_or(false);
        if task_found {
            let agent_name = self.agent_name(agent_id)?;
            let entity = self.find_agent_entity(agent_id)?;
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<IntentComponent>()
                .ok_or_else(|| anyhow!("missing intent component"))?
                .0 = Some(fallback_intent);
            entity_mut
                .get_mut::<ThoughtComponent>()
                .ok_or_else(|| anyhow!("missing thought component"))?
                .0 = "Rebinding para tarefa alimentar prioritaria do lar.".to_string();
            entity_mut
                .get_mut::<DestinationComponent>()
                .ok_or_else(|| anyhow!("missing destination component"))?
                .0 = None;
            entity_mut
                .get_mut::<DestinationLabelComponent>()
                .ok_or_else(|| anyhow!("missing destination label component"))?
                .0 = Some("abastecimento alimentar".to_string());
            entity_mut
                .get_mut::<PathComponent>()
                .ok_or_else(|| anyhow!("missing path component"))?
                .0
                .clear();
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Commerce,
                summary: format!("{agent_name} e desviado para o abastecimento alimentar do lar."),
                impact_tags: vec!["abastecimento".to_string(), "desvio".to_string()],
            });
        }
        Ok(task_found)
    }

    pub(super) fn active_economic_task_for_agent(&self, agent_id: u64) -> Option<&EconomicTask> {
        self.economic_tasks.iter().find(|task| {
            task.assigned_agent_id == Some(agent_id)
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
        })
    }

    pub(super) fn node_access_tile(&self, node: &EconomicNode) -> Option<TileCoord> {
        match node {
            EconomicNode::HouseholdPantry(building_id) => self
                .nearest_storage_for_building(Some(*building_id))
                .and_then(|fixture_id| self.fixture_by_id(fixture_id))
                .and_then(|fixture| self.fixture_access_tile(fixture))
                .or_else(|| {
                    self.building_by_id(*building_id)
                        .map(|building| building.entrance)
                }),
            EconomicNode::Establishment(establishment_id) => self
                .establishment_by_id(*establishment_id)
                .and_then(|establishment| establishment.storage_fixture_id)
                .and_then(|fixture_id| self.fixture_by_id(fixture_id))
                .and_then(|fixture| self.fixture_access_tile(fixture))
                .or_else(|| {
                    self.establishment_by_id(*establishment_id)
                        .and_then(|establishment| establishment.building_id)
                        .and_then(|building_id| self.building_by_id(building_id))
                        .map(|building| building.entrance)
                }),
            EconomicNode::PublicTreasury => self
                .spatial
                .buildings
                .iter()
                .find(|building| building.kind == LocationKind::Manor)
                .map(|building| building.entrance),
            EconomicNode::MilitarySupply(war_id) => self
                .wars
                .iter()
                .find(|war| war.id == *war_id)
                .and_then(|war| war.target_territory_ids.first().copied())
                .and_then(|territory_id| {
                    self.territories
                        .iter()
                        .find(|territory| territory.id == territory_id)
                })
                .and_then(|territory| territory.building_ids.first().copied())
                .and_then(|building_id| self.building_by_id(building_id))
                .map(|building| building.entrance)
                .or_else(|| {
                    self.spatial
                        .buildings
                        .iter()
                        .find(|building| building.kind == LocationKind::GuardPost)
                        .map(|building| building.entrance)
                })
                .or_else(|| {
                    self.spatial
                        .buildings
                        .iter()
                        .find(|building| building.kind == LocationKind::Manor)
                        .map(|building| building.entrance)
                }),
            EconomicNode::ConstructionProject(project_id) => self
                .construction_projects
                .iter()
                .find(|project| project.id == *project_id)
                .map(|project| project.entrance),
        }
    }

    pub(super) fn remove_resource_from_node(
        &mut self,
        node: &EconomicNode,
        resource_id: &str,
        amount: i32,
    ) -> i32 {
        match node {
            EconomicNode::HouseholdPantry(building_id) => self
                .household_by_id_mut(*building_id)
                .map(|household| Self::take_resource(&mut household.pantry, resource_id, amount))
                .unwrap_or(0),
            EconomicNode::Establishment(establishment_id) => self
                .establishment_by_id_mut(*establishment_id)
                .map(|establishment| {
                    Self::take_resource(&mut establishment.stock, resource_id, amount)
                })
                .unwrap_or(0),
            EconomicNode::ConstructionProject(_) => 0,
            EconomicNode::MilitarySupply(_) => 0,
            EconomicNode::PublicTreasury => 0,
        }
    }

    pub(super) fn add_resource_to_node(
        &mut self,
        node: &EconomicNode,
        resource_id: &str,
        amount: i32,
    ) {
        match node {
            EconomicNode::HouseholdPantry(building_id) => {
                if let Some(household) = self.household_by_id_mut(*building_id) {
                    Self::push_resource(&mut household.pantry, resource_id, amount);
                }
            }
            EconomicNode::Establishment(establishment_id) => {
                if let Some(establishment) = self.establishment_by_id_mut(*establishment_id) {
                    Self::push_resource(&mut establishment.stock, resource_id, amount);
                }
            }
            EconomicNode::ConstructionProject(project_id) => {
                if let Some(project) = self
                    .construction_projects
                    .iter_mut()
                    .find(|project| project.id == *project_id)
                {
                    Self::push_resource(&mut project.materials_delivered, resource_id, amount);
                    if project.status == ConstructionStatus::Planned {
                        project.status = ConstructionStatus::GatheringMaterials;
                    }
                }
            }
            EconomicNode::MilitarySupply(war_id) => {
                if let Some(demand) = self
                    .military_demands
                    .iter_mut()
                    .filter(|demand| {
                        demand.war_id == *war_id
                            && matches!(
                                demand.status,
                                MilitaryDemandStatus::Open
                                    | MilitaryDemandStatus::PartiallySupplied
                            )
                    })
                    .min_by_key(|demand| demand.deadline_day)
                {
                    Self::push_resource(&mut demand.delivered, resource_id, amount);
                    Self::recalculate_military_demand_status(demand);
                }
            }
            EconomicNode::PublicTreasury => {}
        }
    }

    pub(super) fn withdraw_cash_for_purchase(&mut self, task: &EconomicTask) -> bool {
        let total_price = task.total_price.max(0);
        if total_price == 0 {
            return true;
        }
        match task.destination {
            EconomicNode::HouseholdPantry(household_id) => {
                if let Some(household) = self.household_by_id_mut(household_id)
                    && household.treasury >= total_price
                {
                    household.treasury -= total_price;
                    return true;
                }
            }
            EconomicNode::Establishment(establishment_id) => {
                if let Some(establishment) = self.establishment_by_id_mut(establishment_id)
                    && establishment.cash >= total_price
                {
                    establishment.cash -= total_price;
                    return true;
                }
            }
            EconomicNode::ConstructionProject(_) => {
                if let Some(household) = self.household_by_id_mut(task.actor_household_id)
                    && household.treasury >= total_price
                {
                    household.treasury -= total_price;
                    return true;
                }
            }
            EconomicNode::MilitarySupply(_) => {
                if self.village_economy.public_treasury >= total_price {
                    self.village_economy.public_treasury -= total_price;
                    if let Some(local_polity_id) = self.polities.first().map(|polity| polity.id)
                        && let Some(polity) = self
                            .polities
                            .iter_mut()
                            .find(|polity| polity.id == local_polity_id)
                    {
                        polity.treasury = (polity.treasury - total_price).max(0);
                    }
                    if let EconomicNode::MilitarySupply(war_id) = task.destination
                        && let Some(demand) = self
                            .military_demands
                            .iter_mut()
                            .find(|demand| demand.war_id == war_id)
                    {
                        demand.cash_delivered =
                            (demand.cash_delivered + total_price).min(demand.cash_required);
                        Self::recalculate_military_demand_status(demand);
                    }
                    return true;
                }
            }
            EconomicNode::PublicTreasury => {}
        }
        false
    }

    pub(super) fn deposit_cash_to_sale_target(&mut self, task: &EconomicTask) {
        if task.total_price <= 0 {
            return;
        }
        match task.kind {
            EconomicTaskKind::Produzir => {}
            EconomicTaskKind::Comprar => {
                if let EconomicNode::Establishment(source_id) = task.source
                    && let Some(establishment) = self.establishment_by_id_mut(source_id)
                {
                    establishment.cash += task.total_price;
                }
            }
            EconomicTaskKind::Vender => {
                if let Some(establishment_id) = task.related_establishment_id
                    && let Some(establishment) = self.establishment_by_id_mut(establishment_id)
                {
                    establishment.cash += task.total_price;
                } else if let Some(household) = self.household_by_id_mut(task.actor_household_id) {
                    household.treasury += task.total_price;
                }
            }
            EconomicTaskKind::Transportar
            | EconomicTaskKind::ReceberPagamento
            | EconomicTaskKind::Construir => {}
        }
    }

    pub(super) fn create_personal_item_purchase_task(
        &mut self,
        agent_id: u64,
        household_id: BuildingId,
        resource_id: &str,
    ) -> Option<EconomicTaskId> {
        if !self.is_equipment_resource(resource_id) {
            return None;
        }
        let household = self.household_by_id(household_id)?;
        let dest_village_idx = self.village_index_of_household(household_id)?;
        let candidate = self
            .establishments
            .iter()
            .filter(|establishment| {
                establishment.item_stock_ids.iter().any(|item_id| {
                    self.item_instance(*item_id)
                        .map(|item| item.resource_id == resource_id)
                        .unwrap_or(false)
                })
            })
            .filter_map(|establishment| {
                let item = establishment
                    .item_stock_ids
                    .iter()
                    .filter_map(|item_id| self.item_instance(*item_id))
                    .filter(|item| item.resource_id == resource_id)
                    .max_by_key(|item| item.craft_quality_score)?;
                let base_unit_price = establishment
                    .posted_prices
                    .iter()
                    .find(|price| price.resource_id == resource_id)
                    .map(|price| price.unit_price)
                    .unwrap_or_else(|| self.base_price(resource_id));
                let is_local =
                    self.village_index_of_establishment(establishment.id) == Some(dest_village_idx);
                let adjusted_floor = if is_local {
                    base_unit_price
                } else {
                    (base_unit_price as f64 * 1.3) as i32
                };
                let unit_price = self.item_instance_unit_price(item, adjusted_floor);
                Some((
                    establishment.id,
                    establishment.name.clone(),
                    item.display_name.clone(),
                    unit_price,
                ))
            })
            .filter(|(_, _, _, unit_price)| household.treasury >= *unit_price)
            .min_by_key(|(_, _, _, unit_price)| *unit_price)?;

        let task_id = self.next_task_id();
        self.economic_tasks.push(EconomicTask {
            id: task_id,
            kind: EconomicTaskKind::Comprar,
            class: EconomicTaskClass::GeneralCommerce,
            priority: 7,
            lock_until_complete: true,
            creates_household_reserve: false,
            actor_household_id: household_id,
            assigned_agent_id: Some(agent_id),
            source: EconomicNode::Establishment(candidate.0),
            destination: EconomicNode::HouseholdPantry(household_id),
            resource_id: Some(resource_id.to_string()),
            amount: 1,
            unit_price: candidate.3,
            total_price: candidate.3,
            description: format!("Comprar {} em {}", candidate.2, candidate.1),
            phase: EconomicTaskPhase::AwaitingPickup,
            related_establishment_id: Some(candidate.0),
            related_construction_project_id: None,
        });
        Some(task_id)
    }

    pub(super) fn create_personal_item_sale_task(
        &mut self,
        agent_id: u64,
        household_id: BuildingId,
        resource_id: &str,
    ) -> Option<EconomicTaskId> {
        if !self.is_equipment_resource(resource_id) {
            return None;
        }
        let entity = self.find_agent_entity(agent_id).ok()?;
        let item_id = self
            .world
            .entity(entity)
            .get::<ItemInventoryComponent>()?
            .0
            .iter()
            .find(|candidate| {
                self.item_instance(**candidate)
                    .map(|item| item.resource_id == resource_id)
                    .unwrap_or(false)
            })
            .copied()?;
        let item_name = self.item_display_name_for_id(item_id);
        let dest_village_idx = self.village_index_of_household(household_id)?;

        let local_buyer = self
            .establishments
            .iter()
            .filter(|establishment| {
                self.village_index_of_establishment(establishment.id) == Some(dest_village_idx)
                    && establishment.cash > 0
                    && (establishment
                        .posted_prices
                        .iter()
                        .any(|price| price.resource_id == resource_id)
                        || self
                            .recipes_for_establishment(establishment)
                            .iter()
                            .any(|recipe| recipe.output_resource_id == resource_id))
            })
            .filter_map(|establishment| {
                let posted_price = establishment
                    .posted_prices
                    .iter()
                    .find(|price| price.resource_id == resource_id)
                    .map(|price| price.unit_price)
                    .unwrap_or_else(|| self.base_price(resource_id));
                let sale_price = (posted_price * 8 / 10).max(1);
                (establishment.cash >= sale_price)
                    .then(|| (establishment.id, establishment.name.clone(), sale_price))
            })
            .max_by_key(|(_, _, sale_price)| *sale_price);

        let (destination, sale_price, destination_label) =
            if let Some((establishment_id, establishment_name, sale_price)) = local_buyer {
                (
                    EconomicNode::Establishment(establishment_id),
                    sale_price,
                    establishment_name,
                )
            } else {
                return None;
            };

        let task_id = self.next_task_id();
        self.economic_tasks.push(EconomicTask {
            id: task_id,
            kind: EconomicTaskKind::Vender,
            class: EconomicTaskClass::GeneralCommerce,
            priority: 6,
            lock_until_complete: true,
            creates_household_reserve: false,
            actor_household_id: household_id,
            assigned_agent_id: Some(agent_id),
            source: destination.clone(),
            destination,
            resource_id: Some(resource_id.to_string()),
            amount: 1,
            unit_price: sale_price,
            total_price: sale_price,
            description: format!("Vender {item_name} em {destination_label}"),
            phase: EconomicTaskPhase::AwaitingPickup,
            related_establishment_id: None,
            related_construction_project_id: None,
        });
        Some(task_id)
    }

    pub(super) fn execute_equipment_purchase_task(
        &mut self,
        agent_id: u64,
        task: EconomicTask,
        resource_id: String,
    ) -> Result<bool> {
        if task.kind != EconomicTaskKind::Comprar || !self.is_equipment_resource(&resource_id) {
            return Ok(false);
        }
        if task.phase != EconomicTaskPhase::AwaitingPickup {
            return Ok(false);
        }
        let agent_name = self.agent_name(agent_id)?;
        if !self.withdraw_cash_for_purchase(&task) {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Scarcity,
                summary: format!(
                    "{agent_name} nao tem caixa suficiente para {}.",
                    task.description
                ),
                impact_tags: vec![
                    "escassez".to_string(),
                    "caixa".to_string(),
                    resource_id.clone(),
                ],
            });
            self.fail_economic_task(agent_id, task.id)?;
            return Ok(true);
        }

        let Some(source_id) = (match task.source {
            EconomicNode::Establishment(establishment_id) => Some(establishment_id),
            _ => None,
        }) else {
            self.fail_economic_task(agent_id, task.id)?;
            return Ok(true);
        };

        let Some(item_id) = self.remove_item_from_establishment_stock(source_id, &resource_id)
        else {
            self.fail_economic_task(agent_id, task.id)?;
            return Ok(true);
        };

        self.add_item_to_agent_inventory(agent_id, item_id)?;
        self.maybe_auto_equip_best_items(agent_id)?;
        self.deposit_cash_to_sale_target(&task);
        if let Some(task_state) = self
            .economic_tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
        {
            task_state.phase = EconomicTaskPhase::Completed;
        }
        self.clear_active_economic_task(agent_id)?;

        let item_name = self.item_display_name_for_id(item_id);
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Commerce,
            summary: format!("{agent_name} compra {item_name}."),
            impact_tags: vec![
                "comercio".to_string(),
                "equipamento".to_string(),
                resource_id,
            ],
        });
        self.sync_establishment_stocks_to_fixtures();
        Ok(true)
    }

    pub(super) fn execute_equipment_sale_task(
        &mut self,
        agent_id: u64,
        task: EconomicTask,
        resource_id: String,
    ) -> Result<bool> {
        if task.kind != EconomicTaskKind::Vender || !self.is_equipment_resource(&resource_id) {
            return Ok(false);
        }
        if task.class == EconomicTaskClass::GeneralCommerce
            && task.phase == EconomicTaskPhase::AwaitingPickup
        {
            let agent_name = self.agent_name(agent_id)?;
            let Some(item_id) = self.remove_item_from_agent_inventory(agent_id, &resource_id)?
            else {
                self.fail_economic_task(agent_id, task.id)?;
                return Ok(true);
            };
            self.maybe_auto_equip_best_items(agent_id)?;
            let item_name = self.item_display_name_for_id(item_id);
            let mut paid = false;
            match task.destination {
                EconomicNode::Establishment(establishment_id) => {
                    if let Some(establishment) = self.establishment_by_id_mut(establishment_id)
                        && establishment.cash >= task.total_price
                    {
                        establishment.cash -= task.total_price;
                        paid = true;
                    }
                    if !paid {
                        let _ = self.add_item_to_agent_inventory(agent_id, item_id);
                        self.maybe_auto_equip_best_items(agent_id)?;
                        self.fail_economic_task(agent_id, task.id)?;
                        return Ok(true);
                    }
                    let _ = self.add_item_to_establishment_stock(establishment_id, item_id);
                }
                _ => {
                    let _ = self.add_item_to_agent_inventory(agent_id, item_id);
                    self.maybe_auto_equip_best_items(agent_id)?;
                    self.fail_economic_task(agent_id, task.id)?;
                    return Ok(true);
                }
            }
            if paid && let Some(household) = self.household_by_id_mut(task.actor_household_id) {
                household.treasury += task.total_price;
            }
            if let Some(task_state) = self
                .economic_tasks
                .iter_mut()
                .find(|entry| entry.id == task.id)
            {
                task_state.phase = EconomicTaskPhase::Completed;
            }
            self.clear_active_economic_task(agent_id)?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Commerce,
                summary: format!("{agent_name} vende {item_name}."),
                impact_tags: vec![
                    "comercio".to_string(),
                    "equipamento".to_string(),
                    resource_id,
                ],
            });
            self.sync_establishment_stocks_to_fixtures();
            return Ok(true);
        }
        let agent_name = self.agent_name(agent_id)?;
        match task.phase {
            EconomicTaskPhase::AwaitingPickup => {
                let item_name = match task.source {
                    EconomicNode::Establishment(establishment_id) => {
                        let Some(item_id) = self
                            .remove_item_from_establishment_stock(establishment_id, &resource_id)
                        else {
                            self.fail_economic_task(agent_id, task.id)?;
                            return Ok(true);
                        };
                        self.add_item_to_agent_inventory(agent_id, item_id)?;
                        self.item_display_name_for_id(item_id)
                    }
                    _ => {
                        let Some(item_id) = self
                            .world
                            .entity(self.find_agent_entity(agent_id)?)
                            .get::<ItemInventoryComponent>()
                            .and_then(|inventory| {
                                inventory.0.iter().find(|candidate| {
                                    self.item_instance(**candidate)
                                        .map(|item| item.resource_id == resource_id)
                                        .unwrap_or(false)
                                })
                            })
                            .copied()
                        else {
                            self.fail_economic_task(agent_id, task.id)?;
                            return Ok(true);
                        };
                        self.item_display_name_for_id(item_id)
                    }
                };
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::InTransit;
                    task_state.amount = 1;
                    task_state.total_price = task_state.unit_price;
                }
                self.deposit_cash_to_sale_target(&task);
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Logistics,
                    summary: format!("{agent_name} separa {} para venda.", item_name),
                    impact_tags: vec![
                        "logistica".to_string(),
                        "equipamento".to_string(),
                        resource_id.clone(),
                    ],
                });
                self.sync_establishment_stocks_to_fixtures();
                Ok(true)
            }
            EconomicTaskPhase::InTransit => {
                let Some(item_id) =
                    self.remove_item_from_agent_inventory(agent_id, &resource_id)?
                else {
                    self.fail_economic_task(agent_id, task.id)?;
                    return Ok(true);
                };
                self.maybe_auto_equip_best_items(agent_id)?;
                let item_name = self.item_display_name_for_id(item_id);
                match task.destination {
                    EconomicNode::Establishment(establishment_id) => {
                        let _ = self.add_item_to_establishment_stock(establishment_id, item_id);
                    }
                    _ => {}
                }
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::Completed;
                }
                self.clear_active_economic_task(agent_id)?;
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Commerce,
                    summary: format!("{agent_name} vende {item_name}."),
                    impact_tags: vec![
                        "comercio".to_string(),
                        "equipamento".to_string(),
                        resource_id,
                    ],
                });
                self.sync_establishment_stocks_to_fixtures();
                Ok(true)
            }
            EconomicTaskPhase::AwaitingPayment
            | EconomicTaskPhase::Completed
            | EconomicTaskPhase::Failed => Ok(true),
        }
    }

    pub(super) fn apply_economic_intent(&mut self, agent_id: u64) -> Result<()> {
        let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() else {
            self.clear_intent_navigation(agent_id)?;
            return Ok(());
        };
        match task.kind {
            EconomicTaskKind::ReceberPagamento => self.execute_payment_collection(agent_id, task),
            EconomicTaskKind::Construir => self.execute_construction_task(agent_id, task),
            _ => self.execute_logistics_task(agent_id, task),
        }
    }

    pub(super) fn execute_payment_collection(
        &mut self,
        agent_id: u64,
        task: EconomicTask,
    ) -> Result<()> {
        let collected = self.collect_pending_payments(task.actor_household_id);
        let agent_name = self.agent_name(agent_id)?;
        let entity = self.find_agent_entity(agent_id)?;
        if let Some(mut economic) = self
            .world
            .entity_mut(entity)
            .get_mut::<EconomicActivityComponent>()
        {
            economic.active_task_id = None;
        }
        if let Some(task_state) = self
            .economic_tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
        {
            task_state.phase = EconomicTaskPhase::Completed;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Salary,
            summary: format!("{agent_name} recolhe {collected} moeda(s) em pagamentos."),
            impact_tags: vec!["salario".to_string(), "pagamento".to_string()],
        });
        Ok(())
    }

    pub(super) fn execute_logistics_task(
        &mut self,
        agent_id: u64,
        task: EconomicTask,
    ) -> Result<()> {
        let resource_id = task
            .resource_id
            .clone()
            .ok_or_else(|| anyhow!("economic task {} missing resource", task.id))?;
        if self.execute_equipment_purchase_task(agent_id, task.clone(), resource_id.clone())? {
            return Ok(());
        }
        if self.execute_equipment_sale_task(agent_id, task.clone(), resource_id.clone())? {
            return Ok(());
        }
        match task.phase {
            EconomicTaskPhase::AwaitingPickup => {
                let agent_name = self.agent_name(agent_id)?;
                if task.kind == EconomicTaskKind::Comprar && !self.withdraw_cash_for_purchase(&task)
                {
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: None,
                        kind: EventKind::Scarcity,
                        summary: format!(
                            "{agent_name} nao tem caixa suficiente para {}.",
                            task.description
                        ),
                        impact_tags: vec!["escassez".to_string(), "caixa".to_string()],
                    });
                    if let Some(task_state) = self
                        .economic_tasks
                        .iter_mut()
                        .find(|entry| entry.id == task.id)
                    {
                        task_state.phase = EconomicTaskPhase::Failed;
                    }
                    self.clear_active_economic_task(agent_id)?;
                    return Ok(());
                }
                let agent_entity = self.find_agent_entity(agent_id)?;
                let carrying_capacity = self
                    .world
                    .entity(agent_entity)
                    .get::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?
                    .carrying_capacity
                    .max(1);
                let pickup_amount = task.amount.min(carrying_capacity);
                let amount =
                    self.remove_resource_from_node(&task.source, &resource_id, pickup_amount);
                if amount <= 0 {
                    if let Some(task_state) = self
                        .economic_tasks
                        .iter_mut()
                        .find(|entry| entry.id == task.id)
                    {
                        task_state.phase = EconomicTaskPhase::Failed;
                    }
                    self.clear_active_economic_task(agent_id)?;
                    return Ok(());
                }
                let entity = self.find_agent_entity(agent_id)?;
                if task.creates_household_reserve
                    && matches!(task.destination, EconomicNode::HouseholdPantry(_))
                {
                    self.world
                        .entity_mut(entity)
                        .get_mut::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?
                        .carrying
                        .clear();
                    if let Some(household) = self.household_by_id_mut(task.actor_household_id) {
                        Self::push_resource(&mut household.reserved_food, &resource_id, amount);
                    }
                } else {
                    self.world
                        .entity_mut(entity)
                        .get_mut::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?
                        .carrying = vec![ResourceStack {
                        resource_id: resource_id.clone(),
                        amount,
                    }];
                }
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::InTransit;
                    task_state.amount = amount;
                    task_state.total_price = task_state.unit_price * amount;
                }
                self.deposit_cash_to_sale_target(&task);
                self.sync_establishment_stocks_to_fixtures();
            }
            EconomicTaskPhase::InTransit => {
                let agent_name = self.agent_name(agent_id)?;
                let delivered_amount = if task.creates_household_reserve
                    && matches!(task.destination, EconomicNode::HouseholdPantry(_))
                {
                    if let Some(household) = self.household_by_id_mut(task.actor_household_id) {
                        Self::take_resource(&mut household.reserved_food, &resource_id, task.amount)
                    } else {
                        0
                    }
                } else {
                    let entity = self.find_agent_entity(agent_id)?;
                    let entry = self.world.entity(entity);
                    entry
                        .get::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?
                        .carrying
                        .iter()
                        .find(|stack| stack.resource_id == resource_id)
                        .map(|stack| stack.amount)
                        .unwrap_or(0)
                };
                if delivered_amount > 0 {
                    self.add_resource_to_node(&task.destination, &resource_id, delivered_amount);
                }
                let entity = self.find_agent_entity(agent_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                let mut economic = entity_mut
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?;
                economic.carrying.clear();
                economic.active_task_id = None;
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::Completed;
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: if matches!(task.destination, EconomicNode::MilitarySupply(_)) {
                        EventKind::MilitarySupply
                    } else {
                        EventKind::Logistics
                    },
                    summary: format!("{agent_name} conclui a tarefa: {}.", task.description),
                    impact_tags: if let EconomicNode::MilitarySupply(war_id) = task.destination {
                        vec![
                            "logistica".to_string(),
                            "suprimento_militar".to_string(),
                            resource_id.clone(),
                            format!("war:{war_id}"),
                        ]
                    } else {
                        vec!["logistica".to_string(), resource_id.clone()]
                    },
                });
                self.sync_household_pantries_to_fixtures();
                self.sync_establishment_stocks_to_fixtures();
            }
            EconomicTaskPhase::AwaitingPayment
            | EconomicTaskPhase::Completed
            | EconomicTaskPhase::Failed => {}
        }
        Ok(())
    }

    pub(super) fn execute_construction_task(
        &mut self,
        agent_id: u64,
        task: EconomicTask,
    ) -> Result<()> {
        let Some(project_id) = task.related_construction_project_id else {
            return self.execute_logistics_task(agent_id, task);
        };
        if task.resource_id.is_some() {
            return self.execute_construction_material_task(agent_id, task, project_id);
        }

        let agent_name = self.agent_name(agent_id)?;
        let project_name = self
            .construction_projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.building_name.clone())
            .unwrap_or_else(|| format!("obra {project_id}"));
        let labor = 3;
        if let Some(project) = self
            .construction_projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            if !matches!(
                project.status,
                ConstructionStatus::GatheringMaterials | ConstructionStatus::UnderConstruction
            ) {
                project.status = ConstructionStatus::UnderConstruction;
            }
            project.labor_done = (project.labor_done + labor).min(project.labor_required);
        }
        if let Some(task_state) = self
            .economic_tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
        {
            task_state.phase = EconomicTaskPhase::Completed;
        }
        self.clear_active_economic_task(agent_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Construction,
            summary: format!("{agent_name} trabalha na obra de {project_name}."),
            impact_tags: vec![
                "construcao".to_string(),
                format!("project:{project_id}"),
                "trabalho".to_string(),
            ],
        });
        self.try_complete_construction_project(project_id);
        Ok(())
    }

    pub(super) fn execute_construction_material_task(
        &mut self,
        agent_id: u64,
        task: EconomicTask,
        project_id: u64,
    ) -> Result<()> {
        let resource_id = task
            .resource_id
            .clone()
            .ok_or_else(|| anyhow!("construction task {} missing resource", task.id))?;
        let project_funded_by_polity = self
            .construction_projects
            .iter()
            .find(|p| p.id == project_id)
            .and_then(|p| p.funding_polity_id);

        match task.phase {
            EconomicTaskPhase::AwaitingPickup => {
                let agent_name = self.agent_name(agent_id)?;
                let has_funds = if let Some(polity_id) = project_funded_by_polity {
                    self.polities
                        .iter()
                        .find(|polity| polity.id == polity_id)
                        .map(|polity| polity.treasury >= task.total_price)
                        .unwrap_or(false)
                } else {
                    self.household_by_id(task.actor_household_id)
                        .map(|household| household.treasury >= task.total_price)
                        .unwrap_or(false)
                };
                if task.total_price > 0 && !has_funds {
                    self.fail_economic_task(agent_id, task.id)?;
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: None,
                        kind: EventKind::Construction,
                        summary: format!(
                            "{agent_name} nao consegue pagar material para {}.",
                            task.description
                        ),
                        impact_tags: vec!["construcao".to_string(), "caixa".to_string()],
                    });
                    return Ok(());
                }
                let agent_entity = self.find_agent_entity(agent_id)?;
                let carrying_capacity = self
                    .world
                    .entity(agent_entity)
                    .get::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?
                    .carrying_capacity
                    .max(1);
                let pickup_amount = task.amount.min(carrying_capacity);
                let amount =
                    self.remove_resource_from_node(&task.source, &resource_id, pickup_amount);
                if amount <= 0 {
                    self.fail_economic_task(agent_id, task.id)?;
                    return Ok(());
                }
                if task.total_price > 0 {
                    if let Some(polity_id) = project_funded_by_polity {
                        if let Some(polity) = self.polities.iter_mut().find(|p| p.id == polity_id) {
                            polity.treasury -= task.total_price.min(polity.treasury);
                        }
                    } else if let Some(household) =
                        self.household_by_id_mut(task.actor_household_id)
                    {
                        household.treasury -= task.total_price.min(household.treasury);
                    }
                    if let EconomicNode::Establishment(source_id) = task.source
                        && let Some(establishment) = self.establishment_by_id_mut(source_id)
                    {
                        establishment.cash += task.total_price;
                    }
                }
                let entity = self.find_agent_entity(agent_id)?;
                self.world
                    .entity_mut(entity)
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?
                    .carrying = vec![ResourceStack {
                    resource_id: resource_id.clone(),
                    amount,
                }];
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::InTransit;
                    task_state.amount = amount;
                    task_state.total_price = task_state.unit_price * amount;
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Construction,
                    summary: format!(
                        "{agent_name} retira {} x{} para obra {}.",
                        self.resource_display_name(&resource_id),
                        amount,
                        project_id
                    ),
                    impact_tags: vec!["construcao".to_string(), resource_id],
                });
                self.sync_establishment_stocks_to_fixtures();
            }
            EconomicTaskPhase::InTransit => {
                let agent_name = self.agent_name(agent_id)?;
                let entity = self.find_agent_entity(agent_id)?;
                let delivered_amount = self
                    .world
                    .entity(entity)
                    .get::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?
                    .carrying
                    .iter()
                    .find(|stack| stack.resource_id == resource_id)
                    .map(|stack| stack.amount)
                    .unwrap_or(0);
                if delivered_amount > 0 {
                    self.add_resource_to_node(
                        &EconomicNode::ConstructionProject(project_id),
                        &resource_id,
                        delivered_amount,
                    );
                }
                let mut entity_mut = self.world.entity_mut(entity);
                let mut economic = entity_mut
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?;
                economic.carrying.clear();
                economic.active_task_id = None;
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::Completed;
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Construction,
                    summary: format!(
                        "{agent_name} entrega {} x{} na obra {}.",
                        self.resource_display_name(&resource_id),
                        delivered_amount,
                        project_id
                    ),
                    impact_tags: vec!["construcao".to_string(), resource_id],
                });
                self.try_complete_construction_project(project_id);
            }
            EconomicTaskPhase::AwaitingPayment
            | EconomicTaskPhase::Completed
            | EconomicTaskPhase::Failed => {}
        }
        Ok(())
    }

    pub(super) fn fail_economic_task(
        &mut self,
        agent_id: u64,
        task_id: EconomicTaskId,
    ) -> Result<()> {
        if let Some(task_state) = self
            .economic_tasks
            .iter_mut()
            .find(|entry| entry.id == task_id)
        {
            task_state.phase = EconomicTaskPhase::Failed;
        }
        self.clear_active_economic_task(agent_id)
    }

    pub(super) fn collect_pending_payments(&mut self, household_id: BuildingId) -> i32 {
        let claims = self
            .household_by_id(household_id)
            .map(|household| household.pending_payments.clone())
            .unwrap_or_default();
        let actor_id = self
            .household_by_id(household_id)
            .and_then(|household| household.member_ids.first().copied())
            .unwrap_or(0);
        let mut collected = 0;
        for claim in claims {
            let paid = if let Some(establishment_id) = claim.payer_establishment_id {
                if let Some(establishment) = self.establishment_by_id_mut(establishment_id) {
                    let amount = establishment.cash.min(claim.amount);
                    establishment.cash -= amount;
                    amount
                } else {
                    0
                }
            } else {
                let amount = self.village_economy.public_treasury.min(claim.amount);
                self.village_economy.public_treasury -= amount;
                amount
            };
            if paid > 0 {
                collected += paid;
                if let Some(household) = self.household_by_id_mut(household_id) {
                    household.treasury += paid;
                    if let Some(existing) = household.pending_payments.iter_mut().find(|pending| {
                        pending.payer_label == claim.payer_label && pending.amount == claim.amount
                    }) {
                        existing.amount -= paid;
                    }
                    household
                        .pending_payments
                        .retain(|pending| pending.amount > 0);
                }
            }
            if paid < claim.amount {
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: actor_id,
                    target: None,
                    kind: EventKind::Salary,
                    summary: format!(
                        "Pagamento de {} ficou parcial: {}/{} moeda(s).",
                        claim.payer_label, paid, claim.amount
                    ),
                    impact_tags: vec!["salario".to_string(), "atraso".to_string()],
                });
            }
        }
        collected
    }

    pub fn apply_work(&mut self, actor_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(actor_id)?;
        let intent_opt = self
            .world
            .entity(entity)
            .get::<IntentComponent>()
            .map(|ic| ic.0.clone())
            .flatten();
        if let Some(ref intent) = intent_opt {
            if intent.target_semantic.as_deref() == Some("motim_comida") {
                return self.execute_food_riot_steal(actor_id);
            }
        }

        let active_production_task = self
            .active_economic_task_for_agent(actor_id)
            .filter(|task| task.kind == EconomicTaskKind::Produzir)
            .cloned();
        let (name, role_id, current_pos, home_building_id, static_work_building_id) = {
            let entry = self.world.entity(entity);
            let core = entry
                .get::<AgentCore>()
                .ok_or_else(|| anyhow!("missing agent core"))?;
            (
                core.name.clone(),
                core.role_id.clone(),
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                core.home_building_id,
                core.work_building_id,
            )
        };
        let positional_work_building_id = self
            .spatial
            .fixtures
            .iter()
            .find(|fixture| {
                fixture.kind == FixtureKind::Workstation
                    && self.fixture_access_tile(fixture) == Some(current_pos)
            })
            .and_then(|fixture| fixture.building_id)
            .or_else(|| {
                self.spatial
                    .buildings
                    .iter()
                    .find(|building| building.entrance == current_pos)
                    .map(|building| building.id)
                    .filter(|building_id| self.establishment_by_building(*building_id).is_some())
            });
        let work_building_id = active_production_task
            .as_ref()
            .and_then(|task| task.related_establishment_id)
            .and_then(|establishment_id| {
                self.establishment_by_id(establishment_id)
                    .and_then(|establishment| establishment.building_id)
            })
            .or(positional_work_building_id)
            .or(static_work_building_id);
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.energy = (state.0.energy - 7).clamp(0, 100);
            state.0.hunger = (state.0.hunger + 6).clamp(0, 100);
            state.0.stress = (state.0.stress + 2).clamp(0, 100);
            state.0.mood = (state.0.mood + 1).clamp(0, 100);
        }
        let mut produced = ResourceStack {
            resource_id: ResourceKind::Moedas.id().to_string(),
            amount: 0,
        };
        let mut produced_item_ids = Vec::new();
        let mut crafted_discipline: Option<&'static str> = None;
        let mut work_failed_reason = None::<String>;
        let mut salary_claim = None::<PendingPaymentClaim>;
        let role_name = self.role_display_name(&role_id);
        let craft_proficiencies = self
            .world
            .entity(entity)
            .get::<CraftProficiencyComponent>()
            .map(|component| component.0.clone())
            .unwrap_or_default();

        let mut is_corvee_labor = false;
        let mut lord_agent_id_opt = None;
        let mut lord_household_id_opt = None;

        if let Some(building_id) = work_building_id {
            let est_info = self.establishment_by_building(building_id).map(|est| {
                (
                    est.id,
                    est.name.clone(),
                    est.establishment_type_id.clone(),
                    est.public_service,
                    est.stock.clone(),
                )
            });

            if let Some(household_id) = home_building_id {
                if let Some(household) = self.household_by_id(household_id) {
                    if household.corvee_days_due > 0 && household.direct_lord_agent_id.is_some() {
                        let lord_id = household.direct_lord_agent_id.unwrap();
                        if let Some((est_id, _, _, _, _)) = &est_info {
                            let in_estate = self.estate_holdings.iter().any(|holding| {
                                holding.holder_agent_id == Some(lord_id)
                                    && holding.establishment_ids.contains(est_id)
                            });
                            let mut is_owned = in_estate;
                            if !is_owned {
                                if let Some(lord_home_id) = self
                                    .agent_core_snapshot(lord_id)
                                    .and_then(|core| core.home_building_id)
                                {
                                    if let Some(est) = self.establishment_by_id(*est_id) {
                                        if est.owner_household_ids.contains(&lord_home_id) {
                                            is_owned = true;
                                        }
                                    }
                                }
                            }
                            if is_owned {
                                is_corvee_labor = true;
                                lord_agent_id_opt = Some(lord_id);
                                lord_household_id_opt = self
                                    .agent_core_snapshot(lord_id)
                                    .and_then(|core| core.home_building_id);
                            }
                        }
                    }
                }
            }

            if let Some((_est_id, est_name, est_type, est_public_service, est_stock)) = est_info {
                let recipe = self.recipe_for_establishment_type(&est_type).cloned();

                enum FarmAction {
                    Harvest(Vec<TileCoord>),
                    Plant(Vec<TileCoord>),
                    FailGrowing,
                }

                let farm_action = if est_type == "fazenda" {
                    let farm_buildings: Vec<&BuildingSpec> = self
                        .spatial
                        .buildings
                        .iter()
                        .filter(|b| b.kind == LocationKind::Farm)
                        .collect();
                    let farm_fields: Vec<TileCoord> = self
                        .spatial
                        .grid
                        .tiles
                        .iter()
                        .filter(|tile| tile.kind == TileKind::Field)
                        .map(|tile| tile.coord)
                        .filter(|&coord| {
                            if farm_buildings.is_empty() {
                                false
                            } else {
                                let closest = farm_buildings
                                    .iter()
                                    .min_by_key(|b| b.entrance.manhattan(coord))
                                    .unwrap();
                                closest.id == building_id
                            }
                        })
                        .collect();

                    let mut ready_fields = Vec::new();
                    let mut empty_fields = Vec::new();
                    for coord in farm_fields {
                        if let Some(crop) = self.crops.get(&coord) {
                            if crop.stage == CropStage::Ready {
                                ready_fields.push(coord);
                            }
                        } else {
                            empty_fields.push(coord);
                        }
                    }

                    if !ready_fields.is_empty() {
                        FarmAction::Harvest(ready_fields)
                    } else if !empty_fields.is_empty() {
                        FarmAction::Plant(empty_fields)
                    } else {
                        FarmAction::FailGrowing
                    }
                } else {
                    FarmAction::FailGrowing
                };

                let mut can_work = false;

                if est_type == "fazenda" {
                    match farm_action {
                        FarmAction::Harvest(ready_fields) => {
                            let mut has_tools = true;
                            if let Some(ref recipe) = recipe {
                                let missing_capital =
                                    recipe.capital_requirements.iter().find(|requirement| {
                                        Self::total_resource_amount(
                                            &est_stock,
                                            &requirement.resource_id,
                                        ) < requirement.amount
                                    });
                                if let Some(requirement) = missing_capital {
                                    work_failed_reason = Some(format!(
                                        "faltam {} em {}",
                                        requirement.resource_id, est_name
                                    ));
                                    has_tools = false;
                                }
                            }
                            if has_tools {
                                for coord in ready_fields {
                                    self.crops.remove(&coord);
                                }
                                produced = ResourceStack {
                                    resource_id: ResourceKind::Graos.id().to_string(),
                                    amount: recipe.as_ref().map(|r| r.output_amount).unwrap_or(6),
                                };
                                if let Some(establishment) =
                                    self.establishment_by_building_mut(building_id)
                                {
                                    if let Some(ref recipe) = recipe {
                                        if recipe.tool_wear > 0
                                            && !recipe.capital_requirements.is_empty()
                                        {
                                            establishment.tool_wear += recipe.tool_wear;
                                            while establishment.tool_wear >= 4 {
                                                let mut degraded = false;
                                                for capital in &recipe.capital_requirements {
                                                    let removed = Self::take_resource(
                                                        &mut establishment.stock,
                                                        &capital.resource_id,
                                                        1,
                                                    );
                                                    if removed > 0 {
                                                        degraded = true;
                                                    }
                                                }
                                                establishment.tool_wear -= 4;
                                                if !degraded {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                can_work = true;
                            }
                        }
                        FarmAction::Plant(empty_fields) => {
                            for coord in empty_fields {
                                self.crops.insert(
                                    coord,
                                    CropState {
                                        stage: CropStage::Planted,
                                        ticks_since_planted: 0,
                                    },
                                );
                            }
                            produced = ResourceStack {
                                resource_id: ResourceKind::Graos.id().to_string(),
                                amount: 0,
                            };
                            can_work = true;
                        }
                        FarmAction::FailGrowing => {
                            work_failed_reason = Some("plantacoes ainda crescendo".to_string());
                            can_work = false;
                        }
                    }
                } else {
                    if let Some(ref recipe) = recipe {
                        let missing_capital =
                            recipe.capital_requirements.iter().find(|requirement| {
                                Self::total_resource_amount(&est_stock, &requirement.resource_id)
                                    < requirement.amount
                            });
                        if let Some(requirement) = missing_capital {
                            work_failed_reason = Some(format!(
                                "faltam {} em {}",
                                requirement.resource_id, est_name
                            ));
                            can_work = false;
                        } else {
                            let is_equipment_output =
                                self.is_equipment_resource(&recipe.output_resource_id);
                            let mut enough_inputs = false;
                            let mut failure_establishment_name = est_name.clone();
                            let mut produced_items = Vec::new();

                            if let Some(establishment) =
                                self.establishment_by_building_mut(building_id)
                            {
                                let mut consumed_inputs = Vec::new();
                                enough_inputs = true;
                                failure_establishment_name = establishment.name.clone();
                                for input in &recipe.inputs {
                                    let taken = Self::take_resource(
                                        &mut establishment.stock,
                                        &input.resource_id,
                                        input.amount,
                                    );
                                    if taken < input.amount {
                                        consumed_inputs.push((input.resource_id.clone(), taken));
                                        enough_inputs = false;
                                        break;
                                    }
                                    consumed_inputs.push((input.resource_id.clone(), taken));
                                }
                                if !enough_inputs {
                                    for (resource_id, amount) in consumed_inputs {
                                        if amount > 0 {
                                            Self::push_resource(
                                                &mut establishment.stock,
                                                &resource_id,
                                                amount,
                                            );
                                        }
                                    }
                                } else if recipe.tool_wear > 0
                                    && !recipe.capital_requirements.is_empty()
                                {
                                    establishment.tool_wear += recipe.tool_wear;
                                    while establishment.tool_wear >= 4 {
                                        let mut degraded = false;
                                        for capital in &recipe.capital_requirements {
                                            let removed = Self::take_resource(
                                                &mut establishment.stock,
                                                &capital.resource_id,
                                                1,
                                            );
                                            if removed > 0 {
                                                degraded = true;
                                            }
                                        }
                                        establishment.tool_wear -= 4;
                                        if !degraded {
                                            break;
                                        }
                                    }
                                }
                            }

                            if !enough_inputs {
                                work_failed_reason = Some(format!(
                                    "faltam insumos para {} em {}",
                                    recipe.output_resource_id, failure_establishment_name
                                ));
                                can_work = false;
                            } else {
                                produced = ResourceStack {
                                    resource_id: recipe.output_resource_id.clone(),
                                    amount: recipe.output_amount,
                                };
                                if is_equipment_output {
                                    crafted_discipline =
                                        Some(self.craft_discipline_for_recipe(recipe));
                                    let material_signature = recipe
                                        .inputs
                                        .iter()
                                        .map(|input| input.resource_id.clone())
                                        .collect::<Vec<_>>()
                                        .join("+");
                                    let owner_household_id = home_building_id;
                                    let item_count = recipe.output_amount.max(1);
                                    for _ in 0..item_count {
                                        if let Some(item) = self.build_item_instance(
                                            &recipe.output_resource_id,
                                            Some(actor_id),
                                            None,
                                            owner_household_id,
                                            &craft_proficiencies,
                                            Some(recipe),
                                            material_signature.clone(),
                                        ) {
                                            produced_item_ids.push(item.id);
                                            produced_items.push(item);
                                        }
                                    }
                                }
                                if !produced_items.is_empty() {
                                    self.item_instances.extend(produced_items);
                                }
                                can_work = true;
                            }
                        }
                    } else {
                        can_work = est_public_service;
                    }
                }

                if can_work {
                    if is_corvee_labor {
                        if let Some(lord_household_id) = lord_household_id_opt {
                            if let Some(lord_household) =
                                self.household_by_id_mut(lord_household_id)
                            {
                                if produced.amount > 0 {
                                    if produced.resource_id == ResourceKind::Moedas.id() {
                                        lord_household.treasury += produced.amount;
                                    } else {
                                        Self::push_resource(
                                            &mut lord_household.pantry,
                                            &produced.resource_id,
                                            produced.amount,
                                        );
                                    }
                                }
                            }
                        }
                        // Decrement peasant corvee days due
                        if let Some(household_id) = home_building_id {
                            if let Some(household) = self.household_by_id_mut(household_id) {
                                household.corvee_days_due = (household.corvee_days_due - 1).max(0);
                            }
                        }
                        // Additional stress +3 and mood -4 penalty for forced labor
                        {
                            let mut entity_mut = self.world.entity_mut(entity);
                            if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
                                state.0.stress = (state.0.stress + 3).clamp(0, 100);
                                state.0.mood = (state.0.mood - 4).clamp(0, 100);
                            }
                        }
                    } else {
                        if let Some(establishment) = self.establishment_by_building_mut(building_id)
                        {
                            if !produced_item_ids.is_empty() {
                                establishment
                                    .item_stock_ids
                                    .extend(produced_item_ids.clone());
                            } else if produced.amount > 0 {
                                Self::push_resource(
                                    &mut establishment.stock,
                                    &produced.resource_id,
                                    produced.amount,
                                );
                            }
                            if let Some(_household_id) = home_building_id {
                                salary_claim = Some(PendingPaymentClaim {
                                    payer_establishment_id: if establishment.public_service {
                                        None
                                    } else {
                                        Some(establishment.id)
                                    },
                                    payer_label: establishment.name.clone(),
                                    amount: establishment.wage_per_shift,
                                });
                            }
                        }
                    }
                }
            }
        }
        let had_salary_claim = salary_claim.is_some();
        if let Some(household_id) = home_building_id
            && let Some(claim) = salary_claim
            && let Some(household) = self.household_by_id_mut(household_id)
        {
            household.pending_payments.push(claim);
        }
        self.sync_establishment_stocks_to_fixtures();
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: if work_failed_reason.is_some() {
                EventKind::Scarcity
            } else if is_corvee_labor {
                EventKind::FeudalSanction
            } else if had_salary_claim {
                EventKind::Salary
            } else {
                EventKind::Routine
            },
            summary: if let Some(reason) = work_failed_reason.clone() {
                format!("{name} tenta trabalhar como {}, mas {}.", role_name, reason)
            } else if is_corvee_labor {
                let lord_name = if let Some(lord_id) = lord_agent_id_opt {
                    self.agent_name(lord_id)
                        .unwrap_or_else(|_| "seu suserano".to_string())
                } else {
                    "seu suserano".to_string()
                };
                format!("{name} executa trabalho de corveia como {role_name} para {lord_name}.")
            } else {
                format!("{name} trabalha como {}.", role_name)
            },
            impact_tags: if is_corvee_labor && work_failed_reason.is_none() {
                vec![
                    "corveia".to_string(),
                    "trabalho".to_string(),
                    produced.resource_id.clone(),
                ]
            } else {
                vec!["trabalho".to_string(), produced.resource_id.clone()]
            },
        });
        if work_failed_reason.is_none() {
            if let Some(task) = active_production_task
                && let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
            {
                task_state.phase = EconomicTaskPhase::Completed;
            }
            self.clear_active_economic_task(actor_id)?;
            if !produced_item_ids.is_empty() {
                let entity = self.find_agent_entity(actor_id)?;
                if let Some(mut prof) = self
                    .world
                    .entity_mut(entity)
                    .get_mut::<CraftProficiencyComponent>()
                {
                    match crafted_discipline {
                        Some("tailoring") => {
                            prof.0.tailoring = (prof.0.tailoring + 1).clamp(0, 100)
                        }
                        Some("jewelry") => prof.0.jewelry = (prof.0.jewelry + 1).clamp(0, 100),
                        Some("leatherwork") => {
                            prof.0.leatherwork = (prof.0.leatherwork + 1).clamp(0, 100)
                        }
                        _ => prof.0.smithing = (prof.0.smithing + 1).clamp(0, 100),
                    }
                }
            }
            self.add_memory(
                actor_id,
                MemoryKind::Success,
                if !produced_item_ids.is_empty() {
                    let item_names = produced_item_ids
                        .iter()
                        .map(|item_id| self.item_display_name_for_id(*item_id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("Trabalho concluido produzindo {}.", item_names)
                } else if produced.amount > 0 {
                    format!(
                        "Trabalho concluido produzindo {}.",
                        self.resource_display_name(&produced.resource_id)
                    )
                } else {
                    "Trabalho civico concluido e pagamento aguardado.".to_string()
                },
                vec!["trabalho".to_string(), produced.resource_id.clone()],
                8,
                Vec::new(),
            )?;
        } else {
            if let Some(task) = active_production_task
                && let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
            {
                task_state.phase = EconomicTaskPhase::Failed;
            }
        }
        Ok(())
    }

    pub(super) fn apply_rest(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;

        let mut stress_reduction = 12;
        if self
            .active_policy_act_by_agenda("proibicao_tavernas")
            .is_some()
        {
            if self
                .agent_memories(actor_id)
                .map(|mems| {
                    mems.iter()
                        .any(|m| m.tags.contains(&"proibicao_tavernas".to_string()))
                })
                .unwrap_or(false)
            {
                stress_reduction = (stress_reduction as f64 * 0.7) as i32;
            }
        }

        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.energy = (state.0.energy + 22).clamp(0, 100);
            state.0.stress = (state.0.stress - stress_reduction).clamp(0, 100);
            state.0.mood = (state.0.mood + 3).clamp(0, 100);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::Routine,
            summary: format!("{name} descansa perto de sua cama."),
            impact_tags: vec!["descanso".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_eat(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;
        let ate = self.consume_food_for_agent(actor_id)?;
        let rationing_energy_gain_percent = self.active_rationing_energy_gain_percent();
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            if ate {
                let mut energy_gain = 4;
                energy_gain = energy_gain * rationing_energy_gain_percent / 100;
                state.0.hunger = (state.0.hunger - 38).clamp(0, 100);
                state.0.energy = (state.0.energy + energy_gain).clamp(0, 100);
                state.0.stress = (state.0.stress - 6).clamp(0, 100);
                state.0.mood = (state.0.mood + 5).clamp(0, 100);
            } else {
                state.0.stress = (state.0.stress + 6).clamp(0, 100);
                state.0.mood = (state.0.mood - 4).clamp(0, 100);
            }
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: if ate {
                EventKind::Commerce
            } else {
                EventKind::Need
            },
            summary: if ate {
                format!("{name} come e recupera forcas.")
            } else {
                format!("{name} procura comida, mas encontra escassez.")
            },
            impact_tags: vec!["fome".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_reflect(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.stress = (state.0.stress - 8).clamp(0, 100);
            state.0.mood = (state.0.mood + 2).clamp(0, 100);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::Reflection,
            summary: format!("{name} se recolhe para refletir em um lugar calmo."),
            impact_tags: vec!["reflexao".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_wander(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.stress = (state.0.stress - 1).clamp(0, 100);
            state.0.mood = (state.0.mood + 1).clamp(0, 100);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::Travel,
            summary: format!("{name} passeia pela vila."),
            impact_tags: vec!["movimento".to_string()],
        });
        Ok(())
    }

    pub(super) fn consume_food_for_agent(&mut self, agent_id: u64) -> Result<bool> {
        let food_order = self.food_resource_ids_sorted();
        let accepted = food_order.iter().map(|id| id.as_str()).collect::<Vec<_>>();
        if let Some(household_id) = self.household_id_for_agent(agent_id) {
            if let Some(household) = self.household_by_id_mut(household_id)
                && consume_matching(&mut household.pantry, &accepted)
            {
                return Ok(true);
            }
            if self.consume_reserved_food_for_household(household_id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn consume_reserved_food_for_household(
        &mut self,
        household_id: BuildingId,
    ) -> Result<bool> {
        let food_order = self.food_resource_ids_sorted();
        let accepted = food_order.iter().map(|id| id.as_str()).collect::<Vec<_>>();
        let consumed = if let Some(household) = self.household_by_id_mut(household_id) {
            consume_matching(&mut household.reserved_food, &accepted)
        } else {
            false
        };
        if !consumed {
            return Ok(false);
        }

        let reserved_task_id = self
            .economic_tasks
            .iter()
            .find(|task| {
                task.actor_household_id == household_id
                    && task.creates_household_reserve
                    && task.phase == EconomicTaskPhase::InTransit
                    && task.amount > 0
                    && task.resource_id.as_deref() == Some(ResourceKind::Graos.id())
            })
            .map(|task| task.id);
        if let Some(task_id) = reserved_task_id
            && let Some(task) = self
                .economic_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
        {
            task.amount = (task.amount - 1).max(0);
            if task.amount == 0 {
                task.phase = EconomicTaskPhase::Completed;
                task.assigned_agent_id = None;
            }
        }
        Ok(true)
    }

    pub(super) fn household_has_ready_food_available(&self, household_id: BuildingId) -> bool {
        self.household_by_id(household_id)
            .map(|household| {
                self.food_resource_ids_sorted()
                    .into_iter()
                    .any(|resource_id| {
                        Self::total_resource_amount(&household.pantry, &resource_id) > 0
                    })
            })
            .unwrap_or(false)
    }

    pub(super) fn household_has_reserved_food_available(&self, household_id: BuildingId) -> bool {
        self.household_by_id(household_id)
            .map(|household| {
                self.food_resource_ids_sorted()
                    .into_iter()
                    .any(|resource_id| {
                        Self::total_resource_amount(&household.reserved_food, &resource_id) > 0
                    })
            })
            .unwrap_or(false)
    }

    pub fn household_by_id(&self, household_id: BuildingId) -> Option<&HouseholdEconomy> {
        self.households
            .iter()
            .find(|household| household.id == household_id)
    }

    pub fn household_by_id_mut(
        &mut self,
        household_id: BuildingId,
    ) -> Option<&mut HouseholdEconomy> {
        self.households
            .iter_mut()
            .find(|household| household.id == household_id)
    }

    pub(super) fn household_id_for_agent(&mut self, agent_id: u64) -> Option<BuildingId> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world
            .entity(entity)
            .get::<AgentCore>()?
            .home_building_id
    }

    pub(super) fn establishment_by_id(
        &self,
        establishment_id: EstablishmentId,
    ) -> Option<&EstablishmentEconomy> {
        self.establishments
            .iter()
            .find(|establishment| establishment.id == establishment_id)
    }

    pub(super) fn establishment_by_id_mut(
        &mut self,
        establishment_id: EstablishmentId,
    ) -> Option<&mut EstablishmentEconomy> {
        self.establishments
            .iter_mut()
            .find(|establishment| establishment.id == establishment_id)
    }

    pub(super) fn establishment_by_building(
        &self,
        building_id: BuildingId,
    ) -> Option<&EstablishmentEconomy> {
        self.establishments
            .iter()
            .find(|establishment| establishment.building_id == Some(building_id))
    }

    pub(super) fn establishment_by_building_mut(
        &mut self,
        building_id: BuildingId,
    ) -> Option<&mut EstablishmentEconomy> {
        self.establishments
            .iter_mut()
            .find(|establishment| establishment.building_id == Some(building_id))
    }

    pub(super) fn economic_task_summary(&self, task_id: EconomicTaskId) -> Option<String> {
        self.economic_tasks
            .iter()
            .find(|task| task.id == task_id && task.phase != EconomicTaskPhase::Completed)
            .map(|task| task.description.clone())
    }

    pub(super) fn total_resource_amount(stacks: &[ResourceStack], resource_id: &str) -> i32 {
        stacks
            .iter()
            .filter(|stack| stack.resource_id == resource_id)
            .map(|stack| stack.amount.max(0))
            .sum()
    }

    pub(super) fn total_food_units(stacks: &[ResourceStack]) -> i32 {
        stacks
            .iter()
            .filter(|stack| matches!(stack.resource_id.as_str(), "graos" | "pao" | "caldo"))
            .map(|stack| stack.amount.max(0))
            .sum()
    }

    pub(super) fn take_resource(
        stacks: &mut Vec<ResourceStack>,
        resource_id: &str,
        amount: i32,
    ) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let mut remaining = amount;
        let mut taken = 0;
        for stack in stacks
            .iter_mut()
            .filter(|stack| stack.resource_id == resource_id)
        {
            if remaining <= 0 {
                break;
            }
            let delta = stack.amount.min(remaining);
            if delta > 0 {
                stack.amount -= delta;
                remaining -= delta;
                taken += delta;
            }
        }
        stacks.retain(|stack| stack.amount > 0);
        taken
    }

    pub(super) fn push_resource(stacks: &mut Vec<ResourceStack>, resource_id: &str, amount: i32) {
        if amount > 0 {
            merge_stack(
                stacks,
                ResourceStack {
                    resource_id: resource_id.to_string(),
                    amount,
                },
            );
        }
    }

    pub(super) fn base_price(&self, resource_id: &str) -> i32 {
        self.catalog
            .resources
            .iter()
            .find(|resource| resource.id == resource_id)
            .map(|resource| resource.base_price)
            .unwrap_or(1)
    }

    pub(super) fn sync_establishment_stocks_to_fixtures(&mut self) {
        let updates = self
            .establishments
            .iter()
            .filter_map(|establishment| {
                establishment
                    .storage_fixture_id
                    .map(|fixture_id| (fixture_id, establishment.stock.clone()))
            })
            .collect::<Vec<_>>();
        for (fixture_id, stock) in updates {
            if let Some(fixture) = self
                .spatial
                .fixtures
                .iter_mut()
                .find(|fixture| fixture.id == fixture_id)
            {
                fixture.stock = stock;
            }
        }
    }

    pub(super) fn sync_household_pantries_to_fixtures(&mut self) {
        let updates = self
            .households
            .iter()
            .filter_map(|household| {
                self.nearest_storage_for_building(Some(household.id))
                    .map(|fixture_id| (fixture_id, household.pantry.clone()))
            })
            .collect::<Vec<_>>();
        for (fixture_id, stock) in updates {
            if let Some(fixture) = self
                .spatial
                .fixtures
                .iter_mut()
                .find(|fixture| fixture.id == fixture_id)
            {
                fixture.stock = stock;
            }
        }
    }

    pub(super) fn refresh_economy_state(&mut self) -> Result<()> {
        let households = self.households.clone();
        let updates = households
            .iter()
            .map(|household| {
                let food_units = Self::total_food_units(&household.pantry)
                    + Self::total_food_units(&household.reserved_food);
                let scarcity_pressure = (household.minimum_food_units - food_units).max(0);
                let hungry_members = self.household_member_count_with_need(household.id, 65);
                let critical_hungry_members =
                    self.household_member_count_with_need(household.id, 85);
                let food_crisis_level = if scarcity_pressure <= 0 {
                    0
                } else if scarcity_pressure >= household.minimum_food_units / 2
                    || critical_hungry_members >= 2
                    || (critical_hungry_members >= 1 && hungry_members >= 2)
                {
                    2
                } else {
                    1
                };
                let reserved_food_workers =
                    self.household_assigned_food_support_workers(household.id) as u8;
                (
                    household.id,
                    household.name.clone(),
                    household.member_ids.first().copied().unwrap_or(0),
                    scarcity_pressure,
                    food_crisis_level,
                    reserved_food_workers,
                )
            })
            .collect::<Vec<_>>();

        let current_total_ticks = self.total_ticks;
        for (
            household_id,
            household_name,
            actor_id,
            scarcity_pressure,
            food_crisis_level,
            reserved_food_workers,
        ) in updates
        {
            let mut previous_level = 0;
            if let Some(household) = self.household_by_id_mut(household_id) {
                previous_level = household.food_crisis_level;
                household.scarcity_pressure = scarcity_pressure;
                household.food_crisis_level = food_crisis_level;
                household.reserved_food_workers = reserved_food_workers;
                if food_crisis_level > 0 {
                    household.last_food_shortage_tick = current_total_ticks;
                }
            }
            if food_crisis_level > previous_level {
                let assessment = self.food_crisis_assessment_for_household(Some(household_id));
                let access_priority = self.household_food_access_priority(household_id);
                let mut impact_tags = vec![
                    "crise_alimentar".to_string(),
                    format!("household:{household_id}"),
                ];
                impact_tags.extend(assessment.bottlenecks.iter().cloned());
                if assessment.material_food_source_count == 0 {
                    impact_tags.push("sem_fornecedor_material".to_string());
                }
                if assessment.stalled_food_processors > 0 {
                    impact_tags.push("processador_parado".to_string());
                }
                if access_priority >= 90 {
                    impact_tags.push("vantagem_feudal_alimentar".to_string());
                } else if access_priority <= 45 {
                    impact_tags.push("acesso_alimentar_fragil".to_string());
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: actor_id,
                    target: None,
                    kind: EventKind::Scarcity,
                    summary: format!(
                        "{household_name} entra em crise alimentar nivel {food_crisis_level}: {}.",
                        assessment.political_cost_summary
                    ),
                    impact_tags,
                });

                if assessment.food_supply_emergency && access_priority <= 55 {
                    let member_ids = self
                        .household_by_id(household_id)
                        .map(|household| household.member_ids.clone())
                        .unwrap_or_default();
                    for member_id in member_ids {
                        let mut delta = InstitutionalPerception::zero_delta();
                        delta.rationing_legitimacy = -4;
                        delta.perceived_fairness = -5;
                        delta.leader_legitimacy = -2;
                        delta.perceived_corruption = 2;
                        let _ = self.adjust_institutional_perception(
                            member_id,
                            delta,
                            "fome e acesso alimentar fragil corroem confianca no racionamento",
                        );
                    }
                }
            }
        }

        let recalculated = self
            .establishments
            .iter()
            .map(|establishment| {
                let posted_prices = self.recalculate_posted_prices(establishment);
                (establishment.id, posted_prices)
            })
            .collect::<Vec<_>>();
        for (establishment_id, posted_prices) in recalculated {
            if let Some(establishment) = self.establishment_by_id_mut(establishment_id) {
                establishment.posted_prices = posted_prices;
            }
        }

        let confiscation_effects = self
            .active_policy_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                PolicyEffect::ResourceConfiscation {
                    resource_id,
                    excluded_establishment_type_ids,
                    destination_establishment_type_id,
                } => Some((
                    resource_id.clone(),
                    excluded_establishment_type_ids.clone(),
                    destination_establishment_type_id.clone(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (resource_id, excluded_establishment_type_ids, destination_establishment_type_id) in
            confiscation_effects
        {
            let mut total_confiscado = 0;
            for est in &mut self.establishments {
                if !excluded_establishment_type_ids.contains(&est.establishment_type_id) {
                    if let Some(stack) = est.stock.iter_mut().find(|s| s.resource_id == resource_id)
                    {
                        if stack.amount > 0 {
                            total_confiscado += stack.amount;
                            stack.amount = 0;
                        }
                    }
                }
            }
            if total_confiscado > 0 {
                if let Some(solar) = self
                    .establishments
                    .iter_mut()
                    .find(|e| e.establishment_type_id == destination_establishment_type_id)
                {
                    if let Some(stack) = solar
                        .stock
                        .iter_mut()
                        .find(|s| s.resource_id == resource_id)
                    {
                        stack.amount += total_confiscado;
                    } else {
                        solar.stock.push(ResourceStack {
                            resource_id: resource_id.clone(),
                            amount: total_confiscado,
                        });
                    }
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: 1,
                    target: None,
                    kind: EventKind::Commerce,
                    summary: format!(
                        "A Guarda confiscou {} unidade(s) de {} para o esforco publico.",
                        total_confiscado,
                        self.resource_display_name(&resource_id)
                    ),
                    impact_tags: vec!["confisco".to_string(), resource_id],
                });
            }
        }

        // Desvio de Grãos para o Armazém Oculto
        let mut armazem_oculto_ids = Vec::new();
        for est in &self.establishments {
            if est.establishment_type_id == "armazem_oculto" {
                armazem_oculto_ids.push(est.id);
            }
        }
        if !armazem_oculto_ids.is_empty() {
            let mut desviado_total = 0;
            for est in &mut self.establishments {
                if est.location_kind == LocationKind::Farm {
                    if let Some(stack) = est.stock.iter_mut().find(|s| s.resource_id == "graos") {
                        let desvio = (stack.amount as f32 * 0.3) as i32;
                        if desvio > 0 {
                            stack.amount -= desvio;
                            desviado_total += desvio;
                        }
                    }
                }
            }
            if desviado_total > 0 {
                if let Some(armazem) = self
                    .establishments
                    .iter_mut()
                    .find(|e| e.establishment_type_id == "armazem_oculto")
                {
                    if let Some(stack) = armazem.stock.iter_mut().find(|s| s.resource_id == "graos")
                    {
                        stack.amount += desviado_total;
                    } else {
                        armazem.stock.push(ResourceStack {
                            resource_id: "graos".to_string(),
                            amount: desviado_total,
                        });
                    }
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: 0,
                    target: None,
                    kind: EventKind::Theft,
                    summary: format!("Camponeses rebeldes desviaram {} grao(s) para o Armazem Oculto clandestino.", desviado_total),
                    impact_tags: vec!["subversao".to_string(), "armazem_oculto".to_string(), "graos".to_string()],
                });
            }
        }

        // Taverna Secreta e Desvio de Bebidas/Caldo
        let mut taverna_secreta_ids = Vec::new();
        for est in &self.establishments {
            if est.establishment_type_id == "taverna_secreta" {
                taverna_secreta_ids.push(est.id);
            }
        }
        if !taverna_secreta_ids.is_empty() {
            let mut caldo_desviado = 0;
            for est in &mut self.establishments {
                if est.location_kind == LocationKind::Tavern
                    && est.establishment_type_id != "taverna_secreta"
                {
                    if let Some(stack) = est.stock.iter_mut().find(|s| s.resource_id == "caldo") {
                        let desvio = stack.amount;
                        if desvio > 0 {
                            stack.amount = 0;
                            caldo_desviado += desvio;
                        }
                    }
                }
            }
            if caldo_desviado > 0 {
                if let Some(secreta) = self
                    .establishments
                    .iter_mut()
                    .find(|e| e.establishment_type_id == "taverna_secreta")
                {
                    if let Some(stack) = secreta.stock.iter_mut().find(|s| s.resource_id == "caldo")
                    {
                        stack.amount += caldo_desviado;
                    } else {
                        secreta.stock.push(ResourceStack {
                            resource_id: "caldo".to_string(),
                            amount: caldo_desviado,
                        });
                    }
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: 0,
                    target: None,
                    kind: EventKind::Commerce,
                    summary: format!("Contrabando: {} caldo(s) foram desviados para a Taverna Secreta (Speakeasy) clandestina.", caldo_desviado),
                    impact_tags: vec!["subversao".to_string(), "taverna_secreta".to_string(), "caldo".to_string()],
                });
            }
        }

        self.village_economy.scarcity_metrics = self.compute_scarcity_metrics();
        self.ensure_economic_tasks();
        self.sync_establishment_stocks_to_fixtures();
        self.sync_household_pantries_to_fixtures();
        Ok(())
    }
}

impl Simulation {
    pub(super) fn close_daily_economy(&mut self) -> Result<()> {
        let mut daily_tax = self.village_economy.daily_household_tax;
        daily_tax = daily_tax * self.active_tax_multiplier_percent() / 100;

        let mut boycotted_members = std::collections::HashSet::new();
        let agent_ids: Vec<u64> = {
            let mut query = self.world.query::<&AgentCore>();
            query.iter(&self.world).map(|core| core.id).collect()
        };
        for id in agent_ids {
            if let Ok(mems) = self.agent_memories(id) {
                if mems
                    .iter()
                    .any(|m| m.tags.contains(&"moedas_escondidas".to_string()))
                {
                    boycotted_members.insert(id);
                }
            }
        }

        let current_day = self.day;
        let tax_results = self
            .households
            .iter()
            .map(|household| {
                let boycotted = household.member_ids.iter().any(|&member_id| {
                    let faction_boycott = self.political_factions.iter().any(|f| {
                        f.is_action_active
                            && f.agenda_tag == "boicote_imposto"
                            && f.member_ids.contains(&member_id)
                    });

                    let edital_boycott = boycotted_members.contains(&member_id);

                    faction_boycott || edital_boycott
                });
                let owed = daily_tax + household.tax_arrears;
                let paid = if boycotted {
                    0
                } else {
                    household.treasury.min(owed.max(0))
                };
                let arrears = owed - paid;
                (
                    household.id,
                    household.name.clone(),
                    household.member_ids.first().copied().unwrap_or(0),
                    owed,
                    paid,
                    arrears,
                    boycotted,
                )
            })
            .collect::<Vec<_>>();
        for (household_id, household_name, actor_id, owed, paid, arrears, boycotted) in tax_results
        {
            if let Some(household) = self.household_by_id_mut(household_id) {
                household.treasury -= paid;
                household.tax_arrears = arrears.max(0);
                if paid > 0 {
                    household.last_tax_paid_day = current_day;
                }
            }
            self.village_economy.public_treasury += paid;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: actor_id,
                target: None,
                kind: EventKind::Tax,
                summary: if boycotted {
                    format!("{household_name} recusa-se a pagar impostos em protesto ativo!")
                } else if paid >= owed {
                    format!("{household_name} paga {paid} moeda(s) de imposto ao caixa publico.")
                } else if paid > 0 {
                    format!(
                        "{household_name} paga apenas {paid}/{owed} moeda(s) de imposto; fica devendo {}.",
                        arrears.max(0)
                    )
                } else {
                    format!(
                        "{household_name} nao consegue pagar imposto; debito acumulado em {} moeda(s).",
                        arrears.max(0)
                    )
                },
                impact_tags: if boycotted {
                    vec!["imposto".to_string(), "boicote_imposto".to_string()]
                } else {
                    vec!["imposto".to_string(), "caixa_publico".to_string()]
                },
            });
        }

        let distributions = self
            .establishments
            .iter()
            .filter(|establishment| {
                !establishment.public_service && !establishment.owner_household_ids.is_empty()
            })
            .map(|establishment| {
                let reserve = 30;
                let distributable = (establishment.cash - reserve).max(0);
                (
                    establishment.id,
                    establishment.owner_household_ids.clone(),
                    distributable,
                )
            })
            .filter(|(_, owners, distributable)| !owners.is_empty() && *distributable > 0)
            .collect::<Vec<_>>();

        for (establishment_id, owners, distributable) in distributions {
            let share = distributable / owners.len() as i32;
            if share <= 0 {
                continue;
            }
            if let Some(establishment) = self.establishment_by_id_mut(establishment_id) {
                establishment.cash -= share * owners.len() as i32;
            }
            for owner in owners {
                if let Some(household) = self.household_by_id_mut(owner) {
                    household.treasury += share;
                }
            }
        }

        // Daily Grain Distribution by the Leader from the Celeiro (Farm establishment)
        let mut farm_est_ids = Vec::new();
        for est in &self.establishments {
            if est.location_kind == LocationKind::Farm {
                farm_est_ids.push(est.id);
            }
        }

        let mut leader_id = None;
        let mut query = self.world.query::<(Entity, &AgentCore)>();
        for (_, core) in query.iter(&self.world) {
            if core.role_id == "lider_local" {
                leader_id = Some(core.id);
                break;
            }
        }

        if !farm_est_ids.is_empty() {
            let mut total_grains = 0;
            for &farm_id in &farm_est_ids {
                if let Some(farm) = self.establishment_by_id(farm_id) {
                    total_grains += Self::total_resource_amount(&farm.stock, "graos");
                }
            }

            if total_grains > 0 {
                struct HouseholdPriority {
                    id: BuildingId,
                    name: String,
                    average_hunger: i32,
                    priority: i32,
                    is_favored: bool,
                    members: Vec<u64>,
                }

                let mut hh_priorities = Vec::new();
                for hh in self.households.clone() {
                    let mut total_hunger = 0;
                    let mut count = 0;
                    for &m_id in &hh.member_ids {
                        if let Ok(state) = self.agent_state(m_id) {
                            total_hunger += state.hunger;
                            count += 1;
                        }
                    }
                    let avg_hunger = if count > 0 { total_hunger / count } else { 0 };

                    let is_favored = hh.member_ids.iter().any(|&m_id| {
                        self.policy_favors
                            .iter()
                            .any(|fav| fav.beneficiary_id == m_id)
                    });

                    let priority = if is_favored {
                        avg_hunger + 1000
                    } else {
                        avg_hunger
                    };

                    hh_priorities.push(HouseholdPriority {
                        id: hh.id,
                        name: hh.name.clone(),
                        average_hunger: avg_hunger,
                        priority,
                        is_favored,
                        members: hh.member_ids.clone(),
                    });
                }

                hh_priorities.sort_by(|a, b| b.priority.cmp(&a.priority));

                let mut grains_left = total_grains;
                let mut served_households = Vec::new();

                for hh_pri in &hh_priorities {
                    let mut needed = (hh_pri.members.len() as i32).max(1);
                    if self.has_active_policy_effect(|effect| {
                        matches!(effect, PolicyEffect::RationingRule { .. })
                    }) {
                        needed = (needed / 2).max(1);
                    }
                    if grains_left >= needed {
                        let mut remaining_to_deduct = needed;
                        for &farm_id in &farm_est_ids {
                            if remaining_to_deduct <= 0 {
                                break;
                            }
                            if let Some(farm) = self.establishment_by_id_mut(farm_id) {
                                if let Some(stack) =
                                    farm.stock.iter_mut().find(|s| s.resource_id == "graos")
                                {
                                    let take = stack.amount.min(remaining_to_deduct);
                                    stack.amount -= take;
                                    remaining_to_deduct -= take;
                                }
                            }
                        }

                        if let Some(hh) = self.household_by_id_mut(hh_pri.id) {
                            if let Some(stack) =
                                hh.pantry.iter_mut().find(|s| s.resource_id == "graos")
                            {
                                stack.amount += needed;
                            } else {
                                hh.pantry.push(ResourceStack {
                                    resource_id: "graos".to_string(),
                                    amount: needed,
                                });
                            }
                        }

                        grains_left -= needed;
                        served_households.push(hh_pri.id);

                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: leader_id.unwrap_or(0),
                            target: hh_pri.members.first().copied(),
                            kind: EventKind::Commerce,
                            summary: format!(
                                "LÃ­der distribuiu {} grÃ£o(s) para {} (Favorecido: {}).",
                                needed, hh_pri.name, hh_pri.is_favored
                            ),
                            impact_tags: vec![
                                "racionamento".to_string(),
                                "distribuicao".to_string(),
                            ],
                        });
                    } else {
                        let was_prejudiced = hh_priorities.iter().any(|other| {
                            served_households.contains(&other.id)
                                && other.is_favored
                                && other.average_hunger < hh_pri.average_hunger
                        });

                        if was_prejudiced {
                            for &m_id in &hh_pri.members {
                                if let Ok(ent) = self.find_agent_entity(m_id) {
                                    let mut entry = self.world.entity_mut(ent);
                                    if let Some(mut state) = entry.get_mut::<StateComponent>() {
                                        state.0.stress = (state.0.stress + 20).clamp(0, 100);
                                    }
                                }
                                if let Some(lid) = leader_id {
                                    self.apply_relation_delta(
                                        m_id,
                                        lid,
                                        &RelationDelta {
                                            resentment: 15,
                                            trust: -10,
                                            friendship: -10,
                                            ..Default::default()
                                        },
                                    )?;
                                }
                            }

                            self.push_event(WorldEvent {
                                day: self.day,
                                tick: self.tick_of_day,
                                actor: hh_pri.members.first().copied().unwrap_or(0),
                                target: leader_id,
                                kind: EventKind::Conflict,
                                summary: format!(
                                    "Membros de {} sentem-se injustiÃ§ados pelo nepotismo do LÃ­der na distribuiÃ§Ã£o de grÃ£os.",
                                    hh_pri.name
                                ),
                                impact_tags: vec!["racionamento".to_string(), "nepotismo".to_string(), "conflito".to_string()],
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub(super) fn recalculate_posted_prices(
        &self,
        establishment: &EstablishmentEconomy,
    ) -> Vec<PostedPrice> {
        let mut prices = establishment
            .stock_targets
            .iter()
            .map(|target| {
                let current =
                    self.establishment_total_resource_units(establishment, &target.resource_id);
                let shortage = (target.amount - current).max(0);
                let mut unit_price = self.base_price(&target.resource_id) + shortage / 2;
                if establishment.cash < 10 {
                    unit_price += 1;
                }
                PostedPrice {
                    resource_id: target.resource_id.clone(),
                    unit_price: unit_price.max(1),
                }
            })
            .collect::<Vec<_>>();

        let unique_item_resources = establishment
            .item_stock_ids
            .iter()
            .filter_map(|item_id| self.item_instance(*item_id))
            .map(|item| item.resource_id.clone())
            .collect::<std::collections::HashSet<_>>();
        for resource_id in unique_item_resources {
            if prices.iter().any(|price| price.resource_id == resource_id) {
                continue;
            }
            let Some(best_item) = establishment
                .item_stock_ids
                .iter()
                .filter_map(|item_id| self.item_instance(*item_id))
                .filter(|item| item.resource_id == resource_id)
                .max_by_key(|item| item.craft_quality_score)
            else {
                continue;
            };
            let unit_price =
                self.item_instance_unit_price(best_item, self.base_price(&resource_id));
            prices.push(PostedPrice {
                resource_id,
                unit_price,
            });
        }
        prices
    }

    pub(super) fn compute_scarcity_metrics(&self) -> Vec<ScarcityMetric> {
        let mut metrics = Vec::new();
        for resource in self
            .catalog
            .resources
            .iter()
            .filter(|resource| !resource.tags.iter().any(|tag| tag == "currency"))
        {
            let available: i32 = self
                .establishments
                .iter()
                .map(|establishment| {
                    self.establishment_total_resource_units(establishment, &resource.id)
                })
                .sum::<i32>()
                + self
                    .households
                    .iter()
                    .map(|household| Self::total_resource_amount(&household.pantry, &resource.id))
                    .sum::<i32>();
            let target: i32 = self
                .establishments
                .iter()
                .map(|establishment| self.stock_target_amount(establishment, &resource.id))
                .sum();
            metrics.push(ScarcityMetric {
                resource_id: resource.id.clone(),
                pressure: (target - available).max(0),
            });
        }
        metrics
    }

    pub(super) fn ensure_economic_tasks(&mut self) {
        self.economic_tasks.retain(|task| {
            task.phase != EconomicTaskPhase::Completed && task.phase != EconomicTaskPhase::Failed
        });
        self.ensure_construction_projects();
        self.ensure_local_production_tasks();
        if self.food_supply_emergency() {
            self.ensure_establishment_supply_tasks();
            self.ensure_household_food_tasks();
        } else {
            self.ensure_household_food_tasks();
            self.ensure_establishment_supply_tasks();
        }
        self.ensure_construction_tasks();
        self.ensure_military_supply_tasks();
        self.ensure_payment_tasks();
        self.ensure_surplus_sale_tasks();
    }

    pub(super) fn recalculate_military_demand_status(demand: &mut MilitaryDemand) {
        let required_units: i32 = demand
            .required
            .iter()
            .map(|stack| stack.amount.max(0))
            .sum();
        let delivered_units: i32 = demand
            .delivered
            .iter()
            .map(|stack| stack.amount.max(0))
            .sum();
        let required_total = required_units + demand.cash_required.max(0);
        let delivered_total = delivered_units + demand.cash_delivered.max(0);
        demand.shortage_score = (required_total - delivered_total).max(0);
        demand.status = if demand.shortage_score <= 0 {
            MilitaryDemandStatus::Satisfied
        } else if delivered_total > 0 {
            MilitaryDemandStatus::PartiallySupplied
        } else {
            MilitaryDemandStatus::Open
        };
    }

    pub(super) fn missing_military_resources_for_demand(
        demand: &MilitaryDemand,
    ) -> Vec<ResourceStack> {
        demand
            .required
            .iter()
            .filter_map(|required| {
                let delivered =
                    Self::total_resource_amount(&demand.delivered, &required.resource_id);
                let missing = (required.amount - delivered).max(0);
                (missing > 0).then(|| ResourceStack {
                    resource_id: required.resource_id.clone(),
                    amount: missing,
                })
            })
            .collect()
    }

    pub(super) fn demanded_military_resource_pressure(&self, resource_id: &str) -> i32 {
        self.military_demands
            .iter()
            .filter(|demand| {
                matches!(
                    demand.status,
                    MilitaryDemandStatus::Open | MilitaryDemandStatus::PartiallySupplied
                )
            })
            .flat_map(Self::missing_military_resources_for_demand)
            .filter(|stack| stack.resource_id == resource_id)
            .map(|stack| stack.amount)
            .sum()
    }

    pub(super) fn military_supply_actor_household_id(&mut self) -> Option<BuildingId> {
        let mut query = self.world.query::<(&AgentCore, &LifeStatusComponent)>();
        query
            .iter(&self.world)
            .find_map(|(core, life)| {
                (life.0 == AgentLifeStatus::Vivo && core.role_id == Role::Guard.id())
                    .then_some(core.home_building_id)
                    .flatten()
            })
            .or_else(|| self.construction_actor_household_id())
            .or_else(|| self.households.first().map(|household| household.id))
    }

    pub(super) fn best_military_supply_source(
        &self,
        resource_id: &str,
    ) -> Option<(EconomicNode, Option<EstablishmentId>, i32)> {
        self.establishments
            .iter()
            .filter(|establishment| {
                Self::total_resource_amount(&establishment.stock, resource_id) > 0
            })
            .max_by_key(|establishment| {
                Self::total_resource_amount(&establishment.stock, resource_id)
            })
            .map(|establishment| {
                (
                    EconomicNode::Establishment(establishment.id),
                    Some(establishment.id),
                    0,
                )
            })
    }

    pub(super) fn living_agent_count(&mut self) -> usize {
        let mut query = self.world.query::<&LifeStatusComponent>();
        query
            .iter(&self.world)
            .filter(|life| life.0 == AgentLifeStatus::Vivo)
            .count()
    }

    pub(super) fn has_open_construction_project_for_type(
        &self,
        establishment_type_id: &str,
    ) -> bool {
        self.construction_projects.iter().any(|project| {
            project.establishment_type_id == establishment_type_id
                && !matches!(
                    project.status,
                    ConstructionStatus::Completed
                        | ConstructionStatus::Blocked
                        | ConstructionStatus::Cancelled
                )
        })
    }

    pub(super) fn open_construction_project(
        &mut self,
        establishment_type_id: &str,
        systemic_reason: String,
        priority: u8,
        funding_polity_id: Option<PolityId>,
    ) {
        let Some(establishment_type) = self.establishment_type_def(establishment_type_id).cloned()
        else {
            return;
        };
        let Some(recipe_id) = establishment_type.construction_recipe_id.clone() else {
            return;
        };
        let Some(recipe) = self
            .catalog
            .construction_recipes
            .iter()
            .find(|recipe| recipe.id == recipe_id)
            .cloned()
        else {
            return;
        };
        let Some((planned_footprint, entrance)) =
            self.plan_construction_site(establishment_type.location_kind, funding_polity_id)
        else {
            return;
        };
        let project_id = self.next_construction_project_id;
        self.next_construction_project_id += 1;
        let building_name = format!("{} {}", establishment_type.display_name, project_id);
        self.construction_projects.push(ConstructionProject {
            id: project_id,
            establishment_type_id: establishment_type_id.to_string(),
            building_name: building_name.clone(),
            planned_footprint,
            entrance,
            materials_required: recipe
                .materials
                .iter()
                .map(|input| ResourceStack {
                    resource_id: input.resource_id.clone(),
                    amount: input.amount,
                })
                .collect(),
            materials_delivered: Vec::new(),
            labor_required: recipe.labor_cost,
            labor_done: 0,
            status: ConstructionStatus::Planned,
            priority,
            systemic_reason: systemic_reason.clone(),
            resulting_building_id: None,
            funding_polity_id,
        });
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: 0,
            target: None,
            kind: EventKind::Construction,
            summary: format!("Projeto aberto: {building_name} por {systemic_reason}."),
            impact_tags: vec![
                "construcao".to_string(),
                format!("project:{project_id}"),
                establishment_type_id.to_string(),
            ],
        });
    }

    pub(super) fn plan_construction_site(
        &self,
        kind: LocationKind,
        restrict_to_polity: Option<PolityId>,
    ) -> Option<(Vec<TileCoord>, TileCoord)> {
        let (width, height) = match kind {
            LocationKind::Home => (7, 5),
            LocationKind::Farm | LocationKind::Woodlot | LocationKind::Quarry => (9, 7),
            LocationKind::Manor => (11, 7),
            _ => (9, 5),
        };
        let occupied_by_projects = self
            .construction_projects
            .iter()
            .filter(|project| {
                !matches!(
                    project.status,
                    ConstructionStatus::Completed
                        | ConstructionStatus::Blocked
                        | ConstructionStatus::Cancelled
                )
            })
            .flat_map(|project| project.planned_footprint.iter().copied())
            .collect::<HashSet<_>>();

        let polity_tiles: Option<HashSet<TileCoord>> = restrict_to_polity.map(|polity_id| {
            self.territories
                .iter()
                .filter(|t| t.controller_polity_id == polity_id)
                .flat_map(|t| t.tile_coords.iter().copied())
                .collect()
        });

        for y in 2..(self.spatial.grid.height - height - 2).max(2) {
            for x in 2..(self.spatial.grid.width - width - 2).max(2) {
                let footprint = (y..y + height)
                    .flat_map(|yy| (x..x + width).map(move |xx| TileCoord { x: xx, y: yy }))
                    .collect::<Vec<_>>();
                if let Some(ref tiles) = polity_tiles {
                    if !footprint.iter().all(|coord| tiles.contains(coord)) {
                        continue;
                    }
                }
                if !footprint
                    .iter()
                    .all(|coord| self.is_buildable_tile(*coord, &occupied_by_projects))
                {
                    continue;
                }
                if let Some(entrance) = footprint.iter().copied().find(|coord| {
                    self.is_border_coord(*coord, x, y, width, height)
                        && coord.neighbors4().into_iter().any(|neighbor| {
                            !footprint.contains(&neighbor)
                                && self
                                    .tile_at(neighbor)
                                    .map(|tile| tile.kind == TileKind::Road)
                                    .unwrap_or(false)
                        })
                }) {
                    return Some((footprint, entrance));
                }
            }
        }
        None
    }

    pub(super) fn is_buildable_tile(
        &self,
        coord: TileCoord,
        reserved: &HashSet<TileCoord>,
    ) -> bool {
        if reserved.contains(&coord) {
            return false;
        }
        let Some(tile) = self.tile_at(coord) else {
            return false;
        };
        tile.kind == TileKind::Grass
            && tile.building_id.is_none()
            && self
                .spatial
                .fixtures
                .iter()
                .all(|fixture| fixture.coord != coord)
    }

    pub(super) fn is_border_coord(
        &self,
        coord: TileCoord,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> bool {
        coord.x == x || coord.x == x + width - 1 || coord.y == y || coord.y == y + height - 1
    }

    pub(super) fn construction_actor_household_id(&mut self) -> Option<BuildingId> {
        let mut query = self.world.query::<(&AgentCore, &LifeStatusComponent)>();
        query
            .iter(&self.world)
            .find_map(|(core, life)| {
                (life.0 == AgentLifeStatus::Vivo && core.role_id == "campones")
                    .then_some(core.home_building_id)
                    .flatten()
            })
            .or_else(|| self.households.first().map(|household| household.id))
    }

    pub(super) fn has_open_construction_material_task(
        &self,
        project_id: u64,
        resource_id: &str,
    ) -> bool {
        self.economic_tasks.iter().any(|task| {
            task.kind == EconomicTaskKind::Construir
                && task.related_construction_project_id == Some(project_id)
                && task.resource_id.as_deref() == Some(resource_id)
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
        })
    }

    pub(super) fn has_open_construction_labor_task(&self, project_id: u64) -> bool {
        self.economic_tasks.iter().any(|task| {
            task.kind == EconomicTaskKind::Construir
                && task.related_construction_project_id == Some(project_id)
                && task.resource_id.is_none()
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
        })
    }

    pub(super) fn best_construction_material_source(
        &self,
        resource_id: &str,
    ) -> Option<(EconomicNode, Option<EstablishmentId>, i32)> {
        if let Some(establishment) = self
            .establishments
            .iter()
            .filter(|establishment| {
                Self::total_resource_amount(&establishment.stock, resource_id) > 0
            })
            .min_by_key(|establishment| {
                establishment
                    .posted_prices
                    .iter()
                    .find(|price| price.resource_id == resource_id)
                    .map(|price| price.unit_price)
                    .unwrap_or_else(|| self.base_price(resource_id))
            })
        {
            let unit_price = establishment
                .posted_prices
                .iter()
                .find(|price| price.resource_id == resource_id)
                .map(|price| price.unit_price)
                .unwrap_or_else(|| self.base_price(resource_id));
            return Some((
                EconomicNode::Establishment(establishment.id),
                Some(establishment.id),
                unit_price,
            ));
        }
        None
    }

    pub(super) fn try_complete_construction_project(&mut self, project_id: u64) {
        let Some(project) = self
            .construction_projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
        else {
            return;
        };
        if matches!(
            project.status,
            ConstructionStatus::Completed
                | ConstructionStatus::Blocked
                | ConstructionStatus::Cancelled
        ) {
            return;
        }
        let materials_ready = project.materials_required.iter().all(|required| {
            Self::total_resource_amount(&project.materials_delivered, &required.resource_id)
                >= required.amount
        });
        if !materials_ready || project.labor_done < project.labor_required {
            return;
        }
        let Some(building_id) = self.materialize_construction_project(&project) else {
            if let Some(project_state) = self
                .construction_projects
                .iter_mut()
                .find(|entry| entry.id == project_id)
            {
                project_state.status = ConstructionStatus::Blocked;
            }
            return;
        };
        if let Some(project_state) = self
            .construction_projects
            .iter_mut()
            .find(|entry| entry.id == project_id)
        {
            project_state.status = ConstructionStatus::Completed;
            project_state.resulting_building_id = Some(building_id);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: 0,
            target: None,
            kind: EventKind::Construction,
            summary: format!(
                "Obra concluida: {} agora existe no grid.",
                project.building_name
            ),
            impact_tags: vec![
                "construcao".to_string(),
                "concluida".to_string(),
                format!("building:{building_id}"),
            ],
        });
    }

    pub(super) fn materialize_construction_project(
        &mut self,
        project: &ConstructionProject,
    ) -> Option<BuildingId> {
        let establishment_type = self
            .establishment_type_def(&project.establishment_type_id)?
            .clone();
        if !project.planned_footprint.iter().all(|coord| {
            self.tile_at(*coord)
                .map(|tile| tile.building_id.is_none())
                .unwrap_or(false)
        }) {
            return None;
        }
        let min_x = project
            .planned_footprint
            .iter()
            .map(|coord| coord.x)
            .min()?;
        let max_x = project
            .planned_footprint
            .iter()
            .map(|coord| coord.x)
            .max()?;
        let min_y = project
            .planned_footprint
            .iter()
            .map(|coord| coord.y)
            .min()?;
        let max_y = project
            .planned_footprint
            .iter()
            .map(|coord| coord.y)
            .max()?;
        let building_id = self.next_building_id();
        let room_id = self.next_room_id();
        let room_tiles = project
            .planned_footprint
            .iter()
            .copied()
            .filter(|coord| {
                coord.x > min_x && coord.x < max_x && coord.y > min_y && coord.y < max_y
            })
            .collect::<Vec<_>>();
        let width = max_x - min_x + 1;
        let height = max_y - min_y + 1;
        let grid_width = self.spatial.grid.width;
        for coord in &project.planned_footprint {
            let kind = if *coord == project.entrance {
                TileKind::Door
            } else if self.is_border_coord(*coord, min_x, min_y, width, height) {
                TileKind::Wall
            } else {
                TileKind::Floor
            };
            let tile = self
                .spatial
                .grid
                .tiles
                .get_mut((coord.y * grid_width + coord.x) as usize)?;
            tile.building_id = Some(building_id);
            tile.room_id = Some(room_id);
            tile.kind = kind;
        }
        self.spatial.rooms.push(RoomSpec {
            id: room_id,
            building_id,
            name: format!("Sala de {}", project.building_name),
            kind: establishment_type.display_name.clone(),
            tiles: room_tiles.clone(),
        });
        self.spatial.buildings.push(BuildingSpec {
            id: building_id,
            name: project.building_name.clone(),
            kind: establishment_type.location_kind,
            entrance: project.entrance,
            room_ids: vec![room_id],
            footprint: project.planned_footprint.clone(),
        });

        let storage_fixture_id = self.add_constructed_fixtures(
            building_id,
            room_id,
            &room_tiles,
            &project.establishment_type_id,
        );
        if establishment_type.location_kind == LocationKind::Home {
            self.households.push(HouseholdEconomy {
                id: building_id,
                name: project.building_name.clone(),
                member_ids: Vec::new(),
                treasury: 12,
                pantry: establishment_type.default_stock.clone(),
                reserved_food: Vec::new(),
                minimum_food_units: 4,
                pending_payments: Vec::new(),
                scarcity_pressure: 0,
                food_crisis_level: 0,
                reserved_food_workers: 0,
                last_food_shortage_tick: 0,
                tax_arrears: 0,
                last_tax_paid_day: self.day,
                direct_lord_agent_id: None,
                feudal_tribute_due: 0,
                corvee_days_due: 0,
                levy_service_due: 0,
                feudal_arrears: 0,
            });
        } else {
            let establishment_id = self.next_establishment_id();
            let mut establishment = EstablishmentEconomy {
                id: establishment_id,
                building_id: Some(building_id),
                name: project.building_name.clone(),
                establishment_type_id: establishment_type.id.clone(),
                location_kind: establishment_type.location_kind,
                owner_household_ids: self
                    .owner_households_for_policy(&establishment_type.owner_policy),
                storage_fixture_id,
                cash: 20,
                stock: establishment_type.default_stock.clone(),
                item_stock_ids: Vec::new(),
                stock_targets: establishment_type.stock_targets.clone(),
                posted_prices: Vec::new(),
                wage_per_shift: establishment_type.wage_per_shift,
                tool_wear: 0,
                public_service: establishment_type.public_service,
            };
            establishment.posted_prices = self.recalculate_posted_prices(&establishment);
            self.establishments.push(establishment);
        }
        if let Some(first_coord) = project.planned_footprint.first() {
            if let Some(territory) = self
                .territories
                .iter_mut()
                .find(|t| t.tile_coords.contains(first_coord))
            {
                if !territory.building_ids.contains(&building_id) {
                    territory.building_ids.push(building_id);
                }
            }
        }
        self.sync_establishment_stocks_to_fixtures();
        self.sync_household_pantries_to_fixtures();
        Some(building_id)
    }

    pub(super) fn add_constructed_fixtures(
        &mut self,
        building_id: BuildingId,
        room_id: RoomId,
        room_tiles: &[TileCoord],
        establishment_type_id: &str,
    ) -> Option<FixtureId> {
        let required_fixtures = self
            .establishment_type_def(establishment_type_id)
            .and_then(|establishment_type| establishment_type.construction_recipe_id.as_ref())
            .and_then(|recipe_id| {
                self.catalog
                    .construction_recipes
                    .iter()
                    .find(|recipe| &recipe.id == recipe_id)
            })
            .map(|recipe| recipe.required_fixtures.clone())
            .unwrap_or_default();
        let mut storage_fixture_id = None;
        for (index, kind) in required_fixtures.into_iter().enumerate() {
            let Some(coord) = room_tiles.get(index % room_tiles.len().max(1)).copied() else {
                continue;
            };
            let fixture_id = self.next_fixture_id();
            if kind == FixtureKind::Storage {
                storage_fixture_id = Some(fixture_id);
            }
            self.spatial.fixtures.push(FixtureSpec {
                id: fixture_id,
                building_id: Some(building_id),
                room_id: Some(room_id),
                kind,
                coord,
                name: format!("{} {}", kind.as_str(), fixture_id),
                blocks_movement: matches!(
                    kind,
                    FixtureKind::Bed | FixtureKind::Table | FixtureKind::Storage
                ),
                stock: Vec::new(),
            });
        }
        storage_fixture_id
    }

    pub(super) fn next_building_id(&self) -> BuildingId {
        self.spatial
            .buildings
            .iter()
            .map(|building| building.id)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub(super) fn next_room_id(&self) -> RoomId {
        self.spatial
            .rooms
            .iter()
            .map(|room| room.id)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub(super) fn next_fixture_id(&self) -> FixtureId {
        self.spatial
            .fixtures
            .iter()
            .map(|fixture| fixture.id)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub(super) fn next_establishment_id(&self) -> EstablishmentId {
        self.establishments
            .iter()
            .map(|establishment| establishment.id)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub(super) fn owner_households_for_policy(
        &mut self,
        policy: &crate::world_model::OwnerPolicyDef,
    ) -> Vec<BuildingId> {
        match policy {
            crate::world_model::OwnerPolicyDef::Civic => Vec::new(),
            crate::world_model::OwnerPolicyDef::PrivateByRole { role_id } => self
                .household_ids_for_role(role_id)
                .into_iter()
                .next()
                .into_iter()
                .collect(),
            crate::world_model::OwnerPolicyDef::SharedByRoles { role_ids } => role_ids
                .iter()
                .flat_map(|role_id| self.household_ids_for_role(role_id))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
        }
    }

    pub(super) fn household_ids_for_role(&mut self, role_id: &str) -> Vec<BuildingId> {
        let mut query = self.world.query::<(&AgentCore, &LifeStatusComponent)>();
        query
            .iter(&self.world)
            .filter_map(|(core, life)| {
                (life.0 == AgentLifeStatus::Vivo && core.role_id == role_id)
                    .then_some(core.home_building_id)
                    .flatten()
            })
            .collect()
    }

    pub(super) fn ensure_construction_projects(&mut self) {
        let living_agents = self.living_agent_count();
        let beds = self
            .spatial
            .fixtures
            .iter()
            .filter(|fixture| fixture.kind == FixtureKind::Bed)
            .count();
        if living_agents > beds && !self.has_open_construction_project_for_type("casa") {
            let deficit = living_agents - beds;
            self.open_construction_project(
                "casa",
                format!("falta de camas para {deficit} agente(s)"),
                95,
                None,
            );
        }

        let metrics = self.village_economy.scarcity_metrics.clone();
        for metric in metrics.into_iter().filter(|metric| metric.pressure >= 8) {
            let Some(recipe) = self
                .catalog
                .recipes
                .iter()
                .find(|recipe| recipe.output_resource_id == metric.resource_id)
                .cloned()
            else {
                continue;
            };
            if self.has_open_construction_project_for_type(&recipe.establishment_type_id) {
                continue;
            }
            let Some(establishment_type) =
                self.establishment_type_def(&recipe.establishment_type_id)
            else {
                continue;
            };
            if establishment_type.construction_recipe_id.is_none() {
                continue;
            }
            self.open_construction_project(
                &recipe.establishment_type_id,
                format!(
                    "deficit persistente de {} ({})",
                    self.resource_display_name(&metric.resource_id),
                    metric.pressure
                ),
                70,
                None,
            );
        }
    }

    pub(super) fn ensure_construction_tasks(&mut self) {
        let Some(actor_household_id) = self.construction_actor_household_id() else {
            return;
        };
        let projects = self.construction_projects.clone();
        for project in projects {
            if matches!(
                project.status,
                ConstructionStatus::Completed
                    | ConstructionStatus::Blocked
                    | ConstructionStatus::Cancelled
            ) {
                continue;
            }
            let mut missing_material = false;
            for required in &project.materials_required {
                let delivered = Self::total_resource_amount(
                    &project.materials_delivered,
                    &required.resource_id,
                );
                let missing = (required.amount - delivered).max(0);
                if missing <= 0 {
                    continue;
                }
                missing_material = true;
                if self.has_open_construction_material_task(project.id, &required.resource_id) {
                    continue;
                }
                let Some((source, related_establishment_id, unit_price)) =
                    self.best_construction_material_source(&required.resource_id)
                else {
                    continue;
                };
                let amount = missing.clamp(1, DEFAULT_CARRYING_CAPACITY);
                let task_id = self.next_task_id();
                self.economic_tasks.push(EconomicTask {
                    id: task_id,
                    kind: EconomicTaskKind::Construir,
                    class: EconomicTaskClass::Construction,
                    priority: project.priority,
                    lock_until_complete: true,
                    creates_household_reserve: false,
                    actor_household_id,
                    assigned_agent_id: None,
                    source,
                    destination: EconomicNode::ConstructionProject(project.id),
                    resource_id: Some(required.resource_id.clone()),
                    amount,
                    unit_price,
                    total_price: unit_price * amount,
                    description: format!(
                        "Levar {} x{} para obra de {}",
                        self.resource_display_name(&required.resource_id),
                        amount,
                        project.building_name
                    ),
                    phase: EconomicTaskPhase::AwaitingPickup,
                    related_establishment_id,
                    related_construction_project_id: Some(project.id),
                });
            }
            if missing_material || self.has_open_construction_labor_task(project.id) {
                continue;
            }
            let remaining_labor = project.labor_required - project.labor_done;
            if remaining_labor <= 0 {
                self.try_complete_construction_project(project.id);
                continue;
            }
            let task_id = self.next_task_id();
            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::Construir,
                class: EconomicTaskClass::Construction,
                priority: project.priority,
                lock_until_complete: true,
                creates_household_reserve: false,
                actor_household_id,
                assigned_agent_id: None,
                source: EconomicNode::ConstructionProject(project.id),
                destination: EconomicNode::ConstructionProject(project.id),
                resource_id: None,
                amount: remaining_labor.clamp(1, 3),
                unit_price: 0,
                total_price: 0,
                description: format!("Trabalhar na obra de {}", project.building_name),
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: None,
                related_construction_project_id: Some(project.id),
            });
        }
    }

    pub(super) fn has_open_task_for(
        &self,
        household_id: BuildingId,
        kind: EconomicTaskKind,
        resource_id: Option<&str>,
        destination: &EconomicNode,
    ) -> bool {
        self.economic_tasks.iter().any(|task| {
            task.actor_household_id == household_id
                && task.kind == kind
                && task.resource_id.as_deref() == resource_id
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
                && &task.destination == destination
        })
    }

    pub(super) fn open_task_count_for_household_class(
        &self,
        household_id: BuildingId,
        class: EconomicTaskClass,
    ) -> usize {
        self.economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.class == class
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
            })
            .count()
    }

    pub(super) fn matching_open_task_count(
        &self,
        household_id: BuildingId,
        kind: EconomicTaskKind,
        resource_id: Option<&str>,
        destination: &EconomicNode,
        source: Option<&EconomicNode>,
    ) -> usize {
        self.economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.kind == kind
                    && task.resource_id.as_deref() == resource_id
                    && task.destination == *destination
                    && source
                        .map(|expected| task.source == *expected)
                        .unwrap_or(true)
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
            })
            .count()
    }

    pub(super) fn village_food_pressure(&self) -> i32 {
        let household_pressure: i32 = self
            .households
            .iter()
            .map(|household| household.scarcity_pressure)
            .sum();
        let market_pressure: i32 = self
            .village_economy
            .scarcity_metrics
            .iter()
            .filter(|metric| self.is_food_resource(&metric.resource_id))
            .map(|metric| metric.pressure)
            .sum();
        household_pressure + market_pressure
    }

    pub(super) fn household_member_count_with_need(
        &mut self,
        household_id: BuildingId,
        hunger_at_least: i32,
    ) -> usize {
        let member_ids = self
            .household_by_id(household_id)
            .map(|household| household.member_ids.clone())
            .unwrap_or_default();
        member_ids
            .into_iter()
            .filter(|agent_id| {
                self.agent_state(*agent_id)
                    .map(|state| state.hunger >= hunger_at_least)
                    .unwrap_or(false)
            })
            .count()
    }

    pub(super) fn household_food_worker_limit(&self, household_id: BuildingId) -> usize {
        let Some(household) = self.household_by_id(household_id) else {
            return 0;
        };
        if household.food_crisis_level == 0 {
            return 0;
        }
        // Crise nÃ­vel 2+: todos os membros podem ajudar com comida
        if household.food_crisis_level >= 2 {
            return household.member_ids.len();
        }
        // Crise nÃ­vel 1: atÃ© metade dos membros (mÃ­nimo 1)
        let max_workers = if household.member_ids.len() > 2 {
            (household.member_ids.len() + 1) / 2
        } else {
            1
        };
        max_workers.min(household.member_ids.len())
    }

    pub(super) fn household_assigned_food_support_workers(
        &self,
        household_id: BuildingId,
    ) -> usize {
        let member_ids = self
            .household_by_id(household_id)
            .map(|household| household.member_ids.clone())
            .unwrap_or_default();
        self.economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.class.is_food_support()
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
                    && task
                        .assigned_agent_id
                        .map(|agent_id| member_ids.contains(&agent_id))
                        .unwrap_or(false)
            })
            .count()
    }

    pub(super) fn allow_food_support_assignment(
        &self,
        household_id: BuildingId,
        agent_id: u64,
        task: &EconomicTask,
    ) -> bool {
        if !task.class.is_food_support() {
            return true;
        }
        if task.assigned_agent_id == Some(agent_id) {
            return true;
        }
        self.household_assigned_food_support_workers(household_id)
            < self.household_food_worker_limit(household_id)
    }

    pub(super) fn next_task_id(&mut self) -> EconomicTaskId {
        let task_id = self.next_economic_task_id;
        self.next_economic_task_id += 1;
        task_id
    }

    pub(super) fn ensure_household_food_tasks(&mut self) {
        let households = self.households.clone();
        for household in households {
            let food_units = Self::total_food_units(&household.pantry)
                + Self::total_food_units(&household.reserved_food);
            if food_units >= household.minimum_food_units {
                continue;
            }
            let destination = EconomicNode::HouseholdPantry(household.id);
            let max_purchase_tasks = self.household_food_worker_limit(household.id).max(1);
            let existing_purchase_tasks = self.open_task_count_for_household_class(
                household.id,
                EconomicTaskClass::HouseholdFoodPurchase,
            );
            if existing_purchase_tasks >= max_purchase_tasks {
                continue;
            }
            let mut remaining_deficit = (household.minimum_food_units - food_units).max(0);
            let slots_to_fill = max_purchase_tasks.saturating_sub(existing_purchase_tasks);
            for _ in 0..slots_to_fill {
                if remaining_deficit <= 0 {
                    break;
                }
                let Some(offer) = self.best_food_source_for_household(household.id) else {
                    break;
                };
                let amount = remaining_deficit.clamp(2, DEFAULT_CARRYING_CAPACITY);
                let task_id = self.next_task_id();
                let is_immediate = self.household_has_critical_hunger(household.id);
                let description = if offer.resource_id == ResourceKind::Graos.id() {
                    if is_immediate {
                        format!(
                            "Comprar {} x{} para segurar a fome imediata de {}",
                            self.resource_display_name(&offer.resource_id),
                            amount,
                            household.name
                        )
                    } else {
                        format!(
                            "Comprar {} x{} para recompor a cadeia alimentar de {}",
                            self.resource_display_name(&offer.resource_id),
                            amount,
                            household.name
                        )
                    }
                } else {
                    format!(
                        "Comprar {} x{} para consumo imediato de {}",
                        self.resource_display_name(&offer.resource_id),
                        amount,
                        household.name
                    )
                };
                let access_priority = self.household_food_access_priority(household.id);
                let policy_priority = match self.local_norms.rationing_policy {
                    RationingPolicy::HouseholdFirst => 100,
                    RationingPolicy::ProducersFirst => {
                        90u8.saturating_sub((2 - household.food_crisis_level.min(2)) * 10)
                    }
                    RationingPolicy::CivicFirst => access_priority.max(75),
                    RationingPolicy::Balanced => {
                        100u8.saturating_sub((2 - household.food_crisis_level.min(2)) * 10)
                    }
                };
                self.economic_tasks.push(EconomicTask {
                    id: task_id,
                    kind: EconomicTaskKind::Comprar,
                    class: EconomicTaskClass::HouseholdFoodPurchase,
                    priority: policy_priority.max(access_priority),
                    lock_until_complete: true,
                    creates_household_reserve: offer.resource_id == ResourceKind::Graos.id(),
                    actor_household_id: household.id,
                    assigned_agent_id: None,
                    source: offer.source,
                    destination: destination.clone(),
                    resource_id: Some(offer.resource_id.clone()),
                    amount,
                    unit_price: offer.unit_price,
                    total_price: offer.unit_price * amount,
                    description,
                    phase: EconomicTaskPhase::AwaitingPickup,
                    related_establishment_id: offer.related_establishment_id,
                    related_construction_project_id: None,
                });
                remaining_deficit -= amount;
            }
        }
    }

    pub(super) fn ensure_local_production_tasks(&mut self) {
        let establishments = self.establishments.clone();
        for establishment in establishments {
            let recipes = self
                .recipes_for_establishment(&establishment)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            for recipe in recipes {
                let resource_id = recipe.output_resource_id.clone();
                let target = self.stock_target_amount(&establishment, &resource_id);
                let current = Self::total_resource_amount(&establishment.stock, &resource_id);
                if current >= target {
                    continue;
                }
                let Some(actor_household_id) = establishment.owner_household_ids.first().copied()
                else {
                    continue;
                };
                let destination = EconomicNode::Establishment(establishment.id);
                if self.has_open_task_for(
                    actor_household_id,
                    EconomicTaskKind::Produzir,
                    Some(&resource_id),
                    &destination,
                ) {
                    continue;
                }
                let amount = recipe
                    .output_amount
                    .clamp(1, DEFAULT_CARRYING_CAPACITY.max(1));
                let is_food = self.is_food_resource(&resource_id);
                let class = if is_food {
                    EconomicTaskClass::FoodProduction
                } else {
                    EconomicTaskClass::EssentialProduction
                };
                let priority = if is_food && self.village_food_pressure() > 0 {
                    let boost =
                        if self.local_norms.rationing_policy == RationingPolicy::ProducersFirst {
                            25
                        } else {
                            15
                        };
                    recipe.priority.saturating_add(boost)
                } else if self.demanded_military_resource_pressure(&resource_id) > 0 {
                    recipe.priority.saturating_add(30).min(100)
                } else {
                    recipe.priority
                };
                let task_id = self.next_task_id();
                self.economic_tasks.push(EconomicTask {
                    id: task_id,
                    kind: EconomicTaskKind::Produzir,
                    class,
                    priority,
                    lock_until_complete: true,
                    creates_household_reserve: false,
                    actor_household_id,
                    assigned_agent_id: None,
                    source: destination.clone(),
                    destination,
                    resource_id: Some(resource_id.clone()),
                    amount,
                    unit_price: 0,
                    total_price: 0,
                    description: format!(
                        "Produzir {} em {}",
                        self.resource_display_name(&resource_id),
                        establishment.name
                    ),
                    phase: EconomicTaskPhase::AwaitingPickup,
                    related_establishment_id: Some(establishment.id),
                    related_construction_project_id: None,
                });
            }
        }
    }

    pub(super) fn ensure_establishment_supply_tasks(&mut self) {
        let establishments = self.establishments.clone();
        let village_food_pressure = self.village_food_pressure();
        for establishment in establishments {
            let recipes = self
                .recipes_for_establishment(&establishment)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            for recipe in recipes {
                for input in recipe
                    .inputs
                    .iter()
                    .chain(recipe.capital_requirements.iter())
                {
                    let priority = if self.is_food_resource(&recipe.output_resource_id) {
                        if self.local_norms.rationing_policy == RationingPolicy::ProducersFirst {
                            100
                        } else if village_food_pressure > 0 {
                            95
                        } else {
                            85
                        }
                    } else {
                        recipe.priority.saturating_sub(10).max(35)
                    };
                    let class = if self.is_food_resource(&recipe.output_resource_id) {
                        EconomicTaskClass::FoodSupplyTransport
                    } else {
                        EconomicTaskClass::EssentialProduction
                    };
                    let max_open_tasks = if self.is_food_resource(&recipe.output_resource_id)
                        && village_food_pressure > 0
                    {
                        2
                    } else {
                        1
                    };
                    if let Some(source_establishment_type_id) = self
                        .catalog
                        .recipes
                        .iter()
                        .find(|candidate| candidate.output_resource_id == input.resource_id)
                        .map(|candidate| candidate.establishment_type_id.clone())
                    {
                        self.ensure_purchase_shortage_task(
                            &establishment,
                            &input.resource_id,
                            &source_establishment_type_id,
                            input.amount.max(1),
                            class,
                            priority,
                            max_open_tasks,
                        );
                    }
                }
            }
        }
    }

    pub(super) fn ensure_military_supply_tasks(&mut self) {
        let Some(local_polity_id) = self.polities.first().map(|polity| polity.id) else {
            return;
        };
        let Some(actor_household_id) = self.military_supply_actor_household_id() else {
            return;
        };
        let demands = self
            .military_demands
            .iter()
            .filter(|demand| {
                demand.polity_id == local_polity_id
                    && matches!(
                        demand.status,
                        MilitaryDemandStatus::Open | MilitaryDemandStatus::PartiallySupplied
                    )
            })
            .cloned()
            .collect::<Vec<_>>();
        for demand in demands {
            let destination = EconomicNode::MilitarySupply(demand.war_id);
            for missing in Self::missing_military_resources_for_demand(&demand) {
                if self.has_open_task_for(
                    actor_household_id,
                    EconomicTaskKind::Transportar,
                    Some(&missing.resource_id),
                    &destination,
                ) || self.has_open_task_for(
                    actor_household_id,
                    EconomicTaskKind::Comprar,
                    Some(&missing.resource_id),
                    &destination,
                ) {
                    continue;
                }
                let amount = missing.amount.clamp(1, DEFAULT_CARRYING_CAPACITY);
                let Some((source, related_establishment_id, unit_price)) =
                    self.best_military_supply_source(&missing.resource_id)
                else {
                    continue;
                };
                let kind = EconomicTaskKind::Transportar;
                let task_id = self.next_task_id();
                let display = self.resource_display_name(&missing.resource_id);
                let source_label = if let Some(establishment_id) = related_establishment_id {
                    self.establishment_by_id(establishment_id)
                        .map(|establishment| establishment.name.clone())
                        .unwrap_or_else(|| "estoque local".to_string())
                } else {
                    "estoque local".to_string()
                };
                self.economic_tasks.push(EconomicTask {
                    id: task_id,
                    kind,
                    class: EconomicTaskClass::MilitarySupply,
                    priority: demand.priority,
                    lock_until_complete: true,
                    creates_household_reserve: false,
                    actor_household_id,
                    assigned_agent_id: None,
                    source,
                    destination: destination.clone(),
                    resource_id: Some(missing.resource_id.clone()),
                    amount,
                    unit_price,
                    total_price: unit_price * amount,
                    description: format!(
                        "Suprir guerra #{} com {} x{} a partir de {}",
                        demand.war_id, display, amount, source_label
                    ),
                    phase: EconomicTaskPhase::AwaitingPickup,
                    related_establishment_id,
                    related_construction_project_id: None,
                });
            }
        }
    }

    pub(super) fn ensure_payment_tasks(&mut self) {
        let households = self.households.clone();
        for household in households {
            if household.pending_payments.is_empty() {
                continue;
            }
            let destination = EconomicNode::HouseholdPantry(household.id);
            if self.has_open_task_for(
                household.id,
                EconomicTaskKind::ReceberPagamento,
                Some(ResourceKind::Moedas.id()),
                &destination,
            ) {
                continue;
            }
            let total_amount: i32 = household
                .pending_payments
                .iter()
                .map(|claim| claim.amount)
                .sum();
            let task_id = self.next_task_id();
            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::ReceberPagamento,
                class: EconomicTaskClass::PaymentCollection,
                priority: 88,
                lock_until_complete: true,
                creates_household_reserve: false,
                actor_household_id: household.id,
                assigned_agent_id: None,
                source: EconomicNode::PublicTreasury,
                destination,
                resource_id: Some(ResourceKind::Moedas.id().to_string()),
                amount: total_amount.max(1),
                unit_price: 1,
                total_price: total_amount.max(1),
                description: format!("Receber pagamentos pendentes para {}", household.name),
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: None,
                related_construction_project_id: None,
            });
        }
    }

    pub(super) fn ensure_surplus_sale_tasks(&mut self) {
        // Surplus no longer sells into an abstract external market. Price can
        // influence priorities, but sale tasks require a material buyer/source
        // already present in the world. Inter-village demand must be expressed
        // as real Comprar/Transportar tasks against existing establishments.
    }

    pub(super) fn best_food_source_for_household(
        &mut self,
        household_id: BuildingId,
    ) -> Option<FoodSourceOffer> {
        let dest_village_idx = self.village_index_of_household(household_id)?;
        let food_order = self.preferred_food_resource_order_for_household(household_id);
        let mut offers = self
            .establishments
            .iter()
            .filter_map(|establishment| {
                let best_stock = food_order
                    .iter()
                    .into_iter()
                    .find(|resource_id| {
                        Self::total_resource_amount(&establishment.stock, resource_id.as_str()) > 0
                    })?
                    .clone();
                let unit_price = establishment
                    .posted_prices
                    .iter()
                    .find(|price| price.resource_id == best_stock)
                    .map(|price| price.unit_price)
                    .unwrap_or_else(|| self.base_price(&best_stock));

                let is_local =
                    self.village_index_of_establishment(establishment.id) == Some(dest_village_idx);
                let final_price = if is_local {
                    unit_price
                } else {
                    (unit_price as f64 * 1.3) as i32
                };
                Some(FoodSourceOffer {
                    source: EconomicNode::Establishment(establishment.id),
                    related_establishment_id: Some(establishment.id),
                    resource_id: best_stock,
                    unit_price: final_price,
                })
            })
            .collect::<Vec<_>>();

        offers.sort_by_key(|offer| {
            let is_local = match offer.source {
                EconomicNode::Establishment(establishment_id) => {
                    self.village_index_of_establishment(establishment_id) == Some(dest_village_idx)
                }

                _ => false,
            };
            (!is_local, offer.unit_price, offer.resource_id.clone())
        });

        let treasury = self
            .household_by_id(household_id)
            .map(|household| household.treasury)
            .unwrap_or(0);

        offers
            .into_iter()
            .filter(|offer| treasury >= offer.unit_price.max(0))
            .next()
    }

    pub(super) fn food_supply_emergency(&self) -> bool {
        let global_grain = self.total_village_resource_amount(ResourceKind::Graos.id());
        let stalled_processors = self.food_processors_missing_resource(ResourceKind::Graos.id());
        global_grain <= 6 || (global_grain <= 12 && stalled_processors > 0)
    }

    pub(super) fn food_crisis_assessment_for_household(
        &self,
        household_id: Option<BuildingId>,
    ) -> FoodCrisisAssessment {
        let household = household_id
            .and_then(|id| self.household_by_id(id))
            .cloned();
        let dest_village_idx = household
            .as_ref()
            .and_then(|household| self.village_index_of_household(household.id));
        let food_ids = self.food_resource_ids_sorted();
        let grain_id = ResourceKind::Graos.id().to_string();
        let household_food_units = household
            .as_ref()
            .map(|household| {
                Self::total_food_units(&household.pantry)
                    + Self::total_food_units(&household.reserved_food)
            })
            .unwrap_or(0);
        let household_minimum_food_units = household
            .as_ref()
            .map(|household| household.minimum_food_units.max(1))
            .unwrap_or(1);
        let household_daily_need = household
            .as_ref()
            .map(|household| household.member_ids.len().max(1) as i32)
            .unwrap_or(1)
            .max(1);
        let household_food_supply_days_tenths =
            (household_food_units * 10 / household_daily_need).clamp(0, 999);

        let mut village_grain_units = 0;
        let mut village_ready_food_units = 0;
        for establishment in &self.establishments {
            if dest_village_idx.is_some()
                && self.village_index_of_establishment(establishment.id) != dest_village_idx
            {
                continue;
            }
            village_grain_units += Self::total_resource_amount(&establishment.stock, &grain_id);
            for resource_id in &food_ids {
                if resource_id != &grain_id {
                    village_ready_food_units +=
                        Self::total_resource_amount(&establishment.stock, resource_id);
                }
            }
        }
        for household in &self.households {
            if dest_village_idx.is_some()
                && self.village_index_of_household(household.id) != dest_village_idx
            {
                continue;
            }
            village_grain_units += Self::total_resource_amount(&household.pantry, &grain_id)
                + Self::total_resource_amount(&household.reserved_food, &grain_id);
            for resource_id in &food_ids {
                if resource_id != &grain_id {
                    village_ready_food_units +=
                        Self::total_resource_amount(&household.pantry, resource_id)
                            + Self::total_resource_amount(&household.reserved_food, resource_id);
                }
            }
        }

        let mut material_food_source_count = 0usize;
        let mut inter_village_food_source_count = 0usize;
        let mut cheapest_food_price: Option<i32> = None;
        for establishment in &self.establishments {
            let has_food = food_ids.iter().any(|resource_id| {
                Self::total_resource_amount(&establishment.stock, resource_id) > 0
            });
            if !has_food {
                continue;
            }
            material_food_source_count += 1;
            if dest_village_idx.is_some()
                && self.village_index_of_establishment(establishment.id) != dest_village_idx
            {
                inter_village_food_source_count += 1;
            }
            for resource_id in &food_ids {
                if Self::total_resource_amount(&establishment.stock, resource_id) <= 0 {
                    continue;
                }
                let price = establishment
                    .posted_prices
                    .iter()
                    .find(|posted| posted.resource_id == *resource_id)
                    .map(|posted| posted.unit_price)
                    .unwrap_or_else(|| self.base_price(resource_id));
                cheapest_food_price =
                    Some(cheapest_food_price.map_or(price, |current| current.min(price)));
            }
        }

        let stalled_food_processors = self.food_processors_missing_resource(&grain_id);
        let food_supply_emergency = village_grain_units <= 6
            || household_food_units < household_minimum_food_units
            || (village_grain_units <= 12 && stalled_food_processors > 0);
        let household_treasury = household
            .as_ref()
            .map(|household| household.treasury)
            .unwrap_or(0);
        let household_feudal_arrears = household
            .as_ref()
            .map(|household| household.feudal_arrears.max(household.tax_arrears))
            .unwrap_or(0);
        let household_tribute_due = household
            .as_ref()
            .map(|household| household.feudal_tribute_due.max(0))
            .unwrap_or(0);

        let mut bottlenecks = Vec::new();
        if village_grain_units <= 6 {
            bottlenecks.push("graos_raiz_critico".to_string());
        }
        if stalled_food_processors > 0 {
            bottlenecks.push(format!("processadores_parados={stalled_food_processors}"));
        }
        if material_food_source_count == 0 {
            bottlenecks.push("sem_fornecedor_material".to_string());
        }
        if let Some(price) = cheapest_food_price {
            if household_treasury < price {
                bottlenecks.push("sem_caixa_para_compra".to_string());
            }
        } else {
            bottlenecks.push("sem_oferta_com_estoque".to_string());
        }
        if household_feudal_arrears > 0 || household_tribute_due > 0 {
            bottlenecks.push("pressao_feudal_fiscal".to_string());
        }

        let access_priority = household
            .as_ref()
            .map(|household| self.household_food_access_priority(household.id))
            .unwrap_or(50);
        let access_summary = if access_priority >= 90 {
            "acesso alimentar privilegiado por autoridade, oficio ou racionamento".to_string()
        } else if access_priority >= 70 {
            "acesso alimentar moderado por funcao produtiva ou caixa".to_string()
        } else if household_feudal_arrears > 0 || household_treasury <= 0 {
            "acesso fragil: pobreza, divida ou atraso feudal reduzem poder de compra".to_string()
        } else {
            "acesso alimentar comum, dependente de oferta material".to_string()
        };
        let political_cost_summary = if food_supply_emergency && access_priority >= 90 {
            "vantagem feudal em crise tende a corroer justica percebida nos prejudicados"
                .to_string()
        } else if food_supply_emergency && household_feudal_arrears > 0 {
            "fome combinada com divida feudal tende a gerar boicote, saque ou revolta".to_string()
        } else if food_supply_emergency {
            "crise alimentar pressiona legitimidade do racionamento".to_string()
        } else {
            "sem custo politico alimentar agudo".to_string()
        };

        FoodCrisisAssessment {
            household_minimum_food_units,
            household_food_supply_days_tenths,
            household_treasury,
            household_feudal_arrears,
            household_tribute_due,
            village_grain_units,
            village_ready_food_units,
            stalled_food_processors,
            material_food_source_count,
            inter_village_food_source_count,
            food_supply_emergency,
            bottlenecks,
            access_summary,
            political_cost_summary,
        }
    }

    pub(super) fn household_food_access_priority(&self, household_id: BuildingId) -> u8 {
        let Some(household) = self.household_by_id(household_id) else {
            return 50;
        };
        let mut score = 50 + i32::from(household.food_crisis_level) * 8;
        score += (household.treasury / 8).clamp(0, 12);
        score -= household.feudal_arrears.clamp(0, 12);
        score -= household.tax_arrears.clamp(0, 8);
        match self.local_norms.rationing_policy {
            RationingPolicy::HouseholdFirst => score += 12,
            RationingPolicy::ProducersFirst => {
                if self.household_has_food_producer_or_authority(household) {
                    score += 18;
                } else {
                    score -= 4;
                }
            }
            RationingPolicy::CivicFirst => {
                if self.household_has_authority_member(household) {
                    score += 20;
                } else {
                    score -= 8;
                }
            }
            RationingPolicy::Balanced => {
                if self.household_has_food_producer_or_authority(household) {
                    score += 8;
                }
            }
        }
        if household.direct_lord_agent_id.is_some() {
            score += 4;
        }
        for agent_id in &household.member_ids {
            score += (self.feudal_power_for_agent(*agent_id) / 25).clamp(0, 12);
        }
        score.clamp(5, 100) as u8
    }

    fn household_has_food_producer_or_authority(&self, household: &HouseholdEconomy) -> bool {
        household.member_ids.iter().any(|agent_id| {
            let role = self.agent_role_id(*agent_id).unwrap_or_default();
            matches!(
                role.as_str(),
                "campones" | "padeiro" | "taverneiro" | "guarda" | "lider_local"
            )
        })
    }

    fn household_has_authority_member(&self, household: &HouseholdEconomy) -> bool {
        household.member_ids.iter().any(|agent_id| {
            let role = self.agent_role_id(*agent_id).unwrap_or_default();
            matches!(role.as_str(), "guarda" | "lider_local")
                || self.active_feudal_title_for_holder(*agent_id).is_some()
        })
    }

    pub(super) fn household_has_critical_hunger(&mut self, household_id: BuildingId) -> bool {
        let member_ids = self
            .household_by_id(household_id)
            .map(|household| household.member_ids.clone())
            .unwrap_or_default();
        let mut query = self.world.query::<(&AgentCore, &StateComponent)>();
        query
            .iter(&self.world)
            .any(|(core, state)| member_ids.contains(&core.id) && state.0.hunger >= 85)
    }

    pub(super) fn preferred_food_resource_order_for_household(
        &mut self,
        household_id: BuildingId,
    ) -> Vec<String> {
        let mut order = self.food_resource_ids_sorted();
        if self.food_supply_emergency() && !self.household_has_critical_hunger(household_id) {
            order.sort_by_key(|resource_id| {
                if resource_id == ResourceKind::Graos.id() {
                    0
                } else {
                    1
                }
            });
        }
        order
    }

    pub(super) fn total_village_resource_amount(&self, resource_id: &str) -> i32 {
        self.establishments
            .iter()
            .map(|establishment| {
                self.establishment_total_resource_units(establishment, resource_id)
            })
            .sum::<i32>()
            + self
                .households
                .iter()
                .map(|household| {
                    Self::total_resource_amount(&household.pantry, resource_id)
                        + Self::total_resource_amount(&household.reserved_food, resource_id)
                })
                .sum::<i32>()
    }

    pub(super) fn food_processors_missing_resource(&self, resource_id: &str) -> usize {
        self.establishments
            .iter()
            .filter(|establishment| {
                self.recipes_for_establishment(establishment)
                    .iter()
                    .any(|recipe| {
                        self.is_food_resource(&recipe.output_resource_id)
                            && recipe.inputs.iter().any(|input| {
                                input.resource_id == resource_id
                                    && Self::total_resource_amount(
                                        &establishment.stock,
                                        input.resource_id.as_str(),
                                    ) < input.amount.max(1)
                            })
                    })
            })
            .count()
    }

    pub(super) fn push_scarcity_event_deduped(
        &mut self,
        actor: u64,
        target: Option<u64>,
        summary: String,
        impact_tags: Vec<String>,
        window_ticks: u64,
    ) {
        let mut canonical_tags = impact_tags.clone();
        canonical_tags.sort();
        if self.has_recent_event(window_ticks, |event| {
            let mut tags = event.impact_tags.clone();
            tags.sort();
            event.kind == EventKind::Scarcity && event.target == target && tags == canonical_tags
        }) {
            return;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor,
            target,
            kind: EventKind::Scarcity,
            summary,
            impact_tags,
        });
    }

    pub(super) fn ensure_purchase_shortage_task(
        &mut self,
        destination_establishment: &EstablishmentEconomy,
        resource_id: &str,
        source_establishment_type_id: &str,
        amount: i32,
        class: EconomicTaskClass,
        priority: u8,
        max_open_tasks: usize,
    ) {
        let current = Self::total_resource_amount(&destination_establishment.stock, resource_id);
        let target = self.stock_target_amount(destination_establishment, resource_id);
        if current >= target {
            return;
        }
        let Some(actor_household_id) = destination_establishment
            .owner_household_ids
            .first()
            .copied()
        else {
            return;
        };

        // Find the buyer's village index
        let Some(dest_village_idx) =
            self.village_index_of_establishment(destination_establishment.id)
        else {
            return;
        };

        // Find candidate supplier establishments that have enough stock
        let mut candidates = self
            .establishments
            .iter()
            .filter(|candidate| {
                candidate.establishment_type_id == source_establishment_type_id
                    && Self::total_resource_amount(&candidate.stock, resource_id) >= amount
            })
            .cloned()
            .collect::<Vec<_>>();

        // Sort candidates so that we prioritize:
        // 1. Same village (dest_village_idx)
        // 2. Lowest total cost (including 30% inter-village import tax if different village)
        let get_total_cost = |candidate: &EstablishmentEconomy| -> i32 {
            let unit_price = candidate
                .posted_prices
                .iter()
                .find(|price| price.resource_id == resource_id)
                .map(|price| price.unit_price)
                .unwrap_or_else(|| self.base_price(resource_id));
            let is_local =
                self.village_index_of_establishment(candidate.id) == Some(dest_village_idx);
            if is_local {
                unit_price
            } else {
                (unit_price as f64 * 1.3) as i32
            }
        };

        candidates.sort_by_key(|c| {
            let is_local = self.village_index_of_establishment(c.id) == Some(dest_village_idx);
            let cost = get_total_cost(c);
            (!is_local, cost)
        });

        let Some(best_supplier) = candidates.first() else {
            let owner_agent_id = self
                .household_by_id(actor_household_id)
                .and_then(|h| h.member_ids.first().copied())
                .unwrap_or(0);
            self.push_scarcity_event_deduped(
                owner_agent_id,
                Some(destination_establishment.id),
                format!(
                    "Producao de {} paralisada: falta do insumo {} em todas as vilas com estoque material.",
                    destination_establishment.name,
                    self.resource_display_name(resource_id)
                ),
                vec![
                    "escassez".to_string(),
                    "producao_parada".to_string(),
                    "sem_fornecedor_com_estoque".to_string(),
                    "sem_origem_material".to_string(),
                    resource_id.to_string(),
                    format!("establishment:{}", destination_establishment.id),
                ],
                15,
            );
            return;
        };

        // Create Comprar task
        let destination = EconomicNode::Establishment(destination_establishment.id);
        let source_node = EconomicNode::Establishment(best_supplier.id);

        let existing = self.matching_open_task_count(
            actor_household_id,
            EconomicTaskKind::Comprar,
            Some(resource_id),
            &destination,
            Some(&source_node),
        );
        if existing >= max_open_tasks {
            return;
        }

        let unit_price = get_total_cost(best_supplier);
        let desired_amount = amount.clamp(1, DEFAULT_CARRYING_CAPACITY);
        let total_price = unit_price * desired_amount;

        for _ in existing..max_open_tasks {
            let task_id = self.next_task_id();
            let is_local =
                self.village_index_of_establishment(best_supplier.id) == Some(dest_village_idx);
            let description = if is_local {
                format!(
                    "Comprar {} x{} de {} para {}",
                    self.resource_display_name(resource_id),
                    desired_amount,
                    best_supplier.name,
                    destination_establishment.name
                )
            } else {
                format!(
                    "Importar {} x{} de {} para {} (taxa inter-vilas)",
                    self.resource_display_name(resource_id),
                    desired_amount,
                    best_supplier.name,
                    destination_establishment.name
                )
            };

            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::Comprar,
                class,
                priority,
                lock_until_complete: true,
                creates_household_reserve: false,
                actor_household_id,
                assigned_agent_id: None,
                source: source_node.clone(),
                destination: destination.clone(),
                resource_id: Some(resource_id.to_string()),
                amount: desired_amount,
                unit_price,
                total_price,
                description,
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: Some(destination_establishment.id),
                related_construction_project_id: None,
            });
        }
    }

    pub(super) fn generate_daily_caravans(&mut self) -> Result<()> {
        let eligible_ests: Vec<_> = self
            .establishments
            .iter()
            .filter(|e| e.building_id.is_some())
            .cloned()
            .collect();
        if eligible_ests.len() < 2 {
            return Ok(());
        }

        use rand::Rng;
        let mut rng = rand::rng();
        let mut source_candidates = Vec::new();
        for establishment in &eligible_ests {
            let Some(source_village) = self.village_index_of_establishment(establishment.id) else {
                continue;
            };
            let has_other_village_destination = eligible_ests.iter().any(|candidate| {
                candidate.id != establishment.id
                    && self.village_index_of_establishment(candidate.id) != Some(source_village)
            });
            if !has_other_village_destination {
                continue;
            }
            for stack in &establishment.stock {
                if stack.amount >= 4 && stack.resource_id != ResourceKind::Moedas.id() {
                    source_candidates.push((
                        establishment.id,
                        establishment.building_id.unwrap(),
                        establishment.name.clone(),
                        source_village,
                        stack.resource_id.clone(),
                        stack.amount,
                    ));
                }
            }
        }
        if source_candidates.is_empty() {
            return Ok(());
        }

        let (
            source_establishment_id,
            source_building_id,
            source_name,
            source_village,
            resource_id,
            available_amount,
        ) = source_candidates[rng.random_range(0..source_candidates.len())].clone();
        let dest_candidates = eligible_ests
            .iter()
            .filter(|candidate| {
                candidate.id != source_establishment_id
                    && self.village_index_of_establishment(candidate.id) != Some(source_village)
            })
            .cloned()
            .collect::<Vec<_>>();
        if dest_candidates.is_empty() {
            return Ok(());
        }
        let dest_est = dest_candidates[rng.random_range(0..dest_candidates.len())].clone();
        let Some(dest_building_id) = dest_est.building_id else {
            return Ok(());
        };
        let dest_name = dest_est.name.clone();

        let Some(start_coord) = self.building_by_id(source_building_id).map(|b| b.entrance) else {
            return Ok(());
        };
        let Some(dest_coord) = self.building_by_id(dest_building_id).map(|b| b.entrance) else {
            return Ok(());
        };

        let requested_amount = rng.random_range(4..=available_amount.min(20).max(4));
        let cargo_amount = self
            .establishment_by_id_mut(source_establishment_id)
            .map(|source| Self::take_resource(&mut source.stock, &resource_id, requested_amount))
            .unwrap_or(0);
        if cargo_amount <= 0 {
            return Ok(());
        }

        let caravan_agent_id = self
            .world
            .query::<&AgentCore>()
            .iter(&self.world)
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            + 1;
        let guard_agent_id = caravan_agent_id + 1;

        self.world.spawn((
            (
                AgentCore {
                    id: caravan_agent_id,
                    name: format!("Caravana de {}", dest_name),
                    role_id: "caravana".to_string(),
                    home_building_id: None,
                    work_building_id: None,
                    home_bed: None,
                },
                ProfileComponent(AgentProfile {
                    traits: vec![],
                    values: vec![],
                    fears: vec![],
                    long_term_desires: vec![],
                    moral_tolerances: vec![],
                    social_style: "prudente".to_string(),
                    trauma_traits: vec![],
                }),
                StateComponent(AgentState {
                    mood: 50,
                    energy: 100,
                    health: 100,
                    hunger: 0,
                    stress: 0,
                    current_focus: "entrega".to_string(),
                    active_goals: vec!["entregar carga".to_string()],
                }),
                LifeStatusComponent(AgentLifeStatus::Vivo),
                InjuryComponent::default(),
                InstitutionalPerceptionComponent::default(),
                PsychologicalStateComponent::default(),
                RumorBeliefComponent::default(),
                StoryBeliefComponent::default(),
            ),
            (
                RelationComponent(HashMap::new()),
                LineageComponent {
                    age: 30,
                    parents: vec![],
                    children: vec![],
                    spouse: None,
                    gender: "Outro".to_string(),
                    mourning_days_left: 0,
                },
                MemoryComponent(vec![]),
                InventoryComponent(vec![ResourceStack {
                    resource_id: resource_id.clone(),
                    amount: cargo_amount,
                }]),
                ItemInventoryComponent::default(),
                EquipmentComponent::default(),
                CraftProficiencyComponent::default(),
                PositionComponent(start_coord),
            ),
            (
                DestinationComponent(Some(dest_coord)),
                DestinationLabelComponent(Some(dest_name.clone())),
                PathComponent(vec![]),
                IntentComponent(None),
                TaskQueueComponent::default(),
            ),
            (
                ThoughtComponent("Transportando mercadorias.".to_string()),
                DecisionBudgetComponent::default(),
                CognitionComponent::default(),
                ConversationComponent::default(),
                EconomicActivityComponent::default(),
                TraumaTrackerComponent::default(),
                UtilityControlComponent::default(),
            ),
        ));

        self.world.spawn((
            (
                AgentCore {
                    id: guard_agent_id,
                    name: "Guarda da Caravana".to_string(),
                    role_id: "guarda_caravana".to_string(),
                    home_building_id: None,
                    work_building_id: None,
                    home_bed: None,
                },
                ProfileComponent(AgentProfile {
                    traits: vec!["leal".to_string()],
                    values: vec!["honra".to_string()],
                    fears: vec![],
                    long_term_desires: vec![],
                    moral_tolerances: vec![],
                    social_style: "agressivo".to_string(),
                    trauma_traits: vec![],
                }),
                StateComponent(AgentState {
                    mood: 60,
                    energy: 100,
                    health: 100,
                    hunger: 0,
                    stress: 0,
                    current_focus: "escolta".to_string(),
                    active_goals: vec!["escoltar caravana".to_string()],
                }),
                LifeStatusComponent(AgentLifeStatus::Vivo),
                InjuryComponent::default(),
                InstitutionalPerceptionComponent::default(),
                PsychologicalStateComponent::default(),
                RumorBeliefComponent::default(),
                StoryBeliefComponent::default(),
            ),
            (
                RelationComponent(HashMap::new()),
                LineageComponent {
                    age: 28,
                    parents: vec![],
                    children: vec![],
                    spouse: None,
                    gender: "Masculino".to_string(),
                    mourning_days_left: 0,
                },
                MemoryComponent(vec![]),
                InventoryComponent::default(),
                ItemInventoryComponent::default(),
                EquipmentComponent::default(),
                CraftProficiencyComponent::default(),
                PositionComponent(start_coord),
            ),
            (
                DestinationComponent(Some(start_coord)),
                DestinationLabelComponent(Some("Caravana".to_string())),
                PathComponent(vec![]),
                IntentComponent(None),
                TaskQueueComponent::default(),
            ),
            (
                ThoughtComponent("Escoltando a caravana.".to_string()),
                DecisionBudgetComponent::default(),
                CognitionComponent::default(),
                ConversationComponent::default(),
                EconomicActivityComponent::default(),
                TraumaTrackerComponent::default(),
                UtilityControlComponent::default(),
            ),
        ));

        let mut headman_id = None;
        let mut query = self.world.query::<(Entity, &AgentCore)>();
        for (_, core) in query.iter(&self.world) {
            if core.role_id == "lider_local" {
                headman_id = Some(core.id);
                break;
            }
        }

        let mut known_by = vec![caravan_agent_id, guard_agent_id];
        if let Some(h_id) = headman_id {
            known_by.push(h_id);
        }
        for hh_id in &dest_est.owner_household_ids {
            if let Some(hh) = self.household_by_id(*hh_id) {
                known_by.extend(hh.member_ids.clone());
            }
        }

        let mut obs_query = self
            .world
            .query::<(Entity, &AgentCore, &ProfileComponent, &LifeStatusComponent)>();
        for (_, core, profile, status) in obs_query.iter(&self.world) {
            if status.0 == AgentLifeStatus::Vivo
                && (profile.0.traits.contains(&"observador".to_string())
                    || profile.0.traits.contains(&"astuto".to_string()))
            {
                if rng.random_bool(0.05) {
                    known_by.push(core.id);
                }
            }
        }
        known_by.dedup();

        let caravan_state = CaravanState {
            id: caravan_agent_id,
            resource_id: resource_id.clone(),
            amount: cargo_amount,
            escort_ids: vec![guard_agent_id],
            position: start_coord,
            destination: dest_coord,
            status: "trÃ¢nsito".to_string(),
        };
        self.caravans.push(caravan_state);

        let secret_id = self.next_secret_id;
        self.next_secret_id += 1;
        let secret = Secret {
            id: secret_id,
            kind: SecretKind::CaravanRoute,
            target_id: caravan_agent_id,
            summary: format!(
                "A rota da caravana de {} para {} carregando {} {}.",
                source_name, dest_name, cargo_amount, resource_id
            ),
            details: format!(
                "Caravana ID: {}. Origem: ({}, {}). Destino: ({}, {}).",
                caravan_agent_id, start_coord.x, start_coord.y, dest_coord.x, dest_coord.y
            ),
            known_by,
        };
        self.secrets.push(secret);

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: caravan_agent_id,
            target: None,
            kind: EventKind::Commerce,
            summary: format!(
                "Uma nova caravana partiu de {} em ({}, {}) carregando {} {} rumo a {}.",
                source_name, start_coord.x, start_coord.y, cargo_amount, resource_id, dest_name
            ),
            impact_tags: vec!["comercio".to_string(), "caravana".to_string()],
        });

        Ok(())
    }

    pub(super) fn apply_caravan_behaviors(&mut self) -> Result<()> {
        let mut caravans_to_update = self.caravans.clone();
        let mut agents_to_despawn = Vec::new();

        for caravan in caravans_to_update.iter_mut() {
            if caravan.status != "trÃ¢nsito" {
                continue;
            }

            let caravan_agent_id = caravan.id;

            let caravan_ent = match self.find_agent_entity(caravan_agent_id) {
                Ok(ent) => ent,
                Err(_) => {
                    caravan.status = "saqueada".to_string();
                    continue;
                }
            };

            let caravan_status = self.life_status(caravan_agent_id)?;

            if caravan_status == AgentLifeStatus::Morto
                || caravan_status == AgentLifeStatus::Incapacitado
            {
                caravan.status = "saqueada".to_string();
                agents_to_despawn.push(caravan_agent_id);
                for &escort_id in &caravan.escort_ids {
                    agents_to_despawn.push(escort_id);
                }

                let caravan_pos = self.world.get::<PositionComponent>(caravan_ent).unwrap().0;
                let mut looter_id = None;
                let mut query =
                    self.world
                        .query::<(Entity, &AgentCore, &PositionComponent, &LifeStatusComponent)>();
                for (_, core, pos, status) in query.iter(&self.world) {
                    if status.0 == AgentLifeStatus::Vivo
                        && core.role_id != "caravana"
                        && core.role_id != "guarda_caravana"
                    {
                        if pos.0.manhattan(caravan_pos) <= 1 {
                            looter_id = Some(core.id);
                            break;
                        }
                    }
                }

                if let Some(lid) = looter_id {
                    let looter_ent = self.find_agent_entity(lid)?;
                    let looter_name = self.agent_name(lid)?;

                    let cargo = {
                        let mut caravan_inv = self
                            .world
                            .get_mut::<InventoryComponent>(caravan_ent)
                            .unwrap();
                        let cargo = caravan_inv.0.clone();
                        caravan_inv.0.clear();
                        cargo
                    };

                    let mut looter_inv = self
                        .world
                        .get_mut::<InventoryComponent>(looter_ent)
                        .unwrap();
                    for stack in cargo {
                        Self::push_resource(&mut looter_inv.0, &stack.resource_id, stack.amount);
                    }

                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: lid,
                        target: Some(caravan_agent_id),
                        kind: EventKind::Violence,
                        summary: format!(
                            "{} interceptou e saqueou a caravana, roubando sua carga.",
                            looter_name
                        ),
                        impact_tags: vec![
                            "violencia".to_string(),
                            "saque".to_string(),
                            "caravana".to_string(),
                        ],
                    });
                } else {
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: caravan_agent_id,
                        target: None,
                        kind: EventKind::Violence,
                        summary: "A caravana foi perdida e destruÃ­da.".to_string(),
                        impact_tags: vec!["violencia".to_string(), "caravana".to_string()],
                    });
                }
                continue;
            }

            let mut all_escorts_defeated = true;
            for &escort_id in &caravan.escort_ids {
                if let Ok(status) = self.life_status(escort_id) {
                    if status == AgentLifeStatus::Vivo {
                        all_escorts_defeated = false;
                        break;
                    }
                }
            }

            let caravan_pos = self.world.get::<PositionComponent>(caravan_ent).unwrap().0;
            caravan.position = caravan_pos;

            if all_escorts_defeated && !caravan.escort_ids.is_empty() {
                let mut looter_id = None;
                let mut query =
                    self.world
                        .query::<(Entity, &AgentCore, &PositionComponent, &LifeStatusComponent)>();
                for (_, core, pos, status) in query.iter(&self.world) {
                    if status.0 == AgentLifeStatus::Vivo
                        && core.role_id != "caravana"
                        && core.role_id != "guarda_caravana"
                    {
                        if pos.0.manhattan(caravan_pos) <= 1 {
                            let is_active_rioter = self.political_factions.iter().any(|f| {
                                f.is_action_active
                                    && f.member_ids.contains(&core.id)
                                    && matches!(
                                        f.objective,
                                        Some(FactionObjective::FoodRiot { .. })
                                    )
                            });
                            if is_active_rioter || core.role_id == "bandido" {
                                looter_id = Some(core.id);
                                break;
                            }
                        }
                    }
                }

                if let Some(lid) = looter_id {
                    caravan.status = "saqueada".to_string();
                    agents_to_despawn.push(caravan_agent_id);
                    for &escort_id in &caravan.escort_ids {
                        agents_to_despawn.push(escort_id);
                    }

                    let looter_ent = self.find_agent_entity(lid)?;
                    let looter_name = self.agent_name(lid)?;

                    let cargo = {
                        let mut caravan_inv = self
                            .world
                            .get_mut::<InventoryComponent>(caravan_ent)
                            .unwrap();
                        let cargo = caravan_inv.0.clone();
                        caravan_inv.0.clear();
                        cargo
                    };

                    let mut looter_inv = self
                        .world
                        .get_mut::<InventoryComponent>(looter_ent)
                        .unwrap();
                    for stack in cargo {
                        Self::push_resource(&mut looter_inv.0, &stack.resource_id, stack.amount);
                    }

                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: lid,
                        target: Some(caravan_agent_id),
                        kind: EventKind::Violence,
                        summary: format!(
                            "{} derrotou a escolta e saqueou a caravana.",
                            looter_name
                        ),
                        impact_tags: vec![
                            "violencia".to_string(),
                            "saque".to_string(),
                            "caravana".to_string(),
                        ],
                    });
                    continue;
                }
            }

            if caravan_pos == caravan.destination {
                let dest = caravan.destination;
                let mut delivered = false;
                let mut dest_name = String::new();

                let dest_building_id = self
                    .spatial
                    .buildings
                    .iter()
                    .find(|b| b.entrance == dest)
                    .map(|b| b.id);

                if let Some(bid) = dest_building_id {
                    if let Some(est) = self
                        .establishments
                        .iter_mut()
                        .find(|e| e.building_id == Some(bid))
                    {
                        delivered = true;
                        dest_name = est.name.clone();
                        if caravan.resource_id == "moedas" {
                            est.cash += caravan.amount;
                        } else {
                            Self::push_resource(
                                &mut est.stock,
                                &caravan.resource_id,
                                caravan.amount,
                            );
                        }
                    }
                }

                if delivered {
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: caravan_agent_id,
                        target: None,
                        kind: EventKind::Commerce,
                        summary: format!(
                            "A caravana chegou ao destino e entregou {} {} em {}.",
                            caravan.amount, caravan.resource_id, dest_name
                        ),
                        impact_tags: vec!["comercio".to_string(), "caravana".to_string()],
                    });
                    caravan.status = "entregue".to_string();
                    agents_to_despawn.push(caravan_agent_id);
                    for &escort_id in &caravan.escort_ids {
                        agents_to_despawn.push(escort_id);
                    }
                    continue;
                } else {
                    let mut is_solar = false;
                    for b in &self.spatial.buildings {
                        if b.entrance == dest && b.kind == LocationKind::Manor {
                            is_solar = true;
                            break;
                        }
                    }
                    if is_solar {
                        delivered = true;
                        if caravan.resource_id == "moedas" {
                            self.village_economy.public_treasury += caravan.amount;
                        } else {
                            if let Some(est) = self
                                .establishments
                                .iter_mut()
                                .find(|e| e.location_kind == LocationKind::Manor)
                            {
                                Self::push_resource(
                                    &mut est.stock,
                                    &caravan.resource_id,
                                    caravan.amount,
                                );
                            }
                        }

                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: caravan_agent_id,
                            target: None,
                            kind: EventKind::Commerce,
                            summary: format!(
                                "A caravana chegou ao solar e depositou {} {} no erÃ¡rio pÃºblico.",
                                caravan.amount, caravan.resource_id
                            ),
                            impact_tags: vec![
                                "comercio".to_string(),
                                "erario".to_string(),
                                "caravana".to_string(),
                            ],
                        });
                    }
                }

                if delivered {
                    caravan.status = "entregue".to_string();
                    agents_to_despawn.push(caravan_agent_id);
                    for &escort_id in &caravan.escort_ids {
                        agents_to_despawn.push(escort_id);
                    }
                    continue;
                }
            }

            let path_empty = self
                .world
                .get::<PathComponent>(caravan_ent)
                .unwrap()
                .0
                .is_empty();
            if path_empty {
                if let Some(path) =
                    self.find_path(caravan_pos, caravan.destination, Some(caravan_agent_id))
                {
                    self.world.get_mut::<PathComponent>(caravan_ent).unwrap().0 = path;
                }
            }

            let mut attackers = Vec::new();
            let mut query =
                self.world
                    .query::<(Entity, &AgentCore, &PositionComponent, &LifeStatusComponent)>();
            for (_, core, pos, status) in query.iter(&self.world) {
                if status.0 == AgentLifeStatus::Vivo
                    && core.role_id != "caravana"
                    && core.role_id != "guarda_caravana"
                {
                    if pos.0.manhattan(caravan_pos) <= 1 {
                        let is_active_rioter = self.political_factions.iter().any(|f| {
                            f.is_action_active
                                && f.member_ids.contains(&core.id)
                                && matches!(f.objective, Some(FactionObjective::FoodRiot { .. }))
                        });
                        if is_active_rioter || core.role_id == "bandido" {
                            attackers.push(core.id);
                        }
                    }
                }
            }

            for attacker_id in attackers {
                let mut living_escort = None;
                for &escort_id in &caravan.escort_ids {
                    if let Ok(status) = self.life_status(escort_id) {
                        if status == AgentLifeStatus::Vivo {
                            living_escort = Some(escort_id);
                            break;
                        }
                    }
                }

                if let Some(esc_id) = living_escort {
                    self.ensure_combat(attacker_id, esc_id)?;
                } else {
                    self.ensure_combat(attacker_id, caravan_agent_id)?;
                }
            }

            for &escort_id in &caravan.escort_ids {
                let escort_ent = match self.find_agent_entity(escort_id) {
                    Ok(ent) => ent,
                    Err(_) => continue,
                };

                if self.life_status(escort_id)? != AgentLifeStatus::Vivo {
                    continue;
                }

                let mut opponent_id = None;
                for combat in &self.combats {
                    if combat.status == CombatStatus::Active
                        && combat.participants.contains(&escort_id)
                    {
                        opponent_id = Some(other_participant(&combat.participants, escort_id));
                        break;
                    }
                }

                if let Some(opp_id) = opponent_id {
                    if self.agents_adjacent(escort_id, opp_id)? {
                        self.apply_attack(escort_id, opp_id, true)?;
                    } else {
                        let opp_ent = self.find_agent_entity(opp_id)?;
                        let opp_pos = self.world.get::<PositionComponent>(opp_ent).unwrap().0;
                        self.world
                            .get_mut::<DestinationComponent>(escort_ent)
                            .unwrap()
                            .0 = Some(opp_pos);
                        let escort_pos = self.world.get::<PositionComponent>(escort_ent).unwrap().0;
                        let path_empty = self
                            .world
                            .get::<PathComponent>(escort_ent)
                            .unwrap()
                            .0
                            .is_empty();
                        if path_empty {
                            if let Some(path) = self.find_path(escort_pos, opp_pos, Some(escort_id))
                            {
                                self.world.get_mut::<PathComponent>(escort_ent).unwrap().0 = path;
                            }
                        }
                    }
                } else {
                    self.world
                        .get_mut::<DestinationComponent>(escort_ent)
                        .unwrap()
                        .0 = Some(caravan_pos);

                    let escort_pos = self.world.get::<PositionComponent>(escort_ent).unwrap().0;
                    if escort_pos.manhattan(caravan_pos) > 1 {
                        let path_empty = self
                            .world
                            .get::<PathComponent>(escort_ent)
                            .unwrap()
                            .0
                            .is_empty();
                        if path_empty {
                            if let Some(path) =
                                self.find_path(escort_pos, caravan_pos, Some(escort_id))
                            {
                                self.world.get_mut::<PathComponent>(escort_ent).unwrap().0 = path;
                            }
                        }
                    }
                }
            }
        }

        for agent_id in agents_to_despawn {
            if let Ok(ent) = self.find_agent_entity(agent_id) {
                self.world.despawn(ent);
            }
        }

        self.caravans = caravans_to_update;
        Ok(())
    }

    pub(super) fn apply_esconder_intent(
        &mut self,
        agent_id: u64,
        intent: &AgentIntent,
    ) -> Result<()> {
        let name = self.agent_name(agent_id)?;
        let role_id = self.agent_role_id(agent_id)?;
        let household_id = self.agent_home_building_id(agent_id)?;

        let target = intent.target_semantic.clone().unwrap_or_default();

        if target == "moedas" {
            if let Some(hh_id) = household_id {
                if let Some(hh) = self.household_by_id_mut(hh_id) {
                    let amount_to_hide = hh.treasury.min(10);
                    if amount_to_hide > 0 {
                        hh.treasury -= amount_to_hide;

                        self.add_memory(
                            agent_id,
                            MemoryKind::Fact,
                            format!("Escondi {} moedas no meu stash privado para nao pagar imposto de guerra.", amount_to_hide),
                            vec!["moedas_escondidas".to_string(), "sonegacao".to_string()],
                            6,
                            vec![],
                        )?;

                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: agent_id,
                            target: None,
                            kind: EventKind::Commerce,
                            summary: format!(
                                "{} sonegou e ocultou {} moeda(s) em seu stash pessoal.",
                                name, amount_to_hide
                            ),
                            impact_tags: vec![
                                "subversao".to_string(),
                                "moedas_escondidas".to_string(),
                            ],
                        });
                    }
                }
            }
        } else if target == "metal_bruto" {
            if let Some(work_id) = self.work_building_id_for_role(&role_id) {
                if let Some(establishment) = self.establishment_by_building_mut(work_id) {
                    if let Some(stack) = establishment
                        .stock
                        .iter_mut()
                        .find(|s| s.resource_id == "metal_bruto")
                    {
                        let amount_to_hide = stack.amount.min(2);
                        if amount_to_hide > 0 {
                            stack.amount -= amount_to_hide;

                            self.add_memory(
                                agent_id,
                                MemoryKind::Fact,
                                format!("Escondi {} metal_bruto na minha arca privada para nao ser confiscado.", amount_to_hide),
                                vec!["metal_escondido".to_string(), "subversao".to_string()],
                                6,
                                vec![],
                            )?;

                            self.push_event(WorldEvent {
                                day: self.day,
                                tick: self.tick_of_day,
                                actor: agent_id,
                                target: None,
                                kind: EventKind::Commerce,
                                summary: format!("{} ocultou {} ferro(s) bruto(s) na sua oficina para evitar o confisco.", name, amount_to_hide),
                                impact_tags: vec!["subversao".to_string(), "metal_escondido".to_string()],
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
