use super::*;
use crate::world_model::TileCoord;

#[derive(Clone)]
pub(super) struct ResolvedTargetCandidate {
    pub(super) destination: TileCoord,
    pub(super) label: String,
}

impl Simulation {
    pub(super) fn ensure_navigation_for_current_intent(&mut self, agent_id: u64) -> Result<()> {
        let _ = self.sync_intent_with_locked_task(agent_id)?;
        let entity = self.find_agent_entity(agent_id)?;
        let (
            intent,
            current_pos,
            current_destination,
            current_path_len,
            active_conversation_id,
            core,
        ) = {
            let entry = self.world.entity(entity);
            (
                entry
                    .get::<IntentComponent>()
                    .ok_or_else(|| anyhow!("missing intent component"))?
                    .0
                    .clone(),
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0,
                entry
                    .get::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .len(),
                entry
                    .get::<ConversationComponent>()
                    .ok_or_else(|| anyhow!("missing conversation component"))?
                    .active_conversation_id,
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .clone(),
            )
        };
        if active_conversation_id.is_some() {
            return Ok(());
        }
        let Some(intent) = intent else {
            return Ok(());
        };
        if intent.kind == IntentKind::Comer && !self.household_has_food_available(agent_id)? {
            if self.reroute_eat_intent_to_food_purchase(agent_id)? {
                return self.ensure_navigation_for_current_intent(agent_id);
            }
        }
        if current_path_len > 0 {
            return Ok(());
        }

        if self.ready_to_execute(agent_id, &intent)? {
            return Ok(());
        }

        let candidates = self.resolve_intent_candidates(&core, current_pos, &intent)?;
        if candidates.is_empty()
            && matches!(
                intent.kind,
                IntentKind::Comprar
                    | IntentKind::Transportar
                    | IntentKind::Vender
                    | IntentKind::ReceberPagamento
                    | IntentKind::Construir
            )
        {
            if self
                .active_economic_task_for_agent(agent_id)
                .map(|task| task.lock_until_complete)
                .unwrap_or(false)
            {
                self.increment_blocked_ticks(agent_id)?;
                if self.blocked_ticks(agent_id)? >= BLOCKED_RECONSIDERATION_TICKS * 3 {
                    self.fail_active_economic_task(
                        agent_id,
                        "nao encontrou rota ou alvo economico valido por tempo demais",
                        true,
                    )?;
                }
                return Ok(());
            }
            if self.try_rebind_household_food_intent(agent_id)? {
                return self.ensure_navigation_for_current_intent(agent_id);
            }
            self.clear_intent_navigation(agent_id)?;
            self.clear_active_economic_task(agent_id)?;
            return Ok(());
        }
        for candidate in candidates {
            if current_pos == candidate.destination {
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = Some(candidate.destination);
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = Some(candidate.label);
                return Ok(());
            }
            if let Some(path) = self.find_path(current_pos, candidate.destination, Some(agent_id)) {
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = Some(candidate.destination);
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = Some(candidate.label);
                entity_mut
                    .get_mut::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0 = path;
                return Ok(());
            }
        }

        if current_destination.is_some() {
            self.increment_blocked_ticks(agent_id)?;
            if self
                .active_economic_task_for_agent(agent_id)
                .map(|task| task.lock_until_complete)
                .unwrap_or(false)
                && self.blocked_ticks(agent_id)? >= BLOCKED_RECONSIDERATION_TICKS * 3
            {
                self.fail_active_economic_task(
                    agent_id,
                    "permaneceu bloqueado no caminho da tarefa economica",
                    true,
                )?;
                return Ok(());
            }
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Blocking,
                summary: format!(
                    "{} nao encontra caminho livre para {}.",
                    core.name,
                    intent
                        .target_semantic
                        .clone()
                        .unwrap_or_else(|| intent.kind.as_str().to_string())
                ),
                impact_tags: vec!["bloqueio".to_string(), "navegacao".to_string()],
            });
        }
        Ok(())
    }

    pub(super) fn clear_navigation_keep_intent(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = None;
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        Ok(())
    }

    pub(super) fn clear_intent_navigation(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = None;
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        Ok(())
    }

    pub(super) fn ready_to_execute(&mut self, agent_id: u64, intent: &AgentIntent) -> Result<bool> {
        let entity = self.find_agent_entity(agent_id)?;
        let (current_pos, destination) = {
            let entry = self.world.entity(entity);
            (
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0,
            )
        };
        match intent.kind {
            IntentKind::Comer => {
                if self.household_has_food_available(agent_id)? {
                    return Ok(true);
                }
                Ok(destination
                    .map(|target| target == current_pos)
                    .unwrap_or(false))
            }
            IntentKind::Socializar => {
                if let Some(target_id) = intent.target_agent {
                    self.agents_adjacent(agent_id, target_id)
                } else {
                    Ok(false)
                }
            }
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento
            | IntentKind::Construir => Ok(destination
                .map(|destination| destination == current_pos)
                .unwrap_or(false)),
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Prender
            | IntentKind::Punir
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
            | IntentKind::NegociarSuserania => {
                if let Some(target_id) = intent.target_agent {
                    self.agents_adjacent(agent_id, target_id)
                } else {
                    Ok(false)
                }
            }
            IntentKind::Furtar => {
                if let Some(target_id) = intent.target_agent {
                    Ok(self
                        .agent_distance_from(current_pos, target_id)
                        .is_some_and(|distance| distance <= 2))
                } else {
                    Ok(false)
                }
            }
            IntentKind::Fugir
            | IntentKind::Acusar
            | IntentKind::Investigar
            | IntentKind::Apoiar
            | IntentKind::Opor
            | IntentKind::ReivindicarTerritorio => Ok(true),
            _ => Ok(destination
                .map(|destination| destination == current_pos)
                .unwrap_or(false)),
        }
    }

    pub(super) fn advance_agent_movement(&mut self, agent_id: u64) -> Result<bool> {
        let entity = self.find_agent_entity(agent_id)?;
        let (current_pos, path, name, active_conversation_id) = {
            let entry = self.world.entity(entity);
            (
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .clone(),
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .name
                    .clone(),
                entry
                    .get::<ConversationComponent>()
                    .ok_or_else(|| anyhow!("missing conversation component"))?
                    .active_conversation_id,
            )
        };
        if active_conversation_id.is_some() {
            return Ok(false);
        }
        let Some(next_step) = path.first().copied() else {
            return Ok(false);
        };
        if !self.is_walkable(next_step) {
            self.increment_blocked_ticks(agent_id)?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Blocking,
                summary: format!("{name} fica bloqueado em seu caminho."),
                impact_tags: vec!["bloqueio".to_string(), "movimento".to_string()],
            });
            return Ok(false);
        }
        let previous_tile = self.tile_at(current_pos).cloned();
        {
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<PositionComponent>()
                .ok_or_else(|| anyhow!("missing position component"))?
                .0 = next_step;
            let mut path_component = entity_mut
                .get_mut::<PathComponent>()
                .ok_or_else(|| anyhow!("missing path component"))?;
            if !path_component.0.is_empty() {
                path_component.0.remove(0);
            }
        }
        self.reset_blocked_ticks(agent_id)?;
        let new_tile = self.tile_at(next_step).cloned();
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Travel,
            summary: format!("{name} anda para ({}, {}).", next_step.x, next_step.y),
            impact_tags: self.tile_tags(next_step),
        });

        if previous_tile.as_ref().and_then(|tile| tile.building_id)
            != new_tile.as_ref().and_then(|tile| tile.building_id)
        {
            if let Some(building_id) = new_tile.as_ref().and_then(|tile| tile.building_id) {
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Arrival,
                    summary: format!(
                        "{name} entra em {}.",
                        self.building_name(building_id)
                            .unwrap_or_else(|| "um edificio".to_string())
                    ),
                    impact_tags: vec!["entrada".to_string(), format!("building:{building_id}")],
                });
            }
        }

        let destination = self
            .world
            .entity(entity)
            .get::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0;
        if destination == Some(next_step) {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Arrival,
                summary: format!("{name} chega ao destino fisico atual."),
                impact_tags: self.tile_tags(next_step),
            });
        }

        Ok(true)
    }

    pub(super) fn resolve_intent_candidates(
        &mut self,
        core: &AgentCore,
        current_pos: TileCoord,
        intent: &AgentIntent,
    ) -> Result<Vec<ResolvedTargetCandidate>> {
        if let Some(place_id) = intent
            .target_semantic
            .as_deref()
            .filter(|target| Self::looks_like_place_id(target))
        {
            if let Some(destination) = self.place_target_coord(place_id) {
                let label = self
                    .place_by_id(place_id)
                    .map(|place| place.display_name)
                    .unwrap_or_else(|| place_id.to_string());
                return Ok(vec![ResolvedTargetCandidate { destination, label }]);
            }
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: core.id,
                target: None,
                kind: EventKind::CognitionFailure,
                summary: format!(
                    "{} tentou usar place_id inexistente: {}",
                    core.name, place_id
                ),
                impact_tags: vec![
                    "place_id_invalido".to_string(),
                    intent.kind.as_str().to_string(),
                ],
            });
            return Ok(Vec::new());
        }

        let mut candidates = match intent.kind {
            IntentKind::Trabalhar => self.work_candidates(core.id, core),
            IntentKind::Descansar => self.rest_candidates(core),
            IntentKind::Comer => self.eat_candidates(core),
            IntentKind::Refletir => self.reflect_candidates(core),
            IntentKind::Andar => self.wander_candidates(core.id),
            IntentKind::Socializar => self.social_candidates(core.id, intent.target_agent),
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento
            | IntentKind::Construir => self.economic_task_candidates(core.id),
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Furtar
            | IntentKind::Prender
            | IntentKind::Punir
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
            | IntentKind::NegociarSuserania => self.social_candidates(core.id, intent.target_agent),
            IntentKind::Fugir
            | IntentKind::Acusar
            | IntentKind::Investigar
            | IntentKind::Apoiar
            | IntentKind::Opor
            | IntentKind::ReivindicarTerritorio
            | IntentKind::Decretar
            | IntentKind::Esconder => {
                vec![ResolvedTargetCandidate {
                    destination: current_pos,
                    label: intent
                        .target_semantic
                        .clone()
                        .unwrap_or_else(|| intent.kind.as_str().to_string()),
                }]
            }
        };

        // Se proibição de tavernas estiver ativa, removemos a taverna pública
        let restricted_establishment_types = self.movement_restricted_establishment_types();
        let tavernas_proibidas = restricted_establishment_types
            .iter()
            .any(|kind| kind == "taverna")
            && self
                .agent_memories(core.id)
                .map(|mems| {
                    mems.iter()
                        .any(|m| m.tags.contains(&"proibicao_tavernas".to_string()))
                })
                .unwrap_or(false);

        if tavernas_proibidas {
            candidates
                .retain(|candidate| candidate.label != "taverna" && candidate.label != "Taverna");
        }

        candidates.sort_by_key(|candidate| current_pos.manhattan(candidate.destination));
        Ok(candidates)
    }

    pub(super) fn looks_like_place_id(value: &str) -> bool {
        value.starts_with("building:")
            || value.starts_with("room:")
            || value.starts_with("fixture:")
            || value.starts_with("territory:")
            || value.starts_with("special:")
    }

    pub(super) fn work_candidates(
        &self,
        actor_id: u64,
        core: &AgentCore,
    ) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        if let Some(task) = self.active_economic_task_for_agent(actor_id)
            && task.kind == EconomicTaskKind::Produzir
        {
            if let Some(establishment_id) = task.related_establishment_id
                && let Some(establishment) = self.establishment_by_id(establishment_id)
            {
                for fixture in self.spatial.fixtures.iter().filter(|fixture| {
                    fixture.kind == FixtureKind::Workstation
                        && fixture.building_id == establishment.building_id
                }) {
                    if let Some(destination) = self.fixture_access_tile(fixture) {
                        candidates.push(ResolvedTargetCandidate {
                            destination,
                            label: task.description.clone(),
                        });
                    }
                }
                if !candidates.is_empty() {
                    return candidates;
                }
            }
        }
        if core.role_id == Role::Farmer.id() {
            for fixture in self.spatial.fixtures.iter().filter(|fixture| {
                fixture.kind == FixtureKind::Workstation
                    && self
                        .building_kind_opt(fixture.building_id)
                        .map(|kind| {
                            matches!(
                                kind,
                                LocationKind::Farm | LocationKind::Woodlot | LocationKind::Quarry
                            )
                        })
                        .unwrap_or(false)
            }) {
                if let Some(destination) = self.fixture_access_tile(fixture) {
                    candidates.push(ResolvedTargetCandidate {
                        destination,
                        label: format!("trabalho em {}", fixture.name),
                    });
                }
            }
            return candidates;
        }
        for fixture in self.spatial.fixtures.iter().filter(|fixture| {
            fixture.kind == FixtureKind::Workstation && fixture.building_id == core.work_building_id
        }) {
            if let Some(destination) = self.fixture_access_tile(fixture) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: format!("trabalho em {}", fixture.name),
                });
            }
        }
        candidates
    }

    pub(super) fn rest_candidates(&self, core: &AgentCore) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        if let Some(home_bed) = core.home_bed {
            if let Some(destination) = self.access_tile_for_coord(home_bed) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: "cama de casa".to_string(),
                });
            }
        }
        candidates
    }

    pub(super) fn eat_candidates(&self, core: &AgentCore) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        for fixture in self.spatial.fixtures.iter().filter(|fixture| {
            matches!(fixture.kind, FixtureKind::Table | FixtureKind::Seat)
                && (fixture.building_id == core.home_building_id
                    || self
                        .building_kind_opt(fixture.building_id)
                        .map(|kind| matches!(kind, LocationKind::Tavern | LocationKind::Bakery))
                        .unwrap_or(false))
        }) {
            if let Some(destination) = self.fixture_access_tile(fixture) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: format!("comer perto de {}", fixture.name),
                });
            }
        }
        candidates
    }

    pub(super) fn reflect_candidates(&self, core: &AgentCore) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        for fixture in self.spatial.fixtures.iter().filter(|fixture| {
            matches!(fixture.kind, FixtureKind::Seat | FixtureKind::Table)
                && (fixture.building_id == core.home_building_id
                    || self
                        .building_kind_opt(fixture.building_id)
                        .map(|kind| kind == LocationKind::Tavern)
                        .unwrap_or(false))
        }) {
            if let Some(destination) = self.fixture_access_tile(fixture) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: format!("refletir perto de {}", fixture.name),
                });
            }
        }
        for coord in [
            TileCoord { x: 24, y: 13 },
            TileCoord { x: 22, y: 13 },
            TileCoord { x: 26, y: 13 },
        ] {
            candidates.push(ResolvedTargetCandidate {
                destination: coord,
                label: "praca central".to_string(),
            });
        }
        candidates
    }

    pub(super) fn wander_candidates(&self, actor_id: u64) -> Vec<ResolvedTargetCandidate> {
        let plaza = [
            TileCoord { x: 24, y: 13 },
            TileCoord { x: 21, y: 13 },
            TileCoord { x: 27, y: 13 },
            TileCoord { x: 24, y: 15 },
        ];
        let index = (self.total_ticks as usize + actor_id as usize) % plaza.len();
        vec![ResolvedTargetCandidate {
            destination: plaza[index],
            label: "praca central".to_string(),
        }]
    }

    pub(super) fn social_candidates(
        &mut self,
        actor_id: u64,
        target_agent: Option<u64>,
    ) -> Vec<ResolvedTargetCandidate> {
        let Some(target_agent) = target_agent else {
            return self.wander_candidates(actor_id);
        };
        let Ok(target_pos) = self.debug_agent_position(target_agent) else {
            return self.wander_candidates(actor_id);
        };
        let mut candidates = Vec::new();
        for neighbor in target_pos.neighbors4() {
            if self.is_walkable(neighbor) {
                candidates.push(ResolvedTargetCandidate {
                    destination: neighbor,
                    label: format!("aproximar-se de agente {}", target_agent),
                });
            }
        }
        candidates
    }

    pub(super) fn economic_task_candidates(
        &mut self,
        agent_id: u64,
    ) -> Vec<ResolvedTargetCandidate> {
        let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() else {
            return Vec::new();
        };
        let node = match task.phase {
            EconomicTaskPhase::AwaitingPickup => &task.source,
            EconomicTaskPhase::InTransit | EconomicTaskPhase::AwaitingPayment => &task.destination,
            EconomicTaskPhase::Completed | EconomicTaskPhase::Failed => return Vec::new(),
        };
        self.node_access_tile(node)
            .map(|destination| {
                vec![ResolvedTargetCandidate {
                    destination,
                    label: task.description,
                }]
            })
            .unwrap_or_default()
    }

    pub(super) fn agent_position(&mut self, agent_id: u64) -> Result<TileCoord> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0)
    }

    pub(super) fn agent_distance_from_immutable(
        &mut self,
        origin: TileCoord,
        other_id: u64,
    ) -> Option<i32> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        query.iter(&self.world).find_map(|(core, position)| {
            (core.id == other_id).then_some(origin.manhattan(position.0))
        })
    }

    pub(super) fn force_agent_position(&mut self, agent_id: u64, coord: TileCoord) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0 = coord;
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        Ok(())
    }

    pub(super) fn occupancy_map(&mut self) -> HashMap<TileCoord, u64> {
        let mut query = self
            .world
            .query::<(&AgentCore, &PositionComponent, &LifeStatusComponent)>();
        query
            .iter(&self.world)
            .filter(|(_, _, life)| life.0 == AgentLifeStatus::Vivo)
            .map(|(core, position, _)| (position.0, core.id))
            .collect()
    }

    pub(super) fn agent_distance_from(&mut self, origin: TileCoord, other_id: u64) -> Option<i32> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        query.iter(&self.world).find_map(|(core, position)| {
            (core.id == other_id).then_some(origin.manhattan(position.0))
        })
    }

    pub(super) fn tile_at(&self, coord: TileCoord) -> Option<&TileSpec> {
        if coord.x < 0
            || coord.y < 0
            || coord.x >= self.spatial.grid.width
            || coord.y >= self.spatial.grid.height
        {
            return None;
        }
        let index = (coord.y * self.spatial.grid.width + coord.x) as usize;
        self.spatial.grid.tiles.get(index)
    }

    pub(super) fn is_walkable(&self, coord: TileCoord) -> bool {
        let Some(tile) = self.tile_at(coord) else {
            return false;
        };
        if !tile.kind.walkable() {
            return false;
        }
        !self
            .spatial
            .fixtures
            .iter()
            .any(|fixture| fixture.coord == coord && fixture.blocks_movement)
    }

    pub(super) fn fixture_at(&self, coord: TileCoord) -> Option<&FixtureSpec> {
        self.spatial
            .fixtures
            .iter()
            .find(|fixture| fixture.coord == coord)
    }

    pub(super) fn building_name(&self, building_id: BuildingId) -> Option<String> {
        self.spatial
            .buildings
            .iter()
            .find(|building| building.id == building_id)
            .map(|building| building.name.clone())
    }

    pub(super) fn building_kind(&self, building_id: BuildingId) -> Option<LocationKind> {
        self.spatial
            .buildings
            .iter()
            .find(|building| building.id == building_id)
            .map(|building| building.kind)
    }

    pub(super) fn building_kind_opt(
        &self,
        building_id: Option<BuildingId>,
    ) -> Option<LocationKind> {
        building_id.and_then(|id| self.building_kind(id))
    }

    pub(super) fn room_name(&self, room_id: RoomId) -> Option<String> {
        self.spatial
            .rooms
            .iter()
            .find(|room| room.id == room_id)
            .map(|room| room.name.clone())
    }

    pub(super) fn area_name(&self, coord: TileCoord) -> String {
        if let Some(tile) = self.tile_at(coord) {
            if let Some(building_id) = tile.building_id {
                return self
                    .building_name(building_id)
                    .unwrap_or_else(|| "Interior".to_string());
            }
            return match tile.kind {
                TileKind::Field => "Campos do Leste".to_string(),
                TileKind::Forest => "Bosque de Coleta".to_string(),
                TileKind::Rock => "Pedreira do Norte".to_string(),
                TileKind::Road => {
                    if (20..=28).contains(&coord.x) && (11..=15).contains(&coord.y) {
                        "Praca Central".to_string()
                    } else {
                        "Estrada da Vila".to_string()
                    }
                }
                _ => "Exterior da Vila".to_string(),
            };
        }
        "Fora do Mundo".to_string()
    }

    pub(super) fn accessible_exits(&self, coord: TileCoord) -> Vec<String> {
        let mut exits = Vec::new();
        if let Some(tile) = self.tile_at(coord) {
            if let Some(building_id) = tile.building_id {
                if let Some(building) = self
                    .spatial
                    .buildings
                    .iter()
                    .find(|building| building.id == building_id)
                {
                    exits.push(format!(
                        "porta para exterior em ({}, {})",
                        building.entrance.x, building.entrance.y
                    ));
                }
            } else {
                for building in self.spatial.buildings.iter() {
                    if coord.manhattan(building.entrance) <= 8 {
                        exits.push(format!("entrada de {}", building.name));
                    }
                }
            }
        }
        exits
    }

    pub(super) fn local_blockers(&self, coord: TileCoord) -> Vec<String> {
        let mut blockers = Vec::new();
        for neighbor in coord.neighbors4() {
            if let Some(tile) = self.tile_at(neighbor) {
                if tile.kind == TileKind::Wall {
                    blockers.push("parede".to_string());
                }
            }
        }
        blockers
    }

    pub(super) fn nearby_fixture_inputs(
        &self,
        coord: TileCoord,
        radius: i32,
    ) -> Vec<NearbyFixtureInput> {
        let mut fixtures = self
            .spatial
            .fixtures
            .iter()
            .filter_map(|fixture| {
                let distance = coord.manhattan(fixture.coord);
                (distance <= radius).then(|| NearbyFixtureInput {
                    id: fixture.id,
                    name: fixture.name.clone(),
                    kind: fixture.kind,
                    distance,
                    building_name: fixture.building_id.and_then(|id| self.building_name(id)),
                    room_name: fixture.room_id.and_then(|id| self.room_name(id)),
                })
            })
            .collect::<Vec<_>>();
        fixtures.sort_by_key(|fixture| fixture.distance);
        fixtures
    }

    pub(super) fn nearby_agent_inputs(
        &mut self,
        agent_id: u64,
        coord: TileCoord,
        current_room_id: Option<RoomId>,
        relations: &HashMap<u64, AgentRelation>,
    ) -> Vec<NearbyAgentInput> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        let mut agents = query
            .iter(&self.world)
            .filter(|(core, _)| core.id != agent_id)
            .filter_map(|(core, position)| {
                let distance = coord.manhattan(position.0);
                (distance <= 6).then(|| NearbyAgentInput {
                    id: core.id,
                    name: core.name.clone(),
                    role: self.role_display_name(&core.role_id),
                    distance,
                    same_room: self.tile_at(position.0).and_then(|tile| tile.room_id)
                        == current_room_id,
                    relation: relations.get(&core.id).cloned(),
                })
            })
            .collect::<Vec<_>>();
        agents.sort_by_key(|agent| agent.distance);
        agents
    }

    pub(super) fn recent_events_for(
        &self,
        agent_id: u64,
        coord: TileCoord,
        limit: usize,
    ) -> Vec<WorldEvent> {
        self.events
            .iter()
            .rev()
            .filter(|event| {
                event.actor == agent_id
                    || event.target == Some(agent_id)
                    || event
                        .impact_tags
                        .iter()
                        .any(|tag| tag == &self.area_name(coord))
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub(super) fn tile_tags(&self, coord: TileCoord) -> Vec<String> {
        let mut tags = vec![self.area_name(coord)];
        if let Some(tile) = self.tile_at(coord) {
            if let Some(building_id) = tile.building_id {
                tags.push(format!("building:{building_id}"));
            }
            if let Some(room_id) = tile.room_id {
                tags.push(format!("room:{room_id}"));
            }
        }
        tags
    }

    pub(super) fn nearest_storage_for_building(
        &self,
        building_id: Option<BuildingId>,
    ) -> Option<FixtureId> {
        self.spatial
            .fixtures
            .iter()
            .find(|fixture| {
                fixture.building_id == building_id && fixture.kind == FixtureKind::Storage
            })
            .map(|fixture| fixture.id)
    }

    pub(super) fn fixture_access_tile(&self, fixture: &FixtureSpec) -> Option<TileCoord> {
        self.access_tile_for_coord(fixture.coord)
    }

    pub(super) fn access_tile_for_coord(&self, coord: TileCoord) -> Option<TileCoord> {
        coord
            .neighbors4()
            .into_iter()
            .find(|neighbor| self.is_walkable(*neighbor))
    }

    pub(super) fn find_path(
        &mut self,
        start: TileCoord,
        goal: TileCoord,
        _ignore_agent_id: Option<u64>,
    ) -> Option<Vec<TileCoord>> {
        if start == goal {
            return Some(Vec::new());
        }
        let mut frontier = VecDeque::new();
        let mut came_from: HashMap<TileCoord, TileCoord> = HashMap::new();
        let mut visited: HashSet<TileCoord> = HashSet::new();
        frontier.push_back(start);
        visited.insert(start);

        while let Some(current) = frontier.pop_front() {
            for neighbor in current.neighbors4() {
                if !visited.contains(&neighbor) && self.is_walkable(neighbor) {
                    visited.insert(neighbor);
                    came_from.insert(neighbor, current);
                    if neighbor == goal {
                        return Some(reconstruct_path(start, goal, &came_from));
                    }
                    frontier.push_back(neighbor);
                }
            }
        }
        None
    }

    pub(super) fn agents_adjacent(&mut self, actor_id: u64, target_id: u64) -> Result<bool> {
        let actor = self.debug_agent_position(actor_id)?;
        let target = self.debug_agent_position(target_id)?;
        Ok(actor.manhattan(target) <= 1)
    }

    pub(super) fn building_by_id(&self, building_id: BuildingId) -> Option<&BuildingSpec> {
        self.spatial
            .buildings
            .iter()
            .find(|building| building.id == building_id)
    }

    pub(super) fn village_index_of_coord(&self, coord: TileCoord) -> usize {
        let centers = [
            TileCoord { x: 75, y: 22 },
            TileCoord { x: 35, y: 72 },
            TileCoord { x: 115, y: 72 },
        ];
        let mut best_index = 0;
        let mut min_dist = i32::MAX;
        for (i, center) in centers.iter().enumerate() {
            let dist = (coord.x - center.x).abs() + (coord.y - center.y).abs();
            if dist < min_dist {
                min_dist = dist;
                best_index = i;
            }
        }
        best_index
    }

    pub(super) fn village_index_of_establishment(&self, id: EstablishmentId) -> Option<usize> {
        let establishment = self.establishment_by_id(id)?;
        let building_id = establishment.building_id?;
        let building = self.building_by_id(building_id)?;
        Some(self.village_index_of_coord(building.entrance))
    }

    pub(super) fn village_index_of_household(&self, id: BuildingId) -> Option<usize> {
        let building = self.building_by_id(id)?;
        Some(self.village_index_of_coord(building.entrance))
    }

    pub(super) fn fixture_by_id(&self, fixture_id: FixtureId) -> Option<&FixtureSpec> {
        self.spatial
            .fixtures
            .iter()
            .find(|fixture| fixture.id == fixture_id)
    }

    pub(super) fn local_prices_for_agent(&self, position: TileCoord) -> Vec<PostedPrice> {
        let mut prices = self
            .establishments
            .iter()
            .filter(|establishment| {
                establishment
                    .building_id
                    .and_then(|building_id| self.building_by_id(building_id))
                    .map(|building| building.entrance.manhattan(position) <= 20)
                    .unwrap_or(false)
            })
            .flat_map(|establishment| establishment.posted_prices.clone())
            .collect::<Vec<_>>();
        prices.sort_by_key(|price| (price.resource_id.clone(), price.unit_price));
        prices.truncate(8);
        prices
    }
}
